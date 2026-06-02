"""
sustena/operatives/mentor.py

MentorOperative — budget watchdog operative.

Responsibility:
  - Monitor budget pocket utilisation.
  - When any pocket exceeds 80% of its allocation, propose a reallocation
    or surface an alert to the user via the Council.
  - Vote YES on proposals that improve the budget position; NO if a proposal
    would push liquid balance below 10% of monthly income.

Sprint 5 rewrite: evaluate() and deliberate() now run OperativeGraph instances
rather than calling the LLM directly. No Anthropic API calls unless an llm.*
node is explicitly added to the graph.

should_evaluate() is unchanged — threshold check, zero LLM calls.
"""

import logging
from typing import Any

from sustena.core.operative_graph import OperativeGraph, OperativeNode, OperativeEdge
from sustena.core.state import StateAccessor
from sustena.operatives.base import (
    BaseOperative,
    OperativeProposal,
    OperativeVote,
    VoteChoice,
)

logger = logging.getLogger(__name__)

# ── Thresholds ─────────────────────────────────────────────────────────────────

WARN_THRESHOLD   = 0.20   # should_evaluate triggers at remaining < 20%
ACTION_THRESHOLD = 0.80   # mentor.evaluate_budget acts at spent > 80%
LIQUID_FLOOR     = 0.10   # mentor.deliberate_budget hard-NO below 10% income


def _build_evaluation_graph(action_threshold: float, liquid_floor: float) -> OperativeGraph:
    """
    Build the evaluation OperativeGraph for MentorOperative.

    Graph: single node — mentor.evaluate_budget scans pockets and returns
    proposal data deterministically. No LLM.
    """
    return OperativeGraph(
        nodes={
            "evaluate": OperativeNode(
                node_id="evaluate",
                operator_name="mentor.evaluate_budget",
                kwargs={
                    "action_threshold": action_threshold,
                    "liquid_floor": liquid_floor,
                },
            ),
        },
        edges=[],
        entry_node="evaluate",
        exit_node="evaluate",
    )


def _build_deliberation_graph(liquid_floor: float) -> OperativeGraph:
    """
    Build the deliberation OperativeGraph for MentorOperative.

    Graph: single node — mentor.deliberate_budget applies rule-based vote logic.
    $trigger_event injects the proposal dict at runtime. No LLM.
    """
    return OperativeGraph(
        nodes={
            "deliberate": OperativeNode(
                node_id="deliberate",
                operator_name="mentor.deliberate_budget",
                kwargs={
                    "proposal_dict": "$trigger_event",
                    "liquid_floor": liquid_floor,
                },
            ),
        },
        edges=[],
        entry_node="deliberate",
        exit_node="deliberate",
    )


class MentorOperative(BaseOperative):
    """
    Budget watchdog operative.

    Config keys (all optional, defaults shown):
      warn_threshold   : float  (default 0.20) — should_evaluate threshold
      action_threshold : float  (default 0.80) — evaluate action threshold
      liquid_floor     : float  (default 0.10) — deliberate NO floor
    """

    operative_id = "mentor"

    def __init__(
        self,
        config: dict,
        state_accessor: StateAccessor,
        claude_client: Any,
        sustain_id: str = "unknown",
        user_id: str = "system",
    ) -> None:
        super().__init__(config, state_accessor, claude_client, sustain_id, user_id)
        self._warn_threshold   = config.get("warn_threshold",   WARN_THRESHOLD)
        self._action_threshold = config.get("action_threshold", ACTION_THRESHOLD)
        self._liquid_floor     = config.get("liquid_floor",     LIQUID_FLOOR)

        # Build graphs — no LLM nodes; fully deterministic
        self.evaluation_graph   = _build_evaluation_graph(
            self._action_threshold, self._liquid_floor
        )
        self.deliberation_graph = _build_deliberation_graph(self._liquid_floor)

    # ── should_evaluate — ZERO LLM CALLS (unchanged) ─────────────────────────

    def should_evaluate(self, state: StateAccessor) -> bool:
        """
        Return True if any pocket has remaining balance < warn_threshold of
        its allocation. O(n) dict scan — no LLM, no I/O.
        """
        pockets: dict = state.get("finances.pockets", {})
        for name, data in pockets.items():
            if not isinstance(data, dict):
                continue
            allocated = data.get("allocated", 0.0)
            spent     = data.get("spent",     0.0)
            if allocated <= 0:
                continue
            remaining_ratio = (allocated - spent) / allocated
            if remaining_ratio < self._warn_threshold:
                logger.debug(
                    "[mentor] should_evaluate=True: pocket '%s' remaining=%.1f%%",
                    name, remaining_ratio * 100,
                )
                return True
        return False

    # ── evaluate — graph-based, no LLM ───────────────────────────────────────

    async def evaluate(self, trigger_event: dict) -> OperativeProposal | None:
        """
        Run the evaluation graph (mentor.evaluate_budget).
        Returns None when no pockets exceed the action threshold.
        Returns OperativeProposal otherwise — no LLM call.
        """
        ctx = self._build_operator_context()
        proposal = await self.evaluation_graph.run(ctx, trigger_event)
        exit_data = proposal.simulation_results.get(self.evaluation_graph.exit_node, {})
        if not exit_data.get("has_action", True):
            return None
        return proposal

    # ── deliberate — graph-based, no LLM ─────────────────────────────────────

    async def deliberate(
        self,
        context: dict,
        proposal: dict,
    ) -> OperativeVote:
        """
        Run the deliberation graph (mentor.deliberate_budget).
        Returns OperativeVote extracted from graph exit node data — no LLM call.
        """
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
        utility = max(0.0, min(1.0, float(exit_data.get("utility", 0.5))))
        return OperativeVote(vote=vote, reasoning=reasoning, utility=utility)
