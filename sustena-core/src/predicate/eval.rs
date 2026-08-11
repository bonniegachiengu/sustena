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
            let mut container = resolve_owned(scope, &list_path.segments);
            if container.as_ref().map_or(true, |v| v.is_null()) && !std::ptr::eq(scope, global_root) {
                container = resolve_owned(global_root, &list_path.segments);
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
            let v = resolve_owned(scope, &p.segments).unwrap_or(Value::Null);
            if v.is_null() && !std::ptr::eq(scope, global_root) {
                resolve_owned(global_root, &p.segments).unwrap_or(Value::Null)
            } else {
                v
            }
        }
        Operand::Aggregate { func, path } => {
            let values = match resolve_owned(scope, &path.segments) {
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

            match func {
                AggFunc::Sum => num(nums.iter().sum::<f64>()),
                AggFunc::Count => Value::from(values.len()),
                AggFunc::Avg => {
                    if nums.is_empty() {
                        Value::from(0)
                    } else {
                        num(nums.iter().sum::<f64>() / nums.len() as f64)
                    }
                }
                AggFunc::Min => nums
                    .iter()
                    .cloned()
                    .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.min(v))))
                    .map(num)
                    .unwrap_or(Value::Null),
                AggFunc::Max => nums
                    .iter()
                    .cloned()
                    .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))))
                    .map(num)
                    .unwrap_or(Value::Null),
            }
        }
        Operand::List(items) => Value::Array(items.clone()),
    }
}

/// Owned resolution, needed once a wildcard is involved.
fn resolve_owned(root: &Value, segments: &[PathSegment]) -> Option<Value> {
    let mut node = root.clone();
    for (i, seg) in segments.iter().enumerate() {
        match seg {
            PathSegment::Name(k) => node = node.as_object()?.get(k)?.clone(),
            PathSegment::Index(idx) => node = node.as_array()?.get(*idx)?.clone(),
            PathSegment::Wildcard => {
                let items: Vec<Value> = match &node {
                    Value::Object(m) => m.values().cloned().collect(),
                    Value::Array(a) => a.clone(),
                    _ => return Some(Value::Array(vec![])),
                };
                let rest = &segments[i + 1..];
                let mapped: Vec<Value> = items
                    .iter()
                    .map(|item| resolve_owned(item, rest).unwrap_or(Value::Null))
                    .collect();
                return Some(Value::Array(mapped));
            }
        }
    }
    Some(node)
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

fn num(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        Value::from(v as i64)
    } else {
        Value::from(v)
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
