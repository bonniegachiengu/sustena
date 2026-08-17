//! `M(t+1) = retain(select_{u,Viab}(vary(M(t))))`, replayed against its
//! conformance vectors (R2 · Operative §IV · OPV-6).
//!
//! ★ The LAST row of the deep bundle. Reference-less for *strategy* learning —
//! but the reference runs this loop one layer down over ParseRules, which is a
//! real counterweight and is recorded as one.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    attention::{Aperture, Attention},
    learning::{
        learn, retain, select, vary, Feedback, Fitness, LearningTrace, LibraryError, LibraryScope,
        MemeLibrary, MemeProvenance, Selection, Stabilisation, VaryOp,
    },
    models::Projection,
    operative::{Cynefin, Objective, Operative, Sense, Shared, Utility},
    operator::{Enforcement, Registry},
    region::{Interval, Region},
    strategy::{StrategyError, StrategyGraph},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("learning.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "learning.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn world() -> Shared {
    Shared::new().with_move("budget.allocate").with_move("budget.record_income")
}

fn allowed() -> Vec<String> {
    vec!["budget.allocate".to_string(), "budget.record_income".to_string()]
}

fn operative() -> Operative {
    let u = Utility::new()
        .with(Objective::new("liquidity", "finances.liquid.balance", Sense::Maximise))
        .unwrap();
    Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
}

/// Reads a dimension no operator here ever writes.
fn operative_reading_wellbeing() -> Operative {
    let u = Utility::new()
        .with(Objective::new("liquidity", "finances.liquid.balance", Sense::Maximise))
        .unwrap()
        .with(Objective::new("wellbeing", "household.wellbeing", Sense::Maximise))
        .unwrap();
    Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
}

fn viable() -> Region {
    Region::new().bounding(Interval::at_least("finances.liquid.balance", 0.0))
}

fn armed() -> Enforcement {
    Enforcement {
        enabled: true,
        invariants: vec![("liquid_non_negative".into(), "finances.liquid.balance >= 0".into())],
        ..Enforcement::default()
    }
}

fn base() -> StrategyGraph {
    let mut kwargs = Map::new();
    kwargs.insert("amount".into(), json!(100.0));
    kwargs.insert("source".into(), json!("salary"));
    StrategyGraph::new("a", "a").with_node(&world(), "a", "budget.record_income", kwargs).unwrap()
}

fn library() -> MemeLibrary {
    let mut lib = MemeLibrary::of(LibraryScope::of("household", "mentor"));
    lib.author("m0", base()).unwrap();
    lib
}

fn attention() -> Attention {
    Attention::declared(
        Aperture::declared(1, 3, 4).unwrap(),
        Aperture::declared(12, 1, 1).unwrap(),
        100,
    )
    .unwrap()
}

fn state() -> Value {
    json!({"finances": {
        "liquid": {"balance": 500.0},
        "pockets": {},
        "income": {"monthly_total": 0.0, "sources": []}
    }})
}

fn allocate_kwargs() -> Map<String, Value> {
    let mut k = Map::new();
    k.insert("pocket_name".into(), json!("food"));
    k.insert("amount".into(), json!(50.0));
    k
}

fn tune(value: f64) -> VaryOp {
    VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(value) }
}

fn scored(ops: &[VaryOp], op: &Operative, enforcement: &Enforcement, s: &Value) -> Selection {
    let lib = library();
    let out = vary(lib.get("m0").unwrap(), &world(), ops);
    select(
        &out.variants,
        op,
        &viable(),
        &Registry::default(),
        &allowed(),
        enforcement,
        s,
        Value::Object(Map::new()),
    )
}

// ── vary ─────────────────────────────────────────────────────────────────────

#[test]
fn variation_cannot_manufacture_a_move_outside_t() {
    let lib = library();
    let out = vary(
        lib.get("m0").unwrap(),
        &world(),
        &[VaryOp::Extend {
            node: "b".into(),
            mv: "budget.teleport".into(),
            after: "a".into(),
            kwargs: Map::new(),
        }],
    );
    assert!(out.variants.is_empty(), "no variant was built");
    assert!(matches!(
        out.refused.first().map(|(_, e)| e),
        Some(StrategyError::MoveNotInT { .. })
    ));
}

#[test]
fn an_extension_with_a_legal_move_is_still_bounded_by_t() {
    let w = world();
    let lib = library();
    let out = vary(
        lib.get("m0").unwrap(),
        &w,
        &[VaryOp::Extend {
            node: "b".into(),
            mv: "budget.allocate".into(),
            after: "a".into(),
            kwargs: allocate_kwargs(),
        }],
    );
    assert_eq!(out.refused.len(), 0);
    let t: BTreeSet<&str> = w.moves().into_iter().collect();
    assert!(out.variants[0].moves().is_subset(&t), "Π ⊆ T survives variation");
}

#[test]
fn optimise_retunes_a_parameter_and_leaves_the_moves_alone() {
    let lib = library();
    let before = lib.get("m0").unwrap().strategy().moves();
    let out = vary(lib.get("m0").unwrap(), &world(), &[tune(250.0)]);
    assert_eq!(out.variants[0].moves(), before);
    assert_eq!(
        out.variants[0].strategy().nodes().next().unwrap().kwargs().get("amount"),
        Some(&json!(250.0))
    );
}

#[test]
fn a_refused_variation_is_reported_not_dropped() {
    let lib = library();
    let out = vary(
        lib.get("m0").unwrap(),
        &world(),
        &[
            VaryOp::Extend {
                node: "b".into(),
                mv: "nope".into(),
                after: "a".into(),
                kwargs: Map::new(),
            },
            tune(1.0),
        ],
    );
    assert_eq!(out.variants.len(), 1, "the legal one still ran");
    assert_eq!(out.refused.len(), 1, "the illegal one is on the record");
}

#[test]
fn a_variants_id_is_unique_per_op_not_per_label() {
    // ★ The regression. Two `Optimise` ops on the same node and parameter used
    // to mint the same id, and `retain` reported the second as *already in the
    // library* — two genuinely different variants, silently conflated.
    let lib = library();
    let out = vary(lib.get("m0").unwrap(), &world(), &[tune(100.0), tune(300.0)]);
    assert_eq!(out.variants.len(), 2);
    assert_ne!(out.variants[0].id(), out.variants[1].id());
    // And the label still says which value, because that is what was learned.
    assert!(out.variants[1].id().contains("300"));
}

// ── select ───────────────────────────────────────────────────────────────────

#[test]
fn a_variant_is_scored_by_running_it_in_the_sandbox() {
    let sel = scored(&[tune(250.0)], &operative(), &Enforcement::default(), &state());
    assert!(matches!(sel.scored[0].1, Fitness::Viable { .. }));
    assert_eq!(sel.frontier.len(), 1);
}

#[test]
fn scoring_leaves_the_state_it_was_handed_untouched() {
    let s = state();
    let _ = scored(&[tune(999.0)], &operative(), &Enforcement::default(), &s);
    assert_eq!(s, state(), "the core is pure; there was no write to protect from");
}

#[test]
fn a_variant_the_gate_refuses_scores_rather_than_erroring() {
    let sel = scored(&[tune(-1000.0)], &operative(), &armed(), &state());
    assert!(matches!(sel.scored[0].1, Fitness::Refused { .. }), "{:?}", sel.scored[0].1);
    assert!(sel.frontier.is_empty(), "a refused variant never reaches the frontier");
}

#[test]
fn a_missing_dimension_is_unscorable_not_zero() {
    let sel = scored(
        &[tune(10.0)],
        &operative_reading_wellbeing(),
        &Enforcement::default(),
        &state(),
    );
    assert!(matches!(sel.scored[0].1, Fitness::Unscorable { .. }), "{:?}", sel.scored[0].1);
    assert!(sel.frontier.is_empty());
}

#[test]
fn selection_returns_a_set_and_there_is_no_method_that_returns_a_winner() {
    // Two viable variants with different tunings: both are scored, and the
    // frontier is whatever is non-dominated — a `Vec`, never collapsed.
    let sel = scored(&[tune(100.0), tune(300.0)], &operative(), &Enforcement::default(), &state());
    assert_eq!(sel.scored.len(), 2, "nothing is discarded before it is recorded");
    assert!(!sel.frontier.is_empty());
}

// ── retain, provenance and scope ─────────────────────────────────────────────

fn a_round(lib: &mut MemeLibrary, ops: &[VaryOp], fb: Feedback) -> Vec<String> {
    learn(
        lib,
        "m0",
        &world(),
        &operative(),
        &viable(),
        ops,
        &Registry::default(),
        &allowed(),
        &Enforcement::default(),
        &state(),
        fb,
    )
    .expect("m0 is in the library")
    .retention
    .kept
}

#[test]
fn a_retained_variant_carries_where_it_came_from_and_why() {
    let mut lib = library();
    let fb = Feedback::Correction { by: "bonnie".into(), note: "too small".into() };
    let kept = a_round(&mut lib, &[tune(250.0)], fb.clone());

    assert_eq!(kept.len(), 1);
    let meme = lib.get(&kept[0]).unwrap();
    assert!(matches!(meme.provenance(), MemeProvenance::Varied { from, .. } if from == "m0"));
    assert!(meme.retained_by().is_some(), "the score that kept it is on the record");
    assert_eq!(meme.because(), Some(fb.describe().as_str()));
}

#[test]
fn an_imported_meme_cannot_read_as_native() {
    let donor = library();
    let m = donor.get("m0").unwrap().clone();
    let mut mine = MemeLibrary::of(LibraryScope::of("neighbour", "mentor"));
    mine.import(&m, donor.scope(), "borrowed").unwrap();

    match mine.get("borrowed").unwrap().provenance() {
        MemeProvenance::Imported { origin } => assert_eq!(origin, donor.scope()),
        other => panic!("an import read as {other:?}"),
    }
    assert_eq!(mine.imported().len(), 1, "and it is countable");
}

#[test]
fn a_library_is_scoped_and_importing_from_itself_is_refused() {
    let lib = library();
    let m = lib.get("m0").unwrap().clone();
    let mut same = MemeLibrary::of(LibraryScope::of("household", "mentor"));
    assert!(matches!(
        same.import(&m, &LibraryScope::of("household", "mentor"), "x"),
        Err(LibraryError::SameScope { .. })
    ));
}

#[test]
fn an_imported_meme_does_not_inherit_the_donors_score() {
    let mut donor = library();
    let kept = a_round(
        &mut donor,
        &[tune(250.0)],
        Feedback::Correction { by: "b".into(), note: "n".into() },
    );
    let m = donor.get(&kept[0]).unwrap().clone();
    assert!(m.retained_by().is_some(), "it was proven there");

    let mut mine = MemeLibrary::of(LibraryScope::of("neighbour", "mentor"));
    mine.import(&m, donor.scope(), "borrowed").unwrap();
    assert!(
        mine.get("borrowed").unwrap().retained_by().is_none(),
        "a fitness earned against another household's problems is not a fitness here"
    );
}

// ── the trigger ──────────────────────────────────────────────────────────────

#[test]
fn there_is_no_feedback_variant_for_a_timer() {
    // The match below is exhaustive over `Feedback`. It compiles only while the
    // enum has exactly these three, so a `Scheduled` added later breaks here.
    for f in [
        Feedback::Correction { by: "b".into(), note: "n".into() },
        Feedback::PredictionDiverged {
            model: "m".into(),
            predicted: 1.0,
            observed: 2.0,
            horizon: 3,
        },
        Feedback::Objection { by: "curator".into(), note: "n".into() },
    ] {
        let kind = match f {
            Feedback::Correction { .. } => "correction",
            Feedback::PredictionDiverged { .. } => "prediction_diverged",
            Feedback::Objection { .. } => "objection",
        };
        assert!(!kind.is_empty());
    }
}

#[test]
fn an_agreeing_projection_produces_no_feedback_and_so_no_round() {
    let p = Projection::over(10.0, 4);
    assert!(Feedback::from_projection("household", &p, 10.0).is_none());
    match Feedback::from_projection("household", &p, 12.0).unwrap() {
        Feedback::PredictionDiverged { horizon, .. } => assert_eq!(horizon, 4),
        other => panic!("{other:?}"),
    }
}

// ── the hypothesis, instrumented and not asserted ────────────────────────────

#[test]
fn stabilisation_never_proves_convergence() {
    for s in [
        Stabilisation::UnchangedFor { rounds: 99 },
        Stabilisation::Churning,
        Stabilisation::Unobserved,
    ] {
        assert!(!s.proves_convergence(), "{s:?} must not claim a theorem");
    }
}

#[test]
fn an_empty_trace_is_unobserved_not_stable() {
    assert_eq!(LearningTrace::new().stabilisation(), Stabilisation::Unobserved);
}

#[test]
fn a_round_that_kept_nothing_reads_as_unchanged() {
    let mut lib = library();
    let mut trace = LearningTrace::new();
    let round = learn(
        &mut lib,
        "m0",
        &world(),
        &operative(),
        &viable(),
        &[VaryOp::Extend {
            node: "b".into(),
            mv: "nope".into(),
            after: "a".into(),
            kwargs: Map::new(),
        }],
        &Registry::default(),
        &allowed(),
        &Enforcement::default(),
        &state(),
        Feedback::Objection { by: "curator".into(), note: "n".into() },
    )
    .unwrap();
    assert!(!round.changed_the_library());
    trace.record(round);
    assert_eq!(trace.stabilisation(), Stabilisation::UnchangedFor { rounds: 1 });
    assert!(!trace.stabilisation().proves_convergence());
}

#[test]
fn the_trace_reads_back_what_triggered_each_round() {
    let mut lib = library();
    let mut trace = LearningTrace::new();
    for (i, fb) in [
        Feedback::Correction { by: "b".into(), note: "n".into() },
        Feedback::Objection { by: "c".into(), note: "n".into() },
    ]
    .into_iter()
    .enumerate()
    {
        let round = learn(
            &mut lib,
            "m0",
            &world(),
            &operative(),
            &viable(),
            &[tune(200.0 + i as f64)],
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            fb,
        )
        .unwrap();
        trace.record(round);
    }
    let t = trace.triggers();
    assert_eq!(t.get("correction"), Some(&1));
    assert_eq!(t.get("objection"), Some(&1));
    assert!(!trace.retained_fitness().is_empty(), "did the survivors score? answerable");
}

// ── the OPV-11 reconcile, and the bundle's closing property ──────────────────

#[test]
fn what_changed_about_me_shows_up_in_the_self_model() {
    let mut lib = library();
    let op = operative();
    let att = attention();

    let before = lib.get("m0").unwrap().self_model(&op, &att, &["household"]);
    assert!(!before.moves().contains("budget.allocate"));

    let kept = a_round(
        &mut lib,
        &[VaryOp::Extend {
            node: "b".into(),
            mv: "budget.allocate".into(),
            after: "a".into(),
            kwargs: allocate_kwargs(),
        }],
        Feedback::Correction { by: "bonnie".into(), note: "allocate it".into() },
    );

    let after = lib.get(&kept[0]).unwrap().self_model(&op, &att, &["household"]);
    assert!(after.moves().contains("budget.allocate"), "M_self reflects the change");
}

#[test]
fn learning_cannot_produce_a_self_model_that_escapes_t() {
    // ★★★ Prop 1 (minting) → Prop 3 (nodes) → variation → M_self, unbroken.
    let w = world();
    let mut lib = library();
    let kept = a_round(
        &mut lib,
        &[VaryOp::Extend {
            node: "b".into(),
            mv: "budget.allocate".into(),
            after: "a".into(),
            kwargs: allocate_kwargs(),
        }],
        Feedback::Correction { by: "bonnie".into(), note: "allocate it".into() },
    );
    let m = lib.get(&kept[0]).unwrap().self_model(&operative(), &attention(), &["household"]);
    assert!(m.moves_within(&w));
}

// ── retain, called directly ──────────────────────────────────────────────────

#[test]
fn retain_records_what_it_dropped_as_well_as_what_it_kept() {
    let mut lib = library();
    let ops = [tune(250.0), tune(-1000.0)];
    let out = vary(lib.get("m0").unwrap(), &world(), &ops);
    let sel = select(
        &out.variants,
        &operative(),
        &viable(),
        &Registry::default(),
        &allowed(),
        &armed(),
        &state(),
        Value::Object(Map::new()),
    );
    let report = retain(&mut lib, &out.variants, &sel, &Feedback::Objection {
        by: "curator".into(),
        note: "reconsider".into(),
    });
    assert_eq!(report.kept.len(), 1, "{report:?}");
    assert_eq!(report.dropped.len(), 1, "the refused one is named, not silently gone");
    assert!(report.dropped[0].1.contains("gate refused"), "{report:?}");
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-spec");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_the_one_place_structure_moves_is_the_STRATEGY_LIBRARY_not_M_world",
        "★★★_2_Proposition_3_had_to_survive_variation_and_it_does_by_inheritance",
        "★★_3_select_could_not_collapse_u_into_a_scalar",
        "★★_4_vary_is_declared_not_random_because_ADR_0001_forbids_RNG",
        "5_what_is_genuinely_new_here",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★★ The M_world consistency must stay stated, not merely implied.
    let one = step0["★★★_1_the_one_place_structure_moves_is_the_STRATEGY_LIBRARY_not_M_world"]
        .as_str()
        .unwrap();
    assert!(one.contains("structure_is_declared"));
    assert!(one.contains("stays `true`"));

    // ★★★ The counterweight — the reference DOES run this loop one layer down.
    let cw = d["★★★_the_counterweight_is_real_the_reference_RUNS_this_loop_one_layer_down"]
        .as_str()
        .unwrap();
    for term in ["synthesize_rule_from_correction", "verify_proposed_rule", "parse_rule_edits"] {
        assert!(cw.contains(term), "the counterweight must name {term}");
    }
    assert!(cw.contains("FEEDBACK-TRIGGERED"), "and credit the discipline it already had");

    let finding = d["★★★_the_finding_what_the_reference_has_no_analogue_of"].as_str().unwrap();
    assert!(finding.contains("NOT SCORED BY VIABILITY"));
    assert!(finding.contains("NO NFL SCOPE"));

    let hypothesis = d["★★_the_hypothesis_this_row_refuses_to_assert"].as_str().unwrap();
    assert!(hypothesis.contains("proves_convergence"));
    assert!(hypothesis.contains("MECHANISM") && hypothesis.contains("SCORING"));

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("variant (135")), "the sharpest one by count");
    assert!(fp.keys().any(|k| k.starts_with("library (32")));
    assert!(fp.keys().any(|k| k.starts_with("provenance (45")));

    let terms = d["term_by_term"].as_object().unwrap();
    for k in ["vary(M(t))", "select_{u,Viab}", "retain", "M(t)", "the trigger"] {
        assert!(terms.contains_key(k), "term_by_term is missing {k}");
    }

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "`Stabilisation::proves_convergence` IS ALWAYS FALSE",
        "`vary` IS DECLARED, NOT SAMPLED",
        "no crossover between two memes",
        "`retain` IS APPEND-ONLY",
        "EXTENDING AT THE EXIT MOVES THE EXIT",
        "SELECTION SCORES THE END STATE ONLY",
        "NOTHING WIRES A LIBRARY ONTO `ω` YET",
        "NO COST IS CHARGED FOR A ROUND",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in [
        "vary_cases",
        "select_cases",
        "retain_cases",
        "trigger_cases",
        "hypothesis_cases",
        "reconcile_cases",
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
    assert_eq!(seen.len(), 22, "every declared case must be present");
}
