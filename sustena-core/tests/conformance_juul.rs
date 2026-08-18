//! The juul ledger, replayed against its conformance vectors
//! (Pawa §2 · PAWA-2).
//!
//! ★★★ **The hard boundary (ADR-0001 D5 / PAWA-13):** a juul balance is an
//! **internal accounting fact on a single host** — never real money, never a
//! transferable asset, never a payment rail. Structural here: **no transfer, no
//! mint, no rail**.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    approval::{EffectClass, NonceLedger},
    juul::{Affordability, Charge, Entry, JuulLedger},
    operator::{execute, execute_afforded, Authorization, Enforcement, Execution, Registry},
    pawa::{candidate_pawa, meter, PawaReading},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("juul.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "juul.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn state() -> Value {
    json!({"finances": {
        "liquid": {"balance": 500.0},
        "pockets": {"food": {"allocated": 100.0, "spent": 0.0, "limit": 0.0}},
        "income": {"monthly_total": 0.0, "sources": []}
    }})
}

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// A real, committed, metered run — the only way to obtain a `PawaReading`.
fn reading_for(principal: &str) -> PawaReading {
    let reg = Registry::default();
    let e = Enforcement::default();
    let x = execute(
        &reg,
        &reg.names(),
        &e,
        &state(),
        "budget.record_income",
        &params(&[("amount", json!(100.0)), ("source", json!("salary"))]),
    );
    meter(&x, reg.get("budget.record_income").unwrap(), &e, "household", principal, 1_000)
        .expect("it committed")
}

fn funded(principal: &str, juul: f64) -> JuulLedger {
    let mut l = JuulLedger::new();
    l.credit(principal, juul, "declared opening balance");
    l
}

// ── a debit is backed by a measurement ───────────────────────────────────────

#[test]
fn a_charge_is_always_backed_by_a_real_metered_run() {
    // ★★★ `charge` takes a `PawaReading`, which has no public constructor —
    // there is no `debit(amount)` in the module at all.
    let r = reading_for("bonnie");
    let mut l = funded("bonnie", 100.0);
    assert!(l.charge(&r).charged());
    let debit = l.debits_of("bonnie").next().cloned().expect("one debit");
    match debit {
        Entry::Debit { amount, operator, sustain, at, .. } => {
            assert_eq!(amount, r.pawa());
            assert_eq!(operator, "budget.record_income");
            assert_eq!(sustain, "household");
            assert_eq!(at, r.at(), "traceable to the run that incurred it");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_run_is_charged_to_whoever_made_it() {
    let mut l = JuulLedger::new();
    l.credit("bonnie", 100.0, "opening");
    l.credit("cira", 100.0, "opening");
    l.charge(&reading_for("bonnie"));
    assert!(l.balance_of("bonnie") < 100.0);
    assert_eq!(l.balance_of("cira"), 100.0, "somebody else's bill is not theirs");
}

// ── the affordability rule ───────────────────────────────────────────────────

#[test]
fn a_charge_is_admitted_only_if_the_balance_covers_it() {
    let r = reading_for("bonnie");
    let mut l = funded("bonnie", 100.0);
    match l.charge(&r) {
        Charge::Charged { spent, balance } => {
            assert_eq!(spent, r.pawa());
            assert_eq!(balance, 100.0 - r.pawa());
            assert_eq!(balance, l.balance_of("bonnie"));
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn an_unaffordable_charge_is_refused_and_the_balance_is_byte_identical() {
    let r = reading_for("bonnie");
    let mut l = funded("bonnie", 1.0);
    let before = l.clone();

    let out = l.charge(&r);
    assert!(!out.charged(), "{}", out.describe());
    assert_eq!(out.shortfall(), Some(r.pawa() - 1.0));
    assert_eq!(l, before, "a refused charge appended nothing");
    assert_eq!(l.balance_of("bonnie"), 1.0);
}

#[test]
fn a_refused_charge_never_clamps_the_balance_to_zero() {
    let mut l = funded("bonnie", 2.0);
    l.charge(&reading_for("bonnie"));
    assert_eq!(l.balance_of("bonnie"), 2.0, "untouched, not zeroed");
    assert_eq!(l.debits_of("bonnie").count(), 0);
}

#[test]
fn a_balance_exactly_equal_to_the_cost_is_affordable() {
    // `balance ≥ pawa`, not `>`.
    let r = reading_for("bonnie");
    let mut l = funded("bonnie", r.pawa());
    assert!(l.charge(&r).charged());
    assert_eq!(l.balance_of("bonnie"), 0.0);
}

#[test]
fn an_unfunded_principal_starts_at_zero_and_cannot_spend() {
    let mut l = JuulLedger::new();
    assert_eq!(l.balance_of("stranger"), 0.0, "a sum over no entries");
    assert!(!l.charge(&reading_for("stranger")).charged());
    assert!(l.entries().is_empty());
}

// ── the balance is a fold ────────────────────────────────────────────────────

#[test]
fn the_balance_is_the_fold_of_the_entries_and_rebuilds_identically() {
    // ★★ No stored balance exists to drift — the fold IS the balance.
    let mut l = funded("bonnie", 100.0);
    for _ in 0..3 {
        assert!(l.charge(&reading_for("bonnie")).charged());
    }
    assert_eq!(l.rebuild()["bonnie"], l.balance_of("bonnie"));
    assert_eq!(l.entries().len(), 4, "one credit and three debits, append-only");
}

#[test]
fn a_hosts_durable_entries_rebuild_the_same_balance() {
    let mut l = funded("bonnie", 100.0);
    l.charge(&reading_for("bonnie"));
    let reloaded = JuulLedger::with_entries(l.entries().to_vec());
    assert_eq!(reloaded, l);
    assert_eq!(reloaded.balance_of("bonnie"), l.balance_of("bonnie"));
}

// ── the hard boundary ────────────────────────────────────────────────────────

#[test]
fn a_debit_reduces_circulation_and_creates_nothing() {
    let mut l = funded("bonnie", 100.0);
    assert_eq!(l.total_in_circulation(), 100.0);
    let r = reading_for("bonnie");
    l.charge(&r);
    assert_eq!(l.total_in_circulation(), 100.0 - r.pawa());
}

#[test]
fn a_credit_and_a_debit_are_different_entries_not_a_signed_number() {
    let mut l = funded("bonnie", 100.0);
    l.charge(&reading_for("bonnie"));
    assert!(matches!(l.entries()[0], Entry::Credit { .. }));
    assert!(matches!(l.entries()[1], Entry::Debit { .. }));
    assert_eq!(l.debits_of("bonnie").count(), 1, "the credit is not a debit");
}

#[test]
fn charging_one_principal_leaves_every_other_balance_untouched() {
    // ★★★ There is no transfer here: nobody is credited by a charge.
    let mut l = JuulLedger::new();
    l.credit("bonnie", 100.0, "opening");
    l.credit("cira", 50.0, "opening");
    let before = l.total_in_circulation();
    let r = reading_for("bonnie");
    l.charge(&r);
    assert_eq!(l.balance_of("cira"), 50.0);
    assert_eq!(l.total_in_circulation(), before - r.pawa(), "spent, not moved");
}


// ── PAWA-3: the out-of-pawa gate clause ──────────────────────────────────────

fn income_params() -> Map<String, Value> {
    params(&[("amount", json!(100.0)), ("source", json!("salary"))])
}

/// Run `budget.record_income` through the real gate under an economy.
fn afforded(l: &mut JuulLedger, principal: &str) -> Execution {
    let reg = Registry::default();
    let names = reg.names();
    let e = Enforcement::default();
    let mut nonces = NonceLedger::new();
    let mut aff =
        Affordability::Metered { ledger: l, principal, sustain: "household", at: 1_000 };
    execute_afforded(
        &reg,
        &names,
        &e,
        &state(),
        "budget.record_income",
        &income_params(),
        &Authorization::Unchecked,
        &EffectClass::Unchecked,
        &mut nonces,
        &mut aff,
    )
}

#[test]
fn an_affordable_run_commits_and_is_charged_exactly_once() {
    let r = reading_for("bonnie");
    let mut l = funded("bonnie", 100.0);
    let x = afforded(&mut l, "bonnie");
    assert!(x.committed(), "{:?}", x.result.reason);
    assert_eq!(l.debits_of("bonnie").count(), 1, "once, not twice");
    assert_eq!(l.balance_of("bonnie"), 100.0 - r.pawa());
    assert_eq!(l.rebuild()["bonnie"], l.balance_of("bonnie"), "the fold still holds");
}

#[test]
fn an_unaffordable_run_is_refused_and_charges_nothing() {
    // ★★★ The gate stops the work BEFORE it costs anything.
    let mut l = funded("bonnie", 1.0);
    let before = l.clone();
    let x = afforded(&mut l, "bonnie");

    assert!(!x.committed(), "refused on cost");
    assert_eq!(x.result.constraint_violated.as_deref(), Some("insufficient_pawa"));
    assert_eq!(l, before, "balance byte-identical — no juul spent on a refused run");
    assert_eq!(x.state, state(), "and state untouched");
    assert!(x.mutations.is_empty() && x.events.is_empty());
}

#[test]
fn it_prices_against_the_real_cost_not_the_declared_estimate() {
    // ★★★ The reference's trap, as a test.
    let reg = Registry::default();
    assert_eq!(
        reg.get("budget.record_income").unwrap().pawa_cost,
        0,
        "the declared estimate really is zero"
    );
    let real = reading_for("bonnie").pawa();
    assert!(real > 0.0, "and the real cost really is not");

    let mut l = funded("bonnie", real / 2.0);
    let x = afforded(&mut l, "bonnie");
    assert!(!x.committed(), "refused — so it read the measurement, not the guess");
    assert_eq!(l.debits_of("bonnie").count(), 0);
}

#[test]
fn the_candidate_price_equals_the_committed_readings_price() {
    // ★★ Admitting on one price and charging another would be a real defect.
    let reg = Registry::default();
    let e = Enforcement::default();
    let x = execute(&reg, &reg.names(), &e, &state(), "budget.record_income", &income_params());
    let meta = reg.get("budget.record_income").unwrap();
    let candidate = candidate_pawa(&x.mutations, &x.events, meta, &e);
    let charged = meter(&x, meta, &e, "household", "bonnie", 1).unwrap();
    assert_eq!(candidate, charged.pawa());
}

#[test]
fn an_unmetered_call_behaves_exactly_as_before() {
    // ★ Additive and opt-in-safe: no ledger ⇒ the clause is VACUOUS, not
    // failing closed to broke.
    let reg = Registry::default();
    let names = reg.names();
    let e = Enforcement::default();
    let plain = execute(&reg, &names, &e, &state(), "budget.record_income", &income_params());

    let mut nonces = NonceLedger::new();
    let mut aff = Affordability::Unmetered;
    let via = execute_afforded(
        &reg,
        &names,
        &e,
        &state(),
        "budget.record_income",
        &income_params(),
        &Authorization::Unchecked,
        &EffectClass::Unchecked,
        &mut nonces,
        &mut aff,
    );
    assert!(via.committed(), "an unfunded, unmetered caller still runs");
    assert_eq!(via.state, plain.state, "byte-identical to the pre-PAWA-3 path");
    assert_eq!(via.mutations, plain.mutations);
    assert!(!aff.metered());
}

#[test]
fn a_run_refused_by_an_earlier_conjunct_never_reaches_the_charge() {
    let reg = Registry::default();
    let names = reg.names();
    let e = Enforcement::default();
    let mut l = funded("bonnie", 1_000.0);
    let before = l.clone();
    let mut nonces = NonceLedger::new();
    let mut aff =
        Affordability::Metered { ledger: &mut l, principal: "bonnie", sustain: "household", at: 1 };
    let x = execute_afforded(
        &reg,
        &names,
        &e,
        &state(),
        "budget.record_income",
        &params(&[("amount", json!(-5.0)), ("source", json!("s"))]),
        &Authorization::Unchecked,
        &EffectClass::Unchecked,
        &mut nonces,
        &mut aff,
    );
    assert!(!x.committed(), "the guard `params.amount > 0` turned it back");
    assert_eq!(l, before, "a guard refusal charges nothing either");
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-with-a-scaffolded-reference-concept");

    // ★★★ The hard boundary, at the top level.
    let hb = doc["★★★_the_hard_boundary_and_it_is_ADR_0001_D5"].as_str().unwrap();
    assert!(hb.contains("never a transferable asset"));
    assert!(hb.contains("NO TRANSFER") && hb.contains("NO MINT") && hb.contains("NO RAIL"));

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_there_is_no_juul_ledger_in_Rust_and_the_two_existing_Ledgers_are_different_things",
        "★★★_2_a_PawaReading_is_what_a_debit_CONSUMES_and_that_is_made_structural",
        "★★_3_the_affordability_rule_here_is_the_LEDGER'S_not_the_gate's",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    let ledgers = step0
        ["★★★_1_there_is_no_juul_ledger_in_Rust_and_the_two_existing_Ledgers_are_different_things"]
        .as_str()
        .unwrap();
    assert!(ledgers.contains("NonceLedger") && ledgers.contains("PAXOS"));
    assert!(step0["★★_3_the_affordability_rule_here_is_the_LEDGER'S_not_the_gate's"]
        .as_str()
        .unwrap()
        .contains("PAWA-3"));

    // ★★★ The finding must keep both halves: what is scaffolded AND what is wired.
    let finding =
        d["★★★_the_finding_the_reference's_LIVE_debit_path_charges_the_WRONG_NUMBER"]
            .as_str()
            .unwrap();
    assert!(finding.contains("zero callers"), "the brief's premise, confirmed");
    assert!(finding.contains("FOUR REAL CALLERS"), "and the half it did not have");
    assert!(finding.contains("meta.pawa_cost"), "the wrong number must be named");
    assert!(finding.contains("wired to the estimate, not to the measurement"));

    let split = d["★★_what_is_at_parity_and_what_is_the_sharpening"].as_object().unwrap();
    assert_eq!(split.len(), 4, "parity and sharpening, both halves, itemised");
    assert!(split
        .keys()
        .any(|k| k.contains("NOT this row's invention")), "the fold must be credited as parity");

    let not_ported = d["★_what_is_deliberately_NOT_ported"].as_str().unwrap();
    assert!(not_ported.contains("ROYALTY SPLIT"));
    assert!(not_ported.contains("TRANSFER"));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "NOTHING IS WIRED INTO THE GATE",
        "`credit` IS AN UNGATED DECLARATION",
        "THE LEDGER IS IN-MEMORY",
        "A CHARGE HAS NO COUNTERPARTY",
        "BALANCES ARE `f64`",
        "THERE IS NO REVOCATION OR REVERSAL",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    // ★★★ PAWA-3's own reconciles and finding must stay recorded too.
    for key in [
        "★★★_4_PAWA_3_the_candidate_ALREADY_EXISTS_at_gate_time_so_the_real_cost_is_free",
        "★★★_5_PAWA_3_priced_against_the_MEASUREMENT_never_the_ESTIMATE",
        "★★_6_PAWA_3_where_the_conjunct_SITS_and_why",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    let seam = d["★★★_PAWA_3_the_reference_HAS_the_seam_but_never_gates_on_cost"].as_str().unwrap();
    assert!(seam.contains("At parity: the seam itself"));
    assert!(seam.contains("THE CLAUSE NEVER FIRES"));
    let additive =
        d["★★_PAWA_3_additive_and_opt_in_safe_by_the_same_device_used_twice_before"].as_str().unwrap();
    assert!(additive.contains("BYTE-FOR-BYTE"));
    assert!(additive.contains("VACUOUS clause, not a fail-closed one"));
    for term in [
        "THE CHARGE CANNOT BE `Insufficient` BY CONSTRUCTION",
        "THE LEDGER IS BORROWED MUTABLY FOR THE CALL",
        "THE CLAUSE PRICES THE WHOLE RUN, NOT PER-NODE",
        "NOTHING CREDITS ANYONE",
    ] {
        assert!(limits.contains(term), "missing PAWA-3 limit: {term}");
    }

    for group in [
        "backing_cases",
        "affordability_cases",
        "fold_cases",
        "boundary_cases",
        "gate_clause_cases",
    ] {
        for c in doc[group].as_array().unwrap_or_else(|| panic!("{group} is an array")) {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 18, "every declared case must be present");
}
