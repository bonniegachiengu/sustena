//! The Operator primitive — guarded transitions (ENZYME).
//!
//! An operator is a *declared, gated* move. It never simply changes state: it
//! states what must be true before it runs, what it may emit, and what must
//! still hold afterwards. The engine enforces all three.
//!
//! ## The execution contract
//!
//! ```text
//!   guard  →  effect  →  gate  →  commit
//! ```
//!
//! 1. **guard** — the operator's own declared pre-conditions. Fail here and
//!    nothing runs at all.
//! 2. **effect** — the operator body, which may only touch state through the
//!    recording API.
//! 3. **gate** — the sustain's invariants and the operator's declared
//!    post-conditions, evaluated against the MUTATED state **before anything
//!    persists** (LAW / the Constraint paper: `admit(o,s)`).
//! 4. **commit** — only reached when the gate passes.
//!
//! **A refusal changes nothing.** No partial write, no event, no half-applied
//! transfer. That property is why the gate runs before persistence rather than
//! after, and it is asserted directly by the conformance vectors.
//!
//! ## No I/O here
//!
//! This module decides *what should happen*; it does not write anything. An
//! [`Execution`] carries the verdict, the mutations and the events, and the
//! host persists them in one transaction. Keeping persistence out is what lets
//! the same core run on a phone, a desktop and a server — and it means the
//! gate cannot be bypassed by a second write path, which is exactly how the
//! reference engine grew one.

pub mod budget;
pub mod meta;

use serde_json::{Map, Value};

pub use meta::{OperatorFn, OperatorMeta, OperatorResult, Registry};

use crate::mutation::Mutation;
use crate::predicate;
use crate::state::State;

/// One published event. The host assigns durable ordering (`seq`) and writes
/// it together with the mutations; the core only says what happened.
#[derive(Debug, Clone, PartialEq)]
pub struct EmittedEvent {
    pub name: String,
    pub payload: Value,
}

/// Everything one operator call decided.
#[derive(Debug, Clone)]
pub struct Execution {
    pub result: OperatorResult,
    /// Empty whenever the call did not commit — a refusal changes nothing.
    pub mutations: Vec<Mutation>,
    pub events: Vec<EmittedEvent>,
    /// Resulting state, or the untouched original if refused.
    pub state: Value,
}

impl Execution {
    pub fn committed(&self) -> bool {
        self.result.is_ok()
    }
}

/// The sustain's own rules, as the gate needs them.
#[derive(Debug, Clone, Default)]
pub struct Enforcement {
    /// Whether the gate is armed for this sustain (`enforcement.enabled`).
    pub enabled: bool,
    /// `(id, expression)` — evaluated against the mutated state before commit.
    pub invariants: Vec<(String, String)>,
}

/// Run one operator end to end: guard → effect → gate → commit.
pub fn execute(
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    operator_name: &str,
    params: &Map<String, Value>,
) -> Execution {
    let untouched = || Execution {
        result: OperatorResult::fail(
            format!("Operator '{operator_name}' is not available on this sustain."),
            "operator_allowed",
        ),
        mutations: vec![],
        events: vec![],
        state: state.clone(),
    };

    // The sustain's allow-list comes first: an operator that exists globally
    // is still not permitted here unless this sustain declared it.
    if !allowed.iter().any(|a| a == operator_name) {
        return untouched();
    }

    let meta = match registry.get(operator_name) {
        Some(m) => m,
        None => {
            return Execution {
                result: OperatorResult::fail(
                    format!("Operator '{operator_name}' is not registered."),
                    "operator_registered",
                ),
                mutations: vec![],
                events: vec![],
                state: state.clone(),
            }
        }
    };

    // ── guard ────────────────────────────────────────────────────────────────
    for guard in &meta.constraints {
        match predicate::check(guard, state, params) {
            Err(e) => {
                return Execution {
                    result: OperatorResult::fail(
                        format!("Guard '{guard}' could not be parsed: {e}"),
                        "guard_unparseable",
                    ),
                    mutations: vec![],
                    events: vec![],
                    state: state.clone(),
                }
            }
            Ok((false, reason)) => {
                return Execution {
                    result: OperatorResult::fail(
                        format!("Constraint failed: {reason}"),
                        guard.clone(),
                    ),
                    mutations: vec![],
                    events: vec![],
                    state: state.clone(),
                }
            }
            Ok((true, _)) => {}
        }
    }

    // ── effect ───────────────────────────────────────────────────────────────
    let mut working = State::new(state.clone());
    let mut events: Vec<EmittedEvent> = Vec::new();
    let result = (meta.run)(&mut working, params, &mut events);

    if !result.is_ok() {
        // The operator refused on its own terms. Discard everything it touched:
        // a refusal must leave no trace, even a partial one.
        return Execution {
            result,
            mutations: vec![],
            events: vec![],
            state: state.clone(),
        };
    }

    // ── gate ─────────────────────────────────────────────────────────────────
    let candidate = working.snapshot();

    if enforcement.enabled {
        for (id, expr) in &enforcement.invariants {
            match predicate::check(expr, &candidate, params) {
                // A rule that will not compile is reported, never skipped.
                // Skipping is how a gate fails OPEN — open gap on the article
                // backlog, and not a behaviour worth carrying forward.
                Err(e) => {
                    return Execution {
                        result: OperatorResult::fail(
                            format!("invariant '{id}' ({expr}) could not be parsed: {e}"),
                            "invariant_unparseable",
                        ),
                        mutations: vec![],
                        events: vec![],
                        state: state.clone(),
                    }
                }
                Ok((false, reason)) => {
                    return Execution {
                        result: OperatorResult::fail(
                            format!("would violate invariant '{id}' ({expr}): {reason}"),
                            "enforcement_gate",
                        ),
                        mutations: vec![],
                        events: vec![],
                        state: state.clone(),
                    }
                }
                Ok((true, _)) => {}
            }
        }
    }

    for post in &meta.post_constraints {
        match predicate::check(post, &candidate, params) {
            Err(e) => {
                return Execution {
                    result: OperatorResult::fail(
                        format!("post-condition '{post}' could not be parsed: {e}"),
                        "post_constraint_unparseable",
                    ),
                    mutations: vec![],
                    events: vec![],
                    state: state.clone(),
                }
            }
            Ok((false, reason)) => {
                return Execution {
                    result: OperatorResult::fail(
                        format!("would violate post-condition ({post}): {reason}"),
                        "enforcement_gate",
                    ),
                    mutations: vec![],
                    events: vec![],
                    state: state.clone(),
                }
            }
            Ok((true, _)) => {}
        }
    }

    // ── commit ───────────────────────────────────────────────────────────────
    Execution {
        result,
        mutations: working.mutations().to_vec(),
        events,
        state: candidate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn homestead_state() -> Value {
        json!({"finances":{"liquid":{"balance":1000.0},"pockets":{},
                           "income":{"monthly_total":0.0,"sources":[]}}})
    }

    fn allowed() -> Vec<String> {
        Registry::default().names()
    }

    fn armed() -> Enforcement {
        Enforcement {
            enabled: true,
            invariants: vec![
                ("liquid_non_negative".into(), "finances.liquid.balance >= 0".into()),
                ("pocket_allocated_non_negative".into(),
                 "ALL finances.pockets[*].allocated >= 0".into()),
            ],
        }
    }

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn a_guard_failure_runs_nothing() {
        let reg = Registry::default();
        let ex = execute(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(5000.0)),
                                   ("period", json!("monthly"))]));
        assert!(!ex.committed(), "guard should refuse: 5000 > 1000 balance");
        assert!(ex.mutations.is_empty());
        assert!(ex.events.is_empty());
        assert_eq!(ex.state, homestead_state(), "a refusal must change nothing");
    }

    #[test]
    fn a_permitted_call_commits_with_its_events() {
        let reg = Registry::default();
        let ex = execute(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(250.0)),
                                   ("period", json!("monthly"))]));
        assert!(ex.committed());
        assert_eq!(ex.state["finances"]["liquid"]["balance"], json!(750.0));
        assert_eq!(ex.state["finances"]["pockets"]["food"]["allocated"], json!(250.0));
        assert_eq!(ex.events.len(), 1);
        assert!(!ex.mutations.is_empty());
    }

    #[test]
    fn an_operator_the_sustain_did_not_declare_is_refused() {
        let reg = Registry::default();
        let ex = execute(&reg, &["budget.summary".to_string()], &armed(),
                         &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(1.0)),
                                   ("period", json!("monthly"))]));
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("operator_allowed"));
    }

    #[test]
    fn the_gate_refuses_a_transition_that_would_break_an_invariant() {
        // A pocket forced negative directly — the operator's own guard would
        // not catch it, so only the gate stands between this and the ledger.
        let reg = Registry::default();
        let state = json!({"finances":{"liquid":{"balance":100.0},
                                       "pockets":{"food":{"allocated":10.0,"spent":0.0,"limit":0.0}},
                                       "income":{"monthly_total":0.0,"sources":[]}}});
        let ex = execute(&reg, &allowed(), &armed(), &state, "test.force_negative",
                         &params(&[]));
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("enforcement_gate"));
        assert_eq!(ex.state, state, "refused transitions leave state untouched");
    }

    #[test]
    fn an_unarmed_sustain_is_not_gated() {
        let reg = Registry::default();
        let state = json!({"finances":{"liquid":{"balance":100.0},"pockets":{},
                                       "income":{"monthly_total":0.0,"sources":[]}}});
        let off = Enforcement { enabled: false, ..armed() };
        let ex = execute(&reg, &allowed(), &off, &state, "test.force_negative", &params(&[]));
        assert!(ex.committed(), "with the gate disarmed this is allowed through");
    }

    #[test]
    fn a_rule_that_will_not_compile_refuses_rather_than_failing_open() {
        let reg = Registry::default();
        let broken = Enforcement {
            enabled: true,
            invariants: vec![("broken".into(), "this is (not ) valid".into())],
        };
        let ex = execute(&reg, &allowed(), &broken, &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(1.0)),
                                   ("period", json!("monthly"))]));
        assert!(!ex.committed(), "an unreadable rule must not be silently skipped");
    }
}
