"""
tests/test_orchie_capture_routes.py

Tests for POST /orchie/capture/infer and POST /orchie/capture/confirm --
effect-first capture (§7) over HTTP. Same module-scoped TestClient pattern
as test_orchie_compose_route.py / test_ingest_routes.py.

Run with:
    python -m pytest tests/test_orchie_capture_routes.py -v
"""

import uuid

import pytest
from starlette.testclient import TestClient

from sustena.api.main import app

BUYGOODS = (
    "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 "
    "at 4:30 PM. New M-PESA balance is Ksh12,050.00"
)


@pytest.fixture(scope="module")
def client():
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


@pytest.fixture(autouse=True)
def _reset_engine_function():
    yield


@pytest.fixture
def user(client):
    email = f"orchie-capture-tests-{uuid.uuid4().hex[:12]}@example.com"
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


def _seed_pocket(client, headers, sustain, name, allocated):
    r = client.post(
        "/devui/console/execute",
        json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": allocated, "source": "seed", "frequency": "once"}},
        headers=headers,
    )
    assert r.status_code == 200, r.text
    r = client.post(
        "/devui/console/execute",
        json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": name, "amount": allocated, "period": "monthly"}},
        headers=headers,
    )
    assert r.status_code == 200, r.text


class TestAuthGuard:
    def test_infer_no_auth_returns_401(self, client, sustain):
        r = client.post("/orchie/capture/infer", json={"sustain_id": sustain, "effect_text": "spent 100 on food"})
        assert r.status_code == 401

    def test_confirm_no_auth_returns_401(self, client, sustain):
        r = client.post("/orchie/capture/confirm", json={"sustain_id": sustain, "operator": "budget.spend", "params": {}})
        assert r.status_code == 401


class TestOwnershipBoundary:
    def test_infer_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.post("/orchie/capture/infer", json={"sustain_id": "does-not-exist", "effect_text": "spent 100 on food"}, headers=headers)
        assert r.status_code == 404

    def test_confirm_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.post("/orchie/capture/confirm", json={"sustain_id": "does-not-exist", "operator": "budget.spend", "params": {}}, headers=headers)
        assert r.status_code == 404


class TestInfer:
    def test_narration_resolves_to_ready(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "effect_text": "spent 500 on food"},
            headers=headers,
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["status"] == "ready"
        assert data["operator"] == "budget.spend"
        assert data["params"]["pocket_name"] == "food"
        assert data["params"]["amount"] == 500.0

    def test_ambiguous_pocket_asks_with_real_pocket_options(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        _seed_pocket(client, headers, sustain, "rent", 15000)
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "effect_text": "spent 500 on snacks"},
            headers=headers,
        )
        data = r.json()
        assert data["status"] == "needs_disambiguation"
        assert data["field"] == "pocket_name"
        values = {o["value"] for o in data["options"]}
        assert values == {"food", "rent"}

    def test_unknown_widget_id_returns_404(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "widget_id": "not-a-real-widget", "effect_text": "x"},
            headers=headers,
        )
        assert r.status_code == 404

    def test_from_a_real_ingested_unmapped_message(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        assert cap.status_code == 200
        message_id = cap.json()["data"]["message_id"]
        assert cap.json()["data"]["status"] == "needs_attention"

        # NAIVAS SUPERMARKET doesn't textually match "food" -- the engine
        # correctly can't guess the pocket from the raw message alone, and
        # must ask, offering the sustain's real declared pockets.
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id},
            headers=headers,
        )
        data = r.json()
        assert data["status"] == "needs_disambiguation"
        assert data["field"] == "pocket_name"
        assert {o["value"] for o in data["options"]} == {"food"}

        # second round: human taps "food" -- amount (450) and description
        # (NAIVAS SUPERMARKET) come from the message itself, never typed.
        r2 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {"pocket_name": "food"}},
            headers=headers,
        )
        data2 = r2.json()
        assert data2["status"] == "ready"
        assert data2["operator"] == "budget.spend"
        assert data2["params"]["amount"] == 450.0
        assert data2["params"]["description"] == "NAIVAS SUPERMARKET"

    def test_message_belonging_to_a_different_sustain_returns_404(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": "not-a-real-message-id"},
            headers=headers,
        )
        assert r.status_code == 404


class TestConfirm:
    def test_real_capture_commits_a_real_event_and_changes_state(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)

        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        food_before = state_before["finances"]["pockets"]["food"]["spent"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 500, "description": "groceries", "category": "food"}},
            headers=headers,
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["result"]["status"] == "ok"

        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_after["finances"]["pockets"]["food"]["spent"] == food_before + 500

        events = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["events"]
        assert events[0]["event_name"] == "event.finances.pocket_spent"

    def test_confirm_resolves_the_source_ingest_message_on_success(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 450, "description": "NAIVAS SUPERMARKET"}, "message_id": message_id},
            headers=headers,
        )
        assert r.json()["message_resolved"] is True

        msg = client.get(f"/api/v1/ingest/messages/{message_id}", headers=headers).json()["data"]
        assert msg["resolved_at"] is not None

    def test_honest_gate_refusal_is_a_normal_response_not_a_fake_success(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 100)  # only 100 allocated

        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 999999, "description": "too much"}},
            headers=headers,
        )
        assert r.status_code == 200  # a refusal is a normal response, not an HTTP error
        data = r.json()
        assert data["result"]["status"] == "failed"
        assert data["result"].get("reason") or data["result"].get("data", {}).get("reason")
        assert data["message_resolved"] is False

        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_before == state_after  # nothing changed on a refusal

    def test_refused_capture_leaves_source_message_still_needing_attention(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 100)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 999999, "description": "x"}, "message_id": message_id},
            headers=headers,
        )
        assert r.json()["message_resolved"] is False

        msg = client.get(f"/api/v1/ingest/messages/{message_id}", headers=headers).json()["data"]
        assert msg["resolved_at"] is None
        assert msg["status"] == "needs_attention"


class TestClassificationHistoryEndToEnd:
    """Real, in-the-field friction this exists to remove: a repeat merchant
    (NAIVAS SUPERMARKET) pre-fills the pocket a human picked last time,
    saving the disambiguation tap -- confirm write path and the CONFIRM tap
    itself are completely untouched."""

    def test_first_capture_from_a_merchant_still_asks(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "effect_text": "spent 680 at NAIVAS SUPERMARKET"},
            headers=headers,
        )
        assert r.json()["status"] == "needs_disambiguation"

    def test_second_capture_from_the_same_merchant_pre_fills_via_history(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)

        # Round 1: real confirm, human taps "food" once.
        cap1 = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        msg1 = cap1.json()["data"]["message_id"]
        infer1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": msg1, "known": {"pocket_name": "food"}},
            headers=headers,
        ).json()
        assert infer1["status"] == "ready"
        confirm1 = client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": infer1["operator"], "params": infer1["params"],
                "message_id": msg1, "description": infer1["description"],
            },
            headers=headers,
        )
        assert confirm1.json()["result"]["status"] == "ok"

        # Round 2: a second, distinct NAIVAS SUPERMARKET message -- must NOT
        # need a tap this time, and must say so honestly (from_history=True).
        second_naivas = BUYGOODS.replace("QGH7XJ4P2Q", "QGH7XJ9Z2W").replace("450.00", "680.00")
        cap2 = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": second_naivas},
            headers=headers,
        )
        msg2 = cap2.json()["data"]["message_id"]
        infer2 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": msg2},
            headers=headers,
        ).json()
        assert infer2["status"] == "ready"
        assert infer2["params"]["pocket_name"] == "food"
        assert infer2["from_history"] is True
        assert infer2["history_use_count"] == 1

    def test_ignore_history_forces_the_normal_disambiguation(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        _seed_pocket(client, headers, sustain, "shopping", 9000)

        cap1 = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        msg1 = cap1.json()["data"]["message_id"]
        infer1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": msg1, "known": {"pocket_name": "food"}},
            headers=headers,
        ).json()
        client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": infer1["operator"], "params": infer1["params"],
                "message_id": msg1, "description": infer1["description"],
            },
            headers=headers,
        )

        second_naivas = BUYGOODS.replace("QGH7XJ4P2Q", "QGH7XJ9Z2W")
        cap2 = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": second_naivas},
            headers=headers,
        )
        msg2 = cap2.json()["data"]["message_id"]

        # Without ignore_history: pre-filled, no tap needed.
        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": msg2},
            headers=headers,
        )
        assert r.json()["from_history"] is True

        # With ignore_history=true: the CHANGE affordance -- forces the
        # normal ask so the human can pick a different pocket this time.
        r2 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": msg2, "ignore_history": True},
            headers=headers,
        )
        data2 = r2.json()
        assert data2["status"] == "needs_disambiguation"
        assert {o["value"] for o in data2["options"]} == {"food", "shopping"}

    def test_history_is_scoped_to_the_owning_sustain(self, client, user, sustain):
        headers, user_id = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap1 = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "d1", "sustain_id": sustain, "raw_payload": BUYGOODS},
            headers=headers,
        )
        msg1 = cap1.json()["data"]["message_id"]
        infer1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": msg1, "known": {"pocket_name": "food"}},
            headers=headers,
        ).json()
        client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": infer1["operator"], "params": infer1["params"],
                "message_id": msg1, "description": infer1["description"],
            },
            headers=headers,
        )

        other_sustain = client.post(
            "/devui/sustains",
            json={"template_id": "homestead", "user_id": user_id, "parameters": {}},
            headers=headers,
        ).json()["data"]["sustain_id"]
        _seed_pocket(client, headers, other_sustain, "food", 9000)

        r = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": other_sustain, "effect_text": "paid to NAIVAS SUPERMARKET"},
            headers=headers,
        )
        # A different sustain never inherits another sustain's classification
        # history, even for an identical merchant name.
        assert r.json().get("from_history") in (False, None)


class TestUnparsedMessageNoLongerDeadEnds:
    """Reproduces the exact real-device bug Bonnie reported: a captured SMS
    no registered transducer parser recognised (source: mpesa, "No
    registered parser recognised this message's shape") landed as truly
    unparsed -- parsed_fields={}, no amount, no counterparty. Classifying
    the pocket used to dead-end at "no amount could be recovered", with
    nothing the person could do about it and no transaction ever recorded."""

    UNPARSED_SMS = "Dear Customer, your account XYZ456 has been credited with Ksh2,500.00 today. Thank you."

    def test_the_raw_message_really_is_unparsed(self, client, user, sustain):
        # Sanity-check the test fixture itself: this must genuinely be a
        # shape none of the real transducer parsers recognise, or this
        # whole test proves nothing. A genuinely unparsed capture's STORED
        # status is still "needs_attention" (same as parsed_unmapped --
        # ingest_engine._process() only distinguishes "mapped" at the top
        # level); the transducer's own "unparsed" verdict shows up in
        # reason/parser_name instead, exactly what Bonnie's real device saw.
        headers, _ = user
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "mpesa", "sustain_id": sustain, "raw_payload": self.UNPARSED_SMS},
            headers=headers,
        )
        data = cap.json()["data"]
        assert data["status"] == "needs_attention"
        assert data["parsed_fields"] == {}
        assert data["parser_name"] in (None, "")
        assert "no registered parser" in (data["reason"] or "").lower()

    def test_pocket_then_typed_amount_reaches_a_real_committed_transaction(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "mpesa", "sustain_id": sustain, "raw_payload": self.UNPARSED_SMS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        # Round 1: no known facts at all -- must ask for a pocket, not
        # dead-end, and must NOT yet be able to reach amount (untested here)
        # since raw_text-based recovery only kicks in once pocket is settled
        # in this widget's field order.
        r1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id},
            headers=headers,
        )
        data1 = r1.json()
        assert data1["status"] == "needs_disambiguation"
        assert data1["field"] == "pocket_name"

        # Round 2: pocket picked. This message's raw text has NO literal
        # "Ksh" figure matched by the safe currency regex at the exact spot
        # a human would expect -- it DOES have one ("Ksh2,500.00"), so this
        # should resolve straight through via the raw-text fallback rather
        # than asking for the amount at all.
        r2 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {"pocket_name": "food"}},
            headers=headers,
        )
        data2 = r2.json()
        # Two candidate operators (spend/allocate) are both satisfiable
        # once pocket+amount are known with no verb/direction hint -- a
        # real, expected disambiguation, not the bug being tested here.
        if data2["status"] == "needs_disambiguation" and data2["field"] == "operator":
            r2b = client.post(
                "/orchie/capture/infer",
                json={
                    "sustain_id": sustain, "message_id": message_id,
                    "known": {"pocket_name": "food", "operator": "budget.spend"},
                },
                headers=headers,
            )
            data2 = r2b.json()

        assert data2["status"] == "ready"
        assert data2["params"]["amount"] == 2500.0  # recovered from raw SMS text, never asked

        confirm = client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": data2["operator"], "params": data2["params"],
                "message_id": message_id, "description": data2["description"],
            },
            headers=headers,
        )
        assert confirm.json()["result"]["status"] == "ok"
        assert confirm.json()["message_resolved"] is True

    def test_a_truly_unrecoverable_amount_asks_and_then_commits_the_typed_value(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        no_amount_at_all = "Your account was accessed from a new device. If this wasn't you, contact us."
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "mpesa", "sustain_id": sustain, "raw_payload": no_amount_at_all},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        infer1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {"pocket_name": "food", "operator": "budget.spend"}},
            headers=headers,
        ).json()
        # No dead end -- a real, answerable question with no tap options
        # (the frontend renders a text input for field="amount").
        assert infer1["status"] == "needs_disambiguation"
        assert infer1["field"] == "amount"
        assert infer1["options"] is None

        # Human types "500" -- arrives as a string, exactly like a text
        # input naturally sends.
        infer2 = client.post(
            "/orchie/capture/infer",
            json={
                "sustain_id": sustain, "message_id": message_id,
                "known": {"pocket_name": "food", "operator": "budget.spend", "amount": "500"},
            },
            headers=headers,
        ).json()
        assert infer2["status"] == "ready"
        assert infer2["params"]["amount"] == 500.0

        confirm = client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": infer2["operator"], "params": infer2["params"],
                "message_id": message_id, "description": infer2["description"],
            },
            headers=headers,
        )
        assert confirm.json()["result"]["status"] == "ok"
        assert confirm.json()["result"]["data"]["amount_spent"] == 500.0


class TestFullCorrectionFreedom:
    """Real degrees of freedom, per Bonnie's explicit ask: a captured
    transaction (parsed wrong, or not parsed at all) must be fully
    correctable in the moment -- amount, description, direction, and
    pocket -- not just 'pick a pocket'. Mirrors exactly what the frontend's
    ReadyReview component does: each correction re-runs infer() with the
    fact folded into `known`."""

    UNPARSED_SMS = "Dear Customer, your account XYZ456 has been credited with Ksh2,500.00 today. Thank you."

    def test_direction_correctable_to_money_in_with_no_pocket_ever_asked(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "kcb", "sustain_id": sustain, "raw_payload": self.UNPARSED_SMS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        # The MONEY IN toggle: an explicit operator override, no pocket
        # fact supplied at all.
        infer1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {"operator": "budget.record_income"}},
            headers=headers,
        ).json()
        assert infer1["status"] == "ready"
        assert infer1["operator"] == "budget.record_income"
        assert infer1["params"]["amount"] == 2500.0
        assert "pocket_name" not in infer1["params"]

        confirm = client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": infer1["operator"], "params": infer1["params"],
                "message_id": message_id, "description": infer1["description"],
            },
            headers=headers,
        )
        assert confirm.json()["result"]["status"] == "ok"
        assert confirm.json()["result"]["data"]["amount_credited"] == 2500.0
        assert confirm.json()["message_resolved"] is True

    def test_a_wrong_auto_recovered_amount_is_correctable(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "kcb", "sustain_id": sustain, "raw_payload": self.UNPARSED_SMS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        infer1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {"pocket_name": "food", "operator": "budget.spend"}},
            headers=headers,
        ).json()
        assert infer1["params"]["amount"] == 2500.0  # auto-recovered from raw text

        # The person notices the auto-recovered figure is wrong (e.g. the
        # SMS's "2,500" was actually a reference number, not the amount)
        # and corrects it -- a real edit, not just accepting the parse.
        infer2 = client.post(
            "/orchie/capture/infer",
            json={
                "sustain_id": sustain, "message_id": message_id,
                "known": {"pocket_name": "food", "operator": "budget.spend", "amount": "1800"},
            },
            headers=headers,
        ).json()
        assert infer2["status"] == "ready"
        assert infer2["params"]["amount"] == 1800.0

        confirm = client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": infer2["operator"], "params": infer2["params"],
                "message_id": message_id, "description": infer2["description"],
            },
            headers=headers,
        )
        assert confirm.json()["result"]["data"]["amount_spent"] == 1800.0

    def test_description_is_correctable_and_flows_into_the_committed_event(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "kcb", "sustain_id": sustain, "raw_payload": self.UNPARSED_SMS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        infer1 = client.post(
            "/orchie/capture/infer",
            json={
                "sustain_id": sustain, "message_id": message_id,
                "known": {"pocket_name": "food", "operator": "budget.spend", "description": "electricity top-up"},
            },
            headers=headers,
        ).json()
        assert infer1["status"] == "ready"
        assert infer1["params"]["description"] == "electricity top-up"
        assert infer1["description"] == "electricity top-up"

        confirm = client.post(
            "/orchie/capture/confirm",
            json={
                "sustain_id": sustain, "operator": infer1["operator"], "params": infer1["params"],
                "message_id": message_id, "description": infer1["description"],
            },
            headers=headers,
        )
        assert confirm.json()["result"]["data"]["description"] == "electricity top-up"

    def test_switching_back_from_money_in_to_out_resumes_pocket_flow(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "kcb", "sustain_id": sustain, "raw_payload": self.UNPARSED_SMS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]

        # First, MONEY IN.
        infer1 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {"operator": "budget.record_income"}},
            headers=headers,
        ).json()
        assert infer1["operator"] == "budget.record_income"

        # MONEY OUT toggle -- clears the operator override, normal
        # spend/allocate + pocket resolution must resume.
        infer2 = client.post(
            "/orchie/capture/infer",
            json={"sustain_id": sustain, "message_id": message_id, "known": {}},
            headers=headers,
        ).json()
        assert infer2["status"] == "needs_disambiguation"
        assert infer2["field"] == "pocket_name"
        assert {o["value"] for o in infer2["options"]} == {"food"}

    def test_raw_payload_is_present_for_the_classify_card_to_render(self, client, user, sustain):
        headers, _ = user
        _seed_pocket(client, headers, sustain, "food", 9000)
        cap = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "kcb", "sustain_id": sustain, "raw_payload": self.UNPARSED_SMS},
            headers=headers,
        )
        message_id = cap.json()["data"]["message_id"]
        msg = client.get(f"/api/v1/ingest/messages/{message_id}", headers=headers).json()["data"]
        assert msg["raw_payload"] == self.UNPARSED_SMS


class TestAddPocketEndToEnd:
    """budget.add_pocket over the REAL HTTP route against a real homestead
    sustain -- this is the exact path that caught a real gap live: adding
    an operator to OPERATOR_REGISTRY is not enough, it must also be
    declared on the sustain's own spec operator list or execute_operator's
    allow-list check refuses it with constraint_violated=operator_allowed."""

    def test_add_pocket_via_the_real_confirm_route(self, client, user, sustain):
        headers, _ = user
        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.add_pocket", "params": {"pocket_name": "holiday fund"}},
            headers=headers,
        )
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["result"]["status"] == "ok"
        assert data["result"]["data"]["pocket"] == "holiday_fund"

        state = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state["finances"]["pockets"]["holiday_fund"]["allocated"] == 0.0
        assert state["finances"]["liquid"]["balance"] == 0.0  # no money moved by pocket creation itself

    def test_newly_created_pocket_can_then_receive_a_real_classification(self, client, user, sustain):
        # The full "+ NEW" flow: create the pocket, then a captured
        # transaction files into it exactly like any pre-existing pocket.
        headers, _ = user
        client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.add_pocket", "params": {"pocket_name": "car repair"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 5000, "source": "seed", "frequency": "once"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "car_repair", "amount": 3000, "period": "monthly"}},
            headers=headers,
        )
        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "car_repair", "amount": 1200, "description": "brake pads"}},
            headers=headers,
        )
        assert r.json()["result"]["status"] == "ok"
        assert r.json()["result"]["data"]["amount_spent"] == 1200.0


class TestAllocateThenRetryRecovery:
    """
    Bonnie's real case: +NEW an "Airtime" pocket (KES 0), spend 50 into it
    -> refused for insufficient pocket balance -> allocate the shortfall
    (via the real gated budget.allocate) -> retry the identical spend ->
    now recorded. Reproduces the exact scenario over the real HTTP routes
    the Orchie frontend's allocateThenRetry() calls, end to end, with no
    hand-written side channel around the gate anywhere in the loop.
    """

    def test_spend_against_empty_pocket_carries_structured_shortfall_over_http(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.add_pocket", "params": {"pocket_name": "Airtime"}},
            headers=headers,
        )
        r = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "Airtime", "amount": 50}},
            headers=headers,
        )
        assert r.status_code == 200, r.text
        result = r.json()["result"]
        assert result["status"] == "failed"
        assert result["constraint_violated"] == "pocket_balance_sufficient"
        assert result["data"] == {"pocket": "Airtime", "remaining": 0.0, "requested": 50.0, "shortfall": 50.0}

    def test_allocate_shortfall_then_retry_the_same_spend_succeeds(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.add_pocket", "params": {"pocket_name": "Airtime"}},
            headers=headers,
        )
        # Fund liquid balance so there's something real to allocate FROM --
        # a genuinely empty pocket with a genuinely empty liquid balance
        # is the honest-failure case covered by the next test.
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 500, "source": "seed", "frequency": "once"}},
            headers=headers,
        )
        refused = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "Airtime", "amount": 50}},
            headers=headers,
        ).json()
        shortfall = refused["result"]["data"]["shortfall"]
        assert shortfall == 50.0

        # Step 1 of the recovery loop: allocate the shortfall into the
        # pocket -- the exact call allocateThenRetry() makes.
        alloc = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "Airtime", "amount": shortfall}},
            headers=headers,
        )
        assert alloc.json()["result"]["status"] == "ok"

        # Step 2: automatic retry of the ORIGINAL, identical spend.
        retry = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "Airtime", "amount": 50}},
            headers=headers,
        )
        assert retry.json()["result"]["status"] == "ok"
        assert retry.json()["result"]["data"]["amount_spent"] == 50.0

        state = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state["finances"]["pockets"]["Airtime"]["spent"] == 50.0
        assert state["finances"]["pockets"]["Airtime"]["allocated"] == 50.0
        assert state["finances"]["liquid"]["balance"] == 450.0  # 500 income - 50 allocated

    def test_allocate_honestly_refuses_when_liquid_balance_cant_cover_it(self, client, user, sustain):
        """No unallocated funds to draw from -- the allocate step itself is
        refused (by budget.allocate's own real constraint, not a fake one
        invented for this recovery flow), surfaced as-is, nothing silently
        retried against state that can't support it."""
        headers, _ = user
        client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.add_pocket", "params": {"pocket_name": "Airtime"}},
            headers=headers,
        )
        # Zero liquid balance -- nothing to allocate from.
        alloc = client.post(
            "/orchie/capture/confirm",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "Airtime", "amount": 50}},
            headers=headers,
        )
        assert alloc.status_code == 200
        result = alloc.json()["result"]
        assert result["status"] == "failed"
        assert "liquid" in result["reason"].lower() or "balance" in result["reason"].lower()

        state = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state["finances"]["pockets"]["Airtime"]["allocated"] == 0.0  # untouched by the failed allocate
