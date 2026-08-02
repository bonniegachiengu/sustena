"""
sustena/core/parse_rule_proposer.py

Phase 3D (Parser primitive-lift, 2 Aug 2026) — the proposer/verifier for
the long tail. Canon: SPEC-parser-primitive-lift.addendum.2026-07-29.md
§4G.7 ("parse-rule proposal: a third generate-and-verify target").

§4G.7's own model: "LLMs are a proposal distribution g(π|context), with
simulation as verifier — generate-and-test. Nothing reaches the world on
fluency alone." Applied to rules: a proposer suggests a candidate
ParseRule for an unparsed message; V(r) — TWO conditions, both required —
decides whether it's even offered to a human; only an explicit human
confirm crystallises it into D via Phase 3C's add_parse_rule/
modify_parse_rule. The proposer can never write on its own.

HONEST SCOPE, stated plainly rather than glossed over: this module builds
the verifier V(r) and the propose -> verify -> confirm WIRING in full —
that machinery is real, tested, and complete. It does NOT wire a genuine
LLM call to actually GENERATE a candidate rule from a novel unparsed
message. Every other operative in this codebase (Mentor, Protégé, ...)
gets away with zero real API spend because their reasoning is a graph of
plain operators — there was never a "come up with a brand-new regex from
one example" problem to fake. Synthesizing a genuinely useful rule from a
single message is a real generation problem; building a convincing-looking
placeholder for it here would be exactly the "opaque weight pretending to
be a rule" this project's own "no fake-learning" discipline explicitly
rejects (see AddRule/ModifyRule's own docstrings). So: no generator is
wired by default. propose_and_verify() takes an explicit `generator`
callable; passing none (the default) returns a clear, honest "no
candidate" result — never a fabricated one. Wiring a real generator
(respecting ANTHROPIC_API_KEY=mock in dev, a genuine API call only in
real deployment) is disclosed, scoped follow-up work, not attempted here.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable

from sustena.core.parse_rule import ParseRule, apply_rule, typecheck_rule


@dataclass
class ProposedRule:
    candidate: ParseRule
    inducing_example: str
    status: str                       # "verified" | "rejected"
    reasons: list[str] = field(default_factory=list)   # why rejected, if rejected — empty when verified


def verify_proposed_rule(
    candidate: ParseRule, inducing_example: str, existing_rules: list[ParseRule],
) -> tuple[bool, list[str]]:
    """
    V(r) — two conditions, BOTH required, per §4G.7:

      1. Inducing-example fidelity: Gamma |- r holds, and applying r to
         the message that induced it actually matches and produces a
         result (a rule that doesn't even recognise its own source
         example is useless by construction).
      2. Stored-message regression: r must not mis-fire against messages
         the existing rule set already handles correctly. Checked here
         against every existing rule's own declared examples[] (the same
         narrower-but-real corpus ModifyRule's migration check uses,
         disclosed there too) — a genuinely broader live-corpus scan
         is the same disclosed, separate follow-up noted in
         SustainEngine.modify_parse_rule's own docstring.

    Returns (True, []) only if BOTH pass. A rule failing either is
    refused before a human ever sees it — the §4H.1 "schema disposes"
    posture, applied to rules instead of views.
    """
    type_errors = typecheck_rule(candidate)
    if type_errors:
        # Short-circuit — an ill-typed rule (e.g. an invalid regex) can't
        # safely be run at all; apply_rule() assumes typecheck_rule already
        # passed and would raise (not refuse) on a genuinely broken pattern.
        return False, [f"Gamma |- r failed: {e}" for e in type_errors]

    reasons: list[str] = []

    fidelity_result = apply_rule(candidate, inducing_example)
    if fidelity_result is None:
        reasons.append("candidate rule does not match its own inducing example")
    elif fidelity_result.parser_name != candidate.id:
        reasons.append("candidate rule matched but resolved to a different rule id — inconsistent state")

    for rule in existing_rules:
        for example in rule.examples:
            if example == inducing_example:
                continue  # the inducing example itself is handled above, not a "stored" collision
            collision = apply_rule(candidate, example)
            if collision is not None:
                reasons.append(
                    f"candidate would intercept '{rule.id}'s own example ({example!r}), "
                    f"which the existing rule set already handles correctly"
                )

    return (len(reasons) == 0, reasons)


def propose_and_verify(
    raw_text: str,
    existing_rules: list[ParseRule],
    generator: Callable[[str], ParseRule | None] | None = None,
) -> ProposedRule | None:
    """
    The full propose -> verify pipeline. `generator` is the (currently
    unwired-by-default, see module docstring) proposal distribution
    g(r | raw, S, T) — a callable taking the raw message and returning a
    single candidate ParseRule, or None if it has nothing to propose.

    Returns None (not a fabricated proposal) when generator is None or
    the generator itself returns None. Returns a ProposedRule with
    status="verified" or "rejected" (reasons populated) when a generator
    did produce a candidate — verified is required before a route ever
    offers it to a human for confirmation.
    """
    if generator is None:
        return None
    candidate = generator(raw_text)
    if candidate is None:
        return None
    ok, reasons = verify_proposed_rule(candidate, raw_text, existing_rules)
    return ProposedRule(
        candidate=candidate, inducing_example=raw_text,
        status="verified" if ok else "rejected", reasons=reasons,
    )
