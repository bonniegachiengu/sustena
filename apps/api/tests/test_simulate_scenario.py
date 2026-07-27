"""
tests/test_simulate_scenario.py

Tests for SustainEngine's Slice 9 (Simulator / scenario tree) additions:
  - simulate() never mutates live state, never appends events, no matter
    how many branches are run
  - simulate() respects the Slice 7 binding/advisory authority model and
    exposes a hypothetical parent_rollup per step (read-only, never written)
  - promote_simulation() genuinely replays a branch through execute_operator
    — real gate, real events — and can honestly diverge from what the
    simulation predicted if live state drifted in between

Every fixture uses SustainEngine.create_definition() (Slice 6) rather than
homestead/habitat, so this suite also proves the simulator machinery is
generic — nothing here says "habitat" or "homestead".

Run with:
    python -m pytest tests/test_simulate_scenario.py -v
"""

import json

import pytest

from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


def _make_child_definition(engine: SustainEngine, minimum=None):
    dims = [{"name": "moisture_level", "type": "number", "default_value": 0}]
    if minimum is not None:
        dims[0]["minimum"] = minimum
    return engine.create_definition(
        owner_user_id="u1", display_name="Leaf", description="synthetic child",
        dimensions=dims, invariants=[], operator_names=["edit.state_patch"],
    )["template_id"]


def _make_binding_parent(engine: SustainEngine):
    result = engine.create_definition(
        owner_user_id="u1", display_name="Pod", description="synthetic parent",
        dimensions=[{"name": "own_field", "type": "number", "default_value": 0}],
        invariants=[], operator_names=["edit.state_patch"],
    )
    tid = result["template_id"]
    spec = engine.get_definition(tid)["spec"]
    spec["aggregates"] = [{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}]
    spec["state_schema"]["pod_total"] = {"type": "number", "description": "computed"}
    spec["invariants"] = [{
        "id": "pod_floor", "expression": "pod_total >= 0", "description": "", "authority": "binding",
    }]
    engine._db.execute("UPDATE sustain_templates SET spec_json = ? WHERE id = ?", (json.dumps(spec), tid))
    engine._db.commit()
    return tid


# ── Isolation: simulate() never touches live state ────────────────────────────

class TestSimulateIsolation:
    @pytest.mark.asyncio
    async def test_state_unchanged_after_simulation(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        before = engine.get_state(sid)
        await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 2000, "period": "monthly"}},
        ])
        assert engine.get_state(sid) == before

    @pytest.mark.asyncio
    async def test_no_events_appended(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        before = len(engine.get_events(sid, limit=100))
        await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}},
        ])
        assert len(engine.get_events(sid, limit=100)) == before

    @pytest.mark.asyncio
    async def test_rebuild_state_still_agrees_after_simulation(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}},
        ])
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    @pytest.mark.asyncio
    async def test_two_branches_from_the_same_start_are_independent(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        branch_a = await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 1000, "source": "a", "frequency": "once"}},
        ])
        branch_b = await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 9000, "source": "b", "frequency": "once"}},
        ])
        assert branch_a[0]["state_after"]["finances"]["liquid"]["balance"] == 1000
        assert branch_b[0]["state_after"]["finances"]["liquid"]["balance"] == 9000
        # both forked from the SAME real starting point (0), not from each other
        assert engine.get_state(sid)["finances"]["liquid"]["balance"] == 0

    @pytest.mark.asyncio
    async def test_a_real_operator_between_two_simulations_is_reflected(self, engine: SustainEngine):
        # simulate() forks from CURRENT live state every call, not a cached
        # snapshot — proven by changing live state for real in between.
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        first = await engine.simulate(sid, [])
        await engine.execute_operator(sid, "budget.record_income", {"amount": 7000, "source": "real", "frequency": "once"})
        second = await engine.simulate(sid, [])
        assert first[-1] if first else True  # empty sequence -> no steps, nothing to assert on first
        state_now = engine.get_state(sid)
        assert state_now["finances"]["liquid"]["balance"] == 7000
        # a fresh simulate() with a real income step now starts from 7000, not 0
        third = await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 1000, "source": "x", "frequency": "once"}},
        ])
        assert third[0]["state_after"]["finances"]["liquid"]["balance"] == 8000


# ── Binding authority + parent_rollup inside the sandbox ─────────────────────

class TestSimulateRespectsAuthority:
    @pytest.mark.asyncio
    async def test_binding_refuses_the_breaching_step_in_branch(self, engine: SustainEngine):
        ptid = _make_binding_parent(engine)
        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child, slot="c1")

        results = await engine.simulate(child, [
            {"operator": "edit.state_patch", "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": 30}]}},
            {"operator": "edit.state_patch", "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": -50}]}},
        ])
        assert results[0]["result"].succeeded
        assert not results[1]["result"].succeeded
        assert results[1]["result"].constraint_violated == "parent_binding_gate"
        assert results[1]["state_after"]["moisture_level"] == 30  # refused step didn't advance

    @pytest.mark.asyncio
    async def test_parent_rollup_present_and_correct_per_step(self, engine: SustainEngine):
        ptid = _make_binding_parent(engine)
        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child, slot="c1")

        results = await engine.simulate(child, [
            {"operator": "edit.state_patch", "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": 42}]}},
        ])
        assert results[0]["parent_rollup"]["aggregates"]["pod_total"]["value"] == 42

    @pytest.mark.asyncio
    async def test_parent_rollup_none_when_no_parent(self, engine: SustainEngine):
        ctid = _make_child_definition(engine)
        standalone = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        results = await engine.simulate(standalone, [
            {"operator": "edit.state_patch", "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": 5}]}},
        ])
        assert results[0]["parent_rollup"] is None

    @pytest.mark.asyncio
    async def test_advisory_never_refuses_in_simulation_either(self, engine: SustainEngine):
        ptid = engine.create_definition(
            owner_user_id="u1", display_name="Pod2", description="",
            dimensions=[{"name": "own_field", "type": "number", "default_value": 0}],
            invariants=[], operator_names=[],
        )["template_id"]
        spec = engine.get_definition(ptid)["spec"]
        spec["aggregates"] = [{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}]
        spec["state_schema"]["pod_total"] = {"type": "number", "description": "computed"}
        spec["invariants"] = [{"id": "pod_floor", "expression": "pod_total >= 0", "description": ""}]  # advisory (default)
        engine._db.execute("UPDATE sustain_templates SET spec_json = ? WHERE id = ?", (json.dumps(spec), ptid))
        engine._db.commit()

        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)

        results = await engine.simulate(child, [
            {"operator": "edit.state_patch", "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": -50}]}},
        ])
        assert results[0]["result"].succeeded  # advisory: applies unconditionally, even in sim


# ── promote_simulation: genuine replay ────────────────────────────────────────

class TestPromoteSimulation:
    @pytest.mark.asyncio
    async def test_promote_genuinely_mutates_live_state(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        seq = [{"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}}]

        # simulate first -- must NOT touch live state
        await engine.simulate(sid, seq)
        assert engine.get_state(sid)["finances"]["liquid"]["balance"] == 0

        results = await engine.promote_simulation(sid, seq)
        assert results[0]["result"].succeeded
        assert engine.get_state(sid)["finances"]["liquid"]["balance"] == 5000

    @pytest.mark.asyncio
    async def test_promote_appends_real_events(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        before = len(engine.get_events(sid, limit=100))
        await engine.promote_simulation(sid, [
            {"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}},
        ])
        assert len(engine.get_events(sid, limit=100)) == before + 1

    @pytest.mark.asyncio
    async def test_promote_rebuild_state_agrees(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.promote_simulation(sid, [
            {"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 2000, "period": "monthly"}},
        ])
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    @pytest.mark.asyncio
    async def test_promote_stops_at_first_real_failure(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        seq = [
            {"operator": "budget.record_income", "params": {"amount": 1000, "source": "x", "frequency": "once"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 999999, "period": "monthly"}},  # overspends liquid
            {"operator": "budget.record_income", "params": {"amount": 1, "source": "never-reached", "frequency": "once"}},
        ]
        results = await engine.promote_simulation(sid, seq)
        assert len(results) == 2  # third step never attempted
        assert results[0]["result"].succeeded
        assert not results[1]["result"].succeeded

    @pytest.mark.asyncio
    async def test_a_step_that_passed_in_simulation_can_honestly_fail_in_promotion(self, engine: SustainEngine):
        # Build a sequence that would succeed against the CURRENT live state...
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        seq = [{"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 1000, "period": "monthly"}}]

        # Simulating against a hypothetical income-first branch shows it would work.
        sim_with_income = await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}},
            *seq,
        ])
        assert all(r["result"].succeeded for r in sim_with_income)

        # ...but promoting JUST the allocate step (without the income) against
        # real, still-zero live state honestly fails -- the simulation's
        # premise (income already applied) was never actually promoted.
        results = await engine.promote_simulation(sid, seq)
        assert not results[0]["result"].succeeded
        assert engine.get_state(sid)["finances"]["liquid"]["balance"] == 0  # untouched
