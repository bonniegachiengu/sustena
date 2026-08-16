//! Holarchic escalation — a violation propagates upward until a level can act
//! (Controller §V · CTL-5).
//!
//! > Koestler's holon: a system that is simultaneously a complete whole AND a
//! > component of a larger whole. [...] A nano-sustain soil sensor breaching
//! > its moisture threshold is an event — if unresolved, it escalates to the
//! > garden sustain, then to Homestead. **The escalation path IS the
//! > holarchy.**
//!
//! So there is no escalation graph here. The path is the parent chain the
//! [`MonitorEngine`] already validated at construction, and this module is the
//! upward walk over it plus the question asked at each stop.
//!
//! ## The question at each level
//!
//! *Can this level act — is there an admissible intervention it could surface?*
//!
//! [`Levels::capacity`] answers it, and the caller implements it: what a level
//! can do comes from its operators and its policy, which is not something this
//! module gets to invent. Two answers, and the asymmetry is the point:
//!
//! - [`Capacity::CanAct`] carries the candidates. The walk stops.
//! - [`Capacity::CannotAct`] **requires a reason**. A level cannot decline
//!   silently, because a breach absorbed without explanation is exactly the
//!   failure §V describes — something went wrong at a depth nobody was
//!   watching, and nothing said so.
//!
//! ## Reaching the root unresolved is an ANSWER, not an error
//!
//! [`EscalationOutcome::ExhaustedAtRoot`] is a normal outcome. A breach that climbed the
//! whole holarchy and found no level able to act is the single most important
//! thing this system can say, and returning `Err` for it would let a caller
//! `?` it away into a log line. It comes back as a result, with the full path,
//! and every level's stated reason for passing it on.
//!
//! ## An escalation cannot be started from a non-breach
//!
//! [`Breach::from_ingested`] returns `None` unless the reading actually crossed
//! the Monitor→Controller boundary — the same discipline as
//! `Ingested::observation`. There is no constructor that takes a severity and a
//! sentence, so a caller cannot manufacture an escalation and walk it up
//! somebody's holarchy.
//!
//! ## The invariant (§V), and one honest interpretation
//!
//! ```text
//! ∀ s ∈ Sustain,  C(s) = True  ∧  s.s ⊆ parent(s).s
//! ```
//!
//! [`validate_holarchy`] checks both clauses at every level. The first is
//! direct: is the state inside its own declared `V`? The second is not — **§V
//! never defines `⊆`**, and inventing a containment relation would be putting
//! words in the article's mouth. What is implemented is the reading available
//! through the machinery that exists: a child must also satisfy its parent's
//! region **on the dimensions the parent bounds**. That is a real, checkable
//! sense of *"consistent with its parent's state"*, and it is recorded here as
//! an interpretation rather than presented as the article's own definition.
//!
//! ## Termination
//!
//! The chain is acyclic because `MonitorEngine::flatten_holarchy` refuses a
//! cycle at construction — a guarantee the monitor slice already earned, relied
//! on here rather than re-checked. The walk still carries a hard bound equal to
//! the number of watched sustains, as a backstop that should be unreachable;
//! [`HolarchyError::PathTooLong`] says so if it ever fires.

use serde_json::Value;
use thiserror::Error;

use crate::detect::Severity;
use crate::monitor::{Ingested, MonitorEngine};
use crate::ooda::{Candidate, OodaObservation};
use crate::region::RegionError;

/// The violation that starts a climb.
///
/// Constructible only from a real [`Ingested`] that actually escalated.
#[derive(Debug, Clone, PartialEq)]
pub struct Breach {
    /// Where it was detected — often a nano-sustain.
    pub origin: String,
    pub severity: Severity,
    /// `W = d(s,V)` at the origin.
    pub w: f64,
    /// The signals that fired, if any.
    pub signals: Vec<String>,
}

impl Breach {
    /// `None` unless the reading crossed the Monitor→Controller boundary.
    ///
    /// There is no constructor from a severity and a sentence: an escalation
    /// has to have come from something that really happened.
    pub fn from_ingested(ingested: &Ingested) -> Option<Self> {
        ingested.escalates().then(|| Self {
            origin: ingested.sustain_id.clone(),
            severity: ingested.severity(),
            w: ingested.reading.w,
            signals: ingested
                .signals
                .as_ref()
                .map(|s| s.triggered.iter().map(|t| t.signal.clone()).collect())
                .unwrap_or_default(),
        })
    }

    pub fn describe(&self) -> String {
        let sig = if self.signals.is_empty() {
            String::new()
        } else {
            format!("; signals {}", self.signals.join(", "))
        };
        format!("{:?} at '{}' (W = {}){sig}", self.severity, self.origin, self.w)
    }
}

/// What a level can offer toward resolving a breach.
#[derive(Debug, Clone, PartialEq)]
pub enum Capacity {
    /// This level has admissible interventions it could surface. The walk stops
    /// here.
    CanAct(Vec<Candidate>),
    /// This level has nothing admissible for this breach.
    ///
    /// **The reason is required.** A level that absorbs a breach without saying
    /// why is the failure §V exists to prevent.
    CannotAct { reason: String },
}

impl Capacity {
    pub fn cannot(reason: &str) -> Self {
        Capacity::CannotAct { reason: reason.to_string() }
    }
}

/// Asked at each level on the way up.
///
/// The caller implements it, because what a level can do comes from its
/// operators and its policy — not from anything this module could know.
pub trait Levels {
    fn capacity(&mut self, sustain_id: &str, breach: &Breach) -> Capacity;
}

/// One stop on the climb.
#[derive(Debug, Clone, PartialEq)]
pub struct Hop {
    pub sustain_id: String,
    /// `0` at the origin.
    pub depth: usize,
    /// Why this level passed it on. `None` at the level that acted.
    pub declined: Option<String>,
}

/// Where the climb ended.
#[derive(Debug, Clone, PartialEq)]
pub enum EscalationOutcome {
    /// A level could act. Its candidates are carried so the caller can drive
    /// that level's OODA loop with them.
    ActedAt { sustain_id: String, candidates: Vec<Candidate> },
    /// The climb reached a root and no level could act.
    ///
    /// A normal outcome, and the most important one this system produces.
    ExhaustedAtRoot { root: String },
}

/// A completed climb.
#[derive(Debug, Clone, PartialEq)]
pub struct Escalation {
    pub breach: Breach,
    /// Every level it passed through, origin first — the provenance a decision
    /// at the top needs in order to be about the sensor at the bottom.
    pub path: Vec<Hop>,
    pub outcome: EscalationOutcome,
}

impl Escalation {
    pub fn acted_at(&self) -> Option<&str> {
        match &self.outcome {
            EscalationOutcome::ActedAt { sustain_id, .. } => Some(sustain_id),
            EscalationOutcome::ExhaustedAtRoot { .. } => None,
        }
    }

    /// How many levels it climbed. `0` when the origin itself could act.
    pub fn levels_climbed(&self) -> usize {
        self.path.len().saturating_sub(1)
    }

    /// The observation to hand the acting level's OODA loop.
    ///
    /// Carries the **origin's** severity and signals, not the acting level's:
    /// a garden-level decision about a soil sensor has to be about the sensor.
    pub fn observation(&self) -> OodaObservation {
        OodaObservation::from_severity(self.breach.severity)
    }

    pub fn describe(&self) -> String {
        let route: Vec<&str> = self.path.iter().map(|h| h.sustain_id.as_str()).collect();
        match &self.outcome {
            EscalationOutcome::ActedAt { sustain_id, candidates } => format!(
                "{} → {} ({} option(s)) via {}",
                self.breach.describe(),
                sustain_id,
                candidates.len(),
                route.join(" → ")
            ),
            EscalationOutcome::ExhaustedAtRoot { root } => format!(
                "{} reached the root '{}' with no level able to act, via {}",
                self.breach.describe(),
                root,
                route.join(" → ")
            ),
        }
    }
}

/// **Walk the holarchy upward until a level can act** (§V).
///
/// The path is the [`MonitorEngine`]'s parent chain — validated when the engine
/// was built, walked here.
pub fn escalate<L: Levels>(
    engine: &MonitorEngine,
    levels: &mut L,
    breach: Breach,
) -> Result<Escalation, HolarchyError> {
    if !engine.watches(&breach.origin) {
        return Err(HolarchyError::UnknownSustain(breach.origin.clone()));
    }

    let bound = engine.sustains().len();
    let mut path: Vec<Hop> = Vec::new();
    let mut here = breach.origin.clone();

    for depth in 0..=bound {
        if depth == bound {
            // Unreachable given flatten_holarchy's construction-time cycle
            // refusal. Kept as a backstop that names itself rather than looping.
            return Err(HolarchyError::PathTooLong { from: breach.origin, bound });
        }

        match levels.capacity(&here, &breach) {
            Capacity::CanAct(candidates) => {
                path.push(Hop { sustain_id: here.clone(), depth, declined: None });
                return Ok(Escalation {
                    breach,
                    path,
                    outcome: EscalationOutcome::ActedAt { sustain_id: here, candidates },
                });
            }
            Capacity::CannotAct { reason } => {
                path.push(Hop { sustain_id: here.clone(), depth, declined: Some(reason) });
                match engine.parent_of(&here) {
                    Some(parent) => here = parent.to_string(),
                    None => {
                        // The top, with nobody able to act. An answer, not an
                        // error — and the loudest one available.
                        return Ok(Escalation {
                            breach,
                            path,
                            outcome: EscalationOutcome::ExhaustedAtRoot { root: here },
                        });
                    }
                }
            }
        }
    }

    unreachable!("the loop returns on every path")
}

// ── §V's invariant ──────────────────────────────────────────────────────────

/// One level's standing against the holarchy invariant.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelCheck {
    pub sustain_id: String,
    /// `C(s) = True` — is the state inside this level's **own** declared `V`?
    pub local_ok: bool,
    /// `s ⊆ parent(s)` — read as: does it also satisfy the parent's region on
    /// the dimensions the parent bounds? `None` at a root.
    ///
    /// **§V does not define `⊆`**; this is the reading available through the
    /// machinery that exists, recorded as an interpretation rather than as the
    /// article's own definition.
    pub consistent_with_parent: Option<bool>,
    /// `W` against its own region.
    pub w: f64,
}

impl LevelCheck {
    pub fn holds(&self) -> bool {
        self.local_ok && self.consistent_with_parent.unwrap_or(true)
    }
}

/// `validate_holarchy` (§V) — both clauses, at every level.
#[derive(Debug, Clone, PartialEq)]
pub struct HolarchyReport {
    pub levels: Vec<LevelCheck>,
}

impl HolarchyReport {
    pub fn holds(&self) -> bool {
        self.levels.iter().all(LevelCheck::holds)
    }

    /// The levels that failed, deepest first — where to start looking.
    pub fn violations(&self) -> Vec<&LevelCheck> {
        let mut v: Vec<&LevelCheck> = self.levels.iter().filter(|l| !l.holds()).collect();
        v.reverse();
        v
    }
}

/// Check `C(s) = True ∧ s ⊆ parent(s)` across the whole declared holarchy.
///
/// A sustain with no supplied state is **skipped and not counted as passing** —
/// it simply does not appear in the report, because "we did not look" and "we
/// looked and it was fine" are different claims.
pub fn validate_holarchy(
    engine: &MonitorEngine,
    states: &[(&str, &Value)],
) -> Result<HolarchyReport, HolarchyError> {
    let mut levels = Vec::new();

    for (id, state) in states {
        let region = engine
            .region_of(id)
            .ok_or_else(|| HolarchyError::UnknownSustain((*id).to_string()))?;
        let d = region.distance(state)?;

        let consistent_with_parent = match engine.parent_of(id) {
            None => None,
            Some(parent) => {
                let pr = engine
                    .region_of(parent)
                    .ok_or_else(|| HolarchyError::UnknownSustain(parent.to_string()))?;
                // The interpretation: the child must also sit inside the
                // parent's region on the dimensions the parent bounds.
                Some(pr.distance(state)?.weighted <= 0.0)
            }
        };

        levels.push(LevelCheck {
            sustain_id: (*id).to_string(),
            local_ok: d.weighted <= 0.0,
            consistent_with_parent,
            w: d.weighted,
        });
    }

    Ok(HolarchyReport { levels })
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum HolarchyError {
    #[error("'{0}' is not watched by this engine, so it has no place in the holarchy")]
    UnknownSustain(String),
    #[error("the climb from '{from}' exceeded {bound} levels — unreachable given flatten_holarchy's cycle refusal, so this means the parent links were built elsewhere")]
    PathTooLong { from: String, bound: usize },
    #[error(transparent)]
    Region(#[from] RegionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::ControlEvent;
    use crate::detect::CusumSpec;
    use crate::monitor::SustainWatch;
    use crate::region::{Interval, Region};
    use serde_json::json;
    use std::collections::BTreeMap;

    /// §V's own example: a soil sensor inside a garden inside a homestead.
    fn holarchy() -> MonitorEngine {
        MonitorEngine::flatten_holarchy(vec![
            watch("homestead", 0.0),
            watch("garden", 0.0).under("homestead"),
            watch("soil_sensor", 30.0).under("garden"),
        ])
        .expect("a well-formed holarchy")
    }

    fn watch(id: &str, floor: f64) -> SustainWatch {
        SustainWatch::new(
            id,
            Region::new().bounding(Interval::at_least("moisture", floor)).weighing("moisture", 1.0),
            0.5,
            CusumSpec::new(0.0, 5.0, 10.0),
        )
    }

    fn moisture(v: f64) -> Value {
        json!({ "moisture": v })
    }

    /// A caller-supplied capacity table: who can do what about this breach.
    struct Table(BTreeMap<String, Capacity>);

    impl Table {
        fn new(entries: &[(&str, Capacity)]) -> Self {
            Self(entries.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
        }
    }

    impl Levels for Table {
        fn capacity(&mut self, sustain_id: &str, _breach: &Breach) -> Capacity {
            self.0
                .get(sustain_id)
                .cloned()
                .unwrap_or_else(|| Capacity::cannot("no declared capacity at this level"))
        }
    }

    fn can_act(op: &str) -> Capacity {
        Capacity::CanAct(vec![Candidate::new(ControlEvent::new("IRRIGATE", op), "wet")])
    }

    fn breach_at(engine: &mut MonitorEngine, id: &str, v: f64) -> Breach {
        let ingested = engine.ingest(id, &moisture(v)).expect("watched");
        Breach::from_ingested(&ingested).expect("this reading escalates")
    }

    // ── the path IS the holarchy ────────────────────────────────────────────

    #[test]
    fn the_parent_chain_is_the_escalation_path() {
        let e = holarchy();
        assert_eq!(e.parent_of("soil_sensor"), Some("garden"));
        assert_eq!(e.parent_of("garden"), Some("homestead"));
        assert_eq!(e.parent_of("homestead"), None, "the top of the holarchy");
    }

    #[test]
    fn a_nano_sustain_is_the_base_case() {
        let e = holarchy();
        assert!(e.is_nano("soil_sensor"), "§V: |children| = 0");
        assert!(!e.is_nano("garden"));
        assert!(!e.is_nano("homestead"));
    }

    #[test]
    fn an_escalation_cannot_be_started_from_a_non_breach() {
        let mut e = holarchy();
        // Comfortably inside V: W = 0, nothing crossed.
        let quiet = e.ingest("soil_sensor", &moisture(90.0)).unwrap();
        assert!(Breach::from_ingested(&quiet).is_none(), "★ no manufactured escalations");
    }

    // ── ★ the climb ─────────────────────────────────────────────────────────

    #[test]
    fn a_nano_sustain_breach_climbs_to_the_level_that_can_act() {
        // ★ THE PROOF OF VALUE, and §V's own example. The sensor cannot water
        // itself; the garden cannot authorise the spend; the homestead can.
        let mut e = holarchy();
        let breach = breach_at(&mut e, "soil_sensor", 0.0);

        let mut levels = Table::new(&[
            ("soil_sensor", Capacity::cannot("a sensor can measure, not irrigate")),
            ("garden", Capacity::cannot("no unallocated water budget at this level")),
            ("homestead", can_act("budget.allocate")),
        ]);

        let esc = escalate(&e, &mut levels, breach).unwrap();

        assert_eq!(esc.acted_at(), Some("homestead"));
        assert_eq!(esc.levels_climbed(), 2, "sensor → garden → homestead");
        let route: Vec<&str> = esc.path.iter().map(|h| h.sustain_id.as_str()).collect();
        assert_eq!(route, ["soil_sensor", "garden", "homestead"]);
    }

    #[test]
    fn every_level_that_passed_it_on_said_why() {
        let mut e = holarchy();
        let breach = breach_at(&mut e, "soil_sensor", 0.0);
        let mut levels = Table::new(&[
            ("soil_sensor", Capacity::cannot("a sensor can measure, not irrigate")),
            ("garden", Capacity::cannot("no unallocated water budget at this level")),
            ("homestead", can_act("budget.allocate")),
        ]);

        let esc = escalate(&e, &mut levels, breach).unwrap();
        let declined: Vec<&str> =
            esc.path.iter().filter_map(|h| h.declined.as_deref()).collect();

        assert_eq!(declined.len(), 2, "★ a level cannot absorb a breach silently");
        assert!(declined[0].contains("measure, not irrigate"));
        assert!(declined[1].contains("water budget"));
        assert!(esc.path.last().unwrap().declined.is_none(), "the acting level declined nothing");
    }

    #[test]
    fn a_level_that_can_act_stops_the_climb_immediately() {
        let mut e = holarchy();
        let breach = breach_at(&mut e, "soil_sensor", 0.0);
        let mut levels = Table::new(&[("soil_sensor", can_act("iot.open_valve"))]);

        let esc = escalate(&e, &mut levels, breach).unwrap();
        assert_eq!(esc.acted_at(), Some("soil_sensor"));
        assert_eq!(esc.levels_climbed(), 0, "resolved where it happened");
        assert_eq!(esc.path.len(), 1, "and nothing above was disturbed");
    }

    #[test]
    fn reaching_the_root_unresolved_is_an_answer_not_an_error() {
        // ★ The loudest thing this system can say. Returning Err would let a
        // caller `?` it away into a log line.
        let mut e = holarchy();
        let breach = breach_at(&mut e, "soil_sensor", 0.0);
        let mut levels = Table::new(&[]); // nobody declared any capacity

        let esc = escalate(&e, &mut levels, breach).expect("an outcome, not a failure");

        match &esc.outcome {
            EscalationOutcome::ExhaustedAtRoot { root } => assert_eq!(root, "homestead"),
            other => panic!("expected ExhaustedAtRoot, got {other:?}"),
        }
        assert_eq!(esc.path.len(), 3, "and the whole path is still reported");
        assert!(esc.describe().contains("no level able to act"));
    }

    #[test]
    fn the_observation_carries_the_origins_severity_not_the_acting_levels() {
        let mut e = holarchy();
        let breach = breach_at(&mut e, "soil_sensor", 0.0);
        let severity = breach.severity;
        let mut levels = Table::new(&[("homestead", can_act("budget.allocate"))]);

        let esc = escalate(&e, &mut levels, breach).unwrap();
        assert_eq!(
            esc.observation().severity(),
            severity,
            "★ a homestead decision about a soil sensor must be about the sensor"
        );
        assert!(esc.observation().escalates(), "and it still crosses the boundary up there");
    }

    #[test]
    fn escalating_from_an_unwatched_sustain_is_refused() {
        let e = holarchy();
        let breach = Breach {
            origin: "elsewhere".into(),
            severity: Severity::Critical,
            w: 1.0,
            signals: vec![],
        };
        let mut levels = Table::new(&[]);
        assert!(matches!(
            escalate(&e, &mut levels, breach),
            Err(HolarchyError::UnknownSustain(_))
        ));
    }

    // ── §V's invariant ──────────────────────────────────────────────────────

    #[test]
    fn the_invariant_holds_when_every_level_is_inside_its_own_region() {
        let e = holarchy();
        let report = validate_holarchy(
            &e,
            &[
                ("homestead", &moisture(50.0)),
                ("garden", &moisture(50.0)),
                ("soil_sensor", &moisture(50.0)),
            ],
        )
        .unwrap();
        assert!(report.holds());
        assert!(report.violations().is_empty());
    }

    #[test]
    fn a_local_breach_is_reported_at_its_own_level() {
        let e = holarchy();
        // The sensor's own floor is 30; 10 breaches it. The garden's floor is
        // 0, so the parent clause still holds — the two clauses are separate.
        let report =
            validate_holarchy(&e, &[("garden", &moisture(50.0)), ("soil_sensor", &moisture(10.0))])
                .unwrap();

        assert!(!report.holds());
        let v = report.violations();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].sustain_id, "soil_sensor");
        assert!(!v[0].local_ok, "C(s) fails");
        assert_eq!(v[0].consistent_with_parent, Some(true), "but it is still within the parent's");
        assert_eq!(v[0].w, 20.0);
    }

    #[test]
    fn the_parent_clause_can_fail_on_its_own() {
        // A level inside its OWN region but outside its parent's — the second
        // clause of the invariant, doing work the first cannot.
        let e = MonitorEngine::flatten_holarchy(vec![
            watch("household", 100.0),
            watch("pocket", 10.0).under("household"),
        ])
        .unwrap();

        let report = validate_holarchy(&e, &[("pocket", &moisture(50.0))]).unwrap();
        let l = &report.levels[0];
        assert!(l.local_ok, "50 clears the pocket's own floor of 10");
        assert_eq!(l.consistent_with_parent, Some(false), "★ but not the household's 100");
        assert!(!report.holds());
    }

    #[test]
    fn a_root_has_no_parent_clause_to_satisfy() {
        let e = holarchy();
        let report = validate_holarchy(&e, &[("homestead", &moisture(50.0))]).unwrap();
        assert_eq!(report.levels[0].consistent_with_parent, None, "nothing above it to agree with");
        assert!(report.holds());
    }

    #[test]
    fn a_level_with_no_supplied_state_is_skipped_not_passed() {
        // "We did not look" and "we looked and it was fine" are different
        // claims, and a report that conflated them would be the quiet kind of
        // wrong.
        let e = holarchy();
        let report = validate_holarchy(&e, &[("garden", &moisture(50.0))]).unwrap();
        assert_eq!(report.levels.len(), 1, "only what was actually checked appears");
        assert_eq!(report.levels[0].sustain_id, "garden");
    }
}
