//! Suited domains, coverage and disorder, replayed against their conformance
//! vectors (R2 · Operative §IX · OPV-16).
//!
//! **SPEC vectors, Rust-only — with the abstention SEAM at parity**, which is
//! the load-bearing half. §IX says so itself: *"`is_relevant()` already
//! produces an ABSTAIN path, and domain mismatch is a **second** reason to take
//! it."* The reference writes `abstain_reason: "no_domain_overlap"` and means
//! *"this is not my topic"*; what it cannot mean is *"my method presupposes
//! what this regime denies"*.
//!
//! ★ This layer routes **on** a domain reading and does not produce one. §IX
//! assigns `dom(s)` to the Monitor and gives no classification algorithm, so
//! the classifier is CAP-13 — a correctly separate layer, not an agent-layer
//! gap.
//!
//! See `conformance/README.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    assess, coverage_gaps, domain_coverage, route_on_domain, transition_risk, Contender, Cynefin,
    Disorder, DomainReading, EstimateRule, GateWeights, Mixture, Objective, Operative, Regime,
    ResponseMode, Sense, Utility, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("cynefin.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "cynefin.json was written for a different contract version"
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

fn fixture(doc: &Value) -> BTreeMap<String, Operative> {
    doc["fixture"]["operatives"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, doms)| {
            let suited: Vec<Cynefin> = doms
                .as_array()
                .unwrap()
                .iter()
                .map(|d| cynefin(d.as_str().unwrap()))
                .collect();
            (id.clone(), op(id, &suited))
        })
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

// ── the response modes ──────────────────────────────────────────────────────

#[test]
fn every_domain_has_a_distinct_response_mode_and_a_presupposition() {
    let doc = load();
    let c = case(
        &doc,
        "response_mode_cases",
        "every_domain_has_a_distinct_response_mode_and_a_presupposition",
    );
    let e = &c["expect"];
    let modes: BTreeSet<&str> = Cynefin::ALL
        .into_iter()
        .map(|d| d.response_mode().name())
        .collect();
    assert_eq!(modes.len(), e["distinct_modes"].as_u64().unwrap() as usize);
    assert_eq!(
        Cynefin::Complicated.response_mode().name(),
        e["complicated"].as_str().unwrap()
    );
    assert_eq!(
        Cynefin::Chaotic.response_mode().name(),
        e["chaotic"].as_str().unwrap()
    );
    assert!(Cynefin::Complicated
        .response_mode()
        .presupposes()
        .contains(e["complicated_presupposes"].as_str().unwrap()));
    // Every mode says what it takes for granted — that is what a regime denies.
    for d in Cynefin::ALL {
        assert!(!d.response_mode().presupposes().is_empty());
        assert!(!d.response_mode().built_as().is_empty());
    }
}

// ── ★★ match(i,s) and its reason ────────────────────────────────────────────

/// ★★ THE PROOF. The reason is derived from declared data.
#[test]
fn a_mismatch_names_the_presupposition_that_fails() {
    let doc = load();
    let c = case(
        &doc,
        "match_cases",
        "★★_a_mismatch_names_the_presupposition_that_fails",
    );
    let e = &c["expect"];
    let ops = fixture(&doc);
    let who = &ops[c["operative"].as_str().unwrap()];
    let regime = cynefin(c["regime"].as_str().unwrap());

    let s = assess(who, regime);
    assert_eq!(s.is_suited(), e["suited"].as_bool().unwrap());
    let m = s.mismatch().expect("mismatched");
    assert_eq!(m.regime, regime);
    assert_eq!(m.operative, "mentor");
    for term in e["why_contains"].as_array().unwrap() {
        assert!(
            m.why.contains(term.as_str().unwrap()),
            "reason should name {term}: {}",
            m.why
        );
    }
    // Derived, not canned: it names the operative's OWN declaration.
    assert!(m.why.contains("complicated"));
}

#[test]
fn a_suited_operative_is_simply_suited() {
    let doc = load();
    let c = case(&doc, "match_cases", "a_suited_operative_is_simply_suited");
    let e = &c["expect"];
    let ops = fixture(&doc);
    let curator = &ops["curator"];
    assert_eq!(
        assess(curator, Cynefin::Complex).is_suited(),
        e["curator_in_complex"].as_bool().unwrap()
    );
    assert_eq!(
        assess(curator, Cynefin::Clear).is_suited(),
        e["curator_in_clear"].as_bool().unwrap()
    );
    assert_eq!(
        assess(curator, Cynefin::Complicated).is_suited(),
        e["curator_in_complicated"].as_bool().unwrap()
    );
}

// ── ★ coverage ──────────────────────────────────────────────────────────────

#[test]
fn coverage_measures_requisite_variety_and_enumerates_the_claimants() {
    let doc = load();
    let c = case(
        &doc,
        "coverage_cases",
        "★_coverage_measures_requisite_variety_and_ENUMERATES_the_claimants",
    );
    let e = &c["expect"];
    let ops = fixture(&doc);
    let refs: Vec<&Operative> = ops.values().collect();
    let cov = domain_coverage(&refs);

    assert_eq!(
        cov.has_requisite_variety(),
        e["has_requisite_variety"].as_bool().unwrap()
    );
    let uncovered: Vec<String> = cov.uncovered().iter().map(|d| d.name().to_string()).collect();
    assert_eq!(uncovered, strings(&e["uncovered"]));
    // Matches the fixture's own declared expectation.
    assert_eq!(uncovered, strings(&doc["fixture"]["uncovered"]));

    // ★ Enumerable: who claims what.
    assert_eq!(cov.claimants(Cynefin::Clear), strings(&e["clear_claimants"]).as_slice());
    assert!(cov.claimants(Cynefin::Chaotic).is_empty());
    assert!(cov.describe().contains(e["describe_contains"].as_str().unwrap()));
}

#[test]
fn adding_the_missing_operative_completes_requisite_variety() {
    let doc = load();
    let c = case(
        &doc,
        "coverage_cases",
        "adding_the_missing_operative_completes_requisite_variety",
    );
    let ops = fixture(&doc);
    let firefighter = op("navigator", &[Cynefin::Chaotic]);
    let mut refs: Vec<&Operative> = ops.values().collect();
    refs.push(&firefighter);
    let cov = domain_coverage(&refs);
    assert_eq!(
        cov.has_requisite_variety(),
        c["expect"]["has_requisite_variety"].as_bool().unwrap()
    );
    assert!(cov
        .describe()
        .contains(c["expect"]["describe_contains"].as_str().unwrap()));
}

#[test]
fn an_operative_declaring_nothing_is_never_suited() {
    let doc = load();
    let c = case(&doc, "coverage_cases", "an_operative_declaring_nothing_is_never_suited");
    let dead = op("dead", &[]);
    let live = op("live", &[Cynefin::Clear]);
    let refs = [&dead, &live];
    let cov = domain_coverage(&refs);
    assert_eq!(cov.never_suited(&refs), strings(&c["expect"]["never_suited"]));
}

// ── ★★ disorder ─────────────────────────────────────────────────────────────

/// ★★ Not forbidden — unsayable.
#[test]
fn disorder_is_undeclarable_by_construction() {
    let doc = load();
    let c = case(
        &doc,
        "disorder_cases",
        "★★_disorder_is_UNDECLARABLE_by_construction",
    );
    let e = &c["expect"];
    assert_eq!(Cynefin::ALL.len(), e["variants"].as_u64().unwrap() as usize);
    let names: BTreeSet<&str> = Cynefin::ALL.into_iter().map(|d| d.name()).collect();
    assert_eq!(!names.contains("disorder"), e["no_disorder_variant"].as_bool().unwrap());
}

/// ★ Surface the gap; route nobody.
#[test]
fn an_uncovered_regime_surfaces_a_gap_and_routes_nobody() {
    let doc = load();
    let c = case(
        &doc,
        "disorder_cases",
        "★_an_uncovered_regime_surfaces_a_gap_and_routes_NOBODY",
    );
    let e = &c["expect"];
    let ops = fixture(&doc);
    let refs: Vec<&Operative> = ops.values().collect();
    let r = route_on_domain(&refs, &DomainReading::known(cynefin(c["regime"].as_str().unwrap())));

    assert_eq!(r.surfaces_a_gap(), e["surfaces_a_gap"].as_bool().unwrap());
    assert!(r.suited().is_empty());
    // ★ The mismatches are KEPT — why nobody was suited is the gap's content.
    assert_eq!(r.mismatched().len(), e["mismatched"].as_u64().unwrap() as usize);
    match r.disorder().expect("disorder") {
        Disorder::UncoveredRegime { regime, requires } => {
            assert_eq!(*regime, Cynefin::Chaotic);
            assert_eq!(requires.name(), e["requires"].as_str().unwrap());
            assert_eq!(*requires, ResponseMode::ActSenseRespond);
        }
        other => panic!("expected UncoveredRegime, got {other:?}"),
    }
    assert!(r.describe().contains(e["describe_contains"].as_str().unwrap()));
}

/// ★ The second way disorder arrives.
#[test]
fn an_indeterminate_reading_is_disorder_too() {
    let doc = load();
    let c = case(&doc, "disorder_cases", "★_an_INDETERMINATE_reading_is_disorder_too");
    let e = &c["expect"];
    let ops = fixture(&doc);
    let refs: Vec<&Operative> = ops.values().collect();
    let r = route_on_domain(
        &refs,
        &DomainReading::indeterminate("the Monitor has too little history to say"),
    );
    assert_eq!(r.surfaces_a_gap(), e["surfaces_a_gap"].as_bool().unwrap());
    assert!(r.suited().is_empty());
    assert!(matches!(
        r.disorder(),
        Some(Disorder::IndeterminateReading { .. })
    ));
    assert!(r.describe().contains(e["describe_contains"].as_str().unwrap()));
}

#[test]
fn a_covered_regime_routes_the_suited_and_abstains_the_rest() {
    let doc = load();
    let c = case(
        &doc,
        "disorder_cases",
        "a_covered_regime_routes_the_suited_and_abstains_the_rest",
    );
    let e = &c["expect"];
    let ops = fixture(&doc);
    let refs: Vec<&Operative> = ops.values().collect();
    let r = route_on_domain(&refs, &DomainReading::known(cynefin(c["regime"].as_str().unwrap())));
    assert_eq!(r.surfaces_a_gap(), e["surfaces_a_gap"].as_bool().unwrap());
    assert_eq!(r.suited(), strings(&e["suited"]).as_slice());
    let mismatched: Vec<String> = r.mismatched().iter().map(|m| m.operative.clone()).collect();
    assert_eq!(mismatched, strings(&e["mismatched"]));
}

// ── ★ the one licensed inference ────────────────────────────────────────────

#[test]
fn criticality_flags_complex_to_chaotic_and_nothing_else() {
    let doc = load();
    let c = case(
        &doc,
        "criticality_wiring_cases",
        "★_criticality_flags_complex_to_chaotic_and_NOTHING_else",
    );
    let e = &c["expect"];
    let complex = DomainReading::known(Cynefin::Complex);

    let risk = transition_risk(&complex, Regime::Supercritical).expect("flagged");
    let pair = strings(&e["complex_supercritical"]);
    assert_eq!(risk.from.name(), pair[0]);
    assert_eq!(risk.to.name(), pair[1]);

    // Calm complex: nothing.
    assert!(transition_risk(&complex, Regime::Subcritical).is_none());
    assert!(e["complex_subcritical"].is_null());
    // ★ σ̂ says nothing about the other domains, so nothing is inferred.
    for d in [Cynefin::Clear, Cynefin::Complicated, Cynefin::Chaotic] {
        assert!(transition_risk(&DomainReading::known(d), Regime::Supercritical).is_none());
    }
    assert!(e["other_domains"].is_null());
    assert!(transition_risk(&DomainReading::indeterminate("x"), Regime::Supercritical).is_none());
}

/// ★★ Wiring an input, not inventing a classifier.
#[test]
fn a_flag_does_not_reclassify_the_reading() {
    let doc = load();
    let c = case(
        &doc,
        "criticality_wiring_cases",
        "★★_a_flag_does_not_reclassify_the_reading",
    );
    let e = &c["expect"];
    let reading = DomainReading::known(Cynefin::Complex);
    let risk = transition_risk(&reading, Regime::Critical).expect("flagged");
    // The reading is untouched — no path here produces a DomainReading.
    assert_eq!(
        reading.domain().unwrap().name(),
        e["reading_unchanged"].as_str().unwrap()
    );
    assert!(risk.describe().contains(e["describe_contains"].as_str().unwrap()));
}

// ── ★★ the composition ──────────────────────────────────────────────────────

/// ★★ The layering that makes §XV's route duty complete: `cynefin` answers
/// *whether*, `mixture` answers *whom*.
#[test]
fn cynefin_decides_whether_to_route_and_mixture_decides_whom() {
    let doc = load();
    let c = case(
        &doc,
        "composition_cases",
        "★★_cynefin_decides_WHETHER_to_route_and_mixture_decides_WHOM",
    );
    let e = &c["expect"];
    let ops = fixture(&doc);
    let refs: Vec<&Operative> = ops.values().collect();
    let gate = Mixture::new(
        GateWeights::new(1.0, 1.0, 0.5).unwrap(),
        2,
        EstimateRule::NetGain,
    )
    .unwrap();
    let contenders: Vec<Contender<'_>> = refs
        .iter()
        .map(|o| Contender::new(o, &["food"], &[1.0]))
        .collect();
    let tags: BTreeSet<String> = ["food".to_string()].into_iter().collect();

    // ★ Indeterminate: cynefin stops it before the gate is ever consulted.
    let unknown = DomainReading::indeterminate("no reading yet");
    let pre = route_on_domain(&refs, &unknown);
    assert!(pre.surfaces_a_gap());
    let reached_gate = unknown.domain().is_some();
    assert_eq!(
        !reached_gate,
        e["indeterminate_stops_before_the_gate"].as_bool().unwrap()
    );

    // ★ Known and covered: the gate runs and asks somebody.
    let known = DomainReading::known(Cynefin::Clear);
    let ok = route_on_domain(&refs, &known);
    assert!(!ok.surfaces_a_gap());
    let dispatch = gate
        .route(&contenders, &tags, known.domain().expect("known"))
        .unwrap();
    assert_eq!(
        !dispatch.tickets().is_empty(),
        e["known_regime_reaches_the_gate"].as_bool().unwrap()
    );
    // And the gate's own abstention agrees with cynefin's assessment.
    assert_eq!(dispatch.tickets().len(), ok.suited().len());
}

/// ★ One source of truth.
#[test]
fn mixture_delegates_coverage_rather_than_keeping_a_copy() {
    let doc = load();
    let c = case(
        &doc,
        "composition_cases",
        "★_mixture_delegates_coverage_rather_than_keeping_a_copy",
    );
    let ops = fixture(&doc);
    let refs: Vec<&Operative> = ops.values().collect();
    let contenders: Vec<Contender<'_>> = refs
        .iter()
        .map(|o| Contender::new(o, &[], &[0.0]))
        .collect();
    let via_gate = coverage_gaps(&contenders);
    let via_canonical = domain_coverage(&refs).uncovered().clone();
    assert_eq!(
        via_gate == via_canonical,
        c["expect"]["agree"].as_bool().unwrap()
    );
    // They agree by construction, not by anyone remembering both.
    assert_eq!(via_gate, [Cynefin::Chaotic].into_iter().collect());
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ The counterweight: the abstain SEAM is at parity and load-bearing.
    let seam = d["★_the_counterweight_the_abstain_SEAM_is_at_parity_and_it_is_the_load_bearing_half"]
        .as_str()
        .unwrap();
    assert!(seam.contains("no_domain_overlap"));
    assert!(seam.contains("already built and already correct"));
    assert!(seam.contains("The seam is at parity"));

    // ★ dom(s) is a separate LAYER, not an agent-layer gap.
    let limits = d["the_honest_limits"].as_str().unwrap();
    assert!(limits.contains("SUPPLIED, NOT COMPUTED"));
    assert!(limits.contains("CAP-13"));
    assert!(limits.contains("SEPARATE LAYER, not an agent-layer gap"));
    assert!(limits.contains("Π-SHAPE MISMATCH CHECK IS OPV-4'S"));
    assert!(limits.contains("EXACTLY ONE TRANSITION"));

    // ★ The reconciliation is recorded, including the rename and why.
    let one = d["★_one_source_of_truth_reconciled"].as_str().unwrap();
    assert!(one.contains("duplicate was REMOVED"));
    assert!(one.contains("goodhart::coverage"));
    assert!(one.contains("newcomer takes the longer name"));

    // The collision this row recorded in advance, and that it held.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    let dom = fp["domain — ★ still an ACTIVE collision, and this row is the one that recorded it"]
        .as_str()
        .unwrap();
    assert!(dom.contains("the flag held"));
    assert!(fp["probe (1 hit, 0 real)"].as_str().unwrap().contains("Liveness probe"));

    // The half that needs Π is named rather than approximated.
    let enumerable = d["term_by_term"]["the enumerable-mismatch property"].as_str().unwrap();
    assert!(enumerable.contains("OPV-4"));
    assert!(enumerable.contains("would be guessing"));

    let groups = [
        "response_mode_cases",
        "match_cases",
        "coverage_cases",
        "disorder_cases",
        "criticality_wiring_cases",
        "composition_cases",
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
