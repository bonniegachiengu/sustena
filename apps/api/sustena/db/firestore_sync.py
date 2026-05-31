"""
sustena/db/firestore_sync.py

Firestore sync layer — keeps SQLite (local truth) mirrored to Firestore (cloud sync).

In development / when Google Cloud is not configured, MockFirestoreSync is used:
  - All writes are no-ops (or stored in an in-memory dict for inspection)
  - No network calls, no credentials required
  - Interface is identical to the real implementation

Switch to real Firestore by setting GOOGLE_CLOUD_PROJECT and providing
GOOGLE_APPLICATION_CREDENTIALS (or deploying to Cloud Run where the
service account is attached automatically).

Usage:
    from sustena.db.firestore_sync import get_firestore_sync
    fs = get_firestore_sync()
    await fs.sync_state(sustain_id, state_json)
"""

import logging
from collections import defaultdict
from typing import Any, Callable

from sustena.config import settings

logger = logging.getLogger(__name__)


# ── Mock implementation ────────────────────────────────────────────────────────


class MockFirestoreSync:
    """
    In-memory Firestore stub. No network calls, no credentials.
    Stores state in dicts — useful for asserting sync behaviour in tests.
    """

    def __init__(self) -> None:
        self._states: dict[str, str] = {}
        self._events: dict[str, list[dict]] = defaultdict(list)
        self._listeners: dict[str, list[Callable]] = defaultdict(list)
        logger.info(
            "MockFirestoreSync active — all Firestore writes are in-memory only. "
            "Deploy to Google Cloud to enable real sync."
        )

    async def sync_state(self, sustain_id: str, state_json: str) -> None:
        """Mirror current sustain state to Firestore (mock: store in dict)."""
        self._states[sustain_id] = state_json
        logger.debug("[MOCK FIRESTORE] sync_state: sustain=%s (%d bytes)", sustain_id, len(state_json))
        # Notify any registered listeners (simulates real-time updates)
        for callback in self._listeners.get(sustain_id, []):
            try:
                callback({"sustain_id": sustain_id, "state_json": state_json})
            except Exception as e:
                logger.warning("Listener error: %s", e)

    async def sync_event(self, event: dict) -> None:
        """Append event to Firestore subcollection (mock: append to list)."""
        sustain_id = event.get("sustain_id", "unknown")
        self._events[sustain_id].append(event)
        logger.debug("[MOCK FIRESTORE] sync_event: %s → %s", sustain_id, event.get("event_name"))

    def subscribe_state(self, sustain_id: str, callback: Callable) -> None:
        """Register a listener for state changes (mock: store callback)."""
        self._listeners[sustain_id].append(callback)
        logger.debug("[MOCK FIRESTORE] subscribe_state: sustain=%s", sustain_id)

    # ── Inspection helpers (useful in tests) ──────────────────────────────────

    def get_state(self, sustain_id: str) -> str | None:
        return self._states.get(sustain_id)

    def get_events(self, sustain_id: str) -> list[dict]:
        return list(self._events.get(sustain_id, []))

    def reset(self) -> None:
        """Clear all in-memory data — use between tests."""
        self._states.clear()
        self._events.clear()
        self._listeners.clear()


# ── Real implementation ────────────────────────────────────────────────────────


class FirestoreSync:
    """
    Real Firestore sync — requires google-cloud-firestore and credentials.
    Activated when GOOGLE_CLOUD_PROJECT is set and credentials are available.
    """

    def __init__(self, project: str) -> None:
        from google.cloud import firestore  # type: ignore
        self._db = firestore.AsyncClient(project=project)
        logger.info("FirestoreSync active — project=%s", project)

    async def sync_state(self, sustain_id: str, state_json: str) -> None:
        doc_ref = self._db.collection("sustains").document(sustain_id).collection("state").document("current")
        await doc_ref.set({"state_json": state_json})

    async def sync_event(self, event: dict) -> None:
        sustain_id = event.get("sustain_id", "unknown")
        await self._db.collection("sustains").document(sustain_id).collection("events").add(event)

    def subscribe_state(self, sustain_id: str, callback: Callable) -> None:
        doc_ref = self._db.collection("sustains").document(sustain_id).collection("state").document("current")
        doc_ref.on_snapshot(lambda doc_snapshot, changes, read_time: callback(doc_snapshot))


# ── Factory ───────────────────────────────────────────────────────────────────

_instance: Any = None


def get_firestore_sync() -> MockFirestoreSync | FirestoreSync:
    """
    Returns MockFirestoreSync in development or when cloud project is not configured.
    Returns real FirestoreSync in production with credentials available.
    """
    global _instance
    if _instance is not None:
        return _instance

    project = settings.google_cloud_project
    use_mock = settings.is_development or not project or project == "sustena-xii-placeholder"

    if use_mock:
        _instance = MockFirestoreSync()
    else:
        try:
            _instance = FirestoreSync(project=project)
        except Exception as e:
            logger.warning("Firestore init failed (%s) — falling back to mock.", e)
            _instance = MockFirestoreSync()

    return _instance
