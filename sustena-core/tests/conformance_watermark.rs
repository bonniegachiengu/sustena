//! Watermarks and the declared lateness policy, replayed against its
//! conformance vectors (R2 · Events and Time §IV · EVT-6).
//!
//! **SPEC vectors, Rust-only — and the divergence is a specific failure rather
//! than an absence.** The reference has three real windows and every one of
//! them takes its boundary from the processing clock, which is `W(τ) = τ`: the
//! §4J.3 failure, in shipped code.
//!
//! ★★ The load-bearing property is that **no window closes by wall-clock
//! hope** — `close` takes a `&Watermark` and nothing else, so reproducing the
//! reference's behaviour requires declaring a zero out-of-orderness bound out
//! loud.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    estimate_heuristic, Advance, CausalStamp, Closing, Event, LateOutcome, Lateness, Provenance,
    SourceGuarantee, Watermark, WatermarkError, WatermarkKind, WatermarkTracker, Window,
    WindowError, WindowState, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("watermark.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "watermark.json was written for a different contract version"
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

fn tau(doc: &Value) -> i64 {
    doc["fixture"]["tau"].as_i64().unwrap()
}

fn window(doc: &Value, lateness: Lateness) -> Window {
    let w = &doc["fixture"]["window"];
    Window::new(w["start"].as_i64().unwrap(), w["end"].as_i64().unwrap(), lateness).unwrap()
}

/// The guarantee at index `i` of the fixture: 0 = the honest 2h bound,
/// 1 = ★ the zero bound that reproduces the reference.
fn guarantee(doc: &Value, i: usize) -> SourceGuarantee {
    let g = &doc["fixture"]["guarantees"][i];
    SourceGuarantee::new(
        g["source_id"].as_str().unwrap(),
        g["max_out_of_orderness"].as_i64().unwrap(),
    )
}

fn ev(id: &str, t_event: i64, t_ingest: Option<i64>) -> Event {
    Event {
        id: id.into(),
        name: "event.finances.pocket_spent".into(),
        t_event,
        t_ingest,
        provenance: Provenance::Observed,
        source: None,
        stamp: CausalStamp::new("phone"),
        causes: vec![],
        mutations: vec![],
        payload: None,
    }
}

fn skew_log(doc: &Value) -> Vec<Event> {
    doc["fixture"]["skew_log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            ev(
                e["id"].as_str().unwrap(),
                e["t_event"].as_i64().unwrap(),
                e["t_ingest"].as_i64(),
            )
        })
        .collect()
}

// ── ★★ no window closes by wall-clock hope ─────────────────────────────────

#[test]
fn a_window_will_not_close_on_processing_time_alone() {
    // ★★ THE LOAD-BEARING CASE. τ stands a full hour past the window's end, and
    // the window is still open — because closing takes a watermark, and the
    // honest guarantee puts it an hour short.
    let doc = load();
    let c = case(
        &doc,
        "wall_clock_cases",
        "★★_a_window_will_NOT_close_on_processing_time_alone",
    );

    let mut st = WindowState::new(window(&doc, Lateness::Drop));
    let honest = Watermark::perfect(tau(&doc), guarantee(&doc, 0));
    assert!(honest.mark() < st.window().end(), "the fixture must actually be short");
    assert!(tau(&doc) > st.window().end(), "and the CLOCK must actually be past it");

    match st.close(&honest, json!(1)) {
        Closing::NotYet { needs, watermark } => {
            assert_eq!(needs, c["expect"]["needs"].as_i64().unwrap());
            assert_eq!(watermark, c["expect"]["watermark"].as_i64().unwrap());
        }
        other => panic!("closed on the clock rather than the watermark: {other:?}"),
    }
    assert!(!st.is_closed(), "τ past the end is not a reason to answer");
}

#[test]
fn reproducing_the_reference_requires_declaring_a_zero_bound_out_loud() {
    // ★★ The divergence made executable. Same τ, same window — the only
    // difference is a declaration someone had to write down.
    let doc = load();
    let c = case(
        &doc,
        "wall_clock_cases",
        "★★_reproducing_the_reference_requires_DECLARING_A_ZERO_BOUND_OUT_LOUD",
    );

    let zero_bound = guarantee(&doc, 1);
    assert_eq!(zero_bound.max_out_of_orderness, 0, "the fixture's own zero claim");

    let hopeful = Watermark::perfect(tau(&doc), zero_bound);
    assert_eq!(hopeful.mark(), hopeful.tau(), "this IS W(τ) = τ");
    assert!(c["expect"]["mark_equals_tau_under_zero_bound"].as_bool().unwrap());

    let mut hoping = WindowState::new(window(&doc, Lateness::Drop));
    let mut honest = WindowState::new(window(&doc, Lateness::Drop));
    let realistic = Watermark::perfect(tau(&doc), guarantee(&doc, 0));

    assert!(matches!(hoping.close(&hopeful, json!(1)), Closing::Closed { .. }));
    assert!(matches!(honest.close(&realistic, json!(1)), Closing::NotYet { .. }));
}

#[test]
fn a_heuristic_close_carries_its_caveat_forward_and_a_guaranteed_one_does_not() {
    // ★ Closing is an answer, not a proof of completeness.
    let doc = load();
    let c = case(
        &doc,
        "wall_clock_cases",
        "★_a_heuristic_close_carries_its_caveat_forward_and_a_guaranteed_one_does_not",
    );

    let mut a = WindowState::new(window(&doc, Lateness::Drop));
    match a.close(&Watermark::perfect(tau(&doc), guarantee(&doc, 1)), json!(1)) {
        Closing::Closed { may_be_wrong, .. } => {
            assert_eq!(may_be_wrong, c["expect"]["perfect_may_be_wrong"].as_bool().unwrap())
        }
        other => panic!("{other:?}"),
    }

    let est = estimate_heuristic(&skew_log(&doc), tau(&doc), 100).unwrap();
    let mut b = WindowState::new(Window::new(0, est.mark(), Lateness::Drop).unwrap());
    match b.close(&est, json!(1)) {
        Closing::Closed { may_be_wrong, .. } => {
            assert_eq!(may_be_wrong, c["expect"]["heuristic_may_be_wrong"].as_bool().unwrap())
        }
        other => panic!("{other:?}"),
    }
}

// ── monotonicity ────────────────────────────────────────────────────────────

#[test]
fn a_watermark_never_moves_backwards_and_says_when_it_tried_to() {
    let doc = load();
    let c = case(
        &doc,
        "monotonicity_cases",
        "★_a_watermark_never_moves_backwards_and_SAYS_when_it_tried_to",
    );
    let g = guarantee(&doc, 1); // zero bound, so mark == τ and the arithmetic is plain
    let hour = doc["fixture"]["HOUR"].as_i64().unwrap();

    let mut t = WatermarkTracker::new(Watermark::perfect(5 * hour, g.clone()));
    assert_eq!(
        t.advance(Watermark::perfect(8 * hour, g.clone())),
        Advance::Advanced { from: 5 * hour, to: c["expect"]["advanced_to"].as_i64().unwrap() }
    );

    let held = t.advance(Watermark::perfect(6 * hour, g));
    assert_eq!(
        held,
        Advance::HeldBack {
            proposed: c["expect"]["held_back_proposed"].as_i64().unwrap(),
            held_at: c["expect"]["held_at"].as_i64().unwrap(),
        }
    );
    assert!(held.was_held_back(), "reported, not silently swallowed");
    assert_eq!(t.current().mark(), c["expect"]["current_after"].as_i64().unwrap());
}

#[test]
fn a_closed_window_cannot_be_un_closed_by_a_retreating_reading() {
    // The reason the requirement exists rather than the requirement restated.
    let doc = load();
    let g = guarantee(&doc, 1);
    let hour = doc["fixture"]["HOUR"].as_i64().unwrap();

    let mut t = WatermarkTracker::new(Watermark::perfect(tau(&doc), g.clone()));
    let mut st = WindowState::new(window(&doc, Lateness::Drop));
    assert!(matches!(st.close(t.current(), json!(1)), Closing::Closed { .. }));

    t.advance(Watermark::perfect(3 * hour, g));
    assert!(st.window().is_closed_by(t.current()), "still closed");
    assert!(st.is_closed());
}

// ── heuristic estimation, over EVT-5's skew ────────────────────────────────

#[test]
fn the_heuristic_is_estimated_from_observed_skew_not_invented() {
    let doc = load();
    let c = case(
        &doc,
        "heuristic_cases",
        "★_the_heuristic_is_ESTIMATED_FROM_OBSERVED_SKEW_not_invented",
    );
    let w = estimate_heuristic(&skew_log(&doc), tau(&doc), 100).unwrap();

    match w.kind() {
        WatermarkKind::Heuristic { percentile, allowance, sampled } => {
            assert_eq!(*percentile as u64, c["expect"]["percentile"].as_u64().unwrap());
            assert_eq!(*allowance, c["expect"]["allowance"].as_i64().unwrap());
            assert_eq!(*sampled as u64, c["expect"]["sampled"].as_u64().unwrap());
        }
        other => panic!("estimation must never mint a guarantee: {other:?}"),
    }
    assert_eq!(w.mark(), c["expect"]["mark"].as_i64().unwrap());

    // The allowance really is the fixture's own largest recorded skew.
    let largest = doc["fixture"]["skew_log"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["skew"].as_i64().unwrap())
        .max()
        .unwrap();
    assert_eq!(c["expect"]["allowance"].as_i64().unwrap(), largest);
}

#[test]
fn no_volume_of_data_promotes_an_estimate_to_a_guarantee() {
    // ★★ Skew is heavy-tailed and unbounded, so the largest ever observed is
    // not a bound on the next. There is no path here that says otherwise.
    let doc = load();
    let c = case(
        &doc,
        "heuristic_cases",
        "★★_NO_VOLUME_OF_DATA_promotes_an_estimate_to_a_guarantee",
    );
    let log: Vec<Event> =
        (0..500).map(|i| ev(&format!("e{i}"), i * 1000, Some(i * 1000 + 1))).collect();
    let w = estimate_heuristic(&log, tau(&doc), 100).unwrap();

    assert!(matches!(w.kind(), WatermarkKind::Heuristic { .. }));
    assert_eq!(c["expect"]["kind_after_500_samples"].as_str().unwrap(), "Heuristic");
    assert_eq!(w.kind().may_be_wrong(), c["expect"]["may_be_wrong"].as_bool().unwrap());
}

#[test]
fn an_unmeasured_source_is_not_a_punctual_one() {
    // ★★ EVT-5's three readings paying off: Unknown and Inferred are not zero
    // skew, so they cannot produce a zero allowance. No watermark is offered.
    let doc = load();
    let c = case(
        &doc,
        "heuristic_cases",
        "★★_an_UNMEASURED_source_is_not_a_PUNCTUAL_one",
    );
    let log = vec![
        ev("no_arrival", 3_600_000, None),
        Event::backfilled("legacy1", "event.test.thing", 5_000_000, CausalStamp::new("n")),
    ];

    assert_eq!(
        estimate_heuristic(&log, tau(&doc), 95),
        Err(WatermarkError::NoObservedSkew {
            unusable: c["expect"]["unusable"].as_u64().unwrap() as usize
        })
    );
}

#[test]
fn a_wider_percentile_is_never_a_later_watermark() {
    let doc = load();
    let log = skew_log(&doc);
    let tight = estimate_heuristic(&log, tau(&doc), 50).unwrap();
    let wide = estimate_heuristic(&log, tau(&doc), 100).unwrap();
    assert!(wide.mark() <= tight.mark(), "more allowance can only be more conservative");
    assert!(case(&doc, "heuristic_cases", "a_wider_percentile_is_never_a_later_watermark")
        ["expect"]["wide_mark_le_tight_mark"]
        .as_bool()
        .unwrap());
}

// ── ★ the lateness policy is mandatory, and each cost is real ──────────────

/// A closed window under the given policy, emitting 100.
fn closed(doc: &Value, lateness: Lateness) -> WindowState {
    let mut st = WindowState::new(window(doc, lateness));
    let w = Watermark::perfect(tau(doc), guarantee(doc, 1));
    assert!(matches!(st.close(&w, json!(100)), Closing::Closed { .. }));
    st
}

/// A late event: inside the window, arriving after it closed.
fn late_event(doc: &Value) -> Event {
    ev("late", doc["fixture"]["window"]["end"].as_i64().unwrap() / 2, Some(tau(doc)))
}

#[test]
fn a_window_cannot_be_declared_without_a_lateness_policy() {
    // ★★ Structural: `Window::new` takes a `Lateness` and the type implements
    // no `Default`, so the compiler is what enforces this. The test says so.
    let doc = load();
    let c = case(
        &doc,
        "lateness_cases",
        "★★_a_window_CANNOT_BE_DECLARED_without_a_lateness_policy",
    );
    assert!(c["expect"]["lateness_is_an_argument"].as_bool().unwrap());

    let w = window(&doc, Lateness::Drop);
    assert_eq!(w.lateness(), Lateness::Drop);
    assert_eq!(
        Window::new(w.end(), w.end(), Lateness::Drop),
        Err(WindowError::Empty { start: w.end(), end: w.end() })
    );
}

#[test]
fn drop_keeps_the_answer_final_and_logs_the_fact_anyway() {
    let doc = load();
    let c = case(
        &doc,
        "lateness_cases",
        "★_drop_keeps_the_answer_final_and_LOGS_THE_FACT_ANYWAY",
    );
    let mut st = closed(&doc, Lateness::Drop);

    match st.admit_late(&late_event(&doc), json!(140)) {
        LateOutcome::Dropped { logged } => assert_eq!(logged.event_id, "late"),
        other => panic!("{other:?}"),
    }
    assert_eq!(st.revision() as u64, c["expect"]["revision"].as_u64().unwrap(), "no second answer");
    assert_eq!(
        st.late_log().len() as u64,
        c["expect"]["late_log"].as_u64().unwrap(),
        "dropped from the ANSWER, not from the record"
    );
    assert_eq!(st.emitted().is_some(), c["expect"]["retained_value"].as_bool().unwrap());
}

#[test]
fn late_fire_produces_a_second_answer_for_the_same_window() {
    let doc = load();
    let c = case(
        &doc,
        "lateness_cases",
        "★_late_fire_produces_a_SECOND_ANSWER_for_the_same_window",
    );
    let mut st = closed(&doc, Lateness::LateFire);

    match st.admit_late(&late_event(&doc), json!(140)) {
        LateOutcome::LateFired { revision, corrected, .. } => {
            assert_eq!(revision as u64, c["expect"]["revision"].as_u64().unwrap());
            assert_eq!(corrected, json!(140));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(st.emitted().is_some(), c["expect"]["retained_value"].as_bool().unwrap());
}

#[test]
fn retract_hands_downstream_both_the_old_value_and_the_new_one() {
    // ★★ The only policy under which a downstream aggregate stays correct
    // without recomputing — so the case does the arithmetic, not just the shape.
    let doc = load();
    let c = case(
        &doc,
        "lateness_cases",
        "★★_retract_hands_downstream_BOTH_the_old_value_and_the_new_one",
    );
    let mut st = closed(&doc, Lateness::AccumulateAndRetract);
    assert_eq!(st.emitted(), Some(&json!(100)), "the cost: it retained its answer");

    match st.admit_late(&late_event(&doc), json!(140)) {
        LateOutcome::Retracted { retracted, corrected, revision, .. } => {
            assert_eq!(retracted, json!(c["expect"]["retracted"].as_i64().unwrap()));
            assert_eq!(corrected, json!(c["expect"]["corrected"].as_i64().unwrap()));
            assert_eq!(revision, 2);

            let downstream_before = 100i64;
            let downstream_after =
                downstream_before - retracted.as_i64().unwrap() + corrected.as_i64().unwrap();
            assert_eq!(
                downstream_after,
                c["expect"]["downstream_after_subtracting"].as_i64().unwrap(),
                "subtract exactly what was added, then add the correction"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(st.emitted().is_some(), c["expect"]["retained_value"].as_bool().unwrap());
}

#[test]
fn a_drop_window_has_nothing_to_retract_even_by_mistake() {
    // ★ The expense §IV names is in the state, not only in the prose.
    let doc = load();
    let c = case(
        &doc,
        "lateness_cases",
        "★_a_DROP_window_has_nothing_to_retract_EVEN_BY_MISTAKE",
    );
    assert_eq!(
        closed(&doc, Lateness::Drop).emitted().is_some(),
        c["expect"]["drop_retains"].as_bool().unwrap()
    );
    assert_eq!(
        closed(&doc, Lateness::LateFire).emitted().is_some(),
        c["expect"]["late_fire_retains"].as_bool().unwrap()
    );
    assert_eq!(
        closed(&doc, Lateness::AccumulateAndRetract).emitted().is_some(),
        c["expect"]["retract_retains"].as_bool().unwrap()
    );
}

#[test]
fn an_event_in_an_open_window_is_not_late() {
    let doc = load();
    let mut st = WindowState::new(window(&doc, Lateness::Drop));
    assert_eq!(st.admit_late(&late_event(&doc), json!(1)), LateOutcome::NotLate);
    assert!(st.late_log().is_empty());
}

#[test]
fn the_window_is_half_open_so_no_event_is_counted_twice() {
    // ★ An event exactly at `b` belongs to the NEXT window — checked both by
    // containment and through the lateness path, where a boundary event
    // offered to the closed earlier window is not late, because it was never
    // that window's event.
    let doc = load();
    let c = case(
        &doc,
        "lateness_cases",
        "★_the_window_is_HALF_OPEN_so_no_event_is_counted_twice",
    );
    let first = window(&doc, Lateness::LateFire);
    let boundary = ev("boundary", first.end(), Some(tau(&doc)));
    let second = Window::new(first.end(), first.end() * 2, Lateness::Drop).unwrap();

    assert_eq!(first.contains(&boundary), c["expect"]["first_contains_boundary"].as_bool().unwrap());
    assert_eq!(
        second.contains(&boundary),
        c["expect"]["second_contains_boundary"].as_bool().unwrap()
    );

    let mut st = closed(&doc, Lateness::LateFire);
    assert_eq!(st.admit_late(&boundary, json!(1)), LateOutcome::NotLate);
    assert!(st.late_log().is_empty(), "and it is not recorded as this window's late fact");
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The counterweight: the reference HAS windows, and they close on τ.
    let cw = d["★★_the_counterweight_windows_DO_exist_there_and_they_close_on_W(τ)=τ_exactly"]
        .as_str()
        .unwrap();
    for named in ["calendar.py", "tasks.py", "protege.py", "window_end"] {
        assert!(cw.contains(named), "the counterweight must name the real window: {named}");
    }
    assert!(cw.contains("close on hope"), "and say what is actually wrong with them");

    // ★ The credit: the clock is injected, not ambient — not a purity defect.
    let credit = d["★_a_real_credit_the_clock_is_INJECTED_not_ambient"].as_str().unwrap();
    assert!(credit.contains("ctx.timestamp"));
    assert!(credit.contains("EVT-13"), "attributes the purity requirement it does meet");

    // ★ And the finding is not overstated: forward-looking windows bite least.
    assert!(d["★_the_forward_looking_windows_are_where_it_bites_least_and_that_is_worth_saying"]
        .as_str()
        .unwrap()
        .contains("EVT-11"));

    // The false positives, including the tracker's own misleading MON-4 row.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("SlidingWindowAggregator")));
    assert!(fp.values().any(|v| v.as_str().is_some_and(|s| s.contains("operating system"))));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "A GUARANTEE IS A DECLARATION AND CAN BE WRONG",
        "THE ESTIMATOR IS ONE ESTIMATOR",
        "NO EMISSION AND NO SCHEDULER",
        "NO RECURRENCE",
        "A RETRACTION IS OFFERED, NOT ENFORCED",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    // EVT-5 is credited as what made the estimate possible.
    assert!(doc["unblocked_by"].as_str().unwrap().contains("EVT-5"));

    let groups = ["wall_clock_cases", "monotonicity_cases", "heuristic_cases", "lateness_cases"];
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
