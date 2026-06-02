"""
sustena/api/routes/users.py

User registration, login, and profile endpoints.
Sprint 8.6 — Auth/Users

Routes (all under /api/v1/users, mounted by main.py):
  POST /register          — create account (email + password) → JWT
  POST /login             — authenticate → JWT
  GET  /me                — current user profile
  GET  /me/stats          — sustain count, pawa balance, operator execution count
  GET  /me/activity       — last 20 operator executions
"""

import hashlib
import logging
import secrets
import uuid
from datetime import datetime, timedelta, timezone

from fastapi import APIRouter, Depends, HTTPException
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer
from jose import JWTError, jwt
from jose.exceptions import ExpiredSignatureError
from pydantic import BaseModel, Field
from sqlalchemy import func, select

from sustena.config import settings
from sustena.db.schema import get_engine
from sustena.db.schema import arena_orders as orders_table
from sustena.db.schema import arena_packages as packages_table
from sustena.db.schema import council_proposals as proposals_table
from sustena.db.schema import lore_entries as lore_table
from sustena.db.schema import operators_log as ops_log_table
from sustena.db.schema import sustains as sustains_table
from sustena.db.schema import users as users_table

logger = logging.getLogger(__name__)

router = APIRouter()

# ── Security helpers ──────────────────────────────────────────────────────────

_ALGORITHM = "HS256"
_TOKEN_EXPIRE_DAYS = 30
_PBKDF2_ITERATIONS = 260_000
_bearer = HTTPBearer(auto_error=False)


def _hash_password(password: str) -> str:
    salt = secrets.token_hex(32)
    dk = hashlib.pbkdf2_hmac("sha256", password.encode(), salt.encode(), _PBKDF2_ITERATIONS)
    return f"{salt}${dk.hex()}"


def _verify_password(plain: str, stored: str) -> bool:
    try:
        salt, dk_hex = stored.split("$", 1)
        expected = hashlib.pbkdf2_hmac("sha256", plain.encode(), salt.encode(), _PBKDF2_ITERATIONS)
        return secrets.compare_digest(expected.hex(), dk_hex)
    except Exception:
        return False


def _create_token(user_id: str) -> str:
    expire = datetime.now(timezone.utc) + timedelta(days=_TOKEN_EXPIRE_DAYS)
    return jwt.encode(
        {"user_id": user_id, "exp": expire},
        settings.secret_key,
        algorithm=_ALGORITHM,
    )


async def get_current_user(
    credentials: HTTPAuthorizationCredentials | None = Depends(_bearer),
) -> dict:
    if credentials is None:
        raise HTTPException(status_code=401, detail="Authorization required")
    try:
        payload = jwt.decode(
            credentials.credentials, settings.secret_key, algorithms=[_ALGORITHM]
        )
        user_id: str = payload.get("user_id")
        if not user_id:
            raise HTTPException(status_code=401, detail="Invalid token")
    except ExpiredSignatureError:
        raise HTTPException(status_code=401, detail="Token expired")
    except JWTError:
        raise HTTPException(status_code=401, detail="Invalid token")

    db_engine = get_engine()
    async with db_engine.connect() as conn:
        row = (
            await conn.execute(select(users_table).where(users_table.c.id == user_id))
        ).first()

    if row is None:
        raise HTTPException(status_code=401, detail="User not found")
    return dict(row._mapping)


# ── Pydantic models ──────────────────────────────────────────────────────────

class RegisterRequest(BaseModel):
    email: str = Field(..., description="User email address")
    password: str = Field(..., min_length=8, description="Password (min 8 characters)")
    display_name: str | None = Field(default=None, max_length=100)


class LoginRequest(BaseModel):
    email: str
    password: str


# ── Helpers ──────────────────────────────────────────────────────────────────

def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def _ok(data) -> dict:
    return {"status": "ok", "data": data, "error": None, "timestamp": _now_iso()}


def _serialize_dt(val) -> str:
    if val is None:
        return None
    return val.isoformat() if hasattr(val, "isoformat") else str(val)


# ── POST /register ────────────────────────────────────────────────────────────

@router.post("/register", summary="Register a new user account")
async def register(body: RegisterRequest) -> dict:
    db_engine = get_engine()
    async with db_engine.connect() as conn:
        existing = (
            await conn.execute(
                select(users_table.c.id).where(users_table.c.email == body.email)
            )
        ).first()
        if existing:
            raise HTTPException(status_code=409, detail="Email already registered")

        user_id = str(uuid.uuid4())
        now = datetime.now(timezone.utc)
        display_name = body.display_name or body.email.split("@")[0]
        await conn.execute(
            users_table.insert().values(
                id=user_id,
                email=body.email,
                password_hash=_hash_password(body.password),
                display_name=display_name,
                phone_number=None,
                pawa_balance=100,
                created_at=now,
                last_active_at=now,
            )
        )
        await conn.commit()

    return _ok(
        {
            "user_id": user_id,
            "email": body.email,
            "display_name": display_name,
            "token": _create_token(user_id),
        }
    )


# ── POST /login ───────────────────────────────────────────────────────────────

@router.post("/login", summary="Authenticate and receive a JWT")
async def login(body: LoginRequest) -> dict:
    db_engine = get_engine()
    async with db_engine.connect() as conn:
        row = (
            await conn.execute(
                select(users_table).where(users_table.c.email == body.email)
            )
        ).first()

    if row is None:
        raise HTTPException(status_code=401, detail="Invalid email or password")

    user = dict(row._mapping)
    if not user.get("password_hash") or not _verify_password(body.password, user["password_hash"]):
        raise HTTPException(status_code=401, detail="Invalid email or password")

    db_engine = get_engine()
    async with db_engine.connect() as conn:
        await conn.execute(
            users_table.update()
            .where(users_table.c.id == user["id"])
            .values(last_active_at=datetime.now(timezone.utc))
        )
        await conn.commit()

    return _ok(
        {
            "user_id": user["id"],
            "email": body.email,
            "display_name": user.get("display_name"),
            "token": _create_token(user["id"]),
        }
    )


# ── GET /me ───────────────────────────────────────────────────────────────────

@router.get("/me", summary="Current user profile")
async def get_me(current_user: dict = Depends(get_current_user)) -> dict:
    return _ok(
        {
            "user_id": current_user["id"],
            "email": current_user.get("email"),
            "display_name": current_user.get("display_name"),
            "pawa_balance": current_user.get("pawa_balance", 0),
            "created_at": _serialize_dt(current_user.get("created_at")),
        }
    )


# ── GET /me/stats ─────────────────────────────────────────────────────────────

@router.get("/me/stats", summary="Current user statistics")
async def get_me_stats(current_user: dict = Depends(get_current_user)) -> dict:
    user_id = current_user["id"]
    db_engine = get_engine()
    async with db_engine.connect() as conn:
        user_sustain_ids = select(sustains_table.c.id).where(
            sustains_table.c.user_id == user_id
        )

        sustain_count = (
            await conn.execute(
                select(func.count()).select_from(sustains_table)
                .where(sustains_table.c.user_id == user_id)
            )
        ).scalar() or 0

        op_count = (
            await conn.execute(
                select(func.count()).select_from(ops_log_table)
                .where(ops_log_table.c.sustain_id.in_(user_sustain_ids))
            )
        ).scalar() or 0

        proposals_passed = (
            await conn.execute(
                select(func.count()).select_from(proposals_table)
                .where(
                    proposals_table.c.sustain_id.in_(user_sustain_ids),
                    proposals_table.c.status == "PASSED",
                )
            )
        ).scalar() or 0

        proposals_in_voting = (
            await conn.execute(
                select(func.count()).select_from(proposals_table)
                .where(
                    proposals_table.c.sustain_id.in_(user_sustain_ids),
                    proposals_table.c.status == "IN_VOTING",
                )
            )
        ).scalar() or 0

        orders_placed = (
            await conn.execute(
                select(func.count()).select_from(orders_table)
                .where(orders_table.c.user_id == user_id)
            )
        ).scalar() or 0

        lore_published = (
            await conn.execute(
                select(func.count()).select_from(lore_table)
                .where(
                    lore_table.c.author_id == user_id,
                    lore_table.c.status == "published",
                )
            )
        ).scalar() or 0

        packages_published = (
            await conn.execute(
                select(func.count()).select_from(packages_table)
                .where(packages_table.c.author_id == user_id)
            )
        ).scalar() or 0

    return _ok(
        {
            "user_id":             user_id,
            "pawa_balance":        current_user.get("pawa_balance", 0),
            "sustain_count":       sustain_count,
            "operator_executions": op_count,
            "proposals_passed":    proposals_passed,
            "proposals_in_voting": proposals_in_voting,
            "orders_placed":       orders_placed,
            "lore_published":      lore_published,
            "packages_published":  packages_published,
        }
    )


# ── GET /me/council ───────────────────────────────────────────────────────────

@router.get("/me/council", summary="Recent council proposals for the current user's sustains")
async def get_me_council(current_user: dict = Depends(get_current_user)) -> dict:
    user_id = current_user["id"]
    db_engine = get_engine()
    async with db_engine.connect() as conn:
        user_sustain_ids = select(sustains_table.c.id).where(
            sustains_table.c.user_id == user_id
        )
        rows = (
            await conn.execute(
                select(proposals_table)
                .where(proposals_table.c.sustain_id.in_(user_sustain_ids))
                .order_by(proposals_table.c.created_at.desc())
                .limit(20)
            )
        ).fetchall()

    proposals = [
        {
            "id":            r.id,
            "sustain_id":    r.sustain_id,
            "operator_name": r.operator_name,
            "proposed_by":   r.proposed_by,
            "status":        r.status,
            "created_at":    _serialize_dt(r.created_at),
            "resolved_at":   _serialize_dt(r.resolved_at),
        }
        for r in rows
    ]

    return _ok({"proposals": proposals, "count": len(proposals)})


# ── GET /me/activity ──────────────────────────────────────────────────────────

@router.get("/me/activity", summary="Recent operator activity for the current user")
async def get_me_activity(current_user: dict = Depends(get_current_user)) -> dict:
    user_id = current_user["id"]
    db_engine = get_engine()
    async with db_engine.connect() as conn:
        user_sustain_ids = select(sustains_table.c.id).where(
            sustains_table.c.user_id == user_id
        )
        rows = (
            await conn.execute(
                select(ops_log_table)
                .where(ops_log_table.c.sustain_id.in_(user_sustain_ids))
                .order_by(ops_log_table.c.timestamp.desc())
                .limit(20)
            )
        ).fetchall()

    activity = [
        {
            "id": r.id,
            "sustain_id": r.sustain_id,
            "operator_name": r.operator_name,
            "status": r.status,
            "pawa_cost": r.pawa_cost,
            "timestamp": _serialize_dt(r.timestamp),
        }
        for r in rows
    ]

    return _ok({"user_id": user_id, "activity": activity, "count": len(activity)})
