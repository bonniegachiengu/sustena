"""
tests/test_definitions.py

Tests for SustainEngine's Slice 6 (Create/Definition flow) additions:
  - _load_spec() disk-first, DB-fallback resolution
  - create_definition() / get_definition() / list_definitions()
  - check_definition_edit_safety() — the migration predicate
  - update_definition() — safe edits, refusal, cache invalidation
  - a user-created definition inherits the S2 gate and S3 fold exactly
    like homestead/habitat, through the SAME instantiate()/execute_operator()
    code path (no special-casing)

Run with:
    python -m pytest tests/test_definitions.py -v
"""

import pytest

from sustena.core.sustain_engine import SustainEngine


# ── Fixtures ──────────────────────────────────────────────────────────────────

@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


def _plant_tracker_kwargs(**overrides):
    kwargs = dict(
        owner_user_id="user-1",
        display_name="Plant Tracker",
        description="Tracks a houseplant",
        dimensions=[
            {"name": "moisture_level", "type": "number", "description": "soil moisture", "default_value": 50.0, "minimum": 0},
            {"name": "plant_name", "type": "string", "description": "name", "default_value": "Fern"},
        ],
        invariants=[
            {"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": "never negative"},
        ],
        operator_names=["edit.state_patch"],
    )
    kwargs.update(overrides)
    return kwargs


# ── create_definition ──────────────────────────────────────────────────────────

class TestCreateDefinition:
    def test_create_returns_template_id_and_spec(self, engine: SustainEngine):
        result = engine.create_definition(**_plant_tracker_kwargs())
        assert result["template_id"]
        assert result["version"] == 1
        assert result["spec"]["display_name"] == "Plant Tracker"

    def test_state_schema_built_from_dimensions(self, engine: SustainEngine):
        result = engine.create_definition(**_plant_tracker_kwargs())
        schema = result["spec"]["state_schema"]
        assert schema["moisture_level"]["type"] == "number"
        assert schema["moisture_level"]["minimum"] == 0
        assert schema["plant_name"]["type"] == "string"

    def test_default_state_built_from_dimensions(self, engine: SustainEngine):
        result = engine.create_definition(**_plant_tracker_kwargs())
        assert result["spec"]["default_state"] == {"moisture_level": 50.0, "plant_name": "Fern"}

    def test_missing_default_value_falls_back_to_type_zero_value(self, engine: SustainEngine):
        result = engine.create_definition(**_plant_tracker_kwargs(dimensions=[
            {"name": "is_healthy", "type": "boolean"},
        ], invariants=[], operator_names=[]))
        assert result["spec"]["default_state"] == {"is_healthy": False}

    def test_enforcement_always_enabled(self, engine: SustainEngine):
        result = engine.create_definition(**_plant_tracker_kwargs())
        assert result["spec"]["enforcement"]["enabled"] is True

    def test_operator_params_auto_derived_from_signature(self, engine: SustainEngine):
        result = engine.create_definition(**_plant_tracker_kwargs())
        op = result["spec"]["operators"][0]
        assert op["name"] == "edit.state_patch"
        assert set(op["params"]) == {"sustain_id", "patch"}

    def test_invalid_dimension_type_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="type must be one of"):
            engine.create_definition(**_plant_tracker_kwargs(dimensions=[
                {"name": "x", "type": "object"},
            ]))

    def test_dimension_without_name_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError):
            engine.create_definition(**_plant_tracker_kwargs(dimensions=[
                {"name": "", "type": "number"},
            ]))

    def test_duplicate_dimension_name_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="Duplicate dimension"):
            engine.create_definition(**_plant_tracker_kwargs(dimensions=[
                {"name": "x", "type": "number"},
                {"name": "x", "type": "string"},
            ]))

    def test_unknown_operator_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="Unknown operator"):
            engine.create_definition(**_plant_tracker_kwargs(operator_names=["not.a.real.operator"]))

    def test_invariant_missing_id_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError):
            engine.create_definition(**_plant_tracker_kwargs(invariants=[
                {"id": "", "expression": "moisture_level >= 0"},
            ]))

    def test_invariant_that_fails_to_compile_raises(self, engine: SustainEngine):
        # References a dimension that doesn't exist in state_schema.
        with pytest.raises(ValueError, match="is invalid"):
            engine.create_definition(**_plant_tracker_kwargs(invariants=[
                {"id": "bad", "expression": "not_a_real_field >= 0"},
            ]))

    def test_invalid_invariant_never_persisted(self, engine: SustainEngine):
        # Nothing should be written if creation is refused.
        try:
            engine.create_definition(**_plant_tracker_kwargs(invariants=[
                {"id": "bad", "expression": "not_a_real_field >= 0"},
            ]))
        except ValueError:
            pass
        assert engine.list_definitions() == []


# ── get_definition / list_definitions ─────────────────────────────────────────

class TestReadDefinitions:
    def test_get_definition_returns_full_record(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        got = engine.get_definition(created["template_id"])
        assert got["owner_user_id"] == "user-1"
        assert got["version"] == 1
        assert got["spec"]["display_name"] == "Plant Tracker"

    def test_get_unknown_definition_returns_none(self, engine: SustainEngine):
        assert engine.get_definition("does-not-exist") is None

    def test_list_definitions_scoped_to_owner(self, engine: SustainEngine):
        engine.create_definition(**_plant_tracker_kwargs(owner_user_id="user-1"))
        engine.create_definition(**_plant_tracker_kwargs(owner_user_id="user-2", display_name="Other"))
        mine = engine.list_definitions(owner_user_id="user-1")
        assert len(mine) == 1
        assert mine[0]["display_name"] == "Plant Tracker"

    def test_list_definitions_all_when_no_owner_given(self, engine: SustainEngine):
        engine.create_definition(**_plant_tracker_kwargs(owner_user_id="user-1"))
        engine.create_definition(**_plant_tracker_kwargs(owner_user_id="user-2", display_name="Other"))
        assert len(engine.list_definitions()) == 2

    def test_list_definitions_summary_counts(self, engine: SustainEngine):
        engine.create_definition(**_plant_tracker_kwargs())
        summary = engine.list_definitions(owner_user_id="user-1")[0]
        assert summary["dimension_count"] == 2
        assert summary["invariant_count"] == 1
        assert summary["operator_count"] == 1


# ── Inherits the gate + fold — no special-casing ──────────────────────────────

class TestInheritsGateAndFold:
    def test_load_spec_resolves_a_definition_template_id(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        spec = engine._load_spec(created["template_id"])
        assert spec["display_name"] == "Plant Tracker"

    def test_disk_templates_still_resolve_unaffected(self, engine: SustainEngine):
        # Built-ins must be completely unaffected by the DB-fallback addition.
        spec = engine._load_spec("homestead")
        assert spec["id"] == "homestead"

    def test_instantiate_a_definition_the_same_way_as_homestead(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        assert engine.get_state(sid) == {"moisture_level": 50.0, "plant_name": "Fern"}

    def test_instantiate_writes_a_genesis_event_like_any_sustain(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        events = engine.get_events(sid, limit=10)
        assert len(events) == 1
        assert events[0]["event_name"] == "event.system.genesis_snapshot"

    def test_list_all_uses_the_definitions_display_name_as_label(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        entry = next(s for s in engine.list_all() if s["id"] == sid)
        assert entry["label"] == "Plant Tracker"

    @pytest.mark.asyncio
    async def test_attached_operator_that_passes_applies(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        result = await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 10}],
        })
        assert result.succeeded
        assert engine.get_state(sid)["moisture_level"] == 10

    @pytest.mark.asyncio
    async def test_gate_refuses_a_transition_that_breaks_the_invariant(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        result = await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -5}],
        })
        assert not result.succeeded
        assert result.constraint_violated == "enforcement_gate"
        assert "moisture_non_negative" in result.reason
        assert engine.get_state(sid)["moisture_level"] == 50.0  # unchanged

    @pytest.mark.asyncio
    async def test_unattached_operator_is_refused(self, engine: SustainEngine):
        # budget.allocate was never attached to this definition — the spec's
        # own operators allowlist must refuse it exactly like any sustain.
        created = engine.create_definition(**_plant_tracker_kwargs())
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        result = await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 100})
        assert not result.succeeded
        assert result.constraint_violated == "operator_allowed"

    @pytest.mark.asyncio
    async def test_rebuild_state_agrees_after_operator_calls(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 33}],
        })
        assert engine.rebuild_state(sid) == engine.get_state(sid)


# ── check_definition_edit_safety / update_definition — the migration predicate ─

class TestMigrationPredicate:
    @pytest.mark.asyncio
    async def test_edit_refused_when_it_would_strand_a_live_instance(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs(invariants=[]))
        sid = engine.instantiate(created["template_id"], "user-1", {"owner_ids": ["user-1"]})
        # No invariant yet, so this succeeds and leaves the instance at -5.
        result = await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -5}],
        })
        assert result.succeeded

        edit_result = engine.update_definition(created["template_id"], "user-1", {
            "invariants": [{"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": ""}],
        })
        assert edit_result["status"] == "refused"
        assert len(edit_result["blocked_by"]) == 1
        assert edit_result["blocked_by"][0]["sustain_id"] == sid
        assert edit_result["blocked_by"][0]["invariant_id"] == "moisture_non_negative"

    @pytest.mark.asyncio
    async def test_refused_edit_leaves_version_and_invariants_unchanged(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs(invariants=[]))
        tid = created["template_id"]
        sid = engine.instantiate(tid, "user-1", {"owner_ids": ["user-1"]})
        await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -5}],
        })
        engine.update_definition(tid, "user-1", {
            "invariants": [{"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": ""}],
        })
        current = engine.get_definition(tid)
        assert current["version"] == 1
        assert current["spec"]["invariants"] == []

    @pytest.mark.asyncio
    async def test_edit_succeeds_once_the_live_instance_is_back_in_range(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs(invariants=[]))
        tid = created["template_id"]
        sid = engine.instantiate(tid, "user-1", {"owner_ids": ["user-1"]})
        await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 10}],
        })
        result = engine.update_definition(tid, "user-1", {
            "invariants": [{"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": ""}],
        })
        assert result["status"] == "ok"
        assert result["version"] == 2

    @pytest.mark.asyncio
    async def test_gate_enforces_new_invariant_immediately_no_restart_needed(self, engine: SustainEngine):
        # Proves the self._specs cache is invalidated on a successful edit —
        # without this, the gate would keep using the OLD invariant set
        # until the process restarted.
        created = engine.create_definition(**_plant_tracker_kwargs(invariants=[]))
        tid = created["template_id"]
        sid = engine.instantiate(tid, "user-1", {"owner_ids": ["user-1"]})
        await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": 10}],
        })
        engine.update_definition(tid, "user-1", {
            "invariants": [{"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": ""}],
        })
        result = await engine.execute_operator(sid, "edit.state_patch", {
            "patch": [{"op": "replace", "path": "moisture_level", "value": -1}],
        })
        assert not result.succeeded
        assert result.constraint_violated == "enforcement_gate"

    def test_edit_with_no_live_instances_is_never_blocked(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs(invariants=[]))
        result = engine.update_definition(created["template_id"], "user-1", {
            "invariants": [{"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": ""}],
        })
        assert result["status"] == "ok"

    def test_edit_by_non_owner_raises(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs(owner_user_id="user-1"))
        with pytest.raises(ValueError, match="not owned"):
            engine.update_definition(created["template_id"], "user-2", {"display_name": "Hijacked"})

    def test_edit_unknown_definition_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError, match="not found"):
            engine.update_definition("does-not-exist", "user-1", {"display_name": "X"})

    def test_edit_with_new_invalid_invariant_raises(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        with pytest.raises(ValueError, match="is invalid"):
            engine.update_definition(created["template_id"], "user-1", {
                "invariants": [{"id": "bad", "expression": "not_a_field >= 0", "description": ""}],
            })

    def test_partial_patch_keeps_omitted_fields(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        result = engine.update_definition(created["template_id"], "user-1", {"description": "updated desc"})
        assert result["status"] == "ok"
        assert result["spec"]["display_name"] == "Plant Tracker"  # unchanged
        assert result["spec"]["description"] == "updated desc"
        assert len(result["spec"]["state_schema"]) == 2  # dimensions preserved
        assert result["spec"]["invariants"][0]["id"] == "moisture_non_negative"  # preserved
        assert result["spec"]["operators"][0]["name"] == "edit.state_patch"  # preserved

    def test_check_definition_edit_safety_reports_compile_errors_as_violations(self, engine: SustainEngine):
        created = engine.create_definition(**_plant_tracker_kwargs())
        tid = created["template_id"]
        engine.instantiate(tid, "user-1", {"owner_ids": ["user-1"]})
        candidate = dict(created["spec"])
        candidate["invariants"] = [{"id": "bad", "expression": "not_a_field >= 0", "description": ""}]
        safe, violations = engine.check_definition_edit_safety(tid, candidate)
        assert not safe
        assert violations[0]["reason"].startswith("does not compile")
