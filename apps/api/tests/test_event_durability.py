"""
tests/test_event_durability.py

The property: a failed history write must never pass for a success.

RECORD / Events-and-Time is explicit that an ack means durably committed. Two
things previously undercut that:

  1. EventBus accepted a SQLAlchemy session and INSERTed events itself,
     swallowing any failure with logger.error — publish() returned an event_id
     whether or not the row landed.

  2. That INSERT omitted `seq` and `mutations_json`. Since the fold orders by
     seq and replays mutations, any row written that way would have sorted
     before the genesis event and contributed nothing — silently corrupting
     state reconstruction for that sustain. The writer was dead (nothing ever
     passed a session), which made it a loaded gun rather than a live bug.

Durability now belongs solely to the engine's commit, which writes events,
seq and mutations in one transaction and re-raises on failure.
"""

import sqlite3

import pytest

from sustena.core.events import EventBus
from sustena.core.sustain_engine import SustainEngine


class TestEventBusDoesNotPersist:

    def test_refuses_a_db_session_rather_than_ignoring_it(self):
        """
        Accepting-and-ignoring would silently drop a caller's persistence
        request — the exact quiet non-application this codebase refuses.
        """
        with pytest.raises(ValueError, match="does not persist"):
            EventBus(sustain_id="s1", db_session=object())

    def test_has_no_database_handle_at_all(self):
        bus = EventBus(sustain_id="s1")
        assert not hasattr(bus, "_db"), (
            "EventBus must hold no DB handle — one writer, one transaction"
        )

    def test_no_sql_insert_path_remains(self):
        import inspect

        from sustena.core import events

        src = inspect.getsource(events)
        assert "_write_to_db" not in src, "the dead second writer is back"
        assert "events_table.insert()" not in src, (
            "EventBus must not INSERT events; the engine owns durability"
        )

    @pytest.mark.asyncio
    async def test_publish_buffers_for_the_engine_to_commit(self):
        bus = EventBus(sustain_id="s1")
        await bus.publish("event.test.thing_happened", {"n": 1})
        published = bus.published_this_context()
        assert len(published) == 1
        assert published[0]["event_name"] == "event.test.thing_happened"


class TestEngineCommitIsTheAck:

    @pytest.mark.asyncio
    async def test_a_failed_history_write_fails_the_operation(self):
        """
        The core property. If the event log cannot be written, the caller must
        find out — never a success with a missing history row.
        """
        engine = SustainEngine(db_path=":memory:")
        sid = engine.instantiate(
            "homestead", "u1", {"household_name": "T", "owner_ids": ["u1"]}
        )

        before_state = engine.get_state(sid)
        before_events = len(engine.get_events(sid, limit=1000))

        real_conn = engine._db

        class FailsOnEventInsert:
            """Delegates everything, but refuses to write the events table.

            sqlite3.Connection.execute is a read-only C attribute, so the
            failure is injected with a proxy rather than a monkeypatch.
            """

            def execute(self, sql, *args, **kwargs):
                flat = " ".join(str(sql).upper().split())
                if "INSERT INTO EVENTS" in flat:
                    raise sqlite3.OperationalError(
                        "simulated disk failure writing history"
                    )
                return real_conn.execute(sql, *args, **kwargs)

            def __getattr__(self, item):
                return getattr(real_conn, item)

        engine._db = FailsOnEventInsert()
        try:
            with pytest.raises(Exception):
                await engine.execute_operator(
                    sid, "budget.record_income", {"amount": 5000, "source": "salary"}
                )
        finally:
            engine._db = real_conn

        # Nothing half-applied: the state change must not survive a failed
        # history write, or state and log diverge permanently.
        assert engine.get_state(sid) == before_state
        assert len(engine.get_events(sid, limit=1000)) == before_events
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    @pytest.mark.asyncio
    async def test_a_successful_write_is_durable_and_reproducible(self):
        engine = SustainEngine(db_path=":memory:")
        sid = engine.instantiate(
            "homestead", "u1", {"household_name": "T", "owner_ids": ["u1"]}
        )
        result = await engine.execute_operator(
            sid, "budget.record_income", {"amount": 5000, "source": "salary"}
        )
        assert result.succeeded
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    @pytest.mark.asyncio
    async def test_every_committed_event_carries_the_fold_columns(self):
        """
        The specific defect the removed writer would have reintroduced: a row
        with no seq sorts before genesis and replays nothing.
        """
        engine = SustainEngine(db_path=":memory:")
        sid = engine.instantiate(
            "homestead", "u1", {"household_name": "T", "owner_ids": ["u1"]}
        )
        await engine.execute_operator(
            sid, "budget.record_income", {"amount": 100, "source": "s"}
        )
        rows = engine._db.execute(
            "SELECT seq, mutations_json FROM events WHERE sustain_id = ?", (sid,)
        ).fetchall()
        assert rows, "expected committed events"
        for seq, _mutations in rows:
            assert seq is not None, (
                "an event with no seq sorts before genesis on replay and "
                "silently corrupts state reconstruction"
            )
