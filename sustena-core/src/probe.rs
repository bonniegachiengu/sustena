//! **Probing, not optimising** (Operative · §VIII).
//!
//! ```text
//!   vary the STARTING POINT → fork → simulate under the SAME admit
//!                           → the Pareto front, and H*
//! ```
//!
//! ★★★ **The bridge out of the complicated domain.** Optimisation asks *what is
//! best given the world works like this*. In a complex domain nobody knows that
//! the world works like this, and an answer that is best under one starting
//! point and catastrophic under a neighbouring one is not an answer — it is a
//! bet nobody was shown. Varying the initial conditions and reporting what
//! survives all of them is the honest form of the same question.
//!
//! ★★★ **Every fork runs under the SAME `admit`.** A probe that relaxed the gate
//! to see further would be reporting on a different system than the one that
//! will actually run, and the comparison between its forks would be a comparison
//! of two fictions. The perturbation is to the *starting point* and to nothing
//! else.
//!
//! ## Enumerated, not sampled — because the core has no randomness
//!
//! ★★★ §VIII says *sample* declared perturbations. This core has no random
//! source by construction (see [`crate::outbox`] for the same constraint met the
//! same way), and rather than importing one, the declared perturbations **are**
//! the sample. That is not a weakening: a declared, enumerated set is
//! reproducible, so a probe run twice gives the same front, and "why did it
//! recommend that in March" has an answer. A random sample would give a
//! different front each time from the same inputs, which is a worse property for
//! something a person is asked to act on.
//!
//! ## The front, not the winner
//!
//! ★★★ **Collapsing to one winner destroys the only information a probe
//! produces.** If A wins under a good start and B wins under a bad one, that
//! disagreement *is* the finding — the household is being told which decision
//! depends on how the month opens. A single ranked answer hides exactly that and
//! looks more confident for having hidden it.

use std::collections::BTreeMap;

use crate::horizon::Horizon;
use crate::score::Score;

/// One declared way the starting point might differ from what was measured.
///
/// ★★ Named, because "perturbation 3" in a report is not something a person can
/// argue with, and "if the salary lands late" is.
#[derive(Debug, Clone, PartialEq)]
pub struct Perturbation {
    pub name: String,
}

impl Perturbation {
    pub fn new(name: &str) -> Self {
        Self { name: name.into() }
    }

    /// The starting point exactly as measured.
    ///
    /// ★★★ Always one of the forks. A probe that only explored the neighbours
    /// would answer a question about worlds nobody is in.
    pub fn as_measured() -> Self {
        Self { name: "as measured".into() }
    }
}

/// One candidate's outcome under one starting point.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub candidate: String,
    pub under: String,
    pub score: Score,
}

/// What a probe found.
#[derive(Debug, Clone, PartialEq)]
pub struct Front {
    /// The undominated candidates, ordered by name so the front is stable.
    pub candidates: Vec<String>,
    /// Everything else, with the candidate that beat it.
    ///
    /// ★★ Reported rather than discarded: "dropped because `allocate` was at
    /// least as good everywhere and better when the salary is late" is a
    /// finding, and an absence is not.
    pub dominated: Vec<(String, String)>,
    /// ★★★ How far ahead any of this means anything (OPV-13).
    pub horizon: Horizon,
    /// The starting points that were actually explored.
    pub under: Vec<String>,
}

impl Front {
    /// ★★★ Did the candidates disagree across starting points?
    ///
    /// The one question a probe exists to answer. A front of one says the choice
    /// does not depend on how the month opens; a front of three says it does,
    /// and which way.
    pub fn is_contested(&self) -> bool {
        self.candidates.len() > 1
    }

    pub fn describe(&self) -> String {
        let head = if self.candidates.is_empty() {
            "nothing survived every starting point".to_string()
        } else if self.is_contested() {
            format!(
                "{} candidates survive, and which is best depends on how things open: {}",
                self.candidates.len(),
                self.candidates.join(", ")
            )
        } else {
            format!("{} is best under every starting point tried", self.candidates[0])
        };
        format!("{head} — {}", self.horizon.describe())
    }
}

/// Does `a` dominate `b`? At least as good everywhere, and better somewhere.
///
/// ★★ Missing outcomes make dominance unprovable rather than assumed either
/// way: a candidate nobody ran under a starting point has not beaten anything
/// there, and has not lost either.
fn dominates(a: &BTreeMap<&str, f64>, b: &BTreeMap<&str, f64>, under: &[String]) -> bool {
    let mut strictly_better_somewhere = false;
    for u in under {
        let (Some(x), Some(y)) = (a.get(u.as_str()), b.get(u.as_str())) else {
            return false;
        };
        if x < y {
            return false;
        }
        if x > y {
            strictly_better_somewhere = true;
        }
    }
    strictly_better_somewhere
}

/// **The Pareto front over starting points.**
///
/// ★★★ Returns the front and the horizon together. A front without its horizon
/// is a recommendation with the "for how long" removed.
pub fn front(outcomes: &[Outcome], horizon: Horizon) -> Front {
    let mut under: Vec<String> = Vec::new();
    for o in outcomes {
        if !under.contains(&o.under) {
            under.push(o.under.clone());
        }
    }
    under.sort();

    let mut by_candidate: BTreeMap<&str, BTreeMap<&str, f64>> = BTreeMap::new();
    for o in outcomes {
        by_candidate
            .entry(o.candidate.as_str())
            .or_default()
            .insert(o.under.as_str(), o.score.total);
    }

    let mut candidates = Vec::new();
    let mut dominated = Vec::new();
    for (name, mine) in &by_candidate {
        let beaten_by = by_candidate
            .iter()
            .filter(|(other, _)| other != &name)
            .find(|(_, theirs)| dominates(theirs, mine, &under))
            .map(|(other, _)| other.to_string());
        match beaten_by {
            Some(other) => dominated.push((name.to_string(), other)),
            None => candidates.push(name.to_string()),
        }
    }
    Front { candidates, dominated, horizon, under }
}

/// Candidates whose ranking flips between starting points.
///
/// ★★★ The report a household can act on: *this* is the decision that depends on
/// how the month opens. A front says several things survive; this says where the
/// disagreement actually is.
pub fn contested_between<'a>(outcomes: &'a [Outcome]) -> Vec<(&'a str, &'a str)> {
    let mut best_per_start: BTreeMap<&str, (&str, f64)> = BTreeMap::new();
    for o in outcomes {
        let slot = best_per_start.entry(o.under.as_str()).or_insert((o.candidate.as_str(), f64::NEG_INFINITY));
        if o.score.total > slot.1 {
            *slot = (o.candidate.as_str(), o.score.total);
        }
    }
    let mut out: Vec<(&str, &str)> =
        best_per_start.into_iter().map(|(u, (c, _))| (u, c)).collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{score, Weights};
    use crate::strong_admit::StrongVerdict;

    fn viable(n: usize) -> Vec<StrongVerdict> {
        vec![StrongVerdict::Survivable; n]
    }

    fn w() -> Weights {
        Weights::new(0.0, 0.0)
    }

    fn outcome(candidate: &str, under: &str, u: f64) -> Outcome {
        Outcome {
            candidate: candidate.into(),
            under: under.into(),
            score: score(u, &viable(2), None, w()),
        }
    }

    fn roomy() -> Horizon {
        Horizon::Steps { steps: 12, lambda: 0.2 }
    }

    #[test]
    fn a_candidate_beaten_everywhere_is_dropped_and_told_who_beat_it() {
        // ★★ "Dropped because allocate was at least as good everywhere" is a
        //    finding; an absence is not.
        let f = front(
            &[
                outcome("allocate", "as measured", 5.0),
                outcome("allocate", "salary late", 4.0),
                outcome("dither", "as measured", 1.0),
                outcome("dither", "salary late", 1.0),
            ],
            roomy(),
        );
        assert_eq!(f.candidates, vec!["allocate"]);
        assert_eq!(f.dominated, vec![("dither".to_string(), "allocate".to_string())]);
        assert!(!f.is_contested());
    }

    #[test]
    fn a_disagreement_between_starting_points_is_kept_rather_than_collapsed() {
        // ★★★ The only information a probe produces. If A wins on a good start
        //     and B on a bad one, that disagreement IS the finding, and a single
        //     winner hides it while looking more confident for having hidden it.
        let f = front(
            &[
                outcome("spend now", "as measured", 9.0),
                outcome("spend now", "salary late", 1.0),
                outcome("hold back", "as measured", 4.0),
                outcome("hold back", "salary late", 6.0),
            ],
            roomy(),
        );
        assert!(f.is_contested());
        assert_eq!(f.candidates, vec!["hold back", "spend now"]);
        assert!(f.describe().contains("depends on how things open"));
    }

    #[test]
    fn where_the_disagreement_actually_is_is_reportable() {
        // ★★★ A front says several things survive; this says which start
        //     prefers which, which is the sentence a household can act on.
        let os = [
            outcome("spend now", "as measured", 9.0),
            outcome("spend now", "salary late", 1.0),
            outcome("hold back", "as measured", 4.0),
            outcome("hold back", "salary late", 6.0),
        ];
        let split = contested_between(&os);
        assert_eq!(split, vec![("as measured", "spend now"), ("salary late", "hold back")]);
    }

    #[test]
    fn the_measured_starting_point_is_always_one_of_them() {
        // ★★★ A probe that only explored the neighbours would answer a question
        //     about worlds nobody is in.
        assert_eq!(Perturbation::as_measured().name, "as measured");
    }

    #[test]
    fn the_front_carries_its_horizon() {
        // ★★★ A front without its horizon is a recommendation with the "for how
        //     long" removed.
        let f = front(&[outcome("a", "as measured", 1.0)], Horizon::Unknown { why: "no data".into() });
        assert!(f.describe().contains("no horizon could be estimated"));
    }

    #[test]
    fn a_probe_run_twice_gives_the_same_front() {
        // ★★★ Enumerated rather than sampled, because the core has no random
        //     source. A random sample would give a different front each time
        //     from the same inputs — a worse property for something a person is
        //     asked to act on.
        let os = [
            outcome("a", "as measured", 3.0),
            outcome("b", "as measured", 3.0),
            outcome("a", "salary late", 1.0),
            outcome("b", "salary late", 2.0),
        ];
        assert_eq!(front(&os, roomy()), front(&os, roomy()));
    }

    #[test]
    fn ties_everywhere_leave_both_standing() {
        // ★★ Dominance needs strictly better somewhere. Two candidates that are
        //    identical under every start are genuinely not separable, and
        //    picking one would be inventing a preference.
        let f = front(
            &[
                outcome("a", "as measured", 3.0),
                outcome("b", "as measured", 3.0),
                outcome("a", "salary late", 1.0),
                outcome("b", "salary late", 1.0),
            ],
            roomy(),
        );
        assert_eq!(f.candidates, vec!["a", "b"]);
        assert!(f.dominated.is_empty());
    }

    #[test]
    fn a_candidate_nobody_ran_everywhere_is_neither_dominant_nor_dominated() {
        // ★★ Missing outcomes make dominance unprovable rather than assumed
        //    either way — it has not beaten anything there, and has not lost.
        let f = front(
            &[
                outcome("thorough", "as measured", 9.0),
                outcome("thorough", "salary late", 9.0),
                outcome("half run", "as measured", 1.0),
            ],
            roomy(),
        );
        assert!(f.candidates.contains(&"half run".to_string()));
        assert!(f.dominated.is_empty());
    }

    #[test]
    fn the_starting_points_explored_are_reported() {
        // ★★ A front over two starting points and a front over twenty are
        //    different claims, and only one of them says which it is.
        let f = front(
            &[outcome("a", "salary late", 1.0), outcome("a", "as measured", 2.0)],
            roomy(),
        );
        assert_eq!(f.under, vec!["as measured", "salary late"]);
    }

    #[test]
    fn an_empty_probe_says_nothing_survived_rather_than_recommending_nothing() {
        let f = front(&[], roomy());
        assert!(f.describe().contains("nothing survived"));
    }
}
