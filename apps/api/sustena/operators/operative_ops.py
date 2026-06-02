"""
sustena/operators/operative_ops.py

operative.* operators — spawn and manage operative instances.

Operators:
  operative.spawn — load a graph template, resolve {{placeholder}} calibration,
                    return the instantiated (resolved) spec ready for execution.
"""

import json
import pathlib
from typing import Any

from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator

# Graphs library directory (contains JSON templates with {{placeholder}} tokens)
_GRAPHS_DIR = pathlib.Path(__file__).parent.parent / "operatives" / "graphs"


# ── operative.spawn ────────────────────────────────────────────────────────────

@sustena_operator(
    name="operative.spawn",
    protocol="rpc",
    description=(
        "Load a graph template operative from the library, resolve {{placeholder}} tokens "
        "using caller-supplied calibration_data + live sustain state, and return the "
        "instantiated spec. No LLM calls. Raises CalibrationError on unresolved placeholders."
    ),
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def operative_spawn(
    ctx: OperatorContext,
    template_id: str,
    calibration_data: dict | None = None,
    caller_id: str = "system",
    scope: str = "session",
) -> OperatorResult:
    """
    Instantiate a graph template by resolving all {{placeholder}} tokens.

    Loads the template from:
      sustena/operatives/graphs/{template_id}.json

    Resolution order (per placeholder):
      1. calibration_data dict (highest priority — caller-injected context)
      2. Live sustain state (via ctx.state dot-path lookup for "state.*" tokens)
      3. Inline defaults declared in the template: {{key|default_value}}

    Returns the resolved spec dict in data["instantiated_spec"]. The caller can
    pass this to OperativeGraph.from_spec() to get a runnable graph.

    Raises CalibrationError (wrapped as OperatorResult.fail) on unresolved required
    placeholder tokens.

    params:
      template_id      : name of the graph template (without .json extension)
      calibration_data : caller-injected context dict for {{placeholder}} resolution
      caller_id        : which operative or user is spawning this (for audit log)
      scope            : "session" (ephemeral) | "persistent" (added to sustain)
    """
    from sustena.core.operative_graph import CalibrationError, OperativeGraph

    cal = dict(calibration_data or {})

    # Merge live state into calibration_data under "state.*" namespace
    # so that {{state.finances.liquid.balance}} resolves from the sustain state.
    _merge_state_into_cal(cal, ctx)

    template_path = _GRAPHS_DIR / f"{template_id}.json"
    if not template_path.is_file():
        return OperatorResult.fail(
            reason=(
                f"Template '{template_id}' not found at {template_path}. "
                f"Available templates: {_list_available_templates()}"
            ),
            constraint_violated="template_exists",
        )

    try:
        with template_path.open(encoding="utf-8") as fh:
            spec_dict = json.load(fh)
    except json.JSONDecodeError as exc:
        return OperatorResult.fail(
            reason=f"Template '{template_id}' contains invalid JSON: {exc}",
            constraint_violated="template_valid_json",
        )

    try:
        # from_spec resolves {{placeholder}} tokens and raises CalibrationError if any remain
        instantiated_graph = OperativeGraph.from_spec(spec_dict, calibration_data=cal)
    except CalibrationError as exc:
        return OperatorResult.fail(
            reason=str(exc),
            constraint_violated="calibration_complete",
        )

    # Rebuild the resolved spec by reading back node kwargs
    resolved_spec = _graph_to_spec(instantiated_graph)

    return OperatorResult.ok({
        "template_id":       template_id,
        "caller_id":         caller_id,
        "scope":             scope,
        "instantiated_spec": resolved_spec,
        "node_count":        len(instantiated_graph.nodes),
        "entry_node":        instantiated_graph.entry_node,
        "exit_node":         instantiated_graph.exit_node,
    })


# ── Helpers ───────────────────────────────────────────────────────────────────

def _merge_state_into_cal(cal: dict, ctx: OperatorContext) -> None:
    """
    Add a "state" key to cal containing the live sustain state snapshot.
    This allows {{state.finances.liquid.balance}} tokens to be resolved.
    """
    try:
        state_snapshot = ctx.state.snapshot()
        cal.setdefault("state", state_snapshot)
    except Exception:
        pass  # state not available — skip; placeholders will raise CalibrationError


def _list_available_templates() -> list[str]:
    """Return names (without extension) of all JSON files in the graphs directory."""
    if not _GRAPHS_DIR.is_dir():
        return []
    return sorted(p.stem for p in _GRAPHS_DIR.glob("*.json"))


def _graph_to_spec(graph: Any) -> dict:
    """Serialise an OperativeGraph back to a spec dict (with resolved kwargs)."""
    nodes: dict = {}
    for node_id, node in graph.nodes.items():
        nodes[node_id] = {
            "operator": node.operator_name,
            "kwargs": node.kwargs,
        }

    edges: list = []
    for edge in graph.edges:
        edge_dict: dict = {"from": edge.from_node, "to": edge.to_node}
        if edge.condition is not None:
            edge_dict["condition"] = {
                "field": edge.condition.field,
                "op":    edge.condition.op,
                "value": edge.condition.value,
            }
        edges.append(edge_dict)

    return {
        "entry": graph.entry_node,
        "exit":  graph.exit_node,
        "nodes": nodes,
        "edges": edges,
    }
