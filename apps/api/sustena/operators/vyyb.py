"""
sustena/operators/vyyb.py

QSR-specific operators for the Vyyb sustain. Extends the Biashara base sustain
with production batch tracking, KDS (Kitchen Display System), staff clock-in/out,
shift scheduling, and QSR-specific procurement.

State schema additions (beyond biashara):
  state.recipes.library       -- recipe dict keyed by recipe_id
  state.production.batches    -- production batch list
  state.kds.tasks             -- KDS task queue
  state.staff.roster          -- staff profiles keyed by staff_id
  state.staff.hours           -- clock-in/out records
  state.staff.schedules       -- shift schedule list
  state.procurement.*         -- QSR supplier and PO history

All four primitives wired in every operator:
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


# ── Shared helpers (mirror biashara pattern) ───────────────────────────────────

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
    return next((item for item in lst if item.get(id_field) == id_value), None)


# ── vyyb.production.start_batch ───────────────────────────────────────────────

@sustena_operator(
    name="vyyb.production.start_batch",
    description="Start a production batch: deduct ingredients, create batch record and KDS task.",
    constraints=[
        "params.quantity_units > 0",
    ],
    side_effects=["event.vyyb.production.batch_started"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "production_batch_widget",
        "fields": [
            {"label": "Recipe", "source": "inputs.recipe_id", "display": "text"},
            {"label": "Units", "source": "inputs.quantity_units", "display": "number"},
            {"label": "Outlet", "source": "inputs.outlet_id", "display": "text"},
        ],
        "ctas": ["View Production", "Complete Batch"],
    },
)
async def vyyb_production_start_batch(
    ctx: OperatorContext,
    recipe_id: str,
    quantity_units: float,
    outlet_id: str = "",
) -> OperatorResult:
    """
    Start a production batch for a recipe.

    Deducts ingredient inventory (recipe.ingredients * quantity_units),
    creates a batch record (status=in_progress), and creates a KDS task.

    Primitives:
      ConstraintEngine -- quantity_units > 0
      PawaLedger       -- 0 pawa
      StateAccessor    -- deducts inventory, appends to production.batches + kds.tasks
      EventBus         -- event.vyyb.production.batch_started
    """
    params = {"quantity_units": quantity_units}

    # 1. ConstraintEngine
    fail = _check_constraints("vyyb.production.start_batch", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("vyyb.production.start_batch", ctx)
    if fail:
        return fail

    # 3. StateAccessor — validate recipe
    library = ctx.state.get("recipes.library", {})
    if recipe_id not in library:
        return OperatorResult.fail(
            reason=f"Recipe '{recipe_id}' not found in recipes.library.",
            constraint_violated="recipe_exists",
        )

    recipe = library[recipe_id]
    ingredients = recipe.get("ingredients", [])
    inventory_items = ctx.state.get("inventory.items", {})

    # Validate sufficient stock before deducting
    for ing in ingredients:
        iid = ing["item_id"]
        needed = float(ing.get("quantity", 0)) * quantity_units
        available = float(inventory_items.get(iid, {}).get("qty", 0.0))
        if available < needed:
            return OperatorResult.fail(
                reason=(
                    f"Insufficient stock for ingredient '{iid}': "
                    f"need {needed}, have {available}."
                ),
                constraint_violated="ingredient_stock_sufficient",
            )

    # Deduct ingredients
    for ing in ingredients:
        iid = ing["item_id"]
        needed = float(ing.get("quantity", 0)) * quantity_units
        inventory_items[iid]["qty"] = inventory_items[iid].get("qty", 0.0) - needed
        ctx.state.append("inventory.movements", {
            "id": str(uuid.uuid4()),
            "item_id": iid,
            "delta": -needed,
            "reason": f"production_batch",
            "source_type": "production",
            "recorded_at": ctx.timestamp.isoformat(),
        })

    # Create batch
    batch_id = str(uuid.uuid4())
    batch = {
        "id": batch_id,
        "batch_id": batch_id,
        "recipe_id": recipe_id,
        "quantity_units": quantity_units,
        "quantity": quantity_units,
        "outlet_id": outlet_id,
        "status": "in_progress",
        "started_at": ctx.timestamp.isoformat(),
        "completed_at": None,
        "actual_yield": None,
    }
    ctx.state.append("production.batches", batch)

    # Create KDS task
    task_id = str(uuid.uuid4())
    ctx.state.append("kds.tasks", {
        "id": task_id,
        "task_id": task_id,
        "batch_id": batch_id,
        "recipe_id": recipe_id,
        "order_id": batch_id,
        "station": outlet_id or "production",
        "status": "pending",
        "dispatched_at": ctx.timestamp.isoformat(),
    })

    # 4. EventBus
    await ctx.events.publish("event.vyyb.production.batch_started", {
        "batch_id": batch_id, "recipe_id": recipe_id,
        "quantity_units": quantity_units, "outlet_id": outlet_id,
    })

    return OperatorResult.ok({
        "batch_id": batch_id, "recipe_id": recipe_id,
        "quantity_units": quantity_units, "status": "in_progress",
        "kds_task_id": task_id,
    })


# ── vyyb.production.complete_batch ────────────────────────────────────────────

@sustena_operator(
    name="vyyb.production.complete_batch",
    description="Complete a production batch: record yield, update finished inventory, post journal entry.",
    constraints=[
        "params.actual_yield > 0",
    ],
    side_effects=["event.vyyb.production.batch_completed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "production_batch_widget",
        "fields": [
            {"label": "Batch ID", "source": "inputs.batch_id", "display": "text"},
            {"label": "Actual Yield", "source": "inputs.actual_yield", "display": "number"},
        ],
        "ctas": ["View Production"],
    },
)
async def vyyb_production_complete_batch(
    ctx: OperatorContext,
    batch_id: str,
    actual_yield: float,
) -> OperatorResult:
    """
    Complete an in-progress production batch.

    Primitives:
      ConstraintEngine -- actual_yield > 0
      PawaLedger       -- 0 pawa
      StateAccessor    -- updates batch, appends finished inventory movement
      EventBus         -- event.vyyb.production.batch_completed
    """
    params = {"actual_yield": actual_yield}

    # 1. ConstraintEngine
    fail = _check_constraints("vyyb.production.complete_batch", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("vyyb.production.complete_batch", ctx)
    if fail:
        return fail

    # 3. StateAccessor — find batch
    batches = ctx.state.get("production.batches", [])
    batch = _find_in_list(batches, "batch_id", batch_id)
    if batch is None:
        return OperatorResult.fail(
            reason=f"Batch '{batch_id}' not found in production.batches.",
            constraint_violated="batch_exists",
        )
    if batch.get("status") != "in_progress":
        return OperatorResult.fail(
            reason=f"Batch '{batch_id}' has status '{batch.get('status')}' — must be 'in_progress'.",
            constraint_violated="batch_in_progress",
        )

    batch["status"] = "completed"
    batch["actual_yield"] = actual_yield
    batch["completed_at"] = ctx.timestamp.isoformat()

    # Add finished product to inventory as output item keyed by recipe_id
    recipe_id = batch.get("recipe_id", "")
    inventory_items = ctx.state.get("inventory.items", {})
    if recipe_id in inventory_items:
        inventory_items[recipe_id]["qty"] = inventory_items[recipe_id].get("qty", 0.0) + actual_yield

    ctx.state.append("inventory.movements", {
        "id": str(uuid.uuid4()),
        "item_id": recipe_id,
        "delta": actual_yield,
        "reason": f"batch_completed:{batch_id}",
        "source_type": "production",
        "recorded_at": ctx.timestamp.isoformat(),
    })

    # Journal: Dr Inventory (finished), Cr WIP
    _post_journal_entry(ctx, f"Production batch completed: {batch_id}", [
        {"account_id": "inventory_finished", "debit": actual_yield, "credit": 0.0},
        {"account_id": "wip", "debit": 0.0, "credit": actual_yield},
    ], source_type="production")

    # 4. EventBus
    await ctx.events.publish("event.vyyb.production.batch_completed", {
        "batch_id": batch_id, "recipe_id": recipe_id, "actual_yield": actual_yield,
    })

    return OperatorResult.ok({
        "batch_id": batch_id, "status": "completed",
        "recipe_id": recipe_id, "actual_yield": actual_yield,
    })


# ── vyyb.kds.dispatch_task ─────────────────────────────────────────────────────

@sustena_operator(
    name="vyyb.kds.dispatch_task",
    description="Dispatch a KDS task to a kitchen station for an active order.",
    constraints=[],
    side_effects=["event.vyyb.kds.task_dispatched"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "kds_widget",
        "fields": [
            {"label": "Order ID", "source": "inputs.order_id", "display": "text"},
            {"label": "Station", "source": "inputs.station", "display": "text"},
        ],
        "ctas": ["View KDS"],
    },
)
async def vyyb_kds_dispatch_task(
    ctx: OperatorContext,
    order_id: str,
    station: str,
) -> OperatorResult:
    """
    Create a KDS task for a station linked to an active order.

    Primitives:
      ConstraintEngine -- none (runtime check)
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to kds.tasks
      EventBus         -- event.vyyb.kds.task_dispatched
    """
    # 1. PawaLedger
    fail = await _charge_pawa("vyyb.kds.dispatch_task", ctx)
    if fail:
        return fail

    # 2. StateAccessor — order must exist
    active_orders = ctx.state.get("orders.active", [])
    order = _find_in_list(active_orders, "order_id", order_id)
    if order is None:
        return OperatorResult.fail(
            reason=f"Order '{order_id}' not found in orders.active.",
            constraint_violated="order_exists",
        )

    task_id = str(uuid.uuid4())
    ctx.state.append("kds.tasks", {
        "id": task_id,
        "task_id": task_id,
        "order_id": order_id,
        "station": station,
        "items": order.get("items", []),
        "status": "pending",
        "dispatched_at": ctx.timestamp.isoformat(),
        "completed_at": None,
    })

    # 4. EventBus
    await ctx.events.publish("event.vyyb.kds.task_dispatched", {
        "task_id": task_id, "order_id": order_id, "station": station,
    })

    return OperatorResult.ok({"task_id": task_id, "order_id": order_id, "station": station, "status": "pending"})


# ── vyyb.kds.complete_task ─────────────────────────────────────────────────────

@sustena_operator(
    name="vyyb.kds.complete_task",
    description="Mark a KDS task as completed.",
    constraints=[],
    side_effects=["event.vyyb.kds.task_completed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "kds_widget",
        "fields": [
            {"label": "Task ID", "source": "inputs.task_id", "display": "text"},
        ],
        "ctas": ["View KDS"],
    },
)
async def vyyb_kds_complete_task(
    ctx: OperatorContext,
    task_id: str,
) -> OperatorResult:
    """
    Complete a pending KDS task.

    Primitives:
      ConstraintEngine -- none (runtime)
      PawaLedger       -- 0 pawa
      StateAccessor    -- updates task status to completed
      EventBus         -- event.vyyb.kds.task_completed
    """
    # 1. PawaLedger
    fail = await _charge_pawa("vyyb.kds.complete_task", ctx)
    if fail:
        return fail

    # 2. StateAccessor — find task
    tasks = ctx.state.get("kds.tasks", [])
    task = _find_in_list(tasks, "task_id", task_id)
    if task is None:
        return OperatorResult.fail(
            reason=f"KDS task '{task_id}' not found in kds.tasks.",
            constraint_violated="task_exists",
        )
    if task.get("status") != "pending":
        return OperatorResult.fail(
            reason=f"Task '{task_id}' has status '{task.get('status')}' — must be 'pending'.",
            constraint_violated="task_pending",
        )

    task["status"] = "completed"
    task["completed_at"] = ctx.timestamp.isoformat()

    # 4. EventBus
    await ctx.events.publish("event.vyyb.kds.task_completed", {
        "task_id": task_id, "order_id": task.get("order_id"), "station": task.get("station"),
    })

    return OperatorResult.ok({"task_id": task_id, "status": "completed"})


# ── vyyb.staff.clock_in ────────────────────────────────────────────────────────

@sustena_operator(
    name="vyyb.staff.clock_in",
    description="Record a staff clock-in event, opening a shift hours record.",
    constraints=[],
    side_effects=["event.vyyb.staff.clocked_in"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "staff_card",
        "fields": [
            {"label": "Staff", "source": "inputs.staff_party_id", "display": "text"},
            {"label": "Outlet", "source": "inputs.outlet_id", "display": "text"},
        ],
        "ctas": ["View Staff"],
    },
)
async def vyyb_staff_clock_in(
    ctx: OperatorContext,
    staff_party_id: str,
    outlet_id: str = "",
) -> OperatorResult:
    """
    Clock in a staff member. Must exist in staff.roster.

    Primitives:
      ConstraintEngine -- none (runtime)
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to staff.hours
      EventBus         -- event.vyyb.staff.clocked_in
    """
    # 1. PawaLedger
    fail = await _charge_pawa("vyyb.staff.clock_in", ctx)
    if fail:
        return fail

    # 2. StateAccessor — staff must exist
    roster = ctx.state.get("staff.roster", {})
    if staff_party_id not in roster:
        return OperatorResult.fail(
            reason=f"Staff '{staff_party_id}' not found in staff.roster.",
            constraint_violated="staff_exists",
        )

    hours_id = str(uuid.uuid4())
    ctx.state.append("staff.hours", {
        "id": hours_id,
        "hours_id": hours_id,
        "staff_party_id": staff_party_id,
        "outlet_id": outlet_id,
        "clock_in": ctx.timestamp.isoformat(),
        "clock_out": None,
        "duration_minutes": None,
        "status": "open",
    })

    # 4. EventBus
    await ctx.events.publish("event.vyyb.staff.clocked_in", {
        "staff_party_id": staff_party_id, "outlet_id": outlet_id,
        "clock_in": ctx.timestamp.isoformat(), "hours_id": hours_id,
    })

    return OperatorResult.ok({
        "hours_id": hours_id, "staff_party_id": staff_party_id,
        "clock_in": ctx.timestamp.isoformat(),
    })


# ── vyyb.staff.clock_out ───────────────────────────────────────────────────────

@sustena_operator(
    name="vyyb.staff.clock_out",
    description="Record staff clock-out, closing the open hours entry and computing duration.",
    constraints=[],
    side_effects=["event.vyyb.staff.clocked_out"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "staff_card",
        "fields": [
            {"label": "Staff", "source": "inputs.staff_party_id", "display": "text"},
            {"label": "Duration", "source": "outputs.duration_minutes", "display": "number"},
        ],
        "ctas": ["View Staff"],
    },
)
async def vyyb_staff_clock_out(
    ctx: OperatorContext,
    staff_party_id: str,
) -> OperatorResult:
    """
    Clock out a staff member by closing their open hours entry.

    Primitives:
      ConstraintEngine -- none (runtime)
      PawaLedger       -- 0 pawa
      StateAccessor    -- updates open hours entry
      EventBus         -- event.vyyb.staff.clocked_out
    """
    # 1. PawaLedger
    fail = await _charge_pawa("vyyb.staff.clock_out", ctx)
    if fail:
        return fail

    # 2. StateAccessor — find open clock-in
    hours_list = ctx.state.get("staff.hours", [])
    open_entry = next(
        (h for h in hours_list
         if h.get("staff_party_id") == staff_party_id and h.get("status") == "open"),
        None,
    )
    if open_entry is None:
        return OperatorResult.fail(
            reason=f"No open clock-in found for staff '{staff_party_id}'.",
            constraint_violated="open_clock_in_exists",
        )

    clock_in_str = open_entry.get("clock_in", "")
    clock_out_time = ctx.timestamp
    duration_minutes = 0.0
    if clock_in_str:
        try:
            clock_in_time = datetime.fromisoformat(clock_in_str)
            duration_minutes = round((clock_out_time - clock_in_time).total_seconds() / 60, 2)
        except ValueError:
            duration_minutes = 0.0

    open_entry["clock_out"] = clock_out_time.isoformat()
    open_entry["duration_minutes"] = duration_minutes
    open_entry["status"] = "closed"

    # 4. EventBus
    await ctx.events.publish("event.vyyb.staff.clocked_out", {
        "staff_party_id": staff_party_id,
        "hours_id": open_entry.get("hours_id"),
        "duration_minutes": duration_minutes,
    })

    return OperatorResult.ok({
        "staff_party_id": staff_party_id,
        "hours_id": open_entry.get("hours_id"),
        "clock_out": clock_out_time.isoformat(),
        "duration_minutes": duration_minutes,
    })


# ── vyyb.staff.schedule_shift ──────────────────────────────────────────────────

@sustena_operator(
    name="vyyb.staff.schedule_shift",
    description="Schedule a future shift for a staff member.",
    constraints=[],
    side_effects=["event.vyyb.staff.shift_scheduled"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "staff_card",
        "fields": [
            {"label": "Staff", "source": "inputs.staff_party_id", "display": "text"},
            {"label": "Date", "source": "inputs.shift_date", "display": "text"},
            {"label": "Template", "source": "inputs.shift_template", "display": "text"},
        ],
        "ctas": ["View Schedule"],
    },
)
async def vyyb_staff_schedule_shift(
    ctx: OperatorContext,
    staff_party_id: str,
    shift_date: str,
    shift_template: str,
    outlet_id: str = "",
) -> OperatorResult:
    """
    Add a shift to staff.schedules.

    Primitives:
      ConstraintEngine -- none
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to staff.schedules
      EventBus         -- event.vyyb.staff.shift_scheduled
    """
    # 1. PawaLedger
    fail = await _charge_pawa("vyyb.staff.schedule_shift", ctx)
    if fail:
        return fail

    # 2. StateAccessor
    schedule_id = str(uuid.uuid4())
    ctx.state.append("staff.schedules", {
        "id": schedule_id,
        "schedule_id": schedule_id,
        "staff_party_id": staff_party_id,
        "shift_date": shift_date,
        "shift_template": shift_template,
        "outlet_id": outlet_id,
        "created_at": ctx.timestamp.isoformat(),
    })

    # 4. EventBus
    await ctx.events.publish("event.vyyb.staff.shift_scheduled", {
        "schedule_id": schedule_id, "staff_party_id": staff_party_id,
        "shift_date": shift_date, "shift_template": shift_template, "outlet_id": outlet_id,
    })

    return OperatorResult.ok({
        "schedule_id": schedule_id, "staff_party_id": staff_party_id,
        "shift_date": shift_date, "shift_template": shift_template,
    })


# ── vyyb.procurement.raise_po ─────────────────────────────────────────────────

@sustena_operator(
    name="vyyb.procurement.raise_po",
    description="Raise a QSR procurement PO to a supplier. Adds delivery_date. Vendor must exist in state.parties.",
    constraints=[],
    side_effects=["event.vyyb.procurement.po_raised"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "po_card",
        "fields": [
            {"label": "Vendor", "source": "inputs.vendor_party_id", "display": "text"},
            {"label": "Delivery Date", "source": "inputs.delivery_date", "display": "text"},
        ],
        "ctas": ["View POs"],
    },
)
async def vyyb_procurement_raise_po(
    ctx: OperatorContext,
    vendor_party_id: str,
    items: list,
    delivery_date: str = "",
) -> OperatorResult:
    """
    Raise a procurement PO — Vyyb namespace. Adds delivery_date to PO record.
    Also appends to vyyb procurement.po_history in addition to purchase_orders.active.

    Primitives:
      ConstraintEngine -- none (runtime)
      PawaLedger       -- 0 pawa
      StateAccessor    -- appends to purchase_orders.active + procurement.po_history
      EventBus         -- event.vyyb.procurement.po_raised
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
    fail = await _charge_pawa("vyyb.procurement.raise_po", ctx)
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
        "namespace": "vyyb.procurement",
    }
    ctx.state.append("purchase_orders.active", po)
    ctx.state.append("procurement.po_history", {**po})

    # 4. EventBus
    await ctx.events.publish("event.vyyb.procurement.po_raised", {
        "po_id": po_id, "vendor_party_id": vendor_party_id,
        "total_value": total_value, "delivery_date": delivery_date,
    })

    return OperatorResult.ok({
        "po_id": po_id, "status": "draft",
        "total_value": total_value, "delivery_date": delivery_date,
    })


# ── vyyb.procurement.receive_po ────────────────────────────────────────────────

@sustena_operator(
    name="vyyb.procurement.receive_po",
    description="Receive goods against an open Vyyb procurement PO. Updates inventory.",
    constraints=[],
    side_effects=["event.vyyb.procurement.po_received"],
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
async def vyyb_procurement_receive_po(
    ctx: OperatorContext,
    po_id: str,
    items_received: list,
) -> OperatorResult:
    """
    Receive goods against an open procurement PO — Vyyb namespace.

    Primitives:
      ConstraintEngine -- none (runtime)
      PawaLedger       -- 0 pawa
      StateAccessor    -- updates inventory, closes PO
      EventBus         -- event.vyyb.procurement.po_received
    """
    # 1. PawaLedger
    fail = await _charge_pawa("vyyb.procurement.receive_po", ctx)
    if fail:
        return fail

    # 2. StateAccessor — find PO (in purchase_orders.active; also present in procurement.po_history)
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
            "reason": f"vyyb_po_receipt:{po_id}",
            "source_type": "purchase",
            "recorded_at": ctx.timestamp.isoformat(),
        })

    po["status"] = "received"
    po["received_at"] = ctx.timestamp.isoformat()
    po["items_received"] = items_received
    active_pos[:] = [p for p in active_pos if p.get("po_id") != po_id]

    # Update the copy in procurement.po_history too
    ph = ctx.state.get("procurement.po_history", [])
    ph_entry = _find_in_list(ph, "po_id", po_id)
    if ph_entry:
        ph_entry["status"] = "received"
        ph_entry["received_at"] = ctx.timestamp.isoformat()

    # 4. EventBus
    await ctx.events.publish("event.vyyb.procurement.po_received", {
        "po_id": po_id, "items_received": items_received,
    })

    return OperatorResult.ok({
        "po_id": po_id, "status": "received",
        "items_received_count": len(items_received),
    })
