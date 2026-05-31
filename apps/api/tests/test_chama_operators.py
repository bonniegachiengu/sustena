"""
tests/test_chama_operators.py

Tests for the chama operator library (Epic 1.1.2).

Covers:
  - Operator registry: all 7 chama operators registered with correct metadata
  - Happy paths for all 7 operators
  - Constraint rejection: chama.loan.disburse with non-PASSED loan status
  - Constraint rejection: chama.fine.record with an invalid reason
  - Integration test: all four primitives fire / DON'T fire correctly on constraint failure

Run with: pytest tests/test_chama_operators.py -v
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration of budget.* AND chama.*


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _fresh_state() -> StateAccessor:
    """Chama sustain state with fund, members, loans, meetings and rules seeded."""
    return StateAccessor({
        "chama": {
            "members": {},
            "loans": [],
            "fines": [],
            "meetings": [],
            "fund": {"total": 0.0},
        },
        "rules": {
            "fine_reasons": ["late_contribution", "absenteeism", "misconduct"],
        },
    })


def _make_ctx(state: StateAccessor, user_id: str = "user-test") -> OperatorContext:
    bus = EventBus(sustain_id="test-chama")
    ledger = PawaLedger()
    return OperatorContext(
        state=state,
        events=bus,
        pawa=ledger,
        sustain_id="test-chama",
        user_id=user_id,
        operative_id=None,
        timestamp=datetime.utcnow(),
    )


async def _record_contribution(state: StateAccessor, member_id: str, amount: float) -> None:
    """Helper: record a contribution via the operator."""
    ctx = _make_ctx(state)
    result = await OPERATOR_REGISTRY["chama.contribution.record"].fn(
        ctx, member_id=member_id, amount=amount, period="2026-05"
    )
    assert result.succeeded, f"contribution.record failed: {result.reason}"


async def _request_loan(state: StateAccessor, member_id: str, amount: float) -> str:
    """Helper: request a loan, return loan_id from state."""
    ctx = _make_ctx(state)
    result = await OPERATOR_REGISTRY["chama.loan.request"].fn(
        ctx, member_id=member_id, amount=amount, purpose="test"
    )
    assert result.is_deferred
    # Return the loan_id from the first item in the loans list
    loans = state.get("chama.loans", [])
    return loans[-1]["id"]  # most recently appended loan


async def _set_loan_status(state: StateAccessor, loan_id: str, status: str) -> None:
    """Directly patch a loan's status — simulates Council resolution.
    Mutates the list item in-place (UUIDs cannot be used as StateAccessor dot-path keys).
    """
    loans: list = state.get("chama.loans", [])
    loan = next((ln for ln in loans if ln.get("id") == loan_id), None)
    if loan is not None:
        loan["status"] = status


def _get_loan(state: StateAccessor, loan_id: str) -> dict | None:
    """Look up a loan dict from the chama.loans list by id."""
    loans: list = state.get("chama.loans", [])
    return next((ln for ln in loans if ln.get("id") == loan_id), None)



def _loan_field(state, loan_id: str, field: str):
    """Read a field from a loan in the list — UUIDs can't be used as dot-path keys."""
    loans = state.get("chama.loans", [])
    loan = next((ln for ln in loans if ln.get("id") == loan_id), None)
    return loan[field] if loan is not None else None


# ── Operator registry ─────────────────────────────────────────────────────────

class TestChamaOperatorRegistry:

    def test_all_chama_operators_registered(self):
        expected = [
            "chama.contribution.record",
            "chama.loan.request",
            "chama.loan.disburse",
            "chama.loan.repay",
            "chama.fine.record",
            "chama.meeting.schedule",
            "chama.dividend.calculate",
        ]
        for name in expected:
            assert name in OPERATOR_REGISTRY, f"'{name}' not in OPERATOR_REGISTRY"

    def test_operators_have_required_metadata(self):
        for name in [
            "chama.contribution.record", "chama.loan.request", "chama.loan.disburse",
            "chama.loan.repay", "chama.fine.record", "chama.meeting.schedule",
            "chama.dividend.calculate",
        ]:
            meta = OPERATOR_REGISTRY[name]
            assert meta.description, f"'{name}' missing description"
            assert meta.author == "sustena_core"
            assert isinstance(meta.pawa_cost, int)
            assert meta.license_tier in ("free", "per_use", "subscription", "one_time")
            assert isinstance(meta.side_effects, list)
            assert isinstance(meta.constraints, list)

    def test_operators_have_ui_schema(self):
        for name in [
            "chama.contribution.record", "chama.loan.request", "chama.loan.disburse",
            "chama.loan.repay", "chama.fine.record", "chama.meeting.schedule",
            "chama.dividend.calculate",
        ]:
            meta = OPERATOR_REGISTRY[name]
            assert meta.ui_schema, f"'{name}' missing ui_schema"
            assert "widget_type" in meta.ui_schema

    def test_mutating_operators_declare_side_effects(self):
        assert "event.chama.contribution_recorded" in \
               OPERATOR_REGISTRY["chama.contribution.record"].side_effects
        assert "event.chama.loan_requested" in \
               OPERATOR_REGISTRY["chama.loan.request"].side_effects
        assert "event.council.proposal_created" in \
               OPERATOR_REGISTRY["chama.loan.request"].side_effects
        assert "event.chama.loan_disbursed" in \
               OPERATOR_REGISTRY["chama.loan.disburse"].side_effects
        assert "event.chama.loan_repayment_recorded" in \
               OPERATOR_REGISTRY["chama.loan.repay"].side_effects
        assert "event.chama.fine_recorded" in \
               OPERATOR_REGISTRY["chama.fine.record"].side_effects
        assert "event.chama.meeting_scheduled" in \
               OPERATOR_REGISTRY["chama.meeting.schedule"].side_effects

    def test_read_only_operator_has_no_side_effects(self):
        assert OPERATOR_REGISTRY["chama.dividend.calculate"].side_effects == []

    def test_fine_record_declares_reason_constraint(self):
        constraints = OPERATOR_REGISTRY["chama.fine.record"].constraints
        assert any("rules.fine_reasons" in c for c in constraints)

    def test_budget_operators_still_registered(self):
        """Regression: importing chama must not disturb the budget registry."""
        for name in ["budget.record_income", "budget.allocate", "budget.spend",
                     "budget.transfer", "budget.summary"]:
            assert name in OPERATOR_REGISTRY


# ── chama.contribution.record ─────────────────────────────────────────────────

class TestChamaContributionRecord:

    @pytest.mark.asyncio
    async def test_credits_member_balance(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=500.0, period="2026-05"
        )
        assert result.succeeded
        assert state.get("chama.members.alice.balance") == 500.0

    @pytest.mark.asyncio
    async def test_increments_fund_total(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=500.0, period="2026-05"
        )
        await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="bob", amount=300.0, period="2026-05"
        )
        assert state.get("chama.fund.total") == 800.0

    @pytest.mark.asyncio
    async def test_appends_contribution_record(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=500.0, period="2026-05"
        )
        contribs = state.get("chama.members.alice.contributions")
        assert len(contribs) == 1
        assert contribs[0]["amount"] == 500.0
        assert contribs[0]["period"] == "2026-05"

    @pytest.mark.asyncio
    async def test_creates_member_if_missing(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="newmember", amount=100.0
        )
        assert state.exists("chama.members.newmember.balance")

    @pytest.mark.asyncio
    async def test_fires_contribution_recorded_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=500.0
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.chama.contribution_recorded" for e in events)

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_amount(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=0.0
        )
        assert result.failed
        assert result.constraint_violated is not None

    @pytest.mark.asyncio
    async def test_constraint_rejects_negative_amount(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=-50.0
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_result_contains_fund_total(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=500.0
        )
        assert result.data["fund_total"] == 500.0
        assert result.data["member_balance"] == 500.0


# ── chama.loan.request ────────────────────────────────────────────────────────

class TestChamaLoanRequest:

    @pytest.mark.asyncio
    async def test_returns_deferred(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.request"].fn(
            ctx, member_id="alice", amount=5000.0, purpose="school fees"
        )
        assert result.is_deferred
        assert result.proposal_id is not None

    @pytest.mark.asyncio
    async def test_writes_loan_to_state_with_in_voting_status(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.request"].fn(
            ctx, member_id="alice", amount=5000.0, purpose="school fees"
        )
        loans = state.get("chama.loans", {})
        assert len(loans) == 1
        loan = loans[-1]  # most recently appended
        assert loan["status"] == "IN_VOTING"
        assert loan["amount"] == 5000.0
        assert loan["member_id"] == "alice"

    @pytest.mark.asyncio
    async def test_fires_loan_requested_and_council_events(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.request"].fn(
            ctx, member_id="alice", amount=5000.0
        )
        events = ctx.events.published_this_context()
        event_names = [e["event_name"] for e in events]
        assert "event.chama.loan_requested" in event_names
        assert "event.council.proposal_created" in event_names

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_amount(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.request"].fn(
            ctx, member_id="alice", amount=0.0
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_no_loan_written_on_constraint_failure(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.request"].fn(
            ctx, member_id="alice", amount=0.0
        )
        assert state.get("chama.loans", []) == []


# ── chama.loan.disburse ───────────────────────────────────────────────────────

class TestChamaLoanDisburse:

    async def _state_with_passed_loan(self) -> tuple[StateAccessor, str]:
        """Return state with a funded pool and a PASSED loan."""
        state = _fresh_state()
        await _record_contribution(state, "alice", 10000.0)
        loan_id = await _request_loan(state, "alice", 5000.0)
        await _set_loan_status(state, loan_id, "PASSED")
        return state, loan_id

    @pytest.mark.asyncio
    async def test_disburses_passed_loan(self):
        state, loan_id = await self._state_with_passed_loan()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        assert result.succeeded
        assert _loan_field(state, loan_id, "status") == "DISBURSED"

    @pytest.mark.asyncio
    async def test_decrements_fund_on_disburse(self):
        state, loan_id = await self._state_with_passed_loan()
        fund_before = state.get("chama.fund.total")
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        assert state.get("chama.fund.total") == fund_before - 5000.0

    @pytest.mark.asyncio
    async def test_sets_disbursed_at_timestamp(self):
        state, loan_id = await self._state_with_passed_loan()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        assert _loan_field(state, loan_id, "disbursed_at") is not None

    @pytest.mark.asyncio
    async def test_fires_loan_disbursed_event(self):
        state, loan_id = await self._state_with_passed_loan()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.chama.loan_disbursed" for e in events)

    @pytest.mark.asyncio
    async def test_constraint_rejects_in_voting_status(self):
        """Runtime constraint: must fail if loan status is IN_VOTING (not PASSED)."""
        state = _fresh_state()
        await _record_contribution(state, "alice", 10000.0)
        loan_id = await _request_loan(state, "alice", 5000.0)
        # Status is still IN_VOTING — NOT patched to PASSED
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        assert result.failed
        assert result.constraint_violated == "loan_status_is_passed"

    @pytest.mark.asyncio
    async def test_constraint_rejects_already_disbursed_loan(self):
        state, loan_id = await self._state_with_passed_loan()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        # Attempt to disburse again — status is now DISBURSED, not PASSED
        ctx2 = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx2, loan_id=loan_id)
        assert result.failed
        assert result.constraint_violated == "loan_status_is_passed"

    @pytest.mark.asyncio
    async def test_constraint_rejects_failed_status(self):
        state = _fresh_state()
        await _record_contribution(state, "alice", 10000.0)
        loan_id = await _request_loan(state, "alice", 5000.0)
        await _set_loan_status(state, loan_id, "FAILED")
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        assert result.failed
        assert result.constraint_violated == "loan_status_is_passed"

    @pytest.mark.asyncio
    async def test_fails_on_unknown_loan_id(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.disburse"].fn(
            ctx, loan_id="nonexistent-id"
        )
        assert result.failed
        assert result.constraint_violated == "loan_exists"

    @pytest.mark.asyncio
    async def test_state_unchanged_on_constraint_failure(self):
        """Fund must not decrease when constraint blocks disbursal."""
        state = _fresh_state()
        await _record_contribution(state, "alice", 10000.0)
        loan_id = await _request_loan(state, "alice", 5000.0)
        # Leave IN_VOTING — constraint will fail
        fund_before = state.get("chama.fund.total")
        status_before = _loan_field(state, loan_id, "status")
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        assert state.get("chama.fund.total") == fund_before
        assert _loan_field(state, loan_id, "status") == status_before


# ── chama.loan.repay ──────────────────────────────────────────────────────────

class TestChamaLoanRepay:

    async def _state_with_disbursed_loan(self) -> tuple[StateAccessor, str]:
        state = _fresh_state()
        await _record_contribution(state, "alice", 10000.0)
        loan_id = await _request_loan(state, "alice", 5000.0)
        await _set_loan_status(state, loan_id, "PASSED")
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        return state, loan_id

    @pytest.mark.asyncio
    async def test_records_partial_repayment(self):
        state, loan_id = await self._state_with_disbursed_loan()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.repay"].fn(
            ctx, loan_id=loan_id, amount=1000.0
        )
        assert result.succeeded
        assert _loan_field(state, loan_id, "repaid") == 1000.0
        assert result.data["fully_repaid"] is False

    @pytest.mark.asyncio
    async def test_increments_fund_on_repayment(self):
        state, loan_id = await self._state_with_disbursed_loan()
        fund_before = state.get("chama.fund.total")
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.repay"].fn(ctx, loan_id=loan_id, amount=1000.0)
        assert state.get("chama.fund.total") == fund_before + 1000.0

    @pytest.mark.asyncio
    async def test_marks_loan_repaid_when_fully_settled(self):
        state, loan_id = await self._state_with_disbursed_loan()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.repay"].fn(ctx, loan_id=loan_id, amount=5000.0)
        assert _loan_field(state, loan_id, "status") == "REPAID"

    @pytest.mark.asyncio
    async def test_fires_repayment_event(self):
        state, loan_id = await self._state_with_disbursed_loan()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.loan.repay"].fn(ctx, loan_id=loan_id, amount=1000.0)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.chama.loan_repayment_recorded" for e in events)

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_amount(self):
        state, loan_id = await self._state_with_disbursed_loan()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.loan.repay"].fn(
            ctx, loan_id=loan_id, amount=0.0
        )
        assert result.failed


# ── chama.fine.record ─────────────────────────────────────────────────────────

class TestChamaFineRecord:

    @pytest.mark.asyncio
    async def test_records_valid_fine(self):
        state = _fresh_state()
        await _record_contribution(state, "alice", 500.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=100.0, reason="late_contribution"
        )
        assert result.succeeded

    @pytest.mark.asyncio
    async def test_decrements_member_balance(self):
        state = _fresh_state()
        await _record_contribution(state, "alice", 500.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=100.0, reason="absenteeism"
        )
        assert state.get("chama.members.alice.balance") == 400.0

    @pytest.mark.asyncio
    async def test_appends_to_global_fines_log(self):
        state = _fresh_state()
        await _record_contribution(state, "alice", 500.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=50.0, reason="misconduct"
        )
        fines = state.get("chama.fines", [])
        assert len(fines) == 1
        assert fines[0]["reason"] == "misconduct"

    @pytest.mark.asyncio
    async def test_fires_fine_recorded_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=50.0, reason="late_contribution"
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.chama.fine_recorded" for e in events)

    @pytest.mark.asyncio
    async def test_constraint_rejects_invalid_reason(self):
        """ConstraintEngine must block a reason not in rules.fine_reasons."""
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=100.0, reason="bad_hair_day"
        )
        assert result.failed
        assert result.constraint_violated is not None

    @pytest.mark.asyncio
    async def test_constraint_rejects_empty_reason(self):
        """Empty string is not in rules.fine_reasons — must be blocked."""
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=100.0, reason=""
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_constraint_accepts_all_valid_reasons(self):
        """All entries in rules.fine_reasons must pass the constraint."""
        state = _fresh_state()
        valid_reasons = state.get("rules.fine_reasons", [])
        assert len(valid_reasons) > 0, "test needs at least one valid fine reason"
        for reason in valid_reasons:
            s = _fresh_state()
            ctx = _make_ctx(s)
            result = await OPERATOR_REGISTRY["chama.fine.record"].fn(
                ctx, member_id="alice", amount=50.0, reason=reason
            )
            assert result.succeeded, f"Valid reason '{reason}' was rejected"

    @pytest.mark.asyncio
    async def test_constraint_rejects_zero_amount(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=0.0, reason="late_contribution"
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_state_unchanged_on_constraint_failure(self):
        """Neither balance nor fines log must change when constraint blocks."""
        state = _fresh_state()
        await _record_contribution(state, "alice", 500.0)
        balance_before = state.get("chama.members.alice.balance")
        fines_before = len(state.get("chama.fines", []))
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=50.0, reason="invalid_reason"
        )
        assert state.get("chama.members.alice.balance") == balance_before
        assert len(state.get("chama.fines", [])) == fines_before

    @pytest.mark.asyncio
    async def test_no_event_on_constraint_failure(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=50.0, reason="invalid_reason"
        )
        assert ctx.events.published_this_context() == []


# ── chama.meeting.schedule ────────────────────────────────────────────────────

class TestChamaMeetingSchedule:

    @pytest.mark.asyncio
    async def test_schedules_meeting(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.meeting.schedule"].fn(
            ctx, date="2026-06-15", agenda=["Elections", "Loan review"]
        )
        assert result.succeeded
        assert result.data["date"] == "2026-06-15"
        assert result.data["status"] == "scheduled"

    @pytest.mark.asyncio
    async def test_appends_to_meetings_list(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.meeting.schedule"].fn(
            ctx, date="2026-06-15", agenda=["Elections"]
        )
        meetings = state.get("chama.meetings", [])
        assert len(meetings) == 1
        assert meetings[0]["date"] == "2026-06-15"
        assert "Elections" in meetings[0]["agenda"]

    @pytest.mark.asyncio
    async def test_multiple_meetings_accumulate(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.meeting.schedule"].fn(ctx, date="2026-06-15")
        await OPERATOR_REGISTRY["chama.meeting.schedule"].fn(ctx, date="2026-07-15")
        assert len(state.get("chama.meetings", [])) == 2

    @pytest.mark.asyncio
    async def test_fires_meeting_scheduled_event(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.meeting.schedule"].fn(ctx, date="2026-06-15")
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.chama.meeting_scheduled" for e in events)

    @pytest.mark.asyncio
    async def test_empty_agenda_allowed(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.meeting.schedule"].fn(
            ctx, date="2026-06-15"
        )
        assert result.succeeded
        assert result.data["agenda"] == []


# ── chama.dividend.calculate ──────────────────────────────────────────────────

class TestChamaDividendCalculate:

    @pytest.mark.asyncio
    async def test_returns_dividend_table_widget(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        assert result.succeeded
        assert result.data["type"] == "dividend_table"

    @pytest.mark.asyncio
    async def test_reflects_member_balances(self):
        state = _fresh_state()
        await _record_contribution(state, "alice", 3000.0)
        await _record_contribution(state, "bob", 1000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        members = result.data["data"]["members"]
        alice = next(m for m in members if m["member_id"] == "alice")
        bob = next(m for m in members if m["member_id"] == "bob")
        assert alice["balance"] == 3000.0
        assert bob["balance"] == 1000.0

    @pytest.mark.asyncio
    async def test_dividend_shares_sum_to_100_pct(self):
        state = _fresh_state()
        await _record_contribution(state, "alice", 3000.0)
        await _record_contribution(state, "bob", 1000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        members = result.data["data"]["members"]
        total_pct = sum(m["dividend_share_pct"] for m in members)
        assert abs(total_pct - 100.0) < 0.1

    @pytest.mark.asyncio
    async def test_proportional_split_correct(self):
        """alice has 75%, bob has 25%."""
        state = _fresh_state()
        await _record_contribution(state, "alice", 3000.0)
        await _record_contribution(state, "bob", 1000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        members = result.data["data"]["members"]
        alice = next(m for m in members if m["member_id"] == "alice")
        bob = next(m for m in members if m["member_id"] == "bob")
        assert abs(alice["dividend_share_pct"] - 75.0) < 0.01
        assert abs(bob["dividend_share_pct"] - 25.0) < 0.01

    @pytest.mark.asyncio
    async def test_members_sorted_by_balance_desc(self):
        state = _fresh_state()
        await _record_contribution(state, "alice", 3000.0)
        await _record_contribution(state, "bob", 1000.0)
        await _record_contribution(state, "carol", 5000.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        members = result.data["data"]["members"]
        balances = [m["balance"] for m in members]
        assert balances == sorted(balances, reverse=True)

    @pytest.mark.asyncio
    async def test_empty_chama_handled(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        data = result.data["data"]
        assert data["member_count"] == 0
        assert data["fund_total"] == 0.0

    @pytest.mark.asyncio
    async def test_no_events_fired(self):
        """dividend.calculate is read-only — must fire no events."""
        state = _fresh_state()
        await _record_contribution(state, "alice", 500.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        assert ctx.events.published_this_context() == []

    @pytest.mark.asyncio
    async def test_pawa_not_deducted(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        balance_before = await ctx.pawa.get_balance(ctx.user_id)
        await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        assert await ctx.pawa.get_balance(ctx.user_id) == balance_before


# ── All-four-primitives integration ──────────────────────────────────────────

class TestAllFourPrimitivesIntegration:
    """
    Integration tests verifying all four primitives fire together on the happy path,
    and that they ALL remain clean (no side-effects) when a constraint blocks execution.
    """

    @pytest.mark.asyncio
    async def test_full_cycle_contribution_record(self):
        """contribution.record: ConstraintEngine ✓ PawaLedger ✓ StateAccessor ✓ EventBus ✓"""
        state = _fresh_state()
        ctx = _make_ctx(state)

        result = await OPERATOR_REGISTRY["chama.contribution.record"].fn(
            ctx, member_id="alice", amount=500.0, period="2026-05"
        )

        # ConstraintEngine passed
        assert result.succeeded

        # StateAccessor mutated
        assert state.get("chama.members.alice.balance") == 500.0
        assert state.get("chama.fund.total") == 500.0

        # EventBus fired exactly one event
        events = ctx.events.published_this_context()
        assert len(events) == 1
        assert events[0]["event_name"] == "event.chama.contribution_recorded"

        # PawaLedger unchanged (0 pawa cost)
        assert await ctx.pawa.get_balance(ctx.user_id) == 0

    @pytest.mark.asyncio
    async def test_full_cycle_fine_record(self):
        """fine.record: ConstraintEngine ✓ PawaLedger ✓ StateAccessor ✓ EventBus ✓"""
        state = _fresh_state()
        await _record_contribution(state, "alice", 500.0)
        ctx = _make_ctx(state)

        result = await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=50.0, reason="absenteeism"
        )

        assert result.succeeded
        assert state.get("chama.members.alice.balance") == 450.0
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.chama.fine_recorded" for e in events)
        assert await ctx.pawa.get_balance(ctx.user_id) == 0

    @pytest.mark.asyncio
    async def test_constraint_failure_leaves_all_primitives_clean_disburse(self):
        """
        chama.loan.disburse with non-PASSED loan:
          ConstraintEngine  → fails (runtime check)
          StateAccessor     → no mutations (fund unchanged, status unchanged)
          EventBus          → no events fired
          PawaLedger        → no pawa deducted
        """
        state = _fresh_state()
        await _record_contribution(state, "alice", 10000.0)
        loan_id = await _request_loan(state, "alice", 5000.0)
        # Loan is still IN_VOTING — do NOT patch to PASSED

        fund_before = state.get("chama.fund.total")
        status_before = _loan_field(state, loan_id, "status")
        pawa_before = None

        ctx = _make_ctx(state)
        pawa_before = await ctx.pawa.get_balance(ctx.user_id)

        result = await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)

        # ConstraintEngine: blocked
        assert result.failed
        assert result.constraint_violated == "loan_status_is_passed"

        # StateAccessor: unchanged
        assert state.get("chama.fund.total") == fund_before
        assert _loan_field(state, loan_id, "status") == status_before

        # EventBus: no events
        assert ctx.events.published_this_context() == []

        # PawaLedger: no deduction
        assert await ctx.pawa.get_balance(ctx.user_id) == pawa_before

    @pytest.mark.asyncio
    async def test_constraint_failure_leaves_all_primitives_clean_fine_invalid_reason(self):
        """
        chama.fine.record with invalid reason:
          ConstraintEngine  → fails (params.reason IN rules.fine_reasons)
          StateAccessor     → no mutations
          EventBus          → no events fired
          PawaLedger        → no pawa deducted
        """
        state = _fresh_state()
        await _record_contribution(state, "alice", 500.0)

        balance_before = state.get("chama.members.alice.balance")
        fines_before = list(state.get("chama.fines", []))

        ctx = _make_ctx(state)
        pawa_before = await ctx.pawa.get_balance(ctx.user_id)

        result = await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="alice", amount=50.0, reason="wearing_sandals"
        )

        # ConstraintEngine: blocked
        assert result.failed
        assert result.constraint_violated is not None

        # StateAccessor: unchanged
        assert state.get("chama.members.alice.balance") == balance_before
        assert len(state.get("chama.fines", [])) == len(fines_before)

        # EventBus: no events
        assert ctx.events.published_this_context() == []

        # PawaLedger: no deduction
        assert await ctx.pawa.get_balance(ctx.user_id) == pawa_before

    @pytest.mark.asyncio
    async def test_full_chama_flow(self):
        """
        End-to-end: contributions → loan request → disburse → repay → dividend.
        Verifies state coherence across the full operator chain.
        """
        state = _fresh_state()
        ctx = _make_ctx(state)

        # Three members contribute
        for member, amount in [("alice", 5000.0), ("bob", 3000.0), ("carol", 2000.0)]:
            r = await OPERATOR_REGISTRY["chama.contribution.record"].fn(
                ctx, member_id=member, amount=amount, period="2026-05"
            )
            assert r.succeeded

        assert state.get("chama.fund.total") == 10000.0

        # alice requests a loan
        r = await OPERATOR_REGISTRY["chama.loan.request"].fn(
            ctx, member_id="alice", amount=3000.0, purpose="medical"
        )
        assert r.is_deferred
        loans = state.get("chama.loans", {})
        loan_id = loans[-1]["id"]

        # Council approves
        await _set_loan_status(state, loan_id, "PASSED")

        # Disburse
        r = await OPERATOR_REGISTRY["chama.loan.disburse"].fn(ctx, loan_id=loan_id)
        assert r.succeeded
        assert state.get("chama.fund.total") == 7000.0

        # alice repays
        r = await OPERATOR_REGISTRY["chama.loan.repay"].fn(
            ctx, loan_id=loan_id, amount=3000.0
        )
        assert r.succeeded
        assert r.data["fully_repaid"] is True
        assert state.get("chama.fund.total") == 10000.0

        # Fine bob
        r = await OPERATOR_REGISTRY["chama.fine.record"].fn(
            ctx, member_id="bob", amount=200.0, reason="late_contribution"
        )
        assert r.succeeded

        # Schedule a meeting
        r = await OPERATOR_REGISTRY["chama.meeting.schedule"].fn(
            ctx, date="2026-06-01", agenda=["Dividend payout", "New loan requests"]
        )
        assert r.succeeded

        # Dividend calculation reflects reality
        r = await OPERATOR_REGISTRY["chama.dividend.calculate"].fn(ctx)
        assert r.succeeded
