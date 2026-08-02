"""
tests/test_orchie_balance_provenance.py

Tests for GET /orchie/balance-provenance (sustena/api/routes/orchie.py) --
Bonnie, 2 Aug 2026: "I still can't tell where the balance number came
from... I want PROVENANCE." Every event that has EVER changed a sustain's
own liquid balance, oldest first, summing to the displayed number, plus a
real consistency check (computed sum vs. the actual live balance).

Run with:
    python -m pytest tests/test_orchie_balance_provenance.py -v
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
    email = f"orchie-provenance-tests-{uuid.uuid4().hex[:12]}@example.com"
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


class TestAuthAndOwnership:
    def test_no_auth_returns_401(self, client, sustain):
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}")
        assert r.status_code == 401

    def test_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.get("/orchie/balance-provenance?sustain_id=does-not-exist", headers=headers)
        assert r.status_code == 404

    def test_scoped_to_the_owning_users_own_sustain_only(self, client, user):
        headers_a, user_id_a = user
        email_b = f"orchie-provenance-other-{uuid.uuid4().hex[:10]}@example.com"
        r = client.post("/api/v1/users/register", json={"email": email_b, "password": "test-password-123"})
        headers_b = {"Authorization": f"Bearer {r.json()['data']['token']}"}
        r = client.post("/devui/sustains", json={"template_id": "homestead", "user_id": user_id_a, "parameters": {}}, headers=headers_a)
        sustain_a = r.json()["data"]["sustain_id"]
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain_a}", headers=headers_b)
        assert r.status_code == 404


class TestOwnProvenance:
    def test_empty_sustain_has_no_entries_and_is_consistent(self, client, user, sustain):
        headers, _ = user
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["own"]["entries"] == []
        assert data["own"]["starting_balance"] == 0.0
        assert data["own"]["computed_total"] == 0.0
        assert data["own"]["live_balance"] == 0.0
        assert data["own"]["consistent"] is True

    def test_a_single_income_event_appears_and_sums_correctly(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 2000, "source": "Salary", "frequency": "once"}},
            headers=headers,
        )
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        data = r.json()["own"]
        assert len(data["entries"]) == 1
        entry = data["entries"][0]
        assert entry["event_name"] == "event.finances.income_received"
        assert entry["amount"] == 2000
        assert entry["running_balance"] == 2000
        assert data["computed_total"] == 2000
        assert data["live_balance"] == 2000
        assert data["consistent"] is True

    def test_income_and_allocate_both_contribute_with_correct_sign(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 5000, "source": "Salary", "frequency": "once"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 2000, "period": "monthly"}},
            headers=headers,
        )
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        data = r.json()["own"]
        assert len(data["entries"]) == 2
        assert data["entries"][0]["amount"] == 5000       # income: +
        assert data["entries"][1]["amount"] == -2000       # allocate: -
        assert data["entries"][1]["running_balance"] == 3000
        assert data["computed_total"] == 3000
        assert data["live_balance"] == 3000
        assert data["consistent"] is True

    def test_spend_and_transfer_never_appear_in_liquid_provenance(self, client, user, sustain):
        # spend/transfer move money between/within pockets -- neither ever
        # touches finances.liquid.balance (verified by grep across the
        # whole codebase, not assumed) -- so neither belongs in this list.
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 5000, "source": "seed", "frequency": "once"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 2000, "period": "monthly"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "rent", "amount": 1000, "period": "monthly"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 500, "description": "x", "category": "test"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.transfer", "params": {"from_pocket": "food", "to_pocket": "rent", "amount": 300}},
            headers=headers,
        )
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        data = r.json()["own"]
        event_names = {e["event_name"] for e in data["entries"]}
        assert event_names == {"event.finances.income_received", "event.finances.pocket_allocated"}
        assert len(data["entries"]) == 3  # 1 income + 2 allocate; spend/transfer excluded
        assert data["live_balance"] == 2000  # 5000 - 2000 - 1000
        assert data["consistent"] is True

    def test_raw_message_link_present_when_classification_came_from_a_capture(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/api/v1/ingest/capture",
            json={
                "source_id": "mpesa", "sustain_id": sustain,
                "raw_payload": "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU 254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00",
            },
            headers=headers,
        )
        assert r.json()["data"]["status"] == "applied"  # mapped straight to budget.record_income

        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        entry = r.json()["own"]["entries"][0]
        assert entry["source"] == "mpesa"
        assert "JOHN KAMAU" in entry["raw_text"]

    def test_manually_entered_income_has_no_raw_message_honestly(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 500, "source": "Salary", "frequency": "once"}},
            headers=headers,
        )
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        entry = r.json()["own"]["entries"][0]
        assert entry["source"] is None
        assert entry["raw_text"] is None
        assert entry["message_id"] is None

    def test_entries_are_oldest_first_matching_running_balance_narrative(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 1000, "source": "A", "frequency": "once"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 2000, "source": "B", "frequency": "once"}},
            headers=headers,
        )
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        entries = r.json()["own"]["entries"]
        assert [e["description"] for e in entries] == ["A", "B"]
        assert [e["running_balance"] for e in entries] == [1000, 3000]

    def test_provenance_is_read_only(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 1000, "source": "A", "frequency": "once"}},
            headers=headers,
        )
        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_before == state_after


class TestHouseholdProvenance:
    def test_no_linked_children_gives_honest_empty_household(self, client, user, sustain):
        headers, _ = user
        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        data = r.json()
        assert data["children"] == []
        assert data["household_total_computed"] == 0.0
        assert data["household_consistent"] is True

    def test_linked_child_contributes_its_own_provenance(self, client, user, sustain):
        headers, user_id = user
        r = client.post(
            f"/devui/sustain/{sustain}/provision-children", json={"user_id": user_id}, headers=headers,
        )
        assert r.status_code == 200, r.text

        children = client.get(f"/devui/sustain/{sustain}/children", headers=headers).json()["data"]["children"]
        assert len(children) == 6  # homestead.json declares 6 habitat slots
        first_child = children[0]["child_sustain_id"]

        client.post(
            "/devui/console/execute",
            json={"sustain_id": first_child, "operator": "budget.record_income", "params": {"amount": 700, "source": "pocket money", "frequency": "once"}},
            headers=headers,
        )

        r = client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        data = r.json()
        assert len(data["children"]) == 6
        funded = next(c for c in data["children"] if c["sustain_id"] == first_child)
        assert funded["live_balance"] == 700
        assert funded["consistent"] is True
        unfunded = [c for c in data["children"] if c["sustain_id"] != first_child]
        assert all(c["live_balance"] == 0.0 and c["entries"] == [] for c in unfunded)

        assert data["household_total_computed"] == 700
        assert data["household_total_via_rollup_engine"] == 700
        assert data["household_consistent"] is True

    def test_household_provenance_is_read_only(self, client, user, sustain):
        headers, user_id = user
        client.post(f"/devui/sustain/{sustain}/provision-children", json={"user_id": user_id}, headers=headers)
        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        client.get(f"/orchie/balance-provenance?sustain_id={sustain}", headers=headers)
        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_before == state_after
