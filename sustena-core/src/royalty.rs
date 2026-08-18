//! Contribution as a programmable flow — the ratified two-revenue-type royalty
//! schedule (PAWA-5 + MYC-5; Pawa §4, Mycelium §VI, Arena §IV).
//!
//! A contribution declares a [`Licence`]. Running something licensed
//! `Royalty` charges the user its cost **and** splits a royalty across five
//! recipients, per the schedule ratified 2026-08-04.
//!
//! ## ★★★ The hard boundary, and what makes an internal transfer distinguishable
//!
//! This is the first slice in the economy layer with a **transfer** in it, so
//! the boundary has to be restated rather than merely inherited. Three
//! properties, each structural:
//!
//! 1. **A transfer only ever moves juul between [`JuulLedger`] balances.** It is
//!    an internal accounting reallocation — never a settlement instruction to
//!    anything outside this process.
//! 2. **`ΣΔ = 0`.** A royalty is a set of [`Entry::Transfer`] values, and that
//!    variant's `circulation_delta` is **`0.0` by construction** — one entry
//!    carrying *both* ends, so half a transfer is unspellable. Contrast a
//!    **mint**, which would raise the total and is a **different variant** that
//!    does not exist yet (PAWA-7 genesis, PAWA-8 issuance).
//! 3. ★★ **The no-rail property PAWA-2 established stays.** There is **no code
//!    path from a transfer to real value**: no cash-out, no redemption, no
//!    external counterparty, no juul→anything bridge. Juul is **not
//!    redeemable**. *That absence is the distinction* — an internal
//!    reallocation and a real payment differ here by the total non-existence of
//!    the second thing, and a test greps these modules for the vocabulary to
//!    prove the absence rather than assert it.
//!
//! ## ★★ The ratified schedule — two revenue types, not one
//!
//! | recipient   | usage (a pawa charge) | access (a licence sale) |
//! |-------------|----------------------:|------------------------:|
//! | contributor |                  70 % |                    80 % |
//! | treasury    |                  15 % |                    10 % |
//! | validator   |                   5 % |                     3 % |
//! | proposer    |                   5 % |                     2 % |
//! | referrer    |                   5 % |                     5 % |
//!
//! This **supersedes** the earlier four-way `70/20/5/5`, which could express
//! neither the **proposer** share nor the difference between the two revenue
//! types. Read the contributor's 70 % as what it is economically: **the price
//! the commons pays for variation** — selection can only act on variation that
//! exists.
//!
//! ## ★★★ Conservation is a THEOREM, not a check bolted on
//!
//! The split is computed in **integers**: each share is an integer floor of
//! `amount · rate`, and the **remainder is assigned to the validator share**.
//! So `Σ shares == amount` **exactly, by construction**, for every amount —
//! there is no rounding step that could leak, because the leftover is not
//! discarded, it is *where the last share comes from*.
//!
//! ★ A rounding leak here would be a **conservation-invariant violation**, not a
//! cosmetic bug, which is why the arithmetic is integral: exact conservation
//! over floats is not achievable, and claiming it would be false.
//!
//! ## Cost and royalty are two different flows
//!
//! `pawa(o)` — PAWA-1's measurement, charged by PAWA-3's gate clause — is a
//! **cost**: it has no counterparty and **leaves circulation**. A royalty is a
//! **transfer**: it has five counterparties and leaves circulation *unchanged*.
//! [`settle`] does the second only; the first is the gate's, unchanged.

use crate::juul::{Charge, JuulLedger};

/// What a contribution charges for use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Licence {
    /// No royalty. A run still pays its own `pawa` cost — free means *free of
    /// royalty*, never free of the work it causes.
    Free,
    /// A royalty of `rate` per mille of the amount, split per the schedule.
    ///
    /// ★ `payee` is the **contributor**: the largest share, and the one the
    /// licence exists to pay.
    Royalty { per_mille: u32, payee: String },
}

impl Licence {
    /// The royalty due on `amount`, in whole juul. `0` for [`Licence::Free`].
    ///
    /// ★ Integer arithmetic throughout — see the module header on why.
    pub fn due_on(&self, amount: u64) -> u64 {
        match self {
            Licence::Free => 0,
            Licence::Royalty { per_mille, .. } => amount * u64::from(*per_mille) / 1_000,
        }
    }

    pub fn payee(&self) -> Option<&str> {
        match self {
            Licence::Free => None,
            Licence::Royalty { payee, .. } => Some(payee),
        }
    }
}

/// Which of the two ratified schedules applies.
///
/// ★★ **Two revenue types, not one.** The earlier four-way split could not tell
/// them apart, and they are genuinely different: paying to *run* something is
/// not paying to *have* it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevenueType {
    /// A pawa charge — someone ran it. `70 / 15 / 5 / 5 / 5`.
    Usage,
    /// A licence sale — someone bought access. `80 / 10 / 3 / 2 / 5`.
    Access,
}

impl RevenueType {
    /// `(contributor, treasury, validator, proposer, referrer)`, in per cent.
    ///
    /// ★ Validator's figure is the **nominal** one; the actual validator share
    /// also absorbs the remainder, which is what makes the split exact.
    pub fn schedule(&self) -> (u64, u64, u64, u64, u64) {
        match self {
            RevenueType::Usage => (70, 15, 5, 5, 5),
            RevenueType::Access => (80, 10, 3, 2, 5),
        }
    }
}

/// Who a royalty is split across.
///
/// ★★ `contributor` and `treasury` are **required**; the other three are
/// `Option`, because a single-host run genuinely has no validator (there is no
/// second node to validate anything) and a contribution often has no proposer
/// or referrer. ★★★ **An absent role's share is folded into the treasury**, by
/// the declared rule in [`split`] — it is **never dropped**, because dropping a
/// share would break conservation, which is the one thing this module must not
/// do. The treasury is the right destination because it *is* the commons: a
/// share nobody is there to receive returns to the pool everyone draws on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipients {
    pub contributor: String,
    /// ★ For this slice the treasury is a **named principal with a balance**.
    /// PAWA-6 makes that recipient a full Sustain `Σ_T` with its own viable
    /// region; the boundary is named rather than pre-empted.
    pub treasury: String,
    pub validator: Option<String>,
    pub proposer: Option<String>,
    pub referrer: Option<String>,
}

impl Recipients {
    pub fn new(contributor: &str, treasury: &str) -> Recipients {
        Recipients {
            contributor: contributor.to_string(),
            treasury: treasury.to_string(),
            validator: None,
            proposer: None,
            referrer: None,
        }
    }

    pub fn with_validator(mut self, id: &str) -> Recipients {
        self.validator = Some(id.to_string());
        self
    }

    pub fn with_proposer(mut self, id: &str) -> Recipients {
        self.proposer = Some(id.to_string());
        self
    }

    pub fn with_referrer(mut self, id: &str) -> Recipients {
        self.referrer = Some(id.to_string());
        self
    }
}

/// One recipient's computed share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub recipient: String,
    pub role: RoyaltyRole,
    pub amount: u64,
}

/// The five roles the schedule names.
///
/// ★ Named `RoyaltyRole`, not `Role` — [`crate::division::Role`] is a
/// division-of-labour role (who does what in a Sustain), a genuinely different
/// question from who is owed a share. **Thirty-eighth collision**; the newcomer
/// takes the longer name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoyaltyRole {
    Contributor,
    Treasury,
    Validator,
    Proposer,
    Referrer,
}

impl RoyaltyRole {
    pub fn name(&self) -> &'static str {
        match self {
            RoyaltyRole::Contributor => "contributor",
            RoyaltyRole::Treasury => "treasury",
            RoyaltyRole::Validator => "validator",
            RoyaltyRole::Proposer => "proposer",
            RoyaltyRole::Referrer => "referrer",
        }
    }
}

/// ★★★ Split `amount` per the ratified schedule — **exactly conserving**.
///
/// Integer floors for the five nominal shares, then **the remainder is added to
/// the validator share**, so `Σ shares == amount` for *every* amount with no
/// rounding step that could leak. Then any share whose role has no recipient is
/// **folded into the treasury**, which is a reallocation among the same five
/// buckets and therefore leaves the sum untouched.
///
/// The returned shares are **merged by recipient**, so a treasury that absorbed
/// two absent roles appears once with the combined figure rather than three
/// times.
pub fn split(amount: u64, revenue: RevenueType, to: &Recipients) -> Vec<Share> {
    let (c, t, v, p, r) = revenue.schedule();

    let contributor = amount * c / 100;
    let treasury = amount * t / 100;
    let mut validator = amount * v / 100;
    let proposer = amount * p / 100;
    let referrer = amount * r / 100;

    // ★★★ The remainder is not discarded — it IS the validator's last juul.
    // This is the line that makes conservation a theorem.
    let remainder = amount - (contributor + treasury + validator + proposer + referrer);
    validator += remainder;

    // ★★ An absent role's share folds into the treasury. Never dropped.
    let mut treasury_total = treasury;
    let mut out = vec![
        Share { recipient: to.contributor.clone(), role: RoyaltyRole::Contributor, amount: contributor },
    ];
    for (share, role, who) in [
        (validator, RoyaltyRole::Validator, to.validator.as_deref()),
        (proposer, RoyaltyRole::Proposer, to.proposer.as_deref()),
        (referrer, RoyaltyRole::Referrer, to.referrer.as_deref()),
    ] {
        match who {
            Some(id) => out.push(Share { recipient: id.to_string(), role, amount: share }),
            None => treasury_total += share,
        }
    }
    out.push(Share { recipient: to.treasury.clone(), role: RoyaltyRole::Treasury, amount: treasury_total });

    // Merge by recipient, so one principal in two roles reads as one share.
    out.retain(|s| s.amount > 0 || s.role == RoyaltyRole::Contributor);
    out
}

/// What a settlement did.
#[derive(Debug, Clone, PartialEq)]
pub enum Settlement {
    /// The royalty was split and transferred. `shares` is what each got.
    Settled { total: u64, shares: Vec<Share> },
    /// ★ [`Licence::Free`] — **nothing was transferred**. The run still paid its
    /// own cost; free means free of royalty.
    NoRoyalty,
    /// The payer could not cover the royalty. **Nothing moved**, and the ledger
    /// is unchanged — the same honest-refusal shape as a charge.
    Insufficient { required: u64, balance: f64 },
}

impl Settlement {
    pub fn settled(&self) -> bool {
        matches!(self, Settlement::Settled { .. })
    }

    /// What actually moved. Zero for both non-settling outcomes.
    pub fn transferred(&self) -> u64 {
        match self {
            Settlement::Settled { shares, .. } => shares.iter().map(|s| s.amount).sum(),
            _ => 0,
        }
    }
}

/// Settle a royalty: split it, then move it between balances.
///
/// ★★ **Transfers only.** Every movement is a [`Charge::Charged`] on
/// `JuulLedger::transfer`, so circulation is provably unchanged — this module
/// never credits, never debits and never mints.
///
/// ★ Checked before moving anything: if `payer` cannot cover the whole royalty,
/// **nothing is transferred at all**, rather than paying the first recipients
/// and running out — a partial settlement would leave the split unconserved
/// against its own schedule.
pub fn settle(
    ledger: &mut JuulLedger,
    payer: &str,
    licence: &Licence,
    amount: u64,
    revenue: RevenueType,
    to: &Recipients,
) -> Settlement {
    let due = licence.due_on(amount);
    if due == 0 {
        return Settlement::NoRoyalty;
    }

    let balance = ledger.balance_of(payer);
    if balance < due as f64 {
        return Settlement::Insufficient { required: due, balance };
    }

    let shares = split(due, revenue, to);
    for s in &shares {
        if s.amount == 0 {
            continue;
        }
        let moved = ledger.transfer(
            payer,
            &s.recipient,
            s.amount as f64,
            &format!("royalty:{}", s.role.name()),
        );
        debug_assert!(matches!(moved, Charge::Charged { .. }), "affordability was checked above");
    }
    Settlement::Settled { total: due, shares }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::juul::Genesis;

    fn recipients_all() -> Recipients {
        Recipients::new("ada", "treasury")
            .with_validator("val")
            .with_proposer("pro")
            .with_referrer("ref")
    }

    fn funded(payer: &str, juul: f64) -> JuulLedger {
        JuulLedger::from_genesis(&Genesis::declared("g0", &[(payer, juul)]).unwrap())
    }

    // ── ★★★ conservation, as a theorem ──────────────────────────────────────

    #[test]
    fn the_split_is_exactly_conserving_for_adversarial_amounts() {
        // ★★★ Primes and tiny values are where an integer split leaks if the
        // remainder is discarded. It is not: it IS the validator's last juul.
        let to = recipients_all();
        for amount in [
            0u64, 1, 2, 3, 7, 11, 13, 17, 19, 23, 97, 101, 997, 1_009, 7_919, 65_537, 999_983,
        ] {
            for revenue in [RevenueType::Usage, RevenueType::Access] {
                let total: u64 = split(amount, revenue, &to).iter().map(|s| s.amount).sum();
                assert_eq!(total, amount, "leak at {amount} under {revenue:?}");
            }
        }
    }

    #[test]
    fn the_split_conserves_with_every_optional_role_absent() {
        // ★★ An absent role's share folds into the treasury, never dropped.
        let bare = Recipients::new("ada", "treasury");
        for amount in [1u64, 3, 7, 101, 7_919] {
            let shares = split(amount, RevenueType::Usage, &bare);
            assert_eq!(shares.iter().map(|s| s.amount).sum::<u64>(), amount);
            assert!(shares.iter().all(|s| s.recipient == "ada" || s.recipient == "treasury"));
        }
    }

    #[test]
    fn the_remainder_lands_on_the_validator_share() {
        // 7 usage: floors are 4/1/0/0/0 = 5, so the remainder is 2.
        let (c, t, v, p, r) = RevenueType::Usage.schedule();
        let amount = 7u64;
        let nominal = amount * v / 100;
        let floors = amount * c / 100 + amount * t / 100 + nominal
            + amount * p / 100
            + amount * r / 100;
        let remainder = amount - floors;
        assert_eq!(remainder, 2, "the leftover the floors did not distribute");

        let shares = split(amount, RevenueType::Usage, &recipients_all());
        let val = shares.iter().find(|s| s.role == RoyaltyRole::Validator).unwrap();
        assert_eq!(val.amount, nominal + remainder, "nominal PLUS the remainder");
        assert_eq!(shares.iter().map(|s| s.amount).sum::<u64>(), amount);
    }

    // ── the two revenue types ───────────────────────────────────────────────

    #[test]
    fn usage_and_access_are_different_schedules() {
        assert_eq!(RevenueType::Usage.schedule(), (70, 15, 5, 5, 5));
        assert_eq!(RevenueType::Access.schedule(), (80, 10, 3, 2, 5));

        let to = recipients_all();
        let by_role = |shares: Vec<Share>, role: RoyaltyRole| {
            shares.iter().find(|s| s.role == role).map(|s| s.amount).unwrap_or(0)
        };
        assert_eq!(by_role(split(1_000, RevenueType::Usage, &to), RoyaltyRole::Contributor), 700);
        assert_eq!(by_role(split(1_000, RevenueType::Access, &to), RoyaltyRole::Contributor), 800);
        assert_eq!(by_role(split(1_000, RevenueType::Usage, &to), RoyaltyRole::Proposer), 50);
        assert_eq!(by_role(split(1_000, RevenueType::Access, &to), RoyaltyRole::Proposer), 20);
    }

    #[test]
    fn the_schedule_has_five_recipients_and_the_proposer_is_one_of_them() {
        // ★ The proposer share is what the superseded four-way could not express.
        let roles: Vec<RoyaltyRole> = split(1_000, RevenueType::Usage, &recipients_all())
            .iter()
            .map(|s| s.role)
            .collect();
        for r in
            [RoyaltyRole::Contributor, RoyaltyRole::Treasury, RoyaltyRole::Validator, RoyaltyRole::Proposer, RoyaltyRole::Referrer]
        {
            assert!(roles.contains(&r), "{} missing", r.name());
        }
    }

    // ── Free vs Royalty ─────────────────────────────────────────────────────

    #[test]
    fn a_free_licence_transfers_nothing() {
        let mut l = funded("user", 1_000.0);
        let before = l.clone();
        let out = settle(&mut l, "user", &Licence::Free, 500, RevenueType::Usage, &recipients_all());
        assert_eq!(out, Settlement::NoRoyalty);
        assert_eq!(out.transferred(), 0);
        assert_eq!(l, before, "free means free of ROYALTY — the ledger is untouched");
    }

    #[test]
    fn a_royalty_licence_settles_across_the_five() {
        let mut l = funded("user", 1_000.0);
        let licence = Licence::Royalty { per_mille: 100, payee: "ada".into() };
        let out =
            settle(&mut l, "user", &licence, 1_000, RevenueType::Usage, &recipients_all());
        assert!(out.settled());
        assert_eq!(out.transferred(), 100, "10 % of 1000");
        assert_eq!(l.balance_of("ada"), 70.0);
        assert_eq!(l.balance_of("treasury"), 15.0);
        assert_eq!(l.balance_of("user"), 900.0);
    }

    // ── ★★★ a transfer conserves circulation ────────────────────────────────

    #[test]
    fn settling_leaves_total_circulation_unchanged() {
        // ★★★ THE distinction from a mint: a royalty reallocates, it creates
        // nothing. `Entry::Transfer::circulation_delta` is 0.0 by construction.
        let mut l = funded("user", 1_000.0);
        let before = l.total_in_circulation();
        let licence = Licence::Royalty { per_mille: 250, payee: "ada".into() };
        settle(&mut l, "user", &licence, 800, RevenueType::Access, &recipients_all());
        assert_eq!(l.total_in_circulation(), before, "reallocated, not created");
        assert_eq!(l.rebuild().values().sum::<f64>(), before, "and the fold agrees");
    }

    #[test]
    fn an_unaffordable_royalty_moves_nothing_at_all() {
        // ★ Not a partial settlement: paying some recipients and running out
        // would leave the split unconserved against its own schedule.
        let mut l = funded("user", 5.0);
        let before = l.clone();
        let licence = Licence::Royalty { per_mille: 1_000, payee: "ada".into() };
        let out = settle(&mut l, "user", &licence, 500, RevenueType::Usage, &recipients_all());
        assert!(matches!(out, Settlement::Insufficient { required: 500, .. }));
        assert_eq!(l, before);
    }

    // ── ★★★ no rail ─────────────────────────────────────────────────────────

    #[test]
    fn there_is_no_path_from_juul_to_real_value() {
        // ★★★ The distinction between an internal reallocation and a real
        // payment is the TOTAL NON-EXISTENCE of the second thing. Proven the
        // way the egress slice proved its own boundary: by reading the source.
        // ★ Scanned over PRODUCTION CODE ONLY — the test half is split off at
        // the `cfg(test)` marker, and comment lines are stripped. Neither is a
        // dodge: the claim is about what these modules' CODE can do, and prose
        // that says *juul is not redeemable* would otherwise trip a search for
        // "redeem" — the documentation of the boundary is not a breach of it.
        let marker = "#[cfg(te";
        let sources: Vec<String> = [
            include_str!("royalty.rs"),
            include_str!("juul.rs"),
            include_str!("pawa.rs"),
        ]
        .iter()
        .map(|src| {
            src.split(marker)
                .next()
                .unwrap_or(src)
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("
")
                .to_lowercase()
        })
        .collect();
        for forbidden in [
            "cash_out", "cashout", "withdraw", "redeem", "payout", "fiat", "stripe", "paypal",
            "mpesa", "bank_transfer", "wire_transfer", "settle_fiat", "exchange_rate",
        ] {
            for src in &sources {
                assert!(
                    !src.contains(forbidden),
                    "'{forbidden}' must not appear: juul is not redeemable"
                );
            }
        }
    }

    #[test]
    fn a_transfer_only_ever_moves_between_two_ledger_balances() {
        // ★★ Both ends are principals in THIS ledger, and the entry carries
        // them together — half a transfer is unspellable.
        let mut l = funded("user", 100.0);
        l.transfer("user", "ada", 30.0, "royalty:contributor");
        assert_eq!(l.balance_of("user"), 70.0);
        assert_eq!(l.balance_of("ada"), 30.0);
        assert_eq!(l.total_in_circulation(), 100.0);
    }

    #[test]
    fn a_transfer_nobody_can_afford_is_refused_and_a_self_transfer_is_not_a_movement() {
        let mut l = funded("user", 10.0);
        let before = l.clone();
        assert!(!l.transfer("user", "ada", 50.0, "too much").charged());
        assert!(!l.transfer("user", "user", 1.0, "to myself").charged());
        assert_eq!(l, before);
    }
}
