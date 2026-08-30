//! **An LLM is a proposal distribution** (Operative · §XII).
//!
//! ```text
//!   π ~ g(·|ctx)                 the model proposes
//!   accept ⟺ verify_sandbox(π)   the gate disposes
//! ```
//!
//! ★★★ **Correctness does not depend on `g`. Only efficiency does.** This is the
//! Metropolis–Hastings posture, and AlphaGo's: a bad proposal distribution wastes
//! compute and cannot produce a wrong result, because nothing reaches state
//! except through a verifier that never asked where the candidate came from. A
//! *good* `g` means fewer rejections. That is the whole of what it buys.
//!
//! ★★★ **The failure this row names is a shape, not a bug.** A `call_claude()`
//! that returns something the code then acts on is **a method call, not an
//! element of `T`** — there is no candidate for a gate to evaluate, because by
//! the time anything is returned the decision has already been made somewhere the
//! gate cannot see. So a [`Proposal`] here is *data naming a move*, and there is
//! no method on it that commits anything.
//!
//! ## "The model was confident" is not an argument
//!
//! ★★★ A proposal may carry whatever self-assessment its generator likes, and
//! [`verify`] **cannot read it** — the confidence is not a parameter. Two
//! proposals with identical content and opposite confidences get identical
//! verdicts, and there is a test that says so. A generator's opinion of itself is
//! evidence about the generator, never about the move.
//!
//! ★★ **Rejection is ordinary.** A rejected proposal costs the compute that
//! produced it and nothing else; the chain stays where it was. So a rejection
//! count is an **efficiency** reading and is never a correctness one, and
//! [`Run::acceptance_rate`] is documented as measuring `g`, not the system.

use std::collections::BTreeMap;

use serde_json::Value;

/// What a generator produced: **a named move with arguments**, not an action.
///
/// ★★★ An element of `T`. The generator says *do this*; whether it happens is
/// somebody else's decision, and there is no method here that makes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub operator: String,
    pub params: BTreeMap<String, Value>,
    /// Whatever the generator wants to claim about itself.
    ///
    /// ★★★ Carried so it can be **reported**, and deliberately never passed to
    /// [`verify`]. Keeping it out of the type entirely would lose a real signal
    /// about the generator; letting the verifier see it would make the model's
    /// self-regard part of the safety argument.
    pub self_reported_confidence: Option<f64>,
    /// Which generator produced it — for measuring `g`, never for admitting `π`.
    pub from: String,
}

impl Proposal {
    pub fn new(operator: &str, from: &str) -> Self {
        Self {
            operator: operator.into(),
            params: BTreeMap::new(),
            self_reported_confidence: None,
            from: from.into(),
        }
    }

    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.params.insert(key.to_string(), value);
        self
    }

    pub fn claiming(mut self, confidence: f64) -> Self {
        self.self_reported_confidence = Some(confidence);
        self
    }
}

/// What the sandbox said.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// It ran in the sandbox and the gate admitted the result.
    Accepted,
    /// ★★ Not an error. The chain stays where it was and the only cost is the
    /// compute that produced the candidate.
    Rejected { because: String },
}

impl Verdict {
    pub fn accepted(&self) -> bool {
        matches!(self, Self::Accepted)
    }
}

/// **`accept ⟺ verify_sandbox(π)`**
///
/// ★★★ The signature is the argument. `verify` takes the proposal's **content**
/// and a checker, and there is no parameter through which a confidence, a
/// generator name or a reputation could reach the decision. Making that
/// structural rather than a rule means a later change would have to widen the
/// signature in a review.
pub fn verify<F>(proposal: &Proposal, check: F) -> Verdict
where
    F: Fn(&str, &BTreeMap<String, Value>) -> Result<(), String>,
{
    match check(&proposal.operator, &proposal.params) {
        Ok(()) => Verdict::Accepted,
        Err(because) => Verdict::Rejected { because },
    }
}

/// One round of proposing and checking.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub accepted: Vec<Proposal>,
    pub rejected: Vec<(Proposal, String)>,
}

impl Run {
    /// **How well the generator is doing — not how safe the system is.**
    ///
    /// ★★★ Documented as a reading of `g`. A council that started treating a low
    /// acceptance rate as a safety problem would be one step from lowering the
    /// bar to raise the number.
    pub fn acceptance_rate(&self) -> Option<f64> {
        let total = self.accepted.len() + self.rejected.len();
        (total > 0).then(|| self.accepted.len() as f64 / total as f64)
    }

    /// What it cost to get here, in proposals.
    pub fn attempts(&self) -> usize {
        self.accepted.len() + self.rejected.len()
    }

    pub fn describe(&self) -> String {
        match self.acceptance_rate() {
            None => "nothing was proposed".into(),
            Some(r) => format!(
                "{} of {} proposals were admitted ({:.0}%) — a reading of the generator, not of \
                 the gate",
                self.accepted.len(),
                self.attempts(),
                r * 100.0
            ),
        }
    }
}

/// **Run a batch of proposals past the verifier.**
///
/// ★★ Every proposal is checked. There is no early exit on a first acceptance,
/// because "the first one that passed" is a property of the order the generator
/// happened to emit them in.
pub fn sift<F>(proposals: Vec<Proposal>, check: F) -> Run
where
    F: Fn(&str, &BTreeMap<String, Value>) -> Result<(), String>,
{
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for p in proposals {
        match verify(&p, &check) {
            Verdict::Accepted => accepted.push(p),
            Verdict::Rejected { because } => rejected.push((p, because)),
        }
    }
    Run { accepted, rejected }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The one rule: you may only spend from a pocket that exists, and never
    /// more than a thousand.
    fn check(op: &str, params: &BTreeMap<String, Value>) -> Result<(), String> {
        if op != "budget.spend" {
            return Err(format!("'{op}' is not a move this Sustain allows"));
        }
        match params.get("pocket").and_then(Value::as_str) {
            Some("food") | Some("rent") => {}
            other => return Err(format!("no such pocket: {other:?}")),
        }
        match params.get("amount").and_then(Value::as_f64) {
            Some(a) if a > 0.0 && a <= 1000.0 => Ok(()),
            other => Err(format!("amount out of bounds: {other:?}")),
        }
    }

    fn good(from: &str) -> Proposal {
        Proposal::new("budget.spend", from).with("pocket", json!("food")).with("amount", json!(500.0))
    }

    fn bad(from: &str) -> Proposal {
        Proposal::new("budget.spend", from)
            .with("pocket", json!("a pocket nobody has"))
            .with("amount", json!(500.0))
    }

    #[test]
    fn a_confident_wrong_proposal_is_refused_exactly_as_a_diffident_one_is() {
        // ★★★ "The model was confident" is not an argument. The confidence is
        //     not a parameter of `verify`, so it could not have been read even
        //     by a careless caller.
        let sure = bad("a very sure model").claiming(0.99);
        let unsure = bad("a hesitant model").claiming(0.01);
        assert_eq!(verify(&sure, check), verify(&unsure, check));
        assert!(!verify(&sure, check).accepted());
    }

    #[test]
    fn a_diffident_correct_proposal_is_admitted() {
        // ★★ The other direction, and it matters: a verifier that quietly
        //    favoured confident proposals would also be penalising honest ones.
        assert!(verify(&good("a hesitant model").claiming(0.02), check).accepted());
    }

    #[test]
    fn correctness_does_not_depend_on_the_generator() {
        // ★★★ The row's whole claim, made into a test. A deliberately terrible
        //     generator and an ideal one produce accepted sets that are equally
        //     valid, because validity is decided somewhere neither of them can
        //     reach.
        let terrible = vec![bad("garbage"), bad("garbage"), good("garbage"), bad("garbage")];
        let ideal = vec![good("excellent"), good("excellent")];

        let a = sift(terrible, check);
        let b = sift(ideal, check);
        for p in a.accepted.iter().chain(b.accepted.iter()) {
            assert!(check(&p.operator, &p.params).is_ok(), "every admitted move is valid");
        }
        // What differs is only the waste.
        assert!(a.acceptance_rate().unwrap() < b.acceptance_rate().unwrap());
    }

    #[test]
    fn a_bad_generator_costs_compute_and_cannot_cost_correctness() {
        // ★★ Metropolis–Hastings: a rejected proposal leaves the chain where it
        //    was. The only price is the work that produced it.
        let run = sift(vec![bad("g"), bad("g"), bad("g")], check);
        assert!(run.accepted.is_empty());
        assert_eq!(run.attempts(), 3);
        assert_eq!(run.acceptance_rate(), Some(0.0));
    }

    #[test]
    fn the_acceptance_rate_reads_the_generator_and_says_so() {
        // ★★★ A council that started treating a low acceptance rate as a safety
        //     problem would be one step from lowering the bar to raise the
        //     number.
        let run = sift(vec![good("g"), bad("g")], check);
        assert!(run.describe().contains("a reading of the generator, not of \nthe gate")
            || run.describe().contains("a reading of the generator, not of the gate"));
    }

    #[test]
    fn a_proposal_is_a_move_and_not_an_action() {
        // ★★★ The shape the row names as the failure: a `call_claude()` whose
        //     return value gets acted on is a method call, not an element of T,
        //     and by the time it returns the decision is already made somewhere
        //     the gate cannot see. This type names a move and has no method
        //     that performs one.
        let p = good("g");
        assert_eq!(p.operator, "budget.spend");
        assert_eq!(p.params.get("amount"), Some(&json!(500.0)));
    }

    #[test]
    fn every_proposal_is_checked_rather_than_stopping_at_the_first_that_passes() {
        // ★★ "The first one that passed" is a property of the order the
        //    generator happened to emit them in, which is not a property of
        //    anything worth deciding on.
        let run = sift(vec![good("g"), good("g"), bad("g")], check);
        assert_eq!(run.accepted.len(), 2);
        assert_eq!(run.rejected.len(), 1);
    }

    #[test]
    fn a_rejection_says_why_and_the_reason_is_about_the_move() {
        // ★★ Not about the model. "Your model is unreliable" is unactionable;
        //    "no such pocket" is a thing somebody can fix.
        let run = sift(vec![bad("g")], check);
        assert!(run.rejected[0].1.contains("no such pocket"));
    }

    #[test]
    fn an_operator_the_sustain_does_not_allow_is_refused_however_it_arrived() {
        let smuggled = Proposal::new("egress.pay", "a creative model").claiming(1.0);
        assert!(!verify(&smuggled, check).accepted());
    }

    #[test]
    fn nothing_proposed_is_not_a_hundred_percent_acceptance() {
        // ★★ An empty run has no rate rather than a perfect one — a division
        //    that flatters an idle generator is a metric people learn to game.
        let idle = sift(vec![], check);
        assert_eq!(idle.acceptance_rate(), None);
        assert!(idle.describe().contains("nothing was proposed"));
    }
}
