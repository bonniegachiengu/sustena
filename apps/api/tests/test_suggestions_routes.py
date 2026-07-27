"""
tests/test_suggestions_routes.py

HTTP-layer tests for the Slice 10 devui.py routes:
  POST /devui/sustain/{id}/evaluate-operatives
  GET  /devui/sustain/{id}/suggestions
  POST /devui/sustain/{id}/suggestions/{sid}/accept
  POST /devui/sustain/{id}/suggestions/{sid}/dismiss
  GET  /devui/state — carries a "suggestions" key

Follows the same module-scoped TestClient + per-test user registration +
get_shared_engine() pattern as test_composition_routes.py / test_simulator_
routes.py, since these routes run against the real shared engine.
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


@pytest.fixture
def user(client):
    email = f"suggestions-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


def _instantiate(client, headers, template_id, user_id, **params):
    body = {"template_id": template_id, "user_id": user_id, "parameters": params}
    r = client.post("/devui/sustains", json=body, headers=headers)
    return r.json()["data"]["sustain_id"]


def _make_urgent_pocket(client, headers, sid, pocket, allocate_amt, spend_amt):
    client.post("/devui/console/execute", json={
        "sustain_id": sid, "operator_id": "budget.allocate",
        "params": {"pocket_name": pocket, "amount": allocate_amt, "period": "monthly"},
    }, headers=headers)
    client.post("/devui/console/execute", json={
        "sustain_id": sid, "operator_id": "budget.spend",
        "params": {"pocket_name": pocket, "amount": spend_amt, "description": "", "category": "general"},
    }, headers=headers)


@pytest.fixture
def homestead_with_condition(client, user):
    headers, user_id = user
    sid = _instantiate(client, headers, "homestead", user_id)
    client.post("/devui/console/execute", json={
        "sustain_id": sid, "operator_id": "budget.record_income",
        "params": {"amount": 5000, "source": "x", "frequency": "once"},
    }, headers=headers)
    _make_urgent_pocket(client, headers, sid, "food", 1000, 950)
    return headers, user_id, sid


class TestEvaluateRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustain/x/evaluate-operatives", json={})
        assert r.status_code == 401

    def test_evaluate_returns_fired_suggestion(self, client, homestead_with_condition):
        headers, _, sid = homestead_with_condition
        r = client.post(f"/devui/sustain/{sid}/evaluate-operatives", json={}, headers=headers)
        assert r.status_code == 200
        suggestions = r.json()["data"]["suggestions"]
        assert len(suggestions) == 1
        assert suggestions[0]["operative_id"] == "mentor"

    def test_evaluate_never_mutates_state(self, client, homestead_with_condition):
        headers, _, sid = homestead_with_condition
        before = client.get(f"/devui/state?sustain_id={sid}", headers=headers).json()["data"]["state"]
        client.post(f"/devui/sustain/{sid}/evaluate-operatives", json={}, headers=headers)
        after = client.get(f"/devui/state?sustain_id={sid}", headers=headers).json()["data"]["state"]
        assert before == after

    def test_unknown_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.post("/devui/sustain/does-not-exist/evaluate-operatives", json={}, headers=headers)
        assert r.status_code == 404


class TestDevuiStateCarriesSuggestions:
    def test_state_suggestions_key_empty_before_evaluate(self, client, user):
        headers, user_id = user
        sid = _instantiate(client, headers, "homestead", user_id)
        r = client.get(f"/devui/state?sustain_id={sid}", headers=headers)
        assert r.json()["data"]["suggestions"] == []

    def test_state_suggestions_key_reflects_evaluated_pending(self, client, homestead_with_condition):
        headers, _, sid = homestead_with_condition
        client.post(f"/devui/sustain/{sid}/evaluate-operatives", json={}, headers=headers)
        r = client.get(f"/devui/state?sustain_id={sid}", headers=headers)
        assert len(r.json()["data"]["suggestions"]) == 1


class TestListSuggestionsRoute:
    def test_no_auth_returns_401(self, client):
        assert client.get("/devui/sustain/x/suggestions").status_code == 401

    def test_status_filter(self, client, homestead_with_condition):
        headers, _, sid = homestead_with_condition
        client.post(f"/devui/sustain/{sid}/evaluate-operatives", json={}, headers=headers)
        pending = client.get(f"/devui/sustain/{sid}/suggestions?status=pending", headers=headers).json()["data"]["suggestions"]
        assert len(pending) == 1
        dismissed = client.get(f"/devui/sustain/{sid}/suggestions?status=dismissed", headers=headers).json()["data"]["suggestions"]
        assert dismissed == []


class TestAcceptRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustain/x/suggestions/y/accept", json={})
        assert r.status_code == 401

    def test_accept_runs_real_operator(self, client, homestead_with_condition):
        headers, _, sid = homestead_with_condition
        sug = client.post(f"/devui/sustain/{sid}/evaluate-operatives", json={}, headers=headers).json()["data"]["suggestions"][0]

        r = client.post(f"/devui/sustain/{sid}/suggestions/{sug['id']}/accept", json={}, headers=headers)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["result"]["status"] == "ok"

        state = client.get(f"/devui/state?sustain_id={sid}", headers=headers).json()["data"]["state"]
        assert state["finances"]["liquid"]["balance"] == 0.0

    def test_accept_unknown_suggestion_returns_422(self, client, user):
        headers, _ = user
        sid = _instantiate(client, headers, "homestead", user_id=user[1])
        r = client.post(f"/devui/sustain/{sid}/suggestions/does-not-exist/accept", json={}, headers=headers)
        assert r.status_code == 422

    def test_accept_honestly_reports_gate_refusal(self, client, homestead_with_condition):
        headers, _, sid = homestead_with_condition
        sug = client.post(f"/devui/sustain/{sid}/evaluate-operatives", json={}, headers=headers).json()["data"]["suggestions"][0]
        proposed_amount = sug["proposed_params"]["amount"]

        # Drift live state for real via a different real operator call.
        client.post("/devui/console/execute", json={
            "sustain_id": sid, "operator_id": "budget.allocate",
            "params": {"pocket_name": "rent", "amount": proposed_amount - 1, "period": "monthly"},
        }, headers=headers)

        r = client.post(f"/devui/sustain/{sid}/suggestions/{sug['id']}/accept", json={}, headers=headers)
        assert r.status_code == 200  # a refusal is a normal response body, not an HTTP error
        result = r.json()["data"]["result"]
        assert result["status"] == "failed"
        assert "liquid.balance" in result["reason"]

        still_pending = client.get(f"/devui/sustain/{sid}/suggestions?status=pending", headers=headers).json()["data"]["suggestions"]
        assert any(s["id"] == sug["id"] for s in still_pending)


class TestDismissRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustain/x/suggestions/y/dismiss", json={})
        assert r.status_code == 401

    def test_dismiss_removes_from_pending(self, client, homestead_with_condition):
        headers, _, sid = homestead_with_condition
        sug = client.post(f"/devui/sustain/{sid}/evaluate-operatives", json={}, headers=headers).json()["data"]["suggestions"][0]

        r = client.post(f"/devui/sustain/{sid}/suggestions/{sug['id']}/dismiss", json={}, headers=headers)
        assert r.status_code == 200

        pending = client.get(f"/devui/sustain/{sid}/suggestions?status=pending", headers=headers).json()["data"]["suggestions"]
        assert pending == []

    def test_dismiss_unknown_returns_404(self, client, user):
        headers, _ = user
        r = client.post("/devui/sustain/x/suggestions/does-not-exist/dismiss", json={}, headers=headers)
        assert r.status_code == 404
