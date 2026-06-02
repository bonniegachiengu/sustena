"""
sustena/api/routes/devui.py

Developer UI endpoints — mounted at /devui.
Only accessible with Authorization: Bearer {ADMIN_TOKEN} (REST)
or ?token={ADMIN_TOKEN} (WebSocket).

Endpoints
---------
GET  /devui/sustains                  — sustain selector list
GET  /devui/state?sustain_id=...      — full state for one sustain
POST /devui/console/execute           — run an operator from the console
POST /devui/simulate                  — forward-simulate a proposal
WS   /devui/state-stream?sustain_id=  — real-time state push

Legacy path-param routes are kept for backwards compatibility:
GET  /devui/sustain/{id}/state
GET  /devui/sustain/{id}/events
GET  /devui/sustain/{id}/proposals
GET  /devui/registry/operators
GET  /devui/registry/operatives
WS   /devui/state-stream/{id}
"""

import asyncio
import logging
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query, WebSocket, WebSocketDisconnect, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field

from sustena.config import settings

router = APIRouter()
logger = logging.getLogger(__name__)


# ── Auth ──────────────────────────────────────────────────────────────────────

from fastapi import Header as _Header


def verify_admin(authorization: str | None = _Header(default=None)) -> str:
    if authorization is None:
        raise HTTPException(status_code=401, detail="Authorization header required")
    scheme, _, token = authorization.partition(" ")
    if scheme.lower() != "bearer" or token != settings.admin_token:
        raise HTTPException(status_code=401, detail="Invalid admin token")
    return token


# ── Helpers ───────────────────────────────────────────────────────────────────

def ok(data: Any) -> dict:
    return {
        "status": "ok",
        "data": data,
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }


# ── Pydantic models ───────────────────────────────────────────────────────────

class ConsoleExecuteRequest(BaseModel):
    sustain_id: str
    # Accept both field names — frontend may send either
    operator: str | None = None
    operator_id: str | None = None
    inputs: dict = Field(default_factory=dict)
    params: dict = Field(default_factory=dict)  # alias for inputs

    def resolved_operator(self) -> str:
        op = self.operator_id or self.operator
        if not op:
            raise HTTPException(status_code=422, detail="operator or operator_id required")
        return op

    def resolved_params(self) -> dict:
        return self.params or self.inputs


class SimulateRequest(BaseModel):
    sustain_id: str
    proposal: list[dict] = Field(
        default_factory=list,
        description="Sequence of {operator, params} steps to simulate",
    )


class SimulatePipelineRequest(BaseModel):
    sustain_id: str
    proposal: list[dict] = Field(
        default_factory=list,
        description="Sequence of {operator, params} steps to run through simulate.fork → run_path → score",
    )
    goal_metric: str = Field(
        default="minimize_budget_deviation",
        description="Goal metric for simulate.score. Options: minimize_budget_deviation | maximize_savings_rate | maximize_liquid_balance",
    )


class PreviewWidgetRequest(BaseModel):
    spec_json: dict = Field(description="A ui_schema dict or operator spec containing ui_schema")
    mock_state: dict = Field(
        default_factory=dict,
        description="Optional mock state for source resolution: {inputs: {...}, state: {...}}",
    )


# ── 1. GET /devui/sustains ────────────────────────────────────────────────────

@router.get("/sustains", summary="List sustains for the selector dropdown")
async def list_sustains(_: str = Depends(verify_admin)) -> dict:
    """
    Returns all sustains with id, label, status, pockets summary, and
    active operative count — enough for the TopBar selector and Monitor hero tiles.
    """
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        raw = engine.list_all()
        return ok({"sustains": raw})
    except Exception as exc:
        logger.debug("SustainEngine.list_all failed: %s", exc)

    return ok({"sustains": []})


# ── 2. GET /devui/state?sustain_id= ──────────────────────────────────────────

@router.get("/state", summary="Full current state for a sustain")
async def get_state(
    sustain_id: str = Query(..., description="e.g. homestead.bonnie"),
    _: str = Depends(verify_admin),
) -> dict:
    """
    Returns pockets, events feed, operative statuses, constraint health,
    and pawa balance — all the data the Monitor panel consumes.
    """
    from sustena.core.engine_singleton import get_shared_engine
    engine = get_shared_engine()

    state: dict = {}
    operatives: list = []
    constraints: list = []
    events: list = []

    try:
        state = engine.get_state(sustain_id)
        operatives = engine.get_operative_statuses(sustain_id)
        constraints = engine.evaluate_constraints(sustain_id)
    except ValueError:
        pass  # sustain not seeded yet — return empty payload
    except Exception as exc:
        logger.debug("get_state(%s) engine error: %s", sustain_id, exc)

    try:
        from sustena.core.events import EventBus
        bus = EventBus(sustain_id=sustain_id)
        events = await bus.get_history(limit=20)
    except Exception as exc:
        logger.debug("EventBus.get_history(%s) failed: %s", sustain_id, exc)

    return ok({
        "sustain_id": sustain_id,
        "state": state,
        "events": events,
        "operatives": operatives,
        "constraints": constraints,
    })


# ── 3. POST /devui/console/execute ────────────────────────────────────────────

@router.post("/console/execute", summary="Execute an operator from the dev console")
async def console_execute(
    body: ConsoleExecuteRequest,
    _: str = Depends(verify_admin),
) -> dict:
    """
    Runs an operator through OperatorContext against live state.
    Returns the result delta + any emitted events.
    """
    operator_name = body.resolved_operator()
    params = body.resolved_params()
    logger.info(
        "DevUI console.execute: sustain=%s operator=%s params=%s",
        body.sustain_id, operator_name, params,
    )
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        result = await engine.execute_operator(
            sustain_id=body.sustain_id,
            operator_name=operator_name,
            params=params,
        )
        return ok({
            "result": result.to_response(),
            "sustain_id": body.sustain_id,
            "operator": operator_name,
            "events": [],
        })
    except Exception as exc:
        logger.warning("console_execute failed: %s", exc)
        return ok({
            "result": {
                "status": "ok",
                "data": {"delta": {}, "note": f"Stub execution: {exc}"},
            },
            "sustain_id": body.sustain_id,
            "operator": operator_name,
            "events": [],
            "note": "Stub — engine not ready",
        })


# ── 4. POST /devui/simulate ───────────────────────────────────────────────────

@router.post("/simulate", summary="Simulate a proposal against a forked state")
async def simulate(
    body: SimulateRequest,
    _: str = Depends(verify_admin),
) -> dict:
    """
    Runs a sequence of operators against a forked copy of the sustain state
    (no DB writes). Returns projected state delta + per-step constraint results.

    Body: { sustain_id, proposal: [{operator, params}, ...] }
    """
    logger.info(
        "DevUI simulate: sustain=%s steps=%d",
        body.sustain_id, len(body.proposal),
    )
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        step_results = await engine.simulate(
            sustain_id=body.sustain_id,
            operator_sequence=body.proposal,
        )
        return ok({
            "sustain_id": body.sustain_id,
            "steps": [
                {
                    "operator":    r["operator"],
                    "params":      r["params"],
                    "result":      r["result"].to_response(),
                    "state_after": r["state_after"],
                }
                for r in step_results
            ],
            "final_state": step_results[-1]["state_after"] if step_results else {},
        })
    except Exception as exc:
        logger.warning("simulate(%s) failed: %s", body.sustain_id, exc)
        stub_steps = []
        for step in body.proposal:
            stub_steps.append({
                "operator": step.get("operator", "?"),
                "params":   step.get("params", {}),
                "result": {
                    "status": "ok",
                    "data": {"note": "Stub simulation step"},
                },
                "state_after": {"_stub": True},
            })
        return ok({
            "sustain_id": body.sustain_id,
            "steps": stub_steps,
            "final_state": {"_stub": True, "note": f"Engine not ready: {exc}"},
        })


# ── 5. WS /devui/state-stream ─────────────────────────────────────────────────

@router.websocket("/state-stream")
async def state_stream_query(
    websocket: WebSocket,
    sustain_id: str = Query(...),
):
    """
    WebSocket state stream (query-param form): /devui/state-stream?sustain_id=homestead.bonnie&token=...
    Sends a full state snapshot on connect, then pushes events as they arrive.
    Falls back to polling every 2s until EventBus subscription is wired.
    """
    token = websocket.query_params.get("token", "")
    if token != settings.admin_token:
        await websocket.close(code=status.WS_1008_POLICY_VIOLATION)
        return

    await websocket.accept()
    logger.info("WS state-stream opened for %s", sustain_id)

    try:
        while True:
            try:
                from sustena.core.engine_singleton import get_shared_engine
                engine = get_shared_engine()
                state = engine.get_state(sustain_id)
            except Exception:
                state = {}

            await websocket.send_json({
                "type": "state_snapshot",
                "sustain_id": sustain_id,
                "state": state,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            })
            await asyncio.sleep(2)

    except WebSocketDisconnect:
        logger.info("WS state-stream closed for %s", sustain_id)
    except Exception as exc:
        logger.error("WS state-stream error for %s: %s", sustain_id, exc)
        try:
            await websocket.close()
        except Exception:
            pass


# ── Legacy path-param routes (backwards compat) ───────────────────────────────

@router.get("/sustain/{sustain_id}/state", summary="Full state dict for a sustain (path-param form)")
async def get_sustain_state(sustain_id: str, _: str = Depends(verify_admin)) -> dict:
    """Delegates to the canonical query-param handler."""
    return await get_state(sustain_id=sustain_id, _=_)


@router.get("/sustain/{sustain_id}/events", summary="Paginated event log")
async def get_sustain_events(
    sustain_id: str,
    limit: int = 100,
    _: str = Depends(verify_admin),
) -> dict:
    try:
        from sustena.core.events import EventBus
        bus = EventBus(sustain_id=sustain_id)
        events = await bus.get_history(limit=limit)
        return ok({"sustain_id": sustain_id, "events": events, "count": len(events)})
    except Exception as exc:
        logger.warning("EventBus.get_history(%s) failed: %s", sustain_id, exc)
    return ok({"sustain_id": sustain_id, "events": [], "count": 0})


@router.get("/sustain/{sustain_id}/proposals", summary="Council proposals")
async def get_sustain_proposals(sustain_id: str, _: str = Depends(verify_admin)) -> dict:
    try:
        from sustena.db.schema import get_engine
        from sqlalchemy import text as _text
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                _text(
                    "SELECT id, proposed_by, operator_name, status, created_at "
                    "FROM council_proposals WHERE sustain_id = :sid ORDER BY created_at DESC"
                ),
                {"sid": sustain_id},
            )
            proposals = [
                {
                    "id": r[0],
                    "proposed_by": r[1],
                    "operator_name": r[2],
                    "status": r[3],
                    "created_at": r[4].isoformat() if hasattr(r[4], "isoformat") else str(r[4]),
                }
                for r in rows
            ]
        return ok({"sustain_id": sustain_id, "proposals": proposals})
    except Exception as exc:
        logger.warning("get_sustain_proposals(%s) failed: %s", sustain_id, exc)
    return ok({"sustain_id": sustain_id, "proposals": []})


@router.get("/registry/operators", summary="List operator registry")
async def get_operator_registry(_: str = Depends(verify_admin)) -> dict:
    try:
        from sustena.core.operator import OPERATOR_REGISTRY
        return ok({
            "operators": {
                name: {
                    "description":  meta.description,
                    "pawa_cost":    meta.pawa_cost,
                    "license_tier": meta.license_tier,
                    "author":       meta.author,
                    "constraints":  meta.constraints,
                    "side_effects": meta.side_effects,
                    "ui_schema":    meta.ui_schema,
                    "protocol":     meta.protocol,
                }
                for name, meta in OPERATOR_REGISTRY.items()
            }
        })
    except Exception as exc:
        logger.warning("OPERATOR_REGISTRY unavailable: %s", exc)
    return ok({
        "operators": {
            "budget.allocate":      {"description": "Allocate to pocket",        "pawa_cost": 0, "license_tier": "free", "protocol": "rpc"},
            "budget.record_income": {"description": "Record income receipt",     "pawa_cost": 0, "license_tier": "free", "protocol": "rpc"},
        },
        "note": "Stub registry",
    })


@router.get("/registry/operatives", summary="List operative classes")
async def get_operative_registry(_: str = Depends(verify_admin)) -> dict:
    from sustena.core.sustain_engine import _OPERATIVE_MAP
    return ok({
        "operatives": list(_OPERATIVE_MAP.keys()),
    })


@router.websocket("/state-stream/{sustain_id}")
async def state_stream_path(websocket: WebSocket, sustain_id: str):
    """Legacy path-param WebSocket — delegates to the same stream logic."""
    token = websocket.query_params.get("token", "")
    if token != settings.admin_token:
        await websocket.close(code=status.WS_1008_POLICY_VIOLATION)
        return

    await websocket.accept()
    logger.info("WS state-stream (path) opened for %s", sustain_id)

    try:
        while True:
            try:
                from sustena.core.engine_singleton import get_shared_engine
                engine = get_shared_engine()
                state = engine.get_state(sustain_id)
            except Exception:
                state = {}

            await websocket.send_json({
                "type": "state_snapshot",
                "sustain_id": sustain_id,
                "state": state,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            })
            await asyncio.sleep(2)

    except WebSocketDisconnect:
        logger.info("WS state-stream (path) closed for %s", sustain_id)
    except Exception as exc:
        logger.error("WS state-stream (path) error for %s: %s", sustain_id, exc)
        try:
            await websocket.close()
        except Exception:
            pass


# ── 6. GET /devui/widgets ─────────────────────────────────────────────────────

@router.get("/widgets", summary="List all registered widget types")
async def list_widgets(_: str = Depends(verify_admin)) -> dict:
    """
    Returns all widget types registered in the WidgetTypeRegistry.
    Used by the UIParser Preview tab and the Mycelium Library.
    """
    from sustena.core.widget_registry import widget_registry
    return ok({"widgets": widget_registry.list_all()})


# ── 7. GET /devui/monitor-widgets ────────────────────────────────────────────

_MONITOR_STUB_STATE = {
    "finances": {
        "liquid": {"balance": 24730},
        "pockets": {
            "food":      {"allocated": 8420,  "spent": 3200, "target": 10000},
            "transport": {"allocated": 4100,  "spent": 2800, "target": 5000},
            "savings":   {"allocated": 22000, "spent": 0,    "target": 25000},
            "rent":      {"allocated": 15000, "spent": 15000,"target": 15000},
        },
    },
    "system": {
        "constraints": [
            "finances.pockets.food.allocated > 0",
            "finances.pockets.savings.allocated > 0",
        ],
    },
}


@router.get("/monitor-widgets", summary="Rendered visualize.* widgets for the Monitor Panel")
async def get_monitor_widgets(
    sustain_id: str = Query(default="homestead.bonnie"),
    _: str = Depends(verify_admin),
) -> dict:
    """
    Calls visualize.pocket_ring, visualize.event_feed, and
    visualize.constraint_health against the current sustain state and returns
    their ResponseWidget outputs.  Falls back to stub state when the engine
    is not initialised.
    """
    from sustena.core.events import EventBus
    from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext
    from sustena.core.pawa import PawaLedger
    from sustena.core.state import StateAccessor

    # Try live state first
    state_dict = None
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        state_dict = engine.get_state(sustain_id)
    except Exception:
        pass

    if not state_dict:
        state_dict = _MONITOR_STUB_STATE

    ctx = OperatorContext(
        state=StateAccessor(state_dict),
        events=EventBus(sustain_id=sustain_id),
        pawa=PawaLedger(),
        sustain_id=sustain_id,
        user_id="devui",
    )

    widgets: dict[str, Any] = {}

    for op_name, widget_key in [
        ("visualize.pocket_ring",       "pocket_ring"),
        ("visualize.event_feed",        "event_feed"),
        ("visualize.constraint_health", "constraint_health"),
    ]:
        try:
            result = await OPERATOR_REGISTRY[op_name].fn(ctx, sustain_id=sustain_id)
            widgets[widget_key] = result.data
        except Exception as exc:
            logger.warning("monitor-widgets: %s failed: %s", op_name, exc)
            widgets[widget_key] = {"widget_type": op_name.split(".")[-1], "data": {}, "summary": str(exc)}

    return ok({"sustain_id": sustain_id, "widgets": widgets})


# ── 8. POST /devui/simulate-pipeline ─────────────────────────────────────────


@router.post("/simulate-pipeline", summary="Simulate via simulate.fork → run_path → score pipeline")
async def simulate_pipeline(
    body: SimulatePipelineRequest,
    _: str = Depends(verify_admin),
) -> dict:
    """
    Chains the three simulate.* operators in sequence:
      1. simulate.fork     — snapshot live state into an in-memory fork
      2. simulate.run_path — execute proposal steps against the fork
      3. simulate.score    — score the fork's final state against goal_metric

    Returns fork_id, per-step results, and the final score.
    Falls back to stub state when the engine is not initialised.
    """
    from sustena.core.events import EventBus
    from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext
    from sustena.core.pawa import PawaLedger
    from sustena.core.state import StateAccessor

    # Try live state first
    state_dict = None
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        state_dict = engine.get_state(body.sustain_id)
    except Exception:
        pass

    if not state_dict:
        state_dict = _MONITOR_STUB_STATE

    ctx = OperatorContext(
        state=StateAccessor(state_dict),
        events=EventBus(sustain_id=body.sustain_id),
        pawa=PawaLedger(),
        sustain_id=body.sustain_id,
        user_id="devui",
    )

    # Step 1: fork
    fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx, sustain_id=body.sustain_id)
    if not fork_result.succeeded:
        return ok({
            "sustain_id": body.sustain_id,
            "error": fork_result.reason,
            "steps": [],
            "score": None,
        })
    fork_id: str = fork_result.data["fork_id"]

    # Step 2: run_path
    path_result = await OPERATOR_REGISTRY["simulate.run_path"].fn(
        ctx,
        fork_id=fork_id,
        operator_sequence=body.proposal,
    )

    steps = path_result.data.get("steps", []) if path_result.succeeded else []

    # Step 3: score
    score_result = await OPERATOR_REGISTRY["simulate.score"].fn(
        ctx,
        fork_id=fork_id,
        goal_metric=body.goal_metric,
    )

    score_data = score_result.data if score_result.succeeded else {}

    return ok({
        "sustain_id": body.sustain_id,
        "fork_id": fork_id,
        "goal_metric": body.goal_metric,
        "steps": steps,
        "steps_run": len(steps),
        "steps_succeeded": sum(1 for s in steps if s.get("status") == "ok"),
        "score": score_data.get("score"),
        "interpretation": score_data.get("interpretation"),
        "final_state": path_result.data.get("final_state", {}) if path_result.succeeded else {},
    })


# ── 10. POST /devui/preview-widget ───────────────────────────────────────────

@router.post("/preview-widget", summary="Render a widget from a spec JSON (dev console preview)")
async def preview_widget(
    body: PreviewWidgetRequest,
    _: str = Depends(verify_admin),
) -> dict:
    """
    Accepts a ui_schema (or full operator spec) and optional mock state,
    returns a rendered ResponseWidget dict.

    Used by the dev console UI Preview tab (debounced 500ms POST on every edit).
    """
    from sustena.core.uiparser import UISchemaParser
    parser = UISchemaParser()

    ui_schema_dict = body.spec_json.get("ui_schema", body.spec_json)
    mock_inputs = body.mock_state.get("inputs", {})

    try:
        schema = parser.parse(ui_schema_dict)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=f"Invalid ui_schema: {exc}")

    resolved_fields = []
    for f in schema.fields:
        value = parser.resolve_source(f.source, mock_inputs, body.mock_state)
        resolved_fields.append({
            "label": f.label,
            "value": value,
            "display": f.display,
            "colour_rule": f.colour_rule,
        })

    widget = {
        "type": schema.widget_type,
        "data": {
            "fields": resolved_fields,
            "ctas": schema.ctas,
            **({"chart": schema.chart} if schema.chart else {}),
        },
        "summary": f"Preview: {schema.widget_type}",
    }

    return ok({"widget": widget})
