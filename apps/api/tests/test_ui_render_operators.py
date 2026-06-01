"""
tests/test_ui_render_operators.py

Tests for ui.render.* operators:
  ui.render.operator_card
  ui.render.operative_dashboard
  ui.render.sustain_home
  ui.render.preview
"""

import pytest
import pytest_asyncio
from unittest.mock import MagicMock, AsyncMock
from datetime import datetime

import sustena.operators  # noqa: F401 — populates OPERATOR_REGISTRY
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult
from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger


# ── Shared fixtures ────────────────────────────────────────────────────────────

@pytest.fixture
def mock_ctx():
    """Minimal OperatorContext with a plain-dict state."""
    state = MagicMock()
    state.get = lambda path, default=None: {
        "finances.liquid.balance": 12000.0,
        "finances.pockets.food.allocated": 5000.0,
        "finances.pockets.food.spent": 1200.0,
    }.get(path, default)
    state.exists = lambda path: True

    pawa = AsyncMock()
    pawa.deduct = AsyncMock(return_value=True)

    events = AsyncMock()
    events.publish = AsyncMock()

    return OperatorContext(
        state=state,
        events=events,
        pawa=pawa,
        sustain_id="homestead.test",
        user_id="test_user",
        timestamp=datetime(2026, 6, 1, 12, 0, 0),
    )


# ── ui.render.operator_card ────────────────────────────────────────────────────

class TestUIRenderOperatorCard:
    @pytest.mark.asyncio
    async def test_renders_correct_widget_type(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operator_card
        ui_schema = {
            "widget_type": "budget_allocation_card",
            "fields": [
                {"label": "Pocket", "source": "inputs.pocket_name", "display": "text"},
                {"label": "Amount", "source": "inputs.amount",      "display": "currency"},
            ],
            "ctas": ["View Budget"],
        }
        result = await ui_render_operator_card(
            mock_ctx,
            ui_schema=ui_schema,
            inputs={"pocket_name": "food", "amount": 3000},
        )
        assert result.succeeded
        assert result.data["type"] == "budget_allocation_card"

    @pytest.mark.asyncio
    async def test_resolves_inputs_fields(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operator_card
        result = await ui_render_operator_card(
            mock_ctx,
            ui_schema={
                "widget_type": "test",
                "fields": [{"label": "Name", "source": "inputs.pocket_name"}],
                "ctas": [],
            },
            inputs={"pocket_name": "rent"},
        )
        assert result.succeeded
        fields = result.data["data"]["fields"]
        assert fields[0]["value"] == "rent"

    @pytest.mark.asyncio
    async def test_resolves_state_fields(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operator_card
        result = await ui_render_operator_card(
            mock_ctx,
            ui_schema={
                "widget_type": "test",
                "fields": [{"label": "Balance", "source": "state.finances.liquid.balance"}],
                "ctas": [],
            },
        )
        assert result.succeeded
        assert result.data["data"]["fields"][0]["value"] == 12000.0

    @pytest.mark.asyncio
    async def test_invalid_schema_fails(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operator_card
        result = await ui_render_operator_card(
            mock_ctx,
            ui_schema={"fields": []},   # missing widget_type
        )
        assert result.failed
        assert "Invalid ui_schema" in result.reason

    @pytest.mark.asyncio
    async def test_result_data_included(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operator_card
        result = await ui_render_operator_card(
            mock_ctx,
            ui_schema={"widget_type": "test", "fields": [], "ctas": []},
            result_data={"pocket": "food", "amount_allocated": 3000},
        )
        assert result.succeeded
        assert result.data["data"]["result"]["pocket"] == "food"


# ── ui.render.operative_dashboard ─────────────────────────────────────────────

class TestUIRenderOperativeDashboard:
    @pytest.mark.asyncio
    async def test_basic_render(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operative_dashboard
        result = await ui_render_operative_dashboard(
            mock_ctx,
            ui_schema={
                "widget_type": "operative_status_card",
                "fields": [{"label": "Status", "source": "inputs.status"}],
                "ctas": [],
            },
            operative_state={"name": "Mentor", "status": "active"},
        )
        assert result.succeeded
        assert result.data["type"] == "operative_status_card"
        fields = result.data["data"]["fields"]
        assert fields[0]["value"] == "active"

    @pytest.mark.asyncio
    async def test_operative_state_in_widget_data(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operative_dashboard
        result = await ui_render_operative_dashboard(
            mock_ctx,
            ui_schema={"widget_type": "operative_status_card", "fields": [], "ctas": []},
            operative_state={"name": "Curator"},
        )
        assert result.data["data"]["operative"]["name"] == "Curator"

    @pytest.mark.asyncio
    async def test_empty_operative_state(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_operative_dashboard
        result = await ui_render_operative_dashboard(
            mock_ctx,
            ui_schema={"widget_type": "operative_status_card", "fields": [], "ctas": []},
        )
        assert result.succeeded


# ── ui.render.sustain_home ─────────────────────────────────────────────────────

class TestUIRenderSustainHome:
    @pytest.mark.asyncio
    async def test_basic_render(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_sustain_home
        result = await ui_render_sustain_home(
            mock_ctx,
            ui_schema={
                "widget_type": "sustain_home",
                "fields": [
                    {"label": "Balance", "source": "state.finances.liquid.balance", "display": "currency"},
                ],
                "ctas": ["Ask Orchie"],
            },
            active_operatives=[{"name": "Mentor"}, {"name": "Curator"}],
        )
        assert result.succeeded
        assert result.data["type"] == "sustain_home"
        assert result.data["data"]["fields"][0]["value"] == 12000.0
        assert len(result.data["data"]["operatives"]) == 2

    @pytest.mark.asyncio
    async def test_empty_operatives_defaults(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_sustain_home
        result = await ui_render_sustain_home(
            mock_ctx,
            ui_schema={"widget_type": "sustain_home", "fields": [], "ctas": []},
        )
        assert result.succeeded
        assert result.data["data"]["operatives"] == []


# ── ui.render.preview ──────────────────────────────────────────────────────────

class TestUIRenderPreview:
    @pytest.mark.asyncio
    async def test_bare_ui_schema(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_preview
        result = await ui_render_preview(
            mock_ctx,
            spec_json={
                "widget_type": "budget_allocation_card",
                "fields": [{"label": "Pocket", "source": "inputs.pocket_name"}],
                "ctas": ["View Budget"],
            },
            mock_state={"inputs": {"pocket_name": "food"}},
        )
        assert result.succeeded
        assert result.data["type"] == "budget_allocation_card"
        assert result.data["data"]["fields"][0]["value"] == "food"

    @pytest.mark.asyncio
    async def test_operator_spec_with_ui_schema_key(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_preview
        spec = {
            "name": "budget.allocate",
            "ui_schema": {
                "widget_type": "budget_allocation_card",
                "fields": [{"label": "Amount", "source": "inputs.amount"}],
                "ctas": [],
            },
        }
        result = await ui_render_preview(
            mock_ctx,
            spec_json=spec,
            mock_state={"inputs": {"amount": 5000}},
        )
        assert result.succeeded
        assert result.data["data"]["fields"][0]["value"] == 5000

    @pytest.mark.asyncio
    async def test_invalid_spec_fails(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_preview
        result = await ui_render_preview(mock_ctx, spec_json={"fields": []})
        assert result.failed

    @pytest.mark.asyncio
    async def test_subtraction_formula_in_preview(self, mock_ctx):
        from sustena.operators.ui_render import ui_render_preview
        mock_state = {
            "finances": {
                "pockets": {"food": {"allocated": 5000.0, "spent": 1200.0}}
            }
        }
        result = await ui_render_preview(
            mock_ctx,
            spec_json={
                "widget_type": "test",
                "fields": [{
                    "label": "Remaining",
                    "source": "state.finances.pockets.food.allocated - state.finances.pockets.food.spent",
                }],
                "ctas": [],
            },
            mock_state=mock_state,
        )
        assert result.succeeded
        assert result.data["data"]["fields"][0]["value"] == pytest.approx(3800.0)


# ── OPERATOR_REGISTRY ──────────────────────────────────────────────────────────

class TestUIRenderRegistration:
    def test_all_ui_render_operators_registered(self):
        expected = {
            "ui.render.operator_card",
            "ui.render.operative_dashboard",
            "ui.render.sustain_home",
            "ui.render.preview",
        }
        assert expected <= set(OPERATOR_REGISTRY.keys())

    def test_all_ui_render_operators_have_zero_pawa_cost(self):
        for name in ["ui.render.operator_card", "ui.render.operative_dashboard",
                     "ui.render.sustain_home", "ui.render.preview"]:
            assert OPERATOR_REGISTRY[name].pawa_cost == 0

    def test_all_ui_render_operators_have_no_side_effects(self):
        for name in ["ui.render.operator_card", "ui.render.operative_dashboard",
                     "ui.render.sustain_home", "ui.render.preview"]:
            assert OPERATOR_REGISTRY[name].side_effects == []
