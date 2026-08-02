"""
sustena/core/parse_rule.py

Phase 3 (Parser primitive-lift, 2 Aug 2026) — the ParseRule primitive.
Canon: IO/strategy/SPEC-parser-primitive-lift.addendum.2026-07-29.md
(§§4K.6/4F.6/4I.6/4G.7) + IO/strategy/sustena-parser-primitive-lift.design.
2026-07-29.md.

transducer.py's tau (RawMessage -> Event | ParseFailure) used to be a set
of hand-registered Python functions (_parse_mpesa/_parse_kcb), each with
match logic AND field-mapping logic baked directly into the function body
-- editable only by a code deploy. This module makes the internal
structure of tau first-class: a ParseRule is a declared, typed, versioned
Library artifact with the same lifecycle as an operator or a widget, and
run_rules() is a small, generic interpreter that evaluates an ordered set
of them, replacing "call the next Python function" with "evaluate the
next declared rule."

r = <id, source, version, pattern, extract, status, operator, params,
     reason_template, trust, provenance, examples>

  pattern    : a regex string with named groups (RawMessage -> Bool via
               re.search; today's match, declared not coded).
  extract    : {field_name: FieldSpec} -- typed field capture
               (RawMessage -> parsed_fields).
  status     : "mapped" | "parsed_unmapped" | "informational" -- mirrors
               TransductionResult's own tiers (never "rejected" or
               "unparsed" -- rejected is the fixed security pre-gate,
               unparsed is "no rule matched," neither is a rule outcome).
  operator/params : for status="mapped" only -- a real operator name plus
               a binding from extracted fields ($field_name) or literals
               to that operator's real params. UNMAPPED is status !=
               "mapped" with operator=None -- a first-class, honest
               outcome, never a silent drop (parsed_unmapped/informational
               both still return a real TransductionResult).
  trust, provenance, examples : the connector's declared trust level and
               the rule's origin/regression corpus, per §4K.6.

Well-formedness -- Gamma |- r (typecheck_rule below), mirroring
§4F.1/§4H.1's existing judgment: a rule that maps to a nonexistent
operator, or binds a param that operator doesn't declare, is a compile-
time finding (typecheck_rule returns errors), never a silent runtime
"just don't match." Checked once when a rule is loaded/added, not on
every message.

Deliberately data-only, no callables: `extract`'s FieldSpec supports
exactly THREE field types (amount -- comma-stripped float; string --
stripped text; literal -- a fixed value) because that is what every
currently-migrated shape actually needs. A shape needing genuinely bespoke
post-processing (e.g. mpesa_withdraw's "Agent {name}" prefix transform)
is NOT force-fit into this schema -- it stays on the Python fallback tier
until either the schema gains a real, shared need for a 4th field type or
the transform turns out to be common enough to warrant one. Same
"additive, migrate what fits cleanly, keep the rest" discipline as every
version of this codebase's own migrations.

The security pre-gate (contains_sensitive_secret) is NOT a ParseRule and
is out of scope for this module entirely -- see transducer.py's own
docstring. No ParseRule can ever be authored, corrected, generated, or
retired to weaken it; it stays fixed code, checked before any rule runs.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class FieldSpec:
    """One extracted field's typed capture instruction.

    type="amount" | "string": `group` names the regex capture group to
    read; a group that legitimately didn't match (an optional trailing
    field like transaction_cost) is silently omitted from parsed_fields --
    the same "absent, not zero" discipline the hand-wired parsers already
    used for optional groups.
    type="literal": `value` is used verbatim, no regex group involved --
    for a fixed field a rule always carries regardless of match content
    (e.g. direction="received" on every mpesa_received hit).
    """
    type: str = "string"          # "amount" | "string" | "literal"
    group: str | None = None      # regex named group (amount/string types)
    value: Any = None             # fixed value (literal type)


@dataclass(frozen=True)
class ParseRule:
    id: str
    source: str                              # "mpesa" | "kcb" | ...
    version: int
    pattern: str                             # regex with named groups
    extract: dict[str, FieldSpec] = field(default_factory=dict)
    status: str = "parsed_unmapped"          # "mapped" | "parsed_unmapped" | "informational"
    operator: str | None = None              # required iff status == "mapped"
    params: dict[str, Any] = field(default_factory=dict)   # param_name -> "$field_name" or a literal
    flags: tuple[str, ...] = ()              # e.g. ("IGNORECASE", "DOTALL")
    reason_template: str | None = None       # a str.format() template over parsed_fields
    trust: str = "shipped"                   # "shipped" | "user_corrected" | "proposed_confirmed"
    provenance: str = "sustena_core"
    examples: tuple[str, ...] = ()


_ALLOWED_FIELD_TYPES = {"amount", "string", "literal"}
_ALLOWED_STATUSES = {"mapped", "parsed_unmapped", "informational"}
_ALLOWED_FLAGS = {"IGNORECASE", "DOTALL", "MULTILINE"}


def typecheck_rule(rule: ParseRule) -> list[str]:
    """
    Gamma |- r: every well-formedness check a rule must pass before it can
    be added to a running rule set. Returns a list of error strings (empty
    = well-typed). Never raises -- callers (AddRule/ModifyRule, the load-
    time seed check) decide what a non-empty result means for them.
    """
    errors: list[str] = []

    try:
        re.compile(rule.pattern)
    except re.error as exc:
        errors.append(f"invalid regex pattern: {exc}")

    for flag_name in rule.flags:
        if flag_name not in _ALLOWED_FLAGS:
            errors.append(f"unknown regex flag '{flag_name}'")

    if rule.status not in _ALLOWED_STATUSES:
        errors.append(f"unknown status '{rule.status}' (expected one of {sorted(_ALLOWED_STATUSES)})")

    for field_name, spec in rule.extract.items():
        if spec.type not in _ALLOWED_FIELD_TYPES:
            errors.append(f"extract field '{field_name}' has unknown type '{spec.type}'")
        elif spec.type in ("amount", "string") and not spec.group:
            errors.append(f"extract field '{field_name}' (type={spec.type}) requires a regex group name")
        elif spec.type == "literal" and spec.value is None:
            errors.append(f"extract field '{field_name}' (type=literal) requires a value")

    if rule.status == "mapped":
        # Import locally -- parse_rule.py must not create an import cycle
        # with operator.py at module-load time (operators/*.py import each
        # other lazily too; same discipline as effect_capture.py's own
        # build_params()).
        from sustena.core.operator import OPERATOR_REGISTRY

        if not rule.operator:
            errors.append("status='mapped' requires an operator")
        elif rule.operator not in OPERATOR_REGISTRY:
            errors.append(f"operator '{rule.operator}' is not registered in OPERATOR_REGISTRY")
        else:
            import inspect
            meta = OPERATOR_REGISTRY[rule.operator]
            real_params = {p for p in inspect.signature(meta.fn).parameters if p != "ctx"}
            for param_name in rule.params:
                if param_name not in real_params:
                    errors.append(
                        f"param '{param_name}' is not a real parameter of operator '{rule.operator}' "
                        f"(real params: {sorted(real_params)})"
                    )
    elif rule.operator or rule.params:
        errors.append(f"status='{rule.status}' must not declare operator/params (only 'mapped' rules do)")

    return errors


def _apply_extract(rule: ParseRule, m: re.Match) -> dict:
    fields: dict[str, Any] = {}
    for name, spec in rule.extract.items():
        if spec.type == "literal":
            fields[name] = spec.value
            continue
        raw = m.group(spec.group)
        if raw is None:
            continue  # an optional group that legitimately didn't match this time
        if spec.type == "amount":
            fields[name] = float(raw.replace(",", ""))
        else:
            fields[name] = raw.strip()
    return fields


def _bind_params(rule: ParseRule, fields: dict) -> dict:
    """
    beta_theta: parsed_fields -> theta. Three binding forms, in order:
      "$name"        -- a direct reference, preserves the field's real
                         type (a float stays a float) -- e.g. "$amount".
      "...{name}..." -- a str.format() template over fields, always
                         produces a string -- e.g. "M-Pesa: {name}" for
                         mpesa_received's operator_params["source"], which
                         the hand-wired parser built as f"M-Pesa: {name}".
      anything else  -- a literal value, used verbatim (e.g. "once").
    """
    bound: dict[str, Any] = {}
    for param_name, ref in rule.params.items():
        if isinstance(ref, str) and ref.startswith("$") and "{" not in ref:
            bound[param_name] = fields.get(ref[1:])
        elif isinstance(ref, str) and "{" in ref:
            bound[param_name] = ref.format(**fields)
        else:
            bound[param_name] = ref
    return bound


def _regex_flags(rule: ParseRule) -> int:
    value = 0
    for name in rule.flags:
        value |= getattr(re, name)
    return value


def apply_rule(rule: ParseRule, raw_text: str):
    """
    Try ONE rule against raw_text. Returns a TransductionResult if it
    matches, else None. Kept separate from run_rules() so a single rule
    can be evaluated in isolation (the §4G.7 proposal verifier's
    "inducing-example fidelity" check needs exactly this).
    """
    from sustena.core.transducer import TransductionResult  # local: avoid a module-load cycle (transducer imports this module)

    m = re.search(rule.pattern, raw_text, _regex_flags(rule))
    if not m:
        return None

    fields = _apply_extract(rule, m)
    reason = rule.reason_template.format(**fields) if rule.reason_template else ""

    if rule.status == "mapped":
        return TransductionResult(
            status="mapped",
            operator_name=rule.operator,
            operator_params=_bind_params(rule, fields),
            external_ref=fields.get("ref"),
            parsed_fields=fields,
            reason=reason,
            parser_name=rule.id,
        )
    if rule.status == "parsed_unmapped":
        return TransductionResult(
            status="parsed_unmapped",
            external_ref=fields.get("ref"),
            parsed_fields=fields,
            reason=reason,
            parser_name=rule.id,
        )
    # informational
    return TransductionResult(
        status="informational",
        parsed_fields=fields or {"direction": "none"},
        reason=reason,
        parser_name=rule.id,
    )


def run_rules(rules: list[ParseRule], raw_text: str):
    """
    tau_c(raw) = the first r in R_c with match(r, raw) -> apply(r, raw);
    else None (a ParseFailure at this tier -- the caller falls through to
    the Python parser fallback tier, then finally "unparsed"). Order-
    within-source is a declared property of the caller's rule LIST (list
    order = precedence), not comment-documented Python function order.
    """
    for rule in rules:
        result = apply_rule(rule, raw_text)
        if result is not None:
            return result
    return None
