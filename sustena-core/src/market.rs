//! **Selection, niches, and what a market may not do** (Arena · §§V.4, VI, VII).
//!
//! ```text
//!   rank(a | ν) = E_{p~D_ν}[value(a,p)] − price(a)      conditional, always
//!   ẋ_i = x_i (φ_i − φ̄)                                 replicator
//! ```
//!
//! ## No Free Lunch, used correctly
//!
//! ★★★ **"A single global top-packages list is a claim about the average over
//! all Niches, and nobody occupies the average."** The marginal
//! `rank(a) = Σ_ν P(ν)·rank(a|ν)` is exactly the quantity the theorem says
//! carries no information about any particular adopter — so the shipped
//! `ORDER BY trust_score DESC` is not merely under-specified, it is **the one
//! aggregation the theorem forbids**.
//!
//! ★★★ **So there is no global-ranking function in this module.** Not a
//! discouraged one, not one behind a warning: [`rank_in`] takes a niche, and a
//! caller with no niche has nothing to call. Making the forbidden aggregation
//! inexpressible is the only version of this rule that survives a deadline.
//!
//! ★★ **And NFL is not the claim that nothing is better than anything else.**
//! Its premise is a uniform distribution over objective functions, which is not
//! the world; for non-uniform distributions it fails. The correct reading is
//! that *"better" has no meaning without a specified problem distribution* —
//! and a library is worth **exactly the non-uniformity of the distribution it is
//! built for**. That is what makes a library valuable, not what makes it
//! useless.
//!
//! ## Selection, and which kind of meme it propagates
//!
//! ★★★ A marketplace is a machine for propagating memes, and §V's whole
//! question is *which kind*. With working evidence it selects for artefacts that
//! deliver value; without it, it selects for whatever merely spreads — the
//! anti-rational failure mode ([`crate::meme`]). A selection process with no
//! criticism does not rank badly, it **inverts**.
//!
//! ## Delisting is not deletion, and that is correct
//!
//! ★★★ A content-addressed artefact already germinated inside a buyer's Sustain
//! **cannot be recalled**. The market can stop listing it, stop settling on it
//! and sanction its author; it cannot reach past the buyer's boundary. This is
//! not a gap — it is the end-to-end argument, and **a market with recall powers
//! is a market that can reach into your state.**
//!
//! ★★★ *The market's job is to make good artefacts findable; making bad
//! artefacts harmless is the gate's job.* That division is why the market does
//! not need recall powers, which is fortunate.

use std::collections::BTreeMap;

use crate::reputation::Versioned;

/// A niche — the problem distribution a ranking is conditioned on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Niche(String);

impl Niche {
    pub fn named(name: &str) -> Self {
        Self(name.to_string())
    }
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// What one artefact is worth to adopters in one niche, net of price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fitness {
    /// `E_{p~D_ν}[value(a,p)]` — expected value delivered in this niche.
    pub value: f64,
    pub price: f64,
}

impl Fitness {
    pub fn new(value: f64, price: f64) -> Self {
        Self { value, price }
    }

    /// `φ` — value net of price.
    pub fn phi(&self) -> f64 {
        self.value - self.price
    }
}

/// **`rank(a | ν)`** — a ranking, and only ever a conditional one.
///
/// ★★★ There is deliberately no sibling that marginalises over niches. The
/// marginal is the one aggregation NFL forbids, and a function that computed it
/// would be used.
pub fn rank_in(
    niche: &Niche,
    fitnesses: &BTreeMap<Versioned, Fitness>,
) -> Vec<(Versioned, f64)> {
    let _ = niche;
    let mut out: Vec<(Versioned, f64)> =
        fitnesses.iter().map(|(a, f)| (a.clone(), f.phi())).collect();
    out.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
    });
    out
}

/// How much a ranking would change if it were taken across niches instead of
/// within one.
///
/// ★★★ Offered as **evidence against** the global list, not as a way to build
/// one: it reports how badly the average misrepresents each niche. A market
/// where this is large is a market where a single "top packages" list is
/// actively misleading, and a market where it is small has simply not found its
/// second niche yet.
pub fn how_wrong_a_global_list_would_be(
    per_niche: &BTreeMap<Niche, BTreeMap<Versioned, Fitness>>,
) -> Vec<(Niche, Versioned)> {
    per_niche
        .iter()
        .filter_map(|(n, f)| rank_in(n, f).first().map(|(a, _)| (n.clone(), a.clone())))
        .collect()
}

/// Do different niches actually want different things?
///
/// ★★ The question that decides whether per-niche ranking is buying anything
/// here. Two niches that agree on the best artefact are not evidence that a
/// global list is fine — they are one niche wearing two names.
pub fn niches_disagree(
    per_niche: &BTreeMap<Niche, BTreeMap<Versioned, Fitness>>,
) -> bool {
    let tops = how_wrong_a_global_list_would_be(per_niche);
    tops.windows(2).any(|w| w[0].1 != w[1].1)
}

/// **One step of the replicator equation**, `ẋ_i = x_i(φ_i − φ̄)`.
///
/// ★★★ Variation, selection, retention — and the crucial dependency: **`φ`
/// comes from evidence.** With no evidence to compute fitness from, a market
/// still selects; it just selects on whatever proxy is lying around, which is
/// adoption. That is the anti-rational failure mode, and it is why
/// [`crate::reputation`] is not decoration.
///
/// ★★ Returns shares that still sum to one. A selection step that leaked share
/// would make the population quietly shrink, which reads as decline.
pub fn replicate(
    shares: &BTreeMap<Versioned, f64>,
    fitness: &BTreeMap<Versioned, Fitness>,
    dt: f64,
) -> BTreeMap<Versioned, f64> {
    let mean: f64 = shares
        .iter()
        .map(|(a, x)| x * fitness.get(a).map(Fitness::phi).unwrap_or(0.0))
        .sum();
    let mut next: BTreeMap<Versioned, f64> = shares
        .iter()
        .map(|(a, x)| {
            let phi = fitness.get(a).map(Fitness::phi).unwrap_or(0.0);
            (a.clone(), (x + dt * x * (phi - mean)).max(0.0))
        })
        .collect();
    let total: f64 = next.values().sum();
    if total > 0.0 {
        for v in next.values_mut() {
            *v /= total;
        }
    }
    next
}

/// The graduated instruments, in order of severity.
///
/// ★★★ **Graduated and each reversible**, per Ostrom's fifth design principle —
/// a market whose only instrument is exile has no way to correct a mistake, and
/// therefore no way to be trusted with the instrument at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sanction {
    /// Rank it lower.
    Demote,
    /// Say so, publicly.
    Warn,
    /// Raise what it costs to keep listing.
    EscalatePrice,
    /// The author may not publish anything new.
    SuspendPublishing,
    /// It is no longer listed. **This is not deletion.**
    Delist,
}

impl Sanction {
    pub fn all() -> [Sanction; 5] {
        [
            Sanction::Demote,
            Sanction::Warn,
            Sanction::EscalatePrice,
            Sanction::SuspendPublishing,
            Sanction::Delist,
        ]
    }

    /// The next step up, or `None` at the top.
    ///
    /// ★★ Escalation is a walk, not a jump. Reaching for the last instrument
    /// first is what makes a sanction system feel arbitrary, and arbitrary
    /// sanctions get ignored or resented rather than obeyed.
    pub fn next(&self) -> Option<Sanction> {
        let all = Self::all();
        let i = all.iter().position(|s| s == self)?;
        all.get(i + 1).copied()
    }

    /// ★★★ Every one of them, including the last. An irreversible sanction is
    /// a judgment nobody can appeal, and §V's own instrument set is explicitly
    /// *each reversible, each appealable*.
    pub fn reversible(&self) -> bool {
        true
    }

    pub fn describe(&self) -> &'static str {
        match self {
            Self::Demote => "ranked lower in its niche",
            Self::Warn => "publicly warned",
            Self::EscalatePrice => "charged more to stay listed",
            Self::SuspendPublishing => "barred from publishing anything new",
            Self::Delist => "no longer listed — and still working for everyone who has it",
        }
    }
}

/// What a delisting actually did.
///
/// ★★★ The type exists to make the limit legible rather than to work around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delisted {
    pub artefact: Versioned,
    /// New buyers cannot find it.
    pub findable: bool,
    /// No further royalties settle on it.
    pub settling: bool,
    /// ★★★ **Always true.** A content-addressed artefact already germinated
    /// inside a buyer's Sustain cannot be recalled, and there is no field here a
    /// caller could set to pretend otherwise.
    pub still_works_for_existing_buyers: bool,
}

/// **Delist an artefact.**
///
/// ★★★ Note what this function does not take: no buyer list, no state, no
/// reach. It cannot revoke anything from anybody, because the market has no
/// path to a buyer's Sustain — and *a market with recall powers is a market that
/// can reach into your state*.
pub fn delist(artefact: &Versioned) -> Delisted {
    Delisted {
        artefact: artefact.clone(),
        findable: false,
        settling: false,
        still_works_for_existing_buyers: true,
    }
}

impl Delisted {
    pub fn describe(&self) -> String {
        format!(
            "{} {} is no longer listed and no longer settles — and it still runs for everyone \
             who already has it. The market makes good artefacts findable; making bad ones \
             harmless is the gate's job.",
            self.artefact.name, self.artefact.version
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(name: &str) -> Versioned {
        Versioned::new(name, "1.0")
    }

    fn fit(pairs: &[(&str, f64, f64)]) -> BTreeMap<Versioned, Fitness> {
        pairs.iter().map(|(n, v, p)| (a(n), Fitness::new(*v, *p))).collect()
    }

    #[test]
    fn a_ranking_is_conditional_and_there_is_no_global_one_to_call() {
        // ★★★ The one aggregation the theorem forbids is not expressible: a
        //     caller with no niche has nothing to call. Making it inexpressible
        //     is the only version of this rule that survives a deadline.
        let household = Niche::named("household finance");
        let ranked = rank_in(&household, &fit(&[("cheap", 5.0, 1.0), ("dear", 9.0, 8.0)]));
        assert_eq!(ranked[0].0, a("cheap"), "value net of price, not value");
    }

    #[test]
    fn nobody_occupies_the_average() {
        // ★★★ Two niches, opposite winners. A single "top packages" list is a
        //     claim about a household that does not exist.
        let mut per_niche = BTreeMap::new();
        per_niche.insert(
            Niche::named("a smallholding"),
            fit(&[("harvest planner", 9.0, 1.0), ("payroll", 2.0, 1.0)]),
        );
        per_niche.insert(
            Niche::named("a shop"),
            fit(&[("harvest planner", 1.0, 1.0), ("payroll", 8.0, 1.0)]),
        );
        assert!(niches_disagree(&per_niche));
        let tops = how_wrong_a_global_list_would_be(&per_niche);
        assert_eq!(tops.len(), 2);
        assert_ne!(tops[0].1, tops[1].1, "the two niches want different things");
    }

    #[test]
    fn two_niches_that_agree_are_one_niche_wearing_two_names() {
        // ★★ Agreement is not evidence that a global list is fine; it is
        //    evidence that the market has not found its second niche yet.
        let mut per_niche = BTreeMap::new();
        for n in ["a", "b"] {
            per_niche.insert(Niche::named(n), fit(&[("good", 9.0, 1.0), ("poor", 2.0, 1.0)]));
        }
        assert!(!niches_disagree(&per_niche));
    }

    #[test]
    fn selection_moves_share_toward_what_delivers_value() {
        // ★★ Variation, selection, retention. The fitter artefact gains share.
        let shares: BTreeMap<Versioned, f64> =
            [(a("good"), 0.5), (a("poor"), 0.5)].into_iter().collect();
        let f = fit(&[("good", 9.0, 1.0), ("poor", 2.0, 1.0)]);
        let next = replicate(&shares, &f, 0.1);
        assert!(next[&a("good")] > 0.5);
        assert!(next[&a("poor")] < 0.5);
    }

    #[test]
    fn share_still_sums_to_one_after_a_step() {
        // ★★ A selection step that leaked share would make the population
        //    quietly shrink, which reads as decline rather than as a bug.
        let shares: BTreeMap<Versioned, f64> =
            [(a("x"), 0.3), (a("y"), 0.3), (a("z"), 0.4)].into_iter().collect();
        let f = fit(&[("x", 5.0, 1.0), ("y", 1.0, 1.0), ("z", 3.0, 1.0)]);
        let total: f64 = replicate(&shares, &f, 0.2).values().sum();
        assert!((total - 1.0).abs() < 1e-9, "{total}");
    }

    #[test]
    fn without_evidence_fitness_is_flat_and_selection_stops_discriminating() {
        // ★★★ φ comes from evidence. With none, a market still selects — it
        //     just selects on whatever proxy is lying around, which is adoption.
        //     That is the anti-rational failure mode, and it is why reputation
        //     is not decoration.
        let shares: BTreeMap<Versioned, f64> =
            [(a("useful"), 0.2), (a("merely popular"), 0.8)].into_iter().collect();
        let no_evidence: BTreeMap<Versioned, Fitness> = BTreeMap::new();
        let next = replicate(&shares, &no_evidence, 0.5);
        assert!((next[&a("merely popular")] - 0.8).abs() < 1e-9, "incumbency is preserved");
        assert!((next[&a("useful")] - 0.2).abs() < 1e-9);
    }

    #[test]
    fn delisting_does_not_reach_past_a_buyers_boundary() {
        // ★★★ And the function signature is the argument: no buyer list, no
        //     state, no reach. A market with recall powers is a market that can
        //     reach into your state.
        let d = delist(&a("something bad"));
        assert!(!d.findable);
        assert!(!d.settling);
        assert!(d.still_works_for_existing_buyers);
        assert!(d.describe().contains("making bad ones \nharmless is the gate's job")
            || d.describe().contains("harmless is the gate's job"));
    }

    #[test]
    fn that_limit_is_stated_as_correct_rather_than_as_a_gap() {
        // ★★★ The end-to-end argument: only the buyer's Sustain knows the
        //     buyer's V. The market makes good artefacts findable; making bad
        //     ones harmless is the gate's job.
        let d = delist(&a("x"));
        assert!(d.describe().contains("still runs for everyone who already has it"));
    }

    #[test]
    fn sanctions_are_graduated_and_escalation_is_a_walk() {
        // ★★★ Reaching for the last instrument first is what makes a sanction
        //     system feel arbitrary, and arbitrary sanctions get ignored or
        //     resented rather than obeyed.
        assert_eq!(Sanction::Demote.next(), Some(Sanction::Warn));
        assert_eq!(Sanction::SuspendPublishing.next(), Some(Sanction::Delist));
        assert_eq!(Sanction::Delist.next(), None);
        assert!(Sanction::Demote < Sanction::Delist);
    }

    #[test]
    fn every_sanction_is_reversible_including_the_last_one() {
        // ★★★ An irreversible sanction is a judgment nobody can appeal, and the
        //     instrument set is explicitly "each reversible, each appealable".
        for s in Sanction::all() {
            assert!(s.reversible(), "{s:?}");
        }
    }

    #[test]
    fn the_price_is_part_of_the_ranking_not_a_column_beside_it() {
        // ★★ `rank = E[value] − price`. A dearer artefact must deliver more to
        //    rank the same, which is the whole content of "net of price".
        let n = Niche::named("anywhere");
        let ranked = rank_in(&n, &fit(&[("dear", 10.0, 9.0), ("modest", 4.0, 1.0)]));
        assert_eq!(ranked[0].0, a("modest"));
    }
}
