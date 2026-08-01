"""
sustena/core/curated_ui.py

The Curated UI engine — Orchie's compose(r) core. Implements §4H of the
upgrade spec (SUSTENA_UPGRADE_SPEC.md, translating the `10-curated-ui-*`
article pair into Sustena's real idioms — state=dict/StateAccessor,
EventBus dot-protocol events, @sustena_operator registry). This module is
read-only: nothing here calls execute_operator or writes to any table.

Formal pieces implemented (walking-skeleton subset — §§1-5 of the article;
disclosure FSMs and effect-first capture are later slices, not here):

  w = ⟨inputs, render, emits⟩   — WidgetSchema, declared per-sustain in the
                                   spec's "curated_widgets" block, typed
                                   against real state (inputs ⊆ dim(S)) and
                                   real operators (emits ⊆ T).
  β : (EventClass ∪ Unit) → 𝒫(W) — build_binding_table(). Event-first: a
                                   widget is looked up FROM what happened,
                                   not the other way around (today's
                                   ui_schema binds operator→widget, the
                                   inverted direction the article flags).
  compose(r), r = ⟨s, q, d⟩      — resolves a view PER REQUEST from current
                                   state + recent events. Never stored.
  score(w) = α·urgency + λ·relevance
                                   — urgency reuses monitor.jsx's existing
                                   Slice 0 `pct = spent/allocated` signal
                                   (the only real distance-to-viable-region
                                   metric this codebase has today — CUSUM/
                                   EWMA per §2 of the upgrade spec do not
                                   exist yet, so this module does not invent
                                   a second "important" notion, it reuses
                                   the one the Monitor already established).
  selection = 0/1 knapsack, K≈4  — knapsack_select(). Genuine DP, not a
                                   sort dressed up: a widget's presence on
                                   screen is provably the outcome of it
                                   outscoring what it displaced.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)

UNIT = "__unit__"  # sentinel β key for widgets not bound to any EventClass

# Same severity ladder monitor.jsx's computeNeedsAttention() already uses
# (Slice 0/Slice 2) — reused, not reinvented, per the upgrade spec's
# explicit instruction that urgency must not get a second notion.
URGENCY_INGEST_UNMAPPED = 1.1

# Salience weights (α·urgency + λ·relevance). Urgency dominates deliberately:
# a widget that's always "there" (high relevance, nothing wrong) must not be
# able to outrank a quiet, genuinely urgent one — the Flight 401 property
# the acceptance check proves.
ALPHA_URGENCY = 0.75
LAMBDA_RELEVANCE = 0.25

DEFAULT_BUDGET = 4  # K ≈ 4 chunks (Cowan)


# ── Widget schema ──────────────────────────────────────────────────────────


@dataclass
class WidgetSchema:
    id: str
    render: str
    inputs: list[str] = field(default_factory=list)
    emits: list[str] = field(default_factory=list)
    event_class: str | None = None   # None => Unit-bound
    unit: bool = False
    cost: int = 1
    description: str = ""

    @property
    def binding_key(self) -> str:
        return UNIT if (self.unit or not self.event_class) else self.event_class


def load_widget_schemas(spec: dict) -> list[WidgetSchema]:
    """Parse the sustain spec's declared `curated_widgets` block into typed
    WidgetSchema objects. Missing/empty block => no widgets (honest empty,
    not an error) — most sustains won't declare any yet."""
    out: list[WidgetSchema] = []
    for raw in spec.get("curated_widgets", []) or []:
        out.append(WidgetSchema(
            id=raw["id"],
            render=raw["render"],
            inputs=list(raw.get("inputs", [])),
            emits=list(raw.get("emits", [])),
            event_class=raw.get("event_class"),
            unit=bool(raw.get("unit", False)),
            cost=int(raw.get("cost", 1)),
            description=raw.get("description", ""),
        ))
    return out


# ── DSL typing: inputs ⊆ dim(S), emits ⊆ T ──────────────────────────────────


def _resolve_dot_path_against_schema(path: str, state_schema: dict) -> bool:
    """Whether a plain dot-path (with optional `*` wildcard segments, e.g.
    'finances.pockets.*.allocated') resolves against a declared state_schema.
    Mirrors predicates.py's own array/additionalProperties walk convention
    but stays self-contained here — widget inputs are plain paths, not full
    predicate-DSL expressions, so a separate small walker is honest rather
    than reaching into predicates.py's underscore-private internals."""
    node: dict | None = {"type": "object", "properties": state_schema}
    for segment in path.split("."):
        if node is None:
            return False
        if segment == "*":
            if node.get("type") == "array" and isinstance(node.get("items"), dict):
                node = node["items"]
            elif isinstance(node.get("additionalProperties"), dict):
                node = node["additionalProperties"]
            else:
                return False
        else:
            if node.get("type") != "object":
                return False
            props = node.get("properties", {})
            if segment not in props:
                return False
            node = props[segment]
    return node is not None


def validate_widget_schema(
    widget: WidgetSchema, state_schema: dict, operator_registry_names: set[str],
) -> list[str]:
    """Type-check one widget: inputs ⊆ dim(S), emits ⊆ T. Returns a list of
    error strings (empty = valid) — never raises, so a caller can report
    every problem at once rather than failing on the first."""
    errors: list[str] = []
    for path in widget.inputs:
        if not _resolve_dot_path_against_schema(path, state_schema):
            errors.append(f"widget '{widget.id}': input '{path}' does not exist in this sustain's state_schema")
    for op_name in widget.emits:
        if op_name not in operator_registry_names:
            errors.append(f"widget '{widget.id}': emits unregistered operator '{op_name}'")
    if not widget.event_class and not widget.unit:
        errors.append(f"widget '{widget.id}': must declare either event_class or unit=true (β key is EventClass ∪ Unit)")
    return errors


def validate_all_widget_schemas(spec: dict) -> list[str]:
    """Type-check every widget declared on a spec. Used at spec-load time,
    same non-fatal discipline as _compile_spec_invariants: a bad widget is
    reported, not silently dropped and not fatal to the sustain loading."""
    from sustena.core.operator import OPERATOR_REGISTRY

    state_schema = spec.get("state_schema", {})
    registry_names = set(OPERATOR_REGISTRY.keys())
    errors: list[str] = []
    for widget in load_widget_schemas(spec):
        errors.extend(validate_widget_schema(widget, state_schema, registry_names))
    return errors


# ── Binding table β : (EventClass ∪ Unit) → 𝒫(W) ────────────────────────────


def build_binding_table(widgets: list[WidgetSchema]) -> dict[str, list[WidgetSchema]]:
    """Event-first binding: keyed by EventClass (what happened) or the Unit
    sentinel (always-eligible widgets), never by operator name. This is the
    formal inverse of today's ui_schema, which is left completely untouched
    — β is additive, not a replacement for the operator→widget binding
    other panels already use."""
    beta: dict[str, list[WidgetSchema]] = {}
    for w in widgets:
        beta.setdefault(w.binding_key, []).append(w)
    return beta


# ── Urgency (distance-to-viable-region, reused) + relevance ────────────────


def urgency_for_pocket(pocket: dict) -> float:
    """Identical formula to monitor.jsx's Slice 0 urgency signal:
    pct = spent / allocated (0 when allocated is 0). Not a new metric —
    the same one already driving the Monitor's urgency sort and
    needs-attention ranking, so Orchie and Mycelium never disagree about
    what counts as urgent."""
    allocated = pocket.get("allocated") or 0
    spent = pocket.get("spent") or 0
    if allocated <= 0:
        return 0.0
    return spent / allocated


def relevance_for(widget: WidgetSchema, query: str | None) -> float:
    """Information scent: token overlap between the query and the widget's
    id/description. No query => neutral relevance (0.5) so urgency alone
    drives ranking, which is the common case (compose() called with no
    search-like intent, just 'what needs me right now')."""
    if not query:
        return 0.5
    q_tokens = {t for t in query.lower().replace("_", " ").split() if t}
    if not q_tokens:
        return 0.5
    haystack = f"{widget.id} {widget.description}".lower().replace("_", " ")
    hay_tokens = {t for t in haystack.split() if t}
    if not hay_tokens or not q_tokens:
        return 0.5
    overlap = len(q_tokens & hay_tokens)
    return min(1.0, 0.2 + 0.4 * overlap)


def salience(urgency: float, relevance: float) -> float:
    return ALPHA_URGENCY * urgency + LAMBDA_RELEVANCE * relevance


# ── Selection: 0/1 knapsack under attention budget K ────────────────────────


def knapsack_select(candidates: list[dict], budget: int = DEFAULT_BUDGET) -> tuple[list[dict], list[dict]]:
    """Real 0/1 knapsack DP over integer costs (Hick-Hyman's superlinear
    per-item cost is why cost isn't uniformly 1 — a widget with more inputs
    costs more attention chunks to parse, see WidgetSchema.cost). Value is
    the widget's salience score scaled to an integer for the DP table.
    Returns (selected, excluded) — both with their score, so the caller can
    show *why* something didn't make the cut, not just that it didn't.

    Each candidate dict must have: 'cost' (int >= 1) and 'score' (float).
    """
    n = len(candidates)
    if n == 0:
        return [], []
    scale = 1000
    values = [max(0, round(c["score"] * scale)) for c in candidates]
    weights = [max(1, int(c["cost"])) for c in candidates]

    # Standard 0/1 knapsack DP: dp[c] = best total value using capacity c.
    dp = [0] * (budget + 1)
    keep = [[False] * (budget + 1) for _ in range(n)]
    for i in range(n):
        w, v = weights[i], values[i]
        for c in range(budget, -1, -1):
            if w <= c and dp[c - w] + v > dp[c]:
                dp[c] = dp[c - w] + v
                keep[i][c] = True
    # Backtrack to find the selected set.
    selected_idx: set[int] = set()
    c = budget
    for i in range(n - 1, -1, -1):
        if keep[i][c]:
            selected_idx.add(i)
            c -= weights[i]

    selected = [candidates[i] for i in range(n) if i in selected_idx]
    excluded = [candidates[i] for i in range(n) if i not in selected_idx]
    selected.sort(key=lambda c: -c["score"])
    excluded.sort(key=lambda c: -c["score"])
    return selected, excluded


# ── Per-widget rendering (state → view fragment) ────────────────────────────
# Deliberately hand-written per `render` id rather than a generic dot-path
# binder: the article's `render` third is "pure (state → view fragment)",
# i.e. a function — WIDGET_RENDERERS *is* that function set. `inputs` above
# is the DSL's typed declaration of what a widget is ALLOWED to read; these
# functions are what actually reads it. Read-only: state is only ever
# passed in, never mutated here.


def _render_pocket_spent_watch(state: dict, ctx: dict) -> dict:
    pocket_name = ctx.get("pocket_name")
    pockets = (state.get("finances") or {}).get("pockets") or {}
    pocket = pockets.get(pocket_name) or {}
    pct = urgency_for_pocket(pocket)
    return {
        "pocket_name": pocket_name,
        "allocated": pocket.get("allocated", 0),
        "spent": pocket.get("spent", 0),
        "remaining": (pocket.get("allocated", 0) or 0) - (pocket.get("spent", 0) or 0),
        "pct_spent": round(pct, 4),
        "headline": f"{pocket_name} is {round(pct * 100)}% spent" if pocket_name else "pocket spend recorded",
    }


def _render_unmapped_capture_classify(state: dict, ctx: dict) -> dict:
    message = ctx.get("message") or {}
    parsed_fields = message.get("parsed_fields") or {}
    amount = parsed_fields.get("amount")
    counterparty = parsed_fields.get("counterparty")
    # Event-first, in-the-moment framing per the field-classify design: state
    # the real amount/merchant already extracted by the transducer right in
    # the headline, so the person doesn't have to open the raw SMS to know
    # what they're being asked about. Falls back to the generic line only
    # when the transducer genuinely didn't recover either field (e.g. a
    # shape it half-recognised but couldn't extract from).
    if amount is not None and counterparty:
        headline = f"Ksh {amount:,.0f} to {counterparty} — which pocket?"
    elif amount is not None:
        headline = f"Ksh {amount:,.0f} needs a pocket"
    else:
        headline = "a capture couldn't be routed automatically — where does this go?"
    return {
        "message_id": message.get("message_id"),
        "source_id": message.get("source_id"),
        "parsed_fields": parsed_fields,
        "reason": message.get("reason"),
        "raw_payload": message.get("raw_payload"),
        "headline": headline,
    }


def _render_household_rollup_summary(state: dict, ctx: dict) -> dict:
    liquid = ((state.get("finances") or {}).get("liquid") or {}).get("balance", 0)
    rollup = ctx.get("rollup")
    out = {
        "liquid_balance": liquid,
        "headline": "household liquid balance",
    }
    if rollup and rollup.get("aggregates"):
        agg = next(iter(rollup["aggregates"].values()), None)
        if agg:
            out["household_total"] = agg.get("value")
            out["included_children"] = len(agg.get("included", []))
            out["excluded_children"] = len(agg.get("excluded", []))
    return out


WIDGET_RENDERERS = {
    "pocket_watch_card": _render_pocket_spent_watch,
    "classify_card": _render_unmapped_capture_classify,
    "rollup_summary_card": _render_household_rollup_summary,
}


# ── compose(r) ───────────────────────────────────────────────────────────


def _gather_candidates(
    engine, sustain_id: str, spec: dict, state: dict, beta: dict[str, list["WidgetSchema"]],
    query: str | None, recent_events: list[dict],
) -> list[dict]:
    """Gather (widget, context, urgency) triples from real, current signals.
    Every trigger here is something that ACTUALLY happened or IS currently
    true — no widget is fabricated a reason to appear."""
    candidates: list[dict] = []

    # Unit-bound widgets are always eligible; each supplies its own urgency.
    for widget in beta.get(UNIT, []):
        if widget.render == "classify_card":
            try:
                from sustena.core.ingest_singleton import get_shared_ingest_engine
                messages = get_shared_ingest_engine().needs_attention(sustain_id=sustain_id)
            except Exception as exc:  # pragma: no cover - defensive, never break compose()
                logger.debug("compose(): needs_attention lookup failed: %s", exc)
                messages = []
            for m in messages:
                candidates.append({
                    "widget": widget,
                    "ctx": {"message": m},
                    "urgency": URGENCY_INGEST_UNMAPPED,
                    "why": f"a capture from {m.get('source_id')} couldn't be auto-routed — {m.get('reason') or 'needs a human decision'}",
                })
        elif widget.render == "rollup_summary_card":
            rollup = None
            try:
                if spec.get("aggregates"):
                    rollup = engine.compute_rollup(sustain_id)
            except Exception as exc:  # pragma: no cover - defensive
                logger.debug("compose(): compute_rollup failed: %s", exc)
            candidates.append({
                "widget": widget,
                "ctx": {"rollup": rollup},
                "urgency": 0.1,  # baseline-informational: present, but rarely the reason you're here
                "why": "your household's baseline liquid position — always shown as a reference point",
            })
        else:  # pragma: no cover - forward-compat for a future unit-bound widget with no bespoke case yet
            candidates.append({"widget": widget, "ctx": {}, "urgency": 0.1, "why": "always eligible"})

    # Event-bound widgets: only eligible if their EventClass actually fired
    # recently — this IS the event-first property, not a database scan.
    for ev in recent_events:
        for widget in beta.get(ev["event_name"], []):
            payload = ev.get("payload") or {}
            pocket_name = payload.get("pocket")
            pocket = ((state.get("finances") or {}).get("pockets") or {}).get(pocket_name, {})
            pct = urgency_for_pocket(pocket)
            candidates.append({
                "widget": widget,
                "ctx": {"pocket_name": pocket_name},
                "urgency": pct,
                "why": f"{pocket_name} pocket was just spent from and is now {round(pct * 100)}% of its allocation — {ev['event_name']}",
            })

    return candidates


def compose(
    engine, sustain_id: str, query: str | None = None, device: str = "phone", budget: int = DEFAULT_BUDGET,
) -> dict:
    """r = ⟨state, query, device⟩ → a ranked, budget-limited view. Computed
    fresh on every call, nothing stored, nothing mutated. Reads only:
    engine.get_spec / get_state / get_events / compute_rollup, plus the
    ingest engine's needs_attention() — all pure reads."""
    spec = engine.get_spec(sustain_id)
    if spec is None:
        return {"sustain_id": sustain_id, "error": "sustain not found", "selected": [], "excluded": []}

    widgets = load_widget_schemas(spec)
    if not widgets:
        return {
            "sustain_id": sustain_id, "device": device, "query": query,
            "selected": [], "excluded": [],
            "message": "no curated widgets declared for this sustain yet",
        }

    beta = build_binding_table(widgets)
    state = engine.get_state(sustain_id)
    recent_events = engine.get_events(sustain_id, limit=25)

    raw_candidates = _gather_candidates(engine, sustain_id, spec, state, beta, query, recent_events)

    scored: list[dict] = []
    seen_ids: set[str] = set()
    for c in raw_candidates:
        widget: WidgetSchema = c["widget"]
        # A widget can fire more than once (e.g. two pockets spent recently)
        # -- keep only its highest-urgency instance so the same card doesn't
        # compete against itself for a knapsack slot.
        dedupe_key = f"{widget.id}:{c['ctx'].get('pocket_name') or c['ctx'].get('message', {}).get('message_id') or ''}"
        if dedupe_key in seen_ids:
            continue
        seen_ids.add(dedupe_key)

        relevance = relevance_for(widget, query)
        score = salience(c["urgency"], relevance)
        renderer = WIDGET_RENDERERS.get(widget.render)
        data = renderer(state, c["ctx"]) if renderer else {}
        scored.append({
            "id": widget.id,
            "render": widget.render,
            "emits": widget.emits,
            "cost": widget.cost,
            "urgency": round(c["urgency"], 4),
            "relevance": round(relevance, 4),
            "score": round(score, 4),
            "why": c["why"],
            "data": data,
        })

    selected, excluded = knapsack_select(scored, budget=budget)

    return {
        "sustain_id": sustain_id,
        "device": device,
        "query": query,
        "budget": budget,
        "candidates_considered": len(scored),
        "selected": selected,
        "excluded": [{"id": e["id"], "score": e["score"], "why_excluded": "outscored under the attention budget"} for e in excluded],
    }
