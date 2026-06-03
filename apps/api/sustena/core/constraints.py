"""
sustena/core/constraints.py

ConstraintEngine — the Constraint primitive.

Evaluates constraint predicate strings against a StateAccessor and a dict
of operator call parameters. Does NOT use Python eval() — uses a hand-written
tokenizer + recursive descent evaluator.

Supported DSL syntax:

  Comparisons:    finances.liquid.balance > 0
                  finances.liquid.balance >= params.amount
                  order.status == "active"
                  order.status != "cancelled"

  Membership:     order.status IN ["pending", "confirmed"]
                  payment.method NOT IN ["cash"]

  Logical:        finances.liquid.balance > 0 AND params.amount > 0
                  order.status == "pending" OR order.status == "confirmed"
                  NOT inventory.item.out_of_stock

  Quantifiers:    ALL finances.pockets FIELD allocated >= 0
                  EXISTS staff.roster FIELD status == "active"

Usage:
    engine = ConstraintEngine()
    ok, reason = engine.evaluate(
        "finances.liquid.balance >= params.amount",
        state_accessor,
        params={"amount": 5000}
    )
    if not ok:
        raise ConstraintViolation(reason)
"""

from typing import Any

from sustena.core.state import StateAccessor


class ConstraintViolation(Exception):
    """Raised when a constraint check fails during operator execution."""


class ConstraintParseError(Exception):
    """Raised when a constraint string cannot be parsed."""


class ConstraintEngine:
    """
    Evaluates constraint predicate strings.
    No eval(), no exec() — hand-written tokenizer + recursive descent evaluator.
    """

    # ── Tokenizer ──────────────────────────────────────────────────────────────

    def _tokenise(self, expr: str) -> list[str]:
        """
        Character-by-character tokenizer.
        Handles: quoted strings, comparison operators, brackets, numbers,
                 keywords (AND/OR/NOT/IN/ALL/EXISTS), and dot-path identifiers.
        """
        tokens: list[str] = []
        i = 0
        s = expr.strip()
        n = len(s)

        while i < n:
            c = s[i]

            # Skip whitespace and commas
            if c.isspace() or c == ',':
                i += 1
                continue

            # Quoted string — scan to matching close quote, respect backslash escapes
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
                tokens.append(s[i:j + 1])   # includes the surrounding quotes
                i = j + 1
                continue

            # Brackets
            if c in ('[', ']'):
                tokens.append(c)
                i += 1
                continue

            # Comparison operators: >= <= != == > < ! =
            if c in ('>', '<', '!', '='):
                if i + 1 < n and s[i + 1] in ('>', '<', '!', '='):
                    tokens.append(s[i:i + 2])
                    i += 2
                else:
                    tokens.append(c)
                    i += 1
                continue

            # Numbers
            if c.isdigit():
                j = i
                while j < n and (s[j].isdigit() or s[j] == '.'):
                    j += 1
                tokens.append(s[i:j])
                i = j
                continue

            # Identifiers, keywords, dot-paths (letters, digits, underscores, dots)
            if c.isalpha() or c == '_':
                j = i
                while j < n and (s[j].isalnum() or s[j] in ('.', '_')):
                    j += 1
                tokens.append(s[i:j])
                i = j
                continue

            # Skip anything unrecognised
            i += 1

        return tokens

    # ── Value resolver ─────────────────────────────────────────────────────────

    def _resolve(self, token: str, state: StateAccessor, params: dict) -> Any:
        """Resolve a single token to its runtime value."""

        # Quoted string → strip surrounding quotes
        if len(token) >= 2 and token[0] in ('"', "'") and token[-1] == token[0]:
            return token[1:-1]

        # Boolean / null literals (case-insensitive)
        lower = token.lower()
        if lower == 'true':
            return True
        if lower == 'false':
            return False
        if lower in ('null', 'none'):
            return None

        # Integer
        try:
            return int(token)
        except ValueError:
            pass

        # Float
        try:
            return float(token)
        except ValueError:
            pass

        # params.* → operator parameter
        if token.startswith('params.'):
            key = token[7:]
            return params.get(key)

        # Everything else → state path
        return state.get(token)

    # ── List literal parser ────────────────────────────────────────────────────

    def _parse_list(self, tokens: list[str], start: int) -> tuple[list, int]:
        """
        Parse a list literal starting at tokens[start] == '['.
        Returns (values, next_index).
        """
        if tokens[start] != '[':
            raise ConstraintParseError(f"Expected '[' at position {start}")
        i = start + 1
        items = []
        while i < len(tokens) and tokens[i] != ']':
            token = tokens[i]
            if len(token) >= 2 and token[0] in ('"', "'") and token[-1] == token[0]:
                items.append(token[1:-1])
            else:
                try:
                    items.append(int(token))
                except ValueError:
                    try:
                        items.append(float(token))
                    except ValueError:
                        items.append(token)
            i += 1
        return items, i + 1  # i+1 skips the ']'

    # ── Core evaluator ─────────────────────────────────────────────────────────

    def _eval(self, tokens: list[str], state: StateAccessor, params: dict) -> tuple[bool, str]:
        """
        Evaluate a token list. Handles AND/OR/NOT at the top level,
        then delegates to _eval_atom for single predicates.

        Operator precedence (low to high): OR → AND → NOT → atom
        """
        if not tokens:
            return True, ""

        # Split on OR (lowest precedence)
        or_parts = self._split_on(tokens, 'OR')
        if len(or_parts) > 1:
            reasons = []
            for part in or_parts:
                ok, reason = self._eval(part, state, params)
                if ok:
                    return True, ""
                reasons.append(reason)
            return False, " OR ".join(reasons)

        # Split on AND
        and_parts = self._split_on(tokens, 'AND')
        if len(and_parts) > 1:
            for part in and_parts:
                ok, reason = self._eval(part, state, params)
                if not ok:
                    return False, reason
            return True, ""

        # NOT <expr>
        if tokens[0].upper() == 'NOT' and len(tokens) > 1:
            ok, reason = self._eval(tokens[1:], state, params)
            if ok:
                return False, f"NOT condition failed: inner expression was True"
            return True, ""

        # Single predicate
        return self._eval_atom(tokens, state, params)

    def _split_on(self, tokens: list[str], keyword: str) -> list[list[str]]:
        """
        Split tokens on a keyword (case-insensitive).
        Returns list of sublists. If keyword not found, returns [tokens].
        """
        parts: list[list[str]] = []
        current: list[str] = []
        for token in tokens:
            if token.upper() == keyword.upper():
                parts.append(current)
                current = []
            else:
                current.append(token)
        parts.append(current)
        # Only split if we actually found the keyword
        return parts if len(parts) > 1 else [tokens]

    def _eval_atom(self, tokens: list[str], state: StateAccessor, params: dict) -> tuple[bool, str]:
        """Evaluate a single predicate expression (no AND/OR/NOT at this level)."""
        if not tokens:
            return True, ""

        t0_upper = tokens[0].upper()

        # ALL <list_path> FIELD <predicate...>
        if t0_upper == 'ALL':
            return self._eval_quantifier('ALL', tokens, state, params)

        # EXISTS <list_path> FIELD <predicate...>
        if t0_upper == 'EXISTS':
            return self._eval_quantifier('EXISTS', tokens, state, params)

        # Single token — truth check
        if len(tokens) == 1:
            val = self._resolve(tokens[0], state, params)
            if not val:
                return False, f"'{tokens[0]}' is falsy (value: {val!r})"
            return True, ""

        # Detect NOT IN as two-token operator: <left> NOT IN <right>
        # tokens: [left, 'NOT', 'IN', '[', ...]  or  [left, 'NOT', 'IN', right]
        if (len(tokens) >= 4
                and tokens[1].upper() == 'NOT'
                and tokens[2].upper() == 'IN'):
            left = self._resolve(tokens[0], state, params)
            if tokens[3] == '[':
                right_list, _ = self._parse_list(tokens, 3)
            else:
                right_list = self._resolve(tokens[3], state, params)
            if not isinstance(right_list, list):
                return False, "NOT IN requires a list on the right side"
            if left in right_list:
                return False, f"'{tokens[0]}' ({left!r}) is IN {right_list} (NOT IN violated)"
            return True, ""

        # Standard two-operand: <left> <op> <right>
        if len(tokens) >= 3:
            left_tok = tokens[0]
            op = tokens[1].upper()
            right_tok = tokens[2]

            # IN operator
            if op == 'IN':
                left = self._resolve(left_tok, state, params)
                if right_tok == '[':
                    right_list, _ = self._parse_list(tokens, 2)
                else:
                    right_list = self._resolve(right_tok, state, params)
                if not isinstance(right_list, list):
                    return False, "IN requires a list on the right side"
                if left not in right_list:
                    return False, f"'{left_tok}' ({left!r}) not IN {right_list}"
                return True, ""

            # Comparison operators
            left = self._resolve(left_tok, state, params)
            right = self._resolve(right_tok, state, params)
            try:
                if op == '>':
                    ok = left > right
                elif op == '>=':
                    ok = left >= right
                elif op == '<':
                    ok = left < right
                elif op == '<=':
                    ok = left <= right
                elif op == '==':
                    ok = left == right
                elif op == '!=':
                    ok = left != right
                else:
                    return False, f"Unknown operator: '{op}'"
            except TypeError as e:
                return False, f"Type error: {left_tok}({left!r}) {op} {right_tok}({right!r}): {e}"

            if not ok:
                return False, f"Constraint failed: {left_tok} ({left!r}) {op} {right_tok} ({right!r})"
            return True, ""

        # Single token fallthrough
        val = self._resolve(tokens[0], state, params)
        if not val:
            return False, f"'{tokens[0]}' is falsy"
        return True, ""

    def _eval_quantifier(
        self, quantifier: str, tokens: list[str], state: StateAccessor, params: dict
    ) -> tuple[bool, str]:
        """
        Handle ALL/EXISTS over a list OR dict. Supported forms:
          ALL <path> FIELD <field> <pred>     (legacy keyword form)
          ALL <path>[*].<field> <op> <value>  (array/dict wildcard)

        The wildcard `[*]` tokenises to '[' ']' tokens, e.g.
        'ALL finances.pockets[*].allocated >= 0' →
        ['ALL','finances.pockets','[',']','allocated','>=','0'].
        Aggregate/block forms (`[*]: SUM(...)`) are not supported here.
        """
        if '[' in tokens and ']' in tokens:
            lb = tokens.index('[')
            rb = tokens.index(']')
            list_path = tokens[lb - 1] if lb - 1 >= 1 else ""
            pred_tokens = tokens[rb + 1:]
            if not pred_tokens or pred_tokens[0] == ':' or any(
                t.upper() in ('SUM', 'COUNT', 'AVG', 'MIN', 'MAX') for t in pred_tokens
            ):
                return False, f"{quantifier}: unsupported aggregate/block expression"
        else:
            try:
                field_idx = next(i for i, t in enumerate(tokens) if t.upper() == 'FIELD')
            except StopIteration:
                return False, f"{quantifier}: missing FIELD keyword"
            list_path = tokens[1] if len(tokens) > 1 else ""
            pred_tokens = tokens[field_idx + 1:]

        items = state.get(list_path)
        # Iterate dict values (e.g. finances.pockets) or list elements.
        if isinstance(items, dict):
            item_list = list(items.values())
        elif isinstance(items, list):
            item_list = items
        else:
            return False, f"{quantifier}: '{list_path}' is not a list or dict"

        for idx, item in enumerate(item_list):
            merged = {**state.snapshot(), **item} if isinstance(item, dict) \
                else {**state.snapshot(), "value": item}
            item_state = StateAccessor(merged)
            ok, reason = self._eval(pred_tokens, item_state, params)
            if quantifier == 'ALL' and not ok:
                return False, f"ALL '{list_path}' failed at index {idx}: {reason}"
            if quantifier == 'EXISTS' and ok:
                return True, ""

        if quantifier == 'ALL':
            return True, ""
        return False, f"EXISTS: no item in '{list_path}' matches the predicate"

    # ── Public interface ───────────────────────────────────────────────────────

    def evaluate(
        self,
        constraint: str,
        state: StateAccessor,
        params: dict | None = None,
    ) -> tuple[bool, str]:
        """
        Evaluate a constraint string against state and params.
        Returns (True, "") on pass.
        Returns (False, reason_string) on fail.
        """
        if not constraint or not constraint.strip():
            return True, ""
        params = params or {}
        tokens = self._tokenise(constraint)
        if not tokens:
            return True, ""
        try:
            return self._eval(tokens, state, params)
        except ConstraintParseError as e:
            return False, f"Parse error: {e}"
        except Exception as e:
            return False, f"Evaluation error: {e}"

    def evaluate_all(
        self,
        constraints: list[str],
        state: StateAccessor,
        params: dict | None = None,
    ) -> tuple[bool, str]:
        """
        Evaluate a list of constraints — all must pass.
        Returns on first failure.
        """
        for constraint in constraints:
            ok, reason = self.evaluate(constraint, state, params)
            if not ok:
                return False, reason
        return True, ""
