"""
sustena/core/pawa_meter.py

The Pawa meter — SUSTENA_UPGRADE_SPEC.md §4L. JUUL is the coin, PAWA is
the gas: the metered compute+storage cost of one real operator run,
denominated in juul. This module is the measurement-only half (the
odometer). Spending juul, gate-blocking on balance, and contribution
royalties (core/pawa.py's already-scaffolded `PawaLedger.charge()`) are
later slices that consume these readings — nothing here touches a
balance, and nothing here is wired to core/pawa.py.

    pawa(run) = KAPPA_COMPUTE * compute(run) + KAPPA_STORAGE * storage(run)

`compute` is a reproducible proxy for work done, deliberately NOT
wall-clock: wall-clock is recorded separately (SustainEngine passes it
through as `elapsed_ms`) as a secondary, non-authoritative field — it's
machine-dependent, so the same operator call would "cost" a different
amount on a slower box, which defeats the point of a metering primitive
meant to be comparable across runs and across time.

    compute = mutation_count + event_count + constraint_eval_count

`storage` is real bytes durably added — the serialized size of exactly
what gets written to the events table's payload_json/mutations_json
columns for this run (computed by the caller from the real event/
mutation records about to be persisted), not an estimate.
"""

from __future__ import annotations

# Declared, documented coefficients — NOT yet calibrated against any real
# infrastructure cost model (that calibration is explicit future work).
# Storage is priced far more granularly than a single compute unit,
# matching how real systems meter storage vs. operation count.
KAPPA_COMPUTE = 1.0   # pawa per compute unit (one mutation, one event, one constraint eval)
KAPPA_STORAGE = 0.01  # pawa per byte durably written


def compute_pawa(compute: float, storage: float) -> float:
    """pawa = kappa_c * compute + kappa_s * storage — the whole formula,
    kept as one small pure function so §4L's step 4 (simulator efficiency
    ranking) can reuse the exact same pricing without duplicating it."""
    return KAPPA_COMPUTE * compute + KAPPA_STORAGE * storage


def compute_units(mutation_count: int, event_count: int, constraint_eval_count: int) -> int:
    """The compute proxy: count of state mutations + events emitted +
    predicate/constraint evaluations run for this operator call."""
    return mutation_count + event_count + constraint_eval_count


def constraint_eval_count(meta, spec: dict, enforcement_ran: bool) -> int:
    """How many predicate checks this call was actually subject to —
    the operator's own declared pre/post-conditions (always evaluated,
    every call: pre-conditions inside the operator function itself,
    post-conditions as part of the article's guard->effect model) plus
    the sustain's declared invariants, but ONLY when the S2 enforcement
    gate actually ran (a sustain that hasn't opted in genuinely had zero
    invariant checks performed against this call — reporting a count
    here would be measuring a check that never happened)."""
    count = len(getattr(meta, "constraints", []) or []) + len(getattr(meta, "post_constraints", []) or [])
    if enforcement_ran:
        count += len(spec.get("invariants", []) or [])
    return count
