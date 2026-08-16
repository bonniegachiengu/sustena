//! The population and threshold commitment replayed against their conformance
//! vectors (R2 · Multiparty §I + §II · MUL-1, MUL-2, MUL-3).
//!
//! **SPEC vectors, Rust-only.** `quorum_sens*`, `neighbourhood`,
//! `concentration`, `cascade` and `bifurcation` are grep-0 in the reference.
//!
//! **The counterweight IS MUL-3.** The reference *has* a quorum — `CouncilSession`
//! counting votes on a raised proposal — and that is §V's: an intersecting
//! majority deciding what a formed body may **ratify**. What it lacks is §II's:
//! a count against a threshold deciding whether a body **forms at all**. So it
//! sits squarely on one side of the distinction the article calls *"the classic
//! error"*.
//!
//! The proof of value: below θ nothing commits; above it, one seed takes the
//! whole ring.
//!
//! See `conformance/README.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{sweep, Population, PopulationError, PopulationSpec, CONFORMANCE_VERSION};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("population.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "population.json was written for a different contract version"
    );
    doc
}

// ── fixture, built from the vector file ─────────────────────────────────────

fn ring(doc: &Value, theta: f64) -> Population {
    let f = &doc["fixture"];
    let d = f["distance"].as_f64().unwrap();
    let mut p = Population::new(PopulationSpec::new(theta, f["lambda"].as_f64().unwrap()).unwrap());
    for n in f["nodes"].as_array().unwrap() {
        p = p.with_node(n.as_str().unwrap());
    }
    for e in f["edges"].as_array().unwrap() {
        let a = e.as_array().unwrap();
        p = p.linking(a[0].as_str().unwrap(), a[1].as_str().unwrap(), d);
    }
    p
}

fn seed_of(doc: &Value) -> String {
    doc["fixture"]["seed"].as_str().unwrap().to_string()
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from population.json"))
}

// ── §I — the node ───────────────────────────────────────────────────────────

#[test]
fn a_node_senses_only_its_own_neighbourhood() {
    let doc = load();
    let c = case(&doc, "node_cases", "a_node_senses_only_its_own_neighbourhood");
    let mut p = ring(&doc, 0.5);

    p.seed(c["commit"].as_str().unwrap()).unwrap();
    let view = p.local_view(c["observer"].as_str().unwrap()).unwrap();
    let e = &c["expect"];

    assert_eq!(
        view.concentration,
        e["concentration"].as_f64().unwrap(),
        "★★ no node ever sees P — a commit across the ring is invisible here"
    );
    assert_eq!(view.committed_neighbours, e["committed_neighbours"].as_u64().unwrap() as usize);
    assert_eq!(view.neighbours, e["neighbours"].as_u64().unwrap() as usize);

    // The observer CAN see it — which is why that accessor is named for the
    // observer and is never an input to a step.
    assert_eq!(p.committed_fraction() > 0.0, e["observer_can_see_it"].as_bool().unwrap());
}

#[test]
fn a_node_cannot_be_its_own_neighbour() {
    let doc = load();
    let _ = case(&doc, "node_cases", "a_node_cannot_be_its_own_neighbour");
    let mut p = ring(&doc, 0.5);
    assert!(
        matches!(p.link("a", "a", 1.0), Err(PopulationError::SelfNeighbour(_))),
        "N(i) ⊆ P \\ {{n_i}} — a body must not form out of one member agreeing with itself"
    );
}

#[test]
fn the_declared_parameters_are_checked() {
    let doc = load();
    let c = case(&doc, "node_cases", "the_declared_parameters_are_checked");
    let e = &c["expect"];

    assert!(matches!(PopulationSpec::new(0.0, 1.0), Err(PopulationError::BadTheta(_))));
    assert_eq!(e["theta_zero"].as_str().unwrap(), "BadTheta");
    assert!(matches!(PopulationSpec::new(-1.0, 1.0), Err(PopulationError::BadTheta(_))));
    assert!(matches!(PopulationSpec::new(1.0, -1.0), Err(PopulationError::BadLambda(_))));
    assert_eq!(e["lambda_negative"].as_str().unwrap(), "BadLambda");
}

#[test]
fn concentration_decays_with_declared_distance() {
    let doc = load();
    let c = case(&doc, "node_cases", "concentration_decays_with_declared_distance");
    let lambda = c["lambda"].as_f64().unwrap();

    let mut p = Population::new(PopulationSpec::new(0.5, lambda).unwrap())
        .with_node("me")
        .with_node("near")
        .with_node("far")
        .linking("me", "near", c["near"].as_f64().unwrap())
        .linking("me", "far", c["far"].as_f64().unwrap());

    p.seed("near").unwrap();
    let near_only = p.concentration("me").unwrap();
    p.seed("far").unwrap();
    let both = p.concentration("me").unwrap();

    assert!(
        (near_only - c["expect"]["near_only"].as_f64().unwrap()).abs() < 1e-12,
        "e^{{-λ·1}} = {near_only}"
    );
    assert_eq!(
        (both - near_only) < near_only,
        c["expect"]["far_adds_less_than_near"].as_bool().unwrap(),
        "★ λ is not inert — the far neighbour genuinely counts for less"
    );
}

// ── §II — the threshold and the cascade ─────────────────────────────────────

#[test]
fn below_theta_nothing_commits() {
    let doc = load();
    let c = case(&doc, "threshold_cases", "below_theta_nothing_commits");
    let mut p = ring(&doc, c["theta"].as_f64().unwrap());
    p.seed(&seed_of(&doc)).unwrap();

    let cascade = p.cascade(20);
    let e = &c["expect"];
    assert_eq!(cascade.converged, e["converged"].as_bool().unwrap());
    assert_eq!(
        cascade.committed.len(),
        e["committed"].as_u64().unwrap() as usize,
        "★ 'Below θ, nothing' — only the seed"
    );
    assert_eq!(cascade.waves(), e["waves"].as_u64().unwrap() as usize);
}

#[test]
fn above_theta_commitment_cascades_through_the_whole_population() {
    let doc = load();
    let c = case(&doc, "threshold_cases", "above_theta_commitment_cascades_through_the_whole_population");
    let mut p = ring(&doc, c["theta"].as_f64().unwrap());
    p.seed(&seed_of(&doc)).unwrap();

    let size = p.size();
    let cascade = p.cascade(20);
    let e = &c["expect"];

    assert_eq!(cascade.converged, e["converged"].as_bool().unwrap());
    assert_eq!(
        cascade.total(size),
        e["total"].as_bool().unwrap(),
        "★★ one seed took the whole population"
    );
    assert_eq!(cascade.final_fraction, e["final_fraction"].as_f64().unwrap());
    assert!(
        cascade.waves() >= e["waves_at_least"].as_u64().unwrap() as usize,
        "and it SPREAD — a cascade, not a broadcast"
    );
}

#[test]
fn the_cascade_spreads_outward_one_neighbourhood_at_a_time() {
    let doc = load();
    let c = case(&doc, "threshold_cases", "the_cascade_spreads_outward_one_neighbourhood_at_a_time");
    let mut p = ring(&doc, c["theta"].as_f64().unwrap());
    p.seed(&seed_of(&doc)).unwrap();

    for key in ["first_tick", "second_tick"] {
        let want: Vec<&str> =
            c["expect"][key].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let got = p.quorum_step();
        assert_eq!(got.newly_committed, want, "{key}");
    }
}

#[test]
fn a_node_is_not_pushed_over_by_a_neighbour_that_crossed_in_the_same_tick() {
    let doc = load();
    let c = case(&doc, "threshold_cases", "a_node_is_not_pushed_over_by_a_neighbour_that_crossed_in_the_same_tick");
    let mut p = ring(&doc, c["theta"].as_f64().unwrap());
    p.seed(&seed_of(&doc)).unwrap();

    let first = p.quorum_step();
    assert_eq!(
        first.newly_committed.len(),
        c["expect"]["first_tick_size"].as_u64().unwrap() as usize,
        "★ decisions read the PRE-step lattice — otherwise one step sweeps the graph"
    );
}

#[test]
fn a_cascade_that_runs_out_of_ticks_says_so() {
    let doc = load();
    let c = case(&doc, "threshold_cases", "a_cascade_that_runs_out_of_ticks_says_so");
    let mut p = ring(&doc, c["theta"].as_f64().unwrap());
    p.seed(&seed_of(&doc)).unwrap();

    let size = p.size();
    let cascade = p.cascade(c["max_ticks"].as_u64().unwrap() as usize);
    assert_eq!(cascade.converged, c["expect"]["converged"].as_bool().unwrap());
    assert_eq!(cascade.total(size), c["expect"]["total"].as_bool().unwrap());
}

// ── the Signal medium underneath ────────────────────────────────────────────

#[test]
fn a_commit_is_announced_once_not_every_tick() {
    let doc = load();
    let c = case(&doc, "medium_cases", "a_commit_is_announced_once_not_every_tick");
    let mut p = ring(&doc, c["theta"].as_f64().unwrap());
    p.seed(&seed_of(&doc)).unwrap();

    let cascade = p.cascade(20);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for s in &cascade.steps {
        for id in &s.announced {
            *counts.entry(id.clone()).or_default() += 1;
        }
    }

    assert!(!counts.is_empty(), "commitments were announced on the medium");
    assert_eq!(
        counts.values().all(|n| *n == 1),
        c["expect"]["every_node_announced_exactly_once"].as_bool().unwrap(),
        "★ a STANDING commitment would re-announce forever without §III's refractory window: {counts:?}"
    );
}

// ── ★ the bifurcation, on THIS graph ────────────────────────────────────────

#[test]
fn the_transition_is_sharp_on_the_declared_graph() {
    let doc = load();
    let c = case(&doc, "bifurcation_cases", "the_transition_is_sharp_on_the_declared_graph");
    let thetas: Vec<f64> =
        c["thetas"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let seed = seed_of(&doc);

    let t = sweep(|| ring(&doc, 1.0), &[&seed], &thetas, 20).unwrap();
    let e = &c["expect"];

    assert_eq!(t.population_size, e["population_size"].as_u64().unwrap() as usize);
    assert_eq!(t.critical.is_some(), e["crossed"].as_bool().unwrap());
    assert_eq!(
        t.is_sharp(),
        e["sharp"].as_bool().unwrap(),
        "★★ a discontinuity, not a slope: {:?}",
        t.curve
    );

    let at = |x: f64| t.curve.iter().find(|(a, _)| *a == x).unwrap().1;
    assert_eq!(at(0.9), e["at_0_9"].as_f64().unwrap(), "below the critical point: total");
    assert!(at(1.1) < e["at_1_1_below"].as_f64().unwrap(), "above it: nothing moved");

    // And no mean-field β* is claimed anywhere in the result.
    assert!(
        doc["divergence"]["the_honest_limit_on_the_bifurcation"]
            .as_str()
            .unwrap()
            .contains("nothing here reports a predicted β*")
    );
}

#[test]
fn a_sweep_that_never_crosses_reports_no_critical_point() {
    let doc = load();
    let c = case(&doc, "bifurcation_cases", "a_sweep_that_never_crosses_reports_no_critical_point");
    let thetas: Vec<f64> =
        c["thetas"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let seed = seed_of(&doc);

    let t = sweep(|| ring(&doc, 1.0), &[&seed], &thetas, 20).unwrap();
    assert_eq!(t.critical.is_some(), c["expect"]["crossed"].as_bool().unwrap());
    assert_eq!(t.is_sharp(), c["expect"]["sharp"].as_bool().unwrap());
}

// ── ★ MUL-3 ─────────────────────────────────────────────────────────────────

#[test]
fn this_quorum_is_a_scalar_threshold_not_a_set_property() {
    let doc = load();
    let c = case(&doc, "mul3_cases", "this_quorum_is_a_scalar_threshold_not_a_set_property");
    let mut p = ring(&doc, 0.5);
    p.seed(&seed_of(&doc)).unwrap();

    let view = p.local_view("b").unwrap();
    let e = &c["expect"];

    assert_eq!(view.would_commit(0.5), e["commits_at_low_theta"].as_bool().unwrap());
    assert_eq!(!view.would_commit(1.5), e["does_not_commit_at_high_theta"].as_bool().unwrap());
    assert_eq!(
        view.would_commit(0.5) == (view.concentration > 0.5),
        e["decision_is_a_scalar_comparison"].as_bool().unwrap(),
        "★ no set of members appears in the decision anywhere"
    );
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // ★ The counterweight IS MUL-3 — the reference has the OTHER quorum.
    let cw = d["★_the_counterweight_IS_MUL-3"].as_str().unwrap();
    assert!(cw.contains("CouncilSession"), "it names the real thing");
    assert!(cw.contains("RATIFY") && cw.contains("FORMS"), "and the distinction between them");
    assert!(cw.contains("classic error"), "quoting the article's own warning");

    // The prettiest false positive: two different θ in one codebase.
    let fp = &d["the_four_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["theta (11 hits)"].as_str().unwrap().contains("OPERATOR PARAMETERS"));
    assert!(fp["threshold (70 hits)"].as_str().unwrap().contains("Real thresholds"));

    // The standing-vs-transient decision, recorded rather than buried.
    let sig = d["★_why_the_commitment_lattice_is_not_routed_through_the_refractory_field"]
        .as_str()
        .unwrap();
    assert!(sig.contains("STANDING") && sig.contains("TRANSIENT"));
    assert!(sig.contains("die out like a wave instead of latching"));
    assert!(
        sig.contains("Neither is load-bearing for the cascade arithmetic"),
        "and the reuse is not overstated"
    );

    // One term is at parity with the PREVIOUS SLICE, not with Python.
    assert!(
        d["term_by_term"]["the Signal medium underneath"]
            .as_str()
            .unwrap()
            .contains("AT PARITY WITH THE PREVIOUS SLICE")
    );

    // The mean-field limit and the symmetry choice are both disclosed.
    assert!(d["the_honest_limit_on_the_bifurcation"].as_str().unwrap().contains("WELL-MIXED"));
    assert!(d["a_disclosed_scope_choice"].as_str().unwrap().contains("SYMMETRIC"));

    for group in
        ["node_cases", "threshold_cases", "medium_cases", "bifurcation_cases", "mul3_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in
        ["node_cases", "threshold_cases", "medium_cases", "bifurcation_cases", "mul3_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
