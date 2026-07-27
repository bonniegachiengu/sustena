"""
tests/test_egress_routes.py

HTTP-layer tests for the Slice 11 devui.py routes:
  POST /devui/sustain/{id}/egress/prepare-summary
  GET  /devui/sustain/{id}/egress
  POST /devui/sustain/{id}/egress/{outbox_id}/confirm
  POST /devui/sustain/{id}/egress/{outbox_id}/cancel
  GET  /devui/state — carries an "egress" key

Follows the same module-scoped TestClient + per-test user registration +
get_shared_engine() pattern as test_suggestions_routes.py / test_simulator_
routes.py, since these routes run against the real shared engine. The
_EXPORTS_DIR is monkeypatched to a tmp_path for the whole module so real
sends during these tests never touch apps/api/exports/.
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
    yield  # overrides conftest's per-function engine reset; see test_composition_routes.py


@pytest.fixture(autouse=True)
def _redirect_exports_dir(tmp_path, monkeypatch):
    monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
    yield


@pytest.fixture
def user(client):
    email = f"egress-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


def _instantiate(client, headers, template_id, user_id, **params):
    body = {"template_id": template_id, "user_id": user_id, "parameters": params}
    r = client.post("/devui/sustains", json=body, headers=headers)
    return r.json()["data"]["sustain_id"]


@pytest.fixture
def homestead(client, user):
    headers, user_id = user
    sid = _instantiate(client, headers, "homestead", user_id)
    return headers, user_id, sid


class TestPrepareRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustain/x/egress/prepare-summary", json={})
        assert r.status_code == 401

    def test_prepare_queues_without_sending(self, client, homestead):
        headers, _, sid = homestead
        r = client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "week1"}, headers=headers)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["result"]["status"] == "ok"
        assert data["outbox"]["status"] == "prepared"

    def test_prepare_idempotent(self, client, homestead):
        headers, _, sid = homestead
        r1 = client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "week1"}, headers=headers)
        r2 = client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "week1"}, headers=headers)
        assert r1.json()["data"]["outbox"]["id"] == r2.json()["data"]["outbox"]["id"]
        all_rows = client.get(f"/devui/sustain/{sid}/egress", headers=headers).json()["data"]["egress"]
        assert len(all_rows) == 1


class TestListEgressRoute:
    def test_no_auth_returns_401(self, client):
        assert client.get("/devui/sustain/x/egress").status_code == 401

    def test_status_filter(self, client, homestead):
        headers, _, sid = homestead
        client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "x"}, headers=headers)
        prepared = client.get(f"/devui/sustain/{sid}/egress?status=prepared", headers=headers).json()["data"]["egress"]
        assert len(prepared) == 1
        sent = client.get(f"/devui/sustain/{sid}/egress?status=sent", headers=headers).json()["data"]["egress"]
        assert sent == []


class TestDevuiStateCarriesEgress:
    def test_state_egress_key_empty_before_prepare(self, client, user):
        headers, user_id = user
        sid = _instantiate(client, headers, "homestead", user_id)
        r = client.get(f"/devui/state?sustain_id={sid}", headers=headers)
        assert r.json()["data"]["egress"] == []

    def test_state_egress_key_reflects_prepared(self, client, homestead):
        headers, _, sid = homestead
        client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "x"}, headers=headers)
        r = client.get(f"/devui/state?sustain_id={sid}", headers=headers)
        assert len(r.json()["data"]["egress"]) == 1


class TestConfirmRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustain/x/egress/y/confirm", json={})
        assert r.status_code == 401

    def test_confirm_sends_exactly_once(self, client, homestead):
        headers, _, sid = homestead
        outbox = client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "week1"}, headers=headers).json()["data"]["outbox"]

        r1 = client.post(f"/devui/sustain/{sid}/egress/{outbox['id']}/confirm", json={}, headers=headers)
        assert r1.status_code == 200
        data1 = r1.json()["data"]
        assert data1["outbox"]["status"] == "sent"
        assert data1["already_sent"] is False

        # Confirming again is an idempotent no-op -- never a second send.
        r2 = client.post(f"/devui/sustain/{sid}/egress/{outbox['id']}/confirm", json={}, headers=headers)
        assert r2.status_code == 200
        data2 = r2.json()["data"]
        assert data2["already_sent"] is True
        assert data2["outbox"]["attempts"] == data1["outbox"]["attempts"]  # no second attempt

    def test_confirm_unknown_returns_422(self, client, user):
        headers, _ = user
        r = client.post("/devui/sustain/x/egress/does-not-exist/confirm", json={}, headers=headers)
        assert r.status_code == 422

    def test_confirm_honest_failure_surfaced(self, client, homestead):
        headers, _, sid = homestead
        outbox = client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "bad?label"}, headers=headers).json()["data"]["outbox"]

        r = client.post(f"/devui/sustain/{sid}/egress/{outbox['id']}/confirm", json={}, headers=headers)
        assert r.status_code == 200  # a send failure is a normal response body, not an HTTP error
        data = r.json()["data"]
        assert data["outbox"]["status"] == "failed"
        assert data["outbox"]["failure_reason"]

        # Retryable: still shows up under status=failed, confirm can be called again.
        failed = client.get(f"/devui/sustain/{sid}/egress?status=failed", headers=headers).json()["data"]["egress"]
        assert any(e["id"] == outbox["id"] for e in failed)

        retry = client.post(f"/devui/sustain/{sid}/egress/{outbox['id']}/confirm", json={}, headers=headers)
        assert retry.json()["data"]["outbox"]["attempts"] == 2


class TestCancelRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustain/x/egress/y/cancel", json={})
        assert r.status_code == 401

    def test_cancel_removes_from_prepared(self, client, homestead):
        headers, _, sid = homestead
        outbox = client.post(f"/devui/sustain/{sid}/egress/prepare-summary", json={"label": "x"}, headers=headers).json()["data"]["outbox"]

        r = client.post(f"/devui/sustain/{sid}/egress/{outbox['id']}/cancel", json={}, headers=headers)
        assert r.status_code == 200

        prepared = client.get(f"/devui/sustain/{sid}/egress?status=prepared", headers=headers).json()["data"]["egress"]
        assert prepared == []

    def test_cancel_unknown_returns_404(self, client, user):
        headers, _ = user
        r = client.post("/devui/sustain/x/egress/does-not-exist/cancel", json={}, headers=headers)
        assert r.status_code == 404
