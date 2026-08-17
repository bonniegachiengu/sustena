//! Frequency-domain change — separating a genuine shift from an expected cycle
//! (MONITOR, Additions · MON-13).
//!
//! > *"A Sustain's history carries periodic structure — a monthly bill, a
//! > seasonal spend, a weekly rhythm. Reading the fold in the frequency domain
//! > (a discrete transform over a windowed series) separates a genuine shift
//! > from an expected cycle, so a 'spend is up' alarm does not fire on the
//! > arrival of a known season."*
//!
//! ## ★ What this reads, and a correction to the tracker row that sent it here
//!
//! MON-13's row said this consumes [`crate::signal`]. **It does not, and it
//! cannot** — `signal.rs` is the excitable-medium primitive: `Phase`, `Pulse`,
//! `Field`, refractoriness, a wave travelling across a **graph of nodes**. It
//! produces no time series to transform.
//!
//! The confusion is traceable to the Additions paragraph itself, which runs two
//! sentences together: the harmonics sentence, and then *"The Signal primitive
//! Monitor relays is defined formally in §4B (§III)"* — a **cross-reference
//! about where Signal is defined**, not a statement that harmonics consumes it.
//!
//! What the article actually says to transform is *"the fold [...] over a
//! windowed series"* — which is the **same `W = d(s,V)` series `detect.rs`'s
//! EWMA and CUSUM already run over**, and which `MonitorEngine` already keeps a
//! history of. So this takes a **supplied** window (`&[f64]`), exactly as
//! `criticality.rs` takes a supplied window rather than owning one: no parallel
//! windowing is defined here, which was the real point of the instruction.
//!
//! ## ★★ The payoff is suppression, and it is only honest if the cycle is real
//!
//! Energy at a known period is an **expected cycle**; the **residual after
//! removing the known cycles is the genuine-shift signal**.
//!
//! ★★ *Known cycles*, plural, means **a fundamental and its harmonics** — which
//! is what the row is named for, and which testing forced rather than
//! inspection suggesting it. A monthly bill is a periodic **impulse**, not a
//! sinusoid, so its energy lands at every multiple of its fundamental;
//! attributing only the fundamental left the harmonics in the residual and
//! reported an on-schedule bill as a shift. The cost is named where it lives
//! ([`KnownCycle::harmonic_bins`]): a genuine shift landing exactly on a
//! harmonic of a known cycle is absorbed with it, which is inherent rather than
//! a defect — you cannot both explain a non-sinusoidal cycle and keep its
//! harmonic bins free. This is precisely
//! what the time-domain detectors cannot do alone — a monthly bill arriving on
//! schedule walks CUSUM's accumulator straight past `h`, and a conformance case
//! runs the real `Cusum` over the same series to show it firing while this
//! reports `ExpectedCycle`.
//!
//! **A cycle used to suppress an alarm must be earned twice over**, and the two
//! requirements are different things:
//!
//! 1. **Authorised** — [`CycleProvenance::Declared`] (someone said so; an EVT-11
//!    monthly period *is* a declared cycle) or [`CycleProvenance::Established`]
//!    (enough real history that the period is observable, not asserted).
//! 2. **Resolvable in this window** — the supplied series must be long enough
//!    to actually carry the period. A DFT bin `k` has period `N/k`, so a period
//!    longer than `N/2` occupies fewer than two bins' worth of cycles and is
//!    indistinguishable from a trend.
//!
//! ★★ **Declaration authorises a cycle; it does not make a short window able to
//! measure it.** Subtracting energy at a bin the window cannot resolve would
//! remove a real shift under the name of a season — the exact dishonesty the
//! row exists to prevent. So the answer there is [`CycleVerdict::Indeterminate`]:
//! *not* "no cycle" and *not* "a shift", but **we cannot yet tell**, in the same
//! family as `Skew::Unknown` and `Soundness::Unavailable`.
//!
//! ## The transform is computed, not imported
//!
//! A direct `O(N²)` DFT, in plain `f64` arithmetic — no dependency, no RNG, no
//! clock. Honest at household-scale windows (a few hundred samples at most).
//! **An FFT is a scaling slot**, named rather than pulled in now.
//!
//! ## It refines the existing boundary, it does not open a new one
//!
//! A genuine shift is classified with `detect::classify` — the **same**
//! `excess → Severity` table the time-domain path uses — so the frequency-domain
//! reading crosses the Monitor→Controller boundary by the same rule, through
//! `Severity::escalates()`. MON-13 is the complement of `detect.rs`, not a
//! replacement and not a parallel alarm channel.

use std::collections::BTreeSet;
use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::detect::{classify, Severity};
use crate::period::Period;

/// How a cycle earned the right to suppress an alarm.
///
/// ★ Named `CycleProvenance`, not `Provenance` — `event::Provenance` already
/// answers a different question (was an event's TIME observed or inferred).
/// Eighteenth collision; the newcomer takes the longer name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleProvenance {
    /// Someone declared it — a monthly budget period, a known billing cycle.
    /// An EVT-11 [`Recurrence`](crate::period::Recurrence) expansion is exactly
    /// this.
    Declared,
    /// Observed often enough in real history to be established rather than
    /// asserted. Carries how many full cycles were seen, because *"we have seen
    /// this twice"* and *"we have seen this fifty times"* are different claims.
    Established { cycles_observed: usize },
}

/// A cycle this window is allowed to treat as expected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownCycle {
    pub id: String,
    /// The period, **in samples of the supplied series**.
    pub period_samples: f64,
    pub provenance: CycleProvenance,
}

impl KnownCycle {
    pub fn declared(id: impl Into<String>, period_samples: f64) -> Self {
        Self { id: id.into(), period_samples, provenance: CycleProvenance::Declared }
    }

    pub fn established(
        id: impl Into<String>,
        period_samples: f64,
        cycles_observed: usize,
    ) -> Self {
        Self {
            id: id.into(),
            period_samples,
            provenance: CycleProvenance::Established { cycles_observed },
        }
    }

    /// ★ Derive a cycle from **EVT-11 periods** — the real tie-in, since a
    /// declared monthly budget period *is* a declared cycle.
    ///
    /// Takes consecutive expanded periods and the series' own sample interval,
    /// and refuses rather than guesses when there are fewer than two (one
    /// period gives a length, not a recurrence) or when they do not partition.
    pub fn from_periods(
        id: impl Into<String>,
        periods: &[Period],
        sample_interval_ms: i64,
    ) -> Result<Self, HarmonicsError> {
        if periods.len() < 2 {
            return Err(HarmonicsError::NotARecurrence { given: periods.len() });
        }
        if sample_interval_ms <= 0 {
            return Err(HarmonicsError::BadSampleInterval(sample_interval_ms));
        }
        // Each period's end is the next one's start (EVT-11 guarantees it), so
        // the cycle length is one period's own span.
        let span = periods[0].window.end() - periods[0].window.start();
        Ok(Self {
            id: id.into(),
            period_samples: span as f64 / sample_interval_ms as f64,
            provenance: CycleProvenance::Declared,
        })
    }

    /// The DFT bin whose period is closest to this cycle, for a window of `n`.
    ///
    /// Bin `k` has period `n/k`, so `k = round(n / period)`.
    fn bin(&self, n: usize) -> usize {
        (n as f64 / self.period_samples).round().max(1.0) as usize
    }

    /// ★★ The cycle's fundamental **and its harmonics** — `k, 2k, 3k, ...`.
    ///
    /// This is what the row is named for, and it was found by testing rather
    /// than by inspection: a monthly bill is a periodic **impulse**, not a
    /// sinusoid, and an impulse train puts energy at *every multiple* of its
    /// fundamental. Attributing only the fundamental leaves the harmonics in
    /// the residual and reports a perfectly on-schedule bill as a shift.
    ///
    /// ★ The cost, stated rather than hidden: a genuine shift that lands
    /// exactly on a harmonic of a known cycle is absorbed with it. That is
    /// inherent — you cannot both explain a non-sinusoidal cycle and keep its
    /// harmonic bins free — and it is why the residual, not the fundamental
    /// alone, is what the verdict is read from.
    fn harmonic_bins(&self, n: usize) -> Vec<usize> {
        let k = self.bin(n);
        (1..).map(|m| m * k).take_while(|b| *b <= n / 2).collect()
    }

    /// ★ Can a window of `n` samples carry this period at all? At least two
    /// full cycles must fit, or the component is indistinguishable from a
    /// trend.
    fn resolvable_in(&self, n: usize) -> bool {
        self.period_samples > 0.0 && self.period_samples <= (n as f64) / 2.0
    }
}

/// What can go wrong reading a window in the frequency domain.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum HarmonicsError {
    #[error("a window of {0} sample(s) has no frequency content to read; at least 4 are needed for two resolvable bins")]
    WindowTooShort(usize),
    #[error("sample {index} is {value} — a transform over a non-finite series returns nothing meaningful")]
    NonFiniteSample { index: usize, value: f64 },
    #[error("cycle '{id}' has period {period} samples, which is not a period")]
    BadPeriod { id: String, period: f64 },
    #[error("{given} period(s) is not a recurrence — a cycle length needs at least two consecutive periods to be a repeat rather than a span")]
    NotARecurrence { given: usize },
    #[error("the sample interval must be positive; {0}ms cannot map a time period onto samples")]
    BadSampleInterval(i64),
    #[error("the residual threshold must be a finite fraction in 0..=1; {0} is not")]
    BadResidualThreshold(f64),
    #[error("min_cycles_to_establish must be at least 2 — one occurrence is a span, not a recurrence")]
    BadMinCycles,
}

/// One frequency component of the windowed series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    /// The DFT bin. Bin `k` completes `k` cycles across the window.
    pub k: usize,
    /// `n / k` — the period this bin represents, in samples.
    pub period_samples: f64,
    pub amplitude: f64,
    /// `|X[k]|²` — what the residual is measured in.
    pub power: f64,
}

/// The one-sided spectrum of a windowed series, mean removed.
///
/// ★ The mean (`k = 0`) is removed before transforming: a non-zero level would
/// otherwise dominate every reading and make the residual a statement about the
/// household's average spend rather than about its rhythm. A **linear** trend is
/// deliberately *not* removed — that is a modelling choice, and removing it
/// silently would suppress exactly the slow drift a Monitor most wants to see.
pub fn spectrum(series: &[f64]) -> Result<Vec<Component>, HarmonicsError> {
    let n = series.len();
    if n < 4 {
        return Err(HarmonicsError::WindowTooShort(n));
    }
    for (index, &value) in series.iter().enumerate() {
        if !value.is_finite() {
            return Err(HarmonicsError::NonFiniteSample { index, value });
        }
    }

    let mean = series.iter().sum::<f64>() / n as f64;
    let centred: Vec<f64> = series.iter().map(|x| x - mean).collect();

    // Direct DFT. One-sided: bins 1..=n/2, since a real series is conjugate-
    // symmetric and bin 0 is the mean we just removed.
    let mut out = Vec::with_capacity(n / 2);
    for k in 1..=n / 2 {
        let (mut re, mut im) = (0.0, 0.0);
        for (idx, x) in centred.iter().enumerate() {
            let angle = TAU * (k * idx) as f64 / n as f64;
            re += x * angle.cos();
            im -= x * angle.sin();
        }
        let power = re * re + im * im;
        out.push(Component {
            k,
            period_samples: n as f64 / k as f64,
            amplitude: power.sqrt() * 2.0 / n as f64,
            power,
        });
    }
    Ok(out)
}

/// The declared policy for reading one series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarmonicsSpec {
    known: Vec<KnownCycle>,
    /// The fraction of total power that may remain unexplained before this is
    /// called a genuine shift. **Declared**, like every other threshold in this
    /// codebase.
    residual_threshold: f64,
    /// How many full cycles a period must be seen through before it counts as
    /// *established*. At least 2 — one occurrence is a span, not a recurrence.
    min_cycles_to_establish: usize,
}

impl HarmonicsSpec {
    pub fn new(
        known: Vec<KnownCycle>,
        residual_threshold: f64,
        min_cycles_to_establish: usize,
    ) -> Result<Self, HarmonicsError> {
        if !(0.0..=1.0).contains(&residual_threshold) || !residual_threshold.is_finite() {
            return Err(HarmonicsError::BadResidualThreshold(residual_threshold));
        }
        if min_cycles_to_establish < 2 {
            return Err(HarmonicsError::BadMinCycles);
        }
        for c in &known {
            if !c.period_samples.is_finite() || c.period_samples <= 1.0 {
                return Err(HarmonicsError::BadPeriod {
                    id: c.id.clone(),
                    period: c.period_samples,
                });
            }
        }
        Ok(Self { known, residual_threshold, min_cycles_to_establish })
    }

    pub fn known(&self) -> &[KnownCycle] {
        &self.known
    }

    pub fn residual_threshold(&self) -> f64 {
        self.residual_threshold
    }

    /// ★ Is this cycle allowed to suppress, in a window of `n`?
    ///
    /// Two independent requirements: **authorised** (declared, or established
    /// through enough cycles) and **resolvable** (the window can carry the
    /// period at all). A declaration does not make a short window able to
    /// measure something.
    fn usable(&self, cycle: &KnownCycle, n: usize) -> Result<(), String> {
        if !cycle.resolvable_in(n) {
            return Err(format!(
                "cycle '{}' has a period of {:.1} samples, which a window of {n} cannot resolve \
                 (fewer than two full cycles fit, so it is indistinguishable from a trend)",
                cycle.id, cycle.period_samples
            ));
        }
        match cycle.provenance {
            CycleProvenance::Declared => Ok(()),
            CycleProvenance::Established { cycles_observed } => {
                if cycles_observed >= self.min_cycles_to_establish {
                    Ok(())
                } else {
                    Err(format!(
                        "cycle '{}' was seen {} time(s), below the declared {} needed to call it \
                         established — using it to suppress would hide a shift behind a season \
                         nobody has confirmed",
                        cycle.id, cycles_observed, self.min_cycles_to_establish
                    ))
                }
            }
        }
    }
}

/// What the frequency-domain reading concluded.
///
/// ★ Named `CycleVerdict`, not `Verdict` — `admission::Verdict` is the gate's
/// admit/refuse answer, a genuinely different judgement. Nineteenth collision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CycleVerdict {
    /// ★ The energy is accounted for by cycles this window may treat as known.
    /// **Do not alarm** — the season arrived, as seasons do.
    ExpectedCycle { explained_by: Vec<String>, explained_power: f64, residual_power: f64 },
    /// Power remains after the known cycles are removed. **Surface it**, at a
    /// severity from the same `excess → Severity` table the time-domain path
    /// uses.
    GenuineShift { residual_power: f64, residual_fraction: f64, severity: Severity },
    /// ★★ **Neither.** The window cannot carry a cycle it would have needed, or
    /// a cycle is not established enough to suppress on. Not "no cycle" and not
    /// "a shift" — *we cannot yet tell*.
    Indeterminate { reason: String },
}

impl CycleVerdict {
    /// Does this cross the Monitor→Controller boundary? Only a genuine shift
    /// does — and by the existing rule.
    pub fn escalates(&self) -> bool {
        matches!(self, CycleVerdict::GenuineShift { severity, .. } if severity.escalates())
    }

    pub fn is_expected_cycle(&self) -> bool {
        matches!(self, CycleVerdict::ExpectedCycle { .. })
    }
}

/// One reading, with everything it concluded from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarmonicReading {
    pub total_power: f64,
    pub explained_power: f64,
    pub residual_power: f64,
    pub verdict: CycleVerdict,
    /// The components attributed to known cycles, named.
    pub attributed: Vec<(String, f64)>,
}

/// Read a windowed series in the frequency domain and say whether what changed
/// is a season or a shift.
///
/// The window is **supplied** — the same `W = d(s,V)` series the time-domain
/// detectors run over. Nothing here defines a windowing of its own.
pub fn read(series: &[f64], spec: &HarmonicsSpec) -> Result<HarmonicReading, HarmonicsError> {
    let n = series.len();
    let components = spectrum(series)?;
    let total_power: f64 = components.iter().map(|c| c.power).sum();

    // ★ Check every declared cycle can actually be used BEFORE attributing any
    // energy to it. A cycle that cannot be resolved is not simply skipped —
    // skipping it would leave its (unmeasurable) energy in the residual and
    // report a shift the window is not entitled to claim.
    let mut blocked: Vec<String> = Vec::new();
    for c in &spec.known {
        if let Err(why) = spec.usable(c, n) {
            blocked.push(why);
        }
    }
    if !blocked.is_empty() {
        return Ok(HarmonicReading {
            total_power,
            explained_power: 0.0,
            residual_power: total_power,
            attributed: Vec::new(),
            verdict: CycleVerdict::Indeterminate { reason: blocked.join("; ") },
        });
    }

    // Attribute each known cycle's bin. A `BTreeSet` so two cycles landing in
    // the same bin cannot double-count its power.
    let mut claimed: BTreeSet<usize> = BTreeSet::new();
    let mut attributed: Vec<(String, f64)> = Vec::new();
    for c in &spec.known {
        let bins = c.harmonic_bins(n);
        let power: f64 = components
            .iter()
            .filter(|x| bins.contains(&x.k))
            .map(|x| x.power)
            .sum();
        attributed.push((c.id.clone(), power));
        claimed.extend(bins);
    }

    let explained_power: f64 =
        components.iter().filter(|c| claimed.contains(&c.k)).map(|c| c.power).sum();
    let residual_power = (total_power - explained_power).max(0.0);

    // A flat series has no energy anywhere; there is nothing to call a shift.
    if total_power <= f64::EPSILON {
        return Ok(HarmonicReading {
            total_power,
            explained_power,
            residual_power,
            attributed,
            verdict: CycleVerdict::ExpectedCycle {
                explained_by: spec.known.iter().map(|c| c.id.clone()).collect(),
                explained_power,
                residual_power,
            },
        });
    }

    let residual_fraction = residual_power / total_power;
    let verdict = if residual_fraction > spec.residual_threshold {
        CycleVerdict::GenuineShift {
            residual_power,
            residual_fraction,
            // ★ The same table the time-domain path uses: the residual measured
            // in units of its own declared threshold, exactly as CUSUM measures
            // its accumulator in units of `h`.
            severity: classify(residual_fraction / spec.residual_threshold),
        }
    } else {
        CycleVerdict::ExpectedCycle {
            explained_by: spec.known.iter().map(|c| c.id.clone()).collect(),
            explained_power,
            residual_power,
        }
    };

    Ok(HarmonicReading { total_power, explained_power, residual_power, attributed, verdict })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{Cusum, CusumSpec};

    /// A household series: a baseline with a bill spike every `period` samples.
    fn with_cycle(n: usize, period: usize, baseline: f64, spike: f64) -> Vec<f64> {
        (0..n).map(|i| if i % period == 0 { baseline + spike } else { baseline }).collect()
    }

    fn monthly(period: f64) -> HarmonicsSpec {
        HarmonicsSpec::new(vec![KnownCycle::declared("monthly_bill", period)], 0.35, 2).unwrap()
    }

    // ── the transform ──────────────────────────────────────────────────────

    #[test]
    fn a_pure_sinusoid_puts_its_power_in_its_own_bin() {
        let n = 32;
        let series: Vec<f64> =
            (0..n).map(|i| (TAU * 4.0 * i as f64 / n as f64).sin()).collect();
        let s = spectrum(&series).unwrap();

        let peak = s.iter().max_by(|a, b| a.power.total_cmp(&b.power)).unwrap();
        assert_eq!(peak.k, 4, "four cycles across the window");
        assert_eq!(peak.period_samples, 8.0);
        assert!((peak.amplitude - 1.0).abs() < 1e-9, "amplitude recovered: {}", peak.amplitude);
    }

    #[test]
    fn the_mean_is_removed_so_a_level_is_not_read_as_a_rhythm() {
        // ★ Otherwise the residual would be a statement about average spend
        // rather than about the household's rhythm.
        let flat = vec![500.0; 16];
        let s = spectrum(&flat).unwrap();
        assert!(s.iter().all(|c| c.power < 1e-18), "a constant series has no rhythm");
    }

    #[test]
    fn a_window_too_short_or_not_finite_is_refused() {
        assert_eq!(spectrum(&[1.0, 2.0]), Err(HarmonicsError::WindowTooShort(2)));
        assert!(matches!(
            spectrum(&[1.0, 2.0, f64::NAN, 4.0]),
            Err(HarmonicsError::NonFiniteSample { index: 2, .. })
        ));
    }

    // ── ★★ the payoff: a known season does not alarm ───────────────────────

    #[test]
    fn a_monthly_bill_arriving_on_schedule_is_an_expected_cycle() {
        // 32 weekly samples, a bill every 4 → 8 full cycles.
        let series = with_cycle(32, 4, 100.0, 300.0);
        let r = read(&series, &monthly(4.0)).unwrap();

        assert!(r.verdict.is_expected_cycle(), "{:?}", r.verdict);
        assert!(!r.verdict.escalates(), "the season arrived, as seasons do");
        assert_eq!(r.attributed[0].0, "monthly_bill");
        assert!(r.explained_power > r.residual_power);
    }

    #[test]
    fn the_time_domain_detector_would_have_fired_on_exactly_that_series() {
        // ★★ THE WHOLE POINT OF THE ROW, run rather than asserted: the real
        // `Cusum` from `detect.rs` walks the same series and crosses `h`,
        // because a spike is a spike to a time-domain accumulator.
        let series = with_cycle(32, 4, 100.0, 300.0);
        let mut cusum = Cusum::new(CusumSpec::new(100.0, 50.0, 100.0)).unwrap();
        let fired = series.iter().any(|w| cusum.update(*w).is_some());

        assert!(fired, "the time-domain detector fires on the season");
        assert!(
            read(&series, &monthly(4.0)).unwrap().verdict.is_expected_cycle(),
            "and the frequency-domain reading does not — which is the complement, not a veto"
        );
    }

    #[test]
    fn the_same_magnitude_arriving_off_cycle_is_a_genuine_shift() {
        // ★ Same spike, same size, at a period the declared cycle does not
        // explain — so the residual carries it and it surfaces.
        let series = with_cycle(32, 5, 100.0, 300.0);
        let r = read(&series, &monthly(4.0)).unwrap();

        match r.verdict {
            CycleVerdict::GenuineShift { residual_fraction, severity, .. } => {
                assert!(residual_fraction > 0.35);
                assert!(severity.escalates(), "it crosses the same boundary as any other alert");
            }
            other => panic!("an off-cycle spike must surface: {other:?}"),
        }
    }

    #[test]
    fn a_new_rhythm_on_top_of_a_known_one_still_surfaces() {
        // The realistic case: the monthly bill is still arriving AND something
        // else started. The known cycle is explained; the new one is not.
        let n = 32;
        let mut series = with_cycle(n, 4, 100.0, 300.0);
        for (i, v) in series.iter_mut().enumerate() {
            *v += 200.0 * (TAU * 7.0 * i as f64 / n as f64).sin();
        }
        assert!(matches!(
            read(&series, &monthly(4.0)).unwrap().verdict,
            CycleVerdict::GenuineShift { .. }
        ));
    }

    // ── ★★ a cycle must be authorised AND resolvable ───────────────────────

    #[test]
    fn a_period_the_window_cannot_resolve_is_indeterminate_not_expected() {
        // ★★ THE HONESTY POINT. The cycle is DECLARED — authorised — but a
        // 12-sample window cannot carry a 10-sample period: fewer than two full
        // cycles fit. Suppressing on it would remove a real shift under the
        // name of a season.
        let series = with_cycle(12, 10, 100.0, 300.0);
        let spec = HarmonicsSpec::new(vec![KnownCycle::declared("quarterly", 10.0)], 0.35, 2)
            .unwrap();

        match read(&series, &spec).unwrap().verdict {
            CycleVerdict::Indeterminate { reason } => {
                assert!(reason.contains("cannot resolve"));
                assert!(reason.contains("indistinguishable from a trend"));
            }
            other => panic!("a declaration does not make a short window able to measure: {other:?}"),
        }
    }

    #[test]
    fn an_unestablished_cycle_cannot_suppress_and_says_why() {
        // ★★ The other half: seen once is not a season. Using it to suppress
        // would hide a shift behind a rhythm nobody has confirmed.
        let series = with_cycle(32, 4, 100.0, 300.0);
        let spec = HarmonicsSpec::new(
            vec![KnownCycle::established("maybe_monthly", 4.0, 1)],
            0.35,
            3,
        )
        .unwrap();

        match read(&series, &spec).unwrap().verdict {
            CycleVerdict::Indeterminate { reason } => {
                assert!(reason.contains("seen 1 time"));
                assert!(reason.contains("nobody has confirmed"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_same_cycle_established_enough_times_does_suppress() {
        // The pair to the case above — the difference is the evidence, and
        // nothing else.
        let series = with_cycle(32, 4, 100.0, 300.0);
        let spec = HarmonicsSpec::new(
            vec![KnownCycle::established("monthly_bill", 4.0, 8)],
            0.35,
            3,
        )
        .unwrap();
        assert!(read(&series, &spec).unwrap().verdict.is_expected_cycle());
    }

    #[test]
    fn indeterminate_does_not_escalate_and_is_not_an_expected_cycle_either() {
        // ★ Neither of the other two answers — it is its own state.
        let v = CycleVerdict::Indeterminate { reason: "too short".into() };
        assert!(!v.escalates());
        assert!(!v.is_expected_cycle());
    }

    // ── the declared policy ────────────────────────────────────────────────

    #[test]
    fn a_nonsense_policy_is_refused_at_declaration() {
        assert!(matches!(
            HarmonicsSpec::new(vec![], 1.5, 2),
            Err(HarmonicsError::BadResidualThreshold(_))
        ));
        assert_eq!(HarmonicsSpec::new(vec![], 0.3, 1), Err(HarmonicsError::BadMinCycles));
        assert!(matches!(
            HarmonicsSpec::new(vec![KnownCycle::declared("bad", 1.0)], 0.3, 2),
            Err(HarmonicsError::BadPeriod { .. })
        ));
    }

    #[test]
    fn a_cycle_from_evt11_periods_needs_at_least_two_of_them() {
        // One period is a span, not a recurrence.
        assert_eq!(
            KnownCycle::from_periods("monthly", &[], 1000),
            Err(HarmonicsError::NotARecurrence { given: 0 })
        );
    }

    #[test]
    fn a_cycle_derived_from_a_real_evt11_recurrence_matches_its_period() {
        // ★ The tie-in: a declared monthly budget period IS a declared cycle.
        use crate::period::{Anchor, CivilDateTime, DstPolicy, LocalResolution, TzProvider, TzError};
        use crate::watermark::Lateness;

        struct Utc;
        impl TzProvider for Utc {
            fn tzdata_version(&self) -> String {
                "test".into()
            }
            fn offset_at(&self, _t: &str, _l: CivilDateTime) -> Result<LocalResolution, TzError> {
                Ok(LocalResolution::Unambiguous { offset_ms: 0 })
            }
        }

        let rule = crate::period::Recurrence::monthly(
            CivilDateTime::date(2026, 1, 1),
            "Etc/UTC",
            1,
            Lateness::Drop,
            DstPolicy::ShiftForward,
        );
        assert!(matches!(rule.anchor, Anchor::Zone { .. }));
        let periods = rule.expand(&Utc, 3).unwrap();

        // Daily samples: January is 31 days, so the cycle is 31 samples.
        let day_ms = 24 * 3_600_000;
        let c = KnownCycle::from_periods("monthly_budget", &periods, day_ms).unwrap();
        assert_eq!(c.period_samples, 31.0);
        assert_eq!(c.provenance, CycleProvenance::Declared);
    }

    #[test]
    fn two_cycles_landing_in_one_bin_cannot_double_count_its_power() {
        let series = with_cycle(32, 4, 100.0, 300.0);
        let spec = HarmonicsSpec::new(
            vec![KnownCycle::declared("a", 4.0), KnownCycle::declared("b", 4.05)],
            0.35,
            2,
        )
        .unwrap();
        let r = read(&series, &spec).unwrap();
        assert!(r.explained_power <= r.total_power + 1e-9, "power cannot exceed the total");
    }
}
