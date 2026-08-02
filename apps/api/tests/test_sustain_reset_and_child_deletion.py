"""
tests/test_sustain_reset_and_child_deletion.py

Tests for SustainEngine.delete_child_sustain() / reset_sustain_to_clean_slate()
and IngestEngine.purge_sustain_data() -- the user-requested "practice on a
clean slate" reset (2 Aug 2026). Same discipline as
reassign_sustain_owner()/purge_test_egress_entries(): ownership-checked,
report-not-silent-success, scoped strictly by sustain_id.

Run with:
    python -m pytest tests/test_sustain_reset_and_child_deletion.py -v
"""

import pytest

from sustena.core.sustain_engine import SustainEngine
from sustena.core.ingest_engine import IngestEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


@pytest.fixture
def ingest(engine) -> IngestEngine:
    return IngestEngine(engine)


class TestDeleteChildSustain:
    def test_deletes_the_sustain_row_and_events(self, engine):
        parent = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Kid"})
        engine.link_child(parent, child, slot="s1", member="Kid")

        result = engine.delete_child_sustain(child, owner_user_id="u1")
        assert result["status"] == "deleted"
        assert result["events_removed"] >= 1  # at least the genesis event

        assert engine._db.execute("SELECT id FROM sustains WHERE id = ?", (child,)).fetchone() is None
        assert engine._db.execute("SELECT id FROM sustain_states WHERE sustain_id = ?", (child,)).fetchone() is None
        assert engine._db.execute("SELECT id FROM events WHERE sustain_id = ?", (child,)).fetchall() == []

    def test_unlinks_from_parent(self, engine):
        parent = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Kid"})
        engine.link_child(parent, child, slot="s1", member="Kid")
        assert len(engine.list_children(parent)) == 1

        engine.delete_child_sustain(child, owner_user_id="u1")
        assert engine.list_children(parent) == []

    def test_refuses_on_owner_mismatch(self, engine):
        child = engine.instantiate("habitat", "real-owner", {"owner_ids": ["real-owner"], "name": "Kid"})
        result = engine.delete_child_sustain(child, owner_user_id="wrong-owner")
        assert result["status"] == "owner_mismatch"
        # untouched
        assert engine._db.execute("SELECT id FROM sustains WHERE id = ?", (child,)).fetchone() is not None

    def test_not_found_reports_cleanly(self, engine):
        result = engine.delete_child_sustain("does-not-exist", owner_user_id="u1")
        assert result["status"] == "not_found"

    def test_clears_operational_tables(self, engine):
        parent = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Kid"})
        engine.link_child(parent, child, slot="s1", member="Kid")
        engine._db.execute(
            "INSERT INTO pawa_meter_log (id, sustain_id, operator_name, principal, compute, storage, pawa, timestamp) "
            "VALUES ('m1', ?, 'budget.spend', 'u1', 1, 0, 1.0, '2026-01-01')",
            (child,),
        )
        engine._db.commit()
        result = engine.delete_child_sustain(child, owner_user_id="u1")
        assert result["table_counts"]["pawa_meter_log"] == 1
        assert engine._db.execute("SELECT * FROM pawa_meter_log WHERE sustain_id = ?", (child,)).fetchall() == []


class TestResetSustainToCleanSlate:
    @pytest.mark.asyncio
    async def test_pockets_are_gone_after_reset(self, engine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 1000, "source": "seed", "frequency": "once"})
        await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 500, "period": "monthly"})
        state = engine.get_state(sid)
        assert "food" in state["finances"]["pockets"]

        result = engine.reset_sustain_to_clean_slate(sid, owner_user_id="u1")
        assert result["status"] == "reset"
        assert result["old_pockets"] == ["food"]

        after = engine.get_state(sid)
        assert after["finances"]["pockets"] == {}
        assert after["finances"]["liquid"]["balance"] == 0.0

    @pytest.mark.asyncio
    async def test_rebuild_state_equals_get_state_after_reset(self, engine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 1000, "source": "seed", "frequency": "once"})
        await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 500, "period": "monthly"})
        await engine.execute_operator(sid, "budget.spend", {"pocket_name": "food", "amount": 100})

        engine.reset_sustain_to_clean_slate(sid, owner_user_id="u1")

        assert engine.get_state(sid) == engine.rebuild_state(sid)

    def test_exactly_one_event_survives_the_reset(self, engine):
        """Old event rows are genuinely deleted (not superseded-but-kept) --
        a 'clean slate to practice on' means the event log itself is
        clean, not just semantically overridden."""
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        events_before = engine._db.execute("SELECT id FROM events WHERE sustain_id = ?", (sid,)).fetchall()
        assert len(events_before) == 1  # genesis only, nothing spent yet

        result = engine.reset_sustain_to_clean_slate(sid, owner_user_id="u1")
        assert result["old_event_count"] == 1

        events_after = engine._db.execute("SELECT id, event_name FROM events WHERE sustain_id = ?", (sid,)).fetchall()
        assert len(events_after) == 1
        assert events_after[0]["event_name"] == "event.system.reset_to_clean_slate"

    @pytest.mark.asyncio
    async def test_old_events_are_actually_deleted_not_merely_superseded(self, engine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 500, "source": "seed", "frequency": "once"})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 500, "source": "seed", "frequency": "once"})
        before_count = engine._db.execute("SELECT COUNT(*) AS n FROM events WHERE sustain_id = ?", (sid,)).fetchone()["n"]
        assert before_count == 3  # genesis + 2 income events

        engine.reset_sustain_to_clean_slate(sid, owner_user_id="u1")
        after_count = engine._db.execute("SELECT COUNT(*) AS n FROM events WHERE sustain_id = ?", (sid,)).fetchone()["n"]
        assert after_count == 1

    def test_linked_habitats_are_permanently_deleted_not_just_unlinked(self, engine):
        parent = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        c1 = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Kid"})
        c2 = engine.instantiate("habitat", "u1", {"owner_ids": ["u1"], "name": "Kid"})
        engine.link_child(parent, c1, slot="s1", member="A")
        engine.link_child(parent, c2, slot="s2", member="B")

        result = engine.reset_sustain_to_clean_slate(parent, owner_user_id="u1")
        assert sorted(result["habitats_removed"]) == sorted([c1, c2])

        # gone entirely -- not just unlinked
        assert engine._db.execute("SELECT id FROM sustains WHERE id = ?", (c1,)).fetchone() is None
        assert engine._db.execute("SELECT id FROM sustains WHERE id = ?", (c2,)).fetchone() is None
        assert engine.list_children(parent) == []
        assert c1 not in {s["id"] for s in engine.list_all(owner_user_id="u1")}
        assert c2 not in {s["id"] for s in engine.list_all(owner_user_id="u1")}

    def test_parent_sustain_row_and_owner_survive(self, engine):
        """The sustain shell and its owner are untouched -- reset empties
        it, it does not delete the account/login-relevant row."""
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        engine.reset_sustain_to_clean_slate(sid, owner_user_id="u1")
        row = engine._db.execute("SELECT id, user_id, template_id FROM sustains WHERE id = ?", (sid,)).fetchone()
        assert row is not None
        assert row["user_id"] == "u1"
        assert row["template_id"] == "homestead"

    def test_refuses_on_owner_mismatch(self, engine):
        sid = engine.instantiate("homestead", "real-owner", {"owner_ids": ["real-owner"]})
        result = engine.reset_sustain_to_clean_slate(sid, owner_user_id="wrong-owner")
        assert result["status"] == "owner_mismatch"
        # untouched -- still has its genesis event, nothing removed
        assert engine._db.execute("SELECT COUNT(*) AS n FROM events WHERE sustain_id = ?", (sid,)).fetchone()["n"] == 1

    def test_not_found_reports_cleanly(self, engine):
        result = engine.reset_sustain_to_clean_slate("does-not-exist", owner_user_id="u1")
        assert result["status"] == "not_found"

    @pytest.mark.asyncio
    async def test_operational_tables_cleared_for_the_reset_sustain(self, engine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        engine._db.execute(
            "INSERT INTO egress_outbox (id, sustain_id, kind, target, payload_json, idempotency_key, status, prepared_at, updated_at) "
            "VALUES ('e1', ?, 'summary', 'local_file', '{}', 'k1', 'sent', '2026-01-01', '2026-01-01')",
            (sid,),
        )
        engine._db.execute(
            "INSERT INTO operative_suggestions (id, sustain_id, operative_id, rule_id, title, reason, severity, dedupe_key, status, created_at, updated_at) "
            "VALUES ('s1', ?, 'mentor', 'rule_x', 't', 'r', 'info', 'dk1', 'pending', '2026-01-01', '2026-01-01')",
            (sid,),
        )
        engine._db.commit()

        result = engine.reset_sustain_to_clean_slate(sid, owner_user_id="u1")
        assert result["table_counts"]["egress_outbox"] == 1
        assert result["table_counts"]["operative_suggestions"] == 1
        assert engine._db.execute("SELECT * FROM egress_outbox WHERE sustain_id = ?", (sid,)).fetchall() == []
        assert engine._db.execute("SELECT * FROM operative_suggestions WHERE sustain_id = ?", (sid,)).fetchall() == []

    def test_a_second_unrelated_sustain_is_completely_untouched(self, engine):
        """The strict scoping requirement -- resetting one sustain must
        never affect another, even for the same owner."""
        sid_a = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        sid_b = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        engine._db.execute(
            "UPDATE sustain_states SET state_json = ? WHERE sustain_id = ?",
            ('{"finances": {"pockets": {"untouched": {"allocated": 5, "spent": 0}}, "liquid": {"balance": 5}}}', sid_b),
        )
        engine._db.commit()

        engine.reset_sustain_to_clean_slate(sid_a, owner_user_id="u1")

        state_b = engine.get_state(sid_b)
        assert state_b["finances"]["pockets"]["untouched"]["allocated"] == 5

    def test_generic_not_homestead_specific(self, engine):
        """A synthetic definition-based parent/child pair, proving this
        isn't wired to homestead/habitat specifically."""
        parent_tid = engine.create_definition(
            owner_user_id="u1", display_name="Pod", description="synthetic",
            dimensions=[{"name": "field", "type": "number", "default_value": 0}],
            invariants=[], operator_names=["edit.state_patch"],
        )["template_id"]
        child_tid = engine.create_definition(
            owner_user_id="u1", display_name="Leaf", description="synthetic",
            dimensions=[{"name": "leaf_field", "type": "number", "default_value": 0}],
            invariants=[], operator_names=["edit.state_patch"],
        )["template_id"]
        parent = engine.instantiate(parent_tid, "u1", {"owner_ids": ["u1"]})
        child = engine.instantiate(child_tid, "u1", {"owner_ids": ["u1"]})
        engine.link_child(parent, child, slot="s1", member="leaf")

        result = engine.reset_sustain_to_clean_slate(parent, owner_user_id="u1")
        assert result["status"] == "reset"
        assert result["habitats_removed"] == [child]


class TestPurgeSustainData:
    @pytest.mark.asyncio
    async def test_clears_ingest_messages_sources_and_history(self, engine, ingest):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        ingest.register_source("mpesa", sid, label="M-Pesa")
        result_capture = await ingest.capture("mpesa", sid, "QGH7 Confirmed. Ksh450.00 paid to NAIVAS on 20/7/26 at 4:30 PM. New M-PESA balance is Ksh1,000.00")
        assert result_capture["is_duplicate"] is False
        ingest.record_classification(sid, "NAIVAS", "food", "budget.spend")

        assert len(ingest.list_messages(sustain_id=sid)) == 1
        assert len(ingest.get_sources(sustain_id=sid)) == 1
        assert ingest.get_classification_history(sid, "NAIVAS") is not None

        result = ingest.purge_sustain_data(sid, owner_user_id="u1")
        assert result["status"] == "purged"
        assert result["table_counts"]["ingest_messages"] == 1
        assert result["table_counts"]["ingest_sources"] == 1
        assert result["table_counts"]["capture_classification_history"] == 1

        assert ingest.list_messages(sustain_id=sid) == []
        assert ingest.get_sources(sustain_id=sid) == []
        assert ingest.get_classification_history(sid, "NAIVAS") is None

    @pytest.mark.asyncio
    async def test_dedup_key_is_gone_so_a_resync_recaptures(self, engine, ingest):
        """The whole point of clearing ingest data on reset -- a re-sync
        must genuinely re-capture the same real message text, not be
        silently treated as a duplicate of pre-wipe history."""
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        text = "QGH7 Confirmed. Ksh450.00 paid to NAIVAS on 20/7/26 at 4:30 PM. New M-PESA balance is Ksh1,000.00"
        first = await ingest.capture("mpesa", sid, text)
        assert first["is_duplicate"] is False

        ingest.purge_sustain_data(sid, owner_user_id="u1")

        second = await ingest.capture("mpesa", sid, text)
        assert second["is_duplicate"] is False  # genuinely re-captured, not deduped against gone history

    @pytest.mark.asyncio
    async def test_refuses_on_owner_mismatch(self, engine, ingest):
        sid = engine.instantiate("homestead", "real-owner", {"owner_ids": ["real-owner"]})
        await ingest.capture("mpesa", sid, "some raw text with no recognisable shape")
        result = ingest.purge_sustain_data(sid, owner_user_id="wrong-owner")
        assert result["status"] == "owner_mismatch"
        assert len(ingest.list_messages(sustain_id=sid)) == 1  # untouched

    def test_not_found_reports_cleanly(self, engine, ingest):
        result = ingest.purge_sustain_data("does-not-exist", owner_user_id="u1")
        assert result["status"] == "not_found"

    @pytest.mark.asyncio
    async def test_a_second_sustains_messages_are_untouched(self, engine, ingest):
        sid_a = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        sid_b = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await ingest.capture("mpesa", sid_a, "text for A")
        await ingest.capture("mpesa", sid_b, "text for B")

        ingest.purge_sustain_data(sid_a, owner_user_id="u1")

        assert ingest.list_messages(sustain_id=sid_a) == []
        assert len(ingest.list_messages(sustain_id=sid_b)) == 1
