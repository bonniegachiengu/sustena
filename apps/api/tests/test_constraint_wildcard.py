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


def test_aggregate_block_form_is_handled_gracefully():
    # SUM(...)/block forms are not supported — must return (False, reason), not crash.
    ok, reason = _ev(
        "ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)",
        {"accounts": {"journal_entries": []}},
    )
    assert ok is False
    assert "unsupported" in reason
