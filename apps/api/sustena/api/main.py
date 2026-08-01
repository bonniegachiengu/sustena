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

# -- Deploy identity -----------------------------------------------------------
# The git commit this running process was actually started from -- resolved
# ONCE at import time (a process never picks up a later `git commit` without a
# real restart, so this is honest: it names what code is genuinely loaded into
# memory right now, not what's on disk this instant). Exists specifically so
# staleness (a running process silently predating the code on disk) is a
# one-line diff against `git rev-parse HEAD` instead of a guessing game --
# this exact class of bug (a stale 0.0.0.0-bound process serving an old
# commit indefinitely) has recurred multiple times per CLAUDE.md's own slice
# notes. Best-effort: a packaged deploy with no .git directory, or git not on
# PATH, must never crash the app over a diagnostic field.


def _resolve_running_commit() -> str:
    import subprocess

    try:
        result = subprocess.run(
            ["git", "rev-parse", "--short=12", "HEAD"],
            cwd=_Path(__file__).resolve().parent,
            capture_output=True,
            text=True,
            timeout=5,
        )
        if result.returncode == 0:
            return result.stdout.strip()
    except Exception:
        pass
    return "unknown"


from pathlib import Path as _Path

_RUNNING_COMMIT = _resolve_running_commit()

# -- Lifespan (startup / shutdown) --------------------------------------------


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Run startup tasks before yielding, shutdown tasks after."""
    logger.info("Starting Sustena XII -- %s", settings.environment)
    from sustena.core.claude_client import get_claude_client
    _client = get_claude_client()
    _mode = "mock mode (zero API calls)" if _client.__class__.__name__ == "MockClaudeClient" else "live mode"
    logger.info("Claude client: %s", _mode)
    await init_db()

    # Seed one homestead sustain if the persistent DB is empty
    try:
        from sustena.core.engine_singleton import get_shared_engine
        _engine = get_shared_engine()
        if not _engine.list_all():
            sid = _engine.instantiate(
                "homestead",
                "system",
                {"owner_ids": ["system"]},
            )
            logger.info("Seeded homestead sustain on startup: %s", sid)
    except Exception as exc:
        logger.warning("Startup seed failed (non-fatal): %s", exc)


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
            "git_commit": _RUNNING_COMMIT,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        },
    )


# -- Routers ------------------------------------------------------------------

from sustena.api.routes import whatsapp, dev
from sustena.api.routes import sustains, operators, council, users
from sustena.api.routes import devui
from sustena.api.routes import orchie
from sustena.api.routes import seed
from sustena.api.routes import lore, journal
from sustena.api.routes import arena
from sustena.api.routes import ingest

# WhatsApp webhook (always mounted)
app.include_router(whatsapp.router, prefix="/webhook", tags=["whatsapp"])

# Orchie conversational interface
app.include_router(orchie.router, prefix="/orchie", tags=["orchie"])

# REST v1 resources
app.include_router(sustains.router,  prefix="/api/v1/sustains",  tags=["sustains"])
app.include_router(operators.router, prefix="/api/v1/operators", tags=["operators"])
app.include_router(council.router,   prefix="/api/v1/council",   tags=["council"])
app.include_router(users.router,     prefix="/api/v1/users",     tags=["users"])
app.include_router(lore.router,      prefix="/api/v1/lore",      tags=["lore"])
app.include_router(journal.router,   prefix="/api/v1/journal",   tags=["journal"])
app.include_router(arena.router,     prefix="/api/v1/arena",     tags=["arena"])
app.include_router(ingest.router,    prefix="/api/v1/ingest",    tags=["ingest"])

# Dev-only simulation and inspection endpoints
if settings.is_development:
    app.include_router(dev.router, prefix="/dev", tags=["dev"])
    app.include_router(devui.router, prefix="/devui", tags=["devui"])
    app.include_router(seed.router, prefix="/seed", tags=["seed"])


# -- Serve the built frontend (single hostname for UI + API) ------------------
# apps/web/dist doesn't exist in a fresh checkout or in test runs (no `npm run
# build` has happened) -- guard on it so importing this module never breaks
# pytest or a backend-only dev setup. Registered LAST: FastAPI matches routes
# in registration order, so every API route above still takes precedence over
# the catch-all below.

from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

_DIST_DIR = _Path(__file__).resolve().parent.parent.parent.parent / "web" / "dist"

if _DIST_DIR.is_dir():
    app.mount("/assets", StaticFiles(directory=_DIST_DIR / "assets"), name="frontend-assets")

    _DIST_DIR_RESOLVED = _DIST_DIR.resolve()

    @app.get("/{full_path:path}", include_in_schema=False)
    async def serve_frontend(full_path: str) -> FileResponse:
        """
        Real static files first (manifest.webmanifest, sw.js, offline.html,
        icons/*, and anything else Vite copied from public/ into dist/ root
        — NOT just /assets, which is the only path StaticFiles is mounted
        at above), SPA fallback second.

        Found live while building the PWA slice: this route used to return
        index.html unconditionally for every unmatched path, which meant
        GET /manifest.webmanifest and GET /sw.js were silently serving HTML
        instead of the real files — a browser cannot install a PWA whose
        manifest is an HTML document, and a service worker script that
        isn't valid JavaScript fails registration outright. FileResponse
        infers the correct Content-Type from the extension automatically
        (application/manifest+json, application/javascript, image/png,
        ...), so no explicit media_type wiring was needed once the file is
        actually being served.

        A client-side react-router path (e.g. /profile) or a genuine 404
        both fall through to index.html exactly as before — that behavior
        is unchanged; only real on-disk files now win over it.
        """
        candidate = (_DIST_DIR / full_path).resolve()
        if candidate.is_file() and candidate.is_relative_to(_DIST_DIR_RESOLVED):
            return FileResponse(candidate)
        return FileResponse(_DIST_DIR / "index.html")

    logger.info("Serving built frontend from %s", _DIST_DIR)
else:
    logger.info("No apps/web/dist found at %s -- API-only mode (run `npm run build` in apps/web to enable single-hostname serving).", _DIST_DIR)
