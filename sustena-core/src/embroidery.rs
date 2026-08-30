//! **Enzymes authored in Embroidery** — `⟦enzyme⟧ = ⟨g, e, ε⟩` (DSL · §4F).
//!
//! ★★★ **Today a definition names Enzymes; it never says what they DO.**
//! `⟦operator⟧` is a name lookup: the guard, the effect and the emissions all
//! live in Rust, so the only people who can add a capability to a Sustain are
//! the people who can compile one. A household that wants a rule of its own —
//! *round every spend up to the nearest ten and put the difference in savings* —
//! cannot have one, and the language it is supposedly written in cannot say it.
//!
//! ## The shape, and the one thing it buys that hand-written Rust cannot
//!
//! ```text
//!   g : guard predicates          — the same Embroidery invariants are written in
//!   e : declared actions          — set / increment / decrement over declared paths
//!   ε : emitted event names       — what the rest of the system may react to
//!   μ : declared movements        — what money crossed, for double entry
//! ```
//!
//! ★★★ **The effect summary is DERIVED, so it cannot lie.** A native Enzyme
//! declares an [`EffectSummary`](crate::compose::EffectSummary) *about* its own
//! body, and [`crate::obligation`] says plainly that a summary is a claim which
//! can be wrong. An authored Enzyme has no separate body to disagree with: the
//! actions **are** the effect, and the summary is read off them. Composition
//! (`wp`, Hoare chaining) therefore reasons about what will actually happen
//! rather than about what somebody wrote down.
//!
//! ## What the effect language deliberately is NOT
//!
//! ★★★ **There is no arithmetic in expressions**, and that is the same
//! commitment the predicate grammar makes (GENOME §III: *small is a commitment*).
//! The arithmetic lives in the ACTIONS — `increment path by e`, `decrement path
//! by e` — which is exactly how `StateAccessor` already works, so this is the
//! existing engine's own shape rather than a second one invented alongside it.
//! It buys totality: every action terminates, every expression is a lookup, and
//! there is nothing to evaluate that could diverge.
//!
//! ★★ **No control flow, no loops, no calling other Enzymes.** An authored
//! Enzyme is a straight line. Sequencing belongs to [`crate::dag`], where it is
//! checked; letting an effect branch would put an unchecked second sequencer
//! inside the one place that must stay total.
//!
//! ★★ **Movements are declared, not inferred.** The ledger's whole check is
//! *observed mutations against declared movements*, so inferring the declaration
//! from the mutations would make double entry check a statement against itself.

use serde_json::{Map, Value};

use crate::compose::{Change, EffectSummary};
use crate::flow::Movement;
use crate::operator::meta::{OperatorResult, ParamDecl, ParamKind};
use crate::operator::EmittedEvent;
use crate::predicate::types::{typecheck as typecheck_predicate, Gamma, Ty};
use crate::predicate::{parse_path, parse_predicate};
use crate::schema::{DimType, Schema, TypeError};
use crate::state::State;

/// Something that produces a value. Three shapes, and no arithmetic.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    /// A value from the call.
    Param(String),
    /// A value from state.
    Path(String),
}

impl Expr {
    fn read(&self, state: &State, params: &Map<String, Value>) -> Option<Value> {
        match self {
            Expr::Literal(v) => Some(v.clone()),
            Expr::Param(name) => params.get(name).cloned(),
            Expr::Path(p) => state.get(p).cloned(),
        }
    }

    fn ty(&self, schema: &Schema, params: &[ParamDecl]) -> Ty {
        match self {
            Expr::Literal(Value::Number(_)) => Ty::Number,
            Expr::Literal(Value::String(_)) => Ty::Text,
            Expr::Literal(Value::Bool(_)) => Ty::Bool,
            Expr::Literal(_) => Ty::Unknown,
            Expr::Param(name) => params
                .iter()
                .find(|p| p.name.as_ref() == name.as_str())
                .map(|p| match p.kind {
                    ParamKind::Number => Ty::Number,
                    ParamKind::Text => Ty::Text,
                    ParamKind::Any => Ty::Unknown,
                })
                .unwrap_or(Ty::Unknown),
            Expr::Path(p) => parse_path(p)
                .ok()
                .and_then(|segs| schema.resolve(&segs))
                .map(Ty::from)
                .unwrap_or(Ty::Unknown),
        }
    }
}

/// One thing an authored Enzyme does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Set { path: String, value: Expr },
    Increment { path: String, by: Expr },
    /// ★★ `allow_negative` is declared rather than assumed. A pocket's spending
    /// may go below zero for a person's tab and may not for an envelope, and
    /// which one this is belongs to the author, not to a default.
    Decrement { path: String, by: Expr, allow_negative: bool },
}

impl Action {
    pub fn path(&self) -> &str {
        match self {
            Action::Set { path, .. }
            | Action::Increment { path, .. }
            | Action::Decrement { path, .. } => path,
        }
    }
}

/// What money crossed, declared so double entry has something to check against.
#[derive(Debug, Clone, PartialEq)]
pub struct MovementDecl {
    pub kind: String,
    pub qty: Expr,
    pub from: Expr,
    pub to: Expr,
}

/// An Enzyme written in the language rather than in Rust.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoredEnzyme {
    pub name: String,
    pub params: Vec<ParamDecl>,
    /// `g` — every one must hold.
    pub guard: Vec<String>,
    /// `e` — applied in order.
    pub effect: Vec<Action>,
    /// `ε`
    pub emits: Vec<String>,
    /// `μ`
    pub moves: Vec<MovementDecl>,
}

impl AuthoredEnzyme {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            guard: Vec::new(),
            effect: Vec::new(),
            emits: Vec::new(),
            moves: Vec::new(),
        }
    }

    pub fn taking(mut self, param: ParamDecl) -> Self {
        self.params.push(param);
        self
    }
    pub fn guarded_by(mut self, expr: &str) -> Self {
        self.guard.push(expr.into());
        self
    }
    pub fn doing(mut self, action: Action) -> Self {
        self.effect.push(action);
        self
    }
    pub fn emitting(mut self, event: &str) -> Self {
        self.emits.push(event.into());
        self
    }
    pub fn moving(mut self, m: MovementDecl) -> Self {
        self.moves.push(m);
        self
    }

    /// **`Γ ⊢ enzyme`** — every guard types, every path is declared, every
    /// action fits what it touches.
    ///
    /// ★★★ An authored Enzyme is a document like any other, so the same rule
    /// applies: a broken one is refused when it is written, not reported as a
    /// refusal the first time somebody runs it.
    pub fn typecheck(&self, schema: &Schema) -> Result<(), Vec<TypeError>> {
        let mut errors = Vec::new();

        let mut gamma = Gamma::new(schema);
        for p in &self.params {
            gamma = gamma.with_param(
                &p.name,
                match p.kind {
                    ParamKind::Number => Ty::Number,
                    ParamKind::Text => Ty::Text,
                    ParamKind::Any => Ty::Unknown,
                },
            );
        }

        for expr in &self.guard {
            match parse_predicate(expr) {
                Err(e) => errors.push(TypeError {
                    path: expr.clone(),
                    detail: format!("guard does not parse: {e}"),
                }),
                Ok(p) => {
                    let unresolved = crate::schema::bind(&p, schema);
                    let resolved = unresolved.is_empty();
                    errors.extend(unresolved);
                    if resolved {
                        errors.extend(typecheck_predicate(&p, &gamma));
                    }
                }
            }
        }

        for action in &self.effect {
            let path = action.path();
            let Some(target) = parse_path(path).ok().and_then(|s| schema.resolve(&s)) else {
                errors.push(TypeError {
                    path: path.to_string(),
                    detail: "is not a dimension this Sustain declares".into(),
                });
                continue;
            };
            let target_ty = Ty::from(target);
            match action {
                // ★★★ Only a number can be moved BY something. Incrementing a
                //     label is not a smaller mistake than summing one; it is
                //     the same mistake on the writing side.
                Action::Increment { by, .. } | Action::Decrement { by, .. } => {
                    if !matches!(target_ty, Ty::Number | Ty::Unknown) {
                        errors.push(TypeError {
                            path: path.to_string(),
                            detail: format!("is {}, so it cannot be moved by an amount", target_ty.name()),
                        });
                    }
                    let by_ty = by.ty(schema, &self.params);
                    if !matches!(by_ty, Ty::Number | Ty::Unknown) {
                        errors.push(TypeError {
                            path: path.to_string(),
                            detail: format!("is moved by {}, which is not an amount", by_ty.name()),
                        });
                    }
                }
                Action::Set { value, .. } => {
                    let v = value.ty(schema, &self.params);
                    if !v.compatible_with(&target_ty) {
                        errors.push(TypeError {
                            path: path.to_string(),
                            detail: format!(
                                "is {} and would be set to {}",
                                target_ty.name(),
                                v.name()
                            ),
                        });
                    }
                }
            }
        }

        for m in &self.moves {
            let qty = m.qty.ty(schema, &self.params);
            if !matches!(qty, Ty::Number | Ty::Unknown) {
                errors.push(TypeError {
                    path: self.name.clone(),
                    detail: format!("declares a movement of {}, which is not an amount", qty.name()),
                });
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    /// **The effect summary, READ OFF the actions.**
    ///
    /// ★★★ This is what an authored Enzyme has that a native one cannot: there
    /// is no separate body for the summary to disagree with. `obligation` is
    /// explicit that a declared summary is a *claim* which may be wrong; here
    /// the claim and the thing are the same object.
    ///
    /// ★★ A move by a literal is a known shift; a move by a parameter is
    /// `Opaque`, because the amount is not known until the call. Naming it
    /// opaque is the honest answer — guessing a shift would make `wp` compute a
    /// precondition for an amount nobody supplied yet.
    pub fn effect_summary(&self) -> EffectSummary {
        let mut summary = EffectSummary::new();
        for action in &self.effect {
            let change = match action {
                Action::Set { value: Expr::Literal(v), .. } => Change::SetTo(v.clone()),
                Action::Increment { by: Expr::Literal(v), .. } => {
                    v.as_f64().map(Change::ShiftBy).unwrap_or(Change::Opaque)
                }
                Action::Decrement { by: Expr::Literal(v), .. } => {
                    v.as_f64().map(|n| Change::ShiftBy(-n)).unwrap_or(Change::Opaque)
                }
                _ => Change::Opaque,
            };
            summary = summary.with(action.path(), change);
        }
        summary
    }

    /// Run it. The same signature a native Enzyme's body has.
    pub fn run(
        &self,
        state: &mut State,
        params: &Map<String, Value>,
        events: &mut Vec<EmittedEvent>,
        movements: &mut Vec<Movement>,
    ) -> OperatorResult {
        // ★★ The guards run through the ordinary evaluator, against the
        //    ordinary merged view of state and params — so an authored guard
        //    means exactly what the same words mean in an invariant.
        for expr in &self.guard {
            let Ok(node) = parse_predicate(expr) else {
                return OperatorResult::fail(
                    format!("guard '{expr}' cannot be read"),
                    "guard_readable",
                );
            };
            // ★★ The evaluator's own words for WHY, not a restatement. A guard
            //    that failed and a guard that failed *because a balance was
            //    short by 40* are different amounts of help.
            let (held, why) = crate::predicate::eval::evaluate(&node, &state.snapshot(), params);
            if !held {
                return OperatorResult::fail(format!("Constraint failed: {why}"), expr.clone());
            }
        }

        for action in &self.effect {
            let outcome = match action {
                Action::Set { path, value } => match value.read(state, params) {
                    Some(v) => state.set(path, v).map(|_| ()),
                    None => {
                        return OperatorResult::fail(
                            format!("'{path}' needs a value nobody supplied"),
                            "effect_value_present",
                        )
                    }
                },
                Action::Increment { path, by } => match by.read(state, params) {
                    Some(v) => state.increment(path, &v).map(|_| ()),
                    None => {
                        return OperatorResult::fail(
                            format!("'{path}' needs an amount nobody supplied"),
                            "effect_value_present",
                        )
                    }
                },
                Action::Decrement { path, by, allow_negative } => match by.read(state, params) {
                    Some(v) => state.decrement(path, &v, *allow_negative).map(|_| ()),
                    None => {
                        return OperatorResult::fail(
                            format!("'{path}' needs an amount nobody supplied"),
                            "effect_value_present",
                        )
                    }
                },
            };
            // ★★★ A refused write is reported, never swallowed. Reporting
            //     success on a mutation that did not happen is the worst lie a
            //     money Enzyme can tell — the card says filed, the ledger says
            //     nothing.
            if let Err(e) = outcome {
                return OperatorResult::fail(e.to_string(), "effect_applies");
            }
        }

        for m in &self.moves {
            let (Some(qty), Some(from), Some(to)) = (
                m.qty.read(state, params),
                m.from.read(state, params),
                m.to.read(state, params),
            ) else {
                return OperatorResult::fail(
                    "a declared movement is missing one of its ends",
                    "movement_complete",
                );
            };
            movements.push(Movement::new(
                &m.kind,
                qty.as_f64().unwrap_or(0.0),
                from.as_str().unwrap_or_default(),
                to.as_str().unwrap_or_default(),
            ));
        }

        for name in &self.emits {
            events.push(EmittedEvent {
                name: name.clone(),
                payload: Value::Object(params.clone()),
            });
        }

        OperatorResult::ok(Value::Object(params.clone()))
    }
}

/// The declared type of a dimension, for callers building an authored Enzyme.
pub fn dim_is_number(ty: &DimType) -> bool {
    matches!(ty, DimType::Number { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn number() -> DimType {
        DimType::Number { lo: None, hi: None }
    }

    fn schema() -> Schema {
        Schema::new()
            .declare(
                "finances",
                DimType::Record {
                    fields: [
                        (
                            "liquid".to_string(),
                            DimType::Record {
                                fields: [("balance".to_string(), number())].into_iter().collect(),
                            },
                        ),
                        ("savings".to_string(), number()),
                        ("label".to_string(), DimType::Text),
                    ]
                    .into_iter()
                    .collect(),
                },
            )
    }

    fn state() -> Value {
        json!({"finances": {"liquid": {"balance": 1000.0}, "savings": 0.0, "label": "home"}})
    }

    /// *Move `amount` from the liquid balance into savings, if it is there.*
    fn sweep() -> AuthoredEnzyme {
        AuthoredEnzyme::new("household.sweep")
            .taking(ParamDecl::number("amount"))
            .guarded_by("finances.liquid.balance >= params.amount")
            .doing(Action::Decrement {
                path: "finances.liquid.balance".into(),
                by: Expr::Param("amount".into()),
                allow_negative: false,
            })
            .doing(Action::Increment {
                path: "finances.savings".into(),
                by: Expr::Param("amount".into()),
            })
            .emitting("event.finances.swept")
    }

    fn run(e: &AuthoredEnzyme, s: &Value, params: &[(&str, Value)]) -> (OperatorResult, Value) {
        let mut st = State::new(s.clone());
        let map: Map<String, Value> =
            params.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        let mut events = Vec::new();
        let mut movements = Vec::new();
        let r = e.run(&mut st, &map, &mut events, &mut movements);
        (r, st.snapshot())
    }

    #[test]
    fn an_enzyme_can_be_written_without_writing_rust() {
        // ★★★ The row's whole point: a household's own rule, expressed in the
        //     language the household is written in.
        assert!(sweep().typecheck(&schema()).is_ok());
        let (result, after) = run(&sweep(), &state(), &[("amount", json!(250.0))]);
        assert!(result.is_ok(), "{:?}", result.reason);
        assert_eq!(after.pointer("/finances/liquid/balance"), Some(&json!(750.0)));
        assert_eq!(after.pointer("/finances/savings"), Some(&json!(250.0)));
    }

    #[test]
    fn an_authored_guard_means_what_the_same_words_mean_in_an_invariant() {
        let (result, after) = run(&sweep(), &state(), &[("amount", json!(5000.0))]);
        assert!(!result.is_ok());
        assert!(result.reason.unwrap().contains("Constraint failed"));
        assert_eq!(after, state(), "a refusal leaves no trace");
    }

    #[test]
    fn writing_to_a_dimension_nobody_declared_is_refused_when_it_is_written() {
        let e = AuthoredEnzyme::new("bad").doing(Action::Set {
            path: "finances.nowhere".into(),
            value: Expr::Literal(json!(1)),
        });
        let errors = e.typecheck(&schema()).expect_err("refuses");
        assert!(errors[0].detail.contains("does not declare") || errors[0]
            .detail
            .contains("not a dimension"));
    }

    #[test]
    fn incrementing_a_label_is_the_same_mistake_as_summing_one() {
        // ★★★ On the writing side rather than the reading side, and it is not a
        //     smaller mistake.
        let e = AuthoredEnzyme::new("bad").doing(Action::Increment {
            path: "finances.label".into(),
            by: Expr::Literal(json!(1)),
        });
        let errors = e.typecheck(&schema()).expect_err("refuses");
        assert!(errors[0].detail.contains("cannot be moved by an amount"), "{:?}", errors[0]);
    }

    #[test]
    fn setting_a_number_to_a_word_is_refused() {
        let e = AuthoredEnzyme::new("bad")
            .taking(ParamDecl::text("what"))
            .doing(Action::Set {
                path: "finances.savings".into(),
                value: Expr::Param("what".into()),
            });
        let errors = e.typecheck(&schema()).expect_err("refuses");
        assert!(errors[0].detail.contains("would be set to text"), "{:?}", errors[0]);
    }

    #[test]
    fn a_guard_that_does_not_type_is_refused_before_it_ever_runs() {
        let e = AuthoredEnzyme::new("bad").guarded_by("finances.label >= 5");
        assert!(e.typecheck(&schema()).is_err());
    }

    #[test]
    fn the_effect_summary_is_read_off_the_actions_so_it_cannot_lie() {
        // ★★★ What an authored Enzyme has that a native one cannot. A native
        //     summary is a CLAIM about a body that may disagree with it; here
        //     the claim and the thing are one object.
        let e = AuthoredEnzyme::new("give").doing(Action::Increment {
            path: "finances.savings".into(),
            by: Expr::Literal(json!(100.0)),
        });
        let summary = e.effect_summary();
        assert_eq!(summary.changes.len(), 1);
        assert_eq!(summary.changes[0].0, "finances.savings");
        assert_eq!(summary.changes[0].1, Change::ShiftBy(100.0));
    }

    #[test]
    fn an_amount_nobody_knows_yet_is_opaque_rather_than_guessed() {
        // ★★ Guessing a shift would make `wp` compute a precondition for an
        //    amount that has not been supplied.
        let summary = sweep().effect_summary();
        assert!(summary.changes.iter().all(|(_, c)| *c == Change::Opaque));
    }

    #[test]
    fn a_refused_write_is_reported_rather_than_swallowed() {
        // ★★★ Reporting success on a mutation that did not happen is the worst
        //     lie a money Enzyme can tell.
        let e = AuthoredEnzyme::new("overdraw")
            .taking(ParamDecl::number("amount"))
            .doing(Action::Decrement {
                path: "finances.savings".into(),
                by: Expr::Param("amount".into()),
                allow_negative: false,
            });
        let (result, after) = run(&e, &state(), &[("amount", json!(50.0))]);
        assert!(!result.is_ok(), "savings is zero, so this cannot happen");
        assert_eq!(result.constraint_violated.as_deref(), Some("effect_applies"));
        assert_eq!(after.pointer("/finances/savings"), Some(&json!(0.0)));
    }

    #[test]
    fn a_declared_movement_reaches_the_ledger() {
        // ★★ Declared rather than inferred: inferring it from the mutations
        //    would make double entry check a statement against itself.
        let e = sweep().moving(MovementDecl {
            kind: "money".into(),
            qty: Expr::Param("amount".into()),
            from: Expr::Literal(json!("finances.liquid")),
            to: Expr::Literal(json!("finances.savings")),
        });
        let mut st = State::new(state());
        let params: Map<String, Value> =
            [("amount".to_string(), json!(100.0))].into_iter().collect();
        let mut events = Vec::new();
        let mut movements = Vec::new();
        let r = e.run(&mut st, &params, &mut events, &mut movements);
        assert!(r.is_ok());
        assert_eq!(movements.len(), 1);
        assert_eq!(movements[0].qty, 100.0);
    }

    #[test]
    fn what_it_emits_is_what_it_declared() {
        let mut st = State::new(state());
        let params: Map<String, Value> =
            [("amount".to_string(), json!(10.0))].into_iter().collect();
        let mut events = Vec::new();
        let mut movements = Vec::new();
        sweep().run(&mut st, &params, &mut events, &mut movements);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "event.finances.swept");
    }
}

#[cfg(test)]
mod through_the_gate_tests {
    //! ★★★ The half that makes DSL-13 real rather than a parser: an Enzyme
    //! written in the language runs through **the same gate** as one written in
    //! Rust — same admission, same invariants, same double-entry check.
    use super::*;
    use crate::operator::meta::{OperatorMeta, Protocol};
    use crate::operator::{execute, Enforcement, Registry};
    use serde_json::json;
    use std::sync::Arc;

    fn number() -> DimType {
        DimType::Number { lo: None, hi: None }
    }

    fn household() -> Value {
        json!({"finances": {
            "liquid": {"balance": 1000.0},
            "savings": 0.0,
            "pockets": {}, "accounts": {},
            "income": {"monthly_total": 0.0, "sources": []}}})
    }

    fn sweep() -> AuthoredEnzyme {
        AuthoredEnzyme::new("household.sweep")
            .taking(ParamDecl::number("amount"))
            .guarded_by("finances.liquid.balance >= params.amount")
            .doing(Action::Decrement {
                path: "finances.liquid.balance".into(),
                by: Expr::Param("amount".into()),
                allow_negative: false,
            })
            .doing(Action::Increment {
                path: "finances.savings".into(),
                by: Expr::Param("amount".into()),
            })
            .emitting("event.finances.swept")
    }

    /// Register an authored Enzyme the way a Sustain would.
    fn registry_with(enzyme: AuthoredEnzyme) -> Registry {
        let mut reg = Registry::default();
        let params = enzyme.params.clone();
        let emits: Vec<&'static str> = Vec::new();
        reg.register(OperatorMeta {
            name: "household.sweep",
            description: "Authored in Embroidery.",
            params,
            constraints: vec![],
            post_constraints: vec![],
            side_effects: emits,
            pawa_cost: 0,
            protocol: Protocol::Rpc,
            min_privilege: 1,
            effect: Some(enzyme.effect_summary()),
            authored: Some(Arc::new(enzyme)),
            run: |_, _, _, _| {
                crate::operator::meta::OperatorResult::fail(
                    "this Enzyme is authored — the interpreter runs it",
                    "authored_dispatch",
                )
            },
        });
        reg
    }

    fn call(reg: &Registry, state: &Value, amount: f64) -> crate::operator::Execution {
        let params: Map<String, Value> =
            [("amount".to_string(), json!(amount))].into_iter().collect();
        execute(reg, &reg.names(), &Enforcement::default(), state, "household.sweep", &params)
    }

    #[test]
    fn an_authored_enzyme_commits_through_the_ordinary_gate() {
        let reg = registry_with(sweep());
        let ex = call(&reg, &household(), 250.0);
        assert!(ex.committed(), "{:?}", ex.result.reason);
        assert_eq!(
            ex.state.pointer("/finances/liquid/balance").and_then(Value::as_f64),
            Some(750.0),
        );
        assert!(!ex.mutations.is_empty(), "and the fold will replay it");
        assert_eq!(ex.events.len(), 1);
    }

    #[test]
    fn an_authored_guard_refuses_through_the_ordinary_gate() {
        let before = household();
        let ex = call(&registry_with(sweep()), &before, 9_999.0);
        assert!(!ex.committed());
        assert_eq!(ex.state, before, "a refusal leaves no trace");
        assert!(ex.mutations.is_empty());
    }

    #[test]
    fn an_authored_enzyme_faces_the_sustains_own_invariants() {
        // ★★★ The property that matters: authoring an Enzyme does not author a
        //     way around the household's laws. A rule it would break refuses it
        //     exactly as it refuses a native one.
        let reg = registry_with(
            AuthoredEnzyme::new("household.sweep")
                .taking(ParamDecl::number("amount"))
                .doing(Action::Decrement {
                    path: "finances.liquid.balance".into(),
                    by: Expr::Param("amount".into()),
                    allow_negative: true,
                }),
        );
        let enforcement = Enforcement {
            enabled: true,
            invariants: vec![(
                "liquid_non_negative".to_string(),
                "finances.liquid.balance >= 0".to_string(),
            )],
            ..Enforcement::default()
        };
        let params: Map<String, Value> =
            [("amount".to_string(), json!(5_000.0))].into_iter().collect();
        let ex = execute(
            &reg,
            &reg.names(),
            &enforcement,
            &household(),
            "household.sweep",
            &params,
        );
        assert!(!ex.committed(), "the household's own law still holds");
    }

    #[test]
    fn an_authored_enzyme_that_moves_money_faces_double_entry() {
        // ★★★ It cannot author its way out of declaring what it moved. An
        //     Enzyme that lowers a cash account and declares nothing is the
        //     single-sided entry the ledger exists to refuse, whichever
        //     language it was written in.
        let silent = AuthoredEnzyme::new("household.sweep")
            .taking(ParamDecl::number("amount"))
            .doing(Action::Decrement {
                path: "finances.accounts.mpesa.balance".into(),
                by: Expr::Param("amount".into()),
                allow_negative: true,
            });
        let mut state = household();
        state["finances"]["accounts"]["mpesa"] = json!({"balance": 1000.0});
        let ex = call(&registry_with(silent), &state, 100.0);
        assert!(!ex.committed(), "{:?}", ex.result);
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("double_entry"));
    }

    #[test]
    fn the_dispatch_stub_is_unreachable_and_says_so_if_it_ever_is_not() {
        // ★★ `run` still holds a real function for an authored Enzyme. If the
        //    dispatch is ever got wrong the result is an honest error, not a
        //    native body running with an authored Enzyme's parameters.
        let reg = registry_with(sweep());
        let meta = reg.get("household.sweep").expect("registered");
        assert!(meta.authored.is_some());
        let mut st = State::new(household());
        let r = (meta.run)(&mut st, &Map::new(), &mut Vec::new(), &mut Vec::new());
        assert!(!r.is_ok());
        assert_eq!(r.constraint_violated.as_deref(), Some("authored_dispatch"));
    }
}
