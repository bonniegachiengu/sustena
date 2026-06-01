"""
sustena/api/routes/orchie.py

Orchie chat endpoint — direct conversational interface to the Sustena AI operative.

POST /orchie/message
    Body:     { sustain_id: str, message: str, context?: dict }
    Response: { reply: str, proposals: list, actions: list, tone: str }

Routes through the same claude_client factory used everywhere else in the stack,
so mock mode works out of the box in development.
"""

import logging
from typing import Any

from fastapi import APIRouter
from pydantic import BaseModel

from sustena.core.claude_client import get_claude_client

logger = logging.getLogger(__name__)

router = APIRouter()

# ── Request / Response schemas ────────────────────────────────────────────────


class OrchieMessageRequest(BaseModel):
    sustain_id: str
    message: str
    context: dict[str, Any] = {}


class OrchieMessageResponse(BaseModel):
    reply: str
    proposals: list = []
    actions: list = []
    tone: str = "neutral"


# ── System prompt ─────────────────────────────────────────────────────────────

ORCHIE_SYSTEM = """You are Orchie, the conversational AI operative for Sustena — a human-agent reality interface built around 7 primitives (state, operators, operatives, council, events, constraints, pawa).

Your role:
- Help the user understand and manage their sustain (their described system — household, business, community, etc.)
- Surface relevant insights: cash position, burn rate, pending council proposals, pantry alerts
- Propose concrete actions when thresholds are breached
- Be concise and direct — one or two sentences unless detail is asked for
- Use Swahili greetings when contextually appropriate (this is a Kenyan product)
- Amounts are in KSH unless otherwise specified
- Never use markdown formatting — respond in plain conversational text

You are NOT a generic assistant. Stay grounded in the user's sustain data and Sustena's operational model."""


# ── Route ─────────────────────────────────────────────────────────────────────


@router.post("/message", response_model=OrchieMessageResponse)
async def orchie_message(req: OrchieMessageRequest) -> OrchieMessageResponse:
    """
    Send a message to Orchie and receive a reply.

    Uses the claude_client factory — mock in development, real Anthropic API in production.
    """
    client = get_claude_client()

    system = f"{ORCHIE_SYSTEM}\n\nActive sustain: {req.sustain_id}"
    if req.context:
        import json
        system += f"\n\nContext snapshot:\n{json.dumps(req.context, indent=2)}"

    try:
        response = await client.messages.create(
            model="claude-haiku-4-5-20251001",
            max_tokens=512,
            system=system,
            messages=[{"role": "user", "content": req.message}],
        )
        reply_text = response.content[0].text if response.content else "Noted."
    except Exception as exc:
        logger.warning("Orchie LLM call failed: %s", exc)
        reply_text = "I hit a snag on my end — try again in a moment."

    logger.info(
        "orchie_message sustain=%s message=%r reply=%r",
        req.sustain_id,
        req.message[:60],
        reply_text[:60],
    )

    return OrchieMessageResponse(reply=reply_text)
