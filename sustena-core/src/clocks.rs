//! Two clocks, and the skew between them (RECORD §III).
//!
//! ```text
//!   skew(e) = t_ingest(e) − t_event(e)
//! ```
//!
//! ## Skew is the normal case, not an error
//!
//! The world happens before the system hears about it. This is the
//! event-time / processing-time distinction the stream-processing literature
//! settled *(Akidau et al., 2015)*, and nothing here treats a gap between the
//! two clocks as a fault.
//!
//! **It is unbounded and heavy-tailed.** There is no `Δ` such that every event
//! arrives within `Δ`. A forwarding macro that broke for a week produced events
//! with a week of skew, and the correct behaviour was to **accept** them. So
//! there is no threshold in this module, no rejection, and no error type: a
//! skew of a week is read exactly the same way as a skew of a second.
//!
//! Deciding *when you may stop waiting* is a different question — a watermark
//! and a declared lateness policy (§IV) — and it is not answered here. This
//! module is what a watermark would be estimated from, not the watermark.
//!
//! ## ★ Negative skew is a signal, never clamped
//!
//! `t_event > t_ingest` means an event claims to have happened *after* the
//! system heard about it, which is impossible in the world and therefore a
//! real fact about a clock: a device disagrees with the receiver. §III is
//! explicit that it must be *"recorded and surfaced as a data-quality fact,
//! never silently clamped — clamping here [...] relocates the violation
//! somewhere nobody is looking."*
//!
//! That is enforced by shape rather than by discipline:
//!
//! - [`Skew::Observed`] carries a **signed** `i64`. There is no unsigned skew
//!   anywhere, so a negative reading has somewhere to live.
//! - [`skew_of`] is the only way to produce one, and its whole body is one
//!   subtraction — no `max`, no `abs`, no saturation.
//! - [`clock_findings`] takes `&[Event]` — an **immutable** slice. The
//!   surfacing path has no mutable access to the record it is reporting on, so
//!   *writing a corrected time back is not expressible from here*.
//!
//! Surfacing is a function that returns findings, not a comment promising that
//! someone will look.
//!
//! ## ★ Three readings, and two of them are not numbers
//!
//! [`Skew`] has three variants because *"we cannot say"* has two distinct
//! causes, and neither of them is zero:
//!
//! - `Observed(ms)` — both clocks were recorded. May be negative.
//! - `Inferred` — the event time was inferred from the ingest time by a
//!   backfill, so the difference is arithmetically zero and **means nothing**.
//! - `Unknown` — no ingest time was ever recorded.
//!
//! [`Skew::millis`] returns `Option<i64>`, so neither non-numeric reading can
//! decay into a `0` that a caller then averages in as a punctual arrival. The
//! raw fields stay on the record for anyone who wants them; it is the
//! *reading* that refuses to guess.
//!
//! ## Which clock is wrong is a provenance question
//!
//! §III says a negative skew *"means a device clock disagrees with the
//! receiver's"* — and which of the two is the more likely culprit is exactly
//! what the source's declared trust level tells you. A source that **is** the
//! authority for what it reports points the suspicion at the receiver; a
//! source merely relaying an observation points it at itself. With no source
//! declared, [`Suspect::Undetermined`] — which is a real answer, not a guess.

use serde::{Deserialize, Serialize};

use crate::event::{Event, Provenance, Source, Trust};

/// The reading of `t_ingest − t_event`.
///
/// See the module docs: two of the three variants are deliberately not
/// numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Skew {
    /// Both clocks were recorded. **Signed** — a negative value is a signal,
    /// not an impossibility, and is carried as-is.
    Observed(i64),
    /// The event time was inferred from the ingest time (a backfilled legacy
    /// row). The difference is arithmetically zero and means nothing, so it is
    /// reported distinctly rather than as `Observed(0)` — otherwise a backfill
    /// would be indistinguishable from a punctual arrival.
    Inferred,
    /// No ingest time was ever recorded. Not zero — unknown.
    Unknown,
}

impl Skew {
    /// The skew in milliseconds, or `None` when there is no honest number.
    ///
    /// Deliberately `Option`: an inferred or absent reading must not degrade
    /// into a `0` that a caller averages in.
    pub fn millis(&self) -> Option<i64> {
        match self {
            Skew::Observed(ms) => Some(*ms),
            Skew::Inferred | Skew::Unknown => None,
        }
    }

    /// A recorded event time *after* the recorded arrival — the clock
    /// disagreement of §III.
    pub fn is_negative(&self) -> bool {
        matches!(self, Skew::Observed(ms) if *ms < 0)
    }

    /// Both clocks read, and in the ordinary direction.
    pub fn is_normal(&self) -> bool {
        matches!(self, Skew::Observed(ms) if *ms >= 0)
    }
}

/// Read the skew off an event.
///
/// The whole body is one subtraction. There is no clamp, no threshold and no
/// error case — a week of skew and a millisecond of skew are read the same
/// way, because §III says accepting the week is the correct behaviour.
pub fn skew_of(event: &Event) -> Skew {
    match event.t_ingest {
        None => Skew::Unknown,
        Some(_) if event.provenance == Provenance::Legacy => Skew::Inferred,
        Some(t_ingest) => Skew::Observed(t_ingest - event.t_event),
    }
}

/// Which clock the evidence points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Suspect {
    /// The reporting device's clock — it is not the authority for the time it
    /// stamped, so its own disagreement is the simpler explanation.
    TheSource,
    /// The receiver's clock — the source **is** the authority for when this
    /// happened, so the side that disagrees with it is the one recording
    /// arrival.
    TheReceiver,
    /// No source declared, or a derived one. A real answer, not a guess.
    Undetermined,
}

/// One surfaced data-quality fact: this event's clocks disagree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockFinding {
    pub event_id: String,
    /// The signed skew, kept exactly as read.
    pub skew: i64,
    /// How far the event time runs ahead of the arrival, i.e. `-skew`. Stated
    /// as its own positive number because that is the quantity a person reads
    /// ("this device is 40 minutes fast"), and deriving it at every call site
    /// is how sign errors get made.
    pub ahead_by: i64,
    pub source: Option<Source>,
}

impl ClockFinding {
    /// Which clock is the more likely culprit, from the source's declared
    /// trust level.
    pub fn suspect(&self) -> Suspect {
        match self.source.as_ref().map(|s| s.trust) {
            Some(Trust::Authoritative) => Suspect::TheReceiver,
            Some(Trust::Reported) => Suspect::TheSource,
            Some(Trust::Derived) | None => Suspect::Undetermined,
        }
    }

    /// A line a person can read.
    pub fn describe(&self) -> String {
        let who = match self.suspect() {
            Suspect::TheSource => "the reporting source's clock is the likely culprit",
            Suspect::TheReceiver => {
                "the source is authoritative for this time, so the RECEIVER's clock \
                 is the likely culprit"
            }
            Suspect::Undetermined => "which clock is wrong is undetermined",
        };
        format!(
            "event '{}' claims to have happened {}ms AFTER it was heard about \
             (skew {}ms) — {}. Recorded and surfaced, never clamped.",
            self.event_id, self.ahead_by, self.skew, who
        )
    }
}

/// Surface every event whose clocks disagree.
///
/// Takes an **immutable** slice on purpose: this reports, and by construction
/// cannot repair. Clamping a disagreeing clock is not expressible from here.
pub fn clock_findings(events: &[Event]) -> Vec<ClockFinding> {
    events
        .iter()
        .filter_map(|e| match skew_of(e) {
            Skew::Observed(ms) if ms < 0 => Some(ClockFinding {
                event_id: e.id.clone(),
                skew: ms,
                ahead_by: -ms,
                source: e.source.clone(),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{order, CausalStamp, SourceKind};

    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const WEEK: i64 = 7 * DAY;

    fn ev(id: &str, t_event: i64, t_ingest: Option<i64>) -> Event {
        Event {
            id: id.into(),
            name: "event.test.thing".into(),
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

    #[test]
    fn skew_is_the_gap_between_the_two_clocks() {
        assert_eq!(skew_of(&ev("a", 1_000, Some(1_400))), Skew::Observed(400));
    }

    #[test]
    fn a_week_of_skew_is_read_the_same_way_as_a_millisecond_of_it() {
        // §III's own case: a forwarding macro broke for a week, and the correct
        // behaviour was to ACCEPT the events, not reject them. So there is no
        // threshold to trip — the reading is a number either way.
        let punctual = skew_of(&ev("a", 1_000, Some(1_001)));
        let stale = skew_of(&ev("b", 1_000, Some(1_000 + WEEK)));
        assert_eq!(punctual, Skew::Observed(1));
        assert_eq!(stale, Skew::Observed(WEEK));
        assert!(stale.is_normal(), "a week late is still the normal direction");
    }

    #[test]
    fn a_week_late_event_still_orders_by_when_it_happened() {
        // The point of carrying both clocks: the late arrival lands back in its
        // own past, rather than at the end where it turned up.
        let mut log = vec![
            ev("recent", 5 * DAY, Some(5 * DAY)),
            ev("stranded", 1_000, Some(1_000 + WEEK)), // heard about last
        ];
        order(&mut log);
        assert_eq!(log[0].id, "stranded", "ordering is on t_event, not arrival");
    }

    // ── ★ negative skew ────────────────────────────────────────────────────

    #[test]
    fn negative_skew_survives_as_a_negative_number() {
        // t_event AFTER t_ingest. The reading keeps the sign; nothing floors it.
        let s = skew_of(&ev("a", 2_000, Some(1_000)));
        assert_eq!(s, Skew::Observed(-1_000));
        assert_eq!(s.millis(), Some(-1_000));
        assert!(s.is_negative());
        assert!(!s.is_normal());
    }

    #[test]
    fn negative_skew_is_surfaced_as_a_finding_rather_than_swallowed() {
        let log = vec![
            ev("fine", 1_000, Some(1_500)),
            ev("fast_clock", 2_000 + 40 * MINUTE, Some(2_000)),
        ];
        let found = clock_findings(&log);
        assert_eq!(found.len(), 1, "only the disagreeing one is a finding");
        assert_eq!(found[0].event_id, "fast_clock");
        assert_eq!(found[0].skew, -(40 * MINUTE));
        assert_eq!(found[0].ahead_by, 40 * MINUTE, "stated the way a person reads it");
    }

    #[test]
    fn the_sign_survives_a_round_trip_through_serialisation() {
        // The clamp, if one ever crept in, would most plausibly creep in at a
        // boundary. So the negative reading is checked across one.
        let s = skew_of(&ev("a", 9_000, Some(3_000)));
        let json = serde_json::to_string(&s).unwrap();
        let back: Skew = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Skew::Observed(-6_000));
        assert!(back.is_negative(), "a boundary crossing did not floor it");
    }

    #[test]
    fn surfacing_a_disagreement_does_not_alter_the_event() {
        let log = vec![ev("fast_clock", 5_000, Some(1_000))];
        let before = log.clone();
        let _ = clock_findings(&log);
        assert_eq!(log, before, "the record is untouched — this reports, it does not repair");
    }

    // ── ★ which clock is wrong ─────────────────────────────────────────────

    fn with_source(mut e: Event, kind: SourceKind, trust: Trust) -> Event {
        e.source = Some(Source::new("src", kind, trust));
        e
    }

    #[test]
    fn an_authoritative_source_points_the_suspicion_at_the_receiver() {
        let log = vec![with_source(
            ev("a", 5_000, Some(1_000)),
            SourceKind::Connector,
            Trust::Authoritative,
        )];
        let f = &clock_findings(&log)[0];
        assert_eq!(f.suspect(), Suspect::TheReceiver);
        assert!(f.describe().contains("RECEIVER"));
    }

    #[test]
    fn a_merely_reporting_source_points_the_suspicion_at_itself() {
        let log = vec![with_source(
            ev("a", 5_000, Some(1_000)),
            SourceKind::Device,
            Trust::Reported,
        )];
        assert_eq!(clock_findings(&log)[0].suspect(), Suspect::TheSource);
    }

    #[test]
    fn with_no_source_declared_the_culprit_is_undetermined_not_guessed() {
        let log = vec![ev("a", 5_000, Some(1_000))];
        assert_eq!(clock_findings(&log)[0].suspect(), Suspect::Undetermined);
    }

    // ── ★ the two readings that are not numbers ────────────────────────────

    #[test]
    fn an_event_with_no_ingest_time_reads_unknown_and_not_zero() {
        let s = skew_of(&ev("legacy_wire", 1_000, None));
        assert_eq!(s, Skew::Unknown);
        assert_eq!(s.millis(), None, "unknown must not decay into 0ms");
        assert!(!s.is_normal(), "and must not pass for a punctual arrival");
    }

    #[test]
    fn a_backfilled_row_reads_inferred_and_not_zero() {
        // Both clocks are the same number, so the arithmetic is 0 — but that 0
        // says "we only ever had one number", not "it arrived instantly".
        let e = Event::backfilled("legacy1", "event.test.thing", 1_000, CausalStamp::new("n"));
        assert_eq!(e.t_event, 1_000);
        assert_eq!(e.t_ingest, Some(1_000));
        assert_eq!(e.provenance, Provenance::Legacy);

        let s = skew_of(&e);
        assert_eq!(s, Skew::Inferred);
        assert_ne!(s, Skew::Observed(0), "a backfill must not pass for punctual");
        assert_eq!(s.millis(), None);
    }

    #[test]
    fn a_backfilled_row_is_never_reported_as_a_clock_disagreement() {
        // Inferred is neither a normal skew nor a negative one, so it produces
        // no finding — reporting a fabricated disagreement would be as wrong as
        // hiding a real one.
        let e = Event::backfilled("legacy1", "event.test.thing", 1_000, CausalStamp::new("n"));
        assert!(clock_findings(&[e]).is_empty());
    }
}
