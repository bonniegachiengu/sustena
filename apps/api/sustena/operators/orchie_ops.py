"""
sustena/operators/orchie_ops.py

Orchie operators — session init and daily morning brief.

Operators:
  orchie.morning_brief  — aggregate today's tasks, calendar events, passed
                          council proposals, and liquid balance into a
                          morning_brief_card ResponseWidget plus a brief_text
                          narration string. No LLM calls.

State paths read:
  tasks.items               → filtered for due_date == today, status != completed
  calendar.events           → filtered for date[:10] == today
  council_proposals         → filtered for status == "PASSED"
  finances.liquid.balance   → liquid float
"""

from datetime import timezone

from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator
from sustena.core.uiparser import ResponseWidget


def _today_iso(ctx: OperatorContext) -> str:
    """Return YYYY-MM-DD for ctx.timestamp."""
    ts = ctx.timestamp
    if ts.tzinfo is None:
        ts = ts.replace(tzinfo=timezone.utc)
    return ts.date().isoformat()


# ── orchie.morning_brief ───────────────────────────────────────────────────────

@sustena_operator(
    name="orchie.morning_brief",
    description=(
        "Aggregate today's tasks, calendar events, passed council strategies, "
        "and liquid balance into a morning_brief_card widget and brief_text narration. "
        "No LLM calls."
    ),
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "morning_brief_card",
        "fields": [
            {"label": "Tasks Due Today",  "source": "state.tasks.items",             "display": "list"},
            {"label": "Events Today",     "source": "state.calendar.events",         "display": "list"},
            {"label": "Passed Strategies","source": "state.council_proposals",       "display": "list"},
            {"label": "Liquid Balance",   "source": "state.finances.liquid.balance", "display": "currency"},
        ],
        "ctas": ["View Tasks", "Ask Orchie"],
    },
)
async def orchie_morning_brief(
    ctx: OperatorContext,
    sustain_id: str | None = None,
    date_override: str | None = None,
) -> OperatorResult:
    """
    Build the morning brief for the current sustain.

    Returns OperatorResult.ok({
        "widget": ResponseWidget.to_dict(),
        "brief_text": str,
    })

    params:
      sustain_id    -- context hint (defaults to ctx.sustain_id)
      date_override -- ISO date YYYY-MM-DD; overrides ctx.timestamp.date() for testing
    """
    today = date_override if date_override else _today_iso(ctx)

    # ── tasks due today ────────────────────────────────────────────────────────
    all_tasks: list = ctx.state.get("tasks.items", []) or []
    tasks_today = [
        t for t in all_tasks
        if t.get("status") != "completed"
        and t.get("due_date", "") == today
    ]

    # ── calendar events today ──────────────────────────────────────────────────
    all_events: list = ctx.state.get("calendar.events", []) or []
    events_today = [
        ev for ev in all_events
        if isinstance(ev.get("date", ""), str) and ev["date"][:10] == today
    ]

    # ── passed council proposals ───────────────────────────────────────────────
    all_proposals: list = ctx.state.get("council_proposals", []) or []
    passed_proposals = [p for p in all_proposals if p.get("status") == "PASSED"]

    # ── liquid balance ─────────────────────────────────────────────────────────
    liquid: float = float(ctx.state.get("finances.liquid.balance", 0.0) or 0.0)

    # ── build widget ───────────────────────────────────────────────────────────
    summary = _build_summary(tasks_today, events_today, passed_proposals, liquid)
    widget = ResponseWidget(
        widget_type="morning_brief_card",
        data={
            "today":             today,
            "tasks_due_today":   tasks_today,
            "task_count":        len(tasks_today),
            "events_today":      events_today,
            "event_count":       len(events_today),
            "passed_strategies": passed_proposals,
            "strategy_count":    len(passed_proposals),
            "liquid_balance":    liquid,
            "sustain_id":        sustain_id or ctx.sustain_id,
        },
        summary=summary,
    )

    brief_text = _build_brief_text(tasks_today, events_today, passed_proposals, liquid)

    return OperatorResult.ok({
        "widget":     widget.to_dict(),
        "brief_text": brief_text,
    })


# ── narrative helpers ──────────────────────────────────────────────────────────

def _build_summary(
    tasks: list,
    events: list,
    proposals: list,
    liquid: float,
) -> str:
    parts = []
    if tasks:
        parts.append(f"{len(tasks)} task{'s' if len(tasks) != 1 else ''} due today")
    if events:
        parts.append(f"{len(events)} calendar event{'s' if len(events) != 1 else ''}")
    if proposals:
        parts.append(f"{len(proposals)} passed strateg{'ies' if len(proposals) != 1 else 'y'}")
    if not parts:
        return "nothing scheduled · sustain state nominal"
    return " · ".join(parts) + f" · KES {liquid:,.0f} liquid"


def _build_brief_text(
    tasks: list,
    events: list,
    proposals: list,
    liquid: float,
) -> str:
    if not tasks and not events and not proposals:
        return "nothing scheduled · sustain state nominal"

    parts = []

    if tasks:
        titles = ", ".join(t.get("title", "task") for t in tasks[:3])
        if len(tasks) > 3:
            titles += f" and {len(tasks) - 3} more"
        parts.append(
            f"{len(tasks)} task{'s' if len(tasks) != 1 else ''} due: {titles}"
        )

    if events:
        ev_titles = ", ".join(e.get("title", "event") for e in events[:2])
        if len(events) > 2:
            ev_titles += f" and {len(events) - 2} more"
        parts.append(
            f"{len(events)} calendar event{'s' if len(events) != 1 else ''}: {ev_titles}"
        )

    if proposals:
        parts.append(
            f"{len(proposals)} council strateg{'ies' if len(proposals) != 1 else 'y'} "
            f"passed and awaiting execution"
        )

    body = ". ".join(parts)
    return f"Habari. {body}. KES {liquid:,.0f} liquid."
