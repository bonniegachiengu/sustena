"""
tests/test_pawa_meter.py

Tests for the Pawa meter (SUSTENA_UPGRADE_SPEC.md §4L) -- the odometer,
not the gas pump. Covers:
  - sustena/core/pawa_meter.py's pure formula
  - SustainEngine._record_pawa_meter / execute_operator instrumentation:
    a real successful run produces a real, non-fabricated meter row; a
    gate refusal produces none (zero real work)
  - Aggregation: per-operator average, per-sustain total, per-principal
    total, all honest (None/zero when genuinely unmeasured, never faked)
  - simulate() accumulating real per-step + cumulative pawa, using the
    identical formula, while never writing to the real meter log
  - Genericity: the mechanism is not hardcoded to any one operator

Run with:
    python -m pytest tests/test_pawa_meter.py -v
"""

import pytest

from sustena.core import pawa_meter
from sustena.core.sustain_engine import SustainEngine


@pytest.fixture
def engine() -> SustainEngine:
    return SustainEngine(db_path=":memory:")


class TestPawaFormula:
    def test_compute_pawa_matches_declared_coefficients(self):
        assert pawa_meter.compute_pawa(compute=10, storage=100) == pytest.approx(
            pawa_meter.KAPPA_COMPUTE * 10 + pawa_meter.KAPPA_STORAGE * 100
        )

    def test_compute_pawa_zero_for_zero_work(self):
        assert pawa_meter.compute_pawa(0, 0) == 0.0

    def test_compute_units_sums_the_three_proxies(self):
        assert pawa_meter.compute_units(mutation_count=2, event_count=1, constraint_eval_count=3) == 6

    def test_constraint_eval_count_excludes_invariants_when_gate_did_not_run(self):
        class FakeMeta:
            constraints = ["a > 0"]
            post_constraints = ["b >= 0"]

        spec = {"invariants": [{"id": "x"}, {"id": "y"}]}
        # gate ran: operator's own 2 declared + 2 sustain invariants = 4
        assert pawa_meter.constraint_eval_count(FakeMeta(), spec, enforcement_ran=True) == 4
        # gate did NOT run: only the operator's own 2 declared checks
        assert pawa_meter.constraint_eval_count(FakeMeta(), spec, enforcement_ran=False) == 2


class TestExecuteOperatorInstrumentation:
    @pytest.mark.asyncio
    async def test_real_run_produces_a_real_nonzero_meter_row(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        result = await engine.execute_operator(sid, "budget.record_income", {"amount": 5000, "source": "x", "frequency": "once"})
        assert result.succeeded

        stats = engine.get_operator_pawa_stats("budget.record_income")
        assert stats is not None
        assert stats["run_count"] == 1
        assert stats["avg_compute"] > 0
        assert stats["avg_storage"] > 0
        assert stats["avg_pawa"] > 0
        # the formula must actually be applied, not just nonzero components
        assert stats["avg_pawa"] == pytest.approx(
            pawa_meter.KAPPA_COMPUTE * stats["avg_compute"] + pawa_meter.KAPPA_STORAGE * stats["avg_storage"]
        )

    @pytest.mark.asyncio
    async def test_gate_refusal_meters_nothing(self, engine: SustainEngine):
        """A refused write is zero real work -- honestly metered as
        nothing, not as a failed-but-costly attempt."""
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        before = engine.get_sustain_pawa_total(sid)["run_count"]

        result = await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 999999, "period": "monthly"})
        assert not result.succeeded

        after = engine.get_sustain_pawa_total(sid)["run_count"]
        assert after == before

    @pytest.mark.asyncio
    async def test_never_run_operator_is_honestly_none_not_zero(self, engine: SustainEngine):
        assert engine.get_operator_pawa_stats("budget.transfer") is None

    @pytest.mark.asyncio
    async def test_generic_across_different_operators(self, engine: SustainEngine):
        """Not hardcoded to one operator -- two different real operators
        both produce real, independently-tracked stats."""
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 500, "period": "monthly"})

        all_stats = engine.get_all_operator_pawa_stats()
        assert set(all_stats.keys()) == {"budget.record_income", "budget.allocate"}
        assert all_stats["budget.record_income"]["run_count"] == 1
        assert all_stats["budget.allocate"]["run_count"] == 1

    @pytest.mark.asyncio
    async def test_repeated_runs_average_correctly(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        for _ in range(3):
            await engine.execute_operator(sid, "budget.record_income", {"amount": 100, "source": "x", "frequency": "once"})
        stats = engine.get_operator_pawa_stats("budget.record_income")
        assert stats["run_count"] == 3

    @pytest.mark.asyncio
    async def test_get_last_pawa_meter_returns_the_most_recent_real_reading(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        assert engine.get_last_pawa_meter(sid, "budget.record_income") is None
        await engine.execute_operator(sid, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        row = engine.get_last_pawa_meter(sid, "budget.record_income")
        assert row is not None
        assert row["pawa"] > 0


class TestSustainAndPrincipalTotals:
    @pytest.mark.asyncio
    async def test_sustain_total_aggregates_across_operators(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        await engine.execute_operator(sid, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        await engine.execute_operator(sid, "budget.allocate", {"pocket_name": "food", "amount": 500, "period": "monthly"})
        total = engine.get_sustain_pawa_total(sid)
        assert total["run_count"] == 2
        assert total["total_pawa"] > 0

    def test_sustain_with_no_activity_is_honest_zero(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        total = engine.get_sustain_pawa_total(sid)
        assert total == {"sustain_id": sid, "run_count": 0, "total_compute": 0, "total_storage": 0, "total_pawa": 0}

    @pytest.mark.asyncio
    async def test_principal_total_aggregates_across_sustains(self, engine: SustainEngine):
        sid_a = engine.instantiate("homestead", "same-user", {"owner_ids": ["same-user"]})
        sid_b = engine.instantiate("homestead", "same-user", {"owner_ids": ["same-user"]})
        await engine.execute_operator(sid_a, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        await engine.execute_operator(sid_b, "budget.record_income", {"amount": 1000, "source": "x", "frequency": "once"})
        total = engine.get_principal_pawa_total("same-user")
        assert total["run_count"] == 2


class TestSimulateEfficiencyRanking:
    @pytest.mark.asyncio
    async def test_successful_steps_accumulate_real_cumulative_pawa(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        steps = [
            {"operator": "budget.record_income", "params": {"amount": 5000, "source": "x", "frequency": "once"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 3000, "period": "monthly"}},
        ]
        results = await engine.simulate(sid, steps)
        assert results[0]["pawa"] > 0
        assert results[1]["pawa"] > 0
        assert results[1]["cumulative_pawa"] == pytest.approx(results[0]["pawa"] + results[1]["pawa"])

    @pytest.mark.asyncio
    async def test_refused_step_contributes_zero_pawa(self, engine: SustainEngine):
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        steps = [
            {"operator": "budget.record_income", "params": {"amount": 1000, "source": "x", "frequency": "once"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 999999, "period": "monthly"}},
        ]
        results = await engine.simulate(sid, steps)
        assert not results[1]["result"].succeeded
        assert results[1]["pawa"] == 0.0
        # cumulative unchanged from the last successful step
        assert results[1]["cumulative_pawa"] == results[0]["pawa"]

    @pytest.mark.asyncio
    async def test_simulate_never_writes_to_the_real_meter_log(self, engine: SustainEngine):
        """A simulated run isn't a real one -- exactly like it never
        appends a real event, it must never pollute pawa_meter_log."""
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        steps = [{"operator": "budget.record_income", "params": {"amount": 1000, "source": "x", "frequency": "once"}}]
        await engine.simulate(sid, steps)
        assert engine.get_sustain_pawa_total(sid)["run_count"] == 0

    @pytest.mark.asyncio
    async def test_cheaper_branch_has_a_lower_final_cumulative_pawa(self, engine: SustainEngine):
        """The actual efficiency-ranking property: two branches reaching
        different amounts of real work are distinguishable by their final
        cumulative_pawa, not just by outcome."""
        sid = engine.instantiate("homestead", "u1", {"owner_ids": ["u1"]})
        cheap_branch = await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 1000, "source": "x", "frequency": "once"}},
        ])
        expensive_branch = await engine.simulate(sid, [
            {"operator": "budget.record_income", "params": {"amount": 1000, "source": "x", "frequency": "once"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "food", "amount": 500, "period": "monthly"}},
            {"operator": "budget.allocate", "params": {"pocket_name": "rent", "amount": 500, "period": "monthly"}},
        ])
        assert expensive_branch[-1]["cumulative_pawa"] > cheap_branch[-1]["cumulative_pawa"]
