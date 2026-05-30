"""
tests/test_sustain_specs.py
────────────────────────────────────────────────────────────────────────────────
Epic 1.5.1 — Static validation tests for the three new sustain specs:
  sustena/sustains/biashara.json
  sustena/sustains/vyyb.json
  sustena/sustains/colosso.json

No database, no running app — pure JSON validation.
"""

import json
import pathlib

import pytest

# ── Paths ──────────────────────────────────────────────────────────────────────

SUSTAINS_DIR = pathlib.Path(__file__).parent.parent.parent / "sustena" / "sustains"

BIASHARA_PATH = SUSTAINS_DIR / "biashara.json"
VYYB_PATH = SUSTAINS_DIR / "vyyb.json"
COLOSSO_PATH = SUSTAINS_DIR / "colosso.json"

# Top-level keys every spec must have (same contract as homestead.json)
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

VYYB_EXTRA_OPERATORS = [
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

COLOSSO_EXTRA_OPERATORS = [
    "colosso.members.onboard",
    "colosso.members.kyc",
    "colosso.credit.originate_loan",
    "colosso.credit.disburse",
    "colosso.credit.repay",
    "colosso.trust.compute_score",
]


# ── Fixtures ───────────────────────────────────────────────────────────────────

@pytest.fixture(scope="module")
def biashara() -> dict:
    assert BIASHARA_PATH.exists(), f"biashara.json not found at {BIASHARA_PATH}"
    return json.loads(BIASHARA_PATH.read_text(encoding="utf-8"))


@pytest.fixture(scope="module")
def vyyb() -> dict:
    assert VYYB_PATH.exists(), f"vyyb.json not found at {VYYB_PATH}"
    return json.loads(VYYB_PATH.read_text(encoding="utf-8"))


@pytest.fixture(scope="module")
def colosso() -> dict:
    assert COLOSSO_PATH.exists(), f"colosso.json not found at {COLOSSO_PATH}"
    return json.loads(COLOSSO_PATH.read_text(encoding="utf-8"))


def _op_names(spec: dict) -> set[str]:
    return {op["name"] for op in spec["operators"]}


# ── File existence ─────────────────────────────────────────────────────────────

def test_biashara_json_exists():
    assert BIASHARA_PATH.exists()


def test_vyyb_json_exists():
    assert VYYB_PATH.exists()


def test_colosso_json_exists():
    assert COLOSSO_PATH.exists()


# ── Top-level keys ─────────────────────────────────────────────────────────────

@pytest.mark.parametrize("spec_name,fixture_fn", [
    ("biashara", "biashara"),
    ("vyyb", "vyyb"),
    ("colosso", "colosso"),
])
def test_required_top_level_keys(spec_name, fixture_fn, request):
    spec = request.getfixturevalue(fixture_fn)
    missing = [k for k in REQUIRED_TOP_LEVEL_KEYS if k not in spec]
    assert not missing, f"{spec_name}.json missing top-level keys: {missing}"


# ── IDs ────────────────────────────────────────────────────────────────────────

def test_biashara_id(biashara):
    assert biashara["id"] == "biashara"


def test_vyyb_id(vyyb):
    assert vyyb["id"] == "vyyb"


def test_colosso_id(colosso):
    assert colosso["id"] == "colosso"


# ── Extends field (fork specs only) ───────────────────────────────────────────

def test_vyyb_extends_biashara(vyyb):
    assert vyyb.get("extends") == "biashara", "vyyb.json must have extends: biashara"


def test_colosso_extends_biashara(colosso):
    assert colosso.get("extends") == "biashara", "colosso.json must have extends: biashara"


def test_biashara_does_not_have_extends(biashara):
    assert "extends" not in biashara, "biashara.json is the base — must not have extends field"


# ── Operators non-empty & field validation ────────────────────────────────────

@pytest.mark.parametrize("spec_name,fixture_fn", [
    ("biashara", "biashara"),
    ("vyyb", "vyyb"),
    ("colosso", "colosso"),
])
def test_operators_non_empty(spec_name, fixture_fn, request):
    spec = request.getfixturevalue(fixture_fn)
    assert isinstance(spec["operators"], list)
    assert len(spec["operators"]) > 0, f"{spec_name}.json operators list must not be empty"


@pytest.mark.parametrize("spec_name,fixture_fn", [
    ("biashara", "biashara"),
    ("vyyb", "vyyb"),
    ("colosso", "colosso"),
])
def test_each_operator_has_name_and_pawa_cost(spec_name, fixture_fn, request):
    spec = request.getfixturevalue(fixture_fn)
    for op in spec["operators"]:
        assert "name" in op, f"{spec_name}: operator missing 'name': {op}"
        assert "pawa_cost" in op, f"{spec_name}: operator '{op.get('name')}' missing 'pawa_cost'"


# ── Biashara operator coverage ────────────────────────────────────────────────

def test_biashara_has_all_required_operators(biashara):
    names = _op_names(biashara)
    missing = [op for op in BIASHARA_OPERATORS if op not in names]
    assert not missing, f"biashara.json missing operators: {missing}"


# ── Vyyb operator coverage ────────────────────────────────────────────────────

def test_vyyb_inherits_biashara_operators(vyyb):
    names = _op_names(vyyb)
    missing = [op for op in BIASHARA_OPERATORS if op not in names]
    assert not missing, f"vyyb.json must include all biashara operators, missing: {missing}"


def test_vyyb_has_all_extra_operators(vyyb):
    names = _op_names(vyyb)
    missing = [op for op in VYYB_EXTRA_OPERATORS if op not in names]
    assert not missing, f"vyyb.json missing vyyb-specific operators: {missing}"


# ── Colosso operator coverage ─────────────────────────────────────────────────

def test_colosso_inherits_biashara_operators(colosso):
    names = _op_names(colosso)
    missing = [op for op in BIASHARA_OPERATORS if op not in names]
    assert not missing, f"colosso.json must include all biashara operators, missing: {missing}"


def test_colosso_has_all_extra_operators(colosso):
    names = _op_names(colosso)
    missing = [op for op in COLOSSO_EXTRA_OPERATORS if op not in names]
    assert not missing, f"colosso.json missing colosso-specific operators: {missing}"


# ── Operatives ────────────────────────────────────────────────────────────────

def test_biashara_operatives(biashara):
    for op in ["mentor", "navigator", "attache", "curator"]:
        assert op in biashara["operatives"], f"biashara.json operatives must include '{op}'"


def test_vyyb_operatives_includes_protege(vyyb):
    for op in ["mentor", "navigator", "attache", "curator", "protege"]:
        assert op in vyyb["operatives"], f"vyyb.json operatives must include '{op}'"


def test_colosso_operatives(colosso):
    for op in ["mentor", "attache", "curator"]:
        assert op in colosso["operatives"], f"colosso.json operatives must include '{op}'"


# ── Invariants ────────────────────────────────────────────────────────────────

@pytest.mark.parametrize("spec_name,fixture_fn", [
    ("biashara", "biashara"),
    ("vyyb", "vyyb"),
    ("colosso", "colosso"),
])
def test_invariants_non_empty(spec_name, fixture_fn, request):
    spec = request.getfixturevalue(fixture_fn)
    assert isinstance(spec["invariants"], list)
    assert len(spec["invariants"]) > 0, f"{spec_name}.json must have at least one invariant"


def test_biashara_journal_balanced_invariant(biashara):
    expressions = [inv["expression"] for inv in biashara["invariants"]]
    assert any("journal" in e.lower() and "debit" in e.lower() for e in expressions), \
        "biashara.json must declare a journal_balanced invariant"


def test_biashara_inventory_non_negative_invariant(biashara):
    expressions = [inv["expression"] for inv in biashara["invariants"]]
    assert any("inventory" in e.lower() and ">= 0" in e for e in expressions), \
        "biashara.json must declare an inventory qty >= 0 invariant"


def test_colosso_has_loan_outstanding_invariant(colosso):
    expressions = [inv["expression"] for inv in colosso["invariants"]]
    assert any("outstanding" in e.lower() and ">= 0" in e for e in expressions), \
        "colosso.json must declare a loan outstanding >= 0 invariant"


# ── Default state structure ───────────────────────────────────────────────────

def test_biashara_default_state(biashara):
    ds = biashara["default_state"]
    assert isinstance(ds["inventory"]["items"], dict)
    assert isinstance(ds["inventory"]["movements"], list)
    assert isinstance(ds["accounts"]["chart"], dict)
    assert isinstance(ds["accounts"]["journal_entries"], list)
    assert isinstance(ds["parties"], dict)
    assert isinstance(ds["orders"]["active"], list)
    assert isinstance(ds["assets"], dict)
    assert isinstance(ds["expenses"], list)
    assert isinstance(ds["calendar"]["events"], list)
    assert isinstance(ds["analytics"]["period_reports"], list)


def test_vyyb_default_state_extends_biashara(vyyb):
    ds = vyyb["default_state"]
    # inherited
    assert "inventory" in ds
    assert "accounts" in ds
    assert "parties" in ds
    # vyyb-specific
    assert isinstance(ds["recipes"]["library"], dict)
    assert isinstance(ds["production"]["batches"], list)
    assert isinstance(ds["kds"]["tasks"], list)
    assert isinstance(ds["staff"]["roster"], dict)
    assert isinstance(ds["procurement"]["suppliers"], dict)


def test_colosso_default_state_extends_biashara(colosso):
    ds = colosso["default_state"]
    # inherited
    assert "inventory" in ds
    assert "accounts" in ds
    # colosso-specific
    assert isinstance(ds["members"], dict)
    assert isinstance(ds["credit"]["loan_book"], list)
    assert isinstance(ds["credit"]["repayment_history"], list)
    assert isinstance(ds["trust_scores"], dict)
    assert isinstance(ds["compliance"]["kyc_records"], list)
    assert isinstance(ds["compliance"]["flags"], list)


# ── Access policy ─────────────────────────────────────────────────────────────

@pytest.mark.parametrize("spec_name,fixture_fn", [
    ("biashara", "biashara"),
    ("vyyb", "vyyb"),
    ("colosso", "colosso"),
])
def test_access_policy_owner_ids_template(spec_name, fixture_fn, request):
    spec = request.getfixturevalue(fixture_fn)
    policy = spec["access_policy"]
    assert "owner_ids" in policy
    assert "{{owner_ids}}" in policy["owner_ids"], \
        f"{spec_name}.json access_policy.owner_ids must use {{{{owner_ids}}}} template"


# ── Parameters ────────────────────────────────────────────────────────────────

@pytest.mark.parametrize("spec_name,fixture_fn", [
    ("biashara", "biashara"),
    ("vyyb", "vyyb"),
    ("colosso", "colosso"),
])
def test_owner_ids_parameter_required(spec_name, fixture_fn, request):
    spec = request.getfixturevalue(fixture_fn)
    param = next((p for p in spec["parameters"] if p["name"] == "owner_ids"), None)
    assert param is not None, f"{spec_name}.json must declare owner_ids parameter"
    assert param.get("required") is True, f"{spec_name}.json owner_ids must be required=True"


def test_biashara_has_currency_param(biashara):
    names = [p["name"] for p in biashara["parameters"]]
    assert "currency" in names


def test_biashara_currency_default_is_kes(biashara):
    param = next(p for p in biashara["parameters"] if p["name"] == "currency")
    assert param.get("default") == "KES"


def test_vyyb_has_outlet_name_param(vyyb):
    names = [p["name"] for p in vyyb["parameters"]]
    assert "outlet_name" in names, "vyyb.json must declare outlet_name parameter"


def test_colosso_has_regulatory_license_param(colosso):
    names = [p["name"] for p in colosso["parameters"]]
    assert "regulatory_license" in names, "colosso.json must declare regulatory_license parameter"


def test_colosso_regulatory_license_is_required(colosso):
    param = next(p for p in colosso["parameters"] if p["name"] == "regulatory_license")
    assert param.get("required") is True


# ── UI schema ─────────────────────────────────────────────────────────────────

@pytest.mark.parametrize("spec_name,fixture_fn", [
    ("biashara", "biashara"),
    ("vyyb", "vyyb"),
    ("colosso", "colosso"),
])
def test_ui_schema_has_sustain_home(spec_name, fixture_fn, request):
    spec = request.getfixturevalue(fixture_fn)
    assert "sustain_home" in spec["ui_schema"]


def test_vyyb_ui_schema_has_kds_override(vyyb):
    assert "kds_widget" in vyyb["ui_schema"]
    assert vyyb["ui_schema"]["kds_widget"].get("override") is True


def test_vyyb_ui_schema_has_production_batch_override(vyyb):
    assert "production_batch_widget" in vyyb["ui_schema"]
    assert vyyb["ui_schema"]["production_batch_widget"].get("override") is True
