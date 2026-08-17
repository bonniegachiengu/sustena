//! Damping, replayed against its conformance vectors
//! (R2 · Controller Additions + §II · CTL-11).
//!
//! **SPEC vectors, Rust-only — and the counterweight is INTRA-RUST**, which is
//! a correction the greps forced: the time-domain half is not at parity either.
//! `lyapunov` is grep-0 on the other side and the four `stable` hits are a
//! stable sort, a stable fingerprint and two testables. CTL-2 is a Rust row
//! that shipped earlier, so what this adds is the frequency-domain face on top
//! of a check that already exists *here*.
//!
//! ★★ The headline runs the **real `is_stable_intervention`** over a ringing
//! trajectory and asserts it returns stable at every step — *"Lyapunov says the
//! gap closes; damping says it closes without oscillation."*
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    controller::is_stable_intervention, crossings, overshoot, read_trajectory, DampingError,
    DampingVerdict, Interval, PoleVerdict, ProportionalLaw, Region, Side, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("damping.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "damping.json was written for a different contract version"
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

// ── the household band, from the vector ─────────────────────────────────────

fn band(doc: &Value) -> Region {
    let b = &doc["fixture"]["band"];
    Region::new().bounding(Interval::new(
        b["dimension"].as_str().unwrap(),
        b["lo"].as_f64().unwrap(),
        b["hi"].as_f64().unwrap(),
    ))
}

fn at(v: f64) -> Value {
    json!({ "finances": { "liquid": v } })
}

fn centre(doc: &Value) -> f64 {
    doc["fixture"]["band"]["centre"].as_f64().unwrap()
}

fn gain(doc: &Value, which: &str) -> ProportionalLaw {
    ProportionalLaw::declared(doc["fixture"]["gains"][which].as_f64().unwrap()).unwrap()
}

fn traj(doc: &Value, which: &str) -> Vec<f64> {
    doc["fixture"]["trajectories"][which]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

// ── ★★ the blind spot CTL-2 cannot see ─────────────────────────────────────

#[test]
fn a_ringing_loop_passes_lyapunov_at_every_single_step() {
    // ★★ THE HEADLINE, with the REAL per-step check run over the trajectory.
    let doc = load();
    let c = case(
        &doc,
        "lyapunov_blind_spot_cases",
        "★★_a_RINGING_loop_passes_LYAPUNOV_at_EVERY_SINGLE_STEP",
    );
    let (region, law, mid) = (band(&doc), gain(&doc, "underdamped"), centre(&doc));

    let mut error: f64 = -100.0;
    for _ in 0..6 {
        let before = at(mid + error);
        error = law.step(error);
        let after = at(mid + error);
        assert!(
            is_stable_intervention(&region, &before, &after).unwrap().stable,
            "Lyapunov passes at every step of a ringing loop"
        );
    }

    let v = law.classify();
    assert_eq!(v.rings(), c["expect"]["rings"].as_bool().unwrap());
    assert_eq!(v.settles(), c["expect"]["settles"].as_bool().unwrap(), "it does converge");
    assert!(v.advice().unwrap().contains(c["expect"]["advice_contains"].as_str().unwrap()));
}

#[test]
fn a_sustained_oscillation_passes_lyapunov_forever_with_equality() {
    // ★★ The sharpest case, and it is arithmetic: at gain 2 the pole is −1, so
    // W never changes and `≤` holds for ever while nothing settles.
    let doc = load();
    let c = case(
        &doc,
        "lyapunov_blind_spot_cases",
        "★★_a_SUSTAINED_oscillation_passes_LYAPUNOV_FOREVER_with_EQUALITY",
    );
    let (region, law, mid) = (band(&doc), gain(&doc, "sustained"), centre(&doc));

    let mut error: f64 = -80.0;
    for _ in 0..8 {
        let before = at(mid + error);
        error = law.step(error);
        let after = at(mid + error);
        let s = is_stable_intervention(&region, &before, &after).unwrap();
        assert!(s.stable, "equality satisfies `≤`");
        assert_eq!(s.before, s.after, "and the distance never moves");
    }

    match law.classify() {
        PoleVerdict::Undamped { pole, sustained } => {
            assert_eq!(pole, c["expect"]["pole"].as_f64().unwrap());
            assert_eq!(sustained, c["expect"]["sustained"].as_bool().unwrap());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(law.classify().settles(), c["expect"]["settles"].as_bool().unwrap());
    assert!(law
        .classify()
        .advice()
        .unwrap()
        .contains(c["expect"]["advice_contains"].as_str().unwrap()));
}

#[test]
fn an_overshoot_reduces_w_and_is_still_the_failure() {
    // ★ The one-step form: W falls, CTL-2 is satisfied, and the state has
    // crossed to the far side.
    let doc = load();
    let c = case(
        &doc,
        "lyapunov_blind_spot_cases",
        "★_an_OVERSHOOT_reduces_W_and_is_STILL_the_failure",
    );
    let region = band(&doc);
    let o = &doc["fixture"]["overshoot"];
    let (before, after) = (at(o["before"].as_f64().unwrap()), at(o["after"].as_f64().unwrap()));

    let s = is_stable_intervention(&region, &before, &after).unwrap();
    assert_eq!(s.stable, c["expect"]["lyapunov_stable"].as_bool().unwrap());
    assert!(s.after < s.before, "W genuinely fell");

    let over = overshoot(&region, &before, &after);
    assert_eq!(over.len(), c["expect"]["overshoots"].as_u64().unwrap() as usize);
    assert_eq!(format!("{:?}", over[0].from), c["expect"]["from"].as_str().unwrap());
    assert_eq!(format!("{:?}", over[0].to), c["expect"]["to"].as_str().unwrap());
    assert!(over[0].describe().contains(c["expect"]["describe_contains"].as_str().unwrap()));
}

#[test]
fn a_damped_correction_lands_inside_and_is_no_overshoot() {
    let doc = load();
    let c = case(
        &doc,
        "lyapunov_blind_spot_cases",
        "a_DAMPED_correction_lands_INSIDE_and_is_no_overshoot",
    );
    let region = band(&doc);
    let d = &doc["fixture"]["damped_correction"];
    let (before, after) = (at(d["before"].as_f64().unwrap()), at(d["after"].as_f64().unwrap()));

    assert_eq!(
        is_stable_intervention(&region, &before, &after).unwrap().stable,
        c["expect"]["lyapunov_stable"].as_bool().unwrap()
    );
    assert_eq!(
        overshoot(&region, &before, &after).len() as u64,
        c["expect"]["overshoots"].as_u64().unwrap(),
        "it arrived, it did not pass"
    );
}

// ── ★ the pole, where a gain is declared ───────────────────────────────────

#[test]
fn a_damped_gain_settles_from_the_same_side() {
    let doc = load();
    let c = case(&doc, "pole_cases", "★_a_DAMPED_gain_settles_from_the_SAME_side");
    let law = gain(&doc, "damped");

    assert_eq!(law.pole(), c["expect"]["pole"].as_f64().unwrap());
    assert!(matches!(law.classify(), PoleVerdict::Damped { .. }));
    assert_eq!(law.classify().settles(), c["expect"]["settles"].as_bool().unwrap());
    assert_eq!(law.classify().rings(), c["expect"]["rings"].as_bool().unwrap());
    assert!(c["expect"]["advice"].is_null());
    assert!(law.classify().advice().is_none());
}

#[test]
fn a_unit_gain_is_deadbeat() {
    let doc = load();
    let c = case(&doc, "pole_cases", "a_UNIT_gain_is_DEADBEAT");
    let law = gain(&doc, "deadbeat");

    assert_eq!(law.pole(), c["expect"]["pole"].as_f64().unwrap());
    assert_eq!(law.classify(), PoleVerdict::Deadbeat);
    assert_eq!(law.step(-100.0), 0.0);
}

#[test]
fn a_gain_past_two_diverges() {
    let doc = load();
    let c = case(&doc, "pole_cases", "★_a_gain_PAST_TWO_diverges");
    let law = gain(&doc, "divergent");

    assert_eq!(law.pole(), c["expect"]["pole"].as_f64().unwrap());
    match law.classify() {
        PoleVerdict::Undamped { sustained, .. } => {
            assert_eq!(sustained, c["expect"]["sustained"].as_bool().unwrap())
        }
        other => panic!("{other:?}"),
    }
    assert!(law
        .classify()
        .advice()
        .unwrap()
        .contains(c["expect"]["advice_contains"].as_str().unwrap()));
}

#[test]
fn a_gain_must_be_declared_and_cannot_be_fitted() {
    // ★★ The MON-1 discipline made structural: there is no
    // `fit_gain(trajectory)` in this module, and the ABSENCE is the substance.
    let doc = load();
    let c = case(&doc, "pole_cases", "★★_a_gain_must_be_DECLARED_and_CANNOT_BE_FITTED");
    assert_eq!(c["expect"]["nan_refused"].as_str().unwrap(), "BadGain");
    assert!(matches!(ProportionalLaw::declared(f64::NAN), Err(DampingError::BadGain(_))));
    assert!(ProportionalLaw::declared(0.7).is_ok());
}

// ── the trajectory reading, where no gain is declared ──────────────────────

#[test]
fn a_monotonically_falling_trajectory_reads_damped() {
    let doc = load();
    let c = case(&doc, "trajectory_cases", "a_MONOTONICALLY_FALLING_trajectory_reads_DAMPED");
    match read_trajectory(&traj(&doc, "falling")) {
        DampingVerdict::Damped { steps, settled } => {
            assert_eq!(steps as u64, c["expect"]["steps"].as_u64().unwrap());
            assert_eq!(settled, c["expect"]["settled"].as_bool().unwrap());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_alternating_trajectory_reads_ringing() {
    let doc = load();
    let c = case(&doc, "trajectory_cases", "★_an_ALTERNATING_trajectory_reads_RINGING");
    match read_trajectory(&traj(&doc, "alternating")) {
        DampingVerdict::Ringing { alternations, steps } => {
            assert_eq!(steps as u64, c["expect"]["steps"].as_u64().unwrap());
            assert_eq!(alternations as u64, c["expect"]["alternations"].as_u64().unwrap());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn fewer_than_three_points_is_indeterminate_not_damped() {
    // ★★ Two points have one direction and no chance to change it.
    let doc = load();
    let c = case(
        &doc,
        "trajectory_cases",
        "★★_fewer_than_THREE_points_is_INDETERMINATE_not_damped",
    );
    let v = read_trajectory(&traj(&doc, "too_short"));

    assert!(v.is_indeterminate());
    assert_eq!(v.rings(), c["expect"]["rings"].as_bool().unwrap());
    match v {
        DampingVerdict::Indeterminate { reason } => {
            assert!(reason.contains(c["expect"]["reason_contains"].as_str().unwrap()))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn repeated_boundary_crossings_are_the_other_face_of_ringing() {
    let doc = load();
    let c = case(
        &doc,
        "trajectory_cases",
        "★_repeated_BOUNDARY_CROSSINGS_are_the_other_face_of_ringing",
    );
    assert_eq!(
        crossings(&[Side::Below, Side::Above, Side::Below, Side::Above]) as u64,
        c["expect"]["alternating_crossings"].as_u64().unwrap()
    );
    assert_eq!(
        crossings(&[Side::Below, Side::Inside, Side::Inside]) as u64,
        c["expect"]["settling_crossings"].as_u64().unwrap()
    );
}

#[test]
fn the_magnitude_series_cannot_see_a_sign_flip_and_the_sides_can() {
    // ★★ The finding that made both native readings ship. A ringing loop's
    // |W| falls every step, so the magnitude reading calls it Damped and is not
    // wrong to — the side series is what carries the ringing there.
    let doc = load();
    let c = case(
        &doc,
        "trajectory_cases",
        "★★_the_MAGNITUDE_series_CANNOT_see_a_sign_flip_and_the_SIDES_can",
    );
    let law = gain(&doc, "underdamped");

    let mut error: f64 = -100.0;
    let mut w = vec![error.abs()];
    let mut sides = vec![Side::Below];
    for _ in 0..5 {
        error = law.step(error);
        w.push(error.abs());
        sides.push(if error < 0.0 { Side::Below } else { Side::Above });
    }

    assert_eq!(
        read_trajectory(&w).rings(),
        c["expect"]["magnitude_reads_ringing"].as_bool().unwrap(),
        "|W| falls monotonically, so the magnitude reading sees no reversal"
    );
    assert_eq!(law.classify().rings(), c["expect"]["pole_says_rings"].as_bool().unwrap());
    assert!(
        crossings(&sides) as u64 > c["expect"]["side_crossings_gt"].as_u64().unwrap(),
        "and the crossings show what the magnitudes hide"
    );
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The framing correction the greps forced.
    let key = "★★_the_counterweight_is_INTRA_RUST_and_a_framing_CORRECTION_belongs_here";
    let cw = d[key].as_str().unwrap();
    assert!(cw.contains("it is not"), "the easy claim is corrected, not repeated");
    assert!(cw.contains("grep-0"));
    assert!(cw.contains("intra-Rust"), "and the real counterweight is named");

    // ★ The reference has the remedy for the symptom — credited, then placed.
    assert!(d["★_what_the_reference_DOES_have_is_the_REMEDY_for_the_SYMPTOM"]
        .as_str()
        .unwrap()
        .contains("control.rollback"));

    // ★★ The honest-fit block, and that nothing is fitted.
    let fit = &doc["★★_honest_fit_the_pole_where_a_gain_EXISTS_the_trajectory_where_not"];
    assert!(fit["★★_and_where_there_is_no_gain_NOTHING_IS_FITTED"]
        .as_str()
        .unwrap()
        .contains("fabricated-`A` trap"));
    assert!(fit["★★_and_the_two_native_readings_are_NOT_redundant"]
        .as_str()
        .unwrap()
        .contains("magnitude"));

    // The false positives that would have produced a wrong counterweight.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.contains("stable (4 hits")));
    assert!(fp.keys().any(|k| k.contains("gain (149 apparent")));
    assert_eq!(
        fp.keys().filter(|k| k.starts_with("gain")).count(),
        1,
        "one gain entry, not two saying different things"
    );

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "POLES ARE DECLINED WHERE NO GAIN EXISTS, NOT APPROXIMATED",
        "THE TWO NATIVE READINGS SEE DIFFERENT THINGS",
        "THREE POINTS MINIMUM",
        "NO REMEDY IS APPLIED",
        "OVERSHOOT IS PER DECLARED INTERVAL",
        "PAWA-METERED TIE-IN IS A NAMED SLOT",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = ["lyapunov_blind_spot_cases", "pole_cases", "trajectory_cases"];
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
