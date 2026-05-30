"""
sustena/api/routes/sustains.py

REST endpoints for sustain instances.
Epic 1.4.1 — stub. Full implementation in Epic 1.4.2.
"""

from fastapi import APIRouter

router = APIRouter()


@router.get("/")
async def list_sustains():
    """List all sustains for the authenticated user. (not yet implemented)"""
    return {"message": "not yet implemented"}
