"""
tests/test_curated_ui.py

Tests for sustena/core/curated_ui.py — the Curated UI engine's compose(r)
core (Orchie's rendering surface, upgrade spec §4H). Covers:
  - DSL typing: inputs ⊆ dim(S), emits ⊆ T (validate_widget_schema)
  - Binding table β construction (EventClass ∪ Unit → widgets)
  - Urgency (reuses Slice 0's pct formula) and relevance
  - 0/1 knapsack selection under an attention budget, including the
    Flight 401 property: a quiet-but-urgent widget must outrank a
    loud-but-safe one when budget forces a choice
  - compose(r) against a real, live-instantiated homestead sustain:
    read-only (state/events byte-unchanged), honest empty state for a
    sustain with no declared widgets, unknown-sustain handling

Run with:
    python -m pytest tests/test_curated_ui.py -v
"""

import json

import pytest

from sustena.core.curated_ui import (
    WidgetSchema,
    build_binding_table,
    compose,
    knapsack_select,
    load_widget_schemas,
    relevance_for,
    salience,
    urgency_for_pocket,
    validate_all_widget_schemas,
    validate_widget_schema,
)
from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


# ── DSL typing ──────────────────────────────────────────────────────────────

class TestTypeChecking:
    def _schema(self):
        return {
            "finances": {
                "type": "object",
                "properties": {
                    "liquid": {"type": "object", "properties": {"balance": {"type": "number"}}},
                    "pockets": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "object",
                            "properties": {"allocated": {"type": "number"}, "spent": {"type": "number"}},
                        },
                    },
                },
            },
        }

    def test_valid_widget_passes(self):
        w = WidgetSchema(
            id="w1", render="pocket_watch_card",
            inputs=["finances.pockets.*.allocated"], emits=["budget.allocate"],
            event_class="event.finances.pocket_spent",
        )
        errors = validate_widget_schema(w, self._schema(), {"budget.allocate"})
        assert errors == []

    def test_nonexistent_input_path_rejected(self):
        w = WidgetSchema(
            id="w2", render="x", inputs=["finances.pockets.*.nonexistent_field"],
            emits=[], event_class="event.finances.pocket_spent",
        )
        errors = validate_widget_schema(w, self._schema(), set())
        assert any("nonexistent_field" in e for e in errors)

    def test_unregistered_operator_in_emits_rejected(self):
        w = WidgetSchema(
            id="w3", render="x", inputs=[], emits=["not.a.real.operator"],
            event_class="event.finances.pocket_spent",
        )
        errors = validate_widget_schema(w, self._schema(), {"budget.allocate"})
        assert any("not.a.real.operator" in e for e in errors)

    def test_widget_missing_both_event_class_and_unit_rejected(self):
        w = WidgetSchema(id="w4", render="x", inputs=[], emits=[])
        errors = validate_widget_schema(w, self._schema(), set())
        assert any("event_class or unit" in e for e in errors)

    def test_real_homestead_spec_typechecks_clean(self):
        """The 3 declared widgets on the real homestead.json must typecheck
        against the real state_schema and the real OPERATOR_REGISTRY —
        not a synthetic stand-in."""
        import sustena.operators  # noqa: F401 - registers OPERATOR_REGISTRY

        spec = json.loads(
            (
                __import__("pathlib").Path(__file__).resolve().parent.parent
                / "sustena" / "sustains" / "homestead.json"
            ).read_text(encoding="utf-8")
        )
        errors = validate_all_widget_schemas(spec)
        assert errors == [], errors
        widgets = load_widget_schemas(spec)
        assert {w.id for w in widgets} == {
            "pocket_spent_watch", "unmapped_capture_classify", "household_rollup_summary",
        }


# ── Binding table β ──────────────────────────────────────────────────────────

class TestBindingTable:
    def test_groups_event_bound_and_unit_bound_separately(self):
        w1 = WidgetSchema(id="a", render="x", event_class="event.finances.pocket_spent")
        w2 = WidgetSchema(id="b", render="x", unit=True)
        w3 = WidgetSchema(id="c", render="x", event_class="event.finances.pocket_spent")
        beta = build_binding_table([w1, w2, w3])
        assert {w.id for w in beta["event.finances.pocket_spent"]} == {"a", "c"}
        assert {w.id for w in beta["__unit__"]} == {"b"}


# ── Urgency + relevance ──────────────────────────────────────────────────────

class TestUrgencyAndRelevance:
    def test_urgency_matches_monitor_jsx_formula(self):
        assert urgency_for_pocket({"allocated": 1000, "spent": 800}) == pytest.approx(0.8)
        assert urgency_for_pocket({"allocated": 1000, "spent": 1000}) == pytest.approx(1.0)
        assert urgency_for_pocket({"allocated": 0, "spent": 0}) == 0.0  # no div-by-zero

    def test_relevance_neutral_with_no_query(self):
        w = WidgetSchema(id="x", render="x", unit=True)
        assert relevance_for(w, None) == 0.5
        assert relevance_for(w, "") == 0.5

    def test_relevance_rises_with_token_overlap(self):
        w = WidgetSchema(id="pocket_spent_watch", render="x", description="pocket spend watch", unit=True)
        assert relevance_for(w, "pocket") > relevance_for(w, "unrelated_zzz")


# ── 0/1 knapsack selection ───────────────────────────────────────────────────

class TestKnapsackSelection:
    def test_empty_candidates_returns_empty(self):
        selected, excluded = knapsack_select([], budget=4)
        assert selected == [] and excluded == []

    def test_flight_401_property_quiet_urgent_beats_loud_safe(self):
        """A widget that's always present with decent relevance but LOW
        urgency must lose its slot to a widget with no special relevance
        but genuinely near its boundary, once the budget is tight enough
        to force a choice between them."""
        loud_safe = {"id": "loud_safe", "cost": 1, "score": salience(urgency=0.1, relevance=0.8)}
        quiet_urgent = {"id": "quiet_urgent", "cost": 1, "score": salience(urgency=0.95, relevance=0.3)}
        selected, excluded = knapsack_select([loud_safe, quiet_urgent], budget=1)
        assert [s["id"] for s in selected] == ["quiet_urgent"]
        assert [e["id"] for e in excluded] == ["loud_safe"]

    def test_selection_never_exceeds_budget_cost(self):
        candidates = [{"id": f"w{i}", "cost": 2, "score": 1.0} for i in range(5)]
        selected, _ = knapsack_select(candidates, budget=4)
        assert sum(c["cost"] for c in selected) <= 4

    def test_higher_total_score_combination_wins_over_single_high_scorer(self):
        # one expensive high scorer vs two cheap slightly-lower scorers that
        # together outscore it and fit the same budget
        expensive = {"id": "expensive", "cost": 3, "score": 0.9}
        cheap_a = {"id": "cheap_a", "cost": 1, "score": 0.5}
        cheap_b = {"id": "cheap_b", "cost": 2, "score": 0.5}
        selected, _ = knapsack_select([expensive, cheap_a, cheap_b], budget=3)
        ids = {s["id"] for s in selected}
        # cheap_a + cheap_b = cost 3, score 1.0 > expensive's score 0.9 at cost 3
        assert ids == {"cheap_a", "cheap_b"}


# ── compose(r) against a real, live sustain ─────────────────────────────────

class TestComposeReadOnly:
    def _make_homestead(self, engine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        return sid

    def test_compose_unknown_sustain(self, engine):
        result = compose(engine, "does-not-exist")
        assert result["selected"] == []
        assert "error" in result

    def test_compose_on_sustain_with_no_declared_widgets_is_honest_empty(self, engine):
        # habitat.json declares no curated_widgets (this slice is homestead-only)
        sid = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Test Habitat"})
        result = compose(engine, sid)
        assert result["selected"] == []
        assert "no curated widgets declared" in result["message"]

    def test_compose_never_mutates_state_or_events(self, engine):
        import asyncio

        sid = self._make_homestead(engine)
        asyncio.run(engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "test", "frequency": "once"}))
        asyncio.run(engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 4900, "period": "monthly"}))
        asyncio.run(engine.execute_operator(sid, "budget.spend", {"pocket_name": "food", "amount": 4900, "description": "near limit", "category": "test"}))

        state_before = engine.get_state(sid)
        events_before = engine.get_events(sid, limit=100)

        compose(engine, sid, query=None, device="phone", budget=4)
        compose(engine, sid, query="pocket", device="phone", budget=2)

        state_after = engine.get_state(sid)
        events_after = engine.get_events(sid, limit=100)

        assert state_before == state_after
        assert events_before == events_after
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    def test_compose_surfaces_near_limit_pocket_over_baseline_rollup_under_tight_budget(self, engine):
        """The end-to-end Flight 401 proof: a real pocket pushed to 100%
        spent must win a slot over the always-present rollup summary when
        the budget only allows one."""
        import asyncio

        sid = self._make_homestead(engine)
        asyncio.run(engine.execute_operator(sid, "budget.record_income", {"amount": 1000, "source": "test", "frequency": "once"}))
        asyncio.run(engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 1000, "period": "monthly"}))
        asyncio.run(engine.execute_operator(sid, "budget.spend", {"pocket_name": "food", "amount": 1000, "description": "spend it all", "category": "test"}))

        result = compose(engine, sid, budget=1)
        assert len(result["selected"]) == 1
        assert result["selected"][0]["id"] == "pocket_spent_watch"
        assert result["selected"][0]["urgency"] == pytest.approx(1.0)
        assert "why" in result["selected"][0] and result["selected"][0]["why"]

    def test_compose_result_is_json_serializable(self, engine):
        import asyncio

        sid = self._make_homestead(engine)
        asyncio.run(engine.execute_operator(sid, "budget.record_income", {"amount": 1000, "source": "test", "frequency": "once"}))
        asyncio.run(engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 500, "period": "monthly"}))
        asyncio.run(engine.execute_operator(sid, "budget.spend", {"pocket_name": "food", "amount": 480, "description": "x", "category": "test"}))
        result = compose(engine, sid)
        json.dumps(result)  # must not raise
