"""
sustena/api/main.py
FastAPI application entry point for Sustena XII.
"""

import time
import logging
from contextlib import asynccontextmanager
from datetime import datetime, timezone
from typing import AsyncGenerator

from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse

from sustena.config import settings
from sustena.db.schema import init_db, get_engine

logger = logging.getLogger(__name__)

# -- Lifespan (startup / shutdown) --------------------------------------------


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Run startup tasks before yielding, shutdown tasks after."""
    logger.info("Starting Sustena XII -- %s", settings.environment)
    await init_db()
    yield
    logger.info("Sustena XII shutting down.")


# -- App factory --------------------------------------------------------------

app = FastAPI(
    title="Sustena XII",
    description="Human-agent reality interface. 7 primitives. Any describable system.",
    version="0.1.0",
    docs_url="/docs" if settings.is_development else None,
    redoc_url="/redoc" if settings.is_development else None,
    lifespan=lifespan,
)

# -- CORS ---------------------------------------------------------------------

ALLOWED_ORIGINS = (
    ["*"]
    if settings.is_development
    else [
        "https://sustena.io",
        "https://app.sustena.io",
    ]
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=ALLOWED_ORIGINS,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


# -- Request logging middleware ------------------------------------------------


@app.middleware("http")
async def log_requests(request: Request, call_next):
    start = time.perf_counter()
    response = await call_next(request)
    duration_ms = round((time.perf_counter() - start) * 1000, 1)
    logger.info(
        "%s %s -> %s (%sms)",
        request.method,
        request.url.path,
        response.status_code,
        duration_ms,
    )
    return response


# -- Dependency injection ------------------------------------------------------


async def get_db() -> AsyncGenerator:
    """
    Yields an async SQLAlchemy connection for use in route handlers.
    Rolls back on error; always closes on exit.
    """
    engine = get_engine()
    async with engine.connect() as conn:
        try:
            yield conn
            await conn.commit()
        except Exception:
            await conn.rollback()
            raise


def get_claude_client():
    """
    Returns the appropriate Claude client (real or mock) for this environment.
    Routes can depend on this for LLM access.
    """
    from sustena.core.claude_client import get_claude_client as _factory
    return _factory()


# -- Health check --------------------------------------------------------------


@app.get("/health", tags=["system"])
async def health():
    """
    Liveness probe -- used by Cloud Run and monitoring.
    Returns db and claude API status.

    claude_status: "ok" if ANTHROPIC_API_KEY is set to a non-mock value,
                   "unknown" if the key is absent or set to the mock placeholder.
    Always returns HTTP 200 -- callers should inspect the body fields.
    """
    from sustena.db.schema import check_db_health

    db_ok = await check_db_health()

    api_key = settings.anthropic_api_key
    is_real_key = bool(api_key) and api_key.strip().lower() not in ("mock", "placeholder", "")
    claude_status = "ok" if is_real_key else "unknown"

    return JSONResponse(
        status_code=200,
        content={
            "status": "ok" if db_ok else "degraded",
            "version": "0.1.0",
            "environment": settings.environment,
            "db_status": "ok" if db_ok else "error",
            "claude_status": claude_status,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        },
    )


# -- Routers ------------------------------------------------------------------

from sustena.api.routes import whatsapp, dev
from sustena.api.routes import sustains, operators, council, users

# WhatsApp webhook (always mounted)
app.include_router(whatsapp.router, prefix="/webhook", tags=["whatsapp"])

# REST v1 resources
app.include_router(sustains.router,  prefix="/api/v1/sustains",  tags=["sustains"])
app.include_router(operators.router, prefix="/api/v1/operators", tags=["operators"])
app.include_router(council.router,   prefix="/api/v1/council",   tags=["council"])
app.include_router(users.router,     prefix="/api/v1/users",     tags=["users"])

# Dev-only simulation and inspection endpoints
if settings.is_development:
    app.include_router(dev.router, prefix="/dev", tags=["dev"])
    logger.info("Dev endpoints mounted at /dev/* (development mode)")
