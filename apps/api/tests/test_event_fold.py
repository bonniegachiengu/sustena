"""
tests/test_event_fold.py

sustena/core/event_fold.py — the pure fold_events() reducer (Slice 3:
state = fold(events)). Unit tests only; SustainEngine integration (seq
assignment, the enforcing gate, migration) is covered in
test_sustain_engine.py::TestEventSourcing.
"""

import pytest

from sustena.core.event_fold import FoldError, apply_mutation, fold_events
from sustena.core.state import StateAccessor


class TestFoldEvents:
    def test_empty_events_yields_empty_state(self):
        assert fold_events([]) == {}

    def test_genesis_replace_root_establishes_baseline(self):
        events = [{"mutations": [{"op": "replace_root", "value": {"a": 1}}]}]
        assert fold_events(events) == {"a": 1}

    def test_set_mutation_applies(self):
        events = [
            {"mutations": [{"op": "replace_root", "value": {"finances": {"liquid": {"balance": 0.0}}}}]},
            {"mutations": [{"op": "set", "path": "finances.liquid.balance", "old": 0.0, "new": 500.0}]},
        ]
        assert fold_events(events)["finances"]["liquid"]["balance"] == 500.0

    def test_set_mutations_apply_in_order(self):
        events = [
            {"mutations": [{"op": "replace_root", "value": {"n": 0}}]},
            {"mutations": [{"op": "set", "path": "n", "old": 0, "new": 1}]},
            {"mutations": [{"op": "set", "path": "n", "old": 1, "new": 2}]},
            {"mutations": [{"op": "set", "path": "n", "old": 2, "new": 3}]},
        ]
        assert fold_events(events)["n"] == 3

    def test_append_mutation_adds_item(self):
        events = [
            {"mutations": [{"op": "replace_root", "value": {"members": []}}]},
            {"mutations": [{"op": "append", "path": "members", "item": {"id": "m1", "name": "Bonnie"}, "item_id": "m1"}]},
        ]
        assert fold_events(events)["members"] == [{"id": "m1", "name": "Bonnie"}]

    def test_append_preserves_original_item_even_if_caller_mutates_dict_after(self):
        """Mutation records store a deep copy — replay must reflect append-time data, not later edits."""
        item = {"id": "m1", "name": "Bonnie"}
        accessor = StateAccessor({"members": []})
        accessor.append("members", item)
        item["name"] = "Changed after append"  # caller still holds the reference
        mutation = accessor.mutations()[0]
        assert mutation["item"]["name"] == "Bonnie"

    def test_remove_mutation_removes_item(self):
        events = [
            {"mutations": [{"op": "replace_root", "value": {"members": [{"id": "m1"}, {"id": "m2"}]}}]},
            {"mutations": [{"op": "remove", "path": "members", "item_id": "m1"}]},
        ]
        assert fold_events(events)["members"] == [{"id": "m2"}]

    def test_empty_mutations_list_is_a_no_op(self):
        """Informational-only events (or the non-first event of a multi-event call) don't change state."""
        events = [
            {"mutations": [{"op": "replace_root", "value": {"a": 1}}]},
            {"mutations": []},
            {"event_name": "no mutations key at all"},
        ]
        assert fold_events(events) == {"a": 1}

    def test_multiple_mutations_in_one_event_all_apply(self):
        """Mirrors a real operator call: several StateAccessor calls, all attached to one event."""
        events = [{
            "mutations": [
                {"op": "replace_root", "value": {"finances": {"liquid": {"balance": 0.0}}, "pockets": {}}},
            ]
        }, {
            "mutations": [
                {"op": "set", "path": "finances.liquid.balance", "old": 0.0, "new": 5000.0},
                {"op": "set", "path": "pockets", "old": {}, "new": {"food": {"allocated": 2000.0}}},
                {"op": "set", "path": "finances.liquid.balance", "old": 5000.0, "new": 3000.0},
            ]
        }]
        result = fold_events(events)
        assert result["finances"]["liquid"]["balance"] == 3000.0
        assert result["pockets"] == {"food": {"allocated": 2000.0}}

    def test_unknown_op_raises_fold_error(self):
        with pytest.raises(FoldError):
            fold_events([{"mutations": [{"op": "teleport", "path": "x"}]}])

    def test_remove_of_missing_item_raises_fold_error(self):
        events = [
            {"mutations": [{"op": "replace_root", "value": {"members": []}}]},
            {"mutations": [{"op": "remove", "path": "members", "item_id": "ghost"}]},
        ]
        with pytest.raises(FoldError):
            fold_events(events)

    def test_set_on_missing_intermediate_path_autocreates(self):
        """Mirrors StateAccessor.set()'s own auto-vivification of intermediate dicts."""
        events = [{"mutations": [{"op": "set", "path": "a.b.c", "old": None, "new": 42}]}]
        assert fold_events(events)["a"]["b"]["c"] == 42

    def test_initial_state_ignored_once_a_replace_root_event_occurs(self):
        """A genesis event always wins over whatever initial_state was passed — matches
        real usage, where rebuild_state() never has a meaningful initial_state to give."""
        events = [{"mutations": [{"op": "replace_root", "value": {"a": 1}}]}]
        assert fold_events(events, initial_state={"a": 999, "b": "should be discarded"}) == {"a": 1}

    def test_apply_mutation_directly_returns_accessor(self):
        accessor = StateAccessor({"n": 0})
        result = apply_mutation(accessor, {"op": "set", "path": "n", "old": 0, "new": 1})
        assert isinstance(result, StateAccessor)
        assert result.get("n") == 1


class TestFoldMatchesRealStateAccessorUsage:
    """
    End-to-end within this module: run real StateAccessor operations, capture
    its own recorded mutations, fold them, and confirm the fold matches what
    StateAccessor itself ended up with. This is the same property
    SustainEngine relies on (state.snapshot() == fold_events(the events it
    just wrote)) but exercised here without any DB involved.
    """

    def test_fold_of_real_mutations_matches_final_snapshot(self):
        accessor = StateAccessor({})
        accessor.set("finances.liquid.balance", 1000.0)
        accessor.set("finances.liquid.balance", 1500.0)
        accessor.append("members", {"name": "Bonnie"})
        accessor.append("members", {"name": "Cira"})
        member_id = accessor.snapshot()["members"][0]["id"]
        accessor.remove("members", member_id)

        events = [{"mutations": accessor.mutations()}]
        assert fold_events(events) == accessor.snapshot()
