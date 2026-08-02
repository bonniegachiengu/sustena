"""
tests/test_parse_rule_routes.py

Phase 3C (Parser primitive-lift, 2 Aug 2026) over HTTP:
  GET  /devui/parse-rules?source=
  GET  /devui/parse-rules/{id}/history
  POST /devui/parse-rules
  POST /devui/parse-rules/{id}/modify
  POST /devui/parse-rules/{id}/retire

Same module-scoped TestClient pattern as test_composition_routes.py.
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
    email = f"parse-rule-route-tests-{uuid.uuid4().hex[:12]}@example.com"
    r = client.post("/api/v1/users/register", json={"email": email, "password": "test-password-123"})
    assert r.status_code == 200, r.text
    data = r.json()["data"]
    return {"Authorization": f"Bearer {data['token']}"}, data["user_id"]


def _rule_body(rule_id="http_test_rule", pattern=r"HTTPTEST (?P<amount>\d+)"):
    return {
        "id": rule_id, "source": "mpesa", "pattern": pattern,
        "extract": {"amount": {"type": "amount", "group": "amount"}},
        "status": "informational",
        "examples": ["HTTPTEST 500"],
    }


class TestAuthGuard:
    def test_list_requires_auth(self, client):
        r = client.get("/devui/parse-rules?source=mpesa")
        assert r.status_code == 401

    def test_add_requires_auth(self, client):
        r = client.post("/devui/parse-rules", json=_rule_body())
        assert r.status_code == 401


class TestListParseRules:
    def test_lists_the_seed_library_for_mpesa(self, client, user):
        headers, _ = user
        r = client.get("/devui/parse-rules?source=mpesa", headers=headers)
        assert r.status_code == 200
        rule_ids = [rule["id"] for rule in r.json()["data"]["rules"]]
        assert "mpesa_received" in rule_ids
        assert "mpesa_paybill" in rule_ids

    def test_kcb_has_fifteen_declared_rules(self, client, user):
        # KCB was migrated to declared ParseRule data 2 Aug 2026 (the
        # "broaden parser coverage" round) -- this test used to assert the
        # opposite (kcb has zero declared rules by design) when that was
        # the real, disclosed scope boundary. Corrected in place now that
        # the boundary has moved, not reverted around.
        headers, _ = user
        r = client.get("/devui/parse-rules?source=kcb", headers=headers)
        assert r.status_code == 200
        rule_ids = [rule["id"] for rule in r.json()["data"]["rules"]]
        assert len(rule_ids) == 15
        assert "kcb_receive" in rule_ids
        assert "kcb_mpesa_paybill" in rule_ids


class TestAddModifyRetireOverHttp:
    def test_full_lifecycle(self, client, user):
        headers, _ = user
        rule_id = f"http_test_rule_{uuid.uuid4().hex[:8]}"
        body = _rule_body(rule_id=rule_id)

        r = client.post("/devui/parse-rules", json=body, headers=headers)
        assert r.status_code == 200, r.text
        assert r.json()["data"]["status"] == "ok"
        assert r.json()["data"]["version"] == 1

        r2 = client.get("/devui/parse-rules?source=mpesa", headers=headers)
        rules = {rule["id"]: rule for rule in r2.json()["data"]["rules"]}
        assert rule_id in rules
        assert rules[rule_id]["is_correction"] is True

        modified = _rule_body(rule_id=rule_id, pattern=r"HTTPTEST\s+(?P<amount>\d+)")
        r3 = client.post(f"/devui/parse-rules/{rule_id}/modify", json=modified, headers=headers)
        assert r3.status_code == 200, r3.text
        assert r3.json()["data"]["status"] == "ok"
        assert r3.json()["data"]["version"] == 2

        history = client.get(f"/devui/parse-rules/{rule_id}/history", headers=headers).json()["data"]["history"]
        assert len(history) == 2
        assert history[0]["status"] == "superseded"
        assert history[1]["status"] == "active"

        r4 = client.post(f"/devui/parse-rules/{rule_id}/retire", headers=headers)
        assert r4.status_code == 200
        assert r4.json()["data"]["status"] == "ok"

        r5 = client.get("/devui/parse-rules?source=mpesa", headers=headers)
        rule_ids_after = [rule["id"] for rule in r5.json()["data"]["rules"]]
        assert rule_id not in rule_ids_after

    def test_add_with_invalid_regex_is_refused_as_normal_200(self, client, user):
        headers, _ = user
        body = _rule_body(rule_id=f"bad_{uuid.uuid4().hex[:8]}", pattern="(unclosed")
        r = client.post("/devui/parse-rules", json=body, headers=headers)
        assert r.status_code == 200  # a typecheck refusal is a normal body, not an HTTP error
        assert r.json()["data"]["status"] == "failed"

    def test_a_regressing_modify_is_refused_over_http(self, client, user):
        headers, _ = user
        rule_id = f"regress_test_{uuid.uuid4().hex[:8]}"
        client.post("/devui/parse-rules", json=_rule_body(rule_id=rule_id), headers=headers)

        broken = _rule_body(rule_id=rule_id, pattern="THIS_NEVER_MATCHES_HTTPTEST_500")
        r = client.post(f"/devui/parse-rules/{rule_id}/modify", json=broken, headers=headers)
        assert r.status_code == 200
        assert r.json()["data"]["status"] == "failed"
        assert "regressions" in r.json()["data"]


class TestProposeRoute:
    def test_propose_route_is_honest_about_no_generator_wired(self, client, user):
        headers, _ = user
        r = client.post(
            "/devui/parse-rules/propose",
            json={"source": "mpesa", "raw_text": "some genuinely novel unparsed text nobody has seen"},
            headers=headers,
        )
        assert r.status_code == 200
        assert r.json()["data"]["proposed"] is False
        assert r.json()["data"]["reason"] == "no_proposer_configured"

    def test_propose_route_requires_auth(self, client):
        r = client.post("/devui/parse-rules/propose", json={"source": "mpesa", "raw_text": "x"})
        assert r.status_code == 401
