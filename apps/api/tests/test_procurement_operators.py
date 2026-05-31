"""
tests/test_procurement_operators.py

Tests for the procurement operator library (Epic 1.1.3).

Covers:
  - Operator registry: all 4 procurement operators registered with correct metadata
  - Happy paths for all 4 operators
  - Constraint rejection: raise_po with non-PASSED council_status
  - Constraint rejection: broadcast_supply_signal with produce not in inventory
  - Integration test: all four primitives (ConstraintEngine, StateAccessor, EventBus,
    PawaLedger) fire correctly — and do NOT fire on constraint failure
  - Mycelium bridge: broadcast event carries destination='mycelium.public'
  - Delivery confirmation: DELIVERED vs PARTIAL status, variance_kg calculation

Run with: pytest tests/test_procurement_operators.py -v
"""

import pytest
from datetime import datetime

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OperatorContext, OPERATOR_REGISTRY
import sustena.operators  # triggers auto-registration of all operators


# ── Fixtures ───────────────────────────────────────────────────────────────────

def _fresh_state() -> StateAccessor:
    """Procurement sustain state with inventory seeded."""
    return StateAccessor({
        "procurement": {
            "inventory": {
                "maize":   {"quantity_kg": 5000.0, "unit_price": 45.0},
                "beans":   {"quantity_kg": 1000.0, "unit_price": 120.0},
                "tomatoes": {"quantity_kg": 500.0,  "unit_price": 80.0},
            },
            "mkulima_supply_signals": [],
            "purchase_orders": [],
        },
    })


def _make_ctx(state: StateAccessor, user_id: str = "user-test") -> OperatorContext:
    bus = EventBus(sustain_id="test-procurement")
    ledger = PawaLedger()
    return OperatorContext(
        state=state,
        events=bus,
        pawa=ledger,
        sustain_id="test-procurement",
        user_id=user_id,
        operative_id=None,
        timestamp=datetime.utcnow(),
    )


# ── Helpers ────────────────────────────────────────────────────────────────────

async def _broadcast(state: StateAccessor, produce: str = "maize", qty: float = 100.0,
                     price: float = 50.0, window: int = 7) -> dict:
    """Broadcast a supply signal and return result.data."""
    ctx = _make_ctx(state)
    result = await OPERATOR_REGISTRY["mkulima.broadcast_supply_signal"].fn(
        ctx, produce=produce, quantity_kg=qty, price_per_kg=price, harvest_window_days=window,
    )
    assert result.succeeded, f"broadcast_supply_signal failed: {result.reason}"
    return result.data


async def _receive(state: StateAccessor, signal_id: str, produce: str = "maize",
                   qty: float = 100.0, price: float = 50.0, farm_id: str = "farm-001",
                   window: int = 7) -> dict:
    """Receive a supply signal and return result.data."""
    ctx = _make_ctx(state)
    result = await OPERATOR_REGISTRY["mkulima.receive_signal"].fn(
        ctx, signal_id=signal_id, produce=produce, quantity_kg=qty,
        price_per_kg=price, farm_id=farm_id, harvest_window_days=window,
    )
    assert result.succeeded, f"receive_signal failed: {result.reason}"
    return result.data


def _patch_signal_status(state: StateAccessor, signal_id: str, status: str) -> None:
    """Directly patch a signal's council_status — simulates Council resolution."""
    signals: list = state.get("procurement.mkulima_supply_signals", [])
    sig = next((s for s in signals if s.get("id") == signal_id), None)
    if sig is not None:
        sig["council_status"] = status


def _get_signal(state: StateAccessor, signal_id: str) -> dict | None:
    signals: list = state.get("procurement.mkulima_supply_signals", [])
    return next((s for s in signals if s.get("id") == signal_id), None)


def _get_po(state: StateAccessor, po_id: str) -> dict | None:
    pos: list = state.get("procurement.purchase_orders", [])
    return next((p for p in pos if p.get("id") == po_id), None)


# ── Operator registry ──────────────────────────────────────────────────────────

class TestProcurementOperatorRegistry:

    def test_all_procurement_operators_registered(self):
        expected = [
            "mkulima.broadcast_supply_signal",
            "mkulima.receive_signal",
            "procurement.raise_po",
            "procurement.confirm_delivery",
        ]
        for name in expected:
            assert name in OPERATOR_REGISTRY, f"'{name}' not in OPERATOR_REGISTRY"

    def test_operators_have_required_metadata(self):
        for name in [
            "mkulima.broadcast_supply_signal",
            "mkulima.receive_signal",
            "procurement.raise_po",
            "procurement.confirm_delivery",
        ]:
            meta = OPERATOR_REGISTRY[name]
            assert meta.description, f"'{name}' has no description"
            assert isinstance(meta.pawa_cost, int), f"'{name}' pawa_cost not int"
            assert meta.license_tier == "free"
            assert meta.author == "sustena_core"

    def test_side_effects_declared(self):
        assert "event.procurement.supply_signal_broadcast" in \
            OPERATOR_REGISTRY["mkulima.broadcast_supply_signal"].side_effects
        assert "event.biashara.signal_evaluation_requested" in \
            OPERATOR_REGISTRY["mkulima.receive_signal"].side_effects
        assert "event.procurement.po_raised" in \
            OPERATOR_REGISTRY["procurement.raise_po"].side_effects
        assert "event.procurement.delivery_confirmed" in \
            OPERATOR_REGISTRY["procurement.confirm_delivery"].side_effects

    def test_previous_operators_still_registered(self):
        """Regression: adding procurement must not break budget and chama registrations."""
        for name in ["budget.record_income", "budget.allocate", "chama.contribution.record"]:
            assert name in OPERATOR_REGISTRY, f"'{name}' missing after procurement import"


# ── Happy paths ────────────────────────────────────────────────────────────────

class TestBroadcastSupplySignalHappyPath:

    @pytest.mark.asyncio
    async def test_broadcast_returns_signal_id(self):
        state = _fresh_state()
        data = await _broadcast(state, produce="maize", qty=200.0, price=45.0, window=5)

        assert "signal_id" in data
        assert data["produce"] == "maize"
        assert data["quantity_kg"] == 200.0
        assert data["price_per_kg"] == 45.0
        assert data["harvest_window_days"] == 5
        assert data["total_value_kes"] == 9000.0
        assert data["status"] == "broadcast"

    @pytest.mark.asyncio
    async def test_broadcast_includes_mycelium_destination(self):
        state = _fresh_state()
        data = await _broadcast(state)
        assert data["destination"] == "mycelium.public"

    @pytest.mark.asyncio
    async def test_broadcast_fires_event_with_mycelium_tag(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["mkulima.broadcast_supply_signal"].fn(
            ctx, produce="beans", quantity_kg=50.0, price_per_kg=120.0, harvest_window_days=3,
        )
        assert result.succeeded
        published = ctx.events.published_this_context()
        assert len(published) == 1
        event = published[0]
        assert event["event_name"] == "event.procurement.supply_signal_broadcast"
        payload = event["_payload"]
        assert payload["destination"] == "mycelium.public"
        assert payload["produce"] == "beans"
        assert payload["quantity_kg"] == 50.0

    @pytest.mark.asyncio
    async def test_broadcast_no_state_mutation(self):
        """broadcast_supply_signal reads inventory but does not mutate state."""
        state = _fresh_state()
        before_signals = state.get("procurement.mkulima_supply_signals", [])
        before_count = len(before_signals)
        await _broadcast(state)
        after_count = len(state.get("procurement.mkulima_supply_signals", []))
        assert after_count == before_count


class TestReceiveSignalHappyPath:

    @pytest.mark.asyncio
    async def test_receive_signal_appends_to_state(self):
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        data = await _receive(state, signal_id=sid, produce="maize", qty=100.0,
                              price=50.0, farm_id="farm-001", window=7)

        assert data["signal_id"] == sid
        assert data["council_status"] == "IN_VOTING"
        assert data["biashara_evaluation"] == "requested"

        # Check state mutation
        signals = state.get("procurement.mkulima_supply_signals", [])
        assert len(signals) == 1
        signal = signals[0]
        assert signal["id"] == sid
        assert signal["produce"] == "maize"
        assert signal["farm_id"] == "farm-001"
        assert signal["council_status"] == "IN_VOTING"

    @pytest.mark.asyncio
    async def test_receive_signal_fires_two_events(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        import uuid
        sid = str(uuid.uuid4())
        result = await OPERATOR_REGISTRY["mkulima.receive_signal"].fn(
            ctx, signal_id=sid, produce="tomatoes", quantity_kg=30.0,
            price_per_kg=80.0, farm_id="farm-xyz", harvest_window_days=2,
        )
        assert result.succeeded
        published = ctx.events.published_this_context()
        event_names = [e["event_name"] for e in published]
        assert "event.procurement.signal_received" in event_names
        assert "event.biashara.signal_evaluation_requested" in event_names

    @pytest.mark.asyncio
    async def test_biashara_event_carries_sustain_id(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        import uuid
        sid = str(uuid.uuid4())
        await OPERATOR_REGISTRY["mkulima.receive_signal"].fn(
            ctx, signal_id=sid, produce="maize", quantity_kg=100.0,
            price_per_kg=50.0, farm_id="farm-001", harvest_window_days=7,
        )
        biashara_event = next(
            e for e in ctx.events.published_this_context()
            if e["event_name"] == "event.biashara.signal_evaluation_requested"
        )
        assert biashara_event["_payload"]["sustain_id"] == "test-procurement"
        assert biashara_event["_payload"]["signal_id"] == sid

    @pytest.mark.asyncio
    async def test_receive_multiple_signals(self):
        state = _fresh_state()
        import uuid
        for i in range(3):
            await _receive(state, signal_id=str(uuid.uuid4()), produce="maize",
                           qty=100.0 * (i + 1), price=50.0, farm_id=f"farm-{i}", window=7)
        signals = state.get("procurement.mkulima_supply_signals", [])
        assert len(signals) == 3


class TestRaisePOHappyPath:

    @pytest.mark.asyncio
    async def test_raise_po_creates_record(self):
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid, produce="maize", qty=100.0, price=50.0,
                       farm_id="farm-001", window=7)
        _patch_signal_status(state, sid, "PASSED")

        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=100.0, total_kes=5000.0,
        )
        assert result.succeeded, f"raise_po failed: {result.reason}"
        data = result.data
        assert "po_id" in data
        assert data["signal_id"] == sid
        assert data["quantity_kg"] == 100.0
        assert data["total_kes"] == 5000.0
        assert data["status"] == "RAISED"
        assert data["produce"] == "maize"
        assert data["farm_id"] == "farm-001"

    @pytest.mark.asyncio
    async def test_raise_po_appends_to_purchase_orders(self):
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid)
        _patch_signal_status(state, sid, "PASSED")

        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=100.0, total_kes=5000.0,
        )
        assert result.succeeded
        pos = state.get("procurement.purchase_orders", [])
        assert len(pos) == 1
        po = pos[0]
        assert po["id"] == result.data["po_id"]
        assert po["status"] == "RAISED"
        assert po["actual_quantity_kg"] is None

    @pytest.mark.asyncio
    async def test_raise_po_fires_event(self):
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid)
        _patch_signal_status(state, sid, "PASSED")

        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=100.0, total_kes=5000.0,
        )
        assert result.succeeded
        published = ctx.events.published_this_context()
        assert len(published) == 1
        assert published[0]["event_name"] == "event.procurement.po_raised"
        assert published[0]["_payload"]["po_id"] == result.data["po_id"]


class TestConfirmDeliveryHappyPath:

    async def _setup_po(self, state: StateAccessor, qty: float = 100.0) -> str:
        """Helper: receive signal → PASSED → raise PO → return po_id."""
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid, qty=qty)
        _patch_signal_status(state, sid, "PASSED")
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=qty, total_kes=qty * 50.0,
        )
        assert result.succeeded
        return result.data["po_id"]

    @pytest.mark.asyncio
    async def test_full_delivery(self):
        state = _fresh_state()
        po_id = await self._setup_po(state, qty=100.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx, po_id=po_id, actual_quantity_kg=100.0,
        )
        assert result.succeeded
        data = result.data
        assert data["status"] == "DELIVERED"
        assert data["fully_delivered"] is True
        assert data["variance_kg"] == 0.0
        assert data["actual_quantity_kg"] == 100.0
        assert data["expected_quantity_kg"] == 100.0

    @pytest.mark.asyncio
    async def test_partial_delivery(self):
        state = _fresh_state()
        po_id = await self._setup_po(state, qty=100.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx, po_id=po_id, actual_quantity_kg=75.0,
        )
        assert result.succeeded
        data = result.data
        assert data["status"] == "PARTIAL"
        assert data["fully_delivered"] is False
        assert data["variance_kg"] == -25.0

    @pytest.mark.asyncio
    async def test_overdelivery_is_delivered(self):
        """Receiving more than ordered is treated as DELIVERED (surplus)."""
        state = _fresh_state()
        po_id = await self._setup_po(state, qty=100.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx, po_id=po_id, actual_quantity_kg=110.0,
        )
        assert result.succeeded
        assert result.data["status"] == "DELIVERED"
        assert result.data["variance_kg"] == 10.0

    @pytest.mark.asyncio
    async def test_delivery_mutates_po_in_state(self):
        state = _fresh_state()
        po_id = await self._setup_po(state, qty=100.0)
        ctx = _make_ctx(state)
        await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx, po_id=po_id, actual_quantity_kg=80.0,
        )
        po = _get_po(state, po_id)
        assert po is not None
        assert po["status"] == "PARTIAL"
        assert po["actual_quantity_kg"] == 80.0
        assert po["delivered_at"] is not None
        assert po["variance_kg"] == -20.0

    @pytest.mark.asyncio
    async def test_delivery_fires_event(self):
        state = _fresh_state()
        po_id = await self._setup_po(state, qty=100.0)
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx, po_id=po_id, actual_quantity_kg=100.0,
        )
        assert result.succeeded
        published = ctx.events.published_this_context()
        assert len(published) == 1
        event = published[0]
        assert event["event_name"] == "event.procurement.delivery_confirmed"
        payload = event["_payload"]
        assert payload["po_id"] == po_id
        assert payload["actual_quantity_kg"] == 100.0
        assert payload["status"] == "DELIVERED"


# ── Constraint rejections ──────────────────────────────────────────────────────

class TestConstraintRejections:

    @pytest.mark.asyncio
    async def test_broadcast_rejects_produce_not_in_inventory(self):
        """broadcast_supply_signal must fail if produce is not in procurement.inventory."""
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["mkulima.broadcast_supply_signal"].fn(
            ctx, produce="avocado", quantity_kg=100.0, price_per_kg=200.0,
            harvest_window_days=5,
        )
        assert result.failed
        assert result.constraint_violated == "produce_in_inventory"
        assert "avocado" in result.reason
        # EventBus must NOT have fired
        assert len(ctx.events.published_this_context()) == 0

    @pytest.mark.asyncio
    async def test_broadcast_rejects_zero_quantity(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["mkulima.broadcast_supply_signal"].fn(
            ctx, produce="maize", quantity_kg=0.0, price_per_kg=50.0,
            harvest_window_days=7,
        )
        assert result.failed
        assert len(ctx.events.published_this_context()) == 0

    @pytest.mark.asyncio
    async def test_raise_po_rejects_in_voting_signal(self):
        """raise_po must fail when council_status is IN_VOTING."""
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid)
        # Do NOT patch to PASSED — leave as IN_VOTING

        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=100.0, total_kes=5000.0,
        )
        assert result.failed
        assert result.constraint_violated == "signal_council_status_is_passed"
        assert "IN_VOTING" in result.reason
        # No PO must have been created
        assert len(state.get("procurement.purchase_orders", [])) == 0
        # No event fired
        assert len(ctx.events.published_this_context()) == 0

    @pytest.mark.asyncio
    async def test_raise_po_rejects_failed_signal(self):
        """raise_po must fail when council_status is FAILED."""
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid)
        _patch_signal_status(state, sid, "FAILED")

        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=100.0, total_kes=5000.0,
        )
        assert result.failed
        assert result.constraint_violated == "signal_council_status_is_passed"

    @pytest.mark.asyncio
    async def test_raise_po_rejects_nonexistent_signal(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id="does-not-exist", quantity_kg=100.0, total_kes=5000.0,
        )
        assert result.failed
        assert result.constraint_violated == "signal_exists"

    @pytest.mark.asyncio
    async def test_confirm_delivery_rejects_nonexistent_po(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx, po_id="ghost-po-id", actual_quantity_kg=100.0,
        )
        assert result.failed
        assert result.constraint_violated == "po_exists"

    @pytest.mark.asyncio
    async def test_confirm_delivery_rejects_already_delivered_po(self):
        """confirm_delivery must fail if PO is already DELIVERED."""
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid, qty=100.0)
        _patch_signal_status(state, sid, "PASSED")

        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=100.0, total_kes=5000.0,
        )
        po_id = result.data["po_id"]

        # First delivery
        ctx2 = _make_ctx(state)
        r2 = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx2, po_id=po_id, actual_quantity_kg=100.0,
        )
        assert r2.succeeded

        # Second delivery attempt — must fail
        ctx3 = _make_ctx(state)
        r3 = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx3, po_id=po_id, actual_quantity_kg=100.0,
        )
        assert r3.failed
        assert r3.constraint_violated == "po_status_is_raised"

    @pytest.mark.asyncio
    async def test_receive_signal_rejects_zero_price(self):
        state = _fresh_state()
        ctx = _make_ctx(state)
        import uuid
        result = await OPERATOR_REGISTRY["mkulima.receive_signal"].fn(
            ctx, signal_id=str(uuid.uuid4()), produce="maize", quantity_kg=100.0,
            price_per_kg=0.0, farm_id="farm-001", harvest_window_days=7,
        )
        assert result.failed
        assert len(state.get("procurement.mkulima_supply_signals", [])) == 0


# ── Integration test: all four primitives ─────────────────────────────────────

class TestAllFourPrimitivesIntegration:
    """
    End-to-end workflow through all four operators.
    Verifies that ConstraintEngine, StateAccessor, EventBus, and PawaLedger
    all participate correctly in the full procurement lifecycle.
    """

    @pytest.mark.asyncio
    async def test_full_procurement_lifecycle(self):
        """
        Workflow:
          1. broadcast_supply_signal  → Mycelium event fired, no state mutation
          2. receive_signal           → signal in state, 2 events fired
          3. raise_po (after PASSED)  → PO in state, po_raised event fired
          4. confirm_delivery         → PO updated, delivery_confirmed event fired
        """
        state = _fresh_state()
        import uuid

        # ── Step 1: Broadcast ──────────────────────────────────────────────────
        ctx1 = _make_ctx(state)
        r1 = await OPERATOR_REGISTRY["mkulima.broadcast_supply_signal"].fn(
            ctx1, produce="maize", quantity_kg=200.0, price_per_kg=48.0,
            harvest_window_days=10,
        )
        assert r1.succeeded

        # ConstraintEngine: static constraints passed (qty>0, price>0, window>0)
        # EventBus: exactly 1 event published
        assert len(ctx1.events.published_this_context()) == 1
        broadcast_event = ctx1.events.published_this_context()[0]
        assert broadcast_event["_payload"]["destination"] == "mycelium.public"

        # StateAccessor: no signals added (broadcast does not mutate signals list)
        assert len(state.get("procurement.mkulima_supply_signals", [])) == 0

        # PawaLedger: 0-cost operator — pawa balance unchanged (starts at 0, stays 0)
        balance = await ctx1.pawa.get_balance("user-test")
        assert balance == 0

        # ── Step 2: Receive signal ─────────────────────────────────────────────
        signal_id = r1.data["signal_id"]
        ctx2 = _make_ctx(state)
        r2 = await OPERATOR_REGISTRY["mkulima.receive_signal"].fn(
            ctx2, signal_id=signal_id, produce="maize", quantity_kg=200.0,
            price_per_kg=48.0, farm_id="farm-nairobi", harvest_window_days=10,
        )
        assert r2.succeeded

        # StateAccessor: signal now in state
        signals = state.get("procurement.mkulima_supply_signals", [])
        assert len(signals) == 1
        assert signals[0]["id"] == signal_id
        assert signals[0]["council_status"] == "IN_VOTING"

        # EventBus: 2 events (signal_received + biashara evaluation)
        event_names_2 = [e["event_name"] for e in ctx2.events.published_this_context()]
        assert "event.procurement.signal_received" in event_names_2
        assert "event.biashara.signal_evaluation_requested" in event_names_2

        # ── Step 3: Simulate Council PASSED, then raise PO ────────────────────
        _patch_signal_status(state, signal_id, "PASSED")
        assert _get_signal(state, signal_id)["council_status"] == "PASSED"

        ctx3 = _make_ctx(state)
        r3 = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx3, signal_id=signal_id, quantity_kg=200.0, total_kes=9600.0,
        )
        assert r3.succeeded
        po_id = r3.data["po_id"]

        # StateAccessor: PO in state
        pos = state.get("procurement.purchase_orders", [])
        assert len(pos) == 1
        assert pos[0]["status"] == "RAISED"
        assert pos[0]["actual_quantity_kg"] is None

        # EventBus: po_raised event
        event_names_3 = [e["event_name"] for e in ctx3.events.published_this_context()]
        assert "event.procurement.po_raised" in event_names_3

        # ConstraintEngine: a non-PASSED signal would have been rejected (tested separately)

        # ── Step 4: Confirm delivery ───────────────────────────────────────────
        ctx4 = _make_ctx(state)
        r4 = await OPERATOR_REGISTRY["procurement.confirm_delivery"].fn(
            ctx4, po_id=po_id, actual_quantity_kg=195.0,
        )
        assert r4.succeeded

        # StateAccessor: PO updated
        po = _get_po(state, po_id)
        assert po["status"] == "PARTIAL"
        assert po["actual_quantity_kg"] == 195.0
        assert po["variance_kg"] == -5.0
        assert po["delivered_at"] is not None

        # EventBus: delivery_confirmed event
        event_names_4 = [e["event_name"] for e in ctx4.events.published_this_context()]
        assert "event.procurement.delivery_confirmed" in event_names_4

        # PawaLedger: still 0 throughout (all free operators)
        balance_final = await ctx4.pawa.get_balance("user-test")
        assert balance_final == 0

    @pytest.mark.asyncio
    async def test_constraint_failure_blocks_all_primitives(self):
        """
        When constraint fails on raise_po (non-PASSED signal):
          - StateAccessor: no PO created
          - EventBus: no events fired
          - PawaLedger: not charged
        """
        state = _fresh_state()
        import uuid
        sid = str(uuid.uuid4())
        await _receive(state, signal_id=sid)
        # Leave signal at IN_VOTING

        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["procurement.raise_po"].fn(
            ctx, signal_id=sid, quantity_kg=100.0, total_kes=5000.0,
        )

        assert result.failed

        # StateAccessor: no PO created
        assert len(state.get("procurement.purchase_orders", [])) == 0

        # EventBus: no events
        assert len(ctx.events.published_this_context()) == 0

        # PawaLedger: balance still 0 (deduction would have been 0 anyway,
        # but the pawa path is only reached after constraint passes)
        balance = await ctx.pawa.get_balance("user-test")
        assert balance == 0

    @pytest.mark.asyncio
    async def test_mycelium_bridge_event_structure(self):
        """
        The Mycelium bridge event must carry all required fields for the
        Phase 2 relay worker to identify and forward it.
        """
        state = _fresh_state()
        ctx = _make_ctx(state)
        result = await OPERATOR_REGISTRY["mkulima.broadcast_supply_signal"].fn(
            ctx, produce="tomatoes", quantity_kg=300.0, price_per_kg=80.0,
            harvest_window_days=2,
        )
        assert result.succeeded
        event = ctx.events.published_this_context()[0]
        payload = event["_payload"]

        # Required fields for Mycelium relay worker
        required = ["signal_id", "produce", "quantity_kg", "price_per_kg",
                    "harvest_window_days", "total_value_kes", "farm_sustain_id",
                    "destination"]
        for field in required:
            assert field in payload, f"Mycelium event missing field: '{field}'"
        assert payload["destination"] == "mycelium.public"
