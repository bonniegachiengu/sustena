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

Four-tier result (three shape-classification tiers, plus a security gate
checked before any of them):
  rejected          - the text carries an OTP/verification code or similar
                       one-time secret. Checked FIRST, before any parser —
                       see contains_sensitive_secret()'s own docstring.
                       Never stored, never parsed further, regardless of
                       which sender it claims to be from.
  mapped            - understood the message AND confidently chose an
                       operator + params to run (e.g. money received ->
                       budget.record_income; unambiguous, no guessing).
  parsed_unmapped   - understood the message's SHAPE (amount, direction,
                      counterparty, an external ref if present) but
                      deliberately did NOT guess an operator — e.g. money
                      spent: which pocket? Nothing here invents a category.
                      An ingest-time categorisation config is real, separate
                      follow-up work, not silently faked here. Also used for
                      genuinely informational shapes with no money movement
                      at all (a balance inquiry, a loan-status notice) —
                      there is nothing to map AND nothing that needs a
                      pocket decision either, but it's still real,
                      recognised information worth surfacing, not silently
                      dropped as unparsed.
  unparsed          - no registered parser recognised the message's shape
                      at all.

parsed_unmapped and unparsed both surface as "needs attention" upstream —
the distinction is only in how much context the human is shown to resolve it.
rejected is never stored at all (see ingest_engine.capture()'s own guard,
which checks contains_sensitive_secret() before ever writing the raw text
to the database — this module's own rejection here is necessary but not
sufficient, since ingest_engine must not persist the OTP text just to learn
that it should have refused it).

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
    status: str  # "rejected" | "mapped" | "parsed_unmapped" | "unparsed"
    operator_name: str | None = None
    operator_params: dict = field(default_factory=dict)
    external_ref: str | None = None            # e.g. the M-Pesa transaction code -- strongest dedup key when present
    parsed_fields: dict = field(default_factory=dict)   # whatever WAS understood, even if unmapped
    reason: str = ""                            # human-legible "why am I seeing this"
    parser_name: str = ""                        # which parser handled it -- audit/debug


# ── Sensitive-secret guard — checked BEFORE any parser, always ─────────────
# CRITICAL, security-relevant: an OTP/verification-code SMS must NEVER be
# parsed, stored, or forwarded anywhere, regardless of which sender it
# claims to be from (a real OTP for a KCB or M-Pesa action arrives from the
# exact same sender id as a real transaction confirmation, so this cannot
# be a sender-based check — it has to be content-based). This is checked in
# TWO independent places, deliberately: the Android capture client
# (SmsSecretFilter.java — primary defense, these should never even leave
# the device) and here, server-side, as defense in depth in case one
# reaches the API anyway. The keyword set is deliberately narrow and
# high-confidence (OTP / "do not share" / verification / one-time-pin
# phrasing) — vocabulary that essentially never appears in a legitimate
# transaction confirmation (which says "Confirmed"/"credited"/"debited"/
# "received"/"balance"), so this should not misfire on real transaction
# text; false negatives (an OTP phrased in a way this doesn't catch) are a
# real residual risk of any keyword-based approach and are why the Android-
# side filter is the PRIMARY defense, not this one.
_SENSITIVE_SECRET_PATTERNS = [
    re.compile(r"\bOTP\b", re.IGNORECASE),
    re.compile(r"do\s+not\s+share", re.IGNORECASE),
    re.compile(r"verification\s+code", re.IGNORECASE),
    re.compile(r"one[\s-]?time\s+(?:pin|password|code)", re.IGNORECASE),
    re.compile(r"security\s+code", re.IGNORECASE),
]


def contains_sensitive_secret(text: str) -> bool:
    """True if the raw text looks like it carries an OTP/verification code
    or similar one-time secret. See this module's own header comment for
    why this exists and why it's checked in two independent places."""
    if not text:
        return False
    return any(p.search(text) for p in _SENSITIVE_SECRET_PATTERNS)


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

# ── M-Pesa <-> KCB bridge shapes — REAL, Bonnie-provided samples (1 Aug 2026) ──
# Two confirmed-real M-Pesa notification templates for money moving FROM
# M-Pesa INTO a KCB destination (a paybill, or a named KCB account) — kept
# under the mpesa parser per Bonnie's own instruction ("keep under the
# mpesa source"), since these are M-Pesa's own "Ksh X sent to Y ... M-PESA
# ref Z" notifications, just for a KCB-addressed recipient rather than a
# phone number.

_MPESA_TO_KCB_PAYBILL_RE = re.compile(
    r"Ksh\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+KCB Pay Bill\s+(?P<paybill>\d+)\s+for account\s+(?P<account>\S+)\s+"
    r"(?P<name>[A-Za-z ]+?)\s+has been received on\s+(?P<date>\S+)\s+at\s+(?P<time>\S+\s*[APap][Mm])"
    r".*?M-PESA ref\s+(?P<ref>[A-Za-z0-9]+)",
    re.IGNORECASE | re.DOTALL,
)

_MPESA_TO_KCB_ACCOUNT_RE = re.compile(
    r"Ksh\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+KCB account\s+(?P<name>[A-Za-z ]+?)\s+(?P<account>\d+)\s+"
    r"has been received on\s+(?P<date>\S+)"
    r".*?M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)",
    re.IGNORECASE | re.DOTALL,
)


def _to_float(amount_str: str) -> float:
    return float(amount_str.replace(",", ""))


def _needs_pocket_reason(kind: str, amount: float, counterparty: str, currency: str = "KES") -> str:
    return (
        f"{kind} of {currency} {amount:,.2f} to {counterparty} recognised, but which pocket to "
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

    m = _MPESA_TO_KCB_PAYBILL_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        counterparty = f"KCB Pay Bill {m.group('paybill')} - {name}"
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": counterparty,
                "account": m.group("account"), "paybill": m.group("paybill"),
            },
            reason=_needs_pocket_reason("M-Pesa to KCB paybill", amount, counterparty),
            parser_name="mpesa_to_kcb_paybill",
        )

    m = _MPESA_TO_KCB_ACCOUNT_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": name,
                "account": m.group("account"),
            },
            reason=_needs_pocket_reason("M-Pesa to KCB account", amount, name),
            parser_name="mpesa_to_kcb_account",
        )

    return None


# ── KCB Kenya SMS alert shapes — REAL, Bonnie-provided samples (1 Aug 2026) ──
# Replaces an earlier, wholly speculative KCB pattern set (structural guesses
# never checked against a real message) with patterns tuned against real
# text Bonnie pasted. None of the earlier guessed patterns matched any of
# these real formats — confirmed by checking each one directly before
# writing these — which is exactly why that earlier set was disclosed as
# unverified rather than shipped with false confidence, and exactly why it
# is now replaced outright rather than left alongside as dead, misleading
# code that would never match anything a real KCB message actually says.
#
# Every shape below is confirmed against Bonnie's real (personal names
# replaced with placeholder Kenyan names matching this file's existing
# convention -- JOHN KAMAU / MARY WANJIRU etc. -- the amounts, dates,
# references, masked account/card numbers, and all wording are otherwise
# exactly as received) pasted samples. Three further shapes (the
# "AMBIGUOUS" block, kcb_bridge_*) are Bonnie's own best guess at their
# origin (Vooma/KCB-app notifications relaying M-Pesa-network activity,
# distinct from the two confirmed M-Pesa<->KCB bridge shapes above) —
# flagged in their own parser_name and this module's docstring as a
# judgment call, not a verified classification.

_KCB_RECEIVE_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{8,})\s+Confirmed!\s+You have received\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+"
    r"from\s+(?P<name>[A-Za-z ]+?)\s*-\s*(?P<account>[\d*]+)\s+at\s+(?P<datetime>[\d\-: ]+?[APap][Mm])\s+via\s+KCB",
    re.IGNORECASE,
)

_KCB_CARD_RE = re.compile(
    r"(?P<currency>KES|USD)\s*(?P<amount>[\d,]+\.?\d*)\s+transaction made on\s+KCB card\s+(?P<card>[\dX*]+)\s+at\s+"
    r"(?P<merchant>.+?)\s+on\s+(?P<date>\S+)\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\s*,\s*Avail(?:able)?\s+balance\s+KES\s*(?P<balance>[\d,]+\.?\d*)",
    re.IGNORECASE | re.DOTALL,
)

_KCB_LOAN_DISBURSED_RE = re.compile(
    r"KCB Mobile Loan of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+has been approved and credited"
    r".*?facility fee of\s+KES\s*(?P<fee>[\d,]+\.?\d*)\s+has been charged"
    r".*?loan balance is\s+KES\s*(?P<loan_balance>[\d,]+\.?\d*)",
    re.IGNORECASE | re.DOTALL,
)

_KCB_LOAN_REPAY_RE = re.compile(
    r"KES\s*(?P<amount>[\d,]+\.?\d*)\s+was debited from your KCB account to repay your KCB Mobile Loan\s+(?P<loan_number>\d+)"
    r".*?loan balance is\s+KES\s*(?P<loan_balance>[\d,]+\.?\d*)",
    re.IGNORECASE | re.DOTALL,
)

_KCB_VOOMA_LOAN_REPAY_RE = re.compile(
    r"KES\s*(?P<amount>[\d,]+\.?\d*)\s+was debited from your Vooma wallet to repay your Vooma Loan\s+(?P<loan_number>\d+)"
    r".*?loan balance (?:is|of)\s+KES\s*(?P<loan_balance>[\d,]+\.?\d*)",
    re.IGNORECASE | re.DOTALL,
)

# Three "look like Vooma/KCB-app notifications of M-Pesa activity" shapes --
# genuinely ambiguous origin (Bonnie's own framing), grouped under kcb here
# as a disclosed best judgment call. All three explicitly reference
# "M-PESA" in their own text, unlike every other KCB pattern above.
_KCB_BRIDGE_RECEIVED_RE = re.compile(
    r"You have received\s+KES\s*(?P<amount>[\d,.]+)\s+from\s+(?P<name>[A-Za-z ]+?)\.\s*M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)",
    re.IGNORECASE,
)

_KCB_BRIDGE_SENT_TO_MPESA_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{8,})\s+completed\.\s*KES\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+M-PESA\s+(?P<phone>2\d{9,11})\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s*"
    r"Transaction cost\s+KES\s*(?P<cost>[\d,]+\.?\d*)",
    re.IGNORECASE,
)

_KCB_BRIDGE_TRANSFERRED_RE = re.compile(
    r"you have successfully transferred\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+to\s+(?P<phone>[\d*]+)-(?P<name>[A-Za-z ]+?)\s+on\s+(?P<date>\S+)"
    r".*?M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)",
    re.IGNORECASE | re.DOTALL,
)

# Informational only -- no money movement, nothing to map, nothing that
# needs a pocket decision. Real balances/arrears figures change (usually
# grow, via accruing interest) on every occurrence, so the exact message
# text differs each time -- the existing dedup_key (a hash of the FULL raw
# text) naturally treats each day's slightly-different figure as a genuinely
# new, worth-resurfacing capture, with no special-casing needed here for
# that: idempotency is about not re-processing the IDENTICAL text twice,
# which still holds, and a changed balance is honestly a different message.

_KCB_BALANCE_RE = re.compile(
    r"Actual balance\s+KES\s*(?P<actual_balance>[\d,]+\.?\d*)\.\s*Available balance\s+KES\s*(?P<available_balance>[\d,]+\.?\d*)"
    r"(?:.*?[Tt]ransaction reference number\s+(?P<ref>[A-Za-z0-9]+))?",
    re.IGNORECASE | re.DOTALL,
)

_KCB_LOAN_OVERDUE_RE = re.compile(
    r"KCB Mobile Loan repayment of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+is overdue since\s+(?P<date>\S+)",
    re.IGNORECASE,
)

_KCB_LOAN_ARREARS_RE = re.compile(
    r"KCB Mobile Loan is in arrears of\s+KES\s*(?P<amount>[\d,]+\.?\d*)",
    re.IGNORECASE,
)

_KCB_LOAN_DEFAULT_RE = re.compile(
    r"loan number\s+(?P<loan_number>\d+)\s+has a balance of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+which is in DEFAULT",
    re.IGNORECASE,
)

_KCB_LOAN_DUE_TODAY_RE = re.compile(
    r"KCB Mobile Loan balance of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+is due today",
    re.IGNORECASE,
)


def _parse_kcb(text: str) -> TransductionResult | None:
    m = _KCB_RECEIVE_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="mapped",
            operator_name="budget.record_income",
            operator_params={"amount": amount, "source": f"KCB: {name}", "frequency": "once"},
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "received", "amount": amount, "counterparty": name,
                "account": m.group("account"), "datetime": m.group("datetime"),
            },
            reason="Money received via KCB — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
            parser_name="kcb_receive",
        )

    m = _KCB_CARD_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        currency = m.group("currency").upper()
        merchant = m.group("merchant").strip()
        return TransductionResult(
            status="parsed_unmapped",
            parsed_fields={
                "direction": "sent", "amount": amount, "currency": currency, "counterparty": merchant,
                "card": m.group("card"), "balance_after": _to_float(m.group("balance")),
            },
            reason=_needs_pocket_reason("KCB card transaction", amount, merchant, currency=currency),
            parser_name="kcb_card",
        )

    m = _KCB_LOAN_DISBURSED_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        return TransductionResult(
            status="mapped",
            operator_name="budget.record_income",
            operator_params={"amount": amount, "source": "KCB Mobile Loan", "frequency": "once"},
            parsed_fields={
                "direction": "received", "amount": amount, "counterparty": "KCB Mobile Loan",
                "fee": _to_float(m.group("fee")), "loan_balance": _to_float(m.group("loan_balance")),
            },
            reason=(
                "KCB Mobile Loan disbursed — mapped to budget.record_income (the loan amount "
                "unambiguously credits liquid balance); a facility fee was also charged, recorded "
                "for context but not separately deducted here."
            ),
            parser_name="kcb_loan_disbursed",
        )

    m = _KCB_LOAN_REPAY_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        counterparty = f"KCB Mobile Loan {m.group('loan_number')} repayment"
        return TransductionResult(
            status="parsed_unmapped",
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": counterparty,
                "loan_number": m.group("loan_number"), "loan_balance": _to_float(m.group("loan_balance")),
            },
            reason=_needs_pocket_reason("KCB Mobile Loan repayment", amount, counterparty),
            parser_name="kcb_loan_repay",
        )

    m = _KCB_VOOMA_LOAN_REPAY_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        counterparty = f"Vooma Loan {m.group('loan_number')} repayment"
        return TransductionResult(
            status="parsed_unmapped",
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": counterparty,
                "sub_source": "vooma", "loan_number": m.group("loan_number"),
                "loan_balance": _to_float(m.group("loan_balance")),
            },
            reason=_needs_pocket_reason("Vooma Loan repayment", amount, counterparty),
            parser_name="kcb_vooma_loan_repay",
        )

    m = _KCB_BRIDGE_RECEIVED_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="mapped",
            operator_name="budget.record_income",
            operator_params={"amount": amount, "source": f"M-Pesa via KCB/Vooma: {name}", "frequency": "once"},
            external_ref=m.group("ref"),
            parsed_fields={"direction": "received", "amount": amount, "counterparty": name},
            reason=(
                "Money received — mapped to budget.record_income. Origin app (KCB/Vooma vs "
                "Safaricom M-Pesa directly) is a best-guess classification, not confirmed."
            ),
            parser_name="kcb_bridge_received",
        )

    m = _KCB_BRIDGE_SENT_TO_MPESA_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        counterparty = f"M-PESA {m.group('phone')}"
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": counterparty,
                "phone": m.group("phone"), "transaction_cost": _to_float(m.group("cost")),
            },
            reason=_needs_pocket_reason("Sent to M-PESA via KCB/Vooma", amount, counterparty),
            parser_name="kcb_bridge_sent_to_mpesa",
        )

    m = _KCB_BRIDGE_TRANSFERRED_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "sent", "amount": amount, "counterparty": name,
                "phone": m.group("phone"),
            },
            reason=_needs_pocket_reason("Transferred via KCB/Vooma", amount, name),
            parser_name="kcb_bridge_transferred",
        )

    m = _KCB_BALANCE_RE.search(text)
    if m:
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "none",
                "actual_balance": _to_float(m.group("actual_balance")),
                "balance_after": _to_float(m.group("available_balance")),
            },
            reason=(
                "KCB balance update — informational only, no pocket decision needed "
                f"(available balance: KES {_to_float(m.group('available_balance')):,.2f})."
            ),
            parser_name="kcb_balance",
        )

    m = _KCB_LOAN_OVERDUE_RE.search(text)
    if m:
        return TransductionResult(
            status="parsed_unmapped",
            parsed_fields={"direction": "none", "loan_status": "overdue", "amount_owed": _to_float(m.group("amount"))},
            reason=(
                f"KCB Mobile Loan repayment overdue since {m.group('date')} — informational only, "
                "no transaction created."
            ),
            parser_name="kcb_loan_overdue",
        )

    m = _KCB_LOAN_ARREARS_RE.search(text)
    if m:
        return TransductionResult(
            status="parsed_unmapped",
            parsed_fields={"direction": "none", "loan_status": "arrears", "amount_owed": _to_float(m.group("amount"))},
            reason="KCB Mobile Loan in arrears — informational only, no transaction created.",
            parser_name="kcb_loan_arrears",
        )

    m = _KCB_LOAN_DEFAULT_RE.search(text)
    if m:
        return TransductionResult(
            status="parsed_unmapped",
            parsed_fields={
                "direction": "none", "loan_status": "default", "loan_number": m.group("loan_number"),
                "amount_owed": _to_float(m.group("amount")),
            },
            reason="KCB Mobile Loan in default — informational only, no transaction created.",
            parser_name="kcb_loan_default",
        )

    m = _KCB_LOAN_DUE_TODAY_RE.search(text)
    if m:
        return TransductionResult(
            status="parsed_unmapped",
            parsed_fields={"direction": "none", "loan_status": "due_today", "amount_owed": _to_float(m.group("amount"))},
            reason="KCB Mobile Loan due today — informational only, no transaction created.",
            parser_name="kcb_loan_due_today",
        )

    return None


_PARSERS: list[Callable[[str], "TransductionResult | None"]] = [_parse_mpesa, _parse_kcb]


def parse_message(raw_text: str) -> TransductionResult:
    """
    Try every registered parser in order; the first one that recognises the
    message wins. No parser recognising it is an honest, visible "unparsed" —
    never a silent drop. The sensitive-secret check runs first, unconditionally,
    before any parser gets a look at the text — see contains_sensitive_secret().
    """
    text = (raw_text or "").strip()
    if not text:
        return TransductionResult(status="unparsed", reason="Empty message.")
    if contains_sensitive_secret(text):
        return TransductionResult(
            status="rejected",
            reason="Message contains an OTP/verification code or similar secret — refused, never parsed or stored.",
        )
    for parser in _PARSERS:
        result = parser(text)
        if result is not None:
            return result
    return TransductionResult(
        status="unparsed",
        reason="No registered parser recognised this message's shape.",
    )
