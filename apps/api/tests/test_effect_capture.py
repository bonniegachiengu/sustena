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
    extract_currency_amount,
    infer,
    match_pocket_names,
    missing_required_params,
    narrow_by_verb,
    required_params_satisfiable,
    resolve_description,
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
    def test_no_amount_recoverable_asks_instead_of_dead_ending(self):
        # Was status="cannot_infer" (a real dead end reported live by
        # Bonnie: pick a pocket, then hit "no amount could be recovered"
        # with nowhere to go). Now asks for it directly instead.
        r = infer(CANDIDATES, STATE, effect_text="spent something on WiFi")
        assert r.status == "needs_disambiguation"
        assert r.field == "amount"
        assert r.options is None
        assert "amount" in r.why

    def test_no_candidate_operators_at_all(self):
        r = infer([], STATE, effect_text="spent 500 on WiFi")
        assert r.status == "cannot_infer"

    def test_operator_choice_not_among_real_candidates_is_rejected(self):
        r = infer(CANDIDATES, STATE, effect_text="500 WiFi", known={"operator": "budget.record_income"})
        assert r.status == "cannot_infer"


class TestResolveDescription:
    def test_known_wins_outright(self):
        assert resolve_description("ignored", {"counterparty": "ignored too"}, {"description": "explicit"}) == "explicit"

    def test_falls_back_to_parsed_counterparty(self):
        assert resolve_description(None, {"counterparty": "NAIVAS"}, None) == "NAIVAS"

    def test_falls_back_to_external_ref_when_no_counterparty(self):
        assert resolve_description(None, {"external_ref": "ABC123"}, None) == "ABC123"

    def test_falls_back_to_effect_text_when_nothing_parsed(self):
        assert resolve_description("spent 500 on WiFi", {}, None) == "spent 500 on WiFi"

    def test_nothing_available_returns_none(self):
        assert resolve_description(None, None, None) is None


class TestInferClassificationHistory:
    """Classification history ("purchase templates") -- a repeat counterparty
    pre-fills the likely pocket, saving the disambiguation tap. See
    effect_capture.infer()'s own docstring: this never skips CONFIRM."""

    def test_history_pre_fills_pocket_when_text_has_no_match(self):
        r = infer(
            CANDIDATES, STATE,
            parsed_fields={"amount": 680, "counterparty": "NAIVAS SUPERMARKET", "direction": "sent"},
            history={"pocket_name": "food", "operator_name": "budget.spend", "use_count": 3},
        )
        assert r.status == "ready"
        assert r.params["pocket_name"] == "food"
        assert r.from_history is True
        assert r.history_use_count == 3
        assert "usual" in r.why.lower()

    def test_history_never_skips_the_confirm_step(self):
        # 'ready' is a proposal, not a write -- the caller must still POST
        # /orchie/capture/confirm explicitly. Nothing about status=ready
        # changes when it came from history vs. a text match.
        r = infer(
            CANDIDATES, STATE,
            parsed_fields={"amount": 680, "counterparty": "NAIVAS SUPERMARKET", "direction": "sent"},
            history={"pocket_name": "food", "operator_name": "budget.spend", "use_count": 3},
        )
        assert r.status == "ready"  # proposal only -- no operator has been called

    def test_history_naming_a_pocket_that_no_longer_exists_is_ignored(self):
        # The pocket may have been renamed/removed since the history row was
        # written -- a stale mapping must fall through to the normal ask,
        # never silently point at a dead pocket.
        r = infer(
            CANDIDATES, STATE,
            parsed_fields={"amount": 680, "counterparty": "NAIVAS SUPERMARKET", "direction": "sent"},
            history={"pocket_name": "groceries-old", "operator_name": "budget.spend", "use_count": 3},
        )
        assert r.status == "needs_disambiguation"
        assert r.field == "pocket_name"

    def test_no_history_falls_through_to_normal_disambiguation(self):
        r = infer(
            CANDIDATES, STATE,
            parsed_fields={"amount": 680, "counterparty": "NAIVAS SUPERMARKET", "direction": "sent"},
            history=None,
        )
        assert r.status == "needs_disambiguation"

    def test_explicit_text_match_wins_over_history(self):
        # An unambiguous pocket name actually present in the text is a
        # stronger signal than a remembered mapping for a DIFFERENT past
        # transaction from the same counterparty -- text match is checked
        # first and short-circuits before history is even consulted.
        r = infer(
            CANDIDATES, STATE, effect_text="spent 500 on WiFi",
            history={"pocket_name": "food", "operator_name": "budget.spend", "use_count": 5},
        )
        assert r.status == "ready"
        assert r.params["pocket_name"] == "WiFi"
        assert r.from_history is False

    def test_a_regular_ready_result_reports_from_history_false(self):
        r = infer(CANDIDATES, STATE, effect_text="spent 500 on WiFi")
        assert r.from_history is False
        assert r.history_use_count is None

    def test_ready_result_carries_the_resolved_description(self):
        r = infer(
            CANDIDATES, STATE,
            parsed_fields={"amount": 450, "counterparty": "NAIVAS SUPERMARKET", "direction": "sent"},
            known={"pocket_name": "food"},
        )
        assert r.description == "NAIVAS SUPERMARKET"


class TestExtractCurrencyAmount:
    def test_ksh_prefixed_amount(self):
        assert extract_currency_amount("Ksh450.00 paid to SOMEWHERE on 20/7/26") == 450.0

    def test_kes_prefixed_amount_with_space(self):
        assert extract_currency_amount("KES 1,350.00 transaction made on card 4243") == 1350.0

    def test_case_insensitive(self):
        assert extract_currency_amount("ksh 99.50 spent") == 99.5

    def test_ignores_bare_digits_with_no_currency_prefix(self):
        # The whole reason this exists instead of reusing extract_amount():
        # a raw SMS body is full of digits (dates, refs, phone fragments)
        # that are NOT the transaction amount.
        assert extract_currency_amount("Ref 254712345678 dated 20/07/2026") is None

    def test_picks_the_currency_prefixed_figure_not_an_earlier_bare_number(self):
        # A reference code with embedded digits appears BEFORE the real,
        # currency-prefixed amount -- must not be picked up instead.
        text = "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 at 4:30 PM."
        assert extract_currency_amount(text) == 450.0

    def test_no_currency_marker_anywhere_returns_none(self):
        assert extract_currency_amount("hey are we still on for lunch tomorrow?") is None

    def test_empty_text_returns_none(self):
        assert extract_currency_amount("") is None
        assert extract_currency_amount(None) is None


class TestInferRawTextAmountFallback:
    """The actual bug Bonnie hit live: a message no registered transducer
    parser recognised (parsed_fields={}, effect_text=None) previously had
    NO way to recover an amount at all. raw_text is the fix -- the full SMS
    body, searched with the safer currency-prefixed regex."""

    UNPARSED_KCB_LIKE_SMS = "Dear customer, Ksh680.00 has been debited from your account XYZ on 21/07/2026."

    def test_amount_recovered_from_raw_text_when_nothing_else_has_it(self):
        # operator explicit so this isolates amount-recovery from the
        # separate spend-vs-allocate disambiguation (both are otherwise
        # equally satisfiable with no verb/direction hint present).
        r = infer(
            CANDIDATES, STATE,
            known={"pocket_name": "food", "operator": "budget.spend"},  # pocket+operator resolved -- only amount is missing
            raw_text=self.UNPARSED_KCB_LIKE_SMS,
        )
        assert r.status == "ready"
        assert r.params["amount"] == 680.0

    def test_effect_text_and_parsed_fields_still_win_over_raw_text(self):
        r = infer(
            CANDIDATES, STATE,
            known={"pocket_name": "food", "operator": "budget.spend"},
            parsed_fields={"amount": 100.0},
            raw_text=self.UNPARSED_KCB_LIKE_SMS,  # would give 680.0 if consulted
        )
        assert r.params["amount"] == 100.0

    def test_raw_text_with_no_currency_marker_falls_through_to_asking(self):
        r = infer(
            CANDIDATES, STATE,
            known={"pocket_name": "food"},
            raw_text="Your account was accessed from a new device.",
        )
        assert r.status == "needs_disambiguation"
        assert r.field == "amount"

    def test_full_flow_pocket_then_typed_amount_reaches_ready(self):
        # Simulates the frontend's CaptureFlow: pocket disambiguation first
        # (no text/history match), then the human types an amount as a
        # string (as a text input naturally would send), then ready.
        r1 = infer(CANDIDATES, STATE, raw_text="a message with no recognisable shape at all")
        assert r1.status == "needs_disambiguation"
        assert r1.field == "pocket_name"

        r2 = infer(
            CANDIDATES, STATE, known={"pocket_name": "food"},
            raw_text="a message with no recognisable shape at all",
        )
        assert r2.status == "needs_disambiguation"
        assert r2.field == "amount"

        # Explicit operator choice isolates the amount-typing behaviour from
        # the separate spend-vs-allocate disambiguation (both are otherwise
        # equally satisfiable once pocket+amount are known, with no verb/
        # direction hint to narrow between them -- a real, expected step,
        # not part of what this test is verifying).
        r3 = infer(
            CANDIDATES, STATE,
            known={"pocket_name": "food", "amount": "500", "operator": "budget.spend"},  # typed, arrives as a string
            raw_text="a message with no recognisable shape at all",
        )
        assert r3.status == "ready"
        assert r3.params["amount"] == 500.0
        assert isinstance(r3.params["amount"], float)  # coerced, never left as a string

    def test_unparseable_typed_amount_is_treated_as_not_provided(self):
        r = infer(CANDIDATES, STATE, known={"pocket_name": "food", "amount": "not a number"})
        assert r.status == "needs_disambiguation"
        assert r.field == "amount"
