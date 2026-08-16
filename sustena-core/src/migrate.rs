//! The migration function μ, and Expand–Migrate–Contract (Editing §VIII · EDIT-12).
//!
//! Until this module, `μ` had no representation anywhere: the classification was
//! binary — *compatible*, or *refused* — because "migratable" needs a migration
//! function to represent it with. The discipline was **refuse-if-unsafe**, never
//! *migrate*. This is the piece that changes that, and it is the last of M-EDIT.
//!
//! ## The pattern, and why the ordering IS the content
//!
//! Factor a strengthening edit into three, in this order:
//!
//! ```text
//! D --e₊--> D† --μ--> D† --e₋--> D'
//! ```
//!
//! - **`e₊` expand** — introduce the new shape *without removing the old*. It is
//!   a **weakening** (`V_D ⊆ V_{D†}`), so §IV's asymmetry makes it **auto-safe
//!   with no check at all** — [`crate::editing::Edit::can_strand`] returns false
//!   for it, and the `|inst| × |Inv|` scan is skipped rather than merely passed.
//! - **`μ` migrate** — move each live instance's state into the new shape, **in
//!   place, one instance at a time, with every intermediate still in `V_{D†}`**.
//! - **`e₋` contract** — remove the old shape. It *is* a strengthening — but by
//!   the time it runs, `Safe(e₋, id)` holds **by construction**, because
//!   migration already put every instance where the check requires.
//!
//! > The one destructive step is made safe by being sequenced last, and at no
//! > point in the sequence is a live instance without a shape that holds it.
//!
//! That is why [`Emc::run`] is one function. There is no public way to run the
//! contract on its own, so the ordering cannot be got wrong by a caller in a
//! hurry — the sequence is the safety property, not a convention around it.
//!
//! ## Two directions of compatibility, named separately
//!
//! Kleppmann's distinction, and both matter here for different reasons:
//!
//! - **backward compatible** — *new code reads old data*. Required because `D'`'s
//!   Enzymes will run on states written under `D`.
//! - **forward compatible** — *old code reads new data*. Required **because
//!   rollback exists** (EDIT-7): `D`'s Enzymes must survive states already
//!   touched by `μ`.
//!
//! An edit achieving only the first is **one-way**. It may still be the right
//! edit — but *"this commits you"* is a fact the author is entitled to **at
//! author time**, not a discovery made during a rollback. So [`Emc::plan`]
//! returns the [`Compatibility`] before anything runs, and [`Emc::run`] cannot be
//! reached without it.
//!
//! ## The knock-on to EDIT-7
//!
//! A μ that **journals its pre-image** collapses the `Partial` rollback region to
//! `Total`: the values the contract dropped are recorded, so `μ⁻¹` is a lookup
//! rather than an inference. [`Emc::run`] does this automatically — every
//! [`StateMove::Drop`] contributes to the [`PreImage`] the contract version is
//! committed with, so a migration is losslessly reversible **by default**, at the
//! storage cost the article names.

use serde_json::Value;
use thiserror::Error;

use crate::editing::{Definition, Edit, Instance, Migration, Stranded};
use crate::schema::DimType;
use crate::state::State;
use crate::version::{PreImage, VersionDag, VersionError};

/// One step of a migration: how a value moves into the new shape.
///
/// Small and declarative on purpose — the same discipline as the effect summary
/// in [`crate::compose`]. A μ that could run arbitrary code would be an escape
/// hatch out of everything the gate guarantees.
#[derive(Debug, Clone, PartialEq)]
pub enum StateMove {
    /// Copy `from` to `to`, **leaving `from` in place**. This is the expand
    /// phase's move: both shapes coexist, which is exactly what makes the
    /// transition period safe and the result forward-compatible.
    Copy { from: String, to: String },
    /// Backfill a path with a literal — a new dimension's declared default.
    Fill { path: String, value: Value },
    /// Remove a path. Only the contract phase should do this, and doing it
    /// records the old value in the pre-image.
    Drop { path: String },
}

impl StateMove {
    fn describe(&self) -> String {
        match self {
            StateMove::Copy { from, to } => format!("copy {from} → {to}"),
            StateMove::Fill { path, value } => format!("fill {path} = {value}"),
            StateMove::Drop { path } => format!("drop {path}"),
        }
    }
}

/// **μ** — the migration function, as declared data.
///
/// `μ : S_D → S_D'`. Applied per instance, in place, one at a time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mu {
    pub moves: Vec<StateMove>,
}

impl Mu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn copying(mut self, from: &str, to: &str) -> Self {
        self.moves.push(StateMove::Copy { from: from.to_string(), to: to.to_string() });
        self
    }

    pub fn filling(mut self, path: &str, value: Value) -> Self {
        self.moves.push(StateMove::Fill { path: path.to_string(), value });
        self
    }

    pub fn dropping(mut self, path: &str) -> Self {
        self.moves.push(StateMove::Drop { path: path.to_string() });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    /// Apply μ to one instance's state.
    ///
    /// A `Copy` whose source is absent is an error rather than a silent skip:
    /// migrating an instance that does not have what the migration assumed is
    /// exactly the case where quietly doing nothing produces a half-migrated
    /// estate nobody notices.
    pub fn apply(&self, state: &Value) -> Result<Value, MigrateError> {
        let mut working = State::new(state.clone());
        for m in &self.moves {
            match m {
                StateMove::Copy { from, to } => {
                    let value = working
                        .get(from)
                        .cloned()
                        .ok_or_else(|| MigrateError::SourceMissing { path: from.clone() })?;
                    working
                        .set(to, value)
                        .map_err(|e| MigrateError::Unapplicable { mv: m.describe(), detail: e.to_string() })?;
                }
                StateMove::Fill { path, value } => {
                    working
                        .set(path, value.clone())
                        .map_err(|e| MigrateError::Unapplicable { mv: m.describe(), detail: e.to_string() })?;
                }
                StateMove::Drop { path } => {
                    // The core's State has no delete; a dropped path is set to
                    // null, and the DECLARATION is what removes it from the
                    // shape. Recorded here rather than left to be discovered:
                    // §VIII's contract step is about the definition, and the
                    // stored value is what the pre-image preserves.
                    working
                        .set(path, Value::Null)
                        .map_err(|e| MigrateError::Unapplicable { mv: m.describe(), detail: e.to_string() })?;
                }
            }
        }
        Ok(working.snapshot())
    }

    /// The values this μ would erase, read off an instance before it runs.
    ///
    /// This is the pre-image journal of §V, produced automatically — which is
    /// what collapses EDIT-7's `Partial` rollback region to `Total`.
    pub fn pre_image_of(&self, state: &Value) -> PreImage {
        let reader = State::new(state.clone());
        let mut pre = PreImage::new();
        for m in &self.moves {
            if let StateMove::Drop { path } = m {
                if let Some(v) = reader.get(path) {
                    // Journal under the top-level dimension name, which is what
                    // a rollback of a RetireDim asks for.
                    let dimension = path.split('.').next().unwrap_or(path);
                    pre = pre.recording(dimension, v.clone());
                }
            }
        }
        pre
    }

    /// Does μ leave the old shape readable? See [`Compatibility`].
    fn preserves_old_shape(&self) -> bool {
        !self.moves.iter().any(|m| matches!(m, StateMove::Drop { .. }))
    }
}

/// The two directions, named separately because they are required for different
/// reasons and an edit can have one without the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compatibility {
    /// New code reads old data — `D'`'s Enzymes on `D`-written states.
    pub backward: bool,
    /// Old code reads new data — `D`'s Enzymes on μ-touched states. Required
    /// **because rollback exists**.
    pub forward: bool,
}

impl Compatibility {
    /// > An edit that achieves only the first is one-way.
    ///
    /// Surfaced at author time by [`Emc::plan`], never as a discovery during a
    /// rollback.
    pub fn is_one_way(&self) -> bool {
        self.backward && !self.forward
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MigrateError {
    #[error("migration expected '{path}' to exist on this instance, and it does not")]
    SourceMissing { path: String },

    #[error("migration step '{mv}' could not be applied: {detail}")]
    Unapplicable { mv: String, detail: String },

    #[error("the expand step must be a weakening, but {kind} can strand a live instance — Expand–Migrate–Contract only works if the new shape is added beside the old, never instead of it")]
    ExpandIsNotAWeakening { kind: &'static str },

    #[error("after migrating, the contract step would still strand {} instance(s) — μ did not put them where the check requires", .0.len())]
    ContractStillStrands(Vec<Stranded>),

    #[error("{0}")]
    Version(#[from] VersionError),
}

/// A planned Expand–Migrate–Contract.
///
/// Constructed only by [`Emc::plan`], which is where `e₊` is checked to be a
/// weakening and the [`Compatibility`] is computed — so holding one means the
/// author has been told what it commits them to.
#[derive(Debug, Clone, PartialEq)]
pub struct Emc {
    expand: Edit,
    mu: Mu,
    contract: Edit,
    compatibility: Compatibility,
}

impl Emc {
    /// Plan the three phases, refusing an `e₊` that is not a weakening.
    pub fn plan(expand: Edit, mu: Mu, contract: Edit) -> Result<Self, MigrateError> {
        if expand.can_strand() {
            return Err(MigrateError::ExpandIsNotAWeakening { kind: expand.kind() });
        }
        let compatibility = Compatibility {
            // The new shape is written by μ before anything reads it under D'.
            backward: true,
            // Old code can still read what it knew, unless μ erased it.
            forward: mu.preserves_old_shape(),
        };
        Ok(Self { expand, mu, contract, compatibility })
    }

    /// The canonical reshape: move `old` to `new`, then retire `old`.
    ///
    /// Expand adds the new dimension, μ copies the value across, contract
    /// retires the old one — the three phases the article names, wired the way
    /// it names them.
    pub fn reshape(old: &str, new: &str, ty: DimType, default: Value) -> Result<Self, MigrateError> {
        Self::plan(
            Edit::AddDim { name: new.to_string(), ty, default },
            Mu::new().copying(old, new).dropping(old),
            Edit::RetireDim { name: old.to_string() },
        )
    }

    pub fn expand(&self) -> &Edit {
        &self.expand
    }
    pub fn mu(&self) -> &Mu {
        &self.mu
    }
    pub fn contract(&self) -> &Edit {
        &self.contract
    }

    /// **Told at author time, not discovered during a rollback.**
    pub fn compatibility(&self) -> Compatibility {
        self.compatibility
    }

    /// The transition definition `D†` — both shapes present at once.
    pub fn transitional(&self, d: &Definition) -> Definition {
        self.expand.apply(d)
    }

    /// Run all three phases, in order, on a version history and its instances.
    ///
    /// There is **no public way to run the contract alone**. The sequence is the
    /// safety property, so it is one call.
    ///
    /// Each phase is committed as its own version node, so the history records
    /// the transition period rather than collapsing it — and the contract node
    /// carries the **pre-image** μ erased, which is what makes the whole thing
    /// reversible `Total` rather than `Partial`.
    pub fn run(
        &self,
        dag: &mut VersionDag,
        instances: &[Instance],
        ids: EmcVersionIds,
        author: &str,
        t: u64,
    ) -> Result<Applied, MigrateError> {
        let base = dag.head().to_string();

        // ── e₊ expand — a weakening, so no scan at all ──────────────────────
        // `plan` already refused anything that could strand, which is what
        // licenses skipping the check here rather than running and passing it.
        dag.commit(ids.expand, &base, self.expand.clone(), author, t)?;
        let transitional = dag.current().clone();

        // ── μ migrate — one instance at a time, each intermediate in V_D† ───
        let mut migrated = Vec::with_capacity(instances.len());
        let mut pre_image = PreImage::new();
        for instance in instances {
            let before = &instance.state;
            let after = self.mu.apply(before)?;

            // Every intermediate must still be in V_{D†}. Checked per instance,
            // because migrating in place means each one is briefly the only
            // thing standing between the old shape and the new.
            if let Err(stranded) =
                crate::editing::safe(&transitional, &[Instance { id: instance.id.clone(), state: after.clone() }], &Migration::Identity)
            {
                return Err(MigrateError::ContractStillStrands(stranded));
            }

            // Journal what this instance is about to lose.
            for (dim, v) in self.mu.pre_image_of(before).dimension_defaults {
                pre_image = pre_image.recording(&dim, v);
            }
            migrated.push(Instance { id: instance.id.clone(), state: after });
        }

        // ── e₋ contract — Safe holds BY CONSTRUCTION, and it is checked ─────
        // The article's claim is that migration already put every instance where
        // the check requires. That is a claim worth verifying rather than
        // trusting: if it fails, the migration was wrong, and saying so beats
        // discovering it later.
        let contracted = self.contract.apply(&transitional);
        if let Err(stranded) = crate::editing::safe(&contracted, &migrated, &Migration::Identity) {
            return Err(MigrateError::ContractStillStrands(stranded));
        }

        dag.commit_with_pre_image(
            ids.contract,
            ids.expand,
            self.contract.clone(),
            pre_image,
            author,
            t,
        )?;

        Ok(Applied {
            transitional_version: ids.expand.to_string(),
            final_version: ids.contract.to_string(),
            instances: migrated,
            compatibility: self.compatibility,
        })
    }
}

/// The version ids the three phases are committed under.
#[derive(Debug, Clone, Copy)]
pub struct EmcVersionIds<'a> {
    pub expand: &'a str,
    pub contract: &'a str,
}

/// Proof that all three phases ran, in order.
///
/// Returned only by [`Emc::run`], so an `Applied` in hand means the destructive
/// step was sequenced last.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub transitional_version: String,
    pub final_version: String,
    /// The migrated instance states.
    pub instances: Vec<Instance>,
    pub compatibility: Compatibility,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::{safe, Definition};
    use crate::schema::Schema;
    use crate::version::Rollback;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn schema() -> Schema {
        Schema::new()
            .declare("legacy", DimType::Record {
                fields: BTreeMap::from([("level".into(), DimType::Number { lo: None, hi: None })]),
            })
    }

    fn base() -> Definition {
        Definition::new(schema())
    }

    fn instances() -> Vec<Instance> {
        vec![
            Instance { id: "a".into(), state: json!({"legacy": {"level": 5}}) },
            Instance { id: "b".into(), state: json!({"legacy": {"level": 9}}) },
        ]
    }

    fn dag() -> VersionDag {
        VersionDag::genesis("v1", base(), "bonnie", 100)
    }

    fn ids() -> EmcVersionIds<'static> {
        EmcVersionIds { expand: "v2", contract: "v3" }
    }

    // ── μ has a representation at last ──────────────────────────────────────

    #[test]
    fn mu_moves_a_value_into_the_new_shape() {
        let mu = Mu::new().copying("legacy.level", "moisture.level");
        let out = mu.apply(&json!({"legacy": {"level": 5}})).unwrap();
        assert_eq!(out["moisture"]["level"], json!(5));
        assert_eq!(out["legacy"]["level"], json!(5), "copy leaves the old shape in place");
    }

    #[test]
    fn mu_backfills_a_default() {
        let mu = Mu::new().filling("moisture.level", json!(0));
        let out = mu.apply(&json!({})).unwrap();
        assert_eq!(out["moisture"]["level"], json!(0));
    }

    #[test]
    fn a_missing_source_is_an_error_not_a_silent_skip() {
        // Quietly doing nothing is how a half-migrated estate happens.
        let mu = Mu::new().copying("nowhere.at.all", "moisture.level");
        assert!(matches!(
            mu.apply(&json!({"legacy": {"level": 5}})),
            Err(MigrateError::SourceMissing { .. })
        ));
    }

    #[test]
    fn safe_now_judges_the_migrated_state_not_the_original() {
        // THE point of EDIT-12: without μ, this edit is refused. With it, the
        // instances are carried into the new shape and the edit is admitted.
        let candidate = base().with_invariant("floor", "moisture.level >= 0");
        let raw = instances();

        assert!(safe(&candidate, &raw, &Migration::Identity).is_err(),
                "with μ = id the instances have no moisture.level at all");

        let mu = Mu::new().copying("legacy.level", "moisture.level");
        assert!(safe(&candidate, &raw, &Migration::Apply(mu)).is_ok(),
                "μ puts them where the check requires — refuse-if-unsafe becomes migrate");
    }

    // ── expand is a weakening, and that is enforced ──────────────────────────

    #[test]
    fn expand_must_be_a_weakening() {
        let bad = Emc::plan(
            Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() },
            Mu::new(),
            Edit::RetireDim { name: "legacy".into() },
        );
        assert!(matches!(bad, Err(MigrateError::ExpandIsNotAWeakening { kind: "AddInv" })));
    }

    #[test]
    fn adding_a_dimension_cannot_strand_so_expand_skips_the_scan_entirely() {
        // §IV's asymmetry is what makes e₊ auto-safe rather than merely cheap.
        assert!(!Edit::AddDim { name: "x".into(), ty: DimType::Any, default: json!(0) }.can_strand());
        assert!(!Edit::RetireDim { name: "x".into() }.can_strand());
        assert!(!Edit::DropInv { id: "x".into() }.can_strand());
        // ...and the three that genuinely can.
        assert!(Edit::AddInv { id: "x".into(), expression: "a >= 0".into() }.can_strand());
        assert!(Edit::ModifyInv { id: "x".into(), expression: "a >= 0".into() }.can_strand());
        assert!(Edit::RetypeDim { name: "x".into(), ty: DimType::Any }.can_strand());
    }

    // ── the sequence ────────────────────────────────────────────────────────

    #[test]
    fn the_three_phases_run_in_order_and_every_instance_arrives() {
        let mut d = dag();
        let emc = Emc::reshape("legacy", "moisture", DimType::Any, json!({"level": 0})).unwrap();
        let applied = emc.run(&mut d, &instances(), ids(), "bonnie", 200).unwrap();

        assert_eq!(applied.transitional_version, "v2");
        assert_eq!(applied.final_version, "v3");
        assert_eq!(applied.instances.len(), 2);
        assert_eq!(applied.instances[0].state["moisture"]["level"], json!(5));
        assert_eq!(applied.instances[1].state["moisture"]["level"], json!(9));
    }

    #[test]
    fn the_transition_period_is_recorded_not_collapsed() {
        // D† is a real version with BOTH shapes — which is what makes the
        // transition period inspectable rather than a moment nobody can see.
        let mut d = dag();
        let emc = Emc::reshape("legacy", "moisture", DimType::Any, json!({"level": 0})).unwrap();
        emc.run(&mut d, &instances(), ids(), "bonnie", 200).unwrap();

        let transitional = &d.get("v2").unwrap().definition;
        assert!(transitional.schema.top_level().any(|n| n == "legacy"), "old shape still there");
        assert!(transitional.schema.top_level().any(|n| n == "moisture"), "new shape too");

        let final_def = &d.get("v3").unwrap().definition;
        assert!(!final_def.schema.top_level().any(|n| n == "legacy"), "and only then removed");
    }

    #[test]
    fn at_no_point_is_an_instance_without_a_shape_that_holds_it() {
        // Walk the sequence and check the invariant at each step, rather than
        // trusting the ordering to imply it.
        let d0 = base().with_invariant("legacy_floor", "legacy.level >= 0");
        let emc = Emc::plan(
            Edit::AddDim { name: "moisture".into(), ty: DimType::Any, default: json!({"level": 0}) },
            Mu::new().copying("legacy.level", "moisture.level"),
            Edit::RetireOp { name: "nothing".into() },
        )
        .unwrap();

        let raw = instances();
        assert!(safe(&d0, &raw, &Migration::Identity).is_ok(), "before: held by D");
        let transitional = emc.transitional(&d0);
        assert!(safe(&transitional, &raw, &Migration::Identity).is_ok(), "during: held by D†");
        let migrated: Vec<Instance> = raw.iter()
            .map(|i| Instance { id: i.id.clone(), state: emc.mu().apply(&i.state).unwrap() })
            .collect();
        assert!(safe(&transitional, &migrated, &Migration::Identity).is_ok(), "after μ: still held");
    }

    #[test]
    fn a_migration_that_does_not_deliver_is_refused_at_the_contract() {
        // The article says Safe(e₋, id) holds BY CONSTRUCTION. That is a claim
        // worth verifying: if μ was wrong, saying so beats discovering it later.
        let mut d = VersionDag::genesis(
            "v1",
            base().with_invariant("floor", "moisture.level >= 100"),
            "bonnie", 100,
        );
        let emc = Emc::plan(
            Edit::AddDim { name: "moisture".into(), ty: DimType::Any, default: json!({"level": 0}) },
            Mu::new().copying("legacy.level", "moisture.level"),
            Edit::RetireDim { name: "legacy".into() },
        )
        .unwrap();
        // levels 5 and 9 do not reach the floor of 100.
        let out = emc.run(&mut d, &instances(), ids(), "bonnie", 200);
        assert!(matches!(out, Err(MigrateError::ContractStillStrands(_))), "{out:?}");
    }

    // ── compatibility, at author time ───────────────────────────────────────

    #[test]
    fn a_migration_that_keeps_the_old_shape_is_both_ways() {
        let emc = Emc::plan(
            Edit::AddDim { name: "moisture".into(), ty: DimType::Any, default: json!(0) },
            Mu::new().copying("legacy.level", "moisture.level"),
            Edit::RetireOp { name: "x".into() },
        )
        .unwrap();
        let c = emc.compatibility();
        assert!(c.backward && c.forward);
        assert!(!c.is_one_way());
    }

    #[test]
    fn a_migration_that_erases_the_old_shape_is_one_way_and_says_so_up_front() {
        // "This commits you" is a fact the author is entitled to at author
        // time — available from `plan`, before `run` is ever called.
        let emc = Emc::reshape("legacy", "moisture", DimType::Any, json!(0)).unwrap();
        let c = emc.compatibility();
        assert!(c.backward, "new code can read migrated data");
        assert!(!c.forward, "old code cannot read what μ erased");
        assert!(c.is_one_way());
    }

    // ── the knock-on to EDIT-7 ──────────────────────────────────────────────

    #[test]
    fn journalling_the_pre_image_collapses_partial_rollback_to_total() {
        // Without EMC, retiring a genesis dimension rolls back Partial.
        let mut plain = dag();
        plain.commit("v2", "v1", Edit::RetireDim { name: "legacy".into() }, "bonnie", 200).unwrap();
        assert!(matches!(plain.rollback("v2").unwrap(), Rollback::Partial { .. }),
                "the baseline: nothing recorded what was lost");

        // Through EMC, μ journals what it erased — so μ⁻¹ is a lookup.
        let mut d = dag();
        let emc = Emc::reshape("legacy", "moisture", DimType::Any, json!({"level": 0})).unwrap();
        emc.run(&mut d, &instances(), ids(), "bonnie", 200).unwrap();

        let r = d.rollback("v3").unwrap();
        assert!(r.is_total(), "a μ that journals its pre-image is losslessly reversible: {r:?}");
        assert!(r.delta().is_empty());
    }

    #[test]
    fn the_pre_image_carries_the_real_erased_value() {
        let mu = Mu::new().copying("legacy.level", "moisture.level").dropping("legacy");
        let pre = mu.pre_image_of(&json!({"legacy": {"level": 5}}));
        assert_eq!(pre.dimension_defaults.get("legacy"), Some(&json!({"level": 5})),
                   "a lookup, not an inference");
    }

    #[test]
    fn an_emc_cannot_be_run_without_being_planned() {
        // `Emc { .. }` does not compile outside this module: the fields are
        // private and `plan` is the only constructor — so the weakening check
        // and the compatibility report cannot be skipped on the way to `run`.
        let emc = Emc::reshape("legacy", "moisture", DimType::Any, json!(0)).unwrap();
        assert_eq!(emc.expand().kind(), "AddDim");
        assert_eq!(emc.contract().kind(), "RetireDim");
    }
}
