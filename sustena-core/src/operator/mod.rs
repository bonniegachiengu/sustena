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

pub use meta::{OperatorFn, OperatorMeta, OperatorResult, ParamDecl, ParamKind, Registry};

use crate::approval::{Binding, EffectClass, NonceLedger};
use crate::mutation::Mutation;
use crate::capability::Capability;
use crate::principal::{permitted_with, Denial, Memberships, SkinRegistry, Tier};
use crate::predicate;
use crate::boundary::{preserves_closure, BoundaryDecl, ClosureViolation};
use crate::flow::{check_flows, flows_of, FlowRule, Movement};
use crate::juul::Affordability;
use crate::pawa::{candidate_pawa, meter};
use crate::schema::Schema;
use crate::transition::{check_all, TransitionRule};
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
    /// `B = ⟨scope, μ⟩`. When present, the **other half of the same closure
    /// law** is enforced: `μ_{o(s)} = μ_s`. An ordinary operator may change the
    /// values inside the boundary and never what the boundary IS.
    ///
    /// A separate field from `schema` because the two halves fail for different
    /// reasons and call for different things from whoever reads the refusal:
    /// one wants a migration, the other a
    /// [`crate::boundary::BoundaryToken`].
    pub boundary: Option<BoundaryDecl>,
    /// `F(φ, s)` — the firewall over what crossed `B`.
    ///
    /// The third predicate family, and the one that is about **neither**
    /// endpoint: two transitions with identical before-and-after states can
    /// differ entirely in what crossed to produce them. Needs `boundary` to
    /// mean anything, because *crossing* is `μ`'s question — declaring rules
    /// with no boundary leaves them inert rather than guessing at one.
    pub firewall: Vec<FlowRule>,
    /// `D(s, s')` — the transition constraints, evaluated over the (before,
    /// after) PAIR. Rate limits, monotonicity, and conservation.
    ///
    /// Separate from `invariants` because it is a different predicate family:
    /// strictly more expressive, and not simulable by a state constraint. Every
    /// single state satisfies every rule here vacuously.
    pub transitions: Vec<TransitionRule>,
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
    /// Check `permitted(α, o, Σ)` across the whole nesting path, from the
    /// principal's **ambient** authority — everything it holds anywhere is in
    /// force for this call.
    Principal {
        id: &'a str,
        memberships: &'a Memberships,
        /// Outermost containing sustain first, target last.
        path: &'a [String],
        /// The skins in force. An edge naming one nothing resolves refuses.
        skins: &'a SkinRegistry,
    },
    /// Check the authority that arrived **with the request** — the
    /// confused-deputy fix (Immune §IV; Hardy 1988).
    ///
    /// ★ Note what this variant does **not** carry: there is no
    /// `&Memberships`. The ambient set is not merely unconsulted here, it is
    /// *absent*, so a deputy holding a capability cannot fall back on the
    /// issuer's wider authority even by mistake — Miller, Yee & Shapiro's *no
    /// ambient authority* made a property of the type rather than a rule
    /// somebody has to keep. `target` is the sustain the request named, which
    /// is what the capability is checked against.
    Capability {
        capability: &'a Capability,
        /// The sustain this call names — often from untrusted input, which is
        /// exactly why it is checked against the capability rather than
        /// trusted.
        target: &'a str,
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
    let mut ledger = NonceLedger::new();
    execute_admitted(
        registry, allowed, enforcement, state, operator_name, params,
        authorization, &EffectClass::Unchecked, &mut ledger,
    )
}

/// The full `admit()` — [`execute_as`] with the approval clause of Operative
/// §XVI closing over it.
///
/// ```text
/// admit(o,s) ⟺ g_o(s) ∧ o(s)∈A ∧ D(s,o(s))
///              ∧ (effect_class = sandbox ∨ valid_token(approve(o, principal)))
/// ```
///
/// The token is checked in two places, and the split is deliberate. Binding,
/// principal and expiry are checked **before** the operator body runs — no
/// reason to compute an effect nobody approved. The nonce is spent **at
/// commit**, after the gate has passed, because a refused call must not consume
/// an approval: being turned back by an invariant should leave the person free
/// to fix the state and try the same approval again.
///
/// A live effect with no approval cannot be requested — see [`EffectClass`].
//
// Nine arguments is two too many, and the fix is a context struct carrying
// registry/allowed/enforcement — which would touch `execute`, `execute_as` and
// every call site. That is a deliberate API refactor, not something to do as a
// side effect of the token slice, so it is named here and left for its own
// change rather than quietly tolerated.
#[allow(clippy::too_many_arguments)]
pub fn execute_admitted(
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    operator_name: &str,
    params: &Map<String, Value>,
    authorization: &Authorization,
    effect: &EffectClass,
    nonces: &mut NonceLedger,
) -> Execution {
    execute_afforded(
        registry,
        allowed,
        enforcement,
        state,
        operator_name,
        params,
        authorization,
        effect,
        nonces,
        &mut Affordability::Unmetered,
    )
}

/// [`execute_admitted`], with the **out-of-pawa conjunct** in force.
///
/// ```text
/// admit = guard ∧ permitted ∧ result∈A ∧ D ∧ F ∧ closure ∧ token ∧ balance ≥ pawa
/// ```
///
/// ★★★ **Priced against the CANDIDATE's REAL cost, never the declared
/// estimate.** By the time the gate runs, the effect has already been applied to
/// a copy and the candidate's mutations and events exist — they had to, for `D`,
/// the invariants and `F`. So the run's real cost is knowable **at gate time at
/// no extra work**, and [`candidate_pawa`] returns exactly the number the
/// committed [`crate::pawa::PawaReading`] will carry. `meta.pawa_cost` — the
/// author's static guess — is **not** consulted: pricing against it is the
/// reference's own wrong path, where the estimate is `0` everywhere and the
/// clause therefore never refuses.
///
/// ★★ **Unaffordable ⇒ refuse and discard.** No commit, no state change, **no
/// juul spent** — the candidate is thrown away exactly as any other gate
/// refusal discards it, so *refusal charges nothing* joins *refusal meters
/// nothing* rather than contradicting it. The refusal carries its own reason,
/// `insufficient_pawa`, distinct from every other conjunct's.
///
/// ★ **Placed before the token redemption**, so being turned back on cost leaves
/// an approval unspent — the same courtesy the invariant gate already gets.
///
/// ★★ **Affordable ⇒ commit, then charge**, exactly once, against the real
/// committed reading rather than the pre-commit number. They are equal by
/// construction; the *ledger entry* is the committed one, so every debit traces
/// to a run that actually happened.
///
/// ★ [`Affordability::Unmetered`] makes the whole clause vacuous, and is what
/// [`execute_admitted`] and [`execute`] pass — so every caller written before
/// PAWA-3 behaves byte-for-byte as it did.
#[allow(clippy::too_many_arguments)]
pub fn execute_afforded(
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    operator_name: &str,
    params: &Map<String, Value>,
    authorization: &Authorization,
    effect: &EffectClass,
    nonces: &mut NonceLedger,
    affordability: &mut Affordability<'_>,
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
    //
    // Two ways to satisfy it, and they are genuinely different questions rather
    // than one with a shortcut. `Principal` asks it of ambient authority — who
    // are you, and what do you hold. `Capability` asks it of the authority that
    // arrived with the request, which is the only form that is safe when a
    // deputy is acting on inputs it did not choose.
    let denied = match authorization {
        Authorization::Unchecked => None,
        Authorization::Principal { id, memberships, path, skins } => {
            permitted_with(memberships, skins, id, path, meta.min_privilege as Tier).err()
        }
        Authorization::Capability { capability, target } => capability
            .permits(target, operator_name, meta.min_privilege as Tier)
            .err(),
    };
    if let Some(denial) = denied {
        // Six refusals, six names. Collapsing these would tell whoever reads
        // the log to fix the wrong thing.
        let rule = match denial {
            Denial::NoEdge { .. } => "not_a_member",
            Denial::InsufficientTier { .. } => "insufficient_privilege",
            Denial::UnresolvedSkin { .. } => "skin_resolves",
            Denial::NotDesignated { .. } => "capability_designates",
            Denial::RightNotHeld { .. } => "capability_carries",
        };
        return Execution {
            result: OperatorResult::fail(denial.to_string(), rule),
            mutations: vec![],
            events: vec![],
            state: state.clone(),
        };
    }

    // ── valid_token(approve(o, principal)) ───────────────────────────────────
    // Operative §XVI's conjunct. Sandbox and Unchecked pass straight through;
    // a live effect is admitted only by an approval bound to THIS act.
    //
    // Placed beside authority rather than at the end: like `permitted`, it does
    // not depend on state, so there is no reason to run an effect for it to
    // judge. Expiry is re-checked here at commit time, which is the point of
    // it — the state at approval time need not be the state now.
    if let EffectClass::Live { token, now } = effect {
        let acting = match authorization {
            Authorization::Principal { id, .. } => Some(*id),
            // A capability is a *transfer* of someone's authority, so the act
            // is approved against the principal whose authority it carries —
            // not the deputy holding it. That is the whole point of keeping
            // `issued_to` on the token.
            Authorization::Capability { capability, .. } => Some(capability.issued_to()),
            Authorization::Unchecked => None,
        };
        let attempted = Binding::new(operator_name, params);
        if let Err(err) = token.validate(&attempted, acting, *now) {
            return Execution {
                result: OperatorResult::fail(err.to_string(), "approval_token"),
                mutations: vec![],
                events: vec![],
                state: state.clone(),
            };
        }
        // Cheap pre-check so a known-spent approval never runs an effect. The
        // authoritative spend is at commit, below.
        if nonces.is_spent(token.nonce()) {
            return Execution {
                result: OperatorResult::fail(
                    crate::approval::TokenError::AlreadySpent { nonce: token.nonce() }.to_string(),
                    "approval_token",
                ),
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
    // Constraint §II: `candidate, flows = o.effect(copy(s), params, ctx)`.
    let mut movements: Vec<Movement> = Vec::new();
    let result = (meta.run)(&mut working, params, &mut events, &mut movements);

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

    // The autopoietic closure law (Sustain §IV):
    //
    //     ∀o ∈ T, ∀s ∈ S:  μ_{o(s)} = μ_s  ∧  schema(o(s)) = schema(s)
    //
    // A sustain is materially open — money and messages cross it constantly —
    // and organisationally closed: an ordinary operator changes values inside
    // the boundary, never what the boundary IS. Adding a member or adopting a
    // child sustain is a different class of act, and belongs to a higher gate
    // (`boundary::BoundaryToken`).
    //
    // Both halves are checked before the invariants, because a state of the
    // wrong shape — or one whose boundary has moved under it — cannot be
    // meaningfully judged against rules written for the right one.
    if let Err(err) = preserves_closure(
        state,
        &candidate,
        enforcement.boundary.as_ref(),
        enforcement.schema.as_ref(),
    ) {
        // The two halves are tagged apart: they fail for different reasons and
        // want different things from whoever reads the refusal.
        let tag = match err {
            ClosureViolation::Shape(_) => "organisational_closure",
            _ => "boundary_closure",
        };
        return Execution {
            result: OperatorResult::fail(err.to_string(), tag),
            mutations: vec![],
            events: vec![],
            state: state.clone(),
        };
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

    // ── D(s, o(s)) ───────────────────────────────────────────────────────────
    // The gate's third conjunct, and the last one to be built. Evaluated over
    // the PAIR, so it can say things no state constraint can: money is
    // conserved, stock only falls, the balance did not jump.
    //
    // Runs unconditionally, not behind `enforcement.enabled`. A conservation
    // law is not an opt-in tightening of a viable region — it is a statement
    // that a quantity does not change, and a sustain that declares one has
    // said the step is illegal, not that it is outside a preference.
    if let Err(v) = check_all(&enforcement.transitions, state, &candidate) {
        return Execution {
            result: OperatorResult::fail(v.to_string(), "transition_constraint"),
            mutations: vec![],
            events: vec![],
            state: state.clone(),
        };
    }

    // The firewall (Constraint §I–II):
    //
    //     admit_F(o,s) = admit(o,s) ∧ ⋀_{φ ∈ flows(o,s)} F(φ,s)
    //
    // A movement is what the operator DECLARED it moved; μ decides whether it
    // crossed B and in which direction. An operator that declares no movement
    // — or whose movements are all internal — satisfies F vacuously, which is
    // why adding this term changes nothing for a sustain that declares no
    // boundary and no rules.
    //
    // F is judged against the BEFORE state, as `F(φ, s)` is written: a rule
    // reading a contacts register asks who you knew when you moved, not who
    // you would know afterwards.
    if !enforcement.firewall.is_empty() {
        if let Some(decl) = enforcement.boundary.as_ref() {
            let crossed = flows_of(&movements, &decl.read(state));
            if let Err(refusal) = check_flows(&crossed, &enforcement.firewall, state) {
                return Execution {
                    result: OperatorResult::fail(refusal.to_string(), "firewall"),
                    mutations: vec![],
                    events: vec![],
                    state: state.clone(),
                };
            }
        }
    }

    // ── balance ≥ pawa ───────────────────────────────────────────────────────
    // PAWA-3's conjunct, and the last one before commit. It sits here because
    // this is the first line at which the run's REAL cost is knowable: the
    // candidate's mutations and events already exist, so pricing them is free.
    //
    // ★ Vacuous under `Unmetered`, which is what every pre-PAWA-3 caller passes.
    if let Affordability::Metered { ledger, parameters, principal, .. } = affordability {
        let cost = candidate_pawa(working.mutations(), &events, meta, enforcement, parameters);
        let balance = ledger.balance_of(principal);
        if balance < cost {
            return Execution {
                result: OperatorResult::fail(
                    format!(
                        "insufficient juul: '{operator_name}' costs {cost} pawa and                          '{principal}' has {balance}"
                    ),
                    "insufficient_pawa",
                ),
                mutations: vec![],
                events: vec![],
                state: state.clone(),
            };
        }
    }

    // ── commit ───────────────────────────────────────────────────────────────
    // The approval is spent here and nowhere else. Everything above this line
    // can refuse, and a refusal must leave the approval unspent — being turned
    // back by an invariant should leave the person free to fix the state and
    // use the same approval, not burn it.
    if let EffectClass::Live { token, .. } = effect {
        if let Err(err) = nonces.redeem(token) {
            return Execution {
                result: OperatorResult::fail(err.to_string(), "approval_token"),
                mutations: vec![],
                events: vec![],
                state: state.clone(),
            };
        }
    }

    let execution = Execution {
        result,
        mutations: working.mutations().to_vec(),
        events,
        state: candidate,
    };

    // ── the debit ────────────────────────────────────────────────────────────
    // ★★ On commit, once, against the REAL reading. `meter` returns `None` for
    // anything uncommitted, so a refused run can never reach a charge — and the
    // charge cannot be `Insufficient`, because the identical cost was checked
    // against the identical balance a few lines above and nothing interleaves.
    // ── the debit, and the issuance seam ─────────────────────────────────────
    // ★★★ **One reading, two flows.** The caller is CHARGED `pawa` and the
    // server is ISSUED `rate × pawa` — from the *same* `PawaReading`, so the
    // work that cost the one is the work that earned the other, and it is
    // demonstrably not a refund: two different amounts on two different
    // balances (PAWA-8).
    if let Affordability::Metered { ledger, parameters, principal, sustain, at, serving } =
        affordability
    {
        if let Some(reading) =
            meter(&execution, meta, enforcement, parameters, sustain, principal, *at)
        {
            let charged = ledger.charge(&reading).charged();

            // ★ Guarded on the charge having actually landed. It always does
            // here — the identical cost was checked against the identical
            // balance a few lines above and nothing interleaves — so this is
            // defence in depth against a later refactor, not a claim that an
            // unpayable run could reach it. Issuance rewards work that was
            // genuinely served AND genuinely paid for.
            if let (true, Some((issuance, server))) = (charged, *serving) {
                ledger.issue(issuance, &reading, server, parameters);
            }
        }
    }

    execution
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowDirection;
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
            boundary: None,
            firewall: vec![],
            transitions: vec![],
        }
    }

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    // ── the autopoietic closure law, AT THE GATE ─────────────────────────────

    /// An operator that appends a name to the roster — i.e. one that moves `μ`.
    /// A real registered operator rather than a predicate call, so the refusal
    /// is proven where it actually has to happen.
    fn admit_member(
        state: &mut State,
        params: &Map<String, Value>,
        _events: &mut Vec<EmittedEvent>,
        _movements: &mut Vec<Movement>,
    ) -> OperatorResult {
        let who = params.get("who").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut roster: Vec<Value> = state
            .get("roster")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        roster.push(json!(who));
        let _ = state.set("roster", json!(roster));
        OperatorResult::ok(json!({"admitted": who}))
    }

    fn bounded_registry() -> Registry {
        let mut r = Registry::default();
        r.register(crate::operator::meta::OperatorMeta {
            name: "roster.admit",
            description: "Append a name to the roster — an ordinary operator that moves μ.",
            params: vec![],
            constraints: vec![],
            post_constraints: vec![],
            side_effects: vec![],
            pawa_cost: 0,
            protocol: crate::operator::meta::Protocol::Rpc,
            min_privilege: 1,
            effect: None,
            run: admit_member,
        });
        r
    }

    fn bounded_state() -> Value {
        json!({
            "roster": ["ama"],
            "finances": {"liquid": {"balance": 1000.0}, "pockets": {},
                         "income": {"monthly_total": 0.0, "sources": []}}
        })
    }

    fn with_boundary() -> Enforcement {
        Enforcement {
            boundary: Some(
                crate::boundary::BoundaryDecl::new(
                    crate::boundary::Scope::new()
                        .spanning("finances")
                        .spanning("roster")
                        .with_entity_register("roster"),
                )
                .owning("finances"),
            ),
            ..Enforcement::default()
        }
    }

    /// ★★ The closure law is structural AT THE GATE, not a convention.
    ///
    /// An ordinary operator that moves `μ` does not commit — and the refusal
    /// names the half of `⟨B, schema⟩` it broke.
    #[test]
    fn an_ordinary_operator_that_moves_the_boundary_is_refused_at_the_gate() {
        let reg = bounded_registry();
        let before = bounded_state();
        let ex = execute(
            &reg,
            &reg.names(),
            &with_boundary(),
            &before,
            "roster.admit",
            &params(&[("who", json!("ben"))]),
        );

        assert_eq!(ex.result.status, meta::OperatorStatus::Failed, "{:?}", ex.result);
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("boundary_closure"));
        let why = ex.result.reason.clone().expect("a refusal names its reason");
        assert!(why.contains("needs a BoundaryToken"), "{why}");
        // ★ Nothing committed: the state comes back untouched.
        assert_eq!(ex.state, before);
        assert!(ex.mutations.is_empty());
        assert!(ex.events.is_empty());
    }

    /// The same operator with no boundary declared behaves exactly as before
    /// this row — which is what makes the new field additive.
    #[test]
    fn a_sustain_that_declares_no_boundary_is_unaffected() {
        let reg = bounded_registry();
        let ex = execute(
            &reg,
            &reg.names(),
            &Enforcement::default(),
            &bounded_state(),
            "roster.admit",
            &params(&[("who", json!("ben"))]),
        );
        assert_eq!(ex.result.status, meta::OperatorStatus::Ok, "{:?}", ex.result);
    }

    /// ★ An ordinary move inside the boundary still commits with the law armed
    /// — the gate refuses boundary changes, not operators.
    #[test]
    fn an_ordinary_move_still_commits_with_the_closure_law_armed() {
        let reg = bounded_registry();
        let ex = execute(
            &reg,
            &reg.names(),
            &with_boundary(),
            &bounded_state(),
            "budget.record_income",
            &params(&[
                ("amount", json!(50.0)),
                ("source", json!("salary")),
                ("entry_id", json!("e1")),
            ]),
        );
        assert_eq!(ex.result.status, meta::OperatorStatus::Ok, "{:?}", ex.result);
    }

    // ── the firewall F, at the gate ──────────────────────────────────────────

    fn household() -> Value {
        json!({
            "roster": ["ama"],
            "contacts": ["employer"],
            "finances": {"liquid": {"balance": 1000.0}, "pockets": {},
                         "income": {"monthly_total": 0.0, "sources": []}}
        })
    }

    fn with_firewall(rules: Vec<FlowRule>) -> Enforcement {
        Enforcement {
            boundary: Some(
                crate::boundary::BoundaryDecl::new(
                    crate::boundary::Scope::new()
                        .spanning("finances")
                        .spanning("roster")
                        .with_entity_register("roster"),
                )
                .owning("finances"),
            ),
            firewall: rules,
            ..Enforcement::default()
        }
    }

    /// ★★ The household's real case, through the gate: the SAME operator is
    /// admitted or refused on the strength of who the counterparty is — which
    /// is a fact about the crossing, not about either endpoint.
    #[test]
    fn the_firewall_refuses_an_inflow_from_an_unknown_counterparty() {
        let reg = Registry::default();
        let rules = vec![FlowRule::OnlyKnown {
            id: "contacts_only".into(),
            dir: Some(FlowDirection::In),
            register: "contacts".into(),
        }];

        // Income from a known counterparty: admitted.
        let ok = execute(
            &reg, &reg.names(), &with_firewall(rules.clone()), &household(),
            "budget.record_income",
            &params(&[("amount", json!(500.0)), ("source", json!("employer")),
                      ("entry_id", json!("e1"))]),
        );
        assert_eq!(ok.result.status, meta::OperatorStatus::Ok, "{:?}", ok.result);

        // The same amount from a stranger: refused at the firewall, and
        // nothing commits.
        let before = household();
        let no = execute(
            &reg, &reg.names(), &with_firewall(rules), &before,
            "budget.record_income",
            &params(&[("amount", json!(500.0)), ("source", json!("loan_shark")),
                      ("entry_id", json!("e2"))]),
        );
        assert_eq!(no.result.status, meta::OperatorStatus::Failed);
        assert_eq!(no.result.constraint_violated.as_deref(), Some("firewall"));
        assert_eq!(no.state, before);
        assert!(no.mutations.is_empty() && no.events.is_empty());
    }

    /// ★★ Pocket-to-pocket is INTERNAL, so a firewall that forbids every
    /// crossing still admits it. `μ` owns both ends, so nothing crossed `B`.
    #[test]
    fn an_internal_move_passes_a_firewall_that_forbids_everything() {
        let reg = Registry::default();
        let forbid_all = vec![FlowRule::Forbid {
            id: "sealed".into(),
            dir: None,
            kind: None,
        }];
        let ex = execute(
            &reg, &reg.names(), &with_firewall(forbid_all), &household(),
            "budget.allocate",
            &params(&[("pocket_name", json!("food")), ("amount", json!(100.0)),
                      ("period", json!("monthly"))]),
        );
        assert_eq!(ex.result.status, meta::OperatorStatus::Ok, "{:?}", ex.result);
    }

    /// A sustain that declares rules but no boundary leaves them inert rather
    /// than guessing at a boundary — and one that declares neither is
    /// untouched by this row entirely.
    #[test]
    fn the_firewall_is_inert_without_a_boundary_to_cross() {
        let reg = Registry::default();
        let rules_only = Enforcement {
            firewall: vec![FlowRule::Forbid { id: "sealed".into(), dir: None, kind: None }],
            ..Enforcement::default()
        };
        let ex = execute(
            &reg, &reg.names(), &rules_only, &household(), "budget.record_income",
            &params(&[("amount", json!(500.0)), ("source", json!("anyone")),
                      ("entry_id", json!("e3"))]),
        );
        assert_eq!(ex.result.status, meta::OperatorStatus::Ok, "{:?}", ex.result);
    }

    // ── the author-time obligation: it FLAGS, it does not block ──────────────

    /// A genuinely unsound Enzyme: its effect sets the pocket negative while
    /// its postcondition declares the opposite. Registered rather than hidden
    /// so the audit and the gate can both be run against the same real thing.
    fn unsound_op() -> crate::operator::meta::OperatorMeta {
        crate::operator::meta::OperatorMeta {
            name: "test.unsound",
            description: "Declares a postcondition its own effect makes impossible.",
            params: vec![],
            constraints: vec![],
            post_constraints: vec!["finances.pockets.food.allocated >= 0".into()],
            side_effects: vec![],
            pawa_cost: 0,
            protocol: crate::operator::meta::Protocol::Rpc,
            min_privilege: 0,
            effect: Some(crate::compose::EffectSummary::new().with(
                "finances.pockets.food.allocated",
                crate::compose::Change::SetTo(json!(-999.0)),
            )),
            run: |state, _p, _e, _m| {
                let _ = state.set("finances.pockets.food.allocated", json!(-999.0));
                OperatorResult::ok(json!({}))
            },
        }
    }

    /// ★★ Both halves of §III at once: the static check **flags** it at author
    /// time, and it **still runs**, with the dynamic check catching it after
    /// the fact — *"the postcondition then catches it after the fact rather
    /// than preventing it. Both are useful, and they are not the same thing."*
    #[test]
    fn an_unsound_operator_is_flagged_at_author_time_and_still_runs() {
        let meta = unsound_op();

        // Static: flagged.
        let report = crate::obligation::check_obligation(&meta);
        assert!(report.is_flagged(), "{}", report.describe());

        // Dynamic: it runs, and the post-condition catches it after the fact.
        let mut reg = Registry::default();
        reg.register(meta);
        let before = json!({"finances": {"pockets": {"food": {"allocated": 10.0}}}});
        let ex = execute(
            &reg, &reg.names(), &Enforcement::default(), &before,
            "test.unsound", &params(&[]),
        );
        assert!(!ex.committed(), "the DYNAMIC check should catch it");
        // The post-condition refusal shares the gate's tag; the reason names
        // which post-condition it was.
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("enforcement_gate"));
        assert!(
            ex.result.reason.as_deref().unwrap_or("").contains("post-condition"),
            "{:?}", ex.result.reason
        );

        // ★ And the flag is not a gate: nothing in `execute` consulted it.
        //   A sound operator with the same shape commits normally.
        let mut sound = unsound_op();
        sound.name = "test.sound";
        sound.post_constraints = vec!["finances.pockets.food.allocated <= 0".into()];
        assert!(!crate::obligation::check_obligation(&sound).is_flagged());
        let mut reg2 = Registry::default();
        reg2.register(sound);
        let ok = execute(
            &reg2, &reg2.names(), &Enforcement::default(), &before,
            "test.sound", &params(&[]),
        );
        assert!(ok.committed(), "{:?}", ok.result);
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
        Authorization::Principal { id, memberships: m, path, skins: SkinRegistry::none() }
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

    // ── the confused deputy, run at the gate ─────────────────────────────────

    /// A Symbiont acting on Bonnie's authority over an input it did not choose.
    ///
    /// The untrusted text names the sustain. Under ambient authority the deputy
    /// applies everything Bonnie holds to that name — Hardy's compiler, exactly.
    /// Under a capability the same deputy, the same principal and the same input
    /// are refused, because the authority arrived *with* the request and
    /// designates one object.
    fn deputy_setup() -> Memberships {
        use crate::principal::{MembershipEdge, TIER_OWNER};
        let mut m = Memberships::new();
        m.grant(MembershipEdge { principal: "bonnie".into(), sustain: "house".into(), tier: TIER_OWNER, skin: None });
        m.grant(MembershipEdge { principal: "bonnie".into(), sustain: "habitat".into(), tier: TIER_OWNER, skin: None });
        m
    }

    fn spend_params() -> Map<String, Value> {
        params(&[("pocket_name", json!("food")), ("amount", json!(10.0)),
                 ("period", json!("monthly"))])
    }

    #[test]
    fn ambient_authority_lets_a_deputy_reach_what_the_request_never_authorised() {
        let m = deputy_setup();
        let reg = Registry::default();

        // The task was about the habitat. The untrusted input says otherwise,
        // and ambient authority has no way to know the difference.
        let untrusted_target = "house".to_string();
        let path = vec![untrusted_target];

        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &spend_params(), &as_principal("bonnie", &m, &path));
        assert!(
            ex.committed(),
            "this is the vulnerability, asserted so the fix has something to be a fix OF"
        );
    }

    #[test]
    fn a_capability_refuses_the_same_deputy_the_same_input() {
        use crate::capability::{Attenuation, Capability, Rights};
        use crate::principal::{SkinRegistry, TIER_OWNER};
        let m = deputy_setup();
        let reg = Registry::default();

        // Bonnie hands the Symbiont authority over the habitat ONLY, narrowed
        // to the one operator the task needs.
        let cap = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat")
            .unwrap()
            .attenuate(&Attenuation::to_rights(Rights::only(["budget.allocate"])))
            .unwrap();
        assert_eq!(cap.tier(), TIER_OWNER);

        // Same untrusted input. Same principal's authority. Refused.
        let before = homestead_state();
        let ex = execute_as(&reg, &allowed(), &armed(), &before, "budget.allocate",
                            &spend_params(),
                            &Authorization::Capability { capability: &cap, target: "house" });
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("capability_designates"));
        assert!(ex.mutations.is_empty(), "a refusal changes nothing");
        assert!(ex.events.is_empty());
        assert_eq!(ex.state, before, "state untouched");

        // And it still does the job it WAS given.
        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &spend_params(),
                            &Authorization::Capability { capability: &cap, target: "habitat" });
        assert!(ex.committed(), "the authorised task is unaffected");
    }

    #[test]
    fn a_capability_narrowed_to_one_operator_refuses_the_others() {
        use crate::capability::{Attenuation, Capability, Rights};
        use crate::principal::SkinRegistry;
        let m = deputy_setup();
        let reg = Registry::default();

        let cap = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat")
            .unwrap()
            .attenuate(&Attenuation::to_rights(Rights::only(["budget.summary"])))
            .unwrap();

        let ex = execute_as(&reg, &allowed(), &armed(), &homestead_state(), "budget.allocate",
                            &spend_params(),
                            &Authorization::Capability { capability: &cap, target: "habitat" });
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("capability_carries"));
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
            params: vec![],
            constraints: vec![],
            post_constraints: vec![],
            side_effects: vec!["event.test.reshaped"],
            pawa_cost: 0,
            protocol: Protocol::Rpc,
            min_privilege: 0,
            effect: None,
            run: |state, _p, events, _movements| {
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

    // ── the approval token at the gate (Operative §XVI · N1) ────────────────

    use crate::approval::{ApprovalToken, EffectClass, NonceLedger, Simulated};
    use crate::council::ProposalStatus;

    fn allocate_params() -> Map<String, Value> {
        params(&[("pocket_name", json!("food")), ("amount", json!(250.0)),
                 ("period", json!("monthly"))])
    }

    /// The only route: simulate → vote → approve.
    fn approved(op: &str, p: &Map<String, Value>, who: &str, nonce: u64, expiry: u64) -> ApprovalToken {
        Simulated::from_sandbox("prop-1", Binding::new(op, p), Ok(()))
            .expect("the sandbox run committed")
            .voted(ProposalStatus::Passed)
            .expect("the council passed it")
            .approve(who, nonce, expiry)
    }

    fn run_live(token: &ApprovalToken, now: u64, nonces: &mut NonceLedger,
                op: &str, p: &Map<String, Value>) -> Execution {
        execute_admitted(
            &Registry::default(), &allowed(), &armed(), &homestead_state(), op, p,
            &Authorization::Unchecked, &EffectClass::Live { token, now }, nonces,
        )
    }

    #[test]
    fn a_live_effect_with_a_matching_approval_commits() {
        let t = approved("budget.allocate", &allocate_params(), "bonnie", 1, 100);
        let mut nonces = NonceLedger::new();
        let ex = run_live(&t, 50, &mut nonces, "budget.allocate", &allocate_params());
        assert!(ex.committed());
        assert_eq!(ex.state["finances"]["liquid"]["balance"], json!(750.0));
    }

    #[test]
    fn an_approval_for_a_different_amount_does_not_admit_this_one() {
        // "Yes, allocate 250" must not admit 900. This is the whole reason the
        // token binds to (o, θ) rather than to o.
        let t = approved("budget.allocate", &allocate_params(), "bonnie", 1, 100);
        let mut nonces = NonceLedger::new();
        let other = params(&[("pocket_name", json!("food")), ("amount", json!(900.0)),
                             ("period", json!("monthly"))]);
        let ex = run_live(&t, 50, &mut nonces, "budget.allocate", &other);
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("approval_token"));
        assert_eq!(ex.state, homestead_state(), "a refusal changes nothing");
    }

    #[test]
    fn an_approval_is_spent_once_and_the_replay_is_refused() {
        let t = approved("budget.allocate", &allocate_params(), "bonnie", 7, 100);
        let mut nonces = NonceLedger::new();

        let first = run_live(&t, 50, &mut nonces, "budget.allocate", &allocate_params());
        assert!(first.committed());

        let replay = run_live(&t, 50, &mut nonces, "budget.allocate", &allocate_params());
        assert!(!replay.committed(), "at-least-once delivery must not spend one approval twice");
        assert_eq!(replay.result.constraint_violated.as_deref(), Some("approval_token"));
        assert_eq!(replay.state, homestead_state());
    }

    #[test]
    fn an_expired_approval_is_refused_at_commit() {
        let t = approved("budget.allocate", &allocate_params(), "bonnie", 1, 100);
        let mut nonces = NonceLedger::new();
        let ex = run_live(&t, 101, &mut nonces, "budget.allocate", &allocate_params());
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("approval_token"));
    }

    #[test]
    fn a_refused_call_does_not_burn_the_approval() {
        // Turned back by the operator's own guard: the person should be free to
        // fix the state and use the same approval, not have it consumed.
        let big = params(&[("pocket_name", json!("food")), ("amount", json!(5000.0)),
                           ("period", json!("monthly"))]);
        let t = approved("budget.allocate", &big, "bonnie", 3, 100);
        let mut nonces = NonceLedger::new();

        let ex = run_live(&t, 10, &mut nonces, "budget.allocate", &big);
        assert!(!ex.committed(), "5000 > 1000 balance");
        assert!(!nonces.is_spent(3), "a refusal must leave the approval unspent");
    }

    #[test]
    fn a_sandbox_effect_needs_no_approval_but_still_meets_the_whole_gate() {
        let mut nonces = NonceLedger::new();
        let reg = Registry::default();

        // No token, and it runs.
        let ok = execute_admitted(&reg, &allowed(), &armed(), &homestead_state(),
                                  "budget.allocate", &allocate_params(),
                                  &Authorization::Unchecked, &EffectClass::Sandbox, &mut nonces);
        assert!(ok.committed(), "a sandboxed effect is exempt from the token, not from the gate");

        // ...but the invariants still bind inside the sandbox: a fork can never
        // reach a state the real Sustain would refuse.
        let state = json!({"finances":{"liquid":{"balance":100.0},
                                       "pockets":{"food":{"allocated":10.0,"spent":0.0,"limit":0.0}},
                                       "income":{"monthly_total":0.0,"sources":[]}}});
        let refused = execute_admitted(&reg, &allowed(), &armed(), &state,
                                       "test.force_negative", &params(&[]),
                                       &Authorization::Unchecked, &EffectClass::Sandbox, &mut nonces);
        assert!(!refused.committed());
        assert_eq!(refused.result.constraint_violated.as_deref(), Some("enforcement_gate"));
    }

    #[test]
    fn one_persons_approval_does_not_admit_anothers_act() {
        let t = approved("budget.allocate", &allocate_params(), "bonnie", 1, 100);
        let m = memberships();
        let path = vec!["house".to_string()];
        let mut nonces = NonceLedger::new();
        let ex = execute_admitted(
            &Registry::default(), &allowed(), &armed(), &homestead_state(),
            "budget.allocate", &allocate_params(),
            &as_principal("cira", &m, &path),
            &EffectClass::Live { token: &t, now: 50 }, &mut nonces,
        );
        assert!(!ex.committed(), "cira may allocate, but bonnie's approval is not hers to spend");
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("approval_token"));
    }

    #[test]
    fn authority_is_still_decided_before_the_approval() {
        // A perfectly good approval does not make an observer an actor. The two
        // conjuncts are independent, and both must hold.
        let t = approved("budget.allocate", &allocate_params(), "guest", 1, 100);
        let m = memberships();
        let path = vec!["house".to_string()];
        let mut nonces = NonceLedger::new();
        let ex = execute_admitted(
            &Registry::default(), &allowed(), &armed(), &homestead_state(),
            "budget.allocate", &allocate_params(),
            &as_principal("guest", &m, &path),
            &EffectClass::Live { token: &t, now: 50 }, &mut nonces,
        );
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("insufficient_privilege"));
        assert!(!nonces.is_spent(1), "refused before the approval was ever reached");
    }

    #[test]
    fn unchecked_preserves_r1_behaviour_exactly() {
        // Every vector recorded before this slice runs through here.
        let mut nonces = NonceLedger::new();
        let ex = execute_admitted(
            &Registry::default(), &allowed(), &armed(), &homestead_state(),
            "budget.allocate", &allocate_params(),
            &Authorization::Unchecked, &EffectClass::Unchecked, &mut nonces,
        );
        let baseline = execute(&Registry::default(), &allowed(), &armed(), &homestead_state(),
                               "budget.allocate", &allocate_params());
        assert_eq!(ex.committed(), baseline.committed());
        assert_eq!(ex.state, baseline.state);
    }

    // ── D(s, o(s)) at the gate (Constraint §I, §VI · CON-1, CON-7) ──────────

    use crate::transition::{Quantity, Tolerance, TransitionRule};

    /// Money as integer minor units, which is what makes the law exact.
    fn minor_units_state() -> Value {
        json!({"finances":{"liquid":{"balance":100000},
                           "pockets":{"food":{"allocated":20000,"spent":0,"limit":0}},
                           "income":{"monthly_total":0,"sources":[]}}})
    }

    fn conserving() -> Enforcement {
        Enforcement {
            enabled: true,
            invariants: vec![],
            schema: None,
            boundary: None,
            firewall: vec![],
            transitions: vec![TransitionRule::Conservation {
                id: "money_conserved".into(),
                quantity: Quantity::new(
                    "household money",
                    &["finances.liquid.balance", "finances.pockets[*].allocated"],
                ),
                tolerance: Tolerance::Exact,
            }],
        }
    }

    /// Adds to a pocket without debiting liquid — money from nowhere. No state
    /// constraint can catch this: every endpoint it produces is perfectly valid.
    fn with_minting_operator() -> (Registry, Vec<String>) {
        use crate::operator::meta::{OperatorMeta, Protocol};
        let mut reg = Registry::default();
        reg.register(OperatorMeta {
            name: "test.mint",
            description: "Credits a pocket with no matching debit. Exists to prove D refuses it.",
            params: vec![],
            constraints: vec![],
            post_constraints: vec![],
            side_effects: vec!["event.test.minted"],
            pawa_cost: 0,
            protocol: Protocol::Rpc,
            min_privilege: 0,
            effect: None,
            run: |state, _p, events, _movements| {
                let _ = state.set("finances.pockets.food.allocated", json!(25000));
                events.push(EmittedEvent { name: "event.test.minted".into(), payload: json!({}) });
                OperatorResult::ok(json!({}))
            },
        });
        let mut allow = reg.names();
        allow.push("test.mint".to_string());
        (reg, allow)
    }

    #[test]
    fn the_gate_refuses_a_step_that_creates_money() {
        // The flagship case. Before this slice it was INEXPRESSIBLE: both
        // evaluators take one state, and the after-state here is entirely valid.
        let (reg, allow) = with_minting_operator();
        let before = minor_units_state();
        let ex = execute(&reg, &allow, &conserving(), &before, "test.mint", &params(&[]));

        assert!(!ex.committed(), "5,000 minor units appeared from nowhere");
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("transition_constraint"));
        let why = ex.result.reason.as_deref().unwrap_or_default();
        assert!(why.contains("created 5000 minor units"), "{why}");
        assert_eq!(ex.state, before, "a refusal changes nothing");
        assert!(ex.mutations.is_empty());
        assert!(ex.events.is_empty());
    }

    #[test]
    fn the_after_state_alone_is_perfectly_admissible() {
        // Proves the point of the family: with the SAME sustain rules but no
        // transition constraint, the very same step commits — because nothing
        // about the resulting state is wrong. Only the step is.
        let (reg, allow) = with_minting_operator();
        let no_d = Enforcement { transitions: vec![], ..conserving() };
        let ex = execute(&reg, &allow, &no_d, &minor_units_state(), "test.mint", &params(&[]));
        assert!(ex.committed(), "a state constraint cannot see what D sees");
    }

    #[test]
    fn a_genuine_transfer_passes_the_conservation_law() {
        let reg = Registry::default();
        let ex = execute(&reg, &allowed(), &conserving(), &minor_units_state(),
                         "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(5000)),
                                   ("period", json!("monthly"))]));
        assert!(ex.committed(), "{:?}", ex.result.reason);
        // Debited from liquid, credited to the pocket: the total is unmoved.
        // The engine promotes to float on arithmetic (Python's rule, preserved),
        // which is exactly why exact mode accepts an INTEGRAL float — 95000.0 is
        // the same count of minor units as 95000.
        assert_eq!(ex.state["finances"]["liquid"]["balance"], json!(95000.0));
        assert_eq!(ex.state["finances"]["pockets"]["food"]["allocated"], json!(25000.0));
    }

    #[test]
    fn a_transition_constraint_binds_even_when_the_sustain_is_unarmed() {
        // `enforcement.enabled` gates the sustain's own invariants — an opt-in
        // tightening of a viable region. A conservation law is a different
        // claim: that a quantity does not change. Declaring one and having it
        // ignored because a flag was off would be the gate failing open.
        let (reg, allow) = with_minting_operator();
        let unarmed = Enforcement { enabled: false, ..conserving() };
        let ex = execute(&reg, &allow, &unarmed, &minor_units_state(), "test.mint", &params(&[]));
        assert!(!ex.committed(), "a declared conservation law is not opt-in");
    }

    #[test]
    fn a_rate_limit_refuses_a_jump_the_endpoints_cannot_show() {
        let reg = Registry::default();
        let enf = Enforcement {
            enabled: false,
            invariants: vec![],
            schema: None,
            boundary: None,
            firewall: vec![],
            transitions: vec![TransitionRule::RateLimit {
                id: "no_big_moves".into(),
                path: "finances.liquid.balance".into(),
                delta: 1000.0,
            }],
        };
        let ex = execute(&reg, &allowed(), &enf, &minor_units_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(5000)),
                                   ("period", json!("monthly"))]));
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("transition_constraint"));
    }

    #[test]
    fn the_refusal_names_which_law_refused() {
        let (reg, allow) = with_minting_operator();
        let ex = execute(&reg, &allow, &conserving(), &minor_units_state(), "test.mint", &params(&[]));
        let why = ex.result.reason.as_deref().unwrap_or_default();
        assert!(why.contains("money_conserved"),
                "the difference between an explanation and a shrug: {why}");
    }

    #[test]
    fn a_rule_that_will_not_compile_refuses_rather_than_failing_open() {
        let reg = Registry::default();
        let broken = Enforcement {
            enabled: true,
            invariants: vec![("broken".into(), "this is (not ) valid".into())],
            schema: None,
            boundary: None,
            firewall: vec![],
            transitions: vec![],
        };
        let ex = execute(&reg, &allowed(), &broken, &homestead_state(), "budget.allocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(1.0)),
                                   ("period", json!("monthly"))]));
        assert!(!ex.committed(), "an unreadable rule must not be silently skipped");
    }

    // ── declared inverses ─────────────────────────────────────────────────────
    //
    // ★★★ §V's second clause is the one worth testing: an inverse that refuses
    // at exactly the states the forward move produces is not an inverse. Each
    // pair below runs the forward move first and then asks its inverse to undo
    // precisely that, which is the state it must never refuse.

    /// Run a sequence of calls, threading state through, and hand back the last state.
    fn run_all(mut state: Value, calls: &[(&str, Vec<(&str, Value)>)]) -> Value {
        let reg = Registry::default();
        for (name, ps) in calls {
            let ex = execute(&reg, &allowed(), &armed(), &state, name, &params(ps));
            assert!(ex.committed(), "{name} was refused: {:?}", ex.result.reason);
            state = ex.state.clone();
        }
        state
    }

    fn at(state: &Value, path: &str) -> f64 {
        let mut cur = state;
        for seg in path.split('.') {
            cur = cur.get(seg).unwrap_or_else(|| panic!("no {path}"));
        }
        cur.as_f64().unwrap_or_else(|| panic!("{path} is not a number"))
    }

    #[test]
    fn unspend_puts_the_room_back_and_leaves_liquid_alone() {
        let after = run_all(
            homestead_state(),
            &[
                ("budget.allocate", vec![("pocket_name", json!("food")), ("amount", json!(300.0))]),
                ("budget.spend", vec![("pocket_name", json!("food")), ("amount", json!(120.0))]),
                ("budget.unspend", vec![("pocket_name", json!("food")), ("amount", json!(120.0))]),
            ],
        );
        assert_eq!(at(&after, "finances.pockets.food.spent"), 0.0);
        assert_eq!(at(&after, "finances.pockets.food.allocated"), 300.0, "the earmark stands");
        assert_eq!(
            at(&after, "finances.liquid.balance"),
            700.0,
            "a refund of a spend does not touch liquid, because the spend never did"
        );
    }

    #[test]
    fn unallocate_returns_unspent_money_to_liquid() {
        let after = run_all(
            homestead_state(),
            &[
                ("budget.allocate", vec![("pocket_name", json!("food")), ("amount", json!(300.0))]),
                ("budget.unallocate", vec![("pocket_name", json!("food")), ("amount", json!(300.0))]),
            ],
        );
        assert_eq!(at(&after, "finances.pockets.food.allocated"), 0.0);
        assert_eq!(at(&after, "finances.liquid.balance"), 1000.0, "back where it started");
    }

    #[test]
    fn the_pair_returns_a_backfilled_charge_to_exactly_where_it_was() {
        // ★★★ The whole point of case 2. A charge filed the backfill way is an
        // allocate and a spend of the same amount; undoing it is the two
        // inverses in reverse order, and the household must land byte-identical
        // to before the charge.
        let before = homestead_state();
        let after = run_all(
            before.clone(),
            &[
                ("budget.allocate", vec![("pocket_name", json!("food")), ("amount", json!(250.0))]),
                ("budget.spend", vec![("pocket_name", json!("food")), ("amount", json!(250.0))]),
                ("budget.unspend", vec![("pocket_name", json!("food")), ("amount", json!(250.0))]),
                ("budget.unallocate", vec![("pocket_name", json!("food")), ("amount", json!(250.0))]),
            ],
        );
        assert_eq!(at(&after, "finances.liquid.balance"), at(&before, "finances.liquid.balance"));
        assert_eq!(at(&after, "finances.pockets.food.allocated"), 0.0);
        assert_eq!(at(&after, "finances.pockets.food.spent"), 0.0);
    }

    #[test]
    fn unspend_refuses_more_than_was_ever_spent() {
        let reg = Registry::default();
        let state = run_all(
            homestead_state(),
            &[
                ("budget.allocate", vec![("pocket_name", json!("food")), ("amount", json!(300.0))]),
                ("budget.spend", vec![("pocket_name", json!("food")), ("amount", json!(100.0))]),
            ],
        );
        let ex = execute(&reg, &allowed(), &armed(), &state, "budget.unspend",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(150.0))]));
        assert!(!ex.committed(), "there was never KES 150 of spending to give back");
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("pocket_spent_sufficient"));
        // And the refusal carries the numbers, so a caller can act without parsing prose.
        assert_eq!(ex.result.data.get("spent").and_then(|v| v.as_f64()), Some(100.0));
    }

    #[test]
    fn unallocate_refuses_to_claw_back_money_already_spent() {
        // ★★★ Guarding on the whole allocation rather than the unspent part
        // would leave `spent > allocated`, which is a pocket claiming to have
        // spent money it never held.
        let reg = Registry::default();
        let state = run_all(
            homestead_state(),
            &[
                ("budget.allocate", vec![("pocket_name", json!("food")), ("amount", json!(300.0))]),
                ("budget.spend", vec![("pocket_name", json!("food")), ("amount", json!(200.0))]),
            ],
        );
        let ex = execute(&reg, &allowed(), &armed(), &state, "budget.unallocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(300.0))]));
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("pocket_unspent_sufficient"));
        assert_eq!(ex.result.data.get("unspent").and_then(|v| v.as_f64()), Some(100.0));
    }

    #[test]
    fn an_inverse_on_a_pocket_that_never_existed_is_refused_not_invented() {
        let reg = Registry::default();
        for op in ["budget.unspend", "budget.unallocate"] {
            let ex = execute(&reg, &allowed(), &armed(), &homestead_state(), op,
                             &params(&[("pocket_name", json!("ghost")), ("amount", json!(10.0))]));
            assert!(!ex.committed(), "{op} conjured a pocket");
            assert_eq!(ex.result.constraint_violated.as_deref(), Some("pocket_exists"));
        }
    }

    #[test]
    fn the_inverses_go_through_the_same_gate_as_everything_else() {
        // A zero or negative amount is refused by the declared constraint, not
        // by anything the operator body does.
        let reg = Registry::default();
        let state = run_all(
            homestead_state(),
            &[("budget.allocate", vec![("pocket_name", json!("food")), ("amount", json!(300.0))])],
        );
        let ex = execute(&reg, &allowed(), &armed(), &state, "budget.unallocate",
                         &params(&[("pocket_name", json!("food")), ("amount", json!(0.0))]));
        assert!(!ex.committed(), "the gate holds for an inverse exactly as for a forward move");
    }

}
