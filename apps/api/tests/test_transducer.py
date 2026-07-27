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
