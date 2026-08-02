"""
tests/test_orchie_compose_route.py

Tests for GET /orchie/compose (sustena/api/routes/orchie.py) — auth guard,
ownership boundary, and the happy path over HTTP. Same module-scoped
TestClient pattern as test_ingest_routes.py.

Run with:
    python -m pytest tests/test_orchie_compose_route.py -v
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
    """Same override as test_ingest_routes.py: this module uses one
    long-lived TestClient/event loop, so a mid-module engine reset would
    wipe the users/sustains tables underneath it."""
    yield


@pytest.fixture
def user(client):
    email = f"orchie-compose-tests-{uuid.uuid4().hex[:12]}@example.com"
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


class TestAuthGuard:
    def test_compose_no_auth_returns_401(self, client, sustain):
        r = client.get(f"/orchie/compose?sustain_id={sustain}")
        assert r.status_code == 401


class TestOwnershipBoundary:
    def test_compose_unowned_sustain_returns_404(self, client, user):
        headers, _ = user
        r = client.get("/orchie/compose?sustain_id=does-not-exist", headers=headers)
        assert r.status_code == 404

    def test_compose_another_users_sustain_returns_404(self, client, user, sustain):
        # `sustain` belongs to the `user` fixture's account; register a
        # second, unrelated account and confirm it cannot compose a view
        # for someone else's household.
        email2 = f"orchie-compose-other-{uuid.uuid4().hex[:12]}@example.com"
        r2 = client.post("/api/v1/users/register", json={"email": email2, "password": "test-password-123"})
        headers2 = {"Authorization": f"Bearer {r2.json()['data']['token']}"}
        r = client.get(f"/orchie/compose?sustain_id={sustain}", headers=headers2)
        assert r.status_code == 404


class TestHappyPath:
    def test_compose_returns_shape_on_a_fresh_homestead(self, client, user, sustain):
        headers, _ = user
        r = client.get(f"/orchie/compose?sustain_id={sustain}&device=phone&budget=4", headers=headers)
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["sustain_id"] == sustain
        assert data["device"] == "phone"
        assert data["budget"] == 4
        assert "selected" in data and "excluded" in data
        # household_rollup_summary is Unit-bound and always eligible in
        # PRINCIPLE, but a genuinely empty sustain (no liquid balance, no
        # linked children at all) suppresses it -- see curated_ui.py's own
        # "genuinely_empty" comment (2 Aug 2026, Bonnie: "don't surface it
        # as clutter"). A freshly instantiated homestead has nothing else
        # to show either, so the honest answer is an empty selection.
        assert data["selected"] == []

    def test_rollup_card_appears_once_there_is_a_real_liquid_balance(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 1000, "source": "t", "frequency": "once"}},
            headers=headers,
        )
        r = client.get(f"/orchie/compose?sustain_id={sustain}&device=phone&budget=4", headers=headers)
        data = r.json()
        ids = [w["id"] for w in data["selected"]]
        assert "household_rollup_summary" in ids

    def test_compose_reflects_a_real_recent_spend(self, client, user, sustain):
        headers, _ = user
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.record_income", "params": {"amount": 1000, "source": "t", "frequency": "once"}},
            headers=headers,
        )
        client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 1000, "period": "monthly"}},
            headers=headers,
        )
        r_spend = client.post(
            "/devui/console/execute",
            json={"sustain_id": sustain, "operator": "budget.spend", "params": {"pocket_name": "food", "amount": 1000, "description": "x", "category": "test"}},
            headers=headers,
        )
        assert r_spend.status_code == 200, r_spend.text

        r = client.get(f"/orchie/compose?sustain_id={sustain}&budget=1", headers=headers)
        assert r.status_code == 200, r.text
        data = r.json()
        assert data["selected"][0]["id"] == "pocket_spent_watch"
        assert data["selected"][0]["urgency"] == pytest.approx(1.0)

    def test_compose_is_read_only_over_http(self, client, user, sustain):
        headers, _ = user
        state_before = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        client.get(f"/orchie/compose?sustain_id={sustain}", headers=headers)
        client.get(f"/orchie/compose?sustain_id={sustain}&query=pocket&budget=2", headers=headers)
        state_after = client.get(f"/devui/state?sustain_id={sustain}", headers=headers).json()["data"]["state"]
        assert state_before == state_after


class TestClassifyQueueChronologicalOrder:
    """Bonnie, 2 Aug 2026: reconstructing a budget from message history means
    recording transactions and pocket-to-pocket transfers in the order they
    actually happened -- you can't transfer out of a pocket before it was
    funded. The classify worklist must replay oldest-first, even though
    IngestEngine.needs_attention() itself is newest-first (correct for OTHER
    consumers, e.g. devui.py's Monitor panel). Exercised over real HTTP
    against the real shared engine -- compose()'s classify_card branch reads
    get_shared_ingest_engine() directly, not the sustain_id-scoped engine
    passed into compose(), so this can only be proven at the route layer."""

    def _capture(self, client, headers, sustain, text, captured_at):
        r = client.post(
            "/api/v1/ingest/capture",
            json={"source_id": "mpesa", "sustain_id": sustain, "raw_payload": text, "captured_at": captured_at},
            headers=headers,
        )
        assert r.status_code == 200, r.text
        return r.json()["data"]

    def test_classify_cards_are_ordered_oldest_first(self, client, user, sustain):
        headers, _ = user
        # Captured out of chronological order (as a wide re-sync naturally
        # would -- the native inbox read has no reason to return messages
        # in any particular order relative to when compose() later reads
        # them), each with a DISTINCT real captured_at.
        self._capture(client, headers, sustain, "unparseable text A", "2026-07-01T10:00:00")
        self._capture(client, headers, sustain, "unparseable text C", "2026-07-15T10:00:00")
        self._capture(client, headers, sustain, "unparseable text B", "2026-07-08T10:00:00")

        r = client.get(f"/orchie/compose?sustain_id={sustain}&budget=10", headers=headers)
        data = r.json()
        classify_cards = [w for w in data["selected"] if w["id"] == "unmapped_capture_classify"]
        assert len(classify_cards) == 3
        raw_texts = [c["data"]["raw_payload"] for c in classify_cards]
        assert raw_texts == ["unparseable text A", "unparseable text B", "unparseable text C"]

    def test_oldest_first_survives_a_tight_budget(self, client, user, sustain):
        # Under a tight budget that can't fit every classify card, the
        # OLDEST ones must be the ones that make the cut -- replaying
        # history front-to-back means starting from the beginning, not an
        # arbitrary/newest subset.
        headers, _ = user
        for i, day in enumerate(["05", "01", "03", "04", "02"]):
            self._capture(client, headers, sustain, f"unparseable text {i}", f"2026-07-{day}T10:00:00")

        # classify_card costs 2 (declared on homestead.json); budget=3 fits
        # exactly one, not two.
        r = client.get(f"/orchie/compose?sustain_id={sustain}&budget=3", headers=headers)
        data = r.json()
        classify_cards = [w for w in data["selected"] if w["id"] == "unmapped_capture_classify"]
        assert len(classify_cards) == 1
        assert classify_cards[0]["data"]["raw_payload"] == "unparseable text 1"  # the "01" (oldest) capture

    def test_does_not_disturb_needs_attention_itself(self, client, user, sustain):
        # GET /api/v1/ingest/messages (backed by needs_attention-adjacent
        # reads) stays newest-first -- this is a compose()-local re-sort,
        # not a change to the underlying read other consumers rely on.
        headers, _ = user
        self._capture(client, headers, sustain, "older", "2026-07-01T10:00:00")
        self._capture(client, headers, sustain, "newer", "2026-07-15T10:00:00")

        r = client.get(f"/api/v1/ingest/messages?sustain_id={sustain}&status=needs_attention", headers=headers)
        messages = r.json()["data"]["messages"]
        assert [m["raw_payload"] for m in messages] == ["newer", "older"]
