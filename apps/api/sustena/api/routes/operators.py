"""
sustena/api/routes/operators.py

REST endpoints for operator execution and history.
Epic 1.4.1 — stub. Full implementation in Epic 1.4.3.
"""

from fastapi import APIRouter

router = APIRouter()


@router.get("/")
async def list_operators():
    """List available operators. (not yet implemented)"""
    return {"message": "not yet implemented"}
