//! Vector clocks and genuine concurrency detection
//! (Events & Time §VI–§VII · MUL-10, EVT-8).
//!
//! ```text
//! V_i[i] += 1                     on a local event
//! V_i ← max(V_i, V_msg)           componentwise, on receive
//!
//! a → b  ⟺  V(a) < V(b)                       happens-before
//! a ∥ b  ⟺  V(a) ≰ V(b) ∧ V(b) ≰ V(a)         concurrent
//! ```
//!
//! ## What the scalar clock cannot do, concretely
//!
//! > The converse fails — `C(a) < C(b)` does not imply `a → b` — so a logical
//! > clock cannot detect concurrency. **Vector clocks restore the
//! > biconditional** (Fidge 1988; Mattern 1989).
//!
//! R1's [`crate::event::CausalStamp`] is a scalar `(counter, node)`, and
//! [`crate::event::Event::happens_before`] answers from two things: does the
//! other event name this one among its **direct** `causes`, or are they on the
//! same node in counter order. Both are correct as far as they go, and
//! together they miss **transitive causality across nodes**:
//!
//! ```text
//! a on node-1  →  b on node-2 (causes: a)  →  c on node-2 (causes: b)
//! ```
//!
//! `a → c` genuinely holds, and the scalar answer is *concurrent* — `c` does
//! not name `a`, and they are on different nodes. That is a **false
//! concurrent**, and it is the specific defect this module removes. It matters
//! because a merge that believes two causally-ordered edits are concurrent
//! will treat a supersession as a conflict.
//!
//! ## `t_event` is a physical stamp and is NOT a causal clock
//!
//! > Both are needed. `t_event` answers *"which window does this fall in, and
//! > which reading is newer"*; the causal stamp answers *"did this actually
//! > depend on that."*
//!
//! So nothing here replaces `order_key()`. [`Stamped`] carries both, and they
//! answer different questions: [`MergeResolution::as_snapshot`] uses the physical
//! total order because that is the right key for *"which reading is newer"*,
//! and the vector clock decides whether that question is even the right one to
//! ask.
//!
//! ## ★ The payoff — LWW converges, and convergence is not correctness
//!
//! §VII proves the LWW register is a CRDT and then says plainly where it is
//! wrong:
//!
//! > Two genuinely concurrent *edits* to the same field under LWW means one is
//! > silently discarded [...] That is precisely why the theorem is scoped to
//! > **snapshot-valued** dimensions: for an *observation of an external
//! > authority*, discarding the older reading is not data loss, it is the
//! > semantically correct answer. For a **contested edit** [...] LWW is the
//! > wrong structure.
//!
//! Telling those apart needs concurrency detection, which is why this module
//! and the merge belong together. [`merge`] returns a [`MergeResolution`], and
//! **`MergeResolution::Concurrent` carries BOTH values**. For a
//! [`Dimension::Snapshot`] the caller may collapse it by the physical order —
//! §VII's theorem, and correct there. For a [`Dimension::Contested`] both
//! survive: *never erase the node*, applied to a concurrent edit.
//!
//! A causally-ordered pair is not a conflict in either case. The later event
//! genuinely supersedes the earlier one, and nothing is lost by saying so.
//!
//! ## Honest limits
//!
//! - **`O(n)` metadata per stamp**, one counter per node that has ever acted.
//!   §VI says it plainly: for a household — a phone, a laptop, a handful of
//!   devices — `n` is small and the cost is nothing. It is not nothing for an
//!   open federation, and that is the scale at which this choice is revisited.
//! - **An absent component reads as 0**, which is what lets clocks over
//!   different node sets compare correctly. A node that has never acted has
//!   done nothing, which is a fact rather than a gap.
//! - **[`assign_clocks`] derives clocks from the recorded `causes` DAG.** It
//!   adds no field to `Event` and changes no wire format, so every R1 parity
//!   vector keeps passing untouched. A cycle in the declared causes is refused
//!   — an event cannot depend on its own consequence.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::event::Event;

/// `V` — one counter per node.
///
/// ★ Serialisable because a clock **crosses a wire**: a peer asking for what
/// it lacks sends its own frontier, and the reply is computed against it. That
/// is the whole of the sync request (`sync::Replica::missing_from`), so the
/// type has to survive the trip.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    counters: BTreeMap<String, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// **An absent component is 0.**
    ///
    /// Not a missing value: a node that has never acted has done nothing. This
    /// is what lets two clocks over different node sets be compared at all.
    pub fn get(&self, node: &str) -> u64 {
        self.counters.get(node).copied().unwrap_or(0)
    }

    /// `V_i[i] += 1` — a local event.
    pub fn tick(&self, node: &str) -> Self {
        let mut c = self.counters.clone();
        *c.entry(node.to_string()).or_insert(0) += 1;
        Self { counters: c }
    }

    /// One component set outright, if it is an advance.
    ///
    /// ★★ Distinct from [`VectorClock::tick`] on purpose. `tick` is *I did
    /// something*; this is *I have observed that node reach this counter* —
    /// which is what building a **frontier** from a log of entries is, and
    /// there is no way to express it by ticking. It never goes backwards, so
    /// it cannot be used to forget.
    pub fn at(&self, node: &str, counter: u64) -> Self {
        let mut c = self.counters.clone();
        let e = c.entry(node.to_string()).or_insert(0);
        *e = (*e).max(counter);
        Self { counters: c }
    }

    /// `V_i ← max(V_i, V_msg)` componentwise.
    pub fn merge(&self, other: &Self) -> Self {
        let mut c = self.counters.clone();
        for (node, v) in &other.counters {
            let e = c.entry(node.clone()).or_insert(0);
            *e = (*e).max(*v);
        }
        Self { counters: c }
    }

    /// Receiving: merge what the message knew, then take a local step.
    pub fn observe(&self, node: &str, message: &Self) -> Self {
        self.merge(message).tick(node)
    }

    /// Every node this clock knows anything about.
    pub fn nodes(&self) -> BTreeSet<&str> {
        self.counters.keys().map(String::as_str).collect()
    }

    /// Componentwise `≥` over the union of both node sets.
    pub fn dominates(&self, other: &Self) -> bool {
        self.nodes()
            .union(&other.nodes())
            .all(|n| self.get(n) >= other.get(n))
    }

    /// **The biconditional** (§VI).
    pub fn compare(&self, other: &Self) -> CausalVerdict {
        let a_ge_b = self.dominates(other);
        let b_ge_a = other.dominates(self);
        match (a_ge_b, b_ge_a) {
            (true, true) => CausalVerdict::Same,
            (false, true) => CausalVerdict::HappensBefore,
            (true, false) => CausalVerdict::HappenedAfter,
            // Neither dominates: V(a) ≰ V(b) ∧ V(b) ≰ V(a).
            (false, false) => CausalVerdict::Concurrent,
        }
    }

    /// `a → b`.
    pub fn happens_before(&self, other: &Self) -> bool {
        self.compare(other) == CausalVerdict::HappensBefore
    }

    /// `a ∥ b` — **a real answer, not a missing one**.
    pub fn concurrent_with(&self, other: &Self) -> bool {
        self.compare(other) == CausalVerdict::Concurrent
    }

    /// The metadata cost §VI names: one counter per node that has acted.
    pub fn size(&self) -> usize {
        self.counters.len()
    }
}

/// The four possible relations between two stamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CausalVerdict {
    /// `V(a) < V(b)` — `a` genuinely influenced `b`.
    HappensBefore,
    /// `V(b) < V(a)`.
    HappenedAfter,
    /// Neither dominates. **Detectable only with a vector clock.**
    Concurrent,
    /// Identical clocks — the same point in causal history.
    Same,
}

// ── the merge ───────────────────────────────────────────────────────────────

/// Which kind of dimension this value is, and therefore what a concurrent pair
/// means (§VII).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    /// An **observation of an authoritative external quantity** — a balance
    /// read off an SMS, a level off a sensor.
    ///
    /// LWW is *correct* here, and §VII proves it: discarding the older reading
    /// is not data loss, it is the semantically right answer, because the newer
    /// reading of a balance supersedes the older one by definition.
    Snapshot,
    /// A **contested edit** — two people changing a shared plan.
    ///
    /// LWW is the wrong structure: it would silently discard one of them.
    Contested,
    /// An **accumulating** quantity — a running total raised by increments.
    ///
    /// ★ Added by EVT-10. Neither of the other two answers is right for it:
    /// collapsing by the physical order would *drop a contribution*, and
    /// handing both back for reconciliation invites a human to pick one, which
    /// also drops a contribution. Both count, and the merge is arithmetic —
    /// see [`crate::dimension::AccumulatingDim`], which carries the applied-id
    /// set that makes it idempotent.
    Accumulating,
}

/// A value with both stamps §VI says are needed.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamped<T> {
    pub value: T,
    /// Causal: *did this depend on that?*
    pub clock: VectorClock,
    /// Physical: *which reading is newer?* Never a causal answer.
    pub t_event: i64,
    /// The tie-break that makes the physical order total.
    pub id: String,
}

impl<T> Stamped<T> {
    pub fn new(value: T, clock: VectorClock, t_event: i64, id: &str) -> Self {
        Self { value, clock, t_event, id: id.to_string() }
    }

    /// §VII's total ordering key — physical, with Lamport's tie-break.
    pub fn order_key(&self) -> (i64, &str) {
        (self.t_event, self.id.as_str())
    }
}

/// What a merge found.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeResolution<T> {
    /// One causally precedes the other. **Not a conflict** — the later event
    /// genuinely supersedes, and nothing is lost by saying so.
    Superseded { winner: Stamped<T>, superseded: Stamped<T> },
    /// **Genuinely concurrent, and both are kept.**
    ///
    /// There is deliberately no accessor that collapses this to one value
    /// without the caller stating which semantics it wants — which is what
    /// stops a contested edit from being silently LWW'd.
    Concurrent { left: Stamped<T>, right: Stamped<T> },
    /// The same point in causal history — idempotent re-delivery.
    Same(Stamped<T>),
}

/// What a merge means once the dimension's declared kind is applied.
///
/// ★★ Three variants because §VII's three kinds are three different answers,
/// and collapsing them into one `Vec` is how a contested edit gets silently
/// discarded — *"the single easiest way to turn a correct theorem into a lost
/// transaction."*
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved<T> {
    /// `Snapshot`: the newer reading supersedes, and that is **correct** — for
    /// an observation of an external authority the older reading is not lost
    /// data, it is superseded data.
    Superseded(Stamped<T>),
    /// `Accumulating`: **both contributions count.** Join them arithmetically
    /// through the counter; picking one drops a contribution.
    BothCount(Vec<Stamped<T>>),
    /// `Contested`: both are kept for a **human or a merge-CRDT** to
    /// reconcile. Never summed, never picked automatically.
    NeedsReconciliation(Vec<Stamped<T>>),
}

impl<T> Resolved<T> {
    /// Everything that survived, whatever the kind — for a caller that only
    /// needs the values and has already honoured the distinction.
    pub fn values(&self) -> &[Stamped<T>] {
        match self {
            Resolved::Superseded(x) => std::slice::from_ref(x),
            Resolved::BothCount(v) | Resolved::NeedsReconciliation(v) => v,
        }
    }
}

impl<T: Clone> MergeResolution<T> {
    pub fn is_concurrent(&self) -> bool {
        matches!(self, MergeResolution::Concurrent { .. })
    }

    /// Collapse by the **physical** order — §VII's LWW register.
    ///
    /// Correct for a [`Dimension::Snapshot`], and available on a concurrent
    /// pair only because for an observation of an external authority that is
    /// genuinely the right answer.
    pub fn as_snapshot(&self) -> Stamped<T> {
        match self {
            MergeResolution::Superseded { winner, .. } => winner.clone(),
            MergeResolution::Same(x) => x.clone(),
            MergeResolution::Concurrent { left, right } => {
                if right.order_key() > left.order_key() {
                    right.clone()
                } else {
                    left.clone()
                }
            }
        }
    }

    /// Everything that survives. **Two values only when genuinely concurrent.**
    pub fn as_contested(&self) -> Vec<Stamped<T>> {
        match self {
            MergeResolution::Superseded { winner, .. } => vec![winner.clone()],
            MergeResolution::Same(x) => vec![x.clone()],
            MergeResolution::Concurrent { left, right } => {
                // Deterministic order so two replicas surface the same pair the
                // same way — the merge must still be commutative.
                if left.order_key() <= right.order_key() {
                    vec![left.clone(), right.clone()]
                } else {
                    vec![right.clone(), left.clone()]
                }
            }
        }
    }

    /// What this resolution means for a declared dimension kind.
    ///
    /// ★★ The one place §VII's scoping is applied — and it returns a **typed**
    /// outcome rather than a bare list, because the three kinds mean three
    /// genuinely different things and a `Vec` that means all three is exactly
    /// the remembered rule EVT-10 exists to replace. A `Contested` result can
    /// no longer be mistaken for a `Snapshot` one, and an `Accumulating` result
    /// can no longer be picked from.
    pub fn resolve(&self, kind: Dimension) -> Resolved<T> {
        match kind {
            Dimension::Snapshot => Resolved::Superseded(self.as_snapshot()),
            Dimension::Contested => Resolved::NeedsReconciliation(self.as_contested()),
            Dimension::Accumulating => Resolved::BothCount(self.as_contested()),
        }
    }
}

/// Merge two stamped values, using the **causal** clock to decide whether this
/// is a supersession or a genuine conflict.
pub fn merge<T: Clone>(left: &Stamped<T>, right: &Stamped<T>) -> MergeResolution<T> {
    match left.clock.compare(&right.clock) {
        CausalVerdict::Same => MergeResolution::Same(left.clone()),
        CausalVerdict::HappensBefore => {
            MergeResolution::Superseded { winner: right.clone(), superseded: left.clone() }
        }
        CausalVerdict::HappenedAfter => {
            MergeResolution::Superseded { winner: left.clone(), superseded: right.clone() }
        }
        CausalVerdict::Concurrent => {
            MergeResolution::Concurrent { left: left.clone(), right: right.clone() }
        }
    }
}

// ── deriving clocks from the log R1 already records ─────────────────────────

/// Build a vector clock for every event from the recorded `causes` DAG.
///
/// **Adds no field to [`Event`] and changes no wire format**, so every R1
/// parity vector keeps passing untouched. Each event's clock is the
/// componentwise max of its causes' clocks, then a local tick — which is
/// exactly §VI's construction, applied to data the log already carries.
///
/// Refuses a cycle: an event cannot depend on its own consequence.
pub fn assign_clocks(events: &[Event]) -> Result<BTreeMap<String, VectorClock>, ClockError> {
    let by_id: BTreeMap<&str, &Event> = events.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut clocks: BTreeMap<String, VectorClock> = BTreeMap::new();
    let mut pending: Vec<&Event> = events.iter().collect();

    while !pending.is_empty() {
        let (ready, held): (Vec<&Event>, Vec<&Event>) = pending.into_iter().partition(|e| {
            e.causes
                .iter()
                // A cause outside this set is not a reason to wait: it is
                // history the caller did not hand over.
                .all(|c| !by_id.contains_key(c.as_str()) || clocks.contains_key(c))
        });

        if ready.is_empty() {
            let mut cycle: Vec<String> = held.iter().map(|e| e.id.clone()).collect();
            cycle.sort();
            return Err(ClockError::CausalCycle(cycle));
        }

        for e in ready {
            let mut v = VectorClock::new();
            for c in &e.causes {
                if let Some(prior) = clocks.get(c) {
                    v = v.merge(prior);
                }
            }
            clocks.insert(e.id.clone(), v.tick(&e.stamp.node));
        }
        pending = held;
    }

    Ok(clocks)
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ClockError {
    #[error("the declared causes contain a cycle among {0:?} — an event cannot depend on its own consequence")]
    CausalCycle(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CausalStamp, Event, Provenance};

    fn ev(id: &str, node: &str, counter: u64, t: i64, causes: &[&str]) -> Event {
        Event {
            id: id.to_string(),
            name: "e".into(),
            t_event: t,
            provenance: Provenance::Observed,
            t_ingest: None,
            source: None,
            stamp: CausalStamp { counter, node: node.to_string() },
            causes: causes.iter().map(|c| c.to_string()).collect(),
            mutations: vec![],
        }
    }

    /// a on node-1 → b on node-2 → c on node-2. Genuinely a → c.
    fn chain() -> Vec<Event> {
        vec![
            ev("a", "node-1", 1, 100, &[]),
            ev("b", "node-2", 2, 200, &["a"]),
            ev("c", "node-2", 3, 300, &["b"]),
        ]
    }

    // ── §VI — the clock ─────────────────────────────────────────────────────

    #[test]
    fn a_local_event_advances_only_its_own_component() {
        let v = VectorClock::new().tick("phone");
        assert_eq!(v.get("phone"), 1);
        assert_eq!(v.get("laptop"), 0, "a node that has never acted has done nothing");
    }

    #[test]
    fn receiving_takes_the_componentwise_max_then_steps() {
        let phone = VectorClock::new().tick("phone").tick("phone"); // {phone: 2}
        let laptop = VectorClock::new().tick("laptop"); // {laptop: 1}
        let after = laptop.observe("laptop", &phone);

        assert_eq!(after.get("phone"), 2, "took what the message knew");
        assert_eq!(after.get("laptop"), 2, "and took its own local step");
    }

    #[test]
    fn clocks_over_different_node_sets_compare_correctly() {
        // Only possible because an absent component reads as 0.
        let a = VectorClock::new().tick("phone");
        let b = a.tick("phone").tick("laptop");
        assert_eq!(a.compare(&b), CausalVerdict::HappensBefore);
        assert_eq!(b.compare(&a), CausalVerdict::HappenedAfter);
    }

    #[test]
    fn the_biconditional_holds_in_all_four_cases() {
        let base = VectorClock::new().tick("p");
        let later = base.tick("p");
        let other = base.tick("q");

        assert_eq!(base.compare(&base), CausalVerdict::Same);
        assert_eq!(base.compare(&later), CausalVerdict::HappensBefore);
        assert_eq!(later.compare(&base), CausalVerdict::HappenedAfter);
        assert_eq!(
            later.compare(&other),
            CausalVerdict::Concurrent,
            "★ neither dominates — a real answer, not a missing one"
        );
    }

    #[test]
    fn the_metadata_is_one_counter_per_node_that_has_acted() {
        // §VI's O(n): for a household, n is small and the cost is nothing.
        let v = VectorClock::new().tick("phone").tick("laptop").tick("phone");
        assert_eq!(v.size(), 2);
    }

    // ── ★★ what the scalar clock gets wrong ─────────────────────────────────

    #[test]
    fn the_scalar_clock_reports_a_false_concurrent_across_a_transitive_chain() {
        // ★★ THE PROOF OF VALUE, first half. a → b → c genuinely holds. R1's
        // scalar answer is CONCURRENT, because `c` does not name `a` directly
        // and they sit on different nodes.
        let c = chain();
        let (a, cc) = (&c[0], &c[2]);

        assert!(!a.happens_before(cc), "the scalar clock cannot see through `b`");
        assert!(
            a.concurrent_with(cc),
            "★ so it calls two causally-ordered events concurrent — a FALSE concurrent"
        );
    }

    #[test]
    fn the_vector_clock_sees_the_transitive_chain_correctly() {
        // ★★ And the same three events, stamped with vector clocks.
        let events = chain();
        let clocks = assign_clocks(&events).unwrap();

        assert_eq!(
            clocks["a"].compare(&clocks["c"]),
            CausalVerdict::HappensBefore,
            "★ a → c, which is the truth"
        );
        assert!(!clocks["a"].concurrent_with(&clocks["c"]));
    }

    #[test]
    fn genuinely_concurrent_events_are_still_reported_concurrent() {
        // The other direction: the fix must not simply order everything.
        let events = vec![
            ev("root", "node-1", 1, 100, &[]),
            ev("x", "node-1", 2, 200, &["root"]),
            ev("y", "node-2", 2, 200, &["root"]),
        ];
        let clocks = assign_clocks(&events).unwrap();
        assert_eq!(clocks["x"].compare(&clocks["y"]), CausalVerdict::Concurrent);
    }

    #[test]
    fn a_causal_cycle_is_refused() {
        let events = vec![
            ev("p", "n", 1, 1, &["q"]),
            ev("q", "n", 2, 2, &["p"]),
        ];
        assert!(matches!(assign_clocks(&events), Err(ClockError::CausalCycle(_))));
    }

    #[test]
    fn a_cause_outside_the_supplied_set_is_history_not_a_deadlock() {
        // The caller handed over part of a log. That is not a cycle.
        let events = vec![ev("only", "n", 5, 500, &["something-older"])];
        let clocks = assign_clocks(&events).unwrap();
        assert_eq!(clocks["only"].get("n"), 1);
    }

    // ── ★★ the merge, sharpened ─────────────────────────────────────────────

    fn stamped(v: &str, clock: VectorClock, t: i64, id: &str) -> Stamped<String> {
        Stamped::new(v.to_string(), clock, t, id)
    }

    #[test]
    fn a_causally_ordered_pair_is_a_supersession_not_a_conflict() {
        let base = VectorClock::new().tick("p");
        let later = base.tick("p");
        let r = merge(&stamped("old", base, 100, "e1"), &stamped("new", later, 200, "e2"));

        assert!(!r.is_concurrent());
        assert_eq!(r.as_snapshot().value, "new");
        assert_eq!(r.as_contested().len(), 1, "nothing was lost, so nothing survives twice");
    }

    #[test]
    fn two_concurrent_edits_are_detected_and_both_survive() {
        // ★★ THE PROOF OF VALUE, second half, and §VII's own honest limit:
        // "two genuinely concurrent edits to the same field under LWW means one
        // is silently discarded". Here neither is.
        let root = VectorClock::new().tick("shared");
        let mine = root.tick("phone");
        let yours = root.tick("laptop");

        let r = merge(&stamped("go monday", mine, 100, "e1"), &stamped("go friday", yours, 200, "e2"));

        assert!(r.is_concurrent(), "★ detectable only with a vector clock");

        let kept = r.resolve(Dimension::Contested);
        let kept = kept.values();
        assert_eq!(kept.len(), 2, "★★ both survive — never erase the node");
        let values: Vec<&str> = kept.iter().map(|s| s.value.as_str()).collect();
        assert!(values.contains(&"go monday") && values.contains(&"go friday"));
    }

    #[test]
    fn the_same_pair_under_snapshot_semantics_collapses_and_that_is_correct() {
        // §VII: for an observation of an external authority, discarding the
        // older reading IS the semantically right answer. Same pair, different
        // declared dimension, different — and both correct — outcomes.
        let root = VectorClock::new().tick("shared");
        let earlier = root.tick("phone");
        let later = root.tick("laptop");

        let r = merge(&stamped("1200", earlier, 100, "e1"), &stamped("1350", later, 200, "e2"));
        assert!(r.is_concurrent(), "concurrent by causality either way");

        let kept = r.resolve(Dimension::Snapshot);
        let kept = kept.values();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].value, "1350", "the newer READING supersedes by definition");
    }

    #[test]
    fn the_contested_pair_surfaces_in_a_deterministic_order() {
        // The merge must still be commutative: two replicas handed the same
        // pair the other way round surface the same list.
        let root = VectorClock::new().tick("shared");
        let (a, b) = (root.tick("phone"), root.tick("laptop"));
        let (x, y) = (stamped("A", a, 100, "e1"), stamped("B", b, 200, "e2"));

        let one = merge(&x, &y).as_contested();
        let two = merge(&y, &x).as_contested();
        assert_eq!(one, two, "★ order of arrival must not change the result");
    }

    #[test]
    fn re_delivering_the_same_stamp_is_idempotent() {
        let v = VectorClock::new().tick("p");
        let r = merge(&stamped("x", v.clone(), 100, "e1"), &stamped("x", v, 100, "e1"));
        assert!(matches!(r, MergeResolution::Same(_)));
        assert_eq!(r.as_contested().len(), 1);
    }

    #[test]
    fn the_physical_stamp_is_never_used_to_decide_causality() {
        // §VI: t_event is a physical stamp and is NOT a causal clock. A
        // concurrent pair stays concurrent however far apart their t_event is.
        let root = VectorClock::new().tick("shared");
        let (a, b) = (root.tick("phone"), root.tick("laptop"));
        let r = merge(&stamped("A", a, 0, "e1"), &stamped("B", b, 999_999, "e2"));
        assert!(r.is_concurrent(), "★ a huge physical gap is not causal evidence");
    }
}
