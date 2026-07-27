"""
tests/test_definition_routes.py

Tests for the Slice 6 (Create/Definition flow) devui.py routes:
  POST   /devui/validate-invariant
  GET    /devui/definitions
  GET    /devui/definitions/{template_id}
  POST   /devui/definitions
  PATCH  /devui/definitions/{template_id}
  GET    /devui/templates (merges user-created definitions)

Follows the same module-scoped TestClient + per-test user registration
pattern as test_devui_routes.py / test_ingest_routes.py: one long-lived
TestClient/event loop for the module so get_shared_engine() (sync sqlite3)
and the SQLAlchemy async engine both see the same lifespan-created DB
throughout.
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
    """See test_devui_routes.py's identical note: one long-lived TestClient
    for this module means a per-function engine reset would wipe the DB
    underneath it."""
    yield


@pytest.fixture
def user(client):
    email = f"define-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


def _plant_tracker_body(user_id, **overrides):
    body = {
        "user_id": user_id,
        "display_name": "Plant Tracker",
        "description": "Tracks a houseplant",
        "dimensions": [
            {"name": "moisture_level", "type": "number", "description": "soil moisture", "default_value": 50, "minimum": 0},
        ],
        "invariants": [
            {"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": "never negative"},
        ],
        "operator_names": ["edit.state_patch"],
    }
    body.update(overrides)
    return body


# ── Auth guard ────────────────────────────────────────────────────────────────

class TestAuthGuard:
    def test_validate_invariant_no_auth_returns_401(self, client):
        r = client.post("/devui/validate-invariant", json={"expression": "x >= 0", "state_schema": {}})
        assert r.status_code == 401

    def test_list_definitions_no_auth_returns_401(self, client):
        r = client.get("/devui/definitions", params={"user_id": "x"})
        assert r.status_code == 401

    def test_create_definition_no_auth_returns_401(self, client, user):
        r = client.post("/devui/definitions", json=_plant_tracker_body("x"))
        # deliberately no auth header
        r = client.post("/devui/definitions", json=_plant_tracker_body("x"), headers={})
        assert r.status_code == 401


# ── POST /devui/validate-invariant ────────────────────────────────────────────

class TestValidateInvariant:
    def test_valid_expression(self, client, user):
        headers, _ = user
        r = client.post(
            "/devui/validate-invariant",
            json={"expression": "moisture_level >= 0", "state_schema": {"moisture_level": {"type": "number"}}},
            headers=headers,
        )
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["valid"] is True
        assert data["errors"] == []

    def test_expression_referencing_undeclared_field(self, client, user):
        headers, _ = user
        r = client.post(
            "/devui/validate-invariant",
            json={"expression": "not_a_field >= 0", "state_schema": {"moisture_level": {"type": "number"}}},
            headers=headers,
        )
        data = r.json()["data"]
        assert data["valid"] is False
        assert data["errors"]

    def test_syntactically_invalid_expression(self, client, user):
        headers, _ = user
        r = client.post(
            "/devui/validate-invariant",
            json={"expression": "moisture_level >=", "state_schema": {"moisture_level": {"type": "number"}}},
            headers=headers,
        )
        assert r.json()["data"]["valid"] is False


# ── POST /devui/definitions ────────────────────────────────────────────────────

class TestCreateDefinitionRoute:
    def test_create_returns_200_with_template_id(self, client, user):
        headers, user_id = user
        r = client.post("/devui/definitions", json=_plant_tracker_body(user_id), headers=headers)
        assert r.status_code == 200
        data = r.json()["data"]
        assert data["template_id"]
        assert data["spec"]["display_name"] == "Plant Tracker"

    def test_invalid_invariant_returns_422(self, client, user):
        headers, user_id = user
        r = client.post(
            "/devui/definitions",
            json=_plant_tracker_body(user_id, invariants=[{"id": "bad", "expression": "not_a_field >= 0", "description": ""}]),
            headers=headers,
        )
        assert r.status_code == 422

    def test_unknown_operator_returns_422(self, client, user):
        headers, user_id = user
        r = client.post(
            "/devui/definitions",
            json=_plant_tracker_body(user_id, operator_names=["not.a.real.operator"]),
            headers=headers,
        )
        assert r.status_code == 422


# ── GET /devui/definitions, GET /devui/definitions/{id} ──────────────────────

class TestReadDefinitionRoutes:
    def test_list_scoped_to_requesting_user(self, client, user):
        headers, user_id = user
        client.post("/devui/definitions", json=_plant_tracker_body(user_id), headers=headers)

        other_headers, other_id = _register_another_user(client)
        client.post("/devui/definitions", json=_plant_tracker_body(other_id, display_name="Other's"), headers=other_headers)

        r = client.get("/devui/definitions", params={"user_id": user_id}, headers=headers)
        names = [d["display_name"] for d in r.json()["data"]["definitions"]]
        assert "Plant Tracker" in names
        assert "Other's" not in names

    def test_get_single_definition(self, client, user):
        headers, user_id = user
        created = client.post("/devui/definitions", json=_plant_tracker_body(user_id), headers=headers).json()["data"]
        r = client.get(f"/devui/definitions/{created['template_id']}", headers=headers)
        assert r.status_code == 200
        assert r.json()["data"]["spec"]["display_name"] == "Plant Tracker"

    def test_get_unknown_definition_returns_404(self, client, user):
        headers, _ = user
        r = client.get("/devui/definitions/does-not-exist", headers=headers)
        assert r.status_code == 404


def _register_another_user(client):
    email = f"define-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


# ── PATCH /devui/definitions/{id} ─────────────────────────────────────────────

class TestUpdateDefinitionRoute:
    def test_edit_own_definition_succeeds(self, client, user):
        headers, user_id = user
        created = client.post("/devui/definitions", json=_plant_tracker_body(user_id), headers=headers).json()["data"]
        r = client.patch(
            f"/devui/definitions/{created['template_id']}",
            json={"user_id": user_id, "description": "updated"},
            headers=headers,
        )
        assert r.status_code == 200
        assert r.json()["data"]["status"] == "ok"
        assert r.json()["data"]["spec"]["description"] == "updated"

    def test_edit_by_non_owner_returns_422(self, client, user):
        headers, user_id = user
        created = client.post("/devui/definitions", json=_plant_tracker_body(user_id), headers=headers).json()["data"]

        other_headers, other_id = _register_another_user(client)
        r = client.patch(
            f"/devui/definitions/{created['template_id']}",
            json={"user_id": other_id, "description": "hijacked"},
            headers=other_headers,
        )
        assert r.status_code == 422

    def test_edit_that_would_strand_a_live_instance_is_refused_not_error(self, client, user):
        headers, user_id = user
        created = client.post(
            "/devui/definitions",
            json=_plant_tracker_body(user_id, invariants=[]),
            headers=headers,
        ).json()["data"]
        tid = created["template_id"]

        inst = client.post("/devui/sustains", json={"template_id": tid, "user_id": user_id, "parameters": {}}, headers=headers)
        sid = inst.json()["data"]["sustain_id"]

        client.post(
            "/devui/console/execute",
            json={"sustain_id": sid, "operator_id": "edit.state_patch",
                  "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": -5}]}},
            headers=headers,
        )

        r = client.patch(
            f"/devui/definitions/{tid}",
            json={"user_id": user_id, "invariants": [
                {"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": ""},
            ]},
            headers=headers,
        )
        assert r.status_code == 200  # refusal is a normal response, not an HTTP error
        data = r.json()["data"]
        assert data["status"] == "refused"
        assert data["blocked_by"][0]["sustain_id"] == sid

    def test_gate_enforces_edited_invariant_immediately_via_console(self, client, user):
        headers, user_id = user
        created = client.post(
            "/devui/definitions",
            json=_plant_tracker_body(user_id, invariants=[]),
            headers=headers,
        ).json()["data"]
        tid = created["template_id"]
        inst = client.post("/devui/sustains", json={"template_id": tid, "user_id": user_id, "parameters": {}}, headers=headers)
        sid = inst.json()["data"]["sustain_id"]

        client.patch(
            f"/devui/definitions/{tid}",
            json={"user_id": user_id, "invariants": [
                {"id": "moisture_non_negative", "expression": "moisture_level >= 0", "description": ""},
            ]},
            headers=headers,
        )

        r = client.post(
            "/devui/console/execute",
            json={"sustain_id": sid, "operator_id": "edit.state_patch",
                  "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": -1}]}},
            headers=headers,
        )
        result = r.json()["data"]["result"]
        assert result["status"] == "failed"
        assert "moisture_non_negative" in result["reason"]


# ── GET /devui/templates merges user-created definitions ────────────────────

class TestTemplatesMerge:
    def test_created_definition_appears_in_templates_list(self, client, user):
        headers, user_id = user
        created = client.post("/devui/definitions", json=_plant_tracker_body(user_id), headers=headers).json()["data"]

        r = client.get("/devui/templates", headers=headers)
        ids = [t["template_id"] for t in r.json()["data"]["templates"]]
        assert created["template_id"] in ids

    def test_another_users_definition_does_not_appear(self, client, user):
        headers, user_id = user
        other_headers, other_id = _register_another_user(client)
        created = client.post("/devui/definitions", json=_plant_tracker_body(other_id), headers=other_headers).json()["data"]

        r = client.get("/devui/templates", headers=headers)
        ids = [t["template_id"] for t in r.json()["data"]["templates"]]
        assert created["template_id"] not in ids

    def test_builtin_templates_still_present(self, client, user):
        headers, _ = user
        r = client.get("/devui/templates", headers=headers)
        ids = [t["template_id"] for t in r.json()["data"]["templates"]]
        assert "homestead" in ids
        assert "habitat" in ids

    def test_instantiate_a_definition_via_the_normal_create_sustain_route(self, client, user):
        headers, user_id = user
        created = client.post("/devui/definitions", json=_plant_tracker_body(user_id), headers=headers).json()["data"]

        r = client.post(
            "/devui/sustains",
            json={"template_id": created["template_id"], "user_id": user_id, "parameters": {}},
            headers=headers,
        )
        assert r.status_code == 200
        sid = r.json()["data"]["sustain_id"]

        state_r = client.get(f"/devui/state?sustain_id={sid}", headers=headers)
        assert state_r.json()["data"]["state"] == {"moisture_level": 50}
