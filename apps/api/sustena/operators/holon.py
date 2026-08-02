"""
sustena/operators/holon.py

Nested holons (Phase 2, 2 Aug 2026) — user-driven runtime composition.
Canon: IO/strategy/SPEC-nested-holons.addendum.2026-08-02.md (§§4D.7/4C.6/
4E.6/4B.6/4H.6) + IO/strategy/sustena-nested-holons.design.2026-08-02.md.

Three operators:
  holon.create_child   — ⊕: instantiate + link an ad-hoc child holon,
                          gated by the parent's declared child_policy.
  holon.dissolve_child — ⊕⁻¹: return the child's funds, then unlink.
  holon.transfer       — the keystone: move liquid funds between this
                          holon and a directly linked parent/child,
                          atomically and conserved (§4E.6's D_conserve).

Authority default stated in canon and preserved here: a parent OBSERVES
its children, never vetoes them (roll-up ρ stays purely observational).
The ONE enforced cross-holon flow is funding — holon.transfer — whose
conservation invariant is binding by construction, checked as an active
runtime assertion (not assumed from the arithmetic), never just trusted.

Why these three operators reach into ctx.engine (a narrow, deliberate
exception — see OperatorContext's own docstring): every other operator in
this codebase only ever touches ONE sustain via ctx.state. A holon
operator's whole point is to read or atomically mutate a SECOND sustain,
which ctx.state — scoped to exactly one sustain — cannot express. These
three operators use ctx.engine's composition primitives (link_child,
get_parent, list_children, instantiate) and its atomic multi-sustain
commit (_commit_atomic_multi_sustain) directly, never touching ctx.state
for the counterpart side.

ctx.engine is None inside a forked simulation (simulate() deliberately
never populates it — see its own OperatorContext construction), so any of
these three operators refuses immediately and honestly rather than
crashing or, worse, silently reaching a real, uninvolved database. A
disclosed scope boundary, not an oversight: full multi-holon fork
simulation (previewing a transfer's effect on both holons before
committing) is a larger feature than this build — holon.transfer's own
guards (conservation + both gates, checked before any persistence) are
what make a live call safe, not a sandboxed preview.
"""

import uuid
from datetime import datetime

from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult, sustena_operator
from sustena.core.state import StateAccessor, StatePathError, StateValueError


def _liquid_balance(accessor: StateAccessor) -> float | None:
    val = accessor.get("finances.liquid.balance")
    if isinstance(val, (int, float)) and not isinstance(val, bool):
        return val
    return None


def _publish_direct(engine, sustain_id: str, event_name: str, payload: dict, current_state: dict | None = None) -> None:
    """
    Append one informational event directly via the engine, bypassing
    ctx.events/ctx.state entirely. See the callers' own comments for why:
    execute_operator()'s outer post-processing only ever persists again if
    ctx.events was published through OR ctx.state was mutated — an
    operator that does 100% of its own persistence via ctx.engine (every
    holon.* operator) must touch neither, or the outer path's stale
    operator-start snapshot would overwrite whatever this call already
    committed. current_state defaults to a fresh disk read so the event's
    accompanying cache write is never stale.
    """
    state = current_state if current_state is not None else engine._load_state_dict(sustain_id)
    engine._append_events_and_update_cache(sustain_id, state, [{
        "event_name": event_name, "payload": payload, "mutations": [],
    }])


async def _execute_holon_transfer(
    engine, from_sustain_id: str, to_sustain_id: str, amount: float,
    idempotency_key: str | None = None,
) -> OperatorResult:
    """
    The keystone, factored out so both the holon.transfer operator
    (human-initiated, either direction) and holon.dissolve_child (the
    mandatory fund-return precondition) share exactly ONE implementation
    of "move money between two holons, conserved and atomic" — there is
    no second code path that could drift out of sync with this one.

    D_conserve (§4E.6): Σ_holons balance(s') = Σ_holons balance(s). Checked
    as an active runtime assertion on the actual before/after totals, not
    assumed from the decrement/increment arithmetic — "provably correct"
    means checking, not trusting.

    Atomic: both the debit and the credit land in ONE sqlite3 transaction
    (SustainEngine._commit_atomic_multi_sustain) — a half-failure commits
    neither side. Every guard (link exists, amount valid, sufficient
    funds, both sustains' own enforcement gates, both sides' parent-
    binding gates, conservation) runs BEFORE any persistence, so a refusal
    always leaves both holons byte-unchanged.

    Idempotent when idempotency_key is provided (§4B.6: "a replayed
    transfer is exactly-once in effect, never double-applied") — a retry
    with the same key returns the SAME prior committed outcome instead of
    moving money again. Without a key, every call is its own transfer (the
    common human-confirms-once case).
    """
    if amount is None or amount <= 0:
        return OperatorResult.fail(
            reason="Transfer amount must be greater than zero.",
            constraint_violated="amount_positive",
        )
    if from_sustain_id == to_sustain_id:
        return OperatorResult.fail(
            reason="Cannot transfer a holon to itself.",
            constraint_violated="distinct_holons",
        )

    parent_of_from = engine.get_parent(from_sustain_id)
    children_of_from = engine.list_children(from_sustain_id)
    linked = (
        (parent_of_from is not None and parent_of_from["parent_sustain_id"] == to_sustain_id)
        or any(c["child_sustain_id"] == to_sustain_id for c in children_of_from)
    )
    if not linked:
        return OperatorResult.fail(
            reason=f"'{to_sustain_id}' is not a directly linked parent or child of '{from_sustain_id}'.",
            constraint_violated="holon_link_exists",
        )

    if idempotency_key:
        existing = engine._get_holon_transfer(idempotency_key)
        if existing is not None:
            return OperatorResult.ok({
                "transfer_id": existing["id"],
                "from_sustain_id": existing["from_sustain_id"],
                "to_sustain_id": existing["to_sustain_id"],
                "amount": existing["amount"],
                "idempotent_replay": True,
            })

    try:
        from_state_dict = engine._load_state_dict(from_sustain_id)
        to_state_dict = engine._load_state_dict(to_sustain_id)
    except ValueError as exc:
        return OperatorResult.fail(reason=str(exc), constraint_violated="holon_state_available")

    from_spec = engine._get_spec(from_sustain_id)
    to_spec = engine._get_spec(to_sustain_id)
    if from_spec is None or to_spec is None:
        return OperatorResult.fail(
            reason="one of the two holons has no spec.",
            constraint_violated="holon_spec_available",
        )

    from_accessor = StateAccessor(from_state_dict)
    to_accessor = StateAccessor(to_state_dict)

    from_before = _liquid_balance(from_accessor)
    to_before = _liquid_balance(to_accessor)
    if from_before is None or to_before is None:
        return OperatorResult.fail(
            reason="one of the two holons has no finances.liquid.balance to move.",
            constraint_violated="liquid_balance_present",
        )
    total_before = from_before + to_before

    if from_before < amount:
        return OperatorResult.fail(
            reason=(
                f"Transfer of KES {amount:,.2f} exceeds '{from_sustain_id}''s "
                f"liquid balance (KES {from_before:,.2f})."
            ),
            constraint_violated="from_balance_sufficient",
        )

    try:
        from_accessor.decrement("finances.liquid.balance", amount)
        to_accessor.increment("finances.liquid.balance", amount)
    except (StateValueError, StatePathError) as exc:
        return OperatorResult.fail(
            reason=f"transfer could not be applied: {exc}",
            constraint_violated="transfer_applies_cleanly",
        )

    from_after = _liquid_balance(from_accessor)
    to_after = _liquid_balance(to_accessor)
    total_after = from_after + to_after
    if round(total_after, 6) != round(total_before, 6):
        # Arithmetically this should be impossible given a matched
        # decrement/increment above — checked anyway, never assumed.
        return OperatorResult.fail(
            reason=(
                f"conservation check failed: total before={total_before}, "
                f"after={total_after} — transfer refused."
            ),
            constraint_violated="conservation",
        )

    transfer_meta = OPERATOR_REGISTRY.get("holon.transfer")
    if engine._enforcement_enabled(from_spec):
        ok, reason = engine._check_enforcement_gate(from_spec, from_accessor, {}, transfer_meta)
        if not ok:
            return OperatorResult.fail(reason=f"'{from_sustain_id}': {reason}", constraint_violated="enforcement_gate")
    if engine._enforcement_enabled(to_spec):
        ok, reason = engine._check_enforcement_gate(to_spec, to_accessor, {}, transfer_meta)
        if not ok:
            return OperatorResult.fail(reason=f"'{to_sustain_id}': {reason}", constraint_violated="enforcement_gate")

    ok, reason = engine._check_parent_binding_gate(from_sustain_id, from_accessor.snapshot())
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated="parent_binding_gate")
    ok, reason = engine._check_parent_binding_gate(to_sustain_id, to_accessor.snapshot())
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated="parent_binding_gate")

    transfer_id = str(uuid.uuid4())
    correlation_key = idempotency_key or transfer_id
    now_iso = datetime.utcnow().isoformat()
    from_event = {
        "event_name": "event.holon.transfer_committed",
        "payload": {
            "transfer_id": transfer_id, "direction": "debit", "counterparty": to_sustain_id,
            "amount": amount, "balance_after": from_after,
        },
        "mutations": from_accessor.mutations(),
    }
    to_event = {
        "event_name": "event.holon.transfer_committed",
        "payload": {
            "transfer_id": transfer_id, "direction": "credit", "counterparty": from_sustain_id,
            "amount": amount, "balance_after": to_after,
        },
        "mutations": to_accessor.mutations(),
    }

    try:
        engine._commit_atomic_multi_sustain(
            [
                (from_sustain_id, from_accessor.snapshot(), [from_event]),
                (to_sustain_id, to_accessor.snapshot(), [to_event]),
            ],
            extra_writes=[
                lambda: engine._write_holon_transfer_no_commit(
                    transfer_id, from_sustain_id, to_sustain_id, amount, correlation_key, now_iso,
                ),
            ],
        )
    except Exception as exc:
        return OperatorResult.fail(
            reason=f"transfer could not be committed: {exc}",
            constraint_violated="atomic_commit",
        )

    return OperatorResult.ok({
        "transfer_id": transfer_id, "from_sustain_id": from_sustain_id, "to_sustain_id": to_sustain_id,
        "amount": amount, "from_balance_after": from_after, "to_balance_after": to_after,
        "idempotent_replay": False,
    })


@sustena_operator(
    name="holon.transfer",
    description=(
        "Move liquid funds between this holon and a directly linked parent or child holon, "
        "atomically and conserved — the combined total across both holons is identical before and after."
    ),
    protocol="rpc",
    constraints=[],  # every real pre-condition (link exists, amount>0, sufficient funds, both gates, conservation) is enforced inline — see _execute_holon_transfer
    post_constraints=[],
    side_effects=["event.holon.transfer_committed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def holon_transfer(
    ctx: OperatorContext,
    to_sustain_id: str,
    amount: float,
    idempotency_key: str | None = None,
) -> OperatorResult:
    """
    params:
      to_sustain_id   -- the linked parent or child to move funds to (this
                          holon, ctx.sustain_id, is always the FROM side)
      amount          -- KES to move
      idempotency_key -- optional; a retry with the same key returns the
                          prior committed outcome instead of moving funds
                          again (a real client-supplied dedup key, e.g.
                          generated once when a human taps CONFIRM)
    """
    if ctx.engine is None:
        return OperatorResult.fail(
            reason="holon.transfer cannot run inside a forked simulation in this build.",
            constraint_violated="engine_available",
        )
    return await _execute_holon_transfer(
        ctx.engine, from_sustain_id=ctx.sustain_id, to_sustain_id=to_sustain_id,
        amount=amount, idempotency_key=idempotency_key,
    )


@sustena_operator(
    name="holon.create_child",
    description=(
        "Spawn a new child holon under this sustain from a declared template, and link it (⊕). "
        "Gated by this sustain's own child_policy allow-list — an ad-hoc child is bounded by a declaration, never unbounded."
    ),
    protocol="rpc",
    constraints=[],
    post_constraints=[],
    side_effects=["event.holon.child_created"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def holon_create_child(
    ctx: OperatorContext,
    template: str,
    name: str,
    slot: str | None = None,
) -> OperatorResult:
    """
    params:
      template -- must be in this sustain's spec["child_policy"]["accepts"]
      name     -- e.g. "Project IO" — passed through to the template's own
                  instantiation params (whichever ones it declares)
      slot     -- optional; auto-generated composition slot key if omitted
    """
    if ctx.engine is None:
        return OperatorResult.fail(
            reason="holon.create_child cannot run inside a forked simulation in this build.",
            constraint_violated="engine_available",
        )
    engine = ctx.engine
    spec = engine._get_spec(ctx.sustain_id) or {}
    policy = spec.get("child_policy") or {}
    accepts = policy.get("accepts") or []
    if template not in accepts:
        return OperatorResult.fail(
            reason=f"'{template}' is not an allowed child template for this holon. Allowed: {sorted(accepts)}",
            constraint_violated="child_policy_accepts",
        )

    cap = policy.get("cap")
    if cap is not None:
        existing_count = len(engine.list_children(ctx.sustain_id))
        if existing_count >= cap:
            return OperatorResult.fail(
                reason=f"This holon already has {existing_count} linked children — the declared cap is {cap}.",
                constraint_violated="child_policy_cap",
            )

    name = (name or "").strip()
    if not name:
        return OperatorResult.fail(reason="A name is required for the new holon.", constraint_violated="name_required")

    if not slot:
        slot = f"adhoc_{uuid.uuid4().hex[:10]}"

    try:
        child_sustain_id = engine.instantiate(template, ctx.user_id, {"owner_ids": [ctx.user_id], "name": name})
        link = engine.link_child(ctx.sustain_id, child_sustain_id, slot=slot, member=name)
    except ValueError as exc:
        return OperatorResult.fail(reason=str(exc), constraint_violated="holon_link_valid")

    # Published directly via the engine, NEVER via ctx.events -- see the
    # module docstring's atomicity note. If this operator ever published
    # through ctx.events instead, execute_operator()'s own post-processing
    # would see a non-empty published-events list and try to persist
    # ctx.state.snapshot() (a snapshot frozen at operator-START, before
    # this call's own engine-level writes) back over sustain_states --
    # harmless here (create_child never mutates the parent's own financial
    # state) but the exact bug that WOULD silently erase real money if a
    # future edit gave this operator a financial side effect. Publishing
    # this way from the start keeps both operators structurally identical
    # and immune to that class of bug regardless of what either does next.
    _publish_direct(engine, ctx.sustain_id, "event.holon.child_created", {
        "child_sustain_id": child_sustain_id, "template": template, "name": name, "slot": link.get("slot"),
    })

    return OperatorResult.ok({
        "child_sustain_id": child_sustain_id, "template": template, "name": name, "slot": link.get("slot"),
    })


@sustena_operator(
    name="holon.dissolve_child",
    description=(
        "Detach a linked child holon (⊕⁻¹) — the declared inverse of holon.create_child. Any balance the parent "
        "funded into the child is returned first via a conserved transfer, so dissolving a funded holon never "
        "strands or destroys money; the child's own accrued history is preserved, not deleted."
    ),
    protocol="rpc",
    constraints=[],
    post_constraints=[],
    side_effects=["event.holon.child_dissolved"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def holon_dissolve_child(ctx: OperatorContext, child_sustain_id: str) -> OperatorResult:
    """
    params:
      child_sustain_id -- must be a currently-linked child of this sustain
    """
    if ctx.engine is None:
        return OperatorResult.fail(
            reason="holon.dissolve_child cannot run inside a forked simulation in this build.",
            constraint_violated="engine_available",
        )
    engine = ctx.engine
    children = engine.list_children(ctx.sustain_id)
    if not any(c["child_sustain_id"] == child_sustain_id for c in children):
        return OperatorResult.fail(
            reason=f"'{child_sustain_id}' is not a linked child of this holon.",
            constraint_violated="holon_link_exists",
        )

    try:
        child_state_dict = engine._load_state_dict(child_sustain_id)
    except ValueError as exc:
        return OperatorResult.fail(reason=str(exc), constraint_violated="child_state_available")

    child_balance = _liquid_balance(StateAccessor(child_state_dict)) or 0.0
    returned = 0.0
    if child_balance > 0:
        transfer_result = await _execute_holon_transfer(
            engine, from_sustain_id=child_sustain_id, to_sustain_id=ctx.sustain_id,
            amount=child_balance, idempotency_key=f"dissolve:{child_sustain_id}",
        )
        if not transfer_result.succeeded:
            return OperatorResult.fail(
                reason=f"could not return the child's funds before dissolving: {transfer_result.reason}",
                constraint_violated="fund_return_precondition",
            )
        returned = transfer_result.data.get("amount", child_balance)

    unlinked = engine.unlink_child(ctx.sustain_id, child_sustain_id)
    if not unlinked:
        return OperatorResult.fail(
            reason="the link was already removed before this call completed.",
            constraint_violated="holon_link_exists",
        )

    # Published directly via the engine, using a FRESH read of this
    # sustain's CURRENT state -- never ctx.events. This is the real,
    # load-bearing case the note in holon_create_child warns about: when
    # returned > 0, _execute_holon_transfer already committed a real
    # credit into ctx.sustain_id's liquid balance, entirely outside
    # ctx.state. If this used ctx.events.publish() instead,
    # execute_operator()'s post-processing would persist ctx.state's
    # STALE pre-transfer snapshot back over sustain_states, silently
    # erasing the just-returned money. Re-reading fresh here sidesteps
    # that class of bug by construction.
    current_state = engine._load_state_dict(ctx.sustain_id)
    _publish_direct(engine, ctx.sustain_id, "event.holon.child_dissolved", {
        "child_sustain_id": child_sustain_id, "funds_returned": returned,
    }, current_state=current_state)
    return OperatorResult.ok({"child_sustain_id": child_sustain_id, "funds_returned": returned})
