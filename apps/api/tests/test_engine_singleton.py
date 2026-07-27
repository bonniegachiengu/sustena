"""
tests/test_engine_singleton.py

Tests for sustena/core/engine_singleton.py and the new public methods
added to SustainEngine in Sprint 8.2.

Run: python -m pytest tests/test_engine_singleton.py -v
"""

import pytest

from sustena.core.engine_singleton import (
    get_shared_engine,
    reset_shared_engine,
)
from sustena.core.sustain_engine import SustainEngine


# ---------------------------------------------------------------------------
# Singleton identity
# ---------------------------------------------------------------------------

class TestSingletonIdentity:
    def test_returns_same_instance_on_repeated_calls(self):
        e1 = get_shared_engine()
        e2 = get_shared_engine()
        assert e1 is e2

    def test_returns_sustain_engine_instance(self):
        engine = get_shared_engine()
        assert isinstance(engine, SustainEngine)

    def test_reset_causes_new_instance_on_next_call(self):
        e1 = get_shared_engine()
        reset_shared_engine()
        e2 = get_shared_engine()
        assert e1 is not e2

    def test_reset_then_repeated_calls_return_same_instance(self):
        reset_shared_engine()
        e1 = get_shared_engine()
        e2 = get_shared_engine()
        assert e1 is e2


# ---------------------------------------------------------------------------
# Data persistence across calls
# ---------------------------------------------------------------------------

class TestDataPersistence:
    def test_data_written_via_one_call_visible_in_next(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "test-user", {"owner_ids": ["test-user"]})
        # Get via the singleton — same instance, data should be there
        same_engine = get_shared_engine()
        state = same_engine.get_state(sid)
        assert isinstance(state, dict)

    def test_list_all_reflects_seeded_sustain(self):
        engine = get_shared_engine()
        engine.instantiate("homestead", "test-user-2", {"owner_ids": ["test-user-2"]})
        sustains = engine.list_all()
        assert len(sustains) >= 1

    def test_list_all_entries_have_required_fields(self):
        engine = get_shared_engine()
        engine.instantiate("homestead", "test-user-3", {"owner_ids": ["test-user-3"]})
        for s in engine.list_all():
            assert "id" in s
            assert "label" in s
            assert "status" in s


# ---------------------------------------------------------------------------
# Owner-scoped list_all — fixes the picker showing every account's sustains
# ---------------------------------------------------------------------------

class TestListAllOwnerScoping:
    def test_no_filter_returns_sustains_from_every_owner(self):
        engine = get_shared_engine()
        sid_a = engine.instantiate("homestead", "owner-a", {"owner_ids": ["owner-a"]})
        sid_b = engine.instantiate("homestead", "owner-b", {"owner_ids": ["owner-b"]})
        ids = {s["id"] for s in engine.list_all()}
        assert sid_a in ids and sid_b in ids

    def test_owner_filter_excludes_other_owners_sustains(self):
        engine = get_shared_engine()
        sid_a = engine.instantiate("homestead", "owner-c", {"owner_ids": ["owner-c"]})
        sid_b = engine.instantiate("homestead", "owner-d", {"owner_ids": ["owner-d"]})
        scoped = engine.list_all(owner_user_id="owner-c")
        ids = {s["id"] for s in scoped}
        assert sid_a in ids
        assert sid_b not in ids

    def test_owner_filter_with_no_sustains_returns_empty(self):
        engine = get_shared_engine()
        engine.instantiate("homestead", "owner-e", {"owner_ids": ["owner-e"]})
        assert engine.list_all(owner_user_id="owner-with-nothing") == []


# ---------------------------------------------------------------------------
# get_spec
# ---------------------------------------------------------------------------

class TestGetSpec:
    def test_get_spec_returns_none_for_unknown_sustain(self):
        engine = get_shared_engine()
        result = engine.get_spec("does-not-exist")
        assert result is None

    def test_get_spec_returns_dict_for_seeded_sustain(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "spec-user", {"owner_ids": ["spec-user"]})
        spec = engine.get_spec(sid)
        assert isinstance(spec, dict)
        assert spec.get("id") == "homestead"


# ---------------------------------------------------------------------------
# get_operative_statuses
# ---------------------------------------------------------------------------

class TestGetOperativeStatuses:
    def test_returns_empty_list_for_unknown_sustain(self):
        engine = get_shared_engine()
        result = engine.get_operative_statuses("does-not-exist")
        assert result == []

    def test_returns_list_for_seeded_sustain(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "op-user", {"owner_ids": ["op-user"]})
        result = engine.get_operative_statuses(sid)
        assert isinstance(result, list)
        assert len(result) > 0

    def test_each_entry_has_required_fields(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "op-user-2", {"owner_ids": ["op-user-2"]})
        for entry in engine.get_operative_statuses(sid):
            assert "id" in entry
            assert "name" in entry
            assert "role" in entry
            assert "status" in entry

    def test_operative_ids_are_prefixed(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "op-user-3", {"owner_ids": ["op-user-3"]})
        for entry in engine.get_operative_statuses(sid):
            assert entry["id"].startswith("op-")


# ---------------------------------------------------------------------------
# evaluate_constraints
# ---------------------------------------------------------------------------

class TestEvaluateConstraints:
    def test_returns_empty_list_for_unknown_sustain(self):
        engine = get_shared_engine()
        result = engine.evaluate_constraints("does-not-exist")
        assert result == []

    def test_returns_list_for_seeded_sustain(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "con-user", {"owner_ids": ["con-user"]})
        result = engine.evaluate_constraints(sid)
        assert isinstance(result, list)

    def test_each_entry_has_expr_and_status(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "con-user-2", {"owner_ids": ["con-user-2"]})
        for entry in engine.evaluate_constraints(sid):
            assert "expr" in entry
            assert "status" in entry
            assert entry["status"] in ("ok", "fail")

    def test_liquid_non_negative_passes_on_fresh_state(self):
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "con-user-3", {"owner_ids": ["con-user-3"]})
        results = engine.evaluate_constraints(sid)
        liquid_constraint = next(
            (r for r in results if "liquid" in r["expr"].lower() and "balance" in r["expr"]),
            None,
        )
        if liquid_constraint:
            assert liquid_constraint["status"] == "ok"


# ---------------------------------------------------------------------------
# _get_spec bug fix — previously truncated, now returns correctly
# ---------------------------------------------------------------------------

class TestGetSpecBugFix:
    def test_get_spec_loads_from_db_when_not_in_cache(self):
        """
        Before Sprint 8.2, _get_spec() was truncated and returned None for
        sustains that were in the DB but not in the in-memory spec cache.
        """
        engine = get_shared_engine()
        sid = engine.instantiate("homestead", "bugfix-user", {"owner_ids": ["bugfix-user"]})

        # Clear the cache to simulate a fresh engine that reconnects to an existing DB
        engine._specs.clear()

        spec = engine._get_spec(sid)
        assert spec is not None
        assert spec.get("id") == "homestead"
