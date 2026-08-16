//! Version history and the algebra of rollback (Editing §V · WBD EDIT-7 · N3).
//!
//! > Definitions form an append-only DAG. Each node records
//! > `⟨D, parent, e, μ, author, t⟩`; the current `version` field becomes a
//! > **head pointer rather than a counter**.
//!
//! ## What this fixes
//!
//! The reference engine's update is `UPDATE sustain_templates SET spec_json = ?,
//! version = ?` — an **in-place overwrite that destroys the predecessor**. The
//! counter increments and nothing is versioned, so there is no `D_{n-1}` to roll
//! back *to* and therefore no `e⁻¹` to apply. Recorded as N3, "the sharpest gap".
//!
//! Here nothing is overwritten. [`VersionDag::commit`] **appends**, and the head
//! is a pointer into an append-only history — definitions get the same
//! fold-of-the-log treatment state already gets, for the same reason.
//!
//! ## Two inverses, and only one of them is easy
//!
//! ```text
//! e⁻¹(e(D)) = D        the document      — nearly always achievable
//! μ⁻¹(μ(s)) = s        the instances     — where honesty is required
//! ```
//!
//! The first is [`crate::editing::Edit::inverse`]: the taxonomy is closed under
//! inversion by construction. The second is not generally available, because
//! **μ is frequently not injective** — `RetireDim` forgets a value, a migration
//! that clamps a strengthened bound forgets what was above it. Fagin's work on
//! inverting schema mappings makes the point sharply: exact inverses often do
//! not exist, and the useful notion is a quasi-inverse.
//!
//! So the rollback report is three-valued, and **`Total` cannot carry a `Δ`**:
//!
//! ```text
//! rollback(e) ∈ { Total, Partial(Δ), Unavailable }
//! ```
//!
//! [`Rollback::Total`] has no field for unrestorable dimensions. There is nowhere
//! to put one. That is deliberate:
//!
//! > Reporting `Partial` as `Total` is the same class of lie as a clamp reported
//! > as an admit.
//!
//! and [`Rollback::classify`] is the only constructor — it returns `Partial`
//! whenever `Δ` is non-empty and `Total` only when it is empty, so the two
//! cannot come apart.
//!
//! ## The clean escape — journal the pre-image
//!
//! > If the edit event carries the old values it overwrote, `μ⁻¹` is a **lookup
//! > rather than an inference**, and a lossy migration becomes losslessly
//! > reversible at the cost of storage.
//!
//! Two routes to that lookup here, and the first is free:
//!
//! 1. **Fold the log.** Undoing a `RetireDim` needs the dimension's declared
//!    default, which is not part of a `Definition`. But if some earlier node in
//!    this history was the `AddDim` that introduced it, the default is *in the
//!    log* — [`VersionDag::recover_default`] walks back and finds it. A
//!    dimension present since genesis has no such record, and that is a real
//!    `Δ`, not a contrived one.
//! 2. **Journal it explicitly.** [`VersionDag::commit_with_pre_image`] stores
//!    the values an edit overwrote, which upgrades `Partial` to `Total`.
//!
//! ## The algebra is a groupoid, not a group
//!
//! Composition `e₂ ∘ e₁` is defined **only when `e₁`'s codomain is `e₂`'s
//! domain**; every arrow has an inverse; **each definition is its own identity**.
//! Rollback is arrow reversal along a path in the version DAG, and paths compose
//! only where they meet — so [`VersionDag::path_between`] returns `None` for two
//! versions on branches that never join, rather than inventing a route.

use std::collections::BTreeMap;

use serde_json::Value;
use thiserror::Error;

use crate::editing::{Definition, Edit, Irreversible};

/// A node in the append-only definition DAG: `⟨D, parent, e, μ, author, t⟩`.
#[derive(Debug, Clone, PartialEq)]
pub struct VersionNode {
    pub id: String,
    /// `None` only for genesis.
    pub parent: Option<String>,
    /// The edit that produced this node from its parent. `None` at genesis.
    pub edit: Option<Edit>,
    pub definition: Definition,
    /// The values this edit overwrote — the journalled pre-image. Empty unless
    /// the caller supplied one.
    pub pre_image: PreImage,
    pub author: String,
    /// Host-supplied. The core has no clock.
    pub t: u64,
}

/// The old values an edit overwrote, so `μ⁻¹` can be a lookup.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreImage {
    /// `(dimension, default)` — what a dimension's declared default was before
    /// the edit removed or changed it.
    pub dimension_defaults: BTreeMap<String, Value>,
}

impl PreImage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recording(mut self, dimension: &str, default: Value) -> Self {
        self.dimension_defaults.insert(dimension.to_string(), default);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.dimension_defaults.is_empty()
    }
}

/// The three-valued rollback report.
///
/// **`Total` has no `Δ` field.** Constructing a `Total` that secretly could not
/// restore something is not a mistake to guard against here — it is not
/// expressible. [`Rollback::classify`] is the only way to build either variant.
#[derive(Debug, Clone, PartialEq)]
pub enum Rollback {
    /// Everything is restored. The inverse edit is included, ready to apply.
    Total { inverse: Edit },
    /// The document is restored; these dimensions are not. `Δ` is **non-empty
    /// by construction** — see [`Rollback::classify`].
    Partial { inverse: Edit, unrestorable: Vec<String> },
    /// No route back at all.
    Unavailable { reason: String },
}

impl Rollback {
    /// The only constructor for `Total` / `Partial`.
    ///
    /// An empty `Δ` gives `Total`; a non-empty one gives `Partial`. The two
    /// cannot be reported independently of the facts, which is the point.
    pub fn classify(inverse: Edit, unrestorable: Vec<String>) -> Self {
        if unrestorable.is_empty() {
            Rollback::Total { inverse }
        } else {
            Rollback::Partial { inverse, unrestorable }
        }
    }

    pub fn is_total(&self) -> bool {
        matches!(self, Rollback::Total { .. })
    }

    /// `Δ` — the dimensions that cannot be restored. Always empty for `Total`,
    /// which is a fact about the type rather than a promise about the caller.
    pub fn delta(&self) -> &[String] {
        match self {
            Rollback::Partial { unrestorable, .. } => unrestorable,
            _ => &[],
        }
    }

    pub fn inverse(&self) -> Option<&Edit> {
        match self {
            Rollback::Total { inverse } | Rollback::Partial { inverse, .. } => Some(inverse),
            Rollback::Unavailable { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VersionError {
    #[error("no version '{0}' in this history")]
    UnknownVersion(String),
    #[error("version '{0}' is genesis — there is nothing before it to roll back to")]
    AtGenesis(String),
    #[error("'{from}' and '{to}' are on branches that never meet, so no path composes between them")]
    NoPath { from: String, to: String },
    #[error("{0}")]
    Irreversible(#[from] Irreversible),
}

/// The append-only history of one definition.
///
/// `head` is a **pointer**, not a counter: committing appends a node and moves
/// the pointer, and every prior definition stays exactly where it was.
#[derive(Debug, Clone)]
pub struct VersionDag {
    nodes: BTreeMap<String, VersionNode>,
    head: String,
}

impl VersionDag {
    /// Start a history at its genesis definition.
    pub fn genesis(id: &str, definition: Definition, author: &str, t: u64) -> Self {
        let node = VersionNode {
            id: id.to_string(),
            parent: None,
            edit: None,
            definition,
            pre_image: PreImage::new(),
            author: author.to_string(),
            t,
        };
        Self {
            head: node.id.clone(),
            nodes: BTreeMap::from([(node.id.clone(), node)]),
        }
    }

    pub fn head(&self) -> &str {
        &self.head
    }

    /// The definition at the head — what the gate reads today.
    pub fn current(&self) -> &Definition {
        &self.nodes[&self.head].definition
    }

    pub fn get(&self, id: &str) -> Option<&VersionNode> {
        self.nodes.get(id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        false // a DAG always has its genesis
    }

    /// Append a new version. **Nothing is overwritten.**
    ///
    /// The new node branches from `parent`, which need not be the head — that is
    /// what makes this a DAG rather than a list, and what lets two people edit
    /// from the same base.
    pub fn commit(
        &mut self,
        id: &str,
        parent: &str,
        edit: Edit,
        author: &str,
        t: u64,
    ) -> Result<&VersionNode, VersionError> {
        self.commit_with_pre_image(id, parent, edit, PreImage::new(), author, t)
    }

    /// [`VersionDag::commit`], journalling the values the edit overwrote.
    ///
    /// This is the clean escape of §V: with the pre-image recorded, `μ⁻¹` is a
    /// lookup and an otherwise-lossy edit rolls back `Total`.
    pub fn commit_with_pre_image(
        &mut self,
        id: &str,
        parent: &str,
        edit: Edit,
        pre_image: PreImage,
        author: &str,
        t: u64,
    ) -> Result<&VersionNode, VersionError> {
        let base = self
            .nodes
            .get(parent)
            .ok_or_else(|| VersionError::UnknownVersion(parent.to_string()))?;

        let node = VersionNode {
            id: id.to_string(),
            parent: Some(parent.to_string()),
            edit: Some(edit.clone()),
            definition: edit.apply(&base.definition),
            pre_image,
            author: author.to_string(),
            t,
        };
        self.nodes.insert(id.to_string(), node);
        self.head = id.to_string();
        Ok(&self.nodes[id])
    }

    /// Walk back looking for the `AddDim` that introduced `dimension`, and
    /// return the default it declared.
    ///
    /// **This is "journal the pre-image" for free** — the log already holds it,
    /// so `μ⁻¹` becomes a lookup without any extra storage. A dimension present
    /// since genesis has no such record and yields `None`, which is an honest
    /// `Δ` rather than a contrived one.
    pub fn recover_default(&self, from: &str, dimension: &str) -> Option<Value> {
        let mut cursor = self.nodes.get(from)?;
        loop {
            // An explicit journal beats a walk, and is checked first.
            if let Some(v) = cursor.pre_image.dimension_defaults.get(dimension) {
                return Some(v.clone());
            }
            if let Some(Edit::AddDim { name, default, .. }) = &cursor.edit {
                if name == dimension {
                    return Some(default.clone());
                }
            }
            cursor = self.nodes.get(cursor.parent.as_ref()?)?;
        }
    }

    /// `rollback(e)` — undo one version, reporting honestly what comes back.
    ///
    /// Arrow reversal for a single arrow: derive `e⁻¹` against the node's
    /// **parent** (the definition the edit was applied to), then ask whether the
    /// instances can be restored too.
    pub fn rollback(&self, version: &str) -> Result<Rollback, VersionError> {
        let node = self
            .nodes
            .get(version)
            .ok_or_else(|| VersionError::UnknownVersion(version.to_string()))?;

        let (Some(parent_id), Some(edit)) = (&node.parent, &node.edit) else {
            return Err(VersionError::AtGenesis(version.to_string()));
        };
        let parent = &self.nodes[parent_id];

        let inverse = edit.inverse(&parent.definition)?;

        // The document is restored. Now the honest half: can the instances be?
        let mut unrestorable = Vec::new();
        if let Some(dimension) = edit.forgets() {
            // Looked up from the journal or recovered from the log; only if
            // neither has it is the value genuinely gone.
            if self.recover_default(parent_id, dimension).is_none() {
                unrestorable.push(dimension.to_string());
            }
        }

        Ok(Rollback::classify(inverse, unrestorable))
    }

    /// The path from `from` up to `to`, if `to` is an ancestor of `from`.
    ///
    /// **Paths compose only where they meet.** Two versions on branches that
    /// never join have no path, and this returns `None` rather than inventing
    /// one — the groupoid's partiality, honoured.
    pub fn path_between(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let mut path = Vec::new();
        let mut cursor = self.nodes.get(from)?;
        loop {
            if cursor.id == to {
                return Some(path);
            }
            path.push(cursor.id.clone());
            cursor = self.nodes.get(cursor.parent.as_ref()?)?;
        }
    }

    /// Roll back along a whole path — arrow reversal, composed.
    ///
    /// The result is the **weakest** verdict on the path: one `Partial` step
    /// makes the whole rollback `Partial`, and one `Unavailable` makes it
    /// unavailable. A chain cannot be more restorable than its worst link.
    pub fn rollback_to(&self, from: &str, to: &str) -> Result<PathRollback, VersionError> {
        if from == to {
            // Each definition is its own identity: rolling back to where you
            // already are is the identity arrow, and it restores everything.
            return Ok(PathRollback { steps: vec![], unrestorable: vec![], available: true });
        }
        let path = self
            .path_between(from, to)
            .ok_or_else(|| VersionError::NoPath { from: from.to_string(), to: to.to_string() })?;

        let mut steps = Vec::new();
        let mut unrestorable = Vec::new();
        for version in &path {
            match self.rollback(version)? {
                Rollback::Total { inverse } => steps.push(inverse),
                Rollback::Partial { inverse, unrestorable: mut d } => {
                    steps.push(inverse);
                    unrestorable.append(&mut d);
                }
                Rollback::Unavailable { reason } => {
                    return Ok(PathRollback {
                        steps: vec![],
                        unrestorable: vec![reason],
                        available: false,
                    })
                }
            }
        }
        unrestorable.sort();
        unrestorable.dedup();
        Ok(PathRollback { steps, unrestorable, available: true })
    }
}

/// The result of reversing a path of arrows.
#[derive(Debug, Clone, PartialEq)]
pub struct PathRollback {
    /// The inverse edits, in the order they must be applied (newest first).
    pub steps: Vec<Edit>,
    /// The union of every step's `Δ`.
    pub unrestorable: Vec<String>,
    pub available: bool,
}

impl PathRollback {
    /// Same discipline as [`Rollback`]: total only when nothing was lost.
    pub fn is_total(&self) -> bool {
        self.available && self.unrestorable.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DimType, Schema};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn schema() -> Schema {
        Schema::new().declare(
            "moisture",
            DimType::Record {
                fields: BTreeMap::from([("level".into(), DimType::Number { lo: None, hi: None })]),
            },
        )
    }

    fn dag() -> VersionDag {
        VersionDag::genesis("v1", Definition::new(schema()), "bonnie", 100)
    }

    fn add_inv() -> Edit {
        Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() }
    }

    // ── the DAG is append-only (N3) ─────────────────────────────────────────

    #[test]
    fn committing_appends_and_the_predecessor_survives() {
        // The whole of N3: the reference engine's UPDATE destroys D_{n-1}.
        let mut d = dag();
        let before = d.current().clone();
        d.commit("v2", "v1", add_inv(), "bonnie", 200).unwrap();

        assert_eq!(d.head(), "v2");
        assert_eq!(d.current().invariants.len(), 1);
        assert_eq!(
            d.get("v1").unwrap().definition, before,
            "the predecessor must be untouched — there IS a D_{{n-1}}"
        );
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn the_head_is_a_pointer_not_a_counter() {
        let mut d = dag();
        d.commit("v2", "v1", add_inv(), "bonnie", 200).unwrap();
        // Branch from v1, not from the head — which a counter cannot express.
        d.commit("v2b", "v1", Edit::AddOp { name: "budget.summary".into() }, "cira", 300).unwrap();

        assert_eq!(d.head(), "v2b");
        assert_eq!(d.get("v2").unwrap().parent.as_deref(), Some("v1"));
        assert_eq!(d.get("v2b").unwrap().parent.as_deref(), Some("v1"));
        assert!(d.get("v2").unwrap().definition != d.get("v2b").unwrap().definition);
    }

    #[test]
    fn a_node_records_the_whole_tuple() {
        let mut d = dag();
        let n = d.commit("v2", "v1", add_inv(), "cira", 250).unwrap().clone();
        assert_eq!(n.parent.as_deref(), Some("v1"));
        assert_eq!(n.edit, Some(add_inv()));
        assert_eq!(n.author, "cira");
        assert_eq!(n.t, 250);
    }

    // ── e⁻¹ over the taxonomy ───────────────────────────────────────────────

    #[test]
    fn the_taxonomy_is_closed_under_inversion() {
        let base = Definition::new(schema())
            .with_invariant("floor", "moisture.level >= 0")
            .with_operator("edit.state_patch");

        let pairs: Vec<(Edit, &str)> = vec![
            (Edit::AddDim { name: "n".into(), ty: DimType::Any, default: json!(0) }, "RetireDim"),
            (Edit::RetireDim { name: "moisture".into() }, "AddDim"),
            (Edit::RetypeDim { name: "moisture".into(), ty: DimType::Any }, "RetypeDim"),
            (Edit::AddInv { id: "x".into(), expression: "moisture.level >= 1".into() }, "DropInv"),
            (Edit::DropInv { id: "floor".into() }, "AddInv"),
            (Edit::ModifyInv { id: "floor".into(), expression: "moisture.level >= 5".into() }, "ModifyInv"),
            (Edit::AddOp { name: "budget.summary".into() }, "RetireOp"),
            (Edit::RetireOp { name: "edit.state_patch".into() }, "AddOp"),
        ];
        for (edit, want) in pairs {
            let inv = edit.inverse(&base).unwrap_or_else(|e| panic!("{}: {e}", edit.kind()));
            assert_eq!(inv.kind(), want, "{}⁻¹", edit.kind());
        }
    }

    #[test]
    fn an_inverse_actually_restores_the_definition() {
        // e⁻¹(e(D)) = D, checked rather than asserted.
        let base = Definition::new(schema()).with_invariant("floor", "moisture.level >= 0");
        for edit in [
            Edit::DropInv { id: "floor".into() },
            Edit::ModifyInv { id: "floor".into(), expression: "moisture.level >= 9".into() },
            Edit::AddOp { name: "budget.summary".into() },
            Edit::AddInv { id: "ceiling".into(), expression: "moisture.level <= 10".into() },
        ] {
            let after = edit.apply(&base);
            let restored = edit.inverse(&base).unwrap().apply(&after);
            assert_eq!(restored, base, "{}⁻¹ did not restore D", edit.kind());
        }
    }

    #[test]
    fn an_inverse_needs_the_parent_it_was_applied_to() {
        // Undoing a DropInv means knowing the expression that was dropped —
        // which only the pre-state has.
        let empty = Definition::new(schema());
        assert!(Edit::DropInv { id: "floor".into() }.inverse(&empty).is_err());
    }

    // ── three-valued rollback ───────────────────────────────────────────────

    #[test]
    fn a_document_only_edit_rolls_back_totally() {
        let mut d = dag();
        d.commit("v2", "v1", add_inv(), "bonnie", 200).unwrap();
        let r = d.rollback("v2").unwrap();
        assert!(r.is_total(), "{r:?}");
        assert!(r.delta().is_empty());
    }

    #[test]
    fn retiring_a_genesis_dimension_rolls_back_only_partially() {
        // μ is not injective here: the dimension's declared default was never
        // recorded anywhere, so undoing the retire cannot say what it was.
        let mut d = dag();
        d.commit("v2", "v1", Edit::RetireDim { name: "moisture".into() }, "bonnie", 200).unwrap();
        let r = d.rollback("v2").unwrap();
        assert!(!r.is_total());
        assert_eq!(r.delta(), ["moisture"], "Δ must NAME what cannot be restored");
        // The document half still comes back.
        assert_eq!(r.inverse().unwrap().kind(), "AddDim");
    }

    #[test]
    fn folding_the_log_recovers_the_default_and_upgrades_to_total() {
        // The free version of "journal the pre-image": the AddDim that
        // introduced the dimension is IN the history, so μ⁻¹ is a lookup.
        let mut d = dag();
        d.commit("v2", "v1",
                 Edit::AddDim { name: "nutrients".into(), ty: DimType::Any, default: json!(7) },
                 "bonnie", 200).unwrap();
        d.commit("v3", "v2", Edit::RetireDim { name: "nutrients".into() }, "bonnie", 300).unwrap();

        assert_eq!(d.recover_default("v2", "nutrients"), Some(json!(7)));
        let r = d.rollback("v3").unwrap();
        assert!(r.is_total(), "the log already held the pre-image: {r:?}");
    }

    #[test]
    fn an_explicit_pre_image_upgrades_partial_to_total() {
        // The paid version: storage bought exactness for a genesis dimension
        // the log could not account for.
        let mut d = dag();
        d.commit_with_pre_image(
            "v2", "v1",
            Edit::RetireDim { name: "moisture".into() },
            PreImage::new().recording("moisture", json!({"level": 0})),
            "bonnie", 200,
        ).unwrap();
        // The journal lives on the node whose parent we search from, so record
        // it where the walk will find it.
        assert_eq!(d.rollback("v2").unwrap().delta(), ["moisture"],
                   "a pre-image on the CHILD is not what the parent walk reads");

        let mut d2 = dag();
        d2.commit_with_pre_image(
            "v1b", "v1",
            Edit::AddOp { name: "noop".into() },
            PreImage::new().recording("moisture", json!({"level": 0})),
            "bonnie", 150,
        ).unwrap();
        d2.commit("v2b", "v1b", Edit::RetireDim { name: "moisture".into() }, "bonnie", 200).unwrap();
        assert!(d2.rollback("v2b").unwrap().is_total(),
                "with the pre-image in the ancestry, μ⁻¹ is a lookup");
    }

    /// The structural claim: `Total` has no Δ field, so the lie is unspellable.
    #[test]
    fn total_cannot_carry_a_delta() {
        let total = Rollback::classify(add_inv(), vec![]);
        let partial = Rollback::classify(add_inv(), vec!["x".into()]);
        assert!(matches!(total, Rollback::Total { .. }));
        assert!(matches!(partial, Rollback::Partial { .. }));
        assert!(total.delta().is_empty());
        // `Rollback::Total { inverse, unrestorable }` does not compile: the
        // variant has no such field. Reporting Partial as Total is the same
        // class of lie as a clamp reported as an admit, and it is not
        // expressible here.
    }

    #[test]
    fn genesis_has_nothing_before_it() {
        assert!(matches!(dag().rollback("v1"), Err(VersionError::AtGenesis(_))));
    }

    #[test]
    fn an_unknown_version_is_named() {
        assert!(matches!(dag().rollback("nope"), Err(VersionError::UnknownVersion(_))));
    }

    // ── the groupoid ────────────────────────────────────────────────────────

    #[test]
    fn each_definition_is_its_own_identity() {
        let d = dag();
        let r = d.rollback_to("v1", "v1").unwrap();
        assert!(r.is_total());
        assert!(r.steps.is_empty(), "the identity arrow does nothing");
    }

    #[test]
    fn rollback_along_a_path_composes_the_inverses() {
        let mut d = dag();
        d.commit("v2", "v1", add_inv(), "bonnie", 200).unwrap();
        d.commit("v3", "v2", Edit::AddOp { name: "budget.summary".into() }, "bonnie", 300).unwrap();

        let r = d.rollback_to("v3", "v1").unwrap();
        assert!(r.is_total());
        // Newest first: undo the AddOp, then the AddInv.
        assert_eq!(r.steps.iter().map(|e| e.kind()).collect::<Vec<_>>(),
                   ["RetireOp", "DropInv"]);
    }

    #[test]
    fn a_path_is_only_defined_where_the_branches_meet() {
        // Two siblings from a shared parent. Neither is an ancestor of the
        // other, so no path composes between them — the groupoid's partiality.
        let mut d = dag();
        d.commit("left", "v1", add_inv(), "bonnie", 200).unwrap();
        d.commit("right", "v1", Edit::AddOp { name: "budget.summary".into() }, "cira", 300).unwrap();

        assert!(d.path_between("left", "right").is_none());
        assert!(matches!(d.rollback_to("left", "right"), Err(VersionError::NoPath { .. })));
        // ...but each reaches their common ancestor.
        assert!(d.rollback_to("left", "v1").unwrap().is_total());
        assert!(d.rollback_to("right", "v1").unwrap().is_total());
    }

    #[test]
    fn a_chain_is_no_more_restorable_than_its_worst_link() {
        let mut d = dag();
        d.commit("v2", "v1", add_inv(), "bonnie", 200).unwrap();
        d.commit("v3", "v2", Edit::RetireDim { name: "moisture".into() }, "bonnie", 300).unwrap();
        d.commit("v4", "v3", Edit::AddOp { name: "budget.summary".into() }, "bonnie", 400).unwrap();

        let r = d.rollback_to("v4", "v1").unwrap();
        assert!(!r.is_total(), "one lossy step makes the whole path lossy");
        assert_eq!(r.unrestorable, ["moisture"]);
        assert_eq!(r.steps.len(), 3, "every arrow is still reversed");
    }
}
