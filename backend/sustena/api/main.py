"""
sustena/api/main.py
FastAPI application entry point for Sustena XII.
"""

import time
import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse

from sustena.config import settings
from sustena.db.schema import init_db

logger = logging.getLogger(__name__)

# ── Lifespan (startup / shutdown) ─────────────────────────────────────────────


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Run startup tasks before yielding, shutdown tasks after."""
    logger.info("Starting Sustena XII — %s", settings.environment)
    await init_db()
    yield
    logger.info("Sustena XII shutting down.")


# ── App factory ───────────────────────────────────────────────────────────────

app = FastAPI(
    title="Sustena XII",
    description="Human-agent reality interface. 7 primitives. Any describable system.",
    version="0.1.0",
    docs_url="/docs" if settings.is_development else None,
    redoc_url="/redoc" if settings.is_development else None,
    lifespan=lifespan,
)

# ── CORS ──────────────────────────────────────────────────────────────────────

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


# ── Request logging middleware ────────────────────────────────────────────────


@app.middleware("http")
async def log_requests(request: Request, call_next):
    start = time.perf_counter()
    response = await call_next(request)
    duration_ms = round((time.perf_counter() - start) * 1000, 1)
    logger.info(
        "%s %s → %s (%sms)",
        request.method,
        request.url.path,
        response.status_code,
        duration_ms,
    )
    return response


# ── Health check ──────────────────────────────────────────────────────────────


@app.get("/health", tags=["system"])
async def health():
    """
    Liveness probe — used by Cloud Run and monitoring.
    Returns db and claude API status.
    """
    from sustena.db.schema import check_db_health
    from anthropic import AsyncAnthropic

    db_ok = await check_db_health()

    claude_ok = False
    try:
        from sustena.core.claude_client import get_claude_client
        client = get_claude_client()
        await client.messages.create(
            model=settings.claude_haiku_model,
            max_tokens=1,
            messages=[{"role": "user", "content": "ping"}],
        )
        claude_ok = True
    except Exception as e:
        logger.warning("Claude health check failed: %s", e)

    status = "ok" if (db_ok and claude_ok) else "degraded"
    return JSONResponse(
        status_code=200 if status == "ok" else 503,
        content={
            "status": status,
            "version": "0.1.0",
            "environment": settings.environment,
            "db_status": "ok" if db_ok else "error",
            "claude_status": "ok" if claude_ok else "error",
        },
    )


# ── Routers ───────────────────────────────────────────────────────────────────

from sustena.api.routes import whatsapp, dev

# WhatsApp webhook (real — always mounted)
app.include_router(whatsapp.router, prefix="/webhook", tags=["whatsapp"])

# Dev-only simulation and inspection endpoints
if settings.is_development:
    app.include_router(dev.router, prefix="/dev", tags=["dev"])
    logger.info("Dev endpoints mounted at /dev/* (development mode)")

# Future routers (uncomment as each Epic is completed):
# from sustena.api.routes import sustains, users
# app.include_router(sustains.router, prefix="/api/v1/sustains", tags=["sustains"])
# app.include_router(users.router,    prefix="/api/v1/users",    tags=["users"])
