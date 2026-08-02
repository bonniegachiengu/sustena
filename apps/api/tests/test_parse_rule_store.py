"""
tests/test_parse_rule_store.py

Phase 3C (Parser primitive-lift, 2 Aug 2026) — gated correction edits:
SustainEngine.add_parse_rule / modify_parse_rule / retire_parse_rule,
the append-only version history, and get_effective_parse_rules() merging
active corrections with the seed library.

"Train the parser" = accumulate declared rules from confirmed corrections
(§4I.6) — every test here proves a correction is gated (typechecked),
versioned (append-only history), reversible (RetireRule <-> AddRule), and
safe (ModifyRule/RetireRule never silently regress a previously-handled
example).
"""

import pytest

import sustena.operators  # noqa: F401 -- OPERATOR_REGISTRY population for mapped-rule typechecks
from sustena.core.parse_rule import FieldSpec, ParseRule
from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


def _new_rule(rule_id="new_shape", pattern=r"NEWSHAPE (?P<amount>\d+)", examples=("NEWSHAPE 500",)):
    return ParseRule(
        id=rule_id, source="mpesa", version=1, pattern=pattern,
        extract={"amount": FieldSpec(type="amount", group="amount")},
        status="informational", examples=examples,
    )


class TestAddParseRule:
    def test_adding_a_brand_new_rule_succeeds(self, engine):
        result = engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        assert result["status"] == "ok"
        assert result["version"] == 1
        assert result["edit_name"] == "AddRule"

    def test_a_typecheck_failure_refuses_and_writes_nothing(self, engine):
        bad_rule = ParseRule(id="bad", source="mpesa", version=1, pattern="(unclosed", status="informational")
        result = engine.add_parse_rule("mpesa", bad_rule, author_user_id="u1")
        assert result["status"] == "failed"
        assert "errors" in result
        assert engine.list_parse_rule_history("bad") == []

    def test_adding_a_rule_id_that_already_has_an_active_version_is_refused(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        result = engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        assert result["status"] == "failed"
        assert "already has an active version" in result["reason"]

    def test_added_rule_appears_in_effective_rules_for_its_source(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        effective = engine.get_effective_parse_rules("mpesa")
        assert any(r.id == "new_shape" for r in effective)

    def test_added_rule_does_not_leak_into_a_different_source(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        effective = engine.get_effective_parse_rules("kcb")
        assert not any(r.id == "new_shape" for r in effective)

    def test_a_mapped_rule_binding_a_nonexistent_operator_param_is_refused(self, engine):
        bad = ParseRule(
            id="badmap", source="mpesa", version=1, pattern=r"(?P<amount>\d+)",
            extract={"amount": FieldSpec(type="amount", group="amount")},
            status="mapped", operator="budget.record_income", params={"not_a_real_param": "$amount"},
        )
        result = engine.add_parse_rule("mpesa", bad, author_user_id="u1")
        assert result["status"] == "failed"


class TestModifyParseRule:
    def test_modifying_a_rule_with_no_baseline_at_all_is_refused(self, engine):
        never_existed = _new_rule(rule_id="ghost")
        result = engine.modify_parse_rule("ghost", never_existed, author_user_id="u1")
        assert result["status"] == "failed"
        assert "no active version and no shipped seed rule" in result["reason"]

    def test_modifying_a_users_own_earlier_addition_succeeds_and_versions_up(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        wider = _new_rule(pattern=r"NEWSHAPE\s+(?P<amount>\d+)")  # a harmless widening, same examples still match
        result = engine.modify_parse_rule("new_shape", wider, author_user_id="u1")
        assert result["status"] == "ok"
        assert result["version"] == 2

    def test_modify_id_mismatch_is_refused(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        mismatched = _new_rule(rule_id="different_id")
        result = engine.modify_parse_rule("new_shape", mismatched, author_user_id="u1")
        assert result["status"] == "failed"
        assert "must match" in result["reason"]

    def test_a_modification_that_would_regress_a_previously_handled_example_is_refused(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        # Narrower pattern -- "NEWSHAPE 500" (the rule's own declared
        # example) would stop matching entirely under this candidate.
        broken = _new_rule(pattern=r"NEWSHAPE_ONLY_UPPERCASE (?P<amount>\d+)")
        result = engine.modify_parse_rule("new_shape", broken, author_user_id="u1")
        assert result["status"] == "failed"
        assert "regressions" in result
        assert result["regressions"][0]["example"] == "NEWSHAPE 500"
        # Untouched -- the original rule is still active at version 1.
        history = engine.list_parse_rule_history("new_shape")
        assert len(history) == 1
        assert history[0]["status"] == "active"

    def test_modifying_a_shipped_seed_rule_directly_creates_version_1_in_the_db(self, engine):
        # mpesa_received is a real seed rule with no prior DB row -- its
        # first-ever correction becomes DB version 1, baselined against
        # the SEED rule's own examples (not a DB row).
        from sustena.core.parse_rules_seed import MPESA_RECEIVED
        # A harmless modification: same pattern, different reason_template only.
        tweaked = ParseRule(
            id="mpesa_received", source="mpesa", version=1, pattern=MPESA_RECEIVED.pattern,
            extract=MPESA_RECEIVED.extract, status="mapped", operator="budget.record_income",
            params=MPESA_RECEIVED.params, reason_template="corrected reason text",
            examples=MPESA_RECEIVED.examples,
        )
        result = engine.modify_parse_rule("mpesa_received", tweaked, author_user_id="u1")
        assert result["status"] == "ok", result
        assert result["version"] == 1
        effective = engine.get_effective_parse_rules("mpesa")
        overridden = next(r for r in effective if r.id == "mpesa_received")
        assert overridden.reason_template == "corrected reason text"

    def test_a_regressing_modification_of_a_shipped_seed_rule_is_refused(self, engine):
        from sustena.core.parse_rules_seed import MPESA_RECEIVED
        broken = ParseRule(
            id="mpesa_received", source="mpesa", version=1,
            pattern=r"THIS_WILL_NEVER_MATCH_ANY_REAL_MESSAGE",
            extract=MPESA_RECEIVED.extract, status="mapped", operator="budget.record_income",
            params=MPESA_RECEIVED.params, examples=MPESA_RECEIVED.examples,
        )
        result = engine.modify_parse_rule("mpesa_received", broken, author_user_id="u1")
        assert result["status"] == "failed"
        assert "regressions" in result
        # The shipped seed rule is untouched -- no DB override exists.
        assert engine.list_active_parse_rule_overrides("mpesa") == []


class TestRetireParseRule:
    def test_retiring_an_active_rule_succeeds(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        result = engine.retire_parse_rule("new_shape", author_user_id="u1")
        assert result["status"] == "ok"
        assert result["edit_name"] == "RetireRule"

    def test_a_retired_rule_no_longer_appears_in_effective_rules(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        engine.retire_parse_rule("new_shape", author_user_id="u1")
        effective = engine.get_effective_parse_rules("mpesa")
        assert not any(r.id == "new_shape" for r in effective)

    def test_retiring_a_rule_with_no_active_version_is_refused(self, engine):
        result = engine.retire_parse_rule("never_added", author_user_id="u1")
        assert result["status"] == "failed"

    def test_retiring_twice_is_refused_the_second_time(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        first = engine.retire_parse_rule("new_shape", author_user_id="u1")
        second = engine.retire_parse_rule("new_shape", author_user_id="u1")
        assert first["status"] == "ok"
        assert second["status"] == "failed"

    def test_history_is_append_only_never_deletes_a_row(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        engine.retire_parse_rule("new_shape", author_user_id="u1")
        history = engine.list_parse_rule_history("new_shape")
        assert len(history) == 1  # the same row, status flipped -- not deleted
        assert history[0]["status"] == "retired"
        assert history[0]["edit_name"] == "AddRule"


class TestVersionHistory:
    def test_history_records_parent_version_correctly(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        engine.modify_parse_rule("new_shape", _new_rule(pattern=r"NEWSHAPE\s+(?P<amount>\d+)"), author_user_id="u2")
        history = engine.list_parse_rule_history("new_shape")
        assert len(history) == 2
        assert history[0]["version"] == 1 and history[0]["status"] == "superseded"
        assert history[1]["version"] == 2 and history[1]["status"] == "active"
        assert history[1]["parent_version"] == 1
        assert history[1]["author_user_id"] == "u2"

    def test_only_one_row_is_ever_active_at_a_time(self, engine):
        engine.add_parse_rule("mpesa", _new_rule(), author_user_id="u1")
        engine.modify_parse_rule("new_shape", _new_rule(pattern=r"NEWSHAPE\s+(?P<amount>\d+)"), author_user_id="u1")
        engine.modify_parse_rule("new_shape", _new_rule(pattern=r"NEWSHAPE\s*(?P<amount>\d+)"), author_user_id="u1")
        history = engine.list_parse_rule_history("new_shape")
        active_rows = [h for h in history if h["status"] == "active"]
        assert len(active_rows) == 1
        assert active_rows[0]["version"] == 3


class TestEffectiveRulesIntegration:
    def test_declared_correction_is_actually_consulted_by_parse_message(self, engine):
        from sustena.core.transducer import parse_message

        engine.add_parse_rule("mpesa", _new_rule(pattern=r"CUSTOMFORMAT (?P<amount>\d+)"), author_user_id="u1")
        rules = engine.get_effective_parse_rules("mpesa")
        result = parse_message("CUSTOMFORMAT 777", source_id="mpesa", declared_rules=rules)
        assert result.status == "informational"
        assert result.parser_name == "new_shape"
        assert result.parsed_fields["amount"] == 777.0

    def test_without_declared_rules_param_the_correction_has_no_effect(self, engine):
        # Proves declared_rules is genuinely opt-in -- a caller (like every
        # existing test) that doesn't pass it never sees engine-side state,
        # keeping parse_message() pure/stateless by default.
        from sustena.core.transducer import parse_message

        engine.add_parse_rule("mpesa", _new_rule(pattern=r"CUSTOMFORMAT (?P<amount>\d+)"), author_user_id="u1")
        result = parse_message("CUSTOMFORMAT 777", source_id="mpesa")
        assert result.status == "unparsed"
