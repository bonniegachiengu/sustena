# sustena/operators/__init__.py
# Importing this package auto-registers all operators in OPERATOR_REGISTRY.
# Add new operator modules here as they are built.

from sustena.operators import budget       # noqa: F401  (registers budget.* operators)
from sustena.operators import calendar     # noqa: F401  (registers homestead.calendar.* operators)
from sustena.operators import chama        # noqa: F401  (registers chama.* operators)
from sustena.operators import procurement  # noqa: F401  (registers mkulima.* and procurement.* operators)
