"""
tests/test_constraint_wildcard.py

E3 — ConstraintEngine must evaluate the `ALL <path>[*].<field> <op> <value>`
array/dict wildcard form. Previously only `ALL <path> FIELD <field> <pred>` over
a *list* was supported, so the homestead invariant
`ALL finances.pockets[*].allocated >= 0` (pockets is a DICT) always returned
"missing FIELD keyword" → permanent ✗ on Constraint Health.
"""

import pytest

from sustena.core.constraints import ConstraintEngine
from sustena.core.state import StateAccessor

POCKET_INV = "ALL finances.pockets[*].allocated >= 0"


def _ev(expr, state_dict):
    return ConstraintEngine().evaluate(expr, StateAccessor(state_dict))


def test_empty_pockets_passes_vacuously():
    ok, _ = _ev(POCKET_INV, {"finances": {"pockets": {}}})
    assert ok is True


def test_populated_dict_pockets_pass():
    ok, _ = _ev(POCKET_INV, {"finances": {"pockets": {
        "food": {"allocated": 30000, "spent": 10000},
        "rent": {"allocated": 20000, "spent": 0},
    }}})
    assert ok is True


def test_negative_pocket_fails():
    ok, reason = _ev(POCKET_INV, {"finances": {"pockets": {
        "bad": {"allocated": -5, "spent": 0},
    }}})
    assert ok is False
    assert "failed" in reason


def test_list_wildcard_passes():
    ok, _ = _ev("ALL inventory.items[*].qty >= 0",
                {"inventory": {"items": [{"qty": 3}, {"qty": 0}]}})
    assert ok is True


def test_list_wildcard_fails_on_negative():
    ok, _ = _ev("ALL inventory.items[*].qty >= 0",
                {"inventory": {"items": [{"qty": -1}]}})
    assert ok is False


def test_legacy_field_form_still_works():
    ok, _ = _ev("ALL finances.pockets FIELD allocated >= 0",
                {"finances": {"pockets": [{"allocated": 5}]}})
    assert ok is True


def test_scalar_constraint_unaffected():
    ok, _ = _ev("finances.liquid.balance >= 0",
                {"finances": {"liquid": {"balance": 70000}}})
    assert ok is True


def test_aggregate_block_form_raises_rather_than_reporting_a_violation():
    """
    SUM(...)/block forms belong to predicates.py, not to this guard evaluator.

    This previously asserted that an unsupported expression returns
    (False, "unsupported"). That was the bug, not the contract: "I cannot parse
    this" and "this rule is violated" are different answers, and returning the
    second for the first is what made aggregate invariants display as FAILING
    on screen while the enforcement gate — which uses predicates.py — held the
    very same rules to be satisfied.

    The case below shows how perverse the old answer was: an EMPTY journal is
    trivially balanced, yet it was reported as a violation.

    The original intent (don't crash the caller) is preserved: this raises a
    typed, catchable ConstraintParseError, not an arbitrary exception.
    """
    from sustena.core.constraints import ConstraintParseError

    expr = "ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)"
    with pytest.raises(ConstraintParseError, match="aggregate"):
        _ev(expr, {"accounts": {"journal_entries": []}})


def test_predicates_is_the_evaluator_that_understands_aggregates():
    """The other half of the contract: the supported path genuinely works."""
    from sustena.core.predicates import compile_invariant, evaluate_predicate

    expr = "ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)"
    node, errors = compile_invariant(expr, None)
    assert not errors, f"predicates.py must compile the aggregate grammar: {errors}"

    balanced = {"accounts": {"journal_entries": [
        {"lines": [{"debit": 100, "credit": 0}, {"debit": 0, "credit": 100}]}
    ]}}
    ok, _ = evaluate_predicate(node, StateAccessor(balanced), {})
    assert ok is True

    unbalanced = {"accounts": {"journal_entries": [
        {"lines": [{"debit": 100, "credit": 0}, {"debit": 0, "credit": 42}]}
    ]}}
    ok, _ = evaluate_predicate(node, StateAccessor(unbalanced), {})
    assert ok is False
