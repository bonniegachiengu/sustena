//! The Physarum router replayed against its conformance vectors
//! (R2 · Multiparty §VIII · MUL-13, and the second half of MUL-15).
//!
//! **SPEC vectors, Rust-only.** `physarum`, `conductance`, `kirchhoff`,
//! `flux`, `reinforce` and `shortest_path` are grep-0 in the reference. It
//! routes in the **dispatch** sense — it sends things to declared destinations
//! — and never in the **adaptive** sense: no edge in it gets thicker for
//! carrying load.
//!
//! The nearest miss is `operative_graph.py`'s *"edge-routing time"*, a declared
//! DAG whose edges carry conditions. Real routing; the edges never change.
//!
//! The proof of value: two routes start identical, and flow alone reinforces
//! the short one and prunes the long one.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    Population, PopulationSpec, Reinforcement, Router, RouterError, RouterSpec,
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("router.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "router.json was written for a different contract version"
    );
    doc
}

// ── fixture, built from the vector file ─────────────────────────────────────

fn spec_of(doc: &Value) -> RouterSpec {
    let s = &doc["fixture"]["spec"];
    let mu = s["mu"].as_f64().unwrap();
    let f = if s["saturating"].as_bool().unwrap() {
        Reinforcement::saturating(mu).unwrap()
    } else {
        Reinforcement::power(mu).unwrap()
    };
    RouterSpec::new(s["dt"].as_f64().unwrap(), f, s["prune_below"].as_f64().unwrap()).unwrap()
}

fn two_routes(doc: &Value) -> Router {
    let mut r = Router::new(spec_of(doc));
    for n in doc["fixture"]["nodes"].as_array().unwrap() {
        r = r.with_node(n.as_str().unwrap());
    }
    for e in doc["fixture"]["edges"].as_array().unwrap() {
        r = r.linking(
            e["from"].as_str().unwrap(),
            e["to"].as_str().unwrap(),
            e["length"].as_f64().unwrap(),
            e["conductivity"].as_f64().unwrap(),
        );
    }
    r
}

fn src(doc: &Value) -> String {
    doc["fixture"]["source"].as_str().unwrap().to_string()
}
fn snk(doc: &Value) -> String {
    doc["fixture"]["sink"].as_str().unwrap().to_string()
}
fn rate(doc: &Value) -> f64 {
    doc["fixture"]["rate"].as_f64().unwrap()
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from router.json"))
}

// ── the flow solve ──────────────────────────────────────────────────────────

#[test]
fn flow_is_conserved_at_every_interior_node() {
    let doc = load();
    let c = case(&doc, "flow_cases", "flow_is_conserved_at_every_interior_node");
    let r = two_routes(&doc);
    let q = r.flux(&r.solve_flow(&src(&doc), &snk(&doc), rate(&doc)).unwrap());
    let e = &c["expect"];

    // Edges in declaration order: (s,a) (a,t) (s,b) (b,t).
    assert_eq!((q[0] - q[1]).abs() < 1e-9, e["conserved_at_a"].as_bool().unwrap());
    assert_eq!((q[2] - q[3]).abs() < 1e-9, e["conserved_at_b"].as_bool().unwrap());
    assert!(
        (q[0] + q[2] - e["sums_to_rate"].as_f64().unwrap()).abs() < 1e-9,
        "★ physics, not planning — nothing is lost or invented"
    );
}

#[test]
fn the_shorter_route_carries_more_flow_before_any_adaptation() {
    let doc = load();
    let c = case(&doc, "flow_cases", "the_shorter_route_carries_more_flow_before_any_adaptation");
    let r = two_routes(&doc);
    let q = r.flux(&r.solve_flow(&src(&doc), &snk(&doc), rate(&doc)).unwrap());

    assert!(
        (q[0].abs() / q[2].abs() - c["expect"]["short_over_long"].as_f64().unwrap()).abs() < 1e-9,
        "★ resistance 2 against 6 — physics alone has already found the shorter route"
    );
}

#[test]
fn the_flow_solve_refuses_what_it_cannot_honestly_answer() {
    let doc = load();

    let _ = case(&doc, "flow_cases", "a_disconnected_sink_is_refused_not_zeroed");
    let island = Router::new(spec_of(&doc))
        .with_node("s")
        .with_node("t")
        .with_node("island")
        .linking("s", "t", 1.0, 1.0);
    assert!(
        matches!(island.solve_flow("s", "island", 1.0), Err(RouterError::NoRoute { .. })),
        "★ 'no flow needed' and 'no route exists' are different answers"
    );

    let _ = case(&doc, "flow_cases", "a_source_that_is_also_the_sink_is_refused");
    let r = two_routes(&doc);
    assert!(matches!(r.solve_flow("s", "s", 1.0), Err(RouterError::SourceIsSink(_))));
}

// ── ★★ the adaptation ───────────────────────────────────────────────────────

#[test]
fn flow_reinforces_the_load_bearing_channel_and_prunes_the_unused_one() {
    let doc = load();
    let c = case(&doc, "adaptation_cases", "flow_reinforces_the_load_bearing_channel_and_prunes_the_unused_one");
    let mut r = two_routes(&doc);

    let conv = r.converge(&src(&doc), &snk(&doc), rate(&doc), 500, 1e-9).unwrap();
    let e = &c["expect"];
    assert_eq!(conv.converged, e["converged"].as_bool().unwrap(), "settled in {}", conv.steps);

    let short = r.conductivity("s", "a").unwrap();
    let long = r.conductivity("s", "b").unwrap();
    assert_eq!(
        short > long,
        e["short_exceeds_long"].as_bool().unwrap(),
        "★★ short {short} vs long {long} — and no planner was consulted"
    );

    let touched = |set: Vec<&sustena_core::Edge>, id: &str| {
        set.iter().any(|x| x.a == id || x.b == id)
    };
    assert!(touched(r.carrying(), e["carrying_includes"].as_str().unwrap()));
    assert!(touched(r.pruned(), e["pruned_includes"].as_str().unwrap()));
}

#[test]
fn the_reinforcement_sees_only_its_own_edges_flux() {
    let doc = load();
    let c = case(&doc, "adaptation_cases", "the_reinforcement_sees_only_its_own_edges_flux");
    let e = &c["expect"];

    let f = Reinforcement::power(1.0).unwrap();
    assert_eq!(f.apply(2.0), e["power_of_2"].as_f64().unwrap());
    assert_eq!(f.apply(-2.0) == f.apply(2.0), e["sign_ignored"].as_bool().unwrap());
    assert_eq!(f.is_monotone(), e["monotone"].as_bool().unwrap());

    let sat = Reinforcement::saturating(1.0).unwrap();
    assert_eq!(
        sat.apply(1000.0) < 1.0,
        e["saturating_bounded"].as_bool().unwrap(),
        "a channel cannot thicken without bound"
    );
    assert!(sat.apply(2.0) > sat.apply(1.0), "and it is still monotone");
}

#[test]
fn an_unused_channel_decays_toward_zero_without_ever_reaching_it() {
    let doc = load();
    let c = case(&doc, "adaptation_cases", "an_unused_channel_decays_toward_zero_without_ever_reaching_it");
    let mut r = two_routes(&doc);
    r.converge(&src(&doc), &snk(&doc), rate(&doc), 500, 1e-12).unwrap();

    let long = r.conductivity("s", "b").unwrap();
    let e = &c["expect"];
    assert_eq!(long > 0.0, e["still_positive"].as_bool().unwrap(), "asymptotic: {long}");
    assert_eq!(
        long < doc["fixture"]["spec"]["prune_below"].as_f64().unwrap(),
        e["below_cutoff"].as_bool().unwrap()
    );
    assert_eq!(
        r.edges().len(),
        e["edge_count_unchanged"].as_u64().unwrap() as usize,
        "★ 'pruned' is a reading at a cutoff — nothing was deleted"
    );
}

#[test]
fn equal_routes_stay_equal() {
    let doc = load();
    let c = case(&doc, "adaptation_cases", "equal_routes_stay_equal");
    let mut r = Router::new(spec_of(&doc))
        .with_node("s")
        .with_node("a")
        .with_node("b")
        .with_node("t")
        .linking("s", "a", 1.0, 1.0)
        .linking("a", "t", 1.0, 1.0)
        .linking("s", "b", 1.0, 1.0)
        .linking("b", "t", 1.0, 1.0);

    r.converge(&src(&doc), &snk(&doc), rate(&doc), 500, 1e-9).unwrap();
    let x = r.conductivity("s", "a").unwrap();
    let y = r.conductivity("s", "b").unwrap();

    assert!(
        (x - y).abs() < c["expect"]["stay_within"].as_f64().unwrap(),
        "★ a tie-break here would be a planner in disguise: {x} vs {y}"
    );
}

#[test]
fn a_run_that_does_not_settle_says_so() {
    let doc = load();
    let c = case(&doc, "adaptation_cases", "a_run_that_does_not_settle_says_so");
    let mut r = two_routes(&doc);
    let conv = r
        .converge(
            &src(&doc),
            &snk(&doc),
            rate(&doc),
            c["max_steps"].as_u64().unwrap() as usize,
            1e-12,
        )
        .unwrap();

    assert_eq!(conv.converged, c["expect"]["converged"].as_bool().unwrap());
    assert_eq!(conv.steps, c["expect"]["steps"].as_u64().unwrap() as usize);
}

// ── ★ composed, not rebuilt ─────────────────────────────────────────────────

#[test]
fn a_router_runs_over_the_populations_declared_topology() {
    let doc = load();
    let c = case(&doc, "composition_cases", "a_router_runs_over_the_populations_declared_topology");

    let mut p = Population::new(PopulationSpec::new(1.0, 0.0).unwrap());
    for n in ["s", "a", "t"] {
        p = p.with_node(n);
    }
    for e in c["population_edges"].as_array().unwrap() {
        let a = e.as_array().unwrap();
        p = p.linking(a[0].as_str().unwrap(), a[1].as_str().unwrap(), a[2].as_f64().unwrap());
    }

    let r = Router::over(&p, spec_of(&doc), 1.0).unwrap();
    let e = &c["expect"];
    assert_eq!(r.nodes().len(), e["nodes"].as_u64().unwrap() as usize);
    assert_eq!(r.edges().len(), e["edges"].as_u64().unwrap() as usize);

    // `over` stores each symmetric pair once with a < b, so match either way.
    let find = |x: &str, y: &str| {
        r.edges()
            .iter()
            .find(|ed| (ed.a == x && ed.b == y) || (ed.a == y && ed.b == x))
            .expect("declared edge")
    };
    assert_eq!(
        find("s", "a").length,
        e["s_a_length"].as_f64().unwrap(),
        "★ the population's d(i,j) IS L_ij — one topology, two readings"
    );
    assert_eq!(find("a", "t").length, e["a_t_length"].as_f64().unwrap());
}

#[test]
fn a_population_with_no_edges_has_nothing_to_route_on() {
    let doc = load();
    let _ = case(&doc, "composition_cases", "a_population_with_no_edges_has_nothing_to_route_on");
    let p = Population::new(PopulationSpec::new(1.0, 0.0).unwrap()).with_node("alone");
    assert!(matches!(Router::over(&p, spec_of(&doc), 1.0), Err(RouterError::NoEdges)));
}

// ── declared parameters ─────────────────────────────────────────────────────

#[test]
fn the_declared_parameters_are_checked() {
    let doc = load();
    let c = case(&doc, "declared_cases", "the_declared_parameters_are_checked");
    let e = &c["expect"];

    assert!(matches!(Reinforcement::power(0.0), Err(RouterError::BadExponent(_))));
    assert_eq!(e["mu_zero"].as_str().unwrap(), "BadExponent");

    let f = Reinforcement::power(1.0).unwrap();
    assert!(matches!(RouterSpec::new(0.0, f, 0.1), Err(RouterError::BadStep(_))));
    assert!(
        matches!(RouterSpec::new(1.5, f, 0.1), Err(RouterError::BadStep(_))),
        "a step above 1 overshoots the fixed point"
    );
    assert_eq!(e["dt_above_one"].as_str().unwrap(), "BadStep");
}

#[test]
fn an_edge_that_starts_at_zero_could_never_be_reinforced() {
    let doc = load();
    let _ = case(&doc, "declared_cases", "an_edge_that_starts_at_zero_could_never_be_reinforced");
    let mut r = Router::new(spec_of(&doc)).with_node("a").with_node("b");
    assert!(
        matches!(r.link("a", "b", 1.0, 0.0), Err(RouterError::BadConductivity(_))),
        "★ zero weight ⇒ zero flux ⇒ f(0) − 0 = 0 — it would sit at zero forever"
    );
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The distinction the whole divergence turns on.
    let s = d["statement"].as_str().unwrap();
    assert!(s.contains("DISPATCH sense") && s.contains("ADAPTIVE sense"));

    // The nearest miss is named as real routing, not dismissed.
    let fp = &d["the_two_greppable_false_positives_named_so_nobody_re_derives_them"];
    let routing = fp["routing (6 hits)"].as_str().unwrap();
    assert!(routing.contains("That IS routing, and it is real"));
    assert!(routing.contains("not is adaptive"));

    // The counterweight names both shipped things and the exact difference.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("procurement.py") && cw.contains("OperativeGraph"));
    assert!(
        cw.contains("a dispatcher picks; a medium adapts"),
        "the distinction, stated in one line"
    );

    // "No global view" is not overclaimed.
    let g = d["★_what_is_global_here_and_what_is_not"].as_str().unwrap();
    assert!(g.contains("easy to overclaim"));
    assert!(g.contains("physics, not planning"));

    // All three honest limits recorded.
    let lim = d["the_honest_limits"].as_str().unwrap();
    for k in ["FIXED-POINT ITERATION", "ASYMPTOTIC", "REFUSED, not zeroed"] {
        assert!(lim.contains(k), "the limits must record {k}");
    }

    // One term is only PARTLY at parity, in intent rather than mechanism.
    assert!(
        d["term_by_term"]["'This is Mycelium' (§VIII)"]
            .as_str()
            .unwrap()
            .contains("PARTLY AT PARITY IN INTENT, not in mechanism")
    );

    for group in ["flow_cases", "adaptation_cases", "composition_cases", "declared_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in ["flow_cases", "adaptation_cases", "composition_cases", "declared_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
