"""
tests/test_purge_test_egress_entries.py

Tests for SustainEngine.purge_test_egress_entries() -- the operational
cleanup used to remove dev-testing acceptance-check rows that leaked into
a real household's live Outbox/Needs-Attention surface (a Windows filepath
and a raw Python OSError traceback from an earlier slice's own live
acceptance check against production). Distinct from cancel_egress(), which
only ever marks a row cancelled and never deletes -- this method exists for
rows that were never real household activity at all, including a genuinely
'sent' one that cancel_egress() can't touch by design.

Run with:
    python -m pytest tests/test_purge_test_egress_entries.py -v
"""

import pytest

from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


async def _prepare_row(engine, sid, label):
    result = await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": label})
    assert result.succeeded
    row = engine.list_egress(sid, status="prepared")[-1]
    return row["id"]


class TestPurgeTestEgressEntries:
    @pytest.mark.asyncio
    async def test_deletes_a_prepared_row(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        outbox_id = await _prepare_row(engine, sid, "test-a")

        result = engine.purge_test_egress_entries(sid, [outbox_id])
        assert result["deleted"] == [outbox_id]
        assert result["not_found"] == []
        assert engine.list_egress(sid) == []

    @pytest.mark.asyncio
    async def test_deletes_a_sent_row_which_cancel_egress_cannot_touch(self, engine: SustainEngine, tmp_path, monkeypatch):
        import sustena.core.sustain_engine as se_module
        monkeypatch.setattr(se_module, "_EXPORTS_DIR", tmp_path)

        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        outbox_id = await _prepare_row(engine, sid, "will-be-sent")
        await engine.confirm_egress(outbox_id)
        assert engine.list_egress(sid, status="sent")

        # cancel_egress() refuses a sent row -- confirms the two methods
        # are genuinely different, not overlapping.
        assert engine.cancel_egress(outbox_id) is False

        result = engine.purge_test_egress_entries(sid, [outbox_id])
        assert result["deleted"] == [outbox_id]
        assert engine.list_egress(sid) == []

    @pytest.mark.asyncio
    async def test_deletes_a_failed_row(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        # a label with a real illegal filename character forces a genuine
        # OSError on confirm, matching the exact failure mode being cleaned up
        outbox_id = await _prepare_row(engine, sid, "bad?label")
        await engine.confirm_egress(outbox_id)
        assert engine.list_egress(sid, status="failed")

        result = engine.purge_test_egress_entries(sid, [outbox_id])
        assert result["deleted"] == [outbox_id]

    def test_id_belonging_to_a_different_sustain_is_not_found_not_deleted(self, engine: SustainEngine):
        sid_a = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        sid_b = engine.instantiate("homestead", "u2", {"owner_ids": ["u2"]})
        engine._db.execute(
            "INSERT INTO egress_outbox (id, sustain_id, kind, target, payload_json, idempotency_key, status, prepared_at, updated_at) "
            "VALUES ('fake-id', ?, 'x', 'local_file', '{}', 'k', 'prepared', 't', 't')",
            (sid_b,),
        )
        engine._db.commit()

        result = engine.purge_test_egress_entries(sid_a, ["fake-id"])
        assert result["deleted"] == []
        assert result["not_found"] == ["fake-id"]
        # untouched, still belongs to sid_b
        assert len(engine.list_egress(sid_b)) == 1

    def test_unknown_id_reported_not_found_not_error(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        result = engine.purge_test_egress_entries(sid, ["does-not-exist"])
        assert result["not_found"] == ["does-not-exist"]
        assert result["deleted"] == []

    def test_idempotent_second_call_reports_not_found(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        engine._db.execute(
            "INSERT INTO egress_outbox (id, sustain_id, kind, target, payload_json, idempotency_key, status, prepared_at, updated_at) "
            "VALUES ('row-1', ?, 'x', 'local_file', '{}', 'k', 'prepared', 't', 't')",
            (sid,),
        )
        engine._db.commit()

        first = engine.purge_test_egress_entries(sid, ["row-1"])
        assert first["deleted"] == ["row-1"]
        second = engine.purge_test_egress_entries(sid, ["row-1"])
        assert second["deleted"] == []
        assert second["not_found"] == ["row-1"]
