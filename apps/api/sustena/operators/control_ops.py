"""
sustena/operators/control_ops.py

control.* operators — execute and commit Council decisions.

A module-level snapshot registry (_SNAPSHOT_REGISTRY) stores pre-execution
state snapshots so control.rollback can restore to a prior version.

Operators:
  control.execute_approved — execute a PASSED Council proposal (rpc)
  control.rollback         — restore sustain state to a prior snapshot (rpc)
"""

import copy
import json
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

# ── In-memory snapshot registry ───────────────────────────────────────────────
# Maps sustain_id → list of snapshot dicts (append-only within a process session)
# Each snapshot: {"version": int, "state": dict, "timestamp": str, "reason": str}

_SNAPSHOT_REGISTRY: dict[str, list[dict]] = {}


def _save_snapshot(sustain_id: str, state: dict, reason: str) -> int:
    """Push a snapshot and return its version number."""
    if sustain_id not in _SNAPSHOT_REGISTRY:
        _SNAPSHOT_REGISTRY[sustain_id] = []
    version = len(_SNAPSHOT_REGISTRY[sustain_id]) + 1
    _SNAPSHOT_REGISTRY[sustain_id].append({
        "version": version,
        "state": copy.deepcopy(state),
        "timestamp": datetime.utcnow().isoformat(),
        "reason": reason,
    })
    return version


def _get_snapshot(sustain_id: str, version: int) -> dict | None:
    """Retrieve a specific snapshot by version number."""
    snapshots = _SNAPSHOT_REGISTRY.get(sustain_id, [])
    for snap in snapshots:
        if snap["version"] == version:
            return snap
    return None


def _latest_version(sustain_id: str) -> int:
    """Return the highest version number stored for this sustain (0 if none)."""
    snapshots = _SNAPSHOT_REGISTRY.get(sustain_id, [])
    return max((s["version"] for s in snapshots), default=0)


def clear_snapshot_registry() -> None:
    """Test helper — wipe all in-memory snapshots."""
    _SNAPSHOT_REGISTRY.clear()


# ── control.execute_approved ───────────────────────────────────────────────────

@sustena_operator(
    name="control.execute_approved",
    protocol="rpc",
    description="Execute a PASSED Council proposal against the live sustain state.",
    side_effects=["event.control.proposal_executed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def control_execute_approved(
    ctx: OperatorContext,
    proposal_id: str,
) -> OperatorResult:
    """
    Look up a PASSED proposal in state, execute its operator, update proposal status.

    Steps:
      1. Find the proposal by ID in state.council_proposals
      2. Check it has status PASSED
      3. Save a pre-execution snapshot
      4. Execute the proposal's operator_name with its input_json params
      5. Update proposal status to EXECUTED
      6. Publish event.control.proposal_executed

    params:
      proposal_id -- the UUID of a PASSED Council proposal
    """
    proposals: list = ctx.state.get("council_proposals", []) or []
    proposal = next((p for p in proposals if p.get("id") == proposal_id), None)

    if proposal is None:
        return OperatorResult.fail(
            reason=f"Proposal '{proposal_id}' not found in state.council_proposals.",
            constraint_violated="proposal_exists",
        )

    if proposal.get("status") != "PASSED":
        return OperatorResult.fail(
            reason=(
                f"Proposal '{proposal_id}' has status '{proposal.get('status')}' — "
                "only PASSED proposals can be executed."
            ),
            constraint_violated="proposal_is_passed",
        )

    operator_name: str = proposal.get("operator_name", "")
    if not operator_name or operator_name not in OPERATOR_REGISTRY:
        return OperatorResult.fail(
            reason=f"Proposal operator '{operator_name}' not found in OPERATOR_REGISTRY.",
            constraint_violated="operator_implemented",
        )

    try:
        input_params: dict = json.loads(proposal.get("input_json", "{}"))
    except (json.JSONDecodeError, TypeError):
        input_params = {}

    # 3. Save pre-execution snapshot
    pre_state = ctx.state.snapshot()
    version = _save_snapshot(ctx.sustain_id, pre_state, f"before executing proposal {proposal_id}")

    # 4. Execute the operator directly against ctx.state
    meta = OPERATOR_REGISTRY[operator_name]
    exec_bus = EventBus(sustain_id=ctx.sustain_id)
    exec_ledger = PawaLedger()
    exec_ctx = OperatorContext(
        state=ctx.state,
        events=exec_bus,
        pawa=exec_ledger,
        sustain_id=ctx.sustain_id,
        user_id=ctx.user_id,
        operative_id="control.execute_approved",
        timestamp=datetime.utcnow(),
    )

    try:
        op_result = await meta.fn(exec_ctx, **input_params)
    except Exception as exc:
        return OperatorResult.fail(
            reason=f"Operator '{operator_name}' raised {type(exc).__name__}: {exc}",
            constraint_violated="operator_runtime_error",
        )

    if not op_result.succeeded:
        return OperatorResult.fail(
            reason=f"Operator '{operator_name}' failed: {op_result.reason}",
            constraint_violated="operator_execution",
        )

    # 5. Update proposal status THROUGH StateAccessor.
    #
    # This previously assigned into `p` directly. `p` is a live reference into
    # the state list, so the status flip changed real state while recording no
    # mutation — the fold never learned the proposal had been executed, and a
    # rebuild would have resurrected it as still PASSED and re-executable.
    resolved_at = datetime.utcnow().isoformat()
    ctx.state.set(
        "council_proposals",
        [
            {**p, "status": "EXECUTED", "resolved_at": resolved_at}
            if p.get("id") == proposal_id else p
            for p in proposals
        ],
    )

    # 6. Publish execution event
    await ctx.events.publish(
        "event.control.proposal_executed",
        {
            "proposal_id": proposal_id,
            "operator_name": operator_name,
            "pre_execution_version": version,
            "result": op_result.data,
        },
    )

    return OperatorResult.ok({
        "proposal_id": proposal_id,
        "operator_name": operator_name,
        "operator_result": op_result.data,
        "pre_execution_snapshot_version": version,
    })


# ── control.rollback ───────────────────────────────────────────────────────────

@sustena_operator(
    name="control.rollback",
    protocol="rpc",
    description="Restore sustain state to a prior snapshot version.",
    side_effects=["event.control.state_rolled_back"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def control_rollback(
    ctx: OperatorContext,
    sustain_id: str | None = None,
    to_version: int | None = None,
) -> OperatorResult:
    """
    Restore state to a specific snapshot version (or the most recent one).

    Snapshots are created by control.execute_approved before each execution.
    Only in-process snapshots are available — does not restore from DB history.

    params:
      sustain_id -- sustain to roll back (defaults to ctx.sustain_id)
      to_version -- snapshot version to restore; omit for the most recent
    """
    target_sustain = sustain_id or ctx.sustain_id
    latest = _latest_version(target_sustain)

    if latest == 0:
        return OperatorResult.fail(
            reason=(
                f"No snapshots found for sustain '{target_sustain}'. "
                "Run control.execute_approved first to create a snapshot."
            ),
            constraint_violated="snapshot_exists",
        )

    target_version = to_version if to_version is not None else latest
    snapshot = _get_snapshot(target_sustain, target_version)

    if snapshot is None:
        return OperatorResult.fail(
            reason=(
                f"Snapshot version {target_version} not found for sustain '{target_sustain}'. "
                f"Available versions: 1–{latest}"
            ),
            constraint_violated="snapshot_version_exists",
        )

    restored_state = copy.deepcopy(snapshot["state"])

    # Apply restored state to ctx.state by overwriting each top-level key
    for top_key, value in restored_state.items():
        ctx.state.set(top_key, value)

    await ctx.events.publish(
        "event.control.state_rolled_back",
        {
            "sustain_id": target_sustain,
            "restored_version": target_version,
            "snapshot_timestamp": snapshot["timestamp"],
            "reason": snapshot.get("reason", ""),
        },
    )

    return OperatorResult.ok({
        "sustain_id": target_sustain,
        "restored_version": target_version,
        "snapshot_timestamp": snapshot["timestamp"],
    })
