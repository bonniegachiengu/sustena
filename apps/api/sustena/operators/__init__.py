# sustena/operators/__init__.py
# Importing this package auto-registers all operators in OPERATOR_REGISTRY.
# Add new operator modules here as they are built.

from sustena.operators import budget       # noqa: F401  (registers budget.* operators)
from sustena.operators import calendar     # noqa: F401  (registers homestead.calendar.* operators)
from sustena.operators import chama        # noqa: F401  (registers chama.* operators)
from sustena.operators import procurement  # noqa: F401  (registers mkulima.* and procurement.* operators)
from sustena.operators import biashara     # noqa: F401  (registers biashara.* operators)
from sustena.operators import vyyb         # noqa: F401  (registers vyyb.* operators)
from sustena.operators import ui_render    # noqa: F401  (registers ui.render.* operators)
from sustena.operators import api_ops      # noqa: F401  (registers api.* operators)
from sustena.operators import monitor      # noqa: F401  (registers monitor.* operators)
from sustena.operators import visualize    # noqa: F401  (registers visualize.* operators)
from sustena.operators import simulate_ops # noqa: F401  (registers simulate.* operators)
from sustena.operators import edit_ops     # noqa: F401  (registers edit.* operators)
from sustena.operators import control_ops  # noqa: F401  (registers control.* operators)
from sustena.operators import mentor_ops   # noqa: F401  (registers mentor.* operators)
