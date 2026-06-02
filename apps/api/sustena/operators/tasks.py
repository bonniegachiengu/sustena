"""
sustena/operators/tasks.py

Homestead task operators — add, complete, list, and carryover tasks.

State schema these operators expect under state.tasks:
{
  "tasks": {
    "items": [
      {
        "id":          "uuid",
        "title":       "Buy groceries",
        "due_date":    "2026-06-02",
        "assigned_to": "",
        "priority":    "normal",
        "status":      "active",
        "created_at":  "2026-06-02T08:00:00"
      }
    ]
  }
}

Operators:
  homestead.tasks.add        — add a new task
  homestead.tasks.complete   — mark a task completed by ID
  homestead.tasks.list       — list tasks by filter (all / overdue / due_today / due_soon)
  homestead.tasks.carryover  — (polling) mark all overdue active tasks as carryover
"""

import uuid
from datetime import datetime, timezone, timedelta, date

from sustena.core.constraints import ConstraintEngine
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult, sustena_operator

_engine = ConstraintEngine()

VALID_PRIORITIES = {"low", "normal", "high"}
VALID_FILTERS = {"all", "overdue", "due_today", "due_soon"}
DUE_SOON_DAYS = 3


def _check_constraints(operator_name: str, ctx: OperatorContext, params: dict) -> OperatorResult | None:
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None or not meta.constraints:
        return None
    ok, reason = _engine.evaluate_all(meta.constraints, ctx.state, params)
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated=reason)
    return None


async def _charge_pawa(operator_name: str, ctx: OperatorContext) -> OperatorResult | None:
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


def _today_str(ctx: OperatorContext) -> str:
    """Return today's ISO date string (YYYY-MM-DD) from ctx.timestamp."""
    ts = ctx.timestamp
    if ts.tzinfo is None:
        ts = ts.replace(tzinfo=timezone.utc)
    return ts.date().isoformat()


def _parse_due_date(due_date: str) -> date | None:
    """Parse YYYY-MM-DD into a date object; return None on failure."""
    try:
        return date.fromisoformat(due_date)
    except (ValueError, TypeError):
        return None


# ── homestead.tasks.add ───────────────────────────────────────────────────────

@sustena_operator(
    name="homestead.tasks.add",
    description="Add a new task to the household task list.",
    constraints=[],
    side_effects=["event.tasks.task_added"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "task_card",
        "fields": [
            {"label": "Title",       "source": "inputs.title",       "display": "text"},
            {"label": "Due",         "source": "inputs.due_date",    "display": "date"},
            {"label": "Priority",    "source": "inputs.priority",    "display": "text"},
            {"label": "Assigned to", "source": "inputs.assigned_to", "display": "text"},
        ],
        "ctas": ["View Tasks"],
    },
)
async def tasks_add(
    ctx: OperatorContext,
    title: str,
    due_date: str,
    assigned_to: str = "",
    priority: str = "normal",
) -> OperatorResult:
    """
    Add a task to tasks.items.

    params:
      title       -- task title
      due_date    -- ISO date string YYYY-MM-DD
      assigned_to -- optional member name
      priority    -- "low" | "normal" | "high"
    """
    if not title or not title.strip():
        return OperatorResult.fail(
            reason="Task title cannot be empty.",
            constraint_violated="title_non_empty",
        )

    if priority not in VALID_PRIORITIES:
        return OperatorResult.fail(
            reason=f"Invalid priority '{priority}'. Must be one of: {sorted(VALID_PRIORITIES)}.",
            constraint_violated="priority_valid",
        )

    if _parse_due_date(due_date) is None:
        return OperatorResult.fail(
            reason=f"Invalid due_date '{due_date}'. Use YYYY-MM-DD format.",
            constraint_violated="due_date_valid",
        )

    fail = await _charge_pawa("homestead.tasks.add", ctx)
    if fail:
        return fail

    task_id = str(uuid.uuid4())
    created_at = ctx.timestamp.isoformat() if ctx.timestamp.tzinfo else ctx.timestamp.replace(tzinfo=timezone.utc).isoformat()
    task = {
        "id":          task_id,
        "title":       title.strip(),
        "due_date":    due_date,
        "assigned_to": assigned_to,
        "priority":    priority,
        "status":      "active",
        "created_at":  created_at,
    }

    items: list = ctx.state.get("tasks.items", [])
    items.append(task)
    ctx.state.set("tasks.items", items)

    await ctx.events.publish(
        "event.tasks.task_added",
        {"id": task_id, "title": title, "due_date": due_date, "priority": priority},
    )

    return OperatorResult.ok({
        "task_id":  task_id,
        "title":    title.strip(),
        "due_date": due_date,
        "priority": priority,
        "status":   "active",
    })


# ── homestead.tasks.complete ──────────────────────────────────────────────────

@sustena_operator(
    name="homestead.tasks.complete",
    description="Mark a task as completed by its ID.",
    constraints=[],
    side_effects=["event.tasks.task_completed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "confirmation",
        "fields": [
            {"label": "Task ID", "source": "inputs.task_id", "display": "text"},
        ],
        "ctas": ["View Tasks"],
    },
)
async def tasks_complete(
    ctx: OperatorContext,
    task_id: str,
) -> OperatorResult:
    """
    Set tasks.items[task_id].status = "completed".

    params:
      task_id -- UUID of the task to complete
    """
    items: list = ctx.state.get("tasks.items", [])
    target = next((t for t in items if t.get("id") == task_id), None)

    if target is None:
        return OperatorResult.fail(
            reason=f"Task '{task_id}' not found.",
            constraint_violated="task_exists",
        )

    if target.get("status") == "completed":
        return OperatorResult.fail(
            reason=f"Task '{task_id}' is already completed.",
            constraint_violated="task_not_already_completed",
        )

    fail = await _charge_pawa("homestead.tasks.complete", ctx)
    if fail:
        return fail

    updated = [{**t, "status": "completed"} if t.get("id") == task_id else t for t in items]
    ctx.state.set("tasks.items", updated)

    await ctx.events.publish(
        "event.tasks.task_completed",
        {"id": task_id, "title": target.get("title", "")},
    )

    return OperatorResult.ok({
        "task_id": task_id,
        "title":   target.get("title", ""),
        "status":  "completed",
    })


# ── homestead.tasks.list ──────────────────────────────────────────────────────

@sustena_operator(
    name="homestead.tasks.list",
    description="List tasks filtered by status window (all / overdue / due_today / due_soon).",
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "task_list",
        "fields": [
            {"label": "Filter", "source": "inputs.filter", "display": "text"},
            {"label": "Tasks",  "source": "state.tasks.items", "display": "list"},
        ],
        "ctas": ["Add Task"],
    },
)
async def tasks_list(
    ctx: OperatorContext,
    filter: str = "all",
) -> OperatorResult:
    """
    Read-only operator — returns a filtered list of tasks.

    filter values:
      "all"       — all non-completed tasks
      "overdue"   — non-completed tasks with due_date < today
      "due_today" — tasks with due_date == today (any status except completed)
      "due_soon"  — tasks due within the next DUE_SOON_DAYS days (non-completed)
    """
    if filter not in VALID_FILTERS:
        return OperatorResult.fail(
            reason=f"Invalid filter '{filter}'. Must be one of: {sorted(VALID_FILTERS)}.",
            constraint_violated="filter_valid",
        )

    fail = await _charge_pawa("homestead.tasks.list", ctx)
    if fail:
        return fail

    all_items: list = ctx.state.get("tasks.items", [])
    today = _parse_due_date(_today_str(ctx))

    def _due(t: dict) -> date | None:
        return _parse_due_date(t.get("due_date", ""))

    if filter == "all":
        result = [t for t in all_items if t.get("status") != "completed"]
    elif filter == "overdue":
        result = [
            t for t in all_items
            if t.get("status") != "completed"
            and _due(t) is not None
            and _due(t) < today
        ]
    elif filter == "due_today":
        result = [
            t for t in all_items
            if t.get("status") != "completed"
            and _due(t) is not None
            and _due(t) == today
        ]
    else:  # due_soon
        window_end = today + timedelta(days=DUE_SOON_DAYS)
        result = [
            t for t in all_items
            if t.get("status") != "completed"
            and _due(t) is not None
            and today <= _due(t) <= window_end
        ]

    result.sort(key=lambda t: (t.get("due_date", ""), t.get("priority", "normal")))

    return OperatorResult.ok({
        "filter": filter,
        "count":  len(result),
        "tasks":  result,
    })


# ── homestead.tasks.carryover ─────────────────────────────────────────────────

@sustena_operator(
    name="homestead.tasks.carryover",
    description="Mark all overdue active tasks as carryover. Runs on a polling schedule.",
    constraints=[],
    side_effects=["event.tasks.tasks_carried_over"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="polling",
    ui_schema={
        "widget_type": "task_carryover_summary",
        "fields": [
            {"label": "Carried Over", "source": "inputs.count", "display": "number"},
        ],
        "ctas": ["View Tasks"],
    },
)
async def tasks_carryover(
    ctx: OperatorContext,
) -> OperatorResult:
    """
    Polling operator — scans tasks.items for active tasks with due_date < today
    and sets their status to "carryover".
    """
    fail = await _charge_pawa("homestead.tasks.carryover", ctx)
    if fail:
        return fail

    all_items: list = ctx.state.get("tasks.items", [])
    today = _parse_due_date(_today_str(ctx))

    carried_ids = []
    updated_items = []
    for t in all_items:
        due = _parse_due_date(t.get("due_date", ""))
        if t.get("status") == "active" and due is not None and due < today:
            updated_items.append({**t, "status": "carryover"})
            carried_ids.append(t["id"])
        else:
            updated_items.append(t)

    if carried_ids:
        ctx.state.set("tasks.items", updated_items)
        await ctx.events.publish(
            "event.tasks.tasks_carried_over",
            {"carried_ids": carried_ids, "count": len(carried_ids)},
        )

    return OperatorResult.ok({
        "carried_over": len(carried_ids),
        "task_ids":     carried_ids,
    })
