"""
tests/test_sustains_migration.py

Tests for _migrate_sustains_schema() — repairs DBs whose `sustains` table
predates the `template_id` column.

Root cause being guarded against: DBs created before template_id existed have
a sustains table without it. metadata.create_all uses CREATE TABLE IF NOT
EXISTS, so it never adds the column; SustainEngine.list_all() / instantiate()
then fail with "no such column: s.template_id" and the platform shows zero
sustains. The migration must (1) add template_id, (2) preserve existing rows,
(3) be idempotent, and (4) be a no-op on a fresh (already-correct) DB.
"""

from sqlalchemy import create_engine, text

from sustena.db.schema import _migrate_sustains_schema


# ── Helpers ─────────────────────────────────────────────────────────────────


def _old_shape_engine(path: str):
    """An engine whose sustains table is the pre-template_id (seed) shape."""
    engine = create_engine(f"sqlite:///{path}")
    with engine.begin() as conn:
        conn.execute(text("""
            CREATE TABLE sustains (
                id               VARCHAR(36) NOT NULL,
                user_id          VARCHAR(36) NOT NULL,
                sustain_type     VARCHAR(50) NOT NULL,
                name             VARCHAR(100) NOT NULL,
                template_version VARCHAR(20) NOT NULL,
                created_at       DATETIME NOT NULL,
                is_active        BOOLEAN NOT NULL,
                PRIMARY KEY (id)
            )
        """))
        conn.execute(text("""
            INSERT INTO sustains
                (id, user_id, sustain_type, name, template_version, created_at, is_active)
            VALUES
                ('s1', 'u1', 'homestead', 'My Home', 'seed-1.0', '2026-06-02T00:00:00', 1)
        """))
    return engine


def _columns(conn) -> list[str]:
    return [row[1] for row in conn.execute(text("PRAGMA table_info(sustains)"))]


# ── Tests ──────────────────────────────────────────────────────────────────


def test_migration_adds_template_id(tmp_path):
    engine = _old_shape_engine(str(tmp_path / "old.db"))
    with engine.begin() as conn:
        assert "template_id" not in _columns(conn)
        _migrate_sustains_schema(conn)
        assert "template_id" in _columns(conn)


def test_migration_preserves_existing_rows(tmp_path):
    engine = _old_shape_engine(str(tmp_path / "old.db"))
    with engine.begin() as conn:
        _migrate_sustains_schema(conn)
        row = conn.execute(text(
            "SELECT id, user_id, sustain_type, name FROM sustains WHERE id='s1'"
        )).fetchone()
    assert row == ("s1", "u1", "homestead", "My Home")


def test_migration_template_id_is_null_after_copy(tmp_path):
    """Old rows have no template_id source; column is added as NULL."""
    engine = _old_shape_engine(str(tmp_path / "old.db"))
    with engine.begin() as conn:
        _migrate_sustains_schema(conn)
        val = conn.execute(text("SELECT template_id FROM sustains WHERE id='s1'")).scalar()
    assert val is None


def test_migration_is_idempotent(tmp_path):
    engine = _old_shape_engine(str(tmp_path / "old.db"))
    with engine.begin() as conn:
        _migrate_sustains_schema(conn)
        cols_first = _columns(conn)
        _migrate_sustains_schema(conn)  # second run must be a no-op
        cols_second = _columns(conn)
        count = conn.execute(text("SELECT COUNT(*) FROM sustains")).scalar()
    assert cols_first == cols_second
    assert count == 1  # row not duplicated or dropped


def test_migration_noop_when_template_id_present(tmp_path):
    """A table that already has template_id must be left untouched."""
    engine = create_engine(f"sqlite:///{tmp_path / 'new.db'}")
    with engine.begin() as conn:
        conn.execute(text("""
            CREATE TABLE sustains (
                id VARCHAR(36) PRIMARY KEY,
                user_id VARCHAR(36) NOT NULL,
                template_id VARCHAR(100),
                created_at DATETIME NOT NULL
            )
        """))
        conn.execute(text(
            "INSERT INTO sustains (id, user_id, template_id, created_at) "
            "VALUES ('s9', 'u9', 'vyyb', '2026-06-02T00:00:00')"
        ))
        _migrate_sustains_schema(conn)
        val = conn.execute(text("SELECT template_id FROM sustains WHERE id='s9'")).scalar()
    assert val == "vyyb"
