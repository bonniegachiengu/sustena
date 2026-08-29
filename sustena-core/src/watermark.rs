//! Watermarks, and the declared lateness policy (RECORD §IV).
//!
//! ```text
//!   W(τ)  — a lower bound: no event with t_event ≤ W(τ) will be observed
//!           after processing time τ. An ASSERTION ABOUT THE FUTURE.
//!   [a,b) closes  ⟺  W ≥ b
//! ```
//!
//! ## ★★ No window closes by wall-clock hope
//!
//! This is the whole point of the section. §4J.3 names the failure exactly:
//! windows close because a date rolled over — which is a watermark of the form
//! `W(τ) = τ`, i.e. **the assumption that skew is zero**, which §III showed to
//! be false for anything crossing a mobile network.
//!
//! So the watermark is the only key that fits the lock:
//!
//! - [`Window::is_closed_by`] and [`WindowState::close`] take a
//!   **`&Watermark`** and nothing else. There is no `close_at(tau)`. A caller
//!   holding only a processing time cannot ask whether a window is closed —
//!   it has to produce a watermark first.
//! - A watermark has exactly two origins: a **declared source guarantee**
//!   ([`Watermark::perfect`]) or an **estimate over observed skew**
//!   ([`estimate_heuristic`]). There is no third constructor and no `From<i64>`.
//!
//! ★ That does not make `W(τ) = τ` impossible, and it should not — a genuinely
//! in-order source really does have a zero bound. What it does is make the
//! claim **sayable only by saying it**: reproducing the reference's behaviour
//! requires declaring `SourceGuarantee { max_out_of_orderness: 0 }`, in as many
//! words, which is the assertion *this source never delivers out of order*. The
//! failure stops being a default and becomes a signature.
//!
//! ## Monotonicity is structural, and it is not the clamp §III warned about
//!
//! §IV requires `W` non-decreasing in `τ`. [`WatermarkTracker`] holds the mark
//! privately and [`WatermarkTracker::advance`] is the only way to move it, so a
//! watermark cannot go backwards — a proposal that would lower it is reported
//! as [`Advance::HeldBack`] rather than applied or swallowed.
//!
//! ★ Worth separating from EVT-5's rule, because the shapes look alike. There,
//! flooring a *measurement* at zero hides a fact about a clock. Here,
//! non-decreasing **is the definition** of the object — §IV states it as a
//! requirement, a watermark that retreats would un-close a closed window, and
//! the rejected proposal is reported rather than discarded. One conceals an
//! observation; the other maintains an invariant and says when it had to.
//!
//! ## Heuristic is what every real source gives
//!
//! Perfect watermarks exist only where the source guarantees a bound on
//! out-of-orderness. Sustena's dominant source is an SMS forwarder over a
//! mobile network, so **heuristic by construction** — and a heuristic
//! watermark *will sometimes be wrong*.
//!
//! [`estimate_heuristic`] reads the skew distribution EVT-5 surfaces, taking a
//! **declared percentile** of the observed skews as its allowance. Two things
//! it refuses to do:
//!
//! - It never counts a [`Skew::Unknown`] or [`Skew::Inferred`] reading as zero
//!   skew. They contribute nothing, which is what EVT-5's three readings are
//!   for.
//! - With no observed skew at all it returns [`WatermarkError::NoObservedSkew`]
//!   rather than a bound of zero. **An unmeasured source is not a punctual
//!   one**, and the cost of pretending otherwise is a window that closes early
//!   on nothing.
//!
//! ★★ And no amount of data promotes an estimate: `estimate_heuristic` can only
//! ever return [`WatermarkKind::Heuristic`]. Skew is heavy-tailed and unbounded
//! (§III), so even the largest skew ever observed is not a bound on the next
//! one — *"we have seen a lot of data"* is not a guarantee, and there is no
//! path in this module that turns it into one.
//!
//! ## ★ Because it will sometimes be wrong, the lateness policy is mandatory
//!
//! [`Window::new`] takes a [`Lateness`] as an argument. There is no default and
//! `Lateness` implements none, so **a window cannot exist without declaring
//! what happens when the watermark is wrong**. That is the honest completion of
//! a heuristic: you cannot promise the bound holds, so you must say what you do
//! when it does not.
//!
//! Each of the three carries its own cost, and the type carries it too:
//!
//! | policy | the answer | the cost |
//! |---|---|---|
//! | [`Lateness::Drop`] | final | wrong whenever the late fact was material |
//! | [`Lateness::LateFire`] | re-emitted | the consumer must handle a *second* answer for the same window |
//! | [`Lateness::AccumulateAndRetract`] | retraction + correction | the window must **retain its emitted value** |
//!
//! The retained value is real, not described: [`WindowState`] stores the
//! emitted answer **only** under `AccumulateAndRetract`. So the expense §IV
//! names is visible in the state, and a `Drop` window cannot issue a retraction
//! even by mistake — it has nothing to retract.
//!
//! A dropped fact is still **recorded in the late log**, because dropping it
//! from the *answer* is not the same as pretending it never arrived.
//!
//! ## Periods are this object seen twice — and they are the next slice
//!
//! §VIII: `[a,b)` says *which events belong*; `W ≥ b` says *when we may
//! answer*. This module builds the second half and the bare half-open interval
//! the first half needs. Recurrence — `RRULE`, `DTSTART;TZID`, zone anchoring —
//! is EVT-11 and is deliberately not started here.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::clocks::{skew_of, Skew};
use crate::event::Event;

// ── the watermark ───────────────────────────────────────────────────────────

/// A source's declared bound on how far out of order it delivers.
///
/// ★ **Zero is a real claim, not a default.** It asserts that this source never
/// delivers out of order — which is exactly the `W(τ) = τ` assumption §4J.3
/// records as the current failure. It is declarable because some sources
/// genuinely are in order; it is *only* declarable, never assumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceGuarantee {
    /// Who is promising.
    pub source_id: String,
    /// The promised bound, in milliseconds.
    pub max_out_of_orderness: i64,
}

impl SourceGuarantee {
    pub fn new(source_id: impl Into<String>, max_out_of_orderness: i64) -> Self {
        Self { source_id: source_id.into(), max_out_of_orderness }
    }
}

/// Where a watermark's authority comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatermarkKind {
    /// Backed by a declared source guarantee. Only as good as the promise.
    Perfect { guarantee: SourceGuarantee },
    /// Estimated from observed skew. **Will sometimes be wrong** — which is
    /// why the lateness policy exists.
    Heuristic {
        /// The declared percentile of observed skew used as the allowance.
        percentile: u8,
        /// The allowance it produced, in milliseconds.
        allowance: i64,
        /// How many observed skew readings it was estimated from. A small
        /// sample is still a heuristic; saying how small is the honest part.
        sampled: usize,
    },
}

impl WatermarkKind {
    /// Whether a late event is still expected to be possible.
    ///
    /// True for every heuristic — that is what heuristic means.
    pub fn may_be_wrong(&self) -> bool {
        matches!(self, WatermarkKind::Heuristic { .. })
    }
}

/// `W(τ)` — no event with `t_event ≤ mark` will be observed after `tau`.
///
/// Constructible only two ways: from a declared guarantee, or by estimating
/// over observed skew. There is deliberately no `From<i64>`, so a bare
/// processing time cannot become a watermark by being cast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watermark {
    tau: i64,
    mark: i64,
    kind: WatermarkKind,
}

impl Watermark {
    /// A watermark backed by a source's declared bound: `mark = τ − bound`.
    ///
    /// ★ With `max_out_of_orderness: 0` this is `W(τ) = τ` — the §4J.3 failure,
    /// available only by declaring the zero explicitly.
    pub fn perfect(tau: i64, guarantee: SourceGuarantee) -> Self {
        let mark = tau - guarantee.max_out_of_orderness;
        Self { tau, mark, kind: WatermarkKind::Perfect { guarantee } }
    }

    /// The processing time at which this reading was taken.
    pub fn tau(&self) -> i64 {
        self.tau
    }

    /// The bound itself — events at or below this are asserted complete.
    pub fn mark(&self) -> i64 {
        self.mark
    }

    pub fn kind(&self) -> &WatermarkKind {
        &self.kind
    }

    /// Is this event late relative to the bound?
    pub fn is_late(&self, event: &Event) -> bool {
        event.t_event < self.mark
    }
}

/// Holds a watermark and enforces §IV's monotonicity.
///
/// The mark is private and [`advance`](Self::advance) is the only way to move
/// it, so a watermark cannot go backwards. A retreating proposal is **reported**
/// rather than applied or silently dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct WatermarkTracker {
    current: Watermark,
}

/// The outcome of offering a new reading to the tracker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Advance {
    /// The watermark moved forward to this mark.
    Advanced { from: i64, to: i64 },
    /// ★ The proposal would have moved it backwards, so it was **held**. §IV
    /// requires `W` non-decreasing: a retreating watermark would un-close a
    /// window that had already answered. Reported, not swallowed — a source
    /// whose readings keep retreating is a fact worth seeing.
    HeldBack { proposed: i64, held_at: i64 },
}

impl Advance {
    pub fn was_held_back(&self) -> bool {
        matches!(self, Advance::HeldBack { .. })
    }
}

impl WatermarkTracker {
    pub fn new(initial: Watermark) -> Self {
        Self { current: initial }
    }

    pub fn current(&self) -> &Watermark {
        &self.current
    }

    /// Offer a new reading. Moves forward or holds; never backwards.
    pub fn advance(&mut self, proposed: Watermark) -> Advance {
        if proposed.mark > self.current.mark {
            let from = self.current.mark;
            let to = proposed.mark;
            self.current = proposed;
            Advance::Advanced { from, to }
        } else {
            Advance::HeldBack { proposed: proposed.mark, held_at: self.current.mark }
        }
    }
}

/// What can go wrong estimating a heuristic watermark.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WatermarkError {
    /// ★ Not a bound of zero. An unmeasured source is not a punctual one, and
    /// assuming it is closes windows early on no evidence at all.
    #[error(
        "no observed skew to estimate from ({unusable} reading(s) were unknown or inferred, \
         which are not zero skew) — an unmeasured source is not a punctual one, so no \
         watermark is offered"
    )]
    NoObservedSkew { unusable: usize },
    #[error("percentile {0} is not a percentile — declare a value in 0..=100")]
    BadPercentile(u8),
}

/// Estimate a heuristic watermark from the skew EVT-5 surfaces.
///
/// `mark = max(t_event observed) − allowance`, where the allowance is the
/// declared `percentile` of the observed skews.
///
/// ★★ Always [`WatermarkKind::Heuristic`], whatever the sample size. Skew is
/// heavy-tailed and unbounded (§III), so the largest skew ever seen is not a
/// bound on the next one — no volume of data promotes an estimate to a
/// guarantee, and there is no path here that tries.
///
/// Only [`Skew::Observed`] readings count. `Unknown` and `Inferred` are not
/// zero skew and are excluded, not folded in as zeros.
pub fn estimate_heuristic(
    events: &[Event],
    tau: i64,
    percentile: u8,
) -> Result<Watermark, WatermarkError> {
    if percentile > 100 {
        return Err(WatermarkError::BadPercentile(percentile));
    }

    let mut skews: Vec<i64> = Vec::new();
    let mut unusable = 0usize;
    let mut max_seen: Option<i64> = None;

    for e in events {
        match skew_of(e) {
            Skew::Observed(ms) => skews.push(ms),
            Skew::Unknown | Skew::Inferred => unusable += 1,
        }
        max_seen = Some(max_seen.map_or(e.t_event, |m: i64| m.max(e.t_event)));
    }

    if skews.is_empty() {
        return Err(WatermarkError::NoObservedSkew { unusable });
    }

    skews.sort_unstable();
    // Nearest-rank: the smallest value at or above the requested percentile.
    let rank = ((percentile as usize * skews.len()).div_ceil(100)).max(1);
    let allowance = skews[rank - 1];

    // A negative allowance is a clock disagreement, not permission to close
    // EARLIER than the newest event — that would be the watermark overtaking
    // its own evidence. Contributing nothing is the honest floor here.
    let allowance = allowance.max(0);

    let newest = max_seen.expect("skews is non-empty, so at least one event was seen");
    Ok(Watermark {
        tau,
        mark: newest - allowance,
        kind: WatermarkKind::Heuristic { percentile, allowance, sampled: skews.len() },
    })
}

// ── windows ─────────────────────────────────────────────────────────────────

/// What a window does with an event that arrives after it closed.
///
/// **Mandatory.** [`Window::new`] takes one, and this type deliberately
/// implements no `Default` — a heuristic watermark will sometimes be wrong, so
/// a window that has not said what happens then is not fully specified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lateness {
    /// The closed answer is final. The late fact is recorded in the late log
    /// but does not alter the result. Cheapest, and the only stable one —
    /// wrong exactly when the late fact was material.
    Drop,
    /// Re-emit the window with the late fact included. The consumer must be
    /// able to handle a **second answer for the same window**.
    LateFire,
    /// Emit a retraction of the previous answer *and* the corrected one, so a
    /// downstream fold that already consumed the old value can subtract it.
    /// The only policy under which a downstream aggregate stays correct
    /// without recomputation, and the most expensive: the window must retain
    /// what it emitted.
    AccumulateAndRetract,
}

/// A half-open window `[start, end)` with its declared lateness policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    start: i64,
    end: i64,
    lateness: Lateness,
}

/// What can go wrong declaring a window.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WindowError {
    #[error("a half-open window needs start < end; [{start}, {end}) contains nothing")]
    Empty { start: i64, end: i64 },
}

impl Window {
    /// Declare a window. The lateness policy is an argument, not an option.
    pub fn new(start: i64, end: i64, lateness: Lateness) -> Result<Self, WindowError> {
        if start >= end {
            return Err(WindowError::Empty { start, end });
        }
        Ok(Self { start, end, lateness })
    }

    pub fn start(&self) -> i64 {
        self.start
    }

    /// Exclusive — `[start, end)`.
    pub fn end(&self) -> i64 {
        self.end
    }

    pub fn lateness(&self) -> Lateness {
        self.lateness
    }

    /// Does this event's *event time* fall in the window? Half-open, so `end`
    /// belongs to the next window and no event is counted twice.
    pub fn contains(&self, event: &Event) -> bool {
        event.t_event >= self.start && event.t_event < self.end
    }

    /// ★★ Closed **only** by a watermark reaching the end.
    ///
    /// Takes a `&Watermark` and nothing else. There is no processing-time
    /// overload, so closing by wall-clock hope is not expressible from here.
    pub fn is_closed_by(&self, w: &Watermark) -> bool {
        w.mark() >= self.end
    }
}

// ── window state: closing, and what happens when the watermark was wrong ────

/// One late fact, recorded whether or not it changed the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LateRecord {
    pub event_id: String,
    pub t_event: i64,
    /// How far below the closing mark it landed.
    pub behind_by: i64,
}

/// The result of offering a value to close a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Closing {
    /// The watermark reached the end. `may_be_wrong` carries the honest caveat
    /// forward: a heuristic close is an answer, not a proof of completeness.
    Closed { at: i64, revision: u32, may_be_wrong: bool },
    /// ★ Not yet — and by how much. A window that has not been closed by a
    /// watermark is *open*, however late it feels on the wall clock.
    NotYet { needs: i64, watermark: i64 },
}

/// The result of offering a late event to a closed window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LateOutcome {
    /// The window is still open, or the event is not in it. Not a late event
    /// at all — a real answer, not a failure.
    NotLate,
    /// `Drop`: the answer stands, and the fact is on the late log anyway.
    /// Dropping it from the *answer* is not pretending it never arrived.
    Dropped { logged: LateRecord },
    /// `LateFire`: a second answer for the same window, at a new revision.
    LateFired { revision: u32, corrected: Value, logged: LateRecord },
    /// `AccumulateAndRetract`: the previous answer to subtract, and the
    /// corrected one to add.
    Retracted { revision: u32, retracted: Value, corrected: Value, logged: LateRecord },
}

/// A window plus what has happened to it.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowState {
    window: Window,
    closed_at: Option<i64>,
    /// ★ Retained **only** under `AccumulateAndRetract` — §IV's stated cost of
    /// that policy, made real rather than described. A `Drop` window therefore
    /// cannot issue a retraction even by mistake: it has nothing to retract.
    emitted: Option<Value>,
    revision: u32,
    late_log: Vec<LateRecord>,
}

impl WindowState {
    pub fn new(window: Window) -> Self {
        Self { window, closed_at: None, emitted: None, revision: 0, late_log: Vec::new() }
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn is_closed(&self) -> bool {
        self.closed_at.is_some()
    }

    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Every late fact seen, whatever the policy did with it.
    pub fn late_log(&self) -> &[LateRecord] {
        &self.late_log
    }

    /// What was emitted, where the policy retains it.
    pub fn emitted(&self) -> Option<&Value> {
        self.emitted.as_ref()
    }

    /// ★★ Close the window — **only** on a watermark.
    ///
    /// No processing time is accepted anywhere in this signature. A caller with
    /// a clock and no watermark cannot close anything.
    pub fn close(&mut self, w: &Watermark, value: Value) -> Closing {
        if !self.window.is_closed_by(w) {
            return Closing::NotYet { needs: self.window.end, watermark: w.mark() };
        }
        self.closed_at = Some(w.mark());
        self.revision = 1;
        if self.window.lateness == Lateness::AccumulateAndRetract {
            self.emitted = Some(value);
        }
        Closing::Closed {
            at: w.mark(),
            revision: self.revision,
            may_be_wrong: w.kind().may_be_wrong(),
        }
    }

    /// Offer an event that arrived after the window closed.
    ///
    /// `corrected` is the recomputed answer including the late fact — the fold
    /// belongs to the caller, not to this module.
    pub fn admit_late(&mut self, event: &Event, corrected: Value) -> LateOutcome {
        let Some(closed_at) = self.closed_at else {
            return LateOutcome::NotLate;
        };
        if !self.window.contains(event) {
            return LateOutcome::NotLate;
        }

        let logged = LateRecord {
            event_id: event.id.clone(),
            t_event: event.t_event,
            behind_by: closed_at - event.t_event,
        };
        self.late_log.push(logged.clone());

        match self.window.lateness {
            Lateness::Drop => LateOutcome::Dropped { logged },
            Lateness::LateFire => {
                self.revision += 1;
                LateOutcome::LateFired { revision: self.revision, corrected, logged }
            }
            Lateness::AccumulateAndRetract => {
                let retracted = self
                    .emitted
                    .clone()
                    .expect("AccumulateAndRetract retains its emitted value at close");
                self.revision += 1;
                self.emitted = Some(corrected.clone());
                LateOutcome::Retracted {
                    revision: self.revision,
                    retracted,
                    corrected,
                    logged,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CausalStamp, Provenance};
    use serde_json::json;

    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;

    fn ev(id: &str, t_event: i64, t_ingest: Option<i64>) -> Event {
        Event {
            id: id.into(),
            name: "event.finances.pocket_spent".into(),
            t_event,
            t_ingest,
            provenance: Provenance::Observed,
            source: None,
            stamp: CausalStamp::new("phone"),
            causes: vec![],
            mutations: vec![],
            payload: None,
        }
    }

    fn a_window(lateness: Lateness) -> Window {
        Window::new(0, 10 * HOUR, lateness).unwrap()
    }

    // ── ★★ no window closes by wall-clock hope ─────────────────────────────

    #[test]
    fn a_window_will_not_close_on_processing_time_alone() {
        // The processing clock has run well past the window's end. Without a
        // watermark there is nothing to ask — `close` takes only a &Watermark,
        // so this test cannot even be written the wrong way.
        let mut st = WindowState::new(a_window(Lateness::Drop));

        // A source that really does deliver up to two hours out of order.
        let honest = Watermark::perfect(11 * HOUR, SourceGuarantee::new("mpesa", 2 * HOUR));
        assert_eq!(honest.mark(), 9 * HOUR);

        match st.close(&honest, json!(1)) {
            Closing::NotYet { needs, watermark } => {
                assert_eq!(needs, 10 * HOUR);
                assert_eq!(watermark, 9 * HOUR);
            }
            other => panic!("closed on the clock rather than the watermark: {other:?}"),
        }
        assert!(!st.is_closed(), "τ past the end is not a reason to answer");
    }

    #[test]
    fn reproducing_the_reference_requires_declaring_a_zero_bound_out_loud() {
        // ★ W(τ) = τ is exactly SourceGuarantee { max_out_of_orderness: 0 } —
        // the claim "this source never delivers out of order". Available, and
        // only by saying it.
        let hopeful = Watermark::perfect(11 * HOUR, SourceGuarantee::new("mpesa", 0));
        assert_eq!(hopeful.mark(), hopeful.tau(), "this IS W(τ) = τ");

        let mut hoping = WindowState::new(a_window(Lateness::Drop));
        let mut honest = WindowState::new(a_window(Lateness::Drop));
        let realistic = Watermark::perfect(11 * HOUR, SourceGuarantee::new("mpesa", 2 * HOUR));

        assert!(matches!(hoping.close(&hopeful, json!(1)), Closing::Closed { .. }));
        assert!(matches!(honest.close(&realistic, json!(1)), Closing::NotYet { .. }));
        // Same τ, same window — the difference is entirely the declared bound.
    }

    #[test]
    fn a_perfect_close_does_not_carry_the_heuristic_caveat_and_an_estimate_does() {
        let mut a = WindowState::new(a_window(Lateness::Drop));
        let guaranteed = Watermark::perfect(11 * HOUR, SourceGuarantee::new("in_order", 0));
        match a.close(&guaranteed, json!(1)) {
            Closing::Closed { may_be_wrong, .. } => assert!(!may_be_wrong),
            other => panic!("{other:?}"),
        }

        let log = vec![ev("a", 9 * HOUR + MINUTE, Some(9 * HOUR + 2 * MINUTE))];
        let est = estimate_heuristic(&log, 11 * HOUR, 95).unwrap();
        let mut b = WindowState::new(Window::new(0, 9 * HOUR, Lateness::Drop).unwrap());
        match b.close(&est, json!(1)) {
            Closing::Closed { may_be_wrong, .. } => {
                assert!(may_be_wrong, "a heuristic close is an answer, not a proof")
            }
            other => panic!("{other:?}"),
        }
    }

    // ── monotonicity ───────────────────────────────────────────────────────

    #[test]
    fn a_watermark_never_moves_backwards_and_says_when_it_tried_to() {
        let g = SourceGuarantee::new("mpesa", 0);
        let mut t = WatermarkTracker::new(Watermark::perfect(5 * HOUR, g.clone()));

        assert_eq!(
            t.advance(Watermark::perfect(8 * HOUR, g.clone())),
            Advance::Advanced { from: 5 * HOUR, to: 8 * HOUR }
        );

        let held = t.advance(Watermark::perfect(6 * HOUR, g));
        assert_eq!(held, Advance::HeldBack { proposed: 6 * HOUR, held_at: 8 * HOUR });
        assert!(held.was_held_back(), "reported, not silently swallowed");
        assert_eq!(t.current().mark(), 8 * HOUR, "and genuinely did not retreat");
    }

    #[test]
    fn a_closed_window_cannot_be_un_closed_by_a_retreating_reading() {
        // Why §IV requires monotonicity at all: a watermark that went backwards
        // would re-open a window that had already answered.
        let g = SourceGuarantee::new("mpesa", 0);
        let mut t = WatermarkTracker::new(Watermark::perfect(11 * HOUR, g.clone()));
        let mut st = WindowState::new(a_window(Lateness::Drop));
        assert!(matches!(st.close(t.current(), json!(1)), Closing::Closed { .. }));

        t.advance(Watermark::perfect(3 * HOUR, g));
        assert!(st.window().is_closed_by(t.current()), "still closed");
    }

    // ── heuristic estimation, over EVT-5's skew ────────────────────────────

    #[test]
    fn a_heuristic_is_estimated_from_the_observed_skew_not_invented() {
        let log = vec![
            ev("a", HOUR, Some(HOUR + MINUTE)),      // 1 min
            ev("b", 2 * HOUR, Some(2 * HOUR + 5 * MINUTE)),  // 5 min
            ev("c", 3 * HOUR, Some(3 * HOUR + 30 * MINUTE)), // 30 min
        ];
        let w = estimate_heuristic(&log, 4 * HOUR, 100).unwrap();

        match w.kind() {
            WatermarkKind::Heuristic { percentile, allowance, sampled } => {
                assert_eq!(*percentile, 100);
                assert_eq!(*allowance, 30 * MINUTE, "the declared percentile of real skews");
                assert_eq!(*sampled, 3);
            }
            other => panic!("estimation must never mint a guarantee: {other:?}"),
        }
        assert_eq!(w.mark(), 3 * HOUR - 30 * MINUTE, "newest t_event minus the allowance");
    }

    #[test]
    fn no_volume_of_data_promotes_an_estimate_to_a_guarantee() {
        // ★★ Skew is heavy-tailed and unbounded, so the largest ever observed
        // is not a bound on the next. There is no path here that says otherwise.
        let log: Vec<Event> =
            (0..500).map(|i| ev(&format!("e{i}"), i * MINUTE, Some(i * MINUTE + 1))).collect();
        let w = estimate_heuristic(&log, 10 * HOUR, 100).unwrap();
        assert!(w.kind().may_be_wrong());
        assert!(matches!(w.kind(), WatermarkKind::Heuristic { .. }));
    }

    #[test]
    fn an_unmeasured_source_is_not_a_punctual_one() {
        // ★ Unknown and Inferred readings are not zero skew, so they cannot
        // produce a zero allowance. No watermark is offered at all.
        let mut backfilled =
            Event::backfilled("legacy1", "event.test.thing", 5 * HOUR, CausalStamp::new("n"));
        backfilled.t_event = 5 * HOUR;
        let log = vec![ev("no_arrival", HOUR, None), backfilled];

        assert_eq!(
            estimate_heuristic(&log, 9 * HOUR, 95),
            Err(WatermarkError::NoObservedSkew { unusable: 2 })
        );
    }

    #[test]
    fn a_percentile_outside_the_scale_is_refused() {
        let log = vec![ev("a", 0, Some(1))];
        assert_eq!(
            estimate_heuristic(&log, 1, 101),
            Err(WatermarkError::BadPercentile(101))
        );
    }

    #[test]
    fn a_wider_percentile_is_never_a_later_watermark() {
        // Asking for more allowance can only make the bound more conservative.
        let log = vec![
            ev("a", HOUR, Some(HOUR + MINUTE)),
            ev("b", 2 * HOUR, Some(2 * HOUR + HOUR)),
        ];
        let tight = estimate_heuristic(&log, 3 * HOUR, 50).unwrap();
        let wide = estimate_heuristic(&log, 3 * HOUR, 100).unwrap();
        assert!(wide.mark() <= tight.mark());
    }

    // ── ★ the lateness policy is mandatory, and each cost is real ──────────

    #[test]
    fn a_window_cannot_be_declared_without_a_lateness_policy() {
        // Structural rather than asserted: `Window::new` takes a `Lateness`,
        // and `Lateness` implements no `Default`. This test exists to say so —
        // the compiler is what enforces it.
        let w = Window::new(0, HOUR, Lateness::Drop).unwrap();
        assert_eq!(w.lateness(), Lateness::Drop);
        assert_eq!(Window::new(HOUR, HOUR, Lateness::Drop), Err(WindowError::Empty { start: HOUR, end: HOUR }));
    }

    fn closed(lateness: Lateness) -> WindowState {
        let mut st = WindowState::new(a_window(lateness));
        let w = Watermark::perfect(10 * HOUR, SourceGuarantee::new("mpesa", 0));
        assert!(matches!(st.close(&w, json!(100)), Closing::Closed { .. }));
        st
    }

    #[test]
    fn drop_keeps_the_answer_and_logs_the_fact_anyway() {
        let mut st = closed(Lateness::Drop);
        match st.admit_late(&ev("late", 5 * HOUR, Some(11 * HOUR)), json!(140)) {
            LateOutcome::Dropped { logged } => {
                assert_eq!(logged.event_id, "late");
                assert_eq!(logged.behind_by, 5 * HOUR);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(st.revision(), 1, "no second answer");
        assert_eq!(st.late_log().len(), 1, "dropped from the ANSWER, not from the record");
        assert!(st.emitted().is_none(), "and nothing was retained — the cheap policy");
    }

    #[test]
    fn late_fire_produces_a_second_answer_for_the_same_window() {
        let mut st = closed(Lateness::LateFire);
        match st.admit_late(&ev("late", 5 * HOUR, Some(11 * HOUR)), json!(140)) {
            LateOutcome::LateFired { revision, corrected, .. } => {
                assert_eq!(revision, 2, "the consumer must handle a second answer");
                assert_eq!(corrected, json!(140));
            }
            other => panic!("{other:?}"),
        }
        assert!(st.emitted().is_none(), "re-firing does not require retaining the old value");
    }

    #[test]
    fn retract_hands_downstream_both_the_old_value_and_the_new_one() {
        // ★ The only policy under which a downstream fold stays correct without
        // recomputing: it can subtract exactly what it already added.
        let mut st = closed(Lateness::AccumulateAndRetract);
        assert_eq!(st.emitted(), Some(&json!(100)), "the cost: it retained its answer");

        match st.admit_late(&ev("late", 5 * HOUR, Some(11 * HOUR)), json!(140)) {
            LateOutcome::Retracted { retracted, corrected, revision, .. } => {
                assert_eq!(retracted, json!(100));
                assert_eq!(corrected, json!(140));
                assert_eq!(revision, 2);
                let downstream = 100 - retracted.as_i64().unwrap() + corrected.as_i64().unwrap();
                assert_eq!(downstream, 140, "subtract the old, add the new");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(st.emitted(), Some(&json!(140)), "and it retains the correction too");
    }

    #[test]
    fn a_drop_window_has_nothing_to_retract_even_by_mistake() {
        // The expense §IV names is in the state, not only in the prose: only
        // AccumulateAndRetract retains, so the other two structurally cannot
        // issue a retraction.
        for policy in [Lateness::Drop, Lateness::LateFire] {
            assert!(closed(policy).emitted().is_none());
        }
        assert!(closed(Lateness::AccumulateAndRetract).emitted().is_some());
    }

    #[test]
    fn an_event_in_an_open_window_is_not_late() {
        let mut st = WindowState::new(a_window(Lateness::Drop));
        assert_eq!(st.admit_late(&ev("a", HOUR, Some(HOUR)), json!(1)), LateOutcome::NotLate);
    }

    #[test]
    fn an_event_from_a_different_window_is_not_late_here() {
        let mut st = closed(Lateness::LateFire);
        // Half-open: `end` belongs to the next window, so this is not ours.
        assert_eq!(
            st.admit_late(&ev("next", 10 * HOUR, Some(11 * HOUR)), json!(1)),
            LateOutcome::NotLate
        );
        assert!(st.late_log().is_empty());
    }

    #[test]
    fn the_window_is_half_open_so_no_event_is_counted_twice() {
        let first = Window::new(0, HOUR, Lateness::Drop).unwrap();
        let second = Window::new(HOUR, 2 * HOUR, Lateness::Drop).unwrap();
        let boundary = ev("b", HOUR, Some(HOUR));
        assert!(!first.contains(&boundary));
        assert!(second.contains(&boundary));
    }
}
