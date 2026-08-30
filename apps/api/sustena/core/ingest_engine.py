"""
sustena/core/ingest_engine.py

The ingest pipeline (Slice 4) — turns a captured raw message into event(s)
on the S3 fold, durably and idempotently.

    capture(source_id, sustain_id, raw_payload)
        -> dedup check (inbox pattern: same source+payload seen before? ->
           return the SAME recorded outcome, do nothing new)
        -> parse_message()  (the transducer, sustena/core/transducer.py)
        -> mapped?   execute_operator()  -- inherits the S2 enforcing gate
                     and the S3 fold-append for free; refused just as
                     honestly as any other operator call.
        -> unmapped/unparsed?  recorded as needs_attention -- never
           silently dropped.

IngestEngine wraps a SustainEngine and reuses its own sqlite3 connection
(self._sustain_engine._db) rather than opening a second one — capture and
the operator execution it triggers need to see the same live database, and
this avoids any cross-connection consistency question entirely.

Async because execute_operator is async (operator functions are async) --
this is a different class from SustainEngine, so CLAUDE.md's "don't add
async to SustainEngine methods" doesn't apply here; it's a first-class
reason for IngestEngine to exist as its own thing instead of more methods
bolted onto SustainEngine.
"""

from __future__ import annotations

import hashlib
import json
import logging
import uuid
from datetime import datetime, timedelta
from typing import Any

from sustena.core.sustain_engine import SustainEngine
from sustena.core.transducer import contains_sensitive_secret, parse_message

logger = logging.getLogger(__name__)

# Terminal statuses a stored ingest_messages row can hold.
STATUS_APPLIED = "applied"
STATUS_REFUSED = "refused"
STATUS_NEEDS_ATTENTION = "needs_attention"
# A message the transducer recognised but which genuinely isn't a
# transaction (a balance inquiry, a loan-status notice, an M-Pesa/KCB
# system/error notice) -- recorded (never silently dropped) but deliberately
# excluded from needs_attention()'s query below, since nothing about it
# needs a human decision. See transducer.py's own "informational" tier
# docstring for the 2 Aug 2026 fix this closes.
STATUS_INFORMATIONAL = "informational"

# NOT a stored row status -- a "rejected" capture is never inserted into
# ingest_messages at all (see capture()'s own docstring for why). Exists as
# a constant purely so callers/tests compare against the same string
# capture() actually returns, not a stored-row terminal state.
STATUS_REJECTED = "rejected"

# The same real-world transaction, notified twice by two DIFFERENT senders.
#
# Distinct from a plain duplicate capture (the identical message re-POSTed,
# caught by dedup_key): this is a different message, correctly parsed, that
# describes a fact already recorded. Bonnie's KCB and M-Pesa alerts genuinely
# both fire for one transfer -- verified: both carry the same M-PESA code --
# so without this the money was counted twice.
#
# Recorded as a real row rather than dropped: the raw text is retained like
# every other capture, and the person can see why it moved no money a second
# time. Excluded from needs_attention(), since nothing needs deciding.
STATUS_DUPLICATE_FACT = "duplicate_fact"


class IngestEngine:
    def __init__(self, sustain_engine: SustainEngine) -> None:
        self._sustain_engine = sustain_engine
        self._db = sustain_engine._db  # same live connection SustainEngine uses
        self._ensure_tables()

    # ── Table management ───────────────────────────────────────────────────────

    def _ensure_tables(self) -> None:
        self._db.executescript("""
            CREATE TABLE IF NOT EXISTS ingest_sources (
                sustain_id                  TEXT NOT NULL,
                source_id                   TEXT NOT NULL,
                label                       TEXT,
                expected_interval_minutes   INTEGER,
                last_seen_at                TEXT,
                created_at                  TEXT NOT NULL,
                PRIMARY KEY (sustain_id, source_id)
            );

            CREATE TABLE IF NOT EXISTS ingest_messages (
                id                     TEXT PRIMARY KEY,
                dedup_key              TEXT NOT NULL UNIQUE,
                source_id              TEXT NOT NULL,
                sustain_id             TEXT NOT NULL,
                raw_payload            TEXT NOT NULL,
                received_at            TEXT NOT NULL,
                status                 TEXT NOT NULL,
                parser_name            TEXT,
                external_ref           TEXT,
                parsed_fields_json     TEXT,
                operator_name          TEXT,
                operator_params_json   TEXT,
                outcome_json           TEXT,
                reason                 TEXT,
                resolved_at            TEXT,
                resolved_by            TEXT,
                fact_key               TEXT,
                duplicate_of           TEXT
            );

            CREATE TABLE IF NOT EXISTS capture_classification_history (
                sustain_id           TEXT NOT NULL,
                counterparty_key     TEXT NOT NULL,
                counterparty_label   TEXT,
                pocket_name          TEXT NOT NULL,
                operator_name        TEXT NOT NULL,
                use_count            INTEGER NOT NULL DEFAULT 1,
                last_used_at         TEXT NOT NULL,
                PRIMARY KEY (sustain_id, counterparty_key)
            );
        """)
        self._migrate_fact_key_columns()
        self._db.execute(
            "CREATE INDEX IF NOT EXISTS idx_ingest_fact_key "
            "ON ingest_messages (sustain_id, fact_key)"
        )
        self._db.commit()

    def _migrate_fact_key_columns(self) -> None:
        """
        Add fact_key / duplicate_of to an ingest_messages table that predates
        them.

        CREATE TABLE IF NOT EXISTS never adds a column to a table that already
        exists, so a database created before cross-source correlation would
        otherwise keep the old shape and fail on INSERT — the same schema-drift
        bug already hit once on egress_outbox. Additive and idempotent:
        detection is PRAGMA table_info, not a failing SELECT (a failed SELECT
        invalidates the surrounding transaction).
        """
        existing = {
            row[1] for row in self._db.execute("PRAGMA table_info(ingest_messages)").fetchall()
        }
        for column in ("fact_key", "duplicate_of"):
            if column not in existing:
                self._db.execute(f"ALTER TABLE ingest_messages ADD COLUMN {column} TEXT")

    # ── Classification history ("purchase templates") ──────────────────────────
    # A human classifying a capture from the same merchant/counterparty twice
    # is real, recurring friction -- the field surface (Orchie) exists so this
    # is handled the moment it happens, not deferred to later, and remembering
    # "last time NAIVAS was 'food'" is exactly the kind of least-friction
    # pre-fill that matters in that moment. Keyed by a normalised counterparty
    # string per sustain (never globally -- one household's "NAIVAS" mapping
    # to 'food' says nothing about another's pockets). This is advisory only:
    # effect_capture.infer() treats a history match as a strong pre-fill, not
    # a bypass of the human tap -- the CONFIRM step is untouched, and the
    # frontend always offers a CHANGE affordance so a stale mapping is never
    # sticky. Recency-only model (most recent confirm wins outright, not a
    # weighted vote across history) -- honest and simple; a merchant that
    # genuinely spans two pockets will just need re-confirming each time it
    # switches, which is a real, disclosed limitation, not hidden complexity.

    @staticmethod
    def _classification_key(counterparty: str | None) -> str:
        return (counterparty or "").strip().upper()

    def get_classification_history(self, sustain_id: str, counterparty: str | None) -> dict | None:
        key = self._classification_key(counterparty)
        if not key:
            return None
        row = self._db.execute(
            "SELECT pocket_name, operator_name, use_count, last_used_at "
            "FROM capture_classification_history WHERE sustain_id = ? AND counterparty_key = ?",
            (sustain_id, key),
        ).fetchone()
        if row is None:
            return None
        return {
            "pocket_name": row["pocket_name"],
            "operator_name": row["operator_name"],
            "use_count": row["use_count"],
            "last_used_at": row["last_used_at"],
        }

    def record_classification(
        self, sustain_id: str, counterparty: str | None, pocket_name: str | None, operator_name: str | None,
    ) -> None:
        """Best-effort: called after a real, successful capture confirm so the
        NEXT capture from this counterparty can pre-fill. Never raises -- a
        template-memory write failing must never be mistaken for the actual
        transaction (already committed by execute_operator before this is
        ever called) having failed."""
        key = self._classification_key(counterparty)
        if not key or not pocket_name or not operator_name:
            return
        now = datetime.utcnow().isoformat()
        self._db.execute(
            "INSERT INTO capture_classification_history "
            "(sustain_id, counterparty_key, counterparty_label, pocket_name, operator_name, use_count, last_used_at) "
            "VALUES (?, ?, ?, ?, ?, 1, ?) "
            "ON CONFLICT(sustain_id, counterparty_key) DO UPDATE SET "
            "counterparty_label = excluded.counterparty_label, pocket_name = excluded.pocket_name, "
            "operator_name = excluded.operator_name, use_count = use_count + 1, last_used_at = excluded.last_used_at",
            (sustain_id, key, counterparty, pocket_name, operator_name, now),
        )
        self._db.commit()

    def purge_sustain_data(self, sustain_id: str, owner_user_id: str) -> dict:
        """
        Permanently clear every row this engine owns for ONE sustain --
        ingest_sources, ingest_messages (including their dedup_key
        fingerprints, so a re-sync genuinely re-captures rather than being
        silently treated as a duplicate of pre-wipe history), and
        capture_classification_history ("remembered" pocket mappings).

        A user-requested clean-slate reset (2 Aug 2026): resetting
        SustainEngine's own state/events without ALSO clearing this data
        would leave every old captured SMS still marked resolved against
        pockets that no longer exist, and every dedup_key still blocking a
        fresh re-capture of the exact same real message text -- exactly
        the stale residue a "truly fresh start" is asking to be rid of.

        Ownership-checked via the SAME live `sustains` table SustainEngine
        itself uses (this engine shares its connection) -- refuses (does
        nothing) rather than silently purging the wrong sustain's data.

        Returns {"status": "purged"|"not_found"|"owner_mismatch",
                 "sustain_id", "table_counts": {...}}.
        """
        row = self._db.execute("SELECT user_id FROM sustains WHERE id = ?", (sustain_id,)).fetchone()
        if row is None:
            return {"status": "not_found", "sustain_id": sustain_id, "table_counts": {}}
        if row["user_id"] != owner_user_id:
            return {"status": "owner_mismatch", "sustain_id": sustain_id, "table_counts": {}}

        table_counts: dict[str, int] = {}
        for table in ("ingest_messages", "ingest_sources", "capture_classification_history"):
            cur = self._db.execute(f"DELETE FROM {table} WHERE sustain_id = ?", (sustain_id,))
            table_counts[table] = cur.rowcount
        self._db.commit()

        logger.info(
            "[IngestEngine] purged sustain_id=%s (owner=%s): table_counts=%s",
            sustain_id, owner_user_id, table_counts,
        )
        return {"status": "purged", "sustain_id": sustain_id, "table_counts": table_counts}

    # ── Dedup ──────────────────────────────────────────────────────────────────

    @staticmethod
    def _amount_minor(parsed_fields: dict | None) -> int | None:
        """The parsed amount in minor units, or None when there isn't one.

        Integer minor units, not a float: 0.1 + 0.2 is not 0.3, and a key that
        disagrees with itself by a rounding error is worse than no key.
        """
        if not parsed_fields:
            return None
        amount = parsed_fields.get("amount")
        if amount is None:
            return None
        try:
            return int(round(float(amount) * 100))
        except (TypeError, ValueError):
            return None

    @classmethod
    def _intake_key(
        cls,
        sustain_id: str,
        source_id: str,
        raw_payload: str,
        external_ref: str | None = None,
        parsed_fields: dict | None = None,
    ) -> str:
        """
        What makes two captures the same INTAKE (ING-5; core/intake_key.rs).

        Intrinsic first, text second, and there is no third option. A generated
        id is not offered even as a parameter, because a generated id
        partitions by ARRIVAL: the same transaction arriving twice gets two
        ids and applies twice, which turns the retry the outbox exists to make
        safe into the thing that doubles somebody's rent.

            i:<sustain>:<source>:<REF>:<amount_minor>   the message carried a code
            t:<sustain>:<source>:<sha256(raw)>          it did not

        The intrinsic form keys on the FACT, so a re-send in different words --
        a reformatted date, an extra space, a changed footer, all of which
        carriers do -- is still one intake. The text form is honest about what
        it can and cannot see: it catches a byte-identical repeat and nothing
        subtler.

        Scoped by sustain AND source, both deliberately:

          - sustain, because a capture client identifies its source (a device
            id) while sustain_id is supplied per call, so nothing stops one
            source_id feeding two sustains -- a shared or relabelled device, a
            copy-paste mistake. Without it, the second sustain's capture of
            identical text is silently treated as a duplicate of the first and
            its event never applies.

          - source, because suppressing ACROSS sources would be the more
            dangerous bug: a genuinely separate transaction that happened to
            share a reference would vanish without trace. The same fact seen
            through two senders still produces two captures and is SURFACED
            for a person by _find_prior_fact below, exactly as before.

        The amount is in the key, and this is where this method and
        _fact_key() deliberately disagree -- see that method's note.
        """
        ref = (external_ref or "").strip().upper()
        amount_minor = cls._amount_minor(parsed_fields)
        if ref and amount_minor is not None:
            return f"i:{sustain_id}:{source_id}:{ref}:{amount_minor}"
        digest = hashlib.sha256(raw_payload.encode("utf-8")).hexdigest()
        return f"t:{sustain_id}:{source_id}:{digest}"

    @staticmethod
    def _fact_key(sustain_id: str, external_ref: str | None) -> str | None:
        """
        Identity of the FACT, not of the message. None when unavailable.

        dedup_key fingerprints a message, so it cannot catch the same real
        transaction notified twice by two different senders: a KCB alert and a
        Safaricom M-PESA alert for one transfer differ in both source_id and
        wording, hash differently, and were therefore both applied — the money
        counted twice. Bonnie's phone genuinely receives both.

        RECEPTOR/Ingest names the remedy exactly: cross-source correlation
        needs "a key that is a property of the fact rather than of the
        message". The M-PESA transaction code is precisely that. Verified
        against the real texts: a KCB-sender paybill notification and the
        Safaricom-sender confirmation of the same transfer both carry the same
        code (UH1B91GYNW).

        Deliberately keyed on the reference ALONE, not amount+ref:

          - the code is already unique per transaction, so amount adds no
            discriminating power;
          - the two parsers extract amounts from differently-formatted text
            ("Ksh 40000.00" vs "Ksh40,000.00"), so including it risks a
            mismatch that would silently let the double-count back in.

        Failing to match is the dangerous direction here; matching too eagerly
        is not, because two genuinely distinct transactions never share an
        M-PESA code.

        _intake_key() DOES include the amount, and the two are not in
        conflict -- each is right in its own scope. This key runs ACROSS
        sources, where the two parsers read differently-formatted text
        ("Ksh 40000.00" vs "Ksh40,000.00") and an amount mismatch would let
        the double-count back in. The intake key runs WITHIN one source, where
        the same parser produced both readings, so the formatting argument
        does not apply -- and there the amount is doing real work: a reference
        collision within one sender (a truncated code, a reversal pair reusing
        a reference) would otherwise suppress a genuinely separate
        transaction, which is the failure that put the amount in the key
        upstream.

        Returns None when no reference was parsed, in which case correlation is
        simply not attempted — an honest "cannot tell" rather than a guess from
        amount and timing, which would suppress real repeat payments (a
        standing order, two identical fares in one day).
        """
        ref = (external_ref or "").strip().upper()
        if not ref:
            return None
        return hashlib.sha256(f"{sustain_id}\x00fact\x00{ref}".encode("utf-8")).hexdigest()

    def _find_prior_fact(self, sustain_id: str, fact_key: str, exclude_message_id: str):
        """
        Return the earliest other message describing this same fact, if any.

        Deliberately NOT filtered to already-applied rows. An earlier
        notification can be sitting in needs_attention (an outbound payment
        waiting for a pocket decision) — and if it is ignored here, the later
        notification applies, the pending one stays queued, and classifying it
        later double-counts anyway. Order-dependent correctness is not
        correctness; the caller decides what to do using both statuses.

        Rows already marked duplicate are excluded, so a third notification
        chains to the original rather than to another duplicate.
        """
        return self._db.execute(
            "SELECT * FROM ingest_messages "
            "WHERE sustain_id = ? AND fact_key = ? AND id != ? AND status != ? "
            "ORDER BY received_at ASC LIMIT 1",
            (sustain_id, fact_key, exclude_message_id, STATUS_DUPLICATE_FACT),
        ).fetchone()

    def _supersede_pending_duplicate(self, pending_row, winner_id: str, external_ref: str | None) -> None:
        """
        Retire a queued notification whose transaction another message just
        recorded.

        Without this, one transfer could still be counted twice by a slower
        route: the M-PESA alert lands first and needs a pocket decision, the
        KCB alert lands second and applies automatically, and the queued one is
        classified by hand a day later — applying the same money again.

        The queued row is kept and linked, never deleted; it simply stops
        asking for a decision that has already been made.
        """
        self._db.execute(
            "UPDATE ingest_messages SET status = ?, duplicate_of = ?, reason = ? WHERE id = ?",
            (
                STATUS_DUPLICATE_FACT,
                winner_id,
                (
                    f"Superseded: the same transaction (reference {external_ref}) "
                    f"was recorded by another notification. No decision needed."
                ),
                pending_row["id"],
            ),
        )
        self._db.commit()

    # -- Migration ------------------------------------------------------------

    def migrate_intake_keys(self, dry_run: bool = True) -> dict:
        """
        Re-key every already-saved capture onto the ING-5 intake key.

        NOTHING IS DELETED AND NOTHING IS MERGED. Every row keeps its id, its
        raw text and its status; only the dedup_key column is rewritten. A row
        that cannot be keyed intrinsically -- no parsed reference, or no parsed
        amount -- takes the text form, which is unique by construction because
        it is built from exactly the inputs the old key was built from. So a
        reformat can never lose a capture, and the count before must equal the
        count after.

        HISTORY IS NOT REWRITTEN. If two saved rows would land on the same
        intrinsic key, they are a repeat the old text key let through -- but
        both already exist, and one of them may already have moved money. The
        migration does NOT retro-suppress the later one: it leaves both on
        their (unique) text keys and REPORTS the pair, so a person decides.
        Silently collapsing two rows would be the migration deciding something
        about somebody's money that it is not entitled to decide.

        dry_run=True by default. A migration that touches real saved data
        should have to be asked for twice.
        """
        rows = self._db.execute(
            "SELECT id, sustain_id, source_id, raw_payload, dedup_key, external_ref, "
            "parsed_fields_json FROM ingest_messages ORDER BY received_at ASC"
        ).fetchall()

        proposed: dict[str, list[str]] = {}
        for r in rows:
            try:
                parsed = json.loads(r["parsed_fields_json"] or "{}")
            except (ValueError, TypeError):
                parsed = {}
            key = self._intake_key(
                r["sustain_id"], r["source_id"], r["raw_payload"],
                external_ref=r["external_ref"], parsed_fields=parsed,
            )
            proposed.setdefault(key, []).append(r["id"])

        collisions = {k: ids for k, ids in proposed.items() if len(ids) > 1}
        held_back = {mid for ids in collisions.values() for mid in ids}

        plan = []
        for r in rows:
            if r["id"] in held_back:
                key = self._intake_key(r["sustain_id"], r["source_id"], r["raw_payload"])
            else:
                key = next(k for k, ids in proposed.items() if r["id"] in ids)
            if key != r["dedup_key"]:
                plan.append((r["id"], key))

        report = {
            "rows_before": len(rows),
            "to_rewrite": len(plan),
            "intrinsic": sum(1 for _, k in plan if k.startswith("i:")),
            "text": sum(1 for _, k in plan if k.startswith("t:")),
            "collisions": [
                {"key": k, "message_ids": ids} for k, ids in collisions.items()
            ],
            "held_back_on_text_key": len(held_back),
            "dry_run": dry_run,
        }

        if dry_run:
            report["rows_after"] = len(rows)
            report["applied"] = False
            return report

        for message_id, key in plan:
            self._db.execute(
                "UPDATE ingest_messages SET dedup_key = ? WHERE id = ?", (key, message_id)
            )
        self._db.commit()

        after = self._db.execute(
            "SELECT COUNT(*) c, COUNT(DISTINCT dedup_key) d FROM ingest_messages"
        ).fetchone()
        report["rows_after"] = after["c"]
        report["distinct_keys_after"] = after["d"]
        report["applied"] = True
        report["intact"] = after["c"] == len(rows) and after["d"] == after["c"]
        logger.info("[ingest] intake-key migration: %s", report)
        return report

    # ── Sources / staleness ─────────────────────────────────────────────────────

    def register_source(
        self, source_id: str, sustain_id: str,
        label: str | None = None, expected_interval_minutes: int | None = None,
    ) -> None:
        """Explicitly (re)configure a source — label and/or the cadence used for
        staleness. Keyed by (sustain_id, source_id), not source_id alone: a
        source belongs to the sustain it feeds, so the same source_id label
        used by two different sustains is two distinct sources, not one that
        silently reassigns itself (same reasoning as the dedup key)."""
        now = datetime.utcnow().isoformat()
        existing = self._db.execute(
            "SELECT source_id, label FROM ingest_sources WHERE sustain_id = ? AND source_id = ?",
            (sustain_id, source_id),
        ).fetchone()
        if existing is None:
            self._db.execute(
                "INSERT INTO ingest_sources "
                "(sustain_id, source_id, label, expected_interval_minutes, last_seen_at, created_at) "
                "VALUES (?, ?, ?, ?, NULL, ?)",
                (sustain_id, source_id, label, expected_interval_minutes, now),
            )
        else:
            self._db.execute(
                "UPDATE ingest_sources SET label = ?, expected_interval_minutes = ? "
                "WHERE sustain_id = ? AND source_id = ?",
                (label if label is not None else existing["label"], expected_interval_minutes, sustain_id, source_id),
            )
        self._db.commit()

    def _mark_source_seen(self, source_id: str, sustain_id: str, seen_at: str) -> None:
        """Upsert a source row and stamp last_seen_at — called on every capture,
        successful or not, since staleness is about the SOURCE being alive,
        not about whether any given message parsed cleanly."""
        existing = self._db.execute(
            "SELECT source_id FROM ingest_sources WHERE sustain_id = ? AND source_id = ?",
            (sustain_id, source_id),
        ).fetchone()
        if existing is None:
            self._db.execute(
                "INSERT INTO ingest_sources "
                "(sustain_id, source_id, label, expected_interval_minutes, last_seen_at, created_at) "
                "VALUES (?, ?, NULL, NULL, ?, ?)",
                (sustain_id, source_id, seen_at, seen_at),
            )
        else:
            self._db.execute(
                "UPDATE ingest_sources SET last_seen_at = ? WHERE sustain_id = ? AND source_id = ?",
                (seen_at, sustain_id, source_id),
            )
        self._db.commit()

    def get_sources(self, sustain_id: str | None = None) -> list[dict]:
        """
        Every registered source with its computed staleness. A source with no
        expected_interval_minutes configured is never reported stale — we
        genuinely don't know its cadence yet, and guessing one would be the
        exact kind of dishonest state this slice exists to avoid.
        """
        if sustain_id:
            rows = self._db.execute(
                "SELECT * FROM ingest_sources WHERE sustain_id = ?", (sustain_id,)
            ).fetchall()
        else:
            rows = self._db.execute("SELECT * FROM ingest_sources").fetchall()

        now = datetime.utcnow()
        out = []
        for r in rows:
            interval = r["expected_interval_minutes"]
            last_seen = r["last_seen_at"]
            is_stale = False
            if interval is not None:
                if last_seen is None:
                    is_stale = True
                else:
                    is_stale = (now - datetime.fromisoformat(last_seen)) > timedelta(minutes=interval)
            out.append({
                "source_id": r["source_id"],
                "sustain_id": r["sustain_id"],
                "label": r["label"],
                "expected_interval_minutes": interval,
                "last_seen_at": last_seen,
                "is_stale": is_stale,
            })
        return out

    # ── Capture / process ────────────────────────────────────────────────────────

    async def capture(
        self, source_id: str, sustain_id: str, raw_payload: str, captured_at: str | None = None,
    ) -> dict:
        """
        The intake entry point. Idempotent: capturing the exact same
        (source_id, raw_payload) more than once processes it exactly once —
        every later call returns the first call's recorded outcome with
        is_duplicate=True, doing no new work and producing no new event.

        CRITICAL, checked FIRST, before anything else touches the database:
        a message containing an OTP/verification code is refused outright —
        never inserted into ingest_messages, never echoed back in the
        response, never reaches parse_message(). This is server-side
        defense in depth; the primary defense is the Android capture
        client, which should never send one of these at all (see
        SmsSecretFilter.java). transducer.py's own parse_message() also
        checks this and would return status="rejected" if somehow called
        directly with sensitive text, but capture() cannot rely on that
        alone — by the time parse_message() ran, the raw text would
        already need to be in hand, and the one thing this guard exists to
        prevent is that raw text ever touching persistent storage at all.
        """
        now = datetime.utcnow().isoformat()

        if contains_sensitive_secret(raw_payload):
            logger.warning(
                "[ingest] refused a capture containing sensitive content (OTP/verification code) "
                "-- not stored, not parsed. source=%s sustain=%s",
                source_id, sustain_id,
            )
            self._mark_source_seen(source_id, sustain_id, now)
            return {
                "message_id": None,
                "status": STATUS_REJECTED,
                "is_duplicate": False,
                "source_id": source_id,
                "sustain_id": sustain_id,
                "reason": "Message contains an OTP/verification code or similar secret — refused, never stored.",
            }

        # Parse BEFORE keying, because the key is a property of the FACT and
        # the fact is not known until the message has been read. The order is
        # not an optimisation to revisit: keying first would mean keying on
        # the only thing available before a parse -- the wording -- which is
        # exactly the partition ING-5 exists to stop using.
        #
        # parse_message is pure and stateless (transducer.py holds no DB
        # handle); declared_rules is the one real call site threading engine
        # state into it, and reading it here rather than in _process changes
        # nothing about what it returns.
        declared_rules = self._sustain_engine.get_effective_parse_rules(source_id.lower())
        result = parse_message(raw_payload, source_id=source_id, declared_rules=declared_rules)

        dedup_key = self._intake_key(
            sustain_id, source_id, raw_payload,
            external_ref=result.external_ref, parsed_fields=result.parsed_fields,
        )

        # A source is "seen" the moment it communicates, regardless of what
        # happens to this particular message.
        self._mark_source_seen(source_id, sustain_id, now)

        message_id = str(uuid.uuid4())
        cur = self._db.execute(
            "INSERT OR IGNORE INTO ingest_messages "
            "(id, dedup_key, source_id, sustain_id, raw_payload, received_at, status) "
            "VALUES (?, ?, ?, ?, ?, ?, 'pending')",
            (message_id, dedup_key, source_id, sustain_id, raw_payload, captured_at or now),
        )
        self._db.commit()

        if cur.rowcount == 0:
            # dedup_key collision -- this exact message was already captured.
            existing = self._db.execute(
                "SELECT * FROM ingest_messages WHERE dedup_key = ?", (dedup_key,)
            ).fetchone()
            logger.info("[ingest] duplicate capture ignored: source=%s dedup_key=%s", source_id, dedup_key[:12])
            return self._to_response(existing, is_duplicate=True)

        return await self._process(message_id, result)

    async def _process(self, message_id: str, result) -> dict:
        """Everything that happens to a capture after it has been taken in.

        Takes the parse result rather than re-deriving it: capture() has to
        parse before it can key the row (the key is a property of the fact),
        and parsing twice would risk the row being keyed on one reading and
        acted on by another.
        """
        row = self._db.execute("SELECT * FROM ingest_messages WHERE id = ?", (message_id,)).fetchone()

        status = STATUS_NEEDS_ATTENTION
        operator_name: str | None = None
        operator_params_json: str | None = None
        outcome_json: str | None = None
        reason = result.reason

        # ── Cross-source correlation ──────────────────────────────────────────
        # Runs after parsing (the reference only exists once parsed) and before
        # anything applies. One real transfer fires both a KCB alert and a
        # Safaricom M-PESA alert on Bonnie's phone; both parse correctly, both
        # used to apply, and the money was counted twice.
        #
        # Recorded, not discarded: the raw text is retained like any other
        # capture, linked to the message that already recorded the fact, so the
        # audit trail shows both notifications and why only one moved money.
        fact_key = self._fact_key(row["sustain_id"], result.external_ref)
        if fact_key:
            self._db.execute(
                "UPDATE ingest_messages SET fact_key = ? WHERE id = ?",
                (fact_key, message_id),
            )
            self._db.commit()

            prior = self._find_prior_fact(row["sustain_id"], fact_key, message_id)

            # A prior that is still awaiting a human decision has not recorded
            # the fact yet. If THIS message can record it outright, let this one
            # win and retire the queued one — otherwise the queued one gets
            # classified later and the same money applies twice.
            if (
                prior is not None
                and prior["status"] == STATUS_NEEDS_ATTENTION
                and result.status == "mapped"
            ):
                self._supersede_pending_duplicate(prior, message_id, result.external_ref)
                prior = None

            if prior is not None:
                self._db.execute(
                    "UPDATE ingest_messages SET status = ?, parser_name = ?, "
                    "external_ref = ?, parsed_fields_json = ?, reason = ?, "
                    "duplicate_of = ? WHERE id = ?",
                    (
                        STATUS_DUPLICATE_FACT,
                        result.parser_name,
                        result.external_ref,
                        json.dumps(result.parsed_fields or {}, default=str),
                        (
                            f"Same transaction already recorded from source "
                            f"'{prior['source_id']}' (reference {result.external_ref}). "
                            f"Kept for the record; not applied a second time."
                        ),
                        prior["id"],
                        message_id,
                    ),
                )
                self._db.commit()
                logger.info(
                    "[ingest] cross-source duplicate: %s from '%s' matches %s from '%s' (ref %s)",
                    message_id[:8], row["source_id"], prior["id"][:8],
                    prior["source_id"], result.external_ref,
                )
                updated = self._db.execute(
                    "SELECT * FROM ingest_messages WHERE id = ?", (message_id,)
                ).fetchone()
                return self._to_response(updated, is_duplicate=True)

        if result.status == "mapped":
            op_result = await self._sustain_engine.execute_operator(
                row["sustain_id"], result.operator_name, result.operator_params,
                origin_message_id=message_id,
            )
            operator_name = result.operator_name
            operator_params_json = json.dumps(result.operator_params, default=str)
            outcome_json = json.dumps(op_result.to_response(), default=str)
            if op_result.succeeded:
                status = STATUS_APPLIED
            else:
                # A parsed, confidently-mapped event that the S2 gate (or any
                # other operator-level check) refused -- quarantined honestly,
                # never force-applied. constraint_violated == "enforcement_gate"
                # specifically means an invariant/post_constraint would have
                # been violated; any other reason is still a real refusal.
                status = STATUS_REFUSED
                reason = f"Operator refused: {op_result.reason}"
        elif result.status == "informational":
            # Recognised, but genuinely not a transaction -- recorded (never
            # silently dropped) but NEVER surfaces in needs_attention(),
            # since nothing about it needs a human decision. Fixed 2 Aug
            # 2026: this used to share parsed_unmapped's fate (both became
            # STATUS_NEEDS_ATTENTION below), which is exactly why a plain
            # M-Pesa "unable to process your request" system notice was
            # showing up as a classify card demanding a pocket decision it
            # never needed.
            status = STATUS_INFORMATIONAL
        # else: parsed_unmapped or unparsed -- both need a human, status stays needs_attention

        self._db.execute(
            "UPDATE ingest_messages SET status=?, parser_name=?, external_ref=?, parsed_fields_json=?, "
            "operator_name=?, operator_params_json=?, outcome_json=?, reason=? WHERE id=?",
            (
                status, result.parser_name, result.external_ref,
                json.dumps(result.parsed_fields, default=str),
                operator_name, operator_params_json, outcome_json, reason,
                message_id,
            ),
        )
        self._db.commit()

        updated = self._db.execute("SELECT * FROM ingest_messages WHERE id = ?", (message_id,)).fetchone()
        return self._to_response(updated, is_duplicate=False)

    # ── Reads ──────────────────────────────────────────────────────────────────

    def list_messages(
        self, sustain_id: str | None = None, status: str | None = None, limit: int = 50,
    ) -> list[dict]:
        conditions: list[str] = []
        params: list[Any] = []
        if sustain_id:
            conditions.append("sustain_id = ?")
            params.append(sustain_id)
        if status:
            conditions.append("status = ?")
            params.append(status)
        query = "SELECT * FROM ingest_messages"
        if conditions:
            query += " WHERE " + " AND ".join(conditions)
        query += " ORDER BY received_at DESC LIMIT ?"
        params.append(limit)
        rows = self._db.execute(query, tuple(params)).fetchall()
        return [self._to_response(r, is_duplicate=False) for r in rows]

    def get_message(self, message_id: str) -> dict | None:
        row = self._db.execute("SELECT * FROM ingest_messages WHERE id = ?", (message_id,)).fetchone()
        return self._to_response(row, is_duplicate=False) if row else None

    def resolve_message(self, message_id: str, resolved_by: str) -> bool:
        """
        Mark a needs_attention row as handled by a human (e.g. they ran the
        right operator manually via the Console). Does not retry or mutate
        state itself — resolution is an acknowledgement, not an action.
        Returns False if the message doesn't exist or isn't in
        needs_attention (already resolved / was never in that state).
        """
        now = datetime.utcnow().isoformat()
        cur = self._db.execute(
            "UPDATE ingest_messages SET resolved_at = ?, resolved_by = ? "
            "WHERE id = ? AND status = ? AND resolved_at IS NULL",
            (now, resolved_by, message_id, STATUS_NEEDS_ATTENTION),
        )
        self._db.commit()
        return cur.rowcount > 0

    def mark_not_a_transaction(self, message_id: str, sustain_id: str, marked_by: str) -> bool:
        """
        Human-in-the-loop override (2 Aug 2026, Bonnie): a captured message
        reached the classify queue asking "which pocket does this belong
        to?" but isn't a transaction at all -- e.g. a real M-Pesa failure
        notice ("Failed. The till number entered is incorrect...") that
        the transducer's own informational-system-message filter didn't
        (yet) recognise. Rather than invent a THIRD "human-informational"
        status, this reuses STATUS_INFORMATIONAL directly -- a message a
        human just told us isn't a transaction is exactly as informational
        as one the transducer itself recognised as such, and gets the
        exact same treatment: kept in the message store (never deleted --
        real audit trail), permanently excluded from needs_attention()'s
        query (same status filter), never booked as a spend/income.

        Distinguishable from an auto-recognised informational message in
        the stored row itself: resolved_at/resolved_by are set here (a
        human acted, recorded who and when) but left NULL for the
        transducer's own automatic classification -- _process() never
        touches those columns for its own STATUS_INFORMATIONAL rows.

        Only acts on a message currently in needs_attention (mirrors
        resolve_message()'s own guard exactly) -- returns False if it's
        already resolved/informational/applied/refused, doesn't exist, or
        belongs to a different sustain than claimed (sustain_id is part of
        the WHERE clause as a defense-in-depth scope check, on top of
        whatever ownership check the calling route already did).
        """
        now = datetime.utcnow().isoformat()
        row = self._db.execute(
            "SELECT reason FROM ingest_messages WHERE id = ? AND sustain_id = ? AND status = ? AND resolved_at IS NULL",
            (message_id, sustain_id, STATUS_NEEDS_ATTENTION),
        ).fetchone()
        if row is None:
            return False
        original_reason = row["reason"] if row["reason"] else ""
        override_note = "Marked 'not a transaction' by a human — never booked."
        new_reason = f"{original_reason} — {override_note}" if original_reason else override_note
        cur = self._db.execute(
            "UPDATE ingest_messages SET status = ?, resolved_at = ?, resolved_by = ?, reason = ? "
            "WHERE id = ? AND sustain_id = ? AND status = ? AND resolved_at IS NULL",
            (
                STATUS_INFORMATIONAL, now, marked_by, new_reason,
                message_id, sustain_id, STATUS_NEEDS_ATTENTION,
            ),
        )
        self._db.commit()
        return cur.rowcount > 0

    def needs_attention(self, sustain_id: str | None = None) -> list[dict]:
        """Unresolved needs_attention rows — the queue a human must clear."""
        conditions = ["status = ?", "resolved_at IS NULL"]
        params: list[Any] = [STATUS_NEEDS_ATTENTION]
        if sustain_id:
            conditions.append("sustain_id = ?")
            params.append(sustain_id)
        rows = self._db.execute(
            f"SELECT * FROM ingest_messages WHERE {' AND '.join(conditions)} ORDER BY received_at DESC",
            tuple(params),
        ).fetchall()
        return [self._to_response(r, is_duplicate=False) for r in rows]

    # ── Serialisation ──────────────────────────────────────────────────────────

    def _to_response(self, row, is_duplicate: bool) -> dict:
        return {
            "message_id": row["id"],
            "status": row["status"],
            "is_duplicate": is_duplicate,
            "source_id": row["source_id"],
            "sustain_id": row["sustain_id"],
            "raw_payload": row["raw_payload"],
            "received_at": row["received_at"],
            "parser_name": row["parser_name"],
            "external_ref": row["external_ref"],
            "parsed_fields": json.loads(row["parsed_fields_json"]) if row["parsed_fields_json"] else {},
            "operator_name": row["operator_name"],
            "operator_params": json.loads(row["operator_params_json"]) if row["operator_params_json"] else None,
            "outcome": json.loads(row["outcome_json"]) if row["outcome_json"] else None,
            "reason": row["reason"],
            # Present when this message describes a transaction another message
            # already recorded (the same transfer notified by two senders).
            "duplicate_of": row["duplicate_of"] if "duplicate_of" in row.keys() else None,
            "resolved_at": row["resolved_at"],
            "resolved_by": row["resolved_by"],
        }
