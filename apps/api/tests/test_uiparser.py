"""
tests/test_uiparser.py

Tests for UISchemaParser — parse() and resolve_source().
"""

import pytest
from sustena.core.uiparser import UISchema, UISchemaField, UISchemaParser, ResponseWidget


@pytest.fixture
def parser():
    return UISchemaParser()


# ── parse() ────────────────────────────────────────────────────────────────────

class TestParse:
    def test_parse_minimal(self, parser):
        schema = parser.parse({"widget_type": "budget_ring"})
        assert isinstance(schema, UISchema)
        assert schema.widget_type == "budget_ring"
        assert schema.fields == []
        assert schema.ctas == []
        assert schema.chart is None

    def test_parse_full(self, parser):
        raw = {
            "widget_type": "budget_allocation_card",
            "fields": [
                {"label": "Pocket",  "source": "inputs.pocket_name", "display": "text"},
                {"label": "Amount",  "source": "inputs.amount",      "display": "currency"},
                {"label": "Balance", "source": "state.finances.liquid.balance", "display": "currency",
                 "colour_rule": "amber_if_below_20pct"},
            ],
            "ctas": ["View Budget", "Allocate Another"],
            "chart": "donut",
        }
        schema = parser.parse(raw)
        assert schema.widget_type == "budget_allocation_card"
        assert len(schema.fields) == 3
        assert schema.fields[0].label == "Pocket"
        assert schema.fields[0].source == "inputs.pocket_name"
        assert schema.fields[0].display == "text"
        assert schema.fields[2].colour_rule == "amber_if_below_20pct"
        assert schema.ctas == ["View Budget", "Allocate Another"]
        assert schema.chart == "donut"

    def test_parse_missing_widget_type_raises(self, parser):
        with pytest.raises(ValueError, match="widget_type"):
            parser.parse({"fields": []})

    def test_parse_ignores_non_dict_fields(self, parser):
        schema = parser.parse({
            "widget_type": "test_widget",
            "fields": ["not-a-dict", {"label": "X", "source": "inputs.x"}],
        })
        assert len(schema.fields) == 1
        assert schema.fields[0].label == "X"

    def test_parse_field_defaults(self, parser):
        schema = parser.parse({
            "widget_type": "test",
            "fields": [{"label": "L", "source": "inputs.x"}],
        })
        f = schema.fields[0]
        assert f.display == "text"
        assert f.colour_rule is None

    def test_to_dict_roundtrip(self, parser):
        raw = {
            "widget_type": "budget_ring",
            "fields": [{"label": "Balance", "source": "state.finances.liquid.balance", "display": "currency"}],
            "ctas": ["Allocate"],
            "chart": "donut",
        }
        schema = parser.parse(raw)
        d = schema.to_dict()
        assert d["widget_type"] == "budget_ring"
        assert d["chart"] == "donut"
        assert d["fields"][0]["label"] == "Balance"


# ── resolve_source() ───────────────────────────────────────────────────────────

class TestResolveSource:
    def test_inputs_key(self, parser):
        result = parser.resolve_source("inputs.pocket_name", {"pocket_name": "food"}, {})
        assert result == "food"

    def test_inputs_missing_key_returns_none(self, parser):
        result = parser.resolve_source("inputs.nonexistent", {}, {})
        assert result is None

    def test_state_dict_simple(self, parser):
        state = {"finances": {"liquid": {"balance": 12345.0}}}
        result = parser.resolve_source("state.finances.liquid.balance", {}, state)
        assert result == 12345.0

    def test_state_dict_missing_path_returns_none(self, parser):
        state = {"finances": {}}
        result = parser.resolve_source("state.finances.liquid.balance", {}, state)
        assert result is None

    def test_state_accessor(self, parser):
        class FakeStateAccessor:
            def get(self, path, default=None):
                data = {"finances.liquid.balance": 5000.0}
                return data.get(path, default)

        result = parser.resolve_source("state.finances.liquid.balance", {}, FakeStateAccessor())
        assert result == 5000.0

    def test_subtraction_formula(self, parser):
        state = {
            "finances": {
                "pockets": {
                    "food": {"allocated": 5000.0, "spent": 1200.0}
                }
            }
        }
        expr = "state.finances.pockets.food.allocated - state.finances.pockets.food.spent"
        result = parser.resolve_source(expr, {}, state)
        assert result == pytest.approx(3800.0)

    def test_subtraction_with_none_left_treats_as_zero(self, parser):
        state = {"finances": {"pockets": {"food": {"spent": 500.0}}}}
        result = parser.resolve_source(
            "state.finances.pockets.food.allocated - state.finances.pockets.food.spent",
            {}, state,
        )
        assert result == pytest.approx(-500.0)

    def test_unrecognised_expression_returns_none(self, parser):
        result = parser.resolve_source("unknown.something", {}, {})
        assert result is None

    def test_no_eval_call_in_module(self):
        import re
        import inspect
        import sustena.core.uiparser as mod
        src = inspect.getsource(mod)
        # Check for actual eval() call (not the word in comments/docstrings)
        assert not re.search(r"^\s*\S*eval\(", src, re.MULTILINE), \
            "eval() call must not appear in uiparser.py"


# ── ResponseWidget ─────────────────────────────────────────────────────────────

class TestResponseWidget:
    def test_to_dict(self):
        w = ResponseWidget(
            widget_type="budget_ring",
            data={"fields": []},
            summary="Test summary",
        )
        d = w.to_dict()
        assert d["type"] == "budget_ring"
        assert d["data"] == {"fields": []}
        assert d["summary"] == "Test summary"
