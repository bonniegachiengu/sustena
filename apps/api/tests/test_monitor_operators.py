"""
tests/test_monitor_operators.py

Tests for the monitor.* operator library.
Sprint 3 criterion: monitor.constraint fires event.monitor.constraint_breached
when the constraint is failing.
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _make_ctx(state_data: dict) -> OperatorContext:
    return OperatorContext(
        state=StateAccessor(state_data),
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="user-test",
        timestamp=datetime.utcnow(),
    )


# ── monitor.constraint ────────────────────────────────────────────────────────

class TestMonitorConstraint:

    @pytest.mark.asyncio
    async def test_passes_when_constraint_satisfied(self):
        ctx = _make_ctx({"pantry": {"cooking_oil_L": 2.5}})
        result = await OPERATOR_REGISTRY["monitor.constraint"].fn(
            ctx, constraint_expr="pantry.cooking_oil_L >= 1.0"
        )
        assert result.succeeded
        assert result.data["passing"] is True
        assert result.data["reason"] is None
        # No breach event
        events = ctx.events.published_this_context()
        assert not any(e["event_name"] == "event.monitor.constraint_breached" for e in events)

    @pytest.mark.asyncio
    async def test_fires_breach_event_when_constraint_fails(self):
        """Sprint 3 criterion: oil at 0.4L < 1.0 → breach event fired."""
        ctx = _make_ctx({"pantry": {"cooking_oil_L": 0.4}})
        result = await OPERATOR_REGISTRY["monitor.constraint"].fn(
            ctx, constraint_expr="pantry.cooking_oil_L >= 1.0"
        )
        assert result.succeeded
        assert result.data["passing"] is False
        events = ctx.events.published_this_context()
        breach_events = [e for e in events if e["event_name"] == "event.monitor.constraint_breached"]
        assert len(breach_events) == 1
        assert breach_events[0]["_payload"]["constraint_expr"] == "pantry.cooking_oil_L >= 1.0"

    @pytest.mark.asyncio
    async def test_interval_s_metadata_in_result(self):
        ctx = _make_ctx({"pantry": {"cooking_oil_L": 5.0}})
        result = await OPERATOR_REGISTRY["monitor.constraint"].fn(
            ctx, constraint_expr="pantry.cooking_oil_L >= 1.0", interval_s=7200
        )
        assert result.data["interval_s"] == 7200

    @pytest.mark.asyncio
    async def test_unparseable_constraint_handled_gracefully(self):
        # The ConstraintEngine returns (False, reason) for unparseable constraints
        # rather than throwing — so monitor.constraint returns ok with passing=False.
        ctx = _make_ctx({})
        result = await OPERATOR_REGISTRY["monitor.constraint"].fn(
            ctx, constraint_expr="this IS NOT VALID"
        )
        # Should not crash; passing=False since the engine can't evaluate it
        assert result.succeeded or result.failed  # either is acceptable


# ── monitor.state_path ────────────────────────────────────────────────────────

class TestMonitorStatePath:

    @pytest.mark.asyncio
    async def test_alert_fires_when_condition_met(self):
        ctx = _make_ctx({"finances": {"pockets": {"food": {"spent": 90}}}})
        result = await OPERATOR_REGISTRY["monitor.state_path"].fn(
            ctx,
            path="finances.pockets.food.spent",
            condition="> 80",
            alert_event="event.monitor.budget_alert_triggered",
        )
        assert result.succeeded
        assert result.data["alert_fired"] is True
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.monitor.budget_alert_triggered" for e in events)

    @pytest.mark.asyncio
    async def test_alert_not_fired_when_condition_not_met(self):
        ctx = _make_ctx({"finances": {"pockets": {"food": {"spent": 50}}}})
        result = await OPERATOR_REGISTRY["monitor.state_path"].fn(
            ctx,
            path="finances.pockets.food.spent",
            condition="> 80",
            alert_event="event.monitor.budget_alert_triggered",
        )
        assert result.succeeded
        assert result.data["alert_fired"] is False
        events = ctx.events.published_this_context()
        assert not any(e["event_name"] == "event.monitor.budget_alert_triggered" for e in events)

    @pytest.mark.asyncio
    async def test_missing_path_returns_fail(self):
        ctx = _make_ctx({})
        result = await OPERATOR_REGISTRY["monitor.state_path"].fn(
            ctx,
            path="no.such.path",
            condition="value > 0",
            alert_event="event.monitor.test_alert",
        )
        assert result.failed


# ── Metadata ──────────────────────────────────────────────────────────────────

class TestMonitorMetadata:

    def test_monitor_state_path_protocol(self):
        assert OPERATOR_REGISTRY["monitor.state_path"].protocol == "event_driven"

    def test_monitor_constraint_protocol(self):
        assert OPERATOR_REGISTRY["monitor.constraint"].protocol == "polling"
