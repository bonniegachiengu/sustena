"""
sustena/db/schema.py
SQLAlchemy Core table definitions for Sustena XII.
All tables are created on startup via init_db().
Alembic handles migrations from this schema.
"""

import logging
from datetime import datetime

from sqlalchemy import (
    Boolean,
    Column,
    DateTime,
    Float,
    Integer,
    MetaData,
    String,
    Table,
    Text,
    create_engine,
    text,
)
from sqlalchemy.ext.asyncio import AsyncEngine, create_async_engine

from sustena.config import settings

logger = logging.getLogger(__name__)

metadata = MetaData()

# ── users ──────────────────────────────────────────────────────────────────────
# One row per registered user.
# phone_number — WhatsApp users; email/password_hash — web/API users (Sprint 8.6)
users = Table(
    "users",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("phone_number", String(20), unique=True, nullable=True),
    Column("email", String(200), unique=True, nullable=True),
    Column("password_hash", String(200), nullable=True),
    Column("display_name", String(100), nullable=True),
    Column("pawa_balance", Integer, default=100, nullable=False),
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
    Column("last_active_at", DateTime, nullable=True),
    # Bumped on logout (or password change). A JWT's embedded token_version
    # claim must match this or get_current_user rejects it — this is what
    # makes logout genuinely revoke access rather than just clearing the
    # browser's copy of a token that would otherwise still be valid.
    Column("token_version", Integer, default=0, nullable=False),
)

# ── sustains ───────────────────────────────────────────────────────────────────
# One row per sustain instance (a described system owned by a user).
#
# Two writers share this table:
#   SustainEngine (sqlite3)   — inserts id, user_id, template_id, created_at
#   seed.py (SQLAlchemy)      — inserts id, user_id, sustain_type, name,
#                               template_version, created_at, is_active
# Columns used by only one writer are nullable so the other's INSERT doesn't fail.
sustains = Table(
    "sustains",
    metadata,
    Column("id", String(36), primary_key=True),               # UUID
    Column("user_id", String(36), nullable=False),             # FK → users.id
    Column("template_id", String(100), nullable=True),         # used by SustainEngine
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
    Column("sustain_type", String(50), nullable=True),         # used by seed.py
    Column("name", String(100), nullable=True),
    Column("template_version", String(20), nullable=True),
    Column("is_active", Boolean, nullable=True),
)

# ── sustain_states ─────────────────────────────────────────────────────────────
# Current state snapshot for each sustain. Versioned for rollback support.
sustain_states = Table(
    "sustain_states",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("sustain_id", String(36), nullable=False),          # FK → sustains.id
    Column("state_json", Text, nullable=False),                # Full state as JSON string
    Column("version_number", Integer, default=1, nullable=False),
    Column("updated_at", DateTime, default=datetime.utcnow, nullable=False),
)

# ── operators_log ──────────────────────────────────────────────────────────────
# Immutable log of every operator execution (success or failure).
operators_log = Table(
    "operators_log",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("sustain_id", String(36), nullable=False),
    Column("operative_id", String(50), nullable=True),         # Which operative called it (null = user direct)
    Column("operator_name", String(100), nullable=False),
    Column("input_json", Text, nullable=False),
    Column("output_json", Text, nullable=True),
    Column("status", String(20), nullable=False),              # ok | failed | deferred
    Column("failure_reason", Text, nullable=True),
    Column("pawa_cost", Integer, default=0, nullable=False),
    Column("llm_tokens_used", Integer, default=0, nullable=False),
    Column("timestamp", DateTime, default=datetime.utcnow, nullable=False),
)

# ── events ─────────────────────────────────────────────────────────────────────
# Immutable event log — the ground truth of every state transition.
# Event names follow dot-protocol: event.domain.type
events = Table(
    "events",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("sustain_id", String(36), nullable=False),
    Column("event_name", String(100), nullable=False),         # e.g. event.finances.income_received
    Column("payload_json", Text, nullable=False),
    Column("operator_log_id", String(36), nullable=True),      # FK → operators_log.id
    Column("timestamp", DateTime, default=datetime.utcnow, nullable=False),
)

# ── pawa_ledger ────────────────────────────────────────────────────────────────
# All pawa token movements. Phase 3: replaced by ERC-20 contract; logs stay identical.
pawa_ledger = Table(
    "pawa_ledger",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("user_id", String(36), nullable=False),
    Column("sustain_id", String(36), nullable=True),
    Column("delta", Integer, nullable=False),                  # Positive = credit, negative = debit
    Column("balance_after", Integer, nullable=False),
    Column("reason", String(200), nullable=False),
    Column("timestamp", DateTime, default=datetime.utcnow, nullable=False),
)

# ── council_proposals ──────────────────────────────────────────────────────────
# DAO proposals — strategies proposed by Orchie or operatives for Council vote.
council_proposals = Table(
    "council_proposals",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("sustain_id", String(36), nullable=False),
    Column("proposed_by", String(50), nullable=False),         # operative_id or 'orchie'
    Column("operator_name", String(100), nullable=False),
    Column("input_json", Text, nullable=False),
    Column("simulation_results_json", Text, nullable=True),
    Column("status", String(30), default="IN_VOTING", nullable=False),
    # IN_VOTING | PASSED | FAILED | DEFERRED | OVERRIDDEN_BY_USER
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
    Column("resolved_at", DateTime, nullable=True),
    Column("expires_at", DateTime, nullable=True),             # 48h default from created_at
)

# ── lore_entries ───────────────────────────────────────────────────────────────
# Public editorial content — blog-style posts with a draft → published lifecycle.
# Read is public; write requires authentication.
# kind: VISION | TECHNICAL | REFLECTION | UPDATE
lore_entries = Table(
    "lore_entries",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("title", String(200), nullable=False),
    Column("body_json", Text, nullable=False),           # JSON content blocks
    Column("kind", String(50), nullable=False),
    Column("status", String(20), default="draft", nullable=False),  # draft | published
    Column("author_id", String(36), nullable=False),     # FK → users.id
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
    Column("published_at", DateTime, nullable=True),
)

# ── journal_entries ────────────────────────────────────────────────────────────
# Private user journal — personal notes linked to sustains and operator actions.
# Always authenticated; never public.
# kind: DECISION | NOTE | REFLECTION | UPDATE
journal_entries = Table(
    "journal_entries",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("user_id", String(36), nullable=False),       # FK → users.id
    Column("sustain_id", String(36), nullable=True),     # FK → sustains.id (optional)
    Column("operator_log_id", String(36), nullable=True),  # FK → operators_log.id (optional)
    Column("kind", String(20), nullable=False),
    Column("body", Text, nullable=False),
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
)

# ── arena_packages ─────────────────────────────────────────────────────────────
# Mycelium marketplace packages — operatives, operators, spores, widgets.
# Read is public; write requires authentication.
# kind: operative | operator | spore | widget
arena_packages = Table(
    "arena_packages",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("name", String(200), nullable=False),
    Column("kind", String(20), nullable=False),          # operative | operator | spore | widget
    Column("spec_json", Text, nullable=True),
    Column("author_id", String(36), nullable=True),      # FK → users.id
    Column("trust_score", Float, default=0.0, nullable=False),
    Column("download_count", Integer, default=0, nullable=False),
    Column("pawa_cost", Integer, default=0, nullable=False),
    Column("is_free", Boolean, default=True, nullable=False),
    Column("tags", Text, nullable=True),                  # JSON array string
    Column("description", Text, nullable=True),
    Column("version", String(20), default="1.0.0", nullable=False),
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
)

# ── arena_products ─────────────────────────────────────────────────────────────
# Real-world products listed by sustains
arena_products = Table(
    "arena_products",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("name", String(200), nullable=False),
    Column("seller_type", String(20), nullable=False),
    Column("seller_name", String(100), nullable=False),
    Column("price", Integer, nullable=False),              # price in KES (whole number)
    Column("unit", String(50), nullable=False),            # e.g. "per portion", "500g"
    Column("description", Text, nullable=True),
    Column("tags", Text, nullable=True),                   # JSON array
    Column("emoji", String(10), nullable=True),
    Column("is_available", Boolean, default=True, nullable=False),
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
)

# ── arena_orders ────────────────────────────────────────────────────────────────
# Orders placed through the Arena checkout flow.
arena_orders = Table(
    "arena_orders",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("ref", String(20), nullable=False, unique=True),  # e.g. SXI-ABC123
    Column("user_id", String(36), nullable=True),            # FK → users.id
    Column("items_json", Text, nullable=False),               # JSON array of cart items
    Column("product_total", Integer, default=0, nullable=False),
    Column("pawa_total", Integer, default=0, nullable=False),
    Column("sustain_id", String(100), nullable=True),
    Column("delivery_addr", String(500), nullable=True),
    Column("pay_method", String(50), nullable=True),
    Column("product_status", String(20), nullable=True),     # PLACED|PROCESSING|DELIVERING|DELIVERED
    Column("package_status", String(20), nullable=True),     # PLACED|INSTALLING|SANDBOXED|LIVE
    Column("licenses_json", Text, nullable=True),             # JSON array [{name, key}]
    Column("created_at", DateTime, default=datetime.utcnow, nullable=False),
)

# ── council_votes ──────────────────────────────────────────────────────────────
# Individual votes per proposal per operative/user.
council_votes = Table(
    "council_votes",
    metadata,
    Column("id", String(36), primary_key=True),
    Column("proposal_id", String(36), nullable=False),         # FK → council_proposals.id
    Column("operative_id", String(50), nullable=False),        # operative name or 'user'
    Column("vote", String(10), nullable=False),                # YES | NO | ABSTAIN
    Column("reasoning", Text, nullable=True),
    Column("weight", Float, default=0.098, nullable=False),    # 51% user; 9.8% each of 5 operatives
    Column("timestamp", DateTime, default=datetime.utcnow, nullable=False),
)


# ── Engine and init ────────────────────────────────────────────────────────────

_engine: AsyncEngine | None = None


def get_engine() -> AsyncEngine:
    global _engine
    if _engine is None:
        db_url = settings.database_url
        # Ensure SQLite uses the async aiosqlite driver
        if "sqlite" in db_url and "aiosqlite" not in db_url:
            db_url = db_url.replace("sqlite://", "sqlite+aiosqlite://", 1)
        _engine = create_async_engine(
            db_url,
            echo=settings.is_development,
            connect_args={"check_same_thread": False} if "sqlite" in db_url else {},
        )
    return _engine


def _migrate_users_auth(sync_conn) -> None:
    """
    Add email + password_hash to the users table on existing DBs.
    SQLite cannot ALTER COLUMN, so we rename → recreate → copy → drop.
    No-op when the columns already exist (fresh DB or already migrated).
    """
    try:
        sync_conn.execute(text("SELECT email FROM users LIMIT 1"))
        return  # columns already present
    except Exception:
        pass

    sync_conn.execute(text("ALTER TABLE users RENAME TO _users_old"))
    sync_conn.execute(text("""
        CREATE TABLE users (
            id            VARCHAR(36) PRIMARY KEY,
            phone_number  VARCHAR(20)  UNIQUE,
            email         VARCHAR(200) UNIQUE,
            password_hash VARCHAR(200),
            display_name  VARCHAR(100),
            pawa_balance  INTEGER NOT NULL DEFAULT 100,
            created_at    DATETIME NOT NULL,
            last_active_at DATETIME
        )
    """))
    sync_conn.execute(text("""
        INSERT INTO users (id, phone_number, display_name, pawa_balance, created_at, last_active_at)
        SELECT id, phone_number, display_name, pawa_balance, created_at, last_active_at
        FROM _users_old
    """))
    sync_conn.execute(text("DROP TABLE _users_old"))
    logger.info("Migrated users table — added email and password_hash columns.")


def _migrate_users_token_version(sync_conn) -> None:
    """
    Add token_version to the users table on existing DBs. SQLite cannot
    ALTER COLUMN, so rename -> recreate -> copy -> drop, same pattern as
    _migrate_sustains_schema. Uses PRAGMA table_info for detection (not a
    failing SELECT) — a failing statement inside this shared transaction
    would invalidate the connection and abort the recreate below.
    No-op when the column already exists.
    """
    old_cols = [row[1] for row in sync_conn.execute(text("PRAGMA table_info(users)"))]
    if not old_cols or "token_version" in old_cols:
        return  # no users table yet (create_all handles it), or already migrated

    carry = [
        c for c in [
            "id", "phone_number", "email", "password_hash", "display_name",
            "pawa_balance", "created_at", "last_active_at",
        ]
        if c in old_cols
    ]
    cols_csv = ", ".join(carry)

    sync_conn.execute(text("ALTER TABLE users RENAME TO _users_old_tv"))
    sync_conn.execute(text("""
        CREATE TABLE users (
            id             VARCHAR(36) PRIMARY KEY,
            phone_number   VARCHAR(20)  UNIQUE,
            email          VARCHAR(200) UNIQUE,
            password_hash  VARCHAR(200),
            display_name   VARCHAR(100),
            pawa_balance   INTEGER NOT NULL DEFAULT 100,
            created_at     DATETIME NOT NULL,
            last_active_at DATETIME,
            token_version  INTEGER NOT NULL DEFAULT 0
        )
    """))
    sync_conn.execute(text(
        f"INSERT INTO users ({cols_csv}) SELECT {cols_csv} FROM _users_old_tv"
    ))
    sync_conn.execute(text("DROP TABLE _users_old_tv"))
    logger.info("Migrated users table — added token_version column (default 0).")


def _migrate_sustains_schema(sync_conn) -> None:
    """
    Add template_id to the sustains table on existing DBs and relax the
    seed-only columns to nullable. SQLite cannot ALTER COLUMN, so we
    rename → recreate → copy → drop.

    No-op when template_id already exists (fresh DB or already migrated).

    Root cause this repairs: DBs created before template_id was added have a
    sustains table without it. metadata.create_all uses CREATE TABLE IF NOT
    EXISTS, so it never adds the column, and SustainEngine.list_all() (which
    SELECTs s.template_id) and instantiate() then fail with
    "no such column: s.template_id" — leaving the platform with zero sustains.
    """
    # Detect with PRAGMA (never raises) — a failing SELECT inside this shared
    # transaction would invalidate the connection and silently abort the
    # recreate below.
    old_cols = [row[1] for row in sync_conn.execute(text("PRAGMA table_info(sustains)"))]
    if not old_cols or "template_id" in old_cols:
        return  # no sustains table yet (create_all handles it), or already migrated

    # Columns the old table actually has — copy only their intersection.
    new_cols = [
        "id", "user_id", "template_id", "created_at",
        "sustain_type", "name", "template_version", "is_active",
    ]
    carry = [c for c in new_cols if c in old_cols]  # template_id won't be in old_cols
    cols_csv = ", ".join(carry)

    sync_conn.execute(text("ALTER TABLE sustains RENAME TO _sustains_old"))
    sync_conn.execute(text("""
        CREATE TABLE sustains (
            id               VARCHAR(36) PRIMARY KEY,
            user_id          VARCHAR(36) NOT NULL,
            template_id      VARCHAR(100),
            created_at       DATETIME NOT NULL,
            sustain_type     VARCHAR(50),
            name             VARCHAR(100),
            template_version VARCHAR(20),
            is_active        BOOLEAN
        )
    """))
    sync_conn.execute(text(
        f"INSERT INTO sustains ({cols_csv}) SELECT {cols_csv} FROM _sustains_old"
    ))
    sync_conn.execute(text("DROP TABLE _sustains_old"))
    logger.info("Migrated sustains table — added template_id, relaxed seed columns.")


async def init_db() -> None:
    """Create all tables if they don't exist. Called at startup."""
    engine = get_engine()
    async with engine.begin() as conn:
        await conn.run_sync(metadata.create_all)
        await conn.run_sync(_migrate_users_auth)
        await conn.run_sync(_migrate_users_token_version)
        await conn.run_sync(_migrate_sustains_schema)
    logger.info("Database initialised — all tables ready.")


async def check_db_health() -> bool:
    """Returns True if the database is reachable."""
    try:
        engine = get_engine()
        async with engine.connect() as conn:
            await conn.execute(text("SELECT 1"))
        return True
    except Exception as e:
        logger.error("DB health check failed: %s", e)
        return False
