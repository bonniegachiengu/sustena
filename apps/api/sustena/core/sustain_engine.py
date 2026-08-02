"""
sustena/core/sustain_engine.py

SustainEngine — the runtime that instantiates, runs, and simulates sustains.

Epic 1.3.2

Responsibilities:
  - Load sustain JSON specs from sustena/sustains/{template_id}.json
  - Resolve {{placeholder}} tokens using caller-supplied parameters
  - Persist sustain rows and state snapshots to SQLite
  - Execute operators against live state via OPERATOR_REGISTRY
  - Simulate operator sequences against a forked state (no DB writes)
  - Instantiate operative instances for each sustain

Usage:
    engine = SustainEngine() # in-memory SQLite by default
    sid    = engine.instantiate("homestead", uid, {"owner_ids": [uid]})
    result = await engine.execute_operator(sid, "budget.record_income", {"amount": 5000})
    state  = engine.get_state(sid)
"""

import copy
import inspect
import json
import logging
import re
import sqlite3
import time
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

import sustena.operators  # noqa: F401 — triggers OPERATOR_REGISTRY population

from sustena.core.advisory import Suggestion, evaluate_operative
from sustena.core.egress import EgressQueue
from sustena.core.event_fold import fold_events
from sustena.core.events import EventBus
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult
from sustena.core.pawa import PawaLedger
from sustena.core import pawa_meter
from sustena.core.predicates import (
    Aggregate as _PredAggregate,
    Comparison as _PredComparison,
    LogicalAnd as _PredLogicalAnd,
    LogicalNot as _PredLogicalNot,
    LogicalOr as _PredLogicalOr,
    Membership as _PredMembership,
    Quantifier as _PredQuantifier,
    StatePath as _PredStatePath,
    compile_invariant,
    evaluate_predicate,
)
from sustena.core.state import StateAccessor
from sustena.operatives import (
    AttacheOperative,
    CuratorOperative,
    MentorOperative,
    NavigatorOperative,
    ProtegeOperative,
)

logger = logging.getLogger(__name__)

# ── Operative name → class mapping ─────────────────────────────────────────────

_OPERATIVE_MAP: dict[str, type] = {
    "mentor":    MentorOperative,
    "protege":   ProtegeOperative,
    "attache":   AttacheOperative,
    "navigator": NavigatorOperative,
    "curator":   CuratorOperative,
}

_OPERATIVE_ROLES: dict[str, str] = {
    "mentor":    "Strategic advisor · finance",
    "protege":   "Learning · pattern recognition",
    "attache":   "Contacts & governance",
    "navigator": "Logistics & routing",
    "curator":   "Assets & procurement",
}

# ── Spec location ───────────────────────────────────────────────────────────────

# sustena/sustains/ lives inside apps/api/sustena/sustains/
# Repo layout: apps/api/
#   sustena/sustains/homestead.json   ← specs live here
#   sustena/core/sustain_engine.py    ← this file
# Path: __file__.parent(core) → parent(sustena) → parent(api) → sustena/sustains/
_SUSTAINS_DIR = Path(__file__).parent.parent.parent / "sustena" / "sustains"

# Slice 11 (Egress) — the ONLY implemented send target is a local file
# under this directory. Not committed to git (see .gitignore); created on
# first send, not at import time.
_EXPORTS_DIR = Path(__file__).parent.parent.parent / "exports"

# {{placeholder}} token pattern
_TOKEN_RE = re.compile(r"\{\{(\w+)\}\}")


def _referenced_root_names(node: Any) -> set[str]:
    """
    Walk a compiled predicate AST (predicates.py's node types) and collect
    every top-level state-path root name it touches — e.g. for
    "household_liquid_total >= 0" this returns {"household_liquid_total"}.

    Used only to detect whether an invariant references a computed roll-up
    aggregate (spec["aggregates"]); such invariants are excluded from gate
    enforcement but still evaluated for display. Not a general-purpose
    predicates.py utility — kept local to this composition-specific use.
    """
    names: set[str] = set()

    def walk(n: Any) -> None:
        if n is None:
            return
        if isinstance(n, _PredStatePath):
            if n.segments and n.segments[0][0] == "name":
                names.add(n.segments[0][1])
        elif isinstance(n, _PredAggregate):
            walk(n.path)
        elif isinstance(n, _PredComparison):
            walk(n.left)
            walk(n.right)
        elif isinstance(n, _PredMembership):
            walk(n.left)
            walk(n.right)
        elif isinstance(n, (_PredLogicalAnd, _PredLogicalOr)):
            for part in n.parts:
                walk(part)
        elif isinstance(n, _PredLogicalNot):
            walk(n.operand)
        elif isinstance(n, _PredQuantifier):
            walk(n.list_path)
            walk(n.predicate)
        # Literal / ParamRef / ListLiteral carry no state path — nothing to add.

    walk(node)
    return names


def _parse_rule_to_dict(rule) -> dict:
    """Serialise a ParseRule (a frozen dataclass, possibly nesting FieldSpec dataclasses) to a plain JSON-able dict."""
    from dataclasses import asdict
    return asdict(rule)


def _parse_rule_from_dict(d: dict):
    """The inverse of _parse_rule_to_dict — reconstructs a real ParseRule (with real FieldSpec instances) from its stored JSON dict."""
    from sustena.core.parse_rule import FieldSpec, ParseRule
    extract = {k: FieldSpec(**v) for k, v in (d.get("extract") or {}).items()}
    return ParseRule(
        id=d["id"], source=d["source"], version=d.get("version", 1), pattern=d["pattern"],
        extract=extract, status=d.get("status", "parsed_unmapped"),
        operator=d.get("operator"), params=d.get("params") or {},
        flags=tuple(d.get("flags") or ()), reason_template=d.get("reason_template"),
        trust=d.get("trust", "shipped"), provenance=d.get("provenance", "sustena_core"),
        examples=tuple(d.get("examples") or ()),
    )


class SustainEngine:
    """
    Runtime for instantiating and operating sustains.

    Backed by a SQLite database (synchronous sqlite3).  Pass db_path=":memory:"
    for tests — ephemeral, no disk I/O, each engine instance gets its own DB.

    The four core primitives (StateAccessor, EventBus, PawaLedger,
    ConstraintEngine) are wired together inside each operator call, exactly as
    they are in the existing operator tests — the engine just orchestrates the
    setup, dispatch, and persistence.
    """

    def __init__(self, db_path: str = ":memory:") -> None:
        self._db_path = db_path
        self._db = sqlite3.connect(db_path, check_same_thread=False)
        self._db.row_factory = sqlite3.Row
        self._ensure_tables()

        # Per-sustain caches (populated by instantiate)
        self._specs:     dict[str, dict]      = {}   # sustain_id → spec dict
        self._owners:    dict[str, str]        = {}   # sustain_id → user_id
        self._operatives: dict[str, dict[str, Any]] = {}  # sustain_id → {name: instance}

    # ── Table management ───────────────────────────────────────────────────────

    def _ensure_tables(self) -> None:
        """
        Create sustains and sustain_states tables if they don't exist.

        Idempotent — safe to call multiple times or against a DB that already
        has these tables from SQLAlchemy schema.py.
        """
        self._db.executescript("""
            CREATE TABLE IF NOT EXISTS sustains (
                id              TEXT PRIMARY KEY,
                user_id         TEXT NOT NULL,
                template_id     TEXT NOT NULL,
                created_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sustain_states (
                id              TEXT PRIMARY KEY,
                sustain_id      TEXT NOT NULL,
                state_json      TEXT NOT NULL,
                version_number  INTEGER NOT NULL DEFAULT 1,
                updated_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                id              TEXT PRIMARY KEY,
                sustain_id      TEXT NOT NULL,
                event_name      TEXT NOT NULL,
                payload_json    TEXT NOT NULL,
                operator_log_id TEXT,
                timestamp       TEXT NOT NULL,
                seq             INTEGER,
                mutations_json  TEXT
            );

            CREATE TABLE IF NOT EXISTS operative_overrides (
                sustain_id      TEXT NOT NULL,
                operative_id    TEXT NOT NULL,
                enabled         INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (sustain_id, operative_id)
            );

            CREATE TABLE IF NOT EXISTS sustain_templates (
                id              TEXT PRIMARY KEY,
                owner_user_id   TEXT NOT NULL,
                spec_json       TEXT NOT NULL,
                version         INTEGER NOT NULL DEFAULT 1,
                created_at      TEXT NOT NULL,
                updated_at      TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sustain_composition (
                id                  TEXT PRIMARY KEY,
                parent_sustain_id   TEXT NOT NULL,
                child_sustain_id    TEXT NOT NULL,
                slot                TEXT,
                member              TEXT,
                linked_at           TEXT NOT NULL,
                UNIQUE(parent_sustain_id, child_sustain_id),
                UNIQUE(parent_sustain_id, slot)
            );

            CREATE TABLE IF NOT EXISTS operative_suggestions (
                id                  TEXT PRIMARY KEY,
                sustain_id          TEXT NOT NULL,
                operative_id        TEXT NOT NULL,
                rule_id             TEXT NOT NULL,
                title               TEXT NOT NULL,
                reason              TEXT NOT NULL,
                severity            TEXT NOT NULL,
                dedupe_key          TEXT NOT NULL,
                proposed_operator   TEXT,
                proposed_params_json TEXT,
                status              TEXT NOT NULL DEFAULT 'pending',
                created_at          TEXT NOT NULL,
                updated_at          TEXT NOT NULL,
                resolved_at         TEXT,
                UNIQUE(sustain_id, dedupe_key)
            );

            CREATE TABLE IF NOT EXISTS egress_outbox (
                id                  TEXT PRIMARY KEY,
                sustain_id          TEXT NOT NULL,
                kind                TEXT NOT NULL,
                target              TEXT NOT NULL,
                payload_json        TEXT NOT NULL,
                idempotency_key     TEXT NOT NULL,
                status              TEXT NOT NULL DEFAULT 'prepared',
                prepared_at         TEXT NOT NULL,
                updated_at          TEXT NOT NULL,
                confirmed_at        TEXT,
                sent_at             TEXT,
                failed_at           TEXT,
                failure_reason      TEXT,
                result_json         TEXT,
                attempts            INTEGER NOT NULL DEFAULT 0,
                UNIQUE(sustain_id, idempotency_key)
            );

            CREATE TABLE IF NOT EXISTS pawa_meter_log (
                id              TEXT PRIMARY KEY,
                sustain_id      TEXT NOT NULL,
                operator_name   TEXT NOT NULL,
                principal       TEXT NOT NULL,
                compute         REAL NOT NULL,
                storage         REAL NOT NULL,
                pawa            REAL NOT NULL,
                elapsed_ms      REAL,
                timestamp       TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS holon_transfers (
                id                  TEXT PRIMARY KEY,
                from_sustain_id     TEXT NOT NULL,
                to_sustain_id       TEXT NOT NULL,
                amount              REAL NOT NULL,
                idempotency_key     TEXT NOT NULL,
                created_at          TEXT NOT NULL,
                UNIQUE(idempotency_key)
            );

            CREATE TABLE IF NOT EXISTS parse_rule_edits (
                id              TEXT PRIMARY KEY,
                rule_id         TEXT NOT NULL,
                source          TEXT NOT NULL,
                version         INTEGER NOT NULL,
                rule_json       TEXT NOT NULL,
                edit_name       TEXT NOT NULL,
                status          TEXT NOT NULL,
                author_user_id  TEXT NOT NULL,
                parent_version  INTEGER,
                created_at      TEXT NOT NULL
            );
        """)
        self._db.commit()
        self._migrate_events_schema_sync()
        self._migrate_egress_schema_sync()

    def _migrate_egress_schema_sync(self) -> None:
        """
        Same class of fix as _migrate_events_schema_sync: CREATE TABLE IF
        NOT EXISTS never adds a column to a table that already existed
        under an older definition. Caught live: an earlier --reload cycle
        during this slice's own development created egress_outbox before
        updated_at was added to the schema above, leaving the real
        sustena.db's table permanently missing it (0 rows at the time,
        confirmed before this fix — no data at risk). Additive, nullable-
        default-free ALTER TABLE; no-op once the column exists.
        """
        cols = [row[1] for row in self._db.execute("PRAGMA table_info(egress_outbox)").fetchall()]
        if not cols or "updated_at" in cols:
            return
        self._db.execute("ALTER TABLE egress_outbox ADD COLUMN updated_at TEXT")
        self._db.execute("UPDATE egress_outbox SET updated_at = prepared_at WHERE updated_at IS NULL")
        self._db.commit()
        logger.info("[SustainEngine] migrated egress_outbox table — added updated_at column.")

    def _migrate_events_schema_sync(self) -> None:
        """
        Sync counterpart to db/schema.py's _migrate_events_schema — same
        additive, nullable-column ALTER TABLE, but through this engine's own
        sqlite3 connection. Needed because SustainEngine talks to sustena.db
        directly and doesn't go through the async init_db() migration path;
        without this, an events table that predates Slice 3 (state =
        fold(events)) would be missing seq/mutations_json and every event
        append would fail with "no such column". No-op once both exist.
        """
        cols = [row[1] for row in self._db.execute("PRAGMA table_info(events)").fetchall()]
        if not cols:
            return  # table doesn't exist yet — the CREATE TABLE above just made it, with both columns
        added = False
        if "seq" not in cols:
            self._db.execute("ALTER TABLE events ADD COLUMN seq INTEGER")
            added = True
        if "mutations_json" not in cols:
            self._db.execute("ALTER TABLE events ADD COLUMN mutations_json TEXT")
            added = True
        if added:
            self._db.commit()
            logger.info("[SustainEngine] migrated events table — added seq + mutations_json columns.")

    # ── Spec loading ───────────────────────────────────────────────────────────

    def _load_spec(self, template_id: str) -> dict:
        """
        Load a sustain spec. Disk-first, DB-fallback: the built-in specs
        (homestead, habitat) live at sustena/sustains/{template_id}.json and
        always take priority (a generated definition template_id can never
        collide with a fixed built-in filename stem, but this ordering means
        a DB row could never shadow a built-in even if it somehow did).

        User-created definitions (Slice 6 — create/definition) persist in the
        sustain_templates table instead of a file, keyed by the same
        template_id instantiate() takes — everything downstream of this call
        (instantiate, execute_operator, the enforcement gate, simulate,
        rebuild_state) reads template_id -> spec through this ONE function, so
        a user-created sustain runs through literally the same code path as
        homestead/habitat. No special-casing anywhere else in the engine.

        Raises ValueError if the template_id resolves to neither.
        """
        spec_path = _SUSTAINS_DIR / f"{template_id}.json"
        if spec_path.exists():
            with spec_path.open(encoding="utf-8") as fh:
                return json.load(fh)

        row = self._db.execute(
            "SELECT spec_json FROM sustain_templates WHERE id = ?", (template_id,)
        ).fetchone()
        if row is not None:
            return json.loads(row["spec_json"])

        raise ValueError(
            f"Sustain spec '{template_id}' not found at {spec_path} or in sustain_templates. "
            f"Available disk specs: {[p.stem for p in _SUSTAINS_DIR.glob('*.json')]}"
        )

    def _compile_spec_invariants(self, spec: dict) -> None:
        """
        Parse + schema-bind every declared invariant into a typed predicate AST
        (Move 1). Mutates spec in place, caching:
          spec["_compiled_invariants"]      -- [{id, description, expr, node,
                                                  is_aggregate, authority, enforced}]
          spec["_invariant_compile_errors"] -- [{id, expr, errors}]

        A predicate referencing a state dimension the schema doesn't declare
        fails HERE, at load time — never silently at runtime. Idempotent and
        non-fatal: an invalid invariant is skipped from enforcement and
        recorded as a compile error, it does not block the sustain from
        loading (existing sustains keep working; see enforcement gate below).

        Composition/roll-up authority model (confirmed with Bonnie — Option C,
        per-rule authority defaulting to advisory, not a blanket "parent never
        vetoes"): `is_aggregate` is True for any invariant whose expression
        references a dimension declared in spec["aggregates"] — a computed
        roll-up value folded from linked children, never part of this
        sustain's own persisted state. `authority` is read straight from the
        invariant's own declaration (spec["invariants"][i]["authority"]),
        defaulting to "advisory" when absent — every existing spec (homestead,
        habitat, every Slice 6 create_definition() invariant) is unaffected by
        this default. Only an aggregate invariant explicitly marked
        `"authority": "binding"` can ever refuse a CHILD's transition (see
        _check_parent_binding_gate) — every other rule, aggregate or not,
        behaves exactly as before.

        `enforced` governs THIS sustain's OWN gate (_check_enforcement_gate,
        used for its own operator calls): always False for an aggregate
        invariant, regardless of authority — an aggregate can't change from
        what this sustain itself does, so gating a sustain's own unrelated
        operator on it would just be a confusing, permanent block. A binding
        aggregate invariant instead governs CHILDREN's transitions, via a
        separate, narrower check (_check_parent_binding_gate) that only fires
        at the moment a child's action would newly breach it.
        """
        if "_compiled_invariants" in spec:
            return
        state_schema = spec.get("state_schema", {})
        aggregate_ids = {agg["id"] for agg in spec.get("aggregates", []) if agg.get("id")}
        compiled: list[dict] = []
        errors: list[dict] = []
        for inv in spec.get("invariants", []):
            expr = inv.get("expression", "")
            node, inv_errors = compile_invariant(expr, state_schema)
            if inv_errors:
                errors.append({"id": inv.get("id"), "expr": expr, "errors": inv_errors})
                logger.warning(
                    "[SustainEngine] invariant '%s' (%s) failed to compile against state_schema: %s",
                    inv.get("id"), expr, inv_errors,
                )
                continue
            is_aggregate = bool(aggregate_ids & _referenced_root_names(node))
            authority = inv.get("authority") or "advisory"
            if authority not in ("binding", "advisory"):
                logger.warning(
                    "[SustainEngine] invariant '%s' has unknown authority '%s' — treating as advisory.",
                    inv.get("id"), authority,
                )
                authority = "advisory"
            compiled.append({
                "id": inv.get("id"), "description": inv.get("description", ""), "expr": expr, "node": node,
                "is_aggregate": is_aggregate, "authority": authority, "enforced": not is_aggregate,
            })
        spec["_compiled_invariants"] = compiled
        spec["_invariant_compile_errors"] = errors

    def _enforcement_enabled(self, spec: dict) -> bool:
        """Opt-in per sustain — spec["enforcement"]["enabled"] must be explicitly true."""
        return bool(spec.get("enforcement", {}).get("enabled", False))

    def _check_enforcement_gate(self, spec: dict, state: StateAccessor, params: dict, meta) -> tuple[bool, str]:
        """
        Move 2 — the enforcing gate. Evaluates the sustain's compiled invariants
        and the operator's declared post_constraints against the candidate
        post-effect state. Returns (True, "") if the transition may commit,
        or (False, reason) if it must be refused.

        Only called when enforcement is enabled for this sustain — callers
        must check _enforcement_enabled() first (kept separate so callers can
        skip building this call's args entirely on non-enforced sustains).
        """
        for inv in spec.get("_compiled_invariants", []):
            if not inv.get("enforced", True):
                continue  # aggregate-referencing — observational only, never blocks (see _compile_spec_invariants)
            ok, reason = evaluate_predicate(inv["node"], state, params)
            if not ok:
                return False, f"would violate invariant '{inv['id']}' ({inv['expr']}): {reason}"

        state_schema = spec.get("state_schema", {})
        for expr in getattr(meta, "post_constraints", []) or []:
            node, compile_errors = compile_invariant(expr, state_schema)
            if compile_errors:
                logger.warning(
                    "[SustainEngine] post_constraint '%s' on operator failed to compile: %s",
                    expr, compile_errors,
                )
                continue
            ok, reason = evaluate_predicate(node, state, params)
            if not ok:
                return False, f"would violate post_constraint ({expr}): {reason}"

        return True, ""

    def _resolve_tokens(self, obj: Any, params: dict) -> Any:
        """
        Recursively replace {{placeholder}} tokens in every string within obj.

        - List values in params are serialised as JSON arrays.
        - Unresolved tokens (key not in params) are left as-is.
        """
        if isinstance(obj, str):
            def _replace(m: re.Match) -> str:
                key = m.group(1)
                val = params.get(key)
                if val is None:
                    return m.group(0)  # leave unresolved
                return json.dumps(val) if not isinstance(val, str) else val
            return _TOKEN_RE.sub(_replace, obj)
        if isinstance(obj, dict):
            return {k: self._resolve_tokens(v, params) for k, v in obj.items()}
        if isinstance(obj, list):
            return [self._resolve_tokens(item, params) for item in obj]
        return obj

    # ── State persistence ──────────────────────────────────────────────────────

    def _persist_state(self, sustain_id: str, state: dict) -> None:
        """
        Cache-only write straight to sustain_states — does NOT append an
        event. Since Slice 3 (state = fold(events)), every real application
        code path must go through _append_events_and_update_cache() (or
        commit_external_mutation()) instead, so the log stays the true
        source of every state change. This method survives only as a raw
        setup utility for test fixtures that need to seed a precondition
        state without caring whether it's fold-reproducible — do not call it
        from a real request/operator path.

        Upsert the state for sustain_id in sustain_states.
        If a row already exists, increments version_number and updates state_json.
        If no row exists, inserts a new one at version 1.
        """
        now = datetime.utcnow().isoformat()
        state_json = json.dumps(state)

        existing = self._db.execute(
            "SELECT id, version_number FROM sustain_states WHERE sustain_id = ?",
            (sustain_id,),
        ).fetchone()

        if existing is None:
            self._db.execute(
                "INSERT INTO sustain_states (id, sustain_id, state_json, version_number, updated_at) "
                "VALUES (?, ?, ?, 1, ?)",
                (str(uuid.uuid4()), sustain_id, state_json, now),
            )
        else:
            self._db.execute(
                "UPDATE sustain_states "
                "SET state_json = ?, version_number = ?, updated_at = ? "
                "WHERE sustain_id = ?",
                (state_json, existing["version_number"] + 1, now, sustain_id),
            )
        self._db.commit()

    def _load_state_dict(self, sustain_id: str) -> dict:
        """
        Load the current state dict for sustain_id from the DB.
        Raises ValueError if the sustain has no state row.
        """
        row = self._db.execute(
            "SELECT state_json FROM sustain_states WHERE sustain_id = ?",
            (sustain_id,),
        ).fetchone()
        if row is None:
            raise ValueError(
                f"No state found for sustain_id='{sustain_id}'. "
                "Has it been instantiated?"
            )
        return json.loads(row["state_json"])

    # ── Event sourcing — state = fold(events) (Slice 3) ────────────────────────
    #
    # sustain_states is now a CACHE, not the source of truth: it exists purely
    # for read performance (the Monitor polls it every second) and is kept in
    # sync with the event log on every write below. The source of truth is the
    # events table — specifically each row's mutations_json, which is exactly
    # what StateAccessor recorded while the operator that produced the event
    # ran. rebuild_state() proves the cache is always reconstructible by
    # folding from scratch and comparing.

    def _next_seq(self, sustain_id: str) -> int:
        """Next per-sustain fold-order sequence number (1-based, gapless)."""
        row = self._db.execute(
            "SELECT COALESCE(MAX(seq), 0) + 1 AS next_seq FROM events WHERE sustain_id = ?",
            (sustain_id,),
        ).fetchone()
        return row["next_seq"] if row is not None else 1

    def _write_events_and_cache_no_commit(
        self, sustain_id: str, new_state: dict, events: list[dict],
    ) -> None:
        """
        The write half of _append_events_and_update_cache, WITHOUT the
        commit/rollback — factored out so a caller that needs to write
        MORE THAN ONE sustain's events+cache in a single atomic transaction
        (holon.transfer's paired debit+credit — see
        _commit_atomic_multi_sustain) can issue several of these against
        the shared connection before committing once. Never call this
        directly outside that pattern; every single-sustain caller should
        keep using _append_events_and_update_cache, which wraps this with
        its own commit/rollback exactly as before.
        """
        now = datetime.utcnow().isoformat()
        seq = self._next_seq(sustain_id)
        for ev in events:
            payload = ev.get("payload", {})
            try:
                payload_json = json.dumps(payload, default=str)
            except (TypeError, ValueError):
                payload_json = json.dumps(str(payload))
            mutations_json = json.dumps(ev.get("mutations") or [], default=str)
            self._db.execute(
                "INSERT INTO events "
                "(id, sustain_id, event_name, payload_json, operator_log_id, timestamp, seq, mutations_json) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    ev.get("id") or str(uuid.uuid4()),
                    sustain_id,
                    ev.get("event_name", "event.unknown"),
                    payload_json,
                    ev.get("operator_log_id"),
                    ev.get("timestamp") or now,
                    seq,
                    mutations_json,
                ),
            )
            seq += 1

        state_json = json.dumps(new_state)
        existing = self._db.execute(
            "SELECT id, version_number FROM sustain_states WHERE sustain_id = ?",
            (sustain_id,),
        ).fetchone()
        if existing is None:
            self._db.execute(
                "INSERT INTO sustain_states (id, sustain_id, state_json, version_number, updated_at) "
                "VALUES (?, ?, ?, 1, ?)",
                (str(uuid.uuid4()), sustain_id, state_json, now),
            )
        else:
            self._db.execute(
                "UPDATE sustain_states "
                "SET state_json = ?, version_number = ?, updated_at = ? "
                "WHERE sustain_id = ?",
                (state_json, existing["version_number"] + 1, now, sustain_id),
            )

    def _append_events_and_update_cache(
        self, sustain_id: str, new_state: dict, events: list[dict],
    ) -> None:
        """
        The ONLY place state is allowed to change once a sustain exists.

        events: [{"id"?, "event_name", "payload", "operator_log_id"?,
                   "timestamp"?, "mutations"}, ...], already in the order they
        should be assigned seq numbers in. Each is inserted into the events
        table with a fresh per-sustain seq; the sustain_states cache is then
        overwritten with new_state (the caller's already-computed fold
        result — recomputing it here via fold_events on every write would be
        correct but O(n) per write as history grows, so the caller passes the
        StateAccessor snapshot it already has in hand; rebuild_state() is the
        from-scratch check that this incremental path never drifts from it).

        Both writes happen in one transaction: if persisting the events fails,
        the cache must not silently move ahead of the log it's supposed to be
        a cache OF. No-op (returns without writing anything) if events is empty.
        """
        if not events:
            return
        try:
            self._write_events_and_cache_no_commit(sustain_id, new_state, events)
            self._db.commit()
        except Exception:
            self._db.rollback()
            raise

    def _commit_atomic_multi_sustain(
        self, entries: list[tuple[str, dict, list[dict]]],
        extra_writes: list | None = None,
    ) -> None:
        """
        Write MORE THAN ONE sustain's events+cache in a single transaction —
        either every entry lands, or none does. This is the literal
        mechanism behind holon.transfer's conservation guarantee (§4E.6):
        "the transfer emits a paired debit+credit under one commit: both
        apply or neither does." Achievable because SustainEngine holds one
        shared sqlite3 connection across every sustain (sustain_id is a
        column value, not a separate database/connection) — a normal
        transaction on that connection genuinely spans both holons.

        entries: [(sustain_id, new_state, events), ...]. An entry with an
        empty events list contributes nothing (same no-op rule as
        _append_events_and_update_cache) but doesn't block the others.

        extra_writes: optional list of zero-arg callables that issue their
        own self._db.execute(...) calls (no commit of their own) — used to
        fold a non-event write (the holon_transfers ledger row) into the
        SAME transaction as the state writes, so a crash between "state
        committed" and "ledger row written" can never happen — the two are
        atomic together, not sequential.
        """
        try:
            for sustain_id, new_state, events in entries:
                if events:
                    self._write_events_and_cache_no_commit(sustain_id, new_state, events)
            for write_fn in (extra_writes or []):
                write_fn()
            self._db.commit()
        except Exception:
            self._db.rollback()
            raise

    def _get_holon_transfer(self, idempotency_key: str) -> dict | None:
        """A previously-committed transfer by its idempotency key, or None."""
        row = self._db.execute(
            "SELECT id, from_sustain_id, to_sustain_id, amount, idempotency_key, created_at "
            "FROM holon_transfers WHERE idempotency_key = ?",
            (idempotency_key,),
        ).fetchone()
        return dict(row) if row else None

    def _write_holon_transfer_no_commit(
        self, transfer_id: str, from_sustain_id: str, to_sustain_id: str,
        amount: float, idempotency_key: str, timestamp: str,
    ) -> None:
        """The audit/dedup ledger row for one committed cross-holon transfer. See _commit_atomic_multi_sustain's extra_writes."""
        self._db.execute(
            "INSERT INTO holon_transfers (id, from_sustain_id, to_sustain_id, amount, idempotency_key, created_at) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (transfer_id, from_sustain_id, to_sustain_id, amount, idempotency_key, timestamp),
        )

    def list_holon_transfers(self, sustain_id: str, limit: int = 20) -> list[dict]:
        """Every transfer touching sustain_id (either side), newest first — for a real transfer-history UI."""
        rows = self._db.execute(
            "SELECT id, from_sustain_id, to_sustain_id, amount, idempotency_key, created_at "
            "FROM holon_transfers WHERE from_sustain_id = ? OR to_sustain_id = ? "
            "ORDER BY created_at DESC LIMIT ?",
            (sustain_id, sustain_id, limit),
        ).fetchall()
        return [dict(r) for r in rows]

    # ── Parse rules as gated edits (Phase 3C, 2 Aug 2026) ───────────────────────
    # Canon: SPEC-parser-primitive-lift.addendum §4I.6 — "D includes the
    # sensing boundary... a correction is an ordinary gated edit — no
    # parallel machinery." Rules are a per-SOURCE (mpesa/kcb), not
    # per-sustain, asset — the same M-Pesa SMS format applies to every
    # user — so this is deliberately its own small versioned store rather
    # than routed through execute_operator()'s single-sustain machinery,
    # which would be the wrong scope entirely. The SAME discipline still
    # applies: type-checked at author time (Gamma |- r), append-only
    # versioned history, declared inverses (AddRule <-> RetireRule), and —
    # for ModifyRule/RetireRule — a real migration-safety check before the
    # edit can land.

    def _get_active_parse_rule_row(self, rule_id: str) -> dict | None:
        row = self._db.execute(
            "SELECT * FROM parse_rule_edits WHERE rule_id = ? AND status = 'active'", (rule_id,),
        ).fetchone()
        return dict(row) if row else None

    def add_parse_rule(self, source: str, rule, author_user_id: str) -> dict:
        """
        AddRule: introduce a new ParseRule. Auto-safe by construction — a
        genuinely new rule_id strictly widens what tau recognises (the
        boundary analog of LoosenInv's V_D subset V_D'), so no migration
        check is needed. Refuses if rule_id already has an active version
        (use modify_parse_rule) or the rule fails Gamma |- r.
        """
        from sustena.core.parse_rule import typecheck_rule

        errors = typecheck_rule(rule)
        if errors:
            return {"status": "failed", "reason": "rule failed to typecheck (Gamma |- r)", "errors": errors}
        if self._get_active_parse_rule_row(rule.id) is not None:
            return {"status": "failed", "reason": f"rule '{rule.id}' already has an active version — use modify_parse_rule instead."}

        now = datetime.utcnow().isoformat()
        self._db.execute(
            "INSERT INTO parse_rule_edits (id, rule_id, source, version, rule_json, edit_name, status, author_user_id, parent_version, created_at) "
            "VALUES (?, ?, ?, 1, ?, 'AddRule', 'active', ?, NULL, ?)",
            (str(uuid.uuid4()), rule.id, source, json.dumps(_parse_rule_to_dict(rule)), author_user_id, now),
        )
        self._db.commit()
        return {"status": "ok", "rule_id": rule.id, "version": 1, "edit_name": "AddRule"}

    def modify_parse_rule(self, rule_id: str, new_rule, author_user_id: str) -> dict:
        """
        ModifyRule: change an existing rule's pattern/extract/maps_to —
        whether "existing" means a prior user correction (an active DB
        row) or a shipped seed rule that has never been corrected before
        (baseline = the seed data itself, first correction becomes DB
        version 1). Always runs the migration check: re-parses the
        baseline rule's own declared examples[] under the CANDIDATE rule
        and refuses if any previously-handled example would now regress
        (mis-parse, or stop matching outright) — narrower than a full
        cross-sustain stored-message corpus scan (a larger, separate
        follow-up, disclosed rather than silently narrowed), but a real,
        non-rubber-stamp safety net.
        """
        from sustena.core.parse_rule import apply_rule, typecheck_rule
        from sustena.core.parse_rules_seed import SEED_RULES_BY_SOURCE

        if new_rule.id != rule_id:
            return {"status": "failed", "reason": "new_rule.id must match the rule_id being modified."}
        errors = typecheck_rule(new_rule)
        if errors:
            return {"status": "failed", "reason": "rule failed to typecheck (Gamma |- r)", "errors": errors}

        existing = self._get_active_parse_rule_row(rule_id)
        if existing is not None:
            old_rule = _parse_rule_from_dict(json.loads(existing["rule_json"]))
            parent_version = existing["version"]
            source = existing["source"]
        else:
            old_rule = None
            for rules in SEED_RULES_BY_SOURCE.values():
                for r in rules:
                    if r.id == rule_id:
                        old_rule = r
                        break
                if old_rule:
                    break
            if old_rule is None:
                return {"status": "failed", "reason": f"rule '{rule_id}' has no active version and no shipped seed rule to modify."}
            parent_version = 0
            source = old_rule.source

        regressions = []
        for example in old_rule.examples:
            old_result = apply_rule(old_rule, example)
            new_result = apply_rule(new_rule, example)
            old_shape = (old_result.status, old_result.operator_name, old_result.operator_params) if old_result else None
            new_shape = (new_result.status, new_result.operator_name, new_result.operator_params) if new_result else None
            if old_shape != new_shape:
                regressions.append({"example": example, "was": old_shape, "now": new_shape})
        if regressions:
            return {
                "status": "failed",
                "reason": "modification would change behaviour for previously-handled examples — refused.",
                "regressions": regressions,
            }

        new_version = parent_version + 1
        now = datetime.utcnow().isoformat()
        if existing is not None:
            self._db.execute("UPDATE parse_rule_edits SET status = 'superseded' WHERE id = ?", (existing["id"],))
        self._db.execute(
            "INSERT INTO parse_rule_edits (id, rule_id, source, version, rule_json, edit_name, status, author_user_id, parent_version, created_at) "
            "VALUES (?, ?, ?, ?, ?, 'ModifyRule', 'active', ?, ?, ?)",
            (str(uuid.uuid4()), rule_id, source, new_version, json.dumps(_parse_rule_to_dict(new_rule)), author_user_id, parent_version, now),
        )
        self._db.commit()
        return {"status": "ok", "rule_id": rule_id, "version": new_version, "edit_name": "ModifyRule"}

    def retire_parse_rule(self, rule_id: str, author_user_id: str) -> dict:
        """RetireRule: remove a rule from its connector's active set. Its declared inverse is add_parse_rule (crystallised as a fresh AddRule row, not a raw un-delete)."""
        existing = self._get_active_parse_rule_row(rule_id)
        if existing is None:
            return {"status": "failed", "reason": f"rule '{rule_id}' has no active version to retire."}
        self._db.execute("UPDATE parse_rule_edits SET status = 'retired' WHERE id = ?", (existing["id"],))
        self._db.commit()
        return {"status": "ok", "rule_id": rule_id, "version": existing["version"], "edit_name": "RetireRule"}

    def list_active_parse_rule_overrides(self, source: str) -> list:
        rows = self._db.execute(
            "SELECT rule_json FROM parse_rule_edits WHERE source = ? AND status = 'active'", (source,),
        ).fetchall()
        return [_parse_rule_from_dict(json.loads(r["rule_json"])) for r in rows]

    def get_effective_parse_rules(self, source: str) -> list:
        """Overrides first (a correction supersedes its shipped default for the same rule_id), then any seed rules not overridden, in the seed's own declared order."""
        from sustena.core.parse_rules_seed import SEED_RULES_BY_SOURCE

        overrides = self.list_active_parse_rule_overrides(source)
        override_ids = {r.id for r in overrides}
        seed = SEED_RULES_BY_SOURCE.get(source, [])
        return overrides + [r for r in seed if r.id not in override_ids]

    def list_parse_rule_history(self, rule_id: str) -> list[dict]:
        """The full append-only edit history for one rule_id, oldest first — §4I.4's `<D, parent_version, edit_name, author, timestamp>` record."""
        rows = self._db.execute(
            "SELECT id, rule_id, source, version, edit_name, status, author_user_id, parent_version, created_at "
            "FROM parse_rule_edits WHERE rule_id = ? ORDER BY version ASC",
            (rule_id,),
        ).fetchall()
        return [dict(r) for r in rows]

    def commit_external_mutation(
        self, sustain_id: str, state: StateAccessor, event_name: str, payload: dict,
        check_gate: bool = False,
    ) -> tuple[bool, str]:
        """
        For state changes that happen outside execute_operator's operator-
        registry path — e.g. a council vote resolving a proposal via
        CouncilSession — but still need state = fold(events) to hold. Captures
        whatever `state` has tracked via its set()/append()/remove() calls,
        appends one event carrying those mutations, and updates the cache.
        No-op (returns (True, "")) if state.mutations() is empty.

        check_gate=False (the default) preserves every existing caller's
        exact original behaviour (seed_pocket, seed_event) — the gate is
        opt-in, not a silent behaviour change for call sites that predate it.

        check_gate=True (used by the council-vote route — Slice 8's second
        slotted follow-up) additionally evaluates the sustain's own compiled,
        ENFORCED invariants (skips aggregate-referencing ones — see
        _compile_spec_invariants) against the mutated state before
        persisting, exactly like the Move 2 gate inside execute_operator. A
        violation refuses the whole mutation: nothing is appended, state is
        untouched, and (False, reason) is returned. There's no operator
        here, so only the sustain's own invariants are checked — no
        post_constraints (those belong to a specific operator call).

        Returns (True, "") on success or no-op, (False, reason) on refusal.
        """
        mutations = state.mutations()
        if not mutations:
            return True, ""

        if check_gate:
            spec = self._get_spec(sustain_id)
            if spec is not None and self._enforcement_enabled(spec):
                for inv in spec.get("_compiled_invariants", []):
                    if not inv.get("enforced", True):
                        continue
                    ok, reason = evaluate_predicate(inv["node"], state, {})
                    if not ok:
                        return False, f"would violate invariant '{inv['id']}' ({inv['expr']}): {reason}"

        self._append_events_and_update_cache(
            sustain_id, state.snapshot(),
            [{"event_name": event_name, "payload": payload, "mutations": mutations}],
        )
        return True, ""

    def rebuild_state(self, sustain_id: str) -> dict:
        """
        Derive state from scratch by folding every event for sustain_id, in
        seq order — completely independent of the sustain_states cache. This
        is the proof that the cache is what it claims to be: call this and
        get_state(sustain_id) back to back and they must be deep-equal.
        """
        rows = self._db.execute(
            "SELECT event_name, payload_json, mutations_json FROM events "
            "WHERE sustain_id = ? ORDER BY seq ASC",
            (sustain_id,),
        ).fetchall()
        events = []
        for r in rows:
            try:
                mutations = json.loads(r["mutations_json"]) if r["mutations_json"] else []
            except (json.JSONDecodeError, TypeError):
                mutations = []
            events.append({"mutations": mutations})
        return fold_events(events)

    def migrate_to_event_sourcing(self) -> dict:
        """
        One-time backfill for sustains that predate Slice 3: every sustain's
        history must start with a genesis "replace_root" event, but sustains
        created before this slice never got one (their initial state was a
        raw cache write). This generates exactly one genesis event per such
        sustain, capturing its CURRENT cached state exactly — not fabricating
        or discarding anything — then renumbers whatever informational events
        already existed for it (mutations = [], since they carry no fold data)
        to come after.

        Iterates the sustains table (the authoritative instance list), not
        sustain_states — a sustain_states row with no matching sustains row
        is orphaned/pre-existing dead data, not a real instance, and is
        reported separately rather than silently touched.

        Idempotent: a sustain that already has a genesis event is skipped.

        Returns a report dict — callers (an operational script, or a test)
        MUST check report["verification"] before trusting the migration; this
        method does not silently swallow a fold mismatch, it surfaces it:
          {
            "migrated": [sustain_id, ...],
            "skipped_already_migrated": [sustain_id, ...],
            "skipped_no_state": [sustain_id, ...],
            "orphaned_state_rows": [sustain_id, ...],  # sustain_states with no sustains row
            "verification": {sustain_id: bool},         # rebuild_state() == get_state() ?
          }
        """
        report: dict = {
            "migrated": [], "skipped_already_migrated": [], "skipped_no_state": [],
            "orphaned_state_rows": [], "verification": {},
        }

        sustain_rows = self._db.execute("SELECT id, template_id, created_at FROM sustains").fetchall()
        known_ids = {row["id"] for row in sustain_rows}

        orphans = self._db.execute("SELECT sustain_id FROM sustain_states").fetchall()
        report["orphaned_state_rows"] = sorted({
            r["sustain_id"] for r in orphans if r["sustain_id"] not in known_ids
        })

        for row in sustain_rows:
            sid = row["id"]

            already = self._db.execute(
                "SELECT COUNT(*) AS n FROM events "
                "WHERE sustain_id = ? AND event_name = 'event.system.genesis_snapshot'",
                (sid,),
            ).fetchone()["n"]
            if already > 0:
                report["skipped_already_migrated"].append(sid)
                continue

            try:
                current_state = self._load_state_dict(sid)
            except ValueError:
                report["skipped_no_state"].append(sid)
                continue

            old_events = self._db.execute(
                "SELECT id FROM events WHERE sustain_id = ? ORDER BY timestamp ASC",
                (sid,),
            ).fetchall()

            try:
                self._db.execute(
                    "INSERT INTO events "
                    "(id, sustain_id, event_name, payload_json, operator_log_id, timestamp, seq, mutations_json) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        str(uuid.uuid4()), sid, "event.system.genesis_snapshot",
                        json.dumps({
                            "template_id": row["template_id"],
                            "reason": (
                                "Slice 3 migration — one-time snapshot capturing this "
                                "sustain's exact state as it stood before event sourcing"
                            ),
                        }),
                        None, row["created_at"], 1,
                        json.dumps([{"op": "replace_root", "value": current_state}], default=str),
                    ),
                )
                seq = 2
                for ev in old_events:
                    self._db.execute(
                        "UPDATE events SET seq = ?, mutations_json = ? WHERE id = ?",
                        (seq, json.dumps([]), ev["id"]),
                    )
                    seq += 1
                self._db.commit()
            except Exception:
                self._db.rollback()
                raise

            report["migrated"].append(sid)
            report["verification"][sid] = (self.rebuild_state(sid) == self.get_state(sid))

        return report

    # ── instantiate ────────────────────────────────────────────────────────────

    def instantiate(
        self,
        template_id: str,
        user_id: str,
        parameters: dict,
    ) -> str:
        """
        Instantiate a new sustain from a spec template.

        Steps:
          1. Load spec from sustena/sustains/{template_id}.json
          2. Validate all required parameters are present (raises ValueError if not)
          3. Deep-copy default_state and resolve {{placeholder}} tokens
          4. Apply parameter-driven state overrides (initial_income, initial_goal_*)
          5. Write sustains row (sustain_id, template_id, user_id, created_at)
          6. Write sustain_states row with the resolved initial state
          7. Instantiate operative instances, bind to the initial StateAccessor

        Returns:
            sustain_id (str UUID)

        Raises:
            ValueError: required parameter missing, or spec file not found.
        """
        spec = self._load_spec(template_id)
        self._compile_spec_invariants(spec)

        # 1. Validate required parameters
        for param_def in spec.get("parameters", []):
            if param_def.get("required", False):
                if param_def["name"] not in parameters:
                    raise ValueError(
                        f"Required parameter '{param_def['name']}' is missing for "
                        f"sustain template '{template_id}'."
                    )

        # 1b. Fill in declared defaults for any OPTIONAL parameter the caller
        # omitted. A real bug found live (2 Aug 2026): habitat.json declares
        # role_in_family as optional with default:"" , but holon.create_child
        # (and any other caller that doesn't explicitly pass it) never
        # supplied it — _resolve_tokens() has no defaulting behaviour of its
        # own, so an omitted token was left as the literal unresolved string
        # "{{role_in_family}}" in the instantiated state, visible to a real
        # user. This was previously worked around per-field (initial_income
        # has its own hardcoded fallback below) rather than fixed at the
        # root; every future optional-with-default param gets this for free
        # now. Copies `parameters` rather than mutating the caller's own
        # dict in place.
        parameters = dict(parameters)
        for param_def in spec.get("parameters", []):
            name = param_def["name"]
            if name not in parameters and "default" in param_def:
                parameters[name] = param_def["default"]

        # 2. Build initial state — token substitution on default_state
        initial_state: dict = copy.deepcopy(spec.get("default_state", {}))
        initial_state = self._resolve_tokens(initial_state, parameters)

        # 3. Apply optional parameter-driven overrides
        if "initial_income" in parameters and parameters["initial_income"] is not None:
            try:
                initial_state["finances"]["income"]["amount"] = float(
                    parameters["initial_income"]
                )
            except (KeyError, TypeError):
                pass  # state schema doesn't have this path — skip

        goal_name = parameters.get("initial_goal_name")
        if goal_name:
            goal_amount   = float(parameters.get("initial_goal_amount") or 0.0)
            goal_deadline = parameters.get("initial_goal_deadline") or ""
            try:
                initial_state["finances"]["goals"].append({
                    "name":           goal_name,
                    "target_amount":  goal_amount,
                    "current_amount": 0.0,
                    "deadline":       goal_deadline,
                })
            except (KeyError, AttributeError):
                pass

        # 4. Persist sustain row
        sustain_id = str(uuid.uuid4())
        now = datetime.utcnow().isoformat()
        self._db.execute(
            "INSERT INTO sustains (id, user_id, template_id, created_at) "
            "VALUES (?, ?, ?, ?)",
            (sustain_id, user_id, template_id, now),
        )
        self._db.commit()

        # 5. Persist initial state as a genesis event — every sustain's fold
        # history starts with exactly one "replace_root" event, whether it's
        # freshly instantiated (here) or a pre-Slice-3 sustain backfilled by
        # migrate_to_event_sourcing(). No sustain ever gets a cache write that
        # isn't backed by a corresponding event.
        self._append_events_and_update_cache(sustain_id, initial_state, [{
            "event_name": "event.system.genesis_snapshot",
            "payload": {"template_id": template_id, "reason": "sustain instantiated"},
            "mutations": [{"op": "replace_root", "value": initial_state}],
        }])

        # 6. Cache spec and owner
        self._specs[sustain_id]  = spec
        self._owners[sustain_id] = user_id

        # 7. Instantiate operatives
        state_accessor = StateAccessor(initial_state)
        operative_instances: dict[str, Any] = {}
        for op_name in spec.get("operatives", []):
            cls = _OPERATIVE_MAP.get(op_name)
            if cls is None:
                logger.warning(
                    "[SustainEngine] Unknown operative '%s' in spec '%s' — skipping.",
                    op_name, template_id,
                )
                continue
            operative_instances[op_name] = cls(
                config={},
                state_accessor=state_accessor,
                claude_client=None,  # injected by higher-level runner when needed
            )

        self._operatives[sustain_id] = operative_instances

        logger.info(
            "[SustainEngine] instantiated sustain_id=%s template=%s user=%s operatives=%s",
            sustain_id, template_id, user_id, list(operative_instances),
        )
        return sustain_id

    # ── get_state ──────────────────────────────────────────────────────────────

    def get_state(self, sustain_id: str) -> dict:
        """Return the current full state dict for the sustain (deep copy)."""
        return copy.deepcopy(self._load_state_dict(sustain_id))

    # ── list_all ───────────────────────────────────────────────────────────────

    def list_all(self, owner_user_id: str | None = None) -> list[dict]:
        """
        Return a summary list of sustain instances in the DB.

        Each entry contains enough for the devui selector dropdown and the
        Monitor panel hero tiles — id, template, name (label), status,
        active operative count, and current pawa balance from state if present.

        owner_user_id=None (the default, used by main.py's startup
        "is the DB empty?" check) returns every sustain system-wide.
        GET /devui/sustains passes the real logged-in user's id so a
        person's picker only ever shows sustains they actually own —
        this was a real gap until this parameter existed: unfiltered,
        any authenticated user saw every other account's (and every
        throwaway test account's) sustains in their own dropdown.
        """
        if owner_user_id:
            rows = self._db.execute(
                "SELECT s.id, s.user_id, s.template_id, s.created_at, "
                "       ss.state_json "
                "FROM sustains s "
                "LEFT JOIN sustain_states ss ON ss.sustain_id = s.id "
                "WHERE s.user_id = ? "
                "ORDER BY s.created_at DESC",
                (owner_user_id,),
            ).fetchall()
        else:
            rows = self._db.execute(
                "SELECT s.id, s.user_id, s.template_id, s.created_at, "
                "       ss.state_json "
                "FROM sustains s "
                "LEFT JOIN sustain_states ss ON ss.sustain_id = s.id "
                "ORDER BY s.created_at DESC"
            ).fetchall()

        result = []
        for row in rows:
            sustain_id   = row["id"]
            template_id  = row["template_id"]
            state: dict  = {}
            if row["state_json"]:
                try:
                    state = json.loads(row["state_json"])
                except (json.JSONDecodeError, TypeError):
                    pass

            # Extract pocket summary from state if available
            pockets: dict = {}
            try:
                raw_pockets = state.get("finances", {}).get("pockets", {})
                pockets = {
                    k: v.get("allocated", 0) if isinstance(v, dict) else v
                    for k, v in raw_pockets.items()
                }
            except (AttributeError, TypeError):
                pass

            pawa_balance: int = state.get("system", {}).get("pawa_balance", 0)
            active_operatives = list(self._operatives.get(sustain_id, {}).keys())

            # A user-created definition's template_id is a UUID, not a nice
            # word — "3f2a1b4c..." would be a useless label. Prefer the
            # spec's own display_name when available (cheap: _get_spec caches
            # per sustain_id after the first lookup); fall back to the old
            # template_id-derived label for built-ins with no display_name
            # mismatch risk.
            label = template_id.replace("_", " ").title()
            spec = self._get_spec(sustain_id)
            if spec and spec.get("display_name"):
                label = spec["display_name"]

            result.append({
                "id":                sustain_id,
                "label":             label,
                "sub":               row["user_id"],
                "status":            "live",
                "template_id":       template_id,
                "created_at":        row["created_at"],
                "pockets":           pockets,
                "active_operatives": active_operatives,
                "pawa_balance":      pawa_balance,
            })

        return result

    # ── reassign_sustain_owner ────────────────────────────────────────────────

    def reassign_sustain_owner(self, sustain_id: str, from_user_id: str, to_user_id: str) -> dict:
        """
        One-off ownership correction for a sustain that predates real
        per-user accounts and is still stamped with a legacy owner string
        (e.g. "system", the main.py startup-seed sentinel from before
        Slice 1's login work) or was otherwise mis-attributed.

        Same discipline as migrate_to_event_sourcing(): idempotent, never
        blind, returns a report the caller must check rather than silently
        succeeding. The write ONLY happens if the sustain's CURRENT owner
        is exactly from_user_id -- this is a targeted correction of one
        known-bad row, not a blind "set owner to X" that could clobber a
        legitimately different owner if the caller's assumption is stale.

        Generic -- not homestead-specific. Works on any sustain, any two
        user ids. Checks (best-effort) that to_user_id resolves to a real
        row in the `users` table, since a typo'd target id would otherwise
        silently orphan the sustain from every real account.

        Returns:
          {
            "status": "reassigned" | "already_owner" | "owner_mismatch"
                       | "sustain_not_found" | "target_user_not_found",
            "sustain_id", "old_owner", "new_owner",
          }
        """
        row = self._db.execute(
            "SELECT user_id FROM sustains WHERE id = ?", (sustain_id,),
        ).fetchone()
        if row is None:
            return {"status": "sustain_not_found", "sustain_id": sustain_id, "old_owner": None, "new_owner": to_user_id}

        current_owner = row["user_id"]

        if current_owner == to_user_id:
            return {"status": "already_owner", "sustain_id": sustain_id, "old_owner": current_owner, "new_owner": to_user_id}

        if current_owner != from_user_id:
            return {"status": "owner_mismatch", "sustain_id": sustain_id, "old_owner": current_owner, "new_owner": to_user_id}

        try:
            target_user = self._db.execute(
                "SELECT id FROM users WHERE id = ?", (to_user_id,),
            ).fetchone()
            if target_user is None:
                return {"status": "target_user_not_found", "sustain_id": sustain_id, "old_owner": current_owner, "new_owner": to_user_id}
        except sqlite3.OperationalError:
            # No `users` table on this connection (e.g. a minimal/test-only
            # SustainEngine not wired to the app's full schema) -- the
            # existence check is inapplicable here, not a real failure.
            pass

        self._db.execute(
            "UPDATE sustains SET user_id = ? WHERE id = ? AND user_id = ?",
            (to_user_id, sustain_id, from_user_id),
        )
        self._db.commit()

        return {"status": "reassigned", "sustain_id": sustain_id, "old_owner": current_owner, "new_owner": to_user_id}

    # ── delete_child_sustain / reset_sustain_to_clean_slate ────────────────────
    # A user-requested "practice on a clean slate" reset (2 Aug 2026). Same
    # discipline as reassign_sustain_owner()/purge_test_egress_entries():
    # ownership-checked (refuses rather than silently no-opping on a
    # mismatch), scoped by exact sustain_id (never a blanket wipe), returns
    # a report the caller must check. Generic -- nothing here references
    # "homestead"/"habitat"; a habitat is just one child sustain among
    # however many a parent happens to have linked.

    # Every table a sustain's own operational data can appear in, keyed by
    # sustain_id directly. events/sustain_states/sustains are handled
    # separately (state-fold semantics, or the row identity itself) --
    # this list is the flat "just delete WHERE sustain_id = ?" set.
    _SUSTAIN_SCOPED_TABLES = (
        "operative_overrides", "operative_suggestions", "egress_outbox",
        "pawa_meter_log", "operators_log", "pawa_ledger",
    )

    def _delete_sustain_scoped_rows(self, sustain_id: str) -> dict:
        """Delete every row keyed by sustain_id across the flat operational
        tables above, plus council_proposals (and their council_votes,
        which reference proposal_id, not sustain_id directly -- resolved
        via a subquery). A table that doesn't exist on this connection
        (e.g. a minimal test-only engine never wired to the full async
        schema) is skipped, not fatal -- same defensive pattern
        reassign_sustain_owner() already uses for the `users` table."""
        removed: dict[str, int] = {}
        for table in self._SUSTAIN_SCOPED_TABLES:
            try:
                cur = self._db.execute(f"DELETE FROM {table} WHERE sustain_id = ?", (sustain_id,))
                removed[table] = cur.rowcount
            except sqlite3.OperationalError:
                removed[table] = 0
        try:
            proposal_ids = [
                r["id"] for r in self._db.execute(
                    "SELECT id FROM council_proposals WHERE sustain_id = ?", (sustain_id,),
                ).fetchall()
            ]
            if proposal_ids:
                placeholders = ",".join("?" * len(proposal_ids))
                self._db.execute(f"DELETE FROM council_votes WHERE proposal_id IN ({placeholders})", proposal_ids)
            cur = self._db.execute("DELETE FROM council_proposals WHERE sustain_id = ?", (sustain_id,))
            removed["council_proposals"] = cur.rowcount
            removed["council_votes"] = len(proposal_ids)
        except sqlite3.OperationalError:
            removed["council_proposals"] = 0
            removed["council_votes"] = 0
        self._db.commit()
        return removed

    def delete_child_sustain(self, child_sustain_id: str, owner_user_id: str) -> dict:
        """
        Fully and permanently remove ONE sustain -- unlinks it from any
        parent, deletes every row scoped to it (events, cached state, and
        every table _delete_sustain_scoped_rows() covers), then the
        `sustains` row itself. Unlike unlink_child() (disaggregation --
        the child keeps its own state, only the link is dissolved), this
        actually destroys the child. Never call this on a sustain you
        haven't already backed up.

        Ownership-checked: refuses (does NOT delete anything) if the
        sustain doesn't exist or isn't owned by owner_user_id -- the same
        "refuse, don't silently no-op on a stale assumption" discipline as
        reassign_sustain_owner().

        Returns {"status": "deleted"|"not_found"|"owner_mismatch",
                 "sustain_id", "events_removed", "table_counts"}.
        """
        row = self._db.execute("SELECT user_id FROM sustains WHERE id = ?", (child_sustain_id,)).fetchone()
        if row is None:
            return {"status": "not_found", "sustain_id": child_sustain_id, "events_removed": 0, "table_counts": {}}
        if row["user_id"] != owner_user_id:
            return {"status": "owner_mismatch", "sustain_id": child_sustain_id, "events_removed": 0, "table_counts": {}}

        # Unlink both directions -- as a child under some parent, and (for
        # generality, even though no habitat has ever had its own children)
        # as a parent of anything linked under IT.
        self._db.execute("DELETE FROM sustain_composition WHERE child_sustain_id = ?", (child_sustain_id,))
        self._db.execute("DELETE FROM sustain_composition WHERE parent_sustain_id = ?", (child_sustain_id,))

        events_removed = self._db.execute(
            "DELETE FROM events WHERE sustain_id = ?", (child_sustain_id,),
        ).rowcount
        self._db.execute("DELETE FROM sustain_states WHERE sustain_id = ?", (child_sustain_id,))
        table_counts = self._delete_sustain_scoped_rows(child_sustain_id)
        self._db.execute("DELETE FROM sustains WHERE id = ?", (child_sustain_id,))
        self._db.commit()

        # Drop this engine's in-memory caches for the now-deleted sustain --
        # nothing can reach it via the ownership-checked API paths once its
        # `sustains` row is gone, but a stale cache entry is still tidiness
        # worth doing (same instinct as Slice 6's cache-invalidation fix).
        self._specs.pop(child_sustain_id, None)
        self._owners.pop(child_sustain_id, None)
        self._operatives.pop(child_sustain_id, None)

        logger.info(
            "[SustainEngine] deleted child sustain_id=%s (owner=%s): events_removed=%d table_counts=%s",
            child_sustain_id, owner_user_id, events_removed, table_counts,
        )
        return {
            "status": "deleted", "sustain_id": child_sustain_id,
            "events_removed": events_removed, "table_counts": table_counts,
        }

    def reset_sustain_to_clean_slate(self, sustain_id: str, owner_user_id: str) -> dict:
        """
        Reset ONE sustain to a genuinely empty starting state -- every
        pocket/dimension gone, every linked child sustain permanently
        deleted (not just unlinked -- see delete_child_sustain()), every
        operational row (suggestions, egress, pawa metering, operator
        execution log, council proposals/votes) scoped to it cleared. The
        `sustains` row and its owner are UNTOUCHED -- this empties a
        sustain, it does not delete it; the account and its login are
        never touched by this method at all.

        The state reset itself goes through the exact same event-sourcing
        primitive every other write in this engine uses
        (_append_events_and_update_cache) -- old event rows are deleted
        (a real, disclosed choice: a superseding replace_root would leave
        rebuild_state() correct but the event log cluttered with the old
        history a "clean slate to practice on" is explicitly asking to be
        rid of) and replaced with exactly ONE fresh event carrying
        {"op": "replace_root", "value": <this template's own default_state>}
        -- structurally identical to what instantiate() itself writes for
        a brand-new sustain of the same template. rebuild_state() and
        get_state() are therefore trivially equal afterward, by
        construction, not by coincidence.

        Does NOT touch ingest_messages/ingest_sources/
        capture_classification_history -- those are IngestEngine's tables,
        not SustainEngine's; call IngestEngine.purge_sustain_data() (once
        per sustain_id you want cleared) alongside this for a full reset.

        Ownership-checked exactly like delete_child_sustain(): refuses
        (does nothing) rather than silently resetting the wrong sustain.

        Returns {"status": "reset"|"not_found"|"owner_mismatch",
                 "sustain_id", "habitats_removed": [...ids...],
                 "old_event_count", "old_pockets": [...names...],
                 "table_counts"}.
        """
        row = self._db.execute(
            "SELECT user_id, template_id FROM sustains WHERE id = ?", (sustain_id,),
        ).fetchone()
        if row is None:
            return {"status": "not_found", "sustain_id": sustain_id, "habitats_removed": [], "old_event_count": 0, "old_pockets": [], "table_counts": {}}
        if row["user_id"] != owner_user_id:
            return {"status": "owner_mismatch", "sustain_id": sustain_id, "habitats_removed": [], "old_event_count": 0, "old_pockets": [], "table_counts": {}}

        # Snapshot what's about to be gone, for the report -- never assume,
        # always report exactly what was found and removed.
        try:
            old_state = self._load_state_dict(sustain_id)
        except ValueError:
            old_state = {}
        old_pockets = sorted((old_state.get("finances") or {}).get("pockets", {}).keys())
        old_event_count = self._db.execute(
            "SELECT COUNT(*) AS n FROM events WHERE sustain_id = ?", (sustain_id,),
        ).fetchone()["n"]

        # Permanently remove every linked child FIRST (each is fully backed
        # up before this method is ever called against real data -- see the
        # caller's own pre-flight backup step).
        children = self.list_children(sustain_id)
        habitats_removed = []
        for child in children:
            result = self.delete_child_sustain(child["child_sustain_id"], owner_user_id)
            if result["status"] == "deleted":
                habitats_removed.append(child["child_sustain_id"])
        # Defensive: an orphaned composition row (a link whose child sustain
        # row was already gone for some other reason) has no delete_child_
        # sustain() call to clean it up via the child-side DELETE above --
        # clear any parent-side leftovers explicitly.
        self._db.execute("DELETE FROM sustain_composition WHERE parent_sustain_id = ?", (sustain_id,))

        table_counts = self._delete_sustain_scoped_rows(sustain_id)

        self._db.execute("DELETE FROM events WHERE sustain_id = ?", (sustain_id,))
        self._db.commit()

        spec = self._load_spec(row["template_id"])
        default_state = copy.deepcopy(spec.get("default_state", {}))
        self._append_events_and_update_cache(sustain_id, default_state, [{
            "event_name": "event.system.reset_to_clean_slate",
            "payload": {
                "template_id": row["template_id"],
                "reason": "user-requested clean-slate reset",
                "old_event_count": old_event_count,
                "old_pockets": old_pockets,
                "habitats_removed": habitats_removed,
            },
            "mutations": [{"op": "replace_root", "value": default_state}],
        }])

        logger.info(
            "[SustainEngine] reset sustain_id=%s to clean slate (owner=%s): "
            "habitats_removed=%d old_event_count=%d old_pockets=%s",
            sustain_id, owner_user_id, len(habitats_removed), old_event_count, old_pockets,
        )
        return {
            "status": "reset", "sustain_id": sustain_id, "habitats_removed": habitats_removed,
            "old_event_count": old_event_count, "old_pockets": old_pockets, "table_counts": table_counts,
        }

    # ── execute_operator ───────────────────────────────────────────────────────

    async def execute_operator(
        self,
        sustain_id: str,
        operator_name: str,
        params: dict,
        operative_id: str | None = None,
        origin_message_id: str | None = None,
    ) -> OperatorResult:
        """
        Execute a named operator against a sustain's live state.

        Steps:
          1. Verify sustain exists and operator is in the spec's allowed list
          2. Verify operator is in OPERATOR_REGISTRY
          3. Load current state into a StateAccessor
          4. Build OperatorContext (state, events, pawa, ids, timestamp)
          5. Call the operator function with **params
          6. Move 2 enforcement gate against the mutated state, if opted in
          7. On success: append the event(s) this call produced (carrying
             StateAccessor's mutations) and update the sustain_states cache —
             state = fold(events); see _append_events_and_update_cache
          8. Return the OperatorResult

        If the operator raises an exception, returns OperatorResult.fail()
        with the exception message — state is NOT persisted.

        origin_message_id (optional): when this call originated from a
        captured message (ingest_engine._process()'s own mapped-capture
        call, or Orchie's capture_confirm route), stamping it directly onto
        the resulting event's payload gives the processed/activity list
        (2 Aug 2026) a REAL, structural link back to the raw message behind
        a transaction -- not a guessed join on amount/timestamp proximity.
        Every existing caller omits this and is completely unaffected.

        Returns:
            OperatorResult (check .succeeded / .failed, .data, .reason)
        """
        t_start = time.monotonic()

        # Load and validate spec
        spec = self._get_spec(sustain_id)
        if spec is None:
            return OperatorResult.fail(
                reason=f"Sustain '{sustain_id}' not found or has no spec.",
                constraint_violated="sustain_exists",
            )

        # Check operator is permitted by this sustain's spec
        allowed_ops = {op["name"] for op in spec.get("operators", [])}
        if operator_name not in allowed_ops:
            return OperatorResult.fail(
                reason=(
                    f"Operator '{operator_name}' is not in the operator list for "
                    f"this sustain. Allowed: {sorted(allowed_ops)}"
                ),
                constraint_violated="operator_allowed",
            )

        # Check operator is implemented
        if operator_name not in OPERATOR_REGISTRY:
            return OperatorResult.fail(
                reason=f"Operator '{operator_name}' is not implemented (not in OPERATOR_REGISTRY).",
                constraint_violated="operator_implemented",
            )

        # Load current state
        state_dict = self._load_state_dict(sustain_id)
        state = StateAccessor(state_dict)

        # Build execution context
        user_id = self._owners.get(sustain_id, "unknown")
        bus     = EventBus(sustain_id=sustain_id)
        ledger  = PawaLedger()
        egress_queue = EgressQueue(sustain_id)
        ctx = OperatorContext(
            state=state,
            events=bus,
            pawa=ledger,
            sustain_id=sustain_id,
            user_id=user_id,
            operative_id=operative_id,
            timestamp=datetime.utcnow(),
            egress=egress_queue,
            engine=self,
        )

        # Execute operator — catch all exceptions, return fail result
        meta = OPERATOR_REGISTRY[operator_name]
        try:
            result: OperatorResult = await meta.fn(ctx, **params)
        except Exception as exc:
            logger.error(
                "[SustainEngine] operator '%s' raised %s: %s",
                operator_name, type(exc).__name__, exc,
            )
            return OperatorResult.fail(
                reason=f"Operator '{operator_name}' raised {type(exc).__name__}: {exc}",
                constraint_violated="operator_runtime_error",
            )

        # Move 2 — enforcing gate. Only on sustains that have opted in
        # (spec["enforcement"]["enabled"]); refusal is a typed OperatorResult.fail,
        # not an exception, and leaves the live state untouched (nothing persisted).
        if result.succeeded and self._enforcement_enabled(spec):
            gate_ok, gate_reason = self._check_enforcement_gate(spec, state, params, meta)
            if not gate_ok:
                logger.info(
                    "[SustainEngine] operator '%s' refused on sustain '%s': %s",
                    operator_name, sustain_id, gate_reason,
                )
                return OperatorResult.fail(reason=gate_reason, constraint_violated="enforcement_gate")

        # Composition/roll-up (Slice 8, revised per Bonnie's Option C): if
        # this sustain is a CHILD of a parent, and that parent has a binding
        # aggregate invariant, check whether this call's candidate state
        # would newly breach it — checked after this sustain's own gate (a
        # cheaper, more relevant failure surfaces first) but before anything
        # is persisted. No-op for the overwhelming majority of sustains
        # (anything with no parent, or a parent with no binding rules).
        if result.succeeded:
            parent_ok, parent_reason = self._check_parent_binding_gate(sustain_id, state.snapshot())
            if not parent_ok:
                logger.info(
                    "[SustainEngine] operator '%s' on '%s' refused by parent binding rule: %s",
                    operator_name, sustain_id, parent_reason,
                )
                return OperatorResult.fail(reason=parent_reason, constraint_violated="parent_binding_gate")

        # Slice 3 — state = fold(events). Append this call's event(s), each
        # carrying whatever StateAccessor recorded, and update the cache — the
        # only place a successful operator call is allowed to change state.
        #
        # If the operator mutated state but published nothing (no event to
        # attach the mutations to), synthesize one — every state change must
        # land in the log, with no silent exceptions for a forgetful operator.
        #
        # If it published MORE THAN ONE event in the same call (e.g.
        # mkulima.receive_signal fires both a "signal received" event and a
        # "biashara evaluation requested" hook), the mutations are attached to
        # the FIRST event only; later events in the same call carry an empty
        # mutations list. This is deliberate, not an oversight: state.mutations()
        # reflects everything that changed during the WHOLE operator call, not
        # per-publish-call — attaching the same list to every event in the call
        # would double-apply it on fold. The extra events stay fully present
        # and readable in the log; they just aren't fold contributors.
        if result.succeeded:
            mutations = state.mutations()
            published = bus.published_this_context()
            if mutations and not published:
                await bus.publish(
                    "event.system.unlogged_state_change",
                    {
                        "operator": operator_name,
                        "note": (
                            "operator mutated state without publishing a domain event; "
                            "synthesized by the engine so the event log stays complete"
                        ),
                    },
                )
                published = bus.published_this_context()
            events_norm: list[dict] = []
            if published:
                events_norm = [
                    {
                        "id": ev.get("id"),
                        "event_name": ev.get("event_name"),
                        "payload": (
                            {**ev.get("_payload", {}), "origin_message_id": origin_message_id}
                            if origin_message_id else ev.get("_payload", {})
                        ),
                        "operator_log_id": ev.get("operator_log_id"),
                        "timestamp": ev.get("timestamp"),
                        "mutations": mutations if i == 0 else [],
                    }
                    for i, ev in enumerate(published)
                ]
                self._append_events_and_update_cache(sustain_id, state.snapshot(), events_norm)

            # Pawa meter (§4L) — the odometer, not the gas pump: records a
            # real ⟨compute, storage⟩ reading for this run. compute is a
            # reproducible proxy (mutations + events + constraint checks
            # actually performed, NOT wall-clock — see pawa_meter.py's own
            # docstring for why); storage is the real serialized byte size
            # of exactly what just got durably written above. Only real,
            # successful runs are metered — a gate refusal did zero real
            # work (nothing mutated, nothing persisted), so it honestly
            # meters as nothing, not as a failed-but-costly attempt.
            storage_bytes = sum(
                len(json.dumps(e["payload"], default=str)) + len(json.dumps(e["mutations"], default=str))
                for e in events_norm
            )
            gate_ran = self._enforcement_enabled(spec)
            compute = pawa_meter.compute_units(
                mutation_count=len(mutations),
                event_count=len(published),
                constraint_eval_count=pawa_meter.constraint_eval_count(meta, spec, gate_ran),
            )
            pawa = pawa_meter.compute_pawa(compute, storage_bytes)
            elapsed_ms = (time.monotonic() - t_start) * 1000
            self._record_pawa_meter(
                sustain_id=sustain_id, operator_name=operator_name, principal=user_id,
                compute=compute, storage=storage_bytes, pawa=pawa, elapsed_ms=elapsed_ms,
            )

        # Slice 11 (Egress) — drain anything the operator queued via
        # ctx.egress into the outbox. This is the ONLY place a queued
        # egress intent becomes a real 'prepared' row: simulate() also
        # passes a real EgressQueue so operators run identically in the
        # sandbox, but simulate() never reaches this line, so a sandboxed
        # egress.prepare_* call queues in-memory and is discarded with the
        # rest of the forked context — nothing is ever persisted from a
        # simulation. Persisting here (after the gate, after the fold) is
        # itself never a send — see _queue_egress()/confirm_egress().
        if result.succeeded:
            for item in egress_queue.queued_this_context():
                self._queue_egress(sustain_id, item)

        return result

    def get_events(self, sustain_id: str, limit: int = 20) -> list[dict]:
        """
        Return recent events for a sustain (newest first) from the events table.
        Shape: [{"event_name": str, "payload": dict, "timestamp": str}].
        Used by GET /devui/state and POST /devui/console/execute.

        Ordered by seq (Slice 3), not just timestamp — timestamp alone is not
        a reliable ordering key: several events from one operator call, or a
        genesis event immediately followed by a seeded one, can land in the
        same millisecond, and ORDER BY timestamp DESC then ties arbitrarily.
        seq is assigned per-sustain and strictly increasing, so it's always
        correct; timestamp DESC only remains as a tiebreaker for any
        pre-Slice-3 rows that still have seq = NULL (SQLite sorts NULL last
        in DESC, so those trail behind everything real, which is the honest
        answer until migrate_to_event_sourcing() backfills them).
        """
        try:
            rows = self._db.execute(
                "SELECT event_name, payload_json, timestamp FROM events "
                "WHERE sustain_id = ? ORDER BY seq DESC, timestamp DESC LIMIT ?",
                (sustain_id, limit),
            ).fetchall()
        except Exception as exc:  # pragma: no cover - defensive
            logger.debug("get_events(%s) failed: %s", sustain_id, exc)
            return []
        out: list[dict] = []
        for r in rows:
            try:
                payload = json.loads(r["payload_json"]) if r["payload_json"] else {}
            except (json.JSONDecodeError, TypeError):
                payload = {}
            out.append({
                "event_name": r["event_name"],
                "payload": payload,
                "timestamp": r["timestamp"],
            })
        return out

    def seed_pocket(self, sustain_id: str, name: str, allocated: float, ceiling: float = 0.0) -> bool:
        """
        Set/replace a budget pocket in a sustain's live engine state so seeded
        pockets appear in the Monitor (which reads engine state). Used by
        POST /seed/pocket. Returns False if the sustain has no engine state
        (e.g. a free-text seed id that was never instantiated).

        Goes through commit_external_mutation (Slice 3) rather than a raw
        cache write — a pocket seeded via the UI is a real state change a
        real user made, so it must land in the event log the same as any
        operator-driven one, or rebuild_state() would silently diverge from
        get_state() for any sustain that ever used the Seed panel.
        """
        try:
            state_dict = self._load_state_dict(sustain_id)
        except ValueError:
            return False
        state = StateAccessor(state_dict)
        prev = state.get(f"finances.pockets.{name}")
        prev = prev if isinstance(prev, dict) else {}
        pocket = {
            "allocated": float(allocated),
            "spent": float(prev.get("spent", 0.0)),
            "limit": float(ceiling),
        }
        state.set(f"finances.pockets.{name}", pocket)
        self.commit_external_mutation(
            sustain_id, state,
            "event.finances.pocket_seeded",
            {"pocket": name, "allocated": pocket["allocated"], "limit": pocket["limit"]},
        )
        return True

    def seed_event(self, sustain_id: str, event_name: str, payload: dict) -> bool:
        """
        Record a purely informational seeded event (no state change) so it
        shows in the Monitor event feed/log. Routed through
        _append_events_and_update_cache so it gets a real seq number in the
        same per-sustain sequence as every other event — a raw INSERT here
        would leave seq NULL and sort ahead of the genesis event on replay.
        Returns False if the sustain has no engine state.
        """
        try:
            state_dict = self._load_state_dict(sustain_id)
        except ValueError:
            return False
        try:
            self._append_events_and_update_cache(sustain_id, state_dict, [{
                "event_name": event_name, "payload": payload, "mutations": [],
            }])
            return True
        except Exception as exc:  # pragma: no cover - defensive
            logger.warning("[SustainEngine] seed_event failed: %s", exc)
            return False

    # ── simulate ───────────────────────────────────────────────────────────────

    async def simulate(
        self,
        sustain_id: str,
        operator_sequence: list[dict],
    ) -> list[dict]:
        """
        Run a sequence of operators against a forked (deep-copied) state.

        The database is NEVER written to — the original live state is
        untouched, no event is ever appended, and nothing published through
        the in-memory EventBus used here (db_session=None) has anywhere to
        persist to. Useful for forward-simulation before committing to a
        Council proposal, and the sandbox core of the Simulator panel's
        scenario tree (Slice 9).

        Each item in operator_sequence:
            {"operator": "<name>", "params": {<kwargs>}}

        Returns a list of per-step result dicts:
            {
                "operator":      str,
                "params":        dict,
                "result":        OperatorResult,
                "state_after":   dict,        # state snapshot after this step
                "parent_rollup": dict | None,  # hypothetical roll-up on the
                                                # parent if sustain_id has one,
                                                # else None — read-only, never
                                                # written anywhere (Slice 9)
                "pawa":            float,      # §4L — this step's own metered
                                                # pawa; 0 for a refused/failed
                                                # step (zero real work, same
                                                # convention as execute_operator)
                "cumulative_pawa": float,      # running total through this step —
                                                # the last step's value is the
                                                # whole branch's efficiency score
            }

        Steps that fail do NOT advance the forked state — subsequent steps see
        the state as it was before the failed step. Nothing here is ever
        written to pawa_meter_log — a simulated run isn't a real one, exactly
        like it never appends a real event; the pawa figures are computed
        with the identical formula (sustena.core.pawa_meter) so a promoted
        branch's real meter reading will match what was estimated here.
        """
        spec = self._get_spec(sustain_id)
        allowed_ops = (
            {op["name"] for op in spec.get("operators", [])}
            if spec else set()
        )

        # Fork — deep copy, never touches DB
        forked_dict = copy.deepcopy(self._load_state_dict(sustain_id))
        user_id = self._owners.get(sustain_id, "unknown")
        parent_link = self.get_parent(sustain_id)
        results: list[dict] = []
        cumulative_pawa = 0.0

        def _parent_rollup_for(state_dict: dict) -> dict | None:
            """Hypothetical roll-up on the parent (Slice 7's composition
            machinery) as if state_dict were sustain_id's committed state —
            purely a read/compute, exactly like the display-only path
            evaluate_constraints() already uses. None if sustain_id has no
            parent, or the parent can't be resolved."""
            if parent_link is None:
                return None
            try:
                return self._hypothetical_rollup(
                    parent_link["parent_sustain_id"],
                    override_child_id=sustain_id, override_state=state_dict,
                )
            except ValueError:
                return None

        for step in operator_sequence:
            operator_name = step.get("operator", "")
            params        = step.get("params", {})

            # Validate step operator
            if operator_name not in allowed_ops:
                result = OperatorResult.fail(
                    reason=f"Operator '{operator_name}' not allowed in this sustain.",
                    constraint_violated="operator_allowed",
                )
                results.append({
                    "operator":    operator_name,
                    "params":      params,
                    "result":      result,
                    "state_after": copy.deepcopy(forked_dict),
                    "parent_rollup": _parent_rollup_for(forked_dict),
                    "pawa": 0.0,
                    "cumulative_pawa": cumulative_pawa,
                })
                continue

            if operator_name not in OPERATOR_REGISTRY:
                result = OperatorResult.fail(
                    reason=f"Operator '{operator_name}' not in OPERATOR_REGISTRY.",
                    constraint_violated="operator_implemented",
                )
                results.append({
                    "operator":    operator_name,
                    "params":      params,
                    "result":      result,
                    "state_after": copy.deepcopy(forked_dict),
                    "parent_rollup": _parent_rollup_for(forked_dict),
                    "pawa": 0.0,
                    "cumulative_pawa": cumulative_pawa,
                })
                continue

            # Run against the forked state
            state_accessor = StateAccessor(forked_dict)
            bus    = EventBus(sustain_id=f"sim:{sustain_id}")
            ledger = PawaLedger()
            # A real EgressQueue so egress.* operators run identically in
            # the sandbox — but simulate() never reads queued_this_context()
            # into egress_outbox (only execute_operator() does), so anything
            # queued here is discarded with the rest of the forked context.
            # Nothing a simulated branch does can ever prepare a real outbox
            # entry, exactly like it can never append a real event.
            egress_queue = EgressQueue(sustain_id)
            ctx = OperatorContext(
                state=state_accessor,
                events=bus,
                pawa=ledger,
                sustain_id=sustain_id,
                user_id=user_id,
                operative_id="simulate",
                timestamp=datetime.utcnow(),
                egress=egress_queue,
            )

            meta = OPERATOR_REGISTRY[operator_name]
            try:
                result = await meta.fn(ctx, **params)
            except Exception as exc:
                result = OperatorResult.fail(
                    reason=f"Operator '{operator_name}' raised {type(exc).__name__}: {exc}",
                    constraint_violated="operator_runtime_error",
                )

            # Move 2 — same enforcing gate as execute_operator, so a forked
            # simulation can never advance past a state the live path would refuse.
            if result.succeeded and self._enforcement_enabled(spec):
                gate_ok, gate_reason = self._check_enforcement_gate(spec, state_accessor, params, meta)
                if not gate_ok:
                    result = OperatorResult.fail(reason=gate_reason, constraint_violated="enforcement_gate")

            # Slice 9: the same parent-binding-authority check execute_operator
            # runs (Slice 7/Option C) — a simulated step that would newly
            # breach a BINDING parent aggregate invariant is refused in the
            # sandbox exactly like it would be for real. Purely read-only:
            # _check_parent_binding_gate never writes anything.
            if result.succeeded:
                parent_ok, parent_reason = self._check_parent_binding_gate(sustain_id, state_accessor.snapshot())
                if not parent_ok:
                    result = OperatorResult.fail(reason=parent_reason, constraint_violated="parent_binding_gate")

            # Advance forked state only on success
            step_pawa = 0.0
            if result.succeeded:
                forked_dict = state_accessor.snapshot()
                # Same formula execute_operator uses (sustena.core.pawa_meter),
                # computed against the sandbox's own mutations/events -- a
                # refused step never reaches here, so it stays 0.0, matching
                # "a refusal is zero real work" for real runs.
                mutations = state_accessor.mutations()
                published = bus.published_this_context()
                gate_ran = self._enforcement_enabled(spec)
                step_compute = pawa_meter.compute_units(
                    mutation_count=len(mutations),
                    event_count=len(published),
                    constraint_eval_count=pawa_meter.constraint_eval_count(meta, spec, gate_ran),
                )
                step_storage = sum(len(json.dumps(ev.get("_payload", {}), default=str)) for ev in published)
                step_pawa = pawa_meter.compute_pawa(step_compute, step_storage)
                cumulative_pawa += step_pawa

            results.append({
                "operator":    operator_name,
                "params":      params,
                "result":      result,
                "state_after": copy.deepcopy(forked_dict),
                "parent_rollup": _parent_rollup_for(forked_dict),
                "pawa": step_pawa,
                "cumulative_pawa": cumulative_pawa,
            })

        return results

    async def promote_simulation(self, sustain_id: str, operator_sequence: list[dict]) -> list[dict]:
        """
        Slice 9's "promote this branch to reality" — genuinely replays
        operator_sequence against LIVE state, in order, through the exact
        same execute_operator() every real Console/API call uses. Same
        gate, real events, real fold-append. This is NOT a shortcut that
        trusts the earlier simulate() result: each step is re-run for real,
        so if live state has drifted since the simulation was built (another
        operator ran in between, a linked child changed, anything), a step
        that passed in simulation can legitimately fail here — that is the
        honest, correct outcome, not a bug to hide.

        Stops at the FIRST real failure — later steps in the sequence are
        never attempted once one refuses, since they were only ever
        validated against a state that's now been proven wrong.

        Returns one entry per step ATTEMPTED (not the full original
        sequence if it stopped early):
            {"operator": str, "params": dict, "result": OperatorResult}
        """
        results: list[dict] = []
        for step in operator_sequence:
            operator_name = step.get("operator", "")
            params = step.get("params", {})
            result = await self.execute_operator(sustain_id, operator_name, params)
            results.append({"operator": operator_name, "params": params, "result": result})
            if not result.succeeded:
                break
        return results

    # ── Internal helpers ───────────────────────────────────────────────────────

    def _get_spec(self, sustain_id: str) -> dict | None:
        """
        Return the cached spec for sustain_id.
        If not in cache, look up template_id from DB and reload from disk.
        Returns None if sustain_id is not found.
        """
        if sustain_id in self._specs:
            return self._specs[sustain_id]

        row = self._db.execute(
            "SELECT template_id FROM sustains WHERE id = ?",
            (sustain_id,),
        ).fetchone()
        if row is None:
            return None

        try:
            spec = self._load_spec(row["template_id"])
        except ValueError:
            return None

        self._compile_spec_invariants(spec)
        self._specs[sustain_id] = spec
        return spec

    # ── Public helpers (used by devui routes) ──────────────────────────────────

    def get_spec(self, sustain_id: str) -> dict | None:
        """Public accessor for the sustain spec (load from cache or disk)."""
        return self._get_spec(sustain_id)

    def get_operative_statuses(self, sustain_id: str) -> list[dict]:
        """
        Return a status entry for each operative declared in this sustain's spec,
        excluding any explicitly disabled via set_operative_enabled(). Names are
        derived from the spec dict; confidence and task are None until the
        runtime tracks them in a future sprint.
        """
        spec = self._get_spec(sustain_id)
        if spec is None:
            return []
        operatives_cfg = spec.get("operatives", {})
        names = list(operatives_cfg.keys()) if isinstance(operatives_cfg, dict) else list(operatives_cfg)

        disabled: set[str] = set()
        try:
            rows = self._db.execute(
                "SELECT operative_id FROM operative_overrides WHERE sustain_id = ? AND enabled = 0",
                (sustain_id,),
            ).fetchall()
            disabled = {r["operative_id"] for r in rows}
        except Exception as exc:  # pragma: no cover - defensive
            logger.debug("get_operative_statuses(%s) override lookup failed: %s", sustain_id, exc)

        return [
            {
                "id": f"op-{name}",
                "name": name.capitalize(),
                "role": _OPERATIVE_ROLES.get(name, "operative"),
                "status": "active",
                "confidence": None,
                "pawa_session": 0,
                "task": None,
            }
            for name in names
            if name not in disabled
        ]

    # ── Advisory idle loop (Slice 10) ────────────────────────────────────────
    #
    # CORE PRINCIPLE: operatives ADVISE, the human DECIDES. Nothing below can
    # mutate a sustain's own state — evaluate_operatives() only reads state
    # and writes rows to operative_suggestions (metadata about advice, not
    # part of any sustain's fold, exactly like ingest_messages/sustain_
    # composition are metadata rather than fold-participants). The ONLY path
    # that can change real state is accept_suggestion(), and it changes
    # state by calling the real, unmodified execute_operator() — same S2
    # gate, same S3 fold-append, same Slice 7 parent-binding check. There is
    # no other write path from a suggestion to live state.

    def _children_staleness(self, sustain_id: str) -> list[dict]:
        """
        For each child linked under sustain_id, how many days since its most
        recent event (genesis included — "no activity since creation" is a
        legitimate honest reading). Feeds rule_child_stale(); pure DB reads,
        no mutation. [] for a sustain with no linked children.
        """
        out: list[dict] = []
        for child in self.list_children(sustain_id):
            events = self.get_events(child["child_sustain_id"], limit=1)
            days = None
            if events:
                try:
                    ts = datetime.fromisoformat(events[0]["timestamp"])
                    days = (datetime.utcnow() - ts.replace(tzinfo=None)).total_seconds() / 86400.0
                except (ValueError, TypeError):
                    days = None
            out.append({
                "sustain_id": child["child_sustain_id"],
                "slot": child.get("slot"),
                "member": child.get("member"),
                "days_since_last_event": days,
            })
        return out

    def _egress_last_sent_days(self, sustain_id: str) -> float | None:
        """
        Days since this sustain's most recently 'sent' egress entry, or
        None if it has never sent one. Feeds rule_summary_not_exported();
        pure DB read, no mutation — never counts 'prepared'/'confirmed'/
        'failed' entries, only genuinely 'sent' ones, so a never-confirmed
        or failed preparation doesn't count as "already handled."
        """
        row = self._db.execute(
            "SELECT sent_at FROM egress_outbox WHERE sustain_id = ? AND status = 'sent' "
            "ORDER BY sent_at DESC LIMIT 1",
            (sustain_id,),
        ).fetchone()
        if row is None or row["sent_at"] is None:
            return None
        try:
            ts = datetime.fromisoformat(row["sent_at"])
            return (datetime.utcnow() - ts.replace(tzinfo=None)).total_seconds() / 86400.0
        except (ValueError, TypeError):
            return None

    def evaluate_operatives(self, sustain_id: str) -> list[dict]:
        """
        The idle loop's triggered pass (Slice 10): run every rule bound to
        every operative this sustain actually declares (and hasn't disabled),
        against CURRENT folded state — no mutation of the sustain itself.

        Lifecycle per pass:
          - compute the current set of fired suggestions (with dedupe_keys)
          - any EXISTING 'pending' row whose dedupe_key is no longer firing
            -> 'expired' (the condition resolved itself, or was resolved by
            something other than accepting this exact suggestion)
          - a fired dedupe_key already 'dismissed' or 'accepted' -> skip,
            never resurfaces for that exact bucketed condition
          - a fired dedupe_key already 'pending' -> refresh its title/reason/
            proposed_params to the current values (same identity, freshest
            description) rather than duplicate
          - a fired dedupe_key with no row at all -> insert new 'pending'

        Returns the current list of pending suggestions for this sustain
        (same shape get_suggestions() returns).
        """
        spec = self._get_spec(sustain_id)
        if spec is None:
            raise ValueError(f"Sustain '{sustain_id}' not found.")

        state = self.get_state(sustain_id)
        statuses = self.get_operative_statuses(sustain_id)
        active_operative_ids = {s["name"].lower() for s in statuses}

        extra: dict = {}
        if "attache" in active_operative_ids:
            extra["children_meta"] = self._children_staleness(sustain_id)
        if "mentor" in active_operative_ids:
            extra["last_sent_days"] = self._egress_last_sent_days(sustain_id)

        fired: list[Suggestion] = []
        for operative_id in active_operative_ids:
            fired.extend(evaluate_operative(operative_id, state, extra))

        fired_by_key = {s.dedupe_key: s for s in fired}
        now = datetime.utcnow().isoformat()

        existing_rows = self._db.execute(
            "SELECT id, dedupe_key, status FROM operative_suggestions WHERE sustain_id = ?",
            (sustain_id,),
        ).fetchall()
        existing_by_key = {r["dedupe_key"]: dict(r) for r in existing_rows}

        for key, row in existing_by_key.items():
            if row["status"] == "pending" and key not in fired_by_key:
                self._db.execute(
                    "UPDATE operative_suggestions SET status = 'expired', updated_at = ?, resolved_at = ? WHERE id = ?",
                    (now, now, row["id"]),
                )

        for key, sug in fired_by_key.items():
            existing = existing_by_key.get(key)
            if existing is None:
                self._db.execute(
                    "INSERT INTO operative_suggestions "
                    "(id, sustain_id, operative_id, rule_id, title, reason, severity, dedupe_key, "
                    " proposed_operator, proposed_params_json, status, created_at, updated_at) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
                    (
                        str(uuid.uuid4()), sustain_id, sug.operative_id, sug.rule_id, sug.title, sug.reason,
                        sug.severity, sug.dedupe_key, sug.proposed_operator,
                        json.dumps(sug.proposed_params) if sug.proposed_params else None,
                        now, now,
                    ),
                )
            elif existing["status"] == "pending":
                self._db.execute(
                    "UPDATE operative_suggestions SET title = ?, reason = ?, severity = ?, "
                    "proposed_operator = ?, proposed_params_json = ?, updated_at = ? WHERE id = ?",
                    (
                        sug.title, sug.reason, sug.severity, sug.proposed_operator,
                        json.dumps(sug.proposed_params) if sug.proposed_params else None,
                        now, existing["id"],
                    ),
                )
            # 'dismissed' / 'accepted' existing rows: leave untouched, never resurface.

        self._db.commit()
        return self.get_suggestions(sustain_id, status="pending")

    def get_suggestions(self, sustain_id: str, status: str | None = "pending") -> list[dict]:
        """
        Read-only. status=None returns every suggestion ever recorded for
        this sustain (any lifecycle state); status="pending" (default)
        returns only what's currently actionable. Never triggers evaluation
        — this is a plain SELECT, safe to call on every /devui/state tick.
        """
        if status is not None:
            rows = self._db.execute(
                "SELECT * FROM operative_suggestions WHERE sustain_id = ? AND status = ? ORDER BY created_at DESC",
                (sustain_id, status),
            ).fetchall()
        else:
            rows = self._db.execute(
                "SELECT * FROM operative_suggestions WHERE sustain_id = ? ORDER BY created_at DESC",
                (sustain_id,),
            ).fetchall()
        out = []
        for r in rows:
            d = dict(r)
            d["proposed_params"] = json.loads(d.pop("proposed_params_json")) if d.get("proposed_params_json") else {}
            out.append(d)
        return out

    async def accept_suggestion(self, suggestion_id: str) -> dict:
        """
        The ONLY path from a suggestion to real state change, and it is
        exactly the real path: execute_operator() with the suggestion's
        frozen proposed_operator/proposed_params. Same S2 gate, same S3
        fold-append, same Slice 7 parent-binding check as any other call —
        no bypass exists.

        A gate refusal is an honest, expected outcome, not an error: the
        suggestion is left 'pending' (state may have drifted since it was
        generated — the human can dismiss it, or fix the underlying
        condition and try again) and the refusal reason is returned as-is.
        Only a genuine success marks the suggestion 'accepted'.

        Raises ValueError if the suggestion doesn't exist, is no longer
        pending, or has no proposed_operator (purely informational
        suggestions can only be dismissed, never accepted).
        """
        row = self._db.execute(
            "SELECT * FROM operative_suggestions WHERE id = ?", (suggestion_id,),
        ).fetchone()
        if row is None:
            raise ValueError(f"Suggestion '{suggestion_id}' not found.")
        row = dict(row)
        if row["status"] != "pending":
            raise ValueError(f"Suggestion '{suggestion_id}' is '{row['status']}', not pending.")
        if not row["proposed_operator"]:
            raise ValueError(f"Suggestion '{suggestion_id}' is informational only — nothing to accept.")

        params = json.loads(row["proposed_params_json"]) if row["proposed_params_json"] else {}
        result = await self.execute_operator(
            row["sustain_id"], row["proposed_operator"], params, operative_id=row["operative_id"],
        )

        now = datetime.utcnow().isoformat()
        if result.succeeded:
            self._db.execute(
                "UPDATE operative_suggestions SET status = 'accepted', updated_at = ?, resolved_at = ? WHERE id = ?",
                (now, now, suggestion_id),
            )
            self._db.commit()
        return {"suggestion": row, "result": result}

    def dismiss_suggestion(self, suggestion_id: str) -> bool:
        """
        Records the dismissal — this exact bucketed dedupe_key will not
        resurface on a future evaluate_operatives() pass unless the
        underlying condition changes enough to bucket differently (see each
        rule's own bucketing choice in advisory.py). Returns False if the
        suggestion doesn't exist or isn't pending.
        """
        now = datetime.utcnow().isoformat()
        cur = self._db.execute(
            "UPDATE operative_suggestions SET status = 'dismissed', updated_at = ?, resolved_at = ? "
            "WHERE id = ? AND status = 'pending'",
            (now, now, suggestion_id),
        )
        self._db.commit()
        return cur.rowcount > 0

    # ── Egress (Slice 11) ────────────────────────────────────────────────────
    #
    # HARD SAFETY BOUNDARY: nothing below can move money, send a payment, or
    # execute a trade. _send_egress() has exactly one implemented target,
    # "local_file" — a JSON file written to _EXPORTS_DIR. There is no
    # payment rail, no third-party messaging API, and no code path from any
    # egress operator to one. Every egress action is prepared, then requires
    # an explicit confirm_egress() call — a human action — before anything
    # leaves the system. Nothing here is ever called automatically: not from
    # execute_operator (which only ever calls _queue_egress, never
    # confirm/_send), not from evaluate_operatives (advisory suggestions can
    # only ever be *accepted* into a 'prepared' row via accept_suggestion's
    # existing execute_operator() call, never auto-confirmed), and not from
    # any scheduler (none exists).

    def _queue_egress(self, sustain_id: str, item: dict) -> dict:
        """
        Persist one EgressQueue item as a 'prepared' outbox row. Idempotent
        on (sustain_id, idempotency_key) — INSERT OR IGNORE, same pattern
        Slice 5's ingest dedup established. Returns the row that now exists
        for this key, whether it was just created or already there —
        preparing the same content twice is a no-op, not a duplicate.
        """
        now = datetime.utcnow().isoformat()
        # updated_at is seeded to the same value as prepared_at on insert.
        self._db.execute(
            "INSERT OR IGNORE INTO egress_outbox "
            "(id, sustain_id, kind, target, payload_json, idempotency_key, status, prepared_at, updated_at) "
            "VALUES (?, ?, ?, ?, ?, ?, 'prepared', ?, ?)",
            (
                str(uuid.uuid4()), sustain_id, item["kind"], item["target"],
                json.dumps(item["payload"]), item["idempotency_key"], now, now,
            ),
        )
        self._db.commit()
        row = self._db.execute(
            "SELECT * FROM egress_outbox WHERE sustain_id = ? AND idempotency_key = ?",
            (sustain_id, item["idempotency_key"]),
        ).fetchone()
        return dict(row)

    def list_egress(self, sustain_id: str, status: str | None = None) -> list[dict]:
        """Read-only. status=None returns full history, newest first."""
        if status is not None:
            rows = self._db.execute(
                "SELECT * FROM egress_outbox WHERE sustain_id = ? AND status = ? ORDER BY prepared_at DESC",
                (sustain_id, status),
            ).fetchall()
        else:
            rows = self._db.execute(
                "SELECT * FROM egress_outbox WHERE sustain_id = ? ORDER BY prepared_at DESC",
                (sustain_id,),
            ).fetchall()
        return [dict(r) for r in rows]

    def _send_egress(self, row: dict) -> dict:
        """
        The ONLY place an outbound effect actually happens. Real file I/O,
        real exceptions propagate to the caller (confirm_egress) to be
        recorded honestly as a failure — nothing here swallows an error.

        target="local_file" is the only implemented target. The exports
        subdirectory is named after the sustain_id (always a safe UUID);
        the FILENAME is the raw idempotency_key (which may embed a raw,
        unsanitized human-supplied label) — deliberately not sanitized, so
        a label containing characters illegal in a filename produces a
        genuine OSError here rather than a silently mangled export.
        """
        if row["target"] != "local_file":
            raise ValueError(f"Unsupported egress target '{row['target']}'.")
        payload = json.loads(row["payload_json"])
        sustain_dir = _EXPORTS_DIR / row["sustain_id"]
        sustain_dir.mkdir(parents=True, exist_ok=True)
        file_path = sustain_dir / f"{row['idempotency_key']}.json"
        file_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        return {"file_path": str(file_path)}

    async def confirm_egress(self, outbox_id: str) -> dict:
        """
        The ONLY path from 'prepared' to an actual outbound effect — and it
        can only be called explicitly (never automatically). Also serves as
        retry: a 'failed' row can be confirmed again (attempts increments
        each time). Confirming an already-'sent' row is an idempotent
        no-op — returns the existing result WITHOUT re-sending, so
        double-clicking confirm (or a duplicate request) can never fire the
        effect twice.

        Passes through a real 'confirmed' status before attempting the
        send, so confirmed_at is always set once a row reaches 'sent' or
        'failed' — proof the explicit-confirm step genuinely happened, not
        just an internal implementation detail.

        Raises ValueError if the outbox entry doesn't exist or is in a
        status that can't be confirmed (there is no such status today
        besides 'sent', which is handled as an idempotent no-op above, but
        this guards a future added status too).
        """
        row = self._db.execute("SELECT * FROM egress_outbox WHERE id = ?", (outbox_id,)).fetchone()
        if row is None:
            raise ValueError(f"Egress outbox entry '{outbox_id}' not found.")
        row = dict(row)
        now = datetime.utcnow().isoformat()

        if row["status"] == "sent":
            return {"outbox": row, "already_sent": True}
        if row["status"] not in ("prepared", "failed"):
            raise ValueError(f"Egress outbox entry '{outbox_id}' is '{row['status']}', not confirmable.")

        self._db.execute(
            "UPDATE egress_outbox SET status = 'confirmed', confirmed_at = ?, updated_at = ?, attempts = attempts + 1 WHERE id = ?",
            (now, now, outbox_id),
        )
        self._db.commit()

        try:
            result = self._send_egress(row)
        except Exception as exc:
            self._db.execute(
                "UPDATE egress_outbox SET status = 'failed', failed_at = ?, updated_at = ?, failure_reason = ? WHERE id = ?",
                (now, now, f"{type(exc).__name__}: {exc}", outbox_id),
            )
            self._db.commit()
            updated = dict(self._db.execute("SELECT * FROM egress_outbox WHERE id = ?", (outbox_id,)).fetchone())
            logger.info("[SustainEngine] egress '%s' confirm FAILED: %s", outbox_id, exc)
            return {"outbox": updated, "already_sent": False, "error": str(exc)}

        self._db.execute(
            "UPDATE egress_outbox SET status = 'sent', sent_at = ?, updated_at = ?, result_json = ? WHERE id = ?",
            (now, now, json.dumps(result), outbox_id),
        )
        self._db.commit()
        updated = dict(self._db.execute("SELECT * FROM egress_outbox WHERE id = ?", (outbox_id,)).fetchone())
        return {"outbox": updated, "already_sent": False}

    def cancel_egress(self, outbox_id: str) -> bool:
        """
        Marks a 'prepared' or 'failed' row 'cancelled' — never sent, kept
        for the audit trail rather than deleted. Returns False if the entry
        doesn't exist or is already 'sent'/'cancelled'.
        """
        now = datetime.utcnow().isoformat()
        cur = self._db.execute(
            "UPDATE egress_outbox SET status = 'cancelled', updated_at = ? "
            "WHERE id = ? AND status IN ('prepared', 'failed')",
            (now, outbox_id),
        )
        self._db.commit()
        return cur.rowcount > 0

    def purge_test_egress_entries(self, sustain_id: str, outbox_ids: list[str]) -> dict:
        """
        Operational cleanup, distinct from the normal user-facing egress
        lifecycle (prepared/confirmed/sent/failed/cancelled). cancel_egress()
        deliberately never deletes -- it marks a 'prepared'/'failed' row
        'cancelled' and keeps it for a real user's audit trail. This method
        exists for a different, narrower case: rows that were never real
        household activity in the first place (dev-testing acceptance
        checks run directly against production during an earlier slice),
        which don't belong in that audit trail at all -- including a
        genuinely 'sent' row, which cancel_egress() can't touch by design
        since a real send can't be honestly un-sent.

        Deletes ONLY the exact ids passed in, ONLY if each one belongs to
        the given sustain_id (a safety check against a stale/wrong id list,
        not a filter on status -- this is an explicit, targeted purge, not
        a status-based bulk operation). Idempotent: an id that's already
        gone or never matched is simply not counted.

        Returns {"sustain_id", "requested": [...], "deleted": [...],
        "not_found": [...]} -- report-not-silent-success, same discipline
        as reassign_sustain_owner()/migrate_to_event_sourcing().
        """
        deleted: list[str] = []
        not_found: list[str] = []
        for outbox_id in outbox_ids:
            row = self._db.execute(
                "SELECT id FROM egress_outbox WHERE id = ? AND sustain_id = ?",
                (outbox_id, sustain_id),
            ).fetchone()
            if row is None:
                not_found.append(outbox_id)
                continue
            self._db.execute("DELETE FROM egress_outbox WHERE id = ?", (outbox_id,))
            deleted.append(outbox_id)
        self._db.commit()
        return {"sustain_id": sustain_id, "requested": list(outbox_ids), "deleted": deleted, "not_found": not_found}

    def set_operative_enabled(self, sustain_id: str, operative_id: str, enabled: bool) -> bool:
        """
        Enable/disable an operative for a sustain so the Monitor's operative
        cards (which read get_operative_statuses()) reflect it. Used by
        POST /seed/operative. Returns False if the sustain has no engine spec
        (e.g. a free-text seed id that was never instantiated).
        """
        if self._get_spec(sustain_id) is None:
            return False
        self._db.execute(
            "INSERT INTO operative_overrides (sustain_id, operative_id, enabled) "
            "VALUES (?, ?, ?) "
            "ON CONFLICT(sustain_id, operative_id) DO UPDATE SET enabled = excluded.enabled",
            (sustain_id, operative_id, 1 if enabled else 0),
        )
        self._db.commit()
        return True

    # ── Pawa meter (§4L) ─────────────────────────────────────────────────────

    def _record_pawa_meter(
        self, sustain_id: str, operator_name: str, principal: str,
        compute: float, storage: float, pawa: float, elapsed_ms: float,
    ) -> dict:
        """Write one real metering row. Called only from execute_operator,
        only for a run that actually succeeded — see the call site's own
        comment for why a gate refusal is honestly metered as nothing."""
        row = {
            "id": str(uuid.uuid4()),
            "sustain_id": sustain_id,
            "operator_name": operator_name,
            "principal": principal,
            "compute": compute,
            "storage": storage,
            "pawa": pawa,
            "elapsed_ms": elapsed_ms,
            "timestamp": datetime.utcnow().isoformat(),
        }
        self._db.execute(
            "INSERT INTO pawa_meter_log "
            "(id, sustain_id, operator_name, principal, compute, storage, pawa, elapsed_ms, timestamp) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (row["id"], sustain_id, operator_name, principal, compute, storage, pawa, elapsed_ms, row["timestamp"]),
        )
        self._db.commit()
        return row

    def get_last_pawa_meter(self, sustain_id: str, operator_name: str) -> dict | None:
        """The single most recent real metering row for this exact
        sustain+operator pair — used to show "this run cost you N pawa"
        immediately after a Console execution. None if never run."""
        row = self._db.execute(
            "SELECT compute, storage, pawa, elapsed_ms, timestamp FROM pawa_meter_log "
            "WHERE sustain_id = ? AND operator_name = ? ORDER BY timestamp DESC LIMIT 1",
            (sustain_id, operator_name),
        ).fetchone()
        if row is None:
            return None
        return dict(row)

    def get_operator_pawa_stats(self, operator_name: str) -> dict | None:
        """Real average/total pawa for one operator across every metered
        run, on every sustain. None (not a zero-filled stub) when the
        operator has never actually run — an honest 'not yet measured',
        never a fabricated number."""
        row = self._db.execute(
            "SELECT COUNT(*) AS n, AVG(compute) AS avg_compute, AVG(storage) AS avg_storage, "
            "AVG(pawa) AS avg_pawa, SUM(pawa) AS total_pawa "
            "FROM pawa_meter_log WHERE operator_name = ?",
            (operator_name,),
        ).fetchone()
        if row is None or row["n"] == 0:
            return None
        return {
            "operator_name": operator_name,
            "run_count": row["n"],
            "avg_compute": row["avg_compute"],
            "avg_storage": row["avg_storage"],
            "avg_pawa": row["avg_pawa"],
            "total_pawa": row["total_pawa"],
        }

    def get_all_operator_pawa_stats(self) -> dict[str, dict]:
        """Same as get_operator_pawa_stats but for every operator that has
        ever run, in one query — what the DEFINE/Console operator LISTS
        need (many operators shown at once), rather than N round-trips."""
        rows = self._db.execute(
            "SELECT operator_name, COUNT(*) AS n, AVG(compute) AS avg_compute, "
            "AVG(storage) AS avg_storage, AVG(pawa) AS avg_pawa, SUM(pawa) AS total_pawa "
            "FROM pawa_meter_log GROUP BY operator_name"
        ).fetchall()
        return {
            r["operator_name"]: {
                "operator_name": r["operator_name"],
                "run_count": r["n"],
                "avg_compute": r["avg_compute"],
                "avg_storage": r["avg_storage"],
                "avg_pawa": r["avg_pawa"],
                "total_pawa": r["total_pawa"],
            }
            for r in rows
        }

    def get_sustain_pawa_total(self, sustain_id: str) -> dict:
        """Real total metered pawa for one sustain across every operator
        that has run on it. run_count=0/totals=0 is the honest answer for
        a sustain with no metered activity yet -- not omitted, not faked."""
        row = self._db.execute(
            "SELECT COUNT(*) AS n, COALESCE(SUM(compute), 0) AS total_compute, "
            "COALESCE(SUM(storage), 0) AS total_storage, COALESCE(SUM(pawa), 0) AS total_pawa "
            "FROM pawa_meter_log WHERE sustain_id = ?",
            (sustain_id,),
        ).fetchone()
        return {
            "sustain_id": sustain_id,
            "run_count": row["n"],
            "total_compute": row["total_compute"],
            "total_storage": row["total_storage"],
            "total_pawa": row["total_pawa"],
        }

    def get_principal_pawa_total(self, principal: str) -> dict:
        """Real total metered pawa for one principal (the acting user)
        across every sustain they've run an operator on."""
        row = self._db.execute(
            "SELECT COUNT(*) AS n, COALESCE(SUM(compute), 0) AS total_compute, "
            "COALESCE(SUM(storage), 0) AS total_storage, COALESCE(SUM(pawa), 0) AS total_pawa "
            "FROM pawa_meter_log WHERE principal = ?",
            (principal,),
        ).fetchone()
        return {
            "principal": principal,
            "run_count": row["n"],
            "total_compute": row["total_compute"],
            "total_storage": row["total_storage"],
            "total_pawa": row["total_pawa"],
        }

    def evaluate_constraints(self, sustain_id: str) -> list[dict]:
        """
        Evaluate each invariant expression from the spec against the live state.
        Returns [{"expr": ..., "status": "ok"|"fail", "value": None}].

        Uses _state_with_aggregates rather than raw state, so a parent's
        aggregate-referencing invariants (household_liquid_total, etc.) show
        a real pass/fail here — display only, never gated (see
        _check_enforcement_gate's "enforced" skip). Sustains with no
        declared aggregates get their normal state back unchanged.
        """
        spec = self._get_spec(sustain_id)
        if spec is None:
            return []
        try:
            state_dict = self._state_with_aggregates(sustain_id)
        except ValueError:
            return []
        state = StateAccessor(state_dict)
        from sustena.core.constraints import ConstraintEngine
        constraint_engine = ConstraintEngine()
        results = []
        for inv in spec.get("invariants", []):
            expr = inv.get("expression", "")
            ok_flag, _ = constraint_engine.evaluate(expr, state)
            results.append({"expr": expr, "status": "ok" if ok_flag else "fail", "value": None})
        return results

    # ── Definitions — user-created sustain templates (Slice 6) ─────────────────
    #
    # A "definition" is a template a person builds through the UI instead of
    # a built-in sustena/sustains/*.json file. It persists in sustain_templates
    # and is loaded by _load_spec() exactly like homestead/habitat — every
    # method below either builds/validates a spec dict in the same shape the
    # built-in JSON files use, or reads/writes sustain_templates directly.
    # instantiate()/execute_operator()/the enforcement gate/simulate() are
    # completely unmodified: they only ever call _load_spec(template_id) and
    # have no idea whether a spec came from disk or the DB.

    _DIMENSION_DEFAULTS: dict[str, Any] = {"number": 0.0, "string": "", "boolean": False}

    def _build_spec_dict(
        self,
        template_id: str,
        display_name: str,
        description: str,
        dimensions: list[dict],
        invariants: list[dict],
        operator_names: list[str],
    ) -> dict:
        """
        Turn UI-shaped input into a spec dict in the exact shape
        sustena/sustains/*.json files use. Raises ValueError on the first
        invalid dimension/invariant/operator name — never silently drops one.

        Deliberately flat, scalar-only dimensions (number/string/boolean) —
        no nested objects/arrays. A real, disclosed scope cut for this
        walking skeleton: enough to declare a structurally-unlike sustain
        (M3) without a recursive schema-builder UI.
        """
        state_schema: dict = {}
        default_state: dict = {}
        for dim in dimensions:
            name = (dim.get("name") or "").strip()
            dtype = dim.get("type")
            if not name or dtype not in self._DIMENSION_DEFAULTS:
                raise ValueError(
                    f"Invalid dimension {dim!r} — name is required and type must be "
                    f"one of {sorted(self._DIMENSION_DEFAULTS)}."
                )
            if name in state_schema:
                raise ValueError(f"Duplicate dimension name '{name}'.")
            schema_node: dict = {"type": dtype, "description": dim.get("description", "")}
            if dtype == "number" and dim.get("minimum") is not None:
                schema_node["minimum"] = dim["minimum"]
            state_schema[name] = schema_node
            default_value = dim.get("default_value")
            default_state[name] = default_value if default_value is not None else self._DIMENSION_DEFAULTS[dtype]

        resolved_operators: list[dict] = []
        for op_name in operator_names:
            meta = OPERATOR_REGISTRY.get(op_name)
            if meta is None:
                raise ValueError(f"Unknown operator '{op_name}' — not in OPERATOR_REGISTRY.")
            try:
                sig_params = [
                    p for p in inspect.signature(meta.fn).parameters
                    if p not in ("ctx", "self")
                ]
            except (TypeError, ValueError):  # pragma: no cover - defensive, no known operator hits this
                sig_params = []
            resolved_operators.append({
                "name": meta.name,
                "description": meta.description,
                "params": sig_params,
                "pawa_cost": meta.pawa_cost,
            })

        clean_invariants: list[dict] = []
        for inv in invariants:
            inv_id = (inv.get("id") or "").strip()
            expr = (inv.get("expression") or "").strip()
            if not inv_id or not expr:
                raise ValueError(f"Invalid invariant {inv!r} — both id and expression are required.")
            authority = inv.get("authority") or "advisory"
            if authority not in ("binding", "advisory"):
                raise ValueError(f"Invariant '{inv_id}' has invalid authority '{authority}' — must be 'binding' or 'advisory'.")
            clean_invariants.append({
                "id": inv_id, "expression": expr, "description": inv.get("description", ""), "authority": authority,
            })

        return {
            "id": template_id,
            "version": "1.0.0",
            "display_name": display_name or template_id,
            "description": description or "",
            "state_schema": state_schema,
            "operators": resolved_operators,
            "operatives": {},
            "invariants": clean_invariants,
            "enforcement": {"enabled": True},
            "ui_schema": {"sustain_home": {"widget": "sustain_home"}},
            "access_policy": {"owner_ids": "{{owner_ids}}"},
            "default_state": default_state,
            "parameters": [
                {
                    "name": "owner_ids", "type": "array", "items": {"type": "string"},
                    "description": "List of user IDs who have owner access to this sustain.",
                    "required": True,
                },
            ],
            "_notes": {"origin": "User-created via the Create/Definition flow (Slice 6)."},
        }

    def _validate_definition_spec(self, spec: dict) -> None:
        """
        Compile-validate every invariant against the candidate state_schema,
        raising ValueError on the FIRST bad one. Distinct from
        _compile_spec_invariants' non-fatal warn-and-skip used for built-in
        specs at instantiate() time — someone actively defining their own
        sustain needs to know immediately if an invariant doesn't parse or
        references an undeclared dimension, not have it silently dropped
        from enforcement.
        """
        state_schema = spec.get("state_schema", {})
        for inv in spec.get("invariants", []):
            _, errors = compile_invariant(inv.get("expression", ""), state_schema)
            if errors:
                raise ValueError(
                    f"Invariant '{inv.get('id')}' ({inv.get('expression')}) is invalid: {'; '.join(errors)}"
                )

    def create_definition(
        self,
        owner_user_id: str,
        display_name: str,
        description: str,
        dimensions: list[dict],
        invariants: list[dict],
        operator_names: list[str],
    ) -> dict:
        """
        Build and persist a brand-new sustain template from UI-shaped input.
        Enforcement is always on (enforcement.enabled=True) for a
        user-created definition — there's no reason to offer an opt-out that
        would let a person accidentally build a sustain with no gate.

        Attaching an operator means selecting an EXISTING registered
        implementation (e.g. edit.state_patch, budget.allocate) — this slice
        does not let a user author new operator code, a materially larger
        feature and out of scope here.

        Raises ValueError on the first invalid dimension/invariant/operator.
        Returns {"template_id", "spec", "version"}.
        """
        template_id = str(uuid.uuid4())
        spec = self._build_spec_dict(
            template_id, display_name, description, dimensions, invariants, operator_names,
        )
        self._validate_definition_spec(spec)

        now = datetime.utcnow().isoformat()
        self._db.execute(
            "INSERT INTO sustain_templates (id, owner_user_id, spec_json, version, created_at, updated_at) "
            "VALUES (?, ?, ?, 1, ?, ?)",
            (template_id, owner_user_id, json.dumps(spec), now, now),
        )
        self._db.commit()
        logger.info(
            "[SustainEngine] created definition template_id=%s owner=%s dims=%d invariants=%d operators=%d",
            template_id, owner_user_id, len(dimensions), len(invariants), len(operator_names),
        )
        return {"template_id": template_id, "spec": spec, "version": 1}

    def get_definition(self, template_id: str) -> dict | None:
        """A single user-created definition's full record, or None if not found."""
        row = self._db.execute(
            "SELECT id, owner_user_id, spec_json, version, created_at, updated_at "
            "FROM sustain_templates WHERE id = ?",
            (template_id,),
        ).fetchone()
        if row is None:
            return None
        return {
            "template_id": row["id"],
            "owner_user_id": row["owner_user_id"],
            "spec": json.loads(row["spec_json"]),
            "version": row["version"],
            "created_at": row["created_at"],
            "updated_at": row["updated_at"],
        }

    def list_definitions(self, owner_user_id: str | None = None) -> list[dict]:
        """Summary list of user-created definitions, optionally scoped to one owner."""
        if owner_user_id:
            rows = self._db.execute(
                "SELECT id, owner_user_id, spec_json, version, created_at, updated_at "
                "FROM sustain_templates WHERE owner_user_id = ? ORDER BY created_at DESC",
                (owner_user_id,),
            ).fetchall()
        else:
            rows = self._db.execute(
                "SELECT id, owner_user_id, spec_json, version, created_at, updated_at "
                "FROM sustain_templates ORDER BY created_at DESC"
            ).fetchall()
        out: list[dict] = []
        for r in rows:
            spec = json.loads(r["spec_json"])
            out.append({
                "template_id": r["id"],
                "owner_user_id": r["owner_user_id"],
                "display_name": spec.get("display_name", r["id"]),
                "description": spec.get("description", ""),
                "dimension_count": len(spec.get("state_schema", {})),
                "invariant_count": len(spec.get("invariants", [])),
                "operator_count": len(spec.get("operators", [])),
                "version": r["version"],
                "created_at": r["created_at"],
                "updated_at": r["updated_at"],
            })
        return out

    def check_definition_edit_safety(self, template_id: str, candidate_spec: dict) -> tuple[bool, list[dict]]:
        """
        The migration predicate: before an edit to a definition is persisted,
        every LIVE instance of that template must still satisfy every
        invariant in the CANDIDATE spec, evaluated against that instance's
        actual current state. If not, the edit must be refused — silently
        applying it would strand a real instance outside its own declared
        viable region with no warning.

        Returns (safe, violations). violations is empty when safe=True.
        Each violation is {"sustain_id", "invariant_id", "expression",
        "reason"} — enough for an honest "why am I seeing this" message. A
        candidate invariant that doesn't even COMPILE against the candidate
        schema is reported as a violation against every live instance
        (there's no state to meaningfully evaluate it against), not
        silently skipped.
        """
        state_schema = candidate_spec.get("state_schema", {})
        compiled: list[dict] = []
        for inv in candidate_spec.get("invariants", []):
            node, errors = compile_invariant(inv.get("expression", ""), state_schema)
            compiled.append({
                "id": inv.get("id"), "expr": inv.get("expression"),
                "node": node, "compile_errors": errors,
            })

        instance_ids = [
            r["id"] for r in self._db.execute(
                "SELECT id FROM sustains WHERE template_id = ?", (template_id,)
            ).fetchall()
        ]

        violations: list[dict] = []
        for sustain_id in instance_ids:
            try:
                state_dict = self._load_state_dict(sustain_id)
            except ValueError:
                continue  # no state row for this instance — nothing to check
            state = StateAccessor(state_dict)
            for inv in compiled:
                if inv["compile_errors"]:
                    violations.append({
                        "sustain_id": sustain_id, "invariant_id": inv["id"], "expression": inv["expr"],
                        "reason": f"does not compile against the new schema: {'; '.join(inv['compile_errors'])}",
                    })
                    continue
                ok, reason = evaluate_predicate(inv["node"], state, {})
                if not ok:
                    violations.append({
                        "sustain_id": sustain_id, "invariant_id": inv["id"],
                        "expression": inv["expr"], "reason": reason,
                    })

        return (len(violations) == 0), violations

    def update_definition(self, template_id: str, owner_user_id: str, patch: dict) -> dict:
        """
        Edit a user-created definition safely. patch may include any of
        display_name, description, dimensions, invariants, operator_names
        (same shapes create_definition() accepts) — fields omitted from
        patch keep their current value, so a caller can send just the one
        thing they changed.

        Returns {"status": "ok", "spec", "version"} on success, or
        {"status": "refused", "reason", "blocked_by": [...]} if the
        migration predicate finds the edit would strand a live instance.
        Raises ValueError for a genuine input/ownership/not-found/compile
        error, matching instantiate()'s and create_definition()'s
        convention — those are programmer/input errors, not an expected
        everyday outcome the way a refusal is.
        """
        existing = self.get_definition(template_id)
        if existing is None:
            raise ValueError(f"Definition '{template_id}' not found.")
        if existing["owner_user_id"] != owner_user_id:
            raise ValueError(f"Definition '{template_id}' is not owned by this user.")

        current_spec = existing["spec"]
        dimensions = patch.get("dimensions")
        invariants = patch.get("invariants")
        operator_names = patch.get("operator_names")

        if dimensions is None:
            dimensions = [
                {
                    "name": name, "type": node.get("type"), "description": node.get("description", ""),
                    "default_value": current_spec.get("default_state", {}).get(name),
                    "minimum": node.get("minimum"),
                }
                for name, node in current_spec.get("state_schema", {}).items()
            ]
        if invariants is None:
            invariants = current_spec.get("invariants", [])
        if operator_names is None:
            operator_names = [op["name"] for op in current_spec.get("operators", [])]

        candidate_spec = self._build_spec_dict(
            template_id,
            patch.get("display_name", current_spec.get("display_name")),
            patch.get("description", current_spec.get("description")),
            dimensions, invariants, operator_names,
        )
        self._validate_definition_spec(candidate_spec)

        safe, violations = self.check_definition_edit_safety(template_id, candidate_spec)
        if not safe:
            stranded = {v["sustain_id"] for v in violations}
            return {
                "status": "refused",
                "reason": f"This edit would strand {len(stranded)} live sustain(s) outside their viable region.",
                "blocked_by": violations,
            }

        now = datetime.utcnow().isoformat()
        new_version = existing["version"] + 1
        self._db.execute(
            "UPDATE sustain_templates SET spec_json = ?, version = ?, updated_at = ? WHERE id = ?",
            (json.dumps(candidate_spec), new_version, now, template_id),
        )
        self._db.commit()

        # Invalidate the per-instance spec cache for every live instance of
        # this template so the new invariants/operators govern immediately —
        # execute_operator() reads self._specs[sustain_id], populated once at
        # instantiate()/first access and never otherwise refreshed. Without
        # this, an edit would silently not take effect until a process
        # restart, which is exactly the kind of quiet non-application "no
        # silent failure" exists to rule out.
        for row in self._db.execute("SELECT id FROM sustains WHERE template_id = ?", (template_id,)).fetchall():
            self._specs.pop(row["id"], None)

        logger.info("[SustainEngine] updated definition template_id=%s version=%d", template_id, new_version)
        return {"status": "ok", "spec": candidate_spec, "version": new_version}

    # ── Composition ⊕ and roll-up ρ (Slice 8) ───────────────────────────────────
    #
    # Generic machinery: nothing below knows the word "habitat" or "homestead".
    # A parent is any sustain with linked children in sustain_composition; a
    # child is any sustain so linked. VOS/Homestead is the first caller of
    # link_child()/provision_declared_children(), not a special case of it — a
    # structurally-unlike future parent (M3) uses the exact same methods.
    #
    # Authority model (confirmed with Bonnie, not guessed): the parent
    # OBSERVES its children, it never VETOES them. A child's own operator call
    # is governed only by the child's own gate — nothing here ever reaches
    # into a different sustain's invariants to block a transaction. The
    # parent's aggregate values and any invariant that references them are
    # purely for display/needs-attention (see _compile_spec_invariants'
    # "enforced" flag and _check_enforcement_gate's skip of it).

    def link_child(
        self, parent_sustain_id: str, child_sustain_id: str,
        slot: str | None = None, member: str | None = None,
    ) -> dict:
        """
        The ⊕ primitive: link a live child sustain under a live parent
        sustain. Both must already exist. Idempotency/uniqueness is enforced
        at the DB level (a child can only have one parent; a slot can only
        be filled once per parent) — raises ValueError with a clear reason
        rather than silently overwriting an existing link.
        """
        if parent_sustain_id == child_sustain_id:
            raise ValueError("a sustain cannot be linked as its own child.")
        for sid, role in ((parent_sustain_id, "parent"), (child_sustain_id, "child")):
            row = self._db.execute("SELECT id FROM sustains WHERE id = ?", (sid,)).fetchone()
            if row is None:
                raise ValueError(f"{role} sustain '{sid}' not found.")

        existing_parent = self.get_parent(child_sustain_id)
        if existing_parent is not None:
            raise ValueError(
                f"child sustain '{child_sustain_id}' is already linked to parent "
                f"'{existing_parent['parent_sustain_id']}'."
            )

        link_id = str(uuid.uuid4())
        now = datetime.utcnow().isoformat()
        try:
            self._db.execute(
                "INSERT INTO sustain_composition (id, parent_sustain_id, child_sustain_id, slot, member, linked_at) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                (link_id, parent_sustain_id, child_sustain_id, slot, member, now),
            )
            self._db.commit()
        except sqlite3.IntegrityError as exc:
            raise ValueError(f"could not link '{child_sustain_id}' under '{parent_sustain_id}': {exc}")

        logger.info(
            "[SustainEngine] linked child=%s under parent=%s slot=%s member=%s",
            child_sustain_id, parent_sustain_id, slot, member,
        )
        return {
            "id": link_id, "parent_sustain_id": parent_sustain_id, "child_sustain_id": child_sustain_id,
            "slot": slot, "member": member, "linked_at": now,
        }

    def list_children(self, parent_sustain_id: str) -> list[dict]:
        """Every child currently linked under a parent, in link order."""
        rows = self._db.execute(
            "SELECT id, parent_sustain_id, child_sustain_id, slot, member, linked_at "
            "FROM sustain_composition WHERE parent_sustain_id = ? ORDER BY linked_at ASC",
            (parent_sustain_id,),
        ).fetchall()
        return [dict(r) for r in rows]

    def get_parent(self, child_sustain_id: str) -> dict | None:
        """The link record for a child's parent, or None if it has none."""
        row = self._db.execute(
            "SELECT id, parent_sustain_id, child_sustain_id, slot, member, linked_at "
            "FROM sustain_composition WHERE child_sustain_id = ?",
            (child_sustain_id,),
        ).fetchone()
        return dict(row) if row else None

    def unlink_child(self, parent_sustain_id: str, child_sustain_id: str) -> bool:
        """
        Disaggregation (§IX of the Multiparty article): dissolve a
        parent/child link. The child keeps its own state — nothing about it
        is deleted or mutated, only the link record. Returns False if no
        such link existed.
        """
        cur = self._db.execute(
            "DELETE FROM sustain_composition WHERE parent_sustain_id = ? AND child_sustain_id = ?",
            (parent_sustain_id, child_sustain_id),
        )
        self._db.commit()
        return cur.rowcount > 0

    def provision_declared_children(self, parent_sustain_id: str, owner_user_id: str) -> list[dict]:
        """
        Reads the parent's spec["declared_children"] — a generic convention
        (NOT specific to homestead/habitat): a list of
        {slot, member, template, status} entries any parent-type spec can
        declare. For each declared slot not yet linked, instantiates a fresh
        (empty/default-state) instance of the declared template and links it.

        Idempotent: already-linked slots are skipped and returned as-is, so
        calling this twice (or on a sustain with no unlinked slots left) is
        safe and does not create duplicates.
        """
        spec = self._get_spec(parent_sustain_id)
        if spec is None:
            raise ValueError(f"Sustain '{parent_sustain_id}' not found.")

        declared = spec.get("declared_children", [])
        existing_by_slot = {c["slot"]: c for c in self.list_children(parent_sustain_id) if c.get("slot")}

        results: list[dict] = []
        for entry in declared:
            slot = entry.get("slot")
            if slot in existing_by_slot:
                results.append(existing_by_slot[slot])
                continue
            template_id = entry.get("template")
            member = entry.get("member", slot)
            if not template_id:
                raise ValueError(f"declared_children entry for slot '{slot}' has no template.")
            child_sustain_id = self.instantiate(
                template_id, owner_user_id, {"owner_ids": [owner_user_id], "name": member},
            )
            link = self.link_child(parent_sustain_id, child_sustain_id, slot=slot, member=member)
            results.append(link)

        logger.info(
            "[SustainEngine] provisioned %d declared children for parent=%s",
            len(results), parent_sustain_id,
        )
        return results

    _ROLLUP_OPS: dict[str, Any] = {
        "sum": sum,
        "count": len,
        "avg": lambda vs: (sum(vs) / len(vs)) if vs else 0.0,
        "min": lambda vs: min(vs) if vs else None,
        "max": lambda vs: max(vs) if vs else None,
    }

    def compute_rollup(self, parent_sustain_id: str) -> dict:
        """
        The ρ primitive: fold every linked child's CURRENT (already-folded,
        via get_state()) state into the parent's declared aggregates.
        Computed fresh on every call — never persisted anywhere, never
        written into the parent's own state. This IS the recompute path: a
        second call with no state changes reproduces the identical result,
        the same discipline as S3's rebuild_state().

        A child that can't be read (no state row — e.g. a stale/orphaned
        link) or whose declared child_path doesn't resolve to a number on
        that child is EXCLUDED from the sum and reported by name, not
        silently treated as contributing zero. The returned value is always
        the honest sum of what WAS readable, with the exclusions listed
        alongside it — never presented as if it covered everyone.

        Returns:
          {
            "children": [{"sustain_id", "slot", "member", "status": "ok"|"missing"}],
            "aggregates": {
              agg_id: {
                "op", "child_path", "value",
                "included": [{"sustain_id","slot","member","value"}],
                "excluded": [{"sustain_id","slot","member","reason"}],
              }
            },
          }
        """
        return self._hypothetical_rollup(parent_sustain_id)

    def _hypothetical_rollup(
        self, parent_sustain_id: str,
        override_child_id: str | None = None, override_state: dict | None = None,
    ) -> dict:
        """
        The shared core of ρ. compute_rollup() is the plain case (every
        child read fresh from disk). Slice 9's _check_parent_binding_gate()
        and simulate() are the override case: ONE child's contribution is
        override_state (a candidate, not-yet-committed state) instead of a
        disk read — every other child is read normally — to answer "what
        WOULD the roll-up be if this candidate state were already
        committed" without writing anything, anywhere.

        Household total = parent + children (2 Aug 2026 fix): the
        aggregate used to sum ONLY linked children, reading as "what your
        children collectively hold" rather than a genuine combined total —
        Bonnie's own framing was "household total = everything." The
        parent's OWN current state is always its real, persisted value
        here (never hypothetical — only a CHILD's candidate state is ever
        the "what if" this function's override mechanism models), so
        folding it in is safe and consistent across every caller:
        compute_rollup() (plain), _check_parent_binding_gate()'s before/
        after pair (the same real parent contribution appears in both,
        so the refuse-only-on-newly-breach delta logic is unaffected —
        only the child's own transition drives any before/after
        difference), and simulate()'s _parent_rollup_for (a simulated
        child's forked state combined with the parent's real current
        total, exactly what a live promotion would actually produce).
        """
        spec = self._get_spec(parent_sustain_id)
        if spec is None:
            raise ValueError(f"Sustain '{parent_sustain_id}' not found.")

        links = self.list_children(parent_sustain_id)
        child_states: dict[str, dict] = {}
        children_report: list[dict] = []
        for link in links:
            cid = link["child_sustain_id"]
            if cid == override_child_id:
                child_states[cid] = override_state
                children_report.append({**link, "status": "ok"})
                continue
            try:
                child_states[cid] = self._load_state_dict(cid)
                children_report.append({**link, "status": "ok"})
            except ValueError:
                children_report.append({
                    **link, "status": "missing",
                    "reason": "no state found for this child — the link may be stale or the child was removed.",
                })

        try:
            own_state = self._load_state_dict(parent_sustain_id)
        except ValueError:
            own_state = None

        aggregates = self._aggregate_from_child_states(spec, links, child_states, parent_sustain_id, own_state)
        return {"children": children_report, "aggregates": aggregates}

    @staticmethod
    def _resolve_child_path_value(state: dict, child_path: str) -> float | None:
        """
        Resolve child_path against one child's state, returning a single
        number, or None if the path doesn't resolve to something numeric.

        Supports two forms:
          - a plain single-numeric path (unchanged, the only form that
            existed before Phase 2 — e.g. "finances.liquid.balance").
          - a wildcard-SUM reducer over a dict of variable keys (Phase 2,
            nested-holons addendum §4D.7's "reducer/wildcard child_path" —
            e.g. "finances.pockets[*].allocated" sums .allocated across
            every pocket the child happens to have, since a household total
            can't be pinned to one fixed pocket name). The wildcard's OWN
            reduction is always SUM — the one sensible combination for "the
            child's total across its own dict of pockets" — the aggregate's
            declared `op` then combines the resulting per-child numbers
            ACROSS children exactly as it already did for the plain form.
        """
        if "[*]" in child_path:
            prefix, _, suffix = child_path.partition("[*]")
            suffix = suffix.lstrip(".")
            container = StateAccessor(state).get(prefix)
            if not isinstance(container, dict):
                return None
            total = 0.0
            for item in container.values():
                if not isinstance(item, dict):
                    continue
                val = item.get(suffix) if suffix else item
                if isinstance(val, (int, float)) and not isinstance(val, bool):
                    total += val
            return total

        val = StateAccessor(state).get(child_path)
        if isinstance(val, (int, float)) and not isinstance(val, bool):
            return val
        return None

    def _aggregate_from_child_states(
        self, spec: dict, links: list[dict], child_states: dict[str, dict],
        parent_sustain_id: str | None = None, own_state: dict | None = None,
    ) -> dict[str, dict]:
        """
        The core of ρ, factored out so both compute_rollup() (reads every
        child's state fresh from the DB) and _check_parent_binding_gate()
        (substitutes ONE child's not-yet-committed candidate state, reading
        every other child normally) share the exact same aggregation logic —
        no second implementation to drift out of sync.

        own_state (2 Aug 2026 — household total = parent + children): when
        given, the PARENT's own current value at child_path is folded into
        the same sum/avg/min/max/count as the children — a genuine combined
        total, not "what the children alone hold." Included/excluded with
        the identical honesty discipline as any child (a parent whose own
        state doesn't have the path is excluded and named, never silently
        treated as zero); its entry is tagged is_self:True and uses
        parent_sustain_id itself so a consumer can distinguish "the
        household's own contribution" from a child's without guessing.
        Backward compatible: parent_sustain_id/own_state both default to
        None (the pre-fix, children-only behaviour) for any caller that
        doesn't pass them.
        """
        aggregates: dict[str, dict] = {}
        for agg in spec.get("aggregates", []):
            agg_id = agg.get("id")
            child_path = agg.get("child_path", "")
            op = agg.get("op", "sum")
            reducer = self._ROLLUP_OPS.get(op)
            if reducer is None:
                raise ValueError(f"aggregate '{agg_id}' declares unknown op '{op}'.")

            included: list[dict] = []
            excluded: list[dict] = []
            values: list[float] = []

            if own_state is not None:
                try:
                    own_val = self._resolve_child_path_value(own_state, child_path)
                except Exception:
                    own_val = None
                if own_val is None:
                    excluded.append({
                        "sustain_id": parent_sustain_id, "slot": None, "member": "(household's own)",
                        "is_self": True,
                        "reason": f"path '{child_path}' not present or not numeric on the household's own state",
                    })
                else:
                    values.append(own_val)
                    included.append({
                        "sustain_id": parent_sustain_id, "slot": None, "member": "(household's own)",
                        "value": own_val, "is_self": True,
                    })

            for link in links:
                cid = link["child_sustain_id"]
                state = child_states.get(cid)
                if state is None:
                    excluded.append({
                        "sustain_id": cid, "slot": link.get("slot"), "member": link.get("member"),
                        "is_self": False,
                        "reason": "child state unavailable",
                    })
                    continue
                try:
                    val = self._resolve_child_path_value(state, child_path)
                except Exception:
                    val = None
                if val is None:
                    excluded.append({
                        "sustain_id": cid, "slot": link.get("slot"), "member": link.get("member"),
                        "is_self": False,
                        "reason": f"path '{child_path}' not present or not numeric on this child",
                    })
                    continue
                values.append(val)
                included.append({
                    "sustain_id": cid, "slot": link.get("slot"), "member": link.get("member"),
                    "value": val, "is_self": False,
                })

            aggregates[agg_id] = {
                "op": op, "child_path": child_path, "value": reducer(values),
                "included": included, "excluded": excluded,
                "includes_parent_own_contribution": own_state is not None,
            }
        return aggregates

    def _check_parent_binding_gate(self, child_sustain_id: str, candidate_child_state: dict) -> tuple[bool, str]:
        """
        Option C (confirmed with Bonnie — per-rule authority, defaulting to
        advisory): if child_sustain_id has a parent, and the parent has any
        aggregate invariant explicitly declared `"authority": "binding"`,
        check whether committing candidate_child_state (the child's
        not-yet-persisted post-operator state) would newly breach it.

        Refuses ONLY on an ok-before -> not-ok-after transition — the literal
        "would push the parent aggregate out of the parent's viable region."
        An aggregate that's already in violation for an unrelated reason (or
        for reasons predating this call) never blocks a later, unconnected
        child operation, and a child action that IMPROVES a currently-bad
        aggregate is never refused either — only the specific transition that
        would newly cause the breach is.

        Returns (True, "") when there's no parent, the parent has no binding
        aggregate rules, or the invariant was already failing (or already
        passing and remains passing) before this candidate state. Returns
        (False, reason) only on a genuine new breach.
        """
        parent_link = self.get_parent(child_sustain_id)
        if parent_link is None:
            return True, ""
        parent_id = parent_link["parent_sustain_id"]
        parent_spec = self._get_spec(parent_id)
        if parent_spec is None or not self._enforcement_enabled(parent_spec):
            return True, ""

        binding_invariants = [
            inv for inv in parent_spec.get("_compiled_invariants", [])
            if inv.get("is_aggregate") and inv.get("authority") == "binding"
        ]
        if not binding_invariants:
            return True, ""

        # "before" = every child read normally, including this one at its
        # actual current state. "after" = this one child's contribution
        # substituted with the not-yet-committed candidate.
        before = self._hypothetical_rollup(parent_id)
        after = self._hypothetical_rollup(
            parent_id, override_child_id=child_sustain_id, override_state=candidate_child_state,
        )

        parent_state_before = copy.deepcopy(self._load_state_dict(parent_id))
        parent_state_after = copy.deepcopy(parent_state_before)
        for agg_id, result in before["aggregates"].items():
            parent_state_before[agg_id] = result["value"]
        for agg_id, result in after["aggregates"].items():
            parent_state_after[agg_id] = result["value"]

        for inv in binding_invariants:
            ok_before, _ = evaluate_predicate(inv["node"], StateAccessor(parent_state_before), {})
            ok_after, reason_after = evaluate_predicate(inv["node"], StateAccessor(parent_state_after), {})
            if ok_before and not ok_after:
                return False, (
                    f"would push parent '{parent_id}' outside its binding invariant "
                    f"'{inv['id']}' ({inv['expr']}): {reason_after}"
                )

        return True, ""

    def _state_with_aggregates(self, sustain_id: str) -> dict:
        """
        The parent's real persisted state, with each declared aggregate's
        CURRENT computed value merged in under its own id — for display and
        invariant evaluation only. Never written back via _persist_state or
        any event-append path; a fresh copy is built on every call. Raises
        ValueError (same as _load_state_dict) if the sustain has no state.
        """
        state_dict = self._load_state_dict(sustain_id)
        spec = self._get_spec(sustain_id)
        if spec and spec.get("aggregates"):
            rollup = self.compute_rollup(sustain_id)
            augmented = copy.deepcopy(state_dict)
            for agg_id, result in rollup["aggregates"].items():
                augmented[agg_id] = result["value"]
            return augmented
        return state_dict
