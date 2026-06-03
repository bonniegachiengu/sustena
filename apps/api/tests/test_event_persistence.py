"""
tests/test_event_persistence.py

E2 — operator events must persist to the events table and be readable.

Root cause being guarded against: execute_operator builds the EventBus without
an async DB session, so published events lived only in memory and never reached
the events table — the Monitor event feed/counter stayed at 0. The engine now
persists bus._published through its own sqlite3 connection and exposes
get_events().
"""

import pytest

from sustena.core.sustain_engine import SustainEngine


def _engine():
    # In-memory DB; _ensure_tables now also creates the events table.
    return SustainEngine(db_path=":memory:")


async def test_record_income_persists_event():
    eng = _engine()
    sid = eng.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
    res = await eng.execute_operator(sid, "budget.record_income", {"amount": 50000, "source": "Salary"})
    assert res.succeeded
    events = eng.get_events(sid)
    assert any(e["event_name"] == "event.finances.income_received" for e in events)


async def test_allocate_persists_event():
    eng = _engine()
    sid = eng.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
    await eng.execute_operator(sid, "budget.record_income", {"amount": 50000, "source": "Salary"})
    await eng.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 15000})
    names = [e["event_name"] for e in eng.get_events(sid)]
    assert "event.finances.pocket_allocated" in names


async def test_events_newest_first():
    eng = _engine()
    sid = eng.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
    await eng.execute_operator(sid, "budget.record_income", {"amount": 10000, "source": "A"})
    await eng.execute_operator(sid, "budget.record_income", {"amount": 20000, "source": "B"})
    events = eng.get_events(sid, limit=10)
    assert len(events) >= 2
    # Newest first — timestamps are non-increasing.
    ts = [e["timestamp"] for e in events]
    assert ts == sorted(ts, reverse=True)


async def test_event_payload_is_parsed_dict():
    eng = _engine()
    sid = eng.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
    await eng.execute_operator(sid, "budget.record_income", {"amount": 50000, "source": "Salary"})
    ev = next(e for e in eng.get_events(sid) if e["event_name"] == "event.finances.income_received")
    assert isinstance(ev["payload"], dict)
    assert ev["payload"].get("amount") == 50000


async def test_failed_operator_persists_no_event():
    eng = _engine()
    sid = eng.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
    res = await eng.execute_operator(sid, "budget.record_income", {"amount": -5, "source": "bad"})
    assert res.failed
    assert eng.get_events(sid) == []


def test_get_events_unknown_sustain_is_empty():
    eng = _engine()
    assert eng.get_events("does-not-exist") == []
