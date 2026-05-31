"""
sustena/operators/biashara.py

General Business (Biashara) domain operators — the financial and operational
primitives for any business sustain on Sustena XII.

State schema these operators expect:
  state.inventory.items          -- dict keyed by item_id
  state.inventory.movements      -- append-only movement ledger
  state.accounts.journal_entries -- journal entry headers with nested lines
  state.accounts.journal_lines   -- flat denormalized journal lines
  state.accounts.chart           -- chart of accounts keyed by account_id
  state.parties                  -- counterparty registry keyed by party_id
  state.orders.active            -- active orders list
  state.orders.history           -- completed/cancelled orders list
  state.purchases.history        -- purchase history
  state.purchase_orders.active   -- open POs
  state.purchase_orders.history  -- closed POs
  state.assets                   -- fixed asset register keyed by asset_id
  state.expenses                 -- expense records list

All four primitives are wired together in every operator:
  1. @sustena_operator   -- registers in OPERATOR_REGISTRY
  2. ConstraintEngine    -- self-checks pre-conditions
  3. StateAccessor       -- all state reads and mutations
  4. EventBus            -- fires domain events after mutations
  5. PawaLedger          -- deducts pawa after constraints pass
"""

import uuid
from datetime import datetime

from sustena.core.constraints import ConstraintEngine
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult, sustena_operator

_engine = ConstraintEngine()


# ── Shared helpers ─────────────────────────────────────────────────────────────

def _check_constraints(operator_name: str, ctx: OperatorContext, params: dict) -> "OperatorResult | None":
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None or not meta.constraints:
        return None
    ok, reason = _engine.evaluate_all(meta.constraints, ctx.state, params)
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated=reason)
    return None


async def _charge_pawa(operator_name: str, ctx: OperatorContext) -> "OperatorResult | None":
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


def _post_journal_entry(
    ctx: OperatorContext,
    description: str,
    lines: list[dict],
    source_type: str = "auto",
) -> str:
    """
    Post a balanced journal entry to accounts.journal_entries and
    the denormalised accounts.journal_lines.

    Each line dict must have: account_id, debit (float), credit (float).
    Returns the entry_id.
    """
    entry_id = str(uuid.uuid4())
    entry_lines = []
    for line in lines:
        line_id = str(uuid.uuid4())
        entry_lines.append({
            "id": line_id,
            "line_id": line_id,
            "account_id": line["account_id"],
            "debit": float(line.get("debit", 0.0)),
            "credit": float(line.get("credit", 0.0)),
        })
        ctx.state.append("accounts.journal_lines", {
            "id": line_id,
            "line_id": line_id,
            "entry_id": entry_id,
            "account_id": line["account_id"],
            "debit": float(line.get("debit", 0.0)),
            "credit": float(line.get("credit", 0.0)),
        })
    ctx.state.append("accounts.journal_entries", {
        "id": entry_id,
        "entry_id": entry_id,
        "date": ctx.timestamp.isoformat(),
        "description": description,
        "source_type": source_type,
        "lines": entry_lines,
    })
    return entry_id


def _find_in_list(lst: list, id_field: str, id_value: str) -> "dict | None":
    """Find the first item in a list where item[id_field] == id_value."""
    return next((item for item in lst if item.get(id_field) == id_value), None)


# ── biashara.inventory.restock ─────────────────────────────────────────────────

@sustena_operator(
    name="biashara.inventory.restock",
    description="Record a stock restock, increasing item quantity and posting Dr Inventory / Cr AP.",
    constraints=[
        "params.quantity > 0",
        "params.unit_cost > 0",
    ],
    side_effects=["event.biashara.inventory.restocked"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "inventory_card",
        "fields": [
            {"label": "Item", "source": "inputs.item_id", "display": "text"},
            {"label": "Quantity", "source": "inputs.quantity", "display": "number"},
            {"label": "Unit Cost", "source": "inputs.unit_cost", "display": "currency"},
        ],
        "ctas": ["View Inventory"],
    },
)
async def biashara_inventory_restock(
    ctx: OperatorContext,
    item_id: str,
    quantity: float,
    unit_cost: float,
    supplier_party_id: str = "",
) -> OperatorResult:
    """
    Increase stock for an existing inventory item and post a journal entry.

    Primitives:
      ConstraintEngine -- quantity > 0, unit_cost > 0
      PawaLedger       -- 0 pawa (free)
      StateAccessor    -- appends movement, increments item qty
      EventBus         -- event.biashara.inventory.restocked
    """
    params = {"quantity": quantity, "unit_cost": unit_cost}

    # 1. ConstraintEngine
    fail = _check_constraints("biashara.inventory.restock", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("biashara.inventory.restock", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime: item must exist
    items = ctx.state.get("inventory.items", {})
    if item_id not in items:
        return OperatorResult.fail(
            reason=f"Item '{item_id}' not found in inventory.items.",
            constraint_violated="item_exists",
        )

    current_qty = items[item_id].get("qty", 0.0)
    new_qty = current_qty + quantity
    # Update via set — works for \w+ item IDs (numeric strings, alphanumeric)
    items[item_id]["qty"] = new_qty

    total_cost = quantity * unit_cost
    movement = {
        "id": str(uuid.uuid4()),
        "item_id": item_id,
        "delta": quantity,
        "reason": "restock",
        "source_type": "purchase",
        "unit_cost": unit_cost,
        "supplier_party_id": supplier_party_id,
        "recorded_at": ctx.timestamp.isoformat(),
    }
    ctx.state.append("inventory.movements", movement)

    _post_journal_entry(ctx, f"Inventory restock: item {item_id}", [
        {"account_id": "inventory_asset", "debit": total_cost, "credit": 0.0},
        {"account_id": "accounts_payable", "debit": 0.0, "credit": total_cost},
    ], source_type="restock")

    # 4. EventBus
    await ctx.events.publish("event.biashara.inventory.restocked", {
        "item_id": item_id, "quantity": quantity, "unit_cost": unit_cost,
        "new_qty": new_qty, "supplier_party_id": supplier_party_id,
    })

    return OperatorResult.ok({
        "item_id": item_id,
        "quantity_added": quantity,
        "new_qty": new_qty,
        "total_cost": total_cost,
    })


# ── biashara.inventory.adjust ──────────────────────────────────────────────────

@sustena_operator(
    name="biashara.inventory.adjust",
    description="Adjust inventory quantity by a signed delta with a mandatory reason.",
    constraints=[],
    side_effects=["event.biashara.inventory.adjusted"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "inventory_card",
        "fields": [
            {"label": "Item", "source": "inputs.item_id", "display": "text"},
            {"label": "Delta", "source": "inputs.qty_delta", "display": "number"},
            {"label": "Reason", "source": "inputs.reason", "display": "text"},
        ],
        "ctas": ["View Inventory"],
    },
)
async def biashara_inventory_adjust(
    ctx: OperatorContext,
    item_id: str,
    qty_delta: float,
    reason: str,
) -> OperatorResult:
    """
    Apply a signed quantity delta to an inventory item.
    Rejects if reason is empty or item does not exist.
    Enforces qty >= 0 invariant.

    Primitives:
      ConstraintEngine -- none (runtime-only checks)
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends movement, updates qty
      EventBus         -- event.biashara.inventory.adjusted
    """
    # 1. Runtime constraints (not expressible as static strings)
    if not reason or not reason.strip():
        return OperatorResult.fail(
            reason="Reason must not be empty for inventory adjustments.",
            constraint_violated="reason_not_empty",
        )

    # 2. PawaLedger
    fail = await _charge_pawa("biashara.inventory.adjust", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    items = ctx.state.get("inventory.items", {})
    if item_id not in items:
        return OperatorResult.fail(
            reason=f"Item '{item_id}' not found in inventory.items.",
            constraint_violated="item_exists",
        )

    current_qty = items[item_id].get("qty", 0.0)
    new_qty = current_qty + qty_delta

    if new_qty < 0:
        return OperatorResult.fail(
            reason=(
                f"Adjustment of {qty_delta} would make '{item_id}' qty negative "
                f"({current_qty} + {qty_delta} = {new_qty})."
            ),
            constraint_violated="inventory_qty_non_negative",
        )

    items[item_id]["qty"] = new_qty
    ctx.state.append("inventory.movements", {
        "id": str(uuid.uuid4()),
        "item_id": item_id,
        "delta": qty_delta,
        "reason": reason,
        "source_type": "adjustment",
        "recorded_at": ctx.timestamp.isoformat(),
    })

    # 4. EventBus
    await ctx.events.publish("event.biashara.inventory.adjusted", {
        "item_id": item_id, "qty_delta": qty_delta, "reason": reason,
        "new_qty": new_qty,
    })

    return OperatorResult.ok({
        "item_id": item_id, "qty_delta": qty_delta,
        "old_qty": current_qty, "new_qty": new_qty,
    })


# ── biashara.orders.place ──────────────────────────────────────────────────────

@sustena_operator(
    name="biashara.orders.place",
    description="Place a new customer order. Creates an active order record with status=pending.",
    constraints=[],
    side_effects=["event.biashara.order.placed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "order_card",
        "fields": [
            {"label": "Customer", "source": "inputs.customer_party_id", "display": "text"},
            {"label": "Items", "source": "inputs.items", "display": "list"},
        ],
        "ctas": ["View Orders", "Fulfill Order"],
    },
)
async def biashara_orders_place(
    ctx: OperatorContext,
    items: list,
    customer_party_id: str = "",
) -> OperatorResult:
    """
    Place a new customer order.

    items: list of {item_id, quantity, unit_price}

    Primitives:
      ConstraintEngine -- none (runtime item validation)
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to orders.active
      EventBus         -- event.biashara.order.placed
    """
    # 1. Runtime validation
    if not items:
        return OperatorResult.fail(
            reason="Order must contain at least one item.",
            constraint_violated="items_not_empty",
        )

    inventory_items = ctx.state.get("inventory.items", {})
    for line in items:
        iid = line.get("item_id", "")
        qty = line.get("quantity", 0)
        if not iid:
            return OperatorResult.fail(
                reason="Each order line must have an item_id.",
                constraint_violated="item_id_required",
            )
        if iid not in inventory_items:
            return OperatorResult.fail(
                reason=f"Item '{iid}' not found in inventory.items.",
                constraint_violated="item_exists",
            )
        if qty <= 0:
            return OperatorResult.fail(
                reason=f"Quantity for item '{iid}' must be > 0 (got {qty}).",
                constraint_violated="quantity_positive",
            )

    # 2. PawaLedger
    fail = await _charge_pawa("biashara.orders.place", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    order_id = str(uuid.uuid4())
    total_value = sum(
        float(line.get("quantity", 0)) * float(line.get("unit_price", 0))
        for line in items
    )
    order = {
        "id": order_id,
        "order_id": order_id,
        "customer_party_id": customer_party_id,
        "items": items,
        "status": "pending",
        "total_value": total_value,
        "placed_at": ctx.timestamp.isoformat(),
    }
    ctx.state.append("orders.active", order)

    # 4. EventBus
    await ctx.events.publish("event.biashara.order.placed", {
        "order_id": order_id, "customer_party_id": customer_party_id,
        "total_value": total_value, "item_count": len(items),
    })

    return OperatorResult.ok({"order_id": order_id, "status": "pending", "total_value": total_value})


# ── biashara.orders.fulfill ────────────────────────────────────────────────────

@sustena_operator(
    name="biashara.orders.fulfill",
    description="Fulfill a pending order: deduct inventory, post accounting entries, move to history.",
    constraints=[],
    side_effects=["event.biashara.order.fulfilled"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "order_card",
        "fields": [
            {"label": "Order ID", "source": "inputs.order_id", "display": "text"},
            {"label": "Status", "source": "outputs.status", "display": "text"},
        ],
        "ctas": ["View Orders"],
    },
)
async def biashara_orders_fulfill(
    ctx: OperatorContext,
    order_id: str,
) -> OperatorResult:
    """
    Fulfill an active order.

    Primitives:
      ConstraintEngine -- none (runtime lookup)
      PawaLedger       -- 0 pawa
      StateAccessor    -- deducts inventory, moves order to history
      EventBus         -- event.biashara.order.fulfilled
    """
    # 1. PawaLedger
    fail = await _charge_pawa("biashara.orders.fulfill", ctx)
    if fail:
        return fail

    # 2. StateAccessor — find order
    active_orders = ctx.state.get("orders.active", [])
    order = _find_in_list(active_orders, "order_id", order_id)
    if order is None:
        return OperatorResult.fail(
            reason=f"Order '{order_id}' not found in orders.active.",
            constraint_violated="order_exists",
        )
    if order.get("status") != "pending":
        return OperatorResult.fail(
            reason=f"Order '{order_id}' has status '{order.get('status')}' — must be 'pending' to fulfill.",
            constraint_violated="order_pending",
        )

    inventory_items = ctx.state.get("inventory.items", {})
    order_items = order.get("items", [])
    total_revenue = 0.0
    total_cogs = 0.0

    for line in order_items:
        iid = line["item_id"]
        qty = float(line.get("quantity", 0))
        unit_price = float(line.get("unit_price", 0))
        line_revenue = qty * unit_price
        total_revenue += line_revenue

        item_rec = inventory_items.get(iid, {})
        unit_cost = float(item_rec.get("unit_cost", item_rec.get("standard_cost", 0.0)))
        total_cogs += qty * unit_cost

        # Deduct inventory
        current_qty = item_rec.get("qty", 0.0)
        item_rec["qty"] = max(0.0, current_qty - qty)

        ctx.state.append("inventory.movements", {
            "id": str(uuid.uuid4()),
            "item_id": iid,
            "delta": -qty,
            "reason": f"order:{order_id}",
            "source_type": "sale",
            "recorded_at": ctx.timestamp.isoformat(),
        })

    # Journal: Dr AR / Cr Revenue + Dr COGS / Cr Inventory
    _post_journal_entry(ctx, f"Order fulfilled: {order_id}", [
        {"account_id": "accounts_receivable", "debit": total_revenue, "credit": 0.0},
        {"account_id": "revenue", "debit": 0.0, "credit": total_revenue},
        {"account_id": "cogs", "debit": total_cogs, "credit": 0.0},
        {"account_id": "inventory_asset", "debit": 0.0, "credit": total_cogs},
    ], source_type="sale")

    # Move order: update status then remove+append
    order["status"] = "fulfilled"
    order["fulfilled_at"] = ctx.timestamp.isoformat()
    order["total_revenue"] = total_revenue
    active_orders[:] = [o for o in active_orders if o.get("order_id") != order_id]
    ctx.state.append("orders.history", order)

    # 4. EventBus
    await ctx.events.publish("event.biashara.order.fulfilled", {
        "order_id": order_id, "total_revenue": total_revenue, "total_cogs": total_cogs,
    })

    return OperatorResult.ok({
        "order_id": order_id, "status": "fulfilled",
        "total_revenue": total_revenue, "total_cogs": total_cogs,
    })


# ── biashara.purchases.record ──────────────────────────────────────────────────

@sustena_operator(
    name="biashara.purchases.record",
    description="Record a supplier purchase: update inventory and post Dr Inventory / Cr Cash journal entry.",
    constraints=[
        "params.quantity > 0",
        "params.total_cost > 0",
    ],
    side_effects=["event.biashara.purchase.recorded"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Item", "source": "inputs.item_id", "display": "text"},
            {"label": "Quantity", "source": "inputs.quantity", "display": "number"},
            {"label": "Total Cost", "source": "inputs.total_cost", "display": "currency"},
        ],
        "ctas": ["View Purchases"],
    },
)
async def biashara_purchases_record(
    ctx: OperatorContext,
    item_id: str,
    quantity: float,
    total_cost: float,
    vendor_party_id: str = "",
    payment_account_id: str = "cash",
) -> OperatorResult:
    """
    Record a direct purchase (immediate payment vs. credit via restock).

    Primitives:
      ConstraintEngine -- quantity > 0, total_cost > 0
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to purchases.history, inventory.movements, updates qty
      EventBus         -- event.biashara.purchase.recorded
    """
    params = {"quantity": quantity, "total_cost": total_cost}

    # 1. ConstraintEngine
    fail = _check_constraints("biashara.purchases.record", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("biashara.purchases.record", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    items = ctx.state.get("inventory.items", {})
    if item_id not in items:
        return OperatorResult.fail(
            reason=f"Item '{item_id}' not found in inventory.items.",
            constraint_violated="item_exists",
        )

    unit_cost = total_cost / quantity
    current_qty = items[item_id].get("qty", 0.0)
    items[item_id]["qty"] = current_qty + quantity

    purchase_id = str(uuid.uuid4())
    ctx.state.append("purchases.history", {
        "id": purchase_id,
        "purchase_id": purchase_id,
        "item_id": item_id,
        "quantity": quantity,
        "total_cost": total_cost,
        "unit_cost": unit_cost,
        "vendor_party_id": vendor_party_id,
        "payment_account_id": payment_account_id,
        "recorded_at": ctx.timestamp.isoformat(),
    })

    ctx.state.append("inventory.movements", {
        "id": str(uuid.uuid4()),
        "item_id": item_id,
        "delta": quantity,
        "reason": "purchase",
        "source_type": "purchase",
        "unit_cost": unit_cost,
        "recorded_at": ctx.timestamp.isoformat(),
    })

    _post_journal_entry(ctx, f"Purchase recorded: item {item_id}", [
        {"account_id": "inventory_asset", "debit": total_cost, "credit": 0.0},
        {"account_id": payment_account_id, "debit": 0.0, "credit": total_cost},
    ], source_type="purchase")

    # 4. EventBus
    await ctx.events.publish("event.biashara.purchase.recorded", {
        "purchase_id": purchase_id, "item_id": item_id,
        "quantity": quantity, "total_cost": total_cost,
    })

    return OperatorResult.ok({
        "purchase_id": purchase_id, "item_id": item_id,
        "quantity": quantity, "total_cost": total_cost,
        "new_qty": items[item_id]["qty"],
    })


# ── biashara.po.raise ──────────────────────────────────────────────────────────

@sustena_operator(
    name="biashara.po.raise",
    description="Raise a Purchase Order to a vendor. Vendor must exist in state.parties.",
    constraints=[],
    side_effects=["event.biashara.po.raised"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "po_card",
        "fields": [
            {"label": "Vendor", "source": "inputs.vendor_party_id", "display": "text"},
            {"label": "Items", "source": "inputs.items", "display": "list"},
        ],
        "ctas": ["View POs"],
    },
)
async def biashara_po_raise(
    ctx: OperatorContext,
    vendor_party_id: str,
    items: list,
    delivery_date: str = "",
) -> OperatorResult:
    """
    Raise a PO to a vendor.

    items: list of {item_id, qty_ordered, unit_price}

    Primitives:
      ConstraintEngine -- none (runtime)
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to purchase_orders.active
      EventBus         -- event.biashara.po.raised
    """
    # 1. Runtime constraints
    parties = ctx.state.get("parties", {})
    if vendor_party_id not in parties:
        return OperatorResult.fail(
            reason=f"Vendor '{vendor_party_id}' not found in state.parties.",
            constraint_violated="vendor_exists",
        )
    if not items:
        return OperatorResult.fail(
            reason="PO must contain at least one line item.",
            constraint_violated="items_not_empty",
        )

    # 2. PawaLedger
    fail = await _charge_pawa("biashara.po.raise", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    po_id = str(uuid.uuid4())
    total_value = sum(
        float(line.get("qty_ordered", 0)) * float(line.get("unit_price", 0))
        for line in items
    )
    po = {
        "id": po_id,
        "po_id": po_id,
        "vendor_party_id": vendor_party_id,
        "items": items,
        "status": "draft",
        "total_value": total_value,
        "delivery_date": delivery_date,
        "raised_at": ctx.timestamp.isoformat(),
    }
    ctx.state.append("purchase_orders.active", po)

    # 4. EventBus
    await ctx.events.publish("event.biashara.po.raised", {
        "po_id": po_id, "vendor_party_id": vendor_party_id, "total_value": total_value,
    })

    return OperatorResult.ok({"po_id": po_id, "status": "draft", "total_value": total_value})


# ── biashara.po.receive ────────────────────────────────────────────────────────

@sustena_operator(
    name="biashara.po.receive",
    description="Receive goods against an open PO. Updates inventory and moves PO to history.",
    constraints=[],
    side_effects=["event.biashara.po.received"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "po_card",
        "fields": [
            {"label": "PO ID", "source": "inputs.po_id", "display": "text"},
            {"label": "Items Received", "source": "inputs.items_received", "display": "list"},
        ],
        "ctas": ["View POs"],
    },
)
async def biashara_po_receive(
    ctx: OperatorContext,
    po_id: str,
    items_received: list,
) -> OperatorResult:
    """
    Receive goods against an open PO.

    items_received: list of {item_id, qty_received}

    Primitives:
      ConstraintEngine -- none (runtime)
      PawaLedger       -- 0 pawa
      StateAccessor    -- updates inventory, moves PO to history
      EventBus         -- event.biashara.po.received
    """
    # 1. PawaLedger
    fail = await _charge_pawa("biashara.po.receive", ctx)
    if fail:
        return fail

    # 2. StateAccessor — find PO
    active_pos = ctx.state.get("purchase_orders.active", [])
    po = _find_in_list(active_pos, "po_id", po_id)
    if po is None:
        return OperatorResult.fail(
            reason=f"PO '{po_id}' not found in purchase_orders.active.",
            constraint_violated="po_exists",
        )
    if po.get("status") not in ("draft", "sent"):
        return OperatorResult.fail(
            reason=f"PO '{po_id}' has status '{po.get('status')}' — must be 'draft' or 'sent'.",
            constraint_violated="po_open",
        )

    inventory_items = ctx.state.get("inventory.items", {})
    for line in items_received:
        iid = line["item_id"]
        qty = float(line.get("qty_received", 0))
        if iid in inventory_items:
            inventory_items[iid]["qty"] = inventory_items[iid].get("qty", 0.0) + qty
        ctx.state.append("inventory.movements", {
            "id": str(uuid.uuid4()),
            "item_id": iid,
            "delta": qty,
            "reason": f"po_receipt:{po_id}",
            "source_type": "purchase",
            "recorded_at": ctx.timestamp.isoformat(),
        })

    po["status"] = "received"
    po["received_at"] = ctx.timestamp.isoformat()
    po["items_received"] = items_received
    active_pos[:] = [p for p in active_pos if p.get("po_id") != po_id]
    ctx.state.append("purchase_orders.history", po)

    # 4. EventBus
    await ctx.events.publish("event.biashara.po.received", {
        "po_id": po_id, "items_received": items_received,
    })

    return OperatorResult.ok({
        "po_id": po_id, "status": "received",
        "items_received_count": len(items_received),
    })


# ── biashara.accounts.journal_entry ───────────────────────────────────────────

@sustena_operator(
    name="biashara.accounts.journal_entry",
    description="Post a manual double-entry journal. Validates sum(debit) == sum(credit).",
    constraints=[],
    side_effects=["event.biashara.accounts.journal_entry_posted"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "journal_entry_card",
        "fields": [
            {"label": "Description", "source": "inputs.description", "display": "text"},
            {"label": "Lines", "source": "inputs.lines", "display": "list"},
        ],
        "ctas": ["View Journal"],
    },
)
async def biashara_accounts_journal_entry(
    ctx: OperatorContext,
    lines: list,
    source_type: str = "manual",
    description: str = "",
) -> OperatorResult:
    """
    Post a manual balanced journal entry.

    lines: list of {account_id, debit, credit}
    Constraint: sum(debit) == sum(credit) — enforced in operator body.

    Primitives:
      ConstraintEngine -- none (complex runtime check)
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to journal_entries + journal_lines
      EventBus         -- event.biashara.accounts.journal_entry_posted
    """
    # 1. Runtime: balanced check
    if not lines:
        return OperatorResult.fail(
            reason="Journal entry must have at least one line.",
            constraint_violated="lines_not_empty",
        )

    total_debit = round(sum(float(l.get("debit", 0.0)) for l in lines), 6)
    total_credit = round(sum(float(l.get("credit", 0.0)) for l in lines), 6)

    if abs(total_debit - total_credit) > 0.001:
        return OperatorResult.fail(
            reason=(
                f"Journal entry is unbalanced: total debit {total_debit} != total credit {total_credit}."
            ),
            constraint_violated="journal_balanced",
        )

    # 2. PawaLedger
    fail = await _charge_pawa("biashara.accounts.journal_entry", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    entry_id = _post_journal_entry(ctx, description or "Manual journal entry", lines, source_type)

    # 4. EventBus
    await ctx.events.publish("event.biashara.accounts.journal_entry_posted", {
        "entry_id": entry_id, "description": description,
        "total_debit": total_debit, "source_type": source_type,
    })

    return OperatorResult.ok({
        "entry_id": entry_id,
        "total_debit": total_debit,
        "total_credit": total_credit,
        "line_count": len(lines),
    })


# ── biashara.tax.calculate_vat ─────────────────────────────────────────────────

@sustena_operator(
    name="biashara.tax.calculate_vat",
    description="Calculate VAT (16%) on fulfilled orders for a period. Returns a ResponseWidget.",
    constraints=[],
    side_effects=["event.biashara.tax.vat_calculated"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "vat_summary_card",
        "fields": [
            {"label": "Period", "source": "inputs.period", "display": "text"},
            {"label": "Gross Revenue", "source": "outputs.gross_revenue", "display": "currency"},
            {"label": "VAT Collected", "source": "outputs.vat_collected", "display": "currency"},
        ],
        "ctas": ["Export VAT Report"],
    },
)
async def biashara_tax_calculate_vat(
    ctx: OperatorContext,
    period: str,
) -> OperatorResult:
    """
    Read-only VAT calculation over fulfilled orders.

    period: YYYY-MM string (e.g. "2026-05") — filters orders.history by fulfilled_at.
    VAT rate: 16% (Kenya standard rate).

    Primitives:
      ConstraintEngine -- none (read-only)
      PawaLedger       -- 0 pawa
      StateAccessor    -- reads orders.history
      EventBus         -- event.biashara.tax.vat_calculated
    """
    VAT_RATE = 0.16

    # 1. PawaLedger
    fail = await _charge_pawa("biashara.tax.calculate_vat", ctx)
    if fail:
        return fail

    # 2. StateAccessor — read fulfilled orders for period
    history = ctx.state.get("orders.history", [])
    period_orders = [
        o for o in history
        if o.get("status") == "fulfilled"
        and str(o.get("fulfilled_at", ""))[:7] == period
    ]

    gross_revenue = sum(float(o.get("total_revenue", o.get("total_value", 0.0))) for o in period_orders)
    # VAT is 16% on top of the taxable amount (exclusive VAT)
    vat_collected = round(gross_revenue * VAT_RATE, 2)
    net_revenue = round(gross_revenue - vat_collected, 2)
    vat_payable = vat_collected  # simplified: all collected is payable

    widget = {
        "type": "vat_summary",
        "period": period,
        "order_count": len(period_orders),
        "gross_revenue": round(gross_revenue, 2),
        "vat_collected": vat_collected,
        "vat_payable": vat_payable,
        "net_revenue": net_revenue,
        "vat_rate": VAT_RATE,
        "summary": (
            f"Period {period}: {len(period_orders)} orders, "
            f"gross KES {gross_revenue:,.2f}, VAT KES {vat_collected:,.2f}."
        ),
    }

    # 3. EventBus
    await ctx.events.publish("event.biashara.tax.vat_calculated", {
        "period": period, "gross_revenue": gross_revenue, "vat_collected": vat_collected,
    })

    return OperatorResult.ok(widget)


# ── biashara.assets.depreciate ─────────────────────────────────────────────────

@sustena_operator(
    name="biashara.assets.depreciate",
    description="Run straight-line depreciation on a fixed asset for a period.",
    constraints=[],
    side_effects=["event.biashara.asset.depreciated"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "asset_card",
        "fields": [
            {"label": "Asset", "source": "inputs.asset_id", "display": "text"},
            {"label": "Period", "source": "inputs.period", "display": "text"},
            {"label": "Depreciation", "source": "outputs.depreciation_amount", "display": "currency"},
        ],
        "ctas": ["View Assets"],
    },
)
async def biashara_assets_depreciate(
    ctx: OperatorContext,
    asset_id: str,
    period: str,
) -> OperatorResult:
    """
    Compute and record straight-line depreciation for one asset, one period.

    Expects asset fields: purchase_cost, useful_life_months (default 60), accumulated_depreciation.
    Depreciation per period = purchase_cost / useful_life_months.

    Primitives:
      ConstraintEngine -- none (runtime asset lookup)
      PawaLedger       -- 0 pawa
      StateAccessor    -- updates asset.accumulated_depreciation, asset.book_value
      EventBus         -- event.biashara.asset.depreciated
    """
    # 1. PawaLedger
    fail = await _charge_pawa("biashara.assets.depreciate", ctx)
    if fail:
        return fail

    # 2. StateAccessor — find asset
    assets = ctx.state.get("assets", {})
    if asset_id not in assets:
        return OperatorResult.fail(
            reason=f"Asset '{asset_id}' not found in state.assets.",
            constraint_violated="asset_exists",
        )

    asset = assets[asset_id]
    purchase_cost = float(asset.get("purchase_cost", 0.0))
    useful_life = int(asset.get("useful_life_months", 60))
    if useful_life <= 0:
        useful_life = 60
    accumulated = float(asset.get("accumulated_depreciation", 0.0))

    depreciation_amount = round(purchase_cost / useful_life, 4)
    new_accumulated = round(accumulated + depreciation_amount, 4)
    new_book_value = max(0.0, round(purchase_cost - new_accumulated, 4))

    asset["accumulated_depreciation"] = new_accumulated
    asset["book_value"] = new_book_value

    _post_journal_entry(ctx, f"Depreciation: asset {asset_id}, period {period}", [
        {"account_id": "depreciation_expense", "debit": depreciation_amount, "credit": 0.0},
        {"account_id": "accumulated_depreciation", "debit": 0.0, "credit": depreciation_amount},
    ], source_type="depreciation")

    # 4. EventBus
    await ctx.events.publish("event.biashara.asset.depreciated", {
        "asset_id": asset_id, "period": period,
        "depreciation_amount": depreciation_amount,
        "accumulated_depreciation": new_accumulated,
        "book_value": new_book_value,
    })

    return OperatorResult.ok({
        "asset_id": asset_id, "period": period,
        "depreciation_amount": depreciation_amount,
        "accumulated_depreciation": new_accumulated,
        "book_value": new_book_value,
    })


# ── biashara.expenses.record ───────────────────────────────────────────────────

@sustena_operator(
    name="biashara.expenses.record",
    description="Record a business expense and post Dr Expense / Cr Cash journal entry.",
    constraints=[
        "params.amount > 0",
    ],
    side_effects=["event.biashara.expense.recorded"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Category", "source": "inputs.category", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
            {"label": "Description", "source": "inputs.description", "display": "text"},
        ],
        "ctas": ["View Expenses"],
    },
)
async def biashara_expenses_record(
    ctx: OperatorContext,
    category: str,
    amount: float,
    payment_account_id: str = "cash",
    description: str = "",
) -> OperatorResult:
    """
    Record a business expense.

    Primitives:
      ConstraintEngine -- amount > 0
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to expenses list
      EventBus         -- event.biashara.expense.recorded
    """
    params = {"amount": amount}

    # 1. ConstraintEngine
    fail = _check_constraints("biashara.expenses.record", ctx, params)
    if fail:
        return fail

    # Runtime: category must not be empty
    if not category or not category.strip():
        return OperatorResult.fail(
            reason="Category must not be empty.",
            constraint_violated="category_not_empty",
        )

    # 2. PawaLedger
    fail = await _charge_pawa("biashara.expenses.record", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    expense_id = str(uuid.uuid4())
    ctx.state.append("expenses", {
        "id": expense_id,
        "expense_id": expense_id,
        "category": category,
        "amount": amount,
        "payment_account_id": payment_account_id,
        "description": description,
        "recorded_at": ctx.timestamp.isoformat(),
    })

    _post_journal_entry(ctx, f"Expense: {category} — {description or '(no description)'}", [
        {"account_id": f"expense_{category.lower().replace(' ', '_')}", "debit": amount, "credit": 0.0},
        {"account_id": payment_account_id, "debit": 0.0, "credit": amount},
    ], source_type="expense")

    # 4. EventBus
    await ctx.events.publish("event.biashara.expense.recorded", {
        "expense_id": expense_id, "category": category,
        "amount": amount, "payment_account_id": payment_account_id,
    })

    return OperatorResult.ok({
        "expense_id": expense_id, "category": category,
        "amount": amount, "description": description,
    })
