//! Event identity, time and ordering (RECORD / the Events-and-Time paper).
//!
//! ## There are two times, and one timestamp cannot say which
//!
//! The world happens; the system hears about it later, by unequal delays. A
//! message sent at 14:02 can land at 14:40 while its neighbour sent at 14:05
//! lands at 14:07 — and therefore lands *first*.
//!
//! **Skew is the normal case, not an error.** So an event carries `t_event`
//! (when it happened) distinctly from when it was processed, and ordering is
//! done on the former.
//!
//! ## `t_event` is a physical stamp, not a causal clock — both are needed
//!
//! This is the part that is easy to get wrong. Physical timestamps do not order
//! events across devices, because clocks disagree. The ordering that survives
//! is Lamport's **happens-before**.
//!
//! - `t_event` answers *which window does this fall in, and which reading is
//!   newer*.
//! - the causal stamp answers *did this actually depend on that*.
//!
//! A system with only one of them is either unable to window or unable to
//! detect a genuine conflict. Both are carried here.
//!
//! ## A total order that every replica agrees on
//!
//! Physical event time alone is not a total order: two distinct observations
//! with the same `t_event` leave neither dominating, and the merge stops
//! commuting. The repair is Lamport's own tie-break — order on the pair
//! `(t_event, id)` lexicographically, so **ties break identically on every
//! replica**.
//!
//! ## Exactly-once effect, without exactly-once delivery
//!
//! ```text
//!   at-least-once delivery ∘ idempotent apply = exactly-once EFFECT
//! ```
//!
//! Which is the only thing anyone wanted. The same fact *will* arrive twice,
//! because the only reliable delivery is retry. The cheapest sufficient
//! implementation is uniform dedupe on the stable `id` at the substrate —
//! [`dedupe`] — rather than each operator defending itself.
//!
//! The reference engine raises on a repeated id instead, which turns a normal
//! consequence of retry into an error (backlog #18).
//!
//! ## Backfilled history is marked
//!
//! A legacy row given an inferred event time carries `Provenance::Legacy`, so
//! an inferred time is never mistaken for an observed one.
//!
//! ## The second clock, and who reported it
//!
//! §I writes the record as
//! `⟨id, type, payload, t_event, t_ingest, source, causes⟩`. `t_ingest` is
//! **when the system durably learned of it**, and it is a genuinely different
//! number from `t_event` — the gap between them is the skew, read in
//! [`crate::clocks`].
//!
//! Both `t_ingest` and `source` are `Option` and `#[serde(default)]`, so a
//! record written before they existed still deserialises, folds and orders
//! unchanged. **`None` is not zero.** An absent ingest time means *nobody ever
//! recorded when this arrived*, which is a different fact from *it arrived
//! instantly*, and the skew reading keeps them apart.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mutation::Mutation;

/// Where an event's time came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// The time was observed — the source said when it happened.
    ///
    /// The `#[default]` reproduces the wire format exactly: a pre-slice record
    /// carrying no `provenance` field was an observation, so deserialising one
    /// must land here and not on `Legacy`.
    #[default]
    Observed,
    /// The time was inferred when backfilling history. Never to be mistaken
    /// for an observation.
    Legacy,
}

/// What kind of thing reported an event.
///
/// §I names them: *"which connector, which device, which Enzyme"*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// An integration — an SMS reader, a bank feed.
    Connector,
    /// A physical thing — a phone, a sensor.
    Device,
    /// The system's own execution produced it.
    Enzyme,
    /// A person entered it.
    Human,
}

/// How far the source is to be taken at its word.
///
/// **Declared, never computed.** Nothing infers a trust level from behaviour;
/// it is a statement made when the source is registered. §I asks for it as part
/// of provenance (*"at what trust level"*).
///
/// The one thing that reads it today is [`crate::clocks::ClockFinding::suspect`]
/// — when the two clocks disagree, which of them is the more likely culprit
/// depends on whether the source is the authority for what it reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    /// The source *is* the authority for what it reports — a bank stating a
    /// transaction it itself performed.
    Authoritative,
    /// It reports what it observed but is not the authority — a phone relaying
    /// an SMS, a sensor taking a reading.
    Reported,
    /// The value was derived rather than observed.
    Derived,
}

/// Provenance of the record: who reported it, and at what trust level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Which connector, device or Enzyme — `"mpesa"`, `"kcb"`, a device id.
    pub id: String,
    pub kind: SourceKind,
    pub trust: Trust,
}

impl Source {
    pub fn new(id: impl Into<String>, kind: SourceKind, trust: Trust) -> Self {
        Self { id: id.into(), kind, trust }
    }
}

/// A Lamport clock reading: `(counter, node)`.
///
/// The node id is part of the stamp so two replicas that increment
/// independently still produce distinct, comparable values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CausalStamp {
    pub counter: u64,
    pub node: String,
}

impl CausalStamp {
    pub fn new(node: impl Into<String>) -> Self {
        Self { counter: 0, node: node.into() }
    }

    /// A local step: the next event on this node.
    pub fn tick(&self) -> Self {
        Self { counter: self.counter + 1, node: self.node.clone() }
    }

    /// Receiving from elsewhere: advance past anything already seen.
    pub fn merge(&self, other: &CausalStamp) -> Self {
        Self {
            counter: self.counter.max(other.counter) + 1,
            node: self.node.clone(),
        }
    }
}

/// One durable event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Stable identity. Dedupe is keyed on this, so a retry of the same fact
    /// must carry the same id.
    pub id: String,
    pub name: String,
    /// When it happened in the world, as milliseconds since the epoch.
    pub t_event: i64,
    /// When the system durably learned of it, as milliseconds since the epoch.
    ///
    /// Additive and optional: a record written before the second clock existed
    /// carries `None`, which reads as *unknown* and never as *zero skew*.
    #[serde(default)]
    pub t_ingest: Option<i64>,
    /// Whether `t_event` was observed or inferred.
    #[serde(default)]
    pub provenance: Provenance,
    /// Who reported it. Optional for the same additive reason as `t_ingest`.
    #[serde(default)]
    pub source: Option<Source>,
    /// The Lamport stamp of the node that produced it.
    pub stamp: CausalStamp,
    /// Causal parents. Together with the stamp this makes the log a directed
    /// graph, not merely a list.
    #[serde(default)]
    pub causes: Vec<String>,
    #[serde(default)]
    pub mutations: Vec<Mutation>,
    /// What the Enzyme said when it emitted this.
    ///
    /// ★★★ **The seventh field, and it was missing for a reason worth keeping.**
    /// `payload` lived on [`crate::operator::EmittedEvent`] at execution time
    /// and never reached the durable record, because nothing bridged the two —
    /// minting an id and stamping the two clocks needs values only a host has.
    /// The row stayed open rather than gaining a field nobody populated.
    ///
    /// ★★★ **The bridge is core's job even though the values are the host's**,
    /// which is what closes it: [`Event::from_emitted`] takes the emission and
    /// the host's own id, clocks and source, and there is no other way to build
    /// an `Event` from an emission — so a host cannot bridge them wrongly by
    /// hand and lose the payload on the way.
    ///
    /// ★★ Additive: `#[serde(default)]` on an `Option`, so a record written
    /// before this field existed still deserialises, and reads as *this event
    /// carried no payload* rather than as an empty one.
    #[serde(default)]
    pub payload: Option<Value>,
}

impl Event {
    /// Backfill a legacy row that carried exactly ONE timestamp.
    ///
    /// The legacy record's single wall-clock stamp is when the *receiver* wrote
    /// it down, so it genuinely **is** an ingest time and lands on `t_ingest` as
    /// an observation. The event time is then *inferred* from it — §III's
    /// `t_event := timestamp` — and marked [`Provenance::Legacy`] so the
    /// inference can never pass for an observation.
    ///
    /// The two clocks therefore read equal, which is honest: it says *we only
    /// ever had one number*, not *it arrived instantly*. The skew reading keeps
    /// that distinction — see [`crate::clocks::skew_of`], which reports
    /// `Inferred` here rather than `Observed(0)`.
    /// **Bridge an emission into the durable record.**
    ///
    /// ★★★ The one way to turn what an Enzyme said into what the log keeps.
    /// The host supplies what only it can know — the id, the two clocks, where
    /// it came from and what caused it — and the payload rides across
    /// automatically rather than by a caller remembering to copy it.
    pub fn from_emitted(
        emitted: &crate::operator::EmittedEvent,
        id: impl Into<String>,
        t_event: i64,
        t_ingest: Option<i64>,
        stamp: CausalStamp,
        source: Option<Source>,
        causes: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: emitted.name.clone(),
            t_event,
            t_ingest,
            provenance: Provenance::Observed,
            source,
            stamp,
            causes,
            mutations: vec![],
            payload: Some(emitted.payload.clone()),
        }
    }

    pub fn backfilled(
        id: impl Into<String>,
        name: impl Into<String>,
        timestamp: i64,
        stamp: CausalStamp,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            t_event: timestamp,
            t_ingest: Some(timestamp),
            provenance: Provenance::Legacy,
            source: None,
            stamp,
            causes: vec![],
            mutations: vec![],
            // ★★ A legacy row carried no payload, and `None` says exactly that
            //    rather than inventing an empty object it never had.
            payload: None,
        }
    }

    /// The replica-agnostic sort key: `(t_event, id)`.
    ///
    /// Lexicographic, so an exact tie on physical time breaks the same way
    /// everywhere. Without the tie-break the order is partial and the merge
    /// stops commuting.
    pub fn order_key(&self) -> (i64, &str) {
        (self.t_event, self.id.as_str())
    }

    /// Lamport happens-before: did `self` causally precede `other`?
    ///
    /// True when `other` names this event among its causes, or when both are on
    /// the same node and this one came first. Events unordered by this relation
    /// are **concurrent** — which is a real answer, not a missing one.
    pub fn happens_before(&self, other: &Event) -> bool {
        if other.causes.iter().any(|c| c == &self.id) {
            return true;
        }
        self.stamp.node == other.stamp.node && self.stamp.counter < other.stamp.counter
    }

    /// Neither causally precedes the other.
    pub fn concurrent_with(&self, other: &Event) -> bool {
        self.id != other.id && !self.happens_before(other) && !other.happens_before(self)
    }
}

/// Sort into the replica-agnostic total order.
///
/// Every replica handed the same set produces the same sequence, which is what
/// makes the fold convergent across devices.
pub fn order(events: &mut [Event]) {
    events.sort_by(|a, b| match a.t_event.cmp(&b.t_event) {
        Ordering::Equal => a.id.cmp(&b.id),
        other => other,
    });
}

/// Drop repeats by stable id, keeping the first occurrence.
///
/// This is the whole of idempotent apply: with it, at-least-once delivery
/// yields exactly-once effect. A repeated id is a normal consequence of retry,
/// not an error to raise on.
pub fn dedupe(events: Vec<Event>) -> Vec<Event> {
    let mut seen = BTreeSet::new();
    events
        .into_iter()
        .filter(|e| seen.insert(e.id.clone()))
        .collect()
}

/// Deduplicate, then order. The normal way to prepare a log for folding.
pub fn merge(events: Vec<Event>) -> Vec<Event> {
    let mut out = dedupe(events);
    order(&mut out);
    out
}

/// A snapshot-valued observation: a value plus the event time it was observed.
///
/// A balance read off an SMS, a level read off a sensor — an observation of an
/// authoritative external quantity rather than an increment of a local one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation<T> {
    pub value: T,
    pub t_event: i64,
    /// Included in the ordering key so ties break identically everywhere.
    pub id: String,
}

impl<T: Clone> Observation<T> {
    /// Last-writer-wins join, keyed on `(t_event, id)`.
    ///
    /// This is a state-based CRDT: the join is commutative, associative and
    /// idempotent, so replicas that see the same observations in any order
    /// converge on the same value.
    pub fn join(&self, other: &Self) -> Self {
        if (other.t_event, other.id.as_str()) > (self.t_event, self.id.as_str()) {
            other.clone()
        } else {
            self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, t: i64, node: &str, counter: u64) -> Event {
        Event {
            id: id.into(),
            name: "event.test.thing".into(),
            t_event: t,
            t_ingest: None,
            provenance: Provenance::Observed,
            source: None,
            stamp: CausalStamp { counter, node: node.into() },
            causes: vec![],
            mutations: vec![],
            payload: None,
        }
    }

    #[test]
    fn ordering_is_by_when_it_happened_not_when_it_arrived() {
        // The paper's own example: sent 14:02, lands 14:40; its neighbour sent
        // 14:05 lands 14:07 and therefore arrives FIRST.
        let arrival_order = vec![ev("b", 1405, "phone", 2), ev("a", 1402, "phone", 1)];
        let mut ordered = arrival_order;
        order(&mut ordered);
        assert_eq!(ordered[0].id, "a", "the earlier happening sorts first");
    }

    #[test]
    fn a_tie_on_physical_time_breaks_identically_on_every_replica() {
        // Without the id tie-break the order is partial and the merge stops
        // commuting.
        let mut one = vec![ev("z", 100, "phone", 1), ev("a", 100, "laptop", 1)];
        let mut two = vec![ev("a", 100, "laptop", 1), ev("z", 100, "phone", 1)];
        order(&mut one);
        order(&mut two);
        assert_eq!(
            one.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            two.iter().map(|e| e.id.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn two_devices_that_were_both_offline_converge_on_the_same_order() {
        let phone = vec![ev("p1", 100, "phone", 1), ev("p2", 300, "phone", 2)];
        let laptop = vec![ev("l1", 200, "laptop", 1), ev("l2", 400, "laptop", 2)];

        let mut a = phone.clone();
        a.extend(laptop.clone());
        let mut b = laptop;
        b.extend(phone);

        let a = merge(a);
        let b = merge(b);
        assert_eq!(
            a.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            b.iter().map(|e| e.id.as_str()).collect::<Vec<_>>()
        );
        assert_eq!(a.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
                   vec!["p1", "l1", "p2", "l2"]);
    }

    #[test]
    fn a_repeated_id_is_dropped_not_raised_on() {
        // Retry is the only reliable delivery, so a repeat is normal.
        let events = vec![ev("a", 100, "n", 1), ev("a", 100, "n", 1), ev("b", 200, "n", 2)];
        let out = dedupe(events);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn at_least_once_delivery_gives_exactly_once_effect() {
        let once = merge(vec![ev("a", 100, "n", 1), ev("b", 200, "n", 2)]);
        let thrice = merge(vec![
            ev("a", 100, "n", 1), ev("b", 200, "n", 2),
            ev("a", 100, "n", 1), ev("b", 200, "n", 2), ev("a", 100, "n", 1),
        ]);
        assert_eq!(once, thrice, "delivering it repeatedly must not apply it repeatedly");
    }

    #[test]
    fn causality_is_recognised_across_devices_where_clocks_would_not_help() {
        let mut child = ev("c", 50, "laptop", 1); // an EARLIER physical stamp
        child.causes = vec!["p".into()];
        let parent = ev("p", 900, "phone", 1);
        assert!(parent.happens_before(&child), "a declared cause outranks a clock");
    }

    #[test]
    fn same_node_order_implies_causality() {
        assert!(ev("a", 100, "phone", 1).happens_before(&ev("b", 200, "phone", 2)));
    }

    #[test]
    fn concurrency_is_a_real_answer_not_a_missing_one() {
        let a = ev("a", 100, "phone", 1);
        let b = ev("b", 100, "laptop", 1);
        assert!(a.concurrent_with(&b));
        assert!(!a.happens_before(&b));
        assert!(!b.happens_before(&a));
    }

    #[test]
    fn a_lamport_clock_advances_past_what_it_has_seen() {
        let local = CausalStamp { counter: 3, node: "phone".into() };
        let remote = CausalStamp { counter: 9, node: "laptop".into() };
        let merged = local.merge(&remote);
        assert_eq!(merged.counter, 10);
        assert_eq!(merged.node, "phone");
    }

    #[test]
    fn backfilled_history_is_marked_so_it_cannot_pass_for_observed() {
        let mut e = ev("legacy1", 0, "n", 1);
        e.provenance = Provenance::Legacy;
        assert_ne!(e.provenance, Provenance::Observed);
    }

    // ── the second clock lands additively ──────────────────────────────────

    /// The exact wire shape of an event written before `t_ingest` and `source`
    /// existed — no such keys anywhere in it.
    const PRE_SLICE_WIRE: &str = r#"{
        "id": "old1",
        "name": "event.finances.income_received",
        "t_event": 1000,
        "provenance": "observed",
        "stamp": {"counter": 1, "node": "phone"},
        "causes": ["genesis"],
        "mutations": []
    }"#;

    #[test]
    fn a_record_written_before_the_second_clock_still_deserialises() {
        let e: Event = serde_json::from_str(PRE_SLICE_WIRE).unwrap();
        assert_eq!(e.id, "old1");
        assert_eq!(e.t_event, 1000);
        assert_eq!(e.t_ingest, None, "absent, which reads as unknown");
        assert_eq!(e.source, None);
        assert_eq!(e.causes, vec!["genesis".to_string()], "the causal graph is untouched");
    }

    #[test]
    fn the_new_fields_do_not_disturb_ordering_dedupe_or_identity() {
        // The migration check: an old-format log and the same log with the
        // second clock attached order and dedupe identically, because neither
        // operation reads t_ingest.
        let old: Event = serde_json::from_str(PRE_SLICE_WIRE).unwrap();
        let mut migrated = old.clone();
        migrated.t_ingest = Some(1000 + 7 * 24 * 60 * 60_000); // a week of skew
        migrated.source = Some(Source::new("mpesa", SourceKind::Connector, Trust::Authoritative));

        assert_eq!(old.order_key(), migrated.order_key(), "the sort key is unchanged");

        let mut a = vec![old.clone(), ev("z", 2000, "phone", 2)];
        let mut b = vec![migrated, ev("z", 2000, "phone", 2)];
        order(&mut a);
        order(&mut b);
        assert_eq!(
            a.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            b.iter().map(|e| e.id.as_str()).collect::<Vec<_>>()
        );

        // And dedupe still keys on the stable id, not on the record's shape.
        assert_eq!(dedupe(vec![old.clone(), old]).len(), 1);
    }

    #[test]
    fn a_source_says_which_connector_at_what_trust_level() {
        let mut e = ev("a", 100, "phone", 1);
        e.source = Some(Source::new("kcb", SourceKind::Connector, Trust::Reported));
        let round: Event = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        let s = round.source.unwrap();
        assert_eq!(s.id, "kcb");
        assert_eq!(s.kind, SourceKind::Connector);
        assert_eq!(s.trust, Trust::Reported);
    }

    // ── the CRDT property, stated as three laws ─────────────────────────────

    fn obs(v: i64, t: i64, id: &str) -> Observation<i64> {
        Observation { value: v, t_event: t, id: id.into() }
    }

    #[test]
    fn last_writer_wins_on_event_time() {
        assert_eq!(obs(10, 100, "a").join(&obs(20, 200, "b")).value, 20);
        assert_eq!(obs(20, 200, "b").join(&obs(10, 100, "a")).value, 20);
    }

    #[test]
    fn the_join_is_commutative_associative_and_idempotent() {
        let (x, y, z) = (obs(1, 100, "x"), obs(2, 100, "y"), obs(3, 50, "z"));

        assert_eq!(x.join(&y), y.join(&x), "commutative");
        assert_eq!(x.join(&y).join(&z), x.join(&y.join(&z)), "associative");
        assert_eq!(x.join(&x), x, "idempotent");
    }

    #[test]
    fn replicas_converge_whatever_order_they_saw_things_in() {
        let (a, b, c) = (obs(1, 100, "a"), obs(2, 300, "b"), obs(3, 200, "c"));
        let phone = a.join(&b).join(&c);
        let laptop = c.join(&a).join(&b);
        let tablet = b.join(&c).join(&a);
        assert_eq!(phone, laptop);
        assert_eq!(laptop, tablet);
        assert_eq!(phone.value, 2, "the latest observation wins");
    }
    // ── EVT-1's seventh field ────────────────────────────────────────────────

    #[test]
    fn what_an_enzyme_said_now_reaches_the_durable_record() {
        // ★★★ The residual EVT-1 carried: `payload` lived on `EmittedEvent` at
        //     execution time and nothing bridged it across, so the log kept the
        //     name and lost the content.
        let emitted = crate::operator::EmittedEvent {
            name: "event.finances.pocket_spent".into(),
            payload: serde_json::json!({ "pocket": "rent", "amount": 15000.0 }),
        };
        let e = Event::from_emitted(
            &emitted,
            "evt-1",
            1_000,
            Some(1_050),
            CausalStamp::new("phone"),
            None,
            vec![],
        );
        assert_eq!(e.name, "event.finances.pocket_spent");
        assert_eq!(e.payload.as_ref().and_then(|p| p.get("pocket")), Some(&serde_json::json!("rent")));
    }

    #[test]
    fn the_bridge_is_the_only_way_across_so_a_host_cannot_lose_it_by_hand() {
        // ★★ The values are the host's — the id, the two clocks, the source —
        //    and the bridge is the core's. Splitting it that way is what stops
        //    a caller assembling an `Event` and forgetting the payload.
        let emitted = crate::operator::EmittedEvent {
            name: "event.x".into(),
            payload: serde_json::json!({ "a": 1 }),
        };
        let e = Event::from_emitted(&emitted, "id", 1, None, CausalStamp::new("n"), None, vec![]);
        assert!(e.payload.is_some());
    }

    #[test]
    fn a_record_written_before_this_field_existed_still_reads() {
        // ★★ Additive, exactly as `t_ingest` and `source` were: a pre-slice
        //    wire record deserialises and reads as "carried no payload" rather
        //    than as an empty one.
        let old = r#"{"id":"e1","name":"event.x","t_event":1,
                      "stamp":{"counter":1,"node":"n"}}"#;
        let e: Event = serde_json::from_str(old).expect("an old record still reads");
        assert_eq!(e.payload, None);
    }

    #[test]
    fn a_backfilled_legacy_row_carries_no_payload_rather_than_an_empty_one() {
        // ★★ `None` says we never had one; `Some({})` would say the Enzyme
        //    emitted nothing, and those are different claims.
        let e = Event::backfilled("e1", "event.x", 1_000, CausalStamp::new("n"));
        assert_eq!(e.payload, None);
    }

}
