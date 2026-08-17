//! `Π`, the strategy DAG, replayed against its conformance vectors
//! (R1+R2 · Operative §III · OPV-4, measured against `operative_graph.py`).
//!
//! ★★ The deep bundle's one row **with a parity half**. The walk, `$dot.path`,
//! the conditional-edge shape and the step guard are the reference's and are
//! ported; three things are sharper, and one of them is a finding about the
//! reference's call path rather than an improvement on it.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    operative::Shared,
    operator::{Enforcement, Registry},
    schema::Schema,
    strategy::{reachable_under, run, Condition, StrategyError, StrategyGraph, WalkOutcome},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("strategy.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "strategy.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn world() -> Shared {
    Shared::new().with_move("budget.record_income").with_move("budget.allocate")
}

fn kwargs(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

fn allowed() -> Vec<String> {
    vec!["budget.record_income".to_string(), "budget.allocate".to_string()]
}

fn state() -> Value {
    json!({"finances": {
        "liquid": {"balance": 0.0},
        "income": {"monthly_total": 0.0, "sources": []},
        "pockets": {}
    }})
}

fn quiet() -> Enforcement {
    Enforcement { enabled: false, schema: Some(Schema::new()), ..Default::default() }
}

fn income(id: &str, amount: f64) -> (String, Map<String, Value>) {
    (
        "budget.record_income".to_string(),
        kwargs(&[
            ("amount", json!(amount)),
            ("source", json!("wages")),
            ("entry_id", json!(id)),
        ]),
    )
}

// ── Proposition 3 ────────────────────────────────────────────────────────────

#[test]
fn a_node_outside_t_is_refused_at_build_and_is_unconstructible() {
    // ★★★ `StrategyNode` holds a `LegalMove`, mintable only by `Shared`.
    let err = StrategyGraph::new("a", "a")
        .with_node(&world(), "a", "egress.prepare_household_summary", Map::new())
        .unwrap_err();
    assert_eq!(
        err,
        StrategyError::MoveNotInT {
            node: "a".into(),
            mv: "egress.prepare_household_summary".into()
        }
    );
}

#[test]
fn every_move_a_strategy_names_is_in_t() {
    let w = world();
    let pi = StrategyGraph::new("a", "b")
        .with_node(&w, "a", "budget.record_income", Map::new())
        .unwrap()
        .with_node(&w, "b", "budget.allocate", Map::new())
        .unwrap();
    let t: BTreeSet<&str> = w.moves().into_iter().collect();
    assert!(pi.moves().is_subset(&t), "Π ⊆ T, by construction");
}

#[test]
fn a_strategy_reaches_no_state_t_did_not_already_allow() {
    // ★★★ The proposition, as a set comparison.
    let w = Shared::new()
        .with_move("m1")
        .with_move("m2")
        .with_transition("s0", "m1", "s1")
        .unwrap()
        .with_transition("s1", "m2", "s2")
        .unwrap();
    let pi = StrategyGraph::new("a", "a").with_node(&w, "a", "m1", Map::new()).unwrap();

    let by_t = w.reachable("s0");
    let by_pi = reachable_under(&pi, &w, "s0");
    assert!(by_pi.is_subset(&by_t), "a strategy adds structure, not capability");
    assert!(by_t.contains("s2") && !by_pi.contains("s2"), "and it can only narrow");
}

// ── the walk ─────────────────────────────────────────────────────────────────

#[test]
fn the_walk_runs_entry_to_exit_and_threads_state() {
    let w = world();
    let (op, kw) = income("e1", 1000.0);
    let pi = StrategyGraph::new("earn", "spend")
        .with_node(&w, "earn", &op, kw)
        .unwrap()
        .with_node(
            &w,
            "spend",
            "budget.allocate",
            kwargs(&[
                ("pocket_name", json!("food")),
                ("amount", json!(400.0)),
                ("period", json!("monthly")),
            ]),
        )
        .unwrap()
        .with_edge("earn", "spend", None);
    pi.typecheck().unwrap();

    let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
    assert_eq!(walk.outcome, WalkOutcome::ReachedExit);
    assert_eq!(walk.steps.len(), 2);
    assert!(walk.steps.iter().all(|s| s.committed));
    assert_eq!(walk.state["finances"]["liquid"]["balance"], json!(600.0), "state threads");
}

#[test]
fn a_dollar_path_kwarg_is_resolved_from_an_earlier_result() {
    // ★ Proven by consequence: 250 in, `$earn.amount` out, balance back to 0.
    let w = world();
    let (op, kw) = income("e1", 250.0);
    let pi = StrategyGraph::new("earn", "spend")
        .with_node(&w, "earn", &op, kw)
        .unwrap()
        .with_node(
            &w,
            "spend",
            "budget.allocate",
            kwargs(&[
                ("pocket_name", json!("food")),
                ("amount", json!("$earn.amount")),
                ("period", json!("monthly")),
            ]),
        )
        .unwrap()
        .with_edge("earn", "spend", None);

    let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
    assert_eq!(walk.outcome, WalkOutcome::ReachedExit);
    assert_eq!(walk.state["finances"]["liquid"]["balance"], json!(0.0));
}

#[test]
fn a_conditional_edge_routes_on_the_producing_nodes_result() {
    let w = world();
    let (op, kw) = income("e1", 10.0);
    let pi = StrategyGraph::new("earn", "spend")
        .with_node(&w, "earn", &op, kw)
        .unwrap()
        .with_node(
            &w,
            "spend",
            "budget.allocate",
            kwargs(&[
                ("pocket_name", json!("food")),
                ("amount", json!(5.0)),
                ("period", json!("monthly")),
            ]),
        )
        .unwrap()
        .with_edge("earn", "spend", Condition::new("amount", ">", json!(100.0)));

    let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
    assert_eq!(walk.outcome, WalkOutcome::NoMatchingEdge { at: "earn".into() });
    assert_eq!(walk.steps.len(), 1, "the second node never ran");
}

#[test]
fn an_absent_condition_field_reads_false_rather_than_erroring() {
    let c = Condition::new("nowhere.at.all", ">", json!(1)).unwrap();
    assert!(!c.holds(&json!({"amount": 500})));
}

#[test]
fn a_type_mismatch_reads_false_too() {
    let c = Condition::new("amount", ">", json!(1)).unwrap();
    assert!(!c.holds(&json!({"amount": "lots"})));
}

#[test]
fn a_cycle_is_bounded_and_the_exhaustion_is_reported() {
    // ★ The reference stops silently here; this says so.
    let w = world();
    let (op, kw) = income("e1", 1.0);
    let pi = StrategyGraph::new("a", "never")
        .with_node(&w, "a", &op, kw)
        .unwrap()
        .with_edge("a", "a", None);

    let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
    assert_eq!(walk.outcome, WalkOutcome::StepsExhausted { limit: 10 });
    assert!(!walk.outcome.finished(), "it ran out; it did not finish");
    assert_eq!(walk.steps.len(), 10);
}

// ── the gate ─────────────────────────────────────────────────────────────────

#[test]
fn a_node_the_gate_refuses_stops_the_walk_honestly() {
    // ★★★ Π gets no bypass.
    let w = world();
    let before = state();
    let pi = StrategyGraph::new("spend", "spend")
        .with_node(
            &w,
            "spend",
            "budget.allocate",
            kwargs(&[
                ("pocket_name", json!("food")),
                ("amount", json!(999.0)),
                ("period", json!("monthly")),
            ]),
        )
        .unwrap();

    let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &before, json!({}));
    match &walk.outcome {
        WalkOutcome::Refused { node, reason } => {
            assert_eq!(node, "spend");
            assert!(!reason.is_empty(), "the gate's own reason travels");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(walk.state, before, "nothing committed");
}

#[test]
fn an_operator_the_sustain_does_not_allow_is_refused_at_the_gate() {
    // ★ Two containments: `T` at build, the sustain's allow-list at the gate.
    let w = world();
    let (op, kw) = income("e1", 1.0);
    let pi = StrategyGraph::new("earn", "earn").with_node(&w, "earn", &op, kw).unwrap();
    let walk = run(&pi, &Registry::default(), &[], &quiet(), &state(), json!({}));
    assert!(matches!(walk.outcome, WalkOutcome::Refused { .. }));
}

// ── build-time shape and calibration ─────────────────────────────────────────

#[test]
fn typecheck_catches_a_dangling_edge_and_a_missing_endpoint() {
    let w = world();
    let base = StrategyGraph::new("a", "a")
        .with_node(&w, "a", "budget.allocate", Map::new())
        .unwrap();
    assert!(matches!(
        base.clone().with_edge("a", "ghost", None).typecheck(),
        Err(StrategyError::DanglingEdge { .. })
    ));
    assert!(matches!(
        StrategyGraph::new("a", "missing")
            .with_node(&w, "a", "budget.allocate", Map::new())
            .unwrap()
            .typecheck(),
        Err(StrategyError::MissingEndpoint { which: "exit", .. })
    ));
}

#[test]
fn calibration_resolves_a_placeholder_and_refuses_an_unsupplied_one() {
    let w = world();
    let pi = StrategyGraph::new("a", "a")
        .with_node(&w, "a", "budget.allocate", kwargs(&[("amount", json!("{{limit}}"))]))
        .unwrap();

    let cal: Map<String, Value> = [("limit".to_string(), json!(500.0))].into_iter().collect();
    let done = pi.clone().calibrated(&cal).unwrap();
    assert_eq!(done.nodes().next().unwrap().kwargs()["amount"], json!(500.0));

    assert!(matches!(
        pi.calibrated(&Map::new()),
        Err(StrategyError::Uncalibrated { token, .. }) if token == "limit"
    ));
}

#[test]
fn the_two_resolution_passes_do_not_see_each_others_syntax() {
    let w = world();
    let pi = StrategyGraph::new("a", "a")
        .with_node(&w, "a", "budget.allocate", kwargs(&[("amount", json!("$earn.amount"))]))
        .unwrap()
        .calibrated(&Map::new())
        .unwrap();
    assert_eq!(pi.nodes().next().unwrap().kwargs()["amount"], json!("$earn.amount"));
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "parity-plus-sharpening");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★_1_ω_has_NO_Π_slot_and_that_was_DELIBERATE",
        "★★★_2_but_the_MECHANISM_Prop_3_needs_was_already_there",
        "★_3_and_the_bound_comes_from_the_thing_it_is_a_bound_ON",
        "4_Π_is_not_the_router",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★ OPV-1's restraint must stay credited, not silently corrected.
    assert!(step0["★★_1_ω_has_NO_Π_slot_and_that_was_DELIBERATE"]
        .as_str()
        .unwrap()
        .contains("reads as built"));

    // ★★★ The finding, and its counterweight, both intact.
    let finding = d["★★★_the_finding_the_reference's_graph_calls_operators_OUTSIDE_the_gate"]
        .as_str()
        .unwrap();
    assert!(finding.contains("op_meta.fn"), "the mechanism must stay named");
    assert!(
        finding.contains("operatives advise, the human decides"),
        "the counterweight must stay — the graph's output is a proposal"
    );
    assert!(finding.contains("shape"), "finding-not-alarm framing must stay");

    assert!(d["★★_sharper_2_Prop_3_is_a_TYPE_not_a_runtime_check"]
        .as_str()
        .unwrap()
        .contains("unconstructible"));
    assert!(d["★_sharper_3_the_cycle_guard_REPORTS_rather_than_truncating"]
        .as_str()
        .unwrap()
        .contains("look alike to the caller"));
    // ★ The small difference must stay recorded rather than rounded away.
    assert!(d["★_a_small_real_difference_worth_naming_rather_than_glossing"]
        .as_str()
        .unwrap()
        .contains("not lists"));

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.contains("Strategy (a collision")));
    assert!(fp.keys().any(|k| k.contains("graph (many real hits")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE WALK DOES NOT BUILD A PROPOSAL",
        "AUTHORIZATION IS `Unchecked` AND EFFECTS ARE `Unchecked` FOR NOW",
        "NO TOPOLOGICAL VALIDATION BEYOND SHAPE",
        "FIRST MATCHING EDGE WINS",
        "STATE THREADS",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["proposition_3_cases", "walk_cases", "gate_cases", "build_cases"] {
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
