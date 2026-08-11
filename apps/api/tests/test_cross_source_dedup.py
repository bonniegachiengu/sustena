"""
tests/test_cross_source_dedup.py

The property: one real-world transaction must move money exactly once, however
many senders announce it.

THE BUG

Bonnie's phone genuinely receives two SMS for a single transfer — one from KCB,
one from Safaricom M-PESA. dedup_key fingerprints the MESSAGE (sustain +
source + raw text), so the two notifications hashed differently, both were
applied, and the amount was counted twice.

THE FIX, per RECEPTOR/Ingest

That paper names the remedy exactly: cross-source correlation needs "a key that
is a property of the fact rather than of the message". The M-PESA transaction
code is that key — verified against Bonnie's real texts, both notifications
carry the same code.

The message texts below are the real shapes captured from his device.
"""

import pytest

from sustena.core.ingest_engine import (
    STATUS_APPLIED,
    STATUS_DUPLICATE_FACT,
    IngestEngine,
)
from sustena.core.sustain_engine import SustainEngine

# One transfer, as announced by each sender. Same M-PESA code: UH1B91GYNW.
KCB_ALERT = (
    "Ksh 40000.00 sent to KCB Pay Bill 522522 for account 135***5140 "
    "BONVENTURE NGUGI MAINA has been received on 01/08/2026 at 12:21 PM. "
    "M-PESA ref UH1B91GYNW. Avail balance KES 59,055.00"
)
MPESA_ALERT = (
    "UH1B91GYNW Confirmed. Ksh40,000.00 sent to KCB PAYBILL for account "
    "135***5140 on 1/8/26 at 12:21 PM. New M-PESA balance is Ksh12,154.47. "
    "Transaction cost, Ksh0.00."
)


@pytest.fixture
def setup():
    engine = SustainEngine(db_path=":memory:")
    ingest = IngestEngine(engine)
    sustain_id = engine.instantiate(
        "homestead", "u1", {"household_name": "T", "owner_ids": ["u1"]}
    )
    return engine, ingest, sustain_id


def _balance(engine, sustain_id):
    return engine.get_state(sustain_id)["finances"]["liquid"]["balance"]


class TestCrossSourceDoubleCount:

    @pytest.mark.asyncio
    async def test_one_transfer_two_senders_counts_once(self, setup):
        engine, ingest, sid = setup

        first = await ingest.capture("kcb", sid, KCB_ALERT)
        assert first["status"] == STATUS_APPLIED
        after_first = _balance(engine, sid)
        assert after_first == 40000.0

        second = await ingest.capture("mpesa", sid, MPESA_ALERT)

        assert second["is_duplicate"] is True
        assert second["status"] == STATUS_DUPLICATE_FACT
        assert _balance(engine, sid) == after_first, "the money was counted twice"

    @pytest.mark.asyncio
    async def test_the_duplicate_is_recorded_and_linked_not_dropped(self, setup):
        """Retention rule: every captured raw message is kept."""
        engine, ingest, sid = setup

        first = await ingest.capture("kcb", sid, KCB_ALERT)
        second = await ingest.capture("mpesa", sid, MPESA_ALERT)

        assert second["raw_payload"] == MPESA_ALERT
        assert second["duplicate_of"] == first["message_id"]
        # The reason must name the other source, so a person can see why this
        # notification moved nothing.
        assert "kcb" in second["reason"].lower()
        assert "UH1B91GYNW" in second["reason"]

    @pytest.mark.asyncio
    async def test_order_does_not_matter(self, setup):
        """
        The harder direction, and the one that hid a latent double-count.

        The M-PESA alert for an outbound payment parses as "needs a pocket
        decision", so it records nothing on arrival. If the later KCB alert
        simply applied and left that one queued, classifying it by hand a day
        later would apply the same money a second time — the double-count
        merely delayed.

        So the queued notification is retired when another message records the
        same transaction. Asserted against final stored state, not the
        capture-time return value, because the supersede happens to the earlier
        row after its own capture call has already returned.
        """
        engine, ingest, sid = setup

        await ingest.capture("mpesa", sid, MPESA_ALERT)   # arrives first, undecided
        await ingest.capture("kcb", sid, KCB_ALERT)       # arrives second, applies

        rows = engine._db.execute(
            "SELECT source_id, status, duplicate_of FROM ingest_messages "
            "WHERE sustain_id = ? ORDER BY received_at",
            (sid,),
        ).fetchall()
        by_source = {r[0]: (r[1], r[2]) for r in rows}

        assert by_source["kcb"][0] == STATUS_APPLIED
        assert by_source["mpesa"][0] == STATUS_DUPLICATE_FACT, (
            "the queued notification must be retired, or classifying it later "
            "double-counts the same transfer"
        )
        assert by_source["mpesa"][1] is not None, "it must be linked to the winner"

        assert _balance(engine, sid) == 40000.0
        assert len(ingest.needs_attention(sid)) == 0, (
            "one transaction must never leave two decisions queued"
        )

    @pytest.mark.asyncio
    async def test_a_genuinely_different_transaction_still_applies(self, setup):
        """
        The dangerous failure mode of any dedup: suppressing something real.
        A different transaction has a different code and must not be swallowed.
        """
        engine, ingest, sid = setup

        await ingest.capture("kcb", sid, KCB_ALERT)
        other = KCB_ALERT.replace("UH1B91GYNW", "ZZ9Q00XABC").replace(
            "Ksh 40000.00", "Ksh 1500.00"
        )
        result = await ingest.capture("kcb", sid, other)

        assert result["status"] != STATUS_DUPLICATE_FACT
        assert _balance(engine, sid) == 41500.0

    @pytest.mark.asyncio
    async def test_messages_without_a_reference_are_never_correlated(self, setup):
        """
        No reference means "cannot tell". Guessing from amount and timing would
        suppress real repeat payments — two identical fares in one day, a
        standing order — which is worse than the double-count it prevents.
        """
        engine, ingest, sid = setup

        no_ref = "You bought Ksh50.00 of airtime on 1/8/26 at 11:15 PM. New M-PESA balance is Ksh13,654.47."
        first = await ingest.capture("mpesa", sid, no_ref)
        second = await ingest.capture(
            "mpesa", sid, no_ref.replace("11:15 PM", "11:47 PM")
        )
        assert second["status"] != STATUS_DUPLICATE_FACT

    @pytest.mark.asyncio
    async def test_correlation_is_scoped_to_one_sustain(self, setup):
        """A different household's identical transfer is a different fact."""
        engine, ingest, sid = setup
        other_sid = engine.instantiate(
            "homestead", "u2", {"household_name": "Other", "owner_ids": ["u2"]}
        )

        await ingest.capture("kcb", sid, KCB_ALERT)
        result = await ingest.capture("kcb", other_sid, KCB_ALERT)

        assert result["status"] != STATUS_DUPLICATE_FACT
        assert _balance(engine, other_sid) == 40000.0

    @pytest.mark.asyncio
    async def test_the_fold_still_holds_after_a_correlated_duplicate(self, setup):
        engine, ingest, sid = setup
        await ingest.capture("kcb", sid, KCB_ALERT)
        await ingest.capture("mpesa", sid, MPESA_ALERT)
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    @pytest.mark.asyncio
    async def test_duplicate_does_not_appear_in_the_attention_queue(self, setup):
        """Nothing about it needs a human decision."""
        engine, ingest, sid = setup
        await ingest.capture("kcb", sid, KCB_ALERT)
        second = await ingest.capture("mpesa", sid, MPESA_ALERT)

        pending_ids = {m["message_id"] for m in ingest.needs_attention(sid)}
        assert second["message_id"] not in pending_ids


class TestFactKey:

    def test_none_without_a_reference(self):
        assert IngestEngine._fact_key("s1", None) is None
        assert IngestEngine._fact_key("s1", "   ") is None

    def test_case_and_whitespace_insensitive(self):
        assert IngestEngine._fact_key("s1", "uh1b91gynw") == \
               IngestEngine._fact_key("s1", "  UH1B91GYNW ")

    def test_different_sustains_never_collide(self):
        assert IngestEngine._fact_key("s1", "REF1") != IngestEngine._fact_key("s2", "REF1")

    def test_different_references_never_collide(self):
        assert IngestEngine._fact_key("s1", "REF1") != IngestEngine._fact_key("s1", "REF2")


class TestSchemaMigration:

    def test_adding_the_columns_to_an_older_table_is_idempotent(self):
        """
        CREATE TABLE IF NOT EXISTS never adds a column to an existing table —
        the schema-drift bug that already bit egress_outbox once.
        """
        engine = SustainEngine(db_path=":memory:")
        ingest = IngestEngine(engine)
        ingest._migrate_fact_key_columns()
        ingest._migrate_fact_key_columns()

        columns = {
            r[1] for r in engine._db.execute("PRAGMA table_info(ingest_messages)").fetchall()
        }
        assert "fact_key" in columns
        assert "duplicate_of" in columns
