"""
tests/test_arena.py

Tests for the /api/v1/arena routes.
Covers: list packages (public), create (auth), get by id (public),
        products + orders stubs.
"""

import pytest
import pytest_asyncio
from httpx import AsyncClient, ASGITransport

from sustena.db.schema import init_db, metadata, get_engine
from sustena.api.main import app


# ── fixture ───────────────────────────────────────────────────────────────────

@pytest_asyncio.fixture
async def client():
    import sqlalchemy
    from sqlalchemy.ext.asyncio import create_async_engine
    import sustena.db.schema as _schema

    test_engine = create_async_engine("sqlite+aiosqlite:///:memory:", connect_args={"check_same_thread": False})
    _schema._engine = test_engine

    async with test_engine.begin() as conn:
        await conn.run_sync(metadata.create_all)

    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac


@pytest_asyncio.fixture
async def auth_client(client):
    """Returns (AsyncClient, token) with a registered test user."""
    r = await client.post("/api/v1/users/register", json={
        "email": "arena_test@example.com",
        "password": "testpassword123",
        "display_name": "Arena Tester",
    })
    assert r.status_code == 200
    token = r.json()["data"]["token"]
    return client, token


# ── list packages (public) ────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_list_packages_empty(client):
    r = await client.get("/api/v1/arena/packages")
    assert r.status_code == 200
    data = r.json()["data"]
    assert data["packages"] == []


@pytest.mark.asyncio
async def test_list_packages_kind_filter(client):
    r = await client.get("/api/v1/arena/packages?kind=operative")
    assert r.status_code == 200
    data = r.json()["data"]
    assert isinstance(data["packages"], list)


# ── create package (auth required) ───────────────────────────────────────────

@pytest.mark.asyncio
async def test_create_package_unauthenticated(client):
    r = await client.post("/api/v1/arena/packages", json={
        "name": "Test Operative",
        "kind": "operative",
    })
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_create_package_ok(auth_client):
    client, token = auth_client
    r = await client.post(
        "/api/v1/arena/packages",
        json={"name": "Budget Monitor", "kind": "operative", "description": "Monitors budget.", "is_free": True},
        headers={"Authorization": f"Bearer {token}"},
    )
    assert r.status_code == 201
    data = r.json()["data"]
    assert data["name"] == "Budget Monitor"
    assert data["kind"] == "operative"


@pytest.mark.asyncio
async def test_create_package_invalid_kind(auth_client):
    client, token = auth_client
    r = await client.post(
        "/api/v1/arena/packages",
        json={"name": "Bad Kind", "kind": "unknown_kind"},
        headers={"Authorization": f"Bearer {token}"},
    )
    assert r.status_code == 422


# ── get package by id ─────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_get_package_not_found(client):
    r = await client.get("/api/v1/arena/packages/nonexistent-id")
    assert r.status_code == 404


@pytest.mark.asyncio
async def test_get_package_ok(auth_client):
    client, token = auth_client
    create_r = await client.post(
        "/api/v1/arena/packages",
        json={"name": "My Spore", "kind": "spore", "tags": ["household", "finance"]},
        headers={"Authorization": f"Bearer {token}"},
    )
    pkg_id = create_r.json()["data"]["id"]

    r = await client.get(f"/api/v1/arena/packages/{pkg_id}")
    assert r.status_code == 200
    pkg = r.json()["data"]["package"]
    assert pkg["name"] == "My Spore"
    assert pkg["kind"] == "spore"
    assert "household" in pkg["tags"]


# ── list packages after creation ──────────────────────────────────────────────

@pytest.mark.asyncio
async def test_list_packages_after_create(auth_client):
    client, token = auth_client
    for name, kind in [("Alpha Widget", "widget"), ("Beta Operator", "operator")]:
        await client.post(
            "/api/v1/arena/packages",
            json={"name": name, "kind": kind},
            headers={"Authorization": f"Bearer {token}"},
        )

    r = await client.get("/api/v1/arena/packages")
    assert r.status_code == 200
    pkgs = r.json()["data"]["packages"]
    names = [p["name"] for p in pkgs]
    assert "Alpha Widget" in names
    assert "Beta Operator" in names


@pytest.mark.asyncio
async def test_list_packages_kind_filter_after_create(auth_client):
    client, token = auth_client
    await client.post(
        "/api/v1/arena/packages",
        json={"name": "My Operative 2", "kind": "operative"},
        headers={"Authorization": f"Bearer {token}"},
    )
    r = await client.get("/api/v1/arena/packages?kind=operative")
    assert r.status_code == 200
    pkgs = r.json()["data"]["packages"]
    assert all(p["kind"] == "operative" for p in pkgs)


# ── products (Sprint 8.9) ─────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_list_products_empty(client):
    r = await client.get("/api/v1/arena/products")
    assert r.status_code == 200
    assert isinstance(r.json()["data"]["products"], list)



# ── orders (Sprint 8.9) ───────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_create_order_unauthenticated(client):
    r = await client.post("/api/v1/arena/orders", json={"items": []})
    assert r.status_code == 401


@pytest.mark.asyncio
async def test_list_orders_empty(auth_client):
    client, token = auth_client
    r = await client.get("/api/v1/arena/orders", headers={"Authorization": f"Bearer {token}"})
    assert r.status_code == 200
    assert r.json()["data"]["orders"] == []


@pytest.mark.asyncio
async def test_place_and_retrieve_order(auth_client):
    client, token = auth_client
    headers = {"Authorization": f"Bearer {token}"}

    r = await client.post("/api/v1/arena/orders", json={
        "items": [
            {"name": "Pilau Kando", "kind": "product", "qty": 2, "price": 280, "unit": "per portion", "emoji": "🍛"},
            {"name": "Sukuma Wiki", "kind": "product", "qty": 1, "price": 35, "unit": "500g", "emoji": "🥬"},
        ],
        "sustain_id": "homestead.bonnie",
        "delivery_addr": "Westlands, Nairobi",
        "pay_method": "till",
    }, headers=headers)
    assert r.status_code == 201
    data = r.json()["data"]
    assert data["ref"].startswith("SXI-")
    assert data["product_status"] == "PLACED"
    assert data["package_status"] is None
    assert data["licenses"] == []

    history = await client.get("/api/v1/arena/orders", headers=headers)
    assert history.status_code == 200
    orders = history.json()["data"]["orders"]
    assert len(orders) == 1
    assert orders[0]["ref"] == data["ref"]
    assert orders[0]["productTotal"] == 595  # 280*2 + 35
    assert orders[0]["deliveryAddr"] == "Westlands, Nairobi"


@pytest.mark.asyncio
async def test_order_with_packages_generates_license(auth_client):
    client, token = auth_client
    r = await client.post("/api/v1/arena/orders", json={
        "items": [
            {"name": "Budget Monitor", "kind": "operative", "qty": 1, "pawa": 50},
        ],
        "sustain_id": "homestead.bonnie",
    }, headers={"Authorization": f"Bearer {token}"})
    assert r.status_code == 201
    data = r.json()["data"]
    assert data["package_status"] == "PLACED"
    assert data["product_status"] is None
    assert len(data["licenses"]) == 1
    assert data["licenses"][0]["name"] == "Budget Monitor"
    assert data["licenses"][0]["key"].startswith("LIC-")


@pytest.mark.asyncio
async def test_free_package_no_license(auth_client):
    client, token = auth_client
    r = await client.post("/api/v1/arena/orders", json={
        "items": [
            {"name": "Free Tool", "kind": "operator", "qty": 1, "pawa": "FREE"},
        ],
    }, headers={"Authorization": f"Bearer {token}"})
    assert r.status_code == 201
    assert r.json()["data"]["licenses"] == []


@pytest.mark.asyncio
async def test_orders_are_user_scoped(auth_client):
    """Two users should not see each other's orders."""
    client, tok_a = auth_client

    reg_b = await client.post("/api/v1/users/register", json={
        "email": "user_b_scoped@test.com", "password": "testpassword123", "display_name": "B",
    })
    assert reg_b.status_code == 200, reg_b.text
    tok_b = reg_b.json()["data"]["token"]

    await client.post("/api/v1/arena/orders", json={
        "items": [{"name": "Chai Bora", "kind": "product", "qty": 1, "price": 80}],
    }, headers={"Authorization": f"Bearer {tok_a}"})

    orders_b = await client.get("/api/v1/arena/orders", headers={"Authorization": f"Bearer {tok_b}"})
    assert orders_b.json()["data"]["orders"] == []


# ── devui library endpoint ────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_devui_library_empty(auth_client):
    client, token = auth_client
    r = await client.get(
        "/devui/library",
        headers={"Authorization": f"Bearer {token}"},
    )
    assert r.status_code == 200
    data = r.json()["data"]
    for key in ["operatives", "operators", "spores", "widgets"]:
        assert key in data
        assert isinstance(data[key], list)
    assert data["total"] == 0


@pytest.mark.asyncio
async def test_devui_library_groups_by_kind(auth_client):
    client, token = auth_client
    for name, kind in [
        ("Op Alpha", "operative"),
        ("Widget Beta", "widget"),
        ("Spore Gamma", "spore"),
    ]:
        await client.post(
            "/api/v1/arena/packages",
            json={"name": name, "kind": kind},
            headers={"Authorization": f"Bearer {token}"},
        )

    r = await client.get(
        "/devui/library",
        headers={"Authorization": f"Bearer {token}"},
    )
    assert r.status_code == 200
    data = r.json()["data"]
    assert len(data["operatives"]) == 1
    assert len(data["widgets"]) == 1
    assert len(data["spores"]) == 1
    assert data["total"] == 3
