"""
tests/test_egress_migration.py

Regression test for a real bug caught live while building Slice 11: an
earlier --reload cycle created egress_outbox (via CREATE TABLE IF NOT
EXISTS) before updated_at was added to the schema, permanently stranding
the real sustena.db's table without that column — every _queue_egress()
call then failed with "table egress_outbox has no column named updated_at".
_migrate_egress_schema_sync() (same pattern as _migrate_events_schema_sync)
detects and additively fixes this. 0 real rows were at risk when this was
found and fixed; this test proves the migration is safe and idempotent on
its own terms, with a synthetic pre-migration table.
"""

import sqlite3

from sustena.core.sustain_engine import SustainEngine


def test_migration_adds_missing_updated_at_column(tmp_path):
    db_path = tmp_path / "old_schema.db"
    conn = sqlite3.connect(str(db_path))
    conn.execute("""
        CREATE TABLE egress_outbox (
            id TEXT PRIMARY KEY,
            sustain_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            target TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            idempotency_key TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'prepared',
            prepared_at TEXT NOT NULL,
            confirmed_at TEXT,
            sent_at TEXT,
            failed_at TEXT,
            failure_reason TEXT,
            result_json TEXT,
            attempts INTEGER NOT NULL DEFAULT 0,
            UNIQUE(sustain_id, idempotency_key)
        )
    """)
    conn.execute(
        "INSERT INTO egress_outbox (id, sustain_id, kind, target, payload_json, idempotency_key, prepared_at) "
        "VALUES ('e1', 's1', 'household_summary_export', 'local_file', '{}', 'k1', '2026-01-01T00:00:00')"
    )
    conn.commit()
    conn.close()

    engine = SustainEngine(db_path=str(db_path))  # __init__ -> _ensure_tables() -> the migration under test

    cols = [r[1] for r in engine._db.execute("PRAGMA table_info(egress_outbox)").fetchall()]
    assert "updated_at" in cols

    row = engine._db.execute("SELECT updated_at, prepared_at FROM egress_outbox WHERE id = 'e1'").fetchone()
    assert row["updated_at"] == row["prepared_at"]  # backfilled, not left NULL


def test_migration_is_idempotent(tmp_path):
    db_path = tmp_path / "fresh.db"
    engine1 = SustainEngine(db_path=str(db_path))
    cols1 = [r[1] for r in engine1._db.execute("PRAGMA table_info(egress_outbox)").fetchall()]
    assert "updated_at" in cols1

    # A second engine against the SAME already-migrated file must not error.
    engine2 = SustainEngine(db_path=str(db_path))
    cols2 = [r[1] for r in engine2._db.execute("PRAGMA table_info(egress_outbox)").fetchall()]
    assert cols1 == cols2


def test_no_op_when_table_does_not_exist_yet():
    # A brand-new :memory: engine's CREATE TABLE already includes
    # updated_at, so the migration must be a genuine no-op, not an error.
    engine = SustainEngine(db_path=":memory:")
    cols = [r[1] for r in engine._db.execute("PRAGMA table_info(egress_outbox)").fetchall()]
    assert "updated_at" in cols
