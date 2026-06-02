"""
tests/test_journal.py

Tests for /api/v1/journal/* — private user journal (CRUD).
Sprint 8.7

Uses the same in-memory DB isolation pattern as test_users.py.
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


async def _register(client, email="user@test.com", password="password123"):
    r = await client.post("/api/v1/users/register", json={"email": email, "password": password})
    return r.json()["data"]["token"]


async def _create_entry(client, token, kind="NOTE", body="today's note"):
    return await client.post(
        "/api/v1/journal/entries",
        json={"kind": kind, "body": body},
        headers={"Authorization": f"Bearer {token}"},
    )


# ── GET /entries ──────────────────────────────────────────────────────────────

class TestListJournalEntries:
    async def test_no_auth_returns_401(self, client):
        r = await client.get("/api/v1/journal/entries")
        assert r.status_code == 401

    async def test_returns_200(self, client):
        token = await _register(client)
        r = await client.get("/api/v1/journal/entries", headers={"Authorization": f"Bearer {token}"})
        assert r.status_code == 200

    async def test_status_ok(self, client):
        token = await _register(client)
        r = await client.get("/api/v1/journal/entries", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["status"] == "ok"

    async def test_empty_for_new_user(self, client):
        token = await _register(client)
        r = await client.get("/api/v1/journal/entries", headers={"Authorization": f"Bearer {token}"})
        data = r.json()["data"]
        assert data["count"] == 0
        assert data["entries"] == []

    async def test_entry_appears_after_creation(self, client):
        token = await _register(client)
        await _create_entry(client, token)
        r = await client.get("/api/v1/journal/entries", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["data"]["count"] == 1

    async def test_user_only_sees_own_entries(self, client):
        token_a = await _register(client, email="a@test.com")
        token_b = await _register(client, email="b@test.com")
        await _create_entry(client, token_a)
        await _create_entry(client, token_a)
        r = await client.get("/api/v1/journal/entries", headers={"Authorization": f"Bearer {token_b}"})
        assert r.json()["data"]["count"] == 0


# ── POST /entries ─────────────────────────────────────────────────────────────

class TestCreateJournalEntry:
    async def test_returns_200(self, client):
        token = await _register(client)
        r = await _create_entry(client, token)
        assert r.status_code == 200

    async def test_status_ok(self, client):
        token = await _register(client)
        r = await _create_entry(client, token)
        assert r.json()["status"] == "ok"

    async def test_returns_id(self, client):
        token = await _register(client)
        r = await _create_entry(client, token)
        assert "id" in r.json()["data"]

    async def test_kind_uppercased(self, client):
        token = await _register(client)
        r = await _create_entry(client, token, kind="decision")
        assert r.json()["data"]["kind"] == "DECISION"

    async def test_body_preserved(self, client):
        token = await _register(client)
        r = await _create_entry(client, token, body="my exact note")
        assert r.json()["data"]["body"] == "my exact note"

    async def test_no_auth_returns_401(self, client):
        r = await client.post("/api/v1/journal/entries", json={"kind": "NOTE", "body": "x"})
        assert r.status_code == 401

    async def test_invalid_kind_returns_422(self, client):
        token = await _register(client)
        r = await client.post(
            "/api/v1/journal/entries",
            json={"kind": "DIARY", "body": "x"},
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.status_code == 422

    async def test_empty_body_returns_422(self, client):
        token = await _register(client)
        r = await client.post(
            "/api/v1/journal/entries",
            json={"kind": "NOTE", "body": ""},
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.status_code == 422

    async def test_all_valid_kinds_accepted(self, client):
        token = await _register(client)
        for kind in ("DECISION", "NOTE", "REFLECTION", "UPDATE"):
            r = await _create_entry(client, token, kind=kind, body=f"entry for {kind}")
            assert r.status_code == 200

    async def test_sustain_id_stored_when_provided(self, client):
        token = await _register(client)
        r = await client.post(
            "/api/v1/journal/entries",
            json={"kind": "NOTE", "body": "linked", "sustain_id": "sid-123"},
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.json()["data"]["sustain_id"] == "sid-123"

    async def test_created_at_present(self, client):
        token = await _register(client)
        r = await _create_entry(client, token)
        assert r.json()["data"]["created_at"] is not None


# ── PUT /entries/{id} ─────────────────────────────────────────────────────────

class TestUpdateJournalEntry:
    async def test_returns_200(self, client):
        token = await _register(client)
        cr = await _create_entry(client, token)
        entry_id = cr.json()["data"]["id"]
        r = await client.put(
            f"/api/v1/journal/entries/{entry_id}",
            json={"body": "updated body"},
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.status_code == 200

    async def test_body_updated(self, client):
        token = await _register(client)
        cr = await _create_entry(client, token, body="original")
        entry_id = cr.json()["data"]["id"]
        r = await client.put(
            f"/api/v1/journal/entries/{entry_id}",
            json={"body": "revised"},
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.json()["data"]["body"] == "revised"

    async def test_kind_updated(self, client):
        token = await _register(client)
        cr = await _create_entry(client, token, kind="NOTE")
        entry_id = cr.json()["data"]["id"]
        r = await client.put(
            f"/api/v1/journal/entries/{entry_id}",
            json={"kind": "DECISION"},
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.json()["data"]["kind"] == "DECISION"

    async def test_other_user_cannot_update(self, client):
        token_a = await _register(client, email="a@test.com")
        token_b = await _register(client, email="b@test.com")
        cr = await _create_entry(client, token_a)
        entry_id = cr.json()["data"]["id"]
        r = await client.put(
            f"/api/v1/journal/entries/{entry_id}",
            json={"body": "hijacked"},
            headers={"Authorization": f"Bearer {token_b}"},
        )
        assert r.status_code == 403

    async def test_nonexistent_returns_404(self, client):
        token = await _register(client)
        r = await client.put(
            "/api/v1/journal/entries/no-such-id",
            json={"body": "x"},
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.status_code == 404

    async def test_no_auth_returns_401(self, client):
        r = await client.put("/api/v1/journal/entries/any-id", json={"body": "x"})
        assert r.status_code == 401


# ── DELETE /entries/{id} ──────────────────────────────────────────────────────

class TestDeleteJournalEntry:
    async def test_returns_200(self, client):
        token = await _register(client)
        cr = await _create_entry(client, token)
        entry_id = cr.json()["data"]["id"]
        r = await client.delete(
            f"/api/v1/journal/entries/{entry_id}",
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.status_code == 200

    async def test_deleted_true(self, client):
        token = await _register(client)
        cr = await _create_entry(client, token)
        entry_id = cr.json()["data"]["id"]
        r = await client.delete(
            f"/api/v1/journal/entries/{entry_id}",
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.json()["data"]["deleted"] is True

    async def test_entry_gone_after_delete(self, client):
        token = await _register(client)
        cr = await _create_entry(client, token)
        entry_id = cr.json()["data"]["id"]
        await client.delete(
            f"/api/v1/journal/entries/{entry_id}",
            headers={"Authorization": f"Bearer {token}"},
        )
        r = await client.get("/api/v1/journal/entries", headers={"Authorization": f"Bearer {token}"})
        assert r.json()["data"]["count"] == 0

    async def test_other_user_cannot_delete(self, client):
        token_a = await _register(client, email="a@test.com")
        token_b = await _register(client, email="b@test.com")
        cr = await _create_entry(client, token_a)
        entry_id = cr.json()["data"]["id"]
        r = await client.delete(
            f"/api/v1/journal/entries/{entry_id}",
            headers={"Authorization": f"Bearer {token_b}"},
        )
        assert r.status_code == 403

    async def test_nonexistent_returns_404(self, client):
        token = await _register(client)
        r = await client.delete(
            "/api/v1/journal/entries/ghost-id",
            headers={"Authorization": f"Bearer {token}"},
        )
        assert r.status_code == 404

    async def test_no_auth_returns_401(self, client):
        r = await client.delete("/api/v1/journal/entries/any-id")
        assert r.status_code == 401
