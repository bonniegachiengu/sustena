"""
sustena/api/routes/sustains.py

REST endpoints for sustain instances — Epic 1.4.2.

Endpoints (all under /api/v1/sustains, mounted by main.py):
  POST   /                                  — create a sustain
  GET    /{sustain_id}                      — get current state
  POST   /{sustain_id}/operators/{op_name} — execute an operator
  GET    /{sustain_id}/events               — event log
  GET    /{sustain_id}/proposals            — council proposals
  POST   /{sustain_id}/proposals/{pid}/vote — vote on a proposal
  GET    /{sustain_id}/export               — download full spec + state

All endpoints require a valid JWT bearer token. The JWT payload must contain
a `user_id` field. Tokens are signed with settings.secret_key (HS256).

Standard response envelope:
  {"status": "ok"|"error", "data": ..., "error": null|"<message>", "timestamp": "<ISO8601>"}
"""

import json
import logging
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from fastapi.responses import JSONResponse, Response
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer
from pydantic import BaseModel

from jose import jwt, JWTError
from jose.exceptions import ExpiredSignatureError

from sustena.config import settings
from sustena.core.sustain_engine import SustainEngine
from sustena.core.council import CouncilSession
from sustena.core.state import StateAccessor

logger = logging.getLogger(__name__)

router = APIRouter()

# ── JWT auth ──────────────────────────────────────────────────────────────────

_bearer_scheme = HTTPBearer(auto_error=False)


def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def _ok(data: Any) -> dict:
    return {"status": "ok", "data": data, "error": None, "timestamp": _now_iso()}


def _err(msg: str, status_code: int = 400) -> JSONResponse:
    return JSONResponse(
        status_code=status_code,
        content={"status": "error", "data": None, "error": msg, "timestamp": _now_iso()},
    )


def get_current_user(
    credentials: HTTPAuthorizationCredentials | None = Depends(_bearer_scheme),
) -> dict:
    """
    Validate the JWT bearer token and return the decoded payload.
    Raises HTTP 401 if the token is missing, expired, or invalid.
    """
    if credentials is None:
        raise HTTPException(status_code=401, detail="Unauthorized")

    token = credentials.credentials
    try:
        payload = jwt.decode(
            token,
            settings.secret_key,
            algorithms=["HS256"],
        )
    except ExpiredSignatureError:
        raise HTTPException(status_code=401, detail="Token expired")
    except JWTError:
        raise HTTPException(status_code=401, detail="Unauthorized")

    if "user_id" not in payload:
        raise HTTPException(status_code=401, detail="Unauthorized")

    return payload


# ── Engine dependency (overridable in tests) ───────────────────────────────────

_shared_engine: SustainEngine | None = None


def get_engine() -> SustainEngine:
    global _shared_engine
    if _shared_engine is None:
        _shared_engine = SustainEngine()
    return _shared_engine


# ── Request bodies ─────────────────────────────────────────────────────────────

class CreateSustainBody(BaseModel):
    template_id: str
    parameters: dict = {}


class ExecuteOperatorBody(BaseModel):
    params: dict = {}


class VoteBody(BaseModel):
    vote: str  # "YES" | "NO"


# ── Helper: ownership check ────────────────────────────────────────────────────

def _assert_owns(engine: SustainEngine, sustain_id: str, user_id: str) -> None:
    """
    Raise HTTP 404 if sustain doesn't exist, HTTP 403 if user doesn't own it.
    Using 404 for both non-existence and ownership failures to avoid leaking IDs.
    """
    row = engine._db.execute(
        "SELECT user_id FROM sustains WHERE id = ?",
        (sustain_id,),
    ).fetchone()
    if row is None:
        raise HTTPException(status_code=404, detail="Sustain not found")
    if row["user_id"] != user_id:
        raise HTTPException(status_code=404, detail="Sustain not found")


# ── POST / — create sustain ───────────────────────────────────────────────────

@router.post("/", status_code=201)
async def create_sustain(
    body: CreateSustainBody,
    user: dict = Depends(get_current_user),
    engine: SustainEngine = Depends(get_engine),
):
    """Instantiate a new sustain from a template."""
    user_id = user["user_id"]
    try:
        sustain_id = engine.instantiate(body.template_id, user_id, body.parameters)
    except ValueError as exc:
        return _err(str(exc), status_code=422)
    except Exception as exc:
        logger.error("create_sustain error: %s", exc)
        return _err("Internal error", status_code=500)

    # Read back created_at from DB
    row = engine._db.execute(
        "SELECT created_at, template_id FROM sustains WHERE id = ?",
        (sustain_id,),
    ).fetchone()

    return JSONResponse(
        status_code=201,
        content=_ok({
            "sustain_id": sustain_id,
            "template_id": row["template_id"],
            "created_at": row["created_at"],
        }),
    )


# ── GET /{sustain_id} — get state ─────────────────────────────────────────────

@router.get("/{sustain_id}")
async def get_sustain(
    sustain_id: str,
    user: dict = Depends(get_current_user),
    engine: SustainEngine = Depends(get_engine),
):
    """Return the current state of a sustain."""
    _assert_owns(engine, sustain_id, user["user_id"])
    try:
        state = engine.get_state(sustain_id)
    except ValueError:
        raise HTTPException(status_code=404, detail="Sustain not found")
    return _ok(state)


# ── POST /{sustain_id}/operators/{operator_name} ──────────────────────────────

@router.post("/{sustain_id}/operators/{operator_name}")
async def execute_operator(
    sustain_id: str,
    operator_name: str,
    body: ExecuteOperatorBody,
    user: dict = Depends(get_current_user),
    engine: SustainEngine = Depends(get_engine),
):
    """Execute a named operator against the sustain's live state."""
    _assert_owns(engine, sustain_id, user["user_id"])
    result = await engine.execute_operator(sustain_id, operator_name, body.params)
    if result.failed:
        return JSONResponse(
            status_code=422,
            content=_ok(result.to_response()),
        )
    return _ok(result.to_response())


# ── GET /{sustain_id}/events ──────────────────────────────────────────────────

@router.get("/{sustain_id}/events")
async def get_events(
    sustain_id: str,
    limit: int = Query(default=50, ge=1, le=500),
    offset: int = Query(default=0, ge=0),
    user: dict = Depends(get_current_user),
    engine: SustainEngine = Depends(get_engine),
):
    """Return the event log for a sustain (from DB if available)."""
    _assert_owns(engine, sustain_id, user["user_id"])

    # Try to read from events table; fall back to empty list gracefully
    try:
        rows = engine._db.execute(
            "SELECT * FROM events WHERE sustain_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            (sustain_id, limit, offset),
        ).fetchall()
        events = [dict(r) for r in rows]
    except Exception:
        events = []

    return _ok({"events": events, "limit": limit, "offset": offset})


# ── GET /{sustain_id}/proposals ───────────────────────────────────────────────

@router.get("/{sustain_id}/proposals")
async def get_proposals(
    sustain_id: str,
    user: dict = Depends(get_current_user),
    engine: SustainEngine = Depends(get_engine),
):
    """Return all council proposals for a sustain (from state)."""
    _assert_owns(engine, sustain_id, user["user_id"])
    state_dict = engine.get_state(sustain_id)
    proposals = state_dict.get("council_proposals", [])
    return _ok({"proposals": proposals})


# ── POST /{sustain_id}/proposals/{proposal_id}/vote ───────────────────────────

@router.post("/{sustain_id}/proposals/{proposal_id}/vote")
async def vote_on_proposal(
    sustain_id: str,
    proposal_id: str,
    body: VoteBody,
    user: dict = Depends(get_current_user),
    engine: SustainEngine = Depends(get_engine),
):
    """Cast a user vote on a council proposal."""
    _assert_owns(engine, sustain_id, user["user_id"])

    vote = body.vote.strip().upper()
    if vote not in ("YES", "NO"):
        return _err("vote must be 'YES' or 'NO'", status_code=422)

    state_dict = engine.get_state(sustain_id)
    state_accessor = StateAccessor(state_dict)

    # Ensure council_proposals exists
    if not state_accessor.exists("council_proposals"):
        raise HTTPException(status_code=404, detail="Proposal not found")

    session = CouncilSession(sustain_id=sustain_id, state=state_accessor)

    # Check proposal exists
    proposal = session._get_proposal(proposal_id)
    if proposal is None:
        raise HTTPException(status_code=404, detail="Proposal not found")

    try:
        new_status = session.resolve(proposal_id, user_vote=vote)
    except ValueError as exc:
        raise HTTPException(status_code=404, detail=str(exc))

    # Persist updated state (council_proposals mutated in-place)
    engine._persist_state(sustain_id, state_accessor.snapshot())

    updated_proposal = session._get_proposal(proposal_id)
    return _ok({"proposal_id": proposal_id, "status": new_status, "proposal": updated_proposal})


# ── GET /{sustain_id}/export ──────────────────────────────────────────────────

@router.get("/{sustain_id}/export")
async def export_sustain(
    sustain_id: str,
    user: dict = Depends(get_current_user),
    engine: SustainEngine = Depends(get_engine),
):
    """Download full sustain spec + current state as JSON."""
    _assert_owns(engine, sustain_id, user["user_id"])

    row = engine._db.execute(
        "SELECT template_id, created_at FROM sustains WHERE id = ?",
        (sustain_id,),
    ).fetchone()

    spec = engine._get_spec(sustain_id) or {}
    state = engine.get_state(sustain_id)

    payload = json.dumps({
        "sustain_id": sustain_id,
        "template_id": row["template_id"] if row else None,
        "created_at": row["created_at"] if row else None,
        "exported_at": _now_iso(),
        "spec": spec,
        "state": state,
    }, indent=2)

    return Response(
        content=payload,
        media_type="application/json",
        headers={"Content-Disposition": f'attachment; filename="sustain_{sustain_id}.json"'},
    )
