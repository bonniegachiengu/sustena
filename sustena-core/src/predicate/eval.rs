//! Evaluating a compiled predicate against concrete state.
//!
//! Returns `(verdict, reason)`. The reason is what a person reads when the gate
//! refuses, so it names the operand and shows the value that failed.
//!
//! ## Scope, and why there are two roots
//!
//! Inside a quantifier body, a bare name resolves against the current ITEM
//! first — that is what makes `ALL pockets[*].allocated >= 0` mean the item's
//! own field. But an unshadowed name must still reach the surrounding state, so
//! a body can say `reason IN rules.fine_reasons` while iterating fines.
//!
//! Both roots are therefore threaded through evaluation: `scope` for the item,
//! `global_root` as the constant outer state. The reference achieves the same
//! effect by merging the item over a copy of the state; the merge is kept here
//! for exact parity on shadowing.

use serde_json::{Map, Value};

use super::ast::*;

pub fn evaluate(node: &Predicate, state: &Value, params: &Map<String, Value>) -> (bool, String) {
    eval_node(node, state, state, params)
}

fn eval_node(
    node: &Predicate,
    scope: &Value,
    global_root: &Value,
    params: &Map<String, Value>,
) -> (bool, String) {
    match node {
        Predicate::Or(parts) => {
            let mut reasons = Vec::new();
            for part in parts {
                let (ok, reason) = eval_node(part, scope, global_root, params);
                if ok {
                    return (true, String::new());
                }
                reasons.push(reason);
            }
            (false, reasons.join(" OR "))
        }

        Predicate::And(parts) => {
            for part in parts {
                let (ok, reason) = eval_node(part, scope, global_root, params);
                if !ok {
                    return (false, reason);
                }
            }
            (true, String::new())
        }

        Predicate::Not(inner) => {
            let (ok, _) = eval_node(inner, scope, global_root, params);
            if ok {
                (false, "NOT condition failed: inner expression was True".into())
            } else {
                (true, String::new())
            }
        }

        Predicate::Comparison { left, op, right } => {
            let l = eval_operand(left, scope, global_root, params);
            let r = eval_operand(right, scope, global_root, params);
            match compare(&l, *op, &r) {
                Err(msg) => (
                    false,
                    format!(
                        "Type error comparing {} {} {}: {msg}",
                        py_repr(&l),
                        op.symbol(),
                        py_repr(&r)
                    ),
                ),
                Ok(true) => (true, String::new()),
                Ok(false) => (
                    false,
                    format!(
                        "{} ({}) {} {} ({}) failed",
                        left.describe(),
                        py_repr(&l),
                        op.symbol(),
                        right.describe(),
                        py_repr(&r)
                    ),
                ),
            }
        }

        Predicate::Membership { left, negate, right } => {
            let l = eval_operand(left, scope, global_root, params);
            let r = eval_operand(right, scope, global_root, params);
            let items = match &r {
                Value::Array(a) => a.clone(),
                _ => return (false, "IN requires a list on the right side".into()),
            };
            let is_in = items.iter().any(|item| py_eq(item, &l));
            let label = left.describe();
            if *negate {
                if is_in {
                    (false, format!("{label} ({}) is IN {} (NOT IN violated)", py_repr(&l), py_list(&items)))
                } else {
                    (true, String::new())
                }
            } else if is_in {
                (true, String::new())
            } else {
                (false, format!("{label} ({}) not IN {}", py_repr(&l), py_list(&items)))
            }
        }

        Predicate::Quantifier { kind, list_path, body } => {
            let mut container = resolve_path(scope, &list_path.segments);
            if container.as_ref().is_none_or(|v| v.is_null()) && !std::ptr::eq(scope, global_root) {
                container = resolve_path(global_root, &list_path.segments);
            }

            let items: Vec<Value> = match container {
                Some(Value::Object(map)) => map.values().cloned().collect(),
                Some(Value::Array(arr)) => arr,
                _ => {
                    return (
                        false,
                        format!("{}: '{}' is not a list or dict", kind.name(), list_path.raw),
                    )
                }
            };

            for (idx, item) in items.iter().enumerate() {
                // Item fields shadow same-named outer state; everything else
                // stays reachable.
                let item_map = match item {
                    Value::Object(m) => m.clone(),
                    other => {
                        let mut m = Map::new();
                        m.insert("value".into(), other.clone());
                        m
                    }
                };
                let item_scope = match global_root {
                    Value::Object(g) => {
                        let mut merged = g.clone();
                        for (k, v) in item_map {
                            merged.insert(k, v);
                        }
                        Value::Object(merged)
                    }
                    _ => Value::Object(item_map),
                };

                let (ok, reason) = eval_node(body, &item_scope, global_root, params);
                match kind {
                    QuantKind::All if !ok => {
                        return (
                            false,
                            format!("ALL '{}' failed at index {idx}: {reason}", list_path.raw),
                        )
                    }
                    QuantKind::Exists if ok => return (true, String::new()),
                    _ => {}
                }
            }

            match kind {
                QuantKind::All => (true, String::new()),
                QuantKind::Exists => (
                    false,
                    format!(
                        "EXISTS: no item in '{}' matches the predicate",
                        list_path.raw
                    ),
                ),
            }
        }
    }
}

fn eval_operand(
    node: &Operand,
    scope: &Value,
    global_root: &Value,
    params: &Map<String, Value>,
) -> Value {
    match node {
        Operand::Literal(v) => v.clone(),
        Operand::Param(key) => params.get(key).cloned().unwrap_or(Value::Null),
        Operand::Path(p) => {
            // Python cannot tell "missing" from "explicitly null" — both are
            // None there — so both fall back to the outer root here.
            let v = resolve_path(scope, &p.segments).unwrap_or(Value::Null);
            if v.is_null() && !std::ptr::eq(scope, global_root) {
                resolve_path(global_root, &p.segments).unwrap_or(Value::Null)
            } else {
                v
            }
        }
        Operand::Aggregate { func, path } => {
            let values = match resolve_path(scope, &path.segments) {
                Some(Value::Array(a)) => a,
                _ => Vec::new(),
            };
            // Booleans are excluded from the numeric set even though Python
            // treats bool as int — matching the reference's explicit guard.
            let nums: Vec<f64> = values
                .iter()
                .filter(|v| !v.is_boolean())
                .filter_map(|v| v.as_f64())
                .collect();

            // ★ One definition of this arithmetic, shared with roll-up ρ. The
            //   population is EVERY element here — `COUNT` inside a predicate
            //   counts what is there, numeric or not.
            func.reduce(&nums, values.len())
        }
        Operand::List(items) => Value::Array(items.clone()),
    }
}

/// Resolve a parsed path against a state document, cloning what it lands on.
///
/// ★ **The one path resolver in this crate.** `V` reaches it through the
/// predicate evaluator and roll-up ρ reaches it directly, so a wildcard means
/// the same thing to a rule and to a household total — which is exactly the
/// divergence [`crate::predicate::parse_state_path`] exists to prevent on the
/// grammar side.
pub fn resolve_path(root: &Value, segments: &[PathSegment]) -> Option<Value> {
    resolve(root, segments).map(std::borrow::Cow::into_owned)
}

/// The walk itself, by reference.
///
/// ★★★ **It used to clone the whole state document to read one field.** The
/// first line was `let mut node = root.clone()`, then every `Name` and `Index`
/// segment cloned again, and a wildcard cloned every item before recursing —
/// so reading `finances.liquid.balance` deep-copied the entire household,
/// pockets, inventory and all. Nothing was wrong with the answers; the cost was
/// invisible because it was spelled `.clone()` in six ordinary-looking places.
///
/// ★★ **And it is paid twice per operand.** `eval_node` resolves against the
/// quantifier `scope` first and falls back to `global_root`, which is a second
/// full copy for every unshadowed name — and this whole path runs per rule, per
/// evaluation, on every §III pulse now that a surface refreshes from a relayed
/// change.
///
/// ★ `Cow` because a wildcard genuinely has to build something new: it maps the
/// remaining segments over each item, and that array exists nowhere in the
/// document. Everything else is a borrow, and the single clone happens at the
/// boundary in [`resolve_path`], on the value actually landed on rather than on
/// everything walked through to reach it.
fn resolve<'a>(
    node: &'a Value,
    segments: &[PathSegment],
) -> Option<std::borrow::Cow<'a, Value>> {
    use std::borrow::Cow;
    let mut cur = node;
    for (i, seg) in segments.iter().enumerate() {
        match seg {
            PathSegment::Name(k) => cur = cur.as_object()?.get(k)?,
            PathSegment::Index(idx) => cur = cur.as_array()?.get(*idx)?,
            PathSegment::Wildcard => {
                let rest = &segments[i + 1..];
                let map_item = |item: &Value| {
                    resolve(item, rest).map(Cow::into_owned).unwrap_or(Value::Null)
                };
                let mapped: Vec<Value> = match cur {
                    Value::Object(m) => m.values().map(map_item).collect(),
                    Value::Array(a) => a.iter().map(map_item).collect(),
                    // ★ A wildcard over something that is not a container is an
                    //   empty list, not an absence — unchanged, and the
                    //   difference matters to COUNT.
                    _ => return Some(Cow::Owned(Value::Array(vec![]))),
                };
                return Some(Cow::Owned(Value::Array(mapped)));
            }
        }
    }
    Some(Cow::Borrowed(cur))
}

/// Python-compatible equality: numbers compare by value across int/float, and
/// bool compares as 1/0 against numbers (Python's `True == 1`).
pub fn py_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Bool(x), Value::Number(_)) => b.as_f64() == Some(if *x { 1.0 } else { 0.0 }),
        (Value::Number(_), Value::Bool(y)) => a.as_f64() == Some(if *y { 1.0 } else { 0.0 }),
        (Value::Number(_), Value::Number(_)) => a.as_f64() == b.as_f64(),
        _ => a == b,
    }
}

/// Ordering comparisons need mutually comparable operands, exactly as Python
/// does — `None > 0` and `1 < "a"` are errors there, not `false`.
fn compare(l: &Value, op: CompOp, r: &Value) -> Result<bool, String> {
    if !op.is_ordering() {
        let eq = py_eq(l, r);
        return Ok(if op == CompOp::Eq { eq } else { !eq });
    }

    let ord = match (l, r) {
        (Value::String(a), Value::String(b)) => a.cmp(b),
        _ => {
            let (a, b) = (numeric(l), numeric(r));
            match (a, b) {
                (Some(a), Some(b)) => a
                    .partial_cmp(&b)
                    .ok_or_else(|| "unorderable values".to_string())?,
                _ => {
                    return Err(format!(
                        "'{}' not supported between instances of '{}' and '{}'",
                        op.symbol(),
                        py_type(l),
                        py_type(r)
                    ))
                }
            }
        }
    };

    Ok(match op {
        CompOp::Gt => ord.is_gt(),
        CompOp::Ge => ord.is_ge(),
        CompOp::Lt => ord.is_lt(),
        CompOp::Le => ord.is_le(),
        _ => unreachable!("equality handled above"),
    })
}

/// Numbers, plus bools — Python's bool IS an int, so `True > 0` is valid there.
fn numeric(v: &Value) -> Option<f64> {
    match v {
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Number(_) => v.as_f64(),
        _ => None,
    }
}

fn py_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "int"
            } else {
                "float"
            }
        }
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// Approximate Python's `repr()` — these strings appear in failure reasons.
pub fn py_repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::String(s) => format!("'{s}'"),
        Value::Array(a) => py_list(a),
        other => other.to_string(),
    }
}

fn py_list(items: &[Value]) -> String {
    format!(
        "[{}]",
        items.iter().map(py_repr).collect::<Vec<_>>().join(", ")
    )
}

#[cfg(test)]
mod resolve_path_tests {
    //! CON-9 — the walk resolves by reference; the answers are unchanged.
    use super::*;
    use crate::predicate::parse_state_path;
    use serde_json::json;

    fn at(state: &Value, path: &str) -> Option<Value> {
        resolve_path(state, &parse_state_path(path).expect("a valid path"))
    }

    fn household() -> Value {
        json!({
            "finances": {
                "liquid": { "balance": 1200.0 },
                "pockets": {
                    "food":  { "allocated": 5000.0, "spent": 2930.0 },
                    "rent":  { "allocated": 17000.0, "spent": 17000.0 },
                    "wifi":  { "allocated": 2500.0, "spent": 2500.0 }
                }
            },
            "roster": [ { "name": "Cira" }, { "name": "Epha" } ]
        })
    }

    #[test]
    fn a_named_path_lands_on_the_value_itself() {
        assert_eq!(at(&household(), "finances.liquid.balance"), Some(json!(1200.0)));
    }

    #[test]
    fn an_index_lands_on_the_element() {
        assert_eq!(at(&household(), "roster[0].name"), Some(json!("Cira")));
    }

    #[test]
    fn a_wildcard_over_an_object_maps_every_member() {
        // ★★ Objects iterate in the map's own order; the SET is what a rule and
        //    roll-up ρ both read, so this asserts membership rather than order.
        let got = at(&household(), "finances.pockets[*].allocated").expect("resolves");
        let mut values: Vec<f64> = got.as_array().expect("an array").iter().filter_map(Value::as_f64).collect();
        values.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        assert_eq!(values, vec![2500.0, 5000.0, 17000.0]);
    }

    #[test]
    fn a_wildcard_over_an_array_maps_every_element() {
        assert_eq!(at(&household(), "roster[*].name"), Some(json!(["Cira", "Epha"])));
    }

    #[test]
    fn a_wildcard_over_something_that_is_not_a_container_is_an_empty_list() {
        // ★★★ Empty is not absent, and the difference is load-bearing: COUNT of
        //     an empty list is 0, where a `None` would make the rule unreadable.
        assert_eq!(at(&household(), "finances.liquid.balance[*]"), Some(json!([])));
    }

    #[test]
    fn a_missing_field_resolves_to_nothing() {
        // ★ `None`, not `Null` -- the caller decides what an absent dimension
        //   means, and flattening the two here would take that decision away.
        assert_eq!(at(&household(), "finances.savings.balance"), None);
        assert_eq!(at(&household(), "roster[9].name"), None);
    }

    #[test]
    fn a_member_missing_the_mapped_field_becomes_null_rather_than_vanishing() {
        // ★★ Position is preserved: a household with a pocket that has no
        //    `spent` still reports three pockets, one of them unknown. Dropping
        //    it would silently change what COUNT and AVG mean.
        let state = json!({ "pockets": { "a": { "x": 1 }, "b": { "y": 2 } } });
        let got = at(&state, "pockets[*].x").expect("resolves");
        let arr = got.as_array().expect("an array");
        assert_eq!(arr.len(), 2, "both members are represented");
        assert!(arr.contains(&json!(1)) && arr.contains(&Value::Null));
    }

    #[test]
    fn the_document_is_read_not_consumed_and_a_big_one_reads_the_same() {
        // ★★★ The case the old walk paid for: it cloned the WHOLE document to
        //     read one field, so cost scaled with the household rather than the
        //     path. This asserts the answer is unchanged on a large document
        //     and that `root` is still intact afterwards -- a by-reference walk
        //     must borrow, never disturb.
        let mut big = household();
        let pockets = big["finances"]["pockets"].as_object_mut().expect("pockets");
        for i in 0..500 {
            pockets.insert(format!("p{i}"), json!({ "allocated": i as f64, "spent": 0.0 }));
        }
        let before = big.clone();

        assert_eq!(at(&big, "finances.liquid.balance"), Some(json!(1200.0)));
        let all = at(&big, "finances.pockets[*].allocated").expect("resolves");
        assert_eq!(all.as_array().expect("an array").len(), 503);

        assert_eq!(big, before, "resolving must not disturb the document it read");
    }
}
