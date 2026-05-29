"""
tests/test_operator_registry.py

Validates the global OPERATOR_REGISTRY for all registered operators.

Epic 1.1 milestone criterion:
  "All operators have pawa_cost, license_tier, author, side_effects metadata"

This test module is the canonical registry contract — if an operator is
registered, it must meet all metadata requirements. Add new operator modules
to the imports below as they are built.

Run with: pytest tests/test_operator_registry.py -v
"""

import pytest

from sustena.core.operator import OPERATOR_REGISTRY
import sustena.operators  # triggers registration of all operator modules


# ── Registry completeness ─────────────────────────────────────────────────────

BUDGET_OPERATORS = [
    "budget.record_income",
    "budget.allocate",
    "budget.spend",
    "budget.transfer",
    "budget.summary",
]

ALL_KNOWN_OPERATORS = BUDGET_OPERATORS  # extend as epics ship


class TestRegistryCompleteness:

    def test_all_budget_operators_present(self):
        """All five Epic 1.1 budget operators must be in the registry."""
        missing = [op for op in BUDGET_OPERATORS if op not in OPERATOR_REGISTRY]
        assert not missing, f"Missing from OPERATOR_REGISTRY: {missing}"

    def test_no_unknown_operators_present(self):
        """Registry should only contain operators we've deliberately shipped."""
        registered = set(OPERATOR_REGISTRY.keys())
        unknown = registered - set(ALL_KNOWN_OPERATORS)
        assert not unknown, (
            f"Unexpected operators in registry: {unknown}. "
            "If you added a new operator, add it to ALL_KNOWN_OPERATORS in this file."
        )


# ── Metadata contract ─────────────────────────────────────────────────────────

class TestOperatorMetadata:

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_name_is_namespaced(self, name):
        """Operator name must contain at least one dot: 'domain.action'."""
        assert "." in name, f"'{name}' is not namespaced"

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_description_not_empty(self, name):
        meta = OPERATOR_REGISTRY[name]
        assert meta.description and len(meta.description) > 10, (
            f"'{name}' description is missing or too short"
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_author_set(self, name):
        meta = OPERATOR_REGISTRY[name]
        assert meta.author, f"'{name}' has no author"

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_pawa_cost_is_non_negative_int(self, name):
        meta = OPERATOR_REGISTRY[name]
        assert isinstance(meta.pawa_cost, int), (
            f"'{name}' pawa_cost must be int, got {type(meta.pawa_cost).__name__}"
        )
        assert meta.pawa_cost >= 0, f"'{name}' pawa_cost must be >= 0"

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_license_tier_valid(self, name):
        meta = OPERATOR_REGISTRY[name]
        valid_tiers = {"free", "per_use", "subscription", "one_time"}
        assert meta.license_tier in valid_tiers, (
            f"'{name}' license_tier '{meta.license_tier}' is not one of {valid_tiers}"
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_side_effects_is_list(self, name):
        meta = OPERATOR_REGISTRY[name]
        assert isinstance(meta.side_effects, list), (
            f"'{name}' side_effects must be a list"
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_side_effects_follow_event_dot_protocol(self, name):
        """Every declared side effect must be a valid event name (event.<domain>.<type>)."""
        import re
        pattern = re.compile(r"^event(\.[a-z][a-z0-9_]*){2,}$")
        meta = OPERATOR_REGISTRY[name]
        for effect in meta.side_effects:
            assert pattern.match(effect), (
                f"'{name}' side_effect '{effect}' does not follow event dot-protocol "
                "(must match: event.<domain>.<type>, lowercase snake_case)"
            )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_constraints_is_list(self, name):
        meta = OPERATOR_REGISTRY[name]
        assert isinstance(meta.constraints, list), (
            f"'{name}' constraints must be a list"
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_ui_schema_present(self, name):
        """Every operator must declare a ui_schema for card rendering."""
        meta = OPERATOR_REGISTRY[name]
        assert isinstance(meta.ui_schema, dict), (
            f"'{name}' ui_schema must be a dict"
        )
        assert meta.ui_schema, f"'{name}' ui_schema must not be empty"
        assert "widget_type" in meta.ui_schema, (
            f"'{name}' ui_schema missing required 'widget_type' key"
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_fn_is_callable(self, name):
        """Registered function must be callable (async function)."""
        import asyncio
        meta = OPERATOR_REGISTRY[name]
        assert callable(meta.fn), f"'{name}' fn is not callable"
        assert asyncio.iscoroutinefunction(meta.fn), (
            f"'{name}' fn must be an async function"
        )


# ── Constraint correctness ────────────────────────────────────────────────────

class TestConstraintCorrectness:
    """
    Spot-check that declared constraints are syntactically valid and evaluate
    correctly against known state snapshots via ConstraintEngine.
    """

    def _engine(self):
        from sustena.core.constraints import ConstraintEngine
        return ConstraintEngine()

    def _state(self, balance: float = 50000.0):
        from sustena.core.state import StateAccessor
        return StateAccessor({
            "finances": {"liquid": {"balance": balance}, "pockets": {}}
        })

    def test_record_income_constraint_passes_positive_amount(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.record_income"]
        ok, _ = engine.evaluate_all(meta.constraints, self._state(), params={"amount": 1000.0})
        assert ok

    def test_record_income_constraint_fails_zero_amount(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.record_income"]
        ok, reason = engine.evaluate_all(meta.constraints, self._state(), params={"amount": 0.0})
        assert not ok
        assert reason

    def test_allocate_constraint_passes_when_liquid_sufficient(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.allocate"]
        ok, _ = engine.evaluate_all(meta.constraints, self._state(50000.0), params={"amount": 5000.0})
        assert ok

    def test_allocate_constraint_fails_when_liquid_insufficient(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.allocate"]
        ok, reason = engine.evaluate_all(
            meta.constraints, self._state(1000.0), params={"amount": 5000.0}
        )
        assert not ok
        assert reason

    def test_allocate_constraint_fails_on_exact_zero(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.allocate"]
        ok, _ = engine.evaluate_all(
            meta.constraints, self._state(50000.0), params={"amount": 0.0}
        )
        assert not ok

    def test_spend_constraint_passes_positive_amount(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.spend"]
        ok, _ = engine.evaluate_all(meta.constraints, self._state(), params={"amount": 500.0})
        assert ok

    def test_spend_constraint_fails_zero_amount(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.spend"]
        ok, _ = engine.evaluate_all(meta.constraints, self._state(), params={"amount": 0.0})
        assert not ok

    def test_transfer_constraint_fails_zero_amount(self):
        engine = self._engine()
        meta = OPERATOR_REGISTRY["budget.transfer"]
        ok, _ = engine.evaluate_all(meta.constraints, self._state(), params={"amount": 0.0})
        assert not ok

    def test_summary_has_no_constraints(self):
        """budget.summary is read-only — no pre-conditions required."""
        meta = OPERATOR_REGISTRY["budget.summary"]
        assert meta.constraints == []


# ── Budget operator specific contract ────────────────────────────────────────

class TestBudgetOperatorContract:

    def test_all_budget_operators_are_free_tier(self):
        """All core budget operators are free (license_tier='free')."""
        for name in BUDGET_OPERATORS:
            assert OPERATOR_REGISTRY[name].license_tier == "free", (
                f"'{name}' should be 'free' tier in Epic 1.1"
            )

    def test_all_budget_operators_have_zero_pawa_cost(self):
        """Core budget operators cost 0 pawa — financial primitives are free."""
        for name in BUDGET_OPERATORS:
            assert OPERATOR_REGISTRY[name].pawa_cost == 0, (
                f"'{name}' should cost 0 pawa in Epic 1.1"
            )

    def test_all_budget_operators_authored_by_sustena_core(self):
        for name in BUDGET_OPERATORS:
            assert OPERATOR_REGISTRY[name].author == "sustena_core", (
                f"'{name}' author should be 'sustena_core'"
            )

    def test_read_only_operator_fires_no_events(self):
        """budget.summary declares no side_effects — it's a pure read."""
        assert OPERATOR_REGISTRY["budget.summary"].side_effects == []

    def test_mutating_operators_declare_events(self):
        expected = {
            "budget.record_income": "event.finances.income_received",
            "budget.allocate":      "event.finances.pocket_allocated",
            "budget.spend":         "event.finances.pocket_spent",
            "budget.transfer":      "event.finances.pocket_transfer",
        }
        for name, event in expected.items():
            assert event in OPERATOR_REGISTRY[name].side_effects, (
                f"'{name}' must declare '{event}' in side_effects"
            )

    def test_allocate_has_liquid_balance_constraint(self):
        """budget.allocate must protect against over-drawing liquid."""
        constraints = OPERATOR_REGISTRY["budget.allocate"].constraints
        assert any("finances.liquid.balance" in c for c in constraints), (
            "budget.allocate must have a constraint checking finances.liquid.balance"
        )

    def test_mutating_operators_have_constraints(self):
        """All state-mutating operators must declare at least one pre-condition."""
        for name in ["budget.record_income", "budget.allocate", "budget.spend", "budget.transfer"]:
            assert OPERATOR_REGISTRY[name].constraints, (
                f"'{name}' must declare at least one pre-condition constraint"
            )
