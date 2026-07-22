"""
sustena/api/routes/council.py

Council proposal and voting endpoints.

Routes (all under /api/v1/council, mounted by main.py):
  GET  /{sustain_id}/proposals           — list proposals with vote tallies
  POST /{sustain_id}/proposals           — create a proposal (status=IN_VOTING)
  GET  /{sustain_id}/proposals/{id}      — single proposal with full vote breakdown
  POST /{sustain_id}/proposals/{id}/vote — cast a vote on a proposal
"""

import json
import logging
import uuid
from datetime import datetime, timedelta, timezone

from fastapi import APIRouter, Depends, HTTPException, Query
from pydantic import BaseModel, Field

from sustena.api.routes.users import get_current_user
from sustena.db.schema import (
    council_proposals as proposals_table,
    council_votes as votes_table,
    get_engine,
)
from sqlalchemy import select

router = APIRouter()
logger = logging.getLogger(__name__)


# ── Root ──────────────────────────────────────────────────────────────────────

@router.get("/", summary="Council router health")
async def council_root() -> dict:
    return {"status": "ok", "service": "council"}


# ── Helpers ───────────────────────────────────────────────────────────────────

def _ok(data) -> dict:
    return {"status": "ok", "data": data, "timestamp": datetime.now(timezone.utc).isoformat()}


def _dt(val) -> str | None:
    if val is None:
        return None
    return val.isoformat() if hasattr(val, "isoformat") else str(val)


def _aggregate_votes(vote_rows) -> dict:
    for_count = against_count = abstain_count = 0
    breakdown = []
    for v in vote_rows:
        norm = (v.vote or "ABSTAIN").upper()
        if norm == "YES":
            for_count += 1
        elif norm == "NO":
            against_count += 1
        else:
            abstain_count += 1
        breakdown.append({
            "operative_id": v.operative_id,
            "vote":         norm,
            "reasoning":    v.reasoning,
            "weight":       float(v.weight or 0),
        })
    return {
        "for":       for_count,
        "against":   against_count,
        "abstain":   abstain_count,
        "total":     len(vote_rows),
        "breakdown": breakdown,
    }


def _row_to_proposal(row, vote_rows) -> dict:
    input_data = {}
    try:
        if row.input_json:
            input_data = json.loads(row.input_json)
    except Exception:
        pass
    sim = None
    try:
        if row.simulation_results_json:
            sim = json.loads(row.simulation_results_json)
    except Exception:
        pass
    return {
        "id":            row.id,
        "sustain_id":    row.sustain_id,
        "proposed_by":   row.proposed_by,
        "operator_name": row.operator_name,
        "input":         input_data,
        "simulation":    sim,
        "status":        row.status,
        "created_at":    _dt(row.created_at),
        "resolved_at":   _dt(row.resolved_at),
        "expires_at":    _dt(row.expires_at),
        "votes":         _aggregate_votes(vote_rows),
    }


# ── GET /{sustain_id}/proposals ───────────────────────────────────────────────

@router.get("/{sustain_id}/proposals", summary="List council proposals with vote tallies")
async def list_council_proposals(
    sustain_id: str,
    status: str | None = Query(default=None, description="Filter by status: IN_VOTING, PASSED, FAILED"),
) -> dict:
    db_engine = get_engine()
    try:
        async with db_engine.connect() as conn:
            q = select(proposals_table).where(
                proposals_table.c.sustain_id == sustain_id
            )
            if status:
                q = q.where(proposals_table.c.status == status.upper())
            q = q.order_by(proposals_table.c.created_at.desc()).limit(50)
            prop_rows = (await conn.execute(q)).fetchall()

            proposals = []
            for row in prop_rows:
                vote_rows = (await conn.execute(
                    select(votes_table).where(votes_table.c.proposal_id == row.id)
                )).fetchall()
                proposals.append(_row_to_proposal(row, vote_rows))

        return _ok({"proposals": proposals, "count": len(proposals), "sustain_id": sustain_id})
    except Exception as exc:
        logger.warning("list_council_proposals(%s) failed: %s", sustain_id, exc)
        return _ok({"proposals": [], "count": 0, "sustain_id": sustain_id})


# ── GET /{sustain_id}/proposals/{id} ─────────────────────────────────────────

@router.get("/{sustain_id}/proposals/{proposal_id}", summary="Single proposal with full vote breakdown")
async def get_council_proposal(sustain_id: str, proposal_id: str) -> dict:
    db_engine = get_engine()
    async with db_engine.connect() as conn:
        row = (await conn.execute(
            select(proposals_table).where(
                proposals_table.c.id == proposal_id,
                proposals_table.c.sustain_id == sustain_id,
            )
        )).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="Proposal not found")
        vote_rows = (await conn.execute(
            select(votes_table).where(votes_table.c.proposal_id == proposal_id)
        )).fetchall()
    return _ok({"proposal": _row_to_proposal(row, vote_rows)})


# ── POST /{sustain_id}/proposals ──────────────────────────────────────────────

class CreateProposalRequest(BaseModel):
    proposed_by:   str  = Field(..., description="Originating operative or 'user'")
    operator_name: str  = Field(..., description="Operator being proposed")
    input_json:    dict = Field(default_factory=dict)


@router.post("/{sustain_id}/proposals", summary="Create a council proposal", status_code=201)
async def create_council_proposal(
    sustain_id: str,
    body: CreateProposalRequest,
    _current_user: dict = Depends(get_current_user),
) -> dict:
    proposal_id = str(uuid.uuid4())
    now = datetime.utcnow()
    db_engine = get_engine()
    async with db_engine.begin() as conn:
        await conn.execute(
            proposals_table.insert().values(
                id=proposal_id,
                sustain_id=sustain_id,
                proposed_by=body.proposed_by,
                operator_name=body.operator_name,
                input_json=json.dumps(body.input_json),
                simulation_results_json=None,
                status="IN_VOTING",
                created_at=now,
                expires_at=now + timedelta(hours=48),
            )
        )
    return _ok({
        "id":            proposal_id,
        "sustain_id":    sustain_id,
        "operator_name": body.operator_name,
        "status":        "IN_VOTING",
        "created_at":    now.isoformat(),
    })


# ── POST /{sustain_id}/proposals/{id}/vote ────────────────────────────────────

class CastVoteRequest(BaseModel):
    operative_id: str = Field(..., description="Operative casting the vote")
    vote:         str = Field(..., pattern="^(YES|NO|ABSTAIN)$")
    reasoning:    str | None = None


@router.post("/{sustain_id}/proposals/{proposal_id}/vote", summary="Cast a vote on a proposal", status_code=201)
async def cast_vote(
    sustain_id: str,
    proposal_id: str,
    body: CastVoteRequest,
    _current_user: dict = Depends(get_current_user),
) -> dict:
    db_engine = get_engine()
    async with db_engine.connect() as conn:
        row = (await conn.execute(
            select(proposals_table).where(
                proposals_table.c.id == proposal_id,
                proposals_table.c.sustain_id == sustain_id,
            )
        )).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="Proposal not found")
        if row.status != "IN_VOTING":
            raise HTTPException(status_code=409, detail=f"Proposal is {row.status}, not IN_VOTING")

    vote_id = str(uuid.uuid4())
    async with db_engine.begin() as conn:
        await conn.execute(
            votes_table.insert().values(
                id=vote_id,
                proposal_id=proposal_id,
                operative_id=body.operative_id,
                vote=body.vote,
                reasoning=body.reasoning,
                weight=0.098,
                timestamp=datetime.utcnow(),
            )
        )
    return _ok({"vote_id": vote_id, "proposal_id": proposal_id, "vote": body.vote})
