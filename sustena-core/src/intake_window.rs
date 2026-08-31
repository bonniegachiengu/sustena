//! Where a household's record begins — the Ingest boundary, in time.
//!
//! ★★★ **Why this is a boundary control and not a filter.** The Ingest paper
//! makes τ total: every message that reaches the boundary is classified, and
//! nothing is silently dropped. That is right for messages the household has
//! decided to admit. It says nothing about messages from *before the household
//! existed*, and treating those as intake is a category error: a phone carries
//! years of texts, and starting Sustena on Tuesday does not mean the household
//! began when the handset did.
//!
//! So this sits **in front of** τ. A message older than the start is not
//! classified-and-skipped, it is **not admitted at all** — nothing is stored,
//! nothing is queued, and the classify queue does not open with two thousand
//! decisions nobody asked for. τ stays total over what actually crosses the
//! boundary, which is the property the paper needs.
//!
//! ★★ **It is a decision, and it is recorded as one.** The start is persisted
//! per node, shown in the surfaces, and changeable. A cutoff nobody can see is
//! a household silently missing things it has no way to discover.
//!
//! ★ **Open by default.** With no start set, everything is admitted — the
//! behaviour before this existed. A person turns it on; it never turns itself
//! on.

use serde::{Deserialize, Serialize};

/// When this node began caring about incoming messages.
///
/// Unix seconds, because that is what an SMS carries and what survives a
/// timezone the phone has since left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntakeWindow {
    /// Admit nothing older than this. `None` admits everything.
    pub start_at: Option<i64>,
}

/// Why a message was or was not admitted — reported, never inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// No start is set, so everything is admitted.
    Open,
    /// At or after the start.
    Admitted,
    /// Older than the start. Carries both numbers so a surface can say by how
    /// much, rather than "no".
    Before { at: i64, start: i64 },
    /// The message carries no timestamp.
    ///
    /// ★★★ Admitted, deliberately. A boundary that discards what it cannot
    /// date would silently lose real transactions whenever a provider omits a
    /// timestamp, and a money app losing a payment to a missing field is far
    /// worse than one extra question in the queue. Refusing needs certainty;
    /// admitting only needs doubt.
    Undated,
}

impl Admission {
    pub fn admits(&self) -> bool {
        !matches!(self, Admission::Before { .. })
    }
}

impl IntakeWindow {
    /// Everything admitted.
    pub fn open() -> Self {
        Self { start_at: None }
    }

    /// Admit nothing before `start_at` (unix seconds).
    pub fn starting_at(start_at: i64) -> Self {
        Self { start_at: Some(start_at) }
    }

    pub fn is_open(&self) -> bool {
        self.start_at.is_none()
    }

    /// Should this message cross the boundary?
    pub fn admission(&self, message_at: Option<i64>) -> Admission {
        match (self.start_at, message_at) {
            (None, _) => Admission::Open,
            (Some(_), None) => Admission::Undated,
            (Some(start), Some(at)) if at < start => Admission::Before { at, start },
            (Some(_), Some(_)) => Admission::Admitted,
        }
    }

    /// The short form, for a caller that only needs yes or no.
    pub fn admits(&self, message_at: Option<i64>) -> bool {
        self.admission(message_at).admits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START: i64 = 1_788_000_000;

    #[test]
    fn with_no_start_everything_is_admitted() {
        // ★★★ Open by default: this never turns itself on. A person setting up
        //     Sustena and doing nothing else must lose nothing.
        let w = IntakeWindow::open();
        assert!(w.is_open());
        assert!(w.admits(Some(0)));
        assert!(w.admits(Some(START)));
        assert!(w.admits(None));
        assert_eq!(w.admission(Some(1)), Admission::Open);
    }

    #[test]
    fn a_message_from_before_the_start_does_not_cross_the_boundary() {
        // ★★★ The whole point: starting today must not drag in years of texts.
        let w = IntakeWindow::starting_at(START);
        let old = START - 86_400;
        assert!(!w.admits(Some(old)));
        assert_eq!(w.admission(Some(old)), Admission::Before { at: old, start: START });
    }

    #[test]
    fn the_start_itself_is_admitted() {
        // ★★ "From this moment" includes this moment. An exclusive boundary
        //    would silently drop the message a person set the cutoff to catch.
        let w = IntakeWindow::starting_at(START);
        assert!(w.admits(Some(START)));
        assert_eq!(w.admission(Some(START)), Admission::Admitted);
    }

    #[test]
    fn a_message_with_no_timestamp_is_admitted_rather_than_lost() {
        // ★★★ Refusing needs certainty; admitting only needs doubt. Losing a
        //     real payment to a missing field is far worse than one more
        //     question in the queue.
        let w = IntakeWindow::starting_at(START);
        assert!(w.admits(None));
        assert_eq!(w.admission(None), Admission::Undated);
    }

    #[test]
    fn the_refusal_says_by_how_much() {
        // ★ A surface can then say "three months before you started" rather
        //   than "no", which is the difference between a person understanding
        //   the boundary and distrusting it.
        let w = IntakeWindow::starting_at(START);
        match w.admission(Some(START - 90 * 86_400)) {
            Admission::Before { at, start } => {
                assert_eq!(start - at, 90 * 86_400);
            }
            other => panic!("expected a dated refusal, got {other:?}"),
        }
    }

    #[test]
    fn it_survives_a_round_trip_as_settings() {
        // ★ Persisted per node: a cutoff that forgot itself on restart would
        //   re-open the backlog the person just closed.
        let w = IntakeWindow::starting_at(START);
        let text = serde_json::to_string(&w).expect("encode");
        assert_eq!(serde_json::from_str::<IntakeWindow>(&text).expect("decode"), w);
        let open: IntakeWindow = serde_json::from_str("{}").expect("absent start");
        assert!(open.is_open(), "a missing field is open, not closed");
    }
}
