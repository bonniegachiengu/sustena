//! **The two sides of a running tab**, read back off the log.
//!
//! ★★★ **Why this is derived and not stored.** A person pocket keeps ONE
//! number — `spent` — and that number is already the answer to "where does the
//! tab stand": positive means he is out of pocket, negative means she has paid
//! in more than he sent. What it cannot say is *how it got there*. KES 1,000
//! outstanding reads identically whether he sent 1,000 once, or sent 40,000
//! across a year and got 39,000 back. Those are different relationships.
//!
//! ★★★ The sides are not a second copy of the truth. `state = fold(events)`,
//! so the log already contains every movement of that pocket; the two sides are
//! a *reading over the same history the state was folded from*. Storing running
//! totals alongside `spent` would be a second source that could disagree with
//! the first, and the day they disagreed there would be no way to say which was
//! right.
//!
//! ★★ It also works backwards. A pocket linked to somebody's number today gets
//! its full history immediately, because the history was never contingent on
//! the link — only on the pocket. Running totals started at the moment of
//! linking would show a tab that began the day it was noticed.
//!
//! ★ No I/O here. The caller holds the log; this reads mutations.

use crate::mutation::Mutation;

/// Which way money went, over the whole life of a tab.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TabSides {
    /// Everything that ever left, toward that person.
    pub sent: f64,
    /// Everything that ever came back from them.
    pub received: f64,
}

impl TabSides {
    /// Where the tab stands: positive means he is owed, negative means he owes.
    ///
    /// ★★ Equal to the pocket's own `spent` by construction, since both are the
    /// same movements summed. A caller comparing the two is checking the fold,
    /// which is a fair thing to want to check.
    pub fn outstanding(self) -> f64 {
        self.sent - self.received
    }
}

/// The dot-path a pocket's spending lives at.
fn spent_path(pocket: &str) -> String {
    format!("finances.pockets.{pocket}.spent")
}

/// Read both sides of one pocket's history out of the mutations that made it.
///
/// ★★★ A `Set` on `spent` carries `old` and `new`, so the DIRECTION is in the
/// record rather than in the operator's name. That matters: reading the
/// operator would mean keeping a list of which operators move a pocket which
/// way, and that list would go stale the first time a new one was added.
/// The delta cannot go stale — it is the movement itself.
///
/// ★★ A movement of exactly zero is ignored rather than counted as either. It
/// is not a side; it is nothing happening.
pub fn tab_sides(pocket: &str, mutations: &[Mutation]) -> TabSides {
    let path = spent_path(pocket);
    let mut sides = TabSides::default();
    for m in mutations {
        let Mutation::Set { path: p, old, new } = m else { continue };
        if *p != path {
            continue;
        }
        // ★ An absent `old` is a first write, which starts from nothing.
        let before = old.as_f64().unwrap_or(0.0);
        let after = new.as_f64().unwrap_or(0.0);
        let delta = after - before;
        if delta > 0.0 {
            sides.sent += delta;
        } else if delta < 0.0 {
            sides.received += -delta;
        }
    }
    sides
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn set(pocket: &str, old: f64, new: f64) -> Mutation {
        Mutation::Set {
            path: spent_path(pocket),
            old: json!(old),
            new: json!(new),
        }
    }

    #[test]
    fn a_tab_with_no_history_has_no_sides() {
        assert_eq!(tab_sides("Aida", &[]), TabSides::default());
    }

    #[test]
    fn money_out_and_money_back_are_counted_separately() {
        // ★★★ The whole point. One net number cannot tell 1,000 sent once from
        //     40,000 sent and 39,000 returned, and those are not the same
        //     relationship.
        let log = [set("Aida", 0.0, 40_000.0), set("Aida", 40_000.0, 1_000.0)];
        let sides = tab_sides("Aida", &log);
        assert_eq!(sides.sent, 40_000.0);
        assert_eq!(sides.received, 39_000.0);
        assert_eq!(sides.outstanding(), 1_000.0);
    }

    #[test]
    fn a_tab_in_her_favour_reads_as_money_he_owes() {
        // She sent first: nothing has gone out, and the tab is against him.
        let sides = tab_sides("Aida", &[set("Aida", 0.0, -1_000.0)]);
        assert_eq!(sides.sent, 0.0);
        assert_eq!(sides.received, 1_000.0);
        assert_eq!(sides.outstanding(), -1_000.0);
    }

    #[test]
    fn one_persons_history_never_borrows_anothers() {
        let log = [set("Aida", 0.0, 500.0), set("food", 0.0, 900.0)];
        assert_eq!(tab_sides("Aida", &log).sent, 500.0);
        assert_eq!(tab_sides("food", &log).sent, 900.0);
    }

    #[test]
    fn a_pockets_other_fields_are_not_movements_of_money_out() {
        // ★★ Allocating into the envelope is not sending anything to anyone.
        let log = [Mutation::Set {
            path: "finances.pockets.Aida.allocated".into(),
            old: json!(0.0),
            new: json!(3_000.0),
        }];
        assert_eq!(tab_sides("Aida", &log), TabSides::default());
    }

    #[test]
    fn a_movement_of_nothing_is_not_a_side() {
        assert_eq!(tab_sides("Aida", &[set("Aida", 500.0, 500.0)]), TabSides::default());
    }

    #[test]
    fn the_sides_agree_with_the_number_the_fold_produced() {
        // ★★★ Both are the same movements summed, so a disagreement would mean
        //     one of them had stopped reading the real history.
        let log = [
            set("Aida", 0.0, 2_000.0),
            set("Aida", 2_000.0, 1_500.0),
            set("Aida", 1_500.0, 4_500.0),
        ];
        assert_eq!(tab_sides("Aida", &log).outstanding(), 4_500.0);
    }
}
