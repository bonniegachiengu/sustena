//! Stream-window typology — tumbling, sliding and session (MONITOR §IV ·
//! MON-4).
//!
//! ```text
//!   tumbling  W[k]   = { e | kT ≤ t_e < (k+1)T }        adjacent, partitioning
//!   sliding   W(t)   = { e | t−T ≤ t_e < t }            overlapping, step < T
//!   session   W(e_i) = { e_j | gap(e_i, e_j) < τ }      DATA-driven, not clock
//! ```
//!
//! *(Zaharia et al., 2012.)*
//!
//! ## ★★ Tumbling is not a third mechanism, and is not rebuilt here
//!
//! `[kT, (k+1)T)` **is** a half-open period partition — the thing EVT-11
//! already builds. So tumbling has no boundary arithmetic of its own in this
//! module, and reaches it two ways depending on what "T" means:
//!
//! - **Calendar-aligned** (a monthly budget, a weekly rhythm):
//!   [`tumbling_from_recurrence`] is a **one-line delegate** to
//!   [`Recurrence::expand`](crate::period::Recurrence::expand). Months are not
//!   a fixed number of milliseconds, and pretending otherwise is exactly what
//!   EVT-11 exists to prevent.
//! - **Fixed-duration** (a 5-minute bucket): ★★ tumbling is **the degenerate
//!   sliding case where `step == size`**. Nothing separate is written for it —
//!   [`Sliding::is_tumbling`] just reports which case a spec is, and a
//!   conformance case checks a fixed-`T` sliding window and an EVT-11 daily
//!   expansion produce **byte-identical** windows.
//!
//! So the genuinely new work in this row is **sliding** and **session**.
//!
//! ## ★★ All three close on a watermark, and nothing here takes a clock
//!
//! EVT-6's discipline carries unchanged: a window closes when `W ≥ its end`.
//! Sliding and tumbling windows **are** EVT-6 [`Window`]s, so they inherit
//! [`Window::is_closed_by`] and there is no other closer to reach for.
//!
//! A session is the case that could have gone wrong, because its end is not
//! known in advance — so [`SessionState::close_on`] takes a **`&Watermark`**
//! and nothing else. There is **no function in this module that accepts a bare
//! processing time**, which keeps the `W(τ) = τ` failure unspellable here as
//! EVT-6 made it unspellable there.
//!
//! ## ★ Sliding overlaps, and the overlap is the point
//!
//! With `step < size`, an event belongs to **every** window that contains it
//! and is counted in each — that is what makes a rolling rate a rolling rate.
//! [`Sliding::windows_containing`] returns a `Vec` on purpose; nothing dedupes
//! an event to one window.
//!
//! A `step > size` would leave **gaps** between windows, so some events would
//! belong to none and be silently dropped. That is refused at construction
//! ([`WindowingError::StepExceedsSize`]) rather than allowed and documented.
//!
//! ## ★ A session's end is not known until the gap is
//!
//! A session extends while events keep arriving within `τ` and closes on a gap
//! larger than `τ`. So the last session in a batch is **[`SessionState::Open`]**
//! unless a gap was actually observed after it — the same *not yet answerable*
//! honesty EVT-6's [`Closing::NotYet`](crate::watermark::Closing) models.
//!
//! ★★ And there are exactly two ways to close one, both of them evidence: a
//! **gap observed in the data**, or a **watermark past `last_event + τ`**,
//! which guarantees no extending event can still arrive. The wall clock is
//! neither.
//!
//! ## Aggregation is a consumer, not part of the type
//!
//! §IV's example aggregates count / rate / by_type / error_rate. The typology
//! is independent of what is summed over it, so no aggregator is built into
//! these types — a caller folds whatever it likes over the events a window
//! contains.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::event::Event;
use crate::period::{Period, PeriodError, Recurrence, TzProvider};
use crate::watermark::{Lateness, Watermark, Window, WindowError};

/// Which of §IV's three a set of boundaries came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Typology {
    /// Adjacent and partitioning. Either an EVT-11 period expansion or the
    /// degenerate sliding case — never its own arithmetic.
    Tumbling,
    /// Overlapping: `step < size`, so an event can belong to several.
    Sliding,
    /// Data-driven: the boundary is a gap in the events themselves.
    Session,
}

/// What can go wrong declaring or running a windowing.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum WindowingError {
    #[error("a window size must be positive; {0}ms contains nothing")]
    BadSize(i64),
    #[error("a step must be positive; {0}ms would never advance")]
    BadStep(i64),
    /// ★ A step larger than the size leaves gaps between windows, so some
    /// events belong to none — a windowing that silently drops data.
    #[error(
        "a step of {step}ms exceeds the window size of {size}ms, which would leave gaps between \
         windows and silently drop every event that fell in one — refused rather than documented"
    )]
    StepExceedsSize { size: i64, step: i64 },
    #[error("a session gap must be positive; {0}ms would end a session between every pair of events")]
    BadGap(i64),
    #[error("cannot window [{from}, {to}) — the range is empty or inverted")]
    EmptyRange { from: i64, to: i64 },
    #[error(transparent)]
    Window(#[from] WindowError),
    #[error(transparent)]
    Period(#[from] PeriodError),
}

// ── tumbling: EVT-11's periods, delegated ──────────────────────────────────

/// Calendar-aligned tumbling windows — **a one-line delegate to EVT-11**.
///
/// ★★ There is deliberately no boundary arithmetic here. A month is not a fixed
/// number of milliseconds, and a tumbling window over calendar time is exactly
/// the half-open period partition `Recurrence::expand` already produces —
/// including the DST-correct 23- and 25-hour days.
pub fn tumbling_from_recurrence(
    rule: &Recurrence,
    tz: &dyn TzProvider,
    count: usize,
) -> Result<Vec<Period>, WindowingError> {
    Ok(rule.expand(tz, count)?)
}

// ── sliding (and, at step == size, tumbling) ───────────────────────────────

/// A sliding window spec: size `T`, advancing by `step`.
///
/// ★★ At `step == size` this **is** tumbling — adjacent, non-overlapping,
/// partitioning — which is why no separate fixed-duration tumbling type exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sliding {
    size_ms: i64,
    step_ms: i64,
    lateness: Lateness,
}

impl Sliding {
    /// Declare a sliding windowing. `step > size` is refused — see
    /// [`WindowingError::StepExceedsSize`].
    pub fn new(size_ms: i64, step_ms: i64, lateness: Lateness) -> Result<Self, WindowingError> {
        if size_ms <= 0 {
            return Err(WindowingError::BadSize(size_ms));
        }
        if step_ms <= 0 {
            return Err(WindowingError::BadStep(step_ms));
        }
        if step_ms > size_ms {
            return Err(WindowingError::StepExceedsSize { size: size_ms, step: step_ms });
        }
        Ok(Self { size_ms, step_ms, lateness })
    }

    /// A fixed-duration **tumbling** windowing: the degenerate case.
    pub fn tumbling(size_ms: i64, lateness: Lateness) -> Result<Self, WindowingError> {
        Self::new(size_ms, size_ms, lateness)
    }

    pub fn size_ms(&self) -> i64 {
        self.size_ms
    }

    pub fn step_ms(&self) -> i64 {
        self.step_ms
    }

    pub fn lateness(&self) -> Lateness {
        self.lateness
    }

    /// ★ Which of §IV's types this spec actually is.
    pub fn typology(&self) -> Typology {
        if self.is_tumbling() {
            Typology::Tumbling
        } else {
            Typology::Sliding
        }
    }

    pub fn is_tumbling(&self) -> bool {
        self.step_ms == self.size_ms
    }

    /// How many windows any single instant belongs to.
    ///
    /// `1` when tumbling; `ceil(size/step)` when sliding — the overlap factor,
    /// stated as a number rather than left to be inferred.
    pub fn overlap_factor(&self) -> usize {
        // Manual ceiling division: `i64::div_ceil` is unstable on the pinned
        // toolchain, and both operands are positive by construction.
        ((self.size_ms + self.step_ms - 1) / self.step_ms) as usize
    }

    /// Every window whose **start** falls in `[from, to)`, anchored at `origin`.
    ///
    /// Anchoring matters: two callers with different origins produce different
    /// boundaries for the same size and step, so the origin is an argument
    /// rather than an assumed zero.
    pub fn windows(&self, origin: i64, from: i64, to: i64) -> Result<Vec<Window>, WindowingError> {
        if to <= from {
            return Err(WindowingError::EmptyRange { from, to });
        }
        // The first window start at or after `from`, on the origin's grid.
        let k0 = (from - origin).div_euclid(self.step_ms);
        let mut start = origin + k0 * self.step_ms;
        if start < from {
            start += self.step_ms;
        }

        let mut out = Vec::new();
        while start < to {
            out.push(Window::new(start, start + self.size_ms, self.lateness)?);
            start += self.step_ms;
        }
        Ok(out)
    }

    /// ★ **Every** window containing this event — plural on purpose.
    ///
    /// With `step < size` an event belongs to several and is counted in each;
    /// nothing dedupes it to one, because the overlap is what makes a rolling
    /// rate roll.
    pub fn windows_containing(
        &self,
        origin: i64,
        event: &Event,
    ) -> Result<Vec<Window>, WindowingError> {
        let t = event.t_event;
        // Windows starting in (t − size, t] contain t, given half-open ends.
        let first = (t - self.size_ms + 1 - origin).div_euclid(self.step_ms);
        let last = (t - origin).div_euclid(self.step_ms);

        let mut out = Vec::new();
        for k in first..=last {
            let start = origin + k * self.step_ms;
            let w = Window::new(start, start + self.size_ms, self.lateness)?;
            if w.contains(event) {
                out.push(w);
            }
        }
        Ok(out)
    }
}

// ── session: the boundary is in the data ───────────────────────────────────

/// A session windowing: events group while gaps stay under `τ`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSpec {
    gap_ms: i64,
    lateness: Lateness,
}

impl SessionSpec {
    pub fn new(gap_ms: i64, lateness: Lateness) -> Result<Self, WindowingError> {
        if gap_ms <= 0 {
            return Err(WindowingError::BadGap(gap_ms));
        }
        Ok(Self { gap_ms, lateness })
    }

    pub fn gap_ms(&self) -> i64 {
        self.gap_ms
    }

    pub fn lateness(&self) -> Lateness {
        self.lateness
    }
}

/// One session, and whether its end is known yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SessionState {
    /// A gap larger than `τ` was **observed in the data**, so the end is known.
    Closed { window: Window, event_ids: Vec<String> },
    /// ★ Still open — no gap has been observed after it, so **the end is not
    /// known**. Not an empty session and not a closed one: *we cannot answer
    /// yet*, the same state EVT-6's `Closing::NotYet` reports.
    Open { start: i64, last_event: i64, event_ids: Vec<String> },
}

impl SessionState {
    pub fn event_ids(&self) -> &[String] {
        match self {
            SessionState::Closed { event_ids, .. } | SessionState::Open { event_ids, .. } => {
                event_ids
            }
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self, SessionState::Open { .. })
    }

    /// The closed window, if the end is known.
    pub fn window(&self) -> Option<&Window> {
        match self {
            SessionState::Closed { window, .. } => Some(window),
            SessionState::Open { .. } => None,
        }
    }

    /// ★★ Close an open session on a **watermark**.
    ///
    /// A watermark past `last_event + τ` guarantees no event can still arrive
    /// that would extend it — so the gap is established without another event
    /// having to appear. This takes a `&Watermark` **and nothing else**: there
    /// is no processing-time overload, which is what keeps the `W(τ) = τ`
    /// failure unspellable here as EVT-6 made it unspellable there.
    pub fn close_on(
        &self,
        w: &Watermark,
        spec: &SessionSpec,
    ) -> Result<Option<Window>, WindowingError> {
        match self {
            SessionState::Closed { window, .. } => Ok(Some(window.clone())),
            SessionState::Open { start, last_event, .. } => {
                let end = last_event + spec.gap_ms;
                if w.mark() >= end {
                    Ok(Some(Window::new(*start, end, spec.lateness)?))
                } else {
                    Ok(None)
                }
            }
        }
    }
}

/// Group events into sessions by the gaps between them.
///
/// The events must already be in event-time order — this reads the gaps it is
/// given rather than imposing an order, the same way `fold` replays the order
/// it is handed.
pub fn sessionise(
    events: &[Event],
    spec: &SessionSpec,
) -> Result<Vec<SessionState>, WindowingError> {
    let mut out: Vec<SessionState> = Vec::new();
    let mut start: Option<i64> = None;
    let mut last: i64 = 0;
    let mut ids: Vec<String> = Vec::new();

    for e in events {
        match start {
            None => {
                start = Some(e.t_event);
                last = e.t_event;
                ids.push(e.id.clone());
            }
            Some(s) => {
                if e.t_event - last > spec.gap_ms {
                    // ★ The gap was OBSERVED, so this session's end is known.
                    out.push(SessionState::Closed {
                        window: Window::new(s, last + spec.gap_ms, spec.lateness)?,
                        event_ids: std::mem::take(&mut ids),
                    });
                    start = Some(e.t_event);
                }
                last = e.t_event;
                ids.push(e.id.clone());
            }
        }
    }

    // ★ The final session is OPEN — nothing has yet shown that it ended.
    if let Some(s) = start {
        out.push(SessionState::Open { start: s, last_event: last, event_ids: ids });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CausalStamp, Provenance};
    use crate::period::{CivilDateTime, DstPolicy, LocalResolution, TzError};
    use crate::watermark::SourceGuarantee;

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;

    fn ev(id: &str, t: i64) -> Event {
        Event {
            id: id.into(),
            name: "event.finances.pocket_spent".into(),
            t_event: t,
            t_ingest: None,
            provenance: Provenance::Observed,
            source: None,
            stamp: CausalStamp::new("phone"),
            causes: vec![],
            mutations: vec![],
            payload: None,
        }
    }

    struct Utc;
    impl TzProvider for Utc {
        fn tzdata_version(&self) -> String {
            "test".into()
        }
        fn offset_at(&self, _t: &str, _l: CivilDateTime) -> Result<LocalResolution, TzError> {
            Ok(LocalResolution::Unambiguous { offset_ms: 0 })
        }
    }

    // ── ★★ tumbling reuses EVT-11 rather than rebuilding it ────────────────

    #[test]
    fn calendar_tumbling_is_a_delegate_to_evt11_and_keeps_its_dst_correctness() {
        // ★★ A month is not a fixed number of milliseconds. Delegating means
        // February is 28 days here for the same reason it is there.
        let rule = Recurrence::monthly(
            CivilDateTime::date(2026, 1, 1),
            "Etc/UTC",
            1,
            Lateness::Drop,
            DstPolicy::ShiftForward,
        );
        let p = tumbling_from_recurrence(&rule, &Utc, 3).unwrap();

        assert_eq!(p[0].window.end() - p[0].window.start(), 31 * DAY, "January");
        assert_eq!(p[1].window.end() - p[1].window.start(), 28 * DAY, "February 2026");
        for pair in p.windows(2) {
            assert_eq!(pair[0].window.end(), pair[1].window.start(), "adjacent, partitioning");
        }
    }

    #[test]
    fn a_fixed_duration_tumbling_is_the_degenerate_sliding_case() {
        // ★★ THE RECONCILIATION, checked rather than asserted: a Sliding with
        // step == size and an EVT-11 daily expansion produce byte-identical
        // windows over the same span. So there is no third mechanism.
        let rule = Recurrence {
            dtstart: CivilDateTime::date(2026, 1, 1),
            anchor: crate::period::Anchor::Zone { tzid: "Etc/UTC".into() },
            freq: crate::period::Freq::Daily,
            interval: 1,
            by_month_day: None,
            by_day: None,
            lateness: Lateness::Drop,
            dst: DstPolicy::ShiftForward,
        };
        let from_periods = tumbling_from_recurrence(&rule, &Utc, 5).unwrap();
        let origin = from_periods[0].window.start();

        let s = Sliding::tumbling(DAY, Lateness::Drop).unwrap();
        let from_sliding = s.windows(origin, origin, origin + 5 * DAY).unwrap();

        assert_eq!(from_sliding.len(), 5);
        for (a, b) in from_periods.iter().zip(&from_sliding) {
            assert_eq!(&a.window, b, "the same windows, two routes, no third arithmetic");
        }
        assert_eq!(s.typology(), Typology::Tumbling);
        assert_eq!(s.overlap_factor(), 1);
    }

    // ── ★ sliding overlaps, honestly ───────────────────────────────────────

    #[test]
    fn a_sliding_window_overlaps_and_an_event_belongs_to_every_one_containing_it() {
        // ★ Size 60m stepping 15m → 4 overlapping windows cover any instant.
        let s = Sliding::new(60 * MIN, 15 * MIN, Lateness::Drop).unwrap();
        assert_eq!(s.typology(), Typology::Sliding);
        assert_eq!(s.overlap_factor(), 4);

        let e = ev("a", 100 * MIN);
        let containing = s.windows_containing(0, &e).unwrap();
        assert_eq!(containing.len(), 4, "counted in each — that is what makes a rate roll");
        for w in &containing {
            assert!(w.contains(&e));
            assert_eq!(w.end() - w.start(), 60 * MIN);
        }
    }

    #[test]
    fn a_tumbling_window_puts_each_event_in_exactly_one() {
        let s = Sliding::tumbling(HOUR, Lateness::Drop).unwrap();
        for t in [0, 1, HOUR - 1, HOUR, 3 * HOUR + 7] {
            assert_eq!(s.windows_containing(0, &ev("x", t)).unwrap().len(), 1, "at t={t}");
        }
    }

    #[test]
    fn a_step_larger_than_the_size_is_refused_because_it_would_drop_events() {
        // ★ Gaps between windows mean some events belong to none. Refused at
        // construction rather than allowed and documented.
        assert_eq!(
            Sliding::new(10 * MIN, 20 * MIN, Lateness::Drop),
            Err(WindowingError::StepExceedsSize { size: 10 * MIN, step: 20 * MIN })
        );
        assert!(matches!(Sliding::new(0, 1, Lateness::Drop), Err(WindowingError::BadSize(0))));
        assert!(matches!(Sliding::new(10, 0, Lateness::Drop), Err(WindowingError::BadStep(0))));
    }

    #[test]
    fn sliding_windows_are_anchored_to_a_declared_origin() {
        // Two origins, same size and step, different boundaries — so the origin
        // is an argument rather than an assumed zero.
        let s = Sliding::tumbling(HOUR, Lateness::Drop).unwrap();
        let a = s.windows(0, 0, 3 * HOUR).unwrap();
        let b = s.windows(30 * MIN, 0, 3 * HOUR).unwrap();
        assert_ne!(a[0].start(), b[0].start());
        assert_eq!(b[0].start(), 30 * MIN);
    }

    // ── ★ session: the boundary is in the data ─────────────────────────────

    #[test]
    fn a_gap_larger_than_tau_closes_a_session_and_a_smaller_one_does_not() {
        let spec = SessionSpec::new(30 * MIN, Lateness::Drop).unwrap();
        let events = vec![
            ev("a", 0),
            ev("b", 10 * MIN),   // within τ — same session
            ev("c", 25 * MIN),   // within τ — same session
            ev("d", 2 * HOUR),   // a 95-minute gap — new session
            ev("e", 2 * HOUR + 5 * MIN),
        ];
        let sessions = sessionise(&events, &spec).unwrap();

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].event_ids(), ["a", "b", "c"]);
        assert_eq!(sessions[1].event_ids(), ["d", "e"]);

        let first = sessions[0].window().expect("the gap after it was observed");
        assert_eq!(first.start(), 0);
        assert_eq!(first.end(), 25 * MIN + 30 * MIN, "it ends τ after its last event");
    }

    #[test]
    fn the_last_session_is_open_because_no_gap_has_been_observed_after_it() {
        // ★ Not an empty session and not a closed one: we cannot answer yet.
        let spec = SessionSpec::new(30 * MIN, Lateness::Drop).unwrap();
        let sessions = sessionise(&[ev("a", 0), ev("b", 10 * MIN)], &spec).unwrap();

        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].is_open());
        assert!(sessions[0].window().is_none(), "the end is not known");
    }

    #[test]
    fn an_empty_batch_yields_no_sessions_rather_than_an_empty_one() {
        let spec = SessionSpec::new(30 * MIN, Lateness::Drop).unwrap();
        assert!(sessionise(&[], &spec).unwrap().is_empty());
    }

    #[test]
    fn a_session_gap_of_zero_is_refused() {
        assert!(matches!(SessionSpec::new(0, Lateness::Drop), Err(WindowingError::BadGap(0))));
    }

    // ── ★★ everything closes on a watermark, nothing on a clock ────────────

    #[test]
    fn an_open_session_closes_only_when_a_watermark_passes_its_gap() {
        // ★★ `close_on` takes a &Watermark and nothing else. A watermark short
        // of `last + τ` does not close it, however late the processing clock is.
        let spec = SessionSpec::new(30 * MIN, Lateness::Drop).unwrap();
        let sessions = sessionise(&[ev("a", 0), ev("b", 10 * MIN)], &spec).unwrap();
        let open = &sessions[0];

        // The session's last event is at 10m and τ is 30m, so the gap is
        // established only once the watermark reaches 40m. A processing clock
        // at 2h with an honest 45-minute bound is still short of it.
        let late_clock = Watermark::perfect(2 * HOUR, SourceGuarantee::new("mpesa", 90 * MIN));
        assert!(late_clock.mark() < 40 * MIN, "the clock ran on; the guarantee did not");
        assert_eq!(open.close_on(&late_clock, &spec).unwrap(), None, "a clock is not evidence");

        let early = Watermark::perfect(20 * MIN, SourceGuarantee::new("mpesa", 15 * MIN));
        assert_eq!(open.close_on(&early, &spec).unwrap(), None, "the gap is not established");

        let reached = Watermark::perfect(40 * MIN, SourceGuarantee::new("mpesa", 0));
        let closed = open.close_on(&reached, &spec).unwrap().expect("the gap is guaranteed");
        assert_eq!(closed.start(), 0);
        assert_eq!(closed.end(), 40 * MIN);
    }

    #[test]
    fn a_sliding_window_closes_only_on_a_watermark_because_it_is_an_evt6_window() {
        // ★★ Inherited rather than re-implemented: these ARE `Window`s.
        let s = Sliding::new(HOUR, 30 * MIN, Lateness::Drop).unwrap();
        let w = &s.windows(0, 0, HOUR).unwrap()[0];

        let hopeful = Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", 15 * MIN));
        assert!(!w.is_closed_by(&hopeful), "the clock reaching the end is not permission");
        let reached = Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", 0));
        assert!(w.is_closed_by(&reached));
    }

    #[test]
    fn the_declared_lateness_rides_through_every_window_the_typology_produces() {
        // EVT-6 made a lateness policy mandatory for a window; a windowing
        // cannot produce one without it either.
        let s = Sliding::new(HOUR, 30 * MIN, Lateness::AccumulateAndRetract).unwrap();
        for w in s.windows(0, 0, 2 * HOUR).unwrap() {
            assert_eq!(w.lateness(), Lateness::AccumulateAndRetract);
        }
        let spec = SessionSpec::new(30 * MIN, Lateness::LateFire).unwrap();
        let sessions = sessionise(&[ev("a", 0), ev("b", 2 * HOUR)], &spec).unwrap();
        assert_eq!(sessions[0].window().unwrap().lateness(), Lateness::LateFire);
    }

    #[test]
    fn an_empty_or_inverted_range_is_refused() {
        let s = Sliding::tumbling(HOUR, Lateness::Drop).unwrap();
        assert_eq!(
            s.windows(0, 100, 100),
            Err(WindowingError::EmptyRange { from: 100, to: 100 })
        );
    }
}
