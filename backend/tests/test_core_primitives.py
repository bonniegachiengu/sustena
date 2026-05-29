"""
tests/test_core_primitives.py

Tests for the four core primitives:
  StateAccessor, ConstraintEngine, EventBus, PawaLedger

Run with: pytest tests/test_core_primitives.py -v
"""

import pytest
from sustena.core.state import StateAccessor, StatePathError, StateValueError
from sustena.core.constraints import ConstraintEngine
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger


# ═══════════════════════════════════════════════════════════════════════════════
# StateAccessor
# ═══════════════════════════════════════════════════════════════════════════════

class TestStateAccessor:

    def _make_state(self) -> StateAccessor:
        return StateAccessor({
            "finances": {
                "liquid": {"balance": 50000.0},
                "pockets": {
                    "food": {"allocated": 5000.0, "spent": 1200.0},
                    "rent": {"allocated": 20000.0, "spent": 0.0},
                }
            },
            "staff": {
                "roster": [
                    {"id": "s1", "name": "Alice", "status": "active"},
                    {"id": "s2", "name": "Bob",   "status": "inactive"},
                ]
            },
            "orders": []
        })

    def test_get_nested_value(self):
        s = self._make_state()
        assert s.get("finances.liquid.balance") == 50000.0

    def test_get_missing_returns_default(self):
        s = self._make_state()
        assert s.get("finances.nonexistent") is None
        assert s.get("finances.nonexistent", 99) == 99

    def test_get_strict_raises_on_missing(self):
        s = self._make_state()
        with pytest.raises(StatePathError):
            s.get_strict("finances.missing.path")

    def test_set_existing_value(self):
        s = self._make_state()
        s.set("finances.liquid.balance", 45000.0)
        assert s.get("finances.liquid.balance") == 45000.0

    def test_set_creates_intermediate_keys(self):
        s = self._make_state()
        s.set("new_section.subsection.value", 42)
        assert s.get("new_section.subsection.value") == 42

    def test_increment(self):
        s = self._make_state()
        new_val = s.increment("finances.liquid.balance", 10000.0)
        assert new_val == 60000.0
        assert s.get("finances.liquid.balance") == 60000.0

    def test_decrement_valid(self):
        s = self._make_state()
        new_val = s.decrement("finances.liquid.balance", 5000.0)
        assert new_val == 45000.0

    def test_decrement_below_zero_raises(self):
        s = self._make_state()
        with pytest.raises(StateValueError):
            s.decrement("finances.liquid.balance", 100000.0)

    def test_decrement_allow_negative(self):
        s = self._make_state()
        new_val = s.decrement("finances.liquid.balance", 100000.0, allow_negative=True)
        assert new_val == -50000.0

    def test_exists_true(self):
        s = self._make_state()
        assert s.exists("finances.liquid.balance") is True

    def test_exists_false(self):
        s = self._make_state()
        assert s.exists("finances.nonexistent") is False

    def test_append_to_list(self):
        s = self._make_state()
        item_id = s.append("orders", {"item": "chapati_pilau", "qty": 2})
        orders = s.get("orders")
        assert len(orders) == 1
        assert orders[0]["id"] == item_id
        assert orders[0]["item"] == "chapati_pilau"

    def test_append_auto_assigns_id(self):
        s = self._make_state()
        item_id = s.append("orders", {"item": "biryani"})
        assert item_id is not None
        assert len(item_id) == 36  # UUID format

    def test_remove_from_list(self):
        s = self._make_state()
        s.remove("staff.roster", "s1")
        roster = s.get("staff.roster")
        assert len(roster) == 1
        assert roster[0]["id"] == "s2"

    def test_remove_nonexistent_raises(self):
        s = self._make_state()
        with pytest.raises(StatePathError):
            s.remove("staff.roster", "nonexistent_id")

    def test_snapshot_is_deep_copy(self):
        s = self._make_state()
        snap = s.snapshot()
        s.set("finances.liquid.balance", 0)
        assert snap["finances"]["liquid"]["balance"] == 50000.0  # Unchanged

    def test_mutations_tracked(self):
        s = self._make_state()
        s.set("finances.liquid.balance", 45000.0)
        s.increment("finances.pockets.food.spent", 500.0)
        mutations = s.mutations()
        assert len(mutations) == 2
        assert mutations[0]["path"] == "finances.liquid.balance"
        assert mutations[0]["old"] == 50000.0
        assert mutations[0]["new"] == 45000.0

    def test_increment_non_numeric_raises(self):
        s = self._make_state()
        with pytest.raises(StateValueError):
            s.increment("staff.roster[0].name", 1)


# ═══════════════════════════════════════════════════════════════════════════════
# ConstraintEngine
# ═══════════════════════════════════════════════════════════════════════════════

class TestConstraintEngine:

    def _engine(self):
        return ConstraintEngine()

    def _state(self, data: dict | None = None) -> StateAccessor:
        return StateAccessor(data or {
            "finances": {"liquid": {"balance": 50000.0}},
            "order": {"status": "pending", "amount": 1000.0},
            "inventory": {"oil": {"quantity": 5}},
        })

    def test_simple_greater_than_pass(self):
        ok, _ = self._engine().evaluate("finances.liquid.balance > 0", self._state())
        assert ok

    def test_simple_greater_than_fail(self):
        ok, reason = self._engine().evaluate("finances.liquid.balance > 100000", self._state())
        assert not ok
        assert "finances.liquid.balance" in reason

    def test_greater_than_or_equal_pass(self):
        ok, _ = self._engine().evaluate("finances.liquid.balance >= 50000", self._state())
        assert ok

    def test_equals_pass(self):
        ok, _ = self._engine().evaluate('order.status == "pending"', self._state())
        assert ok

    def test_equals_fail(self):
        ok, _ = self._engine().evaluate('order.status == "confirmed"', self._state())
        assert not ok

    def test_not_equals(self):
        ok, _ = self._engine().evaluate('order.status != "cancelled"', self._state())
        assert ok

    def test_params_reference(self):
        ok, _ = self._engine().evaluate(
            "finances.liquid.balance >= params.amount",
            self._state(),
            params={"amount": 5000}
        )
        assert ok

    def test_params_reference_fail(self):
        ok, _ = self._engine().evaluate(
            "finances.liquid.balance >= params.amount",
            self._state(),
            params={"amount": 100000}
        )
        assert not ok

    def test_in_operator_pass(self):
        ok, _ = self._engine().evaluate(
            'order.status IN ["pending", "confirmed"]',
            self._state()
        )
        assert ok

    def test_in_operator_fail(self):
        ok, _ = self._engine().evaluate(
            'order.status IN ["shipped", "delivered"]',
            self._state()
        )
        assert not ok

    def test_not_in_operator(self):
        ok, _ = self._engine().evaluate(
            'order.status NOT IN ["cancelled", "refunded"]',
            self._state()
        )
        assert ok

    def test_and_both_pass(self):
        ok, _ = self._engine().evaluate(
            "finances.liquid.balance > 0 AND order.amount > 0",
            self._state()
        )
        assert ok

    def test_and_one_fails(self):
        ok, reason = self._engine().evaluate(
            "finances.liquid.balance > 0 AND order.amount > 99999",
            self._state()
        )
        assert not ok

    def test_or_one_passes(self):
        ok, _ = self._engine().evaluate(
            'order.status == "confirmed" OR order.status == "pending"',
            self._state()
        )
        assert ok

    def test_or_both_fail(self):
        ok, _ = self._engine().evaluate(
            'order.status == "cancelled" OR order.status == "refunded"',
            self._state()
        )
        assert not ok

    def test_not_operator(self):
        ok, _ = self._engine().evaluate(
            'NOT order.status == "cancelled"',
            self._state()
        )
        assert ok

    def test_empty_constraint_passes(self):
        ok, _ = self._engine().evaluate("", self._state())
        assert ok

    def test_evaluate_all_pass(self):
        ok, _ = self._engine().evaluate_all([
            "finances.liquid.balance > 0",
            "order.amount > 0",
        ], self._state())
        assert ok

    def test_evaluate_all_stops_on_first_fail(self):
        ok, reason = self._engine().evaluate_all([
            "finances.liquid.balance > 0",
            "finances.liquid.balance > 999999",
            "order.amount > 0",
        ], self._state())
        assert not ok
        assert "999999" in reason


# ═══════════════════════════════════════════════════════════════════════════════
# EventBus
# ═══════════════════════════════════════════════════════════════════════════════

class TestEventBus:

    @pytest.mark.asyncio
    async def test_publish_valid_event(self):
        bus = EventBus(sustain_id="test-sustain-1")
        event_id = await bus.publish("event.finances.income_received", {"amount": 5000})
        assert event_id is not None
        assert len(event_id) == 36  # UUID

    @pytest.mark.asyncio
    async def test_publish_invalid_name_raises(self):
        bus = EventBus(sustain_id="test-sustain-1")
        with pytest.raises(ValueError):
            await bus.publish("bad_event_name", {})

    @pytest.mark.asyncio
    async def test_publish_two_segment_name_raises(self):
        bus = EventBus(sustain_id="test-sustain-1")
        with pytest.raises(ValueError):
            await bus.publish("event.finances", {})

    @pytest.mark.asyncio
    async def test_subscriber_called(self):
        bus = EventBus(sustain_id="test-sustain-2")
        received = []

        async def handler(payload):
            received.append(payload)

        bus.subscribe("event.orders.order_placed", handler)
        await bus.publish("event.orders.order_placed", {"order_id": "o123", "amount": 250})
        assert len(received) == 1
        assert received[0]["order_id"] == "o123"

    @pytest.mark.asyncio
    async def test_wildcard_subscriber(self):
        bus = EventBus(sustain_id="test-sustain-3")
        received = []

        async def handler(payload):
            received.append(payload)

        bus.subscribe("event.finances.*", handler)
        await bus.publish("event.finances.income_received", {"amount": 1000})
        await bus.publish("event.finances.pocket_spent", {"pocket": "food"})
        assert len(received) == 2

    @pytest.mark.asyncio
    async def test_published_this_context(self):
        bus = EventBus(sustain_id="test-sustain-4")
        await bus.publish("event.orders.order_placed", {"order_id": "o1"})
        await bus.publish("event.orders.order_fulfilled", {"order_id": "o1"})
        published = bus.published_this_context()
        assert len(published) == 2
        assert published[0]["event_name"] == "event.orders.order_placed"


# ═══════════════════════════════════════════════════════════════════════════════
# PawaLedger
# ═══════════════════════════════════════════════════════════════════════════════

class TestPawaLedger:

    @pytest.mark.asyncio
    async def test_initial_balance_zero(self):
        ledger = PawaLedger()
        balance = await ledger.get_balance("user-123")
        assert balance == 0

    @pytest.mark.asyncio
    async def test_credit_increases_balance(self):
        ledger = PawaLedger()
        new_balance = await ledger.credit("user-1", None, 100, "onboarding_grant")
        assert new_balance == 100
        assert await ledger.get_balance("user-1") == 100

    @pytest.mark.asyncio
    async def test_deduct_sufficient_balance(self):
        ledger = PawaLedger()
        await ledger.credit("user-1", None, 100, "test")
        ok = await ledger.deduct("user-1", None, 50, "operator_call")
        assert ok is True
        assert await ledger.get_balance("user-1") == 50

    @pytest.mark.asyncio
    async def test_deduct_insufficient_returns_false(self):
        ledger = PawaLedger()
        await ledger.credit("user-1", None, 10, "test")
        ok = await ledger.deduct("user-1", None, 50, "operator_call")
        assert ok is False
        assert await ledger.get_balance("user-1") == 10  # Unchanged

    @pytest.mark.asyncio
    async def test_deduct_zero_always_succeeds(self):
        ledger = PawaLedger()
        ok = await ledger.deduct("user-1", None, 0, "free_op")
        assert ok is True

    @pytest.mark.asyncio
    async def test_transfer_success(self):
        ledger = PawaLedger()
        await ledger.credit("alice", None, 100, "test")
        ok = await ledger.transfer("alice", "bob", 40, "tip")
        assert ok is True
        assert await ledger.get_balance("alice") == 60
        assert await ledger.get_balance("bob") == 40

    @pytest.mark.asyncio
    async def test_transfer_insufficient_fails(self):
        ledger = PawaLedger()
        await ledger.credit("alice", None, 10, "test")
        ok = await ledger.transfer("alice", "bob", 50, "tip")
        assert ok is False
        assert await ledger.get_balance("alice") == 10
        assert await ledger.get_balance("bob") == 0

    @pytest.mark.asyncio
    async def test_charge_royalty_split(self):
        from sustena.core.pawa import NETWORK_TREASURY_ID
        ledger = PawaLedger()
        await ledger.credit("caller", None, 100, "test")
        ok = await ledger.charge("caller", None, 100, "contributor-1")
        assert ok is True
        assert await ledger.get_balance("caller") == 0
        assert await ledger.get_balance("contributor-1") == 70   # 70%
        # Treasury gets 20% + 5% referrer (no referrer) + 5% validator = 30%
        assert await ledger.get_balance(NETWORK_TREASURY_ID) == 30

    @pytest.mark.asyncio
    async def test_charge_insufficient_blocked(self):
        ledger = PawaLedger()
        await ledger.credit("caller", None, 5, "test")
        ok = await ledger.charge("caller", None, 100, "contributor-1")
        assert ok is False
        assert await ledger.get_balance("caller") == 5  # Unchanged
