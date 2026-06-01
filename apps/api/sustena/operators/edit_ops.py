"""
sustena/operators/edit_ops.py

edit.* operators — spec/state/config mutations.

Operators:
  edit.state_patch    — apply patch operations to live state (rpc)
  edit.operator_spec  — modify operator metadata in the registry (rpc)
"""

from sustena.core.operator import (
    OPERATOR_REGISTRY,
    OperatorContext,
    OperatorResult,
    sustena_operator,
)

# Allowed patch operations (simplified subset of JSON Patch RFC 6902)
_ALLOWED_OPS = {"replace", "add", "remove"}


# ── edit.state_patch ───────────────────────────────────────────────────────────

@sustena_operator(
    name="edit.state_patch",
    protocol="rpc",
    description="Apply a list of patch operations to the live state using dot-path addressing.",
    side_effects=["event.edit.state_patched"],
    pawa_cost=2,
    license_tier="per_use",
    author="sustena_core",
)
async def edit_state_patch(
    ctx: OperatorContext,
    sustain_id: str | None = None,
    patch: list | None = None,
) -> OperatorResult:
    """
    Apply patch operations to the sustain's live state.

    Each operation in patch:
        {"op": "replace", "path": "dot.path.to.field", "value": <new_value>}
        {"op": "add",     "path": "dot.path.to.list",  "value": <item>}
        {"op": "remove",  "path": "dot.path.to.field"}

    Supports: replace (set a scalar), add (append to a list or set), remove (delete key).
    Requires Council approval if changes exceed significant thresholds — not enforced
    at the operator layer in Sprint 3 (deferred to Sprint 7 enrichment).

    params:
      sustain_id -- context hint (defaults to ctx.sustain_id)
      patch      -- list of patch operation dicts
    """
    patch = patch or []
    if not patch:
        return OperatorResult.fail(
            reason="patch list is empty — provide at least one operation.",
            constraint_violated="patch_not_empty",
        )

    applied: list[dict] = []
    errors: list[str] = []

    for op_dict in patch:
        op = op_dict.get("op", "")
        path = op_dict.get("path", "")
        value = op_dict.get("value")

        if op not in _ALLOWED_OPS:
            errors.append(f"Unknown op '{op}' at path '{path}'. Allowed: {sorted(_ALLOWED_OPS)}")
            continue
        if not path:
            errors.append("patch operation missing 'path' field")
            continue

        try:
            if op == "replace":
                ctx.state.set(path, value)
                applied.append({"op": op, "path": path})
            elif op == "add":
                if isinstance(value, dict):
                    ctx.state.append(path, value)
                else:
                    ctx.state.set(path, value)
                applied.append({"op": op, "path": path})
            elif op == "remove":
                # Set to None — full delete not supported by StateAccessor
                ctx.state.set(path, None)
                applied.append({"op": op, "path": path})
        except Exception as exc:
            errors.append(f"Failed to apply op '{op}' at '{path}': {exc}")

    if errors and not applied:
        return OperatorResult.fail(
            reason=f"All patch operations failed: {'; '.join(errors)}",
            constraint_violated="patch_applied",
        )

    await ctx.events.publish(
        "event.edit.state_patched",
        {
            "applied": applied,
            "errors": errors,
            "sustain_id": sustain_id or ctx.sustain_id,
        },
    )

    return OperatorResult.ok({
        "applied": applied,
        "errors": errors,
        "applied_count": len(applied),
        "error_count": len(errors),
    })


# ── edit.operator_spec ─────────────────────────────────────────────────────────

# Mutable metadata fields (restrict to safe, non-functional fields)
_EDITABLE_FIELDS = {"description", "pawa_cost", "license_tier", "author", "ui_schema"}


@sustena_operator(
    name="edit.operator_spec",
    protocol="rpc",
    description="Modify an operator's metadata in the in-memory OPERATOR_REGISTRY.",
    side_effects=["event.edit.operator_spec_modified"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def edit_operator_spec(
    ctx: OperatorContext,
    operator_name: str,
    field: str,
    value: object = None,
) -> OperatorResult:
    """
    Update a single metadata field on a registered operator.

    Only safe, non-functional fields can be modified at runtime:
      description, pawa_cost, license_tier, author, ui_schema.

    Structural fields (name, constraints, fn, protocol) are immutable —
    changing them requires a code deploy.

    params:
      operator_name -- e.g. "budget.allocate"
      field         -- which OperatorMeta field to change
      value         -- the new value
    """
    if operator_name not in OPERATOR_REGISTRY:
        return OperatorResult.fail(
            reason=f"Operator '{operator_name}' not found in OPERATOR_REGISTRY.",
            constraint_violated="operator_exists",
        )

    if field not in _EDITABLE_FIELDS:
        return OperatorResult.fail(
            reason=(
                f"Field '{field}' is not editable at runtime. "
                f"Editable fields: {sorted(_EDITABLE_FIELDS)}"
            ),
            constraint_violated="field_editable",
        )

    meta = OPERATOR_REGISTRY[operator_name]
    old_value = getattr(meta, field, None)
    setattr(meta, field, value)

    await ctx.events.publish(
        "event.edit.operator_spec_modified",
        {
            "operator_name": operator_name,
            "field": field,
            "old_value": str(old_value),
            "new_value": str(value),
        },
    )

    return OperatorResult.ok({
        "operator_name": operator_name,
        "field": field,
        "old_value": old_value,
        "new_value": value,
    })
