//! The widget schema and the check on the load path, replayed against its
//! conformance vectors (R2 · Curated UI · UI-2, first of the surface rows).
//!
//! **SPEC vectors, Rust-only.** ★★ STEP-0's crux: there is **no widget code in
//! this core at all** — `curated` is grep-0 and every `widget`/`salience` hit
//! is prose. So the row is *build the typed load path with the check on it*,
//! not *port a checker to sit beside nothing*.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    editing::Definition,
    operator::Registry,
    schema::{DimType, Schema},
    widget::{EmitError, LoadError, WidgetDecl, WidgetSet},
    CONFORMANCE_VERSION,
};

fn load_vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("widget.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "widget.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn definition() -> Definition {
    Definition::new(
        Schema::new()
            .declare("finances", DimType::Any)
            .declare("roster", DimType::List { item: Box::new(DimType::Text) }),
    )
    .with_operator("budget.record_income")
    .with_operator("budget.allocate")
}

fn valid() -> WidgetDecl {
    WidgetDecl::new("pocket_ring", "ring")
        .reading("finances")
        .emitting("budget.allocate")
}

// ── the load path ────────────────────────────────────────────────────────────

#[test]
fn a_valid_widget_loads() {
    let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
    assert_eq!(set.len(), 1);
    let w = set.get("pocket_ring").expect("present by id");
    assert_eq!(w.render(), "ring");
    assert_eq!(w.inputs(), ["finances".to_string()]);
    assert_eq!(w.emits(), ["budget.allocate".to_string()]);
}

#[test]
fn a_made_up_input_is_rejected_at_load() {
    // ★★ The headline: a typo'd dimension produces NO widget, not a widget that
    // renders blank or reads null at runtime.
    let bad = WidgetDecl::new("bad", "card").reading("finaces.liquid");
    let errors = WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
    assert_eq!(
        errors,
        vec![LoadError::UndeclaredInput {
            widget: "bad".into(),
            path: "finaces.liquid".into()
        }]
    );
}

#[test]
fn an_unpermitted_emit_is_rejected_at_load() {
    // `budget.spend` IS registered — the definition just does not list it — so
    // exactly one error fires and it is the right one.
    let bad = WidgetDecl::new("bad", "card").emitting("budget.spend");
    let errors = WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
    assert_eq!(
        errors,
        vec![LoadError::NotPermitted { widget: "bad".into(), operator: "budget.spend".into() }]
    );
}

#[test]
fn an_unregistered_emit_is_a_different_error_from_an_unpermitted_one() {
    // A definition that DOES permit an operator nothing registers, so only the
    // registry error fires — the two are genuinely separate findings.
    let d = definition().with_operator("budget.invented");
    let bad = WidgetDecl::new("bad", "card").emitting("budget.invented");
    let errors = WidgetSet::load(vec![bad], &d, &Registry::default()).unwrap_err();
    assert_eq!(
        errors,
        vec![LoadError::NotRegistered {
            widget: "bad".into(),
            operator: "budget.invented".into()
        }]
    );
}

#[test]
fn an_unparseable_input_is_reported_as_such() {
    let bad = WidgetDecl::new("bad", "card").reading("");
    let errors = WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
    assert!(matches!(errors[0], LoadError::UnparseableInput { .. }));
}

#[test]
fn a_wildcard_input_is_accepted_where_the_schema_declares_the_map() {
    let d = Definition::new(Schema::new().declare(
        "pockets",
        DimType::Map { value: Box::new(DimType::Number { lo: None, hi: None }) },
    ));
    let w = WidgetDecl::new("ring", "ring").reading("pockets[*]");
    assert!(WidgetSet::load(vec![w], &d, &Registry::default()).is_ok());
}

// ── structural ───────────────────────────────────────────────────────────────

#[test]
fn every_error_is_reported_not_only_the_first() {
    let bad = WidgetDecl::new("bad", "card")
        .reading("nope")
        .reading("alsonope")
        .emitting("budget.spend");
    let errors = WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
    assert_eq!(errors.len(), 3, "fixing a surface one message at a time is the failure mode");
}

#[test]
fn one_bad_widget_refuses_the_whole_set() {
    // ★★ A partial load is silent degradation: the widget simply is not there,
    // which is indistinguishable from one that had nothing to show.
    let decls = vec![valid(), WidgetDecl::new("bad", "card").reading("nope")];
    assert!(WidgetSet::load(decls, &definition(), &Registry::default()).is_err());
}

#[test]
fn a_duplicate_id_is_refused() {
    let errors =
        WidgetSet::load(vec![valid(), valid()], &definition(), &Registry::default()).unwrap_err();
    assert!(errors.contains(&LoadError::DuplicateId { widget: "pocket_ring".into() }));
}

// ── the two containments ─────────────────────────────────────────────────────

#[test]
fn a_loaded_widget_may_emit_what_it_declared() {
    let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
    let emission = set.get("pocket_ring").unwrap().emit("budget.allocate", Map::new()).unwrap();
    assert_eq!(emission.operator(), "budget.allocate");
    assert_eq!(emission.widget_id(), "pocket_ring");
}

#[test]
fn a_loaded_widget_may_not_emit_what_it_did_not_declare() {
    // ★★ The inner containment. `budget.record_income` IS permitted by the
    // definition — this widget never asked for it.
    let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
    assert_eq!(
        set.get("pocket_ring").unwrap().emit("budget.record_income", Map::new()),
        Err(EmitError::NotDeclared {
            widget: "pocket_ring".into(),
            operator: "budget.record_income".into()
        })
    );
}

#[test]
fn params_travel_with_the_emission() {
    let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
    let mut params = Map::new();
    params.insert("amount".into(), json!(10.0));
    let emission = set.get("pocket_ring").unwrap().emit("budget.allocate", params).unwrap();
    assert_eq!(emission.params()["amount"], json!(10.0));
}

// ── staleness against a real definition edit ─────────────────────────────────

#[test]
fn retiring_a_dimension_makes_a_loaded_set_stale() {
    // ★ Driven by a REAL typed edit (EDIT-13's taxonomy), not a hand-built
    // definition: the thing that invalidates a surface in practice.
    use sustena_core::editing::Edit;
    let d = definition();
    let set = WidgetSet::load(vec![valid()], &d, &Registry::default()).unwrap();
    assert!(set.is_current_for(&d));

    let edited = Edit::RetireDim { name: "finances".into() }.apply(&d);
    assert!(!set.is_current_for(&edited), "the widget's input no longer exists");
}

#[test]
fn retiring_an_operator_makes_a_loaded_set_stale() {
    use sustena_core::editing::Edit;
    let d = definition();
    let set = WidgetSet::load(vec![valid()], &d, &Registry::default()).unwrap();
    let edited = Edit::RetireOp { name: "budget.allocate".into() }.apply(&d);
    assert!(!set.is_current_for(&edited));
}

#[test]
fn changing_an_invariant_does_not_make_a_set_stale() {
    // ★ A widget check depends on dim(S) and T only; a wider key would report
    // staleness that is not there.
    use sustena_core::editing::Edit;
    let d = definition();
    let set = WidgetSet::load(vec![valid()], &d, &Registry::default()).unwrap();
    let edited =
        Edit::AddInv { id: "floor".into(), expression: "finances >= 0".into() }.apply(&d);
    assert!(set.is_current_for(&edited));
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load_vectors();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-ahead-of-python");

    let step0 = &doc["★★_the_STEP_0_reconcile_AND_THE_CRUX_WAS_THAT_THERE_IS_NO_LOAD_PATH"];
    for key in [
        "1_there_is_NO_widget_code_in_this_core_at_all",
        "★★_2_so_the_honest_scope_is_BUILD_THE_LOAD_PATH_WITH_THE_CHECK_ON_IT",
        "★_3_the_substrate_it_reads_ALREADY_EXISTS_and_is_not_redefined",
        "★★_4_AND_STEP_0_CAUGHT_THIS_REPOSITORY_OVERSTATING_ITSELF",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★★ The self-correction must stay on the record.
    assert!(step0["★★_4_AND_STEP_0_CAUGHT_THIS_REPOSITORY_OVERSTATING_ITSELF"]
        .as_str()
        .unwrap()
        .contains("no `curated_ui.rs`"));

    // ★★ The counterweight: the check and the schema shape are the reference's.
    let cw = d["★★_the_counterweight_THE_CHECK_ITSELF_IS_THE_REFERENCE'S_AND_SO_IS_THE_SCHEMA_SHAPE"]
        .as_str()
        .unwrap();
    for term in ["written there first", "mostly built", "one word wide", "min_privilege"] {
        assert!(cw.contains(term), "missing counterweight detail: {term}");
    }

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.contains("widget / Widget in RUST (12 hits, ZERO code)")));
    assert!(fp.keys().any(|k| k.contains("curated (grep-0 in Rust)")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THIS IS THE LOAD PATH, NOT A SURFACE",
        "`render` IS AN OPAQUE STRING, DELIBERATELY",
        "THE PARAMS OF AN EMISSION ARE NOT TYPE-CHECKED",
        "A LOADED SET CAN GO STALE",
        "ALL-OR-NOTHING IS A POSITION, NOT AN OVERSIGHT",
        "NOTHING WIRES A `WidgetEmission` TO `execute` HERE",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["load_path_cases", "structural_cases", "containment_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 14, "every declared case must be present");
}
