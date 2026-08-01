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
from sqlalchemy.ext.asyncio import create_async_engine

import sustena.db.schema as _schema
from sustena.api.main import app
from sustena.db.schema import _migrate_users_auth, metadata

BASE = "http://test"


# ── Fixtures ──────────────────────────────────────────────────────────────────

@pytest.fixture
async def client():
    """
    Fresh in-memory DB for each test regardless of DATABASE_URL in the environment.
    CI sets DATABASE_URL to a file-based test.db; conftest setdefault is a no-op there,
    so we manually set an :memory: engine here to guarantee isolation.
    """
    mem_engine = create_async_engine(
        "sqlite+aiosqlite:///:memory:",
        connect_args={"check_same_thread": False},
    )
    _schema._engine = mem_engine

    async with mem_engine.begin() as conn:
        await conn.run_sync(metadata.create_all)
        await conn.run_sync(_migrate_users_auth)

    async with AsyncClient(transport=ASGITransport(app=app), base_url=BASE) as c:
        yield c

    _schema._engine = None
    await mem_engine.dispose()


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


# ── Email case-insensitivity (found live 1 Aug 2026: same account logged in
# fine on Android via autofill's exact-cased memory, failed on the Studio
# desktop app typed fresh in lowercase -- a real account, correct password,
# rejected only on letter case) ─────────────────────────────────────────────

class TestEmailCaseInsensitivity:
    async def test_register_normalizes_email_to_lowercase(self, client):
        r = await _register(client, email="Bonnie@Sustena.IO")
        assert r.json()["data"]["email"] == "bonnie@sustena.io"

    async def test_login_with_lowercase_after_mixedcase_register(self, client):
        await _register(client, email="Bonnie@Sustena.IO", password="password123")
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "bonnie@sustena.io", "password": "password123"},
        )
        assert r.status_code == 200
        assert "token" in r.json()["data"]

    async def test_login_with_original_mixedcase_still_works(self, client):
        # Case shouldn't matter in EITHER direction -- registered mixed,
        # login mixed (identical to registration casing) must still succeed.
        await _register(client, email="Bonnie@Sustena.IO", password="password123")
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "Bonnie@Sustena.IO", "password": "password123"},
        )
        assert r.status_code == 200

    async def test_login_with_different_mixedcase_works(self, client):
        # A third, different casing than either registration or the two
        # cases above -- proves this is genuine case-insensitivity, not a
        # coincidental match against one specific stored/typed form.
        await _register(client, email="Bonnie@Sustena.IO", password="password123")
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "BONNIE@sustena.io", "password": "password123"},
        )
        assert r.status_code == 200

    async def test_login_response_echoes_stored_casing_not_input_casing(self, client):
        await _register(client, email="Bonnie@Sustena.IO", password="password123")
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "bonnie@sustena.io", "password": "password123"},
        )
        # Stored (post-normalization) form, not whatever casing this
        # particular login attempt happened to type.
        assert r.json()["data"]["email"] == "bonnie@sustena.io"

    async def test_wrong_password_with_mismatched_case_still_401s(self, client):
        # Case-insensitivity must not accidentally weaken the password
        # check itself -- only the email lookup changed.
        await _register(client, email="Bonnie@Sustena.IO", password="password123")
        r = await client.post(
            "/api/v1/users/login",
            json={"email": "bonnie@sustena.io", "password": "wrongpassword"},
        )
        assert r.status_code == 401

    async def test_duplicate_registration_blocked_across_case(self, client):
        # Registering "BOB@x.com" when "bob@x.com" already exists must be
        # blocked as a duplicate, not create a second, case-variant row --
        # if it did, a real login could nondeterministically resolve to
        # either one depending on which the query happens to return first.
        await _register(client, email="bob@x.com", password="password123")
        r = await _register(client, email="BOB@X.com", password="password456")
        assert r.status_code == 409


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
        for field in [
            "sustain_count", "pawa_balance", "operator_executions",
            "proposals_passed", "proposals_in_voting",
            "orders_placed", "lore_published", "packages_published",
        ]:
            assert field in data, f"missing field: {field}"

    async def test_new_user_governance_stats_are_zero(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/stats", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert data["proposals_passed"] == 0
        assert data["proposals_in_voting"] == 0
        assert data["orders_placed"] == 0
        assert data["lore_published"] == 0
        assert data["packages_published"] == 0

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


# ── GET /me/council ───────────────────────────────────────────────────────────

class TestMeCouncil:
    async def test_returns_200(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/council", headers={"Authorization": f"Bearer {token}"})
        assert r.status_code == 200

    async def test_has_proposals_list(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/council", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert "proposals" in data
        assert isinstance(data["proposals"], list)
        assert "count" in data

    async def test_new_user_has_no_proposals(self, client):
        token = await _token(client)
        r = await client.get("/api/v1/users/me/council", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert data["count"] == 0
        assert data["proposals"] == []

    async def test_no_auth_returns_401(self, client):
        r = await client.get("/api/v1/users/me/council")
        assert r.status_code == 401


# ── POST /logout ──────────────────────────────────────────────────────────────
# The whole point of token_version: prove logout actually revokes the token
# server-side, not just that the client forgets it.

class TestLogout:
    async def test_no_auth_returns_401(self, client):
        r = await client.post("/api/v1/users/logout")
        assert r.status_code == 401

    async def test_returns_200_with_valid_token(self, client):
        token = await _token(client)
        r = await client.post("/api/v1/users/logout", headers={"Authorization": f"Bearer {token}"})
        assert r.status_code == 200
        assert r.json()["data"]["logged_out"] is True

    async def test_old_token_rejected_after_logout(self, client):
        """The exact behaviour this feature exists for: a token that was
        valid a moment ago must stop working the instant logout runs,
        without needing to wait for its 30-day expiry."""
        token = await _token(client)

        # Token works before logout.
        r1 = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {token}"})
        assert r1.status_code == 200

        await client.post("/api/v1/users/logout", headers={"Authorization": f"Bearer {token}"})

        # Same token, now rejected — not because it's malformed or expired,
        # but because token_version no longer matches.
        r2 = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {token}"})
        assert r2.status_code == 401

    async def test_logging_in_again_after_logout_issues_a_working_token(self, client):
        """Logout must not permanently lock the user out — a fresh login
        issues a token stamped with the new version and works normally."""
        email, password = "logout-relogin@test.com", "password123"
        old_token = await _token(client, email=email, password=password)
        await client.post("/api/v1/users/logout", headers={"Authorization": f"Bearer {old_token}"})

        login_r = await client.post("/api/v1/users/login", json={"email": email, "password": password})
        assert login_r.status_code == 200
        new_token = login_r.json()["data"]["token"]
        assert new_token != old_token

        r = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {new_token}"})
        assert r.status_code == 200

    async def test_logout_only_affects_the_logged_out_user(self, client):
        """Two different users' sessions are independent — logging one out
        must not touch the other's."""
        token_a = await _token(client, email="user-a@test.com")
        token_b = await _token(client, email="user-b@test.com")

        await client.post("/api/v1/users/logout", headers={"Authorization": f"Bearer {token_a}"})

        r_a = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {token_a}"})
        assert r_a.status_code == 401

        r_b = await client.get("/api/v1/users/me", headers={"Authorization": f"Bearer {token_b}"})
        assert r_b.status_code == 200
