"""
sustena/core/ingest_singleton.py

Process-wide IngestEngine singleton, mirroring engine_singleton.py — built
on top of the same shared SustainEngine so ingest sees the same live
sustains/state/events every request sees.

Tests can call reset_shared_ingest_engine() to clear the singleton between
modules (same pattern as reset_shared_engine()).
"""

import threading

_lock = threading.Lock()
_ingest_engine = None


def get_shared_ingest_engine():
    """Return the process-wide IngestEngine singleton, built on get_shared_engine()."""
    global _ingest_engine
    if _ingest_engine is None:
        with _lock:
            if _ingest_engine is None:
                from sustena.core.engine_singleton import get_shared_engine
                from sustena.core.ingest_engine import IngestEngine
                _ingest_engine = IngestEngine(get_shared_engine())
    return _ingest_engine


def reset_shared_ingest_engine() -> None:
    """Tear down the singleton so the next call creates a fresh one. Tests only."""
    global _ingest_engine
    with _lock:
        _ingest_engine = None
