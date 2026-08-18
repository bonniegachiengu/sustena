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
    juul::{Affordability, Audit, Charge, Entry, Genesis, JuulLedger, MintAuthority},
    operator::{execute, execute_afforded, Authorization, Enforcement, Execution, Registry},
    governance::Parameters,
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
    meter(&x, reg.get("budget.record_income").unwrap(), &e, &Parameters::genesis(), "household", principal, 1_000)
        .expect("it committed")
}

fn funded(principal: &str, juul: f64) -> JuulLedger {
    JuulLedger::from_genesis(&Genesis::declared("g0", &[(principal, juul)]).unwrap())
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
    let mut l = JuulLedger::from_genesis(
        &Genesis::declared("g0", &[("bonnie", 100.0), ("cira", 100.0)]).unwrap(),
    );
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
    assert!(matches!(l.entries()[0], Entry::Mint { .. }));
    assert!(matches!(l.entries()[1], Entry::Debit { .. }));
    assert_eq!(l.debits_of("bonnie").count(), 1, "the credit is not a debit");
}

#[test]
fn charging_one_principal_leaves_every_other_balance_untouched() {
    // ★★★ There is no transfer here: nobody is credited by a charge.
    let mut l = JuulLedger::from_genesis(
        &Genesis::declared("g0", &[("bonnie", 100.0), ("cira", 50.0)]).unwrap(),
    );
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
    let params_gov = Parameters::genesis();
    let mut aff = Affordability::Metered {
        ledger: l,
        parameters: &params_gov,
        principal,
        sustain: "household",
        at: 1_000,
    };
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
    let candidate = candidate_pawa(&x.mutations, &x.events, meta, &e, &Parameters::genesis());
    let charged = meter(&x, meta, &e, &Parameters::genesis(), "household", "bonnie", 1).unwrap();
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
    let params_gov = Parameters::genesis();
    let mut aff = Affordability::Metered {
        ledger: &mut l,
        parameters: &params_gov,
        principal: "bonnie",
        sustain: "household",
        at: 1,
    };
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

// ── ★★★ PAWA-7 genesis + MYC-6: minting is declared and visible ─────────────

fn g_two() -> Genesis {
    Genesis::declared("genesis-2026", &[("bonnie", 100.0), ("cira", 40.0)]).unwrap()
}

#[test]
fn a_mint_is_its_own_variant_and_names_the_declaration_that_authorised_it() {
    // ★★★ MYC-6: a mint is VISIBLE as a mint, never a silent credit.
    let g = g_two();
    let l = JuulLedger::from_genesis(&g);
    assert_eq!(l.mints().count(), 2);
    for m in l.mints() {
        match m {
            Entry::Mint { authority, .. } => assert_eq!(authority, &MintAuthority::Genesis(g.id().clone())),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn circulation_is_raised_only_by_a_mint() {
    // ★★★ The accounting: circulation = Σ(mints) − Σ(costs debited),
    // transfers contributing nothing.
    let g = g_two();
    let mut l = JuulLedger::from_genesis(&g);
    assert_eq!(l.total_in_circulation(), g.total());

    // A transfer raises nothing.
    assert!(l.transfer("bonnie", "cira", 25.0, "gift").charged());
    assert_eq!(l.total_in_circulation(), g.total());

    // A debit lowers it, and nothing anywhere raises it again.
    let r = reading_for("bonnie");
    assert!(l.charge(&r).charged());
    assert_eq!(l.total_in_circulation(), g.total() - r.pawa());

    let minted: f64 = l.mints().map(|m| m.circulation_delta()).sum();
    assert_eq!(minted, g.total(), "every juul in existence came from a mint");
}

#[test]
fn an_empty_ledger_has_no_juul_because_it_has_no_genesis() {
    // ★ The honest empty case: no declaration, no money — not a shortcut
    // around genesis.
    let l = JuulLedger::new();
    assert_eq!(l.total_in_circulation(), 0.0);
    assert_eq!(l.mints().count(), 0);
}

#[test]
fn genesis_is_one_time_because_it_is_a_constructor_not_a_method() {
    // ★★★ There is no `mint` method: you cannot mint INTO an existing
    // ledger. A second genesis makes a second ledger, not more juul in this
    // one — so the one-time-ness is a property of the shape.
    let g = g_two();
    let l = JuulLedger::from_genesis(&g);
    let before = l.total_in_circulation();

    let other = Genesis::declared("some-other", &[("mallory", 1_000_000.0)]).unwrap();
    let _elsewhere = JuulLedger::from_genesis(&other);
    assert_eq!(l.total_in_circulation(), before, "another declaration is another ledger");
}

#[test]
fn a_genesis_refuses_a_negative_allocation() {
    // ★ A genesis that took juul from someone would not be a genesis.
    assert!(Genesis::declared("g", &[("bonnie", -1.0)]).is_none());
    assert!(Genesis::declared("", &[("bonnie", 1.0)]).is_none(), "and it must be named");
}

// ── ★★ auditability: declared once, checkable forever ───────────────────

#[test]
fn a_ledgers_money_supply_is_exactly_its_declared_genesis() {
    let g = g_two();
    let mut l = JuulLedger::from_genesis(&g);
    // Trading and spending change balances, never the declaration.
    l.transfer("bonnie", "cira", 30.0, "gift");
    l.charge(&reading_for("cira"));

    let audit = g.audit(&l);
    assert!(audit.clean(), "{}", audit.describe());
    assert_eq!(audit, Audit { foreign: vec![], mismatched: vec![], undeclared: vec![], issued: vec![] });
}

#[test]
fn an_audit_names_a_mint_that_the_declaration_does_not_explain() {
    // ★★ A ledger reloaded from a host's durable copy could carry a mint
    // the declaration never made. The audit NAMES it rather than letting a
    // total quietly absorb it.
    let g = g_two();
    let mut entries = JuulLedger::from_genesis(&g).entries().to_vec();
    entries.push(Entry::Mint {
        principal: "mallory".into(),
        amount: 500.0,
        authority: MintAuthority::Genesis(g.id().clone()),
    });
    let audit = g.audit(&JuulLedger::with_entries(entries));
    assert!(!audit.clean());
    assert_eq!(audit.undeclared, vec!["mallory".to_string()]);
}

#[test]
fn an_audit_names_a_mint_from_a_foreign_declaration() {
    let g = g_two();
    let other = Genesis::declared("not-ours", &[("mallory", 1.0)]).unwrap();
    let mut entries = JuulLedger::from_genesis(&g).entries().to_vec();
    entries.extend(JuulLedger::from_genesis(&other).entries().to_vec());
    let audit = g.audit(&JuulLedger::with_entries(entries));
    assert!(!audit.clean());
    assert_eq!(audit.foreign, vec!["not-ours".to_string()]);
}

#[test]
fn a_balance_is_fully_explained_by_genesis_plus_the_folded_entries() {
    // ★★★ The fold over genesis: every balance equals its allocation plus
    // everything that happened to it, with nothing unexplained.
    let g = g_two();
    let mut l = JuulLedger::from_genesis(&g);
    l.transfer("bonnie", "cira", 30.0, "gift");
    let r = reading_for("cira");
    l.charge(&r);

    for who in ["bonnie", "cira"] {
        let since_genesis: f64 = l
            .entries()
            .iter()
            .filter(|e| !e.is_mint())
            .map(|e| e.delta_for(who))
            .sum();
        assert_eq!(
            l.balance_of(who),
            g.allocation_for(who) + since_genesis,
            "{who}'s balance is genesis plus the fold, exactly"
        );
    }
    assert_eq!(l.rebuild()["bonnie"], l.balance_of("bonnie"), "and rebuild still agrees");
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

    // ★★★ PAWA-7 + MYC-6's reconciles and finding must stay recorded too.
    for key in [
        "★★★_7_PAWA_7_the_reconcile_that_MATTERED_credit_WAS_an_undeclared_mint",
        "★★★_8_PAWA_7_one_time_ness_is_STRUCTURAL_because_from_genesis_is_a_CONSTRUCTOR",
        "★★_9_PAWA_7_minting_the_INTERNAL_unit_is_not_issuing_a_real_coin",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    let mint = step0["★★★_7_PAWA_7_the_reconcile_that_MATTERED_credit_WAS_an_undeclared_mint"]
        .as_str()
        .unwrap();
    assert!(mint.contains("WAS A MINT"), "the finding must stay named");
    assert!(mint.contains("legitimate UNDECLARED"), "and MYC-6's own sentence kept");
    assert!(step0["★★_9_PAWA_7_minting_the_INTERNAL_unit_is_not_issuing_a_real_coin"]
        .as_str()
        .unwrap()
        .contains("Minting juul ≠ issuing a coin"));

    let grant = d["★★★_PAWA_7_the_reference_has_a_grant_whose_KIND_IS_UNDECLARED"].as_str().unwrap();
    assert!(grant.contains("ZERO CALLERS"));
    assert!(grant.contains("KIND IS UNSTATED"));
    for term in [
        "`GenesisId` HAS NO PUBLIC CONSTRUCTOR",
        "THERE IS NO BURN",
        "A GENESIS ALLOCATION OF ZERO IS ALLOWED",
        "NOTHING TIES A GENESIS TO A SUSTAIN",
    ] {
        assert!(limits.contains(term), "missing PAWA-7 limit: {term}");
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
        "genesis_cases",
        "audit_cases",
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
    assert_eq!(seen.len(), 27, "every declared case must be present");
}
