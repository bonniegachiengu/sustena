# sustena/operators/__init__.py
# Importing this package auto-registers all operators in OPERATOR_REGISTRY.
# Add new operator modules here as they are built.

from sustena.operators import budget       # noqa: F401  (registers budget.* operators)
from sustena.operators import calendar     # noqa: F401  (registers homestead.calendar.* operators)
from sustena.operators import procurement  # noqa: F401  (registers mkulima.* and procurement.* operators)
from sustena.operators import ui_render    # noqa: F401  (registers ui.render.* operators)
from sustena.operators import api_ops      # noqa: F401  (registers api.* operators)
from sustena.operators import monitor      # noqa: F401  (registers monitor.* operators)
from sustena.operators import visualize    # noqa: F401  (registers visualize.* operators)
from sustena.operators import simulate_ops # noqa: F401  (registers simulate.* operators)
from sustena.operators import edit_ops     # noqa: F401  (registers edit.* operators)
from sustena.operators import control_ops  # noqa: F401  (registers control.* operators)
from sustena.operators import mentor_ops     # noqa: F401  (registers mentor.* operators)
from sustena.operators import operative_ops  # noqa: F401  (registers operative.* operators)
from sustena.operators import tasks          # noqa: F401  (registers homestead.tasks.* operators)
from sustena.operators import orchie_ops     # noqa: F401  (registers orchie.* operators)
from sustena.operators import egress_ops     # noqa: F401  (registers egress.* operators)
