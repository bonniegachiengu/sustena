//! The definition lens `π : D → G`, replayed against its conformance vectors
//! (R2 · Editing §IX · EDIT-13, the last M-EDIT row).
//!
//! **SPEC vectors, Rust-only.** ★★ `π` on its own is easy; the difficulty is
//! that it **forgets**, so the editor is a lens — `get` paired with a `put`
//! that takes the old `D` precisely to restore what `get` discarded.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    editing::{admit_edit, Definition, EditEffect, EditError, Instance, Migration},
    lens::{get, put, put_apply, DimLabel, GraphEdge, Node, PutError, Supplied},
    operator::Registry,
    schema::{DimType, Schema},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("lens.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "lens.json was written for a different contract version"
    );
    doc
}

// ── fixture ──────────────────────────────────────────────────────────────────

/// Carries two things the graph will not draw: an expression, and a bounded
/// number type.
fn definition() -> Definition {
    Definition::new(
        Schema::new()
            .declare("finances", DimType::Number { lo: Some(0.0), hi: Some(1_000_000.0) })
            .declare("roster", DimType::List { item: Box::new(DimType::Text) }),
    )
    .with_invariant("floor", "finances >= 500")
    .with_operator("budget.record_income")
    .with_operator("budget.allocate")
}

// ── the laws ─────────────────────────────────────────────────────────────────

#[test]
fn getput_an_untouched_graph_produces_no_edit_at_all() {
    let d = definition();
    let edits = put(&get(&d), &d, &Supplied::none()).unwrap();
    assert!(edits.is_empty(), "no spurious change, and not even a no-op edit");
    assert_eq!(put_apply(&get(&d), &d, &Supplied::none()).unwrap(), d);
}

#[test]
fn putget_an_edit_on_the_graph_survives_reprojection() {
    let d = definition();
    let g2 = get(&d).without(&Node::Operator { name: "budget.allocate".into() });
    let d2 = put_apply(&g2, &d, &Supplied::none()).unwrap();
    assert_eq!(get(&d2), g2, "literally equal — G carries no detail to normalise away");
}

#[test]
fn putget_holds_for_an_addition_too() {
    let d = definition();
    let g2 = get(&d).with(Node::Invariant { id: "ceiling".into() });
    let d2 = put_apply(&g2, &d, &Supplied::none().invariant("ceiling", "finances <= 900")).unwrap();

    assert!(get(&d2).invariant_ids().contains("ceiling"));
    // Re-projecting the RESULT is stable, which is the honest form of the law
    // for an addition whose edges the caller could not have drawn.
    let d3 = put_apply(&get(&d2), &d2, &Supplied::none()).unwrap();
    assert_eq!(get(&d3), get(&d2));
}

// ── the forgetting, and the preservation ─────────────────────────────────────

#[test]
fn pi_forgets_the_expression_text_and_the_fine_type_detail() {
    let g = get(&definition());
    let rendered = format!("{g:?}");

    assert!(g.invariant_ids().contains("floor"));
    assert!(!rendered.contains(">= 500"), "the expression must not survive into G");

    assert!(g
        .nodes
        .contains(&Node::Dimension { name: "finances".into(), label: DimLabel::Number }));
    assert!(!rendered.contains("1000000"), "the bounds must not survive into G");
}

#[test]
fn a_round_trip_keeps_what_the_graph_never_showed() {
    // ★★★ THE LENS PROPERTY, and the reason `put` takes the old D.
    let d = definition();
    let g2 = get(&d).without(&Node::Operator { name: "budget.allocate".into() });
    let d2 = put_apply(&g2, &d, &Supplied::none()).unwrap();

    assert_eq!(d2.operators, vec!["budget.record_income".to_string()], "the edit landed");
    assert_eq!(d2.invariants, d.invariants, "expression text preserved");
    assert_eq!(
        d2.schema.dimensions.get("finances"),
        Some(&DimType::Number { lo: Some(0.0), hi: Some(1_000_000.0) }),
        "the bounds the graph never drew are still here"
    );
}

#[test]
fn the_edges_are_derived_from_the_parsed_predicate() {
    let g = get(&definition());
    assert!(g.edges.contains(&GraphEdge {
        from: "inv:floor".into(),
        to: "dim:finances".into(),
        kind: sustena_core::lens::EdgeKind::Constrains,
    }));
    assert!(!g.edges.iter().any(|e| e.to == "dim:roster"), "roster is untouched by the rule");
}

#[test]
fn a_new_node_with_no_supplied_detail_is_refused_not_defaulted() {
    let d = definition();

    let g2 = get(&d).with(Node::Invariant { id: "ceiling".into() });
    assert_eq!(
        put(&g2, &d, &Supplied::none()),
        Err(PutError::MissingDetail { node: "inv:ceiling".into(), needs: "expression" })
    );

    let g3 = get(&d).with(Node::Dimension { name: "extra".into(), label: DimLabel::Number });
    assert!(matches!(put(&g3, &d, &Supplied::none()), Err(PutError::MissingDetail { .. })));
}

#[test]
fn an_unparseable_supplied_expression_is_refused_at_the_lens() {
    let d = definition();
    let g2 = get(&d).with(Node::Invariant { id: "bad".into() });
    assert!(matches!(
        put(&g2, &d, &Supplied::none().invariant("bad", "this is not a predicate")),
        Err(PutError::UnparseableExpression { .. })
    ));
}

#[test]
fn an_operator_needs_nothing_supplied_because_pi_forgot_nothing_there() {
    let d = definition();
    let g2 = get(&d).with(Node::Operator { name: "budget.spend".into() });
    let edits = put(&g2, &d, &Supplied::none()).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].kind(), "AddOp");
}

// ── typed, and routed through the gate ───────────────────────────────────────

#[test]
fn every_graph_edit_is_a_typed_edit() {
    let d = definition();
    let g2 = get(&d)
        .without(&Node::Operator { name: "budget.allocate".into() })
        .without(&Node::Invariant { id: "floor".into() })
        .with(Node::Operator { name: "budget.spend".into() });

    let kinds: BTreeSet<&str> =
        put(&g2, &d, &Supplied::none()).unwrap().iter().map(|e| e.kind()).collect();
    assert_eq!(kinds, ["AddOp", "DropInv", "RetireOp"].into_iter().collect::<BTreeSet<_>>());
}

#[test]
fn a_graph_edit_reaches_admit_edit_and_the_stranding_check_fires() {
    // ★★★ The routing, end to end. A graph-added invariant becomes a typed
    // `AddInv`, which CAN strand — so `admit_edit` runs Safe(e,μ) over the live
    // instances and refuses, naming the instance and the rule.
    let d = definition();
    let g2 = get(&d).with(Node::Invariant { id: "ceiling".into() });
    let edits = put(&g2, &d, &Supplied::none().invariant("ceiling", "finances <= 900")).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].kind(), "AddInv");

    let violating = vec![Instance { id: "i1".into(), state: json!({"finances": 5000.0}) }];
    let verdict = admit_edit(
        &d,
        &edits[0],
        &violating,
        &Migration::Identity,
        &EditEffect::Sandbox,
    );
    assert!(!verdict.admitted());
    match verdict.verdict {
        Err(EditError::WouldStrand(ref stranded)) => {
            assert_eq!(stranded[0].instance_id, "i1");
            assert_eq!(stranded[0].invariant_id, "ceiling");
        }
        other => panic!("expected WouldStrand, got {other:?}"),
    }

    // The same graph edit against a compliant instance is admitted.
    let compliant = vec![Instance { id: "i2".into(), state: json!({"finances": 700.0}) }];
    assert!(admit_edit(&d, &edits[0], &compliant, &Migration::Identity, &EditEffect::Sandbox)
        .admitted());

    // And the registry is untouched by any of it — the lens reads a definition,
    // never an operator body.
    let _ = Registry::default();
}

#[test]
fn a_modified_expression_becomes_a_modify_inv_not_a_drop_and_add() {
    let d = definition();
    let edits = put(&get(&d), &d, &Supplied::none().invariant("floor", "finances >= 700")).unwrap();
    assert_eq!(edits.len(), 1, "one typed edit, not a demolition and a rebuild");
    assert_eq!(edits[0].kind(), "ModifyInv");
}

#[test]
fn supplying_the_identical_expression_is_not_an_edit() {
    let d = definition();
    assert!(put(&get(&d), &d, &Supplied::none().invariant("floor", "finances >= 500"))
        .unwrap()
        .is_empty());
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-ahead-of-python");

    let step0 = &doc["★★_the_STEP_0_reconcile_and_it_corrected_WHAT_π_FORGETS"];
    for key in [
        "1_no_projection_exists_in_this_core",
        "2_everything_put_needs_to_TARGET_already_exists",
        "3_and_everything_get_needs_to_READ_exists_too",
        "★★_4_the_correction_ENZYME_BODIES_ARE_NOT_IN_D_AT_ALL",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★ The correction that shaped the slice must stay on the record.
    assert!(step0["★★_4_the_correction_ENZYME_BODIES_ARE_NOT_IN_D_AT_ALL"]
        .as_str()
        .unwrap()
        .contains("would have been theatre"));

    // The design decision behind the literal PutGet.
    let design = &doc["★★_the_design_decision_that_makes_PutGet_hold_LITERALLY"];
    assert!(design["★★_why_that_was_rejected"]
        .as_str()
        .unwrap()
        .contains("weaker claim wearing the same name"));

    let cw = d["★★_the_counterweight_EVERYTHING_THIS_LENS_TARGETS_AND_READS_ALREADY_EXISTED"]
        .as_str()
        .unwrap();
    for term in ["admit_edit", "EDIT-11", "referenced_dimensions", "read-only"] {
        assert!(cw.contains(term), "missing counterweight detail: {term}");
    }

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(
        fp.keys().any(|k| k.contains("project / projection")),
        "the clamp-projection false positive must stay named"
    );
    assert!(fp.keys().any(|k| k.contains("Edge (a collision")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE LENS IS THE ROW; THE EDITOR IS NOT",
        "FORGETS EDGES BETWEEN OPERATORS AND DIMENSIONS",
        "AN UNPARSEABLE INVARIANT CONTRIBUTES NO EDGES BUT STILL APPEARS AS A NODE",
        "APPLIES EDITS DIRECTLY, NOT THROUGH `admit_edit`",
        "THE GRAPH IS TOP-LEVEL-DIMENSION GRANULAR",
        "DOES NOT REORDER",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["law_cases", "forgetting_cases", "typed_edit_cases"] {
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
