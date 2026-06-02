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
from pathlib import Path
from typing import Any

from sustena.operatives.base import OperativeVote, VoteChoice

logger = logging.getLogger(__name__)

# Graph files live at sustena/operatives/graphs/ — council.py is in sustena/core/
_OPERATIVES_DIR: Path = Path(__file__).parent.parent / "operatives"


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

    def is_relevant(self, proposal_domains: list[str]) -> bool:
        """
        Return True if this councillor should participate in evaluating a proposal.

        Rules:
          - proposal_domains is empty → proposal is untagged (applies to all) → True
          - councillor has no declared domain → always False (will ABSTAIN)
          - otherwise → True iff any councillor domain appears in proposal_domains
        """
        if not proposal_domains:
            return True
        if not self.domain:
            return False
        return bool(set(self.domain) & set(proposal_domains))


# ── Operator domain map ────────────────────────────────────────────────────────

OPERATOR_DOMAIN_MAP: dict[str, list[str]] = {
    # Finance / budget
    "budget.record_income":              ["finances", "budget", "savings"],
    "budget.allocate":                   ["finances", "budget"],
    "budget.spend":                      ["finances", "budget"],
    "budget.transfer":                   ["finances", "budget"],
    "budget.summary":                    ["finances", "budget"],
    "budget.reallocate":                 ["finances", "budget"],
    # Calendar / time
    "homestead.calendar.add_event":       ["calendar", "time"],
    "homestead.calendar.upcoming_events": ["calendar", "time"],
    "homestead.calendar.remove_event":    ["calendar", "time"],
    # Tasks / deadlines
    "homestead.tasks.add":               ["tasks", "deadlines"],
    "homestead.tasks.complete":          ["tasks"],
    "homestead.tasks.list":              ["tasks"],
    "homestead.tasks.carryover":         ["tasks", "deadlines"],
    # Procurement / assets / inventory
    "procurement.raise_po":              ["procurement", "assets", "inventory"],
    "procurement.confirm_delivery":      ["procurement", "logistics", "delivery", "transport"],
    "mkulima.broadcast_supply_signal":   ["procurement", "assets"],
    "mkulima.receive_signal":            ["procurement"],
}


def get_proposal_domains(operator_name: str) -> list[str]:
    """
    Return the domain tags for an operator name.

    Unknown operators return an empty list — their proposals are treated as
    untagged (relevant to all councillors).
    """
    return list(OPERATOR_DOMAIN_MAP.get(operator_name, []))


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


# ── DelegatedVote — sub-operative vote aggregation (Sprint 7.4) ───────────────


@dataclass
class DelegatedVote:
    """
    A vote cast by a single sub-operative inside a councillor's sandbox.

    position   : YES | NO | ABSTAIN
    confidence : 0.0–1.0 — how certain the sub-operative is
    reasoning  : free-text explanation from the sub-operative's simulation result
    """

    position:   VoteChoice
    confidence: float
    reasoning:  str


def aggregate_delegated_votes(delegated: list[DelegatedVote]) -> OperativeVote:
    """
    Aggregate a list of DelegatedVotes into a single OperativeVote.

    Rules:
      - ABSTAIN votes carry weight=0 and are excluded from weighted sums
        (unless ALL votes are ABSTAIN).
      - weighted_yes = Σ confidence for YES votes
      - weighted_no  = Σ confidence for NO votes
      - YES wins if weighted_yes > weighted_no
      - NO wins  if weighted_no > weighted_yes
      - Tie or all-ABSTAIN → ABSTAIN
      - utility = mean of YES confidences; 0.5 if no YES votes
      - reasoning = all individual reasonings joined by " | "
    """
    if not delegated:
        return OperativeVote(
            vote=VoteChoice.ABSTAIN,
            reasoning="No delegated votes.",
            utility=0.5,
        )

    reasoning = " | ".join(d.reasoning for d in delegated)

    yes_votes = [d for d in delegated if d.position == VoteChoice.YES]
    no_votes  = [d for d in delegated if d.position == VoteChoice.NO]

    # All-ABSTAIN shortcut
    if not yes_votes and not no_votes:
        return OperativeVote(
            vote=VoteChoice.ABSTAIN,
            reasoning=reasoning,
            utility=0.5,
        )

    weighted_yes = sum(d.confidence for d in yes_votes)
    weighted_no  = sum(d.confidence for d in no_votes)

    if weighted_yes > weighted_no:
        final_vote = VoteChoice.YES
    elif weighted_no > weighted_yes:
        final_vote = VoteChoice.NO
    else:
        final_vote = VoteChoice.ABSTAIN

    utility = sum(d.confidence for d in yes_votes) / len(yes_votes) if yes_votes else 0.5

    return OperativeVote(vote=final_vote, reasoning=reasoning, utility=utility)


def _parse_sandbox_to_delegated_votes(sandbox_results: dict) -> list[DelegatedVote]:
    """
    Convert raw sandbox_results dict into a list of DelegatedVotes.

    For each sub-operative entry:
      - status != "ok"  → DelegatedVote(ABSTAIN, 0.0, reason_string)
      - status == "ok"  → read vote/confidence/reasoning from simulation_results;
                          absent keys default to ABSTAIN / 0.5 / ""
    """
    delegated: list[DelegatedVote] = []

    for sub_op_name, result in sandbox_results.items():
        status = result.get("status", "error")
        if status != "ok":
            reason = result.get("reason", f"sub-op '{sub_op_name}' status={status}")
            delegated.append(DelegatedVote(
                position=VoteChoice.ABSTAIN,
                confidence=0.0,
                reasoning=reason,
            ))
        else:
            sim = result.get("simulation_results") or {}
            raw_vote = sim.get("vote", "ABSTAIN")
            try:
                position = VoteChoice(str(raw_vote).upper())
            except ValueError:
                position = VoteChoice.ABSTAIN
            confidence = float(sim.get("confidence", 0.5))
            reasoning  = str(sim.get("reasoning", ""))
            delegated.append(DelegatedVote(
                position=position,
                confidence=confidence,
                reasoning=reasoning,
            ))

    return delegated


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
        domains: list[str] | None = None,
    ) -> str:
        """
        Create a new Council proposal and persist it to state.

        Args:
            sustain_id        : sustain context (passed through for clarity)
            proposed_by       : operative_id or 'orchie'
            operator_name     : e.g. "budget.reallocate"
            input_params      : kwargs for the operator
            simulation_results: optional forward-simulation output
            domains           : domain tags for this proposal. If None, auto-derived
                                from OPERATOR_DOMAIN_MAP using operator_name. Pass an
                                explicit list to override (e.g. for cross-domain proposals).

        Returns:
            proposal_id (UUID string)
        """
        now        = self._clock()
        expires_at = now + timedelta(hours=PROPOSAL_TTL_HOURS)

        resolved_domains = domains if domains is not None else get_proposal_domains(operator_name)

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
            "domains":                resolved_domains,
        }

        self.state.append("council_proposals", proposal)
        logger.info(
            "[council] proposal created: id=%s operator=%s proposed_by=%s domains=%s",
            proposal["id"], operator_name, proposed_by, resolved_domains,
        )
        return proposal["id"]

    # ── collect_votes ─────────────────────────────────────────────────────────

    async def collect_votes(
        self,
        proposal_id: str,
        operatives: list[Any],   # list[BaseOperative]
        councillor_configs: dict | None = None,  # dict[str, CouncillorConfig] | None
    ) -> dict:
        """
        Ask each operative to deliberate and record their vote.

        Domain relevance check (Sprint 7.2):
          If councillor_configs is provided and an operative's config declares a domain
          list that does not overlap the proposal's tagged domains, that operative
          receives an immediate ABSTAIN with abstain_reason="no_domain_overlap" — its
          deliberate() method is never called and no sub-operatives are spawned.

        Sandbox simulation (Sprint 7.3):
          Each relevant councillor (domain overlap ≥ 1) that declares sub_operatives
          gets its own isolated simulate.fork() — a deep-copy of current state. Its
          sub-operative graphs run against that fork. No two councillors share a fork.
          The fork_id and sandbox_results are recorded on the vote record and passed
          to deliberate() in the enriched proposal dict.

        Args:
            proposal_id        : Council proposal to vote on.
            operatives         : Operative instances to poll.
            councillor_configs : Optional mapping of operative_id → CouncillorConfig.
                                 When None, all operatives deliberate (backwards compat).

        Returns:
            dict mapping operative_id → OperativeVote dict (includes fork_id when set).
        """
        proposal = self._get_proposal(proposal_id)
        if proposal is None:
            raise ValueError(f"Proposal '{proposal_id}' not found in state.")

        # Record that collect_votes was invoked for this proposal
        self._votes_collected.add(proposal_id)

        proposal_domains: list[str] = proposal.get("domains", [])

        context = {
            "sustain_id":    self.sustain_id,
            "proposal_id":   proposal_id,
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
            op_id = getattr(operative, "operative_id", "unknown")

            # Resolve CouncillorConfig for this operative (None if configs not provided)
            config: "CouncillorConfig | None" = (
                councillor_configs.get(op_id) if councillor_configs is not None else None
            )

            # ── Domain relevance check (7.2) ───────────────────────────────────
            if config is not None and not config.is_relevant(proposal_domains):
                vote_record = {
                    "id":             str(uuid.uuid4()),
                    "proposal_id":    proposal_id,
                    "operative_id":   op_id,
                    "vote":           "ABSTAIN",
                    "reasoning":      "Domain not relevant to this proposal.",
                    "abstain_reason": "no_domain_overlap",
                    "weight":         0.098,
                    "timestamp":      self._clock().isoformat(),
                    "utility":        0.5,
                    "fork_id":        None,
                }
                self.state.append("council_votes", vote_record)
                vote_results[op_id] = {
                    "vote":           "ABSTAIN",
                    "reasoning":      "Domain not relevant to this proposal.",
                    "abstain_reason": "no_domain_overlap",
                    "utility":        0.5,
                    "fork_id":        None,
                }
                logger.info(
                    "[council] operative '%s' ABSTAIN (no domain overlap): "
                    "proposal=%s domains=%s",
                    op_id, proposal_id, proposal_domains,
                )
                continue  # skip deliberate() — zero sub-operatives spawned

            # ── Per-councillor sandbox fork (7.3) ──────────────────────────────
            fork_id: str | None = None
            sandbox_results: dict = {}
            if config is not None and config.sub_operatives:
                fork_id, sandbox_results = await self._create_sandbox(config)

            # Enrich the proposal dict passed to deliberate() with sandbox output
            enriched_proposal: dict = dict(proposal_dict)
            if sandbox_results:
                enriched_proposal["sandbox_results"] = sandbox_results
            if fork_id:
                enriched_proposal["fork_id"] = fork_id

            # ── DelegatedVote aggregation (7.5) ───────────────────────────────
            delegated = (
                _parse_sandbox_to_delegated_votes(sandbox_results)
                if sandbox_results
                else []
            )
            delegated_votes_record = (
                [
                    {
                        "position":   d.position.value,
                        "confidence": d.confidence,
                        "reasoning":  d.reasoning,
                    }
                    for d in delegated
                ]
                if delegated
                else None
            )

            if delegated:
                # Sub-operatives produced votes — aggregate and skip deliberate()
                vote = aggregate_delegated_votes(delegated)
                vote_record = {
                    "id":              str(uuid.uuid4()),
                    "proposal_id":     proposal_id,
                    "operative_id":    op_id,
                    "vote":            vote.vote.value,
                    "reasoning":       vote.reasoning,
                    "weight":          0.098,
                    "timestamp":       self._clock().isoformat(),
                    "utility":         vote.utility,
                    "fork_id":         fork_id,
                    "sandbox_results": sandbox_results or None,
                    "delegated_votes": delegated_votes_record,
                }
                self.state.append("council_votes", vote_record)
                vote_result = vote.to_dict()
                vote_result["fork_id"] = fork_id
                vote_result["delegated_votes"] = delegated_votes_record
                if sandbox_results:
                    vote_result["sandbox_results"] = sandbox_results
                vote_results[op_id] = vote_result
                logger.info(
                    "[council] vote recorded (delegated): proposal=%s operative=%s "
                    "vote=%s fork=%s delegated=%d",
                    proposal_id, op_id, vote.vote.value, fork_id, len(delegated),
                )
            else:
                # No delegated votes — fall through to operative.deliberate()
                try:
                    vote = await operative.deliberate(context, enriched_proposal)
                    vote_record = {
                        "id":              str(uuid.uuid4()),
                        "proposal_id":     proposal_id,
                        "operative_id":    op_id,
                        "vote":            vote.vote.value,
                        "reasoning":       vote.reasoning,
                        "weight":          0.098,
                        "timestamp":       self._clock().isoformat(),
                        "utility":         vote.utility,
                        "fork_id":         fork_id,
                        "sandbox_results": sandbox_results or None,
                        "delegated_votes": None,
                    }
                    self.state.append("council_votes", vote_record)
                    vote_result = vote.to_dict()
                    vote_result["fork_id"] = fork_id
                    if sandbox_results:
                        vote_result["sandbox_results"] = sandbox_results
                    vote_results[op_id] = vote_result
                    logger.info(
                        "[council] vote recorded: proposal=%s operative=%s vote=%s fork=%s",
                        proposal_id, op_id, vote.vote.value, fork_id,
                    )
                except Exception as exc:
                    logger.error(
                        "[council] operative '%s' deliberation error: %s", op_id, exc,
                    )
                    vote_record = {
                        "id":              str(uuid.uuid4()),
                        "proposal_id":     proposal_id,
                        "operative_id":    op_id,
                        "vote":            "ABSTAIN",
                        "reasoning":       f"Deliberation error: {exc}",
                        "weight":          0.098,
                        "timestamp":       self._clock().isoformat(),
                        "utility":         0.5,
                        "fork_id":         fork_id,
                        "sandbox_results": None,
                        "delegated_votes": None,
                    }
                    self.state.append("council_votes", vote_record)
                    vote_results[op_id] = {
                        "vote":      "ABSTAIN",
                        "reasoning": str(exc),
                        "utility":   0.5,
                        "fork_id":   fork_id,
                    }

        return vote_results

    # ── Sandbox helpers (Sprint 7.3) ──────────────────────────────────────────

    def _build_ctx(self) -> Any:
        """Build a minimal OperatorContext from this session's state for operator calls."""
        from sustena.core.events import EventBus
        from sustena.core.operator import OperatorContext
        from sustena.core.pawa import PawaLedger
        return OperatorContext(
            state=self.state,
            events=EventBus(sustain_id=self.sustain_id),
            pawa=PawaLedger(),
            sustain_id=self.sustain_id,
            user_id="council",
            operative_id="council_session",
            timestamp=datetime.now(timezone.utc),
        )

    async def _create_sandbox(
        self,
        config: "CouncillorConfig",
    ) -> "tuple[str | None, dict]":
        """
        Fork the current state and run all of config's sub-operatives against it.

        Each councillor calling this method gets its own fork_id — forks are never
        shared between councillors. Sub-operatives run against the fork's isolated
        state via a dedicated OperatorContext.

        Returns:
            (fork_id, sandbox_results) where sandbox_results maps
            sub_op_name → result dict. fork_id is None if forking failed.
        """
        from sustena.core.events import EventBus
        from sustena.core.operative_graph import CalibrationError, OperativeGraph
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext
        from sustena.core.pawa import PawaLedger
        from sustena.core.state import StateAccessor
        from sustena.operators.simulate_ops import get_fork_state

        # Step 1: Fork the live state (snapshot → stored in _FORK_REGISTRY)
        try:
            fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(self._build_ctx())
        except Exception as exc:
            logger.warning("[council] simulate.fork failed for '%s': %s",
                           config.operative_id, exc)
            return None, {}

        if not fork_result.succeeded:
            logger.warning("[council] simulate.fork returned fail for '%s': %s",
                           config.operative_id, fork_result.reason)
            return None, {}

        fork_id: str = fork_result.data["fork_id"]
        fork_state_dict = get_fork_state(fork_id)  # deep copy of the forked state

        # Step 2: Run each sub-operative graph against the fork
        sandbox_results: dict = {}
        for sub_op_name, graph_rel_path in config.sub_operatives.items():
            full_path = _OPERATIVES_DIR / graph_rel_path
            try:
                sub_state = (
                    StateAccessor(fork_state_dict)
                    if fork_state_dict is not None
                    else self.state
                )
                sub_ctx = OperatorContext(
                    state=sub_state,
                    events=EventBus(
                        sustain_id=f"sandbox:{config.operative_id}:{self.sustain_id}"
                    ),
                    pawa=PawaLedger(),
                    sustain_id=self.sustain_id,
                    user_id="council",
                    operative_id=f"sub_op:{sub_op_name}",
                    timestamp=datetime.now(timezone.utc),
                )
                graph = OperativeGraph.from_spec_file(
                    full_path,
                    calibration_data={"fork_id": fork_id},
                )
                result = await graph.run(
                    sub_ctx, {"fork_id": fork_id, "sub_op": sub_op_name}
                )
                sandbox_results[sub_op_name] = {
                    "status":             "ok",
                    "operator_name":      result.operator_name,
                    "rationale":          result.rationale,
                    "simulation_results": result.simulation_results,
                }
            except CalibrationError as exc:
                logger.warning("[council] sub-op '%s' calibration error: %s",
                               sub_op_name, exc)
                sandbox_results[sub_op_name] = {
                    "status": "calibration_error", "reason": str(exc)
                }
            except FileNotFoundError as exc:
                logger.warning("[council] sub-op '%s' graph not found: %s",
                               sub_op_name, exc)
                sandbox_results[sub_op_name] = {
                    "status": "not_found", "reason": str(exc)
                }
            except Exception as exc:
                logger.warning("[council] sub-op '%s' error: %s", sub_op_name, exc)
                sandbox_results[sub_op_name] = {"status": "error", "reason": str(exc)}

        logger.info(
            "[council] sandbox: operative=%s fork=%s sub_ops=%s",
            config.operative_id, fork_id, list(sandbox_results),
        )
        return fork_id, sandbox_results

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
