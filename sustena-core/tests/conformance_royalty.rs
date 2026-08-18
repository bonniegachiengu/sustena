//! The ratified royalty schedule, replayed against its conformance vectors
//! (Pawa §4 · Mycelium §VI · Arena §IV · PAWA-5 + MYC-5).
//!
//! ★★★ The **first** slice in the economy layer with a **transfer** in it. Every
//! prior module kept the hard boundary by having no transfer at all; here the
//! distinction is made structurally instead — **internal only, `ΣΔ = 0`, and no
//! rail**.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    juul::JuulLedger,
    royalty::{settle, split, Licence, Recipients, RevenueType, RoyaltyRole, Settlement, Share},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("royalty.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "royalty.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn all_five() -> Recipients {
    Recipients::new("ada", "treasury")
        .with_validator("val")
        .with_proposer("pro")
        .with_referrer("ref")
}

fn funded(payer: &str, juul: f64) -> JuulLedger {
    let mut l = JuulLedger::new();
    l.credit(payer, juul, "opening");
    l
}

fn share_of(shares: &[Share], role: RoyaltyRole) -> u64 {
    shares.iter().find(|s| s.role == role).map(|s| s.amount).unwrap_or(0)
}

// ── ★★★ conservation, as a theorem ───────────────────────────────────────────

#[test]
fn the_split_is_exactly_conserving_for_adversarial_amounts() {
    // ★★★ Primes and tiny values are where an integer split leaks if the
    // remainder is discarded. It is not: it IS the validator's last juul.
    let to = all_five();
    for amount in
        [0u64, 1, 2, 3, 7, 11, 13, 17, 19, 23, 97, 101, 997, 1_009, 7_919, 65_537, 999_983]
    {
        for revenue in [RevenueType::Usage, RevenueType::Access] {
            let total: u64 = split(amount, revenue, &to).iter().map(|s| s.amount).sum();
            assert_eq!(total, amount, "leak at {amount} under {revenue:?}");
        }
    }
}

#[test]
fn the_split_conserves_with_every_optional_role_absent() {
    // ★★★ The single-host case: no validator, no proposer, no referrer. Their
    // shares fold into the TREASURY — never dropped, because dropping one
    // would break conservation silently.
    let bare = Recipients::new("ada", "treasury");
    for amount in [1u64, 3, 7, 101, 7_919, 999_983] {
        for revenue in [RevenueType::Usage, RevenueType::Access] {
            let shares = split(amount, revenue, &bare);
            assert_eq!(shares.iter().map(|s| s.amount).sum::<u64>(), amount, "leak at {amount}");
            assert!(shares.iter().all(|s| s.recipient == "ada" || s.recipient == "treasury"));
        }
    }
}

#[test]
fn the_remainder_lands_on_the_validator_share() {
    // 7 usage: floors are 4/1/0/0/0 = 5, so the remainder is 2.
    let (c, t, v, p, r) = RevenueType::Usage.schedule();
    let amount = 7u64;
    let nominal = amount * v / 100;
    let floors =
        amount * c / 100 + amount * t / 100 + nominal + amount * p / 100 + amount * r / 100;
    let remainder = amount - floors;
    assert_eq!(remainder, 2, "the leftover the floors did not distribute");

    let shares = split(amount, RevenueType::Usage, &all_five());
    assert_eq!(
        share_of(&shares, RoyaltyRole::Validator),
        nominal + remainder,
        "nominal PLUS the remainder"
    );
    assert_eq!(shares.iter().map(|s| s.amount).sum::<u64>(), amount);
}

#[test]
fn settling_leaves_total_circulation_unchanged() {
    // ★★★ THE distinction from a mint: a royalty reallocates, it creates
    // nothing. `Entry::Transfer::circulation_delta` is 0.0 by construction.
    let mut l = funded("user", 1_000.0);
    let before = l.total_in_circulation();
    let licence = Licence::Royalty { per_mille: 250, payee: "ada".into() };
    settle(&mut l, "user", &licence, 800, RevenueType::Access, &all_five());
    assert_eq!(l.total_in_circulation(), before, "reallocated, not created");
    assert_eq!(l.rebuild().values().sum::<f64>(), before, "and the independent fold agrees");
}

// ── the ratified schedule ────────────────────────────────────────────────────

#[test]
fn usage_and_access_are_different_schedules() {
    assert_eq!(RevenueType::Usage.schedule(), (70, 15, 5, 5, 5));
    assert_eq!(RevenueType::Access.schedule(), (80, 10, 3, 2, 5));

    let to = all_five();
    let usage = split(1_000, RevenueType::Usage, &to);
    let access = split(1_000, RevenueType::Access, &to);
    assert_eq!(share_of(&usage, RoyaltyRole::Contributor), 700);
    assert_eq!(share_of(&access, RoyaltyRole::Contributor), 800);
    assert_eq!(share_of(&usage, RoyaltyRole::Treasury), 150);
    assert_eq!(share_of(&access, RoyaltyRole::Treasury), 100);
    assert_eq!(share_of(&usage, RoyaltyRole::Proposer), 50);
    assert_eq!(share_of(&access, RoyaltyRole::Proposer), 20);
}

#[test]
fn the_schedule_has_five_recipients_and_the_proposer_is_one_of_them() {
    // ★ The proposer share is what the superseded four-way could not express.
    let roles: BTreeSet<RoyaltyRole> =
        split(1_000, RevenueType::Usage, &all_five()).iter().map(|s| s.role).collect();
    assert_eq!(roles.len(), 5);
    for r in [
        RoyaltyRole::Contributor,
        RoyaltyRole::Treasury,
        RoyaltyRole::Validator,
        RoyaltyRole::Proposer,
        RoyaltyRole::Referrer,
    ] {
        assert!(roles.contains(&r), "{} missing", r.name());
    }
}

// ── Free vs Royalty ──────────────────────────────────────────────────────────

#[test]
fn a_free_licence_transfers_nothing() {
    let mut l = funded("user", 1_000.0);
    let before = l.clone();
    let out = settle(&mut l, "user", &Licence::Free, 500, RevenueType::Usage, &all_five());
    assert_eq!(out, Settlement::NoRoyalty);
    assert_eq!(out.transferred(), 0);
    assert_eq!(l, before, "free means free of ROYALTY — the ledger is untouched");
}

#[test]
fn a_royalty_licence_settles_across_the_five() {
    let mut l = funded("user", 1_000.0);
    let licence = Licence::Royalty { per_mille: 100, payee: "ada".into() };
    let out = settle(&mut l, "user", &licence, 1_000, RevenueType::Usage, &all_five());
    assert!(out.settled());
    assert_eq!(out.transferred(), 100, "10 % of 1000");
    assert_eq!(l.balance_of("ada"), 70.0);
    assert_eq!(l.balance_of("treasury"), 15.0);
    assert_eq!(l.balance_of("user"), 900.0, "the payer fell by exactly what the five gained");
}

#[test]
fn an_unaffordable_royalty_moves_nothing_at_all() {
    // ★ Refused wholesale: a partial settlement would leave the split
    // unconserved against its own schedule.
    let mut l = funded("user", 5.0);
    let before = l.clone();
    let licence = Licence::Royalty { per_mille: 1_000, payee: "ada".into() };
    let out = settle(&mut l, "user", &licence, 500, RevenueType::Usage, &all_five());
    assert!(matches!(out, Settlement::Insufficient { required: 500, .. }));
    assert_eq!(l, before);
}

// ── ★★★ the hard boundary ────────────────────────────────────────────────────

#[test]
fn there_is_no_path_from_juul_to_real_value() {
    // ★★★ The distinction between an internal reallocation and a real payment
    // is the TOTAL NON-EXISTENCE of the second thing — so it is proven by
    // reading the source, as the egress slice proved its own boundary.
    //
    // ★ Production code only: comment lines are stripped, because documenting
    // the boundary (*juul is not redeemable*) is not a breach of it.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for module in ["royalty.rs", "juul.rs", "pawa.rs"] {
        let src = fs::read_to_string(root.join(module)).expect("module is readable");
        let code: String = src
            .split("#[cfg(te")
            .next()
            .unwrap_or(&src)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
            .to_lowercase();
        for forbidden in [
            "cash_out",
            "cashout",
            "withdraw",
            "redeem",
            "payout",
            "fiat",
            "stripe",
            "paypal",
            "mpesa",
            "bank_transfer",
            "wire_transfer",
            "exchange_rate",
        ] {
            assert!(
                !code.contains(forbidden),
                "'{forbidden}' in {module}: juul is not redeemable"
            );
        }
    }
}

#[test]
fn a_transfer_only_ever_moves_between_two_ledger_balances() {
    let mut l = funded("user", 100.0);
    assert!(l.transfer("user", "ada", 30.0, "royalty:contributor").charged());
    assert_eq!(l.balance_of("user"), 70.0);
    assert_eq!(l.balance_of("ada"), 30.0);
    assert_eq!(l.total_in_circulation(), 100.0, "moved, not created or destroyed");
}

#[test]
fn a_transfer_nobody_can_afford_is_refused_and_a_self_transfer_is_not_a_movement() {
    let mut l = funded("user", 10.0);
    let before = l.clone();
    assert!(!l.transfer("user", "ada", 50.0, "too much").charged());
    assert!(!l.transfer("user", "user", 1.0, "to myself").charged());
    assert_eq!(l, before);
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-ratified-schedule-superseding-a-scaffolded-reference");

    // ★★★ The three structural properties of the boundary must stay stated.
    let hb = doc
        ["★★★_the_hard_boundary_what_makes_an_internal_transfer_DISTINGUISHABLE_from_a_real_one"]
        .as_str()
        .unwrap();
    assert!(hb.contains("ONE ENTRY CARRYING BOTH ENDS"));
    assert!(hb.contains("NOT REDEEMABLE"));
    assert!(hb.contains("TOTAL NON-EXISTENCE"));
    assert!(hb.contains("PAWA-7 genesis, PAWA-8 issuance"), "mint stays a different variant");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_the_reference's_four_way_split_is_SUPERSEDED_not_a_port_target",
        "★★★_2_the_ratified_schedule_confirmed_against_the_row_term_by_term",
        "★★_3_why_the_arithmetic_is_INTEGRAL_and_the_ledger_is_not",
        "★_4_the_cost_and_the_royalty_are_two_different_flows_and_stay_apart",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    let sched = step0["★★★_2_the_ratified_schedule_confirmed_against_the_row_term_by_term"]
        .as_str()
        .unwrap();
    assert!(sched.contains("70/15/5/5/5") && sched.contains("80/10/3/2/5"));
    assert!(sched.contains("REMAINDER ASSIGNED TO THE VALIDATOR SHARE"));
    assert!(step0["★★★_1_the_reference's_four_way_split_is_SUPERSEDED_not_a_port_target"]
        .as_str()
        .unwrap()
        .contains("ZERO CALLERS"));

    let split_note = d["★★_what_is_at_parity_and_what_is_the_sharpening"].as_object().unwrap();
    assert_eq!(split_note.len(), 4, "parity and sharpenings, itemised");
    assert!(split_note.keys().any(|k| k.contains("at parity — the concept")));

    let absence = d["★★★_the_single_host_role_absence_and_how_it_is_handled"].as_str().unwrap();
    assert!(absence.contains("FOLDED INTO THE TREASURY"));
    assert!(absence.contains("NOT dropped"));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE TREASURY IS A NAMED PRINCIPAL, NOT YET A SUSTAIN",
        "`settle` IS NOT WIRED INTO THE GATE",
        "THE ROYALTY BASE IS SUPPLIED BY THE CALLER",
        "A PARTIAL SETTLEMENT IS REFUSED WHOLESALE",
        "NOTHING HERE MINTS",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["conservation_cases", "schedule_cases", "licence_cases", "boundary_cases"] {
        for c in doc[group].as_array().unwrap_or_else(|| panic!("{group} is an array")) {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 12, "every declared case must be present");
}
