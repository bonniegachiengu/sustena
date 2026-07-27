"""
tests/test_orchie_compose_route.py

Tests for GET /orchie/compose (sustena/api/routes/orchie.py) — auth guard,
ownership boundary, and the happy path over HTTP. Same module-scoped
TestClient pattern as test_ingest_routes.py.

Run with:
    python -m pytest tests/test_orchie_compose_route.py -v
"""

import uuid

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app


@pytest.fixture(scope="module")
def client():
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


@pytest.fixture(autouse=True)
def _reset_engine_function():
    """Same override as test_ingest_routes.py: this module uses one
    long-lived TestClient/event loop, so a mid-module engine reset would
    wipe the users/sustains tables underneath it."""
    yield


@pytest.fixture
def user(client):
    email = f"orchie-compose-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


@pytest.fixture
def sustain(client, user):
    headers, user_id = user
    r = client.post(
        "/devui/sustains",
        json={"template_id": "homestead", "user_id": user_id, "parameters": {}},
        headers=headers,
    )
    assert r.status_code == 200, r.text
    return r.json()["data"]["sustain_id"]


class TestAuthGuard:
    def test_compose_no_auth_returns_401(self, client, sustain):
        r = client.get(f"/orchie/compose?sustain_id={sustain}")
        assert r.status_code == 401


class TestOwnershipBoundary:
    def test_compose_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.get("/orchie/compose?sustain_id=does-not-exist", headers=headers)
        assert r.status_code == 404

    def test_compose_another_users_sustain_returns_404(self, client, user, sustain):
        # `sustain` belongs to the `user` fixture's account; register a
        # second, unrelated account and confirm it cannot compose a view
        # for someone else's household.
        email2 = f"orchie-compose-other-{uuid.uuid4().hex[:12]}@example.com"
        r2 = client.post("/api/v1/users/register", json={"email": email2, "password": "test-password-123"})
        headers2 = {"Authorization": f"Bearer {r2.json()['data']['token']}"}
        r = client.get(f"/orchie/compose?sustain_id={sustain}", headers=headers2)
        assert r.status_code == 404


class TestHappyPath:
    def test_compose_returns_ranked_budget_limited_view(self, client, user, sustain):
        headers, _ = user
        r = client.get(f"/orchie/compose?sustain_id={sustain}&device=phone&budget=4", headers=headers)
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["sustain_id"] == sustain
        assert data["device"] == "phone"
        assert data["budget"] == 4
        assert "selected" in data and "excluded" in data
        # household_rollup_summary is Unit-bound and always eligible, so on
        # a freshly instantiated homestead with a real budget it's expected.
        assert len(data["selected"]) >= 1

    def test_compose_reflects_a_real_recent_spend(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 1000, "source": "t", "frequency": "once"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 1000, "period": "monthly"}},
            headers=headers,
        )
        r_spend = client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 1000, "description": "x", "category": "test"}},
            headers=headers,
        )
        assert r_spend.status_code == 200, r_spend.text

        r = client.get(f"/orchie/compose?sustain_id={sustain}&budget=1", headers=headers)
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["selected"][0]["id"] == "pocket_spent_watch"
        assert data["selected"][0]["urgency"] == pytest.approx(1.0)

    def test_compose_is_read_only_over_http(self, client, user, sustain):
        headers, _ = user
        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        client.get(f"/orchie/compose?sustain_id={sustain}", headers=headers)
        client.get(f"/orchie/compose?sustain_id={sustain}&query=pocket&budget=2", headers=headers)
        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_before == state_after
