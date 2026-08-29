//! **Positioning — how much room an act leaves you** (Operative · §X).
//!
//! ```text
//!   Reach_H(s)  = states reachable from s within H legal moves
//!   ΔReach_H    = μ(Reach_H(after)) − μ(Reach_H(before))
//! ```
//!
//! ★★★ **An act can be right on today's numbers and wrong on tomorrow's
//! options.** Clearing a pocket to the shilling and clearing a loan can move a
//! balance identically and leave completely different rooms to move in. Utility
//! alone cannot see that difference; `ΔReach` is the term that can, and it is
//! why a beaver builds a dam.
//!
//! ## μ is declared, and never raw cardinality
//!
//! ★★★ **Counting reachable states is not measuring freedom.** A discretisation
//! that splits one situation into ten near-identical states multiplies the count
//! by ten and changes nothing about the household. Worse, it is *invisible*: the
//! number looks like a measurement and is an artefact of how somebody chose to
//! enumerate. So [`Measure`] carries a declared weight per state, and counting
//! is available only as [`Measure::counting`] — spelled out, so a reader can see
//! that it was chosen rather than defaulted into.
//!
//! ## Horizon-limited is an UNDER-estimate, and the sign is carried
//!
//! ★★★ `Reach_H ⊆ Reach_∞`, so a bounded `ΔReach` under-states the value of
//! positioning — and it under-states it **worst for exactly the acts that pay
//! off latest**. The bias is systematic and in one direction, which makes it
//! correctable by a reader who is told about it and invisible to one who is not.
//! [`Positioning::bias`] says so on every answer rather than in a comment here.
//!
//! ## In a chaotic regime the bound is `H*`, not the budget
//!
//! ★★★ Raising `H` past [`crate::horizon`]'s `H*` buys nothing but noise — the
//! extra states are reachable in the arithmetic and not in the world. So the
//! effective horizon is `min(budget, H*)`, and [`Reach`] reports **which bound
//! bit**: being stopped by compute is a reason to buy more, and being stopped by
//! chaos is a reason not to.

use std::collections::{BTreeMap, BTreeSet};

use crate::horizon::Horizon;
use crate::kernel::{KernelError, Move, Space};

/// **`μ`** — a declared measure over states.
///
/// ★★★ Not a count. Two states that are nearly the same situation should not
/// contribute twice as much room as one, and only a declaration can say which
/// states those are.
#[derive(Debug, Clone, PartialEq)]
pub struct Measure {
    weights: BTreeMap<String, f64>,
    /// What an unlisted state weighs.
    ///
    /// ★★ Zero rather than one. An undeclared state contributing to a total is
    /// a measurement made by omission, and a measure whose value depends on
    /// which states somebody remembered to leave out is not a measure.
    default: f64,
}

impl Measure {
    pub fn new() -> Self {
        Self { weights: BTreeMap::new(), default: 0.0 }
    }

    pub fn weighing(mut self, state: &str, w: f64) -> Self {
        self.weights.insert(state.to_string(), w);
        self
    }

    /// ★★★ Raw cardinality, available only by name.
    ///
    /// It is sometimes the honest measure — a space of genuinely distinct
    /// situations. It is never the honest *default*, because then a
    /// discretisation choice silently becomes a claim about freedom.
    pub fn counting() -> Self {
        Self { weights: BTreeMap::new(), default: 1.0 }
    }

    pub fn of(&self, states: &BTreeSet<String>) -> f64 {
        states.iter().map(|s| self.weights.get(s).copied().unwrap_or(self.default)).sum()
    }
}

impl Default for Measure {
    fn default() -> Self {
        Self::new()
    }
}

/// Why the reach computation stopped where it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoppedBy {
    /// The caller's step budget ran out. **More compute would see further.**
    Budget,
    /// ★★★ `H*`. More compute would see further into the *arithmetic* and no
    /// further into the world.
    Chaos,
    /// The reachable set stopped growing before either bound.
    Closure,
    /// ★★★ No horizon could be estimated, so the budget was honoured and the
    /// answer is **labelled** rather than quietly trusted.
    UnknownHorizon,
}

impl StoppedBy {
    /// Would spending more compute buy a better answer?
    pub fn more_compute_helps(&self) -> bool {
        matches!(self, Self::Budget)
    }
}

/// `Reach_H(s)`, and how far it actually got.
#[derive(Debug, Clone, PartialEq)]
pub struct Reach {
    pub states: BTreeSet<String>,
    pub steps_taken: usize,
    pub stopped_by: StoppedBy,
}

/// The effective horizon, and which bound bit.
///
/// ★★ Separate and public because it answers a question on its own: *is it
/// worth raising the budget?* A caller should be able to ask that without
/// running the reach.
pub fn effective_horizon(budget: usize, horizon: &Horizon) -> (usize, StoppedBy) {
    match horizon {
        Horizon::Steps { steps, .. } if *steps < budget => (*steps, StoppedBy::Chaos),
        Horizon::Steps { .. } | Horizon::Unbounded { .. } => (budget, StoppedBy::Budget),
        Horizon::Unknown { .. } => (budget, StoppedBy::UnknownHorizon),
    }
}

/// **`Reach_H(s)`** — everything reachable within the effective horizon.
///
/// ★★ Includes `from` itself: staying put is a thing you can do, and a reach
/// that excluded it would rate an act that removes every option as equal to one
/// that leaves you where you are.
pub fn reach(
    space: &Space,
    moves: &[Move],
    from: &str,
    budget: usize,
    horizon: &Horizon,
) -> Result<Reach, KernelError> {
    let (limit, bound) = effective_horizon(budget, horizon);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    seen.insert(from.to_string());
    let mut frontier: Vec<String> = vec![from.to_string()];
    let mut steps = 0;

    while steps < limit && !frontier.is_empty() {
        let mut next = Vec::new();
        for id in &frontier {
            let Some(s) = space.get(id) else { continue };
            for m in moves {
                if let Some(landed) = m.lands_on(space, s)? {
                    if seen.insert(landed.clone()) {
                        next.push(landed);
                    }
                }
            }
        }
        steps += 1;
        frontier = next;
    }
    let stopped_by = if frontier.is_empty() { StoppedBy::Closure } else { bound };
    Ok(Reach { states: seen, steps_taken: steps, stopped_by })
}

/// Which direction a bounded answer is wrong in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    /// ★★★ `Reach_H ⊆ Reach_∞`, so this **under**-states positioning value, and
    /// worst for the acts that pay off latest.
    UnderStatesPositioning,
    /// The reachable set closed before the horizon, so nothing was cut off.
    None,
}

/// **`ΔReach_H`** — how much more room the candidate leaves than the current state.
#[derive(Debug, Clone, PartialEq)]
pub struct Positioning {
    pub delta: f64,
    pub before: f64,
    pub after: f64,
    pub steps_taken: usize,
    pub stopped_by: StoppedBy,
    pub bias: Bias,
}

impl Positioning {
    /// Does the candidate leave more room than staying put?
    pub fn improves(&self) -> bool {
        self.delta > 0.0
    }

    pub fn describe(&self) -> String {
        let room = if self.delta > 0.0 {
            "leaves more room"
        } else if self.delta < 0.0 {
            "closes options off"
        } else {
            "leaves the same room"
        };
        let caveat = match self.bias {
            Bias::UnderStatesPositioning => format!(
                " — measured to {} steps, so this is a floor and not a figure",
                self.steps_taken
            ),
            Bias::None => String::new(),
        };
        format!("{room} ({:+.2}){caveat}", self.delta)
    }
}

/// **`ΔReach_H(before → after)`** under a declared `μ`.
pub fn delta_reach(
    space: &Space,
    moves: &[Move],
    before: &str,
    after: &str,
    budget: usize,
    horizon: &Horizon,
    mu: &Measure,
) -> Result<Positioning, KernelError> {
    let a = reach(space, moves, before, budget, horizon)?;
    let b = reach(space, moves, after, budget, horizon)?;
    let (before_m, after_m) = (mu.of(&a.states), mu.of(&b.states));
    // ★★ The bias is only absent if BOTH sides closed. One side still growing
    //    means that side was cut off, and a difference with one truncated term
    //    is still a truncated difference.
    let bias = if a.stopped_by == StoppedBy::Closure && b.stopped_by == StoppedBy::Closure {
        Bias::None
    } else {
        Bias::UnderStatesPositioning
    };
    let stopped_by = if a.stopped_by == StoppedBy::Closure { b.stopped_by } else { a.stopped_by };
    Ok(Positioning {
        delta: after_m - before_m,
        before: before_m,
        after: after_m,
        steps_taken: a.steps_taken.max(b.steps_taken),
        stopped_by,
        bias,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::Change;
    use serde_json::json;

    fn at(b: f64) -> serde_json::Value {
        json!({ "balance": b })
    }

    /// A ladder of balances, plus a dead end nothing leaves.
    fn space() -> Space {
        [("rich", 300.0), ("ok", 200.0), ("thin", 100.0), ("stuck", 0.0)]
            .into_iter()
            .map(|(id, b)| (id.to_string(), at(b)))
            .collect()
    }

    /// Spend a hundred, or earn a hundred — but only while there is something
    /// left, so `stuck` genuinely has nowhere to go.
    fn moves() -> Vec<Move> {
        vec![
            Move::new("spend")
                .guarded_by("balance >= 100")
                .changing("balance", Change::ShiftBy(-100.0)),
            Move::new("earn")
                .guarded_by("balance >= 100")
                .changing("balance", Change::ShiftBy(100.0)),
        ]
    }

    fn unbounded() -> Horizon {
        Horizon::Unbounded { lambda: -0.1 }
    }

    #[test]
    fn a_measure_is_declared_and_an_undeclared_state_weighs_nothing() {
        // ★★ A measure whose value depends on which states somebody remembered
        //    to leave out is not a measure.
        let mu = Measure::new().weighing("rich", 2.0).weighing("ok", 1.0);
        let s: BTreeSet<String> =
            ["rich", "ok", "never-declared"].into_iter().map(str::to_string).collect();
        assert_eq!(mu.of(&s), 3.0);
    }

    #[test]
    fn counting_is_available_but_only_by_name() {
        // ★★★ Sometimes honest, never the honest default — otherwise a
        //     discretisation choice silently becomes a claim about freedom.
        let s: BTreeSet<String> = ["a", "b", "c"].into_iter().map(str::to_string).collect();
        assert_eq!(Measure::counting().of(&s), 3.0);
        assert_eq!(Measure::new().of(&s), 0.0);
    }

    #[test]
    fn splitting_one_situation_into_ten_does_not_make_a_household_ten_times_freer() {
        // ★★★ The reason μ exists. Under counting the refined space scores ten
        //     times higher for a household in exactly the same position.
        let one: BTreeSet<String> = ["ok"].into_iter().map(str::to_string).collect();
        let split: BTreeSet<String> =
            (0..10).map(|i| format!("ok-{i}")).collect();
        assert_eq!(Measure::counting().of(&one), 1.0);
        assert_eq!(Measure::counting().of(&split), 10.0, "the artefact, visible");

        let mut declared = Measure::new().weighing("ok", 1.0);
        for i in 0..10 {
            declared = declared.weighing(&format!("ok-{i}"), 0.1);
        }
        assert!(
            (declared.of(&one) - declared.of(&split)).abs() < 1e-9,
            "the same room either way"
        );
    }

    #[test]
    fn staying_put_is_something_you_can_do() {
        // ★★ A reach excluding its own start would rate an act that removes
        //    every option as equal to one that leaves you where you are.
        let r = reach(&space(), &moves(), "stuck", 5, &unbounded()).expect("reaches");
        assert_eq!(r.states.len(), 1);
        assert_eq!(r.stopped_by, StoppedBy::Closure);
    }

    #[test]
    fn an_act_that_closes_options_off_scores_negative() {
        // ★★★ The whole term. `stuck` and `ok` may sit at the same utility and
        //     they are not the same position.
        let p = delta_reach(&space(), &moves(), "ok", "stuck", 5, &unbounded(), &Measure::counting())
            .expect("computes");
        assert!(!p.improves());
        assert!(p.describe().contains("closes options off"));
    }

    #[test]
    fn a_bounded_answer_says_it_is_a_floor() {
        // ★★★ Reach_H ⊆ Reach_∞. The bias is systematic and one-directional,
        //     which makes it correctable by a reader who is told and invisible
        //     to one who is not.
        let tight = delta_reach(
            &space(),
            &moves(),
            "thin",
            "rich",
            1,
            &unbounded(),
            &Measure::counting(),
        )
        .expect("computes");
        assert_eq!(tight.bias, Bias::UnderStatesPositioning);
        assert!(tight.describe().contains("a floor and not a figure"));
    }

    #[test]
    fn a_reach_that_closed_carries_no_caveat() {
        let p = delta_reach(&space(), &moves(), "stuck", "stuck", 5, &unbounded(), &Measure::counting())
            .expect("computes");
        assert_eq!(p.bias, Bias::None);
        assert!(!p.describe().contains("floor"));
    }

    #[test]
    fn one_side_still_growing_still_truncates_the_difference() {
        // ★★ A difference with one truncated term is a truncated difference.
        //    `stuck` closes immediately and `rich` does not.
        let p =
            delta_reach(&space(), &moves(), "stuck", "rich", 1, &unbounded(), &Measure::counting())
                .expect("computes");
        assert_eq!(p.bias, Bias::UnderStatesPositioning);
    }

    #[test]
    fn in_a_chaotic_regime_the_bound_is_the_horizon_and_not_the_budget() {
        // ★★★ Raising H past H* buys states that are reachable in the
        //     arithmetic and not in the world.
        let chaotic = Horizon::Steps { steps: 2, lambda: 0.7 };
        let (limit, why) = effective_horizon(50, &chaotic);
        assert_eq!(limit, 2);
        assert_eq!(why, StoppedBy::Chaos);
        assert!(!why.more_compute_helps(), "buying more compute would buy noise");
    }

    #[test]
    fn a_budget_below_the_horizon_is_a_reason_to_buy_more_compute() {
        let roomy = Horizon::Steps { steps: 40, lambda: 0.01 };
        let (limit, why) = effective_horizon(3, &roomy);
        assert_eq!(limit, 3);
        assert_eq!(why, StoppedBy::Budget);
        assert!(why.more_compute_helps());
    }

    #[test]
    fn an_unknown_horizon_honours_the_budget_and_is_labelled() {
        // ★★★ Not silently trusted as unbounded. The answer is produced and it
        //     says what it does not know.
        let (limit, why) = effective_horizon(4, &Horizon::Unknown { why: "no data".into() });
        assert_eq!(limit, 4);
        assert_eq!(why, StoppedBy::UnknownHorizon);
        assert!(!why.more_compute_helps());
    }

    #[test]
    fn the_reach_really_stops_where_the_horizon_says() {
        let chaotic = Horizon::Steps { steps: 1, lambda: 0.7 };
        let near = reach(&space(), &moves(), "thin", 50, &chaotic).expect("reaches");
        assert_eq!(near.steps_taken, 1);
        assert_eq!(near.stopped_by, StoppedBy::Chaos);
        let far = reach(&space(), &moves(), "thin", 50, &unbounded()).expect("reaches");
        assert!(far.states.len() > near.states.len());
    }
}
