"""
sustena/operators/monitor.py

monitor.* operators — watch state paths and constraints, emit alerts.

Operators:
  monitor.state_path  — check a condition on a state path; fire alert event if met (event_driven)
  monitor.constraint  — evaluate a constraint expression; fire constraint_breached if failing (polling)
"""

from sustena.core.constraints import ConstraintEngine
from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator

_engine = ConstraintEngine()


# ── monitor.state_path ─────────────────────────────────────────────────────────

@sustena_operator(
    name="monitor.state_path",
    protocol="event_driven",
    description="Check a condition on a state path and publish an alert event if the condition is met.",
    side_effects=["event.monitor.alert_triggered"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def monitor_state_path(
    ctx: OperatorContext,
    path: str,
    condition: str,
    alert_event: str,
) -> OperatorResult:
    """
    Watch a state path and fire alert_event if the condition is met.

    Accepts two condition formats:
      Partial: "> 80" or ">= 1.0" — compared against state[path]
      Full: "finances.pockets.food.spent > 80" — full ConstraintEngine expression

    params:
      path        -- dot-path to watch e.g. "finances.pockets.food.spent"
      condition -- comparison ("> 80") or full ConstraintEngine expression
      alert_event -- event name to publish when condition is True
    """
    current_value = ctx.state.get(path)
    if current_value is None:
        return OperatorResult.fail(
            reason=f"State path '{path}' not found.",
            constraint_violated="path_exists",
        )

    # Support both full expressions ("finances.x > 80") and
    # partial comparisons ("> 80" or ">= 1.0") — partial form prepends the path.
    cond = condition.strip()
    if cond and cond[0] in (">", "<", "!", "="):
        full_expr = f"{path} {cond}"
    else:
        full_expr = cond

    try:
        ok, _ = _engine.evaluate(full_expr, ctx.state, {})
    except Exception as exc:
        return OperatorResult.fail(reason=f"Invalid condition '{condition}': {exc}")

    if ok:
        await ctx.events.publish(
            alert_event,
            {
                "path": path,
                "value": current_value,
                "condition": condition,
                "sustain_id": ctx.sustain_id,
            },
        )

    return OperatorResult.ok({
        "path": path,
        "value": current_value,
        "condition": condition,
        "alert_fired": ok,
        "alert_event": alert_event,
    })


# ── monitor.constraint ─────────────────────────────────────────────────────────

@sustena_operator(
    name="monitor.constraint",
    protocol="polling",
    description="Evaluate a constraint expression against live state; publish event.monitor.constraint_breached if failing.",
    side_effects=["event.monitor.constraint_breached"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def monitor_constraint(
    ctx: OperatorContext,
    constraint_expr: str,
    interval_s: int = 3600,
) -> OperatorResult:
    """
    Re-evaluate a constraint expression on every call. Fires
    event.monitor.constraint_breached if the constraint is currently failing.

    params:
      constraint_expr -- ConstraintEngine expression e.g. "pantry.cooking_oil_L >= 1.0"
      interval_s      -- polling interval hint in seconds (metadata only; scheduling is external)
    """
    try:
        ok, reason = _engine.evaluate(constraint_expr, ctx.state, {})
    except Exception as exc:
        return OperatorResult.fail(reason=f"Cannot evaluate constraint '{constraint_expr}': {exc}")

    if not ok:
        await ctx.events.publish(
            "event.monitor.constraint_breached",
            {
                "constraint_expr": constraint_expr,
                "reason": reason,
                "sustain_id": ctx.sustain_id,
            },
        )

    return OperatorResult.ok({
        "constraint_expr": constraint_expr,
        "passing": ok,
        "reason": reason if not ok else None,
        "interval_s": interval_s,
    })
