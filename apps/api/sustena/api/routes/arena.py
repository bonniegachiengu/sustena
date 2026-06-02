"""
sustena/api/routes/arena.py

Mycelium Arena endpoints — public catalogue, real products, and order placement.

Routes
------
GET  /api/v1/arena/packages           — public; supports ?kind= filter
POST /api/v1/arena/packages           — auth required; publish a package
GET  /api/v1/arena/packages/{id}      — public; single package detail
GET  /api/v1/arena/products           — public; Vyyb + Mkulima products
POST /api/v1/arena/orders             — auth required; place an order
GET  /api/v1/arena/orders             — auth required; user order history
"""

import json
import logging
import random
import string
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel, Field

from sustena.api.routes.users import get_current_user

router = APIRouter()
logger = logging.getLogger(__name__)


# ── Demo product seed data ────────────────────────────────────────────────────

DEMO_PRODUCTS = [
    # Vyyb — food businesses
    {
        "id": "vyyb-001",
        "name": "Pilau Kando",
        "seller_type": "vyyb",
        "seller_name": "Mama Wanjiku's Kitchen",
        "price": 280,
        "unit": "per portion",
        "description": "Spiced rice with beef, served with kachumbari and yoghurt raita. Made fresh daily.",
        "tags": ["rice", "beef", "spiced"],
        "emoji": "🍛",
    },
    {
        "id": "vyyb-002",
        "name": "Nyama Choma Set",
        "seller_type": "vyyb",
        "seller_name": "Kamau's Grill",
        "price": 650,
        "unit": "500g plate",
        "description": "Chargrilled goat meat with ugali, kachumbari, and sukuma wiki. A Nairobi classic.",
        "tags": ["goat", "grilled", "set meal"],
        "emoji": "🍖",
    },
    {
        "id": "vyyb-003",
        "name": "Mutura & Chips",
        "seller_type": "vyyb",
        "seller_name": "Nairobi Street Kitchen",
        "price": 200,
        "unit": "per plate",
        "description": "Traditional Kenyan blood sausage with crispy chips. Street food done right.",
        "tags": ["mutura", "street food", "chips"],
        "emoji": "🌭",
    },
    {
        "id": "vyyb-004",
        "name": "Chai Bora",
        "seller_type": "vyyb",
        "seller_name": "Mama Wanjiku's Kitchen",
        "price": 80,
        "unit": "per cup",
        "description": "Ginger chai with fresh milk, cardamom, and cinnamon. Served hot in a thermos flask.",
        "tags": ["tea", "ginger", "hot drink"],
        "emoji": "☕",
    },
    # Mkulima — farms
    {
        "id": "mkulima-001",
        "name": "Sukuma Wiki",
        "seller_type": "mkulima",
        "seller_name": "Kijabe Valley Farm",
        "price": 35,
        "unit": "500g",
        "description": "Fresh kale from organic farm, harvested this morning. No pesticides.",
        "tags": ["kale", "organic", "greens"],
        "emoji": "🥬",
    },
    {
        "id": "mkulima-002",
        "name": "Nyanya Ndogo",
        "seller_type": "mkulima",
        "seller_name": "Thika Road Growers",
        "price": 90,
        "unit": "1kg",
        "description": "Cherry tomatoes, sweet and firm. Ideal for salads or slow-cooked stews.",
        "tags": ["tomatoes", "fresh", "salad"],
        "emoji": "🍅",
    },
    {
        "id": "mkulima-003",
        "name": "Ndengu Bulk",
        "seller_type": "mkulima",
        "seller_name": "Machakos Cooperative",
        "price": 140,
        "unit": "1kg",
        "description": "Dry green grams from Machakos County. High protein, long shelf life.",
        "tags": ["ndengu", "legumes", "protein"],
        "emoji": "🫘",
    },
    {
        "id": "mkulima-004",
        "name": "Viazi Ulaya",
        "seller_type": "mkulima",
        "seller_name": "Kijabe Valley Farm",
        "price": 70,
        "unit": "1kg",
        "description": "Irish potatoes, washed and sorted. Perfect for chapati or as a side.",
        "tags": ["potatoes", "starch", "staple"],
        "emoji": "🥔",
    },
]


async def seed_demo_products() -> None:
    """Insert demo products if the arena_products table is empty."""
    from sqlalchemy import text
    from sustena.db.schema import get_engine
    try:
        db_engine = get_engine()
        async with db_engine.begin() as conn:
            count = (await conn.execute(text("SELECT COUNT(*) FROM arena_products"))).scalar()
            if count and count > 0:
                return
            for p in DEMO_PRODUCTS:
                await conn.execute(
                    text(
                        "INSERT OR IGNORE INTO arena_products "
                        "(id, name, seller_type, seller_name, price, unit, "
                        "description, tags, emoji, is_available, created_at) "
                        "VALUES (:id, :name, :seller_type, :seller_name, :price, :unit, "
                        ":description, :tags, :emoji, :is_available, :created_at)"
                    ),
                    {
                        "id":           p["id"],
                        "name":         p["name"],
                        "seller_type":  p["seller_type"],
                        "seller_name":  p["seller_name"],
                        "price":        p["price"],
                        "unit":         p["unit"],
                        "description":  p["description"],
                        "tags":         json.dumps(p["tags"]),
                        "emoji":        p["emoji"],
                        "is_available": True,
                        "created_at":   datetime.utcnow(),
                    },
                )
        logger.info("Seeded %d demo arena products.", len(DEMO_PRODUCTS))
    except Exception as exc:
        logger.warning("seed_demo_products failed (non-fatal): %s", exc)


# ── Auth ──────────────────────────────────────────────────────────────────────

async def require_user(current_user=Depends(get_current_user)):
    if current_user is None:
        raise HTTPException(status_code=401, detail="Authentication required")
    return current_user


# ── Helpers ───────────────────────────────────────────────────────────────────

def _ok(data: Any) -> dict:
    return {"status": "ok", "data": data, "timestamp": datetime.now(timezone.utc).isoformat()}


def _make_ref() -> str:
    suffix = "".join(random.choices(string.ascii_uppercase + string.digits, k=6))
    return f"SXI-{suffix}"


def _row_to_package(row) -> dict:
    tags = []
    if row[10]:
        try:
            tags = json.loads(row[10])
        except Exception:
            pass
    return {
        "id":          row[0],
        "name":        row[1],
        "kind":        row[2],
        "spec_json":   json.loads(row[3]) if row[3] else None,
        "author_id":   row[4],
        "trust_score": row[5],
        "downloads":   row[6] or 0,
        "pawa_cost":   row[7],
        "is_free":     bool(row[8]),
        "version":     row[9] or "1.0.0",
        "tags":        tags,
        "description": row[11],
        "created_at":  row[12].isoformat() if hasattr(row[12], "isoformat") else str(row[12]),
        "pawa":        "FREE" if row[8] else str(row[7]),
        "trust":       round((row[5] or 0) * 100),
        "author":      row[4] or "unknown",
    }


def _row_to_product(row) -> dict:
    tags = []
    try:
        if row[7]:
            tags = json.loads(row[7])
    except Exception:
        pass
    return {
        "id":          row[0],
        "name":        row[1],
        "seller_type": row[2],
        "seller":      row[3],
        "price":       row[4],
        "unit":        row[5],
        "desc":        row[6] or "",
        "tags":        tags,
        "emoji":       row[8] or "",
        "source":      row[2],          # alias: ArenaPage filters by .source
    }


def _row_to_order(row) -> dict:
    items = []
    try:
        if row[3]:
            items = json.loads(row[3])
    except Exception:
        pass
    licenses = []
    try:
        if row[11]:
            licenses = json.loads(row[11])
    except Exception:
        pass
    created = row[12]
    return {
        "ref":           row[1],
        "date":          created.isoformat()[:10] if hasattr(created, "isoformat") else str(created)[:10],
        "time":          created.strftime("%H:%M") if hasattr(created, "strftime") else "",
        "items":         items,
        "sustain":       row[6] or "",
        "productTotal":  row[4] or 0,
        "paidPwaTotal":  row[5] or 0,
        "productStatus": row[9],
        "packageStatus": row[10],
        "deliveryAddr":  row[7] or "",
        "payMethod":     row[8] or "",
        "licenses":      licenses,
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


class CartItem(BaseModel):
    itemId: str | None = None
    name: str
    kind: str              # product | operative | operator | spore | widget
    qty: int = 1
    price: int | None = None
    unit: str | None = None
    emoji: str | None = None
    pawa: str | int | None = None


class CreateOrderRequest(BaseModel):
    items: list[CartItem] = Field(default_factory=list)
    sustain_id: str | None = None
    delivery_addr: str | None = None
    pay_method: str | None = None


# ── GET /api/v1/arena/packages ────────────────────────────────────────────────

@router.get("/packages", summary="List all arena packages")
async def list_packages(kind: str | None = Query(default=None)) -> dict:
    try:
        from sqlalchemy import text
        from sustena.db.schema import get_engine
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            q = (
                "SELECT id, name, kind, spec_json, author_id, trust_score, "
                "download_count, pawa_cost, is_free, version, tags, description, created_at "
                "FROM arena_packages"
                + (" WHERE kind = :kind" if kind else "")
                + " ORDER BY trust_score DESC"
            )
            params = {"kind": kind} if kind else {}
            rows = await conn.execute(text(q), params)
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
        "id":         pkg_id,
        "name":       body.name,
        "kind":       body.kind,
        "version":    body.version,
        "is_free":    body.is_free,
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

@router.get("/products", summary="List available products (Vyyb + Mkulima)")
async def list_products() -> dict:
    try:
        from sqlalchemy import text
        from sustena.db.schema import get_engine
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                text(
                    "SELECT id, name, seller_type, seller_name, price, unit, "
                    "description, tags, emoji "
                    "FROM arena_products WHERE is_available = 1 ORDER BY seller_type, name"
                )
            )
            products = [_row_to_product(r) for r in rows]
        return _ok({"products": products})
    except Exception as exc:
        logger.warning("list_products failed: %s", exc)
        return _ok({"products": []})


# ── POST /api/v1/arena/orders ─────────────────────────────────────────────────

@router.post("/orders", summary="Place an order", status_code=201)
async def create_order(
    body: CreateOrderRequest,
    current_user=Depends(require_user),
) -> dict:
    import uuid
    from sqlalchemy import text
    from sustena.db.schema import get_engine

    order_id = str(uuid.uuid4())
    ref = _make_ref()
    now = datetime.utcnow()

    items = [i.model_dump() for i in body.items]
    products = [i for i in items if i["kind"] == "product"]
    packages = [i for i in items if i["kind"] != "product"]

    product_total = sum((i["price"] or 0) * (i["qty"] or 1) for i in products)
    pawa_total = sum(
        int(i["pawa"]) for i in packages
        if i.get("pawa") and str(i["pawa"]).isdigit()
    )

    product_status = "PLACED" if products else None
    package_status = "PLACED" if packages else None

    licenses = [
        {"name": i["name"], "key": "LIC-" + "".join(random.choices(string.ascii_uppercase + string.digits, k=8))}
        for i in packages
        if not (str(i.get("pawa", "FREE")).upper() == "FREE" or i.get("pawa") == 0)
    ]

    db_engine = get_engine()
    async with db_engine.begin() as conn:
        await conn.execute(
            text(
                "INSERT INTO arena_orders "
                "(id, ref, user_id, items_json, product_total, pawa_total, "
                "sustain_id, delivery_addr, pay_method, product_status, package_status, "
                "licenses_json, created_at) "
                "VALUES (:id, :ref, :user_id, :items_json, :product_total, :pawa_total, "
                ":sustain_id, :delivery_addr, :pay_method, :product_status, :package_status, "
                ":licenses_json, :created_at)"
            ),
            {
                "id":             order_id,
                "ref":            ref,
                "user_id":        current_user["id"],
                "items_json":     json.dumps(items),
                "product_total":  product_total,
                "pawa_total":     pawa_total,
                "sustain_id":     body.sustain_id,
                "delivery_addr":  body.delivery_addr,
                "pay_method":     body.pay_method,
                "product_status": product_status,
                "package_status": package_status,
                "licenses_json":  json.dumps(licenses),
                "created_at":     now,
            },
        )

    return _ok({
        "ref":            ref,
        "product_status": product_status,
        "package_status": package_status,
        "licenses":       licenses,
        "created_at":     now.isoformat(),
    })


# ── GET /api/v1/arena/orders ──────────────────────────────────────────────────

@router.get("/orders", summary="User order history")
async def list_orders(current_user=Depends(require_user)) -> dict:
    try:
        from sqlalchemy import text
        from sustena.db.schema import get_engine
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                text(
                    "SELECT id, ref, user_id, items_json, product_total, pawa_total, "
                    "sustain_id, delivery_addr, pay_method, product_status, package_status, "
                    "licenses_json, created_at "
                    "FROM arena_orders WHERE user_id = :uid ORDER BY created_at DESC"
                ),
                {"uid": current_user["id"]},
            )
            orders = [_row_to_order(r) for r in rows]
        return _ok({"orders": orders})
    except Exception as exc:
        logger.warning("list_orders failed: %s", exc)
        return _ok({"orders": []})
