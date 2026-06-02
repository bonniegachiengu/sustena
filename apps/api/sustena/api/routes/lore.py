"""
sustena/api/routes/lore.py

Public Lore CMS — editorial content with draft → published lifecycle.
Sprint 8.7

Routes (all under /api/v1/lore, mounted by main.py):
  GET  /entries          — public; returns all published entries
  POST /entries          — auth required; creates a draft entry
  PUT  /entries/{id}     — auth required; updates own entry (title, body_json, kind)
  POST /entries/{id}/publish — auth required; promotes draft → published

kind values: VISION | TECHNICAL | REFLECTION | UPDATE
"""

import json
import logging
import uuid
from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field
from sqlalchemy import select

from sustena.api.routes.users import get_current_user
from sustena.db.schema import get_engine
from sustena.db.schema import lore_entries as lore_table

logger = logging.getLogger(__name__)

router = APIRouter()

_VALID_KINDS = {"VISION", "TECHNICAL", "REFLECTION", "UPDATE"}


# ── Pydantic models ──────────────────────────────────────────────────────────

class LoreCreateRequest(BaseModel):
    title: str = Field(..., max_length=200)
    body_json: str = Field(..., description="JSON string of content blocks")
    kind: str = Field(..., description="VISION | TECHNICAL | REFLECTION | UPDATE")


class LoreUpdateRequest(BaseModel):
    title: str | None = Field(default=None, max_length=200)
    body_json: str | None = None
    kind: str | None = None


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
    d["published_at"] = _serialize_dt(d.get("published_at"))
    return d


def _validate_kind(kind: str) -> str:
    k = kind.upper()
    if k not in _VALID_KINDS:
        raise HTTPException(
            status_code=422,
            detail=f"Invalid kind '{kind}'. Must be one of {sorted(_VALID_KINDS)}.",
        )
    return k


def _validate_body_json(body_json: str) -> str:
    try:
        json.loads(body_json)
    except (ValueError, TypeError):
        raise HTTPException(status_code=422, detail="body_json must be valid JSON")
    return body_json


# ── GET /entries ──────────────────────────────────────────────────────────────

@router.get("/entries", summary="List all published lore entries (public)")
async def list_entries() -> dict:
    engine = get_engine()
    async with engine.connect() as conn:
        rows = (
            await conn.execute(
                select(lore_table)
                .where(lore_table.c.status == "published")
                .order_by(lore_table.c.published_at.desc())
            )
        ).fetchall()

    return _ok({"entries": [_row_to_dict(r) for r in rows], "count": len(rows)})


# ── POST /entries ─────────────────────────────────────────────────────────────

@router.post("/entries", summary="Create a draft lore entry (auth required)")
async def create_entry(
    body: LoreCreateRequest,
    current_user: dict = Depends(get_current_user),
) -> dict:
    kind = _validate_kind(body.kind)
    _validate_body_json(body.body_json)

    entry_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    engine = get_engine()
    async with engine.connect() as conn:
        await conn.execute(
            lore_table.insert().values(
                id=entry_id,
                title=body.title,
                body_json=body.body_json,
                kind=kind,
                status="draft",
                author_id=current_user["id"],
                created_at=now,
                published_at=None,
            )
        )
        await conn.commit()

    return _ok(
        {
            "id": entry_id,
            "title": body.title,
            "kind": kind,
            "status": "draft",
            "author_id": current_user["id"],
            "created_at": now.isoformat(),
            "published_at": None,
        }
    )


# ── PUT /entries/{id} ─────────────────────────────────────────────────────────

@router.put("/entries/{entry_id}", summary="Update own lore entry (auth required)")
async def update_entry(
    entry_id: str,
    body: LoreUpdateRequest,
    current_user: dict = Depends(get_current_user),
) -> dict:
    engine = get_engine()
    async with engine.connect() as conn:
        row = (
            await conn.execute(
                select(lore_table).where(lore_table.c.id == entry_id)
            )
        ).first()

        if row is None:
            raise HTTPException(status_code=404, detail="Entry not found")
        entry = dict(row._mapping)
        if entry["author_id"] != current_user["id"]:
            raise HTTPException(status_code=403, detail="Not your entry")

        updates: dict = {}
        if body.title is not None:
            updates["title"] = body.title
        if body.body_json is not None:
            _validate_body_json(body.body_json)
            updates["body_json"] = body.body_json
        if body.kind is not None:
            updates["kind"] = _validate_kind(body.kind)

        if updates:
            await conn.execute(
                lore_table.update().where(lore_table.c.id == entry_id).values(**updates)
            )
            await conn.commit()

        refreshed = dict(
            (
                await conn.execute(
                    select(lore_table).where(lore_table.c.id == entry_id)
                )
            ).first()._mapping
        )

    return _ok(_row_to_dict(refreshed))


# ── POST /entries/{id}/publish ────────────────────────────────────────────────

@router.post("/entries/{entry_id}/publish", summary="Publish a draft lore entry (auth required)")
async def publish_entry(
    entry_id: str,
    current_user: dict = Depends(get_current_user),
) -> dict:
    engine = get_engine()
    async with engine.connect() as conn:
        row = (
            await conn.execute(
                select(lore_table).where(lore_table.c.id == entry_id)
            )
        ).first()

        if row is None:
            raise HTTPException(status_code=404, detail="Entry not found")
        entry = dict(row._mapping)
        if entry["author_id"] != current_user["id"]:
            raise HTTPException(status_code=403, detail="Not your entry")
        if entry["status"] == "published":
            raise HTTPException(status_code=409, detail="Entry already published")

        now = datetime.now(timezone.utc)
        await conn.execute(
            lore_table.update()
            .where(lore_table.c.id == entry_id)
            .values(status="published", published_at=now)
        )
        await conn.commit()

    return _ok(
        {
            "id": entry_id,
            "status": "published",
            "published_at": now.isoformat(),
        }
    )
