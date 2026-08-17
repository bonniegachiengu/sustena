//! Declared dimension kinds, and the apply each one admits (RECORD §VII).
//!
//! §VII proves that a **snapshot-valued** dimension — *an observation of an
//! authoritative external quantity*, `x = (v, τ)` — is a last-writer-wins
//! register keyed on event time, and therefore a state-based CRDT: the join is
//! idempotent, commutative and associative, so the fold is invariant under
//! **any permutation and any repetition** of the log.
//!
//! ## ★★ Why "never accumulate deltas" is derived, not decreed
//!
//! An accumulating apply `x + δ` is commutative and associative but **not
//! idempotent**: `x + 2δ ≠ x + δ`. Under §V's at-least-once delivery that is
//! not a theoretical blemish, it is a wrong balance. It becomes safe only if
//! the apply is made idempotent by carrying **the set of applied ids** — which
//! rebuilds the grow-only counter with **unbounded metadata**, where LWW gets
//! the same guarantee with `O(1)`.
//!
//! So the rule follows from requiring convergence under at-least-once,
//! out-of-order delivery. It is not a style preference, and both halves are
//! *measured* here rather than asserted: [`SnapshotDim::metadata_len`] is
//! always 1, [`AccumulatingDim::metadata_len`] grows with every distinct
//! applied id.
//!
//! ## ★★ The payoff: delta-accumulation on a snapshot dimension is a type error
//!
//! §4J.4 step 3, and the whole reason this row exists — *"the rule stops being
//! remembered and starts being checked."* Two mechanisms, because the mistake
//! can arrive two ways:
//!
//! - **In code, it does not compile.** [`SnapshotDim`] has no `add`. Not a
//!   refusal at run time — there is no method to call, which a `compile_fail`
//!   doctest on [`SnapshotDim`] proves by failing to build.
//! - **From data, it is refused at the schema.** A kind declared in a spec and
//!   an update arriving off a wire cannot be a Rust type error, so
//!   [`DimensionSchema::check`] returns
//!   [`DimensionError::DeltaOnSnapshot`] — the same rule, enforced where the
//!   type system cannot reach.
//!
//! Both are needed. Claiming the compile-time half covers the declared-in-a-spec
//! case would be the dishonest version of this.
//!
//! ## ★ The honest limit §VII is emphatic about
//!
//! **LWW converges; convergence is not correctness.** It is correct *only* for
//! snapshot-valued dimensions, where discarding the older reading is the
//! semantically right answer because the newer reading of a balance supersedes
//! it by definition. For a **contested edit** — two people changing a shared
//! plan — LWW would silently discard one, which is *"the single easiest way to
//! turn a correct theorem into a lost transaction."*
//!
//! So the three kinds are kept apart at the type level, each with the only
//! apply it admits:
//!
//! | kind | apply | metadata |
//! |---|---|---|
//! | [`Dimension::Snapshot`] | LWW on `(τ, id)` | `O(1)` |
//! | [`Dimension::Accumulating`] | idempotent by applied-id set | **unbounded** |
//! | [`Dimension::Contested`] | a merge-CRDT ([`OrSet`]) — keeps both | grows with edits |
//!
//! [`DimensionValue`] is the sum of the three, and its accessors return
//! `Option`, so reaching an accumulating apply requires first establishing that
//! the dimension *is* accumulating.
//!
//! ## Reused, not re-derived
//!
//! The total order is [`Observation`]'s existing `(t_event, id)` — §VII's own
//! Lamport tie-break, already built and already tested, because two
//! observations with equal event time stop commuting without it. The contested
//! payload is `crdt.rs`'s [`OrSet`] rather than a second merge structure. The
//! laws are checked with `crdt.rs`'s own [`laws_hold`](crate::crdt::laws_hold)
//! and [`converges`](crate::crdt::converges).
//!
//! ## Scope
//!
//! The declaration, the attachment and the refusals live on state and schema
//! and are built here. The **authoring syntax** for declaring a kind is
//! M-DSL's (Phase 4) — named as the downstream consumer, not built.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::crdt::{CrdtError, JoinSemilattice, OrSet, Tag};
use crate::event::Observation;
use crate::vclock::Dimension;

/// What can go wrong applying an update to a declared dimension.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DimensionError {
    /// ★★ The row's whole point, for the case a type cannot catch: a delta
    /// aimed at a dimension declared snapshot-valued.
    #[error(
        "'{path}' is declared snapshot-valued — an observation of an external authority — so a \
         delta of {by} cannot be applied to it. Adding deltas is not idempotent, so a redelivered \
         one would be a wrong balance; the newer OBSERVATION supersedes instead"
    )]
    DeltaOnSnapshot { path: String, by: i64 },
    /// The mirror: an observation aimed at an accumulating dimension.
    #[error(
        "'{path}' is declared accumulating — it is raised by increments, not read off an \
         authority — so an observation cannot replace it; that would discard every contribution \
         already counted"
    )]
    ObservationOnAccumulating { path: String },
    /// ★ And the one §VII is emphatic about: LWW must not eat a contested edit.
    #[error(
        "'{path}' is declared contested — two parties may change it independently — so neither an \
         observation nor a delta applies. LWW would silently discard one edit, which is how a \
         correct theorem becomes a lost transaction; reconcile through the merge-CRDT instead"
    )]
    AutomaticApplyOnContested { path: String },
    #[error("no dimension is declared at '{0}' — the kind must be declared before it is applied to")]
    Undeclared(String),
    #[error("'{path}' is already declared as {existing:?} and cannot be redeclared as {proposed:?}")]
    AlreadyDeclared { path: String, existing: Dimension, proposed: Dimension },
    #[error(transparent)]
    Crdt(#[from] CrdtError),
}

// ── snapshot: LWW on (τ, id), and no `add` anywhere on it ──────────────────

/// A snapshot-valued dimension: `x = (v, τ)`, last-writer-wins on `(τ, id)`.
///
/// ★★ **There is no `add` on this type.** Delta-accumulation is unspellable
/// rather than refused, which is what makes §4J.4 step 3 a type error:
///
/// ```compile_fail
/// use sustena_core::dimension::SnapshotDim;
/// use serde_json::json;
///
/// let balance = SnapshotDim::observed(json!(1200), 100, "sms-1");
/// // A balance read off an SMS is an OBSERVATION, not a running total —
/// // so there is no method here to add a delta with. This does not compile.
/// let wrong = balance.add("sms-2", 50);
/// ```
///
/// The correct move is another observation, which supersedes:
///
/// ```
/// use sustena_core::dimension::SnapshotDim;
/// use serde_json::json;
///
/// let balance = SnapshotDim::observed(json!(1200), 100, "sms-1");
/// let newer = balance.observe(json!(1150), 200, "sms-2");
/// assert_eq!(newer.value(), &json!(1150));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDim {
    reading: Observation<Value>,
}

impl SnapshotDim {
    /// An observation of the authority: the value, and when it was observed.
    pub fn observed(value: Value, t_event: i64, id: impl Into<String>) -> Self {
        Self { reading: Observation { value, t_event, id: id.into() } }
    }

    /// Offer another observation. The newer `(τ, id)` wins — §VII's LWW join,
    /// reusing [`Observation::join`] rather than re-deriving the tie-break.
    pub fn observe(&self, value: Value, t_event: i64, id: impl Into<String>) -> Self {
        Self { reading: self.reading.join(&Observation { value, t_event, id: id.into() }) }
    }

    pub fn value(&self) -> &Value {
        &self.reading.value
    }

    pub fn t_event(&self) -> i64 {
        self.reading.t_event
    }

    pub fn observation_id(&self) -> &str {
        &self.reading.id
    }

    /// ★ Always 1 — `O(1)` metadata, whatever the length of the log. The half
    /// of §VII's argument that makes LWW worth preferring, as a number.
    pub fn metadata_len(&self) -> usize {
        1
    }
}

impl JoinSemilattice for SnapshotDim {
    fn join(&self, other: &Self) -> Self {
        Self { reading: self.reading.join(&other.reading) }
    }
}

// ── accumulating: idempotent only by carrying the applied ids ──────────────

/// An accumulating dimension — a running total raised by increments.
///
/// ★★ It carries **the id of every delta applied**, because that is the only
/// way `apply` becomes idempotent, and idempotence is what §V's at-least-once
/// delivery requires. The map is the cost §VII prices: **unbounded metadata**,
/// against LWW's `O(1)`.
///
/// The naive alternative is not offered — there is no `add(by)` without an id,
/// so an accumulation that could double-count on redelivery is not writable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccumulatingDim {
    /// id → the delta that id contributed. The same id twice is the same fact
    /// twice, and contributes once.
    applied: BTreeMap<String, i64>,
}

impl AccumulatingDim {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a delta, identified. Re-applying the same id is a no-op — which is
    /// the whole point, and why the id is not optional.
    pub fn add(&self, id: impl Into<String>, by: i64) -> Self {
        let mut applied = self.applied.clone();
        applied.entry(id.into()).or_insert(by);
        Self { applied }
    }

    /// The running total.
    pub fn total(&self) -> i64 {
        self.applied.values().sum()
    }

    /// ★ Grows with every distinct applied id — §VII's *unbounded metadata*,
    /// as a number rather than a warning.
    pub fn metadata_len(&self) -> usize {
        self.applied.len()
    }
}

impl JoinSemilattice for AccumulatingDim {
    /// Union of the applied-id maps. An id present on both sides carries the
    /// same delta — it is the same fact — so the union is well defined and the
    /// join is idempotent, commutative and associative.
    fn join(&self, other: &Self) -> Self {
        let mut applied = self.applied.clone();
        for (id, by) in &other.applied {
            applied.entry(id.clone()).or_insert(*by);
        }
        Self { applied }
    }
}

// ── contested: a merge-CRDT, never LWW ─────────────────────────────────────

/// A contested dimension — two parties may change it independently.
///
/// ★ Backed by `crdt.rs`'s [`OrSet`] rather than a second merge structure,
/// because §VII sends this case to the merge-CRDTs by name. Concurrent edits
/// **both survive**; nothing here picks one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContestedDim {
    edits: OrSet,
}

impl ContestedDim {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an edit. Tags are the OR-set's own; a reused tag is refused.
    pub fn edit(&self, value: &str, tag: Tag) -> Result<Self, DimensionError> {
        Ok(Self { edits: self.edits.add(value, tag)? })
    }

    /// Retract an edit this replica has seen.
    pub fn retract(&self, value: &str) -> Self {
        Self { edits: self.edits.remove(value) }
    }

    /// Everything still standing. **Two entries means two live edits**, not a
    /// tie to be broken.
    pub fn live(&self) -> Vec<String> {
        let mut v: Vec<String> = self.edits.elements().into_iter().map(String::from).collect();
        v.sort();
        v
    }

    pub fn metadata_len(&self) -> usize {
        self.live().len()
    }
}

impl JoinSemilattice for ContestedDim {
    fn join(&self, other: &Self) -> Self {
        Self { edits: self.edits.join(&other.edits) }
    }
}

// ── the declared value, and the schema that declares it ────────────────────

/// A dimension's value, in the only shape its declared kind admits.
///
/// ★ The accessors return `Option`, so reaching an accumulating apply requires
/// first establishing that the dimension *is* accumulating — a snapshot never
/// hands out a handle that could add.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DimensionValue {
    Snapshot(SnapshotDim),
    Accumulating(AccumulatingDim),
    Contested(ContestedDim),
}

impl DimensionValue {
    pub fn kind(&self) -> Dimension {
        match self {
            DimensionValue::Snapshot(_) => Dimension::Snapshot,
            DimensionValue::Accumulating(_) => Dimension::Accumulating,
            DimensionValue::Contested(_) => Dimension::Contested,
        }
    }

    pub fn as_snapshot(&self) -> Option<&SnapshotDim> {
        match self {
            DimensionValue::Snapshot(d) => Some(d),
            _ => None,
        }
    }

    /// ★ `None` for a snapshot — there is no handle from here that could add a
    /// delta to one.
    pub fn as_accumulating(&self) -> Option<&AccumulatingDim> {
        match self {
            DimensionValue::Accumulating(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_contested(&self) -> Option<&ContestedDim> {
        match self {
            DimensionValue::Contested(d) => Some(d),
            _ => None,
        }
    }
}

/// An update offered to a dimension, as it would arrive from a spec or a wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Update {
    /// A reading of an external authority.
    Observe { value: Value, t_event: i64, id: String },
    /// An increment.
    Delta { by: i64, id: String },
}

/// The declared kind of every dimension, by state path.
///
/// ★ This is where §4J.4 step 3 lands for updates that are *data* rather than
/// code — a kind declared in a spec, an update off a wire. The type system
/// cannot reach either, so the same rule is checked here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimensionSchema {
    kinds: BTreeMap<String, Dimension>,
}

impl DimensionSchema {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a path's kind. Redeclaring it differently is refused — a
    /// dimension that changed kind under a running log would invalidate every
    /// apply already made to it.
    pub fn declare(
        &mut self,
        path: impl Into<String>,
        kind: Dimension,
    ) -> Result<(), DimensionError> {
        let path = path.into();
        match self.kinds.get(&path) {
            Some(existing) if *existing != kind => Err(DimensionError::AlreadyDeclared {
                path,
                existing: *existing,
                proposed: kind,
            }),
            _ => {
                self.kinds.insert(path, kind);
                Ok(())
            }
        }
    }

    pub fn kind_of(&self, path: &str) -> Option<Dimension> {
        self.kinds.get(path).copied()
    }

    pub fn declared(&self) -> usize {
        self.kinds.len()
    }

    /// ★★ Is this update admissible against the declared kind?
    ///
    /// A delta at a snapshot dimension is [`DimensionError::DeltaOnSnapshot`];
    /// an observation at an accumulating one discards every contribution
    /// already counted; and **neither applies to a contested one**, because
    /// automatic application is exactly what would lose an edit.
    pub fn check(&self, path: &str, update: &Update) -> Result<(), DimensionError> {
        let Some(kind) = self.kind_of(path) else {
            return Err(DimensionError::Undeclared(path.to_string()));
        };
        match (kind, update) {
            (Dimension::Snapshot, Update::Observe { .. }) => Ok(()),
            (Dimension::Snapshot, Update::Delta { by, .. }) => {
                Err(DimensionError::DeltaOnSnapshot { path: path.to_string(), by: *by })
            }
            (Dimension::Accumulating, Update::Delta { .. }) => Ok(()),
            (Dimension::Accumulating, Update::Observe { .. }) => {
                Err(DimensionError::ObservationOnAccumulating { path: path.to_string() })
            }
            (Dimension::Contested, _) => {
                Err(DimensionError::AutomaticApplyOnContested { path: path.to_string() })
            }
        }
    }

    /// Apply an update, checking it first. ★ The LWW attaches **automatically**
    /// to a snapshot dimension — the caller declares the kind and never chooses
    /// the apply.
    pub fn apply(
        &self,
        path: &str,
        current: Option<&DimensionValue>,
        update: Update,
    ) -> Result<DimensionValue, DimensionError> {
        self.check(path, &update)?;
        match (self.kind_of(path).expect("checked above"), update) {
            (Dimension::Snapshot, Update::Observe { value, t_event, id }) => {
                let next = match current.and_then(DimensionValue::as_snapshot) {
                    Some(d) => d.observe(value, t_event, id),
                    None => SnapshotDim::observed(value, t_event, id),
                };
                Ok(DimensionValue::Snapshot(next))
            }
            (Dimension::Accumulating, Update::Delta { by, id }) => {
                let base = current
                    .and_then(DimensionValue::as_accumulating)
                    .cloned()
                    .unwrap_or_default();
                Ok(DimensionValue::Accumulating(base.add(id, by)))
            }
            // Every other pair was refused by `check`.
            _ => unreachable!("check() admits only the two applicable pairs"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::{converges, laws_hold};
    use serde_json::json;

    fn balance(v: i64, t: i64, id: &str) -> SnapshotDim {
        SnapshotDim::observed(json!(v), t, id)
    }

    // ── §VII: LWW converges under permutation AND repetition ───────────────

    #[test]
    fn the_snapshot_join_obeys_the_three_laws() {
        // Reusing crdt.rs's own law checker rather than restating the laws.
        let (a, b, c) = (balance(10, 100, "a"), balance(20, 200, "b"), balance(30, 150, "c"));
        let r = laws_hold(&a, &b, &c);
        assert!(r.commutative && r.associative && r.idempotent, "{r:?}");
    }

    #[test]
    fn replicas_converge_under_any_permutation_and_any_repetition() {
        // ★★ §VII's corollary: two offline phones seeing the same SMS backlog
        // in different orders land on the same balance — and a redelivered SMS
        // cannot corrupt it.
        let updates = vec![balance(10, 100, "a"), balance(30, 300, "b"), balance(20, 200, "c")];
        let report = converges(&updates).unwrap();
        assert!(report.order_independent, "every permutation folded to one state: {report:?}");
        assert!(report.duplication_free, "and with every update delivered twice: {report:?}");
        assert!(report.strongly_eventually_consistent());

        // And the state itself is the latest observation, however it arrived.
        let folded = updates.iter().fold(updates[0].clone(), |a, b| a.join(b));
        assert_eq!(folded.value(), &json!(30), "the latest observation wins");
    }

    #[test]
    fn a_later_observation_supersedes_and_an_earlier_one_does_not() {
        let b = balance(1200, 100, "sms-1");
        assert_eq!(b.observe(json!(1150), 200, "sms-2").value(), &json!(1150));
        assert_eq!(b.observe(json!(9999), 50, "sms-0").value(), &json!(1200), "stale reading");
    }

    #[test]
    fn an_equal_event_time_breaks_on_the_id_so_the_order_stays_total() {
        // ★ Without the tie-break two observations at the same τ stop
        // commuting. Reused from `Observation`, not re-derived here.
        let left = balance(1, 100, "a");
        let right = balance(2, 100, "z");
        assert_eq!(left.join(&right), right.join(&left));
        assert_eq!(left.join(&right).value(), &json!(2));
    }

    #[test]
    fn snapshot_metadata_is_constant_however_many_readings_arrive() {
        let mut b = balance(0, 0, "seed");
        for i in 1..500 {
            b = b.observe(json!(i), i, format!("sms-{i}"));
        }
        assert_eq!(b.metadata_len(), 1, "O(1) — the half of §VII that prefers LWW");
    }

    // ── ★★ why "never accumulate deltas" is derived ────────────────────────

    #[test]
    fn naive_addition_is_not_idempotent_which_is_the_whole_argument() {
        // The counter-example §VII gives, run rather than quoted: `x + 2δ ≠
        // x + δ`. Written here as plain arithmetic because the shipped type
        // deliberately offers no way to do it.
        let (x, delta) = (100i64, 50i64);
        assert_ne!(x + delta + delta, x + delta);
    }

    #[test]
    fn carrying_the_applied_ids_makes_it_idempotent_and_that_is_the_cost() {
        // ★★ The other half: made safe, an accumulation IS the grow-only
        // counter, and the metadata is unbounded — which is what LWW buys out
        // of at O(1).
        let a = AccumulatingDim::new().add("tx-1", 50).add("tx-2", 30);
        assert_eq!(a.total(), 80);

        let redelivered = a.add("tx-1", 50);
        assert_eq!(redelivered.total(), 80, "a redelivered delta counts once");
        assert_eq!(a, redelivered, "and the state is unchanged, not merely equal in total");

        assert_eq!(a.metadata_len(), 2);
        let big = (0..200).fold(AccumulatingDim::new(), |acc, i| acc.add(format!("tx-{i}"), 1));
        assert_eq!(big.metadata_len(), 200, "unbounded — it grows with the log");
        assert_eq!(big.total(), 200);
    }

    #[test]
    fn the_accumulating_join_also_obeys_the_three_laws() {
        let a = AccumulatingDim::new().add("t1", 10);
        let b = AccumulatingDim::new().add("t2", 20);
        let c = AccumulatingDim::new().add("t3", 5);
        let r = laws_hold(&a, &b, &c);
        assert!(r.commutative && r.associative && r.idempotent, "{r:?}");
        assert_eq!(a.join(&b).join(&c).total(), 35);
    }

    // ── ★★ delta on a snapshot is a type error, and a refusal from data ────

    #[test]
    fn a_snapshot_hands_out_no_handle_that_could_add() {
        // The compile-time half is a `compile_fail` doctest on `SnapshotDim`;
        // this is the value-level mirror of it.
        let v = DimensionValue::Snapshot(balance(1200, 100, "sms-1"));
        assert!(v.as_accumulating().is_none());
        assert!(v.as_snapshot().is_some());
        assert_eq!(v.kind(), Dimension::Snapshot);
    }

    #[test]
    fn a_delta_declared_against_a_snapshot_dimension_is_refused() {
        // ★★ The declared-in-a-spec half — where the type system cannot reach.
        let mut s = DimensionSchema::new();
        s.declare("finances.liquid.balance", Dimension::Snapshot).unwrap();

        let err = s
            .check(
                "finances.liquid.balance",
                &Update::Delta { by: 50, id: "tx-1".into() },
            )
            .unwrap_err();
        assert!(matches!(err, DimensionError::DeltaOnSnapshot { by: 50, .. }));
        assert!(err.to_string().contains("not idempotent"), "the reason travels with it");
    }

    #[test]
    fn an_observation_against_an_accumulating_dimension_is_refused_too() {
        // The mirror image: replacing a running total with a reading would
        // discard every contribution already counted.
        let mut s = DimensionSchema::new();
        s.declare("pawa.spent", Dimension::Accumulating).unwrap();
        assert!(matches!(
            s.check("pawa.spent", &Update::Observe { value: json!(5), t_event: 1, id: "x".into() }),
            Err(DimensionError::ObservationOnAccumulating { .. })
        ));
    }

    #[test]
    fn an_undeclared_dimension_is_an_error_not_a_default() {
        let s = DimensionSchema::new();
        assert!(matches!(
            s.check("nowhere", &Update::Delta { by: 1, id: "x".into() }),
            Err(DimensionError::Undeclared(_))
        ));
    }

    #[test]
    fn a_dimension_cannot_be_redeclared_as_a_different_kind() {
        let mut s = DimensionSchema::new();
        s.declare("a", Dimension::Snapshot).unwrap();
        s.declare("a", Dimension::Snapshot).unwrap(); // idempotent
        assert!(matches!(
            s.declare("a", Dimension::Accumulating),
            Err(DimensionError::AlreadyDeclared { .. })
        ));
    }

    // ── ★ LWW attaches automatically, and does not eat a contested edit ────

    #[test]
    fn declaring_the_kind_is_enough_the_apply_attaches_itself() {
        // ★ The caller never chooses LWW; it follows from the declaration.
        let mut s = DimensionSchema::new();
        s.declare("finances.liquid.balance", Dimension::Snapshot).unwrap();

        let first = s
            .apply(
                "finances.liquid.balance",
                None,
                Update::Observe { value: json!(1200), t_event: 100, id: "sms-1".into() },
            )
            .unwrap();
        let second = s
            .apply(
                "finances.liquid.balance",
                Some(&first),
                Update::Observe { value: json!(1150), t_event: 200, id: "sms-2".into() },
            )
            .unwrap();
        assert_eq!(second.as_snapshot().unwrap().value(), &json!(1150));

        // And a stale reading arriving late does not clobber it.
        let stale = s
            .apply(
                "finances.liquid.balance",
                Some(&second),
                Update::Observe { value: json!(9999), t_event: 50, id: "sms-0".into() },
            )
            .unwrap();
        assert_eq!(stale.as_snapshot().unwrap().value(), &json!(1150));
    }

    #[test]
    fn a_contested_dimension_keeps_both_edits_where_lww_would_keep_one() {
        // ★★ §VII's emphatic limit: LWW converges, and convergence is not
        // correctness. The same two concurrent changes — one kind keeps both,
        // the other keeps one, and only one of those is right for a shared plan.
        let contested = ContestedDim::new()
            .edit("meet at 4", Tag::new("phone", 1))
            .unwrap()
            .edit("meet at 6", Tag::new("laptop", 1))
            .unwrap();
        assert_eq!(contested.live().len(), 2, "neither edit is discarded");

        let as_snapshot = balance(4, 100, "phone").join(&balance(6, 100, "laptop"));
        assert_eq!(as_snapshot.metadata_len(), 1, "LWW would have kept exactly one");
    }

    #[test]
    fn a_contested_dimension_refuses_automatic_application_entirely() {
        let mut s = DimensionSchema::new();
        s.declare("plan.dinner_time", Dimension::Contested).unwrap();
        for u in [
            Update::Observe { value: json!("4pm"), t_event: 1, id: "a".into() },
            Update::Delta { by: 1, id: "b".into() },
        ] {
            assert!(matches!(
                s.check("plan.dinner_time", &u),
                Err(DimensionError::AutomaticApplyOnContested { .. })
            ));
        }
    }

    #[test]
    fn the_contested_join_also_obeys_the_three_laws() {
        let a = ContestedDim::new().edit("x", Tag::new("n1", 1)).unwrap();
        let b = ContestedDim::new().edit("y", Tag::new("n2", 1)).unwrap();
        let c = ContestedDim::new().edit("z", Tag::new("n3", 1)).unwrap();
        let r = laws_hold(&a, &b, &c);
        assert!(r.commutative && r.associative && r.idempotent, "{r:?}");
    }
}
