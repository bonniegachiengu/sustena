"""
tests/test_sustain_ownership_reassignment.py

Tests for SustainEngine.reassign_sustain_owner() -- the one-off data
correction used to move a sustain still stamped with a legacy owner (e.g.
the "system" startup-seed sentinel that predates real per-user accounts)
onto its real owner's account. Same idempotent, report-not-silent-success
discipline as migrate_to_event_sourcing().

Run with:
    python -m pytest tests/test_sustain_ownership_reassignment.py -v
"""

import pytest

from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


class TestReassignSustainOwner:
    def test_reassigns_when_current_owner_matches_expected(self, engine):
        sid = engine.instantiate("homestead", "system", {"owner_ids": ["system"]})
        result = engine.reassign_sustain_owner(sid, from_user_id="system", to_user_id="real-user-1")
        assert result["status"] == "reassigned"
        assert result["old_owner"] == "system"
        assert result["new_owner"] == "real-user-1"
        row = engine._db.execute("SELECT user_id FROM sustains WHERE id = ?", (sid,)).fetchone()
        assert row["user_id"] == "real-user-1"

    def test_idempotent_when_already_reassigned(self, engine):
        sid = engine.instantiate("homestead", "system", {"owner_ids": ["system"]})
        engine.reassign_sustain_owner(sid, from_user_id="system", to_user_id="real-user-1")
        second = engine.reassign_sustain_owner(sid, from_user_id="system", to_user_id="real-user-1")
        assert second["status"] == "already_owner"
        row = engine._db.execute("SELECT user_id FROM sustains WHERE id = ?", (sid,)).fetchone()
        assert row["user_id"] == "real-user-1"

    def test_refuses_when_current_owner_does_not_match_expected(self, engine):
        """A stale assumption about the current owner must refuse, not
        blindly overwrite whoever actually owns it now."""
        sid = engine.instantiate("homestead", "someone-else", {"owner_ids": ["someone-else"]})
        result = engine.reassign_sustain_owner(sid, from_user_id="system", to_user_id="real-user-1")
        assert result["status"] == "owner_mismatch"
        assert result["old_owner"] == "someone-else"
        row = engine._db.execute("SELECT user_id FROM sustains WHERE id = ?", (sid,)).fetchone()
        assert row["user_id"] == "someone-else"  # untouched

    def test_unknown_sustain_id_reports_not_found(self, engine):
        result = engine.reassign_sustain_owner("does-not-exist", from_user_id="system", to_user_id="real-user-1")
        assert result["status"] == "sustain_not_found"

    def test_reassigned_sustain_moves_between_owner_scoped_lists(self, engine):
        sid = engine.instantiate("homestead", "system", {"owner_ids": ["system"]})
        assert sid in {s["id"] for s in engine.list_all(owner_user_id="system")}
        assert sid not in {s["id"] for s in engine.list_all(owner_user_id="real-user-1")}

        engine.reassign_sustain_owner(sid, from_user_id="system", to_user_id="real-user-1")

        assert sid not in {s["id"] for s in engine.list_all(owner_user_id="system")}
        assert sid in {s["id"] for s in engine.list_all(owner_user_id="real-user-1")}

    def test_generic_not_homestead_specific(self, engine):
        """Same mechanism, a different template entirely -- proves this
        isn't wired to any one sustain type."""
        tid = engine.create_definition(
            owner_user_id="u1", display_name="Pod", description="synthetic",
            dimensions=[{"name": "field", "type": "number", "default_value": 0}],
            invariants=[], operator_names=["edit.state_patch"],
        )["template_id"]
        sid = engine.instantiate(tid, "legacy-owner", {"owner_ids": ["legacy-owner"]})
        result = engine.reassign_sustain_owner(sid, from_user_id="legacy-owner", to_user_id="new-owner")
        assert result["status"] == "reassigned"

    def test_target_user_not_found_when_users_table_present_and_id_unknown(self, engine):
        """A bare :memory: SustainEngine has no `users` table by default
        (that's owned by the app's separate async schema) -- create one
        directly on this engine's own connection so the existence check
        has something real to fail against."""
        engine._db.execute("CREATE TABLE users (id TEXT PRIMARY KEY)")
        engine._db.execute("INSERT INTO users (id) VALUES ('a-real-user-id')")
        engine._db.commit()

        sid = engine.instantiate("homestead", "legacy-owner-2", {"owner_ids": ["legacy-owner-2"]})
        result = engine.reassign_sustain_owner(sid, from_user_id="legacy-owner-2", to_user_id="totally-unknown-user-id")
        assert result["status"] == "target_user_not_found"
        row = engine._db.execute("SELECT user_id FROM sustains WHERE id = ?", (sid,)).fetchone()
        assert row["user_id"] == "legacy-owner-2"  # untouched

        # and the positive case: a real target id in that same table succeeds
        result2 = engine.reassign_sustain_owner(sid, from_user_id="legacy-owner-2", to_user_id="a-real-user-id")
        assert result2["status"] == "reassigned"

    def test_missing_users_table_does_not_block_reassignment(self, engine):
        """A bare :memory: SustainEngine (no `users` table at all) -- the
        existence check must be treated as inapplicable, not fatal."""
        sid = engine.instantiate("homestead", "system", {"owner_ids": ["system"]})
        result = engine.reassign_sustain_owner(sid, from_user_id="system", to_user_id="real-user-1")
        assert result["status"] == "reassigned"
