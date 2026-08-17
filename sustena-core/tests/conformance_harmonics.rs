//! Frequency-domain change, replayed against its conformance vectors
//! (R2 · Monitor Additions · MON-13).
//!
//! **SPEC vectors, Rust-only — with the substrate at parity and the need real
//! there today.** `detect.rs`'s EWMA/CUSUM over `W = d(s,V)` already ship, and
//! `MonitorEngine` already keeps the very history this transforms; the
//! household already has monthly rhythms (`period: "monthly"` on a pocket).
//! What is missing on the other side is any way to look at periodicity.
//!
//! ★★ The claim *"the time-domain detectors cannot do this alone"* is proven
//! rather than argued: a case runs the **real `Cusum`** over the same series
//! the frequency-domain reading calls an expected cycle, and asserts it fires.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::f64::consts::TAU;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    read_harmonics, spectrum, CycleProvenance, CycleVerdict, Cusum, CusumSpec, HarmonicsError,
    HarmonicsSpec, KnownCycle, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("harmonics.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "harmonics.json was written for a different contract version"
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

// ── the household fixture ───────────────────────────────────────────────────

fn f(doc: &Value, key: &str) -> f64 {
    doc["fixture"][key].as_f64().unwrap()
}

fn u(doc: &Value, key: &str) -> usize {
    doc["fixture"][key].as_u64().unwrap() as usize
}

/// A spend series with a bill spike every `period` samples.
fn series(doc: &Value, period: usize) -> Vec<f64> {
    let (n, base, spike) = (u(doc, "window"), f(doc, "baseline"), f(doc, "spike"));
    (0..n).map(|i| if i % period == 0 { base + spike } else { base }).collect()
}

fn monthly(doc: &Value) -> HarmonicsSpec {
    HarmonicsSpec::new(
        vec![KnownCycle::declared("monthly_bill", u(doc, "on_cycle_period") as f64)],
        f(doc, "residual_threshold"),
        u(doc, "min_cycles_to_establish"),
    )
    .unwrap()
}

// ── the transform ───────────────────────────────────────────────────────────

#[test]
fn a_pure_sinusoid_puts_its_power_in_its_own_bin() {
    let doc = load();
    let c = case(&doc, "transform_cases", "a_pure_sinusoid_puts_its_power_in_its_own_bin");
    let n = u(&doc, "window");
    let k = c["expect"]["peak_k"].as_u64().unwrap() as usize;

    let s: Vec<f64> = (0..n).map(|i| (TAU * k as f64 * i as f64 / n as f64).sin()).collect();
    let peak = spectrum(&s).unwrap().into_iter().max_by(|a, b| a.power.total_cmp(&b.power)).unwrap();

    assert_eq!(peak.k, k);
    assert_eq!(peak.period_samples, c["expect"]["period_samples"].as_f64().unwrap());
    assert!((peak.amplitude - c["expect"]["amplitude"].as_f64().unwrap()).abs() < 1e-9);
}

#[test]
fn the_mean_is_removed_so_a_level_is_not_read_as_a_rhythm() {
    // ★ Otherwise the residual would be a statement about how much a household
    // spends rather than about whether the pattern changed.
    let doc = load();
    let flat = vec![f(&doc, "baseline"); u(&doc, "window")];
    assert!(spectrum(&flat).unwrap().iter().all(|c| c.power < 1e-18));
}

#[test]
fn a_window_too_short_or_not_finite_is_refused() {
    assert!(matches!(spectrum(&[1.0, 2.0]), Err(HarmonicsError::WindowTooShort(2))));
    assert!(matches!(
        spectrum(&[1.0, 2.0, f64::NAN, 4.0]),
        Err(HarmonicsError::NonFiniteSample { index: 2, .. })
    ));
}

// ── ★★ the payoff: a known season does not alarm ───────────────────────────

#[test]
fn a_monthly_bill_arriving_on_schedule_is_an_expected_cycle() {
    let doc = load();
    let c = case(
        &doc,
        "suppression_cases",
        "★★_a_monthly_bill_arriving_ON_SCHEDULE_is_an_EXPECTED_CYCLE",
    );
    let r = read_harmonics(&series(&doc, u(&doc, "on_cycle_period")), &monthly(&doc)).unwrap();

    assert!(r.verdict.is_expected_cycle(), "{:?}", r.verdict);
    assert_eq!(r.verdict.escalates(), c["expect"]["escalates"].as_bool().unwrap());
    assert_eq!(r.attributed[0].0, c["expect"]["attributed_to"].as_str().unwrap());
    assert!(r.explained_power > r.residual_power);
}

#[test]
fn the_time_domain_detector_would_have_fired_on_exactly_that_series() {
    // ★★ The claim made a MEASURED FACT about this codebase's own detector.
    let doc = load();
    let c = case(
        &doc,
        "suppression_cases",
        "★★_the_TIME_DOMAIN_detector_WOULD_HAVE_FIRED_on_exactly_that_series",
    );
    let s = series(&doc, u(&doc, "on_cycle_period"));
    let cs = &doc["fixture"]["cusum"];

    let mut cusum = Cusum::new(CusumSpec::new(
        cs["mu_0"].as_f64().unwrap(),
        cs["delta"].as_f64().unwrap(),
        cs["h"].as_f64().unwrap(),
    ))
    .unwrap();
    let fired = s.iter().any(|w| cusum.update(*w).is_some());

    assert_eq!(fired, c["expect"]["cusum_fires"].as_bool().unwrap());
    assert_eq!(
        read_harmonics(&s, &monthly(&doc)).unwrap().verdict.is_expected_cycle(),
        c["expect"]["harmonics_says_expected_cycle"].as_bool().unwrap(),
        "complement, not veto"
    );
}

#[test]
fn the_same_magnitude_arriving_off_cycle_is_a_genuine_shift() {
    // ★★ What stops suppression from being a blanket mute.
    let doc = load();
    let c = case(
        &doc,
        "suppression_cases",
        "★★_the_SAME_MAGNITUDE_arriving_OFF_CYCLE_is_a_GENUINE_SHIFT",
    );
    let r = read_harmonics(&series(&doc, u(&doc, "off_cycle_period")), &monthly(&doc)).unwrap();

    match r.verdict {
        CycleVerdict::GenuineShift { residual_fraction, severity, .. } => {
            assert!(residual_fraction > c["expect"]["residual_fraction_gt"].as_f64().unwrap());
            assert_eq!(severity.escalates(), c["expect"]["escalates"].as_bool().unwrap());
        }
        other => panic!("an off-cycle spike must surface: {other:?}"),
    }
}

#[test]
fn a_new_rhythm_on_top_of_a_known_one_still_surfaces() {
    // ★ The realistic case — the bill is still arriving AND something started.
    let doc = load();
    let n = u(&doc, "window");
    let mut s = series(&doc, u(&doc, "on_cycle_period"));
    for (i, v) in s.iter_mut().enumerate() {
        *v += 200.0 * (TAU * 7.0 * i as f64 / n as f64).sin();
    }
    assert!(matches!(
        read_harmonics(&s, &monthly(&doc)).unwrap().verdict,
        CycleVerdict::GenuineShift { .. }
    ));
}

// ── ★★ authorised AND resolvable ───────────────────────────────────────────

#[test]
fn a_period_the_window_cannot_resolve_is_indeterminate_not_expected() {
    // ★★ Declaration authorises a cycle; it does not make a short window able
    // to measure it. Suppressing here would remove a real shift under the name
    // of a season.
    let doc = load();
    let c = case(
        &doc,
        "honesty_cases",
        "★★_a_period_the_WINDOW_CANNOT_RESOLVE_is_INDETERMINATE_not_expected",
    );
    let short: Vec<f64> = (0..12).map(|i| if i % 10 == 0 { 400.0 } else { 100.0 }).collect();
    let spec = HarmonicsSpec::new(vec![KnownCycle::declared("quarterly", 10.0)], 0.35, 2).unwrap();

    match read_harmonics(&short, &spec).unwrap().verdict {
        CycleVerdict::Indeterminate { reason } => {
            for want in c["expect"]["reason_contains"].as_array().unwrap() {
                assert!(reason.contains(want.as_str().unwrap()), "reason missing: {want}");
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unestablished_cycle_cannot_suppress_and_says_why() {
    // ★★ Seen once is not a season — suppressing on it would hide a shift
    // behind a rhythm nobody has confirmed.
    let doc = load();
    let c = case(
        &doc,
        "honesty_cases",
        "★★_an_UNESTABLISHED_cycle_cannot_suppress_and_SAYS_WHY",
    );
    let spec = HarmonicsSpec::new(
        vec![KnownCycle::established("maybe_monthly", u(&doc, "on_cycle_period") as f64, 1)],
        f(&doc, "residual_threshold"),
        3,
    )
    .unwrap();

    match read_harmonics(&series(&doc, u(&doc, "on_cycle_period")), &spec).unwrap().verdict {
        CycleVerdict::Indeterminate { reason } => {
            for want in c["expect"]["reason_contains"].as_array().unwrap() {
                assert!(reason.contains(want.as_str().unwrap()), "reason missing: {want}");
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_same_cycle_established_enough_times_does_suppress() {
    // ★ The difference between this and the case above is THE EVIDENCE, and
    // nothing else — same series, same period, same threshold.
    let doc = load();
    let spec = HarmonicsSpec::new(
        vec![KnownCycle::established("monthly_bill", u(&doc, "on_cycle_period") as f64, 8)],
        f(&doc, "residual_threshold"),
        3,
    )
    .unwrap();
    assert!(read_harmonics(&series(&doc, u(&doc, "on_cycle_period")), &spec)
        .unwrap()
        .verdict
        .is_expected_cycle());
}

#[test]
fn indeterminate_is_neither_of_the_other_two_answers() {
    // ★ The same third-answer family as `Skew::Unknown` / `Soundness::Unavailable`.
    let v = CycleVerdict::Indeterminate { reason: "too short".into() };
    assert!(!v.escalates());
    assert!(!v.is_expected_cycle());
}

// ── the declared policy, and the EVT-11 tie-in ─────────────────────────────

#[test]
fn a_nonsense_policy_is_refused_at_declaration() {
    assert!(matches!(
        HarmonicsSpec::new(vec![], 1.5, 2),
        Err(HarmonicsError::BadResidualThreshold(_))
    ));
    assert!(matches!(HarmonicsSpec::new(vec![], 0.3, 1), Err(HarmonicsError::BadMinCycles)));
    assert!(matches!(
        HarmonicsSpec::new(vec![KnownCycle::declared("bad", 1.0)], 0.3, 2),
        Err(HarmonicsError::BadPeriod { .. })
    ));
}

#[test]
fn a_cycle_derived_from_a_real_evt11_recurrence_matches_its_period() {
    // ★ A declared monthly budget period IS a declared cycle — and EVT-11
    // expands REAL periods, so January is 31 samples rather than a nominal 30.4.
    use sustena_core::{
        CivilDateTime, DstPolicy, Lateness, LocalResolution, Recurrence, TzError, TzProvider,
    };

    struct Utc;
    impl TzProvider for Utc {
        fn tzdata_version(&self) -> String {
            "test".into()
        }
        fn offset_at(&self, _t: &str, _l: CivilDateTime) -> Result<LocalResolution, TzError> {
            Ok(LocalResolution::Unambiguous { offset_ms: 0 })
        }
    }

    let doc = load();
    let c = case(
        &doc,
        "policy_cases",
        "★_a_cycle_derived_from_a_REAL_EVT_11_recurrence_matches_its_period",
    );

    let periods = Recurrence::monthly(
        CivilDateTime::date(2026, 1, 1),
        "Etc/UTC",
        1,
        Lateness::Drop,
        DstPolicy::ShiftForward,
    )
    .expand(&Utc, 3)
    .unwrap();

    let cycle =
        KnownCycle::from_periods("monthly_budget", &periods, 24 * 3_600_000).unwrap();
    assert_eq!(cycle.period_samples, c["expect"]["period_samples"].as_f64().unwrap());
    assert_eq!(cycle.provenance, CycleProvenance::Declared);
}

#[test]
fn a_cycle_from_evt11_periods_needs_at_least_two_of_them() {
    assert!(matches!(
        KnownCycle::from_periods("monthly", &[], 1000),
        Err(HarmonicsError::NotARecurrence { given: 0 })
    ));
}

#[test]
fn two_cycles_landing_in_one_bin_cannot_double_count_its_power() {
    let doc = load();
    let p = u(&doc, "on_cycle_period") as f64;
    let spec = HarmonicsSpec::new(
        vec![KnownCycle::declared("a", p), KnownCycle::declared("b", p + 0.05)],
        f(&doc, "residual_threshold"),
        2,
    )
    .unwrap();
    let r = read_harmonics(&series(&doc, u(&doc, "on_cycle_period")), &spec).unwrap();
    assert!(r.explained_power <= r.total_power + 1e-9);
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The reconciliation that corrected the row that sent this slice here.
    let r = &doc["reconciliation_before_building"];
    let sig = r["★★_the_MON_13_row_said_this_consumes_signal_rs_and_it_CANNOT"].as_str().unwrap();
    assert!(sig.contains("excitable-medium"));
    assert!(sig.contains("cross-reference"), "and where the confusion came from");
    assert!(r["★_what_it_actually_reads"].as_str().unwrap().contains("supplied"));

    // ★★ And the correction testing forced, with its cost named.
    let h = r["★★_and_TESTING_forced_a_correction_inspection_would_not_have"].as_str().unwrap();
    assert!(h.contains("impulse"));
    assert!(h.contains("absorbed with it"), "the cost of harmonic attribution is stated");

    // ★ The counterweight: the substrate is at parity AND the need is real.
    let key = "★_the_counterweight_the_SUBSTRATE_is_at_parity_and_the_NEED_is_real_there_TODAY";
    let cw = d[key].as_str().unwrap();
    assert!(cw.contains("MonitorEngine"));
    assert!(cw.contains("period: \\\"monthly\\\"") || cw.contains("monthly"));

    // ★★ And the case is proven by running the real detector.
    assert!(d["★★_and_the_case_is_PROVEN_rather_than_argued"]
        .as_str()
        .unwrap()
        .contains("real `Cusum`"));

    // The false positive most likely to mislead — `frequency` is a param label.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("frequency")));
    assert!(fp.values().any(|v| v.as_str().is_some_and(|s| s.contains("not a component of a series"))));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "A SHIFT ON A HARMONIC OF A KNOWN CYCLE IS ABSORBED",
        "NO WINDOWING OF ITS OWN",
        "THE MEAN IS REMOVED, A LINEAR TREND IS NOT",
        "an FFT is a named scaling slot",
        "SPECTRAL LEAKAGE IS NOT WINDOWED AWAY",
        "NOT WIRED INTO `MonitorEngine`",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = ["transform_cases", "suppression_cases", "honesty_cases", "policy_cases"];
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
