//! **`C = ⟨Plant, Sensor, Governor, Actuator⟩`** — the loop as one object
//! (Controller · §I).
//!
//! ```text
//!   e = r − y          what is wrong, measured against the viable region
//!   u = G(e)           what to do about it
//!   y' = P(y, u)       what happens when somebody does
//! ```
//!
//! ★★★ **Every piece of this existed; the LOOP did not.** `region.rs` gives
//! `e`, `controller.rs` gives Lyapunov and the Sheridan dispatch, `ooda.rs`
//! gives the phases — and each caller assembled them in whatever order it
//! chose. Four callers is four loops, and the day two of them disagree about
//! whether to measure before or after the candidate, nobody can say which is
//! the Controller.
//!
//! ★★★ **The Actuator is deliberately NOT here, and that is the design.**
//! §I closes the loop *through a human*, and ADR-0001 keeps I/O out of the
//! core. So [`ControlSystem::step`] computes what the loop would do and returns
//! it; acting is the host's, and deciding is the person's. A `step` that could
//! act would be a loop that closed through itself, which is the one shape
//! supervisory control is defined in opposition to.
//!
//! ## What the Governor may and may not do
//!
//! ★★★ **It never proposes a move that increases `W`.** Lyapunov is not a
//! preference among candidates; it is the definition of a good decision
//! (CTL-2), so a candidate that moves away from the region is not ranked last —
//! it is not a candidate. A governor that offered one would be offering to make
//! things worse and calling it an option.
//!
//! ★★ **Inside the region it proposes nothing.** `e = 0` means nothing is
//! wrong, and a controller that acts anyway is the over-corrector CTL-11 exists
//! to damp. Doing nothing is a real answer and this one says it out loud.
//!
//! ★★ **Ties are broken by declared preference, never by iteration order.** Two
//! candidates that reduce `W` by the same amount are genuinely equal to the
//! loop; picking whichever the vector happened to hold first would make the
//! answer depend on something nobody declared.

use serde_json::Value;

use crate::controller::{is_stable_intervention, ControllerError, Stability};
use crate::region::Region;

/// `y` — what the Sensor read.
///
/// ★★ A named type rather than a bare `Value`, because a reading is a *claim
/// about a moment* and passing state around unlabelled is how a stale one gets
/// compared against a fresh one.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub state: Value,
    /// `e = r − y`, as the weighted distance to the region.
    pub error: f64,
    /// ★★ Whether `e` is exact or a lower bound. `Region::distance` can only
    /// bound the error when a cross-dimension relation fails, and a controller
    /// that forgot which it had would act on a number it could not defend.
    pub exact: bool,
}

impl Reading {
    /// Inside the region, nothing is wrong.
    pub fn settled(&self) -> bool {
        self.error <= 0.0
    }
}

/// One thing the loop could do, and what the world would look like after.
#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub name: String,
    /// The candidate post-state, from the simulator — never applied here.
    pub after: Value,
    /// What the household said it prefers, when two moves are equally good.
    ///
    /// ★★ Declared rather than derived. A tie broken by iteration order is an
    /// answer that depends on something nobody chose.
    pub preference: i32,
}

impl Move {
    pub fn new(name: &str, after: Value) -> Self {
        Self { name: name.into(), after, preference: 0 }
    }

    pub fn preferred(mut self, weight: i32) -> Self {
        self.preference = weight;
        self
    }
}

/// `u = G(e)` — what the Governor would do, and why.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    /// `e = 0`. Nothing is wrong, so nothing is proposed.
    Settled,
    /// Something is wrong and no move on offer improves it.
    ///
    /// ★★★ A real and important answer, not a failure. Reporting "no action"
    /// as an error would push a caller toward acting anyway; saying *nothing
    /// here helps* is what lets a person go and find something that does.
    NoMoveHelps { error: f64, considered: usize },
    /// Do this, and here is the descent it buys.
    Act { chosen: Move, stability: Stability, descent: f64 },
}

impl Control {
    pub fn acts(&self) -> bool {
        matches!(self, Self::Act { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Settled => "inside the viable region — nothing to do".into(),
            Self::NoMoveHelps { error, considered } => format!(
                "{considered} moves considered, none of them closes the gap of {error:.2}"
            ),
            Self::Act { chosen, descent, .. } => {
                format!("'{}' closes {descent:.2} of the gap", chosen.name)
            }
        }
    }
}

/// **How long until this is over?** — `descent ≥ α ⇒ ⌈W/α⌉ steps`
/// (Sustain · §VII).
///
/// ★★★ **A descent that shrinks is not a descent.** Lyapunov guarantees each
/// step moves toward the region; it does not guarantee arrival. A loop that
/// closes half the remaining gap every time descends forever and never gets
/// there, and every individual step passes the stability check. The bound is
/// what turns *this is improving* into *this ends*, and it needs a floor on the
/// rate rather than the rate of one step.
///
/// ★★ `None` when `α ≤ 0`. There is no number of steps that gets there, and
/// returning a large one would be a promise with nothing behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runway {
    /// Already inside.
    Arrived,
    /// At the guaranteed rate, this many turns.
    Steps(u64),
    /// No guaranteed rate, so no honest answer.
    ///
    /// ★★★ Named rather than reported as a very large number. "Never, at this
    /// rate" is a real and actionable fact — it says look for a bigger move —
    /// and a big number would read as patience being enough.
    Unbounded,
}

impl Runway {
    pub fn describe(&self) -> String {
        match self {
            Self::Arrived => "already inside the region".into(),
            Self::Steps(1) => "one more turn at this rate".into(),
            Self::Steps(n) => format!("{n} turns at this rate"),
            Self::Unbounded => {
                "nothing on offer closes the gap at a rate that ever arrives".into()
            }
        }
    }
}

/// `⌈W/α⌉` — turns to re-enter, given a guaranteed per-step descent `α`.
pub fn runway(error: f64, alpha: f64) -> Runway {
    if error <= 0.0 {
        return Runway::Arrived;
    }
    if alpha <= 0.0 {
        return Runway::Unbounded;
    }
    Runway::Steps((error / alpha).ceil() as u64)
}

/// One turn of the loop, with every quantity it used.
///
/// ★★ The reading is carried alongside the decision so a surface can show *what
/// it saw* next to *what it suggests*. A recommendation whose evidence is not
/// beside it is one a person has to take on trust.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub reading: Reading,
    pub control: Control,
    /// ★★ How many more turns like this one, if the loop keeps managing the
    /// descent it just managed. A projection, not a promise — which is why it
    /// is computed from the descent actually achieved rather than from a rate
    /// somebody declared and nothing checks.
    pub runway: Runway,
}

/// `C = ⟨Plant, Sensor, Governor, Actuator⟩`.
///
/// ★★ Plant and Sensor are one field here on purpose: the plant is the
/// household, and reading it IS taking its state. Splitting them would invent a
/// component with nothing in it.
pub struct ControlSystem<'a> {
    /// `r` — the reference the error is measured against.
    pub reference: &'a Region,
}

impl<'a> ControlSystem<'a> {
    pub fn new(reference: &'a Region) -> Self {
        Self { reference }
    }

    /// `y`, and `e = r − y` with it.
    pub fn sense(&self, plant: &Value) -> Result<Reading, ControllerError> {
        let d = self.reference.distance(plant)?;
        Ok(Reading { state: plant.clone(), error: d.weighted, exact: d.is_exact() })
    }

    /// **One turn.** Sense, then decide — and stop.
    ///
    /// ★★★ Returns what to do. Doing it is the host's, and authorising it is
    /// the person's; a `step` that acted would close the loop through itself,
    /// which is the shape supervisory control is defined against.
    pub fn step(&self, plant: &Value, moves: &[Move]) -> Result<Step, ControllerError> {
        let reading = self.sense(plant)?;

        if reading.settled() {
            return Ok(Step { reading, control: Control::Settled, runway: Runway::Arrived });
        }

        let mut best: Option<(Move, Stability, f64)> = None;
        for candidate in moves {
            let stability =
                is_stable_intervention(self.reference, plant, &candidate.after)?;
            let descent = stability.before - stability.after;
            // ★★★ Not "ranked last" — not a candidate. A governor offering a
            //     move that makes things worse is offering to make things worse
            //     and calling it an option.
            if descent <= 0.0 {
                continue;
            }
            let better = match &best {
                None => true,
                Some((chosen, _, best_descent)) => {
                    descent > *best_descent
                        // ★★ A tie goes to declared preference, never to
                        //    whichever the vector happened to hold first.
                        || (descent == *best_descent
                            && candidate.preference > chosen.preference)
                }
            };
            if better {
                best = Some((candidate.clone(), stability, descent));
            }
        }

        let (control, runway) = match best {
            None => (
                Control::NoMoveHelps { error: reading.error, considered: moves.len() },
                // ★★★ Nothing helps, so no rate, so no arrival. Reporting a
                //     number here would say "keep going" about a loop that is
                //     not going anywhere.
                Runway::Unbounded,
            ),
            Some((chosen, stability, descent)) => (
                Control::Act { chosen, stability, descent },
                runway(reading.error, descent),
            ),
        };
        Ok(Step { reading, control, runway })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Interval;
    use serde_json::json;

    fn region() -> Region {
        Region::new()
            .bounding(Interval::at_least("balance", 0.0))
            .weighing("balance", 1.0)
    }

    fn at(balance: f64) -> Value {
        json!({"balance": balance})
    }

    #[test]
    fn inside_the_region_it_proposes_nothing() {
        // ★★ `e = 0` means nothing is wrong, and a controller that acts anyway
        //    is the over-corrector damping exists to catch.
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c.step(&at(500.0), &[Move::new("anything", at(600.0))]).expect("steps");
        assert_eq!(step.control, Control::Settled);
        assert!(step.reading.settled());
    }

    #[test]
    fn outside_the_region_it_picks_the_move_that_closes_most_of_the_gap() {
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c
            .step(
                &at(-100.0),
                &[
                    Move::new("small", at(-60.0)),
                    Move::new("big", at(-10.0)),
                    Move::new("nothing much", at(-99.0)),
                ],
            )
            .expect("steps");
        match step.control {
            Control::Act { chosen, descent, .. } => {
                assert_eq!(chosen.name, "big");
                assert_eq!(descent, 90.0);
            }
            other => panic!("expected an action: {other:?}"),
        }
    }

    #[test]
    fn a_move_that_makes_things_worse_is_not_a_candidate_at_all() {
        // ★★★ Lyapunov is the definition of a good decision, not a preference
        //     among options. Offering a worsening move would be offering to
        //     make things worse and calling it a choice.
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c
            .step(&at(-100.0), &[Move::new("worse", at(-200.0))])
            .expect("steps");
        assert!(matches!(step.control, Control::NoMoveHelps { considered: 1, .. }));
    }

    #[test]
    fn nothing_helping_is_an_answer_rather_than_an_error() {
        // ★★★ Reporting it as an error would push a caller toward acting
        //     anyway. Saying "nothing here closes the gap" is what lets a
        //     person go and find something that does.
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c.step(&at(-50.0), &[]).expect("steps");
        match step.control {
            Control::NoMoveHelps { error, considered } => {
                assert_eq!(error, 50.0);
                assert_eq!(considered, 0);
            }
            other => panic!("expected the honest nothing: {other:?}"),
        }
        assert!(step.control.describe().contains("none of them"));
    }

    #[test]
    fn a_tie_goes_to_what_the_household_declared() {
        // ★★ Two moves equally good are equal TO THE LOOP. Picking whichever
        //    the vector held first would make the answer depend on something
        //    nobody chose.
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c
            .step(
                &at(-100.0),
                &[
                    Move::new("first in the list", at(-50.0)),
                    Move::new("what they prefer", at(-50.0)).preferred(10),
                ],
            )
            .expect("steps");
        match step.control {
            Control::Act { chosen, .. } => assert_eq!(chosen.name, "what they prefer"),
            other => panic!("expected an action: {other:?}"),
        }
    }

    #[test]
    fn the_loop_never_applies_anything() {
        // ★★★ §I closes the loop through a HUMAN. A step that acted would close
        //     it through itself, which is the shape supervisory control is
        //     defined in opposition to.
        let before = at(-100.0);
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c.step(&before, &[Move::new("fix", at(0.0))]).expect("steps");
        assert!(step.control.acts(), "it recommends");
        assert_eq!(step.reading.state, before, "and the plant is untouched");
    }

    #[test]
    fn the_reading_travels_beside_the_recommendation() {
        // ★★ A recommendation whose evidence is not beside it is one a person
        //    has to take on trust.
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c.step(&at(-30.0), &[Move::new("fix", at(0.0))]).expect("steps");
        assert_eq!(step.reading.error, 30.0);
        assert!(step.reading.exact);
    }

    #[test]
    fn a_bounded_error_says_so_rather_than_pretending_to_be_exact() {
        // ★★★ `Region::distance` can only BOUND the error when a
        //     cross-dimension relation fails, and a controller acting on a
        //     number it cannot defend is worse than one that admits the bound.
        let region = Region::new()
            .bounding(Interval::at_least("balance", 0.0))
            .weighing("balance", 1.0)
            .relating("balance > other");
        let c = ControlSystem::new(&region);
        let reading = c.sense(&json!({"balance": -10.0, "other": 5.0})).expect("senses");
        assert!(!reading.exact, "a failing relation makes this a lower bound");
    }

    #[test]
    fn the_step_says_how_many_more_turns_it_would_take() {
        // ★★ A projection, not a promise — computed from the descent actually
        //    achieved rather than from a rate somebody declared.
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c.step(&at(-100.0), &[Move::new("quarter", at(-75.0))]).expect("steps");
        assert_eq!(step.runway, Runway::Steps(4));
        assert!(step.runway.describe().contains("4 turns"));
    }

    #[test]
    fn a_loop_that_halves_the_gap_forever_is_reported_as_arriving_not_as_stalling() {
        // ★★ Each turn's projection is honest about THAT turn. The bound is
        //    per-step by construction; a loop whose descent shrinks will keep
        //    reporting a larger runway each turn, which is the visible symptom.
        let r = region();
        let c = ControlSystem::new(&r);
        let first = c.step(&at(-100.0), &[Move::new("half", at(-50.0))]).expect("steps");
        let second = c.step(&at(-50.0), &[Move::new("half", at(-25.0))]).expect("steps");
        assert_eq!(first.runway, Runway::Steps(2));
        assert_eq!(second.runway, Runway::Steps(2), "same shape, same projection");
    }

    #[test]
    fn nothing_helping_has_no_runway_rather_than_a_large_one() {
        // ★★★ A big number would read as patience being enough. "Never, at this
        //     rate" is the actionable fact — it says look for a bigger move.
        let r = region();
        let c = ControlSystem::new(&r);
        let step = c.step(&at(-100.0), &[]).expect("steps");
        assert_eq!(step.runway, Runway::Unbounded);
        assert!(step.runway.describe().contains("ever arrives"));
    }

    #[test]
    fn inside_the_region_the_runway_is_arrival_not_zero_steps() {
        // ★★ "Nought turns away" and "already here" are the same number and
        //    different facts; only one of them is what a reader wants.
        let r = region();
        let c = ControlSystem::new(&r);
        assert_eq!(c.step(&at(10.0), &[]).expect("steps").runway, Runway::Arrived);
    }

    #[test]
    fn the_bound_rounds_up_because_a_partial_turn_is_a_turn() {
        assert_eq!(runway(100.0, 30.0), Runway::Steps(4));
        assert_eq!(runway(100.0, 100.0), Runway::Steps(1));
    }

    #[test]
    fn a_descent_of_nothing_never_arrives() {
        // ★★★ The whole content of the bound: Lyapunov guarantees each step
        //     moves toward the region, never that it gets there.
        assert_eq!(runway(100.0, 0.0), Runway::Unbounded);
        assert_eq!(runway(100.0, -5.0), Runway::Unbounded);
    }

    #[test]
    fn one_turn_is_one_turn_and_it_does_not_iterate() {
        // ★★ The loop runs when something drives it — the Monitor, or a person.
        //    A `step` that looped internally would be a controller with its own
        //    clock, which ADR-0001 keeps out of the core.
        let r = region();
        let c = ControlSystem::new(&r);
        let first = c.step(&at(-100.0), &[Move::new("halve", at(-50.0))]).expect("steps");
        match &first.control {
            Control::Act { chosen, .. } => {
                // Still outside — one turn does not chase it to zero.
                let second = c.step(&chosen.after, &[]).expect("steps");
                assert!(!second.reading.settled());
            }
            other => panic!("expected an action: {other:?}"),
        }
    }
}
