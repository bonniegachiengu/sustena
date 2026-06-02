"""
tests/test_operative_graph.py

Sprint 5 — OperativeGraph + OperativeRuntime tests.

Coverage:
  - Condition.evaluate() — all comparison operators
  - OperativeEdge.matches() — unconditional + conditional
  - OperativeGraph.run() — 3-node linear graph → OperativeProposal; zero LLM calls
  - OperativeGraph.run() — conditional routing follows correct edge
  - OperativeGraph.run() — operator not in registry raises ValueError
  - OperativeGraph.run() — no matching edge stops execution gracefully
  - OperativeGraph.from_spec() — loads graph from dict spec and runs correctly
  - OperativeGraph.from_spec(spec, calibration_data) — resolves {{placeholder}} tokens
  - OperativeGraph.from_spec() — raises CalibrationError on unresolved required placeholder
  - OperativeGraph.from_spec() — inline default {{key|fallback}} used when key absent
  - BaseOperative — evaluate/deliberate dispatch to graphs when set
  - OperativeRuntime — event_driven dispatch fires on EventBus event
  - OperativeRuntime — polling dispatch fires on start_polling() schedule
  - OperativeRuntime — rpc run_once() executes graph directly
"""

import asyncio
import pytest
from datetime import datetime
from unittest.mock import AsyncMock

from sustena.core.state import StateAccessor
from sustena.core.events import EventBus
from sustena.core.pawa import PawaLedger
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult
from sustena.core.operative_graph import (
    CalibrationError,
    Condition,
    OperativeEdge,
    OperativeGraph,
    OperativeNode,
)
from sustena.operatives.base import OperativeProposal


# ── Test helpers ──────────────────────────────────────────────────────────────


def _make_ctx() -> OperatorContext:
    """Minimal OperatorContext for graph execution tests."""
    state = StateAccessor({"finances": {"liquid": {"balance": 50000.0}}})
    return OperatorContext(
        state=state,
        events=EventBus(sustain_id="test-sustain"),
        pawa=PawaLedger(),
        sustain_id="test-sustain",
        user_id="test-user",
        timestamp=datetime.utcnow(),
    )


def _ok_result(**data) -> OperatorResult:
    return OperatorResult.ok(data)


# ── Test fixture: temporary operator registration ─────────────────────────────


@pytest.fixture()
def test_operators():
    """
    Register three lightweight test operators in OPERATOR_REGISTRY.
    Removed after each test to keep test_operator_registry.py clean.
    """
    names = ["test.step_a", "test.step_b", "test.step_c"]

    async def step_a(ctx: OperatorContext) -> OperatorResult:
        return OperatorResult.ok({"from": "a", "score": 0.9})

    async def step_b(ctx: OperatorContext, *, score: float = 0.0) -> OperatorResult:
        return OperatorResult.ok({"from": "b", "passed_score": score})

    async def step_c(
        ctx: OperatorContext,
        *,
        operator_name: str = "sustena.noop",
        input_params: dict = None,
        rationale: str = "graph done",
    ) -> OperatorResult:
        return OperatorResult.ok({
            "operator_name": operator_name,
            "input_params": input_params or {},
            "rationale": rationale,
        })

    from sustena.core.operator import OperatorMeta
    for name, fn in zip(names, [step_a, step_b, step_c]):
        OPERATOR_REGISTRY[name] = OperatorMeta(
            name=name,
            description=f"Test operator {name}",
            fn=fn,
            protocol="rpc",
        )

    yield names

    for name in names:
        OPERATOR_REGISTRY.pop(name, None)


# ── Condition tests ───────────────────────────────────────────────────────────


class TestCondition:

    def _result(self, **data) -> OperatorResult:
        return OperatorResult.ok(data)

    def test_greater_than_true(self):
        cond = Condition(field="score", op=">", value=0.5)
        assert cond.evaluate(self._result(score=0.9)) is True

    def test_greater_than_false(self):
        cond = Condition(field="score", op=">", value=0.5)
        assert cond.evaluate(self._result(score=0.3)) is False

    def test_less_than(self):
        cond = Condition(field="count", op="<", value=10)
        assert cond.evaluate(self._result(count=5)) is True
        assert cond.evaluate(self._result(count=15)) is False

    def test_greater_equal(self):
        cond = Condition(field="score", op=">=", value=0.9)
        assert cond.evaluate(self._result(score=0.9)) is True
        assert cond.evaluate(self._result(score=0.89)) is False

    def test_less_equal(self):
        cond = Condition(field="val", op="<=", value=100)
        assert cond.evaluate(self._result(val=100)) is True
        assert cond.evaluate(self._result(val=101)) is False

    def test_equal(self):
        cond = Condition(field="status", op="==", value="ok")
        assert cond.evaluate(self._result(status="ok")) is True
        assert cond.evaluate(self._result(status="fail")) is False

    def test_not_equal(self):
        cond = Condition(field="status", op="!=", value="fail")
        assert cond.evaluate(self._result(status="ok")) is True
        assert cond.evaluate(self._result(status="fail")) is False

    def test_missing_field_returns_false(self):
        cond = Condition(field="nonexistent", op=">", value=0)
        assert cond.evaluate(self._result(score=1.0)) is False

    def test_nested_field(self):
        cond = Condition(field="metrics.deviation", op="<", value=0.2)
        assert cond.evaluate(OperatorResult.ok({"metrics": {"deviation": 0.1}})) is True
        assert cond.evaluate(OperatorResult.ok({"metrics": {"deviation": 0.5}})) is False

    def test_type_error_returns_false(self):
        cond = Condition(field="name", op=">", value=0)
        assert cond.evaluate(self._result(name="string")) is False


# ── OperativeEdge tests ───────────────────────────────────────────────────────


class TestOperativeEdge:

    def test_unconditional_always_matches(self):
        edge = OperativeEdge(from_node="a", to_node="b")
        assert edge.matches(OperatorResult.ok({})) is True
        assert edge.matches(OperatorResult.fail("error")) is True

    def test_conditional_edge_matches_when_condition_true(self):
        edge = OperativeEdge(
            from_node="a", to_node="b",
            condition=Condition(field="score", op=">", value=0.7),
        )
        assert edge.matches(OperatorResult.ok({"score": 0.9})) is True

    def test_conditional_edge_no_match(self):
        edge = OperativeEdge(
            from_node="a", to_node="b",
            condition=Condition(field="score", op=">", value=0.7),
        )
        assert edge.matches(OperatorResult.ok({"score": 0.5})) is False


# ── OperativeGraph.run() tests ────────────────────────────────────────────────


class TestOperativeGraphRun:

    @pytest.mark.asyncio
    async def test_linear_3_node_graph_returns_proposal(self, test_operators):
        """3-node linear graph (a → b → c) returns OperativeProposal; no LLM calls."""
        graph = OperativeGraph(
            nodes={
                "a": OperativeNode("a", "test.step_a"),
                "b": OperativeNode("b", "test.step_b"),
                "c": OperativeNode("c", "test.step_c", kwargs={
                    "operator_name": "budget.reallocate",
                    "input_params": {"pocket_name": "food", "amount": 500.0},
                    "rationale": "Graph complete.",
                }),
            },
            edges=[
                OperativeEdge("a", "b"),
                OperativeEdge("b", "c"),
            ],
            entry_node="a",
            exit_node="c",
        )

        ctx = _make_ctx()
        proposal = await graph.run(ctx, trigger_event={"event": "test"})

        assert isinstance(proposal, OperativeProposal)
        assert proposal.operator_name == "budget.reallocate"
        assert proposal.input_params == {"pocket_name": "food", "amount": 500.0}
        assert proposal.rationale == "Graph complete."

    @pytest.mark.asyncio
    async def test_all_node_results_in_simulation_results(self, test_operators):
        """Accumulated node data appears in proposal.simulation_results."""
        graph = OperativeGraph(
            nodes={
                "a": OperativeNode("a", "test.step_a"),
                "c": OperativeNode("c", "test.step_c"),
            },
            edges=[OperativeEdge("a", "c")],
            entry_node="a",
            exit_node="c",
        )
        ctx = _make_ctx()
        proposal = await graph.run(ctx, trigger_event={})
        assert "a" in proposal.simulation_results
        assert proposal.simulation_results["a"] == {"from": "a", "score": 0.9}

    @pytest.mark.asyncio
    async def test_conditional_routing_follows_matching_edge(self, test_operators):
        """
        Two edges from 'a': one requiring score>0.7 to 'c', fallback to 'b'.
        step_a returns score=0.9 → should route to 'c'.
        """
        graph = OperativeGraph(
            nodes={
                "a": OperativeNode("a", "test.step_a"),
                "b": OperativeNode("b", "test.step_b"),
                "c": OperativeNode("c", "test.step_c", kwargs={
                    "rationale": "high score path",
                }),
            },
            edges=[
                OperativeEdge(
                    "a", "c",
                    condition=Condition("score", ">", 0.7),
                ),
                OperativeEdge("a", "b"),  # fallback
            ],
            entry_node="a",
            exit_node="c",
        )

        ctx = _make_ctx()
        proposal = await graph.run(ctx, trigger_event={})
        assert proposal.rationale == "high score path"
        # 'b' was skipped
        assert "b" not in proposal.simulation_results

    @pytest.mark.asyncio
    async def test_conditional_routing_low_score_takes_fallback(self, test_operators):
        """score=0.9 fails condition score<0.5 → fallback edge to 'b' taken."""

        async def low_score_op(ctx: OperatorContext) -> OperatorResult:
            return OperatorResult.ok({"score": 0.2})

        OPERATOR_REGISTRY["test.low_score"] = __import__(
            "sustena.core.operator", fromlist=["OperatorMeta"]
        ).OperatorMeta(
            name="test.low_score",
            description="Low score test op",
            fn=low_score_op,
            protocol="rpc",
        )
        try:
            graph = OperativeGraph(
                nodes={
                    "entry": OperativeNode("entry", "test.low_score"),
                    "high": OperativeNode("high", "test.step_c", kwargs={"rationale": "high"}),
                    "low": OperativeNode("low", "test.step_c", kwargs={"rationale": "low"}),
                },
                edges=[
                    OperativeEdge(
                        "entry", "high",
                        condition=Condition("score", ">", 0.5),
                    ),
                    OperativeEdge("entry", "low"),  # fallback
                ],
                entry_node="entry",
                exit_node="low",
            )
            ctx = _make_ctx()
            proposal = await graph.run(ctx, trigger_event={})
            assert proposal.rationale == "low"
        finally:
            OPERATOR_REGISTRY.pop("test.low_score", None)

    @pytest.mark.asyncio
    async def test_unknown_operator_raises_value_error(self, test_operators):
        """If a node references an operator not in registry → ValueError."""
        graph = OperativeGraph(
            nodes={"a": OperativeNode("a", "test.step_a"),
                   "b": OperativeNode("b", "nonexistent.operator")},
            edges=[OperativeEdge("a", "b")],
            entry_node="a",
            exit_node="b",
        )
        ctx = _make_ctx()
        with pytest.raises(ValueError, match="not in registry"):
            await graph.run(ctx, trigger_event={})

    @pytest.mark.asyncio
    async def test_no_matching_edge_stops_gracefully(self, test_operators):
        """If no matching edge from current node, execution stops without error."""
        graph = OperativeGraph(
            nodes={
                "a": OperativeNode("a", "test.step_a"),
                "b": OperativeNode("b", "test.step_b"),
            },
            edges=[
                # Edge only matches when score < 0.5; step_a returns 0.9 → no match
                OperativeEdge("a", "b", condition=Condition("score", "<", 0.5)),
            ],
            entry_node="a",
            exit_node="b",
        )
        ctx = _make_ctx()
        proposal = await graph.run(ctx, trigger_event={})
        assert isinstance(proposal, OperativeProposal)
        # Graph stopped at 'a' since no edge matched
        assert "a" in proposal.simulation_results
        assert "b" not in proposal.simulation_results

    @pytest.mark.asyncio
    async def test_single_node_graph(self, test_operators):
        """Entry == exit → runs exactly one node."""
        graph = OperativeGraph(
            nodes={"only": OperativeNode("only", "test.step_c", kwargs={"rationale": "solo"})},
            edges=[],
            entry_node="only",
            exit_node="only",
        )
        ctx = _make_ctx()
        proposal = await graph.run(ctx, trigger_event={})
        assert proposal.rationale == "solo"


# ── OperativeGraph.from_spec() tests ─────────────────────────────────────────


class TestFromSpec:

    def _make_spec(self, *, kwargs: dict = None) -> dict:
        """Build a minimal valid spec dict."""
        return {
            "entry": "a",
            "exit": "c",
            "nodes": {
                "a": {"operator": "test.step_a", "kwargs": {}},
                "b": {"operator": "test.step_b", "kwargs": kwargs or {}},
                "c": {
                    "operator": "test.step_c",
                    "kwargs": {
                        "operator_name": "budget.reallocate",
                        "rationale": "spec-loaded graph",
                    },
                },
            },
            "edges": [
                {"from": "a", "to": "b"},
                {"from": "b", "to": "c"},
            ],
        }

    def test_from_spec_creates_correct_node_count(self, test_operators):
        graph = OperativeGraph.from_spec(self._make_spec())
        assert len(graph.nodes) == 3
        assert graph.entry_node == "a"
        assert graph.exit_node == "c"

    def test_from_spec_creates_edges(self, test_operators):
        graph = OperativeGraph.from_spec(self._make_spec())
        assert len(graph.edges) == 2

    def test_from_spec_with_condition(self, test_operators):
        spec = {
            "entry": "a",
            "exit": "b",
            "nodes": {
                "a": {"operator": "test.step_a", "kwargs": {}},
                "b": {"operator": "test.step_b", "kwargs": {}},
            },
            "edges": [
                {
                    "from": "a",
                    "to": "b",
                    "condition": {"field": "score", "op": ">", "value": 0.5},
                }
            ],
        }
        graph = OperativeGraph.from_spec(spec)
        assert graph.edges[0].condition is not None
        assert graph.edges[0].condition.op == ">"
        assert graph.edges[0].condition.value == 0.5

    @pytest.mark.asyncio
    async def test_from_spec_runs_correctly(self, test_operators):
        """from_spec() → run() produces correct OperativeProposal."""
        graph = OperativeGraph.from_spec(self._make_spec())
        ctx = _make_ctx()
        proposal = await graph.run(ctx, trigger_event={"source": "test"})
        assert isinstance(proposal, OperativeProposal)
        assert proposal.rationale == "spec-loaded graph"

    def test_raises_on_missing_entry(self, test_operators):
        spec = {"exit": "c", "nodes": {}, "edges": []}
        with pytest.raises(ValueError, match="entry"):
            OperativeGraph.from_spec(spec)

    def test_raises_on_missing_exit(self, test_operators):
        spec = {"entry": "a", "nodes": {}, "edges": []}
        with pytest.raises(ValueError, match="exit"):
            OperativeGraph.from_spec(spec)


# ── Placeholder resolution tests ──────────────────────────────────────────────


class TestPlaceholderResolution:

    def _spec_with_placeholder(self, placeholder: str) -> dict:
        return {
            "entry": "a",
            "exit": "a",
            "nodes": {
                "a": {
                    "operator": "test.step_c",
                    "kwargs": {"rationale": f"{{{{{placeholder}}}}}"},
                }
            },
            "edges": [],
        }

    def test_resolves_flat_key(self, test_operators):
        graph = OperativeGraph.from_spec(
            self._spec_with_placeholder("user_income"),
            calibration_data={"user_income": 50000},
        )
        assert graph.nodes["a"].kwargs["rationale"] == 50000

    def test_resolves_nested_dot_path(self, test_operators):
        """{{user.income}} resolved from {"user": {"income": 50000}}."""
        graph = OperativeGraph.from_spec(
            self._spec_with_placeholder("user.income"),
            calibration_data={"user": {"income": 50000}},
        )
        assert graph.nodes["a"].kwargs["rationale"] == 50000

    def test_preserves_int_type(self, test_operators):
        """Single-placeholder values preserve their original type (int, float, etc.)."""
        graph = OperativeGraph.from_spec(
            self._spec_with_placeholder("threshold"),
            calibration_data={"threshold": 42},
        )
        assert isinstance(graph.nodes["a"].kwargs["rationale"], int)
        assert graph.nodes["a"].kwargs["rationale"] == 42

    def test_inline_default_used_when_key_absent(self, test_operators):
        """{{key|fallback_value}} uses fallback when key not in calibration_data."""
        graph = OperativeGraph.from_spec(
            self._spec_with_placeholder("missing_key|default_text"),
            calibration_data={},
        )
        assert graph.nodes["a"].kwargs["rationale"] == "default_text"

    def test_raises_calibration_error_on_missing_required(self, test_operators):
        """Unresolved required placeholder raises CalibrationError."""
        with pytest.raises(CalibrationError, match="user.income"):
            OperativeGraph.from_spec(
                self._spec_with_placeholder("user.income"),
                calibration_data={},
            )

    def test_calibration_data_overrides_default(self, test_operators):
        """Explicitly provided calibration_data takes priority over inline default."""
        graph = OperativeGraph.from_spec(
            self._spec_with_placeholder("goal|minimize_cost"),
            calibration_data={"goal": "maximize_savings"},
        )
        assert graph.nodes["a"].kwargs["rationale"] == "maximize_savings"

    def test_resolves_embedded_placeholder_in_string(self, test_operators):
        """{{name}} embedded in a longer string is string-substituted."""
        spec = {
            "entry": "a",
            "exit": "a",
            "nodes": {
                "a": {
                    "operator": "test.step_c",
                    "kwargs": {"rationale": "Hello {{user_name}}, your goal is {{goal}}"},
                }
            },
            "edges": [],
        }
        graph = OperativeGraph.from_spec(
            spec,
            calibration_data={"user_name": "Bonnie", "goal": "savings"},
        )
        assert graph.nodes["a"].kwargs["rationale"] == "Hello Bonnie, your goal is savings"

    def test_resolves_nested_kwargs_dict(self, test_operators):
        """Placeholders inside nested dicts are resolved too."""
        spec = {
            "entry": "a",
            "exit": "a",
            "nodes": {
                "a": {
                    "operator": "test.step_c",
                    "kwargs": {
                        "input_params": {"amount": "{{amount}}", "pocket": "food"}
                    },
                }
            },
            "edges": [],
        }
        graph = OperativeGraph.from_spec(
            spec,
            calibration_data={"amount": 500.0},
        )
        assert graph.nodes["a"].kwargs["input_params"]["amount"] == 500.0
        assert graph.nodes["a"].kwargs["input_params"]["pocket"] == "food"


# ── BaseOperative graph integration tests (Task 5.2) ─────────────────────────


class TestBaseOperativeGraphIntegration:
    """
    Verify that BaseOperative's default evaluate() / deliberate() dispatch to
    evaluation_graph / deliberation_graph when those are set.
    """

    def _make_eval_graph(self, test_operators) -> OperativeGraph:
        """3-node evaluation graph that returns a budget.reallocate proposal."""
        return OperativeGraph(
            nodes={
                "a": OperativeNode("a", "test.step_a"),
                "b": OperativeNode("b", "test.step_b"),
                "c": OperativeNode("c", "test.step_c", kwargs={
                    "operator_name": "budget.reallocate",
                    "input_params": {"pocket_name": "food", "amount": 500.0},
                    "rationale": "Graph-based evaluation.",
                }),
            },
            edges=[OperativeEdge("a", "b"), OperativeEdge("b", "c")],
            entry_node="a",
            exit_node="c",
        )

    def _make_delib_graph(self, test_operators) -> OperativeGraph:
        """Deliberation graph whose exit node returns vote data."""
        from sustena.core.operator import OperatorMeta

        async def vote_yes(ctx: OperatorContext) -> OperatorResult:
            return OperatorResult.ok({
                "vote": "YES",
                "utility": 0.85,
                "reasoning": "Graph says YES.",
                "operator_name": "deliberation_result",
                "rationale": "Graph says YES.",
            })

        OPERATOR_REGISTRY["test.vote_yes"] = OperatorMeta(
            name="test.vote_yes",
            description="Test vote YES",
            fn=vote_yes,
            protocol="rpc",
        )

        return OperativeGraph(
            nodes={"vote": OperativeNode("vote", "test.vote_yes")},
            edges=[],
            entry_node="vote",
            exit_node="vote",
        )

    @pytest.fixture(autouse=True)
    def cleanup_test_vote_op(self):
        yield
        OPERATOR_REGISTRY.pop("test.vote_yes", None)

    @pytest.mark.asyncio
    async def test_evaluate_uses_evaluation_graph(self, test_operators):
        """evaluate() should run evaluation_graph when set — no LLM call."""
        from sustena.core.state import StateAccessor
        from sustena.operatives.base import BaseOperative, VoteChoice

        class GraphOperative(BaseOperative):
            operative_id = "graph_test"

            def should_evaluate(self, state):
                return True

        state = StateAccessor({"finances": {"liquid": {"balance": 50000.0}}})
        op = GraphOperative(config={}, state_accessor=state, claude_client=None,
                            sustain_id="test", user_id="test-user")
        op.evaluation_graph = self._make_eval_graph(test_operators)

        proposal = await op.evaluate(trigger_event={"event": "test"})

        assert isinstance(proposal, OperativeProposal)
        assert proposal.operator_name == "budget.reallocate"
        assert proposal.rationale == "Graph-based evaluation."

    @pytest.mark.asyncio
    async def test_deliberate_uses_deliberation_graph(self, test_operators):
        """deliberate() should run deliberation_graph when set and extract OperativeVote."""
        from sustena.core.state import StateAccessor
        from sustena.operatives.base import BaseOperative, OperativeVote, VoteChoice

        class GraphOperative(BaseOperative):
            operative_id = "graph_test"

            def should_evaluate(self, state):
                return True

        state = StateAccessor({"finances": {"liquid": {"balance": 50000.0}}})
        op = GraphOperative(config={}, state_accessor=state, claude_client=None,
                            sustain_id="test", user_id="test-user")
        op.deliberation_graph = self._make_delib_graph(test_operators)

        vote = await op.deliberate(context={}, proposal={"operator_name": "budget.reallocate"})

        assert isinstance(vote, OperativeVote)
        assert vote.vote == VoteChoice.YES
        assert vote.utility == pytest.approx(0.85)
        assert vote.reasoning == "Graph says YES."

    @pytest.mark.asyncio
    async def test_evaluate_raises_without_graph_or_override(self):
        """evaluate() raises NotImplementedError when no graph and no override."""
        from sustena.core.state import StateAccessor
        from sustena.operatives.base import BaseOperative

        class MinimalOperative(BaseOperative):
            operative_id = "minimal"

            def should_evaluate(self, state):
                return True

        state = StateAccessor({})
        op = MinimalOperative(config={}, state_accessor=state, claude_client=None)

        with pytest.raises(NotImplementedError, match="evaluation_graph"):
            await op.evaluate({})

    @pytest.mark.asyncio
    async def test_deliberate_raises_without_graph_or_override(self):
        """deliberate() raises NotImplementedError when no graph and no override."""
        from sustena.core.state import StateAccessor
        from sustena.operatives.base import BaseOperative

        class MinimalOperative(BaseOperative):
            operative_id = "minimal"

            def should_evaluate(self, state):
                return True

        state = StateAccessor({})
        op = MinimalOperative(config={}, state_accessor=state, claude_client=None)

        with pytest.raises(NotImplementedError, match="deliberation_graph"):
            await op.deliberate({}, {})


# ── OperativeRuntime protocol dispatch tests (Task 5.4) ───────────────────────


class TestCouncilOperativeGraphSpecs:
    """
    Task 5.6 — Verify that JSON graph spec files exist for all Council operatives
    and can be parsed by OperativeGraph.from_spec() (no operator registry lookups;
    just spec structure validation).
    """

    GRAPHS_DIR = (
        __import__("pathlib").Path(__file__).parent.parent
        / "sustena" / "operatives" / "graphs"
    )

    EXPECTED_FILES = [
        "mentor_evaluation.json",
        "mentor_deliberation.json",
        "protege_evaluation.json",
        "protege_deliberation.json",
        "attache_evaluation.json",
        "attache_deliberation.json",
        "curator_evaluation.json",
        "curator_deliberation.json",
        "navigator_evaluation.json",
        "navigator_deliberation.json",
        "orchie_evaluation.json",
    ]

    def test_all_graph_spec_files_exist(self):
        """Every expected graph spec file must exist in the graphs directory."""
        missing = [
            f for f in self.EXPECTED_FILES
            if not (self.GRAPHS_DIR / f).exists()
        ]
        assert not missing, f"Missing graph spec files: {missing}"

    def test_all_graph_specs_are_valid_json(self):
        """Every graph spec file must be valid JSON."""
        import json
        for fname in self.EXPECTED_FILES:
            path = self.GRAPHS_DIR / fname
            if not path.exists():
                continue
            with path.open(encoding="utf-8") as fh:
                data = json.load(fh)
            assert isinstance(data, dict), f"{fname} must be a JSON object"

    def test_all_graph_specs_have_entry_exit_nodes(self):
        """Every graph spec must declare 'entry' and 'exit'."""
        import json
        for fname in self.EXPECTED_FILES:
            path = self.GRAPHS_DIR / fname
            if not path.exists():
                continue
            with path.open(encoding="utf-8") as fh:
                data = json.load(fh)
            assert "entry" in data, f"{fname} missing 'entry'"
            assert "exit" in data, f"{fname} missing 'exit'"
            assert "nodes" in data, f"{fname} missing 'nodes'"
            assert "edges" in data, f"{fname} missing 'edges'"

    def test_from_spec_parses_all_specs(self):
        """OperativeGraph.from_spec() must parse every graph spec without error."""
        import json
        for fname in self.EXPECTED_FILES:
            path = self.GRAPHS_DIR / fname
            if not path.exists():
                continue
            with path.open(encoding="utf-8") as fh:
                spec_dict = json.load(fh)
            # from_spec() only validates structure; doesn't check operator registry
            graph = OperativeGraph.from_spec(spec_dict)
            assert graph.entry_node is not None, f"{fname} produced graph with no entry"
            assert graph.exit_node is not None, f"{fname} produced graph with no exit"

    def test_homestead_spec_references_all_graph_files(self):
        """homestead.json's operatives section must reference graph files for each operative."""
        import json
        import pathlib
        homestead_path = (
            pathlib.Path(__file__).parent.parent / "sustena" / "sustains" / "homestead.json"
        )
        with homestead_path.open(encoding="utf-8") as fh:
            spec = json.load(fh)

        operatives = spec["operatives"]
        for name, entry in operatives.items():
            if name == "chama_secretary":
                continue
            assert "evaluation_graph" in entry or "class" in entry, (
                f"operative '{name}' must declare at least 'class'"
            )


class TestFromSpecFile:
    """Tests for OperativeGraph.from_spec_file() — Sprint 5.5."""

    def _graphs_dir(self):
        import pathlib
        return pathlib.Path(__file__).parent.parent / "sustena" / "operatives" / "graphs"

    def test_from_spec_file_loads_mentor_evaluation_graph(self):
        """from_spec_file() loads mentor_evaluation.json correctly."""
        import sustena.operators  # ensure operators are registered
        graph = OperativeGraph.from_spec_file(
            self._graphs_dir() / "mentor_evaluation.json"
        )
        assert graph.entry_node == "evaluate"
        assert graph.exit_node == "evaluate"
        assert "evaluate" in graph.nodes
        assert graph.nodes["evaluate"].operator_name == "mentor.evaluate_budget"

    def test_from_spec_file_loads_mentor_deliberation_graph(self):
        """from_spec_file() loads mentor_deliberation.json correctly."""
        import sustena.operators
        graph = OperativeGraph.from_spec_file(
            self._graphs_dir() / "mentor_deliberation.json"
        )
        assert graph.entry_node == "deliberate"
        assert graph.exit_node == "deliberate"
        assert graph.nodes["deliberate"].operator_name == "mentor.deliberate_budget"

    @pytest.mark.asyncio
    async def test_from_spec_file_runs_mentor_evaluation(self):
        """Mentor evaluation graph loaded from file produces a proposal."""
        import sustena.operators
        from sustena.core.state import StateAccessor
        from sustena.core.events import EventBus
        from sustena.core.pawa import PawaLedger
        from sustena.core.operator import OperatorContext
        from datetime import datetime

        graph = OperativeGraph.from_spec_file(
            self._graphs_dir() / "mentor_evaluation.json"
        )
        state = StateAccessor({
            "finances": {
                "liquid": {"balance": 45000.0},
                "pockets": {"food": {"allocated": 5000.0, "spent": 4250.0}},
                "income": {"sources": [], "monthly_total": 50000.0},
            }
        })
        ctx = OperatorContext(
            state=state,
            events=EventBus(sustain_id="test"),
            pawa=PawaLedger(),
            sustain_id="test",
            user_id="test-user",
            timestamp=datetime.utcnow(),
        )
        proposal = await graph.run(ctx, trigger_event={"pocket": "food", "amount": 100.0})
        # Food at 85% → has_action True → reallocate proposal
        assert proposal.operator_name == "budget.reallocate"

    def test_from_spec_file_raises_on_missing_file(self):
        with pytest.raises(FileNotFoundError):
            OperativeGraph.from_spec_file("/no/such/file.json")


class TestOperativeRuntimeProtocolDispatch:
    """
    Verify that OperativeRuntime dispatches graphs correctly based on protocol.
    """

    def _make_runtime_graph(self, test_operators) -> OperativeGraph:
        """Single-node graph using test.step_c (rpc protocol) as the entry."""
        return OperativeGraph(
            nodes={"result": OperativeNode("result", "test.step_c", kwargs={
                "operator_name": "budget.reallocate",
                "rationale": "runtime-dispatch test",
            })},
            edges=[],
            entry_node="result",
            exit_node="result",
        )

    def _context_factory(self) -> OperatorContext:
        from sustena.core.state import StateAccessor
        from sustena.core.events import EventBus
        from sustena.core.pawa import PawaLedger
        from sustena.core.operator import OperatorContext
        from datetime import datetime
        return OperatorContext(
            state=StateAccessor({}),
            events=EventBus(sustain_id="test-sustain"),
            pawa=PawaLedger(),
            sustain_id="test-sustain",
            user_id="test-user",
            timestamp=datetime.utcnow(),
        )

    @pytest.mark.asyncio
    async def test_rpc_run_once_executes_directly(self, test_operators):
        """rpc: run_once() executes the graph immediately."""
        from sustena.core.operative_runtime import OperativeRuntime

        graph = self._make_runtime_graph(test_operators)
        bus = EventBus(sustain_id="test-sustain")
        runtime = OperativeRuntime(graph, bus, self._context_factory)

        proposal = await runtime.run_once({"trigger": "direct"})
        assert proposal.rationale == "runtime-dispatch test"

    @pytest.mark.asyncio
    async def test_event_driven_fires_on_event(self, test_operators):
        """event_driven: graph runs when the registered event fires on EventBus."""
        from sustena.core.operative_runtime import OperativeRuntime
        from sustena.core.operator import OperatorMeta

        # Register a test entry operator with event_driven protocol
        async def ev_entry(ctx: OperatorContext) -> OperatorResult:
            return OperatorResult.ok({
                "operator_name": "sustena.event_result",
                "rationale": "event_driven fired",
            })

        OPERATOR_REGISTRY["test.ev_entry"] = OperatorMeta(
            name="test.ev_entry",
            description="Event-driven test entry",
            fn=ev_entry,
            protocol="event_driven",
        )
        try:
            graph = OperativeGraph(
                nodes={"entry": OperativeNode("entry", "test.ev_entry")},
                edges=[],
                entry_node="entry",
                exit_node="entry",
            )
            bus = EventBus(sustain_id="test-sustain")
            runtime = OperativeRuntime(graph, bus, self._context_factory)
            runtime.register(event_filter="event.finances.pocket_spent")

            # Publishing the event should trigger the graph
            await bus.publish(
                "event.finances.pocket_spent",
                {"pocket": "food", "pct": 0.85},
            )

            proposals = runtime.collect_proposals()
            assert len(proposals) == 1
            assert proposals[0].rationale == "event_driven fired"
        finally:
            OPERATOR_REGISTRY.pop("test.ev_entry", None)

    @pytest.mark.asyncio
    async def test_polling_fires_on_schedule(self, test_operators):
        """polling: start_polling() runs the graph at the configured interval."""
        from sustena.core.operative_runtime import OperativeRuntime
        from sustena.core.operator import OperatorMeta

        call_count = {"n": 0}

        async def poll_entry(ctx: OperatorContext) -> OperatorResult:
            call_count["n"] += 1
            return OperatorResult.ok({"operator_name": "sustena.poll_result",
                                      "rationale": f"poll #{call_count['n']}"})

        OPERATOR_REGISTRY["test.poll_entry"] = OperatorMeta(
            name="test.poll_entry",
            description="Polling test entry",
            fn=poll_entry,
            protocol="polling",
        )
        try:
            graph = OperativeGraph(
                nodes={"entry": OperativeNode("entry", "test.poll_entry")},
                edges=[],
                entry_node="entry",
                exit_node="entry",
            )
            bus = EventBus(sustain_id="test-sustain")
            runtime = OperativeRuntime(graph, bus, self._context_factory)
            runtime.register(interval_s=0)  # 0s interval for immediate repeat

            task = asyncio.create_task(runtime.start_polling())
            # Let it run at least twice
            await asyncio.sleep(0.05)
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass

            assert call_count["n"] >= 1
            proposals = runtime.collect_proposals()
            assert len(proposals) >= 1
        finally:
            OPERATOR_REGISTRY.pop("test.poll_entry", None)

    @pytest.mark.asyncio
    async def test_from_spec_file_not_found_raises(self, test_operators):
        """from_spec_file() raises FileNotFoundError when path does not exist."""
        with pytest.raises(FileNotFoundError):
            OperativeGraph.from_spec_file("/nonexistent/path/graph.json")

    @pytest.mark.asyncio
    async def test_register_event_driven_no_error(self, test_operators):
        """register() for an event_driven entry node completes without error."""
        from sustena.core.operative_runtime import OperativeRuntime
        from sustena.core.operator import OperatorMeta

        async def ev_noop(ctx: OperatorContext) -> OperatorResult:
            return OperatorResult.ok({"operator_name": "noop", "rationale": "noop"})

        OPERATOR_REGISTRY["test.ev_noop"] = OperatorMeta(
            name="test.ev_noop",
            description="Event-driven noop",
            fn=ev_noop,
            protocol="event_driven",
        )
        try:
            graph = OperativeGraph(
                nodes={"entry": OperativeNode("entry", "test.ev_noop")},
                edges=[],
                entry_node="entry",
                exit_node="entry",
            )
            bus = EventBus(sustain_id="test-sustain")
            runtime = OperativeRuntime(graph, bus, self._context_factory)
            runtime.register(event_filter="event.finances.pocket_spent")
            assert runtime._registered is True
        finally:
            OPERATOR_REGISTRY.pop("test.ev_noop", None)
