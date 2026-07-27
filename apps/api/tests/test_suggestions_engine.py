"""
tests/test_suggestions_engine.py

Engine-level tests for Slice 10 (advisory idle loop): evaluate_operatives(),
get_suggestions(), accept_suggestion(), dismiss_suggestion().

Genericity note: create_definition() (Slice 6) only supports flat scalar
dimensions, not the nested finances.{liquid,pockets} shape the advisory
rules key off — so these engine-lifecycle tests use the real homestead/
habitat templates as fixtures, same as most of test_sustain_engine.py
already does. The engine PLUMBING itself (evaluate/accept/dismiss/dedupe/
expire in sustain_engine.py) contains no homestead/habitat-specific code —
it dispatches purely off spec["operatives"] and get_operative_statuses();
genericity of the RULE LOGIC itself is proven separately, with synthetic
non-homestead shapes, in test_advisory.py.
"""

import pytest

from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


async def _make_urgent_pocket(engine, sid, pocket_name, allocate_amt, spend_amt):
    await engine.execute_operator(sid, "budget.allocate", {"pocket_name": pocket_name, "amount": allocate_amt, "period": "monthly"})
    await engine.execute_operator(sid, "budget.spend", {"pocket_name": pocket_name, "amount": spend_amt, "description": "", "category": "general"})


def _unallocated_income(suggestions):
    """
    Isolate the unallocated_income suggestion from a fresh homestead's
    evaluate_operatives() result. A brand-new :memory: sustain has never
    sent an egress, so Mentor's OTHER rule (rule_summary_not_exported)
    legitimately fires too on every evaluate() in these fixtures — that's
    real, correct behavior (Slice 11), not noise to suppress, so tests
    that only care about the income condition select it explicitly rather
    than assuming it's the only suggestion present.
    """
    matches = [s for s in suggestions if s["rule_id"] == "unallocated_income"]
    assert len(matches) == 1, f"expected exactly one unallocated_income suggestion, got {suggestions}"
    return matches[0]


class TestEvaluateOperatives:
    @pytest.mark.asyncio
    async def test_no_operatives_declared_returns_empty(self, engine: SustainEngine):
        sid = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Test"})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        suggestions = engine.evaluate_operatives(sid)
        assert suggestions == []  # habitat.json declares operatives: {}

    @pytest.mark.asyncio
    async def test_fires_mentor_suggestion_for_unallocated_income(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        suggestions = engine.evaluate_operatives(sid)
        sug = _unallocated_income(suggestions)
        assert sug["operative_id"] == "mentor"
        assert sug["proposed_operator"] == "budget.allocate"
        assert sug["status"] == "pending"

    @pytest.mark.asyncio
    async def test_evaluate_never_mutates_state_or_events(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        before_state = engine.get_state(sid)
        before_events = engine.get_events(sid, limit=100)
        engine.evaluate_operatives(sid)
        assert engine.get_state(sid) == before_state
        assert engine.get_events(sid, limit=100) == before_events
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    def test_unknown_sustain_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError):
            engine.evaluate_operatives("does-not-exist")


class TestDedupeAndExpire:
    @pytest.mark.asyncio
    async def test_dismissed_does_not_resurface_same_condition(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        first = engine.evaluate_operatives(sid)
        sug = _unallocated_income(first)
        engine.dismiss_suggestion(sug["id"])
        second = engine.evaluate_operatives(sid)
        assert not any(s["rule_id"] == "unallocated_income" for s in second)

    @pytest.mark.asyncio
    async def test_meaningfully_different_condition_resurfaces_after_dismiss(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        first = engine.evaluate_operatives(sid)
        sug = _unallocated_income(first)
        engine.dismiss_suggestion(sug["id"])

        await engine.execute_operator(sid, "budget.record_income", {"amount": 50000, "source": "y", "frequency": "once"})
        third = engine.evaluate_operatives(sid)
        new_sug = _unallocated_income(third)
        assert new_sug["dedupe_key"] != sug["dedupe_key"]

    @pytest.mark.asyncio
    async def test_pending_suggestion_expires_when_condition_resolves(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        first = engine.evaluate_operatives(sid)
        sug = _unallocated_income(first)

        # Resolve the condition for real: allocate MORE so the pocket's pct drops below threshold.
        await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 3000, "period": "monthly"})
        second = engine.evaluate_operatives(sid)
        assert not any(s["rule_id"] == "unallocated_income" for s in second)
        all_rows = engine.get_suggestions(sid, status=None)
        expired = [r for r in all_rows if r["id"] == sug["id"]]
        assert expired[0]["status"] == "expired"

    @pytest.mark.asyncio
    async def test_repeat_evaluate_refreshes_not_duplicates(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        first = engine.evaluate_operatives(sid)
        second = engine.evaluate_operatives(sid)
        sug1 = _unallocated_income(first)
        sug2 = _unallocated_income(second)
        assert sug1["id"] == sug2["id"]  # same row, not a duplicate

    @pytest.mark.asyncio
    async def test_accepted_does_not_resurface(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        first = engine.evaluate_operatives(sid)
        sug = _unallocated_income(first)
        await engine.accept_suggestion(sug["id"])
        # accepting fully allocates liquid into food, so the condition itself
        # is now resolved too -- but even the exact dedupe_key must never
        # resurface once accepted, regardless of whether liquid is refilled
        # into a state that would rebuild the identical bucket.
        second = engine.evaluate_operatives(sid)
        assert all(s["dedupe_key"] != sug["dedupe_key"] for s in second)


class TestAcceptSuggestion:
    @pytest.mark.asyncio
    async def test_accept_runs_real_operator_and_appends_event(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        before_events = len(engine.get_events(sid, limit=100))
        sug = _unallocated_income(engine.evaluate_operatives(sid))

        outcome = await engine.accept_suggestion(sug["id"])
        assert outcome["result"].succeeded
        assert len(engine.get_events(sid, limit=100)) == before_events + 1
        assert engine.get_state(sid)["finances"]["liquid"]["balance"] == 0.0
        assert engine.rebuild_state(sid) == engine.get_state(sid)

        resolved = engine.get_suggestions(sid, status="accepted")
        assert resolved[0]["id"] == sug["id"]

    @pytest.mark.asyncio
    async def test_accept_honestly_refuses_on_drifted_state(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 2000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 100, 99)
        sug = _unallocated_income(engine.evaluate_operatives(sid))
        proposed_amount = sug["proposed_params"]["amount"]
        assert proposed_amount > 0

        # Drift live state for real via a DIFFERENT operator call, so the
        # frozen suggestion amount is no longer affordable.
        await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "rent", "amount": proposed_amount - 1, "period": "monthly"})

        outcome = await engine.accept_suggestion(sug["id"])
        assert outcome["result"].failed
        assert "liquid.balance" in outcome["result"].reason

        still_pending = engine.get_suggestions(sid, status="pending")
        assert any(s["id"] == sug["id"] for s in still_pending)  # not silently discarded

    @pytest.mark.asyncio
    async def test_accept_unknown_suggestion_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError):
            await engine.accept_suggestion("does-not-exist")

    @pytest.mark.asyncio
    async def test_accept_non_pending_raises(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        sug = _unallocated_income(engine.evaluate_operatives(sid))
        engine.dismiss_suggestion(sug["id"])
        with pytest.raises(ValueError):
            await engine.accept_suggestion(sug["id"])

    @pytest.mark.asyncio
    async def test_accept_informational_only_raises(self, engine: SustainEngine):
        parent = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Cira"})
        engine.link_child(parent, child, member="Cira")
        # Backdate the child's genesis event so it reads as stale.
        row = engine._db.execute(
            "SELECT id FROM events WHERE sustain_id = ? ORDER BY seq ASC LIMIT 1", (child,),
        ).fetchone()
        engine._db.execute(
            "UPDATE events SET timestamp = ? WHERE id = ?",
            ("2020-01-01T00:00:00", row["id"]),
        )
        engine._db.commit()

        suggestions = engine.evaluate_operatives(parent)
        stale = [s for s in suggestions if s["rule_id"] == "child_stale"]
        assert len(stale) == 1
        assert stale[0]["proposed_operator"] is None

        with pytest.raises(ValueError):
            await engine.accept_suggestion(stale[0]["id"])


class TestDismissSuggestion:
    @pytest.mark.asyncio
    async def test_dismiss_marks_dismissed(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        sug = _unallocated_income(engine.evaluate_operatives(sid))
        assert engine.dismiss_suggestion(sug["id"]) is True
        dismissed = engine.get_suggestions(sid, status="dismissed")
        assert any(s["id"] == sug["id"] for s in dismissed)

    def test_dismiss_unknown_returns_false(self, engine: SustainEngine):
        assert engine.dismiss_suggestion("does-not-exist") is False

    @pytest.mark.asyncio
    async def test_dismiss_already_resolved_returns_false(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        sug = _unallocated_income(engine.evaluate_operatives(sid))
        engine.dismiss_suggestion(sug["id"])
        assert engine.dismiss_suggestion(sug["id"]) is False


class TestGetSuggestions:
    @pytest.mark.asyncio
    async def test_status_filter_and_all(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        await _make_urgent_pocket(engine, sid, "food", 1000, 950)
        sug = _unallocated_income(engine.evaluate_operatives(sid))
        engine.dismiss_suggestion(sug["id"])

        # Mentor's second rule (summary_not_exported) also legitimately
        # fires on a fresh sustain that's never sent an egress — still
        # pending, since only the income suggestion was dismissed.
        pending = engine.get_suggestions(sid, status="pending")
        assert not any(s["id"] == sug["id"] for s in pending)
        assert any(s["rule_id"] == "summary_not_exported" for s in pending)

        dismissed = engine.get_suggestions(sid, status="dismissed")
        assert len(dismissed) == 1
        assert dismissed[0]["id"] == sug["id"]

        assert len(engine.get_suggestions(sid, status=None)) == 2


class TestChildStaleComposition:
    @pytest.mark.asyncio
    async def test_stale_child_fires_attache_suggestion(self, engine: SustainEngine):
        parent = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        fresh_child = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Fresh"})
        stale_child = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Stale"})
        engine.link_child(parent, fresh_child, member="Fresh")
        engine.link_child(parent, stale_child, member="Stale")

        row = engine._db.execute(
            "SELECT id FROM events WHERE sustain_id = ? ORDER BY seq ASC LIMIT 1", (stale_child,),
        ).fetchone()
        engine._db.execute("UPDATE events SET timestamp = ? WHERE id = ?", ("2020-01-01T00:00:00", row["id"]))
        engine._db.commit()

        before_state = engine.get_state(parent)
        suggestions = engine.evaluate_operatives(parent)

        stale_hits = [s for s in suggestions if s["rule_id"] == "child_stale"]
        assert len(stale_hits) == 1
        assert "Stale" in stale_hits[0]["title"]
        assert engine.get_state(parent) == before_state  # evaluating never mutates the parent either
