//! The parameters are a declared Sustain; a parameter change is an Enzyme
//! (PAWA-11; the Pawa paper §6.5).
//!
//! ★★★ **Same template as PAWA-6's treasury, and for the same reason: no
//! bespoke engine.** The economic parameters live in a [`Definition`] plus a
//! state; `governance.set_parameter` is an ordinary [`OperatorMeta`] Enzyme
//! admitted by the **same** gate; `V_gov`'s bounds are **ordinary invariants**;
//! a change needs the **same** `ApprovalToken`; and the change lands in the
//! **same** fold, so a parameter's history is replayable by
//! [`crate::semantic::replay_under`], which knows nothing about governance.
//!
//! ## ★★★ One truth per parameter — the constants are GENESIS VALUES, not the
//! live ones
//!
//! Before this row, `κ_c` and `κ_s` were module constants in
//! [`crate::pawa`] — *the* value, read directly by the meter and by PAWA-3's
//! gate clause. A governed parameter cannot also be a hardcoded const that
//! silently overrides it, so the constants were **demoted**: they are now
//! [`Parameters::GENESIS_KAPPA_COMPUTE`] and
//! [`Parameters::GENESIS_KAPPA_STORAGE`] — *the value κ is declared with at
//! genesis*, not *the value κ has*.
//!
//! ★★ **And `pawa::compute_pawa` is gone.** Pricing now happens through
//! [`Parameters::price`], and `meter`, `candidate_pawa` and the affordability
//! conjunct all take a `&Parameters` — so **there is no path that prices from a
//! constant**. The same one-truth discipline the treasury's balance got.
//!
//! ★ **What this row does NOT govern yet, and why**: the ratified royalty
//! schedule stays a constant in [`crate::royalty`]. Governing it needs a bound
//! that does not exist here — *the five shares must sum to 100* — and MYC-5's
//! conservation theorem depends on it. That is a real invariant to design
//! deliberately rather than a value to move, so it is named as a follow-on
//! rather than half-wired.

use serde_json::{json, Map, Value};

use crate::editing::Definition;
use crate::flow::Movement;
use crate::operator::{
    meta::ParamDecl, meta::Protocol, EmittedEvent, Enforcement, OperatorMeta, OperatorResult,
    Registry,
};
use crate::schema::{DimType, Schema};
use crate::state::State;

/// One governed parameter and the range `V_gov` allows it.
///
/// ★ The bounds are **declared here and enforced by the ordinary gate** — they
/// become invariants on the definition, exactly as the treasury's reserve floor
/// does. No bound-checking code is written.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterSpec {
    pub name: &'static str,
    pub genesis: f64,
    pub min: f64,
    pub max: f64,
}

impl ParameterSpec {
    pub fn declared(name: &'static str, genesis: f64, min: f64, max: f64) -> Option<ParameterSpec> {
        if min > max || !(min..=max).contains(&genesis) {
            return None;
        }
        Some(ParameterSpec { name, genesis, min, max })
    }
}

/// The live values, read from a governance state.
///
/// ★★ **The only way to price.** [`crate::pawa`] has no constants left to read,
/// so a caller must obtain these — which means the value in force is always the
/// governed one.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameters {
    kappa_compute: f64,
    kappa_storage: f64,
    issuance_rate: f64,
    attention_kappa: f64,
}

impl Parameters {
    /// ★★ The value `κ_c` is **declared with at genesis** — not the value it
    /// has. It moves by governance from here.
    ///
    /// Still **uncalibrated**, exactly as PAWA-1 said: declaring a number and
    /// making it revisable does not make it right, and governance is the
    /// mechanism for fixing that once real infrastructure cost is known.
    pub const GENESIS_KAPPA_COMPUTE: f64 = 1.0;
    /// The genesis value of `κ_s`. See [`Parameters::GENESIS_KAPPA_COMPUTE`].
    pub const GENESIS_KAPPA_STORAGE: f64 = 0.01;
    /// ★★★ **Issuance is OFF at genesis** (PAWA-8), and that is a statement,
    /// not a placeholder: an economy should not begin inflating because nobody
    /// chose a rate. Switching it on takes a deliberate, gated, recorded
    /// governance act — which is exactly the safety property issuance needs,
    /// since an ungoverned rate is the Sybil-profitable inflation MYC-6 warned
    /// of.
    pub const GENESIS_ISSUANCE_RATE: f64 = 0.0;
    /// ★★ `κ_att` — OPV-7's attention coefficient, `pawa(a) = κ·b·d·ρ`. Its
    /// genesis value is **`1.0`, exactly the constant it replaces**, so the
    /// cost of every already-declared aperture is unchanged.
    ///
    /// Still **uncalibrated**, in the same words as `κ_c`: an attention unit
    /// and a compute unit are the same word on two unmeasured scales, and
    /// saying so is the honest position. See [`crate::attention`].
    pub const GENESIS_ATTENTION_KAPPA: f64 = 1.0;

    /// The declared genesis parameters — what `Σ_gov` opens with.
    pub fn genesis() -> Parameters {
        Parameters {
            kappa_compute: Self::GENESIS_KAPPA_COMPUTE,
            kappa_storage: Self::GENESIS_KAPPA_STORAGE,
            issuance_rate: Self::GENESIS_ISSUANCE_RATE,
            attention_kappa: Self::GENESIS_ATTENTION_KAPPA,
        }
    }

    /// Read the live values out of a governance state.
    ///
    /// ★ A parameter the state does not carry falls back to its **genesis**
    /// value, which is the honest reading: an ungoverned deployment is running
    /// the declared defaults, not running nothing.
    pub fn read(state: &Value) -> Parameters {
        let reader = State::new(state.clone());
        let get = |name: &str, fallback: f64| {
            reader
                .get(&format!("parameters.{name}"))
                .and_then(Value::as_f64)
                .unwrap_or(fallback)
        };
        Parameters {
            kappa_compute: get(KAPPA_COMPUTE, Self::GENESIS_KAPPA_COMPUTE),
            kappa_storage: get(KAPPA_STORAGE, Self::GENESIS_KAPPA_STORAGE),
            issuance_rate: get(ISSUANCE_RATE, Self::GENESIS_ISSUANCE_RATE),
            attention_kappa: get(ATTENTION_KAPPA, Self::GENESIS_ATTENTION_KAPPA),
        }
    }

    pub fn kappa_compute(&self) -> f64 {
        self.kappa_compute
    }

    pub fn kappa_storage(&self) -> f64 {
        self.kappa_storage
    }

    /// ★★★ **Juul issued per pawa served** (PAWA-8) — the governed rate, and
    /// the **only** place it lives.
    ///
    /// [`crate::issuance::Schedule::Fixed`] carries no rate of its own, so an
    /// issuance can only be priced from here — meaning the sole path to
    /// changing how fast new juul enters is `governance.set_parameter`, through
    /// the gate, under an approval token, recorded and replayable.
    pub fn issuance_rate(&self) -> f64 {
        self.issuance_rate
    }

    /// ★★ `κ_att` — the coefficient in `pawa(a) = κ·b·d·ρ` (OPV-7), and the
    /// **only** place it lives. [`crate::attention::Aperture`] has no constant
    /// left to read, so an aperture can only be priced from here.
    pub fn attention_kappa(&self) -> f64 {
        self.attention_kappa
    }

    /// `pawa = κ_c·compute + κ_s·storage`, under **these** parameters.
    ///
    /// ★★★ The formula did not change; where its coefficients come from did.
    /// This replaces `pawa::compute_pawa`, which read the constants — so there
    /// is no longer a way to price from anything but the governed values.
    pub fn price(&self, compute: f64, storage: f64) -> f64 {
        self.kappa_compute * compute + self.kappa_storage * storage
    }
}

/// The governed parameter names, as declared.
pub const KAPPA_COMPUTE: &str = "kappa_compute";
/// See [`KAPPA_COMPUTE`].
pub const KAPPA_STORAGE: &str = "kappa_storage";
/// PAWA-8's issuance rate — juul minted per pawa served.
pub const ISSUANCE_RATE: &str = "issuance_rate";
/// OPV-7's attention coefficient — `pawa(a) = κ·b·d·ρ`.
pub const ATTENTION_KAPPA: &str = "attention_kappa";

/// The parameters this row governs, with their declared bounds.
///
/// ★ `κ` is non-negative and bounded above by a declared ceiling — a negative
/// coefficient would pay people to do work, and an unbounded one would let a
/// single change price every future run out of reach.
pub fn declared_parameters() -> Vec<ParameterSpec> {
    vec![
        ParameterSpec::declared(KAPPA_COMPUTE, Parameters::GENESIS_KAPPA_COMPUTE, 0.0, 1_000.0)
            .expect("the genesis value is within its own bounds"),
        ParameterSpec::declared(KAPPA_STORAGE, Parameters::GENESIS_KAPPA_STORAGE, 0.0, 1_000.0)
            .expect("the genesis value is within its own bounds"),
        // ★★ The ceiling is expressed against a meaning, not picked: a rate of
        // **1.0 is break-even**, where an issuance exactly replaces what the
        // run burned. Above that the economy nets new juul per run, so a bound
        // of 10× break-even is the declared limit on how fast that may happen —
        // and an UNBOUNDED rate is precisely the Sybil-profitable inflation
        // MYC-6 named. Non-negative for the same reason κ is.
        ParameterSpec::declared(ISSUANCE_RATE, Parameters::GENESIS_ISSUANCE_RATE, 0.0, 10.0)
            .expect("the genesis value is within its own bounds"),
        // ★ Bounded like the other coefficients. ★★ Note the honest limit:
        // `ParameterSpec` expresses *bounded*, not *positive*, so a governed
        // `κ_att` of ZERO makes every scan free and the attention budget
        // vacuous. Recorded as a residual rather than hidden — but note the
        // asymmetry with `issuance_rate`, where zero is a *safe* policy (create
        // no money) while here it *disables an enforcement mechanism*. What
        // this row can still say is that it would be a **visible** change:
        // gated, recorded and replayable, rather than a source edit.
        ParameterSpec::declared(ATTENTION_KAPPA, Parameters::GENESIS_ATTENTION_KAPPA, 0.0, 1_000.0)
            .expect("the genesis value is within its own bounds"),
    ]
}

// ── the Enzyme ───────────────────────────────────────────────────────────────

/// Change one parameter.
///
/// ★★ An ordinary operator. Everything that makes a change safe — the approval
/// token, `V_gov`'s bounds, the event, the fold — comes from the gate it runs
/// through, not from anything written here.
fn set_parameter(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
    let Some(value) = params.get("value").and_then(Value::as_f64) else {
        return OperatorResult::fail("A parameter change needs a numeric value.", "value_numeric");
    };
    if name.is_empty() {
        return OperatorResult::fail("A parameter change needs a name.", "name_given");
    }

    let path = format!("parameters.{name}");
    // ★ A parameter nobody declared is not a parameter. Refused rather than
    // created, so `V_gov` cannot be sidestepped by inventing a new name — its
    // bounds only exist for declared ones.
    if !state.exists(&path) {
        return OperatorResult::fail(
            format!("'{name}' is not a declared parameter."),
            "parameter_declared",
        );
    }
    let was = state.get(&path).and_then(Value::as_f64).unwrap_or_default();
    if state.set(&path, json!(value)).is_err() {
        return OperatorResult::fail(format!("'{name}' is not settable."), "parameter_settable");
    }

    // ★ Recorded, so the change is in the fold and the history is replayable.
    events.push(EmittedEvent {
        name: "event.governance.parameter_changed".into(),
        payload: json!({"name": name, "from": was, "to": value}),
    });
    OperatorResult::ok(json!({"name": name, "from": was, "to": value}))
}

/// Register `governance.*` on a registry.
pub fn register(registry: &mut Registry) {
    registry.register(OperatorMeta {
        name: "governance.set_parameter",
        description: "Change one declared economic parameter.",
        params: vec![ParamDecl::text("name"), ParamDecl::number("value")],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.governance.parameter_changed"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 0,
        effect: None,
        run: set_parameter,
    });
}

// ── `Σ_gov` ──────────────────────────────────────────────────────────────────

/// Build the governance Sustain's [`Definition`].
///
/// ★★ Each parameter's declared range becomes **two ordinary invariants**, so
/// an out-of-range change is refused by the ordinary gate with
/// `enforcement_gate` — the same reason a household breach gives, and the same
/// evidence PAWA-6 gave that no bespoke check exists.
pub fn definition(specs: &[ParameterSpec]) -> Definition {
    let mut d = Definition::new(Schema::new().declare("parameters", DimType::Any))
        .with_operator("governance.set_parameter");
    for s in specs {
        d = d
            .with_invariant(&format!("{}_min", s.name), &format!("parameters.{} >= {}", s.name, s.min))
            .with_invariant(&format!("{}_max", s.name), &format!("parameters.{} <= {}", s.name, s.max));
    }
    d
}

/// The governance Sustain's opening state — every declared parameter at its
/// **genesis** value.
///
/// ★ The same lesson PAWA-6 learned the hard way: a bound over a value the
/// state does not hold compares `None` to a number, which is a type error that
/// refuses **everything**. So the specs that declare the bounds are the same
/// specs that seed the values.
pub fn opening_state(specs: &[ParameterSpec]) -> Value {
    let parameters: Map<String, Value> =
        specs.iter().map(|s| (s.name.to_string(), json!(s.genesis))).collect();
    json!({ "parameters": parameters })
}

/// `V_gov`, armed. ★ Reused, not rebuilt — see [`crate::treasury::enforcement`].
pub fn enforcement(d: &Definition) -> Enforcement {
    crate::semantic::enforcement_of(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::{ApprovalToken, Binding, EffectClass, NonceLedger, Simulated};
    use crate::council::ProposalStatus;
    use crate::operator::{execute_admitted, Authorization};
    use crate::semantic::{replay_under, CallOutcome, EnzymeCall, ReplayMode};

    fn registry() -> Registry {
        let mut r = Registry::default();
        register(&mut r);
        r
    }

    fn sigma_gov() -> Definition {
        definition(&declared_parameters())
    }

    fn opening() -> Value {
        opening_state(&declared_parameters())
    }

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn token_for(p: &Map<String, Value>, nonce: u64) -> ApprovalToken {
        Simulated::from_sandbox("gov-1", Binding::new("governance.set_parameter", p), Ok(()))
            .expect("the sandbox committed")
            .voted(ProposalStatus::Passed)
            .expect("passed")
            .approve("bonnie", nonce, 9_999)
    }

    /// A change, through the real gate, under a real token.
    fn change(state: &Value, name: &str, value: f64, nonce: u64) -> crate::operator::Execution {
        let d = sigma_gov();
        let p = params(&[("name", json!(name)), ("value", json!(value))]);
        let token = token_for(&p, nonce);
        execute_admitted(
            &registry(),
            &d.operators,
            &enforcement(&d),
            state,
            "governance.set_parameter",
            &p,
            &Authorization::Unchecked,
            &EffectClass::Live { token: &token, now: 1_000 },
            &mut NonceLedger::new(),
        )
    }

    // ── ★★ no bespoke engine ────────────────────────────────────────────────

    #[test]
    fn a_parameter_change_is_an_ordinary_enzyme_with_ordinary_bounds() {
        let d = sigma_gov();
        let reg = registry();
        let meta = reg.get("governance.set_parameter").expect("registered like any operator");
        assert!(!meta.params.is_empty(), "it declares its shape like any other");
        // `V_gov` is an ordinary invariant list, two entries per parameter.
        assert_eq!(d.invariants.len(), declared_parameters().len() * 2);
        assert!(d.invariants.iter().any(|(id, _)| id == "kappa_compute_min"));
    }

    #[test]
    fn a_valid_change_commits_and_records_the_event() {
        let x = change(&opening(), KAPPA_COMPUTE, 2.5, 1);
        assert!(x.committed(), "{:?}", x.result.reason);
        assert_eq!(x.state["parameters"][KAPPA_COMPUTE], json!(2.5));
        assert_eq!(x.events.len(), 1);
        assert_eq!(x.events[0].name, "event.governance.parameter_changed");
        assert_eq!(x.events[0].payload["from"], json!(1.0));
        assert_eq!(x.events[0].payload["to"], json!(2.5));
    }

    // ── ★★ gated: no token, no change ───────────────────────────────────────

    #[test]
    fn an_untokened_parameter_change_is_refused() {
        // ★★ A parameter change is a deliberate act, not something an agent
        // does alone — a token bound to a DIFFERENT change cannot admit this
        // one, which is the same as having none for it.
        let d = sigma_gov();
        let asked = params(&[("name", json!(KAPPA_COMPUTE)), ("value", json!(2.5))]);
        let wrong = token_for(&params(&[("name", json!(KAPPA_STORAGE))]), 9);
        let x = execute_admitted(
            &registry(),
            &d.operators,
            &enforcement(&d),
            &opening(),
            "governance.set_parameter",
            &asked,
            &Authorization::Unchecked,
            &EffectClass::Live { token: &wrong, now: 1_000 },
            &mut NonceLedger::new(),
        );
        assert!(!x.committed());
        assert_eq!(x.result.constraint_violated.as_deref(), Some("approval_token"));
        assert_eq!(x.state, opening(), "and nothing changed");
    }

    // ── ★ bounds are ordinary invariants ────────────────────────────────────

    #[test]
    fn a_change_outside_the_declared_bound_is_refused_by_the_ordinary_gate() {
        for bad in [-1.0, 10_000.0] {
            let x = change(&opening(), KAPPA_COMPUTE, bad, 2);
            assert!(!x.committed(), "{bad} should be out of range");
            assert_eq!(x.result.constraint_violated.as_deref(), Some("enforcement_gate"));
            assert_eq!(x.state, opening());
        }
    }

    #[test]
    fn an_undeclared_parameter_cannot_be_invented() {
        // ★ Otherwise `V_gov` could be sidestepped: bounds only exist for
        // declared names, so a new name would be an ungoverned parameter.
        let x = change(&opening(), "kappa_mystery", 5.0, 3);
        assert!(!x.committed());
        assert_eq!(x.result.constraint_violated.as_deref(), Some("parameter_declared"));
    }

    // ── ★ replayable: the value is a fold of its changes ────────────────────

    #[test]
    fn a_parameters_value_is_the_fold_of_its_changes_over_genesis() {
        // ★★ Auditable by replay: `replay_under` takes a `Definition` and knows
        // nothing about governance, and reconstructs the parameter's history.
        let calls: Vec<EnzymeCall> = [2.0, 3.0, 4.5]
            .iter()
            .enumerate()
            .map(|(i, v)| {
                EnzymeCall::new(format!("c{i}"), "governance.set_parameter")
                    .with("name", json!(KAPPA_COMPUTE))
                    .with("value", json!(v))
            })
            .collect();
        let out = replay_under(&calls, &registry(), &sigma_gov(), &opening(), ReplayMode::Replay);
        assert!(out.steps.iter().all(|s| matches!(s, CallOutcome::Admitted { .. })), "{:?}", out.steps);
        assert_eq!(out.state["parameters"][KAPPA_COMPUTE], json!(4.5), "the last change wins");
        assert_eq!(out.steps.len(), 3, "and every change is in the history");
    }

    // ── ★★★ one truth: the live value is what prices ────────────────────────

    #[test]
    fn the_constants_are_genesis_values_and_the_governed_value_is_what_prices() {
        // ★★★ `pawa::compute_pawa` is gone; pricing goes through `Parameters`.
        let genesis = Parameters::genesis();
        assert_eq!(genesis.price(7.0, 542.0), 12.42, "the genesis pricing, unchanged");

        let changed = change(&opening(), KAPPA_COMPUTE, 2.0, 4);
        assert!(changed.committed());
        let live = Parameters::read(&changed.state);
        assert_eq!(live.kappa_compute(), 2.0);
        assert_eq!(live.price(7.0, 542.0), 2.0 * 7.0 + 0.01 * 542.0, "priced by the GOVERNED κ");
        assert_ne!(live.price(7.0, 542.0), genesis.price(7.0, 542.0));
    }

    #[test]
    fn an_ungoverned_state_reads_the_declared_genesis_values() {
        // ★ An ungoverned deployment runs the declared defaults, not nothing.
        assert_eq!(Parameters::read(&json!({})), Parameters::genesis());
        assert_eq!(Parameters::read(&opening()), Parameters::genesis());
    }

    #[test]
    fn a_spec_whose_genesis_is_outside_its_own_bounds_is_refused() {
        assert!(ParameterSpec::declared("x", 5.0, 0.0, 1.0).is_none());
        assert!(ParameterSpec::declared("x", 0.5, 1.0, 0.0).is_none(), "and min > max");
    }
}
