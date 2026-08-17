//! `M_self` and `M_world`, replayed against their conformance vectors
//! (R2 · Operative §VII · OPV-11).
//!
//! **Reference-less** — `M_self`, `M_world`, `conant` and `ashby` are all
//! grep-0 there. ★★ But most of `M_world`'s engine already existed **here**
//! (`tenet::TransitionModel`, `ensemble::ModelTemplate`), so this row is
//! largely a framing, and saying which parts are new is its substance.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value};
use sustena_core::{
    attention::{Aperture, Attention},
    ensemble::{ModelTemplate, Prob, Scenario},
    models::{Agreement, EffectiveN, Fidelity, Projection, SelfModel, WorldModel},
    operative::{Cynefin, Objective, Operative, Sense, Shared, Utility},
    region::{Interval, Region},
    schema::{DimType, Schema},
    strategy::StrategyGraph,
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("models.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "models.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn world() -> Shared {
    Shared::new().with_move("budget.allocate").with_move("budget.record_income")
}

fn fidelity() -> Fidelity {
    Fidelity::claimed(0.7, "declared from the household's own spec, unmeasured").unwrap()
}

fn model() -> WorldModel {
    WorldModel::declared(
        "household",
        &["household"],
        &["balance"],
        Region::new().bounding(Interval::new("balance", 0.0, 100.0)),
        ModelTemplate::new().certain("s0", "budget.allocate", "s1"),
        fidelity(),
    )
}

fn operative() -> Operative {
    let u = Utility::new()
        .with(Objective::new("thrift", "balance", Sense::Maximise))
        .unwrap();
    Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
}

fn strategy() -> StrategyGraph {
    StrategyGraph::new("a", "a")
        .with_node(&world(), "a", "budget.allocate", Map::new())
        .unwrap()
}

fn attention() -> Attention {
    Attention::declared(
        Aperture::declared(1, 3, 4).unwrap(),
        Aperture::declared(12, 1, 1).unwrap(),
        100,
    )
    .unwrap()
}

// ── M_self ───────────────────────────────────────────────────────────────────

#[test]
fn the_self_model_is_built_from_omegas_own_parts() {
    // ★★ Composed, not forked — so it cannot drift from what it models.
    let m = SelfModel::of(&operative(), &strategy(), &attention(), &["household"]);
    assert_eq!(m.operative_id(), "mentor");
    assert_eq!(m.objectives(), operative().utility().m());
    assert_eq!(
        m.moves(),
        &strategy().moves().into_iter().map(str::to_string).collect::<BTreeSet<_>>()
    );
    assert_eq!(m.attention_budget(), attention().cost());
}

#[test]
fn the_self_models_moves_are_bounded_by_t() {
    // ★★★ CELL's recursion: Π's moves came from Shared, so the inner Sustain
    // is itself bounded by the shared world.
    let m = SelfModel::of(&operative(), &strategy(), &attention(), &["household"]);
    assert!(m.moves_within(&world()));
}

// ── M_world ──────────────────────────────────────────────────────────────────

#[test]
fn the_structure_is_declared_and_only_parameters_move() {
    let m = model();
    assert!(m.structure_is_declared());
    let before = m.transitions().actions();
    let _ = m.transitions().instantiate(&Scenario::new("optimistic"));
    assert_eq!(m.transitions().actions(), before, "structure is not up for revision by data");
}

#[test]
fn a_model_may_disagree_with_the_world_and_the_disagreement_is_reported() {
    // ★ A report, not a guard.
    let m = WorldModel::declared(
        "fanciful",
        &["household"],
        &["balance"],
        Region::new(),
        ModelTemplate::new().certain("s0", "budget.teleport", "s1"),
        fidelity(),
    );
    let a = m.agrees_with(&world());
    assert!(a.imagined.contains("budget.teleport"), "the model is wrong");
    assert!(a.unmodelled.contains("budget.allocate"), "and it is blind");
    assert!(!a.exact());
}

#[test]
fn dimensions_outside_the_model_are_named_as_unregulated() {
    let schema =
        Schema::new().declare("balance", DimType::Any).declare("wellbeing", DimType::Any);
    assert_eq!(model().blind_to(&schema), vec!["wellbeing".to_string()]);
}

#[test]
fn a_fidelity_claim_needs_a_basis_and_the_gate_never_reads_it() {
    // ★ Limit 1. And structurally: `execute` takes no model, so a claim cannot
    // reach an admission.
    assert!(Fidelity::claimed(0.9, "   ").is_none());
    assert!(Fidelity::claimed(1.5, "anything").is_none());
    assert_eq!(model().fidelity().value(), 0.7);
    assert!(!model().fidelity().basis().is_empty());
}

#[test]
fn a_projection_cannot_be_read_without_its_horizon() {
    // ★ Limit 3: no accessor returns the value alone.
    let p = Projection::over(12.5, 8);
    assert_eq!(p.reported(), (12.5, 8));
    assert_eq!(p.horizon(), 8);
}

#[test]
fn a_scenario_binds_the_residual_and_the_template_holds_the_structure() {
    let t = ModelTemplate::new().edge("s0", "budget.allocate", vec![("s1", Prob::param("p"))]);
    assert!(t.parameters().contains("p"), "the residual is a named parameter");
    assert!(t.actions().contains("budget.allocate"), "the structure is not");
}

// ── the shared-model correlation ─────────────────────────────────────────────

#[test]
fn independent_observers_have_an_effective_n_equal_to_their_headcount() {
    assert_eq!(Agreement::independent(5).effective_n(), EffectiveN::Independent(5));
    assert!(Agreement::independent(5).effective_n().is_independent());
}

#[test]
fn councillors_who_forked_one_model_are_not_n_independent_confirmations() {
    // ★★★ THE KEYSTONE.
    let a = Agreement::via_shared_model(5, &model());
    let n = a.effective_n();
    assert_eq!(n, EffectiveN::BoundedAbove { headcount: 5, shared_model: "household".into() });
    assert!(!n.is_independent(), "unanimity is not independence");
    assert_eq!(a.headcount(), 5, "and the headcount is still reported");
}

#[test]
fn no_effective_n_number_is_fabricated_for_a_shared_model() {
    // ★★★ The shortfall is unmeasurable from inside, so no estimate is given.
    let n = Agreement::via_shared_model(9, &model()).effective_n();
    match &n {
        EffectiveN::BoundedAbove { headcount, .. } => assert_eq!(*headcount, 9),
        other => panic!("a number was fabricated: {other:?}"),
    }
    assert!(n.describe().contains("cannot be measured from inside"));
}

#[test]
fn shared_model_agreement_cannot_be_declared_independent() {
    // ★★ Unspellable: the same headcount reads differently by provenance alone.
    let shared = Agreement::via_shared_model(4, &model()).effective_n();
    let alone = Agreement::independent(4).effective_n();
    assert_ne!(shared, alone);
    assert!(alone.is_independent() && !shared.is_independent());
}

#[test]
fn forking_changes_the_sampling_not_the_assumptions() {
    // ★ The correlation does not dilute with headcount.
    for n in [2usize, 20, 200] {
        assert!(
            !Agreement::via_shared_model(n, &model()).effective_n().is_independent(),
            "{n} councillors, still one model"
        );
    }
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-spec");

    let step0 = &doc["★★_the_STEP_0_two_reconciles"];
    for key in [
        "★★★_1_M_world_is_a_FRAMING_over_pieces_that_already_existed",
        "★★★_2_how_an_operative_models_a_world_it_cannot_HOLD",
        "★_3_M_self_composes_ω's_own_parts_rather_than_forking_them",
        "4_what_is_genuinely_new",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★★ The framing must stay credited to the pieces it framed.
    assert!(step0["★★★_1_M_world_is_a_FRAMING_over_pieces_that_already_existed"]
        .as_str()
        .unwrap()
        .contains("ModelTemplate"));

    // ★★★ The finding, and its counterweight.
    let finding = d["★★★_the_finding_the_reference_SUMS_confidence_across_councillors"]
        .as_str()
        .unwrap();
    assert!(finding.contains("weighted_yes"), "the arithmetic must stay named");
    assert!(finding.contains("forks per councillor"), "the counterweight must stay");
    assert!(finding.contains("never claims to"), "the fairness must stay");

    let refusal = d["★★_and_the_correction_refuses_to_fabricate_a_number"].as_str().unwrap();
    assert!(refusal.contains("nobody inside can measure"));
    assert!(refusal.contains("unspellable"));

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.contains("fidelity")));
    assert!(fp.keys().any(|k| k.contains("conant / ashby")));
    assert!(fp.keys().any(|k| k.contains("model (very many real hits")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "`structure_is_declared` IS ALWAYS TRUE",
        "THE FIDELITY CLAIM IS DECLARED, NEVER MEASURED",
        "NO EFFECTIVE-N NUMBER IS OFFERED FOR A SHARED MODEL",
        "`agrees_with` IS A REPORT, NOT A GUARD",
        "NOTHING WIRES EITHER MODEL ONTO `ω` YET",
        "`⊕̂` IS NOT MODELLED",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["self_model_cases", "world_model_cases", "correlation_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 13, "every declared case must be present");
}
