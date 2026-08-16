//! Holarchic escalation replayed against its conformance vectors
//! (R2 · Controller §V · CTL-5).
//!
//! **SPEC vectors, Rust-only** — but this is the slice where saying only that
//! would overstate the gap most. `holarchy.json` keeps **three directions**
//! apart: the reference has **downward authority** (a `binding` aggregate
//! invariant really does refuse a child's transition) and **upward
//! information** (`compute_rollup`). What it does not have is **upward
//! escalation** — a violation climbing until it finds a level that can *act*.
//!
//! `holon` returns 83 hits and they are not false positives: the holarchy §V
//! describes genuinely exists in Python. What was never built is the walk up
//! it carrying a breach.
//!
//! The proof of value is §V's own sentence made executable: a soil sensor's
//! moisture breach escalates to the garden, then to the homestead.
//!
//! See `conformance/README.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    escalate, validate_holarchy, Breach, Candidate, Capacity, ControlEvent, CusumSpec,
    EscalationOutcome, HolarchyError, Interval, Levels, MonitorEngine, Region, SustainWatch,
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("holarchy.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "holarchy.json was written for a different contract version"
    );
    doc
}

// ── fixture, built from the vector file ─────────────────────────────────────

fn watch(id: &str, floor: f64) -> SustainWatch {
    SustainWatch::new(
        id,
        Region::new().bounding(Interval::at_least("moisture", floor)).weighing("moisture", 1.0),
        0.5,
        CusumSpec::new(0.0, 5.0, 10.0),
    )
}

fn engine_of(doc: &Value) -> MonitorEngine {
    let decls: Vec<SustainWatch> = doc["fixture"]["holarchy"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let w = watch(n["id"].as_str().unwrap(), n["moisture_floor"].as_f64().unwrap());
            match n["parent"].as_str() {
                Some(p) => w.under(p),
                None => w,
            }
        })
        .collect();
    MonitorEngine::flatten_holarchy(decls).expect("a well-formed holarchy")
}

fn moisture(v: f64) -> Value {
    json!({ "moisture": v })
}

struct Table(BTreeMap<String, Capacity>);

impl Table {
    fn from_fixture(doc: &Value) -> Self {
        // The fixture's capacity map: a sentence means "cannot, and here is
        // why"; an operator name means "can act".
        let mut m = BTreeMap::new();
        for (id, v) in doc["fixture"]["capacities"].as_object().unwrap() {
            let s = v.as_str().unwrap();
            let cap = if s.contains('.') {
                Capacity::CanAct(vec![Candidate::new(ControlEvent::new("IRRIGATE", s), "wet")])
            } else {
                Capacity::cannot(s)
            };
            m.insert(id.clone(), cap);
        }
        Self(m)
    }

    fn only(entries: &[(&str, Capacity)]) -> Self {
        Self(entries.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }

    fn none() -> Self {
        Self(BTreeMap::new())
    }
}

impl Levels for Table {
    fn capacity(&mut self, sustain_id: &str, _breach: &Breach) -> Capacity {
        self.0
            .get(sustain_id)
            .cloned()
            .unwrap_or_else(|| Capacity::cannot("no declared capacity at this level"))
    }
}

fn breach_at(engine: &mut MonitorEngine, id: &str, v: f64) -> Breach {
    let ingested = engine.ingest(id, &moisture(v)).expect("watched");
    Breach::from_ingested(&ingested).expect("this reading escalates")
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from holarchy.json"))
}

// ── the path IS the holarchy ────────────────────────────────────────────────

#[test]
fn the_parent_chain_is_the_escalation_path() {
    let doc = load();
    let c = case(&doc, "path_cases", "the_parent_chain_is_the_escalation_path");
    let e = engine_of(&doc);

    for (id, want) in c["expect"].as_object().unwrap() {
        assert_eq!(
            e.parent_of(id),
            want.as_str(),
            "★ no escalation graph — the path is the links the engine already validated"
        );
    }
}

#[test]
fn a_nano_sustain_is_the_base_case() {
    let doc = load();
    let c = case(&doc, "path_cases", "a_nano_sustain_is_the_base_case");
    let e = engine_of(&doc);
    for (id, want) in c["expect"].as_object().unwrap() {
        assert_eq!(e.is_nano(id), want.as_bool().unwrap(), "{id}");
    }
}

#[test]
fn an_escalation_cannot_be_started_from_a_non_breach() {
    let doc = load();
    let c = case(&doc, "path_cases", "an_escalation_cannot_be_started_from_a_non_breach");
    let mut e = engine_of(&doc);

    let quiet = e.ingest("soil_sensor", &moisture(c["moisture"].as_f64().unwrap())).unwrap();
    assert!(
        Breach::from_ingested(&quiet).is_none(),
        "★ no manufactured escalations — an escalation has to come from something that happened"
    );
}

#[test]
fn escalating_from_an_unwatched_sustain_is_refused() {
    let doc = load();
    let _ = case(&doc, "path_cases", "escalating_from_an_unwatched_sustain_is_refused");
    let mut e = engine_of(&doc);

    // A real breach, then re-pointed at a sustain the engine does not watch.
    let mut real = breach_at(&mut e, "soil_sensor", 0.0);
    real.origin = "elsewhere".into();

    assert!(matches!(
        escalate(&e, &mut Table::none(), real),
        Err(HolarchyError::UnknownSustain(_))
    ));
}

// ── ★ the climb ─────────────────────────────────────────────────────────────

#[test]
fn a_nano_sustain_breach_climbs_to_the_level_that_can_act() {
    let doc = load();
    let c = case(&doc, "climb_cases", "a_nano_sustain_breach_climbs_to_the_level_that_can_act");
    let mut e = engine_of(&doc);

    let breach = breach_at(&mut e, c["origin"].as_str().unwrap(), c["moisture"].as_f64().unwrap());
    let esc = escalate(&e, &mut Table::from_fixture(&doc), breach).unwrap();
    let ex = &c["expect"];

    assert_eq!(esc.acted_at(), ex["acted_at"].as_str());
    assert_eq!(esc.levels_climbed(), ex["levels_climbed"].as_u64().unwrap() as usize);

    let route: Vec<&str> = esc.path.iter().map(|h| h.sustain_id.as_str()).collect();
    let want: Vec<&str> =
        ex["route"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(route, want, "★★ the sensor cannot water itself; the homestead can authorise it");

    // Depths are the climb, not an index into anything.
    for (i, hop) in esc.path.iter().enumerate() {
        assert_eq!(hop.depth, i);
    }
}

#[test]
fn every_level_that_passed_it_on_said_why() {
    let doc = load();
    let c = case(&doc, "climb_cases", "every_level_that_passed_it_on_said_why");
    let mut e = engine_of(&doc);

    let breach = breach_at(&mut e, "soil_sensor", 0.0);
    let esc = escalate(&e, &mut Table::from_fixture(&doc), breach).unwrap();

    let declined: Vec<&str> = esc.path.iter().filter_map(|h| h.declined.as_deref()).collect();
    assert_eq!(
        declined.len(),
        c["expect"]["declined_count"].as_u64().unwrap() as usize,
        "★ a level cannot absorb a breach silently"
    );
    // Each reason is the one the fixture declared, not a generic placeholder.
    let caps = &doc["fixture"]["capacities"];
    assert_eq!(declined[0], caps["soil_sensor"].as_str().unwrap());
    assert_eq!(declined[1], caps["garden"].as_str().unwrap());
    assert!(esc.path.last().unwrap().declined.is_none(), "the acting level declined nothing");
}

#[test]
fn a_level_that_can_act_stops_the_climb_immediately() {
    let doc = load();
    let c = case(&doc, "climb_cases", "a_level_that_can_act_stops_the_climb_immediately");
    let mut e = engine_of(&doc);

    let breach = breach_at(&mut e, "soil_sensor", 0.0);
    let capable = c["capable"].as_str().unwrap();
    let mut levels = Table::only(&[(
        capable,
        Capacity::CanAct(vec![Candidate::new(
            ControlEvent::new("IRRIGATE", "iot.open_valve"),
            "wet",
        )]),
    )]);

    let esc = escalate(&e, &mut levels, breach).unwrap();
    let ex = &c["expect"];
    assert_eq!(esc.acted_at(), ex["acted_at"].as_str());
    assert_eq!(esc.levels_climbed(), ex["levels_climbed"].as_u64().unwrap() as usize);
    assert_eq!(
        esc.path.len(),
        ex["path_len"].as_u64().unwrap() as usize,
        "escalation is not broadcast — nothing above was disturbed"
    );
}

#[test]
fn reaching_the_root_unresolved_is_an_answer_not_an_error() {
    let doc = load();
    let c = case(&doc, "climb_cases", "reaching_the_root_unresolved_is_an_answer_not_an_error");
    let mut e = engine_of(&doc);

    let breach = breach_at(&mut e, "soil_sensor", 0.0);
    let esc =
        escalate(&e, &mut Table::none(), breach).expect("★ an outcome, not a failure to `?` away");
    let ex = &c["expect"];

    match &esc.outcome {
        EscalationOutcome::ExhaustedAtRoot { root } => {
            assert_eq!(root, ex["root"].as_str().unwrap())
        }
        other => panic!("expected ExhaustedAtRoot, got {other:?}"),
    }
    assert_eq!(
        esc.path.len(),
        ex["path_len"].as_u64().unwrap() as usize,
        "and the whole path is still reported"
    );
    assert!(esc.describe().contains(ex["describe_contains"].as_str().unwrap()));
    // Every level on the way stated a reason, including the root.
    assert!(esc.path.iter().all(|h| h.declined.is_some()));
}

#[test]
fn the_observation_carries_the_origins_severity_not_the_acting_levels() {
    let doc = load();
    let c = case(&doc, "climb_cases", "the_observation_carries_the_origins_severity_not_the_acting_levels");
    let mut e = engine_of(&doc);

    let breach = breach_at(&mut e, "soil_sensor", 0.0);
    let origin_severity = breach.severity;
    let esc = escalate(&e, &mut Table::from_fixture(&doc), breach).unwrap();

    assert_eq!(
        esc.observation().severity() == origin_severity,
        c["expect"]["severity_matches_origin"].as_bool().unwrap(),
        "★ a homestead decision about a soil sensor must be about the sensor"
    );
    assert_eq!(
        esc.observation().escalates(),
        c["expect"]["still_escalates"].as_bool().unwrap()
    );
    assert_eq!(esc.breach.origin, "soil_sensor", "and the provenance survives the climb");
}

// ── §V's invariant ──────────────────────────────────────────────────────────

#[test]
fn the_invariant_holds_when_every_level_is_inside_its_own_region() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "the_invariant_holds_when_every_level_is_inside_its_own_region");
    let e = engine_of(&doc);
    let m = moisture(c["moisture"].as_f64().unwrap());

    let report =
        validate_holarchy(&e, &[("homestead", &m), ("garden", &m), ("soil_sensor", &m)]).unwrap();

    assert_eq!(report.holds(), c["expect"]["holds"].as_bool().unwrap());
    assert_eq!(report.violations().len(), c["expect"]["violations"].as_u64().unwrap() as usize);
}

#[test]
fn a_local_breach_is_reported_at_its_own_level() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "a_local_breach_is_reported_at_its_own_level");
    let e = engine_of(&doc);

    let sensor = moisture(c["soil_sensor"].as_f64().unwrap());
    let garden = moisture(c["garden"].as_f64().unwrap());
    let report = validate_holarchy(&e, &[("garden", &garden), ("soil_sensor", &sensor)]).unwrap();
    let ex = &c["expect"];

    assert_eq!(report.holds(), ex["holds"].as_bool().unwrap());
    let v = report.violations();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].sustain_id, ex["violating_level"].as_str().unwrap());
    assert_eq!(v[0].local_ok, ex["local_ok"].as_bool().unwrap(), "C(s) fails");
    assert_eq!(
        v[0].consistent_with_parent,
        Some(ex["consistent_with_parent"].as_bool().unwrap()),
        "★ the two clauses are SEPARATE — this one still holds"
    );
    assert_eq!(v[0].w, ex["w"].as_f64().unwrap());
}

#[test]
fn the_parent_clause_can_fail_on_its_own() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "the_parent_clause_can_fail_on_its_own");
    let e = MonitorEngine::flatten_holarchy(vec![
        watch("household", c["household_floor"].as_f64().unwrap()),
        watch("pocket", c["pocket_floor"].as_f64().unwrap()).under("household"),
    ])
    .unwrap();

    let v = moisture(c["pocket_value"].as_f64().unwrap());
    let report = validate_holarchy(&e, &[("pocket", &v)]).unwrap();
    let l = &report.levels[0];
    let ex = &c["expect"];

    assert_eq!(l.local_ok, ex["local_ok"].as_bool().unwrap(), "inside its own region");
    assert_eq!(
        l.consistent_with_parent,
        Some(ex["consistent_with_parent"].as_bool().unwrap()),
        "★ but not its parent's — the clause the first cannot do"
    );
    assert_eq!(report.holds(), ex["holds"].as_bool().unwrap());
}

#[test]
fn a_root_has_no_parent_clause_to_satisfy() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "a_root_has_no_parent_clause_to_satisfy");
    let e = engine_of(&doc);

    let report = validate_holarchy(&e, &[("homestead", &moisture(50.0))]).unwrap();
    assert!(
        report.levels[0].consistent_with_parent.is_none(),
        "None, not true — nothing above it to agree with"
    );
    assert_eq!(report.holds(), c["expect"]["holds"].as_bool().unwrap());
}

#[test]
fn a_level_with_no_supplied_state_is_skipped_not_passed() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "a_level_with_no_supplied_state_is_skipped_not_passed");
    let e = engine_of(&doc);

    let m = moisture(50.0);
    let supplied: Vec<(&str, &Value)> = c["supply"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v.as_str().unwrap(), &m))
        .collect();

    let report = validate_holarchy(&e, &supplied).unwrap();
    assert_eq!(
        report.levels.len(),
        c["expect"]["levels_reported"].as_u64().unwrap() as usize,
        "★ 'we did not look' and 'we looked and it was fine' are different claims"
    );
    assert_eq!(report.levels[0].sustain_id, c["expect"]["reported"].as_str().unwrap());
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // Three directions kept apart — the thing this slice is most able to
    // overstate.
    let s = d["statement"].as_str().unwrap();
    for direction in ["DOWNWARD AUTHORITY exists", "UPWARD INFORMATION exists", "UPWARD ESCALATION"]
    {
        assert!(s.contains(direction), "the statement must keep {direction} distinct");
    }

    // The two near-misses are named as REAL, not dismissed.
    let nm = &d["the_two_greppable_near_misses_named_because_they_are_REAL"];
    assert!(nm["holon (83 hits)"].as_str().unwrap().contains("Not a false positive at all"));
    assert!(nm["holon (83 hits)"].as_str().unwrap().contains("EXISTS in Python"));
    assert!(nm["get_parent (10) / parent_of (2)"].as_str().unwrap().contains("Also real"));

    // Three terms recorded AT PARITY.
    let tt = &d["term_by_term"];
    let at_parity = ["the holarchy / recursive Sustain⟨…,children⟩ (§V)", "nano-sustain (|children| = 0)", "roll-up ρ"];
    for term in at_parity {
        assert!(
            tt[term].as_str().unwrap().contains("AT PARITY"),
            "{term} must be recorded at parity — claiming it would overstate the gap"
        );
    }

    // The counterweight names the authority model the reference states itself.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("OBSERVES its children, it never VETOES them"));
    assert!(cw.contains("third thing"), "and why escalation is neither of the two it has");

    // The ⊆ reading is disclosed as an interpretation.
    let interp = d["the_interpretation_disclosed"].as_str().unwrap();
    assert!(interp.contains("never defines ⊆"));
    assert!(interp.contains("recorded here as an interpretation"));

    for group in ["path_cases", "climb_cases", "invariant_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in ["path_cases", "climb_cases", "invariant_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
