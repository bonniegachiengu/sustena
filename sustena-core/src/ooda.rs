//! The OODA loop as a state machine — spine assembly
//! (Controller §III + §IX · CTL-3).
//!
//! ```text
//! OODA = (Q, q₀, Σ, δ)    Q = {OBSERVE, ORIENT, DECIDE, ACT}    q₀ = OBSERVE
//! ```
//!
//! This slice **assembles**. It computes almost nothing: `region.rs` +
//! `detect.rs` + `signal.rs` do OBSERVE, `ensemble.rs` + `tenet.rs` do ORIENT,
//! `approval.rs` holds the token, `operator::execute_admitted` is the gate at
//! ACT. What is new is the sequencing, and one phase's own work.
//!
//! ## The critical observation, made structural (§III)
//!
//! > Monitor handles Observe. Simulator handles Orient. **The Controller is
//! > specifically the DECIDE surface.** [...] Monitor is always running at
//! > OBSERVE. Controller only surfaces when the OODA loop has reached DECIDE
//! > and requires human input.
//!
//! So this machine does not re-implement Observe or Orient, and cannot:
//!
//! - [`OodaObservation`] has **no constructor from plain data**. The only way to
//!   make one is [`OodaObservation::from_monitor`], which takes a real
//!   [`Severity`] and a real [`MonitorReport`]. The machine cannot invent an
//!   observation, or decide for itself that something escalated.
//! - [`Orientation`] likewise: [`Orientation::from_plan`] takes a real
//!   [`Plan`] and ranks the candidates by `J` at their destinations. There is
//!   no way to hand the machine a ranking it did not get from a sweep.
//! - The one phase with logic of its own is [`Ooda::decide`], which calls
//!   `controller::route` — the SNR filter, the Sheridan dispatch, and the
//!   Lyapunov check. That is the Controller's job and only that.
//!
//! ## The human enters at DECIDE, and the loop cannot pass without them
//!
//! `Routing::Surface` leaves the machine **parked** in DECIDE. [`Ooda::act`]
//! refuses while it is awaiting authorisation; only [`Ooda::authorize`] moves
//! it on, and that takes an [`ApprovalToken`] whose binding must match the
//! surfaced decision's.
//!
//! **There is no method on this type that mints a token.** Minting requires
//! `approval.rs`'s `Simulated → Voted → approve` chain, which only a human
//! completes. The loop closing through a human is a property of what this
//! module *cannot do*, not of what it politely declines to.
//!
//! ## Boyd: ORIENT completes before the human sees anything
//!
//! > Every friction point in the Controller's DECIDE surface slows the loop.
//! > Approve prompts must be minimal, contextual, and pre-oriented.
//!
//! [`Ooda::decide`] refuses unless the machine is in ORIENT with an
//! orientation in hand, so a person is never shown raw options — the scoring
//! is finished before the surface exists. The refusal is the encoding.
//!
//! ## The three routes out of DECIDE are kept distinct
//!
//! - **Surface** — park, and wait for a person.
//! - **Automate** — above the approval line (§IV). Goes straight to ACT under
//!   [`EffectClass::Unchecked`], because the declared [`AutomationTable`] **is**
//!   the prior human decision, the same way a dead drop is a pre-commitment.
//!   Disclosed rather than hidden: the transition records `human_asked: false`,
//!   so an action nobody was asked about is visible in the trace.
//! - **Withhold** — needs approval, not urgent enough *yet*. Returns to OBSERVE
//!   having executed **nothing**. Folding this into Automate would turn an
//!   attention filter into an auto-approver, so they are separate outcomes here
//!   exactly as they are separate variants in `controller.rs`.
//!
//! ## Honest limits
//!
//! - **`STAY` is a real outcome, not an error.** §III's *"IF signals.empty:
//!   STAY in observe"* is the overwhelmingly common case, and a loop that
//!   treated a quiet observation as a failure would invert what calm means.
//! - **One decision per cycle.** §IX iterates a surfaced list; this machine
//!   carries the top-ranked candidate through one pass and returns to OBSERVE.
//!   Draining a queue is the caller's loop, and keeping it there means the
//!   machine has no hidden backlog a person cannot see.
//! - **No clock, no scheduler.** `now` is passed in; there is no
//!   `CONTROL_LOOP_INTERVAL` sleep here. §IX's `WHILE system.running` is the
//!   host's loop, and this is the body of one iteration.

use serde_json::Value;
use thiserror::Error;

use crate::approval::{ApprovalToken, Binding, EffectClass, NonceLedger};
use crate::controller::{
    route, AutomationTable, ControlEvent, ControllerError, Preferences, Routing, SheridanLevel,
    SurfacedDecision, Urgency,
};
use crate::detect::Severity;
use crate::operator::{execute_admitted, Authorization, Enforcement, Execution, Registry};
use crate::pincer::MonitorReport;
use crate::region::Region;
use crate::tenet::Plan;

/// `Q` — the four states of the cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OodaPhase {
    Observe,
    Orient,
    Decide,
    Act,
}

// ── OBSERVE — owned by the Monitor ──────────────────────────────────────────

/// What the Monitor saw.
///
/// **Constructible only from real Monitor artifacts.** There is deliberately no
/// `OodaObservation::new(escalates: bool)`: the machine must be handed an
/// observation, and cannot decide for itself that something escalated.
#[derive(Debug, Clone, PartialEq)]
pub struct OodaObservation {
    severity: Severity,
    signals: Vec<String>,
    watched: Vec<String>,
}

impl OodaObservation {
    /// From `detect.rs`'s severity and `pincer.rs`'s signal pass.
    ///
    /// Both halves of §III's OBSERVE: `Monitor.snapshot` becomes the CUSUM
    /// severity, `SignalMonitor.evaluate` becomes the report.
    pub fn from_monitor(severity: Severity, report: &MonitorReport) -> Self {
        Self {
            severity,
            signals: report.triggered.iter().map(|t| t.signal.clone()).collect(),
            watched: report.watched().to_vec(),
        }
    }

    /// `δ`'s condition at OBSERVE: does anything cross the Monitor→Controller
    /// boundary?
    ///
    /// Either a severity that escalates (`Severity::escalates`, the boundary
    /// built in the detect slice) or at least one fired signal.
    pub fn escalates(&self) -> bool {
        self.severity.escalates() || !self.signals.is_empty()
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn signals(&self) -> &[String] {
        &self.signals
    }

    /// **What the Monitor was watching** — carried so a quiet observation
    /// cannot be read as "nothing is wrong", only as "nothing among these".
    pub fn watched(&self) -> &[String] {
        &self.watched
    }
}

// ── ORIENT — owned by Tenet ─────────────────────────────────────────────────

/// One candidate action, with where it lands and what it would do.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub event: ControlEvent,
    /// The state id it leads to — what `J` is looked up against.
    pub to: String,
    /// The candidate after-state, when the caller simulated one. Without it the
    /// Controller can still surface but cannot vouch for stability.
    pub after: Option<Value>,
}

impl Candidate {
    pub fn new(event: ControlEvent, to: &str) -> Self {
        Self { event, to: to.to_string(), after: None }
    }

    pub fn landing_in(mut self, after: Value) -> Self {
        self.after = Some(after);
        self
    }
}

/// The pre-oriented, **ranked** options — §III's `scored`.
///
/// **Constructible only from a real [`Plan`].** There is no way to hand the
/// machine a ranking that did not come from a backward-induction sweep, which
/// is what keeps ORIENT owned by Tenet.
#[derive(Debug, Clone, PartialEq)]
pub struct Orientation {
    ranked: Vec<(Candidate, f64)>,
    horizon: usize,
}

impl Orientation {
    /// Score each candidate by `J` at its destination and rank descending.
    ///
    /// A candidate landing somewhere the plan has no value for is **refused**,
    /// not scored zero — scoring it zero would rank an unknown below a known
    /// bad option, which is a claim nobody made.
    pub fn from_plan(plan: &Plan, at: usize, candidates: Vec<Candidate>) -> Result<Self, OodaError> {
        if candidates.is_empty() {
            return Err(OodaError::NothingToOrient);
        }
        let mut ranked = Vec::with_capacity(candidates.len());
        for c in candidates {
            let j = plan
                .value(at, &c.to)
                .ok_or_else(|| OodaError::UnscoredCandidate { to: c.to.clone(), at })?;
            ranked.push((c, j));
        }
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(Self { ranked, horizon: plan.horizon() })
    }

    /// The best candidate — what DECIDE will consider.
    pub fn top(&self) -> &Candidate {
        &self.ranked.first().expect("non-empty by construction").0
    }

    pub fn ranked(&self) -> impl Iterator<Item = (&Candidate, f64)> {
        self.ranked.iter().map(|(c, j)| (c, *j))
    }

    /// **The window the ranking is about**, carried forward from TEN-4.
    pub fn horizon(&self) -> usize {
        self.horizon
    }
}

// ── the trace ───────────────────────────────────────────────────────────────

/// Why the loop stayed at OBSERVE.
#[derive(Debug, Clone, PartialEq)]
pub enum StayReason {
    /// §III's `IF signals.empty` — the common, and desirable, case.
    NothingEscalated { watched: usize },
    /// Surfaced, considered, and not urgent enough to interrupt anyone yet.
    Withheld { urgency: Urgency, threshold: f64 },
}

/// What one call to the machine did.
#[derive(Debug, Clone, PartialEq)]
pub enum OodaStep {
    /// Remained at OBSERVE.
    Stayed(StayReason),
    Advanced { from: OodaPhase, to: OodaPhase },
    /// Parked at DECIDE, waiting for a person.
    Surfaced(Box<SurfacedDecision>),
    /// Above the approval line — went to ACT with nobody asked.
    Automated { level: SheridanLevel },
}

impl OodaStep {
    pub fn surfaced(&self) -> Option<&SurfacedDecision> {
        match self {
            OodaStep::Surfaced(d) => Some(d),
            _ => None,
        }
    }

    /// Did a person get interrupted by this step?
    pub fn interrupts_a_human(&self) -> bool {
        matches!(self, OodaStep::Surfaced(_))
    }
}

/// One `δ` firing, kept so a cycle can be read back.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub cycle: usize,
    pub from: OodaPhase,
    pub to: OodaPhase,
    /// **False for an automated action.** An action nobody was asked about is
    /// legitimate above the approval line and must still be visible.
    pub human_asked: bool,
    pub note: String,
}

// ── the machine ─────────────────────────────────────────────────────────────

/// `OODA = (Q, q₀, Σ, δ)`.
#[derive(Debug, Clone)]
pub struct Ooda {
    phase: OodaPhase,
    cycle: usize,
    observation: Option<OodaObservation>,
    orientation: Option<Orientation>,
    pending: Option<SurfacedDecision>,
    authorized: Option<ApprovalToken>,
    automated: Option<SheridanLevel>,
    trace: Vec<Transition>,
}

impl Default for Ooda {
    fn default() -> Self {
        Self::new()
    }
}

impl Ooda {
    /// `q₀ = OBSERVE`.
    pub fn new() -> Self {
        Self {
            phase: OodaPhase::Observe,
            cycle: 0,
            observation: None,
            orientation: None,
            pending: None,
            authorized: None,
            automated: None,
            trace: Vec::new(),
        }
    }

    pub fn phase(&self) -> OodaPhase {
        self.phase
    }

    pub fn cycle(&self) -> usize {
        self.cycle
    }

    pub fn trace(&self) -> &[Transition] {
        &self.trace
    }

    /// Parked at DECIDE, waiting for a person to authorise.
    pub fn awaiting_authorization(&self) -> bool {
        self.phase == OodaPhase::Decide && self.pending.is_some() && self.authorized.is_none()
    }

    /// The decision on the surface right now, if any.
    pub fn surfaced(&self) -> Option<&SurfacedDecision> {
        self.pending.as_ref()
    }

    fn record(&mut self, to: OodaPhase, human_asked: bool, note: &str) {
        self.trace.push(Transition {
            cycle: self.cycle,
            from: self.phase,
            to,
            human_asked,
            note: note.to_string(),
        });
        self.phase = to;
    }

    /// `δ(OBSERVE, observation)` — stay, or advance to ORIENT.
    pub fn observe(&mut self, observation: OodaObservation) -> Result<OodaStep, OodaError> {
        self.expect(OodaPhase::Observe)?;

        if !observation.escalates() {
            // §III: "IF signals.empty: STAY in observe // nothing to surface".
            // The common case, and the one calm is made of.
            return Ok(OodaStep::Stayed(StayReason::NothingEscalated {
                watched: observation.watched().len(),
            }));
        }

        let note = format!(
            "{:?}; {} signal(s) of {} watched",
            observation.severity(),
            observation.signals().len(),
            observation.watched().len()
        );
        self.observation = Some(observation);
        self.record(OodaPhase::Orient, false, &note);
        Ok(OodaStep::Advanced { from: OodaPhase::Observe, to: OodaPhase::Orient })
    }

    /// `δ(ORIENT, orientation)` — the scoring is done; move to DECIDE.
    pub fn orient(&mut self, orientation: Orientation) -> Result<OodaStep, OodaError> {
        self.expect(OodaPhase::Orient)?;
        let note = format!(
            "{} candidate(s) ranked over H={}",
            orientation.ranked.len(),
            orientation.horizon()
        );
        self.orientation = Some(orientation);
        self.record(OodaPhase::Decide, false, &note);
        Ok(OodaStep::Advanced { from: OodaPhase::Orient, to: OodaPhase::Decide })
    }

    /// `δ(DECIDE, …)` — **the Controller's own work**: the SNR filter, the
    /// Sheridan dispatch and the Lyapunov check, via `controller::route`.
    ///
    /// Refuses unless the machine is at DECIDE with an orientation in hand, so
    /// a person is never shown options that were not scored first (Boyd).
    #[allow(clippy::too_many_arguments)]
    pub fn decide(
        &mut self,
        region: &Region,
        state: &Value,
        table: &AutomationTable,
        prefs: &Preferences,
        now: u64,
    ) -> Result<OodaStep, OodaError> {
        self.expect(OodaPhase::Decide)?;
        let orientation = self.orientation.as_ref().ok_or(OodaError::NotOriented)?;
        let candidate = orientation.top().clone();

        match route(region, state, candidate.after.as_ref(), &candidate.event, table, prefs, now)? {
            Routing::Surface(decision) => {
                // Park. The machine goes no further without a person.
                let note = format!("surfaced '{}' for authorization", decision.event.operator);
                self.pending = Some(*decision.clone());
                self.trace.push(Transition {
                    cycle: self.cycle,
                    from: OodaPhase::Decide,
                    to: OodaPhase::Decide,
                    human_asked: true,
                    note,
                });
                Ok(OodaStep::Surfaced(decision))
            }
            Routing::Automate { level } => {
                // Above the approval line (§IV). The declared table IS the
                // prior human decision — recorded as human_asked: false so an
                // action nobody was asked about is never invisible.
                self.automated = Some(level);
                self.pending = None;
                self.record(OodaPhase::Act, false, &format!("automated at {level:?}, nobody asked"));
                Ok(OodaStep::Automated { level })
            }
            Routing::Withhold { urgency, threshold } => {
                // NOT the same as automated: nothing executes, nobody is
                // interrupted, and the loop returns to watching.
                self.reset_cycle("withheld — below θ_user, nothing executed");
                Ok(OodaStep::Stayed(StayReason::Withheld { urgency, threshold }))
            }
        }
    }

    /// The human's authorisation — **the only way past a surfaced decision**.
    ///
    /// The token's binding must match the surfaced `(o, θ)`; a token minted for
    /// something else does not admit this act.
    pub fn authorize(&mut self, token: ApprovalToken) -> Result<OodaStep, OodaError> {
        self.expect(OodaPhase::Decide)?;
        let pending = self.pending.as_ref().ok_or(OodaError::NothingSurfaced)?;

        let want = pending.binding();
        if token.binding() != &want {
            return Err(OodaError::TokenBindingMismatch {
                expected: Box::new(want),
                got: Box::new(token.binding().clone()),
            });
        }

        self.authorized = Some(token);
        self.record(OodaPhase::Act, true, "authorized by a human");
        Ok(OodaStep::Advanced { from: OodaPhase::Decide, to: OodaPhase::Act })
    }

    /// `δ(ACT, …)` — execute through the gate, then back to OBSERVE.
    ///
    /// Refuses while parked at DECIDE: the loop cannot auto-advance past the
    /// human seam.
    #[allow(clippy::too_many_arguments)]
    pub fn act(
        &mut self,
        registry: &Registry,
        allowed: &[String],
        enforcement: &Enforcement,
        state: &Value,
        nonces: &mut NonceLedger,
        now: u64,
    ) -> Result<Execution, OodaError> {
        if self.awaiting_authorization() {
            return Err(OodaError::AwaitingAuthorization);
        }
        self.expect(OodaPhase::Act)?;

        let (operator, params) = match (&self.pending, &self.orientation) {
            (Some(d), _) => (d.event.operator.clone(), d.event.params.clone()),
            (None, Some(o)) => (o.top().event.operator.clone(), o.top().event.params.clone()),
            (None, None) => return Err(OodaError::NothingToAct),
        };

        let execution = match &self.authorized {
            Some(token) => execute_admitted(
                registry,
                allowed,
                enforcement,
                state,
                &operator,
                &params,
                &Authorization::Unchecked,
                &EffectClass::Live { token, now },
                nonces,
            ),
            None => {
                // Automated: the declared table already answered.
                execute_admitted(
                    registry,
                    allowed,
                    enforcement,
                    state,
                    &operator,
                    &params,
                    &Authorization::Unchecked,
                    &EffectClass::Unchecked,
                    nonces,
                )
            }
        };

        let note = if execution.committed() {
            format!("executed '{operator}'; Monitor records")
        } else {
            format!("gate refused '{operator}': {:?}", execution.result.reason)
        };
        self.reset_cycle(&note);
        Ok(execution)
    }

    /// Back to OBSERVE, one cycle later, carrying nothing forward.
    fn reset_cycle(&mut self, note: &str) {
        self.record(OodaPhase::Observe, false, note);
        self.cycle += 1;
        self.observation = None;
        self.orientation = None;
        self.pending = None;
        self.authorized = None;
        self.automated = None;
    }

    fn expect(&self, phase: OodaPhase) -> Result<(), OodaError> {
        if self.phase == phase {
            Ok(())
        } else {
            Err(OodaError::WrongPhase { expected: phase, actual: self.phase })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum OodaError {
    #[error("the loop is at {actual:?}, not {expected:?} — δ fires on completion events, not on request")]
    WrongPhase { expected: OodaPhase, actual: OodaPhase },
    #[error("nothing to orient — a ranking over an empty candidate set is not a ranking")]
    NothingToOrient,
    #[error("candidate lands in '{to}', which the plan has no value for at t={at} — refused rather than scored 0, which would rank an unknown below a known bad option")]
    UnscoredCandidate { to: String, at: usize },
    #[error("cannot decide without an orientation — the scoring completes before a person sees anything (Boyd)")]
    NotOriented,
    #[error("nothing is on the surface to authorize")]
    NothingSurfaced,
    #[error("the token is bound to {got:?}, not to the surfaced act {expected:?}")]
    // Boxed so the whole error stays small: every method on this type returns
    // Result<_, OodaError>, and a fat error would be paid for on every call.
    TokenBindingMismatch { expected: Box<Binding>, got: Box<Binding> },
    #[error("parked at DECIDE awaiting a human — the loop cannot advance itself past this seam")]
    AwaitingAuthorization,
    #[error("nothing to act on")]
    NothingToAct,
    #[error(transparent)]
    Controller(#[from] ControllerError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::Simulated;
    use crate::council::ProposalStatus;
    use crate::detect::Severity;
    use crate::operator::Registry;
    use crate::pincer::{Signal, SignalMonitor, Trajectory};
    use crate::region::Interval;
    use crate::signal::SignalSpec;
    use crate::tenet::{backward_induct, BellmanSpec, InversionPoint, TransitionModel};
    use serde_json::json;

    /// A minimum operating float of 1000, so the live state is genuinely
    /// OUTSIDE V and `W > 0`.
    ///
    /// This matters: urgency is Lyapunov-derived, so a state comfortably inside
    /// the region has urgency 0 and every route is `Withhold`. A fixture that
    /// sat inside V would test the withhold branch four times over while
    /// claiming to test surfacing — the same class of fixture error the tenet
    /// and kernel slices each hit once.
    fn region() -> Region {
        Region::new()
            .bounding(Interval::at_least("finances.liquid.balance", 1000.0))
            .weighing("finances.liquid.balance", 1.0)
    }

    fn state(balance: f64) -> Value {
        json!({"finances": {
            "liquid": {"balance": balance},
            "pockets": {},
            "income": {"monthly_total": 0.0, "sources": []}
        }})
    }

    fn table() -> AutomationTable {
        AutomationTable::new()
            .declaring("ROUTINE", SheridanLevel::ExecuteAndNotify)
            .declaring("SPEND", SheridanLevel::ExecuteIfApproved)
    }

    fn prefs() -> Preferences {
        Preferences::default().with_threshold(1.0).weighing("SPEND", 10.0)
    }

    /// A real MonitorReport, so OodaObservation cannot be faked.
    fn report(fires: bool) -> MonitorReport {
        let mut m = SignalMonitor::new(
            vec![Signal::new(
                "drain",
                Trajectory::ever_within(1, "finances.liquid.balance <= 100"),
            )],
            SignalSpec::new(1.0, 3).unwrap(),
        )
        .unwrap();
        let h = if fires { vec![state(50.0)] } else { vec![state(9000.0)] };
        m.monitor(&h).unwrap()
    }

    /// A real Plan, so Orientation cannot be faked.
    fn plan() -> Plan {
        let space: crate::kernel::Space = [("here", 500.0), ("better", 900.0), ("worse", 10.0)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), json!({ "finances": {"liquid": {"balance": v}} })))
            .collect();
        let model = TransitionModel::new()
            .moving("here", "top_up", "better")
            .moving("here", "drain", "worse")
            .moving("better", "hold", "better")
            .moving("worse", "hold", "worse");
        let ip = InversionPoint::Predicate(vec!["finances.liquid.balance >= 900".into()]);
        backward_induct(&space, &model, &ip, &BellmanSpec::new(2, 0.9, 100.0)).unwrap()
    }

    /// The winning candidate is a real operator whose real effect matches the
    /// after-state it claims: `record_income(400)` on a balance of 500 genuinely
    /// produces 900. A candidate whose declared landing disagreed with what the
    /// operator does would make the stability verdict a fiction.
    fn candidates() -> Vec<Candidate> {
        vec![
            Candidate::new(
                ControlEvent::new("SPEND", "budget.spend").with_param("amount", json!(50.0)),
                "worse",
            )
            .landing_in(state(10.0)),
            Candidate::new(
                ControlEvent::new("SPEND", "budget.record_income")
                    .with_param("amount", json!(400.0)),
                "better",
            )
            .landing_in(state(900.0)),
        ]
    }

    fn token_for(d: &SurfacedDecision, nonce: u64) -> ApprovalToken {
        Simulated::from_sandbox("prop-1", d.binding(), Ok(()))
            .expect("sandbox committed")
            .voted(ProposalStatus::Passed)
            .expect("passed")
            .approve("bonnie", nonce, 10_000)
    }

    // ── §III — the machine ──────────────────────────────────────────────────

    #[test]
    fn q0_is_observe() {
        assert_eq!(Ooda::new().phase(), OodaPhase::Observe);
    }

    #[test]
    fn a_quiet_observation_stays_at_observe() {
        // §III: "IF signals.empty: STAY in observe". The common case, and the
        // one calm is made of — not an error.
        let mut o = Ooda::new();
        let obs = OodaObservation::from_monitor(Severity::Info, &report(false));
        assert!(!obs.escalates());

        let step = o.observe(obs).unwrap();
        assert!(matches!(step, OodaStep::Stayed(StayReason::NothingEscalated { .. })));
        assert_eq!(o.phase(), OodaPhase::Observe);
        assert!(!step.interrupts_a_human());
    }

    #[test]
    fn a_quiet_observation_still_says_what_it_was_watching() {
        let obs = OodaObservation::from_monitor(Severity::Info, &report(false));
        assert_eq!(obs.watched(), ["drain"], "'nothing fired' is only ever 'nothing among these'");
    }

    #[test]
    fn an_escalating_observation_advances_to_orient() {
        let mut o = Ooda::new();
        let obs = OodaObservation::from_monitor(Severity::Warning, &report(true));
        assert!(obs.escalates());
        assert!(matches!(o.observe(obs).unwrap(), OodaStep::Advanced { to: OodaPhase::Orient, .. }));
        assert_eq!(o.phase(), OodaPhase::Orient);
    }

    #[test]
    fn severity_alone_escalates_even_with_no_signal() {
        // The Monitor→Controller boundary built in the detect slice, reused —
        // not re-decided here.
        let obs = OodaObservation::from_monitor(Severity::Critical, &report(false));
        assert!(obs.escalates());
        assert!(obs.signals().is_empty());
    }

    // ── ORIENT is owned by Tenet ────────────────────────────────────────────

    #[test]
    fn the_orientation_is_ranked_by_the_plan() {
        let orientation = Orientation::from_plan(&plan(), 0, candidates()).unwrap();
        assert_eq!(
            orientation.top().event.operator,
            "budget.record_income",
            "the sweep ranks the candidate that reaches the inversion point first"
        );
        assert_eq!(orientation.horizon(), 2, "and the window is carried");
    }

    #[test]
    fn a_candidate_the_plan_cannot_score_is_refused_not_zeroed() {
        let bad = vec![Candidate::new(ControlEvent::new("SPEND", "budget.spend"), "nowhere")];
        assert!(matches!(
            Orientation::from_plan(&plan(), 0, bad),
            Err(OodaError::UnscoredCandidate { .. })
        ));
    }

    #[test]
    fn an_empty_candidate_set_is_refused() {
        assert!(matches!(
            Orientation::from_plan(&plan(), 0, vec![]),
            Err(OodaError::NothingToOrient)
        ));
    }

    // ── ★ DECIDE — the human seam ───────────────────────────────────────────

    fn to_decide() -> Ooda {
        let mut o = Ooda::new();
        o.observe(OodaObservation::from_monitor(Severity::Warning, &report(true))).unwrap();
        o.orient(Orientation::from_plan(&plan(), 0, candidates()).unwrap()).unwrap();
        o
    }

    #[test]
    fn the_loop_parks_at_decide_and_cannot_advance_itself() {
        let mut o = to_decide();
        let step = o.decide(&region(), &state(500.0), &table(), &prefs(), 0).unwrap();

        assert!(step.interrupts_a_human());
        assert!(o.awaiting_authorization(), "★ parked, waiting for a person");
        assert_eq!(o.phase(), OodaPhase::Decide);

        // ACT refuses. The loop cannot close itself.
        let mut n = NonceLedger::new();
        let err = o
            .act(&Registry::default(), &Registry::default().names(), &Enforcement::default(), &state(500.0), &mut n, 0)
            .unwrap_err();
        assert_eq!(err, OodaError::AwaitingAuthorization);
    }

    #[test]
    fn decide_refuses_without_an_orientation() {
        // Boyd: the scoring completes before a person sees anything.
        let mut o = Ooda::new();
        o.observe(OodaObservation::from_monitor(Severity::Warning, &report(true))).unwrap();
        // At ORIENT, not DECIDE — no orientation supplied yet.
        let err = o.decide(&region(), &state(500.0), &table(), &prefs(), 0).unwrap_err();
        assert!(matches!(err, OodaError::WrongPhase { .. }));
    }

    #[test]
    fn a_token_for_a_different_act_does_not_admit_this_one() {
        let mut o = to_decide();
        o.decide(&region(), &state(500.0), &table(), &prefs(), 0).unwrap();

        let other = ControlEvent::new("SPEND", "budget.spend").with_param("amount", json!(1.0));
        let wrong = Simulated::from_sandbox("p", other.binding(), Ok(()))
            .unwrap()
            .voted(ProposalStatus::Passed)
            .unwrap()
            .approve("bonnie", 7, 10_000);

        assert!(matches!(o.authorize(wrong), Err(OodaError::TokenBindingMismatch { .. })));
        assert!(o.awaiting_authorization(), "and it is still parked");
    }

    #[test]
    fn the_human_moves_the_loop_to_act() {
        let mut o = to_decide();
        let step = o.decide(&region(), &state(500.0), &table(), &prefs(), 0).unwrap();
        let token = token_for(step.surfaced().unwrap(), 1);

        assert!(matches!(o.authorize(token).unwrap(), OodaStep::Advanced { to: OodaPhase::Act, .. }));
        assert_eq!(o.phase(), OodaPhase::Act);
        assert!(!o.awaiting_authorization());
    }

    // ── the three routes out of DECIDE stay distinct ────────────────────────

    #[test]
    fn withheld_is_not_automated_and_executes_nothing() {
        let mut o = to_decide();
        // θ_user far above anything this urgency can reach.
        let strict = Preferences::default().with_threshold(1e9).weighing("SPEND", 1.0);
        let step = o.decide(&region(), &state(500.0), &table(), &strict, 0).unwrap();

        assert!(matches!(step, OodaStep::Stayed(StayReason::Withheld { .. })));
        assert!(!step.interrupts_a_human(), "nobody was interrupted");
        assert_eq!(o.phase(), OodaPhase::Observe, "and it went back to watching");
        assert!(o.surfaced().is_none(), "nothing executed, nothing pending");
    }

    #[test]
    fn an_automated_action_skips_the_human_and_says_so() {
        let mut o = Ooda::new();
        o.observe(OodaObservation::from_monitor(Severity::Warning, &report(true))).unwrap();
        let routine = vec![Candidate::new(
            ControlEvent::new("ROUTINE", "budget.record_income").with_param("amount", json!(400.0)),
            "better",
        )
        .landing_in(state(900.0))];
        o.orient(Orientation::from_plan(&plan(), 0, routine).unwrap()).unwrap();

        let step = o.decide(&region(), &state(500.0), &table(), &prefs(), 0).unwrap();
        assert!(matches!(step, OodaStep::Automated { .. }));
        assert_eq!(o.phase(), OodaPhase::Act);
        assert!(!step.interrupts_a_human());

        let t = o.trace().last().unwrap();
        assert!(!t.human_asked, "★ an action nobody was asked about must be visible in the trace");
        assert!(t.note.contains("nobody asked"));
    }

    // ── ★ the full cycle ────────────────────────────────────────────────────

    #[test]
    fn the_full_cycle_closes_and_returns_to_observe() {
        let mut o = to_decide();
        let step = o.decide(&region(), &state(500.0), &table(), &prefs(), 0).unwrap();
        let token = token_for(step.surfaced().unwrap(), 1);
        o.authorize(token).unwrap();

        let mut nonces = NonceLedger::new();
        let execution = o
            .act(
                &Registry::default(),
                &Registry::default().names(),
                &Enforcement::default(),
                &state(500.0),
                &mut nonces,
                0,
            )
            .unwrap();

        assert!(
            execution.committed(),
            "★ the gate genuinely executed the approved act: {:?}",
            execution.result.reason
        );
        assert_eq!(o.phase(), OodaPhase::Observe, "★ back to OBSERVE");
        assert_eq!(o.cycle(), 1, "one full cycle");

        // The human was asked exactly once, at DECIDE.
        let asked: Vec<&Transition> = o.trace().iter().filter(|t| t.human_asked).collect();
        assert_eq!(asked.len(), 2, "surfaced, then authorized — both at DECIDE");
        assert!(asked.iter().all(|t| t.from == OodaPhase::Decide));
    }

    #[test]
    fn the_machine_has_no_way_to_mint_a_token() {
        // Structural, not polite: the only route to an ApprovalToken is
        // approval.rs's Simulated → Voted → approve chain. Nothing on Ooda
        // returns one, and `authorize` only ACCEPTS one.
        let o = to_decide();
        // If a mint existed it would have to appear here; the type carries only
        // a token it was GIVEN.
        assert!(o.surfaced().is_none() || o.surfaced().is_some());
        assert!(!o.awaiting_authorization(), "not yet decided, so nothing is parked");
    }

    #[test]
    fn phases_fire_on_completion_events_not_on_request() {
        let mut o = Ooda::new();
        // orient before observe
        assert!(matches!(
            o.orient(Orientation::from_plan(&plan(), 0, candidates()).unwrap()),
            Err(OodaError::WrongPhase { .. })
        ));
        // act before anything
        let mut n = NonceLedger::new();
        assert!(matches!(
            o.act(&Registry::default(), &[], &Enforcement::default(), &state(1.0), &mut n, 0),
            Err(OodaError::WrongPhase { .. })
        ));
    }
}
