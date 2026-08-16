//! The wider CRDT family — join-semilattices with a LUB merge
//! (Multiparty §VI · MUL-10).
//!
//! > **Conflict-free Replicated Data Types** (Shapiro et al., 2011) let
//! > independent replicas converge with *no coordination at all*, provided the
//! > state forms a join-semilattice and merges take the least upper bound:
//!
//! ```text
//! s ⊔ t = t ⊔ s        (s ⊔ t) ⊔ u = s ⊔ (t ⊔ u)        s ⊔ s = s
//! ```
//!
//! > Commutative, associative, idempotent — **which is exactly the statement
//! > that arrival order, duplication, and retry cannot change the result.**
//! > Monotone merges ⇒ **strong eventual consistency**: same set of updates,
//! > same state, order-independent.
//!
//! ## ★ The three laws are not documentation — they are the proof obligation
//!
//! [`laws_hold`] checks all three on any three values of any member of the
//! family, and [`converges`] does the stronger thing the article's sentence
//! actually claims: it folds **every permutation** of a delivery set, and
//! folds each permutation again with **every update duplicated**, and asserts
//! a single result. Arrival order, duplication and retry, tested rather than
//! asserted — the discipline the R1 LWW register already used, generalised
//! over the family by a trait.
//!
//! ## ★ "Never erase the node" IS the merge
//!
//! > Because the group value is a *merge over member entries* and never an
//! > overwrite of them, the member's own number survives every aggregation by
//! > construction. The holon invariant — never erase the node — is not a policy
//! > sitting on top of the merge. It **is** the merge.
//!
//! [`GCounter::join`] is per-node `max`, and `max` is monotone, so
//! `join(a, b)[id] ≥ a[id]` for **every** id and every pair — a merge cannot
//! lower anyone's entry, whatever the other side says. That is asserted
//! directly across every permutation rather than described, and it is the same
//! never-erase property [`crate::vclock`] gives a concurrent edit and
//! [`crate::disaggregation`] gives a departing member.
//!
//! ## The roll-up correspondence, and where it stops being exact
//!
//! §VI: *"That is exactly a shared-pocket roll-up: each person's pocket is
//! their own entry, the group total is the merge, and no member's number is
//! ever overwritten by the group."* [`GCounter::from_entries`] takes exactly
//! the `included: [{member, value}]` shape the reference roll-up already
//! returns, and [`GCounter::value`] is its total.
//!
//! **It is exact only where the quantity is grow-only**, which §VI says in the
//! name. A pocket balance that can fall is not a G-counter — a `max` merge
//! would silently discard the decrease. That is why [`PnCounter`] is here: two
//! G-counters, one up and one down, which is the standard family answer and
//! the honest completion of the correspondence rather than a claim that the
//! roll-up is already a CRDT.
//!
//! ## ★ Where this makes a clock unnecessary
//!
//! The previous slice gave a concurrent pair two honest readings:
//! [`crate::vclock::Dimension::Snapshot`] collapses by physical order, and
//! `Contested` keeps both because collapsing would lose an edit. A CRDT needs
//! **neither**. [`merge_stamped`] takes the LUB regardless of the causal
//! verdict, and a test asserts the causally-ordered case and the concurrent
//! case go through the *same* code path with the same guarantee — because
//! `a ⊔ successor(a) = successor(a)`, so the LUB agrees with LWW exactly where
//! LWW is right and loses nothing exactly where LWW is wrong.
//!
//! The verdict is still reported, because *why* two replicas differed remains
//! worth knowing even when the answer no longer depends on it.
//!
//! ## The CAP tax (§VI), named
//!
//! > CAP (Gilbert & Lynch, 2002) names the tax — under partition you pick
//! > consistency or availability — and CRDTs choose **availability plus
//! > eventual convergence**, which is the right call for a household that must
//! > keep working offline.
//!
//! So a replica here never blocks and never refuses an update for want of a
//! peer. What it does not offer is a read that is up to date with a partner it
//! has not heard from — and no operation in this module pretends otherwise.
//!
//! ## Honest limits
//!
//! - **No RNG in this crate**, so a unique tag or element id is *caller
//!   supplied* as `(node, seq)`. That is not a workaround: a replica already
//!   has its own id and a monotone counter, which is precisely what
//!   uniqueness needs, and it keeps the core reproducible.
//! - **Tombstones are never collected.** [`OrSet`] keeps removed tags and
//!   [`Rga`] keeps removed elements, because a tag that vanished could not
//!   suppress a duplicate delivery and an element that vanished would orphan a
//!   concurrent insert anchored to it. Monotone growth is what buys
//!   convergence; unbounded growth is what it costs, and garbage collection
//!   needs a causal-stability barrier this module does not have.
//! - **[`converges`] is exhaustive over permutations** and refuses above
//!   [`PERMUTATION_LIMIT`] rather than sampling — a convergence proof over a
//!   sample is not a proof.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::vclock::{CausalVerdict, Stamped};

/// [`converges`] refuses above this many updates rather than sampling
/// permutations. `7! = 5040`; a convergence claim checked over a sample is not
/// a convergence claim.
pub const PERMUTATION_LIMIT: usize = 7;

// ---------------------------------------------------------------------------
// The lattice
// ---------------------------------------------------------------------------

/// A join-semilattice with a least-upper-bound merge (§VI).
///
/// Implementing this is a claim that [`laws_hold`] passes. Nothing enforces
/// that at compile time — which is why every member of the family in this
/// module has the laws checked against it by a test, over real permutations,
/// rather than by assertion in a doc comment.
pub trait JoinSemilattice: Clone + PartialEq {
    /// `s ⊔ t` — the least upper bound.
    fn join(&self, other: &Self) -> Self;

    /// `s ⊑ t ⟺ s ⊔ t = t`. The lattice order, derived from the join rather
    /// than declared beside it, so the two cannot disagree.
    fn leq(&self, other: &Self) -> bool {
        self.join(other) == *other
    }
}

/// The three laws, checked on one triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LawReport {
    pub commutative: bool,
    pub associative: bool,
    pub idempotent: bool,
}

impl LawReport {
    pub fn all_hold(&self) -> bool {
        self.commutative && self.associative && self.idempotent
    }

    pub fn describe(&self) -> String {
        if self.all_hold() {
            return "commutative ∧ associative ∧ idempotent".to_string();
        }
        let mut broken = Vec::new();
        for (name, ok) in [
            ("commutative", self.commutative),
            ("associative", self.associative),
            ("idempotent", self.idempotent),
        ] {
            if !ok {
                broken.push(name);
            }
        }
        format!("NOT a join-semilattice — {} fails", broken.join(" and "))
    }
}

/// `s ⊔ t = t ⊔ s`, `(s ⊔ t) ⊔ u = s ⊔ (t ⊔ u)`, `s ⊔ s = s` — on one triple.
pub fn laws_hold<T: JoinSemilattice>(a: &T, b: &T, c: &T) -> LawReport {
    LawReport {
        commutative: a.join(b) == b.join(a),
        associative: a.join(b).join(c) == a.join(&b.join(c)),
        idempotent: a.join(a) == *a,
    }
}

/// What [`converges`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvergenceReport {
    /// How many distinct orderings were folded.
    pub orderings: usize,
    /// ★ Every ordering produced the same state.
    pub order_independent: bool,
    /// ★ Folding an ordering with every update delivered **twice** produced
    /// the same state — duplication and retry are free.
    pub duplication_free: bool,
}

impl ConvergenceReport {
    /// Strong eventual consistency for this update set: same updates, same
    /// state, whatever the order and however many times each arrived.
    pub fn strongly_eventually_consistent(&self) -> bool {
        self.order_independent && self.duplication_free
    }
}

/// **The article's sentence, executed.**
///
/// Folds *every* permutation of `updates`, then folds every permutation again
/// with each update delivered twice, and reports whether one state came out of
/// all of it. This is the whole of *"arrival order, duplication, and retry
/// cannot change the result"* — checked, not claimed.
pub fn converges<T: JoinSemilattice>(updates: &[T]) -> Result<ConvergenceReport, CrdtError> {
    if updates.len() < 2 {
        return Err(CrdtError::NotEnoughUpdates(updates.len()));
    }
    if updates.len() > PERMUTATION_LIMIT {
        return Err(CrdtError::TooManyToPermute {
            count: updates.len(),
            limit: PERMUTATION_LIMIT,
        });
    }

    let mut index: Vec<usize> = (0..updates.len()).collect();
    let mut orders = Vec::new();
    permute(&mut index, 0, &mut orders);

    let fold = |order: &[usize], twice: bool| -> T {
        let mut acc = updates[order[0]].clone();
        for &i in order {
            acc = acc.join(&updates[i]);
            if twice {
                acc = acc.join(&updates[i]);
            }
        }
        acc
    };

    let reference = fold(&orders[0], false);
    let mut order_independent = true;
    let mut duplication_free = true;
    for order in &orders {
        if fold(order, false) != reference {
            order_independent = false;
        }
        if fold(order, true) != reference {
            duplication_free = false;
        }
    }

    Ok(ConvergenceReport {
        orderings: orders.len(),
        order_independent,
        duplication_free,
    })
}

fn permute(items: &mut Vec<usize>, k: usize, out: &mut Vec<Vec<usize>>) {
    if k == items.len() {
        out.push(items.clone());
        return;
    }
    for i in k..items.len() {
        items.swap(k, i);
        permute(items, k + 1, out);
        items.swap(k, i);
    }
}

// ---------------------------------------------------------------------------
// G-counter — §VI's own worked example
// ---------------------------------------------------------------------------

/// The grow-only counter — §VI's own pseudocode, and the shape of a
/// shared-pocket roll-up.
///
/// ```text
/// merge(a, b):   FOR each node_id id:  out[id] = max(a[id], b[id])
/// value(g):      RETURN Σ g[id]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GCounter {
    entries: BTreeMap<String, u64>,
}

impl GCounter {
    pub fn new() -> Self {
        Self::default()
    }

    /// **The roll-up correspondence, literally.** Takes the
    /// `included: [{member, value}]` shape the composition roll-up already
    /// returns — each member's own entry, kept as their own entry.
    pub fn from_entries(entries: &[(&str, u64)]) -> Self {
        Self {
            entries: entries.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        }
    }

    /// A node's own contribution. Only that node ever raises it.
    ///
    /// Overflow is refused rather than wrapped or saturated: a counter that
    /// silently stopped counting would converge on a wrong number, which is
    /// worse than not converging.
    pub fn increment(&self, node: &str, by: u64) -> Result<Self, CrdtError> {
        let current = self.of(node);
        let next = current
            .checked_add(by)
            .ok_or_else(|| CrdtError::CounterOverflow(node.to_string()))?;
        let mut out = self.clone();
        out.entries.insert(node.to_string(), next);
        Ok(out)
    }

    pub fn of(&self, node: &str) -> u64 {
        self.entries.get(node).copied().unwrap_or(0)
    }

    /// `Σ g[id]` — the group total.
    ///
    /// Widened to `u128` so summing entries that are each individually legal
    /// cannot overflow the answer.
    pub fn value(&self) -> u128 {
        self.entries.values().map(|v| *v as u128).sum()
    }

    pub fn nodes(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }

    pub fn entries(&self) -> &BTreeMap<String, u64> {
        &self.entries
    }
}

impl JoinSemilattice for GCounter {
    /// Per-node `max` — and `max` is monotone, which is the whole of *never
    /// erase the node*: no merge can lower anybody's entry.
    fn join(&self, other: &Self) -> Self {
        let mut entries = self.entries.clone();
        for (node, v) in &other.entries {
            let slot = entries.entry(node.clone()).or_insert(0);
            *slot = (*slot).max(*v);
        }
        Self { entries }
    }
}

// ---------------------------------------------------------------------------
// PN-counter — the honest completion of the roll-up correspondence
// ---------------------------------------------------------------------------

/// A counter that can also go **down** — two G-counters, one up and one down.
///
/// Here because the roll-up correspondence demanded it, not because §VI names
/// it. §VI's example is *grow-only* by name, and a real pocket balance falls;
/// merging a falling quantity with `max` would silently discard the decrease.
/// Splitting it into two monotone halves is the standard family answer
/// (Shapiro et al.), and it keeps every claim about the roll-up honest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PnCounter {
    up: GCounter,
    down: GCounter,
}

impl PnCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn increment(&self, node: &str, by: u64) -> Result<Self, CrdtError> {
        Ok(Self {
            up: self.up.increment(node, by)?,
            down: self.down.clone(),
        })
    }

    pub fn decrement(&self, node: &str, by: u64) -> Result<Self, CrdtError> {
        Ok(Self {
            up: self.up.clone(),
            down: self.down.increment(node, by)?,
        })
    }

    /// `Σ up − Σ down`. Signed, because the answer legitimately can be.
    pub fn value(&self) -> i128 {
        self.up.value() as i128 - self.down.value() as i128
    }

    /// This node's own net contribution — still its own, still unerasable.
    pub fn of(&self, node: &str) -> i128 {
        self.up.of(node) as i128 - self.down.of(node) as i128
    }
}

impl JoinSemilattice for PnCounter {
    fn join(&self, other: &Self) -> Self {
        Self {
            up: self.up.join(&other.up),
            down: self.down.join(&other.down),
        }
    }
}

// ---------------------------------------------------------------------------
// OR-set — observed-remove
// ---------------------------------------------------------------------------

/// A unique tag for one add.
///
/// `(node, seq)` rather than a random id, because this crate has **no random
/// source** — and a replica already has its own id and a monotone counter,
/// which is exactly what uniqueness needs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tag {
    pub seq: u64,
    pub node: String,
}

impl Tag {
    pub fn new(node: &str, seq: u64) -> Self {
        Self {
            seq,
            node: node.to_string(),
        }
    }
}

/// The observed-remove set (Shapiro et al., 2011 — the citation §VI makes,
/// rather than §VI's own prose, which names only the counter).
///
/// An element is present iff it carries an add-tag that has not been
/// observed-removed. A remove only ever tombstones the tags the removing
/// replica had **seen**, which is what makes a concurrent add-and-remove
/// resolve deterministically: the concurrent add minted a tag the remover
/// never observed, so **add wins** — not by a tie-break rule, but because the
/// remove was never about that add.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrSet {
    adds: BTreeMap<String, BTreeSet<Tag>>,
    removes: BTreeSet<Tag>,
}

impl OrSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuses a reused tag. Two adds sharing a tag would let one remove
    /// silently kill both, which is the exact failure observed-remove exists
    /// to prevent — so it is refused where it is a mistake rather than
    /// discovered where it is a lost element.
    pub fn add(&self, element: &str, tag: Tag) -> Result<Self, CrdtError> {
        if self.adds.values().any(|tags| tags.contains(&tag)) || self.removes.contains(&tag) {
            return Err(CrdtError::TagReused {
                node: tag.node,
                seq: tag.seq,
            });
        }
        let mut out = self.clone();
        out.adds.entry(element.to_string()).or_default().insert(tag);
        Ok(out)
    }

    /// Tombstones every tag for this element **that this replica has
    /// observed**. A tag it has not seen is untouched, by definition.
    pub fn remove(&self, element: &str) -> Self {
        let mut out = self.clone();
        if let Some(tags) = self.adds.get(element) {
            for t in tags {
                out.removes.insert(t.clone());
            }
        }
        out
    }

    /// Present iff some add-tag survives the observed removes.
    pub fn contains(&self, element: &str) -> bool {
        self.adds
            .get(element)
            .is_some_and(|tags| tags.iter().any(|t| !self.removes.contains(t)))
    }

    pub fn elements(&self) -> Vec<&str> {
        self.adds
            .keys()
            .filter(|e| self.contains(e))
            .map(|s| s.as_str())
            .collect()
    }

    /// The surviving tags for an element — the evidence behind `contains`.
    pub fn live_tags(&self, element: &str) -> Vec<&Tag> {
        self.adds
            .get(element)
            .map(|tags| tags.iter().filter(|t| !self.removes.contains(t)).collect())
            .unwrap_or_default()
    }

    /// Tombstones kept forever, and how many. Growth is the price of
    /// convergence; reporting it is how it stays revisitable.
    pub fn tombstones(&self) -> usize {
        self.removes.len()
    }
}

impl JoinSemilattice for OrSet {
    /// Union of adds, union of removes — two grow-only sets, so all three laws
    /// come from set union rather than from a rule anyone has to remember.
    fn join(&self, other: &Self) -> Self {
        let mut adds = self.adds.clone();
        for (element, tags) in &other.adds {
            adds.entry(element.clone()).or_default().extend(tags.iter().cloned());
        }
        let mut removes = self.removes.clone();
        removes.extend(other.removes.iter().cloned());
        Self { adds, removes }
    }
}

// ---------------------------------------------------------------------------
// RGA — the sequence
// ---------------------------------------------------------------------------

/// A sequence element's identity. Ordered `(seq, node)`, which gives the
/// deterministic total order sibling insertions are broken by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ElementId {
    pub seq: u64,
    pub node: String,
}

impl ElementId {
    pub fn new(node: &str, seq: u64) -> Self {
        Self {
            seq,
            node: node.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RgaElement {
    value: String,
    /// The element this one was inserted after — `None` for the head.
    after: Option<ElementId>,
}

/// A replicated growable array (Roh et al., 2011) — the sequence member of the
/// family.
///
/// Every element is inserted **after** an element the inserting replica had
/// already seen, and removal is a tombstone. Both halves are grow-only sets,
/// so the join is set union and the three laws are structural rather than
/// argued.
///
/// The read is a **pure function of the merged set**: depth-first from the
/// head, siblings sharing an anchor ordered by **descending id**. That is what
/// makes convergence follow immediately — two replicas holding the same set
/// read the same sequence, with no further agreement needed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rga {
    elements: BTreeMap<ElementId, RgaElement>,
    tombstones: BTreeSet<ElementId>,
}

impl Rga {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `value` with identity `id`, positioned after `after`.
    ///
    /// Refuses a duplicate id (identity is the whole basis of the order) and
    /// refuses an `after` this replica has never seen — **you can only insert
    /// after something you have observed**, which is what keeps the structure
    /// a forest and makes the read terminate.
    pub fn insert_after(
        &self,
        id: ElementId,
        value: &str,
        after: Option<&ElementId>,
    ) -> Result<Self, CrdtError> {
        if self.elements.contains_key(&id) {
            return Err(CrdtError::DuplicateElementId {
                node: id.node,
                seq: id.seq,
            });
        }
        if let Some(a) = after {
            if !self.elements.contains_key(a) {
                return Err(CrdtError::UnknownAnchor {
                    node: a.node.clone(),
                    seq: a.seq,
                });
            }
        }
        let mut out = self.clone();
        out.elements.insert(
            id,
            RgaElement {
                value: value.to_string(),
                after: after.cloned(),
            },
        );
        Ok(out)
    }

    /// Tombstone. The element stays as an **anchor** — an element that
    /// vanished would orphan any concurrent insert positioned after it.
    pub fn remove(&self, id: &ElementId) -> Self {
        let mut out = self.clone();
        out.tombstones.insert(id.clone());
        out
    }

    /// The visible sequence — a pure function of the merged set.
    pub fn read(&self) -> Vec<&str> {
        self.walk().0
    }

    /// ★ Elements whose anchor has not arrived yet.
    ///
    /// Neither dropped nor appended somewhere plausible: guessing a position
    /// would give two replicas holding the same set two different reads, which
    /// is the one thing convergence forbids. Reported instead, and reported
    /// identically by every replica holding that set.
    pub fn pending(&self) -> Vec<&ElementId> {
        self.walk().1
    }

    fn walk(&self) -> (Vec<&str>, Vec<&ElementId>) {
        // Siblings grouped by anchor, each group ordered descending — RGA's
        // rule for concurrent inserts at the same position.
        let mut children: BTreeMap<Option<&ElementId>, Vec<&ElementId>> = BTreeMap::new();
        for (id, el) in &self.elements {
            children.entry(el.after.as_ref()).or_default().push(id);
        }
        for group in children.values_mut() {
            group.sort_by(|a, b| b.cmp(a));
        }

        let mut out = Vec::new();
        let mut seen: BTreeSet<&ElementId> = BTreeSet::new();
        let mut stack: Vec<&ElementId> = children
            .get(&None)
            .map(|g| g.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(id) = stack.pop() {
            // Defensive: a malformed set must not hang a read.
            if !seen.insert(id) {
                continue;
            }
            if !self.tombstones.contains(id) {
                out.push(self.elements[id].value.as_str());
            }
            if let Some(group) = children.get(&Some(id)) {
                for child in group.iter().rev() {
                    stack.push(child);
                }
            }
        }

        let pending: Vec<&ElementId> = self
            .elements
            .keys()
            .filter(|id| !seen.contains(id))
            .collect();
        (out, pending)
    }

    /// The sub-state carrying only these elements — **what a partial delivery
    /// actually is**.
    ///
    /// A delta may reference an anchor it does not contain, which is precisely
    /// why [`Rga::pending`] exists: the network delivers pieces, and a replica
    /// has to hold a piece it cannot yet place without either losing it or
    /// guessing where it goes.
    pub fn delta(&self, ids: &[ElementId]) -> Self {
        let wanted: BTreeSet<&ElementId> = ids.iter().collect();
        Self {
            elements: self
                .elements
                .iter()
                .filter(|(id, _)| wanted.contains(id))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            tombstones: self
                .tombstones
                .iter()
                .filter(|id| wanted.contains(id))
                .cloned()
                .collect(),
        }
    }

    pub fn tombstones(&self) -> usize {
        self.tombstones.len()
    }

    pub fn len(&self) -> usize {
        self.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl JoinSemilattice for Rga {
    /// Union of elements, union of tombstones. Both grow-only, so the laws are
    /// set union's.
    ///
    /// A same-id collision keeps the left. That branch is **unreachable** with
    /// the `(node, seq)` id scheme as long as each replica mints under its own
    /// node id — it is defined rather than left to panic, not because it is
    /// expected.
    fn join(&self, other: &Self) -> Self {
        let mut elements = self.elements.clone();
        for (id, el) in &other.elements {
            elements.entry(id.clone()).or_insert_with(|| el.clone());
        }
        let mut tombstones = self.tombstones.clone();
        tombstones.extend(other.tombstones.iter().cloned());
        Self {
            elements,
            tombstones,
        }
    }
}

// ---------------------------------------------------------------------------
// The vclock composition point
// ---------------------------------------------------------------------------

/// **Merge two stamped CRDT values — and report *why* they differed.**
///
/// The LUB is taken regardless of the causal verdict, which is the point: for
/// a CRDT the two cases the previous slice had to tell apart collapse into
/// one. A causally-ordered pair satisfies `a ⊑ b`, so `a ⊔ b = b` — exactly
/// what LWW would have picked. A concurrent pair merges into a value
/// containing both, where LWW would have discarded one and `Contested` would
/// have handed back two.
///
/// So the clock is no longer load-bearing for the *answer* here. It is still
/// returned, because why two replicas differed is worth knowing even when what
/// to do about it no longer depends on it.
pub fn merge_stamped<T: JoinSemilattice>(
    left: &Stamped<T>,
    right: &Stamped<T>,
) -> (Stamped<T>, CausalVerdict) {
    let verdict = left.clock.compare(&right.clock);
    // The physical stamp still picks the provenance the merged value is
    // attributed to; it no longer picks the value.
    let newer = if right.order_key() >= left.order_key() {
        right
    } else {
        left
    };
    let merged = Stamped::new(
        left.value.join(&right.value),
        left.clock.merge(&right.clock),
        newer.t_event,
        &newer.id,
    );
    (merged, verdict)
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CrdtError {
    #[error("counter for '{0}' would overflow — refused rather than wrapped, because a counter that silently stops counting converges on a wrong number")]
    CounterOverflow(String),
    #[error("tag ({node}, {seq}) is already in use — two adds sharing a tag would let one remove kill both")]
    TagReused { node: String, seq: u64 },
    #[error("element id ({node}, {seq}) is already in use — identity is the whole basis of the sequence order")]
    DuplicateElementId { node: String, seq: u64 },
    #[error("cannot insert after ({node}, {seq}) — this replica has never seen it, and you can only position against what you have observed")]
    UnknownAnchor { node: String, seq: u64 },
    #[error("{0} update(s) is not enough to test convergence — the laws are about a pair and a triple")]
    NotEnoughUpdates(usize),
    #[error("{count} updates exceeds the {limit}-permutation limit — refused rather than sampled, because a convergence proof over a sample is not a proof")]
    TooManyToPermute { count: usize, limit: usize },
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vclock::VectorClock;

    fn g(entries: &[(&str, u64)]) -> GCounter {
        GCounter::from_entries(entries)
    }

    // -- the laws, on every member of the family ----------------------------

    #[test]
    fn the_three_laws_hold_for_the_g_counter() {
        let r = laws_hold(&g(&[("a", 3)]), &g(&[("b", 5)]), &g(&[("a", 9), ("c", 1)]));
        assert!(r.all_hold(), "{}", r.describe());
    }

    #[test]
    fn the_three_laws_hold_for_the_pn_counter() {
        let a = PnCounter::new().increment("a", 5).unwrap();
        let b = PnCounter::new().decrement("b", 2).unwrap();
        let c = PnCounter::new().increment("c", 7).unwrap().decrement("c", 3).unwrap();
        let r = laws_hold(&a, &b, &c);
        assert!(r.all_hold(), "{}", r.describe());
    }

    #[test]
    fn the_three_laws_hold_for_the_or_set() {
        let a = OrSet::new().add("rice", Tag::new("a", 1)).unwrap();
        let b = OrSet::new().add("beans", Tag::new("b", 1)).unwrap();
        let c = a.remove("rice");
        let r = laws_hold(&a, &b, &c);
        assert!(r.all_hold(), "{}", r.describe());
    }

    #[test]
    fn the_three_laws_hold_for_the_sequence() {
        let base = Rga::new()
            .insert_after(ElementId::new("a", 1), "milk", None)
            .unwrap();
        let b = base
            .insert_after(ElementId::new("b", 2), "eggs", Some(&ElementId::new("a", 1)))
            .unwrap();
        let c = base.remove(&ElementId::new("a", 1));
        let r = laws_hold(&base, &b, &c);
        assert!(r.all_hold(), "{}", r.describe());
    }

    // -- ★★ order, duplication, retry ---------------------------------------

    #[test]
    fn a_g_counter_converges_over_every_ordering_and_every_duplication() {
        let updates = vec![
            g(&[("ama", 3)]),
            g(&[("ben", 5)]),
            g(&[("ama", 7)]),
            g(&[("cira", 2)]),
        ];
        let r = converges(&updates).expect("small enough");
        assert_eq!(r.orderings, 24);
        assert!(r.strongly_eventually_consistent());
    }

    #[test]
    fn an_or_set_converges_over_every_ordering_and_every_duplication() {
        let a = OrSet::new().add("rice", Tag::new("a", 1)).unwrap();
        let b = a.remove("rice");
        let c = OrSet::new().add("rice", Tag::new("c", 1)).unwrap();
        let r = converges(&[a, b, c]).expect("small enough");
        assert_eq!(r.orderings, 6);
        assert!(r.strongly_eventually_consistent());
    }

    #[test]
    fn a_sequence_converges_over_every_ordering_and_every_duplication() {
        let head = Rga::new()
            .insert_after(ElementId::new("a", 1), "milk", None)
            .unwrap();
        let anchor = ElementId::new("a", 1);
        let x = head.insert_after(ElementId::new("b", 2), "eggs", Some(&anchor)).unwrap();
        let y = head.insert_after(ElementId::new("c", 2), "bread", Some(&anchor)).unwrap();
        let r = converges(&[head, x, y]).expect("small enough");
        assert!(r.strongly_eventually_consistent());
    }

    #[test]
    fn convergence_refuses_to_sample_rather_than_prove() {
        let many: Vec<GCounter> = (0..8).map(|i| g(&[("n", i)])).collect();
        assert!(matches!(
            converges(&many),
            Err(CrdtError::TooManyToPermute { .. })
        ));
    }

    // -- ★★ never erase the node --------------------------------------------

    #[test]
    fn a_merge_can_never_lower_anybody_s_entry() {
        let mine = g(&[("ama", 40), ("ben", 30)]);
        let theirs = g(&[("ama", 10), ("cira", 90)]);
        for other in [&theirs, &g(&[]), &g(&[("ama", 0)])] {
            let merged = mine.join(other);
            for node in mine.nodes() {
                assert!(
                    merged.of(node) >= mine.of(node),
                    "{node} was lowered by a merge"
                );
            }
        }
        // The specific claim: my own number is untouched by a smaller one.
        assert_eq!(mine.join(&theirs).of("ama"), 40);
    }

    #[test]
    fn the_group_total_is_the_merge_and_the_entries_stay_the_members_own() {
        // The roll-up shape: included: [{member, value}].
        let group = g(&[("ama", 40), ("ben", 30), ("cira", 20), ("dee", 10)]);
        assert_eq!(group.value(), 100);
        let stale = g(&[("ama", 5)]);
        let merged = group.join(&stale);
        assert_eq!(merged.value(), 100);
        assert_eq!(merged.of("ama"), 40);
    }

    // -- the counters -------------------------------------------------------

    #[test]
    fn a_pn_counter_carries_a_decrease_a_g_counter_would_discard() {
        let up_only = g(&[("ama", 10)]).join(&g(&[("ama", 4)]));
        assert_eq!(up_only.of("ama"), 10, "max discards the smaller reading");

        let pn = PnCounter::new()
            .increment("ama", 10)
            .unwrap()
            .decrement("ama", 4)
            .unwrap();
        assert_eq!(pn.value(), 6);
        assert_eq!(pn.of("ama"), 6);
    }

    #[test]
    fn counter_overflow_is_refused_not_wrapped() {
        let c = g(&[("a", u64::MAX)]);
        assert!(matches!(
            c.increment("a", 1),
            Err(CrdtError::CounterOverflow(_))
        ));
    }

    // -- the OR-set ---------------------------------------------------------

    #[test]
    fn a_concurrent_add_survives_a_remove_that_never_observed_it() {
        let shared = OrSet::new().add("rice", Tag::new("ama", 1)).unwrap();
        // ama removes what it can see.
        let ama = shared.remove("rice");
        // ben concurrently re-adds under a tag ama never saw.
        let ben = shared.add("rice", Tag::new("ben", 1)).unwrap();
        let merged = ama.join(&ben);
        assert!(merged.contains("rice"), "add wins over a remove that never saw it");
        assert_eq!(merged.live_tags("rice").len(), 1);
        assert_eq!(merged.live_tags("rice")[0].node, "ben");
    }

    #[test]
    fn a_remove_that_observed_every_tag_removes_the_element() {
        let s = OrSet::new()
            .add("rice", Tag::new("a", 1))
            .unwrap()
            .add("rice", Tag::new("b", 1))
            .unwrap();
        let gone = s.remove("rice");
        assert!(!gone.contains("rice"));
        // Duplicate delivery of the remove changes nothing.
        assert_eq!(gone.join(&gone), gone);
    }

    #[test]
    fn a_reused_tag_is_refused() {
        let s = OrSet::new().add("rice", Tag::new("a", 1)).unwrap();
        assert!(matches!(
            s.add("beans", Tag::new("a", 1)),
            Err(CrdtError::TagReused { .. })
        ));
    }

    // -- the sequence -------------------------------------------------------

    #[test]
    fn concurrent_inserts_at_the_same_anchor_order_deterministically() {
        let head = Rga::new()
            .insert_after(ElementId::new("a", 1), "milk", None)
            .unwrap();
        let anchor = ElementId::new("a", 1);
        let left = head.insert_after(ElementId::new("b", 2), "eggs", Some(&anchor)).unwrap();
        let right = head.insert_after(ElementId::new("c", 2), "bread", Some(&anchor)).unwrap();
        // Descending id: (2, "c") before (2, "b").
        assert_eq!(left.join(&right).read(), vec!["milk", "bread", "eggs"]);
        assert_eq!(right.join(&left).read(), left.join(&right).read());
    }

    #[test]
    fn a_removed_element_stays_as_an_anchor() {
        let anchor = ElementId::new("a", 1);
        let head = Rga::new().insert_after(anchor.clone(), "milk", None).unwrap();
        let with_child = head
            .insert_after(ElementId::new("b", 2), "eggs", Some(&anchor))
            .unwrap();
        let removed = with_child.remove(&anchor);
        assert_eq!(removed.read(), vec!["eggs"], "the child is not orphaned");
        assert_eq!(removed.tombstones(), 1);
    }

    #[test]
    fn an_element_whose_anchor_has_not_arrived_is_reported_not_guessed() {
        let anchor = ElementId::new("a", 1);
        let head = Rga::new().insert_after(anchor.clone(), "milk", None).unwrap();
        let child = head
            .insert_after(ElementId::new("b", 2), "eggs", Some(&anchor))
            .unwrap();
        // A partial delivery: the child element without the head it anchors to.
        let child_id = ElementId::new("b", 2);
        let orphan_only = Rga::new().join(&child.delta(std::slice::from_ref(&child_id)));
        assert_eq!(orphan_only.read(), Vec::<&str>::new());
        assert_eq!(orphan_only.pending(), vec![&child_id]);
        // And once the anchor arrives nothing was lost.
        let healed = orphan_only.join(&child);
        assert_eq!(healed.read(), vec!["milk", "eggs"]);
        assert!(healed.pending().is_empty());
    }

    #[test]
    fn inserting_after_something_never_seen_is_refused() {
        let r = Rga::new();
        assert!(matches!(
            r.insert_after(ElementId::new("a", 1), "milk", Some(&ElementId::new("z", 9))),
            Err(CrdtError::UnknownAnchor { .. })
        ));
    }

    // -- the vclock composition point ---------------------------------------

    #[test]
    fn a_concurrent_pair_merges_rather_than_one_being_picked() {
        let base = VectorClock::new();
        let a = Stamped::new(g(&[("ama", 5)]), base.tick("ama"), 100, "e-a");
        let b = Stamped::new(g(&[("ben", 7)]), base.tick("ben"), 200, "e-b");
        let (merged, verdict) = merge_stamped(&a, &b);
        assert_eq!(verdict, CausalVerdict::Concurrent);
        // LWW would have kept only ben's 7. Nothing is lost.
        assert_eq!(merged.value.of("ama"), 5);
        assert_eq!(merged.value.of("ben"), 7);
        assert_eq!(merged.value.value(), 12);
    }

    /// ★ For a CRDT the causally-ordered case and the concurrent case are the
    /// same operation — which is why a CRDT needs no clock to be correct.
    #[test]
    fn a_causally_ordered_pair_merges_to_the_successor_which_is_what_lww_would_pick() {
        let c1 = VectorClock::new().tick("ama");
        let c2 = c1.tick("ama");
        let earlier = Stamped::new(g(&[("ama", 5)]), c1, 100, "e-1");
        let later = Stamped::new(g(&[("ama", 9)]), c2, 200, "e-2");
        let (merged, verdict) = merge_stamped(&earlier, &later);
        assert_eq!(verdict, CausalVerdict::HappensBefore);
        assert_eq!(merged.value, later.value, "a ⊔ successor(a) = successor(a)");
        // And order does not matter here either.
        assert_eq!(merge_stamped(&later, &earlier).0.value, later.value);
    }

    #[test]
    fn the_lattice_order_is_derived_from_the_join() {
        let small = g(&[("a", 1)]);
        let big = g(&[("a", 2), ("b", 1)]);
        assert!(small.leq(&big));
        assert!(!big.leq(&small));
        assert!(small.leq(&small));
    }
}
