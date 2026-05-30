"""
sustena/api/routes/council.py

REST endpoints for Council proposals and votes.
Epic 1.4.1 — stub. Full implementation in Epic 1.4.4.
"""

from fastapi import APIRouter

router = APIRouter()


@router.get("/")
async def list_proposals():
    """List Council proposals for a sustain. (not yet implemented)"""
    return {"message": "not yet implemented"}
