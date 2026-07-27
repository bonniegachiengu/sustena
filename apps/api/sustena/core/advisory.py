"""
sustena/core/advisory.py

Advisory rule evaluation — Slice 10's "operatives advise, humans decide" layer.

CORE PRINCIPLE this module exists to serve: nothing in here can mutate
anything. Every function is pure — given a sustain's already-folded state
(and, for composition-aware rules, already-computed child metadata), it
returns Suggestion objects and nothing else. No DB access, no operator
calls, no eval()/exec(). SustainEngine (sustain_engine.py) is the only
thing that persists a Suggestion or acts on one, and "acts on one" always
means routing through the real execute_operator() path — the same S2 gate
and S3 fold every other write goes through. This module cannot bypass that
even by accident, because it has no access to anything that writes.

Generic by construction: every rule below reads only the generic
finances.{liquid,pockets} shape and generic composition metadata (child
sustain_id/member/days_since_last_event) that ANY sustain following those
conventions exposes — homestead AND habitat both do, and so would a future,
structurally-unlike sustain that happens to declare a `finances` block.
Nothing here references "homestead" or "habitat" by name.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable

# ── Suggestion ──────────────────────────────────────────────────────────────

@dataclass
class Suggestion:
    """
    A single piece of advice an operative is offering — never an action.

    dedupe_key identifies "this same underlying condition" across repeated
    evaluate() passes, coarsely bucketed (see each rule below) so a trivial
    fluctuation doesn't count as a new condition, but a meaningfully
    different one does. SustainEngine uses it to avoid re-surfacing a
    suggestion the human already dismissed or accepted for this exact
    bucketed condition, and to expire a pending suggestion whose condition
    no longer holds on a later pass.

    proposed_operator/proposed_params are optional — a suggestion may be
    purely informational (e.g. "member X has gone quiet") with nothing a
    single operator call could resolve.
    """

    operative_id: str
    rule_id: str
    title: str
    reason: str
    severity: str  # "info" | "warn" | "urgent"
    dedupe_key: str
    proposed_operator: str | None = None
    proposed_params: dict = field(default_factory=dict)


# ── Rule: Mentor — unallocated income sitting idle while a pocket strains ───

_URGENT_POCKET_PCT = 0.8  # same threshold Slice 2's needs-attention block uses


def _pocket_pct(pocket: dict) -> float:
    spent = pocket.get("spent", 0) or 0
    limit = pocket.get("limit", 0) or 0
    allocated = pocket.get("allocated", 0) or 0
    denom = limit if limit > 0 else allocated
    if denom <= 0:
        return 0.0
    return spent / denom


def rule_unallocated_income(state: dict) -> list[Suggestion]:
    """
    Fires when liquid balance is sitting > 0 while some pocket is under real
    strain (>= 80% of its limit/allocation spent) — the household has money
    that isn't doing anything while a real need exists. Proposes sweeping
    the FULL idle liquid balance into the most-strained pocket; a smaller
    or partial amount is left to the human to decide (this is advice, not
    a prescription of exactly how much).
    """
    finances = state.get("finances")
    if not isinstance(finances, dict):
        return []
    liquid_balance = (finances.get("liquid") or {}).get("balance", 0) or 0
    if liquid_balance <= 0:
        return []
    pockets = finances.get("pockets") or {}
    if not isinstance(pockets, dict) or not pockets:
        return []

    most_urgent_name, most_urgent_pct = None, 0.0
    for name, pocket in pockets.items():
        if not isinstance(pocket, dict):
            continue
        pct = _pocket_pct(pocket)
        if pct > most_urgent_pct:
            most_urgent_name, most_urgent_pct = name, pct

    if most_urgent_name is None or most_urgent_pct < _URGENT_POCKET_PCT:
        return []

    # Coarse bucket (nearest 500) so a trivial balance wiggle doesn't count
    # as a "new" condition once dismissed, but a real change in how much is
    # idle does.
    bucket = int(liquid_balance // 500) * 500
    return [Suggestion(
        operative_id="mentor",
        rule_id="unallocated_income",
        title=f"unallocated income while {most_urgent_name} strains",
        reason=(
            f"KES {liquid_balance:,.0f} is sitting unallocated in liquid balance while "
            f"'{most_urgent_name}' is {most_urgent_pct * 100:.0f}% spent. "
            f"Consider topping it up."
        ),
        severity="warn",
        dedupe_key=f"mentor:unallocated_income:{most_urgent_name}:{bucket}",
        proposed_operator="budget.allocate",
        proposed_params={"pocket_name": most_urgent_name, "amount": liquid_balance, "period": "monthly"},
    )]


# ── Rule: Attache — a linked member habitat has gone quiet ──────────────────

_STALE_DAYS_THRESHOLD = 7


def rule_child_stale(children_meta: list[dict]) -> list[Suggestion]:
    """
    Fires per linked child whose most recent event is >= 7 days old — a real,
    already-computed composition signal (days_since_last_event, supplied by
    the engine from get_events()), not a new state dimension. Purely
    informational: "who to check in on" isn't something a single operator
    call can resolve, so this rule never proposes one.
    """
    out: list[Suggestion] = []
    for child in children_meta:
        days = child.get("days_since_last_event")
        if days is None or days < _STALE_DAYS_THRESHOLD:
            continue
        member = child.get("member") or child.get("slot") or child.get("sustain_id", "")[:8]
        bucket = (int(days) // 7) * 7  # weekly buckets — resurfaces after another full week
        out.append(Suggestion(
            operative_id="attache",
            rule_id="child_stale",
            title=f"{member} has gone quiet",
            reason=f"{member} hasn't logged any activity in {int(days)} day(s).",
            severity="info",
            dedupe_key=f"attache:child_stale:{child.get('sustain_id')}:{bucket}",
        ))
    return out


# ── Rule: Mentor — a household summary export has gone stale ────────────────
#
# Demonstrates Slice 11's "an operative may SUGGEST an egress, but it's
# still human-confirmed" requirement. Accepting this suggestion runs
# egress.prepare_household_summary through the exact same real
# execute_operator() path any other accepted suggestion uses — same S2
# gate, same S3 fold. That call only ever QUEUES a 'prepared' outbox row;
# it never confirms or sends anything. Sending still requires a completely
# separate, explicit human confirm_egress() call. No suggestion — from this
# rule or any other — can ever reach the send step on its own.

_SUMMARY_EXPORT_STALE_DAYS = 7


def rule_summary_not_exported(last_sent_days: float | None) -> list[Suggestion]:
    """
    Fires when no household summary has ever been sent, or the last one
    was sent >= 7 days ago. last_sent_days is pre-computed by the engine
    from egress_outbox (MAX(sent_at) WHERE status='sent') — kept out of
    this pure module for the same reason days_since_last_event is passed
    into rule_child_stale rather than computed here.
    """
    if last_sent_days is not None and last_sent_days < _SUMMARY_EXPORT_STALE_DAYS:
        return []
    bucket = -1 if last_sent_days is None else (int(last_sent_days) // 7) * 7
    reason = (
        "No household summary has ever been exported."
        if last_sent_days is None
        else f"The last exported summary was sent {int(last_sent_days)} day(s) ago."
    )
    return [Suggestion(
        operative_id="mentor",
        rule_id="summary_not_exported",
        title="household summary hasn't been exported",
        reason=reason,
        severity="info",
        dedupe_key=f"mentor:summary_not_exported:{bucket}",
        proposed_operator="egress.prepare_household_summary",
        proposed_params={},
    )]


# ── Registry — operative_id -> rules bound to it ─────────────────────────────

ADVISORY_RULES: dict[str, list[Callable[..., list[Suggestion]]]] = {
    "mentor": [rule_unallocated_income, rule_summary_not_exported],
    "attache": [rule_child_stale],
}


def evaluate_operative(
    operative_id: str,
    state: dict,
    extra: dict | None = None,
) -> list[Suggestion]:
    """
    Run every rule bound to operative_id and return the combined
    suggestions. extra carries whatever engine-computed, non-state inputs
    specific rules need (children_meta for rule_child_stale,
    last_sent_days for rule_summary_not_exported) — this module stays
    DB-free, the engine supplies these.

    Unknown operative_id (or one with no bound rules) returns [] — not an
    error, since not every operative advises (e.g. Navigator/Curator/Protege
    have no rules yet; a future slice can add them the same way without
    touching anything here).
    """
    extra = extra or {}
    rules = ADVISORY_RULES.get(operative_id, [])
    out: list[Suggestion] = []
    for rule in rules:
        if rule is rule_child_stale:
            out.extend(rule(extra.get("children_meta") or []))
        elif rule is rule_summary_not_exported:
            out.extend(rule(extra.get("last_sent_days")))
        else:
            out.extend(rule(state))
    return out
