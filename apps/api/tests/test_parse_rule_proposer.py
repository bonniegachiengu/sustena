"""
tests/test_parse_rule_proposer.py

Phase 3D (Parser primitive-lift, 2 Aug 2026) — verify_proposed_rule and
propose_and_verify. See parse_rule_proposer.py's own docstring for the
honest scope statement: the verifier/wiring is real and tested here; no
generator is wired by default (propose_and_verify(generator=None) always
returns None, never a fabricated proposal) — these tests prove exactly
that honesty property alongside the verifier's real two-condition logic.
"""

import sustena.operators  # noqa: F401
from sustena.core.parse_rule import FieldSpec, ParseRule
from sustena.core.parse_rule_proposer import ProposedRule, propose_and_verify, verify_proposed_rule
from sustena.core.parse_rules_seed import SEED_RULES_MPESA


def _candidate(pattern=r"NEWTHING (?P<amount>\d+)"):
    return ParseRule(
        id="proposed_rule", source="mpesa", version=1, pattern=pattern,
        extract={"amount": FieldSpec(type="amount", group="amount")},
        status="informational",
    )


class TestVerifyProposedRule:
    def test_a_well_formed_rule_matching_its_inducing_example_and_no_collision_verifies(self):
        ok, reasons = verify_proposed_rule(_candidate(), "NEWTHING 500", SEED_RULES_MPESA)
        assert ok is True
        assert reasons == []

    def test_fails_the_typecheck_gate_first(self):
        bad = ParseRule(id="bad", source="mpesa", version=1, pattern="(unclosed", status="informational")
        ok, reasons = verify_proposed_rule(bad, "anything", SEED_RULES_MPESA)
        assert ok is False
        assert any("Gamma |- r failed" in r for r in reasons)

    def test_fails_inducing_example_fidelity_if_it_does_not_match_its_own_example(self):
        rule = _candidate(pattern=r"COMPLETELY_DIFFERENT_TEXT (?P<amount>\d+)")
        ok, reasons = verify_proposed_rule(rule, "NEWTHING 500", SEED_RULES_MPESA)
        assert ok is False
        assert any("does not match its own inducing example" in r for r in reasons)

    def test_fails_stored_message_regression_if_it_would_intercept_an_existing_rules_example(self):
        # A dangerously broad candidate that would swallow mpesa_received's
        # own real example text too.
        broad = ParseRule(
            id="too_broad", source="mpesa", version=1,
            pattern=r"Confirmed", status="informational",
        )
        inducing = "Confirmed some new shape nobody has seen"
        existing_examples = [e for r in SEED_RULES_MPESA for e in r.examples]
        assert any("Confirmed" in e for e in existing_examples)  # sanity: real collision risk exists
        ok, reasons = verify_proposed_rule(broad, inducing, SEED_RULES_MPESA)
        assert ok is False
        assert any("would intercept" in r for r in reasons)

    def test_both_conditions_required_a_rule_can_fail_only_regression(self):
        # Matches its own example fine, but is broad enough to also catch
        # an existing rule's real example -- must still be rejected.
        rule = ParseRule(
            id="partial_fail", source="mpesa", version=1,
            pattern=r"(?P<amount>\d+)", extract={"amount": FieldSpec(type="amount", group="amount")},
            status="informational",
        )
        ok, reasons = verify_proposed_rule(rule, "500", SEED_RULES_MPESA)
        assert ok is False
        assert any("would intercept" in r for r in reasons)


class TestProposeAndVerifyHonesty:
    def test_with_no_generator_wired_returns_none_not_a_fabricated_proposal(self):
        result = propose_and_verify("some genuinely novel unparsed text", SEED_RULES_MPESA, generator=None)
        assert result is None

    def test_default_call_with_no_generator_arg_also_returns_none(self):
        result = propose_and_verify("some genuinely novel unparsed text", SEED_RULES_MPESA)
        assert result is None

    def test_a_generator_returning_none_itself_also_yields_no_proposal(self):
        result = propose_and_verify("x", SEED_RULES_MPESA, generator=lambda text: None)
        assert result is None

    def test_a_generator_producing_a_verified_candidate_is_reported_verified(self):
        result = propose_and_verify("NEWTHING 500", SEED_RULES_MPESA, generator=lambda text: _candidate())
        assert isinstance(result, ProposedRule)
        assert result.status == "verified"
        assert result.reasons == []

    def test_a_generator_producing_a_bad_candidate_is_reported_rejected_with_reasons(self):
        bad_generator = lambda text: ParseRule(id="x", source="mpesa", version=1, pattern="(unclosed", status="informational")
        result = propose_and_verify("anything", SEED_RULES_MPESA, generator=bad_generator)
        assert result.status == "rejected"
        assert len(result.reasons) > 0
