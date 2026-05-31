"""
tests/test_chama_secretary.py

Test suite for Epic 1.5b — ChamaSecretaryOperative and Chama Council governance.

Coverage:
  1.  match_contribution — correct BillRefNumber → operator call spec; wrong format → None
  2.  compute_trust_score — 100% on-time 12 months; 2 misses 24 months
  3.  generate_reminder — mock _call_claude; tone guidance changes with miss_count
  4.  evaluate_loan_request — eligible → proposal; above ratio → reject; low trust → reject
  5.  weekly_audit_summary — ResponseWidget with correct member data
  6.  Chama council financial: 6/10 YES → PASSED (60% quorum)
  7.  Chama council financial: 5/10 votes cast → IN_VOTING (below quorum)
  8.  Chama council financial: majority NO → FAILED
  9.  Chama council structural: 9/10 YES → PASSED
  10. Chama council structural: 8/10 YES → not PASSED
  11. Chama council structural: Secretary veto → FAILED
  12. Chama council structural: any member NO → FAILED
  13. chama.rotation.advance — happy path
  14. chama.rotation.advance — insufficient pool → fail
  15. chama.rotation.advance — no pending entry → fail
  16. Secretary deliberate → always ABSTAIN

Run with: pytest tests/test_chama_secretary.py -v
"""

import asyncio
import pytest
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
from sustena.core.council import CouncilSession
from sustena.operatives.chama_secretary import ChamaSecretaryOperative
from sustena.operatives.base import VoteChoice
import sustena.operators  # triggers auto-registration of all operators


# ── Shared helpers ────────────────────────────────────────────────────────────

def _fresh_state(members=None) -> StateAccessor:
    data = {
        "members": members or {},
        "rotation": {"schedule": [], "current_period": "2026-05"},
        "pool": {
            "balance": 100_000.0,
            "total_contributed_mtd": 0.0,
            "total_disbursed_mtd": 0.0,
            "reserve": 5_000.0,
        },
        "loans": {"active": [], "repayment_history": []},
        "proposals": [],
        "rules": {
            "min_contribution": 500,
            "max_contribution": 50_000,
            "loan_interest_rate": 0.07,
            "max_loan_ratio": 0.5,
            "fine_reasons": ["late_contribution", "absent_meeting", "misconduct", "late_loan_repayment"],
            "fine_amounts": {"late_contribution": 100, "absent_meeting": 50, "misconduct": 500, "late_loan_repayment": 200},
            "quorum_threshold": 0.6,
            "rotation_frequency": "monthly",
            "trust_score_weights": {"on_time_rate": 0.6, "longevity": 0.2, "dispute_history": 0.2},
        },
        "fines": [],
        "meetings": [],
        "council_proposals": [],
        "council_votes": [],
        "accounts": {"journal_entries": []},
    }
    return StateAccessor(data)


def _rotation_state(pool_balance: float = 100_000.0) -> StateAccessor:
    return StateAccessor({
        "members": {},
        "rotation": {
            "schedule": [
                {"period": "2026-05", "recipient_member_id": "m001", "amount": 10_000.0, "status": "pending"},
                {"period": "2026-06", "recipient_member_id": "m002", "amount": 10_000.0, "status": "pending"},
            ],
            "current_period": "2026-05",
        },
        "pool": {"balance": pool_balance, "total_contributed_mtd": 50_000.0, "total_disbursed_mtd": 0.0, "reserve": 5_000.0},
        "loans": {"active": [], "repayment_history": []},
        "proposals": [],
        "rules": {
            "min_contribution": 500, "max_contribution": 50_000,
            "loan_interest_rate": 0.07, "max_loan_ratio": 0.5,
            "fine_reasons": ["late_contribution"], "fine_amounts": {},
            "quorum_threshold": 0.6, "rotation_frequency": "monthly",
        },
        "fines": [], "meetings": [],
        "accounts": {"journal_entries": []},
        "council_proposals": [], "council_votes": [],
    })


def _mock_client(response_text: str = "mocked") -> MagicMock:
    mock_response = MagicMock()
    mock_response.content = [MagicMock(text=response_text)]
    mock_response.usage = MagicMock(input_tokens=50, output_tokens=30)
    client = MagicMock()
    client.messages.create = AsyncMock(return_value=mock_response)
    return client


def _make_secretary(state: StateAccessor, claude_response: str = "mocked") -> ChamaSecretaryOperative:
    return ChamaSecretaryOperative(config={}, state_accessor=state, claude_client=_mock_client(claude_response))


def _make_ctx(state: StateAccessor) -> OperatorContext:
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-chama"),
        pawa=PawaLedger(),
        sustain_id="test-chama",
        user_id="secretary-001",
        operative_id="chama_secretary",
        timestamp=datetime.now(timezone.utc),
    )


# ══════════════════════════════════════════════════════════════════════════════
# 1. match_contribution
# ══════════════════════════════════════════════════════════════════════════════

class TestMatchContribution:

    def test_correct_bill_ref_returns_operator_call(self):
        sec = _make_secretary(_fresh_state())
        result = sec.match_contribution({
            "BillRefNumber": "KILIMANI-m001-2026-05",
            "TransAmount": "2500",
            "MSISDN": "+254712345678",
        })
        assert result is not None
        assert result["operator"] == "chama.contribution.record"
        assert result["params"]["member_id"] == "m001"
        assert result["params"]["amount"] == 2500.0
        assert result["params"]["period"] == "2026-05"
        assert result["_matched_chama_id"] == "KILIMANI"

    def test_wrong_format_returns_none(self):
        sec = _make_secretary(_fresh_state())
        for ref in [
            "KILIMANI-m001",           # missing period
            "m001-2026-05",            # missing chama_id
            "KILIMANI-m001-May-2026",  # wrong period format
            "",                         # empty
            "no-dashes-here",          # no period segment
            "CHAMA-member-26-05",      # period not 4-digit year
        ]:
            result = sec.match_contribution({"BillRefNumber": ref, "TransAmount": "500", "MSISDN": "+254700000000"})
            assert result is None, f"Expected None for {ref!r}, got {result}"

    def test_float_trans_amount(self):
        sec = _make_secretary(_fresh_state())
        result = sec.match_contribution({
            "BillRefNumber": "CHAMA-alice-2026-06",
            "TransAmount": 1500.75,
            "MSISDN": "+254799999999",
        })
        assert result is not None
        assert result["params"]["amount"] == 1500.75
        assert result["params"]["member_id"] == "alice"
        assert result["params"]["period"] == "2026-06"


# ══════════════════════════════════════════════════════════════════════════════
# 2. compute_trust_score
# ══════════════════════════════════════════════════════════════════════════════

class TestComputeTrustScore:

    def _member(self, join_date, contributions, disputes=0):
        return {
            "name": "Test", "phone": "+2547", "join_date": join_date,
            "status": "active", "contribution_history": contributions,
            "loan_history": [], "trust_score": 0.0, "dispute_count": disputes,
        }

    def test_perfect_score_12_months(self):
        contributions = [
            {"id": str(i), "amount": 1000, "period": f"2025-{i:02d}",
             "on_time": True, "recorded_at": f"2025-{i:02d}-01T10:00:00+00:00"}
            for i in range(1, 13)
        ]
        members = {"m001": self._member("2025-05-01", contributions)}
        state = _fresh_state(members=members)
        sec = _make_secretary(state)
        score = sec.compute_trust_score("m001", state)
        # on_time: 12/12*0.6=0.6, longevity: ~0.1, dispute: 0.2 → ~0.9
        assert 0.85 <= score <= 1.0, f"Expected ~0.9, got {score}"

    def test_score_with_misses_24_months(self):
        contributions = [
            {"id": str(i), "amount": 1000, "period": f"2024-{i:02d}",
             "on_time": i not in (3, 7), "recorded_at": f"2024-{i:02d}-01T10:00:00+00:00"}
            for i in range(1, 13)
        ]
        members = {"m002": self._member("2024-05-01", contributions)}
        state = _fresh_state(members=members)
        sec = _make_secretary(state)
        score = sec.compute_trust_score("m002", state)
        # on_time: 10/12*0.6 ≈ 0.5, longevity: ~0.2, dispute: 0.2 → ~0.9
        expected_on_time = (10 / 12) * 0.6
        assert abs(score - (expected_on_time + 0.2 + 0.2)) < 0.05, f"Got {score}"
        assert score < 0.95

    def test_unknown_member_returns_zero(self):
        sec = _make_secretary(_fresh_state())
        assert sec.compute_trust_score("nonexistent", _fresh_state()) == 0.0

    def test_new_member_no_contributions(self):
        members = {"new": self._member("2026-05-01", [])}
        state = _fresh_state(members=members)
        sec = _make_secretary(state)
        score = sec.compute_trust_score("new", state)
        assert 0.0 <= score <= 0.25

    def test_dispute_reduces_score(self):
        c = [{"id": "1", "amount": 1000, "period": "2026-01", "on_time": True,
              "recorded_at": "2026-01-01T00:00:00+00:00"}]
        state_clean = _fresh_state(members={"m": self._member("2026-01-01", c, disputes=0)})
        state_bad   = _fresh_state(members={"m": self._member("2026-01-01", c, disputes=1)})
        sec_clean = _make_secretary(state_clean)
        sec_bad   = _make_secretary(state_bad)
        assert sec_bad.compute_trust_score("m", state_bad) < sec_clean.compute_trust_score("m", state_clean)


# ══════════════════════════════════════════════════════════════════════════════
# 3. generate_reminder
# ══════════════════════════════════════════════════════════════════════════════

class TestGenerateReminder:

    def _member_state(self, name, trust_score=0.8):
        return _fresh_state(members={
            "m001": {"name": name, "phone": "+254700000001", "join_date": "2025-01-01",
                     "status": "active", "contribution_history": [],
                     "trust_score": trust_score, "dispute_count": 0},
        })

    def _capture_client(self, reply: str):
        captured = {}
        async def mock_create(**kwargs):
            captured["system"] = kwargs.get("system", "")
            r = MagicMock()
            r.content = [MagicMock(text=reply)]
            r.usage = MagicMock(input_tokens=30, output_tokens=10)
            return r
        client = MagicMock()
        client.messages.create = AsyncMock(side_effect=mock_create)
        return client, captured

    @pytest.mark.asyncio
    async def test_warm_tone_miss_count_zero(self):
        state = self._member_state("Achieng")
        client, captured = self._capture_client("Hey Achieng!")
        sec = ChamaSecretaryOperative(config={}, state_accessor=state, claude_client=client)
        result = await sec.generate_reminder("m001", days_to_deadline=5, miss_count=0)
        assert result
        assert any(w in captured["system"].lower() for w in ["warm", "friendly", "encouraging"])

    @pytest.mark.asyncio
    async def test_firm_tone_miss_count_2(self):
        state = self._member_state("Kamau", trust_score=0.5)
        client, captured = self._capture_client("Kamau, two missed contributions.")
        sec = ChamaSecretaryOperative(config={}, state_accessor=state, claude_client=client)
        result = await sec.generate_reminder("m001", days_to_deadline=3, miss_count=2)
        assert result
        assert any(w in captured["system"].lower() for w in ["firm", "respectful", "pattern", "trust score"])

    @pytest.mark.asyncio
    async def test_escalation_tone_miss_count_5(self):
        state = self._member_state("Njeri", trust_score=0.2)
        client, captured = self._capture_client("Njeri, your standing is at risk.")
        sec = ChamaSecretaryOperative(config={}, state_accessor=state, claude_client=client)
        result = await sec.generate_reminder("m001", days_to_deadline=1, miss_count=5)
        assert result
        assert any(w in captured["system"].lower() for w in ["escalation", "formal", "serious", "at risk"])
        assert "never" in captured["system"].lower() or "NEVER" in captured["system"]


# ══════════════════════════════════════════════════════════════════════════════
# 4. evaluate_loan_request
# ══════════════════════════════════════════════════════════════════════════════

class TestEvaluateLoanRequest:

    def _good_member(self, trust_score=0.75):
        return {
            "name": "Wanjiku", "phone": "+254711111111", "join_date": "2024-01-01",
            "status": "active",
            "contribution_history": [
                {"id": str(i), "amount": 1000, "period": f"2025-{i:02d}",
                 "on_time": True, "recorded_at": f"2025-{i:02d}-01T00:00:00+00:00"}
                for i in range(1, 7)
            ],
            "trust_score": trust_score, "loan_history": [], "dispute_count": 0,
        }

    @pytest.mark.asyncio
    async def test_eligible_loan_returns_disburse_proposal(self):
        members = {"m001": self._good_member(trust_score=0.75)}
        state = _fresh_state(members=members)
        claude_resp = '{"recommendation": "approve", "rationale": "Good history.", "risk_level": "low"}'
        sec = _make_secretary(state, claude_response=claude_resp)
        proposal = await sec.evaluate_loan_request(
            {"member_id": "m001", "amount": 30_000.0, "purpose": "school fees"}, state
        )
        assert proposal.operator_name == "chama.loan.disburse"
        assert proposal.simulation_results["eligible"] is True
        assert proposal.input_params["member_id"] == "m001"

    @pytest.mark.asyncio
    async def test_loan_above_max_ratio_rejected(self):
        state = _fresh_state(members={"m001": self._good_member(trust_score=0.75)})
        sec = _make_secretary(state)
        proposal = await sec.evaluate_loan_request(
            {"member_id": "m001", "amount": 60_000.0, "purpose": "business"}, state
        )
        assert proposal.operator_name == "chama.loan.reject"
        assert proposal.simulation_results["rejection_reason"] == "exceeds_max_loan_ratio"
        sec.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_low_trust_score_rejected(self):
        state = _fresh_state(members={"m001": self._good_member(trust_score=0.3)})
        sec = _make_secretary(state)
        proposal = await sec.evaluate_loan_request(
            {"member_id": "m001", "amount": 10_000.0, "purpose": "medical"}, state
        )
        assert proposal.operator_name == "chama.loan.reject"
        assert proposal.simulation_results["rejection_reason"] == "trust_score_too_low"
        sec.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_borderline_trust_0_4_is_eligible(self):
        state = _fresh_state(members={"m001": self._good_member(trust_score=0.4)})
        sec = _make_secretary(state, claude_response='{"recommendation": "caution", "rationale": "Borderline.", "risk_level": "medium"}')
        proposal = await sec.evaluate_loan_request(
            {"member_id": "m001", "amount": 5_000.0, "purpose": "rent"}, state
        )
        assert proposal.operator_name == "chama.loan.disburse"
        assert proposal.simulation_results["eligible"] is True


# ══════════════════════════════════════════════════════════════════════════════
# 5. weekly_audit_summary
# ══════════════════════════════════════════════════════════════════════════════

class TestWeeklyAuditSummary:

    def test_returns_widget_with_table(self):
        members = {
            "m001": {"name": "Achieng", "phone": "+1", "join_date": "2025-01-01",
                     "status": "active", "trust_score": 0.8, "dispute_count": 0,
                     "contribution_history": [
                         {"id": "c1", "amount": 1000, "period": "2026-05",
                          "on_time": True, "recorded_at": "2026-05-01T10:00:00+00:00"}
                     ], "loan_history": []},
            "m002": {"name": "Kamau", "phone": "+2", "join_date": "2025-01-01",
                     "status": "active", "trust_score": 0.6, "dispute_count": 0,
                     "contribution_history": [], "loan_history": []},
        }
        state = _fresh_state(members=members)
        widget = _make_secretary(state).weekly_audit_summary(state)
        assert widget["type"] == "contribution_audit_table"
        assert widget["period"] == "2026-05"
        rows = widget["data"]["rows"]
        assert len(rows) == 2
        row_map = {r["member_id"]: r for r in rows}
        assert row_map["m001"]["status"] == "paid"
        assert row_map["m001"]["actual"] == 1000.0
        assert row_map["m002"]["status"] in ("pending", "late")
        assert widget["data"]["summary"]["paid_count"] == 1

    def test_suspended_members_excluded(self):
        members = {
            "active_m":    {"name": "A", "phone": "+1", "join_date": "2025-01-01", "status": "active",
                            "trust_score": 0.7, "dispute_count": 0, "contribution_history": [], "loan_history": []},
            "suspended_m": {"name": "S", "phone": "+2", "join_date": "2025-01-01", "status": "suspended",
                            "trust_score": 0.2, "dispute_count": 5, "contribution_history": [], "loan_history": []},
        }
        state = _fresh_state(members=members)
        widget = _make_secretary(state).weekly_audit_summary(state)
        ids_in_table = {r["member_id"] for r in widget["data"]["rows"]}
        assert "active_m" in ids_in_table
        assert "suspended_m" not in ids_in_table

    def test_empty_members(self):
        widget = _make_secretary(_fresh_state()).weekly_audit_summary(_fresh_state())
        assert widget["data"]["rows"] == []
        assert widget["data"]["summary"]["total_expected"] == 0.0


# ══════════════════════════════════════════════════════════════════════════════
# 6–12. Chama Council governance
# ══════════════════════════════════════════════════════════════════════════════

class TestChamaCouncil:

    def _council(self, state: StateAccessor) -> CouncilSession:
        return CouncilSession(sustain_id="test-chama", state=state)

    def _members(self, n: int) -> list:
        return [f"member-{i:03d}" for i in range(1, n + 1)]

    # ── Financial ─────────────────────────────────────────────────────────────

    def test_financial_60_pct_yes_passes(self):
        state = _fresh_state()
        council = self._council(state)
        member_ids = self._members(10)
        pid = council.create_chama_proposal("test-chama", "chama.financial", "chama_secretary",
                                            {"action": "loan_approval", "amount": 10_000})
        votes = {m: "YES" for m in member_ids[:6]}
        votes.update({m: "NO" for m in member_ids[6:]})
        council.collect_member_votes(pid, votes)
        assert council.resolve_chama(pid, member_ids) == "PASSED"

    def test_financial_below_quorum_in_voting(self):
        state = _fresh_state()
        council = self._council(state)
        member_ids = self._members(10)
        pid = council.create_chama_proposal("test-chama", "chama.financial", "chama_secretary",
                                            {"action": "loan_approval", "amount": 5_000})
        council.collect_member_votes(pid, {m: "YES" for m in member_ids[:5]})
        assert council.resolve_chama(pid, member_ids) == "IN_VOTING"

    def test_financial_majority_no_fails(self):
        state = _fresh_state()
        council = self._council(state)
        member_ids = self._members(10)
        pid = council.create_chama_proposal("test-chama", "chama.financial", "member-001",
                                            {"action": "expense", "amount": 2_000})
        votes = {m: "NO" for m in member_ids[:7]}
        votes.update({m: "YES" for m in member_ids[7:]})
        council.collect_member_votes(pid, votes)
        assert council.resolve_chama(pid, member_ids) == "FAILED"

    # ── Structural ────────────────────────────────────────────────────────────

    def test_structural_9_of_10_yes_passes(self):
        state = _fresh_state()
        council = self._council(state)
        member_ids = self._members(10)
        pid = council.create_chama_proposal("test-chama", "chama.structural", "chama_secretary",
                                            {"action": "rule_change", "rule": "min_contribution", "new_value": 750})
        council.collect_member_votes(pid, {m: "YES" for m in member_ids[:9]})
        assert council.resolve_chama(pid, member_ids, secretary_id="chama_secretary") == "PASSED"

    def test_structural_8_of_10_yes_not_passed(self):
        state = _fresh_state()
        council = self._council(state)
        member_ids = self._members(10)
        pid = council.create_chama_proposal("test-chama", "chama.structural", "chama_secretary",
                                            {"action": "membership_change", "new_member": "m_new"})
        council.collect_member_votes(pid, {m: "YES" for m in member_ids[:8]})
        status = council.resolve_chama(pid, member_ids, secretary_id="chama_secretary")
        assert status != "PASSED"

    def test_structural_secretary_veto(self):
        state = _fresh_state()
        council = self._council(state)
        member_ids = self._members(10)
        secretary_id = "secretary-001"
        pid = council.create_chama_proposal("test-chama", "chama.structural", "member-001",
                                            {"action": "rule_change", "rule": "loan_interest_rate", "new_value": 0.03})
        votes = {m: "YES" for m in member_ids[:9]}
        votes[secretary_id] = "NO"
        council.collect_member_votes(pid, votes)
        assert council.resolve_chama(pid, member_ids + [secretary_id], secretary_id=secretary_id) == "FAILED"

    def test_structural_any_member_no_fails(self):
        state = _fresh_state()
        council = self._council(state)
        member_ids = self._members(10)
        pid = council.create_chama_proposal("test-chama", "chama.structural", "chama_secretary",
                                            {"action": "membership_removal", "member_id": "member-010"})
        votes = {m: "YES" for m in member_ids[:9]}
        votes["member-010"] = "NO"
        council.collect_member_votes(pid, votes)
        assert council.resolve_chama(pid, member_ids) == "FAILED"


# ══════════════════════════════════════════════════════════════════════════════
# 13–15. chama.rotation.advance — sync tests via asyncio.run()
#        (avoids pytest 9 + pytest-asyncio 1.4 assertion-rewriter interaction)
# ══════════════════════════════════════════════════════════════════════════════

def test_rotation_advance_happy_path():
    """Rotation advances: entry marked paid, pool decremented, journal posted."""
    from sustena.operators.chama import chama_rotation_advance
    state = _rotation_state(pool_balance=100_000.0)
    ctx = _make_ctx(state)
    result = asyncio.run(chama_rotation_advance(ctx))

    assert result.status == "ok", f"Got {result.status}: {result.reason}"
    d = result.data
    assert d["period"] == "2026-05"
    assert d["recipient_member_id"] == "m001"
    assert d["amount"] == 10_000.0
    assert d["pool_balance_remaining"] == 90_000.0
    assert d["next_period"] == "2026-06"

    schedule = state.get("rotation.schedule")
    assert schedule[0]["status"] == "paid"
    assert schedule[1]["status"] == "pending"
    assert state.get("rotation.current_period") == "2026-06"
    assert state.get("pool.balance") == 90_000.0

    entries = state.get("accounts.journal_entries")
    assert len(entries) == 1
    lines = entries[0]["lines"]
    assert any(l["account"] == "Pool Balance" and l["debit"] == 10_000.0 for l in lines)
    assert any(l["account"] == "Member Disbursement" and l["credit"] == 10_000.0 for l in lines)


def test_rotation_advance_insufficient_pool():
    """Pool balance < rotation amount → constraint_violated."""
    from sustena.operators.chama import chama_rotation_advance
    state = _rotation_state(pool_balance=5_000.0)
    ctx = _make_ctx(state)
    result = asyncio.run(chama_rotation_advance(ctx))
    assert result.status != "ok", "Expected failure on insufficient pool"
    assert "pool_sufficient_for_rotation" in (result.constraint_violated or "")


def test_rotation_advance_no_pending_entry():
    """No pending rotation entry for current_period → constraint_violated."""
    from sustena.operators.chama import chama_rotation_advance
    state = _rotation_state()
    state.get("rotation.schedule")[0]["status"] = "paid"
    state.set("rotation.current_period", "2026-07")
    ctx = _make_ctx(state)
    result = asyncio.run(chama_rotation_advance(ctx))
    assert result.status != "ok", "Expected failure on no pending entry"
    assert "rotation_pending_entry_exists" in (result.constraint_violated or "")


# ══════════════════════════════════════════════════════════════════════════════
# 16. Secretary deliberate → always ABSTAIN
# ══════════════════════════════════════════════════════════════════════════════

class TestSecretaryDeliberate:

    @pytest.mark.asyncio
    async def test_secretary_always_abstains(self):
        sec = _make_secretary(_fresh_state())
        vote = await sec.deliberate(
            context={"sustain_id": "test-chama"},
            proposal={"operator_name": "chama.loan.disburse", "input_params": {}},
        )
        assert vote.vote == VoteChoice.ABSTAIN
        assert "orchie" in vote.reasoning.lower() or "does not vote" in vote.reasoning.lower()

    @pytest.mark.asyncio
    async def test_secretary_abstains_on_structural_proposal(self):
        sec = _make_secretary(_fresh_state())
        vote = await sec.deliberate(
            context={},
            proposal={"operator_name": "chama.structural.rule_change", "input_params": {}},
        )
        assert vote.vote == VoteChoice.ABSTAIN
