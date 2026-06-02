"""
tests/test_users.py

Tests for /api/v1/users/* — register, login, /me, /me/stats, /me/activity.
Sprint 8.6

Uses AsyncClient + explicit init_db() so each test function gets a
fresh in-memory DB with tables (the autouse _reset_engine_function in
conftest resets the engine before each function; init_db() recreates it).
"""

import pytest
from httpx import ASGITransport, AsyncClient

from sustena.api.main import app
from sustena.db.schema import init_db

BASE = "http://test"


# ── Fixtures ──────────────────────────────────────────────────────────────────

@pytest.fixture
async def client():
    """Fresh in-memory DB + async HTTP client for each test function."""
    await init_db()
    async with AsyncClient(transport=ASGITransport(app=app), base_url=BASE) as c:
        yield c


async def _register(client, email="user@test.com", password="password123", display_name=None):
    body = {"email": email, "password": password}
    if display_name:
        body["display_name"] = display_name
    r = await client.post("/api/v1/users/register", json=body)
    return r


async def _token(client, email="user@test.com", password="password123"):
    r = await _register(client, email=email, password=password)
    return r.json()["data"]["token"]


# ── POST /register ────────────────────────────────────────────────────────────

class TestRegister:
    async def test_returns_200(self, client):
        r = await _register(client)
        assert r.status_code == 200

    async def test_response_status_ok(self, client):
        r = await _register(client)
        assert r.json()["status"] == "ok"

    async def test_returns_token(self, client):
        r = await _register(client)
        data = r.json()["data"]
        assert "token" in data
        assert len(data["token"]) > 20

    async def test_returns_user_id(self, client):
        r = await _register(client)
        assert "user_id" in r.json()["data"]

    async def test_returns_email(self, client):
        r = await _register(client, email="hello@world.com")
        assert r.json()["data"]["email"] == "hello@world.com"

    async def test_display_name_defaults_to_email_prefix(self, client):
        r = await _register(client, email="bonnie@sustena.io")
        assert r.json()["data"]["display_name"] == "bonnie"

    async def test_custom_display_name_preserved(self, client):
        r = await _register(client, display_name="Bonnie G")
        assert r.json()["data"]["display_name"] == "Bonnie G"

    async def test_duplicate_email_returns_409(self, client):
        await _register(client, email="dup@test.com")
        r = await _register(client, email="dup@test.com")
        assert r.status_code == 409

    async def test_short_password_returns_422(self, client):
        r = await client.post(
            "/api/v1/users/register",
            json={"email": "x@y.com", "password": "short"},
        )
        assert r.status_code == 422

    async def test_missing_email_returns_422(self, client):
        r = await client.post(
            "/api/v1/users/register",
            json={"password": "password123"},
        )
        assert r.status_code == 422


# ── POST /login ───────────────────────────────────────────────────────────────

class TestLogin:
    async def test_login_returns_200(self, client):
        await _register(client)
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "user@test.com", "password": "password123"},
        )
        assert r.status_code == 200

    async def test_login_returns_token(self, client):
        await _register(client)
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "user@test.com", "password": "password123"},
        )
        assert "token" in r.json()["data"]

    async def test_login_echoes_email(self, client):
        await _register(client)
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "user@test.com", "password": "password123"},
        )
        assert r.json()["data"]["email"] == "user@test.com"

    async def test_wrong_password_returns_401(self, client):
        await _register(client)
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "user@test.com", "password": "wrongpassword"},
        )
        assert r.status_code == 401

    async def test_unknown_email_returns_401(self, client):
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "nobody@nowhere.com", "password": "password123"},
        )
        assert r.status_code == 401

    async def test_response_status_ok(self, client):
        await _register(client)
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "user@test.com", "password": "password123"},
        )
        assert r.json()["status"] == "ok"


# ── GET /me ───────────────────────────────────────────────────────────────────

class TestMe:
    async def test_returns_200(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {token}"})
        assert r.status_code == 200

    async def test_returns_profile(self, client):
        token = await _token(client, email="me@sustena.io")
        r = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert data["email"] == "me@sustena.io"
        assert "user_id" in data
        assert "pawa_balance" in data
        assert "created_at" in data

    async def test_no_auth_returns_401(self, client):
        r = await client.get("/api/v1/users/me")
        assert r.status_code == 401

    async def test_bad_token_returns_401(self, client):
        r = await client.get(
            "/api/v1/users/me",
            headers={"Authorization": "Bearer this.is.fake"},
        )
        assert r.status_code == 401

    async def test_pawa_balance_is_100_on_registration(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["data"]["pawa_balance"] == 100


# ── GET /me/stats ─────────────────────────────────────────────────────────────

class TestMeStats:
    async def test_returns_200(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/stats", headers={"Authorization": f"Bearer {token}"})
        assert r.status_code == 200

    async def test_has_required_fields(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/stats", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert "sustain_count" in data
        assert "pawa_balance" in data
        assert "operator_executions" in data

    async def test_pawa_balance_is_100(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/stats", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["data"]["pawa_balance"] == 100

    async def test_new_user_has_zero_sustains(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/stats", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["data"]["sustain_count"] == 0

    async def test_no_auth_returns_401(self, client):
        r = await client.get("/api/v1/users/me/stats")
        assert r.status_code == 401

    async def test_response_status_ok(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/stats", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["status"] == "ok"


# ── GET /me/activity ──────────────────────────────────────────────────────────

class TestMeActivity:
    async def test_returns_200(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/activity", headers={"Authorization": f"Bearer {token}"})
        assert r.status_code == 200

    async def test_has_activity_list(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/activity", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert "activity" in data
        assert isinstance(data["activity"], list)

    async def test_has_count_field(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/activity", headers={"Authorization": f"Bearer {token}"})
        assert "count" in r.json()["data"]

    async def test_new_user_has_empty_activity(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/activity", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert data["count"] == 0
        assert data["activity"] == []

    async def test_no_auth_returns_401(self, client):
        r = await client.get("/api/v1/users/me/activity")
        assert r.status_code == 401

    async def test_response_status_ok(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/activity", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["status"] == "ok"
