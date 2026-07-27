"""
tests/test_advisory.py

Pure rule-logic tests for sustena/core/advisory.py — Slice 10's "operatives
advise, humans decide" layer. No engine, no DB, no operators: every rule is
a pure function of state (or, for the composition-aware rule, pre-computed
child metadata) -> list[Suggestion]. Confirms the rules are generic (react
to the shape, not to "homestead"/"habitat" by name) and that nothing here
has any way to mutate anything (there's nothing to call).
"""

from sustena.core.advisory import (
    ADVISORY_RULES,
    Suggestion,
    evaluate_operative,
    rule_child_stale,
    rule_summary_not_exported,
    rule_unallocated_income,
)


def _state(liquid_balance, pockets):
    return {"finances": {"liquid": {"balance": liquid_balance}, "pockets": pockets}}


class TestRuleUnallocatedIncome:
    def test_fires_when_liquid_idle_and_pocket_strained(self):
        state = _state(5000, {"food": {"allocated": 1000, "spent": 900, "limit": 0}})
        out = rule_unallocated_income(state)
        assert len(out) == 1
        s = out[0]
        assert s.operative_id == "mentor"
        assert s.rule_id == "unallocated_income"
        assert s.proposed_operator == "budget.allocate"
        assert s.proposed_params == {"pocket_name": "food", "amount": 5000, "period": "monthly"}
        assert "food" in s.reason and "5,000" in s.reason

    def test_no_fire_when_liquid_zero(self):
        state = _state(0, {"food": {"allocated": 1000, "spent": 900, "limit": 0}})
        assert rule_unallocated_income(state) == []

    def test_no_fire_when_no_pockets(self):
        state = _state(5000, {})
        assert rule_unallocated_income(state) == []

    def test_no_fire_when_pockets_below_threshold(self):
        state = _state(5000, {"food": {"allocated": 1000, "spent": 200, "limit": 0}})
        assert rule_unallocated_income(state) == []

    def test_uses_limit_over_allocated_when_limit_set(self):
        # allocated=1000, spent=850 -> 85% by allocated, but limit=2000 -> 42.5% by limit -> should NOT fire
        state = _state(5000, {"food": {"allocated": 1000, "spent": 850, "limit": 2000}})
        assert rule_unallocated_income(state) == []

    def test_picks_the_most_urgent_pocket(self):
        state = _state(5000, {
            "food": {"allocated": 1000, "spent": 850, "limit": 0},   # 85%
            "rent": {"allocated": 1000, "spent": 999, "limit": 0},   # 99.9% -- most urgent
        })
        out = rule_unallocated_income(state)
        assert out[0].proposed_params["pocket_name"] == "rent"

    def test_missing_finances_block_returns_empty(self):
        assert rule_unallocated_income({}) == []

    def test_generic_shape_not_tied_to_any_template(self):
        # A totally synthetic sustain shape (not homestead/habitat) that
        # merely happens to follow the finances.{liquid,pockets} convention.
        state = {"finances": {"liquid": {"balance": 100}, "pockets": {"widgets": {"allocated": 10, "spent": 9, "limit": 0}}}}
        out = rule_unallocated_income(state)
        assert len(out) == 1

    def test_dedupe_key_buckets_by_pocket_and_coarse_amount(self):
        state_a = _state(5000, {"food": {"allocated": 1000, "spent": 999, "limit": 0}})
        state_b = _state(5100, {"food": {"allocated": 1000, "spent": 999, "limit": 0}})
        key_a = rule_unallocated_income(state_a)[0].dedupe_key
        key_b = rule_unallocated_income(state_b)[0].dedupe_key
        assert key_a == key_b  # same 500-wide bucket

        state_c = _state(6000, {"food": {"allocated": 1000, "spent": 999, "limit": 0}})
        key_c = rule_unallocated_income(state_c)[0].dedupe_key
        assert key_c != key_a  # meaningfully different amount -> different bucket


class TestRuleChildStale:
    def test_fires_past_threshold(self):
        children = [{"sustain_id": "c1", "member": "Cira", "days_since_last_event": 9.5}]
        out = rule_child_stale(children)
        assert len(out) == 1
        assert out[0].operative_id == "attache"
        assert out[0].proposed_operator is None  # informational only
        assert "Cira" in out[0].reason
        assert "9" in out[0].reason

    def test_no_fire_below_threshold(self):
        children = [{"sustain_id": "c1", "member": "Cira", "days_since_last_event": 2}]
        assert rule_child_stale(children) == []

    def test_no_fire_when_days_unknown(self):
        children = [{"sustain_id": "c1", "member": "Cira", "days_since_last_event": None}]
        assert rule_child_stale(children) == []

    def test_multiple_children_independent(self):
        children = [
            {"sustain_id": "c1", "member": "A", "days_since_last_event": 1},
            {"sustain_id": "c2", "member": "B", "days_since_last_event": 30},
        ]
        out = rule_child_stale(children)
        assert len(out) == 1
        assert "B" in out[0].reason

    def test_empty_children_no_fire(self):
        assert rule_child_stale([]) == []

    def test_dedupe_key_weekly_bucket(self):
        c8 = rule_child_stale([{"sustain_id": "c1", "member": "A", "days_since_last_event": 8}])[0]
        c13 = rule_child_stale([{"sustain_id": "c1", "member": "A", "days_since_last_event": 13}])[0]
        c14 = rule_child_stale([{"sustain_id": "c1", "member": "A", "days_since_last_event": 14}])[0]
        assert c8.dedupe_key == c13.dedupe_key  # same week-7 bucket
        assert c8.dedupe_key != c14.dedupe_key  # next week bucket


class TestRuleSummaryNotExported:
    def test_fires_when_never_exported(self):
        out = rule_summary_not_exported(None)
        assert len(out) == 1
        assert out[0].operative_id == "mentor"
        assert out[0].proposed_operator == "egress.prepare_household_summary"
        assert out[0].proposed_params == {}
        assert "ever" in out[0].reason

    def test_fires_when_stale(self):
        out = rule_summary_not_exported(10)
        assert len(out) == 1
        assert "10" in out[0].reason

    def test_no_fire_when_recent(self):
        assert rule_summary_not_exported(1) == []
        assert rule_summary_not_exported(6.9) == []

    def test_fires_exactly_at_threshold(self):
        assert len(rule_summary_not_exported(7)) == 1

    def test_never_exported_and_stale_have_different_dedupe_keys(self):
        never = rule_summary_not_exported(None)[0]
        stale = rule_summary_not_exported(30)[0]
        assert never.dedupe_key != stale.dedupe_key

    def test_proposes_no_send_only_prepare(self):
        # The proposed operator can only ever QUEUE (prepare) — it is not
        # confirm_egress or anything capable of sending. This is the
        # structural proof that accepting this suggestion cannot send.
        out = rule_summary_not_exported(None)
        assert out[0].proposed_operator == "egress.prepare_household_summary"
        assert "confirm" not in out[0].proposed_operator
        assert "send" not in out[0].proposed_operator


class TestEvaluateOperative:
    def test_dispatches_to_bound_rules(self):
        # Mentor has two bound rules; pin last_sent_days recent enough that
        # only rule_unallocated_income fires, isolating this test to
        # "dispatch reaches the bound rule and returns its output" rather
        # than also asserting how many rules happen to be bound today.
        state = _state(5000, {"food": {"allocated": 1000, "spent": 999, "limit": 0}})
        out = evaluate_operative("mentor", state, extra={"last_sent_days": 1})
        assert len(out) == 1
        assert out[0].operative_id == "mentor"
        assert out[0].rule_id == "unallocated_income"

    def test_dispatches_to_all_bound_rules_when_all_fire(self):
        state = _state(5000, {"food": {"allocated": 1000, "spent": 999, "limit": 0}})
        out = evaluate_operative("mentor", state, extra={"last_sent_days": None})
        rule_ids = {s.rule_id for s in out}
        assert rule_ids == {"unallocated_income", "summary_not_exported"}

    def test_unbound_operative_returns_empty(self):
        state = _state(5000, {"food": {"allocated": 1000, "spent": 999, "limit": 0}})
        assert evaluate_operative("navigator", state) == []
        assert evaluate_operative("curator", state) == []
        assert evaluate_operative("protege", state) == []

    def test_unknown_operative_id_returns_empty_not_error(self):
        assert evaluate_operative("not_a_real_operative", {}) == []

    def test_attache_dispatches_children_meta(self):
        children = [{"sustain_id": "c1", "member": "A", "days_since_last_event": 30}]
        out = evaluate_operative("attache", {}, extra={"children_meta": children})
        assert len(out) == 1
        assert out[0].rule_id == "child_stale"

    def test_registry_shape(self):
        assert "mentor" in ADVISORY_RULES
        assert "attache" in ADVISORY_RULES
        assert callable(ADVISORY_RULES["mentor"][0])


class TestSuggestionIsPlainData:
    def test_suggestion_has_no_side_effect_surface(self):
        # A Suggestion is a plain dataclass with no methods beyond what
        # @dataclass generates -- nothing on it can execute an operator or
        # touch a DB. This is the structural guarantee behind "advisory
        # only": the object this module hands back is inert data.
        s = Suggestion(
            operative_id="mentor", rule_id="x", title="t", reason="r",
            severity="info", dedupe_key="k",
        )
        methods = [m for m in dir(s) if not m.startswith("_")]
        assert set(methods) == {
            "operative_id", "rule_id", "title", "reason", "severity",
            "dedupe_key", "proposed_operator", "proposed_params",
        }
