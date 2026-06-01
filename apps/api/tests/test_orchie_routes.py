"""
tests/test_orchie_routes.py

Tests for POST /orchie/message — the Orchie conversational endpoint.

No auth guard on this route (unlike devui).
Uses the mock Claude client (ANTHROPIC_API_KEY=mock) which is the default
in the test environment.

Run: pytest tests/test_orchie_routes.py -v
"""

import os

import pytest
from starlette.testclient import TestClient

# Ensure mock client is used before importing the app
os.environ.setdefault("ANTHROPIC_API_KEY", "mock")
os.environ.setdefault("ENVIRONMENT", "development")

from sustena.api.main import app  # noqa: E402

URL = "/orchie/message"


@pytest.fixture(scope="module")
def client():
    with TestClient(app, raise_server_exceptions=False) as c:
        yield c


# ---------------------------------------------------------------------------
# Response shape
# ---------------------------------------------------------------------------

class TestOrchieResponseShape:
    """Every successful call must return the four required fields."""

    def test_basic_message_returns_200(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert r.status_code == 200

    def test_response_has_reply_field(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert "reply" in r.json()

    def test_response_has_proposals_field(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert "proposals" in r.json()

    def test_response_has_actions_field(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert "actions" in r.json()

    def test_response_has_tone_field(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert "tone" in r.json()

    def test_proposals_is_list(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert isinstance(r.json()["proposals"], list)

    def test_actions_is_list(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert isinstance(r.json()["actions"], list)

    def test_tone_is_string(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hi"})
        assert isinstance(r.json()["tone"], str)


# ---------------------------------------------------------------------------
# Context handling
# ---------------------------------------------------------------------------

class TestContextHandling:
    """Context dict is optional; both forms should behave the same."""

    def test_message_with_context_returns_200(self, client):
        r = client.post(URL, json={
            "sustain_id": "homestead.bonnie",
            "message": "what's my balance?",
            "context": {"balance_kes": 8420, "pocket": "food"},
        })
        assert r.status_code == 200

    def test_message_with_context_has_reply(self, client):
        r = client.post(URL, json={
            "sustain_id": "homestead.bonnie",
            "message": "what's my balance?",
            "context": {"balance_kes": 8420},
        })
        assert r.json()["reply"]

    def test_empty_context_same_as_no_context(self, client):
        msg = "hi"
        r_no_ctx = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": msg})
        r_empty_ctx = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": msg, "context": {}})
        # Both must succeed with non-empty replies
        assert r_no_ctx.status_code == 200
        assert r_empty_ctx.status_code == 200
        assert r_no_ctx.json()["reply"]
        assert r_empty_ctx.json()["reply"]


# ---------------------------------------------------------------------------
# Validation errors
# ---------------------------------------------------------------------------

class TestValidation:
    """Missing required fields must return 422."""

    def test_missing_sustain_id_returns_422(self, client):
        r = client.post(URL, json={"message": "hello"})
        assert r.status_code == 422

    def test_missing_message_returns_422(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie"})
        assert r.status_code == 422

    def test_empty_body_returns_422(self, client):
        r = client.post(URL, json={})
        assert r.status_code == 422


# ---------------------------------------------------------------------------
# Mock response content
# ---------------------------------------------------------------------------

class TestMockResponses:
    """Verify the mock client returns non-empty, plausible replies."""

    def test_hello_message_gets_non_empty_reply(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hello"})
        assert r.status_code == 200
        assert len(r.json()["reply"]) > 0

    def test_balance_message_gets_non_empty_reply(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "balance"})
        assert r.status_code == 200
        assert len(r.json()["reply"]) > 0

    def test_unknown_message_falls_back_to_default_200(self, client):
        r = client.post(URL, json={
            "sustain_id": "homestead.bonnie",
            "message": "xyzzy-completely-unknown-input-abc123",
        })
        assert r.status_code == 200
        assert len(r.json()["reply"]) > 0

    def test_hello_reply_is_string(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "hello"})
        assert isinstance(r.json()["reply"], str)

    def test_balance_reply_is_string(self, client):
        r = client.post(URL, json={"sustain_id": "homestead.bonnie", "message": "balance"})
        assert isinstance(r.json()["reply"], str)
