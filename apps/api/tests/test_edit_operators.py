"""
tests/test_edit_operators.py

Tests for the edit.* operator library.
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _make_ctx(state_data: dict | None = None) -> OperatorContext:
    return OperatorContext(
        state=StateAccessor(state_data or {"system": {}, "config": {}}),
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="user-test",
        timestamp=datetime.utcnow(),
    )


# ── edit.state_patch ───────────────────────────────────────────────────────────

class TestEditStatePatch:

    @pytest.mark.asyncio
    async def test_replace_scalar_value(self):
        ctx = _make_ctx({"settings": {"theme": "light"}})
        result = await OPERATOR_REGISTRY["edit.state_patch"].fn(
            ctx,
            patch=[{"op": "replace", "path": "settings.theme", "value": "dark"}],
        )
        assert result.succeeded
        assert ctx.state.get("settings.theme") == "dark"

    @pytest.mark.asyncio
    async def test_add_new_key(self):
        ctx = _make_ctx({"config": {}})
        result = await OPERATOR_REGISTRY["edit.state_patch"].fn(
            ctx,
            patch=[{"op": "add", "path": "config.language", "value": "sw"}],
        )
        assert result.succeeded
        assert ctx.state.get("config.language") == "sw"

    @pytest.mark.asyncio
    async def test_remove_sets_to_none(self):
        ctx = _make_ctx({"settings": {"debug": True}})
        result = await OPERATOR_REGISTRY["edit.state_patch"].fn(
            ctx,
            patch=[{"op": "remove", "path": "settings.debug"}],
        )
        assert result.succeeded
        assert ctx.state.get("settings.debug") is None

    @pytest.mark.asyncio
    async def test_multiple_operations(self):
        ctx = _make_ctx({"a": {"x": 1}, "b": {"y": 2}})
        result = await OPERATOR_REGISTRY["edit.state_patch"].fn(
            ctx,
            patch=[
                {"op": "replace", "path": "a.x", "value": 10},
                {"op": "replace", "path": "b.y", "value": 20},
            ],
        )
        assert result.succeeded
        assert result.data["applied_count"] == 2
        assert ctx.state.get("a.x") == 10
        assert ctx.state.get("b.y") == 20

    @pytest.mark.asyncio
    async def test_empty_patch_returns_fail(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["edit.state_patch"].fn(ctx, patch=[])
        assert result.failed

    @pytest.mark.asyncio
    async def test_unknown_op_recorded_as_error(self):
        ctx = _make_ctx({"x": 1})
        result = await OPERATOR_REGISTRY["edit.state_patch"].fn(
            ctx,
            patch=[{"op": "explode", "path": "x", "value": 999}],
        )
        # The result may succeed (other ops applied) or fail (all ops failed)
        # but the error should be recorded
        assert result.data.get("error_count", 0) > 0 or result.failed

    @pytest.mark.asyncio
    async def test_publishes_event(self):
        ctx = _make_ctx({"settings": {"x": 0}})
        await OPERATOR_REGISTRY["edit.state_patch"].fn(
            ctx,
            patch=[{"op": "replace", "path": "settings.x", "value": 1}],
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.edit.state_patched" for e in events)


# ── edit.operator_spec ─────────────────────────────────────────────────────────

class TestEditOperatorSpec:

    @pytest.mark.asyncio
    async def test_update_description(self):
        ctx = _make_ctx()
        original = OPERATOR_REGISTRY["budget.summary"].description
        try:
            result = await OPERATOR_REGISTRY["edit.operator_spec"].fn(
                ctx,
                operator_name="budget.summary",
                field="description",
                value="Updated description for testing",
            )
            assert result.succeeded
            assert OPERATOR_REGISTRY["budget.summary"].description == "Updated description for testing"
        finally:
            # Restore original
            OPERATOR_REGISTRY["budget.summary"].description = original

    @pytest.mark.asyncio
    async def test_update_pawa_cost(self):
        ctx = _make_ctx()
        original = OPERATOR_REGISTRY["budget.summary"].pawa_cost
        try:
            result = await OPERATOR_REGISTRY["edit.operator_spec"].fn(
                ctx,
                operator_name="budget.summary",
                field="pawa_cost",
                value=5,
            )
            assert result.succeeded
            assert OPERATOR_REGISTRY["budget.summary"].pawa_cost == 5
        finally:
            OPERATOR_REGISTRY["budget.summary"].pawa_cost = original

    @pytest.mark.asyncio
    async def test_unknown_operator_returns_fail(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["edit.operator_spec"].fn(
            ctx,
            operator_name="no.such.operator",
            field="description",
            value="test",
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_immutable_field_returns_fail(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["edit.operator_spec"].fn(
            ctx,
            operator_name="budget.allocate",
            field="protocol",
            value="streaming",
        )
        assert result.failed

    @pytest.mark.asyncio
    async def test_publishes_event(self):
        ctx = _make_ctx()
        original = OPERATOR_REGISTRY["budget.summary"].description
        try:
            await OPERATOR_REGISTRY["edit.operator_spec"].fn(
                ctx,
                operator_name="budget.summary",
                field="description",
                value="test",
            )
        finally:
            OPERATOR_REGISTRY["budget.summary"].description = original

        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.edit.operator_spec_modified" for e in events)


# ── Metadata ──────────────────────────────────────────────────────────────────

class TestEditMetadata:

    def test_state_patch_protocol(self):
        assert OPERATOR_REGISTRY["edit.state_patch"].protocol == "rpc"

    def test_operator_spec_protocol(self):
        assert OPERATOR_REGISTRY["edit.operator_spec"].protocol == "rpc"
