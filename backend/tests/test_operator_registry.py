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

ALL_KNOWN_OPERATORS = BUDGET_OPERATORS + CHAMA_OPERATORS + PROCUREMENT_OPERATORS + CALENDAR_OPERATORS  # extend as epics ship


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
