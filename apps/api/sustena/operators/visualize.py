"""
sustena/operators/visualize.py

visualize.* operators — produce ResponseWidget outputs from live state.

Wires UIParser into operator outputs. All are read-only (pawa_cost=0, no
side_effects). Every operator that produces a display should use visualize.*.

Operators:
  visualize.pocket_ring — budget ring chart (rpc)
  visualize.event_feed — recent event feed widget (streaming)
  visualize.constraint_health — constraint health grid (rpc)
"""

from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator
from sustena.core.uiparser import ResponseWidget


# ── visualize.pocket_ring ──────────────────────────────────────────────────────

@sustena_operator(
    name="visualize.pocket_ring",
    protocol="rpc",
    description="Return a budget_ring_chart ResponseWidget showing all pockets and key metrics.",
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def visualize_pocket_ring(
    ctx: OperatorContext,
    sustain_id: str | None = None,
) -> OperatorResult:
    """
    Reads the budget section of current state and returns a ResponseWidget
    of type budget_ring_chart that UIParser can render.

    params:
      sustain_id -- context hint (defaults to ctx.sustain_id)
    """
    liquid: float = ctx.state.get("finances.liquid.balance", 0.0)
    pockets_raw: dict = ctx.state.get("finances.pockets", {})

    pocket_list = []
    total_allocated = 0.0
    total_spent = 0.0

    for name, data in (pockets_raw or {}).items():
        if not isinstance(data, dict):
            continue
        allocated = data.get("allocated", 0.0)
        spent = data.get("spent", 0.0)
        remaining = allocated - spent
        pct = round((spent / allocated * 100) if allocated > 0 else 0.0, 1)
        total_allocated += allocated
        total_spent += spent
        pocket_list.append({
            "name": name,
            "allocated": allocated,
            "spent": spent,
            "remaining": remaining,
            "pct_spent": pct,
            "status": "ok" if pct < 80 else ("warn" if pct < 100 else "over"),
        })

    pocket_list.sort(key=lambda p: p["pct_spent"], reverse=True)
    pct_total = round(
        (total_spent / total_allocated * 100) if total_allocated > 0 else 0.0, 1
    )

    widget = ResponseWidget(
        widget_type="budget_ring_chart",
        data={
            "liquid": liquid,
            "total_allocated": total_allocated,
            "total_spent": total_spent,
            "pct_spent": pct_total,
            "pockets": pocket_list,
        },
        summary=f"KES {liquid:,.0f} unallocated. {pct_total:.0f}% of budget spent.",
    )
    return OperatorResult.ok(widget.to_dict())


# ── visualize.event_feed ───────────────────────────────────────────────────────

@sustena_operator(
    name="visualize.event_feed",
    protocol="streaming",
    description="Return a ResponseWidget listing the most recent sustain events.",
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def visualize_event_feed(
    ctx: OperatorContext,
    sustain_id: str | None = None,
    limit: int = 20,
) -> OperatorResult:
    """
    Reads the event log from the current execution context and returns a
    ResponseWidget of type event_feed that UIParser can render.

    For streaming use, subsequent calls (as events arrive) return updated feeds.

    params:
      sustain_id -- context hint (defaults to ctx.sustain_id)
      limit      -- maximum number of recent events to include
    """
    recent_events = await ctx.events.get_history(limit=limit)

    event_items = [
        {
            "event_name": e.get("event_name", e.get("event_type", "unknown")),
            "timestamp": e.get("timestamp", ""),
            "payload": e.get("_payload", e.get("payload", {})),
        }
        for e in recent_events
    ]

    widget = ResponseWidget(
        widget_type="event_feed",
        data={
            "events": event_items,
            "total": len(event_items),
            "sustain_id": sustain_id or ctx.sustain_id,
        },
        summary=f"{len(event_items)} recent event(s)",
    )
    return OperatorResult.ok(widget.to_dict())


# ── visualize.constraint_health ────────────────────────────────────────────────

@sustena_operator(
    name="visualize.constraint_health",
    protocol="rpc",
    description="Return a ResponseWidget showing pass/fail status for a list of constraints.",
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def visualize_constraint_health(
    ctx: OperatorContext,
    sustain_id: str | None = None,
    constraints: list | None = None,
) -> OperatorResult:
    """
    Evaluates a list of constraint expressions against live state and returns
    a constraint_health_grid ResponseWidget.

    params:
      sustain_id  -- context hint (defaults to ctx.sustain_id)
      constraints -- list of ConstraintEngine expression strings to evaluate;
                     if omitted, reads from state.system.constraints
    """
    from sustena.core.constraints import ConstraintEngine
    _engine = ConstraintEngine()

    exprs: list[str] = constraints or ctx.state.get("system.constraints", []) or []

    health_items = []
    passing_count = 0

    for expr in exprs:
        try:
            ok, reason = _engine.evaluate(expr, ctx.state, {})
        except Exception as exc:
            ok, reason = False, str(exc)
        if ok:
            passing_count += 1
        health_items.append({
            "constraint": expr,
            "passing": ok,
            "reason": reason if not ok else None,
        })

    total = len(health_items)
    widget = ResponseWidget(
        widget_type="constraint_health_grid",
        data={
            "constraints": health_items,
            "passing": passing_count,
            "failing": total - passing_count,
            "total": total,
            "sustain_id": sustain_id or ctx.sustain_id,
        },
        summary=f"{passing_count}/{total} constraints passing",
    )
    return OperatorResult.ok(widget.to_dict())
