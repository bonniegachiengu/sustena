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
                resolved_by            TEXT
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
        self._db.commit()

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
    def _dedup_key(sustain_id: str, source_id: str, raw_payload: str) -> str:
        """
        Stable fingerprint for the inbox dedup boundary: the same source
        reporting the exact same raw text INTO THE SAME SUSTAIN is the same
        captured message, full stop — including messages that fail to parse
        at all, so a garbled message doesn't pile up duplicate needs_attention
        rows every time a flaky capture client retries the POST.

        sustain_id is part of the key, not just source_id+payload: a capture
        client identifies its source (e.g. a device id) but sustain_id is
        supplied per-call, so nothing stops the same source_id from feeding
        two different sustains (a shared/relabelled device, a copy-paste
        mistake). Without sustain_id in the key, the second sustain's capture
        of identical text would be silently treated as a duplicate of the
        first and its event would never apply — the exact kind of cross-tenant
        dedup collision "no silent failure" exists to prevent.
        """
        digest = hashlib.sha256(f"{sustain_id}\x00{source_id}\x00{raw_payload}".encode("utf-8")).hexdigest()
        return digest

    # KNOWN, DISCLOSED, NOT FIXED (flagged 1 Aug 2026): this key does NOT
    # catch the same real-world transaction arriving twice from two DIFFERENT
    # sources -- e.g. a KCB-sender notification AND a genuine Safaricom
    # MPESA-sender SMS about the identical transfer. source_id ("kcb" vs
    # "mpesa") and raw_payload (different wording/sender template) both
    # differ between the two notifications, so they hash to two distinct
    # dedup_keys and both get captured and processed as separate income/spend
    # events -- a real double-count risk for any transaction that genuinely
    # triggers both a bank-side and a telco-side SMS. See transducer.py's own
    # note beside _PARSERS for the fuller writeup and a sketch of what a real
    # fix (cross-source correlation on amount + shared M-PESA ref) would need.
    # Not fixed here -- flagged for a deliberate decision, not silently patched.

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

        dedup_key = self._dedup_key(sustain_id, source_id, raw_payload)

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

        return await self._process(message_id)

    async def _process(self, message_id: str) -> dict:
        row = self._db.execute("SELECT * FROM ingest_messages WHERE id = ?", (message_id,)).fetchone()
        # source_id (whatever the capture client tagged this with, decided
        # strictly by SMS sender -- see transducer.py's own _PARSERS_BY_SOURCE
        # comment) is passed through so parsing stays scoped to that source's
        # own parser set -- body content can never override it.
        #
        # declared_rules (Phase 3C, 2 Aug 2026): the real, engine-aware
        # effective rule set (seed + any active user corrections) --
        # transducer.py itself stays pure/stateless (no DB access), so this
        # is the one real call site threading engine state into it.
        declared_rules = self._sustain_engine.get_effective_parse_rules((row["source_id"] or "").lower())
        result = parse_message(row["raw_payload"], source_id=row["source_id"], declared_rules=declared_rules)

        status = STATUS_NEEDS_ATTENTION
        operator_name: str | None = None
        operator_params_json: str | None = None
        outcome_json: str | None = None
        reason = result.reason

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
            "resolved_at": row["resolved_at"],
            "resolved_by": row["resolved_by"],
        }
