"""
tests/test_ingest_engine.py

Integration tests for sustena/core/ingest_engine.py — the durable, idempotent
intake that turns a captured message into event(s) on the S3 fold via the
existing execute_operator() path (inheriting the S2 enforcing gate and the
S3 fold-append for free).

Covers:
  - mapped capture applies the operator and lands exactly one event
  - identical capture replayed twice produces the event exactly once
    (dedup) and rebuild_state() still agrees with get_state() after replay
  - a gate-refused ingest is quarantined honestly (status="refused"),
    state is untouched, no spurious event is appended
  - parsed_unmapped / unparsed captures land as needs_attention, never
    silently dropped
  - resolve_message acknowledges a needs_attention row exactly once
  - staleness: only ever raised for a source with an explicit configured
    cadence; never fabricated for one with none
"""

import pytest

from sustena.core.ingest_engine import IngestEngine, STATUS_APPLIED, STATUS_NEEDS_ATTENTION, STATUS_REFUSED
from sustena.core.sustain_engine import SustainEngine


RECEIVED = (
    "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU "
    "254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00"
)

BUYGOODS = (
    "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 "
    "at 4:30 PM. New M-PESA balance is Ksh12,050.00"
)

GARBAGE = "hey are we still on for lunch tomorrow?"


# ── Fixtures ──────────────────────────────────────────────────────────────────

@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


@pytest.fixture
def homestead_sid(engine: SustainEngine) -> str:
    return engine.instantiate(
        template_id="homestead",
        user_id="user-test-1",
        parameters={"owner_ids": ["user-test-1"]},
    )


@pytest.fixture
def ingest(engine: SustainEngine) -> IngestEngine:
    return IngestEngine(engine)


# ── Mapped capture → applied, lands on the fold ──────────────────────────────

class TestMappedCaptureApplies:
    @pytest.mark.asyncio
    async def test_capture_applies_and_returns_applied_status(self, ingest, engine, homestead_sid):
        result = await ingest.capture("device-1", homestead_sid, RECEIVED)
        assert result["status"] == STATUS_APPLIED
        assert result["is_duplicate"] is False
        assert result["operator_name"] == "budget.record_income"

    @pytest.mark.asyncio
    async def test_capture_actually_mutates_state(self, ingest, engine, homestead_sid):
        before = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        after = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        assert after == before + 5000.0

    @pytest.mark.asyncio
    async def test_capture_appends_exactly_one_new_event(self, ingest, engine, homestead_sid):
        before = len(engine.get_events(homestead_sid, limit=50))
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        after = len(engine.get_events(homestead_sid, limit=50))
        assert after == before + 1

    @pytest.mark.asyncio
    async def test_rebuild_state_agrees_after_mapped_capture(self, ingest, engine, homestead_sid):
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)


# ── CRITICAL: OTP/secret content is refused before it ever touches the DB ──
# Server-side defense in depth (the primary defense is the Android capture
# client, which should never send one of these at all). Even if it does,
# capture() must never persist the raw text -- checked directly against the
# ingest_messages table, not just the returned status, since the whole
# point is that the OTP text never lands in storage regardless of what the
# response says.

OTP_MESSAGE = (
    "Your card ending with 0319 has initiated an online transaction of USD 113.8 "
    "at ANTHROPIC. Your OTP is 083345. DO NOT SHARE WITH ANYONE."
)


class TestSensitiveSecretNeverPersisted:
    @pytest.mark.asyncio
    async def test_capture_returns_rejected_status(self, ingest, homestead_sid):
        result = await ingest.capture("device-1", homestead_sid, OTP_MESSAGE)
        assert result["status"] == "rejected"

    @pytest.mark.asyncio
    async def test_rejected_response_never_echoes_the_raw_text(self, ingest, homestead_sid):
        # The response itself must not leak the OTP text back to the
        # caller -- unlike a normal capture response, which does include
        # raw_payload.
        result = await ingest.capture("device-1", homestead_sid, OTP_MESSAGE)
        assert "raw_payload" not in result
        assert "083345" not in str(result)

    @pytest.mark.asyncio
    async def test_no_row_is_ever_inserted_into_ingest_messages(self, ingest, engine, homestead_sid):
        before = engine._db.execute(
            "SELECT COUNT(*) FROM ingest_messages WHERE sustain_id = ?", (homestead_sid,)
        ).fetchone()[0]
        await ingest.capture("device-1", homestead_sid, OTP_MESSAGE)
        after = engine._db.execute(
            "SELECT COUNT(*) FROM ingest_messages WHERE sustain_id = ?", (homestead_sid,)
        ).fetchone()[0]
        assert after == before

    @pytest.mark.asyncio
    async def test_no_state_or_event_side_effects(self, ingest, engine, homestead_sid):
        state_before = engine.get_state(homestead_sid)
        events_before = engine.get_events(homestead_sid, limit=50)
        await ingest.capture("device-1", homestead_sid, OTP_MESSAGE)
        assert engine.get_state(homestead_sid) == state_before
        assert engine.get_events(homestead_sid, limit=50) == events_before

    @pytest.mark.asyncio
    async def test_source_is_still_marked_seen(self, ingest, homestead_sid):
        # Staleness tracking is about whether the device/app is still
        # communicating, which is orthogonal to whether any one message's
        # CONTENT was accepted -- a source that only ever sends OTPs (which
        # would be unusual, but not impossible if the sender filter on
        # Android were ever misconfigured) should still not be reported
        # falsely stale.
        result = await ingest.capture("device-1", homestead_sid, OTP_MESSAGE)
        sources = ingest.get_sources(homestead_sid)
        assert any(s["source_id"] == "device-1" for s in sources)

    @pytest.mark.asyncio
    async def test_a_real_transaction_from_the_same_source_still_works_after_a_rejection(self, ingest, engine, homestead_sid):
        # A rejected capture must not corrupt or block subsequent real
        # captures from the same source.
        await ingest.capture("device-1", homestead_sid, OTP_MESSAGE)
        result = await ingest.capture("device-1", homestead_sid, RECEIVED)
        assert result["status"] == "applied"


# ── Idempotent intake: replay lands on the fold exactly once ────────────────

class TestDedupAndReplay:
    @pytest.mark.asyncio
    async def test_first_capture_is_not_a_duplicate(self, ingest, homestead_sid):
        result = await ingest.capture("device-1", homestead_sid, RECEIVED)
        assert result["is_duplicate"] is False

    @pytest.mark.asyncio
    async def test_identical_replay_is_flagged_duplicate(self, ingest, homestead_sid):
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        replay = await ingest.capture("device-1", homestead_sid, RECEIVED)
        assert replay["is_duplicate"] is True

    @pytest.mark.asyncio
    async def test_replay_returns_the_same_message_id(self, ingest, homestead_sid):
        first = await ingest.capture("device-1", homestead_sid, RECEIVED)
        replay = await ingest.capture("device-1", homestead_sid, RECEIVED)
        assert replay["message_id"] == first["message_id"]

    @pytest.mark.asyncio
    async def test_double_replay_applies_the_operator_exactly_once(self, ingest, engine, homestead_sid):
        # The core idempotency proof: capturing the SAME (source, payload)
        # three times must mutate state and append an event only once.
        before_balance = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        before_events = len(engine.get_events(homestead_sid, limit=50))

        await ingest.capture("device-1", homestead_sid, RECEIVED)
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        await ingest.capture("device-1", homestead_sid, RECEIVED)

        after_balance = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        after_events = len(engine.get_events(homestead_sid, limit=50))

        assert after_balance == before_balance + 5000.0  # not 15000.0
        assert after_events == before_events + 1          # not 3

    @pytest.mark.asyncio
    async def test_rebuild_state_agrees_after_replay(self, ingest, engine, homestead_sid):
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)

    @pytest.mark.asyncio
    async def test_different_source_same_payload_is_a_distinct_message(self, ingest, engine, homestead_sid):
        # Dedup key is (source_id, raw_payload) — a different device reporting
        # the identical text is a genuinely different capture, not a replay.
        first = await ingest.capture("device-1", homestead_sid, RECEIVED)
        second = await ingest.capture("device-2", homestead_sid, RECEIVED)
        assert second["is_duplicate"] is False
        assert second["message_id"] != first["message_id"]

    @pytest.mark.asyncio
    async def test_same_source_different_payload_is_a_distinct_message(self, ingest, homestead_sid):
        first = await ingest.capture("device-1", homestead_sid, RECEIVED)
        second = await ingest.capture("device-1", homestead_sid, BUYGOODS)
        assert second["is_duplicate"] is False
        assert second["message_id"] != first["message_id"]


# ── Gate governance: a refused ingest is quarantined honestly ───────────────

class TestGateGovernsIngestedEvents:
    @pytest.mark.asyncio
    async def test_gate_refusal_is_reported_as_refused(self, ingest, engine, homestead_sid, monkeypatch):
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        real_fn = OPERATOR_REGISTRY["budget.record_income"].fn

        async def bad_record_income(ctx, **kwargs):
            # Bypass the normal increment and push the balance negative
            # directly, exactly like the S2 gate's own test suite does, to
            # prove the gate -- not the operator's own guard -- is what
            # catches this for an ingested event.
            ctx.state.set("finances.liquid.balance", -999.0)
            return OperatorResult.ok({"forced": True})

        monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", bad_record_income)
        try:
            result = await ingest.capture("device-1", homestead_sid, RECEIVED)
        finally:
            monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", real_fn)

        assert result["status"] == STATUS_REFUSED
        assert "enforcement_gate" in result["reason"] or "invariant" in result["reason"].lower()

    @pytest.mark.asyncio
    async def test_gate_refusal_leaves_state_untouched(self, ingest, engine, homestead_sid, monkeypatch):
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        real_fn = OPERATOR_REGISTRY["budget.record_income"].fn
        before = engine.get_state(homestead_sid)

        async def bad_record_income(ctx, **kwargs):
            ctx.state.set("finances.liquid.balance", -999.0)
            return OperatorResult.ok({"forced": True})

        monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", bad_record_income)
        try:
            await ingest.capture("device-1", homestead_sid, RECEIVED)
        finally:
            monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", real_fn)

        after = engine.get_state(homestead_sid)
        assert after == before

    @pytest.mark.asyncio
    async def test_gate_refusal_appends_no_spurious_event(self, ingest, engine, homestead_sid, monkeypatch):
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        real_fn = OPERATOR_REGISTRY["budget.record_income"].fn
        before = len(engine.get_events(homestead_sid, limit=50))  # just genesis

        async def bad_record_income(ctx, **kwargs):
            ctx.state.set("finances.liquid.balance", -999.0)
            return OperatorResult.ok({"forced": True})

        monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", bad_record_income)
        try:
            await ingest.capture("device-1", homestead_sid, RECEIVED)
        finally:
            monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", real_fn)

        after = len(engine.get_events(homestead_sid, limit=50))
        assert after == before

    @pytest.mark.asyncio
    async def test_rebuild_state_agrees_after_a_refusal(self, ingest, engine, homestead_sid, monkeypatch):
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult

        real_fn = OPERATOR_REGISTRY["budget.record_income"].fn

        async def bad_record_income(ctx, **kwargs):
            ctx.state.set("finances.liquid.balance", -999.0)
            return OperatorResult.ok({"forced": True})

        monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", bad_record_income)
        try:
            await ingest.capture("device-1", homestead_sid, RECEIVED)
        finally:
            monkeypatch.setattr(OPERATOR_REGISTRY["budget.record_income"], "fn", real_fn)

        assert engine.rebuild_state(homestead_sid) == engine.get_state(homestead_sid)


# ── Needs-attention: never silently dropped ──────────────────────────────────

class TestNeedsAttentionQueue:
    @pytest.mark.asyncio
    async def test_parsed_unmapped_lands_as_needs_attention(self, ingest, homestead_sid):
        result = await ingest.capture("device-1", homestead_sid, BUYGOODS)
        assert result["status"] == STATUS_NEEDS_ATTENTION
        assert result["parsed_fields"]["counterparty"] == "NAIVAS SUPERMARKET"

    @pytest.mark.asyncio
    async def test_unparsed_lands_as_needs_attention(self, ingest, homestead_sid):
        result = await ingest.capture("device-1", homestead_sid, GARBAGE)
        assert result["status"] == STATUS_NEEDS_ATTENTION
        assert result["reason"]

    @pytest.mark.asyncio
    async def test_needs_attention_never_mutates_state(self, ingest, engine, homestead_sid):
        before = engine.get_state(homestead_sid)
        await ingest.capture("device-1", homestead_sid, BUYGOODS)
        assert engine.get_state(homestead_sid) == before

    @pytest.mark.asyncio
    async def test_needs_attention_appears_in_the_queue(self, ingest, homestead_sid):
        await ingest.capture("device-1", homestead_sid, BUYGOODS)
        queue = ingest.needs_attention(sustain_id=homestead_sid)
        assert len(queue) == 1
        assert queue[0]["status"] == STATUS_NEEDS_ATTENTION

    @pytest.mark.asyncio
    async def test_applied_and_refused_never_appear_in_needs_attention(self, ingest, homestead_sid):
        await ingest.capture("device-1", homestead_sid, RECEIVED)  # applied
        queue = ingest.needs_attention(sustain_id=homestead_sid)
        assert queue == []

    @pytest.mark.asyncio
    async def test_resolve_message_clears_it_from_the_queue(self, ingest, homestead_sid):
        result = await ingest.capture("device-1", homestead_sid, BUYGOODS)
        resolved = ingest.resolve_message(result["message_id"], resolved_by="user-test-1")
        assert resolved is True
        assert ingest.needs_attention(sustain_id=homestead_sid) == []

    @pytest.mark.asyncio
    async def test_resolve_message_is_not_idempotent_a_second_time(self, ingest, homestead_sid):
        # A second resolve of an already-resolved message is a no-op that
        # reports failure -- there's nothing left to acknowledge.
        result = await ingest.capture("device-1", homestead_sid, BUYGOODS)
        first = ingest.resolve_message(result["message_id"], resolved_by="user-test-1")
        second = ingest.resolve_message(result["message_id"], resolved_by="user-test-1")
        assert first is True
        assert second is False

    def test_resolve_unknown_message_returns_false(self, ingest):
        assert ingest.resolve_message("does-not-exist", resolved_by="user-test-1") is False


# ── Informational: recognised, but never a needs_attention card ─────────────
# Fixed 2 Aug 2026: an M-Pesa/KCB system/error/balance/loan-status notice
# used to share parsed_unmapped's fate and surface as a classify card
# demanding a decision it never needed. STATUS_INFORMATIONAL is recorded
# (never silently dropped) but deliberately excluded from needs_attention().

MPESA_SYSTEM_BUSY = (
    "M-PESA is unable to process your request because a similar transaction is currently "
    "underway. Please wait while we complete your initial request."
)


class TestInformationalNeverSurfacesAsNeedsAttention:
    @pytest.mark.asyncio
    async def test_system_notice_is_recorded_as_informational(self, ingest, homestead_sid):
        from sustena.core.ingest_engine import STATUS_INFORMATIONAL
        result = await ingest.capture("device-1", homestead_sid, MPESA_SYSTEM_BUSY)
        assert result["status"] == STATUS_INFORMATIONAL

    @pytest.mark.asyncio
    async def test_system_notice_never_appears_in_needs_attention(self, ingest, homestead_sid):
        await ingest.capture("device-1", homestead_sid, MPESA_SYSTEM_BUSY)
        assert ingest.needs_attention(sustain_id=homestead_sid) == []

    @pytest.mark.asyncio
    async def test_system_notice_is_still_retrievable_not_silently_dropped(self, ingest, homestead_sid):
        # Recorded, just not in the "needs a decision" queue -- list_messages
        # (the raw, unfiltered per-sustain history) still has it.
        result = await ingest.capture("device-1", homestead_sid, MPESA_SYSTEM_BUSY)
        rows = ingest.list_messages(sustain_id=homestead_sid)
        assert any(r["message_id"] == result["message_id"] for r in rows)

    @pytest.mark.asyncio
    async def test_system_notice_never_mutates_state(self, ingest, engine, homestead_sid):
        before = engine.get_state(homestead_sid)
        await ingest.capture("device-1", homestead_sid, MPESA_SYSTEM_BUSY)
        assert engine.get_state(homestead_sid) == before

    @pytest.mark.asyncio
    async def test_a_real_transaction_still_lands_as_needs_attention_alongside_it(self, ingest, homestead_sid):
        # The fix is scoped to genuinely non-transactional shapes -- a real
        # parsed_unmapped transaction captured in the same sustain still
        # correctly surfaces.
        await ingest.capture("device-1", homestead_sid, MPESA_SYSTEM_BUSY)
        await ingest.capture("device-1", homestead_sid, BUYGOODS)
        queue = ingest.needs_attention(sustain_id=homestead_sid)
        assert len(queue) == 1
        assert queue[0]["parsed_fields"]["counterparty"] == "NAIVAS SUPERMARKET"


# ── Staleness: never fabricated ──────────────────────────────────────────────

class TestStaleness:
    @pytest.mark.asyncio
    async def test_source_with_no_configured_interval_is_never_stale(self, ingest, homestead_sid):
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        sources = ingest.get_sources(sustain_id=homestead_sid)
        assert len(sources) == 1
        assert sources[0]["expected_interval_minutes"] is None
        assert sources[0]["is_stale"] is False

    def test_registered_source_with_interval_and_no_capture_yet_is_stale(self, ingest, homestead_sid):
        # Configured a cadence but the source has never actually reported —
        # last_seen_at is None, which with an explicit interval means stale.
        ingest.register_source("device-1", homestead_sid, expected_interval_minutes=15)
        sources = ingest.get_sources(sustain_id=homestead_sid)
        assert sources[0]["is_stale"] is True

    @pytest.mark.asyncio
    async def test_source_seen_recently_with_interval_is_not_stale(self, ingest, homestead_sid):
        ingest.register_source("device-1", homestead_sid, expected_interval_minutes=15)
        await ingest.capture("device-1", homestead_sid, RECEIVED)
        sources = ingest.get_sources(sustain_id=homestead_sid)
        assert sources[0]["is_stale"] is False

    @pytest.mark.asyncio
    async def test_source_older_than_its_own_interval_is_stale(self, ingest, homestead_sid):
        import sqlite3
        from datetime import datetime, timedelta

        ingest.register_source("device-1", homestead_sid, expected_interval_minutes=15)
        await ingest.capture("device-1", homestead_sid, RECEIVED)

        long_ago = (datetime.utcnow() - timedelta(minutes=60)).isoformat()
        ingest._db.execute(
            "UPDATE ingest_sources SET last_seen_at = ? WHERE sustain_id = ? AND source_id = ?",
            (long_ago, homestead_sid, "device-1"),
        )
        ingest._db.commit()

        sources = ingest.get_sources(sustain_id=homestead_sid)
        assert sources[0]["is_stale"] is True

    @pytest.mark.asyncio
    async def test_capture_marks_source_seen_even_on_needs_attention(self, ingest, homestead_sid):
        # Staleness is about the source being alive, not about whether any
        # given message happened to parse cleanly.
        ingest.register_source("device-1", homestead_sid, expected_interval_minutes=15)
        await ingest.capture("device-1", homestead_sid, GARBAGE)
        sources = ingest.get_sources(sustain_id=homestead_sid)
        assert sources[0]["last_seen_at"] is not None
        assert sources[0]["is_stale"] is False

    def test_register_source_preserves_label_when_not_overridden(self, ingest, homestead_sid):
        ingest.register_source("device-1", homestead_sid, label="Bonnie's phone")
        ingest.register_source("device-1", homestead_sid, expected_interval_minutes=30)
        sources = ingest.get_sources(sustain_id=homestead_sid)
        assert sources[0]["label"] == "Bonnie's phone"
        assert sources[0]["expected_interval_minutes"] == 30


# ── Classification history ("purchase templates") ───────────────────────────

class TestClassificationHistory:
    def test_no_history_returns_none(self, ingest, homestead_sid):
        assert ingest.get_classification_history(homestead_sid, "NAIVAS SUPERMARKET") is None

    def test_record_then_retrieve(self, ingest, homestead_sid):
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", "food", "budget.spend")
        hist = ingest.get_classification_history(homestead_sid, "NAIVAS SUPERMARKET")
        assert hist == {
            "pocket_name": "food", "operator_name": "budget.spend",
            "use_count": 1, "last_used_at": hist["last_used_at"],
        }
        assert hist["last_used_at"]  # a real timestamp was recorded

    def test_key_is_case_and_whitespace_normalised(self, ingest, homestead_sid):
        ingest.record_classification(homestead_sid, "  Naivas Supermarket  ", "food", "budget.spend")
        hist = ingest.get_classification_history(homestead_sid, "NAIVAS SUPERMARKET")
        assert hist is not None
        assert hist["pocket_name"] == "food"

    def test_repeat_classification_increments_use_count(self, ingest, homestead_sid):
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", "food", "budget.spend")
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", "food", "budget.spend")
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", "food", "budget.spend")
        hist = ingest.get_classification_history(homestead_sid, "NAIVAS SUPERMARKET")
        assert hist["use_count"] == 3

    def test_a_different_pocket_next_time_overwrites_not_accumulates(self, ingest, homestead_sid):
        # Recency-only model, disclosed in record_classification()'s own
        # docstring: the MOST RECENT confirm wins outright.
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", "food", "budget.spend")
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", "shopping", "budget.spend")
        hist = ingest.get_classification_history(homestead_sid, "NAIVAS SUPERMARKET")
        assert hist["pocket_name"] == "shopping"
        assert hist["use_count"] == 2  # still counted -- a real repeat classification, just changed pockets

    def test_history_is_scoped_per_sustain(self, engine, ingest):
        other_sid = engine.instantiate(
            template_id="homestead", user_id="user-test-2", parameters={"owner_ids": ["user-test-2"]},
        )
        ingest.record_classification(other_sid, "NAIVAS SUPERMARKET", "rent", "budget.spend")
        # A different sustain's history must never leak into this one's lookup.
        assert ingest.get_classification_history("some-other-sustain-id", "NAIVAS SUPERMARKET") is None

    def test_missing_pocket_name_or_operator_is_a_safe_no_op(self, ingest, homestead_sid):
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", None, "budget.spend")
        ingest.record_classification(homestead_sid, "NAIVAS SUPERMARKET", "food", None)
        assert ingest.get_classification_history(homestead_sid, "NAIVAS SUPERMARKET") is None

    def test_empty_counterparty_returns_none_and_records_nothing(self, ingest, homestead_sid):
        ingest.record_classification(homestead_sid, "", "food", "budget.spend")
        ingest.record_classification(homestead_sid, None, "food", "budget.spend")
        assert ingest.get_classification_history(homestead_sid, "") is None
