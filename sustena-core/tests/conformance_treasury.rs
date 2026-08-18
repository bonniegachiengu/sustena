//! The treasury as a Sustain, replayed against its conformance vectors
//! (Pawa §5 · PAWA-6).
//!
//! ★★★ The row whose point is that there is **no new machinery**: a treasury is
//! a `Definition` plus a state, admitted like any Enzyme and replayed like any
//! Sustain. This binary drives it entirely through the crate's **public**
//! surface, which is itself part of the proof.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    approval::{ApprovalToken, Binding, EffectClass, NonceLedger, Simulated},
    council::ProposalStatus,
    editing::Definition,
    juul::{Genesis, JuulLedger},
    operator::{execute_admitted, Authorization, Registry},
    semantic::{replay_under, CallOutcome, EnzymeCall, ReplayMode},
    treasury::{definition, enforcement, opening_state, pay_out, register, PayOut},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("treasury.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "treasury.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

const FLOOR: f64 = 100.0;
const CAPS: &[(&str, f64)] = &[("grants", 500.0)];

fn registry() -> Registry {
    let mut r = Registry::default();
    register(&mut r);
    r
}

fn sigma_t() -> Definition {
    definition(FLOOR, CAPS)
}

fn state_of(balance: f64) -> Value {
    opening_state(balance, 20, 300.0, CAPS)
}

fn funded(juul: f64) -> JuulLedger {
    JuulLedger::from_genesis(&Genesis::declared("g0", &[("treasury", juul)]).unwrap())
}

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// A real approval token — simulated, voted, approved. The only chain there is.
fn token_for(operator: &str, p: &Map<String, Value>, nonce: u64) -> ApprovalToken {
    Simulated::from_sandbox("prop-1", Binding::new(operator, p), Ok(()))
        .expect("the sandbox committed")
        .voted(ProposalStatus::Passed)
        .expect("passed")
        .approve("bonnie", nonce, 9_999)
}

fn grant_of(l: &mut JuulLedger, state: &Value, amount: f64, nonce: u64) -> PayOut {
    let asked = params(&[("recipient", json!("ada")), ("amount", json!(amount))]);
    let mut bound = asked.clone();
    bound.insert("ledger_balance".into(), json!(l.balance_of("treasury")));
    let token = token_for("treasury.grant", &bound, nonce);
    pay_out(
        &registry(),
        &sigma_t(),
        l,
        &mut NonceLedger::new(),
        "treasury",
        state,
        "treasury.grant",
        &asked,
        &token,
        1_000,
    )
}

// ── ★★★ no new machinery ─────────────────────────────────────────────────────

#[test]
fn the_treasury_is_an_ordinary_definition_with_ordinary_operators() {
    let d = sigma_t();
    let reg = registry();
    for op in &d.operators {
        let meta = reg.get(op).unwrap_or_else(|| panic!("{op} not registered"));
        assert!(!meta.params.is_empty(), "{op} declares its shape like any other operator");
    }
    assert!(d
        .invariants
        .iter()
        .any(|(id, expr)| id == "reserve_floor" && expr.contains("treasury.balance >=")));
}

#[test]
fn an_allocation_moves_no_juul_at_all() {
    let d = sigma_t();
    let l = funded(1_000.0);
    let before = l.total_in_circulation();
    let x = execute_admitted(
        &registry(),
        &d.operators,
        &enforcement(&d),
        &state_of(1_000.0),
        "treasury.allocate",
        &params(&[("category", json!("grants")), ("amount", json!(250.0))]),
        &Authorization::Unchecked,
        &EffectClass::Unchecked,
        &mut NonceLedger::new(),
    );
    assert!(x.committed(), "{:?}", x.result.reason);
    assert_eq!(x.state["treasury"]["allocations"]["grants"], json!(250.0));
    assert_eq!(x.state["treasury"]["balance"], json!(1_000.0), "an earmark is not a payment");
    assert_eq!(l.total_in_circulation(), before);
}

#[test]
fn the_treasury_replays_under_its_own_definition_like_any_sustain() {
    // ★★★ `replay_under` takes a `Definition` and knows nothing about
    // treasuries — the recursion, demonstrated rather than argued.
    let call = EnzymeCall::new("t1", "treasury.allocate")
        .with("category", json!("grants"))
        .with("amount", json!(250.0));
    let out =
        replay_under(&[call], &registry(), &sigma_t(), &state_of(1_000.0), ReplayMode::Replay);
    assert!(matches!(out.steps.first(), Some(CallOutcome::Admitted { .. })), "{:?}", out.steps);
}

// ── ★★ V_T ───────────────────────────────────────────────────────────────────

#[test]
fn a_payout_that_would_breach_the_reserve_floor_is_refused_by_the_ordinary_gate() {
    // Balance 150, floor 100: paying 80 would leave 70.
    let mut l = funded(150.0);
    let before = l.clone();
    let out = grant_of(&mut l, &state_of(150.0), 80.0, 1);

    assert!(!out.paid());
    assert_eq!(
        out.execution.result.constraint_violated.as_deref(),
        Some("enforcement_gate"),
        "the SAME reason a household invariant breach gives"
    );
    assert!(out.moved.is_none(), "the transfer was not even attempted");
    assert_eq!(l, before, "the ledger is byte-identical");
}

#[test]
fn a_payout_that_stays_above_the_floor_commits() {
    let mut l = funded(150.0);
    let out = grant_of(&mut l, &state_of(150.0), 40.0, 2);
    assert!(out.paid(), "{:?}", out.execution.result.reason);
    assert_eq!(l.balance_of("treasury"), 110.0);
    assert_eq!(l.balance_of("ada"), 40.0);
}

#[test]
fn a_category_cap_is_the_same_device_as_the_floor() {
    let d = sigma_t();
    let x = execute_admitted(
        &registry(),
        &d.operators,
        &enforcement(&d),
        &state_of(1_000.0),
        "treasury.allocate",
        &params(&[("category", json!("grants")), ("amount", json!(600.0))]),
        &Authorization::Unchecked,
        &EffectClass::Unchecked,
        &mut NonceLedger::new(),
    );
    assert!(!x.committed());
    assert_eq!(x.result.constraint_violated.as_deref(), Some("enforcement_gate"));
}

// ── ★★★ conserved, never minted ──────────────────────────────────────────────

#[test]
fn a_treasury_outflow_leaves_total_circulation_unchanged() {
    let mut l = funded(1_000.0);
    let before = l.total_in_circulation();
    assert!(grant_of(&mut l, &state_of(1_000.0), 200.0, 3).paid());
    assert_eq!(l.total_in_circulation(), before, "moved, not minted");
    assert_eq!(l.balance_of("treasury") + l.balance_of("ada"), before);
}

#[test]
fn the_treasury_cannot_pay_out_more_than_it_holds() {
    let mut l = funded(150.0);
    let before = l.clone();
    let out = grant_of(&mut l, &state_of(150.0), 500.0, 4);
    assert!(!out.paid());
    assert_eq!(out.execution.result.constraint_violated.as_deref(), Some("treasury_solvent"));
    assert_eq!(l, before);
}

#[test]
fn the_states_balance_is_a_projection_of_the_ledger_not_a_second_record() {
    // ★★★ One truth: the state figure was derived from the ledger this call.
    let mut l = funded(1_000.0);
    let out = grant_of(&mut l, &state_of(1_000.0), 200.0, 5);
    assert!(out.paid());
    assert_eq!(
        out.execution.state["treasury"]["balance"].as_f64().unwrap(),
        l.balance_of("treasury")
    );
}

#[test]
fn a_disbursement_draws_down_its_own_earmark() {
    let d = sigma_t();
    let reg = registry();
    let mut l = funded(1_000.0);
    let allocated = execute_admitted(
        &reg,
        &d.operators,
        &enforcement(&d),
        &state_of(1_000.0),
        "treasury.allocate",
        &params(&[("category", json!("grants")), ("amount", json!(300.0))]),
        &Authorization::Unchecked,
        &EffectClass::Unchecked,
        &mut NonceLedger::new(),
    );
    assert!(allocated.committed());

    let asked = params(&[
        ("recipient", json!("ada")),
        ("category", json!("grants")),
        ("amount", json!(120.0)),
    ]);
    let mut bound = asked.clone();
    bound.insert("ledger_balance".into(), json!(1_000.0));
    let out = pay_out(
        &reg,
        &d,
        &mut l,
        &mut NonceLedger::new(),
        "treasury",
        &allocated.state,
        "treasury.disburse",
        &asked,
        &token_for("treasury.disburse", &bound, 11),
        1_000,
    );
    assert!(out.paid(), "{:?}", out.execution.result.reason);
    assert_eq!(
        out.execution.state["treasury"]["allocations"]["grants"],
        json!(180.0),
        "an allocation cannot be spent twice"
    );
}

// ── ★★ every disbursement is human-gated ─────────────────────────────────────

#[test]
fn a_payout_without_a_valid_token_is_refused() {
    let mut l = funded(1_000.0);
    let before = l.clone();
    let asked = params(&[("recipient", json!("ada")), ("amount", json!(50.0))]);
    let wrong = token_for("treasury.grant", &params(&[("recipient", json!("mallory"))]), 9);
    let out = pay_out(
        &registry(),
        &sigma_t(),
        &mut l,
        &mut NonceLedger::new(),
        "treasury",
        &state_of(1_000.0),
        "treasury.grant",
        &asked,
        &wrong,
        1_000,
    );
    assert!(!out.paid());
    assert_eq!(out.execution.result.constraint_violated.as_deref(), Some("approval_token"));
    assert!(out.moved.is_none());
    assert_eq!(l, before);
}

#[test]
fn a_token_is_single_use_so_a_payout_cannot_be_replayed() {
    let d = sigma_t();
    let reg = registry();
    let mut l = funded(1_000.0);
    let mut nonces = NonceLedger::new();
    let state = state_of(1_000.0);
    let asked = params(&[("recipient", json!("ada")), ("amount", json!(50.0))]);
    let mut bound = asked.clone();
    bound.insert("ledger_balance".into(), json!(1_000.0));

    let first = pay_out(
        &reg, &d, &mut l, &mut nonces, "treasury", &state, "treasury.grant", &asked,
        &token_for("treasury.grant", &bound, 7), 1_000,
    );
    assert!(first.paid());

    let after = l.clone();
    let replay = pay_out(
        &reg, &d, &mut l, &mut nonces, "treasury", &state, "treasury.grant", &asked,
        &token_for("treasury.grant", &bound, 7), 1_000,
    );
    assert!(!replay.paid(), "the nonce was already spent");
    assert_eq!(l, after, "at-least-once delivery must not become pay-twice");
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-spec-over-a-reference-that-has-only-an-id");

    let hb = doc["★★★_the_hard_boundary_still_holds_and_by_the_same_device_as_PAWA_5"]
        .as_str()
        .unwrap();
    assert!(hb.contains("CANNOT MINT"));
    assert!(hb.contains("no juul→real path"));

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_no_new_machinery_and_that_is_the_ROW",
        "★★★_2_ONE_TRUTH_for_the_juul_and_how_the_state_relates_to_the_ledger",
        "★★_3_the_reserve_floor_is_an_ORDINARY_INVARIANT_not_a_bespoke_check",
        "★_4_φ_p_and_the_periodic_budget_are_DECLARED_PARAMETERS",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    assert!(step0["★★★_1_no_new_machinery_and_that_is_the_ROW"]
        .as_str()
        .unwrap()
        .contains("THE RECURSION IS THE SUBSTANCE"));
    let one_truth = step0["★★★_2_ONE_TRUTH_for_the_juul_and_how_the_state_relates_to_the_ledger"]
        .as_str()
        .unwrap();
    assert!(one_truth.contains("SAME-CALL PROJECTION"));
    assert!(one_truth.contains("ONLY IF THE GATE ADMITTED"));

    let split = d["★★_what_is_at_parity_and_what_is_the_sharpening"].as_object().unwrap();
    assert_eq!(split.len(), 4);
    assert!(split.keys().any(|k| k.contains("at parity — the treasury as a named recipient")));

    // ★★★ The finding, and the reason it was fixed at the root.
    let finding = d["★★★_a_real_finding_a_TEST_caught_not_inspection"].as_str().unwrap();
    assert!(finding.contains("REFUSES EVERYTHING"));
    assert!(finding.contains("A DECLARED CAP IMPLIES THE CATEGORY EXISTS"));
    assert!(finding.contains("silently never fire"), "and why leniency was the wrong fix");

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "NOTHING CHECKS THAT THE DEFINITION AND THE OPENING STATE AGREE",
        "THE PERIODIC BUDGET IS A DECLARED NUMBER, NOT A MECHANISM",
        "`φ_p` IS DECLARED AND UNCONSUMED",
        "A GRANT IS A TRANSFER, NOT A MINT",
        "THE TREASURY IS NOT YET COMPOSED",
        "`pay_out` TAKES THE TOKEN AS AN ARGUMENT",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in [
        "no_new_machinery_cases",
        "viable_region_cases",
        "conservation_cases",
        "human_gate_cases",
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
    assert_eq!(seen.len(), 12, "every declared case must be present");
}
