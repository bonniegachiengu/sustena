//! The Monitor engine — the per-sustain object that owns the detection chain
//! (Monitor §IX + Additions · MON-9).
//!
//! The chain has existed since the detect slice; nothing **owned** it. A
//! `Watch` had to be constructed, fed and interpreted by whoever happened to
//! call it, which meant OBSERVE could not run itself. This is the object that
//! holds one chain per sustain, reads each sustain's own declared parameters,
//! and hands an escalation to the OODA loop — after which OBSERVE is
//! self-driving.
//!
//! ```text
//! ingest(state):  W = d(s,V)  →  EWMA  →  CUSUM  →  Severity        §IX
//!                 + σ_k(history)                                    §IX
//!                 IF Severity ≥ WARNING → drive OODA's OBSERVE
//! ```
//!
//! ## ★ The pipeline is the NATIVE one — Kalman is a declined import
//!
//! §IX's `ingest` runs **Kalman → EWMA → CUSUM**. This runs
//! **`W = d(s,V)` → EWMA → CUSUM**, and the substitution is deliberate. The
//! article's own Additions say why:
//!
//! > The observability and Kalman apparatus of §I–II assumes a *continuous
//! > linear* system `ẋ = Ax + Bu`. Sustena's state is discrete, typed, and
//! > event-sourced [...] the Kalman filter [is] a loose import, not a literal
//! > fit. The Sustena-native form [...] is distance-to-V computed on the fold
//! > of the log, with CUSUM/EWMA over that series — the same escalation
//! > boundary, without assuming an ODE.
//!
//! So MON-2 is **not a gap this engine leaves open**: it is a row the series
//! declined, and MON-11 discharges the caveat without it. Building a Kalman
//! stage here would add an estimator for noise this state model does not have,
//! in front of a detector that does not need it.
//!
//! ## Watching never stops — and it does not interrupt looking
//!
//! §III: *"Monitor is always running at OBSERVE. Controller only surfaces when
//! the OODA loop has reached DECIDE."* [`MonitorEngine::drive`] therefore keeps
//! ingesting while the loop is parked mid-cycle — the EWMA level and the CUSUM
//! accumulator go on advancing — but hands off **only when the loop is
//! actually at OBSERVE**. A reading that arrived while a person was being asked
//! something is recorded, not dropped and not forced in; [`Driven::held_back`]
//! names the phase that declined it, so a deferred hand-off is visible rather
//! than silent.
//!
//! ## Escalation is the only route from watching to looking
//!
//! [`Ingested::observation`] returns `Option<OodaObservation>` and is `None`
//! unless the reading crosses the boundary. There is no way to walk from a
//! quiet ingest into OBSERVE through this engine — *watching is metered,
//! looking spends a budget*, and the type says so.
//!
//! ## Two clean slots, deliberately NOT stubbed
//!
//! §IX's engine also holds a `BeliefStateTracker` (MON-8) and a
//! `WidgetVisualEncoder` (MON-7), and `render_widget()`/`tick()` use them.
//! Neither is built here and neither has a placeholder field, because an empty
//! `belief_trackers` map would read as built-and-idle rather than absent. They
//! are separate ⬜ rows; the seam for them is the per-sustain keying this
//! engine already establishes.
//!
//! ## The host drives; core does not schedule
//!
//! §IX's `tick()` *"runs at display refresh rate"*. There is no clock, no
//! scheduler and no `tick()` here — the same boundary CTL-9 draws for the
//! Controller's `WHILE system.running`. The host calls [`MonitorEngine::drive`]
//! when it has a new state; this module supplies the body of one pass, not the
//! loop around it.
//!
//! ## A disclosed limit: history grows
//!
//! Trajectory signals are `σ_k: S^t → {0,1}`, so the engine retains each
//! sustain's observed states. It does **not** truncate, because dropping the
//! oldest observation silently changes what a bounded `Never` can conclude —
//! precisely the class of quiet wrongness the three-valued verdict exists to
//! prevent. [`MonitorEngine::history_len`] exposes the growth so a host can see
//! it and decide, rather than having the decision made invisibly here.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use thiserror::Error;

use crate::detect::{Cusum, CusumSpec, DetectorError, Ewma, Reading, Severity, Watch};
use crate::ooda::{Ooda, OodaError, OodaObservation, OodaPhase, OodaStep};
use crate::pincer::{MonitorReport, PincerError, Signal, SignalMonitor};
use crate::region::{Region, RegionError};
use crate::signal::SignalSpec;

/// One sustain's declared detection parameters (§IX's per-sustain maps).
///
/// `α`, `μ₀`/`δ`/`h` and `θ_fire` are the sustain's **own** declared numbers,
/// the same discipline as the region weights: what counts as a shift worth
/// noticing is a statement about this household, not a constant.
#[derive(Debug, Clone)]
pub struct SustainWatch {
    pub sustain_id: String,
    /// The parent in the declared holarchy, if any. Used only for ordering
    /// here; escalation *through* the holarchy is CTL-5, a separate row.
    pub parent: Option<String>,
    /// Declares `V`, and so where `W = d(s,V)` comes from.
    pub region: Region,
    /// EWMA `α`.
    pub alpha: f64,
    /// CUSUM `μ₀`, `δ`, `h`.
    pub cusum: CusumSpec,
    pub signals: Vec<Signal>,
    pub signal_spec: SignalSpec,
}

impl SustainWatch {
    pub fn new(sustain_id: &str, region: Region, alpha: f64, cusum: CusumSpec) -> Self {
        Self {
            sustain_id: sustain_id.to_string(),
            parent: None,
            region,
            alpha,
            cusum,
            signals: Vec::new(),
            signal_spec: SignalSpec::new(1.0, 3).expect("a real medium"),
        }
    }

    pub fn under(mut self, parent: &str) -> Self {
        self.parent = Some(parent.to_string());
        self
    }

    pub fn watching_for(mut self, signals: Vec<Signal>, spec: SignalSpec) -> Self {
        self.signals = signals;
        self.signal_spec = spec;
        self
    }
}

/// What one pass of the native pipeline produced.
#[derive(Debug, Clone)]
pub struct Ingested {
    pub sustain_id: String,
    /// `W`, the EWMA level, and the CUSUM alert if it crossed.
    pub reading: Reading,
    /// The trajectory signals, evaluated against this sustain's history.
    ///
    /// `None` when this sustain declared none — which is a real configuration
    /// (covered by the CUSUM alone), not a missing result. Fabricating an empty
    /// report would invent a signal pass that never ran.
    pub signals: Option<MonitorReport>,
}

impl Ingested {
    /// The CUSUM severity, or `Info` when nothing crossed.
    ///
    /// `Info` rather than an `Option`: a reading that did not cross is a real
    /// observation with a real severity, not a missing one.
    pub fn severity(&self) -> Severity {
        self.reading.alert.as_ref().map_or(Severity::Info, |a| a.severity)
    }

    /// Does this cross the Monitor→Controller boundary?
    ///
    /// Either the CUSUM escalated or a declared signal fired — the same
    /// condition `OodaObservation::escalates` applies, reused rather than
    /// restated.
    pub fn escalates(&self) -> bool {
        self.reading.escalates()
            || self.signals.as_ref().is_some_and(|s| !s.triggered.is_empty())
    }

    /// **The only route from watching to looking.**
    ///
    /// `None` unless this reading crossed. There is no way to walk a quiet
    /// ingest into OBSERVE through this engine.
    pub fn observation(&self) -> Option<OodaObservation> {
        let obs = match &self.signals {
            Some(report) => OodaObservation::from_monitor(self.severity(), report),
            None => OodaObservation::from_severity(self.severity()),
        };
        obs.escalates().then_some(obs)
    }
}

/// What [`MonitorEngine::drive`] did.
#[derive(Debug, Clone)]
pub struct Driven {
    pub ingested: Ingested,
    /// `Some` only when the reading escalated **and** the loop was at OBSERVE.
    pub step: Option<OodaStep>,
    /// The phase that declined the hand-off, when one did.
    ///
    /// Watching never stops, so a reading arriving mid-cycle is still recorded
    /// — and saying which phase held it back keeps that from looking like the
    /// reading was ignored.
    pub held_back: Option<OodaPhase>,
}

/// The per-sustain object that owns the chain (§IX).
#[derive(Debug)]
pub struct MonitorEngine {
    order: Vec<String>,
    /// **The escalation path** (Controller §V: *"the escalation path IS the
    /// holarchy"*). Validated at construction since this engine shipped;
    /// retained from 2026-08-16, when CTL-5 gave it something to walk.
    parents: BTreeMap<String, String>,
    regions: BTreeMap<String, Region>,
    watches: BTreeMap<String, Watch>,
    monitors: BTreeMap<String, SignalMonitor>,
    histories: BTreeMap<String, Vec<Value>>,
}

impl MonitorEngine {
    /// `flatten_holarchy(sustain_hierarchy)` (§IX).
    ///
    /// Takes the declared sustains and orders them parents-before-children.
    /// Refuses a duplicate id, a parent that was never declared, and a cycle —
    /// a holarchy that contains itself is not a hierarchy, and finding out at
    /// traversal time would be far worse than finding out here.
    ///
    /// **There is no holarchy TYPE in core to walk**, so this takes the
    /// flattened declaration and validates the links rather than pretending to
    /// traverse a tree it was not given.
    pub fn flatten_holarchy(declarations: Vec<SustainWatch>) -> Result<Self, MonitorError> {
        if declarations.is_empty() {
            return Err(MonitorError::NothingToWatch);
        }

        let mut ids = BTreeSet::new();
        for d in &declarations {
            if !ids.insert(d.sustain_id.clone()) {
                return Err(MonitorError::DuplicateSustain(d.sustain_id.clone()));
            }
        }
        for d in &declarations {
            if let Some(p) = &d.parent {
                if !ids.contains(p) {
                    return Err(MonitorError::UnknownParent {
                        sustain: d.sustain_id.clone(),
                        parent: p.clone(),
                    });
                }
            }
        }

        // Parents before children. Anything left over is in a cycle.
        let mut order: Vec<String> = Vec::with_capacity(declarations.len());
        let mut placed: BTreeSet<String> = BTreeSet::new();
        let mut remaining: Vec<&SustainWatch> = declarations.iter().collect();
        while !remaining.is_empty() {
            let (ready, held): (Vec<&SustainWatch>, Vec<&SustainWatch>) = remaining
                .into_iter()
                .partition(|d| d.parent.as_ref().is_none_or(|p| placed.contains(p)));
            if ready.is_empty() {
                let mut cycle: Vec<String> = held.iter().map(|d| d.sustain_id.clone()).collect();
                cycle.sort();
                return Err(MonitorError::HolarchyCycle(cycle));
            }
            for d in ready {
                placed.insert(d.sustain_id.clone());
                order.push(d.sustain_id.clone());
            }
            remaining = held;
        }

        let mut parents = BTreeMap::new();
        let mut regions = BTreeMap::new();
        let mut watches = BTreeMap::new();
        let mut monitors = BTreeMap::new();
        let mut histories = BTreeMap::new();

        for d in declarations {
            let ewma = Ewma::new(d.alpha)?;
            let cusum = Cusum::new(d.cusum)?;
            watches.insert(d.sustain_id.clone(), Watch::new(ewma, cusum));

            if !d.signals.is_empty() {
                monitors.insert(
                    d.sustain_id.clone(),
                    SignalMonitor::new(d.signals, d.signal_spec)?,
                );
            }
            histories.insert(d.sustain_id.clone(), Vec::new());
            if let Some(p) = d.parent {
                parents.insert(d.sustain_id.clone(), p);
            }
            regions.insert(d.sustain_id, d.region);
        }

        Ok(Self { order, parents, regions, watches, monitors, histories })
    }

    /// One sustain, for the common case.
    pub fn watching(declaration: SustainWatch) -> Result<Self, MonitorError> {
        Self::flatten_holarchy(vec![declaration])
    }

    /// Every watched sustain, parents first.
    pub fn sustains(&self) -> &[String] {
        &self.order
    }

    /// The next level up, or `None` at a root.
    ///
    /// The links are acyclic by construction — [`MonitorEngine::flatten_holarchy`]
    /// refuses a cycle — which is what makes walking this chain terminate.
    pub fn parent_of(&self, sustain_id: &str) -> Option<&str> {
        self.parents.get(sustain_id).map(String::as_str)
    }

    /// A **nano-sustain** (§V's base case): no declared children.
    pub fn is_nano(&self, sustain_id: &str) -> bool {
        self.regions.contains_key(sustain_id)
            && !self.parents.values().any(|p| p == sustain_id)
    }

    /// This sustain's own declared region — the `V` its `C(s)` is measured
    /// against.
    pub fn region_of(&self, sustain_id: &str) -> Option<&Region> {
        self.regions.get(sustain_id)
    }

    pub fn watches(&self, sustain_id: &str) -> bool {
        self.regions.contains_key(sustain_id)
    }

    /// How many observations this sustain has accumulated.
    ///
    /// Exposed because the history is **not** truncated — see the module docs
    /// on why silently dropping the oldest would change what a bounded `Never`
    /// concludes.
    pub fn history_len(&self, sustain_id: &str) -> Option<usize> {
        self.histories.get(sustain_id).map(Vec::len)
    }

    /// `ingest(state_update)` (§IX) — the **native** pipeline.
    ///
    /// `W = d(s,V)` → EWMA → CUSUM → `Severity`, plus the trajectory signals
    /// over this sustain's accumulated history. No Kalman stage; see the module
    /// docs.
    pub fn ingest(&mut self, sustain_id: &str, state: &Value) -> Result<Ingested, MonitorError> {
        let region = self
            .regions
            .get(sustain_id)
            .ok_or_else(|| MonitorError::UnknownSustain(sustain_id.to_string()))?;

        // W = d(s, V) — computed on the fold, which is the Sustena-native form.
        let w = region.distance(state)?.weighted;

        let watch = self.watches.get_mut(sustain_id).expect("declared together");
        let reading = watch.observe(w);

        let history = self.histories.get_mut(sustain_id).expect("declared together");
        history.push(state.clone());

        // No declared signals is a real configuration, not a missing result:
        // this sustain is covered by the CUSUM alone. `None` says that; an
        // empty report would claim a signal pass that never ran.
        let signals = match self.monitors.get_mut(sustain_id) {
            Some(m) => Some(m.monitor(history)?),
            None => None,
        };

        Ok(Ingested { sustain_id: sustain_id.to_string(), reading, signals })
    }

    /// **Ingest, and drive the OODA loop's OBSERVE when it escalates.**
    ///
    /// This is what makes OBSERVE self-driving. Watching continues regardless
    /// of where the loop is; the hand-off happens only when the loop is at
    /// OBSERVE and the reading crossed the boundary.
    pub fn drive(
        &mut self,
        ooda: &mut Ooda,
        sustain_id: &str,
        state: &Value,
    ) -> Result<Driven, MonitorError> {
        let ingested = self.ingest(sustain_id, state)?;

        let Some(observation) = ingested.observation() else {
            // Watching, not looking. The chain advanced; nobody was disturbed.
            return Ok(Driven { ingested, step: None, held_back: None });
        };

        if ooda.phase() != OodaPhase::Observe {
            // §III: the Controller only surfaces when the loop reaches DECIDE.
            // A reading arriving mid-cycle is recorded, not forced in.
            let held = ooda.phase();
            return Ok(Driven { ingested, step: None, held_back: Some(held) });
        }

        let step = ooda.observe(observation)?;
        Ok(Driven { ingested, step: Some(step), held_back: None })
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum MonitorError {
    #[error("an engine with no declared sustains would watch nothing and report calm")]
    NothingToWatch,
    #[error("sustain '{0}' is declared twice")]
    DuplicateSustain(String),
    #[error("'{sustain}' declares parent '{parent}', which is not in this holarchy")]
    UnknownParent { sustain: String, parent: String },
    #[error("the declared holarchy has a cycle among {0:?} — a holon that contains itself is not a hierarchy")]
    HolarchyCycle(Vec<String>),
    #[error("'{0}' is not watched by this engine")]
    UnknownSustain(String),
    #[error(transparent)]
    Detector(#[from] DetectorError),
    #[error(transparent)]
    Region(#[from] RegionError),
    #[error(transparent)]
    Signal(#[from] PincerError),
    #[error(transparent)]
    Ooda(#[from] OodaError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pincer::Trajectory;
    use crate::region::Interval;
    use serde_json::json;

    fn region() -> Region {
        Region::new()
            .bounding(Interval::at_least("balance", 1000.0))
            .weighing("balance", 1.0)
    }

    fn state(balance: f64) -> Value {
        json!({ "balance": balance, "pending": 0 })
    }

    /// A state well inside V (so `W = 0` and the CUSUM never stirs) carrying a
    /// dimension the region does not bound at all.
    fn state_pending(pending: i64) -> Value {
        json!({ "balance": 5000.0, "pending": pending })
    }

    /// μ₀ = 0 (a household inside V has W = 0), δ = 50, h = 100.
    fn cusum() -> CusumSpec {
        CusumSpec::new(0.0, 50.0, 100.0)
    }

    fn decl(id: &str) -> SustainWatch {
        SustainWatch::new(id, region(), 0.5, cusum())
    }

    // ── the engine owns the chain ───────────────────────────────────────────

    #[test]
    fn an_engine_with_no_sustains_is_refused() {
        assert!(matches!(
            MonitorEngine::flatten_holarchy(vec![]),
            Err(MonitorError::NothingToWatch)
        ));
    }

    #[test]
    fn flatten_holarchy_orders_parents_before_children() {
        let e = MonitorEngine::flatten_holarchy(vec![
            decl("child").under("household"),
            decl("household"),
            decl("grandchild").under("child"),
        ])
        .unwrap();
        assert_eq!(e.sustains(), ["household", "child", "grandchild"]);
    }

    #[test]
    fn a_parent_that_was_never_declared_is_refused() {
        assert!(matches!(
            MonitorEngine::flatten_holarchy(vec![decl("child").under("ghost")]),
            Err(MonitorError::UnknownParent { .. })
        ));
    }

    #[test]
    fn a_cycle_in_the_declared_holarchy_is_refused() {
        let err = MonitorEngine::flatten_holarchy(vec![
            decl("a").under("b"),
            decl("b").under("a"),
        ])
        .unwrap_err();
        assert!(matches!(err, MonitorError::HolarchyCycle(_)), "got {err:?}");
        assert!(err.to_string().contains("contains itself"));
    }

    #[test]
    fn a_duplicate_sustain_is_refused() {
        assert!(matches!(
            MonitorEngine::flatten_holarchy(vec![decl("a"), decl("a")]),
            Err(MonitorError::DuplicateSustain(_))
        ));
    }

    #[test]
    fn each_sustain_keeps_its_own_chain() {
        // The point of per-sustain ownership: one household's drift must not
        // move another's accumulator.
        let mut e = MonitorEngine::flatten_holarchy(vec![decl("a"), decl("b")]).unwrap();
        for _ in 0..5 {
            e.ingest("a", &state(0.0)).unwrap();
        }
        e.ingest("b", &state(0.0)).unwrap();

        assert_eq!(e.history_len("a"), Some(5));
        assert_eq!(e.history_len("b"), Some(1));
    }

    #[test]
    fn an_unwatched_sustain_is_refused() {
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        assert!(matches!(
            e.ingest("elsewhere", &state(0.0)),
            Err(MonitorError::UnknownSustain(_))
        ));
    }

    // ── §IX — the native pipeline ───────────────────────────────────────────

    #[test]
    fn ingest_computes_w_as_distance_to_v_not_the_raw_state() {
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        // Inside V: W = 0, whatever the balance is.
        assert_eq!(e.ingest("a", &state(5000.0)).unwrap().reading.w, 0.0);
        // Outside: W is the shortfall, not the balance.
        assert_eq!(e.ingest("a", &state(600.0)).unwrap().reading.w, 400.0);
    }

    #[test]
    fn the_ewma_smooths_across_ingests_because_the_engine_owns_it() {
        // The chain has STATE, and the engine is what carries it between
        // observations — the thing that was missing before this slice.
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        let first = e.ingest("a", &state(1000.0)).unwrap().reading.smoothed;
        assert_eq!(first, 0.0, "the first observation seeds the level");

        let second = e.ingest("a", &state(0.0)).unwrap().reading.smoothed;
        assert_eq!(second, 500.0, "α = 0.5, so halfway toward the new level");
        assert!(second > first);
    }

    #[test]
    fn a_quiet_stream_does_not_escalate() {
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        for _ in 0..10 {
            let i = e.ingest("a", &state(5000.0)).unwrap();
            assert!(!i.escalates());
            assert_eq!(i.severity(), Severity::Info);
            assert!(i.observation().is_none(), "★ watching, never looking");
        }
    }

    #[test]
    fn a_sustained_drift_eventually_escalates() {
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        let mut escalated = false;
        for _ in 0..12 {
            // Well outside V, held there — exactly what CUSUM is for.
            if e.ingest("a", &state(0.0)).unwrap().escalates() {
                escalated = true;
                break;
            }
        }
        assert!(escalated, "a sustained shift must cross h eventually");
    }

    // ── ★ OBSERVE is self-driving ───────────────────────────────────────────

    #[test]
    fn a_quiet_ingest_leaves_the_loop_untouched() {
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        let mut o = Ooda::new();
        let d = e.drive(&mut o, "a", &state(5000.0)).unwrap();

        assert!(d.step.is_none());
        assert!(d.held_back.is_none(), "nothing was held back — nothing tried to go");
        assert_eq!(o.phase(), OodaPhase::Observe);
    }

    #[test]
    fn an_escalating_ingest_drives_the_loop_into_orient() {
        // ★ THE PROOF OF VALUE. A stream of states through ONE engine advances
        // the chain until the CUSUM crosses, and that crossing walks the OODA
        // loop out of OBSERVE without anyone calling observe() by hand.
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        let mut o = Ooda::new();

        let mut drove = false;
        for _ in 0..12 {
            let d = e.drive(&mut o, "a", &state(0.0)).unwrap();
            if d.step.is_some() {
                drove = true;
                break;
            }
        }
        assert!(drove, "★ the engine drove OBSERVE itself");
        assert_eq!(o.phase(), OodaPhase::Orient);
    }

    #[test]
    fn a_signal_alone_drives_the_loop_even_without_a_cusum_crossing() {
        // The state is comfortably INSIDE V, so W = 0 and the CUSUM never
        // stirs. The signal watches `pending`, a dimension the region does not
        // bound at all — which is precisely why escalation has two inputs: a
        // distance cannot measure what V does not describe.
        let mut e = MonitorEngine::watching(decl("a").watching_for(
            vec![Signal::new("backlog", Trajectory::ever_within(1, "pending >= 3"))],
            SignalSpec::new(1.0, 5).unwrap(),
        ))
        .unwrap();
        let mut o = Ooda::new();

        let d = e.drive(&mut o, "a", &state_pending(9)).unwrap();
        assert_eq!(d.ingested.reading.w, 0.0, "inside V — distance-to-V sees nothing");
        assert!(!d.ingested.reading.escalates(), "so the CUSUM has nothing to cross on");
        assert!(d.ingested.escalates(), "★ but a declared signal fired");
        assert!(d.step.is_some(), "★ and that alone drives OBSERVE");
        assert_eq!(o.phase(), OodaPhase::Orient);
    }

    #[test]
    fn watching_continues_while_the_loop_is_busy_and_says_who_held_it() {
        // ★ §III: the Monitor is always running; the Controller only surfaces
        // when the loop reaches DECIDE. A reading arriving mid-cycle must be
        // RECORDED, not dropped and not forced in.
        //
        // Driven by the CUSUM rather than a signal, deliberately: a signal is
        // debounced by its refractory window (see the test below), so it would
        // not still be escalating on the second pass and there would be
        // nothing to hold back.
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        let mut o = Ooda::new();

        e.drive(&mut o, "a", &state(0.0)).unwrap();
        assert_eq!(o.phase(), OodaPhase::Orient, "the loop has moved on");

        let before = e.history_len("a").unwrap();
        let d = e.drive(&mut o, "a", &state(0.0)).unwrap();

        assert!(d.ingested.escalates(), "still escalating");
        assert_eq!(e.history_len("a"), Some(before + 1), "★ the watching never stopped");
        assert!(d.step.is_none(), "and it did not barge into a busy loop");
        assert_eq!(d.held_back, Some(OodaPhase::Orient), "★ and it says which phase held it");
    }

    #[test]
    fn a_persistent_signal_does_not_repeatedly_drive_observe() {
        // The pincer's refractory debounce, doing its job one layer up. A
        // condition that stays true does not re-drive OBSERVE on every single
        // ingest — which is the difference between a monitor and an alarm that
        // will not stop.
        let mut e = MonitorEngine::watching(decl("a").watching_for(
            vec![Signal::new("backlog", Trajectory::ever_within(1, "pending >= 3"))],
            SignalSpec::new(1.0, 5).unwrap(),
        ))
        .unwrap();
        let mut o = Ooda::new();

        let first = e.drive(&mut o, "a", &state_pending(9)).unwrap();
        assert!(first.step.is_some(), "the first crossing drives OBSERVE");

        let second = e.drive(&mut o, "a", &state_pending(12)).unwrap();
        assert!(!second.ingested.escalates(), "★ the same condition is debounced, not re-fired");
        assert!(second.step.is_none());
        assert!(
            second.held_back.is_none(),
            "and nothing was held back — nothing tried to go in the first place"
        );
        assert_eq!(
            second.ingested.signals.as_ref().unwrap().debounced,
            ["backlog"],
            "the suppression stays VISIBLE all the way up here"
        );
    }

    #[test]
    fn the_history_is_not_truncated() {
        // Disclosed rather than silently bounded: dropping the oldest would
        // change what a bounded `Never` can conclude.
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        for _ in 0..50 {
            e.ingest("a", &state(5000.0)).unwrap();
        }
        assert_eq!(e.history_len("a"), Some(50));
    }

    // ── the declined import ─────────────────────────────────────────────────

    #[test]
    fn there_is_no_kalman_stage() {
        // §IX runs Kalman → EWMA → CUSUM. The Additions' fit caveat says the
        // Kalman apparatus assumes a continuous linear ODE that discrete
        // event-sourced state is not, so the native form replaces it. What
        // reaches the EWMA is W itself — unfiltered, and equal to d(s,V).
        let mut e = MonitorEngine::watching(decl("a")).unwrap();
        let r = e.ingest("a", &state(600.0)).unwrap().reading;
        assert_eq!(r.w, 400.0);
        assert_eq!(
            r.smoothed, r.w,
            "the first observation seeds the EWMA directly from W — nothing estimated it first"
        );
    }
}
