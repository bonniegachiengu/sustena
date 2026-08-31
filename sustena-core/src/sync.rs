//! **The replicated log** — the join-semilattice a transport carries.
//!
//! Multiparty §IV (Lamport ordering) and §VI (CRDTs), as the one I/O-free
//! entry point a peer transport needs. The socket, the handshake and the peer
//! book are the **host's**: ADR-0001 fixes this crate's dependencies, and a
//! socket is I/O exactly as persistence and the DTO layer are. What belongs
//! here is the part that must be identical on every node — *what merges with
//! what, and in what order it is folded* — because two nodes that disagree
//! about that do not converge no matter how good the wire is.
//!
//! ★★★ **The log is a grow-only set of immutable entries, keyed by
//! `(node, counter)`.** That is the whole of the CRDT: an entry is never
//! edited, so the join is a union, and a union of sets is commutative,
//! associative and idempotent by construction. §VI's three laws are not
//! asserted here — [`crate::crdt::laws_hold`] and [`crate::crdt::converges`]
//! are run against [`Replica`] itself in the tests, over real permutations,
//! the same treatment every other member of the family gets.
//!
//! ★★★ **`(node, counter)` is the identity, and it has to be.** A host log
//! keyed on a bare per-node sequence collides the moment a second node exists:
//! both write `seq: 5`, and a union by sequence silently drops one. Prefixing
//! the writing node's id is what makes independent writes independent — the
//! same reasoning [`CausalStamp`] already carries in its own docstring.
//!
//! ★★★ **The fold order is §IV's own sentence, executed.** *"Break ties by
//! node id and you get a total order every node computes identically from
//! local data alone."* Entries carry a scalar Lamport clock — bumped past
//! everything seen on receive — so `a → b ⟹ lamport(a) < lamport(b)`, and
//! ties fall to `(node, counter)`, which is unique. Every node therefore folds
//! the same set in the same order and lands on the same state.
//!
//! ★★ **Both clocks are carried, and each is used for what it is for.** The
//! article is explicit that the scalar clock *"manufactures an order over
//! events that were genuinely concurrent, and then cannot tell you which those
//! were"*, and that the vector clock is the instrument for recovering that.
//! So: the **scalar** orders the fold, the **vector** answers *were these two
//! concurrent* — reported by [`Reconciliation::concurrent`] rather than hidden
//! behind an order that looks decisive.
//!
//! ★★★ **The honest limit, stated in the type rather than in prose: merging
//! STATE is not merging ADMISSIBILITY.** Every entry was admitted by its own
//! node's gate against the state *that node* could see. Two concurrent spends
//! can each be admissible alone and jointly carry the merged state outside
//! `V`. Union-then-fold converges — the CRDT law is untouched — but it can
//! converge onto a state no node would have admitted. [`reconcile`] therefore
//! folds **and then re-checks the Sustain's own invariants — the same
//! predicates the gate evaluates** — and says so. It does not repair:
//! choosing which of two admitted facts to discard is a governance decision,
//! not an arithmetic one, and §V's quorum is the mechanism for the Sustains
//! that cannot tolerate the risk.
//!
//! ★★ **Equivocation is detected, not resolved away.** A node that reuses a
//! counter for two different payloads has forked its own history — §V's
//! Byzantine case in its cheapest form. The join cannot return an error
//! without breaking the lattice, so the winner is picked **deterministically**
//! (by serialised payload, so every replica picks the same one) and the
//! offending stamp is recorded in a fork set that merges alongside. The laws
//! survive; the fact does not get swallowed.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::crdt::JoinSemilattice;
use crate::event::CausalStamp;
use crate::error::FoldError;
use crate::fold::{fold_events, FoldEvent};
use crate::mutation::Mutation;
use crate::predicate;
use crate::vclock::{CausalVerdict, VectorClock};

// ---------------------------------------------------------------------------
// The entry
// ---------------------------------------------------------------------------

/// One immutable line of a replicated log.
///
/// ★ Named `LogEntry` rather than `Entry` — name-collision rule #45, the
/// newcomer takes the longer name: `juul::Entry` is a ledger line and had it
/// first.
///
/// ★ `payload` is the host's own record shape. This module never reads it —
/// it merges and orders entries, and asks the payload only for the mutations
/// it contributes, through [`Replayable`]. Keeping it generic is what stops
/// the core from acquiring an opinion about the host's log format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry<T> {
    /// `(node, counter)` — the identity. Unique by construction across nodes.
    pub stamp: CausalStamp,
    /// The scalar logical clock. **Orders the fold.**
    pub lamport: u64,
    /// What this node had seen when it wrote. **Answers concurrency.**
    pub clock: VectorClock,
    /// The writer's own wall clock, carried for display and never for ordering
    /// — there is no shared clock, which is the reason `lamport` exists.
    pub t_event: i64,
    pub payload: T,
}

impl<T> LogEntry<T> {
    /// The total-order key: §IV's rule, and nothing else.
    ///
    /// ★ `t_event` is deliberately absent. A node with a wrong wall clock
    /// would otherwise reorder everyone's history, and no node can check
    /// another's clock.
    pub fn order_key(&self) -> (u64, &str, u64) {
        (self.lamport, self.stamp.node.as_str(), self.stamp.counter)
    }

    /// `(node, counter)`, owned — the map key.
    pub fn key(&self) -> (String, u64) {
        (self.stamp.node.clone(), self.stamp.counter)
    }
}

/// A payload that can contribute to a fold.
///
/// ★ One method, because that is all the core needs of a host's log line: the
/// mutations it applies. Everything else about the line — which operator ran,
/// what it emitted, who authorised it — is the host's business.
pub trait Replayable {
    fn mutations(&self) -> &[Mutation];
}

// ---------------------------------------------------------------------------
// The replica
// ---------------------------------------------------------------------------

/// A checkable summary of what a replica holds.
///
/// ★★ Deliberately small and payload-free: it crosses the wire between nodes
/// that need not trust each other, so it carries the *identity* of what is
/// held and nothing about its contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holdings {
    /// How many entries. Not a checksum — the thing that makes an incoherent
    /// claim readable by a person.
    pub count: u64,
    /// SHA-256 over the sorted `(node, counter)` pairs.
    pub digest: String,
}

/// A node's copy of one Sustain's log.
///
/// ★★★ The join is a **union**, and that is the whole CRDT. Nothing here is
/// clever; the cleverness was in choosing an identity that makes independent
/// writes independent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replica<T> {
    entries: BTreeMap<(String, u64), LogEntry<T>>,
    /// Stamps seen carrying two different payloads. See the module docs.
    forks: BTreeSet<(String, u64)>,
}

impl<T> Default for Replica<T> {
    fn default() -> Self {
        Self { entries: BTreeMap::new(), forks: BTreeSet::new() }
    }
}

impl<T: Clone + PartialEq + Serialize> Replica<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one entry. Idempotent: re-adding an identical entry is a no-op.
    ///
    /// ★ An entry whose stamp is present with a **different** payload is an
    /// equivocation, and is recorded rather than accepted or rejected — see
    /// [`Replica::forks`].
    pub fn insert(&mut self, entry: LogEntry<T>) {
        let key = entry.key();
        match self.entries.get(&key) {
            None => {
                self.entries.insert(key, entry);
            }
            Some(existing) if *existing == entry => {}
            Some(existing) => {
                self.forks.insert(key.clone());
                if canonical(&entry) < canonical(existing) {
                    self.entries.insert(key, entry);
                }
            }
        }
    }

    pub fn contains(&self, node: &str, counter: u64) -> bool {
        self.entries.contains_key(&(node.to_string(), counter))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every entry, in the deterministic total order of §IV.
    pub fn ordered(&self) -> Vec<&LogEntry<T>> {
        let mut all: Vec<&LogEntry<T>> = self.entries.values().collect();
        all.sort_by(|a, b| a.order_key().cmp(&b.order_key()));
        all
    }

    /// Stamps that arrived carrying two different payloads — a node that
    /// forked its own history. **Never empty silently:** an empty set means
    /// none was seen, and the set merges like everything else.
    pub fn forks(&self) -> &BTreeSet<(String, u64)> {
        &self.forks
    }

    /// The frontier: the highest counter seen per node.
    ///
    /// ★ This is what a peer sends to say *what I already have*, so the reply
    /// carries only what is missing. A vector clock is exactly the right shape
    /// for it, which is why it is one.
    pub fn frontier(&self) -> VectorClock {
        let mut clock = VectorClock::new();
        for (node, counter) in self.entries.keys() {
            if *counter > clock.get(node) {
                clock = clock.at(node, *counter);
            }
        }
        clock
    }

    /// What this replica **actually holds**, as something a peer can check.
    ///
    /// ★★★ **Why a frontier is not enough.** A frontier is a claim about the
    /// highest counter seen per node, and two replicas can agree on it while
    /// holding different sets of entries — gaps are legitimate, so
    /// `max counter` says nothing about what sits below it. That gap is not
    /// theoretical: a real device sync returned a frontier its own log could
    /// not justify, and because `missing_from` had nothing to send, the round
    /// reported `sent = 0`. **A lie and a healthy idempotent sync produced the
    /// same bytes on the wire**, which is what made it cost a day to not solve.
    ///
    /// ★★ The digest closes over the sorted `(node, counter)` pairs — the
    /// identity of every entry held, and nothing about payloads. `entries` is a
    /// `BTreeMap`, so that order is the map's own and needs no sorting step;
    /// two replicas holding the same set therefore hash the same on any
    /// machine.
    ///
    /// ★ `count` rides along because it is what makes a bad claim *legible*.
    /// Digests only ever say "different". A peer asserting a frontier that
    /// reaches 283 while holding 2 entries names itself.
    pub fn holdings(&self) -> Holdings {
        use sha2::{Digest as _, Sha256};
        let mut hasher = Sha256::new();
        for (node, counter) in self.entries.keys() {
            hasher.update(node.as_bytes());
            hasher.update(b":");
            hasher.update(counter.to_be_bytes());
            hasher.update(b";");
        }
        Holdings { count: self.entries.len() as u64, digest: format!("{:x}", hasher.finalize()) }
    }

    /// Everything this replica holds that a peer at `theirs` does not.
    ///
    /// ★★ Compared per node against the frontier rather than by a global
    /// sequence, because there is no global sequence — that is the whole
    /// premise. A node absent from `theirs` contributes all of its entries.
    pub fn missing_from(&self, theirs: &VectorClock) -> Vec<&LogEntry<T>> {
        let mut out: Vec<&LogEntry<T>> = self
            .entries
            .values()
            .filter(|e| e.stamp.counter > theirs.get(&e.stamp.node))
            .collect();
        out.sort_by(|a, b| a.order_key().cmp(&b.order_key()));
        out
    }

    /// The next `(counter, lamport)` this node should write with.
    ///
    /// ★ The counter is this node's own sequence; the lamport is one past
    /// **everything** seen, which is §IV's `max(C_i, C_msg) + 1` applied to a
    /// whole log rather than to one message.
    pub fn next_write(&self, node: &str) -> (u64, u64) {
        let counter = self.frontier().get(node) + 1;
        let lamport = self.entries.values().map(|e| e.lamport).max().unwrap_or(0) + 1;
        (counter, lamport)
    }
}

/// A stable byte form for the deterministic fork tie-break.
///
/// ★ `serde_json` with this crate's `preserve_order` feature keeps insertion
/// order, so the same value serialises the same way on every node.
fn canonical<T: Serialize>(entry: &LogEntry<T>) -> String {
    serde_json::to_string(&entry.payload).unwrap_or_default()
}

impl<T: Clone + PartialEq + Serialize> JoinSemilattice for Replica<T> {
    fn join(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for entry in other.entries.values() {
            out.insert(entry.clone());
        }
        for key in &other.forks {
            out.forks.insert(key.clone());
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Reconciliation
// ---------------------------------------------------------------------------

/// One concurrent write that another concurrent write overwrote.
///
/// ★★★ **Convergence is not preservation, and this is the difference.** The
/// log is a grow-only set, so no ENTRY is ever lost — that is the CRDT law and
/// it holds unconditionally. But a `Set` mutation is an absolute assignment,
/// so two concurrent entries writing the same path both replay and the later
/// one in the fold order wins. Both nodes reach the same state (§VI's promise,
/// intact); one node's *value* is superseded.
///
/// ★★ §VI's stronger promise — *no member's number is ever overwritten by the
/// group* — belongs to the value CRDTs in [`crate::crdt`] (a `GCounter` merges
/// by per-node maximum and cannot lose a contribution). A state document of
/// arbitrary JSON is not one of those. Rather than pretend otherwise, every
/// supersession is **named**: which path, which entry won, which lost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Supersession {
    pub path: String,
    pub winner: CausalStamp,
    pub loser: CausalStamp,
}

/// A pair of entries neither of which happened before the other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Concurrent {
    pub left: CausalStamp,
    pub right: CausalStamp,
}

/// What a merged log folds to, and whether anyone should be comfortable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reconciliation {
    /// The folded state. Identical on every node holding the same entry set.
    pub state: Value,
    pub entries: usize,
    /// Pairs the vector clocks report as genuinely concurrent. Ordering them
    /// was a choice the scalar clock made; this says so.
    pub concurrent: Vec<Concurrent>,
    /// Stamps carrying two payloads — equivocation, surfaced.
    pub forks: Vec<CausalStamp>,
    /// ★★★ Concurrent writes to one path, where the fold order decided which
    /// value survives. **Empty is a real answer**: it means no two concurrent
    /// entries touched the same place, so nothing was overwritten.
    pub superseded: Vec<Supersession>,
    /// ★★★ Whether the merged state satisfies the Sustain's own invariants —
    /// **the same predicates the gate checks**, so this answers *would this
    /// state have been admitted* rather than a different question that merely
    /// sounds like it. `None` when the caller supplied none to check against:
    /// *unmeasured*, which is not the same as *fine*, and the type says which.
    pub admissible: Option<bool>,
    /// The invariants the merged state breaks, as `(id, why)`. Empty when
    /// admissible, and also empty when unmeasured — read it with `admissible`.
    pub violated: Vec<(String, String)>,
}

impl Reconciliation {
    /// ★★ A merge that converged onto a state no single node would have
    /// admitted. Not an error and not a repair — a fact a surface must be able
    /// to say out loud.
    pub fn admissible_gap(&self) -> bool {
        self.admissible == Some(false)
    }

    pub fn describe(&self) -> String {
        let mut parts = vec![format!("{} entries folded", self.entries)];
        if !self.concurrent.is_empty() {
            parts.push(format!("{} concurrent pair(s), ordered by node id", self.concurrent.len()));
        }
        if !self.forks.is_empty() {
            parts.push(format!("{} forked stamp(s)", self.forks.len()));
        }
        if !self.superseded.is_empty() {
            parts.push(format!(
                "{} value(s) superseded ({}) — the entries all survive; these values did not",
                self.superseded.len(),
                self.superseded
                    .iter()
                    .map(|s| s.path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        match self.admissible {
            None => parts.push("nothing to check against — admissibility unmeasured".into()),
            Some(true) => parts.push("every invariant still holds".into()),
            Some(false) => parts.push(format!(
                "OUTSIDE the viable region — {} — every entry was admitted locally; the merge was not",
                self.violated
                    .iter()
                    .map(|(id, why)| format!("{id}: {why}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        }
        parts.join(" · ")
    }
}

/// Fold a merged replica, and report what the fold could not decide.
///
/// ★★★ Deterministic by construction: the order is [`LogEntry::order_key`], which
/// every node computes from the entries alone. Two nodes holding the same set
/// produce the same `state`, byte for byte, whatever order the entries
/// arrived in — which is the property the transport exists to deliver.
pub fn reconcile<T: Clone + PartialEq + Serialize + Replayable>(
    replica: &Replica<T>,
    initial: Option<Value>,
    invariants: Option<&[(String, String)]>,
) -> Result<Reconciliation, FoldError> {
    let ordered = replica.ordered();

    let events: Vec<FoldEvent> = ordered
        .iter()
        .map(|e| FoldEvent { mutations: e.payload.mutations().to_vec() })
        .collect();
    let state = fold_events(&events, initial)?;

    // Concurrency is reported, not resolved: the scalar clock already put
    // these in an order, and this is the part that order cannot justify.
    let mut concurrent = Vec::new();
    for (i, a) in ordered.iter().enumerate() {
        for b in ordered.iter().skip(i + 1) {
            if a.stamp.node != b.stamp.node
                && a.clock.compare(&b.clock) == CausalVerdict::Concurrent
            {
                concurrent.push(Concurrent {
                    left: a.stamp.clone(),
                    right: b.stamp.clone(),
                });
            }
        }
    }

    // ★★★ The SAME predicates the gate evaluates, through the same
    //     `predicate::check`. Checking anything else here — a `Region`, a
    //     schema, a heuristic — would answer a question that merely sounds
    //     like *would this have been admitted*.
    let (admissible, violated) = match invariants {
        None => (None, Vec::new()),
        Some(rules) => {
            let params = serde_json::Map::new();
            let mut broken = Vec::new();
            for (id, expr) in rules {
                match predicate::check(expr, &state, &params) {
                    // ★ A rule that will not parse is a BREACH, not a skip — the
                    //   gate's own fail-safe rule, applied here for the same
                    //   reason: skipping is how a check fails open.
                    Err(e) => broken.push((id.clone(), format!("could not be parsed: {e}"))),
                    Ok((false, why)) => broken.push((id.clone(), why)),
                    Ok((true, _)) => {}
                }
            }
            (Some(broken.is_empty()), broken)
        }
    };

    // ★★★ Which concurrent write lost. Only pairs the clocks call CONCURRENT
    //     count: a later write that genuinely saw the earlier one is an
    //     update, not a supersession, and calling it one would cry wolf on
    //     every ordinary edit.
    let mut superseded = Vec::new();
    for (i, a) in ordered.iter().enumerate() {
        for b in ordered.iter().skip(i + 1) {
            if a.stamp.node == b.stamp.node
                || a.clock.compare(&b.clock) != CausalVerdict::Concurrent
            {
                continue;
            }
            for path in touched(a.payload.mutations()) {
                if touched(b.payload.mutations()).contains(&path) {
                    superseded.push(Supersession {
                        path,
                        // `b` is later in the fold order, so `b` wins.
                        winner: b.stamp.clone(),
                        loser: a.stamp.clone(),
                    });
                }
            }
        }
    }

    let forks = replica
        .forks()
        .iter()
        .map(|(node, counter)| CausalStamp { counter: *counter, node: node.clone() })
        .collect();

    Ok(Reconciliation {
        state,
        entries: ordered.len(),
        concurrent,
        forks,
        superseded,
        admissible,
        violated,
    })
}

/// Every path a mutation list writes to. ★ `ReplaceRoot` writes everything,
/// so it is reported as the root — a genesis that lands concurrently with an
/// edit really does supersede it.
fn touched(mutations: &[Mutation]) -> Vec<String> {
    mutations
        .iter()
        .map(|m| match m {
            Mutation::Set { path, .. } => path.clone(),
            Mutation::Append { path, .. } => path.clone(),
            Mutation::Remove { path, .. } => path.clone(),
            // ★ Both write the whole document, so neither has a path.
            Mutation::ReplaceRoot { .. } | Mutation::JoinRoot { .. } => String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::{converges, laws_hold};
    use serde_json::json;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Line {
        mutations: Vec<Mutation>,
    }

    impl Replayable for Line {
        fn mutations(&self) -> &[Mutation] {
            &self.mutations
        }
    }

    fn set(path: &str, v: Value) -> Line {
        Line { mutations: vec![Mutation::Set { path: path.into(), old: Value::Null, new: v }] }
    }

    fn root(v: Value) -> Line {
        Line { mutations: vec![Mutation::ReplaceRoot { value: v }] }
    }

    fn entry(node: &str, counter: u64, lamport: u64, payload: Line) -> LogEntry<Line> {
        let mut clock = VectorClock::new();
        clock = clock.at(node, counter);
        LogEntry {
            stamp: CausalStamp { counter, node: node.into() },
            lamport,
            clock,
            t_event: 0,
            payload,
        }
    }

    fn replica(entries: Vec<LogEntry<Line>>) -> Replica<Line> {
        let mut r = Replica::new();
        for e in entries {
            r.insert(e);
        }
        r
    }

    // ── the lattice ─────────────────────────────────────────────────────────

    #[test]
    fn the_log_is_a_join_semilattice() {
        // ★★★ Not asserted — checked by the same `laws_hold` every other CRDT
        //     in this crate is checked by.
        let a = replica(vec![entry("a", 1, 1, set("x", json!(1)))]);
        let b = replica(vec![entry("b", 1, 1, set("y", json!(2)))]);
        let c = replica(vec![entry("c", 1, 1, set("z", json!(3)))]);
        let report = laws_hold(&a, &b, &c);
        assert!(report.all_hold(), "{}", report.describe());
    }

    #[test]
    fn arrival_order_and_duplication_cannot_change_the_result() {
        // §VI's own sentence, over the real log type rather than a counter.
        let updates = vec![
            replica(vec![entry("a", 1, 1, set("x", json!(1)))]),
            replica(vec![entry("b", 1, 1, set("y", json!(2)))]),
            replica(vec![entry("a", 2, 3, set("x", json!(9)))]),
        ];
        let report = converges(&updates).expect("permutable");
        assert!(report.strongly_eventually_consistent(), "{report:?}");
    }

    #[test]
    fn two_nodes_writing_the_same_counter_do_not_collide() {
        // ★★★ The reason the identity is `(node, counter)` and not `counter`.
        let merged = replica(vec![entry("a", 1, 1, set("x", json!(1)))])
            .join(&replica(vec![entry("b", 1, 1, set("y", json!(2)))]));
        assert_eq!(merged.len(), 2, "a union by bare sequence would have dropped one");
    }

    // ── the order ───────────────────────────────────────────────────────────

    #[test]
    fn the_fold_order_is_identical_on_both_sides() {
        let a = entry("alice", 1, 1, set("x", json!(1)));
        let b = entry("bob", 1, 1, set("y", json!(2)));
        let c = entry("alice", 2, 5, set("x", json!(3)));

        let left = replica(vec![a.clone(), b.clone(), c.clone()]);
        let right = replica(vec![c, b, a]); // arrived in the opposite order

        let keys = |r: &Replica<Line>| -> Vec<String> {
            r.ordered().iter().map(|e| format!("{}:{}", e.stamp.node, e.stamp.counter)).collect()
        };
        assert_eq!(keys(&left), keys(&right));
        // Lamport first, then node id — §IV's tie-break, visible.
        assert_eq!(keys(&left), vec!["alice:1", "bob:1", "alice:2"]);
    }

    #[test]
    fn a_wrong_wall_clock_cannot_reorder_anyone() {
        // ★★ `t_event` is not in the order key, deliberately: no node can
        //    check another's clock, so trusting it would hand any node the
        //    ability to reorder the shared history.
        let mut liar = entry("bob", 1, 2, set("y", json!(2)));
        liar.t_event = i64::MIN;
        let r = replica(vec![entry("alice", 1, 1, set("x", json!(1))), liar]);
        let first = &r.ordered()[0].stamp;
        assert_eq!(first.node, "alice", "the liar's clock bought it nothing");
    }

    #[test]
    fn causality_is_respected_by_the_scalar_clock() {
        // a → b forces lamport(a) < lamport(b), so a caused entry never folds
        // before its cause however the ids sort.
        let cause = entry("zeta", 1, 1, set("x", json!(1)));
        let effect = entry("alpha", 1, 2, set("x", json!(2)));
        let r = replica(vec![effect, cause]);
        let out = fold(&r);
        assert_eq!(out["x"], json!(2), "the effect folded last despite 'alpha' < 'zeta'");
    }

    // ── convergence, end to end ─────────────────────────────────────────────

    fn fold(r: &Replica<Line>) -> Value {
        reconcile(r, None, None).expect("folds").state
    }

    #[test]
    fn two_replicas_that_exchange_reach_the_same_state() {
        let genesis = entry("alice", 1, 1, root(json!({"balance": 100, "note": ""})));

        // Concurrent local writes, neither having seen the other.
        let mut alice = replica(vec![genesis.clone(), entry("alice", 2, 2, set("balance", json!(70)))]);
        let mut bob = replica(vec![genesis, entry("bob", 1, 2, set("note", json!("rent")))]);
        assert_ne!(fold(&alice), fold(&bob), "they genuinely diverged first");

        // Each hands the other exactly what it lacks.
        let for_bob: Vec<LogEntry<Line>> =
            alice.missing_from(&bob.frontier()).into_iter().cloned().collect();
        let for_alice: Vec<LogEntry<Line>> =
            bob.missing_from(&alice.frontier()).into_iter().cloned().collect();
        assert_eq!(for_bob.len(), 1, "only the missing one crosses the wire");
        assert_eq!(for_alice.len(), 1);
        for e in for_bob {
            bob.insert(e);
        }
        for e in for_alice {
            alice.insert(e);
        }

        assert_eq!(fold(&alice), fold(&bob));
        assert_eq!(fold(&alice), json!({"balance": 70, "note": "rent"}));
    }

    #[test]
    fn syncing_twice_changes_nothing() {
        let mut alice = replica(vec![entry("alice", 1, 1, root(json!({"n": 0})))]);
        let bob = replica(vec![entry("bob", 1, 2, set("n", json!(5)))]);
        for _ in 0..3 {
            for e in bob.ordered() {
                alice.insert(e.clone());
            }
        }
        assert_eq!(alice.len(), 2);
        assert_eq!(fold(&alice), json!({"n": 5}));
    }

    #[test]
    fn the_frontier_is_what_a_peer_asks_with() {
        let r = replica(vec![
            entry("alice", 1, 1, set("x", json!(1))),
            entry("alice", 2, 2, set("x", json!(2))),
            entry("bob", 1, 1, set("y", json!(1))),
        ]);
        let f = r.frontier();
        assert_eq!(f.get("alice"), 2);
        assert_eq!(f.get("bob"), 1);
        assert_eq!(f.get("carol"), 0, "a node never heard of is at zero, honestly");

        // A peer that has all of alice but none of bob gets exactly bob's.
        let mut theirs = VectorClock::new();
        theirs = theirs.at("alice", 2);
        let missing = r.missing_from(&theirs);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].stamp.node, "bob");
    }

    #[test]
    fn next_write_advances_past_everything_seen() {
        let mut r = replica(vec![entry("alice", 1, 1, set("x", json!(1)))]);
        r.insert(entry("bob", 1, 7, set("y", json!(1))));
        let (counter, lamport) = r.next_write("alice");
        assert_eq!(counter, 2, "this node's own sequence");
        assert_eq!(lamport, 8, "one past everything, including bob's");
    }

    // ── what the fold cannot decide ─────────────────────────────────────────

    #[test]
    fn concurrency_is_reported_rather_than_hidden() {
        let mut a_clock = VectorClock::new();
        a_clock = a_clock.at("alice", 1);
        let mut b_clock = VectorClock::new();
        b_clock = b_clock.at("bob", 1);

        let r = replica(vec![
            LogEntry {
                stamp: CausalStamp { counter: 1, node: "alice".into() },
                lamport: 1,
                clock: a_clock,
                t_event: 0,
                payload: root(json!({"x": 0})),
            },
            LogEntry {
                stamp: CausalStamp { counter: 1, node: "bob".into() },
                lamport: 1,
                clock: b_clock,
                t_event: 0,
                payload: set("x", json!(1)),
            },
        ]);
        let out = reconcile(&r, None, None).expect("folds");
        assert_eq!(out.concurrent.len(), 1, "the order the scalar clock chose was a choice");
        assert!(out.describe().contains("concurrent"));
    }

    #[test]
    fn equivocation_is_recorded_and_the_laws_still_hold() {
        // ★★ Two payloads under one stamp: a node forking its own history.
        let honest = entry("bob", 1, 1, set("x", json!(1)));
        let forged = entry("bob", 1, 1, set("x", json!(999)));

        let one = replica(vec![honest.clone(), forged.clone()]);
        let other = replica(vec![forged, honest]); // opposite arrival order
        assert_eq!(one, other, "the winner is deterministic, so the join stays commutative");
        assert_eq!(one.forks().len(), 1, "and the fork is not swallowed");

        let out = reconcile(&one, None, None).expect("folds");
        assert_eq!(out.forks.len(), 1);
        assert!(out.describe().contains("forked"));
    }

    #[test]
    fn a_merge_can_converge_onto_a_state_no_node_would_have_admitted() {
        // ★★★ The property this module exists to be honest about. Two nodes
        //     each spend from a balance of 100; each spend is admissible where
        //     it was made. The merge is neither.
        let genesis = entry("alice", 1, 1, root(json!({"balance": 100.0})));
        let a_spend = entry("alice", 2, 2, set("balance", json!(30.0)));
        let b_spend = entry("bob", 1, 2, set("balance", json!(-40.0)));
        let r = replica(vec![genesis, a_spend, b_spend]);

        let rules = [("solvent".to_string(), "balance >= 0".to_string())];

        let out = reconcile(&r, None, Some(&rules)).expect("folds");
        assert_eq!(out.admissible, Some(false));
        assert!(out.admissible_gap());
        assert_eq!(out.violated.len(), 1);
        assert_eq!(out.violated[0].0, "solvent");
        assert!(out.describe().contains("every entry was admitted locally"));
        // And it did NOT repair: the state is the fold, unedited.
        assert_eq!(out.state["balance"], json!(-40.0));
    }

    #[test]
    fn a_superseded_value_is_named_rather_than_quietly_dropped() {
        // ★★★ The difference between convergence and preservation. Both nodes
        //     write the same path while unaware of each other; the fold order
        //     picks one, and the other is REPORTED rather than vanishing.
        let mut a_clock = VectorClock::new();
        a_clock = a_clock.at("alice", 2);
        let mut b_clock = VectorClock::new();
        b_clock = b_clock.at("bob", 1);

        let r = replica(vec![
            entry("alice", 1, 1, root(json!({"balance": 0}))),
            LogEntry {
                stamp: CausalStamp { counter: 2, node: "alice".into() },
                lamport: 2,
                clock: a_clock,
                t_event: 0,
                payload: set("balance", json!(300)),
            },
            LogEntry {
                stamp: CausalStamp { counter: 1, node: "bob".into() },
                lamport: 3,
                clock: b_clock,
                t_event: 0,
                payload: set("balance", json!(700)),
            },
        ]);
        let out = reconcile(&r, None, None).expect("folds");
        // Converged, deterministically — the law is untouched.
        assert_eq!(out.state["balance"], json!(700));
        // And the loss is named.
        let lost: Vec<&Supersession> =
            out.superseded.iter().filter(|s| s.path == "balance").collect();
        assert_eq!(lost.len(), 1);
        assert_eq!(lost[0].winner.node, "bob");
        assert_eq!(lost[0].loser.node, "alice");
        assert!(out.describe().contains("superseded"));
    }

    #[test]
    fn a_write_that_saw_the_earlier_one_is_an_update_not_a_supersession() {
        // ★★ Only genuinely concurrent pairs count. An ordinary sequence of
        //    edits to one path must not be reported as loss, or the report
        //    cries wolf on every household that spends twice.
        let r = replica(vec![
            entry("alice", 1, 1, root(json!({"balance": 0}))),
            entry("alice", 2, 2, set("balance", json!(300))),
            entry("alice", 3, 3, set("balance", json!(500))),
        ]);
        let out = reconcile(&r, None, None).expect("folds");
        assert_eq!(out.state["balance"], json!(500));
        assert!(out.superseded.is_empty());
    }

    #[test]
    fn unmeasured_is_not_the_same_as_fine() {
        let r = replica(vec![entry("alice", 1, 1, root(json!({"balance": -5.0})))]);
        let out = reconcile(&r, None, None).expect("folds");
        assert_eq!(out.admissible, None, "nothing to check against, so nothing is claimed");
        assert!(!out.admissible_gap(), "and unmeasured is not reported as a breach either");
        assert!(out.describe().contains("unmeasured"));
    }

    #[test]
    fn a_clean_merge_says_so() {
        let r = replica(vec![
            entry("alice", 1, 1, root(json!({"balance": 100.0}))),
            entry("bob", 1, 2, set("balance", json!(40.0))),
        ]);
        let rules = [("solvent".to_string(), "balance >= 0".to_string())];
        let out = reconcile(&r, None, Some(&rules)).expect("folds");
        assert_eq!(out.admissible, Some(true));
        assert!(out.violated.is_empty());
    }

    // ── holdings ────────────────────────────────────────────────────────
    //
    // ★★★ A frontier is a claim about the highest counter per node. Two
    //     replicas can agree on it while holding different entries, because
    //     gaps below the maximum are legitimate. These are the tests for the
    //     summary that closes that gap.

    #[test]
    fn the_same_entries_hash_the_same_whatever_order_they_arrived_in() {
        // ★★★ The property the whole check rests on. Two nodes reach the same
        //     set by different routes -- one merged, one wrote -- and if the
        //     digest depended on arrival order they would accuse each other
        //     of divergence forever.
        let a = replica(vec![
            entry("alice", 1, 1, set("x", Value::from(1))),
            entry("bob", 1, 2, set("y", Value::from(2))),
            entry("alice", 2, 3, set("z", Value::from(3))),
        ]);
        let b = replica(vec![
            entry("alice", 2, 3, set("z", Value::from(3))),
            entry("alice", 1, 1, set("x", Value::from(1))),
            entry("bob", 1, 2, set("y", Value::from(2))),
        ]);
        assert_eq!(a.holdings(), b.holdings());
        assert_eq!(a.holdings().count, 3);
    }

    #[test]
    fn a_gap_below_the_frontier_changes_the_holdings_but_not_the_frontier() {
        // ★★★ Exactly the shape the device trace had: same frontier, fewer
        //     entries. The frontier cannot tell these apart, which is why it
        //     was believed. The digest can.
        let full = replica(vec![
            entry("alice", 1, 1, set("x", Value::from(1))),
            entry("alice", 2, 2, set("y", Value::from(2))),
            entry("alice", 3, 3, set("z", Value::from(3))),
        ]);
        let gappy = replica(vec![
            entry("alice", 1, 1, set("x", Value::from(1))),
            entry("alice", 3, 3, set("z", Value::from(3))),
        ]);
        assert_eq!(full.frontier(), gappy.frontier(), "the frontier cannot see the gap");
        assert_ne!(full.holdings(), gappy.holdings(), "the holdings can");
        assert_eq!(full.holdings().count, 3);
        assert_eq!(gappy.holdings().count, 2);
    }

    #[test]
    fn an_empty_replica_has_holdings_rather_than_nothing() {
        // ★ A node with no entries still makes a checkable claim. Treating
        //   empty as "no answer" would exempt exactly the node most likely to
        //   be wrong about what it holds.
        let h = Replica::<Line>::new().holdings();
        assert_eq!(h.count, 0);
        assert_eq!(h.digest.len(), 64);
    }
}
