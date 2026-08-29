//! **What it costs to look** (Operative · §V).
//!
//! ```text
//!   pawa(a) = κ_a · b · d · ρ  ≤  B_att
//!             breadth · depth · resolution
//! ```
//!
//! ★★★ **The system already had the HOLD without the priced SCAN** — the
//! knapsack that fits about four things into a person's working memory was
//! built, and nothing priced the looking that fills it. That is the inverse of
//! the usual gap: normally a system measures eagerly and cannot decide what to
//! show; here it decided well and never charged for the search.
//!
//! ★★★ **Iso-cost is the property worth having.** Deep-and-narrow and
//! broad-and-shallow are the *same spend*, because the product is what is
//! bounded. So the budget expresses no preference about the *shape* of a search,
//! only its total — which is right, since whether to look widely or closely is a
//! judgement about the question, not about the money.
//!
//! ★★ **`κ_a` is declared and uncalibrated, and says so** — exactly like the
//! meter's own `κ_c` and `κ_s`. A coefficient presented as measured when nobody
//! measured it is the failure this whole layer exists to avoid.

/// One search over the `⊕` tree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scan {
    /// `b` — how many siblings are examined at each level.
    pub breadth: u32,
    /// `d` — how far down the composition tree it goes.
    pub depth: u32,
    /// `ρ` — how finely each thing is sampled.
    pub resolution: u32,
}

impl Scan {
    pub fn new(breadth: u32, depth: u32, resolution: u32) -> Self {
        Self { breadth, depth, resolution }
    }

    /// `b · d · ρ` — the raw extent, before any price.
    pub fn extent(&self) -> u64 {
        self.breadth as u64 * self.depth as u64 * self.resolution as u64
    }

    /// **`pawa(a) = κ_a · b · d · ρ`**
    pub fn pawa(&self, kappa_a: f64) -> f64 {
        kappa_a * self.extent() as f64
    }
}

/// The attention coefficient.
///
/// ★★★ **Declared, and not calibrated against anything.** It converts an extent
/// into pawa, and the number is a design choice waiting on real data — the same
/// honest position `κ_c` and `κ_s` hold. Naming it here rather than burying it
/// in a formula is what keeps it from being mistaken for a measurement.
pub const KAPPA_A: f64 = 1.0;

/// ★★ Whether anybody has calibrated it yet. A function so the answer is
/// executable rather than a comment somebody may not read.
pub fn kappa_is_calibrated() -> bool {
    false
}

/// What a scan was refused for, if it was.
#[derive(Debug, Clone, PartialEq)]
pub enum Budgeted {
    /// It fits.
    Affordable { pawa: f64, left: f64 },
    /// ★★★ It does not, and the report says by how much — so a caller can
    /// narrow, shallow or coarsen rather than being told only "no".
    TooExpensive { pawa: f64, budget: f64, over: f64 },
}

impl Budgeted {
    pub fn affordable(&self) -> bool {
        matches!(self, Self::Affordable { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Affordable { pawa, left } => {
                format!("costs {pawa:.1} of the attention budget, leaving {left:.1}")
            }
            Self::TooExpensive { pawa, budget, over } => format!(
                "costs {pawa:.1} against a budget of {budget:.1} — {over:.1} over, so look at \
                 fewer things, or less deeply, or less finely"
            ),
        }
    }
}

/// **Does this search fit the attention budget?**
pub fn afford(scan: &Scan, kappa_a: f64, budget: f64) -> Budgeted {
    let pawa = scan.pawa(kappa_a);
    if pawa <= budget {
        Budgeted::Affordable { pawa, left: budget - pawa }
    } else {
        Budgeted::TooExpensive { pawa, budget, over: pawa - budget }
    }
}

/// Do two searches cost the same?
///
/// ★★★ The iso-cost check. Deep-and-narrow and broad-and-shallow are the same
/// spend, so a budget constrains *how much* looking happens and says nothing
/// about its shape — which is right, because whether to look widely or closely
/// is a judgement about the question rather than about the money.
pub fn iso_cost(a: &Scan, b: &Scan) -> bool {
    a.extent() == b.extent()
}

/// The widest search affordable at a given depth and resolution.
///
/// ★★ Useful in the direction a caller actually thinks: not *what does this
/// cost* but *how much can I look at*. `None` when depth or resolution is zero,
/// because a search of nothing is not a wide search.
pub fn widest_affordable(budget: f64, kappa_a: f64, depth: u32, resolution: u32) -> Option<u32> {
    let per_sibling = kappa_a * depth as f64 * resolution as f64;
    (per_sibling > 0.0).then(|| (budget / per_sibling).floor().max(0.0) as u32)
}

/// The two halves of attention, kept distinct.
///
/// ★★★ **Scan is what it costs to look; hold is what fits once you have
/// looked.** They are different scarcities: the first is compute over the tree,
/// the second is a person's working memory. A design that priced only one would
/// either search forever and show four things, or search cheaply and try to show
/// forty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttentionCost {
    /// Priced here, in pawa.
    Scan,
    /// Bounded by the knapsack, in chunks — about four.
    Hold,
}

impl AttentionCost {
    pub fn what_it_is_scarce_in(&self) -> &'static str {
        match self {
            Self::Scan => "pawa — compute spent looking over the composition tree",
            Self::Hold => "chunks — a person's working memory, about four of them",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_and_narrow_costs_the_same_as_broad_and_shallow() {
        // ★★★ Iso-cost. The budget constrains how much looking happens and says
        //     nothing about its shape — which is right, because whether to look
        //     widely or closely is a judgement about the question rather than
        //     about the money.
        let deep = Scan::new(2, 12, 1);
        let broad = Scan::new(12, 2, 1);
        assert!(iso_cost(&deep, &broad));
        assert_eq!(deep.pawa(KAPPA_A), broad.pawa(KAPPA_A));
    }

    #[test]
    fn a_finer_resolution_costs_more_at_the_same_shape() {
        // ★★ Resolution is a real term, not decoration: sampling each thing
        //    four times over is four times the looking.
        let coarse = Scan::new(4, 3, 1);
        let fine = Scan::new(4, 3, 4);
        assert_eq!(fine.extent(), coarse.extent() * 4);
    }

    #[test]
    fn a_scan_that_does_not_fit_says_by_how_much_and_what_to_change() {
        // ★★★ So a caller can narrow, shallow or coarsen — being told only
        //     "no" leaves them guessing which of the three to give up.
        let b = afford(&Scan::new(20, 10, 5), KAPPA_A, 100.0);
        assert!(!b.affordable());
        assert!(b.describe().contains("fewer things, or less deeply, or less finely"));
    }

    #[test]
    fn an_affordable_scan_says_what_is_left() {
        // ★★ A budget report that only says "yes" cannot tell a caller whether
        //    it has room for a second look.
        match afford(&Scan::new(2, 2, 2), KAPPA_A, 100.0) {
            Budgeted::Affordable { pawa, left } => {
                assert_eq!(pawa, 8.0);
                assert_eq!(left, 92.0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_coefficient_says_it_is_uncalibrated() {
        // ★★★ A coefficient presented as measured when nobody measured it is
        //     the failure this whole layer exists to avoid — so the answer is
        //     executable rather than a comment somebody may not read.
        assert!(!kappa_is_calibrated());
    }

    #[test]
    fn how_much_can_i_look_at_is_askable_in_the_direction_a_caller_thinks() {
        // ★★ Not "what does this cost" but "how wide can I go" — which is the
        //    question somebody planning a search actually has.
        assert_eq!(widest_affordable(100.0, KAPPA_A, 5, 2), Some(10));
        assert_eq!(widest_affordable(100.0, KAPPA_A, 0, 2), None, "a search of nothing");
    }

    #[test]
    fn scan_and_hold_are_scarce_in_different_things() {
        // ★★★ A design that priced only one would either search forever and
        //     show four things, or search cheaply and try to show forty.
        assert!(AttentionCost::Scan.what_it_is_scarce_in().contains("pawa"));
        assert!(AttentionCost::Hold.what_it_is_scarce_in().contains("working memory"));
        assert_ne!(
            AttentionCost::Scan.what_it_is_scarce_in(),
            AttentionCost::Hold.what_it_is_scarce_in()
        );
    }

    #[test]
    fn a_scan_of_nothing_costs_nothing() {
        assert_eq!(Scan::new(0, 5, 5).extent(), 0);
        assert!(afford(&Scan::new(0, 5, 5), KAPPA_A, 0.0).affordable());
    }

    #[test]
    fn the_budget_binds_the_product_rather_than_any_one_term() {
        // ★★★ Which is what iso-cost means in practice: doubling the depth and
        //     halving the breadth changes nothing about affordability.
        let a = Scan::new(8, 4, 1);
        let b = Scan::new(4, 8, 1);
        assert_eq!(afford(&a, KAPPA_A, 32.0).affordable(), afford(&b, KAPPA_A, 32.0).affordable());
    }
}
