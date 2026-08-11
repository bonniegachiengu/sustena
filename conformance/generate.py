#!/usr/bin/env python
"""
Generate conformance vectors by exercising the REFERENCE (Python) engine.

Run from apps/api so the sustena package resolves:

    cd apps/api && python ../../conformance/generate.py

This RECORDS behaviour; it does not assert it. If regenerating changes a
vector, the reference engine's behaviour changed — review the diff, do not
wave it through.

See conformance/README.md for the contract.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
sys.path.insert(0, str(REPO / "apps" / "api"))

from sustena.core.event_fold import FoldError, diff_to_mutations, fold_events  # noqa: E402
from sustena.core.state import StateAccessor, StatePathError, StateValueError  # noqa: E402

CONFORMANCE_VERSION = 1


def _kind(exc: BaseException) -> str:
    if isinstance(exc, StatePathError):
        return "path"
    if isinstance(exc, StateValueError):
        return "value"
    if isinstance(exc, FoldError):
        return "fold"
    return "other"


# ── State slice ───────────────────────────────────────────────────────────────

def run_state_case(initial: dict, ops: list[dict]) -> dict:
    """Replay ops against the reference StateAccessor and record everything."""
    acc = StateAccessor(initial)
    results = []

    for op in ops:
        kind = op["op"]
        try:
            if kind == "set":
                acc.set(op["path"], op["value"])
                value = None
            elif kind == "increment":
                value = acc.increment(op["path"], op["delta"])
            elif kind == "decrement":
                value = acc.decrement(op["path"], op["delta"], op.get("allow_negative", False))
            elif kind == "append":
                item = dict(op["item"])
                # Ids are always supplied: the reference mints a UUID when one
                # is absent, which is not reproducible and so cannot be a
                # conformance expectation.
                item.setdefault("id", op["id"])
                value = acc.append(op["path"], item)
            elif kind == "remove":
                acc.remove(op["path"], op["item_id"])
                value = None
            elif kind == "get":
                value = acc.get(op["path"])
            elif kind == "get_strict":
                value = acc.get_strict(op["path"])
            elif kind == "exists":
                value = acc.exists(op["path"])
            else:
                raise AssertionError(f"unknown op in vector spec: {kind}")
            results.append({"ok": value})
        except (StatePathError, StateValueError) as exc:
            results.append({"error": {"kind": _kind(exc), "message": str(exc)}})

    return {
        "results": results,
        "snapshot": acc.snapshot(),
        "mutations": acc.mutations(),
        "reconciled_mutations": acc.reconciled_mutations(),
        "unrecorded_changes": acc.unrecorded_changes(),
    }


STATE_CASES = [
    ("read_missing_path_yields_null", {"a": {"b": 1}}, [{"op": "get", "path": "a.zzz"}]),
    ("read_nested", {"a": {"b": {"c": 7}}}, [{"op": "get", "path": "a.b.c"}]),
    ("read_indexed", {"r": [{"n": "x"}, {"n": "y"}]}, [{"op": "get", "path": "r[1].n"}]),
    ("get_strict_missing_raises", {"a": {}}, [{"op": "get_strict", "path": "a.nope"}]),
    ("exists_true_and_false", {"a": {"b": None}},
     [{"op": "exists", "path": "a.b"}, {"op": "exists", "path": "a.c"}]),

    ("set_scalar", {"a": {"b": 1}}, [{"op": "set", "path": "a.b", "value": 2}]),
    ("set_creates_intermediate_objects", {}, [{"op": "set", "path": "x.y.z", "value": 5}]),
    ("set_replaces_whole_subtree", {"a": {"b": {"c": 1}}},
     [{"op": "set", "path": "a.b", "value": {"d": 2}}]),
    ("set_indexed_element", {"r": [{"n": "x"}, {"n": "y"}]},
     [{"op": "set", "path": "r[0].n", "value": "z"}]),
    ("set_string_and_null_and_bool", {},
     [{"op": "set", "path": "s", "value": "hi"},
      {"op": "set", "path": "n", "value": None},
      {"op": "set", "path": "b", "value": True}]),

    ("increment_number", {"f": {"bal": 100}}, [{"op": "increment", "path": "f.bal", "delta": 50}]),
    ("increment_float", {"f": {"bal": 100.5}}, [{"op": "increment", "path": "f.bal", "delta": 0.25}]),
    ("increment_non_numeric_raises", {"f": {"bal": "nope"}},
     [{"op": "increment", "path": "f.bal", "delta": 1}]),
    ("increment_missing_path_raises", {}, [{"op": "increment", "path": "nope", "delta": 1}]),

    ("decrement_number", {"f": {"bal": 100}}, [{"op": "decrement", "path": "f.bal", "delta": 40}]),
    ("decrement_to_exactly_zero_is_allowed", {"f": {"bal": 40}},
     [{"op": "decrement", "path": "f.bal", "delta": 40}]),
    ("decrement_below_zero_refused", {"f": {"bal": 10}},
     [{"op": "decrement", "path": "f.bal", "delta": 25}]),
    ("decrement_below_zero_allowed_when_permitted", {"f": {"bal": 10}},
     [{"op": "decrement", "path": "f.bal", "delta": 25, "allow_negative": True}]),

    ("append_to_existing_list", {"items": []},
     [{"op": "append", "path": "items", "item": {"kind": "task"}, "id": "i1"}]),
    ("append_creates_missing_list", {},
     [{"op": "append", "path": "todo.items", "item": {"kind": "task"}, "id": "i1"}]),
    ("append_preserves_supplied_id", {"items": []},
     [{"op": "append", "path": "items", "item": {"id": "given", "k": 1}, "id": "ignored"}]),
    ("append_to_non_list_raises", {"items": {"a": 1}},
     [{"op": "append", "path": "items", "item": {}, "id": "i1"}]),

    ("remove_by_id", {"items": [{"id": "a"}, {"id": "b"}]},
     [{"op": "remove", "path": "items", "item_id": "a"}]),
    ("remove_missing_id_raises", {"items": [{"id": "a"}]},
     [{"op": "remove", "path": "items", "item_id": "zzz"}]),
    ("remove_from_non_list_raises", {"items": {"a": 1}},
     [{"op": "remove", "path": "items", "item_id": "a"}]),

    ("sequence_records_every_mutation_in_order", {"f": {"bal": 100}, "items": []},
     [{"op": "set", "path": "f.note", "value": "start"},
      {"op": "increment", "path": "f.bal", "delta": 10},
      {"op": "append", "path": "items", "item": {"k": 1}, "id": "i1"},
      {"op": "decrement", "path": "f.bal", "delta": 5},
      {"op": "remove", "path": "items", "item_id": "i1"}]),

    ("failed_op_records_nothing", {"f": {"bal": 10}},
     [{"op": "decrement", "path": "f.bal", "delta": 999},
      {"op": "set", "path": "f.after", "value": 1}]),

    ("invalid_path_segment_raises", {"a": 1}, [{"op": "set", "path": "a-b", "value": 1}]),
]


# ── Fold slice ────────────────────────────────────────────────────────────────

def run_fold_case(events: list[dict], initial):
    try:
        return {"state": fold_events(events, initial)}
    except FoldError as exc:
        return {"error": {"kind": "fold", "message": str(exc)}}


FOLD_CASES = [
    ("empty_log_yields_initial", [], {"a": 1}),
    ("genesis_replace_root_discards_prior", [
        {"mutations": [{"op": "replace_root", "value": {"a": 1}}]}], {"stale": True}),
    ("sets_apply_in_order", [
        {"mutations": [{"op": "set", "path": "a.b", "old": None, "new": 1}]},
        {"mutations": [{"op": "set", "path": "a.b", "old": 1, "new": 2}]}], {}),
    ("append_then_remove", [
        {"mutations": [{"op": "append", "path": "items", "action": "append",
                        "item_id": "i1", "item": {"id": "i1", "k": 1}}]},
        {"mutations": [{"op": "remove", "path": "items", "action": "remove", "item_id": "i1"}]}], {}),
    ("event_with_no_mutations_is_a_noop", [{"mutations": []}], {"a": 1}),
    ("multi_mutation_event", [
        {"mutations": [
            {"op": "set", "path": "a", "old": None, "new": 1},
            {"op": "set", "path": "b", "old": None, "new": 2}]}], {}),
    ("remove_of_missing_item_fails_loudly", [
        {"mutations": [{"op": "remove", "path": "items", "action": "remove", "item_id": "gone"}]}], {"items": []}),
]

DIFF_CASES = [
    ("identical", {"a": 1}, {"a": 1}),
    ("changed_scalar", {"a": 1}, {"a": 2}),
    ("added_key", {"a": 1}, {"a": 1, "b": 2}),
    ("removed_key_widens", {"x": {"k1": 1, "k2": 2}}, {"x": {"k1": 1}}),
    ("nested_precise", {"f": {"p": {"food": {"spent": 0}}}}, {"f": {"p": {"food": {"spent": 250}}}}),
    ("list_change_widens", {"i": [1, 2, 3]}, {"i": [1, 2, 3, 4]}),
    ("list_of_dicts_status_flip",
     {"o": [{"id": "po1", "status": "RAISED"}]}, {"o": [{"id": "po1", "status": "DELIVERED"}]}),
    ("unaddressable_key_widens", {"p": {"holiday fund": 0}}, {"p": {"holiday fund": 50}}),
    ("type_change", {"a": {"b": 1}}, {"a": "str"}),
    ("root_shape_change", {"a": 1, "b": 2}, {"a": 1}),
]


def main() -> int:
    out_dir = HERE / "vectors"
    out_dir.mkdir(parents=True, exist_ok=True)

    state_doc = {
        "conformance_version": CONFORMANCE_VERSION,
        "slice": "state",
        "generated_from": "sustena/core/state.py",
        "note": "Recorded from the reference engine. See conformance/README.md.",
        "cases": [
            {"name": name, "initial": initial, "ops": ops, "expect": run_state_case(initial, ops)}
            for name, initial, ops in STATE_CASES
        ],
    }

    fold_doc = {
        "conformance_version": CONFORMANCE_VERSION,
        "slice": "fold",
        "generated_from": "sustena/core/event_fold.py",
        "note": "Recorded from the reference engine. See conformance/README.md.",
        "fold_cases": [
            {"name": name, "initial": initial, "events": events,
             "expect": run_fold_case(events, initial)}
            for name, events, initial in FOLD_CASES
        ],
        "diff_cases": [
            {"name": name, "before": before, "after": after,
             "expect": {
                 "mutations": diff_to_mutations(before, after),
                 "roundtrip": fold_events(
                     [{"mutations": diff_to_mutations(before, after)}], initial_state=before),
             }}
            for name, before, after in DIFF_CASES
        ],
    }

    for filename, doc in (("state.json", state_doc), ("fold.json", fold_doc)):
        path = out_dir / filename
        path.write_text(json.dumps(doc, indent=2, sort_keys=False) + "\n", encoding="utf-8")
        count = len(doc.get("cases") or doc.get("fold_cases", [])) + len(doc.get("diff_cases", []))
        print(f"  wrote {path.relative_to(REPO)}  ({count} cases)")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
