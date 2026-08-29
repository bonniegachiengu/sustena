//! The rules slice — the Viable region V.
//!
//! An expression is parsed ONCE into a typed AST and only the AST is ever
//! executed. No `eval`, no dynamic evaluation of user text, at any point.
//!
//! ## One evaluator, not two
//!
//! The reference engine grew two: `predicates.py` (typed AST, aggregates,
//! drives the enforcement gate) and `constraints.py` (string walker, rejects
//! aggregates, drives operator guards). They disagreed — a rule could pass the
//! gate and simultaneously display as failing — and unifying them was open gap
//! #4 on the article backlog.
//!
//! This core has one. Guard expressions and invariants are the same language,
//! so they are the same parser and the same evaluator. The divergence is not
//! ported and cannot recur.

pub mod ast;
pub mod eval;
pub mod lex;
pub mod parse;
pub mod types;

pub use ast::{AggFunc, CompOp, Operand, PathSegment, Predicate, QuantKind, StatePath};
pub use parse::{parse_predicate, SyntaxError};
pub use types::{typecheck, Gamma, Ty};

use serde_json::{Map, Value};

/// Parse a bare state path — `finances.pockets[*].allocated` — into segments.
///
/// The predicate parser reaches this grammar only from inside an expression.
/// Transition constraints ([`crate::transition`]) name paths directly, so the
/// grammar is exposed here rather than reimplemented there: a second wildcard
/// path grammar inside one crate is exactly the divergence the Constraint
/// article records about the reference engine's two evaluators.
///
/// Accepts `name`, `name[0]` and `name[*]`, dot-separated.
pub fn parse_state_path(raw: &str) -> Result<Vec<PathSegment>, String> {
    if raw.is_empty() {
        return Err("a path cannot be empty".to_string());
    }
    let mut out = Vec::new();
    for part in raw.split('.') {
        let (name, bracket) = match part.split_once('[') {
            Some((n, rest)) => {
                let inner = rest
                    .strip_suffix(']')
                    .ok_or_else(|| format!("unclosed '[' in segment '{part}'"))?;
                (n, Some(inner))
            }
            None => (part, None),
        };
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!("'{name}' is not a valid path segment"));
        }
        out.push(PathSegment::Name(name.to_string()));
        match bracket {
            None => {}
            Some("*") => out.push(PathSegment::Wildcard),
            Some(n) => out.push(PathSegment::Index(
                n.parse::<usize>()
                    .map_err(|_| format!("'{n}' is not an index or '*' in segment '{part}'"))?,
            )),
        }
    }
    Ok(out)
}

/// Parse, then evaluate against concrete state.
///
/// Returns `(verdict, reason)`. The reason is empty when the verdict holds and
/// otherwise names the operand and the value that failed — it is read by a
/// person when the gate refuses.
pub fn check(
    expr: &str,
    state: &Value,
    params: &Map<String, Value>,
) -> Result<(bool, String), SyntaxError> {
    let node = parse_predicate(expr)?;
    Ok(eval::evaluate(&node, state, params))
}

/// Evaluate an already-parsed predicate. Preferred on a hot path: an invariant
/// is compiled once when its spec loads, then evaluated on every gate check.
pub fn evaluate(node: &Predicate, state: &Value, params: &Map<String, Value>) -> (bool, String) {
    eval::evaluate(node, state, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(expr: &str, state: Value) -> bool {
        check(expr, &state, &Map::new()).unwrap().0
    }

    #[test]
    fn scalar_comparison() {
        assert!(ok("finances.liquid.balance >= 0", json!({"finances":{"liquid":{"balance":10}}})));
        assert!(!ok("finances.liquid.balance >= 0", json!({"finances":{"liquid":{"balance":-1}}})));
    }

    #[test]
    fn quantifier_over_a_dict() {
        // pockets is a dict, not a list — the homestead invariant's real shape.
        let state = json!({"finances":{"pockets":{"food":{"allocated":5},"rent":{"allocated":0}}}});
        assert!(ok("ALL finances.pockets[*].allocated >= 0", state));
    }

    #[test]
    fn quantifier_catches_the_one_bad_item() {
        let state = json!({"p":{"a":{"v":1},"b":{"v":-2}}});
        let (verdict, reason) = check("ALL p[*].v >= 0", &state, &Map::new()).unwrap();
        assert!(!verdict);
        assert!(reason.contains("failed at index"), "reason should locate it: {reason}");
    }

    #[test]
    fn aggregates() {
        let state = json!({"e":[{"lines":[{"d":100,"c":0},{"d":0,"c":100}]}]});
        assert!(ok("ALL e[*]: SUM(lines[*].d) == SUM(lines[*].c)", state));
    }

    #[test]
    fn quantifier_body_reaches_outer_state() {
        // `rules.allowed` is not a field of the item — it must still resolve.
        let state = json!({
            "fines": [{"reason":"late"}],
            "rules": {"allowed": ["late","damage"]}
        });
        assert!(ok("ALL fines[*].reason IN rules.allowed", state));
    }

    #[test]
    fn item_fields_shadow_outer_state() {
        let state = json!({"status":"outer","items":[{"status":"inner"}]});
        assert!(ok("ALL items[*].status == \"inner\"", state));
    }

    #[test]
    fn params_are_available() {
        let mut params = Map::new();
        params.insert("amount".into(), json!(50));
        let state = json!({"finances":{"liquid":{"balance":100}}});
        let (verdict, _) =
            check("finances.liquid.balance >= params.amount", &state, &params).unwrap();
        assert!(verdict);
    }

    #[test]
    fn logical_composition_and_precedence() {
        let s = json!({"a":1,"b":0});
        assert!(ok("a > 0 AND b == 0", s.clone()));
        assert!(ok("a > 5 OR b == 0", s.clone()));
        assert!(ok("NOT a > 5", s.clone()));
        // OR binds loosest: parsed as (a>5 AND b==1) OR (a==1)
        assert!(ok("a > 5 AND b == 1 OR a == 1", s));
    }

    #[test]
    fn comparing_incomparable_types_is_an_error_not_a_false() {
        let (verdict, reason) =
            check("missing.path > 0", &json!({}), &Map::new()).unwrap();
        assert!(!verdict);
        assert!(reason.contains("Type error"), "got: {reason}");
    }

    #[test]
    fn syntax_errors_are_reported_not_swallowed() {
        assert!(parse_predicate("this is (not ) valid").is_err());
        assert!(parse_predicate("a >").is_err());
    }
}
