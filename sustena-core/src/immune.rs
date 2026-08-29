//! **Detection over the log, and why it may never deny** (Immunity · §VII).
//!
//! ```text
//!   anomaly detector  → ATTENTION      it is a classifier, and the base rate beats it
//!   heartbeat         → DENIAL allowed it is a declared expectation, not a guess
//! ```
//!
//! ★★★ **The base-rate limit is arithmetic, not caution.** A detector that is
//! 99% accurate, run over a household where one event in ten thousand is
//! genuinely hostile, produces about a hundred false alarms for every real one.
//! Treating its output as a denial means blocking a hundred legitimate acts to
//! stop one attack, and the household turns the detector off by the end of the
//! week. So the output is **attention** — route it to a person — and
//! [`Disposition::Deny`] is unreachable from this path.
//!
//! ★★★ **A heartbeat has no base-rate problem, and that is the whole reason it
//! is treated differently.** "This source promised to speak every day and has
//! not spoken for three" is a *declared expectation checked deterministically*,
//! not a classification. There is no false-positive rate to multiply by a base
//! rate, because nothing was inferred. So a heartbeat may deny, and it is the
//! only thing here that may.
//!
//! ## Negative selection, and the failure immunology already knows about
//!
//! ★★★ **Whatever was happening during training becomes "self".** A immune
//! system that matures in the presence of a pathogen learns to tolerate it, and
//! a detector that learns "normal" from a window containing an attack learns the
//! attack as normal. This is not a hypothetical — it is the known failure mode of
//! the method being borrowed. [`SelfSet::tolerated_during_training`] exists so
//! the question *"was this shape learned, or has it always been fine?"* has an
//! answer, because a detector that cannot distinguish those is one nobody can
//! audit after an incident.

use std::collections::{BTreeMap, BTreeSet};

/// A shape of thing that happens: what, by whom.
///
/// ★★ Deliberately coarse. A "shape" that included the amount would make every
/// payment its own novelty, and a detector that flags everything has told you
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Shape {
    pub event: String,
    pub principal: String,
}

impl Shape {
    pub fn new(event: &str, principal: &str) -> Self {
        Self { event: event.into(), principal: principal.into() }
    }
}

/// What this Sustain has learned to regard as itself.
#[derive(Debug, Clone, PartialEq)]
pub struct SelfSet {
    shapes: BTreeSet<Shape>,
    /// How the window was described, for the audit that comes after.
    learned_from: String,
}

impl SelfSet {
    /// **Learn "self" from a window of the log.**
    ///
    /// ★★ The window must be *described*, not merely bounded. "The last 30 days"
    /// and "the 30 days after the phone was compromised" are the same integer
    /// and very different provenance.
    pub fn learned_from(window: &str, observed: &[Shape]) -> Self {
        Self { shapes: observed.iter().cloned().collect(), learned_from: window.into() }
    }

    pub fn window(&self) -> &str {
        &self.learned_from
    }

    pub fn is_self(&self, shape: &Shape) -> bool {
        self.shapes.contains(shape)
    }

    /// ★★★ **Was this shape learned as self, rather than being self?**
    ///
    /// The same question, asked so an auditor can answer it. Everything in the
    /// set was learned; what matters after an incident is being able to say so
    /// out loud, and to name the window it was learned from.
    pub fn tolerated_during_training(&self, shape: &Shape) -> Option<&str> {
        self.is_self(shape).then_some(self.learned_from.as_str())
    }

    pub fn size(&self) -> usize {
        self.shapes.len()
    }
}

/// A detector's declared error rates.
///
/// ★★★ Declared, because a detector with unstated rates cannot have its
/// precision computed — and an uncomputable precision is exactly how a
/// classifier ends up trusted as a denial.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    /// How often it catches a real one.
    pub true_positive: f64,
    /// How often it flags a harmless one.
    pub false_positive: f64,
}

impl Rates {
    pub fn new(true_positive: f64, false_positive: f64) -> Self {
        Self { true_positive, false_positive }
    }
}

/// **`P(hostile | flagged)`** — the number that decides what a flag is worth.
///
/// ★★★ Bayes, and the whole of §VII's caution in one function. A detector's
/// accuracy says nothing on its own; the base rate does most of the work, and it
/// does it in the direction people do not expect.
pub fn precision(base_rate: f64, rates: Rates) -> Option<f64> {
    if !(0.0..=1.0).contains(&base_rate) {
        return None;
    }
    let hits = base_rate * rates.true_positive;
    let misses = (1.0 - base_rate) * rates.false_positive;
    let flagged = hits + misses;
    (flagged > 0.0).then(|| hits / flagged)
}

/// What may be done about a finding.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// ★★★ Put it in front of a person. **The only outcome an anomaly detector
    /// can produce**, whatever its accuracy.
    Attention { why: String, precision: Option<f64> },
    /// ★★★ Refuse it outright. Reachable **only** from a deterministic check —
    /// see [`heartbeat_missed`]. There is no path from [`flag`] to here.
    Deny { because: String },
    /// Nothing to say.
    Quiet,
}

impl Disposition {
    pub fn denies(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Attention { why, precision } => match precision {
                Some(p) => format!(
                    "worth a look: {why} — about {:.0}% of flags like this turn out to be real",
                    p * 100.0
                ),
                None => format!("worth a look: {why}"),
            },
            Self::Deny { because } => format!("refused: {because}"),
            Self::Quiet => "nothing unusual".into(),
        }
    }
}

/// **Flag a shape against what this Sustain knows as itself.**
///
/// ★★★ The return type is the argument. There is no branch of this function
/// that can produce a `Deny`, however confident the detector's declared rates
/// are — because confidence is not what is missing. The base rate is.
pub fn flag(shape: &Shape, known: &SelfSet, base_rate: f64, rates: Rates) -> Disposition {
    if known.is_self(shape) {
        return Disposition::Quiet;
    }
    Disposition::Attention {
        why: format!(
            "'{}' by '{}' has not been seen before (learned from {})",
            shape.event,
            shape.principal,
            known.window()
        ),
        precision: precision(base_rate, rates),
    }
}

/// **A declared expectation, checked. This one may deny.**
///
/// ★★★ Not a classifier: nothing was inferred, so there is no false-positive
/// rate to multiply by a base rate. "It promised to speak every day and has not
/// spoken for three" is a fact about a promise.
pub fn heartbeat_missed(source: &str, promised_ms: i64, silent_for_ms: i64) -> Disposition {
    if silent_for_ms <= promised_ms {
        return Disposition::Quiet;
    }
    Disposition::Deny {
        because: format!(
            "{source} promised to report every {}h and has been silent for {}h — this is a \
             checked expectation, not a guess",
            promised_ms / 3_600_000,
            silent_for_ms / 3_600_000
        ),
    }
}

/// How many false alarms a person would see per real one.
///
/// ★★★ The number to put in front of whoever wants the detector to block
/// things. "97% accurate" persuades; "you will investigate 32 innocent
/// transactions for every real one" decides.
pub fn false_alarms_per_real(base_rate: f64, rates: Rates) -> Option<f64> {
    let p = precision(base_rate, rates)?;
    (p > 0.0).then(|| (1.0 - p) / p)
}

/// Everything flagged, ordered so the report is stable.
pub fn sweep(
    shapes: &[Shape],
    known: &SelfSet,
    base_rate: f64,
    rates: Rates,
) -> Vec<(Shape, Disposition)> {
    let mut out: Vec<(Shape, Disposition)> = shapes
        .iter()
        .map(|s| (s.clone(), flag(s, known, base_rate, rates)))
        .filter(|(_, d)| !matches!(d, Disposition::Quiet))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// How much of the log a `SelfSet` covers, as a fraction.
///
/// ★★ A self-set that matches everything flags nothing and is worthless; one
/// that matches almost nothing flags everything and is worse. Askable, so the
/// detector's usefulness is a number rather than a feeling.
pub fn coverage(known: &SelfSet, recent: &[Shape]) -> Option<f64> {
    (!recent.is_empty())
        .then(|| recent.iter().filter(|s| known.is_self(s)).count() as f64 / recent.len() as f64)
}

/// Which shapes account for the flags, most first.
///
/// ★★ One novel shape appearing four hundred times is one finding, not four
/// hundred, and a report that does not group is a report nobody finishes.
pub fn grouped(flags: &[(Shape, Disposition)]) -> Vec<(Shape, usize)> {
    let mut counts: BTreeMap<&Shape, usize> = BTreeMap::new();
    for (s, _) in flags {
        *counts.entry(s).or_insert(0) += 1;
    }
    let mut out: Vec<(Shape, usize)> = counts.into_iter().map(|(s, n)| (s.clone(), n)).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn ordinary() -> SelfSet {
        SelfSet::learned_from(
            "the 30 days before the phone was set up",
            &[
                Shape::new("finances.pocket_spent", "bonnie"),
                Shape::new("finances.income_recorded", "mpesa"),
            ],
        )
    }

    fn good_detector() -> Rates {
        Rates::new(0.99, 0.01)
    }

    #[test]
    fn an_anomaly_detector_can_only_ask_for_attention() {
        // ★★★ There is no branch of `flag` that produces a Deny, however good
        //     the declared rates are — because confidence is not what is
        //     missing. The base rate is.
        let novel = Shape::new("egress.pay", "somebody new");
        let d = flag(&novel, &ordinary(), 0.0001, good_detector());
        assert!(!d.denies());
        assert!(matches!(d, Disposition::Attention { .. }));
    }

    #[test]
    fn a_ninety_nine_percent_detector_is_wrong_almost_every_time_it_fires() {
        // ★★★ The base-rate limit as arithmetic rather than caution. One event
        //     in ten thousand is hostile, and about a hundred innocent ones are
        //     investigated for each real one.
        let p = precision(0.0001, good_detector()).expect("computable");
        assert!(p < 0.01, "precision was {p}");
        let per_real = false_alarms_per_real(0.0001, good_detector()).expect("computable");
        assert!(per_real > 90.0, "{per_real} innocent per real one");
    }

    #[test]
    fn the_number_that_decides_is_the_one_a_person_can_act_on() {
        // ★★★ "97% accurate" persuades; "you will investigate a hundred
        //     innocent transactions for every real one" decides.
        let d = flag(&Shape::new("egress.pay", "new"), &ordinary(), 0.0001, good_detector());
        assert!(d.describe().contains("turn out to be real"));
    }

    #[test]
    fn a_heartbeat_may_deny_because_nothing_was_inferred() {
        // ★★★ A declared expectation checked deterministically has no
        //     false-positive rate to multiply by a base rate.
        let d = heartbeat_missed("bonnie's phone", 24 * HOUR, 72 * HOUR);
        assert!(d.denies());
        assert!(d.describe().contains("not a guess"));
    }

    #[test]
    fn a_heartbeat_that_was_kept_says_nothing() {
        assert_eq!(heartbeat_missed("a phone", 24 * HOUR, 3 * HOUR), Disposition::Quiet);
    }

    #[test]
    fn a_familiar_shape_is_quiet() {
        assert_eq!(
            flag(&Shape::new("finances.pocket_spent", "bonnie"), &ordinary(), 0.001, good_detector()),
            Disposition::Quiet
        );
    }

    #[test]
    fn whatever_was_happening_during_training_became_self() {
        // ★★★ The known failure of the method being borrowed: an immune system
        //     that matures alongside a pathogen learns to tolerate it. A
        //     detector that cannot say "this was LEARNED, from this window" is
        //     one nobody can audit after an incident.
        let compromised = SelfSet::learned_from(
            "the 30 days after the phone was compromised",
            &[Shape::new("egress.pay", "the attacker")],
        );
        let attack = Shape::new("egress.pay", "the attacker");
        assert_eq!(flag(&attack, &compromised, 0.001, good_detector()), Disposition::Quiet);
        assert_eq!(
            compromised.tolerated_during_training(&attack),
            Some("the 30 days after the phone was compromised"),
            "and the window it was learned from is recoverable"
        );
    }

    #[test]
    fn a_shape_that_was_never_learned_has_no_training_window_to_report() {
        assert_eq!(ordinary().tolerated_during_training(&Shape::new("x", "y")), None);
    }

    #[test]
    fn a_shape_is_coarse_on_purpose() {
        // ★★ Including the amount would make every payment its own novelty, and
        //    a detector that flags everything has told you nothing.
        let a = Shape::new("finances.pocket_spent", "bonnie");
        let b = Shape::new("finances.pocket_spent", "bonnie");
        assert_eq!(a, b);
    }

    #[test]
    fn how_useful_the_self_set_is_is_a_number_rather_than_a_feeling() {
        // ★★ One that matches everything flags nothing; one that matches almost
        //    nothing flags everything, which is worse.
        let recent = [
            Shape::new("finances.pocket_spent", "bonnie"),
            Shape::new("finances.pocket_spent", "bonnie"),
            Shape::new("egress.pay", "new"),
        ];
        let c = coverage(&ordinary(), &recent).expect("computable");
        assert!((c - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(coverage(&ordinary(), &[]), None);
    }

    #[test]
    fn four_hundred_of_the_same_novelty_is_one_finding() {
        // ★★ A report that does not group is a report nobody finishes.
        let noisy: Vec<Shape> = (0..400).map(|_| Shape::new("egress.pay", "new")).collect();
        let flags = sweep(&noisy, &ordinary(), 0.001, good_detector());
        assert_eq!(flags.len(), 400);
        let g = grouped(&flags);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].1, 400);
    }

    #[test]
    fn a_base_rate_outside_its_own_range_computes_nothing() {
        assert_eq!(precision(1.5, good_detector()), None);
        assert_eq!(precision(-0.1, good_detector()), None);
    }

    #[test]
    fn a_detector_that_never_fires_has_no_precision_rather_than_a_perfect_one() {
        // ★★ Dividing by nothing flatters a detector that has caught nothing,
        //    and a metric that flatters silence is one people learn to game.
        assert_eq!(precision(0.0, Rates::new(0.99, 0.0)), None);
    }
}
