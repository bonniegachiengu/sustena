"""
sustena/core/parse_rules_seed.py

Phase 3B (Parser primitive-lift, 2 Aug 2026) — the seed rule library.
Canon: SPEC-parser-primitive-lift.addendum.2026-07-29.md §4K.6's own plan
step 2: "Migrate the live regex into declared rule artifacts — the
existing _MPESA_*/_KCB_* shapes become data-authored rules... keep Python
fallback until declared parity, then retire."

Seven M-Pesa shapes plus fifteen KCB shapes are migrated here as ParseRule
DATA (item 2 of the "broaden parser coverage" round, 2 Aug 2026, closes
out the KCB-not-yet-migrated gap this file previously disclosed). Each
one's `id`, `status`, `operator`/`params`, `parsed_fields` shape, and
`reason` text are an exact match for the hand-wired parser it replaces —
verified by running the FULL pre-existing test_transducer.py suite
(zero-regression proof) plus dedicated new parity tests.

Three field types beyond plain amount/string/literal were needed to express
real per-shape nuance without inventing bespoke callables (see
parse_rule.py's own module docstring for the full rationale of each):
  - prefix_if_missing — mpesa_withdraw's "Agent {name}" normalisation.
  - upper             — kcb_card's currency (KES/USD) normalisation.
  - template          — several KCB shapes build ONE parsed_fields value
                        (typically `counterparty`) out of MULTIPLE
                        captured pieces, e.g. kcb_loan_repay's
                        counterparty=f"KCB Mobile Loan {loan_number}
                        repayment". A template field's str.format() call
                        sees both the raw regex groups and any field
                        already extracted earlier in the same rule's
                        `extract` dict (insertion order matters — see
                        parse_rule.py).

Deliberately NOT migrated, disclosed rather than force-fit:
  - kcb_system_notice — the shared informational-system-message fallback
    for BOTH mpesa and kcb (matched via _is_informational_system_message(),
    itself a list of ~20 loosely-related sub-patterns, not one named-
    group regex). This is a genuinely different shape of thing than every
    other rule here — a generic catch-all, not a single declarable
    pattern — and stays on the Python fallback tier. Every KCB message
    that ISN'T one of the 15 shapes below still correctly falls through
    to _parse_kcb() (which tries this catch-all last), exactly as before.
These stay on the Python fallback tier (_parse_mpesa/_parse_kcb in
transducer.py, tried after declared rules find no match) — nothing about
them is broken or disabled, they're simply not yet declared data.
"""

from sustena.core.parse_rule import FieldSpec, ParseRule

_REF = FieldSpec(type="string", group="ref")
_AMOUNT = FieldSpec(type="amount", group="amount")
_BALANCE = FieldSpec(type="amount", group="balance")
_COST = FieldSpec(type="amount", group="cost")
_LOAN_BALANCE = FieldSpec(type="amount", group="loan_balance")

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

MPESA_WITHDRAW = ParseRule(
    id="mpesa_withdraw",
    source="mpesa",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{10})\s+Confirmed\.\s+Ksh(?P<amount>[\d,]+\.?\d*)\s+withdrawn from\s+"
        r"(?P<agent>[A-Za-z0-9 \-]+?)\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s+"
        r"New M-PESA balance is Ksh(?P<balance>[\d,]+\.?\d*)"
    ),
    extract={
        "ref": _REF, "amount": _AMOUNT, "balance_after": _BALANCE,
        "counterparty": FieldSpec(type="prefix_if_missing", group="agent", value="Agent"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Cash withdrawal of KES {amount:,.2f} to {counterparty} recognised, but which pocket to "
        "spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "QGH7XJ4P2Q Confirmed. Ksh2,000.00 withdrawn from Agent 123456 - TOWN SHOP on 2/8/26 "
        "at 1:00 PM. New M-PESA balance is Ksh10,050.00",
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
    MPESA_RECEIVED, MPESA_PAYBILL, MPESA_BUYGOODS, MPESA_SENT, MPESA_WITHDRAW,
    MPESA_PAYBILL_SENT, MPESA_AIRTIME,
]


# ── KCB Kenya SMS alert shapes (declared data, migrated 2 Aug 2026) ─────────
# Regex patterns, field shapes, and reason text are copied verbatim from the
# hand-wired _parse_kcb() in transducer.py (see that function's own header
# comment for provenance -- these are REAL, Bonnie-verified sample formats,
# not speculative guesses). Order matches _parse_kcb()'s own if-chain
# exactly, which matters: run_rules() is first-match-wins over this LIST,
# same semantics as the original if/elif chain, so preserving order
# preserves precedence byte-for-byte (e.g. nothing here has an ordering
# collision risk that the original didn't already have).
KCB_RECEIVE = ParseRule(
    id="kcb_receive",
    source="kcb",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{8,})\s+Confirmed!\s+You have received\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+"
        r"from\s+(?P<name>[A-Za-z ]+?)\s*-\s*(?P<account>[\d*]+)\s+at\s+(?P<datetime>[\d\-: ]+?[APap][Mm])\s+via\s+KCB"
    ),
    flags=("IGNORECASE",),
    extract={
        "ref": _REF, "amount": _AMOUNT,
        "counterparty": FieldSpec(type="string", group="name"),
        "account": FieldSpec(type="string", group="account"),
        "datetime": FieldSpec(type="string", group="datetime"),
        "direction": FieldSpec(type="literal", value="received"),
    },
    status="mapped",
    operator="budget.record_income",
    params={"amount": "$amount", "source": "KCB: {counterparty}", "frequency": "once"},
    reason_template="Money received via KCB — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
    trust="shipped", provenance="sustena_core",
    examples=(
        "ABCD1234 Confirmed! You have received KES 5,000.00 from JOHN KAMAU - 1234 "
        "at 02-08-26 10:15 AM via KCB",
    ),
)

KCB_CARD = ParseRule(
    id="kcb_card",
    source="kcb",
    version=1,
    pattern=(
        r"(?P<currency>KES|USD)\s*(?P<amount>[\d,]+\.?\d*)\s+transaction made on\s+KCB card\s+(?P<card>[\dX*]+)\s+at\s+"
        r"(?P<merchant>.+?)\s+on\s+(?P<date>\S+)\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\s*,\s*Avail(?:able)?\s+balance\s+KES\s*(?P<balance>[\d,]+\.?\d*)"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "amount": _AMOUNT,
        "currency": FieldSpec(type="upper", group="currency"),
        "counterparty": FieldSpec(type="string", group="merchant"),
        "card": FieldSpec(type="string", group="card"),
        "balance_after": _BALANCE,
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "KCB card transaction of {currency} {amount:,.2f} to {counterparty} recognised, but which "
        "pocket to spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "KES 429.00 transaction made on KCB card 1234XXXXXXXX5678 at GOOGLE *Spotify Music "
        "on 1/8/26 12:25pm, Avail balance KES 59,055.00",
    ),
)

KCB_LOAN_DISBURSED = ParseRule(
    id="kcb_loan_disbursed",
    source="kcb",
    version=1,
    pattern=(
        r"KCB Mobile Loan of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+has been approved and credited"
        r".*?facility fee of\s+KES\s*(?P<fee>[\d,]+\.?\d*)\s+has been charged"
        r".*?loan balance is\s+KES\s*(?P<loan_balance>[\d,]+\.?\d*)"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "amount": _AMOUNT,
        "fee": FieldSpec(type="amount", group="fee"),
        "loan_balance": _LOAN_BALANCE,
        "counterparty": FieldSpec(type="literal", value="KCB Mobile Loan"),
        "direction": FieldSpec(type="literal", value="received"),
    },
    status="mapped",
    operator="budget.record_income",
    params={"amount": "$amount", "source": "KCB Mobile Loan", "frequency": "once"},
    reason_template=(
        "KCB Mobile Loan disbursed — mapped to budget.record_income (the loan amount unambiguously "
        "credits liquid balance); a facility fee was also charged, recorded for context but not "
        "separately deducted here."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "KCB Mobile Loan of KES 10,000.00 has been approved and credited to your account. A "
        "facility fee of KES 250.00 has been charged. Your new loan balance is KES 10,250.00",
    ),
)

KCB_LOAN_REPAY = ParseRule(
    id="kcb_loan_repay",
    source="kcb",
    version=1,
    pattern=(
        r"KES\s*(?P<amount>[\d,]+\.?\d*)\s+was debited from your KCB account to repay your KCB Mobile Loan\s+(?P<loan_number>\d+)"
        r".*?loan balance is\s+KES\s*(?P<loan_balance>[\d,]+\.?\d*)"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "amount": _AMOUNT,
        "loan_number": FieldSpec(type="string", group="loan_number"),
        "loan_balance": _LOAN_BALANCE,
        "counterparty": FieldSpec(type="template", value="KCB Mobile Loan {loan_number} repayment"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "KCB Mobile Loan repayment of KES {amount:,.2f} to {counterparty} recognised, but which "
        "pocket to spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "KES 2,000.00 was debited from your KCB account to repay your KCB Mobile Loan 998877. "
        "Your new loan balance is KES 8,250.00",
    ),
)

KCB_VOOMA_LOAN_REPAY = ParseRule(
    id="kcb_vooma_loan_repay",
    source="kcb",
    version=1,
    pattern=(
        r"KES\s*(?P<amount>[\d,]+\.?\d*)\s+was debited from your Vooma wallet to repay your Vooma Loan\s+(?P<loan_number>\d+)"
        r".*?loan balance (?:is|of)\s+KES\s*(?P<loan_balance>[\d,]+\.?\d*)"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "amount": _AMOUNT,
        "loan_number": FieldSpec(type="string", group="loan_number"),
        "loan_balance": _LOAN_BALANCE,
        "counterparty": FieldSpec(type="template", value="Vooma Loan {loan_number} repayment"),
        "sub_source": FieldSpec(type="literal", value="vooma"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Vooma Loan repayment of KES {amount:,.2f} to {counterparty} recognised, but which pocket "
        "to spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "KES 1,500.00 was debited from your Vooma wallet to repay your Vooma Loan 445566. Your "
        "new loan balance of KES 3,000.00",
    ),
)

KCB_MPESA_PAYBILL = ParseRule(
    id="kcb_mpesa_paybill",
    source="kcb",
    version=1,
    pattern=(
        r"Ksh\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+KCB Pay Bill\s+(?P<paybill>\d+)\s+for account\s+(?P<account>\S+)\s+"
        r"(?P<name>[A-Za-z ]+?)\s+has been received on\s+(?P<date>\S+)\s+at\s+(?P<time>\S+\s*[APap][Mm])"
        r".*?M-PESA ref\s+(?P<ref>[A-Za-z0-9]+)"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "amount": _AMOUNT,
        "name": FieldSpec(type="string", group="name"),
        "account": FieldSpec(type="string", group="account"),
        "paybill": FieldSpec(type="string", group="paybill"),
        "ref": _REF,
        "counterparty": FieldSpec(type="template", value="KCB Pay Bill {paybill} - {name}"),
        "direction": FieldSpec(type="literal", value="received"),
    },
    status="mapped",
    operator="budget.record_income",
    params={"amount": "$amount", "source": "M-Pesa via KCB Pay Bill: {name}", "frequency": "once"},
    reason_template="Money received into KCB via M-Pesa paybill — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
    trust="shipped", provenance="sustena_core",
    examples=(
        "Ksh40000.00 sent to KCB Pay Bill 522522 for account 135***5140 BONVENTURE NGUGI MAINA "
        "has been received on 01/08/2026 at 12:21 PM. M-PESA ref UH1B91GYNW",
    ),
)

KCB_MPESA_ACCOUNT = ParseRule(
    id="kcb_mpesa_account",
    source="kcb",
    version=1,
    pattern=(
        r"Ksh\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+KCB account\s+(?P<name>[A-Za-z ]+?)\s+(?P<account>\d+)\s+"
        r"has been received on\s+(?P<date>\S+)"
        r".*?M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "amount": _AMOUNT,
        "counterparty": FieldSpec(type="string", group="name"),
        "account": FieldSpec(type="string", group="account"),
        "ref": _REF,
        "direction": FieldSpec(type="literal", value="received"),
    },
    status="mapped",
    operator="budget.record_income",
    params={"amount": "$amount", "source": "M-Pesa via KCB account: {counterparty}", "frequency": "once"},
    reason_template="Money received into KCB via M-Pesa account transfer — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
    trust="shipped", provenance="sustena_core",
    examples=(
        "Ksh5000.00 sent to KCB account JOHN KAMAU 112233 has been received on 01/08/2026. "
        "M-PESA Ref UH1B91GYNX",
    ),
)

KCB_MPESA_RECEIVED = ParseRule(
    id="kcb_mpesa_received",
    source="kcb",
    version=1,
    pattern=(
        r"You have received\s+KES\s*(?P<amount>[\d,.]+)\s+from\s+(?P<name>[A-Za-z ]+?)\.\s*M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)"
    ),
    flags=("IGNORECASE",),
    extract={
        "amount": _AMOUNT,
        "counterparty": FieldSpec(type="string", group="name"),
        "ref": _REF,
        "direction": FieldSpec(type="literal", value="received"),
    },
    status="mapped",
    operator="budget.record_income",
    params={"amount": "$amount", "source": "M-Pesa via KCB: {counterparty}", "frequency": "once"},
    reason_template="Money received via KCB (M-Pesa-network transfer) — mapped to budget.record_income (unambiguous; income always credits liquid balance).",
    trust="shipped", provenance="sustena_core",
    examples=(
        "You have received KES 750.00 from MARY WANJIRU. M-PESA Ref UH1B91GYNY",
    ),
)

KCB_MPESA_SENT = ParseRule(
    id="kcb_mpesa_sent",
    source="kcb",
    version=1,
    pattern=(
        r"(?P<ref>[A-Z0-9]{8,})\s+completed\.\s*KES\s*(?P<amount>[\d,]+\.?\d*)\s+sent to\s+M-PESA\s+(?P<phone>2\d{9,11})\s+on\s+(?P<date>\S+)\s+at\s+(?P<time>\d{1,2}:\d{2}\s*[APap][Mm])\.?\s*"
        r"Transaction cost\s+KES\s*(?P<cost>[\d,]+\.?\d*)"
    ),
    flags=("IGNORECASE",),
    extract={
        "ref": _REF, "amount": _AMOUNT, "transaction_cost": _COST,
        "phone": FieldSpec(type="string", group="phone"),
        "counterparty": FieldSpec(type="template", value="M-PESA {phone}"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Sent to M-PESA via KCB of KES {amount:,.2f} to {counterparty} recognised, but which "
        "pocket to spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "ABCD5678 completed. KES 1,200.00 sent to M-PESA 254712345678 on 2/8/26 at 3:00 PM. "
        "Transaction cost KES 22.00",
    ),
)

KCB_MPESA_TRANSFERRED = ParseRule(
    id="kcb_mpesa_transferred",
    source="kcb",
    version=1,
    pattern=(
        r"you have successfully transferred\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+to\s+(?P<phone>[\d*]+)-(?P<name>[A-Za-z ]+?)\s+on\s+(?P<date>\S+)"
        r".*?M-PESA Ref\s+(?P<ref>[A-Za-z0-9]+)"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "ref": _REF, "amount": _AMOUNT,
        "counterparty": FieldSpec(type="string", group="name"),
        "phone": FieldSpec(type="string", group="phone"),
        "direction": FieldSpec(type="literal", value="sent"),
    },
    status="parsed_unmapped",
    reason_template=(
        "Transferred via KCB (M-Pesa network) of KES {amount:,.2f} to {counterparty} recognised, "
        "but which pocket to spend from isn't determined automatically — resolve manually."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "you have successfully transferred KES 900.00 to 254***5678-JOHN KAMAU on 2/8/26. "
        "M-PESA Ref UH1B91GYNZ",
    ),
)

KCB_BALANCE = ParseRule(
    id="kcb_balance",
    source="kcb",
    version=1,
    pattern=(
        r"Actual balance\s+KES\s*(?P<actual_balance>[\d,]+\.?\d*)\.\s*Available balance\s+KES\s*(?P<available_balance>[\d,]+\.?\d*)"
        r"(?:.*?[Tt]ransaction reference number\s+(?P<ref>[A-Za-z0-9]+))?"
    ),
    flags=("IGNORECASE", "DOTALL"),
    extract={
        "actual_balance": FieldSpec(type="amount", group="actual_balance"),
        "balance_after": FieldSpec(type="amount", group="available_balance"),
        "ref": _REF,
        "direction": FieldSpec(type="literal", value="none"),
    },
    status="informational",
    reason_template=(
        "KCB balance update — informational only, no pocket decision needed "
        "(available balance: KES {balance_after:,.2f})."
    ),
    trust="shipped", provenance="sustena_core",
    examples=(
        "Actual balance KES 12,500.00. Available balance KES 12,500.00. Transaction reference "
        "number UH1B91GYP0",
    ),
)

KCB_LOAN_OVERDUE = ParseRule(
    id="kcb_loan_overdue",
    source="kcb",
    version=1,
    pattern=r"KCB Mobile Loan repayment of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+is overdue since\s+(?P<date>\S+)",
    flags=("IGNORECASE",),
    extract={
        "amount_owed": _AMOUNT,
        "loan_status": FieldSpec(type="literal", value="overdue"),
        "direction": FieldSpec(type="literal", value="none"),
    },
    status="informational",
    reason_template="KCB Mobile Loan repayment overdue since {date} — informational only, no transaction created.",
    trust="shipped", provenance="sustena_core",
    examples=(
        "KCB Mobile Loan repayment of KES 1,000.00 is overdue since 20/7/26",
    ),
)

KCB_LOAN_ARREARS = ParseRule(
    id="kcb_loan_arrears",
    source="kcb",
    version=1,
    pattern=r"KCB Mobile Loan is in arrears of\s+KES\s*(?P<amount>[\d,]+\.?\d*)",
    flags=("IGNORECASE",),
    extract={
        "amount_owed": _AMOUNT,
        "loan_status": FieldSpec(type="literal", value="arrears"),
        "direction": FieldSpec(type="literal", value="none"),
    },
    status="informational",
    reason_template="KCB Mobile Loan in arrears — informational only, no transaction created.",
    trust="shipped", provenance="sustena_core",
    examples=(
        "KCB Mobile Loan is in arrears of KES 3,500.00",
    ),
)

KCB_LOAN_DEFAULT = ParseRule(
    id="kcb_loan_default",
    source="kcb",
    version=1,
    pattern=r"loan number\s+(?P<loan_number>\d+)\s+has a balance of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+which is in DEFAULT",
    flags=("IGNORECASE",),
    extract={
        "loan_number": FieldSpec(type="string", group="loan_number"),
        "amount_owed": _AMOUNT,
        "loan_status": FieldSpec(type="literal", value="default"),
        "direction": FieldSpec(type="literal", value="none"),
    },
    status="informational",
    reason_template="KCB Mobile Loan in default — informational only, no transaction created.",
    trust="shipped", provenance="sustena_core",
    examples=(
        "loan number 998877 has a balance of KES 5,000.00 which is in DEFAULT",
    ),
)

KCB_LOAN_DUE_TODAY = ParseRule(
    id="kcb_loan_due_today",
    source="kcb",
    version=1,
    pattern=r"KCB Mobile Loan balance of\s+KES\s*(?P<amount>[\d,]+\.?\d*)\s+is due today",
    flags=("IGNORECASE",),
    extract={
        "amount_owed": _AMOUNT,
        "loan_status": FieldSpec(type="literal", value="due_today"),
        "direction": FieldSpec(type="literal", value="none"),
    },
    status="informational",
    reason_template="KCB Mobile Loan due today — informational only, no transaction created.",
    trust="shipped", provenance="sustena_core",
    examples=(
        "KCB Mobile Loan balance of KES 2,000.00 is due today",
    ),
)

# Order matches _parse_kcb()'s own if-chain exactly (see this block's own
# header comment above) -- kcb_system_notice is deliberately absent, see
# module docstring's "Deliberately NOT migrated" section.
SEED_RULES_KCB: list[ParseRule] = [
    KCB_RECEIVE, KCB_CARD, KCB_LOAN_DISBURSED, KCB_LOAN_REPAY, KCB_VOOMA_LOAN_REPAY,
    KCB_MPESA_PAYBILL, KCB_MPESA_ACCOUNT, KCB_MPESA_RECEIVED, KCB_MPESA_SENT, KCB_MPESA_TRANSFERRED,
    KCB_BALANCE, KCB_LOAN_OVERDUE, KCB_LOAN_ARREARS, KCB_LOAN_DEFAULT, KCB_LOAN_DUE_TODAY,
]

SEED_RULES_BY_SOURCE: dict[str, list[ParseRule]] = {
    "mpesa": SEED_RULES_MPESA,
    "kcb": SEED_RULES_KCB,
}
