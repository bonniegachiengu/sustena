//! Typed state — `S` in `Σ = ⟨B, S, V, T, ⊕⟩` (CELL / the Sustain paper, §II and §V).
//!
//! ```text
//!   S = ∏ over d ∈ Dim of τ(d)
//! ```
//!
//! **A record type, not a bag.** Each dimension carries a type and, where
//! meaningful, bounds. A path is a projection onto a subspace, which is what
//! makes "this operator reads `finances` and writes `finances.pockets`"
//! a statement with content rather than a comment.
//!
//! ## Why type it at all
//!
//! Not tidiness. The paper is precise: typing moves a whole class of failure
//! **from runtime to load time**.
//!
//! A guard, invariant or post-condition is a predicate over `S`. If it
//! references a dimension the schema does not declare, that is a *type error in
//! the predicate* — catchable when the spec is loaded, rather than a silent
//! comparison against `None` at 2 a.m.
//!
//! [`bind`] is that check:
//!
//! ```text
//!   bind(predicate, schema) -> (node, errors)
//!   empty errors  ⟺  the predicate is typed by S
//! ```
//!
//! ## Organisational closure — the property with teeth
//!
//! ```text
//!   ∀ o ∈ T, ∀ s ∈ S:   μ_{o(s)} = μ_s  ∧  schema(o(s)) = schema(s)
//! ```
//!
//! **Operators may change the values inside the boundary; they may not change
//! what the boundary is.** A sustain is *materially open* — money, stock,
//! messages cross it constantly — and *organisationally closed*: the structure
//! is preserved by every ordinary move.
//!
//! Changing the boundary itself (adding a member, adopting a child sustain) is
//! a distinct class of act governed by a separate, higher gate. That is why
//! such changes feel different to a person, and should.
//!
//! [`preserves_shape`] enforces it, so an operator that quietly introduces or
//! drops a dimension is refused rather than silently reshaping the sustain.
//!
//! ## Evolvable
//!
//! A schema may version without invalidating existing operators. Validation is
//! therefore structural, not nominal: adding a dimension does not break a
//! predicate that never mentioned it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::predicate::ast::{Operand, PathSegment, Predicate};

/// `τ(d)` — the type of one dimension, with bounds where meaningful.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DimType {
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lo: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hi: Option<f64>,
    },
    Text,
    Bool,
    /// A nested record — named, typed sub-dimensions.
    Record { fields: BTreeMap<String, DimType> },
    /// A homogeneous list.
    List { item: Box<DimType> },
    /// A record whose KEYS are open but whose values share a type — the shape
    /// `finances.pockets` actually has, where the pocket names are the user's.
    Map { value: Box<DimType> },
    /// Deliberately untyped. Present so a schema can be adopted incrementally
    /// rather than all at once; every `Any` is an admission, not a default.
    Any,
}

/// The declared state space of a sustain.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Schema {
    pub dimensions: BTreeMap<String, DimType>,
}

impl Schema {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn declare(mut self, name: &str, ty: DimType) -> Self {
        self.dimensions.insert(name.to_string(), ty);
        self
    }

    /// Does the schema declare this dot-path?
    ///
    /// A wildcard or index segment steps *into* a list or open map, so it does
    /// not need its own declaration.
    pub fn declares(&self, segments: &[PathSegment]) -> bool {
        self.resolve(segments).is_some()
    }

    /// `τ` at a path, if the schema declares it.
    pub fn resolve(&self, segments: &[PathSegment]) -> Option<&DimType> {
        let mut current: Option<&DimType> = None;
        for seg in segments {
            current = match (current, seg) {
                (None, PathSegment::Name(n)) => self.dimensions.get(n),
                (Some(DimType::Record { fields }), PathSegment::Name(n)) => fields.get(n),
                (Some(DimType::Map { value }), PathSegment::Name(_)) => Some(value.as_ref()),
                (Some(DimType::List { item }), PathSegment::Index(_)) => Some(item.as_ref()),
                (Some(DimType::List { item }), PathSegment::Wildcard) => Some(item.as_ref()),
                (Some(DimType::Map { value }), PathSegment::Wildcard) => Some(value.as_ref()),
                // Any absorbs whatever follows: an untyped region cannot
                // usefully disprove a path inside it.
                (Some(DimType::Any), _) => return Some(&DimType::Any),
                _ => return None,
            };
            current?;
        }
        current
    }
}

/// A dimension a predicate references but the schema does not declare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError {
    pub path: String,
    pub detail: String,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.detail)
    }
}

/// `bind(predicate, schema)` — empty errors ⟺ the predicate is typed by `S`.
///
/// Run when a spec is LOADED. A predicate that survives this cannot later
/// compare a declared dimension against nothing, because there is no
/// undeclared dimension left in it to resolve to nothing.
pub fn bind(predicate: &Predicate, schema: &Schema) -> Vec<TypeError> {
    let mut errors = Vec::new();
    walk(predicate, schema, &mut errors, &[]);
    errors
}

fn walk(node: &Predicate, schema: &Schema, errors: &mut Vec<TypeError>, scope: &[PathSegment]) {
    match node {
        Predicate::And(parts) | Predicate::Or(parts) => {
            for p in parts {
                walk(p, schema, errors, scope);
            }
        }
        Predicate::Not(inner) => walk(inner, schema, errors, scope),
        Predicate::Comparison { left, right, .. } => {
            check_operand(left, schema, errors, scope);
            check_operand(right, schema, errors, scope);
        }
        Predicate::Membership { left, right, .. } => {
            check_operand(left, schema, errors, scope);
            check_operand(right, schema, errors, scope);
        }
        Predicate::Quantifier { list_path, body, .. } => {
            if !schema.declares(&list_path.segments) {
                errors.push(TypeError {
                    path: list_path.raw.clone(),
                    detail: "undeclared dimension".into(),
                });
                return;
            }
            // Inside the body a bare name resolves against the ITEM first, so
            // the scope for nested lookups is the container plus a wildcard.
            let mut inner_scope = list_path.segments.clone();
            inner_scope.push(PathSegment::Wildcard);
            walk(body, schema, errors, &inner_scope);
        }
    }
}

fn check_operand(
    operand: &Operand,
    schema: &Schema,
    errors: &mut Vec<TypeError>,
    scope: &[PathSegment],
) {
    let path = match operand {
        Operand::Path(p) => p,
        Operand::Aggregate { path, .. } => path,
        // Literals and params are not dimensions of S.
        _ => return,
    };

    // A name inside a quantifier body may be the item's own field OR an
    // outer-state dimension — the evaluator tries both, so binding accepts
    // either. Rejecting the item-relative form would make every correct
    // quantifier a type error.
    if schema.declares(&path.segments) {
        return;
    }
    if !scope.is_empty() {
        let mut relative = scope.to_vec();
        relative.extend(path.segments.iter().cloned());
        if schema.declares(&relative) {
            return;
        }
    }

    errors.push(TypeError {
        path: path.raw.clone(),
        detail: "undeclared dimension".into(),
    });
}

/// Does a concrete state conform to the schema?
pub fn validate(state: &Value, schema: &Schema) -> Vec<TypeError> {
    let mut errors = Vec::new();
    for (name, ty) in &schema.dimensions {
        match state.get(name) {
            None => errors.push(TypeError {
                path: name.clone(),
                detail: "declared dimension is absent from state".into(),
            }),
            Some(v) => check_value(v, ty, name, &mut errors),
        }
    }
    errors
}

fn check_value(value: &Value, ty: &DimType, path: &str, errors: &mut Vec<TypeError>) {
    let mismatch = |errors: &mut Vec<TypeError>, want: &str| {
        errors.push(TypeError {
            path: path.to_string(),
            detail: format!("expected {want}, found {}", kind_of(value)),
        });
    };

    match ty {
        DimType::Any => {}
        DimType::Text => {
            if !value.is_string() {
                mismatch(errors, "text")
            }
        }
        DimType::Bool => {
            if !value.is_boolean() {
                mismatch(errors, "bool")
            }
        }
        DimType::Number { lo, hi } => match value.as_f64() {
            None => mismatch(errors, "number"),
            Some(n) => {
                if let Some(lo) = lo {
                    if n < *lo {
                        errors.push(TypeError {
                            path: path.to_string(),
                            detail: format!("{n} is below the declared lower bound {lo}"),
                        });
                    }
                }
                if let Some(hi) = hi {
                    if n > *hi {
                        errors.push(TypeError {
                            path: path.to_string(),
                            detail: format!("{n} is above the declared upper bound {hi}"),
                        });
                    }
                }
            }
        },
        DimType::Record { fields } => match value.as_object() {
            None => mismatch(errors, "record"),
            Some(map) => {
                for (name, field_ty) in fields {
                    match map.get(name) {
                        None => errors.push(TypeError {
                            path: format!("{path}.{name}"),
                            detail: "declared dimension is absent from state".into(),
                        }),
                        Some(v) => check_value(v, field_ty, &format!("{path}.{name}"), errors),
                    }
                }
            }
        },
        DimType::Map { value: vty } => match value.as_object() {
            None => mismatch(errors, "map"),
            Some(map) => {
                for (k, v) in map {
                    check_value(v, vty, &format!("{path}.{k}"), errors);
                }
            }
        },
        DimType::List { item } => match value.as_array() {
            None => mismatch(errors, "list"),
            Some(items) => {
                for (i, v) in items.iter().enumerate() {
                    check_value(v, item, &format!("{path}[{i}]"), errors);
                }
            }
        },
    }
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "text",
        Value::Array(_) => "list",
        Value::Object(_) => "record",
    }
}

/// Organisational closure: does the transition preserve the shape?
///
/// `schema(o(s)) = schema(s)`. Values may change; the structure may not. An
/// operator that introduces or drops a dimension is changing what the boundary
/// IS, which is a different class of act and belongs to a higher gate.
///
/// Open-map keys are exempt: a new pocket is a value, not a new dimension —
/// the schema declared "a map of pockets", and it still is one.
pub fn preserves_shape(before: &Value, after: &Value, schema: &Schema) -> Result<(), TypeError> {
    for (name, ty) in &schema.dimensions {
        let b = before.get(name);
        let a = after.get(name);
        match (b, a) {
            (Some(b), Some(a)) => shape_eq(b, a, ty, name)?,
            (None, None) => {}
            (Some(_), None) => {
                return Err(TypeError {
                    path: name.clone(),
                    detail: "operator removed a declared dimension".into(),
                })
            }
            (None, Some(_)) => {
                return Err(TypeError {
                    path: name.clone(),
                    detail: "operator introduced a declared dimension".into(),
                })
            }
        }
    }
    Ok(())
}

fn shape_eq(before: &Value, after: &Value, ty: &DimType, path: &str) -> Result<(), TypeError> {
    match ty {
        // An open map's keys ARE its values: adding a pocket is not a
        // structural change.
        DimType::Map { value } => {
            if !after.is_object() {
                return Err(TypeError {
                    path: path.into(),
                    detail: "operator changed a map into something else".into(),
                });
            }
            if let (Some(b), Some(a)) = (before.as_object(), after.as_object()) {
                for (k, av) in a {
                    if let Some(bv) = b.get(k) {
                        shape_eq(bv, av, value, &format!("{path}.{k}"))?;
                    }
                }
            }
            Ok(())
        }
        DimType::Record { fields } => {
            let (Some(b), Some(a)) = (before.as_object(), after.as_object()) else {
                return Err(TypeError {
                    path: path.into(),
                    detail: "operator changed a record into something else".into(),
                });
            };
            for name in fields.keys() {
                match (b.contains_key(name), a.contains_key(name)) {
                    (true, false) => {
                        return Err(TypeError {
                            path: format!("{path}.{name}"),
                            detail: "operator removed a declared dimension".into(),
                        })
                    }
                    (false, true) => {
                        return Err(TypeError {
                            path: format!("{path}.{name}"),
                            detail: "operator introduced a declared dimension".into(),
                        })
                    }
                    _ => {}
                }
                if let (Some(bv), Some(av)) = (b.get(name), a.get(name)) {
                    shape_eq(bv, av, &fields[name], &format!("{path}.{name}"))?;
                }
            }
            Ok(())
        }
        DimType::Any => Ok(()),
        _ => {
            // A scalar or list may change value freely, but not category.
            if kind_of(before) != kind_of(after) {
                return Err(TypeError {
                    path: path.into(),
                    detail: format!(
                        "operator changed the type of a dimension: {} became {}",
                        kind_of(before),
                        kind_of(after)
                    ),
                });
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate::parse_predicate;
    use serde_json::json;

    fn pocket() -> DimType {
        DimType::Record {
            fields: BTreeMap::from([
                ("allocated".into(), DimType::Number { lo: Some(0.0), hi: None }),
                ("spent".into(), DimType::Number { lo: Some(0.0), hi: None }),
            ]),
        }
    }

    fn finances() -> Schema {
        Schema::new().declare(
            "finances",
            DimType::Record {
                fields: BTreeMap::from([
                    (
                        "liquid".into(),
                        DimType::Record {
                            fields: BTreeMap::from([(
                                "balance".into(),
                                DimType::Number { lo: None, hi: None },
                            )]),
                        },
                    ),
                    ("pockets".into(), DimType::Map { value: Box::new(pocket()) }),
                ]),
            },
        )
    }

    fn state() -> Value {
        json!({"finances":{"liquid":{"balance":100.0},
                           "pockets":{"food":{"allocated":10.0,"spent":2.0}}}})
    }

    #[test]
    fn a_predicate_over_declared_dimensions_binds_cleanly() {
        let p = parse_predicate("finances.liquid.balance >= 0").unwrap();
        assert!(bind(&p, &finances()).is_empty());
    }

    #[test]
    fn an_undeclared_dimension_is_a_load_time_type_error() {
        // The whole point: caught when the spec loads, not as a silent
        // comparison against nothing at 2 a.m.
        let p = parse_predicate("finances.savings.balance >= 0").unwrap();
        let errors = bind(&p, &finances());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "finances.savings.balance");
    }

    #[test]
    fn a_quantifier_over_an_open_map_binds() {
        let p = parse_predicate("ALL finances.pockets[*].allocated >= 0").unwrap();
        assert!(bind(&p, &finances()).is_empty(), "pocket names are values, not dimensions");
    }

    #[test]
    fn an_undeclared_field_inside_a_quantifier_is_caught() {
        let p = parse_predicate("ALL finances.pockets[*].nonsense >= 0").unwrap();
        assert!(!bind(&p, &finances()).is_empty());
    }

    #[test]
    fn aggregates_are_bound_too() {
        let ok = parse_predicate("SUM(finances.pockets[*].allocated) >= 0").unwrap();
        assert!(bind(&ok, &finances()).is_empty());
        let bad = parse_predicate("SUM(finances.pockets[*].ghost) >= 0").unwrap();
        assert!(!bind(&bad, &finances()).is_empty());
    }

    #[test]
    fn a_conforming_state_validates() {
        assert!(validate(&state(), &finances()).is_empty());
    }

    #[test]
    fn a_wrong_type_is_reported() {
        let s = json!({"finances":{"liquid":{"balance":"lots"},"pockets":{}}});
        let errors = validate(&s, &finances());
        assert!(errors.iter().any(|e| e.path == "finances.liquid.balance"));
    }

    #[test]
    fn declared_bounds_are_checked() {
        let s = json!({"finances":{"liquid":{"balance":1.0},
                                   "pockets":{"food":{"allocated":-5.0,"spent":0.0}}}});
        let errors = validate(&s, &finances());
        assert!(errors.iter().any(|e| e.detail.contains("lower bound")));
    }

    #[test]
    fn organisational_closure_allows_a_value_change() {
        let after = json!({"finances":{"liquid":{"balance":50.0},
                                       "pockets":{"food":{"allocated":10.0,"spent":2.0}}}});
        assert!(preserves_shape(&state(), &after, &finances()).is_ok());
    }

    #[test]
    fn organisational_closure_allows_a_new_pocket() {
        // An open map's keys are values. A new pocket is not a new dimension.
        let after = json!({"finances":{"liquid":{"balance":100.0},
                                       "pockets":{"food":{"allocated":10.0,"spent":2.0},
                                                  "rent":{"allocated":0.0,"spent":0.0}}}});
        assert!(preserves_shape(&state(), &after, &finances()).is_ok());
    }

    #[test]
    fn organisational_closure_refuses_a_removed_dimension() {
        let after = json!({"finances":{"pockets":{"food":{"allocated":10.0,"spent":2.0}}}});
        let err = preserves_shape(&state(), &after, &finances()).unwrap_err();
        assert!(err.detail.contains("removed"), "got: {err}");
    }

    #[test]
    fn organisational_closure_refuses_a_changed_type() {
        let after = json!({"finances":{"liquid":{"balance":"gone"},
                                       "pockets":{"food":{"allocated":10.0,"spent":2.0}}}});
        let err = preserves_shape(&state(), &after, &finances()).unwrap_err();
        assert!(err.detail.contains("changed the type"), "got: {err}");
    }

    #[test]
    fn any_is_an_admission_and_absorbs_what_follows() {
        let s = Schema::new().declare("scratch", DimType::Any);
        let p = parse_predicate("scratch.whatever.deep == 1").unwrap();
        assert!(bind(&p, &s).is_empty(), "an untyped region cannot disprove a path inside it");
    }
}
