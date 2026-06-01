"""
tests/test_visualize_operators.py

Tests for the visualize.* operator library.
Sprint 3 criterion: visualize.pocket_ring returns a ResponseWidget that UIParser can render.
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _budget_state() -> StateAccessor:
    return StateAccessor({
        "finances": {
            "liquid": {"balance": 10000.0},
            "pockets": {
                "food":      {"allocated": 5000.0, "spent": 2000.0, "limit": 0.0},
                "transport": {"allocated": 3000.0, "spent": 2800.0, "limit": 0.0},
                "rent":      {"allocated": 15000.0, "spent": 0.0,   "limit": 0.0},
            },
            "income": {"sources": [], "monthly_total": 33000.0},
        }
    })


def _make_ctx(state: StateAccessor | None = None) -> OperatorContext:
    state = state or _budget_state()
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="user-test",
        timestamp=datetime.utcnow(),
    )


# ── visualize.pocket_ring ──────────────────────────────────────────────────────

class TestVisualizePocketRing:

    @pytest.mark.asyncio
    async def test_returns_response_widget(self):
        """Sprint 3 criterion: returns a ResponseWidget that UIParser can render."""
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.pocket_ring"].fn(ctx)

        assert result.succeeded
        widget = result.data
        assert widget["type"] == "budget_ring_chart"
        assert "data" in widget
        assert "summary" in widget

    @pytest.mark.asyncio
    async def test_pocket_ring_data_structure(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.pocket_ring"].fn(ctx)

        data = result.data["data"]
        assert "liquid" in data
        assert "total_allocated" in data
        assert "total_spent" in data
        assert "pct_spent" in data
        assert "pockets" in data
        assert isinstance(data["pockets"], list)

    @pytest.mark.asyncio
    async def test_pocket_ring_liquid_balance(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.pocket_ring"].fn(ctx)
        assert result.data["data"]["liquid"] == 10000.0

    @pytest.mark.asyncio
    async def test_pocket_ring_totals(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.pocket_ring"].fn(ctx)
        data = result.data["data"]
        # 5000 + 3000 + 15000 = 23000 allocated; 2000 + 2800 + 0 = 4800 spent
        assert data["total_allocated"] == 23000.0
        assert data["total_spent"] == 4800.0

    @pytest.mark.asyncio
    async def test_pocket_ring_warn_status(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.pocket_ring"].fn(ctx)
        pockets = {p["name"]: p for p in result.data["data"]["pockets"]}
        # transport: 2800/3000 = 93.3% → "warn" (>80% but <100%)
        assert pockets["transport"]["status"] == "warn"
        # food: 2000/5000 = 40% → "ok"
        assert pockets["food"]["status"] == "ok"

    @pytest.mark.asyncio
    async def test_empty_pockets_handled(self):
        ctx = _make_ctx(StateAccessor({"finances": {"liquid": {"balance": 50000.0}}}))
        result = await OPERATOR_REGISTRY["visualize.pocket_ring"].fn(ctx)
        assert result.succeeded
        assert result.data["data"]["total_allocated"] == 0.0


# ── visualize.event_feed ───────────────────────────────────────────────────────

class TestVisualizeEventFeed:

    @pytest.mark.asyncio
    async def test_returns_event_feed_widget(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.event_feed"].fn(ctx)

        assert result.succeeded
        widget = result.data
        assert widget["type"] == "event_feed"
        assert "events" in widget["data"]
        assert isinstance(widget["data"]["events"], list)

    @pytest.mark.asyncio
    async def test_event_feed_includes_published_events(self):
        ctx = _make_ctx()
        # Publish an event so the feed has something
        await ctx.events.publish("event.test.budget_spent", {"amount": 100})
        result = await OPERATOR_REGISTRY["visualize.event_feed"].fn(ctx, limit=5)

        assert result.succeeded
        events = result.data["data"]["events"]
        assert len(events) >= 1

    @pytest.mark.asyncio
    async def test_event_feed_limit_respected(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.event_feed"].fn(ctx, limit=10)
        assert result.succeeded
        data = result.data["data"]
        assert data["total"] <= 10


# ── visualize.constraint_health ────────────────────────────────────────────────

class TestVisualizeConstraintHealth:

    @pytest.mark.asyncio
    async def test_returns_health_grid_widget(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.constraint_health"].fn(
            ctx,
            constraints=["finances.liquid.balance > 0"],
        )
        assert result.succeeded
        widget = result.data
        assert widget["type"] == "constraint_health_grid"
        assert "constraints" in widget["data"]
        assert "passing" in widget["data"]
        assert "failing" in widget["data"]

    @pytest.mark.asyncio
    async def test_passing_constraint(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.constraint_health"].fn(
            ctx,
            constraints=["finances.liquid.balance > 0"],
        )
        assert result.data["data"]["passing"] == 1
        assert result.data["data"]["failing"] == 0

    @pytest.mark.asyncio
    async def test_failing_constraint(self):
        ctx = _make_ctx(StateAccessor({"finances": {"liquid": {"balance": 0.0}}}))
        result = await OPERATOR_REGISTRY["visualize.constraint_health"].fn(
            ctx,
            constraints=["finances.liquid.balance > 1000"],
        )
        assert result.data["data"]["failing"] == 1

    @pytest.mark.asyncio
    async def test_empty_constraints_list(self):
        ctx = _make_ctx()
        result = await OPERATOR_REGISTRY["visualize.constraint_health"].fn(
            ctx, constraints=[]
        )
        assert result.succeeded
        assert result.data["data"]["total"] == 0


# ── Metadata ──────────────────────────────────────────────────────────────────

class TestVisualizeMetadata:

    def test_pocket_ring_protocol(self):
        assert OPERATOR_REGISTRY["visualize.pocket_ring"].protocol == "rpc"

    def test_event_feed_protocol(self):
        assert OPERATOR_REGISTRY["visualize.event_feed"].protocol == "streaming"

    def test_constraint_health_protocol(self):
        assert OPERATOR_REGISTRY["visualize.constraint_health"].protocol == "rpc"
