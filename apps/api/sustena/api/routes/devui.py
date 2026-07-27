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
GET  /devui/sustain/{id}/graph        — operative council graph for a sustain
GET  /devui/sustain/{id}/operators    — operators allowed by the sustain spec

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

from sustena.api.routes.users import get_current_user, get_user_from_token

router = APIRouter()
logger = logging.getLogger(__name__)

# Every route below requires a real user session (get_current_user, a JWT
# obtained via POST /api/v1/users/login or /register) — this used to be a
# single shared ADMIN_TOKEN, which meant anyone with the token had full
# access and the token itself had to live in the public frontend bundle.


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
async def list_sustains(_: dict = Depends(get_current_user)) -> dict:
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


# ── 1b. GET /devui/templates ──────────────────────────────────────────────────

@router.get("/templates", summary="Available sustain spec templates for creation")
async def list_templates(_: dict = Depends(get_current_user)) -> dict:
    """
    List the sustain spec templates that can be instantiated (homestead, vyyb,
    chama, …). Powers the 'create sustain' control in the UI.
    """
    import json as _json
    from pathlib import Path as _Path

    specs_dir = _Path(__file__).resolve().parent.parent.parent / "sustains"
    templates: list[dict] = []
    for path in sorted(specs_dir.glob("*.json")):
        try:
            spec = _json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        # operatives may be a dict (homestead) or a list (vyyb, chama, …).
        ops = spec.get("operatives") or {}
        operative_names = list(ops.keys()) if isinstance(ops, dict) else list(ops)
        templates.append({
            "template_id":  path.stem,
            "display_name": spec.get("display_name") or path.stem.replace("_", " ").title(),
            "description":  spec.get("description", ""),
            "operatives":   operative_names,
            "parameters":   spec.get("parameters", []),
        })
    return ok({"templates": templates})


# ── 1c. POST /devui/sustains ──────────────────────────────────────────────────

class CreateSustainRequest(BaseModel):
    template_id: str = Field(description="Spec template to instantiate, e.g. 'homestead'")
    user_id: str = Field(default="owner", description="Owner user id for the new sustain")
    parameters: dict = Field(
        default_factory=dict,
        description="Spec parameters. owner_ids defaults to [user_id] when omitted.",
    )


@router.post("/sustains", summary="Create + hydrate a sustain from a template")
async def create_sustain(body: CreateSustainRequest, _: dict = Depends(get_current_user)) -> dict:
    """
    Instantiate a fully-hydrated sustain via SustainEngine — operators allowed,
    operatives enabled, and an initial state built from the template's
    default_state. Returns the new sustain_id plus the selector-shaped entry.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    params = dict(body.parameters or {})
    params.setdefault("owner_ids", [body.user_id])

    try:
        sustain_id = engine.instantiate(body.template_id, body.user_id, params)
    except ValueError as exc:
        # Unknown template spec or a required parameter is missing.
        raise HTTPException(status_code=422, detail=str(exc))

    entry = next(
        (s for s in engine.list_all() if s.get("id") == sustain_id),
        {"id": sustain_id, "template_id": body.template_id, "status": "live"},
    )
    logger.info("Created sustain %s from template %s", sustain_id, body.template_id)
    return ok({"sustain_id": sustain_id, "sustain": entry})


# ── Shared: visualize.* widget computation ───────────────────────────────────
# Used by both GET /devui/state and GET /devui/monitor-widgets so the Monitor
# panel's pocket ring / event feed / constraint health widgets are computed
# from the SAME state snapshot the rest of the panel reads — one computation,
# not two independently-fetched ones that can disagree at a given instant.

async def _get_proposals_in_voting(sustain_id: str) -> list[dict]:
    """
    Council proposals still awaiting a vote — a real, already-computable
    needs-attention signal (council_proposals already exists; this isn't
    new backend state, just a read the Monitor never surfaced before).
    Best-effort: returns [] on any failure rather than breaking /devui/state.
    """
    try:
        from sqlalchemy import text as _text
        from sustena.db.schema import get_engine as _get_sqlalchemy_engine

        db_engine = _get_sqlalchemy_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                _text(
                    "SELECT id, proposed_by, operator_name, created_at "
                    "FROM council_proposals WHERE sustain_id = :sid AND status = 'IN_VOTING' "
                    "ORDER BY created_at DESC"
                ),
                {"sid": sustain_id},
            )
            return [
                {
                    "id": r[0],
                    "proposed_by": r[1],
                    "operator_name": r[2],
                    "created_at": r[3].isoformat() if hasattr(r[3], "isoformat") else str(r[3]),
                }
                for r in rows
            ]
    except Exception as exc:
        logger.debug("_get_proposals_in_voting(%s) failed: %s", sustain_id, exc)
        return []


async def _get_ingest_attention(sustain_id: str) -> dict:
    """
    Ingest pipeline (Slice 4) needs-attention signal for the Monitor: unresolved
    needs_attention messages (unparsed/parsed-but-unmapped captures) and stale
    capture sources for this sustain. Same best-effort-empty-on-failure contract
    as _get_proposals_in_voting -- this must never break /devui/state.
    """
    try:
        from sustena.core.ingest_singleton import get_shared_ingest_engine

        ingest = get_shared_ingest_engine()
        messages = ingest.needs_attention(sustain_id=sustain_id)
        sources = [s for s in ingest.get_sources(sustain_id=sustain_id) if s["is_stale"]]
        return {"messages": messages, "stale_sources": sources}
    except Exception as exc:
        logger.debug("_get_ingest_attention(%s) failed: %s", sustain_id, exc)
        return {"messages": [], "stale_sources": []}


async def _compute_monitor_widgets(
    sustain_id: str,
    state_dict: dict,
    constraint_exprs: list[str],
    real_events: list,
) -> dict[str, Any]:
    from sustena.core.events import EventBus
    from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext
    from sustena.core.pawa import PawaLedger
    from sustena.core.state import StateAccessor

    ctx = OperatorContext(
        state=StateAccessor(state_dict),
        events=EventBus(sustain_id=sustain_id),
        pawa=PawaLedger(),
        sustain_id=sustain_id,
        user_id="devui",
    )

    widgets: dict[str, Any] = {}

    for op_name, widget_key, extra_kwargs in [
        ("visualize.pocket_ring",       "pocket_ring",       {}),
        ("visualize.event_feed",        "event_feed",        {}),
        ("visualize.constraint_health", "constraint_health", {"constraints": constraint_exprs}),
    ]:
        try:
            result = await OPERATOR_REGISTRY[op_name].fn(ctx, sustain_id=sustain_id, **extra_kwargs)
            widgets[widget_key] = result.data
        except Exception as exc:
            logger.warning("monitor-widgets: %s failed: %s", op_name, exc)
            widgets[widget_key] = {"widget_type": op_name.split(".")[-1], "data": {}, "summary": str(exc)}

    # visualize.event_feed runs in a fresh context with no events; populate the
    # widget from the persisted events table so the Monitor feed + counter are real.
    widgets["event_feed"] = {
        "widget_type": "event_feed",
        "data": {
            "events": [
                {"event_name": e["event_name"], "timestamp": e["timestamp"], "payload": e["payload"]}
                for e in real_events
            ],
            "total": len(real_events),
            "sustain_id": sustain_id,
        },
        "summary": f"{len(real_events)} recent event(s)",
    }

    return widgets


# ── 2. GET /devui/state?sustain_id= ──────────────────────────────────────────

@router.get("/state", summary="Full current state for a sustain")
async def get_state(
    sustain_id: str = Query(..., description="e.g. homestead.bonnie"),
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Returns pockets, events feed, operative statuses, constraint health,
    proposals awaiting a vote, and the visualize.* Monitor widgets — all
    computed from one state snapshot so every card on the Monitor panel
    (including the needs-attention block) reflects the same instant.
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
        events = engine.get_events(sustain_id, limit=20)
    except Exception as exc:
        logger.debug("get_events(%s) failed: %s", sustain_id, exc)

    constraint_exprs = [c["expr"] for c in constraints]
    widgets = await _compute_monitor_widgets(sustain_id, state, constraint_exprs, events)
    proposals_in_voting = await _get_proposals_in_voting(sustain_id)
    ingest_attention = await _get_ingest_attention(sustain_id)

    return ok({
        "sustain_id": sustain_id,
        "state": state,
        "events": events,
        "operatives": operatives,
        "constraints": constraints,
        "widgets": widgets,
        "proposals_in_voting": proposals_in_voting,
        "ingest_attention": ingest_attention,
    })


# ── 3. POST /devui/console/execute ────────────────────────────────────────────

@router.post("/console/execute", summary="Execute an operator from the dev console")
async def console_execute(
    body: ConsoleExecuteRequest,
    _: dict = Depends(get_current_user),
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
        try:
            recent_events = engine.get_events(body.sustain_id, limit=10)
        except Exception:
            recent_events = []
        return ok({
            "result": result.to_response(),
            "sustain_id": body.sustain_id,
            "operator": operator_name,
            "events": recent_events,
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
    _: dict = Depends(get_current_user),
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

    token= must be a valid user session JWT (same one used for REST calls) —
    browsers can't set a custom Authorization header on a WS handshake, so
    the token travels as a query param instead. Validated the same way as
    every other route, including the token_version / logout check.
    """
    token = websocket.query_params.get("token", "")
    if not await get_user_from_token(token):
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
async def get_sustain_state(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    """Delegates to the canonical query-param handler."""
    return await get_state(sustain_id=sustain_id, _=_)


@router.get("/sustain/{sustain_id}/events", summary="Paginated event log")
async def get_sustain_events(
    sustain_id: str,
    limit: int = 100,
    _: dict = Depends(get_current_user),
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
async def get_sustain_proposals(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
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
async def get_operator_registry(_: dict = Depends(get_current_user)) -> dict:
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
async def get_operative_registry(_: dict = Depends(get_current_user)) -> dict:
    from sustena.core.sustain_engine import _OPERATIVE_MAP
    return ok({
        "operatives": list(_OPERATIVE_MAP.keys()),
    })


@router.websocket("/state-stream/{sustain_id}")
async def state_stream_path(websocket: WebSocket, sustain_id: str):
    """Legacy path-param WebSocket — delegates to the same stream logic."""
    token = websocket.query_params.get("token", "")
    if not await get_user_from_token(token):
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
async def list_widgets(_: dict = Depends(get_current_user)) -> dict:
    """
    Returns all widget types registered in the WidgetTypeRegistry.
    Used by the UIParser Preview tab and the Mycelium Library.
    """
    from sustena.core.widget_registry import widget_registry
    return ok({"widgets": widget_registry.list_all()})


# ── 7. GET /devui/monitor-widgets ────────────────────────────────────────────

@router.get("/monitor-widgets", summary="Rendered visualize.* widgets for the Monitor Panel")
async def get_monitor_widgets(
    sustain_id: str = Query(default="homestead.bonnie"),
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Calls visualize.pocket_ring, visualize.event_feed, and
    visualize.constraint_health against the current sustain state and returns
    their ResponseWidget outputs.  Returns empty-state widgets when the engine
    is not initialised or the sustain has no state.

    Kept as a standalone endpoint for callers that only need the widgets (e.g.
    the shell's event-count heartbeat); GET /devui/state uses the same
    _compute_monitor_widgets() helper so the two never compute this differently.
    """
    state_dict: dict = {}
    constraint_exprs: list[str] = []
    real_events: list = []
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        state_dict = engine.get_state(sustain_id) or {}
        constraint_data = engine.evaluate_constraints(sustain_id)
        constraint_exprs = [c["expr"] for c in constraint_data]
        real_events = engine.get_events(sustain_id, limit=20)
    except Exception:
        pass

    widgets = await _compute_monitor_widgets(sustain_id, state_dict, constraint_exprs, real_events)
    return ok({"sustain_id": sustain_id, "widgets": widgets})


# ── 8. POST /devui/simulate-pipeline ─────────────────────────────────────────


@router.post("/simulate-pipeline", summary="Simulate via simulate.fork → run_path → score pipeline")
async def simulate_pipeline(
    body: SimulatePipelineRequest,
    _: dict = Depends(get_current_user),
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

    state_dict: dict = {}
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        state_dict = engine.get_state(body.sustain_id) or {}
    except Exception:
        pass

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
    _: dict = Depends(get_current_user),
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


# ── 11. GET /devui/sustain/{id}/graph ─────────────────────────────────────────

_OPERATIVE_ROLE_LABELS: dict[str, str] = {
    "mentor":    "Strategic advisor · finance",
    "protege":   "Learning · pattern recognition",
    "attache":   "Contacts & governance",
    "navigator": "Logistics & routing",
    "curator":   "Assets & procurement",
}


@router.get("/sustain/{sustain_id}/graph", summary="Operative council graph for a sustain")
async def get_sustain_graph(
    sustain_id: str,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Returns the operative council graph for a sustain — nodes (Orchie +
    council operatives + their sub-operatives) and edges (delegation
    relationships).  Used by the Orchie panel graph tree.

    Returns empty nodes/edges when the sustain is not found.
    """
    spec = None
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        spec = engine.get_spec(sustain_id)
    except Exception as exc:
        logger.debug("get_sustain_graph(%s) engine error: %s", sustain_id, exc)

    if spec is None:
        return ok({"sustain_id": sustain_id, "nodes": [], "edges": []})

    operatives_cfg = spec.get("operatives", {})
    if not isinstance(operatives_cfg, dict):
        operatives_cfg = {name: {} for name in operatives_cfg}

    nodes: list[dict] = [
        {
            "id":    "orchie",
            "label": "Orchie",
            "role":  "AI orchestrator",
            "type":  "orchie",
        }
    ]
    edges: list[dict] = []

    for name, cfg in operatives_cfg.items():
        if not isinstance(cfg, dict):
            cfg = {}
        nodes.append({
            "id":     name,
            "label":  name.capitalize(),
            "role":   _OPERATIVE_ROLE_LABELS.get(name, "operative"),
            "type":   "operative",
            "domain": cfg.get("domain", []),
        })
        edges.append({"from": "orchie", "to": name, "type": "council"})

        for sub_name in cfg.get("sub_operatives", {}):
            sub_id = f"{name}.{sub_name}"
            nodes.append({
                "id":     sub_id,
                "label":  sub_name.replace("_", " ").title(),
                "role":   "sub-operative",
                "type":   "sub_operative",
                "parent": name,
            })
            edges.append({"from": name, "to": sub_id, "type": "delegation"})

    return ok({
        "sustain_id":  sustain_id,
        "template_id": spec.get("id", ""),
        "nodes":       nodes,
        "edges":       edges,
    })


# ── 12. GET /devui/library ───────────────────────────────────────────────────

@router.get("/library", summary="Arena packages grouped by kind for the Library panel")
async def get_library(_: dict = Depends(get_current_user)) -> dict:
    """
    Returns all arena_packages grouped by kind (operative, operator, spore, widget).
    Used by the LibraryPanel in the DevUI. Falls back to empty groups when the
    arena_packages table is empty or unavailable.
    """
    import json as _json
    from sqlalchemy import text as _text
    from sustena.db.schema import get_engine

    groups: dict[str, list] = {"operative": [], "operator": [], "spore": [], "widget": []}
    try:
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                _text(
                    "SELECT id, name, kind, author_id, trust_score, download_count, "
                    "pawa_cost, is_free, version, tags, description, created_at "
                    "FROM arena_packages ORDER BY trust_score DESC"
                )
            )
            for r in rows:
                kind = r[2]
                tags = []
                try:
                    if r[9]:
                        tags = _json.loads(r[9])
                except Exception:
                    pass
                pkg = {
                    "id":             r[0],
                    "name":           r[1],
                    "kind":           kind,
                    "author":         r[3] or "unknown",
                    "trust":          round((r[4] or 0) * 100),
                    "downloads":      r[5] or 0,
                    "pawa":           "free" if r[7] else r[6],
                    "version":        r[8] or "1.0.0",
                    "tags":           tags,
                    "desc":           r[10] or "",
                    "created_at":     r[11].isoformat() if hasattr(r[11], "isoformat") else str(r[11]),
                }
                if kind in groups:
                    groups[kind].append(pkg)
    except Exception as exc:
        logger.warning("get_library failed: %s", exc)

    return ok({
        "operatives": groups["operative"],
        "operators":  groups["operator"],
        "spores":     groups["spore"],
        "widgets":    groups["widget"],
        "total":      sum(len(v) for v in groups.values()),
    })


# ── 13. GET /devui/sustain/{id}/operators ─────────────────────────────────────

@router.get("/sustain/{sustain_id}/operators", summary="Operators allowed by a sustain spec")
async def get_sustain_operators(
    sustain_id: str,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Returns the operator list declared in the sustain's spec under the "operators"
    key.  Each entry is augmented with the protocol field from OPERATOR_REGISTRY
    when available.  Used by the SimulatorPanel operator-selector dropdown.
    Returns an empty list when the sustain is unknown or has no operators declared.
    """
    spec = None
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        spec = engine.get_spec(sustain_id)
    except Exception as exc:
        logger.debug("get_sustain_operators(%s) engine error: %s", sustain_id, exc)

    if spec is None:
        return ok({"sustain_id": sustain_id, "operators": []})

    spec_ops = spec.get("operators", [])

    try:
        from sustena.core.operator import OPERATOR_REGISTRY
        registry = OPERATOR_REGISTRY
    except Exception:
        registry = {}

    result = []
    for op in spec_ops:
        name = op.get("name", "") if isinstance(op, dict) else str(op)
        if not name:
            continue
        meta = registry.get(name)
        result.append({
            "name": name,
            "description": op.get("description", meta.description if meta else ""),
            "params": op.get("params", []),
            "pawa_cost": op.get("pawa_cost", meta.pawa_cost if meta else 0),
            "protocol": meta.protocol if meta else "rpc",
        })

    return ok({"sustain_id": sustain_id, "operators": result})
