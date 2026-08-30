//! **What a trust score has to be** (Arena · §V).
//!
//! ```text
//!   τ = (α₀ + s) / (α₀ + β₀ + s + f)      a posterior, not a mean
//!   s ← λs + w·1[good],  f ← λf + w·1[bad]   evidence ages
//!   rank = LCB₁₋δ(τ)                       uncertainty costs the seller
//! ```
//!
//! ★★★ **The lemons market is not a future risk here, it is the current
//! state.** `trust_score` is written `0.0` at publish, updated by no code path,
//! and every listing sorts by it — so `ORDER BY trust_score DESC` over an
//! identically-zero column is an arbitrary order **presented as a quality
//! ranking**. Akerlof's unravelling needs only the absence of a quality signal,
//! and a signal that is constant is absent.
//!
//! ★★★ **And a selection process with no criticism does not merely rank badly —
//! it inverts.** Deutsch's distinction, which [`crate::meme`] already carries:
//! a market with working evidence selects for artefacts that deliver value; a
//! market without it selects for whatever merely spreads. Trust is not
//! marketplace decoration; it is the error-correction that keeps selection
//! rational.
//!
//! ## The four properties, each forced by something above
//!
//! ★★★ **(a) Per version.** Trust earned on v1.0 must not transfer intact to
//! v1.1 — that is the shape of essentially every real package-ecosystem
//! compromise. So the evidence is keyed on `(name, version)` and there is no
//! method that reads a version's standing from its neighbour.
//!
//! ★★ **(b) A posterior, not a mean.** In `[0,1]` **by construction**, so the
//! Arena's bound is structural rather than clamped; it shrinks toward the prior
//! when evidence is thin, so two lucky ratings do not outrank two thousand; and
//! the prior is exactly where author history enters, **discounted** — inherited
//! but never equal.
//!
//! ★★ **(c) Decaying.** Reputation must be a *flow, not a stock*, or a large
//! accumulated balance of old goodwill finances present cheating. Evidence is
//! aged **before** new evidence is added, per the article's own pseudocode.
//!
//! ★★★ **(d) Ranked by a lower bound.** Uncertainty becomes a cost borne by the
//! seller rather than a free option: a new artefact must accumulate evidence to
//! rise, and cannot be pushed to the top by a handful of favourable reports.
//!
//! ★★ **(e) Sybil-bounded.** Only purchasers may rate, which ties the cost of a
//! fake rating to the cost of a fake purchase, and one rating per purchase.
//! **Stated limit:** stake weighting is transitive trust, and transitive trust
//! is vulnerable to collusive clusters; EigenTrust's own answer is a set of
//! pre-trusted peers, which reintroduces an authority. Adopted only with that
//! named.

use std::collections::{BTreeMap, BTreeSet};

/// Which exact thing is being trusted.
///
/// ★★★ Name **and** version. There is no constructor taking a name alone,
/// because a score attached to a name is a score attached to a moving target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Versioned {
    pub name: String,
    pub version: String,
}

impl Versioned {
    pub fn new(name: &str, version: &str) -> Self {
        Self { name: name.into(), version: version.into() }
    }
}

/// Aged counts of satisfactory and unsatisfactory outcomes.
///
/// ★★ Floating point because decay makes them continuous: after one halving a
/// single good outcome is worth half a good outcome, and rounding that to an
/// integer would quietly restore the stock this design is trying to avoid.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Evidence {
    pub good: f64,
    pub bad: f64,
}

impl Evidence {
    pub fn none() -> Self {
        Self::default()
    }

    /// How much evidence there is at all.
    pub fn weight(&self) -> f64 {
        self.good + self.bad
    }
}

/// `(α₀, β₀)` — what is believed before any evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prior {
    pub alpha0: f64,
    pub beta0: f64,
}

impl Prior {
    /// Uninformative: one imagined good outcome and one imagined bad one.
    ///
    /// ★★ Not `(0,0)`, which would make τ undefined with no evidence and would
    /// let the very first rating swing it to 0 or 1.
    pub fn uninformative() -> Self {
        Self { alpha0: 1.0, beta0: 1.0 }
    }

    /// **Where author history enters, and it enters discounted.**
    ///
    /// ★★★ *Inherited but not equal to the author's score.* `discount` is the
    /// fraction of the author's standing a new artefact starts with — at 1.0 a
    /// good author's next artefact would launch as though it had already been
    /// proven, which is exactly the malicious-update vector wearing a different
    /// hat.
    pub fn from_author(author_tau: f64, author_weight: f64, discount: f64) -> Self {
        let carried = (author_weight * discount).max(0.0);
        let a = author_tau.clamp(0.0, 1.0);
        Self { alpha0: 1.0 + carried * a, beta0: 1.0 + carried * (1.0 - a) }
    }
}

impl Default for Prior {
    fn default() -> Self {
        Self::uninformative()
    }
}

/// **`τ`** — the posterior mean.
///
/// ★★ In `[0,1]` by construction, so the Arena's bound needs no clamp: a bound
/// enforced by arithmetic cannot be forgotten by a caller.
pub fn tau(evidence: &Evidence, prior: &Prior) -> f64 {
    let a = prior.alpha0 + evidence.good;
    let b = prior.beta0 + evidence.bad;
    a / (a + b)
}

/// **Age the record.** `s ← λs`, `f ← λf`.
///
/// ★★★ Applied **before** new evidence is added, per §V.3's pseudocode. Ageing
/// afterwards would discount the rating that just arrived, which is the one
/// piece of evidence nobody should be discounting.
pub fn decay(evidence: &Evidence, lambda: f64) -> Evidence {
    let l = lambda.clamp(0.0, 1.0);
    Evidence { good: evidence.good * l, bad: evidence.bad * l }
}

/// How many observations back the record effectively remembers, `1/(1−λ)`.
///
/// ★★ Reportable, because "λ = 0.95" means nothing to a person and "it
/// remembers about the last twenty" means something.
pub fn effective_memory(lambda: f64) -> Option<f64> {
    (0.0..1.0).contains(&lambda).then(|| 1.0 / (1.0 - lambda))
}

/// **`rank = LCB₁₋δ(τ)`** — the lower end of a credible interval.
///
/// ★★★ Ranking on this rather than on `τ` is what makes uncertainty a cost the
/// seller bears. Two artefacts with the same τ rank differently when one has
/// ten ratings and the other ten thousand, and the thin one ranks lower — which
/// is the honest ordering, because it is the one we are less sure about.
///
/// ★★ **A Wilson-style normal approximation to the Beta interval, and named as
/// one.** The exact inverse Beta CDF is a heavier dependency than this earns;
/// Wilson is well-behaved at small `n`, which is precisely where a naive normal
/// approximation is not, and it errs conservative — in the direction this design
/// already wants.
pub fn lower_bound(evidence: &Evidence, prior: &Prior, z: f64) -> f64 {
    let n = prior.alpha0 + prior.beta0 + evidence.weight();
    if n <= 0.0 {
        return 0.0;
    }
    let p = tau(evidence, prior);
    let denom = 1.0 + z * z / n;
    let centre = p + z * z / (2.0 * n);
    let margin = z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).max(0.0).sqrt();
    ((centre - margin) / denom).clamp(0.0, 1.0)
}

/// `z` for a 95% one-sided bound. Declared, so the confidence is visible.
pub const Z_95: f64 = 1.645;

/// What a rating attempt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rated {
    /// Recorded.
    Recorded,
    /// ★★★ The rater never bought it. **Refused** — this is the narrowing that
    /// ties the cost of a fake rating to the cost of a fake purchase.
    NotAPurchaser,
    /// ★★ One rating per purchase. A second is refused rather than replacing
    /// the first, because replacement lets a rater test the market and settle
    /// on whichever score suits them.
    AlreadyRated,
}

impl Rated {
    pub fn describe(&self) -> String {
        match self {
            Self::Recorded => "recorded".into(),
            Self::NotAPurchaser => {
                "only purchasers may rate — a fake rating should cost what a fake purchase costs"
                    .into()
            }
            Self::AlreadyRated => "already rated once, and a rating is not revisable".into(),
        }
    }
}

/// The market's evidence, keyed per version.
#[derive(Debug, Clone, Default)]
pub struct Reputation {
    evidence: BTreeMap<Versioned, Evidence>,
    purchases: BTreeSet<(String, Versioned)>,
    rated: BTreeSet<(String, Versioned)>,
    lambda: f64,
}

impl Reputation {
    /// `lambda` is required, because a decay rate nobody chose is a memory
    /// nobody chose.
    pub fn with_decay(lambda: f64) -> Self {
        Self { lambda: lambda.clamp(0.0, 1.0), ..Default::default() }
    }

    pub fn record_purchase(&mut self, buyer: &str, of: &Versioned) {
        self.purchases.insert((buyer.to_string(), of.clone()));
    }

    fn purchased(&self, buyer: &str, of: &Versioned) -> bool {
        self.purchases.contains(&(buyer.to_string(), of.clone()))
    }

    /// **Rate an artefact.**
    ///
    /// ★★ `stake` is the rater's own earned standing — transitive trust, with
    /// the limit stated in the module header. A caller with no stake model
    /// passes `1.0` and gets flat weighting, which is honest rather than
    /// pretending to a weighting it does not have.
    pub fn rate(
        &mut self,
        rater: &str,
        of: &Versioned,
        satisfactory: bool,
        stake: f64,
    ) -> Rated {
        if !self.purchased(rater, of) {
            return Rated::NotAPurchaser;
        }
        if !self.rated.insert((rater.to_string(), of.clone())) {
            return Rated::AlreadyRated;
        }
        // ★★★ Age first, then add — §V.3's own order.
        let aged = decay(self.evidence.get(of).unwrap_or(&Evidence::none()), self.lambda);
        let w = stake.max(0.0);
        let next = if satisfactory {
            Evidence { good: aged.good + w, bad: aged.bad }
        } else {
            Evidence { good: aged.good, bad: aged.bad + w }
        };
        self.evidence.insert(of.clone(), next);
        Rated::Recorded
    }

    pub fn evidence_for(&self, of: &Versioned) -> Evidence {
        self.evidence.get(of).copied().unwrap_or_default()
    }

    /// What this version's standing is, or that nobody has rated it.
    ///
    /// ★★★ `None` for no evidence at all. An artefact nobody has rated is not
    /// trusted and not distrusted, and a market that showed it a number would be
    /// showing a number nobody produced — which is the defect this row exists
    /// to close, reintroduced.
    pub fn standing(&self, of: &Versioned, prior: &Prior) -> Option<f64> {
        let e = self.evidence.get(of)?;
        (e.weight() > 0.0).then(|| tau(e, prior))
    }

    /// **Rank a set of versions**, best first, by the lower bound.
    ///
    /// ★★ Unrated versions are returned separately rather than sorted in at
    /// zero. Sorting them last says *bad*; sorting them first says *good*; both
    /// are claims nobody made.
    pub fn ranked(
        &self,
        among: &[Versioned],
        prior: &Prior,
        z: f64,
    ) -> (Vec<(Versioned, f64)>, Vec<Versioned>) {
        let mut rated: Vec<(Versioned, f64)> = Vec::new();
        let mut unrated: Vec<Versioned> = Vec::new();
        for v in among {
            match self.evidence.get(v) {
                Some(e) if e.weight() > 0.0 => rated.push((v.clone(), lower_bound(e, prior, z))),
                _ => unrated.push(v.clone()),
            }
        }
        rated.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
        });
        unrated.sort();
        (rated, unrated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(version: &str) -> Versioned {
        Versioned::new("a widget", version)
    }

    fn market() -> Reputation {
        Reputation::with_decay(0.95)
    }

    fn buy_and_rate(r: &mut Reputation, who: &str, of: &Versioned, good: bool) {
        r.record_purchase(who, of);
        assert_eq!(r.rate(who, of, good, 1.0), Rated::Recorded);
    }

    #[test]
    fn trust_earned_on_one_version_does_not_transfer_to_the_next() {
        // ★★★ The malicious-update vector, and the shape of essentially every
        //     real package-ecosystem compromise: earn on v1.0, ship the payload
        //     in v1.1.
        let mut r = market();
        for i in 0..20 {
            buy_and_rate(&mut r, &format!("buyer{i}"), &v("1.0"), true);
        }
        assert!(r.standing(&v("1.0"), &Prior::uninformative()).unwrap() > 0.8);
        assert_eq!(r.standing(&v("1.1"), &Prior::uninformative()), None);
    }

    #[test]
    fn a_thing_nobody_rated_has_no_score_rather_than_a_zero() {
        // ★★★ Showing a number nobody produced is the exact defect this row
        //     closes; a zero would reintroduce it wearing a different label.
        let r = market();
        assert_eq!(r.standing(&v("1.0"), &Prior::uninformative()), None);
    }

    #[test]
    fn two_lucky_ratings_do_not_outrank_two_thousand() {
        // ★★★ The posterior shrinks toward the prior when evidence is thin, and
        //     ranking on the lower bound turns that into an ordering.
        let mut r = market();
        let lucky = Versioned::new("new thing", "1.0");
        let proven = Versioned::new("old thing", "1.0");
        for i in 0..2 {
            buy_and_rate(&mut r, &format!("a{i}"), &lucky, true);
        }
        for i in 0..200 {
            buy_and_rate(&mut r, &format!("b{i}"), &proven, true);
        }
        let p = Prior::uninformative();
        assert!(lower_bound(&r.evidence_for(&proven), &p, Z_95)
            > lower_bound(&r.evidence_for(&lucky), &p, Z_95));
    }

    #[test]
    fn uncertainty_is_a_cost_borne_by_the_seller_not_a_free_option() {
        // ★★★ Ranking on τ alone would let a handful of favourable reports push
        //     a new artefact to the top. On the lower bound it must accumulate
        //     evidence to rise.
        let thin = Evidence { good: 3.0, bad: 0.0 };
        let thick = Evidence { good: 300.0, bad: 0.0 };
        let p = Prior::uninformative();
        assert!(tau(&thin, &p) < tau(&thick, &p), "even the point estimate favours thick");
        let gap_point = tau(&thick, &p) - tau(&thin, &p);
        let gap_lcb = lower_bound(&thick, &p, Z_95) - lower_bound(&thin, &p, Z_95);
        assert!(gap_lcb > gap_point, "the lower bound punishes thinness harder");
    }

    #[test]
    fn the_score_is_in_zero_to_one_by_construction_rather_than_by_clamping() {
        // ★★ A bound enforced by arithmetic cannot be forgotten by a caller.
        let p = Prior::uninformative();
        for (g, b) in [(0.0, 0.0), (1e9, 0.0), (0.0, 1e9), (5.0, 5.0)] {
            let t = tau(&Evidence { good: g, bad: b }, &p);
            assert!((0.0..=1.0).contains(&t), "τ was {t}");
        }
    }

    #[test]
    fn old_goodwill_stops_financing_present_cheating() {
        // ★★★ Reputation as a flow, not a stock. A long run of good outcomes
        //     followed by bad ones must fall, and fall faster than an
        //     undecayed count would.
        let mut r = Reputation::with_decay(0.7);
        for i in 0..30 {
            buy_and_rate(&mut r, &format!("good{i}"), &v("1.0"), true);
        }
        let peak = r.standing(&v("1.0"), &Prior::uninformative()).unwrap();
        for i in 0..10 {
            buy_and_rate(&mut r, &format!("bad{i}"), &v("1.0"), false);
        }
        let now = r.standing(&v("1.0"), &Prior::uninformative()).unwrap();
        assert!(now < 0.5, "ten bad outcomes after thirty good ones: {now} (was {peak})");
    }

    #[test]
    fn evidence_is_aged_before_the_new_rating_is_added() {
        // ★★★ §V.3's own order. Ageing afterwards would discount the rating
        //     that just arrived — the one piece of evidence nobody should be
        //     discounting.
        let mut r = Reputation::with_decay(0.5);
        buy_and_rate(&mut r, "a", &v("1.0"), true);
        assert_eq!(r.evidence_for(&v("1.0")).good, 1.0, "the first is undiscounted");
        buy_and_rate(&mut r, "b", &v("1.0"), true);
        assert_eq!(r.evidence_for(&v("1.0")).good, 1.5, "0.5*1 + 1, not 0.5*(1+1)");
    }

    #[test]
    fn how_far_back_the_record_remembers_is_sayable() {
        // ★★ "λ = 0.95" means nothing to a person; "it remembers about the last
        //    twenty" means something.
        assert_eq!(effective_memory(0.95).map(|m| m.round()), Some(20.0));
        assert_eq!(effective_memory(1.0), None, "a record that never forgets has no window");
    }

    #[test]
    fn only_purchasers_may_rate() {
        // ★★★ The narrowing that ties the cost of a fake rating to the cost of
        //     a fake purchase.
        let mut r = market();
        let out = r.rate("a stranger", &v("1.0"), true, 1.0);
        assert_eq!(out, Rated::NotAPurchaser);
        assert!(out.describe().contains("what a fake purchase costs"));
        assert_eq!(r.standing(&v("1.0"), &Prior::uninformative()), None);
    }

    #[test]
    fn a_rating_is_not_revisable() {
        // ★★ Replacement lets a rater test the market and settle on whichever
        //    score suits them.
        let mut r = market();
        r.record_purchase("a", &v("1.0"));
        assert_eq!(r.rate("a", &v("1.0"), true, 1.0), Rated::Recorded);
        assert_eq!(r.rate("a", &v("1.0"), false, 1.0), Rated::AlreadyRated);
    }

    #[test]
    fn a_rater_with_more_standing_moves_the_number_further() {
        // ★★ Stake weighting, with the limit stated in the header: this is
        //    transitive trust, and transitive trust is vulnerable to collusive
        //    clusters.
        let mut light = market();
        let mut heavy = market();
        light.record_purchase("a", &v("1.0"));
        heavy.record_purchase("a", &v("1.0"));
        light.rate("a", &v("1.0"), true, 1.0);
        heavy.rate("a", &v("1.0"), true, 10.0);
        let p = Prior::uninformative();
        assert!(heavy.standing(&v("1.0"), &p).unwrap() > light.standing(&v("1.0"), &p).unwrap());
    }

    #[test]
    fn author_history_is_inherited_and_never_equal() {
        // ★★★ At a discount of 1.0 a good author's next artefact would launch
        //     as though already proven — the malicious-update vector wearing a
        //     different hat.
        let fresh = Prior::uninformative();
        let by_a_good_author = Prior::from_author(0.95, 100.0, 0.1);
        let no_evidence = Evidence::none();
        assert!(tau(&no_evidence, &by_a_good_author) > tau(&no_evidence, &fresh));
        assert!(
            tau(&no_evidence, &by_a_good_author) < 0.95,
            "inherited, not equal to the author's own score"
        );
    }

    #[test]
    fn unrated_things_are_returned_apart_rather_than_sorted_in_at_zero() {
        // ★★ Sorting them last says "bad"; sorting them first says "good".
        //    Both are claims nobody made.
        let mut r = market();
        let known = Versioned::new("known", "1.0");
        let brand_new = Versioned::new("brand new", "1.0");
        buy_and_rate(&mut r, "a", &known, true);
        let (rated, unrated) =
            r.ranked(&[known.clone(), brand_new.clone()], &Prior::uninformative(), Z_95);
        assert_eq!(rated.len(), 1);
        assert_eq!(rated[0].0, known);
        assert_eq!(unrated, vec![brand_new]);
    }

    #[test]
    fn a_constant_column_is_not_a_ranking_and_this_one_is_not_constant() {
        // ★★★ The live defect: `ORDER BY trust_score DESC` over an identically
        //     zero column is an arbitrary order presented as a quality ranking.
        let mut r = market();
        let a = Versioned::new("a", "1.0");
        let b = Versioned::new("b", "1.0");
        for i in 0..10 {
            buy_and_rate(&mut r, &format!("x{i}"), &a, true);
            buy_and_rate(&mut r, &format!("y{i}"), &b, i < 3);
        }
        let (rated, _) = r.ranked(&[a.clone(), b], &Prior::uninformative(), Z_95);
        assert_eq!(rated[0].0, a, "the one that actually satisfied people ranks first");
        assert!(rated[0].1 > rated[1].1);
    }
}
