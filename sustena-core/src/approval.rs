//! The approval token, inside the gate (Operative §XVI · Capstone §VI.1 duty 3).
//!
//! ```text
//! admit(o,s) ⟺ g_o(s) ∧ o(s)∈A ∧ D(s,o(s))          ← §4E, already built
//!              ∧ (effect_class = sandbox ∨ valid_token(approve(o, principal)))
//! ```
//!
//! A sandboxed effect needs no token. A **live** effect does. The article's
//! closing form is the specification this module implements literally:
//!
//! > An unapproved live effect is not blocked, penalised, or flagged for
//! > review. There is no admitted operator application that produces it.
//! > **It is not a move.**
//!
//! ## The token
//!
//! ```text
//! τ = ⟨hash(o,θ), principal, proposal_id, nonce, expiry⟩
//! ```
//!
//! - **Bound to (o,θ)** — otherwise it approves a *shape* of act, not an act.
//!   "Yes, pay the rent" must not admit a different amount to a different
//!   payee.
//! - **Single-use (nonce)** — delivery is at-least-once, so a replayable
//!   approval is one you gave once and spent twice.
//! - **Expiring, re-admitted at commit** — the state at approval time need not
//!   be the state at commit time.
//!
//! ## Three things are unrepresentable here, not merely checked
//!
//! Build clean: prefer making a defect impossible to writing a guard against
//! it (R2 working rule 4).
//!
//! 1. **A live effect with no token.** [`EffectClass::Live`] *carries* the
//!    token. There is no way to spell "live, unapproved" — the enum variant
//!    that would express it does not exist.
//!
//! 2. **An approval that skipped simulation or the vote.** [`ApprovalToken`]
//!    has private fields and no public constructor. The only way to obtain one
//!    is [`Simulated`] → [`Voted`] → [`Voted::approve`], and each step
//!    **consumes the previous by value**. That is OPV-31's trace invariant —
//!    `e_sim ≺ e_vote ≺ e_approve ≺ e` — enforced by move semantics rather
//!    than by a runtime scan of the log that something could forget to run.
//!    The invariant is prefix-closed, hence a safety property (Alpern–Schneider),
//!    hence in the enforceable class; this is what "enforceable" cashes out to.
//!
//! 3. **A token matching the wrong act.** The binding is kept
//!    **structurally** — the operator name and the actual parameters — and
//!    compared by equality, not by digest. A hash would introduce a collision
//!    surface for a property that does not need one. [`Binding::fingerprint`]
//!    exists for logs and display and is documented as **not** a security
//!    primitive.
//!
//! ## Depth cannot get around it (Corollary 2)
//!
//! Admittance composes along the holon path, so a sub-operative nested three
//! deep meets the same conjunction and **adds** a conjunct rather than escaping
//! one. Nothing here is per-level: there is one gate, and this clause is inside
//! it.
//!
//! ## No clock
//!
//! The core has no clock (see the crate docs), so expiry is judged against a
//! `now` supplied by the host, in the same units the host issued the token in.
//! A core whose promise is reproducibility cannot read the wall clock.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::council::ProposalStatus;

/// The act a token approves: an operator and the exact parameters it was
/// approved for.
///
/// Kept structurally rather than as `hash(o,θ)`. The article writes a hash
/// because it is describing a value that may travel; inside this core the
/// binding never leaves, so equality on the real thing is strictly stronger —
/// there is no collision to find.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    operator: String,
    params: Map<String, Value>,
}

impl Binding {
    pub fn new(operator: &str, params: &Map<String, Value>) -> Self {
        Self { operator: operator.to_string(), params: params.clone() }
    }

    pub fn operator(&self) -> &str {
        &self.operator
    }

    pub fn params(&self) -> &Map<String, Value> {
        &self.params
    }

    /// A short, stable, **non-cryptographic** fingerprint — FNV-1a over the
    /// canonical JSON — for logs, ids and display.
    ///
    /// Deliberately not used for any decision. Admission compares the binding
    /// itself (see [`Binding`]); if this ever becomes the thing compared —
    /// because tokens start crossing a trust boundary — it must be replaced by
    /// a real digest first. Recorded as such rather than left to be discovered.
    pub fn fingerprint(&self) -> u64 {
        let canonical = serde_json::to_string(&serde_json::json!({
            "operator": self.operator,
            "params": self.params,
        }))
        .unwrap_or_default();

        // FNV-1a 64. Explicit rather than DefaultHasher, whose output is not
        // guaranteed stable across releases — a conformance contract cannot
        // rest on that.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in canonical.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

/// Why a token was refused. Each variant is a distinct answer, because
/// "expired" and "already spent" call for different things from the person.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TokenError {
    #[error("approval does not match this act: approved '{approved}', attempted '{attempted}'")]
    WrongAct { approved: String, attempted: String },

    #[error("approval is for the same operator but different parameters — an approval binds to an act, not a shape of act")]
    WrongParams,

    #[error("approval was given by '{approved}', but '{acting}' is acting")]
    WrongPrincipal { approved: String, acting: String },

    #[error("approval expired at {expiry} (now {now})")]
    Expired { expiry: u64, now: u64 },

    #[error("approval has already been spent (nonce {nonce}) — an approval is single-use")]
    AlreadySpent { nonce: u64 },
}

/// Why a trace could not advance. The stages exist to make the OPV-31 order
/// structural, so these are the only two ways to be turned back.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TraceError {
    #[error("a proposal must be simulated successfully before it can be voted on; the sandbox run was refused: {reason}")]
    SandboxRefused { reason: String },

    #[error("only a PASSED proposal can be approved; this one is {status}")]
    NotPassed { status: &'static str },
}

/// Stage 1 — the act has been **simulated**, under the same gate, with
/// `effect_class = sandbox`.
///
/// Constructed only from a sandbox run that actually committed. A simulation
/// the gate refused cannot advance: there is nothing to approve.
#[derive(Debug, Clone)]
pub struct Simulated {
    proposal_id: String,
    binding: Binding,
}

impl Simulated {
    /// Record that `binding` was simulated under proposal `proposal_id`.
    ///
    /// `sandbox_committed` is the verdict of the sandbox run — the caller has
    /// just executed with [`EffectClass::Sandbox`], and passes what the gate
    /// said. A refusal stops the chain here.
    pub fn from_sandbox(
        proposal_id: &str,
        binding: Binding,
        sandbox_committed: Result<(), String>,
    ) -> Result<Self, TraceError> {
        sandbox_committed
            .map_err(|reason| TraceError::SandboxRefused { reason })
            .map(|()| Self { proposal_id: proposal_id.to_string(), binding })
    }

    pub fn proposal_id(&self) -> &str {
        &self.proposal_id
    }

    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// Stage 2 — the council has resolved. Consumes the simulation, so the
    /// order cannot be reversed or the step skipped.
    ///
    /// Only [`ProposalStatus::Passed`] advances. `Deferred` in particular is
    /// never an auto-approval — the deadline passing is not a decision.
    pub fn voted(self, status: ProposalStatus) -> Result<Voted, TraceError> {
        match status {
            ProposalStatus::Passed => Ok(Voted { proposal_id: self.proposal_id, binding: self.binding }),
            other => Err(TraceError::NotPassed { status: other.as_str() }),
        }
    }
}

/// Stage 2 — simulated **and** voted through. Still not an approval: the
/// council advises, the person decides.
#[derive(Debug, Clone)]
pub struct Voted {
    proposal_id: String,
    binding: Binding,
}

impl Voted {
    pub fn proposal_id(&self) -> &str {
        &self.proposal_id
    }

    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// Stage 3 — a person approves. Consumes the vote.
    ///
    /// This is the only constructor of [`ApprovalToken`] in the crate, which
    /// is what makes `e_sim ≺ e_vote ≺ e_approve` a property of the type
    /// rather than a promise about behaviour.
    pub fn approve(self, principal: &str, nonce: u64, expiry: u64) -> ApprovalToken {
        ApprovalToken {
            binding: self.binding,
            principal: principal.to_string(),
            proposal_id: self.proposal_id,
            nonce,
            expiry,
        }
    }
}

/// τ — a bound, single-use, expiring approval for one specific act.
///
/// No public constructor and no public fields: reachable only through
/// [`Simulated`] → [`Voted`] → [`Voted::approve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalToken {
    binding: Binding,
    principal: String,
    proposal_id: String,
    nonce: u64,
    expiry: u64,
}

impl ApprovalToken {
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn principal(&self) -> &str {
        &self.principal
    }
    pub fn proposal_id(&self) -> &str {
        &self.proposal_id
    }
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
    pub fn expiry(&self) -> u64 {
        self.expiry
    }

    /// `valid_token(approve(o, principal))` — everything except single-use,
    /// which needs the ledger and belongs at commit ([`NonceLedger::redeem`]).
    ///
    /// Checked in the order that gives the most useful answer first: the act,
    /// then who approved it, then whether it is still live.
    pub fn validate(
        &self,
        attempted: &Binding,
        acting_principal: Option<&str>,
        now: u64,
    ) -> Result<(), TokenError> {
        if self.binding.operator != attempted.operator {
            return Err(TokenError::WrongAct {
                approved: self.binding.operator.clone(),
                attempted: attempted.operator.clone(),
            });
        }
        if self.binding.params != attempted.params {
            return Err(TokenError::WrongParams);
        }
        if let Some(acting) = acting_principal {
            if acting != self.principal {
                return Err(TokenError::WrongPrincipal {
                    approved: self.principal.clone(),
                    acting: acting.to_string(),
                });
            }
        }
        if now > self.expiry {
            return Err(TokenError::Expired { expiry: self.expiry, now });
        }
        Ok(())
    }
}

/// The nonces already spent.
///
/// Single-use has to be remembered somewhere; the core holds the decision, the
/// host holds the durability. Redemption happens **inside** the gate at commit
/// — no cached verdict, no fast path (Saltzer & Schroeder, complete mediation).
#[derive(Debug, Clone, Default)]
pub struct NonceLedger {
    spent: BTreeSet<u64>,
}

impl NonceLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a host's durable record of spent nonces.
    pub fn with_spent<I: IntoIterator<Item = u64>>(spent: I) -> Self {
        Self { spent: spent.into_iter().collect() }
    }

    pub fn is_spent(&self, nonce: u64) -> bool {
        self.spent.contains(&nonce)
    }

    pub fn spent(&self) -> impl Iterator<Item = &u64> {
        self.spent.iter()
    }

    /// Spend a token's nonce. The second call for the same nonce is the replay,
    /// and it is refused.
    pub fn redeem(&mut self, token: &ApprovalToken) -> Result<(), TokenError> {
        if !self.spent.insert(token.nonce) {
            return Err(TokenError::AlreadySpent { nonce: token.nonce });
        }
        Ok(())
    }
}

/// Whether an effect is real, and — if it is — what approves it.
///
/// The reason this is an enum carrying the token, rather than a flag beside
/// one, is that "live and unapproved" must not be expressible. There is no
/// variant for it.
#[derive(Debug, Clone)]
pub enum EffectClass<'a> {
    /// No approval model in force — the host has already decided.
    ///
    /// Named for the same reason [`crate::operator::Authorization::Unchecked`]
    /// is: a bypass that reads as ordinary is the problem; one that has to be
    /// spelled out is not. R1 parity calls come through here, so the vectors
    /// recorded before this slice keep passing unchanged.
    Unchecked,
    /// A simulated effect. Runs the full §4E gate, writes nothing real, needs
    /// no approval — Operative §X's fork-and-replay under the *same* rules.
    Sandbox,
    /// A real effect, with the approval that admits it.
    ///
    /// `now` rides along because the core has no clock and expiry is
    /// re-checked at commit; a live effect is the only place time is load
    /// bearing, so it is the only place the host is asked for it.
    Live {
        token: &'a ApprovalToken,
        now: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn rent() -> Binding {
        Binding::new("budget.spend", &params(&[
            ("pocket_name", json!("rent")),
            ("amount", json!(15000.0)),
        ]))
    }

    /// The happy path, and the only path: sim → vote → approve.
    fn token_for(b: Binding, principal: &str, nonce: u64, expiry: u64) -> ApprovalToken {
        Simulated::from_sandbox("prop-1", b, Ok(()))
            .expect("sandbox committed")
            .voted(ProposalStatus::Passed)
            .expect("passed")
            .approve(principal, nonce, expiry)
    }

    #[test]
    fn a_token_admits_the_exact_act_it_was_given_for() {
        let t = token_for(rent(), "bonnie", 1, 100);
        assert_eq!(t.validate(&rent(), Some("bonnie"), 50), Ok(()));
    }

    #[test]
    fn an_approval_binds_to_an_act_not_a_shape_of_act() {
        // "Yes, pay the rent" must not admit a different amount.
        let t = token_for(rent(), "bonnie", 1, 100);
        let different_amount = Binding::new("budget.spend", &params(&[
            ("pocket_name", json!("rent")),
            ("amount", json!(90000.0)),
        ]));
        assert_eq!(t.validate(&different_amount, Some("bonnie"), 50), Err(TokenError::WrongParams));

        // ...nor a different payee.
        let different_payee = Binding::new("budget.spend", &params(&[
            ("pocket_name", json!("someone_else")),
            ("amount", json!(15000.0)),
        ]));
        assert_eq!(t.validate(&different_payee, Some("bonnie"), 50), Err(TokenError::WrongParams));
    }

    #[test]
    fn a_token_for_one_operator_does_not_admit_another() {
        let t = token_for(rent(), "bonnie", 1, 100);
        let other = Binding::new("budget.allocate", t.binding().params());
        match t.validate(&other, Some("bonnie"), 50) {
            Err(TokenError::WrongAct { approved, attempted }) => {
                assert_eq!(approved, "budget.spend");
                assert_eq!(attempted, "budget.allocate");
            }
            other => panic!("expected WrongAct, got {other:?}"),
        }
    }

    #[test]
    fn one_persons_approval_does_not_admit_anothers_act() {
        let t = token_for(rent(), "bonnie", 1, 100);
        assert!(matches!(
            t.validate(&rent(), Some("cira"), 50),
            Err(TokenError::WrongPrincipal { .. })
        ));
    }

    #[test]
    fn an_expired_token_is_refused_at_commit() {
        // The state at approval time need not be the state at commit time,
        // which is the whole reason approvals expire.
        let t = token_for(rent(), "bonnie", 1, 100);
        assert_eq!(t.validate(&rent(), Some("bonnie"), 100), Ok(()), "expiry is inclusive");
        assert!(matches!(
            t.validate(&rent(), Some("bonnie"), 101),
            Err(TokenError::Expired { .. })
        ));
    }

    #[test]
    fn an_approval_is_single_use() {
        let t = token_for(rent(), "bonnie", 7, 100);
        let mut ledger = NonceLedger::new();
        assert_eq!(ledger.redeem(&t), Ok(()));
        assert_eq!(
            ledger.redeem(&t),
            Err(TokenError::AlreadySpent { nonce: 7 }),
            "at-least-once delivery makes a replayable approval one you gave once and spent twice"
        );
    }

    #[test]
    fn a_ledger_loaded_from_the_host_still_refuses_a_replay() {
        // The process restarting is not a way to spend an approval twice.
        let t = token_for(rent(), "bonnie", 7, 100);
        let mut ledger = NonceLedger::with_spent([7]);
        assert!(matches!(ledger.redeem(&t), Err(TokenError::AlreadySpent { .. })));
    }

    // ── the trace invariant, as structure (OPV-31) ──────────────────────────

    #[test]
    fn a_refused_simulation_cannot_be_voted_on() {
        let refused = Simulated::from_sandbox("p", rent(), Err("would violate invariant".into()));
        assert!(matches!(refused, Err(TraceError::SandboxRefused { .. })));
    }

    #[test]
    fn only_a_passed_proposal_can_be_approved() {
        for status in [
            ProposalStatus::InVoting,
            ProposalStatus::Failed,
            ProposalStatus::Deferred,
            ProposalStatus::OverriddenByUser,
        ] {
            let sim = Simulated::from_sandbox("p", rent(), Ok(())).unwrap();
            assert!(
                matches!(sim.voted(status), Err(TraceError::NotPassed { .. })),
                "{status:?} must not advance to an approval"
            );
        }
    }

    #[test]
    fn a_deadline_passing_is_not_a_decision() {
        let sim = Simulated::from_sandbox("p", rent(), Ok(())).unwrap();
        assert!(matches!(
            sim.voted(ProposalStatus::Deferred),
            Err(TraceError::NotPassed { status: "DEFERRED" })
        ));
    }

    /// The structural claim, stated as a test even though the compiler is the
    /// real proof: there is no other way to build one of these.
    #[test]
    fn the_only_route_to_a_token_is_sim_then_vote_then_approve() {
        let t = token_for(rent(), "bonnie", 1, 10);
        assert_eq!(t.proposal_id(), "prop-1");
        // `ApprovalToken { .. }` does not compile outside this module: the
        // fields are private and no `new` exists. Nor does `Voted { .. }`.
        // Skipping a stage is not a thing that can be written down.
    }

    #[test]
    fn the_fingerprint_is_stable_and_binding_sensitive() {
        assert_eq!(rent().fingerprint(), rent().fingerprint());
        let other = Binding::new("budget.spend", &params(&[
            ("pocket_name", json!("rent")),
            ("amount", json!(15001.0)),
        ]));
        assert_ne!(rent().fingerprint(), other.fingerprint());
    }
}
