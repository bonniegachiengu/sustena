"""
Transducer vector generation — R1 PARITY, recorded from the reference.

Drives the reference's real `parse_message` over:

  - every shipped rule's OWN declared examples, under its own source
    (the real M-Pesa and KCB samples, tuned against Bonnie's real device),
  - the same texts under the WRONG source (source-strict: a body must never
    promote a message out of its sender's set),
  - real OTP / verification-code shapes (the security pre-gate),
  - an empty message.

For each case the recorded expectation is the whole decision: the tier, the
operator and params if it mapped, every parsed field, the external ref, and
which parser handled it. A transducer that got the tier right while extracting
a different amount would pass a tier-only check and be a wrong ledger entry.
"""

from __future__ import annotations

import sustena.operators  # noqa: F401 -- importing registers every operator
from sustena.core.parse_rules_seed import SEED_RULES_BY_SOURCE
from sustena.core.transducer import parse_message

# ★ Real OTP / secret shapes. The digits are placeholders — the SHAPE is what
#   is being recorded, and a real code must never live in a repo.
SECRET_SAMPLES = [
    "Your OTP is 483920. Do not share it with anyone.",
    "483920 is your verification code for KCB.",
    "Use one-time pin 112233 to authorise this transaction.",
    "Your TAN code is 55123.",
    "Activation code: 9931. This code is valid for only 90s.",
    "Your KCB card secret PIN is 4417.",
    "Security code 8821 for your transaction.",
]


def _case(name, text, source):
    r = parse_message(text, source)
    return {
        "name": name,
        "source": source,
        "text": text,
        "expect": {
            "status": r.status,
            "operator": r.operator_name,
            "params": r.operator_params,
            "parsed_fields": r.parsed_fields,
            "external_ref": r.external_ref,
            "parser_name": r.parser_name,
        },
    }


def build_cases() -> list[dict]:
    cases: list[dict] = []

    # 1 · every shipped rule, against its own declared examples.
    for source, rules in SEED_RULES_BY_SOURCE.items():
        for rule in rules:
            for i, example in enumerate(rule.examples):
                cases.append(_case(f"{rule.id}#{i}", example, source))

    # 2 · source-strict: the same texts under the OTHER source.
    others = {"mpesa": "kcb", "kcb": "mpesa"}
    for source, rules in SEED_RULES_BY_SOURCE.items():
        for rule in rules:
            if not rule.examples:
                continue
            cases.append(
                _case(f"strict:{rule.id}-as-{others[source]}", rule.examples[0], others[source])
            )

    # 3 · the security pre-gate. ★ Recorded under BOTH sources, because a real
    #     OTP arrives from the same sender id as a real confirmation.
    for i, text in enumerate(SECRET_SAMPLES):
        for source in ("mpesa", "kcb"):
            cases.append(_case(f"secret:{i}-{source}", text, source))

    # 4 · edges.
    cases.append(_case("empty", "   ", "mpesa"))
    cases.append(_case("gibberish", "hello there, nothing financial here", "mpesa"))

    return cases
