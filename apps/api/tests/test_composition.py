"""
tests/test_composition.py

Tests for SustainEngine's Slice 8 (Coordination & roll-up) additions:
  - link_child / list_children / get_parent / unlink_child (⊕)
  - provision_declared_children — generic bootstrap from spec["declared_children"]
  - compute_rollup (ρ) — computed-on-read aggregation, honest exclusion
  - Per-rule authority (Option C, confirmed with Bonnie): each invariant
    declares authority ("binding"|"advisory", default "advisory"). Advisory
    aggregate invariants stay display-only, same as the original "parent
    never vetoes" behaviour (still the default). Binding ones can refuse a
    CHILD's transition, but ONLY the specific ok-before -> not-ok-after
    transition — never an already-bad aggregate, never an unrelated action.
  - commit_external_mutation's opt-in check_gate parameter

Every fixture here uses SustainEngine.create_definition() (Slice 6) to build
synthetic parent/child specs rather than referencing homestead/habitat, so
these tests also double as proof the composition machinery is genuinely
generic — nothing here says "habitat" or "homestead".

Run with:
    python -m pytest tests/test_composition.py -v
"""

import json

import pytest

from sustena.core.operator import OperatorResult
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


def _make_parent_definition(engine: SustainEngine, aggregates=None, extra_invariants=None, extra_schema=None):
    """A synthetic parent with no dimensions of its own beyond one scalar,
    then aggregates/invariants/state_schema patched in directly (create_definition's
    flat dimension builder doesn't support declaring computed/aggregate
    dimensions, so this mirrors exactly what a real spec author would do by
    hand for an aggregate-referencing invariant)."""
    result = engine.create_definition(
        owner_user_id="u1", display_name="Pod", description="synthetic parent",
        dimensions=[{"name": "own_field", "type": "number", "default_value": 0}],
        invariants=[], operator_names=["edit.state_patch"],
    )
    tid = result["template_id"]
    spec = engine.get_definition(tid)["spec"]
    spec["aggregates"] = aggregates or []
    if extra_schema:
        spec["state_schema"].update(extra_schema)
    if extra_invariants:
        spec["invariants"] = extra_invariants
    engine._db.execute("UPDATE sustain_templates SET spec_json = ? WHERE id = ?", (json.dumps(spec), tid))
    engine._db.commit()
    return tid


# ── link_child / list_children / get_parent / unlink_child ──────────────────

class TestLinkChild:
    def test_link_creates_a_record(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})

        link = engine.link_child(parent, child, slot="c1", member="Leaf One")
        assert link["parent_sustain_id"] == parent
        assert link["child_sustain_id"] == child
        assert link["slot"] == "c1"
        assert link["member"] == "Leaf One"

    def test_parent_must_exist(self, engine: SustainEngine):
        ctid = _make_child_definition(engine)
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        with pytest.raises(ValueError, match="parent sustain"):
            engine.link_child("does-not-exist", child)

    def test_child_must_exist(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        with pytest.raises(ValueError, match="child sustain"):
            engine.link_child(parent, "does-not-exist")

    def test_cannot_link_self(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        with pytest.raises(ValueError, match="own child"):
            engine.link_child(parent, parent)

    def test_a_child_can_only_have_one_parent(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent1 = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        parent2 = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent1, child)
        with pytest.raises(ValueError, match="already linked"):
            engine.link_child(parent2, child)

    def test_slot_unique_per_parent(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child1 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        child2 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child1, slot="c1")
        with pytest.raises(ValueError):
            engine.link_child(parent, child2, slot="c1")


class TestListChildrenGetParent:
    def test_list_children_in_link_order(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        c1 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        c2 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, c1, slot="a")
        engine.link_child(parent, c2, slot="b")
        children = engine.list_children(parent)
        assert [c["child_sustain_id"] for c in children] == [c1, c2]

    def test_list_children_empty_for_a_non_parent(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        assert engine.list_children(parent) == []

    def test_get_parent_reverse_lookup(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child, slot="c1")
        found = engine.get_parent(child)
        assert found["parent_sustain_id"] == parent

    def test_get_parent_none_for_unlinked_child(self, engine: SustainEngine):
        ctid = _make_child_definition(engine)
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        assert engine.get_parent(child) is None


class TestUnlinkChild:
    def test_unlink_removes_the_link(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)
        assert engine.unlink_child(parent, child) is True
        assert engine.get_parent(child) is None

    def test_unlink_returns_false_when_no_link_exists(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        assert engine.unlink_child(parent, child) is False

    def test_child_keeps_its_own_state_after_unlink(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)
        before = engine.get_state(child)
        engine.unlink_child(parent, child)
        assert engine.get_state(child) == before


# ── provision_declared_children ───────────────────────────────────────────────

class TestProvisionDeclaredChildren:
    def _parent_with_declared_children(self, engine: SustainEngine, child_template: str, n=3):
        tid = _make_parent_definition(engine)
        spec = engine.get_definition(tid)["spec"]
        spec["declared_children"] = [
            {"slot": f"slot_{i}", "member": f"Member {i}", "template": child_template}
            for i in range(n)
        ]
        engine._db.execute("UPDATE sustain_templates SET spec_json = ? WHERE id = ?", (json.dumps(spec), tid))
        engine._db.commit()
        return tid

    def test_provisions_every_declared_slot(self, engine: SustainEngine):
        ctid = _make_child_definition(engine)
        ptid = self._parent_with_declared_children(engine, ctid, n=3)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})

        links = engine.provision_declared_children(parent, "u1")
        assert len(links) == 3
        assert {l["slot"] for l in links} == {"slot_0", "slot_1", "slot_2"}
        assert len(engine.list_children(parent)) == 3

    def test_provisioned_children_start_empty(self, engine: SustainEngine):
        ctid = _make_child_definition(engine)
        ptid = self._parent_with_declared_children(engine, ctid, n=1)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        links = engine.provision_declared_children(parent, "u1")
        child_state = engine.get_state(links[0]["child_sustain_id"])
        assert child_state["moisture_level"] == 0  # never fabricated

    def test_idempotent_second_call_creates_nothing_new(self, engine: SustainEngine):
        ctid = _make_child_definition(engine)
        ptid = self._parent_with_declared_children(engine, ctid, n=2)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        first = engine.provision_declared_children(parent, "u1")
        second = engine.provision_declared_children(parent, "u1")
        assert [l["child_sustain_id"] for l in first] == [l["child_sustain_id"] for l in second]
        assert len(engine.list_children(parent)) == 2

    def test_provision_on_unknown_parent_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="not found"):
            engine.provision_declared_children("does-not-exist", "u1")

    def test_declared_child_with_no_template_raises(self, engine: SustainEngine):
        tid = _make_parent_definition(engine)
        spec = engine.get_definition(tid)["spec"]
        spec["declared_children"] = [{"slot": "x", "member": "X"}]
        engine._db.execute("UPDATE sustain_templates SET spec_json = ? WHERE id = ?", (json.dumps(spec), tid))
        engine._db.commit()
        parent = engine.instantiate(tid, "u1", {"owner_ids": ["u1"]})
        with pytest.raises(ValueError, match="no template"):
            engine.provision_declared_children(parent, "u1")

    def test_parent_with_no_declared_children_is_a_no_op(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        assert engine.provision_declared_children(parent, "u1") == []


# ── compute_rollup ─────────────────────────────────────────────────────────────

class TestComputeRollup:
    def _linked_pod(self, engine: SustainEngine, n_children=2):
        ptid = _make_parent_definition(engine, aggregates=[
            {"id": "pod_total", "child_path": "moisture_level", "op": "sum"},
        ])
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        children = []
        for i in range(n_children):
            cid = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
            engine.link_child(parent, cid, slot=f"c{i}", member=f"Leaf {i}")
            children.append(cid)
        return parent, children

    def test_sum_of_fresh_zero_children_is_honestly_zero(self, engine: SustainEngine):
        parent, _ = self._linked_pod(engine, n_children=6)
        rollup = engine.compute_rollup(parent)
        assert rollup["aggregates"]["pod_total"]["value"] == 0

    @pytest.mark.asyncio
    async def test_sum_reflects_a_real_child_change(self, engine: SustainEngine):
        parent, children = self._linked_pod(engine, n_children=3)
        await engine.execute_operator(children[0], "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 42}],
        })
        rollup = engine.compute_rollup(parent)
        assert rollup["aggregates"]["pod_total"]["value"] == 42

    @pytest.mark.asyncio
    async def test_manual_sum_matches_computed_rollup(self, engine: SustainEngine):
        parent, children = self._linked_pod(engine, n_children=3)
        values = [10, 20, 30]
        for cid, v in zip(children, values):
            await engine.execute_operator(cid, "edit.state_patch", {
                "patch": [{"op": "replace", "path": "moisture_level", "value": v}],
            })
        rollup = engine.compute_rollup(parent)
        assert rollup["aggregates"]["pod_total"]["value"] == sum(values)

    @pytest.mark.asyncio
    async def test_recompute_reproduces_identical_result(self, engine: SustainEngine):
        parent, children = self._linked_pod(engine, n_children=2)
        await engine.execute_operator(children[0], "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 7}],
        })
        first = engine.compute_rollup(parent)
        second = engine.compute_rollup(parent)
        assert first == second

    def test_missing_child_excluded_not_zeroed(self, engine: SustainEngine):
        import uuid as _uuid
        parent, children = self._linked_pod(engine, n_children=1)
        ghost_id = str(_uuid.uuid4())
        engine._db.execute(
            "INSERT INTO sustain_composition (id, parent_sustain_id, child_sustain_id, slot, member, linked_at) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (str(_uuid.uuid4()), parent, ghost_id, "ghost", "Ghost", "2020-01-01T00:00:00"),
        )
        engine._db.commit()

        rollup = engine.compute_rollup(parent)
        agg = rollup["aggregates"]["pod_total"]
        assert len(agg["included"]) == 1
        assert len(agg["excluded"]) == 1
        assert agg["excluded"][0]["sustain_id"] == ghost_id
        assert agg["value"] == 0  # only the one real, zero-balance child

        ghost_report = next(c for c in rollup["children"] if c["child_sustain_id"] == ghost_id)
        assert ghost_report["status"] == "missing"

    def test_unknown_op_raises(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine, aggregates=[
            {"id": "bad", "child_path": "moisture_level", "op": "median"},
        ])
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        with pytest.raises(ValueError, match="unknown op"):
            engine.compute_rollup(parent)

    def test_unknown_parent_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="not found"):
            engine.compute_rollup("does-not-exist")

    @pytest.mark.asyncio
    async def test_avg_min_max_count_ops(self, engine: SustainEngine):
        ptid = _make_parent_definition(engine, aggregates=[
            {"id": "t_avg", "child_path": "moisture_level", "op": "avg"},
            {"id": "t_min", "child_path": "moisture_level", "op": "min"},
            {"id": "t_max", "child_path": "moisture_level", "op": "max"},
            {"id": "t_count", "child_path": "moisture_level", "op": "count"},
        ])
        ctid = _make_child_definition(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        vals = [10, 20, 30]
        for v in vals:
            cid = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
            engine.link_child(parent, cid)
            await engine.execute_operator(cid, "edit.state_patch", {
                "patch": [{"op": "replace", "path": "moisture_level", "value": v}],
            })
        rollup = engine.compute_rollup(parent)
        assert rollup["aggregates"]["t_avg"]["value"] == 20
        assert rollup["aggregates"]["t_min"]["value"] == 10
        assert rollup["aggregates"]["t_max"]["value"] == 30
        assert rollup["aggregates"]["t_count"]["value"] == 3


# ── Parent observes, never vetoes ──────────────────────────────────────────────

class TestParentObservesNeverVetoes:
    def _parent_with_aggregate_invariant(self, engine: SustainEngine):
        return _make_parent_definition(
            engine,
            aggregates=[{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}],
            extra_schema={"pod_total": {"type": "number", "description": "computed"}},
            extra_invariants=[{"id": "pod_floor", "expression": "pod_total >= 0", "description": "observational"}],
        )

    def test_aggregate_invariant_is_marked_not_enforced(self, engine: SustainEngine):
        ptid = self._parent_with_aggregate_invariant(engine)
        spec = engine._load_spec(ptid)
        engine._compile_spec_invariants(spec)
        inv = spec["_compiled_invariants"][0]
        assert inv["id"] == "pod_floor"
        assert inv["enforced"] is False

    def test_normal_invariant_stays_enforced(self, engine: SustainEngine):
        ptid = _make_parent_definition(
            engine,
            extra_invariants=[{"id": "own_floor", "expression": "own_field >= 0", "description": ""}],
        )
        spec = engine._load_spec(ptid)
        engine._compile_spec_invariants(spec)
        inv = spec["_compiled_invariants"][0]
        assert inv["enforced"] is True

    @pytest.mark.asyncio
    async def test_child_op_succeeds_even_though_it_would_violate_parent_aggregate(self, engine: SustainEngine):
        ptid = self._parent_with_aggregate_invariant(engine)
        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)

        result = await engine.execute_operator(child, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -50}],
        })
        assert result.succeeded
        assert engine.compute_rollup(parent)["aggregates"]["pod_total"]["value"] == -50

    @pytest.mark.asyncio
    async def test_parent_own_unrelated_operator_still_succeeds(self, engine: SustainEngine):
        ptid = self._parent_with_aggregate_invariant(engine)
        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)
        await engine.execute_operator(child, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -50}],
        })

        # The parent's own aggregate now (observably) violates pod_floor —
        # an UNRELATED operator on the parent itself must still succeed.
        result = await engine.execute_operator(parent, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "own_field", "value": 1}],
        })
        assert result.succeeded

    @pytest.mark.asyncio
    async def test_violated_aggregate_invariant_shown_as_failing_in_display(self, engine: SustainEngine):
        ptid = self._parent_with_aggregate_invariant(engine)
        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)
        await engine.execute_operator(child, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -50}],
        })
        constraints = engine.evaluate_constraints(parent)
        assert any(c["expr"] == "pod_total >= 0" and c["status"] == "fail" for c in constraints)

    @pytest.mark.asyncio
    async def test_healthy_aggregate_shows_ok_in_display(self, engine: SustainEngine):
        ptid = self._parent_with_aggregate_invariant(engine)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        constraints = engine.evaluate_constraints(parent)
        assert any(c["expr"] == "pod_total >= 0" and c["status"] == "ok" for c in constraints)


# ── commit_external_mutation's check_gate ─────────────────────────────────────

class TestCommitExternalMutationGate:
    def test_no_op_when_nothing_changed(self, engine: SustainEngine):
        from sustena.core.state import StateAccessor
        ctid = _make_child_definition(engine)
        sid = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        state = StateAccessor(engine.get_state(sid))
        ok, reason = engine.commit_external_mutation(sid, state, "event.test", {}, check_gate=True)
        assert ok is True
        assert reason == ""

    def test_check_gate_false_preserves_old_unchecked_behavior(self, engine: SustainEngine):
        from sustena.core.state import StateAccessor
        ptid = _make_parent_definition(
            engine, extra_invariants=[{"id": "own_floor", "expression": "own_field >= 0", "description": ""}],
        )
        sid = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        state = StateAccessor(engine.get_state(sid))
        state.set("own_field", -999)  # would violate own_floor
        ok, reason = engine.commit_external_mutation(sid, state, "event.test", {}, check_gate=False)
        assert ok is True
        assert engine.get_state(sid)["own_field"] == -999

    def test_check_gate_true_refuses_a_real_violation(self, engine: SustainEngine):
        from sustena.core.state import StateAccessor
        ptid = _make_parent_definition(
            engine, extra_invariants=[{"id": "own_floor", "expression": "own_field >= 0", "description": ""}],
        )
        sid = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        before = engine.get_state(sid)
        state = StateAccessor(engine.get_state(sid))
        state.set("own_field", -999)
        ok, reason = engine.commit_external_mutation(sid, state, "event.test", {}, check_gate=True)
        assert ok is False
        assert "own_floor" in reason
        assert engine.get_state(sid) == before  # untouched

    def test_check_gate_true_still_succeeds_when_no_violation(self, engine: SustainEngine):
        from sustena.core.state import StateAccessor
        ptid = _make_parent_definition(
            engine, extra_invariants=[{"id": "own_floor", "expression": "own_field >= 0", "description": ""}],
        )
        sid = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        state = StateAccessor(engine.get_state(sid))
        state.set("own_field", 5)
        ok, reason = engine.commit_external_mutation(sid, state, "event.test", {}, check_gate=True)
        assert ok is True
        assert engine.get_state(sid)["own_field"] == 5

    def test_check_gate_true_skips_aggregate_referencing_invariants(self, engine: SustainEngine):
        from sustena.core.state import StateAccessor
        ptid = _make_parent_definition(
            engine,
            aggregates=[{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}],
            extra_schema={"pod_total": {"type": "number", "description": "computed"}},
            extra_invariants=[{"id": "pod_floor", "expression": "pod_total >= 0", "description": ""}],
        )
        sid = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        state = StateAccessor(engine.get_state(sid))
        state.set("own_field", 1)  # unrelated to pod_total; must not be blocked by pod_floor
        ok, reason = engine.commit_external_mutation(sid, state, "event.test", {}, check_gate=True)
        assert ok is True


# ── Per-rule authority (Option C) — binding vs advisory ───────────────────────

class TestPerRuleAuthority:
    def _binding_pod(self, engine: SustainEngine):
        ptid = _make_parent_definition(
            engine,
            aggregates=[{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}],
            extra_schema={"pod_total": {"type": "number", "description": "computed"}},
            extra_invariants=[{
                "id": "pod_floor", "expression": "pod_total >= 0", "description": "",
                "authority": "binding",
            }],
        )
        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        return ptid, ctid, parent

    def test_binding_invariant_compiles_with_authority_binding(self, engine: SustainEngine):
        ptid, _, _ = self._binding_pod(engine)
        spec = engine._load_spec(ptid)
        engine._compile_spec_invariants(spec)
        inv = spec["_compiled_invariants"][0]
        assert inv["authority"] == "binding"
        assert inv["is_aggregate"] is True
        assert inv["enforced"] is False  # still never gates the PARENT's own operator calls

    def test_default_authority_is_advisory(self, engine: SustainEngine):
        ptid = _make_parent_definition(
            engine,
            aggregates=[{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}],
            extra_schema={"pod_total": {"type": "number", "description": "computed"}},
            extra_invariants=[{"id": "pod_floor", "expression": "pod_total >= 0", "description": ""}],
        )
        spec = engine._load_spec(ptid)
        engine._compile_spec_invariants(spec)
        assert spec["_compiled_invariants"][0]["authority"] == "advisory"

    def test_create_definition_rejects_invalid_authority(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="authority"):
            engine.create_definition(
                owner_user_id="u1", display_name="Bad", description="",
                dimensions=[{"name": "x", "type": "number", "default_value": 0}],
                invariants=[{"id": "r", "expression": "x >= 0", "authority": "mandatory"}],
                operator_names=[],
            )

    @pytest.mark.asyncio
    async def test_binding_refuses_the_transition_that_newly_breaches_it(self, engine: SustainEngine):
        ptid, ctid, parent = self._binding_pod(engine)
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child, slot="c1")

        result = await engine.execute_operator(child, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -50}],
        })
        assert not result.succeeded
        assert result.constraint_violated == "parent_binding_gate"
        assert "pod_floor" in result.reason
        assert engine.get_state(child)["moisture_level"] == 0  # untouched

    @pytest.mark.asyncio
    async def test_binding_allows_a_transition_that_stays_compliant(self, engine: SustainEngine):
        ptid, ctid, parent = self._binding_pod(engine)
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child, slot="c1")

        result = await engine.execute_operator(child, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 30}],
        })
        assert result.succeeded

    @pytest.mark.asyncio
    async def test_binding_across_two_children_refuses_only_the_breaching_one(self, engine: SustainEngine):
        ptid, ctid, parent = self._binding_pod(engine)
        c1 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        c2 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, c1, slot="c1")
        engine.link_child(parent, c2, slot="c2")

        # c1 = 30, c2 = -10 -> total 20, still compliant.
        r1 = await engine.execute_operator(c1, "edit.state_patch", {"patch": [{"op": "replace", "path": "moisture_level", "value": 30}]})
        r2 = await engine.execute_operator(c2, "edit.state_patch", {"patch": [{"op": "replace", "path": "moisture_level", "value": -10}]})
        assert r1.succeeded and r2.succeeded

        # Now push c1 down so total would go negative -- refused, c1 untouched.
        r3 = await engine.execute_operator(c1, "edit.state_patch", {"patch": [{"op": "replace", "path": "moisture_level", "value": -25}]})
        assert not r3.succeeded
        assert engine.get_state(c1)["moisture_level"] == 30
        assert engine.compute_rollup(parent)["aggregates"]["pod_total"]["value"] == 20

    @pytest.mark.asyncio
    async def test_advisory_never_blocks_even_when_it_would_breach(self, engine: SustainEngine):
        ptid = _make_parent_definition(
            engine,
            aggregates=[{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}],
            extra_schema={"pod_total": {"type": "number", "description": "computed"}},
            extra_invariants=[{"id": "pod_floor", "expression": "pod_total >= 0", "description": ""}],  # advisory (default)
        )
        ctid = _make_child_definition(engine, minimum=-999)
        parent = engine.instantiate(ptid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)

        result = await engine.execute_operator(child, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -50}],
        })
        assert result.succeeded  # advisory: applies unconditionally
        assert engine.compute_rollup(parent)["aggregates"]["pod_total"]["value"] == -50
        constraints = engine.evaluate_constraints(parent)
        assert any(c["expr"] == "pod_total >= 0" and c["status"] == "fail" for c in constraints)

    @pytest.mark.asyncio
    async def test_binding_does_not_block_an_already_violating_aggregate_from_an_unrelated_action(self, engine: SustainEngine):
        # Regression guard for the "was_ok before" check: an aggregate that's
        # ALREADY in breach (for whatever reason) must not turn into a
        # permanent block on every future, unrelated child operation.
        ptid, ctid, parent = self._binding_pod(engine)
        c1 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        c2 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, c1, slot="c1")

        # Establish the breach directly via commit_external_mutation (bypasses
        # the binding gate, matching the S2-era test pattern for forcing a bad
        # state) so we can prove the SEPARATE child's unrelated op is unblocked.
        from sustena.core.state import StateAccessor as _SA
        state = _SA(engine.get_state(c1))
        state.set("moisture_level", -999)
        engine.commit_external_mutation(c1, state, "event.test.forced", {}, check_gate=False)
        assert engine.compute_rollup(parent)["aggregates"]["pod_total"]["value"] == -999

        engine.link_child(parent, c2, slot="c2")
        result = await engine.execute_operator(c2, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 0}],
        })
        assert result.succeeded  # total stays -999, unchanged -- not a NEW breach

    @pytest.mark.asyncio
    async def test_binding_allows_improving_an_already_bad_aggregate(self, engine: SustainEngine):
        ptid, ctid, parent = self._binding_pod(engine)
        c1 = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, c1, slot="c1")

        from sustena.core.state import StateAccessor as _SA
        state = _SA(engine.get_state(c1))
        state.set("moisture_level", -999)
        engine.commit_external_mutation(c1, state, "event.test.forced", {}, check_gate=False)

        # Still negative afterward, but strictly better -- must not be refused.
        result = await engine.execute_operator(c1, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -10}],
        })
        assert result.succeeded

    @pytest.mark.asyncio
    async def test_binding_has_no_effect_on_an_unlinked_sustain(self, engine: SustainEngine):
        ctid = _make_child_definition(engine, minimum=-999)
        standalone = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        result = await engine.execute_operator(standalone, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -50}],
        })
        assert result.succeeded

    @pytest.mark.asyncio
    async def test_binding_requires_parent_enforcement_enabled(self, engine: SustainEngine):
        ptid, ctid, parent = self._binding_pod(engine)
        # Disable enforcement on the parent after the fact.
        spec = engine.get_definition(ptid)["spec"]
        spec["enforcement"] = {"enabled": False}
        engine._db.execute("UPDATE sustain_templates SET spec_json = ? WHERE id = ?", (json.dumps(spec), ptid))
        engine._db.commit()
        engine._specs.pop(parent, None)

        child = engine.instantiate(ctid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child)
        result = await engine.execute_operator(child, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -50}],
        })
        assert result.succeeded  # parent enforcement off -> binding rule doesn't apply
