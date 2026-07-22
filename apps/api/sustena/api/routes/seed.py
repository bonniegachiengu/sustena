"""
sustena/api/routes/seed.py

Seed UI endpoints — mounted at /seed (dev-only).
Lets Bonnie seed real sustain data through the Control Panel
instead of raw SQL.

Endpoints:
  POST /seed/sustain   — create or update a sustain
  POST /seed/pocket    — create or update a pocket
  POST /seed/operative — enable an operative for a sustain
  POST /seed/event     — manually inject an event
  GET  /seed/status    — return what's currently seeded

Auth: Authorization: Bearer {ADMIN_TOKEN}  (same as /devui)
"""

import json
import logging
import uuid
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from sqlalchemy import text

from sustena.api.routes.users import get_current_user
from sustena.db.schema import get_engine

router = APIRouter()
logger = logging.getLogger(__name__)

# Every route below requires a real user session (get_current_user) — see
# devui.py for the reasoning; this used to be the same shared ADMIN_TOKEN.


# ── Helpers ───────────────────────────────────────────────────────────────────

def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def ok(data: Any) -> dict:
    return {"status": "ok", "data": data, "timestamp": _now_iso()}


def err(msg: str, status_code: int = 400) -> JSONResponse:
    return JSONResponse(
        status_code=status_code,
        content={"status": "error", "data": None, "error": msg, "timestamp": _now_iso()},
    )


# ── Ensure seed-specific tables exist ────────────────────────────────────────

_tables_ready = False


async def _ensure_tables(conn) -> None:
    global _tables_ready
    if _tables_ready:
        return

    await conn.execute(text("""
        CREATE TABLE IF NOT EXISTS seed_pockets (
            id          TEXT PRIMARY KEY,
            sustain_id  TEXT NOT NULL,
            name        TEXT NOT NULL,
            allocation  REAL NOT NULL DEFAULT 0,
            ceiling     REAL NOT NULL DEFAULT 0,
            currency    TEXT NOT NULL DEFAULT 'KES',
            created_at  TEXT NOT NULL,
            UNIQUE(sustain_id, name)
        )
    """))

    await conn.execute(text("""
        CREATE TABLE IF NOT EXISTS seed_operatives (
            id           TEXT PRIMARY KEY,
            sustain_id   TEXT NOT NULL,
            operative_id TEXT NOT NULL,
            config_json  TEXT NOT NULL DEFAULT '{}',
            enabled      INTEGER NOT NULL DEFAULT 1,
            created_at   TEXT NOT NULL,
            UNIQUE(sustain_id, operative_id)
        )
    """))

    _tables_ready = True


# ── Request models ────────────────────────────────────────────────────────────

class SustainBody(BaseModel):
    id: str                          # e.g. "homestead.bonnie"
    label: str
    type: str                        # household | business | chama | farm
    description: str = ""


class PocketBody(BaseModel):
    sustain_id: str
    name: str
    allocation: float = 0
    ceiling: float = 0
    currency: str = "KES"


class OperativeBody(BaseModel):
    sustain_id: str
    operative_id: str                # mentor | protege | curator | attache | navigator | clerk
    config: dict = Field(default_factory=dict)
    enabled: bool = True


class EventBody(BaseModel):
    sustain_id: str
    type: str                        # received | sent | purchase | sale
    amount: float
    description: str = ""
    metadata: dict = Field(default_factory=dict)


# ── POST /seed/sustain ────────────────────────────────────────────────────────

@router.post("/sustain")
async def seed_sustain(
    body: SustainBody,
    _current_user: dict = Depends(get_current_user),
):
    """Create or update a sustain by ID."""
    # Map seed 'type' to schema's sustain_type vocabulary
    type_map = {
        "household": "homestead",
        "business":  "vyyb",
        "chama":     "chama",
        "farm":      "mkulima",
    }
    sustain_type = type_map.get(body.type, body.type)
    now = _now_iso()

    engine = get_engine()
    async with engine.begin() as conn:
        # Upsert: try insert first, update on conflict
        from sqlalchemy import text
        row = await conn.execute(
            text("SELECT id FROM sustains WHERE id = :id"),
            {"id": body.id},
        )
        exists = row.fetchone()

        if exists:
            await conn.execute(
                text("""
                    UPDATE sustains
                    SET name = :name, sustain_type = :sustain_type, is_active = 1
                    WHERE id = :id
                """),
                {"id": body.id, "name": body.label, "sustain_type": sustain_type},
            )
            action = "updated"
        else:
            await conn.execute(
                text("""
                    INSERT INTO sustains (id, user_id, sustain_type, name, template_version, created_at, is_active)
                    VALUES (:id, :user_id, :sustain_type, :name, :version, :created_at, 1)
                """),
                {
                    "id": body.id,
                    "user_id": "seed-admin",
                    "sustain_type": sustain_type,
                    "name": body.label,
                    "version": "seed-1.0",
                    "created_at": now,
                },
            )
            action = "created"

    logger.info("seed_sustain: %s sustain=%s", action, body.id)
    return ok({"sustain_id": body.id, "action": action})


# ── POST /seed/pocket ─────────────────────────────────────────────────────────

@router.post("/pocket")
async def seed_pocket(
    body: PocketBody,
    _current_user: dict = Depends(get_current_user),
):
    """Create or update a pocket for a sustain."""
    now = _now_iso()

    engine = get_engine()
    async with engine.begin() as conn:
        from sqlalchemy import text
        await _ensure_tables(conn)

        row = await conn.execute(
            text("SELECT id FROM seed_pockets WHERE sustain_id = :sid AND name = :name"),
            {"sid": body.sustain_id, "name": body.name},
        )
        exists = row.fetchone()

        if exists:
            await conn.execute(
                text("""
                    UPDATE seed_pockets
                    SET allocation = :allocation, ceiling = :ceiling, currency = :currency
                    WHERE sustain_id = :sid AND name = :name
                """),
                {
                    "sid": body.sustain_id, "name": body.name,
                    "allocation": body.allocation, "ceiling": body.ceiling,
                    "currency": body.currency,
                },
            )
            action = "updated"
        else:
            await conn.execute(
                text("""
                    INSERT INTO seed_pockets (id, sustain_id, name, allocation, ceiling, currency, created_at)
                    VALUES (:id, :sid, :name, :allocation, :ceiling, :currency, :created_at)
                """),
                {
                    "id": str(uuid.uuid4()),
                    "sid": body.sustain_id, "name": body.name,
                    "allocation": body.allocation, "ceiling": body.ceiling,
                    "currency": body.currency, "created_at": now,
                },
            )
            action = "created"

    # Bridge into engine state so the pocket shows in the Monitor (best-effort —
    # succeeds only when sustain_id is a real engine sustain).
    bridged = False
    try:
        from sustena.core.engine_singleton import get_shared_engine
        bridged = get_shared_engine().seed_pocket(
            body.sustain_id, body.name, body.allocation, body.ceiling
        )
    except Exception as exc:
        logger.debug("seed_pocket engine bridge failed: %s", exc)

    logger.info("seed_pocket: %s sustain=%s pocket=%s bridged=%s", action, body.sustain_id, body.name, bridged)
    return ok({"sustain_id": body.sustain_id, "pocket": body.name, "action": action, "bridged_to_monitor": bridged})


# ── POST /seed/operative ──────────────────────────────────────────────────────

@router.post("/operative")
async def seed_operative(
    body: OperativeBody,
    _current_user: dict = Depends(get_current_user),
):
    """Enable or disable an operative for a sustain."""
    now = _now_iso()

    engine = get_engine()
    async with engine.begin() as conn:
        from sqlalchemy import text
        await _ensure_tables(conn)

        row = await conn.execute(
            text("SELECT id FROM seed_operatives WHERE sustain_id = :sid AND operative_id = :oid"),
            {"sid": body.sustain_id, "oid": body.operative_id},
        )
        exists = row.fetchone()

        if exists:
            await conn.execute(
                text("""
                    UPDATE seed_operatives
                    SET config_json = :config, enabled = :enabled
                    WHERE sustain_id = :sid AND operative_id = :oid
                """),
                {
                    "sid": body.sustain_id, "oid": body.operative_id,
                    "config": json.dumps(body.config),
                    "enabled": 1 if body.enabled else 0,
                },
            )
            action = "updated"
        else:
            await conn.execute(
                text("""
                    INSERT INTO seed_operatives (id, sustain_id, operative_id, config_json, enabled, created_at)
                    VALUES (:id, :sid, :oid, :config, :enabled, :created_at)
                """),
                {
                    "id": str(uuid.uuid4()),
                    "sid": body.sustain_id, "oid": body.operative_id,
                    "config": json.dumps(body.config),
                    "enabled": 1 if body.enabled else 0,
                    "created_at": now,
                },
            )
            action = "created"

    # Bridge into engine state so enabling/disabling actually changes what the
    # Monitor's operative cards show (best-effort — succeeds only when
    # sustain_id is a real engine sustain with this operative in its spec).
    bridged = False
    try:
        from sustena.core.engine_singleton import get_shared_engine
        bridged = get_shared_engine().set_operative_enabled(
            body.sustain_id, body.operative_id, body.enabled
        )
    except Exception as exc:
        logger.debug("seed_operative engine bridge failed: %s", exc)

    status_label = "enabled" if body.enabled else "disabled"
    logger.info("seed_operative: %s %s for sustain=%s bridged=%s", status_label, body.operative_id, body.sustain_id, bridged)
    return ok({
        "sustain_id": body.sustain_id,
        "operative_id": body.operative_id,
        "enabled": body.enabled,
        "action": action,
        "bridged_to_monitor": bridged,
    })


# ── POST /seed/event ──────────────────────────────────────────────────────────

@router.post("/event")
async def seed_event(
    body: EventBody,
    _current_user: dict = Depends(get_current_user),
):
    """Manually inject an event into a sustain's event log."""
    event_id = str(uuid.uuid4())
    now = _now_iso()

    # Map UI type names to dot-protocol event names
    event_name_map = {
        "received": "event.finances.income_received",
        "sent":     "event.finances.payment_sent",
        "purchase": "event.finances.purchase_made",
        "sale":     "event.finances.sale_completed",
    }
    event_name = event_name_map.get(body.type, f"event.seed.{body.type}")

    payload = {
        "amount": body.amount,
        "description": body.description,
        "currency": "KES",
        "source": "seed-ui",
        **body.metadata,
    }

    engine = get_engine()
    async with engine.begin() as conn:
        from sqlalchemy import text
        await conn.execute(
            text("""
                INSERT INTO events (id, sustain_id, event_name, payload_json, operator_log_id, timestamp)
                VALUES (:id, :sustain_id, :event_name, :payload_json, NULL, :timestamp)
            """),
            {
                "id": event_id,
                "sustain_id": body.sustain_id,
                "event_name": event_name,
                "payload_json": json.dumps(payload),
                "timestamp": now,
            },
        )

    logger.info("seed_event: injected %s for sustain=%s amount=%.2f", event_name, body.sustain_id, body.amount)
    return ok({"event_id": event_id, "event_name": event_name, "sustain_id": body.sustain_id})


# ── GET /seed/status ──────────────────────────────────────────────────────────

@router.get("/status")
async def seed_status(
    _current_user: dict = Depends(get_current_user),
):
    """Return what's currently seeded: sustains, pocket counts, event counts."""
    engine = get_engine()
    async with engine.connect() as conn:
        from sqlalchemy import text

        # All seed sustains (seeded by us, user_id = seed-admin)
        rows = await conn.execute(
            text("SELECT id, name, sustain_type, created_at FROM sustains WHERE user_id = 'seed-admin' ORDER BY created_at DESC")
        )
        sustains_raw = rows.fetchall()

        result = []
        for s in sustains_raw:
            sid = s[0]

            # Pocket count
            try:
                await _ensure_tables(conn)
                pc = await conn.execute(
                    text("SELECT COUNT(*) FROM seed_pockets WHERE sustain_id = :sid"),
                    {"sid": sid},
                )
                pocket_count = pc.scalar() or 0
            except Exception:
                pocket_count = 0

            # Event count
            try:
                ec = await conn.execute(
                    text("SELECT COUNT(*) FROM events WHERE sustain_id = :sid"),
                    {"sid": sid},
                )
                event_count = ec.scalar() or 0
            except Exception:
                event_count = 0

            # Operative count
            try:
                oc = await conn.execute(
                    text("SELECT COUNT(*) FROM seed_operatives WHERE sustain_id = :sid AND enabled = 1"),
                    {"sid": sid},
                )
                operative_count = oc.scalar() or 0
            except Exception:
                operative_count = 0

            result.append({
                "id": sid,
                "name": s[1],
                "type": s[2],
                "created_at": s[3],
                "pocket_count": pocket_count,
                "event_count": event_count,
                "operative_count": operative_count,
            })

    return ok({"sustains": result, "total": len(result)})
