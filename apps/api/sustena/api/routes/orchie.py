"""
sustena/api/routes/orchie.py

Orchie chat endpoint — direct conversational interface to the Sustena AI operative.

POST /orchie/message
    Body:     { sustain_id: str, message: str, context?: dict }
    Response: { reply: str, proposals: list, actions: list, tone: str }

Routes through the same claude_client factory used everywhere else in the stack,
so mock mode works out of the box in development.
"""

import logging
from typing import Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from sustena.core.claude_client import get_claude_client
from sustena.api.routes.users import get_current_user

logger = logging.getLogger(__name__)

router = APIRouter()

# ── Request / Response schemas ────────────────────────────────────────────────


class OrchieMessageRequest(BaseModel):
    sustain_id: str
    message: str
    context: dict[str, Any] = {}


class OrchieMessageResponse(BaseModel):
    reply: str
    proposals: list = []
    actions: list = []
    tone: str = "neutral"


# ── System prompt ─────────────────────────────────────────────────────────────

ORCHIE_SYSTEM = """You are Orchie, the conversational AI operative for Sustena — a human-agent reality interface built around 7 primitives (state, operators, operatives, council, events, constraints, pawa).

Your role:
- Help the user understand and manage their sustain (their described system — household, business, community, etc.)
- Surface relevant insights: cash position, burn rate, pending council proposals, pantry alerts
- Propose concrete actions when thresholds are breached
- Be concise and direct — one or two sentences unless detail is asked for
- Use Swahili greetings when contextually appropriate (this is a Kenyan product)
- Amounts are in KSH unless otherwise specified
- Never use markdown formatting — respond in plain conversational text

You are NOT a generic assistant. Stay grounded in the user's sustain data and Sustena's operational model."""


# ── Route ─────────────────────────────────────────────────────────────────────


@router.post("/message", response_model=OrchieMessageResponse)
async def orchie_message(req: OrchieMessageRequest) -> OrchieMessageResponse:
    """
    Send a message to Orchie and receive a reply.

    Uses the claude_client factory — mock in development, real Anthropic API in production.
    """
    client = get_claude_client()

    system = f"{ORCHIE_SYSTEM}\n\nActive sustain: {req.sustain_id}"
    if req.context:
        import json
        system += f"\n\nContext snapshot:\n{json.dumps(req.context, indent=2)}"

    try:
        response = await client.messages.create(
            model="claude-haiku-4-5-20251001",
            max_tokens=512,
            system=system,
            messages=[{"role": "user", "content": req.message}],
        )
        reply_text = response.content[0].text if response.content else "Noted."
    except Exception as exc:
        logger.warning("Orchie LLM call failed: %s", exc)
        reply_text = "I hit a snag on my end — try again in a moment."

    logger.info(
        "orchie_message sustain=%s message=%r reply=%r",
        req.sustain_id,
        req.message[:60],
        reply_text[:60],
    )

    return OrchieMessageResponse(reply=reply_text)


# ── GET /orchie/compose — the Curated UI engine's compose(r) ──────────────────
#
# The rendering surface of ORCHIE (the phone-first, event-first curated
# surface — see SUSTENA_UPGRADE_SPEC.md §4H). Deliberately separate from
# every /devui/* route: those serve MYCELIUM, the orchestrator/dev cockpit,
# and are left untouched by this endpoint. compose(r) is read-only — it
# never calls execute_operator and never writes to any table.


def _assert_owns_sustain(sustain_id: str, user_id: str) -> None:
    """Same 404-for-both-cases pattern as ingest.py's _assert_owns_sustain —
    duplicated rather than imported across route files, matching this
    codebase's existing convention (each route file is self-contained)."""
    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    row = engine._db.execute(
        "SELECT user_id FROM sustains WHERE id = ?", (sustain_id,)
    ).fetchone()
    if row is None or row["user_id"] != user_id:
        raise HTTPException(status_code=404, detail="Sustain not found")


@router.get("/compose")
async def orchie_compose(
    sustain_id: str,
    query: str | None = None,
    device: str = "phone",
    budget: int = 4,
    current_user: dict = Depends(get_current_user),
) -> dict:
    """
    r = ⟨state, query, device⟩ → a ranked, attention-budgeted widget set.

    Read-only: computes a view fresh from current state + recent events
    every call. Nothing is stored, nothing is mutated -- proven by the
    acceptance check comparing state/events byte-for-byte before and after.
    """
    _assert_owns_sustain(sustain_id, current_user["id"])

    from sustena.core.engine_singleton import get_shared_engine
    from sustena.core.curated_ui import compose

    engine = get_shared_engine()
    try:
        result = compose(engine, sustain_id, query=query, device=device, budget=budget)
    except Exception as exc:
        logger.warning("orchie_compose(%s) failed: %s", sustain_id, exc)
        raise HTTPException(status_code=500, detail=f"compose failed: {exc}")

    return result


# ── POST /orchie/capture/infer + /confirm — effect-first capture (§7) ─────────
#
# Two separate steps, deliberately never collapsed into one:
#   infer()   — read-only. epsilon -> (o, theta) or a disambiguation question.
#               Never calls execute_operator, never mutates anything.
#   confirm() — the only place a capture can ever write. Calls the real,
#               unmodified execute_operator() -- the same S2 gate + S3 fold
#               every other operator call in this codebase goes through.
#               This split IS the article's "approval token" clause in
#               Sustena's real idiom: a proposal alone can never touch
#               state; only an explicit, separately authenticated confirm
#               call (the human tapping CONFIRM) can.


class CaptureInferRequest(BaseModel):
    sustain_id: str
    widget_id: str = "unmapped_capture_classify"
    message_id: str | None = None
    effect_text: str | None = None
    known: dict = {}
    ignore_history: bool = False


@router.post("/capture/infer")
async def capture_infer(
    body: CaptureInferRequest, current_user: dict = Depends(get_current_user),
) -> dict:
    """
    epsilon -> (o, theta), or one disambiguating question. Read-only.

    widget_id picks which declared widget's `emits` narrows the candidate
    operators (beta) -- the same widget schema from compose()'s own slice.
    message_id (optional) pulls a real, already-ingested-but-unmapped
    message's parsed_fields as additional signal; effect_text (optional)
    is a free narration. known carries the accumulated answers from any
    prior disambiguation round in this same capture session (this route
    is stateless -- the frontend re-sends known facts each call, same
    approach compose() itself uses for "nothing is stored between calls").

    Classification history ("purchase templates"): looks up this
    counterparty's most recent confirmed pocket (capture_classification_
    history, written by capture_confirm on a real success) and passes it to
    infer() as a pre-fill hint -- see infer()'s own docstring for why this
    only ever saves a disambiguation tap, never the CONFIRM tap. Pass
    ignore_history=true (the frontend's CHANGE affordance) to force the
    normal disambiguation question even when a history match exists.
    """
    _assert_owns_sustain(body.sustain_id, current_user["id"])

    from sustena.core.engine_singleton import get_shared_engine
    from sustena.core.curated_ui import load_widget_schemas
    from sustena.core.effect_capture import infer, resolve_description

    engine = get_shared_engine()
    spec = engine.get_spec(body.sustain_id)
    if spec is None:
        raise HTTPException(status_code=404, detail="Sustain not found")

    widgets = load_widget_schemas(spec)
    widget = next((w for w in widgets if w.id == body.widget_id), None)
    if widget is None:
        raise HTTPException(status_code=404, detail=f"No widget '{body.widget_id}' declared on this sustain")

    parsed_fields = None
    raw_text = None
    if body.message_id:
        from sustena.core.ingest_singleton import get_shared_ingest_engine

        message = get_shared_ingest_engine().get_message(body.message_id)
        if message is None or message.get("sustain_id") != body.sustain_id:
            raise HTTPException(status_code=404, detail="Message not found")
        parsed_fields = message.get("parsed_fields") or {}
        # The raw SMS body, only ever used as an amount-recovery FALLBACK
        # (infer()'s own currency-prefixed regex) for a capture no registered
        # transducer parser recognised -- see infer()'s docstring, step 1.
        raw_text = message.get("raw_payload")

    history = None
    if not body.ignore_history:
        description = resolve_description(body.effect_text, parsed_fields, body.known)
        if description:
            from sustena.core.ingest_singleton import get_shared_ingest_engine

            try:
                history = get_shared_ingest_engine().get_classification_history(body.sustain_id, description)
            except Exception as exc:  # pragma: no cover - defensive, never block a real capture on this
                logger.debug("capture_infer(): classification history lookup failed: %s", exc)
                history = None

    state = engine.get_state(body.sustain_id)
    result = infer(
        widget.emits, state,
        effect_text=body.effect_text, parsed_fields=parsed_fields, known=body.known, history=history,
        raw_text=raw_text,
    )
    return result.to_dict()


class CaptureConfirmRequest(BaseModel):
    sustain_id: str
    operator: str
    params: dict
    message_id: str | None = None
    description: str | None = None


@router.post("/capture/confirm")
async def capture_confirm(
    body: CaptureConfirmRequest, current_user: dict = Depends(get_current_user),
) -> dict:
    """
    The only write path for a capture. Routes the human-confirmed (o, theta)
    through the real, unmodified execute_operator() -- same S2 enforcing
    gate, same S3 fold-append as every other write in this codebase. A
    gate refusal is a normal 200 response with the real reason (same
    convention devui.py's console_execute already uses) -- never a fake
    success, never a silently swallowed failure.

    On genuine success, if this capture resolved a real ingest message,
    that message is marked resolved (acknowledgement only -- the message
    row itself never mutates state, execute_operator already did that).
    A gate refusal leaves the message in needs_attention, correctly --
    nothing was actually handled.

    `description` (optional, echoed back from the infer() result the
    frontend just confirmed -- see InferenceResult.description) is the
    counterparty/merchant text this classification is remembered against
    for next time (capture_classification_history). Recorded only on a
    genuine success and only best-effort -- see record_classification()'s
    own docstring for why a template-memory write can never be allowed to
    look like the real transaction failed.
    """
    _assert_owns_sustain(body.sustain_id, current_user["id"])

    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()

    # Whether body.operator is actually declared on this sustain's spec is
    # checked INSIDE execute_operator itself (the same allow-list check
    # every other write goes through) -- not duplicated here.
    result = await engine.execute_operator(body.sustain_id, body.operator, body.params)

    resolved = False
    if result.succeeded and body.message_id:
        from sustena.core.ingest_singleton import get_shared_ingest_engine

        resolved = get_shared_ingest_engine().resolve_message(body.message_id, resolved_by=current_user["id"])

    if result.succeeded and body.description and body.params.get("pocket_name"):
        from sustena.core.ingest_singleton import get_shared_ingest_engine

        try:
            get_shared_ingest_engine().record_classification(
                body.sustain_id, body.description, body.params.get("pocket_name"), body.operator,
            )
        except Exception as exc:  # pragma: no cover - defensive, never let this shadow a real success
            logger.debug("capture_confirm(): record_classification failed: %s", exc)

    return {
        "sustain_id": body.sustain_id,
        "operator": body.operator,
        "params": body.params,
        "result": result.to_response(),
        "message_resolved": resolved,
    }


# ── GET /orchie/capture/sources — allocate-recovery source picker data ────────
#
# Read-only, ownership-checked like every other Orchie route (never the
# laxer "any authenticated user" pattern devui.py's own /state route uses).
# Lists every OTHER pocket's real, current available (allocated - spent)
# balance plus the unallocated liquid balance -- exactly what a human needs
# to see before CHOOSING where a top-up should come from. Nothing here
# picks a source; the choice is made by a real tap in the frontend, then
# routed through budget.allocate (liquid source) or budget.transfer
# (another pocket) -- the same gated operators every other write uses.


@router.get("/capture/sources")
async def capture_sources(
    sustain_id: str, current_user: dict = Depends(get_current_user),
) -> dict:
    """
    Candidate ALLOCATION SOURCES for the refused-spend recovery loop's
    source picker: every pocket's name + allocated/spent/available, plus
    the unallocated liquid balance. A plain read off the current, already-
    folded state -- no aggregation, no recommendation, no default.
    """
    _assert_owns_sustain(sustain_id, current_user["id"])

    from sustena.core.engine_singleton import get_shared_engine

    engine = get_shared_engine()
    try:
        state = engine.get_state(sustain_id)
    except Exception as exc:
        logger.debug("capture_sources(%s): get_state failed: %s", sustain_id, exc)
        state = {}

    finances = state.get("finances") or {}
    pockets_raw = finances.get("pockets") or {}
    pockets = []
    for name, p in pockets_raw.items():
        if not isinstance(p, dict):
            continue
        allocated = p.get("allocated", 0.0) or 0.0
        spent = p.get("spent", 0.0) or 0.0
        pockets.append({
            "name": name,
            "allocated": allocated,
            "spent": spent,
            "available": allocated - spent,
        })

    liquid_balance = (finances.get("liquid") or {}).get("balance", 0.0) or 0.0

    return {"pockets": pockets, "liquid_balance": liquid_balance}
