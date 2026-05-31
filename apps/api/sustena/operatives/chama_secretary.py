"""
sustena/operatives/chama_secretary.py

ChamaSecretaryOperative — the Orchie for any Chama sustain.

Role: Secretary IS the Orchie. She proposes and executes on behalf of the Chama
but does NOT vote on Council proposals. DAO governance is performed by the human
members (see council.py chama extensions).

Access level: Level 0 (full access to all state).

Five responsibilities:
  1. match_contribution()    — parse M-Pesa BillRefNumber, route to operator (sync)
  2. compute_trust_score()   — weighted trust score from contribution + loan history (sync)
  3. generate_reminder()     — WhatsApp reminder via Claude, tone-calibrated (async)
  4. evaluate_loan_request() — eligibility check + Claude recommendation (async)
  5. weekly_audit_summary()  — contribution matrix for current period (sync)

should_evaluate(): True if any member has a contribution due within 3 days OR
  any active loan has a repayment due within 7 days.

Design notes:
  - match_contribution and compute_trust_score MUST NOT call the LLM.
  - weekly_audit_summary MUST NOT call the LLM.
  - generate_reminder and evaluate_loan_request use _call_claude (async).
  - This is a Colosso product. It does NOT inherit Sustena-core council voting.
"""

import json
import logging
import re
import uuid
from datetime import datetime, timezone
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

# ── Access level constants ─────────────────────────────────────────────────────
ACCESS_LEVEL_0 = "secretary"   # full access
ACCESS_LEVEL_1 = "member"      # own records only

# ── BillRefNumber format ───────────────────────────────────────────────────────
# Expected: {chama_id}-{member_id}-{period}
# e.g. KILIMANI-m001-2026-05
BILL_REF_PATTERN = re.compile(
    r"^(?P<chama_id>[A-Za-z0-9_]+)-(?P<member_id>[A-Za-z0-9_]+)-(?P<period>\d{4}-\d{2})$"
)

# ── Trust score thresholds ─────────────────────────────────────────────────────
MIN_TRUST_FOR_LOAN = 0.4   # members below this threshold cannot qualify for loans
LONGEVITY_CAP_MONTHS = 24  # longevity score caps at 24 months active


class ChamaSecretaryOperative(BaseOperative):
    """
    The Chama Secretary — Orchie for Chama sustains.

    Like Orchie in other sustains, the Secretary proposes and executes
    but does NOT vote on proposals. DAO votes are cast by human members.

    Config keys (all optional, defaults shown):
      min_trust_for_loan   : float (default 0.4)
      longevity_cap_months : int   (default 24)
      contribution_due_warning_days : int (default 3)
      loan_due_warning_days         : int (default 7)
    """

    operative_id = "chama_secretary"

    def __init__(
        self,
        config: dict,
        state_accessor: StateAccessor,
        claude_client: Any,
    ) -> None:
        super().__init__(config, state_accessor, claude_client)
        self._min_trust        = config.get("min_trust_for_loan",            MIN_TRUST_FOR_LOAN)
        self._longevity_cap    = config.get("longevity_cap_months",          LONGEVITY_CAP_MONTHS)
        self._contrib_warn_days = config.get("contribution_due_warning_days", 3)
        self._loan_warn_days   = config.get("loan_due_warning_days",         7)

    # ── should_evaluate — ZERO LLM CALLS ──────────────────────────────────────

    def should_evaluate(self, state: StateAccessor) -> bool:
        """
        Return True if:
          - Any active member has a contribution due within 3 days, OR
          - Any active loan has a repayment due within 7 days.

        O(n) scan — no LLM, no I/O.
        """
        now = datetime.now(timezone.utc)
        members: dict = state.get("members", {})

        # Check contribution deadlines (approximate: period = YYYY-MM, due end of month)
        current_period_str: str | None = state.get("rotation.current_period")
        if current_period_str:
            try:
                year, month = map(int, current_period_str.split("-"))
                # Deadline: last day of the contribution month
                import calendar
                last_day = calendar.monthrange(year, month)[1]
                deadline = datetime(year, month, last_day, tzinfo=timezone.utc)
                days_left = (deadline - now).days
                if 0 <= days_left <= self._contrib_warn_days:
                    # Check if any member has NOT contributed this period
                    for member_id, member_data in members.items():
                        if member_data.get("status") != "active":
                            continue
                        history = member_data.get("contribution_history", [])
                        contributed_this_period = any(
                            c.get("period") == current_period_str for c in history
                        )
                        if not contributed_this_period:
                            logger.debug(
                                "[chama_secretary] should_evaluate=True: "
                                "member %s has not contributed for period %s (%d days left)",
                                member_id, current_period_str, days_left,
                            )
                            return True
            except (ValueError, TypeError):
                pass

        # Check active loan repayment deadlines
        active_loans: list = state.get("loans.active", [])
        for loan in active_loans:
            if loan.get("status") not in ("DISBURSED",):
                continue
            due_date_str = loan.get("due_date")
            if not due_date_str:
                continue
            try:
                due_date = datetime.fromisoformat(due_date_str)
                if due_date.tzinfo is None:
                    due_date = due_date.replace(tzinfo=timezone.utc)
                days_left = (due_date - now).days
                if 0 <= days_left <= self._loan_warn_days:
                    logger.debug(
                        "[chama_secretary] should_evaluate=True: "
                        "loan %s due in %d days",
                        loan.get("loan_id"), days_left,
                    )
                    return True
            except (ValueError, TypeError):
                pass

        return False

    # ── 1. match_contribution — SYNC, NO LLM ─────────────────────────────────

    def match_contribution(self, mpesa_event: dict) -> dict | None:
        """
        Parse an M-Pesa payment event and match it to a chama contribution.

        BillRefNumber format: {chama_id}-{member_id}-{period}
        Example: "KILIMANI-m001-2026-05"

        Args:
            mpesa_event: dict with at minimum:
                - BillRefNumber (str)
                - TransAmount (str or float)
                - MSISDN (str) — payer phone

        Returns:
            Operator call spec dict if matched (ready to pass to operator runner):
              {
                "operator": "chama.contribution.record",
                "params": {"member_id": ..., "amount": ..., "period": ..., "on_time": True}
              }
            None if the BillRefNumber does not match the expected pattern
            (caller routes to support queue).
        """
        bill_ref = str(mpesa_event.get("BillRefNumber", "")).strip()
        match = BILL_REF_PATTERN.match(bill_ref)

        if not match:
            logger.debug(
                "[chama_secretary] match_contribution: no match for BillRefNumber=%r",
                bill_ref,
            )
            return None

        chama_id  = match.group("chama_id")
        member_id = match.group("member_id")
        period    = match.group("period")

        try:
            amount = float(mpesa_event.get("TransAmount", 0))
        except (TypeError, ValueError):
            logger.warning(
                "[chama_secretary] match_contribution: invalid TransAmount in event %r",
                mpesa_event,
            )
            return None

        logger.info(
            "[chama_secretary] match_contribution: chama=%s member=%s period=%s amount=%.2f",
            chama_id, member_id, period, amount,
        )

        return {
            "operator": "chama.contribution.record",
            "params": {
                "member_id": member_id,
                "amount": amount,
                "period": period,
                "on_time": True,   # caller can override based on deadline check
            },
            "_matched_chama_id": chama_id,
        }

    # ── 2. compute_trust_score — SYNC, NO LLM ────────────────────────────────

    def compute_trust_score(self, member_id: str, state: StateAccessor) -> float:
        """
        Compute the trust score for a member.

        Formula (weights from Brian):
          on_time_rate  = (on_time_contributions / total_contributions)   × 0.60
          longevity     = min(months_active / 24, 1.0)                    × 0.20
          dispute_rate  = (1 - disputes / max(total_transactions, 1))     × 0.20

        Returns float in [0.0, 1.0]. Returns 0.0 if the member does not exist.

        This is sync — no LLM call.
        """
        members: dict = state.get("members", {})
        member = members.get(member_id)

        if member is None:
            logger.warning(
                "[chama_secretary] compute_trust_score: member %r not found", member_id
            )
            return 0.0

        contribution_history: list = member.get("contribution_history", [])
        loan_history:         list = member.get("loan_history",         [])
        dispute_count: int         = member.get("dispute_count",        0)

        # ── Component 1: on-time rate (weight 0.60) ─────────────────────────
        total_contributions = len(contribution_history)
        on_time_contributions = sum(
            1 for c in contribution_history if c.get("on_time", False)
        )
        if total_contributions > 0:
            on_time_rate_score = (on_time_contributions / total_contributions) * 0.60
        else:
            on_time_rate_score = 0.0   # no contributions yet → no score

        # ── Component 2: longevity (weight 0.20) ────────────────────────────
        join_date_str = member.get("join_date")
        if join_date_str:
            try:
                join_date = datetime.fromisoformat(join_date_str)
                if join_date.tzinfo is None:
                    join_date = join_date.replace(tzinfo=timezone.utc)
                now = datetime.now(timezone.utc)
                months_active = (
                    (now.year - join_date.year) * 12 + (now.month - join_date.month)
                )
                months_active = max(0, months_active)
                longevity_score = min(months_active / self._longevity_cap, 1.0) * 0.20
            except (ValueError, TypeError):
                longevity_score = 0.0
        else:
            longevity_score = 0.0

        # ── Component 3: dispute history (weight 0.20) ───────────────────────
        total_transactions = total_contributions + len(loan_history)
        dispute_rate_score = (
            1.0 - dispute_count / max(total_transactions, 1)
        ) * 0.20

        trust_score = on_time_rate_score + longevity_score + dispute_rate_score
        trust_score = max(0.0, min(1.0, trust_score))  # clamp to [0, 1]

        logger.debug(
            "[chama_secretary] trust score for %s: %.3f "
            "(on_time=%.3f longevity=%.3f dispute=%.3f)",
            member_id, trust_score, on_time_rate_score, longevity_score, dispute_rate_score,
        )
        return round(trust_score, 4)

    # ── 3. generate_reminder — ASYNC, USES LLM ───────────────────────────────

    async def generate_reminder(
        self,
        member_id: str,
        days_to_deadline: int,
        miss_count: int,
    ) -> str:
        """
        Generate a calibrated WhatsApp reminder message for a member.

        Tone escalation by miss_count:
          0-1 : warm and encouraging
          2-3 : firmer but respectful, mentions consequences
          4+  : escalation language, formal tone; NEVER aggressive or shaming

        Uses _call_claude (HAIKU model). Returns the reminder message string.
        """
        member_data = self.state.get(f"members.{member_id}", {})
        member_name = member_data.get("name", member_id)
        trust_score = member_data.get("trust_score", 0.0)
        total_contributions = len(member_data.get("contribution_history", []))

        # Tone guidance based on miss_count
        if miss_count <= 1:
            tone_guidance = (
                "Tone: Warm, friendly, and encouraging. Acknowledge their commitment. "
                "Use positive language. This is a gentle nudge, not a warning."
            )
        elif miss_count <= 3:
            tone_guidance = (
                "Tone: Firm but respectful. Acknowledge the pattern while staying empathetic. "
                "Mention that continued misses may affect their trust score and loan eligibility. "
                "Avoid lecturing — be direct and professional."
            )
        else:
            tone_guidance = (
                "Tone: Formal and serious. This is an escalation notice. "
                "State clearly that their standing in the Chama is at risk. "
                "Reference potential fines and suspension. "
                "NEVER be aggressive, threatening, or shaming — remain dignified and factual."
            )

        system_prompt = (
            "You are the Chama Secretary sending a WhatsApp contribution reminder. "
            "You know the member personally and care about the health of the group.\n\n"
            f"Member: {member_name} (ID: {member_id})\n"
            f"Trust score: {trust_score:.2f}\n"
            f"Total contributions to date: {total_contributions}\n"
            f"Consecutive missed contributions: {miss_count}\n"
            f"Days until deadline: {days_to_deadline}\n\n"
            f"{tone_guidance}\n\n"
            "Rules:\n"
            "  - Write in a natural WhatsApp style (concise, no corporate jargon).\n"
            "  - Include the member\'s name.\n"
            "  - State the deadline clearly (e.g. \'end of this month\').\n"
            "  - NEVER be aggressive, threatening, or shame the member.\n"
            "  - Maximum 3 sentences for miss_count <= 1; up to 5 sentences for 2+.\n"
            "  - Do not include greetings like \'Dear\' — use their name naturally.\n"
            "Respond with ONLY the reminder message text. No JSON, no markdown."
        )

        user_message = (
            f"Write a contribution reminder for {member_name}. "
            f"They have {days_to_deadline} day(s) left to contribute this month. "
            f"This is miss number {miss_count} for them."
        )

        reminder = await self._call_claude(
            system_prompt=system_prompt,
            user_message=user_message,
            model=HAIKU,
            max_tokens=256,
        )

        logger.info(
            "[chama_secretary] generated reminder for member=%s miss_count=%d days=%d",
            member_id, miss_count, days_to_deadline,
        )
        return reminder.strip()

    # ── 4. evaluate_loan_request — ASYNC, USES LLM ───────────────────────────

    async def evaluate_loan_request(
        self,
        request: dict,
        state: StateAccessor,
    ) -> OperativeProposal:
        """
        Evaluate a loan request and return an OperativeProposal.

        Eligibility checks (hard rules, no LLM):
          1. Requested amount must be <= max_loan_ratio × pool.balance.
          2. Member trust_score must be >= min_trust_for_loan (0.4).

        If eligible, Claude evaluates the purpose and generates a recommendation.
        The Secretary creates a proposal — does not disburse directly (DAO vote required).

        Args:
            request: {
                "member_id": str,
                "amount":    float,
                "purpose":   str,
            }
            state: live StateAccessor

        Returns:
            OperativeProposal with operator_name="chama.loan.disburse" (if eligible)
            or operator_name="chama.loan.reject" (if not eligible).
            Secretary does NOT vote — she creates the proposal, Council (members) decide.
        """
        member_id = request.get("member_id", "")
        amount    = float(request.get("amount", 0))
        purpose   = request.get("purpose", "")

        rules: dict         = state.get("rules", {})
        pool_balance: float = state.get("pool.balance", 0.0)
        max_loan_ratio: float = float(rules.get("max_loan_ratio", 0.5))
        max_allowed = pool_balance * max_loan_ratio

        member_data  = state.get(f"members.{member_id}", {})
        trust_score: float = float(member_data.get("trust_score", 0.0))
        member_name  = member_data.get("name", member_id)

        # ── Hard eligibility checks (no LLM) ──────────────────────────────
        if amount > max_allowed:
            return OperativeProposal(
                operator_name="chama.loan.reject",
                input_params={
                    "member_id": member_id,
                    "amount":    amount,
                    "reason":    (
                        f"Requested amount KES {amount:,.0f} exceeds the maximum allowed "
                        f"(KES {max_allowed:,.0f} = {int(max_loan_ratio*100)}% of pool "
                        f"balance KES {pool_balance:,.0f})."
                    ),
                },
                rationale=(
                    f"Loan rejected: amount exceeds {int(max_loan_ratio*100)}% pool ratio. "
                    f"Pool balance: KES {pool_balance:,.0f}, max allowed: KES {max_allowed:,.0f}."
                ),
                simulation_results={
                    "eligible": False,
                    "rejection_reason": "exceeds_max_loan_ratio",
                    "pool_balance": pool_balance,
                    "max_allowed": max_allowed,
                    "requested": amount,
                    "trust_score": trust_score,
                },
            )

        if trust_score < self._min_trust:
            return OperativeProposal(
                operator_name="chama.loan.reject",
                input_params={
                    "member_id": member_id,
                    "amount":    amount,
                    "reason":    (
                        f"Trust score {trust_score:.2f} is below the minimum threshold "
                        f"of {self._min_trust:.2f} required for loan eligibility."
                    ),
                },
                rationale=(
                    f"Loan rejected: {member_name}\'s trust score ({trust_score:.2f}) "
                    f"is below the minimum threshold ({self._min_trust:.2f})."
                ),
                simulation_results={
                    "eligible": False,
                    "rejection_reason": "trust_score_too_low",
                    "trust_score": trust_score,
                    "min_required": self._min_trust,
                    "pool_balance": pool_balance,
                },
            )

        # ── Claude evaluates purpose and generates recommendation ─────────
        loan_history: list = member_data.get("loan_history", [])
        contribution_history: list = member_data.get("contribution_history", [])
        active_loans: list = state.get("loans.active", [])
        member_active_loans = [
            ln for ln in active_loans
            if ln.get("member_id") == member_id and ln.get("status") == "DISBURSED"
        ]

        system_prompt = (
            "You are the Chama Secretary evaluating a loan request for the group.\n\n"
            f"Member: {member_name} (ID: {member_id})\n"
            f"Trust score: {trust_score:.2f} (minimum for loans: {self._min_trust:.2f})\n"
            f"Total contributions: {len(contribution_history)}\n"
            f"Past loans: {len(loan_history)}\n"
            f"Active loans outstanding: {len(member_active_loans)}\n"
            f"Pool balance: KES {pool_balance:,.0f}\n"
            f"Requested amount: KES {amount:,.0f}\n"
            f"Purpose: {purpose}\n\n"
            "Evaluate the loan purpose and member history. Consider:\n"
            "  - Is the purpose legitimate and likely to generate repayment capacity?\n"
            "  - Does the member\'s history support confidence in repayment?\n"
            "  - Are there any risk signals?\n\n"
            "Respond ONLY with valid JSON:\n"
            "{\"recommendation\": \"approve\" | \"caution\" | \"decline\", "
            "\"rationale\": \"<2-3 sentences>\", "
            "\"risk_level\": \"low\" | \"medium\" | \"high\"}"
        )

        user_message = (
            f"Evaluate loan request: KES {amount:,.0f} for \"{purpose}\" "
            f"from {member_name}."
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed = json.loads(raw)
            recommendation = parsed.get("recommendation", "caution")
            rationale      = parsed.get("rationale", "Secretary evaluation complete.")
            risk_level     = parsed.get("risk_level", "medium")
        except (json.JSONDecodeError, ValueError):
            logger.warning("[chama_secretary] evaluate_loan_request: LLM parse error; defaulting to caution")
            recommendation = "caution"
            rationale      = f"Loan request for KES {amount:,.0f} from {member_name} for {purpose}."
            risk_level     = "medium"

        return OperativeProposal(
            operator_name="chama.loan.disburse",
            input_params={
                "member_id": member_id,
                "amount":    amount,
                "purpose":   purpose,
            },
            rationale=(
                f"[{recommendation.upper()}] {rationale}"
            ),
            simulation_results={
                "eligible": True,
                "recommendation": recommendation,
                "risk_level": risk_level,
                "trust_score": trust_score,
                "pool_balance": pool_balance,
                "pool_after_disbursement": pool_balance - amount,
                "member_active_loans": len(member_active_loans),
            },
        )

    # ── 5. weekly_audit_summary — SYNC, NO LLM ───────────────────────────────

    def weekly_audit_summary(self, state: StateAccessor) -> dict:
        """
        Generate a contribution audit matrix for the current period.

        For each active member: expected contribution, actual contribution, status.

        Returns a ResponseWidget dict with a table layout. This is sync — no LLM call.

        Return format:
          {
            "type":    "contribution_audit_table",
            "period":  "2026-05",
            "data": {
              "rows": [
                {
                  "member_id":   str,
                  "name":        str,
                  "expected":    float,
                  "actual":      float,
                  "status":      "paid" | "pending" | "late",
                  "trust_score": float,
                }
              ],
              "summary": {
                "total_expected":    float,
                "total_collected":   float,
                "paid_count":        int,
                "pending_count":     int,
                "late_count":        int,
                "collection_rate":   float,
              }
            },
            "generated_at": str,
          }
        """
        current_period: str | None = state.get("rotation.current_period")
        members: dict              = state.get("members", {})
        rules: dict                = state.get("rules", {})
        min_contribution: float    = float(rules.get("min_contribution", 500))

        rows = []
        total_expected = 0.0
        total_collected = 0.0
        paid_count = pending_count = late_count = 0

        # Determine if the period deadline has passed (for late vs pending)
        period_overdue = False
        if current_period:
            try:
                year, month = map(int, current_period.split("-"))
                import calendar
                last_day = calendar.monthrange(year, month)[1]
                deadline = datetime(year, month, last_day, tzinfo=timezone.utc)
                period_overdue = datetime.now(timezone.utc) > deadline
            except (ValueError, TypeError):
                pass

        for member_id, member_data in members.items():
            if member_data.get("status") != "active":
                continue

            name        = member_data.get("name", member_id)
            trust_score = float(member_data.get("trust_score", 0.0))
            history     = member_data.get("contribution_history", [])

            # Sum contributions for the current period
            period_contributions = [
                c for c in history
                if c.get("period") == current_period
            ]
            actual = sum(float(c.get("amount", 0)) for c in period_contributions)

            expected = min_contribution  # expected is the minimum floor
            total_expected  += expected
            total_collected += actual

            if actual >= expected:
                status      = "paid"
                paid_count += 1
            elif period_overdue:
                status     = "late"
                late_count += 1
            else:
                status          = "pending"
                pending_count  += 1

            rows.append({
                "member_id":   member_id,
                "name":        name,
                "expected":    expected,
                "actual":      actual,
                "status":      status,
                "trust_score": round(trust_score, 3),
            })

        # Sort: paid first, then pending, then late; alphabetically within group
        status_order = {"paid": 0, "pending": 1, "late": 2}
        rows.sort(key=lambda r: (status_order.get(r["status"], 9), r["name"]))

        collection_rate = (
            round(total_collected / total_expected, 4) if total_expected > 0 else 0.0
        )

        return {
            "type":   "contribution_audit_table",
            "period": current_period,
            "data": {
                "rows": rows,
                "summary": {
                    "total_expected":  total_expected,
                    "total_collected": total_collected,
                    "paid_count":      paid_count,
                    "pending_count":   pending_count,
                    "late_count":      late_count,
                    "collection_rate": collection_rate,
                },
            },
            "generated_at": datetime.now(timezone.utc).isoformat(),
        }

    # ── evaluate — required by BaseOperative ──────────────────────────────────

    async def evaluate(self, trigger_event: dict) -> OperativeProposal | None:
        """
        Triggered by scheduler or event bus when should_evaluate() is True.

        Routes to the appropriate sub-method based on trigger_event type.
        Returns None if the trigger doesn't warrant a proposal.
        """
        event_type = trigger_event.get("type", "")

        if event_type == "chama.loan_requested":
            loan_request = {
                "member_id": trigger_event.get("member_id"),
                "amount":    trigger_event.get("amount"),
                "purpose":   trigger_event.get("purpose", ""),
            }
            return await self.evaluate_loan_request(loan_request, self.state)

        # Default: no proposal
        return None

    # ── deliberate — Secretary does NOT vote ──────────────────────────────────

    async def deliberate(self, context: dict, proposal: dict) -> OperativeVote:
        """
        The Secretary does NOT vote on proposals — she is the Orchie.
        Always returns ABSTAIN with explanation.
        """
        return OperativeVote(
            vote=VoteChoice.ABSTAIN,
            reasoning=(
                "ChamaSecretaryOperative is the Orchie — she proposes and executes "
                "but does not vote on Council proposals. DAO voting is performed by "
                "the human Chama members."
            ),
            utility=0.5,
        )
