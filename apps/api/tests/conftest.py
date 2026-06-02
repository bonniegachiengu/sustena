"""
tests/conftest.py — Shared fixtures for the Sustena XII test suite.

Problems this file solves
─────────────────────────
1. Engine-per-event-loop contamination
   SQLAlchemy's async engine (_engine in schema.py) is a module-level
   singleton bound to the event loop that first creates it.
   pytest-asyncio (asyncio_mode=auto) creates a *new* loop per test
   function; starlette TestClient creates its own via anyio.
   Reusing the engine across loops triggers an aiosqlite PRAGMA failure
   during metadata.create_all introspection.
   Fix: reset _engine = None at the start of every module AND every function.

2. Test database isolation
   Tests use sqlite+aiosqlite:///:memory: — ephemeral, no disk writes,
   no state leakage between runs.  Each lifespan startup creates a fresh
   in-memory DB, so every test module starts clean.

3. Required env vars
   Set defaults before any sustena import so pydantic BaseSettings picks
   them up at instantiation time (env vars beat .env file in pydantic
   priority order).
"""

import os

# ── Must happen before any `sustena.*` import ─────────────────────────────────
os.environ.setdefault("DATABASE_URL",         "sqlite+aiosqlite:///:memory:")
os.environ.setdefault("ANTHROPIC_API_KEY",    "mock")
os.environ.setdefault("ENVIRONMENT",          "development")
os.environ.setdefault("ADMIN_TOKEN",          "dev-admin-token")
os.environ.setdefault("SECRET_KEY",           "dev-secret-change-in-production")
os.environ.setdefault("WHATSAPP_TOKEN",       "mock")
os.environ.setdefault("WHATSAPP_PHONE_ID",    "mock")

import pytest
import sustena.db.schema as _schema
import sustena.core.engine_singleton as _singleton


# ── Engine reset — module scope ───────────────────────────────────────────────

@pytest.fixture(autouse=True, scope="module")
def _reset_engine_module():
    """
    Reset the engine singleton before each test module so that
    module-scoped TestClient fixtures create a fresh engine in the
    correct anyio event loop.
    """
    _schema._engine = None
    _singleton.reset_shared_engine()
    yield
    _schema._engine = None
    _singleton.reset_shared_engine()


# ── Engine reset — function scope ─────────────────────────────────────────────

@pytest.fixture(autouse=True, scope="function")
def _reset_engine_function():
    """
    Reset before each async test function that owns its own event loop
    (pytest-asyncio asyncio_mode=auto creates one per function).
    Ensures AsyncClient-based tests always get a fresh engine.
    """
    _schema._engine = None
    yield
    _schema._engine = None
