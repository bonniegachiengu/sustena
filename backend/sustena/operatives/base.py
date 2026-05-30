"""
sustena/operatives/base.py

BaseOperative — abstract base class for all Sustena operatives.

An operative is an AI agent that watches a sustain's state, proposes actions,
and votes on Council proposals. Operatives never call operators directly —
they create proposals that go through the CouncilSession for ratification.

Design principles:
  - should_evaluate() is ALWAYS cheap: threshold check only, zero LLM calls.
  - evaluate() is called only when should_evaluate() returns True.
  - deliberate() votes on a proposal using LLM reasoning.
  - _call_claude() wraps the Anthropic client with retry + cost logging.

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
from enum import Enum
from typing import Any

from sustena.config import settings
from sustena.core.state import StateAccessor

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
      - evaluate(trigger_event) → OperativeProposal | None
      - deliberate(context, proposal) → OperativeVote

    Subclasses inherit:
      - _call_claude(...) → str (LLM call with retry + cost logging)
      - _build_state_context(paths) → str (readable state context for prompts)
    """

    #: Subclasses set this to identify themselves in logs and council_votes.
    operative_id: str = "base_operative"

    def __init__(
        self,
        config: dict,
        state_accessor: StateAccessor,
        claude_client: Any,
    ) -> None:
        """
        Args:
            config: Operative-specific config (thresholds, prompts, etc.)
            state_accessor: StateAccessor bound to the sustain being watched.
            claude_client:  Anthropic client (real or mock). Must expose
                            .messages.create(**kwargs) coroutine.
        """
        self.config         = config
        self.state          = state_accessor
        self.claude_client  = claude_client
        self._log_entries: list[dict] = []   # in-memory operators_log sink

    # ── Abstract interface ─────────────────────────────────────────────────────

    @abstractmethod
    def should_evaluate(self, state: StateAccessor) -> bool:
        """
        Cheap threshold check — MUST NOT call the LLM.

        Return True only when a measurable threshold in `state` is breached
        (e.g., a pocket is <20% of allocation). The operative runner calls this
        on every relevant event; calling the LLM here would be prohibitively
        expensive and slow.
        """
        ...

    @abstractmethod
    async def evaluate(self, trigger_event: dict) -> "OperativeProposal | None":
        """
        Analyse the trigger event and optionally propose an operator call.

        Called only when should_evaluate() returns True. May use _call_claude()
        for reasoning. Returns None if no action is warranted.
        """
        ...

    @abstractmethod
    async def deliberate(
        self,
        context: dict,
        proposal: dict,
    ) -> OperativeVote:
        """
        Vote on a Council proposal.

        Args:
            context: Snapshot of relevant state sections.
            proposal: The OperativeProposal (or council_proposals row) as dict.

        Returns:
            OperativeVote with YES/NO/ABSTAIN + reasoning.
        """
        ...

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
