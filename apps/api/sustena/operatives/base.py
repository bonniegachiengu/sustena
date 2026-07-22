"""
sustena/operatives/base.py

BaseOperative — abstract base class for all Sustena operatives.

An operative is an AI agent that watches a sustain's state, proposes actions,
and votes on Council proposals. Operatives never call operators directly —
they create proposals that go through the CouncilSession for ratification.

Design principles:
  - should_evaluate() is ALWAYS cheap: threshold check only, zero LLM calls.
  - evaluate() runs evaluation_graph if set; subclasses may override instead.
  - deliberate() runs deliberation_graph if set; subclasses may override instead.
  - _call_claude() wraps the Anthropic client with retry + cost logging.

Graph-based operatives (Sprint 5+):
  Set self.evaluation_graph and/or self.deliberation_graph (OperativeGraph instances)
  to replace LLM-dependent evaluate/deliberate with deterministic operator graphs.
  LLM is still available as an explicit llm.* node in the graph spec.

Model convention:
  HAIKU  = claude-haiku-4-5-20251001 (fast, cheap — default for operatives)
  SONNET = claude-sonnet-4-6 (strategic reasoning — use sparingly)
"""

import asyncio
import json
import logging
import uuid
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from typing import TYPE_CHECKING, Any

from sustena.config import settings
from sustena.core.state import StateAccessor

if TYPE_CHECKING:
    from sustena.core.operative_graph import OperativeGraph

logger = logging.getLogger(__name__)

# ── Model constants ────────────────────────────────────────────────────────────

HAIKU  = settings.claude_haiku_model   # "claude-haiku-4-5-20251001"
SONNET = settings.claude_sonnet_model  # "claude-sonnet-4-6"

# ── Vote types ─────────────────────────────────────────────────────────────────


class VoteChoice(str, Enum):
    YES     = "YES"
    NO      = "NO"
    ABSTAIN = "ABSTAIN"


@dataclass
class OperativeVote:
    """
    The output of deliberate().

    vote      : YES | NO | ABSTAIN
    reasoning : LLM-generated explanation (stored in council_votes.reasoning)
    utility   : float 0..1 — how much this proposal improves the operative's
                utility function (used by nash_utility_score). 0.5 = neutral.
    """
    vote:      VoteChoice
    reasoning: str
    utility:   float = 0.5

    def to_dict(self) -> dict:
        return {
            "vote":      self.vote.value,
            "reasoning": self.reasoning,
            "utility":   self.utility,
        }


@dataclass
class OperativeProposal:
    """
    The output of evaluate() — a suggested operator call for the Council.

    operator_name    : the operator to invoke if passed, e.g. "budget.reallocate"
    input_params     : kwargs to pass to the operator
    rationale        : why the operative is proposing this
    simulation_results: optional forward simulation snapshot
    """
    operator_name:      str
    input_params:       dict
    rationale:          str
    simulation_results: dict = field(default_factory=dict)

    def to_dict(self) -> dict:
        return {
            "operator_name":      self.operator_name,
            "input_params":       self.input_params,
            "rationale":          self.rationale,
            "simulation_results": self.simulation_results,
        }


# ── BaseOperative ──────────────────────────────────────────────────────────────


class BaseOperative(ABC):
    """
    Abstract base class for Sustena operatives.

    Subclasses must implement:
      - should_evaluate(state) → bool (fast threshold check, NO LLM)

    Subclasses may either override or provide graphs for:
      - evaluate(trigger_event) → OperativeProposal | None
      - deliberate(context, proposal) → OperativeVote

    Graph-based path (Sprint 5):
      Set evaluation_graph / deliberation_graph in __init__ and the base class
      methods dispatch to them automatically. No LLM required.

    LLM-based path (legacy / optional node):
      Override evaluate() / deliberate() directly, or include llm.* nodes in graphs.

    Subclasses inherit:
      - _call_claude(...) → str (LLM call with retry + cost logging)
      - _build_state_context(paths) → str (readable state context for prompts)
      - _build_operator_context() → OperatorContext (for graph execution)
    """

    #: Subclasses set this to identify themselves in logs and council_votes.
    operative_id: str = "base_operative"

    def __init__(
        self,
        config: dict,
        state_accessor: StateAccessor,
        claude_client: Any,
        sustain_id: str = "unknown",
        user_id: str = "system",
    ) -> None:
        """
        Args:
            config: Operative-specific config (thresholds, prompts, etc.)
            state_accessor: StateAccessor bound to the sustain being watched.
            claude_client:  Anthropic client (real or mock). Exposes .messages.create().
            sustain_id:     Sustain this operative belongs to (used in OperatorContext).
            user_id:        User context for OperatorContext (default: "system").
        """
        self.config        = config
        self.state         = state_accessor
        self.claude_client = claude_client
        self.sustain_id    = sustain_id
        self.user_id       = user_id
        self._log_entries: list[dict] = []

        # Graph-based evaluate / deliberate (Sprint 5).
        # Set these in subclass __init__ to replace LLM-dependent methods.
        self.evaluation_graph:    "OperativeGraph | None" = None
        self.deliberation_graph:  "OperativeGraph | None" = None

    # ── Abstract interface ─────────────────────────────────────────────────────

    @abstractmethod
    def should_evaluate(self, state: StateAccessor) -> bool:
        """
        Cheap threshold check — MUST NOT call the LLM.

        Return True only when a measurable threshold in `state` is breached
        (e.g., a pocket is <20% of allocation). Called on every relevant event.
        """
        ...

    # ── Graph-backed evaluate / deliberate ────────────────────────────────────

    async def evaluate(self, trigger_event: dict) -> "OperativeProposal | None":
        """
        Analyse the trigger event and optionally propose an operator call.

        Default: runs self.evaluation_graph if set.
        Subclasses may override for custom (including LLM-based) logic.
        Returns None if no action is warranted.
        """
        if self.evaluation_graph is not None:
            ctx = self._build_operator_context()
            return await self.evaluation_graph.run(ctx, trigger_event)
        raise NotImplementedError(
            f"{self.__class__.__name__} must either override evaluate() "
            "or set self.evaluation_graph"
        )

    async def deliberate(
        self,
        context: dict,
        proposal: dict,
    ) -> OperativeVote:
        """
        Vote on a Council proposal.

        Default: runs self.deliberation_graph if set; extracts vote/utility from
        the exit node's result data.
        Subclasses may override for custom (including LLM-based) logic.
        """
        if self.deliberation_graph is not None:
            ctx = self._build_operator_context()
            graph_proposal = await self.deliberation_graph.run(ctx, proposal)
            exit_data = graph_proposal.simulation_results.get(
                self.deliberation_graph.exit_node, {}
            )
            vote_str = str(exit_data.get("vote", "ABSTAIN")).upper()
            try:
                vote = VoteChoice(vote_str)
            except ValueError:
                vote = VoteChoice.ABSTAIN
            reasoning = exit_data.get("reasoning", graph_proposal.rationale)
            utility = float(exit_data.get("utility", 0.5))
            utility = max(0.0, min(1.0, utility))
            return OperativeVote(vote=vote, reasoning=reasoning, utility=utility)
        raise NotImplementedError(
            f"{self.__class__.__name__} must either override deliberate() "
            "or set self.deliberation_graph"
        )

    # ── OperatorContext builder ───────────────────────────────────────────────

    def _build_operator_context(self):
        """Build a minimal OperatorContext for graph execution (no DB, no Firestore)."""
        from sustena.core.events import EventBus
        from sustena.core.operator import OperatorContext
        from sustena.core.pawa import PawaLedger

        return OperatorContext(
            state=self.state,
            events=EventBus(sustain_id=self.sustain_id),
            pawa=PawaLedger(),
            sustain_id=self.sustain_id,
            user_id=self.user_id,
            operative_id=self.operative_id,
            timestamp=datetime.utcnow(),
        )

    # ── LLM wrapper ───────────────────────────────────────────────────────────

    async def _call_claude(
        self,
        system_prompt: str,
        user_message: str,
        model: str = HAIKU,
        max_tokens: int = 512,
    ) -> str:
        """
        Call Claude with retry logic (3 attempts, exponential backoff).

        Logs token usage and model to self._log_entries (flushed to operators_log
        by the operative runner). Returns the text of the first content block.

        Raises RuntimeError if all 3 attempts fail.
        """
        max_attempts = 3
        last_exc: Exception | None = None

        for attempt in range(max_attempts):
            try:
                response = await self.claude_client.messages.create(
                    model=model,
                    max_tokens=max_tokens,
                    system=system_prompt,
                    messages=[{"role": "user", "content": user_message}],
                )
                text = response.content[0].text if response.content else ""

                # Token counting + cost logging
                input_tokens  = getattr(response.usage, "input_tokens",  0)
                output_tokens = getattr(response.usage, "output_tokens", 0)
                total_tokens  = input_tokens + output_tokens

                log_entry = {
                    "id":            str(uuid.uuid4()),
                    "operative_id":  self.operative_id,
                    "operator_name": f"llm_call:{self.operative_id}",
                    "model":         model,
                    "input_tokens":  input_tokens,
                    "output_tokens": output_tokens,
                    "llm_tokens_used": total_tokens,
                    "status":        "ok",
                    "attempt":       attempt + 1,
                }
                self._log_entries.append(log_entry)

                logger.debug(
                    "[%s] LLM call OK model=%s tokens=%d (attempt %d)",
                    self.operative_id, model, total_tokens, attempt + 1,
                )
                return text

            except Exception as exc:
                last_exc = exc
                wait = 2 ** attempt  # 1s, 2s, 4s
                logger.warning(
                    "[%s] LLM call failed (attempt %d/%d): %s — retrying in %ds",
                    self.operative_id, attempt + 1, max_attempts, exc, wait,
                )
                await asyncio.sleep(wait)

        # All attempts exhausted
        error_entry = {
            "id":            str(uuid.uuid4()),
            "operative_id":  self.operative_id,
            "operator_name": f"llm_call:{self.operative_id}",
            "model":         model,
            "status":        "failed",
            "failure_reason": str(last_exc),
        }
        self._log_entries.append(error_entry)
        raise RuntimeError(
            f"[{self.operative_id}] LLM call failed after {max_attempts} attempts: {last_exc}"
        )

    # ── State context builder ─────────────────────────────────────────────────

    def _build_state_context(self, paths: list[str]) -> str:
        """
        Read the specified dot-paths from state and format them as a
        human-readable context block for use in LLM system prompts.

        Example:
            paths = ["finances.liquid.balance", "finances.pockets"]
            → "finances.liquid.balance: 45000.0\nfinances.pockets: {...}\n"
        """
        lines: list[str] = []
        for path in paths:
            value = self.state.get(path)
            if isinstance(value, (dict, list)):
                try:
                    formatted = json.dumps(value, indent=2)
                except (TypeError, ValueError):
                    formatted = str(value)
                lines.append(f"{path}:\n{formatted}")
            else:
                lines.append(f"{path}: {value}")
        return "\n".join(lines)

    # ── Log access ────────────────────────────────────────────────────────────

    def flush_log_entries(self) -> list[dict]:
        """Return and clear all accumulated log entries (for the runner to persist)."""
        entries = list(self._log_entries)
        self._log_entries.clear()
        return entries
