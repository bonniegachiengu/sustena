"""
sustena/core/widget_registry.py

WidgetTypeRegistry — catalogue of renderable widget types.

Usage:
    from sustena.core.widget_registry import widget_registry
    widget_registry.register("my_widget", description="...", schema={...})
    html = widget_registry.render({"type": "my_widget", "data": {...}})
    all_types = widget_registry.list_all()          # → GET /devui/widgets
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


@dataclass
class WidgetType:
    widget_type: str
    description: str
    schema: dict = field(default_factory=dict)
    template: str = ""  # Jinja2 template string; empty → generic fallback

    def to_dict(self) -> dict:
        return {
            "widget_type": self.widget_type,
            "description": self.description,
            "schema": self.schema,
        }


class WidgetTypeRegistry:
    """
    Local registry mapping widget_type strings to their metadata and templates.

    All budget, calendar, and other built-in widgets are pre-registered
    at instantiation time. New widget types can be registered at any time.
    """

    def __init__(self) -> None:
        self._registry: dict[str, WidgetType] = {}
        self._register_builtins()

    def register(
        self,
        widget_type: str,
        description: str = "",
        schema: dict | None = None,
        template: str = "",
    ) -> None:
        """Register (or overwrite) a widget type."""
        self._registry[widget_type] = WidgetType(
            widget_type=widget_type,
            description=description,
            schema=schema or {},
            template=template,
        )
        logger.debug("WidgetTypeRegistry: registered %s", widget_type)

    def render(self, widget: dict) -> str:
        """
        Render a ResponseWidget dict to an HTML fragment string.

        Uses the registered Jinja2 template if present.
        Falls back to a generic <dl> layout.
        """
        widget_type = widget.get("type", "unknown")
        data = widget.get("data", {})
        summary = widget.get("summary", "")

        wt = self._registry.get(widget_type)
        if wt and wt.template:
            try:
                from jinja2 import Template
                return Template(wt.template).render(data=data, summary=summary)
            except Exception as exc:
                logger.warning("WidgetTypeRegistry.render: template error for %s: %s", widget_type, exc)

        return self._render_generic(widget_type, data, summary)

    def list_all(self) -> list[dict]:
        return [wt.to_dict() for wt in self._registry.values()]

    def get(self, widget_type: str) -> WidgetType | None:
        return self._registry.get(widget_type)

    # ── private ───────────────────────────────────────────────────────────────

    def _render_generic(self, widget_type: str, data: dict, summary: str) -> str:
        fields_html = "".join(
            f'<dt>{f.get("label", "")}</dt>'
            f'<dd>{f.get("value") if f.get("value") is not None else "—"}</dd>'
            for f in data.get("fields", [])
        )
        ctas_html = "".join(
            f'<button class="sustena-cta">{c}</button>'
            for c in data.get("ctas", [])
        )
        return (
            f'<div class="sustena-widget" data-type="{widget_type}">'
            f'<dl class="sustena-fields">{fields_html}</dl>'
            f'<div class="sustena-ctas">{ctas_html}</div>'
            f'<p class="sustena-summary">{summary}</p>'
            f"</div>"
        )

    def _register_builtins(self) -> None:
        _BUILTINS = [
            {
                "widget_type": "budget_allocation_card",
                "description": "Budget allocation result: pocket name, amount, remaining liquid balance.",
                "schema": {"fields": ["Pocket", "Amount", "Liquid Remaining"], "ctas": ["View Budget", "Allocate Another"]},
            },
            {
                "widget_type": "budget_ring",
                "description": "Donut chart of all budget pockets — allocated, spent, unallocated.",
                "schema": {"chart": "donut", "fields": ["Unallocated", "Total Allocated", "Total Spent"], "ctas": ["Allocate", "Record Spend", "Ask Orchie"]},
            },
            {
                "widget_type": "transaction_confirmation",
                "description": "Confirms a financial transaction with amount, description, and resulting balances.",
                "schema": {"fields": ["Amount", "Description", "New Balance"], "ctas": ["View Budget"]},
            },
            {
                "widget_type": "calendar_event_card",
                "description": "Calendar event with title, date/time, and location.",
                "schema": {"fields": ["Title", "Start", "End", "Location"], "ctas": ["View Calendar"]},
            },
            {
                "widget_type": "operative_status_card",
                "description": "Operative name, role, status, confidence, and current task.",
                "schema": {"fields": ["Name", "Role", "Status", "Confidence", "Task"], "ctas": []},
            },
            {
                "widget_type": "constraint_health_grid",
                "description": "Grid of all active constraints and their pass/fail status.",
                "schema": {"fields": ["Expression", "Status", "Value"], "ctas": []},
            },
            {
                "widget_type": "sustain_home",
                "description": "Home screen with key metrics, active operatives, and quick actions.",
                "schema": {"fields": ["Pawa Balance", "Active Operatives", "Open Proposals"], "ctas": ["Ask Orchie", "View Budget", "Council"]},
            },
            {
                "widget_type": "procurement_order_card",
                "description": "Procurement or purchase order with vendor, amount, and delivery status.",
                "schema": {"fields": ["Vendor", "Amount", "Status", "Delivery Date"], "ctas": ["Approve", "Reject"]},
            },
            {
                "widget_type": "morning_brief_card",
                "description": "Daily morning brief — tasks due today, calendar events, passed council strategies, liquid balance.",
                "schema": {"fields": ["Tasks Due Today", "Events Today", "Passed Strategies", "Liquid Balance"], "ctas": ["View Tasks", "Ask Orchie"]},
            },
            {
                "widget_type": "task_card",
                "description": "Single task card with title, due date, priority, and assigned member.",
                "schema": {"fields": ["Title", "Due", "Priority", "Assigned To"], "ctas": ["Complete", "View Tasks"]},
            },
            {
                "widget_type": "task_list",
                "description": "Filtered list of household tasks.",
                "schema": {"fields": ["Filter", "Tasks"], "ctas": ["Add Task"]},
            },
            {
                "widget_type": "task_carryover_summary",
                "description": "Summary of tasks marked as carryover from a prior polling cycle.",
                "schema": {"fields": ["Carried Over"], "ctas": ["View Tasks"]},
            },
        ]
        for b in _BUILTINS:
            self.register(
                widget_type=b["widget_type"],
                description=b["description"],
                schema=b["schema"],
            )


# Module-level singleton shared across the app
widget_registry = WidgetTypeRegistry()
