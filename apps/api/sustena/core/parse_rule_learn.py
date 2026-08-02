"""
sustena/core/parse_rule_learn.py

"User fixes once -> it learns" (2 Aug 2026): the human-correction path
made real, turning Phase 3 groundwork into the actual feature. When a
person manually classifies a captured message that no declared rule (or
Python fallback parser) recognised, this module turns their OWN
just-confirmed correction into a candidate ParseRule -- so a message
repeating the same template (a recurring bill, a regular merchant)
auto-recognises next time instead of asking again.

THIS IS NOT AN LLM AND IS NOT THE §4G.7 PROPOSER. That module
(parse_rule_proposer.py) is explicit that no generative AI is wired --
this module is the OTHER, deliberately narrower thing: deterministic
TEMPLATE synthesis from exactly one human-confirmed example. It
parameterises exactly two things in the raw message text --
  1. a leading alphanumeric reference code, if the message has one (the
     standard bank/telco confirmation-SMS convention -- every migrated
     seed rule already extracts this as "ref"), and
  2. the CONFIRMED amount's own literal formatted substring (e.g.
     "1,500.00" or "500") --
everything else in the message is kept as a literal, regex-escaped
anchor. The resulting rule therefore matches a FUTURE message that
repeats the identical surrounding wording with a different amount (and
reference code) -- genuinely useful for a recurring bill/merchant -- but
will NOT match a message with even slightly different wording. That is a
disclosed, deliberate limit, not a bug: broader natural-language
generalisation is exactly the "opaque weight pretending to be a rule"
this project's own no-fake-learning discipline (see
parse_rule_proposer.py's own docstring) declines to fake. If the
confirmed amount's text can't be located anywhere in the raw message,
synthesize_rule_from_correction() returns None rather than manufacture a
useless rule that could only ever match this one exact byte-for-byte
message.

Money-safety asymmetry, matching the EXISTING shipped seed rules exactly
(not a new policy invented here): every migrated "received" shape is
status="mapped" (auto-applies, no human tap -- money coming IN is already
treated as safe/unambiguous by this codebase); every migrated "sent" /
"paid" / "withdrawn" shape is status="parsed_unmapped" (still needs a
human's confirm tap, however fast/pre-filled). A LEARNED rule follows the
identical rule: if the human's correction confirmed budget.record_income,
the learned rule is "mapped" and will genuinely auto-book next time. Any
other operator (budget.spend, budget.allocate, ...) produces a
"parsed_unmapped" learned rule -- the shape is recognised (a real
improvement over "no registered parser recognised this message's shape"),
and combined with the already-existing capture_classification_history
(per-counterparty pocket memory), the classify card shows up pre-filled
-- but a human still taps CONFIRM before any outbound money moves. This
is deliberate: "auto-parse without asking" for income, "fast to confirm,
never silent" for anything that spends real money.
"""

from __future__ import annotations

import re
import uuid
from typing import Any

from sustena.core.parse_rule import FieldSpec, ParseRule


def _amount_text_candidates(amount: float) -> list[str]:
    """
    Plausible textual representations of a confirmed float amount as it
    might literally appear in the raw SMS -- which carries the ORIGINAL
    formatting (thousands separators, trailing .00), not the parsed float
    repr. Tried in order; the first one found in the raw text wins.
    """
    candidates: list[str] = []
    if amount == int(amount):
        i = int(amount)
        candidates += [f"{i:,}.00", f"{i:,}", f"{i}.00", f"{i}"]
    else:
        candidates += [f"{amount:,.2f}", f"{amount:.2f}"]
    seen: set[str] = set()
    out: list[str] = []
    for c in candidates:
        if c not in seen:
            seen.add(c)
            out.append(c)
    return out


_LEADING_REF_RE = re.compile(r"^([A-Z0-9]{8,12})\b")


def synthesize_rule_from_correction(
    source: str, raw_text: str, operator: str, params: dict[str, Any],
) -> ParseRule | None:
    """
    Build a candidate ParseRule from one human-confirmed correction.
    Returns None (never a useless rule) if the confirmed amount's own
    text can't be located anywhere in raw_text -- see module docstring.
    """
    amount = params.get("amount")
    if not isinstance(amount, (int, float)) or isinstance(amount, bool):
        return None  # nothing to anchor a generalisable rule on

    escaped = re.escape(raw_text)
    extract: dict[str, FieldSpec] = {}

    ref_match = _LEADING_REF_RE.match(raw_text)
    if ref_match:
        escaped_ref = re.escape(ref_match.group(1))
        if escaped_ref in escaped:
            escaped = escaped.replace(escaped_ref, r"(?P<ref>[A-Z0-9]{8,12})", 1)
            extract["ref"] = FieldSpec(type="string", group="ref")

    amount_found = False
    for candidate_text in _amount_text_candidates(float(amount)):
        escaped_candidate = re.escape(candidate_text)
        if escaped_candidate in escaped:
            escaped = escaped.replace(escaped_candidate, r"(?P<amount>[\d,]+\.?\d*)", 1)
            extract["amount"] = FieldSpec(type="amount", group="amount")
            amount_found = True
            break
    if not amount_found:
        return None

    rule_id = f"learned_{source}_{uuid.uuid4().hex[:12]}"
    is_income = operator == "budget.record_income"

    rule_params: dict[str, Any] = {}
    if is_income:
        for key, val in params.items():
            if (
                key == "amount" and isinstance(val, (int, float))
                and not isinstance(val, bool) and float(val) == float(amount)
            ):
                rule_params[key] = "$amount"
            else:
                rule_params[key] = val

    return ParseRule(
        id=rule_id, source=source, version=1, pattern=escaped, extract=extract,
        status="mapped" if is_income else "parsed_unmapped",
        operator=operator if is_income else None,
        params=rule_params if is_income else {},
        reason_template=(
            None if is_income else
            "Recognised from a message you classified before — still needs a pocket/operator decision."
        ),
        trust="user_corrected", provenance="human_correction",
        examples=(raw_text,),
    )
