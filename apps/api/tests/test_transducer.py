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

# ── Two more real M-Pesa shapes — REAL text Bonnie pasted (2 Aug 2026) ──────
# Both came through UNPARSED against the original 5 patterns above.
# PAYBILL_SENT uses "sent to X for account Y" -- the existing PAYBILL
# pattern only recognises "paid to X for account Y"; Safaricom genuinely
# uses both verbs for the same paybill-payment shape. AIRTIME is a wholly
# different sentence structure with no counterparty at all, and its real
# sample has lowercase "confirmed" (every other real sample capitalises
# it) -- a genuine, observed variation, not a guess.

MPESA_PAYBILL_SENT = (
    "UH1B91JZZN Confirmed. Ksh1,500.00 sent to SAFARICOMHOME for account 11619547 "
    "on 1/8/26 at 11:22 PM. New M-PESA balance is Ksh12,154.47. Transaction cost, Ksh0.00."
)

MPESA_AIRTIME = (
    "UH1B91JUJI confirmed. You bought Ksh50.00 of airtime on 1/8/26 at 11:15 PM. "
    "New M-PESA balance is Ksh13,654.47. Transaction cost, Ksh0.00. Amount you can "
    "transact within the day is 440,663.00. Download My OneApp on https://saf.cx/3wAmy"
)

# ── KCB<->M-Pesa samples — REAL text Bonnie pasted (1 Aug 2026) ─────────────
# Personal names replaced with placeholder Kenyan names matching this
# file's existing convention (JOHN KAMAU / MARY WANJIRU) -- amounts, dates,
# references, and all other wording are exactly as received. These two
# formats read like a Safaricom M-Pesa confirmation but Bonnie confirmed
# (1 Aug 2026) they arrive from the KCB sender id on his real device --
# source is decided by sender, never by wording. See TestKcbMpesaShapes below.

KCB_MPESA_PAYBILL = (
    "Ksh 15452.00 sent to KCB Pay Bill 522522 for account 121***2684 "
    "MARY WANJIRU has been received on 29/05/2026 at 10:01 AM. M-PESA ref UETB9611ZB"
)

KCB_MPESA_ACCOUNT = (
    "Ksh 700.00 sent to KCB account GRACE NJERI OTIENO 7757675 has been "
    "received on 31/03/2026 at 09:00 AM. M-PESA Ref UCVB9B85C2"
)

# ── OTP / sensitive-secret sample — REAL shape Bonnie pasted ────────────────
# Must NEVER be parsed, stored, or forwarded regardless of sender.

OTP_MESSAGE = (
    "Your card ending with 0319 has initiated an online transaction of USD 113.8 "
    "at ANTHROPIC. Your OTP is 083345. DO NOT SHARE WITH ANYONE."
)

# Three more real secret shapes Bonnie's actual KCB thread showed (2 Aug
# 2026) that the original 5-pattern filter missed entirely -- CODE DIGITS
# REPLACED WITH PLACEHOLDERS, same discipline already used for real names
# elsewhere in this file, since these are live-ish secrets.
TAN_CODE_MESSAGE = "Your tan code is 000000. It will be active for the next 02:00 minutes"
ACTIVATION_CODE_MESSAGE = "Please use Activation code: 0000. This code is valid for only 90s"
CARD_SECRET_PIN_MESSAGE = "Your KCB card secret PIN is 0000."

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

# Replaced 2 Aug 2026 with the AUTHORITATIVE real texts from Bonnie's actual
# KCB screenshots -- the earlier versions here ("...as at 01/08/2026
# 12:25pm") were a speculative guess at the trailing text (never verified
# against a real message) that happened to still parse only because the
# regex doesn't anchor to end-of-string; the real trailer is completely
# different ("Enquiries call ...Pata extra cash with a flexible Mobile
# Loan..."). Confirmed against every real sample directly before replacing,
# same discipline the original KCB pattern-set replacement used.
KCB_CARD_KES = (
    "KES 429.00 transaction made on KCB card 4243***0319 at GOOGLE *Spotify Music "
    "on 01/08/2026 12:25pm, Avail balance KES 59,055.00 Enquiries call "
    "+254711087000.Pata extra cash with a flexible Mobile Loan. Dial *522# or use "
    "the KCB App to check limit."
)

# A second real card sample -- a merchant string containing "#" and a URL-
# shaped counterparty, confirming the merchant capture group handles both
# without special-casing.
KCB_CARD_HELPPAY = (
    "KES 1,500.00 transaction made on KCB card 4243***0319 at g.co/helppay# "
    "on 01/08/2026 12:38pm, Avail balance KES 57,555.00 Enquiries call "
    "+254711087000. Pata extra cash with a flexible Mobile Loan. Dial *522# or use "
    "the KCB App to check limit."
)

# The USD sample was given truncated ("...") in the real screenshot excerpt
# -- reconstructed here following the exact same real trailer template the
# two KES samples above confirmed, not invented from scratch. Disclosed,
# not silently presented as independently verified word-for-word.
KCB_CARD_USD = (
    "USD 113.80 transaction made on KCB card 4243***0319 at ANTHROPIC* CLAUDE SUB "
    "on 01/08/2026 12:48pm, Avail balance KES 42,381.65 Enquiries call "
    "+254711087000. Pata extra cash with a flexible Mobile Loan. Dial *522# or use "
    "the KCB App to check limit."
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

# ── KCB<->M-Pesa-network samples — CONFIRMED KCB sender (1 Aug 2026) ────────
# Previously flagged as "AMBIGUOUS" -- Bonnie's own best-guess framing that
# these might be Vooma/KCB-app relays of M-Pesa activity, origin sender
# unconfirmed. Bonnie has since checked his real device directly: all three
# arrive from the KCB sender id. Real names replaced as above.

KCB_MPESA_RECEIVED = (
    "You have received KES 1050.0 from MARY WANJIRU. M-PESA Ref TF2987NE4D. "
    "Transaction Ref No CF259N9TRN"
)

KCB_MPESA_SENT = (
    "CHT4C7F3H0 completed.KES 500.00 sent to M-PESA 254727916967 on 29/08/2025 "
    "at 12:26 PM.Transaction cost KES 11.90 New M-PESA balance is KES 500.00"
)

KCB_MPESA_TRANSFERRED = (
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


class TestNewMpesaShapes:
    """Two real, genuine M-Pesa shapes (2 Aug 2026) that came through
    UNPARSED against the original 5 patterns -- both are outgoing (money
    OUT), so both must carry direction="sent" (the signal effect_capture.
    infer() uses to correctly narrow toward budget.spend, never
    budget.allocate)."""

    def test_paybill_sent_variant_is_parsed_unmapped(self):
        result = parse_message(MPESA_PAYBILL_SENT)
        assert result.status == "parsed_unmapped"
        assert result.parser_name == "mpesa_paybill_sent"
        assert result.operator_name is None
        assert result.parsed_fields["direction"] == "sent"
        assert result.parsed_fields["amount"] == 1500.0
        assert result.parsed_fields["counterparty"] == "SAFARICOMHOME"
        assert result.parsed_fields["account"] == "11619547"
        assert result.parsed_fields["transaction_cost"] == 0.0
        assert result.parsed_fields["balance_after"] == 12154.47
        assert result.external_ref == "UH1B91JZZN"

    def test_airtime_is_parsed_unmapped(self):
        result = parse_message(MPESA_AIRTIME)
        assert result.status == "parsed_unmapped"
        assert result.parser_name == "mpesa_airtime"
        assert result.operator_name is None
        assert result.parsed_fields["direction"] == "sent"
        assert result.parsed_fields["amount"] == 50.0
        assert result.parsed_fields["balance_after"] == 13654.47
        assert result.parsed_fields["transaction_cost"] == 0.0
        assert result.external_ref == "UH1B91JUJI"

    def test_airtime_lowercase_confirmed_still_matches(self):
        # The real sample's own wording -- "confirmed." not "Confirmed." --
        # unlike every other real M-Pesa sample in this module.
        assert "confirmed." in MPESA_AIRTIME and "Confirmed." not in MPESA_AIRTIME
        assert parse_message(MPESA_AIRTIME).status == "parsed_unmapped"

    def test_both_new_shapes_reasons_mention_pocket(self):
        for text in (MPESA_PAYBILL_SENT, MPESA_AIRTIME):
            result = parse_message(text)
            assert "pocket" in result.reason.lower()

    def test_neither_new_shape_is_flagged_as_sensitive(self):
        for text in (MPESA_PAYBILL_SENT, MPESA_AIRTIME):
            assert parse_message(text).status != "rejected"


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


class TestExpandedSecretPatterns:
    """Three real secret shapes Bonnie's actual KCB thread showed that the
    original 5-pattern filter missed entirely -- must never be parsed,
    stored, or forwarded, exactly like the original OTP case."""

    def test_tan_code_is_rejected(self):
        assert parse_message(TAN_CODE_MESSAGE).status == "rejected"

    def test_activation_code_is_rejected(self):
        assert parse_message(ACTIVATION_CODE_MESSAGE).status == "rejected"

    def test_card_secret_pin_is_rejected(self):
        assert parse_message(CARD_SECRET_PIN_MESSAGE).status == "rejected"

    def test_none_of_the_three_leak_parsed_fields(self):
        for text in (TAN_CODE_MESSAGE, ACTIVATION_CODE_MESSAGE, CARD_SECRET_PIN_MESSAGE):
            result = parse_message(text)
            assert result.parsed_fields == {}
            assert result.operator_name is None

    def test_false_positive_check_against_every_real_transaction_fixture(self):
        # None of the five NEW patterns (tan code / activation code / secret
        # pin / "PIN is" / "code is valid") may ever fire on genuine
        # transaction vocabulary -- checked against every real fixture in
        # this module, not just a couple of samples.
        real_transaction_texts = (
            RECEIVED, PAYBILL, BUYGOODS, SENT, WITHDRAW,
            KCB_RECEIVE, KCB_CARD_KES, KCB_CARD_USD, KCB_CARD_HELPPAY,
            KCB_LOAN_DISBURSED, KCB_LOAN_REPAY, KCB_VOOMA_LOAN_REPAY,
            KCB_MPESA_PAYBILL, KCB_MPESA_ACCOUNT,
            KCB_MPESA_RECEIVED, KCB_MPESA_SENT, KCB_MPESA_TRANSFERRED,
            KCB_BALANCE, KCB_LOAN_OVERDUE, KCB_LOAN_ARREARS,
            KCB_LOAN_DEFAULT, KCB_LOAN_DUE_TODAY,
        )
        for text in real_transaction_texts:
            result = parse_message(text)
            assert result.status != "rejected", f"false positive on: {text!r}"


# ── KCB<->M-Pesa-network shapes — real text, source CONFIRMED as kcb ───────
# (1 Aug 2026) These read like a Safaricom M-Pesa "sent to" confirmation but
# arrive from the KCB sender id on Bonnie's real device -- classification is
# by sender, never by wording, so they're parsed by _parse_kcb, not
# _parse_mpesa. See TestKcbMpesaShapes below for the other three confirmed
# KCB<->M-Pesa-network shapes.

class TestKcbMpesaPaybillAndAccount:
    def test_paybill_to_kcb_is_mapped_as_received_income(self):
        # Real bug found + fixed against Bonnie's own paybill sample (2 Aug
        # 2026): despite "sent to KCB Pay Bill... has been received", this
        # is money credited INTO the account this SMS is about -- direction
        # is "received", not "sent", and it's unambiguous the same way
        # kcb_mpesa_received is, so it's mapped to budget.record_income too.
        result = parse_message(KCB_MPESA_PAYBILL)
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.parser_name == "kcb_mpesa_paybill"
        assert result.parsed_fields["direction"] == "received"
        assert result.parsed_fields["amount"] == 15452.00
        assert result.parsed_fields["account"] == "121***2684"
        assert result.external_ref == "UETB9611ZB"

    def test_kcb_account_is_mapped_as_received_income(self):
        result = parse_message(KCB_MPESA_ACCOUNT)
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.parser_name == "kcb_mpesa_account"
        assert result.parsed_fields["direction"] == "received"
        assert result.parsed_fields["amount"] == 700.00
        assert result.parsed_fields["counterparty"] == "GRACE NJERI OTIENO"
        assert result.external_ref == "UCVB9B85C2"

    def test_both_shapes_are_parsed_by_the_kcb_parser_not_mpesa(self):
        # The whole point of this reclassification: source is sender-based,
        # not wording-based, even though both shapes say "M-PESA" in the text.
        for text in (KCB_MPESA_PAYBILL, KCB_MPESA_ACCOUNT):
            assert parse_message(text).parser_name.startswith("kcb_")


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

    def test_helppay_card_transaction_merchant_with_hash_and_url_shape(self):
        # A second real sample -- confirms the merchant capture handles a
        # URL-shaped counterparty containing "#" with no special-casing.
        result = parse_message(KCB_CARD_HELPPAY)
        assert result.status == "parsed_unmapped"
        assert result.operator_name is None
        assert result.parsed_fields["amount"] == 1500.00
        assert result.parsed_fields["currency"] == "KES"
        assert result.parsed_fields["counterparty"] == "g.co/helppay#"
        assert result.parsed_fields["balance_after"] == 57555.00


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


class TestKcbMpesaShapes:
    """Formerly 'ambiguous' -- Bonnie's own earlier framing was that these
    might be Vooma/KCB-app relays of M-Pesa activity, origin sender
    unconfirmed. Bonnie has since checked his real device directly (1 Aug
    2026): all three genuinely arrive from the KCB sender id. Confirmed,
    not guessed -- kcb_mpesa_* parser names reflect that."""

    def test_received_is_mapped(self):
        result = parse_message(KCB_MPESA_RECEIVED)
        assert result.status == "mapped"
        assert result.operator_name == "budget.record_income"
        assert result.operator_params["amount"] == 1050.0
        assert result.parser_name == "kcb_mpesa_received"
        assert result.external_ref == "TF2987NE4D"

    def test_sent_to_mpesa_is_parsed_unmapped(self):
        result = parse_message(KCB_MPESA_SENT)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["amount"] == 500.00
        assert result.parsed_fields["transaction_cost"] == 11.90
        assert result.external_ref == "CHT4C7F3H0"
        assert result.parser_name == "kcb_mpesa_sent"

    def test_transferred_is_parsed_unmapped(self):
        result = parse_message(KCB_MPESA_TRANSFERRED)
        assert result.status == "parsed_unmapped"
        assert result.parsed_fields["amount"] == 14100.00
        assert result.parsed_fields["counterparty"] == "GRACE NJERI OTIENO"
        assert result.external_ref == "UESB95XFAB"
        assert result.parser_name == "kcb_mpesa_transferred"

    def test_all_five_kcb_mpesa_shapes_confirmed_by_sender_not_wording(self):
        # The reclassification's core property: every shape that mentions
        # "M-PESA" in its own text but is confirmed to arrive from the KCB
        # sender id parses under kcb_*, never mpesa_* -- classification is
        # by sender, never by content.
        for text in (
            KCB_MPESA_PAYBILL, KCB_MPESA_ACCOUNT,
            KCB_MPESA_RECEIVED, KCB_MPESA_SENT, KCB_MPESA_TRANSFERRED,
        ):
            assert parse_message(text).parser_name.startswith("kcb_mpesa_")


class TestKCBBalanceIsInformationalOnly:
    def test_balance_is_informational_not_mapped(self):
        # Was "parsed_unmapped" -- fixed 2 Aug 2026, since that tier
        # surfaces as a needs_attention classify card, and a plain balance
        # inquiry has nothing for a human to decide.
        result = parse_message(KCB_BALANCE)
        assert result.status == "informational"
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
        # Was "parsed_unmapped" -- fixed 2 Aug 2026, same reasoning as the
        # balance-inquiry fix above.
        result = parse_message(KCB_LOAN_OVERDUE)
        assert result.status == "informational"
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


class TestSourceStrictParsing:
    """FIX (2 Aug 2026): source_id, decided strictly by SMS sender on the
    Android capture client, now GATES which parser set even runs -- body
    content can never again promote a message out of its own sender's
    parser set. Before this, parse_message() always tried every mpesa_*
    regex first for every message regardless of source_id (source_id was
    pure display metadata, never actually used to scope parsing)."""

    def test_kcb_sourced_kcb_shaped_message_still_parses_as_kcb(self):
        result = parse_message(KCB_RECEIVE, source_id="kcb")
        assert result.status == "mapped"
        assert result.parser_name == "kcb_receive"

    def test_mpesa_sourced_mpesa_shaped_message_still_parses_as_mpesa(self):
        result = parse_message(RECEIVED, source_id="mpesa")
        assert result.status == "mapped"
        assert result.parser_name == "mpesa_received"

    def test_a_kcb_sourced_message_saying_mpesa_still_uses_only_kcb_parsers(self):
        # The exact real shape this fix targets: a KCB-sender message whose
        # own body says "M-PESA ref ..." must never be tried against the
        # mpesa_* parser set at all -- only kcb_* ever gets a look at it.
        result = parse_message(KCB_MPESA_PAYBILL, source_id="kcb")
        assert result.parser_name.startswith("kcb_")

    def test_mpesa_sourced_message_is_never_tried_against_kcb_parsers(self):
        # Symmetric protection -- a genuine Safaricom mpesa-sender message
        # must never be tried against kcb_* parsers either, even if it
        # somehow structurally resembled one.
        result = parse_message(RECEIVED, source_id="mpesa")
        assert not result.parser_name.startswith("kcb_")

    def test_unknown_source_falls_back_to_trying_everything(self):
        # An honest degrade, not a refusal -- a source this codebase
        # doesn't know about yet still gets a real attempt at every parser.
        result = parse_message(RECEIVED, source_id="some-future-bank")
        assert result.status == "mapped"
        assert result.parser_name == "mpesa_received"

    def test_no_source_id_falls_back_to_trying_everything(self):
        # Backward compatible -- every existing call site/test that omits
        # source_id keeps the original "try every parser" behaviour.
        result = parse_message(KCB_RECEIVE)
        assert result.status == "mapped"
        assert result.parser_name == "kcb_receive"

    def test_source_id_is_case_insensitive(self):
        result = parse_message(KCB_RECEIVE, source_id="KCB")
        assert result.parser_name == "kcb_receive"


# Bonnie's exact real sample (2 Aug 2026) -- an M-Pesa system/error notice,
# not a transaction. No personal info in the text itself, safe to use
# verbatim (unlike a real transaction sample, which would need names/amounts
# redacted).
MPESA_SYSTEM_BUSY = (
    "M-PESA is unable to process your request because a similar transaction is currently "
    "underway. Please wait while we complete your initial request."
)


class TestInformationalSystemMessages:
    """M-Pesa/KCB system/error notices -- recognised, but genuinely not a
    transaction. Fixed 2 Aug 2026: these used to fall through to
    "unparsed" (no amount, no counterparty, no shape any transaction
    parser recognises) and surfaced as a classify card demanding a pocket
    decision for something that never happened to any money."""

    def test_mpesa_busy_notice_is_informational_not_unparsed(self):
        result = parse_message(MPESA_SYSTEM_BUSY, source_id="mpesa")
        assert result.status == "informational"
        assert result.operator_name is None
        assert result.parsed_fields["direction"] == "none"
        assert result.parser_name == "mpesa_system_notice"

    def test_kcb_sender_with_the_same_wording_is_also_informational(self):
        # Source-strict parsing (transducer.py's own _PARSERS_BY_SOURCE)
        # means the identical wording is checked against the KCB parser
        # set when tagged kcb -- confirms the informational fallback isn't
        # only wired into the mpesa side.
        result = parse_message(MPESA_SYSTEM_BUSY, source_id="kcb")
        assert result.status == "informational"
        assert result.parser_name == "kcb_system_notice"

    def test_common_busy_error_phrasings_are_all_caught(self):
        samples = [
            "Sorry, the service is currently unavailable. Please try again later.",
            "Your request has timed out. Please try again.",
            "System is currently busy, please try again after a few minutes.",
        ]
        for text in samples:
            result = parse_message(text, source_id="mpesa")
            assert result.status == "informational", f"expected informational for: {text!r}"

    def test_a_real_transaction_is_never_shadowed_by_the_informational_fallback(self):
        # The fallback only runs AFTER every real transaction regex has
        # already had a chance to match -- a genuine transaction, even one
        # that happens to mention unrelated words, still parses normally.
        result = parse_message(RECEIVED, source_id="mpesa")
        assert result.status == "mapped"

    def test_otp_guard_still_wins_even_over_informational_wording(self):
        # The security gate is checked FIRST in parse_message(), before any
        # parser -- an OTP-like message must never fall through to
        # "informational" just because it also contains busy/error wording.
        result = parse_message(
            "Your OTP is 483920, please try again if it expires. Do not share this code.",
            source_id="mpesa",
        )
        assert result.status == "rejected"


# Bonnie's exact real sample (2 Aug 2026, round 2) -- a genuine M-Pesa
# failure notice that reached the classify card asking "which pocket does
# this belong to?" because the round-1 informational filter didn't yet
# recognise failure/rejection wording (only busy/retry/timeout).
MPESA_TILL_FAILURE = (
    "Failed. The till number entered is incorrect. Kindly enter the correct "
    "Till Number and try again."
)


class TestInformationalFailureAndErrorNotices:
    """Round 2 of the informational filter (2 Aug 2026): failure/rejection
    notices, not just busy/retry ones. Same fallback-only discipline --
    checked LAST, after every real transaction-shape regex has already had
    its chance, so these can never shadow a genuine transaction."""

    def test_the_exact_reported_till_failure_is_informational(self):
        result = parse_message(MPESA_TILL_FAILURE, source_id="mpesa")
        assert result.status == "informational"
        assert result.operator_name is None
        assert result.parsed_fields["direction"] == "none"
        assert result.parser_name == "mpesa_system_notice"

    def test_the_same_failure_is_informational_under_kcb_source_too(self):
        result = parse_message(MPESA_TILL_FAILURE, source_id="kcb")
        assert result.status == "informational"
        assert result.parser_name == "kcb_system_notice"

    def test_common_failure_and_rejection_phrasings_are_all_caught(self):
        samples = [
            "Failed. The paybill number entered is incorrect. Please try again.",
            "Your transaction has failed. Please try again.",
            "Wrong PIN entered. Please try again.",
            "The PIN you entered is incorrect.",
            "Invalid account number. Please check and try again.",
            "This number is not registered on M-PESA.",
            "Your transaction was not successful.",
            "Your payment has been declined.",
            "Your transaction has been cancelled.",
            "Sorry, your request could not be completed at this time.",
            "Sorry, you do not have enough money to complete this transaction.",
            "Insufficient funds to complete this transaction.",
            "This transaction exceeds your daily limit.",
        ]
        for text in samples:
            result = parse_message(text, source_id="mpesa")
            assert result.status == "informational", f"expected informational for: {text!r} (got {result.status})"

    def test_reversal_wording_is_deliberately_NOT_caught(self):
        # Explicitly excluded per the research that informed this pattern
        # set: a genuine reversal credits money BACK into the account (a
        # real state change), so guessing at reversal wording risks
        # silently swallowing a real credit. This must fall through to
        # unparsed (visible, needing a human), never informational.
        result = parse_message("Ksh500.00 has been reversed to your account. New M-PESA balance is Ksh2,000.00.", source_id="mpesa")
        assert result.status != "informational"

    def test_a_real_completed_transaction_is_never_caught_by_failure_wording(self):
        # None of the new patterns can shadow a real "Confirmed" message --
        # they only run as the last fallback after every transaction-shape
        # regex has already failed to match.
        received = parse_message(RECEIVED, source_id="mpesa")
        assert received.status == "mapped"
        buygoods = parse_message(BUYGOODS, source_id="mpesa")
        assert buygoods.status == "parsed_unmapped"


class TestMoneyStillMovedOverride:
    """Adversarial verify (2 Aug 2026) live-confirmed two real messages where
    a Round 2 failure/rejection pattern would otherwise swallow a genuine
    money-movement event into 'informational' instead of the safe
    'unparsed' fallback. Both must now fall through to unparsed -- surfaced
    to a human, never silently dropped -- regardless of which pattern would
    otherwise have matched."""

    def test_fuliza_overdraft_topup_is_not_swallowed_by_insufficient_funds(self):
        text = (
            "Fuliza M-PESA amount is Ksh500.00. Interest charged Ksh6.00. "
            "You had insufficient funds; Fuliza M-PESA has topped up your "
            "transaction. Available Fuliza limit is Ksh4,494.00."
        )
        result = parse_message(text, source_id="mpesa")
        assert result.status != "informational"

    def test_failure_plus_reversal_hybrid_is_not_swallowed_by_failed_pattern(self):
        text = (
            "Your transaction of Ksh2,000.00 failed. The amount has been "
            "reversed to your M-PESA account. New balance is Ksh5,000.00."
        )
        result = parse_message(text, source_id="mpesa")
        assert result.status != "informational"

    def test_refunded_wording_also_overrides(self):
        text = "Your payment has been declined. The amount has been refunded to your account."
        result = parse_message(text, source_id="mpesa")
        assert result.status != "informational"

    def test_override_does_not_affect_genuine_failure_notices_with_no_money_words(self):
        # Sanity check the override is scoped -- it must not blanket-disable
        # the whole informational filter, only the specific hybrid case.
        result = parse_message(MPESA_TILL_FAILURE, source_id="mpesa")
        assert result.status == "informational"
