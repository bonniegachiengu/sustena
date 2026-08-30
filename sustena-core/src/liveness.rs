//! **Is this source still alive?** (Ingest · §VIII).
//!
//! ```text
//!   learned:      stale ⟺ gap > θ(c),  θ = Percentile_p(observed gaps)
//!   declared:     stale ⟺ gap > h + δ          — a heartbeat, and no statistics
//! ```
//!
//! ★★★ **A capture path that dies is silent, and silence is what it looks like
//! when everything is fine.** A phone whose SMS permission is revoked, a rule
//! that stopped matching, a bank that changed its sender id — every one of them
//! presents as *no new messages*, which is also what a quiet Tuesday looks like.
//! Nothing else in the system can tell the difference, because there is nothing
//! to notice: the failure is an absence.
//!
//! ## Never guess a cadence
//!
//! ★★★ The row says it and it is the whole design constraint. A source with two
//! observed gaps has no cadence — it has two numbers — and declaring a threshold
//! from them would produce an alarm whose only real content is that somebody was
//! busy last week. [`learned_theta`] returns `None` below a floor of
//! observations, and a source with no θ is reported as **unknown**, never as
//! healthy and never as stale.
//!
//! ★★★ **A heartbeat converts a statistical detector into a deterministic
//! one.** If a source promises to speak every `h`, then `gap > h + δ` is a fact
//! rather than an inference — no percentile, no history, no warm-up. Where both
//! exist the heartbeat wins, because a promise beats an estimate.
//!
//! ## Seen when it SPEAKS, not when it parses
//!
//! ★★★ A message nobody could read still proves the source is alive. Treating
//! only successful parses as contact makes a source look dead the moment its
//! wording changes — which is exactly when a person needs to be told *the format
//! changed*, not *the phone is off*. Two different problems, and conflating them
//! sends somebody to check the wrong one.

/// The fewest gaps a cadence can honestly be learned from.
///
/// ★★★ Five gaps is six observations. Below that a "percentile" is a sorted
/// list of two or three numbers, and a threshold from it says more about last
/// week than about this source.
pub const MIN_GAPS: usize = 5;

/// How much later than promised still counts as on time.
///
/// ★★ A declared cadence is a promise about intent, not about the network. A
/// heartbeat that is a minute late has not stopped; one that is an hour late
/// has. `δ` is where a caller draws that, and it is required rather than
/// defaulted so nobody inherits a tolerance they did not choose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Heartbeat {
    /// `h` — how often it promised to speak, in milliseconds.
    pub every_ms: i64,
    /// `δ` — the grace on top.
    pub grace_ms: i64,
}

impl Heartbeat {
    pub fn new(every_ms: i64, grace_ms: i64) -> Self {
        Self { every_ms, grace_ms }
    }

    fn deadline(self) -> i64 {
        self.every_ms + self.grace_ms
    }
}

/// What can be said about a source right now.
#[derive(Debug, Clone, PartialEq)]
pub enum Liveness {
    /// It has spoken recently enough.
    Alive { gap_ms: i64, threshold_ms: i64 },
    /// It has not, and here is what it was measured against.
    Stale { gap_ms: i64, threshold_ms: i64, basis: Basis },
    /// ★★★ Not enough history to have a cadence, and no heartbeat declared.
    /// **Not healthy and not stale** — a third answer, because inventing either
    /// would be a confident claim about a source nobody has watched long enough.
    Unknown { why: String },
}

/// Where the threshold came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// Learned from this source's own observed gaps.
    Observed,
    /// Declared by the source itself.
    Promised,
}

impl Liveness {
    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale { .. })
    }

    pub fn describe(&self, source: &str) -> String {
        match self {
            Self::Alive { .. } => format!("{source} is reporting"),
            Self::Stale { gap_ms, basis, .. } => {
                let hours = *gap_ms as f64 / 3_600_000.0;
                match basis {
                    Basis::Promised => {
                        format!("{source} has not reported for {hours:.1}h — it promised sooner")
                    }
                    Basis::Observed => format!(
                        "{source} has not reported for {hours:.1}h — longer than it usually goes"
                    ),
                }
            }
            Self::Unknown { why } => format!("{source}: {why}"),
        }
    }
}

/// **`θ = Percentile_p(gaps)`** — the cadence a source has actually kept.
///
/// ★★★ `None` below [`MIN_GAPS`]. That is the row's *never guess a cadence*
/// made structural: with three observations there is no distribution to take a
/// percentile of, and a number produced from them would be an alarm about last
/// week wearing a threshold's clothes.
pub fn learned_theta(gaps_ms: &[i64], percentile: f64) -> Option<i64> {
    if gaps_ms.len() < MIN_GAPS || !(0.0..=1.0).contains(&percentile) {
        return None;
    }
    let mut sorted: Vec<i64> = gaps_ms.iter().copied().filter(|g| *g >= 0).collect();
    if sorted.len() < MIN_GAPS {
        return None;
    }
    sorted.sort_unstable();
    // Nearest-rank. Chosen because it always returns a gap this source really
    // had, rather than an interpolated one it never did.
    let rank = ((percentile * sorted.len() as f64).ceil() as usize).max(1);
    Some(sorted[rank.min(sorted.len()) - 1])
}

/// **Is it stale?**
///
/// ★★★ A declared heartbeat wins over a learned cadence: a promise beats an
/// estimate, and a source that says how often it will speak has removed the
/// need to infer it.
pub fn liveness(
    gap_ms: i64,
    heartbeat: Option<Heartbeat>,
    observed_gaps: &[i64],
    percentile: f64,
) -> Liveness {
    if let Some(h) = heartbeat {
        let threshold_ms = h.deadline();
        return if gap_ms > threshold_ms {
            Liveness::Stale { gap_ms, threshold_ms, basis: Basis::Promised }
        } else {
            Liveness::Alive { gap_ms, threshold_ms }
        };
    }

    match learned_theta(observed_gaps, percentile) {
        None => Liveness::Unknown {
            why: format!(
                "only {} gaps observed and no cadence declared — too little to say",
                observed_gaps.len()
            ),
        },
        Some(threshold_ms) => {
            if gap_ms > threshold_ms {
                Liveness::Stale { gap_ms, threshold_ms, basis: Basis::Observed }
            } else {
                Liveness::Alive { gap_ms, threshold_ms }
            }
        }
    }
}

/// **Does this contact count as the source being alive?**
///
/// ★★★ Always yes, whatever the message said. A text nobody could parse still
/// proves the phone is on and the permission is granted. Counting only
/// successful parses makes a source look dead the moment its wording changes,
/// which sends somebody to check the phone when the answer is *the format
/// changed*.
pub fn counts_as_contact(_parsed_successfully: bool) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn gaps(hours: &[i64]) -> Vec<i64> {
        hours.iter().map(|h| h * HOUR).collect()
    }

    #[test]
    fn a_source_with_almost_no_history_has_no_cadence_to_learn() {
        // ★★★ The row's own constraint, made structural. Three observations are
        //     three numbers, not a distribution, and a threshold from them is an
        //     alarm about last week wearing a threshold's clothes.
        assert_eq!(learned_theta(&gaps(&[1, 2, 3]), 0.9), None);
        assert!(learned_theta(&gaps(&[1, 2, 3, 4, 5]), 0.9).is_some());
    }

    #[test]
    fn a_source_nobody_has_watched_long_enough_is_unknown_not_healthy() {
        // ★★★ A third answer. Calling it healthy hides a dead capture path;
        //     calling it stale cries wolf about a source that is simply new.
        let l = liveness(99 * HOUR, None, &gaps(&[1, 2]), 0.9);
        assert!(matches!(l, Liveness::Unknown { .. }));
        assert!(!l.is_stale());
        assert!(l.describe("mpesa").contains("too little to say"));
    }

    #[test]
    fn a_learned_cadence_catches_a_source_that_has_gone_quiet() {
        // ★★★ A capture path that dies is silent, and silence is also what a
        //     quiet Tuesday looks like. This is the only thing that can tell.
        let history = gaps(&[1, 1, 2, 1, 2, 1]);
        let l = liveness(48 * HOUR, None, &history, 0.9);
        assert!(l.is_stale());
        assert!(l.describe("mpesa").contains("longer than it usually goes"));
    }

    #[test]
    fn a_gap_within_what_it_usually_goes_is_not_an_alarm() {
        let history = gaps(&[1, 1, 2, 1, 2, 1]);
        assert!(!liveness(HOUR, None, &history, 0.9).is_stale());
    }

    #[test]
    fn a_promise_beats_an_estimate() {
        // ★★★ A heartbeat converts a statistical detector into a deterministic
        //     one — no percentile, no history, no warm-up.
        let history = gaps(&[1, 1, 1, 1, 1, 1]);
        let beat = Heartbeat::new(24 * HOUR, HOUR);
        let l = liveness(10 * HOUR, Some(beat), &history, 0.9);
        match l {
            Liveness::Alive { threshold_ms, .. } => assert_eq!(threshold_ms, 25 * HOUR),
            other => panic!("the promise should govern: {other:?}"),
        }
    }

    #[test]
    fn a_heartbeat_needs_no_history_at_all() {
        // ★★ Which is the point: a brand-new source that declares a cadence is
        //    watchable from its first minute.
        let l = liveness(30 * HOUR, Some(Heartbeat::new(24 * HOUR, HOUR)), &[], 0.9);
        assert!(l.is_stale());
        assert!(l.describe("the phone").contains("promised sooner"));
    }

    #[test]
    fn the_grace_is_required_rather_than_inherited() {
        // ★★ A declared cadence is a promise about intent, not about the
        //    network. A minute late has not stopped; an hour late has — and
        //    where that line sits belongs to whoever declared it.
        let tight = liveness(25 * HOUR, Some(Heartbeat::new(24 * HOUR, 0)), &[], 0.9);
        let lenient = liveness(25 * HOUR, Some(Heartbeat::new(24 * HOUR, 2 * HOUR)), &[], 0.9);
        assert!(tight.is_stale());
        assert!(!lenient.is_stale());
    }

    #[test]
    fn the_threshold_is_a_gap_the_source_really_had() {
        // ★★ Nearest-rank rather than interpolation, so the number a person is
        //    shown is one this source actually went — not an average of two it
        //    never did.
        let history = gaps(&[1, 2, 3, 4, 100, 6]);
        let theta = learned_theta(&history, 0.9).expect("learned");
        assert!(history.contains(&theta), "{theta} was never an observed gap");
    }

    #[test]
    fn a_message_nobody_could_read_still_proves_the_source_is_alive() {
        // ★★★ Counting only successful parses makes a source look dead the
        //     moment its wording changes — and sends somebody to check the
        //     phone when the answer is "the format changed". Two problems, and
        //     conflating them sends them to the wrong one.
        assert!(counts_as_contact(false));
        assert!(counts_as_contact(true));
    }

    #[test]
    fn a_percentile_outside_its_own_range_learns_nothing() {
        assert_eq!(learned_theta(&gaps(&[1, 2, 3, 4, 5, 6]), 1.5), None);
        assert_eq!(learned_theta(&gaps(&[1, 2, 3, 4, 5, 6]), -0.1), None);
    }

    #[test]
    fn the_reason_travels_with_the_verdict() {
        // ★★ "Stale" alone sends somebody looking; "has not reported for 48
        //    hours, longer than it usually goes" tells them what to look for.
        let l = liveness(48 * HOUR, None, &gaps(&[1, 1, 2, 1, 2, 1]), 0.9);
        match l {
            Liveness::Stale { gap_ms, threshold_ms, basis } => {
                assert_eq!(gap_ms, 48 * HOUR);
                assert!(threshold_ms < gap_ms);
                assert_eq!(basis, Basis::Observed);
            }
            other => panic!("expected stale: {other:?}"),
        }
    }
}
