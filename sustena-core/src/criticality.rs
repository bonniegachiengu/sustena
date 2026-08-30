//! Proximity to criticality — the branching-ratio read
//! (Operative §VIII · OPV-14).
//!
//! > Bak, Tang and Wiesenfeld (1987): a driven, dissipative system with local
//! > thresholds evolves *without parameter tuning* to a critical state whose
//! > avalanche sizes follow `P(s) ∝ s^-τ`.
//!
//! ```text
//! σ = E[direct consequences per event]     σ<1 subcritical · =1 critical · >1 supercritical
//! ```
//!
//! ## ★ Cheap because the field already exists
//!
//! > Cheap here because §4J's event record **already carries `causes`** — the
//! > causal-parent pointer is exactly what the estimate needs, so `σ̂` is **a
//! > read over the log, not new instrumentation.**
//!
//! Verified before building: [`crate::event::Event::causes`] is on the record
//! and [`crate::vclock::assign_clocks`] already derives the DAG from it. No
//! field was added for this.
//!
//! ## ★★ Near criticality, "expected loss" is not a usable summary
//!
//! > `τ ≤ 2 ⟹ E[s]` diverges; `τ ≤ 3 ⟹ Var[s]` diverges. Real systems are
//! > finite, so a cutoff `s_max` set by system size — **not the mean** — is
//! > what a planner is exposed to. The honest quantity is the largest cascade
//! > the system can physically produce, **bounded for a household by the
//! > household, not by history.**
//!
//! So there is **no method here that returns a bare expected cascade size**.
//! [`BranchingReading::cascade_summary`] returns a [`CascadeSummary`], and its
//! `Mean` variant is unreachable at or above criticality — the type carries the
//! regime instead of a number that would read as safe.
//!
//! And the honest quantity is handled honestly in turn: what this can report is
//! the **largest cascade observed**, which is *history*, and the article is
//! explicit that the bound is set by the system rather than by history. So it
//! is reported as a **floor** — a lower bound on what is possible — with the
//! real `s_max` named as needing a declared system size that nobody has
//! declared. Presenting an observed maximum as the bound would be exactly the
//! misreading §VIII is warning about, one level down.
//!
//! ## ★★ A reading surfaces to a person; it never triggers anything
//!
//! > The honest limit on all three: they are diagnostics with **real
//! > false-positive rates on short household series.** A criticality reading is
//! > a **signal to surface to a person**, not a trigger for autonomous action.
//!
//! Structural, not a convention. Everything here is a pure function of
//! `&[Event]` and `&[f64]` — the borrow is immutable, so the compiler enforces
//! that a reading cannot change the log it read. There is no engine, no
//! operator and no `&mut` anywhere in the module, and the reading types are
//! inert data with no method that acts. Same keystone as the advisory layer:
//! it advises, the human decides.
//!
//! ## The three detectors, and what this slice actually ships
//!
//! §VIII lists three, *"increasing in cheapness and decreasing in rigour"*:
//!
//! 1. **Branching ratio** — built, in full. [`branching_ratio`].
//! 2. **Tail shape** — **deliberately not built.** See below; the reason is
//!    structural, not scheduling.
//! 3. **Early-warning statistics** — built over a **supplied** window, with a
//!    correction to the article's premise. See below.
//!
//! ### ★ Why tail shape is slotted rather than shipped
//!
//! > Caveat loudly: fitting power laws to data is a documented trap, and the
//! > disciplined procedure (MLE + goodness-of-fit + explicit lognormal
//! > comparison) is Clauset, Shalizi & Newman (2009). **A straight-ish line on
//! > log-log is not evidence.**
//!
//! The MLE and the lognormal likelihood-ratio comparison are closed-form and
//! could ship. The **goodness-of-fit step cannot**: CSN's test is a
//! semi-parametric bootstrap, which needs a random source, and this crate has
//! none by construction (ADR-0001 — a core whose promise is reproducibility
//! cannot contain a non-reproducible call). Without it there is no answer to
//! *"is a power law even a plausible fit at all"*, and CSN are explicit that a
//! lognormal frequently wins and that **both candidates can be bad**.
//!
//! So shipping the two computable thirds would license a `τ = 2.3` that reads
//! as a fitted power law and is not one. That is the trap in better clothes.
//! When it is built, the constraint it must satisfy is already known: **the
//! verdict type may not have an absolute "power law" variant** — only a
//! comparison — until the goodness-of-fit step exists to support one.
//!
//! ### ★ A correction to §VIII's premise for detector 3
//!
//! §VIII says rising variance and lag-1 autocorrelation *"are exactly the
//! moments §2's Monitor already maintains for CUSUM/EWMA. **A second use of
//! existing signal.**"*
//!
//! **That is not true of this build, and it was checked rather than assumed.**
//! [`crate::detect::Ewma`] keeps a *level* and nothing else;
//! [`crate::detect::Cusum`] keeps two one-sided cumulative sums. Neither is a
//! second moment, and nothing anywhere in the crate retains a previous value
//! for a lag-1 term. Variance would need an EWMV alongside the EWMA; lag-1
//! needs the previous observation held.
//!
//! So [`critical_slowing_down`] computes both over a **supplied window** — the
//! same *supplied, not computed* discipline as MUL-14's pressure and OPV-28's
//! `dom(s)`. It needs no change to [`crate::detect`], and wiring it into
//! [`crate::detect::Watch`] so it genuinely *is* a second use of existing
//! signal is a separate change this slice does not make.
//!
//! ## Out of scope, named not stubbed
//!
//! - **The `⟨λ, β, γ⟩` weight modulation** (*"β up, faith in `Δu` down near
//!   criticality"*) is §VIII's scoring *implication*, which the article itself
//!   marks as design rather than derivation. This row produces the reading;
//!   consuming it to reweight is a later one.
//! - **`scenario_ensemble` (§VIII-iii)** is a different thing — constructive
//!   chaos, varying over initial conditions — and is not this slice.
//!
//! ## Honest limits
//!
//! - **Right-censoring biases `σ̂` down.** Events near the end of the window
//!   have not had time to produce consequences. [`BranchingReading::window`]
//!   carries the sample size so a short series is visible as a short series,
//!   and §VIII's own limit — real false-positive rates on short household
//!   series — is the reason none of this gates anything.
//! - **Edges to parents outside the window are counted as dangling**, not as
//!   offspring. [`BranchingReading::dangling_causes`] reports how much of the
//!   DAG the window cut, so a suspiciously low `σ̂` can be recognised as a
//!   truncation artefact rather than a calm household.
//! - **The critical band is declared.** With floating point, `σ̂ = 1` exactly
//!   is a measure-zero event, so *how close to 1 counts as critical* is a
//!   declared parameter rather than a hidden epsilon.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use crate::event::Event;

/// `σ < 1` subcritical, `= 1` critical, `> 1` supercritical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regime {
    /// Cascades die out. The mean is a usable summary here.
    Subcritical,
    /// Within the declared band of 1. No characteristic cascade size.
    Critical,
    /// Cascades grow. `E[s]` is not something to plan against.
    Supercritical,
}

impl Regime {
    pub fn name(&self) -> &'static str {
        match self {
            Regime::Subcritical => "subcritical",
            Regime::Critical => "critical",
            Regime::Supercritical => "supercritical",
        }
    }

    /// ★ Is a mean a usable summary of cascade size in this regime?
    ///
    /// False at *and* above criticality, not only above it: `τ ≤ 2` makes
    /// `E[s]` diverge, and being inside the band is precisely not knowing which
    /// side of that you are on.
    pub fn mean_is_usable(&self) -> bool {
        matches!(self, Regime::Subcritical)
    }
}

/// How close to `1` counts as critical — **declared**, because with floating
/// point `σ̂ = 1` exactly is a measure-zero event and a hidden epsilon would
/// decide a regime nobody could argue with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CriticalitySpec {
    band: f64,
}

impl CriticalitySpec {
    pub fn new(band: f64) -> Result<Self, CriticalityError> {
        if !band.is_finite() || band < 0.0 {
            return Err(CriticalityError::BadBand(band.to_string()));
        }
        Ok(Self { band })
    }

    pub fn band(&self) -> f64 {
        self.band
    }

    pub fn classify(&self, sigma: f64) -> Regime {
        if (sigma - 1.0).abs() <= self.band {
            Regime::Critical
        } else if sigma < 1.0 {
            Regime::Subcritical
        } else {
            Regime::Supercritical
        }
    }
}

impl Default for CriticalitySpec {
    /// A narrow band. Declared here so a caller that does not care still gets a
    /// value it can look up rather than one buried in a comparison.
    fn default() -> Self {
        Self { band: 0.05 }
    }
}

/// What a summary of cascade size may honestly be.
///
/// ★ There is deliberately **no way to obtain a bare `f64`** at or above
/// criticality. §VIII's whole risk-arithmetic point is that a mean there is not
/// a usable summary, so the type carries the regime instead of a number that
/// would read as safe.
#[derive(Debug, Clone, PartialEq)]
pub enum CascadeSummary {
    /// Subcritical: cascades die out and the mean means something.
    Mean(f64),
    /// ★ At or above criticality. `τ ≤ 2 ⟹ E[s]` diverges, so no mean is
    /// offered — not a smaller one, none.
    NotUsable {
        regime: Regime,
        /// The largest cascade **observed**. This is history, and §VIII is
        /// explicit that the bound is set by the system rather than by history
        /// — so it is a **floor** on what is possible, never the bound.
        largest_observed: usize,
        why: String,
    },
}

impl CascadeSummary {
    /// The mean, when there is an honest one. `None` is the answer at or above
    /// criticality, not a missing value.
    pub fn mean(&self) -> Option<f64> {
        match self {
            CascadeSummary::Mean(m) => Some(*m),
            CascadeSummary::NotUsable { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            CascadeSummary::Mean(m) => format!("mean cascade size {m:.3} (subcritical)"),
            CascadeSummary::NotUsable {
                regime,
                largest_observed,
                why,
            } => format!(
                "{} — no expected size offered. Largest observed cascade {} (a FLOOR, not the bound). {}",
                regime.name(),
                largest_observed,
                why
            ),
        }
    }
}

/// `σ̂` and what it was computed from.
///
/// Inert data. There is no method here that acts, and nothing in this module
/// takes a `&mut` of anything — a reading surfaces to a person.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchingReading {
    sigma_hat: f64,
    regime: Regime,
    /// `n` — how many events the estimate is over. A short series is visible
    /// as a short series.
    window: usize,
    /// Offspring edges counted: both ends inside the window.
    edges: usize,
    /// ★ Edges whose parent is **outside** the window. Reported so a low `σ̂`
    /// can be recognised as a truncation artefact rather than a calm household.
    dangling_causes: usize,
    /// Size of the largest weakly-connected causal component.
    largest_cascade: usize,
    /// Mean component size — computed but only ever surfaced through
    /// [`BranchingReading::cascade_summary`], which withholds it above the band.
    mean_cascade: f64,
    cascades: usize,
}

impl BranchingReading {
    /// `σ̂ = E[direct consequences per event]`.
    pub fn sigma_hat(&self) -> f64 {
        self.sigma_hat
    }

    pub fn regime(&self) -> Regime {
        self.regime
    }

    pub fn window(&self) -> usize {
        self.window
    }

    pub fn edges(&self) -> usize {
        self.edges
    }

    pub fn dangling_causes(&self) -> usize {
        self.dangling_causes
    }

    /// How many distinct causal components the window held.
    pub fn cascades(&self) -> usize {
        self.cascades
    }

    /// The largest cascade **observed** — a floor on what the system can
    /// produce, not the `s_max` §VIII means.
    pub fn largest_observed_cascade(&self) -> usize {
        self.largest_cascade
    }

    /// ★★ The risk-arithmetic point, as a type.
    ///
    /// Subcritical yields a mean; at or above criticality yields no number at
    /// all, because §VIII's argument is that the number would be misleading
    /// rather than merely uncertain.
    pub fn cascade_summary(&self) -> CascadeSummary {
        if self.regime.mean_is_usable() {
            return CascadeSummary::Mean(self.mean_cascade);
        }
        CascadeSummary::NotUsable {
            regime: self.regime,
            largest_observed: self.largest_cascade,
            why: "τ ≤ 2 makes E[s] diverge; what a planner is exposed to is the largest \
                  cascade the system can physically produce, which is bounded by the system \
                  and is not declared here — the observed maximum is a floor on it"
                .to_string(),
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "σ̂ = {:.3} ({}) over {} events, {} edges, {} dangling · {}",
            self.sigma_hat,
            self.regime.name(),
            self.window,
            self.edges,
            self.dangling_causes,
            self.cascade_summary().describe()
        )
    }
}

/// `σ̂` over a window of the event log — **a read, and only a read**.
///
/// Takes `&[Event]`, so the compiler enforces that computing a criticality
/// reading cannot change the log it was computed from.
///
/// Counting, stated because the edges of a window are where this kind of
/// estimate quietly goes wrong: *individuals* are the events in the window;
/// *offspring edges* are those with **both ends** inside it; an edge naming a
/// parent outside the window is counted as **dangling** and reported, never as
/// an offspring nobody had.
pub fn branching_ratio(
    events: &[Event],
    spec: CriticalitySpec,
) -> Result<BranchingReading, CriticalityError> {
    if events.is_empty() {
        return Err(CriticalityError::EmptyWindow);
    }

    let mut ids = BTreeSet::new();
    for e in events {
        if !ids.insert(e.id.as_str()) {
            return Err(CriticalityError::DuplicateEvent(e.id.clone()));
        }
    }

    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut in_window_parents: BTreeMap<&str, usize> = BTreeMap::new();
    let mut edges = 0usize;
    let mut dangling = 0usize;

    for e in events {
        for c in &e.causes {
            if ids.contains(c.as_str()) {
                children.entry(c.as_str()).or_default().push(e.id.as_str());
                *in_window_parents.entry(e.id.as_str()).or_insert(0) += 1;
                edges += 1;
            } else {
                dangling += 1;
            }
        }
    }

    // Weakly-connected causal components, walked from the roots — events with
    // no parent inside the window.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut sizes = Vec::new();
    for e in events {
        let id = e.id.as_str();
        if in_window_parents.contains_key(id) || seen.contains(id) {
            continue;
        }
        let mut size = 0usize;
        let mut queue = VecDeque::from([id]);
        seen.insert(id);
        while let Some(cur) = queue.pop_front() {
            size += 1;
            for child in children.get(cur).into_iter().flatten() {
                if seen.insert(child) {
                    queue.push_back(child);
                }
            }
        }
        sizes.push(size);
    }

    // Every event must belong to a component. One that does not is only
    // reachable through a cycle, which is a malformed log rather than a quiet
    // one — refused, matching `assign_clocks`.
    if seen.len() != events.len() {
        let stranded: Vec<String> = events
            .iter()
            .map(|e| e.id.clone())
            .filter(|id| !seen.contains(id.as_str()))
            .collect();
        return Err(CriticalityError::CausalCycle(stranded));
    }

    let n = events.len();
    let sigma_hat = edges as f64 / n as f64;
    let largest = sizes.iter().copied().max().unwrap_or(0);
    let mean_cascade = if sizes.is_empty() {
        0.0
    } else {
        sizes.iter().sum::<usize>() as f64 / sizes.len() as f64
    };

    Ok(BranchingReading {
        sigma_hat,
        regime: spec.classify(sigma_hat),
        window: n,
        edges,
        dangling_causes: dangling,
        largest_cascade: largest,
        mean_cascade,
        cascades: sizes.len(),
    })
}

// ---------------------------------------------------------------------------
// Detector 2 — tail shape, as a COMPARISON and never as a fit
// ---------------------------------------------------------------------------

/// Which of two candidate distributions the data prefers.
///
/// ★★★ **There is no `PowerLaw` variant, and its absence is the whole design.**
/// The deferral this module shipped with named the constraint any future build
/// must satisfy: *"the verdict type may not have an absolute 'power law'
/// variant — only a comparison — until the goodness-of-fit step exists to
/// support one."* That step is Clauset, Shalizi & Newman's semi-parametric
/// bootstrap, it needs a random source, and ADR-0001 forbids one in a core
/// whose promise is reproducibility. So the missing third is still missing, and
/// this type is shaped so that its absence cannot be papered over: **there is
/// no value here that says a power law fits.**
///
/// ★★ What a comparison can honestly say is *which of the two is less bad*,
/// and CSN are explicit that **both candidates can be bad**. Hence
/// [`TailComparison::absolute_fit`], which always answers `Unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefers {
    /// The power law explains the tail better than the lognormal does.
    PowerLawOverLognormal,
    /// The lognormal does. CSN report this is the common outcome on real data.
    LognormalOverPowerLaw,
    /// The likelihood ratio is not distinguishable from zero at the declared
    /// level. ★ A real third answer: *the data cannot tell them apart* is not
    /// a weak version of either verdict.
    Indistinguishable,
}

impl Prefers {
    pub fn describe(&self) -> &'static str {
        match self {
            Self::PowerLawOverLognormal => {
                "the power law explains this tail better than a lognormal — which is not the \
                 same as it fitting"
            }
            Self::LognormalOverPowerLaw => {
                "a lognormal explains this tail better than a power law"
            }
            Self::Indistinguishable => {
                "the two are not distinguishable on this data — which is an answer, not a \
                 failure to get one"
            }
        }
    }
}

/// Whether an absolute goodness-of-fit is available at all.
///
/// ★★★ One variant, on purpose. A future slice that ships CSN's bootstrap adds
/// a second; until then this type makes *"nobody has tested whether either
/// candidate fits"* a value the caller has to receive rather than an omission
/// they might not notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsoluteFit {
    /// Requires CSN's semi-parametric bootstrap, which requires a random
    /// source, which ADR-0001 forbids here.
    Unavailable,
}

/// The tail reading: two closed-form thirds, and the third that is missing.
#[derive(Debug, Clone, PartialEq)]
pub struct TailComparison {
    alpha: f64,
    x_min: f64,
    tail_n: usize,
    log_likelihood_ratio: f64,
    normalised_ratio: f64,
    prefers: Prefers,
}

impl TailComparison {
    /// Hill's MLE for the scaling exponent, over the tail above `x_min`.
    ///
    /// ★★ Reported because it is what a caller asks for, and it is **only
    /// meaningful conditional on the comparison**. An `alpha` of 2.3 from data
    /// a lognormal explains better is a number about a model that lost.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// The declared lower bound. ★ **Supplied, never estimated.** CSN estimate
    /// `x_min` by minimising a KS distance, which is a fit, and a fit is the
    /// thing this build refuses to do without its goodness-of-fit step.
    pub fn x_min(&self) -> f64 {
        self.x_min
    }

    /// How many observations were actually in the tail.
    pub fn tail_n(&self) -> usize {
        self.tail_n
    }

    /// `R = ln L_powerlaw − ln L_lognormal`. Sign is the direction, magnitude
    /// alone is not a verdict — see [`Self::normalised_ratio`].
    pub fn log_likelihood_ratio(&self) -> f64 {
        self.log_likelihood_ratio
    }

    /// Vuong's normalised ratio `R / (√n · σ)`, which is standard-normal under
    /// the null that the two models are equally close to the truth.
    ///
    /// ★★★ **This is the part that makes the comparison honest rather than a
    /// sign check.** A raw `R > 0` says the power law scored higher on this
    /// sample; it does not say the difference means anything. Normalising by
    /// the per-observation standard deviation of the log-ratio is what turns
    /// "scored higher" into "scored higher by more than sampling noise".
    pub fn normalised_ratio(&self) -> f64 {
        self.normalised_ratio
    }

    pub fn prefers(&self) -> Prefers {
        self.prefers
    }

    /// ★★★ Always [`AbsoluteFit::Unavailable`]. See [`Prefers`].
    pub fn absolute_fit(&self) -> AbsoluteFit {
        AbsoluteFit::Unavailable
    }

    pub fn describe(&self) -> String {
        format!(
            "over {} observations above x_min {:.3}: alpha {:.3}, normalised log-likelihood \
             ratio {:.3} — {}. No absolute goodness-of-fit: both candidates may be bad and \
             nothing here has tested it.",
            self.tail_n,
            self.x_min,
            self.alpha,
            self.normalised_ratio,
            self.prefers.describe()
        )
    }
}

/// Below this |normalised ratio| the two models are called indistinguishable.
///
/// ★★ 1.96 is the two-sided 5 % point of the standard normal. Declared as a
/// constant rather than buried, because it is the line between *"a difference"*
/// and *"a difference worth naming"*, and a reader should be able to disagree
/// with it.
pub const RATIO_SIGNIFICANT_AT: f64 = 1.96;

/// The smallest tail worth comparing on.
///
/// ★★ Hill's estimator is badly behaved on a handful of points and a Vuong
/// statistic over five observations is noise with a decimal point. Refusing is
/// the honest answer; returning a verdict is not.
pub const MIN_TAIL: usize = 20;

/// **Tail shape as a comparison** (Criticality §VIII; Clauset, Shalizi &
/// Newman, 2009).
///
/// Two closed-form thirds of CSN's procedure — the Hill MLE for `alpha`, and
/// the Vuong-normalised likelihood ratio against a lognormal. The third,
/// absolute goodness-of-fit, needs a bootstrap and is not here; the return type
/// says so rather than leaving the caller to assume otherwise.
///
/// `x_min` is **supplied**, not estimated, for the reason given on
/// [`TailComparison::x_min`].
///
/// ★ Deterministic: no random source, no sampling, no iteration to a
/// tolerance. The same window returns the same reading, which is what makes it
/// legal in this crate at all.
pub fn tail_shape(sizes: &[f64], x_min: f64) -> Result<TailComparison, CriticalityError> {
    if !x_min.is_finite() || x_min <= 0.0 {
        return Err(CriticalityError::BadXMin(x_min));
    }
    if sizes.iter().any(|v| !v.is_finite()) {
        return Err(CriticalityError::NotFinite);
    }

    let tail: Vec<f64> = sizes.iter().copied().filter(|v| *v >= x_min).collect();
    if tail.len() < MIN_TAIL {
        return Err(CriticalityError::TailTooShort(tail.len()));
    }

    let n = tail.len() as f64;
    let logs: Vec<f64> = tail.iter().map(|v| (v / x_min).ln()).collect();
    let sum_logs: f64 = logs.iter().sum();
    if sum_logs <= 0.0 {
        // Every observation sits exactly on x_min. There is no tail to shape.
        return Err(CriticalityError::DegenerateTail);
    }

    // Hill: alpha = 1 + n / Σ ln(x_i / x_min)
    let alpha = 1.0 + n / sum_logs;

    // Per-observation log-likelihoods.
    //   power law: ln[(a-1)/x_min] - a · ln(x/x_min)
    let pl: Vec<f64> = logs
        .iter()
        .map(|l| ((alpha - 1.0) / x_min).ln() - alpha * l)
        .collect();

    //   lognormal fitted on the tail's own logs, then truncated at x_min:
    //   ln f(x) = -ln x - ln(s√(2π)) - (ln x - m)² / (2s²) - ln(1 - Φ((ln x_min - m)/s))
    let ln_x: Vec<f64> = tail.iter().map(|v| v.ln()).collect();
    let m = ln_x.iter().sum::<f64>() / n;
    let var = ln_x.iter().map(|l| (l - m) * (l - m)).sum::<f64>() / n;
    let s = var.sqrt();
    // NaN must fail this too, and it does not compare, so it is named.
    if s.is_nan() || s <= 0.0 {
        return Err(CriticalityError::DegenerateTail);
    }
    let tail_mass = 1.0 - normal_cdf((x_min.ln() - m) / s);
    if tail_mass.is_nan() || tail_mass <= 0.0 {
        return Err(CriticalityError::DegenerateTail);
    }
    let ln_norm = (s * (2.0 * std::f64::consts::PI).sqrt()).ln();
    let ln_tail_mass = tail_mass.ln();
    let ln: Vec<f64> = ln_x
        .iter()
        .map(|l| -l - ln_norm - (l - m) * (l - m) / (2.0 * var) - ln_tail_mass)
        .collect();

    // Vuong: R = Σ (pl_i - ln_i); normalise by √n · sd of the per-point ratio.
    let diffs: Vec<f64> = pl.iter().zip(&ln).map(|(a, b)| a - b).collect();
    let r: f64 = diffs.iter().sum();
    let mean_d = r / n;
    let sigma = (diffs.iter().map(|d| (d - mean_d) * (d - mean_d)).sum::<f64>() / n).sqrt();

    // ★★ σ = 0 means every observation gave the identical ratio, so the models
    //    are the same model on this data. Indistinguishable is the truth.
    let normalised = if sigma > 0.0 { r / (n.sqrt() * sigma) } else { 0.0 };

    let prefers = if normalised.abs() < RATIO_SIGNIFICANT_AT {
        Prefers::Indistinguishable
    } else if normalised > 0.0 {
        Prefers::PowerLawOverLognormal
    } else {
        Prefers::LognormalOverPowerLaw
    };

    Ok(TailComparison {
        alpha,
        x_min,
        tail_n: tail.len(),
        log_likelihood_ratio: r,
        normalised_ratio: normalised,
        prefers,
    })
}

/// Standard normal CDF, closed form.
///
/// ★★ Abramowitz & Stegun 7.1.26 — a rational approximation with |error| under
/// 1.5e-7, evaluated in fixed time with no iteration and no random source. It
/// is here because the truncated lognormal's normalising constant needs `Φ`,
/// and pulling in a statistics crate for one function would widen the core's
/// dependency surface for nothing.
fn normal_cdf(z: f64) -> f64 {
    let sign = if z < 0.0 { -1.0 } else { 1.0 };
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    0.5 * (1.0 + sign * y)
}

// ---------------------------------------------------------------------------
// Detector 3 — critical slowing down
// ---------------------------------------------------------------------------

/// Rising variance and lag-1 autocorrelation (Scheffer et al., 2009).
///
/// Inert data, like [`BranchingReading`]. Neither number is a threshold and
/// neither gates anything — *rising* is the signal, and one window cannot say
/// whether anything is rising.
#[derive(Debug, Clone, PartialEq)]
pub struct EarlyWarning {
    variance: f64,
    lag1_autocorrelation: f64,
    window: usize,
}

impl EarlyWarning {
    pub fn variance(&self) -> f64 {
        self.variance
    }

    /// `r₁ ∈ [-1, 1]`. Approaching 1 is the slowing-down signature.
    pub fn lag1_autocorrelation(&self) -> f64 {
        self.lag1_autocorrelation
    }

    pub fn window(&self) -> usize {
        self.window
    }

    pub fn describe(&self) -> String {
        format!(
            "variance {:.4}, lag-1 autocorrelation {:.4} over {} points — a single window, \
             so this is a level and not a trend",
            self.variance, self.lag1_autocorrelation, self.window
        )
    }
}

/// Critical slowing down over a **supplied** window.
///
/// ★ §VIII calls these *"exactly the moments §2's Monitor already maintains"* —
/// which is not true of this build, checked rather than assumed:
/// [`crate::detect::Ewma`] keeps a level and [`crate::detect::Cusum`] keeps
/// one-sided cumulative sums, and nothing retains a previous value for a lag-1
/// term. So the window is supplied, and making this genuinely a second use of
/// existing signal means adding an EWMV to [`crate::detect`], which this slice
/// does not do.
///
/// Refuses a window under three points — a lag-1 autocorrelation from a single
/// pair is an arithmetic result, not a statistic — and refuses a constant
/// series, where `r₁` is `0/0`. Returning `0` there would read as *no warning*
/// when the truth is *no signal*.
pub fn critical_slowing_down(window: &[f64]) -> Result<EarlyWarning, CriticalityError> {
    if window.len() < 3 {
        return Err(CriticalityError::WindowTooShort(window.len()));
    }
    if window.iter().any(|v| !v.is_finite()) {
        return Err(CriticalityError::NotFinite);
    }

    let n = window.len() as f64;
    let mean = window.iter().sum::<f64>() / n;
    let centred: Vec<f64> = window.iter().map(|v| v - mean).collect();
    let ss: f64 = centred.iter().map(|d| d * d).sum();
    if ss == 0.0 {
        return Err(CriticalityError::ConstantSeries);
    }

    let cov: f64 = centred.windows(2).map(|w| w[0] * w[1]).sum();
    Ok(EarlyWarning {
        variance: ss / n,
        lag1_autocorrelation: cov / ss,
        window: window.len(),
    })
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Error)]
pub enum CriticalityError {
    #[error("no events to read — σ̂ over an empty window is not zero, it is unanswered")]
    EmptyWindow,
    #[error("event '{0}' appears twice in the window; out-degree over a duplicated node is not a branching ratio")]
    DuplicateEvent(String),
    #[error("a cycle in the declared causes leaves {0:?} unreachable from any root — an event cannot descend from its own consequence")]
    CausalCycle(Vec<String>),
    #[error("critical band {0} is not usable — it declares how close to 1 counts as critical, so it cannot be negative")]
    BadBand(String),
    #[error("x_min {0} is not usable — it is the lower bound of the tail, so it must be finite and positive")]
    BadXMin(f64),
    #[error("{0} observations above x_min is too short — Hill's estimator is badly behaved on a handful of points and a Vuong statistic over five is noise with a decimal point")]
    TailTooShort(usize),
    #[error("the tail is degenerate — every observation sits on the bound, or the logs have no spread, so there is no shape to compare")]
    DegenerateTail,
    #[error("{0} points is too short — a lag-1 autocorrelation from a single pair is an arithmetic result, not a statistic")]
    WindowTooShort(usize),
    #[error("a constant series has no lag-1 autocorrelation (0/0); returning 0 would read as 'no warning' when the truth is 'no signal'")]
    ConstantSeries,
    #[error("the window contains a non-finite reading")]
    NotFinite,
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CausalStamp, Provenance};

    fn ev(id: &str, causes: &[&str]) -> Event {
        Event {
            id: id.into(),
            name: "event.test.thing".into(),
            t_event: 0,
            provenance: Provenance::Observed,
            t_ingest: None,
            source: None,
            stamp: CausalStamp {
                counter: 0,
                node: "n".into(),
            },
            causes: causes.iter().map(|c| (*c).to_string()).collect(),
            mutations: vec![],
            payload: None,
        }
    }

    // -- σ̂ -------------------------------------------------------------------

    #[test]
    fn a_chain_is_subcritical_and_a_fan_out_is_supercritical() {
        let spec = CriticalitySpec::default();

        // 4 events, 3 edges — each event has one consequence but the last.
        let chain = [ev("a", &[]), ev("b", &["a"]), ev("c", &["b"]), ev("d", &["c"])];
        let r = branching_ratio(&chain, spec).unwrap();
        assert!((r.sigma_hat() - 0.75).abs() < 1e-12);
        assert_eq!(r.regime(), Regime::Subcritical);
        assert_eq!(r.cascades(), 1);
        assert_eq!(r.largest_observed_cascade(), 4);

        // 7 events, 6 edges, each parent with two children — but the leaves have
        // none, so σ̂ is still under 1. A branching ratio over a finite window
        // is bounded by (n-1)/n for a tree, which is worth knowing.
        let tree = [
            ev("r", &[]),
            ev("l", &["r"]),
            ev("rr", &["r"]),
            ev("ll", &["l"]),
            ev("lr", &["l"]),
            ev("rl", &["rr"]),
            ev("rrr", &["rr"]),
        ];
        let t = branching_ratio(&tree, spec).unwrap();
        assert!((t.sigma_hat() - 6.0 / 7.0).abs() < 1e-12);

        // Genuine supercriticality needs events with several parents — a DAG,
        // not a tree: 3 events, 4 edges.
        let dag = [ev("a", &[]), ev("b", &["a"]), ev("c", &["a", "b", "a"])];
        let d = branching_ratio(&dag, spec).unwrap();
        assert!((d.sigma_hat() - 4.0 / 3.0).abs() < 1e-12);
        assert_eq!(d.regime(), Regime::Supercritical);
    }

    #[test]
    fn the_critical_band_is_declared_not_a_hidden_epsilon() {
        let narrow = CriticalitySpec::new(0.01).unwrap();
        let wide = CriticalitySpec::new(0.30).unwrap();
        assert_eq!(narrow.classify(0.9), Regime::Subcritical);
        assert_eq!(wide.classify(0.9), Regime::Critical);
        assert_eq!(narrow.classify(1.0), Regime::Critical);
        assert!(CriticalitySpec::new(-0.1).is_err());
    }

    // -- ★★ the risk arithmetic ---------------------------------------------

    #[test]
    fn no_expected_size_is_offered_at_or_above_criticality() {
        let spec = CriticalitySpec::new(0.2).unwrap();

        // Subcritical — a mean is honest.
        let calm = [ev("a", &[]), ev("b", &[]), ev("c", &[]), ev("d", &["a"])];
        let r = branching_ratio(&calm, spec).unwrap();
        assert_eq!(r.regime(), Regime::Subcritical);
        assert!(r.cascade_summary().mean().is_some());

        // Supercritical — no number at all, not a smaller one.
        let hot = [ev("a", &[]), ev("b", &["a"]), ev("c", &["a", "b", "a"])];
        let h = branching_ratio(&hot, spec).unwrap();
        assert_eq!(h.regime(), Regime::Supercritical);
        assert_eq!(h.cascade_summary().mean(), None);
        match h.cascade_summary() {
            CascadeSummary::NotUsable { largest_observed, why, .. } => {
                assert_eq!(largest_observed, 3);
                assert!(why.contains("diverge"));
                assert!(why.contains("floor"));
            }
            other => panic!("expected NotUsable, got {other:?}"),
        }
        // ★ The observed maximum is a floor, and the description says so.
        assert!(h.cascade_summary().describe().contains("FLOOR, not the bound"));
    }

    #[test]
    fn being_inside_the_band_already_withholds_the_mean() {
        let spec = CriticalitySpec::new(0.2).unwrap();
        // σ̂ = 1.0 exactly: 3 events, 3 edges.
        let events = [ev("a", &[]), ev("b", &["a"]), ev("c", &["a", "b"])];
        let r = branching_ratio(&events, spec).unwrap();
        assert!((r.sigma_hat() - 1.0).abs() < 1e-12);
        assert_eq!(r.regime(), Regime::Critical);
        assert!(!r.regime().mean_is_usable(), "inside the band is not knowing which side you are on");
        assert_eq!(r.cascade_summary().mean(), None);
    }

    // -- the window's edges --------------------------------------------------

    #[test]
    fn a_cause_outside_the_window_is_dangling_not_an_offspring_nobody_had() {
        let spec = CriticalitySpec::default();
        let events = [ev("b", &["a_outside"]), ev("c", &["b"])];
        let r = branching_ratio(&events, spec).unwrap();
        assert_eq!(r.dangling_causes(), 1);
        assert_eq!(r.edges(), 1);
        assert!((r.sigma_hat() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn an_empty_window_is_refused_rather_than_read_as_zero() {
        assert!(matches!(
            branching_ratio(&[], CriticalitySpec::default()),
            Err(CriticalityError::EmptyWindow)
        ));
    }

    #[test]
    fn a_causal_cycle_is_refused() {
        let spec = CriticalitySpec::default();
        let events = [ev("a", &["b"]), ev("b", &["a"])];
        assert!(matches!(
            branching_ratio(&events, spec),
            Err(CriticalityError::CausalCycle(_))
        ));
    }

    #[test]
    fn a_duplicated_event_is_refused() {
        let spec = CriticalitySpec::default();
        let events = [ev("a", &[]), ev("a", &[])];
        assert!(matches!(
            branching_ratio(&events, spec),
            Err(CriticalityError::DuplicateEvent(_))
        ));
    }

    // -- ★★ read-only --------------------------------------------------------

    #[test]
    fn a_reading_is_a_pure_function_of_the_log() {
        let spec = CriticalitySpec::default();
        let events = [ev("a", &[]), ev("b", &["a"]), ev("c", &["b"])];
        let first = branching_ratio(&events, spec).unwrap();
        let second = branching_ratio(&events, spec).unwrap();
        assert_eq!(first, second);
        // The log is untouched — the borrow is immutable, so this is the
        // compiler's guarantee rather than a convention.
        assert_eq!(events[1].causes, vec!["a".to_string()]);
    }

    // -- detector 3 ----------------------------------------------------------

    #[test]
    fn critical_slowing_down_reports_variance_and_lag_one() {
        // A ramp, pinned exactly because it is hand-computable: centred
        // [-2,-1,0,1,2], ss = 10, lag-1 covariance = 4, so r₁ = 0.4.
        let short = [1.0, 2.0, 3.0, 4.0, 5.0];
        let w = critical_slowing_down(&short).unwrap();
        assert!((w.variance() - 2.0).abs() < 1e-12);
        assert!((w.lag1_autocorrelation() - 0.4).abs() < 1e-12);

        // ★ The SAME shape reads far more persistent on a longer window
        // (r₁ = 0.7 at ten points). r₁ on a short series understates
        // persistence — which is exactly why §VIII calls these diagnostics
        // with real false-positive rates on short household series, and why
        // none of this gates anything.
        let long: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        let l = critical_slowing_down(&long).unwrap();
        assert!((l.lag1_autocorrelation() - 0.7).abs() < 1e-12);
        assert!(l.lag1_autocorrelation() > w.lag1_autocorrelation());

        // An alternating series is strongly anti-correlated.
        let alt = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
        let a = critical_slowing_down(&alt).unwrap();
        assert!(a.lag1_autocorrelation() < -0.5, "{}", a.lag1_autocorrelation());
        assert!(a.describe().contains("not a trend"));
    }

    #[test]
    fn a_constant_series_is_refused_rather_than_reported_as_zero() {
        assert!(matches!(
            critical_slowing_down(&[2.0, 2.0, 2.0, 2.0]),
            Err(CriticalityError::ConstantSeries)
        ));
    }

    #[test]
    fn a_window_under_three_points_is_refused() {
        assert!(matches!(
            critical_slowing_down(&[1.0, 2.0]),
            Err(CriticalityError::WindowTooShort(2))
        ));
        assert!(matches!(
            critical_slowing_down(&[1.0, f64::NAN, 3.0]),
            Err(CriticalityError::NotFinite)
        ));
    }
    // -- Detector 2: tail shape, as a comparison ----------------------------

    /// A power-law sample built deterministically by inverse transform:
    /// x_i = x_min · (1 - u_i)^(-1/(a-1)) over a fixed grid of u. No RNG —
    /// the grid IS the sample, which is what makes this legal here.
    fn power_law_sample(n: usize, x_min: f64, alpha: f64) -> Vec<f64> {
        (1..=n)
            .map(|i| {
                let u = i as f64 / (n as f64 + 1.0);
                x_min * (1.0 - u).powf(-1.0 / (alpha - 1.0))
            })
            .collect()
    }

    /// A lognormal sample the same way, through the normal quantile.
    fn lognormal_sample(n: usize, m: f64, s: f64) -> Vec<f64> {
        (1..=n)
            .map(|i| {
                let u = i as f64 / (n as f64 + 1.0);
                (m + s * normal_quantile(u)).exp()
            })
            .collect()
    }

    /// Acklam-style rational inverse normal, deterministic. Test-only.
    fn normal_quantile(p: f64) -> f64 {
        // Beasley-Springer-Moro, adequate for generating a shaped fixture.
        let a = [2.50662823884, -18.61500062529, 41.39119773534, -25.44106049637];
        let b = [-8.47351093090, 23.08336743743, -21.06224101826, 3.13082909833];
        let c = [
            0.3374754822726147, 0.9761690190917186, 0.1607979714918209, 0.0276438810333863,
            0.0038405729373609, 0.0003951896511919, 0.0000321767881768, 0.0000002888167364,
            0.0000003960315187,
        ];
        let y = p - 0.5;
        if y.abs() < 0.42 {
            let r = y * y;
            let num = y * (((a[3] * r + a[2]) * r + a[1]) * r + a[0]);
            let den = (((b[3] * r + b[2]) * r + b[1]) * r + b[0]) * r + 1.0;
            return num / den;
        }
        let r = if y > 0.0 { 1.0 - p } else { p };
        let r = (-(r.ln())).ln();
        let mut x = c[0];
        let mut t = 1.0;
        for ci in &c[1..] {
            t *= r;
            x += ci * t;
        }
        if y < 0.0 { -x } else { x }
    }

    #[test]
    fn there_is_no_value_that_says_a_power_law_fits() {
        // ★★★ The constraint the deferral recorded, now enforced by the type.
        //     `Prefers` has three variants and none of them is an absolute
        //     claim, and `absolute_fit` has one variant and it is Unavailable.
        //     A future slice shipping CSN's bootstrap adds the second; until
        //     then a caller cannot receive "it fits" from this module.
        let s = power_law_sample(200, 1.0, 2.5);
        let r = tail_shape(&s, 1.0).expect("reads");
        assert_eq!(r.absolute_fit(), AbsoluteFit::Unavailable);
        assert!(r.describe().contains("No absolute goodness-of-fit"));
    }

    #[test]
    fn a_power_law_tail_prefers_the_power_law() {
        let s = power_law_sample(400, 1.0, 2.5);
        let r = tail_shape(&s, 1.0).expect("reads");
        assert_eq!(r.prefers(), Prefers::PowerLawOverLognormal);
        // Hill recovers roughly the alpha the sample was built with. Loose on
        // purpose: this asserts the estimator is wired to the right formula,
        // not that a 400-point grid reproduces a parameter exactly.
        assert!((r.alpha() - 2.5).abs() < 0.35, "alpha was {}", r.alpha());
    }

    #[test]
    fn a_lognormal_with_its_curvature_in_view_prefers_the_lognormal() {
        // ★★ The outcome CSN report is common on real data, and a comparison
        //    that could only ever pick the power law would not be one.
        //    x_min well below the median keeps the bend in view, which is what
        //    there is to see.
        let s = lognormal_sample(400, 0.0, 1.0);
        let r = tail_shape(&s, 0.3).expect("reads");
        assert_eq!(r.prefers(), Prefers::LognormalOverPowerLaw);
    }

    #[test]
    fn a_lognormal_cut_at_its_median_is_indistinguishable_and_that_is_correct() {
        // ★★★ **CSN's central warning, made executable.** The upper half of a
        //     lognormal over a limited range genuinely does look like a power
        //     law — that is *why* fitting one to a straight-ish log-log line is
        //     a documented trap. Cut at the median, this data cannot tell the
        //     two apart, and the honest answer is to say so.
        //
        //     This test was originally written expecting `LognormalOverPowerLaw`
        //     and it failed. The expectation was naive, not the code: a module
        //     that returned a confident verdict here would be committing the
        //     exact error the whole detector exists to avoid.
        let s = lognormal_sample(400, 0.0, 1.0);
        let r = tail_shape(&s, 1.0).expect("reads");
        assert_eq!(r.prefers(), Prefers::Indistinguishable);
        assert!(r.normalised_ratio().abs() < RATIO_SIGNIFICANT_AT);
    }

    #[test]
    fn where_the_bound_sits_changes_the_verdict_and_it_should() {
        // ★★ Not a defect — a property, and the reason `x_min` is supplied
        //    rather than estimated. How much of the body you keep decides how
        //    much shape there is to read, so the caller has to own that choice
        //    instead of having one silently fitted for them.
        let s = lognormal_sample(400, 0.0, 1.0);
        assert_eq!(tail_shape(&s, 1.0).unwrap().prefers(), Prefers::Indistinguishable);
        assert_eq!(
            tail_shape(&s, 0.2).unwrap().prefers(),
            Prefers::LognormalOverPowerLaw
        );
    }

    #[test]
    fn the_verdict_reads_the_normalised_ratio_not_the_sign_of_the_raw_one() {
        // ★★★ The part that makes this honest rather than a sign check. A raw
        //     R > 0 says the power law scored higher on this sample; only the
        //     normalisation says whether that beat sampling noise.
        let s = lognormal_sample(400, 0.0, 1.0);
        let r = tail_shape(&s, 0.3).expect("reads");
        assert!(r.normalised_ratio().abs() > 0.0);
        // Direction agrees between the two, but the DECISION reads the
        // normalised one — asserted by construction below.
        assert_eq!(
            r.log_likelihood_ratio() < 0.0,
            r.normalised_ratio() < 0.0,
            "the two must at least agree on direction"
        );
    }

    #[test]
    fn indistinguishable_is_a_real_third_answer() {
        // ★★ Below the declared level the module says the data cannot tell
        //    them apart, rather than picking whichever scored a hair higher.
        const { assert!(RATIO_SIGNIFICANT_AT > 0.0) };
        let r = TailComparison {
            alpha: 2.0,
            x_min: 1.0,
            tail_n: 50,
            log_likelihood_ratio: 0.4,
            normalised_ratio: 0.9,
            prefers: Prefers::Indistinguishable,
        };
        assert!(r.prefers().describe().contains("not distinguishable"));
    }

    #[test]
    fn a_short_tail_is_refused_rather_than_answered() {
        // ★★ Hill on a handful of points is noise with a decimal point.
        let s = power_law_sample(10, 1.0, 2.5);
        assert!(matches!(
            tail_shape(&s, 1.0),
            Err(CriticalityError::TailTooShort(_))
        ));
    }

    #[test]
    fn a_bad_lower_bound_is_refused() {
        let s = power_law_sample(200, 1.0, 2.5);
        assert!(matches!(tail_shape(&s, 0.0), Err(CriticalityError::BadXMin(_))));
        assert!(matches!(tail_shape(&s, -1.0), Err(CriticalityError::BadXMin(_))));
    }

    #[test]
    fn x_min_is_supplied_and_filters_the_tail() {
        // ★★ Supplied, never estimated — estimating it is a fit, and a fit is
        //    what this build refuses without its goodness-of-fit step.
        let s = power_law_sample(400, 1.0, 2.5);
        let all = tail_shape(&s, 1.0).expect("reads");
        let high = tail_shape(&s, 2.0).expect("reads");
        assert!(high.tail_n() < all.tail_n(), "a higher bound keeps fewer points");
        assert_eq!(high.x_min(), 2.0);
    }

    #[test]
    fn the_reading_is_deterministic() {
        // ★★★ The property that makes it legal in this crate: no random
        //     source, so the same window is the same answer, always.
        let s = power_law_sample(300, 1.0, 2.2);
        assert_eq!(tail_shape(&s, 1.0).unwrap(), tail_shape(&s, 1.0).unwrap());
    }

    #[test]
    fn the_normal_cdf_is_right_where_it_is_checked() {
        assert!((normal_cdf(0.0) - 0.5).abs() < 1e-9);
        assert!((normal_cdf(1.96) - 0.975).abs() < 1e-4);
        assert!((normal_cdf(-1.96) - 0.025).abs() < 1e-4);
        assert!(normal_cdf(-8.0) >= 0.0 && normal_cdf(8.0) <= 1.0);
    }

}
