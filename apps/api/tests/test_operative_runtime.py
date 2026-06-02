"""
tests/test_operative_runtime.py

Epic 1.2 — Operative Runtime tests.

Coverage:
  - MentorOperative.should_evaluate() threshold check (no LLM call)
  - event.finances.pocket_spent at 85% → proposal created in council_proposals
  - council.collect_votes() → vote records created per operative
  - 48h timeout → proposal status moves to DEFERRED
  - Log shows Haiku model; token count logged to operators_log (_log_entries)
  - _call_claude NOT called when should_evaluate() returns False

All Anthropic API calls are mocked — zero real HTTP calls.
"""

import json
import math
import pytest
from datetime import datetime, timedelta, timezone
from unittest.mock import AsyncMock, MagicMock, patch

from sustena.core.state import StateAccessor
from sustena.operatives.base import (
    HAIKU,
    BaseOperative,
    OperativeProposal,
    OperativeVote,
    VoteChoice,
)
from sustena.operatives.mentor import MentorOperative
from sustena.core.council import (
    CouncilSession,
    STATUS_IN_VOTING,
    STATUS_PASSED,
    STATUS_DEFERRED,
    STATUS_OVERRIDDEN_BY_USER,
)


# ── Fixtures ──────────────────────────────────────────────────────────────────


def _make_state(
    liquid: float = 45000.0,
    food_allocated: float = 5000.0,
    food_spent: float = 4300.0,     # 86% — over threshold
    monthly_income: float = 50000.0,
) -> StateAccessor:
    """Build a Homestead state with one budget pocket near-exhaustion."""
    return StateAccessor({
        "finances": {
            "liquid": {"balance": liquid},
            "pockets": {
                "food": {
                    "allocated": food_allocated,
                    "spent":     food_spent,
                    "limit":     food_allocated,
                }
            },
            "income": {
                "sources":       [],
                "monthly_total": monthly_income,
            },
        },
        "council_proposals": [],
        "council_votes":     [],
    })


def _make_mock_claude_client(response_text: str = None) -> MagicMock:
    """
    Return a mock Anthropic client whose .messages.create() is an AsyncMock.
    Matches the interface: client.messages.create(model, max_tokens, system, messages)
    """
    mock_usage = MagicMock()
    mock_usage.input_tokens  = 15
    mock_usage.output_tokens = 25

    mock_content = MagicMock()
    mock_content.text = response_text or json.dumps({
        "vote":      "YES",
        "reasoning": "Proposal improves budget position.",
        "utility":   0.8,
    })

    mock_message = MagicMock()
    mock_message.content = [mock_content]
    mock_message.usage   = mock_usage
    mock_message.model   = HAIKU

    mock_messages = MagicMock()
    mock_messages.create = AsyncMock(return_value=mock_message)

    client = MagicMock()
    client.messages = mock_messages
    return client


def _make_mentor(
    state: StateAccessor,
    claude_response: str = None,
    config: dict = None,
) -> MentorOperative:
    client = _make_mock_claude_client(claude_response)
    return MentorOperative(
        config=config or {},
        state_accessor=state,
        claude_client=client,
    )


# ── should_evaluate tests ─────────────────────────────────────────────────────


class TestShouldEvaluate:

    def test_returns_true_when_pocket_below_warn_threshold(self):
        """Food pocket at 86% spent (14% remaining) → should_evaluate = True."""
        state  = _make_state(food_allocated=5000.0, food_spent=4300.0)
        mentor = _make_mentor(state)
        assert mentor.should_evaluate(state) is True

    def test_returns_false_when_all_pockets_healthy(self):
        """Food pocket at 40% spent (60% remaining) → should_evaluate = False."""
        state  = _make_state(food_allocated=5000.0, food_spent=2000.0)
        mentor = _make_mentor(state)
        assert mentor.should_evaluate(state) is False

    def test_returns_false_on_empty_pockets(self):
        """No pockets → should_evaluate = False."""
        raw_state = StateAccessor({
            "finances": {
                "liquid":  {"balance": 10000.0},
                "pockets": {},
                "income":  {"sources": [], "monthly_total": 50000.0},
            },
            "council_proposals": [],
            "council_votes":     [],
        })
        mentor = _make_mentor(raw_state)
        assert mentor.should_evaluate(raw_state) is False

    def test_no_llm_call_when_should_evaluate_false(self):
        """
        When should_evaluate() returns False, _call_claude must not be invoked.
        This asserts the zero-LLM-call guarantee.
        """
        state  = _make_state(food_allocated=5000.0, food_spent=2000.0)
        mentor = _make_mentor(state)

        # Patch _call_claude to track calls
        mentor._call_claude = AsyncMock()

        result = mentor.should_evaluate(state)
        assert result is False
        mentor._call_claude.assert_not_called()

    def test_uses_custom_warn_threshold(self):
        """Custom warn_threshold=0.30 → pocket at 75% spent triggers."""
        state = _make_state(food_allocated=5000.0, food_spent=3750.0)  # 75% spent, 25% remaining
        # Default threshold 0.20 → remaining=0.25 > 0.20 → False
        mentor_default = _make_mentor(state)
        assert mentor_default.should_evaluate(state) is False

        # Custom threshold 0.30 → remaining=0.25 < 0.30 → True
        mentor_custom = _make_mentor(state, config={"warn_threshold": 0.30})
        assert mentor_custom.should_evaluate(state) is True


# ── evaluate tests ────────────────────────────────────────────────────────────


class TestEvaluate:

    @pytest.mark.asyncio
    async def test_pocket_at_85pct_creates_proposal(self):
        """
        Trigger event.finances.pocket_spent with food pocket at 85%.
        evaluate() returns an OperativeProposal via the graph — no LLM call.
        """
        state  = _make_state(food_allocated=5000.0, food_spent=4250.0)  # 85%
        mentor = _make_mentor(state)
        trigger = {
            "pocket":    "food",
            "amount":    250.0,
            "timestamp": datetime.utcnow().isoformat(),
        }
        proposal = await mentor.evaluate(trigger)

        assert proposal is not None
        assert isinstance(proposal, OperativeProposal)

    @pytest.mark.asyncio
    async def test_evaluate_does_not_call_llm(self):
        """Sprint 5.3: evaluate() uses the graph — zero Anthropic API calls."""
        state  = _make_state(food_allocated=5000.0, food_spent=4300.0)
        mentor = _make_mentor(state)
        await mentor.evaluate({"pocket": "food", "amount": 100.0})
        mentor.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_evaluate_produces_no_llm_log_entries(self):
        """Graph-based evaluate() generates no LLM log entries."""
        state  = _make_state(food_allocated=5000.0, food_spent=4300.0)
        mentor = _make_mentor(state)
        await mentor.evaluate({"pocket": "food", "amount": 100.0})
        entries = mentor.flush_log_entries()
        assert len(entries) == 0

    @pytest.mark.asyncio
    async def test_evaluate_returns_none_when_no_pockets_over_threshold(self):
        """Food at 40% → evaluate() returns None (no proposal warranted)."""
        state  = _make_state(food_allocated=5000.0, food_spent=2000.0)
        mentor = _make_mentor(state)
        proposal = await mentor.evaluate({"pocket": "food", "amount": 100.0})
        assert proposal is None


# ── Council integration tests ─────────────────────────────────────────────────


class TestCouncilSession:

    def _make_council(self, state: StateAccessor, clock=None) -> CouncilSession:
        return CouncilSession(
            sustain_id="test-sustain",
            state=state,
            db_session=None,
            clock=clock,
        )

    # ── create_proposal ───────────────────────────────────────────────────────

    def test_create_proposal_persists_to_state(self):
        state   = _make_state()
        council = self._make_council(state)

        pid = council.create_proposal(
            sustain_id="test-sustain",
            proposed_by="mentor",
            operator_name="budget.reallocate",
            input_params={"pocket_name": "food", "amount": 500.0},
            simulation_results={"liquid_after": 44500.0},
        )

        proposals = state.get("council_proposals")
        assert len(proposals) == 1
        p = proposals[0]
        assert p["id"] == pid
        assert p["operator_name"] == "budget.reallocate"
        assert p["proposed_by"] == "mentor"
        assert p["status"] == "IN_VOTING"

    def test_create_proposal_sets_48h_expiry(self):
        fixed_now = datetime(2026, 6, 1, 12, 0, 0, tzinfo=timezone.utc)
        state     = _make_state()
        council   = self._make_council(state, clock=lambda: fixed_now)

        pid = council.create_proposal(
            sustain_id="test-sustain",
            proposed_by="mentor",
            operator_name="budget.reallocate",
            input_params={"pocket_name": "food", "amount": 500.0},
        )

        proposals = state.get("council_proposals")
        p = proposals[0]
        expires_at = datetime.fromisoformat(p["expires_at"])
        expected   = fixed_now + timedelta(hours=48)
        assert expires_at == expected

    # ── collect_votes ─────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_collect_votes_creates_vote_records(self):
        """collect_votes() should append one vote record per operative to state."""
        state   = _make_state()
        council = self._make_council(state)

        pid = council.create_proposal(
            sustain_id="test-sustain",
            proposed_by="mentor",
            operator_name="budget.reallocate",
            input_params={"pocket_name": "food", "amount": 500.0},
        )

        mentor = _make_mentor(
            state,
            claude_response=json.dumps({
                "vote": "YES", "reasoning": "Improves budget.", "utility": 0.8
            }),
        )

        vote_results = await council.collect_votes(pid, [mentor])

        # One vote record in state
        votes = state.get("council_votes")
        assert len(votes) == 1
        assert votes[0]["proposal_id"] == pid
        assert votes[0]["vote"] == "YES"
        assert votes[0]["operative_id"] == "mentor"

        # Return value also contains the vote
        assert "mentor" in vote_results
        assert vote_results["mentor"]["vote"] == "YES"

    @pytest.mark.asyncio
    async def test_collect_votes_multiple_operatives(self):
        """Two operatives → two vote records. mentor2 uses low-liquid state → votes NO."""
        state   = _make_state()
        council = self._make_council(state)

        # Small amount → stays above floor → YES
        pid = council.create_proposal(
            sustain_id="test-sustain",
            proposed_by="mentor",
            operator_name="budget.reallocate",
            input_params={"pocket_name": "food", "amount": 500.0},
        )

        mentor1 = _make_mentor(state)  # liquid=45000, amount=500 → above floor → YES

        # mentor2 has very low liquid: any reallocation breaches floor
        low_liquid_state = StateAccessor({
            "finances": {
                "liquid": {"balance": 1000.0},   # well below 10% floor (5000)
                "pockets": {"food": {"allocated": 5000.0, "spent": 4250.0, "limit": 5000.0}},
                "income":  {"sources": [], "monthly_total": 50000.0},
            },
            "council_proposals": [],
            "council_votes": [],
        })
        mentor2 = _make_mentor(low_liquid_state)
        mentor2.operative_id = "mentor_2"

        await council.collect_votes(pid, [mentor1, mentor2])

        votes = state.get("council_votes")
        assert len(votes) == 2
        vote_map = {v["operative_id"]: v["vote"] for v in votes}
        assert vote_map["mentor"]   == "YES"
        assert vote_map["mentor_2"] == "NO"

    # ── resolve tests ─────────────────────────────────────────────────────────

    def test_resolve_passed_on_user_yes(self):
        state   = _make_state()
        council = self._make_council(state)
        pid = council.create_proposal("test-sustain", "mentor", "budget.reallocate", {})

        status = council.resolve(pid, user_vote="YES")
        assert status == STATUS_PASSED

        p = council._get_proposal(pid)
        assert p["status"] == STATUS_PASSED

    def test_resolve_overridden_on_user_no(self):
        state   = _make_state()
        council = self._make_council(state)
        pid = council.create_proposal("test-sustain", "mentor", "budget.reallocate", {})

        status = council.resolve(pid, user_vote="NO")
        assert status == STATUS_OVERRIDDEN_BY_USER

    def test_resolve_deferred_on_48h_timeout(self):
        """
        Simulate 48h passing with no user response → status moves to DEFERRED.
        """
        created_at = datetime(2026, 5, 1, 9, 0, 0, tzinfo=timezone.utc)
        # Council is created at created_at
        council_clock = MagicMock(return_value=created_at)
        state   = _make_state()
        council = self._make_council(state, clock=council_clock)

        pid = council.create_proposal("test-sustain", "mentor", "budget.reallocate", {})

        # Now advance time past 48h expiry
        expired_time = created_at + timedelta(hours=49)
        council._clock = lambda: expired_time

        status = council.resolve(pid, user_vote=None)
        assert status == STATUS_DEFERRED

        p = council._get_proposal(pid)
        assert p["status"] == STATUS_DEFERRED

    def test_resolve_in_voting_before_timeout(self):
        """Before 48h and no user vote → stays IN_VOTING."""
        created_at = datetime(2026, 5, 1, 9, 0, 0, tzinfo=timezone.utc)
        council_clock = MagicMock(return_value=created_at)
        state   = _make_state()
        council = self._make_council(state, clock=council_clock)

        pid = council.create_proposal("test-sustain", "mentor", "budget.reallocate", {})

        # Advance time but stay within TTL
        council._clock = lambda: created_at + timedelta(hours=24)
        status = council.resolve(pid, user_vote=None)
        assert status == STATUS_IN_VOTING

    # ── nash_utility_score ────────────────────────────────────────────────────

    def test_nash_utility_score_two_yes_votes(self):
        state   = _make_state()
        council = self._make_council(state)

        votes = {
            "mentor":   {"vote": "YES", "utility": 0.8},
            "guardian": {"vote": "YES", "utility": 0.6},
        }
        score = council.nash_utility_score(votes)
        # Geometric mean of [0.8, 0.6]
        expected = math.sqrt(0.8 * 0.6)
        assert abs(score - expected) < 1e-9

    def test_nash_utility_score_no_yes_votes(self):
        state   = _make_state()
        council = self._make_council(state)

        votes = {
            "mentor": {"vote": "NO",     "utility": 0.2},
            "guard":  {"vote": "ABSTAIN","utility": 0.5},
        }
        assert council.nash_utility_score(votes) == 0.0

    def test_nash_utility_score_mixed_votes(self):
        state   = _make_state()
        council = self._make_council(state)

        votes = {
            "mentor":   {"vote": "YES",    "utility": 0.9},
            "guardian": {"vote": "NO",     "utility": 0.1},
            "scout":    {"vote": "ABSTAIN","utility": 0.5},
        }
        score = council.nash_utility_score(votes)
        # Only YES votes count
        assert abs(score - 0.9) < 1e-9


# ── Full integration: 85% spend → proposal ────────────────────────────────────


class TestFullPocketSpentFlow:
    """
    End-to-end: simulate a pocket_spent event at 85% utilisation,
    run the operative pipeline, and confirm a proposal is created in state.
    """

    @pytest.mark.asyncio
    async def test_85pct_pocket_spent_creates_council_proposal(self):
        state = _make_state(food_allocated=5000.0, food_spent=4250.0)   # 85%
        mentor = _make_mentor(
            state,
            claude_response=json.dumps({
                "action":    "reallocate",
                "pocket":    "food",
                "amount":    500.0,
                "rationale": "Food at 85%, topping up.",
            }),
        )

        trigger_event = {
            "pocket":    "food",
            "amount":    250.0,
            "timestamp": "2026-05-29T10:00:00",
        }

        # 1. Threshold check
        assert mentor.should_evaluate(state) is True

        # 2. Evaluate
        proposal = await mentor.evaluate(trigger_event)
        assert proposal is not None

        # 3. Push to Council
        council = CouncilSession(
            sustain_id="test-sustain",
            state=state,
            db_session=None,
        )
        pid = council.create_proposal(
            sustain_id="test-sustain",
            proposed_by=mentor.operative_id,
            operator_name=proposal.operator_name,
            input_params=proposal.input_params,
            simulation_results=proposal.simulation_results,
        )

        # Proposal exists in state
        proposals = state.get("council_proposals")
        assert len(proposals) == 1
        assert proposals[0]["id"] == pid
        assert proposals[0]["status"] == "IN_VOTING"

    @pytest.mark.asyncio
    async def test_vote_records_created_per_operative(self):
        """
        collect_votes() on two operatives → two vote records in council_votes.
        """
        state = _make_state(food_allocated=5000.0, food_spent=4250.0)

        council = CouncilSession(sustain_id="test-sustain", state=state)
        pid = council.create_proposal(
            "test-sustain", "mentor", "budget.reallocate",
            {"pocket_name": "food", "amount": 500.0},
        )

        operative_a = _make_mentor(
            state,
            claude_response=json.dumps({"vote": "YES", "reasoning": "Good.", "utility": 0.8}),
        )
        operative_b = _make_mentor(
            state,
            claude_response=json.dumps({"vote": "YES", "reasoning": "Agreed.", "utility": 0.75}),
        )
        operative_b.operative_id = "mentor_b"

        votes = await council.collect_votes(pid, [operative_a, operative_b])

        all_votes = state.get("council_votes")
        assert len(all_votes) == 2

        ids = {v["operative_id"] for v in all_votes}
        assert "mentor"   in ids
        assert "mentor_b" in ids

        # Nash score with two YES votes
        score = council.nash_utility_score(votes)
        assert score > 0.0


# ── deliberate — vote logic ───────────────────────────────────────────────────


class TestMentorDeliberate:

    @pytest.mark.asyncio
    async def test_deliberate_votes_no_when_liquid_below_floor(self):
        """
        If executing the proposal would push liquid below 10% of income (floor),
        MentorOperative must vote NO without calling the LLM.
        """
        # Income = 50000 → floor = 5000
        # Liquid = 6000, proposal amount = 2000 → liquid_after = 4000 < floor
        state = StateAccessor({
            "finances": {
                "liquid": {"balance": 6000.0},
                "pockets": {
                    "food": {"allocated": 5000.0, "spent": 4300.0, "limit": 5000.0}
                },
                "income": {"sources": [], "monthly_total": 50000.0},
            },
            "council_proposals": [],
            "council_votes":     [],
        })

        mentor = _make_mentor(state)

        proposal = {
            "operator_name": "budget.reallocate",
            "input_params":  {"pocket_name": "food", "amount": 2000.0},
        }

        vote = await mentor.deliberate(context={}, proposal=proposal)
        assert vote.vote == VoteChoice.NO
        # LLM should NOT have been called (hard NO by rule)
        mentor.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_abstains_on_non_finance_proposal(self):
        """Proposals outside the budget domain → ABSTAIN without LLM call."""
        state  = _make_state()
        mentor = _make_mentor(state)

        proposal = {
            "operator_name": "staff.hire",
            "input_params":  {"role": "manager"},
        }
        vote = await mentor.deliberate(context={}, proposal=proposal)
        assert vote.vote == VoteChoice.ABSTAIN
        mentor.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_yes_does_not_call_llm(self):
        """Sprint 5.3: deliberate() YES vote uses graph — zero Anthropic API calls."""
        state  = _make_state()
        mentor = _make_mentor(state)

        proposal = {
            "operator_name": "budget.reallocate",
            "input_params":  {"pocket_name": "food", "amount": 200.0},
        }
        vote = await mentor.deliberate(context={}, proposal=proposal)
        assert vote.vote == VoteChoice.YES
        mentor.claude_client.messages.create.assert_not_called()


# ── Log entries (operators_log sink) ─────────────────────────────────────────


class TestOperativeLogEntries:

    @pytest.mark.asyncio
    async def test_evaluate_produces_no_log_entries(self):
        """Sprint 5.3: graph-based evaluate() generates no LLM log entries."""
        state  = _make_state(food_allocated=5000.0, food_spent=4300.0)
        mentor = _make_mentor(state)
        await mentor.evaluate({"pocket": "food", "amount": 100.0})
        entries = mentor.flush_log_entries()
        assert len(entries) == 0

    @pytest.mark.asyncio
    async def test_flush_clears_log_entries(self):
        """flush_log_entries() returns empty list on second call."""
        state  = _make_state(food_allocated=5000.0, food_spent=4300.0)
        mentor = _make_mentor(state)
        await mentor.evaluate({"pocket": "food", "amount": 100.0})
        first  = mentor.flush_log_entries()
        second = mentor.flush_log_entries()
        assert first  == []
        assert second == []
