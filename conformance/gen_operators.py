"""
Operator-slice vector generation.

Drives the reference engine's execution contract — guard → effect → gate →
commit — and records what it decided.

The contract is reproduced here from the engine's own pieces (the declared
`constraints` on each `OperatorMeta`, the real operator bodies, and the same
`predicates` module the enforcement gate uses) rather than by calling
`SustainEngine.execute_operator`, which additionally persists to SQLite and
assigns event sequence numbers. Those are host concerns the portable core
deliberately does not have, so including them would make the vectors untestable
by the very thing they exist to test.

What IS recorded is the part both engines must agree on: whether each call was
admitted, which rule refused it, which events it emitted, and the resulting
state.
"""

from __future__ import annotations

import asyncio
import json

import sustena.operators  # noqa: F401  -- importing registers every operator
from sustena.core.constraints import ConstraintEngine, ConstraintParseError
from sustena.core.events import EventBus
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext
from sustena.core.pawa import PawaLedger
from sustena.core.predicates import compile_invariant, evaluate_predicate
from sustena.core.state import StateAccessor


def run_operator_case(initial: dict, allowed: list, enforcement: dict, calls: list) -> dict:
    state_dict = json.loads(json.dumps(initial))
    outcomes = []
    guards = ConstraintEngine()

    for call in calls:
        name, params = call["operator"], call["params"]
        acc = StateAccessor(state_dict)

        # The sustain's allow-list comes first.
        if name not in allowed:
            outcomes.append({"status": "failed", "constraint_violated": "operator_allowed"})
            continue

        meta = OPERATOR_REGISTRY.get(name)
        if meta is None:
            outcomes.append({"status": "failed", "constraint_violated": "operator_registered"})
            continue

        # ── guard ─────────────────────────────────────────────────────────────
        refused_by = None
        for guard in meta.constraints:
            try:
                ok, _reason = guards.evaluate(guard, acc, params)
            except ConstraintParseError:
                refused_by = "guard_unparseable"
                break
            if not ok:
                refused_by = guard
                break
        if refused_by is not None:
            outcomes.append({"status": "failed", "constraint_violated": refused_by})
            continue

        # ── effect ────────────────────────────────────────────────────────────
        bus = EventBus(sustain_id="conformance")
        ctx = OperatorContext(
            state=acc, events=bus, pawa=PawaLedger(),
            sustain_id="conformance", user_id="conformance-user",
        )
        result = asyncio.run(meta.fn(ctx, **params))

        if not result.succeeded:
            outcomes.append({
                "status": "failed",
                "constraint_violated": result.constraint_violated,
            })
            continue

        # ── gate: invariants against the MUTATED state, before commit ─────────
        candidate = acc.snapshot()
        gate_refusal = None
        if enforcement.get("enabled"):
            for inv in enforcement.get("invariants", []):
                node, errors = compile_invariant(inv["expression"], None)
                if errors:
                    gate_refusal = "invariant_unparseable"
                    break
                ok, _reason = evaluate_predicate(node, StateAccessor(candidate), params)
                if not ok:
                    gate_refusal = "enforcement_gate"
                    break
        if gate_refusal is None:
            for post in meta.post_constraints:
                node, errors = compile_invariant(post, None)
                if errors:
                    gate_refusal = "post_constraint_unparseable"
                    break
                ok, _reason = evaluate_predicate(node, StateAccessor(candidate), params)
                if not ok:
                    gate_refusal = "enforcement_gate"
                    break

        if gate_refusal is not None:
            # A refusal changes nothing: state_dict is deliberately not advanced.
            outcomes.append({"status": "failed", "constraint_violated": gate_refusal})
            continue

        # ── commit ────────────────────────────────────────────────────────────
        state_dict = candidate
        outcomes.append({
            "status": "ok",
            "constraint_violated": None,
            "events": [e["event_name"] for e in bus.published_this_context()],
        })

    return {"outcomes": outcomes, "final_state": state_dict}


HOMESTEAD_LIKE = {
    "finances": {
        "liquid": {"balance": 1000.0},
        "pockets": {},
        "income": {"monthly_total": 0.0, "sources": []},
    }
}

ALLOWED = ["budget.record_income", "budget.allocate", "budget.add_pocket", "budget.spend"]

ARMED = {
    "enabled": True,
    "invariants": [
        {"id": "liquid_non_negative", "expression": "finances.liquid.balance >= 0"},
        {"id": "pocket_allocated_non_negative",
         "expression": "ALL finances.pockets[*].allocated >= 0"},
    ],
}
DISARMED = {"enabled": False, "invariants": ARMED["invariants"]}


def _alloc(pocket, amount):
    return {"operator": "budget.allocate",
            "params": {"pocket_name": pocket, "amount": amount, "period": "monthly"}}


def _spend(pocket, amount):
    return {"operator": "budget.spend",
            "params": {"pocket_name": pocket, "amount": amount, "description": "x"}}


OPERATOR_CASES = [
    ("allocate_commits", HOMESTEAD_LIKE, ALLOWED, ARMED, [_alloc("food", 250.0)]),
    ("allocate_guard_refuses_overspend", HOMESTEAD_LIKE, ALLOWED, ARMED, [_alloc("food", 5000.0)]),
    ("allocate_guard_refuses_zero", HOMESTEAD_LIKE, ALLOWED, ARMED, [_alloc("food", 0.0)]),

    ("record_income_commits", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [{"operator": "budget.record_income", "params": {"amount": 500.0, "source": "salary"}}]),
    ("record_income_guard_refuses_negative", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [{"operator": "budget.record_income", "params": {"amount": -5.0, "source": "x"}}]),

    ("add_pocket_then_allocate_then_spend", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [{"operator": "budget.add_pocket", "params": {"pocket_name": "travel", "limit": 0.0}},
      _alloc("travel", 100.0), _spend("travel", 40.0)]),
    ("spend_beyond_remaining_refused", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [_alloc("food", 100.0), _spend("food", 500.0)]),
    ("spend_on_missing_pocket_refused", HOMESTEAD_LIKE, ALLOWED, ARMED, [_spend("nope", 1.0)]),
    ("spend_exactly_remaining_is_allowed", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [_alloc("food", 100.0), _spend("food", 100.0)]),

    ("add_pocket_duplicate_refused", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [{"operator": "budget.add_pocket", "params": {"pocket_name": "food", "limit": 0.0}},
      {"operator": "budget.add_pocket", "params": {"pocket_name": "food", "limit": 0.0}}]),
    ("pocket_name_is_normalised", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [{"operator": "budget.add_pocket", "params": {"pocket_name": "holiday fund", "limit": 0.0}}]),
    ("pocket_name_all_punctuation_refused", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [{"operator": "budget.add_pocket", "params": {"pocket_name": "!!!", "limit": 0.0}}]),

    ("operator_not_declared_by_the_sustain", HOMESTEAD_LIKE, ["budget.summary"], ARMED,
     [_alloc("food", 1.0)]),

    ("sequence_of_commits", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [{"operator": "budget.record_income", "params": {"amount": 200.0, "source": "s"}},
      _alloc("a", 300.0), _alloc("b", 400.0)]),

    ("a_refusal_mid_sequence_leaves_earlier_commits_intact", HOMESTEAD_LIKE, ALLOWED, ARMED,
     [_alloc("a", 300.0), _alloc("b", 5000.0), _alloc("c", 100.0)]),

    ("gate_disarmed_still_honours_operator_guards", HOMESTEAD_LIKE, ALLOWED, DISARMED,
     [_alloc("food", 5000.0)]),
]
