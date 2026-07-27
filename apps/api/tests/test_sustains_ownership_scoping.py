"""
tests/test_sustains_ownership_scoping.py

GET /devui/sustains previously returned every sustain in the DB regardless
of who was logged in -- any authenticated user's picker showed every other
account's (and every throwaway test account's) sustains alongside their
own. Found live during the Orchie slice's own acceptance check: a
newly-built page's "auto-pick a sustain" fallback grabbed a stray sustain
belonging to a different account entirely. Fixed by threading
owner_user_id through SustainEngine.list_all() into this route.

Run with:
    python -m pytest tests/test_sustains_ownership_scoping.py -v
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


def _register(client):
    email = f"ownership-scope-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


class TestSustainPickerScoping:
    def test_each_user_only_sees_their_own_sustains(self, client):
        headers_a, user_a = _register(client)
        headers_b, user_b = _register(client)

        r_a = client.post("/devui/sustains", json={"template_id": "homestead", "user_id": user_a, "parameters": {}}, headers=headers_a)
        sid_a = r_a.json()["data"]["sustain_id"]
        r_b = client.post("/devui/sustains", json={"template_id": "homestead", "user_id": user_b, "parameters": {}}, headers=headers_b)
        sid_b = r_b.json()["data"]["sustain_id"]

        list_a = client.get("/devui/sustains", headers=headers_a).json()["data"]["sustains"]
        list_b = client.get("/devui/sustains", headers=headers_b).json()["data"]["sustains"]

        ids_a = {s["id"] for s in list_a}
        ids_b = {s["id"] for s in list_b}

        assert sid_a in ids_a
        assert sid_b not in ids_a  # user A never sees user B's household
        assert sid_b in ids_b
        assert sid_a not in ids_b  # and vice versa

    def test_a_user_with_no_sustains_gets_an_honest_empty_list(self, client):
        headers, _ = _register(client)
        result = client.get("/devui/sustains", headers=headers).json()["data"]["sustains"]
        assert result == []
