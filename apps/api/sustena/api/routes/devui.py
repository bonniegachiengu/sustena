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


# ── 1. GET /devui/sustains ────────────────────────────────────────────────────

@router.get("/sustains", summary="List sustains for the selector dropdown")
async def list_sustains(_: str = Depends(verify_admin)) -> dict:
    """
    Returns all sustains with id, label, status, pockets summary, and
    active operative count — enough for the TopBar selector and Monitor hero tiles.
    """
    try:
        from sustena.core.sustain_engine import SustainEngine
        engine = SustainEngine()
        raw = engine.list_all()
        if raw:
            return ok({"sustains": raw})
    except Exception as exc:
        logger.debug("SustainEngine.list_all not available: %s", exc)

    # Stub fallback — shown when the DB has no seeded sustains
    return ok({
        "sustains": [
            {
                "id": "homestead.bonnie",
                "label": "Homestead",
                "sub": "bonnie",
                "status": "live",
                "score": 84,
                "pockets": {
                    "food":      8420,
                    "transport": 4100,
                    "savings":   22000,
                    "rent":      15000,
                },
                "active_operatives": ["mentor", "curator", "navigator"],
                "pawa_balance": 8420,
            },
            {
                "id": "vyyb.hive",
                "label": "Vyyb Hive",
                "sub": "hive",
                "status": "live",
                "score": 91,
                "pockets": {"events": 45000, "talent": 18000, "operations": 9400},
                "active_operatives": ["mentor", "navigator"],
                "pawa_balance": 3210,
            },
            {
                "id": "mkulima.alpha",
                "label": "Mkulima",
                "sub": "alpha",
                "status": "seed",
                "score": 55,
                "pockets": {"inputs": 12000, "harvest": 0},
                "active_operatives": [],
                "pawa_balance": 500,
            },
            {
                "id": "sustena.xii",
                "label": "Sustena XII",
                "sub": "core",
                "status": "live",
                "score": 99,
                "pockets": {"ops": 80000, "dev": 40000},
                "active_operatives": ["mentor", "curator", "navigator", "protege", "attache"],
                "pawa_balance": 99999,
            },
            {
                "id": "chama.nairobi",
                "label": "Chama Nairobi",
                "sub": "circle",
                "status": "seed",
                "score": 61,
                "pockets": {"pool": 198400, "emergency": 25000},
                "active_operatives": ["curator"],
                "pawa_balance": 1200,
            },
        ]
    })


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
    try:
        from sustena.core.sustain_engine import SustainEngine
        engine = SustainEngine()
        state = engine.get_state(sustain_id)
        return ok({"sustain_id": sustain_id, "state": state})
    except Exception as exc:
        logger.debug("get_state(%s) failed: %s", sustain_id, exc)

    # TODO: wire real — load from sustain_states table
    return ok({
        "sustain_id": sustain_id,
        "state": {
            "finances": {
                "cash_position": 184250,
                "pockets": {
                    "food":      {"allocated": 8420,  "target": 10000, "status": "amber"},
                    "transport": {"allocated": 4100,  "target": 5000,  "status": "ok"},
                    "savings":   {"allocated": 22000, "target": 25000, "status": "ok"},
                    "rent":      {"allocated": 15000, "target": 15000, "status": "ok"},
                },
                "burn_rate": 4214,
                "income":    {"amount": 0, "currency": "KSH"},
                "goals": [],
            },
            "pantry": {
                "cooking_oil_L": 0.4,
                "tomatoes_kg":   2.8,
                "rice_kg":       3.0,
            },
            "system": {
                "pawa_balance":    8420,
                "api_p95_ms":      428,
                "ops_per_min":     12,
                "orchie_load_pct": 23,
            },
            "council": {
                "quorum_pct": 78,
                "open_proposals": 1,
            },
            "chama": {
                "contributions": 198400,
                "members":       12,
            },
        },
        "events": [
            # TODO: wire real — query EventBus.get_history()
            {
                "id": "evt-001",
                "event_name": "event.finances.pocket_allocated",
                "payload": {"pocket": "food", "amount": 8420},
                "timestamp": datetime.now(timezone.utc).isoformat(),
            },
            {
                "id": "evt-002",
                "event_name": "event.pantry.low_stock_alert",
                "payload": {"item": "cooking_oil_L", "current": 0.4, "threshold": 1.0},
                "timestamp": datetime.now(timezone.utc).isoformat(),
            },
        ],
        "operatives": [
            # TODO: wire real — query operative instance statuses
            {
                "id": "op-mentor",
                "name": "Mentor",
                "role": "Strategic advisor · finance",
                "status": "active",
                "confidence": 88,
                "pawa_session": 42,
                "task": "Monitoring burn rate deviation from weekly plan.",
            },
            {
                "id": "op-curator",
                "name": "Curator",
                "role": "Pantry & procurement",
                "status": "alert",
                "confidence": 71,
                "pawa_session": 18,
                "task": "Cooking oil at critical threshold. Awaiting approval.",
            },
            {
                "id": "op-navigator",
                "name": "Navigator",
                "role": "Council & governance",
                "status": "active",
                "confidence": 94,
                "pawa_session": 55,
                "task": "Tracking SUS-0148 quorum. 1h 22m to close.",
            },
            {
                "id": "op-protege",
                "name": "Protégé",
                "role": "Learning · pattern recognition",
                "status": "idle",
                "confidence": 65,
                "pawa_session": 8,
                "task": "Observing Mentor's burn analysis.",
            },
        ],
        "constraints": [
            # TODO: wire real — run ConstraintEngine against live state
            {"expr": "finances.cash_position > 0",          "status": "ok",    "value": 184250},
            {"expr": "finances.pockets.food.allocated >= 0", "status": "ok",    "value": 8420},
            {"expr": "pantry.cooking_oil_L >= 1.0",          "status": "fail",  "value": 0.4},
            {"expr": "council.quorum_pct >= 80",             "status": "amber", "value": 78},
        ],
        "note": "Stub state — SustainEngine not initialised",
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
        from sustena.core.sustain_engine import SustainEngine
        engine = SustainEngine()
        result = await engine.execute_operator(
            sustain_id=body.sustain_id,
            operator_name=operator_name,
            params=params,
        )
        return ok({
            "result": result.to_response(),
            "sustain_id": body.sustain_id,
            "operator": operator_name,
            "events": [],  # TODO: wire real — return EventBus.published_this_context()
        })
    except Exception as exc:
        logger.warning("console_execute failed: %s", exc)
        # TODO: wire real — engine not initialised yet
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
        from sustena.core.sustain_engine import SustainEngine
        engine = SustainEngine()
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
        # TODO: wire real — return actual projected deltas
        stub_steps = []
        for step in body.proposal:
            stub_steps.append({
                "operator": step.get("operator", "?"),
                "params":   step.get("params", {}),
                "result": {
                    "status": "ok",
                    "data": {"note": "Stub simulation step"},
                },
                "state_after": {
                    "finances": {"cash_position": 184250, "_stub": True},
                },
                "constraint_results": [
                    {"expr": "finances.cash_position > 0", "status": "ok"},
                ],
            })
        return ok({
            "sustain_id": body.sustain_id,
            "steps": stub_steps,
            "final_state": {"_stub": True, "note": f"Engine not ready: {exc}"},
            "constraint_satisfaction": {
                "total": 4,
                "passing": 3,
                "failing": 1,
                "details": [
                    {"expr": "finances.cash_position > 0",          "status": "ok"},
                    {"expr": "pantry.cooking_oil_L >= 1.0",          "status": "fail"},
                    {"expr": "council.quorum_pct >= 80",             "status": "amber"},
                    {"expr": "finances.pockets.food.allocated >= 0", "status": "ok"},
                ],
            },
            "note": "Stub simulation",
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
                from sustena.core.sustain_engine import SustainEngine
                engine = SustainEngine()
                state = engine.get_state(sustain_id)
            except Exception:
                # TODO: wire real — connect to a shared SustainEngine singleton
                state = {
                    "_stub": True,
                    "finances": {
                        "cash_position": 184250,
                        "burn_rate": 4214,
                        "pockets": {"food": 8420, "transport": 4100},
                    },
                    "system": {"pawa_balance": 8420, "api_p95_ms": 428, "ops_per_min": 12, "orchie_load_pct": 23},
                }

            await websocket.send_json({
                "type": "state_snapshot",
                "sustain_id": sustain_id,
                "state": state,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            })
            # TODO: wire real — subscribe to EventBus and push event deltas instead of polling
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
    # TODO: wire real
    stub = [
        {
            "id": f"evt-{i}",
            "event_name": "event.finances.pocket_allocated",
            "payload": {"pocket": "food", "amount": 8420},
            "timestamp": datetime.now(timezone.utc).isoformat(),
        }
        for i in range(min(limit, 5))
    ]
    return ok({"sustain_id": sustain_id, "events": stub, "count": len(stub), "note": "Stub"})


@router.get("/sustain/{sustain_id}/proposals", summary="Council proposals")
async def get_sustain_proposals(sustain_id: str, _: str = Depends(verify_admin)) -> dict:
    # TODO: wire real — query council proposals from DB
    return ok({
        "sustain_id": sustain_id,
        "proposals": [
            {
                "id": "SUS-0148",
                "title": "Advance Q3 disbursement",
                "status": "awaiting",
                "council": {"for": 7, "against": 2, "abstain": 1},
            },
        ],
        "note": "Stub proposals",
    })


@router.get("/registry/operators", summary="List operator registry")
async def get_operator_registry(_: str = Depends(verify_admin)) -> dict:
    try:
        from sustena.core.operator import OPERATOR_REGISTRY
        return ok({
            "operators": {
                name: {
                    "description": meta.description,
                    "pawa_cost":   meta.pawa_cost,
                    "license_tier": meta.license_tier,
                    "author":      meta.author,
                    "constraints": meta.constraints,
                    "side_effects": meta.side_effects,
                    "ui_schema":   meta.ui_schema,
                }
                for name, meta in OPERATOR_REGISTRY.items()
            }
        })
    except Exception as exc:
        logger.warning("OPERATOR_REGISTRY unavailable: %s", exc)
    # TODO: wire real
    return ok({
        "operators": {
            "budget.allocate":    {"description": "Allocate to pocket",         "pawa_cost": 0,    "license_tier": "free"},
            "budget.record_income": {"description": "Record income receipt",    "pawa_cost": 0,    "license_tier": "free"},
            "mpesa.parse":        {"description": "Parse M-PESA message",       "pawa_cost": 0,    "license_tier": "free"},
            "pantry.consume":     {"description": "Log pantry consumption",     "pawa_cost": 0,    "license_tier": "free"},
            "chama.contribute":   {"description": "Record chama contribution",  "pawa_cost": 0,    "license_tier": "free"},
        },
        "note": "Stub registry",
    })


@router.get("/registry/operatives", summary="List operative classes")
async def get_operative_registry(_: str = Depends(verify_admin)) -> dict:
    # TODO: wire real — expose _OPERATIVE_MAP from SustainEngine
    return ok({
        "operatives": ["mentor", "protege", "attache", "navigator", "curator"],
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
                from sustena.core.sustain_engine import SustainEngine
                engine = SustainEngine()
                state = engine.get_state(sustain_id)
            except Exception:
                state = {"_stub": True, "finances": {"cash_position": 184250, "burn_rate": 4214}}

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
