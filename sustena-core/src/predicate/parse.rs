//! Recursive-descent parser: tokens → typed AST.
//!
//! Precedence, lowest first: `OR` → `AND` → `NOT` → primary.
//! A primary is a parenthesised group, a quantifier, or a comparison.

use serde_json::{json, Value};

use super::ast::*;
use super::lex::{self, AGGREGATE_FUNCS, COMPARISON_OPS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub message: String,
    pub position: usize,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at token {})", self.message, self.position)
    }
}

type PResult<T> = Result<T, SyntaxError>;

pub fn parse_predicate(expr: &str) -> PResult<Predicate> {
    let tokens = lex::tokenize(expr);
    let mut p = Parser { tokens, pos: 0 };
    let node = p.parse_or()?;
    if p.pos != p.tokens.len() {
        return Err(p.err(format!(
            "Unexpected trailing token '{}'",
            p.peek().unwrap_or_default()
        )));
    }
    Ok(node)
}

struct Parser {
    tokens: Vec<String>,
    pos: usize,
}

impl Parser {
    fn err(&self, message: String) -> SyntaxError {
        SyntaxError { message, position: self.pos }
    }

    fn peek(&self) -> Option<String> {
        self.tokens.get(self.pos).cloned()
    }

    fn peek_upper(&self) -> Option<String> {
        self.peek().map(|t| t.to_ascii_uppercase())
    }

    fn advance(&mut self) -> PResult<String> {
        match self.tokens.get(self.pos) {
            Some(t) => {
                self.pos += 1;
                Ok(t.clone())
            }
            None => Err(self.err("Unexpected end of expression".into())),
        }
    }

    fn expect(&mut self, want: &str) -> PResult<String> {
        let t = self.advance()?;
        if !t.eq_ignore_ascii_case(want) {
            return Err(SyntaxError {
                message: format!("Expected '{want}' but got '{t}'"),
                position: self.pos - 1,
            });
        }
        Ok(t)
    }

    fn parse_or(&mut self) -> PResult<Predicate> {
        let mut parts = vec![self.parse_and()?];
        while self.peek_upper().as_deref() == Some("OR") {
            self.advance()?;
            parts.push(self.parse_and()?);
        }
        Ok(if parts.len() == 1 { parts.pop().unwrap() } else { Predicate::Or(parts) })
    }

    fn parse_and(&mut self) -> PResult<Predicate> {
        let mut parts = vec![self.parse_not()?];
        while self.peek_upper().as_deref() == Some("AND") {
            self.advance()?;
            parts.push(self.parse_not()?);
        }
        Ok(if parts.len() == 1 { parts.pop().unwrap() } else { Predicate::And(parts) })
    }

    fn parse_not(&mut self) -> PResult<Predicate> {
        if self.peek_upper().as_deref() == Some("NOT") {
            self.advance()?;
            return Ok(Predicate::Not(Box::new(self.parse_not()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> PResult<Predicate> {
        if self.peek().as_deref() == Some("(") {
            self.advance()?;
            let node = self.parse_or()?;
            self.expect(")")?;
            return Ok(node);
        }
        match self.peek_upper().as_deref() {
            Some("ALL") | Some("EXISTS") => self.parse_quantifier(),
            _ => self.parse_comparison(),
        }
    }

    fn parse_quantifier(&mut self) -> PResult<Predicate> {
        let kind = if self.advance()?.eq_ignore_ascii_case("ALL") {
            QuantKind::All
        } else {
            QuantKind::Exists
        };
        let list_path = self.parse_path(false, false)?;
        self.expect("[")?;
        self.expect("*")?;
        self.expect("]")?;

        let body = if self.peek().as_deref() == Some(":") {
            // Block form: `ALL entries[*]: <full predicate>`
            self.advance()?;
            self.parse_or()?
        } else {
            // Field form: `ALL pockets[*].allocated >= 0`
            let field_tok = self.advance()?;
            let field = Operand::Path(path_from_token(&field_tok));
            match self.peek_upper().as_deref() {
                Some("NOT") | Some("IN") => {
                    let negate = if self.peek_upper().as_deref() == Some("NOT") {
                        self.advance()?;
                        true
                    } else {
                        false
                    };
                    self.expect("IN")?;
                    let right = self.parse_membership_rhs()?;
                    Predicate::Membership { left: field, negate, right }
                }
                _ => {
                    let op = self.parse_comp_op()?;
                    let right = self.parse_operand()?;
                    Predicate::Comparison { left: field, op, right }
                }
            }
        };

        Ok(Predicate::Quantifier { kind, list_path, body: Box::new(body) })
    }

    fn parse_comparison(&mut self) -> PResult<Predicate> {
        let left = self.parse_operand()?;
        match self.peek_upper().as_deref() {
            Some("NOT") | Some("IN") => {
                let negate = if self.peek_upper().as_deref() == Some("NOT") {
                    self.advance()?;
                    true
                } else {
                    false
                };
                self.expect("IN")?;
                let right = self.parse_membership_rhs()?;
                Ok(Predicate::Membership { left, negate, right })
            }
            _ => {
                let op = self.parse_comp_op()?;
                let right = self.parse_operand()?;
                Ok(Predicate::Comparison { left, op, right })
            }
        }
    }

    fn parse_comp_op(&mut self) -> PResult<CompOp> {
        let t = self.advance()?;
        if !COMPARISON_OPS.contains(&t.as_str()) {
            return Err(SyntaxError {
                message: format!("Expected a comparison operator, got '{t}'"),
                position: self.pos - 1,
            });
        }
        Ok(CompOp::parse(&t).expect("checked above"))
    }

    fn parse_membership_rhs(&mut self) -> PResult<Operand> {
        if self.peek().as_deref() == Some("[") {
            return self.parse_list_literal();
        }
        Ok(Operand::Path(self.parse_path(false, true)?))
    }

    fn parse_list_literal(&mut self) -> PResult<Operand> {
        self.expect("[")?;
        let mut items = Vec::new();
        while self.peek().as_deref() != Some("]") {
            let tok = self.advance()?;
            items.push(coerce_literal(&tok));
        }
        self.expect("]")?;
        Ok(Operand::List(items))
    }

    fn parse_operand(&mut self) -> PResult<Operand> {
        let t = match self.peek() {
            Some(t) => t,
            None => {
                return Err(self.err("Unexpected end of expression while reading an operand".into()))
            }
        };

        if AGGREGATE_FUNCS.contains(&t.to_ascii_uppercase().as_str()) {
            let func = AggFunc::parse(&self.advance()?).expect("checked above");
            self.expect("(")?;
            let path = self.parse_path(true, true)?;
            self.expect(")")?;
            return Ok(Operand::Aggregate { func, path });
        }
        if t == "[" {
            return self.parse_list_literal();
        }
        if is_literal_token(&t) {
            self.advance()?;
            return Ok(Operand::Literal(coerce_literal(&t)));
        }
        if let Some(key) = t.strip_prefix("params.") {
            let key = key.to_string();
            self.advance()?;
            return Ok(Operand::Param(key));
        }
        Ok(Operand::Path(self.parse_path(true, true)?))
    }

    fn parse_path(&mut self, allow_wildcard: bool, consume_brackets: bool) -> PResult<StatePath> {
        let tok = self.advance()?;
        if lex::is_keyword(&tok) || matches!(tok.as_str(), "(" | ")" | "[" | "]" | ":") {
            return Err(SyntaxError {
                message: format!("Expected a path, got '{tok}'"),
                position: self.pos - 1,
            });
        }

        let mut segments: Vec<PathSegment> =
            tok.split('.').map(|s| PathSegment::Name(s.to_string())).collect();
        let mut raw = tok.clone();

        if !consume_brackets {
            return Ok(StatePath { raw, segments });
        }

        while self.peek().as_deref() == Some("[") {
            self.advance()?;
            if self.peek().as_deref() == Some("*") {
                if !allow_wildcard {
                    return Err(self.err("Wildcard '[*]' is not allowed here".into()));
                }
                self.advance()?;
                self.expect("]")?;
                segments.push(PathSegment::Wildcard);
                raw.push_str("[*]");
            } else {
                let idx_tok = self.advance()?;
                let idx: usize = idx_tok.parse().map_err(|_| SyntaxError {
                    message: format!(
                        "Expected a numeric index or '*' inside '[]', got '{idx_tok}'"
                    ),
                    position: self.pos - 1,
                })?;
                self.expect("]")?;
                segments.push(PathSegment::Index(idx));
                raw.push_str(&format!("[{idx_tok}]"));
            }

            // A field continuation directly after ']', e.g. `[*].allocated`.
            if let Some(next) = self.peek() {
                let is_structural = lex::is_keyword(&next)
                    || matches!(next.as_str(), "(" | ")" | "[" | "]" | ":" | ">" | "<" | "=" | "!")
                    || COMPARISON_OPS.contains(&next.as_str());
                if !is_structural {
                    let cont = self.advance()?;
                    segments.extend(cont.split('.').map(|s| PathSegment::Name(s.to_string())));
                    raw.push('.');
                    raw.push_str(&cont);
                }
            }
        }

        Ok(StatePath { raw, segments })
    }
}

fn path_from_token(tok: &str) -> StatePath {
    StatePath {
        raw: tok.to_string(),
        segments: tok.split('.').map(|s| PathSegment::Name(s.to_string())).collect(),
    }
}

fn is_literal_token(tok: &str) -> bool {
    let chars: Vec<char> = tok.chars().collect();
    if chars.len() >= 2
        && (chars[0] == '"' || chars[0] == '\'')
        && chars[chars.len() - 1] == chars[0]
    {
        return true;
    }
    if matches!(tok.to_ascii_lowercase().as_str(), "true" | "false" | "null" | "none") {
        return true;
    }
    tok.parse::<f64>().is_ok()
}

/// Literal coercion, matching the reference exactly — including its final
/// fallback of treating a bare identifier as a string, which is what lets an
/// unquoted enum value work.
pub fn coerce_literal(tok: &str) -> Value {
    let chars: Vec<char> = tok.chars().collect();
    if chars.len() >= 2
        && (chars[0] == '"' || chars[0] == '\'')
        && chars[chars.len() - 1] == chars[0]
    {
        return Value::String(chars[1..chars.len() - 1].iter().collect());
    }
    match tok.to_ascii_lowercase().as_str() {
        "true" => return json!(true),
        "false" => return json!(false),
        "null" | "none" => return Value::Null,
        _ => {}
    }
    if let Ok(i) = tok.parse::<i64>() {
        return json!(i);
    }
    if let Ok(f) = tok.parse::<f64>() {
        return json!(f);
    }
    Value::String(tok.to_string())
}
