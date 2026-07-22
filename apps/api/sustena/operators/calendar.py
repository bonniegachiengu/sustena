"""
sustena/operators/calendar.py

Calendar domain operators — shared infrastructure for any Homestead sustain.

State schema these operators expect under state.calendar:
{
  "calendar": {
    "events": [
      {
        "id":          "uuid",
        "title":       "School fundraiser",
        "date":        "2026-06-15T10:00:00",
        "description": "Annual school event"
      }
    ]
  }
}

Operators:
  homestead.calendar.add_event      — add a future event to the calendar
  homestead.calendar.upcoming_events — list events within the next N days
  homestead.calendar.remove_event   — remove an event by ID

All three operators wire the four primitives:
  1. @sustena_operator   — registers in OPERATOR_REGISTRY with metadata + constraints
  2. ConstraintEngine    — self-checks pre-conditions at the start of each operator body
  3. StateAccessor       — all state reads and mutations go through ctx.state
  4. EventBus            — ctx.events.publish() fires domain events after mutations
  5. PawaLedger          — ctx.pawa.deduct() charges pawa after constraints pass
"""

import uuid
from datetime import datetime, timezone, timedelta

from sustena.core.constraints import ConstraintEngine
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult, sustena_operator

# Shared engine instance — stateless, safe to reuse across calls
_engine = ConstraintEngine()


def _check_constraints(operator_name: str, ctx: OperatorContext, params: dict) -> OperatorResult | None:
    """
    Run the operator's declared pre-condition constraints via ConstraintEngine.
    Returns an OperatorResult.fail() if any constraint fails, else None (proceed).
    """
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None or not meta.constraints:
        return None
    ok, reason = _engine.evaluate_all(meta.constraints, ctx.state, params)
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated=reason)
    return None


async def _charge_pawa(operator_name: str, ctx: OperatorContext) -> OperatorResult | None:
    """
    Deduct pawa for this operator call via PawaLedger.
    Returns an OperatorResult.fail() if the user has insufficient balance, else None.
    """
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None:
        return None
    charged = await ctx.pawa.deduct(
        user_id=ctx.user_id,
        sustain_id=ctx.sustain_id,
        amount=meta.pawa_cost,
        reason=f"op:{operator_name}",
    )
    if not charged:
        return OperatorResult.fail(
            reason=(
                f"Insufficient pawa: '{operator_name}' costs {meta.pawa_cost} pawa "
                f"but your balance is too low."
            ),
            constraint_violated="pawa_balance_sufficient",
        )
    return None


# ── homestead.calendar.add_event ───────────────────────────────────────────────

@sustena_operator(
    name="homestead.calendar.add_event",
    description="Add an event to the household calendar. Date must be in the future.",
    constraints=[],  # date-in-future enforced inline in operator body
    side_effects=["event.calendar.event_added"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "calendar_event_card",
        "fields": [
            {"label": "Title",       "source": "inputs.title",       "display": "text"},
            {"label": "Date",        "source": "inputs.date_str",    "display": "date"},
            {"label": "Description", "source": "inputs.description", "display": "text"},
        ],
        "ctas": ["View Calendar"],
    },
)
async def calendar_add_event(
    ctx: OperatorContext,
    title: str,
    date_str: str,
    description: str = "",
) -> OperatorResult:
    """
    Add a future event to calendar.events.

    Primitives used:
      ConstraintEngine  — enforces date must be in the future
      PawaLedger — deducts 0 pawa (free operator)
      StateAccessor — appends to calendar.events
      EventBus — fires event.calendar.event_added

    params:
      title       -- event title e.g. "School fundraiser"
      date_str    -- ISO 8601 datetime string e.g. "2026-06-15T10:00:00"
      description -- optional description
    """
    params = {"title": title, "date_str": date_str, "description": description}

    # 1. ConstraintEngine — static pre-condition check
    fail = _check_constraints("homestead.calendar.add_event", ctx, params)
    if fail:
        return fail

    # 2. Runtime constraint — date must be in the future
    try:
        event_dt = datetime.fromisoformat(date_str)
    except (ValueError, TypeError):
        return OperatorResult.fail(
            reason=f"Invalid date format '{date_str}'. Use ISO 8601 e.g. '2026-06-15T10:00:00'.",
            constraint_violated="date_valid_iso8601",
        )

    # Normalise to UTC-aware for comparison
    now_utc = ctx.timestamp.replace(tzinfo=timezone.utc) if ctx.timestamp.tzinfo is None else ctx.timestamp
    if event_dt.tzinfo is None:
        event_dt_utc = event_dt.replace(tzinfo=timezone.utc)
    else:
        event_dt_utc = event_dt.astimezone(timezone.utc)

    if event_dt_utc <= now_utc:
        return OperatorResult.fail(
            reason=f"Event date '{date_str}' must be in the future.",
            constraint_violated="date_in_future",
        )

    # 3. PawaLedger — deduct operator cost
    fail = await _charge_pawa("homestead.calendar.add_event", ctx)
    if fail:
        return fail

    # 4. StateAccessor — mutate state
    event_id = str(uuid.uuid4())
    event_entry = {
        "id":          event_id,
        "title":       title,
        "date":        date_str,
        "description": description,
    }
    ctx.state.append("calendar.events", event_entry)

    # 5. EventBus — fire domain event
    await ctx.events.publish(
        "event.calendar.event_added",
        {"id": event_id, "title": title, "date": date_str},
    )

    return OperatorResult.ok({
        "event_id":    event_id,
        "title":       title,
        "date":        date_str,
        "description": description,
    })


# ── homestead.calendar.upcoming_events ────────────────────────────────────────

@sustena_operator(
    name="homestead.calendar.upcoming_events",
    description="Return a list of upcoming calendar events within the next N days.",
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "calendar_list",
        "fields": [
            {"label": "Days",   "source": "inputs.days",  "display": "number"},
            {"label": "Events", "source": "state.calendar.events", "display": "list"},
        ],
        "ctas": ["Add Event"],
    },
)
async def calendar_upcoming_events(
    ctx: OperatorContext,
    days: int = 7,
) -> OperatorResult:
    """
    Read-only operator — returns calendar events within the next `days` days.

    Primitives used:
      ConstraintEngine  — no constraints declared (read-only, always permitted)
      PawaLedger — deducts 0 pawa (free operator)
      StateAccessor — reads calendar.events, filters by date window
      EventBus — no events (read-only)
    """
    params = {"days": days}

    # 1. PawaLedger — deduct operator cost
    fail = await _charge_pawa("homestead.calendar.upcoming_events", ctx)
    if fail:
        return fail

    # 2. StateAccessor — read and filter
    all_events: list = ctx.state.get("calendar.events", [])

    now_utc = ctx.timestamp.replace(tzinfo=timezone.utc) if ctx.timestamp.tzinfo is None else ctx.timestamp
    window_end = now_utc + timedelta(days=days)

    upcoming = []
    for ev in all_events:
        try:
            ev_dt = datetime.fromisoformat(ev["date"])
            if ev_dt.tzinfo is None:
                ev_dt = ev_dt.replace(tzinfo=timezone.utc)
            else:
                ev_dt = ev_dt.astimezone(timezone.utc)
            if now_utc <= ev_dt <= window_end:
                upcoming.append(ev)
        except (ValueError, KeyError):
            continue  # skip malformed events

    upcoming.sort(key=lambda e: e["date"])

    widget = {
        "type":    "calendar_list",
        "days":    days,
        "count":   len(upcoming),
        "events":  upcoming,
        "summary": (
            f"{len(upcoming)} event{'s' if len(upcoming) != 1 else ''} "
            f"in the next {days} day{'s' if days != 1 else ''}."
        ),
    }

    # 3. EventBus — no events for read-only operator
    return OperatorResult.ok(widget)


# ── homestead.calendar.remove_event ───────────────────────────────────────────

@sustena_operator(
    name="homestead.calendar.remove_event",
    description="Remove a calendar event by its ID.",
    constraints=[],  # event-exists enforced inline in operator body
    side_effects=["event.calendar.event_removed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "confirmation",
        "fields": [
            {"label": "Event ID", "source": "inputs.event_id", "display": "text"},
        ],
        "ctas": ["View Calendar"],
    },
)
async def calendar_remove_event(
    ctx: OperatorContext,
    event_id: str,
) -> OperatorResult:
    """
    Remove an event from calendar.events by ID.

    Primitives used:
      ConstraintEngine  — enforces event_id is not None
      PawaLedger — deducts 0 pawa (free operator)
      StateAccessor — reads calendar.events, removes matching event
      EventBus — fires event.calendar.event_removed

    params:
      event_id -- UUID of the event to remove
    """
    params = {"event_id": event_id}

    # 1. ConstraintEngine — static pre-condition check
    fail = _check_constraints("homestead.calendar.remove_event", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger — deduct operator cost
    fail = await _charge_pawa("homestead.calendar.remove_event", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime constraint: event must exist
    all_events: list = ctx.state.get("calendar.events", [])
    target = next((ev for ev in all_events if ev.get("id") == event_id), None)

    if target is None:
        return OperatorResult.fail(
            reason=f"Event '{event_id}' does not exist in calendar.",
            constraint_violated="event_exists",
        )

    updated_events = [ev for ev in all_events if ev.get("id") != event_id]
    ctx.state.set("calendar.events", updated_events)

    # 4. EventBus — fire domain event
    await ctx.events.publish(
        "event.calendar.event_removed",
        {"id": event_id, "title": target.get("title", "")},
    )

    return OperatorResult.ok({
        "removed_event_id":   event_id,
        "removed_event_title": target.get("title", ""),
        "events_remaining":   len(updated_events),
    })
