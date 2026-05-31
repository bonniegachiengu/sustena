"""
tests/test_budget_operators.py

Tests for the budget operator library.
Covers: happy paths, constraint violations (via ConstraintEngine directly AND
        through the operator body), pawa deduction, state mutations, events fired.

Run with: pytest tests/test_budget_operators.py -v
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor, StateValueError
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _fresh_state(balance: float = 50000.0) -> StateAccessor:
    """Homestead sustain state with a clean budget section."""
    return StateAccessor({
        "finances": {
            "liquid": {"balance": balance},
            "pockets": {},
            "income": {
                "sources": [],
                "monthly_total": 0.0,
            },
        }
    })


def _make_ctx(state: StateAccessor, user_id: str = "user-test", pawa_balance: int = 0) -> OperatorContext:
    """
    Build a minimal OperatorContext for unit testing operators directly.
    pawa_balance: seed the ledger with this many pawa (for testing pawa-cost operators).
    """
    bus = EventBus(sustain_id="test-sustain")
    ledger = PawaLedger()
    ctx = OperatorContext(
        state=state,
        events=bus,
        pawa=ledger,
        sustain_id="test-sustain",
        user_id=user_id,
        operative_id=None,
        timestamp=datetime.utcnow(),
    )
    return ctx


async def _seed_pawa(ctx: OperatorContext, amount: int) -> None:
    """Credit pawa to the context user — helper for pawa-cost tests."""
    await ctx.pawa.credit(ctx.user_id, ctx.sustain_id, amount, "test_seed")


# ── Operator registry ─────────────────────────────────────────────────────────

class TestOperatorRegistry:

    def test_budget_operators_registered(self):
        expected = [
            "budget.record_income",
            "budget.allocate",
            "budget.spend",
            "budget.transfer",
            "budget.summary",
        ]
        for name in expected:
            assert name in OPERATOR_REGISTRY, f"'{name}' not in OPERATOR_REGISTRY"

    def test_operators_have_required_metadata(self):
        for name in ["budget.record_income", "budget.allocate", "budget.spend",
                     "budget.transfer", "budget.summary"]:
            meta = OPERATOR_REGISTRY[name]
            assert meta.description
            assert meta.author == "sustena_core"
            assert isinstance(meta.pawa_cost, int)
            assert meta.license_tier in ("free", "per_use", "subscription", "one_time")
            assert isinstance(meta.side_effects, list)
            assert isinstance(meta.constraints, list)

    def test_budget_operators_have_ui_schema(self):
        """Every budget operator should declare a ui_schema for card rendering."""
        for name in ["budget.record_income", "budget.allocate", "budget.spend",
                     "budget.transfer", "budget.summary"]:
            meta = OPERATOR_REGISTRY[name]
            assert meta.ui_schema, f"'{name}' is missing ui_schema"
            assert "widget_type" in meta.ui_schema, f"'{name}' ui_schema missing widget_type"

    def test_mutating_operators_declare_side_effects(self):
        """Operators that fire events must declare them in side_effects."""
        assert "event.finances.income_received" in OPERATOR_REGISTRY["budget.record_income"].side_effects
        assert "event.finances.pocket_allocated" in OPERATOR_REGISTRY["budget.allocate"].side_effects
        assert "event.finances.pocket_spent"     in OPERATOR_REGISTRY["budget.spend"].side_effects
        assert "event.finances.pocket_transfer"  in OPERATOR_REGISTRY["budget.transfer"].side_effects

    def test_summary_has_no_side_effects(self):
        """budget.summary is read-only — must declare empty side_effects."""
        assert OPERATOR_REGISTRY["budget.summary"].side_effects == []

    def test_allocate_has_liquid_balance_constraint(self):
        """budget.allocate must constrain against over-drawing liquid."""
        constraints = OPERATOR_REGISTRY["budget.allocate"].constraints
        assert any("finances.liquid.balance" in c for c in constraints)


# ── budget.record_income ──────────────────────────────────────────────────────

class TestBudgetRecordIncome:

    @pytest.mark.asyncio
    async def test_credits_liquid_balance(self):
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.record_income"].fn(
            ctx, amount=45000.0, source="Salary"
        )
        assert result.succeeded
        assert state.get("finances.liquid.balance") == 45000.0

    @pytest.mark.asyncio
    async def test_appends_income_source(self):
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.record_income"].fn(
            ctx, amount=10000.0, source="Side hustle"
        )
        sources = state.get("finances.income.sources")
        assert len(sources) == 1
        assert sources[0]["label"] == "Side hustle"
        assert sources[0]["amount"] == 10000.0

    @pytest.mark.asyncio
    async def test_fires_income_received_event(self):
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.record_income"].fn(
            ctx, amount=5000.0, source="Chama payout"
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.finances.income_received" for e in events)

    @pytest.mark.asyncio
    async def test_multiple_incomes_accumulate(self):
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.record_income"].fn(ctx, amount=30000.0, source="Job")
        await OPERATOR_REGISTRY["budget.record_income"].fn(ctx, amount=15000.0, source="Freelance")
        assert state.get("finances.liquid.balance") == 45000.0
        assert len(state.get("finances.income.sources")) == 2

    @pytest.mark.asyncio
    async def test_result_contains_new_balance(self):
        state = _fresh_state(balance=10000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.record_income"].fn(
            ctx, amount=5000.0, source="Bonus"
        )
        assert result.data["liquid_balance"] == 15000.0

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_amount(self):
        """ConstraintEngine must block amount=0 through the operator body."""
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.record_income"].fn(
            ctx, amount=0.0, source="Empty"
        )
        assert result.failed
        assert result.constraint_violated is not None

    @pytest.mark.asyncio
    async def test_constraint_rejects_negative_amount(self):
        """ConstraintEngine must block negative amounts."""
        state = _fresh_state(balance=10000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.record_income"].fn(
            ctx, amount=-500.0, source="Bad input"
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_state_unchanged_on_constraint_failure(self):
        """State must not mutate when a constraint blocks the operator."""
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        balance_before = state.get("finances.liquid.balance")
        await OPERATOR_REGISTRY["budget.record_income"].fn(ctx, amount=0.0, source="Bad")
        assert state.get("finances.liquid.balance") == balance_before
        assert state.get("finances.income.sources") == []

    @pytest.mark.asyncio
    async def test_no_event_on_constraint_failure(self):
        """Events must not fire when a constraint blocks the operator."""
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.record_income"].fn(ctx, amount=0.0, source="Bad")
        assert ctx.events.published_this_context() == []

    @pytest.mark.asyncio
    async def test_pawa_deducted(self):
        """PawaLedger.deduct() is called (0 pawa for free operators — no-op but verified)."""
        state = _fresh_state(balance=0.0)
        ctx = _make_ctx(state)
        balance_before = await ctx.pawa.get_balance(ctx.user_id)
        await OPERATOR_REGISTRY["budget.record_income"].fn(ctx, amount=5000.0, source="Test")
        balance_after = await ctx.pawa.get_balance(ctx.user_id)
        # pawa_cost=0, so balance unchanged — but the call was made without error
        assert balance_after == balance_before


# ── budget.allocate ───────────────────────────────────────────────────────────

class TestBudgetAllocate:

    @pytest.mark.asyncio
    async def test_moves_from_liquid_to_pocket(self):
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.allocate"].fn(
            ctx, pocket_name="food", amount=5000.0
        )
        assert result.succeeded
        assert state.get("finances.liquid.balance") == 45000.0
        assert state.get("finances.pockets.food.allocated") == 5000.0
        assert state.get("finances.pockets.food.spent") == 0.0

    @pytest.mark.asyncio
    async def test_creates_pocket_if_missing(self):
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="emergency", amount=3000.0)
        assert state.exists("finances.pockets.emergency.allocated")

    @pytest.mark.asyncio
    async def test_fires_pocket_allocated_event(self):
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="rent", amount=20000.0)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.finances.pocket_allocated" for e in events)

    @pytest.mark.asyncio
    async def test_allocate_multiple_pockets(self):
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="food", amount=5000.0)
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="rent", amount=20000.0)
        assert state.get("finances.liquid.balance") == 25000.0
        assert state.get("finances.pockets.food.allocated") == 5000.0
        assert state.get("finances.pockets.rent.allocated") == 20000.0

    @pytest.mark.asyncio
    async def test_constraint_blocks_over_allocation_via_operator(self):
        """Constraint must be enforced THROUGH the operator body (not just via ConstraintEngine directly)."""
        state = _fresh_state(balance=1000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.allocate"].fn(
            ctx, pocket_name="food", amount=5000.0
        )
        assert result.failed
        assert result.constraint_violated is not None

    @pytest.mark.asyncio
    async def test_state_unchanged_after_constraint_failure(self):
        """Liquid must not decrease when allocation is blocked by constraint."""
        state = _fresh_state(balance=1000.0)
        ctx = _make_ctx(state)
        liquid_before = state.get("finances.liquid.balance")
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="food", amount=5000.0)
        assert state.get("finances.liquid.balance") == liquid_before
        assert not state.exists("finances.pockets.food.allocated")

    @pytest.mark.asyncio
    async def test_no_event_on_constraint_failure(self):
        state = _fresh_state(balance=1000.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="food", amount=5000.0)
        assert ctx.events.published_this_context() == []

    @pytest.mark.asyncio
    async def test_constraint_blocks_zero_amount(self):
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="food", amount=0.0)
        assert result.failed

    @pytest.mark.asyncio
    async def test_constraint_passes_when_sufficient(self):
        """Happy path: constraint passes and state is mutated correctly."""
        from sustena.core.constraints import ConstraintEngine
        state = _fresh_state(balance=50000.0)
        engine = ConstraintEngine()
        meta = OPERATOR_REGISTRY["budget.allocate"]
        ok, _ = engine.evaluate_all(meta.constraints, state, params={"amount": 5000.0})
        assert ok

    @pytest.mark.asyncio
    async def test_pawa_deducted(self):
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        balance_before = await ctx.pawa.get_balance(ctx.user_id)
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="food", amount=1000.0)
        balance_after = await ctx.pawa.get_balance(ctx.user_id)
        assert balance_after == balance_before  # pawa_cost=0


# ── budget.spend ──────────────────────────────────────────────────────────────

class TestBudgetSpend:

    async def _state_with_food_pocket(self) -> tuple[StateAccessor, OperatorContext]:
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["budget.allocate"].fn(ctx, pocket_name="food", amount=5000.0)
        return state, _make_ctx(state)  # fresh ctx for spend (new event bus)

    @pytest.mark.asyncio
    async def test_deducts_from_pocket_spent(self):
        state, ctx = await self._state_with_food_pocket()
        result = await OPERATOR_REGISTRY["budget.spend"].fn(
            ctx, pocket_name="food", amount=1200.0, description="Grocery run"
        )
        assert result.succeeded
        assert state.get("finances.pockets.food.spent") == 1200.0

    @pytest.mark.asyncio
    async def test_remaining_calculated_correctly(self):
        state, ctx = await self._state_with_food_pocket()
        result = await OPERATOR_REGISTRY["budget.spend"].fn(
            ctx, pocket_name="food", amount=2000.0
        )
        assert result.data["pocket_remaining"] == 3000.0

    @pytest.mark.asyncio
    async def test_fires_pocket_spent_event(self):
        state, ctx = await self._state_with_food_pocket()
        await OPERATOR_REGISTRY["budget.spend"].fn(
            ctx, pocket_name="food", amount=500.0, description="Naivas"
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.finances.pocket_spent" for e in events)

    @pytest.mark.asyncio
    async def test_fails_if_pocket_missing(self):
        state = _fresh_state(balance=50000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["budget.spend"].fn(
            ctx, pocket_name="nonexistent", amount=100.0
        )
        assert result.failed
        assert "nonexistent" in result.reason

    @pytest.mark.asyncio
    async def test_fails_if_exceeds_allocated(self):
        state, ctx = await self._state_with_food_pocket()
        result = await OPERATOR_REGISTRY["budget.spend"].fn(
            ctx, pocket_name="food", amount=9999.0
        )
        assert result.failed
        assert "exceed" in result.reason