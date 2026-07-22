"""
tests/test_dev_routes.py

Tests for sustena/api/routes/dev.py auth guard. This router is only mounted
when ENVIRONMENT=development, but that mount gate is not per-route auth --
every route also requires a real user session (get_current_user), same as
devui.py and seed.py. This file checks that guard actually applies.
"""

import uuid

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app

AUTH_HEADER = {}  # populated by _seed_auth_header before each test


@pytest.fixture(scope="module")
def client():
    """Synchronous TestClient — module-scoped so the FastAPI lifespan
    (init_db()) runs once and the SQLAlchemy engine stays alive for every
    test in this file, on one event loop."""
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


@pytest.fixture(autouse=True)
def _reset_engine_function():
    """Overrides conftest.py's per-function engine reset for this file —
    see the identical, more detailed note in test_devui_routes.py. Short
    version: that reset assumes a fresh AsyncClient (and event loop) per
    test; this file's `client` is one module-scoped TestClient instead, so
    the reset only wipes tables the routes then can't find."""
    yield


@pytest.fixture(autouse=True)
def _seed_auth_header(client):
    """Registers a fresh real user before every test and points
    AUTH_HEADER at their JWT."""
    r = client.post(
        "/api/v1/users/register",
        json={"email": f"dev-route-tests-{uuid.uuid4().hex[:12]}@example.com", "password": "test-password-123"},
    )
    assert r.status_code == 200, f"test user registration failed: {r.text}"
    token = r.json()["data"]["token"]
    AUTH_HEADER["Authorization"] = f"Bearer {token}"
    yield


class TestDevRoutesRequireAuth:
    def test_sessions_no_auth_returns_401(self, client):
        r = client.get("/dev/sessions")
        assert r.status_code == 401

    def test_sessions_bad_token_returns_401(self, client):
        r = client.get("/dev/sessions", headers={"Authorization": "Bearer wrong-token"})
        assert r.status_code == 401

    def test_sessions_with_auth_returns_200(self, client):
        r = client.get("/dev/sessions", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_outbox_no_auth_returns_401(self, client):
        r = client.get("/dev/outbox")
        assert r.status_code == 401

    def test_outbox_with_auth_returns_200(self, client):
        r = client.get("/dev/outbox", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_support_queue_no_auth_returns_401(self, client):
        r = client.get("/dev/support-queue")
        assert r.status_code == 401

    def test_support_queue_with_auth_returns_200(self, client):
        r = client.get("/dev/support-queue", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_simulate_no_auth_returns_401(self, client):
        r = client.post("/dev/simulate", json={"phone": "+254712345678", "message": "hello"})
        assert r.status_code == 401

    def test_reset_session_no_auth_returns_401(self, client):
        r = client.delete("/dev/sessions/254712345678")
        assert r.status_code == 401

    def test_mark_addressed_no_auth_returns_401(self, client):
        r = client.patch("/dev/support-queue/some-id/addressed")
        assert r.status_code == 401
