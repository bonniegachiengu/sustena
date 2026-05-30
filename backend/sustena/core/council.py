"""
sustena/core/council.py

CouncilSession — the DAO governance layer for a sustain.

Every significant operator call proposed by an operative goes through the
Council before execution. The Council collects votes from operatives, applies
Nash bargaining to resolve the outcome, and notifies the user for final
ratification.

Vote weight distribution (from schema.py):
  user            : 51%  (weight = 0.51)
  each operative  : 9.8% (weight = 0.098 × 5 = 49%)

Resolution rules:
  IN_VOTING  → 2+ operative YES votes, user not yet responded.
  PASSED     → user votes YES.
  OVERRIDDEN_BY_USER → user votes NO.
  DEFERRED   → no user response within 48 hours.

Nash bargaining:
  Utility score = geometric mean of operative utility improvements.
  Used as a tiebreaker and as a metric for reporting.
"""

import json
import logging
import math
import uuid
from datetime import datetime, timedelta, timezone
from typing import Any

logger = logging.getLogger(__name__)

# ── Proposal status constants ──────────────────────────────────────────────────

STATUS_IN_VOTING         = "IN_VOTING"
STATUS_PASSED            = "PASSED"
STATUS_FAILED            = "FAILED"
STATUS_DEFERRED          = "DEFERRED"
STATUS_OVERRIDDEN_BY_USER = "OVERRIDDEN_BY_USER"

# Proposals expire after 48 hours without user response
PROPOSAL_TTL_HOURS = 48


class CouncilSession:
    """
    Manages the full lifecycle of a Council proposal.

    Args:
        sustain_id : ID of the sustain this session governs.
        state      : StateAccessor bound to the sustain's live state.
                     Used to persist proposals and votes in the state dict
                     (council_proposals / council_votes lists).
        db_session : Optional SQLAlchemy async session for persistence.
                     If None, proposals are stored only in state (tests / local dev).
        clock      : Optional callable () → datetime for testable time injection.
                     Defaults to datetime.utcnow.
    """

    def __init__(
        self,
        sustain_id: str,
        state: Any,          # StateAccessor
        db_session: Any = None,
        clock: Any = None,   # () -> datetime
    ) -> None:
        self.sustain_id  = sustain_id
        self.state       = state
        self._db         = db_session
        self._clock      = clock or (lambda: datetime.now(timezone.utc))
        # Track which proposals have had collect_votes called on them
        self._votes_collected: set[str] = set()

        # Ensure state has the council collections
        if not self.state.exists("council_proposals"):
            self.state.set("council_proposals", [])
        if not self.state.exists("council_votes"):
            self.state.set("council_votes", [])

    # ── create_proposal ───────────────────────────────────────────────────────

    def create_proposal(
        self,
        sustain_id: str,
        proposed_by: str,
        operator_name: str,
        input_params: dict,
        simulation_results: dict | None = None,
    ) -> str:
        """
        Create a new Council proposal and persist it to state.

        Args:
            sustain_id        : sustain context (passed through for clarity)
            proposed_by       : operative_id or 'orchie'
            operator_name     : e.g. "budget.reallocate"
            input_params      : kwargs for the operator
            simulation_results: optional forward-simulation output

        Returns:
            proposal_id (UUID string)
        """
        now        = self._clock()
        expires_at = now + timedelta(hours=PROPOSAL_TTL_HOURS)

        proposal = {
            "id":                     str(uuid.uuid4()),
            "sustain_id":             sustain_id,
            "proposed_by":            proposed_by,
            "operator_name":          operator_name,
            "input_json":             json.dumps(input_params),
            "simulation_results_json": json.dumps(simulation_results or {}),
            "status":                 STATUS_IN_VOTING,
            "created_at":             now.isoformat(),
            "resolved_at":            None,
            "expires_at":             expires_at.isoformat(),
        }

        self.state.append("council_proposals", proposal)
        logger.info(
            "[council] proposal created: id=%s operator=%s proposed_by=%s",
            proposal["id"], operator_name, proposed_by,
        )
        return proposal["id"]

    # ── collect_votes ─────────────────────────────────────────────────────────

    async def collect_votes(
        self,
        proposal_id: str,
        operatives: list[Any],   # list[BaseOperative]
    ) -> dict:
        """
        Ask each operative to deliberate and record their vote.

        Args:
            proposal_id : Council proposal to vote on.
            operatives  : Operative instances to poll.

        Returns:
            dict mapping operative_id → OperativeVote dict.
        """
        proposal = self._get_proposal(proposal_id)
        if proposal is None:
            raise ValueError(f"Proposal '{proposal_id}' not found in state.")

        # Record that collect_votes was invoked for this proposal
        self._votes_collected.add(proposal_id)

        context = {
            "sustain_id":   self.sustain_id,
            "proposal_id":  proposal_id,
            "operator_name": proposal.get("operator_name"),
        }

        proposal_dict = {
            "operator_name": proposal.get("operator_name"),
            "input_params":  json.loads(proposal.get("input_json", "{}")),
            "simulation_results": json.loads(
                proposal.get("simulation_results_json", "{}")
            ),
        }

        vote_results: dict[str, dict] = {}

        for operative in operatives:
            try:
                vote = await operative.deliberate(context, proposal_dict)
                vote_record = {
                    "id":           str(uuid.uuid4()),
                    "proposal_id":  proposal_id,
                    "operative_id": operative.operative_id,
                    "vote":         vote.vote.value,
                    "reasoning":    vote.reasoning,
                    "weight":       0.098,   # per schema default
                    "timestamp":    self._clock().isoformat(),
                    "utility":      vote.utility,
                }
                self.state.append("council_votes", vote_record)
                vote_results[operative.operative_id] = vote.to_dict()

                logger.info(
                    "[council] vote recorded: proposal=%s operative=%s vote=%s",
                    proposal_id, operative.operative_id, vote.vote.value,
                )
            except Exception as exc:
                logger.error(
                    "[council] operative '%s' deliberation error: %s",
                    getattr(operative, "operative_id", "unknown"), exc,
                )
                # Record ABSTAIN on error
                vote_record = {
                    "id":           str(uuid.uuid4()),
                    "proposal_id":  proposal_id,
                    "operative_id": getattr(operative, "operative_id", "unknown"),
                    "vote":         "ABSTAIN",
                    "reasoning":    f"Deliberation error: {exc}",
                    "weight":       0.098,
                    "timestamp":    self._clock().isoformat(),
                    "utility":      0.5,
                }
                self.state.append("council_votes", vote_record)
                vote_results[getattr(operative, "operative_id", "unknown")] = {
                    "vote": "ABSTAIN",
                    "reasoning": str(exc),
                    "utility": 0.5,
                }

        return vote_results

    # ── resolve ───────────────────────────────────────────────────────────────

    def resolve(
        self,
        proposal_id: str,
        user_vote: str | None = None,
    ) -> str:
        """
        Resolve the status of a proposal.

        Resolution logic:
          1. Tally operative YES votes from council_votes.
          2. If 2+ YES votes and user_vote is None → IN_VOTING (notify user).
          3. If user_vote == "YES"  → PASSED.
          4. If user_vote == "NO"   → OVERRIDDEN_BY_USER.
          5. If created_at + 48h <= now and user_vote is None → DEFERRED.
          6. Otherwise → IN_VOTING (still waiting).

        Updates the proposal status in state and returns the new status string.
        """
        proposal = self._get_proposal(proposal_id)
        if proposal is None:
            raise ValueError(f"Proposal '{proposal_id}' not found in state.")

        # Collect operative votes for this proposal
        all_votes: list[dict] = self.state.get("council_votes", [])
        proposal_votes = [v for v in all_votes if v.get("proposal_id") == proposal_id]
        yes_count = sum(1 for v in proposal_votes if v.get("vote") == "YES")

        now = self._clock()

        # Failure path: collect_votes was called but no operatives responded at all
        if proposal_id in self._votes_collected and len(proposal_votes) == 0:
            new_status = STATUS_FAILED
            proposals: list[dict] = self.state.get("council_proposals", [])
            for p in proposals:
                if p.get("id") == proposal_id:
                    p["status"] = new_status
                    p["resolved_at"] = self._clock().isoformat()
                    break
            logger.info(
                "[council] proposal %s resolved → %s (no operatives responded)",
                proposal_id, new_status,
            )
            return new_status

        # User explicit override
        if user_vote is not None:
            uv = user_vote.strip().upper()
            if uv == "YES":
                new_status = STATUS_PASSED
            elif uv == "NO":
                new_status = STATUS_OVERRIDDEN_BY_USER
            else:
                new_status = STATUS_IN_VOTING
        else:
            # Check TTL
            expires_at_str = proposal.get("expires_at")
            if expires_at_str:
                try:
                    expires_at = datetime.fromisoformat(expires_at_str)
                    # Normalise tz
                    if expires_at.tzinfo is None:
                        expires_at = expires_at.replace(tzinfo=timezone.utc)
                    if now.tzinfo is None:
                        now = now.replace(tzinfo=timezone.utc)
                    if now >= expires_at:
                        new_status = STATUS_DEFERRED
                    elif yes_count >= 2:
                        new_status = STATUS_IN_VOTING  # sufficient votes, awaiting user
                    else:
                        new_status = STATUS_IN_VOTING
                except ValueError:
                    new_status = STATUS_IN_VOTING
            elif yes_count >= 2:
                new_status = STATUS_IN_VOTING
            else:
                new_status = STATUS_IN_VOTING

        # Update proposal in state
        proposals: list[dict] = self.state.get("council_proposals", [])
        for p in proposals:
            if p.get("id") == proposal_id:
                p["status"] = new_status
                if new_status not in (STATUS_IN_VOTING,):
                    p["resolved_at"] = self._clock().isoformat()
                break

        logger.info(
            "[council] proposal %s resolved → %s (yes_votes=%d user_vote=%s)",
            proposal_id, new_status, yes_count, user_vote,
        )
        return new_status

    # ── nash_utility_score ────────────────────────────────────────────────────

    def nash_utility_score(self, votes: dict) -> float:
        """
        Compute the Nash bargaining utility score for a set of votes.

        The Nash solution maximises the geometric mean of utility improvements
        above the disagreement point (status quo = 0.5).

        Args:
            votes: dict of {operative_id: {"vote": ..., "utility": float, ...}}

        Returns:
            float in [0, 1] — geometric mean of utility scores for YES votes.
            Returns 0.0 if no operative voted YES.
        """
        yes_utilities = [
            float(v.get("utility", 0.5))
            for v in votes.values()
            if v.get("vote") == "YES"
        ]

        if not yes_utilities:
            return 0.0

        # Geometric mean
        log_sum = sum(math.log(max(u, 1e-9)) for u in yes_utilities)
        return math.exp(log_sum / len(yes_utilities))

    # ── Internal helpers ───────────────────────────────────────────────────────

    def _get_proposal(self, proposal_id: str) -> dict | None:
        """Retrieve a proposal dict from state by ID."""
        proposals: list[dict] = self.state.get("council_proposals", [])
        for p in proposals:
            if p.get("id") == proposal_id:
                return p
        return None
            Returns 0.0 if no operative voted YES.
        """
        yes_utilities = [
            float(v.get("utility", 0.5))
            for v in votes.values()
            if v.get("vote") == "YES"
        ]

        if not yes_utilities:
            return 0.0

        # Geometric mean
        log_sum = sum(math.log(max(u, 1e-9)) for u in yes_utilities)
        return math.exp(log_sum / len(yes_utilities))

    # ── Internal helpers ───────────────────────────────────────────────────────

    def _get_proposal(self, proposal_id: str) -> dict | None:
        """Retrieve a proposal dict from state by ID."""
        proposals: list[dict] = self.state.get("council_proposals", [])
        for p in proposals:
            if p.get("id") == proposal_id:
                return p
        return None
