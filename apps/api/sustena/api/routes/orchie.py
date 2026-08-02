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

    # Race guard (adversarial verify, 2 Aug 2026): a human can mark a
    # message "not a transaction" (mark_not_a_transaction) at roughly the
    # same moment a stale UI still lets them tap CONFIRM on the same
    # message's disambiguation flow. execute_operator() itself has no
    # concept of ingest_messages.status -- without this check, a message a
    # human just told the system "isn't a transaction" could still get
    # booked for real. Checked fresh here, right before the real write,
    # not cached from whatever state the frontend last saw.
    if body.message_id:
        from sustena.core.ingest_singleton import get_shared_ingest_engine as _get_ingest

        existing = _get_ingest().get_message(body.message_id)
        if existing is not None and existing.get("sustain_id") == body.sustain_id and existing.get("status") != "needs_attention":
            return {
                "sustain_id": body.sustain_id,
                "operator": body.operator,
                "params": body.params,
                "result": {
                    "status": "failed",
                    "data": None,
                    "reason": f"This message was already resolved (status: {existing.get('status')}) — nothing was booked.",
                },
                "message_resolved": False,
            }

    # Whether body.operator is actually declared on this sustain's spec is
    # checked INSIDE execute_operator itself (the same allow-list check
    # every other write goes through) -- not duplicated here.
    #
    # origin_message_id (when this confirm resolved a real captured
    # message) stamps the resulting event with a real, structural link
    # back to the raw SMS -- see execute_operator()'s own docstring. Omitted
    # entirely for a narration-only capture (no message_id), which is
    # exactly the honest "no raw message behind this" case the processed/
    # activity list surfaces as such, not a guessed link.
    result = await engine.execute_operator(
        body.sustain_id, body.operator, body.params, origin_message_id=body.message_id,
    )

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


# ── POST /orchie/capture/messages/{id}/not-a-transaction ───────────────────
#
# Bonnie, 2 Aug 2026: a real M-Pesa failure notice ("Failed. The till
# number entered is incorrect...") reached the classify card asking "which
# pocket does this belong to?" -- the informational-system-message filter
# didn't (yet) recognise it. This is the human escape hatch: a real, typed,
# gated action (not a raw DB write from the frontend) that marks a captured
# message as genuinely not a transaction. Never calls execute_operator --
# there's nothing to book, only a status change, the same category as
# resolve_message().


class MarkNotATransactionRequest(BaseModel):
    sustain_id: str


@router.post("/capture/messages/{message_id}/not-a-transaction")
async def capture_mark_not_a_transaction(
    message_id: str, body: MarkNotATransactionRequest, current_user: dict = Depends(get_current_user),
) -> dict:
    """
    Marks a captured message STATUS_INFORMATIONAL -- kept in the message
    store for audit (never deleted), permanently removed from the classify
    queue (needs_attention()'s own status filter), never booked as a
    spend/income. See IngestEngine.mark_not_a_transaction()'s own
    docstring for why this reuses the informational status rather than
    inventing a new one.

    Ownership-checked like every other Orchie route. Returns
    {"message_id", "marked": bool} -- marked=False (not an error) means
    the message was already resolved/informational/applied/refused, or
    never existed for this sustain; the frontend treats that as "nothing
    left to do here" rather than a failure.
    """
    _assert_owns_sustain(body.sustain_id, current_user["id"])

    from sustena.core.ingest_singleton import get_shared_ingest_engine

    ingest = get_shared_ingest_engine()
    marked = ingest.mark_not_a_transaction(message_id, body.sustain_id, marked_by=current_user["id"])
    return {"message_id": message_id, "marked": marked}


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


# ── GET /orchie/activity — the processed/activity list ─────────────────────
#
# Bonnie, 2 Aug 2026: "there is no UI for the processed" -- Orchie only ever
# showed what still NEEDS a decision (classify cards) plus the rollup
# number; there was no way to see what had actually already been recorded.
# This is the first, deliberately lightweight cut of a fuller message-audit
# log (queue #2) -- amount/direction/pocket/description/date/source per
# transaction, plus access to the raw message behind it, newest first.

# The two event types this first cut treats as "a transaction" -- what a
# person means by "did money move in or out." allocate/transfer (money
# moving between the household's OWN pockets) is a different category with
# no merchant/direction in the everyday sense and is deliberately left out
# of this first cut, not silently conflated with a real transaction.
_ACTIVITY_EVENT_TYPES = {
    "event.finances.pocket_spent": "out",
    "event.finances.income_received": "in",
}


@router.get("/activity")
async def orchie_activity(
    sustain_id: str, limit: int = 50, current_user: dict = Depends(get_current_user),
) -> dict:
    """
    Every real, committed spend/income transaction for this sustain,
    newest first -- sourced directly from the real S3 event log (never
    from ingest_messages alone: a narrated or Console-entered transaction
    has no message row at all, so events are the one source guaranteed
    complete regardless of how a transaction was entered).

    Each entry includes source/raw_text ONLY when the underlying event
    carries a real origin_message_id (see execute_operator()'s own
    docstring for how that gets stamped on) -- a structural link, never a
    guessed join on amount/timestamp proximity. A narration-only or
    Console-entered transaction honestly has neither field, rather than a
    fabricated or best-guessed one.

    Read-only, ownership-checked like every other Orchie route.
    """
    _assert_owns_sustain(sustain_id, current_user["id"])

    from sustena.core.engine_singleton import get_shared_engine
    from sustena.core.ingest_singleton import get_shared_ingest_engine

    engine = get_shared_engine()
    # A generous, bounded raw pull -- filtered down to transaction events
    # below, then truncated to the requested `limit`. Reuses get_events()
    # (the real, already-tested, seq-ordered read every other consumer of
    # the event log uses) rather than standing up a second, parallel query.
    try:
        raw_events = engine.get_events(sustain_id, limit=max(limit * 4, 200))
    except Exception as exc:
        logger.debug("orchie_activity(%s): get_events failed: %s", sustain_id, exc)
        raw_events = []

    ingest = get_shared_ingest_engine()
    entries: list[dict] = []
    for ev in raw_events:
        direction = _ACTIVITY_EVENT_TYPES.get(ev.get("event_name"))
        if direction is None:
            continue
        payload = ev.get("payload") or {}
        entry = {
            "event_name": ev["event_name"],
            "direction": direction,
            "amount": payload.get("amount"),
            "pocket": payload.get("pocket"),
            "description": payload.get("description") or payload.get("source") or None,
            "date": ev.get("timestamp"),
            "message_id": None,
            "source": None,
            "raw_text": None,
        }
        msg_id = payload.get("origin_message_id")
        if msg_id:
            msg = ingest.get_message(msg_id)
            if msg and msg.get("sustain_id") == sustain_id:
                entry["message_id"] = msg_id
                entry["source"] = msg.get("source_id")
                entry["raw_text"] = msg.get("raw_payload")
        entries.append(entry)
        if len(entries) >= limit:
            break

    return {"sustain_id": sustain_id, "entries": entries}


# ── GET /orchie/balance-provenance — trace the liquid/household number ─────
#
# Bonnie, 2 Aug 2026: "I still can't tell where the balance number came
# from... I want PROVENANCE." Every event that has EVER changed a sustain's
# own liquid balance, oldest first, summing to the displayed number -- plus
# a real consistency check (computed sum vs. the actual live balance).
#
# Verified by grep across the whole codebase, not assumed: exactly TWO
# operators ever write finances.liquid.balance --
#   budget.record_income  -- ctx.state.increment(...)  (+)
#   budget.allocate        -- ctx.state.decrement(...)  (-)
# budget.spend/transfer/add_pocket never touch liquid at all (spend draws
# from a pocket's own already-allocated balance; transfer moves allocation
# between two pockets; add_pocket moves nothing). So event.finances.
# income_received (+) and event.finances.pocket_allocated (-) are the
# COMPLETE set of liquid-affecting events -- nothing else could be missing.


def _liquid_provenance_for_one_sustain(engine, ingest, sid: str, label: str | None) -> dict:
    """One sustain's own liquid provenance + consistency check. Never
    assumes the genesis/reset snapshot started at zero (true for every
    real spec's default_state today, but a migrated legacy sustain could
    honestly have started somewhere else) -- reads the real starting value
    off the genesis event's own replace_root mutation."""
    import json

    genesis_row = engine._db.execute(
        "SELECT mutations_json FROM events WHERE sustain_id = ? ORDER BY seq ASC LIMIT 1",
        (sid,),
    ).fetchone()
    starting_balance = 0.0
    if genesis_row and genesis_row["mutations_json"]:
        try:
            for m in json.loads(genesis_row["mutations_json"]):
                if m.get("op") == "replace_root":
                    starting_balance = (
                        ((m.get("value") or {}).get("finances") or {}).get("liquid") or {}
                    ).get("balance", 0.0) or 0.0
        except (ValueError, TypeError, AttributeError):
            pass  # malformed/legacy mutation row -- honestly falls back to 0.0, not a crash

    raw_events = engine.get_events(sid, limit=1000)  # newest first
    entries: list[dict] = []
    running = starting_balance
    for ev in reversed(raw_events):  # oldest first, so running_balance reads top-to-bottom
        payload = ev.get("payload") or {}
        name = ev.get("event_name")
        if name == "event.finances.income_received":
            amount = payload.get("amount") or 0.0
            signed = amount
        elif name == "event.finances.pocket_allocated":
            amount = payload.get("amount") or 0.0
            signed = -amount
        else:
            continue
        running += signed
        entry = {
            "sustain_id": sid, "label": label, "event_name": name, "amount": signed,
            "date": ev.get("timestamp"),
            "description": payload.get("description") or payload.get("source") or payload.get("pocket"),
            "running_balance": round(running, 2),
            "message_id": None, "source": None, "raw_text": None,
        }
        msg_id = payload.get("origin_message_id")
        if msg_id:
            msg = ingest.get_message(msg_id)
            if msg and msg.get("sustain_id") == sid:
                entry["message_id"] = msg_id
                entry["source"] = msg.get("source_id")
                entry["raw_text"] = msg.get("raw_payload")
        entries.append(entry)

    computed_total = round(running, 2)
    try:
        live_state = engine.get_state(sid)
    except Exception:
        live_state = {}
    live_balance = round(
        ((live_state.get("finances") or {}).get("liquid") or {}).get("balance", 0.0) or 0.0, 2,
    )
    return {
        "sustain_id": sid,
        "label": label,
        "starting_balance": round(starting_balance, 2),
        "entries": entries,
        "computed_total": computed_total,
        "live_balance": live_balance,
        "consistent": abs(computed_total - live_balance) < 0.01,
    }


@router.get("/balance-provenance")
async def orchie_balance_provenance(
    sustain_id: str, current_user: dict = Depends(get_current_user),
) -> dict:
    """
    Full provenance of the displayed 'liquid' and 'household' figures --
    the exact events that produced them, oldest first, summing to the
    displayed number. Real consistency check included per sustain
    (computed sum of contributing events vs. the actual live balance) --
    a mismatch means `consistent: false` with both numbers shown, never
    silently reconciled or hidden.

    'liquid' = this sustain's own provenance (`own` below).
    'household' = the sum of every currently linked child's own liquid
    balance, per child (`children` below) -- the exact same definition
    compute_rollup() uses, cross-checked here (`household_consistent`)
    rather than re-derived independently, so a real divergence between
    the rollup engine and this trace would itself be caught, not masked
    by two different code paths quietly agreeing to be wrong the same way.

    Read-only, ownership-checked like every other Orchie route.
    """
    _assert_owns_sustain(sustain_id, current_user["id"])

    from sustena.core.engine_singleton import get_shared_engine
    from sustena.core.ingest_singleton import get_shared_ingest_engine

    engine = get_shared_engine()
    ingest = get_shared_ingest_engine()

    own = _liquid_provenance_for_one_sustain(engine, ingest, sustain_id, label=None)

    children_out = []
    try:
        links = engine.list_children(sustain_id)
    except Exception:
        links = []
    for link in links:
        child_id = link.get("child_sustain_id")
        if not child_id:
            continue
        children_out.append(
            _liquid_provenance_for_one_sustain(engine, ingest, child_id, label=link.get("member") or child_id)
        )

    household_total_from_children = round(sum(c["live_balance"] for c in children_out), 2)
    try:
        rollup = engine.compute_rollup(sustain_id) if links else None
    except Exception:
        rollup = None
    rollup_total = None
    if rollup and rollup.get("aggregates"):
        agg = next(iter(rollup["aggregates"].values()), None)
        if agg:
            rollup_total = round(agg.get("value") or 0.0, 2)

    return {
        "sustain_id": sustain_id,
        "own": own,
        "children": children_out,
        "household_total_computed": household_total_from_children,
        "household_total_via_rollup_engine": rollup_total,
        "household_consistent": (
            own["consistent"]
            and all(c["consistent"] for c in children_out)
            and (rollup_total is None or abs(rollup_total - household_total_from_children) < 0.01)
        ),
    }
