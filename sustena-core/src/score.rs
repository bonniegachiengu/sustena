//! **The score** (Operative · §X).
//!
//! ```text
//!   score_i = Δu_i + β·1[∀k: s̃_k ∈ Viab_T(V)] + γ·ΔReach_H − λ·Pawa(π, s)
//! ```
//!
//! Four terms, and the interesting content is in what each one refuses to be.
//!
//! ## The efficiency term, and why it stays visible
//!
//! ★★★ **"Among strategies that achieve the same outcome, the one with lower
//! pawa wins"** (Pawa §3). Efficiency stops being a vibe and becomes a number
//! the simulator returns *before* anything real happens — and because a leaner
//! strategy consumes less of the network real compute and storage, the same
//! number that rewards elegance also spares the network.
//!
//! ★★★ **The pawa term is kept in the vector, never collapsed into the total
//! alone** — the article names this explicitly: *"keep the pawa term visible in
//! the vector, don't let a cheap-but-harmful plan hide inside an aggregate."* A
//! plan that is cheap and ruinous still loses the whole of `β`, and a reader can
//! see which term carried it.
//!
//! ## Viability is a property of the TRAJECTORY, not of the endpoint
//!
//! ★★★ **`∀k` — every intermediate state, not the last one.** A plan that empties
//! the rent pocket in week two and refills it in week four ends exactly where a
//! plan that never touched it ends, and they are not the same plan: one of them
//! has a fortnight in it where the household cannot pay rent. Scoring the
//! endpoint rates them identically, which is not a rounding error — it is the
//! difference between a plan and a plan that ruins somebody in the middle.
//!
//! ★★★ **An indicator, not a graded penalty.** `1[…]` is 0 or 1: either the whole
//! path stayed viable or the term is gone. A depth-proportional penalty would be
//! smoother and would let a large enough `Δu` **buy** a trip through ruin, which
//! is exactly the trade this term exists to forbid.
//!
//! ## `γ = 0` refuses every beaver dam, and that is a choice with a name
//!
//! ★★★ A dam has no immediate utility. Its whole value is the room it leaves,
//! so at `γ = 0` every act of positioning scores zero on the only axis that can
//! see it and loses to anything with a present payoff. That may be the right
//! setting for a household in a crisis; it is never the right *default*, because
//! it is a policy that looks like an absent parameter.
//! [`Weights::refuses_positioning`] makes it sayable.
//!
//! ## One β means "unproven" and "ruinous" score alike
//!
//! ★★★ A limitation of the declared formula, not of this implementation, and it
//! is left visible: with one weight and an indicator there is no number between
//! *we watched it fail* and *we could not tell*. Adding a second weight would be
//! adding a parameter the canon does not declare, so the distinction is carried
//! in the reason — [`ViabilityTerm::is_known_failure`] — where a caller can
//! break a tie on evidence without anybody's score moving.
//!
//! ## The score is a floor
//!
//! ★★ `ΔReach_H` under-states positioning ([`crate::reach`]), so a score
//! carrying a bounded reach is a lower bound on the real score. Carried through
//! rather than dropped at the boundary — a caveat that stops at a function
//! signature is a caveat nobody reads.

use crate::reach::{Bias, Positioning};
use crate::strong_admit::StrongVerdict;

/// `β` and `γ` — declared, never defaulted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// What a wholly-viable trajectory is worth.
    pub beta: f64,
    /// What room to move in is worth.
    pub gamma: f64,
    /// `λ` — what a unit of pawa costs the ranking.
    ///
    /// ★★ Declared like the others. At zero, two plans reaching the same place
    /// for wildly different amounts of the network compute rank identically,
    /// which is a policy and not an absent parameter.
    pub lambda: f64,
}

impl Weights {
    pub fn new(beta: f64, gamma: f64) -> Self {
        Self { beta, gamma, lambda: 0.0 }
    }

    /// Declare what a unit of pawa costs the ranking.
    pub fn charging(mut self, lambda: f64) -> Self {
        self.lambda = lambda;
        self
    }

    /// ★★★ Does this weighting make positioning invisible?
    ///
    /// Named so the setting can be reported rather than inferred from a zero
    /// somebody may not have chosen on purpose.
    pub fn refuses_positioning(&self) -> bool {
        self.gamma == 0.0
    }

    /// Does this weighting make trajectory viability worth nothing?
    pub fn ignores_viability(&self) -> bool {
        self.beta == 0.0
    }

    /// ★★★ Does this weighting make efficiency invisible?
    ///
    /// Named for the same reason `refuses_positioning` is: at `λ = 0` a plan
    /// that burns ten times the compute to reach the same place ranks
    /// identically, and that is a decision somebody should be able to see they
    /// made.
    pub fn ignores_efficiency(&self) -> bool {
        self.lambda == 0.0
    }
}

/// Why the viability bonus was or was not paid.
#[derive(Debug, Clone, PartialEq)]
pub enum ViabilityTerm {
    /// Every state along the way was in the kernel. The bonus is earned.
    HeldThroughout,
    /// ★★★ Some intermediate state was not, **and it says which**.
    ///
    /// The step matters: "step 2 of 5 left the viable region" is a plan somebody
    /// can repair, and "not viable" is one they can only discard.
    BrokeAt { step: usize, reason: String },
    /// ★★★ Nothing could be shown either way, so **the bonus is withheld**.
    ///
    /// Paying it would pay for an unverified claim, and refusing the candidate
    /// outright would double-count a decision [`crate::strong_admit`] has
    /// already made.
    ///
    /// ★★★ **And a limitation, stated rather than papered over:** the declared
    /// formula has ONE β and an indicator, so *withheld* and *failed* come to
    /// the same number. An unproven candidate scores exactly as low as a known
    /// ruinous one. Inventing a second weight to separate them would be adding
    /// a parameter the canon does not declare, so the distinction lives in the
    /// reason instead — see [`ViabilityTerm::is_known_failure`], which lets a
    /// caller break a tie without changing anybody's score.
    Unproven { why: String },
}

impl ViabilityTerm {
    /// ★★ Did we SEE it fail, or fail to see?
    ///
    /// Both score zero. Only one of them is evidence, and a tie between two
    /// candidates is the moment that difference is worth something.
    pub fn is_known_failure(&self) -> bool {
        matches!(self, Self::BrokeAt { .. })
    }

    fn indicator(&self) -> f64 {
        matches!(self, Self::HeldThroughout) as u8 as f64
    }

    pub fn describe(&self) -> String {
        match self {
            Self::HeldThroughout => "viable the whole way".into(),
            Self::BrokeAt { step, reason } => format!("leaves the viable region at step {step}: {reason}"),
            Self::Unproven { why } => format!("viability could not be shown: {why}"),
        }
    }
}

/// **Read the trajectory term from the verdict on every intermediate state.**
///
/// ★★★ Takes the whole path. There is no overload that takes an endpoint,
/// because the endpoint is the thing this term exists not to be scored on.
///
/// ★★ An empty path holds vacuously — a candidate that changes nothing passes
/// through nothing — and that is not the same as an unproven one.
pub fn viability_of(path: &[StrongVerdict]) -> ViabilityTerm {
    for (k, v) in path.iter().enumerate() {
        match v {
            StrongVerdict::Survivable => {}
            StrongVerdict::ProvisionallySurvivable { horizon } => {
                // ★★★ A horizon kernel is a SUPERSET of the true one, so this
                //     state may still be doomed. It admits (strong_admit said
                //     so) and it does not earn a bonus for a guarantee nobody
                //     has: paying on a superset is paying for the optimism.
                return ViabilityTerm::Unproven {
                    why: format!(
                        "step {k} is inside a {horizon}-step kernel, which is a superset of the \
                         true one"
                    ),
                };
            }
            StrongVerdict::Doomed { reason } | StrongVerdict::Unknown { why: reason } => {
                if matches!(v, StrongVerdict::Unknown { .. }) {
                    return ViabilityTerm::Unproven { why: format!("step {k}: {reason}") };
                }
                return ViabilityTerm::BrokeAt { step: k, reason: reason.clone() };
            }
            StrongVerdict::Outside => {
                return ViabilityTerm::BrokeAt {
                    step: k,
                    reason: "outside the viable region".into(),
                };
            }
        }
    }
    ViabilityTerm::HeldThroughout
}

/// One candidate's score, with every term still visible.
///
/// ★★★ The parts are kept rather than summed away. A ranked list of bare
/// numbers cannot answer *why is this one above that one*, and that is the
/// question a person actually asks of a recommendation.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub total: f64,
    pub utility: f64,
    pub viability: ViabilityTerm,
    pub positioning: Option<Positioning>,
    /// `Pawa(π, s)` — what the whole branch cost, accumulated in the sandbox.
    ///
    /// ★★★ Kept beside the total rather than folded away, because the article
    /// asks for exactly that: a cheap-but-harmful plan must not be able to hide
    /// inside an aggregate. A reader can see what the efficiency term
    /// contributed and what it did not rescue.
    pub pawa: f64,
    pub weights: Weights,
    /// ★★ True when a bounded reach means the total is a lower bound.
    pub is_floor: bool,
}

impl Score {
    pub fn describe(&self) -> String {
        let mut parts = vec![format!("{:+.2} from the change itself", self.utility)];
        if !self.weights.ignores_viability() {
            parts.push(self.viability.describe());
        }
        match (&self.positioning, self.weights.refuses_positioning()) {
            (_, true) => parts.push("room to move is not being counted (γ=0)".into()),
            (Some(p), false) => parts.push(p.describe()),
            (None, false) => parts.push("room to move was not measured".into()),
        }
        if self.weights.ignores_efficiency() {
            if self.pawa > 0.0 {
                parts.push(format!("cost {:.1} pawa, which is not being counted (λ=0)", self.pawa));
            }
        } else {
            parts.push(format!("cost {:.1} pawa", self.pawa));
        }
        if self.is_floor {
            parts.push("the total is a floor".into());
        }
        format!("{:+.2} — {}", self.total, parts.join("; "))
    }
}

/// **`score = Δu + β·1[∀k viable] + γ·ΔReach_H`**
pub fn score(
    delta_u: f64,
    path: &[StrongVerdict],
    positioning: Option<Positioning>,
    weights: Weights,
) -> Score {
    scored_at(delta_u, path, positioning, 0.0, weights)
}

/// **`score = Δu + β·1[∀k viable] + γ·ΔReach_H − λ·Pawa(π, s)`**
///
/// ★★★ `pawa` is the branch total, **accumulated in the sandbox with live state
/// untouched** — the whole point of ranking efficiency is that it happens before
/// anything real has been spent.
pub fn scored_at(
    delta_u: f64,
    path: &[StrongVerdict],
    positioning: Option<Positioning>,
    pawa: f64,
    weights: Weights,
) -> Score {
    let viability = viability_of(path);
    let reach_term = positioning.as_ref().map(|p| p.delta).unwrap_or(0.0);
    let total = delta_u + weights.beta * viability.indicator() + weights.gamma * reach_term
        - weights.lambda * pawa;
    // The floor only matters if the positioning term is actually being counted.
    let is_floor = !weights.refuses_positioning()
        && positioning
            .as_ref()
            .is_some_and(|p| p.bias == Bias::UnderStatesPositioning);
    Score { total, utility: delta_u, viability, positioning, pawa, weights, is_floor }
}

/// Rank candidates, best first, with ties broken by name so the order is stable.
///
/// ★★ A ranking that reorders equal candidates between runs is one people stop
/// trusting, and equal scores are common once a term is an indicator.
pub fn rank(scored: &[(String, Score)]) -> Vec<&(String, Score)> {
    let mut out: Vec<&(String, Score)> = scored.iter().collect();
    out.sort_by(|a, b| {
        b.1.total.partial_cmp(&a.1.total).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reach::StoppedBy;

    fn ok_path(n: usize) -> Vec<StrongVerdict> {
        vec![StrongVerdict::Survivable; n]
    }

    fn dips_at(k: usize, n: usize) -> Vec<StrongVerdict> {
        let mut p = ok_path(n);
        p[k] = StrongVerdict::Doomed { reason: "the rent pocket is empty".into() };
        p
    }

    fn positioning(delta: f64, bias: Bias) -> Positioning {
        Positioning {
            delta,
            before: 0.0,
            after: delta,
            steps_taken: 3,
            stopped_by: StoppedBy::Budget,
            bias,
        }
    }

    fn w() -> Weights {
        Weights::new(10.0, 1.0)
    }

    #[test]
    fn a_plan_that_ruins_somebody_in_the_middle_does_not_score_as_one_that_does_not() {
        // ★★★ The whole point of ∀k. Both plans end in the same place; one of
        //     them has a fortnight in it where the rent cannot be paid.
        let clean = score(5.0, &ok_path(5), None, w());
        let dips = score(5.0, &dips_at(2, 5), None, w());
        assert!(clean.total > dips.total);
        assert!(dips.viability.describe().contains("at step 2"));
    }

    #[test]
    fn the_failing_step_is_named_so_the_plan_can_be_repaired() {
        // ★★ "Step 2 of 5 left the viable region" is a plan somebody can fix;
        //    "not viable" is one they can only discard.
        match viability_of(&dips_at(3, 6)) {
            ViabilityTerm::BrokeAt { step, reason } => {
                assert_eq!(step, 3);
                assert!(reason.contains("rent pocket"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn utility_cannot_buy_a_trip_through_ruin() {
        // ★★★ An indicator, not a graded penalty. A depth-proportional penalty
        //     would be smoother and would let a big enough Δu purchase exactly
        //     the trade this term forbids — so the bonus is all or nothing, and
        //     the ruinous plan pays the whole β no matter how good its numbers.
        let modest_and_clean = score(1.0, &ok_path(4), None, w());
        let brilliant_and_ruinous = score(10.0, &dips_at(1, 4), None, w());
        assert!(
            modest_and_clean.total > brilliant_and_ruinous.total,
            "ten times the utility still does not buy the trip"
        );
        // ★★ And the term costs the same however shallow the dip was — which
        //    is exactly the property a graded penalty would give away.
        let shallower = score(10.0, &dips_at(3, 4), None, w());
        assert_eq!(brilliant_and_ruinous.total, shallower.total);
    }

    #[test]
    fn gamma_zero_refuses_every_beaver_dam_and_says_so() {
        // ★★★ A dam has no immediate utility; its whole value is the room it
        //     leaves. At γ=0 it loses to anything with a present payoff, and
        //     that is a policy rather than an absent parameter.
        let none = Weights::new(10.0, 0.0);
        assert!(none.refuses_positioning());
        let dam = score(0.0, &ok_path(3), Some(positioning(9.0, Bias::None)), none);
        let trinket = score(0.5, &ok_path(3), Some(positioning(0.0, Bias::None)), none);
        assert!(trinket.total > dam.total, "the dam loses");
        assert!(dam.describe().contains("γ=0"));
    }

    #[test]
    fn with_gamma_the_dam_wins() {
        let dam = score(0.0, &ok_path(3), Some(positioning(9.0, Bias::None)), w());
        let trinket = score(0.5, &ok_path(3), Some(positioning(0.0, Bias::None)), w());
        assert!(dam.total > trinket.total);
    }

    #[test]
    fn an_unproven_viability_earns_no_bonus_and_takes_no_penalty() {
        // ★★★ Paying would pay for an unverified claim; refusing would
        //     double-count a decision strong_admit has already made.
        let unproven =
            vec![StrongVerdict::Survivable, StrongVerdict::Unknown { why: "too big".into() }];
        let s = score(5.0, &unproven, None, w());
        assert_eq!(s.total, 5.0, "no bonus");
        assert!(matches!(s.viability, ViabilityTerm::Unproven { .. }));
        assert!(!s.viability.is_known_failure(), "and it is not evidence of failure");
    }

    #[test]
    fn withheld_and_failed_score_the_same_and_the_reason_is_what_separates_them() {
        // ★★★ A real limitation of the declared formula, stated rather than
        //     papered over: one β and an indicator means "we could not show it"
        //     lands exactly where "we watched it fail" does. Inventing a second
        //     weight would add a parameter the canon does not declare, so the
        //     distinction lives in the reason — and a caller with a tie can use
        //     it without changing anybody's score.
        let unproven = score(
            5.0,
            &[StrongVerdict::Unknown { why: "too big to enumerate".into() }],
            None,
            w(),
        );
        let failed = score(5.0, &dips_at(0, 1), None, w());
        assert_eq!(unproven.total, failed.total, "the numbers cannot tell them apart");
        assert!(!unproven.viability.is_known_failure());
        assert!(failed.viability.is_known_failure(), "the reason can");
    }

    #[test]
    fn a_horizon_kernel_does_not_earn_the_guarantee_bonus() {
        // ★★★ Viab^H is a SUPERSET of the true kernel, so a state inside it may
        //     still be doomed. Paying the bonus on it is paying for the optimism.
        let provisional = vec![StrongVerdict::ProvisionallySurvivable { horizon: 4 }];
        match viability_of(&provisional) {
            ViabilityTerm::Unproven { why } => assert!(why.contains("superset")),
            other => panic!("must not be treated as a guarantee: {other:?}"),
        }
    }

    #[test]
    fn a_candidate_that_passes_through_nothing_holds_vacuously() {
        // ★★ Not the same as unproven: there was nothing to fail.
        assert_eq!(viability_of(&[]), ViabilityTerm::HeldThroughout);
    }

    #[test]
    fn a_bounded_reach_makes_the_whole_score_a_floor() {
        // ★★ A caveat that stops at a function signature is a caveat nobody
        //    reads.
        let s = score(
            1.0,
            &ok_path(2),
            Some(positioning(3.0, Bias::UnderStatesPositioning)),
            w(),
        );
        assert!(s.is_floor);
        assert!(s.describe().contains("floor"));
    }

    #[test]
    fn a_floor_on_a_term_nobody_is_counting_is_not_a_floor() {
        // ★★ At γ=0 the truncated term contributes nothing, so the total is not
        //    understated by it — claiming otherwise would be a caveat about
        //    arithmetic that did not happen.
        let s = score(
            1.0,
            &ok_path(2),
            Some(positioning(3.0, Bias::UnderStatesPositioning)),
            Weights::new(10.0, 0.0),
        );
        assert!(!s.is_floor);
    }

    #[test]
    fn the_terms_survive_the_sum() {
        // ★★★ A ranked list of bare numbers cannot answer "why is this one
        //     above that one", which is the question a person asks of a
        //     recommendation.
        let s = score(2.0, &ok_path(3), Some(positioning(4.0, Bias::None)), w());
        assert_eq!(s.utility, 2.0);
        assert_eq!(s.total, 2.0 + 10.0 + 4.0);
        assert_eq!(s.positioning.as_ref().unwrap().delta, 4.0);
        assert!(s.describe().contains("from the change itself"));
    }

    // ── the efficiency term (PAWA-4 · TEN-11 · Pawa §3) ────────────────────

    fn efficient() -> Weights {
        Weights::new(10.0, 1.0).charging(0.5)
    }

    #[test]
    fn among_strategies_that_reach_the_same_place_the_cheaper_one_wins() {
        // ★★★ The article's own sentence, made into a test. Identical utility,
        //     identical viability, identical positioning — and one of them
        //     burns four times the compute to get there.
        let lean = scored_at(5.0, &ok_path(3), None, 2.0, efficient());
        let wasteful = scored_at(5.0, &ok_path(3), None, 8.0, efficient());
        assert!(lean.total > wasteful.total);
        assert!(lean.describe().contains("cost 2.0 pawa"));
    }

    #[test]
    fn a_cheap_and_ruinous_plan_cannot_hide_inside_the_aggregate() {
        // ★★★ The anti-scalar-collapse guard the article names explicitly. A
        //     plan that costs nothing and passes through ruin still loses the
        //     whole of β, and the terms are all still there to read.
        let free_and_ruinous = scored_at(5.0, &dips_at(1, 3), None, 0.0, efficient());
        let costly_and_sound = scored_at(5.0, &ok_path(3), None, 6.0, efficient());
        assert!(costly_and_sound.total > free_and_ruinous.total);
        assert!(free_and_ruinous.viability.is_known_failure());
        assert_eq!(free_and_ruinous.pawa, 0.0, "and the cost is still readable");
    }

    #[test]
    fn lambda_zero_makes_efficiency_invisible_and_says_so() {
        // ★★ Same shape as γ=0 refusing the beaver dam: a policy, not an absent
        //    parameter. Two plans reaching the same place for wildly different
        //    amounts of the network's compute rank identically.
        let free = Weights::new(10.0, 1.0);
        assert!(free.ignores_efficiency());
        let cheap = scored_at(5.0, &ok_path(2), None, 1.0, free);
        let dear = scored_at(5.0, &ok_path(2), None, 900.0, free);
        assert_eq!(cheap.total, dear.total);
        assert!(dear.describe().contains("not being counted (λ=0)"));
    }

    #[test]
    fn the_cost_survives_the_sum_even_when_nobody_is_charging_for_it() {
        // ★★ A branch that was measured and not charged is different from one
        //    nobody measured, and the reader can tell.
        let s = scored_at(1.0, &ok_path(2), None, 7.0, Weights::new(1.0, 0.0));
        assert_eq!(s.pawa, 7.0);
        assert_eq!(s.total, 2.0, "λ=0, so the cost changed nothing");
    }

    #[test]
    fn efficiency_can_be_outweighed_but_never_ignored() {
        // ★★ A far better outcome should still win against a cheaper worse one
        //    — the term is a weight, not a veto. Elegance is rewarded, not
        //    mandated.
        let great_and_dear = scored_at(20.0, &ok_path(2), None, 10.0, efficient());
        let poor_and_cheap = scored_at(1.0, &ok_path(2), None, 0.0, efficient());
        assert!(great_and_dear.total > poor_and_cheap.total);
    }

    #[test]
    fn the_plain_score_helper_charges_nothing_and_is_honest_about_it() {
        // ★★ `score()` is `scored_at(..., 0.0, ...)`: a caller that has not
        //    measured a branch reports zero cost, which is true of what it
        //    knows, and `λ` then multiplies nothing.
        let s = score(3.0, &ok_path(2), None, efficient());
        assert_eq!(s.pawa, 0.0);
        assert_eq!(s.total, 13.0);
    }

    #[test]
    fn equal_candidates_rank_in_the_same_order_every_time() {
        // ★★ Equal scores are common once a term is an indicator, and a list
        //    that reorders itself between runs is one people stop trusting.
        let a = ("allocate".to_string(), score(1.0, &ok_path(2), None, w()));
        let b = ("borrow".to_string(), score(1.0, &ok_path(2), None, w()));
        let c = ("spend".to_string(), score(9.0, &ok_path(2), None, w()));
        let shuffled = [b.clone(), c.clone(), a.clone()];
        let ordered = [a, b, c];
        let names: Vec<&str> = rank(&shuffled).iter().map(|(n, _)| n.as_str()).collect();
        let again: Vec<&str> = rank(&ordered).iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["spend", "allocate", "borrow"]);
        assert_eq!(names, again, "the input order does not move the output order");
    }
}
