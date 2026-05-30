"""
tests/test_vyyb_operators.py

Tests for the Vyyb QSR operator library — all 9 operators.
Covers: happy paths, constraint rejections (missing recipe, batch not in_progress,
        no open clock-in, unknown staff, etc.), state mutations, events fired.

Run with: pytest tests/test_vyyb_operators.py -v
"""

import pytest
from datetime import datetime, timedelta

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _base_state() -> dict:
    """Minimal Vyyb state: inventory, recipes, production, kds, staff, parties, procurement."""
    return {
        "inventory": {
            "items": {
                "flour": {"item_id": "flour", "name": "Wheat Flour",
                           "qty": 500.0, "unit": "kg", "unit_cost": 60.0},
                "oil":   {"item_id": "oil",   "name": "Cooking Oil",
                           "qty": 100.0, "unit": "L",  "unit_cost": 200.0},
                "chapati_finished": {"item_id": "chapati_finished", "name": "Chapati",
                                      "qty": 0.0, "unit": "piece", "unit_cost": 14.0},
            },
            "movements": [],
        },
        "accounts": {
            "chart": {},
            "journal_entries": [],
            "journal_lines": [],
        },
        "parties": {
            "vendor1": {"party_id": "vendor1", "name": "Unga Mills", "type": "vendor"},
        },
        "orders": {"active": [], "history": []},
        "purchases": {"active": [], "history": []},
        "purchase_orders": {"active": [], "history": []},
        "assets": {},
        "expenses": [],
        "recipes": {
            "library": {
                "chapati": {
                    "recipe_id": "chapati",
                    "name": "Chapati",
                    "tier": "finished_good",
                    "yield_qty": 10.0,
                    "yield_unit": "piece",
                    "ingredients": [
                        {"item_id": "flour", "quantity": 0.5, "unit": "kg"},
                        {"item_id": "oil",   "quantity": 0.1, "unit": "L"},
                    ],
                },
            },
        },
        "production": {"batches": [], "plans": []},
        "kds": {"tasks": [], "station_routes": {}},
        "staff": {
            "roster": {
                "alice": {"staff_id": "alice", "name": "Alice Wanjiru", "role": "chef"},
                "bob":   {"staff_id": "bob",   "name": "Bob Otieno",    "role": "cashier"},
            },
            "schedules": [],
            "shifts": [],
            "payroll": [],
            "hours": [],
        },
        "procurement": {"suppliers": {}, "po_history": []},
    }


def _fresh_state() -> StateAccessor:
    return StateAccessor(_base_state())


def _make_ctx(state: StateAccessor, ts: datetime = None) -> OperatorContext:
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-vyyb"),
        pawa=PawaLedger(),
        sustain_id="test-vyyb",
        user_id="user-test",
        timestamp=ts or datetime.utcnow(),
    )


def _op(name):
    return OPERATOR_REGISTRY[name].fn


# ── Registry ──────────────────────────────────────────────────────────────────

class TestVyybRegistry:

    def test_all_9_vyyb_operators_registered(self):
        expected = [
            "vyyb.production.start_batch",
            "vyyb.production.complete_batch",
            "vyyb.kds.dispatch_task",
            "vyyb.kds.complete_task",
            "vyyb.staff.clock_in",
            "vyyb.staff.clock_out",
            "vyyb.staff.schedule_shift",
            "vyyb.procurement.raise_po",
            "vyyb.procurement.receive_po",
        ]
        for name in expected:
            assert name in OPERATOR_REGISTRY, f"'{name}' not in OPERATOR_REGISTRY"

    def test_operators_have_required_metadata(self):
        for name in OPERATOR_REGISTRY:
            if not name.startswith("vyyb."):
                continue
            meta = OPERATOR_REGISTRY[name]
            assert meta.description
            assert meta.author == "sustena_core"
            assert isinstance(meta.pawa_cost, int)
            assert isinstance(meta.ui_schema, dict)
            assert "widget_type" in meta.ui_schema

    def test_side_effects_declared(self):
        pairs = [
            ("vyyb.production.start_batch",    "event.vyyb.production.batch_started"),
            ("vyyb.production.complete_batch",  "event.vyyb.production.batch_completed"),
            ("vyyb.kds.dispatch_task",          "event.vyyb.kds.task_dispatched"),
            ("vyyb.kds.complete_task",          "event.vyyb.kds.task_completed"),
            ("vyyb.staff.clock_in",             "event.vyyb.staff.clocked_in"),
            ("vyyb.staff.clock_out",            "event.vyyb.staff.clocked_out"),
            ("vyyb.staff.schedule_shift",       "event.vyyb.staff.shift_scheduled"),
            ("vyyb.procurement.raise_po",       "event.vyyb.procurement.po_raised"),
            ("vyyb.procurement.receive_po",     "event.vyyb.procurement.po_received"),
        ]
        for op_name, event_name in pairs:
            assert event_name in OPERATOR_REGISTRY[op_name].side_effects,                 f"{op_name} missing side_effect {event_name}"


# ── vyyb.production.start_batch ───────────────────────────────────────────────

class TestProductionStartBatch:

    @pytest.mark.asyncio
    async def test_creates_batch_in_progress(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.production.start_batch")(
            ctx, recipe_id="chapati", quantity_units=4.0, outlet_id="kitchen1"
        )
        assert result.succeeded
        assert result.data["status"] == "in_progress"
        batches = state.get("production.batches")
        assert len(batches) == 1
        assert batches[0]["status"] == "in_progress"

    @pytest.mark.asyncio
    async def test_deducts_ingredients(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        before_flour = state.get("inventory.items")["flour"]["qty"]
        before_oil   = state.get("inventory.items")["oil"]["qty"]
        await _op("vyyb.production.start_batch")(ctx, recipe_id="chapati", quantity_units=4.0)
        # flour: 0.5 kg * 4 units = 2.0 kg deducted; oil: 0.1 L * 4 = 0.4 L
        assert state.get("inventory.items")["flour"]["qty"] == before_flour - 2.0
        assert state.get("inventory.items")["oil"]["qty"]   == pytest.approx(before_oil - 0.4, abs=1e-6)

    @pytest.mark.asyncio
    async def test_appends_ingredient_movements(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("vyyb.production.start_batch")(ctx, recipe_id="chapati", quantity_units=2.0)
        movements = state.get("inventory.movements")
        assert len(movements) == 2  # flour + oil
        assert all(m["source_type"] == "production" for m in movements)
        assert all(m["delta"] < 0 for m in movements)

    @pytest.mark.asyncio
    async def test_creates_kds_task(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.production.start_batch")(
            ctx, recipe_id="chapati", quantity_units=1.0, outlet_id="grill"
        )
        assert "kds_task_id" in result.data
        tasks = state.get("kds.tasks")
        assert len(tasks) == 1

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("vyyb.production.start_batch")(ctx, recipe_id="chapati", quantity_units=1.0)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.production.batch_started" for e in events)

    @pytest.mark.asyncio
    async def test_fails_missing_recipe(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.production.start_batch")(
            ctx, recipe_id="no_such_recipe", quantity_units=1.0
        )
        assert result.failed
        assert result.constraint_violated == "recipe_exists"

    @pytest.mark.asyncio
    async def test_fails_zero_quantity(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.production.start_batch")(
            ctx, recipe_id="chapati", quantity_units=0.0
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_fails_insufficient_stock(self):
        state = _fresh_state()
        # Drain flour to near-zero
        state.get("inventory.items")["flour"]["qty"] = 0.1
        ctx = _make_ctx(state)
        result = await _op("vyyb.production.start_batch")(
            ctx, recipe_id="chapati", quantity_units=100.0
        )
        assert result.failed
        assert result.constraint_violated == "ingredient_stock_sufficient"


# ── vyyb.production.complete_batch ────────────────────────────────────────────

class TestProductionCompleteBatch:

    async def _state_with_batch(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        r = await _op("vyyb.production.start_batch")(
            ctx, recipe_id="chapati", quantity_units=4.0
        )
        return state, _make_ctx(state), r.data["batch_id"]

    @pytest.mark.asyncio
    async def test_marks_batch_completed(self):
        state, ctx, batch_id = await self._state_with_batch()
        result = await _op("vyyb.production.complete_batch")(
            ctx, batch_id=batch_id, actual_yield=38.0
        )
        assert result.succeeded
        assert result.data["status"] == "completed"
        batches = state.get("production.batches")
        assert batches[0]["status"] == "completed"
        assert batches[0]["actual_yield"] == 38.0

    @pytest.mark.asyncio
    async def test_posts_balanced_journal(self):
        state, ctx, batch_id = await self._state_with_batch()
        await _op("vyyb.production.complete_batch")(ctx, batch_id=batch_id, actual_yield=38.0)
        entries = state.get("accounts.journal_entries")
        entry = entries[-1]
        lines = entry["lines"]
        assert abs(sum(l["debit"] for l in lines) - sum(l["credit"] for l in lines)) < 0.001

    @pytest.mark.asyncio
    async def test_appends_production_movement(self):
        state, ctx, batch_id = await self._state_with_batch()
        await _op("vyyb.production.complete_batch")(ctx, batch_id=batch_id, actual_yield=38.0)
        movements = state.get("inventory.movements")
        prod_movements = [m for m in movements if m["source_type"] == "production" and m["delta"] > 0]
        assert len(prod_movements) == 1
        assert prod_movements[0]["delta"] == 38.0

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state, ctx, batch_id = await self._state_with_batch()
        await _op("vyyb.production.complete_batch")(ctx, batch_id=batch_id, actual_yield=38.0)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.production.batch_completed" for e in events)

    @pytest.mark.asyncio
    async def test_fails_missing_batch(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.production.complete_batch")(
            ctx, batch_id="no-such-batch", actual_yield=10.0
        )
        assert result.failed
        assert result.constraint_violated == "batch_exists"

    @pytest.mark.asyncio
    async def test_fails_batch_not_in_progress(self):
        state, ctx, batch_id = await self._state_with_batch()
        # Complete it once
        await _op("vyyb.production.complete_batch")(ctx, batch_id=batch_id, actual_yield=38.0)
        # Try to complete again
        ctx2 = _make_ctx(state)
        result = await _op("vyyb.production.complete_batch")(ctx2, batch_id=batch_id, actual_yield=10.0)
        assert result.failed
        assert result.constraint_violated == "batch_in_progress"

    @pytest.mark.asyncio
    async def test_fails_zero_actual_yield(self):
        state, ctx, batch_id = await self._state_with_batch()
        result = await _op("vyyb.production.complete_batch")(
            ctx, batch_id=batch_id, actual_yield=0.0
        )
        assert result.failed


# ── vyyb.kds.dispatch_task ─────────────────────────────────────────────────────

class TestKdsDispatchTask:

    async def _state_with_order(self):
        import sustena.operators.biashara  # noqa
        state = _fresh_state()
        ctx = _make_ctx(state)
        r = await _op("biashara.orders.place")(
            ctx, items=[{"item_id": "flour", "quantity": 1.0, "unit_price": 120.0}]
        )
        return state, _make_ctx(state), r.data["order_id"]

    @pytest.mark.asyncio
    async def test_creates_kds_task(self):
        state, ctx, order_id = await self._state_with_order()
        result = await _op("vyyb.kds.dispatch_task")(ctx, order_id=order_id, station="grill")
        assert result.succeeded
        assert result.data["status"] == "pending"
        tasks = state.get("kds.tasks")
        assert len(tasks) == 1
        assert tasks[0]["station"] == "grill"

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state, ctx, order_id = await self._state_with_order()
        await _op("vyyb.kds.dispatch_task")(ctx, order_id=order_id, station="fryer")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.kds.task_dispatched" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_order(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.kds.dispatch_task")(ctx, order_id="no-order", station="grill")
        assert result.failed
        assert result.constraint_violated == "order_exists"


# ── vyyb.kds.complete_task ─────────────────────────────────────────────────────

class TestKdsCompleteTask:

    async def _state_with_task(self):
        import sustena.operators.biashara  # noqa
        state = _fresh_state()
        ctx = _make_ctx(state)
        r = await _op("biashara.orders.place")(
            ctx, items=[{"item_id": "flour", "quantity": 1.0, "unit_price": 100.0}]
        )
        order_id = r.data["order_id"]
        r2 = await _op("vyyb.kds.dispatch_task")(ctx, order_id=order_id, station="grill")
        return state, _make_ctx(state), r2.data["task_id"]

    @pytest.mark.asyncio
    async def test_marks_task_completed(self):
        state, ctx, task_id = await self._state_with_task()
        result = await _op("vyyb.kds.complete_task")(ctx, task_id=task_id)
        assert result.succeeded
        tasks = state.get("kds.tasks")
        assert tasks[0]["status"] == "completed"

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state, ctx, task_id = await self._state_with_task()
        await _op("vyyb.kds.complete_task")(ctx, task_id=task_id)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.kds.task_completed" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_task(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.kds.complete_task")(ctx, task_id="no-such-task")
        assert result.failed
        assert result.constraint_violated == "task_exists"

    @pytest.mark.asyncio
    async def test_fails_task_not_pending(self):
        state, ctx, task_id = await self._state_with_task()
        await _op("vyyb.kds.complete_task")(ctx, task_id=task_id)
        ctx2 = _make_ctx(state)
        result = await _op("vyyb.kds.complete_task")(ctx2, task_id=task_id)
        assert result.failed
        assert result.constraint_violated == "task_pending"


# ── vyyb.staff.clock_in ────────────────────────────────────────────────────────

class TestStaffClockIn:

    @pytest.mark.asyncio
    async def test_creates_hours_entry(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.staff.clock_in")(ctx, staff_party_id="alice", outlet_id="hive")
        assert result.succeeded
        hours = state.get("staff.hours")
        assert len(hours) == 1
        assert hours[0]["status"] == "open"
        assert hours[0]["staff_party_id"] == "alice"

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("vyyb.staff.clock_in")(ctx, staff_party_id="alice")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.staff.clocked_in" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_staff(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.staff.clock_in")(ctx, staff_party_id="ghost_staff")
        assert result.failed
        assert result.constraint_violated == "staff_exists"


# ── vyyb.staff.clock_out ───────────────────────────────────────────────────────

class TestStaffClockOut:

    async def _state_clocked_in(self, clock_in_delta_minutes=120):
        state = _fresh_state()
        ts_in = datetime.utcnow() - timedelta(minutes=clock_in_delta_minutes)
        ctx_in = _make_ctx(state, ts=ts_in)
        await _op("vyyb.staff.clock_in")(ctx_in, staff_party_id="alice", outlet_id="hive")
        return state, _make_ctx(state)

    @pytest.mark.asyncio
    async def test_closes_hours_entry(self):
        state, ctx = await self._state_clocked_in()
        result = await _op("vyyb.staff.clock_out")(ctx, staff_party_id="alice")
        assert result.succeeded
        hours = state.get("staff.hours")
        assert hours[0]["status"] == "closed"
        assert hours[0]["clock_out"] is not None

    @pytest.mark.asyncio
    async def test_computes_duration(self):
        state, ctx = await self._state_clocked_in(clock_in_delta_minutes=120)
        result = await _op("vyyb.staff.clock_out")(ctx, staff_party_id="alice")
        assert result.succeeded
        # Duration should be approximately 120 minutes (±2 min tolerance for test execution)
        assert result.data["duration_minutes"] >= 118.0

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state, ctx = await self._state_clocked_in()
        await _op("vyyb.staff.clock_out")(ctx, staff_party_id="alice")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.staff.clocked_out" for e in events)

    @pytest.mark.asyncio
    async def test_fails_no_open_clock_in(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.staff.clock_out")(ctx, staff_party_id="alice")
        assert result.failed
        assert result.constraint_violated == "open_clock_in_exists"


# ── vyyb.staff.schedule_shift ──────────────────────────────────────────────────

class TestStaffScheduleShift:

    @pytest.mark.asyncio
    async def test_appends_schedule(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.staff.schedule_shift")(
            ctx, staff_party_id="bob", shift_date="2026-06-01",
            shift_template="morning", outlet_id="hive",
        )
        assert result.succeeded
        schedules = state.get("staff.schedules")
        assert len(schedules) == 1
        assert schedules[0]["staff_party_id"] == "bob"
        assert schedules[0]["shift_date"] == "2026-06-01"

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("vyyb.staff.schedule_shift")(
            ctx, staff_party_id="alice", shift_date="2026-06-02", shift_template="evening"
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.staff.shift_scheduled" for e in events)

    @pytest.mark.asyncio
    async def test_schedule_id_present(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.staff.schedule_shift")(
            ctx, staff_party_id="alice", shift_date="2026-06-03", shift_template="morning"
        )
        assert "schedule_id" in result.data


# ── vyyb.procurement.raise_po ─────────────────────────────────────────────────

class TestProcurementRaisePO:

    @pytest.mark.asyncio
    async def test_creates_draft_po(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.procurement.raise_po")(
            ctx, vendor_party_id="vendor1",
            items=[{"item_id": "flour", "qty_ordered": 50.0, "unit_price": 60.0}],
            delivery_date="2026-06-10",
        )
        assert result.succeeded
        assert result.data["status"] == "draft"
        assert result.data["delivery_date"] == "2026-06-10"
        assert len(state.get("purchase_orders.active")) == 1

    @pytest.mark.asyncio
    async def test_also_appended_to_procurement_po_history(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("vyyb.procurement.raise_po")(
            ctx, vendor_party_id="vendor1",
            items=[{"item_id": "flour", "qty_ordered": 20.0, "unit_price": 60.0}],
        )
        assert len(state.get("procurement.po_history")) == 1

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("vyyb.procurement.raise_po")(
            ctx, vendor_party_id="vendor1",
            items=[{"item_id": "flour", "qty_ordered": 10.0, "unit_price": 60.0}],
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.procurement.po_raised" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_vendor(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.procurement.raise_po")(
            ctx, vendor_party_id="no_vendor",
            items=[{"item_id": "flour", "qty_ordered": 10.0, "unit_price": 60.0}],
        )
        assert result.failed
        assert result.constraint_violated == "vendor_exists"

    @pytest.mark.asyncio
    async def test_fails_empty_items(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.procurement.raise_po")(
            ctx, vendor_party_id="vendor1", items=[]
        )
        assert result.failed


# ── vyyb.procurement.receive_po ────────────────────────────────────────────────

class TestProcurementReceivePO:

    async def _state_with_po(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        r = await _op("vyyb.procurement.raise_po")(
            ctx, vendor_party_id="vendor1",
            items=[{"item_id": "flour", "qty_ordered": 50.0, "unit_price": 60.0}],
        )
        return state, _make_ctx(state), r.data["po_id"]

    @pytest.mark.asyncio
    async def test_marks_po_received(self):
        state, ctx, po_id = await self._state_with_po()
        result = await _op("vyyb.procurement.receive_po")(
            ctx, po_id=po_id,
            items_received=[{"item_id": "flour", "qty_received": 50.0}],
        )
        assert result.succeeded
        assert result.data["status"] == "received"
        assert state.get("purchase_orders.active") == []

    @pytest.mark.asyncio
    async def test_updates_inventory(self):
        state, ctx, po_id = await self._state_with_po()
        before_qty = state.get("inventory.items")["flour"]["qty"]
        await _op("vyyb.procurement.receive_po")(
            ctx, po_id=po_id,
            items_received=[{"item_id": "flour", "qty_received": 50.0}],
        )
        assert state.get("inventory.items")["flour"]["qty"] == before_qty + 50.0

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state, ctx, po_id = await self._state_with_po()
        await _op("vyyb.procurement.receive_po")(
            ctx, po_id=po_id,
            items_received=[{"item_id": "flour", "qty_received": 10.0}],
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.vyyb.procurement.po_received" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_po(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("vyyb.procurement.receive_po")(
            ctx, po_id="no-such-po",
            items_received=[{"item_id": "flour", "qty_received": 10.0}],
        )
        assert result.failed
        assert result.constraint_violated == "po_exists"
