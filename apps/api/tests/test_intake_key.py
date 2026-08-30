"""
tests/test_intake_key.py

ING-5 — idempotent intake, host side (core/intake_key.rs is the reference).

    at-least-once ∘ idempotent = exactly-once EFFECT

    key = intrinsic transaction code, where the message carries one
        = hash of the text, where it does not
        ≠ a generated id, ever

Covers:
  - the key shape itself: intrinsic first, text fallback, no third option
  - a reworded re-send of ONE transaction is ONE intake, and does not move
    money a second time
  - a duplicate intake is a no-op and leaves no second row, however it
    is worded
  - the honest limit: a reformat that breaks the parser fails SAFE (a
    question for a person, never a second application of the money)
  - the same fact from two SOURCES stays two intakes (surfaced by
    correlation, never suppressed here)
  - the migration of already-saved captures: nothing lost, nothing merged,
    a would-be collision reported rather than silently collapsed
"""

import pytest

from sustena.core.ingest_engine import IngestEngine, STATUS_APPLIED
from sustena.core.sustain_engine import SustainEngine


RECEIVED = (
    "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU "
    "254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00"
)

BUYGOODS = (
    "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 "
    "at 4:30 PM. New M-PESA balance is Ksh12,050.00"
)

# The same transaction, re-sent with a four-digit year and an appended
# promotional footer. A real thing carriers do, and the whole reason a text
# hash is not enough: it hashes differently and is the same transaction.
RECEIVED_REWORDED = (
    "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU "
    "254712345678 on 20/07/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00. "
    "Transaction cost, Ksh0.00."
)

# A reformat severe enough that the parser no longer recognises the message at
# all. Kept as its own fixture because the honest behaviour there is DIFFERENT
# and worth pinning: see test_a_reformat_that_breaks_the_parser_fails_safe.
RECEIVED_UNRECOGNISABLE = (
    "QGH7XJ2K9L confirmed. You have received Ksh5,000.00 from JOHN KAMAU "
    "254712345678 on 20/07/2026 at 14:15."
)

GARBAGE = "hey are we still on for lunch tomorrow?"

AMOUNT_5000 = {"amount": 5000.0}


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


# ── The key itself ────────────────────────────────────────────────────────────

class TestTheKeyShape:
    def test_a_message_with_a_code_and_an_amount_is_keyed_on_the_fact(self):
        key = IngestEngine._intake_key(
            "sustain-1", "mpesa", RECEIVED,
            external_ref="QGH7XJ2K9L", parsed_fields=AMOUNT_5000,
        )
        assert key == "i:sustain-1:mpesa:QGH7XJ2K9L:500000"

    def test_a_message_without_a_code_falls_back_to_the_text(self):
        assert IngestEngine._intake_key("sustain-1", "mpesa", GARBAGE).startswith(
            "t:sustain-1:mpesa:"
        )

    def test_a_parsed_amount_but_no_reference_still_falls_back(self):
        # Half a fact is not a fact. Keying on the amount alone would collapse
        # two genuinely separate payments that happened to be the same size.
        key = IngestEngine._intake_key(
            "s", "mpesa", GARBAGE, external_ref=None, parsed_fields={"amount": 500.0}
        )
        assert key.startswith("t:")

    def test_a_reference_but_no_parsed_amount_still_falls_back(self):
        key = IngestEngine._intake_key("s", "mpesa", GARBAGE, external_ref="ABC")
        assert key.startswith("t:")

    def test_the_amount_is_minor_units_not_a_float(self):
        # 0.1 + 0.2 is not 0.3, and a key that disagrees with itself by a
        # rounding error is worse than no key at all.
        key = IngestEngine._intake_key(
            "s", "mpesa", "x", external_ref="ABC", parsed_fields={"amount": 0.1 + 0.2}
        )
        assert key.endswith(":30")

    def test_an_unreadable_amount_falls_back_rather_than_guessing(self):
        key = IngestEngine._intake_key(
            "s", "mpesa", GARBAGE, external_ref="ABC", parsed_fields={"amount": "lots"}
        )
        assert key.startswith("t:")

    def test_the_same_text_in_two_sustains_is_two_intakes(self):
        # A capture client identifies its source; sustain_id is supplied per
        # call, so nothing stops one source feeding two sustains. Without it,
        # the second sustain's capture is silently swallowed by the first.
        a = IngestEngine._intake_key("sustain-a", "mpesa", RECEIVED, "REF1", AMOUNT_5000)
        b = IngestEngine._intake_key("sustain-b", "mpesa", RECEIVED, "REF1", AMOUNT_5000)
        assert a != b

    def test_the_same_fact_from_two_sources_is_two_intakes_not_one(self):
        # Suppressing across sources would be the more dangerous bug: a
        # genuinely separate transaction sharing a reference would vanish
        # without trace. Cross-source is SURFACED by correlation, never
        # suppressed here.
        a = IngestEngine._intake_key("s", "mpesa", RECEIVED, "REF1", AMOUNT_5000)
        b = IngestEngine._intake_key("s", "kcb", RECEIVED, "REF1", AMOUNT_5000)
        assert a != b

    def test_one_reference_at_two_amounts_is_two_intakes(self):
        # Where this key and _fact_key deliberately part company. Within one
        # sender the same parser read both amounts, so a reference collision
        # (a truncated code, a reversal pair reusing a reference) must not
        # swallow a genuinely separate transaction.
        a = IngestEngine._intake_key("s", "mpesa", "x", "REF1", {"amount": 500.0})
        b = IngestEngine._intake_key("s", "mpesa", "y", "REF1", {"amount": 900.0})
        assert a != b

    def test_a_text_hash_would_genuinely_have_let_the_resend_through(self):
        # Worth asserting rather than claiming: if the two texts hashed the
        # same, this whole row would be buying nothing.
        assert IngestEngine._intake_key("s", "mpesa", RECEIVED) != IngestEngine._intake_key(
            "s", "mpesa", RECEIVED_REWORDED
        )


# ── On the real capture path ──────────────────────────────────────────────────

class TestIntakeOnTheRealPath:
    @pytest.mark.asyncio
    async def test_a_reworded_resend_is_one_intake_and_applies_once(
        self, ingest, engine, homestead_sid
    ):
        # The gap the row names, on the real path: a text hash partitions by
        # WORDING, so a reformatted date used to apply the money twice.
        first = await ingest.capture("mpesa", homestead_sid, RECEIVED)
        assert first["status"] == STATUS_APPLIED
        before = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]

        again = await ingest.capture("mpesa", homestead_sid, RECEIVED_REWORDED)
        assert again["is_duplicate"] is True
        assert again["message_id"] == first["message_id"]

        after = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        assert after == before, "the reworded re-send must not have moved money"

    @pytest.mark.asyncio
    async def test_a_duplicate_intake_leaves_no_second_row_however_it_is_worded(
        self, ingest, homestead_sid
    ):
        # A duplicate is a NO-OP, and the wording does not change that. The
        # byte-identical repeat already left no row; a reworded repeat of the
        # same fact is the same intake and is treated identically, rather
        # than being a near-miss that gets recorded and then reconciled.
        #
        # Cross-source is the case that genuinely warrants keeping a row --
        # two senders describing one transfer are two real notifications --
        # and _find_prior_fact still keeps and links those.
        first = await ingest.capture("mpesa", homestead_sid, RECEIVED)
        await ingest.capture("mpesa", homestead_sid, RECEIVED_REWORDED)

        rows = ingest.list_messages(sustain_id=homestead_sid, limit=50)
        assert len(rows) == 1
        assert rows[0]["message_id"] == first["message_id"]

    @pytest.mark.asyncio
    async def test_a_reformat_that_breaks_the_parser_fails_safe(
        self, ingest, engine, homestead_sid
    ):
        # The honest limit of the intrinsic key, pinned rather than glossed:
        # it can only key on a fact the parser could read. A reformat severe
        # enough to break parsing has no code and no amount to key on, so it
        # falls back to the text and is NOT recognised as the same intake.
        #
        # The failure mode is the safe one, and that is the point: an
        # unreadable message becomes a question for a person, never a second
        # application of the money. It shows up as a new capture awaiting a
        # decision, which is exactly what somebody should see when their
        # carrier has changed a format.
        first = await ingest.capture("mpesa", homestead_sid, RECEIVED)
        before = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]

        again = await ingest.capture("mpesa", homestead_sid, RECEIVED_UNRECOGNISABLE)
        assert again["is_duplicate"] is False
        assert again["status"] == "needs_attention"
        assert again["message_id"] != first["message_id"]

        after = engine.get_state(homestead_sid)["finances"]["liquid"]["balance"]
        assert after == before, "an unreadable message must not move money by itself"

    @pytest.mark.asyncio
    async def test_a_byte_identical_repeat_never_reaches_the_parser(
        self, ingest, homestead_sid
    ):
        # The cheap check still runs first, so a flaky client retrying the
        # same POST does no parsing work and leaves no second row.
        first = await ingest.capture("mpesa", homestead_sid, RECEIVED)
        again = await ingest.capture("mpesa", homestead_sid, RECEIVED)
        assert again["is_duplicate"] is True
        assert again["message_id"] == first["message_id"]
        assert len(ingest.list_messages(sustain_id=homestead_sid, limit=50)) == 1

    @pytest.mark.asyncio
    async def test_two_different_transactions_stay_two_intakes(
        self, ingest, homestead_sid
    ):
        await ingest.capture("mpesa", homestead_sid, RECEIVED)
        second = await ingest.capture("mpesa", homestead_sid, BUYGOODS)
        assert second["is_duplicate"] is False
        assert len(ingest.list_messages(sustain_id=homestead_sid, limit=50)) == 2

    @pytest.mark.asyncio
    async def test_an_unparseable_capture_keeps_its_text_key(
        self, ingest, engine, homestead_sid
    ):
        # Nothing to key on, and the key says so rather than implying a
        # guarantee it does not have.
        result = await ingest.capture("mpesa", homestead_sid, GARBAGE)
        row = engine._db.execute(
            "SELECT dedup_key FROM ingest_messages WHERE id = ?", (result["message_id"],)
        ).fetchone()
        assert row["dedup_key"].startswith("t:")

    @pytest.mark.asyncio
    async def test_a_parsed_capture_ends_up_keyed_on_the_fact(
        self, ingest, engine, homestead_sid
    ):
        result = await ingest.capture("mpesa", homestead_sid, RECEIVED)
        row = engine._db.execute(
            "SELECT dedup_key FROM ingest_messages WHERE id = ?", (result["message_id"],)
        ).fetchone()
        assert row["dedup_key"].startswith("i:")
        assert "QGH7XJ2K9L" in row["dedup_key"]


# ── Migrating captures already saved the old way ──────────────────────────────

class TestMigration:
    @pytest.mark.asyncio
    async def test_a_dry_run_changes_nothing(self, ingest, homestead_sid):
        # A migration that touches real saved data should have to be asked
        # for twice.
        await ingest.capture("mpesa", homestead_sid, RECEIVED)
        before = ingest.list_messages(sustain_id=homestead_sid, limit=50)
        report = ingest.migrate_intake_keys(dry_run=True)
        assert report["dry_run"] is True and report["applied"] is False
        assert ingest.list_messages(sustain_id=homestead_sid, limit=50) == before

    @pytest.mark.asyncio
    async def test_no_capture_is_lost_and_every_key_stays_distinct(
        self, ingest, homestead_sid
    ):
        for text in (RECEIVED, BUYGOODS, GARBAGE):
            await ingest.capture("mpesa", homestead_sid, text)
        report = ingest.migrate_intake_keys(dry_run=False)
        assert report["rows_before"] == report["rows_after"] == 3
        assert report["distinct_keys_after"] == 3
        assert report["intact"] is True

    @pytest.mark.asyncio
    async def test_migrating_twice_is_a_no_op(self, ingest, homestead_sid):
        await ingest.capture("mpesa", homestead_sid, RECEIVED)
        ingest.migrate_intake_keys(dry_run=False)
        again = ingest.migrate_intake_keys(dry_run=False)
        assert again["to_rewrite"] == 0
        assert again["intact"] is True

    @pytest.mark.asyncio
    async def test_a_row_the_old_key_let_through_twice_is_reported_not_merged(
        self, ingest, engine, homestead_sid
    ):
        # Both rows already exist and one of them may already have moved
        # money. Retro-suppressing the later one would be the migration
        # deciding something about somebody's money it is not entitled to
        # decide — so it reports the pair and leaves both keyed distinctly.
        await ingest.capture("mpesa", homestead_sid, RECEIVED)
        engine._db.execute(
            "INSERT INTO ingest_messages (id, dedup_key, source_id, sustain_id, "
            "raw_payload, received_at, status, external_ref, parsed_fields_json) "
            "VALUES (?,?,?,?,?,?,?,?,?)",
            ("legacy-dupe", "old-style-hash", "mpesa", homestead_sid,
             RECEIVED_REWORDED, "2026-07-20T14:15:00", STATUS_APPLIED,
             "QGH7XJ2K9L", '{"amount": 5000.0}'),
        )
        engine._db.commit()

        report = ingest.migrate_intake_keys(dry_run=False)
        assert len(report["collisions"]) == 1
        assert "legacy-dupe" in report["collisions"][0]["message_ids"]
        assert report["rows_before"] == report["rows_after"] == 2
        assert report["intact"] is True, "both kept, both still distinctly keyed"
        assert report["held_back_on_text_key"] == 2

    @pytest.mark.asyncio
    async def test_a_migrated_row_still_dedupes_a_later_repeat(
        self, ingest, engine, homestead_sid
    ):
        # The point of migrating at all: an old capture must still recognise
        # a re-send that arrives after the migration.
        first = await ingest.capture("mpesa", homestead_sid, RECEIVED)
        engine._db.execute(
            "UPDATE ingest_messages SET dedup_key = 'old-style-hash' WHERE id = ?",
            (first["message_id"],),
        )
        engine._db.commit()

        ingest.migrate_intake_keys(dry_run=False)

        again = await ingest.capture("mpesa", homestead_sid, RECEIVED_REWORDED)
        assert again["is_duplicate"] is True
        assert again["message_id"] == first["message_id"]
