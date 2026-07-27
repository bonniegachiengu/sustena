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
import json
import logging
import re
import sqlite3
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any

import sustena.operators  # noqa: F401 — triggers OPERATOR_REGISTRY population

from sustena.core.event_fold import fold_events
from sustena.core.events import EventBus
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult
from sustena.core.pawa import PawaLedger
from sustena.core.predicates import compile_invariant, evaluate_predicate
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

# {{placeholder}} token pattern
_TOKEN_RE = re.compile(r"\{\{(\w+)\}\}")


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
        """)
        self._db.commit()
        self._migrate_events_schema_sync()

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
        Load the JSON spec from sustena/sustains/{template_id}.json.
        Raises ValueError if the file is not found.
        """
        spec_path = _SUSTAINS_DIR / f"{template_id}.json"
        if not spec_path.exists():
            raise ValueError(
                f"Sustain spec '{template_id}' not found at {spec_path}. "
                f"Available specs: {[p.stem for p in _SUSTAINS_DIR.glob('*.json')]}"
            )
        with spec_path.open(encoding="utf-8") as fh:
            return json.load(fh)

    def _compile_spec_invariants(self, spec: dict) -> None:
        """
        Parse + schema-bind every declared invariant into a typed predicate AST
        (Move 1). Mutates spec in place, caching:
          spec["_compiled_invariants"]      -- [{id, description, expr, node}]
          spec["_invariant_compile_errors"] -- [{id, expr, errors}]

        A predicate referencing a state dimension the schema doesn't declare
        fails HERE, at load time — never silently at runtime. Idempotent and
        non-fatal: an invalid invariant is skipped from enforcement and
        recorded as a compile error, it does not block the sustain from
        loading (existing sustains keep working; see enforcement gate below).
        """
        if "_compiled_invariants" in spec:
            return
        state_schema = spec.get("state_schema", {})
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
            compiled.append({
                "id": inv.get("id"), "description": inv.get("description", ""), "expr": expr, "node": node,
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
        now = datetime.utcnow().isoformat()
        try:
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
            self._db.commit()
        except Exception:
            self._db.rollback()
            raise

    def commit_external_mutation(
        self, sustain_id: str, state: StateAccessor, event_name: str, payload: dict,
    ) -> None:
        """
        For state changes that happen outside execute_operator's operator-
        registry path — e.g. a council vote resolving a proposal via
        CouncilSession — but still need state = fold(events) to hold. Captures
        whatever `state` has tracked via its set()/append()/remove() calls,
        appends one event carrying those mutations, and updates the cache.
        No-op if state.mutations() is empty (nothing actually changed).

        Deliberately does NOT run the Slice 2 enforcement gate — call sites
        outside execute_operator predate that gate and adding it here would be
        a behaviour change beyond this method's job (making the log complete);
        closing that gap is real, separate follow-up work, not silently done
        as a side effect of an event-sourcing migration.
        """
        mutations = state.mutations()
        if not mutations:
            return
        self._append_events_and_update_cache(
            sustain_id, state.snapshot(),
            [{"event_name": event_name, "payload": payload, "mutations": mutations}],
        )

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

    def list_all(self) -> list[dict]:
        """
        Return a summary list of all sustain instances in the DB.

        Each entry contains enough for the devui selector dropdown and the
        Monitor panel hero tiles — id, template, name (label), status,
        active operative count, and current pawa balance from state if present.

        Used by GET /devui/sustains.
        """
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

            result.append({
                "id":                sustain_id,
                "label":             template_id.replace("_", " ").title(),
                "sub":               row["user_id"],
                "status":            "live",
                "template_id":       template_id,
                "created_at":        row["created_at"],
                "pockets":           pockets,
                "active_operatives": active_operatives,
                "pawa_balance":      pawa_balance,
            })

        return result

    # ── execute_operator ───────────────────────────────────────────────────────

    async def execute_operator(
        self,
        sustain_id: str,
        operator_name: str,
        params: dict,
        operative_id: str | None = None,
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

        Returns:
            OperatorResult (check .succeeded / .failed, .data, .reason)
        """
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
        ctx = OperatorContext(
            state=state,
            events=bus,
            pawa=ledger,
            sustain_id=sustain_id,
            user_id=user_id,
            operative_id=operative_id,
            timestamp=datetime.utcnow(),
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
            if published:
                events_norm = [
                    {
                        "id": ev.get("id"),
                        "event_name": ev.get("event_name"),
                        "payload": ev.get("_payload", {}),
                        "operator_log_id": ev.get("operator_log_id"),
                        "timestamp": ev.get("timestamp"),
                        "mutations": mutations if i == 0 else [],
                    }
                    for i, ev in enumerate(published)
                ]
                self._append_events_and_update_cache(sustain_id, state.snapshot(), events_norm)

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

        The database is NEVER written to — the original live state is untouched.
        Useful for forward-simulation before committing to a Council proposal.

        Each item in operator_sequence:
            {"operator": "<name>", "params": {<kwargs>}}

        Returns a list of per-step result dicts:
            {
                "operator":    str,
                "params":      dict,
                "result":      OperatorResult,
                "state_after": dict,   # state snapshot after this step
            }

        Steps that fail do NOT advance the forked state — subsequent steps see
        the state as it was before the failed step.
        """
        spec = self._get_spec(sustain_id)
        allowed_ops = (
            {op["name"] for op in spec.get("operators", [])}
            if spec else set()
        )

        # Fork — deep copy, never touches DB
        forked_dict = copy.deepcopy(self._load_state_dict(sustain_id))
        user_id = self._owners.get(sustain_id, "unknown")
        results: list[dict] = []

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
                })
                continue

            # Run against the forked state
            state_accessor = StateAccessor(forked_dict)
            bus    = EventBus(sustain_id=f"sim:{sustain_id}")
            ledger = PawaLedger()
            ctx = OperatorContext(
                state=state_accessor,
                events=bus,
                pawa=ledger,
                sustain_id=sustain_id,
                user_id=user_id,
                operative_id="simulate",
                timestamp=datetime.utcnow(),
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

            # Advance forked state only on success
            if result.succeeded:
                forked_dict = state_accessor.snapshot()

            results.append({
                "operator":    operator_name,
                "params":      params,
                "result":      result,
                "state_after": copy.deepcopy(forked_dict),
            })

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

    def evaluate_constraints(self, sustain_id: str) -> list[dict]:
        """
        Evaluate each invariant expression from the spec against the live state.
        Returns [{"expr": ..., "status": "ok"|"fail", "value": None}].
        """
        spec = self._get_spec(sustain_id)
        if spec is None:
            return []
        try:
            state_dict = self._load_state_dict(sustain_id)
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
