"""
sustena/api/routes/journal.py

Private user journal — personal notes linked to sustains and operator executions.
Sprint 8.7

Routes (all under /api/v1/journal, mounted by main.py):
  GET    /entries           — auth required; returns caller's entries (newest first)
  POST   /entries           — auth required; creates an entry
  PUT    /entries/{id}      — auth required; updates own entry (kind, body)
  DELETE /entries/{id}      — auth required; deletes own entry

kind values: DECISION | NOTE | REFLECTION | UPDATE
"""

import logging
import uuid
from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field
from sqlalchemy import select

from sustena.api.routes.users import get_current_user
from sustena.db.schema import get_engine
from sustena.db.schema import journal_entries as journal_table

logger = logging.getLogger(__name__)

router = APIRouter()

_VALID_KINDS = {"DECISION", "NOTE", "REFLECTION", "UPDATE"}


# ── Pydantic models ──────────────────────────────────────────────────────────

class JournalCreateRequest(BaseModel):
    kind: str = Field(..., description="DECISION | NOTE | REFLECTION | UPDATE")
    body: str = Field(..., min_length=1)
    sustain_id: str | None = None
    operator_log_id: str | None = None


class JournalUpdateRequest(BaseModel):
    kind: str | None = None
    body: str | None = Field(default=None, min_length=1)


# ── Helpers ──────────────────────────────────────────────────────────────────

def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def _ok(data) -> dict:
    return {"status": "ok", "data": data, "error": None, "timestamp": _now_iso()}


def _serialize_dt(val) -> str | None:
    if val is None:
        return None
    return val.isoformat() if hasattr(val, "isoformat") else str(val)


def _row_to_dict(row_or_dict) -> dict:
    d = dict(row_or_dict._mapping) if hasattr(row_or_dict, "_mapping") else dict(row_or_dict)
    d["created_at"] = _serialize_dt(d.get("created_at"))
    return d


def _validate_kind(kind: str) -> str:
    k = kind.upper()
    if k not in _VALID_KINDS:
        raise HTTPException(
            status_code=422,
            detail=f"Invalid kind '{kind}'. Must be one of {sorted(_VALID_KINDS)}.",
        )
    return k


# ── GET /entries ──────────────────────────────────────────────────────────────

@router.get("/entries", summary="List caller's journal entries (auth required)")
async def list_entries(current_user: dict = Depends(get_current_user)) -> dict:
    user_id = current_user["id"]
    engine = get_engine()
    async with engine.connect() as conn:
        rows = (
            await conn.execute(
                select(journal_table)
                .where(journal_table.c.user_id == user_id)
                .order_by(journal_table.c.created_at.desc())
            )
        ).fetchall()

    return _ok({"entries": [_row_to_dict(r) for r in rows], "count": len(rows)})


# ── POST /entries ─────────────────────────────────────────────────────────────

@router.post("/entries", summary="Create a journal entry (auth required)")
async def create_entry(
    body: JournalCreateRequest,
    current_user: dict = Depends(get_current_user),
) -> dict:
    kind = _validate_kind(body.kind)
    entry_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    engine = get_engine()
    async with engine.connect() as conn:
        await conn.execute(
            journal_table.insert().values(
                id=entry_id,
                user_id=current_user["id"],
                sustain_id=body.sustain_id,
                operator_log_id=body.operator_log_id,
                kind=kind,
                body=body.body,
                created_at=now,
            )
        )
        await conn.commit()

    return _ok(
        {
            "id": entry_id,
            "user_id": current_user["id"],
            "kind": kind,
            "body": body.body,
            "sustain_id": body.sustain_id,
            "operator_log_id": body.operator_log_id,
            "created_at": now.isoformat(),
        }
    )


# ── PUT /entries/{id} ─────────────────────────────────────────────────────────

@router.put("/entries/{entry_id}", summary="Update own journal entry (auth required)")
async def update_entry(
    entry_id: str,
    body: JournalUpdateRequest,
    current_user: dict = Depends(get_current_user),
) -> dict:
    engine = get_engine()
    async with engine.connect() as conn:
        row = (
            await conn.execute(
                select(journal_table).where(journal_table.c.id == entry_id)
            )
        ).first()

        if row is None:
            raise HTTPException(status_code=404, detail="Entry not found")
        entry = dict(row._mapping)
        if entry["user_id"] != current_user["id"]:
            raise HTTPException(status_code=403, detail="Not your entry")

        updates: dict = {}
        if body.kind is not None:
            updates["kind"] = _validate_kind(body.kind)
        if body.body is not None:
            updates["body"] = body.body

        if updates:
            await conn.execute(
                journal_table.update()
                .where(journal_table.c.id == entry_id)
                .values(**updates)
            )
            await conn.commit()

        refreshed = dict(
            (
                await conn.execute(
                    select(journal_table).where(journal_table.c.id == entry_id)
                )
            ).first()._mapping
        )

    return _ok(_row_to_dict(refreshed))


# ── DELETE /entries/{id} ──────────────────────────────────────────────────────

@router.delete("/entries/{entry_id}", summary="Delete own journal entry (auth required)")
async def delete_entry(
    entry_id: str,
    current_user: dict = Depends(get_current_user),
) -> dict:
    engine = get_engine()
    async with engine.connect() as conn:
        row = (
            await conn.execute(
                select(journal_table).where(journal_table.c.id == entry_id)
            )
        ).first()

        if row is None:
            raise HTTPException(status_code=404, detail="Entry not found")
        entry = dict(row._mapping)
        if entry["user_id"] != current_user["id"]:
            raise HTTPException(status_code=403, detail="Not your entry")

        await conn.execute(
            journal_table.delete().where(journal_table.c.id == entry_id)
        )
        await conn.commit()

    return _ok({"deleted": True, "id": entry_id})
