//! Disaggregation with hysteresis, replayed against its conformance vectors
//! (R2 · Multiparty §IX · MUL-14).
//!
//! **SPEC vectors, Rust-only — but narrowly**, and this is the slice where that
//! label most needs qualifying. §IX's *dissolution semantics* are already in the
//! reference, deliberately and citing the article by name:
//! `holon.dissolve_child` is *"⊕⁻¹: return the child's funds, then unlink"*,
//! and `unlink_child` removes the link while the child keeps its own state.
//! **"Departure removes an edge, never a node" is implemented there.**
//!
//! What is absent is §IX's *trigger* — the hysteresis band that decides WHEN,
//! automatically. The reference's dissolution is human-initiated, so there is
//! no oscillation to damp, which is coherent right up until the decision stops
//! being a person's.
//!
//! The proof of value: eight readings across the band produce exactly two
//! transitions, and every member goes home with what it brought.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    Assembly, AssemblyState, Band, BandPosition, BandTransition, DisaggregationError, Dimension,
    Stamped, VectorClock, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("disaggregation.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "disaggregation.json was written for a different contract version"
    );
    doc
}

// ── fixture ─────────────────────────────────────────────────────────────────

fn band_of(doc: &Value) -> Band {
    Band::new(
        doc["fixture"]["form_at"].as_f64().unwrap(),
        doc["fixture"]["disperse_at"].as_f64().unwrap(),
    )
    .expect("θ↓ < θ↑")
}

fn assembly_of(doc: &Value, dim: Dimension) -> Assembly {
    let mut a = Assembly::new(band_of(doc), dim);
    for m in doc["fixture"]["members"].as_array().unwrap() {
        a = a.with_member(m.as_str().unwrap());
    }
    a
}

fn portion(node: &str, v: &str, clock: VectorClock, t: i64) -> Stamped<Value> {
    Stamped::new(json!(v), clock, t, node)
}

fn position_name(p: BandPosition) -> &'static str {
    match p {
        BandPosition::AboveForm => "AboveForm",
        BandPosition::Within => "Within",
        BandPosition::BelowDisperse => "BelowDisperse",
    }
}

fn readings(c: &Value) -> Vec<f64> {
    c["readings"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect()
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from disaggregation.json"))
}

// ── ★ the band ──────────────────────────────────────────────────────────────

#[test]
fn a_zero_or_inverted_band_is_refused() {
    let doc = load();
    let c = case(&doc, "band_cases", "a_zero_or_inverted_band_is_refused");

    for key in ["zero", "inverted"] {
        let pair = c[key].as_array().unwrap();
        let err = Band::new(pair[0].as_f64().unwrap(), pair[1].as_f64().unwrap()).unwrap_err();
        assert!(matches!(err, DisaggregationError::NoBand { .. }), "{key}: {err:?}");
        assert!(
            err.to_string().contains(c["expect"]["message_contains"].as_str().unwrap()),
            "★ the refusal explains why, not just that"
        );
    }
}

#[test]
fn the_gap_is_the_declared_design_parameter() {
    let doc = load();
    let c = case(&doc, "band_cases", "the_gap_is_the_declared_design_parameter");
    let b = band_of(&doc);

    assert!((b.gap() - c["expect"]["gap"].as_f64().unwrap()).abs() < 1e-12);
    assert!(b.gap() > 0.0, "bought with responsiveness — wider is steadier and slower");
}

#[test]
fn the_band_classifies_all_three_regions() {
    let doc = load();
    let c = case(&doc, "band_cases", "the_band_classifies_all_three_regions");
    let b = band_of(&doc);

    for (reading, want) in c["expect"].as_object().unwrap() {
        let x: f64 = reading.parse().expect("a number");
        assert_eq!(
            position_name(b.classify(x)),
            want.as_str().unwrap(),
            "★ boundary at c = {x}"
        );
    }
}

// ── ★★ hysteresis ───────────────────────────────────────────────────────────

#[test]
fn the_body_forms_at_the_upper_threshold() {
    let doc = load();
    let c = case(&doc, "hysteresis_cases", "the_body_forms_at_the_upper_threshold");
    let mut a = assembly_of(&doc, Dimension::Snapshot);

    assert_eq!(a.state(), AssemblyState::Dispersed);
    assert!(matches!(a.sense(c["at"].as_f64().unwrap()).unwrap(), BandTransition::Formed { .. }));
    assert_eq!(a.is_formed(), c["expect"]["formed"].as_bool().unwrap());
}

#[test]
fn a_formed_body_does_not_dissolve_inside_the_band() {
    let doc = load();
    let c = case(&doc, "hysteresis_cases", "a_formed_body_does_not_dissolve_inside_the_band");
    let mut a = assembly_of(&doc, Dimension::Snapshot);
    a.sense(0.8).unwrap();

    for r in readings(c) {
        let t = a.sense(r).unwrap();
        assert_eq!(t.changed(), c["expect"]["changed"].as_bool().unwrap(), "★ held at c = {r}");
        assert!(matches!(
            t,
            BandTransition::Held { position: BandPosition::Within, state: AssemblyState::Formed }
        ));
        assert!(a.is_formed(), "★★ a single threshold would have dissolved it here");
    }
}

#[test]
fn a_dispersed_body_does_not_form_inside_the_band() {
    let doc = load();
    let c = case(&doc, "hysteresis_cases", "a_dispersed_body_does_not_form_inside_the_band");
    let mut a = assembly_of(&doc, Dimension::Snapshot);

    for r in readings(c) {
        let t = a.sense(r).unwrap();
        assert_eq!(t.changed(), c["expect"]["changed"].as_bool().unwrap());
        assert!(matches!(
            t,
            BandTransition::Held { position: BandPosition::Within, state: AssemblyState::Dispersed }
        ));
    }
    assert!(
        !a.is_formed(),
        "★★ the same readings that HOLD a formed body fail to assemble a dispersed one"
    );
}

#[test]
fn the_full_loop_does_not_flicker() {
    let doc = load();
    let c = case(&doc, "hysteresis_cases", "the_full_loop_does_not_flicker");
    let mut a = assembly_of(&doc, Dimension::Snapshot);

    let rs = readings(c);
    let changes = rs.iter().filter(|r| a.sense(**r).unwrap().changed()).count();

    assert_eq!(
        changes,
        c["expect"]["transitions"].as_u64().unwrap() as usize,
        "★★ {} readings, exactly {} transitions",
        rs.len(),
        c["expect"]["transitions"]
    );
    assert_eq!(a.is_formed(), c["expect"]["ends_formed"].as_bool().unwrap());

    // And the comparison the case claims: a single threshold in the middle of
    // the band would have flipped far more often.
    let single = 0.5;
    let mut state = false;
    let naive = rs
        .iter()
        .filter(|r| {
            let want = **r >= single;
            let flipped = want != state;
            state = want;
            flipped
        })
        .count();
    assert!(naive > changes, "★★ one threshold: {naive} transitions; a band: {changes}");
}

#[test]
fn the_body_dissolves_only_below_the_release_value() {
    let doc = load();
    let c = case(&doc, "hysteresis_cases", "the_body_dissolves_only_below_the_release_value");
    let mut a = assembly_of(&doc, Dimension::Snapshot);

    a.sense(c["form_at"].as_f64().unwrap()).unwrap();
    let t = a.sense(c["release_at"].as_f64().unwrap()).unwrap();

    assert_eq!(t.changed(), c["expect"]["changed"].as_bool().unwrap());
    let d = t.dispersal().expect("dispersed");
    assert!((d.released_at - c["expect"]["released_at"].as_f64().unwrap()).abs() < 1e-12);
    assert!(!a.is_formed());
}

#[test]
fn a_non_finite_pressure_is_refused() {
    let doc = load();
    let _ = case(&doc, "hysteresis_cases", "a_non_finite_pressure_is_refused");
    let mut a = assembly_of(&doc, Dimension::Snapshot);
    assert!(
        matches!(a.sense(f64::NAN), Err(DisaggregationError::BadPressure(_))),
        "★ NaN classifies into the band by accident — every comparison against it is false"
    );
}

// ── ★★ departure removes an edge, never a node ──────────────────────────────

#[test]
fn every_member_carries_its_portion_home() {
    let doc = load();
    let c = case(&doc, "never_erase_cases", "every_member_carries_its_portion_home");
    let mut a = assembly_of(&doc, Dimension::Snapshot);

    let root = VectorClock::new().tick("shared");
    for (node, v) in c["portions"].as_object().unwrap() {
        a.contribute(node, portion(node, v.as_str().unwrap(), root.tick(node), 100)).unwrap();
    }
    a.sense(0.9).unwrap();

    let t = a.sense(0.05).unwrap();
    let d = t.dispersal().unwrap();
    let e = &c["expect"];

    assert_eq!(
        d.members_removed,
        e["members_removed"].as_u64().unwrap() as usize,
        "★★ departure removes an edge, NEVER a node"
    );
    assert_eq!(
        d.edges_removed > 0,
        e["edges_removed_positive"].as_bool().unwrap(),
        "the composition genuinely came apart"
    );
    assert_eq!(
        d.everyone_carried_home(a.members()),
        e["everyone_home"].as_bool().unwrap(),
        "★★ every member got its portion back"
    );
    for (node, v) in c["portions"].as_object().unwrap() {
        assert_eq!(d.carried_home[node].value, json!(v.as_str().unwrap()), "{node}: untouched");
    }
}

#[test]
fn a_non_member_cannot_contribute() {
    let doc = load();
    let _ = case(&doc, "never_erase_cases", "a_non_member_cannot_contribute");
    let mut a = assembly_of(&doc, Dimension::Snapshot);
    let v = VectorClock::new().tick("x");
    assert!(matches!(
        a.contribute("outsider", portion("outsider", "v", v, 1)),
        Err(DisaggregationError::NotAMember(_))
    ));
}

#[test]
fn dispersing_an_empty_assembly_settles_to_nothing() {
    let doc = load();
    let c = case(&doc, "never_erase_cases", "dispersing_an_empty_assembly_settles_to_nothing");
    let mut a = assembly_of(&doc, Dimension::Snapshot);
    a.sense(0.9).unwrap();

    let t = a.sense(0.0).unwrap();
    let d = t.dispersal().unwrap();
    assert_eq!(d.settled.len(), c["expect"]["settled"].as_u64().unwrap() as usize);
    assert_eq!(
        d.members_removed,
        c["expect"]["members_removed"].as_u64().unwrap() as usize,
        "the property does not depend on there being anything to protect"
    );
}

// ── the settle ──────────────────────────────────────────────────────────────

fn settle_with(doc: &Value, dim: Dimension, portions: &Value) -> sustena_core::Dispersal {
    let mut a = assembly_of(doc, dim);
    let root = VectorClock::new().tick("shared");
    let mut t = 100;
    for (node, v) in portions.as_object().unwrap() {
        a.contribute(node, portion(node, v.as_str().unwrap(), root.tick(node), t)).unwrap();
        t += 100;
    }
    a.sense(0.9).unwrap();
    a.sense(0.1).unwrap().dispersal().unwrap().clone()
}

#[test]
fn a_snapshot_dimension_settles_to_one_final_value() {
    let doc = load();
    let c = case(&doc, "settle_cases", "a_snapshot_dimension_settles_to_one_final_value");
    let d = settle_with(&doc, Dimension::Snapshot, &c["portions"]);
    let e = &c["expect"];

    assert_eq!(d.settled.len(), e["settled"].as_u64().unwrap() as usize, "a final merged value");
    assert_eq!(d.settled[0].value, json!(e["value"].as_str().unwrap()));
    assert_eq!(
        d.carried_home.len(),
        e["carried_home"].as_u64().unwrap() as usize,
        "the settle and the hand-back are separate"
    );
}

#[test]
fn a_contested_dimension_keeps_what_collapsing_would_discard() {
    let doc = load();
    let c = case(&doc, "settle_cases", "a_contested_dimension_keeps_what_collapsing_would_discard");
    let d = settle_with(&doc, Dimension::Contested, &c["portions"]);

    assert_eq!(
        d.settled.len(),
        c["expect"]["settled"].as_u64().unwrap() as usize,
        "★ genuinely concurrent — neither is discarded"
    );
    let vs: Vec<&Value> = d.settled.iter().map(|s| &s.value).collect();
    for v in c["portions"].as_object().unwrap().values() {
        assert!(vs.contains(&&json!(v.as_str().unwrap())));
    }
}

#[test]
fn a_causally_ordered_pair_settles_to_the_later_one_under_either_dimension() {
    let doc = load();
    let c = case(&doc, "settle_cases", "a_causally_ordered_pair_settles_to_the_later_one_under_either_dimension");

    for dim in [Dimension::Snapshot, Dimension::Contested] {
        let mut a = Assembly::new(band_of(&doc), dim).with_member("a").with_member("b");
        let first = VectorClock::new().tick("a");
        let second = first.tick("b");
        a.contribute("a", portion("a", "draft", first, 100)).unwrap();
        a.contribute("b", portion("b", "revised", second, 200)).unwrap();
        a.sense(0.9).unwrap();

        let d = a.sense(0.1).unwrap().dispersal().unwrap().clone();
        assert_eq!(
            d.settled.len(),
            c["expect"]["settled"].as_u64().unwrap() as usize,
            "★ {dim:?}: a supersession is not a conflict"
        );
        assert_eq!(d.settled[0].value, json!(c["expect"]["value"].as_str().unwrap()));
    }
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // ★ The qualifier is the point of this block.
    let q = d["★_but_narrowly_and_the_counterweight_is_unusually_strong"].as_str().unwrap();
    assert!(q.contains("already implemented in the reference"));
    assert!(
        q.contains("one term missing from a row that is otherwise built, not as a module"),
        "the gap must not be overstated"
    );

    // TWO terms are AT PARITY, and the reference cites the article itself.
    let tt = &d["term_by_term"];
    assert!(tt["departure removes an edge, never a node"].as_str().unwrap().contains("AT PARITY"));
    assert!(tt["each node carries its portion home"].as_str().unwrap().contains("AT PARITY"));
    assert!(
        d["statement"].as_str().unwrap().contains("§IX of the Multiparty article"),
        "the reference cites §IX by name in its own comments"
    );

    // The hysteresis gap is explained, not just noted.
    assert!(
        tt["hysteresis / the two-threshold band"]
            .as_str()
            .unwrap()
            .contains("no oscillation to damp"),
        "with a human-initiated dissolve the band would have nothing to do"
    );

    // The false positives, including the domain-name one.
    let fp = &d["the_two_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["band (2 hits)"].as_str().unwrap().contains("vyybandasky"));
    assert!(fp["debounce (1 hit)"].as_str().unwrap().contains("wildly different stakes"));

    // The counterweight calls the reference's design coherent, not deficient.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("holon.dissolve_child"));
    assert!(cw.contains("a coherent design, not an omission"));

    // The singular-noun reading is disclosed as an interpretation.
    assert!(
        d["the_honest_reading_of_the_settle"]
            .as_str()
            .unwrap()
            .contains("not as a correction to the article")
    );

    for group in ["band_cases", "hysteresis_cases", "never_erase_cases", "settle_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in ["band_cases", "hysteresis_cases", "never_erase_cases", "settle_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
