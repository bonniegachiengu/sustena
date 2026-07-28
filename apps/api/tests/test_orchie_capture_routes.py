"""
tests/test_orchie_capture_routes.py

Tests for POST /orchie/capture/infer and POST /orchie/capture/confirm --
effect-first capture (§7) over HTTP. Same module-scoped TestClient pattern
as test_orchie_compose_route.py / test_ingest_routes.py.

Run with:
    python -m pytest tests/test_orchie_capture_routes.py -v
"""

import uuid

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app

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
    yield


@pytest.fixture
def user(client):
    email = f"orchie-capture-tests-{uuid.uuid4().hex[:12]}@example.com"
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


def _seed_pocket(client, headers, sustain, name, allocated):
    r = client.post(
        "/devui/console/execute",
        json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": allocated, "source": "seed", "frequency": "once"}},
        headers=headers,
    )
    assert r.status_code == 200, r.text
    r = client.post(
        "/devui/console/execute",
        json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": name, "amount": allocated, "period": "monthly"}},
        headers=headers,
    )
    assert r.status_code == 200, r.text


class TestAuthGuard:
    def test_infer_no_auth_returns_401(self, client, sustain):
        r = client.post("/orchie/capture/infer", json={"sustain_id": sustain, "effect_text": "spent 100 on food"})
        assert r.status_code == 401

    def test_confirm_no_auth_returns_401(self, client, sustain):
        r = client.post("/orchie/capture/confirm", json={"sustain_id": sustain, "operator": "budget.spend", "params": {}})
        assert r.status_code == 401


class TestOwnershipBoundary:
    def test_infer_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.post("/orchie/capture/infer", json={"sustain_id": "does-not-exist", "effect_text": "spent 100 on food"}, headers=headers)
        assert r.status_code == 404

    def test_confirm_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.post("/orchie/capture/confirm", json={"sustain_id": "does-not-exist", "operator": "budget.spend", "params": {}}, headers=headers)
        assert r.status_code == 404


class TestInfer:
    def test_narration_resolves_to_ready(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "effect_text": "spent 500 on food"},
            headers=headers,
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["status"] == "ready"
        assert data["operator"] == "budget.spend"
        assert data["params"]["pocket_name"] == "food"
        assert data["params"]["amount"] == 500.0

    def test_ambiguous_pocket_asks_with_real_pocket_options(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        _seed_pocket(client, headers, sustain, "rent", 15000)
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "effect_text": "spent 500 on snacks"},
            headers=headers,
        )
        data = r.json()
        assert data["status"] == "needs_disambiguation"
        assert data["field"] == "pocket_name"
        values = {o["value"] for o in data["options"]}
        assert values == {"food", "rent"}

    def test_unknown_widget_id_returns_404(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "widget_id": "not-a-real-widget", "effect_text": "x"},
            headers=headers,
        )
        assert r.status_code == 404

    def test_from_a_real_ingested_unmapped_message(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        assert cap.status_code == 200
        message_id = cap.json()["data"]["message_id"]
        assert cap.json()["data"]["status"] == "needs_attention"

        # NAIVAS SUPERMARKET doesn't textually match "food" -- the engine
        # correctly can't guess the pocket from the raw message alone, and
        # must ask, offering the sustain's real declared pockets.
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id},
            headers=headers,
        )
        data = r.json()
        assert data["status"] == "needs_disambiguation"
        assert data["field"] == "pocket_name"
        assert {o["value"] for o in data["options"]} == {"food"}

        # second round: human taps "food" -- amount (450) and description
        # (NAIVAS SUPERMARKET) come from the message itself, never typed.
        r2 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {"pocket_name": "food"}},
            headers=headers,
        )
        data2 = r2.json()
        assert data2["status"] == "ready"
        assert data2["operator"] == "budget.spend"
        assert data2["params"]["amount"] == 450.0
        assert data2["params"]["description"] == "NAIVAS SUPERMARKET"

    def test_message_belonging_to_a_different_sustain_returns_404(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": "not-a-real-message-id"},
            headers=headers,
        )
        assert r.status_code == 404


class TestConfirm:
    def test_real_capture_commits_a_real_event_and_changes_state(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)

        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        food_before = state_before["finances"]["pockets"]["food"]["spent"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 500, "description": "groceries", "category": "food"}},
            headers=headers,
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["result"]["status"] == "ok"

        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_after["finances"]["pockets"]["food"]["spent"] == food_before + 500

        events = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["events"]
        assert events[0]["event_name"] == "event.finances.pocket_spent"

    def test_confirm_resolves_the_source_ingest_message_on_success(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 450, "description": "NAIVAS SUPERMARKET"}, "message_id": message_id},
            headers=headers,
        )
        assert r.json()["message_resolved"] is True

        msg = client.get(f"/api/v1/ingest/messages/{message_id}", headers=headers).json()["data"]
        assert msg["resolved_at"] is not None

    def test_honest_gate_refusal_is_a_normal_response_not_a_fake_success(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 100)  # only 100 allocated

        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 999999, "description": "too much"}},
            headers=headers,
        )
        assert r.status_code == 200  # a refusal is a normal response, not an HTTP error
        data = r.json()
        assert data["result"]["status"] == "failed"
        assert data["result"].get("reason") or data["result"].get("data", {}).get("reason")
        assert data["message_resolved"] is False

        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_before == state_after  # nothing changed on a refusal

    def test_refused_capture_leaves_source_message_still_needing_attention(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 100)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 999999, "description": "x"}, "message_id": message_id},
            headers=headers,
        )
        assert r.json()["message_resolved"] is False

        msg = client.get(f"/api/v1/ingest/messages/{message_id}", headers=headers).json()["data"]
        assert msg["resolved_at"] is None
        assert msg["status"] == "needs_attention"
