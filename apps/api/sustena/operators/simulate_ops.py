"""
sustena/operators/simulate_ops.py

simulate.* operators — fork + run operator sequences against in-memory state.

The DB is never written to during simulation. Forks are stored in a
module-level registry (_FORK_REGISTRY) keyed by a UUID fork_id.

Operators:
  simulate.fork      — deep-copy current state into the fork registry (rpc)
  simulate.run_path  — execute an operator sequence against a forked state (rpc)
  simulate.score     — score the forked state against a named goal metric (rpc)
"""

import copy
import uuid
from datetime import datetime

from sustena.core.events import EventBus
from sustena.core.operator import (
    OPERATOR_REGISTRY,
    OperatorContext,
    OperatorResult,
    sustena_operator,
)
from sustena.core.pawa import PawaLedger
from sustena.core.state import StateAccessor

# ── In-memory fork registry ────────────────────────────────────────────────────
# Maps fork_id → {"state": dict, "sustain_id": str, "user_id": str, "path": list}

_FORK_REGISTRY: dict[str, dict] = {}


def clear_fork_registry() -> None:
    """Test helper — wipe all in-memory forks."""
    _FORK_REGISTRY.clear()


def get_fork_state(fork_id: str) -> dict | None:
    """
    Return a deep copy of the forked state dict, or None if the fork is unknown.

    Used by CouncilSession._create_sandbox() to give each sub-operative its own
    isolated view of the fork without exposing _FORK_REGISTRY directly.
    """
    fork = _FORK_REGISTRY.get(fork_id)
    if fork is None:
        return None
    return copy.deepcopy(fork["state"])


# ── simulate.fork ──────────────────────────────────────────────────────────────

@sustena_operator(
    name="simulate.fork",
    protocol="rpc",
    description="Deep-copy current sustain state into an in-memory fork for simulation.",
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def simulate_fork(
    ctx: OperatorContext,
    sustain_id: str | None = None,
) -> OperatorResult:
    """
    Create an in-memory copy of the current state. Returns a fork_id that
    simulate.run_path and simulate.score use to operate on the fork.

    The original live state and DB are untouched.

    params:
      sustain_id -- context hint (defaults to ctx.sustain_id)
    """
    fork_id = str(uuid.uuid4())
    _FORK_REGISTRY[fork_id] = {
        "state": ctx.state.snapshot(),
        "sustain_id": sustain_id or ctx.sustain_id,
        "user_id": ctx.user_id,
        "path": [],
    }
    return OperatorResult.ok({
        "fork_id": fork_id,
        "sustain_id": sustain_id or ctx.sustain_id,
    })


# ── simulate.run_path ──────────────────────────────────────────────────────────

@sustena_operator(
    name="simulate.run_path",
    protocol="rpc",
    description="Run a sequence of operators against a forked state. DB is never touched.",
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def simulate_run_path(
    ctx: OperatorContext,
    fork_id: str,
    operator_sequence: list,
) -> OperatorResult:
    """
    Execute a list of operator steps against the forked state identified by fork_id.

    Each item in operator_sequence:
        {"operator": "<name>", "params": {<kwargs>}}

    Steps that fail do NOT advance the forked state — subsequent steps see
    the state as it was before the failed step.

    Returns a SimulationPath: list of per-step result dicts.

    params:
      fork_id           -- ID returned by simulate.fork
      operator_sequence -- list of {"operator": str, "params": dict}
    """
    if fork_id not in _FORK_REGISTRY:
        return OperatorResult.fail(
            reason=f"Fork '{fork_id}' not found. Call simulate.fork first.",
            constraint_violated="fork_exists",
        )

    fork = _FORK_REGISTRY[fork_id]
    forked_dict = fork["state"]
    sustain_id = fork["sustain_id"]
    user_id = fork["user_id"]
    path_results: list[dict] = []

    for step in operator_sequence:
        op_name = step.get("operator", "")
        params = step.get("params", {})

        if op_name not in OPERATOR_REGISTRY:
            step_result = OperatorResult.fail(
                reason=f"Operator '{op_name}' not in OPERATOR_REGISTRY.",
                constraint_violated="operator_implemented",
            )
            path_results.append({
                "operator": op_name,
                "params": params,
                "status": step_result.status,
                "data": step_result.data,
                "reason": step_result.reason,
                "state_after": copy.deepcopy(forked_dict),
            })
            continue

        state_accessor = StateAccessor(forked_dict)
        sim_bus = EventBus(sustain_id=f"sim:{sustain_id}")
        ledger = PawaLedger()
        sim_ctx = OperatorContext(
            state=state_accessor,
            events=sim_bus,
            pawa=ledger,
            sustain_id=sustain_id,
            user_id=user_id,
            operative_id="simulate",
            timestamp=datetime.utcnow(),
        )

        meta = OPERATOR_REGISTRY[op_name]
        try:
            step_result = await meta.fn(sim_ctx, **params)
        except Exception as exc:
            step_result = OperatorResult.fail(
                reason=f"Operator '{op_name}' raised {type(exc).__name__}: {exc}",
                constraint_violated="operator_runtime_error",
            )

        if step_result.succeeded:
            forked_dict = state_accessor.snapshot()

        path_results.append({
            "operator": op_name,
            "params": params,
            "status": step_result.status,
            "data": step_result.data,
            "reason": step_result.reason,
            "state_after": copy.deepcopy(forked_dict),
        })

    # Update fork with final state
    fork["state"] = forked_dict
    fork["path"].extend(path_results)

    return OperatorResult.ok({
        "fork_id": fork_id,
        "steps": path_results,
        "steps_run": len(path_results),
        "steps_succeeded": sum(1 for s in path_results if s["status"] == "ok"),
        "final_state": copy.deepcopy(forked_dict),
    })


# ── Goal scorers ───────────────────────────────────────────────────────────────

def _score_budget_deviation(state: dict) -> float:
    """Lower over-spend deviation = higher score (1.0 = perfect, 0.0 = all over-spent)."""
    pockets = state.get("finances", {}).get("pockets", {})
    if not pockets:
        return 1.0
    scores = []
    for data in pockets.values():
        if not isinstance(data, dict):
            continue
        allocated = data.get("allocated", 0.0)
        spent = data.get("spent", 0.0)
        if allocated <= 0:
            continue
        pct = spent / allocated
        scores.append(max(0.0, 1.0 - max(0.0, pct - 1.0)))
    return round(sum(scores) / len(scores), 4) if scores else 1.0


def _score_savings_rate(state: dict) -> float:
    """Higher unspent ratio = higher score."""
    pockets = state.get("finances", {}).get("pockets", {})
    total_allocated = sum(
        v.get("allocated", 0.0) for v in pockets.values() if isinstance(v, dict)
    )
    total_spent = sum(
        v.get("spent", 0.0) for v in pockets.values() if isinstance(v, dict)
    )
    if total_allocated <= 0:
        return 1.0
    return round(1.0 - (total_spent / total_allocated), 4)


def _score_liquid_balance(state: dict) -> float:
    """Higher liquid balance relative to 50k baseline = higher score (capped at 1.0)."""
    liquid = state.get("finances", {}).get("liquid", {}).get("balance", 0.0)
    return round(min(1.0, liquid / 50000.0), 4)


_SCORERS: dict[str, object] = {
    "minimize_budget_deviation": _score_budget_deviation,
    "maximize_savings_rate": _score_savings_rate,
    "maximize_liquid_balance": _score_liquid_balance,
}


# ── simulate.score ─────────────────────────────────────────────────────────────

@sustena_operator(
    name="simulate.score",
    protocol="rpc",
    description="Score a forked state against a named goal metric, returning a 0.0–1.0 score.",
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def simulate_score(
    ctx: OperatorContext,
    fork_id: str,
    goal_metric: str,
) -> OperatorResult:
    """
    Evaluate the final state of a fork against a goal metric.

    Built-in metrics:
      minimize_budget_deviation  — 1.0 = no pocket over-spent
      maximize_savings_rate      — 1.0 = nothing spent
      maximize_liquid_balance    — 1.0 = >= 50k liquid

    params:
      fork_id     -- ID returned by simulate.fork
      goal_metric -- one of the built-in metric names
    """
    if fork_id not in _FORK_REGISTRY:
        return OperatorResult.fail(
            reason=f"Fork '{fork_id}' not found. Call simulate.fork first.",
            constraint_violated="fork_exists",
        )

    scorer = _SCORERS.get(goal_metric)
    if scorer is None:
        return OperatorResult.fail(
            reason=(
                f"Unknown goal metric '{goal_metric}'. "
                f"Available: {sorted(_SCORERS)}"
            ),
            constraint_violated="goal_metric_known",
        )

    forked_state = _FORK_REGISTRY[fork_id]["state"]
    score = scorer(forked_state)

    return OperatorResult.ok({
        "fork_id": fork_id,
        "goal_metric": goal_metric,
        "score": score,
        "interpretation": (
            "excellent" if score >= 0.8 else
            "good" if score >= 0.6 else
            "fair" if score >= 0.4 else
            "poor"
        ),
    })
