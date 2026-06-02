"""
sustena/core/operative_graph.py

OperativeGraph — directed graph of operator calls for LLM-optional operative reasoning.

An operative's intelligence is defined as a graph of operator nodes connected by
conditional edges. The graph executes deterministically; no LLM is required unless
an llm.* node appears explicitly in the graph spec.

Key classes:
  CalibrationError  — raised when {{placeholder}} tokens remain unresolved at from_spec time
  Condition         — field/op/value condition evaluated against OperatorResult.data
  OperativeNode     — a single operator call (name + kwargs)
  OperativeEdge     — directed edge with optional Condition guard
  OperativeGraph    — the graph: nodes + edges + entry/exit + run() + from_spec()

Spec format (JSON):
  {
    "entry": "node_id",
    "exit":  "node_id",
    "nodes": {
      "node_id": {
        "operator": "operator.name",
        "kwargs": {"param": "value or {{placeholder}}"}
      }
    },
    "edges": [
      {"from": "a", "to": "b"},
      {"from": "b", "to": "c", "condition": {"field": "score", "op": ">", "value": 0.5}}
    ]
  }

Placeholder resolution in from_spec():
  1. calibration_data dict (caller-injected — highest priority)
  2. Default declared in template with pipe syntax: {{key|default_value}}
  Unresolved required placeholders raise CalibrationError.
"""

import logging
import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult

if TYPE_CHECKING:
    # Import only for type checkers — not at runtime to avoid circular imports.
    # operative_graph.py → operatives/base.py → operatives/__init__.py → mentor.py → operative_graph.py
    from sustena.operatives.base import OperativeProposal

logger = logging.getLogger(__name__)

_PLACEHOLDER_RE = re.compile(r"\{\{([^}]+)\}\}")

# ── Exceptions ────────────────────────────────────────────────────────────────


class CalibrationError(Exception):
    """Raised when a required {{placeholder}} in a graph spec cannot be resolved."""


# ── Graph primitives ──────────────────────────────────────────────────────────


@dataclass
class Condition:
    """
    Condition evaluated against OperatorResult.data at edge-routing time.

    field : dot-path into data dict, e.g. "score" or "metrics.deviation"
    op    : one of ">", "<", ">=", "<=", "==", "!="
    value : the right-hand comparison value
    """

    field: str
    op: str
    value: Any

    def evaluate(self, result: OperatorResult) -> bool:
        """Return True if the condition holds for the given result's data."""
        actual = _get_nested(result.data, self.field)
        if actual is None:
            return False
        try:
            if self.op == ">":
                return actual > self.value
            if self.op == "<":
                return actual < self.value
            if self.op == ">=":
                return actual >= self.value
            if self.op == "<=":
                return actual <= self.value
            if self.op == "==":
                return actual == self.value
            if self.op == "!=":
                return actual != self.value
        except TypeError:
            return False
        return False


@dataclass
class OperativeNode:
    """A single operator call within the OperativeGraph."""

    node_id: str
    operator_name: str
    kwargs: dict = field(default_factory=dict)


@dataclass
class OperativeEdge:
    """A directed edge between two nodes with an optional Condition guard."""

    from_node: str
    to_node: str
    condition: Condition | None = None

    def matches(self, result: OperatorResult) -> bool:
        """Return True if this edge should be followed given the result."""
        if self.condition is None:
            return True
        return self.condition.evaluate(result)


# ── OperativeGraph ─────────────────────────────────────────────────────────────


class OperativeGraph:
    """
    Directed graph of operator calls defining an operative's reasoning logic.

    Execution: entry → [run operator] → [route by conditions] → … → exit → OperativeProposal

    Nodes reference operators in OPERATOR_REGISTRY by name.
    Edges connect nodes; first matching edge is followed (unconditional edges always match).
    If no edge matches at a node, execution stops (treated as reaching exit).

    LLM nodes (llm.*) are just operators — when ANTHROPIC_API_KEY=mock they return mock
    responses. The graph executes the same either way.
    """

    def __init__(
        self,
        nodes: dict[str, OperativeNode],
        edges: list[OperativeEdge],
        entry_node: str,
        exit_node: str,
    ) -> None:
        self.nodes = nodes
        self.edges = edges
        self.entry_node = entry_node
        self.exit_node = exit_node
        # Build adjacency index for O(1) edge lookup
        self._adjacency: dict[str, list[OperativeEdge]] = {}
        for edge in edges:
            self._adjacency.setdefault(edge.from_node, []).append(edge)

    # ── Run ───────────────────────────────────────────────────────────────────

    async def run(
        self,
        context: OperatorContext,
        trigger_event: dict,
    ) -> "OperativeProposal":
        """
        Execute the graph starting from entry_node.

        Each node's operator is called with context and its declared kwargs.
        Results accumulate in a dict keyed by node_id. The exit node's result
        is used to construct the returned OperativeProposal.

        Stops when:
          - exit_node is reached, OR
          - no matching outgoing edge is found, OR
          - max_steps (safety guard against cycles) is exceeded

        If an operator raises an exception, it propagates to the caller.
        """
        accumulated: dict[str, Any] = {"trigger_event": trigger_event}
        last_result: OperatorResult | None = None
        current_id = self.entry_node
        max_steps = max(len(self.nodes) * 2, 10)

        for step in range(max_steps):
            node = self.nodes.get(current_id)
            if node is None:
                raise ValueError(
                    f"[OperativeGraph] node '{current_id}' not found. "
                    f"Known nodes: {list(self.nodes)}"
                )

            op_meta = OPERATOR_REGISTRY.get(node.operator_name)
            if op_meta is None:
                raise ValueError(
                    f"[OperativeGraph] operator '{node.operator_name}' not in registry"
                )

            logger.debug(
                "[OperativeGraph] step=%d node='%s' operator='%s'",
                step, current_id, node.operator_name,
            )
            # Resolve $-prefixed string kwargs as dot-path references into accumulated
            resolved_kwargs = _resolve_dynamic_kwargs(node.kwargs, accumulated)
            last_result = await op_meta.fn(context, **resolved_kwargs)
            accumulated[current_id] = last_result.data

            if current_id == self.exit_node:
                break

            # Route to next node via first matching edge
            outgoing = self._adjacency.get(current_id, [])
            next_id: str | None = None
            for edge in outgoing:
                if edge.matches(last_result):
                    next_id = edge.to_node
                    break

            if next_id is None:
                logger.debug(
                    "[OperativeGraph] no matching edge from '%s'; stopping", current_id
                )
                break

            current_id = next_id

        return self._build_proposal(last_result, accumulated)  # type: ignore[return-value]

    # ── from_spec ─────────────────────────────────────────────────────────────

    @staticmethod
    def from_spec(
        spec_dict: dict,
        calibration_data: dict | None = None,
    ) -> "OperativeGraph":
        """
        Deserialise an OperativeGraph from a JSON spec dict.

        {{placeholder}} tokens in node kwargs are resolved using calibration_data.
        Resolution order:
          1. calibration_data (dot-path lookup, e.g. {"user": {"income": 50000}} resolves {{user.income}})
          2. Inline default declared with pipe: {{key|default_value}}
        Raises CalibrationError if a required placeholder cannot be resolved.
        """
        cal = calibration_data or {}

        entry = spec_dict.get("entry")
        exit_ = spec_dict.get("exit")
        if not entry or not exit_:
            raise ValueError("Graph spec must declare 'entry' and 'exit' node IDs")

        nodes: dict[str, OperativeNode] = {}
        for node_id, node_spec in spec_dict.get("nodes", {}).items():
            raw_kwargs = node_spec.get("kwargs", {})
            resolved_kwargs = _resolve_placeholders(raw_kwargs, cal, node_id)
            nodes[node_id] = OperativeNode(
                node_id=node_id,
                operator_name=node_spec["operator"],
                kwargs=resolved_kwargs,
            )

        edges: list[OperativeEdge] = []
        for edge_spec in spec_dict.get("edges", []):
            cond: Condition | None = None
            if edge_spec.get("condition"):
                c = edge_spec["condition"]
                cond = Condition(
                    field=c["field"],
                    op=c["op"],
                    value=c["value"],
                )
            edges.append(OperativeEdge(
                from_node=edge_spec["from"],
                to_node=edge_spec["to"],
                condition=cond,
            ))

        return OperativeGraph(
            nodes=nodes,
            edges=edges,
            entry_node=entry,
            exit_node=exit_,
        )

    # ── from_spec_file ────────────────────────────────────────────────────────

    @classmethod
    def from_spec_file(
        cls,
        spec_path: Any,  # str | Path — avoid pathlib import at type level
        calibration_data: dict | None = None,
    ) -> "OperativeGraph":
        """
        Load an OperativeGraph from a JSON file on disk.

        Convenience wrapper around from_spec() for sustain spec integration.
        The spec_path can be absolute or relative to the caller.

        params:
          spec_path       : path to the JSON graph spec file (str or pathlib.Path)
          calibration_data: optional dict for {{placeholder}} resolution
        """
        import json
        from pathlib import Path

        path = Path(spec_path)
        if not path.is_file():
            raise FileNotFoundError(
                f"Graph spec file not found: {path}. "
                "Check that the path is absolute or relative to the graphs directory."
            )
        with path.open(encoding="utf-8") as fh:
            spec_dict = json.load(fh)
        return cls.from_spec(spec_dict, calibration_data)

    # ── Internal helpers ───────────────────────────────────────────────────────

    def _build_proposal(
        self,
        last_result: OperatorResult | None,
        accumulated: dict[str, Any],
    ) -> "OperativeProposal":
        """
        Construct an OperativeProposal from the graph's accumulated results.

        The exit node's result data is checked first for operator_name / input_params /
        rationale fields. Falls back to last_result, then to generic defaults.
        """
        # Lazy import to avoid circular dependency at module level.
        from sustena.operatives.base import OperativeProposal  # noqa: PLC0415

        exit_data = accumulated.get(self.exit_node, {})
        last_data = last_result.data if last_result else {}

        operator_name = (
            exit_data.get("operator_name")
            or last_data.get("operator_name")
            or "sustena.graph_result"
        )
        input_params = (
            exit_data.get("input_params")
            or last_data.get("input_params")
            or {}
        )
        rationale = (
            exit_data.get("rationale")
            or last_data.get("rationale")
            or "OperativeGraph execution complete."
        )

        return OperativeProposal(
            operator_name=operator_name,
            input_params=input_params,
            rationale=rationale,
            simulation_results={k: v for k, v in accumulated.items() if k != "trigger_event"},
        )


# ── Placeholder resolution helpers ────────────────────────────────────────────


def _get_nested(data: dict, dot_path: str) -> Any:
    """Walk a dot-separated path through a dict. Returns None if any key is missing."""
    current: Any = data
    for part in dot_path.split("."):
        if not isinstance(current, dict):
            return None
        current = current.get(part)
        if current is None:
            return None
    return current


def _resolve_placeholders(kwargs: dict, cal: dict, node_id: str) -> dict:
    """Recursively resolve {{placeholder}} tokens in all string values of kwargs."""
    result: dict = {}
    for key, value in kwargs.items():
        if isinstance(value, str):
            result[key] = _resolve_str(value, cal, node_id, key)
        elif isinstance(value, dict):
            result[key] = _resolve_placeholders(value, cal, node_id)
        elif isinstance(value, list):
            result[key] = [
                _resolve_str(item, cal, node_id, key) if isinstance(item, str) else item
                for item in value
            ]
        else:
            result[key] = value
    return result


def _resolve_dynamic_kwargs(kwargs: dict, accumulated: dict) -> dict:
    """
    Replace $-prefixed string kwargs with accumulated node results.

    "$trigger_event"           → accumulated["trigger_event"]
    "$node_id.field.path"      → accumulated["node_id"]["field"]["path"]

    Non-string and non-$-prefixed values are returned unchanged.
    """
    result: dict = {}
    for key, value in kwargs.items():
        if isinstance(value, str) and value.startswith("$"):
            path = value[1:]
            resolved = _get_nested(accumulated, path)
            result[key] = resolved if resolved is not None else value
        elif isinstance(value, dict):
            result[key] = _resolve_dynamic_kwargs(value, accumulated)
        else:
            result[key] = value
    return result


def _resolve_str(value: str, cal: dict, node_id: str, param_name: str) -> Any:
    """Replace {{placeholder}} tokens in a single string value."""
    matches = _PLACEHOLDER_RE.findall(value)
    if not matches:
        return value

    # Entire value is a single placeholder → preserve the resolved type (int, float, etc.)
    stripped = value.strip()
    if stripped == "{{" + matches[0] + "}}":
        return _resolve_token(matches[0], cal, node_id, param_name)

    # Embedded or multiple placeholders → string substitution
    def replacer(m: re.Match) -> str:
        return str(_resolve_token(m.group(1), cal, node_id, param_name))

    return _PLACEHOLDER_RE.sub(replacer, value)


def _resolve_token(token: str, cal: dict, node_id: str, param_name: str) -> Any:
    """
    Resolve a single placeholder token against calibration_data.

    token may include an inline default: "key|default_value"
    Raises CalibrationError if unresolved and no default.
    """
    default: Any = None
    has_default = False
    if "|" in token:
        token, default_raw = token.split("|", 1)
        token = token.strip()
        default = default_raw.strip()
        has_default = True

    # Dot-path lookup in calibration_data
    resolved = _get_nested(cal, token)
    if resolved is not None:
        return resolved

    # Flat key lookup (handles non-nested keys)
    if token in cal:
        return cal[token]

    if has_default:
        return default

    raise CalibrationError(
        f"Unresolved placeholder '{{{{{token}}}}}' in node '{node_id}', "
        f"param '{param_name}'. Provide it in calibration_data."
    )
