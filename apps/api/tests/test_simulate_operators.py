"""
tests/test_simulate_operators.py

Tests for the simulate.* operator library.
Sprint 3 criterion: simulate.fork + simulate.run_path with 3-step budget sequence
returns correct SimulationPath; DB unchanged.
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
from sustena.operators.simulate_ops import clear_fork_registry
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _budget_state(liquid: float = 50000.0) -> StateAccessor:
    return StateAccessor({
        "finances": {
            "liquid": {"balance": liquid},
            "pockets": {},
            "income": {"sources": [], "monthly_total": 0.0},
        }
    })


def _make_ctx(state: StateAccessor | None = None) -> OperatorContext:
    state = state or _budget_state()
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="user-test",
        timestamp=datetime.utcnow(),
    )


@pytest.fixture(autouse=True)
def clean_forks():
    """Reset fork registry between tests."""
    clear_fork_registry()
    yield
    clear_fork_registry()


# ── simulate.fork ──────────────────────────────────────────────────────────────

class TestSimulateFork:

    @pytest.mark.asyncio
    async def test_fork_returns_fork_id(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        assert result.succeeded
        assert "fork_id" in result.data
        assert len(result.data["fork_id"]) == 36  # UUID

    @pytest.mark.asyncio
    async def test_fork_does_not_mutate_original_state(self):
        state = _budget_state(liquid=50000.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        # Original state unchanged
        assert ctx.state.get("finances.liquid.balance") == 50000.0


# ── simulate.run_path ──────────────────────────────────────────────────────────

class TestSimulateRunPath:

    @pytest.mark.asyncio
    async def test_three_step_budget_sequence(self):
        """Sprint 3 criterion: 3-step budget sequence returns correct SimulationPath."""
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        fork_id = fork_result.data["fork_id"]

        sequence = [
            {"operator": "budget.record_income", "params": {"amount": 10000.0, "source": "salary"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 5000.0}},
            {"operator": "budget.spend", "params": {"pocket_name": "food", "amount": 2000.0}},
        ]

        result = await OPERATOR_REGISTRY["simulate.run_path"].fn(
            ctx, fork_id=fork_id, operator_sequence=sequence
        )

        assert result.succeeded
        assert result.data["steps_run"] == 3
        assert result.data["steps_succeeded"] == 3

    @pytest.mark.asyncio
    async def test_run_path_db_unchanged(self):
        """The original state must not be mutated by simulation."""
        initial_balance = 50000.0
        ctx = _make_ctx(_budget_state(liquid=initial_balance))
        fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        fork_id = fork_result.data["fork_id"]

        await OPERATOR_REGISTRY["simulate.run_path"].fn(
            ctx,
            fork_id=fork_id,
            operator_sequence=[
                {"operator": "budget.record_income", "params": {"amount": 99000.0}},
            ],
        )

        # Original ctx.state unchanged
        assert ctx.state.get("finances.liquid.balance") == initial_balance

    @pytest.mark.asyncio
    async def test_failed_step_does_not_advance_fork_state(self):
        ctx = _make_ctx(_budget_state(liquid=100.0))
        fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        fork_id = fork_result.data["fork_id"]

        # This will fail — can't allocate 5000 from 100
        result = await OPERATOR_REGISTRY["simulate.run_path"].fn(
            ctx,
            fork_id=fork_id,
            operator_sequence=[
                {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 5000.0}},
            ],
        )

        assert result.succeeded
        steps = result.data["steps"]
        assert steps[0]["status"] == "failed"
        # Forked balance still 100 (step failed, state not advanced)
        final_balance = result.data["final_state"]["finances"]["liquid"]["balance"]
        assert final_balance == 100.0

    @pytest.mark.asyncio
    async def test_unknown_fork_id_returns_fail(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["simulate.run_path"].fn(
            ctx, fork_id="nonexistent-fork", operator_sequence=[]
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_unknown_operator_in_sequence_recorded(self):
        ctx = _make_ctx()
        fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        fork_id = fork_result.data["fork_id"]

        result = await OPERATOR_REGISTRY["simulate.run_path"].fn(
            ctx,
            fork_id=fork_id,
            operator_sequence=[{"operator": "no.such.operator", "params": {}}],
        )
        assert result.succeeded
        assert result.data["steps"][0]["status"] == "failed"


# ── simulate.score ─────────────────────────────────────────────────────────────

class TestSimulateScore:

    @pytest.mark.asyncio
    async def test_score_after_run_path(self):
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        fork_id = fork_result.data["fork_id"]

        # Run a light spend
        await OPERATOR_REGISTRY["simulate.run_path"].fn(
            ctx,
            fork_id=fork_id,
            operator_sequence=[
                {"operator": "budget.record_income", "params": {"amount": 5000.0}},
                {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 3000.0}},
                {"operator": "budget.spend", "params": {"pocket_name": "food", "amount": 1000.0}},
            ],
        )

        score_result = await OPERATOR_REGISTRY["simulate.score"].fn(
            ctx, fork_id=fork_id, goal_metric="minimize_budget_deviation"
        )

        assert score_result.succeeded
        assert 0.0 <= score_result.data["score"] <= 1.0
        assert score_result.data["goal_metric"] == "minimize_budget_deviation"
        assert score_result.data["interpretation"] in ("excellent", "good", "fair", "poor")

    @pytest.mark.asyncio
    async def test_maximize_liquid_balance_metric(self):
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        fork_id = fork_result.data["fork_id"]

        result = await OPERATOR_REGISTRY["simulate.score"].fn(
            ctx, fork_id=fork_id, goal_metric="maximize_liquid_balance"
        )
        assert result.succeeded
        assert result.data["score"] == 1.0  # 50k/50k = 1.0

    @pytest.mark.asyncio
    async def test_unknown_goal_metric(self):
        ctx = _make_ctx()
        fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx)
        fork_id = fork_result.data["fork_id"]

        result = await OPERATOR_REGISTRY["simulate.score"].fn(
            ctx, fork_id=fork_id, goal_metric="no_such_metric"
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_unknown_fork_id_returns_fail(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["simulate.score"].fn(
            ctx, fork_id="bad-fork", goal_metric="maximize_savings_rate"
        )
        assert result.failed


# ── Metadata ──────────────────────────────────────────────────────────────────

class TestSimulateMetadata:

    def test_all_protocols_are_rpc(self):
        for name in ["simulate.fork", "simulate.run_path", "simulate.score"]:
            assert OPERATOR_REGISTRY[name].protocol == "rpc"
