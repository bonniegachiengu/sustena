//! Ongoing issuance, replayed against its conformance vectors
//! (Pawa §6.2 · PAWA-8).
//!
//! ★★★ The **governed mint**: new juul enters by rewarding whoever served the
//! pawa, at a rate that moves **only** through PAWA-11's gated Enzyme. Driven
//! entirely through the crate's public surface.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    approval::{ApprovalToken, Binding, EffectClass, NonceLedger, Simulated},
    council::ProposalStatus,
    governance::{
        declared_parameters, definition, enforcement, opening_state, register as register_gov,
        Parameters, ISSUANCE_RATE,
    },
    issuance::{Issuance, Schedule},
    juul::{Entry, Genesis, JuulLedger, MintAuthority},
    operator::{execute, execute_admitted, Authorization, Enforcement, Execution, Registry},
    pawa::{meter, PawaReading},
    semantic::{replay_under, CallOutcome, EnzymeCall, ReplayMode},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("issuance.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "issuance.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn household() -> Value {
    json!({"finances": {
        "liquid": {"balance": 500.0},
        "pockets": {"food": {"allocated": 100.0, "spent": 0.0, "limit": 0.0}},
        "income": {"monthly_total": 0.0, "sources": []}
    }})
}

/// A real, committed run, metered — the **only** way to get a `PawaReading`.
fn served_by(caller: &str) -> PawaReading {
    let reg = Registry::default();
    let e = Enforcement::default();
    let x = execute(
        &reg,
        &reg.names(),
        &e,
        &household(),
        "budget.record_income",
        &params(&[("amount", json!(100.0)), ("source", json!("salary"))]),
    );
    assert!(x.committed(), "the fixture must commit: {:?}", x.result.reason);
    meter(
        &x,
        reg.get("budget.record_income").unwrap(),
        &e,
        &Parameters::genesis(),
        "household",
        caller,
        1_000,
    )
    .expect("a committed run meters")
}

fn issuance() -> Issuance {
    Issuance::declared("iss-1", Schedule::Fixed).expect("a named issuance")
}

fn genesis() -> Genesis {
    Genesis::declared("g-1", &[("bonnie", 500.0)]).expect("a declared genesis")
}

// ── governance, driven for real ──────────────────────────────────────────────

fn gov_registry() -> Registry {
    let mut r = Registry::default();
    register_gov(&mut r);
    r
}

fn token_for(p: &Map<String, Value>, nonce: u64) -> ApprovalToken {
    Simulated::from_sandbox("gov-1", Binding::new("governance.set_parameter", p), Ok(()))
        .expect("the sandbox committed")
        .voted(ProposalStatus::Passed)
        .expect("passed")
        .approve("bonnie", nonce, 9_999)
}

/// Change a parameter through the **real** gate, under a **real** token.
fn change(state: &Value, name: &str, value: f64, nonce: u64) -> Execution {
    let d = definition(&declared_parameters());
    let p = params(&[("name", json!(name)), ("value", json!(value))]);
    let token = token_for(&p, nonce);
    execute_admitted(
        &gov_registry(),
        &d.operators,
        &enforcement(&d),
        state,
        "governance.set_parameter",
        &p,
        &Authorization::Unchecked,
        &EffectClass::Live { token: &token, now: 1_000 },
        &mut NonceLedger::new(),
    )
}

fn gov_opening() -> Value {
    opening_state(&declared_parameters())
}

// ── ★★ the governed mint ─────────────────────────────────────────────────────

#[test]
fn the_rate_is_zero_at_genesis_so_issuance_is_off_until_governance_switches_it_on() {
    // ★★★ A statement, not a placeholder: an economy should not begin inflating
    // because nobody chose a rate.
    assert_eq!(Parameters::genesis().issuance_rate(), 0.0);
    let g = genesis();
    let mut l = JuulLedger::from_genesis(&g);
    let before = l.entries().len();

    let out = l.issue(&issuance(), &served_by("bonnie"), "host-0", &Parameters::genesis());
    assert!(!out.minted(), "{}", out.describe());
    assert_eq!(out.amount(), 0.0);
    assert_eq!(l.entries().len(), before, "a zero mint appends nothing");
    assert_eq!(l.total_in_circulation(), g.total(), "and creates nothing");
}

#[test]
fn an_issuance_at_a_governed_rate_reflects_the_value_read_back_from_state() {
    // ★★★ The row's central proof — PAWA-11 exercised end to end.
    let switched_on = change(&gov_opening(), ISSUANCE_RATE, 1.5, 1);
    assert!(switched_on.committed(), "{:?}", switched_on.result.reason);

    // Read the parameters BACK OUT of the resulting state — not from a const,
    // and not from the value that was passed in.
    let live = Parameters::read(&switched_on.state);
    assert_eq!(live.issuance_rate(), 1.5);

    let r = served_by("bonnie");
    let mut l = JuulLedger::from_genesis(&genesis());
    let out = l.issue(&issuance(), &r, "host-0", &live);

    assert!(out.minted());
    assert_eq!(out.amount(), 1.5 * r.pawa());
    assert_ne!(
        out.amount(),
        issuance().issued_for(&r, &Parameters::genesis()),
        "the genesis rate is not silently shadowing the governed one"
    );
    assert_eq!(l.balance_of("host-0"), out.amount());
}

#[test]
fn the_only_path_to_the_rate_is_the_gated_enzyme() {
    // ★★ Issuance's whole safety property, asserted THROUGH to the issued
    // amount rather than only at the gate.
    let asked = params(&[("name", json!(ISSUANCE_RATE)), ("value", json!(9.0))]);
    let wrong = token_for(&params(&[("name", json!("kappa_compute"))]), 7);
    let d = definition(&declared_parameters());
    let untokened = execute_admitted(
        &gov_registry(),
        &d.operators,
        &enforcement(&d),
        &gov_opening(),
        "governance.set_parameter",
        &asked,
        &Authorization::Unchecked,
        &EffectClass::Live { token: &wrong, now: 1_000 },
        &mut NonceLedger::new(),
    );
    assert!(!untokened.committed());
    assert_eq!(untokened.result.constraint_violated.as_deref(), Some("approval_token"));
    assert_eq!(untokened.state, gov_opening(), "the state did not move");
    assert_eq!(
        Parameters::read(&untokened.state).issuance_rate(),
        0.0,
        "so the rate a later issuance prices from is unmoved"
    );

    // ★ And the ceiling is an ordinary invariant, refused for the ordinary reason.
    let over = change(&gov_opening(), ISSUANCE_RATE, 10_000.0, 8);
    assert!(!over.committed());
    assert_eq!(over.result.constraint_violated.as_deref(), Some("enforcement_gate"));
}

#[test]
fn a_rate_change_is_recorded_and_replayable_like_any_other_parameter() {
    // ★ Issuance needed NO new governance machinery.
    let x = change(&gov_opening(), ISSUANCE_RATE, 2.0, 2);
    assert!(x.committed());
    assert_eq!(x.events.len(), 1);
    assert_eq!(x.events[0].name, "event.governance.parameter_changed");
    assert_eq!(x.events[0].payload["from"], json!(0.0));
    assert_eq!(x.events[0].payload["to"], json!(2.0));

    let calls: Vec<EnzymeCall> = [0.5, 2.0, 3.25]
        .iter()
        .enumerate()
        .map(|(i, v)| {
            EnzymeCall::new(format!("c{i}"), "governance.set_parameter")
                .with("name", json!(ISSUANCE_RATE))
                .with("value", json!(v))
        })
        .collect();
    let out = replay_under(
        &calls,
        &gov_registry(),
        &definition(&declared_parameters()),
        &gov_opening(),
        ReplayMode::Replay,
    );
    assert!(out.steps.iter().all(|s| matches!(s, CallOutcome::Admitted { .. })), "{:?}", out.steps);
    assert_eq!(Parameters::read(&out.state).issuance_rate(), 3.25);
}

// ── ★★ denominated in work ───────────────────────────────────────────────────

#[test]
fn there_is_no_mint_amount_so_there_is_no_issuance_without_work() {
    // ★★★ Structural: `issue` takes a `PawaReading`, which has no public
    // constructor — the only source is metering a run that COMMITTED.
    let r = served_by("bonnie");
    let live = Parameters::read(&json!({"parameters": {"issuance_rate": 3.0}}));
    let mut l = JuulLedger::from_genesis(&genesis());
    let out = l.issue(&issuance(), &r, "host-0", &live);
    assert_eq!(out.amount(), 3.0 * r.pawa(), "the measured work IS the amount");
}

#[test]
fn the_caller_pays_and_the_server_earns_so_an_issuance_is_not_a_refund() {
    let r = served_by("bonnie");
    let live = Parameters::read(&json!({"parameters": {"issuance_rate": 2.0}}));
    let mut l = JuulLedger::from_genesis(&genesis());

    assert!(l.charge(&r).charged());
    let out = l.issue(&issuance(), &r, "host-0", &live);

    assert_eq!(l.balance_of("bonnie"), 500.0 - r.pawa(), "the caller is down by the cost");
    assert_eq!(l.balance_of("host-0"), out.amount(), "the server is up by the issuance");
    assert_ne!(r.pawa(), out.amount(), "and the two are different numbers");
}

#[test]
fn an_issuance_mint_names_the_issuance_that_authorised_it() {
    let live = Parameters::read(&json!({"parameters": {"issuance_rate": 1.0}}));
    let mut l = JuulLedger::from_genesis(&genesis());
    l.issue(&issuance(), &served_by("bonnie"), "host-0", &live);
    match l.entries().last().expect("an entry was appended") {
        Entry::Mint { authority, principal, .. } => {
            assert_eq!(authority, &MintAuthority::Issued(issuance().id().clone()));
            assert_eq!(authority.id(), "iss-1");
            assert!(!authority.is_genesis());
            assert_eq!(principal, "host-0");
        }
        other => panic!("an issuance must be a visible Mint, got {other:?}"),
    }
}

#[test]
fn the_genesis_audit_names_issued_juul_rather_than_calling_it_foreign_or_ignoring_it() {
    // ★★★ The reconcile, made checkable.
    let g = genesis();
    let live = Parameters::read(&json!({"parameters": {"issuance_rate": 2.0}}));
    let mut l = JuulLedger::from_genesis(&g);
    let out = l.issue(&issuance(), &served_by("bonnie"), "host-0", &live);

    let audit = g.audit(&l);
    assert!(audit.clean(), "a governed issuance is legitimate, not a defect: {audit:?}");
    assert!(audit.foreign.is_empty(), "and it is NOT a foreign genesis");
    assert_eq!(audit.issued, vec![("iss-1".to_string(), out.amount())], "but it IS named");
    assert_eq!(audit.issued_total(), out.amount());
    assert!(audit.describe().contains("plus"), "the description says so too: {}", audit.describe());
}

// ── ★★ conservation ──────────────────────────────────────────────────────────

#[test]
fn issuance_is_the_second_and_only_other_circulation_raiser() {
    let g = genesis();
    let live = Parameters::read(&json!({"parameters": {"issuance_rate": 2.0}}));
    let mut l = JuulLedger::from_genesis(&g);
    let r = served_by("bonnie");

    l.issue(&issuance(), &r, "host-0", &live);
    assert!(l.charge(&r).charged());
    assert!(l.transfer("bonnie", "ada", 25.0, "royalty:contributor").charged());

    let debited: f64 = l
        .entries()
        .iter()
        .filter_map(|e| match e {
            Entry::Debit { amount, .. } => Some(*amount),
            _ => None,
        })
        .sum();

    assert_eq!(
        l.total_in_circulation(),
        g.total() + l.issued_total() - debited,
        "circulation = genesis + issuance − costs, transfers at zero"
    );
    // ★ And the two mint terms are SEPARABLE, not merely summing correctly.
    assert_eq!(l.minted_total() - l.issued_total(), g.total());
}

#[test]
fn a_transfer_of_issued_juul_still_moves_no_total() {
    let live = Parameters::read(&json!({"parameters": {"issuance_rate": 1.0}}));
    let mut l = JuulLedger::from_genesis(&genesis());
    let out = l.issue(&issuance(), &served_by("bonnie"), "host-0", &live);

    let before = l.total_in_circulation();
    assert!(l.transfer("host-0", "ada", out.amount(), "royalty:contributor").charged());
    assert_eq!(l.total_in_circulation(), before, "issued juul is ordinary juul once it exists");
    assert_eq!(l.balance_of("ada"), out.amount());
}

#[test]
fn the_no_rail_property_survives_a_second_way_of_creating_juul() {
    // ★★★ ADR-0001 D5 / PAWA-13, re-checked where the word most invites it.
    //
    // ★ Production code only: the file is split at the test module and comment
    // lines are stripped — and that is SAID, because a boundary check that
    // trips on its own prose proves nothing, and one quietly narrowed to pass
    // would prove less.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for module in ["issuance.rs", "juul.rs"] {
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
            "cash_out", "cashout", "withdraw", "redeem", "payout", "fiat", "stripe", "paypal",
            "mpesa", "bank_transfer", "wire_transfer", "exchange_rate",
        ] {
            assert!(!code.contains(forbidden), "'{forbidden}' in {module}: juul is not redeemable");
        }
    }
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-spec-no-reference-issuance");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_TWO_MINTING_PATHS_AND_THEY_STAY_DISTINCT",
        "★★★_2_THE_SCHEDULE_SHAPE_what_is_GOVERNED_vs_DECLARED_AT_SPEC",
        "★★_3_ISSUANCE_NEEDED_NO_NEW_GOVERNANCE_MACHINERY",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }

    // ★★ The two paths must stay named as distinct, and the audit's move with them.
    let paths = step0["★★★_1_TWO_MINTING_PATHS_AND_THEY_STAY_DISTINCT"].as_str().unwrap();
    assert!(paths.contains("NOT a second genesis"));
    assert!(paths.contains("FOREIGN"), "the audit reconcile must stay recorded");

    // ★★ And the governed-vs-declared split must keep its reason, not just its verdict.
    let shape = step0["★★★_2_THE_SCHEDULE_SHAPE_what_is_GOVERNED_vs_DECLARED_AT_SPEC"]
        .as_str()
        .unwrap();
    assert!(shape.contains("THE RATE FITS THAT SHAPE"));
    assert!(shape.contains("HAS NO CLOCK"), "why a decaying schedule does not fit");
    assert!(shape.contains("ONE VARIANT"), "and why the enum is shaped as it is");

    // ★ The greppable false positive must stay named so a re-grepper is not misled.
    assert!(d["★_the_greppable_false_positive_named_so_a_re_grepper_is_not_misled"]
        .as_str()
        .unwrap()
        .contains("bulk transaction issuance"));

    // ★★ The cautionary reference case, both halves.
    let grant = d["★★_what_the_reference_DOES_have_and_why_it_is_the_cautionary_case"]
        .as_str()
        .unwrap();
    assert!(grant.contains("ONBOARDING_GRANT"));
    assert!(grant.contains("DECLARED BUT UNWIRED"));
    assert!(grant.contains("UNDECLARED MINT"));

    // ★★★ The counterweight must name what issuance composes.
    let cw = d["★★★_the_counterweight_what_issuance_COMPOSES_rather_than_invents"]
        .as_str()
        .unwrap();
    for row in ["PAWA-1", "PAWA-7", "PAWA-11"] {
        assert!(cw.contains(row), "the counterweight must name {row}");
    }

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "ONE SERVER",
        "ONE SCHEDULE SHAPE",
        "NOTHING CALLS `issue` AUTOMATICALLY",
        "NO BURN",
        "`Issuance` ITSELF IS UNGOVERNED",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["governed_mint_cases", "denominated_in_work_cases", "conservation_cases"] {
        for c in doc[group].as_array().unwrap_or_else(|| panic!("{group} is an array")) {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 11, "every declared case must be present");
}
