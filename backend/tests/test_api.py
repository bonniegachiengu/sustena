"""
tests/test_api.py

Epic 1.4.1 — FastAPI application structure tests.

Covers:
  - GET /health response shape and status
  - CORS headers on OPTIONS request
  - 404 on unknown routes
  - Request logging middleware smoke test
  - OpenAPI docs availability in development mode

All tests use dependency overrides — no real DB or API calls are made.

Run with: pytest tests/test_api.py -v
"""

import pytest
from httpx import AsyncClient, ASGITransport
from sqlalchemy import text

from sustena.api.main import app, get_db, get_claude_client


# ── Dependency overrides ───────────────────────────────────────────────────────


class _MockConn:
    """Minimal async connection stub that satisfies get_db callers."""

    async def execute(self, *args, **kwargs):
        return None

    async def commit(self):
        pass

    async def rollback(self):
        pass

    async def __aenter__(self):
        return self

    async def __aexit__(self, *args):
        pass


async def _override_get_db():
    yield _MockConn()


class _MockClaudeClient:
    pass


def _override_get_claude_client():
    return _MockClaudeClient()


@pytest.fixture(autouse=True)
def apply_overrides():
    """Apply dependency overrides for every test in this module."""
    app.dependency_overrides[get_db] = _override_get_db
    app.dependency_overrides[get_claude_client] = _override_get_claude_client
    yield
    app.dependency_overrides.clear()


# ── Helper ─────────────────────────────────────────────────────────────────────


def _client() -> AsyncClient:
    return AsyncClient(transport=ASGITransport(app=app), base_url="http://test")


# ── /health ────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_health_returns_200():
    """GET /health must always return 200."""
    async with _client() as client:
        response = await client.get("/health")
    assert response.status_code == 200


@pytest.mark.asyncio
async def test_health_has_required_keys():
    """GET /health body must include db_status, version, and timestamp."""
    async with _client() as client:
        response = await client.get("/health")
    data = response.json()
    assert "db_status" in data, "missing key: db_status"
    assert "version" in data, "missing key: version"
    assert "timestamp" in data, "missing key: timestamp"
    assert "status" in data, "missing key: status"
    assert "claude_status" in data, "missing key: claude_status"


@pytest.mark.asyncio
async def test_health_db_status_ok_when_db_accessible():
    """
    db_status should be 'ok' when the DB is reachable.
    In the test environment the DB is a real in-memory SQLite instance
    initialised at startup — check_db_health() should return True.
    """
    async with _client() as client:
        response = await client.get("/health")
    data = response.json()
    assert data["db_status"] == "ok", (
        f"Expected db_status='ok', got '{data['db_status']}'"
    )


@pytest.mark.asyncio
async def test_health_timestamp_is_iso8601():
    """timestamp field must be a non-empty ISO-8601 string."""
    from datetime import datetime
    async with _client() as client:
        response = await client.get("/health")
    ts = response.json().get("timestamp", "")
    assert ts, "timestamp must not be empty"
    # Parse to verify it's valid ISO-8601
    datetime.fromisoformat(ts)


@pytest.mark.asyncio
async def test_health_claude_status_values():
    """claude_status must be either 'ok' or 'unknown' — never anything else."""
    async with _client() as client:
        response = await client.get("/health")
    claude_status = response.json().get("claude_status")
    assert claude_status in ("ok", "unknown"), (
        f"Unexpected claude_status value: '{claude_status}'"
    )


# ── CORS ───────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_cors_headers_on_options():
    """
    OPTIONS preflight to /health must return CORS headers.
    In development mode allow_origins=["*"], so the response should include
    the Access-Control-Allow-Origin header.
    """
    async with _client() as client:
        response = await client.options(
            "/health",
            headers={
                "Origin": "http://localhost:3000",
                "Access-Control-Request-Method": "GET",
            },
        )
    # FastAPI/Starlette returns 200 for preflight when CORS is configured
    assert response.status_code == 200
    assert "access-control-allow-origin" in response.headers, (
        "CORS Access-Control-Allow-Origin header missing on OPTIONS response"
    )


# ── 404 ────────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_unknown_route_returns_404():
    """Requests to non-existent routes must return 404, not 500."""
    async with _client() as client:
        response = await client.get("/this/route/does/not/exist")
    assert response.status_code == 404


# ── Request logging smoke test ─────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_request_logging_does_not_crash():
    """
    The logging middleware must not raise or alter the response.
    A successful GET /health is sufficient to exercise the middleware path.
    """
    async with _client() as client:
        response = await client.get("/health")
    # If the middleware crashed the request would be 500
    assert response.status_code == 200


# ── OpenAPI docs ────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_openapi_docs_available_in_development():
    """
    /docs must return 200 when ENVIRONMENT=development (the default for tests).
    The app is created with docs_url="/docs" when settings.is_development is True.
    """
    async with _client() as client:
        response = await client.get("/docs")
    assert response.status_code == 200, (
        f"Expected /docs to be available in development, got {response.status_code}"
    )


@pytest.mark.asyncio
async def test_openapi_json_available_in_development():
    """The raw OpenAPI schema must be accessible at /openapi.json in development."""
    async with _client() as client:
        response = await client.get("/openapi.json")
    assert response.status_code == 200
    schema = response.json()
    assert "openapi" in schema
    assert schema.get("info", {}).get("title") == "Sustena XII"


# ── Stub router smoke tests ─────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_sustains_stub_route_reachable():
    """GET /api/v1/sustains/ must return 200 (stub route)."""
    async with _client() as client:
        response = await client.get("/api/v1/sustains/")
    assert response.status_code == 200


@pytest.mark.asyncio
async def test_operators_stub_route_reachable():
    """GET /api/v1/operators/ must return 200 (stub route)."""
    async with _client() as client:
        response = await client.get("/api/v1/operators/")
    assert response.status_code == 200


@pytest.mark.asyncio
async def test_council_stub_route_reachable():
    """GET /api/v1/council/ must return 200 (stub route)."""
    async with _client() as client:
        response = await client.get("/api/v1/council/")
    assert response.status_code == 200


@pytest.mark.asyncio
async def test_users_stub_route_reachable():
    """GET /api/v1/users/ must return 200 (stub route)."""
    async with _client() as client:
        response = await client.get("/api/v1/users/")
    assert response.status_code == 200
