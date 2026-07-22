"""
sustena/operators/ui_render.py

ui.render.* sub-operators — convert ui_schema + live state into ResponseWidgets.

All operators here are:
  - pawa_cost  = 0  (rendering is never charged)
  - side_effects = []  (read-only, no mutations, no events)

Operators:
  ui.render.operator_card — renders a single operator result card from its ui_schema
  ui.render.operative_dashboard — renders an operative's status panel
  ui.render.sustain_home — renders the sustain home screen
  ui.render.preview — dev console live preview (spec JSON + mock state → widget)
"""

from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator
from sustena.core.uiparser import ResponseWidget, UISchemaParser

_parser = UISchemaParser()

# ── ui.render.operator_card ────────────────────────────────────────────────────


@sustena_operator(
    name="ui.render.operator_card",
    description="Render an operator's result as a card widget using its ui_schema.",
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={},
)
async def ui_render_operator_card(
    ctx: OperatorContext,
    ui_schema: dict,
    inputs: dict | None = None,
    result_data: dict | None = None,
) -> OperatorResult:
    """
    Resolve every field in a ui_schema against live state + operator inputs,
    and return a ResponseWidget ready for the frontend.

    params:
      ui_schema   — the operator's ui_schema dict (from OPERATOR_REGISTRY)
      inputs      — the call's input parameters (used to resolve "inputs.*" sources)
      result_data — the operator's OperatorResult.data (included in widget.data.result)
    """
    inputs = inputs or {}
    result_data = result_data or {}

    try:
        schema = _parser.parse(ui_schema)
    except ValueError as exc:
        return OperatorResult.fail(reason=f"Invalid ui_schema: {exc}")

    widget = ResponseWidget(
        widget_type=schema.widget_type,
        data={
            "fields": [
                {
                    "label": f.label,
                    "value": _parser.resolve_source(f.source, inputs, ctx.state),
                    "display": f.display,
                    "colour_rule": f.colour_rule,
                }
                for f in schema.fields
            ],
            "ctas": schema.ctas,
            "result": result_data,
        },
        summary=f"Rendered {schema.widget_type}",
    )
    return OperatorResult.ok(widget.to_dict())


# ── ui.render.operative_dashboard ─────────────────────────────────────────────


@sustena_operator(
    name="ui.render.operative_dashboard",
    description="Render an operative's current status as a dashboard widget.",
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={},
)
async def ui_render_operative_dashboard(
    ctx: OperatorContext,
    ui_schema: dict,
    operative_state: dict | None = None,
) -> OperatorResult:
    """
    Render an operative's state into a dashboard widget.

    params:
      ui_schema       — the operative's ui_schema dict
      operative_state — current operative state (status, confidence, task, etc.)
                        used as operator_inputs for "inputs.*" source resolution
    """
    operative_state = operative_state or {}

    try:
        schema = _parser.parse(ui_schema)
    except ValueError as exc:
        return OperatorResult.fail(reason=f"Invalid ui_schema: {exc}")

    widget = ResponseWidget(
        widget_type=schema.widget_type,
        data={
            "fields": [
                {
                    "label": f.label,
                    "value": _parser.resolve_source(f.source, operative_state, ctx.state),
                    "display": f.display,
                }
                for f in schema.fields
            ],
            "ctas": schema.ctas,
            "operative": operative_state,
        },
        summary=f"Operative dashboard: {operative_state.get('name', '?')}",
    )
    return OperatorResult.ok(widget.to_dict())


# ── ui.render.sustain_home ─────────────────────────────────────────────────────


@sustena_operator(
    name="ui.render.sustain_home",
    description="Render the sustain home screen as a list of key-metric widgets.",
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={},
)
async def ui_render_sustain_home(
    ctx: OperatorContext,
    ui_schema: dict,
    full_state: dict | None = None,
    active_operatives: list | None = None,
) -> OperatorResult:
    """
    Render the sustain home screen from a ui_schema.

    params:
      ui_schema — home-screen ui_schema
      full_state — full sustain state snapshot (used as plain dict fallback)
      active_operatives — list of operative state dicts to include in the widget
    """
    active_operatives = active_operatives or []

    try:
        schema = _parser.parse(ui_schema)
    except ValueError as exc:
        return OperatorResult.fail(reason=f"Invalid ui_schema: {exc}")

    widget = ResponseWidget(
        widget_type=schema.widget_type,
        data={
            "fields": [
                {
                    "label": f.label,
                    "value": _parser.resolve_source(f.source, {}, ctx.state),
                    "display": f.display,
                }
                for f in schema.fields
            ],
            "ctas": schema.ctas,
            "operatives": active_operatives,
        },
        summary="Sustain home",
    )
    return OperatorResult.ok(widget.to_dict())


# ── ui.render.preview ──────────────────────────────────────────────────────────


@sustena_operator(
    name="ui.render.preview",
    description="Dev console: parse a spec JSON + mock state and return a ResponseWidget preview.",
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={},
)
async def ui_render_preview(
    ctx: OperatorContext,
    spec_json: dict,
    mock_state: dict | None = None,
) -> OperatorResult:
    """
    Live preview for the dev console.

    Accepts either a bare ui_schema dict or an operator spec that contains one:
      { "ui_schema": { "widget_type": ..., "fields": [...] } }

    params:
      spec_json — a ui_schema dict or an operator spec containing ui_schema
      mock_state — optional mock state/inputs for source resolution
                   shape: { "inputs": {...}, "state": {...} }
    """
    mock_state = mock_state or {}
    mock_inputs = mock_state.get("inputs", {})

    # Support both a bare ui_schema dict and a full operator spec
    ui_schema_dict = spec_json.get("ui_schema", spec_json)

    try:
        schema = _parser.parse(ui_schema_dict)
    except ValueError as exc:
        return OperatorResult.fail(reason=f"Cannot parse spec: {exc}")

    # For preview, we resolve against mock_state as a plain dict
    widget = ResponseWidget(
        widget_type=schema.widget_type,
        data={
            "fields": [
                {
                    "label": f.label,
                    "value": _parser.resolve_source(f.source, mock_inputs, mock_state),
                    "display": f.display,
                    "colour_rule": f.colour_rule,
                }
                for f in schema.fields
            ],
            "ctas": schema.ctas,
            **({"chart": schema.chart} if schema.chart else {}),
        },
        summary=f"Preview: {schema.widget_type}",
    )
    return OperatorResult.ok(widget.to_dict())
