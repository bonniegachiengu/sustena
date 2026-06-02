"""
tests/test_operator_registry.py

Validates the global OPERATOR_REGISTRY for all registered operators.

Epic 1.1 milestone criterion:
  "All operators have pawa_cost, license_tier, author, side_effects metadata"

This test module is the canonical registry contract — if an operator is
registered, it must meet all metadata requirements. Add new operator modules
to the imports below as they are built.

Run with: pytest tests/test_operator_registry.py -v
"""

import pytest

from sustena.core.operator import OPERATOR_REGISTRY
import sustena.operators  # triggers registration of all operator modules


# ── Registry completeness ─────────────────────────────────────────────────────

BUDGET_OPERATORS = [
    "budget.record_income",
    "budget.allocate",
    "budget.spend",
    "budget.transfer",
    "budget.summary",
]

CHAMA_OPERATORS = [
    "chama.contribution.record",
    "chama.loan.request",
    "chama.loan.disburse",
    "chama.loan.repay",
    "chama.fine.record",
    "chama.meeting.schedule",
    "chama.dividend.calculate",
    "chama.rotation.advance",
]

PROCUREMENT_OPERATORS = [
    "mkulima.broadcast_supply_signal",
    "mkulima.receive_signal",
    "procurement.raise_po",
    "procurement.confirm_delivery",
]

CALENDAR_OPERATORS = [
    "homestead.calendar.add_event",
    "homestead.calendar.upcoming_events",
    "homestead.calendar.remove_event",
]

BIASHARA_OPERATORS = [
    "biashara.inventory.restock",
    "biashara.inventory.adjust",
    "biashara.orders.place",
    "biashara.orders.fulfill",
    "biashara.purchases.record",
    "biashara.po.raise",
    "biashara.po.receive",
    "biashara.accounts.journal_entry",
    "biashara.tax.calculate_vat",
    "biashara.assets.depreciate",
    "biashara.expenses.record",
]

VYYB_OPERATORS = [
    "vyyb.production.start_batch",
    "vyyb.production.complete_batch",
    "vyyb.kds.dispatch_task",
    "vyyb.kds.complete_task",
    "vyyb.staff.clock_in",
    "vyyb.staff.clock_out",
    "vyyb.staff.schedule_shift",
    "vyyb.procurement.raise_po",
    "vyyb.procurement.receive_po",
]

UI_RENDER_OPERATORS = [
    "ui.render.operator_card",
    "ui.render.operative_dashboard",
    "ui.render.sustain_home",
    "ui.render.preview",
]

API_OPERATORS = [
    "api.get",
    "api.post",
    "api.webhook_listen",
]

MONITOR_OPERATORS = [
    "monitor.state_path",
    "monitor.constraint",
]

VISUALIZE_OPERATORS = [
    "visualize.pocket_ring",
    "visualize.event_feed",
    "visualize.constraint_health",
]

SIMULATE_OPERATORS = [
    "simulate.fork",
    "simulate.run_path",
    "simulate.score",
]

EDIT_OPERATORS = [
    "edit.state_patch",
    "edit.operator_spec",
]

CONTROL_OPERATORS = [
    "control.execute_approved",
    "control.rollback",
]

MENTOR_OPERATORS = [
    "mentor.evaluate_budget",
    "mentor.deliberate_budget",
]

OPERATIVE_OPERATORS = [
    "operative.spawn",
]

TASKS_OPERATORS = [
    "homestead.tasks.add",
    "homestead.tasks.complete",
    "homestead.tasks.list",
    "homestead.tasks.carryover",
]

ORCHIE_OPERATORS = [
    "orchie.morning_brief",
]

ALL_KNOWN_OPERATORS = (
    BUDGET_OPERATORS + CHAMA_OPERATORS + PROCUREMENT_OPERATORS + CALENDAR_OPERATORS
    + BIASHARA_OPERATORS + VYYB_OPERATORS  # Epic 1.5.2
    + UI_RENDER_OPERATORS                  # Sprint 2 UIParser
    + API_OPERATORS                        # Sprint 3.1
    + MONITOR_OPERATORS                    # Sprint 3.2
    + VISUALIZE_OPERATORS                  # Sprint 3.3
    + SIMULATE_OPERATORS                   # Sprint 3.4
    + EDIT_OPERATORS                       # Sprint 3.5
    + CONTROL_OPERATORS                    # Sprint 3.6
    + MENTOR_OPERATORS                     # Sprint 5.3
    + OPERATIVE_OPERATORS                  # Sprint 5.7
    + TASKS_OPERATORS                      # Sprint 6.1
    + ORCHIE_OPERATORS                     # Sprint 6.2
)


class TestRegistryCompleteness:

    def test_all_budget_operators_present(self):
        """All five Epic 1.1 budget operators must be in the registry."""
        missing = [op for op in BUDGET_OPERATORS if op not in OPERATOR_REGISTRY]
        assert not missing, f"Missing from OPERATOR_REGISTRY: {missing}"

    def test_all_chama_operators_present(self):
        """All seven Epic 1.1.2 chama operators must be in the registry."""
        missing = [op for op in CHAMA_OPERATORS if op not in OPERATOR_REGISTRY]
        assert not missing, f"Missing from OPERATOR_REGISTRY: {missing}"

    def test_no_unknown_operators_present(self):
        """Registry should only contain operators we've deliberately shipped."""
        registered = set(OPERATOR_REGISTRY.keys())
        unknown = registered - set(ALL_KNOWN_OPERATORS)
        assert not unknown, (
            f"Unexpected operators in registry: {unknown}. "
            "If you added a new operator, add it to ALL_KNOWN_OPERATORS in this file."
        )


# ── Metadata contract ─────────────────────────────────────────────────────────

class TestOperatorMetadata:

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_name_is_namespaced(self, name):
        """Operator name must contain at least one dot: 'domain.action'."""
        assert "." in name, f"'{name}' is not namespaced"

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_pawa_cost_present(self, name):
        """Every registered operator must declare a pawa_cost."""
        meta = OPERATOR_REGISTRY.get(name)
        assert meta is not None, f"Operator '{name}' not found in registry."
        assert hasattr(meta, "pawa_cost"), f"Operator '{name}' missing pawa_cost."
        assert isinstance(meta.pawa_cost, (int, float)), (
            f"Operator '{name}' pawa_cost must be int or float, got {type(meta.pawa_cost)}."
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_license_tier_present(self, name):
        """Every registered operator must declare a license_tier string."""
        meta = OPERATOR_REGISTRY.get(name)
        assert meta is not None
        assert hasattr(meta, "license_tier"), f"Operator '{name}' missing license_tier."
        assert isinstance(meta.license_tier, str), (
            f"Operator '{name}' license_tier must be a string."
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_author_present(self, name):
        """Every registered operator must declare an author string."""
        meta = OPERATOR_REGISTRY.get(name)
        assert meta is not None
        assert hasattr(meta, "author"), f"Operator '{name}' missing author."
        assert isinstance(meta.author, str) and meta.author, (
            f"Operator '{name}' author must be a non-empty string."
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_side_effects_is_list(self, name):
        """Every registered operator must declare side_effects as a list."""
        meta = OPERATOR_REGISTRY.get(name)
        assert meta is not None
        assert hasattr(meta, "side_effects"), f"Operator '{name}' missing side_effects."
        assert isinstance(meta.side_effects, list), (
            f"Operator '{name}' side_effects must be a list."
        )

    @pytest.mark.parametrize("name", ALL_KNOWN_OPERATORS)
    def test_protocol_present_and_valid(self, name):
        """Every registered operator must declare a valid protocol. Sprint 3.7."""
        meta = OPERATOR_REGISTRY.get(name)
        assert meta is not None
        assert hasattr(meta, "protocol"), f"Operator '{name}' missing protocol."
        valid = {"rpc", "event_driven", "polling", "streaming"}
        assert meta.protocol in valid, (
            f"Operator '{name}' protocol '{meta.protocol}' not in {valid}."
        )
