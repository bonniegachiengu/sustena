"""
sustena/core/whatsapp_sender.py

Sends messages via the Meta WhatsApp Business API.

In development / when WhatsApp credentials are not configured,
MockWhatsAppSender is used — all messages are printed to the console
and written to `whatsapp_mock_outbox.jsonl` in the project root.
This lets you see exactly what would be sent without any Meta account.

Switch to real sending by setting WHATSAPP_TOKEN and WHATSAPP_PHONE_ID.

Usage:
    from sustena.core.whatsapp_sender import get_whatsapp_sender
    sender = get_whatsapp_sender()
    await sender.send_text(to="+254712345678", message="Habari!")
"""

import json
import logging
from datetime import datetime
from pathlib import Path
from typing import Any

import httpx

from sustena.config import settings

logger = logging.getLogger(__name__)

MOCK_OUTBOX_PATH = Path("whatsapp_mock_outbox.jsonl")
META_API_BASE = "https://graph.facebook.com/v20.0"


# ── Mock implementation ────────────────────────────────────────────────────────


class MockWhatsAppSender:
    """
    Logs all outbound messages to console and to whatsapp_mock_outbox.jsonl.
    No Meta account, no network calls, no credentials needed.
    """

    def __init__(self) -> None:
        logger.info(
            "MockWhatsAppSender active — messages logged to console + %s",
            MOCK_OUTBOX_PATH,
        )

    def _log(self, to: str, message_type: str, payload: dict) -> bool:
        entry = {
            "timestamp": datetime.utcnow().isoformat(),
            "to": to,
            "type": message_type,
            "payload": payload,
        }
        print(f"\n📱 [WHATSAPP → {to}] {message_type}:\n{json.dumps(payload, indent=2, ensure_ascii=False)}\n")
        with open(MOCK_OUTBOX_PATH, "a", encoding="utf-8") as f:
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
        return True

    async def send_text(self, to: str, message: str) -> bool:
        return self._log(to, "text", {"body": message})

    async def send_interactive_buttons(
        self, to: str, body: str, buttons: list[dict]
    ) -> bool:
        """Buttons: list of {id, title} dicts — max 3."""
        return self._log(to, "interactive_buttons", {"body": body, "buttons": buttons[:3]})

    async def send_list_message(
        self,
        to: str,
        header: str,
        body: str,
        footer: str,
        sections: list[dict],
    ) -> bool:
        return self._log(to, "list_message", {
            "header": header, "body": body, "footer": footer, "sections": sections
        })

    async def send_template(self, to: str, template_name: str, params: list[str]) -> bool:
        return self._log(to, "template", {"template": template_name, "params": params})


# ── Real implementation ────────────────────────────────────────────────────────


class WhatsAppSender:
    """
    Real Meta WhatsApp Business API sender.
    Requires WHATSAPP_TOKEN and WHATSAPP_PHONE_ID in config.
    """

    MAX_RETRIES = 3

    def __init__(self, token: str, phone_id: str) -> None:
        self._token = token
        self._phone_id = phone_id
        self._base_url = f"{META_API_BASE}/{phone_id}/messages"
        logger.info("WhatsAppSender active — phone_id=%s", phone_id)

    async def _post(self, payload: dict) -> bool:
        headers = {
            "Authorization": f"Bearer {self._token}",
            "Content-Type": "application/json",
        }
        for attempt in range(1, self.MAX_RETRIES + 1):
            try:
                async with httpx.AsyncClient(timeout=10.0) as client:
                    resp = await client.post(self._base_url, json=payload, headers=headers)
                    resp.raise_for_status()
                    return True
            except httpx.HTTPStatusError as e:
                logger.warning("WhatsApp send attempt %d failed: %s", attempt, e)
            except Exception as e:
                logger.error("WhatsApp send error: %s", e)
        return False

    async def send_text(self, to: str, message: str) -> bool:
        return await self._post({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "text",
            "text": {"body": message},
        })

    async def send_interactive_buttons(self, to: str, body: str, buttons: list[dict]) -> bool:
        return await self._post({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "interactive",
            "interactive": {
                "type": "button",
                "body": {"text": body},
                "action": {
                    "buttons": [
                        {"type": "reply", "reply": {"id": b["id"], "title": b["title"]}}
                        for b in buttons[:3]
                    ]
                },
            },
        })

    async def send_list_message(self, to, header, body, footer, sections) -> bool:
        return await self._post({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "interactive",
            "interactive": {
                "type": "list",
                "header": {"type": "text", "text": header},
                "body": {"text": body},
                "footer": {"text": footer},
                "action": {"button": "View options", "sections": sections},
            },
        })

    async def send_template(self, to: str, template_name: str, params: list[str]) -> bool:
        return await self._post({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "template",
            "template": {
                "name": template_name,
                "language": {"code": "en"},
                "components": [
                    {
                        "type": "body",
                        "parameters": [{"type": "text", "text": p} for p in params],
                    }
                ],
            },
        })


# ── Factory ───────────────────────────────────────────────────────────────────

_instance: Any = None


def get_whatsapp_sender() -> MockWhatsAppSender | WhatsAppSender:
    """
    Returns MockWhatsAppSender when credentials are absent or in development.
    Returns real WhatsAppSender in production with valid credentials.
    """
    global _instance
    if _instance is not None:
        return _instance

    token = settings.whatsapp_token
    phone_id = settings.whatsapp_phone_id
    is_mock = (
        settings.is_development
        or not token
        or token in ("mock", "placeholder", "")
        or not phone_id
        or phone_id in ("mock", "placeholder", "")
    )

    _instance = MockWhatsAppSender() if is_mock else WhatsAppSender(token=token, phone_id=phone_id)
    return _instance
