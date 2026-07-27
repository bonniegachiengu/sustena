"""
tests/test_ingest_routes.py

Tests for /api/v1/ingest/* (sustena/api/routes/ingest.py) — auth guard,
ownership boundary, and the capture/messages/resolve/sources surface.

Follows the same module-scoped TestClient + per-test user registration
pattern as test_devui_routes.py: a single event loop for the whole module
so get_shared_engine() (sync sqlite3) and the SQLAlchemy async engine both
see the same lifespan-created in-memory DB throughout.
"""

import uuid

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app

RECEIVED = (
    "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU "
    "254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00"
)

BUYGOODS = (
    "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 "
    "at 4:30 PM. New M-PESA balance is Ksh12,050.00"
)


@pytest.fixture(scope="module")
def client():
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


@pytest.fixture(autouse=True)
def _reset_engine_function():
    """Overrides conftest's per-function engine reset — see test_devui_routes.py's
    identical note: this module uses one long-lived TestClient/event loop, so a
    mid-module engine reset would wipe the users/sustains tables underneath it."""
    yield


@pytest.fixture
def user(client):
    """Register a fresh user and hand back (auth_header, user_id)."""
    email = f"ingest-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


@pytest.fixture
def sustain(client, user):
    """A real homestead sustain owned by `user`."""
    headers, user_id = user
    r = client.post(
        "/devui/sustains",
        json={"template_id": "homestead", "user_id": user_id, "parameters": {}},
        headers=headers,
    )
    assert r.status_code == 200, r.text
    return r.json()["data"]["sustain_id"]


# ── Auth guard ────────────────────────────────────────────────────────────────

class TestAuthGuard:
    def test_capture_no_auth_returns_401(self, client):
        r = client.post("/api/v1/ingest/capture", json={"source_id": "d1", "sustain_id": "x", "raw_payload": "hi"})
        assert r.status_code == 401

    def test_messages_no_auth_returns_401(self, client):
        r = client.get("/api/v1/ingest/messages")
        assert r.status_code == 401

    def test_sources_no_auth_returns_401(self, client):
        r = client.get("/api/v1/ingest/sources")
        assert r.status_code == 401

    def test_register_source_no_auth_returns_401(self, client):
        r = client.post("/api/v1/ingest/sources", json={"source_id": "d1", "sustain_id": "x"})
        assert r.status_code == 401


# ── Ownership boundary ────────────────────────────────────────────────────────

class TestOwnershipBoundary:
    def test_capture_into_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": "does-not-exist", "raw_payload": RECEIVED},
            headers=headers,
        )
        assert r.status_code == 404

    def test_capture_into_another_users_sustain_returns_404(self, client, user, sustain):
        # A second, different user must not be able to capture into the
        # first user's sustain even if they happen to know its id.
        email = f"ingest-tests-{uuid.uuid4().hex[:12]}@example.com"
        r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
        other_headers = {"Authorization": f"Bearer {r.json()['data']['token']}"}

        r = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=other_headers,
        )
        assert r.status_code == 404

    def test_register_source_for_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.post(
            "/api/v1/ingest/sources",
            json={"source_id": "d1", "sustain_id": "does-not-exist"},
            headers=headers,
        )
        assert r.status_code == 404


# ── POST /capture ──────────────────────────────────────────────────────────────

class TestCapture:
    def test_mapped_capture_returns_applied(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=headers,
        )
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["status"] == "applied"
        assert data["is_duplicate"] is False

    def test_replaying_the_same_capture_is_flagged_duplicate(self, client, user, sustain):
        headers, _ = user
        body = {"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED}
        first = client.post("/api/v1/ingest/capture", json=body, headers=headers).json()["data"]
        replay = client.post("/api/v1/ingest/capture", json=body, headers=headers).json()["data"]
        assert first["is_duplicate"] is False
        assert replay["is_duplicate"] is True
        assert replay["message_id"] == first["message_id"]

    def test_unparsed_capture_returns_needs_attention(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": "not a real message"},
            headers=headers,
        )
        assert r.json()["data"]["status"] == "needs_attention"


# ── GET /messages, GET /messages/{id}, POST /messages/{id}/resolve ──────────

class TestMessages:
    def test_list_messages_for_sustain(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=headers,
        )
        r = client.get("/api/v1/ingest/messages", params={"sustain_id": sustain}, headers=headers)
        assert r.status_code == 200
        assert len(r.json()["data"]["messages"]) == 1

    def test_get_single_message(self, client, user, sustain):
        headers, _ = user
        captured = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=headers,
        ).json()["data"]
        r = client.get(f"/api/v1/ingest/messages/{captured['message_id']}", headers=headers)
        assert r.status_code == 200
        assert r.json()["data"]["message_id"] == captured["message_id"]

    def test_get_message_owned_by_another_user_returns_404(self, client, user, sustain):
        headers, _ = user
        captured = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=headers,
        ).json()["data"]

        email = f"ingest-tests-{uuid.uuid4().hex[:12]}@example.com"
        r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
        other_headers = {"Authorization": f"Bearer {r.json()['data']['token']}"}

        r = client.get(f"/api/v1/ingest/messages/{captured['message_id']}", headers=other_headers)
        assert r.status_code == 404

    def test_get_unknown_message_returns_404(self, client, user):
        headers, _ = user
        r = client.get("/api/v1/ingest/messages/does-not-exist", headers=headers)
        assert r.status_code == 404

    def test_resolve_needs_attention_message(self, client, user, sustain):
        headers, _ = user
        captured = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        ).json()["data"]
        assert captured["status"] == "needs_attention"

        r = client.post(f"/api/v1/ingest/messages/{captured['message_id']}/resolve", json={}, headers=headers)
        assert r.status_code == 200
        assert r.json()["data"]["resolved_at"] is not None

    def test_resolve_already_resolved_message_returns_409(self, client, user, sustain):
        headers, _ = user
        captured = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        ).json()["data"]
        client.post(f"/api/v1/ingest/messages/{captured['message_id']}/resolve", json={}, headers=headers)
        r = client.post(f"/api/v1/ingest/messages/{captured['message_id']}/resolve", json={}, headers=headers)
        assert r.status_code == 409

    def test_resolve_applied_message_returns_409(self, client, user, sustain):
        # An applied (not needs_attention) message was never awaiting resolution.
        headers, _ = user
        captured = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=headers,
        ).json()["data"]
        r = client.post(f"/api/v1/ingest/messages/{captured['message_id']}/resolve", json={}, headers=headers)
        assert r.status_code == 409


# ── GET/POST /sources ───────────────────────────────────────────────────────

class TestSources:
    def test_register_source_then_list_it(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/api/v1/ingest/sources",
            json={"source_id": "d1", "sustain_id": sustain, "label": "Bonnie's phone", "expected_interval_minutes": 30},
            headers=headers,
        )
        assert r.status_code == 200
        assert r.json()["data"]["label"] == "Bonnie's phone"

        r = client.get("/api/v1/ingest/sources", params={"sustain_id": sustain}, headers=headers)
        sources = r.json()["data"]["sources"]
        assert len(sources) == 1
        assert sources[0]["source_id"] == "d1"

    def test_registered_source_with_no_capture_yet_is_stale(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/api/v1/ingest/sources",
            json={"source_id": "d1", "sustain_id": sustain, "expected_interval_minutes": 15},
            headers=headers,
        )
        r = client.get("/api/v1/ingest/sources", params={"sustain_id": sustain}, headers=headers)
        assert r.json()["data"]["sources"][0]["is_stale"] is True

    def test_source_with_no_configured_interval_is_never_stale(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=headers,
        )
        r = client.get("/api/v1/ingest/sources", params={"sustain_id": sustain}, headers=headers)
        source = next(s for s in r.json()["data"]["sources"] if s["source_id"] == "d1")
        assert source["expected_interval_minutes"] is None
        assert source["is_stale"] is False


# ── /devui/state carries ingest_attention ────────────────────────────────────

class TestDevuiStateIngestAttention:
    def test_state_includes_ingest_attention_key(self, client, user, sustain):
        headers, _ = user
        r = client.get("/devui/state", params={"sustain_id": sustain}, headers=headers)
        data = r.json()["data"]
        assert "ingest_attention" in data
        assert "messages" in data["ingest_attention"]
        assert "stale_sources" in data["ingest_attention"]

    def test_needs_attention_capture_surfaces_in_devui_state(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        r = client.get("/devui/state", params={"sustain_id": sustain}, headers=headers)
        messages = r.json()["data"]["ingest_attention"]["messages"]
        assert len(messages) == 1
        assert messages[0]["status"] == "needs_attention"

    def test_applied_capture_does_not_surface_as_needs_attention(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": RECEIVED},
            headers=headers,
        )
        r = client.get("/devui/state", params={"sustain_id": sustain}, headers=headers)
        assert r.json()["data"]["ingest_attention"]["messages"] == []

    def test_stale_source_surfaces_in_devui_state(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/api/v1/ingest/sources",
            json={"source_id": "d1", "sustain_id": sustain, "expected_interval_minutes": 15},
            headers=headers,
        )
        r = client.get("/devui/state", params={"sustain_id": sustain}, headers=headers)
        stale = r.json()["data"]["ingest_attention"]["stale_sources"]
        assert len(stale) == 1
        assert stale[0]["source_id"] == "d1"
