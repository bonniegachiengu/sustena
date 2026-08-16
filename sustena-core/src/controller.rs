//! Controller decision-math — **the slice where the loop closes through a human**
//! (Controller §II, §IV, §VII + Additions · CTL-2, CTL-4, CTL-7).
//!
//! ```text
//! Monitor            Controller                         human            gate
//! ───────            ──────────                         ─────            ────
//! CUSUM ─► Severity ─► should_surface (SNR §VII)  ─► approval token ─► execute
//!          escalates   is_stable_intervention (§II)     (N1)
//!                      AUTOMATION_LEVEL (§IV)
//! ```
//!
//! Every input this module needs was built by an earlier slice: `W = d(s,V)`
//! from [`crate::region`], `Severity::escalates()` from [`crate::detect`], and
//! the bound single-use token from [`crate::approval`]. This is where they meet.
//!
//! ## `is_stable_intervention` — the formal definition of a good decision (§II)
//!
//! > An intervention is valid **if and only if it reduces the Lyapunov
//! > distance**. The approve/reject mechanism is a stability check.
//!
//! ```text
//! stable ⟺ W(after) ≤ W(before)
//! ```
//!
//! **Levelled up, deliberately.** §II writes the Lyapunov function as
//! `V(s) = ‖s_desired − s_actual‖²` — distance to a *point*. The Controller's own
//! Additions correct this to the potential well `W(s) = d(s, V)`, and that is
//! what is implemented: **the viable region IS the desired set.** A point target
//! would make every state but one "bad" and would need a desired point nobody
//! declared; a region is what the household actually declared, and it is the
//! same `W` the Monitor already computes. One quantity, three consumers.
//!
//! The rollback panel's purpose follows immediately: an action that *increased*
//! `W` moved away from the region, so [`should_rollback`] is just `!stable`.
//!
//! ## `should_surface` — alert fatigue, formally (§VII)
//!
//! ```text
//! SNR_gov = events requiring human judgment / events surfaced unnecessarily
//! φ(e) = 1 ⟺ Sheridan_level(e) ≤ 5 ∧ urgency(e) ≥ θ_user
//! ```
//!
//! **Filtering suppresses interruption, never authorization.** This is the one
//! place the article's shape could be read dangerously: `should_surface` being
//! false for a level-5 action does **not** mean the action proceeds. It means
//! nobody is interrupted about it *yet*. An action that needs approval and has
//! not been approved simply does not happen — so [`Routing`] has a distinct
//! [`Routing::Withhold`] variant rather than folding "not surfaced" into
//! "automated". Collapsing those two would turn an attention filter into an
//! auto-approver.
//!
//! ## `urgency` is Lyapunov-derived, not heuristic (§VII)
//!
//! ```text
//! urgency = base × lyapunov × time_pressure
//! ```
//!
//! > The urgency function is itself a Lyapunov-derived score — events that move
//! > the system further from its desired state score higher. This is not
//! > heuristic. It follows directly from the stability condition.
//!
//! `base` comes from declared `URGENCY_WEIGHTS`, `lyapunov` is `W = d(s,V)`, and
//! `time_pressure` is `1/(deadline − now)` or `1.0` with no deadline. The core
//! has no clock, so `now` is supplied by the host.
//!
//! ## What is declared, and why
//!
//! `AUTOMATION_LEVEL`, `θ_user` and `URGENCY_WEIGHTS` are all **spec fields** —
//! the same discipline as the region weights and the CUSUM parameters. They are
//! the *"how much does the human decide"* dial, and that dial belongs where it
//! can be argued with.
//!
//! **The default is conservative by construction:** an action type absent from
//! the table routes to [`SheridanLevel::ExecuteIfApproved`], so a forgotten
//! declaration surfaces for approval rather than automating itself.
//!
//! ## The Controller cannot approve
//!
//! [`SurfacedDecision`] exposes the [`Binding`] a token must match and nothing
//! that produces one. Minting still requires [`crate::approval`]'s chain, which
//! only a human's approval completes. *"Advise, the human decides"* is therefore
//! a fact about what this module can construct, not a promise about how it
//! behaves.
//!
//! ## Scope
//!
//! The decision-math only. The OODA frame (§III), holarchy escalation (§V), the
//! IoT bridge (§VI), persistent panels (§VIII) and the assembled loop (§IX) are
//! follow-ons — they compose *these* functions rather than changing them.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::approval::Binding;
use crate::region::{Region, RegionError};

/// Sheridan & Verplank's spectrum between full human control and full
/// automation (§IV).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SheridanLevel {
    OffersNoAssistance = 1,
    OffersAlternatives = 2,
    NarrowsAlternatives = 3,
    SuggestsOne = 4,
    /// **5** — the computer executes *if the human approves*. The approval line.
    ExecuteIfApproved = 5,
    /// **6** — the computer executes and notifies.
    ExecuteAndNotify = 6,
    NotifiesIfAsked = 7,
    NotifiesIfSignificant = 8,
    DecidesAutonomously = 9,
    FullAutomation = 10,
}

impl SheridanLevel {
    /// `level ≤ 5` — the §VII condition. Sustena's Controller operates at 5–6,
    /// and this is the line between them.
    pub fn requires_approval(&self) -> bool {
        *self <= SheridanLevel::ExecuteIfApproved
    }

    pub fn number(&self) -> u8 {
        *self as u8
    }
}

/// `AUTOMATION_LEVEL[action_type]` — the declared *"how much does the human
/// decide"* dial (§IV).
#[derive(Debug, Clone, PartialEq)]
pub struct AutomationTable {
    levels: BTreeMap<String, SheridanLevel>,
}

impl Default for AutomationTable {
    fn default() -> Self {
        Self::new()
    }
}

impl AutomationTable {
    pub fn new() -> Self {
        Self { levels: BTreeMap::new() }
    }

    pub fn declaring(mut self, action_type: &str, level: SheridanLevel) -> Self {
        self.levels.insert(action_type.to_string(), level);
        self
    }

    /// The level for an action type.
    ///
    /// **An undeclared type routes to `ExecuteIfApproved`**, so forgetting a
    /// declaration surfaces for approval rather than automating itself. Fail-safe
    /// defaults, at the governance layer.
    pub fn level_for(&self, action_type: &str) -> SheridanLevel {
        self.levels
            .get(action_type)
            .copied()
            .unwrap_or(SheridanLevel::ExecuteIfApproved)
    }

    /// Action types being routed by the conservative default rather than by a
    /// declaration. Surfaced for the same reason undeclared region weights are.
    pub fn undeclared(&self, seen: &[&str]) -> Vec<String> {
        seen.iter()
            .filter(|t| !self.levels.contains_key(**t))
            .map(|t| (*t).to_string())
            .collect()
    }
}

/// The declared governance preferences of one person.
#[derive(Debug, Clone, PartialEq)]
pub struct Preferences {
    /// `θ_user` — the urgency below which nothing interrupts.
    pub threshold: f64,
    /// `URGENCY_WEIGHTS[event.type]` — the `base` term.
    pub urgency_weights: BTreeMap<String, f64>,
    /// Used when an event type has no declared weight. `1.0` keeps the urgency
    /// equal to the Lyapunov term rather than silently zeroing it — a missing
    /// weight must not make something invisible.
    pub default_weight: f64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { threshold: 1.0, urgency_weights: BTreeMap::new(), default_weight: 1.0 }
    }
}

impl Preferences {
    pub fn with_threshold(mut self, theta: f64) -> Self {
        self.threshold = theta;
        self
    }

    pub fn weighing(mut self, event_type: &str, base: f64) -> Self {
        self.urgency_weights.insert(event_type.to_string(), base);
        self
    }

    pub fn weight_for(&self, event_type: &str) -> f64 {
        self.urgency_weights.get(event_type).copied().unwrap_or(self.default_weight)
    }

    pub fn typecheck(&self) -> Result<(), ControllerError> {
        if !self.threshold.is_finite() || self.threshold < 0.0 {
            return Err(ControllerError::BadThreshold(self.threshold.to_string()));
        }
        for (t, w) in &self.urgency_weights {
            if !w.is_finite() || *w < 0.0 {
                return Err(ControllerError::BadWeight { event_type: t.clone(), w: w.to_string() });
            }
        }
        Ok(())
    }
}

/// A governance event — something the Controller might act on.
///
/// Named `ControlEvent` rather than `Event` because [`crate::event::Event`] is
/// the log record. They are genuinely different things: one is what happened,
/// the other is what someone might do about it.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlEvent {
    /// Keys `AUTOMATION_LEVEL`.
    pub action_type: String,
    /// Keys `URGENCY_WEIGHTS`. Often the same as `action_type`; kept separate
    /// because the article keys them separately and they answer different
    /// questions — *who decides* versus *how loud*.
    pub event_type: String,
    /// The Enzyme this event proposes, and with what parameters. Carried so the
    /// surfaced decision can name the approval binding.
    pub operator: String,
    pub params: Map<String, Value>,
    /// Host-supplied, in the host's own units. `None` means no deadline.
    pub deadline: Option<u64>,
}

impl ControlEvent {
    pub fn new(action_type: &str, operator: &str) -> Self {
        Self {
            action_type: action_type.to_string(),
            event_type: action_type.to_string(),
            operator: operator.to_string(),
            params: Map::new(),
            deadline: None,
        }
    }

    pub fn typed(mut self, event_type: &str) -> Self {
        self.event_type = event_type.to_string();
        self
    }

    pub fn with_param(mut self, key: &str, value: Value) -> Self {
        self.params.insert(key.to_string(), value);
        self
    }

    pub fn due_at(mut self, deadline: u64) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn binding(&self) -> Binding {
        Binding::new(&self.operator, &self.params)
    }
}

/// `urgency = base × lyapunov × time_pressure`, decomposed so a person can be
/// told which factor made it loud.
#[derive(Debug, Clone, PartialEq)]
pub struct Urgency {
    pub value: f64,
    /// `URGENCY_WEIGHTS[event.type]`.
    pub base: f64,
    /// `W = d(s,V)` — the Lyapunov term. **This is why urgency is not
    /// heuristic:** it follows from the stability condition.
    pub lyapunov: f64,
    /// `1/(deadline − now)`, or `1.0` with no deadline.
    pub time_pressure: f64,
    /// False when the region distance was only a lower bound (a cross-dimension
    /// relation failed and a boolean has no natural distance). The urgency is
    /// then an **under**-estimate, and saying so beats quietly ranking it low.
    pub lyapunov_exact: bool,
}

/// The Lyapunov verdict on one intervention (§II).
#[derive(Debug, Clone, PartialEq)]
pub struct Stability {
    pub before: f64,
    pub after: f64,
    /// `W(after) ≤ W(before)`.
    pub stable: bool,
    /// False when either distance was a lower bound. The comparison is then
    /// **not reliable**, and it is reported rather than trusted.
    pub exact: bool,
}

impl Stability {
    /// How much closer to the region the intervention gets. Negative when it
    /// moves away.
    pub fn improvement(&self) -> f64 {
        self.before - self.after
    }
}

/// What the Controller decided to do with an event.
#[derive(Debug, Clone, PartialEq)]
pub enum Routing {
    /// `φ(e) = 1` — needs approval **and** is urgent enough to interrupt.
    Surface(Box<SurfacedDecision>),
    /// Above the approval line: executes and notifies the Monitor. The human is
    /// not asked.
    Automate { level: SheridanLevel },
    /// Needs approval, but not urgent enough to interrupt **yet**.
    ///
    /// **Not the same as automated.** The action does not proceed; nobody is
    /// being interrupted about it. Folding this into `Automate` would turn an
    /// attention filter into an auto-approver.
    Withhold { urgency: Urgency, threshold: f64 },
}

impl Routing {
    pub fn surfaces(&self) -> bool {
        matches!(self, Routing::Surface(_))
    }

    /// Does this route reach a person? The §VII filter's own answer.
    pub fn interrupts_a_human(&self) -> bool {
        self.surfaces()
    }
}

/// Everything a person needs in order to decide, and the binding an approval
/// must match.
///
/// **Carries no way to approve itself.** Minting a token requires
/// [`crate::approval`]'s chain, which only a human completes.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfacedDecision {
    pub event: ControlEvent,
    pub level: SheridanLevel,
    pub urgency: Urgency,
    /// `None` when no candidate after-state was supplied — the Controller can
    /// still surface, it just cannot vouch for stability.
    pub stability: Option<Stability>,
}

impl SurfacedDecision {
    /// The `(o, θ)` an approval token must be bound to.
    pub fn binding(&self) -> Binding {
        self.event.binding()
    }

    /// Whether the Controller can vouch for this intervention reducing `W`.
    ///
    /// `false` for an unstable intervention **and** for one whose stability was
    /// never assessed — the two are different, and neither is a yes.
    pub fn vouched_stable(&self) -> bool {
        self.stability.as_ref().is_some_and(|s| s.stable && s.exact)
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ControllerError {
    #[error("θ_user must be finite and non-negative; got {0}")]
    BadThreshold(String),
    #[error("URGENCY_WEIGHTS['{event_type}'] must be finite and non-negative; got {w}")]
    BadWeight { event_type: String, w: String },
    #[error("the deadline {deadline} is not after now ({now}); time pressure would be infinite or negative")]
    DeadlinePassed { deadline: u64, now: u64 },
    #[error("{0}")]
    Region(#[from] RegionError),
}

/// `is_stable_intervention` (§II) — **the formal definition of a good decision.**
///
/// Measured against the **viable region**, not an ad-hoc desired point: the
/// region is what the household declared, and it is the same `W` the Monitor
/// computes. See the module docs on why this levels up §II's `‖s_desired − s‖²`.
pub fn is_stable_intervention(
    region: &Region,
    before: &Value,
    after: &Value,
) -> Result<Stability, ControllerError> {
    let d_before = region.distance(before)?;
    let d_after = region.distance(after)?;
    Ok(Stability {
        before: d_before.weighted,
        after: d_after.weighted,
        stable: d_after.weighted <= d_before.weighted,
        exact: d_before.is_exact() && d_after.is_exact(),
    })
}

/// An action that increased `W` moved away from the region, so the previous
/// state was closer. That is the rollback panel's whole purpose (§II), and it
/// pairs with the derived inverse (OP-6) and the version history (EDIT-7).
pub fn should_rollback(stability: &Stability) -> bool {
    !stability.stable
}

/// `compute_urgency` (§VII) — Lyapunov-derived, not heuristic.
pub fn compute_urgency(
    region: &Region,
    state: &Value,
    event: &ControlEvent,
    prefs: &Preferences,
    now: u64,
) -> Result<Urgency, ControllerError> {
    let d = region.distance(state)?;
    let base = prefs.weight_for(&event.event_type);

    let time_pressure = match event.deadline {
        None => 1.0,
        Some(deadline) => {
            if deadline <= now {
                // A passed deadline would divide by zero or go negative, which
                // would silently invert the ranking. Refused rather than
                // clamped: "this was due yesterday" is a real condition the
                // host should handle, not a number to invent.
                return Err(ControllerError::DeadlinePassed { deadline, now });
            }
            1.0 / (deadline - now) as f64
        }
    };

    Ok(Urgency {
        value: base * d.weighted * time_pressure,
        base,
        lyapunov: d.weighted,
        time_pressure,
        lyapunov_exact: d.is_exact(),
    })
}

/// `should_surface` / `route_action` (§IV + §VII) — the SNR filter and the
/// Sheridan dispatch, as one decision.
///
/// `candidate_after` is the simulated post-state, when the caller has one. With
/// it, the surfaced decision carries a stability verdict; without it, the
/// Controller still surfaces but does not vouch.
pub fn route(
    region: &Region,
    state: &Value,
    candidate_after: Option<&Value>,
    event: &ControlEvent,
    table: &AutomationTable,
    prefs: &Preferences,
    now: u64,
) -> Result<Routing, ControllerError> {
    let level = table.level_for(&event.action_type);

    // Above the approval line: it executes and tells the Monitor. The human is
    // not part of this loop.
    if !level.requires_approval() {
        return Ok(Routing::Automate { level });
    }

    let urgency = compute_urgency(region, state, event, prefs, now)?;

    // φ(e) = 1 ⟺ level ≤ 5 ∧ urgency ≥ θ_user
    if urgency.value < prefs.threshold {
        return Ok(Routing::Withhold { urgency, threshold: prefs.threshold });
    }

    let stability = match candidate_after {
        None => None,
        Some(after) => Some(is_stable_intervention(region, state, after)?),
    };

    Ok(Routing::Surface(Box::new(SurfacedDecision { event: event.clone(), level, urgency, stability })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::{EffectClass, NonceLedger, Simulated};
    use crate::council::ProposalStatus;
    use crate::operator::{execute_admitted, Authorization, Enforcement, Registry};
    use crate::region::Interval;
    use serde_json::json;

    fn region() -> Region {
        Region::new()
            .bounding(Interval::at_least("finances.liquid.balance", 0.0))
            .weighing("finances.liquid.balance", 1.0)
    }

    fn state(balance: f64) -> Value {
        json!({"finances": {"liquid": {"balance": balance}}})
    }

    fn table() -> AutomationTable {
        AutomationTable::new()
            .declaring("ROUTINE", SheridanLevel::ExecuteAndNotify)
            .declaring("FINANCIAL", SheridanLevel::ExecuteIfApproved)
            .declaring("CRITICAL", SheridanLevel::ExecuteIfApproved)
            .declaring("IOT_WRITE", SheridanLevel::ExecuteIfApproved)
            .declaring("ROLLBACK", SheridanLevel::ExecuteIfApproved)
    }

    fn prefs() -> Preferences {
        Preferences::default().with_threshold(10.0).weighing("FINANCIAL", 1.0)
    }

    // ── §II — is_stable_intervention ────────────────────────────────────────

    #[test]
    fn an_intervention_that_reduces_the_distance_is_stable() {
        // -300 → -100 is 200 closer to the region. That IS a good decision.
        let s = is_stable_intervention(&region(), &state(-300.0), &state(-100.0)).unwrap();
        assert!(s.stable);
        assert_eq!(s.before, 300.0);
        assert_eq!(s.after, 100.0);
        assert_eq!(s.improvement(), 200.0);
    }

    #[test]
    fn an_intervention_that_increases_the_distance_is_not() {
        let s = is_stable_intervention(&region(), &state(-100.0), &state(-300.0)).unwrap();
        assert!(!s.stable);
        assert!(s.improvement() < 0.0);
        assert!(should_rollback(&s), "the rollback panel's purpose, derived not asserted");
    }

    #[test]
    fn holding_still_is_stable_because_the_condition_is_non_strict() {
        // W(after) ≤ W(before). Doing nothing does not move away.
        let s = is_stable_intervention(&region(), &state(-100.0), &state(-100.0)).unwrap();
        assert!(s.stable);
        assert!(!should_rollback(&s));
    }

    #[test]
    fn reaching_the_region_is_the_strongest_stable_move() {
        let s = is_stable_intervention(&region(), &state(-100.0), &state(50.0)).unwrap();
        assert!(s.stable);
        assert_eq!(s.after, 0.0, "inside V, W = 0");
    }

    #[test]
    fn a_comparison_against_a_lower_bound_is_reported_not_trusted() {
        // A violated relation makes the box distance a lower bound, so the
        // ordering of two such numbers is not reliable. Saying so beats a
        // confident wrong answer.
        let r = region().relating("finances.liquid.balance >= 999999");
        let s = is_stable_intervention(&r, &state(10.0), &state(20.0)).unwrap();
        assert!(!s.exact, "the relation fails on both sides, so neither W is exact");
    }

    // ── §IV — Sheridan levels ───────────────────────────────────────────────

    #[test]
    fn the_approval_line_is_level_five() {
        assert!(SheridanLevel::ExecuteIfApproved.requires_approval());
        assert!(SheridanLevel::SuggestsOne.requires_approval(), "4 is below the line");
        assert!(!SheridanLevel::ExecuteAndNotify.requires_approval(), "6 is above it");
        assert!(!SheridanLevel::FullAutomation.requires_approval());
    }

    #[test]
    fn the_articles_table_routes_as_it_says() {
        let t = table();
        assert_eq!(t.level_for("ROUTINE"), SheridanLevel::ExecuteAndNotify);
        for critical in ["FINANCIAL", "CRITICAL", "IOT_WRITE", "ROLLBACK"] {
            assert_eq!(t.level_for(critical), SheridanLevel::ExecuteIfApproved, "{critical}");
        }
    }

    #[test]
    fn an_undeclared_action_type_surfaces_rather_than_automating_itself() {
        // Fail-safe defaults at the governance layer: forgetting a declaration
        // must not be how something starts running unattended.
        let t = table();
        assert_eq!(t.level_for("SOMETHING_NEW"), SheridanLevel::ExecuteIfApproved);
        assert!(t.level_for("SOMETHING_NEW").requires_approval());
        assert_eq!(t.undeclared(&["ROUTINE", "SOMETHING_NEW"]), ["SOMETHING_NEW"]);
    }

    // ── §VII — urgency is Lyapunov-derived ──────────────────────────────────

    #[test]
    fn urgency_rises_with_distance_from_the_region() {
        let (r, p) = (region(), prefs());
        let near = compute_urgency(&r, &state(-10.0), &ControlEvent::new("FINANCIAL", "budget.spend"), &p, 0).unwrap();
        let far = compute_urgency(&r, &state(-1000.0), &ControlEvent::new("FINANCIAL", "budget.spend"), &p, 0).unwrap();
        assert!(far.value > near.value, "not heuristic — it follows from the stability condition");
        assert_eq!(far.lyapunov, 1000.0);
    }

    #[test]
    fn a_state_inside_the_region_has_zero_urgency() {
        let u = compute_urgency(&region(), &state(500.0), &ControlEvent::new("FINANCIAL", "x"), &prefs(), 0).unwrap();
        assert_eq!(u.value, 0.0, "W = 0 inside V, so nothing is pulling");
    }

    #[test]
    fn the_declared_base_weight_scales_it() {
        let r = region();
        let p = Preferences::default().weighing("LOUD", 10.0).weighing("QUIET", 0.5);
        let loud = compute_urgency(&r, &state(-100.0), &ControlEvent::new("A", "x").typed("LOUD"), &p, 0).unwrap();
        let quiet = compute_urgency(&r, &state(-100.0), &ControlEvent::new("A", "x").typed("QUIET"), &p, 0).unwrap();
        assert_eq!(loud.value, 1000.0);
        assert_eq!(quiet.value, 50.0);
    }

    #[test]
    fn a_nearer_deadline_raises_the_pressure() {
        let (r, p) = (region(), prefs());
        let soon = ControlEvent::new("FINANCIAL", "x").due_at(2);
        let later = ControlEvent::new("FINANCIAL", "x").due_at(10);
        let a = compute_urgency(&r, &state(-100.0), &soon, &p, 1).unwrap();
        let b = compute_urgency(&r, &state(-100.0), &later, &p, 1).unwrap();
        assert!(a.value > b.value);
        assert_eq!(a.time_pressure, 1.0);
    }

    #[test]
    fn no_deadline_means_no_time_pressure_not_zero_urgency() {
        let u = compute_urgency(&region(), &state(-100.0), &ControlEvent::new("FINANCIAL", "x"), &prefs(), 0).unwrap();
        assert_eq!(u.time_pressure, 1.0);
        assert_eq!(u.value, 100.0);
    }

    #[test]
    fn a_passed_deadline_is_refused_rather_than_clamped() {
        // Dividing by zero or a negative would silently invert the ranking.
        let e = ControlEvent::new("FINANCIAL", "x").due_at(5);
        assert!(matches!(
            compute_urgency(&region(), &state(-100.0), &e, &prefs(), 5),
            Err(ControllerError::DeadlinePassed { .. })
        ));
    }

    #[test]
    fn an_inexact_lyapunov_term_is_flagged_on_the_urgency() {
        let r = region().relating("finances.liquid.balance >= 999999");
        let u = compute_urgency(&r, &state(10.0), &ControlEvent::new("FINANCIAL", "x"), &prefs(), 0).unwrap();
        assert!(!u.lyapunov_exact, "an under-estimate must not quietly rank low");
    }

    // ── §VII — the SNR filter ───────────────────────────────────────────────

    #[test]
    fn an_urgent_action_needing_approval_surfaces() {
        let e = ControlEvent::new("FINANCIAL", "budget.spend").with_param("amount", json!(300));
        let r = route(&region(), &state(-100.0), None, &e, &table(), &prefs(), 0).unwrap();
        assert!(r.surfaces() && r.interrupts_a_human());
    }

    #[test]
    fn an_automated_action_goes_to_the_monitor_not_the_human() {
        let e = ControlEvent::new("ROUTINE", "budget.summary");
        let r = route(&region(), &state(-100000.0), None, &e, &table(), &prefs(), 0).unwrap();
        assert!(matches!(r, Routing::Automate { level: SheridanLevel::ExecuteAndNotify }),
                "above the line, however urgent — the human is not part of this loop");
        assert!(!r.interrupts_a_human());
    }

    /// **The most important distinction in the module.**
    #[test]
    fn withholding_is_not_automating() {
        // Needs approval, but too quiet to interrupt. The action does NOT
        // proceed — folding this into Automate would make an attention filter
        // into an auto-approver.
        let e = ControlEvent::new("FINANCIAL", "budget.spend");
        let r = route(&region(), &state(-1.0), None, &e, &table(), &prefs(), 0).unwrap();
        match &r {
            Routing::Withhold { urgency, threshold } => {
                assert!(urgency.value < *threshold);
            }
            other => panic!("expected Withhold, got {other:?}"),
        }
        assert!(!r.surfaces(), "nobody is interrupted");
        assert!(!matches!(r, Routing::Automate { .. }), "and nothing runs");
    }

    #[test]
    fn the_declared_threshold_moves_the_line() {
        let e = ControlEvent::new("FINANCIAL", "budget.spend");
        let s = state(-50.0);
        let jumpy = route(&region(), &s, None, &e, &table(), &prefs().with_threshold(10.0), 0).unwrap();
        let calm = route(&region(), &s, None, &e, &table(), &prefs().with_threshold(1000.0), 0).unwrap();
        assert!(jumpy.surfaces());
        assert!(!calm.surfaces(), "same event, different declared tolerance for interruption");
    }

    #[test]
    fn a_surfaced_decision_carries_the_stability_verdict_when_one_can_be_made() {
        let e = ControlEvent::new("FINANCIAL", "budget.spend");
        let r = route(&region(), &state(-100.0), Some(&state(-10.0)), &e, &table(), &prefs(), 0).unwrap();
        let Routing::Surface(d) = r else { panic!("should surface") };
        assert!(d.vouched_stable(), "the Controller can vouch: it moves toward V");
    }

    #[test]
    fn an_unassessed_intervention_is_not_vouched_for() {
        // No candidate after-state supplied. "Not assessed" and "assessed and
        // unstable" are different, and neither is a yes.
        let e = ControlEvent::new("FINANCIAL", "budget.spend");
        let r = route(&region(), &state(-100.0), None, &e, &table(), &prefs(), 0).unwrap();
        let Routing::Surface(d) = r else { panic!("should surface") };
        assert!(d.stability.is_none());
        assert!(!d.vouched_stable());
    }

    #[test]
    fn an_unstable_intervention_still_surfaces_but_is_not_vouched_for() {
        // The Controller does not hide a bad idea from the person — it shows it
        // and says it is a bad idea. Filtering is about attention, not truth.
        let e = ControlEvent::new("FINANCIAL", "budget.spend");
        let r = route(&region(), &state(-100.0), Some(&state(-500.0)), &e, &table(), &prefs(), 0).unwrap();
        let Routing::Surface(d) = r else { panic!("should surface") };
        assert!(!d.vouched_stable());
        assert!(should_rollback(d.stability.as_ref().unwrap()));
    }

    // ── declared parameters ─────────────────────────────────────────────────

    #[test]
    fn the_preferences_are_checked_at_authoring_time() {
        assert!(prefs().typecheck().is_ok());
        assert!(matches!(Preferences::default().with_threshold(-1.0).typecheck(),
                         Err(ControllerError::BadThreshold(_))));
        assert!(matches!(Preferences::default().weighing("x", -1.0).typecheck(),
                         Err(ControllerError::BadWeight { .. })));
    }

    #[test]
    fn a_missing_urgency_weight_does_not_make_something_invisible() {
        // Defaulting to 0.0 would silently suppress every unweighted event.
        let p = Preferences::default();
        assert_eq!(p.weight_for("NEVER_DECLARED"), 1.0);
        let u = compute_urgency(&region(), &state(-100.0), &ControlEvent::new("X", "x"), &p, 0).unwrap();
        assert_eq!(u.value, 100.0);
    }

    // ── the loop closes through a human ─────────────────────────────────────

    /// The Controller **cannot** approve. It surfaces the binding a token must
    /// match, and nothing that produces one.
    #[test]
    fn the_controller_surfaces_a_binding_but_cannot_mint_a_token() {
        let e = ControlEvent::new("FINANCIAL", "budget.allocate")
            .with_param("pocket_name", json!("food"))
            .with_param("amount", json!(250.0))
            .with_param("period", json!("monthly"));
        let r = route(&region(), &state(-100.0), None, &e, &table(), &prefs(), 0).unwrap();
        let Routing::Surface(d) = r else { panic!("should surface") };

        assert_eq!(d.binding().operator(), "budget.allocate");
        // There is no `d.approve(..)` — minting still requires the approval
        // chain, which only a human completes. "Advise, the human decides" as a
        // fact about what this type can construct.
    }

    #[test]
    fn monitor_to_controller_to_human_to_gate_end_to_end() {
        use crate::detect::{Cusum, CusumSpec, Ewma, Watch};

        // ── Monitor: a sustained distance from V crosses the boundary ───────
        let mut watch = Watch::new(
            Ewma::new(0.5).unwrap(),
            Cusum::new(CusumSpec::new(0.0, 1.0, 10.0)).unwrap(),
        );
        let readings = watch.observe_all(&[3.0; 20]);
        assert!(readings.iter().any(|r| r.alert.is_some()), "the Monitor sees the drift");

        // ── Controller: filter, then validate ──────────────────────────────
        let params: Map<String, Value> = [
            ("pocket_name".to_string(), json!("food")),
            ("amount".to_string(), json!(250.0)),
            ("period".to_string(), json!("monthly")),
        ]
        .into_iter()
        .collect();
        let event = ControlEvent {
            action_type: "FINANCIAL".into(),
            event_type: "FINANCIAL".into(),
            operator: "budget.allocate".into(),
            params: params.clone(),
            deadline: None,
        };
        let before = json!({"finances": {"liquid": {"balance": -100.0}}});
        let after = json!({"finances": {"liquid": {"balance": -10.0}}});
        let routed = route(&region(), &before, Some(&after), &event, &table(), &prefs(), 0).unwrap();
        let Routing::Surface(decision) = routed else { panic!("must reach a person") };
        assert!(decision.vouched_stable(), "and it is a good decision by the Lyapunov test");

        // ── the human approves — this is the only step that mints a token ───
        let token = Simulated::from_sandbox("ctl-1", decision.binding(), Ok(()))
            .expect("the stability check IS the sandbox run")
            .voted(ProposalStatus::Passed)
            .expect("passed")
            .approve("bonnie", 1, 100);

        // ── the gate: the approved act runs, and only the approved act ─────
        let registry = Registry::default();
        let allowed = registry.names();
        let live_state = json!({"finances": {"liquid": {"balance": 1000.0},
                                             "pockets": {},
                                             "income": {"monthly_total": 0.0, "sources": []}}});
        let mut nonces = NonceLedger::new();
        let ex = execute_admitted(
            &registry, &allowed, &Enforcement::default(), &live_state,
            "budget.allocate", &params,
            &Authorization::Unchecked,
            &EffectClass::Live { token: &token, now: 50 },
            &mut nonces,
        );
        assert!(ex.committed(), "the loop closes: {:?}", ex.result.reason);

        // ...and the same approval cannot be spent twice.
        let replay = execute_admitted(
            &registry, &allowed, &Enforcement::default(), &ex.state,
            "budget.allocate", &params,
            &Authorization::Unchecked,
            &EffectClass::Live { token: &token, now: 50 },
            &mut nonces,
        );
        assert!(!replay.committed(), "single-use holds across the whole loop");
    }
}
