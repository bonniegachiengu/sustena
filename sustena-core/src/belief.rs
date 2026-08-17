//! Belief state under partial observability (MONITOR §VIII).
//!
//! When a nano-sustain goes silent — sensor offline, network partition, device
//! failure — the Monitor cannot observe its true state, and the system moves
//! from fully observable to **partially observable**. The formal model is a
//! POMDP maintaining a belief `b(s)` over possible true states *(Sondik,
//! 1971)*.
//!
//! ## ★★ No Kalman — the observation update is EVT-10's snapshot collapse
//!
//! §VIII's pseudocode writes the full update as
//! `point_mass(kalman_filter.update(observation))`. **The Kalman half is not
//! built**, and that is a position this codebase already holds rather than a
//! shortcut taken here: MON-2 is a **declined import**, because Kalman assumes
//! a continuous linear system `ẋ = Ax + Bu` and Sustena's state is discrete,
//! typed and event-sourced. `monitor.rs` says the same thing about the
//! detection pipeline.
//!
//! What survives is the **`point_mass`** — and Sustena already has a rule for
//! what an observation of an external authority means. §VII's snapshot
//! dimension: *a newer reading supersedes*. So the belief holds an EVT-10
//! [`SnapshotDim`] and collapses through it, which makes the coherence
//! structural rather than parallel — **"an observation arrived" has one meaning
//! in this codebase, not two.**
//!
//! The visible consequence is a real behaviour, not a tidiness: **a stale
//! reading arriving late does not collapse the belief.** It loses on `(τ, id)`
//! exactly as it would on the state dimension, and [`Collapse::Stale`] says so.
//!
//! ## ★ Silence widens, and the widening does not invent dynamics
//!
//! §VIII's silence branch is `predict_forward(belief, transition_model)` then
//! `widen(belief, sigma = elapsed * DRIFT_RATE)`.
//!
//! The widening is honest on its own: **we know less the longer we have not
//! heard.** The `predict_forward` is only honest if a transition model actually
//! exists, so [`Dynamics`] is a **declared** argument with no `Default` —
//! [`Dynamics::Unknown`] holds the mean and grows only the uncertainty, which
//! is the honest floor, and [`Dynamics::LinearDrift`] moves it only when
//! someone has said how. **Fabricating dynamics to fill the pseudocode's shape
//! would put a made-up trajectory behind a real-looking mean.**
//!
//! ## ★ The escalation is an admission, not an alarm
//!
//! §VIII: silence past the threshold escalates *"not because we know something
//! is wrong, but because uncertainty has grown to the point where human
//! attention is warranted."*
//!
//! So a [`SilenceAlert`] carries the variance as its reason and rides the
//! **existing** Monitor→Controller boundary — `detect::Severity::escalates()`,
//! the one function that already draws that line. It deliberately is **not** a
//! [`detect::Alert`], because that type carries a [`Shift`](crate::detect::Shift)
//! and *nothing shifted*: claiming a direction the data does not have would be
//! a worse error than a duplicated struct. The boundary is reused; the shape is
//! not.
//!
//! And a silence alert can never be `Info`: [`SilenceSpec::new`] refuses it,
//! because an alert that does not escalate is not the thing §VIII describes.
//!
//! ## Scope, honestly
//!
//! - **This is not a full joint POMDP belief.** A distribution over the whole
//!   state space is intractable for a rich state, and §VIII's own render only
//!   ever asks for `mean()` and `variance()`. So the tractable form is what is
//!   built: **per observed dimension, a mean and a growing variance**, scoped to
//!   the silent-capable quantities. The joint belief is the theoretical model
//!   this approximates, named rather than claimed.
//! - **§I's implication is served, not proven.** *"Every state variable worth
//!   governing must be reachable from a measurable output — unmeasured state is
//!   ungoverned state."* [`BeliefTracker::ungoverned`] answers the cheap,
//!   concrete half: which declared-governed dimensions have never been observed
//!   at all. Full observability-matrix rank analysis is MON-1 and is a slot,
//!   not something done here.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::detect::Severity;
use crate::dimension::SnapshotDim;

/// What is known about how a quantity moves while unobserved.
///
/// **Declared, never defaulted** — the type implements no `Default`, the same
/// discipline `Lateness` and `DstPolicy` carry, because *we have no model* and
/// *we have this model* are different claims and only one of them is usually
/// true.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dynamics {
    /// ★ No transition model. The mean is **held** and only the uncertainty
    /// grows — *we know less over time*, which is the honest floor and is what
    /// most Sustena quantities actually warrant.
    Unknown,
    /// A declared linear drift, in units per hour. Moves the mean while
    /// unobserved — usable only where someone has genuinely said how the
    /// quantity behaves.
    LinearDrift { per_hour: f64 },
}

/// What can go wrong declaring a silence policy.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BeliefError {
    /// ★ §VIII defines silence past the threshold as *warranting human
    /// attention*. An alert that stays in the Monitor is not that alert.
    #[error(
        "a silence alert cannot be INFO — §VIII escalates it because uncertainty has grown to \
         where human attention is warranted, and INFO is the severity that does not cross the \
         Monitor→Controller boundary"
    )]
    SilenceCannotBeInfo,
    #[error("the drift rate must be finite and non-negative; {0} would make uncertainty shrink or undefined with silence")]
    BadDriftRate(f64),
    #[error("the silence threshold must be positive; {0}ms would alert on every gap including none")]
    BadThreshold(i64),
}

/// The declared policy for one tracker: how fast uncertainty grows, what moves
/// the mean, and when silence becomes a person's problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SilenceSpec {
    /// `σ` added per hour of silence — §VIII's `DRIFT_RATE`.
    drift_rate: f64,
    dynamics: Dynamics,
    silence_threshold_ms: i64,
    severity: Severity,
}

impl SilenceSpec {
    pub fn new(
        drift_rate: f64,
        dynamics: Dynamics,
        silence_threshold_ms: i64,
        severity: Severity,
    ) -> Result<Self, BeliefError> {
        if !drift_rate.is_finite() || drift_rate < 0.0 {
            return Err(BeliefError::BadDriftRate(drift_rate));
        }
        if silence_threshold_ms <= 0 {
            return Err(BeliefError::BadThreshold(silence_threshold_ms));
        }
        if !severity.escalates() {
            return Err(BeliefError::SilenceCannotBeInfo);
        }
        Ok(Self { drift_rate, dynamics, silence_threshold_ms, severity })
    }

    pub fn drift_rate(&self) -> f64 {
        self.drift_rate
    }

    pub fn dynamics(&self) -> Dynamics {
        self.dynamics
    }

    pub fn silence_threshold_ms(&self) -> i64 {
        self.silence_threshold_ms
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }
}

const HOUR_MS: f64 = 3_600_000.0;

/// A belief about one observed dimension.
///
/// ★★ The collapsed reading is held as an EVT-10 [`SnapshotDim`] — not a copy
/// of its semantics, the thing itself — so the belief's *full update* and the
/// state dimension's *supersede* cannot drift apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Belief {
    reading: SnapshotDim,
    /// The reading's own numeric value, kept alongside so `mean()` needs no
    /// fallible extraction at every call.
    mean_at_reading: f64,
}

impl Belief {
    fn new(value: f64, t_event: i64, id: &str) -> Self {
        Self { reading: SnapshotDim::observed(json!(value), t_event, id), mean_at_reading: value }
    }

    /// When the belief last collapsed onto a real observation.
    pub fn last_contact(&self) -> i64 {
        self.reading.t_event()
    }

    pub fn observation_id(&self) -> &str {
        self.reading.observation_id()
    }

    /// How long unheard from, at `now`. Never negative.
    pub fn silent_for(&self, now: i64) -> i64 {
        (now - self.last_contact()).max(0)
    }

    /// ★ The estimate. With [`Dynamics::Unknown`] this is the last observation,
    /// **held** — the mean does not wander because nothing said it would.
    pub fn mean(&self, now: i64, spec: &SilenceSpec) -> f64 {
        match spec.dynamics {
            Dynamics::Unknown => self.mean_at_reading,
            Dynamics::LinearDrift { per_hour } => {
                self.mean_at_reading + per_hour * (self.silent_for(now) as f64 / HOUR_MS)
            }
        }
    }

    /// ★ `σ = elapsed × DRIFT_RATE`, variance `= σ²` — §VIII's widening,
    /// literally. **Zero at the moment of collapse**, and monotonically
    /// non-decreasing in elapsed silence thereafter.
    pub fn variance(&self, now: i64, spec: &SilenceSpec) -> f64 {
        let sigma = (self.silent_for(now) as f64 / HOUR_MS) * spec.drift_rate;
        sigma * sigma
    }
}

/// What an offered observation did to the belief.
///
/// Three outcomes because there are three, and [`Collapse::Stale`] is the one
/// that proves the update is §VII's supersede rather than *whatever arrived
/// last*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Collapse {
    /// There was no belief here before — the first sight of this dimension.
    FirstContact { mean: f64 },
    /// The observation was newer by `(τ, id)`: the belief collapsed to a point
    /// mass on it, and the variance is zero until silence grows it again.
    Collapsed { mean: f64, superseded: f64 },
    /// ★★ The observation lost on `(τ, id)` — a stale reading arriving late.
    /// The belief is **unchanged**, exactly as EVT-10's snapshot dimension
    /// would leave the state.
    Stale { kept: f64, kept_at: i64 },
}

/// One dimension rendered for the Monitor (§VIII's `render_for_monitor`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeliefReading {
    pub dimension: String,
    pub estimated: f64,
    /// The uncertainty band. Zero only at the instant of an observation.
    pub uncertainty: f64,
    pub last_contact: i64,
    pub silent_for_ms: i64,
    /// `Some` once silence exceeds the declared threshold.
    pub alert: Option<SilenceAlert>,
}

/// Silence past the threshold — **an admission, not an alarm**.
///
/// ★ Deliberately not a [`detect::Alert`](crate::detect::Alert): that type
/// carries a [`Shift`](crate::detect::Shift), and nothing shifted. It reuses
/// the **boundary** — [`Severity::escalates`] — rather than the shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SilenceAlert {
    pub dimension: String,
    pub silent_for_ms: i64,
    pub last_contact: i64,
    /// ★ The reason, carried: uncertainty has grown this far.
    pub uncertainty: f64,
    pub severity: Severity,
}

impl SilenceAlert {
    /// Rides the existing Monitor→Controller boundary.
    pub fn escalates(&self) -> bool {
        self.severity.escalates()
    }

    /// A line that says what this is and what it is not.
    pub fn describe(&self) -> String {
        format!(
            "SENSOR_SILENT: '{}' unheard for {}ms (last contact {}). Uncertainty has grown to \
             {:.3}. Escalated NOT because something is known to be wrong, but because we have \
             lost sight of it.",
            self.dimension, self.silent_for_ms, self.last_contact, self.uncertainty
        )
    }
}

/// Per-sustain belief state over the observed dimensions.
///
/// Sits **alongside** the CUSUM/EWMA chain rather than inside it: that chain
/// answers *has this moved*, and this answers *do we still know where it is*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeliefTracker {
    spec: SilenceSpec,
    beliefs: BTreeMap<String, Belief>,
    /// Dimensions declared worth governing — §I's list, against which
    /// [`ungoverned`](Self::ungoverned) is answered.
    governed: BTreeSet<String>,
}

impl BeliefTracker {
    pub fn new(spec: SilenceSpec) -> Self {
        Self { spec, beliefs: BTreeMap::new(), governed: BTreeSet::new() }
    }

    /// Declare a dimension worth governing. §I: an unmeasured one is ungoverned.
    pub fn govern(&mut self, dimension: impl Into<String>) {
        self.governed.insert(dimension.into());
    }

    pub fn spec(&self) -> &SilenceSpec {
        &self.spec
    }

    /// ★★ An observation arrives. Collapses to a point mass **iff it wins on
    /// `(τ, id)`** — decided by [`SnapshotDim`] itself, not by a second copy of
    /// the rule.
    pub fn observe(
        &mut self,
        dimension: impl Into<String>,
        value: f64,
        t_event: i64,
        id: impl Into<String>,
    ) -> Collapse {
        let dimension = dimension.into();
        let id = id.into();
        match self.beliefs.get(&dimension) {
            None => {
                self.beliefs.insert(dimension, Belief::new(value, t_event, &id));
                Collapse::FirstContact { mean: value }
            }
            Some(existing) => {
                // The supersede is EVT-10's, run rather than reimplemented.
                let winner = existing.reading.observe(json!(value), t_event, id.clone());
                if winner.observation_id() == id {
                    let superseded = existing.mean_at_reading;
                    self.beliefs.insert(dimension, Belief::new(value, t_event, &id));
                    Collapse::Collapsed { mean: value, superseded }
                } else {
                    Collapse::Stale {
                        kept: existing.mean_at_reading,
                        kept_at: existing.last_contact(),
                    }
                }
            }
        }
    }

    pub fn belief(&self, dimension: &str) -> Option<&Belief> {
        self.beliefs.get(dimension)
    }

    /// §VIII's `render_for_monitor`, for one dimension.
    pub fn read(&self, dimension: &str, now: i64) -> Option<BeliefReading> {
        let b = self.beliefs.get(dimension)?;
        let silent_for_ms = b.silent_for(now);
        let uncertainty = b.variance(now, &self.spec);
        let alert = (silent_for_ms > self.spec.silence_threshold_ms).then(|| SilenceAlert {
            dimension: dimension.to_string(),
            silent_for_ms,
            last_contact: b.last_contact(),
            uncertainty,
            severity: self.spec.severity,
        });
        Some(BeliefReading {
            dimension: dimension.to_string(),
            estimated: b.mean(now, &self.spec),
            uncertainty,
            last_contact: b.last_contact(),
            silent_for_ms,
            alert,
        })
    }

    /// Every tracked dimension, rendered.
    pub fn render(&self, now: i64) -> Vec<BeliefReading> {
        self.beliefs
            .keys()
            .filter_map(|d| self.read(d, now))
            .collect()
    }

    /// Everything that has gone silent past the declared threshold.
    pub fn silent(&self, now: i64) -> Vec<SilenceAlert> {
        self.render(now).into_iter().filter_map(|r| r.alert).collect()
    }

    /// ★ §I's cheap, concrete half: which declared-governed dimensions have
    /// **never been observed at all**.
    ///
    /// *"Unmeasured state is ungoverned state."* This does not attempt the
    /// observability-matrix rank analysis §I builds toward — that is MON-1, and
    /// naming it as absent is more useful than approximating it. What this does
    /// answer, exactly, is the case that needs no matrix: a quantity someone
    /// declared worth governing that no sensor has ever spoken about.
    pub fn ungoverned(&self) -> Vec<String> {
        self.governed
            .iter()
            .filter(|d| !self.beliefs.contains_key(*d))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn spec(dynamics: Dynamics) -> SilenceSpec {
        // 6 hours, §VIII's own silence_threshold_hours.
        SilenceSpec::new(2.0, dynamics, 6 * HOUR, Severity::Warning).unwrap()
    }

    fn tracker() -> BeliefTracker {
        BeliefTracker::new(spec(Dynamics::Unknown))
    }

    // ── ★★ the observation update is EVT-10's snapshot collapse ────────────

    #[test]
    fn an_observation_collapses_the_belief_to_a_point_mass() {
        let mut t = tracker();
        assert_eq!(t.observe("soil.moisture", 42.0, 100, "s1"), Collapse::FirstContact { mean: 42.0 });

        let r = t.read("soil.moisture", 100).unwrap();
        assert_eq!(r.estimated, 42.0);
        assert_eq!(r.uncertainty, 0.0, "a point mass — zero variance at the instant of the reading");
        assert_eq!(r.last_contact, 100);
    }

    #[test]
    fn a_newer_observation_supersedes_and_the_variance_resets() {
        let mut t = tracker();
        t.observe("soil.moisture", 42.0, 100, "s1");
        // It has been silent, so uncertainty has grown...
        assert!(t.read("soil.moisture", 100 + 3 * HOUR).unwrap().uncertainty > 0.0);

        let c = t.observe("soil.moisture", 38.0, 100 + 3 * HOUR, "s2");
        assert_eq!(c, Collapse::Collapsed { mean: 38.0, superseded: 42.0 });
        let r = t.read("soil.moisture", 100 + 3 * HOUR).unwrap();
        assert_eq!(r.estimated, 38.0);
        assert_eq!(r.uncertainty, 0.0, "...and hearing again collapses it back to a point");
    }

    #[test]
    fn a_stale_observation_arriving_late_does_not_collapse_the_belief() {
        // ★★ THE COHERENCE PROOF. The belief's full update IS §VII's supersede,
        // because it runs through the same `SnapshotDim` the state dimension
        // uses — so a late reading loses here exactly as it loses there.
        let mut t = tracker();
        t.observe("finances.liquid.balance", 1200.0, 200, "sms-2");

        let c = t.observe("finances.liquid.balance", 9999.0, 50, "sms-0");
        assert_eq!(c, Collapse::Stale { kept: 1200.0, kept_at: 200 });
        assert_eq!(t.read("finances.liquid.balance", 200).unwrap().estimated, 1200.0);
    }

    #[test]
    fn an_equal_event_time_breaks_on_the_id_as_it_does_on_the_state_dimension() {
        let mut t = tracker();
        t.observe("x", 1.0, 100, "a");
        // 'z' > 'a' at the same τ, so it wins — the same total order, reused.
        assert!(matches!(t.observe("x", 2.0, 100, "z"), Collapse::Collapsed { .. }));
        let mut u = tracker();
        u.observe("x", 1.0, 100, "z");
        assert!(matches!(u.observe("x", 2.0, 100, "a"), Collapse::Stale { .. }));
    }

    // ── ★ silence widens, monotonically ────────────────────────────────────

    #[test]
    fn uncertainty_grows_monotonically_with_silence() {
        let mut t = tracker();
        t.observe("soil.moisture", 42.0, 0, "s1");

        let mut last = -1.0;
        for h in 0..24 {
            let v = t.read("soil.moisture", h * HOUR).unwrap().uncertainty;
            assert!(v >= last, "variance shrank between hour {} and {h}", h - 1);
            last = v;
        }
        // σ = elapsed_hours × drift_rate; variance = σ².
        assert_eq!(t.read("soil.moisture", 3 * HOUR).unwrap().uncertainty, 36.0);
    }

    #[test]
    fn with_no_transition_model_the_mean_is_held_and_only_uncertainty_grows() {
        // ★ The honest floor: we know less, not something else.
        let mut t = tracker();
        t.observe("soil.moisture", 42.0, 0, "s1");
        let far = t.read("soil.moisture", 48 * HOUR).unwrap();
        assert_eq!(far.estimated, 42.0, "the mean did not wander — nothing said it would");
        assert!(far.uncertainty > 0.0);
    }

    #[test]
    fn a_declared_drift_moves_the_mean_and_nothing_else_does() {
        let mut t = BeliefTracker::new(spec(Dynamics::LinearDrift { per_hour: -1.5 }));
        t.observe("soil.moisture", 42.0, 0, "s1");
        assert_eq!(t.read("soil.moisture", 4 * HOUR).unwrap().estimated, 36.0);
    }

    // ── ★ the escalation is an admission, not an alarm ─────────────────────

    #[test]
    fn silence_past_the_threshold_raises_an_alert_and_before_it_does_not() {
        let mut t = tracker();
        t.observe("soil.moisture", 42.0, 0, "s1");

        assert!(t.read("soil.moisture", 5 * HOUR).unwrap().alert.is_none());
        assert!(t.silent(5 * HOUR).is_empty());

        let alert = t.read("soil.moisture", 7 * HOUR).unwrap().alert.expect("past the threshold");
        assert_eq!(alert.silent_for_ms, 7 * HOUR);
        assert_eq!(alert.dimension, "soil.moisture");
        assert!(alert.uncertainty > 0.0, "the reason travels with it");
    }

    #[test]
    fn the_alert_rides_the_existing_monitor_to_controller_boundary() {
        // ★ `Severity::escalates()` is the one function that draws that line;
        // this reuses it rather than inventing a second notion of crossing.
        let mut t = tracker();
        t.observe("soil.moisture", 42.0, 0, "s1");
        let alert = &t.silent(7 * HOUR)[0];
        assert!(alert.escalates());
        assert!(alert.describe().contains("NOT because something is known to be wrong"));
    }

    #[test]
    fn a_silence_alert_can_never_be_info() {
        // ★ §VIII defines it as warranting attention, so the one severity that
        // would not cross is refused at declaration.
        assert_eq!(
            SilenceSpec::new(1.0, Dynamics::Unknown, HOUR, Severity::Info),
            Err(BeliefError::SilenceCannotBeInfo)
        );
        assert!(SilenceSpec::new(1.0, Dynamics::Unknown, HOUR, Severity::Critical).is_ok());
    }

    #[test]
    fn a_nonsense_policy_is_refused_at_declaration() {
        assert!(matches!(
            SilenceSpec::new(-1.0, Dynamics::Unknown, HOUR, Severity::Warning),
            Err(BeliefError::BadDriftRate(_))
        ));
        assert!(matches!(
            SilenceSpec::new(1.0, Dynamics::Unknown, 0, Severity::Warning),
            Err(BeliefError::BadThreshold(0))
        ));
    }

    #[test]
    fn hearing_again_clears_the_alert() {
        let mut t = tracker();
        t.observe("soil.moisture", 42.0, 0, "s1");
        assert_eq!(t.silent(7 * HOUR).len(), 1);
        t.observe("soil.moisture", 40.0, 7 * HOUR, "s2");
        assert!(t.silent(7 * HOUR).is_empty(), "the sensor spoke — we can see it again");
    }

    // ── §I: unmeasured state is ungoverned state ───────────────────────────

    #[test]
    fn a_governed_dimension_never_observed_is_reported_ungoverned() {
        let mut t = tracker();
        t.govern("soil.moisture");
        t.govern("tank.level");
        t.observe("soil.moisture", 42.0, 0, "s1");

        assert_eq!(t.ungoverned(), vec!["tank.level".to_string()]);
        t.observe("tank.level", 10.0, 0, "t1");
        assert!(t.ungoverned().is_empty());
    }

    #[test]
    fn tracking_is_per_dimension_and_they_do_not_interfere() {
        let mut t = tracker();
        t.observe("soil.moisture", 42.0, 0, "s1");
        t.observe("tank.level", 10.0, 5 * HOUR, "t1");

        let rendered = t.render(7 * HOUR);
        assert_eq!(rendered.len(), 2);
        let silent = t.silent(7 * HOUR);
        assert_eq!(silent.len(), 1, "only the one that actually went quiet");
        assert_eq!(silent[0].dimension, "soil.moisture");
    }
}
