"""
sustena/operators/procurement.py

Procurement domain operators — supply signal broadcasting, PO lifecycle, and
delivery confirmation for the mkulima (farmer) sustain type.

State schema these operators expect under state.procurement:
{
  "procurement": {
    "inventory": {
      "<produce_name>": {          -- e.g. "maize", "beans", "tomatoes"
        "quantity_kg": 1000.0,
        "unit_price": 50.0
      }
    },
    "mkulima_supply_signals": [
      {
        "id": "uuid",
        "produce": "maize",
        "quantity_kg": 100.0,
        "price_per_kg": 50.0,
        "farm_id": "farm-001",
        "harvest_window_days": 7,
        "council_status": "IN_VOTING",  -- IN_VOTING | PASSED | FAILED
        "created_at": "ISO"
      }
    ],
    "purchase_orders": [
      {
        "id": "uuid",
        "signal_id": "<signal_id>",
        "quantity_kg": 100.0,
        "total_kes": 5000.0,
        "status": "RAISED",         -- RAISED | DELIVERED | PARTIAL
        "actual_quantity_kg": null,
        "raised_at": "ISO",
        "delivered_at": null
      }
    ]
  }
}

Mycelium bridge (Phase 1):
  broadcast_supply_signal publishes to the Mycelium network by writing an event
  to the shared SQLite events table with destination='mycelium.public' in the
  payload. The EventBus already persists every event to SQLite — no extra table
  is needed for Phase 1. Phase 2 will introduce a dedicated Mycelium relay
  worker that tails this table and forwards events to the P2P network.

Operators:
  mkulima.broadcast_supply_signal  — validates produce in inventory, publishes
                                     to Mycelium via EventBus
  mkulima.receive_signal           — records an inbound signal, triggers Biashara
                                     operative evaluation hook
  procurement.raise_po             — raises a PO (requires Council PASSED signal)
  procurement.confirm_delivery     — confirms delivery, records actual vs expected

All four primitives are wired in every operator:
  1. @sustena_operator   — registers in OPERATOR_REGISTRY with metadata + constraints
  2. ConstraintEngine    — evaluates pre-conditions at the top of every operator body
  3. StateAccessor       — all reads and mutations go through ctx.state
  4. EventBus            — ctx.events.publish() fires domain events after mutations
  5. PawaLedger          — ctx.pawa.deduct() charges pawa after constraints pass
"""

import uuid
from datetime import datetime

from sustena.core.constraints import ConstraintEngine
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult, sustena_operator

# Shared engine instance — stateless, safe to reuse across calls
_engine = ConstraintEngine()


def _check_constraints(operator_name: str, ctx: OperatorContext, params: dict) -> "OperatorResult | None":
    """
    Run the operator's declared pre-condition constraints via ConstraintEngine.
    Returns OperatorResult.fail() on the first violated constraint, else None.

    Identical pattern to budget.py and chama.py — called at the top of every operator body.
    """
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None or not meta.constraints:
        return None
    ok, reason = _engine.evaluate_all(meta.constraints, ctx.state, params)
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated=reason)
    return None


async def _charge_pawa(operator_name: str, ctx: OperatorContext) -> "OperatorResult | None":
    """
    Deduct pawa for this operator call via PawaLedger.
    Returns OperatorResult.fail() if the user has insufficient balance, else None.
    """
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None:
        return None
    charged = await ctx.pawa.deduct(
        user_id=ctx.user_id,
        sustain_id=ctx.sustain_id,
        amount=meta.pawa_cost,
        reason=f"op:{operator_name}",
    )
    if not charged:
        return OperatorResult.fail(
            reason=(
                f"Insufficient pawa: '{operator_name}' costs {meta.pawa_cost} pawa "
                f"but your balance is too low."
            ),
            constraint_violated="pawa_balance_sufficient",
        )
    return None


def _find_signal(ctx: OperatorContext, signal_id: str) -> dict | None:
    """
    Look up a supply signal record in procurement.mkulima_supply_signals by id.
    Returns the signal dict (a live reference into state) or None.
    """
    signals: list = ctx.state.get("procurement.mkulima_supply_signals", [])
    return next((s for s in signals if s.get("id") == signal_id), None)


def _find_po(ctx: OperatorContext, po_id: str) -> dict | None:
    """
    Look up a purchase order in procurement.purchase_orders by id.
    Returns the PO dict (a live reference into state) or None.
    """
    pos: list = ctx.state.get("procurement.purchase_orders", [])
    return next((p for p in pos if p.get("id") == po_id), None)


# ── mkulima.broadcast_supply_signal ───────────────────────────────────────────

@sustena_operator(
    name="mkulima.broadcast_supply_signal",
    description=(
        "Broadcast a supply signal to the Mycelium market network. "
        "Validates the produce exists in the farm's inventory before publishing."
    ),
    constraints=[
        "params.quantity_kg > 0",
        "params.price_per_kg > 0",
        "params.harvest_window_days > 0",
    ],
    side_effects=["event.procurement.supply_signal_broadcast"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "signal_card",
        "fields": [
            {"label": "Produce", "source": "inputs.produce", "display": "text"},
            {"label": "Quantity (kg)", "source": "inputs.quantity_kg", "display": "number"},
            {"label": "Price/kg", "source": "inputs.price_per_kg", "display": "currency"},
            {"label": "Harvest Window", "source": "inputs.harvest_window_days", "display": "text"},
        ],
        "ctas": ["View Signals", "Broadcast Another"],
    },
)
async def mkulima_broadcast_supply_signal(
    ctx: OperatorContext,
    produce: str,
    quantity_kg: float,
    price_per_kg: float,
    harvest_window_days: int,
) -> OperatorResult:
    """
    Validate produce exists in inventory, then publish a supply signal to Mycelium.

    Phase 1 Mycelium bridge: the event is written to the shared SQLite events table
    (via EventBus) with destination='mycelium.public' in the payload. A future relay
    worker will tail this table and forward events to the P2P network.

    The produce key must exist under procurement.inventory — this is a runtime
    constraint (depends on dynamic state) checked in the operator body.

    Primitives used:
      ConstraintEngine  — enforces quantity_kg > 0, price_per_kg > 0, harvest_window_days > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — reads inventory to validate produce; no mutation
      EventBus          — fires event.procurement.supply_signal_broadcast with
                          destination='mycelium.public' (Phase 1 Mycelium bridge)

    params:
      produce              -- produce name, must exist in procurement.inventory
      quantity_kg          -- quantity available to sell
      price_per_kg         -- asking price in KES
      harvest_window_days  -- days until produce is ready / available
    """
    params = {
        "produce": produce,
        "quantity_kg": quantity_kg,
        "price_per_kg": price_per_kg,
        "harvest_window_days": harvest_window_days,
    }

    # 1. ConstraintEngine — static pre-conditions
    fail = _check_constraints("mkulima.broadcast_supply_signal", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("mkulima.broadcast_supply_signal", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime constraint: produce must exist in inventory
    inventory: dict = ctx.state.get("procurement.inventory", {})
    if produce not in inventory:
        return OperatorResult.fail(
            reason=(
                f"Produce '{produce}' is not in inventory. "
                f"Add it to procurement.inventory before broadcasting."
            ),
            constraint_violated="produce_in_inventory",
        )

    signal_id = str(uuid.uuid4())
    total_value = round(quantity_kg * price_per_kg, 2)

    # 4. EventBus — publish to Mycelium (Phase 1: SQLite events table with destination tag)
    await ctx.events.publish(
        "event.procurement.supply_signal_broadcast",
        {
            "signal_id": signal_id,
            "produce": produce,
            "quantity_kg": quantity_kg,
            "price_per_kg": price_per_kg,
            "harvest_window_days": harvest_window_days,
            "total_value_kes": total_value,
            "farm_sustain_id": ctx.sustain_id,
            # Mycelium Phase 1 bridge marker — relay worker filters on this field
            "destination": "mycelium.public",
        },
    )

    return OperatorResult.ok({
        "signal_id": signal_id,
        "produce": produce,
        "quantity_kg": quantity_kg,
        "price_per_kg": price_per_kg,
        "harvest_window_days": harvest_window_days,
        "total_value_kes": total_value,
        "destination": "mycelium.public",
        "status": "broadcast",
    })


# ── mkulima.receive_signal ─────────────────────────────────────────────────────

@sustena_operator(
    name="mkulima.receive_signal",
    description=(
        "Record an inbound supply signal from Mycelium and trigger Biashara operative "
        "evaluation. The Biashara operative will assess the signal and propose a PO if viable."
    ),
    constraints=[
        "params.quantity_kg > 0",
        "params.price_per_kg > 0",
        "params.harvest_window_days > 0",
    ],
    side_effects=[
        "event.procurement.signal_received",
        "event.biashara.signal_evaluation_requested",
    ],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "signal_card",
        "fields": [
            {"label": "Produce", "source": "inputs.produce", "display": "text"},
            {"label": "Farm", "source": "inputs.farm_id", "display": "text"},
            {"label": "Quantity (kg)", "source": "inputs.quantity_kg", "display": "number"},
            {"label": "Price/kg", "source": "inputs.price_per_kg", "display": "currency"},
        ],
        "ctas": ["View Signals", "Ask Biashara"],
    },
)
async def mkulima_receive_signal(
    ctx: OperatorContext,
    signal_id: str,
    produce: str,
    quantity_kg: float,
    price_per_kg: float,
    farm_id: str,
    harvest_window_days: int,
) -> OperatorResult:
    """
    Record a supply signal received from the Mycelium network and trigger Biashara
    operative evaluation.

    Appends to procurement.mkulima_supply_signals with council_status='IN_VOTING'
    (the signal is available but not yet PO-approved). Fires a Biashara evaluation
    event — the actual Biashara operative is a future hook (Phase 2 operative).

    Primitives used:
      ConstraintEngine  — enforces quantity_kg > 0, price_per_kg > 0, harvest_window_days > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — appends signal to procurement.mkulima_supply_signals
      EventBus          — fires event.procurement.signal_received +
                          event.biashara.signal_evaluation_requested (Biashara hook)

    params:
      signal_id           -- ID from the originating broadcast (or new UUID if none)
      produce             -- commodity name
      quantity_kg         -- quantity offered
      price_per_kg        -- asking price in KES
      farm_id             -- originating farm identifier
      harvest_window_days -- days until available
    """
    params = {
        "signal_id": signal_id,
        "produce": produce,
        "quantity_kg": quantity_kg,
        "price_per_kg": price_per_kg,
        "farm_id": farm_id,
        "harvest_window_days": harvest_window_days,
    }

    # 1. ConstraintEngine
    fail = _check_constraints("mkulima.receive_signal", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("mkulima.receive_signal", ctx)
    if fail:
        return fail

    # 3. StateAccessor — append signal record
    if not ctx.state.exists("procurement.mkulima_supply_signals"):
        ctx.state.set("procurement.mkulima_supply_signals", [])

    signal_record = {
        "id": signal_id,
        "produce": produce,
        "quantity_kg": quantity_kg,
        "price_per_kg": price_per_kg,
        "farm_id": farm_id,
        "harvest_window_days": harvest_window_days,
        "council_status": "IN_VOTING",   # Council must PASS before PO can be raised
        "created_at": ctx.timestamp.isoformat(),
    }
    # Use append — it will not overwrite the id since we've already set it
    ctx.state.append("procurement.mkulima_supply_signals", signal_record)

    total_value = round(quantity_kg * price_per_kg, 2)

    # 4. EventBus — domain event + Biashara operative hook
    await ctx.events.publish(
        "event.procurement.signal_received",
        {
            "signal_id": signal_id,
            "produce": produce,
            "quantity_kg": quantity_kg,
            "price_per_kg": price_per_kg,
            "farm_id": farm_id,
            "harvest_window_days": harvest_window_days,
            "total_value_kes": total_value,
        },
    )

    # Biashara evaluation hook — Phase 2 operative will subscribe to this event
    # and autonomously assess viability, check budget pockets, and propose a PO
    await ctx.events.publish(
        "event.biashara.signal_evaluation_requested",
        {
            "signal_id": signal_id,
            "produce": produce,
            "quantity_kg": quantity_kg,
            "price_per_kg": price_per_kg,
            "farm_id": farm_id,
            "total_value_kes": total_value,
            "sustain_id": ctx.sustain_id,
        },
    )

    return OperatorResult.ok({
        "signal_id": signal_id,
        "produce": produce,
        "quantity_kg": quantity_kg,
        "price_per_kg": price_per_kg,
        "farm_id": farm_id,
        "harvest_window_days": harvest_window_days,
        "total_value_kes": total_value,
        "council_status": "IN_VOTING",
        "biashara_evaluation": "requested",
    })


# ── procurement.raise_po ───────────────────────────────────────────────────────

@sustena_operator(
    name="procurement.raise_po",
    description=(
        "Raise a Purchase Order against a Council-approved supply signal. "
        "The signal must have council_status == 'PASSED'."
    ),
    constraints=[
        "params.quantity_kg > 0",
        "params.total_kes > 0",
    ],
    side_effects=["event.procurement.po_raised"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "po_card",
        "fields": [
            {"label": "Signal ID", "source": "inputs.signal_id", "display": "text"},
            {"label": "Quantity (kg)", "source": "inputs.quantity_kg", "display": "number"},
            {"label": "Total (KES)", "source": "inputs.total_kes", "display": "currency"},
            {"label": "Status", "source": "outputs.status", "display": "badge"},
        ],
        "ctas": ["View PO", "Confirm Delivery"],
    },
)
async def procurement_raise_po(
    ctx: OperatorContext,
    signal_id: str,
    quantity_kg: float,
    total_kes: float,
) -> OperatorResult:
    """
    Raise a PO against a supply signal that has passed Council approval.

    Runtime constraint (same pattern as chama.loan.disburse): the signal must exist
    in state AND have council_status == "PASSED". If the Council hasn't approved it
    yet (still IN_VOTING or FAILED), this operator returns a failure.

    Primitives used:
      ConstraintEngine  — enforces quantity_kg > 0 and total_kes > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — creates PO record in procurement.purchase_orders
      EventBus          — fires event.procurement.po_raised

    params:
      signal_id   -- the supply signal to raise PO against (must be PASSED)
      quantity_kg -- quantity to purchase
      total_kes   -- total purchase value in KES
    """
    params = {"signal_id": signal_id, "quantity_kg": quantity_kg, "total_kes": total_kes}

    # 1. ConstraintEngine — static pre-conditions
    fail = _check_constraints("procurement.raise_po", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("procurement.raise_po", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime constraint: signal must exist and be PASSED
    signal = _find_signal(ctx, signal_id)

    if signal is None:
        return OperatorResult.fail(
            reason=f"Signal '{signal_id}' does not exist in procurement.mkulima_supply_signals.",
            constraint_violated="signal_exists",
        )

    council_status = signal.get("council_status", "IN_VOTING")
    if council_status != "PASSED":
        return OperatorResult.fail(
            reason=(
                f"Signal '{signal_id}' cannot be PO'd — council_status is "
                f"'{council_status}', must be 'PASSED'."
            ),
            constraint_violated="signal_council_status_is_passed",
        )

    # Create the PO record
    po_id = str(uuid.uuid4())

    if not ctx.state.exists("procurement.purchase_orders"):
        ctx.state.set("procurement.purchase_orders", [])

    po_record = {
        "id": po_id,
        "signal_id": signal_id,
        "produce": signal.get("produce"),
        "farm_id": signal.get("farm_id"),
        "quantity_kg": quantity_kg,
        "total_kes": total_kes,
        "status": "RAISED",
        "actual_quantity_kg": None,
        "raised_at": ctx.timestamp.isoformat(),
        "delivered_at": None,
    }
    ctx.state.append("procurement.purchase_orders", po_record)

    # 4. EventBus
    await ctx.events.publish(
        "event.procurement.po_raised",
        {
            "po_id": po_id,
            "signal_id": signal_id,
            "produce": signal.get("produce"),
            "farm_id": signal.get("farm_id"),
            "quantity_kg": quantity_kg,
            "total_kes": total_kes,
        },
    )

    return OperatorResult.ok({
        "po_id": po_id,
        "signal_id": signal_id,
        "produce": signal.get("produce"),
        "farm_id": signal.get("farm_id"),
        "quantity_kg": quantity_kg,
        "total_kes": total_kes,
        "status": "RAISED",
    })


# ── procurement.confirm_delivery ───────────────────────────────────────────────

@sustena_operator(
    name="procurement.confirm_delivery",
    description="Confirm delivery against a RAISED Purchase Order, recording actual vs expected quantity.",
    constraints=[
        "params.actual_quantity_kg > 0",
    ],
    side_effects=["event.procurement.delivery_confirmed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "delivery_confirmation_card",
        "fields": [
            {"label": "PO ID", "source": "inputs.po_id", "display": "text"},
            {"label": "Actual Quantity (kg)", "source": "inputs.actual_quantity_kg", "display": "number"},
            {"label": "Expected Quantity (kg)", "source": "state.po.quantity_kg", "display": "number"},
            {"label": "Status", "source": "outputs.status", "display": "badge"},
        ],
        "ctas": ["View PO", "View Procurement"],
    },
)
async def procurement_confirm_delivery(
    ctx: OperatorContext,
    po_id: str,
    actual_quantity_kg: float,
) -> OperatorResult:
    """
    Confirm delivery against a Purchase Order and record actual vs expected quantity.

    The PO must exist and be in RAISED status. After confirmation:
      - Status becomes DELIVERED if actual_quantity_kg >= expected, else PARTIAL.
      - actual_quantity_kg and delivered_at are recorded on the PO.

    Primitives used:
      ConstraintEngine  — enforces actual_quantity_kg > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — updates PO status, actual_quantity_kg, delivered_at
      EventBus          — fires event.procurement.delivery_confirmed

    params:
      po_id              -- the purchase order to confirm
      actual_quantity_kg -- quantity actually received
    """
    params = {"po_id": po_id, "actual_quantity_kg": actual_quantity_kg}

    # 1. ConstraintEngine
    fail = _check_constraints("procurement.confirm_delivery", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("procurement.confirm_delivery", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime constraint: PO must exist and be RAISED
    po = _find_po(ctx, po_id)

    if po is None:
        return OperatorResult.fail(
            reason=f"Purchase Order '{po_id}' does not exist.",
            constraint_violated="po_exists",
        )

    po_status = po.get("status", "")
    if po_status != "RAISED":
        return OperatorResult.fail(
            reason=(
                f"PO '{po_id}' cannot be confirmed — status is '{po_status}', "
                f"must be 'RAISED'."
            ),
            constraint_violated="po_status_is_raised",
        )

    expected_qty = po.get("quantity_kg", 0.0)
    is_full_delivery = actual_quantity_kg >= expected_qty
    new_status = "DELIVERED" if is_full_delivery else "PARTIAL"
    variance_kg = round(actual_quantity_kg - expected_qty, 4)

    # Mutate PO in-place (live reference into state list)
    po["actual_quantity_kg"] = actual_quantity_kg
    po["status"] = new_status
    po["delivered_at"] = ctx.timestamp.isoformat()
    po["variance_kg"] = variance_kg

    # 4. EventBus
    await ctx.events.publish(
        "event.procurement.delivery_confirmed",
        {
            "po_id": po_id,
            "signal_id": po.get("signal_id"),
            "produce": po.get("produce"),
            "farm_id": po.get("farm_id"),
            "expected_quantity_kg": expected_qty,
            "actual_quantity_kg": actual_quantity_kg,
            "variance_kg": variance_kg,
            "status": new_status,
        },
    )

    return OperatorResult.ok({
        "po_id": po_id,
        "signal_id": po.get("signal_id"),
        "produce": po.get("produce"),
        "expected_quantity_kg": expected_qty,
        "actual_quantity_kg": actual_quantity_kg,
        "variance_kg": variance_kg,
        "status": new_status,
        "fully_delivered": is_full_delivery,
    })
