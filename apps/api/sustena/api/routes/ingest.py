"""
sustena/api/routes/ingest.py

The server-side half of the Ingest & capture pipeline (Slice 4) — mounted
at /api/v1/ingest. This is the API a future "dumb" capture client (a phone
app that just forwards raw SMS text, no parsing logic of its own) is meant
to POST to; the transducer and all the intelligence live server-side
(sustena/core/transducer.py, sustena/core/ingest_engine.py). The native
capture app itself is downstream and out of scope for this slice — this
route surface is designed so it can be built against later without change.

Endpoints
---------
POST /capture                        — submit a raw captured message
GET  /messages                       — list recent ingest messages
GET  /messages/{id}                  — one message's full detail
POST /messages/{id}/resolve          — acknowledge a needs_attention item
GET  /sources                        — registered capture sources + staleness
POST /sources                        — register/configure a source

Auth: reuses the real, DB-backed get_current_user (from users.py) — the
same one devui.py and journal.py use, not sustains.py's disconnected-engine
copy. A capture client authenticates the same way any user of the app does
(the existing login flow); a dedicated device-token scheme is real, separate
follow-up work, not built here.
"""

import logging
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field

from sustena.api.routes.users import get_current_user

logger = logging.getLogger(__name__)
router = APIRouter()


def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def _ok(data: Any) -> dict:
    return {"status": "ok", "data": data, "error": None, "timestamp": _now_iso()}


def _assert_owns_sustain(sustain_id: str, user_id: str) -> None:
    """
    Raise 404 if the sustain doesn't exist or isn't owned by this user — same
    404-for-both-cases choice sustains.py's _assert_owns makes, to avoid
    leaking which sustain IDs exist to an unauthorised caller. Uses the real
    shared engine (get_shared_engine), not sustains.py's disconnected
    in-memory one, since ingest must see Bonnie's actual live sustains.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    row = engine._db.execute(
        "SELECT user_id FROM sustains WHERE id = ?", (sustain_id,)
    ).fetchone()
    if row is None or row["user_id"] != user_id:
        raise HTTPException(status_code=404, detail="Sustain not found")


# ── Request bodies ────────────────────────────────────────────────────────────

class CaptureBody(BaseModel):
    source_id: str = Field(..., description="Identifies the capture source, e.g. a phone/device id.")
    sustain_id: str = Field(..., description="Which sustain this source feeds.")
    raw_payload: str = Field(..., description="The raw captured message text, verbatim.")
    captured_at: str | None = Field(None, description="ISO timestamp the client captured this at, if known.")


class ResolveBody(BaseModel):
    note: str | None = None


class RegisterSourceBody(BaseModel):
    source_id: str
    sustain_id: str
    label: str | None = None
    expected_interval_minutes: int | None = Field(
        None, description="How often this source is expected to report. Omit if unknown — "
                           "a source with no configured cadence is never reported stale."
    )


# ── POST /capture ──────────────────────────────────────────────────────────────

@router.post("/capture")
async def capture(body: CaptureBody, current_user: dict = Depends(get_current_user)) -> dict:
    """
    Submit a raw captured message. Idempotent: the exact same (source_id,
    raw_payload) pair captured more than once returns the first capture's
    recorded outcome (is_duplicate: true) and produces no new event — safe
    for a flaky capture client to retry a POST it's not sure landed.
    """
    _assert_owns_sustain(body.sustain_id, current_user["id"])

    from sustena.core.ingest_singleton import get_shared_ingest_engine

    ingest = get_shared_ingest_engine()
    result = await ingest.capture(
        source_id=body.source_id,
        sustain_id=body.sustain_id,
        raw_payload=body.raw_payload,
        captured_at=body.captured_at,
    )
    return _ok(result)


# ── GET /messages ──────────────────────────────────────────────────────────────

@router.get("/messages")
async def list_messages(
    sustain_id: str | None = None,
    status: str | None = None,
    limit: int = 50,
    current_user: dict = Depends(get_current_user),
) -> dict:
    """
    Recent ingest messages — applied, refused, and needs_attention alike,
    newest first. If sustain_id is given, ownership is checked; omitting it
    is only meaningful for an admin-style view and currently returns
    everything (no cross-user filtering is applied server-side beyond the
    per-sustain check) — narrow scope, disclosed rather than silently gated.
    """
    if sustain_id:
        _assert_owns_sustain(sustain_id, current_user["id"])

    from sustena.core.ingest_singleton import get_shared_ingest_engine

    ingest = get_shared_ingest_engine()
    messages = ingest.list_messages(sustain_id=sustain_id, status=status, limit=limit)
    return _ok({"messages": messages})


# ── GET /messages/{id} ─────────────────────────────────────────────────────────

@router.get("/messages/{message_id}")
async def get_message(message_id: str, current_user: dict = Depends(get_current_user)) -> dict:
    from sustena.core.ingest_singleton import get_shared_ingest_engine

    ingest = get_shared_ingest_engine()
    message = ingest.get_message(message_id)
    if message is None:
        raise HTTPException(status_code=404, detail="Message not found")
    _assert_owns_sustain(message["sustain_id"], current_user["id"])
    return _ok(message)


# ── POST /messages/{id}/resolve ────────────────────────────────────────────────

@router.post("/messages/{message_id}/resolve")
async def resolve_message(
    message_id: str, body: ResolveBody, current_user: dict = Depends(get_current_user),
) -> dict:
    """
    Acknowledge a needs_attention item as handled by a human — e.g. they ran
    the right operator manually via the Console. Does not retry or mutate
    sustain state itself; resolving is bookkeeping, not an action.
    """
    from sustena.core.ingest_singleton import get_shared_ingest_engine

    ingest = get_shared_ingest_engine()
    message = ingest.get_message(message_id)
    if message is None:
        raise HTTPException(status_code=404, detail="Message not found")
    _assert_owns_sustain(message["sustain_id"], current_user["id"])

    resolved = ingest.resolve_message(message_id, resolved_by=current_user["id"])
    if not resolved:
        raise HTTPException(
            status_code=409,
            detail="Message is not in needs_attention (already resolved, or was applied/refused directly).",
        )
    return _ok(ingest.get_message(message_id))


# ── GET /sources ───────────────────────────────────────────────────────────────

@router.get("/sources")
async def list_sources(
    sustain_id: str | None = None, current_user: dict = Depends(get_current_user),
) -> dict:
    if sustain_id:
        _assert_owns_sustain(sustain_id, current_user["id"])

    from sustena.core.ingest_singleton import get_shared_ingest_engine

    ingest = get_shared_ingest_engine()
    return _ok({"sources": ingest.get_sources(sustain_id=sustain_id)})


# ── POST /sources ──────────────────────────────────────────────────────────────

@router.post("/sources")
async def register_source(body: RegisterSourceBody, current_user: dict = Depends(get_current_user)) -> dict:
    """Register a new capture source, or update an existing one's label/expected cadence."""
    _assert_owns_sustain(body.sustain_id, current_user["id"])

    from sustena.core.ingest_singleton import get_shared_ingest_engine

    ingest = get_shared_ingest_engine()
    ingest.register_source(
        source_id=body.source_id,
        sustain_id=body.sustain_id,
        label=body.label,
        expected_interval_minutes=body.expected_interval_minutes,
    )
    sources = ingest.get_sources(sustain_id=body.sustain_id)
    match = next((s for s in sources if s["source_id"] == body.source_id), None)
    return _ok(match)
