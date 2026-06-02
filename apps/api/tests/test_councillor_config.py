"""
tests/test_councillor_config.py

Tests for CouncillorConfig and load_councillor_configs (Sprint 7.1).

Verifies:
  - CouncillorConfig dataclass construction and defaults
  - load_councillor_configs() parses homestead.json operatives section correctly
  - All 5 homestead councillors have non-empty domain lists
  - All 5 homestead councillors have a sub-operative roster (2 entries each)
  - Every sub-operative graph file path declared in homestead.json exists on disk
  - Graph files are valid JSON with the required OperativeGraph keys
"""

import json
from pathlib import Path

import pytest

from sustena.core.council import CouncillorConfig, load_councillor_configs

# Path to the sustena package root (apps/api/sustena/)
_SUSTENA_ROOT = Path(__file__).parent.parent / "sustena"
_GRAPHS_DIR   = _SUSTENA_ROOT / "operatives" / "graphs"
_HOMESTEAD    = _SUSTENA_ROOT / "sustains" / "homestead.json"

# Expected councillor names in homestead.json
COUNCILLOR_IDS = ["mentor", "protege", "attache", "navigator", "curator"]

# Expected domains per councillor (order-independent)
EXPECTED_DOMAINS: dict[str, set[str]] = {
    "mentor":    {"finances", "budget", "savings"},
    "protege":   {"tasks", "calendar", "time", "deadlines"},
    "attache":   {"contacts", "social", "relationships", "trust"},
    "navigator": {"logistics", "routing", "delivery", "transport"},
    "curator":   {"assets", "inventory", "procurement", "investment"},
}


# ── CouncillorConfig unit tests ────────────────────────────────────────────────


class TestCouncillorConfigDataclass:
    def test_construction_with_all_fields(self):
        cfg = CouncillorConfig(
            operative_id="mentor",
            domain=["finances", "budget"],
            sub_operatives={"risk_assessor": "graphs/risk.json"},
        )
        assert cfg.operative_id == "mentor"
        assert cfg.domain == ["finances", "budget"]
        assert cfg.sub_operatives == {"risk_assessor": "graphs/risk.json"}

    def test_default_domain_is_empty_list(self):
        cfg = CouncillorConfig(operative_id="unknown")
        assert cfg.domain == []

    def test_default_sub_operatives_is_empty_dict(self):
        cfg = CouncillorConfig(operative_id="unknown")
        assert cfg.sub_operatives == {}

    def test_domain_defaults_do_not_share_state(self):
        """Mutable defaults must not be shared between instances."""
        cfg_a = CouncillorConfig(operative_id="a")
        cfg_b = CouncillorConfig(operative_id="b")
        cfg_a.domain.append("finances")
        assert cfg_b.domain == [], "mutable default shared between instances"

    def test_sub_operatives_defaults_do_not_share_state(self):
        cfg_a = CouncillorConfig(operative_id="a")
        cfg_b = CouncillorConfig(operative_id="b")
        cfg_a.sub_operatives["k"] = "v"
        assert cfg_b.sub_operatives == {}, "mutable default shared between instances"


# ── load_councillor_configs unit tests ────────────────────────────────────────


class TestLoadCouncillorConfigs:
    def _spec(self):
        return {
            "mentor": {
                "class": "MentorOperative",
                "domain": ["finances", "budget"],
                "sub_operatives": {"assessor": "graphs/a.json"},
            },
            "curator": {
                "class": "CuratorOperative",
                "domain": ["assets"],
                "sub_operatives": {},
            },
        }

    def test_returns_dict_keyed_by_operative_id(self):
        configs = load_councillor_configs(self._spec())
        assert set(configs.keys()) == {"mentor", "curator"}

    def test_operative_id_matches_key(self):
        configs = load_councillor_configs(self._spec())
        assert configs["mentor"].operative_id == "mentor"
        assert configs["curator"].operative_id == "curator"

    def test_domain_parsed_correctly(self):
        configs = load_councillor_configs(self._spec())
        assert configs["mentor"].domain == ["finances", "budget"]
        assert configs["curator"].domain == ["assets"]

    def test_sub_operatives_parsed_correctly(self):
        configs = load_councillor_configs(self._spec())
        assert configs["mentor"].sub_operatives == {"assessor": "graphs/a.json"}
        assert configs["curator"].sub_operatives == {}

    def test_missing_domain_defaults_to_empty(self):
        spec = {"op": {"class": "SomeOperative"}}
        configs = load_councillor_configs(spec)
        assert configs["op"].domain == []

    def test_missing_sub_operatives_defaults_to_empty(self):
        spec = {"op": {"class": "SomeOperative"}}
        configs = load_councillor_configs(spec)
        assert configs["op"].sub_operatives == {}

    def test_empty_spec_returns_empty_dict(self):
        assert load_councillor_configs({}) == {}

    def test_values_are_copies_not_references(self):
        """Mutating parsed config must not affect the original spec dict."""
        spec = self._spec()
        configs = load_councillor_configs(spec)
        configs["mentor"].domain.append("EXTRA")
        assert "EXTRA" not in spec["mentor"]["domain"]


# ── Homestead.json integration ────────────────────────────────────────────────


class TestHomesteadCouncillorConfigs:
    @pytest.fixture(scope="class")
    def homestead_spec(self):
        return json.loads(_HOMESTEAD.read_text(encoding="utf-8"))

    @pytest.fixture(scope="class")
    def configs(self, homestead_spec):
        return load_councillor_configs(homestead_spec["operatives"])

    def test_all_councillors_present(self, configs):
        assert set(configs.keys()) == set(COUNCILLOR_IDS)

    @pytest.mark.parametrize("op_id", COUNCILLOR_IDS)
    def test_domain_is_non_empty(self, configs, op_id):
        assert len(configs[op_id].domain) > 0, f"{op_id} has no domain declared"

    @pytest.mark.parametrize("op_id", COUNCILLOR_IDS)
    def test_expected_domains_match(self, configs, op_id):
        actual   = set(configs[op_id].domain)
        expected = EXPECTED_DOMAINS[op_id]
        assert actual == expected, (
            f"{op_id}: domain mismatch. got={actual} expected={expected}"
        )

    @pytest.mark.parametrize("op_id", COUNCILLOR_IDS)
    def test_sub_operatives_roster_has_two_entries(self, configs, op_id):
        count = len(configs[op_id].sub_operatives)
        assert count == 2, f"{op_id} has {count} sub_operatives, expected 2"

    @pytest.mark.parametrize("op_id", COUNCILLOR_IDS)
    def test_sub_operative_graph_files_exist(self, configs, op_id):
        for name, rel_path in configs[op_id].sub_operatives.items():
            # rel_path is e.g. "graphs/financial_risk_assessment.json"
            full_path = _SUSTENA_ROOT / "operatives" / rel_path
            assert full_path.exists(), (
                f"{op_id}.{name}: graph file not found at {full_path}"
            )

    @pytest.mark.parametrize("op_id", COUNCILLOR_IDS)
    def test_sub_operative_graph_files_are_valid_json(self, configs, op_id):
        for name, rel_path in configs[op_id].sub_operatives.items():
            full_path = _SUSTENA_ROOT / "operatives" / rel_path
            try:
                spec = json.loads(full_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError as exc:
                pytest.fail(f"{op_id}.{name}: invalid JSON at {full_path}: {exc}")
            assert "entry" in spec, f"{op_id}.{name}: missing 'entry' key"
            assert "exit"  in spec, f"{op_id}.{name}: missing 'exit' key"
            assert "nodes" in spec, f"{op_id}.{name}: missing 'nodes' key"
            assert isinstance(spec["nodes"], dict), (
                f"{op_id}.{name}: 'nodes' must be a dict"
            )
            assert len(spec["nodes"]) > 0, (
                f"{op_id}.{name}: 'nodes' must have at least one entry"
            )
