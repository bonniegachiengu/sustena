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
