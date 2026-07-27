"""
tests/test_simulator_routes.py

Tests for the Slice 9 (Simulator / scenario tree) devui.py routes:
  POST /devui/simulate                        — now carries parent_rollup
  POST /devui/sustain/{id}/promote-simulation  — genuine replay

Follows the same module-scoped TestClient + per-test user registration +
get_shared_engine() fixture-building pattern as test_composition_routes.py,
since these routes run against the real shared engine, not a per-test
:memory: instance.
"""

import json as _json
import uuid

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app
from sustena.core.engine_singleton import get_shared_engine


@pytest.fixture(scope="module")
def client():
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


@pytest.fixture(autouse=True)
def _reset_engine_function():
    # Overrides conftest.py's per-function engine reset (which would tear
    # down the module-scoped client's engine between every test) — same
    # no-op override test_composition_routes.py uses for the identical reason.
    yield


@pytest.fixture
def user(client):
    email = f"simulator-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


def _create_definition(client, headers, user_id, **overrides):
    body = {
        "user_id": user_id,
        "display_name": overrides.pop("display_name", "Leaf"),
        "description": "",
        "dimensions": overrides.pop("dimensions", [
            {"name": "moisture_level", "type": "number", "default_value": 0},
        ]),
        "invariants": overrides.pop("invariants", []),
        "operator_names": overrides.pop("operator_names", ["edit.state_patch"]),
    }
    return client.post("/devui/definitions", json=body, headers=headers).json()["data"]


def _instantiate(client, headers, template_id, user_id):
    r = client.post("/devui/sustains", json={"template_id": template_id, "user_id": user_id, "parameters": {}}, headers=headers)
    return r.json()["data"]["sustain_id"]


@pytest.fixture
def homestead_sustain(client, user):
    headers, user_id = user
    sid = _instantiate(client, headers, "homestead", user_id)
    return headers, user_id, sid


# ── POST /devui/simulate ──────────────────────────────────────────────────────

class TestSimulateRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/simulate", json={"sustain_id": "x", "proposal": []})
        assert r.status_code == 401

    def test_simulate_never_mutates_live_state(self, client, homestead_sustain):
        headers, _, sid = homestead_sustain
        before = client.get(f"/devui/state?sustain_id={sid}", headers=headers).json()["data"]["state"]

        r = client.post("/devui/simulate", json={
            "sustain_id": sid,
            "proposal": [{"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}}],
        }, headers=headers)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["final_state"]["finances"]["liquid"]["balance"] == 5000

        after = client.get(f"/devui/state?sustain_id={sid}", headers=headers).json()["data"]["state"]
        assert after == before  # live state completely untouched

    def test_simulate_carries_parent_rollup_for_a_child(self, client, user):
        headers, user_id = user
        child_def = _create_definition(client, headers, user_id, display_name="Leaf")
        parent_def = _create_definition(client, headers, user_id, display_name="Pod")

        engine = get_shared_engine()
        spec = engine.get_definition(parent_def["template_id"])["spec"]
        spec["aggregates"] = [{"id": "pod_total", "child_path": "moisture_level", "op": "sum"}]
        engine._db.execute(
            "UPDATE sustain_templates SET spec_json = ? WHERE id = ?",
            (_json.dumps(spec), parent_def["template_id"]),
        )
        engine._db.commit()

        parent = _instantiate(client, headers, parent_def["template_id"], user_id)
        child = _instantiate(client, headers, child_def["template_id"], user_id)
        client.post(f"/devui/sustain/{parent}/children", json={"child_sustain_id": child}, headers=headers)

        r = client.post("/devui/simulate", json={
            "sustain_id": child,
            "proposal": [{"operator": "edit.state_patch", "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": 77}]}}],
        }, headers=headers)
        data = r.json()["data"]
        assert data["final_parent_rollup"]["aggregates"]["pod_total"]["value"] == 77

        # parent's OWN live state / rollup untouched by the child's simulation
        parent_rollup = client.get(f"/devui/sustain/{parent}/rollup", headers=headers).json()["data"]
        assert parent_rollup["aggregates"]["pod_total"]["value"] == 0

    def test_simulate_captures_a_gate_refusal_in_branch(self, client, homestead_sustain):
        headers, _, sid = homestead_sustain
        r = client.post("/devui/simulate", json={
            "sustain_id": sid,
            "proposal": [
                {"operator": "budget.record_income", "params": {"amount": 1000, "source": "x", "frequency": "once"}},
                {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 999999, "period": "monthly"}},
            ],
        }, headers=headers)
        steps = r.json()["data"]["steps"]
        assert steps[0]["result"]["status"] == "ok"
        assert steps[1]["result"]["status"] == "failed"


# ── POST /devui/sustain/{id}/promote-simulation ───────────────────────────────

class TestPromoteSimulationRoute:
    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustain/x/promote-simulation", json={"steps": []})
        assert r.status_code == 401

    def test_promote_genuinely_applies_state(self, client, homestead_sustain):
        headers, _, sid = homestead_sustain
        r = client.post(f"/devui/sustain/{sid}/promote-simulation", json={
            "steps": [{"operator": "budget.record_income", "params": {"amount": 4000, "source": "x", "frequency": "once"}}],
        }, headers=headers)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["all_succeeded"] is True
        assert data["steps_attempted"] == 1

        state = client.get(f"/devui/state?sustain_id={sid}", headers=headers).json()["data"]["state"]
        assert state["finances"]["liquid"]["balance"] == 4000

    def test_promote_stops_at_first_real_failure(self, client, homestead_sustain):
        headers, _, sid = homestead_sustain
        r = client.post(f"/devui/sustain/{sid}/promote-simulation", json={
            "steps": [
                {"operator": "budget.record_income", "params": {"amount": 500, "source": "x", "frequency": "once"}},
                {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 999999, "period": "monthly"}},
                {"operator": "budget.record_income", "params": {"amount": 1, "source": "never-reached", "frequency": "once"}},
            ],
        }, headers=headers)
        data = r.json()["data"]
        assert data["all_succeeded"] is False
        assert data["steps_attempted"] == 2
        assert data["steps_requested"] == 3
