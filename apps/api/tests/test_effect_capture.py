"""
tests/test_effect_capture.py

Tests for sustena/core/effect_capture.py -- the §7 inference engine
(epsilon -> (o, theta)). Pure-function tests, no engine/DB involved.

Run with:
    python -m pytest tests/test_effect_capture.py -v
"""

import pytest

import sustena.operators  # noqa: F401 - registers OPERATOR_REGISTRY
from sustena.core.effect_capture import (
    build_params,
    extract_amount,
    infer,
    match_pocket_names,
    missing_required_params,
    narrow_by_verb,
    required_params_satisfiable,
)

STATE = {
    "finances": {
        "pockets": {
            "food": {"allocated": 9000, "spent": 2250},
            "WiFi": {"allocated": 3000, "spent": 3000},
            "rent": {"allocated": 17000, "spent": 15000},
        }
    }
}

CANDIDATES = ["budget.spend", "budget.allocate"]


class TestExtractAmount:
    def test_plain_number(self):
        assert extract_amount("spent 500 on WiFi") == 500.0

    def test_number_with_commas(self):
        assert extract_amount("paid 12,500 for rent") == 12500.0

    def test_decimal(self):
        assert extract_amount("bought 45.50 worth of snacks") == 45.5

    def test_no_number_returns_none(self):
        assert extract_amount("no amount here") is None

    def test_empty_text_returns_none(self):
        assert extract_amount("") is None


class TestMatchPocketNames:
    def test_single_match(self):
        assert match_pocket_names("spent 500 on WiFi", STATE) == ["WiFi"]

    def test_case_insensitive(self):
        assert match_pocket_names("spent 500 on wifi", STATE) == ["WiFi"]

    def test_no_match(self):
        assert match_pocket_names("spent 500 on snacks", STATE) == []

    def test_multiple_matches(self):
        matches = match_pocket_names("food and rent this month", STATE)
        assert set(matches) == {"food", "rent"}

    def test_empty_text(self):
        assert match_pocket_names("", STATE) == []


class TestNarrowByVerb:
    def test_spend_verb(self):
        assert narrow_by_verb("spent 500 on WiFi", CANDIDATES) == ["budget.spend"]

    def test_allocate_verb(self):
        assert narrow_by_verb("allocate 500 to food", CANDIDATES) == ["budget.allocate"]

    def test_no_verb_returns_empty_not_all(self):
        assert narrow_by_verb("500 WiFi", CANDIDATES) == []


class TestParamIntrospection:
    def test_spend_requires_pocket_and_amount(self):
        assert required_params_satisfiable("budget.spend", {}) is False
        assert required_params_satisfiable("budget.spend", {"pocket_name": "food", "amount": 100}) is True

    def test_spend_description_and_category_optional(self):
        # required-only check: description/category have defaults, so they
        # must NOT be required for satisfiability
        assert required_params_satisfiable("budget.spend", {"pocket_name": "food", "amount": 100}) is True

    def test_missing_required_params_lists_exactly_whats_missing(self):
        missing = missing_required_params("budget.spend", {"pocket_name": "food"})
        assert missing == ["amount"]

    def test_build_params_only_pulls_real_signature_fields(self):
        facts = {"pocket_name": "food", "amount": 100, "operator": "budget.spend", "junk": "x"}
        params = build_params("budget.spend", facts)
        assert params == {"pocket_name": "food", "amount": 100}


class TestInferReady:
    def test_narration_resolves_pocket_amount_and_operator(self):
        r = infer(CANDIDATES, STATE, effect_text="spent 500 on WiFi")
        assert r.status == "ready"
        assert r.operator == "budget.spend"
        assert r.params["pocket_name"] == "WiFi"
        assert r.params["amount"] == 500.0
        # theta was recovered, not asked of the user
        assert r.why  # a real, non-empty explanation

    def test_ingest_message_with_known_pocket_recovers_amount_and_description_from_message(self):
        """The core §7 claim: the user only supplied the pocket (one tap);
        amount and description came from the message, not from typing."""
        r = infer(
            CANDIDATES, STATE,
            parsed_fields={"amount": 450, "counterparty": "NAIVAS SUPERMARKET", "direction": "sent"},
            known={"pocket_name": "food"},
        )
        assert r.status == "ready"
        assert r.operator == "budget.spend"
        assert r.params == {"pocket_name": "food", "amount": 450, "description": "NAIVAS SUPERMARKET"}

    def test_direction_sent_hints_toward_spend_over_allocate(self):
        r = infer(
            CANDIDATES, STATE,
            parsed_fields={"amount": 100, "counterparty": "x", "direction": "sent"},
            known={"pocket_name": "food"},
        )
        assert r.status == "ready"
        assert r.operator == "budget.spend"

    def test_explicit_prior_operator_choice_is_honored(self):
        """Second round of a disambiguation: the human already answered
        'what should this do?' -- that answer must win outright."""
        r = infer(
            CANDIDATES, STATE, effect_text="500 WiFi",
            known={"operator": "budget.allocate"},
        )
        assert r.status == "ready"
        assert r.operator == "budget.allocate"
        assert r.params["pocket_name"] == "WiFi"


class TestInferNeedsDisambiguation:
    def test_unmatched_pocket_asks_with_real_pocket_options(self):
        r = infer(CANDIDATES, STATE, effect_text="spent 500 on snacks")
        assert r.status == "needs_disambiguation"
        assert r.field == "pocket_name"
        values = {o["value"] for o in r.options}
        assert values == {"food", "WiFi", "rent"}  # the sustain's OWN real pockets, nothing invented

    def test_ambiguous_multiple_pocket_matches_offers_only_the_matches(self):
        r = infer(CANDIDATES, STATE, effect_text="food and rent this month, 500")
        assert r.status == "needs_disambiguation"
        assert r.field == "pocket_name"
        values = {o["value"] for o in r.options}
        assert values == {"food", "rent"}

    def test_ambiguous_operator_after_pocket_resolved(self):
        r = infer(CANDIDATES, STATE, effect_text="WiFi 500")  # no verb hint
        assert r.status == "needs_disambiguation"
        assert r.field == "operator"
        values = {o["value"] for o in r.options}
        assert values == {"budget.spend", "budget.allocate"}
        # labels are humanized, not raw operator names
        assert all(o["label"] != o["value"] for o in r.options)

    def test_disambiguation_options_are_uniformly_shaped(self):
        r = infer(CANDIDATES, STATE, effect_text="spent 500 on snacks")
        for opt in r.options:
            assert set(opt.keys()) == {"value", "label"}

    def test_no_pockets_declared_is_cannot_infer_not_a_forced_choice(self):
        r = infer(CANDIDATES, {"finances": {"pockets": {}}}, effect_text="spent 500 on nothing")
        assert r.status == "cannot_infer"


class TestInferCannotInfer:
    def test_no_amount_recoverable(self):
        r = infer(CANDIDATES, STATE, effect_text="spent something on WiFi")
        assert r.status == "cannot_infer"
        assert "amount" in r.why

    def test_no_candidate_operators_at_all(self):
        r = infer([], STATE, effect_text="spent 500 on WiFi")
        assert r.status == "cannot_infer"

    def test_operator_choice_not_among_real_candidates_is_rejected(self):
        r = infer(CANDIDATES, STATE, effect_text="500 WiFi", known={"operator": "budget.record_income"})
        assert r.status == "cannot_infer"
