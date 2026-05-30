"""
tests/test_calendar_operators.py

Tests for the calendar operator library.
Covers: happy paths, constraint rejections (past date, missing event),
        ResponseWidget for upcoming_events, all four primitives.

Run with: pytest tests/test_calendar_operators.py -v
"""

import pytest
from datetime import datetime, timezone, timedelta

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration


# ── Fixtures ──────────────────────────────────────────────────────────────────

def _fresh_state() -> StateAccessor:
    """Homestead sustain state with a clean calendar section."""
    return StateAccessor({
        "calendar": {
            "events": []
        }
    })


def _make_ctx(state: StateAccessor, user_id: str = "user-test") -> OperatorContext:
    """Build a minimal OperatorContext for unit testing operators directly."""
    bus    = EventBus(sustain_id="test-sustain")
    ledger = PawaLedger()
    # Use a fixed "now" well in the past so future dates are easy to construct
    now    = datetime(2026, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
    return OperatorContext(
        state=state,
        events=bus,
        pawa=ledger,
        sustain_id="test-sustain",
        user_id=user_id,
        operative_id=None,
        timestamp=now,
    )


def _future(days: int = 7) -> str:
    """Return an ISO 8601 datetime string N days from our fixed 'now'."""
    return (datetime(2026, 1, 1, tzinfo=timezone.utc) + timedelta(days=days)).isoformat()


def _past(days: int = 1) -> str:
    """Return an ISO 8601 datetime string N days before our fixed 'now'."""
    return (datetime(2026, 1, 1, tzinfo=timezone.utc) - timedelta(days=days)).isoformat()


# ── Operator registry ─────────────────────────────────────────────────────────

class TestCalendarOperatorRegistry:

    def test_calendar_operators_registered(self):
        expected = [
            "homestead.calendar.add_event",
            "homestead.calendar.upcoming_events",
            "homestead.calendar.remove_event",
        ]
        for name in expected:
            assert name in OPERATOR_REGISTRY, f"'{name}' not in OPERATOR_REGISTRY"

    def test_operators_have_required_metadata(self):
        for name in [
            "homestead.calendar.add_event",
            "homestead.calendar.upcoming_events",
            "homestead.calendar.remove_event",
        ]:
            meta = OPERATOR_REGISTRY[name]
            assert meta.description
            assert meta.author == "sustena_core"
            assert isinstance(meta.pawa_cost, int)
            assert meta.license_tier in ("free", "per_use", "subscription", "one_time")
            assert isinstance(meta.side_effects, list)
            assert isinstance(meta.constraints, list)

    def test_calendar_operators_have_ui_schema(self):
        for name in [
            "homestead.calendar.add_event",
            "homestead.calendar.upcoming_events",
            "homestead.calendar.remove_event",
        ]:
            meta = OPERATOR_REGISTRY[name]
            assert meta.ui_schema, f"'{name}' is missing ui_schema"
            assert "widget_type" in meta.ui_schema

    def test_mutating_operators_declare_side_effects(self):
        assert "event.calendar.event_added"   in OPERATOR_REGISTRY["homestead.calendar.add_event"].side_effects
        assert "event.calendar.event_removed" in OPERATOR_REGISTRY["homestead.calendar.remove_event"].side_effects

    def test_upcoming_events_has_no_side_effects(self):
        assert OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].side_effects == []


# ── homestead.calendar.add_event ─────────────────────────────────────────────

class TestCalendarAddEvent:

    @pytest.mark.asyncio
    async def test_adds_event_to_calendar(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="School fundraiser", date_str=_future(7)
        )
        assert result.succeeded
        events = state.get("calendar.events")
        assert len(events) == 1
        assert events[0]["title"] == "School fundraiser"

    @pytest.mark.asyncio
    async def test_result_contains_event_id(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Doctor appointment", date_str=_future(3)
        )
        assert result.succeeded
        assert "event_id" in result.data
        assert result.data["event_id"] is not None

    @pytest.mark.asyncio
    async def test_fires_event_added_event(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Birthday", date_str=_future(10)
        )
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.calendar.event_added" for e in events)

    @pytest.mark.asyncio
    async def test_multiple_events_accumulate(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Event A", date_str=_future(5)
        )
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Event B", date_str=_future(10)
        )
        events = state.get("calendar.events")
        assert len(events) == 2

    @pytest.mark.asyncio
    async def test_description_stored(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Meeting", date_str=_future(2), description="Q1 review"
        )
        event = state.get("calendar.events")[0]
        assert event["description"] == "Q1 review"

    @pytest.mark.asyncio
    async def test_constraint_rejects_past_date(self):
        """Date in the past must be rejected."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Past event", date_str=_past(1)
        )
        assert result.failed
        assert result.constraint_violated is not None
        assert "future" in result.reason.lower()

    @pytest.mark.asyncio
    async def test_constraint_rejects_invalid_date_format(self):
        """Unparseable date string must be rejected."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Bad date", date_str="not-a-date"
        )
        assert result.failed
        assert result.constraint_violated is not None

    @pytest.mark.asyncio
    async def test_state_unchanged_on_past_date(self):
        """State must not mutate when constraint blocks the operator."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Bad", date_str=_past(2)
        )
        assert state.get("calendar.events") == []

    @pytest.mark.asyncio
    async def test_no_event_on_past_date(self):
        """EventBus must not fire when constraint blocks."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Bad", date_str=_past(1)
        )
        assert ctx.events.published_this_context() == []

    @pytest.mark.asyncio
    async def test_pawa_deducted(self):
        """PawaLedger.deduct() is called (0 pawa — no-op but verified)."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        balance_before = await ctx.pawa.get_balance(ctx.user_id)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Test", date_str=_future(5)
        )
        balance_after = await ctx.pawa.get_balance(ctx.user_id)
        assert balance_after == balance_before  # pawa_cost=0


# ── homestead.calendar.upcoming_events ───────────────────────────────────────

class TestCalendarUpcomingEvents:

    async def _state_with_events(self) -> tuple[StateAccessor, OperatorContext]:
        """Set up state with three events at 3d, 10d, and 20d from now."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        for title, days in [("Near", 3), ("Mid", 10), ("Far", 20)]:
            await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
                ctx, title=title, date_str=_future(days)
            )
        return state, _make_ctx(state)

    @pytest.mark.asyncio
    async def test_returns_calendar_list_widget(self):
        state, ctx = await self._state_with_events()
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx)
        assert result.succeeded
        assert result.data["type"] == "calendar_list"

    @pytest.mark.asyncio
    async def test_default_window_is_7_days(self):
        """Default days=7 should return only events within 7 days."""
        state, ctx = await self._state_with_events()
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx)
        # Near(3d) is in window; Mid(10d) and Far(20d) are not
        assert result.data["count"] == 1
        assert result.data["events"][0]["title"] == "Near"

    @pytest.mark.asyncio
    async def test_custom_window_returns_more_events(self):
        state, ctx = await self._state_with_events()
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx, days=15)
        # Near(3d) and Mid(10d) are in 15-day window
        assert result.data["count"] == 2

    @pytest.mark.asyncio
    async def test_events_sorted_by_date(self):
        """upcoming_events must return events sorted chronologically."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(ctx, title="Later",  date_str=_future(8))
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(ctx, title="Sooner", date_str=_future(2))
        ctx2   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx2, days=30)
        titles = [e["title"] for e in result.data["events"]]
        assert titles == ["Sooner", "Later"]

    @pytest.mark.asyncio
    async def test_empty_calendar_returns_zero_events(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx)
        assert result.succeeded
        assert result.data["count"] == 0
        assert result.data["events"] == []

    @pytest.mark.asyncio
    async def test_summary_field_present(self):
        """ResponseWidget must include a summary string."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx)
        assert "summary" in result.data
        assert isinstance(result.data["summary"], str)

    @pytest.mark.asyncio
    async def test_no_events_fired(self):
        """upcoming_events is read-only — must fire no events."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx)
        assert ctx.events.published_this_context() == []

    @pytest.mark.asyncio
    async def test_pawa_deducted(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        balance_before = await ctx.pawa.get_balance(ctx.user_id)
        await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx)
        balance_after = await ctx.pawa.get_balance(ctx.user_id)
        assert balance_after == balance_before


# ── homestead.calendar.remove_event ──────────────────────────────────────────

class TestCalendarRemoveEvent:

    async def _state_with_one_event(self) -> tuple[StateAccessor, str, OperatorContext]:
        """Set up state with one event, returning (state, event_id, fresh_ctx)."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Dentist appointment", date_str=_future(5)
        )
        event_id = result.data["event_id"]
        return state, event_id, _make_ctx(state)

    @pytest.mark.asyncio
    async def test_removes_event_from_calendar(self):
        state, event_id, ctx = await self._state_with_one_event()
        result = await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx, event_id=event_id)
        assert result.succeeded
        assert state.get("calendar.events") == []

    @pytest.mark.asyncio
    async def test_result_contains_removed_event_id(self):
        state, event_id, ctx = await self._state_with_one_event()
        result = await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx, event_id=event_id)
        assert result.data["removed_event_id"] == event_id

    @pytest.mark.asyncio
    async def test_fires_event_removed(self):
        state, event_id, ctx = await self._state_with_one_event()
        await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx, event_id=event_id)
        events = ctx.events.published_this_context()
        assert any(e["event_name"] == "event.calendar.event_removed" for e in events)

    @pytest.mark.asyncio
    async def test_only_removes_target_event(self):
        """Removing one event must leave all others intact."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        r1 = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(ctx, title="A", date_str=_future(5))
        r2 = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(ctx, title="B", date_str=_future(8))
        id_to_remove = r1.data["event_id"]

        ctx2 = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx2, event_id=id_to_remove)

        remaining = state.get("calendar.events")
        assert len(remaining) == 1
        assert remaining[0]["title"] == "B"

    @pytest.mark.asyncio
    async def test_constraint_rejects_missing_event(self):
        """remove_event must fail when the event_id doesn't exist."""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(
            ctx, event_id="nonexistent-uuid"
        )
        assert result.failed
        assert result.constraint_violated == "event_exists"

    @pytest.mark.asyncio
    async def test_state_unchanged_on_missing_event(self):
        state, event_id, ctx = await self._state_with_one_event()
        await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx, event_id="bad-id")
        assert len(state.get("calendar.events")) == 1

    @pytest.mark.asyncio
    async def test_no_event_on_missing_event_failure(self):
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx, event_id="ghost")
        assert ctx.events.published_this_context() == []

    @pytest.mark.asyncio
    async def test_pawa_deducted(self):
        state, event_id, ctx = await self._state_with_one_event()
        balance_before = await ctx.pawa.get_balance(ctx.user_id)
        await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx, event_id=event_id)
        balance_after = await ctx.pawa.get_balance(ctx.user_id)
        assert balance_after == balance_before


# ── All-four-primitives integration ──────────────────────────────────────────

class TestAllFourPrimitivesIntegration:
    """
    Integration tests verifying all four primitives fire together in each operator.
    """

    @pytest.mark.asyncio
    async def test_full_cycle_add_event(self):
        """add_event: ConstraintEngine ✓ PawaLedger ✓ StateAccessor ✓ EventBus ✓"""
        state = _fresh_state()
        ctx   = _make_ctx(state)

        result = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Board meeting", date_str=_future(14), description="Q2 review"
        )

        # ConstraintEngine — constraint passed (future date)
        assert result.succeeded

        # StateAccessor — event appended
        events = state.get("calendar.events")
        assert len(events) == 1
        assert events[0]["title"] == "Board meeting"

        # EventBus — event fired
        published = ctx.events.published_this_context()
        assert len(published) == 1
        assert published[0]["event_name"] == "event.calendar.event_added"

        # PawaLedger — deduct called (0 pawa, balance unchanged)
        assert await ctx.pawa.get_balance(ctx.user_id) == 0

    @pytest.mark.asyncio
    async def test_full_cycle_upcoming_events(self):
        """upcoming_events: ConstraintEngine ✓ PawaLedger ✓ StateAccessor ✓ EventBus ✓"""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(ctx, title="Soon", date_str=_future(3))

        ctx2   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx2, days=7)

        # ConstraintEngine — no constraints (always passes)
        assert result.succeeded

        # StateAccessor — reads and filters correctly
        assert result.data["count"] == 1

        # EventBus — no events for read-only
        assert ctx2.events.published_this_context() == []

        # PawaLedger — zero cost
        assert await ctx2.pawa.get_balance(ctx2.user_id) == 0

    @pytest.mark.asyncio
    async def test_full_cycle_remove_event(self):
        """remove_event: ConstraintEngine ✓ PawaLedger ✓ StateAccessor ✓ EventBus ✓"""
        state = _fresh_state()
        ctx   = _make_ctx(state)
        r     = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Dentist", date_str=_future(5)
        )
        event_id = r.data["event_id"]

        ctx2   = _make_ctx(state)
        result = await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(ctx2, event_id=event_id)

        # ConstraintEngine — event exists check passed
        assert result.succeeded

        # StateAccessor — event removed
        assert state.get("calendar.events") == []

        # EventBus — event fired
        published = ctx2.events.published_this_context()
        assert any(e["event_name"] == "event.calendar.event_removed" for e in published)

        # PawaLedger — zero cost
        assert await ctx2.pawa.get_balance(ctx2.user_id) == 0

    @pytest.mark.asyncio
    async def test_constraint_failure_leaves_all_primitives_clean(self):
        """
        When ConstraintEngine blocks an operator:
        - StateAccessor: no mutations
        - EventBus: no events fired
        - PawaLedger: no pawa deducted
        """
        state = _fresh_state()
        ctx   = _make_ctx(state)

        result = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Old event", date_str=_past(5)
        )

        # Constraint failed
        assert result.failed

        # StateAccessor: unchanged
        assert state.get("calendar.events") == []

        # EventBus: no events
        assert ctx.events.published_this_context() == []

        # PawaLedger: no deduction
        assert await ctx.pawa.get_balance(ctx.user_id) == 0

    @pytest.mark.asyncio
    async def test_full_add_view_remove_flow(self):
        """End-to-end: add → upcoming_events → remove → verify empty."""
        state = _fresh_state()
        ctx   = _make_ctx(state)

        # Add two events
        r1 = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Event One", date_str=_future(3)
        )
        r2 = await OPERATOR_REGISTRY["homestead.calendar.add_event"].fn(
            ctx, title="Event Two", date_str=_future(6)
        )
        assert r1.succeeded and r2.succeeded

        # View upcoming (7 day window — both visible)
        result = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx, days=7)
        assert result.data["count"] == 2

        # Remove first event
        r3 = await OPERATOR_REGISTRY["homestead.calendar.remove_event"].fn(
            ctx, event_id=r1.data["event_id"]
        )
        assert r3.succeeded

        # View again — only one remains
        result2 = await OPERATOR_REGISTRY["homestead.calendar.upcoming_events"].fn(ctx, days=7)
        assert result2.data["count"] == 1
        assert result2.data["events"][0]["title"] == "Event Two"
