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

use std::cmp::Ordering;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::mutation::Mutation;

/// Where an event's time came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// The time was observed — the source said when it happened.
    Observed,
    /// The time was inferred when backfilling history. Never to be mistaken
    /// for an observation.
    Legacy,
}

impl Default for Provenance {
    fn default() -> Self {
        Provenance::Observed
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
    #[serde(default)]
    pub provenance: Provenance,
    /// The Lamport stamp of the node that produced it.
    pub stamp: CausalStamp,
    /// Causal parents. Together with the stamp this makes the log a directed
    /// graph, not merely a list.
    #[serde(default)]
    pub causes: Vec<String>,
    #[serde(default)]
    pub mutations: Vec<Mutation>,
}

impl Event {
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
            provenance: Provenance::Observed,
            stamp: CausalStamp { counter, node: node.into() },
            causes: vec![],
            mutations: vec![],
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
}
