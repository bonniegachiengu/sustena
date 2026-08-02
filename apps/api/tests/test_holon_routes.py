"""
tests/test_holon_routes.py

Phase 2 (Nested Holons, 2 Aug 2026) over HTTP: holon.create_child/
holon.transfer/holon.dissolve_child invoked through the real, unmodified
POST /orchie/capture/confirm route (no new execute route needed — see
holon.py's own design note), plus the new GET /devui/sustain/{id}/parent
route. Same module-scoped TestClient pattern as test_orchie_capture_routes.py.

Ends with the full acceptance scenario from the brief, run over real HTTP
with a real authenticated account: create Project IO -> fund from
homestead -> give it a pocket -> roll-up includes it -> navigate via
parent/children routes -> dissolve returns the funds.
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
    # Overrides conftest.py's real per-function engine reset (same name
    # shadows it for this module) -- a module-scoped TestClient triggers
    # lifespan/init_db() ONCE; resetting the SQLAlchemy engine again before
    # each test function would orphan that already-populated :memory: db
    # and lazily create a fresh, empty one on the next query. Same pattern
    # test_orchie_capture_routes.py already establishes for the identical
    # module-scoped-client shape.
    yield


@pytest.fixture
def user(client):
    email = f"holon-route-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


@pytest.fixture
def homestead(client, user):
    headers, user_id = user
    r = client.post(
        "/devui/sustains", json={"template_id": "homestead", "user_id": user_id, "parameters": {}}, headers=headers,
    )
    assert r.status_code == 200, r.text
    return r.json()["data"]["sustain_id"]


def _confirm(client, headers, sustain_id, operator, params):
    return client.post(
        "/orchie/capture/confirm",
        json={"sustain_id": sustain_id, "operator": operator, "params": params},
        headers=headers,
    )


class TestGetParentRoute:
    def test_no_auth_returns_401(self, client, homestead):
        r = client.get(f"/devui/sustain/{homestead}/parent")
        assert r.status_code == 401

    def test_no_parent_returns_null(self, client, user, homestead):
        headers, _ = user
        r = client.get(f"/devui/sustain/{homestead}/parent", headers=headers)
        assert r.status_code == 200
        assert r.json()["data"]["parent"] is None

    def test_a_real_linked_child_reports_its_parent_with_display_name(self, client, user, homestead):
        headers, _ = user
        r = _confirm(client, headers, homestead, "holon.create_child", {"template": "habitat", "name": "Project IO"})
        assert r.status_code == 200, r.text
        assert r.json()["result"]["status"] == "ok", r.json()
        child_id = r.json()["result"]["data"]["child_sustain_id"]

        r2 = client.get(f"/devui/sustain/{child_id}/parent", headers=headers)
        assert r2.status_code == 200
        parent = r2.json()["data"]["parent"]
        assert parent["parent_sustain_id"] == homestead
        assert parent["display_name"] == "Homestead"


class TestHolonOperatorsOverHttp:
    def test_create_child_via_capture_confirm(self, client, user, homestead):
        headers, _ = user
        r = _confirm(client, headers, homestead, "holon.create_child", {"template": "habitat", "name": "Project IO"})
        assert r.status_code == 200, r.text
        assert r.json()["result"]["status"] == "ok"

    def test_create_child_disallowed_template_refused_as_normal_200(self, client, user, homestead):
        headers, _ = user
        r = _confirm(client, headers, homestead, "holon.create_child", {"template": "homestead", "name": "Nested"})
        assert r.status_code == 200
        assert r.json()["result"]["status"] == "failed"
        assert r.json()["result"]["constraint_violated"] == "child_policy_accepts"

    def test_transfer_conserves_total_over_http(self, client, user, homestead):
        headers, _ = user
        r = _confirm(client, headers, homestead, "holon.create_child", {"template": "habitat", "name": "Project IO"})
        child_id = r.json()["result"]["data"]["child_sustain_id"]
        _confirm(client, headers, homestead, "budget.record_income", {"amount": 5000, "source": "salary", "frequency": "once"})

        state_before_parent = client.get(f"/devui/state?sustain_id={homestead}", headers=headers).json()["data"]["state"]
        state_before_child = client.get(f"/devui/state?sustain_id={child_id}", headers=headers).json()["data"]["state"]
        total_before = state_before_parent["finances"]["liquid"]["balance"] + state_before_child["finances"]["liquid"]["balance"]

        r2 = _confirm(client, headers, homestead, "holon.transfer", {"to_sustain_id": child_id, "amount": 2000})
        assert r2.status_code == 200
        assert r2.json()["result"]["status"] == "ok", r2.json()

        state_after_parent = client.get(f"/devui/state?sustain_id={homestead}", headers=headers).json()["data"]["state"]
        state_after_child = client.get(f"/devui/state?sustain_id={child_id}", headers=headers).json()["data"]["state"]
        total_after = state_after_parent["finances"]["liquid"]["balance"] + state_after_child["finances"]["liquid"]["balance"]

        assert total_before == total_after
        assert state_after_child["finances"]["liquid"]["balance"] == 2000

    def test_transfer_to_an_unlinked_sustain_is_refused_not_500(self, client, user, homestead):
        headers, _ = user
        r = _confirm(client, headers, homestead, "holon.transfer", {"to_sustain_id": "not-a-real-sustain-id", "amount": 10})
        assert r.status_code == 200
        assert r.json()["result"]["status"] == "failed"
        assert r.json()["result"]["constraint_violated"] == "holon_link_exists"

    def test_dissolve_child_returns_funds_over_http(self, client, user, homestead):
        headers, _ = user
        r = _confirm(client, headers, homestead, "holon.create_child", {"template": "habitat", "name": "Project IO"})
        child_id = r.json()["result"]["data"]["child_sustain_id"]
        _confirm(client, headers, homestead, "budget.record_income", {"amount": 3000, "source": "salary", "frequency": "once"})
        _confirm(client, headers, homestead, "holon.transfer", {"to_sustain_id": child_id, "amount": 1200})

        r2 = _confirm(client, headers, homestead, "holon.dissolve_child", {"child_sustain_id": child_id})
        assert r2.status_code == 200, r2.text
        assert r2.json()["result"]["status"] == "ok"
        assert r2.json()["result"]["data"]["funds_returned"] == 1200

        state_after = client.get(f"/devui/state?sustain_id={homestead}", headers=headers).json()["data"]["state"]
        assert state_after["finances"]["liquid"]["balance"] == 3000  # fully returned

        parent_of_child = client.get(f"/devui/sustain/{child_id}/parent", headers=headers)
        assert parent_of_child.json()["data"]["parent"] is None


class TestFullAcceptanceScenario:
    """create Project IO -> fund from homestead -> give it pockets -> roll-up
    -> navigate in/out -> dissolve. Mirrors the brief's own acceptance
    scenario, over real HTTP with a real authenticated account."""

    def test_end_to_end(self, client, user, homestead):
        headers, _ = user

        # 1. Fund the homestead first so there's something to move.
        r = _confirm(client, headers, homestead, "budget.record_income", {"amount": 20000, "source": "salary", "frequency": "once"})
        assert r.json()["result"]["status"] == "ok"

        # 2. Create Project IO.
        r = _confirm(client, headers, homestead, "holon.create_child", {"template": "habitat", "name": "Project IO"})
        assert r.status_code == 200, r.text
        project_id = r.json()["result"]["data"]["child_sustain_id"]

        # 3. Fund it from the homestead.
        r = _confirm(client, headers, homestead, "holon.transfer", {"to_sustain_id": project_id, "amount": 6000})
        assert r.json()["result"]["status"] == "ok"

        # 4. Give it pockets.
        r = _confirm(client, headers, project_id, "budget.allocate", {"pocket_name": "airtime", "amount": 1000, "period": "monthly"})
        assert r.json()["result"]["status"] == "ok"
        r = _confirm(client, headers, project_id, "budget.allocate", {"pocket_name": "wifi", "amount": 2000, "period": "monthly"})
        assert r.json()["result"]["status"] == "ok"

        # 5. Roll-up includes it -- both the liquid total and the pockets total.
        rollup = client.get(f"/devui/sustain/{homestead}/rollup", headers=headers).json()["data"]
        liquid_agg = rollup["aggregates"]["household_liquid_total"]
        pockets_agg = rollup["aggregates"]["household_pockets_total"]
        assert project_id in [c["sustain_id"] for c in liquid_agg["included"]]
        assert liquid_agg["value"] == 3000  # 6000 funded - 3000 allocated to pockets
        assert pockets_agg["value"] == 3000  # 1000 + 2000

        # 6. Navigate: children list from the parent, parent link from the child.
        children = client.get(f"/devui/sustain/{homestead}/children", headers=headers).json()["data"]["children"]
        assert any(c["child_sustain_id"] == project_id for c in children)
        parent_link = client.get(f"/devui/sustain/{project_id}/parent", headers=headers).json()["data"]["parent"]
        assert parent_link["parent_sustain_id"] == homestead

        # 7. Dissolve -- funds return first (the full remaining liquid balance
        #    only; the pocket allocations themselves are NOT liquid and stay
        #    with the dissolved sustain's own history, which is preserved).
        homestead_before = client.get(f"/devui/state?sustain_id={homestead}", headers=headers).json()["data"]["state"]
        r = _confirm(client, headers, homestead, "holon.dissolve_child", {"child_sustain_id": project_id})
        assert r.json()["result"]["status"] == "ok", r.json()
        assert r.json()["result"]["data"]["funds_returned"] == 3000

        homestead_after = client.get(f"/devui/state?sustain_id={homestead}", headers=headers).json()["data"]["state"]
        assert (
            homestead_after["finances"]["liquid"]["balance"]
            == homestead_before["finances"]["liquid"]["balance"] + 3000
        )

        # Project IO no longer appears as a linked child, but its own state
        # (including its pocket history) is untouched -- verified directly.
        children_after = client.get(f"/devui/sustain/{homestead}/children", headers=headers).json()["data"]["children"]
        assert not any(c["child_sustain_id"] == project_id for c in children_after)
        project_state = client.get(f"/devui/state?sustain_id={project_id}", headers=headers).json()["data"]["state"]
        assert project_state["finances"]["pockets"]["airtime"]["allocated"] == 1000
        assert project_state["finances"]["pockets"]["wifi"]["allocated"] == 2000
