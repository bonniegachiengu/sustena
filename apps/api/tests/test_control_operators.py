"""
tests/test_control_operators.py

Tests for the control.* operator library.
Sprint 3 criteria:
  - control.execute_approved runs a PASSED proposal and updates state
  - control.rollback restores state to prior snapshot
"""

import json
import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
from sustena.operators.control_ops import clear_snapshot_registry
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _budget_state(liquid: float = 50000.0) -> dict:
    return {
        "finances": {
            "liquid": {"balance": liquid},
            "pockets": {},
            "income": {"sources": [], "monthly_total": 0.0},
        },
        "council_proposals": [],
        "council_votes": [],
    }


def _make_ctx(state_data: dict | None = None) -> OperatorContext:
    return OperatorContext(
        state=StateAccessor(state_data or _budget_state()),
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="user-test",
        timestamp=datetime.utcnow(),
    )


def _add_proposal(ctx: OperatorContext, proposal_id: str, status: str, operator_name: str, params: dict) -> None:
    """Helper: inject a proposal directly into state."""
    proposals = ctx.state.get("council_proposals", []) or []
    proposals.append({
        "id": proposal_id,
        "status": status,
        "operator_name": operator_name,
        "input_json": json.dumps(params),
        "proposed_by": "mentor",
    })
    ctx.state.set("council_proposals", proposals)


@pytest.fixture(autouse=True)
def clean_snapshots():
    """Reset snapshot registry between tests."""
    clear_snapshot_registry()
    yield
    clear_snapshot_registry()


# ── control.execute_approved ───────────────────────────────────────────────────

class TestControlExecuteApproved:

    @pytest.mark.asyncio
    async def test_executes_passed_proposal_and_updates_state(self):
        """Sprint 3 criterion: runs a PASSED proposal and updates state."""
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        _add_proposal(
            ctx, "prop-1", "PASSED", "budget.record_income",
            {"amount": 10000.0, "source": "salary"}
        )

        result = await OPERATOR_REGISTRY["control.execute_approved"].fn(
            ctx, proposal_id="prop-1"
        )

        assert result.succeeded
        # State updated: liquid balance increased by 10000
        new_balance = ctx.state.get("finances.liquid.balance")
        assert new_balance == 60000.0

    @pytest.mark.asyncio
    async def test_updates_proposal_status_to_executed(self):
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        _add_proposal(
            ctx, "prop-2", "PASSED", "budget.record_income",
            {"amount": 5000.0}
        )

        await OPERATOR_REGISTRY["control.execute_approved"].fn(
            ctx, proposal_id="prop-2"
        )

        proposals = ctx.state.get("council_proposals", [])
        executed = next(p for p in proposals if p["id"] == "prop-2")
        assert executed["status"] == "EXECUTED"

    @pytest.mark.asyncio
    async def test_fails_on_non_passed_proposal(self):
        ctx = _make_ctx()
        _add_proposal(
            ctx, "prop-3", "IN_VOTING", "budget.record_income", {"amount": 100}
        )
        result = await OPERATOR_REGISTRY["control.execute_approved"].fn(
            ctx, proposal_id="prop-3"
        )
        assert result.failed
        assert "PASSED" in result.reason

    @pytest.mark.asyncio
    async def test_fails_on_missing_proposal(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["control.execute_approved"].fn(
            ctx, proposal_id="no-such-proposal"
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_saves_pre_execution_snapshot(self):
        from sustena.operators.control_ops import _latest_version
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        _add_proposal(
            ctx, "prop-4", "PASSED", "budget.record_income", {"amount": 1000.0}
        )
        assert _latest_version("test-sustain") == 0
        await OPERATOR_REGISTRY["control.execute_approved"].fn(ctx, proposal_id="prop-4")
        assert _latest_version("test-sustain") == 1

    @pytest.mark.asyncio
    async def test_publishes_event(self):
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        _add_proposal(
            ctx, "prop-5", "PASSED", "budget.record_income", {"amount": 1000.0}
        )
        await OPERATOR_REGISTRY["control.execute_approved"].fn(ctx, proposal_id="prop-5")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.control.proposal_executed" for e in events)


# ── control.rollback ───────────────────────────────────────────────────────────

class TestControlRollback:

    @pytest.mark.asyncio
    async def test_rollback_restores_prior_state(self):
        """Sprint 3 criterion: control.rollback restores state to prior snapshot."""
        ctx = _make_ctx(_budget_state(liquid=50000.0))
        _add_proposal(
            ctx, "prop-r1", "PASSED", "budget.record_income", {"amount": 20000.0}
        )

        # Execute proposal — saves snapshot version 1 first, then adds 20000
        await OPERATOR_REGISTRY["control.execute_approved"].fn(ctx, proposal_id="prop-r1")
        assert ctx.state.get("finances.liquid.balance") == 70000.0

        # Rollback to version 1 (pre-execution state with 50000)
        rollback_result = await OPERATOR_REGISTRY["control.rollback"].fn(
            ctx, to_version=1
        )
        assert rollback_result.succeeded
        assert ctx.state.get("finances.liquid.balance") == 50000.0

    @pytest.mark.asyncio
    async def test_rollback_without_version_uses_latest(self):
        ctx = _make_ctx(_budget_state(liquid=30000.0))
        _add_proposal(
            ctx, "prop-r2", "PASSED", "budget.record_income", {"amount": 5000.0}
        )
        await OPERATOR_REGISTRY["control.execute_approved"].fn(ctx, proposal_id="prop-r2")
        # Balance is now 35000; rollback to most recent (version 1 = 30000)
        result = await OPERATOR_REGISTRY["control.rollback"].fn(ctx)
        assert result.succeeded
        assert ctx.state.get("finances.liquid.balance") == 30000.0

    @pytest.mark.asyncio
    async def test_rollback_with_no_snapshots_returns_fail(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["control.rollback"].fn(ctx)
        assert result.failed

    @pytest.mark.asyncio
    async def test_rollback_invalid_version_returns_fail(self):
        ctx = _make_ctx(_budget_state(liquid=10000.0))
        _add_proposal(ctx, "prop-r3", "PASSED", "budget.record_income", {"amount": 1.0})
        await OPERATOR_REGISTRY["control.execute_approved"].fn(ctx, proposal_id="prop-r3")
        result = await OPERATOR_REGISTRY["control.rollback"].fn(ctx, to_version=99)
        assert result.failed

    @pytest.mark.asyncio
    async def test_rollback_publishes_event(self):
        ctx = _make_ctx(_budget_state(liquid=10000.0))
        _add_proposal(ctx, "prop-r4", "PASSED", "budget.record_income", {"amount": 1.0})
        await OPERATOR_REGISTRY["control.execute_approved"].fn(ctx, proposal_id="prop-r4")
        await OPERATOR_REGISTRY["control.rollback"].fn(ctx, to_version=1)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.control.state_rolled_back" for e in events)


# ── Metadata ──────────────────────────────────────────────────────────────────

class TestControlMetadata:

    def test_execute_approved_protocol(self):
        assert OPERATOR_REGISTRY["control.execute_approved"].protocol == "rpc"

    def test_rollback_protocol(self):
        assert OPERATOR_REGISTRY["control.rollback"].protocol == "rpc"
