"""
sustena/core/event_fold.py

The fold — state = fold(events).

A sustain's current state is derived by replaying its ordered event log
through a reducer, not by mutating a stored blob in place. This module is
that reducer: pure, DB-independent, and reused identically by the live
write path (SustainEngine._append_events_and_update_cache, which keeps a
materialized cache in sync on every write) and the verification/rebuild
path (SustainEngine.rebuild_state, which folds from scratch).

Model chosen: a state-transition-log fold, not a hand-written business-logic
reducer per domain event type. Each event carries a `mutations` list — the
exact StateAccessor mutation records (op/path/value) produced while the
operator that published it ran. Replaying those mutations in order can never
diverge from what the operator actually did, because it IS what the operator
did — there is no parallel "what should event X mean" logic to keep in sync
with the operators. The tradeoff, stated honestly: this gives correct,
provably-reproducible state, not semantic event replay — you can't yet ask
"what would state look like if pocket_spent meant something different,"
because a mutation record is an opaque patch, not domain intent. That's a
reasonable line for this slice; a richer domain-typed model can be layered
on later without changing the storage shape (mutations are already there).

Mutation record shapes (mirror sustena/core/state.py's StateAccessor exactly
— see its set()/append()/remove(), and the "op" key added there for this):
  {"op": "set",    "path": str, "old": Any, "new": Any}
  {"op": "append", "path": str, "item": dict, "item_id": str}
  {"op": "remove", "path": str, "item_id": str}
  {"op": "replace_root", "value": dict}
      -- genesis marker only: every sustain's history starts with exactly one
         of these (written at instantiate() time, or backfilled by the
         one-time migration for sustains that predate event sourcing).
         Folding it discards whatever came before and starts fresh from
         `value` — by construction it is always the first event, so nothing
         is actually discarded in practice.

No eval()/exec()/compile() — this module only ever calls StateAccessor's own
existing, already-tested set()/append()/remove() methods to apply a patch.
"""

from __future__ import annotations

import re
from typing import Any

from sustena.core.state import StateAccessor, StatePathError


class FoldError(Exception):
    """
    Raised when a mutation can't be replayed — e.g. a 'remove' referencing an
    item_id no longer present, or an unrecognised 'op'. This should never
    happen against a log SustainEngine itself produced; if it does, that's a
    real bug worth failing loudly on rather than silently returning wrong
    state (per the standing no-silent-failure rule).
    """


def apply_mutation(accessor: StateAccessor, mutation: dict) -> StateAccessor:
    """
    Apply one mutation record to accessor, returning the accessor to continue
    folding with (a new one for "replace_root", the same one otherwise).
    """
    op = mutation.get("op")
    try:
        if op == "replace_root":
            return StateAccessor(mutation["value"])
        if op == "set":
            accessor.set(mutation["path"], mutation["new"])
            return accessor
        if op == "append":
            accessor.append(mutation["path"], dict(mutation["item"]))
            return accessor
        if op == "remove":
            accessor.remove(mutation["path"], mutation["item_id"])
            return accessor
    except (StatePathError, KeyError, TypeError) as exc:
        raise FoldError(f"failed to replay mutation {mutation!r}: {exc}") from exc

    raise FoldError(f"unknown mutation op {op!r} in {mutation!r}")


_SAFE_KEY = re.compile(r"\w+\Z")


def diff_to_mutations(before: Any, after: Any, path: str = "") -> list[dict]:
    """
    Derive the mutation records that transform `before` into `after`.

    This is the fold's inverse, and it exists to make fold-fidelity a
    STRUCTURAL guarantee rather than a matter of operator discipline.

    StateAccessor.get() hands back a LIVE reference into the underlying dict,
    so an operator can legally write `po["status"] = "DELIVERED"` and change
    real state without any mutation being recorded. That state then persists to
    the cache while the event log never learns about it — and rebuild_state()
    silently stops matching get_state(). Comparing the accessor's untouched
    `_original` against its final `_data` catches exactly that, no matter how
    the change was made.

    Emits the narrowest correct patch, and deliberately widens to a whole-value
    `set` (or `replace_root`) whenever a narrower one could not faithfully
    reproduce the result:

      - a removed dict key, which `set` cannot express;
      - a key that is not a plain \\w+ identifier, which StateAccessor's own
        path grammar cannot address;
      - any list change, since element-wise list patching would need index
        semantics the mutation vocabulary does not have.

    Widening is always safe: a `set` of the parent value reproduces the exact
    subtree. Correctness first, minimality second.
    """
    if before == after:
        return []

    if isinstance(before, dict) and isinstance(after, dict):
        removed = [k for k in before if k not in after]
        unsafe = [k for k in after if not _SAFE_KEY.match(str(k))]
        if removed or unsafe:
            # Cannot express with per-key `set` — replace this whole node.
            if not path:
                return [{"op": "replace_root", "value": after}]
            return [{"op": "set", "path": path, "old": before, "new": after}]

        out: list[dict] = []
        for key, new_val in after.items():
            child = f"{path}.{key}" if path else str(key)
            if key not in before:
                out.append({"op": "set", "path": child, "old": None, "new": new_val})
            else:
                out.extend(diff_to_mutations(before[key], new_val, child))
        return out

    # Lists, scalars, and type changes: replace the value outright.
    if not path:
        return [{"op": "replace_root", "value": after}]
    return [{"op": "set", "path": path, "old": before, "new": after}]


def fold_events(events: list[dict], initial_state: dict | None = None) -> dict:
    """
    Fold an ordered list of event rows into a state dict.

    events: must already be in application order (SustainEngine queries
            "ORDER BY seq ASC" before calling this — fold_events itself does
            not sort, it's a pure replay of whatever order it's handed).
            Each event is a dict with a "mutations" key: a list of mutation
            records, possibly empty (an informational-only event, or the
            non-first event of a multi-event operator call, is a no-op here).
    initial_state: starting point before replay. Normally omitted — every
            real sustain's first event is a "replace_root" genesis snapshot
            that establishes the true starting point regardless of what's
            passed here.
    """
    accessor = StateAccessor(initial_state or {})
    for event in events:
        for mutation in (event.get("mutations") or []):
            accessor = apply_mutation(accessor, mutation)
    return accessor.snapshot()
