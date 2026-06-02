"""
tests/test_domain_relevance.py

Tests for Sprint 7.2 — domain relevance check and auto-ABSTAIN logic.

Covers:
  - CouncillorConfig.is_relevant() — overlap, no-overlap, untagged proposal, undeclared domain
  - get_proposal_domains() — known operators, unknown operators, empty map entry
  - create_proposal() — domains auto-derived from OPERATOR_DOMAIN_MAP
  - create_proposal() — domains explicit override
  - collect_votes() with councillor_configs — irrelevant councillors auto-ABSTAIN
  - collect_votes() with councillor_configs — relevant councillors deliberate normally
  - collect_votes() without councillor_configs — all deliberate (backwards compat)
  - ABSTAIN vote record includes abstain_reason: "no_domain_overlap"
  - deliberate() is never called for an auto-abstained operative
"""

import json
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from sustena.core.council import (
    CouncillorConfig,
    CouncilSession,
    OPERATOR_DOMAIN_MAP,
    get_proposal_domains,
    load_councillor_configs,
)
from sustena.core.state import StateAccessor
from sustena.operatives.base import HAIKU, OperativeVote, VoteChoice


# ── Helpers ────────────────────────────────────────────────────────────────────

def _make_state() -> StateAccessor:
    return StateAccessor({
        "finances": {
            "liquid": {"balance": 45_000.0},
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


def _make_mock_operative(op_id: str, vote: str = "YES") -> MagicMock:
    """Return a mock operative whose deliberate() returns the given vote."""
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


# ── CouncillorConfig.is_relevant() ────────────────────────────────────────────


class TestIsRelevant:
    def _config(self, domain: list[str]) -> CouncillorConfig:
        return CouncillorConfig(operative_id="test", domain=domain)

    def test_overlap_returns_true(self):
        cfg = self._config(["finances", "budget"])
        assert cfg.is_relevant(["budget", "savings"]) is True

    def test_no_overlap_returns_false(self):
        cfg = self._config(["finances", "budget"])
        assert cfg.is_relevant(["tasks", "deadlines"]) is False

    def test_untagged_proposal_is_relevant_to_all(self):
        """Empty proposal_domains → untagged → relevant to every councillor."""
        cfg = self._config(["finances", "budget"])
        assert cfg.is_relevant([]) is True

    def test_untagged_proposal_relevant_even_with_undeclared_domain(self):
        """Councillor with no domain → undeclared. Untagged proposal still passes."""
        cfg = self._config([])
        assert cfg.is_relevant([]) is True

    def test_tagged_proposal_irrelevant_to_undeclared_domain_councillor(self):
        """Tagged proposal + councillor with no domain → ABSTAIN."""
        cfg = self._config([])
        assert cfg.is_relevant(["finances"]) is False

    def test_single_shared_domain_is_enough(self):
        cfg = self._config(["finances", "budget", "savings"])
        assert cfg.is_relevant(["savings"]) is True

    def test_completely_disjoint_domains(self):
        cfg = self._config(["logistics", "routing"])
        assert cfg.is_relevant(["finances", "budget"]) is False

    def test_all_domains_match(self):
        cfg = self._config(["tasks", "deadlines"])
        assert cfg.is_relevant(["tasks", "deadlines"]) is True

    def test_case_sensitivity(self):
        """Domain matching is case-sensitive — "Finances" ≠ "finances"."""
        cfg = self._config(["finances"])
        assert cfg.is_relevant(["Finances"]) is False


# ── get_proposal_domains() ────────────────────────────────────────────────────


class TestGetProposalDomains:
    def test_known_budget_operator(self):
        domains = get_proposal_domains("budget.allocate")
        assert "finances" in domains
        assert "budget" in domains

    def test_known_income_operator_includes_savings(self):
        domains = get_proposal_domains("budget.record_income")
        assert "savings" in domains

    def test_known_task_operator(self):
        domains = get_proposal_domains("homestead.tasks.add")
        assert "tasks" in domains
        assert "deadlines" in domains

    def test_known_calendar_operator(self):
        domains = get_proposal_domains("homestead.calendar.add_event")
        assert "calendar" in domains
        assert "time" in domains

    def test_known_procurement_operator(self):
        domains = get_proposal_domains("procurement.raise_po")
        assert "procurement" in domains
        assert "inventory" in domains

    def test_delivery_operator_includes_logistics(self):
        domains = get_proposal_domains("procurement.confirm_delivery")
        assert "logistics" in domains
        assert "delivery" in domains

    def test_unknown_operator_returns_empty(self):
        assert get_proposal_domains("nonexistent.operator") == []

    def test_returns_copy_not_reference(self):
        """Mutating the returned list must not corrupt OPERATOR_DOMAIN_MAP."""
        domains = get_proposal_domains("budget.allocate")
        domains.append("INJECTED")
        assert "INJECTED" not in OPERATOR_DOMAIN_MAP.get("budget.allocate", [])

    def test_all_map_values_are_lists(self):
        for op, domains in OPERATOR_DOMAIN_MAP.items():
            assert isinstance(domains, list), f"{op} value is not a list"

    def test_all_domain_values_are_strings(self):
        for op, domains in OPERATOR_DOMAIN_MAP.items():
            for d in domains:
                assert isinstance(d, str), f"{op} domain entry '{d}' is not a string"


# ── create_proposal() domain tagging ─────────────────────────────────────────


class TestCreateProposalDomains:
    def test_auto_derives_domains_from_operator(self):
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="mentor",
            operator_name="budget.allocate", input_params={},
        )
        proposal = council._get_proposal(pid)
        assert "finances" in proposal["domains"]
        assert "budget"   in proposal["domains"]

    def test_unknown_operator_gets_empty_domains(self):
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="unknown.custom_op", input_params={},
        )
        proposal = council._get_proposal(pid)
        assert proposal["domains"] == []

    def test_explicit_domains_override_auto_derived(self):
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
            domains=["custom_domain"],
        )
        proposal = council._get_proposal(pid)
        assert proposal["domains"] == ["custom_domain"]

    def test_explicit_empty_override_possible(self):
        """Passing domains=[] explicitly marks proposal as untagged."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
            domains=[],
        )
        proposal = council._get_proposal(pid)
        assert proposal["domains"] == []

    def test_domains_stored_in_state(self):
        state   = _make_state()
        council = _make_council(state)
        council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="homestead.tasks.add", input_params={},
        )
        proposals = state.get("council_proposals")
        assert len(proposals) == 1
        assert "tasks" in proposals[0]["domains"]

    def test_backwards_compat_no_domains_arg(self):
        """create_proposal() without domains param must not raise."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="mentor",
            operator_name="budget.spend", input_params={"amount": 100},
        )
        assert pid is not None


# ── collect_votes() domain-based ABSTAIN ──────────────────────────────────────


class TestCollectVotesDomainAbstain:

    def _homestead_configs(self) -> dict[str, CouncillorConfig]:
        return {
            "mentor":    CouncillorConfig("mentor",    ["finances", "budget", "savings"], {}),
            "protege":   CouncillorConfig("protege",   ["tasks", "calendar", "time", "deadlines"], {}),
            "attache":   CouncillorConfig("attache",   ["contacts", "social", "relationships", "trust"], {}),
            "navigator": CouncillorConfig("navigator", ["logistics", "routing", "delivery", "transport"], {}),
            "curator":   CouncillorConfig("curator",   ["assets", "inventory", "procurement", "investment"], {}),
        }

    @pytest.mark.asyncio
    async def test_irrelevant_councillor_auto_abstains(self):
        """budget proposal → attache (contacts domain) should auto-ABSTAIN."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        attache = _make_mock_operative("attache")
        configs = self._homestead_configs()

        await council.collect_votes(pid, [attache], councillor_configs=configs)

        votes = state.get("council_votes")
        assert len(votes) == 1
        assert votes[0]["vote"] == "ABSTAIN"
        assert votes[0]["abstain_reason"] == "no_domain_overlap"

    @pytest.mark.asyncio
    async def test_irrelevant_councillor_deliberate_never_called(self):
        """deliberate() must not be invoked when domain check fails."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        attache = _make_mock_operative("attache")
        configs = self._homestead_configs()

        await council.collect_votes(pid, [attache], councillor_configs=configs)

        attache.deliberate.assert_not_called()

    @pytest.mark.asyncio
    async def test_relevant_councillor_deliberates(self):
        """budget proposal → mentor (finances domain) should deliberate normally."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        mentor = _make_mock_operative("mentor", vote="YES")
        configs = self._homestead_configs()

        results = await council.collect_votes(pid, [mentor], councillor_configs=configs)

        mentor.deliberate.assert_called_once()
        assert results["mentor"]["vote"] == "YES"
        assert "abstain_reason" not in results["mentor"]

    @pytest.mark.asyncio
    async def test_mixed_relevance_in_one_call(self):
        """budget proposal → mentor deliberates, attache auto-abstains."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.spend", input_params={},
        )
        mentor  = _make_mock_operative("mentor", vote="YES")
        attache = _make_mock_operative("attache")
        configs = self._homestead_configs()

        results = await council.collect_votes(
            pid, [mentor, attache], councillor_configs=configs
        )

        assert results["mentor"]["vote"] == "YES"
        assert results["attache"]["vote"] == "ABSTAIN"
        assert results["attache"]["abstain_reason"] == "no_domain_overlap"
        mentor.deliberate.assert_called_once()
        attache.deliberate.assert_not_called()

        votes = state.get("council_votes")
        assert len(votes) == 2

    @pytest.mark.asyncio
    async def test_untagged_proposal_all_deliberate(self):
        """Untagged proposal (empty domains) → all councillors deliberate."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="unknown.op", input_params={},
        )
        mentor  = _make_mock_operative("mentor", vote="YES")
        attache = _make_mock_operative("attache", vote="YES")
        configs = self._homestead_configs()

        await council.collect_votes(pid, [mentor, attache], councillor_configs=configs)

        mentor.deliberate.assert_called_once()
        attache.deliberate.assert_called_once()

    @pytest.mark.asyncio
    async def test_without_councillor_configs_all_deliberate(self):
        """When councillor_configs is None, all operatives deliberate (backwards compat)."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        attache = _make_mock_operative("attache", vote="YES")

        # No configs passed — backwards compatible path
        results = await council.collect_votes(pid, [attache])

        attache.deliberate.assert_called_once()
        assert results["attache"]["vote"] == "YES"

    @pytest.mark.asyncio
    async def test_abstain_vote_record_structure(self):
        """Auto-abstain record must have all required fields."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        attache = _make_mock_operative("attache")
        configs = self._homestead_configs()

        await council.collect_votes(pid, [attache], councillor_configs=configs)

        record = state.get("council_votes")[0]
        assert record["vote"]          == "ABSTAIN"
        assert record["abstain_reason"] == "no_domain_overlap"
        assert record["operative_id"]  == "attache"
        assert record["proposal_id"]   == pid
        assert "id"        in record
        assert "timestamp" in record
        assert "weight"    in record
        assert "utility"   in record

    @pytest.mark.asyncio
    async def test_operative_not_in_configs_still_deliberates(self):
        """Operative absent from councillor_configs dict → treated as relevant."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="budget.allocate", input_params={},
        )
        unknown_op = _make_mock_operative("mystery_op", vote="YES")
        # configs only contains mentor — mystery_op has no entry
        configs = {"mentor": CouncillorConfig("mentor", ["finances"], {})}

        await council.collect_votes(pid, [unknown_op], councillor_configs=configs)

        unknown_op.deliberate.assert_called_once()

    @pytest.mark.asyncio
    async def test_task_proposal_protege_relevant_attache_not(self):
        """Task proposal → protégé deliberates, attache auto-abstains."""
        state   = _make_state()
        council = _make_council(state)
        pid = council.create_proposal(
            sustain_id="s", proposed_by="orchie",
            operator_name="homestead.tasks.add", input_params={},
        )
        protege = _make_mock_operative("protege", vote="YES")
        attache = _make_mock_operative("attache")
        configs = self._homestead_configs()

        results = await council.collect_votes(
            pid, [protege, attache], councillor_configs=configs
        )

        protege.deliberate.assert_called_once()
        attache.deliberate.assert_not_called()
        assert results["protege"]["vote"] == "YES"
        assert results["attache"]["abstain_reason"] == "no_domain_overlap"
