"""
sustena/core/events.py

EventBus — the Event primitive.

Publishes immutable, structured records of state transitions.
Events are the ground truth of everything that has happened in a sustain.

Event name protocol (dot-path, 3+ segments):
  event.finances.income_received
  event.orders.order_fulfilled
  event.chama.contribution_recorded
  event.inventory.low_stock_alert

Events are:
  - Buffered into the current execution context by publish()
  - Mirrored to Firestore via FirestoreSync (a downstream copy, not the record)
  - Dispatched to any registered in-process subscribers (for operative triggers)

DURABILITY IS NOT THIS CLASS'S JOB. publish() does not commit anything; it
records that an event happened during this operator call. The engine drains
published_this_context() and writes the events, their per-sustain `seq` and
their state mutations in ONE transaction, re-raising on failure. That commit is
the ack (RECORD / Events-and-Time: an ack means durably committed) — a publish
call is not.

This class previously accepted a SQLAlchemy session and inserted rows itself,
swallowing failures. That path is gone: nothing ever passed a session, and its
INSERT omitted `seq` and `mutations_json`, so any row it wrote would have
broken state reconstruction for that sustain. One writer, one transaction —
ADR-0001 Decision 2.
"""

import logging
import re
import uuid
from collections import defaultdict
from datetime import datetime
from typing import Callable

logger = logging.getLogger(__name__)

# Event name must be 3+ dot-separated lowercase_snake_case segments
_EVENT_NAME_RE = re.compile(r"^event(\.[a-z][a-z0-9_]*){2,}$")


class EventBus:
    """
    Records events for one execution context and dispatches to subscribers.

    Each EventBus instance is scoped to a single sustain execution context.
    The operator runner creates one per operator call and passes it in via
    OperatorContext. It holds no database handle: the engine commits what was
    published here, together with state mutations, in one transaction.
    """

    def __init__(self, sustain_id: str, db_session=None, firestore_sync=None) -> None:
        self._sustain_id = sustain_id
        self._fs = firestore_sync      # FirestoreSync instance (None = skip)
        self._subscribers: dict[str, list[Callable]] = defaultdict(list)
        self._published: list[dict] = []  # All events published this execution context

        # `db_session` is refused, not ignored.
        #
        # This bus used to accept a SQLAlchemy session and INSERT events itself.
        # That writer was dead (nothing has ever passed a session) and, worse,
        # it was a trap: its INSERT omitted `seq` and `mutations_json`, the two
        # columns the fold depends on. A row written that way sorts before the
        # genesis event on replay and contributes no mutations — so enabling it
        # would have silently corrupted state reconstruction for that sustain.
        #
        # Durability belongs to exactly one place: the engine's own commit
        # (SustainEngine._append_events_and_update_cache), which writes seq and
        # mutations together in one transaction and re-raises on failure. Two
        # writers on one table with no shared transaction is the hazard ADR-0001
        # Decision 2 exists to remove.
        #
        # Failing loudly rather than accepting-and-ignoring: silently dropping a
        # caller's persistence request would be exactly the kind of quiet
        # non-application this codebase refuses elsewhere.
        if db_session is not None:
            raise ValueError(
                "EventBus does not persist events and cannot accept a db_session. "
                "Event durability belongs to SustainEngine's commit, which writes "
                "seq and mutations_json in one transaction. See ADR-0001 Decision 2."
            )

    # ── Validation ─────────────────────────────────────────────────────────────

    @staticmethod
    def validate_event_name(name: str) -> None:
        """
        Enforce the dot-protocol naming convention.
        Raises ValueError if the name is invalid.
        """
        if not _EVENT_NAME_RE.match(name):
            raise ValueError(
                f"Invalid event name '{name}'. "
                "Must match: event.<domain>.<type> (lowercase, snake_case, 3+ segments). "
                "Examples: event.finances.income_received, event.orders.order_placed"
            )

    # ── Publishing ─────────────────────────────────────────────────────────────

    async def publish(
        self,
        name: str,
        payload: dict,
        timestamp: datetime | None = None,
        operator_log_id: str | None = None,
    ) -> str:
        """
        Publish an event into this execution context.

        Returns the event_id. Buffers the event, mirrors it to Firestore if
        configured, and dispatches to subscribers. Does NOT durably commit —
        see the module docstring; the engine's transaction is the ack.
        """
        self.validate_event_name(name)

        event_id = str(uuid.uuid4())
        ts = timestamp or datetime.utcnow()

        event = {
            "id": event_id,
            "sustain_id": self._sustain_id,
            "event_name": name,
            "payload_json": str(payload),  # stored as string; serialised properly in DB write
            "operator_log_id": operator_log_id,
            "timestamp": ts.isoformat(),
            # Keep parsed payload for subscribers
            "_payload": payload,
        }

        self._published.append(event)
        logger.debug("EVENT %s → %s", self._sustain_id, name)

        # NOTE ON DURABILITY (RECORD / Events-and-Time: an ack means durably
        # committed). publish() does NOT durably commit — it buffers into this
        # execution context. The engine drains published_this_context() and
        # writes the events, their seq and their mutations in ONE transaction,
        # re-raising on failure so a failed history write can never pass for a
        # success. That commit is the ack; this call is not.
        #
        # This used to also INSERT via a SQLAlchemy session and swallow any
        # failure with logger.error. Removed: it was a dead second writer that
        # omitted the fold columns. See __init__.

        # Mirror to Firestore. Swallowing here is deliberate and correct: the
        # mirror is a downstream copy, not the source of truth, so a mirror
        # outage must not fail an operation whose real record committed fine.
        # The log line is the honest disclosure that the copy is behind.
        if self._fs is not None:
            try:
                await self._fs.sync_event(event)
            except Exception as e:
                logger.warning("Firestore sync failed for event '%s': %s", name, e)

        # Dispatch to subscribers
        await self._dispatch(name, payload)

        return event_id

    # ── Subscribing ────────────────────────────────────────────────────────────

    def subscribe(self, event_name: str, handler: Callable) -> None:
        """
        Register a handler for an event name.
        Handler signature: async def handler(payload: dict) -> None
        Supports wildcard suffix: "event.finances.*" matches all finance events.
        """
        self._subscribers[event_name].append(handler)

    async def _dispatch(self, event_name: str, payload: dict) -> None:
        """Call all matching subscribers for an event."""
        handlers: list[Callable] = []

        # Exact match
        handlers.extend(self._subscribers.get(event_name, []))

        # Wildcard match: "event.finances.*" matches "event.finances.income_received"
        parts = event_name.split(".")
        for i in range(2, len(parts)):
            wildcard = ".".join(parts[:i]) + ".*"
            handlers.extend(self._subscribers.get(wildcard, []))

        for handler in handlers:
            try:
                await handler(payload)
            except Exception as e:
                logger.error("Subscriber error for '%s': %s", event_name, e)

    # ── History ────────────────────────────────────────────────────────────────

    async def get_history(
        self,
        event_name: str | None = None,
        limit: int = 50,
    ) -> list[dict]:
        """
        Return events published during THIS execution context.

        Scoped to the current context on purpose. The bus is created per
        operator call and holds no database handle, so it cannot see a
        sustain's durable history — and should not pretend to. This used to
        fall back to a SQLAlchemy query when handed a session, which also
        ordered by `timestamp`; the durable log orders by `seq`, because
        near-simultaneous events tie on timestamp and sort nondeterministically
        (a real bug already fixed once on the engine's own read path).

        For a sustain's real history use SustainEngine.get_events(), which is
        seq-ordered and reads the committed log.
        """
        events = self._published
        if event_name:
            events = [e for e in events if e["event_name"] == event_name]
        return events[-limit:]

    def published_this_context(self) -> list[dict]:
        """Return all events published during this execution context."""
        return list(self._published)
