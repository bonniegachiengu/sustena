"""
sustena/core/whatsapp_handler.py

Core WhatsApp message handler.

Receives a normalised inbound message (phone + text) and routes it to the
correct flow. All replies go through WhatsAppSender (real or mock).

Flows:
  1. Onboarding  — new users (name → income → goal → Homestead sustain created)
  2. Active user — existing users routed to Orchie (stubbed in Epic 0.3;
                   real Orchie wired in Epic 1.2)
  3. Support     — unrecognised messages queued for manual review

Session state is held in-memory (dict) for Phase 0.
Phase 1: migrate to SQLite user + session tables (Epic 1.3).
"""

import logging
import uuid
from datetime import datetime
from typing import Any

from sustena.core.whatsapp_sender import get_whatsapp_sender

logger = logging.getLogger(__name__)

# ── In-memory session store ────────────────────────────────────────────────────
# { phone_number: { "state": str, "data": dict, "user_id": str | None } }
# Cleared on server restart. Fine for Phase 0 / solo testing.
_SESSIONS: dict[str, dict] = {}

# ── Support queue ──────────────────────────────────────────────────────────────
# { "id": str, "phone": str, "message": str, "timestamp": str, "addressed": bool }
_SUPPORT_QUEUE: list[dict] = []


class WhatsAppHandler:
    """
    Routes inbound WhatsApp messages to the correct flow.
    Instantiate once and reuse (sender is a singleton).
    """

    def __init__(self) -> None:
        self._sender = get_whatsapp_sender()

    async def handle(self, phone: str, message: str) -> None:
        """
        Main entry point. Called by both the real webhook and /dev/simulate.

        phone   — E.164 format, e.g. "254712345678" (no '+')
        message — raw text body from the user
        """
        message = message.strip()
        if not message:
            return

        session = _SESSIONS.get(phone, {})
        state = session.get("state", "NEW")

        logger.info("INBOUND  phone=%-15s state=%-20s msg=%s", phone, state, message[:60])

        if state in ("NEW", "AWAITING_NAME"):
            await self._onboard_name(phone, message, session)
        elif state == "AWAITING_INCOME":
            await self._onboard_income(phone, message, session)
        elif state == "AWAITING_GOAL":
            await self._onboard_goal(phone, message, session)
        elif state == "COMPLETE":
            await self._active_user(phone, message, session)
        else:
            await self._support(phone, message)

    # ── Onboarding ─────────────────────────────────────────────────────────────

    async def _onboard_name(self, phone: str, message: str, session: dict) -> None:
        """Step 1: first contact → ask for name. Step 2: record name → ask income."""
        if not session or session.get("state") == "NEW":
            # First contact — greet and ask name
            _SESSIONS[phone] = {"state": "AWAITING_NAME", "data": {}}
            await self._sender.send_text(
                phone,
                "Sema! 👋 Mimi ni Orchie — your Sustena assistant.\n\n"
                "Niambie — what's your name?",
            )
        else:
            # They've replied with their name
            name = message.strip()
            if len(name) < 2:
                await self._sender.send_text(phone, "Samahani, sikuona jina. Jaribu tena?")
                return
            _SESSIONS[phone]["data"]["name"] = name
            _SESSIONS[phone]["state"] = "AWAITING_INCOME"
            await self._sender.send_text(
                phone,
                f"Poa, {name}! 🙌\n\n"
                "Niambie — roughly how much do you earn per month (in KES)?\n"
                "Example: 35000",
            )

    async def _onboard_income(self, phone: str, message: str, session: dict) -> None:
        """Step 3: record income → ask main goal."""
        raw = message.replace(",", "").replace(" ", "").strip()
        try:
            income = float(raw)
            if income <= 0:
                raise ValueError("income must be positive")
        except ValueError:
            await self._sender.send_text(
                phone,
                "Sijui hiyo. Tuma number tu — e.g. 35000 or 35,000 ✍️",
            )
            return

        _SESSIONS[phone]["data"]["income"] = income
        _SESSIONS[phone]["state"] = "AWAITING_GOAL"
        await self._sender.send_text(
            phone,
            "Sawa! Swali moja zaidi —\n\n"
            "What's your main financial goal right now?\n\n"
            "Example: 'Save for emergency fund' · 'Pay off my debt' · 'Grow my business'",
        )

    async def _onboard_goal(self, phone: str, message: str, session: dict) -> None:
        """Step 4: record goal → create sustain → welcome to active state."""
        goal = message.strip()
        if len(goal) < 3:
            await self._sender.send_text(phone, "Niambie zaidi — what's your goal? 🎯")
            return

        data = _SESSIONS[phone]["data"]
        data["goal"] = goal
        data["user_id"] = str(uuid.uuid4())   # placeholder until DB write in Epic 1.3
        data["created_at"] = datetime.utcnow().isoformat()
        _SESSIONS[phone]["state"] = "COMPLETE"

        name = data.get("name", "")
        income = data.get("income", 0)

        # TODO (Epic 1.3): persist user + Homestead sustain to SQLite
        logger.info(
            "Onboarding complete: phone=%s name=%s income=%.0f goal=%s user_id=%s",
            phone, name, income, goal, data["user_id"],
        )

        await self._sender.send_text(
            phone,
            f"✅ Tumeanza, {name}!\n\n"
            f"Homestead sustain yako iko tayari:\n"
            f"  💰 Income: KES {income:,.0f}/month\n"
            f"  🎯 Goal: {goal}\n\n"
            "Sasa unaweza kuniambia chochote — budget, savings, chama, biashara.\n"
            "Tuma *help* uone ninachoweza kukusaidia. Niko hapa! 🌱",
        )

    # ── Active user routing ────────────────────────────────────────────────────

    async def _active_user(self, phone: str, message: str, session: dict) -> None:
        """
        Route messages from onboarded users.
        Stubbed in Epic 0.3 — Orchie intelligence wires in at Epic 1.2.
        """
        name = session.get("data", {}).get("name", "")
        msg_lower = message.lower()

        # Keyword routing (stub responses until real Orchie)
        if any(w in msg_lower for w in ["help", "msaada", "what can you do", "commands"]):
            reply = (
                f"Hii ndio naweza kufanya, {name}:\n\n"
                "💰 *Budget* — track income, pockets, spending\n"
                "🏦 *Chama* — contributions, payouts, Trust Scores\n"
                "📦 *Biashara* — business P&L, orders, inventory\n"
                "🌱 *Mkulima* — farm calendar, harvest, market prices\n\n"
                "Niambie chochote! Orchie anasikia. 👂"
            )
        elif any(w in msg_lower for w in ["budget", "bajeti", "spend", "matumizi", "pesa"]):
            reply = (
                f"Budget tracking inakuja, {name}! 💰\n"
                "(Full Orchie intelligence: Epic 1.2 🔧)"
            )
        elif any(w in msg_lower for w in ["chama", "contribution", "mchango", "payout"]):
            reply = (
                f"Chama management inakuja hivi karibuni, {name}! 🏦\n"
                "(Chama Secretary Operative: Epic 1.5b 🔧)"
            )
        elif any(w in msg_lower for w in ["biashara", "business", "order", "sale", "revenue"]):
            reply = (
                f"Biashara intelligence inakuja, {name}! 📦\n"
                "(Vyyb Biashara Sustain: Epic 1.5 🔧)"
            )
        elif any(w in msg_lower for w in ["shamba", "farm", "mkulima", "harvest", "crop"]):
            reply = (
                f"Mkulima operative inakuja, {name}! 🌱\n"
                "(Farm intelligence: Epic 1.13 🔧)"
            )
        else:
            # Unknown — queue for support and reply warmly
            await self._support(phone, message)
            return

        await self._sender.send_text(phone, reply)

    # ── Support queue ──────────────────────────────────────────────────────────

    async def _support(self, phone: str, message: str) -> None:
        """Log unhandled message to support queue and send a holding reply."""
        entry = {
            "id": str(uuid.uuid4()),
            "phone": phone,
            "message": message,
            "timestamp": datetime.utcnow().isoformat(),
            "addressed": False,
        }
        _SUPPORT_QUEUE.append(entry)
        logger.warning("Support queue: phone=%s msg=%s", phone, message[:80])

        name = _SESSIONS.get(phone, {}).get("data", {}).get("name", "")
        greeting = f"{name}, s" if name else "S"
        await self._sender.send_text(
            phone,
            f"{greeting}amahani, sikuelewe vizuri. 🙏\n"
            "Tuma *help* uone ninachoweza kukusaidia.",
        )

    # ── Support queue accessors (used by admin API) ────────────────────────────

    @staticmethod
    def get_support_queue(addressed: bool | None = None) -> list[dict]:
        if addressed is None:
            return list(_SUPPORT_QUEUE)
        return [e for e in _SUPPORT_QUEUE if e["addressed"] == addressed]

    @staticmethod
    def mark_addressed(event_id: str) -> bool:
        for entry in _SUPPORT_QUEUE:
            if entry["id"] == event_id:
                entry["addressed"] = True
                return True
        return False

    @staticmethod
    def get_sessions() -> dict:
        """Return all active sessions — for debug/admin use only."""
        return dict(_SESSIONS)

    @staticmethod
    def reset_session(phone: str) -> None:
        """Clear a user's session — useful for testing re-onboarding."""
        _SESSIONS.pop(phone, None)
