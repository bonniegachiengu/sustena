"""
sustena/api/routes/whatsapp.py

Meta WhatsApp Business API webhook endpoints.

GET  /webhook  — One-time verification handshake required by Meta before
                 they'll send messages to this endpoint.
POST /webhook  — Receives all inbound WhatsApp messages. Verifies the
                 X-Hub-Signature-256 header, parses Meta's payload format,
                 and routes to WhatsAppHandler.

Meta webhook payload structure (simplified):
{
  "object": "whatsapp_business_account",
  "entry": [{
    "changes": [{
      "value": {
        "messages": [{
          "from": "254712345678",
          "type": "text",
          "text": {"body": "hello"}
        }]
      }
    }]
  }]
}
"""

import hashlib
import hmac
import logging

from fastapi import APIRouter, HTTPException, Request, Response

from sustena.config import settings
from sustena.core.whatsapp_handler import WhatsAppHandler

router = APIRouter()
logger = logging.getLogger(__name__)

_handler = WhatsAppHandler()


# ── GET /webhook — Meta verification handshake ────────────────────────────────

@router.get("")
async def webhook_verify(request: Request) -> Response:
    """
    Meta calls this once when you register the webhook URL in their dashboard.
    It sends hub.verify_token and expects hub.challenge back if the token matches.
    """
    params = dict(request.query_params)
    mode      = params.get("hub.mode")
    token     = params.get("hub.verify_token")
    challenge = params.get("hub.challenge")

    if mode == "subscribe" and token == settings.whatsapp_verify_token:
        logger.info("Webhook verified successfully.")
        return Response(content=challenge, media_type="text/plain")

    logger.warning("Webhook verification failed. mode=%s token=%s", mode, token)
    raise HTTPException(status_code=403, detail="Verification failed")


# ── POST /webhook — Receive inbound messages ──────────────────────────────────

@router.post("")
async def webhook_receive(request: Request) -> dict:
    """
    Receives all inbound WhatsApp events from Meta.
    - Verifies the HMAC-SHA256 signature (skipped in development).
    - Extracts text messages and routes to WhatsAppHandler.
    - Always returns 200 immediately — Meta expects a fast ACK.
    """
    body = await request.body()

    # Signature verification (production only)
    if settings.is_production:
        signature = request.headers.get("X-Hub-Signature-256", "")
        _verify_signature(body, signature)

    try:
        payload = await request.json()
    except Exception:
        logger.warning("Could not parse webhook payload.")
        return {"status": "ok"}

    # Extract messages from Meta's nested payload structure
    entries = payload.get("entry", [])
    for entry in entries:
        for change in entry.get("changes", []):
            value = change.get("value", {})
            messages = value.get("messages", [])
            for msg in messages:
                phone = msg.get("from", "")
                msg_type = msg.get("type", "")
                if msg_type == "text":
                    body_text = msg.get("text", {}).get("body", "")
                    if phone and body_text:
                        # Fire and forget — always return 200 to Meta quickly
                        import asyncio
                        asyncio.create_task(_handler.handle(phone, body_text))

    return {"status": "ok"}


# ── Signature verification helper ─────────────────────────────────────────────

def _verify_signature(body: bytes, signature: str) -> None:
    """
    Verify the X-Hub-Signature-256 header.
    Meta computes HMAC-SHA256 of the raw body using the app secret.
    Raises HTTPException(401) if invalid.
    """
    if not signature.startswith("sha256="):
        raise HTTPException(status_code=401, detail="Missing signature")

    expected = hmac.new(
        settings.whatsapp_token.encode(),
        body,
        hashlib.sha256,
    ).hexdigest()

    if not hmac.compare_digest(f"sha256={expected}", signature):
        raise HTTPException(status_code=401, detail="Invalid signature")
