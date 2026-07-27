"""
tests/test_sustain_engine.py

Tests for SustainEngine — Epic 1.3.2.

Covers:
  - Instantiate a Homestead sustain → sustain_id returned, state initialised
  - get_state returns correct default state
  - execute_operator with budget.record_income → state updated, result.ok == True
  - execute_operator with unknown operator → result.ok == False
  - execute_operator with budget.spend exceeding allocation → result.ok == False, state unchanged
  - simulate with 3-step sequence → 3 results returned, DB state unchanged after
  - Instantiate with missing required parameter → ValueError raised

Run with:
    pytest tests/test_sustain_engine.py -v
"""

import copy
import json
import pytest

from sustena.core.sustain_engine import SustainEngine


# ── Fixtures ──────────────────────────────────────────────────────────────────

@pytest.fixture
def engine() -> SustainEngine:
    """Fresh in-memory SustainEngine for each test — no disk I/O."""
    return SustainEngine(db_path=":memory:")


@pytest.fixture
def homestead_sid(engine: SustainEngine) -> str:
    """Instantiated Homestead sustain bound to a test user."""
    return engine.instantiate(
        template_id="homestead",
        user_id="user-test-1",
        parameters={"owner_ids": ["user-test-1"]},
    )


# ── 1. Instantiate returns a sustain_id ───────────────────────────────────────

class TestInstantiate:

    def test_returns_non_empty_string(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        assert isinstance(sid, str)
        assert len(sid) > 0

    def test_returns_uuid_format(self, engine: SustainEngine):
        import uuid
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        # Must be a valid UUID (no exception raised)
        uuid.UUID(sid)

    def test_state_initialised_in_db(self, engine: SustainEngine, homestead_sid: str):
        """State is written to DB on instantiate — get_state must not raise."""
        state = engine.get_state(homestead_sid)
        assert isinstance(state, dict)

    def test_missing_required_parameter_raises(self, engine: SustainEngine):
        """owner_ids is required — omitting it must raise ValueError."""
        with pytest.raises(ValueError, match="owner_ids"):
            engine.instantiate("homestead", "u1", {})

    def test_optional_parameter_applied_to_state(self, engine: SustainEngine):
        """initial_income (optional) is applied to finances.income.amount."""
        sid = engine.instantiate(
            "homestead", "u1",
            {"owner_ids": ["u1"], "initial_income": 45000.0},
        )
        state = engine.get_state(sid)
        assert state["finances"]["income"]["amount"] == 45000.0

    def test_two_instantiations_return_distinct_ids(self, engine: SustainEngine):
        sid1 = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        sid2 = engine.instantiate("homestead", "u2", {"owner_ids": ["u2"]})
        assert sid1 != sid2


# ── 2. get_state returns correct default state ────────────────────────────────

class TestGetState:

    def test_default_members_empty(self, engine: SustainEngine, homestead_sid: str):
        state = engine.get_state(homestead_sid)
        assert state["members"] == []

    def test_default_liquid_balance_zero(self, engine: SustainEngine, homestead_sid: str):
        state = engine.get_state(homestead_sid)
        assert state["finances"]["liquid"]["balance"] == 0.0

    def test_default_pockets_empty(self, engine: SustainEngine, homestead_sid: str):
        state = engine.get_state(homestead_sid)
        assert state["finances"]["pockets"] == {}

    def test_default_goals_empty(self, engine: SustainEngine, homestead_sid: str):
        state = engine.get_state(homestead_sid)
        assert state["finances"]["goals"] == []

    def test_default_calendar_events_empty(self, engine: SustainEngine, homestead_sid: str):
        state = engine.get_state(homestead_sid)
        assert state["calendar"]["events"] == []

    def test_default_alerts_empty(self, engine: SustainEngine, homestead_sid: str):
        state = engine.get_state(homestead_sid)
        assert state["alerts"] == []

    def test_get_state_returns_deep_copy(self, engine: SustainEngine, homestead_sid: str):
        """Mutating the returned dict must not corrupt the stored state."""
        state = engine.get_state(homestead_sid)
        state["finances"]["liquid"]["balance"] = 999999.0
        state2 = engine.get_state(homestead_sid)
        assert state2["finances"]["liquid"]["balance"] == 0.0


# ── 3. execute_operator — budget.record_income (happy path) ──────────────────

class TestExecuteOperatorRecordIncome:
    """
    budget.record_income expects:
      finances.income.sources     list
      finances.income.monthly_total  float

    The homestead default_state uses a simpler income schema
    (amount / frequency / last_recorded). We seed the compatible schema
    directly via engine._persist_state before testing the operator.
    """

    def _seed_income_state(self, engine: SustainEngine, sid: str, liquid: float = 10000.0):
        """Patch stored state to have the income schema budget.py operators expect."""
        state = engine.get_state(sid)
        state["finances"]["liquid"]["balance"] = liquid
        state["finances"]["income"] = {
            "sources":       [],
            "monthly_total": 0.0,
        }
        engine._persist_state(sid, state)

    @pytest.mark.asyncio
    async def test_record_income_succeeds(self, engine: SustainEngine, homestead_sid: str):
        self._seed_income_state(engine, homestead_sid)
        result = await engine.execute_operator(
            homestead_sid,
            "budget.record_income",
            {"amount": 5000.0, "source": "Salary", "frequency": "monthly"},
        )
        assert result.succeeded, f"Expected ok, got: {result.reason}"

    @pytest.mark.asyncio
    async def test_record_income_updates_liquid_balance(
        self, engine: SustainEngine, homestead_sid: str
    ):
        self._seed_income_state(engine, homestead_sid, liquid=0.0)
        await engine.execute_operator(
            homestead_sid,
            "budget.record_income",
            {"amount": 3000.0, "source": "Side hustle"},
        )
        state = engine.get_state(homestead_sid)
        assert state["finances"]["liquid"]["balance"] == 3000.0

    @pytest.mark.asyncio
    async def test_record_income_state_persisted(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """State change must survive a fresh get_state call."""
        self._seed_income_state(engine, homestead_sid, liquid=1000.0)
        await engine.execute_operator(
            homestead_sid,
            "budget.record_income",
            {"amount": 4000.0, "source": "Freelance"},
        )
        state = engine.get_state(homestead_sid)
        assert state["finances"]["liquid"]["balance"] == 5000.0


# ── 4. execute_operator — unknown operator returns failure ────────────────────

class TestExecuteOperatorUnknown:

    @pytest.mark.asyncio
    async def test_unknown_operator_returns_fail(
        self, engine: SustainEngine, homestead_sid: str
    ):
        result = await engine.execute_operator(
            homestead_sid,
            "nonexistent.operator",
            {},
        )
        assert not result.succeeded
        assert result.failed

    @pytest.mark.asyncio
    async def test_unknown_operator_has_reason(
        self, engine: SustainEngine, homestead_sid: str
    ):
        result = await engine.execute_operator(homestead_sid, "ghost.op", {})
        assert result.reason is not None
        assert len(result.reason) > 0

    @pytest.mark.asyncio
    async def test_operator_not_in_spec_returns_fail(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """chama.contribute is a real registered operator but not in homestead spec."""
        result = await engine.execute_operator(homestead_sid, "chama.contribute", {})
        assert not result.succeeded
        assert "not in the operator list" in result.reason or result.reason


# ── 5. execute_operator — budget.spend exceeding allocation ───────────────────

class TestExecuteOperatorSpendExceedsAllocation:

    def _seed_with_pocket(self, engine: SustainEngine, sid: str):
        """Seed state with a 'food' pocket (allocated=500, spent=0)."""
        state = engine.get_state(sid)
        state["finances"]["liquid"]["balance"] = 1000.0
        state["finances"]["income"] = {"sources": [], "monthly_total": 0.0}
        state["finances"]["pockets"]["food"] = {
            "allocated": 500.0,
            "spent":     0.0,
            "limit":     0.0,
        }
        engine._persist_state(sid, state)

    @pytest.mark.asyncio
    async def test_spend_exceeding_pocket_fails(
        self, engine: SustainEngine, homestead_sid: str
    ):
        self._seed_with_pocket(engine, homestead_sid)
        result = await engine.execute_operator(
            homestead_sid,
            "budget.spend",
            {"pocket_name": "food", "amount": 600.0, "description": "Groceries"},
        )
        assert not result.succeeded
        assert result.failed

    @pytest.mark.asyncio
    async def test_spend_exceeding_state_unchanged(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """State must not change when a spend is rejected."""
        self._seed_with_pocket(engine, homestead_sid)
        state_before = engine.get_state(homestead_sid)

        result = await engine.execute_operator(
            homestead_sid,
            "budget.spend",
            {"pocket_name": "food", "amount": 999.0, "description": "Too much"},
        )
        assert not result.succeeded

        state_after = engine.get_state(homestead_sid)
        assert state_after["finances"]["pockets"]["food"]["spent"] == \
               state_before["finances"]["pockets"]["food"]["spent"]

    @pytest.mark.asyncio
    async def test_spend_within_allocation_succeeds(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """Confirm a valid spend does work (sanity check for the failure tests)."""
        self._seed_with_pocket(engine, homestead_sid)
        result = await engine.execute_operator(
            homestead_sid,
            "budget.spend",
            {"pocket_name": "food", "amount": 100.0, "description": "Lunch"},
        )
        assert result.succeeded


# ── 6. simulate — 3-step sequence, DB untouched ───────────────────────────────

class TestSimulate:
    """
    Use homestead.calendar.* operators for simulate tests — they only need
    calendar.events which is [] in the default state (no schema patching needed).
    """

    _FUTURE_DATE_1 = "2099-01-15T10:00:00"
    _FUTURE_DATE_2 = "2099-02-20T14:00:00"

    @pytest.mark.asyncio
    async def test_simulate_returns_three_results(
        self, engine: SustainEngine, homestead_sid: str
    ):
        sequence = [
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Meeting A", "date_str": self._FUTURE_DATE_1}},
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Meeting B", "date_str": self._FUTURE_DATE_2}},
            {"operator": "homestead.calendar.upcoming_events",
             "params": {"days": 30000}},
        ]
        results = await engine.simulate(homestead_sid, sequence)
        assert len(results) == 3

    @pytest.mark.asyncio
    async def test_simulate_results_have_required_keys(
        self, engine: SustainEngine, homestead_sid: str
    ):
        sequence = [
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Event X", "date_str": self._FUTURE_DATE_1}},
        ]
        results = await engine.simulate(homestead_sid, sequence)
        step = results[0]
        assert "operator"    in step
        assert "params"      in step
        assert "result"      in step
        assert "state_after" in step

    @pytest.mark.asyncio
    async def test_simulate_does_not_persist_to_db(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """DB state must be untouched after simulate — live state unchanged."""
        state_before = engine.get_state(homestead_sid)

        sequence = [
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Ghost Event", "date_str": self._FUTURE_DATE_1}},
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Another Ghost", "date_str": self._FUTURE_DATE_2}},
            {"operator": "homestead.calendar.upcoming_events",
             "params": {"days": 30000}},
        ]
        await engine.simulate(homestead_sid, sequence)

        state_after_simulate = engine.get_state(homestead_sid)
        assert state_after_simulate["calendar"]["events"] == \
               state_before["calendar"]["events"]

    @pytest.mark.asyncio
    async def test_simulate_state_advances_across_steps(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """State_after in step 2 should reflect the event added in step 1."""
        sequence = [
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "First Event", "date_str": self._FUTURE_DATE_1}},
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Second Event", "date_str": self._FUTURE_DATE_2}},
            {"operator": "homestead.calendar.upcoming_events",
             "params": {"days": 30000}},
        ]
        results = await engine.simulate(homestead_sid, sequence)

        # After step 1: 1 event
        assert len(results[0]["state_after"]["calendar"]["events"]) == 1
        # After step 2: 2 events
        assert len(results[1]["state_after"]["calendar"]["events"]) == 2

    @pytest.mark.asyncio
    async def test_simulate_failed_step_does_not_advance_state(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """A failing step must not change the forked state for subsequent steps."""
        sequence = [
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Good Event", "date_str": self._FUTURE_DATE_1}},
            # past date → should fail
            {"operator": "homestead.calendar.add_event",
             "params": {"title": "Bad Event", "date_str": "2000-01-01T00:00:00"}},
            {"operator": "homestead.calendar.upcoming_events",
             "params": {"days": 30000}},
        ]
        results = await engine.simulate(homestead_sid, sequence)

        # Step 1 succeeded
        assert results[0]["result"].succeeded
        # Step 2 failed (past date)
        assert not results[1]["result"].succeeded
        # Step 3 state should still only have 1 event (from step 1)
        assert len(results[2]["state_after"]["calendar"]["events"]) == 1


# ── 7. Instantiate with missing required parameter ────────────────────────────

class TestInstantiateMissingParam:

    def test_raises_value_error(self, engine: SustainEngine):
        with pytest.raises(ValueError):
            engine.instantiate("homestead", "u1", {})

    def test_error_message_names_param(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="owner_ids"):
            engine.instantiate("homestead", "u1", {})

    def test_raises_only_for_required_params(self, engine: SustainEngine):
        """Omitting optional params (e.g. initial_income) must NOT raise."""
        sid = engine.instantiate(
            "homestead", "u1",
            {"owner_ids": ["u1"]},  # only required param
        )
        assert sid  # instantiated successfully


# ── 8. Slice 2 — the enforcing gate (Move 2) ───────────────────────────────────

class TestEnforcementGate:
    """
    homestead.json ships with enforcement.enabled=true (Slice 2). Every real
    homestead operator already self-guards against its own invariants, so to
    prove the ENGINE-level gate independently — not just re-test operators
    that were already safe — these tests temporarily monkeypatch an operator's
    registered .fn to a version that violates an invariant directly, bypassing
    StateAccessor.decrement()'s own negative-balance guard. This simulates
    exactly the scenario the gate exists for: a coding mistake in some future
    operator that doesn't self-guard.
    """

    @pytest.fixture
    def bad_income_fn(self):
        """budget.record_income, monkeypatched to push liquid balance negative."""
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        meta = OPERATOR_REGISTRY["budget.record_income"]
        original_fn = meta.fn

        async def violates_invariant(ctx, **kwargs):
            ctx.state.set("finances.liquid.balance", -500.0)
            return OperatorResult.ok({"hacked": True})

        meta.fn = violates_invariant
        yield
        meta.fn = original_fn

    @pytest.mark.asyncio
    async def test_bad_transition_is_refused(
        self, engine: SustainEngine, homestead_sid: str, bad_income_fn
    ):
        result = await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 1.0})
        assert result.failed
        assert result.constraint_violated == "enforcement_gate"
        assert "liquid_non_negative" in result.reason

    @pytest.mark.asyncio
    async def test_bad_transition_leaves_live_state_untouched(
        self, engine: SustainEngine, homestead_sid: str, bad_income_fn
    ):
        before = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 1.0})
        after = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        assert after == before
        assert after != -500.0

    @pytest.mark.asyncio
    async def test_refusal_reason_names_the_failing_dimension(
        self, engine: SustainEngine, homestead_sid: str, bad_income_fn
    ):
        result = await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 1.0})
        assert "finances.liquid.balance" in result.reason
        assert "-500" in result.reason

    @pytest.mark.asyncio
    async def test_good_transition_still_commits_under_enforcement(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """budget.allocate with sufficient liquid balance — real operator, no patching."""
        state = engine.get_state(homestead_sid)
        state["finances"]["liquid"]["balance"] = 10000.0
        engine._persist_state(homestead_sid, state)

        result = await engine.execute_operator(
            homestead_sid, "budget.allocate", {"pocket_name": "food", "amount": 3000.0}
        )
        assert result.succeeded, f"Expected ok, got: {result.reason}"
        after = engine.get_state(homestead_sid)
        assert after["finances"]["liquid"]["balance"] == 7000.0
        assert after["finances"]["pockets"]["food"]["allocated"] == 3000.0

    @pytest.mark.asyncio
    async def test_boundary_transition_to_exactly_zero_commits(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """liquid_non_negative is >= 0 (inclusive) — landing exactly on 0 must not be refused."""
        state = engine.get_state(homestead_sid)
        state["finances"]["liquid"]["balance"] = 500.0
        engine._persist_state(homestead_sid, state)

        result = await engine.execute_operator(
            homestead_sid, "budget.allocate", {"pocket_name": "rent", "amount": 500.0}
        )
        assert result.succeeded, f"Expected ok, got: {result.reason}"
        assert engine.get_state(homestead_sid)["finances"]["liquid"]["balance"] == 0.0

    @pytest.mark.asyncio
    async def test_non_enforced_sustain_is_unaffected(self, engine: SustainEngine, homestead_sid: str):
        """
        A sustain without spec["enforcement"]["enabled"] must be a true no-op
        for the gate. homestead and habitat are now the only two shipped
        sustains (chama was removed in this slice — "only homestead should
        remain" — and both biashara/vyyb were removed earlier), and both ship
        with enforcement ON, so there's no real non-enforced sustain left to
        instantiate. Proven instead by disabling homestead's enforcement
        in-memory on its already-compiled spec and confirming a transition
        that would otherwise be refused now commits — a stronger check than
        the old version, which only inspected the flag and never actually
        exercised a bad transition against a disabled gate.
        """
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        spec = engine.get_spec(homestead_sid)
        assert spec.get("enforcement", {}).get("enabled") is True  # sanity: normally on
        spec["enforcement"] = {"enabled": False}

        meta = OPERATOR_REGISTRY["budget.record_income"]
        original_fn = meta.fn

        async def violates_invariant(ctx, **kwargs):
            ctx.state.set("finances.liquid.balance", -500.0)
            return OperatorResult.ok({"hacked": True})

        meta.fn = violates_invariant
        try:
            result = await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 1.0})
        finally:
            meta.fn = original_fn
            spec["enforcement"] = {"enabled": True}  # restore for any later test using this fixture's spec cache

        assert result.succeeded, "gate must not fire when enforcement is disabled"
        assert engine.get_state(homestead_sid)["finances"]["liquid"]["balance"] == -500.0

    @pytest.mark.asyncio
    async def test_simulate_refuses_bad_step_and_does_not_advance_fork(
        self, engine: SustainEngine, homestead_sid: str, bad_income_fn
    ):
        results = await engine.simulate(
            homestead_sid,
            [{"operator": "budget.record_income", "params": {"amount": 1.0}}],
        )
        assert len(results) == 1
        assert results[0]["result"].failed
        assert results[0]["result"].constraint_violated == "enforcement_gate"
        assert results[0]["state_after"]["finances"]["liquid"]["balance"] != -500.0

    def test_compiled_invariants_cached_on_spec(self, engine: SustainEngine, homestead_sid: str):
        spec = engine.get_spec(homestead_sid)
        ids = {inv["id"] for inv in spec["_compiled_invariants"]}
        assert ids == {"liquid_non_negative", "pocket_allocated_non_negative"}
        assert spec["_invariant_compile_errors"] == []


# ── 9. Slice 3 — state = fold(events) ──────────────────────────────────────────

class TestEventSourcing:
    """
    sustain_states is now a cache; the events table (specifically each row's
    mutations_json) is the source of truth. get_state() reads the cache;
    rebuild_state() derives state from scratch by folding every event. The
    two must always agree — that's the property this class exists to prove.
    """

    def test_instantiate_writes_exactly_one_genesis_event(self, engine: SustainEngine, homestead_sid: str):
        events = engine.get_events(homestead_sid, limit=50)
        assert len(events) == 1
        assert events[0]["event_name"] == "event.system.genesis_snapshot"

    def test_genesis_seq_is_one(self, engine: SustainEngine, homestead_sid: str):
        row = engine._db.execute(
            "SELECT seq FROM events WHERE sustain_id = ?", (homestead_sid,)
        ).fetchone()
        assert row["seq"] == 1

    def test_rebuild_state_matches_get_state_immediately_after_instantiate(
        self, engine: SustainEngine, homestead_sid: str
    ):
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)

    @pytest.mark.asyncio
    async def test_rebuild_state_matches_get_state_after_operator_sequence(
        self, engine: SustainEngine, homestead_sid: str
    ):
        await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 50000.0, "source": "Salary"})
        await engine.execute_operator(homestead_sid, "budget.allocate", {"pocket_name": "food", "amount": 15000.0})
        await engine.execute_operator(homestead_sid, "budget.spend", {"pocket_name": "food", "amount": 3000.0, "description": "shop", "category": "groceries"})
        await engine.execute_operator(homestead_sid, "budget.transfer", {"from_pocket": "food", "to_pocket": "rent", "amount": 2000.0})

        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)
        # sanity: it's not trivially equal because nothing happened
        assert engine.get_state(homestead_sid)["finances"]["liquid"]["balance"] == 35000.0

    @pytest.mark.asyncio
    async def test_seq_numbers_are_gapless_and_increasing(self, engine: SustainEngine, homestead_sid: str):
        await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 1000.0, "source": "A"})
        await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 1000.0, "source": "B"})
        rows = engine._db.execute(
            "SELECT seq FROM events WHERE sustain_id = ? ORDER BY seq ASC", (homestead_sid,)
        ).fetchall()
        seqs = [r["seq"] for r in rows]
        assert seqs == list(range(1, len(seqs) + 1))

    @pytest.mark.asyncio
    async def test_multi_event_call_attaches_mutations_to_first_event_only(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """
        Mirrors mkulima.receive_signal, which publishes two events from one
        call. The mutations list reflects everything StateAccessor recorded
        for the WHOLE call — attaching it to more than one event would
        double-apply it on fold.
        """
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        meta = OPERATOR_REGISTRY["budget.record_income"]
        original_fn = meta.fn

        async def multi_event_fn(ctx, **kwargs):
            ctx.state.increment("finances.liquid.balance", 777.0)
            await ctx.events.publish("event.finances.income_received", {"amount": 777.0})
            await ctx.events.publish("event.system.secondary_hook", {"note": "no new mutation"})
            return OperatorResult.ok({})

        meta.fn = multi_event_fn
        try:
            result = await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 777.0})
        finally:
            meta.fn = original_fn

        assert result.succeeded
        rows = engine._db.execute(
            "SELECT event_name, seq, mutations_json FROM events WHERE sustain_id = ? ORDER BY seq ASC",
            (homestead_sid,),
        ).fetchall()
        assert [r["event_name"] for r in rows] == [
            "event.system.genesis_snapshot",
            "event.finances.income_received",
            "event.system.secondary_hook",
        ]
        assert len(json.loads(rows[1]["mutations_json"])) > 0   # first event: real mutations
        assert json.loads(rows[2]["mutations_json"]) == []       # second event: no-op for fold
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)
        assert engine.get_state(homestead_sid)["finances"]["liquid"]["balance"] == 777.0

    @pytest.mark.asyncio
    async def test_operator_mutating_without_publishing_gets_a_synthesized_event(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """
        The completeness safety net: no state change is allowed to be
        unlogged, even if an operator forgets to publish() — this proves
        the mechanism exists and actually gets exercised, not just declared.
        """
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        meta = OPERATOR_REGISTRY["budget.record_income"]
        original_fn = meta.fn

        async def silent_mutator(ctx, **kwargs):
            ctx.state.increment("finances.liquid.balance", 42.0)
            return OperatorResult.ok({})  # no publish() at all

        meta.fn = silent_mutator
        try:
            result = await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 42.0})
        finally:
            meta.fn = original_fn

        assert result.succeeded
        events = engine.get_events(homestead_sid, limit=50)
        synthesized = [e for e in events if e["event_name"] == "event.system.unlogged_state_change"]
        assert len(synthesized) == 1
        assert synthesized[0]["payload"]["operator"] == "budget.record_income"
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)
        assert engine.get_state(homestead_sid)["finances"]["liquid"]["balance"] == 42.0

    @pytest.mark.asyncio
    async def test_gate_refusal_leaves_fold_consistent(
        self, engine: SustainEngine, homestead_sid: str
    ):
        """Combines TestEnforcementGate's refusal check with the fold property:
        a refused transition must change neither the cache nor the log, so the
        two stay in agreement."""
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        meta = OPERATOR_REGISTRY["budget.record_income"]
        original_fn = meta.fn

        async def violates_invariant(ctx, **kwargs):
            ctx.state.set("finances.liquid.balance", -500.0)
            return OperatorResult.ok({"hacked": True})

        meta.fn = violates_invariant
        before_events = len(engine.get_events(homestead_sid, limit=100))
        try:
            result = await engine.execute_operator(homestead_sid, "budget.record_income", {"amount": 1.0})
        finally:
            meta.fn = original_fn

        assert result.failed
        assert len(engine.get_events(homestead_sid, limit=100)) == before_events
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)

    def test_seed_pocket_appends_event_and_stays_fold_consistent(
        self, engine: SustainEngine, homestead_sid: str
    ):
        before = len(engine.get_events(homestead_sid, limit=100))
        assert engine.seed_pocket(homestead_sid, "rent", 20000.0, 25000.0) is True
        after_events = engine.get_events(homestead_sid, limit=100)
        assert len(after_events) == before + 1
        # get_events() is newest-first by seq (Slice 3 fix — see its docstring)
        # so the just-seeded event, being the latest, is index 0.
        assert after_events[0]["event_name"] == "event.finances.pocket_seeded"
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)
        assert engine.get_state(homestead_sid)["finances"]["pockets"]["rent"]["allocated"] == 20000.0

    def test_seed_pocket_unknown_sustain_returns_false(self, engine: SustainEngine):
        assert engine.seed_pocket("does-not-exist", "food", 100) is False

    def test_seed_event_appends_with_empty_mutations_and_stays_fold_consistent(
        self, engine: SustainEngine, homestead_sid: str
    ):
        before_state = engine.get_state(homestead_sid)
        assert engine.seed_event(homestead_sid, "event.seed.note", {"amount": 5}) is True
        row = engine._db.execute(
            "SELECT mutations_json FROM events WHERE sustain_id = ? AND event_name = 'event.seed.note'",
            (homestead_sid,),
        ).fetchone()
        assert json.loads(row["mutations_json"]) == []
        assert engine.get_state(homestead_sid) == before_state  # purely informational — no state change
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)

    def test_seed_event_unknown_sustain_returns_false(self, engine: SustainEngine):
        assert engine.seed_event("does-not-exist", "event.seed.note", {}) is False

    def test_commit_external_mutation_appends_event_and_updates_cache(
        self, engine: SustainEngine, homestead_sid: str
    ):
        from sustena.core.state import StateAccessor

        state = StateAccessor(engine.get_state(homestead_sid))
        state.set("finances.liquid.balance", 9999.0)
        engine.commit_external_mutation(
            homestead_sid, state, "event.council.vote_resolved", {"proposal_id": "p1", "vote": "YES"}
        )
        assert engine.get_state(homestead_sid)["finances"]["liquid"]["balance"] == 9999.0
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)
        events = engine.get_events(homestead_sid, limit=50)
        assert any(e["event_name"] == "event.council.vote_resolved" for e in events)

    def test_commit_external_mutation_is_a_no_op_when_nothing_changed(
        self, engine: SustainEngine, homestead_sid: str
    ):
        from sustena.core.state import StateAccessor

        before = len(engine.get_events(homestead_sid, limit=100))
        state = StateAccessor(engine.get_state(homestead_sid))  # no .set()/.append()/.remove() calls
        engine.commit_external_mutation(homestead_sid, state, "event.council.vote_resolved", {})
        assert len(engine.get_events(homestead_sid, limit=100)) == before


class TestMigrateToEventSourcing:
    """
    migrate_to_event_sourcing() backfills sustains created before Slice 3 —
    a raw cache write at instantiate() time, some old events with no seq/
    mutations, and critically NO genesis event. These tests build that exact
    shape by hand via raw SQL (the only way to get it now that instantiate()
    always writes a genesis event) to mirror the real production DB inventory
    found before this slice: 2 homestead sustains, one with pre-existing
    informational events, plus one orphaned sustain_states row with no
    matching sustains row.
    """

    def _seed_pre_slice3_sustain(self, engine, sid, template_id, state, old_events=()):
        """old_events: list of (event_name, payload_dict) in chronological order."""
        import json as _json
        import uuid as _uuid
        from datetime import datetime as _dt

        created_at = "2026-06-01T00:00:00"
        engine._db.execute(
            "INSERT INTO sustains (id, user_id, template_id, created_at) VALUES (?, ?, ?, ?)",
            (sid, "legacy-user", template_id, created_at),
        )
        engine._db.execute(
            "INSERT INTO sustain_states (id, sustain_id, state_json, version_number, updated_at) "
            "VALUES (?, ?, ?, 1, ?)",
            (str(_uuid.uuid4()), sid, _json.dumps(state), created_at),
        )
        for i, (name, payload) in enumerate(old_events):
            # Old-shape row: no seq, no mutations_json — exactly what a
            # pre-Slice-3 events table row looked like.
            engine._db.execute(
                "INSERT INTO events (id, sustain_id, event_name, payload_json, operator_log_id, timestamp) "
                "VALUES (?, ?, ?, ?, NULL, ?)",
                (str(_uuid.uuid4()), sid, name, _json.dumps(payload), f"2026-06-02T00:0{i}:00"),
            )
        engine._db.commit()
        engine._specs.pop(sid, None)  # force _get_spec to reload fresh (not cached from elsewhere)

    def test_migrates_sustain_with_no_prior_events(self, engine: SustainEngine):
        sid = "legacy-sid-1"
        state = {
            "members": [], "finances": {"liquid": {"balance": 7500.0}, "pockets": {}, "income": {"amount": 0.0, "frequency": "monthly", "last_recorded": None}, "goals": []},
            "calendar": {"events": []}, "tasks": {"items": []}, "alerts": [],
        }
        self._seed_pre_slice3_sustain(engine, sid, "homestead", state)

        report = engine.migrate_to_event_sourcing()

        assert sid in report["migrated"]
        assert report["verification"][sid] is True
        assert engine.rebuild_state(sid) == engine.get_state(sid)
        assert engine.get_state(sid)["finances"]["liquid"]["balance"] == 7500.0
        events = engine.get_events(sid, limit=50)
        assert len(events) == 1
        assert events[0]["event_name"] == "event.system.genesis_snapshot"

    def test_migrates_sustain_and_preserves_prior_event_history(self, engine: SustainEngine):
        sid = "legacy-sid-2"
        state = {
            "members": [], "finances": {"liquid": {"balance": 100.0}, "pockets": {"rent": {"allocated": 900.0, "spent": 900.0, "limit": 0.0}}, "income": {"amount": 0.0, "frequency": "monthly", "last_recorded": None}, "goals": []},
            "calendar": {"events": []}, "tasks": {"items": []}, "alerts": [],
        }
        old_events = [
            ("event.finances.income_received", {"amount": 1000.0}),
            ("event.finances.pocket_spent", {"pocket": "rent", "amount": 900.0}),
        ]
        self._seed_pre_slice3_sustain(engine, sid, "homestead", state, old_events)

        before_count = engine._db.execute(
            "SELECT COUNT(*) AS n FROM events WHERE sustain_id = ?", (sid,)
        ).fetchone()["n"]
        assert before_count == 2  # sanity: the two old events exist pre-migration

        report = engine.migrate_to_event_sourcing()

        assert report["verification"][sid] is True
        # Old history is renumbered, not deleted — 2 old events + 1 genesis = 3.
        events = engine.get_events(sid, limit=50)
        assert len(events) == 3
        names = {e["event_name"] for e in events}
        assert "event.finances.income_received" in names
        assert "event.finances.pocket_spent" in names
        assert "event.system.genesis_snapshot" in names

        # Genesis must be seq=1 — old events renumbered after it, in original order.
        rows = engine._db.execute(
            "SELECT event_name, seq, mutations_json FROM events WHERE sustain_id = ? ORDER BY seq ASC",
            (sid,),
        ).fetchall()
        assert rows[0]["event_name"] == "event.system.genesis_snapshot"
        assert rows[0]["seq"] == 1
        assert rows[1]["event_name"] == "event.finances.income_received"
        assert rows[1]["seq"] == 2
        assert rows[2]["event_name"] == "event.finances.pocket_spent"
        assert rows[2]["seq"] == 3
        # Renumbered old events carry no mutations — they don't re-contribute
        # to the fold, since the genesis snapshot already captured their effect.
        import json as _json
        assert _json.loads(rows[1]["mutations_json"]) == []
        assert _json.loads(rows[2]["mutations_json"]) == []

        assert engine.rebuild_state(sid) == engine.get_state(sid)
        assert engine.get_state(sid)["finances"]["liquid"]["balance"] == 100.0
        assert engine.get_state(sid)["finances"]["pockets"]["rent"]["spent"] == 900.0

    def test_is_idempotent(self, engine: SustainEngine):
        sid = "legacy-sid-3"
        state = {"members": [], "finances": {"liquid": {"balance": 1.0}, "pockets": {}, "income": {"amount": 0.0, "frequency": "monthly", "last_recorded": None}, "goals": []}, "calendar": {"events": []}, "tasks": {"items": []}, "alerts": []}
        self._seed_pre_slice3_sustain(engine, sid, "homestead", state)

        report1 = engine.migrate_to_event_sourcing()
        assert sid in report1["migrated"]
        count_after_first = engine._db.execute(
            "SELECT COUNT(*) AS n FROM events WHERE sustain_id = ?", (sid,)
        ).fetchone()["n"]

        report2 = engine.migrate_to_event_sourcing()
        assert sid in report2["skipped_already_migrated"]
        assert sid not in report2["migrated"]
        count_after_second = engine._db.execute(
            "SELECT COUNT(*) AS n FROM events WHERE sustain_id = ?", (sid,)
        ).fetchone()["n"]
        assert count_after_first == count_after_second  # no duplicate genesis event

    def test_already_event_sourced_sustain_is_skipped(self, engine: SustainEngine, homestead_sid: str):
        """A sustain instantiated under the new code (genesis already present) must not be re-migrated."""
        report = engine.migrate_to_event_sourcing()
        assert homestead_sid in report["skipped_already_migrated"]
        assert homestead_sid not in report["migrated"]

    def test_orphaned_sustain_states_row_is_reported_not_touched(self, engine: SustainEngine):
        """
        Mirrors the real orphan found in the live DB before this slice: a
        sustain_states row with no matching sustains row (unreachable via the
        normal engine API — _get_spec requires a sustains row). Must be
        reported, not silently migrated or silently ignored.
        """
        import json as _json
        import uuid as _uuid

        orphan_sid = "orphan-sid"
        engine._db.execute(
            "INSERT INTO sustain_states (id, sustain_id, state_json, version_number, updated_at) "
            "VALUES (?, ?, ?, 1, ?)",
            (str(_uuid.uuid4()), orphan_sid, _json.dumps({"finances": {"pockets": {}}}), "2026-06-01T00:00:00"),
        )
        engine._db.commit()

        report = engine.migrate_to_event_sourcing()

        assert orphan_sid in report["orphaned_state_rows"]
        assert orphan_sid not in report["migrated"]
        # Untouched: still zero events for it, still no genesis.
        assert engine.get_events(orphan_sid, limit=10) == []

    def test_sustains_row_with_no_state_row_is_skipped_not_crashed(self, engine: SustainEngine):
        """A sustains row that somehow has no sustain_states row (never fully instantiated) must be reported, not crash the whole migration."""
        engine._db.execute(
            "INSERT INTO sustains (id, user_id, template_id, created_at) VALUES (?, ?, ?, ?)",
            ("no-state-sid", "u1", "homestead", "2026-06-01T00:00:00"),
        )
        engine._db.commit()

        report = engine.migrate_to_event_sourcing()

        assert "no-state-sid" in report["skipped_no_state"]
        assert "no-state-sid" not in report["migrated"]
