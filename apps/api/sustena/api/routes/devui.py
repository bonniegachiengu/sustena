"""
sustena/api/routes/devui.py

Developer UI endpoints — mounted at /devui.
Only accessible with Authorization: Bearer {ADMIN_TOKEN} (REST)
or ?token={ADMIN_TOKEN} (WebSocket).

Endpoints
---------
GET  /devui/sustains                  — sustain selector list
GET  /devui/state?sustain_id=...      — full state for one sustain
POST /devui/console/execute           — run an operator from the console
POST /devui/simulate                  — forward-simulate a proposal
WS   /devui/state-stream?sustain_id=  — real-time state push
GET  /devui/sustain/{id}/graph        — operative council graph for a sustain
GET  /devui/sustain/{id}/operators    — operators allowed by the sustain spec
GET  /devui/sustain/{id}/definition   — raw structural definition (Studio Edit-inspect + sustain graph)

Legacy path-param routes are kept for backwards compatibility:
GET  /devui/sustain/{id}/state
GET  /devui/sustain/{id}/events
GET  /devui/sustain/{id}/proposals
GET  /devui/registry/operators
GET  /devui/registry/operatives
WS   /devui/state-stream/{id}
"""

import asyncio
import logging
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query, WebSocket, WebSocketDisconnect, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field

from sustena.api.routes.users import get_current_user, get_user_from_token

router = APIRouter()
logger = logging.getLogger(__name__)

# Every route below requires a real user session (get_current_user, a JWT
# obtained via POST /api/v1/users/login or /register) — this used to be a
# single shared ADMIN_TOKEN, which meant anyone with the token had full
# access and the token itself had to live in the public frontend bundle.


# ── Helpers ───────────────────────────────────────────────────────────────────

def ok(data: Any) -> dict:
    return {
        "status": "ok",
        "data": data,
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }


# ── Pydantic models ───────────────────────────────────────────────────────────

class ConsoleExecuteRequest(BaseModel):
    sustain_id: str
    # Accept both field names — frontend may send either
    operator: str | None = None
    operator_id: str | None = None
    inputs: dict = Field(default_factory=dict)
    params: dict = Field(default_factory=dict)  # alias for inputs

    def resolved_operator(self) -> str:
        op = self.operator_id or self.operator
        if not op:
            raise HTTPException(status_code=422, detail="operator or operator_id required")
        return op

    def resolved_params(self) -> dict:
        return self.params or self.inputs


class SimulateRequest(BaseModel):
    sustain_id: str
    proposal: list[dict] = Field(
        default_factory=list,
        description="Sequence of {operator, params} steps to simulate",
    )


class SimulatePipelineRequest(BaseModel):
    sustain_id: str
    proposal: list[dict] = Field(
        default_factory=list,
        description="Sequence of {operator, params} steps to run through simulate.fork → run_path → score",
    )
    goal_metric: str = Field(
        default="minimize_budget_deviation",
        description="Goal metric for simulate.score. Options: minimize_budget_deviation | maximize_savings_rate | maximize_liquid_balance",
    )


class PreviewWidgetRequest(BaseModel):
    spec_json: dict = Field(description="A ui_schema dict or operator spec containing ui_schema")
    mock_state: dict = Field(
        default_factory=dict,
        description="Optional mock state for source resolution: {inputs: {...}, state: {...}}",
    )


# ── 1. GET /devui/sustains ────────────────────────────────────────────────────

@router.get("/sustains", summary="List sustains for the selector dropdown")
async def list_sustains(current_user: dict = Depends(get_current_user)) -> dict:
    """
    Returns the CURRENT USER'S OWN sustains with id, label, status, pockets
    summary, and active operative count — enough for the TopBar selector
    and Monitor hero tiles.

    Scoped by owner (list_all(owner_user_id=...)) -- previously this
    returned every sustain in the DB regardless of who was logged in, so
    any authenticated user's picker showed every other account's (and
    every throwaway test account's) sustains alongside their own.
    """
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        raw = engine.list_all(owner_user_id=current_user.get("id"))
        return ok({"sustains": raw})
    except Exception as exc:
        logger.debug("SustainEngine.list_all failed: %s", exc)

    return ok({"sustains": []})


# ── 1b. GET /devui/templates ──────────────────────────────────────────────────

@router.get("/templates", summary="Available sustain spec templates for creation")
async def list_templates(current_user: dict = Depends(get_current_user)) -> dict:
    """
    List the sustain spec templates that can be instantiated. Powers the
    'create sustain' control in the UI.

    Merges two sources deliberately, not two separate surfaces: the disk
    templates (homestead, habitat) and the current user's own Slice 6
    user-created definitions (sustain_templates). Both instantiate() the
    exact same way — SustainEngine._load_spec() resolves either
    transparently — so they belong in one list, not a "built-in vs custom"
    split. Only the current user's own definitions are included: someone
    else's custom sustain isn't a template a stranger should be able to
    instantiate from the picker.
    """
    import json as _json
    from pathlib import Path as _Path

    specs_dir = _Path(__file__).resolve().parent.parent.parent / "sustains"
    templates: list[dict] = []
    for path in sorted(specs_dir.glob("*.json")):
        try:
            spec = _json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        # operatives may be a dict (homestead) or a list (vyyb, chama, …).
        ops = spec.get("operatives") or {}
        operative_names = list(ops.keys()) if isinstance(ops, dict) else list(ops)
        templates.append({
            "template_id":  path.stem,
            "display_name": spec.get("display_name") or path.stem.replace("_", " ").title(),
            "description":  spec.get("description", ""),
            "operatives":   operative_names,
            "parameters":   spec.get("parameters", []),
            "user_created": False,
        })

    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        for d in engine.list_definitions(owner_user_id=current_user.get("id")):
            templates.append({
                "template_id":  d["template_id"],
                "display_name": d["display_name"],
                "description":  d["description"],
                "operatives":   [],
                "parameters":   [{"name": "owner_ids", "type": "array", "required": True}],
                "user_created": True,
            })
    except Exception as exc:
        logger.debug("merging user-created definitions into template list failed: %s", exc)

    return ok({"templates": templates})


# ── 1c. POST /devui/sustains ──────────────────────────────────────────────────

class CreateSustainRequest(BaseModel):
    template_id: str = Field(description="Spec template to instantiate, e.g. 'homestead'")
    user_id: str = Field(default="owner", description="Owner user id for the new sustain")
    parameters: dict = Field(
        default_factory=dict,
        description="Spec parameters. owner_ids defaults to [user_id] when omitted.",
    )


@router.post("/sustains", summary="Create + hydrate a sustain from a template")
async def create_sustain(body: CreateSustainRequest, _: dict = Depends(get_current_user)) -> dict:
    """
    Instantiate a fully-hydrated sustain via SustainEngine — operators allowed,
    operatives enabled, and an initial state built from the template's
    default_state. Returns the new sustain_id plus the selector-shaped entry.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    params = dict(body.parameters or {})
    params.setdefault("owner_ids", [body.user_id])

    try:
        sustain_id = engine.instantiate(body.template_id, body.user_id, params)
    except ValueError as exc:
        # Unknown template spec or a required parameter is missing.
        raise HTTPException(status_code=422, detail=str(exc))

    entry = next(
        (s for s in engine.list_all() if s.get("id") == sustain_id),
        {"id": sustain_id, "template_id": body.template_id, "status": "live"},
    )
    logger.info("Created sustain %s from template %s", sustain_id, body.template_id)
    return ok({"sustain_id": sustain_id, "sustain": entry})


# ── 1d. Definitions — user-created sustain templates (Slice 6) ───────────────
# A "definition" is a sustain template a person builds through the DEFINE UI
# instead of hand-editing a sustena/sustains/*.json file. It persists in
# SustainEngine's sustain_templates table and instantiates through the exact
# same POST /devui/sustains + engine.instantiate() path as homestead/habitat
# — these routes only create/read/edit the DEFINITION itself.

class DimensionSpec(BaseModel):
    name: str
    type: str = Field(description="One of: number, string, boolean")
    description: str = ""
    default_value: Any = None
    minimum: float | None = None


class InvariantSpec(BaseModel):
    id: str
    expression: str
    description: str = ""
    authority: str = Field(
        default="advisory",
        description="'binding' or 'advisory' (default). Only matters for an invariant that "
                     "references a roll-up aggregate: 'advisory' invariants are display-only "
                     "and never block a child's transaction; 'binding' ones can refuse a "
                     "child action that would newly breach them. Every other invariant is "
                     "unaffected by this field either way.",
    )


class CreateDefinitionRequest(BaseModel):
    user_id: str = Field(description="Owner user id for the new definition")
    display_name: str
    description: str = ""
    dimensions: list[DimensionSpec] = Field(default_factory=list)
    invariants: list[InvariantSpec] = Field(default_factory=list)
    operator_names: list[str] = Field(
        default_factory=list,
        description="Names of EXISTING operators (from GET /devui/registry/operators) to attach.",
    )


class UpdateDefinitionRequest(BaseModel):
    user_id: str = Field(description="Must match the definition's owner.")
    display_name: str | None = None
    description: str | None = None
    dimensions: list[DimensionSpec] | None = None
    invariants: list[InvariantSpec] | None = None
    operator_names: list[str] | None = None


class ValidateInvariantRequest(BaseModel):
    expression: str
    state_schema: dict = Field(default_factory=dict)


@router.post("/validate-invariant", summary="Live-compile an invariant expression against a candidate schema")
async def validate_invariant(body: ValidateInvariantRequest, _: dict = Depends(get_current_user)) -> dict:
    """
    Wraps predicates.compile_invariant() so the DEFINE UI's invariant builder
    can check an expression as the person builds it — a field/operator/value
    picker generates the expression string, this confirms it actually parses
    and binds against the dimensions declared so far, before the person is
    allowed to add it.
    """
    from sustena.core.predicates import compile_invariant

    _, errors = compile_invariant(body.expression, body.state_schema)
    return ok({"valid": not errors, "errors": errors})


@router.get("/definitions", summary="List the current user's sustain definitions")
async def list_definitions_route(user_id: str = Query(...), _: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    return ok({"definitions": engine.list_definitions(owner_user_id=user_id)})


@router.get("/definitions/{template_id}", summary="Full spec detail for one definition")
async def get_definition_route(template_id: str, _: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    definition = engine.get_definition(template_id)
    if definition is None:
        raise HTTPException(status_code=404, detail="Definition not found")
    return ok(definition)


@router.post("/definitions", summary="Create a new user-defined sustain template")
async def create_definition_route(body: CreateDefinitionRequest, _: dict = Depends(get_current_user)) -> dict:
    """
    Builds and persists a brand-new sustain template from schema dimensions +
    invariants + attached operator names. Never silently drops an invalid
    dimension/invariant/operator — SustainEngine.create_definition() raises
    ValueError on the first bad one, surfaced here as 422 with the exact
    reason (a compile error names the offending invariant and expression).
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        result = engine.create_definition(
            owner_user_id=body.user_id,
            display_name=body.display_name,
            description=body.description,
            dimensions=[d.model_dump() for d in body.dimensions],
            invariants=[i.model_dump() for i in body.invariants],
            operator_names=body.operator_names,
        )
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc))

    logger.info("Created definition %s owner=%s", result["template_id"], body.user_id)
    return ok(result)


@router.patch("/definitions/{template_id}", summary="Edit a sustain definition safely")
async def update_definition_route(
    template_id: str, body: UpdateDefinitionRequest, _: dict = Depends(get_current_user),
) -> dict:
    """
    Edits a definition, gated by the migration predicate: SustainEngine.
    update_definition() refuses (returns status="refused", not an HTTP
    error) if any LIVE instance of this template would fail one of the
    candidate invariants. This is a normal, expected outcome — not an
    exception — so it comes back as HTTP 200 with the refusal spelled out
    in the body, the same convention an operator refusal already uses
    (POST /devui/console/execute never turns a gate refusal into an HTTP
    error either).

    Only fields present in the body are changed; omitted fields keep their
    current value.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    patch: dict = {}
    if body.display_name is not None:
        patch["display_name"] = body.display_name
    if body.description is not None:
        patch["description"] = body.description
    if body.dimensions is not None:
        patch["dimensions"] = [d.model_dump() for d in body.dimensions]
    if body.invariants is not None:
        patch["invariants"] = [i.model_dump() for i in body.invariants]
    if body.operator_names is not None:
        patch["operator_names"] = body.operator_names

    try:
        result = engine.update_definition(template_id, body.user_id, patch)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc))

    return ok(result)


# ── 1e. Composition ⊕ and roll-up ρ (Slice 8) ─────────────────────────────────
# Generic parent/child linking + computed aggregation. Nothing here knows the
# word "habitat" — a parent is any sustain with linked children; homestead is
# the first caller, not a special case of the machinery.

class LinkChildRequest(BaseModel):
    child_sustain_id: str
    slot: str | None = None
    member: str | None = None


class ProvisionChildrenRequest(BaseModel):
    user_id: str = Field(description="Owner for any newly-instantiated declared children.")


@router.get("/sustain/{sustain_id}/children", summary="List a parent's linked children")
async def list_children_route(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    """
    Every child currently linked under sustain_id, enriched with each
    child's spec display_name where resolvable (best-effort — a child
    whose spec can't be loaded still appears, just without a label).
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    links = engine.list_children(sustain_id)
    for link in links:
        spec = engine._get_spec(link["child_sustain_id"])
        link["display_name"] = spec.get("display_name") if spec else None
    return ok({"children": links})


@router.get("/sustain/{sustain_id}/parent", summary="The link record for a child's parent, or null (Phase 2 — nested holons)")
async def get_parent_route(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    """
    get_parent() already existed as an engine method with no route (flagged
    in the Phase 2 design doc as the one real gap for Orchie's breadcrumb
    "up" — everything else composition-navigation needs already had a
    route). Returns {"parent": null} for a sustain with no parent (most
    sustains), or {"parent": {...link, "display_name"}} — same
    best-effort display_name enrichment list_children_route already uses.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    link = engine.get_parent(sustain_id)
    if link is None:
        return ok({"parent": None})
    spec = engine._get_spec(link["parent_sustain_id"])
    link["display_name"] = spec.get("display_name") if spec else None
    return ok({"parent": link})


@router.post("/sustain/{sustain_id}/children", summary="Link an existing sustain as a child (⊕)")
async def link_child_route(
    sustain_id: str, body: LinkChildRequest, _: dict = Depends(get_current_user),
) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        link = engine.link_child(sustain_id, body.child_sustain_id, slot=body.slot, member=body.member)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc))
    return ok(link)


@router.delete("/sustain/{sustain_id}/children/{child_sustain_id}", summary="Unlink a child (disaggregation)")
async def unlink_child_route(
    sustain_id: str, child_sustain_id: str, _: dict = Depends(get_current_user),
) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    unlinked = engine.unlink_child(sustain_id, child_sustain_id)
    if not unlinked:
        raise HTTPException(status_code=404, detail="No such link.")
    return ok({"unlinked": True})


@router.post("/sustain/{sustain_id}/provision-children", summary="Instantiate + link every declared child")
async def provision_children_route(
    sustain_id: str, body: ProvisionChildrenRequest, _: dict = Depends(get_current_user),
) -> dict:
    """
    Reads sustain_id's spec["declared_children"] (a generic convention, not
    homestead-specific) and instantiates+links any slot not yet linked.
    Idempotent — safe to call again once every slot is filled.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        links = engine.provision_declared_children(sustain_id, body.user_id)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc))
    return ok({"children": links})


@router.get("/sustain/{sustain_id}/rollup", summary="Computed roll-up of a parent's declared aggregates")
async def get_rollup_route(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    """
    Computed fresh on every call from linked children's current state —
    never stored. A sustain with no spec["aggregates"] declared returns an
    empty aggregates dict, not an error (most sustains aren't parents).
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        rollup = engine.compute_rollup(sustain_id)
    except ValueError as exc:
        raise HTTPException(status_code=404, detail=str(exc))
    return ok(rollup)


# ── Advisory idle loop (Slice 10) ─────────────────────────────────────────────
# CORE PRINCIPLE: operatives ADVISE, the human DECIDES. evaluate-operatives
# only reads state and persists suggestion metadata — it can never mutate a
# sustain. accept is the ONLY route that changes real state, and it does so
# by calling the real execute_operator() path (same gate, same fold).

@router.post("/sustain/{sustain_id}/evaluate-operatives", summary="Run the idle-loop advisory pass")
async def evaluate_operatives_route(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    """
    Triggered pass (not a background scheduler — see CLAUDE.md's Slice 10
    notes on why): runs every rule bound to every operative this sustain
    declares against current state, returns the resulting pending
    suggestions. Never mutates the sustain's own state or event log.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        suggestions = engine.evaluate_operatives(sustain_id)
    except ValueError as exc:
        raise HTTPException(status_code=404, detail=str(exc))
    return ok({"suggestions": suggestions})


@router.get("/sustain/{sustain_id}/suggestions", summary="List suggestions for a sustain")
async def list_suggestions_route(
    sustain_id: str,
    status: str | None = Query(default="pending", description="pending|accepted|dismissed|expired, or omit for all"),
    _: dict = Depends(get_current_user),
) -> dict:
    """Read-only — never triggers evaluation, safe to poll cheaply."""
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    return ok({"suggestions": engine.get_suggestions(sustain_id, status=status)})


@router.post("/sustain/{sustain_id}/suggestions/{suggestion_id}/accept", summary="Accept a suggestion — runs the real operator")
async def accept_suggestion_route(
    sustain_id: str, suggestion_id: str, _: dict = Depends(get_current_user),
) -> dict:
    """
    Runs the suggestion's proposed operator for real through
    execute_operator() — same S2 gate, same S3 fold-append, same Slice 7
    parent-binding check as any other write. A gate refusal is returned as
    a normal 'failed' OperatorResult, not an HTTP error — the suggestion
    stays pending so the human can retry after fixing the condition, or
    dismiss it.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        outcome = await engine.accept_suggestion(suggestion_id)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc))
    return ok({
        "suggestion": outcome["suggestion"],
        "result": outcome["result"].to_response(),
    })


@router.post("/sustain/{sustain_id}/suggestions/{suggestion_id}/dismiss", summary="Dismiss a suggestion")
async def dismiss_suggestion_route(
    sustain_id: str, suggestion_id: str, _: dict = Depends(get_current_user),
) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    dismissed = engine.dismiss_suggestion(suggestion_id)
    if not dismissed:
        raise HTTPException(status_code=404, detail="Suggestion not found or not pending.")
    return ok({"dismissed": True})


async def _get_suggestions(sustain_id: str) -> list[dict]:
    """
    Read-only pending suggestions for /devui/state — same best-effort-
    empty-on-failure contract as the other needs-attention helpers. Never
    triggers evaluation (that's a separate, explicit POST); this is a plain
    SELECT so it's cheap to include on every state poll.
    """
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        return engine.get_suggestions(sustain_id, status="pending")
    except Exception as exc:
        logger.debug("_get_suggestions(%s) failed: %s", sustain_id, exc)
        return []


# ── Egress / outbox (Slice 11) ────────────────────────────────────────────────
# HARD SAFETY BOUNDARY: none of these routes can move money. prepare-summary
# only ever QUEUES a 'prepared' row (a real, gated, fold-recorded operator
# call — see egress.prepare_household_summary). confirm is the ONLY route
# that can trigger an actual send, and it only ever writes a local JSON file
# — there is no payment rail anywhere in this file or the engine methods it
# calls. Nothing here ever runs automatically.

class PrepareSummaryRequest(BaseModel):
    label: str = Field(default="", description="Optional label — lets a human prepare more than one distinct summary per day.")


@router.post("/sustain/{sustain_id}/egress/prepare-summary", summary="Prepare a household summary export (queues, does not send)")
async def prepare_summary_route(
    sustain_id: str, body: PrepareSummaryRequest, _: dict = Depends(get_current_user),
) -> dict:
    """
    Runs egress.prepare_household_summary through the real, gated
    execute_operator() path. Only ever QUEUES a 'prepared' outbox row —
    nothing leaves the system from this call. Idempotent: preparing the
    same label (or the same day, with no label) twice returns the same row.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    result = await engine.execute_operator(
        sustain_id, "egress.prepare_household_summary", {"label": body.label},
    )
    if not result.succeeded:
        return ok({"result": result.to_response(), "outbox": None})

    idempotency_key = result.data.get("idempotency_key")
    matches = [
        row for row in engine.list_egress(sustain_id, status=None)
        if row["idempotency_key"] == idempotency_key
    ]
    return ok({"result": result.to_response(), "outbox": matches[0] if matches else None})


@router.get("/sustain/{sustain_id}/egress", summary="List outbox entries for a sustain")
async def list_egress_route(
    sustain_id: str,
    status: str | None = Query(default=None, description="prepared|confirmed|sent|failed|cancelled, or omit for full history"),
    _: dict = Depends(get_current_user),
) -> dict:
    """Read-only — never triggers a send. Newest first."""
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    return ok({"egress": engine.list_egress(sustain_id, status=status)})


@router.post("/sustain/{sustain_id}/egress/{outbox_id}/confirm", summary="Confirm an outbox entry — the ONLY route that can send")
async def confirm_egress_route(
    sustain_id: str, outbox_id: str, _: dict = Depends(get_current_user),
) -> dict:
    """
    The explicit human confirm step. Also serves as retry for a 'failed'
    entry. Confirming an already-'sent' entry is an idempotent no-op — the
    response says so, nothing is re-sent. A send failure is reported as a
    normal 200 body with status='failed' and a reason, never an HTTP error.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        outcome = await engine.confirm_egress(outbox_id)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=str(exc))
    return ok(outcome)


@router.post("/sustain/{sustain_id}/egress/{outbox_id}/cancel", summary="Cancel a prepared or failed outbox entry")
async def cancel_egress_route(
    sustain_id: str, outbox_id: str, _: dict = Depends(get_current_user),
) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    cancelled = engine.cancel_egress(outbox_id)
    if not cancelled:
        raise HTTPException(status_code=404, detail="Outbox entry not found or already sent/cancelled.")
    return ok({"cancelled": True})


async def _get_egress(sustain_id: str) -> list[dict]:
    """
    Read-only outbox history for /devui/state — same best-effort-empty-on-
    failure contract as the other needs-attention helpers. Never triggers
    a send. Returns full history (usually short) so the Outbox card can
    show sent/failed alongside prepared without a second poll.
    """
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        return engine.list_egress(sustain_id, status=None)
    except Exception as exc:
        logger.debug("_get_egress(%s) failed: %s", sustain_id, exc)
        return []


# ── Shared: visualize.* widget computation ───────────────────────────────────
# Used by both GET /devui/state and GET /devui/monitor-widgets so the Monitor
# panel's pocket ring / event feed / constraint health widgets are computed
# from the SAME state snapshot the rest of the panel reads — one computation,
# not two independently-fetched ones that can disagree at a given instant.

async def _get_proposals_in_voting(sustain_id: str) -> list[dict]:
    """
    Council proposals still awaiting a vote — a real, already-computable
    needs-attention signal (council_proposals already exists; this isn't
    new backend state, just a read the Monitor never surfaced before).
    Best-effort: returns [] on any failure rather than breaking /devui/state.
    """
    try:
        from sqlalchemy import text as _text
        from sustena.db.schema import get_engine as _get_sqlalchemy_engine

        db_engine = _get_sqlalchemy_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                _text(
                    "SELECT id, proposed_by, operator_name, created_at "
                    "FROM council_proposals WHERE sustain_id = :sid AND status = 'IN_VOTING' "
                    "ORDER BY created_at DESC"
                ),
                {"sid": sustain_id},
            )
            return [
                {
                    "id": r[0],
                    "proposed_by": r[1],
                    "operator_name": r[2],
                    "created_at": r[3].isoformat() if hasattr(r[3], "isoformat") else str(r[3]),
                }
                for r in rows
            ]
    except Exception as exc:
        logger.debug("_get_proposals_in_voting(%s) failed: %s", sustain_id, exc)
        return []


async def _get_ingest_attention(sustain_id: str) -> dict:
    """
    Ingest pipeline (Slice 4) needs-attention signal for the Monitor: unresolved
    needs_attention messages (unparsed/parsed-but-unmapped captures) and stale
    capture sources for this sustain. Same best-effort-empty-on-failure contract
    as _get_proposals_in_voting -- this must never break /devui/state.
    """
    try:
        from sustena.core.ingest_singleton import get_shared_ingest_engine

        ingest = get_shared_ingest_engine()
        messages = ingest.needs_attention(sustain_id=sustain_id)
        sources = [s for s in ingest.get_sources(sustain_id=sustain_id) if s["is_stale"]]
        return {"messages": messages, "stale_sources": sources}
    except Exception as exc:
        logger.debug("_get_ingest_attention(%s) failed: %s", sustain_id, exc)
        return {"messages": [], "stale_sources": []}


def _get_rollup(engine, sustain_id: str) -> dict | None:
    """
    Composition/roll-up (Slice 8) for the Monitor: computed fresh from
    linked children, best-effort. Returns None (not {}) for a sustain with
    no spec["aggregates"] declared — the overwhelming majority — so the
    frontend can cleanly distinguish "not a parent" from "parent, zero
    children yet." Never raises into /devui/state.
    """
    try:
        spec = engine._get_spec(sustain_id)
        if not spec or not spec.get("aggregates"):
            return None
        return engine.compute_rollup(sustain_id)
    except Exception as exc:
        logger.debug("_get_rollup(%s) failed: %s", sustain_id, exc)
        return None


async def _compute_monitor_widgets(
    sustain_id: str,
    state_dict: dict,
    constraint_exprs: list[str],
    real_events: list,
) -> dict[str, Any]:
    from sustena.core.events import EventBus
    from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext
    from sustena.core.pawa import PawaLedger
    from sustena.core.state import StateAccessor

    ctx = OperatorContext(
        state=StateAccessor(state_dict),
        events=EventBus(sustain_id=sustain_id),
        pawa=PawaLedger(),
        sustain_id=sustain_id,
        user_id="devui",
    )

    widgets: dict[str, Any] = {}

    for op_name, widget_key, extra_kwargs in [
        ("visualize.pocket_ring",       "pocket_ring",       {}),
        ("visualize.event_feed",        "event_feed",        {}),
        ("visualize.constraint_health", "constraint_health", {"constraints": constraint_exprs}),
    ]:
        try:
            result = await OPERATOR_REGISTRY[op_name].fn(ctx, sustain_id=sustain_id, **extra_kwargs)
            widgets[widget_key] = result.data
        except Exception as exc:
            logger.warning("monitor-widgets: %s failed: %s", op_name, exc)
            widgets[widget_key] = {"widget_type": op_name.split(".")[-1], "data": {}, "summary": str(exc)}

    # visualize.event_feed runs in a fresh context with no events; populate the
    # widget from the persisted events table so the Monitor feed + counter are real.
    widgets["event_feed"] = {
        "widget_type": "event_feed",
        "data": {
            "events": [
                {"event_name": e["event_name"], "timestamp": e["timestamp"], "payload": e["payload"]}
                for e in real_events
            ],
            "total": len(real_events),
            "sustain_id": sustain_id,
        },
        "summary": f"{len(real_events)} recent event(s)",
    }

    return widgets


# ── 2. GET /devui/state?sustain_id= ──────────────────────────────────────────

@router.get("/state", summary="Full current state for a sustain")
async def get_state(
    sustain_id: str = Query(..., description="e.g. homestead.bonnie"),
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Returns pockets, events feed, operative statuses, constraint health,
    proposals awaiting a vote, and the visualize.* Monitor widgets — all
    computed from one state snapshot so every card on the Monitor panel
    (including the needs-attention block) reflects the same instant.
    """
    from sustena.core.engine_singleton import get_shared_engine
    engine = get_shared_engine()

    state: dict = {}
    operatives: list = []
    constraints: list = []
    events: list = []

    try:
        state = engine.get_state(sustain_id)
        operatives = engine.get_operative_statuses(sustain_id)
        constraints = engine.evaluate_constraints(sustain_id)
    except ValueError:
        pass  # sustain not seeded yet — return empty payload
    except Exception as exc:
        logger.debug("get_state(%s) engine error: %s", sustain_id, exc)

    try:
        events = engine.get_events(sustain_id, limit=20)
    except Exception as exc:
        logger.debug("get_events(%s) failed: %s", sustain_id, exc)

    constraint_exprs = [c["expr"] for c in constraints]
    widgets = await _compute_monitor_widgets(sustain_id, state, constraint_exprs, events)
    proposals_in_voting = await _get_proposals_in_voting(sustain_id)
    ingest_attention = await _get_ingest_attention(sustain_id)
    rollup = _get_rollup(engine, sustain_id)
    suggestions = await _get_suggestions(sustain_id)
    egress = await _get_egress(sustain_id)

    return ok({
        "sustain_id": sustain_id,
        "state": state,
        "events": events,
        "operatives": operatives,
        "constraints": constraints,
        "widgets": widgets,
        "proposals_in_voting": proposals_in_voting,
        "ingest_attention": ingest_attention,
        "rollup": rollup,
        "suggestions": suggestions,
        "egress": egress,
    })


# ── 3. POST /devui/console/execute ────────────────────────────────────────────

@router.post("/console/execute", summary="Execute an operator from the dev console")
async def console_execute(
    body: ConsoleExecuteRequest,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Runs an operator through OperatorContext against live state.
    Returns the result delta + any emitted events.
    """
    operator_name = body.resolved_operator()
    params = body.resolved_params()
    logger.info(
        "DevUI console.execute: sustain=%s operator=%s params=%s",
        body.sustain_id, operator_name, params,
    )
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        result = await engine.execute_operator(
            sustain_id=body.sustain_id,
            operator_name=operator_name,
            params=params,
        )
        try:
            recent_events = engine.get_events(body.sustain_id, limit=10)
        except Exception:
            recent_events = []
        # Pawa meter (§4L) — the real ⟨compute, storage, pawa⟩ reading for
        # THIS execution, if it succeeded. None on refusal/failure (zero
        # real work happened, so nothing was metered) or if unavailable.
        meter = None
        if result.succeeded:
            try:
                meter = engine.get_last_pawa_meter(body.sustain_id, operator_name)
            except Exception as exc:
                logger.debug("get_last_pawa_meter failed: %s", exc)
        return ok({
            "result": result.to_response(),
            "sustain_id": body.sustain_id,
            "operator": operator_name,
            "events": recent_events,
            "meter": meter,
        })
    except Exception as exc:
        logger.warning("console_execute failed: %s", exc)
        return ok({
            "result": {
                "status": "ok",
                "data": {"delta": {}, "note": f"Stub execution: {exc}"},
            },
            "sustain_id": body.sustain_id,
            "operator": operator_name,
            "events": [],
            "note": "Stub — engine not ready",
        })


# ── 4. POST /devui/simulate ───────────────────────────────────────────────────

@router.post("/simulate", summary="Simulate a proposal against a forked state")
async def simulate(
    body: SimulateRequest,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Runs a sequence of operators against a forked copy of the sustain state
    (no DB writes). Returns projected state delta + per-step constraint results.

    Body: { sustain_id, proposal: [{operator, params}, ...] }
    """
    logger.info(
        "DevUI simulate: sustain=%s steps=%d",
        body.sustain_id, len(body.proposal),
    )
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        step_results = await engine.simulate(
            sustain_id=body.sustain_id,
            operator_sequence=body.proposal,
        )
        return ok({
            "sustain_id": body.sustain_id,
            "steps": [
                {
                    "operator":      r["operator"],
                    "params":        r["params"],
                    "result":        r["result"].to_response(),
                    "state_after":   r["state_after"],
                    "parent_rollup": r.get("parent_rollup"),
                    "pawa":            r.get("pawa", 0.0),
                    "cumulative_pawa": r.get("cumulative_pawa", 0.0),
                }
                for r in step_results
            ],
            "final_state": step_results[-1]["state_after"] if step_results else {},
            "final_parent_rollup": step_results[-1].get("parent_rollup") if step_results else None,
            # §4L — the whole branch's efficiency score: total metered pawa
            # if every step in this proposal were promoted for real.
            "total_pawa": step_results[-1].get("cumulative_pawa", 0.0) if step_results else 0.0,
        })
    except Exception as exc:
        logger.warning("simulate(%s) failed: %s", body.sustain_id, exc)
        stub_steps = []
        for step in body.proposal:
            stub_steps.append({
                "operator": step.get("operator", "?"),
                "params":   step.get("params", {}),
                "result": {
                    "status": "ok",
                    "data": {"note": "Stub simulation step"},
                },
                "state_after": {"_stub": True},
            })
        return ok({
            "sustain_id": body.sustain_id,
            "steps": stub_steps,
            "final_state": {"_stub": True, "note": f"Engine not ready: {exc}"},
        })


# ── 4b. POST /devui/sustain/{id}/promote-simulation ───────────────────────────

class PromoteSimulationRequest(BaseModel):
    steps: list[dict] = Field(
        default_factory=list,
        description="[{operator, params}, ...] — the exact sequence a branch in the "
                     "scenario tree represents, replayed for real.",
    )


@router.post("/sustain/{sustain_id}/promote-simulation", summary="Genuinely replay a simulated branch against live state")
async def promote_simulation_route(
    sustain_id: str, body: PromoteSimulationRequest, _: dict = Depends(get_current_user),
) -> dict:
    """
    "Promote this branch to reality" — NOT a shortcut that trusts the
    earlier simulation result. Every step is re-run for real through
    execute_operator() (same gate, real events, real fold-append) against
    CURRENT live state. If live state drifted since the simulation was
    built, a step that passed in simulation can honestly fail here — that
    outcome is reported plainly, never papered over. Stops at the first
    real failure; later steps are never attempted.
    """
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    step_results = await engine.promote_simulation(sustain_id, body.steps)
    all_succeeded = bool(step_results) and all(r["result"].succeeded for r in step_results)
    return ok({
        "sustain_id": sustain_id,
        "steps_attempted": len(step_results),
        "steps_requested": len(body.steps),
        "all_succeeded": all_succeeded,
        "steps": [
            {"operator": r["operator"], "params": r["params"], "result": r["result"].to_response()}
            for r in step_results
        ],
    })


# ── 5. WS /devui/state-stream ─────────────────────────────────────────────────

@router.websocket("/state-stream")
async def state_stream_query(
    websocket: WebSocket,
    sustain_id: str = Query(...),
):
    """
    WebSocket state stream (query-param form): /devui/state-stream?sustain_id=homestead.bonnie&token=...
    Sends a full state snapshot on connect, then pushes events as they arrive.
    Falls back to polling every 2s until EventBus subscription is wired.

    token= must be a valid user session JWT (same one used for REST calls) —
    browsers can't set a custom Authorization header on a WS handshake, so
    the token travels as a query param instead. Validated the same way as
    every other route, including the token_version / logout check.
    """
    token = websocket.query_params.get("token", "")
    if not await get_user_from_token(token):
        await websocket.close(code=status.WS_1008_POLICY_VIOLATION)
        return

    await websocket.accept()
    logger.info("WS state-stream opened for %s", sustain_id)

    try:
        while True:
            try:
                from sustena.core.engine_singleton import get_shared_engine
                engine = get_shared_engine()
                state = engine.get_state(sustain_id)
            except Exception:
                state = {}

            await websocket.send_json({
                "type": "state_snapshot",
                "sustain_id": sustain_id,
                "state": state,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            })
            await asyncio.sleep(2)

    except WebSocketDisconnect:
        logger.info("WS state-stream closed for %s", sustain_id)
    except Exception as exc:
        logger.error("WS state-stream error for %s: %s", sustain_id, exc)
        try:
            await websocket.close()
        except Exception:
            pass


# ── Legacy path-param routes (backwards compat) ───────────────────────────────

@router.get("/sustain/{sustain_id}/state", summary="Full state dict for a sustain (path-param form)")
async def get_sustain_state(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    """Delegates to the canonical query-param handler."""
    return await get_state(sustain_id=sustain_id, _=_)


@router.get("/sustain/{sustain_id}/events", summary="Paginated event log")
async def get_sustain_events(
    sustain_id: str,
    limit: int = 100,
    _: dict = Depends(get_current_user),
) -> dict:
    try:
        from sustena.core.events import EventBus
        bus = EventBus(sustain_id=sustain_id)
        events = await bus.get_history(limit=limit)
        return ok({"sustain_id": sustain_id, "events": events, "count": len(events)})
    except Exception as exc:
        logger.warning("EventBus.get_history(%s) failed: %s", sustain_id, exc)
    return ok({"sustain_id": sustain_id, "events": [], "count": 0})


@router.get("/sustain/{sustain_id}/proposals", summary="Council proposals")
async def get_sustain_proposals(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    try:
        from sustena.db.schema import get_engine
        from sqlalchemy import text as _text
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                _text(
                    "SELECT id, proposed_by, operator_name, status, created_at "
                    "FROM council_proposals WHERE sustain_id = :sid ORDER BY created_at DESC"
                ),
                {"sid": sustain_id},
            )
            proposals = [
                {
                    "id": r[0],
                    "proposed_by": r[1],
                    "operator_name": r[2],
                    "status": r[3],
                    "created_at": r[4].isoformat() if hasattr(r[4], "isoformat") else str(r[4]),
                }
                for r in rows
            ]
        return ok({"sustain_id": sustain_id, "proposals": proposals})
    except Exception as exc:
        logger.warning("get_sustain_proposals(%s) failed: %s", sustain_id, exc)
    return ok({"sustain_id": sustain_id, "proposals": []})


@router.get("/registry/operators", summary="List operator registry")
async def get_operator_registry(_: dict = Depends(get_current_user)) -> dict:
    try:
        from sustena.core.operator import OPERATOR_REGISTRY

        # measured_pawa (§4L, the Pawa meter) — the REAL average metered
        # pawa across every recorded run, alongside (not replacing) the
        # static author-declared pawa_cost. None when the operator has
        # never actually run — an honest "not yet measured", never a
        # fabricated number. Best-effort: this route must not break if
        # the meter table/engine is unavailable for any reason.
        measured: dict[str, dict] = {}
        try:
            from sustena.core.engine_singleton import get_shared_engine
            measured = get_shared_engine().get_all_operator_pawa_stats()
        except Exception as exc:
            logger.debug("pawa meter stats unavailable for registry: %s", exc)

        return ok({
            "operators": {
                name: {
                    "description":  meta.description,
                    "pawa_cost":    meta.pawa_cost,
                    "measured_pawa": measured.get(name),
                    "license_tier": meta.license_tier,
                    "author":       meta.author,
                    "constraints":  meta.constraints,
                    "side_effects": meta.side_effects,
                    "ui_schema":    meta.ui_schema,
                    "protocol":     meta.protocol,
                }
                for name, meta in OPERATOR_REGISTRY.items()
            }
        })
    except Exception as exc:
        logger.warning("OPERATOR_REGISTRY unavailable: %s", exc)
    return ok({
        "operators": {
            "budget.allocate":      {"description": "Allocate to pocket",        "pawa_cost": 0, "license_tier": "free", "protocol": "rpc"},
            "budget.record_income": {"description": "Record income receipt",     "pawa_cost": 0, "license_tier": "free", "protocol": "rpc"},
        },
        "note": "Stub registry",
    })


@router.get("/registry/operatives", summary="List operative classes")
async def get_operative_registry(_: dict = Depends(get_current_user)) -> dict:
    from sustena.core.sustain_engine import _OPERATIVE_MAP
    return ok({
        "operatives": list(_OPERATIVE_MAP.keys()),
    })


@router.websocket("/state-stream/{sustain_id}")
async def state_stream_path(websocket: WebSocket, sustain_id: str):
    """Legacy path-param WebSocket — delegates to the same stream logic."""
    token = websocket.query_params.get("token", "")
    if not await get_user_from_token(token):
        await websocket.close(code=status.WS_1008_POLICY_VIOLATION)
        return

    await websocket.accept()
    logger.info("WS state-stream (path) opened for %s", sustain_id)

    try:
        while True:
            try:
                from sustena.core.engine_singleton import get_shared_engine
                engine = get_shared_engine()
                state = engine.get_state(sustain_id)
            except Exception:
                state = {}

            await websocket.send_json({
                "type": "state_snapshot",
                "sustain_id": sustain_id,
                "state": state,
                "timestamp": datetime.now(timezone.utc).isoformat(),
            })
            await asyncio.sleep(2)

    except WebSocketDisconnect:
        logger.info("WS state-stream (path) closed for %s", sustain_id)
    except Exception as exc:
        logger.error("WS state-stream (path) error for %s: %s", sustain_id, exc)
        try:
            await websocket.close()
        except Exception:
            pass


# ── 6. GET /devui/widgets ─────────────────────────────────────────────────────

@router.get("/widgets", summary="List all registered widget types")
async def list_widgets(_: dict = Depends(get_current_user)) -> dict:
    """
    Returns all widget types registered in the WidgetTypeRegistry.
    Used by the UIParser Preview tab and the Mycelium Library.
    """
    from sustena.core.widget_registry import widget_registry
    return ok({"widgets": widget_registry.list_all()})


# ── 7. GET /devui/monitor-widgets ────────────────────────────────────────────

@router.get("/monitor-widgets", summary="Rendered visualize.* widgets for the Monitor Panel")
async def get_monitor_widgets(
    sustain_id: str = Query(default="homestead.bonnie"),
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Calls visualize.pocket_ring, visualize.event_feed, and
    visualize.constraint_health against the current sustain state and returns
    their ResponseWidget outputs.  Returns empty-state widgets when the engine
    is not initialised or the sustain has no state.

    Kept as a standalone endpoint for callers that only need the widgets (e.g.
    the shell's event-count heartbeat); GET /devui/state uses the same
    _compute_monitor_widgets() helper so the two never compute this differently.
    """
    state_dict: dict = {}
    constraint_exprs: list[str] = []
    real_events: list = []
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        state_dict = engine.get_state(sustain_id) or {}
        constraint_data = engine.evaluate_constraints(sustain_id)
        constraint_exprs = [c["expr"] for c in constraint_data]
        real_events = engine.get_events(sustain_id, limit=20)
    except Exception:
        pass

    widgets = await _compute_monitor_widgets(sustain_id, state_dict, constraint_exprs, real_events)
    return ok({"sustain_id": sustain_id, "widgets": widgets})


# ── 8. POST /devui/simulate-pipeline ─────────────────────────────────────────


@router.post("/simulate-pipeline", summary="Simulate via simulate.fork → run_path → score pipeline")
async def simulate_pipeline(
    body: SimulatePipelineRequest,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Chains the three simulate.* operators in sequence:
      1. simulate.fork     — snapshot live state into an in-memory fork
      2. simulate.run_path — execute proposal steps against the fork
      3. simulate.score    — score the fork's final state against goal_metric

    Returns fork_id, per-step results, and the final score.
    Falls back to stub state when the engine is not initialised.
    """
    from sustena.core.events import EventBus
    from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext
    from sustena.core.pawa import PawaLedger
    from sustena.core.state import StateAccessor

    state_dict: dict = {}
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        state_dict = engine.get_state(body.sustain_id) or {}
    except Exception:
        pass

    ctx = OperatorContext(
        state=StateAccessor(state_dict),
        events=EventBus(sustain_id=body.sustain_id),
        pawa=PawaLedger(),
        sustain_id=body.sustain_id,
        user_id="devui",
    )

    # Step 1: fork
    fork_result = await OPERATOR_REGISTRY["simulate.fork"].fn(ctx, sustain_id=body.sustain_id)
    if not fork_result.succeeded:
        return ok({
            "sustain_id": body.sustain_id,
            "error": fork_result.reason,
            "steps": [],
            "score": None,
        })
    fork_id: str = fork_result.data["fork_id"]

    # Step 2: run_path
    path_result = await OPERATOR_REGISTRY["simulate.run_path"].fn(
        ctx,
        fork_id=fork_id,
        operator_sequence=body.proposal,
    )

    steps = path_result.data.get("steps", []) if path_result.succeeded else []

    # Step 3: score
    score_result = await OPERATOR_REGISTRY["simulate.score"].fn(
        ctx,
        fork_id=fork_id,
        goal_metric=body.goal_metric,
    )

    score_data = score_result.data if score_result.succeeded else {}

    return ok({
        "sustain_id": body.sustain_id,
        "fork_id": fork_id,
        "goal_metric": body.goal_metric,
        "steps": steps,
        "steps_run": len(steps),
        "steps_succeeded": sum(1 for s in steps if s.get("status") == "ok"),
        "score": score_data.get("score"),
        "interpretation": score_data.get("interpretation"),
        "final_state": path_result.data.get("final_state", {}) if path_result.succeeded else {},
    })


# ── 10. POST /devui/preview-widget ───────────────────────────────────────────

@router.post("/preview-widget", summary="Render a widget from a spec JSON (dev console preview)")
async def preview_widget(
    body: PreviewWidgetRequest,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Accepts a ui_schema (or full operator spec) and optional mock state,
    returns a rendered ResponseWidget dict.

    Used by the dev console UI Preview tab (debounced 500ms POST on every edit).
    """
    from sustena.core.uiparser import UISchemaParser
    parser = UISchemaParser()

    ui_schema_dict = body.spec_json.get("ui_schema", body.spec_json)
    mock_inputs = body.mock_state.get("inputs", {})

    try:
        schema = parser.parse(ui_schema_dict)
    except ValueError as exc:
        raise HTTPException(status_code=422, detail=f"Invalid ui_schema: {exc}")

    resolved_fields = []
    for f in schema.fields:
        value = parser.resolve_source(f.source, mock_inputs, body.mock_state)
        resolved_fields.append({
            "label": f.label,
            "value": value,
            "display": f.display,
            "colour_rule": f.colour_rule,
        })

    widget = {
        "type": schema.widget_type,
        "data": {
            "fields": resolved_fields,
            "ctas": schema.ctas,
            **({"chart": schema.chart} if schema.chart else {}),
        },
        "summary": f"Preview: {schema.widget_type}",
    }

    return ok({"widget": widget})


# ── 11. GET /devui/sustain/{id}/graph ─────────────────────────────────────────

_OPERATIVE_ROLE_LABELS: dict[str, str] = {
    "mentor":    "Strategic advisor · finance",
    "protege":   "Learning · pattern recognition",
    "attache":   "Contacts & governance",
    "navigator": "Logistics & routing",
    "curator":   "Assets & procurement",
}


@router.get("/sustain/{sustain_id}/graph", summary="Operative council graph for a sustain")
async def get_sustain_graph(
    sustain_id: str,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Returns the operative council graph for a sustain — nodes (Orchie +
    council operatives + their sub-operatives) and edges (delegation
    relationships).  Used by the Orchie panel graph tree.

    Returns empty nodes/edges when the sustain is not found.
    """
    spec = None
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        spec = engine.get_spec(sustain_id)
    except Exception as exc:
        logger.debug("get_sustain_graph(%s) engine error: %s", sustain_id, exc)

    if spec is None:
        return ok({"sustain_id": sustain_id, "nodes": [], "edges": []})

    operatives_cfg = spec.get("operatives", {})
    if not isinstance(operatives_cfg, dict):
        operatives_cfg = {name: {} for name in operatives_cfg}

    nodes: list[dict] = [
        {
            "id":    "orchie",
            "label": "Orchie",
            "role":  "AI orchestrator",
            "type":  "orchie",
        }
    ]
    edges: list[dict] = []

    for name, cfg in operatives_cfg.items():
        if not isinstance(cfg, dict):
            cfg = {}
        nodes.append({
            "id":     name,
            "label":  name.capitalize(),
            "role":   _OPERATIVE_ROLE_LABELS.get(name, "operative"),
            "type":   "operative",
            "domain": cfg.get("domain", []),
        })
        edges.append({"from": "orchie", "to": name, "type": "council"})

        for sub_name in cfg.get("sub_operatives", {}):
            sub_id = f"{name}.{sub_name}"
            nodes.append({
                "id":     sub_id,
                "label":  sub_name.replace("_", " ").title(),
                "role":   "sub-operative",
                "type":   "sub_operative",
                "parent": name,
            })
            edges.append({"from": name, "to": sub_id, "type": "delegation"})

    return ok({
        "sustain_id":  sustain_id,
        "template_id": spec.get("id", ""),
        "nodes":       nodes,
        "edges":       edges,
    })


# ── 12. GET /devui/library ───────────────────────────────────────────────────

@router.get("/library", summary="Arena packages grouped by kind for the Library panel")
async def get_library(_: dict = Depends(get_current_user)) -> dict:
    """
    Returns all arena_packages grouped by kind (operative, operator, spore, widget).
    Used by the LibraryPanel in the DevUI. Falls back to empty groups when the
    arena_packages table is empty or unavailable.
    """
    import json as _json
    from sqlalchemy import text as _text
    from sustena.db.schema import get_engine

    groups: dict[str, list] = {"operative": [], "operator": [], "spore": [], "widget": []}
    try:
        db_engine = get_engine()
        async with db_engine.connect() as conn:
            rows = await conn.execute(
                _text(
                    "SELECT id, name, kind, author_id, trust_score, download_count, "
                    "pawa_cost, is_free, version, tags, description, created_at "
                    "FROM arena_packages ORDER BY trust_score DESC"
                )
            )
            for r in rows:
                kind = r[2]
                tags = []
                try:
                    if r[9]:
                        tags = _json.loads(r[9])
                except Exception:
                    pass
                pkg = {
                    "id":             r[0],
                    "name":           r[1],
                    "kind":           kind,
                    "author":         r[3] or "unknown",
                    "trust":          round((r[4] or 0) * 100),
                    "downloads":      r[5] or 0,
                    "pawa":           "free" if r[7] else r[6],
                    "version":        r[8] or "1.0.0",
                    "tags":           tags,
                    "desc":           r[10] or "",
                    "created_at":     r[11].isoformat() if hasattr(r[11], "isoformat") else str(r[11]),
                }
                if kind in groups:
                    groups[kind].append(pkg)
    except Exception as exc:
        logger.warning("get_library failed: %s", exc)

    return ok({
        "operatives": groups["operative"],
        "operators":  groups["operator"],
        "spores":     groups["spore"],
        "widgets":    groups["widget"],
        "total":      sum(len(v) for v in groups.values()),
    })


# ── 13. GET /devui/sustain/{id}/operators ─────────────────────────────────────

@router.get("/sustain/{sustain_id}/operators", summary="Operators allowed by a sustain spec")
async def get_sustain_operators(
    sustain_id: str,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Returns the operator list declared in the sustain's spec under the "operators"
    key.  Each entry is augmented with the protocol field from OPERATOR_REGISTRY
    when available.  Used by the SimulatorPanel operator-selector dropdown.
    Returns an empty list when the sustain is unknown or has no operators declared.
    """
    spec = None
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        spec = engine.get_spec(sustain_id)
    except Exception as exc:
        logger.debug("get_sustain_operators(%s) engine error: %s", sustain_id, exc)

    if spec is None:
        return ok({"sustain_id": sustain_id, "operators": []})

    spec_ops = spec.get("operators", [])

    try:
        from sustena.core.operator import OPERATOR_REGISTRY
        registry = OPERATOR_REGISTRY
    except Exception:
        registry = {}

    # measured_pawa (§4L Pawa meter) — real average metered pawa per
    # operator, alongside the static declared pawa_cost. Best-effort.
    measured: dict[str, dict] = {}
    try:
        from sustena.core.engine_singleton import get_shared_engine
        measured = get_shared_engine().get_all_operator_pawa_stats()
    except Exception as exc:
        logger.debug("pawa meter stats unavailable for sustain operators: %s", exc)

    result = []
    for op in spec_ops:
        name = op.get("name", "") if isinstance(op, dict) else str(op)
        if not name:
            continue
        meta = registry.get(name)
        result.append({
            "name": name,
            "description": op.get("description", meta.description if meta else ""),
            "params": op.get("params", []),
            "pawa_cost": op.get("pawa_cost", meta.pawa_cost if meta else 0),
            "measured_pawa": measured.get(name),
            "protocol": meta.protocol if meta else "rpc",
        })

    return ok({"sustain_id": sustain_id, "operators": result})


# ── 13b. Pawa meter (§4L) — read-only aggregates over real metered runs ───────
# The odometer's dashboard: per-operator average, per-sustain total,
# per-principal total. Every number here is a real SUM/AVG over
# pawa_meter_log rows written by execute_operator — never fabricated,
# never a static declared cost dressed up as a measurement.

@router.get("/pawa/operators", summary="Real measured pawa stats for every operator that has run")
async def get_pawa_operator_stats(_: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine
    engine = get_shared_engine()
    return ok({"operators": engine.get_all_operator_pawa_stats()})


@router.get("/pawa/operators/{operator_name:path}", summary="Real measured pawa stats for one operator")
async def get_pawa_operator_stat(operator_name: str, _: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine
    engine = get_shared_engine()
    stats = engine.get_operator_pawa_stats(operator_name)
    return ok({"operator_name": operator_name, "stats": stats})


@router.get("/pawa/me", summary="Real total metered pawa for the current authenticated principal")
async def get_pawa_my_total(current_user: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine
    engine = get_shared_engine()
    return ok(engine.get_principal_pawa_total(current_user["id"]))


@router.get("/sustain/{sustain_id}/pawa", summary="Real total metered pawa for one sustain")
async def get_sustain_pawa(sustain_id: str, _: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine
    engine = get_shared_engine()
    return ok(engine.get_sustain_pawa_total(sustain_id))


# ── 14. GET /devui/sustain/{id}/definition ────────────────────────────────────
# The Modeling Studio's Edit-inspect view and visual sustain graph both need
# the raw STRUCTURAL definition (dimensions, invariants, composition,
# curated widgets) — something no existing route returns whole. This is a
# thin passthrough over the already-built engine.get_spec(); it does not
# duplicate any engine logic, it only whitelists the JSON-safe declared
# fields out of the cached spec dict (the spec also carries
# "_compiled_invariants"/"_invariant_compile_errors", which hold live
# predicate AST objects attached by _compile_spec_invariants() and are not
# JSON-serializable — those are deliberately excluded here in favour of the
# raw declared "invariants" list every spec file already has).

@router.get("/sustain/{sustain_id}/definition", summary="Raw structural definition for the Studio's Edit-inspect view + sustain graph")
async def get_sustain_definition(
    sustain_id: str,
    _: dict = Depends(get_current_user),
) -> dict:
    """
    Returns the declared shape of a sustain's definition — state schema,
    invariants (the viable region V), declared operators/operatives,
    composition (declared_children + aggregates), and curated widgets —
    for any sustain, disk-templated (homestead, habitat) or user-defined
    (Slice 6), since both resolve through the same engine.get_spec().

    Honest empty: an unknown/unseeded sustain_id returns found=false with
    every list empty, not a 404 — the Studio can render "nothing to show
    yet" without a special-case error branch.
    """
    spec = None
    try:
        from sustena.core.engine_singleton import get_shared_engine
        engine = get_shared_engine()
        spec = engine.get_spec(sustain_id)
    except Exception as exc:
        logger.debug("get_sustain_definition(%s) engine error: %s", sustain_id, exc)

    if spec is None:
        return ok({
            "sustain_id": sustain_id,
            "found": False,
            "id": None,
            "display_name": None,
            "description": None,
            "version": None,
            "state_schema": {},
            "invariants": [],
            "enforcement": {},
            "operators": [],
            "operatives": {},
            "declared_children": [],
            "aggregates": [],
            "curated_widgets": [],
            "access_policy": {},
        })

    operatives_cfg = spec.get("operatives", {})
    if not isinstance(operatives_cfg, dict):
        operatives_cfg = {name: {} for name in operatives_cfg}

    return ok({
        "sustain_id": sustain_id,
        "found": True,
        "id": spec.get("id"),
        "display_name": spec.get("display_name"),
        "description": spec.get("description"),
        "version": spec.get("version"),
        "state_schema": spec.get("state_schema", {}),
        "invariants": spec.get("invariants", []),
        "enforcement": spec.get("enforcement", {}),
        "operators": spec.get("operators", []),
        "operatives": operatives_cfg,
        "declared_children": spec.get("declared_children", []),
        "aggregates": spec.get("aggregates", []),
        "curated_widgets": spec.get("curated_widgets", []),
        "access_policy": spec.get("access_policy", {}),
    })


# ── Parse rules as gated edits (Phase 3C, 2 Aug 2026) ─────────────────────────
# Canon: SPEC-parser-primitive-lift.addendum §4I.6. Rules are a per-SOURCE
# (mpesa/kcb), not per-sustain, asset -- these routes are auth-gated
# (any signed-in user) rather than sustain-ownership-scoped, matching this
# file's own existing precedent for global/shared resources (the
# composition routes above have the identical auth-only shape). A
# correction to "how M-Pesa airtime purchases parse" is shared connector
# infrastructure, not owned by any one sustain.

class ParseRuleBody(BaseModel):
    id: str
    source: str
    pattern: str
    extract: dict = Field(default_factory=dict)   # {field_name: {"type": ..., "group": ..., "value": ...}}
    status: str = "parsed_unmapped"
    operator: str | None = None
    params: dict = Field(default_factory=dict)
    flags: list[str] = Field(default_factory=list)
    reason_template: str | None = None
    examples: list[str] = Field(default_factory=list)


def _rule_body_to_parse_rule(body: "ParseRuleBody"):
    from sustena.core.parse_rule import FieldSpec, ParseRule
    extract = {k: FieldSpec(**v) for k, v in body.extract.items()}
    return ParseRule(
        id=body.id, source=body.source, version=1, pattern=body.pattern, extract=extract,
        status=body.status, operator=body.operator, params=body.params,
        flags=tuple(body.flags), reason_template=body.reason_template,
        trust="user_corrected", provenance="user", examples=tuple(body.examples),
    )


@router.get("/parse-rules", summary="Active parse-rule corrections + seed library for a source")
async def list_parse_rules_route(source: str, current_user: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    effective = engine.get_effective_parse_rules(source)
    overrides = {r.id for r in engine.list_active_parse_rule_overrides(source)}
    return ok({
        "source": source,
        "rules": [
            {"id": r.id, "status": r.status, "operator": r.operator, "trust": r.trust, "is_correction": r.id in overrides}
            for r in effective
        ],
    })


@router.get("/parse-rules/{rule_id}/history", summary="Full append-only edit history for one rule")
async def parse_rule_history_route(rule_id: str, current_user: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    return ok({"rule_id": rule_id, "history": engine.list_parse_rule_history(rule_id)})


@router.post("/parse-rules", summary="AddRule — introduce a new declared ParseRule")
async def add_parse_rule_route(body: ParseRuleBody, current_user: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    rule = _rule_body_to_parse_rule(body)
    result = engine.add_parse_rule(body.source, rule, author_user_id=current_user["id"])
    return ok(result)


@router.post("/parse-rules/{rule_id}/modify", summary="ModifyRule — change an existing rule, gated by a regression check")
async def modify_parse_rule_route(rule_id: str, body: ParseRuleBody, current_user: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    rule = _rule_body_to_parse_rule(body)
    result = engine.modify_parse_rule(rule_id, rule, author_user_id=current_user["id"])
    return ok(result)


@router.post("/parse-rules/{rule_id}/retire", summary="RetireRule — remove a rule from its connector's active set")
async def retire_parse_rule_route(rule_id: str, current_user: dict = Depends(get_current_user)) -> dict:
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    result = engine.retire_parse_rule(rule_id, author_user_id=current_user["id"])
    return ok(result)


class ProposeParseRuleRequest(BaseModel):
    source: str
    raw_text: str


@router.post("/parse-rules/propose", summary="Phase 3D — propose+verify a candidate rule for an unparsed message")
async def propose_parse_rule_route(body: ProposeParseRuleRequest, current_user: dict = Depends(get_current_user)) -> dict:
    """
    Honest scope (see parse_rule_proposer.py's own docstring in full): the
    verify-and-crystallise MACHINERY here is real and complete, but no
    generator is wired by default — this always reports
    "no_proposer_configured" today rather than fabricate a candidate.
    Wiring a genuine LLM-based generator (respecting this project's
    zero-real-API-spend-in-dev discipline) is disclosed follow-up work.
    """
    from sustena.core.engine_singleton import get_shared_engine
    from sustena.core.parse_rule_proposer import propose_and_verify

    engine = get_shared_engine()
    existing = engine.get_effective_parse_rules(body.source)
    proposal = propose_and_verify(body.raw_text, existing, generator=None)
    if proposal is None:
        return ok({"proposed": False, "reason": "no_proposer_configured"})
    return ok({
        "proposed": True,
        "status": proposal.status,
        "reasons": proposal.reasons,
        "candidate_id": proposal.candidate.id,
    })
