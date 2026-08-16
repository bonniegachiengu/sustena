//! The Monitor engine replayed against its conformance vectors
//! (R2 · Monitor §IX + Additions · MON-9).
//!
//! **SPEC vectors, Rust-only.** The reference has no `MonitorEngine`, and the
//! sharpest statement of the gap is not a grep count: **it watches by
//! RECOMPUTING and this engine watches by ACCUMULATING.** `GET /devui/state`
//! derives its widgets fresh on every poll, so a CUSUM — definitionally an
//! accumulator — has nowhere to live.
//!
//! `monitor.json` also names the sweetest false positive in the series: the
//! only grep hits for `ewma` and `cusum` are one comment saying they do not
//! exist.
//!
//! The proof of value: a stream of state updates through one engine drives
//! `Severity` to escalate and walks the OODA loop out of OBSERVE with nobody
//! calling `observe()` by hand.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    CusumSpec, Interval, MonitorEngine, MonitorError, Ooda, OodaPhase, Region, Severity, Signal,
    SignalSpec, SustainWatch, Trajectory, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("monitor.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "monitor.json was written for a different contract version"
    );
    doc
}

// ── fixture, built from the vector file ─────────────────────────────────────

fn region_of(doc: &Value) -> Region {
    let spec = &doc["fixture"]["region"];
    let mut r = Region::new();
    for iv in spec["intervals"].as_array().unwrap() {
        let a = iv.as_array().unwrap();
        r = r.bounding(Interval::new(
            a[0].as_str().unwrap(),
            a[1].as_f64().unwrap_or(f64::NEG_INFINITY),
            a[2].as_f64().unwrap_or(f64::INFINITY),
        ));
    }
    for (dim, w) in spec["weights"].as_object().unwrap() {
        r = r.weighing(dim, w.as_f64().unwrap());
    }
    r
}

fn cusum_of(doc: &Value) -> CusumSpec {
    let c = &doc["fixture"]["cusum"];
    CusumSpec::new(
        c["mu_0"].as_f64().unwrap(),
        c["delta"].as_f64().unwrap(),
        c["h"].as_f64().unwrap(),
    )
}

fn decl(doc: &Value, id: &str) -> SustainWatch {
    SustainWatch::new(id, region_of(doc), doc["fixture"]["alpha"].as_f64().unwrap(), cusum_of(doc))
}

fn state(balance: f64) -> Value {
    json!({ "balance": balance, "pending": 0 })
}

/// Inside V (so `W = 0`) carrying the dimension the region does not bound.
fn state_pending(pending: i64) -> Value {
    json!({ "balance": 5000.0, "pending": pending })
}

fn backlog_signal(doc: &Value, id: &str) -> SustainWatch {
    let dim = doc["fixture"]["unbounded_dimension"].as_str().unwrap();
    decl(doc, id).watching_for(
        vec![Signal::new("backlog", Trajectory::ever_within(1, &format!("{dim} >= 3")))],
        SignalSpec::new(1.0, 5).unwrap(),
    )
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from monitor.json"))
}

// ── the engine owns the chain ───────────────────────────────────────────────

#[test]
fn an_engine_with_no_sustains_is_refused() {
    let doc = load();
    let _ = case(&doc, "engine_cases", "an_engine_with_no_sustains_is_refused");
    assert!(
        matches!(MonitorEngine::flatten_holarchy(vec![]), Err(MonitorError::NothingToWatch)),
        "an engine watching nothing would report calm forever"
    );
}

#[test]
fn flatten_holarchy_orders_parents_before_children() {
    let doc = load();
    let c = case(&doc, "engine_cases", "flatten_holarchy_orders_parents_before_children");

    let decls: Vec<SustainWatch> = c["declare"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let a = d.as_array().unwrap();
            let w = decl(&doc, a[0].as_str().unwrap());
            match a[1].as_str() {
                Some(p) => w.under(p),
                None => w,
            }
        })
        .collect();

    let e = MonitorEngine::flatten_holarchy(decls).unwrap();
    let want: Vec<&str> =
        c["expect"]["order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(e.sustains(), want);
}

#[test]
fn a_cycle_in_the_declared_holarchy_is_refused() {
    let doc = load();
    let c = case(&doc, "engine_cases", "a_cycle_in_the_declared_holarchy_is_refused");

    let decls: Vec<SustainWatch> = c["declare"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let a = d.as_array().unwrap();
            decl(&doc, a[0].as_str().unwrap()).under(a[1].as_str().unwrap())
        })
        .collect();

    let err = MonitorEngine::flatten_holarchy(decls).unwrap_err();
    assert!(matches!(err, MonitorError::HolarchyCycle(_)), "got {err:?}");
    assert!(err.to_string().contains(c["expect"]["message_contains"].as_str().unwrap()));
}

#[test]
fn a_malformed_declaration_is_refused() {
    let doc = load();

    let _ = case(&doc, "engine_cases", "a_parent_that_was_never_declared_is_refused");
    assert!(matches!(
        MonitorEngine::flatten_holarchy(vec![decl(&doc, "child").under("ghost")]),
        Err(MonitorError::UnknownParent { .. })
    ));

    let _ = case(&doc, "engine_cases", "a_duplicate_sustain_is_refused");
    assert!(matches!(
        MonitorEngine::flatten_holarchy(vec![decl(&doc, "a"), decl(&doc, "a")]),
        Err(MonitorError::DuplicateSustain(_))
    ));

    let _ = case(&doc, "engine_cases", "an_unwatched_sustain_is_refused");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();
    assert!(matches!(
        e.ingest("elsewhere", &state(0.0)),
        Err(MonitorError::UnknownSustain(_))
    ));
}

#[test]
fn each_sustain_keeps_its_own_chain() {
    let doc = load();
    let c = case(&doc, "engine_cases", "each_sustain_keeps_its_own_chain");
    let mut e = MonitorEngine::flatten_holarchy(vec![decl(&doc, "a"), decl(&doc, "b")]).unwrap();

    for _ in 0..c["ingests"]["a"].as_u64().unwrap() {
        e.ingest("a", &state(0.0)).unwrap();
    }
    for _ in 0..c["ingests"]["b"].as_u64().unwrap() {
        e.ingest("b", &state(0.0)).unwrap();
    }

    assert_eq!(e.history_len("a"), Some(c["expect"]["history_a"].as_u64().unwrap() as usize));
    assert_eq!(
        e.history_len("b"),
        Some(c["expect"]["history_b"].as_u64().unwrap() as usize),
        "★ one household's drift must not move another's accumulator"
    );
}

// ── §IX — the native pipeline ───────────────────────────────────────────────

#[test]
fn ingest_computes_w_as_distance_to_v_not_the_raw_state() {
    let doc = load();
    let c = case(&doc, "pipeline_cases", "ingest_computes_w_as_distance_to_v_not_the_raw_state");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();

    for step in c["expect"].as_array().unwrap() {
        let got = e.ingest("a", &state(step["balance"].as_f64().unwrap())).unwrap().reading.w;
        assert_eq!(got, step["w"].as_f64().unwrap(), "★ what enters the detector is the shortfall");
    }
}

#[test]
fn the_ewma_smooths_across_ingests_because_the_engine_owns_it() {
    let doc = load();
    let c = case(&doc, "pipeline_cases", "the_ewma_smooths_across_ingests_because_the_engine_owns_it");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();

    let first = e.ingest("a", &state(1000.0)).unwrap().reading.smoothed;
    let second = e.ingest("a", &state(0.0)).unwrap().reading.smoothed;

    assert_eq!(first, c["expect"]["first_smoothed"].as_f64().unwrap());
    assert_eq!(
        second,
        c["expect"]["second_smoothed"].as_f64().unwrap(),
        "★★ halfway toward the new level — only possible because the first was REMEMBERED"
    );
}

#[test]
fn a_quiet_stream_does_not_escalate() {
    let doc = load();
    let c = case(&doc, "pipeline_cases", "a_quiet_stream_does_not_escalate");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();

    for _ in 0..c["ingests"].as_u64().unwrap() {
        let i = e.ingest("a", &state(c["balance"].as_f64().unwrap())).unwrap();
        assert_eq!(i.escalates(), c["expect"]["escalates"].as_bool().unwrap());
        assert_eq!(i.severity(), Severity::Info);
        assert!(
            i.observation().is_none(),
            "★ no route from a quiet ingest into OBSERVE at all"
        );
    }
}

#[test]
fn a_sustained_drift_eventually_escalates() {
    let doc = load();
    let c = case(&doc, "pipeline_cases", "a_sustained_drift_eventually_escalates");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();

    let mut escalated = false;
    for _ in 0..c["within_ingests"].as_u64().unwrap() {
        if e.ingest("a", &state(c["balance"].as_f64().unwrap())).unwrap().escalates() {
            escalated = true;
            break;
        }
    }
    assert_eq!(
        escalated,
        c["expect"]["escalates"].as_bool().unwrap(),
        "★ the one behaviour a recompute-per-request design structurally cannot produce"
    );
}

#[test]
fn there_is_no_kalman_stage() {
    let doc = load();
    let c = case(&doc, "pipeline_cases", "there_is_no_kalman_stage");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();

    let r = e.ingest("a", &state(c["balance"].as_f64().unwrap())).unwrap().reading;
    assert_eq!(r.w, c["expect"]["w"].as_f64().unwrap());
    assert_eq!(
        r.smoothed == r.w,
        c["expect"]["smoothed_equals_w"].as_bool().unwrap(),
        "★ the EWMA seeds directly from d(s,V) — nothing estimated it first"
    );

    // And the decline is recorded as a decline, not as a gap.
    assert!(
        doc["divergence"]["term_by_term"]["Kalman (MON-2)"]
            .as_str()
            .unwrap()
            .contains("DECLINED IMPORT, not a gap")
    );
}

// ── ★ OBSERVE is self-driving ───────────────────────────────────────────────

#[test]
fn a_quiet_ingest_leaves_the_loop_untouched() {
    let doc = load();
    let c = case(&doc, "self_driving_cases", "a_quiet_ingest_leaves_the_loop_untouched");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();
    let mut o = Ooda::new();

    let d = e.drive(&mut o, "a", &state(c["balance"].as_f64().unwrap())).unwrap();
    assert!(d.step.is_none());
    assert!(d.held_back.is_none(), "nothing was held back — nothing tried to go");
    assert_eq!(format!("{:?}", o.phase()), c["expect"]["phase"].as_str().unwrap());
}

#[test]
fn an_escalating_ingest_drives_the_loop_into_orient() {
    let doc = load();
    let c = case(&doc, "self_driving_cases", "an_escalating_ingest_drives_the_loop_into_orient");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();
    let mut o = Ooda::new();

    let mut drove = false;
    for _ in 0..c["within_ingests"].as_u64().unwrap() {
        if e.drive(&mut o, "a", &state(c["balance"].as_f64().unwrap())).unwrap().step.is_some() {
            drove = true;
            break;
        }
    }
    assert_eq!(
        drove,
        c["expect"]["drove"].as_bool().unwrap(),
        "★★ the engine drove OBSERVE itself — nobody called observe() by hand"
    );
    assert_eq!(format!("{:?}", o.phase()), c["expect"]["phase"].as_str().unwrap());
}

#[test]
fn a_signal_alone_drives_the_loop_even_without_a_cusum_crossing() {
    let doc = load();
    let c = case(&doc, "self_driving_cases", "a_signal_alone_drives_the_loop_even_without_a_cusum_crossing");
    let mut e = MonitorEngine::watching(backlog_signal(&doc, "a")).unwrap();
    let mut o = Ooda::new();

    let d = e.drive(&mut o, "a", &state_pending(c["pending"].as_i64().unwrap())).unwrap();
    let ex = &c["expect"];

    assert_eq!(d.ingested.reading.w, ex["w"].as_f64().unwrap(), "inside V");
    assert_eq!(d.ingested.reading.escalates(), ex["reading_escalates"].as_bool().unwrap());
    assert_eq!(
        d.ingested.escalates(),
        ex["ingested_escalates"].as_bool().unwrap(),
        "★ a distance cannot measure what V does not describe"
    );
    assert!(d.step.is_some());
    assert_eq!(format!("{:?}", o.phase()), ex["phase"].as_str().unwrap());
}

#[test]
fn watching_continues_while_the_loop_is_busy_and_says_who_held_it() {
    let doc = load();
    let c = case(&doc, "self_driving_cases", "watching_continues_while_the_loop_is_busy_and_says_who_held_it");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();
    let mut o = Ooda::new();
    let b = c["balance"].as_f64().unwrap();

    e.drive(&mut o, "a", &state(b)).unwrap();
    assert_eq!(o.phase(), OodaPhase::Orient, "the loop has moved on");

    let before = e.history_len("a").unwrap();
    let d = e.drive(&mut o, "a", &state(b)).unwrap();

    assert!(d.ingested.escalates(), "still escalating");
    assert_eq!(
        e.history_len("a") == Some(before + 1),
        c["expect"]["history_grew"].as_bool().unwrap(),
        "★ the watching never stopped"
    );
    assert!(d.step.is_none(), "and it did not barge into a busy loop");
    assert_eq!(
        format!("{:?}", d.held_back.unwrap()),
        c["expect"]["held_back"].as_str().unwrap(),
        "★ and it says which phase held it"
    );
}

#[test]
fn a_persistent_signal_does_not_repeatedly_drive_observe() {
    let doc = load();
    let c = case(&doc, "self_driving_cases", "a_persistent_signal_does_not_repeatedly_drive_observe");
    let mut e = MonitorEngine::watching(backlog_signal(&doc, "a")).unwrap();
    let mut o = Ooda::new();

    let p = c["pendings"].as_array().unwrap();
    let first = e.drive(&mut o, "a", &state_pending(p[0].as_i64().unwrap())).unwrap();
    assert_eq!(first.step.is_some(), c["expect"]["first_drove"].as_bool().unwrap());

    let second = e.drive(&mut o, "a", &state_pending(p[1].as_i64().unwrap())).unwrap();
    assert_eq!(
        second.ingested.escalates(),
        c["expect"]["second_escalates"].as_bool().unwrap(),
        "★ the same condition is debounced, not re-fired"
    );
    assert!(second.held_back.is_none(), "nothing tried to go in the first place");

    let want: Vec<String> = c["expect"]["debounced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        second.ingested.signals.as_ref().unwrap().debounced,
        want,
        "the suppression stays VISIBLE all the way up here"
    );
}

// ── the disclosed limit ─────────────────────────────────────────────────────

#[test]
fn the_history_is_not_truncated() {
    let doc = load();
    let c = case(&doc, "disclosed_limits", "the_history_is_not_truncated");
    let mut e = MonitorEngine::watching(decl(&doc, "a")).unwrap();

    let n = c["ingests"].as_u64().unwrap();
    for _ in 0..n {
        e.ingest("a", &state(5000.0)).unwrap();
    }
    assert_eq!(e.history_len("a"), Some(c["expect"]["history_len"].as_u64().unwrap() as usize));
    assert!(
        c["why"].as_str().unwrap().contains("bounded Never"),
        "the reason it is not truncated is recorded, not just the fact"
    );
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The gap stated as a design difference, not a grep count.
    let s = d["statement"].as_str().unwrap();
    assert!(s.contains("RECOMPUTING") && s.contains("ACCUMULATING"));
    assert!(s.contains("nowhere to live"), "and why that makes a CUSUM structurally absent");

    // The sweetest false positive in the series.
    let fp = &d["the_three_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(
        fp["ewma (1 hit) and cusum (1 hit)"].as_str().unwrap().contains("ABSENCE"),
        "the only hits for the detectors are a comment saying they are not there"
    );

    // Kalman is a DECLINE, not a gap — the one term where rust-ahead would be
    // the wrong word.
    assert!(d["term_by_term"]["Kalman (MON-2)"].as_str().unwrap().contains("DECLINED IMPORT"));

    // The two unbuilt rows are named as rows, not as omissions.
    for row in ["BeliefStateTracker (MON-8)", "WidgetVisualEncoder / render_widget (MON-7)"] {
        assert!(
            d["term_by_term"][row].as_str().unwrap().contains("NOT BUILT HERE"),
            "{row} must be named as a separate row rather than quietly missing"
        );
    }
    assert!(
        d["term_by_term"]["tick() at display refresh rate"]
            .as_str()
            .unwrap()
            .contains("OUTSIDE CORE")
    );

    // The counterweight: the reference genuinely watches, and cannot remember.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("real, shipped and in daily use"));
    assert!(cw.contains("REMEMBER"), "the precise thing it cannot do");

    for group in ["engine_cases", "pipeline_cases", "self_driving_cases", "disclosed_limits"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in ["engine_cases", "pipeline_cases", "self_driving_cases", "disclosed_limits"] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
