//! The OODA loop replayed against its conformance vectors
//! (R2 · Controller §III + §IX · CTL-3).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no OODA machine —
//! `OODA`, `orient`, `state_machine` and `control_loop` are all grep-0. More
//! precisely than *absent*: it has real pieces of OBSERVE, DECIDE and ACT, and
//! **no `δ`**. Nothing holds a phase, so nothing enforces the ordering.
//!
//! `ooda.json` names five greppable false positives and keeps a counterweight
//! that treats `curated_ui.compose(r)` as the genuine near-miss it is.
//!
//! The proof of value is one full cycle: a signal at OBSERVE → Tenet ranks at
//! ORIENT → the Controller surfaces and a human authorises at DECIDE → the
//! gate executes at ACT → back to OBSERVE.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    backward_induct, ApprovalToken, AutomationTable, BellmanSpec, Candidate, ControlEvent,
    Enforcement, Interval, InversionPoint, NonceLedger, Ooda, OodaError, OodaObservation, OodaPhase,
    Orientation, Preferences, ProposalStatus, Region, Registry, Severity, SheridanLevel, Signal,
    SignalMonitor, SignalSpec, Simulated, Space, StayReason, SurfacedDecision, Trajectory,
    TransitionModel, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("ooda.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "ooda.json was written for a different contract version"
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

fn state(balance: f64) -> Value {
    json!({"finances": {
        "liquid": {"balance": balance},
        "pockets": {},
        "income": {"monthly_total": 0.0, "sources": []}
    }})
}

fn live(doc: &Value) -> Value {
    state(doc["fixture"]["live_balance"].as_f64().unwrap())
}

fn table_of(doc: &Value) -> AutomationTable {
    let mut t = AutomationTable::new();
    for (k, lvl) in doc["fixture"]["automation_table"].as_object().unwrap() {
        let level = match lvl.as_str().unwrap() {
            "ExecuteAndNotify" => SheridanLevel::ExecuteAndNotify,
            "ExecuteIfApproved" => SheridanLevel::ExecuteIfApproved,
            other => panic!("unknown Sheridan level '{other}'"),
        };
        t = t.declaring(k, level);
    }
    t
}

fn prefs_of(doc: &Value) -> Preferences {
    let p = &doc["fixture"]["preferences"];
    let mut prefs = Preferences::default().with_threshold(p["threshold"].as_f64().unwrap());
    for (k, w) in p["weights"].as_object().unwrap() {
        prefs = prefs.weighing(k, w.as_f64().unwrap());
    }
    prefs
}

/// A real `MonitorReport`, so an `OodaObservation` cannot be fabricated.
fn report(fires: bool) -> sustena_core::MonitorReport {
    let mut m = SignalMonitor::new(
        vec![Signal::new("drain", Trajectory::ever_within(1, "finances.liquid.balance <= 100"))],
        SignalSpec::new(1.0, 3).unwrap(),
    )
    .unwrap();
    let h = if fires { vec![state(50.0)] } else { vec![state(9000.0)] };
    m.monitor(&h).unwrap()
}

fn severity_of(name: &str) -> Severity {
    match name {
        "Info" => Severity::Info,
        "Warning" => Severity::Warning,
        "Critical" => Severity::Critical,
        other => panic!("unknown severity '{other}'"),
    }
}

/// A real `Plan`, so an `Orientation` cannot be fabricated.
fn plan_of(doc: &Value) -> sustena_core::Plan {
    let space: Space = doc["fixture"]["plan_space"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), state(v.as_f64().unwrap())))
        .collect();
    let model = TransitionModel::new()
        .moving("here", "top_up", "better")
        .moving("here", "drain", "worse")
        .moving("better", "hold", "better")
        .moving("worse", "hold", "worse");
    let ip = InversionPoint::Predicate(
        doc["fixture"]["inversion_point"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect(),
    );
    backward_induct(&space, &model, &ip, &BellmanSpec::new(2, 0.9, 100.0)).unwrap()
}

fn candidates_of(doc: &Value) -> Vec<Candidate> {
    doc["fixture"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let ev = ControlEvent::new(c["action_type"].as_str().unwrap(), c["operator"].as_str().unwrap())
                .with_param("amount", json!(c["amount"].as_f64().unwrap()));
            Candidate::new(ev, c["to"].as_str().unwrap())
                .landing_in(state(c["lands_at"].as_f64().unwrap()))
        })
        .collect()
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from ooda.json"))
}

/// Walk to DECIDE using the real Monitor and Tenet artifacts.
fn to_decide(doc: &Value) -> Ooda {
    let mut o = Ooda::new();
    o.observe(OodaObservation::from_monitor(Severity::Warning, &report(true))).unwrap();
    o.orient(Orientation::from_plan(&plan_of(doc), 0, candidates_of(doc)).unwrap()).unwrap();
    o
}

fn token_for(d: &SurfacedDecision, nonce: u64) -> ApprovalToken {
    Simulated::from_sandbox("ooda-1", d.binding(), Ok(()))
        .expect("sandbox committed")
        .voted(ProposalStatus::Passed)
        .expect("passed")
        .approve("bonnie", nonce, 10_000)
}

// ── §III — the machine ──────────────────────────────────────────────────────

#[test]
fn q0_is_observe() {
    let doc = load();
    let c = case(&doc, "machine_cases", "q0_is_observe");
    assert_eq!(format!("{:?}", Ooda::new().phase()), c["expect"]["phase"].as_str().unwrap());
}

#[test]
fn a_quiet_observation_stays_at_observe() {
    let doc = load();
    let c = case(&doc, "machine_cases", "a_quiet_observation_stays_at_observe");
    let mut o = Ooda::new();

    let obs = OodaObservation::from_monitor(
        severity_of(c["severity"].as_str().unwrap()),
        &report(c["signal_fires"].as_bool().unwrap()),
    );
    assert!(!obs.escalates());

    let step = o.observe(obs).unwrap();
    assert!(
        matches!(step, sustena_core::OodaStep::Stayed(StayReason::NothingEscalated { .. })),
        "★ STAY is a real outcome, not a failure — it is what calm is made of"
    );
    assert_eq!(format!("{:?}", o.phase()), c["expect"]["phase"].as_str().unwrap());
    assert_eq!(step.interrupts_a_human(), c["expect"]["interrupts_a_human"].as_bool().unwrap());
}

#[test]
fn a_quiet_observation_still_says_what_it_was_watching() {
    let doc = load();
    let c = case(&doc, "machine_cases", "a_quiet_observation_still_says_what_it_was_watching");
    let obs = OodaObservation::from_monitor(Severity::Info, &report(false));

    let want: Vec<String> = c["expect"]["watched"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(obs.watched(), want.as_slice(), "'nothing fired' is only ever 'nothing among these'");
}

#[test]
fn an_escalating_observation_advances_to_orient() {
    let doc = load();
    let c = case(&doc, "machine_cases", "an_escalating_observation_advances_to_orient");
    let mut o = Ooda::new();
    let obs = OodaObservation::from_monitor(
        severity_of(c["severity"].as_str().unwrap()),
        &report(c["signal_fires"].as_bool().unwrap()),
    );
    assert!(obs.escalates());
    o.observe(obs).unwrap();
    assert_eq!(format!("{:?}", o.phase()), c["expect"]["advanced_to"].as_str().unwrap());
}

#[test]
fn severity_alone_escalates_even_with_no_signal() {
    let doc = load();
    let c = case(&doc, "machine_cases", "severity_alone_escalates_even_with_no_signal");
    let obs = OodaObservation::from_monitor(
        severity_of(c["severity"].as_str().unwrap()),
        &report(c["signal_fires"].as_bool().unwrap()),
    );
    assert_eq!(obs.escalates(), c["expect"]["escalates"].as_bool().unwrap());
    assert_eq!(obs.signals().len(), c["expect"]["signals"].as_u64().unwrap() as usize);
}

#[test]
fn phases_fire_on_completion_events_not_on_request() {
    let doc = load();
    let _ = case(&doc, "machine_cases", "phases_fire_on_completion_events_not_on_request");
    let mut o = Ooda::new();

    assert!(matches!(
        o.orient(Orientation::from_plan(&plan_of(&doc), 0, candidates_of(&doc)).unwrap()),
        Err(OodaError::WrongPhase { .. })
    ));

    let mut n = NonceLedger::new();
    assert!(matches!(
        o.act(&Registry::default(), &[], &Enforcement::default(), &live(&doc), &mut n, 0),
        Err(OodaError::WrongPhase { .. })
    ));
}

// ── ORIENT is owned by Tenet ────────────────────────────────────────────────

#[test]
fn the_orientation_is_ranked_by_the_plan() {
    let doc = load();
    let c = case(&doc, "orient_cases", "the_orientation_is_ranked_by_the_plan");
    let orientation = Orientation::from_plan(&plan_of(&doc), 0, candidates_of(&doc)).unwrap();

    assert_eq!(
        orientation.top().event.operator,
        c["expect"]["top_operator"].as_str().unwrap(),
        "★ the ranking comes from a real sweep, not from anything this machine computed"
    );
    assert_eq!(orientation.horizon(), c["expect"]["horizon"].as_u64().unwrap() as usize);
}

#[test]
fn a_candidate_the_plan_cannot_score_is_refused_not_zeroed() {
    let doc = load();
    let c = case(&doc, "orient_cases", "a_candidate_the_plan_cannot_score_is_refused_not_zeroed");
    let bad = vec![Candidate::new(
        ControlEvent::new("SPEND", "budget.spend"),
        c["to"].as_str().unwrap(),
    )];
    assert!(matches!(
        Orientation::from_plan(&plan_of(&doc), 0, bad),
        Err(OodaError::UnscoredCandidate { .. })
    ));
}

#[test]
fn an_empty_candidate_set_is_refused() {
    let doc = load();
    let _ = case(&doc, "orient_cases", "an_empty_candidate_set_is_refused");
    assert!(matches!(
        Orientation::from_plan(&plan_of(&doc), 0, vec![]),
        Err(OodaError::NothingToOrient)
    ));
}

// ── ★ DECIDE — the human seam ───────────────────────────────────────────────

#[test]
fn the_loop_parks_at_decide_and_cannot_advance_itself() {
    let doc = load();
    let c = case(&doc, "decide_cases", "the_loop_parks_at_decide_and_cannot_advance_itself");
    let mut o = to_decide(&doc);

    let step = o.decide(&region_of(&doc), &live(&doc), &table_of(&doc), &prefs_of(&doc), 0).unwrap();
    assert_eq!(step.interrupts_a_human(), c["expect"]["interrupts_a_human"].as_bool().unwrap());
    assert_eq!(o.awaiting_authorization(), c["expect"]["awaiting_authorization"].as_bool().unwrap());

    let mut n = NonceLedger::new();
    let err = o
        .act(
            &Registry::default(),
            &Registry::default().names(),
            &Enforcement::default(),
            &live(&doc),
            &mut n,
            0,
        )
        .unwrap_err();
    assert_eq!(
        format!("{err:?}").split(' ').next().unwrap(),
        c["expect"]["act_error"].as_str().unwrap(),
        "★ the loop cannot close itself past the human seam"
    );
}

#[test]
fn decide_refuses_without_an_orientation() {
    let doc = load();
    let _ = case(&doc, "decide_cases", "decide_refuses_without_an_orientation");
    let mut o = Ooda::new();
    o.observe(OodaObservation::from_monitor(Severity::Warning, &report(true))).unwrap();

    // At ORIENT, not DECIDE — nothing has been scored yet.
    assert!(
        matches!(
            o.decide(&region_of(&doc), &live(&doc), &table_of(&doc), &prefs_of(&doc), 0),
            Err(OodaError::WrongPhase { .. })
        ),
        "★ Boyd: the scoring completes before a person sees anything"
    );
}

#[test]
fn a_token_for_a_different_act_does_not_admit_this_one() {
    let doc = load();
    let c = case(&doc, "decide_cases", "a_token_for_a_different_act_does_not_admit_this_one");
    let mut o = to_decide(&doc);
    o.decide(&region_of(&doc), &live(&doc), &table_of(&doc), &prefs_of(&doc), 0).unwrap();

    let other = ControlEvent::new("SPEND", "budget.spend").with_param("amount", json!(1.0));
    let wrong = Simulated::from_sandbox("p", other.binding(), Ok(()))
        .unwrap()
        .voted(ProposalStatus::Passed)
        .unwrap()
        .approve("bonnie", 7, 10_000);

    assert!(matches!(o.authorize(wrong), Err(OodaError::TokenBindingMismatch { .. })));
    assert_eq!(o.awaiting_authorization(), c["expect"]["still_parked"].as_bool().unwrap());
}

#[test]
fn the_human_moves_the_loop_to_act() {
    let doc = load();
    let c = case(&doc, "decide_cases", "the_human_moves_the_loop_to_act");
    let mut o = to_decide(&doc);
    let step = o.decide(&region_of(&doc), &live(&doc), &table_of(&doc), &prefs_of(&doc), 0).unwrap();

    o.authorize(token_for(step.surfaced().unwrap(), 1)).unwrap();
    assert_eq!(format!("{:?}", o.phase()), c["expect"]["advanced_to"].as_str().unwrap());
    assert_eq!(o.awaiting_authorization(), c["expect"]["awaiting_authorization"].as_bool().unwrap());
}

#[test]
fn withheld_is_not_automated_and_executes_nothing() {
    let doc = load();
    let c = case(&doc, "decide_cases", "withheld_is_not_automated_and_executes_nothing");
    let mut o = to_decide(&doc);

    let strict = Preferences::default()
        .with_threshold(c["threshold"].as_f64().unwrap())
        .weighing("SPEND", 1.0);
    let step = o.decide(&region_of(&doc), &live(&doc), &table_of(&doc), &strict, 0).unwrap();

    assert!(matches!(step, sustena_core::OodaStep::Stayed(StayReason::Withheld { .. })));
    assert_eq!(step.interrupts_a_human(), c["expect"]["interrupts_a_human"].as_bool().unwrap());
    assert_eq!(format!("{:?}", o.phase()), c["expect"]["phase"].as_str().unwrap());
    assert_eq!(
        o.surfaced().is_none(),
        c["expect"]["nothing_pending"].as_bool().unwrap(),
        "★ an attention filter must not become an auto-approver"
    );
}

#[test]
fn an_automated_action_skips_the_human_and_says_so() {
    let doc = load();
    let c = case(&doc, "decide_cases", "an_automated_action_skips_the_human_and_says_so");
    let mut o = Ooda::new();
    o.observe(OodaObservation::from_monitor(Severity::Warning, &report(true))).unwrap();

    let routine = vec![Candidate::new(
        ControlEvent::new(c["action_type"].as_str().unwrap(), "budget.record_income")
            .with_param("amount", json!(400.0)),
        "better",
    )
    .landing_in(state(900.0))];
    o.orient(Orientation::from_plan(&plan_of(&doc), 0, routine).unwrap()).unwrap();

    let step = o.decide(&region_of(&doc), &live(&doc), &table_of(&doc), &prefs_of(&doc), 0).unwrap();
    assert!(matches!(step, sustena_core::OodaStep::Automated { .. }));
    assert_eq!(format!("{:?}", o.phase()), c["expect"]["phase"].as_str().unwrap());

    let t = o.trace().last().unwrap();
    assert_eq!(
        t.human_asked,
        c["expect"]["human_asked"].as_bool().unwrap(),
        "★ an action nobody was asked about must be VISIBLE in the trace"
    );
    assert!(t.note.contains(c["expect"]["note_contains"].as_str().unwrap()));
}

// ── ★★ the full cycle ───────────────────────────────────────────────────────

#[test]
fn the_full_cycle_closes_and_returns_to_observe() {
    let doc = load();
    let c = case(&doc, "full_cycle_cases", "the_full_cycle_closes_and_returns_to_observe");
    let e = &c["expect"];

    // OBSERVE — a real Monitor artifact.
    let mut o = Ooda::new();
    o.observe(OodaObservation::from_monitor(Severity::Warning, &report(true))).unwrap();

    // ORIENT — a real Tenet sweep.
    o.orient(Orientation::from_plan(&plan_of(&doc), 0, candidates_of(&doc)).unwrap()).unwrap();

    // DECIDE — the Controller's own work, then a human.
    let step = o.decide(&region_of(&doc), &live(&doc), &table_of(&doc), &prefs_of(&doc), 0).unwrap();
    let decision = step.surfaced().expect("surfaced");
    o.authorize(token_for(decision, 1)).unwrap();

    // ACT — the real gate.
    let mut nonces = NonceLedger::new();
    let execution = o
        .act(
            &Registry::default(),
            &Registry::default().names(),
            &Enforcement::default(),
            &live(&doc),
            &mut nonces,
            0,
        )
        .unwrap();

    assert_eq!(
        execution.committed(),
        e["committed"].as_bool().unwrap(),
        "★ an act really happened: {:?}",
        execution.result.reason
    );
    assert_eq!(format!("{:?}", o.phase()), e["final_phase"].as_str().unwrap());
    assert_eq!(o.cycle(), e["cycle"].as_u64().unwrap() as usize);

    // The human was asked exactly at DECIDE, and nowhere else.
    let asked: Vec<&sustena_core::Transition> =
        o.trace().iter().filter(|t| t.human_asked).collect();
    assert_eq!(asked.len(), e["human_asked_transitions"].as_u64().unwrap() as usize);
    assert_eq!(
        asked.iter().all(|t| t.from == OodaPhase::Decide),
        e["human_asked_only_at_decide"].as_bool().unwrap(),
        "★ §III: the human enters at DECIDE, not OBSERVE"
    );
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The statement is precise about WHAT is missing — δ, not everything.
    let s = d["statement"].as_str().unwrap();
    assert!(s.contains("no δ"), "the gap is the transition function, not the whole loop");
    assert!(s.contains("without ever having surfaced"), "and its concrete consequence");

    // The false positives, named — including the one that is a real near-miss.
    let fp = &d["the_five_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["surface (33 hits)"].as_str().unwrap().contains("near-miss"));
    assert!(fp["ranked (2 hits)"].as_str().unwrap().contains("WIDGETS TO SHOW"));
    assert!(fp["decide (9 hits)"].as_str().unwrap().contains("genuinely honoured"));

    // Two terms are recorded as AT PARITY — claiming the whole module would
    // overstate the gap.
    let tt = &d["term_by_term"];
    assert!(tt["ACT"].as_str().unwrap().contains("AT PARITY"));
    assert!(tt["DECIDE"].as_str().unwrap().contains("at parity as a SURFACE"));
    assert!(tt["OBSERVE"].as_str().unwrap().contains("PARTLY AT PARITY"));

    // The counterweight names three shipped things.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    for real in ["curated_ui.compose(r)", "advisory layer", "control.execute_approved"] {
        assert!(cw.contains(real), "the counterweight must name {real}");
    }

    // The automation widening is disclosed, not buried.
    let auto = d["the_automation_disclosure"].as_str().unwrap();
    assert!(auto.contains("EffectClass::Unchecked"));
    assert!(auto.contains("human_asked: false"), "and made visible in the trace");

    for group in ["machine_cases", "orient_cases", "decide_cases", "full_cycle_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in ["machine_cases", "orient_cases", "decide_cases", "full_cycle_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
