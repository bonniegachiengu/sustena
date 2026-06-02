"""
tests/test_homestead_spec.py

Validates sustena/sustains/homestead.json — the Homestead Sustain spec.
These are static schema tests: no database, no running app.
"""

import json
import pathlib

import pytest

# Resolve the spec file relative to this test file
SPEC_PATH = pathlib.Path(__file__).parent.parent / "sustena" / "sustains" / "homestead.json"

REQUIRED_TOP_LEVEL_KEYS = [
    "id",
    "version",
    "display_name",
    "description",
    "state_schema",
    "operators",
    "operatives",
    "invariants",
    "ui_schema",
    "access_policy",
    "default_state",
    "parameters",
]

REQUIRED_BUDGET_OPERATORS = [
    "budget.record_income",
    "budget.allocate",
    "budget.spend",
    "budget.transfer",
    "budget.summary",
]

CALENDAR_OPERATORS = [
    "homestead.calendar.add_event",
    "homestead.calendar.upcoming_events",
    "homestead.calendar.remove_event",
]


# ── Fixtures ──────────────────────────────────────────────────────────────────

@pytest.fixture(scope="module")
def spec() -> dict:
    """Load and return the parsed homestead.json spec."""
    assert SPEC_PATH.exists(), f"homestead.json not found at {SPEC_PATH}"
    with SPEC_PATH.open(encoding="utf-8") as fh:
        return json.load(fh)


@pytest.fixture(scope="module")
def operator_names(spec) -> set:
    return {op["name"] for op in spec["operators"]}


# ── Parsing ───────────────────────────────────────────────────────────────────

def test_spec_file_exists():
    assert SPEC_PATH.exists(), f"Expected {SPEC_PATH} to exist"


def test_spec_parses_as_valid_json():
    with SPEC_PATH.open(encoding="utf-8") as fh:
        data = json.load(fh)
    assert isinstance(data, dict)


# ── Top-level keys ────────────────────────────────────────────────────────────

def test_all_required_top_level_keys_present(spec):
    missing = [k for k in REQUIRED_TOP_LEVEL_KEYS if k not in spec]
    assert not missing, f"Missing top-level keys: {missing}"


def test_id_is_homestead(spec):
    assert spec["id"] == "homestead"


def test_version_present(spec):
    assert spec["version"], "version must be a non-empty string"


def test_display_name_present(spec):
    assert spec["display_name"], "display_name must be a non-empty string"


def test_description_present(spec):
    assert spec["description"], "description must be a non-empty string"


# ── Operators ─────────────────────────────────────────────────────────────────

def test_operators_is_list(spec):
    assert isinstance(spec["operators"], list)


def test_all_budget_operators_listed(spec, operator_names):
    missing = [op for op in REQUIRED_BUDGET_OPERATORS if op not in operator_names]
    assert not missing, f"Missing budget operators: {missing}"


def test_calendar_operators_declared(spec, operator_names):
    """Calendar operators must be declared in the spec even if not yet implemented."""
    missing = [op for op in CALENDAR_OPERATORS if op not in operator_names]
    assert not missing, f"Calendar operators missing from spec: {missing}"


def test_each_operator_has_name_field(spec):
    for op in spec["operators"]:
        assert "name" in op, f"Operator entry missing 'name': {op}"


def test_each_operator_has_pawa_cost(spec):
    for op in spec["operators"]:
        assert "pawa_cost" in op, f"Operator '{op.get('name')}' missing 'pawa_cost'"


# ── Operatives ────────────────────────────────────────────────────────────────

def test_operatives_contains_all_council_members(spec):
    operatives = spec["operatives"]
    for expected in ["mentor", "protege", "navigator", "attache"]:
        assert expected in operatives, f"operatives must include '{expected}'"


def test_operatives_is_dict_with_class_keys(spec):
    """Sprint 5.5: operatives section is now a dict mapping name → {class, graphs?}."""
    operatives = spec["operatives"]
    assert isinstance(operatives, dict), "operatives must be a dict (Sprint 5.5)"
    for name, entry in operatives.items():
        assert isinstance(entry, dict), f"operative '{name}' entry must be a dict"
        assert "class" in entry, f"operative '{name}' must declare a 'class' key"


def test_mentor_operative_has_graph_references(spec):
    """Mentor must declare evaluation_graph and deliberation_graph paths."""
    mentor = spec["operatives"]["mentor"]
    assert "evaluation_graph" in mentor, "mentor must declare evaluation_graph"
    assert "deliberation_graph" in mentor, "mentor must declare deliberation_graph"


def test_mentor_graph_files_exist(spec):
    """The graph JSON files referenced in mentor must exist on disk."""
    import pathlib
    graphs_dir = pathlib.Path(__file__).parent.parent / "sustena" / "operatives" / "graphs"
    mentor = spec["operatives"]["mentor"]
    for key in ("evaluation_graph", "deliberation_graph"):
        graph_path = graphs_dir / pathlib.Path(mentor[key]).name
        assert graph_path.exists(), f"Graph file not found: {graph_path}"


# ── Invariants ────────────────────────────────────────────────────────────────

def test_invariants_is_non_empty_list(spec):
    assert isinstance(spec["invariants"], list)
    assert len(spec["invariants"]) > 0, "At least one invariant must be defined"


def test_liquid_balance_invariant_exists(spec):
    expressions = [inv["expression"] for inv in spec["invariants"]]
    assert any("finances.liquid.balance >= 0" in e for e in expressions), \
        "Invariant 'finances.liquid.balance >= 0' must be declared"


def test_pocket_allocated_invariant_exists(spec):
    expressions = [inv["expression"] for inv in spec["invariants"]]
    assert any("allocated" in e and ">= 0" in e for e in expressions), \
        "Invariant for pocket allocated >= 0 must be declared"


def test_each_invariant_has_expression(spec):
    for inv in spec["invariants"]:
        assert "expression" in inv, f"Invariant missing 'expression': {inv}"


# ── Default state ─────────────────────────────────────────────────────────────

def test_default_state_present(spec):
    assert isinstance(spec["default_state"], dict)


def test_default_state_has_members(spec):
    assert "members" in spec["default_state"]
    assert isinstance(spec["default_state"]["members"], list)


def test_default_state_has_finances(spec):
    assert "finances" in spec["default_state"]
    fin = spec["default_state"]["finances"]
    assert "liquid" in fin
    assert "balance" in fin["liquid"]
    assert fin["liquid"]["balance"] == 0.0


def test_default_state_has_empty_pockets(spec):
    pockets = spec["default_state"]["finances"]["pockets"]
    # pockets is now a dict keyed by pocket name (matches budget.py storage)
    assert isinstance(pockets, dict)
    assert len(pockets) == 0, "default_state pockets must start empty"


def test_default_state_has_income(spec):
    income = spec["default_state"]["finances"]["income"]
    assert "amount" in income
    assert income["amount"] == 0.0


def test_default_state_has_goals(spec):
    goals = spec["default_state"]["finances"]["goals"]
    assert isinstance(goals, list)


def test_default_state_has_calendar(spec):
    assert "calendar" in spec["default_state"]
    assert "events" in spec["default_state"]["calendar"]
    assert isinstance(spec["default_state"]["calendar"]["events"], list)


def test_default_state_has_alerts(spec):
    assert "alerts" in spec["default_state"]
    assert isinstance(spec["default_state"]["alerts"], list)


# ── State schema structure ────────────────────────────────────────────────────

def test_state_schema_has_all_sections(spec):
    schema = spec["state_schema"]
    for section in ["members", "finances", "calendar", "alerts"]:
        assert section in schema, f"state_schema missing section: {section}"


def test_finances_schema_has_liquid(spec):
    assert "liquid" in spec["state_schema"]["finances"]["properties"]


def test_finances_schema_has_pockets(spec):
    assert "pockets" in spec["state_schema"]["finances"]["properties"]


def test_finances_schema_has_income(spec):
    assert "income" in spec["state_schema"]["finances"]["properties"]


def test_finances_schema_has_goals(spec):
    assert "goals" in spec["state_schema"]["finances"]["properties"]


# ── Access policy ─────────────────────────────────────────────────────────────

def test_access_policy_has_owner_ids_template(spec):
    policy = spec["access_policy"]
    assert "owner_ids" in policy
    assert "{{owner_ids}}" in policy["owner_ids"], \
        "access_policy.owner_ids must use the {{owner_ids}} template token"


# ── Parameters ────────────────────────────────────────────────────────────────

def test_parameters_is_list(spec):
    assert isinstance(spec["parameters"], list)


def test_owner_ids_parameter_is_required(spec):
    owner_param = next((p for p in spec["parameters"] if p["name"] == "owner_ids"), None)
    assert owner_param is not None, "parameters must include 'owner_ids'"
    assert owner_param.get("required") is True, "owner_ids parameter must be required"


def test_initial_income_parameter_exists(spec):
    names = [p["name"] for p in spec["parameters"]]
    assert "initial_income" in names, "parameters must include 'initial_income'"


# ── UI schema ─────────────────────────────────────────────────────────────────

def test_ui_schema_has_sustain_home_widget(spec):
    ui = spec["ui_schema"]
    assert "sustain_home" in ui
    assert ui["sustain_home"]["widget"] == "sustain_home"
