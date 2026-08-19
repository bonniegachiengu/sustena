//! `holon::transfer` — an **atomic, conserved** move between two Sustains
//! (Composition §4E.6 · the reference's `holon.transfer`).
//!
//! Two households each hold their own state and their own rules. Moving a
//! quantity from one to the other is the single operation that cannot be
//! expressed as *an operator on a Sustain*, because it is a transition on a
//! **pair**. Doing it as two ordinary calls is not a smaller version of this —
//! it is a different thing that loses money:
//!
//! > spend here, then record income there. If the second refuses, the money has
//! > left one household and arrived nowhere.
//!
//! That is why the cockpit refused to approximate it. This is the real one.
//!
//! ## ★★★ Atomicity is structural, not procedural
//!
//! The only way to obtain a leg is to hold a [`TransferSettlement`], and a `TransferSettlement`
//! **always holds both**. There is no constructor, no `Leg::new`, and no
//! function that returns one leg — so "apply the debit, then decide about the
//! credit" is not a sequence a caller can write. A host still has to write two
//! logs atomically, and it can still get *that* wrong; what it cannot do is be
//! handed half a transfer by this module.
//!
//! Everything is decided before anything is produced. Every guard — amount,
//! link, path, sufficiency, conservation, and **both** sides' own gates — runs
//! against candidates, and [`Transfer::Refused`] carries no state at all. A
//! refusal cannot leave a partial anything because a refusal produces nothing.
//!
//! ## ★★★ Conservation is CHECKED, by the core's own conservation law
//!
//! `Σ q(s') = Σ q(s)` is not asserted from the fact that a decrement was
//! matched with an increment. It is evaluated by
//! [`TransitionRule::Conservation`] over the **combined pair** — the same `D`
//! machinery a single Sustain's declared conservation rules run through. The
//! two states are wrapped as `{"from": …, "to": …}` and the quantity is summed
//! across both paths, so the law is checked on the real totals.
//!
//! ★★ And it is checked in **integer minor units** by default
//! ([`Tolerance::Exact`]), which is stricter than the reference's
//! `round(total, 6)`. A conservation law that holds only to the sixth decimal
//! place is not a law; a fractional amount is refused rather than rounded away.
//!
//! ## ★★ Domain-free, so `finances.liquid.balance` appears nowhere
//!
//! The moving dimension is a **declared path**, supplied by the caller. The
//! reference hardcodes money; this core has no money concept anywhere outside
//! doc examples, and baking one in here would be the universality claim failing
//! at the first place it was tested. A household passes
//! `finances.liquid.balance` and gets the reference's behaviour; something
//! structurally unlike a household passes its own dimension and gets the same
//! guarantees.
//!
//! ## ★ What this module does NOT do
//!
//! **It does not persist.** ADR-0001: the core has no I/O. It returns both legs
//! in one value and the host commits them together — which is exactly where
//! crash-safety lives, and where the host's own proof has to be.
//!
//! **It does not mint an id or read a clock.** A `transfer_id` and a timestamp
//! belong to whoever owns a clock and a random source. Generating them here
//! would make the result non-reproducible, which a conformance vector cannot
//! test. Idempotency (the reference's `idempotency_key`) is likewise a store
//! question: the core cannot know what was committed before.

use serde_json::{json, Map, Value};

use crate::mutation::Mutation;
use crate::operator::{EmittedEvent, Enforcement};
use crate::predicate;
use crate::state::State;
use crate::transition::{self, Quantity, Tolerance, TransitionRule};

/// A declared parent/child edge, as the caller's registry holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub child: String,
    pub parent: String,
}

impl Link {
    pub fn new(child: &str, parent: &str) -> Self {
        Self {
            child: child.to_string(),
            parent: parent.to_string(),
        }
    }
}

/// Proof that two Sustains are **directly** linked, one way or the other.
///
/// ★★★ No public constructor. [`transfer`] takes one, so a transfer between two
/// unrelated households is not something a caller can express — rather than
/// something it is politely refused for. The reference checks the link inside
/// the operator; making it a token moves the check to the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    a: String,
    b: String,
}

impl Linked {
    /// The token, if `a` and `b` are directly linked in either direction.
    ///
    /// ★ A Sustain is never linked to itself, whatever the edge list says: a
    /// transfer to yourself is not a small transfer, it is a different thing
    /// with no meaning, and the reference refuses it too.
    pub fn between(a: &str, b: &str, links: &[Link]) -> Option<Self> {
        if a == b {
            return None;
        }
        let joined = links.iter().any(|l| {
            (l.child == a && l.parent == b) || (l.child == b && l.parent == a)
        });
        joined.then(|| Self {
            a: a.to_string(),
            b: b.to_string(),
        })
    }

    pub fn pair(&self) -> (&str, &str) {
        (&self.a, &self.b)
    }

    fn covers(&self, from: &str, to: &str) -> bool {
        (self.a == from && self.b == to) || (self.a == to && self.b == from)
    }
}

/// One side of a transfer: who it is, what it holds, and what it enforces.
#[derive(Debug, Clone)]
pub struct Party {
    pub sustain_id: String,
    pub state: Value,
    /// ★★ That side's OWN rules. Both are checked — a transfer that satisfied
    /// the sender's household and broke the receiver's would be a refusal
    /// nobody made.
    pub enforcement: Enforcement,
}

impl Party {
    pub fn new(sustain_id: &str, state: Value, enforcement: Enforcement) -> Self {
        Self {
            sustain_id: sustain_id.to_string(),
            state,
            enforcement,
        }
    }
}

/// The dimension that moves, and how exactly it must be conserved.
#[derive(Debug, Clone, PartialEq)]
pub struct Moving {
    path: String,
    tolerance: Tolerance,
}

impl Moving {
    /// Money. ★ Exact, in integer minor units — see the module docs.
    pub fn money(path: &str) -> Self {
        Self {
            path: path.to_string(),
            tolerance: Tolerance::Exact,
        }
    }

    /// A genuinely continuous quantity, with the tolerance **named out loud**.
    pub fn real(path: &str, tolerance: f64) -> Self {
        Self {
            path: path.to_string(),
            tolerance: Tolerance::Within(tolerance),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

/// One side's half of a settled transfer.
///
/// ★★★ Obtainable only from a [`TransferSettlement`], which always holds both.
#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    sustain_id: String,
    state_after: Value,
    mutations: Vec<Mutation>,
    event: EmittedEvent,
    balance_after: f64,
}

impl Leg {
    pub fn sustain_id(&self) -> &str {
        &self.sustain_id
    }

    pub fn state_after(&self) -> &Value {
        &self.state_after
    }

    pub fn mutations(&self) -> &[Mutation] {
        &self.mutations
    }

    pub fn event(&self) -> &EmittedEvent {
        &self.event
    }

    pub fn balance_after(&self) -> f64 {
        self.balance_after
    }
}

/// A transfer that passed every gate — **both legs, or nothing**.
///
/// ★ Named the long way because `royalty::Settlement` already exists and means
/// something else: the 41st name collision in this build, resolved by the same
/// house rule every time — the newcomer takes the longer name.
///
/// ★★★ No public constructor and no way to take one leg out without the other
/// coming with it. This is the atomicity guarantee at the type level: a caller
/// cannot construct half a transfer, so the only remaining way to lose money is
/// a host that writes one log and not the other — which is why the host owes
/// its own crash-safety proof.
#[derive(Debug, Clone, PartialEq)]
pub struct TransferSettlement {
    from: Leg,
    to: Leg,
    path: String,
    amount: f64,
    total_before: f64,
    total_after: f64,
}

impl TransferSettlement {
    /// ★ Both, always. There is deliberately no `from()` / `to()` returning one
    /// leg on its own: a caller that could ask for one would be a caller that
    /// could apply one.
    pub fn legs(&self) -> (&Leg, &Leg) {
        (&self.from, &self.to)
    }

    pub fn amount(&self) -> f64 {
        self.amount
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn total_before(&self) -> f64 {
        self.total_before
    }

    pub fn total_after(&self) -> f64 {
        self.total_after
    }

    /// The conservation law, restated for a caller that wants to show it.
    /// Always true — the settlement could not exist otherwise.
    pub fn conserved(&self) -> bool {
        self.total_before == self.total_after
    }
}

/// What the gate decided about a transfer.
///
/// ★★ A refusal carries **no state and no leg**, so "refused but partially
/// applied" is unrepresentable rather than merely avoided.
#[derive(Debug, Clone, PartialEq)]
pub enum Transfer {
    /// ★ Boxed because a settlement carries two whole states and a refusal
    ///   carries two strings; leaving them the same size on the stack would
    ///   make every refusal pay for a commit it did not make.
    Committed(Box<TransferSettlement>),
    Refused { rule: String, reason: String },
}

impl Transfer {
    pub fn committed(&self) -> bool {
        matches!(self, Transfer::Committed(_))
    }

    pub fn settlement(&self) -> Option<&TransferSettlement> {
        match self {
            Transfer::Committed(s) => Some(s.as_ref()),
            Transfer::Refused { .. } => None,
        }
    }
}

fn refuse(rule: &str, reason: String) -> Transfer {
    Transfer::Refused {
        rule: rule.to_string(),
        reason,
    }
}

/// The wrapper keys the conservation law is evaluated over. Synthetic, so they
/// cannot collide with a real dimension name in either state.
const FROM_KEY: &str = "from";
const TO_KEY: &str = "to";

/// **Move `amount` along `moving.path` from one linked Sustain to the other.**
///
/// Everything is decided before anything is produced. Nothing is written; the
/// caller commits both legs together.
pub fn transfer(
    link: &Linked,
    from: &Party,
    to: &Party,
    moving: &Moving,
    amount: f64,
) -> Transfer {
    // ── 1 · the amount ──────────────────────────────────────────────────────
    if !amount.is_finite() || amount <= 0.0 {
        return refuse(
            "amount_positive",
            "Transfer amount must be greater than zero.".to_string(),
        );
    }

    // ── 2 · the two parties ─────────────────────────────────────────────────
    if from.sustain_id == to.sustain_id {
        return refuse(
            "distinct_holons",
            "Cannot transfer a holon to itself.".to_string(),
        );
    }
    if !link.covers(&from.sustain_id, &to.sustain_id) {
        // ★ The token proves SOME pair is linked; this proves it is THIS pair.
        //   A token for a different edge is a real mistake, not a hypothetical.
        return refuse(
            "holon_link_exists",
            format!(
                "the link token is for '{}'/'{}', not '{}'/'{}'.",
                link.a, link.b, from.sustain_id, to.sustain_id
            ),
        );
    }

    // ── 3 · the path resolves to a number on both sides ─────────────────────
    let mut from_state = State::new(from.state.clone());
    let mut to_state = State::new(to.state.clone());
    let (Some(from_before), Some(to_before)) = (
        numeric_at(&from_state, &moving.path),
        numeric_at(&to_state, &moving.path),
    ) else {
        return refuse(
            "path_present",
            format!(
                "'{}' does not hold a number on both '{}' and '{}'.",
                moving.path, from.sustain_id, to.sustain_id
            ),
        );
    };
    let total_before = from_before + to_before;

    // ── 4 · sufficiency ─────────────────────────────────────────────────────
    if from_before < amount {
        return refuse(
            "from_balance_sufficient",
            format!(
                "Transfer of {amount} exceeds '{}''s balance at '{}' ({from_before}).",
                from.sustain_id, moving.path
            ),
        );
    }

    // ── 5 · apply, to CANDIDATES ────────────────────────────────────────────
    let delta = json!(amount);
    if let Err(e) = from_state.decrement(&moving.path, &delta, false) {
        return refuse("transfer_applies_cleanly", format!("{e}"));
    }
    if let Err(e) = to_state.increment(&moving.path, &delta) {
        return refuse("transfer_applies_cleanly", format!("{e}"));
    }
    let (Some(from_after), Some(to_after)) = (
        numeric_at(&from_state, &moving.path),
        numeric_at(&to_state, &moving.path),
    ) else {
        return refuse(
            "path_present",
            format!("'{}' stopped being a number after the move.", moving.path),
        );
    };
    let total_after = from_after + to_after;

    // ── 6 · CONSERVATION, by the core's own law ─────────────────────────────
    //
    // ★★★ Checked over the combined pair with `TransitionRule::Conservation`,
    //   not asserted from the matched decrement/increment above. "Provably
    //   correct" means checking.
    let law = TransitionRule::Conservation {
        id: format!("transfer conserves '{}'", moving.path),
        quantity: Quantity::new(
            &moving.path,
            &[
                &format!("{FROM_KEY}.{}", moving.path),
                &format!("{TO_KEY}.{}", moving.path),
            ],
        ),
        tolerance: moving.tolerance,
    };
    let pair_before = json!({ FROM_KEY: from.state, TO_KEY: to.state });
    let pair_after = json!({ FROM_KEY: from_state.snapshot(), TO_KEY: to_state.snapshot() });
    if let Err(v) = law.check(&pair_before, &pair_after) {
        return refuse("conservation", v.detail);
    }

    // ── 7 · BOTH sides' own gates ───────────────────────────────────────────
    //
    // ★★ The same `predicate::check` the operator gate uses, and an unparseable
    //   rule refuses rather than being skipped — a gate that fails open is the
    //   one failure mode a viable region must not have.
    let params = Map::new();
    for (party, candidate) in [
        (from, from_state.snapshot()),
        (to, to_state.snapshot()),
    ] {
        if !party.enforcement.enabled {
            continue;
        }
        for (id, expr) in &party.enforcement.invariants {
            match predicate::check(expr, &candidate, &params) {
                Err(e) => {
                    return refuse(
                        "invariant_unparseable",
                        format!(
                            "'{}': invariant '{id}' ({expr}) could not be parsed: {e}",
                            party.sustain_id
                        ),
                    )
                }
                Ok((false, reason)) => {
                    return refuse(
                        "enforcement_gate",
                        format!(
                            "'{}': would violate invariant '{id}' ({expr}): {reason}",
                            party.sustain_id
                        ),
                    )
                }
                Ok((true, _)) => {}
            }
        }
    }

    // ── 8 · each side's own D(s, s') ────────────────────────────────────────
    //
    // ★ A household may declare its own conservation, rate limits or
    //   monotonicity, and a transfer is a transition like any other. Skipping
    //   them here would make a transfer the one way past rules a Sustain
    //   declared about itself.
    for (party, candidate) in [
        (from, from_state.snapshot()),
        (to, to_state.snapshot()),
    ] {
        if !party.enforcement.enabled {
            continue;
        }
        if let Err(v) = transition::check_all(&party.enforcement.transitions, &party.state, &candidate)
        {
            return refuse(
                "transition_constraint",
                format!("'{}': {}", party.sustain_id, v.detail),
            );
        }
    }

    // ── 9 · settled — both legs, together ───────────────────────────────────
    Transfer::Committed(Box::new(TransferSettlement {
        from: Leg {
            sustain_id: from.sustain_id.clone(),
            mutations: from_state.mutations().to_vec(),
            event: leg_event("debit", &to.sustain_id, amount, from_after, &moving.path),
            state_after: from_state.snapshot(),
            balance_after: from_after,
        },
        to: Leg {
            sustain_id: to.sustain_id.clone(),
            mutations: to_state.mutations().to_vec(),
            event: leg_event("credit", &from.sustain_id, amount, to_after, &moving.path),
            state_after: to_state.snapshot(),
            balance_after: to_after,
        },
        path: moving.path.clone(),
        amount,
        total_before,
        total_after,
    }))
}

/// Both legs carry the SAME event name and a `direction` — one fact, recorded
/// twice from two points of view, rather than two facts that could disagree.
fn leg_event(
    direction: &str,
    counterparty: &str,
    amount: f64,
    balance_after: f64,
    path: &str,
) -> EmittedEvent {
    EmittedEvent {
        name: "event.holon.transfer_committed".to_string(),
        payload: json!({
            "direction": direction,
            "counterparty": counterparty,
            "amount": amount,
            "path": path,
            "balance_after": balance_after,
        }),
    }
}

fn numeric_at(state: &State, path: &str) -> Option<f64> {
    match state.get(path) {
        Some(v) if !v.is_boolean() => v.as_f64(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "finances.liquid.balance";

    fn links() -> Vec<Link> {
        vec![Link::new("bonnie", "homestead"), Link::new("cira", "homestead")]
    }

    fn state(n: f64) -> Value {
        json!({ "finances": { "liquid": { "balance": n } } })
    }

    fn open(enabled: bool, invariants: &[(&str, &str)]) -> Enforcement {
        Enforcement {
            enabled,
            invariants: invariants
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn party(id: &str, n: f64) -> Party {
        Party::new(id, state(n), open(false, &[]))
    }

    fn linked(a: &str, b: &str) -> Linked {
        Linked::between(a, b, &links()).expect("declared")
    }

    /// The property, as an equation over the two real states.
    fn total(t: &Transfer) -> (f64, f64) {
        let s = t.settlement().expect("committed");
        let (f, c) = s.legs();
        (
            s.total_before(),
            f.state_after()[ "finances" ]["liquid"]["balance"].as_f64().unwrap()
                + c.state_after()["finances"]["liquid"]["balance"].as_f64().unwrap(),
        )
    }

    #[test]
    fn a_transfer_conserves_the_total() {
        let t = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 1000.0),
            &party("homestead", 250.0),
            &Moving::money(PATH),
            400.0,
        );
        let (before, after) = total(&t);
        assert_eq!(before, 1250.0);
        assert_eq!(after, before, "a transfer is a move, never a mint or a burn");
        let s = t.settlement().unwrap();
        let (f, c) = s.legs();
        assert_eq!(f.balance_after(), 600.0);
        assert_eq!(c.balance_after(), 650.0);
        assert!(s.conserved());
    }

    #[test]
    fn both_legs_or_nothing_a_refusal_carries_neither() {
        let t = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 10.0),
            &party("homestead", 0.0),
            &Moving::money(PATH),
            999.0,
        );
        assert!(matches!(&t, Transfer::Refused { rule, .. } if rule == "from_balance_sufficient"));
        // ★ There is no leg to inspect, which is the point.
        assert!(t.settlement().is_none());
    }

    #[test]
    fn the_receivers_own_gate_refuses_and_the_sender_is_untouched() {
        // A ceiling on the receiver. The debit would be perfectly legal on its
        // own — this is the exact case a two-call workaround loses money on.
        let receiver = Party::new(
            "homestead",
            state(900.0),
            open(true, &[("ceiling", "finances.liquid.balance <= 1000")]),
        );
        let sender = party("bonnie", 500.0);
        let t = transfer(
            &linked("bonnie", "homestead"),
            &sender,
            &receiver,
            &Moving::money(PATH),
            300.0,
        );
        match &t {
            Transfer::Refused { rule, reason } => {
                assert_eq!(rule, "enforcement_gate");
                assert!(reason.contains("homestead"), "the refusal names the side: {reason}");
            }
            Transfer::Committed(_) => panic!("the receiver's ceiling must refuse this"),
        }
        // Both sides byte-identical to before.
        assert_eq!(sender.state, state(500.0));
        assert_eq!(receiver.state, state(900.0));
    }

    #[test]
    fn the_senders_own_gate_refuses_too() {
        let sender = Party::new(
            "bonnie",
            state(500.0),
            open(true, &[("floor", "finances.liquid.balance >= 400")]),
        );
        let t = transfer(
            &linked("bonnie", "homestead"),
            &sender,
            &party("homestead", 0.0),
            &Moving::money(PATH),
            300.0,
        );
        assert!(matches!(&t, Transfer::Refused { rule, .. } if rule == "enforcement_gate"));
    }

    #[test]
    fn a_declared_transition_rule_is_honoured() {
        let sender = Party::new(
            "bonnie",
            state(500.0),
            Enforcement {
                enabled: true,
                transitions: vec![TransitionRule::RateLimit {
                    id: "slow".into(),
                    path: PATH.into(),
                    delta: 100.0,
                }],
                ..Default::default()
            },
        );
        let t = transfer(
            &linked("bonnie", "homestead"),
            &sender,
            &party("homestead", 0.0),
            &Moving::money(PATH),
            300.0,
        );
        assert!(matches!(&t, Transfer::Refused { rule, .. } if rule == "transition_constraint"));
    }

    #[test]
    fn a_self_transfer_has_no_token_at_all() {
        assert!(Linked::between("bonnie", "bonnie", &links()).is_none());
    }

    #[test]
    fn unlinked_sustains_have_no_token_at_all() {
        // Both are children of the homestead, and neither is the other's
        // parent — a sibling transfer is not a direct link.
        assert!(Linked::between("bonnie", "cira", &links()).is_none());
    }

    #[test]
    fn a_token_for_a_different_edge_does_not_authorise_this_pair() {
        let t = transfer(
            &linked("cira", "homestead"),
            &party("bonnie", 100.0),
            &party("homestead", 0.0),
            &Moving::money(PATH),
            10.0,
        );
        assert!(matches!(&t, Transfer::Refused { rule, .. } if rule == "holon_link_exists"));
    }

    #[test]
    fn zero_and_negative_and_nan_amounts_are_refused() {
        for amount in [0.0, -5.0, f64::NAN, f64::INFINITY] {
            let t = transfer(
                &linked("bonnie", "homestead"),
                &party("bonnie", 100.0),
                &party("homestead", 0.0),
                &Moving::money(PATH),
                amount,
            );
            assert!(
                matches!(&t, Transfer::Refused { rule, .. } if rule == "amount_positive"),
                "{amount} must be refused"
            );
        }
    }

    #[test]
    fn a_missing_path_on_either_side_is_refused_not_treated_as_zero() {
        let t = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 100.0),
            &Party::new("homestead", json!({ "soil": 40 }), open(false, &[])),
            &Moving::money(PATH),
            10.0,
        );
        assert!(matches!(&t, Transfer::Refused { rule, .. } if rule == "path_present"));
    }

    #[test]
    fn a_fractional_amount_is_refused_under_exact_conservation() {
        // ★ Stricter than the reference, deliberately: a law that holds to six
        //   decimal places is not a law.
        let t = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 100.0),
            &party("homestead", 0.0),
            &Moving::money(PATH),
            0.5,
        );
        assert!(matches!(&t, Transfer::Refused { rule, .. } if rule == "conservation"));
    }

    #[test]
    fn a_continuous_quantity_moves_under_a_named_tolerance() {
        let t = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 100.0),
            &party("homestead", 0.0),
            &Moving::real(PATH, 1e-9),
            0.5,
        );
        assert!(t.committed(), "a declared tolerance is what makes this legal");
        let (before, after) = total(&t);
        assert!((after - before).abs() < 1e-9);
    }

    #[test]
    fn both_legs_carry_the_same_event_from_two_points_of_view() {
        let t = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 100.0),
            &party("homestead", 0.0),
            &Moving::money(PATH),
            40.0,
        );
        let (f, c) = t.settlement().unwrap().legs();
        assert_eq!(f.event().name, c.event().name);
        assert_eq!(f.event().payload["direction"], json!("debit"));
        assert_eq!(c.event().payload["direction"], json!("credit"));
        assert_eq!(f.event().payload["counterparty"], json!("homestead"));
        assert_eq!(c.event().payload["counterparty"], json!("bonnie"));
        assert_eq!(f.event().payload["amount"], c.event().payload["amount"]);
    }

    #[test]
    fn both_legs_carry_real_mutations_for_the_fold() {
        let t = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 100.0),
            &party("homestead", 0.0),
            &Moving::money(PATH),
            40.0,
        );
        let (f, c) = t.settlement().unwrap().legs();
        assert_eq!(f.mutations().len(), 1);
        assert_eq!(c.mutations().len(), 1);
    }

    #[test]
    fn the_direction_is_the_callers_it_works_child_to_parent_and_back() {
        let up = transfer(
            &linked("bonnie", "homestead"),
            &party("bonnie", 100.0),
            &party("homestead", 10.0),
            &Moving::money(PATH),
            40.0,
        );
        let down = transfer(
            &linked("bonnie", "homestead"),
            &party("homestead", 100.0),
            &party("bonnie", 10.0),
            &Moving::money(PATH),
            40.0,
        );
        assert!(up.committed() && down.committed());
    }

    #[test]
    fn a_transfer_is_reproducible() {
        let call = || {
            transfer(
                &linked("bonnie", "homestead"),
                &party("bonnie", 100.0),
                &party("homestead", 0.0),
                &Moving::money(PATH),
                40.0,
            )
        };
        assert_eq!(call(), call(), "no clock, no id, no randomness");
    }
}
