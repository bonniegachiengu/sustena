"""
sustena/operatives/protege.py

ProtegeOperative — scheduling operative.

Responsibility:
  - Manage all time-bound commitments: tasks, calendar, conflicts, delegation.
  - System prompt context: pending tasks from state, upcoming calendar events,
    time blocks.
  - When tasks are past due or unassigned tasks are due within 24 hours,
    propose scheduling or delegation actions.

Utility function: maximise schedule adherence / minimise missed deadlines.

Disagreement point: status quo (no action taken) — used in Nash bargaining.

should_evaluate() threshold:
  - Any task is PAST DUE (due_date < now), OR
  - Any unassigned task has due_date within 24 hours.
  Zero LLM calls in this check.
"""

import json
import logging
from datetime import datetime, timezone, timedelta
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

# should_evaluate triggers when an unassigned task is due within this window
URGENT_WINDOW_HOURS = 24


class ProtegeOperative(BaseOperative):
    """
    Scheduling operative.

    Config keys (all optional, defaults shown):
      urgent_window_hours : int (default 24) — hours-ahead window for urgent tasks
    """

    operative_id = "protege"

    def __init__(
        self,
        config: dict,
        state_accessor: StateAccessor,
        claude_client: Any,
    ) -> None:
        super().__init__(config, state_accessor, claude_client)
        self._urgent_window_hours = config.get("urgent_window_hours", URGENT_WINDOW_HOURS)

    # ── should_evaluate — ZERO LLM CALLS ──────────────────────────────────────

    def should_evaluate(self, state: StateAccessor) -> bool:
        """
        Return True if:
          - Any task is past due (due_date < now), OR
          - Any unassigned task has due_date within urgent_window_hours.

        O(n) scan over tasks — no LLM, no I/O.
        """
        tasks: list = state.get("schedule.tasks", [])
        if not tasks:
            return False

        now_utc = datetime.now(tz=timezone.utc)
        cutoff  = now_utc + timedelta(hours=self._urgent_window_hours)

        for task in tasks:
            if not isinstance(task, dict):
                continue

            due_str = task.get("due_date")
            if not due_str:
                continue

            try:
                due_dt = datetime.fromisoformat(due_str)
                if due_dt.tzinfo is None:
                    due_dt = due_dt.replace(tzinfo=timezone.utc)
            except (ValueError, TypeError):
                continue

            # Trigger: task is past due
            if due_dt < now_utc:
                logger.debug(
                    "[protege] should_evaluate=True: task '%s' is past due",
                    task.get("name", "?"),
                )
                return True

            # Trigger: unassigned task due within urgent window
            assigned_to = task.get("assigned_to")
            if not assigned_to and due_dt <= cutoff:
                logger.debug(
                    "[protege] should_evaluate=True: unassigned task '%s' due within %dh",
                    task.get("name", "?"), self._urgent_window_hours,
                )
                return True

        return False

    # ── evaluate ──────────────────────────────────────────────────────────────

    async def evaluate(self, trigger_event: dict) -> OperativeProposal | None:
        """
        Triggered by event.calendar.event_added or event.task.created.

        Checks for scheduling conflicts and unassigned tasks.
        Proposes scheduling or delegation actions.

        Returns an OperativeProposal if action is warranted, else None.
        """
        tasks: list       = self.state.get("schedule.tasks", [])
        events: list      = self.state.get("calendar.events", [])
        time_blocks: list = self.state.get("schedule.time_blocks", [])

        now_utc = datetime.now(tz=timezone.utc)
        cutoff  = now_utc + timedelta(hours=self._urgent_window_hours)

        past_due: list[dict]          = []
        unassigned_urgent: list[dict] = []

        for task in tasks:
            if not isinstance(task, dict):
                continue
            due_str = task.get("due_date")
            if not due_str:
                continue
            try:
                due_dt = datetime.fromisoformat(due_str)
                if due_dt.tzinfo is None:
                    due_dt = due_dt.replace(tzinfo=timezone.utc)
            except (ValueError, TypeError):
                continue

            if due_dt < now_utc:
                past_due.append({
                    "id":            task.get("id"),
                    "name":          task.get("name", "?"),
                    "due_date":      due_str,
                    "assigned_to":   task.get("assigned_to"),
                    "hours_overdue": round((now_utc - due_dt).total_seconds() / 3600, 1),
                })
            elif not task.get("assigned_to") and due_dt <= cutoff:
                unassigned_urgent.append({
                    "id":         task.get("id"),
                    "name":       task.get("name", "?"),
                    "due_date":   due_str,
                    "hours_left": round((due_dt - now_utc).total_seconds() / 3600, 1),
                })

        if not past_due and not unassigned_urgent:
            return None

        system_prompt = self._build_system_prompt(tasks, events, time_blocks)

        user_message = (
            f"Trigger event: {json.dumps(trigger_event)}\n\n"
            f"Past-due tasks:\n{json.dumps(past_due, indent=2)}\n\n"
            f"Unassigned tasks due within {self._urgent_window_hours}h:\n"
            f"{json.dumps(unassigned_urgent, indent=2)}\n\n"
            "Propose either:\n"
            "  A) schedule.assign — assign an unassigned task to a time block or person.\n"
            "  B) schedule.delegate — delegate a task to another household member.\n"
            "  C) sustena.alert — surface an overdue task alert to the user.\n\n"
            "Respond ONLY with valid JSON:\n"
            '{"action": "assign" | "delegate" | "alert", '
            '"task_id": "<id_or_null>", "assignee": "<name_or_null>", '
            '"time_block": "<block_or_null>", "rationale": "<brief>"}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed = json.loads(raw)
        except (json.JSONDecodeError, ValueError):
            logger.warning("[protege] LLM returned non-JSON; falling back to alert")
            first = past_due[0] if past_due else unassigned_urgent[0]
            parsed = {
                "action":     "alert",
                "task_id":    first.get("id"),
                "assignee":   None,
                "time_block": None,
                "rationale":  f"Task '{first.get('name', '?')}' requires scheduling attention.",
            }

        action     = parsed.get("action", "alert")
        task_id    = parsed.get("task_id")
        assignee   = parsed.get("assignee")
        time_block = parsed.get("time_block")
        rationale  = parsed.get("rationale", "Scheduling conflict or missed deadline detected.")

        if action == "assign" and task_id:
            return OperativeProposal(
                operator_name="schedule.assign",
                input_params={
                    "task_id":    task_id,
                    "time_block": time_block,
                    "reason":     f"ProtegeOperative: {rationale}",
                },
                rationale=rationale,
                simulation_results={
                    "past_due":          past_due,
                    "unassigned_urgent": unassigned_urgent,
                },
            )
        elif action == "delegate" and task_id and assignee:
            return OperativeProposal(
                operator_name="schedule.delegate",
                input_params={
                    "task_id":  task_id,
                    "assignee": assignee,
                    "reason":   f"ProtegeOperative: {rationale}",
                },
                rationale=rationale,
                simulation_results={
                    "past_due":          past_due,
                    "unassigned_urgent": unassigned_urgent,
                },
            )
        else:
            return OperativeProposal(
                operator_name="sustena.alert",
                input_params={
                    "message":  rationale,
                    "tasks":    past_due + unassigned_urgent,
                    "severity": "warn",
                },
                rationale=rationale,
                simulation_results={
                    "past_due":          past_due,
                    "unassigned_urgent": unassigned_urgent,
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
          - The proposal advances goal completion or resolves scheduling conflicts.

        Vote NO if:
          - The proposal creates deadline conflicts or exceeds the time budget.

        Vote ABSTAIN if:
          - The proposal is unrelated to scheduling.
        """
        operator_name = proposal.get("operator_name", "")
        input_params  = proposal.get("input_params", {})

        # Abstain on operators outside the scheduling domain
        scheduling_domains = ("schedule.", "sustena.alert", "calendar.")
        if not any(operator_name.startswith(d) for d in scheduling_domains):
            return OperativeVote(
                vote=VoteChoice.ABSTAIN,
                reasoning="Proposal is outside ProtegeOperative's domain (scheduling/tasks).",
                utility=0.5,
            )

        # Hard NO: proposal assigns to a locked time block
        proposed_time_block = input_params.get("time_block")
        if proposed_time_block:
            time_blocks: list = self.state.get("schedule.time_blocks", [])
            for block in time_blocks:
                if isinstance(block, dict) and block.get("id") == proposed_time_block:
                    if block.get("locked"):
                        return OperativeVote(
                            vote=VoteChoice.NO,
                            reasoning=(
                                f"NO: time block '{proposed_time_block}' is locked "
                                "by a committed task — scheduling conflict."
                            ),
                            utility=0.1,
                        )

        # Use LLM for nuanced YES/ABSTAIN reasoning
        system_prompt = self._build_system_prompt(
            self.state.get("schedule.tasks", []),
            self.state.get("calendar.events", []),
            self.state.get("schedule.time_blocks", []),
        )

        user_message = (
            f"Council proposal:\n{json.dumps(proposal, indent=2)}\n\n"
            f"Context:\n{json.dumps(context, indent=2)}\n\n"
            "Vote YES if this advances goal completion or resolves scheduling conflicts.\n"
            "Vote NO if this creates deadline conflicts or exceeds the time budget.\n"
            "Vote ABSTAIN if you cannot determine the scheduling impact.\n\n"
            "Respond ONLY with valid JSON:\n"
            '{"vote": "YES" | "NO" | "ABSTAIN", "reasoning": "<brief>", "utility": <0.0-1.0>}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed    = json.loads(raw)
            vote_str  = parsed.get("vote", "ABSTAIN").upper()
            vote      = VoteChoice(vote_str) if vote_str in VoteChoice.__members__ else VoteChoice.ABSTAIN
            reasoning = parsed.get("reasoning", "LLM deliberation complete.")
            utility   = float(parsed.get("utility", 0.5))
            utility   = max(0.0, min(1.0, utility))
        except (json.JSONDecodeError, ValueError, KeyError) as exc:
            logger.warning("[protege] deliberate LLM parse error: %s", exc)
            vote      = VoteChoice.ABSTAIN
            reasoning = "Could not parse LLM deliberation response; defaulting to ABSTAIN."
            utility   = 0.5

        return OperativeVote(vote=vote, reasoning=reasoning, utility=utility)

    # ── Internal helpers ───────────────────────────────────────────────────────

    def _build_system_prompt(
        self,
        tasks: list,
        events: list,
        time_blocks: list,
    ) -> str:
        """Build the ProtegeOperative system prompt with tasks, calendar, and time blocks."""
        task_lines: list[str] = []
        for t in tasks:
            if isinstance(t, dict):
                name     = t.get("name", "?")
                due      = t.get("due_date", "no due date")
                assignee = t.get("assigned_to") or "UNASSIGNED"
                status   = t.get("status", "pending")
                task_lines.append(
                    f"  [{t.get('id', '?')}] {name} — due={due} assignee={assignee} status={status}"
                )

        event_lines: list[str] = []
        for ev in events:
            if isinstance(ev, dict):
                title = ev.get("title", "?")
                date  = ev.get("date", "?")
                event_lines.append(f"  [{ev.get('id', '?')}] {title} @ {date}")

        block_lines: list[str] = []
        for blk in time_blocks:
            if isinstance(blk, dict):
                label  = blk.get("label", "?")
                start  = blk.get("start", "?")
                locked = "LOCKED" if blk.get("locked") else "free"
                block_lines.append(f"  [{blk.get('id', '?')}] {label} @ {start} ({locked})")

        return (
            "You are ProtegeOperative, the scheduling operative for a Sustena sustain.\n"
            "Your role: manage time-bound commitments — tasks, calendar events, conflicts, and delegation.\n\n"
            "Pending tasks:\n" + ("\n".join(task_lines) or "  (none)") + "\n\n"
            "Upcoming calendar events:\n" + ("\n".join(event_lines) or "  (none)") + "\n\n"
            "Time blocks:\n" + ("\n".join(block_lines) or "  (none)") + "\n\n"
            f"Urgent window: {self._urgent_window_hours} hours ahead.\n\n"
            "Always respond with valid JSON only. No markdown, no prose."
        )
