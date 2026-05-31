"""
sustena/api/routes/users.py

REST endpoints for user management.
Epic 1.4.1 — stub. Full implementation in Epic 1.4.5.
"""

from fastapi import APIRouter

router = APIRouter()


@router.get("/")
async def list_users():
    """List users (admin). (not yet implemented)"""
    return {"message": "not yet implemented"}
