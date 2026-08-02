"""
tests/test_holon_operators.py

Phase 2 (Nested Holons, 2 Aug 2026) — holon.create_child, holon.dissolve_child,
holon.transfer. Canon: IO/strategy/SPEC-nested-holons.addendum.2026-08-02.md
(§§4D.7/4C.6/4E.6/4B.6) + IO/strategy/sustena-nested-holons.design.2026-08-02.md.

holon.transfer is the keystone and gets the deepest coverage: conservation
(Σ balances invariant before==after), atomicity (a half-failure commits
neither side), idempotency (a replayed call never double-applies), and both
holons' own gates. Every conservation-relevant test asserts the SUM, not
just each side individually — the sum holding is the actual claim being
tested, not a side effect of two individually-correct numbers.

Uses real homestead/habitat instances (via engine.instantiate), not
create_definition() synthetic specs — create_definition()'s flat-scalar
dimension builder doesn't support the nested finances.{liquid,pockets}
shape these operators are inherently about (a pre-existing, disclosed
constraint of the Slice 6 definition builder — see test_sustain_engine.py's
own TestEventSourcing class for the same reasoning).
"""

import pytest

from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult
from sustena.core.state import StateAccessor
from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


def _homestead(engine: SustainEngine, owner="u1") -> str:
    return engine.instantiate("homestead", owner, {"owner_ids": [owner]})


def _habitat(engine: SustainEngine, owner="u1", name="Bonnie") -> str:
    return engine.instantiate("habitat", owner, {"owner_ids": [owner], "name": name})


async def _run(engine: SustainEngine, sustain_id: str, operator_name: str, **params):
    return await engine.execute_operator(sustain_id, operator_name, params)


def _liquid(engine: SustainEngine, sustain_id: str) -> float:
    return StateAccessor(engine.get_state(sustain_id)).get("finances.liquid.balance")


class TestHolonCreateChild:
    @pytest.mark.asyncio
    async def test_creates_and_links_an_allowed_template(self, engine):
        parent = _homestead(engine)
        result = await _run(engine, parent, "holon.create_child", template="habitat", name="Project IO")
        assert result.succeeded, result.reason
        child_id = result.data["child_sustain_id"]
        assert engine.get_parent(child_id)["parent_sustain_id"] == parent
        state = engine.get_state(child_id)
        assert StateAccessor(state).get("identity.name") == "Project IO"

    @pytest.mark.asyncio
    async def test_created_holon_never_shows_the_raw_unresolved_role_in_family_token(self, engine):
        # Real bug found live: holon.create_child never passes role_in_family,
        # and instantiate() used to leave the literal "{{role_in_family}}"
        # string in the child's real state -- fixed at instantiate()'s own
        # root (see test_sustain_engine.py's TestInstantiateMissingParam).
        parent = _homestead(engine)
        result = await _run(engine, parent, "holon.create_child", template="habitat", name="Project IO")
        assert result.succeeded, result.reason
        state = engine.get_state(result.data["child_sustain_id"])
        assert StateAccessor(state).get("identity.role_in_family") == ""

    @pytest.mark.asyncio
    async def test_refuses_a_template_not_in_child_policy(self, engine):
        parent = _homestead(engine)
        result = await _run(engine, parent, "holon.create_child", template="homestead", name="Nested Household")
        assert result.status == "failed"
        assert result.constraint_violated == "child_policy_accepts"
        assert engine.list_children(parent) == []

    @pytest.mark.asyncio
    async def test_refuses_empty_name(self, engine):
        parent = _homestead(engine)
        result = await _run(engine, parent, "holon.create_child", template="habitat", name="   ")
        assert result.status == "failed"
        assert result.constraint_violated == "name_required"

    @pytest.mark.asyncio
    async def test_respects_a_declared_cap(self, engine):
        parent = _homestead(engine)
        spec = engine._get_spec(parent)
        spec["child_policy"]["cap"] = 1
        r1 = await _run(engine, parent, "holon.create_child", template="habitat", name="Project A")
        assert r1.succeeded
        r2 = await _run(engine, parent, "holon.create_child", template="habitat", name="Project B")
        assert r2.status == "failed"
        assert r2.constraint_violated == "child_policy_cap"
        assert len(engine.list_children(parent)) == 1

    @pytest.mark.asyncio
    async def test_publishes_child_created_event(self, engine):
        parent = _homestead(engine)
        result = await _run(engine, parent, "holon.create_child", template="habitat", name="Project IO")
        events = engine.get_events(parent, limit=5)
        assert any(e["event_name"] == "event.holon.child_created" for e in events)

    @pytest.mark.asyncio
    async def test_a_sustain_with_no_child_policy_refuses_every_template(self, engine):
        # habitat.json declares no child_policy at all -- a habitat cannot
        # spawn children in this build, and that must be a clean refusal,
        # not a crash on a missing key.
        leaf = _habitat(engine)
        result = await _run(engine, leaf, "holon.create_child", template="habitat", name="Sub")
        assert result.status == "failed"
        assert result.constraint_violated == "operator_allowed"  # holon.create_child isn't even declared on habitat.json


class TestHolonTransferConservation:
    """The keystone. Every test here proves money is neither created nor
    destroyed — the sum across both holons is checked explicitly, not
    inferred from two individually-plausible numbers."""

    @pytest.mark.asyncio
    async def test_parent_to_child_transfer_conserves_total(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=10000, source="salary")

        total_before = _liquid(engine, parent) + _liquid(engine, child)
        result = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=3000)
        assert result.succeeded, result.reason
        total_after = _liquid(engine, parent) + _liquid(engine, child)

        assert total_before == total_after
        assert _liquid(engine, parent) == 7000
        assert _liquid(engine, child) == 3000

    @pytest.mark.asyncio
    async def test_child_to_parent_return_conserves_total(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=10000, source="salary")
        await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=4000)

        total_before = _liquid(engine, parent) + _liquid(engine, child)
        result = await _run(engine, child, "holon.transfer", to_sustain_id=parent, amount=1500)
        assert result.succeeded, result.reason
        total_after = _liquid(engine, parent) + _liquid(engine, child)

        assert total_before == total_after
        assert _liquid(engine, child) == 2500
        assert _liquid(engine, parent) == 7500

    @pytest.mark.asyncio
    async def test_rebuild_state_matches_get_state_on_both_sides_after_transfer(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=5000, source="salary")
        await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=2000)

        assert engine.rebuild_state(parent) == engine.get_state(parent)
        assert engine.rebuild_state(child) == engine.get_state(child)

    @pytest.mark.asyncio
    async def test_ten_repeated_transfers_conserve_total_every_step(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=100000, source="salary")

        for _ in range(10):
            total_before = _liquid(engine, parent) + _liquid(engine, child)
            result = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=777)
            assert result.succeeded
            total_after = _liquid(engine, parent) + _liquid(engine, child)
            assert total_before == total_after

        assert _liquid(engine, parent) + _liquid(engine, child) == 100000


class TestHolonTransferRefusals:
    @pytest.mark.asyncio
    async def test_refuses_unlinked_sustains(self, engine):
        a = _homestead(engine)
        b = _homestead(engine, owner="u2")
        result = await _run(engine, a, "holon.transfer", to_sustain_id=b, amount=100)
        assert result.status == "failed"
        assert result.constraint_violated == "holon_link_exists"

    @pytest.mark.asyncio
    async def test_refuses_zero_or_negative_amount(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        for bad_amount in (0, -50):
            result = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=bad_amount)
            assert result.status == "failed"
            assert result.constraint_violated == "amount_positive"

    @pytest.mark.asyncio
    async def test_refuses_self_transfer(self, engine):
        parent = _homestead(engine)
        result = await _run(engine, parent, "holon.transfer", to_sustain_id=parent, amount=100)
        assert result.status == "failed"
        assert result.constraint_violated == "distinct_holons"

    @pytest.mark.asyncio
    async def test_insufficient_funds_refuses_and_leaves_both_sides_byte_unchanged(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=1000, source="salary")

        parent_state_before = engine.get_state(parent)
        child_state_before = engine.get_state(child)
        parent_events_before = len(engine.get_events(parent, limit=1000))
        child_events_before = len(engine.get_events(child, limit=1000))

        result = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=999999)
        assert result.status == "failed"
        assert result.constraint_violated == "from_balance_sufficient"

        assert engine.get_state(parent) == parent_state_before
        assert engine.get_state(child) == child_state_before
        assert len(engine.get_events(parent, limit=1000)) == parent_events_before
        assert len(engine.get_events(child, limit=1000)) == child_events_before

    @pytest.mark.asyncio
    async def test_refused_in_simulation_never_touches_the_real_database(self, engine):
        # ctx.engine is deliberately None inside simulate() (see holon.py's
        # own docstring) -- a disclosed scope boundary, not an oversight.
        # Prove it refuses cleanly rather than crashing or, worse, reaching
        # the real database from inside a sandboxed fork.
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=5000, source="salary")

        parent_state_before = engine.get_state(parent)
        child_state_before = engine.get_state(child)

        results = await engine.simulate(parent, [
            {"operator": "holon.transfer", "params": {"to_sustain_id": child, "amount": 1000}},
        ])
        assert not results[0]["result"].succeeded
        assert results[0]["result"].constraint_violated == "engine_available"

        assert engine.get_state(parent) == parent_state_before
        assert engine.get_state(child) == child_state_before


class TestHolonTransferAtomicity:
    @pytest.mark.asyncio
    async def test_a_forced_commit_failure_leaves_neither_side_changed(self, engine, monkeypatch):
        """
        Half-failure must refuse the whole transfer -- proven by forcing
        the atomic commit itself to raise partway through and confirming
        BOTH sides are byte-identical to before the attempt, with
        rebuild_state()==get_state() still holding for both. This is the
        direct test of "a half-failure commits neither side."
        """
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=5000, source="salary")

        parent_state_before = engine.get_state(parent)
        child_state_before = engine.get_state(child)
        parent_events_before = len(engine.get_events(parent, limit=1000))
        child_events_before = len(engine.get_events(child, limit=1000))

        def _boom(*args, **kwargs):
            raise RuntimeError("simulated crash mid-commit")

        monkeypatch.setattr(engine, "_commit_atomic_multi_sustain", _boom)

        result = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=1000)
        assert result.status == "failed"
        assert result.constraint_violated == "atomic_commit"

        assert engine.get_state(parent) == parent_state_before
        assert engine.get_state(child) == child_state_before
        assert len(engine.get_events(parent, limit=1000)) == parent_events_before
        assert len(engine.get_events(child, limit=1000)) == child_events_before
        assert engine.rebuild_state(parent) == engine.get_state(parent)
        assert engine.rebuild_state(child) == engine.get_state(child)

    @pytest.mark.asyncio
    async def test_a_genuine_sqlite_integrity_error_mid_commit_rolls_back_both_sides(self, engine):
        """
        Not mocked -- forces a REAL sqlite3.IntegrityError from inside
        _commit_atomic_multi_sustain's own extra_writes callback (a raw
        duplicate-primary-key INSERT against holon_transfers, bypassing
        the idempotency pre-check entirely) so self._db.rollback() runs
        against a genuinely partially-executed transaction: both state
        writes will have already been ._db.execute()'d before the ledger
        INSERT raises. Proves the rollback undoes the STATE writes too,
        not just the failed statement itself.
        """
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=5000, source="salary")

        parent_state_before = engine.get_state(parent)
        child_state_before = engine.get_state(child)

        # Pre-seed a row whose PRIMARY KEY (id) collides with a fixed value
        # the extra_writes callback below will also try to insert -- a raw
        # sqlite3.IntegrityError, not a caught/handled one.
        engine._write_holon_transfer_no_commit(
            "fixed-colliding-id", "irrelevant-a", "irrelevant-b", 1.0, "irrelevant-key", "2020-01-01T00:00:00",
        )
        engine._db.commit()

        from sustena.core.state import StateAccessor as SA
        parent_accessor = SA(engine.get_state(parent))
        child_accessor = SA(engine.get_state(child))
        parent_accessor.decrement("finances.liquid.balance", 1000)
        child_accessor.increment("finances.liquid.balance", 1000)

        with pytest.raises(Exception):
            engine._commit_atomic_multi_sustain(
                [
                    (parent, parent_accessor.snapshot(), [{"event_name": "event.test.probe", "payload": {}, "mutations": parent_accessor.mutations()}]),
                    (child, child_accessor.snapshot(), [{"event_name": "event.test.probe", "payload": {}, "mutations": child_accessor.mutations()}]),
                ],
                extra_writes=[
                    lambda: engine._db.execute(
                        "INSERT INTO holon_transfers (id, from_sustain_id, to_sustain_id, amount, idempotency_key, created_at) "
                        "VALUES ('fixed-colliding-id', 'x', 'y', 1.0, 'another-key', '2020-01-01T00:00:00')",
                    ),
                ],
            )

        assert engine.get_state(parent) == parent_state_before
        assert engine.get_state(child) == child_state_before
        assert engine.rebuild_state(parent) == engine.get_state(parent)
        assert engine.rebuild_state(child) == engine.get_state(child)


class TestHolonTransferIdempotency:
    @pytest.mark.asyncio
    async def test_replayed_call_with_same_key_moves_money_exactly_once(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=10000, source="salary")

        r1 = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=2000, idempotency_key="confirm-tap-1")
        assert r1.succeeded
        assert r1.data["idempotent_replay"] is False
        assert _liquid(engine, child) == 2000

        r2 = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=2000, idempotency_key="confirm-tap-1")
        assert r2.succeeded
        assert r2.data["idempotent_replay"] is True
        # Balance moved ONCE, not twice -- the direct proof against double-counting.
        assert _liquid(engine, child) == 2000
        assert _liquid(engine, parent) == 8000

    @pytest.mark.asyncio
    async def test_two_calls_with_no_key_are_both_genuine_transfers(self, engine):
        # Dedup is opt-in via an explicit key -- omitting it means every
        # call is its own transfer (the common single-confirm-tap case).
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=10000, source="salary")

        await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=1000)
        await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=1000)
        assert _liquid(engine, child) == 2000
        assert _liquid(engine, parent) == 8000

    @pytest.mark.asyncio
    async def test_idempotent_replay_returns_the_prior_committed_amount_not_a_new_one(self, engine):
        parent = _homestead(engine)
        child = _habitat(engine)
        engine.link_child(parent, child, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=10000, source="salary")

        r1 = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=500, idempotency_key="k1")
        # A caller retrying with a DIFFERENT amount but the SAME key still
        # gets the original committed outcome -- the key, not the amount,
        # is authoritative on replay.
        r2 = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=9999, idempotency_key="k1")
        assert r2.data["amount"] == 500
        assert _liquid(engine, child) == 500


class TestHolonTransferGates:
    @pytest.mark.asyncio
    async def test_grandparent_binding_invariant_refuses_a_transfer_that_would_breach_it(self, engine):
        """
        Three-level nesting: grandparent G has a BINDING aggregate
        invariant over its child P's contribution; P has its own child C.
        Moving funds OUT of P (P -> C) reduces P's own liquid, which
        reduces G's rollup -- if that newly breaches G's binding rule, the
        transfer must be refused, and G/P/C must all be byte-unchanged.
        """
        grandparent = _homestead(engine)
        parent = _homestead(engine, owner="u1")
        child = _habitat(engine)

        engine.link_child(grandparent, parent, slot="p1", member="Parent")
        engine.link_child(parent, child, slot="c1", member="Child")

        gp_spec = engine._get_spec(grandparent)
        gp_spec["state_schema"]["household_liquid_total"] = {"type": "number", "description": "computed aggregate"}
        gp_spec["invariants"].append({
            "id": "household_floor", "expression": "household_liquid_total >= 5000",
            "description": "test-only binding floor", "authority": "binding",
        })
        gp_spec.pop("_compiled_invariants", None)
        engine._compile_spec_invariants(gp_spec)

        await _run(engine, parent, "budget.record_income", amount=5500, source="salary")
        # Rollup is now exactly at the floor (5500 >= 5000) -- moving 1000
        # out of parent would drop it to 4500, breaching the binding rule.
        rollup_before = engine.compute_rollup(grandparent)
        assert rollup_before["aggregates"]["household_liquid_total"]["value"] == 5500

        parent_before = engine.get_state(parent)
        child_before = engine.get_state(child)

        result = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=1000)
        assert result.status == "failed"
        assert result.constraint_violated == "parent_binding_gate"

        assert engine.get_state(parent) == parent_before
        assert engine.get_state(child) == child_before

    @pytest.mark.asyncio
    async def test_a_transfer_that_keeps_the_grandparent_compliant_succeeds(self, engine):
        grandparent = _homestead(engine)
        parent = _homestead(engine, owner="u1")
        child = _habitat(engine)
        engine.link_child(grandparent, parent, slot="p1", member="Parent")
        engine.link_child(parent, child, slot="c1", member="Child")

        gp_spec = engine._get_spec(grandparent)
        gp_spec["state_schema"]["household_liquid_total"] = {"type": "number", "description": "computed aggregate"}
        gp_spec["invariants"].append({
            "id": "household_floor", "expression": "household_liquid_total >= 100",
            "description": "test-only binding floor", "authority": "binding",
        })
        gp_spec.pop("_compiled_invariants", None)
        engine._compile_spec_invariants(gp_spec)

        await _run(engine, parent, "budget.record_income", amount=5500, source="salary")
        result = await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=1000)
        assert result.succeeded, result.reason


class TestHolonDissolveChild:
    @pytest.mark.asyncio
    async def test_dissolves_a_zero_balance_child_with_no_transfer_needed(self, engine):
        parent = _homestead(engine)
        create = await _run(engine, parent, "holon.create_child", template="habitat", name="Project IO")
        child = create.data["child_sustain_id"]

        result = await _run(engine, parent, "holon.dissolve_child", child_sustain_id=child)
        assert result.succeeded, result.reason
        assert result.data["funds_returned"] == 0.0
        assert engine.get_parent(child) is None

    @pytest.mark.asyncio
    async def test_returns_funds_before_dissolving_a_funded_child(self, engine):
        parent = _homestead(engine)
        create = await _run(engine, parent, "holon.create_child", template="habitat", name="Project IO")
        child = create.data["child_sustain_id"]
        await _run(engine, parent, "budget.record_income", amount=5000, source="salary")
        await _run(engine, parent, "holon.transfer", to_sustain_id=child, amount=2000)
        assert _liquid(engine, child) == 2000
        assert _liquid(engine, parent) == 3000

        total_before = _liquid(engine, parent) + _liquid(engine, child)
        result = await _run(engine, parent, "holon.dissolve_child", child_sustain_id=child)
        assert result.succeeded, result.reason
        assert result.data["funds_returned"] == 2000

        assert _liquid(engine, parent) == 5000
        assert _liquid(engine, child) == 0
        assert _liquid(engine, parent) == total_before  # conserved through dissolution too
        assert engine.get_parent(child) is None

    @pytest.mark.asyncio
    async def test_refuses_dissolving_an_unlinked_sustain(self, engine):
        parent = _homestead(engine)
        stray = _habitat(engine)
        result = await _run(engine, parent, "holon.dissolve_child", child_sustain_id=stray)
        assert result.status == "failed"
        assert result.constraint_violated == "holon_link_exists"

    @pytest.mark.asyncio
    async def test_child_state_and_history_are_preserved_not_deleted(self, engine):
        parent = _homestead(engine)
        create = await _run(engine, parent, "holon.create_child", template="habitat", name="Project IO")
        child = create.data["child_sustain_id"]
        await _run(engine, child, "budget.record_income", amount=100, source="misc")

        await _run(engine, parent, "holon.dissolve_child", child_sustain_id=child)

        # The child sustain itself still exists, fully intact -- only the
        # link record is gone.
        state = engine.get_state(child)
        assert state is not None
        events = engine.get_events(child, limit=100)
        assert len(events) >= 2  # genesis + the income event


class TestWildcardRollup:
    @pytest.mark.asyncio
    async def test_household_pockets_total_sums_across_children(self, engine):
        parent = _homestead(engine)
        c1 = _habitat(engine, name="Bonnie")
        c2 = _habitat(engine, name="Cira")
        engine.link_child(parent, c1, slot="s1", member="Bonnie")
        engine.link_child(parent, c2, slot="s2", member="Cira")

        await _run(engine, c1, "budget.record_income", amount=1000, source="x")
        await _run(engine, c1, "budget.allocate", pocket_name="food", amount=300)
        await _run(engine, c2, "budget.record_income", amount=500, source="x")
        await _run(engine, c2, "budget.allocate", pocket_name="wifi", amount=150)
        await _run(engine, c2, "budget.allocate", pocket_name="airtime", amount=50)

        rollup = engine.compute_rollup(parent)
        agg = rollup["aggregates"]["household_pockets_total"]
        # Household-total fix (2 Aug 2026): the parent's own pockets now
        # also contribute -- homestead's own finances.pockets is {} (never
        # allocated in this test), which resolves to a real 0.0 (an empty
        # dict, not a missing path), so it's INCLUDED at 0, not excluded.
        assert agg["value"] == 500  # parent's own 0 + 300 + 150 + 50
        assert len(agg["included"]) == 3  # parent + 2 children
        assert sum(1 for i in agg["included"] if i.get("is_self")) == 1

    @pytest.mark.asyncio
    async def test_a_child_with_no_pockets_yet_contributes_zero_not_excluded(self, engine):
        parent = _homestead(engine)
        c1 = _habitat(engine, name="Bonnie")
        engine.link_child(parent, c1, slot="s1", member="Bonnie")

        rollup = engine.compute_rollup(parent)
        agg = rollup["aggregates"]["household_pockets_total"]
        assert agg["value"] == 0.0
        assert len(agg["included"]) == 2  # the parent's own (0) + the one child (0)
        assert len(agg["excluded"]) == 0

    @pytest.mark.asyncio
    async def test_parent_own_liquid_is_now_folded_into_the_household_total(self, engine):
        # Household-total fix (2 Aug 2026): household_liquid_total used to
        # sum ONLY linked children ("what your kids collectively hold").
        # Bonnie's own framing: "household total = everything" -- the
        # parent's own liquid balance must now be included too.
        parent = _homestead(engine)
        c1 = _habitat(engine, name="Bonnie")
        engine.link_child(parent, c1, slot="s1", member="Bonnie")
        await _run(engine, parent, "budget.record_income", amount=5000, source="salary")
        await _run(engine, c1, "budget.record_income", amount=777, source="x")

        rollup = engine.compute_rollup(parent)
        agg = rollup["aggregates"]["household_liquid_total"]
        assert agg["value"] == 5777  # 5000 (parent's own) + 777 (child)
        assert agg["includes_parent_own_contribution"] is True
        self_entry = next(i for i in agg["included"] if i.get("is_self"))
        assert self_entry["sustain_id"] == parent
        assert self_entry["value"] == 5000

    @pytest.mark.asyncio
    async def test_a_previously_written_test_with_zero_parent_balance_still_matches_numerically(self, engine):
        # The parent's own liquid balance happens to be 0 in this scenario
        # (never funded), so folding it in doesn't change the NUMBER -- but
        # the mechanism genuinely changed (this now reads parent+child, not
        # child-only, and just happens to produce the same total).
        parent = _homestead(engine)
        c1 = _habitat(engine, name="Bonnie")
        engine.link_child(parent, c1, slot="s1", member="Bonnie")
        await _run(engine, c1, "budget.record_income", amount=777, source="x")

        rollup = engine.compute_rollup(parent)
        assert rollup["aggregates"]["household_liquid_total"]["value"] == 777


class TestHolonAmberGenerality:
    """No 'homestead'/'habitat' string anywhere in the holon operator
    implementation itself — proven by grepping the module, same discipline
    every prior composition slice held itself to."""

    def test_no_template_specific_strings_in_holon_module(self):
        import pathlib
        src = pathlib.Path(__file__).parent.parent / "sustena" / "operators" / "holon.py"
        text = src.read_text(encoding="utf-8")
        for banned in ("homestead", "habitat"):
            assert banned not in text.lower(), f"holon.py should be template-agnostic, found {banned!r}"
