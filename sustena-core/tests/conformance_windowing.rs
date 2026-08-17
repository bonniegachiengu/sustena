//! Stream-window typology, replayed against its conformance vectors
//! (R2 · Monitor §IV · MON-4).
//!
//! **SPEC vectors, Rust-only — and most of the substrate shipped hours
//! earlier.** EVT-6 built the half-open `Window`, the watermark that closes it
//! and the mandatory lateness policy; EVT-11 built the period partition
//! tumbling *is*. What this adds is **sliding** and **session**.
//!
//! ★★ Tumbling is not a third mechanism: a case proves a fixed-`T` sliding
//! windowing and an EVT-11 daily expansion produce **byte-identical** windows.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    sessionise, tumbling_from_recurrence, Anchor, CausalStamp, CivilDateTime, DstPolicy, Event,
    Freq, Lateness, LocalResolution, Provenance, Recurrence, SessionSpec, Sliding, SourceGuarantee,
    TzError, TzProvider, Typology, Watermark, WindowingError, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("windowing.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "windowing.json was written for a different contract version"
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

// ── the fixture ─────────────────────────────────────────────────────────────

fn ms(doc: &Value, key: &str) -> i64 {
    doc["fixture"][key].as_i64().unwrap()
}

fn ev(id: &str, t: i64) -> Event {
    Event {
        id: id.into(),
        name: "event.finances.pocket_spent".into(),
        t_event: t,
        t_ingest: None,
        provenance: Provenance::Observed,
        source: None,
        stamp: CausalStamp::new("phone"),
        causes: vec![],
        mutations: vec![],
    }
}

struct Utc;
impl TzProvider for Utc {
    fn tzdata_version(&self) -> String {
        "test".into()
    }
    fn offset_at(&self, _t: &str, _l: CivilDateTime) -> Result<LocalResolution, TzError> {
        Ok(LocalResolution::Unambiguous { offset_ms: 0 })
    }
}

fn daily_rule(doc: &Value) -> Recurrence {
    let d = doc["fixture"]["tumbling"]["dtstart"].as_array().unwrap();
    Recurrence {
        dtstart: CivilDateTime::date(
            d[0].as_i64().unwrap(),
            d[1].as_u64().unwrap() as u32,
            d[2].as_u64().unwrap() as u32,
        ),
        anchor: Anchor::Zone { tzid: "Etc/UTC".into() },
        freq: Freq::Daily,
        interval: 1,
        by_month_day: None,
        by_day: None,
        lateness: Lateness::Drop,
        dst: DstPolicy::ShiftForward,
    }
}

fn sliding(doc: &Value, lateness: Lateness) -> Sliding {
    let s = &doc["fixture"]["sliding"];
    Sliding::new(
        s["size_min"].as_i64().unwrap() * ms(doc, "MIN"),
        s["step_min"].as_i64().unwrap() * ms(doc, "MIN"),
        lateness,
    )
    .unwrap()
}

fn session_events(doc: &Value) -> Vec<Event> {
    doc["fixture"]["session"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            let a = e.as_array().unwrap();
            ev(a[0].as_str().unwrap(), a[1].as_i64().unwrap() * ms(doc, "MIN"))
        })
        .collect()
}

fn session_spec(doc: &Value, lateness: Lateness) -> SessionSpec {
    SessionSpec::new(
        doc["fixture"]["session"]["gap_min"].as_i64().unwrap() * ms(doc, "MIN"),
        lateness,
    )
    .unwrap()
}

// ── ★★ tumbling reuses EVT-11 ──────────────────────────────────────────────

#[test]
fn calendar_tumbling_is_a_delegate_to_evt11_and_keeps_its_correctness() {
    // ★★ A month is not a fixed number of milliseconds.
    let doc = load();
    let c = case(
        &doc,
        "tumbling_cases",
        "★★_CALENDAR_tumbling_is_a_DELEGATE_to_EVT_11_and_keeps_its_correctness",
    );
    let day = ms(&doc, "DAY");
    let monthly = Recurrence::monthly(
        CivilDateTime::date(2026, 1, 1),
        "Etc/UTC",
        1,
        Lateness::Drop,
        DstPolicy::ShiftForward,
    );
    let p = tumbling_from_recurrence(&monthly, &Utc, 3).unwrap();

    assert_eq!(
        (p[0].window.end() - p[0].window.start()) / day,
        c["expect"]["january_days"].as_i64().unwrap()
    );
    assert_eq!(
        (p[1].window.end() - p[1].window.start()) / day,
        c["expect"]["february_days"].as_i64().unwrap()
    );
    for pair in p.windows(2) {
        assert_eq!(pair[0].window.end(), pair[1].window.start());
    }
}

#[test]
fn a_fixed_duration_tumbling_is_the_degenerate_sliding_case() {
    // ★★ THE RECONCILIATION, measured: two routes, byte-identical windows, so
    // there is no third mechanism.
    let doc = load();
    let c = case(
        &doc,
        "tumbling_cases",
        "★★_a_FIXED_DURATION_tumbling_IS_the_degenerate_SLIDING_case",
    );
    let day = ms(&doc, "DAY");
    let count = doc["fixture"]["tumbling"]["count"].as_u64().unwrap() as usize;

    let from_periods = tumbling_from_recurrence(&daily_rule(&doc), &Utc, count).unwrap();
    let origin = from_periods[0].window.start();

    let s = Sliding::tumbling(day, Lateness::Drop).unwrap();
    let from_sliding = s.windows(origin, origin, origin + count as i64 * day).unwrap();

    assert_eq!(from_sliding.len(), c["expect"]["windows"].as_u64().unwrap() as usize);
    for (a, b) in from_periods.iter().zip(&from_sliding) {
        assert_eq!(&a.window, b, "the same windows, two routes, no third arithmetic");
    }
    assert_eq!(s.typology(), Typology::Tumbling);
    assert_eq!(s.overlap_factor() as u64, c["expect"]["overlap_factor"].as_u64().unwrap());
}

#[test]
fn a_tumbling_window_puts_each_event_in_exactly_one() {
    // Probed at a boundary too — the place a closed interval double-counts.
    let doc = load();
    let hour = ms(&doc, "HOUR");
    let s = Sliding::tumbling(hour, Lateness::Drop).unwrap();

    for t in [0, 1, hour - 1, hour, 3 * hour + 7] {
        assert_eq!(s.windows_containing(0, &ev("x", t)).unwrap().len(), 1, "at t={t}");
    }
}

// ── ★★ sliding overlaps ────────────────────────────────────────────────────

#[test]
fn a_sliding_window_overlaps_and_an_event_belongs_to_every_one_containing_it() {
    // ★★ The overlap is the point — deduping to one window would turn a rolling
    // rate into a tumbling one wearing the wrong name.
    let doc = load();
    let c = case(
        &doc,
        "sliding_cases",
        "★★_a_SLIDING_window_OVERLAPS_and_an_event_belongs_to_EVERY_one_containing_it",
    );
    let min = ms(&doc, "MIN");
    let s = sliding(&doc, Lateness::Drop);

    assert_eq!(s.typology(), Typology::Sliding);
    assert_eq!(s.overlap_factor() as u64, c["expect"]["overlap_factor"].as_u64().unwrap());

    let e = ev("a", 100 * min);
    let containing = s.windows_containing(0, &e).unwrap();
    assert_eq!(containing.len(), c["expect"]["containing"].as_u64().unwrap() as usize);
    for w in &containing {
        assert!(w.contains(&e));
        assert_eq!(
            (w.end() - w.start()) / min,
            c["expect"]["each_window_size_min"].as_i64().unwrap()
        );
    }
}

#[test]
fn a_step_larger_than_the_size_is_refused_because_it_would_drop_events() {
    // ★★ Gaps between windows mean events belonging to none.
    let doc = load();
    let min = ms(&doc, "MIN");
    assert_eq!(
        Sliding::new(10 * min, 20 * min, Lateness::Drop),
        Err(WindowingError::StepExceedsSize { size: 10 * min, step: 20 * min })
    );
    assert!(matches!(Sliding::new(0, 1, Lateness::Drop), Err(WindowingError::BadSize(0))));
    assert!(matches!(Sliding::new(10, 0, Lateness::Drop), Err(WindowingError::BadStep(0))));
}

#[test]
fn sliding_windows_are_anchored_to_a_declared_origin() {
    let doc = load();
    let (min, hour) = (ms(&doc, "MIN"), ms(&doc, "HOUR"));
    let s = Sliding::tumbling(hour, Lateness::Drop).unwrap();

    let a = s.windows(0, 0, 3 * hour).unwrap();
    let b = s.windows(30 * min, 0, 3 * hour).unwrap();
    assert_ne!(a[0].start(), b[0].start());
    assert_eq!(b[0].start(), 30 * min);
}

// ── ★★ session: the boundary is in the data ────────────────────────────────

#[test]
fn a_gap_larger_than_tau_closes_a_session_and_a_smaller_one_does_not() {
    // ★★ The end belongs to the session that ended, not to the event that
    // revealed it had — so it lands τ after its own last event.
    let doc = load();
    let c = case(
        &doc,
        "session_cases",
        "★★_a_gap_LARGER_than_τ_closes_a_session_and_a_smaller_one_does_not",
    );
    let min = ms(&doc, "MIN");
    let sessions = sessionise(&session_events(&doc), &session_spec(&doc, Lateness::Drop)).unwrap();

    assert_eq!(sessions.len(), c["expect"]["sessions"].as_u64().unwrap() as usize);
    let ids = |k: &str| -> Vec<String> {
        c["expect"][k].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect()
    };
    assert_eq!(sessions[0].event_ids(), ids("first_ids").as_slice());
    assert_eq!(sessions[1].event_ids(), ids("second_ids").as_slice());

    let first = sessions[0].window().expect("the gap after it was observed");
    assert_eq!(first.start(), 0);
    assert_eq!(first.end() / min, c["expect"]["first_end_min"].as_i64().unwrap());
}

#[test]
fn the_last_session_is_open_because_no_gap_has_been_observed_after_it() {
    // ★★ Not an empty session and not a closed one.
    let doc = load();
    let c = case(
        &doc,
        "session_cases",
        "★★_the_LAST_session_is_OPEN_because_no_gap_has_been_observed_after_it",
    );
    let min = ms(&doc, "MIN");
    let sessions =
        sessionise(&[ev("a", 0), ev("b", 10 * min)], &session_spec(&doc, Lateness::Drop)).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].is_open(), c["expect"]["is_open"].as_bool().unwrap());
    assert!(c["expect"]["window"].is_null());
    assert!(sessions[0].window().is_none());
}

#[test]
fn an_empty_batch_yields_no_sessions_rather_than_an_empty_one() {
    let doc = load();
    assert!(sessionise(&[], &session_spec(&doc, Lateness::Drop)).unwrap().is_empty());
}

// ── ★★ everything closes on a watermark, nothing on a clock ────────────────

#[test]
fn an_open_session_closes_only_when_a_watermark_passes_its_gap() {
    // ★★ THE GUARANTEE at the one place it could have been lost. A clock is not
    // evidence.
    let doc = load();
    let c = case(
        &doc,
        "watermark_cases",
        "★★_an_OPEN_session_closes_only_when_a_WATERMARK_passes_its_gap",
    );
    let (min, hour) = (ms(&doc, "MIN"), ms(&doc, "HOUR"));
    let spec = session_spec(&doc, Lateness::Drop);
    let sessions = sessionise(&[ev("a", 0), ev("b", 10 * min)], &spec).unwrap();
    let open = &sessions[0];

    let late_clock = Watermark::perfect(2 * hour, SourceGuarantee::new("mpesa", 90 * min));
    assert_eq!(
        open.close_on(&late_clock, &spec).unwrap().is_some(),
        c["expect"]["late_clock_closes"].as_bool().unwrap(),
        "the clock ran on; the guarantee did not"
    );

    let early = Watermark::perfect(20 * min, SourceGuarantee::new("mpesa", 15 * min));
    assert_eq!(
        open.close_on(&early, &spec).unwrap().is_some(),
        c["expect"]["early_watermark_closes"].as_bool().unwrap()
    );

    let reached = Watermark::perfect(40 * min, SourceGuarantee::new("mpesa", 0));
    let closed = open.close_on(&reached, &spec).unwrap().expect("the gap is guaranteed");
    assert_eq!(closed.end() / min, c["expect"]["closed_end_min"].as_i64().unwrap());
}

#[test]
fn a_sliding_window_closes_only_on_a_watermark_because_it_is_an_evt6_window() {
    // ★ Inherited rather than re-implemented — these ARE `Window`s.
    let doc = load();
    let min = ms(&doc, "MIN");
    let s = sliding(&doc, Lateness::Drop);
    let w = &s.windows(0, 0, ms(&doc, "HOUR")).unwrap()[0];

    let hopeful = Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", 15 * min));
    assert!(!w.is_closed_by(&hopeful), "the clock reaching the end is not permission");
    assert!(w.is_closed_by(&Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", 0))));
}

#[test]
fn the_declared_lateness_rides_through_every_window_the_typology_produces() {
    // ★ Checked on a session too, since it builds its `Window` at a different
    // moment and could have dropped the policy on the way.
    let doc = load();
    let (min, hour) = (ms(&doc, "MIN"), ms(&doc, "HOUR"));

    let s = sliding(&doc, Lateness::AccumulateAndRetract);
    for w in s.windows(0, 0, 2 * hour).unwrap() {
        assert_eq!(w.lateness(), Lateness::AccumulateAndRetract);
    }

    let spec = session_spec(&doc, Lateness::LateFire);
    let sessions = sessionise(&[ev("a", 0), ev("b", 2 * hour)], &spec).unwrap();
    assert_eq!(sessions[0].window().unwrap().lateness(), Lateness::LateFire);
    assert_eq!(min, 60_000, "the fixture's own unit, so the numbers above are minutes");
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ Tumbling reuses EVT-11 and fixed-T tumbling is the degenerate case.
    let r = &doc["reconciliation_before_building"];
    assert!(r["★★_tumbling_reuses_EVT_11_rather_than_duplicating_it"]
        .as_str()
        .unwrap()
        .contains("no tumbling boundary arithmetic"));
    assert!(r["★★_and_fixed_duration_tumbling_is_the_DEGENERATE_SLIDING_case"]
        .as_str()
        .unwrap()
        .contains("byte-identical"));

    // ★★ The correction to this row's own text, carried forward.
    let fix = d["★★_a_correction_to_MON_4's_OWN_ROW_carried_forward"].as_str().unwrap();
    assert!(fix.contains("SlidingWindowAggregator"));
    assert!(fix.contains("grep-0"));
    assert!(fix.contains("on either side"), "and it is unbuilt on BOTH sides, deliberately");

    // ★ The counterweight: most of the substrate shipped hours earlier.
    let key = "★_the_counterweight_the_SUBSTRATE_is_at_parity_and_most_of_it_shipped_HOURS_ago";
    let cw = d[key].as_str().unwrap();
    assert!(cw.contains("EVT-6") && cw.contains("EVT-11"));

    // ★ And the reference's real windows are credited as real, then distinguished.
    assert!(d["★_and_the_reference_has_REAL_windows_that_are_none_of_the_three"]
        .as_str()
        .unwrap()
        .contains("windowing *code* and no windowing *typology*"));

    // The false positive that will surface on any re-grep.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("session")));
    assert!(fp.values().any(|v| v.as_str().is_some_and(|s| s.contains("operating system"))));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "NO AGGREGATOR IS BUILT",
        "SUB-DAILY FIXED-`T` TUMBLING GOES THROUGH `Sliding`",
        "SESSIONS READ THE ORDER THEY ARE HANDED",
        "NO SESSION MERGING ACROSS BATCHES",
        "NOT A TIMEOUT",
        "NOT WIRED INTO `MonitorEngine`",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = ["tumbling_cases", "sliding_cases", "session_cases", "watermark_cases"];
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
