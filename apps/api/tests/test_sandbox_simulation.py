"""
tests/test_sandbox_simulation.py

Tests for Sprint 7.3 — per-councillor sandbox simulation.

Covers:
  - _create_sandbox() forks state and returns a valid fork_id
  - Each councillor gets a distinct fork_id (no sharing)
  - Sub-operatives run against the fork and produce sandbox_results
  - sandbox_results are keyed by sub_op_name
  - sub_op results carry status, operator_name, rationale, simulation_results
  - fork_id is recorded on vote records in state
  - sandbox_results appear on vote records when present
  - enriched proposal_dict passed to deliberate() includes sandbox_results + fork_id
  - Councillors with no sub_operatives still deliberate (no fork created)
  - Fork failure (simulate.fork unavailable) degrades gracefully — deliberation proceeds
  - Sub-operative graph error (e.g. CalibrationError, FileNotFoundError) is recorded
    per-sub-op and does not block the others
  - Two relevant councillors called in one collect_votes() get different fork_ids
"""

import json
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock

import pytest

from sustena.core.council import CouncillorConfig, CouncilSession
from sustena.core.state import StateAccessor
from sustena.operatives.base import OperativeVote, VoteChoice
from sustena.operators.simulate_ops import _FORK_REGISTRY, clear_fork_registry

# Absolute path to the operatives/graphs directory for assertions
_GRAPHS_DIR = (
    Path(__file__).parent.parent / "sustena" / "operatives" / "graphs"
)


# ── Fixtures / helpers ────────────────────────────────────────────────────────

def _make_state() -> StateAccessor:
    return StateAccessor({
        "finances": {
            "liquid":  {"balance": 50_000.0},
            "pockets": {
                "food": {"allocated": 5_000.0, "spent": 500.0, "limit": 5_000.0}
            },
            "income": {"sources": [], "monthly_total": 50_000.0},
        },
        "council_proposals": [],
        "council_votes":     [],
    })


def _make_council(state: StateAccessor) -> CouncilSession:
    return CouncilSession(sustain_id="test-sustain", state=state)


def _make_operative(op_id: str, vote: str = "YES") -> MagicMock:
    op = MagicMock()
    op.operative_id = op_id
    op.deliberate = AsyncMock(
        return_value=OperativeVote(
            vote=VoteChoice(vote),
            reasoning=f"{op_id} says {vote}",
            utility=0.8,
        )
    )
    return op


def _mentor_config(sub_operatives: dict | None = None) -> CouncillorConfig:
    return CouncillorConfig(
        operative_id="mentor",
        domain=["finances", "budget", "savings"],
        sub_operatives=sub_operatives if sub_operatives is not None else {
            "financial_risk_assessor": "graphs/financial_risk_assessment.json",
            "burn_rate_analyst":       "graphs/burn_rate_analysis.json",
        },
    )


def _configs(
    mentor_sub: dict | None = None,
    extra: dict | None = None,
) -> dict[str, CouncillorConfig]:
    cfg = {
        "mentor":    _mentor_config(sub_operatives=mentor_sub),
        "protege":   CouncillorConfig("protege",   ["tasks", "calendar", "time", "deadlines"],  {"deadline_analyst": "graphs/deadline_analysis.json", "priority_sorter": "graphs/priority_sorting.json"}),
        "attache":   CouncillorConfig("attache",   ["contacts", "social", "relationships", "trust"], {"trust_assessor": "graphs/trust_assessment.json", "relationship_tracker": "graphs/relationship_tracking.json"}),
        "navigator": CouncillorConfig("navigator", ["logistics", "routing", "delivery", "transport"], {"route_optimizer": "graphs/route_optimization.json", "delivery_tracker": "graphs/delivery_tracking.json"}),
        "curator":   CouncillorConfig("curator",   ["assets", "inventory", "procurement", "investment"], {"investment_advisor": "graphs/investment_advisory.json", "asset_valuator": "graphs/asset_valuation.json"}),
    }
    if extra:
        cfg.update(extra)
    return cfg


# ── _create_sandbox() unit tests ──────────────────────────────────────────────


class TestCreateSandbox:

    @pytest.fixture(autouse=True)
    def wipe_forks(self):
        clear_fork_registry()
        yield
        clear_fork_registry()

    @pytest.mark.asyncio
    async def test_returns_fork_id_string(self):
        state   = _make_state()
        council = _make_council(state)
        config  = _mentor_config()

        fork_id, _ = await council._create_sandbox(config)

        assert isinstance(fork_id, str)
        assert len(fork_id) > 0

    @pytest.mark.asyncio
    async def test_fork_registered_in_registry(self):
        state   = _make_state()
        council = _make_council(state)
        config  = _mentor_config()

        fork_id, _ = await council._create_sandbox(config)

        assert fork_id in _FORK_REGISTRY

    @pytest.mark.asyncio
    async def test_sandbox_results_keyed_by_sub_op_name(self):
        state   = _make_state()
        council = _make_council(state)
        config  = _mentor_config()

        _, sandbox_results = await council._create_sandbox(config)

        assert "financial_risk_assessor" in sandbox_results
        assert "burn_rate_analyst"       in sandbox_results

    @pytest.mark.asyncio
    async def test_each_sub_op_result_has_status(self):
        state   = _make_state()
        council = _make_council(state)
        config  = _mentor_config()

        _, sandbox_results = await council._create_sandbox(config)

        for name, result in sandbox_results.items():
            assert "status" in result, f"{name}: missing 'status' key"

    @pytest.mark.asyncio
    async def test_ok_sub_op_has_required_fields(self):
        state   = _make_state()
        council = _make_council(state)
        # burn_rate_analysis uses budget.summary — should succeed
        config  = CouncillorConfig(
            operative_id="mentor",
            domain=["finances"],
            sub_operatives={"burn_rate": "graphs/burn_rate_analysis.json"},
        )

        _, sandbox_results = await council._create_sandbox(config)

        result = sandbox_results["burn_rate"]
        assert result["status"] == "ok"
        assert "operator_name"      in result
        assert "rationale"          in result
        assert "simulation_results" in result

    @pytest.mark.asyncio
    async def test_two_sandboxes_produce_different_fork_ids(self):
        """Each _create_sandbox() call must produce a unique fork — no sharing."""
        state    = _make_state()
        council  = _make_council(state)
        config_a = CouncillorConfig(
            operative_id="mentor", domain=["finances"],
            sub_operatives={"burn_rate": "graphs/burn_rate_analysis.json"},
        )
        config_b = CouncillorConfig(
            operative_id="curator", domain=["assets"],
            sub_operatives={"burn_rate": "graphs/burn_rate_analysis.json"},
        )

        fork_id_a, _ = await council._create_sandbox(config_a)
        fork_id_b, _ = await council._create_sandbox(config_b)

        assert fork_id_a != fork_id_b

    @pytest.mark.asyncio
    async def test_missing_graph_file_recorded_as_not_found(self):
        state   = _make_state()
        council = _make_council(state)
        config  = CouncillorConfig(
            operative_id="mentor", domain=["finances"],
            sub_operatives={"ghost": "graphs/does_not_exist.json"},
        )

        fork_id, sandbox_results = await council._create_sandbox(config)

        assert fork_id is not None
        assert sandbox_results["ghost"]["status"] == "not_found"

    @pytest.mark.asyncio
    async def test_no_sub_operatives_returns_empty_sandbox(self):
        state   = _make_state()
        council = _make_council(state)
        config  = CouncillorConfig(
            operative_id="mentor", domain=["finances"], sub_operatives={}
        )

        # _create_sandbox should not be called in this case, but test it directly
        # to verify robustness: it still forks and returns empty results
        fork_id, sandbox_results = await council._create_sandbox(config)

        assert fork_id is not None
        assert sandbox_results == {}


# ── collect_votes() sandbox integration ───────────────────────────────────────


class TestCollectVotesSandbox:

    @pytest.fixture(autouse=True)
    def wipe_forks(self):
        clear_fork_registry()
        yield
        clear_fork_registry()

    @pytest.mark.asyncio
    async def test_fork_id_on_vote_record_in_state(self):
        """Relevant councillor with sub_operatives → fork_id stored on vote record."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        await council.collect_votes(pid, [mentor], councillor_configs=_configs())

        votes = state.get("council_votes")
        assert len(votes) == 1
        assert votes[0]["fork_id"] is not None
        assert isinstance(votes[0]["fork_id"], str)

    @pytest.mark.asyncio
    async def test_sandbox_results_on_vote_record(self):
        """sandbox_results appear on the vote record when sub-operatives run."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        await council.collect_votes(pid, [mentor], councillor_configs=_configs())

        votes = state.get("council_votes")
        sr = votes[0].get("sandbox_results")
        assert sr is not None
        assert "financial_risk_assessor" in sr
        assert "burn_rate_analyst"       in sr

    @pytest.mark.asyncio
    async def test_fork_id_in_return_value(self):
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        results = await council.collect_votes(
            pid, [mentor], councillor_configs=_configs()
        )

        assert results["mentor"]["fork_id"] is not None

    @pytest.mark.asyncio
    async def test_sandbox_results_in_return_value(self):
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        results = await council.collect_votes(
            pid, [mentor], councillor_configs=_configs()
        )

        assert "sandbox_results" in results["mentor"]

    @pytest.mark.asyncio
    async def test_deliberate_not_called_when_sub_ops_ran(self):
        """When sub-operatives produce delegated votes, deliberate() is skipped."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        results = await council.collect_votes(pid, [mentor], councillor_configs=_configs())

        # Delegated path taken — deliberate() never called
        mentor.deliberate.assert_not_called()
        # Vote is still recorded in results
        assert "vote" in results["mentor"]
        # delegated_votes present in return value
        assert "delegated_votes" in results["mentor"]

    @pytest.mark.asyncio
    async def test_two_relevant_councillors_get_different_forks(self):
        """
        Two councillors relevant to a finances+procurement proposal must each
        receive a distinct fork — no two councillors share a fork.
        """
        state   = _make_state()
        council = _make_council(state)
        # domains=["finances","procurement"] → both mentor and curator are relevant
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
            domains=["finances", "procurement"],
        )
        mentor  = _make_operative("mentor",  vote="YES")
        curator = _make_operative("curator", vote="YES")

        results = await council.collect_votes(
            pid, [mentor, curator], councillor_configs=_configs()
        )

        fid_mentor  = results["mentor"]["fork_id"]
        fid_curator = results["curator"]["fork_id"]
        assert fid_mentor  is not None
        assert fid_curator is not None
        assert fid_mentor != fid_curator, "councillors must not share a fork"

    @pytest.mark.asyncio
    async def test_abstained_councillor_has_no_fork(self):
        """Councillor that auto-abstains due to no domain overlap must not create a fork."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        attache = _make_operative("attache")  # contacts domain — no overlap with budget

        await council.collect_votes(pid, [attache], councillor_configs=_configs())

        votes = state.get("council_votes")
        assert len(votes) == 1
        assert votes[0]["fork_id"] is None
        assert _FORK_REGISTRY == {}

    @pytest.mark.asyncio
    async def test_no_sub_operatives_no_fork_created(self):
        """Relevant councillor with empty sub_operatives does not create a fork."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")
        # config with empty sub_operatives
        configs = {"mentor": CouncillorConfig("mentor", ["finances", "budget"], {})}

        await council.collect_votes(pid, [mentor], councillor_configs=configs)

        assert _FORK_REGISTRY == {}
        votes = state.get("council_votes")
        assert votes[0]["fork_id"] is None

    @pytest.mark.asyncio
    async def test_fork_failure_degrades_gracefully(self):
        """If simulate.fork raises, deliberation still proceeds with no fork_id."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        # Patch _create_sandbox directly to simulate a fork failure
        async def _failing_sandbox(config):
            return None, {}

        council._create_sandbox = _failing_sandbox

        results = await council.collect_votes(
            pid, [mentor], councillor_configs=_configs()
        )

        # Deliberation still happened
        mentor.deliberate.assert_called_once()
        assert results["mentor"]["vote"] == "YES"
        assert results["mentor"]["fork_id"] is None

    @pytest.mark.asyncio
    async def test_backwards_compat_no_configs(self):
        """Without councillor_configs, no sandbox is created — all deliberate normally."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        results = await council.collect_votes(pid, [mentor])  # no configs

        mentor.deliberate.assert_called_once()
        assert _FORK_REGISTRY == {}
        assert results["mentor"]["vote"] == "YES"

    @pytest.mark.asyncio
    async def test_three_councillors_mixed_relevance(self):
        """
        budget proposal → mentor (relevant), attache (irrelevant), protege (irrelevant).
        Only mentor gets a fork. Others auto-abstain.
        """
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor  = _make_operative("mentor",  vote="YES")
        attache = _make_operative("attache")
        protege = _make_operative("protege")

        results = await council.collect_votes(
            pid, [mentor, attache, protege], councillor_configs=_configs()
        )

        # mentor has sub_operatives → delegated path → deliberate() NOT called
        mentor.deliberate.assert_not_called()
        attache.deliberate.assert_not_called()
        protege.deliberate.assert_not_called()

        assert results["mentor"]["fork_id"]  is not None
        assert results["attache"]["fork_id"] is None
        assert results["protege"]["fork_id"] is None

        # Only mentor created a fork
        assert len(_FORK_REGISTRY) == 1


# ── Sprint 7.5 — delegated vote wiring in collect_votes ──────────────────────


class TestCollectVotesDelegation:

    @pytest.fixture(autouse=True)
    def wipe_forks(self):
        clear_fork_registry()
        yield
        clear_fork_registry()

    @pytest.mark.asyncio
    async def test_delegated_votes_on_vote_record_when_sub_ops_run(self):
        """Vote record in state has delegated_votes list when sub-ops ran."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        await council.collect_votes(pid, [mentor], councillor_configs=_configs())

        votes = state.get("council_votes")
        assert len(votes) == 1
        assert votes[0]["delegated_votes"] is not None
        assert isinstance(votes[0]["delegated_votes"], list)
        assert len(votes[0]["delegated_votes"]) > 0

    @pytest.mark.asyncio
    async def test_delegated_votes_none_when_no_sub_ops(self):
        """Vote record has delegated_votes=None when no sub-operatives ran."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")
        configs = {"mentor": CouncillorConfig("mentor", ["finances", "budget"], {})}

        await council.collect_votes(pid, [mentor], councillor_configs=configs)

        votes = state.get("council_votes")
        assert votes[0]["delegated_votes"] is None

    @pytest.mark.asyncio
    async def test_delegated_votes_each_entry_has_required_fields(self):
        """Each delegated_vote entry has position, confidence, reasoning."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor")

        await council.collect_votes(pid, [mentor], councillor_configs=_configs())

        votes = state.get("council_votes")
        for dv in votes[0]["delegated_votes"]:
            assert "position"   in dv
            assert "confidence" in dv
            assert "reasoning"  in dv
            assert dv["position"] in ("YES", "NO", "ABSTAIN")

    @pytest.mark.asyncio
    async def test_deliberate_still_called_after_fork_failure(self):
        """Fork failure → sandbox_results={} → delegated=[] → deliberate() runs."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        async def _failing_sandbox(config):
            return None, {}

        council._create_sandbox = _failing_sandbox

        results = await council.collect_votes(
            pid, [mentor], councillor_configs=_configs()
        )

        mentor.deliberate.assert_called_once()
        assert results["mentor"]["vote"] == "YES"

    @pytest.mark.asyncio
    async def test_deliberate_called_when_no_sub_operatives(self):
        """Config with empty sub_operatives → no sandbox → deliberate() runs."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="NO")
        configs = {"mentor": CouncillorConfig("mentor", ["finances", "budget"], {})}

        results = await council.collect_votes(pid, [mentor], councillor_configs=configs)

        mentor.deliberate.assert_called_once()
        assert results["mentor"]["vote"] == "NO"

    @pytest.mark.asyncio
    async def test_sub_ops_with_yes_vote_produce_yes_outcome(self):
        """Sub-op returning vote=YES in simulation_results → councillor votes YES."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor")

        # Patch _create_sandbox to return a YES sub-op result
        async def _yes_sandbox(config):
            return "fork-yes", {
                "assessor": {
                    "status": "ok",
                    "operator_name": "budget.summary",
                    "rationale": "all good",
                    "simulation_results": {
                        "vote": "YES",
                        "confidence": 0.9,
                        "reasoning": "budget is healthy",
                    },
                }
            }

        council._create_sandbox = _yes_sandbox

        results = await council.collect_votes(
            pid, [mentor], councillor_configs=_configs()
        )

        mentor.deliberate.assert_not_called()
        assert results["mentor"]["vote"] == "YES"
        assert results["mentor"]["fork_id"] == "fork-yes"

    @pytest.mark.asyncio
    async def test_sub_ops_all_error_produce_abstain_via_delegation(self):
        """All sub-op errors → all ABSTAIN delegated → aggregate returns ABSTAIN."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor")

        async def _error_sandbox(config):
            return "fork-err", {
                "assessor": {"status": "error", "reason": "timeout"},
            }

        council._create_sandbox = _error_sandbox

        results = await council.collect_votes(
            pid, [mentor], councillor_configs=_configs()
        )

        # delegated list is non-empty (one ABSTAIN entry) → delegation path taken
        mentor.deliberate.assert_not_called()
        assert results["mentor"]["vote"] == "ABSTAIN"
        assert results["mentor"]["delegated_votes"] is not None

    @pytest.mark.asyncio
    async def test_deliberate_fallback_no_councillor_configs(self):
        """Without councillor_configs, no sandbox, deliberate() always runs."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_operative("mentor", vote="YES")

        results = await council.collect_votes(pid, [mentor])  # no configs

        mentor.deliberate.assert_called_once()
        votes = state.get("council_votes")
        assert votes[0]["delegated_votes"] is None
        assert results["mentor"]["vote"] == "YES"
