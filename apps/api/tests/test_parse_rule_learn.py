"""
tests/test_parse_rule_learn.py

"User fixes once -> it learns" (2 Aug 2026) — synthesize_rule_from_correction.
See parse_rule_learn.py's own module docstring for the honest scope
statement: deterministic template synthesis from ONE human-confirmed
example, not an LLM, not the §4G.7 proposer. Tests prove: it generalises
correctly to a same-template/different-amount message, stays appropriately
narrow against a differently-worded one, and follows the exact
mapped-for-income/parsed_unmapped-for-spend asymmetry the shipped seed
rules already establish.
"""

import sustena.operators  # noqa: F401 -- OPERATOR_REGISTRY population for typecheck
from sustena.core.parse_rule import apply_rule, typecheck_rule
from sustena.core.parse_rule_learn import synthesize_rule_from_correction

UNPARSED_SPEND_TEXT = (
    "XYZ9K2P7QR Confirmed. Ksh650.00 sent to KENYA POWER PREPAID via new mobile "
    "top-up channel on 2/8/26 at 9:00 AM. Ref: TOKEN2026."
)
UNPARSED_SPEND_TEXT_DIFFERENT_AMOUNT = UNPARSED_SPEND_TEXT.replace("XYZ9K2P7QR", "XYZ9K2P8QS").replace("650.00", "900.00")
UNPARSED_INCOME_TEXT = (
    "ABC1234567 Confirmed. You have been credited Ksh2,500.00 via a new bonus "
    "programme not previously seen on 2/8/26 at 10:00 AM."
)


class TestSynthesizeSpendCorrection:
    def test_synthesizes_a_parsed_unmapped_rule_for_a_spend_correction(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "amount": 650.0, "description": "KENYA POWER PREPAID", "category": ""},
        )
        assert rule is not None
        assert rule.status == "parsed_unmapped"
        assert rule.operator is None
        assert rule.trust == "user_corrected"
        assert rule.provenance == "human_correction"
        assert typecheck_rule(rule) == []

    def test_the_synthesized_rule_matches_its_own_inducing_example(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "amount": 650.0, "description": "KENYA POWER PREPAID", "category": ""},
        )
        result = apply_rule(rule, UNPARSED_SPEND_TEXT)
        assert result is not None
        assert result.parsed_fields["amount"] == 650.0
        assert result.parsed_fields["ref"] == "XYZ9K2P7QR"

    def test_the_synthesized_rule_generalises_to_the_same_template_with_a_different_amount(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "amount": 650.0, "description": "KENYA POWER PREPAID", "category": ""},
        )
        result = apply_rule(rule, UNPARSED_SPEND_TEXT_DIFFERENT_AMOUNT)
        assert result is not None, "the learned rule should recognise the SAME template with a different amount/ref"
        assert result.parsed_fields["amount"] == 900.0
        assert result.parsed_fields["ref"] == "XYZ9K2P8QS"

    def test_the_synthesized_rule_does_not_match_a_differently_worded_message(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "amount": 650.0, "description": "KENYA POWER PREPAID", "category": ""},
        )
        unrelated = "QGH7XJ2K9L Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 at 4:30 PM. New M-PESA balance is Ksh12,050.00"
        assert apply_rule(rule, unrelated) is None

    def test_a_spend_correction_never_binds_operator_or_params_since_not_mapped(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "amount": 650.0, "description": "KENYA POWER PREPAID", "category": ""},
        )
        assert rule.params == {}
        assert rule.reason_template is not None
        result = apply_rule(rule, UNPARSED_SPEND_TEXT)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None


class TestSynthesizeIncomeCorrection:
    def test_synthesizes_a_mapped_rule_for_an_income_correction(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_INCOME_TEXT, "budget.record_income",
            {"amount": 2500.0, "source": "M-Pesa: bonus", "frequency": "once"},
        )
        assert rule is not None
        assert rule.status == "mapped"
        assert rule.operator == "budget.record_income"
        assert typecheck_rule(rule) == []

    def test_mapped_rule_binds_amount_by_reference_and_keeps_other_params_literal(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_INCOME_TEXT, "budget.record_income",
            {"amount": 2500.0, "source": "M-Pesa: bonus", "frequency": "once"},
        )
        assert rule.params["amount"] == "$amount"
        assert rule.params["source"] == "M-Pesa: bonus"
        assert rule.params["frequency"] == "once"

    def test_the_mapped_rule_auto_resolves_operator_params_on_a_future_match(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_INCOME_TEXT, "budget.record_income",
            {"amount": 2500.0, "source": "M-Pesa: bonus", "frequency": "once"},
        )
        future_text = UNPARSED_INCOME_TEXT.replace("ABC1234567", "ABC7654321").replace("2,500.00", "3,000.00")
        result = apply_rule(rule, future_text)
        assert result is not None
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.operator_params["amount"] == 3000.0
        assert result.operator_params["source"] == "M-Pesa: bonus"  # literal, unchanged


class TestSynthesisRefusesWhenUnsafe:
    def test_returns_none_if_the_confirmed_amount_is_not_in_the_raw_text(self):
        rule = synthesize_rule_from_correction(
            "mpesa", "some message with no numbers relevant at all", "budget.spend",
            {"pocket_name": "food", "amount": 500.0, "description": "x", "category": ""},
        )
        assert rule is None

    def test_returns_none_if_no_amount_param_present(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "description": "KENYA POWER PREPAID"},
        )
        assert rule is None

    def test_returns_none_if_amount_is_not_numeric(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "amount": "not-a-number"},
        )
        assert rule is None


class TestRefExtraction:
    def test_ref_code_extracted_when_present(self):
        rule = synthesize_rule_from_correction(
            "mpesa", UNPARSED_SPEND_TEXT, "budget.spend",
            {"pocket_name": "electricity", "amount": 650.0, "description": "x", "category": ""},
        )
        assert "ref" in rule.extract
        result = apply_rule(rule, UNPARSED_SPEND_TEXT)
        assert result.external_ref == "XYZ9K2P7QR"

    def test_no_ref_extraction_when_text_has_no_leading_code(self):
        text = "you spent Ksh300.00 on something with no leading reference code at all"
        rule = synthesize_rule_from_correction(
            "mpesa", text, "budget.spend", {"pocket_name": "misc", "amount": 300.0},
        )
        assert rule is not None
        assert "ref" not in rule.extract
        result = apply_rule(rule, text)
        assert result is not None
        assert result.parsed_fields["amount"] == 300.0
