"""
tests/test_conformance_vectors.py

The reference engine replaying its own conformance vectors.

Why this matters even though the vectors were GENERATED from this engine: it
turns the vector files into a regression net. If a change to state.py or
event_fold.py alters recorded behaviour, these fail — so a behavioural change
has to be deliberate and reviewed, not discovered later by the Rust port
disagreeing.

`sustena-core/tests/conformance.rs` replays the identical files. Between them,
the two engines are held to one contract.

See conformance/README.md.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from sustena.core.event_fold import FoldError, diff_to_mutations, fold_events
from sustena.core.state import StateAccessor, StatePathError, StateValueError

VECTORS = Path(__file__).resolve().parents[3] / "conformance" / "vectors"
CONFORMANCE_VERSION = 1


def _load(name: str) -> dict:
    path = VECTORS / name
    if not path.exists():
        pytest.skip(f"vector file missing: {path} — run conformance/generate.py")
    doc = json.loads(path.read_text(encoding="utf-8"))
    assert doc["conformance_version"] == CONFORMANCE_VERSION, (
        f"{name} was written for contract v{doc['conformance_version']}, "
        f"this engine speaks v{CONFORMANCE_VERSION}"
    )
    return doc


def _run_state_ops(initial: dict, ops: list[dict]) -> dict:
    """Must mirror conformance/generate.py's replay exactly."""
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
                raise AssertionError(f"unknown op: {kind}")
            results.append({"ok": value})
        except StatePathError as exc:
            results.append({"error": {"kind": "path", "message": str(exc)}})
        except StateValueError as exc:
            results.append({"error": {"kind": "value", "message": str(exc)}})
    return {
        "results": results,
        "snapshot": acc.snapshot(),
        "mutations": acc.mutations(),
        "reconciled_mutations": acc.reconciled_mutations(),
        "unrecorded_changes": acc.unrecorded_changes(),
    }


def _state_cases():
    return [(c["name"], c) for c in _load("state.json")["cases"]]


@pytest.mark.parametrize("name,case", _state_cases(), ids=[n for n, _ in _state_cases()])
def test_state_vector(name, case):
    actual = _run_state_ops(case["initial"], case["ops"])
    expect = case["expect"]

    assert actual["snapshot"] == expect["snapshot"], f"{name}: resulting state differs"
    assert actual["mutations"] == expect["mutations"], f"{name}: mutation records differ"

    assert len(actual["results"]) == len(expect["results"]), f"{name}: result count differs"
    for i, (got, want) in enumerate(zip(actual["results"], expect["results"])):
        # Failure KIND is the contract; exact prose is not (see the README).
        assert ("error" in got) == ("error" in want), f"{name}: op {i} success/failure differs"
        if "error" in want:
            assert got["error"]["kind"] == want["error"]["kind"], f"{name}: op {i} error kind differs"
        else:
            assert got["ok"] == want["ok"], f"{name}: op {i} return value differs"


def _fold_cases():
    return [(c["name"], c) for c in _load("fold.json")["fold_cases"]]


@pytest.mark.parametrize("name,case", _fold_cases(), ids=[n for n, _ in _fold_cases()])
def test_fold_vector(name, case):
    expect = case["expect"]
    if "error" in expect:
        with pytest.raises(FoldError):
            fold_events(case["events"], case["initial"])
    else:
        assert fold_events(case["events"], case["initial"]) == expect["state"]


def _diff_cases():
    return [(c["name"], c) for c in _load("fold.json")["diff_cases"]]


@pytest.mark.parametrize("name,case", _diff_cases(), ids=[n for n, _ in _diff_cases()])
def test_diff_vector(name, case):
    muts = diff_to_mutations(case["before"], case["after"])
    assert muts == case["expect"]["mutations"], f"{name}: derived mutations differ"

    # The property that actually matters: the patch reproduces `after`.
    replayed = fold_events([{"mutations": muts}], initial_state=case["before"])
    assert replayed == case["after"], f"{name}: patch does not reproduce the target state"


def test_folding_a_log_never_mutates_its_source():
    """
    Regression lock for the aliasing bug the vectors uncovered.

    set() recorded live references rather than snapshots, so replaying a log
    mutated the state it was rebuilt from — reading the past changed the
    present — and an append to a missing list applied its item twice.
    """
    acc = StateAccessor({})
    acc.append("todo.items", {"kind": "task", "id": "i1"})

    before = json.dumps(acc.snapshot(), sort_keys=True)
    replayed = fold_events([{"mutations": acc.mutations()}], initial_state={})
    after = json.dumps(acc.snapshot(), sort_keys=True)

    assert before == after, "folding a log corrupted the state it read from"
    assert replayed == acc.snapshot(), "the log does not reproduce its own state"
    assert len(replayed["todo"]["items"]) == 1, "the item was applied twice"


def test_a_recorded_mutation_cannot_be_rewritten_afterwards():
    acc = StateAccessor({})
    acc.set("items", [])
    recorded = acc.mutations()[0]["new"]
    acc.get("items").append({"id": "x"})
    assert recorded == [], "a mutation record must be a snapshot, not a live reference"
