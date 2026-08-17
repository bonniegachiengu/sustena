//! Periods — `RRULE` recurrence anchored to a zone id — replayed against its
//! conformance vectors (R2 · Events and Time §VIII · EVT-11).
//!
//! **SPEC vectors, Rust-only — and half of §VIII landed hours earlier.** EVT-6
//! built `W ≥ b` (*when we may answer*) and the bare half-open interval; this
//! builds `[a,b)` (*which events belong*), so the two halves of §VIII's "same
//! object seen twice" now meet in one type.
//!
//! ★★ The two requirements with teeth: **store the rule and recompute the
//! expansion**, and **anchor to a zone id, never a bare offset** — both for the
//! same reason, that the tz database is itself versioned. A case runs one rule
//! under two tzdata releases and gets two different answers.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    partitions, Anchor, CausalStamp, CivilDateTime, DstPolicy, Event, Freq, Lateness,
    LocalResolution, PeriodError, Provenance, Recurrence, SourceGuarantee, TzError, TzProvider,
    Watermark, Weekday, CONFORMANCE_VERSION,
};

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("period.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "period.json was written for a different contract version"
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

// ── the fixture providers, declared in the vector ───────────────────────────

/// A fixed-offset zone with a named release. Nairobi is EAT, no DST — §VIII's
/// vacuous local case — so the two releases differ only in the offset itself.
struct Fixed {
    tzid: &'static str,
    version: String,
    offset_ms: i64,
}

impl TzProvider for Fixed {
    fn tzdata_version(&self) -> String {
        self.version.clone()
    }
    fn offset_at(&self, tzid: &str, _l: CivilDateTime) -> Result<LocalResolution, TzError> {
        if tzid == self.tzid {
            Ok(LocalResolution::Unambiguous { offset_ms: self.offset_ms })
        } else {
            Err(TzError::UnknownZone { tzid: tzid.into(), version: self.tzdata_version() })
        }
    }
}

fn nairobi(doc: &Value, which: &str) -> Fixed {
    let p = &doc["fixture"]["providers"][which];
    Fixed {
        tzid: "Africa/Nairobi",
        version: p["version"].as_str().unwrap().to_string(),
        offset_ms: p["offset_hours"].as_i64().unwrap() * HOUR_MS,
    }
}

/// ★ The DST-observing counterparty. Europe/Berlin 2026: spring forward
/// 29 March 02:00→03:00, fall back 25 October 03:00→02:00.
struct Berlin;

impl TzProvider for Berlin {
    fn tzdata_version(&self) -> String {
        "2026a".into()
    }
    fn offset_at(&self, tzid: &str, l: CivilDateTime) -> Result<LocalResolution, TzError> {
        if tzid != "Europe/Berlin" {
            return Err(TzError::UnknownZone { tzid: tzid.into(), version: self.tzdata_version() });
        }
        if (l.year, l.month, l.day, l.hour) == (2026, 3, 29, 2) {
            return Ok(LocalResolution::Nonexistent {
                gap_ms: HOUR_MS,
                before_ms: HOUR_MS,
                after_ms: 2 * HOUR_MS,
            });
        }
        if (l.year, l.month, l.day, l.hour) == (2026, 10, 25, 2) {
            return Ok(LocalResolution::Ambiguous { earlier_ms: 2 * HOUR_MS, later_ms: HOUR_MS });
        }
        let at = (l.month, l.day, l.hour);
        let summer = ((3, 29, 3)..(10, 25, 3)).contains(&at);
        Ok(LocalResolution::Unambiguous { offset_ms: if summer { 2 * HOUR_MS } else { HOUR_MS } })
    }
}

fn dtstart(doc: &Value) -> CivilDateTime {
    let d = &doc["fixture"]["dtstart"];
    CivilDateTime::new(
        d["year"].as_i64().unwrap(),
        d["month"].as_u64().unwrap() as u32,
        d["day"].as_u64().unwrap() as u32,
        d["hour"].as_u64().unwrap() as u32,
        d["minute"].as_u64().unwrap() as u32,
        d["second"].as_u64().unwrap() as u32,
    )
}

/// The fixture's own rule: `FREQ=MONTHLY;BYMONTHDAY=1`, Nairobi.
fn rule(doc: &Value) -> Recurrence {
    assert_eq!(doc["fixture"]["rrule"].as_str().unwrap(), "FREQ=MONTHLY;BYMONTHDAY=1");
    Recurrence::monthly(
        dtstart(doc),
        doc["fixture"]["tzid"].as_str().unwrap(),
        1,
        Lateness::Drop,
        DstPolicy::ShiftForward,
    )
}

fn ymd(c: CivilDateTime) -> String {
    format!("{:04}-{:02}-{:02}", c.year, c.month, c.day)
}

fn berlin_daily(from: CivilDateTime, dst: DstPolicy) -> Recurrence {
    Recurrence {
        dtstart: from,
        anchor: Anchor::Zone { tzid: "Europe/Berlin".into() },
        freq: Freq::Daily,
        interval: 1,
        by_month_day: None,
        by_day: None,
        lateness: Lateness::Drop,
        dst,
    }
}

// ── ★ the partition ─────────────────────────────────────────────────────────

#[test]
fn a_year_is_the_sum_of_its_twelve_months_and_not_one_instant_more() {
    // ★★ Dijkstra (EWD831) at the roll-up correctness point.
    let doc = load();
    let c = case(
        &doc,
        "partition_cases",
        "★★_a_YEAR_is_the_SUM_OF_ITS_TWELVE_MONTHS_and_not_one_instant_more",
    );
    let p = rule(&doc).expand(&nairobi(&doc, "nairobi_2026a"), 12).unwrap();

    assert_eq!(p.len(), c["expect"]["periods"].as_u64().unwrap() as usize);
    assert!(partitions(&p), "each end IS the next start");

    let span = p[11].window.end() - p[0].window.start();
    let sum: i64 = p.iter().map(|q| q.window.end() - q.window.start()).sum();
    assert_eq!(sum, span, "the parts sum to the whole with nothing left over");
    assert_eq!(span, c["expect"]["span_days"].as_i64().unwrap() * DAY_MS);

    assert_eq!(ymd(p[0].start_local), c["expect"]["first_start_local"].as_str().unwrap());
    assert_eq!(ymd(p[11].end_local), c["expect"]["last_end_local"].as_str().unwrap());
}

#[test]
fn a_boundary_instant_belongs_to_exactly_one_month() {
    // ★★ "Not the months plus twelve boundary instants", asserted over every
    // boundary rather than argued once.
    let doc = load();
    let p = rule(&doc).expand(&nairobi(&doc, "nairobi_2026a"), 12).unwrap();
    let covered_end = p[11].window.end();

    let at = |t: i64| Event {
        id: "boundary".into(),
        name: "event.test.boundary".into(),
        t_event: t,
        t_ingest: None,
        provenance: Provenance::Observed,
        source: None,
        stamp: CausalStamp::new("n"),
        causes: vec![],
        mutations: vec![],
    };

    for q in &p {
        for t in [q.window.start(), q.window.end()] {
            let hits = p.iter().filter(|r| r.window.contains(&at(t))).count();
            assert!(hits <= 1, "an instant landed in two months at t={t}");
            if t < covered_end {
                assert_eq!(hits, 1, "an instant landed in no month at t={t}");
            } else {
                // Correct for a half-open partition: the final end belongs to
                // the thirteenth month, not to none.
                assert_eq!(hits, 0);
            }
        }
    }
}

#[test]
fn the_partition_is_structural_and_survives_dst_and_intervals() {
    let doc = load();
    // Across a real DST transition, where the days are 23 and 25 hours long.
    let spring = berlin_daily(CivilDateTime::date(2026, 3, 28), DstPolicy::ShiftForward)
        .expand(&Berlin, 3)
        .unwrap();
    assert!(partitions(&spring));

    let mut quarterly = rule(&doc);
    quarterly.interval = 3;
    assert!(partitions(&quarterly.expand(&nairobi(&doc, "nairobi_2026a"), 4).unwrap()));
}

// ── ★★ store the rule, recompute the expansion ─────────────────────────────

#[test]
fn the_same_rule_expands_differently_under_a_different_tzdata_release() {
    // ★★ THE REASON THE EXPANSION IS NOT STORED, run rather than described.
    let doc = load();
    let c = case(
        &doc,
        "store_the_rule_cases",
        "★★_THE_SAME_RULE_EXPANDS_DIFFERENTLY_under_a_different_tzdata_release",
    );
    let r = rule(&doc);
    let old = r.expand(&nairobi(&doc, "nairobi_2026a"), 1).unwrap();
    let new = r.expand(&nairobi(&doc, "nairobi_2027b"), 1).unwrap();

    assert_eq!(old[0].start_local, new[0].start_local, "the LOCAL time is identical");
    assert_ne!(old[0].window.start(), new[0].window.start(), "the instant is not");
    assert_eq!(
        new[0].window.start() - old[0].window.start(),
        c["expect"]["shift_hours"].as_i64().unwrap() * HOUR_MS
    );
    assert_eq!(old[0].tzdata_version.as_deref(), Some(c["expect"]["old_version"].as_str().unwrap()));
    assert_eq!(new[0].tzdata_version.as_deref(), Some(c["expect"]["new_version"].as_str().unwrap()));
}

#[test]
fn the_stored_rule_holds_no_instants() {
    // ★★ Structural rather than conventional, checked through serialisation
    // because the stored form is what would go in a database.
    let doc = load();
    let r = rule(&doc);
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains("window"), "no expanded instants ride along");
    assert!(!json.contains("tzdata_version"));

    let back: Recurrence = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
    let tz = nairobi(&doc, "nairobi_2026a");
    assert_eq!(back.expand(&tz, 3).unwrap(), r.expand(&tz, 3).unwrap());
}

#[test]
fn expansion_is_a_pure_function_of_the_rule_and_the_tz_data() {
    let doc = load();
    let r = rule(&doc);
    let tz = nairobi(&doc, "nairobi_2026a");
    assert_eq!(r.expand(&tz, 6).unwrap(), r.expand(&tz, 6).unwrap());
}

// ── ★★ zone id, not a bare offset ──────────────────────────────────────────

#[test]
fn a_zone_anchored_period_records_the_tzdata_that_derived_it() {
    let doc = load();
    let c = case(
        &doc,
        "zone_anchor_cases",
        "★_a_ZONE_ANCHORED_period_records_the_tzdata_that_derived_it",
    );
    let p = rule(&doc).expand(&nairobi(&doc, "nairobi_2026a"), 1).unwrap();
    assert_eq!(p[0].tzdata_version.as_deref(), Some(c["expect"]["tzdata_version"].as_str().unwrap()));
    assert_eq!(rule(&doc).tzid(), Some(c["expect"]["tzid"].as_str().unwrap()));
}

#[test]
fn a_fixed_offset_anchor_is_a_distinct_lesser_thing_not_a_silent_equal() {
    // ★★ Kept, because real data arrives carrying only an offset — but never
    // mistakable: no tz data consulted, no version to trace. And the trap is
    // SHOWN: it agrees today, and diverges under the revised release.
    let doc = load();
    let c = case(
        &doc,
        "zone_anchor_cases",
        "★★_a_FIXED_OFFSET_anchor_is_a_DISTINCT_LESSER_THING_not_a_silent_equal",
    );
    let offset_hours = doc["fixture"]["providers"]["nairobi_2026a"]["offset_hours"].as_i64().unwrap();

    let mut frozen = rule(&doc);
    frozen.anchor = Anchor::FixedOffset { minutes: (offset_hours * 60) as i32 };

    let f = frozen.expand(&nairobi(&doc, "nairobi_2026a"), 1).unwrap();
    assert!(c["expect"]["tzdata_version"].is_null());
    assert_eq!(f[0].tzdata_version, None, "untraceable, and it says so");
    assert_eq!(frozen.tzid(), None);

    let today = rule(&doc).expand(&nairobi(&doc, "nairobi_2026a"), 1).unwrap();
    assert_eq!(f[0].window.start(), today[0].window.start(), "agrees today — that is the trap");

    let revised = rule(&doc).expand(&nairobi(&doc, "nairobi_2027b"), 1).unwrap();
    assert_ne!(
        f[0].window.start(),
        revised[0].window.start(),
        "and the zone-anchored one moves while the frozen copy does not"
    );
}

#[test]
fn an_unknown_zone_is_refused_by_the_supplied_tz_data() {
    let doc = load();
    let mut r = rule(&doc);
    r.anchor = Anchor::Zone { tzid: "Mars/Olympus".into() };
    match r.expand(&nairobi(&doc, "nairobi_2026a"), 1) {
        Err(PeriodError::Tz(TzError::UnknownZone { version, .. })) => {
            assert_eq!(version, "2026a", "the error records WHICH release did not know it")
        }
        other => panic!("{other:?}"),
    }
}

// ── ★★ the DST machinery, exercised where it is not vacuous ────────────────

#[test]
fn the_dst_machinery_is_exercised_by_a_dst_observing_counterparty() {
    // ★★ §VIII: Nairobi's local case is vacuous, and the machinery must still
    // be tested. So it is tested in Berlin.
    let doc = load();
    let c = case(
        &doc,
        "dst_cases",
        "★★_the_DST_machinery_is_exercised_by_a_DST_OBSERVING_COUNTERPARTY",
    );
    let hours = |a: &sustena_core::Period| (a.window.end() - a.window.start()) / HOUR_MS;

    let spring = berlin_daily(CivilDateTime::date(2026, 3, 28), DstPolicy::ShiftForward)
        .expand(&Berlin, 3)
        .unwrap();
    assert_eq!(hours(&spring[0]), c["expect"]["neighbour_day_hours"].as_i64().unwrap());
    assert_eq!(hours(&spring[1]), c["expect"]["spring_forward_day_hours"].as_i64().unwrap());

    let autumn = berlin_daily(CivilDateTime::date(2026, 10, 24), DstPolicy::ShiftForward)
        .expand(&Berlin, 2)
        .unwrap();
    assert_eq!(hours(&autumn[1]), c["expect"]["fall_back_day_hours"].as_i64().unwrap());

    assert!(partitions(&spring) && partitions(&autumn), "short and long hour and all");
}

#[test]
fn a_nonexistent_local_start_is_refused_under_a_policy_that_says_so() {
    // ★ For a period BOUNDARY, which decides which events belong, an invented
    // instant silently reassigns data.
    let rule = berlin_daily(CivilDateTime::new(2026, 3, 29, 2, 30, 0), DstPolicy::Refuse);
    match rule.expand(&Berlin, 1) {
        Err(PeriodError::NonexistentLocalTime { gap_ms, .. }) => assert_eq!(gap_ms, HOUR_MS),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_ambiguous_local_start_resolves_by_the_declared_policy_never_a_default() {
    // ★★ Three declared policies, three real answers. `DstPolicy` implements
    // no `Default`, the same discipline EVT-6 applies to `Lateness`.
    let doc = load();
    let c = case(
        &doc,
        "dst_cases",
        "★★_an_AMBIGUOUS_local_start_resolves_by_the_DECLARED_policy_never_a_default",
    );
    let ambiguous = CivilDateTime::new(2026, 10, 25, 2, 30, 0);

    let earlier =
        berlin_daily(ambiguous, DstPolicy::UseEarlier).expand(&Berlin, 1).unwrap()[0].window.start();
    let later =
        berlin_daily(ambiguous, DstPolicy::UseLater).expand(&Berlin, 1).unwrap()[0].window.start();
    assert_eq!(
        later - earlier,
        c["expect"]["use_earlier_and_use_later_differ_by_hours"].as_i64().unwrap() * HOUR_MS,
        "genuinely two different instants"
    );

    assert!(matches!(
        berlin_daily(ambiguous, DstPolicy::Refuse).expand(&Berlin, 1),
        Err(PeriodError::AmbiguousLocalTime { .. })
    ));
}

// ── the grammar that IS shipped ────────────────────────────────────────────

#[test]
fn monthly_skips_a_month_with_no_such_day_rather_than_clamping() {
    // ★ RFC-5545 skips invalid dates. Clamping 31 Jan to 28 Feb would invent an
    // occurrence the rule never selected.
    let doc = load();
    let c = case(
        &doc,
        "grammar_cases",
        "★_MONTHLY_SKIPS_a_month_with_no_such_day_rather_than_clamping",
    );
    let r = Recurrence::monthly(
        CivilDateTime::date(2026, 1, 31),
        "Africa/Nairobi",
        31,
        Lateness::Drop,
        DstPolicy::ShiftForward,
    );
    let p = r.expand(&nairobi(&doc, "nairobi_2026a"), 3).unwrap();
    let want: Vec<&str> =
        c["expect"]["starts"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(p.iter().map(|q| ymd(q.start_local)).collect::<Vec<_>>(), want);
}

#[test]
fn weekly_byday_lands_on_the_declared_weekday() {
    let doc = load();
    let c = case(&doc, "grammar_cases", "weekly_byday_lands_on_the_declared_weekday");
    let r = Recurrence {
        dtstart: dtstart(&doc), // 2026-08-01, a Saturday
        anchor: Anchor::Zone { tzid: "Africa/Nairobi".into() },
        freq: Freq::Weekly,
        interval: 1,
        by_month_day: None,
        by_day: Some(Weekday::Mo),
        lateness: Lateness::Drop,
        dst: DstPolicy::ShiftForward,
    };
    let p = r.expand(&nairobi(&doc, "nairobi_2026a"), 3).unwrap();
    assert_eq!(ymd(p[0].start_local), c["expect"]["first_start_local"].as_str().unwrap());
    for q in &p {
        assert_eq!(
            q.window.end() - q.window.start(),
            c["expect"]["period_days"].as_i64().unwrap() * DAY_MS
        );
    }
}

#[test]
fn interval_spaces_the_periods_out_and_the_partition_survives_it() {
    let doc = load();
    let c = case(
        &doc,
        "grammar_cases",
        "interval_spaces_the_periods_out_and_the_partition_survives_it",
    );
    let mut r = rule(&doc);
    r.interval = 3;
    let p = r.expand(&nairobi(&doc, "nairobi_2026a"), 2).unwrap();
    assert_eq!(ymd(p[0].start_local), c["expect"]["first_start_local"].as_str().unwrap());
    assert_eq!(ymd(p[0].end_local), c["expect"]["first_end_local"].as_str().unwrap());
    assert!(partitions(&p));
}

#[test]
fn a_nonsense_rule_is_refused_at_expansion_rather_than_looping_or_guessing() {
    let doc = load();
    let tz = nairobi(&doc, "nairobi_2026a");

    let mut zero = rule(&doc);
    zero.interval = 0;
    assert_eq!(zero.expand(&tz, 1), Err(PeriodError::ZeroInterval));

    let mut bad_day = rule(&doc);
    bad_day.by_month_day = Some(32);
    assert_eq!(bad_day.expand(&tz, 1), Err(PeriodError::BadMonthDay(32)));

    let mut bad_start = rule(&doc);
    bad_start.dtstart = CivilDateTime::date(2026, 2, 30);
    assert!(matches!(bad_start.expand(&tz, 1), Err(PeriodError::BadDtStart(_))));
}

#[test]
fn zero_periods_is_an_empty_answer_not_an_error() {
    let doc = load();
    assert!(rule(&doc).expand(&nairobi(&doc, "nairobi_2026a"), 0).unwrap().is_empty());
}

// ── one primitive, many consumers ──────────────────────────────────────────

#[test]
fn a_period_hands_out_an_evt6_window_carrying_the_declared_lateness() {
    // ★ EVT-6's mandatory lateness rides through from the rule: because a
    // window cannot exist without a policy, neither can a recurring period.
    let doc = load();
    let mut r = rule(&doc);
    r.lateness = Lateness::AccumulateAndRetract;
    let p = r.expand(&nairobi(&doc, "nairobi_2026a"), 1).unwrap();
    assert_eq!(p[0].window.lateness(), Lateness::AccumulateAndRetract);
}

#[test]
fn a_period_closes_only_on_a_watermark_reaching_its_end() {
    // ★★ §VIII's "same object seen twice", completed and checked in one place.
    let doc = load();
    let p = rule(&doc).expand(&nairobi(&doc, "nairobi_2026a"), 1).unwrap();
    let w = &p[0].window;

    let short = Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", HOUR_MS));
    assert!(!w.is_closed_by(&short), "the boundary passing on the clock is not permission");
    let reached = Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", 0));
    assert!(w.is_closed_by(&reached));
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ The dependency decision is recorded, with §VIII's own argument for it.
    let dep = &doc["the_dependency_decision"];
    let decided = dep["★★_decided_INJECTED_not_bundled_and_§VIII's_own_argument_is_why"]
        .as_str()
        .unwrap();
    assert!(decided.contains("TzProvider"));
    assert!(decided.contains("invisibly"), "names what bundling would hide");
    assert!(dep["★_the_line_is_drawn_where_the_VERSIONING_is_not_where_the_difficulty_is"]
        .as_str()
        .unwrap()
        .contains("Hinnant"));
    assert!(dep["no_new_dependency_was_added"].as_str().unwrap().contains("unchanged"));

    // ★ The counterweight: half of §VIII already landed in EVT-6.
    assert!(d["★_the_counterweight_half_of_§VIII_ALREADY_LANDED_hours_ago_in_EVT_6"]
        .as_str()
        .unwrap()
        .contains("EVT-6"));

    // ★ And the reference's calendar is credited as real before being faulted.
    let cal = d["★_and_the_reference's_calendar_is_REAL_just_ad_hoc"].as_str().unwrap();
    assert!(cal.contains("calendar.py") && cal.contains("careful code"));
    assert!(d["★★_and_a_stored_instant_is_the_exact_failure_this_row_names"]
        .as_str()
        .unwrap()
        .contains("state.calendar.events"));

    // The false positives, including the real calendar and the UTC normalisation.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("calendar")));
    assert!(fp.keys().any(|k| k.starts_with("recurring")));
    assert!(fp.values().any(|v| v.as_str().is_some_and(|s| s.contains("discards"))));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE RRULE GRAMMAR IS PARTIAL",
        "NO RRULE TEXT PARSER",
        "THE TZ DATA IS THE HOST'S PROBLEM",
        "NO CONSUMER MIGRATED",
        "THE DST POLICY RESOLVES, IT DOES NOT PREDICT",
        "A FIXED-OFFSET ANCHOR IS KEPT",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }
    // The unshipped grammar is enumerated rather than waved at.
    for unshipped in ["COUNT", "UNTIL", "BYSETPOS", "EXDATE"] {
        assert!(limits.contains(unshipped), "the slot must name {unshipped}");
    }

    let groups = [
        "partition_cases",
        "store_the_rule_cases",
        "zone_anchor_cases",
        "dst_cases",
        "grammar_cases",
        "one_primitive_cases",
    ];
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
