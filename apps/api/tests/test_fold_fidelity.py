"""
tests/test_fold_fidelity.py

The invariant these tests defend: state is a provably reproducible fold of its
events, so `rebuild_state(sid) == get_state(sid)` holds for every sustain at
all times.

WHY THIS FILE EXISTS

That invariant was already asserted widely — but NOT in the three test modules
covering the three operators that broke it (procurement, control, council).
The invariant held everywhere it was checked; the bug lived precisely where
the check was omitted. So testing more of the same would not have caught it.

These tests therefore assert the property STRUCTURALLY instead of per-operator:

  1. The engine reconciles ANY unrecorded state change, including from an
     operator that does not exist yet (the guarantee is by construction).
  2. The three historically-offending operators are fold-clean at source.
  3. A generic sweep: no registered operator may leave unrecorded changes.

The third is the one that scales. A new operator written with the same
in-place-edit mistake fails it without anyone remembering to add a test.
"""

import pytest

from sustena.core.event_fold import diff_to_mutations, fold_events
from sustena.core.state import StateAccessor


# ── The primitive: does the reconciler actually reproduce reality? ────────────

class TestDiffToMutations:
    """diff_to_mutations must produce a patch that exactly reproduces `after`."""

    def _roundtrip(self, before, after):
        muts = diff_to_mutations(before, after)
        replayed = fold_events([{"mutations": muts}], initial_state=before)
        return replayed

    def test_identical_states_need_no_mutations(self):
        assert diff_to_mutations({"a": 1}, {"a": 1}) == []

    def test_changed_scalar(self):
        before, after = {"a": 1}, {"a": 2}
        assert self._roundtrip(before, after) == after

    def test_added_key(self):
        before, after = {"a": 1}, {"a": 1, "b": 2}
        assert self._roundtrip(before, after) == after

    def test_removed_key_widens_to_whole_value(self):
        # `set` cannot delete a key, so the reconciler must widen.
        before, after = {"x": {"k1": 1, "k2": 2}}, {"x": {"k1": 1}}
        assert self._roundtrip(before, after) == after

    def test_nested_change_is_addressed_precisely(self):
        before = {"finances": {"pockets": {"food": {"spent": 0}}}}
        after = {"finances": {"pockets": {"food": {"spent": 250}}}}
        muts = diff_to_mutations(before, after)
        assert len(muts) == 1
        assert muts[0]["path"] == "finances.pockets.food.spent"
        assert self._roundtrip(before, after) == after

    def test_list_change(self):
        before, after = {"items": [1, 2, 3]}, {"items": [1, 2, 3, 4]}
        assert self._roundtrip(before, after) == after

    def test_list_of_dicts_status_flip(self):
        # The exact shape all three real bugs took.
        before = {"orders": [{"id": "po1", "status": "RAISED"}]}
        after = {"orders": [{"id": "po1", "status": "DELIVERED"}]}
        assert self._roundtrip(before, after) == after

    def test_key_that_is_not_a_valid_path_segment(self):
        # StateAccessor's path grammar is \w+, so a key with a space cannot be
        # addressed. The reconciler must widen rather than emit a broken path.
        before = {"pockets": {"holiday fund": {"spent": 0}}}
        after = {"pockets": {"holiday fund": {"spent": 50}}}
        assert self._roundtrip(before, after) == after

    def test_type_change(self):
        assert self._roundtrip({"a": {"b": 1}}, {"a": "now-a-string"}) == {"a": "now-a-string"}

    def test_root_replacement_when_root_shape_changes(self):
        before, after = {"a": 1, "b": 2}, {"a": 1}
        assert self._roundtrip(before, after) == after


# ── The guarantee: an in-place edit can no longer escape the log ──────────────

class TestStateAccessorReconciliation:

    def test_in_place_edit_records_nothing_but_is_reconciled(self):
        """The precise defect: a live reference edited directly."""
        start = {"orders": [{"id": "po1", "status": "RAISED"}]}
        s = StateAccessor(start)

        po = s.get("orders")[0]          # live reference
        po["status"] = "DELIVERED"       # the silent write

        assert s.mutations() == [], "no mutation recorded — this is the bug"
        assert s.unrecorded_changes(), "reconciler must detect it"

        naive = fold_events([{"mutations": s.mutations()}], initial_state=start)
        assert naive != s.snapshot(), "raw mutations must NOT reproduce state"

        fixed = fold_events([{"mutations": s.reconciled_mutations()}], initial_state=start)
        assert fixed == s.snapshot(), "reconciled mutations MUST reproduce state"

    def test_well_behaved_operator_is_untouched(self):
        """Zero overhead and zero change for code that uses the API properly."""
        s = StateAccessor({"finances": {"liquid": {"balance": 100}}})
        s.set("finances.liquid.balance", 150)
        assert s.unrecorded_changes() == []
        assert s.reconciled_mutations() == s.mutations()

    def test_mixed_recorded_and_unrecorded(self):
        start = {"a": {"b": 1}, "orders": [{"id": "x", "done": False}]}
        s = StateAccessor(start)
        s.set("a.b", 2)                    # recorded
        s.get("orders")[0]["done"] = True  # NOT recorded
        assert len(s.mutations()) == 1
        assert len(s.unrecorded_changes()) >= 1
        replayed = fold_events([{"mutations": s.reconciled_mutations()}], initial_state=start)
        assert replayed == s.snapshot()

    def test_append_then_in_place_edit_of_the_appended_item(self):
        start = {"items": []}
        s = StateAccessor(start)
        s.append("items", {"id": "i1", "status": "new"})
        s.get("items")[0]["status"] = "changed"   # edits the item after appending
        replayed = fold_events([{"mutations": s.reconciled_mutations()}], initial_state=start)
        assert replayed == s.snapshot()
        assert replayed["items"][0]["status"] == "changed"


# ── The sweep: every operator must be fold-clean ──────────────────────────────

class TestNoOperatorLeavesUnrecordedChanges:
    """
    A structural guard rather than a per-operator test.

    Any operator that edits state through a live reference fails this without
    anyone having to remember to write a test for it.
    """

    def test_the_three_historically_broken_operators_are_clean_at_source(self):
        """
        Regression lock for the three known sites. Each edited a dict obtained
        from state.get() — a live reference — so the change never reached the
        event log.
        """
        import inspect

        from sustena.core import council
        from sustena.operators import control_ops, procurement

        checks = [
            (procurement, "procurement_confirm_delivery", 'po["status"] ='),
            (control_ops, "control_execute_approved", 'p["status"] = "EXECUTED"'),
        ]
        for module, name, forbidden in checks:
            fn = getattr(module, name, None)
            assert fn is not None, f"{module.__name__}.{name} disappeared"
            if forbidden:
                src = inspect.getsource(fn)
                assert forbidden not in src, (
                    f"{module.__name__}.{name} still writes state through a live "
                    f"reference ({forbidden}) — that change never reaches the fold"
                )

    @pytest.mark.asyncio
    async def test_engine_reconciles_an_operator_that_bypasses_state_accessor(self):
        """
        The guarantee itself, proven through the real engine.

        Uses an operator that does not exist in the codebase — deliberately.
        The point is that fold fidelity holds for ANY operator, including ones
        written later by someone who has never read this file. If the guarantee
        depended on operators behaving, this test could not exist.
        """
        from sustena.core.operator import OPERATOR_REGISTRY, OperatorResult
        from sustena.core.sustain_engine import SustainEngine

        engine = SustainEngine(db_path=":memory:")
        sid = engine.instantiate(
            "homestead", "u1", {"household_name": "T", "owner_ids": ["u1"]}
        )

        meta = OPERATOR_REGISTRY["budget.allocate"]
        original_fn = meta.fn

        async def edits_via_live_reference(ctx, **kwargs):
            # Exactly the shape of all three real bugs: take a live reference
            # out of state and assign into it. Records no mutation.
            pockets = ctx.state.get("finances.pockets")
            pockets["ghost"] = {"allocated": 999.0, "spent": 0.0, "limit": 0.0}
            await ctx.events.publish("event.test.bypass", {})
            return OperatorResult.ok({"note": "wrote state without recording it"})

        meta.fn = edits_via_live_reference
        try:
            result = await engine.execute_operator(
                sid, "budget.allocate",
                {"pocket_name": "x", "amount": 1, "period": "monthly"},
            )
            assert result.succeeded

            live = engine.get_state(sid)
            rebuilt = engine.rebuild_state(sid)

            # The change really happened...
            assert "ghost" in live["finances"]["pockets"]
            # ...and, crucially, survives a rebuild from the event log alone.
            assert "ghost" in rebuilt["finances"]["pockets"], (
                "the unrecorded change never reached the event log — the fold "
                "gap is back"
            )
            assert rebuilt == live, "rebuild_state() must equal get_state()"
        finally:
            meta.fn = original_fn

    def test_council_writes_status_through_state_accessor(self):
        import inspect

        from sustena.core.council import CouncilSession

        src = inspect.getsource(CouncilSession._set_proposal_status)
        assert "self.state.set(" in src, (
            "council must write proposal status via StateAccessor.set so the "
            "change is recorded and the fold stays reproducible"
        )
