"""
sustena/operatives/navigator.py

NavigatorOperative -- logistics operative.

Responsibility:
  - Order tracking, delivery planning, route optimisation, fleet state,
    Vyyb/Mkulima dispatch.
  - System prompt context: active orders in state.orders.active, fleet state
    in state.fleet, pending physical tasks.
  - When active orders have no delivery option assigned, evaluate and propose
    the best delivery option (Sendy, Glovo, in-house driver).

Utility function: minimise time and cost of physical tasks.

should_evaluate() threshold:
  - Any active order has no assigned delivery option.
  Zero LLM calls in this check.
"""

import json
import logging
from typing import Any

from sustena.core.state import StateAccessor
from sustena.operatives.base import (
    HAIKU,
    BaseOperative,
    OperativeProposal,
    OperativeVote,
    VoteChoice,
)

logger = logging.getLogger(__name__)


class NavigatorOperative(BaseOperative):
    """
    Logistics operative.

    Config keys: none currently.
    """

    operative_id = "navigator"

    def __init__(self, config: dict, state_accessor: StateAccessor, claude_client: Any) -> None:
        super().__init__(config, state_accessor, claude_client)

    # -- should_evaluate -- ZERO LLM CALLS --------------------------------

    def should_evaluate(self, state: StateAccessor) -> bool:
        """Return True if any active order has no assigned delivery option."""
        orders: list = state.get("orders.active", [])
        for order in orders:
            if not isinstance(order, dict):
                continue
            if not order.get("delivery_option"):
                logger.debug("[navigator] should_evaluate=True: order %s has no delivery option",
                             order.get("id", "?"))
                return True
        return False

    # -- evaluate ---------------------------------------------------------

    async def evaluate(self, trigger_event: dict) -> "OperativeProposal | None":
        """Triggered by event.vyyb.order_placed or event.homestead.grocery_run_needed."""
        orders: list = self.state.get("orders.active", [])
        fleet: dict  = self.state.get("fleet", {})

        unassigned = [
            o for o in orders
            if isinstance(o, dict) and not o.get("delivery_option")
        ]

        if not unassigned:
            return None

        system_prompt = self._build_system_prompt(orders, fleet)

        user_message = (
            f"Trigger event: {json.dumps(trigger_event)}\n\n"
            f"Unassigned orders (no delivery option):\n"
            f"{json.dumps(unassigned, indent=2)}\n\n"
            f"Fleet state:\n{json.dumps(fleet, indent=2)}\n\n"
            "Evaluate delivery options:\n"
            "  - sendy: Sendy API\n"
            "  - glovo: Glovo on-demand\n"
            "  - in_house: in-house driver (only if fleet has available vehicle)\n\n"
            "Recommend the option that minimises cost and delivery time.\n"
            "Respond ONLY with valid JSON:\n"
            '{"order_id": "<id>", "recommended_option": "sendy" | "glovo" | "in_house", '
            '"estimated_cost_kes": <float>, "estimated_minutes": <int>, "rationale": "<brief>"}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed = json.loads(raw)
        except (json.JSONDecodeError, ValueError):
            logger.warning("[navigator] LLM returned non-JSON; falling back to sendy")
            first = unassigned[0]
            parsed = {
                "order_id":           first.get("id"),
                "recommended_option": "sendy",
                "estimated_cost_kes": None,
                "estimated_minutes":  None,
                "rationale":          f"Order '{first.get('id', '?')}' needs a delivery option.",
            }

        order_id  = parsed.get("order_id", unassigned[0].get("id"))
        option    = parsed.get("recommended_option", "sendy")
        cost_kes  = parsed.get("estimated_cost_kes")
        minutes   = parsed.get("estimated_minutes")
        rationale = parsed.get("rationale", "Unassigned order requires logistics decision.")

        return OperativeProposal(
            operator_name="logistics.assign_delivery",
            input_params={
                "order_id":           order_id,
                "delivery_option":    option,
                "estimated_cost_kes": cost_kes,
                "estimated_minutes":  minutes,
                "reason":             f"NavigatorOperative: {rationale}",
            },
            rationale=rationale,
            simulation_results={
                "unassigned_orders": unassigned,
                "fleet_state":       fleet,
            },
        )

    # -- deliberate -------------------------------------------------------

    async def deliberate(self, context: dict, proposal: dict) -> OperativeVote:
        """Vote on a Council proposal."""
        operator_name = proposal.get("operator_name", "")
        input_params  = proposal.get("input_params", {})

        logistics_domains = ("logistics.", "vyyb.", "fleet.", "sustena.dispatch")
        if not any(operator_name.startswith(d) for d in logistics_domains):
            return OperativeVote(
                vote=VoteChoice.ABSTAIN,
                reasoning="Proposal is outside NavigatorOperative's domain (logistics/delivery).",
                utility=0.5,
            )

        fleet: dict    = self.state.get("fleet", {})
        fuel_budget    = float(fleet.get("fuel_budget_kes", 0) or 0)
        estimated_cost = float(input_params.get("estimated_cost_kes", 0) or 0)

        if fuel_budget > 0 and estimated_cost > fuel_budget:
            return OperativeVote(
                vote=VoteChoice.NO,
                reasoning=(
                    f"NO: estimated delivery cost KES {estimated_cost:,.0f} exceeds "
                    f"fleet fuel budget KES {fuel_budget:,.0f}."
                ),
                utility=0.1,
            )

        if input_params.get("delivery_option") == "in_house":
            vehicles: list = fleet.get("vehicles", [])
            available = [v for v in vehicles if isinstance(v, dict) and v.get("available")]
            if not available:
                return OperativeVote(
                    vote=VoteChoice.NO,
                    reasoning="NO: in-house delivery requested but no vehicle is available.",
                    utility=0.1,
                )

        system_prompt = self._build_system_prompt(self.state.get("orders.active", []), fleet)

        user_message = (
            f"Council proposal:\n{json.dumps(proposal, indent=2)}\n\n"
            f"Context:\n{json.dumps(context, indent=2)}\n\n"
            "Vote YES if this minimises delivery time and cost without conflicts.\n"
            "Vote NO if this creates logistics conflicts or exceeds budget.\n"
            "Vote ABSTAIN if you cannot determine the logistics impact.\n\n"
            "Respond ONLY with valid JSON:\n"
            '{"vote": "YES" | "NO" | "ABSTAIN", "reasoning": "<brief>", "utility": <0.0-1.0>}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed    = json.loads(raw)
            vote_str  = parsed.get("vote", "ABSTAIN").upper()
            vote      = VoteChoice(vote_str) if vote_str in VoteChoice.__members__ else VoteChoice.ABSTAIN
            reasoning = parsed.get("reasoning", "LLM deliberation complete.")
            utility   = float(parsed.get("utility", 0.5))
            utility   = max(0.0, min(1.0, utility))
        except (json.JSONDecodeError, ValueError, KeyError) as exc:
            logger.warning("[navigator] deliberate LLM parse error: %s", exc)
            vote      = VoteChoice.ABSTAIN
            reasoning = "Could not parse LLM deliberation response; defaulting to ABSTAIN."
            utility   = 0.5

        return OperativeVote(vote=vote, reasoning=reasoning, utility=utility)

    # -- route_optimize ---------------------------------------------------

    def route_optimize(self, orders: list, outlet_id: str) -> dict:
        """Phase 1: group orders by delivery zone and assign to available riders."""
        fleet: dict    = self.state.get("fleet", {})
        vehicles: list = fleet.get("vehicles", [])
        available_riders = [
            v for v in vehicles
            if isinstance(v, dict) and v.get("available") and v.get("type") == "rider"
        ]

        assignments = []
        for idx, order in enumerate(orders):
            if not isinstance(order, dict):
                continue
            rider = available_riders[idx % len(available_riders)]["id"] if available_riders else None
            assignments.append({
                "order_id":       order.get("id"),
                "outlet_id":      outlet_id,
                "assigned_rider": rider,
                "zone":           "default",
            })

        return {
            "widget":           "route_plan",
            "outlet_id":        outlet_id,
            "total_orders":     len(assignments),
            "available_riders": len(available_riders),
            "assignments":      assignments,
        }

    # -- Internal helpers -------------------------------------------------

    def _build_system_prompt(self, orders: list, fleet: dict) -> str:
        order_lines = []
        for o in orders:
            if isinstance(o, dict):
                option = o.get("delivery_option") or "UNASSIGNED"
                order_lines.append(
                    f"  [{o.get('id','?')}] status={o.get('status','?')} delivery={option}"
                )
        vehicle_lines = []
        for v in fleet.get("vehicles", []):
            if isinstance(v, dict):
                avail = "available" if v.get("available") else "busy"
                vehicle_lines.append(f"  [{v.get('id','?')}] type={v.get('type','?')} ({avail})")
        fuel = fleet.get("fuel_budget_kes", "N/A")
        return (
            "You are NavigatorOperative, the logistics operative for a Sustena sustain.\n"
            "Your role: order tracking, delivery planning, route optimisation, fleet dispatch.\n\n"
            "Active orders:\n" + ("\n".join(order_lines) or "  (none)") + "\n\n"
            "Fleet vehicles:\n" + ("\n".join(vehicle_lines) or "  (none)") + "\n"
            f"Fuel budget: KES {fuel}\n\n"
            "Delivery options: sendy, glovo, in_house.\n"
            "Always respond with valid JSON only. No markdown, no prose."
        )
