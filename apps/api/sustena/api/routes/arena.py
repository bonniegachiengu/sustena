"""
sustena/api/routes/arena.py

Mycelium Arena endpoints — public catalogue + authenticated publishing.

Routes
------
GET  /api/v1/arena/packages           — public; supports ?kind= filter
POST /api/v1/arena/packages           — auth required; create a package
GET  /api/v1/arena/packages/{id}      — public; single package detail
GET  /api/v1/arena/products           — public; returns [] (products live in Sprint 8.9)
POST /api/v1/arena/orders             — auth required; stub until Sprint 8.9
GET  /api/v1/arena/orders             — auth required; stub until Sprint 8.9
"""

import json
import logging
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel, Field

from sustena.api.routes.users import get_current_user

router = APIRouter()
logger = logging.getLogger(__name__)


# ── Auth ──────────────────────────────────────────────────────────────────────

async def require_user(current_user=Depends(get_current_user)):
    if current_user is None:
        raise HTTPException(status_code=401, detail="Authentication required")
    return current_user


# ── Helpers ───────────────────────────────────────────────────────────────────

def _ok(data: Any) -> dict:
    return {"status": "ok", "data": data, "timestamp": datetime.now(timezone.utc).isoformat()}


def _row_to_package(row) -> dict:
    tags = []
    if row[10]:
        try:
            tags = json.loads(row[10])
        except Exception:
            tags = []
    return {
        "id":             row[0],
        "name":           row[1],
        "kind":           row[2],
        "spec_json":      json.loads(row[3]) if row[3] else None,
        "author_id":      row[4],
        "trust_score":    row[5],
        "download_count": row[6],
        "pawa_cost":      row[7],
        "is_free":        bool(row[8]),
        "version":        row[9],
        "tags":           tags,
        "description":    row[11],
        "created_at":     row[12].isoformat() if hasattr(row[12], "isoformat") else str(row[12]),
        "pawa":           "FREE" if row[8] else str(row[7]),
        "trust":          round((row[5] or 0) * 100),
        "downloads":      row[6] or 0,
    }


# ── Models ────────────────────────────────────────────────────────────────────

class CreatePackageRequest(BaseModel):
    name: str = Field(..., min_length=1, max_length=200)
    kind: str = Field(..., pattern="^(operative|operator|spore|widget)$")
    description: str | None = None
    spec_json: dict | None = None
    tags: list[str] = Field(default_factory=list)
    version: str = "1.0.0"
    pawa_cost: int = 0
    is_free: bool = True


# ── GET /api/v1/arena/packages ────────────────────────────────────────────────

@router.get("/packages", summary="List all arena packages")
async def list_packages(kind: str | None = Query(default=None)) -> dict:
    try:
        import uuid
        from sqlalchemy import text
        from sustena.db.schema import get_engine
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            if kind:
                rows = await conn.execute(
                    text(
                        "SELECT id, name, kind, spec_json, author_id, trust_score, "
                        "download_count, pawa_cost, is_free, version, tags, description, created_at "
                        "FROM arena_packages WHERE kind = :kind ORDER BY trust_score DESC"
                    ),
                    {"kind": kind},
                )
            else:
                rows = await conn.execute(
                    text(
                        "SELECT id, name, kind, spec_json, author_id, trust_score, "
                        "download_count, pawa_cost, is_free, version, tags, description, created_at "
                        "FROM arena_packages ORDER BY trust_score DESC"
                    )
                )
            packages = [_row_to_package(r) for r in rows]
        return _ok({"packages": packages})
    except Exception as exc:
        logger.warning("list_packages failed: %s", exc)
        return _ok({"packages": []})


# ── POST /api/v1/arena/packages ───────────────────────────────────────────────

@router.post("/packages", summary="Publish a new package", status_code=201)
async def create_package(
    body: CreatePackageRequest,
    current_user=Depends(require_user),
) -> dict:
    import uuid
    from sqlalchemy import text
    from sustena.db.schema import get_engine
    pkg_id = str(uuid.uuid4())
    now = datetime.utcnow()
    db_engine = get_engine()
    async with db_engine.begin() as conn:
        await conn.execute(
            text(
                "INSERT INTO arena_packages "
                "(id, name, kind, spec_json, author_id, trust_score, download_count, "
                "pawa_cost, is_free, tags, description, version, created_at) "
                "VALUES (:id, :name, :kind, :spec_json, :author_id, :trust_score, :download_count, "
                ":pawa_cost, :is_free, :tags, :description, :version, :created_at)"
            ),
            {
                "id":             pkg_id,
                "name":           body.name,
                "kind":           body.kind,
                "spec_json":      json.dumps(body.spec_json) if body.spec_json else None,
                "author_id":      current_user["id"],
                "trust_score":    0.0,
                "download_count": 0,
                "pawa_cost":      body.pawa_cost,
                "is_free":        body.is_free,
                "tags":           json.dumps(body.tags),
                "description":    body.description,
                "version":        body.version,
                "created_at":     now,
            },
        )
    return _ok({
        "id": pkg_id,
        "name": body.name,
        "kind": body.kind,
        "version": body.version,
        "is_free": body.is_free,
        "created_at": now.isoformat(),
    })


# ── GET /api/v1/arena/packages/{id} ──────────────────────────────────────────

@router.get("/packages/{pkg_id}", summary="Single package detail")
async def get_package(pkg_id: str) -> dict:
    try:
        from sqlalchemy import text
        from sustena.db.schema import get_engine
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            row = (await conn.execute(
                text(
                    "SELECT id, name, kind, spec_json, author_id, trust_score, "
                    "download_count, pawa_cost, is_free, version, tags, description, created_at "
                    "FROM arena_packages WHERE id = :id"
                ),
                {"id": pkg_id},
            )).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="Package not found")
        return _ok({"package": _row_to_package(row)})
    except HTTPException:
        raise
    except Exception as exc:
        logger.warning("get_package(%s) failed: %s", pkg_id, exc)
        raise HTTPException(status_code=500, detail="Failed to fetch package")


# ── GET /api/v1/arena/products ────────────────────────────────────────────────

@router.get("/products", summary="List arena products (stub until Sprint 8.9)")
async def list_products() -> dict:
    return _ok({"products": []})


# ── POST /api/v1/arena/orders ─────────────────────────────────────────────────

@router.post("/orders", summary="Place an order (stub until Sprint 8.9)", status_code=201)
async def create_order(current_user=Depends(require_user)) -> dict:
    return _ok({"message": "orders live in Sprint 8.9"})


# ── GET /api/v1/arena/orders ──────────────────────────────────────────────────

@router.get("/orders", summary="List user orders (stub until Sprint 8.9)")
async def list_orders(current_user=Depends(require_user)) -> dict:
    return _ok({"orders": []})
