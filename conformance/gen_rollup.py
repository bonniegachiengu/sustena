"""
Roll-up ρ vector generation — R1 PARITY, recorded from the reference engine.

Drives `SustainEngine._aggregate_from_child_states` directly. That method is the
core of `compute_rollup`: `compute_rollup` only reads each child's state off
disk first, and the Rust core has no disk, so the shared arithmetic is exactly
what parity is about.

Each case records, per aggregate: the combined value, who was INCLUDED (with
their own number), and who was EXCLUDED (with the reference's own reason text).
The exclusions matter as much as the value — a roll-up that silently zeroes an
unreadable member is the failure this slice exists to rule out.
"""

from __future__ import annotations

from sustena.core.sustain_engine import SustainEngine


def _liquid(n):
    return {"finances": {"liquid": {"balance": n}}}


def _pockets(**kv):
    return {"finances": {"pockets": {k: {"allocated": v} for k, v in kv.items()}}}


LIQUID = "finances.liquid.balance"
POCKETS = "finances.pockets[*].allocated"

# name, aggregates, links, child_states, own_state (None = children only)
ROLLUP_CASES = [
    (
        "household_plus_children",
        [{"id": "total", "child_path": LIQUID, "op": "sum"}],
        [
            {"child_sustain_id": "bonnie", "slot": "habitat_1", "member": "Bonnie"},
            {"child_sustain_id": "cira", "slot": "habitat_2", "member": "Cira"},
        ],
        {"bonnie": _liquid(1800), "cira": _liquid(5000)},
        _liquid(14000),
    ),
    (
        "children_only_when_no_own_state",
        [{"id": "total", "child_path": LIQUID, "op": "sum"}],
        [{"child_sustain_id": "bonnie", "slot": "habitat_1", "member": "Bonnie"}],
        {"bonnie": _liquid(1800)},
        None,
    ),
    (
        "unreadable_child_excluded_and_named",
        [{"id": "total", "child_path": LIQUID, "op": "sum"}],
        [
            {"child_sustain_id": "bonnie", "slot": "habitat_1", "member": "Bonnie"},
            {"child_sustain_id": "frankie", "slot": "habitat_6", "member": "Frankie"},
        ],
        {"bonnie": _liquid(600)},  # frankie absent -> excluded, never zeroed
        _liquid(1000),
    ),
    (
        "child_without_the_path_excluded_and_named",
        [{"id": "total", "child_path": LIQUID, "op": "sum"}],
        [{"child_sustain_id": "garden", "slot": None, "member": "Garden"}],
        {"garden": {"soil": 40}},
        _liquid(1000),
    ),
    (
        "household_without_the_path_excluded_and_named",
        [{"id": "total", "child_path": LIQUID, "op": "sum"}],
        [{"child_sustain_id": "bonnie", "slot": None, "member": "Bonnie"}],
        {"bonnie": _liquid(600)},
        {"soil": 40},
    ),
    (
        "avg_over_contributors",
        [{"id": "total", "child_path": LIQUID, "op": "avg"}],
        [
            {"child_sustain_id": "a", "slot": None, "member": None},
            {"child_sustain_id": "b", "slot": None, "member": None},
        ],
        {"a": _liquid(10), "b": _liquid(30)},
        None,
    ),
    (
        "min_over_contributors",
        [{"id": "total", "child_path": LIQUID, "op": "min"}],
        [
            {"child_sustain_id": "a", "slot": None, "member": None},
            {"child_sustain_id": "b", "slot": None, "member": None},
        ],
        {"a": _liquid(10), "b": _liquid(30)},
        _liquid(25),
    ),
    (
        "max_over_contributors",
        [{"id": "total", "child_path": LIQUID, "op": "max"}],
        [
            {"child_sustain_id": "a", "slot": None, "member": None},
            {"child_sustain_id": "b", "slot": None, "member": None},
        ],
        {"a": _liquid(10), "b": _liquid(30)},
        _liquid(25),
    ),
    (
        "count_counts_contributors_not_attempts",
        [{"id": "total", "child_path": LIQUID, "op": "count"}],
        [
            {"child_sustain_id": "a", "slot": None, "member": None},
            {"child_sustain_id": "gone", "slot": None, "member": None},
        ],
        {"a": _liquid(2)},
        _liquid(1),
    ),
    (
        "min_over_nothing_is_null",
        [{"id": "total", "child_path": LIQUID, "op": "min"}],
        [],
        {},
        None,
    ),
    (
        "max_over_nothing_is_null",
        [{"id": "total", "child_path": LIQUID, "op": "max"}],
        [],
        {},
        None,
    ),
    (
        "sum_over_nothing_is_zero",
        [{"id": "total", "child_path": LIQUID, "op": "sum"}],
        [],
        {},
        None,
    ),
    (
        "avg_over_nothing_is_zero",
        [{"id": "total", "child_path": LIQUID, "op": "avg"}],
        [],
        {},
        None,
    ),
    (
        "wildcard_sums_within_each_contributor",
        [{"id": "pockets", "child_path": POCKETS, "op": "sum"}],
        [{"child_sustain_id": "bonnie", "slot": None, "member": "Bonnie"}],
        {"bonnie": _pockets(food=800, rent=200)},
        _pockets(food=1500, rent=500),
    ),
    (
        "wildcard_empty_container_is_a_real_zero",
        [{"id": "pockets", "child_path": POCKETS, "op": "sum"}],
        [{"child_sustain_id": "empty", "slot": None, "member": "Empty"}],
        {"empty": {"finances": {"pockets": {}}}},
        None,
    ),
    (
        "wildcard_scalar_container_is_excluded_not_zeroed",
        [{"id": "pockets", "child_path": POCKETS, "op": "sum"}],
        [{"child_sustain_id": "scalar", "slot": None, "member": "Scalar"}],
        {"scalar": {"finances": {"pockets": 7}}},
        None,
    ),
    (
        "wildcard_missing_container_is_excluded",
        [{"id": "pockets", "child_path": POCKETS, "op": "sum"}],
        [{"child_sustain_id": "nope", "slot": None, "member": "Nope"}],
        {"nope": {"soil": 1}},
        None,
    ),
    (
        "a_bool_is_not_a_number",
        [{"id": "total", "child_path": LIQUID, "op": "sum"}],
        [{"child_sustain_id": "flag", "slot": None, "member": "Flag"}],
        {"flag": _liquid(True)},
        None,
    ),
    (
        "several_aggregates_answered_independently",
        [
            {"id": "total", "child_path": LIQUID, "op": "sum"},
            {"id": "pockets", "child_path": POCKETS, "op": "sum"},
        ],
        [{"child_sustain_id": "bonnie", "slot": "habitat_1", "member": "Bonnie"}],
        {"bonnie": {**_liquid(100), "finances": {"liquid": {"balance": 100},
                                                "pockets": {"food": {"allocated": 9}}}}},
        {"finances": {"liquid": {"balance": 5}, "pockets": {"food": {"allocated": 1}}}},
    ),
    (
        "the_real_household_shape",
        [
            {"id": "household_liquid_total", "child_path": LIQUID, "op": "sum"},
            {"id": "household_pockets_total", "child_path": POCKETS, "op": "sum"},
        ],
        [
            {"child_sustain_id": f"habitat-{m.lower()}", "slot": f"habitat_{i}", "member": m}
            for i, m in enumerate(["Bonnie", "Cira", "Epha", "Mum", "Kui", "Frankie"], start=1)
        ],
        {
            "habitat-bonnie": {"finances": {"liquid": {"balance": 1800},
                                            "pockets": {"food": {"allocated": 200}}}},
            "habitat-cira": {"finances": {"liquid": {"balance": 5000}, "pockets": {}}},
            "habitat-epha": {"finances": {"liquid": {"balance": 0}, "pockets": {}}},
            "habitat-mum": {"finances": {"liquid": {"balance": 0}, "pockets": {}}},
            "habitat-kui": {"finances": {"liquid": {"balance": 0}, "pockets": {}}},
            "habitat-frankie": {"finances": {"liquid": {"balance": 0}, "pockets": {}}},
        },
        {"finances": {"liquid": {"balance": 6600},
                      "pockets": {"food": {"allocated": 8000}, "rent": {"allocated": 20000}}}},
    ),
]


def run_rollup_case(aggregates, links, child_states, own_state) -> dict:
    """Record what the reference engine answers. This does not assert."""
    engine = SustainEngine(db_path=":memory:")
    spec = {"aggregates": aggregates}
    parent_id = "homestead" if own_state is not None else None
    return engine._aggregate_from_child_states(  # noqa: SLF001 — recording the reference
        spec, links, child_states, parent_id, own_state
    )
