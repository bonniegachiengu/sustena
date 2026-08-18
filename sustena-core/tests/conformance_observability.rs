//! Observability, replayed against its conformance vectors
//! (R2 · Monitor §I + MON-11's fit caveat · MON-1).
//!
//! **SPEC vectors, Rust-only — and the literal matrix is DECLINED on both
//! sides.** §I's `rank([C; CA; …; CAⁿ⁻¹]) = n` is not built here, and that is a
//! position rather than a gap: the same one MON-2 takes on the Kalman filter,
//! resting on the same ratified fit caveat. What ships is the discrete
//! analogue — reachability to a measurement over the operator graph — and the
//! correspondence is a translation, not an approximation.
//!
//! ★ The counterweight has two halves: MON-8's `ungoverned()` shipped the
//! simplest case hours earlier, and the matrix's absence in the reference is
//! not a shortfall there either. What is new is the middle — reachability
//! through operator chains.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    Change, EffectSummary, MeasuredBy, Movement, Observability, ObservabilityError,
    ObservabilityGraph, OperatorMeta, OperatorResult, Registry, State, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("observability.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "observability.json was written for a different contract version"
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

// ── the fixture, built from the vector ──────────────────────────────────────

fn noop(
    _s: &mut State,
    _p: &Map<String, Value>,
    _e: &mut Vec<sustena_core::operator::EmittedEvent>,
    _m: &mut Vec<Movement>,
) -> OperatorResult {
    OperatorResult::ok(json!({}))
}

/// One declared operator: reads become guard predicates, writes an effect
/// summary. `writes: None` is an operator that declared no summary.
fn op(name: &'static str, reads: &[&str], writes: Option<&[&str]>) -> OperatorMeta {
    OperatorMeta {
        name,
        description: "",
        params: vec![],
        constraints: reads.iter().map(|r| format!("{r} >= 0")).collect(),
        post_constraints: vec![],
        side_effects: vec![],
        pawa_cost: 0,
        protocol: sustena_core::operator::meta::Protocol::Rpc,
        min_privilege: 0,
        effect: writes
            .map(|ws| ws.iter().fold(EffectSummary::new(), |e, p| e.with(p, Change::Opaque))),
        run: noop,
    }
}

/// Leaked to `'static` so the vector's names can drive `OperatorMeta`'s
/// `&'static str` fields — a test-only cost, and the alternative is restating
/// the fixture in Rust.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn registry(doc: &Value) -> Registry {
    let mut r = Registry::new();
    for spec in doc["fixture"]["operators"].as_array().unwrap() {
        let reads: Vec<&str> =
            spec["reads"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let writes: Vec<&str> =
            spec["writes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        r.register(op(leak(spec["name"].as_str().unwrap()), &reads, Some(&writes)));
    }
    r
}

fn sources(doc: &Value) -> Vec<MeasuredBy> {
    let m = &doc["fixture"]["measured"];
    let dims: Vec<&str> =
        m["dimensions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    vec![MeasuredBy::new(m["source_id"].as_str().unwrap(), &dims)]
}

fn governed(doc: &Value) -> Vec<&str> {
    doc["fixture"]["governed"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect()
}

// ── ★★ the translation of the matrix ───────────────────────────────────────

#[test]
fn a_directly_measured_dimension_is_the_c_row() {
    let doc = load();
    let c = case(&doc, "translation_cases", "★_a_DIRECTLY_MEASURED_dimension_is_the_C_ROW");
    let g = ObservabilityGraph::build(&registry(&doc), &sources(&doc)).unwrap();

    match g.observability_of("finances") {
        Observability::Measured { by } => {
            let want: Vec<String> =
                c["expect"]["by"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect();
            assert_eq!(by, want);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_dimension_one_operator_from_a_measurement_is_inferable_at_one_hop() {
    // ★★ `CA¹`, concretely.
    let doc = load();
    let c = case(
        &doc,
        "translation_cases",
        "★★_a_dimension_ONE_operator_from_a_measurement_is_INFERABLE_at_ONE_hop",
    );
    let g = ObservabilityGraph::build(&registry(&doc), &sources(&doc)).unwrap();

    match g.observability_of("derived") {
        Observability::Inferable { hops, reaches, via } => {
            assert_eq!(hops as u64, c["expect"]["hops"].as_u64().unwrap());
            assert_eq!(reaches, c["expect"]["reaches"].as_str().unwrap());
            assert_eq!(via, ["b.carry"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_chain_of_two_operators_is_inferable_at_two_hops_and_names_the_chain() {
    // ★★ `CA²` — and the verdict carries the route, so the claim is checkable.
    let doc = load();
    let c = case(
        &doc,
        "translation_cases",
        "★★_a_CHAIN_of_two_operators_is_inferable_at_TWO_hops_and_names_the_chain",
    );
    let g = ObservabilityGraph::build(&registry(&doc), &sources(&doc)).unwrap();

    match g.observability_of("sensor") {
        Observability::Inferable { hops, via, reaches } => {
            assert_eq!(hops as u64, c["expect"]["hops"].as_u64().unwrap());
            let want: Vec<String> = c["expect"]["via"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().into())
                .collect();
            assert_eq!(via, want);
            assert_eq!(reaches, c["expect"]["reaches"].as_str().unwrap());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_search_is_breadth_first_so_it_reports_the_fewest_hops() {
    // ★ "Which block row" is a question about the smallest k.
    let doc = load();
    let c = case(
        &doc,
        "translation_cases",
        "★_the_search_is_BREADTH_FIRST_so_it_reports_the_FEWEST_hops",
    );
    let mut r = registry(&doc);
    r.register(op("c.shortcut", &["sensor"], Some(&["finances.other"])));
    let g = ObservabilityGraph::build(&r, &sources(&doc)).unwrap();

    match g.observability_of("sensor") {
        Observability::Inferable { hops, .. } => {
            assert_eq!(hops as u64, c["expect"]["hops_with_shortcut"].as_u64().unwrap())
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_cycle_does_not_hang_the_search() {
    let doc = load();
    let mut r = Registry::new();
    r.register(op("x.to_y", &["x"], Some(&["y.v"])));
    r.register(op("y.to_x", &["y"], Some(&["x.v"])));
    let g = ObservabilityGraph::build(&r, &sources(&doc)).unwrap();
    assert!(!g.observability_of("x").is_observable());
}

// ── ★★ conservative where the effects are unsummarised ─────────────────────

#[test]
fn an_operator_with_no_effect_summary_contributes_no_edges_and_is_named() {
    // ★★ The safe direction — and the verdict says it is conservative rather
    // than presenting itself as definitive.
    let doc = load();
    let c = case(
        &doc,
        "conservative_cases",
        "★★_an_operator_with_NO_EFFECT_SUMMARY_contributes_no_edges_AND_IS_NAMED",
    );
    let mut r = Registry::new();
    r.register(op("a.opaque", &["sensor"], None));
    let g = ObservabilityGraph::build(&r, &sources(&doc)).unwrap();

    let o = g.observability_of("sensor");
    assert_eq!(o.is_observable(), c["expect"]["observable"].as_bool().unwrap());
    assert_eq!(o.is_conservative(), c["expect"]["is_conservative"].as_bool().unwrap());
    match &o {
        Observability::Unobservable { unsummarised } => {
            let want: Vec<String> = c["expect"]["unsummarised"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().into())
                .collect();
            assert_eq!(unsummarised, &want);
        }
        other => panic!("{other:?}"),
    }
    assert!(o.describe("sensor").contains(c["expect"]["describe_contains"].as_str().unwrap()));
}

#[test]
fn declaring_the_effect_turns_the_same_dimension_observable() {
    // ★ The gap between the two readings is a declaration, nothing else.
    let doc = load();
    let c = case(
        &doc,
        "conservative_cases",
        "★_DECLARING_the_effect_turns_the_SAME_dimension_observable",
    );

    let mut without = Registry::new();
    without.register(op("a.lift", &["sensor"], None));
    assert_eq!(
        ObservabilityGraph::build(&without, &sources(&doc))
            .unwrap()
            .observability_of("sensor")
            .is_observable(),
        c["expect"]["without_summary"].as_bool().unwrap()
    );

    let mut with = Registry::new();
    with.register(op("a.lift", &["sensor"], Some(&["finances.liquid"])));
    assert_eq!(
        ObservabilityGraph::build(&with, &sources(&doc))
            .unwrap()
            .observability_of("sensor")
            .is_observable(),
        c["expect"]["with_summary"].as_bool().unwrap()
    );
}

#[test]
fn an_opaque_change_still_makes_an_edge_because_opaque_is_about_the_value() {
    // Treating Opaque as no-write would have lost every parameter-driven
    // operator, which is most of them.
    let doc = load();
    let mut r = Registry::new();
    r.register(op("a.lift", &["sensor"], Some(&["finances.liquid"])));
    assert!(ObservabilityGraph::build(&r, &sources(&doc))
        .unwrap()
        .observability_of("sensor")
        .is_observable());
}

#[test]
fn over_the_real_registry_every_verdict_is_conservative() {
    // ★★ OP-3's finding from a second angle, asserted so a future summarised
    // operator breaks it and forces a look.
    let doc = load();
    let c = case(
        &doc,
        "conservative_cases",
        "★★_over_the_REAL_registry_EVERY_verdict_is_conservative",
    );
    let g = ObservabilityGraph::build(&Registry::with_builtins(), &sources(&doc)).unwrap();

    assert_eq!(
        !g.unsummarised().is_empty(),
        c["expect"]["unsummarised_nonempty"].as_bool().unwrap(),
        "no shipped operator declares an EffectSummary"
    );
    let o = g.observability_of("tasks");
    assert_eq!(o.is_observable(), c["expect"]["observable"].as_bool().unwrap());
    assert_eq!(o.is_conservative(), c["expect"]["is_conservative"].as_bool().unwrap());
}

// ── ★★ the design finding, and §I's implication ────────────────────────────

#[test]
fn a_governed_but_unobservable_dimension_is_a_design_finding() {
    // ★★ "Unmeasured state is ungoverned state" — the failure that never
    // appears as a symptom.
    let doc = load();
    let c = case(
        &doc,
        "finding_cases",
        "★★_a_GOVERNED_but_UNOBSERVABLE_dimension_is_a_DESIGN_FINDING",
    );
    let g = ObservabilityGraph::build(&registry(&doc), &sources(&doc)).unwrap();
    let report = g.report(&governed(&doc));

    let want: Vec<&str> = c["expect"]["declared_but_unobservable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(report.declared_but_unobservable(), want);
    assert_eq!(
        report.completely_observable(),
        c["expect"]["completely_observable"].as_bool().unwrap()
    );
    // Every finding here is conservative-free: these operators declared effects.
    assert!(report.conservative().is_empty());
}

#[test]
fn everything_reachable_reads_as_completely_observable() {
    // A checker that could only report problems would be untrustworthy about
    // the ones it did report.
    let doc = load();
    let g = ObservabilityGraph::build(&registry(&doc), &sources(&doc)).unwrap();
    assert!(g.report(&["finances", "derived", "sensor"]).completely_observable());
}

#[test]
fn an_unobservable_dimension_is_exactly_one_that_needs_a_belief() {
    // ★★ §I's *partial observability → a distribution, not a point*, wired to
    // where that distribution lives (MON-8). The two rows meet here.
    let doc = load();
    let c = case(
        &doc,
        "finding_cases",
        "★★_an_UNOBSERVABLE_dimension_is_EXACTLY_one_that_needs_a_BELIEF",
    );
    let g = ObservabilityGraph::build(&registry(&doc), &sources(&doc)).unwrap();
    let report = g.report(&governed(&doc));

    let want: Vec<&str> = c["expect"]["requiring_belief"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(report.requiring_belief(), want);
    assert_eq!(
        g.observability_of("finances").requires_belief(),
        c["expect"]["measured_requires_belief"].as_bool().unwrap()
    );
}

#[test]
fn with_nothing_measured_the_graph_refuses_rather_than_reporting_all_unobservable() {
    // ★ A report full of findings would read as "this system is badly
    // designed" when it means "you told me about no sensors".
    let doc = load();
    assert_eq!(
        ObservabilityGraph::build(&registry(&doc), &[]),
        Err(ObservabilityError::NothingMeasured)
    );
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The matrix is declined, with the concrete reason and its precedents.
    let decl = doc["★★_the_matrix_is_DECLINED_and_here_is_the_CONCRETE_reason"].as_str().unwrap();
    assert!(decl.contains("MON-2") && decl.contains("MON-11"), "cross-references the precedents");
    assert!(decl.contains("needs a single `A` to take powers of"), "the concrete reason, not only the categorical one");
    assert!(decl.contains("worse than none"), "and the row's own warning");

    // ★★ And the replacement is a translation, term by term.
    let t = &doc["★★_and_the_replacement_is_a_TRANSLATION_not_an_approximation"];
    assert!(t["rank(O) = n"].as_str().unwrap().contains("completely_observable"));
    assert!(t["★_the_edge_direction_stated_carefully"]
        .as_str()
        .unwrap()
        .contains("Getting this backwards"));

    // ★ The counterweight, and the runtime-vs-structural distinction.
    let key = "★_the_counterweight_the_SIMPLEST_CASE_shipped_hours_ago_and_the_MATRIX_is_declined_not_missing";
    assert!(d[key].as_str().unwrap().contains("ungoverned()"));
    assert!(d["★★_and_the_difference_from_ungoverned_is_RUNTIME_versus_STRUCTURAL"]
        .as_str()
        .unwrap()
        .contains("answerable by waiting"));

    // The false positives, including the corrected `reachable` count.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("reachable (6 hits")), "6 prose hits, not a zero");
    assert!(fp.keys().any(|k| k.starts_with("monitor")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE MATRIX IS DECLINED, NOT DEFERRED",
        "CONSERVATIVE WHERE EFFECTS ARE UNSUMMARISED",
        "ROOT-NAME GRANULARITY",
        "AN UNPARSEABLE CONSTRAINT CONTRIBUTES NO READS",
        "A PATH IS NOT AN ESTIMATOR",
        "NOT WIRED INTO `MonitorEngine`",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }
    // ★ The one limit that is coarse in the UNSAFE direction is said so.
    assert!(limits.contains("less safe one"));

    let groups = ["translation_cases", "conservative_cases", "finding_cases"];
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
