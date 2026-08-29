//! **Two prices, and they are different objects** (Arena · §IV).
//!
//! ```text
//!   p_access(a)   once, to acquire it
//!   p_use(a)      per invocation, metered
//! ```
//!
//! ★★★ **One column was doing both jobs, and that is the defect.** The shipped
//! `pawa_cost` is read as *"pawa to install"* by the surfaces and charged as a
//! *per-call royalty* by the ledger. Those are not two readings of one number;
//! they are two economics sharing a field, and whichever one is right the other
//! is silently wrong.
//!
//! ★★★ **They behave differently, which is why they cannot share a field.** An
//! access price is a one-time signal and a one-time transfer — it says *this is
//! worth having* and settles once. A usage price is a metered stream, and it is
//! the one that makes an artefact's cost proportional to the value it actually
//! delivers over time. An artefact that is expensive to acquire and free to run
//! is an ordinary, sensible thing; a single column cannot express it.
//!
//! ★★★ **And this is exactly why [`RevenueType`] is revenue-*typed*.** The
//! schedule that splits a licence sale is not the schedule that splits a pawa
//! charge. With one price there is no way to know which schedule applies, so the
//! two could only ever have been mismatched by convention. Here the occasion
//! chooses both the amount and the schedule together — see [`charge_for`].
//!
//! ## Free is declared, never inferred
//!
//! ★★★ `None` means **nobody has priced this**; `Some(0)` means **somebody
//! decided it is free**. Collapsing them would make an unpriced artefact
//! silently free, which is a decision taken by omission — and the commons is
//! the one place where "free" should be a choice somebody made and can be asked
//! about.

use crate::royalty::RevenueType;

/// What an artefact costs, in the two ways it can cost anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Price {
    /// `p_access` — paid once, to acquire it. `None` = nobody has priced it.
    pub access: Option<u64>,
    /// `p_use` — paid per invocation, on top of the pawa the machine costs.
    pub per_use: Option<u64>,
}

impl Price {
    /// Nothing declared either way.
    ///
    /// ★★ Not "free". An artefact nobody has priced is one somebody still has
    /// to think about.
    pub fn unpriced() -> Self {
        Self::default()
    }

    pub fn to_acquire(mut self, juul: u64) -> Self {
        self.access = Some(juul);
        self
    }

    pub fn to_run(mut self, juul: u64) -> Self {
        self.per_use = Some(juul);
        self
    }

    /// ★★★ Declared free, which is a different claim from unpriced.
    pub fn declared_free() -> Self {
        Self { access: Some(0), per_use: Some(0) }
    }

    /// Has anybody said what this costs, either way?
    pub fn is_priced(&self) -> bool {
        self.access.is_some() || self.per_use.is_some()
    }

    /// Which prices are still unanswered.
    ///
    /// ★★ Askable, because a marketplace listing something with half a price is
    /// showing a number that will surprise somebody at the other half.
    pub fn undeclared(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.access.is_none() {
            out.push("what it costs to acquire");
        }
        if self.per_use.is_none() {
            out.push("what it costs to run");
        }
        out
    }

    pub fn describe(&self) -> String {
        match (self.access, self.per_use) {
            (Some(0), Some(0)) => "free to acquire and free to run".into(),
            (Some(a), Some(0)) => format!("{a} to acquire, then free to run"),
            (Some(0), Some(u)) => format!("free to acquire, {u} per run"),
            (Some(a), Some(u)) => format!("{a} to acquire, then {u} per run"),
            (Some(a), None) => format!("{a} to acquire; nobody has said what it costs to run"),
            (None, Some(u)) => format!("{u} per run; nobody has said what it costs to acquire"),
            (None, None) => "nobody has priced this".into(),
        }
    }
}

/// Why money is changing hands.
///
/// ★★★ The occasion, not the amount. Naming it separately is what stops a
/// licence sale being settled on the usage schedule: the caller says *what
/// happened*, and the amount and the split both follow from that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occasion {
    /// Somebody acquired it.
    Acquiring,
    /// Somebody ran it.
    Running,
}

/// What is owed, and how it splits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owed {
    /// This much, split on this schedule.
    ///
    /// ★★★ The amount and the schedule travel **together**, because they are
    /// both consequences of the same occasion. Handing a caller an amount and
    /// letting it pick a schedule is the mismatch this type exists to prevent.
    Due { amount: u64, revenue: RevenueType },
    /// Declared free for this occasion. Nothing is owed and that was a choice.
    Free,
    /// ★★★ Nobody priced this occasion. **Not free** — the caller must decide
    /// what to do, and pretending zero would be making the decision for them.
    Unpriced { occasion: Occasion },
}

impl Owed {
    pub fn amount(&self) -> Option<u64> {
        match self {
            Self::Due { amount, .. } => Some(*amount),
            Self::Free => Some(0),
            Self::Unpriced { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Due { amount, revenue } => format!(
                "{amount} owed, split on the {} schedule",
                match revenue {
                    RevenueType::Usage => "usage",
                    RevenueType::Access => "access",
                }
            ),
            Self::Free => "declared free for this".into(),
            Self::Unpriced { occasion } => format!(
                "nobody has priced {} — this is not the same as free",
                match occasion {
                    Occasion::Acquiring => "acquiring it",
                    Occasion::Running => "running it",
                }
            ),
        }
    }
}

/// **What this occasion costs, and on which schedule it splits.**
///
/// ★★★ The one function, and the mapping is not a choice a caller gets to make:
/// acquiring settles on `Access`, running settles on `Usage`. That correspondence
/// is the whole reason the schedule is revenue-typed, and leaving it to a caller
/// would put the mismatch back.
pub fn charge_for(price: &Price, occasion: Occasion) -> Owed {
    let (declared, revenue) = match occasion {
        Occasion::Acquiring => (price.access, RevenueType::Access),
        Occasion::Running => (price.per_use, RevenueType::Usage),
    };
    match declared {
        None => Owed::Unpriced { occasion },
        Some(0) => Owed::Free,
        Some(amount) => Owed::Due { amount, revenue },
    }
}

/// **What an artefact costs somebody over its life**, given how often they run it.
///
/// ★★★ The comparison a buyer actually needs and a single column cannot make. A
/// cheap artefact charged per run overtakes an expensive one that is free to
/// run, and *where* it overtakes is a number rather than a feeling. Returns
/// `None` if either half is unpriced, because a lifetime cost computed from a
/// price nobody set is a guess wearing a total.
pub fn lifetime_cost(price: &Price, runs: u64) -> Option<u64> {
    let access = price.access?;
    let per_use = price.per_use?;
    Some(access + per_use.saturating_mul(runs))
}

/// After how many runs does `b` become more expensive than `a`?
///
/// ★★ `None` when they never cross — which is itself the answer, and a more
/// useful one than a number somebody would misread as a threshold.
pub fn crossover(a: &Price, b: &Price) -> Option<u64> {
    let (a_access, a_use) = (a.access?, a.per_use?);
    let (b_access, b_use) = (b.access?, b.per_use?);
    if b_use <= a_use {
        return None;
    }
    let gap = a_access.checked_sub(b_access)?;
    Some(gap / (b_use - a_use) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_column_cannot_express_expensive_to_buy_and_free_to_run() {
        // ★★★ An ordinary, sensible artefact — and the shape a single
        //     `pawa_cost` field is structurally unable to hold.
        let p = Price::unpriced().to_acquire(500).to_run(0);
        assert_eq!(charge_for(&p, Occasion::Acquiring), Owed::Due { amount: 500, revenue: RevenueType::Access });
        assert_eq!(charge_for(&p, Occasion::Running), Owed::Free);
        assert!(p.describe().contains("then free to run"));
    }

    #[test]
    fn the_occasion_chooses_the_schedule_so_a_caller_cannot_mismatch_them() {
        // ★★★ With one price there was no way to know which schedule applied,
        //     so the two could only ever be matched by convention. Acquiring
        //     settles on Access; running settles on Usage; a caller does not get
        //     to decide otherwise.
        let p = Price::unpriced().to_acquire(100).to_run(3);
        match charge_for(&p, Occasion::Acquiring) {
            Owed::Due { revenue, .. } => assert_eq!(revenue, RevenueType::Access),
            other => panic!("{other:?}"),
        }
        match charge_for(&p, Occasion::Running) {
            Owed::Due { revenue, .. } => assert_eq!(revenue, RevenueType::Usage),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unpriced_is_not_free_and_the_type_keeps_them_apart() {
        // ★★★ Collapsing them makes an unpriced artefact silently free — a
        //     decision taken by omission, in the one place where "free" ought to
        //     be a choice somebody made and can be asked about.
        let nobody_said = Price::unpriced();
        let somebody_decided = Price::declared_free();
        assert_eq!(
            charge_for(&nobody_said, Occasion::Running),
            Owed::Unpriced { occasion: Occasion::Running }
        );
        assert_eq!(charge_for(&somebody_decided, Occasion::Running), Owed::Free);
        assert_eq!(charge_for(&nobody_said, Occasion::Running).amount(), None);
        assert_eq!(charge_for(&somebody_decided, Occasion::Running).amount(), Some(0));
    }

    #[test]
    fn an_unpriced_occasion_says_so_rather_than_charging_nothing() {
        let half = Price::unpriced().to_acquire(50);
        let owed = charge_for(&half, Occasion::Running);
        assert!(owed.describe().contains("not the same as free"));
    }

    #[test]
    fn half_a_price_is_reportable_before_somebody_is_surprised_by_the_other_half() {
        // ★★ A marketplace listing something with half a price is showing a
        //    number that will surprise somebody at the other half.
        let half = Price::unpriced().to_acquire(50);
        assert_eq!(half.undeclared(), vec!["what it costs to run"]);
        assert!(Price::declared_free().undeclared().is_empty());
        assert_eq!(Price::unpriced().undeclared().len(), 2);
    }

    #[test]
    fn a_usage_price_makes_cost_proportional_to_value_delivered_over_time() {
        // ★★ The property that motivates having a second price at all: run it
        //    twice and it costs twice, which an access price cannot express.
        let metered = Price::unpriced().to_acquire(0).to_run(10);
        assert_eq!(lifetime_cost(&metered, 0), Some(0));
        assert_eq!(lifetime_cost(&metered, 5), Some(50));
    }

    #[test]
    fn where_a_cheap_metered_artefact_overtakes_an_expensive_free_one_is_a_number() {
        // ★★★ The comparison a buyer actually needs, and one a single column
        //     cannot make. 500 up front and free to run, against nothing up
        //     front and 10 a run: the metered one overtakes after 50.
        let outright = Price::unpriced().to_acquire(500).to_run(0);
        let metered = Price::unpriced().to_acquire(0).to_run(10);
        assert_eq!(crossover(&outright, &metered), Some(51));
        assert_eq!(lifetime_cost(&outright, 51), Some(500));
        assert_eq!(lifetime_cost(&metered, 51), Some(510));
    }

    #[test]
    fn two_prices_that_never_cross_say_so_rather_than_returning_a_threshold() {
        // ★★ "They never cross" is the answer, and a number here would be read
        //    as a threshold that does not exist.
        let cheap = Price::unpriced().to_acquire(10).to_run(1);
        let dearer_both_ways = Price::unpriced().to_acquire(100).to_run(1);
        assert_eq!(crossover(&cheap, &dearer_both_ways), None);
    }

    #[test]
    fn a_lifetime_cost_from_a_price_nobody_set_is_refused() {
        // ★★ A guess wearing a total is worse than no total.
        assert_eq!(lifetime_cost(&Price::unpriced().to_acquire(5), 10), None);
        assert_eq!(lifetime_cost(&Price::unpriced(), 10), None);
    }

    #[test]
    fn nothing_priced_reads_as_nothing_priced() {
        assert!(!Price::unpriced().is_priced());
        assert!(Price::declared_free().is_priced());
        assert_eq!(Price::unpriced().describe(), "nobody has priced this");
    }
}
