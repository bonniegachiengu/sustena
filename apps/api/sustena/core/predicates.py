"""
sustena/core/predicates.py

Typed predicate ASTs for Sustain invariants and Operator post_constraints.

Where sustena/core/constraints.py evaluates a constraint string directly by
tokenising and walking it once (no reusable structure, no schema binding),
this module first PARSES a constraint string into a typed, immutable AST
(StatePath / Comparison / Quantifier / Aggregate / ...), then:

  1. binds it against a sustain's declared `state_schema` — a predicate that
     references a state dimension the schema doesn't declare is a load-time
     error (validate_against_schema), not a silent runtime None-comparison.
  2. evaluates it against a concrete state (evaluate_predicate) — used by the
     SustainEngine's enforcing gate (Move 2) to check sustain invariants and
     operator post_constraints against the candidate post-effect state.

No eval()/exec()/compile() — hand-written tokenizer + recursive descent,
same discipline as constraints.py.

Grammar (matches the DSL actually used in sustena/sustains/*.json invariants,
which brackets the wildcard AFTER the path — `path[*].field` — rather than
the `[path].field` form shown in the older Sustena_DSL_Dot_Protocol.md
reference doc; the shipped specs are the contract this module follows):

    predicate    := or_expr
    or_expr      := and_expr ('OR' and_expr)*
    and_expr     := not_expr ('AND' not_expr)*
    not_expr     := 'NOT' not_expr | primary
    primary      := '(' predicate ')' | quantifier | comparison
    quantifier   := ('ALL'|'EXISTS') path '[' '*' ']' quant_tail
    quant_tail   := '.' name comp_op operand
                  | '.' name ['NOT'] 'IN' operand
                  | ':' predicate                    -- block form, scoped to item
    comparison   := operand comp_op operand
                  | operand ['NOT'] 'IN' operand
    comp_op      := '>' | '>=' | '<' | '<=' | '==' | '!='
    operand      := literal | aggregate | list_literal | path | paramref
    aggregate    := ('SUM'|'COUNT'|'AVG'|'MIN'|'MAX') '(' wildcard_path ')'
    path         := name ('.' name | '[' number ']')*          -- no wildcard
    wildcard_path:= name ('.' name | '[' '*' ']' | '[' number ']')*
    paramref     := 'params' '.' name
    list_literal := '[' (operand (',' operand)*)? ']'

Usage:
    node, errors = compile_invariant("finances.liquid.balance >= 0", state_schema)
    if errors:
        ...  # spec bug — report, don't enforce
    ok, reason = evaluate_predicate(node, state, params={})
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Union

from sustena.core.state import StateAccessor

_COMPARISON_OPS = ('>=', '<=', '==', '!=', '>', '<')
_AGGREGATE_FUNCS = ('SUM', 'COUNT', 'AVG', 'MIN', 'MAX')
_KEYWORDS = ('AND', 'OR', 'NOT', 'IN', 'ALL', 'EXISTS') + _AGGREGATE_FUNCS


class PredicateSyntaxError(Exception):
    """Raised when a predicate string cannot be parsed. Carries a token position."""

    def __init__(self, message: str, position: int | None = None):
        self.position = position
        suffix = f" (at token {position})" if position is not None else ""
        super().__init__(f"{message}{suffix}")


class PredicateSchemaError(Exception):
    """Raised (or collected) when a predicate references a state dimension the schema doesn't declare."""


# ── AST ────────────────────────────────────────────────────────────────────────
# All nodes are frozen/immutable — a compiled predicate is safe to cache and
# reuse across every call to the operator/sustain it belongs to.

@dataclass(frozen=True)
class Literal:
    value: Any


@dataclass(frozen=True)
class ParamRef:
    key: str


@dataclass(frozen=True)
class StatePath:
    raw: str
    segments: tuple  # each is ('name', key) | ('wildcard',) | ('index', int)


@dataclass(frozen=True)
class Aggregate:
    func: str  # SUM | COUNT | AVG | MIN | MAX
    path: StatePath  # must contain exactly one wildcard segment


@dataclass(frozen=True)
class ListLiteral:
    items: tuple  # tuple of Literal


@dataclass(frozen=True)
class Comparison:
    left: Any
    op: str
    right: Any


@dataclass(frozen=True)
class Membership:
    left: Any
    negate: bool
    right: Any  # ListLiteral | StatePath


@dataclass(frozen=True)
class LogicalAnd:
    parts: tuple


@dataclass(frozen=True)
class LogicalOr:
    parts: tuple


@dataclass(frozen=True)
class LogicalNot:
    operand: Any


@dataclass(frozen=True)
class Quantifier:
    kind: str  # ALL | EXISTS
    list_path: StatePath  # no wildcard — the container itself
    predicate: Any  # evaluated with each item as the scope root


PredicateNode = Union[
    Comparison, Membership, LogicalAnd, LogicalOr, LogicalNot, Quantifier
]


# ── Tokenizer ──────────────────────────────────────────────────────────────────

def _tokenize(expr: str) -> list[str]:
    tokens: list[str] = []
    i, n, s = 0, len(expr), expr.strip()
    n = len(s)
    while i < n:
        c = s[i]
        if c.isspace() or c == ',':
            i += 1
            continue
        if c in ('"', "'"):
            quote = c
            j = i + 1
            while j < n:
                if s[j] == '\\':
                    j += 2
                    continue
                if s[j] == quote:
                    break
                j += 1
            tokens.append(s[i:j + 1])
            i = j + 1
            continue
        if c in ('[', ']', '(', ')', ':', '*'):
            tokens.append(c)
            i += 1
            continue
        if c in ('>', '<', '!', '='):
            if i + 1 < n and s[i + 1] == '=':
                tokens.append(s[i:i + 2])
                i += 2
            else:
                tokens.append(c)
                i += 1
            continue
        if c.isdigit():
            j = i
            while j < n and (s[j].isdigit() or s[j] == '.'):
                j += 1
            tokens.append(s[i:j])
            i = j
            continue
        if c.isalpha() or c == '_':
            j = i
            while j < n and (s[j].isalnum() or s[j] in ('.', '_')):
                j += 1
            tokens.append(s[i:j])
            i = j
            continue
        # Unrecognised char (e.g. a stray leading '.' after ']') — skip.
        i += 1
    return tokens


def _split_path_token(token: str) -> list[str]:
    """'finances.pockets' -> ['finances', 'pockets']. A single name has no dots."""
    return token.split('.')


# ── Parser ─────────────────────────────────────────────────────────────────────

class _Parser:
    def __init__(self, tokens: list[str]):
        self.tokens = tokens
        self.pos = 0

    def _peek(self) -> str | None:
        return self.tokens[self.pos] if self.pos < len(self.tokens) else None

    def _peek_upper(self) -> str | None:
        t = self._peek()
        return t.upper() if t is not None else None

    def _advance(self) -> str:
        if self.pos >= len(self.tokens):
            raise PredicateSyntaxError("Unexpected end of expression", self.pos)
        t = self.tokens[self.pos]
        self.pos += 1
        return t

    def _expect(self, token: str) -> str:
        t = self._advance()
        if t.upper() != token.upper():
            raise PredicateSyntaxError(f"Expected '{token}' but got '{t}'", self.pos - 1)
        return t

    def parse(self) -> Any:
        node = self._parse_or()
        if self.pos != len(self.tokens):
            raise PredicateSyntaxError(f"Unexpected trailing token '{self._peek()}'", self.pos)
        return node

    def _parse_or(self) -> Any:
        parts = [self._parse_and()]
        while self._peek_upper() == 'OR':
            self._advance()
            parts.append(self._parse_and())
        return parts[0] if len(parts) == 1 else LogicalOr(tuple(parts))

    def _parse_and(self) -> Any:
        parts = [self._parse_not()]
        while self._peek_upper() == 'AND':
            self._advance()
            parts.append(self._parse_not())
        return parts[0] if len(parts) == 1 else LogicalAnd(tuple(parts))

    def _parse_not(self) -> Any:
        if self._peek_upper() == 'NOT':
            self._advance()
            return LogicalNot(self._parse_not())
        return self._parse_primary()

    def _parse_primary(self) -> Any:
        if self._peek() == '(':
            self._advance()
            node = self._parse_or()
            self._expect(')')
            return node
        if self._peek_upper() in ('ALL', 'EXISTS'):
            return self._parse_quantifier()
        return self._parse_comparison()

    def _parse_quantifier(self) -> Quantifier:
        kind = self._advance().upper()
        list_path = self._parse_path(allow_wildcard=False, consume_brackets=False)
        self._expect('[')
        self._expect('*')
        self._expect(']')
        tail = self._peek()
        if tail == ':':
            self._advance()
            predicate = self._parse_or()
        elif tail == '.':  # pragma: no cover — dots are absorbed into identifiers by the tokenizer
            raise PredicateSyntaxError("Unexpected '.' — quant tail field must follow ']' directly", self.pos)
        else:
            field_tok = self._advance()
            field = StatePath(field_tok, tuple(('name', seg) for seg in _split_path_token(field_tok)))
            if self._peek_upper() == 'NOT' or self._peek_upper() == 'IN':
                negate = False
                if self._peek_upper() == 'NOT':
                    self._advance()
                    negate = True
                self._expect('IN')
                right = self._parse_membership_rhs()
                predicate = Membership(field, negate, right)
            else:
                op = self._parse_comp_op()
                operand = self._parse_operand()
                predicate = Comparison(field, op, operand)
        return Quantifier(kind, list_path, predicate)

    def _parse_comparison(self) -> Any:
        left = self._parse_operand()
        if self._peek_upper() == 'NOT' or self._peek_upper() == 'IN':
            negate = False
            if self._peek_upper() == 'NOT':
                self._advance()
                negate = True
            self._expect('IN')
            right = self._parse_membership_rhs()
            return Membership(left, negate, right)
        op = self._parse_comp_op()
        right = self._parse_operand()
        return Comparison(left, op, right)

    def _parse_comp_op(self) -> str:
        t = self._advance()
        if t not in _COMPARISON_OPS:
            raise PredicateSyntaxError(f"Expected a comparison operator, got '{t}'", self.pos - 1)
        return t

    def _parse_membership_rhs(self) -> Any:
        if self._peek() == '[':
            return self._parse_list_literal()
        return self._parse_path(allow_wildcard=False)

    def _parse_list_literal(self) -> ListLiteral:
        self._expect('[')
        items = []
        while self._peek() != ']':
            items.append(self._parse_literal_only())
        self._expect(']')
        return ListLiteral(tuple(items))

    def _parse_literal_only(self) -> Literal:
        tok = self._advance()
        return Literal(_coerce_literal(tok))

    def _parse_operand(self) -> Any:
        t = self._peek()
        if t is None:
            raise PredicateSyntaxError("Unexpected end of expression while reading an operand", self.pos)
        if t.upper() in _AGGREGATE_FUNCS:
            return self._parse_aggregate()
        if t == '[':
            return self._parse_list_literal()
        if _is_literal_token(t):
            self._advance()
            return Literal(_coerce_literal(t))
        if t.startswith('params.'):
            self._advance()
            return ParamRef(t[len('params.'):])
        return self._parse_path(allow_wildcard=True)

    def _parse_aggregate(self) -> Aggregate:
        func = self._advance().upper()
        self._expect('(')
        path = self._parse_path(allow_wildcard=True)
        self._expect(')')
        return Aggregate(func, path)

    def _parse_path(self, allow_wildcard: bool, consume_brackets: bool = True) -> StatePath:
        tok = self._advance()
        if tok.upper() in _KEYWORDS or tok in ('(', ')', '[', ']', ':'):
            raise PredicateSyntaxError(f"Expected a path, got '{tok}'", self.pos - 1)
        segments: list[tuple] = [('name', seg) for seg in _split_path_token(tok)]
        raw_parts = [tok]
        if not consume_brackets:
            return StatePath(''.join(raw_parts), tuple(segments))
        while self._peek() == '[':
            self._advance()
            inner = self._peek()
            if inner == '*':
                if not allow_wildcard:
                    raise PredicateSyntaxError("Wildcard '[*]' is not allowed here", self.pos)
                self._advance()
                self._expect(']')
                segments.append(('wildcard',))
                raw_parts.append('[*]')
            else:
                idx_tok = self._advance()
                if not idx_tok.isdigit():
                    raise PredicateSyntaxError(
                        f"Expected a numeric index or '*' inside '[]', got '{idx_tok}'", self.pos - 1
                    )
                self._expect(']')
                segments.append(('index', int(idx_tok)))
                raw_parts.append(f'[{idx_tok}]')
            # A field continuation right after ']', e.g. '[*].allocated'
            nxt = self._peek()
            if nxt is not None and not nxt.upper() in _KEYWORDS and nxt not in (
                '(', ')', '[', ']', ':', '>', '<', '=', '!'
            ):
                cont = self._advance()
                segments.extend(('name', seg) for seg in _split_path_token(cont))
                raw_parts.append('.' + cont)
        return StatePath(''.join(raw_parts), tuple(segments))


def _is_literal_token(tok: str) -> bool:
    if len(tok) >= 2 and tok[0] in ('"', "'") and tok[-1] == tok[0]:
        return True
    if tok.lower() in ('true', 'false', 'null', 'none'):
        return True
    try:
        float(tok)
        return True
    except ValueError:
        return False


def _coerce_literal(tok: str) -> Any:
    if len(tok) >= 2 and tok[0] in ('"', "'") and tok[-1] == tok[0]:
        return tok[1:-1]
    lower = tok.lower()
    if lower == 'true':
        return True
    if lower == 'false':
        return False
    if lower in ('null', 'none'):
        return None
    try:
        return int(tok)
    except ValueError:
        pass
    try:
        return float(tok)
    except ValueError:
        pass
    return tok  # bare identifier used as a literal (e.g. an unquoted enum value)


def parse_predicate(expr: str) -> Any:
    """Parse a constraint DSL string into a typed predicate AST. Raises PredicateSyntaxError."""
    tokens = _tokenize(expr)
    if not tokens:
        raise PredicateSyntaxError("Empty predicate expression", 0)
    return _Parser(tokens).parse()


# ── Schema binding ─────────────────────────────────────────────────────────────

def _resolve_schema_node(root_node: dict, segments: tuple) -> dict | None:
    node = root_node
    for kind, *rest in segments:
        if kind == 'name':
            key = rest[0]
            if not isinstance(node, dict) or node.get('type') != 'object':
                return None
            props = node.get('properties', {})
            if key in props:
                node = props[key]
            else:
                return None
        elif kind == 'wildcard':
            if not isinstance(node, dict):
                return None
            if node.get('type') == 'array' and isinstance(node.get('items'), dict):
                node = node['items']
            elif isinstance(node.get('additionalProperties'), dict):
                node = node['additionalProperties']
            else:
                return None
        elif kind == 'index':
            if not isinstance(node, dict) or node.get('type') != 'array' or not isinstance(node.get('items'), dict):
                return None
            node = node['items']
    return node


def _schema_root(state_schema: dict) -> dict:
    return {'type': 'object', 'properties': state_schema}


def _item_schema(container_node: dict | None) -> dict | None:
    """Given the schema node a quantifier's list_path resolves to, return the per-item schema."""
    if not isinstance(container_node, dict):
        return None
    if container_node.get('type') == 'array' and isinstance(container_node.get('items'), dict):
        return container_node['items']
    if isinstance(container_node.get('additionalProperties'), dict):
        return container_node['additionalProperties']
    return None


def validate_against_schema(node: Any, state_schema: dict) -> list[str]:
    """
    Walk every StatePath in the predicate and confirm it resolves against
    state_schema. Returns a list of human-readable error strings — empty
    means the predicate is fully bound to real, declared state dimensions.

    Inside a quantifier body, a name may resolve either against the current
    item's own fields OR against the top-level state (mirrors evaluate_predicate's
    item-merged-onto-global-root semantics) — so `scope_schema` (what a bare
    name resolves against right here) and `root_schema` (the constant,
    top-level schema, always still reachable) are tracked separately.
    """
    errors: list[str] = []
    root_schema = _schema_root(state_schema)
    _walk_validate(node, root_schema, root_schema, errors)
    return errors


def _walk_validate(node: Any, scope_schema: dict, root_schema: dict, errors: list[str]) -> None:
    if isinstance(node, StatePath):
        if (_resolve_schema_node(scope_schema, node.segments) is None
                and _resolve_schema_node(root_schema, node.segments) is None):
            errors.append(f"'{node.raw}' does not resolve against the declared state_schema")
        return
    if isinstance(node, Aggregate):
        _walk_validate(node.path, scope_schema, root_schema, errors)
        return
    if isinstance(node, (Literal, ParamRef, ListLiteral)):
        return
    if isinstance(node, Comparison):
        _walk_validate(node.left, scope_schema, root_schema, errors)
        _walk_validate(node.right, scope_schema, root_schema, errors)
        return
    if isinstance(node, Membership):
        _walk_validate(node.left, scope_schema, root_schema, errors)
        _walk_validate(node.right, scope_schema, root_schema, errors)
        return
    if isinstance(node, (LogicalAnd, LogicalOr)):
        for part in node.parts:
            _walk_validate(part, scope_schema, root_schema, errors)
        return
    if isinstance(node, LogicalNot):
        _walk_validate(node.operand, scope_schema, root_schema, errors)
        return
    if isinstance(node, Quantifier):
        container = (_resolve_schema_node(scope_schema, node.list_path.segments)
                     or _resolve_schema_node(root_schema, node.list_path.segments))
        if container is None:
            errors.append(f"'{node.list_path.raw}' does not resolve against the declared state_schema")
            return
        item_schema = _item_schema(container)
        if item_schema is None:
            errors.append(
                f"'{node.list_path.raw}[*]' — schema for '{node.list_path.raw}' has no 'items' "
                f"or 'additionalProperties' to quantify over"
            )
            return
        # Item fields shadow same-named top-level fields, exactly as evaluate_predicate merges
        # {**global_root, **item} — so the nested scope is item properties over root properties.
        merged_scope = {
            'type': 'object',
            'properties': {**root_schema.get('properties', {}), **item_schema.get('properties', {})},
        }
        _walk_validate(node.predicate, merged_scope, root_schema, errors)
        return


def compile_invariant(expr: str, state_schema: dict) -> tuple[Any | None, list[str]]:
    """
    Parse + schema-validate a constraint string in one call.
    Returns (node, []) on success, or (None, [error, ...]) on failure —
    either a PredicateSyntaxError message or one or more schema-binding errors.
    """
    try:
        node = parse_predicate(expr)
    except PredicateSyntaxError as e:
        return None, [f"parse error: {e}"]
    errors = validate_against_schema(node, state_schema)
    if errors:
        return None, errors
    return node, []


# ── Evaluation ─────────────────────────────────────────────────────────────────

def _resolve_value(root: Any, segments: tuple) -> Any:
    """Walk segments against a raw dict/list. A wildcard maps the remaining
    segments over every element and returns a list; otherwise returns a scalar."""
    node = root
    for i, seg in enumerate(segments):
        kind = seg[0]
        if kind == 'name':
            if not isinstance(node, dict):
                return None
            node = node.get(seg[1])
        elif kind == 'index':
            if not isinstance(node, list) or seg[1] >= len(node):
                return None
            node = node[seg[1]]
        elif kind == 'wildcard':
            if isinstance(node, dict):
                items = list(node.values())
            elif isinstance(node, list):
                items = node
            else:
                return []
            remaining = segments[i + 1:]
            return [_resolve_value(item, remaining) for item in items]
    return node


def _describe(node: Any) -> str:
    """Human-legible label for an operand — used in failure reasons surfaced to the UI."""
    if isinstance(node, Literal):
        return repr(node.value)
    if isinstance(node, ParamRef):
        return f"params.{node.key}"
    if isinstance(node, StatePath):
        return node.raw
    if isinstance(node, Aggregate):
        return f"{node.func}({node.path.raw})"
    return str(node)


def _eval_operand(node: Any, scope: Any, global_root: Any, params: dict) -> Any:
    if isinstance(node, Literal):
        return node.value
    if isinstance(node, ParamRef):
        return params.get(node.key)
    if isinstance(node, StatePath):
        val = _resolve_value(scope, node.segments)
        if val is None and scope is not global_root:
            val = _resolve_value(global_root, node.segments)
        return val
    if isinstance(node, Aggregate):
        values = _resolve_value(scope, node.path.segments)
        if not isinstance(values, list):
            values = []
        nums = [v for v in values if isinstance(v, (int, float)) and not isinstance(v, bool)]
        if node.func == 'SUM':
            return sum(nums)
        if node.func == 'COUNT':
            return len(values)
        if node.func == 'AVG':
            return (sum(nums) / len(nums)) if nums else 0
        if node.func == 'MIN':
            return min(nums) if nums else None
        if node.func == 'MAX':
            return max(nums) if nums else None
        raise ValueError(f"Unknown aggregate function '{node.func}'")
    if isinstance(node, ListLiteral):
        return [item.value for item in node.items]
    raise TypeError(f"'{type(node).__name__}' is not a valid operand")


def evaluate_predicate(node: Any, state: StateAccessor, params: dict | None = None) -> tuple[bool, str]:
    """Evaluate a compiled predicate against a concrete state. Returns (ok, reason)."""
    root = state.snapshot()
    params = params or {}
    try:
        return _eval_node(node, root, root, params)
    except (TypeError, ValueError) as e:
        return False, f"Evaluation error: {e}"


def _eval_node(node: Any, scope: Any, global_root: Any, params: dict) -> tuple[bool, str]:
    """
    scope: what a bare name resolves against right here (the item, inside a
           quantifier body; the full state otherwise).
    global_root: the constant, unchanging top-level state — always still
           reachable so a quantifier body can reference outer state
           (e.g. `rules.fine_reasons` from inside `ALL fines[*]...`).
    """
    if isinstance(node, LogicalOr):
        reasons = []
        for part in node.parts:
            ok, reason = _eval_node(part, scope, global_root, params)
            if ok:
                return True, ""
            reasons.append(reason)
        return False, " OR ".join(reasons)

    if isinstance(node, LogicalAnd):
        for part in node.parts:
            ok, reason = _eval_node(part, scope, global_root, params)
            if not ok:
                return False, reason
        return True, ""

    if isinstance(node, LogicalNot):
        ok, _ = _eval_node(node.operand, scope, global_root, params)
        return (False, "NOT condition failed: inner expression was True") if ok else (True, "")

    if isinstance(node, Comparison):
        left = _eval_operand(node.left, scope, global_root, params)
        right = _eval_operand(node.right, scope, global_root, params)
        try:
            if node.op == '>':
                ok = left > right
            elif node.op == '>=':
                ok = left >= right
            elif node.op == '<':
                ok = left < right
            elif node.op == '<=':
                ok = left <= right
            elif node.op == '==':
                ok = left == right
            elif node.op == '!=':
                ok = left != right
            else:  # pragma: no cover — _parse_comp_op already restricts this
                return False, f"Unknown operator '{node.op}'"
        except TypeError as e:
            return False, f"Type error comparing {left!r} {node.op} {right!r}: {e}"
        if not ok:
            return False, f"{_describe(node.left)} ({left!r}) {node.op} {_describe(node.right)} ({right!r}) failed"
        return True, ""

    if isinstance(node, Membership):
        left = _eval_operand(node.left, scope, global_root, params)
        right = _eval_operand(node.right, scope, global_root, params)
        if not isinstance(right, list):
            return False, "IN requires a list on the right side"
        is_in = left in right
        label = _describe(node.left)
        if node.negate:
            return (False, f"{label} ({left!r}) is IN {right} (NOT IN violated)") if is_in else (True, "")
        return (True, "") if is_in else (False, f"{label} ({left!r}) not IN {right}")

    if isinstance(node, Quantifier):
        container = _resolve_value(scope, node.list_path.segments)
        if container is None and scope is not global_root:
            container = _resolve_value(global_root, node.list_path.segments)
        if isinstance(container, dict):
            item_list = list(container.values())
        elif isinstance(container, list):
            item_list = container
        else:
            return False, f"{node.kind}: '{node.list_path.raw}' is not a list or dict"
        for idx, item in enumerate(item_list):
            item_dict = item if isinstance(item, dict) else {"value": item}
            # Item fields shadow same-named top-level state fields; anything not
            # shadowed (e.g. 'rules' from inside 'ALL fines[*]...') stays reachable.
            item_scope = {**global_root, **item_dict} if isinstance(global_root, dict) else item_dict
            ok, reason = _eval_node(node.predicate, item_scope, global_root, params)
            if node.kind == 'ALL' and not ok:
                return False, f"ALL '{node.list_path.raw}' failed at index {idx}: {reason}"
            if node.kind == 'EXISTS' and ok:
                return True, ""
        return (True, "") if node.kind == 'ALL' else (
            False, f"EXISTS: no item in '{node.list_path.raw}' matches the predicate"
        )

    raise TypeError(f"'{type(node).__name__}' is not an evaluable predicate node")  # pragma: no cover
