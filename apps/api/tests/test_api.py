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
async def test_sustains_route_reachable():
    """POST /api/v1/sustains/ without auth returns 401 (route is mounted and requires JWT)."""
    async with _client() as client:
        response = await client.post(
            "/api/v1/sustains/",
            json={"template_id": "homestead", "parameters": {}},
        )
    assert response.status_code == 401


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
async def test_users_register_reachable():
    """POST /api/v1/users/register must return 200 or 422 (not 404)."""
    async with _client() as client:
        response = await client.post("/api/v1/users/register", json={})
    assert response.status_code != 404


# =============================================================
# Epic 1.4.2 -- Sustain route tests
# =============================================================

import json as _json
from datetime import datetime as _dt, timedelta as _td, timezone as _tz
from unittest.mock import AsyncMock, MagicMock

from jose import jwt as pyjwt
import pytest
from httpx import AsyncClient, ASGITransport

from sustena.api.routes import sustains as _sm
from sustena.core.operator import OperatorResult


def _tok(user_id="user-abc", exp=3600):
    from sustena.config import settings
    payload = {"user_id": user_id, "exp": _dt.now(_tz.utc) + _td(seconds=exp)}
    return pyjwt.encode(payload, settings.secret_key, algorithm="HS256")


def _hdrs(user_id="user-abc"):
    return {"Authorization": "Bearer " + _tok(user_id)}


def _mk_engine(
    sustain_id="sustain-123",
    user_id="user-abc",
    template_id="homestead",
    created_at="2026-01-01T00:00:00",
    state=None,
    execute_result=None,
    instantiate_exc=None,
):
    if state is None:
        state = {"finances": {"liquid": {"balance": 10000}}, "council_proposals": [], "council_votes": []}

    eng = MagicMock()
    eng._db = MagicMock()

    def _exec(sql, params=()):
        row = MagicMock()
        res = MagicMock()
        if "SELECT user_id FROM sustains" in sql:
            row.__getitem__ = lambda s, k: user_id if k == "user_id" else None
            res.fetchone.return_value = row
        elif "SELECT created_at, template_id FROM sustains" in sql:
            def _gi(s, k):
                return created_at if k == "created_at" else template_id
            row.__getitem__ = _gi
            res.fetchone.return_value = row
        elif "SELECT template_id FROM sustains" in sql:
            row.__getitem__ = lambda s, k: template_id
            res.fetchone.return_value = row
        elif "SELECT * FROM events" in sql:
            res.fetchall.return_value = []
        else:
            res.fetchone.return_value = None
            res.fetchall.return_value = []
        return res

    eng._db.execute.side_effect = _exec

    if instantiate_exc:
        eng.instantiate.side_effect = instantiate_exc
    else:
        eng.instantiate.return_value = sustain_id

    eng.get_state.return_value = state

    if execute_result is None:
        execute_result = OperatorResult.ok({"done": True})
    eng.execute_operator = AsyncMock(return_value=execute_result)
    eng._get_spec.return_value = {}
    eng._persist_state = MagicMock()
    return eng


def _cli():
    from sustena.api.main import app
    return AsyncClient(transport=ASGITransport(app=app), base_url="http://test")


def _ov(eng):
    from sustena.api.main import app
    app.dependency_overrides[_sm.get_engine] = lambda: eng


def _cl():
    from sustena.api.main import app
    app.dependency_overrides.pop(_sm.get_engine, None)


@pytest.mark.asyncio
async def test_create_sustain_201():
    eng = _mk_engine()
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.post("/api/v1/sustains/", json={"template_id": "homestead", "parameters": {}}, headers=_hdrs())
        assert r.status_code == 201
        b = r.json()
        assert b["status"] == "ok"
        assert b["data"]["sustain_id"] == "sustain-123"
        assert b["data"]["template_id"] == "homestead"
        assert "created_at" in b["data"]
    finally:
        _cl()


@pytest.mark.asyncio
async def test_create_sustain_no_auth_401():
    async with _cli() as c:
        r = await c.post("/api/v1/sustains/", json={"template_id": "homestead", "parameters": {}})
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_get_sustain_200():
    eng = _mk_engine(state={"finances": {"liquid": {"balance": 9999}}, "council_proposals": []})
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.get("/api/v1/sustains/sustain-123", headers=_hdrs())
        assert r.status_code == 200
        assert r.json()["data"]["finances"]["liquid"]["balance"] == 9999
    finally:
        _cl()


@pytest.mark.asyncio
async def test_get_sustain_404():
    from sustena.api.main import app
    eng = MagicMock()
    eng._db = MagicMock()
    eng._db.execute.return_value.fetchone.return_value = None
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.get("/api/v1/sustains/no-such-id", headers=_hdrs())
        assert r.status_code == 404
    finally:
        _cl()


@pytest.mark.asyncio
async def test_execute_operator_200():
    result = OperatorResult.ok({"amount": 5000, "new_balance": 15000})
    eng = _mk_engine(execute_result=result)
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.post(
                "/api/v1/sustains/sustain-123/operators/budget.record_income",
                json={"params": {"amount": 5000, "description": "salary"}},
                headers=_hdrs(),
            )
        assert r.status_code == 200
        assert r.json()["data"]["status"] == "ok"
    finally:
        _cl()


@pytest.mark.asyncio
async def test_execute_operator_constraint_violation_422():
    result = OperatorResult.fail("Insufficient funds", "finances.liquid.balance >= params.amount")
    eng = _mk_engine(execute_result=result)
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.post(
                "/api/v1/sustains/sustain-123/operators/budget.spend",
                json={"params": {"amount": 999999}},
                headers=_hdrs(),
            )
        assert r.status_code == 422
        assert r.json()["data"]["status"] == "failed"
        assert "Insufficient" in r.json()["data"]["reason"]
    finally:
        _cl()


@pytest.mark.asyncio
async def test_get_events_200():
    eng = _mk_engine()
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.get("/api/v1/sustains/sustain-123/events?limit=10&offset=0", headers=_hdrs())
        assert r.status_code == 200
        assert "events" in r.json()["data"]
    finally:
        _cl()


@pytest.mark.asyncio
async def test_vote_on_proposal_yes_200():
    expires = (_dt.now(_tz.utc) + _td(hours=48)).isoformat()
    state = {
        "council_proposals": [{
            "id": "prop-1",
            "sustain_id": "sustain-123",
            "proposed_by": "mentor",
            "operator_name": "budget.reallocate",
            "input_json": "{}",
            "simulation_results_json": "{}",
            "status": "IN_VOTING",
            "created_at": _dt.now(_tz.utc).isoformat(),
            "resolved_at": None,
            "expires_at": expires,
        }],
        "council_votes": [],
    }
    eng = _mk_engine(state=state)
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.post(
                "/api/v1/sustains/sustain-123/proposals/prop-1/vote",
                json={"vote": "YES"},
                headers=_hdrs(),
            )
        assert r.status_code == 200
        b = r.json()
        assert b["status"] == "ok"
        assert b["data"]["status"] == "PASSED"
        assert b["data"]["proposal_id"] == "prop-1"
    finally:
        _cl()


@pytest.mark.asyncio
async def test_export_sustain_200():
    eng = _mk_engine()
    _ov(eng)
    try:
        async with _cli() as c:
            r = await c.get("/api/v1/sustains/sustain-123/export", headers=_hdrs())
        assert r.status_code == 200
        assert "content-disposition" in r.headers
        assert 'filename="sustain_sustain-123.json"' in r.headers["content-disposition"]
        payload = r.json()
        assert "sustain_id" in payload
        assert "state" in payload
        assert "spec" in payload
    finally:
        _cl()
