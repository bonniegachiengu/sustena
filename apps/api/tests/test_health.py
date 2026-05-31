"""
tests/test_health.py
Smoke test — verifies the FastAPI app starts and /health responds.
Run with: pytest tests/test_health.py
"""
import pytest
from httpx import AsyncClient, ASGITransport
from sustena.api.main import app


@pytest.mark.asyncio
async def test_health_endpoint_exists():
    """
    /health must return 200 or 503 (never 404/500).
    We accept 503 in test environment since Claude key may be a placeholder.
    """
    async with AsyncClient(
        transport=ASGITransport(app=app), base_url="http://test"
    ) as client:
        response = await client.get("/health")
    assert response.status_code in (200, 503), (
        f"Expected 200 or 503, got {response.status_code}"
    )
    data = response.json()
    assert "status" in data
    assert "db_status" in data
    assert "claude_status" in data
