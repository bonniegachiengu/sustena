//! `ℐ : ε → (o, θ)`, replayed against its conformance vectors
//! (R2 · Curated UI §VIII · follow-on wiring 3).
//!
//! ★★ The **last** of the three follow-on wirings. Reference-less: the
//! reference's own `ℐ` is app-layer by the ING rows' decision, so there is no
//! parity half. The counterweight is the **two named residuals** — UI-11's
//! *named core slice* and EVT-15's un-produced `EnzymeCall` — which between
//! them named this consumer, its inputs and its rules.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    editing::Definition,
    enzyme::{propose_call, CallProposal, Effect},
    operator::Registry,
    schema::{DimType, Schema},
    semantic::{replay_under, CallOutcome, ReplayMode},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("enzyme.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "enzyme.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn registry() -> Registry {
    Registry::default()
}

fn definition() -> Definition {
    Definition::new(
        Schema::new()
            .declare("finances.liquid.balance", DimType::Any)
            .declare("finances.pockets", DimType::Any),
    )
    .with_operator("budget.record_income")
    .with_operator("budget.allocate")
    .with_operator("budget.spend")
}

fn state() -> Value {
    json!({"finances": {
        "liquid": {"balance": 500.0},
        "pockets": {
            "food": {"allocated": 100.0, "spent": 20.0, "limit": 0.0},
            "rent": {"allocated": 200.0, "spent": 0.0, "limit": 0.0}
        },
        "income": {"monthly_total": 0.0, "sources": []}
    }})
}

fn from(text: &str) -> CallProposal {
    propose_call("c1", &Effect::described(text), &definition(), &registry(), &state())
}

// ── ε → a correct (o, θ) ─────────────────────────────────────────────────────

#[test]
fn a_described_effect_becomes_a_candidate_over_the_real_registry() {
    let p = from("spend 50 from food");
    let ready = p.ready().unwrap_or_else(|| panic!("{}", p.describe()));
    assert_eq!(ready.call.operator, "budget.spend");
    assert_eq!(ready.call.params["pocket_name"], json!("food"));
    assert_eq!(ready.call.params["amount"], json!(50.0));
    assert!(ready.from_history.is_empty(), "nothing here was remembered");
}

#[test]
fn the_pocket_is_matched_against_this_states_own_keys() {
    // ★★ The core never learns what a pocket is: the parameter declares the
    // path, the state supplies the keys.
    assert_eq!(from("spend 30 from rent").ready().unwrap().call.params["pocket_name"], json!("rent"));
}

#[test]
fn a_proposal_never_carries_a_field_the_operator_does_not_declare() {
    // ★★★ UI-11's core property, surviving the declared-instead-of-introspected
    // substitution intact.
    let p = from("allocate 40 to food");
    let ready = p.ready().unwrap_or_else(|| panic!("{}", p.describe()));
    let reg = registry();
    let declared: BTreeSet<&str> =
        reg.get("budget.allocate").unwrap().params.iter().map(|d| d.name.as_ref()).collect();
    assert!(!declared.is_empty(), "the operator declares a shape at all");
    assert!(ready.call.params.keys().all(|k| declared.contains(k.as_str())));
}

// ── ambiguity is a named gap, never a guess ──────────────────────────────────

#[test]
fn an_unnamed_pocket_is_a_named_gap_with_the_real_options() {
    match from("spend 50 at naivas") {
        CallProposal::NeedsDisambiguation { field, options, .. } => {
            assert_eq!(field, "pocket_name");
            assert_eq!(options, vec!["food".to_string(), "rent".to_string()]);
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn a_missing_amount_is_a_gap_with_no_fabricated_options() {
    match from("spend from food") {
        CallProposal::NeedsDisambiguation { field, options, .. } => {
            assert_eq!(field, "amount");
            assert!(options.is_empty(), "an amount is not a tap-list");
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn a_description_matching_nothing_asks_rather_than_reporting_no_match() {
    // ★★★ UI-11's rule verbatim: a hint that eliminates EVERY candidate is
    // treated as NO hint.
    match from("something happened") {
        CallProposal::NeedsDisambiguation { field, options, .. } => {
            assert_eq!(field, "operator");
            assert_eq!(options.len(), 3, "the full candidate set, not zero");
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn the_operators_own_prose_is_not_a_matching_vocabulary() {
    // ★★ The regression a test caught: matching on the description made the
    // stopword "from" an operator hint, so "spend ... from ..." also matched
    // `budget.allocate`. The name is the contract; the prose is not.
    let p = from("spend 50 from food");
    assert_eq!(p.ready().unwrap().call.operator, "budget.spend");
    assert!(
        registry().get("budget.allocate").unwrap().description.contains("from"),
        "the trap is still present in the prose, so this test still means something"
    );
}

// ── rejection, not fabrication ───────────────────────────────────────────────

#[test]
fn an_operator_outside_the_definition_is_rejected() {
    let e = Effect::described("do it").knowing("operator", json!("budget.add_pocket"));
    match propose_call("c1", &e, &definition(), &registry(), &state()) {
        CallProposal::Rejected { reason } => assert!(reason.contains("budget.add_pocket")),
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn a_remembered_pocket_that_no_longer_exists_is_rejected_not_proposed() {
    // ★ History pre-fill is not a bypass.
    let e = Effect::described("spend 50")
        .knowing("operator", json!("budget.spend"))
        .knowing("pocket_name", json!("holiday"));
    match propose_call("c1", &e, &definition(), &registry(), &state()) {
        CallProposal::Rejected { reason } => assert!(reason.contains("holiday")),
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn an_operator_with_no_declared_params_cannot_have_a_theta_invented_for_it() {
    let d = Definition::new(Schema::new()).with_operator("test.force_negative");
    match propose_call("c1", &Effect::described("force it"), &d, &registry(), &state()) {
        CallProposal::CannotInfer { reason } => assert!(reason.contains("declares no parameters")),
        other => panic!("{}", other.describe()),
    }
}

// ── the output is a candidate, and EnzymeCall's producer ─────────────────────

#[test]
fn a_remembered_value_is_reported_as_remembered() {
    let e = Effect::described("spend 50").knowing("pocket_name", json!("food"));
    let p = propose_call("c1", &e, &definition(), &registry(), &state());
    let ready = p.ready().unwrap_or_else(|| panic!("{}", p.describe()));
    assert!(ready.from_history.contains("pocket_name"), "remembered");
    assert!(!ready.from_history.contains("amount"), "inferred");
}

#[test]
fn proposing_runs_nothing_and_leaves_the_state_alone() {
    let s = state();
    let _ = propose_call(
        "c1",
        &Effect::described("spend 50 from food"),
        &definition(),
        &registry(),
        &s,
    );
    assert_eq!(s, state(), "a candidate, never an execution");
}

#[test]
fn a_proposed_call_is_exactly_what_evt_15s_replay_consumes() {
    // ★★★ THE DISCHARGE: `EnzymeCall` finally has a producer, and
    // `replay_under` finally has something it was not handed by hand.
    let call = from("spend 50 from food").ready().unwrap().call.clone();
    let outcome =
        replay_under(&[call], &registry(), &definition(), &state(), ReplayMode::Replay);
    assert!(
        matches!(outcome.steps.first(), Some(CallOutcome::Admitted { operator, .. }) if operator == "budget.spend"),
        "{:?}",
        outcome.steps
    );
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-internal-wiring");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_the_two_residuals_ARE_one_consumer_and_UI_11's_row_says_so",
        "★★★_2_but_the_discharge_is_SCOPED_HONESTLY_because_EVT_15_wanted_a_DIRECTION_this_does_not_build",
        "★★★_3_θ_IS_DECLARED_HERE_WHERE_THE_REFERENCE_INTROSPECTS_and_that_is_a_real_difference",
        "★★★_4_pocket_matching_GENERALISED_rather_than_ported",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★★ The honest half of the discharge must stay stated.
    let scoped = step0
        ["★★★_2_but_the_discharge_is_SCOPED_HONESTLY_because_EVT_15_wanted_a_DIRECTION_this_does_not_build"]
        .as_str()
        .unwrap();
    assert!(scoped.contains("RECORDS"), "the direction EVT-15 asked for must stay named");
    assert!(scoped.contains("HOST STATE"), "and why it is not built here");

    // ★★★ UI-11's condition must stay quoted where it was made.
    assert!(step0["★★★_1_the_two_residuals_ARE_one_consumer_and_UI_11's_row_says_so"]
        .as_str()
        .unwrap()
        .contains("WHEN A RUST-SIDE CALLER EXISTS"));

    let cw = d["★★★_the_counterweight_is_the_two_named_residuals"].as_str().unwrap();
    assert!(cw.contains("named the caller"), "the counterweight names what it had to build");
    assert!(cw.contains("NEVER SILENTLY"), "and the rule it had to keep");
    assert!(cw.contains("written in advance"), "and why it is stronger than a grep");

    let reuse = d["★★_what_this_slice_REUSES_rather_than_reinvents"].as_object().unwrap();
    for k in ["the output", "the operator set", "the parameter shape", "the live values"] {
        assert!(reuse.contains_key(k), "no reuse statement for {k}");
    }
    assert!(reuse["the output"].as_str().unwrap().contains("EnzymeCall"));

    let flaw = d["★★_a_real_design_flaw_a_TEST_caught_not_inspection"].as_str().unwrap();
    assert!(flaw.contains("\"FROM\"") || flaw.contains("'FROM'"));
    assert!(flaw.contains("THE NAME IS THE CONTRACT"));

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("propose (")));
    assert!(fp.keys().any(|k| k.starts_with("Proposal (")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE APP-LAYER FEED STAYS APP-LAYER",
        "EVT-15's RECORDER HALF IS NOT BUILT",
        "NUMBER EXTRACTION IS DELIBERATELY DULL",
        "OPERATOR NARROWING IS EXACT-TOKEN, NOT FUZZY",
        "ONE QUESTION AT A TIME",
        "NOTHING HERE COMMITS",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["inference_cases", "gap_cases", "rejection_cases", "candidate_cases"] {
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
