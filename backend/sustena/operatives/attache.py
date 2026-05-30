"""
sustena/operatives/attache.py

AttacheOperative -- network/profiler operative.

Responsibility:
  - Contacts graph, M-Pesa counterparty enrichment, Chama Cred scoring,
    reputational intelligence.
  - System prompt context: all contacts in state.network.contacts with
    transaction history, last interaction, Chama Cred score.
  - When a recent M-Pesa transaction involves an unrecognised phone number
    (not in contacts), propose contact enrichment.

Utility function: maximise social capital / minimise isolation.

should_evaluate() threshold:
  - Any recent transaction has an unrecognised phone number (not in contacts).
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


class AttacheOperative(BaseOperative):
    """
    Network/profiler operative.

    Config keys: none currently.
    """

    operative_id = "attache"

    def __init__(self, config: dict, state_accessor: StateAccessor, claude_client: Any) -> None:
        super().__init__(config, state_accessor, claude_client)

    # -- should_evaluate -- ZERO LLM CALLS ------------------------------------

    def should_evaluate(self, state: StateAccessor) -> bool:
        """Return True if any recent transaction has a phone not in contacts."""
        contacts: list = state.get("network.contacts", [])
        known_phones: set = {
            str(c.get("phone", "")).strip()
            for c in contacts
            if isinstance(c, dict) and c.get("phone")
        }
        transactions: list = state.get("finances.mpesa_transactions", [])
        for txn in transactions:
            if not isinstance(txn, dict):
                continue
            phone = str(txn.get("counterparty_phone", "")).strip()
            if phone and phone not in known_phones:
                logger.debug("[attache] should_evaluate=True: unknown phone %s", phone)
                return True
        return False

    # -- evaluate -------------------------------------------------------------

    async def evaluate(self, trigger_event: dict) -> OperativeProposal | None:
        """Triggered by event.finances.mpesa_transaction."""
        contacts: list = self.state.get("network.contacts", [])
        known_phones: set = {
            str(c.get("phone", "")).strip()
            for c in contacts
            if isinstance(c, dict) and c.get("phone")
        }
        transactions: list = self.state.get("finances.mpesa_transactions", [])

        unknown: dict = {}
        for txn in transactions:
            if not isinstance(txn, dict):
                continue
            phone = str(txn.get("counterparty_phone", "")).strip()
            if phone and phone not in known_phones:
                unknown.setdefault(phone, []).append(txn)

        if not unknown:
            return None

        candidates = []
        for phone, txns in unknown.items():
            total_kes = sum(float(t.get("amount", 0)) for t in txns)
            candidates.append({
                "phone":     phone,
                "txn_count": len(txns),
                "total_kes": round(total_kes, 2),
                "last_txn":  txns[-1],
            })

        system_prompt = self._build_system_prompt(contacts)

        user_message = (
            f"Trigger event: {json.dumps(trigger_event)}\n\n"
            f"Unknown counterparties (not in contacts):\n"
            f"{json.dumps(candidates, indent=2)}\n\n"
            "For the most frequent or highest-value unknown counterparty, propose:\n"
            "  A) network.enrich -- add them to the contacts graph.\n\n"
            "Respond ONLY with valid JSON:\n"
            '{"phone": "<phone>", "suggested_name": "<name_or_null>", '
            '"relationship_type": "<type_or_null>", "rationale": "<brief>"}'
        )

        raw = await self._call_claude(system_prompt, user_message, model=HAIKU)

        try:
            parsed = json.loads(raw)
        except (json.JSONDecodeError, ValueError):
            logger.warning("[attache] LLM returned non-JSON; falling back")
            top = candidates[0]
            parsed = {
                "phone":             top["phone"],
                "suggested_name":    None,
                "relationship_type": "unknown",
                "rationale": (
                    f"I see KES {top['total_kes']:,.0f} to {top['phone']} "
                    f"across {top['txn_count']} transaction(s). "
                    "Should I add them to your network?"
                ),
            }

        phone     = parsed.get("phone", candidates[0]["phone"])
        rationale = parsed.get(
            "rationale",
            f"Unknown counterparty {phone} appears repeatedly in M-Pesa transactions.",
        )

        return OperativeProposal(
            operator_name="network.enrich",
            input_params={
                "phone":             phone,
                "suggested_name":    parsed.get("suggested_name"),
                "relationship_type": parsed.get("relationship_type", "unknown"),
                "reason":            f"AttacheOperative: {rationale}",
            },
            rationale=rationale,
            simulation_results={
                "unknown_counterparties": candidates,
                "total_contacts_before":  len(contacts),
            },
        )

    # -- deliberate -----------------------------------------------------------

    async def deliberate(self, context: dict, proposal: dict) -> OperativeVote:
        """Vote on a Council proposal."""
        operator_name = proposal.get("operator_name", "")
        input_params  = proposal.get("input_params", {})

        network_domains = ("network.", "chama.", "sustena.enrich")
        if not any(operator_name.startswith(d) for d in network_domains):
            return OperativeVote(
                vote=VoteChoice.ABSTAIN,
                reasoning="Proposal is outside AttacheOperative's domain (network/social capital).",
                utility=0.5,
            )

        # Hard NO: counterparty has poor Chama Cred (< 40)
        proposed_phone = str(input_params.get("phone", "")).strip()
        if proposed_phone:
            contacts: list = self.state.get("network.contacts", [])
            for contact in contacts:
                if not isinstance(contact, dict):
                    continue
                if str(contact.get("phone", "")).strip() == proposed_phone:
                    cred_score = contact.get("chama_cred_score", 100)
                    if isinstance(cred_score, (int, float)) and cred_score < 40:
                        return OperativeVote(
                            vote=VoteChoice.NO,
                            reasoning=(
                                f"NO: counterparty {proposed_phone} has Chama Cred score "
                                f"{cred_score} (below 40). Proceeding would risk social capital."
                            ),
                            utility=0.1,
                        )

        contacts_list: list = self.state.get("network.contacts", [])
        system_prompt = self._build_system_prompt(contacts_list)

        user_message = (
            f"Council proposal:\n{json.dumps(proposal, indent=2)}\n\n"
            f"Context:\n{json.dumps(context, indent=2)}\n\n"
            "Vote YES if this improves social capital.\n"
            "Vote NO if this risks social capital or reputational harm.\n"
            "Vote ABSTAIN if you cannot determine the network impact.\n\n"
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
            logger.warning("[attache] deliberate LLM parse error: %s", exc)
            vote      = VoteChoice.ABSTAIN
            reasoning = "Could not parse LLM deliberation response; defaulting to ABSTAIN."
            utility   = 0.5

        return OperativeVote(vote=vote, reasoning=reasoning, utility=utility)

    # -- contact_profile ------------------------------------------------------

    def contact_profile(self, phone_number: str) -> dict:
        """Returns a contact card for a given phone number."""
        phone_number = str(phone_number).strip()
        contacts: list = self.state.get("network.contacts", [])
        contact = next(
            (c for c in contacts if isinstance(c, dict)
             and str(c.get("phone", "")).strip() == phone_number),
            None,
        )
        transactions: list = self.state.get("finances.mpesa_transactions", [])
        related_txns = [
            t for t in transactions
            if isinstance(t, dict)
            and str(t.get("counterparty_phone", "")).strip() == phone_number
        ]
        total_kes = sum(float(t.get("amount", 0)) for t in related_txns)
        return {
            "widget":  "contact_profile",
            "phone":   phone_number,
            "name":    contact.get("name") if contact else None,
            "transaction_history_summary": {
                "txn_count": len(related_txns),
                "total_kes": round(total_kes, 2),
            },
            "relationship_type": contact.get("relationship_type") if contact else None,
            "chama_cred_score":  contact.get("chama_cred_score") if contact else None,
            "last_interaction":  contact.get("last_interaction") if contact else None,
            "known":             contact is not None,
        }

    # -- Internal helpers -----------------------------------------------------

    def _build_system_prompt(self, contacts: list) -> str:
        contact_lines: list = []
        for c in contacts:
            if isinstance(c, dict):
                name  = c.get("name", "unknown")
                phone = c.get("phone", "?")
                cred  = c.get("chama_cred_score", "N/A")
                rel   = c.get("relationship_type", "?")
                contact_lines.append(f"  {name} ({phone}) -- rel={rel} cred={cred}")
        contact_block = "\n".join(contact_lines) if contact_lines else "  (no contacts)"
        return (
            "You are AttacheOperative, the network and profiler operative for a Sustena sustain.\n"
            "Your role: manage the contacts graph, enrich M-Pesa counterparties, "
            "and track Chama Cred scores for reputational intelligence.\n\n"
            f"Known contacts:\n{contact_block}\n\n"
            "Chama Cred score: 0-100. Scores < 40 indicate reputational risk.\n\n"
            "Always respond with valid JSON only. No markdown, no prose."
        )
