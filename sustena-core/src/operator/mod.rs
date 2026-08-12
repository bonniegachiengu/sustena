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
use crate::principal::{permitted, Denial, Memberships, Tier};
use crate::predicate;
use crate::schema::{preserves_shape, Schema};
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
    /// The declared state space. When present, organisational closure is
    /// enforced: an operator may change values, never the shape.
    pub schema: Option<Schema>,
}

/// Who is acting, for the `permitted(α, o, Σ)` conjunct.
///
/// `Unchecked` exists so that skipping authorization is a VISIBLE choice at the
/// call site rather than a silent default. The Immune paper's audit is
/// specifically about permissive defaults: a second write path whose gate was
/// `check_gate=False` by default, adopted to preserve existing callers, and
/// then exercised in-tree as a tool for reaching states the gate forbids. A
/// bypass that reads as ordinary is the problem; one that has to be named is
/// not.
#[derive(Debug, Clone)]
pub enum Authorization<'a> {
    /// No principal model in force — the host has already decided.
    Unchecked,
    /// Check `permitted(α, o, Σ)` across the whole nesting path.
    Principal {
        id: &'a str,
        memberships: &'a Memberships,
        /// Outermost containing sustain first, target last.
        path: &'a [String],
    },
}

/// Run one operator end to end.
///
/// ```text
///   auth → permitted → guard → effect → gate → commit
/// ```
///
/// The order follows the Immune paper's extended admit(): authority is decided
/// before the guard, because "may this principal act here" does not depend on
/// state and should not cost an evaluation of it.
pub fn execute(
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    operator_name: &str,
    params: &Map<String, Value>,
) -> Execution {
    execute_as(
        registry, allowed, enforcement, state, operator_name, params,
        &Authorization::Unchecked,
    )
}

/// [`execute`], with the principal named.
pub fn execute_as(
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    operator_name: &str,
    params: &Map<String, Value>,
    authorization: &Authorization,
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

    // ── permitted(α, o, Σ) ───────────────────────────────────────────────────
    // The conjunct the reference declares on every operator and reads nowhere.
    if let Authorization::Principal { id, memberships, path } = authorization {
        if let Err(denial) = permitted(memberships, id, path, meta.min_privilege as Tier) {
            let rule = match denial {
                Denial::NoEdge { .. } => "not_a_member",
                Denial::InsufficientTier { .. } => "insufficient_privilege",
            };
            return Execution {
                result: OperatorResult::fail(denial.to_string(), rule),
                mutations: vec![],
                events: vec![],
                state: state.clone(),
            };
        }
    }

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

    // Organisational closure (CELL §V): schema(o(s)) = schema(s).
    //
    // A sustain is materially open — money and messages cross it constantly —
    // and organisationally closed: an ordinary operator changes values inside
    // the boundary, never what the boundary IS. Adding a member or adopting a
    // child sustain is a different class of act, and belongs to a higher gate.
    //
    // Checked before the invariants, because a state of the wrong shape cannot
    // be meaningfully judged against rules written for the right one.
    if let Some(schema) = &enforcement.schema {
        if let Err(err) = preserves_shape(state, &candidate, schema) {
            return Execution {
                result: OperatorResult::fail(
                    format!("would change the sustain's shape, not just its values: {err}"),
                    "organisational_closure",
                ),
                mutations: vec![],
                events: vec![],
                state: state.clone(),
            };
        }
    }

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
            schema: None,
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

    // ── authorization at the gate (R2 #10) ──────────────────────────────────

    fn memberships() -> crate::principal::Memberships {
        use crate::principal::{MembershipEdge, Memberships, TIER_MEMBER, TIER_OBSERVER, TIER_OWNER};
        let mut m = Memberships::new();
        m.grant(MembershipEdge { principal: "bonnie".into(), sustain: "house".into(), tier: TIER_OWNER, skin: None });
        m.grant(MembershipEdge { principal: "cira".into(), sustain: "house".into(), tier: TIER_MEMBER, skin: None });
        m.grant(MembershipEdge { principal: "guest".into(), sustain: "house".into(), tier: TIER_OBSERVER, skin: None });
        m
    }

    fn as_principal<'a>(id: &'a str, m: &'a crate::principal::Memberships, path: &'a [String]) -> Authorization<'a> {
        Authorization::Principal { id, memberships: m, path }
    }

    #[test]
    fn a_member_may_run_a_member_level_operator() {
        let (reg, m) = (Registry::default(), memberships());
        let path = vec!["house".to_string()];
        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &params(&[("pocket_name", json!("food")), ("amount", json!(10.0)),
                                      ("period", json!("monthly"))]),
                            &as_principal("cira", &m, &path));
        assert!(ex.committed());
    }

    #[test]
    fn an_observer_is_refused_before_the_guard_ever_runs() {
        let (reg, m) = (Registry::default(), memberships());
        let path = vec!["house".to_string()];
        // The amount would pass the guard easily; authority is decided first.
        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &params(&[("pocket_name", json!("food")), ("amount", json!(10.0)),
                                      ("period", json!("monthly"))]),
                            &as_principal("guest", &m, &path));
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("insufficient_privilege"));
        assert_eq!(ex.state, homestead_state(), "a refusal changes nothing");
    }

    #[test]
    fn a_stranger_gets_a_different_answer_from_an_under_privileged_member() {
        let (reg, m) = (Registry::default(), memberships());
        let path = vec!["house".to_string()];
        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &params(&[("pocket_name", json!("food")), ("amount", json!(10.0)),
                                      ("period", json!("monthly"))]),
                            &as_principal("nobody", &m, &path));
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("not_a_member"),
                   "'not a member here' and 'a member who may not do this' are different answers");
    }

    #[test]
    fn nesting_shrinks_what_a_principal_may_do() {
        use crate::principal::{MembershipEdge, Memberships, TIER_OBSERVER, TIER_OWNER};
        let mut m = Memberships::new();
        m.grant(MembershipEdge { principal: "epha".into(), sustain: "house".into(), tier: TIER_OBSERVER, skin: None });
        m.grant(MembershipEdge { principal: "epha".into(), sustain: "habitat".into(), tier: TIER_OWNER, skin: None });
        let reg = Registry::default();

        let direct = vec!["habitat".to_string()];
        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &params(&[("pocket_name", json!("food")), ("amount", json!(10.0)),
                                      ("period", json!("monthly"))]),
                            &as_principal("epha", &m, &direct));
        assert!(ex.committed(), "owner of the habitat, acting on the habitat");

        let nested = vec!["house".to_string(), "habitat".to_string()];
        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &params(&[("pocket_name", json!("food")), ("amount", json!(10.0)),
                                      ("period", json!("monthly"))]),
                            &as_principal("epha", &m, &nested));
        assert!(!ex.committed(), "adding a containing sustain can only shrink authority");
    }

    #[test]
    fn unchecked_authorization_is_a_named_choice_not_a_silent_default() {
        // execute() delegates to execute_as(.., Unchecked). The bypass exists,
        // but it has a name — which is the whole point.
        let (reg, _m) = (Registry::default(), memberships());
        let ex = execute(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(10.0)),
                                   ("period", json!("monthly"))]));
        assert!(ex.committed());
    }

    // ── organisational closure at the gate (R2 #14) ─────────────────────────

    fn typed() -> Enforcement {
        use crate::schema::{DimType, Schema};
        use std::collections::BTreeMap;
        let pocket = DimType::Record {
            fields: BTreeMap::from([
                ("allocated".into(), DimType::Number { lo: None, hi: None }),
                ("spent".into(), DimType::Number { lo: None, hi: None }),
                ("limit".into(), DimType::Number { lo: None, hi: None }),
            ]),
        };
        let schema = Schema::new().declare(
            "finances",
            DimType::Record {
                fields: BTreeMap::from([
                    ("liquid".into(), DimType::Record {
                        fields: BTreeMap::from([("balance".into(), DimType::Number { lo: None, hi: None })]),
                    }),
                    ("pockets".into(), DimType::Map { value: Box::new(pocket) }),
                    ("income".into(), DimType::Any),
                ]),
            },
        );
        Enforcement { schema: Some(schema), ..armed() }
    }

    #[test]
    fn closure_allows_an_ordinary_value_change() {
        let reg = Registry::default();
        let ex = execute(&reg, &allowed(), &typed(), &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(50.0)),
                                   ("period", json!("monthly"))]));
        assert!(ex.committed(), "money moving is a value change, not a shape change");
    }

    #[test]
    fn closure_allows_a_new_pocket_because_pocket_names_are_values() {
        let reg = Registry::default();
        let ex = execute(&reg, &allowed(), &typed(), &homestead_state(), "budget.add_pocket",
                         &params(&[("pocket_name", json!("travel")), ("limit", json!(0.0))]));
        assert!(ex.committed(), "the schema declared a map of pockets, and it still is one");
    }

    #[test]
    fn closure_refuses_an_operator_that_reshapes_the_sustain() {
        use crate::operator::meta::{OperatorMeta, Protocol};
        let mut reg = Registry::default();
        reg.register(OperatorMeta {
            name: "test.reshape",
            description: "Removes a declared dimension. Exists to prove the gate refuses it.",
            constraints: vec![],
            post_constraints: vec![],
            side_effects: vec!["event.test.reshaped"],
            pawa_cost: 0,
            protocol: Protocol::Rpc,
            min_privilege: 0,
            run: |state, _p, events| {
                // Replace the whole finances record with one missing `liquid`.
                let _ = state.set("finances", json!({"pockets": {}, "income": {}}));
                events.push(EmittedEvent { name: "event.test.reshaped".into(), payload: json!({}) });
                OperatorResult::ok(json!({}))
            },
        });
        let mut allow = allowed();
        allow.push("test.reshape".to_string());

        let ex = execute(&reg, &allow, &typed(), &homestead_state(), "test.reshape", &params(&[]));
        assert!(!ex.committed(), "changing what the boundary IS belongs to a higher gate");
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("organisational_closure"));
        assert_eq!(ex.state, homestead_state(), "a refusal changes nothing");
    }

    #[test]
    fn closure_is_not_enforced_when_no_schema_is_declared() {
        // Adoption is incremental: an untyped sustain behaves exactly as before.
        let reg = Registry::default();
        let ex = execute(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(50.0)),
                                   ("period", json!("monthly"))]));
        assert!(ex.committed());
    }

    #[test]
    fn a_rule_that_will_not_compile_refuses_rather_than_failing_open() {
        let reg = Registry::default();
        let broken = Enforcement {
            enabled: true,
            invariants: vec![("broken".into(), "this is (not ) valid".into())],
            schema: None,
        };
        let ex = execute(&reg, &allowed(), &broken, &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(1.0)),
                                   ("period", json!("monthly"))]));
        assert!(!ex.committed(), "an unreadable rule must not be silently skipped");
    }
}
