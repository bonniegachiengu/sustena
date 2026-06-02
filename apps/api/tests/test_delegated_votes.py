"""
tests/test_delegated_votes.py

Tests for Sprint 7.4:
  - DelegatedVote dataclass
  - aggregate_delegated_votes()
  - _parse_sandbox_to_delegated_votes()
"""

import pytest

from sustena.core.council import (
    DelegatedVote,
    _parse_sandbox_to_delegated_votes,
    aggregate_delegated_votes,
)
from sustena.operatives.base import OperativeVote, VoteChoice


# ── DelegatedVote dataclass ────────────────────────────────────────────────────


class TestDelegatedVoteDataclass:
    def test_construction(self):
        dv = DelegatedVote(
            position=VoteChoice.YES,
            confidence=0.9,
            reasoning="looks good",
        )
        assert dv.position == VoteChoice.YES
        assert dv.confidence == 0.9
        assert dv.reasoning == "looks good"

    def test_abstain_construction(self):
        dv = DelegatedVote(position=VoteChoice.ABSTAIN, confidence=0.0, reasoning="no signal")
        assert dv.position == VoteChoice.ABSTAIN
        assert dv.confidence == 0.0

    def test_no_construction(self):
        dv = DelegatedVote(position=VoteChoice.NO, confidence=0.7, reasoning="too risky")
        assert dv.position == VoteChoice.NO


# ── aggregate_delegated_votes ──────────────────────────────────────────────────


class TestAggregateDelegatedVotes:
    def test_empty_list_returns_abstain(self):
        result = aggregate_delegated_votes([])
        assert result.vote == VoteChoice.ABSTAIN
        assert result.utility == 0.5

    def test_all_abstain_returns_abstain(self):
        delegated = [
            DelegatedVote(VoteChoice.ABSTAIN, 0.0, "no signal"),
            DelegatedVote(VoteChoice.ABSTAIN, 0.0, "unclear"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.ABSTAIN

    def test_yes_wins_when_weighted_yes_gt_no(self):
        delegated = [
            DelegatedVote(VoteChoice.YES, 0.8, "safe move"),
            DelegatedVote(VoteChoice.NO,  0.3, "minor risk"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.YES

    def test_no_wins_when_weighted_no_gt_yes(self):
        delegated = [
            DelegatedVote(VoteChoice.YES, 0.2, "slight benefit"),
            DelegatedVote(VoteChoice.NO,  0.9, "dangerous"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.NO

    def test_tie_returns_abstain(self):
        delegated = [
            DelegatedVote(VoteChoice.YES, 0.5, "neutral"),
            DelegatedVote(VoteChoice.NO,  0.5, "neutral"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.ABSTAIN

    def test_abstain_votes_do_not_affect_weighted_sums(self):
        delegated = [
            DelegatedVote(VoteChoice.YES,    0.6, "good"),
            DelegatedVote(VoteChoice.ABSTAIN, 0.0, "skip"),
            DelegatedVote(VoteChoice.ABSTAIN, 0.0, "skip"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.YES

    def test_utility_is_mean_of_yes_confidences(self):
        delegated = [
            DelegatedVote(VoteChoice.YES, 0.8, "a"),
            DelegatedVote(VoteChoice.YES, 0.6, "b"),
            DelegatedVote(VoteChoice.NO,  0.9, "c"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.YES
        assert abs(result.utility - 0.7) < 1e-9  # mean of [0.8, 0.6]

    def test_utility_is_0_5_when_no_yes_votes(self):
        delegated = [
            DelegatedVote(VoteChoice.NO, 0.9, "risky"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.NO
        assert result.utility == 0.5

    def test_reasoning_joins_all_with_pipe(self):
        delegated = [
            DelegatedVote(VoteChoice.YES, 0.8, "reason A"),
            DelegatedVote(VoteChoice.NO,  0.3, "reason B"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.reasoning == "reason A | reason B"

    def test_single_yes_vote(self):
        delegated = [DelegatedVote(VoteChoice.YES, 0.9, "clear YES")]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.YES
        assert result.utility == pytest.approx(0.9)

    def test_single_no_vote(self):
        delegated = [DelegatedVote(VoteChoice.NO, 0.75, "clear NO")]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.NO
        assert result.utility == 0.5

    def test_returns_operative_vote_instance(self):
        delegated = [DelegatedVote(VoteChoice.YES, 0.7, "ok")]
        result = aggregate_delegated_votes(delegated)
        assert isinstance(result, OperativeVote)

    def test_high_confidence_yes_beats_many_low_confidence_nos(self):
        delegated = [
            DelegatedVote(VoteChoice.YES, 0.9, "strong YES"),
            DelegatedVote(VoteChoice.NO,  0.2, "weak NO"),
            DelegatedVote(VoteChoice.NO,  0.2, "weak NO"),
            DelegatedVote(VoteChoice.NO,  0.2, "weak NO"),
        ]
        # weighted_yes=0.9, weighted_no=0.6 → YES
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.YES

    def test_abstain_mixed_with_yes_no_does_not_break_tie(self):
        delegated = [
            DelegatedVote(VoteChoice.YES,    0.5, "yes"),
            DelegatedVote(VoteChoice.NO,     0.5, "no"),
            DelegatedVote(VoteChoice.ABSTAIN, 0.0, "skip"),
        ]
        result = aggregate_delegated_votes(delegated)
        assert result.vote == VoteChoice.ABSTAIN


# ── _parse_sandbox_to_delegated_votes ─────────────────────────────────────────


class TestParseSandboxToDelegatedVotes:
    def test_empty_sandbox_results_returns_empty_list(self):
        result = _parse_sandbox_to_delegated_votes({})
        assert result == []

    def test_ok_status_reads_vote_confidence_reasoning(self):
        sandbox = {
            "risk_assessor": {
                "status": "ok",
                "operator_name": "budget.summary",
                "rationale": "safe",
                "simulation_results": {
                    "vote": "YES",
                    "confidence": 0.85,
                    "reasoning": "budget healthy",
                },
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert len(votes) == 1
        assert votes[0].position == VoteChoice.YES
        assert votes[0].confidence == pytest.approx(0.85)
        assert votes[0].reasoning == "budget healthy"

    def test_non_ok_status_produces_abstain_with_zero_confidence(self):
        sandbox = {
            "risk_assessor": {
                "status": "error",
                "reason": "graph not found",
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert len(votes) == 1
        assert votes[0].position == VoteChoice.ABSTAIN
        assert votes[0].confidence == 0.0
        assert "graph not found" in votes[0].reasoning

    def test_calibration_error_status_produces_abstain(self):
        sandbox = {
            "assessor": {
                "status": "calibration_error",
                "reason": "missing placeholder",
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.ABSTAIN
        assert votes[0].confidence == 0.0

    def test_not_found_status_produces_abstain(self):
        sandbox = {
            "assessor": {
                "status": "not_found",
                "reason": "file missing",
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.ABSTAIN

    def test_ok_with_missing_vote_defaults_to_abstain(self):
        sandbox = {
            "assessor": {
                "status": "ok",
                "simulation_results": {"confidence": 0.6, "reasoning": "unclear"},
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.ABSTAIN

    def test_ok_with_missing_confidence_defaults_to_0_5(self):
        sandbox = {
            "assessor": {
                "status": "ok",
                "simulation_results": {"vote": "YES", "reasoning": "fine"},
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].confidence == pytest.approx(0.5)

    def test_ok_with_missing_reasoning_defaults_to_empty_string(self):
        sandbox = {
            "assessor": {
                "status": "ok",
                "simulation_results": {"vote": "NO", "confidence": 0.7},
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].reasoning == ""

    def test_ok_with_none_simulation_results_defaults_all_fields(self):
        sandbox = {
            "assessor": {
                "status": "ok",
                "simulation_results": None,
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.ABSTAIN
        assert votes[0].confidence == pytest.approx(0.5)
        assert votes[0].reasoning == ""

    def test_invalid_vote_string_defaults_to_abstain(self):
        sandbox = {
            "assessor": {
                "status": "ok",
                "simulation_results": {"vote": "MAYBE", "confidence": 0.5},
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.ABSTAIN

    def test_multiple_sub_ops_all_parsed(self):
        sandbox = {
            "sub_a": {
                "status": "ok",
                "simulation_results": {"vote": "YES", "confidence": 0.9, "reasoning": "A ok"},
            },
            "sub_b": {
                "status": "error",
                "reason": "B failed",
            },
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert len(votes) == 2
        positions = {v.position for v in votes}
        assert VoteChoice.YES in positions
        assert VoteChoice.ABSTAIN in positions

    def test_no_status_key_treats_as_error(self):
        sandbox = {
            "assessor": {"reason": "something weird"}
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.ABSTAIN
        assert votes[0].confidence == 0.0

    def test_non_ok_with_no_reason_uses_fallback_message(self):
        sandbox = {
            "mystery_op": {"status": "timeout"}
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.ABSTAIN
        assert "mystery_op" in votes[0].reasoning or "timeout" in votes[0].reasoning

    def test_vote_case_insensitive(self):
        sandbox = {
            "assessor": {
                "status": "ok",
                "simulation_results": {"vote": "yes", "confidence": 0.8},
            }
        }
        votes = _parse_sandbox_to_delegated_votes(sandbox)
        assert votes[0].position == VoteChoice.YES
