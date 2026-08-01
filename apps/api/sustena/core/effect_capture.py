"""
sustena/core/effect_capture.py

Effect-first capture — §7 of the Curated UI engine (SUSTENA_UPGRADE_SPEC.md
§4H). The user narrates an effect epsilon ("that 280 to Jesse was
groceries", "spent 500 on WiFi") or taps a classification onto a real
ingested-but-unmapped event; this module solves the INFERENCE half:
find (o, theta) such that o is a candidate operator and effect(o, theta, s)
plausibly matches epsilon -- so the person speaks in plain effects and
never sees a pocket dot-path or an operator name they have to type.

Deliberately NOT a natural-language model. Same no-eval/no-magic
discipline as transducer.py and predicates.py: deterministic pattern
matching (amount regex, live-state pocket-name matching, a small
verb-to-operator keyword table) plus real introspection of each
candidate operator's actual required parameters via inspect.signature
against OPERATOR_REGISTRY -- the same pattern SustainEngine._build_spec_dict
already uses to describe an operator's params to the DEFINE UI.

This module is read-only: it only ever reads `state` and OPERATOR_REGISTRY
metadata. It never calls execute_operator and never mutates anything --
inference (this module) and admission (execute_operator's existing S2
gate + S3 fold) are two separate steps, exactly mirroring compose()'s own
read-only/mutate split from the previous slice. The caller (the
/orchie/capture/confirm route) is what actually calls execute_operator,
and only after the human has explicitly reviewed and confirmed the
inferred (o, theta) -- the "approval token" the article's admit() clause
formalizes is, in Sustena's real idiom, exactly that explicit, separately
authenticated confirm step: a proposal alone can never write.
"""

from __future__ import annotations

import inspect
import re
from dataclasses import dataclass, field
from typing import Any, Literal

# Keyword -> operator-name suffix. A narrated verb narrows which of a
# widget's declared `emits` candidates applies. Deliberately small and
# literal (no stemming/NLP) -- an unmatched verb just means "no hint",
# never a wrong guess.
_VERB_TO_SUFFIX: dict[str, str] = {
    "spent": "spend", "paid": "spend", "bought": "spend", "spend": "spend",
    "allocate": "allocate", "allocated": "allocate", "set aside": "allocate",
    "put into": "allocate", "moved": "allocate", "budget for": "allocate",
}

_AMOUNT_RE = re.compile(r"(\d[\d,]*(?:\.\d+)?)")

# Currency-prefixed amount -- deliberately NARROWER than _AMOUNT_RE above.
# Used specifically as a fallback against a message's RAW SMS text (not a
# short human narration), where a bare "any digit sequence" match is a real
# risk: a raw bank SMS routinely contains dates, reference codes, and phone
# number fragments that also contain digits, and _AMOUNT_RE would happily
# (and wrongly) match the first one of those it finds. Every real M-Pesa/
# KCB SMS this codebase has seen prefixes the actual transaction amount
# with "Ksh" or "KES" (see transducer.py's own regexes) -- anchoring on
# that prefix is what makes this safe to run against a full raw message.
_CURRENCY_AMOUNT_RE = re.compile(r"(?:Ksh|KES)\.?\s*([\d,]+(?:\.\d+)?)", re.IGNORECASE)

# Humanized labels for the small set of operators this slice's widgets can
# emit -- used only when asking "which action?" during disambiguation.
# Falls back to the operator's own OPERATOR_REGISTRY description when an
# operator isn't in this table, so an unlisted future operator still gets
# an honest label rather than a crash.
_OPERATOR_LABELS: dict[str, str] = {
    "budget.spend": "spend it",
    "budget.allocate": "set it aside",
}


def extract_amount(text: str) -> float | None:
    """Recover a plain KES amount from narrated text ('spent 500 on WiFi'
    -> 500.0). No currency-symbol handling beyond digits/commas/decimal --
    disclosed, narrow scope; a genuinely unparseable amount is a real
    'cannot infer', not a guess."""
    if not text:
        return None
    m = _AMOUNT_RE.search(text)
    if not m:
        return None
    try:
        return float(m.group(1).replace(",", ""))
    except ValueError:  # pragma: no cover - regex already constrains this
        return None


def extract_currency_amount(text: str) -> float | None:
    """Recover a KES amount from a currency-PREFIXED figure in raw,
    unstructured text (a full SMS body a registered transducer parser
    couldn't recognise the shape of) -- the fallback that lets an otherwise
    'unparsed' capture still classify with a real amount instead of
    dead-ending at 'no amount could be recovered'. See _CURRENCY_AMOUNT_RE's
    own comment for why this is intentionally stricter than extract_amount()."""
    if not text:
        return None
    m = _CURRENCY_AMOUNT_RE.search(text)
    if not m:
        return None
    try:
        return float(m.group(1).replace(",", ""))
    except ValueError:  # pragma: no cover - regex already constrains this
        return None


def resolve_description(effect_text: str | None, parsed_fields: dict | None, known: dict | None) -> str | None:
    """The same 'what is this' resolution infer() uses internally --
    extracted so a caller (the /orchie/capture/infer route) can compute the
    identical description BEFORE calling infer(), to look up classification
    history (a merchant/counterparty key) without duplicating this logic."""
    known = known or {}
    if "description" in known:
        return known["description"]
    parsed_fields = parsed_fields or {}
    desc = parsed_fields.get("counterparty") or parsed_fields.get("external_ref")
    if not desc and effect_text:
        desc = effect_text
    return desc


def match_pocket_names(text: str, state: dict) -> list[str]:
    """Which of the sustain's OWN real, currently-declared pockets are
    named in this text. This is the concrete mechanism behind 'theta is
    recovered from state, not typed by the user' -- the person just says
    a word that happens to already be one of their own pocket names."""
    if not text:
        return []
    pockets = ((state.get("finances") or {}).get("pockets")) or {}
    lowered = text.lower()
    return [name for name in pockets.keys() if name and name.lower() in lowered]


def narrow_by_verb(text: str, candidate_operators: list[str]) -> list[str]:
    """Which candidate operators a narrated verb points at, if any. Empty
    list means 'no hint' (caller should NOT treat that as elimination)."""
    if not text:
        return []
    lowered = text.lower()
    hinted_suffixes = {suffix for kw, suffix in _VERB_TO_SUFFIX.items() if kw in lowered}
    if not hinted_suffixes:
        return []
    return [op for op in candidate_operators if op.rsplit(".", 1)[-1] in hinted_suffixes]


def _operator_signature(operator_name: str) -> "inspect.Signature | None":
    from sustena.core.operator import OPERATOR_REGISTRY

    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None:
        return None
    try:
        return inspect.signature(meta.fn)
    except (TypeError, ValueError):  # pragma: no cover - defensive, no known operator hits this
        return None


def _param_names(operator_name: str) -> dict[str, inspect.Parameter]:
    sig = _operator_signature(operator_name)
    if sig is None:
        return {}
    return {name: p for name, p in sig.parameters.items() if name not in ("ctx", "self")}


def required_params_satisfiable(operator_name: str, facts: dict) -> bool:
    """Whether every REQUIRED (no-default) parameter of this operator is
    already present in facts -- the generic 'can I build theta yet?'
    check. Works for any operator whose param names line up with the
    canonical fact keys (pocket_name, amount, description, category,
    period, source, frequency); an operator with unusual param names
    simply can never be satisfied here, which is the honest outcome."""
    params = _param_names(operator_name)
    if not params and operator_name:
        return False
    for name, p in params.items():
        if p.default is inspect.Parameter.empty and name not in facts:
            return False
    return True


def missing_required_params(operator_name: str, facts: dict) -> list[str]:
    params = _param_names(operator_name)
    return [name for name, p in params.items() if p.default is inspect.Parameter.empty and name not in facts]


def build_params(operator_name: str, facts: dict) -> dict:
    """Pulls exactly the fields this operator's real signature declares
    out of facts -- never more, never invented. This is theta."""
    params = _param_names(operator_name)
    return {name: facts[name] for name in params if name in facts}


def _option(value: str, label: str | None = None) -> dict:
    """Every disambiguation option is {value, label} -- value is what the
    caller sends back in `known` on the next call (a real pocket name, or
    a real operator name), label is what a human taps. Uniform shape so
    the frontend never special-cases which field is being asked about."""
    return {"value": value, "label": label or value}


@dataclass
class InferenceResult:
    status: Literal["ready", "needs_disambiguation", "cannot_infer"]
    operator: str | None = None
    params: dict | None = None
    question: str | None = None
    field: str | None = None
    options: list[dict] | None = None
    why: str = ""
    description: str | None = None
    from_history: bool = False
    history_use_count: int | None = None

    def to_dict(self) -> dict:
        return {
            "status": self.status,
            "operator": self.operator,
            "params": self.params,
            "question": self.question,
            "field": self.field,
            "options": self.options,
            "why": self.why,
            "description": self.description,
            "from_history": self.from_history,
            "history_use_count": self.history_use_count,
        }


def infer(
    candidate_operators: list[str],
    state: dict,
    effect_text: str | None = None,
    parsed_fields: dict | None = None,
    known: dict | None = None,
    history: dict | None = None,
    raw_text: str | None = None,
) -> InferenceResult:
    """
    epsilon -> (o, theta). One deterministic pass:
      1. amount: known -> parsed_fields.amount -> regex on effect_text ->
         currency-prefixed regex on raw_text (the full, un-parsed SMS body,
         when this capture came from a message no registered transducer
         parser recognised -- see extract_currency_amount()'s own comment
         for why this is a separate, narrower regex than the effect_text
         one). This is what lets an "unparsed" capture (parsed_fields={})
         still recover a real amount instead of dead-ending.
      2. description: best-effort, never blocks inference
      3. narrow candidate operators by a verb hint in effect_text, or by
         parsed_fields.direction=='sent' (a real ingest signal: money has
         already left, so a 'spend' is a strictly better fit than an
         internal 'allocate' move) -- only when it narrows to >=1, a hint
         that eliminates every candidate is treated as no hint at all.
      4. pocket_name: known -> exactly one live-state match in the text ->
         else, if `history` names a pocket that still exists in this
         sustain's live state, pre-fill it (classification history / a
         "purchase template" -- the caller looked this up by the resolved
         description before calling infer(); see resolve_description()) ->
         else ask (needs_disambiguation), offering the sustain's own real
         pocket names, or (zero matches, no history) the same list with an
         honest 'couldn't find one' reason.
      5. amount still missing -> ask for it directly (needs_disambiguation,
         field="amount", no options -- the caller renders a free-text/
         numeric input rather than tap buttons, same generic answer(value)
         contract as every other field). Genuinely last-resort: the raw-text
         fallback in step 1 already resolves this for the overwhelming
         majority of real bank SMS, which always state the amount somewhere
         in the text. NEVER a dead end -- a capture with no auto-recoverable
         amount is still fully classifiable, just with one more real tap.
      6. more than one operator still viable -> ask which action (Hick's
         law: at most as many options as remain, typically 2).
      7. build theta from the operator's real signature; if a required
         field still isn't satisfiable -> cannot_infer naming it.

    A history-based pre-fill is never silent: the returned InferenceResult
    carries from_history=True + history_use_count, so the caller can label
    it ("usual: food, 3x before") and offer a CHANGE affordance -- it still
    stops at 'ready', requiring the same explicit CONFIRM tap every other
    capture requires. Nothing about the write path changes; this only ever
    saves the disambiguation TAP for a repeat counterparty, never the
    confirm.
    """
    facts: dict[str, Any] = dict(known or {})
    parsed_fields = parsed_fields or {}

    # A human-typed amount (the field="amount" disambiguation this function
    # itself can now ask for) arrives from a text input, so `known["amount"]`
    # may be a string -- normalise it here rather than trusting the wire
    # shape, same discipline transducer.py's own _to_float applies. An
    # unparseable typed value is treated as not-provided (re-asks) rather
    # than crashing later inside the real operator call.
    if isinstance(facts.get("amount"), str):
        try:
            facts["amount"] = float(facts["amount"].replace(",", ""))
        except ValueError:
            del facts["amount"]

    if "amount" not in facts:
        amt = parsed_fields.get("amount")
        if amt is None:
            amt = extract_amount(effect_text or "")
        if amt is None:
            amt = extract_currency_amount(raw_text or "")
        if amt is not None:
            facts["amount"] = amt

    if "description" not in facts:
        desc = resolve_description(effect_text, parsed_fields, known)
        if desc:
            facts["description"] = desc

    from_history = False
    history_use_count: int | None = None

    ops = list(candidate_operators)
    chosen_operator = facts.pop("operator", None)
    if chosen_operator is not None:
        # The human already answered a prior "what should this do?" round
        # -- their choice wins outright, no further narrowing needed.
        ops = [chosen_operator] if chosen_operator in candidate_operators else []
    elif len(ops) > 1:
        hinted = narrow_by_verb(effect_text or "", ops)
        if not hinted and parsed_fields.get("direction") == "sent":
            spend_ops = [op for op in ops if op.rsplit(".", 1)[-1] == "spend"]
            if spend_ops:
                hinted = spend_ops
        if hinted:
            ops = hinted

    if "pocket_name" not in facts:
        search_text = " ".join(filter(None, [effect_text, facts.get("description")]))
        matches = match_pocket_names(search_text, state)
        live_pockets = ((state.get("finances") or {}).get("pockets")) or {}
        if len(matches) == 1:
            facts["pocket_name"] = matches[0]
        elif len(matches) == 0:
            hist_pocket = (history or {}).get("pocket_name")
            if hist_pocket and hist_pocket in live_pockets:
                facts["pocket_name"] = hist_pocket
                from_history = True
                history_use_count = (history or {}).get("use_count")
            else:
                options = sorted(live_pockets.keys())
                if not options:
                    return InferenceResult(status="cannot_infer", why="this sustain has no pockets declared yet to classify against")
                return InferenceResult(
                    status="needs_disambiguation", field="pocket_name",
                    question="which pocket does this belong to?", options=[_option(o) for o in options],
                    why="couldn't find a pocket name in what you described",
                )
        else:
            return InferenceResult(
                status="needs_disambiguation", field="pocket_name",
                question="which pocket did you mean?", options=[_option(o) for o in matches],
                why=f"more than one of your pockets matched: {', '.join(matches)}",
            )

    if "amount" not in facts:
        # Genuinely last-resort -- both the effect_text and the raw-message
        # currency-prefixed fallback already ran (step 1) and found nothing.
        # This is never a dead end: field="amount" with no options tells the
        # caller to render a real numeric input, not tap buttons -- the
        # human types the amount once, then the flow continues exactly like
        # any other resolved fact (same answer(value) -> re-infer contract).
        return InferenceResult(
            status="needs_disambiguation", field="amount",
            question="how much was this for?", options=None,
            why="couldn't find an amount in the message — enter it to continue",
        )

    if len(ops) > 1:
        satisfiable = [op for op in ops if required_params_satisfiable(op, facts)]
        if len(satisfiable) == 1:
            ops = satisfiable
        else:
            candidates = satisfiable or ops
            return InferenceResult(
                status="needs_disambiguation", field="operator",
                question="what should this do?",
                options=[_option(op, _OPERATOR_LABELS.get(op)) for op in candidates],
                why="more than one kind of action would fit this effect",
            )

    if not ops:
        return InferenceResult(status="cannot_infer", why="no candidate operator fits this effect")

    operator_name = ops[0]
    if not required_params_satisfiable(operator_name, facts):
        missing = missing_required_params(operator_name, facts)
        return InferenceResult(status="cannot_infer", why=f"missing required detail: {', '.join(missing)}")

    params = build_params(operator_name, facts)
    described = ", ".join(f"{k}={v}" for k, v in params.items())
    if from_history:
        count_phrase = f"{history_use_count}x before" if history_use_count else "before"
        why = f"usual pocket ({count_phrase}) — inferred {operator_name}({described})"
    else:
        why = f"inferred {operator_name}({described}) from what you described"
    return InferenceResult(
        status="ready", operator=operator_name, params=params, why=why,
        description=facts.get("description"),
        from_history=from_history, history_use_count=history_use_count if from_history else None,
    )
