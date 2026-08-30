//! The two clocks and the skew between them, replayed against its conformance
//! vectors (R2 · Events and Time §III + §I · EVT-5, EVT-1).
//!
//! **SPEC vectors, Rust-only — with the INGEST half at parity.** The reference
//! records a real arrival time (`ingest_messages.received_at`) and a real
//! source (`source_id`); what it lacks is the other clock to subtract from it,
//! though its own transducer regexes capture the SMS's stated date and time in
//! thirteen patterns before discarding them.
//!
//! ★★ The load-bearing property here is that a **negative** skew survives —
//! §III requires it *"recorded and surfaced as a data-quality fact, never
//! silently clamped"* — and that surfacing takes an immutable slice, so
//! clamping is not expressible from the reporting path at all.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    clock_findings, fold_events, merge, order, skew_of, CausalStamp, Event, FoldEvent, Mutation,
    Provenance, Skew, Source, SourceKind, Suspect, Trust, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("clocks.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "clocks.json was written for a different contract version"
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

// ── the fixture, built from the vector rather than restated in Rust ─────────

fn kind_of(s: &str) -> SourceKind {
    match s {
        "connector" => SourceKind::Connector,
        "device" => SourceKind::Device,
        "enzyme" => SourceKind::Enzyme,
        "human" => SourceKind::Human,
        other => panic!("unknown source kind {other}"),
    }
}

fn trust_of(s: &str) -> Trust {
    match s {
        "authoritative" => Trust::Authoritative,
        "reported" => Trust::Reported,
        "derived" => Trust::Derived,
        other => panic!("unknown trust level {other}"),
    }
}

fn events(doc: &Value) -> Vec<Event> {
    doc["fixture"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Event {
            id: e["id"].as_str().unwrap().to_string(),
            name: "event.finances.pocket_spent".into(),
            t_event: e["t_event"].as_i64().unwrap(),
            t_ingest: e["t_ingest"].as_i64(),
            provenance: match e["provenance"].as_str().unwrap() {
                "observed" => Provenance::Observed,
                "legacy" => Provenance::Legacy,
                other => panic!("unknown provenance {other}"),
            },
            source: e["source"].as_object().map(|s| {
                Source::new(
                    s["id"].as_str().unwrap(),
                    kind_of(s["kind"].as_str().unwrap()),
                    trust_of(s["trust"].as_str().unwrap()),
                )
            }),
            stamp: CausalStamp::new("phone"),
            causes: vec![],
            mutations: vec![],
            payload: None,
        })
        .collect()
}

fn find(doc: &Value, id: &str) -> Event {
    events(doc).into_iter().find(|e| e.id == id).unwrap_or_else(|| panic!("no event {id}"))
}

// ── skew is normal, unbounded, and never rejected ──────────────────────────

#[test]
fn skew_is_the_gap_between_the_two_clocks() {
    let doc = load();
    let c = case(&doc, "skew_cases", "skew_is_the_gap_between_the_two_clocks");
    let s = skew_of(&find(&doc, c["event"].as_str().unwrap()));

    assert_eq!(s, Skew::Observed(c["expect"]["millis"].as_i64().unwrap()));
    assert!(s.is_normal());
}

#[test]
fn a_week_of_skew_is_accepted_and_read_exactly_like_a_millisecond_of_it() {
    // ★ §III's field case: the forwarding macro broke for a week, and the
    // correct behaviour was to ACCEPT the events. The assertion is not that a
    // week is tolerated but that nothing reads a threshold at all — there is
    // no error type in the module to return.
    let doc = load();
    let c = case(
        &doc,
        "skew_cases",
        "★_a_week_of_skew_is_ACCEPTED_and_read_exactly_like_a_millisecond_of_it",
    );
    let week = c["expect"]["millis"].as_i64().unwrap();
    assert_eq!(week, doc["fixture"]["WEEK"].as_i64().unwrap());

    let stale = skew_of(&find(&doc, c["event"].as_str().unwrap()));
    assert_eq!(stale, Skew::Observed(week));
    assert!(stale.is_normal(), "a week late is still the ordinary direction");

    // Read exactly like a punctual one: same variant, same accessor, no branch.
    let punctual = skew_of(&find(&doc, "sms1"));
    assert!(matches!(punctual, Skew::Observed(_)) && matches!(stale, Skew::Observed(_)));
    assert!(punctual.millis().is_some() && stale.millis().is_some());
}

#[test]
fn a_week_late_event_still_orders_by_when_it_happened() {
    // ★ The reason to carry both clocks rather than pick the better one — and
    // the check that the new field did not quietly become an ordering term.
    let doc = load();
    let mut log = events(&doc);
    log.retain(|e| e.id == "sms1" || e.id == "stranded");
    log.reverse(); // arrival order: the stranded one turned up last
    order(&mut log);

    let c = case(&doc, "skew_cases", "★_a_week_late_event_still_orders_by_WHEN_IT_HAPPENED");
    assert_eq!(log[0].id, c["expect"]["first_by_order"].as_str().unwrap());
    assert_eq!(log[1].id, "stranded", "it lands back in its own past");
}

// ── ★★ negative skew: surfaced, never clamped ──────────────────────────────

#[test]
fn a_negative_skew_survives_as_a_negative_number() {
    // ★★ THE LOAD-BEARING CASE. Clamping would satisfy every downstream
    // consumer and relocate the violation somewhere nobody is looking.
    let doc = load();
    let c = case(
        &doc,
        "negative_skew_cases",
        "★★_a_negative_skew_SURVIVES_AS_A_NEGATIVE_NUMBER",
    );
    let s = skew_of(&find(&doc, c["event"].as_str().unwrap()));

    let expected = c["expect"]["millis"].as_i64().unwrap();
    assert!(expected < 0, "the vector's own case must actually be negative");
    assert_eq!(s, Skew::Observed(expected));
    assert_eq!(s.millis(), Some(expected));
    assert!(s.is_negative());
    assert!(!s.is_normal());
}

#[test]
fn the_sign_survives_a_serialisation_boundary() {
    // ★ A boundary is where a clamp would most plausibly creep in.
    let doc = load();
    let c = case(&doc, "negative_skew_cases", "★_the_sign_survives_a_serialisation_boundary");
    let s = skew_of(&find(&doc, c["event"].as_str().unwrap()));

    let back: Skew = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back.millis(), Some(c["expect"]["millis"].as_i64().unwrap()));
    assert!(back.is_negative(), "a boundary crossing did not floor it");
}

#[test]
fn a_disagreement_is_surfaced_as_a_finding_not_merely_representable() {
    // ★★ Representing a negative skew and never mentioning it again would meet
    // the letter of "never clamped" and miss "surfaced as a data-quality fact".
    let doc = load();
    let c = case(
        &doc,
        "negative_skew_cases",
        "★★_it_is_SURFACED_as_a_finding_not_merely_representable",
    );
    let found = clock_findings(&events(&doc));

    assert_eq!(found.len(), c["expect"]["findings"].as_u64().unwrap() as usize);
    assert_eq!(found[0].event_id, c["expect"]["event_id"].as_str().unwrap());
    assert_eq!(found[0].skew, c["expect"]["skew"].as_i64().unwrap());
    assert_eq!(
        found[0].ahead_by,
        c["expect"]["ahead_by"].as_i64().unwrap(),
        "stated as its own positive number — the one a person reads"
    );
    assert!(found[0].describe().contains("never clamped"));
}

#[test]
fn surfacing_cannot_repair_because_it_has_no_mutable_access() {
    // ★★ The structural half. `clock_findings` takes `&[Event]`, so writing a
    // corrected time back is not expressible from the surfacing path — the
    // type makes the clamp unspellable rather than a reviewer catching it.
    let doc = load();
    let log = events(&doc);
    let before = log.clone();

    let found = clock_findings(&log);
    assert!(!found.is_empty(), "there is a real disagreement to report");
    assert_eq!(log, before, "the record is untouched — this reports, it cannot repair");
}

#[test]
fn an_authoritative_source_points_the_suspicion_at_the_receiver() {
    // ★ Which clock is wrong is a provenance question, and this is the one
    // thing that reads the declared trust level.
    let doc = load();
    let mut e = find(&doc, "fast_clock");
    e.source = Some(Source::new("mpesa", SourceKind::Connector, Trust::Authoritative));

    let f = &clock_findings(&[e])[0];
    assert_eq!(f.suspect(), Suspect::TheReceiver);
    assert!(f.describe().contains("RECEIVER"));
    assert_eq!(
        case(
            &doc,
            "negative_skew_cases",
            "★_an_authoritative_source_points_the_suspicion_at_the_RECEIVER"
        )["expect"]["suspect"]
            .as_str()
            .unwrap(),
        "TheReceiver"
    );
}

#[test]
fn a_merely_reporting_source_points_the_suspicion_at_itself() {
    let doc = load();
    let c = case(
        &doc,
        "negative_skew_cases",
        "a_merely_reporting_source_points_the_suspicion_at_ITSELF",
    );
    // The fixture's own device, at trust `reported` — the realistic case.
    let f = &clock_findings(&[find(&doc, c["event"].as_str().unwrap())])[0];
    assert_eq!(f.suspect(), Suspect::TheSource);
}

#[test]
fn with_no_source_declared_the_culprit_is_undetermined_not_guessed() {
    // ★ Naming either clock would be a guess dressed as a finding — and the
    // disagreement is still reported in full, with only the attribution held back.
    let doc = load();
    let mut e = find(&doc, "fast_clock");
    e.source = None;

    let found = clock_findings(&[e]);
    assert_eq!(found.len(), 1, "still reported");
    assert_eq!(found[0].suspect(), Suspect::Undetermined);
    assert_eq!(found[0].ahead_by, 2_400_000, "the quantity is not withheld");
}

// ── ★ the two readings that are deliberately not numbers ───────────────────

#[test]
fn an_absent_ingest_time_reads_unknown_and_not_zero() {
    // ★★ Collapsing this to 0 would put a fabricated punctual arrival into
    // every average taken over a migrated log.
    let doc = load();
    let c = case(
        &doc,
        "three_readings_cases",
        "★★_an_absent_ingest_time_reads_UNKNOWN_and_not_ZERO",
    );
    let s = skew_of(&find(&doc, c["event"].as_str().unwrap()));

    assert_eq!(s, Skew::Unknown);
    assert_eq!(s.millis(), None);
    assert!(!s.is_normal());
    assert_ne!(s, Skew::Observed(0));
}

#[test]
fn a_backfilled_row_reads_inferred_and_not_zero() {
    // ★★ The subtler one: the arithmetic genuinely IS zero, and that zero says
    // "we only ever had one number", not "it arrived instantly".
    let doc = load();
    let c = case(
        &doc,
        "three_readings_cases",
        "★★_a_BACKFILLED_row_reads_INFERRED_and_not_ZERO",
    );
    let e = find(&doc, c["event"].as_str().unwrap());
    assert_eq!(e.t_event, e.t_ingest.unwrap(), "the two clocks do read equal");

    let s = skew_of(&e);
    assert_eq!(s, Skew::Inferred);
    assert_ne!(s, Skew::Observed(0), "a backfill must not pass for punctual");
    assert_eq!(s.millis(), None);
}

#[test]
fn a_backfilled_row_is_never_reported_as_a_clock_disagreement() {
    // ★ Inventing a disagreement that was never observed would be as wrong as
    // hiding one that was.
    let doc = load();
    let c = case(
        &doc,
        "three_readings_cases",
        "★_a_backfilled_row_is_never_reported_as_a_clock_disagreement",
    );
    assert!(clock_findings(&[find(&doc, c["event"].as_str().unwrap())]).is_empty());
    assert_eq!(c["expect"]["findings_for_it"].as_u64().unwrap(), 0);
}

#[test]
fn backfill_marks_the_inferred_clock_and_leaves_the_observed_one_alone() {
    // Which of the two times the marker is about, stated rather than implied.
    let doc = load();
    let c = case(
        &doc,
        "three_readings_cases",
        "backfill_marks_the_INFERRED_clock_and_leaves_the_observed_one_alone",
    );
    let stamp = doc["fixture"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "legacy1")
        .unwrap()["t_event"]
        .as_i64()
        .unwrap();

    let built = Event::backfilled("legacy1", "event.finances.pocket_spent", stamp, CausalStamp::new("n"));
    assert_eq!(built.t_event, c["expect"]["t_event"].as_i64().unwrap());
    assert_eq!(built.t_ingest, Some(c["expect"]["t_ingest"].as_i64().unwrap()));
    assert_eq!(built.provenance, Provenance::Legacy);
    assert_eq!(skew_of(&built), Skew::Inferred);
}

// ── ★★ additive: nothing that already worked changed ───────────────────────

/// The exact wire shape of an event written before this slice — no `t_ingest`
/// key and no `source` key anywhere in it.
const PRE_SLICE_WIRE: &str = r#"{
    "id": "old1",
    "name": "event.finances.pocket_spent",
    "t_event": 1000,
    "provenance": "observed",
    "stamp": {"counter": 1, "node": "phone"},
    "causes": ["genesis"],
    "mutations": [{"op": "set", "path": "finances.liquid.balance", "old": 0, "new": 500}]
}"#;

#[test]
fn a_record_written_before_the_second_clock_still_deserialises() {
    let doc = load();
    let c = case(
        &doc,
        "additive_migration_cases",
        "★★_a_record_written_before_the_second_clock_still_DESERIALISES",
    );
    let e: Event = serde_json::from_str(PRE_SLICE_WIRE).expect("the old wire shape still reads");

    assert_eq!(e.t_ingest, None);
    assert_eq!(e.source, None);
    assert_eq!(e.causes, vec!["genesis".to_string()]);
    assert!(c["expect"]["causes_preserved"].as_bool().unwrap());
    assert_eq!(skew_of(&e), Skew::Unknown, "absent, which reads as unknown");
}

#[test]
fn the_new_fields_do_not_change_order_dedupe_or_the_fold() {
    // ★★ The property that could not regress: the sort key is `(t_event, id)`,
    // dedupe keys on the stable id, and the fold reads only `mutations`. So the
    // same log with and without the second clock — including a week of skew —
    // yields the identical sequence AND the identical folded state.
    let doc = load();
    let week = doc["fixture"]["WEEK"].as_i64().unwrap();

    let old: Event = serde_json::from_str(PRE_SLICE_WIRE).unwrap();
    let mut migrated = old.clone();
    migrated.t_ingest = Some(old.t_event + week);
    migrated.source = Some(Source::new("mpesa", SourceKind::Connector, Trust::Authoritative));

    assert_eq!(old.order_key(), migrated.order_key(), "the sort key is unchanged");

    let other = Event {
        id: "z".into(),
        name: "event.finances.pocket_spent".into(),
        t_event: 2000,
        t_ingest: None,
        provenance: Provenance::Observed,
        source: None,
        stamp: CausalStamp::new("phone"),
        causes: vec![],
        mutations: vec![Mutation::Set {
            path: "finances.liquid.balance".into(),
            old: serde_json::json!(500),
            new: serde_json::json!(700),
        }],
        payload: None,
    };

    let a = merge(vec![old.clone(), other.clone(), old.clone()]);
    let b = merge(vec![migrated, other, old]);
    assert_eq!(
        a.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        b.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        "same order after dedupe"
    );
    assert_eq!(a.len(), 2, "the repeat was dropped on the stable id, as before");

    // ★ And the rebuild: fold both logs from scratch and compare the states.
    let fold_of = |log: &[Event]| {
        let steps: Vec<FoldEvent> =
            log.iter().map(|e| FoldEvent { mutations: e.mutations.clone() }).collect();
        fold_events(&steps, None).expect("the log folds")
    };
    assert_eq!(fold_of(&a), fold_of(&b), "rebuilt state is byte-identical");

    let c = case(
        &doc,
        "additive_migration_cases",
        "★★_the_new_fields_do_not_change_ORDER_DEDUPE_or_the_FOLD",
    );
    for key in ["same_order", "same_dedupe", "same_folded_state"] {
        assert!(c["expect"][key].as_bool().unwrap());
    }
}

#[test]
fn a_source_records_which_connector_at_what_trust_level() {
    let doc = load();
    let c = case(
        &doc,
        "additive_migration_cases",
        "a_source_records_which_connector_at_what_trust_level",
    );
    let e = find(&doc, "sms1");
    let round: Event = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
    let s = round.source.expect("provenance that does not survive being written down is not provenance");

    assert_eq!(s.id, c["expect"]["id"].as_str().unwrap());
    assert_eq!(s.kind, kind_of(c["expect"]["kind"].as_str().unwrap()));
    assert_eq!(s.trust, trust_of(c["expect"]["trust"].as_str().unwrap()));
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ The counterweight: the ingest half and the source both exist there.
    let cw = d["★★_the_counterweight_the_INGEST_HALF_exists_at_the_app_layer_and_so_does_source"]
        .as_str()
        .unwrap();
    assert!(cw.contains("received_at"), "names the real column");
    assert!(cw.contains("source_id"), "names the real source field");

    // ★ And the sharper finding: the event time is captured, then discarded.
    let sharp = d["★★_and_the_event_time_is_CAPTURED_THEN_DISCARDED_which_is_sharper_than_absent"]
        .as_str()
        .unwrap();
    assert!(sharp.contains("13"), "names how many patterns capture it");
    assert!(sharp.contains("ING-13"), "attributes it to the tracker row that owns it");

    // ★ The near-miss: declared trust/provenance exists, on the wrong object.
    let near = d
        ["★_a_near_miss_the_reference_DOES_have_declared_trust_and_provenance_just_not_on_the_event"]
        .as_str()
        .unwrap();
    assert!(near.contains("ParseRule"));
    assert!(near.contains("Unapplied, not missing"));

    // The reference is not accused of clamping — it cannot compute the value.
    let terms = &d["term_by_term"];
    assert!(terms["★★ negative skew surfaced, never clamped"]
        .as_str()
        .unwrap()
        .contains("NOT because the reference clamps"));

    // The false positives are named so a re-grep does not mistake them.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("t_ingest")));
    assert!(fp.keys().any(|k| k.starts_with("t_event")));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "NO WATERMARK AND NO LATENESS POLICY",
        "NOTHING REJECTS A LATE EVENT",
        "DECLARED, never computed",
        "THE SUSPECT IS AN INFERENCE FROM A DECLARATION",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    // ★ The reconciliation done before building is recorded, not just done.
    let r = &doc["reconciliation_before_building"];
    assert!(r["★_causes_was_already_present_and_was_NOT_re_added"]
        .as_str()
        .unwrap()
        .contains("stale, not the code"));
    assert!(r["★_provenance_was_ALSO_already_present"]
        .as_str()
        .unwrap()
        .contains("did NOT build that distinction"));

    let groups = [
        "skew_cases",
        "negative_skew_cases",
        "three_readings_cases",
        "additive_migration_cases",
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
