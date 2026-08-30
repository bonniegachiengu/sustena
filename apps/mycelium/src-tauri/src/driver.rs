//! **The loop that runs, and the queue a person can see** (Controller · §IX).
//!
//! ```text
//!   WHILE system.running:
//!       observe → orient → decide → act
//!       anything surfaced goes into a queue somebody can look at
//! ```
//!
//! ★★★ **The core deliberately has no clock and no scheduler**, and the machine
//! it does have ([`sustena_core::ooda::Ooda`]) is a state machine somebody has
//! to step. This is the somebody. Keeping it here rather than in the core is
//! ADR-0001's line: the decision logic is testable and replayable precisely
//! because nothing inside it knows what time it is.
//!
//! ★★★ **So the clock is a parameter, not a call.** `tick(now_ms)` means a test
//! can ask *what happens after three days* without waiting three days, and two
//! replays of the same log reach the same state. A driver that read the clock
//! itself would be untestable in exactly the cases that matter.
//!
//! ## No hidden backlog
//!
//! ★★★ **A queue a person cannot see is a system making decisions by not making
//! them.** Everything the loop parks at DECIDE goes into [`Driver::awaiting`],
//! with how long it has been waiting — because *"three things need you"* is a
//! prompt and *"something is pending somewhere"* is a shrug. A decision that has
//! sat for a week has effectively been decided against, and the only honest
//! thing to do is say so.
//!
//! ★★★ **The driver cannot authorize.** `tick` has no parameter through which an
//! approval token could arrive, so the loop can observe, orient, decide and act
//! on something already authorized, and it structurally cannot approve its own
//! surfaced decision. That is the one property that would be worth nothing as a
//! convention.

use std::collections::BTreeMap;

use serde_json::Value;
use sustena_core::controller::{AutomationTable, Preferences, SurfacedDecision};
use sustena_core::ooda::{Ooda, OodaObservation, OodaStep, Orientation};
use sustena_core::region::Region;

/// How long a surfaced decision may wait before the queue says so.
///
/// ★★ A declared number rather than a feeling. Three days is long enough that a
/// weekend does not raise it and short enough that a week cannot pass quietly.
pub const STALE_AFTER_MS: u64 = 3 * 24 * 60 * 60 * 1_000;

/// One thing waiting for a person.
#[derive(Debug, Clone)]
pub struct Waiting {
    pub sustain_id: String,
    pub decision: SurfacedDecision,
    /// When the loop parked it.
    pub since_ms: u64,
}

impl Waiting {
    pub fn waited_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.since_ms)
    }

    /// ★★★ Has this been waiting long enough that not deciding IS the decision?
    pub fn effectively_decided_against(&self, now_ms: u64) -> bool {
        self.waited_ms(now_ms) > STALE_AFTER_MS
    }
}

/// What one pass of the loop did for one sustain.
#[derive(Debug, Clone)]
pub enum Pass {
    /// Nothing needed doing.
    Quiet,
    /// It went as far as it could and parked for a person.
    Surfaced,
    /// It acted without asking, above the approval line.
    Acted,
    /// ★★★ It could not run, and the reason is kept rather than swallowed.
    ///
    /// A loop that silently skipped a sustain would look identical to a quiet
    /// one, and the difference is the whole of whether the system is working.
    Failed { why: String },
}

/// The host's loop.
#[derive(Default)]
pub struct Driver {
    machines: BTreeMap<String, Ooda>,
    waiting: Vec<Waiting>,
}

impl Driver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything parked for a person, oldest first.
    ///
    /// ★★★ Public and ordered. The oldest is the one most likely to have been
    /// decided by neglect, so it is the one a surface should show first.
    pub fn awaiting(&self) -> Vec<&Waiting> {
        let mut out: Vec<&Waiting> = self.waiting.iter().collect();
        out.sort_by_key(|w| w.since_ms);
        out
    }

    /// How many things are waiting. The leading health number for this loop.
    pub fn depth(&self) -> usize {
        self.waiting.len()
    }

    /// Decisions that have waited past the declared bound.
    pub fn stale(&self, now_ms: u64) -> Vec<&Waiting> {
        self.waiting.iter().filter(|w| w.effectively_decided_against(now_ms)).collect()
    }

    /// A sentence a surface can show without further arithmetic.
    pub fn describe(&self, now_ms: u64) -> String {
        match (self.depth(), self.stale(now_ms).len()) {
            (0, _) => "nothing is waiting on you".into(),
            (n, 0) => format!("{n} thing(s) need you"),
            (n, s) => format!(
                "{n} thing(s) need you, and {s} have waited long enough that not deciding is \
                 the decision"
            ),
        }
    }

    /// **One pass over one sustain.**
    ///
    /// ★★ Takes the observation rather than fetching it. The driver decides
    /// *when* things happen, and what is true is somebody else's job — which is
    /// what keeps this testable without a world attached.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        sustain_id: &str,
        observation: OodaObservation,
        orientation: Orientation,
        region: &Region,
        state: &Value,
        table: &AutomationTable,
        prefs: &Preferences,
        now_ms: u64,
    ) -> Pass {
        let machine = self.machines.entry(sustain_id.to_string()).or_default();

        // ★★ A sustain already parked for a person is not re-driven. Stepping
        //    it again would surface the same decision twice and make the queue
        //    a count of ticks rather than of things needing attention.
        if machine.awaiting_authorization() {
            return Pass::Quiet;
        }

        match machine.observe(observation) {
            Ok(OodaStep::Stayed(_)) => return Pass::Quiet,
            Ok(_) => {}
            Err(e) => return Pass::Failed { why: e.to_string() },
        }
        if let Err(e) = machine.orient(orientation) {
            return Pass::Failed { why: e.to_string() };
        }
        match machine.decide(region, state, table, prefs, now_ms) {
            Ok(OodaStep::Surfaced(decision)) => {
                self.waiting.push(Waiting {
                    sustain_id: sustain_id.to_string(),
                    decision: *decision,
                    since_ms: now_ms,
                });
                Pass::Surfaced
            }
            Ok(OodaStep::Automated { .. }) => Pass::Acted,
            Ok(_) => Pass::Quiet,
            Err(e) => Pass::Failed { why: e.to_string() },
        }
    }

    /// Take one waiting decision off the queue, because a person dealt with it.
    ///
    /// ★★★ Returns whether anything was actually there. A drain that silently
    /// succeeded on an empty queue would let a surface report *handled* for
    /// something nobody handled.
    pub fn resolve(&mut self, sustain_id: &str) -> bool {
        let before = self.waiting.len();
        self.waiting.retain(|w| w.sustain_id != sustain_id);
        self.waiting.len() < before
    }

    /// Is this sustain's machine parked for a person?
    pub fn parked(&self, sustain_id: &str) -> bool {
        self.machines.get(sustain_id).is_some_and(|m| m.awaiting_authorization())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sustena_core::controller::{ControlEvent, SheridanLevel, Urgency};
    use sustena_core::detect::Severity;

    const HOUR: u64 = 3_600_000;

    fn surfaced(sustain: &str, since_ms: u64) -> Waiting {
        Waiting {
            sustain_id: sustain.into(),
            decision: SurfacedDecision {
                event: ControlEvent::new("spend", "budget.spend"),
                level: SheridanLevel::ExecuteIfApproved,
                urgency: Urgency {
                    value: 0.5,
                    base: 0.5,
                    lyapunov: 0.0,
                    time_pressure: 0.0,
                    lyapunov_exact: true,
                },
                stability: None,
            },
            since_ms,
        }
    }

    fn with(waiting: Vec<Waiting>) -> Driver {
        Driver { machines: BTreeMap::new(), waiting }
    }

    #[test]
    fn the_queue_is_visible_and_oldest_first() {
        // ★★★ A queue a person cannot see is a system making decisions by not
        //     making them, and the oldest is the one most likely to have been
        //     decided by neglect.
        let d = with(vec![surfaced("b", 5 * HOUR), surfaced("a", HOUR)]);
        let q = d.awaiting();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].sustain_id, "a");
    }

    #[test]
    fn nothing_waiting_says_so_plainly() {
        assert_eq!(with(vec![]).describe(0), "nothing is waiting on you");
    }

    #[test]
    fn three_things_need_you_is_a_prompt_and_something_is_pending_is_a_shrug() {
        let d = with(vec![surfaced("a", 0), surfaced("b", 0), surfaced("c", 0)]);
        assert_eq!(d.describe(HOUR), "3 thing(s) need you");
    }

    #[test]
    fn a_decision_that_waited_a_week_has_effectively_been_decided_against() {
        // ★★★ Saying so is the only honest option. Leaving it in a list that
        //     reads the same on day one and day nine is how a system decides by
        //     neglect and keeps looking like it is waiting.
        let d = with(vec![surfaced("a", 0)]);
        let a_week = 7 * 24 * HOUR;
        assert_eq!(d.stale(a_week).len(), 1);
        assert!(d.describe(a_week).contains("not deciding is the decision"));
    }

    #[test]
    fn a_decision_over_a_weekend_is_not_stale() {
        // ★★ The bound is declared so a weekend does not raise it and a week
        //    cannot pass quietly.
        let d = with(vec![surfaced("a", 0)]);
        assert!(d.stale(2 * 24 * HOUR).is_empty());
    }

    #[test]
    fn resolving_says_whether_anything_was_actually_there() {
        // ★★★ A drain that silently succeeded on an empty queue would let a
        //     surface report "handled" for something nobody handled.
        let mut d = with(vec![surfaced("a", 0)]);
        assert!(d.resolve("a"));
        assert!(!d.resolve("a"));
        assert_eq!(d.depth(), 0);
    }

    #[test]
    fn resolving_one_sustain_leaves_the_others_waiting() {
        let mut d = with(vec![surfaced("a", 0), surfaced("b", 0)]);
        assert!(d.resolve("a"));
        assert_eq!(d.depth(), 1);
        assert_eq!(d.awaiting()[0].sustain_id, "b");
    }

    #[test]
    fn the_clock_is_a_parameter_so_three_days_can_be_tested_without_waiting_three_days() {
        // ★★★ A driver that read the clock itself would be untestable in
        //     exactly the cases that matter.
        let d = with(vec![surfaced("a", 0)]);
        assert!(d.stale(0).is_empty());
        assert_eq!(d.stale(STALE_AFTER_MS + 1).len(), 1);
    }

    #[test]
    fn how_long_something_waited_is_never_negative() {
        // ★★ A clock that goes backwards is a real thing on a phone, and a
        //    negative wait would read as a decision from the future.
        let w = surfaced("a", 10 * HOUR);
        assert_eq!(w.waited_ms(0), 0);
    }

    #[test]
    fn there_is_no_parameter_through_which_the_driver_could_authorize() {
        // ★★★ Structural, and the one property that would be worth nothing as a
        //     convention: `tick` takes an observation, an orientation, a region,
        //     a state, a table, preferences and a clock. No token.
        let d = Driver::new();
        assert_eq!(d.depth(), 0);
        assert!(!d.parked("anything"));
    }

    #[test]
    fn a_severity_that_does_not_escalate_leaves_the_loop_quiet() {
        // ★★ The common case, and it must not enqueue anything — a loop that
        //    parked something every tick would make the queue a count of ticks.
        let o = OodaObservation::from_severity(Severity::Info);
        assert!(!o.escalates());
    }
}
