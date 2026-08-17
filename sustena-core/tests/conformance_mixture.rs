//! The sparse mixture-of-experts gate, replayed against its conformance
//! vectors (R2 · Operative §XV + §XIII · OPV-28).
//!
//! **SPEC vectors, Rust-only — with three counterweights, each real.**
//! Mismatch → abstain is **at parity in behaviour** (`no_domain_overlap`); the
//! cheap/expensive split **exists as a convention** (`should_evaluate` *"MUST
//! NOT call the LLM"* versus `evaluate`); and a declared-weight,
//! budget-bounded selector with disclosed exclusions **exists for widgets**
//! (`knapsack_select`, `α·urgency + λ·relevance`). Nothing here is a new idea
//! to the codebase — what is new is putting the three together on the agent
//! layer and making the sparsity structural.
//!
//! Two proofs: the expensive path runs **`k` times, not `N`**, and the routing
//! scalar **cannot reach the presentation** — the frontier is byte-identical
//! whether or not routing happened.
//!
//! See `conformance/README.md`.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    coverage_gaps, present, subject_relevance, Alternative, Contender, Cynefin, EstimateRule,
    GateWeights, Mixture, MixtureError, Objective, Operative, Sense, Utility, WithheldReason,
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("mixture.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "mixture.json was written for a different contract version"
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

fn cynefin(s: &str) -> Cynefin {
    Cynefin::ALL
        .into_iter()
        .find(|c| c.name() == s)
        .unwrap_or_else(|| panic!("unknown domain {s}"))
}

fn tags(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn op(id: &str, suited: &[Cynefin]) -> Operative {
    Operative::new(
        id,
        Utility::new()
            .with(Objective::new("x", "x", Sense::Maximise))
            .unwrap(),
        suited,
    )
    .unwrap()
}

// ── fixture ─────────────────────────────────────────────────────────────────

/// The four declared operatives, keyed by id so borrows outlive the contenders.
fn fixture_operatives(doc: &Value) -> HashMap<String, Operative> {
    doc["fixture"]["contenders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let id = c["id"].as_str().unwrap().to_string();
            let suited: Vec<Cynefin> = c["suited"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| cynefin(s.as_str().unwrap()))
                .collect();
            (id.clone(), op(&id, &suited))
        })
        .collect()
}

fn fixture_contenders<'a>(doc: &Value, ops: &'a HashMap<String, Operative>) -> Vec<Contender<'a>> {
    doc["fixture"]["contenders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let id = c["id"].as_str().unwrap();
            let t: Vec<&str> = c["tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            let est: Vec<f64> = c["estimate"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            Contender::new(&ops[id], &t, &est)
        })
        .collect()
}

fn gate(doc: &Value, k: Option<usize>) -> Mixture {
    let w = &doc["fixture"]["weights"];
    Mixture::new(
        GateWeights::new(
            w["alpha"].as_f64().unwrap(),
            w["eta"].as_f64().unwrap(),
            w["zeta"].as_f64().unwrap(),
        )
        .unwrap(),
        k.unwrap_or(doc["fixture"]["k"].as_u64().unwrap() as usize),
        EstimateRule::NetGain,
    )
    .unwrap()
}

fn request_tags(doc: &Value) -> BTreeSet<String> {
    doc["fixture"]["request_tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn dom_s(doc: &Value) -> Cynefin {
    cynefin(doc["fixture"]["dom_s"].as_str().unwrap())
}

// ── ★★ structural sparsity ──────────────────────────────────────────────────

/// ★★ THE PROOF. The expensive path is *entered* k times, not filtered to k.
#[test]
fn the_expensive_path_runs_k_times_not_n() {
    let doc = load();
    let c = case(&doc, "sparsity_cases", "★★_the_expensive_path_runs_k_times_not_N");
    let e = &c["expect"];
    let ops = fixture_operatives(&doc);
    let contenders = fixture_contenders(&doc, &ops);

    let dispatch = gate(&doc, None)
        .route(&contenders, &request_tags(&doc), dom_s(&doc))
        .unwrap();

    let calls = RefCell::new(Vec::new());
    dispatch.run(|t| calls.borrow_mut().push(t.operative().to_string()));

    assert_eq!(dispatch.considered(), e["considered"].as_u64().unwrap() as usize);
    assert_eq!(calls.borrow().len(), e["calls"].as_u64().unwrap() as usize);
    let want: Vec<String> = e["called"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(*calls.borrow(), want, "the other two were never entered");
    assert!((dispatch.sparsity() - e["sparsity"].as_f64().unwrap()).abs() < 1e-12);

    assert_eq!(dispatch.withheld().len(), e["withheld"].as_u64().unwrap() as usize);
    assert_eq!(
        dispatch
            .withheld()
            .iter()
            .all(|w| w.reason == WithheldReason::BelowTopK),
        e["all_withheld_below_top_k"].as_bool().unwrap()
    );
    // Ranks are dense and ordered — 0 is the strongest selection.
    for (i, t) in dispatch.tickets().iter().enumerate() {
        assert_eq!(t.rank(), i);
    }
}

#[test]
fn softmax_is_monotone_so_it_does_not_change_the_selection() {
    let doc = load();
    let c = case(
        &doc,
        "sparsity_cases",
        "softmax_is_monotone_so_it_does_not_change_the_selection",
    );
    let ops = fixture_operatives(&doc);
    let contenders = fixture_contenders(&doc, &ops);
    // k = N so every gate weight is reported.
    let d = gate(&doc, Some(4))
        .route(&contenders, &request_tags(&doc), dom_s(&doc))
        .unwrap();

    let total: f64 = d.tickets().iter().map(|t| t.gate_weight()).sum();
    assert_eq!((total - 1.0).abs() < 1e-12, c["expect"]["sums_to_one"].as_bool().unwrap());
    // Selection order and weight order agree — softmax preserved the ranking.
    let ordered = d
        .tickets()
        .windows(2)
        .all(|w| w[0].gate_weight() >= w[1].gate_weight() && w[0].score() >= w[1].score());
    assert_eq!(ordered, c["expect"]["order_preserved"].as_bool().unwrap());
}

#[test]
fn k_larger_than_the_field_asks_everyone_suited() {
    let doc = load();
    let c = case(&doc, "sparsity_cases", "k_larger_than_the_field_asks_everyone_suited");
    let a = op("a", &[Cynefin::Clear]);
    let b = op("b", &[Cynefin::Clear]);
    let contenders = vec![
        Contender::new(&a, &["food"], &[1.0]),
        Contender::new(&b, &["food"], &[2.0]),
    ];
    let d = gate(&doc, Some(9))
        .route(&contenders, &tags(&["food"]), Cynefin::Clear)
        .unwrap();
    assert_eq!(d.tickets().len(), c["expect"]["tickets"].as_u64().unwrap() as usize);
    assert!((d.sparsity() - c["expect"]["sparsity"].as_f64().unwrap()).abs() < 1e-12);
}

// ── ★ the two axes ──────────────────────────────────────────────────────────

/// ★ A generalisation, not a replacement — it agrees with the hard test at
/// both boundaries.
#[test]
fn relevance_generalises_the_hard_test_and_agrees_at_its_boundaries() {
    let doc = load();
    let c = case(
        &doc,
        "axis_cases",
        "★_relevance_generalises_the_hard_test_and_agrees_at_its_boundaries",
    );
    let e = &c["expect"];
    // `if not proposal_domains: return True`
    assert_eq!(
        subject_relevance(&tags(&["food"]), &BTreeSet::new()),
        e["untagged_request"].as_f64().unwrap()
    );
    // `if not self.domain: return False`
    assert_eq!(
        subject_relevance(&BTreeSet::new(), &tags(&["food"])),
        e["untagged_operative"].as_f64().unwrap()
    );
    // Soft in between, where the hard test can only say "yes".
    assert!(
        (subject_relevance(&tags(&["food"]), &tags(&["food", "rent"]))
            - e["half_covered"].as_f64().unwrap())
        .abs()
            < 1e-12
    );
    assert_eq!(
        subject_relevance(&tags(&["a"]), &tags(&["b"])),
        e["no_overlap"].as_f64().unwrap()
    );
}

/// ★ A high estimate does not buy past a regime mismatch.
#[test]
fn a_domain_mismatch_abstains_under_its_own_reason_and_is_never_scored() {
    let doc = load();
    let c = case(
        &doc,
        "axis_cases",
        "★_a_domain_mismatch_abstains_under_its_own_reason_and_is_never_scored",
    );
    let e = &c["expect"];
    let fit = op("fit", &[Cynefin::Complex]);
    let unfit = op("unfit", &[Cynefin::Complicated]);
    let contenders = vec![
        Contender::new(&fit, &["food"], &[1.0]),
        Contender::new(&unfit, &["food"], &[99.0]),
    ];
    let d = gate(&doc, Some(2))
        .route(&contenders, &tags(&["food"]), Cynefin::Complex)
        .unwrap();

    let got: Vec<&str> = d.tickets().iter().map(|t| t.operative()).collect();
    let want: Vec<&str> = e["tickets"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(got, want);

    let w = &d.withheld()[0];
    assert_eq!(w.operative, e["withheld_operative"].as_str().unwrap());
    assert_eq!(w.reason, WithheldReason::DomainMismatch);
    assert_eq!(e["reason"].as_str().unwrap(), "DomainMismatch");
    assert_eq!(w.score, None, "never scored — the regime question came first");
    assert!(e["score"].is_null());
}

// ── ★ the coverage condition ────────────────────────────────────────────────

/// ★ §XIII: surface the gap rather than route to the least-bad scorer.
#[test]
fn a_regime_no_operative_is_suited_to_surfaces_the_gap_and_asks_nobody() {
    let doc = load();
    let c = case(
        &doc,
        "coverage_cases",
        "★_a_regime_no_operative_is_suited_to_surfaces_the_gap_and_asks_nobody",
    );
    let e = &c["expect"];
    let a = op("a", &[Cynefin::Clear]);
    let b = op("b", &[Cynefin::Complicated]);
    let contenders = vec![
        Contender::new(&a, &["food"], &[9.0]),
        Contender::new(&b, &["food"], &[9.0]),
    ];
    let d = gate(&doc, Some(2))
        .route(&contenders, &tags(&["food"]), Cynefin::Chaotic)
        .unwrap();

    assert_eq!(d.tickets().len(), e["tickets"].as_u64().unwrap() as usize);
    assert_eq!(d.coverage_gap(), Some(cynefin(e["gap"].as_str().unwrap())));
    assert!(d.describe().contains("gap surfaced"));

    // The expensive path genuinely does not run.
    let calls = RefCell::new(0usize);
    d.run(|_| *calls.borrow_mut() += 1);
    assert_eq!(*calls.borrow(), e["expensive_calls"].as_u64().unwrap() as usize);
}

#[test]
fn coverage_gaps_measures_the_union_against_all_four() {
    let doc = load();
    let c = case(&doc, "coverage_cases", "coverage_gaps_measures_the_union_against_all_four");
    let a = op("a", &[Cynefin::Clear, Cynefin::Complicated]);
    let b = op("b", &[Cynefin::Complex]);
    let contenders = vec![
        Contender::new(&a, &[], &[0.0]),
        Contender::new(&b, &[], &[0.0]),
    ];
    let want: BTreeSet<Cynefin> = c["expect"]["uncovered"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| cynefin(v.as_str().unwrap()))
        .collect();
    assert_eq!(coverage_gaps(&contenders), want);
}

#[test]
fn an_operative_suited_to_nothing_is_named_as_never_reachable() {
    let doc = load();
    let c = case(
        &doc,
        "coverage_cases",
        "an_operative_suited_to_nothing_is_named_as_never_reachable",
    );
    let dead = op("dead", &[]);
    let live = op("live", &[Cynefin::Clear]);
    let contenders = vec![
        Contender::new(&dead, &[], &[0.0]),
        Contender::new(&live, &[], &[0.0]),
    ];
    let want: Vec<&str> = c["expect"]["never_reachable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(gate(&doc, Some(1)).never_reachable(&contenders), want);
}

// ── ★★ the wall ─────────────────────────────────────────────────────────────

/// ★★ THE OTHER PROOF — routing collapses a vector, and that collapse cannot
/// reach a person.
#[test]
fn the_routing_scalar_does_not_reach_the_presentation() {
    let doc = load();
    let c = case(
        &doc,
        "wall_cases",
        "★★_the_routing_scalar_does_not_reach_the_presentation",
    );
    let e = &c["expect"];

    // The same non-convex option set the previous slices used.
    let alts = [
        Alternative::new("a", &[("x", 10.0), ("y", 0.0)]),
        Alternative::new("b", &[("x", 0.0), ("y", 10.0)]),
        Alternative::new("c", &[("x", 4.0), ("y", 4.0)]),
        Alternative::new("d", &[("x", 1.0), ("y", 1.0)]),
    ];
    let u = Utility::new()
        .with(Objective::new("x", "x", Sense::Maximise))
        .unwrap()
        .with(Objective::new("y", "y", Sense::Maximise))
        .unwrap();

    let before = present(&u, &alts).unwrap();

    // Route — which performs exactly the collapse OPV-29 forbids downstream.
    let ops = fixture_operatives(&doc);
    let contenders = fixture_contenders(&doc, &ops);
    let dispatch = gate(&doc, None)
        .route(&contenders, &request_tags(&doc), dom_s(&doc))
        .unwrap();
    assert!(!dispatch.tickets().is_empty(), "routing really happened");
    // The scalars exist and are real.
    assert!(dispatch.tickets().iter().all(|t| t.score().is_finite()));

    let after = present(&u, &alts).unwrap();

    // ★ Byte-identical. The routing collapse had zero influence on the surface.
    assert_eq!(before, after);
    assert_eq!(before.frontier() == after.frontier(), e["frontier_unchanged"].as_bool().unwrap());
    let want: Vec<String> = e["frontier"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(after.frontier(), want.as_slice());
    // And the non-convex option a routing-style weighted sum would delete is
    // still there for a person to choose.
    assert!(after.frontier().contains(&"c".to_string()));
}

#[test]
fn the_estimate_collapse_is_a_named_declared_rule() {
    let doc = load();
    let c = case(&doc, "wall_cases", "the_estimate_collapse_is_a_named_declared_rule");
    let e = &c["expect"];
    let v = [5.0, -3.0];

    let via = |rule: EstimateRule, est: &[f64]| -> Result<f64, MixtureError> {
        // Exercised through the real gate rather than a private helper: a
        // single suited contender makes the score α·1 + η·collapse + ζ.
        let only = op("only", &[Cynefin::Clear]);
        let contenders = vec![Contender::new(&only, &[], est)];
        let m = Mixture::new(GateWeights::new(0.0, 1.0, 0.0).unwrap(), 1, rule)?;
        let d = m.route(&contenders, &BTreeSet::new(), Cynefin::Clear)?;
        Ok(d.tickets()[0].score())
    };

    assert!((via(EstimateRule::NetGain, &v).unwrap() - e["net_gain"].as_f64().unwrap()).abs() < 1e-12);
    assert!(
        (via(EstimateRule::GainsOnly, &v).unwrap() - e["gains_only"].as_f64().unwrap()).abs() < 1e-12
    );
    assert!(
        (via(EstimateRule::Weighted(vec![1.0, 2.0]), &v).unwrap()
            - e["weighted"].as_f64().unwrap())
        .abs()
            < 1e-12
    );
    assert!(matches!(
        via(EstimateRule::Weighted(vec![1.0]), &v),
        Err(MixtureError::EstimateArity { .. })
    ));
    assert_eq!(e["arity_mismatch"].as_str().unwrap(), "EstimateArity");
}

// ── the declared gate ───────────────────────────────────────────────────────

#[test]
fn a_negative_weight_or_a_zero_k_is_refused() {
    let doc = load();
    let _ = case(&doc, "declaration_cases", "a_negative_weight_or_a_zero_k_is_refused");
    assert!(matches!(
        GateWeights::new(-1.0, 1.0, 1.0),
        Err(MixtureError::BadWeight { .. })
    ));
    assert!(matches!(
        GateWeights::new(0.0, 0.0, 0.0),
        Err(MixtureError::AllWeightsZero)
    ));
    assert!(matches!(
        Mixture::new(GateWeights::new(1.0, 1.0, 1.0).unwrap(), 0, EstimateRule::NetGain),
        Err(MixtureError::ZeroK)
    ));
}

#[test]
fn routing_is_deterministic_and_ties_break_by_declaration_order() {
    let doc = load();
    let c = case(
        &doc,
        "declaration_cases",
        "routing_is_deterministic_and_ties_break_by_declaration_order",
    );
    let a = op("a", &[Cynefin::Clear]);
    let b = op("b", &[Cynefin::Clear]);
    let contenders = vec![
        Contender::new(&a, &["food"], &[1.0]),
        Contender::new(&b, &["food"], &[1.0]),
    ];
    let g = gate(&doc, Some(1));
    let first = g.route(&contenders, &tags(&["food"]), Cynefin::Clear).unwrap();
    let second = g.route(&contenders, &tags(&["food"]), Cynefin::Clear).unwrap();
    assert_eq!(first == second, c["expect"]["identical"].as_bool().unwrap());
    assert_eq!(
        first.tickets()[0].operative(),
        c["expect"]["tie_winner"].as_str().unwrap()
    );
}

#[test]
fn a_duplicate_contender_is_refused() {
    let doc = load();
    let _ = case(&doc, "declaration_cases", "a_duplicate_contender_is_refused");
    let a = op("a", &[Cynefin::Clear]);
    let contenders = vec![
        Contender::new(&a, &["food"], &[1.0]),
        Contender::new(&a, &["food"], &[2.0]),
    ];
    assert!(matches!(
        gate(&doc, Some(1)).route(&contenders, &tags(&["food"]), Cynefin::Clear),
        Err(MixtureError::DuplicateContender(_))
    ));
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ All three counterweights, each with its evidence.
    let cw = &d["★_three_counterweights_and_each_is_real"];
    let abstain = cw["1. mismatch → abstain is AT PARITY IN BEHAVIOUR"].as_str().unwrap();
    assert!(abstain.contains("no_domain_overlap"));
    assert!(abstain.contains("already the shipped behaviour"));

    let split = cw["2. the cheap/expensive split EXISTS AS A CONVENTION"].as_str().unwrap();
    assert!(split.contains("should_evaluate"));
    assert!(split.contains("MUST NOT call the LLM"));
    assert!(split.contains("promotes it to a type"));

    let widgets = cw
        ["3. a declared-weight, budget-bounded selector with disclosed exclusions EXISTS — FOR WIDGETS"]
        .as_str()
        .unwrap();
    assert!(widgets.contains("knapsack"));
    assert!(widgets.contains("ALPHA_URGENCY"));
    assert!(widgets.contains("never turned on the agent layer"));

    // ★ Selection, not combination — the borrowed-paper limit.
    let borrow = d["★_borrowed_for_selection_deliberately_not_for_combination"]
        .as_str()
        .unwrap();
    assert!(borrow.contains("Sustena must not"));
    assert!(borrow.contains("MONOTONE"));

    // ★ The wall is described AND proven.
    let wall = d["★_the_routing_scalarisation_tension_and_how_the_wall_is_built"]
        .as_str()
        .unwrap();
    assert!(wall.contains("whom to ask, never what to do"));
    assert!(wall.contains("byte-identical"));

    // Load balancing is refused rather than faked.
    let lb = d["term_by_term"]["load balancing (Shazeer)"].as_str().unwrap();
    assert!(lb.contains("would be fabrication"));

    // The gating false positive — always the admission sense here.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["gating (3 hits, 0 real)"].as_str().unwrap().contains("ADMISSION sense"));
    assert!(fp["relevance (16 hits) — ★ NOT a false positive, and the two senses must be told apart"]
        .as_str()
        .unwrap()
        .contains("one of them got the good machinery"));

    // Four honest limits, all present.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "IS SUPPLIED, NOT COMPUTED",
        "`Δû` IS SUPPLIED",
        "DISORDER IS NOT DETECTED",
        "LOAD BALANCING IS NOT BUILT",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = [
        "sparsity_cases",
        "axis_cases",
        "coverage_cases",
        "wall_cases",
        "declaration_cases",
    ];
    let mut seen = BTreeSet::new();
    for group in groups {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(
                seen.insert(c["name"].as_str().unwrap().to_string()),
                "duplicate case name"
            );
        }
    }
}
