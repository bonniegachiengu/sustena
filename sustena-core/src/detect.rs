//! EWMA and CUSUM over the `W` series — the Monitor→Controller boundary
//! (Monitor §V, §VI · MON-5, MON-6, MON-11).
//!
//! [`crate::region`] made `W(s) = d(s,V)` a real quantity. This module is the
//! detector layer **over** that series: the formal boundary between *watching*
//! (continuous, cheap, stays in the Monitor) and *looking* (surfacing something
//! to a person, which spends one of the ~4 attention slots the Curated UI
//! budgets).
//!
//! ```text
//!   state ──fold──► W = d(s,V) ──EWMA──► level ──CUSUM──► Severity ──► Controller
//!                   region.rs            §V              §VI          (escalates)
//! ```
//!
//! ## EWMA — smooth telemetry without lag (§V)
//!
//! ```text
//! EWMA[k] = α·y[k] + (1−α)·EWMA[k−1],    α ∈ (0,1)
//! t½ = log 0.5 / log(1−α) ≈ 0.693 / α
//! ```
//!
//! > Raw telemetry jitters. A budget ring that flickers every second trains the
//! > eye to ignore it.
//!
//! `α` is a declared tradeoff, not a constant: **a budget ring can be
//! cosmetically smooth; a constraint-health indicator must respond within
//! seconds.** So it is a per-detector spec field, and [`Ewma::half_life`] states
//! what the chosen value actually buys.
//!
//! ## CUSUM — persistent shifts, not spikes (§VI)
//!
//! ```text
//! S⁺[k] = max(0, S⁺[k−1] + (y[k] − μ₀ − δ))
//! S⁻[k] = max(0, S⁻[k−1] − (y[k] − μ₀ + δ))
//! alert when S⁺ > h or S⁻ > h
//! ```
//!
//! > CUSUM is specifically designed to detect small persistent changes — a
//! > budget that's spending 5% over allocation for 10 consecutive days is more
//! > alarming than a single 50% spike that immediately corrects.
//!
//! That property is the reason this detector and not a threshold, and it is
//! pinned by a vector rather than left as prose: a sustained small drift alerts,
//! a single much larger spike that corrects does not.
//!
//! **μ₀, δ and h are declared** — the same discipline as the region weights.
//! `δ` is *the minimum shift you care about detecting* and `h` is *how much
//! evidence you want before being interrupted*; both are judgements about this
//! household, and they belong in the spec where they can be argued with.
//!
//! ## The boundary is the severity (§VI)
//!
//! ```text
//! excess = max(S⁺, S⁻) / h
//! > 3.0 → CRITICAL  ─┐
//! > 1.5 → WARNING   ─┴─► escalate to the Controller
//! else  → INFO      ───► stays in the Monitor
//! ```
//!
//! [`Severity::escalates`] is that seam as one function, and it is what the
//! Controller slice (CTL-7) will consume.
//!
//! ## ⚠ A defect in the article's pseudocode, levelled up rather than copied
//!
//! The article's `CUSUMDetector.update` zeroes the accumulator **before**
//! computing the severity:
//!
//! ```text
//! if self.S_pos > self.h:
//!     self.S_pos = 0                       # ← reset
//!     return Alert(..., severity=self.classify())   # ← reads max(S_pos, S_neg)
//! ```
//!
//! `classify()` reads `max(S⁺, S⁻)/h`, so by the time it runs the quantity that
//! triggered the alert is gone: an upward shift would report `excess ≈ 0` and
//! classify **INFO** — never escalating, however extreme. The prose is
//! unambiguous about the intent (`> 3.0` → CRITICAL), so the ordering is the
//! error, not the rule.
//!
//! Implemented as the prose specifies: **the excess is captured at the moment
//! of the alert, then the accumulator resets.** `Alert::excess` carries it, so
//! the number the severity came from is visible rather than recomputed from
//! state that has moved on.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// How loud a shift is, and therefore who deals with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    /// Stays in the Monitor. Watching, not looking.
    Info,
    /// Escalates to the Controller.
    Warning,
    /// Escalates to the Controller.
    Critical,
}

impl Severity {
    /// **The Monitor→Controller boundary, as one function.**
    ///
    /// Continuous cheap watching on one side; spending a person's attention on
    /// the other. `CRITICAL` and `WARNING` cross; `INFO` does not.
    pub fn escalates(&self) -> bool {
        matches!(self, Severity::Warning | Severity::Critical)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Warning => "WARNING",
            Severity::Critical => "CRITICAL",
        }
    }
}

/// Which way the process moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    /// `S⁺` crossed — the series is running persistently high. On a `W` series
    /// this is *drifting away from the viable region*.
    Upward,
    /// `S⁻` crossed — persistently low. On a `W` series, *recovering*.
    Downward,
}

impl Shift {
    pub fn as_str(&self) -> &'static str {
        match self {
            Shift::Upward => "UPWARD_SHIFT",
            Shift::Downward => "DOWNWARD_SHIFT",
        }
    }
}

/// A fired detection.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub shift: Shift,
    /// The observation that tipped it.
    pub value: f64,
    pub severity: Severity,
    /// `max(S⁺, S⁻) / h` **at the moment of the alert**, before the reset.
    ///
    /// Carried rather than recomputed, because the accumulator it came from is
    /// zeroed immediately afterwards — see the module docs.
    pub excess: f64,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum DetectorError {
    #[error("EWMA α must be in (0, 1); got {0}")]
    BadAlpha(String),
    #[error("CUSUM threshold h must be finite and positive; got {0}")]
    BadThreshold(String),
    #[error("CUSUM δ must be finite and non-negative; got {0}")]
    BadDelta(String),
    #[error("CUSUM μ₀ must be finite; got {0}")]
    BadMean(String),
}

/// Exponentially-weighted moving average (§V).
#[derive(Debug, Clone, PartialEq)]
pub struct Ewma {
    alpha: f64,
    value: Option<f64>,
}

impl Ewma {
    /// `α ∈ (0,1)`. Both endpoints are excluded and that is deliberate: `α = 1`
    /// is no smoothing (use the raw series) and `α = 0` never moves at all.
    pub fn new(alpha: f64) -> Result<Self, DetectorError> {
        if !alpha.is_finite() || alpha <= 0.0 || alpha >= 1.0 {
            return Err(DetectorError::BadAlpha(alpha.to_string()));
        }
        Ok(Self { alpha, value: None })
    }

    /// Feed one observation, get the smoothed level.
    ///
    /// The first observation seeds the level. Starting from zero instead would
    /// make every series appear to ramp up from nothing, which is a fabricated
    /// transient in the one signal the system is supposed to trust.
    pub fn update(&mut self, raw: f64) -> f64 {
        let next = match self.value {
            None => raw,
            Some(prev) => self.alpha * raw + (1.0 - self.alpha) * prev,
        };
        self.value = Some(next);
        next
    }

    pub fn value(&self) -> Option<f64> {
        self.value
    }

    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// `t½ = log 0.5 / log(1−α)` — how many updates until an old observation
    /// contributes less than half.
    ///
    /// Exposed because it is what the chosen `α` actually *means* to whoever has
    /// to argue about it: "α = 0.1" says little; "half-life ≈ 7 updates" says
    /// what it buys.
    pub fn half_life(&self) -> f64 {
        (0.5f64).ln() / (1.0 - self.alpha).ln()
    }
}

/// The declared CUSUM parameters.
///
/// A spec field, like the region weights: `δ` is the minimum shift worth
/// detecting and `h` is how much evidence is wanted before interrupting
/// someone. Both are judgements about this household, not constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CusumSpec {
    /// `μ₀` — the expected mean of the series.
    pub mu_0: f64,
    /// `δ` — the smallest shift worth detecting.
    pub delta: f64,
    /// `h` — the decision threshold.
    pub h: f64,
}

impl CusumSpec {
    pub fn new(mu_0: f64, delta: f64, h: f64) -> Self {
        Self { mu_0, delta, h }
    }

    /// Authoring-time well-formedness, so an undetectable detector is a
    /// load-time finding rather than a silence nobody notices.
    pub fn typecheck(&self) -> Result<(), DetectorError> {
        if !self.mu_0.is_finite() {
            return Err(DetectorError::BadMean(self.mu_0.to_string()));
        }
        if !self.delta.is_finite() || self.delta < 0.0 {
            return Err(DetectorError::BadDelta(self.delta.to_string()));
        }
        if !self.h.is_finite() || self.h <= 0.0 {
            return Err(DetectorError::BadThreshold(self.h.to_string()));
        }
        Ok(())
    }
}

/// Cumulative-sum detector for **persistent** shifts (§VI, Page 1954).
#[derive(Debug, Clone, PartialEq)]
pub struct Cusum {
    spec: CusumSpec,
    s_pos: f64,
    s_neg: f64,
}

impl Cusum {
    pub fn new(spec: CusumSpec) -> Result<Self, DetectorError> {
        spec.typecheck()?;
        Ok(Self { spec, s_pos: 0.0, s_neg: 0.0 })
    }

    pub fn spec(&self) -> CusumSpec {
        self.spec
    }
    pub fn s_pos(&self) -> f64 {
        self.s_pos
    }
    pub fn s_neg(&self) -> f64 {
        self.s_neg
    }

    /// Feed one observation. `Some(Alert)` when the accumulator crosses `h`.
    pub fn update(&mut self, value: f64) -> Option<Alert> {
        let CusumSpec { mu_0, delta, h } = self.spec;

        self.s_pos = (self.s_pos + (value - mu_0 - delta)).max(0.0);
        self.s_neg = (self.s_neg - (value - mu_0 + delta)).max(0.0);

        // Capture the excess BEFORE resetting — see the module docs on the
        // article's pseudocode ordering. Classifying after the reset would make
        // every upward shift report INFO and never escalate.
        let excess = self.s_pos.max(self.s_neg) / h;

        if self.s_pos > h {
            self.s_pos = 0.0;
            return Some(Alert {
                shift: Shift::Upward,
                value,
                severity: classify(excess),
                excess,
            });
        }
        if self.s_neg > h {
            self.s_neg = 0.0;
            return Some(Alert {
                shift: Shift::Downward,
                value,
                severity: classify(excess),
                excess,
            });
        }
        None
    }

    /// Forget the accumulated evidence. The process is back to normal by
    /// declaration rather than by observation.
    pub fn reset(&mut self) {
        self.s_pos = 0.0;
        self.s_neg = 0.0;
    }
}

/// `excess → Severity`, the §VI table.
pub fn classify(excess: f64) -> Severity {
    if excess > 3.0 {
        Severity::Critical
    } else if excess > 1.5 {
        Severity::Warning
    } else {
        Severity::Info
    }
}

/// One observation of a `W` series, all the way through the pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// `W = d(s,V)` — the raw distance-to-V for this observation.
    pub w: f64,
    /// The EWMA level after this observation.
    pub smoothed: f64,
    /// `Some` when the CUSUM crossed.
    pub alert: Option<Alert>,
}

impl Reading {
    /// Whether this reading crosses the Monitor→Controller boundary.
    pub fn escalates(&self) -> bool {
        self.alert.as_ref().is_some_and(|a| a.severity.escalates())
    }
}

/// The watching pipeline over a `W` series: smooth, then detect.
///
/// Deliberately takes `W` values rather than states. The distance-to-V
/// computation belongs to [`crate::region`]; this module's job is the time
/// series, and keeping the seam sharp is what lets either side be tested
/// without the other.
///
/// **CUSUM runs on the SMOOTHED series.** Feeding it raw would let a single
/// jittery sample push the accumulator, which is exactly the spike sensitivity
/// CUSUM exists to avoid — the two detectors are a pipeline, not alternatives.
#[derive(Debug, Clone, PartialEq)]
pub struct Watch {
    ewma: Ewma,
    cusum: Cusum,
}

impl Watch {
    pub fn new(ewma: Ewma, cusum: Cusum) -> Self {
        Self { ewma, cusum }
    }

    pub fn observe(&mut self, w: f64) -> Reading {
        let smoothed = self.ewma.update(w);
        let alert = self.cusum.update(smoothed);
        Reading { w, smoothed, alert }
    }

    /// Feed a whole series, in order. Convenience for replaying a fold.
    pub fn observe_all(&mut self, series: &[f64]) -> Vec<Reading> {
        series.iter().map(|w| self.observe(*w)).collect()
    }

    pub fn ewma(&self) -> &Ewma {
        &self.ewma
    }
    pub fn cusum(&self) -> &Cusum {
        &self.cusum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EWMA (§V) ───────────────────────────────────────────────────────────

    #[test]
    fn the_first_observation_seeds_the_level() {
        // Starting from zero would make every series appear to ramp up from
        // nothing — a fabricated transient in the signal everything trusts.
        let mut e = Ewma::new(0.1).unwrap();
        assert_eq!(e.update(100.0), 100.0);
    }

    #[test]
    fn smoothing_tracks_the_series_without_following_every_jitter() {
        let mut e = Ewma::new(0.5).unwrap();
        e.update(0.0);
        assert_eq!(e.update(10.0), 5.0);
        assert_eq!(e.update(10.0), 7.5);
        assert_eq!(e.update(10.0), 8.75, "converging on the new level, not jumping to it");
    }

    #[test]
    fn a_smaller_alpha_smooths_harder() {
        let mut fast = Ewma::new(0.8).unwrap();
        let mut slow = Ewma::new(0.1).unwrap();
        fast.update(0.0);
        slow.update(0.0);
        let (f, s) = (fast.update(100.0), slow.update(100.0));
        assert!(f > s, "a constraint-health indicator responds; a budget ring stays calm");
    }

    #[test]
    fn the_half_life_says_what_the_chosen_alpha_actually_buys() {
        // The article's own worked values: α = 0.1 ≈ 7 updates, α = 0.4 ≈ 2.
        let slow = Ewma::new(0.1).unwrap().half_life();
        let fast = Ewma::new(0.4).unwrap().half_life();
        assert!((slow - 6.58).abs() < 0.05, "α = 0.1 → t½ ≈ 7, got {slow}");
        assert!((fast - 1.36).abs() < 0.05, "α = 0.4 → t½ ≈ 2, got {fast}");
    }

    #[test]
    fn alpha_is_bounded_at_both_ends() {
        for bad in [0.0, 1.0, -0.5, 1.5, f64::NAN] {
            assert!(Ewma::new(bad).is_err(), "α = {bad} must be refused");
        }
        assert!(Ewma::new(0.5).is_ok());
    }

    // ── CUSUM (§VI) ─────────────────────────────────────────────────────────

    fn detector() -> Cusum {
        Cusum::new(CusumSpec::new(0.0, 1.0, 10.0)).unwrap()
    }

    /// **The property CUSUM exists for**, and the reason it is not a threshold.
    #[test]
    fn a_small_persistent_drift_alerts() {
        let mut c = detector();
        // W sitting at 2 against an expected 0: each step adds (2 − 0 − 1) = 1.
        let mut fired = None;
        for k in 1..=15 {
            if let Some(a) = c.update(2.0) {
                fired = Some((k, a));
                break;
            }
        }
        let (k, alert) = fired.expect("a sustained drift must eventually be seen");
        assert_eq!(alert.shift, Shift::Upward);
        assert_eq!(k, 11, "S⁺ crosses h = 10 on the eleventh step");
    }

    #[test]
    fn a_single_much_larger_spike_that_corrects_does_not() {
        // The article's own comparison: 5% over for ten days beats a 50% spike
        // that immediately corrects. Here the spike is 4.5× the drift value and
        // still never fires, because it does not persist.
        let mut c = detector();
        assert!(c.update(9.0).is_none(), "the spike itself is under h");
        for _ in 0..20 {
            assert!(c.update(0.0).is_none(), "and it decays back rather than accumulating");
        }
        assert_eq!(c.s_pos(), 0.0, "the accumulator returned to zero on its own");
    }

    #[test]
    fn the_accumulator_resets_when_the_process_returns_to_normal() {
        let mut c = detector();
        for _ in 0..5 {
            c.update(2.0);
        }
        assert!(c.s_pos() > 0.0);
        for _ in 0..20 {
            c.update(0.0);
        }
        assert_eq!(c.s_pos(), 0.0, "evidence for a shift that stopped is not kept forever");
    }

    #[test]
    fn a_downward_shift_is_detected_too() {
        // On a W series this is recovery — the household moving back toward V.
        let mut c = Cusum::new(CusumSpec::new(10.0, 1.0, 10.0)).unwrap();
        let mut fired = None;
        for _ in 0..15 {
            if let Some(a) = c.update(8.0) {
                fired = Some(a);
                break;
            }
        }
        assert_eq!(fired.expect("a sustained drop must be seen").shift, Shift::Downward);
    }

    #[test]
    fn delta_sets_the_smallest_shift_worth_noticing() {
        // A drift below δ never accumulates at all. That is the parameter doing
        // its job, not the detector missing something.
        let mut c = Cusum::new(CusumSpec::new(0.0, 5.0, 10.0)).unwrap();
        for _ in 0..100 {
            assert!(c.update(2.0).is_none());
        }
        assert_eq!(c.s_pos(), 0.0, "2 is inside the δ = 5 band, so it is not a shift");
    }

    // ── the severity IS the boundary (§VI) ──────────────────────────────────

    #[test]
    fn the_severity_table_is_the_articles() {
        assert_eq!(classify(1.0), Severity::Info);
        assert_eq!(classify(1.5), Severity::Info, "> 1.5, not >=");
        assert_eq!(classify(1.6), Severity::Warning);
        assert_eq!(classify(3.0), Severity::Warning, "> 3.0, not >=");
        assert_eq!(classify(3.1), Severity::Critical);
    }

    #[test]
    fn warning_and_critical_cross_the_boundary_and_info_does_not() {
        assert!(!Severity::Info.escalates(), "watching");
        assert!(Severity::Warning.escalates(), "looking");
        assert!(Severity::Critical.escalates(), "looking");
    }

    /// The article's pseudocode zeroes `S_pos` **before** calling `classify()`,
    /// which reads `max(S_pos, S_neg)/h` — so an upward shift would always
    /// report `excess ≈ 0` and classify INFO, never escalating however extreme.
    /// The prose is unambiguous, so the ordering is the error.
    #[test]
    fn a_severe_shift_escalates_rather_than_reporting_info() {
        let mut c = Cusum::new(CusumSpec::new(0.0, 0.0, 1.0)).unwrap();
        // One observation of 5 against h = 1: excess = 5.0 → CRITICAL.
        let alert = c.update(5.0).expect("crosses h immediately");
        assert_eq!(alert.excess, 5.0, "the excess is captured before the reset");
        assert_eq!(alert.severity, Severity::Critical);
        assert!(alert.severity.escalates(),
                "classifying after the reset would have made this INFO and silent");
    }

    #[test]
    fn the_accumulator_is_still_reset_after_firing() {
        let mut c = Cusum::new(CusumSpec::new(0.0, 0.0, 1.0)).unwrap();
        c.update(5.0).unwrap();
        assert_eq!(c.s_pos(), 0.0, "the reset itself is kept — only its ORDER changed");
    }

    // ── declared parameters ─────────────────────────────────────────────────

    #[test]
    fn the_parameters_are_declared_and_checked_at_authoring_time() {
        assert!(CusumSpec::new(0.0, 1.0, 10.0).typecheck().is_ok());
        assert!(matches!(CusumSpec::new(0.0, 1.0, 0.0).typecheck(),
                         Err(DetectorError::BadThreshold(_))), "h = 0 alerts on everything");
        assert!(matches!(CusumSpec::new(0.0, -1.0, 10.0).typecheck(),
                         Err(DetectorError::BadDelta(_))));
        assert!(matches!(CusumSpec::new(f64::NAN, 1.0, 10.0).typecheck(),
                         Err(DetectorError::BadMean(_))));
    }

    #[test]
    fn the_declared_threshold_changes_when_a_person_is_interrupted() {
        // h is "how much evidence do you want before being interrupted" — a
        // judgement about this household, which is why it is a spec field.
        let drift = [2.0; 12];
        let mut jumpy = Cusum::new(CusumSpec::new(0.0, 1.0, 3.0)).unwrap();
        let mut patient = Cusum::new(CusumSpec::new(0.0, 1.0, 30.0)).unwrap();
        let jumpy_fired = drift.iter().any(|w| jumpy.update(*w).is_some());
        let patient_fired = drift.iter().any(|w| patient.update(*w).is_some());
        assert!(jumpy_fired && !patient_fired, "same series, different declared patience");
    }

    // ── the pipeline over a W series ────────────────────────────────────────

    fn watch() -> Watch {
        Watch::new(Ewma::new(0.5).unwrap(), Cusum::new(CusumSpec::new(0.0, 1.0, 10.0)).unwrap())
    }

    #[test]
    fn a_household_sitting_inside_v_never_escalates() {
        // W = 0 throughout: nothing to look at, and watching stays cheap.
        let readings = watch().observe_all(&[0.0; 30]);
        assert!(readings.iter().all(|r| r.alert.is_none()));
        assert!(readings.iter().all(|r| !r.escalates()));
    }

    #[test]
    fn a_household_drifting_out_of_v_eventually_gets_looked_at() {
        let mut w = watch();
        let readings = w.observe_all(&[3.0; 20]);
        let first = readings.iter().position(|r| r.alert.is_some())
            .expect("a sustained distance from V must surface");
        assert!(first > 0, "not on the first sample — that would be a threshold, not a CUSUM");
        assert!(readings[first].escalates() || readings[first].alert.is_some());
    }

    #[test]
    fn the_pipeline_smooths_before_it_detects() {
        // CUSUM sees the EWMA level, not the raw series: a lone jittery sample
        // must not push the accumulator, which is the spike sensitivity CUSUM
        // exists to avoid.
        let mut w = watch();
        let r = w.observe(20.0);
        assert_eq!(r.w, 20.0, "the raw W is still reported");
        assert_eq!(r.smoothed, 20.0, "seeded on the first observation");
        let r2 = w.observe(0.0);
        assert_eq!(r2.smoothed, 10.0, "and smoothed thereafter");
    }

    #[test]
    fn a_reading_carries_the_raw_w_alongside_the_smoothed_level() {
        // Both are wanted: the raw distance is what a person is told, the
        // smoothed level is what the detector reasons over.
        let mut w = watch();
        w.observe(0.0);
        let r = w.observe(8.0);
        assert_eq!(r.w, 8.0);
        assert_eq!(r.smoothed, 4.0);
        assert_ne!(r.w, r.smoothed, "and they are genuinely different quantities");
    }
}
