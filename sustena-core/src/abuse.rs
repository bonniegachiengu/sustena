//! **Security falls out of the same mechanics that priced computation**
//! (Pawa · §6.4).
//!
//! ```text
//!   out-of-pawa gating  a run that cannot pay does not run
//!   pricing-via-processing  flooding costs the flooder in proportion to the flood
//!   the efficiency term  lean strategies spare the network
//! ```
//!
//! ★★★ **Nothing here is a new defence.** Every one of the three is the meter
//! doing a second job: the balance check that stops an unaffordable run is the
//! same check that stops a runaway one, and the term that rewards an elegant
//! plan is the same term that reduces load. That is the whole claim of §6.4 —
//! honesty pays, waste costs, and elegance is rewarded, all from the single act
//! of metering a thought.
//!
//! ## The honest cost of pricing via processing
//!
//! ★★★ **It prices the attacker and the honest user identically, and that is
//! not a detail.** Dwork–Naor works because cost is indiscriminate: there is no
//! way to charge a flooder and not charge a household having an unusually busy
//! month. A defence that could tell them apart would not need a price. So the
//! right question is never *"does this stop an attacker"* — it is **"what does
//! this cost the person it was not aimed at"**, and [`burden_on_the_innocent`]
//! exists so that question has a number.
//!
//! ## Where the Sybil arithmetic actually turns
//!
//! ★★★ **A grant that MINTS rather than transfers makes Sybil registration
//! profitable**, and no amount of identity friction fixes it — a transfer is
//! bounded by the treasury and self-limiting, while a mint is unbounded, so the
//! attacker's return per identity is fixed and their cost is whatever identity
//! costs. The defence is not "make identity expensive enough"; it is **do not
//! mint per identity**.

/// What it costs to send `n` junk calls at a given price.
///
/// ★★ Linear, deliberately. Pricing via processing does not need a clever
/// curve; it needs the flooder to pay for the flood, and superlinear pricing
/// would punish a legitimate burst harder than a slow drip attack.
pub fn cost_of_flooding(calls: u64, pawa_per_call: u64) -> u64 {
    calls.saturating_mul(pawa_per_call)
}

/// How long a balance survives at a given rate.
///
/// ★★★ The number that makes out-of-pawa gating a *bound* rather than a hope:
/// an attacker with a finite balance has a finite flood in them, and this says
/// how long it is. `None` when the price is zero — and a free operation has no
/// bound at all, which is worth knowing before declaring one.
pub fn calls_affordable(balance: u64, pawa_per_call: u64) -> Option<u64> {
    (pawa_per_call > 0).then(|| balance / pawa_per_call)
}

/// **What the defence costs the person it was not aimed at.**
///
/// ★★★ Pricing via processing is indiscriminate by construction: a defence that
/// could tell an attacker from a busy household would not need a price. So this
/// is the number that should be looked at before a coefficient is raised —
/// what an ordinary month costs at this rate.
pub fn burden_on_the_innocent(ordinary_calls_per_month: u64, pawa_per_call: u64) -> u64 {
    cost_of_flooding(ordinary_calls_per_month, pawa_per_call)
}

/// Whether raising the price hurts the attacker more than the household.
///
/// ★★★ **It never does, proportionally**, and that is the point of the
/// function: both sides pay the same multiple. What raising the price buys is a
/// shorter flood for a fixed attacker budget, not a better ratio — and anybody
/// reaching for the coefficient should see that the household's bill moved by
/// exactly the same factor.
pub fn raising_the_price_changes_the_ratio() -> bool {
    false
}

/// How a per-identity payout is funded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Funding {
    /// Moved from a purse that had it. `Σ Δ = 0`, bounded, self-limiting.
    Transfer { treasury_balance: u64 },
    /// ★★★ Created. `Σ Δ = +grant`, **unbounded** — and this is the variant that
    /// makes Sybil registration profitable no matter what identity costs.
    Mint,
}

impl Funding {
    pub fn is_bounded(&self) -> bool {
        matches!(self, Self::Transfer { .. })
    }
}

/// What the Sybil arithmetic says about a per-identity grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SybilVerdict {
    /// Manufacturing identities loses money. The grant is safe to offer.
    Uneconomic { profit_per_identity: i64 },
    /// ★★★ Manufacturing identities pays, and the treasury bounds the damage.
    /// Bad, and finite.
    ProfitableButBounded { profit_per_identity: i64, identities_fundable: u64 },
    /// ★★★ Manufacturing identities pays and **nothing bounds it**, because the
    /// grant mints. No amount of identity friction fixes this: the return per
    /// identity is fixed and the supply of identities is not.
    ProfitableAndUnbounded { profit_per_identity: i64 },
}

impl SybilVerdict {
    pub fn safe(&self) -> bool {
        matches!(self, Self::Uneconomic { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Uneconomic { profit_per_identity } => format!(
                "each manufactured identity loses {} — the grant is not worth farming",
                -profit_per_identity
            ),
            Self::ProfitableButBounded { profit_per_identity, identities_fundable } => format!(
                "each manufactured identity nets {profit_per_identity}, and the purse funds \
                 {identities_fundable} of them before it is empty — bad, and finite"
            ),
            Self::ProfitableAndUnbounded { profit_per_identity } => format!(
                "each manufactured identity nets {profit_per_identity} and the grant MINTS, so \
                 nothing bounds it — making identity dearer only changes the rate, and the fix \
                 is not to mint per identity"
            ),
        }
    }
}

/// **Is a per-identity grant farmable?**
///
/// ★★ `identity_cost` is what it costs an attacker to present one more
/// identity: an attestation, an invitation, a verified number. Zero is a real
/// and common answer, and this reports what follows from it rather than
/// refusing to compute.
pub fn sybil(grant: u64, identity_cost: u64, funding: Funding) -> SybilVerdict {
    let profit = grant as i64 - identity_cost as i64;
    if profit <= 0 {
        return SybilVerdict::Uneconomic { profit_per_identity: profit };
    }
    match funding {
        Funding::Mint => SybilVerdict::ProfitableAndUnbounded { profit_per_identity: profit },
        Funding::Transfer { treasury_balance } => SybilVerdict::ProfitableButBounded {
            profit_per_identity: profit,
            identities_fundable: if grant > 0 { treasury_balance / grant } else { 0 },
        },
    }
}

/// What the efficiency term buys the network, as a fraction of load avoided.
///
/// ★★ The same number that rewards an elegant plan. `None` when the wasteful
/// branch cost nothing, because a saving expressed as a fraction of zero is not
/// a saving anybody can read.
pub fn load_spared(wasteful_pawa: u64, lean_pawa: u64) -> Option<f64> {
    (wasteful_pawa > 0)
        .then(|| (wasteful_pawa.saturating_sub(lean_pawa)) as f64 / wasteful_pawa as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flooder_pays_for_the_flood() {
        // ★★ Linear on purpose: pricing via processing needs the flooder to pay
        //    for the flood, and a superlinear curve would punish a legitimate
        //    burst harder than a slow drip attack.
        assert_eq!(cost_of_flooding(1_000, 12), 12_000);
    }

    #[test]
    fn a_finite_balance_is_a_finite_flood() {
        // ★★★ What makes out-of-pawa gating a bound rather than a hope.
        assert_eq!(calls_affordable(12_000, 12), Some(1_000));
    }

    #[test]
    fn a_free_operation_has_no_bound_at_all_and_says_so() {
        // ★★★ Worth knowing before declaring a bound: at zero price the gate
        //     stops nothing, however healthy the balance looks.
        assert_eq!(calls_affordable(1_000_000, 0), None);
    }

    #[test]
    fn the_defence_prices_the_attacker_and_the_household_identically() {
        // ★★★ Dwork-Naor works because cost is indiscriminate. A defence that
        //     could tell them apart would not need a price — so the right
        //     question is never "does this stop an attacker" but "what does it
        //     cost the person it was not aimed at".
        let household_month = burden_on_the_innocent(400, 12);
        let attacker_burst = cost_of_flooding(400, 12);
        assert_eq!(household_month, attacker_burst);
    }

    #[test]
    fn raising_the_price_buys_a_shorter_flood_and_not_a_better_ratio() {
        // ★★★ Both sides pay the same multiple. Anybody reaching for the
        //     coefficient should see that the household's bill moved by exactly
        //     the same factor.
        assert!(!raising_the_price_changes_the_ratio());
        let cheap = (burden_on_the_innocent(400, 1), calls_affordable(10_000, 1).unwrap());
        let dear = (burden_on_the_innocent(400, 10), calls_affordable(10_000, 10).unwrap());
        assert_eq!(dear.0, cheap.0 * 10, "the household pays ten times more");
        assert_eq!(dear.1 * 10, cheap.1, "and the flood is ten times shorter");
    }

    #[test]
    fn a_grant_smaller_than_an_identity_is_not_worth_farming() {
        let v = sybil(100, 150, Funding::Transfer { treasury_balance: 1_000_000 });
        assert!(v.safe());
        assert!(v.describe().contains("not worth farming"));
    }

    #[test]
    fn a_transferred_grant_that_pays_is_bad_and_finite() {
        // ★★ Σ Δ = 0 means the purse bounds the damage. Bad is not the same as
        //    unbounded, and a report that conflated them would make every
        //    profitable grant look like an emergency.
        match sybil(100, 10, Funding::Transfer { treasury_balance: 5_000 }) {
            SybilVerdict::ProfitableButBounded { profit_per_identity, identities_fundable } => {
                assert_eq!(profit_per_identity, 90);
                assert_eq!(identities_fundable, 50);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_minting_grant_is_profitable_and_nothing_bounds_it() {
        // ★★★ The finding that matters: no amount of identity friction fixes a
        //     mint. The return per identity is fixed and the supply of
        //     identities is not, so making identity dearer only changes the
        //     RATE — and the fix is not to mint per identity.
        let v = sybil(100, 10, Funding::Mint);
        assert!(!v.safe());
        assert!(matches!(v, SybilVerdict::ProfitableAndUnbounded { .. }));
        assert!(v.describe().contains("the fix \nis not to mint per identity")
            || v.describe().contains("not to mint per identity"));
    }

    #[test]
    fn making_identity_dearer_does_not_rescue_a_mint() {
        // ★★★ Asserted directly, because "make identity expensive" is the
        //     obvious move and it is the wrong one: at any identity cost below
        //     the grant, a mint is still unbounded.
        for identity_cost in [0, 10, 50, 99] {
            assert!(matches!(
                sybil(100, identity_cost, Funding::Mint),
                SybilVerdict::ProfitableAndUnbounded { .. }
            ));
        }
        // It only stops when the grant is no longer worth having at all.
        assert!(sybil(100, 100, Funding::Mint).safe());
    }

    #[test]
    fn a_transfer_is_bounded_and_a_mint_is_not() {
        assert!(Funding::Transfer { treasury_balance: 1 }.is_bounded());
        assert!(!Funding::Mint.is_bounded());
    }

    #[test]
    fn the_term_that_rewards_elegance_is_the_one_that_spares_the_network() {
        // ★★ One number, two jobs — which is §6.4's whole claim.
        assert_eq!(load_spared(100, 25), Some(0.75));
        assert_eq!(load_spared(0, 0), None, "a saving over nothing is not a saving");
    }
}
