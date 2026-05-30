"""
tests/test_council_operatives.py

Tests for the four council operatives: ProtegeOperative, AttacheOperative,
NavigatorOperative, and CuratorOperative.

Each operative is tested against its correct document-defined role:
  - ProtegeOperative  : scheduling (tasks, calendar, delegation)
  - AttacheOperative  : network/profiler (M-Pesa, contacts, Chama Cred)
  - NavigatorOperative: logistics (orders, delivery, fleet)
  - CuratorOperative  : asset register / inventory

Coverage:
  - should_evaluate() True/False for each operative's specific threshold
  - evaluate() → OperativeProposal or None
  - deliberate() YES/NO/ABSTAIN for each operative's specific domain logic
  - All mocked — _call_claude never makes real API calls

All Anthropic API calls are mocked — zero real HTTP calls.
"""

import json
import pytest
from datetime import datetime, timezone, timedelta
from unittest.mock import AsyncMock, MagicMock

from sustena.core.state import StateAccessor
from sustena.operatives.base import HAIKU, OperativeProposal, OperativeVote, VoteChoice
from sustena.operatives.protege   import ProtegeOperative
from sustena.operatives.attache   import AttacheOperative
from sustena.operatives.navigator import NavigatorOperative
from sustena.operatives.curator   import CuratorOperative


# ── Shared helpers ─────────────────────────────────────────────────────────────

def _make_mock_claude_client(response_text: str = None) -> MagicMock:
    """Return a mock Anthropic client whose .messages.create() is an AsyncMock."""
    mock_usage = MagicMock()
    mock_usage.input_tokens  = 15
    mock_usage.output_tokens = 25

    mock_content      = MagicMock()
    mock_content.text = response_text or json.dumps(
        {"vote": "YES", "reasoning": "Looks good.", "utility": 0.8}
    )

    mock_message         = MagicMock()
    mock_message.content = [mock_content]
    mock_message.usage   = mock_usage
    mock_message.model   = HAIKU

    mock_messages        = MagicMock()
    mock_messages.create = AsyncMock(return_value=mock_message)

    client          = MagicMock()
    client.messages = mock_messages
    return client


def _past_iso(hours: int = 2) -> str:
    """ISO datetime string N hours in the PAST."""
    return (datetime.now(tz=timezone.utc) - timedelta(hours=hours)).isoformat()


def _future_iso(hours: int = 12) -> str:
    """ISO datetime string N hours in the FUTURE."""
    return (datetime.now(tz=timezone.utc) + timedelta(hours=hours)).isoformat()


# ═══════════════════════════════════════════════════════════════════════════════
# ProtegeOperative — scheduling
# ═══════════════════════════════════════════════════════════════════════════════

class TestProtegeOperative:

    def _make_state(self, tasks: list = None, events: list = None, time_blocks: list = None) -> StateAccessor:
        return StateAccessor({
            "schedule": {
                "tasks":       tasks or [],
                "time_blocks": time_blocks or [],
            },
            "calendar": {"events": events or []},
        })

    def _make_protege(self, state: StateAccessor, response: str = None, config: dict = None) -> ProtegeOperative:
        client = _make_mock_claude_client(response)
        return ProtegeOperative(config=config or {}, state_accessor=state, claude_client=client)

    # ── should_evaluate ────────────────────────────────────────────────────────

    def test_should_evaluate_true_on_past_due_task(self):
        """Task with due_date in the past → True."""
        state   = self._make_state(tasks=[{
            "id": "t1", "name": "Send invoice",
            "due_date": _past_iso(hours=3), "assigned_to": "Bonnie",
        }])
        protege = self._make_protege(state)
        assert protege.should_evaluate(state) is True

    def test_should_evaluate_true_on_unassigned_urgent_task(self):
        """Unassigned task due in <24h → True."""
        state   = self._make_state(tasks=[{
            "id": "t2", "name": "Buy supplies",
            "due_date": _future_iso(hours=10), "assigned_to": None,
        }])
        protege = self._make_protege(state)
        assert protege.should_evaluate(state) is True

    def test_should_evaluate_false_assigned_future_task(self):
        """Assigned task due in >24h → False."""
        state   = self._make_state(tasks=[{
            "id": "t3", "name": "Review report",
            "due_date": _future_iso(hours=48), "assigned_to": "Bonnie",
        }])
        protege = self._make_protege(state)
        assert protege.should_evaluate(state) is False

    def test_should_evaluate_false_no_tasks(self):
        state   = self._make_state(tasks=[])
        protege = self._make_protege(state)
        assert protege.should_evaluate(state) is False

    def test_should_evaluate_false_unassigned_but_far_future(self):
        """Unassigned task due in 48h → outside default 24h window → False."""
        state   = self._make_state(tasks=[{
            "id": "t4", "name": "Long task",
            "due_date": _future_iso(hours=48), "assigned_to": None,
        }])
        protege = self._make_protege(state)
        assert protege.should_evaluate(state) is False

    def test_should_evaluate_no_llm_call(self):
        """should_evaluate must not call the LLM."""
        state   = self._make_state(tasks=[{
            "id": "t1", "name": "Overdue",
            "due_date": _past_iso(), "assigned_to": None,
        }])
        protege = self._make_protege(state)
        protege._call_claude = AsyncMock()
        assert protege.should_evaluate(state) is True
        protege._call_claude.assert_not_called()

    def test_custom_urgent_window_hours(self):
        """urgent_window_hours=48 → unassigned task due in 36h triggers (default 24 would not)."""
        state = self._make_state(tasks=[{
            "id": "t1", "name": "Task",
            "due_date": _future_iso(hours=36), "assigned_to": None,
        }])
        default = self._make_protege(state)
        assert default.should_evaluate(state) is False  # 36h > 24h default

        custom = self._make_protege(state, config={"urgent_window_hours": 48})
        assert custom.should_evaluate(state) is True   # 36h < 48h custom

    # ── evaluate ──────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_evaluate_returns_proposal_for_past_due_task(self):
        state = self._make_state(tasks=[{
            "id": "t1", "name": "File tax",
            "due_date": _past_iso(hours=5), "assigned_to": None,
        }])
        protege = self._make_protege(state, response=json.dumps({
            "action": "alert", "task_id": "t1", "assignee": None,
            "time_block": None, "rationale": "Task is overdue.",
        }))
        proposal = await protege.evaluate({"event_name": "event.task.created"})
        assert proposal is not None
        assert isinstance(proposal, OperativeProposal)

    @pytest.mark.asyncio
    async def test_evaluate_returns_assign_proposal(self):
        state = self._make_state(tasks=[{
            "id": "t2", "name": "Restock",
            "due_date": _future_iso(hours=5), "assigned_to": None,
        }])
        protege = self._make_protege(state, response=json.dumps({
            "action": "assign", "task_id": "t2", "assignee": None,
            "time_block": "morning", "rationale": "Assign to morning block.",
        }))
        proposal = await protege.evaluate({"event_name": "event.calendar.event_added"})
        assert proposal is not None
        assert proposal.operator_name == "schedule.assign"

    @pytest.mark.asyncio
    async def test_evaluate_returns_none_when_no_triggers(self):
        state   = self._make_state(tasks=[{
            "id": "t1", "name": "Distant task",
            "due_date": _future_iso(hours=72), "assigned_to": "Bonnie",
        }])
        protege  = self._make_protege(state)
        proposal = await protege.evaluate({})
        assert proposal is None

    @pytest.mark.asyncio
    async def test_evaluate_falls_back_on_invalid_json(self):
        state = self._make_state(tasks=[{
            "id": "t1", "name": "Overdue",
            "due_date": _past_iso(), "assigned_to": None,
        }])
        protege  = self._make_protege(state, response="not valid json")
        proposal = await protege.evaluate({})
        assert proposal is not None
        assert proposal.operator_name == "sustena.alert"

    # ── deliberate ────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_deliberate_abstains_on_non_scheduling_proposal(self):
        state   = self._make_state()
        protege = self._make_protege(state)
        vote    = await protege.deliberate(
            context={}, proposal={"operator_name": "budget.allocate", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN
        protege.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_no_on_locked_time_block(self):
        state = self._make_state(time_blocks=[{
            "id": "blk1", "label": "Morning", "start": "08:00", "locked": True
        }])
        protege = self._make_protege(state)
        vote = await protege.deliberate(
            context={},
            proposal={
                "operator_name": "schedule.assign",
                "input_params":  {"task_id": "t1", "time_block": "blk1"},
            }
        )
        assert vote.vote == VoteChoice.NO
        protege.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_yes_via_llm(self):
        state   = self._make_state()
        protege = self._make_protege(state, response=json.dumps({
            "vote": "YES", "reasoning": "Resolves conflict.", "utility": 0.9
        }))
        vote = await protege.deliberate(
            context={},
            proposal={"operator_name": "schedule.assign", "input_params": {"task_id": "t1"}}
        )
        assert vote.vote == VoteChoice.YES

    @pytest.mark.asyncio
    async def test_deliberate_uses_haiku_model(self):
        state   = self._make_state()
        protege = self._make_protege(state, response=json.dumps({
            "vote": "YES", "reasoning": "OK.", "utility": 0.7
        }))
        await protege.deliberate(
            context={},
            proposal={"operator_name": "schedule.assign", "input_params": {"task_id": "t1"}}
        )
        call_kwargs = protege.claude_client.messages.create.call_args
        used_model  = call_kwargs.kwargs.get("model") or (call_kwargs.args[0] if call_kwargs.args else None)
        assert used_model == HAIKU

    @pytest.mark.asyncio
    async def test_deliberate_falls_back_on_invalid_json(self):
        state   = self._make_state()
        protege = self._make_protege(state, response="not json")
        vote    = await protege.deliberate(
            context={}, proposal={"operator_name": "schedule.assign", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN


# ═══════════════════════════════════════════════════════════════════════════════
# AttacheOperative — network/profiler
# ═══════════════════════════════════════════════════════════════════════════════

class TestAttacheOperative:

    def _make_state(
        self,
        contacts: list = None,
        transactions: list = None,
    ) -> StateAccessor:
        return StateAccessor({
            "network": {"contacts": contacts or []},
            "finances": {
                "mpesa_transactions": transactions or [],
            },
        })

    def _make_attache(self, state: StateAccessor, response: str = None, config: dict = None) -> AttacheOperative:
        client = _make_mock_claude_client(response)
        return AttacheOperative(config=config or {}, state_accessor=state, claude_client=client)

    # ── should_evaluate ────────────────────────────────────────────────────────

    def test_should_evaluate_true_when_unknown_phone_in_transactions(self):
        """Transaction with phone not in contacts → True."""
        state   = self._make_state(
            contacts=[{"phone": "+254700000001", "name": "Alice"}],
            transactions=[{"counterparty_phone": "+254722XXXXXX", "amount": 500}],
        )
        attache = self._make_attache(state)
        assert attache.should_evaluate(state) is True

    def test_should_evaluate_false_when_all_phones_known(self):
        """All transaction phones are in contacts → False."""
        state   = self._make_state(
            contacts=[{"phone": "+254700000001", "name": "Alice"}],
            transactions=[{"counterparty_phone": "+254700000001", "amount": 200}],
        )
        attache = self._make_attache(state)
        assert attache.should_evaluate(state) is False

    def test_should_evaluate_false_no_transactions(self):
        state   = self._make_state(contacts=[], transactions=[])
        attache = self._make_attache(state)
        assert attache.should_evaluate(state) is False

    def test_should_evaluate_no_llm_call(self):
        state = self._make_state(
            contacts=[],
            transactions=[{"counterparty_phone": "+254799999999", "amount": 100}],
        )
        attache = self._make_attache(state)
        attache._call_claude = AsyncMock()
        assert attache.should_evaluate(state) is True
        attache._call_claude.assert_not_called()

    # ── evaluate ──────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_evaluate_returns_enrich_proposal_for_unknown_phone(self):
        state = self._make_state(
            contacts=[],
            transactions=[
                {"counterparty_phone": "+254722XXXXXX", "amount": 1200},
                {"counterparty_phone": "+254722XXXXXX", "amount": 800},
                {"counterparty_phone": "+254722XXXXXX", "amount": 500},
            ],
        )
        attache = self._make_attache(state, response=json.dumps({
            "phone": "+254722XXXXXX", "suggested_name": None,
            "relationship_type": "vendor",
            "rationale": "I see KES 2,500 to +254722XXXXXX across 3 transactions. Should I add them to your network?",
        }))
        proposal = await attache.evaluate({"event_name": "event.finances.mpesa_transaction"})
        assert proposal is not None
        assert isinstance(proposal, OperativeProposal)
        assert proposal.operator_name == "network.enrich"

    @pytest.mark.asyncio
    async def test_evaluate_returns_none_when_all_known(self):
        state = self._make_state(
            contacts=[{"phone": "+254700000001", "name": "Bob"}],
            transactions=[{"counterparty_phone": "+254700000001", "amount": 300}],
        )
        attache  = self._make_attache(state)
        proposal = await attache.evaluate({})
        assert proposal is None

    @pytest.mark.asyncio
    async def test_evaluate_falls_back_on_invalid_json(self):
        state = self._make_state(
            contacts=[],
            transactions=[{"counterparty_phone": "+254711111111", "amount": 600}],
        )
        attache  = self._make_attache(state, response="bad json")
        proposal = await attache.evaluate({})
        assert proposal is not None
        assert proposal.operator_name == "network.enrich"

    # ── deliberate ────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_deliberate_abstains_on_non_network_proposal(self):
        state   = self._make_state()
        attache = self._make_attache(state)
        vote    = await attache.deliberate(
            context={}, proposal={"operator_name": "budget.allocate", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN
        attache.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_no_on_poor_chama_cred(self):
        """Counterparty with Chama Cred < 40 → hard NO without LLM."""
        state = self._make_state(contacts=[{
            "phone": "+254733333333", "name": "Bad Actor",
            "chama_cred_score": 20,
        }])
        attache = self._make_attache(state)
        vote = await attache.deliberate(
            context={},
            proposal={
                "operator_name": "network.enrich",
                "input_params":  {"phone": "+254733333333"},
            }
        )
        assert vote.vote == VoteChoice.NO
        attache.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_yes_on_good_cred_via_llm(self):
        state = self._make_state(contacts=[{
            "phone": "+254744444444", "name": "Good Contact",
            "chama_cred_score": 85,
        }])
        attache = self._make_attache(state, response=json.dumps({
            "vote": "YES", "reasoning": "Improves social capital.", "utility": 0.9
        }))
        vote = await attache.deliberate(
            context={},
            proposal={
                "operator_name": "network.enrich",
                "input_params":  {"phone": "+254744444444"},
            }
        )
        assert vote.vote == VoteChoice.YES

    @pytest.mark.asyncio
    async def test_deliberate_uses_haiku_model(self):
        state   = self._make_state()
        attache = self._make_attache(state, response=json.dumps({
            "vote": "YES", "reasoning": "Good.", "utility": 0.7
        }))
        await attache.deliberate(
            context={},
            proposal={"operator_name": "network.enrich", "input_params": {}}
        )
        call_kwargs = attache.claude_client.messages.create.call_args
        used_model  = call_kwargs.kwargs.get("model") or (call_kwargs.args[0] if call_kwargs.args else None)
        assert used_model == HAIKU

    @pytest.mark.asyncio
    async def test_deliberate_falls_back_on_invalid_json(self):
        state   = self._make_state()
        attache = self._make_attache(state, response="not json")
        vote    = await attache.deliberate(
            context={}, proposal={"operator_name": "network.enrich", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN

    # ── contact_profile ───────────────────────────────────────────────────────

    def test_contact_profile_known_contact(self):
        state = self._make_state(
            contacts=[{
                "phone": "+254700000001", "name": "Alice",
                "relationship_type": "friend", "chama_cred_score": 90,
            }],
            transactions=[
                {"counterparty_phone": "+254700000001", "amount": 500},
                {"counterparty_phone": "+254700000001", "amount": 300},
            ],
        )
        attache = self._make_attache(state)
        profile = attache.contact_profile("+254700000001")
        assert profile["known"] is True
        assert profile["name"] == "Alice"
        assert profile["chama_cred_score"] == 90
        assert profile["transaction_history_summary"]["txn_count"] == 2
        assert profile["transaction_history_summary"]["total_kes"] == 800.0

    def test_contact_profile_unknown_contact(self):
        state   = self._make_state(contacts=[])
        attache = self._make_attache(state)
        profile = attache.contact_profile("+254799999999")
        assert profile["known"] is False
        assert profile["name"] is None
        assert profile["chama_cred_score"] is None


# ═══════════════════════════════════════════════════════════════════════════════
# NavigatorOperative — logistics
# ═══════════════════════════════════════════════════════════════════════════════

class TestNavigatorOperative:

    def _make_state(
        self,
        orders: list = None,
        fleet: dict = None,
    ) -> StateAccessor:
        return StateAccessor({
            "orders": {"active": orders or []},
            "fleet":  fleet or {},
        })

    def _make_navigator(self, state: StateAccessor, response: str = None, config: dict = None) -> NavigatorOperative:
        client = _make_mock_claude_client(response)
        return NavigatorOperative(config=config or {}, state_accessor=state, claude_client=client)

    # ── should_evaluate ────────────────────────────────────────────────────────

    def test_should_evaluate_true_when_order_has_no_delivery_option(self):
        state     = self._make_state(orders=[{"id": "o1", "status": "pending", "delivery_option": None}])
        navigator = self._make_navigator(state)
        assert navigator.should_evaluate(state) is True

    def test_should_evaluate_false_when_all_orders_assigned(self):
        state     = self._make_state(orders=[{"id": "o1", "status": "pending", "delivery_option": "sendy"}])
        navigator = self._make_navigator(state)
        assert navigator.should_evaluate(state) is False

    def test_should_evaluate_false_no_orders(self):
        state     = self._make_state(orders=[])
        navigator = self._make_navigator(state)
        assert navigator.should_evaluate(state) is False

    def test_should_evaluate_no_llm_call(self):
        state     = self._make_state(orders=[{"id": "o1", "delivery_option": None}])
        navigator = self._make_navigator(state)
        navigator._call_claude = AsyncMock()
        assert navigator.should_evaluate(state) is True
        navigator._call_claude.assert_not_called()

    def test_should_evaluate_false_empty_string_treated_as_falsy(self):
        """delivery_option='' (empty string) is falsy — should trigger."""
        state     = self._make_state(orders=[{"id": "o1", "delivery_option": ""}])
        navigator = self._make_navigator(state)
        assert navigator.should_evaluate(state) is True

    # ── evaluate ──────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_evaluate_returns_logistics_proposal(self):
        state = self._make_state(orders=[{
            "id": "o1", "status": "pending", "delivery_option": None,
            "destination": "CBD",
        }])
        navigator = self._make_navigator(state, response=json.dumps({
            "order_id": "o1", "recommended_option": "sendy",
            "estimated_cost_kes": 350.0, "estimated_minutes": 45,
            "rationale": "Sendy is cheapest for CBD delivery.",
        }))
        proposal = await navigator.evaluate({"event_name": "event.vyyb.order_placed"})
        assert proposal is not None
        assert isinstance(proposal, OperativeProposal)
        assert proposal.operator_name == "logistics.assign_delivery"
        assert proposal.input_params["delivery_option"] == "sendy"

    @pytest.mark.asyncio
    async def test_evaluate_returns_none_when_all_assigned(self):
        state     = self._make_state(orders=[{"id": "o1", "delivery_option": "glovo"}])
        navigator = self._make_navigator(state)
        proposal  = await navigator.evaluate({})
        assert proposal is None

    @pytest.mark.asyncio
    async def test_evaluate_falls_back_on_invalid_json(self):
        state = self._make_state(orders=[{
            "id": "o1", "delivery_option": None, "status": "pending",
        }])
        navigator = self._make_navigator(state, response="bad json")
        proposal  = await navigator.evaluate({})
        assert proposal is not None
        assert proposal.operator_name == "logistics.assign_delivery"

    # ── deliberate ────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_deliberate_abstains_on_non_logistics_proposal(self):
        state     = self._make_state()
        navigator = self._make_navigator(state)
        vote      = await navigator.deliberate(
            context={}, proposal={"operator_name": "budget.allocate", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN
        navigator.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_no_when_cost_exceeds_fuel_budget(self):
        state = self._make_state(fleet={"fuel_budget_kes": 500, "vehicles": []})
        navigator = self._make_navigator(state)
        vote = await navigator.deliberate(
            context={},
            proposal={
                "operator_name": "logistics.assign_delivery",
                "input_params":  {"order_id": "o1", "delivery_option": "sendy", "estimated_cost_kes": 800.0},
            }
        )
        assert vote.vote == VoteChoice.NO
        navigator.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_no_when_in_house_but_no_vehicles(self):
        state = self._make_state(fleet={
            "fuel_budget_kes": 10000,
            "vehicles": [{"id": "v1", "type": "rider", "available": False}],
        })
        navigator = self._make_navigator(state)
        vote = await navigator.deliberate(
            context={},
            proposal={
                "operator_name": "logistics.assign_delivery",
                "input_params":  {"delivery_option": "in_house", "estimated_cost_kes": 100.0},
            }
        )
        assert vote.vote == VoteChoice.NO
        navigator.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_yes_via_llm(self):
        state = self._make_state(fleet={"fuel_budget_kes": 5000, "vehicles": []})
        navigator = self._make_navigator(state, response=json.dumps({
            "vote": "YES", "reasoning": "Cost-effective.", "utility": 0.85
        }))
        vote = await navigator.deliberate(
            context={},
            proposal={
                "operator_name": "logistics.assign_delivery",
                "input_params":  {"delivery_option": "sendy", "estimated_cost_kes": 300.0},
            }
        )
        assert vote.vote == VoteChoice.YES

    @pytest.mark.asyncio
    async def test_deliberate_uses_haiku_model(self):
        state     = self._make_state(fleet={"fuel_budget_kes": 5000, "vehicles": []})
        navigator = self._make_navigator(state, response=json.dumps({
            "vote": "YES", "reasoning": "OK.", "utility": 0.7
        }))
        await navigator.deliberate(
            context={},
            proposal={"operator_name": "logistics.assign_delivery", "input_params": {"estimated_cost_kes": 100}}
        )
        call_kwargs = navigator.claude_client.messages.create.call_args
        used_model  = call_kwargs.kwargs.get("model") or (call_kwargs.args[0] if call_kwargs.args else None)
        assert used_model == HAIKU

    @pytest.mark.asyncio
    async def test_deliberate_falls_back_on_invalid_json(self):
        state     = self._make_state()
        navigator = self._make_navigator(state, response="not json")
        vote      = await navigator.deliberate(
            context={}, proposal={"operator_name": "logistics.assign_delivery", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN

    # ── route_optimize ────────────────────────────────────────────────────────

    def test_route_optimize_assigns_to_available_rider(self):
        state = self._make_state(
            orders=[{"id": "o1"}, {"id": "o2"}],
            fleet={"vehicles": [
                {"id": "r1", "type": "rider", "available": True},
                {"id": "r2", "type": "rider", "available": False},
            ]},
        )
        navigator = self._make_navigator(state)
        result = navigator.route_optimize(
            orders=[{"id": "o1"}, {"id": "o2"}],
            outlet_id="outlet-1",
        )
        assert result["widget"] == "route_plan"
        assert result["total_orders"] == 2
        assert result["available_riders"] == 1
        # Both orders should be assigned to the single available rider
        assigned = [a["assigned_rider"] for a in result["assignments"]]
        assert all(r == "r1" for r in assigned)

    def test_route_optimize_no_riders(self):
        state     = self._make_state(fleet={"vehicles": []})
        navigator = self._make_navigator(state)
        result    = navigator.route_optimize(orders=[{"id": "o1"}], outlet_id="outlet-1")
        assert result["available_riders"] == 0
        assert result["assignments"][0]["assigned_rider"] is None


# ═══════════════════════════════════════════════════════════════════════════════
# CuratorOperative — asset register / inventory
# ═══════════════════════════════════════════════════════════════════════════════

class TestCuratorOperative:

    def _make_state(
        self,
        inventory: list = None,
        asset_register: list = None,
        purchase_history: list = None,
    ) -> StateAccessor:
        return StateAccessor({
            "inventory": {"items": inventory or []},
            "assets":    {"register": asset_register or []},
            "procurement": {"purchase_history": purchase_history or []},
        })

    def _make_curator(self, state: StateAccessor, response: str = None, config: dict = None) -> CuratorOperative:
        client = _make_mock_claude_client(response)
        return CuratorOperative(config=config or {}, state_accessor=state, claude_client=client)

    # ── should_evaluate ────────────────────────────────────────────────────────

    def test_should_evaluate_true_when_item_at_threshold(self):
        """Item quantity == reorder_threshold → True."""
        state   = self._make_state(inventory=[{
            "id": "i1", "name": "Rice 2kg", "quantity": 3, "reorder_threshold": 3
        }])
        curator = self._make_curator(state)
        assert curator.should_evaluate(state) is True

    def test_should_evaluate_true_when_item_below_threshold(self):
        """Item quantity < reorder_threshold → True."""
        state   = self._make_state(inventory=[{
            "id": "i1", "name": "Sugar", "quantity": 1, "reorder_threshold": 5
        }])
        curator = self._make_curator(state)
        assert curator.should_evaluate(state) is True

    def test_should_evaluate_false_when_all_items_above_threshold(self):
        state = self._make_state(inventory=[
            {"id": "i1", "name": "Flour", "quantity": 10, "reorder_threshold": 3},
            {"id": "i2", "name": "Salt",  "quantity": 8,  "reorder_threshold": 2},
        ])
        curator = self._make_curator(state)
        assert curator.should_evaluate(state) is False

    def test_should_evaluate_false_no_inventory(self):
        state   = self._make_state(inventory=[])
        curator = self._make_curator(state)
        assert curator.should_evaluate(state) is False

    def test_should_evaluate_no_llm_call(self):
        state = self._make_state(inventory=[{
            "id": "i1", "name": "Oil", "quantity": 0, "reorder_threshold": 2
        }])
        curator = self._make_curator(state)
        curator._call_claude = AsyncMock()
        assert curator.should_evaluate(state) is True
        curator._call_claude.assert_not_called()

    # ── evaluate ──────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_evaluate_returns_purchase_order_proposal(self):
        state = self._make_state(inventory=[{
            "id": "i1", "name": "Rice", "quantity": 1, "reorder_threshold": 5,
            "unit_cost_kes": 200, "preferred_supplier": "Naivas",
        }])
        curator = self._make_curator(state, response=json.dumps({
            "action": "purchase_order", "item_id": "i1", "quantity": 10,
            "supplier": "Naivas", "estimated_cost_kes": 2000.0,
            "rationale": "Rice is below reorder threshold.",
        }))
        proposal = await curator.evaluate({"event_name": "event.inventory.item_consumed"})
        assert proposal is not None
        assert isinstance(proposal, OperativeProposal)
        assert proposal.operator_name == "inventory.purchase_order"

    @pytest.mark.asyncio
    async def test_evaluate_returns_reallocate_proposal(self):
        state = self._make_state(inventory=[{
            "id": "i1", "name": "Maize", "quantity": 0, "reorder_threshold": 2,
        }])
        curator = self._make_curator(state, response=json.dumps({
            "action": "reallocate", "item_id": "i1", "quantity": 5,
            "supplier": None, "estimated_cost_kes": None,
            "rationale": "Reallocate surplus from warehouse.",
        }))
        proposal = await curator.evaluate({})
        assert proposal is not None
        assert proposal.operator_name == "inventory.reallocate"

    @pytest.mark.asyncio
    async def test_evaluate_returns_none_when_all_stocked(self):
        state = self._make_state(inventory=[{
            "id": "i1", "name": "Beans", "quantity": 20, "reorder_threshold": 5
        }])
        curator  = self._make_curator(state)
        proposal = await curator.evaluate({})
        assert proposal is None

    @pytest.mark.asyncio
    async def test_evaluate_falls_back_on_invalid_json(self):
        state = self._make_state(inventory=[{
            "id": "i1", "name": "Oil", "quantity": 0, "reorder_threshold": 3
        }])
        curator  = self._make_curator(state, response="bad json")
        proposal = await curator.evaluate({})
        assert proposal is not None
        assert proposal.operator_name == "inventory.purchase_order"

    # ── deliberate ────────────────────────────────────────────────────────────

    @pytest.mark.asyncio
    async def test_deliberate_abstains_on_non_inventory_proposal(self):
        state   = self._make_state()
        curator = self._make_curator(state)
        vote    = await curator.deliberate(
            context={}, proposal={"operator_name": "budget.allocate", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN
        curator.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_no_on_unregistered_item(self):
        """Purchasing an item not in inventory register → impulse spending → NO."""
        state = self._make_state(inventory=[
            {"id": "i1", "name": "Rice", "quantity": 5, "reorder_threshold": 2}
        ])
        curator = self._make_curator(state)
        vote = await curator.deliberate(
            context={},
            proposal={
                "operator_name": "inventory.purchase_order",
                "input_params":  {"item_id": "UNKNOWN_ITEM"},
            }
        )
        assert vote.vote == VoteChoice.NO
        curator.claude_client.messages.create.assert_not_called()

    @pytest.mark.asyncio
    async def test_deliberate_yes_via_llm_for_registered_item(self):
        state = self._make_state(inventory=[{
            "id": "i1", "name": "Sugar", "quantity": 1, "reorder_threshold": 4
        }])
        curator = self._make_curator(state, response=json.dumps({
            "vote": "YES", "reasoning": "Restocking below threshold.", "utility": 0.9
        }))
        vote = await curator.deliberate(
            context={},
            proposal={
                "operator_name": "inventory.purchase_order",
                "input_params":  {"item_id": "i1", "quantity": 10},
            }
        )
        assert vote.vote == VoteChoice.YES

    @pytest.mark.asyncio
    async def test_deliberate_uses_haiku_model(self):
        state   = self._make_state()
        curator = self._make_curator(state, response=json.dumps({
            "vote": "YES", "reasoning": "Good.", "utility": 0.7
        }))
        await curator.deliberate(
            context={},
            proposal={"operator_name": "inventory.purchase_order", "input_params": {}}
        )
        call_kwargs = curator.claude_client.messages.create.call_args
        used_model  = call_kwargs.kwargs.get("model") or (call_kwargs.args[0] if call_kwargs.args else None)
        assert used_model == HAIKU

    @pytest.mark.asyncio
    async def test_deliberate_falls_back_on_invalid_json(self):
        state   = self._make_state()
        curator = self._make_curator(state, response="not json")
        vote    = await curator.deliberate(
            context={}, proposal={"operator_name": "inventory.purchase_order", "input_params": {}}
        )
        assert vote.vote == VoteChoice.ABSTAIN


# ═══════════════════════════════════════════════════════════════════════════════
# Cross-operative: log entries
# ═══════════════════════════════════════════════════════════════════════════════

class TestOperativeLogEntries:
    """Verify all four operatives log tokens correctly after LLM calls."""

    @pytest.mark.asyncio
    async def test_protege_logs_tokens(self):
        state = StateAccessor({
            "schedule": {"tasks": [{
                "id": "t1", "name": "Overdue task",
                "due_date": _past_iso(hours=2), "assigned_to": None,
            }], "time_blocks": []},
            "calendar": {"events": []},
        })
        client = _make_mock_claude_client(json.dumps({
            "action": "alert", "task_id": "t1", "assignee": None,
            "time_block": None, "rationale": "Task is overdue.",
        }))
        protege = ProtegeOperative(config={}, state_accessor=state, claude_client=client)
        await protege.evaluate({})
        entries = protege.flush_log_entries()
        assert len(entries) >= 1
        assert entries[0]["llm_tokens_used"] == 40  # 15 + 25 mock

    @pytest.mark.asyncio
    async def test_attache_logs_tokens(self):
        state = StateAccessor({
            "network":  {"contacts": []},
            "finances": {"mpesa_transactions": [
                {"counterparty_phone": "+254799999999", "amount": 500}
            ]},
        })
        client = _make_mock_claude_client(json.dumps({
            "phone": "+254799999999", "suggested_name": None,
            "relationship_type": "unknown", "rationale": "Seen 1 time.",
        }))
        attache = AttacheOperative(config={}, state_accessor=state, claude_client=client)
        await attache.evaluate({})
        entries = attache.flush_log_entries()
        assert len(entries) >= 1
        assert entries[0]["llm_tokens_used"] == 40

    @pytest.mark.asyncio
    async def test_navigator_logs_tokens(self):
        state = StateAccessor({
            "orders": {"active": [{"id": "o1", "delivery_option": None, "status": "pending"}]},
            "fleet":  {"vehicles": []},
        })
        client = _make_mock_claude_client(json.dumps({
            "order_id": "o1", "recommended_option": "sendy",
            "estimated_cost_kes": 300, "estimated_minutes": 30, "rationale": "Best option.",
        }))
        navigator = NavigatorOperative(config={}, state_accessor=state, claude_client=client)
        await navigator.evaluate({})
        entries = navigator.flush_log_entries()
        assert len(entries) >= 1
        assert entries[0]["model"] == HAIKU

    @pytest.mark.asyncio
    async def test_curator_logs_tokens(self):
        state = StateAccessor({
            "inventory":   {"items": [{"id": "i1", "name": "Rice", "quantity": 0, "reorder_threshold": 5}]},
            "assets":      {"register": []},
            "procurement": {"purchase_history": []},
        })
        client = _make_mock_claude_client(json.dumps({
            "action": "purchase_order", "item_id": "i1", "quantity": 10,
            "supplier": None, "estimated_cost_kes": None, "rationale": "Below threshold.",
        }))
        curator = CuratorOperative(config={}, state_accessor=state, claude_client=client)
        await curator.evaluate({})
        entries = curator.flush_log_entries()
        assert len(entries) >= 1
        assert entries[0]["llm_tokens_used"] == 40
