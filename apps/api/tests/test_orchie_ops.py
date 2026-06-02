"""
tests/test_orchie_ops.py

Tests for orchie.morning_brief operator.

Covers:
  - Operator is registered with correct metadata
  - Empty state → "nothing scheduled · sustain state nominal"
  - Tasks due today are included; completed and off-date tasks are excluded
  - Calendar events for today are included; other dates excluded
  - Passed council proposals included; non-PASSED excluded
  - Liquid balance reflected in widget and brief_text
  - Full state: all sections populated, widget and brief_text correct

Run with: pytest tests/test_orchie_ops.py -v
"""

import pytest
from datetime import datetime, timezone

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration

TODAY = "2026-06-02"
TOMORROW = "2026-06-03"
YESTERDAY = "2026-06-01"
NOW = datetime(2026, 6, 2, 8, 0, 0, tzinfo=timezone.utc)


def _make_ctx(state_dict: dict) -> OperatorContext:
    state = StateAccessor(state_dict)
    bus = EventBus(sustain_id="test-sustain")
    ledger = PawaLedger()
    return OperatorContext(
        state=state,
        events=bus,
        pawa=ledger,
        sustain_id="test-sustain",
        user_id="user-test",
        operative_id=None,
        timestamp=NOW,
    )


def _empty_ctx() -> OperatorContext:
    return _make_ctx({
        "tasks": {"items": []},
        "calendar": {"events": []},
        "council_proposals": [],
        "finances": {"liquid": {"balance": 0.0}},
    })


# ── Registry ───────────────────────────────────────────────────────────────────

class TestOrchieOpsRegistry:

    def test_registered(self):
        assert "orchie.morning_brief" in OPERATOR_REGISTRY

    def test_metadata(self):
        meta = OPERATOR_REGISTRY["orchie.morning_brief"]
        assert meta.protocol == "rpc"
        assert meta.pawa_cost == 0
        assert meta.author == "sustena_core"
        assert meta.license_tier == "free"
        assert isinstance(meta.side_effects, list)
        assert meta.side_effects == []


# ── Empty state ────────────────────────────────────────────────────────────────

class TestMorningBriefEmpty:

    @pytest.mark.asyncio
    async def test_empty_returns_ok(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _empty_ctx()
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        assert result.status == "ok"

    @pytest.mark.asyncio
    async def test_empty_brief_text(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _empty_ctx()
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        assert result.data["brief_text"] == "nothing scheduled · sustain state nominal"

    @pytest.mark.asyncio
    async def test_empty_widget_summary(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _empty_ctx()
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        widget = result.data["widget"]
        assert widget["summary"] == "nothing scheduled · sustain state nominal"

    @pytest.mark.asyncio
    async def test_empty_widget_type(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _empty_ctx()
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        assert result.data["widget"]["type"] == "morning_brief_card"

    @pytest.mark.asyncio
    async def test_empty_widget_counts_are_zero(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _empty_ctx()
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        data = result.data["widget"]["data"]
        assert data["task_count"] == 0
        assert data["event_count"] == 0
        assert data["strategy_count"] == 0


# ── Tasks filtering ────────────────────────────────────────────────────────────

class TestMorningBriefTasks:

    def _ctx_with_tasks(self) -> OperatorContext:
        return _make_ctx({
            "tasks": {"items": [
                {"id": "t1", "title": "Buy milk", "due_date": TODAY,     "status": "active",    "priority": "normal", "assigned_to": "", "created_at": "2026-06-01T10:00:00"},
                {"id": "t2", "title": "Pay rent",  "due_date": TOMORROW, "status": "active",    "priority": "high",   "assigned_to": "", "created_at": "2026-06-01T10:00:00"},
                {"id": "t3", "title": "Old task",  "due_date": TODAY,    "status": "completed", "priority": "low",    "assigned_to": "", "created_at": "2026-06-01T10:00:00"},
            ]},
            "calendar": {"events": []},
            "council_proposals": [],
            "finances": {"liquid": {"balance": 5000.0}},
        })

    @pytest.mark.asyncio
    async def test_due_today_included(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_tasks(), date_override=TODAY)
        data = result.data["widget"]["data"]
        assert data["task_count"] == 1
        assert data["tasks_due_today"][0]["title"] == "Buy milk"

    @pytest.mark.asyncio
    async def test_completed_excluded(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_tasks(), date_override=TODAY)
        data = result.data["widget"]["data"]
        titles = [t["title"] for t in data["tasks_due_today"]]
        assert "Old task" not in titles

    @pytest.mark.asyncio
    async def test_tomorrow_task_excluded(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_tasks(), date_override=TODAY)
        data = result.data["widget"]["data"]
        titles = [t["title"] for t in data["tasks_due_today"]]
        assert "Pay rent" not in titles

    @pytest.mark.asyncio
    async def test_brief_text_mentions_task(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_tasks(), date_override=TODAY)
        assert "Buy milk" in result.data["brief_text"]


# ── Calendar events filtering ──────────────────────────────────────────────────

class TestMorningBriefEvents:

    def _ctx_with_events(self) -> OperatorContext:
        return _make_ctx({
            "tasks": {"items": []},
            "calendar": {"events": [
                {"id": "e1", "title": "Team sync",    "date": f"{TODAY}T09:00:00"},
                {"id": "e2", "title": "Doctor appt",  "date": f"{TOMORROW}T14:00:00"},
                {"id": "e3", "title": "Past event",   "date": f"{YESTERDAY}T10:00:00"},
            ]},
            "council_proposals": [],
            "finances": {"liquid": {"balance": 0.0}},
        })

    @pytest.mark.asyncio
    async def test_today_event_included(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_events(), date_override=TODAY)
        data = result.data["widget"]["data"]
        assert data["event_count"] == 1
        assert data["events_today"][0]["title"] == "Team sync"

    @pytest.mark.asyncio
    async def test_tomorrow_event_excluded(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_events(), date_override=TODAY)
        data = result.data["widget"]["data"]
        titles = [e["title"] for e in data["events_today"]]
        assert "Doctor appt" not in titles

    @pytest.mark.asyncio
    async def test_brief_text_mentions_event(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_events(), date_override=TODAY)
        assert "Team sync" in result.data["brief_text"]


# ── Council proposals filtering ────────────────────────────────────────────────

class TestMorningBriefProposals:

    def _ctx_with_proposals(self) -> OperatorContext:
        return _make_ctx({
            "tasks": {"items": []},
            "calendar": {"events": []},
            "council_proposals": [
                {"id": "p1", "operator_name": "budget.allocate", "status": "PASSED",    "proposed_by": "mentor"},
                {"id": "p2", "operator_name": "budget.spend",    "status": "IN_VOTING", "proposed_by": "mentor"},
                {"id": "p3", "operator_name": "edit.state_patch","status": "FAILED",    "proposed_by": "curator"},
            ],
            "finances": {"liquid": {"balance": 12000.0}},
        })

    @pytest.mark.asyncio
    async def test_passed_proposals_included(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_proposals(), date_override=TODAY)
        data = result.data["widget"]["data"]
        assert data["strategy_count"] == 1
        assert data["passed_strategies"][0]["id"] == "p1"

    @pytest.mark.asyncio
    async def test_non_passed_excluded(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_proposals(), date_override=TODAY)
        data = result.data["widget"]["data"]
        ids = [p["id"] for p in data["passed_strategies"]]
        assert "p2" not in ids
        assert "p3" not in ids

    @pytest.mark.asyncio
    async def test_brief_text_mentions_strategy(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        result = await orchie_morning_brief(self._ctx_with_proposals(), date_override=TODAY)
        assert "strateg" in result.data["brief_text"]


# ── Liquid balance ─────────────────────────────────────────────────────────────

class TestMorningBriefLiquid:

    @pytest.mark.asyncio
    async def test_liquid_in_widget_data(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _make_ctx({
            "tasks": {"items": []},
            "calendar": {"events": []},
            "council_proposals": [],
            "finances": {"liquid": {"balance": 25000.0}},
        })
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        assert result.data["widget"]["data"]["liquid_balance"] == 25000.0

    @pytest.mark.asyncio
    async def test_liquid_in_summary_when_data_present(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _make_ctx({
            "tasks": {"items": [
                {"id": "t1", "title": "Buy milk", "due_date": TODAY, "status": "active",
                 "priority": "normal", "assigned_to": "", "created_at": "2026-06-01T00:00:00"},
            ]},
            "calendar": {"events": []},
            "council_proposals": [],
            "finances": {"liquid": {"balance": 7500.0}},
        })
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        assert "7,500" in result.data["widget"]["summary"]


# ── Full state ─────────────────────────────────────────────────────────────────

class TestMorningBriefFullState:

    @pytest.mark.asyncio
    async def test_all_sections_populated(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _make_ctx({
            "tasks": {"items": [
                {"id": "t1", "title": "Write report", "due_date": TODAY, "status": "active",
                 "priority": "high", "assigned_to": "", "created_at": "2026-06-01T00:00:00"},
            ]},
            "calendar": {"events": [
                {"id": "e1", "title": "Budget review", "date": f"{TODAY}T10:00:00"},
            ]},
            "council_proposals": [
                {"id": "p1", "operator_name": "budget.allocate", "status": "PASSED", "proposed_by": "mentor"},
            ],
            "finances": {"liquid": {"balance": 18000.0}},
        })
        result = await orchie_morning_brief(ctx, date_override=TODAY)

        assert result.status == "ok"
        data = result.data["widget"]["data"]
        assert data["task_count"] == 1
        assert data["event_count"] == 1
        assert data["strategy_count"] == 1
        assert data["liquid_balance"] == 18000.0
        assert data["today"] == TODAY

        brief = result.data["brief_text"]
        assert brief.startswith("Habari.")
        assert "Write report" in brief
        assert "Budget review" in brief
        assert "18,000" in brief

    @pytest.mark.asyncio
    async def test_widget_has_correct_keys(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _empty_ctx()
        result = await orchie_morning_brief(ctx, date_override=TODAY)
        widget = result.data["widget"]
        assert "type" in widget
        assert "data" in widget
        assert "summary" in widget

    @pytest.mark.asyncio
    async def test_date_override_respected(self):
        from sustena.operators.orchie_ops import orchie_morning_brief
        ctx = _make_ctx({
            "tasks": {"items": [
                {"id": "t1", "title": "Meeting prep", "due_date": TOMORROW, "status": "active",
                 "priority": "normal", "assigned_to": "", "created_at": "2026-06-01T00:00:00"},
            ]},
            "calendar": {"events": []},
            "council_proposals": [],
            "finances": {"liquid": {"balance": 0.0}},
        })
        # Override to TOMORROW — task should now appear
        result = await orchie_morning_brief(ctx, date_override=TOMORROW)
        data = result.data["widget"]["data"]
        assert data["task_count"] == 1
        assert data["tasks_due_today"][0]["title"] == "Meeting prep"
