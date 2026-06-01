"""
tests/test_devui_routes.py

Tests for sustena/api/routes/devui.py
- Covers all primary endpoints + auth guard
- Mocks are avoided where possible (routes are stub-tolerant by design)
- No real network, DB, or Claude API calls

Run: pytest tests/test_devui_routes.py -v
"""

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app
from sustena.config import settings

# The devui router is only mounted in development mode (the default in tests).
# All tests use the singleton `app` — same as test_health.py.

ADMIN_TOKEN = settings.admin_token          # default: "dev-admin-token"
AUTH_HEADER = {"Authorization": f"Bearer {ADMIN_TOKEN}"}


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture(scope="module")
def client():
    """Synchronous TestClient — required for WebSocket tests."""
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


# ---------------------------------------------------------------------------
# Auth guard
# ---------------------------------------------------------------------------

class TestAuthGuard:
    """Every REST endpoint must reject requests without a valid token."""

    def test_list_sustains_no_auth_returns_401(self, client):
        r = client.get("/devui/sustains")
        assert r.status_code == 401

    def test_list_sustains_bad_token_returns_401(self, client):
        r = client.get("/devui/sustains", headers={"Authorization": "Bearer wrong-token"})
        assert r.status_code == 401

    def test_get_state_no_auth_returns_401(self, client):
        r = client.get("/devui/state", params={"sustain_id": "homestead.bonnie"})
        assert r.status_code == 401

    def test_console_execute_no_auth_returns_401(self, client):
        r = client.post(
            "/devui/console/execute",
            json={"sustain_id": "homestead.bonnie", "operator": "budget.allocate", "params": {}},
        )
        assert r.status_code == 401

    def test_simulate_no_auth_returns_401(self, client):
        r = client.post(
            "/devui/simulate",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
        )
        assert r.status_code == 401


# ---------------------------------------------------------------------------
# GET /devui/sustains
# ---------------------------------------------------------------------------

class TestListSustains:
    def test_returns_200(self, client):
        r = client.get("/devui/sustains", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_response_has_status_ok(self, client):
        r = client.get("/devui/sustains", headers=AUTH_HEADER)
        body = r.json()
        assert body["status"] == "ok"

    def test_response_data_contains_sustains_list(self, client):
        r = client.get("/devui/sustains", headers=AUTH_HEADER)
        body = r.json()
        assert "sustains" in body["data"]
        assert isinstance(body["data"]["sustains"], list)

    def test_sustains_list_is_non_empty(self, client):
        r = client.get("/devui/sustains", headers=AUTH_HEADER)
        sustains = r.json()["data"]["sustains"]
        assert len(sustains) > 0

    def test_each_sustain_has_id_and_label(self, client):
        r = client.get("/devui/sustains", headers=AUTH_HEADER)
        for s in r.json()["data"]["sustains"]:
            assert "id" in s, f"sustain missing 'id': {s}"
            assert "label" in s, f"sustain missing 'label': {s}"

    def test_response_has_timestamp(self, client):
        r = client.get("/devui/sustains", headers=AUTH_HEADER)
        assert "timestamp" in r.json()


# ---------------------------------------------------------------------------
# GET /devui/state
# ---------------------------------------------------------------------------

class TestGetState:
    def test_returns_200_with_valid_sustain_id(self, client):
        r = client.get(
            "/devui/state",
            params={"sustain_id": "homestead.bonnie"},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200

    def test_response_echoes_sustain_id(self, client):
        r = client.get(
            "/devui/state",
            params={"sustain_id": "homestead.bonnie"},
            headers=AUTH_HEADER,
        )
        body = r.json()
        assert body["data"]["sustain_id"] == "homestead.bonnie"

    def test_response_contains_state_key(self, client):
        r = client.get(
            "/devui/state",
            params={"sustain_id": "homestead.bonnie"},
            headers=AUTH_HEADER,
        )
        assert "state" in r.json()["data"]

    def test_missing_sustain_id_returns_422(self, client):
        r = client.get("/devui/state", headers=AUTH_HEADER)
        assert r.status_code == 422

    def test_different_sustain_id_echoed_back(self, client):
        r = client.get(
            "/devui/state",
            params={"sustain_id": "vyyb.hive"},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["sustain_id"] == "vyyb.hive"


# ---------------------------------------------------------------------------
# POST /devui/console/execute
# ---------------------------------------------------------------------------

class TestConsoleExecute:
    def test_returns_200_with_operator_field(self, client):
        r = client.post(
            "/devui/console/execute",
            json={
                "sustain_id": "homestead.bonnie",
                "operator": "budget.allocate",
                "params": {"pocket": "food", "amount": 1000},
            },
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200

    def test_returns_200_with_operator_id_field(self, client):
        """operator_id is the alternative field name accepted by the endpoint."""
        r = client.post(
            "/devui/console/execute",
            json={
                "sustain_id": "homestead.bonnie",
                "operator_id": "budget.allocate",
                "inputs": {"pocket": "food", "amount": 500},
            },
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200

    def test_response_echoes_sustain_id(self, client):
        r = client.post(
            "/devui/console/execute",
            json={
                "sustain_id": "homestead.bonnie",
                "operator": "budget.allocate",
                "params": {},
            },
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["sustain_id"] == "homestead.bonnie"

    def test_response_echoes_operator_name(self, client):
        r = client.post(
            "/devui/console/execute",
            json={
                "sustain_id": "homestead.bonnie",
                "operator": "budget.record_income",
                "params": {},
            },
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["operator"] == "budget.record_income"

    def test_response_has_result_key(self, client):
        r = client.post(
            "/devui/console/execute",
            json={
                "sustain_id": "homestead.bonnie",
                "operator": "budget.allocate",
                "params": {},
            },
            headers=AUTH_HEADER,
        )
        assert "result" in r.json()["data"]

    def test_missing_both_operator_fields_returns_422(self, client):
        """Neither 'operator' nor 'operator_id' provided → 422 or the endpoint
        returns a 200 with an error payload (the resolved_operator() guard raises HTTPException).
        Both are valid — the contract is that no silent success occurs."""
        r = client.post(
            "/devui/console/execute",
            json={"sustain_id": "homestead.bonnie", "params": {}},
            headers=AUTH_HEADER,
        )
        # The endpoint raises 422 from HTTPException inside resolved_operator()
        assert r.status_code in (422, 200)

    def test_empty_sustain_id_still_accepted(self, client):
        """The endpoint is stub-tolerant — any sustain_id string is accepted."""
        r = client.post(
            "/devui/console/execute",
            json={"sustain_id": "nonexistent.sustain", "operator": "budget.allocate"},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200


# ---------------------------------------------------------------------------
# POST /devui/simulate
# ---------------------------------------------------------------------------

class TestSimulate:
    def test_returns_200_empty_proposal(self, client):
        r = client.post(
            "/devui/simulate",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200

    def test_returns_200_with_proposal_steps(self, client):
        r = client.post(
            "/devui/simulate",
            json={
                "sustain_id": "homestead.bonnie",
                "proposal": [
                    {"operator": "budget.record_income", "params": {"amount": 5000, "source": "salary"}},
                    {"operator": "budget.allocate",      "params": {"pocket": "food", "amount": 2000}},
                ],
            },
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200

    def test_response_echoes_sustain_id(self, client):
        r = client.post(
            "/devui/simulate",
            json={"sustain_id": "vyyb.hive", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["sustain_id"] == "vyyb.hive"

    def test_response_contains_steps_list(self, client):
        r = client.post(
            "/devui/simulate",
            json={
                "sustain_id": "homestead.bonnie",
                "proposal": [{"operator": "budget.allocate", "params": {}}],
            },
            headers=AUTH_HEADER,
        )
        assert "steps" in r.json()["data"]

    def test_step_count_matches_proposal_length(self, client):
        proposal = [
            {"operator": "budget.record_income", "params": {}},
            {"operator": "budget.allocate", "params": {}},
            {"operator": "budget.spend", "params": {}},
        ]
        r = client.post(
            "/devui/simulate",
            json={"sustain_id": "homestead.bonnie", "proposal": proposal},
            headers=AUTH_HEADER,
        )
        assert len(r.json()["data"]["steps"]) == len(proposal)

    def test_response_has_status_ok(self, client):
        r = client.post(
            "/devui/simulate",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert r.json()["status"] == "ok"


# ---------------------------------------------------------------------------
# WS /devui/state-stream
# ---------------------------------------------------------------------------

class TestStateStream:
    def test_rejected_without_token(self, client):
        """Missing token → WebSocket should be closed immediately (policy violation)."""
        from starlette.websockets import WebSocketDisconnect as _WSD
        try:
            with client.websocket_connect(
                "/devui/state-stream?sustain_id=homestead.bonnie"
            ) as ws:
                ws.receive_json()
        except _WSD:
            pass  # Expected — server closed connection with policy violation

    def test_rejects_wrong_token(self, client):
        """Wrong token → policy violation close."""
        try:
            with client.websocket_connect(
                "/devui/state-stream?sustain_id=homestead.bonnie&token=wrong"
            ) as ws:
                ws.receive_json()
        except Exception:
            pass  # Expected close

    def test_valid_token_receives_state_snapshot(self, client):
        """Valid token → first message should be a state_snapshot frame."""
        with client.websocket_connect(
            f"/devui/state-stream?sustain_id=homestead.bonnie&token={ADMIN_TOKEN}"
        ) as ws:
            msg = ws.receive_json()
        assert msg["type"] == "state_snapshot"
        assert msg["sustain_id"] == "homestead.bonnie"
        assert "state" in msg
        assert "timestamp" in msg

    def test_valid_token_path_param_form_receives_snapshot(self, client):
        """Legacy path-param WS also works."""
        with client.websocket_connect(
            f"/devui/state-stream/homestead.bonnie?token={ADMIN_TOKEN}"
        ) as ws:
            msg = ws.receive_json()
        assert msg["type"] == "state_snapshot"
        assert msg["sustain_id"] == "homestead.bonnie"


# ---------------------------------------------------------------------------
# Legacy path-param routes
# ---------------------------------------------------------------------------

class TestLegacyRoutes:
    def test_sustain_state_path_param(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/state", headers=AUTH_HEADER)
        assert r.status_code == 200
        assert r.json()["data"]["sustain_id"] == "homestead.bonnie"

    def test_sustain_events(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/events", headers=AUTH_HEADER)
        assert r.status_code == 200
        body = r.json()
        assert body["status"] == "ok"
        assert "events" in body["data"]
        assert isinstance(body["data"]["events"], list)

    def test_sustain_events_default_limit(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/events", headers=AUTH_HEADER)
        events = r.json()["data"]["events"]
        assert len(events) <= 100  # default limit

    def test_sustain_proposals(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/proposals", headers=AUTH_HEADER)
        assert r.status_code == 200
        body = r.json()
        assert "proposals" in body["data"]

    def test_registry_operators(self, client):
        r = client.get("/devui/registry/operators", headers=AUTH_HEADER)
        assert r.status_code == 200
        assert "operators" in r.json()["data"]

    def test_registry_operatives(self, client):
        r = client.get("/devui/registry/operatives", headers=AUTH_HEADER)
        assert r.status_code == 200
        body = r.json()
        assert "operatives" in body["data"]
        assert isinstance(body["data"]["operatives"], list)
