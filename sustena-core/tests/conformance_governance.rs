//! Governance, replayed against its conformance vectors
//! (Pawa §6.5 · PAWA-11).
//!
//! ★★★ The parameters are a **declared Sustain** and a change is an **ordinary
//! Enzyme** — same template as PAWA-6's treasury, and for the same reason: no
//! bespoke engine. Driven entirely through the crate's public surface.
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
    governance::{
        declared_parameters, definition, enforcement, opening_state, register, ParameterSpec,
        Parameters, KAPPA_COMPUTE, KAPPA_STORAGE,
    },
    operator::{execute_admitted, Authorization, Execution, Registry},
    semantic::{replay_under, CallOutcome, EnzymeCall, ReplayMode},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("governance.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "governance.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn registry() -> Registry {
    let mut r = Registry::default();
    register(&mut r);
    r
}

fn sigma_gov() -> Definition {
    definition(&declared_parameters())
}

fn opening() -> Value {
    opening_state(&declared_parameters())
}

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn token_for(p: &Map<String, Value>, nonce: u64) -> ApprovalToken {
    Simulated::from_sandbox("gov-1", Binding::new("governance.set_parameter", p), Ok(()))
        .expect("the sandbox committed")
        .voted(ProposalStatus::Passed)
        .expect("passed")
        .approve("bonnie", nonce, 9_999)
}

fn change(state: &Value, name: &str, value: f64, nonce: u64) -> Execution {
    let d = sigma_gov();
    let p = params(&[("name", json!(name)), ("value", json!(value))]);
    let token = token_for(&p, nonce);
    execute_admitted(
        &registry(),
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

// ── ★★ no bespoke engine ─────────────────────────────────────────────────────

#[test]
fn a_parameter_change_is_an_ordinary_enzyme_with_ordinary_bounds() {
    let d = sigma_gov();
    let reg = registry();
    let meta = reg.get("governance.set_parameter").expect("registered like any operator");
    assert!(!meta.params.is_empty(), "it declares its shape like any other");
    assert_eq!(d.invariants.len(), declared_parameters().len() * 2, "a min and a max each");
    assert!(d.invariants.iter().any(|(id, _)| id == "kappa_compute_min"));
    assert!(d.invariants.iter().any(|(id, _)| id == "kappa_storage_max"));
}

#[test]
fn a_valid_change_commits_and_records_the_event() {
    let x = change(&opening(), KAPPA_COMPUTE, 2.5, 1);
    assert!(x.committed(), "{:?}", x.result.reason);
    assert_eq!(x.state["parameters"][KAPPA_COMPUTE], json!(2.5));
    assert_eq!(x.events.len(), 1);
    assert_eq!(x.events[0].name, "event.governance.parameter_changed");
    assert_eq!(x.events[0].payload["from"], json!(1.0), "the event says what moved");
    assert_eq!(x.events[0].payload["to"], json!(2.5));
}

#[test]
fn a_parameters_value_is_the_fold_of_its_changes_over_genesis() {
    // ★★★ `replay_under` takes a `Definition` and knows nothing about
    // governance, yet reconstructs a parameter's whole history.
    let calls: Vec<EnzymeCall> = [2.0, 3.0, 4.5]
        .iter()
        .enumerate()
        .map(|(i, v)| {
            EnzymeCall::new(format!("c{i}"), "governance.set_parameter")
                .with("name", json!(KAPPA_COMPUTE))
                .with("value", json!(v))
        })
        .collect();
    let out = replay_under(&calls, &registry(), &sigma_gov(), &opening(), ReplayMode::Replay);
    assert!(
        out.steps.iter().all(|s| matches!(s, CallOutcome::Admitted { .. })),
        "{:?}",
        out.steps
    );
    assert_eq!(out.state["parameters"][KAPPA_COMPUTE], json!(4.5), "the last change wins");
    assert_eq!(out.steps.len(), 3, "and every change is in the history");
}

// ── ★★ gated ─────────────────────────────────────────────────────────────────

#[test]
fn an_untokened_parameter_change_is_refused() {
    let d = sigma_gov();
    let asked = params(&[("name", json!(KAPPA_COMPUTE)), ("value", json!(2.5))]);
    let wrong = token_for(&params(&[("name", json!(KAPPA_STORAGE))]), 9);
    let x = execute_admitted(
        &registry(),
        &d.operators,
        &enforcement(&d),
        &opening(),
        "governance.set_parameter",
        &asked,
        &Authorization::Unchecked,
        &EffectClass::Live { token: &wrong, now: 1_000 },
        &mut NonceLedger::new(),
    );
    assert!(!x.committed());
    assert_eq!(x.result.constraint_violated.as_deref(), Some("approval_token"));
    assert_eq!(x.state, opening(), "and nothing changed");
}

#[test]
fn a_change_outside_the_declared_bound_is_refused_by_the_ordinary_gate() {
    for bad in [-1.0, 10_000.0] {
        let x = change(&opening(), KAPPA_COMPUTE, bad, 2);
        assert!(!x.committed(), "{bad} should be out of range");
        assert_eq!(
            x.result.constraint_violated.as_deref(),
            Some("enforcement_gate"),
            "the SAME reason a household breach gives"
        );
        assert_eq!(x.state, opening());
    }
}

#[test]
fn an_undeclared_parameter_cannot_be_invented() {
    let x = change(&opening(), "kappa_mystery", 5.0, 3);
    assert!(!x.committed());
    assert_eq!(x.result.constraint_violated.as_deref(), Some("parameter_declared"));
}

#[test]
fn a_spec_whose_genesis_is_outside_its_own_bounds_is_refused() {
    assert!(ParameterSpec::declared("x", 5.0, 0.0, 1.0).is_none());
    assert!(ParameterSpec::declared("x", 0.5, 1.0, 0.0).is_none(), "and min > max");
}

// ── ★★★ one truth ────────────────────────────────────────────────────────────

#[test]
fn the_constants_are_genesis_values_and_the_governed_value_is_what_prices() {
    // ★★★ `pawa::compute_pawa` is gone; pricing goes through `Parameters`.
    let genesis = Parameters::genesis();
    assert_eq!(genesis.price(7.0, 542.0), 12.42, "the genesis pricing, unchanged from PAWA-1");

    let changed = change(&opening(), KAPPA_COMPUTE, 2.0, 4);
    assert!(changed.committed());
    let live = Parameters::read(&changed.state);
    assert_eq!(live.kappa_compute(), 2.0);
    assert_eq!(live.price(7.0, 542.0), 2.0 * 7.0 + 0.01 * 542.0, "priced by the GOVERNED κ");
    assert_ne!(live.price(7.0, 542.0), genesis.price(7.0, 542.0), "not shadowed by the constant");
}

#[test]
fn an_ungoverned_state_reads_the_declared_genesis_values() {
    assert_eq!(Parameters::read(&json!({})), Parameters::genesis());
    assert_eq!(Parameters::read(&opening()), Parameters::genesis());
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-spec-over-ungoverned-constants");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_PAWA_6's_treasury_is_the_TEMPLATE_and_it_is_reused_not_re_derived",
        "★★★_2_the_CONST_reconcile_and_what_it_forced",
        "★★_3_what_this_row_governs_NOW_and_what_it_names_as_follow_on",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    assert!(step0["★★★_1_PAWA_6's_treasury_is_the_TEMPLATE_and_it_is_reused_not_re_derived"]
        .as_str()
        .unwrap()
        .contains("NO BESPOKE GOVERNANCE ENGINE EXISTS"));

    // ★★★ The one-truth reconcile must keep both halves: the demotion AND the deletion.
    let consts = step0["★★★_2_the_CONST_reconcile_and_what_it_forced"].as_str().unwrap();
    assert!(consts.contains("DEMOTED"), "the constants' new status must stay named");
    assert!(consts.contains("WAS REMOVED OUTRIGHT"), "and the deletion of compute_pawa");
    assert!(consts.contains("NO PATH LEFT THAT PRICES FROM A CONSTANT"));

    // ★★ And the coverage must stay honest about what is NOT governed, with the reason.
    let scope = step0["★★_3_what_this_row_governs_NOW_and_what_it_names_as_follow_on"]
        .as_str()
        .unwrap();
    assert!(scope.contains("GOVERNED NOW"));
    assert!(scope.contains("NAMED AS FOLLOW-ON"));
    assert!(scope.contains("SUM TO 100"), "the reason the schedule is not governed yet");

    let split = d["★★_what_is_at_parity_and_what_is_the_sharpening"].as_object().unwrap();
    assert_eq!(split.len(), 4);
    assert!(split.keys().any(|k| k.contains("at parity")));
    assert!(split
        .values()
        .any(|v| v.as_str().unwrap_or_default().contains("enforced by DELETION")
            || v.as_str().unwrap_or_default().contains("is gone")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "ONLY TWO PARAMETERS ARE GOVERNED",
        "A PARAMETER IS A SCALAR",
        "NOTHING CONNECTS A CHANGE TO A COUNCIL",
        "NO CHANGE TAKES EFFECT AT A FUTURE TIME",
        "`Parameters::read` FALLS BACK TO GENESIS",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["no_bespoke_engine_cases", "gated_cases", "one_truth_cases"] {
        for c in doc[group].as_array().unwrap_or_else(|| panic!("{group} is an array")) {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 9, "every declared case must be present");
}
