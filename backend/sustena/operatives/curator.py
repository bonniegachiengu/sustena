"""
sustena/operatives/curator.py

CuratorOperative — asset register / inventory operative.

Responsibility:
  - Asset register, inventory management, purchase value optimisation.
  - System prompt context: full asset register from state, inventory levels,
    recent purchase history.
  - When any inventory item is at or below its reorder_threshold, propose
    purchase orders or inventory reallocation.

Utility function: maximise purchase value per KES / minimise impulse spending.

Disagreement point: status quo (no purchase) — used in Nash bargaining.

should_evaluate() threshold:
  - Any inventory item is at or below its reorder_threshold.
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


class CuratorOperative(BaseOperative):
    """
    Asset register / inventory operative.

    Config keys: none currently — thresholds are item-level (reorder_threshold
    on each inventory item), not a global scalar.
    """

    operative_id = "curator"

    def __init__(
        self,
        config: dict,
        state_accessor: StateAccessor,
        claude_client: Any,
    ) -> None:
        super().__init__(config, state_accessor, claude_client)

    # ── should_evaluate — ZERO LLM CALLS ──────────────────────────────────────

    def should_evaluate(self, state: StateAccessor) -> bool:
        """
        Return True if any inventory item's quantity is at or below its
        reorder_threshold.

        O(n) scan over inventory — no LLM, no I/O.
        """
        inventory: list = state.get("inventory.items", [])
        for item in inventory:
            if not isinstance(item, dict):
                continue
            quantity  = item.get("quantity", 0)
            threshold = item.get("reorder_threshold", 0)
            if quantity <= threshold:
                logger.debug(
                    "[curator] should_evaluate=True: item '%s' qty=%s <= threshold=%s",
                    item.get("name", "?"), quantity, threshold,
                )
                return True
        return False

    # ── evaluate ──────────────────────────────────────────────────────────────

    async def evaluate(self, trigger_event: dict) -> OperativeProposal | None:
        """
        Triggered by event.procurement.delivery_confirmed or
        event.inventory.item_consumed.

        Checks stock levels against reorder thresholds. Proposes purchase orders
        or inventory reallocation.

        Returns an OperativeProposal if action is warranted, else None.
        """
        inventory: list   = self.state.get("inventory.items", [])
        asset_register    = self.state.get("assets.register", [])
        purchase_history  = self.state.get("procurement.purchase_history", [])

        below_threshold = [
            {
                "id":                item.get("id"),
                "name":              item.get("name", "?"),
                "quantity":          item.get("quantity", 0),
                "reorder_threshold": item.get("reorder_threshold", 0),
                "unit_cost_kes":     item.get("unit_cost_kes"),
                "preferred_supplier": item.get("preferred_supplier"),
                "shortfall":         item.get("reorder_threshold", 0) - item.get("quantity", 0),
            }
            for item in inventory
            if isinstance(item, dict) and item.get("quantity", 0) <= item.get("reorder_threshold", 0)
        ]

        if not below_threshold:
            return None

        system_prompt = self._build_system_prompt(asset_register, purchase_history)

        user_message = (
            f"Trigger event: {json.dumps(trigger_event)}\n\n"
            f"Items at or below reorder threshold:\n"
            f"{json.dumps(below_threshold, indent=2)}\n\n"
            "Propose either:\n"
            "  A) inventory.purchase_order — create a purchase order for restocking.\n"
            "  B) inventory.reallocate — reallocate surplus from another location.\n\n"
            "Respond ONLY with valid JSON:\n"
            '{"action": "purchase_order" | "reallocate", "item_id": "<id>", '
            '"quantity": <int>, "supplier": "<supplier_or_null>", '
            '"estimated_cost_kes": <float_or_null>, "rationale": "<brief>"}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed = json.loads(raw)
        except (json.JSONDecodeError, ValueError):
            logger.warning("[curator] LLM returned non-JSON; falling back to purchase_order")
            first = below_threshold[0]
            parsed = {
                "action":            "purchase_order",
                "item_id":           first.get("id"),
                "quantity":          max(first.get("shortfall", 1), 1),
                "supplier":          first.get("preferred_supplier"),
                "estimated_cost_kes": None,
                "rationale":         f"Item '{first.get('name', '?')}' is at or below reorder threshold.",
            }

        action    = parsed.get("action", "purchase_order")
        item_id   = parsed.get("item_id", below_threshold[0].get("id"))
        quantity  = parsed.get("quantity", 1)
        supplier  = parsed.get("supplier")
        cost_kes  = parsed.get("estimated_cost_kes")
        rationale = parsed.get("rationale", "Inventory restock required.")

        if action == "reallocate":
            return OperativeProposal(
                operator_name="inventory.reallocate",
                input_params={
                    "item_id":  item_id,
                    "quantity": quantity,
                    "reason":   f"CuratorOperative: {rationale}",
                },
                rationale=rationale,
                simulation_results={
                    "items_below_threshold": below_threshold,
                },
            )
        else:
            return OperativeProposal(
                operator_name="inventory.purchase_order",
                input_params={
                    "item_id":            item_id,
                    "quantity":           quantity,
                    "supplier":           supplier,
                    "estimated_cost_kes": cost_kes,
                    "reason":             f"CuratorOperative: {rationale}",
                },
                rationale=rationale,
                simulation_results={
                    "items_below_threshold": below_threshold,
                },
            )

    # ── deliberate ────────────────────────────────────────────────────────────

    async def deliberate(
        self,
        context: dict,
        proposal: dict,
    ) -> OperativeVote:
        """
        Vote on a Council proposal.

        Vote YES on proposals that improve asset value or stock health.
        Vote NO on proposals that represent impulse spending or poor value per KES.
        Vote ABSTAIN if the proposal is unrelated to assets/inventory.
        """
        operator_name = proposal.get("operator_name", "")
        input_params  = proposal.get("input_params", {})

        # Abstain on operators outside the inventory/asset domain
        inventory_domains = ("inventory.", "assets.", "procurement.", "sustena.stock")
        if not any(operator_name.startswith(d) for d in inventory_domains):
            return OperativeVote(
                vote=VoteChoice.ABSTAIN,
                reasoning="Proposal is outside CuratorOperative's domain (assets/inventory).",
                utility=0.5,
            )

        # Hard NO: purchase of an item not in the inventory register (impulse spending)
        item_id = input_params.get("item_id")
        if operator_name == "inventory.purchase_order" and item_id:
            inventory: list = self.state.get("inventory.items", [])
            known_ids = {i.get("id") for i in inventory if isinstance(i, dict)}
            if item_id not in known_ids:
                return OperativeVote(
                    vote=VoteChoice.NO,
                    reasoning=(
                        f"NO: item '{item_id}' is not in the inventory register. "
                        "Purchasing unregistered items represents impulse spending."
                    ),
                    utility=0.1,
                )

        # Use LLM for nuanced YES/ABSTAIN reasoning
        asset_register   = self.state.get("assets.register", [])
        purchase_history = self.state.get("procurement.purchase_history", [])
        system_prompt    = self._build_system_prompt(asset_register, purchase_history)

        user_message = (
            f"Council proposal:\n{json.dumps(proposal, indent=2)}\n\n"
            f"Context:\n{json.dumps(context, indent=2)}\n\n"
            "Vote YES if this improves asset value or restores healthy stock levels.\n"
            "Vote NO if this represents poor value per KES or impulse spending.\n"
            "Vote ABSTAIN if you cannot determine the inventory impact.\n\n"
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
            logger.warning("[curator] deliberate LLM parse error: %s", exc)
            vote      = VoteChoice.ABSTAIN
            reasoning = "Could not parse LLM deliberation response; defaulting to ABSTAIN."
            utility   = 0.5

        return OperativeVote(vote=vote, reasoning=reasoning, utility=utility)

    # ── Internal helpers ───────────────────────────────────────────────────────

    def _build_system_prompt(
        self,
        asset_register: list,
        purchase_history: list,
    ) -> str:
        """Build the CuratorOperative system prompt with asset register and purchase history."""
        inventory: list = self.state.get("inventory.items", [])

        item_lines: list[str] = []
        for item in inventory:
            if isinstance(item, dict):
                name      = item.get("name", "?")
                qty       = item.get("quantity", 0)
                threshold = item.get("reorder_threshold", 0)
                cost      = item.get("unit_cost_kes", "N/A")
                item_lines.append(
                    f"  {name}: qty={qty} threshold={threshold} unit_cost=KES {cost}"
                )

        asset_lines: list[str] = []
        for asset in asset_register:
            if isinstance(asset, dict):
                name  = asset.get("name", "?")
                value = asset.get("current_value_kes", "N/A")
                asset_lines.append(f"  {name}: value=KES {value}")

        recent_purchases = purchase_history[-5:] if purchase_history else []
        purchase_lines: list[str] = []
        for p in recent_purchases:
            if isinstance(p, dict):
                item  = p.get("item_name", "?")
                cost  = p.get("cost_kes", "?")
                date  = p.get("date", "?")
                purchase_lines.append(f"  {item}: KES {cost} on {date}")

        return (
            "You are CuratorOperative, the asset register and inventory operative "
            "for a Sustena sustain.\n"
            "Your role: manage the asset register, monitor inventory levels against "
            "reorder thresholds, and optimise purchase value per KES.\n\n"
            "Current inventory:\n" + ("\n".join(item_lines) or "  (none)") + "\n\n"
            "Asset register:\n" + ("\n".join(asset_lines) or "  (none)") + "\n\n"
            "Recent purchases:\n" + ("\n".join(purchase_lines) or "  (none)") + "\n\n"
            "Always respond with valid JSON only. No markdown, no prose."
        )
