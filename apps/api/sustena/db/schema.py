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


async def init_db() -> None:
    """Create all tables if they don't exist. Called at startup."""
    engine = get_engine()
    async with engine.begin() as conn:
        await conn.run_sync(metadata.create_all)
        await conn.run_sync(_migrate_users_auth)
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
