"""
sustena/core/uiparser.py

UISchemaParser — parses the ui_schema block on any operator/operative/sustain spec
and resolves field source expressions against live state and operator inputs.

UISchema block format (used in @sustena_operator and sustain spec JSON):
{
  "widget_type": "budget_allocation_card",       -- required
  "fields": [
    {
      "label":       "Amount",                   -- display label
      "source":      "inputs.amount",            -- expression (see below)
      "display":     "currency",                 -- text | currency | currency_kes | percentage | date
      "colour_rule": "amber_if_below_20pct"      -- optional colour hint
    }
  ],
  "ctas":  ["View Budget", "Allocate Another"],  -- call-to-action button labels
  "chart": "donut"                               -- optional chart hint (donut | line | bar)
}

resolve_source() Phase 1 — no dynamic code execution:
  "inputs.<key>"                  → operator_inputs[key]
  "state.<dot.path>"              → state.get(dot.path)
  "state.<path> - state.<path>"   → subtraction of two state paths
"""

from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any

logger = logging.getLogger(__name__)


# ── Typed schema dataclasses ──────────────────────────────────────────────────


@dataclass
class UISchemaField:
    label: str
    source: str
    display: str = "text"
    colour_rule: str | None = None

    def to_dict(self) -> dict:
        d: dict = {"label": self.label, "source": self.source, "display": self.display}
        if self.colour_rule:
            d["colour_rule"] = self.colour_rule
        return d


@dataclass
class UISchema:
    widget_type: str
    fields: list[UISchemaField] = field(default_factory=list)
    ctas: list[str] = field(default_factory=list)
    chart: str | None = None

    def to_dict(self) -> dict:
        d: dict = {
            "widget_type": self.widget_type,
            "fields": [f.to_dict() for f in self.fields],
            "ctas": self.ctas,
        }
        if self.chart:
            d["chart"] = self.chart
        return d


@dataclass
class ResponseWidget:
    """Serialisable widget returned by ui.render.* operators."""
    widget_type: str
    data: dict = field(default_factory=dict)
    summary: str = ""

    def to_dict(self) -> dict:
        return {"type": self.widget_type, "data": self.data, "summary": self.summary}


# ── Parser ────────────────────────────────────────────────────────────────────


class UISchemaParser:
    """
    Parses ui_schema dicts and resolves field source expressions.

    No eval() anywhere — expressions are matched structurally:
      Phase 1: inputs.<key>, state.<path>, state.<a> - state.<b>
    """

    def parse(self, ui_schema_dict: dict) -> UISchema:
        """
        Parse a raw ui_schema dict into a typed UISchema.

        Raises ValueError if widget_type is missing.
        """
        widget_type = ui_schema_dict.get("widget_type")
        if not widget_type:
            raise ValueError("ui_schema must have a 'widget_type' field")

        fields = [
            UISchemaField(
                label=f.get("label", ""),
                source=f.get("source", ""),
                display=f.get("display", "text"),
                colour_rule=f.get("colour_rule"),
            )
            for f in ui_schema_dict.get("fields", [])
            if isinstance(f, dict)
        ]

        return UISchema(
            widget_type=widget_type,
            fields=fields,
            ctas=list(ui_schema_dict.get("ctas", [])),
            chart=ui_schema_dict.get("chart"),
        )

    def resolve_source(
        self,
        source_expr: str,
        operator_inputs: dict,
        state: Any,
    ) -> Any:
        """
        Resolve a source expression to a concrete value.

        Phase 1 rules (no dynamic code execution):
          "inputs.<key>"            → operator_inputs[key]
          "state.<dot.path>"        → state.get(dot.path) or plain dict walk
          "state.<a> - state.<b>"   → numeric subtraction of two state paths
        """
        expr = source_expr.strip()

        if " - " in expr:
            left_expr, right_expr = expr.split(" - ", 1)
            left = self._resolve_single(left_expr.strip(), operator_inputs, state)
            right = self._resolve_single(right_expr.strip(), operator_inputs, state)
            try:
                return (left or 0) - (right or 0)
            except TypeError:
                logger.warning("resolve_source: cannot subtract %r - %r", left, right)
                return None

        return self._resolve_single(expr, operator_inputs, state)

    # ── private ───────────────────────────────────────────────────────────────

    def _resolve_single(self, expr: str, operator_inputs: dict, state: Any) -> Any:
        if expr.startswith("inputs."):
            return operator_inputs.get(expr[len("inputs."):])

        if expr.startswith("state."):
            return self._get_state(expr[len("state."):], state)

        logger.debug("resolve_source: unrecognised expression %r", expr)
        return None

    def _get_state(self, dot_path: str, state: Any) -> Any:
        try:
            if isinstance(state, dict):
                # Plain dict — walk the dot path manually
                val = state
                for part in dot_path.split("."):
                    if not isinstance(val, dict):
                        return None
                    val = val.get(part)
                return val
            if hasattr(state, "get"):
                # StateAccessor — understands dot paths natively
                return state.get(dot_path)
            return None
        except Exception as exc:
            logger.warning("resolve_source: error reading %r: %s", dot_path, exc)
            return None


# Module-level singleton
parser = UISchemaParser()
