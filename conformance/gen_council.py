"""
Council-slice vector generation.

Records two decisions from the reference engine:

  - aggregation: several sub-operative votes combining into one councillor vote
  - resolution: councillor votes plus the person's decision producing a status

Resolution is driven through the real `CouncilSession.resolve()` against an
in-memory state, so the recorded branch order is the engine's own.
"""

from __future__ import annotations

from datetime import datetime, timedelta, timezone

from sustena.core.council import (
    STATUS_DEFERRED,
    STATUS_FAILED,
    STATUS_IN_VOTING,
    STATUS_OVERRIDDEN_BY_USER,
    STATUS_PASSED,
    CouncilSession,
    DelegatedVote,
    aggregate_delegated_votes,
)
from sustena.core.state import StateAccessor

_ = (STATUS_DEFERRED, STATUS_FAILED, STATUS_IN_VOTING,
     STATUS_OVERRIDDEN_BY_USER, STATUS_PASSED)


# ── Aggregation ───────────────────────────────────────────────────────────────

AGGREGATION_CASES = [
    ("heavier_confidence_wins", [("YES", 0.9), ("NO", 0.4)]),
    ("heavier_no_wins", [("YES", 0.2), ("NO", 0.8)]),
    ("tie_abstains", [("YES", 0.5), ("NO", 0.5)]),
    ("abstentions_carry_no_weight", [("ABSTAIN", 1.0), ("YES", 0.1)]),
    ("all_abstain", [("ABSTAIN", 0.9), ("ABSTAIN", 0.9)]),
    ("no_votes_at_all", []),
    ("utility_neutral_when_no_yes", [("NO", 0.9)]),
    ("utility_is_mean_of_yes_confidences", [("YES", 0.4), ("YES", 0.8), ("NO", 0.1)]),
    ("single_yes", [("YES", 1.0)]),
]


def run_aggregation_case(votes: list) -> dict:
    delegated = [
        DelegatedVote(position=p, confidence=c, reasoning="r") for p, c in votes
    ]
    out = aggregate_delegated_votes(delegated)
    vote = out.vote.value if hasattr(out.vote, "value") else str(out.vote)
    return {"vote": vote, "utility": out.utility}


# ── Resolution ────────────────────────────────────────────────────────────────

RESOLUTION_CASES = [
    # (name, councillor votes, votes_collected, user_vote, expired)
    ("council_support_alone_does_not_pass", ["YES", "YES", "YES"], True, None, False),
    ("one_yes_still_waits", ["YES"], True, None, False),
    ("user_yes_passes", ["YES", "YES"], True, "YES", False),
    ("user_no_overrides_a_unanimous_council", ["YES", "YES", "YES"], True, "NO", False),
    ("user_yes_passes_despite_a_rejecting_council", ["NO", "NO"], True, "YES", False),
    ("expired_with_no_user_decision_defers", ["YES", "YES"], True, None, True),
    ("expired_but_user_said_yes_still_passes", ["YES"], True, "YES", True),
    ("asked_and_nobody_answered_fails", [], True, None, False),
    ("not_yet_asked_keeps_waiting", [], False, None, False),
    ("all_no_votes_keeps_waiting_for_the_person", ["NO", "NO"], True, None, False),
]


def run_resolution_case(votes, votes_collected, user_vote, expired) -> dict:
    now = datetime.now(timezone.utc)
    created = now - timedelta(hours=72 if expired else 1)
    expires = created + timedelta(hours=48)

    proposal_id = "p1"
    state_dict = {
        "council_proposals": [{
            "id": proposal_id,
            "status": "IN_VOTING",
            "created_at": created.isoformat(),
            "expires_at": expires.isoformat(),
        }],
        "council_votes": [
            {"proposal_id": proposal_id, "operative_id": f"op{i}", "vote": v}
            for i, v in enumerate(votes)
        ],
    }

    session = CouncilSession(sustain_id="conformance", state=StateAccessor(state_dict))
    if votes_collected:
        session._votes_collected.add(proposal_id)

    status = session.resolve(proposal_id, user_vote=user_vote)
    return {"status": status}
