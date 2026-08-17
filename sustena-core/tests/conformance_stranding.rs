//! States vs histories, and the three-outcome decision, replayed against their
//! conformance vectors (R2 · Editing §I–III · EDIT-11, consuming EVT-15).
//!
//! **SPEC vectors, Rust-only on the decision.** ★★ The mechanism was finished
//! before this slice opened, twice over: `editing::safe` is the migration
//! predicate over live STATES (2026-08-12), `semantic::replay_under` is the
//! reducer over HISTORIES (EVT-15). What was missing is the wiring and the
//! choice — migrate / grandfather / refuse, offered with evidence.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    editing::{Definition, Instance, Migration},
    migrate::Mu,
    operator::Registry,
    schema::{DimType, Schema},
    stranding::{assess_edit, Remedy},
    CallOutcome, EnzymeCall, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("stranding.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "stranding.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn base() -> Definition {
    Definition::new(Schema::new().declare("finances", DimType::Any))
        .with_operator("budget.record_income")
}

/// `D′` — a floor the endpoint clears and the journey did not.
fn with_floor() -> Definition {
    base().with_invariant("floor", "finances.liquid.balance >= 500")
}

fn genesis() -> Value {
    json!({"finances": {
        "liquid": {"balance": 0.0},
        "income": {"monthly_total": 0.0, "sources": []},
        "pockets": {}
    }})
}

fn income(id: &str, amount: f64) -> EnzymeCall {
    EnzymeCall::new(id, "budget.record_income")
        .with("amount", json!(amount))
        .with("source", json!("wages"))
        .with("entry_id", json!(id))
}

/// 0 → 300 → 1000. The dip is the whole point.
fn dipping() -> sustena_core::InstanceHistory {
    sustena_core::InstanceHistory::new("i1", genesis())
        .then(income("c1", 300.0))
        .then(income("c2", 700.0))
}

fn at(balance: f64) -> Value {
    json!({"finances": {"liquid": {"balance": balance}}})
}

fn instance(balance: f64) -> Vec<Instance> {
    vec![Instance { id: "i1".into(), state: at(balance) }]
}

fn kinds(remedies: &[Remedy]) -> Vec<&'static str> {
    remedies.iter().map(Remedy::kind).collect()
}

// ── the distinction ──────────────────────────────────────────────────────────

#[test]
fn a_history_is_refused_that_the_current_state_passes() {
    // ★★ THE CRUX. 1000 clears a >= 500 floor as a state; the path through 300
    // does not. The state-only check cannot see this, by construction.
    let reg = Registry::default();
    let impact = assess_edit(
        &with_floor(),
        &instance(1000.0),
        &[dipping()],
        &reg,
        &Migration::Identity,
    );

    assert!(impact.state_stranded.is_empty(), "viable under D′ as it stands");
    assert_eq!(impact.history_stranded.len(), 1);
    assert!(impact.histories_only());

    match &impact.history_stranded[0].outcome {
        CallOutcome::Refused { id, reason, .. } => {
            assert_eq!(id, "c1", "the call that dipped, not the one that recovered");
            assert!(reason.contains("floor"), "the rule is named: {reason}");
        }
        other => panic!("expected a gate refusal, got {other:?}"),
    }
}

#[test]
fn a_history_that_never_dipped_is_clear() {
    let reg = Registry::default();
    let straight = sustena_core::InstanceHistory::new("i1", genesis()).then(income("c1", 1000.0));
    let impact = assess_edit(
        &with_floor(),
        &instance(1000.0),
        &[straight],
        &reg,
        &Migration::Identity,
    );

    assert!(impact.is_clear());
    assert!(
        impact.remedies(&Migration::Identity).is_empty(),
        "offering `Refuse` for a safe edit would be noise dressed as caution"
    );
}

#[test]
fn both_lists_are_filled_and_neither_short_circuits() {
    let reg = Registry::default();
    let impact = assess_edit(
        &with_floor(),
        &instance(100.0),
        &[dipping()],
        &reg,
        &Migration::Identity,
    );

    assert!(!impact.state_stranded.is_empty(), "state scan ran");
    assert!(!impact.history_stranded.is_empty(), "history scan ran too");
    assert_eq!(impact.affected_instances(), vec!["i1".to_string()]);
}

#[test]
fn a_dropped_operator_is_not_permitted_not_refused() {
    let reg = Registry::default();
    let dropped = Definition::new(Schema::new().declare("finances", DimType::Any));
    let impact = assess_edit(&dropped, &[], &[dipping()], &reg, &Migration::Identity);

    assert_eq!(impact.history_stranded.len(), 2);
    assert!(
        impact
            .history_stranded
            .iter()
            .all(|h| matches!(h.outcome, CallOutcome::NotPermitted { .. })),
        "EVT-15's split must survive the wiring or it was decorative"
    );
}

#[test]
fn assessing_leaves_the_instance_state_untouched() {
    let reg = Registry::default();
    let instances = instance(1000.0);
    let before = instances[0].state.clone();
    let _ = assess_edit(&with_floor(), &instances, &[dipping()], &reg, &Migration::Identity);
    assert_eq!(instances[0].state, before);
}

// ── the decision ─────────────────────────────────────────────────────────────

#[test]
fn grandfather_is_offered_when_only_histories_are_stranded() {
    let reg = Registry::default();
    let impact = assess_edit(
        &with_floor(),
        &instance(1000.0),
        &[dipping()],
        &reg,
        &Migration::Identity,
    );
    assert_eq!(kinds(&impact.remedies(&Migration::Identity)), vec!["grandfather", "refuse"]);
}

#[test]
fn grandfather_is_withheld_while_the_state_itself_is_stranded() {
    // ★★ Soundness, not policy: scoping D′ to new actions cannot help an
    // instance already sitting somewhere D′ forbids — the next call refuses
    // regardless, so the remedy would not remedy anything.
    let reg = Registry::default();
    let impact = assess_edit(
        &with_floor(),
        &instance(100.0),
        &[dipping()],
        &reg,
        &Migration::Identity,
    );
    assert_eq!(
        kinds(&impact.remedies(&Migration::Identity)),
        vec!["refuse"],
        "and refuse being the ONLY option is exactly what a default would hide"
    );
}

#[test]
fn migration_never_clears_a_stranded_history() {
    // ★★ The distinction showing up in the remedies rather than the checks.
    let reg = Registry::default();
    let mu = Migration::Apply(Mu::new());
    let impact = assess_edit(&with_floor(), &instance(1000.0), &[dipping()], &reg, &mu);

    let remedies = impact.remedies(&mu);
    let migrate = remedies
        .iter()
        .find(|r| r.kind() == "migrate")
        .expect("a declared μ offers migration");

    match migrate {
        Remedy::Migrate { unresolved_states, unresolved_histories } => {
            assert!(unresolved_states.is_empty(), "μ leaves the states fine here");
            assert_eq!(unresolved_histories.len(), 1, "and cannot touch the past");
        }
        other => panic!("{other:?}"),
    }
    assert!(migrate.leaves_residue(), "a Migrate that read as 'fixed' would lie");
}

#[test]
fn migration_is_not_offered_when_none_was_declared() {
    let reg = Registry::default();
    let impact = assess_edit(&with_floor(), &instance(100.0), &[], &reg, &Migration::Identity);

    assert!(impact.states_only());
    assert_eq!(kinds(&impact.remedies(&Migration::Identity)), vec!["refuse"]);
}

#[test]
fn there_is_no_single_recommended_remedy() {
    // Structural: `remedies()` is the only accessor, it returns a Vec, and
    // `Remedy` implements no Default. Nothing here collapses the choice.
    let reg = Registry::default();
    let impact = assess_edit(
        &with_floor(),
        &instance(1000.0),
        &[dipping()],
        &reg,
        &Migration::Identity,
    );
    assert!(impact.remedies(&Migration::Identity).len() > 1);
}

#[test]
fn refuse_is_always_available_and_never_leaves_residue() {
    let reg = Registry::default();
    let mu = Migration::Apply(Mu::new());

    for (balance, migration) in
        [(1000.0, &Migration::Identity), (100.0, &Migration::Identity), (1000.0, &mu)]
    {
        let impact = assess_edit(&with_floor(), &instance(balance), &[dipping()], &reg, migration);
        let remedies = impact.remedies(migration);
        let refuse = remedies
            .iter()
            .find(|r| r.kind() == "refuse")
            .expect("do-not-do-this must never become unavailable");
        assert!(!refuse.leaves_residue());
    }
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The STEP-0 reconcile must stay on the record: the mechanism was already
    // there twice over, and rebuilding it would have been the real failure.
    let step0 = &doc["★★_the_STEP_0_reconcile_the_IMM_7_LESSON_APPLIED_AGAIN"];
    for key in [
        "1_the_state_half_is_BUILT_and_it_is_complete",
        "2_the_history_MECHANISM_is_BUILT_TOO",
        "3_and_the_REST_of_the_editing_engine_was_already_there",
        "★★_4_so_what_was_ACTUALLY_missing",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }

    // The crux, and the fact that it recurs in the remedies.
    let crux = &doc["★★_the_crux_states_are_not_histories"];
    assert!(crux["statement"].as_str().unwrap().contains("perfectly valid state"));
    assert!(crux["★_and_it_shows_up_a_SECOND_time_in_the_remedies"]
        .as_str()
        .unwrap()
        .contains("cannot reach back"));

    // ★★ The counterweight, including the reference naming the gap itself.
    let cw = d["★★_the_counterweight_is_STRONG_and_the_reference_NAMES_THIS_GAP_ABOUT_ITSELF"]
        .as_str()
        .unwrap();
    assert!(cw.contains("fail-closed"), "the reference's own rigour must stay credited");
    assert!(cw.contains("blocked_by"), "the reference had the witness set first");
    assert!(
        cw.contains("can only say no"),
        "the reference's self-description of this gap must stay quoted"
    );

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(
        fp.keys().any(|k| k.contains("check_definition_edit_safety (2 hits")),
        "the two-hits-is-not-a-stub note must stay named"
    );
    assert!(fp.keys().any(|k| k.contains("grep-0")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE HISTORY CHECK CANNOT SEE μ OR F",
        "NOTHING VERIFIES THAT A SUPPLIED HISTORY IS THE INSTANCE'S OWN",
        "THE REMEDIES ARE OFFERED, NOT APPLIED",
        "GRANDFATHERING HAS NO ENFORCEMENT ARM HERE",
        "A HISTORY IS REPLAYED FROM ITS DECLARED GENESIS",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["distinction_cases", "decision_cases"] {
        for c in doc[group].as_array().unwrap() {
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
