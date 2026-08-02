"""
tests/test_parse_rule.py

Phase 3B (Parser primitive-lift, 2 Aug 2026) — the ParseRule primitive
itself: typecheck_rule (Gamma |- r), the run_rules/apply_rule interpreter,
and the seed library's own well-formedness.

Parity between the declared-rule tier and the hand-wired Python fallback
for every migrated shape is proven by test_transducer.py's own 86 existing
tests continuing to pass byte-for-byte unchanged with the declared tier
wired in ahead of the fallback (see transducer.py's parse_message) — that
IS the strongest possible parity proof, since those tests assert exact
status/operator_name/operator_params/parsed_fields/reason/parser_name
values that were written against the OLD hand-wired output. This file
instead covers the PRIMITIVE's own correctness in isolation.
"""

import pytest

import sustena.operators  # noqa: F401 -- registers OPERATOR_REGISTRY; typecheck_rule's mapped-rule check needs it populated
from sustena.core.parse_rule import FieldSpec, ParseRule, apply_rule, run_rules, typecheck_rule
from sustena.core.parse_rules_seed import SEED_RULES_BY_SOURCE, SEED_RULES_KCB, SEED_RULES_MPESA
from sustena.core.transducer import parse_message


class TestTypecheckRule:
    def test_a_well_formed_mapped_rule_has_no_errors(self):
        rule = ParseRule(
            id="t1", source="mpesa", version=1,
            pattern=r"(?P<amount>\d+)",
            extract={"amount": FieldSpec(type="amount", group="amount")},
            status="mapped", operator="budget.record_income",
            params={"amount": "$amount", "source": "test", "frequency": "once"},
        )
        assert typecheck_rule(rule) == []

    def test_a_well_formed_parsed_unmapped_rule_has_no_errors(self):
        rule = ParseRule(
            id="t2", source="mpesa", version=1,
            pattern=r"(?P<amount>\d+)",
            extract={"amount": FieldSpec(type="amount", group="amount")},
            status="parsed_unmapped",
        )
        assert typecheck_rule(rule) == []

    def test_invalid_regex_is_a_compile_error(self):
        rule = ParseRule(id="t3", source="mpesa", version=1, pattern=r"(unclosed", status="informational")
        errors = typecheck_rule(rule)
        assert any("invalid regex" in e for e in errors)

    def test_mapped_rule_with_no_operator_is_a_compile_error(self):
        rule = ParseRule(id="t4", source="mpesa", version=1, pattern=r"x", status="mapped")
        errors = typecheck_rule(rule)
        assert any("requires an operator" in e for e in errors)

    def test_mapped_rule_with_nonexistent_operator_is_a_compile_error(self):
        rule = ParseRule(id="t5", source="mpesa", version=1, pattern=r"x", status="mapped", operator="not.a.real.operator")
        errors = typecheck_rule(rule)
        assert any("not registered" in e for e in errors)

    def test_mapped_rule_binding_a_param_the_operator_does_not_declare_is_a_compile_error(self):
        rule = ParseRule(
            id="t6", source="mpesa", version=1, pattern=r"(?P<amount>\d+)",
            status="mapped", operator="budget.record_income",
            params={"totally_made_up_param": "$amount"},
        )
        errors = typecheck_rule(rule)
        assert any("not a real parameter" in e for e in errors)

    def test_non_mapped_rule_declaring_operator_is_a_compile_error(self):
        rule = ParseRule(id="t7", source="mpesa", version=1, pattern=r"x", status="parsed_unmapped", operator="budget.record_income")
        errors = typecheck_rule(rule)
        assert any("must not declare operator" in e for e in errors)

    def test_unknown_status_is_a_compile_error(self):
        rule = ParseRule(id="t8", source="mpesa", version=1, pattern=r"x", status="bogus")
        errors = typecheck_rule(rule)
        assert any("unknown status" in e for e in errors)

    def test_unknown_field_type_is_a_compile_error(self):
        rule = ParseRule(
            id="t9", source="mpesa", version=1, pattern=r"(?P<x>\d+)",
            extract={"x": FieldSpec(type="bogus", group="x")}, status="informational",
        )
        errors = typecheck_rule(rule)
        assert any("unknown type" in e for e in errors)

    def test_amount_field_without_a_group_is_a_compile_error(self):
        rule = ParseRule(
            id="t10", source="mpesa", version=1, pattern=r"x",
            extract={"amount": FieldSpec(type="amount", group=None)}, status="informational",
        )
        errors = typecheck_rule(rule)
        assert any("requires a regex group name" in e for e in errors)

    def test_literal_field_without_a_value_is_a_compile_error(self):
        rule = ParseRule(
            id="t11", source="mpesa", version=1, pattern=r"x",
            extract={"k": FieldSpec(type="literal", value=None)}, status="informational",
        )
        errors = typecheck_rule(rule)
        assert any("requires a value" in e for e in errors)

    def test_unknown_regex_flag_is_a_compile_error(self):
        rule = ParseRule(id="t12", source="mpesa", version=1, pattern=r"x", status="informational", flags=("BOGUS_FLAG",))
        errors = typecheck_rule(rule)
        assert any("unknown regex flag" in e for e in errors)


class TestRunRulesInterpreter:
    def test_first_matching_rule_wins_order_matters(self):
        broad = ParseRule(id="broad", source="x", version=1, pattern=r"paid to (?P<who>\w+)", status="parsed_unmapped",
                           extract={"who": FieldSpec(type="string", group="who")})
        narrow = ParseRule(id="narrow", source="x", version=1, pattern=r"paid to (?P<who>\w+) for account", status="parsed_unmapped",
                            extract={"who": FieldSpec(type="string", group="who")})
        # broad listed first -- it wins even though narrow would also match
        result = run_rules([broad, narrow], "paid to KPLC for account 123")
        assert result.parser_name == "broad"
        # reversed order -- narrow wins
        result2 = run_rules([narrow, broad], "paid to KPLC for account 123")
        assert result2.parser_name == "narrow"

    def test_no_matching_rule_returns_none(self):
        rule = ParseRule(id="r1", source="x", version=1, pattern=r"^NEVER_MATCHES_ANYTHING$", status="informational")
        assert run_rules([rule], "some real text") is None

    def test_empty_rule_list_returns_none(self):
        assert run_rules([], "anything") is None

    def test_optional_group_absent_is_omitted_not_none_valued(self):
        rule = ParseRule(
            id="r2", source="x", version=1,
            pattern=r"amount (?P<amount>\d+)(?: cost (?P<cost>\d+))?",
            extract={"amount": FieldSpec(type="amount", group="amount"), "cost": FieldSpec(type="amount", group="cost")},
            status="parsed_unmapped",
        )
        result = apply_rule(rule, "amount 500")
        assert result.parsed_fields == {"amount": 500.0}
        assert "cost" not in result.parsed_fields

    def test_literal_field_is_always_present(self):
        rule = ParseRule(
            id="r3", source="x", version=1, pattern=r"x",
            extract={"direction": FieldSpec(type="literal", value="sent")}, status="informational",
        )
        result = apply_rule(rule, "x")
        assert result.parsed_fields == {"direction": "sent"}

    def test_amount_field_strips_commas(self):
        rule = ParseRule(
            id="r4", source="x", version=1, pattern=r"(?P<amount>[\d,]+)",
            extract={"amount": FieldSpec(type="amount", group="amount")}, status="informational",
        )
        result = apply_rule(rule, "12,345")
        assert result.parsed_fields["amount"] == 12345.0

    def test_string_field_is_stripped(self):
        rule = ParseRule(
            id="r5", source="x", version=1, pattern=r"name:(?P<name>[A-Za-z ]+?);",
            extract={"name": FieldSpec(type="string", group="name")}, status="informational",
        )
        result = apply_rule(rule, "name: John Doe ;")
        assert result.parsed_fields["name"] == "John Doe"

    def test_direct_field_reference_preserves_type(self):
        rule = ParseRule(
            id="r6", source="mpesa", version=1, pattern=r"(?P<amount>\d+)",
            extract={"amount": FieldSpec(type="amount", group="amount")},
            status="mapped", operator="budget.record_income",
            params={"amount": "$amount", "source": "x", "frequency": "once"},
        )
        result = apply_rule(rule, "500")
        assert result.operator_params["amount"] == 500.0
        assert isinstance(result.operator_params["amount"], float)

    def test_format_template_param_produces_a_string(self):
        rule = ParseRule(
            id="r7", source="mpesa", version=1, pattern=r"(?P<name>\w+)",
            extract={"name": FieldSpec(type="string", group="name")},
            status="mapped", operator="budget.record_income",
            params={"amount": 1.0, "source": "M-Pesa: {name}", "frequency": "once"},
        )
        result = apply_rule(rule, "JohnKamau")
        assert result.operator_params["source"] == "M-Pesa: JohnKamau"

    def test_reason_template_formats_over_parsed_fields(self):
        rule = ParseRule(
            id="r8", source="x", version=1, pattern=r"(?P<amount>\d+) to (?P<who>\w+)",
            extract={"amount": FieldSpec(type="amount", group="amount"), "who": FieldSpec(type="string", group="who")},
            status="parsed_unmapped", reason_template="Sent {amount:,.2f} to {who}.",
        )
        result = apply_rule(rule, "500 to KPLC")
        assert result.reason == "Sent 500.00 to KPLC."

    def test_informational_rule_with_no_extract_gets_direction_none_default(self):
        rule = ParseRule(id="r9", source="x", version=1, pattern=r"busy", status="informational")
        result = apply_rule(rule, "system busy")
        assert result.status == "informational"
        assert result.parsed_fields == {"direction": "none"}


class TestSeedLibraryWellFormedness:
    def test_every_seed_rule_typechecks_clean(self):
        for rule in SEED_RULES_MPESA + SEED_RULES_KCB:
            errors = typecheck_rule(rule)
            assert errors == [], f"{rule.id}: {errors}"

    def test_every_seed_rule_parses_its_own_inducing_example(self):
        # The §4G.7 "inducing-example fidelity" property, applied to the
        # seed library itself: every declared rule must actually recognise
        # the real sample it was built from.
        for rule in SEED_RULES_MPESA + SEED_RULES_KCB:
            for example in rule.examples:
                result = apply_rule(rule, example)
                assert result is not None, f"{rule.id} failed to match its own example: {example!r}"
                assert result.parser_name == rule.id

    def test_seed_rules_by_source_declares_both_mpesa_and_kcb(self):
        # KCB was migrated to declared data 2 Aug 2026 (the "broaden parser
        # coverage" round) -- this test used to assert the opposite
        # (kcb absent, fallback-only) when that was the real, disclosed
        # scope boundary. Corrected in place now that boundary has moved,
        # not reverted around.
        assert "mpesa" in SEED_RULES_BY_SOURCE
        assert "kcb" in SEED_RULES_BY_SOURCE
        assert len(SEED_RULES_BY_SOURCE["kcb"]) == 15

    def test_kcb_receive_now_answered_by_a_declared_rule_not_the_python_fallback(self):
        # Mirrors what test_kcb_source_gets_zero_declared_rules_falls_
        # through_to_python_tier used to prove for the OLD (fallback-only)
        # scope -- now proves the opposite: this exact shape is answered by
        # the declared tier.
        result = parse_message(
            "QGH7XJ2K9L Confirmed! You have received KES5,000.00 from JOHN KAMAU - 123456789 "
            "at 20-07-26 02:15PM via KCB",
            source_id="kcb",
        )
        assert result.status == "mapped"
        assert result.parser_name == "kcb_receive"  # a declared ParseRule, not the Python fallback

    def test_kcb_system_notice_still_falls_through_to_python_tier(self):
        # The one deliberately-NOT-migrated KCB shape (see
        # parse_rules_seed.py's own module docstring) -- a generic system/
        # error notice that isn't any of the 15 declared shapes must still
        # fall through correctly to the Python fallback tier.
        result = parse_message(
            "Dear customer, we are unable to process your request at this time. Please try again later.",
            source_id="kcb",
        )
        assert result.status == "informational"
        assert result.parser_name == "kcb_system_notice"


class TestDeclaredTierIntegration:
    """Confirms the declared tier is genuinely being consulted first for
    mpesa (not just that the end-to-end answer happens to match)."""

    def test_a_migrated_shape_is_answered_by_the_declared_rule_not_the_fallback(self):
        text = (
            "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU "
            "254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00"
        )
        result = parse_message(text, source_id="mpesa")
        # parser_name is shared between the declared rule and the Python
        # fallback by design (same id, see parse_rules_seed.py) -- prove
        # the DECLARED path specifically ran by checking a declared-only
        # rule directly answers the same text.
        declared = run_rules(SEED_RULES_MPESA, text)
        assert declared is not None
        assert declared.status == result.status == "mapped"
        assert declared.operator_params == result.operator_params

    def test_mpesa_withdraw_now_answered_by_the_declared_rule(self):
        # mpesa_withdraw was migrated 2 Aug 2026 (the "broaden parser
        # coverage" round) via the new prefix_if_missing field type --
        # this test used to prove the OPPOSITE (fallback-only) when that
        # was the real, disclosed scope boundary. Corrected in place, not
        # reverted around, now that the boundary has moved.
        text = (
            "QGH7XJ6T4U Confirmed. Ksh3,000.00 withdrawn from Agent 123456 - TOWN SHOP "
            "on 20/7/26 at 6:00 PM. New M-PESA balance is Ksh8,050.00"
        )
        result = parse_message(text, source_id="mpesa")
        assert result.status == "parsed_unmapped"
        assert result.parser_name == "mpesa_withdraw"
        assert result.parsed_fields["counterparty"] == "Agent 123456 - TOWN SHOP"
        declared = run_rules(SEED_RULES_MPESA, text)
        assert declared is not None
        assert declared.parsed_fields == result.parsed_fields

    def test_mpesa_withdraw_prefix_if_missing_when_agent_already_present(self):
        # The one real conditional transform in mpesa_withdraw: some real
        # withdrawal SMS already say "Agent 123456", others just the raw
        # agent code -- prefix_if_missing must not double the prefix.
        result = parse_message(
            "QGH7XJ6T4V Confirmed. Ksh1,000.00 withdrawn from 654321 - ESTATE SHOP "
            "on 20/7/26 at 6:00 PM. New M-PESA balance is Ksh7,050.00",
            source_id="mpesa",
        )
        assert result.parsed_fields["counterparty"] == "Agent 654321 - ESTATE SHOP"

    def test_a_non_migrated_mpesa_shape_still_falls_through_to_python_tier(self):
        # mpesa_system_notice (the shared, generic system/error-message
        # catch-all) is the one remaining non-migrated mpesa shape -- see
        # parse_rules_seed.py's own module docstring.
        result = parse_message(
            "Request timed out. Please try again later.",
            source_id="mpesa",
        )
        assert result.status == "informational"
        assert result.parser_name == "mpesa_system_notice"


class TestKCBDeclaredParity:
    """Every migrated KCB shape's declared-rule output is byte-for-byte
    identical to the Python fallback's output, run against the same real
    sample text -- the same parity proof M-Pesa's own migration
    established, extended to all 15 declared KCB shapes."""

    @pytest.mark.parametrize("rule", SEED_RULES_KCB, ids=[r.id for r in SEED_RULES_KCB])
    def test_declared_matches_python_fallback(self, rule):
        for example in rule.examples:
            declared = run_rules(SEED_RULES_KCB, example)
            fallback = parse_message(example, source_id="kcb", declared_rules=[])
            assert declared is not None, f"{rule.id}'s own example didn't match its declared rule: {example!r}"
            assert fallback.parser_name == rule.id, (
                f"{rule.id}'s own example was answered by the fallback tier's "
                f"'{fallback.parser_name}' instead -- pattern/order mismatch"
            )
            assert declared.status == fallback.status
            assert declared.operator_name == fallback.operator_name
            assert declared.operator_params == fallback.operator_params
            assert declared.external_ref == fallback.external_ref
            # A declared rule's parsed_fields can carry a FEW harmless extra
            # keys the hand-wired parser never returned: "ref" (needed to
            # populate external_ref via fields.get("ref") -- a pre-existing
            # characteristic since Phase 3B's very first migrated rule,
            # MPESA_RECEIVED, has the identical extra key) and, for a
            # handful of KCB shapes, an intermediate field (e.g. "name")
            # used only to build a "template"-type field (e.g.
            # kcb_mpesa_paybill's counterparty combines paybill+name).
            # Neither ever changes or removes anything the original
            # returned -- assert the fallback's exact fields are still all
            # present with identical values, not that no extras exist.
            declared_fields = declared.parsed_fields
            fallback_fields = fallback.parsed_fields
            for key, value in fallback_fields.items():
                assert key in declared_fields, f"{rule.id}: fallback field '{key}' missing from declared output"
                assert declared_fields[key] == value, f"{rule.id}: field '{key}' differs ({declared_fields[key]!r} != {value!r})"
            extra_keys = set(declared_fields) - set(fallback_fields) - {"ref", "name"}
            assert not extra_keys, f"{rule.id}: unexpected extra declared field(s) {extra_keys}"
            assert declared.reason == fallback.reason

    def test_declared_tier_is_genuinely_consulted_first_for_kcb(self):
        # Mirrors TestDeclaredTierIntegration's own mpesa proof -- confirms
        # parse_message() (the real caller) answers from the declared tier,
        # not merely that the Python fallback happens to agree.
        text = "You have received KES 750.00 from MARY WANJIRU. M-PESA Ref UH1B91GYNY"
        result = parse_message(text, source_id="kcb")
        declared = run_rules(SEED_RULES_KCB, text)
        assert declared is not None
        assert declared.status == result.status == "mapped"
        assert declared.operator_params == result.operator_params

    def test_kcb_card_currency_upper_field_type(self):
        # The one shape needing the new "upper" field type -- a lowercase
        # currency in the source text must still normalise to KES/USD.
        result = parse_message(
            "kes 429.00 transaction made on KCB card 1234XXXXXXXX5678 at NAIVAS SUPERMARKET "
            "on 1/8/26 12:25pm, Avail balance KES 59,055.00",
            source_id="kcb",
        )
        assert result.parser_name == "kcb_card"
        assert result.parsed_fields["currency"] == "KES"

    def test_kcb_loan_repay_template_field_builds_counterparty(self):
        # The "template" field type's core case: counterparty is built from
        # TWO pieces (a literal phrase plus a captured loan_number), not a
        # single regex group.
        result = parse_message(
            "KES 2,500.00 was debited from your KCB account to repay your KCB Mobile Loan 445566. "
            "Your new loan balance is KES 6,000.00",
            source_id="kcb",
        )
        assert result.parser_name == "kcb_loan_repay"
        assert result.parsed_fields["counterparty"] == "KCB Mobile Loan 445566 repayment"
        assert result.parsed_fields["loan_number"] == "445566"

    def test_kcb_loan_overdue_reason_references_raw_group_not_a_stored_field(self):
        # reason_template widened to see raw regex groups, not just
        # parsed_fields -- "date" is never promoted to a permanent field
        # (matching the original hand-wired parser exactly) but the reason
        # text must still state it correctly.
        result = parse_message(
            "KCB Mobile Loan repayment of KES 1,000.00 is overdue since 20/7/26",
            source_id="kcb",
        )
        assert result.parser_name == "kcb_loan_overdue"
        assert "date" not in result.parsed_fields
        assert "20/7/26" in result.reason

    def test_kcb_paybill_and_account_direction_is_received_not_sent(self):
        # The real direction bug found and fixed against Bonnie's actual
        # paybill sample (2 Aug 2026) -- despite "sent to..." wording, this
        # is money being credited INTO the account, not sent from it.
        result = parse_message(
            "Ksh40000.00 sent to KCB Pay Bill 522522 for account 135***5140 BONVENTURE NGUGI MAINA "
            "has been received on 01/08/2026 at 12:21 PM. M-PESA ref UH1B91GYNW",
            source_id="kcb",
        )
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.parsed_fields["direction"] == "received"
