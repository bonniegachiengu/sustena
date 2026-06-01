"""
tests/test_api_operators.py

Tests for the api.* operator library.
Covers: api.get, api.post, api.webhook_listen — mock HTTP, state mutations, events.
"""

import pytest
from datetime import datetime
from unittest.mock import AsyncMock, MagicMock, patch

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _fresh_state() -> StateAccessor:
    return StateAccessor({"system": {}})


def _make_ctx(state: StateAccessor | None = None) -> OperatorContext:
    state = state or _fresh_state()
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="user-test",
        timestamp=datetime.utcnow(),
    )


# ── api.get ────────────────────────────────────────────────────────────────────

class TestApiGet:

    @pytest.mark.asyncio
    async def test_api_get_success(self):
        mock_response = MagicMock()
        mock_response.status_code = 200
        mock_response.text = '{"hello": "world"}'
        mock_response.headers = {"content-type": "application/json"}

        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=None)
        mock_client.get = AsyncMock(return_value=mock_response)

        with patch("sustena.operators.api_ops.httpx.AsyncClient", return_value=mock_client):
            ctx = _make_ctx()
            result = await OPERATOR_REGISTRY["api.get"].fn(
                ctx, url="http://example.com/data"
            )

        assert result.succeeded
        assert result.data["status_code"] == 200
        assert result.data["body"] == '{"hello": "world"}'

    @pytest.mark.asyncio
    async def test_api_get_timeout(self):
        import httpx
        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=None)
        mock_client.get = AsyncMock(side_effect=httpx.TimeoutException("timeout"))

        with patch("sustena.operators.api_ops.httpx.AsyncClient", return_value=mock_client):
            ctx = _make_ctx()
            result = await OPERATOR_REGISTRY["api.get"].fn(
                ctx, url="http://slow.example.com", timeout_s=1
            )

        assert result.failed
        assert "timed out" in result.reason

    @pytest.mark.asyncio
    async def test_api_get_request_error(self):
        import httpx
        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=None)
        mock_client.get = AsyncMock(
            side_effect=httpx.RequestError("connection refused")
        )

        with patch("sustena.operators.api_ops.httpx.AsyncClient", return_value=mock_client):
            ctx = _make_ctx()
            result = await OPERATOR_REGISTRY["api.get"].fn(
                ctx, url="http://dead.host.local"
            )

        assert result.failed
        assert "failed" in result.reason


# ── api.post ───────────────────────────────────────────────────────────────────

class TestApiPost:

    @pytest.mark.asyncio
    async def test_api_post_success(self):
        mock_response = MagicMock()
        mock_response.status_code = 201
        mock_response.text = '{"created": true}'
        mock_response.headers = {}

        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=None)
        mock_client.post = AsyncMock(return_value=mock_response)

        with patch("sustena.operators.api_ops.httpx.AsyncClient", return_value=mock_client):
            ctx = _make_ctx()
            result = await OPERATOR_REGISTRY["api.post"].fn(
                ctx,
                url="http://api.example.com/items",
                body={"name": "cooking oil", "qty": 2},
            )

        assert result.succeeded
        assert result.data["status_code"] == 201

    @pytest.mark.asyncio
    async def test_api_post_timeout(self):
        import httpx
        mock_client = AsyncMock()
        mock_client.__aenter__ = AsyncMock(return_value=mock_client)
        mock_client.__aexit__ = AsyncMock(return_value=None)
        mock_client.post = AsyncMock(side_effect=httpx.TimeoutException("timeout"))

        with patch("sustena.operators.api_ops.httpx.AsyncClient", return_value=mock_client):
            ctx = _make_ctx()
            result = await OPERATOR_REGISTRY["api.post"].fn(
                ctx, url="http://slow.api.com/order", body={}
            )

        assert result.failed
        assert "timed out" in result.reason


# ── api.webhook_listen ─────────────────────────────────────────────────────────

class TestApiWebhookListen:

    @pytest.mark.asyncio
    async def test_webhook_registers_in_state(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["api.webhook_listen"].fn(
            ctx,
            event_type="event.mpesa.payment_received",
            endpoint_path="/webhook/mpesa",
        )

        assert result.succeeded
        assert result.data["registered"] is True
        listeners = ctx.state.get("webhooks.listeners")
        assert listeners is not None
        assert len(listeners) == 1
        assert listeners[0]["event_type"] == "event.mpesa.payment_received"
        assert listeners[0]["endpoint_path"] == "/webhook/mpesa"

    @pytest.mark.asyncio
    async def test_webhook_publishes_event(self):
        ctx = _make_ctx()
        await OPERATOR_REGISTRY["api.webhook_listen"].fn(
            ctx, event_type="event.delivery.arrived"
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.api.webhook_registered" for e in events)

    @pytest.mark.asyncio
    async def test_webhook_multiple_listeners_accumulate(self):
        ctx = _make_ctx()
        await OPERATOR_REGISTRY["api.webhook_listen"].fn(
            ctx, event_type="event.a", endpoint_path="/a"
        )
        await OPERATOR_REGISTRY["api.webhook_listen"].fn(
            ctx, event_type="event.b", endpoint_path="/b"
        )
        listeners = ctx.state.get("webhooks.listeners")
        assert len(listeners) == 2


# ── Registry metadata ──────────────────────────────────────────────────────────

class TestApiOperatorMetadata:

    @pytest.mark.parametrize("name", ["api.get", "api.post", "api.webhook_listen"])
    def test_operator_in_registry(self, name):
        assert name in OPERATOR_REGISTRY

    def test_api_get_protocol(self):
        assert OPERATOR_REGISTRY["api.get"].protocol == "rpc"

    def test_api_post_protocol(self):
        assert OPERATOR_REGISTRY["api.post"].protocol == "rpc"

    def test_api_webhook_listen_protocol(self):
        assert OPERATOR_REGISTRY["api.webhook_listen"].protocol == "event_driven"
