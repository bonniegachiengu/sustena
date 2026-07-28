"""
tests/test_pawa_meter_routes.py

Tests for the Pawa meter's read-only HTTP surface (§4L):
  GET  /devui/pawa/operators
  GET  /devui/pawa/operators/{operator_name}
  GET  /devui/pawa/me
  GET  /devui/sustain/{id}/pawa
  and the measured_pawa/meter fields threaded into the existing
  /devui/registry/operators, /devui/sustain/{id}/operators, and
  /devui/console/execute responses.

Run with:
    python -m pytest tests/test_pawa_meter_routes.py -v
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
    yield


@pytest.fixture
def user(client):
    email = f"pawa-meter-tests-{uuid.uuid4().hex[:12]}@example.com"
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


def _run(client, headers, sustain, operator, params):
    return client.post(
        "/devui/console/execute",
        json={"sustain_id": sustain, "operator": operator, "params": params},
        headers=headers,
    )


class TestAuthGuard:
    def test_pawa_operators_requires_auth(self, client):
        assert client.get("/devui/pawa/operators").status_code == 401

    def test_pawa_me_requires_auth(self, client):
        assert client.get("/devui/pawa/me").status_code == 401


class TestOperatorStats:
    def test_never_run_operator_returns_none_not_zero(self, client, user):
        headers, _ = user
        r = client.get("/devui/pawa/operators/budget.transfer", headers=headers)
        assert r.status_code == 200
        assert r.json()["data"]["stats"] is None

    def test_a_real_run_shows_up_in_the_aggregate(self, client, user, sustain):
        headers, _ = user
        r = _run(client, headers, sustain, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        assert r.status_code == 200, r.text

        stat = client.get("/devui/pawa/operators/budget.record_income", headers=headers).json()["data"]["stats"]
        assert stat is not None
        assert stat["run_count"] >= 1
        assert stat["avg_pawa"] > 0

        all_ops = client.get("/devui/pawa/operators", headers=headers).json()["data"]["operators"]
        assert "budget.record_income" in all_ops


class TestSustainAndPrincipalTotals:
    def test_sustain_total_honest_zero_before_any_activity(self, client, user, sustain):
        headers, _ = user
        r = client.get(f"/devui/sustain/{sustain}/pawa", headers=headers)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["run_count"] == 0
        assert data["total_pawa"] == 0

    def test_sustain_total_reflects_a_real_run(self, client, user, sustain):
        headers, _ = user
        _run(client, headers, sustain, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        data = client.get(f"/devui/sustain/{sustain}/pawa", headers=headers).json()["data"]
        assert data["run_count"] == 1
        assert data["total_pawa"] > 0

    def test_me_reflects_the_authenticated_principals_own_activity(self, client, user, sustain):
        headers, _ = user
        _run(client, headers, sustain, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        data = client.get("/devui/pawa/me", headers=headers).json()["data"]
        assert data["run_count"] >= 1


class TestMeasuredPawaThreadedIntoExistingRoutes:
    def test_registry_operators_carries_measured_pawa_field(self, client, user, sustain):
        headers, _ = user
        _run(client, headers, sustain, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        ops = client.get("/devui/registry/operators", headers=headers).json()["data"]["operators"]
        assert "measured_pawa" in ops["budget.record_income"]
        assert ops["budget.record_income"]["measured_pawa"]["run_count"] >= 1
        # an operator that has never run still gets the honest None, not an absent key
        assert ops["budget.transfer"]["measured_pawa"] is None

    def test_sustain_operators_carries_measured_pawa_field(self, client, user, sustain):
        headers, _ = user
        _run(client, headers, sustain, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        ops = client.get(f"/devui/sustain/{sustain}/operators", headers=headers).json()["data"]["operators"]
        record_income = next(o for o in ops if o["name"] == "budget.record_income")
        assert record_income["measured_pawa"]["run_count"] >= 1

    def test_console_execute_returns_the_real_meter_reading_for_that_run(self, client, user, sustain):
        headers, _ = user
        r = _run(client, headers, sustain, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        meter = r.json()["data"]["meter"]
        assert meter is not None
        assert meter["pawa"] > 0
        assert meter["compute"] > 0

    def test_console_execute_meter_is_none_on_refusal(self, client, user, sustain):
        headers, _ = user
        r = _run(client, headers, sustain, "budget.allocate", {"pocket_name": "food", "amount": 999999, "period": "monthly"})
        assert r.json()["data"]["meter"] is None
