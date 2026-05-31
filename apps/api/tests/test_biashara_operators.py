"""
tests/test_biashara_operators.py

Tests for the Biashara operator library — all 11 operators.
Covers: happy paths, constraint rejections, journal balance invariant,
        VAT ResponseWidget, state mutations, events fired.

Run with: pytest tests/test_biashara_operators.py -v
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _base_state() -> dict:
    """Minimal Biashara state dict with two inventory items, one party, and one asset."""
    return {
        "inventory": {
            "items": {
                "flour": {"item_id": "flour", "name": "Wheat Flour", "qty": 100.0,
                           "unit": "kg", "unit_cost": 60.0, "standard_cost": 60.0},
                "oil": {"item_id": "oil", "name": "Cooking Oil", "qty": 50.0,
                         "unit": "L", "unit_cost": 200.0, "standard_cost": 200.0},
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
            "cust1":   {"party_id": "cust1",   "name": "Jane Njeri",  "type": "customer"},
        },
        "orders": {"active": [], "history": []},
        "purchases": {"active": [], "history": []},
        "purchase_orders": {"active": [], "history": []},
        "assets": {
            "oven1": {
                "asset_id": "oven1", "name": "Commercial Oven",
                "purchase_cost": 120000.0, "book_value": 120000.0,
                "useful_life_months": 60, "accumulated_depreciation": 0.0,
            },
        },
        "expenses": [],
    }


def _fresh_state() -> StateAccessor:
    return StateAccessor(_base_state())


def _make_ctx(state: StateAccessor) -> OperatorContext:
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-biashara"),
        pawa=PawaLedger(),
        sustain_id="test-biashara",
        user_id="user-test",
        timestamp=datetime.utcnow(),
    )


def _op(name):
    return OPERATOR_REGISTRY[name].fn


# ── Registry ──────────────────────────────────────────────────────────────────

class TestBiasharaRegistry:

    def test_all_11_operators_registered(self):
        expected = [
            "biashara.inventory.restock",
            "biashara.inventory.adjust",
            "biashara.orders.place",
            "biashara.orders.fulfill",
            "biashara.purchases.record",
            "biashara.po.raise",
            "biashara.po.receive",
            "biashara.accounts.journal_entry",
            "biashara.tax.calculate_vat",
            "biashara.assets.depreciate",
            "biashara.expenses.record",
        ]
        for name in expected:
            assert name in OPERATOR_REGISTRY, f"'{name}' not in OPERATOR_REGISTRY"

    def test_operators_have_metadata(self):
        for name in OPERATOR_REGISTRY:
            if not name.startswith("biashara."):
                continue
            meta = OPERATOR_REGISTRY[name]
            assert meta.description
            assert meta.author == "sustena_core"
            assert isinstance(meta.pawa_cost, int)
            assert isinstance(meta.ui_schema, dict)
            assert "widget_type" in meta.ui_schema

    def test_mutating_operators_declare_side_effects(self):
        pairs = [
            ("biashara.inventory.restock",   "event.biashara.inventory.restocked"),
            ("biashara.orders.fulfill",       "event.biashara.order.fulfilled"),
            ("biashara.expenses.record",      "event.biashara.expense.recorded"),
            ("biashara.accounts.journal_entry", "event.biashara.accounts.journal_entry_posted"),
        ]
        for op_name, event_name in pairs:
            assert event_name in OPERATOR_REGISTRY[op_name].side_effects,                 f"{op_name} missing side_effect {event_name}"


# ── biashara.inventory.restock ─────────────────────────────────────────────────

class TestInventoryRestock:

    @pytest.mark.asyncio
    async def test_increases_qty(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.restock")(
            ctx, item_id="flour", quantity=50.0, unit_cost=60.0, supplier_party_id="vendor1"
        )
        assert result.succeeded
        assert state.get("inventory.items")["flour"]["qty"] == 150.0

    @pytest.mark.asyncio
    async def test_appends_movement(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.inventory.restock")(ctx, item_id="flour", quantity=10.0, unit_cost=60.0)
        movements = state.get("inventory.movements")
        assert len(movements) == 1
        assert movements[0]["source_type"] == "purchase"
        assert movements[0]["delta"] == 10.0

    @pytest.mark.asyncio
    async def test_posts_balanced_journal_entry(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.inventory.restock")(ctx, item_id="flour", quantity=10.0, unit_cost=60.0)
        entries = state.get("accounts.journal_entries")
        assert len(entries) == 1
        lines = entries[0]["lines"]
        total_debit  = sum(l["debit"]  for l in lines)
        total_credit = sum(l["credit"] for l in lines)
        assert abs(total_debit - total_credit) < 0.001

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.inventory.restock")(ctx, item_id="flour", quantity=10.0, unit_cost=60.0)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.inventory.restocked" for e in events)

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_quantity(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.restock")(ctx, item_id="flour", quantity=0.0, unit_cost=60.0)
        assert result.failed

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_unit_cost(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.restock")(ctx, item_id="flour", quantity=10.0, unit_cost=0.0)
        assert result.failed

    @pytest.mark.asyncio
    async def test_fails_unknown_item(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.restock")(ctx, item_id="ghost", quantity=10.0, unit_cost=60.0)
        assert result.failed
        assert result.constraint_violated == "item_exists"


# ── biashara.inventory.adjust ──────────────────────────────────────────────────

class TestInventoryAdjust:

    @pytest.mark.asyncio
    async def test_positive_delta_increases_qty(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.adjust")(ctx, item_id="flour", qty_delta=20.0, reason="stocktake")
        assert result.succeeded
        assert state.get("inventory.items")["flour"]["qty"] == 120.0

    @pytest.mark.asyncio
    async def test_negative_delta_decreases_qty(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.adjust")(ctx, item_id="flour", qty_delta=-10.0, reason="waste")
        assert result.succeeded
        assert state.get("inventory.items")["flour"]["qty"] == 90.0

    @pytest.mark.asyncio
    async def test_appends_adjustment_movement(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.inventory.adjust")(ctx, item_id="oil", qty_delta=5.0, reason="found")
        movements = state.get("inventory.movements")
        assert movements[0]["source_type"] == "adjustment"
        assert movements[0]["reason"] == "found"

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.inventory.adjust")(ctx, item_id="flour", qty_delta=5.0, reason="audit")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.inventory.adjusted" for e in events)

    @pytest.mark.asyncio
    async def test_fails_on_empty_reason(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.adjust")(ctx, item_id="flour", qty_delta=5.0, reason="  ")
        assert result.failed
        assert result.constraint_violated == "reason_not_empty"

    @pytest.mark.asyncio
    async def test_fails_on_negative_qty_result(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.adjust")(ctx, item_id="flour", qty_delta=-999.0, reason="error")
        assert result.failed
        assert result.constraint_violated == "inventory_qty_non_negative"

    @pytest.mark.asyncio
    async def test_fails_unknown_item(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.inventory.adjust")(ctx, item_id="ghost", qty_delta=1.0, reason="test")
        assert result.failed


# ── biashara.orders.place ──────────────────────────────────────────────────────

class TestOrdersPlace:

    @pytest.mark.asyncio
    async def test_creates_active_order(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.orders.place")(
            ctx,
            items=[{"item_id": "flour", "quantity": 2.0, "unit_price": 120.0}],
            customer_party_id="cust1",
        )
        assert result.succeeded
        assert result.data["status"] == "pending"
        active = state.get("orders.active")
        assert len(active) == 1
        assert active[0]["status"] == "pending"

    @pytest.mark.asyncio
    async def test_total_value_calculated(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.orders.place")(
            ctx,
            items=[
                {"item_id": "flour", "quantity": 2.0, "unit_price": 120.0},
                {"item_id": "oil",   "quantity": 1.0, "unit_price": 300.0},
            ],
        )
        assert result.succeeded
        assert result.data["total_value"] == 540.0

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.orders.place")(
            ctx, items=[{"item_id": "flour", "quantity": 1.0, "unit_price": 100.0}]
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.order.placed" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_item(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.orders.place")(
            ctx, items=[{"item_id": "ghost", "quantity": 1.0, "unit_price": 100.0}]
        )
        assert result.failed
        assert result.constraint_violated == "item_exists"

    @pytest.mark.asyncio
    async def test_fails_zero_quantity(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.orders.place")(
            ctx, items=[{"item_id": "flour", "quantity": 0.0, "unit_price": 100.0}]
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_fails_empty_items(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.orders.place")(ctx, items=[])
        assert result.failed


# ── biashara.orders.fulfill ────────────────────────────────────────────────────

class TestOrdersFulfill:

    async def _state_with_order(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        r = await _op("biashara.orders.place")(
            ctx,
            items=[{"item_id": "flour", "quantity": 5.0, "unit_price": 120.0}],
            customer_party_id="cust1",
        )
        order_id = r.data["order_id"]
        return state, _make_ctx(state), order_id

    @pytest.mark.asyncio
    async def test_moves_order_to_history(self):
        state, ctx, order_id = await self._state_with_order()
        result = await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        assert result.succeeded
        assert state.get("orders.active") == []
        history = state.get("orders.history")
        assert len(history) == 1
        assert history[0]["status"] == "fulfilled"

    @pytest.mark.asyncio
    async def test_deducts_inventory(self):
        state, ctx, order_id = await self._state_with_order()
        before_qty = state.get("inventory.items")["flour"]["qty"]
        await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        after_qty = state.get("inventory.items")["flour"]["qty"]
        assert after_qty == before_qty - 5.0

    @pytest.mark.asyncio
    async def test_appends_sale_movement(self):
        state, ctx, order_id = await self._state_with_order()
        await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        movements = state.get("inventory.movements")
        sale_mvts = [m for m in movements if m["source_type"] == "sale"]
        assert len(sale_mvts) == 1
        assert sale_mvts[0]["delta"] == -5.0

    @pytest.mark.asyncio
    async def test_posts_balanced_journal_entry(self):
        state, ctx, order_id = await self._state_with_order()
        await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        entries = state.get("accounts.journal_entries")
        assert len(entries) == 1
        lines = entries[0]["lines"]
        assert abs(sum(l["debit"] for l in lines) - sum(l["credit"] for l in lines)) < 0.001

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state, ctx, order_id = await self._state_with_order()
        await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.order.fulfilled" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_order(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.orders.fulfill")(ctx, order_id="no-such-order")
        assert result.failed
        assert result.constraint_violated == "order_exists"

    @pytest.mark.asyncio
    async def test_fails_already_fulfilled_order(self):
        state, ctx, order_id = await self._state_with_order()
        await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        ctx2 = _make_ctx(state)
        result = await _op("biashara.orders.fulfill")(ctx2, order_id=order_id)
        assert result.failed


# ── biashara.purchases.record ──────────────────────────────────────────────────

class TestPurchasesRecord:

    @pytest.mark.asyncio
    async def test_appends_to_history(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.purchases.record")(
            ctx, item_id="flour", quantity=20.0, total_cost=1200.0, vendor_party_id="vendor1"
        )
        assert result.succeeded
        assert len(state.get("purchases.history")) == 1

    @pytest.mark.asyncio
    async def test_increases_inventory_qty(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.purchases.record")(ctx, item_id="flour", quantity=20.0, total_cost=1200.0)
        assert state.get("inventory.items")["flour"]["qty"] == 120.0

    @pytest.mark.asyncio
    async def test_posts_balanced_journal(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.purchases.record")(ctx, item_id="flour", quantity=10.0, total_cost=600.0)
        entries = state.get("accounts.journal_entries")
        lines = entries[0]["lines"]
        assert abs(sum(l["debit"] for l in lines) - sum(l["credit"] for l in lines)) < 0.001

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.purchases.record")(ctx, item_id="flour", quantity=10.0, total_cost=600.0)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.purchase.recorded" for e in events)

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_quantity(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.purchases.record")(ctx, item_id="flour", quantity=0.0, total_cost=600.0)
        assert result.failed

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_cost(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.purchases.record")(ctx, item_id="flour", quantity=10.0, total_cost=0.0)
        assert result.failed


# ── biashara.po.raise ──────────────────────────────────────────────────────────

class TestPORaise:

    @pytest.mark.asyncio
    async def test_creates_draft_po(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.po.raise")(
            ctx,
            vendor_party_id="vendor1",
            items=[{"item_id": "flour", "qty_ordered": 100.0, "unit_price": 60.0}],
        )
        assert result.succeeded
        assert result.data["status"] == "draft"
        assert len(state.get("purchase_orders.active")) == 1

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.po.raise")(
            ctx, vendor_party_id="vendor1",
            items=[{"item_id": "flour", "qty_ordered": 10.0, "unit_price": 60.0}],
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.po.raised" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_vendor(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.po.raise")(
            ctx, vendor_party_id="ghost_vendor",
            items=[{"item_id": "flour", "qty_ordered": 10.0, "unit_price": 60.0}],
        )
        assert result.failed
        assert result.constraint_violated == "vendor_exists"

    @pytest.mark.asyncio
    async def test_fails_empty_items(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.po.raise")(ctx, vendor_party_id="vendor1", items=[])
        assert result.failed


# ── biashara.po.receive ────────────────────────────────────────────────────────

class TestPOReceive:

    async def _state_with_po(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        r = await _op("biashara.po.raise")(
            ctx, vendor_party_id="vendor1",
            items=[{"item_id": "flour", "qty_ordered": 50.0, "unit_price": 60.0}],
        )
        return state, _make_ctx(state), r.data["po_id"]

    @pytest.mark.asyncio
    async def test_moves_po_to_history(self):
        state, ctx, po_id = await self._state_with_po()
        result = await _op("biashara.po.receive")(
            ctx, po_id=po_id,
            items_received=[{"item_id": "flour", "qty_received": 50.0}],
        )
        assert result.succeeded
        assert state.get("purchase_orders.active") == []
        history = state.get("purchase_orders.history")
        assert len(history) == 1
        assert history[0]["status"] == "received"

    @pytest.mark.asyncio
    async def test_updates_inventory_qty(self):
        state, ctx, po_id = await self._state_with_po()
        before_qty = state.get("inventory.items")["flour"]["qty"]
        await _op("biashara.po.receive")(
            ctx, po_id=po_id, items_received=[{"item_id": "flour", "qty_received": 50.0}]
        )
        assert state.get("inventory.items")["flour"]["qty"] == before_qty + 50.0

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state, ctx, po_id = await self._state_with_po()
        await _op("biashara.po.receive")(
            ctx, po_id=po_id, items_received=[{"item_id": "flour", "qty_received": 10.0}]
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.po.received" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_po(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.po.receive")(
            ctx, po_id="no-such-po", items_received=[]
        )
        assert result.failed
        assert result.constraint_violated == "po_exists"


# ── biashara.accounts.journal_entry ───────────────────────────────────────────

class TestJournalEntry:

    @pytest.mark.asyncio
    async def test_posts_balanced_entry(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.accounts.journal_entry")(
            ctx,
            lines=[
                {"account_id": "cash",    "debit": 1000.0, "credit": 0.0},
                {"account_id": "revenue",  "debit": 0.0,    "credit": 1000.0},
            ],
            description="Manual entry",
        )
        assert result.succeeded
        assert result.data["total_debit"] == 1000.0
        assert result.data["total_credit"] == 1000.0

    @pytest.mark.asyncio
    async def test_appends_to_journal_entries(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.accounts.journal_entry")(
            ctx,
            lines=[
                {"account_id": "cash",   "debit": 500.0, "credit": 0.0},
                {"account_id": "equity",  "debit": 0.0,   "credit": 500.0},
            ],
        )
        entries = state.get("accounts.journal_entries")
        assert len(entries) == 1
        assert len(entries[0]["lines"]) == 2

    @pytest.mark.asyncio
    async def test_also_appends_to_journal_lines(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.accounts.journal_entry")(
            ctx,
            lines=[
                {"account_id": "a", "debit": 200.0, "credit": 0.0},
                {"account_id": "b", "debit": 0.0,   "credit": 200.0},
            ],
        )
        assert len(state.get("accounts.journal_lines")) == 2

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.accounts.journal_entry")(
            ctx,
            lines=[
                {"account_id": "x", "debit": 100.0, "credit": 0.0},
                {"account_id": "y", "debit": 0.0,   "credit": 100.0},
            ],
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.accounts.journal_entry_posted" for e in events)

    @pytest.mark.asyncio
    async def test_rejects_unbalanced_entry(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.accounts.journal_entry")(
            ctx,
            lines=[
                {"account_id": "cash",   "debit": 1000.0, "credit": 0.0},
                {"account_id": "revenue", "debit": 0.0,    "credit": 999.0},  # off by 1
            ],
        )
        assert result.failed
        assert result.constraint_violated == "journal_balanced"

    @pytest.mark.asyncio
    async def test_no_state_mutation_on_unbalanced(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.accounts.journal_entry")(
            ctx,
            lines=[
                {"account_id": "cash", "debit": 500.0, "credit": 0.0},
                {"account_id": "rev",  "debit": 0.0,   "credit": 100.0},  # unbalanced
            ],
        )
        assert state.get("accounts.journal_entries") == []

    @pytest.mark.asyncio
    async def test_journal_balance_invariant(self):
        """All posted entries must satisfy sum(debit)==sum(credit) — the Biashara invariant."""
        state = _fresh_state()
        ctx = _make_ctx(state)
        # Post three different balanced entries
        for amount in [100.0, 250.0, 999.99]:
            await _op("biashara.accounts.journal_entry")(
                ctx,
                lines=[
                    {"account_id": "a", "debit": amount, "credit": 0.0},
                    {"account_id": "b", "debit": 0.0,    "credit": amount},
                ],
            )
        entries = state.get("accounts.journal_entries")
        for entry in entries:
            td = sum(l["debit"]  for l in entry["lines"])
            tc = sum(l["credit"] for l in entry["lines"])
            assert abs(td - tc) < 0.001, f"Unbalanced entry detected: {entry['entry_id']}"


# ── biashara.tax.calculate_vat ─────────────────────────────────────────────────

class TestCalculateVat:

    async def _state_with_fulfilled_order(self, period="2026-05"):
        from datetime import datetime
        state = _fresh_state()
        ctx = _make_ctx(state)
        # Place + fulfill order
        r = await _op("biashara.orders.place")(
            ctx,
            items=[{"item_id": "flour", "quantity": 10.0, "unit_price": 120.0}],
        )
        order_id = r.data["order_id"]
        await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        # Stamp fulfilled_at with correct period
        history = state.get("orders.history")
        history[0]["fulfilled_at"] = f"{period}-15T10:00:00"
        return state, _make_ctx(state)

    @pytest.mark.asyncio
    async def test_returns_vat_summary_widget(self):
        state, ctx = await self._state_with_fulfilled_order()
        result = await _op("biashara.tax.calculate_vat")(ctx, period="2026-05")
        assert result.succeeded
        assert result.data["type"] == "vat_summary"

    @pytest.mark.asyncio
    async def test_gross_revenue_correct(self):
        state, ctx = await self._state_with_fulfilled_order()
        result = await _op("biashara.tax.calculate_vat")(ctx, period="2026-05")
        # 10 units * 120 = 1200 gross
        assert result.data["gross_revenue"] == 1200.0

    @pytest.mark.asyncio
    async def test_vat_16_percent(self):
        state, ctx = await self._state_with_fulfilled_order()
        result = await _op("biashara.tax.calculate_vat")(ctx, period="2026-05")
        expected_vat = round(1200.0 * 0.16, 2)
        assert abs(result.data["vat_collected"] - expected_vat) < 0.01

    @pytest.mark.asyncio
    async def test_empty_period_returns_zeros(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.tax.calculate_vat")(ctx, period="2099-01")
        assert result.succeeded
        assert result.data["gross_revenue"] == 0.0
        assert result.data["vat_collected"] == 0.0

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.tax.calculate_vat")(ctx, period="2026-05")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.tax.vat_calculated" for e in events)


# ── biashara.assets.depreciate ─────────────────────────────────────────────────

class TestAssetsDepreciate:

    @pytest.mark.asyncio
    async def test_computes_straight_line(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.assets.depreciate")(ctx, asset_id="oven1", period="2026-05")
        assert result.succeeded
        expected = round(120000.0 / 60, 4)
        assert abs(result.data["depreciation_amount"] - expected) < 0.01

    @pytest.mark.asyncio
    async def test_updates_accumulated_depreciation(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.assets.depreciate")(ctx, asset_id="oven1", period="2026-05")
        asset = state.get("assets")["oven1"]
        assert asset["accumulated_depreciation"] > 0.0

    @pytest.mark.asyncio
    async def test_book_value_decreases(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.assets.depreciate")(ctx, asset_id="oven1", period="2026-05")
        asset = state.get("assets")["oven1"]
        assert asset["book_value"] < 120000.0

    @pytest.mark.asyncio
    async def test_posts_balanced_journal(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.assets.depreciate")(ctx, asset_id="oven1", period="2026-05")
        entries = state.get("accounts.journal_entries")
        assert len(entries) == 1
        lines = entries[0]["lines"]
        assert abs(sum(l["debit"] for l in lines) - sum(l["credit"] for l in lines)) < 0.001

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.assets.depreciate")(ctx, asset_id="oven1", period="2026-05")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.asset.depreciated" for e in events)

    @pytest.mark.asyncio
    async def test_fails_unknown_asset(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.assets.depreciate")(ctx, asset_id="ghost_asset", period="2026-05")
        assert result.failed
        assert result.constraint_violated == "asset_exists"


# ── biashara.expenses.record ───────────────────────────────────────────────────

class TestExpensesRecord:

    @pytest.mark.asyncio
    async def test_appends_expense(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.expenses.record")(
            ctx, category="utilities", amount=5000.0, description="Electricity bill"
        )
        assert result.succeeded
        expenses = state.get("expenses")
        assert len(expenses) == 1
        assert expenses[0]["category"] == "utilities"
        assert expenses[0]["amount"] == 5000.0

    @pytest.mark.asyncio
    async def test_posts_balanced_journal(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.expenses.record")(ctx, category="rent", amount=30000.0)
        entries = state.get("accounts.journal_entries")
        lines = entries[0]["lines"]
        assert abs(sum(l["debit"] for l in lines) - sum(l["credit"] for l in lines)) < 0.001

    @pytest.mark.asyncio
    async def test_fires_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await _op("biashara.expenses.record")(ctx, category="transport", amount=1500.0)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.biashara.expense.recorded" for e in events)

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_amount(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.expenses.record")(ctx, category="rent", amount=0.0)
        assert result.failed

    @pytest.mark.asyncio
    async def test_constraint_rejects_empty_category(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.expenses.record")(ctx, category="  ", amount=1000.0)
        assert result.failed
        assert result.constraint_violated == "category_not_empty"

    @pytest.mark.asyncio
    async def test_constraint_rejects_negative_amount(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await _op("biashara.expenses.record")(ctx, category="rent", amount=-500.0)
        assert result.failed


# ── All-four-primitives integration ───────────────────────────────────────────

class TestBiasharaFourPrimitivesIntegration:

    @pytest.mark.asyncio
    async def test_full_restock_and_order_cycle(self):
        """Restock → place order → fulfill → VAT calc."""
        state = _fresh_state()
        ctx = _make_ctx(state)

        # Restock flour
        r = await _op("biashara.inventory.restock")(
            ctx, item_id="flour", quantity=200.0, unit_cost=60.0, supplier_party_id="vendor1"
        )
        assert r.succeeded
        assert state.get("inventory.items")["flour"]["qty"] == 300.0

        # Place order
        r = await _op("biashara.orders.place")(
            ctx, items=[{"item_id": "flour", "quantity": 10.0, "unit_price": 120.0}],
            customer_party_id="cust1",
        )
        assert r.succeeded
        order_id = r.data["order_id"]

        # Fulfill
        r = await _op("biashara.orders.fulfill")(ctx, order_id=order_id)
        assert r.succeeded
        assert state.get("inventory.items")["flour"]["qty"] == 290.0

        # All events: restock + order.placed + order.fulfilled
        events = ctx.events.published_this_context()
        event_names = {e["event_name"] for e in events}
        assert "event.biashara.inventory.restocked" in event_names
        assert "event.biashara.order.placed" in event_names
        assert "event.biashara.order.fulfilled" in event_names

        # All journal entries are balanced
        entries = state.get("accounts.journal_entries")
        for entry in entries:
            lines = entry["lines"]
            assert abs(sum(l["debit"] for l in lines) - sum(l["credit"] for l in lines)) < 0.001

    @pytest.mark.asyncio
    async def test_constraint_failure_no_state_mutation_no_event(self):
        """ConstraintEngine block → state unchanged, no event, no pawa."""
        state = _fresh_state()
        ctx = _make_ctx(state)
        before_entries = len(state.get("accounts.journal_entries"))

        result = await _op("biashara.inventory.restock")(
            ctx, item_id="flour", quantity=-1.0, unit_cost=60.0
        )
        assert result.failed
        assert state.get("inventory.items")["flour"]["qty"] == 100.0
        assert len(state.get("accounts.journal_entries")) == before_entries
        assert ctx.events.published_this_context() == []
        assert await ctx.pawa.get_balance(ctx.user_id) == 0
