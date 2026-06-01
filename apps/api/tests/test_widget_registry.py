"""
tests/test_widget_registry.py

Tests for WidgetTypeRegistry — register, list_all, render.
"""

import pytest
from sustena.core.widget_registry import WidgetType, WidgetTypeRegistry


@pytest.fixture
def registry():
    return WidgetTypeRegistry()


class TestBuiltins:
    def test_all_builtins_registered(self, registry):
        expected = {
            "budget_allocation_card", "budget_ring", "transaction_confirmation",
            "calendar_event_card", "operative_status_card", "constraint_health_grid",
            "sustain_home", "chama_summary_card", "procurement_order_card",
        }
        registered = {w["widget_type"] for w in registry.list_all()}
        assert expected <= registered

    def test_budget_ring_has_chart(self, registry):
        wt = registry.get("budget_ring")
        assert wt is not None
        assert wt.schema.get("chart") == "donut"

    def test_list_all_returns_dicts(self, registry):
        result = registry.list_all()
        assert isinstance(result, list)
        for item in result:
            assert "widget_type" in item
            assert "description" in item
            assert "schema" in item


class TestRegister:
    def test_register_new(self, registry):
        registry.register("custom_card", description="A custom widget", schema={"fields": ["X"]})
        wt = registry.get("custom_card")
        assert wt is not None
        assert wt.description == "A custom widget"

    def test_register_overwrites(self, registry):
        registry.register("budget_ring", description="Updated description")
        wt = registry.get("budget_ring")
        assert wt.description == "Updated description"

    def test_registered_widget_appears_in_list_all(self, registry):
        registry.register("unique_widget_xyz", description="Test")
        names = {w["widget_type"] for w in registry.list_all()}
        assert "unique_widget_xyz" in names


class TestRender:
    def test_render_generic_fallback(self, registry):
        widget = {
            "type": "unknown_widget",
            "data": {
                "fields": [{"label": "Foo", "value": "bar"}],
                "ctas": ["Go"],
            },
            "summary": "Test",
        }
        html = registry.render(widget)
        assert "Foo" in html
        assert "bar" in html
        assert "Go" in html
        assert "sustena-widget" in html

    def test_render_none_value_shows_dash(self, registry):
        widget = {
            "type": "budget_ring",
            "data": {"fields": [{"label": "Balance", "value": None}], "ctas": []},
            "summary": "",
        }
        html = registry.render(widget)
        assert "—" in html

    def test_render_with_template(self, registry):
        registry.register(
            "templated",
            description="Has a template",
            template='<span class="val">{{ data.fields[0].value }}</span>',
        )
        widget = {
            "type": "templated",
            "data": {"fields": [{"label": "X", "value": "hello"}], "ctas": []},
            "summary": "",
        }
        html = registry.render(widget)
        assert "hello" in html

    def test_render_template_error_falls_back_gracefully(self, registry):
        registry.register("broken", template="{{ this_will_crash | undefined_filter }}")
        widget = {"type": "broken", "data": {"fields": [], "ctas": []}, "summary": ""}
        # Should not raise — falls back to generic
        html = registry.render(widget)
        assert "sustena-widget" in html


class TestWidgetTypeDictShape:
    def test_to_dict(self):
        wt = WidgetType(
            widget_type="test",
            description="A test widget",
            schema={"fields": ["X"]},
        )
        d = wt.to_dict()
        assert d == {
            "widget_type": "test",
            "description": "A test widget",
            "schema": {"fields": ["X"]},
        }
        assert "template" not in d
