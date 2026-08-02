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

Five-tier result (four shape-classification tiers, plus a security gate
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
                      follow-up work, not silently faked here. ALWAYS a real
                      transaction that needs a human pocket/operator
                      decision — see "informational" below for recognised
                      shapes that are NOT a transaction at all.
  informational     - recognised, but genuinely NOT a transaction: no money
                      moved, nothing to map, and no pocket/operator decision
                      for a human to make either (a balance inquiry, a
                      loan-status accrual notice, or an M-Pesa/KCB system
                      "unable to process your request" / "try again later"
                      notice). Still recorded (never silently dropped —
                      matches every other tier's "disclose, don't drop"
                      discipline) but does NOT surface in needs_attention,
                      because nothing about it needs a human at all. Fixed
                      2 Aug 2026: this used to share the parsed_unmapped
                      tier, which meant every one of these ALSO surfaced as
                      a classify card demanding a decision it never actually
                      needed — a real, reported bug, not a design choice.
  unparsed          - no registered parser recognised the message's shape
                      at all.

parsed_unmapped and unparsed both surface as "needs attention" upstream —
the distinction is only in how much context the human is shown to resolve
it. informational does NOT surface there (see above). rejected is never
stored at all (see ingest_engine.capture()'s own guard, which checks
contains_sensitive_secret() before ever writing the raw text to the
database — this module's own rejection here is necessary but not
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
    status: str  # "rejected" | "mapped" | "parsed_unmapped" | "informational" | "unparsed"
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
    # Expanded 2 Aug 2026 against real KCB thread content Bonnie's screen-
    # shots surfaced -- his real KCB thread carries several genuine secret
    # shapes the original 5 patterns above missed entirely: TAN codes, a
    # generic "activation code"/"this code is valid" framing, and a card's
    # "secret PIN". None of these overlap with real transaction-confirmation
    # vocabulary (Confirmed/credited/debited/received/balance/transaction
    # made) -- verified against every real fixture already in this codebase
    # before shipping, same false-positive discipline the original 5 used.
    re.compile(r"tan\s+code", re.IGNORECASE),
    re.compile(r"activation\s+code", re.IGNORECASE),
    re.compile(r"secret\s+pin", re.IGNORECASE),
    re.compile(r"\bpin\s+is\b", re.IGNORECASE),
    re.compile(r"code\s+is\s+valid", re.IGNORECASE),
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

# Two more real, genuine Safaricom M-Pesa shapes -- REAL text Bonnie pasted
# (2 Aug 2026) -- that came through UNPARSED against the original 5 above.
# Textually distinct from every existing mpesa_* pattern (checked before
# adding, not assumed): PAYBILL_SENT uses "sent to X for account Y" (the
# existing PAYBILL pattern only recognises "paid to X for account Y" --
# Safaricom genuinely uses both verbs for what is structurally the same
# paybill-payment shape); AIRTIME is an entirely different sentence shape
# ("You bought Ksh X of airtime") with no counterparty at all. AIRTIME's
# own real sample also has lowercase "confirmed" (every other real M-Pesa
# sample in this file capitalises it) -- re.IGNORECASE added specifically
# because of that observed real variation, not applied speculatively.
_MPESA_PAYBILL_SENT_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+sent to\s+"
    r"(?P<business>[A-Za-z0-9 ]+?)\s+for account\s+(?P<account>\S+)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
    r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
    r"(?:\.\s*Transaction cost,?\s*Ksh(?P<cost>[\d,]+\.?\d*))?",
    re.IGNORECASE | re.DOTALL,
)

_MPESA_AIRTIME_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{10})\s+confirmed\.\s+You bought\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+of airtime\s+on\s+"
    r"(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
    r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
    r"(?:\.\s*Transaction cost,?\s*Ksh(?P<cost>[\d,]+\.?\d*))?",
    re.IGNORECASE | re.DOTALL,
)

# NOTE: two "Ksh X sent to KCB Pay Bill/account ... M-PESA ref Z" shapes used
# to live here, kept under the mpesa parser on the assumption they were genuine
# Safaricom M-Pesa notifications. Bonnie confirmed (1 Aug 2026) that on his
# real device EVERY sample of this shape actually arrives from the KCB sender
# id, not MPESA — source is decided by SENDER, never by wording. Moved to
# _parse_kcb() below as _KCB_MPESA_PAYBILL_RE / _KCB_MPESA_ACCOUNT_RE.


def _to_float(amount_str: str) -> float:
    return float(amount_str.replace(",", ""))


def _needs_pocket_reason(kind: str, amount: float, counterparty: str, currency: str = "KES") -> str:
    return (
        f"{kind} of {currency} {amount:,.2f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    )


# ── Informational system/error notices — recognised, but not a transaction ──
# M-Pesa/KCB occasionally send a plain system or error notice from the SAME
# sender id as a real transaction confirmation: "unable to process your
# request", a busy/timeout message, "try again later". These carry no
# amount and describe nothing that happened to your money -- surfacing one
# as a classify card ("which pocket does this belong to?") is actively
# wrong, not just unhelpful, since there is no transaction to classify.
#
# Checked as the LAST fallback in both _parse_mpesa/_parse_kcb, only AFTER
# every real transaction-shape regex above has already failed to match --
# this can never shadow or swallow a genuine transaction, since a real
# transaction is always tried first and wins outright.
#
# Real, disclosed judgment call: built from ONE confirmed real sample
# (Bonnie, 2 Aug 2026 -- "M-PESA is unable to process your request because a
# similar transaction is currently underway. Please wait while we complete
# your initial request.") plus common Safaricom/bank system-message
# phrasing, NOT verified against a wide corpus the way the transaction
# parsers above are. Deliberately narrow and focused on error/busy/retry
# wording specifically, NOT general promotional content -- a promo blast
# can share vocabulary with a real transaction's own trailer text (e.g.
# "Download My OneApp" appears on the real, mapped airtime-purchase
# message too), so promotional detection is NOT attempted here to avoid
# false-positiving on a genuine transaction.
_INFORMATIONAL_SYSTEM_PATTERNS = [
    re.compile(r"unable to process your request", re.IGNORECASE),
    re.compile(r"similar transaction (?:is|was) currently underway", re.IGNORECASE),
    re.compile(r"please wait while we complete", re.IGNORECASE),
    re.compile(r"(?:system|service) is currently (?:busy|unavailable)", re.IGNORECASE),
    re.compile(r"service (?:is )?temporarily unavailable", re.IGNORECASE),
    re.compile(r"please try again (?:later|after)", re.IGNORECASE),
    re.compile(r"request (?:has )?timed? out", re.IGNORECASE),

    # Round 2 (2 Aug 2026) -- FAILURE/ERROR/REJECTION notices. The real bug
    # that surfaced these: "Failed. The till number entered is incorrect.
    # Kindly enter the correct Till Number and try again." (a genuine
    # M-Pesa failure -- zero money moved) fell all the way through to
    # "unparsed" and surfaced as a classify card asking "which pocket does
    # this belong to?" for a message with no transaction to classify.
    #
    # Each pattern below pairs a failure/negation VERB with an explicit
    # transaction-related NOUN ("transaction", "payment", "PIN", a named
    # field like "till number") rather than matching the bare verb alone --
    # a completed "Confirmed..."/"...has been received..." message never
    # pairs those nouns with a negation, so none of these can shadow a real
    # transaction, on top of only ever running as the LAST fallback after
    # every transaction-shape regex has already failed to match.
    #
    # Sourced from a 3-way independent research pass (Safaricom-specific,
    # KCB-specific, generic cross-bank wording) cross-checked against each
    # other for convergence, not a single unverified guess -- but still
    # genuinely unconfirmed against a real corpus beyond the one sample
    # above, same disclosed-confidence discipline as round 1.
    #
    # Deliberately did NOT add "reversed"/"reversal" wording -- all three
    # research passes independently flagged it as unsafe: a genuine
    # reversal credits money BACK into the account, a real state change,
    # not a non-event. Guessing at reversal wording risks silently
    # swallowing a real credit as if nothing happened. A real reversal SMS
    # needs its own dedicated mapped/parsed_unmapped parser (a future,
    # separate addition against a real sample), not this fallback.
    re.compile(r"\bfailed\.\s", re.IGNORECASE),
    re.compile(r"\btransaction\s+(?:has\s+)?fail(?:ed)?\b", re.IGNORECASE),
    re.compile(r"(?:till|paybill|account|business|phone)\s+number\s+(?:entered\s+)?is\s+incorrect", re.IGNORECASE),
    re.compile(r"\bincorrect\s+(?:pin|till\s+number|paybill\s+number|account\s+number|password)\b", re.IGNORECASE),
    re.compile(r"\bwrong\s+pin\b", re.IGNORECASE),
    re.compile(r"\bpin\s+(?:you\s+)?entered\s+is\s+incorrect\b", re.IGNORECASE),
    re.compile(r"\binvalid\s+(?:pin|till|paybill|account)\s*(?:number)?\b", re.IGNORECASE),
    re.compile(r"\b(?:is|was)\s+not\s+(?:a\s+)?registered\b", re.IGNORECASE),
    re.compile(r"\b(?:was|is)\s+not\s+successful\b", re.IGNORECASE),
    re.compile(r"\b(?:payment|transaction|request)\s+(?:was\s+|has\s+been\s+)?declined\b", re.IGNORECASE),
    re.compile(r"\btransaction\s+(?:has\s+been\s+|was\s+)?(?:cancelled|canceled)\b", re.IGNORECASE),
    re.compile(r"\b(?:could\s+not|was\s+not|has\s+not\s+been)\s+(?:be\s+)?completed\b", re.IGNORECASE),
    re.compile(r"\byou\s+do\s+not\s+have\s+enough\s+money\b", re.IGNORECASE),
    re.compile(r"\binsufficient\s+(?:funds|balance)\b", re.IGNORECASE),
    re.compile(r"\bexceeds\s+(?:your\s+)?(?:daily\s+)?(?:transaction\s+)?limit\b", re.IGNORECASE),
]

# Adversarial verify (2 Aug 2026) live-confirmed two real messages where a
# Round 2 pattern above would silently swallow a genuine money-movement
# event instead of the safe "unparsed" fallback:
#   1. A Fuliza overdraft top-up notice ("...You had insufficient funds;
#      Fuliza M-PESA has topped up your transaction...") -- a real credit
#      extension, caught by the bare "insufficient funds" pattern.
#   2. A hybrid failure+reversal message ("Your transaction... failed. The
#      amount has been reversed to your M-PESA account...") -- a real
#      credit (the reversal), caught by the "failed." pattern even though
#      "reversed" wording was deliberately never added as a positive
#      trigger for exactly this reason.
# This is a hard override, not another pattern to balance against the rest:
# if any of these appear ANYWHERE in the text, the message can never be
# classified informational, regardless of which pattern above matched --
# it falls through to "unparsed" (surfaced, not silently dropped) instead.
_MONEY_STILL_MOVED_OVERRIDE = re.compile(
    r"\b(?:reversed|reversal|refunded|refund|topped\s+up|top-up|fuliza)\b", re.IGNORECASE
)


def _is_informational_system_message(text: str) -> bool:
    if _MONEY_STILL_MOVED_OVERRIDE.search(text):
        return False
    return any(p.search(text) for p in _INFORMATIONAL_SYSTEM_PATTERNS)


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

    m = _MPESA_PAYBILL_SENT_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        business = m.group("business").strip()
        fields = {
            "direction": "sent", "amount": amount, "counterparty": business,
            "account": m.group("account"), "balance_after": _to_float(m.group("balance")),
        }
        if m.group("cost") is not None:
            fields["transaction_cost"] = _to_float(m.group("cost"))
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields=fields,
            reason=_needs_pocket_reason("Paybill payment", amount, business),
            parser_name="mpesa_paybill_sent",
        )

    m = _MPESA_AIRTIME_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        counterparty = "Safaricom airtime"
        fields = {
            "direction": "sent", "amount": amount, "counterparty": counterparty,
            "balance_after": _to_float(m.group("balance")),
        }
        if m.group("cost") is not None:
            fields["transaction_cost"] = _to_float(m.group("cost"))
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=m.group("ref"),
            parsed_fields=fields,
            reason=_needs_pocket_reason("Airtime purchase", amount, counterparty),
            parser_name="mpesa_airtime",
        )

    if _is_informational_system_message(text):
        return TransductionResult(
            status="informational",
            parsed_fields={"direction": "none"},
            reason="An M-Pesa system/error notice, not a transaction — nothing to record.",
            parser_name="mpesa_system_notice",
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
# exactly as received) pasted samples.
#
# Five of the shapes below (_KCB_MPESA_*) explicitly mention "M-PESA" in
# their own wording -- money moving between a KCB account/card and the
# M-Pesa network, or notifications shaped like Safaricom's own confirmation
# template. These were ORIGINALLY split across two buckets: two were kept
# under the mpesa parser (assumed to be genuine Safaricom notifications just
# addressed to a KCB destination) and three were placed here as a disclosed
# "best guess" (Bonnie's own framing at the time: possibly Vooma/KCB-app
# relays of M-Pesa activity, origin sender unconfirmed). Bonnie has since
# checked his real device directly (1 Aug 2026): ALL FIVE of these shapes
# arrive from the KCB sender id, not MPESA. Classification is by SENDER,
# never by wording -- a message that talks about M-PESA can still be a KCB
# notification if that's who actually sent it. All five now live here,
# confirmed (not guessed), sharing the _KCB_MPESA_* naming convention below.

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

# Two shapes formatted exactly like a Safaricom M-Pesa "sent to" confirmation,
# but for a KCB-addressed recipient (a paybill or a named KCB account) --
# CONFIRMED to arrive from the KCB sender id (see this section's header note).
#
# Real bug found and fixed against Bonnie's actual paybill sample (2 Aug
# 2026): despite the "sent to KCB Pay Bill/account..." wording, this is
# money being CREDITED INTO the KCB account the SMS is about (paybill 522522
# is KCB's own deposit paybill; "for account ... NAME has been received"
# names the RECIPIENT, i.e. the account holder this message was sent to) --
# the party who *sent* the money is a third party, not the message's
# recipient. The direction is "received", not "sent" -- these two handlers
# originally got this backwards (parsed_unmapped, direction="sent"),
# unnoticed until tested against a real message. Same certainty class as
# _KCB_MPESA_RECEIVED_RE below (an unambiguous credit to liquid balance),
# so both are mapped to budget.record_income too, not left needing a
# pocket decision that was never actually ambiguous.
_KCB_MPESA_PAYBILL_RE = re.compile(
    r"Ksh\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+KCB Pay Bill\s+(?P<paybill>\d+)\s+for account\s+(?P<account>\S+)\s+"
    r"(?P<name>[A-Za-z ]+?)\s+has been received on\s+(?P<date>\S+)\s+at\s+(?P<time>\S+\s*[APap][Mm])"
    r".*?M-PESA ref\s+(?P<ref>[A-Za-z0-9]+)",
    re.IGNORECASE | re.DOTALL,
)

_KCB_MPESA_ACCOUNT_RE = re.compile(
    r"Ksh\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+KCB account\s+(?P<name>[A-Za-z ]+?)\s+(?P<account>\d+)\s+"
    r"has been received on\s+(?P<date>\S+)"
    r".*?M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)",
    re.IGNORECASE | re.DOTALL,
)

# Three shapes describing money moving between a KCB/Vooma wallet and the
# M-Pesa network directly -- CONFIRMED to arrive from the KCB sender id (see
# this section's header note; formerly flagged as an unverified best guess).
_KCB_MPESA_RECEIVED_RE = re.compile(
    r"You have received\s+KES\s*(?P<amount>[\d,.]+)\s+from\s+(?P<name>[A-Za-z ]+?)\.\s*M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)",
    re.IGNORECASE,
)

_KCB_MPESA_SENT_RE = re.compile(
    r"(?P<ref>[A-Z0-9]{8,})\s+completed\.\s*KES\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+M-PESA\s+(?P<phone>2\d{9,11})\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s*"
    r"Transaction cost\s+KES\s*(?P<cost>[\d,]+\.?\d*)",
    re.IGNORECASE,
)

_KCB_MPESA_TRANSFERRED_RE = re.compile(
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

    m = _KCB_MPESA_PAYBILL_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        counterparty = f"KCB Pay Bill {m.group('paybill')} - {name}"
        return TransductionResult(
            status="mapped",
            operator_name="budget.record_income",
            operator_params={"amount": amount, "source": f"M-Pesa via KCB Pay Bill: {name}", "frequency": "once"},
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "received", "amount": amount, "counterparty": counterparty,
                "account": m.group("account"), "paybill": m.group("paybill"),
            },
            reason="Money received into KCB via M-Pesa paybill — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
            parser_name="kcb_mpesa_paybill",
        )

    m = _KCB_MPESA_ACCOUNT_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="mapped",
            operator_name="budget.record_income",
            operator_params={"amount": amount, "source": f"M-Pesa via KCB account: {name}", "frequency": "once"},
            external_ref=m.group("ref"),
            parsed_fields={
                "direction": "received", "amount": amount, "counterparty": name,
                "account": m.group("account"),
            },
            reason="Money received into KCB via M-Pesa account transfer — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
            parser_name="kcb_mpesa_account",
        )

    m = _KCB_MPESA_RECEIVED_RE.search(text)
    if m:
        amount = _to_float(m.group("amount"))
        name = m.group("name").strip()
        return TransductionResult(
            status="mapped",
            operator_name="budget.record_income",
            operator_params={"amount": amount, "source": f"M-Pesa via KCB: {name}", "frequency": "once"},
            external_ref=m.group("ref"),
            parsed_fields={"direction": "received", "amount": amount, "counterparty": name},
            reason="Money received via KCB (M-Pesa-network transfer) — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
            parser_name="kcb_mpesa_received",
        )

    m = _KCB_MPESA_SENT_RE.search(text)
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
            reason=_needs_pocket_reason("Sent to M-PESA via KCB", amount, counterparty),
            parser_name="kcb_mpesa_sent",
        )

    m = _KCB_MPESA_TRANSFERRED_RE.search(text)
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
            reason=_needs_pocket_reason("Transferred via KCB (M-Pesa network)", amount, name),
            parser_name="kcb_mpesa_transferred",
        )

    m = _KCB_BALANCE_RE.search(text)
    if m:
        return TransductionResult(
            status="informational",
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
            status="informational",
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
            status="informational",
            parsed_fields={"direction": "none", "loan_status": "arrears", "amount_owed": _to_float(m.group("amount"))},
            reason="KCB Mobile Loan in arrears — informational only, no transaction created.",
            parser_name="kcb_loan_arrears",
        )

    m = _KCB_LOAN_DEFAULT_RE.search(text)
    if m:
        return TransductionResult(
            status="informational",
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
            status="informational",
            parsed_fields={"direction": "none", "loan_status": "due_today", "amount_owed": _to_float(m.group("amount"))},
            reason="KCB Mobile Loan due today — informational only, no transaction created.",
            parser_name="kcb_loan_due_today",
        )

    if _is_informational_system_message(text):
        return TransductionResult(
            status="informational",
            parsed_fields={"direction": "none"},
            reason="A KCB system/error notice, not a transaction — nothing to record.",
            parser_name="kcb_system_notice",
        )

    return None


# KNOWN, DISCLOSED, NOT FIXED (flagged 1 Aug 2026): source is now purely
# sender-based (KCB sender -> _parse_kcb, MPESA sender -> _parse_mpesa; see
# the Android side's classifySource() and this module's own KCB-section
# header note above). If the SAME real-world transaction ever produces BOTH
# a KCB-sender notification (e.g. one of the _KCB_MPESA_* shapes above) AND
# a genuine Safaricom MPESA-sender SMS about that same transfer, ingest_engine
# .capture()'s dedup_key (sha256 of sustain_id + source_id + raw_payload) will
# NOT catch it as a duplicate -- source_id ("kcb" vs "mpesa") and raw_payload
# (different wording/sender) both differ between the two notifications, so
# they hash to two distinct keys and both get captured/processed as separate
# transactions. A real double-count risk for any transaction that genuinely
# triggers both a bank-side and a telco-side SMS. No fix attempted here --
# a real fix needs cross-source correlation (e.g. matching amount + a shared
# M-PESA ref extracted from both messages' parsed_fields, which several
# _KCB_MPESA_* shapes above already expose via their own "ref" group) that
# doesn't exist yet. Flagged for a deliberate decision, not silently patched.
_PARSERS: list[Callable[[str], "TransductionResult | None"]] = [_parse_mpesa, _parse_kcb]

# SOURCE IS STRICT (fixed 2 Aug 2026, real bug found via Bonnie's actual
# device): a message's SENDER (source_id, decided entirely by the Android
# capture client's classifySource() -- see smsCapture.js/IngestWorker.java's
# own comments, both purely sender-string-based) determines which parser
# SET is even eligible to run, full stop. Before this, parse_message()
# always tried _parse_mpesa first for EVERY message regardless of source_id
# -- source_id was pure display metadata, never actually gating parsing.
# That's a structural risk, not just a specific-incident one: several real
# KCB-sender messages legitimately contain the substring "M-PESA" in their
# own wording (the kcb_mpesa_* shapes above), and nothing stopped a FUTURE
# mpesa_* regex from being written broadly enough to accidentally match
# KCB-worded text, silently overriding what the sender already told us this
# was. This dict is what parse_message() actually uses when a known
# source_id is supplied -- text content can never again promote a message
# out of its own sender's parser set.
_PARSERS_BY_SOURCE: dict[str, list[Callable[[str], "TransductionResult | None"]]] = {
    "mpesa": [_parse_mpesa],
    "kcb": [_parse_kcb],
}


def parse_message(raw_text: str, source_id: str | None = None) -> TransductionResult:
    """
    source_id (optional) makes parsing STRICT when it names a known source:
    only that source's own parser set (_PARSERS_BY_SOURCE) is ever tried,
    regardless of what the message body itself says -- see this module's
    own comment above _PARSERS_BY_SOURCE for the real bug this closes.
    Every real caller supplies source_id (ingest_engine.capture() passes
    the exact source_id it was given straight through); source_id=None/
    unknown falls back to trying every registered parser in order (used by
    direct unit tests exercising the parser set on its own, and as an
    honest degrade for a genuinely unrecognised source rather than refusing
    outright).

    Within whichever set applies, the first parser that recognises the
    message wins. No parser recognising it is an honest, visible
    "unparsed" — never a silent drop. The sensitive-secret check runs
    first, unconditionally, before any parser gets a look at the text —
    see contains_sensitive_secret().
    """
    text = (raw_text or "").strip()
    if not text:
        return TransductionResult(status="unparsed", reason="Empty message.")
    if contains_sensitive_secret(text):
        return TransductionResult(
            status="rejected",
            reason="Message contains an OTP/verification code or similar secret — refused, never parsed or stored.",
        )
    parsers = _PARSERS_BY_SOURCE.get((source_id or "").lower(), _PARSERS)
    for parser in parsers:
        result = parser(text)
        if result is not None:
            return result
    return TransductionResult(
        status="unparsed",
        reason="No registered parser recognised this message's shape.",
    )
