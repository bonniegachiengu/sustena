"""
sustena/core/parse_rules_seed.py

Phase 3B (Parser primitive-lift, 2 Aug 2026) — the seed rule library.
Canon: SPEC-parser-primitive-lift.addendum.2026-07-29.md §4K.6's own plan
step 2: "Migrate the live regex into declared rule artifacts — the
existing _MPESA_*/_KCB_* shapes become data-authored rules... keep Python
fallback until declared parity, then retire."

Six real M-Pesa shapes are migrated here as ParseRule DATA — the ones
whose field extraction is pure regex-group-capture-plus-type-coercion,
with no bespoke post-processing beyond what ParseRule's three field types
(amount/string/literal) already express. Each one's `id`, `status`,
`operator`/`params`, `parsed_fields` shape, and `reason` text are an exact
match for the hand-wired parser it replaces — verified by dedicated parity
tests in test_parse_rule.py that run BOTH paths against the same real
sample text and assert identical TransductionResults.

Deliberately NOT migrated, disclosed rather than force-fit:
  - mpesa_withdraw — its counterparty is `agent if agent.startswith("agent")
    else f"Agent {agent}"`, a real conditional transform beyond simple
    string/amount coercion or a fixed format() template.
  - Every KCB shape and every informational/system-notice pattern — a
    materially larger migration (KCB alone has ~17 shapes with more
    per-field nuance: conditional currency, sub_source labeling, loan-
    status accrual fields). Left on the Python fallback tier for a future
    slice, exactly as the canon's own rollout plan sanctions: "existing
    parsers keep serving as the fallback tier throughout."
These stay on the Python fallback tier (_parse_mpesa/_parse_kcb in
transducer.py, tried after declared rules find no match) — nothing about
them is broken or disabled, they're simply not yet declared data.
"""

from sustena.core.parse_rule import FieldSpec, ParseRule

_REF = FieldSpec(type="string", group="ref")
_AMOUNT = FieldSpec(type="amount", group="amount")
_BALANCE = FieldSpec(type="amount", group="balance")
_COST = FieldSpec(type="amount", group="cost")

MPESA_RECEIVED = ParseRule(
    id="mpesa_received",
    source="mpesa",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+You have received\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+"
        r"from\s+(?P<name>[A-Za-z ]+?)\s+(?P<phone>2\d{9,11})\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
        r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
    ),
    extract={
        "ref": _REF, "amount": _AMOUNT, "balance_after": _BALANCE,
        "counterparty": FieldSpec(type="string", group="name"),
        "phone": FieldSpec(type="string", group="phone"),
        "direction": FieldSpec(type="literal", value="received"),
    },
    status="mapped",
    operator="budget.record_income",
    params={"amount": "$amount", "source": "M-Pesa: {counterparty}", "frequency": "once"},
    reason_template="Money received — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
    trust="shipped", provenance="sustena_core",
    examples=(
        "QGH7XJ4P2Q Confirmed. You have received Ksh1,500.00 from JOHN KAMAU 254712345678 "
        "on 2/8/26 at 10:15 AM. New M-PESA balance is Ksh12,050.00",
    ),
)

MPESA_PAYBILL = ParseRule(
    id="mpesa_paybill",
    source="mpesa",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+paid to\s+"
        r"(?P<business>[A-Za-z0-9 ]+?)\s+for account\s+(?P<account>\S+)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
        r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
    ),
    extract={
        "ref": _REF, "amount": _AMOUNT, "balance_after": _BALANCE,
        "counterparty": FieldSpec(type="string", group="business"),
        "account": FieldSpec(type="string", group="account"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Paybill payment of KES {amount:,.2f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIROBI WATER for account 998877 on 2/8/26 "
        "at 4:30 PM. New M-PESA balance is Ksh12,050.00",
    ),
)

MPESA_BUYGOODS = ParseRule(
    id="mpesa_buygoods",
    source="mpesa",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+paid to\s+"
        r"(?P<business>[A-Za-z0-9 ]+?)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
        r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
    ),
    extract={
        "ref": _REF, "amount": _AMOUNT, "balance_after": _BALANCE,
        "counterparty": FieldSpec(type="string", group="business"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Till payment of KES {amount:,.2f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 at 4:30 PM. "
        "New M-PESA balance is Ksh12,050.00",
    ),
)

MPESA_SENT = ParseRule(
    id="mpesa_sent",
    source="mpesa",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+sent to\s+"
        r"(?P<name>[A-Za-z ]+?)\s+(?P<phone>2\d{9,11})\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
        r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
    ),
    extract={
        "ref": _REF, "amount": _AMOUNT, "balance_after": _BALANCE,
        "counterparty": FieldSpec(type="string", group="name"),
        "phone": FieldSpec(type="string", group="phone"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Money sent of KES {amount:,.2f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "QGH7XJ4P2Q Confirmed. Ksh1,000.00 sent to MARY WANJIRU 254798765432 on 2/8/26 "
        "at 9:00 AM. New M-PESA balance is Ksh12,050.00",
    ),
)

MPESA_PAYBILL_SENT = ParseRule(
    id="mpesa_paybill_sent",
    source="mpesa",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+sent to\s+"
        r"(?P<business>[A-Za-z0-9 ]+?)\s+for account\s+(?P<account>\S+)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
        r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
        r"(?:\.\s*Transaction cost,?\s*Ksh(?P<cost>[\d,]+\.?\d*))?"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "ref": _REF, "amount": _AMOUNT, "balance_after": _BALANCE, "transaction_cost": _COST,
        "counterparty": FieldSpec(type="string", group="business"),
        "account": FieldSpec(type="string", group="account"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Paybill payment of KES {amount:,.2f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "QGH7XJ4P2Q Confirmed. Ksh1,500.00 sent to SAFARICOMHOME for account 11619547 on 1/8/26 "
        "at 3:22 PM. New M-PESA balance is Ksh12,154.47. Transaction cost, Ksh0.00.",
    ),
)

MPESA_AIRTIME = ParseRule(
    id="mpesa_airtime",
    source="mpesa",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{10})\s+confirmed\.\s+You bought\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+of airtime\s+on\s+"
        r"(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
        r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
        r"(?:\.\s*Transaction cost,?\s*Ksh(?P<cost>[\d,]+\.?\d*))?"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "ref": _REF, "amount": _AMOUNT, "balance_after": _BALANCE, "transaction_cost": _COST,
        "counterparty": FieldSpec(type="literal", value="Safaricom airtime"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Airtime purchase of KES {amount:,.2f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "QGH7XJ4P2Q confirmed. You bought Ksh50.00 of airtime on 1/8/26 at 11:15 PM. "
        "New M-PESA balance is Ksh13,654.47. Transaction cost, Ksh0.00.",
    ),
)

# Order matters exactly as it did in the hand-wired _parse_mpesa: PAYBILL's
# pattern is a strict superset of BUY_GOODS' (both are "paid to X on...";
# only paybill has "for account Y" in between), so paybill must be tried
# first or a paybill message would silently match buy-goods with the
# account number swallowed into the business-name group. RECEIVED is tried
# first overall since it's the one unambiguous "mapped" shape.
SEED_RULES_MPESA: list[ParseRule] = [
    MPESA_RECEIVED, MPESA_PAYBILL, MPESA_BUYGOODS, MPESA_SENT, MPESA_PAYBILL_SENT, MPESA_AIRTIME,
]

SEED_RULES_BY_SOURCE: dict[str, list[ParseRule]] = {
    "mpesa": SEED_RULES_MPESA,
    # "kcb": [] intentionally absent -- KCB stays 100% on the Python
    # fallback tier this slice; SEED_RULES_BY_SOURCE.get(source, []) is the
    # caller's own honest empty-list default, not a missing-key bug.
}
