//! Proximity to criticality, replayed against its conformance vectors
//! (R2 · Operative §VIII · OPV-14).
//!
//! **SPEC vectors, Rust-only — and this slice found three premises that did
//! not hold, each by checking rather than assuming.**
//!
//! 1. §VIII calls `σ̂` cheap because *"the event record already carries
//!    `causes`"*. True of the spec and of the Rust core; the shipped Python
//!    `events` table has no such column, and the causal edge is **known at
//!    runtime and thrown away**.
//! 2. §VIII justifies detector 3 as *"exactly the moments the Monitor already
//!    maintains"*. `Ewma` keeps a level and `Cusum` keeps one-sided sums —
//!    neither is a second moment.
//! 3. **My own** prior claim that OPV-14 moves both of §XV's remaining duties
//!    to `Built`. It does not: §XIII says the criticality detector is *"one of
//!    the inputs"* to a domain reading, not the classification.
//!
//! The counterweight is real and it is the discipline: *advise, never act* is
//! already this codebase's keystone.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    branching_ratio, critical_slowing_down, CascadeSummary, CausalStamp, CriticalityError,
    CriticalitySpec, Event, Provenance, Regime, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("criticality.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "criticality.json was written for a different contract version"
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

fn ev(id: &str, causes: &[String]) -> Event {
    Event {
        id: id.into(),
        name: "event.test.thing".into(),
        t_event: 0,
        provenance: Provenance::Observed,
        stamp: CausalStamp {
            counter: 0,
            node: "n".into(),
        },
        causes: causes.to_vec(),
        mutations: vec![],
    }
}

/// Build a shape from the vector's `[[id, [causes]], ...]` form.
fn shape(doc: &Value, key: &str) -> Vec<Event> {
    doc["fixture"][key]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            let causes: Vec<String> = pair[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_string())
                .collect();
            ev(pair[0].as_str().unwrap(), &causes)
        })
        .collect()
}

fn spec(doc: &Value) -> CriticalitySpec {
    CriticalitySpec::new(doc["fixture"]["band"].as_f64().unwrap()).expect("declared band")
}

fn regime_name(r: Regime) -> &'static str {
    r.name()
}

// ── σ̂ ───────────────────────────────────────────────────────────────────────

/// ★ A read over an existing field — verified present before building.
#[test]
fn sigma_hat_is_a_read_over_the_causal_dag() {
    let doc = load();
    let c = case(&doc, "branching_cases", "★_sigma_hat_is_a_read_over_the_causal_DAG");
    let e = &c["expect"];
    let r = branching_ratio(&shape(&doc, "chain"), spec(&doc)).unwrap();
    assert!((r.sigma_hat() - e["sigma_hat"].as_f64().unwrap()).abs() < 1e-12);
    assert_eq!(regime_name(r.regime()), e["regime"].as_str().unwrap());
    assert_eq!(r.cascades(), e["cascades"].as_u64().unwrap() as usize);
    assert_eq!(
        r.largest_observed_cascade(),
        e["largest"].as_u64().unwrap() as usize
    );
}

/// ★ The obvious wrong intuition, pinned.
#[test]
fn a_tree_over_a_finite_window_can_never_read_supercritical() {
    let doc = load();
    let c = case(
        &doc,
        "branching_cases",
        "★_a_tree_over_a_finite_window_can_never_read_supercritical",
    );
    let e = &c["expect"];
    let tree = shape(&doc, "tree");
    let r = branching_ratio(&tree, spec(&doc)).unwrap();
    assert!((r.sigma_hat() - e["sigma_hat"].as_f64().unwrap()).abs() < 1e-12);
    // ★ The general property, not just this tree: n nodes, n−1 edges, so
    // σ̂ < 1 always. This is what the case is about, and it holds under any
    // band.
    assert_eq!(r.edges(), tree.len() - 1);
    assert_eq!(r.sigma_hat() < 1.0, e["always_below_one"].as_bool().unwrap());

    // ★★ The REGIME, though, is the band's judgement rather than the tree's:
    // 6/7 is only 0.143 from 1, so a wide band calls it critical and the
    // narrower default calls it subcritical. Neither is wrong — which is why
    // the band is declared.
    assert_eq!(regime_name(r.regime()), e["regime"].as_str().unwrap());
    let narrow = branching_ratio(&tree, CriticalitySpec::default()).unwrap();
    assert_eq!(
        regime_name(narrow.regime()),
        e["under_narrow_band"].as_str().unwrap()
    );
}

#[test]
fn a_multi_parent_dag_reads_supercritical() {
    let doc = load();
    let c = case(&doc, "branching_cases", "a_multi_parent_dag_reads_supercritical");
    let e = &c["expect"];
    let r = branching_ratio(&shape(&doc, "dag"), spec(&doc)).unwrap();
    assert!((r.sigma_hat() - e["sigma_hat"].as_f64().unwrap()).abs() < 1e-12);
    assert_eq!(regime_name(r.regime()), e["regime"].as_str().unwrap());
}

/// ★ Window edges are where a branching estimate quietly goes wrong.
#[test]
fn a_cause_outside_the_window_is_dangling_not_an_offspring_nobody_had() {
    let doc = load();
    let c = case(
        &doc,
        "branching_cases",
        "★_a_cause_outside_the_window_is_dangling_not_an_offspring_nobody_had",
    );
    let e = &c["expect"];
    let events = [
        ev("b", &["a_outside".to_string()]),
        ev("c", &["b".to_string()]),
    ];
    let r = branching_ratio(&events, spec(&doc)).unwrap();
    assert_eq!(r.dangling_causes(), e["dangling"].as_u64().unwrap() as usize);
    assert_eq!(r.edges(), e["edges"].as_u64().unwrap() as usize);
    assert!((r.sigma_hat() - e["sigma_hat"].as_f64().unwrap()).abs() < 1e-12);
}

#[test]
fn the_critical_band_is_declared_not_a_hidden_epsilon() {
    let doc = load();
    let c = case(
        &doc,
        "branching_cases",
        "the_critical_band_is_declared_not_a_hidden_epsilon",
    );
    let e = &c["expect"];
    let narrow = CriticalitySpec::new(0.01).unwrap();
    let wide = CriticalitySpec::new(0.30).unwrap();
    assert_eq!(regime_name(narrow.classify(0.9)), e["narrow_at_0_9"].as_str().unwrap());
    assert_eq!(regime_name(wide.classify(0.9)), e["wide_at_0_9"].as_str().unwrap());
    assert_eq!(regime_name(narrow.classify(1.0)), e["narrow_at_1_0"].as_str().unwrap());
    assert!(matches!(
        CriticalitySpec::new(-0.1),
        Err(CriticalityError::BadBand(_))
    ));
}

#[test]
fn an_empty_window_and_a_causal_cycle_and_a_duplicate_are_refused() {
    let doc = load();
    let _ = case(
        &doc,
        "branching_cases",
        "an_empty_window_and_a_causal_cycle_and_a_duplicate_are_refused",
    );
    let s = spec(&doc);
    assert!(matches!(
        branching_ratio(&[], s),
        Err(CriticalityError::EmptyWindow)
    ));
    let cycle = [ev("a", &["b".to_string()]), ev("b", &["a".to_string()])];
    assert!(matches!(
        branching_ratio(&cycle, s),
        Err(CriticalityError::CausalCycle(_))
    ));
    let dup = [ev("a", &[]), ev("a", &[])];
    assert!(matches!(
        branching_ratio(&dup, s),
        Err(CriticalityError::DuplicateEvent(_))
    ));
}

// ── ★★ the risk arithmetic ──────────────────────────────────────────────────

/// ★★ THE PROOF. No number, not a caveated one.
#[test]
fn no_expected_size_is_offered_at_or_above_criticality() {
    let doc = load();
    let c = case(
        &doc,
        "risk_arithmetic_cases",
        "★★_no_expected_size_is_offered_at_or_above_criticality",
    );
    let e = &c["expect"];
    let s = spec(&doc);

    // Subcritical: a mean is honest.
    let calm = [ev("a", &[]), ev("b", &[]), ev("c", &[]), ev("d", &["a".to_string()])];
    let r = branching_ratio(&calm, s).unwrap();
    assert_eq!(r.regime(), Regime::Subcritical);
    assert_eq!(
        r.cascade_summary().mean().is_some(),
        e["subcritical_has_mean"].as_bool().unwrap()
    );

    // Supercritical: no number at all.
    let hot = branching_ratio(&shape(&doc, "dag"), s).unwrap();
    assert_eq!(hot.regime(), Regime::Supercritical);
    assert_eq!(hot.cascade_summary().mean(), None);
    assert!(e["supercritical_mean"].is_null());
    match hot.cascade_summary() {
        CascadeSummary::NotUsable { why, .. } => {
            for term in e["why_contains"].as_array().unwrap() {
                assert!(why.contains(term.as_str().unwrap()), "missing: {term}");
            }
        }
        other => panic!("expected NotUsable, got {other:?}"),
    }
}

/// ★ Withheld AT criticality, not only above it.
#[test]
fn being_inside_the_band_already_withholds_the_mean() {
    let doc = load();
    let c = case(
        &doc,
        "risk_arithmetic_cases",
        "★_being_INSIDE_the_band_already_withholds_the_mean",
    );
    let e = &c["expect"];
    // 3 events, 3 edges → σ̂ = 1.0 exactly.
    let events = [
        ev("a", &[]),
        ev("b", &["a".to_string()]),
        ev("c", &["a".to_string(), "b".to_string()]),
    ];
    let r = branching_ratio(&events, spec(&doc)).unwrap();
    assert!((r.sigma_hat() - e["sigma_hat"].as_f64().unwrap()).abs() < 1e-12);
    assert_eq!(regime_name(r.regime()), e["regime"].as_str().unwrap());
    assert_eq!(
        r.regime().mean_is_usable(),
        e["mean_is_usable"].as_bool().unwrap()
    );
    assert_eq!(r.cascade_summary().mean(), None);
}

/// ★ History is not the bound.
#[test]
fn the_largest_observed_cascade_is_reported_as_a_floor() {
    let doc = load();
    let c = case(
        &doc,
        "risk_arithmetic_cases",
        "★_the_largest_observed_cascade_is_reported_as_a_FLOOR",
    );
    let r = branching_ratio(&shape(&doc, "dag"), spec(&doc)).unwrap();
    assert!(r
        .cascade_summary()
        .describe()
        .contains(c["expect"]["describe_contains"].as_str().unwrap()));
}

// ── ★★ read-only ────────────────────────────────────────────────────────────

/// ★★ A reading surfaces to a person; the borrow checker enforces it.
#[test]
fn a_reading_is_a_pure_function_of_the_log_and_can_never_act() {
    let doc = load();
    let c = case(
        &doc,
        "read_only_cases",
        "★★_a_reading_is_a_pure_function_of_the_log_and_can_never_act",
    );
    let e = &c["expect"];
    let events = shape(&doc, "chain");
    let before = events.clone();
    let first = branching_ratio(&events, spec(&doc)).unwrap();
    let second = branching_ratio(&events, spec(&doc)).unwrap();
    assert_eq!(first == second, e["repeatable"].as_bool().unwrap());
    // `&[Event]` in, reading out — the log is byte-identical.
    assert_eq!(events == before, e["log_untouched"].as_bool().unwrap());
}

// ── detector 3 ──────────────────────────────────────────────────────────────

#[test]
fn variance_and_lag_one_over_a_supplied_window() {
    let doc = load();
    let c = case(
        &doc,
        "slowing_down_cases",
        "★_variance_and_lag_one_over_a_SUPPLIED_window",
    );
    let series: Vec<f64> = c["short_ramp"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let w = critical_slowing_down(&series).unwrap();
    assert!((w.variance() - c["expect"]["variance"].as_f64().unwrap()).abs() < 1e-12);
    assert!((w.lag1_autocorrelation() - c["expect"]["lag1"].as_f64().unwrap()).abs() < 1e-12);
    assert_eq!(w.window(), series.len());
}

/// ★ r₁ on a short series understates persistence.
#[test]
fn the_same_shape_reads_more_persistent_on_a_longer_window() {
    let doc = load();
    let c = case(
        &doc,
        "slowing_down_cases",
        "★_the_same_shape_reads_more_persistent_on_a_longer_window",
    );
    let e = &c["expect"];
    let short: Vec<f64> = (1..=5).map(|i| i as f64).collect();
    let long: Vec<f64> = (1..=10).map(|i| i as f64).collect();
    let s = critical_slowing_down(&short).unwrap();
    let l = critical_slowing_down(&long).unwrap();
    assert!((s.lag1_autocorrelation() - e["short_lag1"].as_f64().unwrap()).abs() < 1e-12);
    assert!((l.lag1_autocorrelation() - e["long_lag1"].as_f64().unwrap()).abs() < 1e-12);
    assert_eq!(
        l.lag1_autocorrelation() > s.lag1_autocorrelation(),
        e["longer_is_larger"].as_bool().unwrap()
    );
    assert!(s.describe().contains("not a trend"));
}

#[test]
fn an_anti_correlated_series_reads_negative() {
    let doc = load();
    let c = case(&doc, "slowing_down_cases", "an_anti_correlated_series_reads_negative");
    let alt = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
    let a = critical_slowing_down(&alt).unwrap();
    assert!(a.lag1_autocorrelation() < c["expect"]["lag1_below"].as_f64().unwrap());
}

#[test]
fn a_constant_series_is_refused_rather_than_reported_as_zero() {
    let doc = load();
    let _ = case(
        &doc,
        "slowing_down_cases",
        "★_a_constant_series_is_refused_rather_than_reported_as_zero",
    );
    assert!(matches!(
        critical_slowing_down(&[2.0, 2.0, 2.0, 2.0]),
        Err(CriticalityError::ConstantSeries)
    ));
}

#[test]
fn a_window_under_three_points_is_refused() {
    let doc = load();
    let _ = case(&doc, "slowing_down_cases", "a_window_under_three_points_is_refused");
    assert!(matches!(
        critical_slowing_down(&[1.0, 2.0]),
        Err(CriticalityError::WindowTooShort(2))
    ));
    assert!(matches!(
        critical_slowing_down(&[1.0, f64::NAN, 3.0]),
        Err(CriticalityError::NotFinite)
    ));
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact — and this slice's three corrections
/// live in it. If the reference catches up, this test is where the note gets
/// rewritten, not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ Correction 1 — the reference has no `causes` column.
    let c1 = d["★★_a_correction_to_the_articles_premise_number_1_the_reference_does_NOT_carry_causes"]
        .as_str()
        .unwrap();
    assert!(c1.contains("no causal-parent column"));
    assert!(c1.contains("new instrumentation"));
    let sharp = d["★_and_the_sharpest_part_the_edge_is_KNOWN_and_then_thrown_away"]
        .as_str()
        .unwrap();
    assert!(sharp.contains("trigger_event"));
    assert!(sharp.contains("one column and one assignment"));

    // ★★ Correction 2 — the Monitor keeps no second moment.
    let c2 = d["★★_a_correction_to_the_articles_premise_number_2_the_monitor_does_NOT_maintain_those_moments"]
        .as_str()
        .unwrap();
    assert!(c2.contains("keeps a level"));
    assert!(c2.contains("SUPPLIED window"));

    // ★★ Correction 3 — my own prior claim.
    let c3 = d["★★_a_correction_to_MY_OWN_prior_claim"].as_str().unwrap();
    assert!(c3.contains("It does not"));
    assert!(c3.contains("ONE OF THE INPUTS"));
    assert!(c3.contains("OPV-16"));
    assert!(c3.contains("conflated"));

    // The counterweight is the discipline, and it is credited.
    let cw = d["term_by_term"]["★ a reading surfaces, never triggers"].as_str().unwrap();
    assert!(cw.contains("AT PARITY IN DISCIPLINE"));
    assert!(cw.contains("advisory.py"));
    let what = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(what.contains("one schema change away"));

    // The false positive that a security-conscious codebase makes likely.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["critical (2 hits, 0 real)"].as_str().unwrap().contains("Emphasis, not a phase transition"));

    // Five honest limits, including why the tail is slotted structurally.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "TAIL SHAPE IS SLOTTED",
        "STRUCTURAL not scheduling",
        "RIGHT-CENSORING",
        "DANGLING",
        "A FLOOR, NOT s_max",
        "THE CRITICAL BAND IS DECLARED",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }
    // And the constraint a future tail slice must satisfy is already recorded.
    assert!(limits.contains("may not have an absolute 'power law' variant"));

    // The out-of-scope rows are named rather than stubbed.
    let oos = d["out_of_scope_named_not_stubbed"].as_str().unwrap();
    assert!(oos.contains("⟨λ,β,γ⟩"));
    assert!(oos.contains("scenario_ensemble"));

    let groups = [
        "branching_cases",
        "risk_arithmetic_cases",
        "read_only_cases",
        "slowing_down_cases",
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
