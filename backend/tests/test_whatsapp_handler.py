"""
tests/test_whatsapp_handler.py

Tests for the WhatsApp message handler and onboarding flow.

Run with: pytest tests/test_whatsapp_handler.py -v
"""

import pytest
from sustena.core.whatsapp_handler import WhatsAppHandler, _SESSIONS, _SUPPORT_QUEUE


def _reset():
    """Clear global state between tests."""
    _SESSIONS.clear()
    _SUPPORT_QUEUE.clear()


@pytest.fixture(autouse=True)
def clean_state():
    _reset()
    yield
    _reset()


PHONE = "254712345678"


class TestOnboardingFlow:

    @pytest.mark.asyncio
    async def test_first_message_triggers_greeting(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        session = _SESSIONS.get(PHONE, {})
        assert session.get("state") == "AWAITING_NAME"

    @pytest.mark.asyncio
    async def test_name_advances_to_income(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")          # greeting
        await handler.handle(PHONE, "Bonnie")         # name
        session = _SESSIONS[PHONE]
        assert session["state"] == "AWAITING_INCOME"
        assert session["data"]["name"] == "Bonnie"

    @pytest.mark.asyncio
    async def test_income_advances_to_goal(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        await handler.handle(PHONE, "Bonnie")
        await handler.handle(PHONE, "45000")
        session = _SESSIONS[PHONE]
        assert session["state"] == "AWAITING_GOAL"
        assert session["data"]["income"] == 45000.0

    @pytest.mark.asyncio
    async def test_income_with_commas_parsed_correctly(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        await handler.handle(PHONE, "Bonnie")
        await handler.handle(PHONE, "45,000")
        assert _SESSIONS[PHONE]["data"]["income"] == 45000.0

    @pytest.mark.asyncio
    async def test_invalid_income_stays_on_income_step(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        await handler.handle(PHONE, "Bonnie")
        await handler.handle(PHONE, "not a number")
        assert _SESSIONS[PHONE]["state"] == "AWAITING_INCOME"

    @pytest.mark.asyncio
    async def test_goal_completes_onboarding(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        await handler.handle(PHONE, "Bonnie")
        await handler.handle(PHONE, "45000")
        await handler.handle(PHONE, "Save for a house")
        session = _SESSIONS[PHONE]
        assert session["state"] == "COMPLETE"
        assert session["data"]["goal"] == "Save for a house"
        assert "user_id" in session["data"]

    @pytest.mark.asyncio
    async def test_full_onboarding_creates_user_id(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        await handler.handle(PHONE, "Bonnie")
        await handler.handle(PHONE, "45000")
        await handler.handle(PHONE, "Build Sustena")
        user_id = _SESSIONS[PHONE]["data"].get("user_id")
        assert user_id is not None
        assert len(user_id) == 36  # UUID format


class TestActiveUserRouting:

    async def _complete_onboarding(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        await handler.handle(PHONE, "Bonnie")
        await handler.handle(PHONE, "45000")
        await handler.handle(PHONE, "Build Sustena")
        return handler

    @pytest.mark.asyncio
    async def test_help_message_handled(self):
        handler = await self._complete_onboarding()
        # Should not raise and should not add to support queue
        await handler.handle(PHONE, "help")
        assert len(_SUPPORT_QUEUE) == 0

    @pytest.mark.asyncio
    async def test_budget_keyword_handled(self):
        handler = await self._complete_onboarding()
        await handler.handle(PHONE, "what is my budget?")
        assert len(_SUPPORT_QUEUE) == 0

    @pytest.mark.asyncio
    async def test_chama_keyword_handled(self):
        handler = await self._complete_onboarding()
        await handler.handle(PHONE, "show my chama")
        assert len(_SUPPORT_QUEUE) == 0

    @pytest.mark.asyncio
    async def test_unknown_message_goes_to_support_queue(self):
        handler = await self._complete_onboarding()
        await handler.handle(PHONE, "xyzzy frobulator quux")
        assert len(_SUPPORT_QUEUE) == 1
        assert _SUPPORT_QUEUE[0]["phone"] == PHONE
        assert _SUPPORT_QUEUE[0]["addressed"] is False


class TestSupportQueue:

    @pytest.mark.asyncio
    async def test_mark_addressed(self):
        handler = WhatsAppHandler()
        # Trigger a support queue entry (new user sends unhandled message after onboarding)
        _SESSIONS[PHONE] = {
            "state": "COMPLETE",
            "data": {"name": "Test", "income": 1000, "goal": "test"},
        }
        await handler.handle(PHONE, "xyzzy frobulator")
        assert len(_SUPPORT_QUEUE) == 1
        event_id = _SUPPORT_QUEUE[0]["id"]
        ok = WhatsAppHandler.mark_addressed(event_id)
        assert ok is True
        assert _SUPPORT_QUEUE[0]["addressed"] is True

    def test_mark_addressed_nonexistent_returns_false(self):
        ok = WhatsAppHandler.mark_addressed("nonexistent-id")
        assert ok is False

    @pytest.mark.asyncio
    async def test_get_support_queue_filtered(self):
        handler = WhatsAppHandler()
        _SESSIONS[PHONE] = {
            "state": "COMPLETE",
            "data": {"name": "Test", "income": 1000, "goal": "test"},
        }
        await handler.handle(PHONE, "unknown message 1")
        await handler.handle(PHONE, "unknown message 2")
        # Mark one addressed
        WhatsAppHandler.mark_addressed(_SUPPORT_QUEUE[0]["id"])
        unaddressed = WhatsAppHandler.get_support_queue(addressed=False)
        addressed = WhatsAppHandler.get_support_queue(addressed=True)
        assert len(unaddressed) == 1
        assert len(addressed) == 1


class TestSessionReset:

    @pytest.mark.asyncio
    async def test_reset_allows_re_onboarding(self):
        handler = WhatsAppHandler()
        await handler.handle(PHONE, "hello")
        await handler.handle(PHONE, "Bonnie")
        assert _SESSIONS[PHONE]["state"] == "AWAITING_INCOME"

        WhatsAppHandler.reset_session(PHONE)
        assert PHONE not in _SESSIONS

        # After reset, starts fresh
        await handler.handle(PHONE, "hello")
        assert _SESSIONS[PHONE]["state"] == "AWAITING_NAME"
