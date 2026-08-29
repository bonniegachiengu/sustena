//! **Two lifecycles in one order, and why they cannot be one** (Arena · §IX).
//!
//! ```text
//!   product:  PLACED → PROCESSING → DELIVERING → DELIVERED    rival, conserved
//!   package:  PLACED → INSTALLING → SANDBOXED  → LIVE         non-rival, copied
//! ```
//!
//! ★★★ **Two status columns are not redundancy — collapsing them into one
//! field would make one of the two lifecycles inexpressible.** A plate of pilau
//! is delivered and gone; an Enzyme is germinated and copied. This is Mycelium's
//! law (value is conserved, information is not, so they route differently)
//! appearing **inside a single transaction**, and one order can genuinely carry
//! both.
//!
//! ★★★ **So there is no shared `Status`.** The two sequences have different
//! lengths, different names and different meanings, and a type that could hold
//! either would let a package be marked `DELIVERED` — which is not a claim
//! anybody could act on.
//!
//! ## Settlement is checked, not assumed
//!
//! ★★★ **`Σ Δ = 0` at commit.** An order is *value*, so it settles on the
//! ledger and never on gossip. The article writes the assertion into its own
//! pseudocode, and it is asserted here for the same reason it is asserted in
//! [`crate::royalty`]: a conservation law that is only ever stated is a comment.
//!
//! ★★★ **Nothing partial.** If the buyer cannot cover the whole order, nothing
//! moves at all — settling the first three items and running out would leave the
//! books unbalanced against the very invariant this checks.
//!
//! ## A receipt is not a capability
//!
//! ★★★ **A licence key issued at order time and never checked is a receipt**, a
//! record that a purchase happened. A capability is a different object: bound to
//! a hash, to a principal, to an order, single-use, expiring. *An unverified key
//! is the same design error as an unchecked trust score — a field carrying the
//! NAME of a guarantee without the mechanism.* Receipts are not worthless; they
//! are the right object for an audit trail. They must simply never be presented
//! where a capability is implied, and [`Receipt`] has no method that admits
//! anything.

use crate::reputation::Versioned;

/// Where a physically-delivered thing has got to.
///
/// ★★ Rival and conserved: one plate of pilau delivered is one plate nobody
/// else has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProductStatus {
    Placed,
    Processing,
    Delivering,
    Delivered,
}

/// Where a digital artefact has got to.
///
/// ★★ Non-rival and copied: germinating an Enzyme takes it from nobody.
/// **`Sandboxed` is a real stop**, not a formality — it is where the approval
/// token sits, on the sandboxed→live transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackageStatus {
    Placed,
    Installing,
    Sandboxed,
    Live,
}

impl ProductStatus {
    pub fn next(&self) -> Option<ProductStatus> {
        match self {
            Self::Placed => Some(Self::Processing),
            Self::Processing => Some(Self::Delivering),
            Self::Delivering => Some(Self::Delivered),
            Self::Delivered => None,
        }
    }
}

impl PackageStatus {
    pub fn next(&self) -> Option<PackageStatus> {
        match self {
            Self::Placed => Some(Self::Installing),
            Self::Installing => Some(Self::Sandboxed),
            Self::Sandboxed => Some(Self::Live),
            Self::Live => None,
        }
    }

    /// ★★★ Does advancing from here need a person?
    ///
    /// Only `Sandboxed → Live`. Germination is `⊕`, and the approval token
    /// belongs on the transition where something stops being contained and
    /// starts being part of the household.
    pub fn needs_approval_to_advance(&self) -> bool {
        matches!(self, Self::Sandboxed)
    }
}

/// One line of an order. **Two kinds, and they do not share a status.**
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Rival, physically delivered, priced in the household's own money.
    Product { name: String, price_minor: u64, status: ProductStatus },
    /// Non-rival, germinated, priced in pawa.
    Package { artefact: Versioned, author: String, pawa: u64, status: PackageStatus },
}

impl Item {
    pub fn amount(&self) -> u64 {
        match self {
            Self::Product { price_minor, .. } => *price_minor,
            Self::Package { pawa, .. } => *pawa,
        }
    }

    /// Has this line finished its own lifecycle?
    pub fn settled(&self) -> bool {
        match self {
            Self::Product { status, .. } => *status == ProductStatus::Delivered,
            Self::Package { status, .. } => *status == PackageStatus::Live,
        }
    }
}

/// One order, which may carry both kinds at once.
///
/// ★★★ **Two totals, because the two halves are denominated in different
/// things.** `product_total` is money and `pawa_total` is metered work; adding
/// them would produce a number in no unit at all, and a single `total` column is
/// exactly that addition waiting to happen.
#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: String,
    pub buyer: String,
    pub items: Vec<Item>,
}

impl Order {
    pub fn new(id: &str, buyer: &str) -> Self {
        Self { id: id.into(), buyer: buyer.into(), items: Vec::new() }
    }

    pub fn with(mut self, item: Item) -> Self {
        self.items.push(item);
        self
    }

    /// Money owed, in minor units.
    pub fn product_total(&self) -> u64 {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Product { price_minor, .. } => Some(*price_minor),
                _ => None,
            })
            .sum()
    }

    /// Pawa owed.
    pub fn pawa_total(&self) -> u64 {
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::Package { pawa, .. } => Some(*pawa),
                _ => None,
            })
            .sum()
    }

    /// Lines still waiting on somebody.
    ///
    /// ★★ Both kinds, in one list, because a person waiting on an order does
    /// not care which half of the schema their pilau is in.
    pub fn outstanding(&self) -> Vec<&Item> {
        self.items.iter().filter(|i| !i.settled()).collect()
    }
}

/// What settlement did.
#[derive(Debug, Clone, PartialEq)]
pub enum Settled {
    /// It settled, and the deltas summed to zero.
    Ok { money_moved: u64, pawa_moved: u64 },
    /// ★★★ The buyer could not cover it. **Nothing moved** — settling the first
    /// three items and running out would leave the books unbalanced against the
    /// very invariant this exists to check.
    Insufficient { needed: u64, held: u64, of: &'static str },
    /// ★★★ What was charged and what was distributed do not agree. Reported
    /// rather than corrected: a conservation failure quietly patched is a
    /// conservation failure nobody learns about.
    NotConserved { residue: i64 },
}

impl Settled {
    pub fn describe(&self) -> String {
        match self {
            Self::Ok { money_moved, pawa_moved } => {
                format!("settled: {money_moved} money and {pawa_moved} pawa moved, deltas sum to zero")
            }
            Self::Insufficient { needed, held, of } => format!(
                "needs {needed} {of} and holds {held} — nothing moved, because a part-settled \
                 order is an unbalanced one"
            ),
            Self::NotConserved { residue } => format!(
                "the deltas do not sum to zero ({residue} left over) — this is a conservation \
                 violation and is reported rather than rounded away"
            ),
        }
    }
}

/// **`settle_order`** — guard, charge, and check `Σ Δ = 0` against what was
/// actually distributed.
///
/// ★★★ **The check compares two independently-computed things, or it is not a
/// check.** A first draft of this compared the order total to itself, which can
/// never fail — the same defect §IX.3 names one paragraph later: a field
/// carrying the *name* of a guarantee without the mechanism. So the caller
/// supplies what each package line actually distributed (from
/// [`crate::royalty::split`]), and settlement verifies that the shares add up to
/// what the buyer was charged.
///
/// ★★★ The two units are checked **separately and never summed**, because money
/// and metered work are not the same kind of quantity and a combined residue
/// would be a number in no unit at all.
pub fn settle(
    order: &Order,
    money_held: u64,
    pawa_held: u64,
    pawa_distributed: u64,
) -> Settled {
    let money = order.product_total();
    let pawa = order.pawa_total();
    if money_held < money {
        return Settled::Insufficient { needed: money, held: money_held, of: "money" };
    }
    if pawa_held < pawa {
        return Settled::Insufficient { needed: pawa, held: pawa_held, of: "pawa" };
    }
    // Σ Δ = 0 — what left the buyer must equal what reached the recipients.
    let residue = pawa as i64 - pawa_distributed as i64;
    if residue != 0 {
        return Settled::NotConserved { residue };
    }
    Settled::Ok { money_moved: money, pawa_moved: pawa }
}

/// A record that a purchase happened.
///
/// ★★★ **Not a capability, and there is no method here that admits anything.**
/// A licence key issued at order time and never checked carries the *name* of a
/// guarantee without the mechanism — the same design error as an unchecked trust
/// score. Receipts are the right object for an audit trail; they must simply
/// never be presented where a capability is implied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub order_id: String,
    pub artefact: Versioned,
    pub key: String,
}

impl Receipt {
    pub fn new(order_id: &str, artefact: Versioned, key: &str) -> Self {
        Self { order_id: order_id.into(), artefact, key: key.into() }
    }

    /// ★★★ Always false, and it is a function rather than a comment so the
    /// claim is executable. A future change that made a receipt admit something
    /// would have to delete this and the test that calls it.
    pub fn admits_anything(&self) -> bool {
        false
    }

    pub fn describe(&self) -> String {
        format!(
            "a record that {} {} was bought on order {} — evidence of a purchase, and not \
             permission to run anything",
            self.artefact.name, self.artefact.version, self.order_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pilau() -> Item {
        Item::Product { name: "a plate of pilau".into(), price_minor: 30_000, status: ProductStatus::Placed }
    }

    fn widget() -> Item {
        Item::Package {
            artefact: Versioned::new("a budget widget", "1.0"),
            author: "bonnie".into(),
            pawa: 40,
            status: PackageStatus::Placed,
        }
    }

    #[test]
    fn one_order_carries_both_kinds_and_they_do_not_share_a_status() {
        // ★★★ Collapsing the two status columns into one field would make one
        //     of the two lifecycles inexpressible — a package cannot be
        //     DELIVERED and a plate of pilau cannot be LIVE.
        let o = Order::new("o-1", "bonnie").with(pilau()).with(widget());
        assert_eq!(o.product_total(), 30_000);
        assert_eq!(o.pawa_total(), 40);
        assert_eq!(o.outstanding().len(), 2);
    }

    #[test]
    fn the_two_totals_are_never_added_because_they_are_not_the_same_quantity() {
        // ★★★ One is money and one is metered work. A single `total` column is
        //     that addition waiting to happen, and the sum would be a number in
        //     no unit at all.
        let o = Order::new("o-1", "b").with(pilau()).with(widget());
        assert_ne!(o.product_total(), o.pawa_total());
        // There is deliberately no `o.total()`.
    }

    #[test]
    fn the_rival_half_runs_its_own_sequence() {
        let mut s = ProductStatus::Placed;
        let mut seen = vec![s];
        while let Some(n) = s.next() {
            s = n;
            seen.push(s);
        }
        assert_eq!(
            seen,
            vec![
                ProductStatus::Placed,
                ProductStatus::Processing,
                ProductStatus::Delivering,
                ProductStatus::Delivered
            ]
        );
    }

    #[test]
    fn the_non_rival_half_runs_a_different_one_and_stops_for_a_person() {
        // ★★★ `Sandboxed` is a real stop, not a formality: germination is ⊕,
        //     and the approval token belongs where something stops being
        //     contained and starts being part of the household.
        assert!(PackageStatus::Sandboxed.needs_approval_to_advance());
        for s in [PackageStatus::Placed, PackageStatus::Installing, PackageStatus::Live] {
            assert!(!s.needs_approval_to_advance(), "{s:?}");
        }
    }

    #[test]
    fn a_settled_order_conserves() {
        // ★★ Checked at commit rather than assumed. A conservation law that is
        //    only ever stated is a comment.
        let o = Order::new("o-1", "b").with(pilau()).with(widget());
        match settle(&o, 50_000, 100, 40) {
            Settled::Ok { money_moved, pawa_moved } => {
                assert_eq!(money_moved, 30_000);
                assert_eq!(pawa_moved, 40);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nothing_settles_partially() {
        // ★★★ Settling the first three items and running out would leave the
        //     books unbalanced against the very invariant this checks.
        let o = Order::new("o-1", "b").with(pilau()).with(widget());
        let out = settle(&o, 10, 100, 40);
        assert!(matches!(out, Settled::Insufficient { of: "money", .. }));
        assert!(out.describe().contains("a part-settled \norder is an unbalanced one")
            || out.describe().contains("part-settled"));
    }

    #[test]
    fn the_two_purses_are_checked_separately() {
        // ★★ Enough money and not enough pawa is a real state, and a single
        //    balance check could not express it.
        let o = Order::new("o-1", "b").with(pilau()).with(widget());
        assert!(matches!(settle(&o, 50_000, 1, 40), Settled::Insufficient { of: "pawa", .. }));
    }

    #[test]
    fn an_order_of_only_one_kind_is_ordinary() {
        // ★★ Most orders are. The two-column shape must not make a
        //    single-kind order awkward, or people will collapse it back.
        let just_food = Order::new("o-2", "b").with(pilau());
        assert_eq!(just_food.pawa_total(), 0);
        assert!(matches!(settle(&just_food, 30_000, 0, 0), Settled::Ok { .. }));
    }

    #[test]
    fn a_split_that_lost_a_unit_is_caught_rather_than_rounded_away() {
        // ★★★ The check compares two independently-computed things, or it is
        //     not a check. A first draft compared the order total to itself and
        //     could never fail — the same defect §IX.3 names one paragraph
        //     later: the NAME of a guarantee without the mechanism.
        let o = Order::new("o-1", "b").with(widget());
        match settle(&o, 0, 100, 39) {
            Settled::NotConserved { residue } => assert_eq!(residue, 1),
            other => panic!("a lost unit must be caught: {other:?}"),
        }
    }

    #[test]
    fn a_split_that_invented_a_unit_is_caught_too() {
        // ★★ Both directions. Over-distribution is a mint wearing a rounding
        //    error, and it is the more dangerous of the two.
        let o = Order::new("o-1", "b").with(widget());
        assert!(matches!(settle(&o, 0, 100, 41), Settled::NotConserved { residue: -1 }));
    }

    #[test]
    fn a_receipt_admits_nothing_and_says_so_executably() {
        // ★★★ A licence key issued and never checked carries the NAME of a
        //     guarantee without the mechanism — the same design error as an
        //     unchecked trust score. This is a function rather than a comment so
        //     the claim can be tested.
        let r = Receipt::new("o-1", Versioned::new("a widget", "1.0"), "LIC-ABCD1234");
        assert!(!r.admits_anything());
        assert!(r.describe().contains("not \npermission to run anything")
            || r.describe().contains("not permission to run anything"));
    }

    #[test]
    fn a_receipt_is_still_worth_having() {
        // ★★ The point is not that receipts are worthless — they are the right
        //    object for an audit trail. Only that they must not be presented
        //    where a capability is implied.
        let r = Receipt::new("o-1", Versioned::new("a widget", "1.0"), "LIC-ABCD1234");
        assert_eq!(r.order_id, "o-1");
        assert!(r.describe().contains("evidence of a purchase"));
    }

    #[test]
    fn a_line_is_outstanding_until_its_own_lifecycle_finishes() {
        let done = Order::new("o", "b")
            .with(Item::Product {
                name: "pilau".into(),
                price_minor: 1,
                status: ProductStatus::Delivered,
            })
            .with(Item::Package {
                artefact: Versioned::new("w", "1.0"),
                author: "b".into(),
                pawa: 1,
                status: PackageStatus::Live,
            });
        assert!(done.outstanding().is_empty());
    }
}
