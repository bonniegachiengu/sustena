//! **A stable unit, not a volatile asset** (Pawa · Additions; Mycelium §VI).
//!
//! ★★★ **The gas-token posture fixes what Juul is *for*; the peg fixes its
//! *stability*.** A household's costs are in shillings, so a unit that floated
//! against the shilling would mean the price of running a Sustain moved for
//! reasons that had nothing to do with the Sustain. The peg is not a financial
//! flourish — it is what keeps the meter's numbers meaning the same thing next
//! month.
//!
//! ★★★ **And this module builds the SHAPE, not the instrument.** §6.7's line
//! governs: issuing a real, public, transferable token backed by real reserves
//! is a distinct, later, human-authorized act with legal weight. So there is no
//! constructor here that marks a peg live, no path that moves real money, and
//! [`Peg::is_live`] is false for everything this codebase can build — exactly
//! as [`crate::settlement::Rail::Ethereum`] is.
//!
//! ## Collateralisation is a ratio somebody must check
//!
//! ★★★ **A peg is a claim about reserves, and a claim nobody checks is a
//! rumour.** [`Reserves::covers`] is the check, and it is deliberately not a
//! boolean on the peg itself: the ratio is what tells you whether the peg is
//! merely intact or comfortably so, and a boolean cannot show a peg thinning.
//!
//! ★★ **The segregated floor is separate from the rest of the reserve** because
//! it is the part that is not supposed to be working. Instruments earn; a floor
//! sits there. Collapsing them into one number lets a reserve look healthy while
//! the part that must always be liquid has been lent out.
//!
//! ## Migratable behind the switch
//!
//! ★★ The peg target is a *value*, not a hard-coded currency: a CBK digital
//! shilling or a basket is a different [`Target`], not a rewrite. Same discipline
//! as the settlement adapter.

/// What the unit is pegged to.
///
/// ★★ An enum rather than a string, so a target nobody has thought about cannot
/// be introduced by a typo — and migration is a new variant, reviewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The Kenyan Shilling. One Juul is one shilling.
    KenyanShilling,
    /// A central-bank digital shilling, if one arrives.
    DigitalShilling,
    /// A basket, at scale. The weights are declared with it.
    Basket { components: Vec<(String, u32)> },
}

impl Target {
    pub fn describe(&self) -> String {
        match self {
            Self::KenyanShilling => "the Kenyan Shilling".into(),
            Self::DigitalShilling => "a central-bank digital shilling".into(),
            Self::Basket { components } => format!(
                "a basket of {}",
                components.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(", ")
            ),
        }
    }

    /// ★★ Do the weights add to a hundred? A basket whose weights do not sum is
    /// not a basket, it is a list.
    pub fn is_well_formed(&self) -> bool {
        match self {
            Self::Basket { components } => {
                components.iter().map(|(_, w)| *w).sum::<u32>() == 100 && !components.is_empty()
            }
            _ => true,
        }
    }
}

/// What backs the unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reserves {
    /// Held in permitted instruments. These earn, and they can move in value.
    pub instruments_minor: u64,
    /// ★★★ The segregated bank floor. **Kept separate** because it is the part
    /// that is not supposed to be working — collapsing the two lets a reserve
    /// look healthy while the part that must always be liquid has been lent out.
    pub segregated_floor_minor: u64,
}

impl Reserves {
    pub fn total(&self) -> u64 {
        self.instruments_minor + self.segregated_floor_minor
    }

    /// **Does this cover what has been issued?**
    ///
    /// ★★★ A ratio rather than a boolean, because a boolean cannot show a peg
    /// *thinning*. 1.02 and 4.00 are both "covered" and are not the same
    /// situation.
    pub fn covers(&self, issued_minor: u64) -> Option<f64> {
        (issued_minor > 0).then(|| self.total() as f64 / issued_minor as f64)
    }

    /// Is the part that must always be liquid actually there?
    ///
    /// ★★ Checked against its own floor, separately from the total: a reserve
    /// can be over-collateralised in instruments and still fail here, and that
    /// is the failure that matters on the day somebody redeems.
    pub fn floor_intact(&self, required_floor_minor: u64) -> bool {
        self.segregated_floor_minor >= required_floor_minor
    }
}

/// How healthy the peg is.
#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    /// Covered, with room.
    Comfortable { ratio: f64 },
    /// Covered, and only just. **Not the same as healthy**, and a boolean would
    /// have said it was.
    Thin { ratio: f64 },
    /// Not covered. The peg is a claim the reserves do not support.
    Undercollateralised { ratio: f64 },
    /// ★★★ The instruments cover it and the segregated floor does not hold.
    /// **Covered on paper and unable to pay** — the failure that only shows on
    /// the day somebody redeems.
    FloorBreached { ratio: f64, floor_short_by: u64 },
    /// Nothing issued, so there is nothing to cover.
    NothingIssued,
}

impl Health {
    pub fn describe(&self) -> String {
        match self {
            Self::Comfortable { ratio } => format!("covered {ratio:.2}× over"),
            Self::Thin { ratio } => {
                format!("covered {ratio:.2}× — intact, and thin, which is not the same as healthy")
            }
            Self::Undercollateralised { ratio } => {
                format!("covered only {ratio:.2}× — the peg is a claim the reserves do not support")
            }
            Self::FloorBreached { floor_short_by, .. } => format!(
                "the segregated floor is {floor_short_by} short — covered on paper and unable to \
                 pay on the day somebody redeems"
            ),
            Self::NothingIssued => "nothing issued, so there is nothing to cover".into(),
        }
    }

    pub fn is_sound(&self) -> bool {
        matches!(self, Self::Comfortable { .. } | Self::NothingIssued)
    }
}

/// Below this, coverage is reported as thin rather than comfortable.
///
/// ★★ Declared, because "fully backed" at 1.00 leaves no room for an
/// instrument to move a percent before the peg is a claim nobody can honour.
pub const COMFORTABLE_RATIO: f64 = 1.05;

/// A peg, as this codebase can hold one.
#[derive(Debug, Clone, PartialEq)]
pub struct Peg {
    pub target: Target,
    pub reserves: Reserves,
    pub issued_minor: u64,
    pub required_floor_minor: u64,
}

impl Peg {
    /// ★★★ **Always false.** There is no field and no constructor that could
    /// make it true, because going live is a legal and financial act with
    /// human authorization — not a boolean this codebase gets to set. A function
    /// rather than a comment, so a change would have to delete it and its test.
    pub fn is_live(&self) -> bool {
        false
    }

    /// **How the peg is holding.**
    pub fn health(&self) -> Health {
        let Some(ratio) = self.reserves.covers(self.issued_minor) else {
            return Health::NothingIssued;
        };
        if !self.reserves.floor_intact(self.required_floor_minor) {
            return Health::FloorBreached {
                ratio,
                floor_short_by: self
                    .required_floor_minor
                    .saturating_sub(self.reserves.segregated_floor_minor),
            };
        }
        if ratio < 1.0 {
            Health::Undercollateralised { ratio }
        } else if ratio < COMFORTABLE_RATIO {
            Health::Thin { ratio }
        } else {
            Health::Comfortable { ratio }
        }
    }

    /// What a holder would get back, in the pegged unit.
    ///
    /// ★★ One-to-one by definition of a peg — and this returns the *claim*, not
    /// a movement of money. Nothing here can pay anybody.
    pub fn redemption_claim(&self, juul: u64) -> u64 {
        juul
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reserves(instruments: u64, floor: u64) -> Reserves {
        Reserves { instruments_minor: instruments, segregated_floor_minor: floor }
    }

    fn peg(instruments: u64, floor: u64, issued: u64, required_floor: u64) -> Peg {
        Peg {
            target: Target::KenyanShilling,
            reserves: reserves(instruments, floor),
            issued_minor: issued,
            required_floor_minor: required_floor,
        }
    }

    #[test]
    fn no_peg_this_codebase_can_build_is_live() {
        // ★★★ Going live is a legal and financial act with human
        //     authorization, not a boolean. A function rather than a comment,
        //     so a change would have to delete this and the test.
        assert!(!peg(1_000_000, 500_000, 1_000_000, 100_000).is_live());
    }

    #[test]
    fn a_peg_is_a_claim_about_reserves_and_the_claim_is_checked() {
        // ★★ A claim nobody checks is a rumour.
        let p = peg(900_000, 300_000, 1_000_000, 100_000);
        assert!(matches!(p.health(), Health::Comfortable { .. }));
    }

    #[test]
    fn thin_is_not_the_same_as_healthy_and_a_boolean_would_have_said_it_was() {
        // ★★★ 1.02 and 4.00 are both "covered" and are not the same situation.
        let p = peg(1_000_000, 20_000, 1_000_000, 10_000);
        match p.health() {
            Health::Thin { ratio } => assert!((1.0..COMFORTABLE_RATIO).contains(&ratio)),
            other => panic!("{other:?}"),
        }
        assert!(!p.health().is_sound());
        assert!(p.health().describe().contains("not the same as healthy"));
    }

    #[test]
    fn undercollateralised_says_the_reserves_do_not_support_the_claim() {
        let p = peg(400_000, 100_000, 1_000_000, 50_000);
        assert!(matches!(p.health(), Health::Undercollateralised { .. }));
        assert!(p.health().describe().contains("do not support"));
    }

    #[test]
    fn a_reserve_can_be_over_collateralised_and_still_unable_to_pay() {
        // ★★★ The failure that only shows on the day somebody redeems. The
        //     segregated floor is the part that is not supposed to be working,
        //     and collapsing it into the total lets a reserve look healthy
        //     while the liquid part has been lent out.
        let p = peg(10_000_000, 0, 1_000_000, 200_000);
        match p.health() {
            Health::FloorBreached { ratio, floor_short_by } => {
                assert!(ratio > 5.0, "hugely over-collateralised on paper");
                assert_eq!(floor_short_by, 200_000);
            }
            other => panic!("the floor must be checked separately: {other:?}"),
        }
        assert!(p.health().describe().contains("unable to \npay")
            || p.health().describe().contains("unable to pay"));
    }

    #[test]
    fn nothing_issued_is_not_a_coverage_failure() {
        // ★★ A ratio over zero issued is not infinity, it is undefined — and
        //    reporting it as a failure would alarm somebody about a peg that
        //    has not been used.
        let p = peg(0, 0, 0, 0);
        assert_eq!(p.health(), Health::NothingIssued);
        assert!(p.health().is_sound());
    }

    #[test]
    fn the_target_is_a_value_so_migration_is_a_variant_and_not_a_rewrite() {
        // ★★ A CBK digital shilling or a basket is a different target, reviewed
        //    as a new variant rather than introduced by a typo in a string.
        assert!(Target::KenyanShilling.is_well_formed());
        assert!(Target::DigitalShilling.is_well_formed());
        assert!(Target::Basket {
            components: vec![("KES".into(), 70), ("USD".into(), 30)]
        }
        .is_well_formed());
    }

    #[test]
    fn a_basket_whose_weights_do_not_sum_is_a_list_and_not_a_basket() {
        assert!(!Target::Basket { components: vec![("KES".into(), 70)] }.is_well_formed());
        assert!(!Target::Basket { components: vec![] }.is_well_formed());
    }

    #[test]
    fn redemption_returns_a_claim_and_moves_nothing() {
        // ★★ One-to-one by definition of a peg — and nothing here can pay
        //    anybody, which is the whole point of building the shape and not
        //    the instrument.
        let p = peg(1_000_000, 500_000, 1_000_000, 100_000);
        assert_eq!(p.redemption_claim(250), 250);
        assert!(!p.is_live());
    }

    #[test]
    fn the_comfortable_ratio_is_declared_rather_than_being_one() {
        // ★★ "Fully backed" at 1.00 leaves no room for an instrument to move a
        //    percent before the peg is a claim nobody can honour.
        const { assert!(COMFORTABLE_RATIO > 1.0) };
    }
}
