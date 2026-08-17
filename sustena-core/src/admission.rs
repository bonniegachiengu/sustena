//! The admission model — `admit(o, s)` (LAW / the Constraint paper, §IV, §VII).
//!
//! ```text
//!   admit(o, s) ⟺ g_o(s) ∧ o(s) ∈ A ∧ D(s, o(s))
//!                  guard     result     step legal
//! ```
//!
//! R1 implemented the first two conjuncts with a single outcome: refuse. The
//! paper specifies three strategies per constraint, and says plainly that the
//! gate's codomain is therefore **not boolean**:
//!
//! ```text
//!   strategy(C) ∈ { refuse, clamp, defer }
//! ```
//!
//! ## Refuse
//!
//! The transition does not commit; state is unchanged; the rule that refused
//! is **named**. The naming is the point — "which law refused" is the
//! difference between an explanation and a shrug.
//!
//! ## Clamp — projection onto the admissible set, Π_A
//!
//! Capping a spend at the available balance genuinely *is* the exact
//! projection, because the per-dimension box of V is convex. The paper attaches
//! three warnings, and each is enforced here rather than noted:
//!
//! 1. **V is not convex in general.** Clamping is declarable only on a
//!    constraint whose feasible set is an interval; declared anywhere else it
//!    is rejected at authoring time, not silently downgraded.
//! 2. **Clamping and conservation interact badly.** Capping a transfer's
//!    outflow while leaving its inflow untouched *creates money* — it satisfies
//!    C by violating D. A clamp on a conserved dimension is rejected at
//!    authoring time.
//! 3. **A clamp commits something nobody asked for.** The person requested s′
//!    and the system performed Π_A(s′). That is a different act and must be
//!    reported as such — every time, in the event and in the interface. A
//!    [`Verdict::Clamped`] therefore carries what was asked for alongside what
//!    was done; it is not representable without that.
//!
//! The paper is also explicit that a clamp *relocates* a violation rather than
//! dissolving one. Nothing here pretends otherwise.
//!
//! ## Defer
//!
//! Neither committed nor refused: it becomes a proposal for the Council or a
//! person (Sheridan & Verplank level 5 — executed only if the human approves).
//!
//! Deferral converts a synchronous transition into a pending one, so the state
//! at approval time need not be the state at proposal time. A deferred proposal
//! must therefore be **re-admitted at commit**, never admitted once at
//! proposal. That is the easiest place to lose complete mediation, so
//! [`Deferred::requires_readmission`] exists to make skipping it a visible
//! choice rather than an oversight.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::predicate::{self, ast::Operand, ast::Predicate, parse_predicate};

/// What a constraint does when it is not satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    /// Do not commit; name the rule. The default, and the only safe choice
    /// when nothing more specific has been established.
    ///
    /// The `#[default]` is deliberate and load-bearing: an undeclared strategy
    /// must refuse, never clamp. It is marked on the variant so the safe
    /// choice is visible where the variants are read.
    #[default]
    Refuse,
    /// Project onto the admissible interval. Only legal on interval-shaped,
    /// non-conserved constraints — see [`typecheck_constraint`].
    Clamp,
    /// Route to a person or the Council instead of deciding.
    Defer,
}

/// A declared constraint: the rule, what it is called, and what happens when it
/// is not met.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintDecl {
    pub id: String,
    pub expression: String,
    #[serde(default)]
    pub strategy: Strategy,
    /// Dimensions that must be conserved across a transition. A clamp on any of
    /// these is rejected at authoring time: capping one side of a transfer
    /// without the other creates value out of nothing.
    #[serde(default)]
    pub conserved_dimensions: Vec<String>,
}

/// The gate's answer. Deliberately not a boolean.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Commit as requested.
    Admit,
    /// Do not commit. `rule` names which law refused.
    Refused { rule: String, reason: String },
    /// Committed, but NOT what was asked for. Both are carried so the
    /// difference can be reported every time.
    Clamped {
        rule: String,
        path: String,
        requested: Value,
        committed: Value,
    },
    /// Neither committed nor refused — awaiting a human or the Council.
    Deferred(Deferred),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Deferred {
    pub rule: String,
    pub reason: String,
    /// Always true. The state at approval time need not be the state at
    /// proposal time, so a deferred proposal is re-admitted at commit rather
    /// than trusting the admission it passed when it was raised.
    pub requires_readmission: bool,
}

/// Why a constraint declaration is not well-formed.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclError {
    Unparseable { id: String, detail: String },
    /// Clamp declared on a rule whose feasible set is not an interval.
    ClampOnNonInterval { id: String, detail: String },
    /// Clamp declared on a dimension the sustain conserves.
    ClampOnConserved { id: String, dimension: String },
}

impl std::fmt::Display for DeclError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeclError::Unparseable { id, detail } => {
                write!(f, "constraint '{id}' does not parse: {detail}")
            }
            DeclError::ClampOnNonInterval { id, detail } => write!(
                f,
                "constraint '{id}' declares clamp, but its feasible set is not an interval ({detail}). \
                 Projection is only well-defined on a convex set; clamping here would commit an \
                 arbitrary nearby state rather than the nearest one."
            ),
            DeclError::ClampOnConserved { id, dimension } => write!(
                f,
                "constraint '{id}' declares clamp on conserved dimension '{dimension}'. \
                 Capping one side of a conserved transition without the other creates value \
                 out of nothing — it satisfies the constraint by violating conservation."
            ),
        }
    }
}

/// Check a declaration is well-formed BEFORE it can ever run.
///
/// Authoring time, not evaluation time: an ill-formed clamp must be impossible
/// to install, not merely refused later when it is too late to say why.
pub fn typecheck_constraint(decl: &ConstraintDecl) -> Result<Predicate, DeclError> {
    let node = parse_predicate(&decl.expression).map_err(|e| DeclError::Unparseable {
        id: decl.id.clone(),
        detail: e.to_string(),
    })?;

    if decl.strategy == Strategy::Clamp {
        let bound = interval_bound(&node).ok_or_else(|| DeclError::ClampOnNonInterval {
            id: decl.id.clone(),
            detail: "expected a single comparison of one state path against a bound".into(),
        })?;

        if decl
            .conserved_dimensions
            .iter()
            .any(|d| d == &bound.path || bound.path.starts_with(&format!("{d}.")))
        {
            return Err(DeclError::ClampOnConserved {
                id: decl.id.clone(),
                dimension: bound.path,
            });
        }
    }

    Ok(node)
}

/// A constraint that bounds one path on one side — the only shape that is an
/// interval, and therefore the only shape a clamp may be declared on.
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalBound {
    pub path: String,
    /// True when the constraint is an upper bound (`x <= k`, `x < k`).
    pub upper: bool,
    pub inclusive: bool,
}

fn interval_bound(node: &Predicate) -> Option<IntervalBound> {
    use crate::predicate::ast::CompOp;
    match node {
        Predicate::Comparison { left, op, right } => {
            let path = match left {
                Operand::Path(p) => p.raw.clone(),
                _ => return None,
            };
            // The other side must be a fixed bound, not another state reading:
            // an interval has a constant edge.
            match right {
                Operand::Literal(_) | Operand::Param(_) => {}
                _ => return None,
            }
            match op {
                CompOp::Le => Some(IntervalBound { path, upper: true, inclusive: true }),
                CompOp::Lt => Some(IntervalBound { path, upper: true, inclusive: false }),
                CompOp::Ge => Some(IntervalBound { path, upper: false, inclusive: true }),
                CompOp::Gt => Some(IntervalBound { path, upper: false, inclusive: false }),
                // Equality pins a point, not an interval; projecting onto it
                // would overwrite rather than cap.
                CompOp::Eq | CompOp::Ne => None,
            }
        }
        _ => None,
    }
}

/// Evaluate one constraint against a candidate state and apply its strategy.
///
/// `candidate` is the state the operator produced — the paper's `o(s)`. The
/// effect has already run on a copy; only an admitted candidate becomes
/// durable. That ordering is what makes a result-check safe to have at all:
/// the alternative, mutate-then-repair, needs a rollback that is correct, and
/// correct rollback is strictly harder than never committing.
pub fn admit_one(
    decl: &ConstraintDecl,
    node: &Predicate,
    candidate: &Value,
    params: &Map<String, Value>,
) -> Verdict {
    let (ok, reason) = predicate::evaluate(node, candidate, params);
    if ok {
        return Verdict::Admit;
    }

    match decl.strategy {
        Strategy::Refuse => Verdict::Refused { rule: decl.id.clone(), reason },

        Strategy::Defer => Verdict::Deferred(Deferred {
            rule: decl.id.clone(),
            reason,
            requires_readmission: true,
        }),

        Strategy::Clamp => {
            // typecheck_constraint has already established this is an interval
            // on a non-conserved dimension.
            let Some(bound) = interval_bound(node) else {
                return Verdict::Refused { rule: decl.id.clone(), reason };
            };
            let Predicate::Comparison { right, .. } = node else {
                return Verdict::Refused { rule: decl.id.clone(), reason };
            };

            let edge = match right {
                Operand::Literal(v) => v.clone(),
                Operand::Param(k) => params.get(k).cloned().unwrap_or(Value::Null),
                _ => return Verdict::Refused { rule: decl.id.clone(), reason },
            };
            let Some(edge_n) = edge.as_f64() else {
                return Verdict::Refused { rule: decl.id.clone(), reason };
            };

            let requested = read_path(candidate, &bound.path).unwrap_or(Value::Null);
            if requested.as_f64().is_none() {
                return Verdict::Refused { rule: decl.id.clone(), reason };
            }

            // Π_A on one axis: move to the edge, no further. Exclusive bounds
            // have no nearest admissible point, so they cannot be clamped —
            // refusing is the honest answer rather than inventing an epsilon.
            if !bound.inclusive {
                return Verdict::Refused { rule: decl.id.clone(), reason };
            }

            Verdict::Clamped {
                rule: decl.id.clone(),
                path: bound.path,
                requested,
                committed: number(edge_n),
            }
        }
    }
}

fn read_path(root: &Value, path: &str) -> Option<Value> {
    let mut node = root;
    for seg in path.split('.') {
        node = node.get(seg)?;
    }
    Some(node.clone())
}

fn number(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        Value::from(v as i64)
    } else {
        Value::from(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decl(id: &str, expr: &str, strategy: Strategy) -> ConstraintDecl {
        ConstraintDecl {
            id: id.into(),
            expression: expr.into(),
            strategy,
            conserved_dimensions: vec![],
        }
    }

    fn check(d: &ConstraintDecl, state: Value) -> Verdict {
        let node = typecheck_constraint(d).expect("declaration should typecheck");
        admit_one(d, &node, &state, &Map::new())
    }

    #[test]
    fn a_satisfied_constraint_admits() {
        let d = decl("liquid", "finances.liquid.balance >= 0", Strategy::Refuse);
        assert_eq!(check(&d, json!({"finances":{"liquid":{"balance":5}}})), Verdict::Admit);
    }

    #[test]
    fn refuse_names_the_rule_that_refused() {
        let d = decl("liquid_non_negative", "finances.liquid.balance >= 0", Strategy::Refuse);
        match check(&d, json!({"finances":{"liquid":{"balance":-1}}})) {
            Verdict::Refused { rule, .. } => assert_eq!(rule, "liquid_non_negative"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn clamp_projects_to_the_edge_and_reports_both_values() {
        let d = decl("floor", "finances.liquid.balance >= 0", Strategy::Clamp);
        match check(&d, json!({"finances":{"liquid":{"balance":-40}}})) {
            Verdict::Clamped { requested, committed, path, .. } => {
                assert_eq!(path, "finances.liquid.balance");
                assert_eq!(requested, json!(-40));
                assert_eq!(committed, json!(0), "Π_A moves to the edge, no further");
            }
            other => panic!("expected a clamp, got {other:?}"),
        }
    }

    #[test]
    fn clamp_on_a_non_interval_is_rejected_at_authoring_time() {
        // Two paths compared against each other is not a box.
        let d = decl("cross", "finances.a >= finances.b", Strategy::Clamp);
        assert!(matches!(
            typecheck_constraint(&d),
            Err(DeclError::ClampOnNonInterval { .. })
        ));
    }

    #[test]
    fn clamp_on_an_equality_is_rejected() {
        // Equality pins a point; projecting onto it overwrites rather than caps.
        let d = decl("pinned", "finances.total == 100", Strategy::Clamp);
        assert!(matches!(
            typecheck_constraint(&d),
            Err(DeclError::ClampOnNonInterval { .. })
        ));
    }

    #[test]
    fn clamp_on_a_conserved_dimension_is_rejected() {
        // Capping one side of a transfer creates money.
        let d = ConstraintDecl {
            id: "cap".into(),
            expression: "finances.liquid.balance >= 0".into(),
            strategy: Strategy::Clamp,
            conserved_dimensions: vec!["finances.liquid".into()],
        };
        match typecheck_constraint(&d) {
            Err(DeclError::ClampOnConserved { dimension, .. }) => {
                assert_eq!(dimension, "finances.liquid.balance");
            }
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    #[test]
    fn an_exclusive_bound_refuses_rather_than_inventing_an_epsilon() {
        let d = decl("strict", "finances.liquid.balance > 0", Strategy::Clamp);
        assert!(matches!(
            check(&d, json!({"finances":{"liquid":{"balance":-1}}})),
            Verdict::Refused { .. }
        ));
    }

    #[test]
    fn defer_is_a_third_verdict_and_demands_readmission() {
        let d = decl("big_spend", "finances.liquid.balance >= 0", Strategy::Defer);
        match check(&d, json!({"finances":{"liquid":{"balance":-1}}})) {
            Verdict::Deferred(d) => {
                assert!(d.requires_readmission, "state can move between proposal and approval");
                assert_eq!(d.rule, "big_spend");
            }
            other => panic!("expected a deferral, got {other:?}"),
        }
    }

    #[test]
    fn refuse_is_the_default_strategy() {
        let d: ConstraintDecl = serde_json::from_value(json!({
            "id": "x", "expression": "a >= 0"
        }))
        .unwrap();
        assert_eq!(d.strategy, Strategy::Refuse);
    }

    #[test]
    fn an_unparseable_declaration_is_rejected_not_skipped() {
        let d = decl("broken", "this is (not ) valid", Strategy::Refuse);
        assert!(matches!(typecheck_constraint(&d), Err(DeclError::Unparseable { .. })));
    }
}
