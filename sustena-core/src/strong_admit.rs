//! **The strong form** — `admit` against `Viab_T(V)`, not `V` (Constraint §VIII).
//!
//! ```text
//!   weak:    o(s) ∈ V              — it is fine NOW
//!   strong:  o(s) ∈ Viab_T(V)      — and a legal move still exists, forever
//! ```
//!
//! ★★★ **A monitor enforces exactly safety, never liveness** (Schneider), and
//! that is not a limitation to work around — it is what a monitor *is*. "Stays
//! in `V`" is checkable one transition at a time. "A move always exists" is not,
//! because it is a claim about every future, and no single transition can
//! witness it.
//!
//! ★★★ **So the weak gate admits states with no way out.** A household can be
//! inside its viable region and already lost: every pocket funded, every rule
//! satisfied, and no sequence of legal moves that keeps it there next month. The
//! gate says yes because it is asking about *now*, and by the time the answer
//! changes there is nothing left to do about it. Substituting the kernel moves
//! the refusal to the last moment it can still help.
//!
//! ## Why this is opt-in and not simply better
//!
//! ★★★ **It is expensive, and it depends on `T` being honest.** The kernel is a
//! fixed point over the enumerated reachable space — exponential in dimensions —
//! and every state it keeps is one the declared moves say is survivable. A
//! transition model that overstates what is possible produces a kernel that is
//! too generous, and the gate then admits confidently into a corner. That is a
//! worse failure than the weak gate's, because it comes wearing a guarantee.
//!
//! ★★★ **A HORIZON kernel is never treated as a guarantee.** `Viab^H` is a
//! *superset* of the true kernel: surviving `H` steps is easier than surviving
//! forever, so a state inside it may still be doomed at `H+1`. Admitting on that
//! and calling it strong would be the exact overstatement above. So a horizon
//! answer admits **and says it is provisional** — which is a third outcome, and
//! the honest one.

use std::collections::BTreeSet;

use crate::kernel::Kernel;

/// What the strong gate decided, and how much it is worth.
#[derive(Debug, Clone, PartialEq)]
pub enum StrongVerdict {
    /// In the exact kernel. A legal move exists from here, forever.
    ///
    /// ★★ The only outcome that is a promise, and it is only reachable from
    /// `Kernel::Exact`.
    Survivable,
    /// In a horizon kernel — survivable for `H` steps, and unknown after.
    ///
    /// ★★★ Admitted, and **labelled**. Calling this a guarantee would be
    /// exactly the overstatement that makes a strong gate worse than a weak
    /// one: a confident yes into a corner.
    ProvisionallySurvivable { horizon: usize },
    /// Inside `V` right now, and with no way to stay there.
    ///
    /// ★★★ The case the weak gate cannot see: every rule satisfied and already
    /// lost.
    Doomed { reason: String },
    /// Not even inside `V`. The weak gate refuses this too.
    Outside,
    /// ★★★ No kernel could be computed, so there is nothing stronger to say.
    ///
    /// **Falls back to the weak verdict rather than refusing.** A strong gate
    /// that refused whenever it could not compute would make an expensive
    /// analysis into a denial of service on the household's own money.
    Unknown { why: String },
}

impl StrongVerdict {
    /// May this be committed?
    ///
    /// ★★ `Unknown` admits: the weak gate has already had its say, and this
    /// layer only ever *adds* a refusal it can justify.
    pub fn admits(&self) -> bool {
        !matches!(self, Self::Doomed { .. } | Self::Outside)
    }

    /// Is this a promise, or only evidence?
    pub fn is_guarantee(&self) -> bool {
        matches!(self, Self::Survivable)
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Survivable => "there is always a way on from here".into(),
            Self::ProvisionallySurvivable { horizon } => {
                format!("survivable for {horizon} steps — beyond that, unknown")
            }
            Self::Doomed { reason } => format!("no way on from here: {reason}"),
            Self::Outside => "outside the viable region".into(),
            Self::Unknown { why } => format!("nothing stronger could be said: {why}"),
        }
    }
}

/// **`admit_strong(o, s) ⟺ o(s) ∈ Viab_T(V)`**
///
/// ★★★ `candidate` is the state the operator WOULD produce — this runs on the
/// same copy the weak gate judges, before anything is committed. A strong check
/// after commit would be a diagnosis rather than a gate.
pub fn admit_strong(candidate: &str, kernel: &Kernel, in_region: bool) -> StrongVerdict {
    if !in_region {
        return StrongVerdict::Outside;
    }
    if kernel.is_empty() {
        // ★★★ An empty kernel means NO state survives — which is almost always
        //     a model that is wrong rather than a household that is doomed.
        //     Refusing every move on it would freeze somebody's money over an
        //     analysis nobody checked.
        return StrongVerdict::Unknown {
            why: "the kernel is empty, which says more about the model than the household".into(),
        };
    }
    if !kernel.contains(candidate) {
        return StrongVerdict::Doomed {
            reason: format!(
                "'{candidate}' is inside the region but no legal sequence keeps it there"
            ),
        };
    }
    match kernel {
        Kernel::Exact { .. } => StrongVerdict::Survivable,
        Kernel::Horizon { horizon, .. } => {
            StrongVerdict::ProvisionallySurvivable { horizon: *horizon }
        }
    }
}

/// States the weak gate admits and the strong gate would not.
///
/// ★★★ The whole value of the row, made countable. If this is empty the strong
/// gate is buying nothing on this household and the cost is not worth paying —
/// which is a real answer, and better than assuming the analysis earns its keep.
pub fn doomed_but_viable(region_members: &BTreeSet<String>, kernel: &Kernel) -> Vec<String> {
    region_members.difference(kernel.states()).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::Change;
    use crate::kernel::{viability_kernel, viability_kernel_horizon, Move, Space};
    use crate::region::{Interval, Region};

    fn region() -> Region {
        Region::new().bounding(Interval::at_least("balance", 0.0)).weighing("balance", 1.0)
    }

    fn at(balance: f64) -> serde_json::Value {
        serde_json::json!({ "balance": balance })
    }

    /// A household at four balances. `trap` is INSIDE the region and every legal
    /// move from it lands outside.
    fn space() -> Space {
        [("rich", 500.0), ("ok", 400.0), ("trap", 10.0), ("broke", -90.0)]
            .into_iter()
            .map(|(id, b)| (id.to_string(), at(b)))
            .collect()
    }

    /// Two moves: hold on to what you have, or spend a hundred.
    ///
    /// ★★ `hold` is guarded on having enough to matter, so it is unavailable at
    /// `trap` — which is what makes `trap` a trap rather than a state that can
    /// simply wait.
    fn moves() -> Vec<Move> {
        vec![
            Move::new("hold")
                .guarded_by("balance >= 400")
                .changing("balance", Change::ShiftBy(0.0)),
            Move::new("spend").changing("balance", Change::ShiftBy(-100.0)),
        ]
    }

    fn kernel() -> Kernel {
        viability_kernel(&region(), &space(), &moves()).expect("converges")
    }

    #[test]
    fn a_state_with_a_way_on_forever_is_survivable() {
        assert_eq!(admit_strong("rich", &kernel(), true), StrongVerdict::Survivable);
        assert!(admit_strong("rich", &kernel(), true).is_guarantee());
    }

    #[test]
    fn a_state_inside_the_region_with_no_way_out_is_caught() {
        // ★★★ The case the weak gate cannot see: every rule satisfied and
        //     already lost. `trap` has a positive balance — the weak gate says
        //     yes — and its only legal move leaves the region.
        let k = kernel();
        let v = admit_strong("trap", &k, true);
        assert!(!v.admits());
        assert!(v.describe().contains("no legal sequence keeps it there"));
    }

    #[test]
    fn the_weak_gate_really_would_have_admitted_that_state() {
        // ★★ Worth asserting rather than claiming: if the weak gate refused it
        //    anyway, this whole layer would be buying nothing.
        let r = region();
        let d = r.distance(&at(10.0)).expect("distance");
        assert_eq!(d.weighted, 0.0, "the weak gate is content with `trap`");
    }

    #[test]
    fn a_state_outside_the_region_is_refused_by_the_ordinary_rule() {
        assert_eq!(admit_strong("broke", &kernel(), false), StrongVerdict::Outside);
    }

    #[test]
    fn a_horizon_answer_admits_and_says_it_is_provisional() {
        // ★★★ `Viab^H` is a SUPERSET of the true kernel — surviving H steps is
        //     easier than surviving forever. Admitting on it and calling it
        //     strong would be a confident yes into a corner.
        let h = viability_kernel_horizon(&region(), &space(), &moves(), 2).expect("computes");
        let v = admit_strong("rich", &h, true);
        assert!(v.admits());
        assert!(!v.is_guarantee(), "evidence, not a promise");
        assert!(v.describe().contains("beyond that, unknown"));
    }

    #[test]
    fn an_empty_kernel_says_the_model_is_wrong_rather_than_freezing_the_money() {
        // ★★★ No state surviving is almost always a model that is wrong, not a
        //     household that is doomed. Refusing every move on it would freeze
        //     somebody's money over an analysis nobody checked.
        let empty = Kernel::Exact { states: BTreeSet::new(), passes: 1 };
        let v = admit_strong("rich", &empty, true);
        assert!(v.admits(), "it falls back rather than refusing");
        assert!(v.describe().contains("more about the model"));
    }

    #[test]
    fn unknown_admits_because_this_layer_only_ever_adds_a_refusal_it_can_justify() {
        // ★★ The weak gate has already had its say. A strong gate that refused
        //    whenever it could not compute would make an expensive analysis into
        //    a denial of service.
        let v = StrongVerdict::Unknown { why: "too big to enumerate".into() };
        assert!(v.admits());
        assert!(!v.is_guarantee());
    }

    #[test]
    fn what_the_strong_gate_buys_on_this_household_is_countable() {
        // ★★★ If this is empty the strong gate is buying nothing here and the
        //     cost is not worth paying — a real answer, and better than assuming
        //     the analysis earns its keep.
        let viable: BTreeSet<String> =
            ["rich", "ok", "trap"].into_iter().map(str::to_string).collect();
        let extra = doomed_but_viable(&viable, &kernel());
        assert_eq!(extra, vec!["trap".to_string()]);
    }

    #[test]
    fn a_household_where_the_strong_gate_buys_nothing_says_so() {
        let viable: BTreeSet<String> = ["rich", "ok"].into_iter().map(str::to_string).collect();
        assert!(doomed_but_viable(&viable, &kernel()).is_empty());
    }
}
