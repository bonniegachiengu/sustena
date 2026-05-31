"""
sustena/core/claude_client.py

Gets you a Claude client appropriate for the current environment.

  - ENVIRONMENT=development + ANTHROPIC_API_KEY=mock  → MockClaudeClient
    Returns hardcoded responses. Zero API calls. Zero spend. Full stack runs locally.

  - ENVIRONMENT=production (or real API key present) → real AsyncAnthropic client

Usage anywhere in the codebase:
    from sustena.core.claude_client import get_claude_client
    client = get_claude_client()
    response = await client.messages.create(...)

The interface is identical either way — swap to production by setting a real key.
"""

import logging
from dataclasses import dataclass, field
from typing import Any

from sustena.config import settings

logger = logging.getLogger(__name__)

# ── Mock response shapes ───────────────────────────────────────────────────────


@dataclass
class MockContentBlock:
    text: str
    type: str = "text"


@dataclass
class MockUsage:
    input_tokens: int = 10
    output_tokens: int = 20


@dataclass
class MockMessage:
    content: list[MockContentBlock]
    model: str = "mock-haiku"
    stop_reason: str = "end_turn"
    usage: MockUsage = field(default_factory=MockUsage)

    @property
    def text(self) -> str:
        """Convenience accessor matching real Anthropic response pattern."""
        return self.content[0].text if self.content else ""


# ── Mock responses keyed by common prompt patterns ────────────────────────────

MOCK_RESPONSES: dict[str, str] = {
    # M-Pesa transaction parsing
    "mpesa": '{"type": "received", "amount": 1000, "from": "JOHN DOE", "ref": "QJH4X8Y9Z2", "balance": 4500, "timestamp": "2026-05-28 09:14:00"}',

    # Budget classification
    "classify": '{"category": "food", "confidence": 0.92, "pocket": "food"}',

    # Orchie greeting / onboarding
    "hello": "Sema! Mimi ni Orchie, your Sustena assistant. Niambie — what's your name?",
    "name": "Poa! Niambie, roughly how much do you earn per month (in KES)?",
    "income": "Sawa. Last question — what's your main financial goal right now?",

    # Budget summary
    "summary": "Your budget this month: KES 4,200 liquid. Food pocket at 60%, rent fully covered. You're on track.",

    # Operative deliberation (Council vote)
    "deliberate": '{"vote": "YES", "reasoning": "Proposal improves budget position. Constraint check: liquid balance remains above 10% floor after execution."}',

    # Generic fallback
    "default": "Noted. Let me check that for you.",
}


def _match_mock_response(prompt: str) -> str:
    """Find the best mock response for a given prompt string."""
    prompt_lower = prompt.lower()
    for keyword, response in MOCK_RESPONSES.items():
        if keyword in prompt_lower:
            return response
    return MOCK_RESPONSES["default"]


# ── Mock client ────────────────────────────────────────────────────────────────


class MockMessages:
    """Mimics the anthropic.AsyncAnthropic().messages interface."""

    async def create(
        self,
        model: str,
        max_tokens: int,
        messages: list[dict],
        system: str | None = None,
        **kwargs: Any,
    ) -> MockMessage:
        # Pull the last user message to determine the mock response
        last_user = next(
            (m["content"] for m in reversed(messages) if m["role"] == "user"),
            "",
        )
        context = f"{system or ''} {last_user}".strip()
        response_text = _match_mock_response(context)

        logger.debug(
            "[MOCK CLAUDE] model=%s max_tokens=%d → %s...",
            model,
            max_tokens,
            response_text[:60],
        )

        return MockMessage(content=[MockContentBlock(text=response_text)])


class MockClaudeClient:
    """
    Drop-in replacement for anthropic.AsyncAnthropic.
    Returns deterministic mock responses — no network calls, no spend.
    """

    def __init__(self) -> None:
        self.messages = MockMessages()
        logger.info(
            "MockClaudeClient active — all LLM calls return hardcoded responses. "
            "Set ANTHROPIC_API_KEY to a real key to use the live API."
        )


# ── Factory ───────────────────────────────────────────────────────────────────


def get_claude_client() -> Any:
    """
    Returns the correct Claude client for the current environment.

    Mock conditions (both must be true):
      1. ENVIRONMENT=development
      2. ANTHROPIC_API_KEY is missing, 'mock', or starts with 'sk-ant-mock'

    Otherwise returns a real AsyncAnthropic client.
    """
    api_key = settings.anthropic_api_key
    is_mock_key = (
        not api_key
        or api_key.strip().lower() in ("mock", "placeholder", "")
        or api_key.startswith("sk-ant-mock")
    )

    if settings.is_development and is_mock_key:
        return MockClaudeClient()

    # Real client
    from anthropic import AsyncAnthropic
    return AsyncAnthropic(api_key=api_key)
