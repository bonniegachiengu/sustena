"""
tests/test_egress_engine.py

Engine-level tests for Slice 11 (Egress): egress.prepare_household_summary
(the operator), _queue_egress/list_egress/confirm_egress/cancel_egress
(SustainEngine), and the HARD SAFETY BOUNDARY — no financial/money-movement
path exists anywhere in this machinery.

Genericity note: egress.prepare_household_summary is declared on both
homestead.json and habitat.json (same finances.{liquid,pockets} shape both
already share) — these tests use homestead as the fixture, matching every
other engine-level test file, but the operator itself and every engine
method here contain zero homestead/habitat-specific code (grep-verifiable).
"""

import json as _json

import pytest

from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


class TestPrepareEgress:
    @pytest.mark.asyncio
    async def test_prepare_queues_prepared_row(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})

        result = await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "week1"})
        assert result.succeeded

        rows = engine.list_egress(sid, status="prepared")
        assert len(rows) == 1
        assert rows[0]["kind"] == "household_summary_export"
        assert rows[0]["target"] == "local_file"
        assert rows[0]["status"] == "prepared"
        payload = _json.loads(rows[0]["payload_json"])
        assert payload["liquid_balance"] == 5000.0

    @pytest.mark.asyncio
    async def test_prepare_never_mutates_state_or_events_beyond_its_own_event(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        before_state = engine.get_state(sid)
        before_events = len(engine.get_events(sid, limit=100))

        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})

        # State is byte-unchanged -- preparing an export mutates NOTHING.
        assert engine.get_state(sid) == before_state
        # Exactly one new event (the egress.prepared marker) was appended --
        # a real, gated, fold-recorded transition, just with empty mutations.
        assert len(engine.get_events(sid, limit=100)) == before_events + 1
        assert engine.rebuild_state(sid) == engine.get_state(sid)

    @pytest.mark.asyncio
    async def test_prepare_appends_a_real_egress_event(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})
        events = engine.get_events(sid, limit=5)
        assert events[0]["event_name"] == "event.egress.prepared"

    @pytest.mark.asyncio
    async def test_prepare_idempotent_same_label(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "week1"})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "week1"})
        rows = engine.list_egress(sid, status=None)
        assert len(rows) == 1

    @pytest.mark.asyncio
    async def test_prepare_different_labels_create_separate_entries(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "week1"})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "week2"})
        rows = engine.list_egress(sid, status=None)
        assert len(rows) == 2

    @pytest.mark.asyncio
    async def test_prepare_no_label_defaults_to_per_day_key(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        r1 = await engine.execute_operator(sid, "egress.prepare_household_summary", {})
        r2 = await engine.execute_operator(sid, "egress.prepare_household_summary", {})
        assert r1.data["idempotency_key"] == r2.data["idempotency_key"]
        assert len(engine.list_egress(sid, status=None)) == 1

    @pytest.mark.asyncio
    async def test_prepare_refused_when_operator_not_in_sustain_allow_list(self, engine: SustainEngine):
        # Real proof egress goes through the SAME operator-allow-list gate
        # as anything else: a definition that never selected
        # egress.prepare_household_summary as one of its operators refuses
        # the call exactly like any other unlisted operator would.
        definition = engine.create_definition(
            owner_user_id="u1", display_name="Bare", description="",
            dimensions=[{"name": "x", "type": "number", "default_value": 0}],
            invariants=[], operator_names=["edit.state_patch"],
        )
        sid = engine.instantiate(definition["template_id"], "u1", {"owner_ids": ["u1"]})
        result = await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})
        assert result.failed
        assert result.constraint_violated == "operator_allowed"
        assert engine.list_egress(sid, status=None) == []


class TestSimulateNeverQueuesEgress:
    @pytest.mark.asyncio
    async def test_egress_operator_in_simulate_creates_no_outbox_row(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        results = await engine.simulate(sid, [
            {"operator": "egress.prepare_household_summary", "params": {"label": "sandboxed"}},
        ])
        assert results[0]["result"].succeeded  # the operator itself runs fine in the sandbox
        assert engine.list_egress(sid, status=None) == []  # but nothing is ever persisted from it

    @pytest.mark.asyncio
    async def test_simulate_egress_leaves_live_state_and_events_untouched(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        before_state = engine.get_state(sid)
        before_events = engine.get_events(sid, limit=100)
        await engine.simulate(sid, [
            {"operator": "egress.prepare_household_summary", "params": {"label": "sandboxed"}},
        ])
        assert engine.get_state(sid) == before_state
        assert engine.get_events(sid, limit=100) == before_events


class TestConfirmEgress:
    @pytest.mark.asyncio
    async def test_confirm_writes_a_real_file(self, engine: SustainEngine, tmp_path, monkeypatch):
        monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 1234, "source": "x", "frequency": "once"})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "week1"})
        row = engine.list_egress(sid, status="prepared")[0]

        outcome = await engine.confirm_egress(row["id"])
        assert outcome["outbox"]["status"] == "sent"
        assert outcome["already_sent"] is False

        file_path = _json.loads(outcome["outbox"]["result_json"])["file_path"]
        from pathlib import Path
        content = _json.loads(Path(file_path).read_text(encoding="utf-8"))
        assert content["liquid_balance"] == 1234.0
        assert content["label"] == "week1"

    @pytest.mark.asyncio
    async def test_confirm_passes_through_confirmed_before_sent(self, engine: SustainEngine, tmp_path, monkeypatch):
        monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})
        row = engine.list_egress(sid, status="prepared")[0]
        outcome = await engine.confirm_egress(row["id"])
        assert outcome["outbox"]["confirmed_at"] is not None  # the confirmed step genuinely happened
        assert outcome["outbox"]["sent_at"] is not None

    @pytest.mark.asyncio
    async def test_confirm_already_sent_is_idempotent_no_resend(self, engine: SustainEngine, tmp_path, monkeypatch):
        monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})
        row = engine.list_egress(sid, status="prepared")[0]

        first = await engine.confirm_egress(row["id"])
        assert first["already_sent"] is False
        assert first["outbox"]["attempts"] == 1

        second = await engine.confirm_egress(row["id"])
        assert second["already_sent"] is True
        assert second["outbox"]["attempts"] == 1  # unchanged -- no second send attempted

    @pytest.mark.asyncio
    async def test_confirm_honest_failure_on_illegal_filename_character(self, engine: SustainEngine, tmp_path, monkeypatch):
        # A genuine OS-level failure (confirmed empirically while building
        # this operator: "?" in a Windows filename raises a real OSError),
        # not a simulated/fake one -- proves confirm_egress reports a real
        # send failure honestly rather than swallowing it.
        monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "bad?label"})
        row = engine.list_egress(sid, status="prepared")[0]

        outcome = await engine.confirm_egress(row["id"])
        assert outcome["outbox"]["status"] == "failed"
        assert outcome["outbox"]["failure_reason"]
        assert "error" in outcome

    @pytest.mark.asyncio
    async def test_failed_entry_is_retryable(self, engine: SustainEngine, tmp_path, monkeypatch):
        monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "bad?label"})
        row = engine.list_egress(sid, status="prepared")[0]

        first = await engine.confirm_egress(row["id"])
        assert first["outbox"]["status"] == "failed"
        assert first["outbox"]["attempts"] == 1

        # Retry with the SAME bad data still honestly fails again -- not a
        # bug, the underlying problem (illegal filename char) is unfixed.
        second = await engine.confirm_egress(row["id"])
        assert second["outbox"]["status"] == "failed"
        assert second["outbox"]["attempts"] == 2

    @pytest.mark.asyncio
    async def test_confirm_unknown_raises(self, engine: SustainEngine):
        with pytest.raises(ValueError):
            await engine.confirm_egress("does-not-exist")

    @pytest.mark.asyncio
    async def test_confirm_cancelled_raises(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})
        row = engine.list_egress(sid, status="prepared")[0]
        engine.cancel_egress(row["id"])
        with pytest.raises(ValueError):
            await engine.confirm_egress(row["id"])


class TestCancelEgress:
    @pytest.mark.asyncio
    async def test_cancel_prepared(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})
        row = engine.list_egress(sid, status="prepared")[0]
        assert engine.cancel_egress(row["id"]) is True
        assert engine.list_egress(sid, status="cancelled")[0]["id"] == row["id"]

    @pytest.mark.asyncio
    async def test_cancel_failed(self, engine: SustainEngine, tmp_path, monkeypatch):
        monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "bad?label"})
        row = engine.list_egress(sid, status="prepared")[0]
        await engine.confirm_egress(row["id"])
        assert engine.cancel_egress(row["id"]) is True

    def test_cancel_unknown_returns_false(self, engine: SustainEngine):
        assert engine.cancel_egress("does-not-exist") is False

    @pytest.mark.asyncio
    async def test_cancel_already_sent_returns_false(self, engine: SustainEngine, tmp_path, monkeypatch):
        monkeypatch.setattr("sustena.core.sustain_engine._EXPORTS_DIR", tmp_path)
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "egress.prepare_household_summary", {"label": "x"})
        row = engine.list_egress(sid, status="prepared")[0]
        await engine.confirm_egress(row["id"])
        assert engine.cancel_egress(row["id"]) is False


class TestNoFinancialPath:
    """
    HARD SAFETY BOUNDARY, verified by construction, not just by convention:
    no egress operator or send target can move money, in this slice or by
    accident in a future one reusing this module carelessly.
    """

    def test_only_one_egress_operator_exists_and_it_is_non_financial(self):
        from sustena.core.operator import OPERATOR_REGISTRY
        egress_operators = [name for name in OPERATOR_REGISTRY if name.startswith("egress.")]
        assert egress_operators == ["egress.prepare_household_summary"]

    def test_send_egress_rejects_any_target_other_than_local_file(self, engine: SustainEngine):
        with pytest.raises(ValueError):
            engine._send_egress({
                "target": "payment_gateway", "sustain_id": "x",
                "idempotency_key": "y", "payload_json": "{}",
            })

    def test_no_payment_vocabulary_in_egress_source(self):
        import sustena.core.egress as egress_module
        import sustena.operators.egress_ops as egress_ops_module
        import inspect
        source = inspect.getsource(egress_module) + inspect.getsource(egress_ops_module)
        forbidden = ["stripe", "mpesa", "paypal", "transfer_funds", "wire_transfer", "send_payment", "charge_card"]
        lowered = source.lower()
        hits = [w for w in forbidden if w in lowered]
        assert hits == [], f"Found financial vocabulary in egress source: {hits}"

    @pytest.mark.asyncio
    async def test_evaluate_operatives_never_sends_only_ever_prepares(self, engine: SustainEngine):
        # A Mentor suggestion proposing egress.prepare_household_summary
        # can only ever be ACCEPTED into a 'prepared' row -- accept_suggestion
        # calls execute_operator, never confirm_egress. Structural proof:
        # after accepting such a suggestion, nothing is 'sent'.
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        suggestions = engine.evaluate_operatives(sid)
        summary_sugg = [s for s in suggestions if s["rule_id"] == "summary_not_exported"]
        assert len(summary_sugg) == 1
        assert summary_sugg[0]["proposed_operator"] == "egress.prepare_household_summary"

        outcome = await engine.accept_suggestion(summary_sugg[0]["id"])
        assert outcome["result"].succeeded
        assert engine.list_egress(sid, status="sent") == []
        assert len(engine.list_egress(sid, status="prepared")) == 1
