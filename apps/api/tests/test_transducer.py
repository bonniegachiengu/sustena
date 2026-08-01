"""
tests/test_transducer.py

Unit tests for sustena/core/transducer.py — the deterministic tau that turns
a raw captured message into a TransductionResult, never silently.

Covers the three-tier contract:
  mapped           — money received, confidently routed to budget.record_income
  parsed_unmapped   — money sent (paybill/till/person/withdrawal) understood,
                      pocket deliberately not guessed
  unparsed          — garbage / empty text, honestly declared unrecognised

Sample messages mirror Safaricom's publicly documented M-Pesa confirmation
SMS formats — not tied to any real transaction.
"""

from sustena.core.transducer import parse_message


RECEIVED = (
    "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN KAMAU "
    "254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is Ksh15,000.00"
)

PAYBILL = (
    "QGH7XJ3M1N Confirmed. Ksh2,500.00 paid to KPLC PREPAID for account "
    "12345678 on 20/7/26 at 3:00 PM. New M-PESA balance is Ksh12,500.00"
)

BUYGOODS = (
    "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on 20/7/26 "
    "at 4:30 PM. New M-PESA balance is Ksh12,050.00"
)

SENT = (
    "QGH7XJ5R3S Confirmed. Ksh1,000.00 sent to MARY WANJIRU 254798765432 "
    "on 20/7/26 at 5:00 PM. New M-PESA balance is Ksh11,050.00"
)

WITHDRAW = (
    "QGH7XJ6T4U Confirmed. Ksh3,000.00 withdrawn from Agent 123456 - TOWN "
    "SHOP on 20/7/26 at 6:00 PM. New M-PESA balance is Ksh8,050.00"
)

WITHDRAW_NO_AGENT_PREFIX = (
    "QGH7XJ7V5W Confirmed. Ksh1,200.00 withdrawn from 654321 - ESTATE SHOP "
    "on 20/7/26 at 7:00 PM. New M-PESA balance is Ksh6,850.00"
)

# ── M-Pesa -> KCB bridge samples — REAL text Bonnie pasted (1 Aug 2026) ─────
# Personal names replaced with placeholder Kenyan names matching this
# file's existing convention (JOHN KAMAU / MARY WANJIRU) -- amounts, dates,
# references, and all other wording are exactly as received.

MPESA_TO_KCB_PAYBILL = (
    "Ksh 15452.00 sent to KCB Pay Bill 522522 for account 121***2684 "
    "MARY WANJIRU has been received on 29/05/2026 at 10:01 AM. M-PESA ref UETB9611ZB"
)

MPESA_TO_KCB_ACCOUNT = (
    "Ksh 700.00 sent to KCB account GRACE NJERI OTIENO 7757675 has been "
    "received on 31/03/2026 at 09:00 AM. M-PESA Ref UCVB9B85C2"
)

# ── OTP / sensitive-secret sample — REAL shape Bonnie pasted ────────────────
# Must NEVER be parsed, stored, or forwarded regardless of sender.

OTP_MESSAGE = (
    "Your card ending with 0319 has initiated an online transaction of USD 113.8 "
    "at ANTHROPIC. Your OTP is 083345. DO NOT SHARE WITH ANYONE."
)

# ── KCB samples — REAL text Bonnie pasted (1 Aug 2026) ──────────────────────
# Replaces an earlier, wholly speculative KCB pattern set that never
# matched any of these real formats. Personal names replaced with
# placeholder Kenyan names (same convention as above); amounts, dates,
# references, masked account/card numbers, and all other wording are
# exactly as received.

KCB_RECEIVE = (
    "MBNHE52IR84HTIOL Confirmed! You have received KES 1,350.00 from "
    "MARY WANJIRU - 121****684 at 2026-06-10 06:00:17 PM via KCB."
)

KCB_CARD_KES = (
    "KES 429.00 transaction made on KCB card 4243***0319 at GOOGLE *Spotify Music "
    "on 01/08/2026 12:25pm, Avail balance KES 59,055.00 as at 01/08/2026 12:25pm"
)

KCB_CARD_USD = (
    "USD 113.80 transaction made on KCB card 4243***0319 at ANTHROPIC* CLAUDE SUB "
    "on 01/08/2026 12:48pm, Avail balance KES 42,381.65 as at 01/08/2026 12:48pm"
)

KCB_LOAN_DISBURSED = (
    "your KCB Mobile Loan of KES 3000 has been approved and credited to your KCB "
    "Account. A facility fee of KES 214.62 has been charged. Your loan balance is "
    "KES 3214.62, first instalment is due on 27/09/2025."
)

KCB_LOAN_REPAY = (
    "KES 77.45 was debited from your KCB account to repay your KCB Mobile Loan "
    "188925612. Thank you. Your loan balance is KES 15451.89 as at 01/08/2026."
)

KCB_VOOMA_LOAN_REPAY = (
    "KES 8.52 was debited from your Vooma wallet to repay your Vooma Loan "
    "173389621. Thank you. Your loan balance of KES 14515.23 as at 01/08/2026."
)

KCB_BALANCE = (
    "Dear MARY, Actual balance KES 6,372.36. Available balance KES 13,382.60. "
    "Transaction reference number CHSEC6BDHU"
)

KCB_LOAN_OVERDUE = (
    "your KCB Mobile Loan repayment of KES 14522.94 is overdue since 31/07/2025 "
    "please pay to avoid penalties."
)

KCB_LOAN_ARREARS = (
    "Dear customer your KCB Mobile Loan is in arrears of KES 6984.34 please clear "
    "immediately."
)

KCB_LOAN_DEFAULT = (
    "Your loan number 173389621 has a balance of KES 14515.23 which is in DEFAULT "
    "please contact us."
)

KCB_LOAN_DUE_TODAY = (
    "your KCB Mobile Loan balance of KES 15521.09 is due today please pay to avoid "
    "penalties."
)

# ── "AMBIGUOUS" samples — Bonnie's own framing: look like Vooma/KCB-app ─────
# notifications of M-Pesa-network activity. Classified under kcb as a
# disclosed best-judgment call (see kcb_bridge_* parser_names) rather than a
# verified classification -- real names replaced as above.

KCB_BRIDGE_RECEIVED = (
    "You have received KES 1050.0 from MARY WANJIRU. M-PESA Ref TF2987NE4D. "
    "Transaction Ref No CF259N9TRN"
)

KCB_BRIDGE_SENT_TO_MPESA = (
    "CHT4C7F3H0 completed.KES 500.00 sent to M-PESA 254727916967 on 29/08/2025 "
    "at 12:26 PM.Transaction cost KES 11.90 New M-PESA balance is KES 500.00"
)

KCB_BRIDGE_TRANSFERRED = (
    "Dear MARY WANJIRU you have successfully transferred KES 14,100.00 to "
    "25472****143-GRACE NJERI OTIENO on 28/05/2026 at 09:00 AM. M-PESA Ref UESB95XFAB"
)


class TestMapped:
    def test_received_maps_to_record_income(self):
        result = parse_message(RECEIVED)
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.operator_params["amount"] == 5000.0
        assert result.operator_params["source"] == "M-Pesa: JOHN KAMAU"
        assert result.operator_params["frequency"] == "once"

    def test_received_external_ref_and_parsed_fields(self):
        result = parse_message(RECEIVED)
        assert result.external_ref == "QGH7XJ2K9L"
        assert result.parsed_fields["direction"] == "received"
        assert result.parsed_fields["counterparty"] == "JOHN KAMAU"
        assert result.parsed_fields["balance_after"] == 15000.0

    def test_received_reason_is_legible(self):
        result = parse_message(RECEIVED)
        assert result.reason
        assert "record_income" in result.reason

    def test_received_parser_name(self):
        assert parse_message(RECEIVED).parser_name == "mpesa_received"


class TestParsedUnmappedNeverGuessesAPocket:
    def test_paybill_is_parsed_unmapped(self):
        result = parse_message(PAYBILL)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None
        assert result.operator_params == {}

    def test_paybill_parsed_fields(self):
        result = parse_message(PAYBILL)
        assert result.parsed_fields["direction"] == "sent"
        assert result.parsed_fields["amount"] == 2500.0
        assert result.parsed_fields["counterparty"] == "KPLC PREPAID"
        assert result.parsed_fields["account"] == "12345678"
        assert result.external_ref == "QGH7XJ3M1N"

    def test_buygoods_is_parsed_unmapped(self):
        result = parse_message(BUYGOODS)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None
        assert result.parsed_fields["counterparty"] == "NAIVAS SUPERMARKET"
        assert result.parsed_fields["amount"] == 450.0

    def test_paybill_beats_buygoods_pattern(self):
        # Paybill's pattern is a strict superset of buy-goods' (both are
        # "paid to X on..."; only paybill has "for account Y" first) — a
        # paybill message must never fall through and match buy-goods with
        # the account number swallowed into the business name.
        result = parse_message(PAYBILL)
        assert result.parser_name == "mpesa_paybill"
        assert "for account" not in result.parsed_fields["counterparty"]

    def test_sent_is_parsed_unmapped(self):
        result = parse_message(SENT)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["counterparty"] == "MARY WANJIRU"
        assert result.parsed_fields["amount"] == 1000.0

    def test_withdraw_is_parsed_unmapped(self):
        result = parse_message(WITHDRAW)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["amount"] == 3000.0

    def test_withdraw_counterparty_not_double_prefixed(self):
        # Regression: "Agent 123456 - TOWN SHOP" must not render as
        # "Agent: Agent 123456 - TOWN SHOP".
        result = parse_message(WITHDRAW)
        assert result.parsed_fields["counterparty"] == "Agent 123456 - TOWN SHOP"
        assert not result.parsed_fields["counterparty"].lower().startswith("agent agent")

    def test_withdraw_agent_prefix_added_when_missing(self):
        result = parse_message(WITHDRAW_NO_AGENT_PREFIX)
        assert result.parsed_fields["counterparty"] == "Agent 654321 - ESTATE SHOP"

    def test_all_parsed_unmapped_reasons_mention_pocket(self):
        for text in (PAYBILL, BUYGOODS, SENT, WITHDRAW):
            result = parse_message(text)
            assert "pocket" in result.reason.lower()


# ── Sensitive-secret rejection — CRITICAL, checked before every parser ─────

class TestSensitiveSecretRejection:
    def test_otp_message_is_rejected(self):
        result = parse_message(OTP_MESSAGE)
        assert result.status == "rejected"

    def test_rejected_never_maps_to_an_operator(self):
        result = parse_message(OTP_MESSAGE)
        assert result.operator_name is None
        assert result.operator_params == {}

    def test_rejected_carries_no_parsed_transaction_fields(self):
        # A rejected message must never leak the amount/merchant it
        # happened to also contain -- there is nothing here to resolve
        # manually, unlike parsed_unmapped.
        result = parse_message(OTP_MESSAGE)
        assert result.parsed_fields == {}

    def test_rejected_reason_is_legible(self):
        result = parse_message(OTP_MESSAGE)
        assert "otp" in result.reason.lower() or "secret" in result.reason.lower()

    def test_a_real_kcb_transaction_is_never_flagged_as_sensitive(self):
        # False-positive check: none of the real transaction vocabulary
        # (Confirmed/credited/debited/received/balance) should ever trip
        # the sensitive-secret guard.
        for text in (KCB_RECEIVE, KCB_CARD_KES, KCB_LOAN_DISBURSED, RECEIVED, PAYBILL):
            assert parse_message(text).status != "rejected"


# ── M-Pesa -> KCB bridge — real text, kept under the mpesa parser ──────────

class TestMpesaToKcbBridge:
    def test_paybill_to_kcb_is_parsed_unmapped(self):
        result = parse_message(MPESA_TO_KCB_PAYBILL)
        assert result.status == "parsed_unmapped"
        assert result.parser_name == "mpesa_to_kcb_paybill"
        assert result.parsed_fields["amount"] == 15452.00
        assert result.parsed_fields["account"] == "121***2684"
        assert result.external_ref == "UETB9611ZB"

    def test_kcb_account_is_parsed_unmapped(self):
        result = parse_message(MPESA_TO_KCB_ACCOUNT)
        assert result.status == "parsed_unmapped"
        assert result.parser_name == "mpesa_to_kcb_account"
        assert result.parsed_fields["amount"] == 700.00
        assert result.parsed_fields["counterparty"] == "GRACE NJERI OTIENO"
        assert result.external_ref == "UCVB9B85C2"


# ── KCB — real text Bonnie pasted, replacing the earlier speculative set ───

class TestKCBReceiveIsMapped:
    def test_receive_maps_to_record_income(self):
        result = parse_message(KCB_RECEIVE)
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.operator_params["amount"] == 1350.00
        assert result.operator_params["frequency"] == "once"

    def test_receive_external_ref_and_parsed_fields(self):
        result = parse_message(KCB_RECEIVE)
        assert result.external_ref == "MBNHE52IR84HTIOL"
        assert result.parsed_fields["direction"] == "received"
        assert result.parsed_fields["counterparty"] == "MARY WANJIRU"
        assert result.parsed_fields["account"] == "121****684"

    def test_receive_parser_name(self):
        assert parse_message(KCB_RECEIVE).parser_name == "kcb_receive"


class TestKCBCardIsParsedUnmapped:
    def test_kes_card_transaction(self):
        result = parse_message(KCB_CARD_KES)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None
        assert result.parsed_fields["amount"] == 429.00
        assert result.parsed_fields["currency"] == "KES"
        assert result.parsed_fields["counterparty"] == "GOOGLE *Spotify Music"
        assert result.parsed_fields["balance_after"] == 59055.00

    def test_usd_card_transaction(self):
        # Currency can be KES or USD -- the transaction amount's own
        # currency, distinct from Avail balance which is always KES.
        result = parse_message(KCB_CARD_USD)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["amount"] == 113.80
        assert result.parsed_fields["currency"] == "USD"
        assert result.parsed_fields["counterparty"] == "ANTHROPIC* CLAUDE SUB"
        assert result.parsed_fields["balance_after"] == 42381.65

    def test_usd_reason_names_usd_not_kes(self):
        result = parse_message(KCB_CARD_USD)
        assert "USD" in result.reason
        assert "pocket" in result.reason.lower()


class TestKCBLoanDisbursement:
    def test_disbursement_maps_to_record_income(self):
        result = parse_message(KCB_LOAN_DISBURSED)
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.operator_params["amount"] == 3000.0
        assert result.operator_params["source"] == "KCB Mobile Loan"

    def test_disbursement_carries_fee_and_loan_balance(self):
        result = parse_message(KCB_LOAN_DISBURSED)
        assert result.parsed_fields["fee"] == 214.62
        assert result.parsed_fields["loan_balance"] == 3214.62


class TestKCBLoanRepayment:
    def test_kcb_loan_repay_is_parsed_unmapped(self):
        result = parse_message(KCB_LOAN_REPAY)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None
        assert result.parsed_fields["amount"] == 77.45
        assert result.parsed_fields["loan_number"] == "188925612"
        assert result.parsed_fields["loan_balance"] == 15451.89

    def test_vooma_loan_repay_is_parsed_unmapped(self):
        result = parse_message(KCB_VOOMA_LOAN_REPAY)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["amount"] == 8.52
        assert result.parsed_fields["sub_source"] == "vooma"
        assert result.parsed_fields["loan_number"] == "173389621"

    def test_kcb_and_vooma_repay_never_mapped(self):
        for text in (KCB_LOAN_REPAY, KCB_VOOMA_LOAN_REPAY):
            assert parse_message(text).operator_name is None


class TestKCBAmbiguousBridgeShapes:
    """Bonnie's own framing: these look like Vooma/KCB-app notifications of
    M-Pesa-network activity. Classified under kcb (kcb_bridge_* parser
    names) as a disclosed best-judgment call -- not a verified
    classification of which real sender id these actually arrive from."""

    def test_bridge_received_is_mapped(self):
        result = parse_message(KCB_BRIDGE_RECEIVED)
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.operator_params["amount"] == 1050.0
        assert result.parser_name == "kcb_bridge_received"
        assert result.external_ref == "TF2987NE4D"

    def test_bridge_sent_to_mpesa_is_parsed_unmapped(self):
        result = parse_message(KCB_BRIDGE_SENT_TO_MPESA)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["amount"] == 500.00
        assert result.parsed_fields["transaction_cost"] == 11.90
        assert result.external_ref == "CHT4C7F3H0"
        assert result.parser_name == "kcb_bridge_sent_to_mpesa"

    def test_bridge_transferred_is_parsed_unmapped(self):
        result = parse_message(KCB_BRIDGE_TRANSFERRED)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["amount"] == 14100.00
        assert result.parsed_fields["counterparty"] == "GRACE NJERI OTIENO"
        assert result.external_ref == "UESB95XFAB"
        assert result.parser_name == "kcb_bridge_transferred"


class TestKCBBalanceIsInformationalOnly:
    def test_balance_is_parsed_unmapped_not_mapped(self):
        result = parse_message(KCB_BALANCE)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None

    def test_balance_has_no_direction_or_amount(self):
        result = parse_message(KCB_BALANCE)
        assert result.parsed_fields["direction"] == "none"
        assert "amount" not in result.parsed_fields
        assert result.parsed_fields["actual_balance"] == 6372.36
        assert result.parsed_fields["balance_after"] == 13382.60

    def test_balance_reason_says_informational(self):
        result = parse_message(KCB_BALANCE)
        assert "informational" in result.reason.lower()

    def test_balance_external_ref(self):
        assert parse_message(KCB_BALANCE).external_ref == "CHSEC6BDHU"


class TestKCBLoanStatusIsInformationalOnly:
    """Overdue/arrears/default/due-today notices recur and their amount
    grows via accruing interest -- never a transaction, always
    informational, never mapped to an operator."""

    def test_overdue_is_informational(self):
        result = parse_message(KCB_LOAN_OVERDUE)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None
        assert result.parsed_fields["direction"] == "none"
        assert result.parsed_fields["loan_status"] == "overdue"
        assert result.parsed_fields["amount_owed"] == 14522.94
        assert "informational" in result.reason.lower()

    def test_arrears_is_informational(self):
        result = parse_message(KCB_LOAN_ARREARS)
        assert result.parsed_fields["loan_status"] == "arrears"
        assert result.parsed_fields["amount_owed"] == 6984.34
        assert "informational" in result.reason.lower()

    def test_default_is_informational(self):
        result = parse_message(KCB_LOAN_DEFAULT)
        assert result.parsed_fields["loan_status"] == "default"
        assert result.parsed_fields["loan_number"] == "173389621"
        assert result.parsed_fields["amount_owed"] == 14515.23
        assert "informational" in result.reason.lower()

    def test_due_today_is_informational(self):
        result = parse_message(KCB_LOAN_DUE_TODAY)
        assert result.parsed_fields["loan_status"] == "due_today"
        assert result.parsed_fields["amount_owed"] == 15521.09
        assert "informational" in result.reason.lower()

    def test_none_of_these_are_ever_mapped(self):
        for text in (KCB_LOAN_OVERDUE, KCB_LOAN_ARREARS, KCB_LOAN_DEFAULT, KCB_LOAN_DUE_TODAY):
            assert parse_message(text).operator_name is None

    def test_repeated_occurrences_with_a_different_amount_are_distinct_captures(self):
        # Not a transducer-level concern per se (dedup happens in
        # ingest_engine on the full raw text), but confirms the shape
        # recognition itself doesn't collapse two different amounts into
        # the same result -- each day's slightly larger overdue figure
        # really is parsed as a different amount_owed.
        grown = KCB_LOAN_OVERDUE.replace("14522.94", "14601.10")
        a = parse_message(KCB_LOAN_OVERDUE)
        b = parse_message(grown)
        assert a.parsed_fields["amount_owed"] != b.parsed_fields["amount_owed"]


class TestKCBDoesNotShadowMpesa:
    def test_mpesa_messages_still_parse_as_mpesa(self):
        # Adding _parse_kcb to _PARSERS must never change an M-Pesa
        # message's outcome -- mpesa is tried first.
        for text in (RECEIVED, PAYBILL, BUYGOODS, SENT, WITHDRAW):
            result = parse_message(text)
            assert result.parser_name.startswith("mpesa_")

    def test_kcb_determinism(self):
        a = parse_message(KCB_RECEIVE)
        b = parse_message(KCB_RECEIVE)
        assert a == b


class TestUnparsed:
    def test_empty_string_is_unparsed(self):
        result = parse_message("")
        assert result.status == "unparsed"
        assert result.reason == "Empty message."

    def test_whitespace_only_is_unparsed(self):
        result = parse_message("   \n\t  ")
        assert result.status == "unparsed"

    def test_garbage_text_is_unparsed(self):
        result = parse_message("hey are we still on for lunch tomorrow?")
        assert result.status == "unparsed"
        assert result.operator_name is None
        assert result.reason

    def test_unparsed_never_silently_empty_reason(self):
        result = parse_message("asdkjashdkjashd")
        assert result.reason != ""


class TestDeterminism:
    def test_same_input_produces_identical_result_twice(self):
        # The idempotent-intake layer above this relies on the transducer
        # never producing a different answer for the same input.
        a = parse_message(RECEIVED)
        b = parse_message(RECEIVED)
        assert a == b

    def test_unparsed_is_deterministic_too(self):
        a = parse_message("garbled nonsense")
        b = parse_message("garbled nonsense")
        assert a == b
