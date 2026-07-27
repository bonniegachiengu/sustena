"""
tests/test_composition_routes.py

Tests for the Slice 8 (Coordination & roll-up) devui.py routes:
  GET    /devui/sustain/{id}/children
  POST   /devui/sustain/{id}/children
  DELETE /devui/sustain/{id}/children/{child_id}
  POST   /devui/sustain/{id}/provision-children
  GET    /devui/sustain/{id}/rollup
  GET    /devui/state — carries a "rollup" key

Follows the same module-scoped TestClient + per-test user registration
pattern as test_devui_routes.py / test_definition_routes.py.
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
    email = f"composition-tests-{uuid.uuid4().hex[:12]}@example.com"
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
def parent_and_child(client, user):
    headers, user_id = user
    parent_def = _create_definition(client, headers, user_id, display_name="Pod")
    child_def = _create_definition(client, headers, user_id, display_name="Leaf")
    parent = _instantiate(client, headers, parent_def["template_id"], user_id)
    child = _instantiate(client, headers, child_def["template_id"], user_id)
    return headers, user_id, parent, child


# ── POST/GET/DELETE children ──────────────────────────────────────────────────

class TestChildrenRoutes:
    def test_no_auth_returns_401(self, client):
        assert client.get("/devui/sustain/x/children").status_code == 401
        assert client.post("/devui/sustain/x/children", json={"child_sustain_id": "y"}).status_code == 401

    def test_link_then_list(self, client, parent_and_child):
        headers, _, parent, child = parent_and_child
        r = client.post(
            f"/devui/sustain/{parent}/children",
            json={"child_sustain_id": child, "slot": "c1", "member": "Leaf One"},
            headers=headers,
        )
        assert r.status_code == 200

        r2 = client.get(f"/devui/sustain/{parent}/children", headers=headers)
        children = r2.json()["data"]["children"]
        assert len(children) == 1
        assert children[0]["child_sustain_id"] == child
        assert children[0]["display_name"] == "Leaf"

    def test_link_unknown_child_returns_422(self, client, parent_and_child):
        headers, _, parent, _ = parent_and_child
        r = client.post(
            f"/devui/sustain/{parent}/children",
            json={"child_sustain_id": "does-not-exist"},
            headers=headers,
        )
        assert r.status_code == 422

    def test_unlink_removes_link(self, client, parent_and_child):
        headers, _, parent, child = parent_and_child
        client.post(f"/devui/sustain/{parent}/children", json={"child_sustain_id": child}, headers=headers)
        r = client.delete(f"/devui/sustain/{parent}/children/{child}", headers=headers)
        assert r.status_code == 200
        assert client.get(f"/devui/sustain/{parent}/children", headers=headers).json()["data"]["children"] == []

    def test_unlink_unknown_link_returns_404(self, client, parent_and_child):
        headers, _, parent, child = parent_and_child
        r = client.delete(f"/devui/sustain/{parent}/children/{child}", headers=headers)
        assert r.status_code == 404


# ── POST provision-children ───────────────────────────────────────────────────

class TestProvisionChildrenRoute:
    def test_provisions_declared_children(self, client, user):
        headers, user_id = user
        child_def = _create_definition(client, headers, user_id, display_name="Leaf")
        parent_def = _create_definition(client, headers, user_id, display_name="Pod")

        definition = client.get(f"/devui/definitions/{parent_def['template_id']}", headers=headers).json()["data"]
        spec = definition["spec"]
        spec["declared_children"] = [
            {"slot": "s1", "member": "One", "template": child_def["template_id"]},
            {"slot": "s2", "member": "Two", "template": child_def["template_id"]},
        ]
        # Persist directly via update_definition's dimensions/invariants/operator_names
        # patch surface doesn't cover declared_children (a create/definition-flow
        # concept, not exposed for editing there) — write it straight to the
        # engine for this test, matching how the composition tests build fixtures.
        from sustena.core.engine_singleton import get_shared_engine
        import json as _json
        engine = get_shared_engine()
        engine._db.execute(
            "UPDATE sustain_templates SET spec_json = ? WHERE id = ?",
            (_json.dumps(spec), parent_def["template_id"]),
        )
        engine._db.commit()

        parent = _instantiate(client, headers, parent_def["template_id"], user_id)
        r = client.post(f"/devui/sustain/{parent}/provision-children", json={"user_id": user_id}, headers=headers)
        assert r.status_code == 200
        links = r.json()["data"]["children"]
        assert len(links) == 2

    def test_unknown_parent_returns_422(self, client, user):
        headers, user_id = user
        r = client.post("/devui/sustain/does-not-exist/provision-children", json={"user_id": user_id}, headers=headers)
        assert r.status_code == 422


# ── GET rollup + /devui/state carrying rollup ────────────────────────────────

class TestRollupRoute:
    def test_rollup_on_a_non_parent_returns_empty_aggregates(self, client, parent_and_child):
        headers, _, parent, _ = parent_and_child
        r = client.get(f"/devui/sustain/{parent}/rollup", headers=headers)
        assert r.status_code == 200
        assert r.json()["data"]["aggregates"] == {}

    def test_rollup_unknown_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.get("/devui/sustain/does-not-exist/rollup", headers=headers)
        assert r.status_code == 404

    def test_devui_state_rollup_key_is_null_for_non_parent(self, client, parent_and_child):
        headers, _, parent, _ = parent_and_child
        r = client.get(f"/devui/state?sustain_id={parent}", headers=headers)
        assert r.json()["data"]["rollup"] is None

    def test_devui_state_rollup_reflects_real_aggregate(self, client, user):
        headers, user_id = user
        child_def = _create_definition(client, headers, user_id, display_name="Leaf")
        parent_def = _create_definition(client, headers, user_id, display_name="Pod")

        from sustena.core.engine_singleton import get_shared_engine
        import json as _json
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
        client.post(
            "/devui/console/execute",
            json={"sustain_id": child, "operator_id": "edit.state_patch",
                  "params": {"patch": [{"op": "replace", "path": "moisture_level", "value": 33}]}},
            headers=headers,
        )

        r = client.get(f"/devui/state?sustain_id={parent}", headers=headers)
        rollup = r.json()["data"]["rollup"]
        assert rollup["aggregates"]["pod_total"]["value"] == 33
