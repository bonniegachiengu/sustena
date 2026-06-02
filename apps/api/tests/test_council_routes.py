"""
tests/test_council_routes.py

Tests for /api/v1/council routes:
  GET  /                                  — health / root
  GET  /{sustain_id}/proposals            — list proposals (with status filter)
  POST /{sustain_id}/proposals            — create proposal
  GET  /{sustain_id}/proposals/{id}       — single proposal with vote breakdown
  POST /{sustain_id}/proposals/{id}/vote  — cast a vote
"""

import pytest
import pytest_asyncio
from httpx import AsyncClient, ASGITransport

from sustena.db.schema import init_db, metadata, get_engine
from sustena.api.main import app


# ── fixture ───────────────────────────────────────────────────────────────────

@pytest_asyncio.fixture
async def client():
    import sustena.db.schema as _schema
    from sqlalchemy.ext.asyncio import create_async_engine

    test_engine = create_async_engine(
        "sqlite+aiosqlite:///:memory:",
        connect_args={"check_same_thread": False},
    )
    _schema._engine = test_engine

    async with test_engine.begin() as conn:
        await conn.run_sync(metadata.create_all)

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac


SUSTAIN_ID = "homestead.test"


# ── root ──────────────────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_council_root(client):
    r = await client.get("/api/v1/council/")
    assert r.status_code == 200
    assert r.json()["service"] == "council"


# ── list proposals ─────────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_list_proposals_empty(client):
    r = await client.get(f"/api/v1/council/{SUSTAIN_ID}/proposals")
    assert r.status_code == 200
    data = r.json()["data"]
    assert data["proposals"] == []
    assert data["count"] == 0


@pytest.mark.asyncio
async def test_list_proposals_status_filter(client):
    # Create two proposals
    await client.post(f"/api/v1/council/{SUSTAIN_ID}/proposals", json={
        "proposed_by": "mentor",
        "operator_name": "budget.allocate",
        "input_json": {"pocket_name": "food", "amount": 5000},
    })
    await client.post(f"/api/v1/council/{SUSTAIN_ID}/proposals", json={
        "proposed_by": "curator",
        "operator_name": "budget.record_income",
        "input_json": {},
    })

    r_all = await client.get(f"/api/v1/council/{SUSTAIN_ID}/proposals")
    assert r_all.json()["data"]["count"] == 2

    r_passed = await client.get(f"/api/v1/council/{SUSTAIN_ID}/proposals?status=PASSED")
    assert r_passed.json()["data"]["count"] == 0

    r_voting = await client.get(f"/api/v1/council/{SUSTAIN_ID}/proposals?status=IN_VOTING")
    assert r_voting.json()["data"]["count"] == 2


# ── create proposal ───────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_create_proposal(client):
    r = await client.post(f"/api/v1/council/{SUSTAIN_ID}/proposals", json={
        "proposed_by": "mentor",
        "operator_name": "budget.allocate",
        "input_json": {"pocket_name": "food", "amount": 5000},
    })
    assert r.status_code == 201
    data = r.json()["data"]
    assert data["status"] == "IN_VOTING"
    assert data["operator_name"] == "budget.allocate"
    assert "id" in data


# ── get single proposal ───────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_get_proposal(client):
    create_r = await client.post(f"/api/v1/council/{SUSTAIN_ID}/proposals", json={
        "proposed_by": "mentor",
        "operator_name": "budget.allocate",
        "input_json": {},
    })
    proposal_id = create_r.json()["data"]["id"]

    r = await client.get(f"/api/v1/council/{SUSTAIN_ID}/proposals/{proposal_id}")
    assert r.status_code == 200
    proposal = r.json()["data"]["proposal"]
    assert proposal["id"] == proposal_id
    assert proposal["votes"]["for"] == 0
    assert proposal["votes"]["breakdown"] == []


@pytest.mark.asyncio
async def test_get_proposal_not_found(client):
    r = await client.get(f"/api/v1/council/{SUSTAIN_ID}/proposals/nonexistent-id")
    assert r.status_code == 404


# ── cast vote ─────────────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_cast_vote(client):
    create_r = await client.post(f"/api/v1/council/{SUSTAIN_ID}/proposals", json={
        "proposed_by": "mentor",
        "operator_name": "budget.allocate",
        "input_json": {},
    })
    proposal_id = create_r.json()["data"]["id"]

    vote_r = await client.post(
        f"/api/v1/council/{SUSTAIN_ID}/proposals/{proposal_id}/vote",
        json={"operative_id": "mentor", "vote": "YES", "reasoning": "looks good"},
    )
    assert vote_r.status_code == 201
    assert vote_r.json()["data"]["vote"] == "YES"

    # Vote tally reflected in list
    prop_r = await client.get(f"/api/v1/council/{SUSTAIN_ID}/proposals/{proposal_id}")
    votes = prop_r.json()["data"]["proposal"]["votes"]
    assert votes["for"] == 1
    assert votes["total"] == 1
    assert votes["breakdown"][0]["operative_id"] == "mentor"
    assert votes["breakdown"][0]["reasoning"] == "looks good"


@pytest.mark.asyncio
async def test_cast_vote_invalid_status(client):
    """Cannot vote on non-IN_VOTING proposal."""
    r = await client.post(
        f"/api/v1/council/{SUSTAIN_ID}/proposals/nonexistent/vote",
        json={"operative_id": "mentor", "vote": "YES"},
    )
    assert r.status_code == 404
