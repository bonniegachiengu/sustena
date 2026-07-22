"""
sustena/api/routes/dev.py

Development-only endpoints. ONLY mounted when ENVIRONMENT=development.
These endpoints are NEVER available in production. Every route also requires
a real user session (get_current_user) — the is_development mount gate alone
isn't route-level auth, and this router exposes session/support-queue data
plus mutating endpoints (reset session, mark addressed).

Endpoints:
  POST /dev/simulate        — Simulate an inbound WhatsApp message
  GET  /dev/sessions        — Inspect all active user sessions
  DELETE /dev/sessions/{phone} — Reset a user's session (re-test onboarding)
  GET  /dev/outbox          — Read whatsapp_mock_outbox.jsonl
  GET  /dev/support-queue   — View support queue
  PATCH /dev/support-queue/{event_id}/addressed
"""

import json
import logging
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, ConfigDict

from sustena.api.routes.users import get_current_user
from sustena.core.whatsapp_handler import WhatsAppHandler

router = APIRouter()
logger = logging.getLogger(__name__)

_handler = WhatsAppHandler()

MOCK_OUTBOX_PATH = Path("whatsapp_mock_outbox.jsonl")


# ── Request / response models ─────────────────────────────────────────────────

class SimulateRequest(BaseModel):
    phone: str = "+254712345678"
    message: str

    model_config = ConfigDict(
        json_schema_extra={
            "example": {
                "phone": "+254712345678",
                "message": "hello",
            }
        }
    )


# ── Endpoints ─────────────────────────────────────────────────────────────────

@router.post("/simulate", summary="Simulate an inbound WhatsApp message")
async def simulate_message(body: SimulateRequest, _: dict = Depends(get_current_user)) -> dict:
    """
    Send a test message as if it came from a WhatsApp user.

    The handler processes it identically to a real inbound message.
    Orchie's reply is:
      1. Printed to the server console (look at your uvicorn terminal)
      2. Written to whatsapp_mock_outbox.jsonl in the backend/ folder

    Use this to test the full onboarding flow:
      1. POST {"phone": "+254712345678", "message": "hello"}          ← greeting
      2. POST {"phone": "+254712345678", "message": "Bonnie"}         ← name
      3. POST {"phone": "+254712345678", "message": "45000"}          ← income
      4. POST {"phone": "+254712345678", "message": "Save for a house"} ← goal
    """
    # Strip the + prefix if present — handler uses E.164 without +
    phone = body.phone.lstrip("+")
    await _handler.handle(phone, body.message)
    return {
        "status": "dispatched",
        "phone": phone,
        "message": body.message,
        "note": "Check your terminal console and whatsapp_mock_outbox.jsonl for the reply.",
    }


@router.get("/sessions", summary="Inspect all active user sessions")
async def get_sessions(_: dict = Depends(get_current_user)) -> dict:
    """View all in-memory sessions (onboarding state + user data)."""
    return {"sessions": WhatsAppHandler.get_sessions()}


@router.delete("/sessions/{phone}", summary="Reset a user session")
async def reset_session(phone: str, _: dict = Depends(get_current_user)) -> dict:
    """
    Clear a user's session so you can re-test the onboarding flow.
    Strips leading '+' automatically.
    """
    WhatsAppHandler.reset_session(phone.lstrip("+"))
    return {"status": "session cleared", "phone": phone}


@router.get("/outbox", summary="Read mock outbox (sent messages)")
async def get_outbox(limit: int = 20, _: dict = Depends(get_current_user)) -> dict:
    """
    Read the last N messages from whatsapp_mock_outbox.jsonl.
    These are the replies Orchie sent (or would have sent) to users.
    """
    if not MOCK_OUTBOX_PATH.exists():
        return {"messages": [], "note": "No messages sent yet — try /dev/simulate first."}

    lines = MOCK_OUTBOX_PATH.read_text(encoding="utf-8").strip().splitlines()
    recent = lines[-limit:] if len(lines) > limit else lines
    messages = []
    for line in recent:
        try:
            messages.append(json.loads(line))
        except json.JSONDecodeError:
            pass

    return {"count": len(messages), "messages": messages}


@router.get("/support-queue", summary="View support queue")
async def get_support_queue(addressed: bool | None = None, _: dict = Depends(get_current_user)) -> dict:
    """View messages that couldn't be handled — queued for manual review."""
    items = WhatsAppHandler.get_support_queue(addressed=addressed)
    return {"count": len(items), "items": items}


@router.patch("/support-queue/{event_id}/addressed", summary="Mark support item as addressed")
async def mark_addressed(event_id: str, _: dict = Depends(get_current_user)) -> dict:
    """Mark a support queue item as addressed."""
    ok = WhatsAppHandler.mark_addressed(event_id)
    if not ok:
        raise HTTPException(status_code=404, detail=f"Event {event_id} not found")
    return {"status": "addressed", "event_id": event_id}
