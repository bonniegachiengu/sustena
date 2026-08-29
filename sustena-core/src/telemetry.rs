//! **Metrics and traces, as folds over the log** (Monitor · §III).
//!
//! ```text
//!   Logs     — the append-only event log            (EVT, already the substrate)
//!   Metrics  — a fold over it                       (here)
//!   Traces   — the causal DAG it already carries    (here)
//! ```
//!
//! ★★★ **The three pillars are conventionally three pipelines, and here that
//! would be the same mistake the state cache was.** An industry metrics store is
//! written *beside* the log by the same code that writes the log, which means it
//! can disagree with it — a counter that says forty and a log that holds
//! thirty-nine, and no way to tell which is wrong. This project has already paid
//! that bill once (`state = fold(events)`), and paying it again for telemetry
//! would be choosing to.
//!
//! ★★★ **So nothing here is stored.** A metric is computed from the events on
//! every read, and a trace is the `causes` graph that the log already is. They
//! cannot drift, because there is nothing to drift *from*: no second write path
//! exists to be inconsistent with the first.
//!
//! The cost is real and worth naming — every read is a scan, where a stored
//! counter is an increment. That is the right trade at a household's volume and
//! it is the wrong one at a million events a second; when it stops being right
//! the answer is a *cache* that is provably a fold, not a second writer.
//!
//! ## What a span is, and what it cannot be
//!
//! ★★★ **The log records instants, not intervals — so spans have no duration,
//! and inventing one would be a fabricated measurement.** OpenTelemetry's `Span`
//! carries `start` and `end`; an event carries `t_event` and nothing else about
//! how long anything took. The available honest reading is the *elapsed time
//! along a causal edge* — from a cause to its effect — which is a real number
//! about the system, and is not the same thing as "how long the operator ran".
//! A `Span::duration_ms` does not exist here for that reason.

use std::collections::{BTreeMap, BTreeSet};

use crate::event::Event;

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// One reading: `(name, value, t, labels)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    /// Event time, not read time. A metric read twice about the same past
    /// reports the same instant both times.
    pub at: i64,
    pub labels: BTreeMap<String, String>,
}

/// Which field of an event a label is drawn from.
///
/// ★★★ **An enum rather than a string path, and that is the cardinality
/// defence.** Unbounded label values are the classic way a metrics system falls
/// over, and the unbounded values here would be payload text — a merchant name,
/// a description someone typed. Naming the legal sources exhaustively means a
/// free-text label cannot be declared by mistake; it would have to be added to
/// this enum, deliberately, by somebody who saw this paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LabelOf {
    /// The event's own name. Bounded by the operator registry.
    EventName,
    /// Which device or feed reported it. Bounded by the fleet — large, not
    /// unbounded, and the one worth watching.
    Source,
    /// Observed, inferred or legacy. Three values.
    Provenance,
}

impl LabelOf {
    fn key(self) -> &'static str {
        match self {
            Self::EventName => "event",
            Self::Source => "source",
            Self::Provenance => "provenance",
        }
    }

    fn read(self, e: &Event) -> String {
        match self {
            Self::EventName => e.name.clone(),
            // ★★ A source that was never recorded reads as "unknown", not as a
            //    missing series. Dropping the event would make an old log look
            //    quieter than it was.
            Self::Source => e
                .source
                .as_ref()
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|| "unknown".into()),
            Self::Provenance => format!("{:?}", e.provenance).to_lowercase(),
        }
    }
}

/// What is being counted or summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reduce {
    /// How many events matched.
    Count,
    /// How many state mutations they carried between them.
    ///
    /// ★★ A real quantity rather than a stand-in for one: an event that changed
    /// nothing and an event that changed four things are different weights of
    /// work, and a count alone cannot tell them apart.
    Mutations,
}

/// A declared metric. Nothing is measured that nobody asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricSpec {
    pub name: String,
    /// Event-name prefix this metric is about. Empty matches everything.
    ///
    /// ★★ A prefix rather than an exact name, because the dot protocol is
    /// hierarchical: `finances` should be able to mean every `finances.*`
    /// without restating the list every time one is added.
    pub about: String,
    pub reduce: Reduce,
    pub labels: Vec<LabelOf>,
}

impl MetricSpec {
    pub fn new(name: &str, about: &str, reduce: Reduce) -> Self {
        Self { name: name.into(), about: about.into(), reduce, labels: vec![] }
    }

    pub fn labelled_by(mut self, by: LabelOf) -> Self {
        self.labels.push(by);
        self
    }

    fn matches(&self, e: &Event) -> bool {
        self.about.is_empty()
            || e.name == self.about
            || e.name.starts_with(&format!("{}.", self.about))
    }
}

/// **Compute a metric's series from the log.**
///
/// ★★★ A fold, evaluated on read. There is no counter anywhere to be wrong.
///
/// One [`Metric`] per distinct label combination, ordered by labels so two runs
/// over the same log produce byte-identical output — a report that reorders
/// itself between reads is a report people stop trusting.
pub fn series(spec: &MetricSpec, events: &[Event]) -> Vec<Metric> {
    let mut buckets: BTreeMap<BTreeMap<String, String>, (f64, i64)> = BTreeMap::new();
    for e in events.iter().filter(|e| spec.matches(e)) {
        let labels: BTreeMap<String, String> =
            spec.labels.iter().map(|l| (l.key().to_string(), l.read(e))).collect();
        let add = match spec.reduce {
            Reduce::Count => 1.0,
            Reduce::Mutations => e.mutations.len() as f64,
        };
        let slot = buckets.entry(labels).or_insert((0.0, i64::MIN));
        slot.0 += add;
        slot.1 = slot.1.max(e.t_event);
    }
    buckets
        .into_iter()
        .map(|(labels, (value, at))| Metric { name: spec.name.clone(), value, at, labels })
        .collect()
}

/// How many distinct series this metric produces over this log.
///
/// ★★★ The number that says whether a label was a mistake. Cardinality failures
/// are not caught by a metric being *wrong* — every series is individually
/// correct — they are caught by there being fifty thousand of them, so the count
/// has to be askable.
pub fn cardinality(spec: &MetricSpec, events: &[Event]) -> usize {
    series(spec, events).len()
}

// ---------------------------------------------------------------------------
// Traces
// ---------------------------------------------------------------------------

/// One node of a trace.
///
/// ★★★ **No `duration`.** The log records instants; a per-span duration would
/// have to be invented, and an invented measurement is worse than an absent one
/// because it looks like evidence. See [`Trace::elapsed_ms`] for the reading
/// that is genuinely available.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub event_id: String,
    pub name: String,
    pub at: i64,
    /// ★★★ **Parents, plural.** An event may be caused by several, and picking
    /// the first to force a tree would invent a causality nobody recorded.
    pub causes: Vec<String>,
}

/// A causal DAG over one window of the log.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    pub spans: Vec<Span>,
    /// Spans with no cause inside this window.
    pub roots: Vec<String>,
    /// ★★★ Spans naming a cause that is **not in this window**.
    ///
    /// Reported rather than reparented: a window boundary is not an origin, and
    /// silently making these roots would turn "we did not fetch far enough back"
    /// into "this is where it started".
    pub orphans: Vec<(String, String)>,
}

/// **Build the trace the log already is.**
///
/// ★★ Not a collector, not an instrumentation API — `causes` was recorded when
/// the events were written, so this reads rather than gathers. A `TraceCollector`
/// in the OpenTelemetry sense would be a second recording of a fact the log
/// already holds, and the two could disagree.
pub fn trace(events: &[Event]) -> Trace {
    let present: BTreeSet<&str> = events.iter().map(|e| e.id.as_str()).collect();
    let mut spans = Vec::new();
    let mut roots = Vec::new();
    let mut orphans = Vec::new();

    for e in events {
        for cause in &e.causes {
            if !present.contains(cause.as_str()) {
                orphans.push((e.id.clone(), cause.clone()));
            }
        }
        if e.causes.is_empty() {
            roots.push(e.id.clone());
        }
        spans.push(Span {
            event_id: e.id.clone(),
            name: e.name.clone(),
            at: e.t_event,
            causes: e.causes.clone(),
        });
    }
    Trace { spans, roots, orphans }
}

impl Trace {
    pub fn get(&self, event_id: &str) -> Option<&Span> {
        self.spans.iter().find(|s| s.event_id == event_id)
    }

    /// Everything caused, directly or transitively, by `event_id`.
    ///
    /// ★★ Bounded by the span count rather than by recursion depth, so a
    /// malformed window that does contain a cycle terminates and reports what it
    /// reached instead of overflowing the stack.
    pub fn descendants(&self, event_id: &str) -> Vec<&Span> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut frontier = vec![event_id];
        let mut out = Vec::new();
        while let Some(id) = frontier.pop() {
            for s in self.spans.iter().filter(|s| s.causes.iter().any(|c| c == id)) {
                if seen.insert(s.event_id.as_str()) {
                    out.push(s);
                    frontier.push(s.event_id.as_str());
                }
            }
        }
        out.sort_by(|a, b| (a.at, &a.event_id).cmp(&(b.at, &b.event_id)));
        out
    }

    /// **Elapsed time from a cause to its furthest effect.**
    ///
    /// ★★★ The reading that genuinely exists, and it is not "how long it took to
    /// run". It is how long the world took to finish reacting — a capture at
    /// 11:15 whose last consequence lands at 11:18 took three minutes to settle,
    /// and no part of that claims to be CPU time.
    pub fn elapsed_ms(&self, event_id: &str) -> Option<i64> {
        let start = self.get(event_id)?.at;
        let last = self.descendants(event_id).iter().map(|s| s.at).max().unwrap_or(start);
        Some(last - start)
    }

    /// Is this window acyclic?
    ///
    /// ★★ Asked rather than assumed. Lamport stamps make a cycle very unlikely
    /// and "unlikely" is not "impossible", and every traversal here is a loop
    /// that would otherwise not terminate.
    pub fn is_acyclic(&self) -> bool {
        self.spans.iter().all(|s| !self.descendants(&s.event_id).iter().any(|d| d.event_id == s.event_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CausalStamp, Provenance};
    use crate::mutation::Mutation;
    use serde_json::json;

    fn stamp() -> CausalStamp {
        CausalStamp::new("node-a")
    }

    fn ev(id: &str, name: &str, at: i64) -> Event {
        Event {
            id: id.into(),
            name: name.into(),
            t_event: at,
            t_ingest: Some(at),
            provenance: Provenance::Observed,
            source: None,
            stamp: stamp(),
            causes: vec![],
            mutations: vec![],
        }
    }

    fn caused_by(mut e: Event, causes: &[&str]) -> Event {
        e.causes = causes.iter().map(|c| c.to_string()).collect();
        e
    }

    fn moving(mut e: Event, n: usize) -> Event {
        e.mutations = (0..n)
            .map(|i| Mutation::Set { path: format!("a.b{i}"), old: json!(null), new: json!(i) })
            .collect();
        e
    }

    fn log() -> Vec<Event> {
        vec![
            moving(ev("e1", "finances.pocket_spent", 100), 2),
            moving(ev("e2", "finances.pocket_spent", 200), 1),
            ev("e3", "system.backfill_declared", 300),
        ]
    }

    #[test]
    fn a_metric_is_a_fold_and_has_no_counter_to_be_wrong() {
        // ★★★ Read twice over the same log, identical both times — because
        //     there is no second write path to be inconsistent with the first.
        let spec = MetricSpec::new("spends", "finances", Reduce::Count);
        assert_eq!(series(&spec, &log()), series(&spec, &log()));
        assert_eq!(series(&spec, &log())[0].value, 2.0);
    }

    #[test]
    fn a_metric_moves_when_the_log_moves_and_nothing_else_is_written() {
        let spec = MetricSpec::new("spends", "finances", Reduce::Count);
        let mut longer = log();
        longer.push(ev("e4", "finances.pocket_spent", 400));
        assert_eq!(series(&spec, &longer)[0].value, 3.0);
    }

    #[test]
    fn a_prefix_covers_the_dot_hierarchy_without_restating_it() {
        // ★★ A metric about `finances` should not need editing every time a
        //    `finances.*` event is added.
        let spec = MetricSpec::new("all-finance", "finances", Reduce::Count);
        let mut l = log();
        l.push(ev("e5", "finances.income_recorded", 500));
        assert_eq!(series(&spec, &l)[0].value, 3.0);
    }

    #[test]
    fn counting_events_and_weighing_what_they_changed_are_different_questions() {
        // ★★ An event that changed nothing and one that changed four things are
        //    different weights of work, and a count cannot tell them apart.
        let l = log();
        let count = series(&MetricSpec::new("n", "finances", Reduce::Count), &l);
        let work = series(&MetricSpec::new("n", "finances", Reduce::Mutations), &l);
        assert_eq!(count[0].value, 2.0);
        assert_eq!(work[0].value, 3.0);
    }

    #[test]
    fn labels_split_the_series_and_the_split_is_stable() {
        // ★★ Ordered output: a report that reorders itself between reads is one
        //    people stop trusting.
        let spec =
            MetricSpec::new("by-event", "", Reduce::Count).labelled_by(LabelOf::EventName);
        let s = series(&spec, &log());
        assert_eq!(s.len(), 2);
        assert_eq!(s, series(&spec, &log()));
        assert_eq!(s[0].labels["event"], "finances.pocket_spent");
    }

    #[test]
    fn an_event_that_never_said_where_it_came_from_is_unknown_not_absent() {
        // ★★ Dropping it would make an old log look quieter than it was.
        let spec = MetricSpec::new("by-source", "", Reduce::Count).labelled_by(LabelOf::Source);
        let s = series(&spec, &log());
        assert_eq!(s[0].labels["source"], "unknown");
    }

    #[test]
    fn how_many_series_a_label_produces_is_askable() {
        // ★★★ Cardinality failures are not caught by a metric being wrong —
        //     every series is individually correct. They are caught by there
        //     being fifty thousand of them.
        let plain = MetricSpec::new("n", "", Reduce::Count);
        let split = MetricSpec::new("n", "", Reduce::Count).labelled_by(LabelOf::EventName);
        assert_eq!(cardinality(&plain, &log()), 1);
        assert_eq!(cardinality(&split, &log()), 2);
    }

    #[test]
    fn a_metrics_timestamp_is_event_time_not_read_time() {
        // ★★ A metric read twice about the same past reports the same instant.
        let s = series(&MetricSpec::new("n", "finances", Reduce::Count), &log());
        assert_eq!(s[0].at, 200);
    }

    #[test]
    fn the_trace_is_the_log_not_a_second_recording_of_it() {
        // ★★ `causes` was written when the events were. A collector would be a
        //    second recording of a fact the log already holds.
        let l = vec![
            ev("a", "capture", 100),
            caused_by(ev("b", "spend", 160), &["a"]),
            caused_by(ev("c", "rollup", 280), &["b"]),
        ];
        let t = trace(&l);
        assert_eq!(t.roots, vec!["a"]);
        assert_eq!(t.descendants("a").len(), 2);
    }

    #[test]
    fn an_event_with_several_causes_keeps_all_of_them() {
        // ★★★ Picking the first to force a tree would invent a causality
        //     nobody recorded.
        let l = vec![
            ev("a", "capture", 100),
            ev("b", "capture", 110),
            caused_by(ev("c", "reconcile", 200), &["a", "b"]),
        ];
        let t = trace(&l);
        assert_eq!(t.get("c").unwrap().causes.len(), 2);
        assert_eq!(t.roots, vec!["a", "b"]);
        assert_eq!(t.descendants("a")[0].event_id, "c");
        assert_eq!(t.descendants("b")[0].event_id, "c");
    }

    #[test]
    fn a_cause_outside_the_window_is_reported_not_turned_into_a_root() {
        // ★★★ A window boundary is not an origin. Reparenting would turn "we
        //     did not fetch far enough back" into "this is where it started".
        let t = trace(&[caused_by(ev("b", "spend", 160), &["a-fetched-earlier"])]);
        assert!(t.roots.is_empty(), "an orphan is not a root");
        assert_eq!(t.orphans, vec![("b".to_string(), "a-fetched-earlier".to_string())]);
    }

    #[test]
    fn elapsed_is_how_long_the_world_took_to_settle_not_how_long_it_ran() {
        // ★★★ The log records instants. A per-span duration would have to be
        //     invented, and an invented measurement looks like evidence.
        let l = vec![
            ev("a", "capture", 100),
            caused_by(ev("b", "spend", 160), &["a"]),
            caused_by(ev("c", "rollup", 280), &["b"]),
        ];
        assert_eq!(trace(&l).elapsed_ms("a"), Some(180));
    }

    #[test]
    fn something_nothing_followed_took_no_time_to_settle() {
        let t = trace(&[ev("a", "capture", 100)]);
        assert_eq!(t.elapsed_ms("a"), Some(0));
        assert_eq!(t.elapsed_ms("nobody"), None);
    }

    #[test]
    fn a_malformed_window_terminates_and_reports_rather_than_overflowing() {
        // ★★ Lamport stamps make a cycle very unlikely, and unlikely is not
        //    impossible — every traversal here is a loop that would otherwise
        //    not terminate.
        let l = vec![
            caused_by(ev("a", "x", 100), &["b"]),
            caused_by(ev("b", "y", 110), &["a"]),
        ];
        let t = trace(&l);
        assert!(!t.is_acyclic());
        assert_eq!(t.descendants("a").len(), 2, "it stops rather than looping");
    }

    #[test]
    fn an_ordinary_window_is_acyclic() {
        let l = vec![ev("a", "capture", 100), caused_by(ev("b", "spend", 160), &["a"])];
        assert!(trace(&l).is_acyclic());
    }
}
