"""
sustena/operatives/mentor.py

MentorOperative — budget watchdog operative.

Responsibility:
  - Monitor budget pocket utilisation.
  - When any pocket exceeds 80% of its allocation, propose a reallocation
    or surface an alert to the user via the Council.
  - Vote YES on proposals that improve the budget position; NO if a proposal
    would push liquid balance below 10% of monthly income.

Utility function: minimise deviation of each pocket's spend rate from the
  planned rate (i.e. keep all pockets close to their proportional target).

Disagreement point: status quo (no change) — used in Nash bargaining.

should_evaluate() threshold: any pocket balance < 20% of allocation.
  This check is O(n) over pockets — zero LLM calls.
"""

import json
import logging
from typing import Any

from sustena.core.state import StateAccessor
from sustena.operatives.base import (
    HAIKU,
    BaseOperative,
    OperativeProposal,
    OperativeVote,
    VoteChoice,
)

logger = logging.getLogger(__name__)

# ── Thresholds ─────────────────────────────────────────────────────────────────

# should_evaluate triggers when remaining < WARN_THRESHOLD * allocated
WARN_THRESHOLD    = 0.20   # 20% remaining → operative wakes up

# evaluate() proposes action when pocket is >80% spent
ACTION_THRESHOLD  = 0.80   # 80% spent

# deliberate() rejects proposals that push liquid < LIQUID_FLOOR * income
LIQUID_FLOOR      = 0.10   # 10% of monthly income


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
    ) -> None:
        super().__init__(config, state_accessor, claude_client)
        self._warn_threshold   = config.get("warn_threshold",   WARN_THRESHOLD)
        self._action_threshold = config.get("action_threshold", ACTION_THRESHOLD)
        self._liquid_floor     = config.get("liquid_floor",     LIQUID_FLOOR)

    # ── should_evaluate — ZERO LLM CALLS ──────────────────────────────────────

    def should_evaluate(self, state: StateAccessor) -> bool:
        """
        Return True if any pocket has the remaining balance < warn_threshold of
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

    # ── evaluate ──────────────────────────────────────────────────────────────

    async def evaluate(self, trigger_event: dict) -> OperativeProposal | None:
        """
        Triggered by event.finances.pocket_spent.

        Scans all pockets for >80% utilisation. If found, calls Claude Haiku
        to reason about whether to propose budget.reallocate or surface an alert.

        Returns an OperativeProposal if action is warranted, else None.
        """
        pockets: dict = self.state.get("finances.pockets", {})
        monthly_income = self.state.get("finances.income.monthly_total", 0.0)
        liquid_balance = self.state.get("finances.liquid.balance", 0.0)

        # Find pockets that are >80% spent
        over_threshold = []
        for name, data in pockets.items():
            if not isinstance(data, dict):
                continue
            allocated = data.get("allocated", 0.0)
            spent     = data.get("spent",     0.0)
            if allocated <= 0:
                continue
            pct = spent / allocated
            if pct > self._action_threshold:
                over_threshold.append({
                    "pocket":    name,
                    "allocated": allocated,
                    "spent":     spent,
                    "pct":       round(pct * 100, 1),
                    "remaining": allocated - spent,
                })

        if not over_threshold:
            return None

        system_prompt = self._build_system_prompt(monthly_income, liquid_balance)

        user_message = (
            f"Trigger event: {json.dumps(trigger_event)}\n\n"
            f"Pockets over {int(self._action_threshold * 100)}% utilisation:\n"
            f"{json.dumps(over_threshold, indent=2)}\n\n"
            "Propose either:\n"
            "  A) budget.reallocate — if liquid balance allows a top-up.\n"
            "  B) alert — if no reallocation is possible.\n\n"
            "Respond ONLY with valid JSON:\n"
            '{"action": "reallocate" | "alert", "pocket": "<name>", '
            '"amount": <float_or_null>, "rationale": "<brief>"}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed = json.loads(raw)
        except (json.JSONDecodeError, ValueError):
            # Fallback: generate a simple alert proposal
            logger.warning("[mentor] LLM returned non-JSON; falling back to alert")
            parsed = {
                "action":    "alert",
                "pocket":    over_threshold[0]["pocket"],
                "amount":    None,
                "rationale": f"Pocket '{over_threshold[0]['pocket']}' is over 80% spent.",
            }

        action   = parsed.get("action", "alert")
        pocket   = parsed.get("pocket", over_threshold[0]["pocket"])
        amount   = parsed.get("amount")
        rationale = parsed.get("rationale", "Budget threshold breached.")

        if action == "reallocate" and amount and liquid_balance >= amount:
            return OperativeProposal(
                operator_name="budget.reallocate",
                input_params={
                    "pocket_name": pocket,
                    "amount":      float(amount),
                    "reason":      f"MentorOperative: {rationale}",
                },
                rationale=rationale,
                simulation_results={
                    "pockets_over_threshold": over_threshold,
                    "liquid_before":          liquid_balance,
                    "liquid_after":           liquid_balance - float(amount),
                },
            )
        else:
            # Alert proposal — no operator call, just surface to user
            return OperativeProposal(
                operator_name="sustena.alert",
                input_params={
                    "message":  rationale,
                    "pockets":  over_threshold,
                    "severity": "warn",
                },
                rationale=rationale,
                simulation_results={
                    "pockets_over_threshold": over_threshold,
                    "liquid_balance":          liquid_balance,
                },
            )

    # ── deliberate ────────────────────────────────────────────────────────────

    async def deliberate(
        self,
        context: dict,
        proposal: dict,
    ) -> OperativeVote:
        """
        Vote on a Council proposal.

        Vote YES if:
          - The proposal improves budget position (reduces pocket overspend),
            AND liquid balance after execution remains >= LIQUID_FLOOR * income.

        Vote NO if:
          - The proposal would push liquid balance below LIQUID_FLOOR * income.

        Vote ABSTAIN if:
          - The proposal is unrelated to finances.
        """
        operator_name = proposal.get("operator_name", "")

        # If not a finance-related operator, abstain
        if not operator_name.startswith(("budget.", "sustena.alert")):
            return OperativeVote(
                vote=VoteChoice.ABSTAIN,
                reasoning="Proposal is outside MentorOperative's domain (budget/finance).",
                utility=0.5,
            )

        monthly_income = self.state.get("finances.income.monthly_total", 0.0)
        liquid_balance = self.state.get("finances.liquid.balance",       0.0)
        floor_amount   = monthly_income * self._liquid_floor

        # Hard NO: would push liquid below floor
        input_params = proposal.get("input_params", {})
        proposed_amount = float(input_params.get("amount", 0) or 0)

        if proposed_amount > 0 and (liquid_balance - proposed_amount) < floor_amount:
            reasoning = (
                f"NO: executing this proposal would reduce liquid balance from "
                f"KES {liquid_balance:,.0f} to KES {liquid_balance - proposed_amount:,.0f}, "
                f"below the 10% income floor of KES {floor_amount:,.0f}."
            )
            return OperativeVote(
                vote=VoteChoice.NO,
                reasoning=reasoning,
                utility=0.1,
            )

        # Use LLM for nuanced YES/ABSTAIN reasoning
        system_prompt = self._build_system_prompt(monthly_income, liquid_balance)

        user_message = (
            f"Council proposal to evaluate:\n{json.dumps(proposal, indent=2)}\n\n"
            f"Additional context:\n{json.dumps(context, indent=2)}\n\n"
            "Vote YES if this improves the budget position or keeps all constraints.\n"
            "Vote NO if it worsens the budget position or violates constraints.\n"
            "Vote ABSTAIN if you cannot determine the impact.\n\n"
            "Respond ONLY with valid JSON:\n"
            '{"vote": "YES" | "NO" | "ABSTAIN", "reasoning": "<brief>", "utility": <0.0-1.0>}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed = json.loads(raw)
            vote_str = parsed.get("vote", "ABSTAIN").upper()
            vote      = VoteChoice(vote_str) if vote_str in VoteChoice.__members__ else VoteChoice.ABSTAIN
            reasoning = parsed.get("reasoning", "LLM deliberation complete.")
            utility   = float(parsed.get("utility", 0.5))
            utility   = max(0.0, min(1.0, utility))  # clamp to [0, 1]
        except (json.JSONDecodeError, ValueError, KeyError) as exc:
            logger.warning("[mentor] deliberate LLM parse error: %s", exc)
            vote      = VoteChoice.ABSTAIN
            reasoning = "Could not parse LLM deliberation response; defaulting to ABSTAIN."
            utility   = 0.5

        return OperativeVote(vote=vote, reasoning=reasoning, utility=utility)

    # ── Internal helpers ───────────────────────────────────────────────────────

    def _build_system_prompt(self, monthly_income: float, liquid_balance: float = 0.0) -> str:
        """Build the MentorOperative system prompt with income, liquid balance,
        pocket allocations, and spending history."""
        pockets_raw: dict = self.state.get("finances.pockets", {})
        pocket_lines: list[str] = []
        for name, data in pockets_raw.items():
            if isinstance(data, dict):
                allocated = data.get("allocated", 0.0)
                spent     = data.get("spent",     0.0)
                remaining = allocated - spent
                pct_spent = (spent / allocated * 100) if allocated > 0 else 0.0
                pocket_lines.append(
                    f"  {name}: allocated={allocated:,.0f}, spent={spent:,.0f}, "
                    f"remaining={remaining:,.0f} ({pct_spent:.1f}% spent)"
                )

        pocket_block = "\n".join(pocket_lines) if pocket_lines else "  (no pockets set)"

        return (
            "You are MentorOperative, the budget watchdog for a Sustena sustain.\n"
            "Your role: monitor budget pockets and propose reallocations or alerts.\n\n"
            f"User monthly income: KES {monthly_income:,.0f}\n"
            f"Current liquid balance: KES {liquid_balance:,.0f}\n\n"
            f"Pocket allocations and spending history:\n{pocket_block}\n\n"
            f"Constraints:\n"
            f"  - Liquid balance must never fall below 10% of income "
            f"(floor = KES {monthly_income * self._liquid_floor:,.0f}).\n"
            f"  - A pocket is 'at risk' when spent > {int(self._action_threshold * 100)}% of allocated.\n\n"
            "Always respond with valid JSON only. No markdown, no prose."
        )
