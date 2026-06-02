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
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any

logger = logging.getLogger(__name__)


# ── CouncillorConfig ───────────────────────────────────────────────────────────


@dataclass
class CouncillorConfig:
    """
    Configuration for a single councillor (operative) within a sustain.

    Loaded from the 'operatives' section of a sustain JSON spec.

    domain         : domain tags this councillor covers (e.g. ["finances", "budget"]).
                     Used at runtime to determine relevance to a given proposal.
                     Empty list → councillor has no declared domain → will always ABSTAIN.
    sub_operatives : mapping of name → graph file path for sub-operative templates.
                     Each template is a library graph calibrated and spawned during
                     sandbox evaluation (Sprint 7.3+).
                     Empty dict → councillor is relevant but spawns no sub-operatives.
    """

    operative_id:   str
    domain:         list[str] = field(default_factory=list)
    sub_operatives: dict[str, str] = field(default_factory=dict)


def load_councillor_configs(operatives_spec: dict) -> dict[str, "CouncillorConfig"]:
    """
    Parse the 'operatives' section of a sustain spec into CouncillorConfig instances.

    Each entry may include optional 'domain' and 'sub_operatives' keys alongside
    'class', 'evaluation_graph', and 'deliberation_graph'. Missing keys get safe
    empty defaults.

    Args:
        operatives_spec : dict keyed by operative_id, values are per-operative dicts.

    Returns:
        dict mapping operative_id → CouncillorConfig.
    """
    configs: dict[str, CouncillorConfig] = {}
    for op_id, spec in operatives_spec.items():
        configs[op_id] = CouncillorConfig(
            operative_id=op_id,
            domain=list(spec.get("domain", [])),
            sub_operatives=dict(spec.get("sub_operatives", {})),
        )
    return configs


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


# ══════════════════════════════════════════════════════════════════════════════
# Chama-specific governance extensions
# ══════════════════════════════════════════════════════════════════════════════
#
# Chama DAO governance differs from the standard Sustena Council:
#
#   Standard Council: operatives deliberate (AI agents vote), user ratifies.
#   Chama Council:    human members vote (YES/NO via WhatsApp), Secretary has
#                     veto power on structural changes only.
#
# Proposal types:
#   "chama.financial"  — loan approvals, fines, expense approvals.
#                        Requires 60% quorum + simple majority of YES votes.
#   "chama.structural" — rule changes, membership changes, rotation amendments.
#                        Requires ALL members minus 1 to vote YES.
#                        Secretary has veto: any NO from Secretary → FAILED.
#
# The Secretary (ChamaSecretaryOperative) does NOT vote on proposals —
# she creates them. Secretary veto is a separate explicit mechanism.
# ══════════════════════════════════════════════════════════════════════════════


    def create_chama_proposal(
        self,
        chama_id: str,
        proposal_type: str,
        proposed_by: str,
        details: dict,
    ) -> str:
        """
        Create a Chama DAO governance proposal.

        Args:
            chama_id      : ID of the chama (sustain instance)
            proposal_type : "chama.financial" | "chama.structural"
            proposed_by   : member_id or "chama_secretary"
            details       : proposal-specific data (amount, reason, rule change, etc.)

        Returns:
            proposal_id (UUID string)

        The proposal is stored in the "proposals" list in chama state (separate
        from the standard council_proposals used by AI operative voting).
        """
        if proposal_type not in ("chama.financial", "chama.structural"):
            raise ValueError(
                f"Invalid chama proposal_type: '{proposal_type}'. "
                "Must be 'chama.financial' or 'chama.structural'."
            )

        now = self._clock()
        proposal_id = str(uuid.uuid4())

        proposal = {
            "proposal_id": proposal_id,
            "chama_id":    chama_id,
            "type":        proposal_type,
            "proposed_by": proposed_by,
            "details":     details,
            "status":      STATUS_IN_VOTING,
            "votes":       {},   # member_id → "YES" | "NO"
            "created_at":  now.isoformat(),
        }

        # Store in chama proposals list (not standard council_proposals)
        if not self.state.exists("proposals"):
            self.state.set("proposals", [])
        self.state.append("proposals", proposal)

        logger.info(
            "[council:chama] proposal created: id=%s type=%s proposed_by=%s",
            proposal_id, proposal_type, proposed_by,
        )
        return proposal_id

    def collect_member_votes(
        self,
        proposal_id: str,
        votes: dict,
    ) -> None:
        """
        Record votes from Chama members for a proposal.

        Votes come in asynchronously (WhatsApp-native) and are stored as they
        arrive. This method merges the provided votes dict into the proposal.

        Args:
            proposal_id : the chama proposal to update
            votes       : dict of {member_id: "YES" | "NO"}

        The proposals list in state is updated in-place. Call resolve_chama()
        after all votes are collected (or on each vote) to check outcome.
        """
        proposals: list = self.state.get("proposals", [])
        proposal = next(
            (p for p in proposals if p.get("proposal_id") == proposal_id),
            None,
        )

        if proposal is None:
            raise ValueError(
                f"Chama proposal '{proposal_id}' not found in state."
            )

        # Merge votes (in-place mutation of the live list entry)
        existing_votes: dict = proposal.get("votes", {})
        for member_id, vote in votes.items():
            vote_normalised = str(vote).strip().upper()
            if vote_normalised not in ("YES", "NO"):
                logger.warning(
                    "[council:chama] invalid vote '%s' from member %s for proposal %s; skipping",
                    vote, member_id, proposal_id,
                )
                continue
            existing_votes[member_id] = vote_normalised
            logger.info(
                "[council:chama] vote recorded: proposal=%s member=%s vote=%s",
                proposal_id, member_id, vote_normalised,
            )
        proposal["votes"] = existing_votes

    def resolve_chama(
        self,
        proposal_id: str,
        member_ids: list,
        secretary_id: str | None = None,
    ) -> str:
        """
        Resolve the outcome of a Chama DAO proposal.

        Resolution rules by proposal type:

        "chama.financial":
          - Quorum = 60% of active member_ids must have voted (YES or NO).
          - If quorum not met → IN_VOTING (still waiting).
          - Simple majority of votes cast must be YES → PASSED.
          - Otherwise → FAILED.

        "chama.structural":
          - ALL members minus 1 must vote YES → PASSED.
            (i.e. at most 1 dissenting or abstaining member allowed)
          - Any NO vote from the Secretary → FAILED (Secretary veto).
          - Otherwise → FAILED if condition not met.

        Secretary veto (structural only):
          - If secretary_id is provided and the Secretary cast a NO vote → FAILED
            immediately, regardless of other votes.

        Args:
            proposal_id  : the chama proposal to resolve
            member_ids   : list of all active member IDs eligible to vote
            secretary_id : optional — if provided, check Secretary veto on structural

        Returns:
            New status string: "PASSED" | "FAILED" | "IN_VOTING"

        Updates the proposal's status in state.
        """
        proposals: list = self.state.get("proposals", [])
        proposal = next(
            (p for p in proposals if p.get("proposal_id") == proposal_id),
            None,
        )

        if proposal is None:
            raise ValueError(
                f"Chama proposal '{proposal_id}' not found in state."
            )

        proposal_type: str = proposal.get("type", "")
        votes: dict        = proposal.get("votes", {})

        total_members = len(member_ids)
        if total_members == 0:
            new_status = STATUS_FAILED
            proposal["status"] = new_status
            return new_status

        yes_votes = sum(1 for v in votes.values() if v == "YES")
        no_votes  = sum(1 for v in votes.values() if v == "NO")
        votes_cast = yes_votes + no_votes

        if proposal_type == "chama.financial":
            quorum_rules: dict = self.state.get("rules", {})
            quorum_threshold: float = float(
                quorum_rules.get("quorum_threshold", 0.6)
            )
            quorum_count = total_members * quorum_threshold

            if votes_cast < quorum_count:
                # Quorum not reached — still waiting
                new_status = STATUS_IN_VOTING
            elif yes_votes > no_votes:
                new_status = STATUS_PASSED
            else:
                new_status = STATUS_FAILED

        elif proposal_type == "chama.structural":
            # Secretary veto: any NO from secretary_id → immediate FAILED
            if secretary_id and votes.get(secretary_id) == "NO":
                new_status = STATUS_FAILED
                proposal["status"] = new_status
                if new_status not in (STATUS_IN_VOTING,):
                    proposal["resolved_at"] = self._clock().isoformat()
                logger.info(
                    "[council:chama] structural proposal %s → FAILED (Secretary veto)",
                    proposal_id,
                )
                return new_status

            # All members minus 1 must vote YES
            # i.e. yes_votes >= total_members - 1  AND  no_votes == 0
            # (abstentions / non-votes count as not-YES)
            min_yes_required = total_members - 1

            if no_votes > 0:
                # Any NO (from non-secretary members) → FAILED
                new_status = STATUS_FAILED
            elif yes_votes >= min_yes_required:
                new_status = STATUS_PASSED
            else:
                # Not enough YES votes yet — still waiting
                new_status = STATUS_IN_VOTING

        else:
            # Unknown type — fail safe
            logger.warning(
                "[council:chama] unknown proposal type '%s' for proposal %s",
                proposal_type, proposal_id,
            )
            new_status = STATUS_FAILED

        proposal["status"] = new_status
        if new_status not in (STATUS_IN_VOTING,):
            proposal["resolved_at"] = self._clock().isoformat()

        logger.info(
            "[council:chama] proposal %s → %s (type=%s yes=%d no=%d total=%d)",
            proposal_id, new_status, proposal_type, yes_votes, no_votes, total_members,
        )
        return new_status
