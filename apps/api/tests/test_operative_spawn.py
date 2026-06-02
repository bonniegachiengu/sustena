"""
tests/test_operative_spawn.py

Sprint 5.7 — operative.spawn operator tests.

Coverage:
  - operative.spawn loads template and resolves {{placeholder}} from calibration_data
  - operative.spawn resolves {{state.*}} from live sustain state
  - operative.spawn raises CalibrationError (wrapped as OperatorResult.fail) on missing
    required placeholder
  - operative.spawn returns an instantiated_spec with all placeholders resolved
  - operative.spawn → OperativeGraph.from_spec(result["instantiated_spec"]) works
  - operative.spawn returns OperatorResult.fail for unknown template_id
  - Orchie-spawned graph returns OperativeProposal
  - Councillor-spawned graph returns OperativeVote-shaped data (DelegatedVote precursor)
"""

import json
import pathlib
import pytest
from datetime import datetime
from unittest.mock import AsyncMock

import sustena.operators  # trigger registration
from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorMeta, OperatorResult
from sustena.core.operative_graph import CalibrationError, OperativeGraph


# ── Fixtures ──────────────────────────────────────────────────────────────────

GRAPHS_DIR = pathlib.Path(__file__).parent.parent / "sustena" / "operatives" / "graphs"


def _make_ctx(state_data: dict = None) -> OperatorContext:
    state = StateAccessor(state_data or {
        "finances": {
            "liquid": {"balance": 45000.0},
            "income": {"monthly_total": 50000.0},
        }
    })
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="test-user",
        timestamp=datetime.utcnow(),
    )


async def _call_spawn(ctx: OperatorContext, template_id: str, calibration_data: dict = None,
                      caller_id: str = "orchie", scope: str = "session") -> OperatorResult:
    meta = OPERATOR_REGISTRY["operative.spawn"]
    return await meta.fn(
        ctx,
        template_id=template_id,
        calibration_data=calibration_data or {},
        caller_id=caller_id,
        scope=scope,
    )


# ── Basic spawn tests ─────────────────────────────────────────────────────────


class TestOperativeSpawn:

    @pytest.mark.asyncio
    async def test_spawn_mentor_evaluation_template(self):
        """operative.spawn loads mentor_evaluation.json and returns instantiated spec."""
        ctx = _make_ctx()
        result = await _call_spawn(ctx, "mentor_evaluation")

        assert result.succeeded
        assert result.data["template_id"] == "mentor_evaluation"
        assert "instantiated_spec" in result.data
        assert result.data["node_count"] >= 1
        assert result.data["entry_node"] == "evaluate"

    @pytest.mark.asyncio
    async def test_spawn_unknown_template_returns_fail(self):
        """operative.spawn returns OperatorResult.fail when template_id is unknown."""
        ctx = _make_ctx()
        result = await _call_spawn(ctx, "nonexistent_template")

        assert result.failed
        assert result.constraint_violated == "template_exists"

    @pytest.mark.asyncio
    async def test_spawn_returns_caller_id_and_scope(self):
        """operative.spawn echoes caller_id and scope in the result."""
        ctx = _make_ctx()
        result = await _call_spawn(ctx, "mentor_evaluation",
                                   caller_id="orchie", scope="session")
        assert result.data["caller_id"] == "orchie"
        assert result.data["scope"] == "session"

    @pytest.mark.asyncio
    async def test_instantiated_spec_is_valid_json_serialisable(self):
        """The instantiated_spec must be serialisable to JSON."""
        ctx = _make_ctx()
        result = await _call_spawn(ctx, "mentor_evaluation")
        assert result.succeeded
        serialised = json.dumps(result.data["instantiated_spec"])
        assert serialised  # non-empty

    @pytest.mark.asyncio
    async def test_instantiated_spec_can_rebuild_graph(self):
        """OperativeGraph.from_spec(instantiated_spec) must work after spawn."""
        ctx = _make_ctx()
        result = await _call_spawn(ctx, "mentor_evaluation")
        assert result.succeeded

        graph = OperativeGraph.from_spec(result.data["instantiated_spec"])
        assert graph.entry_node == "evaluate"
        assert "evaluate" in graph.nodes


# ── Placeholder resolution tests ──────────────────────────────────────────────


class TestOperativeSpawnPlaceholders:
    """Create a temporary graph template with {{placeholder}} tokens and test resolution."""

    TEMP_TEMPLATE = "test_spawn_template"

    def _make_template(self, kwargs_with_placeholders: dict) -> dict:
        return {
            "entry": "action",
            "exit": "action",
            "nodes": {
                "action": {
                    "operator": "mentor.evaluate_budget",
                    "kwargs": kwargs_with_placeholders,
                }
            },
            "edges": [],
        }

    @pytest.fixture(autouse=True)
    def _write_and_cleanup_template(self, tmp_path, monkeypatch):
        """Write a temp template to the graphs dir and clean up after test."""
        import sustena.operators.operative_ops as spawn_mod
        self._original_graphs_dir = spawn_mod._GRAPHS_DIR
        monkeypatch.setattr(spawn_mod, "_GRAPHS_DIR", tmp_path)
        self._tmp_path = tmp_path
        yield
        # cleanup happens automatically via tmp_path

    def _write_template(self, spec: dict):
        path = self._tmp_path / f"{self.TEMP_TEMPLATE}.json"
        with path.open("w", encoding="utf-8") as fh:
            json.dump(spec, fh)

    @pytest.mark.asyncio
    async def test_resolves_calibration_data_placeholder(self):
        """{{user.income}} is resolved from calibration_data."""
        self._write_template(self._make_template({"action_threshold": "{{user.income}}"}))
        ctx = _make_ctx()
        result = await _call_spawn(
            ctx, self.TEMP_TEMPLATE,
            calibration_data={"user": {"income": 0.75}},
        )
        assert result.succeeded
        spec = result.data["instantiated_spec"]
        assert spec["nodes"]["action"]["kwargs"]["action_threshold"] == 0.75

    @pytest.mark.asyncio
    async def test_resolves_state_placeholder(self):
        """{{state.finances.liquid.balance}} resolves from live sustain state."""
        self._write_template(
            self._make_template({"action_threshold": "{{state.finances.income.monthly_total}}"})
        )
        ctx = _make_ctx()
        result = await _call_spawn(ctx, self.TEMP_TEMPLATE)
        assert result.succeeded
        spec = result.data["instantiated_spec"]
        assert spec["nodes"]["action"]["kwargs"]["action_threshold"] == 50000.0

    @pytest.mark.asyncio
    async def test_calibration_overrides_state(self):
        """calibration_data takes priority over state for the same key."""
        self._write_template(
            self._make_template({"action_threshold": "{{goal_threshold}}"})
        )
        ctx = _make_ctx()
        result = await _call_spawn(
            ctx, self.TEMP_TEMPLATE,
            calibration_data={"goal_threshold": 0.99},
        )
        assert result.succeeded
        spec = result.data["instantiated_spec"]
        assert spec["nodes"]["action"]["kwargs"]["action_threshold"] == 0.99

    @pytest.mark.asyncio
    async def test_unresolved_required_placeholder_returns_fail(self):
        """Unresolved required {{placeholder}} → OperatorResult.fail with CalibrationError."""
        self._write_template(
            self._make_template({"action_threshold": "{{required_but_missing}}"})
        )
        ctx = _make_ctx()
        result = await _call_spawn(ctx, self.TEMP_TEMPLATE, calibration_data={})
        assert result.failed
        assert result.constraint_violated == "calibration_complete"
        assert "required_but_missing" in result.reason

    @pytest.mark.asyncio
    async def test_inline_default_used_when_key_absent(self):
        """{{key|0.8}} uses fallback 0.8 when key is not in calibration_data."""
        self._write_template(
            self._make_template({"action_threshold": "{{missing_key|0.80}}"})
        )
        ctx = _make_ctx()
        result = await _call_spawn(ctx, self.TEMP_TEMPLATE)
        assert result.succeeded
        spec = result.data["instantiated_spec"]
        # Inline default is a string "0.80" — exact type depends on placeholder resolution
        assert spec["nodes"]["action"]["kwargs"]["action_threshold"] == "0.80"


# ── Orchie vs councillor spawn ────────────────────────────────────────────────


class TestSpawnContext:

    @pytest.mark.asyncio
    async def test_orchie_spawned_graph_produces_operative_proposal(self):
        """
        Orchie spawns a graph and runs it → produces OperativeProposal.
        Uses the real mentor_evaluation.json template.
        """
        ctx = _make_ctx(state_data={
            "finances": {
                "liquid": {"balance": 45000.0},
                "pockets": {"food": {"allocated": 5000.0, "spent": 4250.0}},
                "income": {"sources": [], "monthly_total": 50000.0},
            }
        })

        result = await _call_spawn(ctx, "mentor_evaluation", caller_id="orchie")
        assert result.succeeded

        graph = OperativeGraph.from_spec(result.data["instantiated_spec"])
        proposal = await graph.run(ctx, trigger_event={"pocket": "food"})

        from sustena.operatives.base import OperativeProposal
        assert isinstance(proposal, OperativeProposal)
        assert proposal.operator_name == "budget.reallocate"

    @pytest.mark.asyncio
    async def test_councillor_spawned_graph_produces_vote_data(self):
        """
        A councillor spawns the mentor_deliberation.json template and runs it
        → produces exit node data with vote/utility fields (DelegatedVote precursor).
        """
        ctx = _make_ctx(state_data={
            "finances": {
                "liquid": {"balance": 45000.0},
                "pockets": {},
                "income": {"sources": [], "monthly_total": 50000.0},
            }
        })

        proposal = {
            "operator_name": "budget.reallocate",
            "input_params": {"pocket_name": "food", "amount": 500.0},
        }

        result = await _call_spawn(ctx, "mentor_deliberation", caller_id="mentor")
        assert result.succeeded

        graph = OperativeGraph.from_spec(result.data["instantiated_spec"])
        graph_proposal = await graph.run(ctx, trigger_event=proposal)

        # Extract vote from exit node result
        exit_data = graph_proposal.simulation_results.get("deliberate", {})
        assert "vote" in exit_data
        assert exit_data["vote"] in ("YES", "NO", "ABSTAIN")
        assert "utility" in exit_data
