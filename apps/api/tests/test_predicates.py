"""
tests/test_predicates.py

Typed predicate AST — parser, schema binder, evaluator (sustena/core/predicates.py).
"""

import json
from pathlib import Path

import pytest

from sustena.core.predicates import (
    PredicateSyntaxError,
    compile_invariant,
    evaluate_predicate,
    parse_predicate,
    validate_against_schema,
)
from sustena.core.state import StateAccessor

_SUSTAINS_DIR = Path(__file__).parent.parent / "sustena" / "sustains"

HOMESTEAD_SCHEMA = {
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

BIASHARA_ACCOUNTS_SCHEMA = {
    "accounts": {
        "type": "object",
        "properties": {
            "journal_entries": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "lines": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {"debit": {"type": "number"}, "credit": {"type": "number"}},
                            },
                        }
                    },
                },
            }
        },
    },
    "inventory": {
        "type": "object",
        "properties": {
            "items": {
                "type": "object",
                "additionalProperties": {"type": "object", "properties": {"qty": {"type": "number"}}},
            }
        },
    },
}


class TestParsing:
    def test_simple_comparison(self):
        node = parse_predicate("finances.liquid.balance >= 0")
        assert node.op == ">="

    def test_and_or_not(self):
        parse_predicate("a > 0 AND b > 0")
        parse_predicate("a > 0 OR b > 0")
        parse_predicate("NOT a == 0")

    def test_parenthesised(self):
        parse_predicate("(a > 0 AND b > 0) OR c == 1")

    def test_quantifier_simple_field_form(self):
        node = parse_predicate("ALL finances.pockets[*].allocated >= 0")
        assert node.kind == "ALL"
        assert node.list_path.raw == "finances.pockets"

    def test_quantifier_block_form_with_sum_aggregate(self):
        node = parse_predicate(
            "ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)"
        )
        assert node.kind == "ALL"

    def test_exists_quantifier(self):
        node = parse_predicate("EXISTS staff.roster[*].status == \"active\"")
        assert node.kind == "EXISTS"

    def test_in_and_not_in(self):
        parse_predicate('status IN ["pending", "confirmed"]')
        parse_predicate('status NOT IN ["cancelled"]')

    def test_quantifier_in_tail(self):
        parse_predicate("ALL fines[*].reason IN rules.fine_reasons")

    def test_params_ref(self):
        node = parse_predicate("params.amount > 0")
        assert node.left.key == "amount"

    def test_bad_syntax_raises_with_position(self):
        with pytest.raises(PredicateSyntaxError):
            parse_predicate("ALL contribution: amount >= 1")  # missing '[*]'

    def test_arithmetic_not_supported_raises(self):
        with pytest.raises(PredicateSyntaxError):
            parse_predicate("amount <= rules.max_ratio * pool.balance")

    def test_dynamic_index_not_supported_raises(self):
        with pytest.raises(PredicateSyntaxError):
            parse_predicate("members[loan.member_id].status == \"x\"")

    def test_empty_expression_raises(self):
        with pytest.raises(PredicateSyntaxError):
            parse_predicate("   ")


class TestSchemaValidation:
    def test_valid_path_passes(self):
        node = parse_predicate("finances.liquid.balance >= 0")
        assert validate_against_schema(node, HOMESTEAD_SCHEMA) == []

    def test_missing_dimension_fails_loudly(self):
        node = parse_predicate("finances.liqiud.balance >= 0")  # typo'd dimension
        errors = validate_against_schema(node, HOMESTEAD_SCHEMA)
        assert len(errors) == 1
        assert "finances.liqiud.balance" in errors[0]

    def test_quantifier_over_dict_of_objects(self):
        node = parse_predicate("ALL finances.pockets[*].allocated >= 0")
        assert validate_against_schema(node, HOMESTEAD_SCHEMA) == []

    def test_quantifier_field_not_on_item_fails(self):
        node = parse_predicate("ALL finances.pockets[*].nonexistent_field >= 0")
        errors = validate_against_schema(node, HOMESTEAD_SCHEMA)
        assert errors

    def test_nested_sum_aggregate_over_array_of_arrays(self):
        node = parse_predicate(
            "ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)"
        )
        assert validate_against_schema(node, BIASHARA_ACCOUNTS_SCHEMA) == []

    def test_outer_state_reachable_from_inside_quantifier_body(self):
        schema = {
            **HOMESTEAD_SCHEMA,
            "fines": {"type": "array", "items": {"type": "object", "properties": {"reason": {"type": "string"}}}},
            "rules": {"type": "object", "properties": {"fine_reasons": {"type": "array", "items": {"type": "string"}}}},
        }
        node = parse_predicate("ALL fines[*].reason IN rules.fine_reasons")
        assert validate_against_schema(node, schema) == []

    def test_compile_invariant_returns_none_and_errors_on_failure(self):
        node, errors = compile_invariant("finances.nope.balance >= 0", HOMESTEAD_SCHEMA)
        assert node is None
        assert errors


class TestEvaluation:
    def test_simple_comparison_pass(self):
        node = parse_predicate("finances.liquid.balance >= 0")
        state = StateAccessor({"finances": {"liquid": {"balance": 500.0}}})
        ok, reason = evaluate_predicate(node, state)
        assert ok is True and reason == ""

    def test_simple_comparison_fail_has_legible_reason(self):
        node = parse_predicate("finances.liquid.balance >= 0")
        state = StateAccessor({"finances": {"liquid": {"balance": -50.0}}})
        ok, reason = evaluate_predicate(node, state)
        assert ok is False
        assert "finances.liquid.balance" in reason
        assert "-50" in reason

    def test_quantifier_all_pass(self):
        node = parse_predicate("ALL finances.pockets[*].allocated >= 0")
        state = StateAccessor({"finances": {"pockets": {"food": {"allocated": 100}, "rent": {"allocated": 0}}}})
        ok, _ = evaluate_predicate(node, state)
        assert ok is True

    def test_quantifier_all_fail_names_the_index(self):
        node = parse_predicate("ALL finances.pockets[*].allocated >= 0")
        state = StateAccessor({"finances": {"pockets": {"food": {"allocated": -5}}}})
        ok, reason = evaluate_predicate(node, state)
        assert ok is False
        assert "failed at index" in reason

    def test_quantifier_exists_pass(self):
        node = parse_predicate('EXISTS staff[*].status == "active"')
        state = StateAccessor({"staff": [{"status": "off"}, {"status": "active"}]})
        ok, _ = evaluate_predicate(node, state)
        assert ok is True

    def test_quantifier_exists_fail(self):
        node = parse_predicate('EXISTS staff[*].status == "active"')
        state = StateAccessor({"staff": [{"status": "off"}]})
        ok, _ = evaluate_predicate(node, state)
        assert ok is False

    def test_sum_aggregate_balanced_journal_passes(self):
        node = parse_predicate(
            "ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)"
        )
        state = StateAccessor({
            "accounts": {"journal_entries": [{"lines": [{"debit": 100, "credit": 0}, {"debit": 0, "credit": 100}]}]}
        })
        ok, _ = evaluate_predicate(node, state)
        assert ok is True

    def test_sum_aggregate_unbalanced_journal_fails(self):
        node = parse_predicate(
            "ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)"
        )
        state = StateAccessor({
            "accounts": {"journal_entries": [{"lines": [{"debit": 100, "credit": 0}, {"debit": 0, "credit": 50}]}]}
        })
        ok, reason = evaluate_predicate(node, state)
        assert ok is False
        assert "SUM(lines[*].debit)" in reason

    def test_outer_state_reachable_from_quantifier_body_at_eval_time(self):
        node = parse_predicate("ALL fines[*].reason IN rules.fine_reasons")
        state = StateAccessor({
            "fines": [{"reason": "late_contribution"}],
            "rules": {"fine_reasons": ["late_contribution", "absent_meeting"]},
        })
        ok, _ = evaluate_predicate(node, state)
        assert ok is True

    def test_outer_state_quantifier_body_fail_case(self):
        node = parse_predicate("ALL fines[*].reason IN rules.fine_reasons")
        state = StateAccessor({
            "fines": [{"reason": "made_up_reason"}],
            "rules": {"fine_reasons": ["late_contribution"]},
        })
        ok, reason = evaluate_predicate(node, state)
        assert ok is False
        assert "made_up_reason" in reason

    def test_and_or_not_combinators(self):
        node = parse_predicate("a > 0 AND b > 0")
        assert evaluate_predicate(node, StateAccessor({"a": 1, "b": 1}))[0] is True
        assert evaluate_predicate(node, StateAccessor({"a": 1, "b": -1}))[0] is False

        node = parse_predicate("a > 0 OR b > 0")
        assert evaluate_predicate(node, StateAccessor({"a": -1, "b": 1}))[0] is True
        assert evaluate_predicate(node, StateAccessor({"a": -1, "b": -1}))[0] is False

        node = parse_predicate('NOT status == "locked"')
        assert evaluate_predicate(node, StateAccessor({"status": "open"}))[0] is True
        assert evaluate_predicate(node, StateAccessor({"status": "locked"}))[0] is False

    def test_params_ref_evaluation(self):
        node = parse_predicate("params.amount > 0")
        ok, _ = evaluate_predicate(node, StateAccessor({}), params={"amount": 50})
        assert ok is True
        ok, _ = evaluate_predicate(node, StateAccessor({}), params={"amount": -50})
        assert ok is False

    def test_in_membership(self):
        node = parse_predicate('status IN ["pending", "confirmed"]')
        assert evaluate_predicate(node, StateAccessor({"status": "pending"}))[0] is True
        assert evaluate_predicate(node, StateAccessor({"status": "cancelled"}))[0] is False

    def test_no_eval_exec_compile_used(self):
        """Static guarantee: the module must never shell out to eval/exec/compile."""
        src = Path(__file__).parent.parent.joinpath("sustena", "core", "predicates.py").read_text(encoding="utf-8")
        code_lines = [ln for ln in src.splitlines() if not ln.strip().startswith(("#", '"', "No ", "*"))]
        code_only = "\n".join(code_lines)
        for banned in ("eval(", "exec(", "compile("):
            assert banned not in code_only, f"found forbidden call: {banned}"


class TestRealSustainSpecs:
    """
    Compile every declared invariant from every shipped sustain spec. This is
    the load-time validation Move 1 exists for — a predicate referencing a
    dimension the schema doesn't declare must fail here, not silently at
    runtime.

    biashara/vyyb/colosso were removed in the homestead-only prune, and chama
    followed in the same slice ("only homestead should remain") — their
    dedicated compile-error assertions went with them rather than being kept
    around to test files that no longer exist. Only homestead and habitat
    remain (plus habitat's siblings, if any are added later); both are
    expected fully clean.
    """

    @staticmethod
    def _load(name):
        with (_SUSTAINS_DIR / f"{name}.json").open(encoding="utf-8") as fh:
            return json.load(fh)

    def test_homestead_invariants_all_compile_clean(self):
        spec = self._load("homestead")
        for inv in spec["invariants"]:
            node, errors = compile_invariant(inv["expression"], spec["state_schema"])
            assert not errors, f"{inv['id']}: {errors}"

    def test_habitat_invariants_all_compile_clean(self):
        spec = self._load("habitat")
        for inv in spec["invariants"]:
            node, errors = compile_invariant(inv["expression"], spec["state_schema"])
            assert not errors, f"{inv['id']}: {errors}"
