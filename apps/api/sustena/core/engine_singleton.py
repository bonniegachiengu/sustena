"""
sustena/core/engine_singleton.py

Process-wide SustainEngine singleton backed by the persistent sustena.db.

All devui routes use get_shared_engine() instead of SustainEngine() so they
see the same database and the same in-memory spec/operative caches across
every request.

Tests can call reset_shared_engine() to clear the singleton between modules.
"""

import threading

_lock = threading.Lock()
_engine = None


def _db_path_from_url(url: str) -> str:
    """Extract a sqlite3-compatible path from a SQLAlchemy database URL."""
    for prefix in ("sqlite+aiosqlite:///", "sqlite:///"):
        if url.startswith(prefix):
            return url[len(prefix):]
    return ":memory:"


def get_shared_engine():
    """
    Return the process-wide SustainEngine singleton.

    On first call, reads DATABASE_URL from settings to derive the DB path
    (same file the SQLAlchemy async engine uses), then creates a SustainEngine
    backed by that file.  Subsequent calls return the same instance.
    """
    global _engine
    if _engine is None:
        with _lock:
            if _engine is None:
                from sustena.config import settings
                from sustena.core.sustain_engine import SustainEngine
                db_path = _db_path_from_url(settings.database_url)
                _engine = SustainEngine(db_path=db_path)
    return _engine


def reset_shared_engine() -> None:
    """
    Tear down the singleton so the next call to get_shared_engine() creates a
    fresh one.  For use in tests only — never call this in production code.
    """
    global _engine
    with _lock:
        _engine = None
