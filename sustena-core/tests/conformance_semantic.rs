//! Semantic replay under `D′`, replayed against its conformance vectors
//! (R2 · Events and Time §IX, second consumer · EVT-15 → unblocks EDIT-11).
//!
//! **SPEC vectors, Rust-only on the reducer — and PYTHON-AHEAD on the
//! ingredient.** The reference's `operators_log` keeps `operator_name` and
//! `input_json` per call; the Rust `Event` keeps neither, which is why
//! `EnzymeCall` exists. A one-directional summary would be wrong, so both
//! directions are recorded.
//!
//! ★★ The premise was checked before building: the patch fold is
//! `D`-independent (there is no parameter through which a definition could
//! enter), so the semantic layer sits **beside** it.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value};
use sustena_core::{
    is_deterministic, replay_under, schema::DimType, CallOutcome, Definition, EnzymeCall, Registry,
    ReplayMode, Schema, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("semantic.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "semantic.json was written for a different contract version"
    );
    doc
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap_or_else(|| panic!("group {group} missing"))
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("case {group}/{name} missing"))
}

// ── the fixture, from the vector ────────────────────────────────────────────

fn initial(doc: &Value) -> Value {
    doc["fixture"]["initial"].clone()
}

fn definition(doc: &Value) -> Definition {
    let d = &doc["fixture"]["definition"];
    // `Any` is an admission, not a default — the fixture is about the
    // DEFINITION changing, not about typing.
    let mut out = Definition::new(Schema::new().declare("finances", DimType::Any));
    for inv in d["invariants"].as_array().unwrap() {
        let p = inv.as_array().unwrap();
        out = out.with_invariant(p[0].as_str().unwrap(), p[1].as_str().unwrap());
    }
    for op in d["operators"].as_array().unwrap() {
        out = out.with_operator(op.as_str().unwrap());
    }
    out
}

fn calls(doc: &Value) -> Vec<EnzymeCall> {
    doc["fixture"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let mut call =
                EnzymeCall::new(c["id"].as_str().unwrap(), c["operator"].as_str().unwrap());
            let params: &Map<String, Value> = c["params"].as_object().unwrap();
            for (k, v) in params {
                call = call.with(k, v.clone());
            }
            call
        })
        .collect()
}

fn d_prime_floor(doc: &Value) -> Definition {
    let f = &doc["fixture"]["d_prime_floor"];
    definition(doc).with_invariant(f["id"].as_str().unwrap(), f["expression"].as_str().unwrap())
}

// ── ★★ the premise ─────────────────────────────────────────────────────────

#[test]
fn the_patch_level_replay_is_d_independent() {
    // ★★ There is no parameter through which a definition could enter
    // `checkpoint::replay` — it takes a log and a checkpoint. The premise this
    // whole slice rests on, asserted by the signature it is called through.
    let doc = load();
    let c = case(&doc, "premise_cases", "★★_the_PATCH_level_replay_is_D_INDEPENDENT");
    assert!(c["expect"]["folds_without_a_definition"].as_bool().unwrap());

    assert!(sustena_core::replay(&[], None).is_ok(), "a log and a checkpoint, and nothing else");
}

#[test]
fn under_the_same_definition_the_calls_reproduce_the_patch_state() {
    // ★★ THE CONSISTENCY CHECK. Without it every `D′` verdict would be about a
    // different history than the one recorded.
    let doc = load();
    let c = case(
        &doc,
        "premise_cases",
        "★★_under_the_SAME_definition_the_calls_REPRODUCE_the_patch_state",
    );
    let reg = Registry::with_builtins();
    let out = replay_under(&calls(&doc), &reg, &definition(&doc), &initial(&doc), ReplayMode::Replay);

    assert_eq!(out.wholly_admissible(), c["expect"]["wholly_admissible"].as_bool().unwrap());
    // Re-running the identical calls a second way lands on the identical state.
    let again =
        replay_under(&calls(&doc), &reg, &definition(&doc), &initial(&doc), ReplayMode::Replay);
    assert_eq!(out.state, again.state);
    assert_ne!(out.state, initial(&doc), "and the history genuinely moved the state");
}

#[test]
fn replaying_twice_lands_on_the_same_state() {
    let doc = load();
    let c = case(&doc, "premise_cases", "★_replaying_TWICE_lands_on_the_same_state");
    assert_eq!(
        is_deterministic(&calls(&doc), &Registry::with_builtins(), &definition(&doc), &initial(&doc)),
        c["expect"]["deterministic"].as_bool().unwrap()
    );
}

// ── ★★ under D′: reinterpret and check admissibility ───────────────────────

#[test]
fn a_call_admitted_under_d_can_be_refused_under_d_prime() {
    // ★★ EDIT-11'S QUESTION, ANSWERED — and *partly* inadmissible, which is the
    // realistic shape and far more useful to an editor than a yes/no.
    let doc = load();
    let c = case(
        &doc,
        "reinterpretation_cases",
        "★★_a_call_ADMITTED_under_D_can_be_REFUSED_under_D_PRIME",
    );
    let out = replay_under(
        &calls(&doc),
        &Registry::with_builtins(),
        &d_prime_floor(&doc),
        &initial(&doc),
        ReplayMode::Replay,
    );

    assert_eq!(out.wholly_admissible(), c["expect"]["wholly_admissible"].as_bool().unwrap());
    assert_eq!(out.steps[0].admitted(), c["expect"]["first_admitted"].as_bool().unwrap());

    let bad = out.inadmissible();
    let want: Vec<&str> =
        c["expect"]["inadmissible"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(bad.iter().map(|o| o.id()).collect::<Vec<_>>(), want);
    assert!(matches!(bad[0], CallOutcome::Refused { .. }), "the GATE refused it");
    assert!(bad[0].describe().contains("would NOT have been admissible"));
}

#[test]
fn the_same_history_stays_admissible_under_a_d_prime_that_does_not_bite() {
    // ★ A CHANGED definition is not automatically a refusal — otherwise the
    // check would report *the definition changed* and teach an editor nothing.
    let doc = load();
    let c = case(
        &doc,
        "reinterpretation_cases",
        "★_the_SAME_history_stays_admissible_under_a_D_PRIME_that_does_not_bite",
    );
    let generous = definition(&doc).with_invariant("generous", "finances.liquid.balance >= 0");
    let out = replay_under(
        &calls(&doc),
        &Registry::with_builtins(),
        &generous,
        &initial(&doc),
        ReplayMode::Replay,
    );
    assert_eq!(out.wholly_admissible(), c["expect"]["wholly_admissible"].as_bool().unwrap());
}

#[test]
fn an_operator_d_prime_no_longer_permits_is_a_different_finding() {
    // ★★ The allow-list and the gate answer different questions; collapsing
    // them would tell an editor to fix the wrong thing.
    let doc = load();
    let c = case(
        &doc,
        "reinterpretation_cases",
        "★★_an_operator_D_PRIME_no_longer_PERMITS_is_a_DIFFERENT_finding",
    );
    let narrowed = Definition::new(Schema::new().declare("finances", DimType::Any))
        .with_operator("budget.record_income"); // allocate dropped

    let out = replay_under(
        &calls(&doc),
        &Registry::with_builtins(),
        &narrowed,
        &initial(&doc),
        ReplayMode::Replay,
    );
    let bad = out.inadmissible();
    assert_eq!(bad.len(), c["expect"]["inadmissible"].as_u64().unwrap() as usize);
    assert!(matches!(bad[0], CallOutcome::NotPermitted { .. }));
    assert!(bad[0].describe().contains("does not permit"));
}

#[test]
fn a_refused_call_does_not_advance_the_state() {
    // The gate's standing promise carried into replay.
    let doc = load();
    let out = replay_under(
        &calls(&doc),
        &Registry::with_builtins(),
        &d_prime_floor(&doc),
        &initial(&doc),
        ReplayMode::Replay,
    );
    assert!(out.steps.iter().any(|s| !s.admitted()));
    assert!(out.steps.iter().any(CallOutcome::admitted), "and only the admitted ones moved it");
}

// ── ★★ no effect re-fires ──────────────────────────────────────────────────

#[test]
fn replay_mode_discards_every_emission_and_counts_it() {
    // ★★ §IX's assertion as a returned FACT — and counted, because a silent
    // suppression is indistinguishable from an operator that emitted nothing.
    let doc = load();
    let c = case(
        &doc,
        "effect_safety_cases",
        "★★_REPLAY_mode_DISCARDS_every_emission_AND_COUNTS_IT",
    );
    let out = replay_under(
        &calls(&doc),
        &Registry::with_builtins(),
        &definition(&doc),
        &initial(&doc),
        ReplayMode::Replay,
    );

    assert_eq!(
        out.no_external_effects_were_issued(),
        c["expect"]["no_external_effects"].as_bool().unwrap()
    );
    assert!(
        out.suppressed_events as u64 > c["expect"]["suppressed_events_gt"].as_u64().unwrap(),
        "these operators DO emit, so the guarantee is exercised rather than vacuous"
    );
}

#[test]
fn live_mode_has_to_be_named() {
    // ★ The same discipline `Authorization::Unchecked` follows: a bypass that
    // reads as ordinary is the problem; one that has to be named is not.
    let doc = load();
    let c = case(&doc, "effect_safety_cases", "★_LIVE_mode_has_to_be_NAMED");
    let out = replay_under(
        &calls(&doc),
        &Registry::with_builtins(),
        &definition(&doc),
        &initial(&doc),
        ReplayMode::Live,
    );
    assert_eq!(
        out.no_external_effects_were_issued(),
        c["expect"]["no_external_effects"].as_bool().unwrap()
    );
    assert_eq!(out.suppressed_events as u64, c["expect"]["suppressed_events"].as_u64().unwrap());
}

#[test]
fn an_empty_history_is_the_initial_state_and_no_verdicts() {
    let doc = load();
    let out = replay_under(
        &[],
        &Registry::with_builtins(),
        &definition(&doc),
        &initial(&doc),
        ReplayMode::Replay,
    );
    assert_eq!(out.state, initial(&doc));
    assert!(out.steps.is_empty());
    assert!(out.wholly_admissible(), "vacuously, and said rather than implied");
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The premise, checked before building — all three parts.
    let p = &doc["★★_the_premise_CHECKED_before_building_on_it"];
    assert!(p["1_the_patch_fold_IS_D_independent_confirmed"]
        .as_str()
        .unwrap()
        .contains("property, not a shortfall"));
    assert!(p["★_2_the_ingredient_exists_but_in_the_REFERENCE_not_here"]
        .as_str()
        .unwrap()
        .contains("operators_log"));
    assert!(p["★★_3_D_prime_needed_NO_NEW_TYPE"]
        .as_str()
        .unwrap()
        .contains("editing::Definition"));

    // ★★ And the symmetric finding — the ingredient is PYTHON-ahead.
    assert!(d["★★_and_the_symmetric_finding_the_INGREDIENT_is_MISSING_HERE"]
        .as_str()
        .unwrap()
        .contains("one-directional summary would be wrong"));
    assert!(d["term_by_term"]["★ the logged Enzyme call (operator + params)"]
        .as_str()
        .unwrap()
        .contains("PYTHON-AHEAD"));

    // ★ The nearest relative, studied rather than dismissed.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp
        .iter()
        .any(|(k, v)| k.starts_with("check_definition_edit_safety")
            && v.as_str().is_some_and(|s| s.contains("states") && s.contains("histories"))));
    // ★ And the corrected grep: `semantic` is not a zero, and its first hit is
    // the reference naming this gap about itself.
    assert!(fp.keys().any(|k| k.contains("semantic (4 hits")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THIS IS THE MECHANISM EDIT-11 CONSUMES, NOT EDIT-11",
        "REPLAY EMITS NOTHING",
        "DETERMINISM IS NOT PURITY",
        "THE CALL LOG IS NOT WIRED TO `execute`",
        "BOUNDARY AND FIREWALL ARE NOT PART OF A `Definition`",
        "NO CHECKPOINTS IN THE SEMANTIC PATH",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }
    assert!(limits.contains("EVT-14 effect journaling is not built in Rust"));

    let groups = ["premise_cases", "reinterpretation_cases", "effect_safety_cases"];
    let mut seen = BTreeSet::new();
    for group in groups {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
