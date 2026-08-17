//! Belief state under partial observability, replayed against its conformance
//! vectors (R2 · Monitor §VIII + §I · MON-8).
//!
//! **SPEC vectors, Rust-only — with a coarse staleness notion already at
//! parity.** `ingest_engine.get_sources()` computes `is_stale` against a
//! declared cadence and never flags a source without one, and the
//! needs-attention surface already carries it to a person. What is new is the
//! *value*: an estimate that degrades rather than a flag that flips.
//!
//! ★★ The observation update is EVT-10's snapshot collapse, **not a Kalman
//! filter** — MON-2 is a declined import, and the `point_mass` half of §VIII's
//! pseudocode is the half Sustena already has a rule for.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    BeliefError, BeliefTracker, Collapse, Dynamics, Severity, SilenceSpec, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("belief.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "belief.json was written for a different contract version"
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

// ── the fixture, from the vector ────────────────────────────────────────────

fn hour(doc: &Value) -> i64 {
    doc["fixture"]["HOUR_MS"].as_i64().unwrap()
}

fn spec(doc: &Value, dynamics: Dynamics) -> SilenceSpec {
    SilenceSpec::new(
        doc["fixture"]["drift_rate_per_hour"].as_f64().unwrap(),
        dynamics,
        doc["fixture"]["silence_threshold_hours"].as_i64().unwrap() * hour(doc),
        match doc["fixture"]["severity"].as_str().unwrap() {
            "WARNING" => Severity::Warning,
            "CRITICAL" => Severity::Critical,
            other => panic!("a silence alert cannot be {other}"),
        },
    )
    .unwrap()
}

fn tracker(doc: &Value) -> BeliefTracker {
    BeliefTracker::new(spec(doc, Dynamics::Unknown))
}

fn soil(doc: &Value) -> &str {
    doc["fixture"]["soil"]["dimension"].as_str().unwrap()
}

/// The declared readings: `[value, t_event, id]`.
fn readings(doc: &Value) -> Vec<(f64, i64, String)> {
    doc["fixture"]["soil"]["readings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let a = r.as_array().unwrap();
            (a[0].as_f64().unwrap(), a[1].as_i64().unwrap(), a[2].as_str().unwrap().to_string())
        })
        .collect()
}

// ── ★★ the observation update is EVT-10's snapshot collapse ────────────────

#[test]
fn an_observation_collapses_the_belief_to_a_point_mass() {
    let doc = load();
    let c = case(&doc, "observation_cases", "★_an_observation_collapses_the_belief_to_a_POINT_MASS");
    let mut t = tracker(&doc);
    let (v, tau, id) = readings(&doc)[0].clone();

    assert_eq!(t.observe(soil(&doc), v, tau, id), Collapse::FirstContact { mean: v });
    let r = t.read(soil(&doc), tau).unwrap();

    assert_eq!(r.estimated, c["expect"]["estimated"].as_f64().unwrap());
    assert_eq!(r.uncertainty, c["expect"]["uncertainty"].as_f64().unwrap());
    assert_eq!(r.last_contact, c["expect"]["last_contact"].as_i64().unwrap());
}

#[test]
fn a_stale_observation_arriving_late_does_not_collapse_the_belief() {
    // ★★ THE COHERENCE PROOF. The belief holds an EVT-10 `SnapshotDim` and the
    // supersede runs through it, so a late reading loses here exactly as it
    // loses on the state dimension.
    let doc = load();
    let c = case(
        &doc,
        "observation_cases",
        "★★_a_STALE_observation_arriving_LATE_does_NOT_collapse_the_belief",
    );
    let stale = &doc["fixture"]["stale_reading"];
    let kept = c["expect"]["kept"].as_f64().unwrap();
    let kept_at = c["expect"]["kept_at"].as_i64().unwrap();

    let mut t = tracker(&doc);
    t.observe("finances.liquid.balance", kept, kept_at, "sms-2");

    let outcome = t.observe(
        "finances.liquid.balance",
        stale["value"].as_f64().unwrap(),
        stale["t_event"].as_i64().unwrap(),
        stale["id"].as_str().unwrap(),
    );
    assert_eq!(outcome, Collapse::Stale { kept, kept_at });
    assert_eq!(t.read("finances.liquid.balance", kept_at).unwrap().estimated, kept);
}

#[test]
fn an_equal_event_time_breaks_on_the_id_as_it_does_on_the_state_dimension() {
    // ★ The same total order, reused — checked in both directions so it is the
    // order deciding and not the arrival sequence.
    let doc = load();
    let mut a = tracker(&doc);
    a.observe("x", 1.0, 100, "a");
    assert!(matches!(a.observe("x", 2.0, 100, "z"), Collapse::Collapsed { .. }));

    let mut b = tracker(&doc);
    b.observe("x", 1.0, 100, "z");
    assert!(matches!(b.observe("x", 2.0, 100, "a"), Collapse::Stale { .. }));
}

#[test]
fn hearing_again_collapses_the_variance_back_to_zero() {
    let doc = load();
    let c = case(&doc, "observation_cases", "hearing_again_collapses_the_variance_back_to_zero");
    let r = readings(&doc);
    let mut t = tracker(&doc);
    t.observe(soil(&doc), r[0].0, r[0].1, r[0].2.clone());

    let at = r[1].1;
    assert_eq!(t.read(soil(&doc), at).unwrap().uncertainty, c["expect"]["before"].as_f64().unwrap());

    t.observe(soil(&doc), r[1].0, r[1].1, r[1].2.clone());
    let after = t.read(soil(&doc), at).unwrap();
    assert_eq!(after.uncertainty, c["expect"]["after_new_reading"].as_f64().unwrap());
    assert_eq!(after.estimated, c["expect"]["estimated"].as_f64().unwrap());
}

// ── ★★ silence widens, and does not invent dynamics ────────────────────────

#[test]
fn uncertainty_grows_monotonically_with_silence() {
    // ★★ Checked across the whole curve, not one point — a formula that dipped
    // anywhere would be worse than one that was simply wrong.
    let doc = load();
    let c = case(&doc, "silence_cases", "★★_uncertainty_grows_MONOTONICALLY_with_silence");
    let h = hour(&doc);
    let mut t = tracker(&doc);
    t.observe(soil(&doc), 42.0, 0, "s1");

    let mut last = -1.0;
    for i in 0..24 {
        let v = t.read(soil(&doc), i * h).unwrap().uncertainty;
        assert!(v >= last, "variance shrank between hour {} and {i}", i - 1);
        last = v;
    }
    assert_eq!(
        t.read(soil(&doc), 3 * h).unwrap().uncertainty,
        c["expect"]["variance_at_3h"].as_f64().unwrap(),
        "σ = elapsed_hours × drift_rate, variance = σ²"
    );
}

#[test]
fn with_no_transition_model_the_mean_is_held_and_only_uncertainty_grows() {
    // ★★ The honest floor: inventing dynamics would put a made-up trajectory
    // behind a real-looking mean.
    let doc = load();
    let c = case(
        &doc,
        "silence_cases",
        "★★_with_NO_transition_model_the_mean_is_HELD_and_only_uncertainty_grows",
    );
    let mut t = tracker(&doc);
    t.observe(soil(&doc), 42.0, 0, "s1");

    let far = t.read(soil(&doc), 48 * hour(&doc)).unwrap();
    assert_eq!(far.estimated, c["expect"]["estimated_after_48h"].as_f64().unwrap());
    assert!(far.uncertainty > c["expect"]["uncertainty_after_48h_gt"].as_f64().unwrap());
}

#[test]
fn a_declared_drift_moves_the_mean_and_nothing_else_does() {
    let doc = load();
    let c = case(&doc, "silence_cases", "★_a_DECLARED_drift_moves_the_mean_and_nothing_else_does");
    let per_hour = c["expect"]["per_hour"].as_f64().unwrap();

    let mut t = BeliefTracker::new(spec(&doc, Dynamics::LinearDrift { per_hour }));
    t.observe(soil(&doc), 42.0, 0, "s1");
    assert_eq!(
        t.read(soil(&doc), 4 * hour(&doc)).unwrap().estimated,
        c["expect"]["estimated_after_4h"].as_f64().unwrap()
    );
}

// ── ★ the escalation is an admission, not an alarm ─────────────────────────

#[test]
fn silence_past_the_threshold_alerts_and_before_it_does_not() {
    let doc = load();
    let c = case(
        &doc,
        "escalation_cases",
        "★_silence_past_the_threshold_alerts_and_before_it_does_not",
    );
    let h = hour(&doc);
    let mut t = tracker(&doc);
    t.observe(soil(&doc), 42.0, 0, "s1");

    assert!(c["expect"]["at_5h"].is_null());
    assert!(t.read(soil(&doc), 5 * h).unwrap().alert.is_none());
    assert!(t.silent(5 * h).is_empty());

    let alert = t.read(soil(&doc), 7 * h).unwrap().alert.expect("past the threshold");
    assert_eq!(alert.silent_for_ms, c["expect"]["at_7h_silent_for_ms"].as_i64().unwrap());
    assert!(alert.uncertainty > 0.0, "the reason travels with it");
}

#[test]
fn the_alert_rides_the_existing_monitor_to_controller_boundary() {
    // ★★ `Severity::escalates()` reused rather than a second notion of
    // crossing — and deliberately NOT a `detect::Alert`, because that carries a
    // `Shift` and nothing shifted.
    let doc = load();
    let c = case(
        &doc,
        "escalation_cases",
        "★★_the_alert_rides_the_EXISTING_Monitor_to_Controller_boundary",
    );
    let mut t = tracker(&doc);
    t.observe(soil(&doc), 42.0, 0, "s1");

    let alert = &t.silent(7 * hour(&doc))[0];
    assert_eq!(alert.escalates(), c["expect"]["escalates"].as_bool().unwrap());
    assert!(alert.describe().contains(c["expect"]["describe_contains"].as_str().unwrap()));
}

#[test]
fn a_silence_alert_can_never_be_info() {
    // ★★ The one severity that would contradict §VIII is refused at
    // declaration rather than allowed and hoped about.
    let doc = load();
    let h = hour(&doc);
    assert_eq!(
        SilenceSpec::new(1.0, Dynamics::Unknown, h, Severity::Info),
        Err(BeliefError::SilenceCannotBeInfo)
    );
    assert!(SilenceSpec::new(1.0, Dynamics::Unknown, h, Severity::Critical).is_ok());
    assert!(!Severity::Info.escalates(), "which is exactly why it is refused");
}

#[test]
fn hearing_again_clears_the_alert() {
    let doc = load();
    let c = case(&doc, "escalation_cases", "hearing_again_CLEARS_the_alert");
    let h = hour(&doc);
    let mut t = tracker(&doc);
    t.observe(soil(&doc), 42.0, 0, "s1");

    assert_eq!(t.silent(7 * h).len() as u64, c["expect"]["before"].as_u64().unwrap());
    t.observe(soil(&doc), 40.0, 7 * h, "s2");
    assert_eq!(t.silent(7 * h).len() as u64, c["expect"]["after"].as_u64().unwrap());
}

#[test]
fn only_what_actually_went_quiet_raises_anything() {
    let doc = load();
    let c = case(&doc, "escalation_cases", "only_what_ACTUALLY_went_quiet_raises_anything");
    let second = &doc["fixture"]["second"];
    let h = hour(&doc);

    let mut t = tracker(&doc);
    t.observe(soil(&doc), 42.0, 0, "s1");
    t.observe(
        second["dimension"].as_str().unwrap(),
        second["value"].as_f64().unwrap(),
        second["t_event"].as_i64().unwrap(),
        second["id"].as_str().unwrap(),
    );

    assert_eq!(t.render(7 * h).len() as u64, c["expect"]["rendered"].as_u64().unwrap());
    let silent = t.silent(7 * h);
    assert_eq!(silent.len() as u64, c["expect"]["silent"].as_u64().unwrap());
    assert_eq!(silent[0].dimension, c["expect"]["silent_dimension"].as_str().unwrap());
}

// ── §I: unmeasured state is ungoverned state ───────────────────────────────

#[test]
fn a_governed_dimension_never_observed_is_reported_ungoverned() {
    // ★ The concrete half that needs no matrix. Rank analysis is MON-1 and is
    // a slot — a rough answer to a rank question is worse than none.
    let doc = load();
    let c = case(
        &doc,
        "observability_cases",
        "★_a_GOVERNED_dimension_never_observed_is_reported_UNGOVERNED",
    );
    let other = doc["fixture"]["second"]["dimension"].as_str().unwrap();

    let mut t = tracker(&doc);
    t.govern(soil(&doc));
    t.govern(other);
    t.observe(soil(&doc), 42.0, 0, "s1");

    let want: Vec<String> = c["expect"]["ungoverned"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(t.ungoverned(), want);

    t.observe(other, 10.0, 0, "t1");
    assert!(t.ungoverned().is_empty());
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The no-Kalman position is recorded as a POSITION, with its citation.
    let k = doc["★★_the_no_Kalman_position_and_why_it_is_a_POSITION_not_a_shortcut"]
        .as_str()
        .unwrap();
    assert!(k.contains("declined import"));
    assert!(k.contains("SnapshotDim"), "and names what replaced it");
    assert!(
        d["term_by_term"]["Kalman"].as_str().unwrap().contains("NEITHER SIDE HAS IT"),
        "the absence must not read as a gap on either side"
    );

    // ★ The counterweight: a coarse staleness notion already exists, correctly
    // scoped — it never flags a source with no declared cadence.
    let key = "★_the_counterweight_a_COARSE_staleness_notion_ALREADY_EXISTS_and_it_is_the_right_idea";
    let cw = d[key].as_str().unwrap();
    assert!(cw.contains("is_stale") && cw.contains("expected_interval_minutes"));
    assert!(cw.contains("never flagged"), "the care it takes is credited, not glossed");

    // ★★ And the gap is stated precisely rather than as "no belief state".
    let shape = d["★★_the_shape_of_the_gap_stated_precisely"].as_str().unwrap();
    for term in ["Per-dimension", "Continuous", "degrades"] {
        assert!(shape.contains(term), "the gap must name {term}");
    }

    // The false positive that reads topical and is the opposite kind of thing.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("variance")));
    assert!(fp.values().any(|v| v.as_str().is_some_and(|s| s.contains("procurement"))));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "NOT A JOINT POMDP BELIEF",
        "THE POINT-MASS COLLAPSE ASSUMES THE OBSERVATION IS AUTHORITATIVE",
        "NO TRANSITION MODEL IS INVENTED",
        "DRIFT RATE IS DECLARED, NOT LEARNED",
        "OBSERVABILITY MATRIX IS NOT BUILT",
        "NOT WIRED INTO `MonitorEngine`",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }
    // ★ The noisy-sensor cost is tied back to the declined import, not hidden.
    assert!(limits.contains("where a Kalman filter would come in"));

    let groups =
        ["observation_cases", "silence_cases", "escalation_cases", "observability_cases"];
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
