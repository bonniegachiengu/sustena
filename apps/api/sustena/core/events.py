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
  - Written to the SQLite events table (append-only, never deleted)
  - Mirrored to Firestore via FirestoreSync
  - Dispatched to any registered in-process subscribers (for operative triggers)
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
    Publishes events to the SQLite event log and dispatches to subscribers.

    Each EventBus instance is scoped to a single sustain execution context.
    The operator runner creates one per operator call and passes it in via OperatorContext.
    """

    def __init__(self, sustain_id: str, db_session=None, firestore_sync=None) -> None:
        self._sustain_id = sustain_id
        self._db = db_session          # SQLAlchemy async session (None = in-memory only)
        self._fs = firestore_sync      # FirestoreSync instance (None = skip)
        self._subscribers: dict[str, list[Callable]] = defaultdict(list)
        self._published: list[dict] = []  # All events published this execution context

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
        Publish an event.
        Returns the event_id.
        Writes to SQLite (if db session available) and Firestore (if configured).
        Dispatches to any registered subscribers.
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

        # Write to SQLite
        if self._db is not None:
            try:
                await self._write_to_db(event, ts)
            except Exception as e:
                logger.error("Failed to persist event '%s': %s", name, e)

        # Mirror to Firestore
        if self._fs is not None:
            try:
                await self._fs.sync_event(event)
            except Exception as e:
                logger.warning("Firestore sync failed for event '%s': %s", name, e)

        # Dispatch to subscribers
        await self._dispatch(name, payload)

        return event_id

    async def _write_to_db(self, event: dict, ts: datetime) -> None:
        """Persist event to SQLite events table."""
        import json
        from sustena.db.schema import events as events_table
        await self._db.execute(
            events_table.insert().values(
                id=event["id"],
                sustain_id=event["sustain_id"],
                event_name=event["event_name"],
                payload_json=json.dumps(event["_payload"]),
                operator_log_id=event.get("operator_log_id"),
                timestamp=ts,
            )
        )
        await self._db.commit()

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
        Retrieve event history from SQLite.
        Falls back to in-process published events if no DB session.
        """
        if self._db is None:
            # In-memory fallback (useful in tests)
            events = self._published
            if event_name:
                events = [e for e in events if e["event_name"] == event_name]
            return events[-limit:]

        import json
        from sqlalchemy import select, desc
        from sustena.db.schema import events as events_table

        query = (
            select(events_table)
            .where(events_table.c.sustain_id == self._sustain_id)
            .order_by(desc(events_table.c.timestamp))
            .limit(limit)
        )
        if event_name:
            query = query.where(events_table.c.event_name == event_name)

        result = await self._db.execute(query)
        rows = result.mappings().all()
        return [
            {
                "id": r["id"],
                "event_name": r["event_name"],
                "payload": json.loads(r["payload_json"]),
                "timestamp": r["timestamp"].isoformat() if hasattr(r["timestamp"], "isoformat") else str(r["timestamp"]),
            }
            for r in rows
        ]

    def published_this_context(self) -> list[dict]:
        """Return all events published during this execution context."""
        return list(self._published)
