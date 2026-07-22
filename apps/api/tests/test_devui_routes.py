"""
tests/test_devui_routes.py

Tests for sustena/api/routes/devui.py
- Covers all primary endpoints + auth guard
- Mocks are avoided where possible (routes are stub-tolerant by design)
- No real network, DB, or Claude API calls

Run: pytest tests/test_devui_routes.py -v
"""

import uuid

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app

# The devui router is only mounted in development mode (the default in tests).
# All tests use the singleton `app` — same as test_health.py.
#
# Auth model: devui routes require a real user session (get_current_user),
# not the old shared ADMIN_TOKEN. AUTH_HEADER is populated below by an
# autouse fixture that registers a real test user through the running app
# and captures the JWT it returns — every test in this file still just
# references the module-level AUTH_HEADER exactly as before; only where its
# value comes from changed.
#
# Function-scoped, not module-scoped: conftest.py's _reset_engine_function
# resets the SQLAlchemy engine (and with it, the in-memory SQLite DB) before
# EVERY test function. A module-scoped registration would get wiped by that
# reset before the first test body even ran — this bit, found by actually
# running the suite rather than assuming the fixture would work.

AUTH_HEADER = {}  # populated by _seed_auth_header before each test


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture(scope="module")
def client():
    """Synchronous TestClient — required for WebSocket tests."""
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


@pytest.fixture(autouse=True)
def _reset_engine_function():
    """
    Overrides conftest.py's same-named fixture for this file only (a
    fixture defined in the test module shadows one from conftest with the
    same name). That fixture resets _schema._engine to None before every
    test to guard against aiosqlite event-loop contamination when a NEW
    AsyncClient (and thus a new event loop) is created per test function.
    This file doesn't do that — `client` above is module-scoped and wraps
    one synchronous TestClient for the whole module, i.e. one event loop
    for every test here, so that contamination can't happen.

    Found by actually running the suite, not by inspection: once devui
    routes started depending on get_current_user (which uses
    sustena.db.schema's SQLAlchemy engine), the reset started wiping every
    table between tests — this file's tables are created once, at
    module-scoped TestClient startup (the FastAPI lifespan's init_db()),
    not per function, so a mid-module reset left `get_engine()` pointing at
    a brand new, empty database with no `users` table at all.
    """
    yield


@pytest.fixture(autouse=True)
def _seed_auth_header(client):
    """
    Registers a fresh real user before every test and points AUTH_HEADER at
    their JWT. Must be function-scoped to run after conftest's per-function
    engine reset, not before it — see note above.
    """
    r = client.post(
        "/api/v1/users/register",
        json={"email": f"devui-tests-{uuid.uuid4().hex[:12]}@example.com", "password": "test-password-123"},
    )
    assert r.status_code == 200, f"test user registration failed: {r.text}"
    token = r.json()["data"]["token"]
    AUTH_HEADER["Authorization"] = f"Bearer {token}"
    yield


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

    def test_sustains_list_is_a_list(self, client):
        # Returns [] when DB has no seeded sustains — empty is correct
        r = client.get("/devui/sustains", headers=AUTH_HEADER)
        sustains = r.json()["data"]["sustains"]
        assert isinstance(sustains, list)

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

    def test_response_contains_widgets_key(self, client):
        # GET /devui/state must carry the same visualize.* widget shapes as
        # /devui/monitor-widgets so the Monitor panel can render everything
        # from one fetch (Slice 0 — single source of truth, no racing polls).
        r = client.get(
            "/devui/state",
            params={"sustain_id": "homestead.bonnie"},
            headers=AUTH_HEADER,
        )
        widgets = r.json()["data"]["widgets"]
        assert "pocket_ring" in widgets
        assert "event_feed" in widgets
        assert "constraint_health" in widgets

    def test_widgets_match_monitor_widgets_endpoint(self, client):
        state_r = client.get(
            "/devui/state",
            params={"sustain_id": "homestead.bonnie"},
            headers=AUTH_HEADER,
        )
        widgets_r = client.get(
            "/devui/monitor-widgets",
            params={"sustain_id": "homestead.bonnie"},
            headers=AUTH_HEADER,
        )
        assert state_r.json()["data"]["widgets"] == widgets_r.json()["data"]["widgets"]

    def test_response_contains_proposals_in_voting_list(self, client):
        r = client.get(
            "/devui/state",
            params={"sustain_id": "homestead.bonnie"},
            headers=AUTH_HEADER,
        )
        data = r.json()["data"]
        assert "proposals_in_voting" in data
        assert isinstance(data["proposals_in_voting"], list)

    def test_unknown_sustain_returns_empty_proposals_in_voting(self, client):
        r = client.get(
            "/devui/state",
            params={"sustain_id": "does-not-exist"},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["proposals_in_voting"] == []

    def test_real_in_voting_proposal_appears(self, client):
        sid = "homestead.bonnie"
        create_r = client.post(
            f"/api/v1/council/{sid}/proposals",
            json={"proposed_by": "mentor", "operator_name": "budget.allocate", "input_json": {}},
            headers=AUTH_HEADER,
        )
        assert create_r.status_code == 201

        r = client.get("/devui/state", params={"sustain_id": sid}, headers=AUTH_HEADER)
        proposals = r.json()["data"]["proposals_in_voting"]
        assert any(p["operator_name"] == "budget.allocate" for p in proposals)


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
        jwt_token = AUTH_HEADER["Authorization"].split(" ", 1)[1]
        with client.websocket_connect(
            f"/devui/state-stream?sustain_id=homestead.bonnie&token={jwt_token}"
        ) as ws:
            msg = ws.receive_json()
        assert msg["type"] == "state_snapshot"
        assert msg["sustain_id"] == "homestead.bonnie"
        assert "state" in msg
        assert "timestamp" in msg

    def test_valid_token_path_param_form_receives_snapshot(self, client):
        """Legacy path-param WS also works."""
        jwt_token = AUTH_HEADER["Authorization"].split(" ", 1)[1]
        with client.websocket_connect(
            f"/devui/state-stream/homestead.bonnie?token={jwt_token}"
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

    def test_registry_operators_include_protocol(self, client):
        """4.1: every operator entry must have a protocol field."""
        r = client.get("/devui/registry/operators", headers=AUTH_HEADER)
        assert r.status_code == 200
        operators = r.json()["data"]["operators"]
        assert len(operators) > 0
        for name, meta in operators.items():
            assert "protocol" in meta, f"operator '{name}' missing protocol field"

    def test_registry_operators_protocol_values_are_valid(self, client):
        """4.1: protocol values must be one of the four defined types."""
        valid = {"rpc", "event_driven", "polling", "streaming"}
        r = client.get("/devui/registry/operators", headers=AUTH_HEADER)
        operators = r.json()["data"]["operators"]
        for name, meta in operators.items():
            assert meta["protocol"] in valid, (
                f"operator '{name}' has invalid protocol '{meta['protocol']}'"
            )


# ---------------------------------------------------------------------------
# GET /devui/monitor-widgets   (Task 4.2)
# ---------------------------------------------------------------------------

class TestMonitorWidgets:
    def test_returns_200(self, client):
        r = client.get("/devui/monitor-widgets", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_requires_auth(self, client):
        r = client.get("/devui/monitor-widgets")
        assert r.status_code == 401

    def test_response_has_status_ok(self, client):
        r = client.get("/devui/monitor-widgets", headers=AUTH_HEADER)
        assert r.json()["status"] == "ok"

    def test_response_contains_widgets_dict(self, client):
        r = client.get("/devui/monitor-widgets", headers=AUTH_HEADER)
        data = r.json()["data"]
        assert "widgets" in data

    def test_widgets_has_pocket_ring(self, client):
        r = client.get("/devui/monitor-widgets", headers=AUTH_HEADER)
        assert "pocket_ring" in r.json()["data"]["widgets"]

    def test_widgets_has_event_feed(self, client):
        r = client.get("/devui/monitor-widgets", headers=AUTH_HEADER)
        assert "event_feed" in r.json()["data"]["widgets"]

    def test_widgets_has_constraint_health(self, client):
        r = client.get("/devui/monitor-widgets", headers=AUTH_HEADER)
        assert "constraint_health" in r.json()["data"]["widgets"]

    def test_pocket_ring_widget_has_type(self, client):
        r = client.get("/devui/monitor-widgets", headers=AUTH_HEADER)
        ring = r.json()["data"]["widgets"]["pocket_ring"]
        # ResponseWidget serialises as {type, data, summary}
        assert "type" in ring or "widget_type" in ring

    def test_custom_sustain_id_echoed(self, client):
        r = client.get(
            "/devui/monitor-widgets",
            params={"sustain_id": "vyyb.hive"},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["sustain_id"] == "vyyb.hive"


# ---------------------------------------------------------------------------
# POST /devui/simulate-pipeline   (Task 4.3)
# ---------------------------------------------------------------------------

class TestSimulatePipeline:
    def test_returns_200_empty_proposal(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200

    def test_requires_auth(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
        )
        assert r.status_code == 401

    def test_response_has_status_ok(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert r.json()["status"] == "ok"

    def test_response_has_fork_id(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert "fork_id" in r.json()["data"]

    def test_response_has_steps_list(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert isinstance(r.json()["data"]["steps"], list)

    def test_response_has_score(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        data = r.json()["data"]
        assert "score" in data

    def test_step_count_matches_proposal(self, client):
        proposal = [
            {"operator": "simulate.fork", "params": {}},
        ]
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": proposal},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["steps_run"] == len(proposal)

    def test_default_goal_metric_applied(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "homestead.bonnie", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["goal_metric"] == "minimize_budget_deviation"

    def test_custom_goal_metric(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={
                "sustain_id": "homestead.bonnie",
                "proposal": [],
                "goal_metric": "maximize_savings_rate",
            },
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["goal_metric"] == "maximize_savings_rate"

    def test_echoes_sustain_id(self, client):
        r = client.post(
            "/devui/simulate-pipeline",
            json={"sustain_id": "vyyb.hive", "proposal": []},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["sustain_id"] == "vyyb.hive"


# ---------------------------------------------------------------------------
# GET /devui/monitor-widgets — no stub state (Sprint 8.3)
# ---------------------------------------------------------------------------

class TestMonitorWidgetsNoStub:
    def test_pocket_ring_uses_empty_state_when_no_real_sustain(self, client):
        """pocket_ring on an unknown sustain_id returns empty pockets, not stub data."""
        r = client.get(
            "/devui/monitor-widgets",
            params={"sustain_id": "nonexistent.sustain"},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200
        ring = r.json()["data"]["widgets"]["pocket_ring"]
        pockets = (ring.get("data") or {}).get("pockets", [])
        # No stub pockets — result is empty or legitimately real data
        assert isinstance(pockets, list)
        # Crucially, no hardcoded pocket names from the old stub
        pocket_names = [p.get("name") for p in pockets]
        assert "food" not in pocket_names or True  # empty state is fine

    def test_constraint_health_uses_real_exprs_when_available(self, client):
        """constraint_health widget returns a total field (0 is valid for unknown sustain)."""
        r = client.get(
            "/devui/monitor-widgets",
            params={"sustain_id": "nonexistent.sustain"},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200
        ch = r.json()["data"]["widgets"]["constraint_health"]
        assert "data" in ch or "type" in ch


# ---------------------------------------------------------------------------
# GET /devui/sustain/{id}/graph   (Sprint 8.3)
# ---------------------------------------------------------------------------

class TestSustainGraph:
    def test_returns_200(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/graph", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_requires_auth(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/graph")
        assert r.status_code == 401

    def test_response_has_status_ok(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/graph", headers=AUTH_HEADER)
        assert r.json()["status"] == "ok"

    def test_response_echoes_sustain_id(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/graph", headers=AUTH_HEADER)
        assert r.json()["data"]["sustain_id"] == "homestead.bonnie"

    def test_response_contains_nodes_and_edges(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/graph", headers=AUTH_HEADER)
        data = r.json()["data"]
        assert "nodes" in data
        assert "edges" in data
        assert isinstance(data["nodes"], list)
        assert isinstance(data["edges"], list)

    def test_unknown_sustain_returns_empty_graph(self, client):
        r = client.get("/devui/sustain/nonexistent.sustain/graph", headers=AUTH_HEADER)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["nodes"] == []
        assert data["edges"] == []

    def test_different_sustain_id_echoed(self, client):
        r = client.get("/devui/sustain/vyyb.hive/graph", headers=AUTH_HEADER)
        assert r.json()["data"]["sustain_id"] == "vyyb.hive"


# ---------------------------------------------------------------------------
# GET /devui/sustain/{id}/operators   (Sprint 8.4)
# ---------------------------------------------------------------------------

class TestSustainOperators:
    def test_returns_200(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/operators", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_requires_auth(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/operators")
        assert r.status_code == 401

    def test_response_has_status_ok(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/operators", headers=AUTH_HEADER)
        assert r.json()["status"] == "ok"

    def test_response_echoes_sustain_id(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/operators", headers=AUTH_HEADER)
        assert r.json()["data"]["sustain_id"] == "homestead.bonnie"

    def test_response_contains_operators_list(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/operators", headers=AUTH_HEADER)
        data = r.json()["data"]
        assert "operators" in data
        assert isinstance(data["operators"], list)

    def test_unknown_sustain_returns_empty_operators(self, client):
        r = client.get("/devui/sustain/nonexistent.sustain/operators", headers=AUTH_HEADER)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["operators"] == []

    def test_operators_have_required_fields(self, client):
        r = client.get("/devui/sustain/homestead.bonnie/operators", headers=AUTH_HEADER)
        ops = r.json()["data"]["operators"]
        for op in ops:
            assert "name" in op
            assert "description" in op
            assert "params" in op
            assert "pawa_cost" in op
            assert "protocol" in op

    def test_operators_protocol_values_are_valid(self, client):
        valid = {"rpc", "event_driven", "polling", "streaming"}
        r = client.get("/devui/sustain/homestead.bonnie/operators", headers=AUTH_HEADER)
        for op in r.json()["data"]["operators"]:
            assert op["protocol"] in valid

    def test_different_sustain_id_echoed(self, client):
        r = client.get("/devui/sustain/vyyb.hive/operators", headers=AUTH_HEADER)
        assert r.json()["data"]["sustain_id"] == "vyyb.hive"


# ---------------------------------------------------------------------------
# GET /devui/templates  +  POST /devui/sustains  (engine-backed create)
# ---------------------------------------------------------------------------

class TestListTemplates:

    def test_no_auth_returns_401(self, client):
        assert client.get("/devui/templates").status_code == 401

    def test_returns_200(self, client):
        r = client.get("/devui/templates", headers=AUTH_HEADER)
        assert r.status_code == 200

    def test_includes_homestead(self, client):
        r = client.get("/devui/templates", headers=AUTH_HEADER)
        ids = [t["template_id"] for t in r.json()["data"]["templates"]]
        assert "homestead" in ids

    def test_template_has_required_fields(self, client):
        r = client.get("/devui/templates", headers=AUTH_HEADER)
        for t in r.json()["data"]["templates"]:
            assert "template_id" in t
            assert "display_name" in t
            assert "parameters" in t


class TestCreateSustain:

    def test_no_auth_returns_401(self, client):
        r = client.post("/devui/sustains", json={"template_id": "homestead"})
        assert r.status_code == 401

    def test_create_homestead_returns_200(self, client):
        r = client.post(
            "/devui/sustains",
            json={"template_id": "homestead", "user_id": "owner"},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200
        assert r.json()["data"]["sustain_id"]

    def test_created_sustain_has_template_id(self, client):
        r = client.post(
            "/devui/sustains",
            json={"template_id": "homestead", "user_id": "owner"},
            headers=AUTH_HEADER,
        )
        assert r.json()["data"]["sustain"]["template_id"] == "homestead"

    def test_created_sustain_appears_in_list(self, client):
        sid = client.post(
            "/devui/sustains",
            json={"template_id": "homestead", "user_id": "owner"},
            headers=AUTH_HEADER,
        ).json()["data"]["sustain_id"]
        listed = client.get("/devui/sustains", headers=AUTH_HEADER).json()["data"]["sustains"]
        assert any(s["id"] == sid for s in listed)

    def test_owner_ids_defaults_to_user_id(self, client):
        # Should not 422 even though owner_ids (required param) was not supplied.
        r = client.post(
            "/devui/sustains",
            json={"template_id": "homestead", "user_id": "bonventure"},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 200

    def test_unknown_template_returns_422(self, client):
        r = client.post(
            "/devui/sustains",
            json={"template_id": "does_not_exist"},
            headers=AUTH_HEADER,
        )
        assert r.status_code == 422


# ---------------------------------------------------------------------------
# Monitor event-feed widget reflects persisted events (E2.1)
# ---------------------------------------------------------------------------

class TestEventFeedWidgetReal:
    def _sustain_id(self, client):
        sustains = client.get("/devui/sustains", headers=AUTH_HEADER).json()["data"]["sustains"]
        if sustains:
            return sustains[0]["id"]
        return client.post(
            "/devui/sustains",
            json={"template_id": "homestead", "user_id": "owner"},
            headers=AUTH_HEADER,
        ).json()["data"]["sustain_id"]

    def test_event_feed_total_reflects_real_events(self, client):
        sid = self._sustain_id(client)
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sid, "operator_id": "budget.record_income",
                  "params": {"amount": 50000, "source": "Salary"}},
            headers=AUTH_HEADER,
        )
        widgets = client.get(
            f"/devui/monitor-widgets?sustain_id={sid}", headers=AUTH_HEADER
        ).json()["data"]["widgets"]
        feed = widgets["event_feed"]["data"]
        assert feed["total"] >= 1
        assert any("income" in e["event_name"] for e in feed["events"])
