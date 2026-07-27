"""
sustena/core/transducer.py

The transducer tau — turns a captured raw message into domain events, or
honestly declares it couldn't.

    tau: raw_text -> TransductionResult

Deliberately NOT an LLM call: deterministic, hand-written pattern matching,
same no-eval/no-magic discipline as constraints.py/predicates.py. A
transducer must be testable without a network call and must never produce a
different answer for the same input twice — that's what "durable, idempotent
intake" downstream is relying on.

Three-tier result, not a boolean:
  mapped           - understood the message AND confidently chose an
                      operator + params to run (e.g. money received ->
                      budget.record_income; unambiguous, no guessing).
  parsed_unmapped   - understood the message's SHAPE (amount, direction,
                      counterparty, an external ref if present) but
                      deliberately did NOT guess an operator — e.g. money
                      spent: which pocket? Nothing here invents a category.
                      An ingest-time categorisation config is real, separate
                      follow-up work, not silently faked here.
  unparsed          - no registered parser recognised the message's shape
                      at all.

parsed_unmapped and unparsed both surface as "needs attention" upstream —
the distinction is only in how much context the human is shown to resolve it.

Registered as an ordered list of parser functions so this generalises beyond
M-Pesa (the first instance, not the target, per the Slice 4 brief) — a future
parser (a different mobile money provider, a bank SMS, a sensor line) is just
another entry in _PARSERS, tried in order until one recognises the shape.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Callable


@dataclass(frozen=True)
class TransductionResult:
    status: str  # "mapped" | "parsed_unmapped" | "unparsed"
    operator_name: str | None = None
    operator_params: dict = field(default_factory=dict)
    external_ref: str | None = None            # e.g. the M-Pesa transaction code -- strongest dedup key when present
    parsed_fields: dict = field(default_factory=dict)   # whatever WAS understood, even if unmapped
    reason: str = ""                            # human-legible "why am I seeing this"
    parser_name: str = ""                        # which parser handled it -- audit/debug


# ── M-Pesa confirmation SMS shapes ──────────────────────────────────────────────
# Standard Safaricom M-Pesa confirmation message formats (publicly documented
# structure, not tied to any real transaction). Order matters: PAYBILL's
# pattern is a strict superset of BUY_GOODS' (both are "paid to X on..."; only
# paybill has "for account Y" in between) so paybill must be tried first, or a
# paybill message would silently match buy-goods with the account number
# swallowed into the business-name group.

_MPESA_RECEIVED_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+You have received\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+"
    r"from\s+(?P<name>[A-Za-z ]+?)\s+(?P<phone>2\d{9,11})\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
    r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)",
)

_MPESA_PAYBILL_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+paid to\s+"
    r"(?P<business>[A-Za-z0-9 ]+?)\s+for account\s+(?P<account>\S+)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
    r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)",
)

_MPESA_BUYGOODS_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+paid to\s+"
    r"(?P<business>[A-Za-z0-9 ]+?)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
    r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)",
)

_MPESA_SENT_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+sent to\s+"
    r"(?P<name>[A-Za-z ]+?)\s+(?P<phone>2\d{9,11})\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
    r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)",
)

_MPESA_WITHDRAW_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+withdrawn from\s+"
    r"(?P<agent>[A-Za-z0-9 \-]+?)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
    r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)",
)


def _to_float(amount_str: str) -> float:
    return float(amount_str.replace(",", ""))


def _needs_pocket_reason(kind: str, amount: float, counterparty: str) -> str:
    return (
        f"{kind} of KES {amount:,.0f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    )


def _parse_mpesa(text: str) -> TransductionResult | None:
    m = _MPESA_RECEIVED_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="mapped",
            operator_name="budget.record_income",
            operator_params={"amount": amount, "source": f"M-Pesa: {name}", "frequency": "once"},
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "received", "amount": amount, "counterparty": name,
                "phone": m.group("phone"), "balance_after": _to_float(m.group("balance")),
            },
            reason="Money received — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
            parser_name="mpesa_received",
        )

    m = _MPESA_PAYBILL_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        business = m.group("business").strip()
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": business,
                "account": m.group("account"), "balance_after": _to_float(m.group("balance")),
            },
            reason=_needs_pocket_reason("Paybill payment", amount, business),
            parser_name="mpesa_paybill",
        )

    m = _MPESA_BUYGOODS_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        business = m.group("business").strip()
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": business,
                "balance_after": _to_float(m.group("balance")),
            },
            reason=_needs_pocket_reason("Till payment", amount, business),
            parser_name="mpesa_buygoods",
        )

    m = _MPESA_SENT_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": name,
                "phone": m.group("phone"), "balance_after": _to_float(m.group("balance")),
            },
            reason=_needs_pocket_reason("Money sent", amount, name),
            parser_name="mpesa_sent",
        )

    m = _MPESA_WITHDRAW_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        agent = m.group("agent").strip()
        counterparty = agent if agent.lower().startswith("agent") else f"Agent {agent}"
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": counterparty,
                "balance_after": _to_float(m.group("balance")),
            },
            reason=_needs_pocket_reason("Cash withdrawal", amount, counterparty),
            parser_name="mpesa_withdraw",
        )

    return None


_PARSERS: list[Callable[[str], "TransductionResult | None"]] = [_parse_mpesa]


def parse_message(raw_text: str) -> TransductionResult:
    """
    Try every registered parser in order; the first one that recognises the
    message wins. No parser recognising it is an honest, visible "unparsed" —
    never a silent drop.
    """
    text = (raw_text or "").strip()
    if not text:
        return TransductionResult(status="unparsed", reason="Empty message.")
    for parser in _PARSERS:
        result = parser(text)
        if result is not None:
            return result
    return TransductionResult(
        status="unparsed",
        reason="No registered parser recognised this message's shape.",
    )
