//! The preattentive encoder `φ`, replayed against its conformance vectors
//! (R2 · Monitor §VII · MON-7, the door into the surface block).
//!
//! **SPEC vectors, Rust-only.** ★ Every data dimension encoded here is a number
//! the consolidated `MonitorEngine` already computed — `w` **is** `d(s,V)` —
//! so the encoder adds no state and cannot disagree with the detector about
//! what the series is doing.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    detect::{Alert, CusumSpec, Reading, Severity, Shift},
    monitor::{Ingested, MonitorEngine, SustainWatch},
    preattentive::{
        encode_field, Arrow, Channel, DataDimension, Hue, Motion, VisualAttribute, VisualSpec,
    },
    region::{Interval, Region},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("preattentive.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "preattentive.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn ingested(id: &str, w: f64, smoothed: f64, severity: Option<Severity>) -> Ingested {
    Ingested {
        sustain_id: id.into(),
        reading: Reading {
            w,
            smoothed,
            alert: severity.map(|s| Alert {
                shift: Shift::Upward,
                value: w,
                severity: s,
                excess: 1.0,
            }),
        },
        signals: None,
        cycle: None,
    }
}

fn brightness(spec: &VisualSpec) -> f64 {
    match spec.channel(Channel::Brightness) {
        Some(VisualAttribute::Brightness { value, .. }) => value.get(),
        other => panic!("expected brightness, got {other:?}"),
    }
}

fn arrow(spec: &VisualSpec) -> Arrow {
    match spec.channel(Channel::Orientation) {
        Some(VisualAttribute::Orientation { value, .. }) => *value,
        other => panic!("expected orientation, got {other:?}"),
    }
}

fn motion(spec: &VisualSpec) -> Motion {
    match spec.channel(Channel::Motion) {
        Some(VisualAttribute::Motion { value, .. }) => *value,
        other => panic!("expected motion, got {other:?}"),
    }
}

// ── the closure ──────────────────────────────────────────────────────────────

#[test]
fn every_channel_used_is_preattentive() {
    let a = ingested("a", 3.0, 2.0, Some(Severity::Warning));
    let b = ingested("b", 1.0, 1.0, None);
    for spec in encode_field(&[&a, &b]) {
        for channel in spec.channels_used() {
            assert!(matches!(
                channel,
                Channel::Hue
                    | Channel::Saturation
                    | Channel::Brightness
                    | Channel::Size
                    | Channel::Motion
                    | Channel::Orientation
                    | Channel::Enclosure
            ));
        }
    }
}

#[test]
fn saturation_and_enclosure_are_left_unassigned() {
    let a = ingested("a", 3.0, 2.0, None);
    let b = ingested("b", 1.0, 1.0, None);
    for spec in encode_field(&[&a, &b]) {
        assert!(spec.channel(Channel::Saturation).is_none());
        assert!(spec.channel(Channel::Enclosure).is_none());
    }
}

#[test]
fn each_channel_carries_its_declared_dimension() {
    let a = ingested("a", 3.0, 2.0, Some(Severity::Critical));
    let b = ingested("b", 1.0, 1.0, None);
    let spec = &encode_field(&[&a, &b])[0];
    assert_eq!(spec.carries(Channel::Hue), Some(DataDimension::Status));
    assert_eq!(spec.carries(Channel::Brightness), Some(DataDimension::Urgency));
    assert_eq!(spec.carries(Channel::Size), Some(DataDimension::Magnitude));
    assert_eq!(spec.carries(Channel::Motion), Some(DataDimension::Activity));
    assert_eq!(spec.carries(Channel::Orientation), Some(DataDimension::Trend));
}

#[test]
fn nothing_here_is_a_pixel_or_a_colour_value() {
    let a = ingested("a", 3.0, 2.0, Some(Severity::Critical));
    let b = ingested("b", 1.0, 1.0, None);
    let rendered = format!("{:?}", encode_field(&[&a, &b]));
    for leak in ["#", "px", "rgb", "hsl"] {
        assert!(!rendered.contains(leak), "a rendering detail leaked in: {leak}");
    }
}

// ── discriminability ─────────────────────────────────────────────────────────

#[test]
fn the_three_status_classes_get_three_distinct_hues() {
    let peer = ingested("peer", 9.0, 9.0, None);
    let mut hues = Vec::new();
    for severity in [None, Some(Severity::Warning), Some(Severity::Critical)] {
        let s = ingested("s", 1.0, 1.0, severity);
        match encode_field(&[&s, &peer])[0].channel(Channel::Hue) {
            Some(VisualAttribute::Hue { value, .. }) => hues.push(*value),
            other => panic!("expected hue, got {other:?}"),
        }
    }
    assert_eq!(hues, vec![Hue::Green, Hue::Amber, Hue::Red]);
    assert_eq!(
        hues.iter().copied().collect::<BTreeSet<_>>().len(),
        3,
        "near-identical hues would be a broken encoding, not a weaker one"
    );
}

#[test]
fn distinct_state_classes_encode_distinctly() {
    // ★★ Discriminability's necessary condition, RUN over every class this
    // encoder can see, with an identical peer held constant so the peer cannot
    // be what makes them differ.
    let peer = ingested("peer", 5.0, 5.0, None);
    let mut specs = Vec::new();
    for severity in [None, Some(Severity::Warning), Some(Severity::Critical)] {
        for (w, smoothed) in [(2.0, 1.0), (1.0, 1.0), (1.0, 2.0)] {
            let subject = ingested("subject", w, smoothed, severity);
            specs.push(encode_field(&[&subject, &peer])[0].clone());
        }
    }
    for (i, a) in specs.iter().enumerate() {
        for b in specs.iter().skip(i + 1) {
            assert_ne!(a.attributes(), b.attributes(), "two state-classes encode identically");
        }
    }
}

#[test]
fn trend_reads_the_latest_observation_against_its_own_level() {
    let up = ingested("u", 5.0, 1.0, None);
    let flat = ingested("f", 1.0, 1.0, None);
    let down = ingested("d", 0.5, 1.0, None);
    let specs = encode_field(&[&up, &flat, &down]);
    assert_eq!(arrow(&specs[0]), Arrow::Rising);
    assert_eq!(arrow(&specs[1]), Arrow::Level);
    assert_eq!(arrow(&specs[2]), Arrow::Falling);
}

#[test]
fn the_dead_band_scales_with_the_series() {
    // `level` must mean level on a series near a million, not only near 1.
    let big = ingested("b", 1_000_000.0, 1_000_000.000_000_1, None);
    let peer = ingested("p", 1.0, 1.0, None);
    assert_eq!(arrow(&encode_field(&[&big, &peer])[0]), Arrow::Level);
}

#[test]
fn motion_reports_detection_not_escalation() {
    let a = ingested("a", 3.0, 1.0, Some(Severity::Critical));
    let b = ingested("b", 1.0, 1.0, None);
    let specs = encode_field(&[&a, &b]);
    assert_eq!(motion(&specs[0]), Motion::Pulse);
    assert_eq!(motion(&specs[1]), Motion::Still);
}

// ── un-gameable urgency ──────────────────────────────────────────────────────

#[test]
fn brightness_comes_from_the_real_distance_to_v() {
    let far = ingested("far", 10.0, 1.0, None);
    let near = ingested("near", 2.0, 1.0, None);
    let specs = encode_field(&[&far, &near]);
    assert_eq!(brightness(&specs[0]), 1.0);
    assert!((brightness(&specs[1]) - 0.2).abs() < 1e-12);
}

#[test]
fn a_widget_can_only_get_brighter_by_being_worse() {
    // ★★★ Both directions. There is no action a widget can take to become more
    // salient except actually being in worse shape.
    let peer = ingested("peer", 4.0, 1.0, None);
    let quiet = ingested("x", 2.0, 1.0, None);
    let base = brightness(&encode_field(&[&quiet, &peer])[0]);

    let worse = ingested("x", 8.0, 1.0, None);
    assert!(brightness(&encode_field(&[&worse, &peer])[0]) > base, "further from V is brighter");

    let worse_peer = ingested("peer", 40.0, 1.0, None);
    assert!(
        brightness(&encode_field(&[&quiet, &worse_peer])[0]) < base,
        "unchanged itself, dimmer because a peer got worse — attention is zero-sum"
    );
}

#[test]
fn a_single_item_field_carries_no_brightness_and_no_size() {
    let only = ingested("only", 3.0, 2.0, Some(Severity::Warning));
    let spec = &encode_field(&[&only])[0];
    assert!(spec.channel(Channel::Brightness).is_none());
    assert!(spec.channel(Channel::Size).is_none());
    assert!(spec.channel(Channel::Hue).is_some(), "status needs no field");
}

#[test]
fn an_all_zero_field_emits_no_brightness_rather_than_zero_over_zero() {
    let a = ingested("a", 0.0, 0.0, None);
    let b = ingested("b", 0.0, 0.0, None);
    for spec in encode_field(&[&a, &b]) {
        assert!(spec.channel(Channel::Brightness).is_none());
        assert!(spec.channel(Channel::Size).is_none());
    }
}

#[test]
fn there_is_no_way_to_construct_a_spec_by_hand() {
    // `VisualSpec`'s fields are private and it has no constructor — compile
    // time. What is assertable here is that the specs are keyed to the sustains
    // the encoder was given.
    let a = ingested("alpha", 1.0, 1.0, None);
    let b = ingested("beta", 2.0, 1.0, None);
    let specs = encode_field(&[&a, &b]);
    assert_eq!(specs[0].sustain_id(), "alpha");
    assert_eq!(specs[1].sustain_id(), "beta");
}

#[test]
fn the_encoder_consumes_a_real_monitor_engine_pass() {
    // ★ Not a hand-built reading: a real engine, a real Region, real states.
    let region = || {
        Region::new()
            .bounding(Interval::new("balance", 0.0, 100.0))
            .weighing("balance", 1.0)
    };
    let spec = CusumSpec::new(0.0, 1.0, 5.0);

    let mut far_engine =
        MonitorEngine::watching(SustainWatch::new("far", region(), 0.3, spec)).unwrap();
    let mut near_engine =
        MonitorEngine::watching(SustainWatch::new("near", region(), 0.3, spec)).unwrap();

    // 150 is 50 outside the band; 110 is 10 outside.
    let far = far_engine.ingest("far", &json!({"balance": 150.0})).unwrap();
    let near = near_engine.ingest("near", &json!({"balance": 110.0})).unwrap();

    // The engine's own `w` is d(s,V) — confirm the fixture before encoding it.
    assert!(far.reading.w > near.reading.w, "the further sustain has the larger d(s,V)");

    let specs = encode_field(&[&far, &near]);
    assert_eq!(brightness(&specs[0]), 1.0, "the furthest from V is brightest");
    assert!(brightness(&specs[1]) < 1.0);
    assert_eq!(specs[0].sustain_id(), "far");
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-ahead-of-python");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "1_no_encoder_exists_in_this_core",
        "★_2_everything_it_CONSUMES_already_exists_and_is_the_real_thing",
        "★_3_what_this_row_deliberately_does_NOT_touch",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }

    // ★★ The one-urgency counterweight must stay, including that the reference
    // established it and already reuses it.
    let cw = d["★★_the_counterweight_THE_ONE_URGENCY_ALREADY_EXISTS_THERE_AND_IS_ALREADY_REUSED"]
        .as_str()
        .unwrap();
    for term in ["Slice 0", "Slice 13", "no second notion of important", "selection"] {
        assert!(cw.contains(term), "missing counterweight detail: {term}");
    }

    // And the contrast that names exactly what this row forbids.
    let contrast =
        d["★_the_contrast_worth_stating_because_it_is_the_exact_thing_this_row_forbids"]
            .as_str()
            .unwrap();
    assert!(contrast.contains("widget-chosen"));

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.contains("visual (27 hits")));
    assert!(fp.keys().any(|k| k.contains("salience (4 hits")));
    assert!(fp.keys().any(|k| k.contains("colour_rule")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THIS IS THE ENCODER, NOT THE RENDERER",
        "THE PSYCHOPHYSICS IS NOT CHECKABLE HERE",
        "THE MAP IS FIXED, NOT CONFIGURABLE",
        "BRIGHTNESS AND SIZE ARE FIELD-RELATIVE",
        "FIVE OF SEVEN CHANNELS ARE USED",
        "NOTHING CALLS THIS FROM THE ENGINE YET",
        "TREND IS A ONE-STEP READING",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["closure_cases", "discriminability_cases", "ungameable_urgency_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 15, "every declared case must be present");
}
