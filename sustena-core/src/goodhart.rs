//! The Goodhart guard — an invariant over the unwatched set, enforced by the
//! gate (Operative §XIV · OPV-27).
//!
//! > Goodhart (1975), in Strathern's formulation: *when a measure becomes a
//! > target, it ceases to be a good measure.* With `O = ⋃_i supp(u_i)` and
//! > `U = dim(S) \ O`, the defining property of `U` is the dangerous one: **no
//! > agent's score changes when `U` moves.** Damage there is not merely
//! > unpunished; it is **unobserved by every scoring function in the system.**
//!
//! ## ★ Why this module is not in the agent layer, and the file layout says so
//!
//! > The fix is deliberately **not** "add an operative for `U`" — that converts
//! > `U` into `O` and creates a new, smaller `U` one step later. [...] The fix
//! > **moves the check out of the agent layer**: an invariant in §4D's `V` over
//! > the dimensions in `U`, enforced by §4E's gate.
//!
//! So this is not a module about operatives that happens to be careful. A
//! [`GoodhartGuard`] is a [`ConstraintDecl`] — the same declared rule every
//! other invariant is — and it is enforced by [`crate::admission::admit_one`],
//! the gate that already exists. It carries **no utility, no vote, no score
//! and no rank**: there is nothing on the type that could act as another
//! scoring voice, which is the distinction §XIV turns on.
//!
//! This is the same move LAW made about clamping. A household holding its
//! headline number by drawing on something nobody measures has a good blood
//! test and a weakening skeleton.
//!
//! ## ★ A guard that watches nothing unwatched is refused
//!
//! [`GoodhartGuard::declare`] checks that the constraint's expression genuinely
//! references at least one dimension in `U`. A "guard" over dimensions some
//! operative already scores is not a guard — it is a redundant voice for
//! something already watched, and calling it a Goodhart guard would be the one
//! kind of false assurance this module exists to prevent. Refused at authoring
//! time, where it is a mistake, rather than at 2 a.m., where it is a gap.
//!
//! ## The article's own argument about coverage, made checkable
//!
//! [`coverage`] reports `O` and `U` and nothing else — it is a read, not a
//! remedy. What it makes observable is §XIV's reason for refusing the obvious
//! fix: adding an operative for a dimension genuinely moves it from `U` to `O`,
//! and `U` shrinks — *"a benefit and not a solution"* — while declaring one new
//! dimension of `S` puts a fresh one back. Both halves are exercised, so the
//! conclusion is demonstrated rather than quoted.
//!
//! ## Honest limits
//!
//! - **`U` is computed against the dimensions the world declares.** A quantity
//!   nothing in `dim(S)` names is invisible to this too, and no coverage
//!   report can find what the model does not contain.
//! - **Root-name granularity.** A support is a dimension name and a predicate
//!   path's first segment is what is compared, so an operative reading
//!   `finances.liquid` is treated as watching `finances`. Coarse in the safe
//!   direction — it can call something watched that is only partly watched, so
//!   the guard it would refuse is one it should have allowed. Refusing a real
//!   guard is visible; silently accepting a fake one would not be.
//! - **The guard does not choose the strategy.** `refuse`, `clamp` or `defer`
//!   is the declaration's own business, as it is for every other constraint.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::admission::{admit_one, typecheck_constraint, ConstraintDecl, DeclError, Verdict};
use crate::operative::Omega;
use crate::predicate::{Operand, Predicate};

/// `O` and `U`, and nothing more.
///
/// A read, not a remedy. §XIV is explicit that the fix for an unwatched
/// dimension is not another operative, so this type has no method that adds
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    /// `O = ⋃_i supp(u_i)` — every dimension some operative's score reads.
    pub observed: BTreeSet<String>,
    /// ★ `U = dim(S) \ O` — the dimensions **no** agent's score changes for.
    pub unwatched: BTreeSet<String>,
}

impl Coverage {
    /// Is anything unwatched at all?
    pub fn has_blind_spot(&self) -> bool {
        !self.unwatched.is_empty()
    }

    /// How much of `dim(S)` any operative reads. Reported for legibility; it
    /// is not a target, and treating it as one would be Goodhart applied to
    /// the Goodhart guard.
    pub fn observed_fraction(&self) -> f64 {
        let total = self.observed.len() + self.unwatched.len();
        if total == 0 {
            return 0.0;
        }
        self.observed.len() as f64 / total as f64
    }

    pub fn describe(&self) -> String {
        if self.unwatched.is_empty() {
            return "every declared dimension is read by some operative's score".to_string();
        }
        format!(
            "unwatched: {} — no agent's score changes when these move",
            self.unwatched.iter().cloned().collect::<Vec<_>>().join(", ")
        )
    }
}

/// `O = ⋃_i supp(u_i)` and `U = dim(S) \ O`, over a population and the world
/// it shares.
pub fn coverage(omega: &Omega) -> Coverage {
    Coverage {
        observed: omega.observed_dimensions().into_iter().map(String::from).collect(),
        unwatched: omega.unwatched_dimensions().into_iter().map(String::from).collect(),
    }
}

/// A declared invariant over dimensions in `U`, enforced by the gate.
///
/// **Not an operative.** No utility, no vote, no score, no rank — there is
/// nothing on this type that could become another scoring voice, which is
/// exactly §XIV's point. It is a [`ConstraintDecl`] with a proof attached that
/// it genuinely watches something nothing else does.
#[derive(Debug, Clone, PartialEq)]
pub struct GoodhartGuard {
    decl: ConstraintDecl,
    node: Predicate,
    /// The dimensions of `U` this guard actually covers. Non-empty by
    /// construction — see [`GoodhartGuard::declare`].
    covers: BTreeSet<String>,
}

impl GoodhartGuard {
    /// Declare a guard over the unwatched set.
    ///
    /// Refuses a constraint that references **no** dimension in `U`: watching
    /// something an operative already scores is a redundant voice, not a
    /// guard, and letting it be called one would be false assurance about the
    /// exact blind spot §XIV is about.
    pub fn declare(decl: ConstraintDecl, coverage: &Coverage) -> Result<Self, GuardError> {
        let node = typecheck_constraint(&decl)?;
        let referenced = referenced_dimensions(&node);
        let covers: BTreeSet<String> = referenced
            .intersection(&coverage.unwatched)
            .cloned()
            .collect();
        if covers.is_empty() {
            return Err(GuardError::WatchesNothingUnwatched {
                id: decl.id.clone(),
                references: referenced.into_iter().collect(),
                unwatched: coverage.unwatched.iter().cloned().collect(),
            });
        }
        Ok(Self { decl, node, covers })
    }

    pub fn id(&self) -> &str {
        &self.decl.id
    }

    /// The `U` dimensions this guard covers. Never empty.
    pub fn covers(&self) -> &BTreeSet<String> {
        &self.covers
    }

    pub fn declaration(&self) -> &ConstraintDecl {
        &self.decl
    }

    /// ★ **Enforced by the gate that already exists.**
    ///
    /// This is a thin call to [`admit_one`] on purpose: the guard is not a
    /// second enforcement path, it is an ordinary constraint standing where no
    /// score is looking. If this method did anything the gate does not, the
    /// module would have re-invented the thing §XIV says to reuse.
    pub fn check(&self, candidate: &Value, params: &Map<String, Value>) -> Verdict {
        admit_one(&self.decl, &self.node, candidate, params)
    }
}

/// The root dimension names a predicate reads.
///
/// Root-name granularity, matching how a support is declared — see the module
/// docs on why that is coarse in the safe direction.
pub fn referenced_dimensions(node: &Predicate) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk(node, &mut out);
    out
}

fn walk(node: &Predicate, out: &mut BTreeSet<String>) {
    match node {
        Predicate::Comparison { left, right, .. } | Predicate::Membership { left, right, .. } => {
            operand(left, out);
            operand(right, out);
        }
        Predicate::And(v) | Predicate::Or(v) => v.iter().for_each(|p| walk(p, out)),
        Predicate::Not(p) => walk(p, out),
        Predicate::Quantifier { list_path, body, .. } => {
            if let Some(root) = root_of(&list_path.raw) {
                out.insert(root);
            }
            walk(body, out);
        }
    }
}

fn operand(op: &Operand, out: &mut BTreeSet<String>) {
    let path = match op {
        Operand::Path(p) => p,
        Operand::Aggregate { path, .. } => path,
        // Literals and params are not dimensions of S.
        Operand::Literal(_) | Operand::Param(_) | Operand::List(_) => return,
    };
    if let Some(root) = root_of(&path.raw) {
        out.insert(root);
    }
}

fn root_of(raw: &str) -> Option<String> {
    let head = raw
        .split(['.', '['])
        .next()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())?;
    Some(head.to_string())
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum GuardError {
    #[error("guard '{id}' is not well-formed: {source_detail:?}")]
    Malformed {
        id: String,
        source_detail: DeclError,
    },
    #[error(
        "guard '{id}' references {references:?} and none of them is unwatched \
         ({unwatched:?}) — watching what an operative already scores is a \
         redundant voice, not a guard, and calling it one would be false \
         assurance about the exact blind spot §XIV is about"
    )]
    WatchesNothingUnwatched {
        id: String,
        references: Vec<String>,
        unwatched: Vec<String>,
    },
}

impl From<DeclError> for GuardError {
    fn from(e: DeclError) -> Self {
        let id = match &e {
            DeclError::Unparseable { id, .. }
            | DeclError::ClampOnNonInterval { id, .. }
            | DeclError::ClampOnConserved { id, .. } => id.clone(),
        };
        GuardError::Malformed {
            id,
            source_detail: e,
        }
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::Strategy;
    use crate::operative::{Cynefin, Objective, Operative, Sense, Shared, Utility};
    use serde_json::json;

    fn world() -> Shared {
        Shared::new()
            .with_dimension("savings")
            .with_dimension("sleep")
            .with_dimension("goodwill")
    }

    fn saver() -> Operative {
        Operative::new(
            "mentor",
            Utility::new()
                .with(Objective::new("savings", "savings", Sense::Maximise))
                .unwrap(),
            &[Cynefin::Clear],
        )
        .unwrap()
    }

    fn omega() -> Omega {
        Omega::over(world()).with(saver()).unwrap()
    }

    fn guard_decl() -> ConstraintDecl {
        ConstraintDecl {
            id: "sleep_floor".into(),
            expression: "sleep >= 6".into(),
            strategy: Strategy::Refuse,
            conserved_dimensions: vec![],
        }
    }

    #[test]
    fn coverage_reports_o_and_u() {
        let c = coverage(&omega());
        assert_eq!(c.observed, ["savings".to_string()].into_iter().collect());
        assert_eq!(
            c.unwatched,
            ["goodwill".to_string(), "sleep".to_string()].into_iter().collect()
        );
        assert!(c.has_blind_spot());
        assert!(c.describe().contains("no agent's score changes"));
    }

    /// ★★ THE PROOF. Damage to `U` is invisible to every score, and only the
    /// gate catches it.
    #[test]
    fn damage_to_an_unwatched_dimension_is_seen_by_no_operative_and_caught_only_by_the_gate() {
        let o = omega();
        let cov = coverage(&o);
        assert!(cov.unwatched.contains("sleep"));

        let before = [("savings".to_string(), 100.0), ("sleep".to_string(), 8.0)]
            .into_iter()
            .collect();
        let after = [("savings".to_string(), 100.0), ("sleep".to_string(), 3.0)]
            .into_iter()
            .collect();

        // ★ No agent's score changes. Every operative reads the two states as
        //   identical, so nothing in the agent layer could rank against this.
        for m in o.members() {
            let u_before = m.utility().at(&before).unwrap();
            let u_after = m.utility().at(&after).unwrap();
            assert_eq!(u_before, u_after, "{} should be blind to sleep", m.id());
            assert_eq!(m.utility().delta(&before, &after).unwrap(), vec![0.0]);
        }

        // ★ And the gate refuses it, via a declared invariant over U.
        let guard = GoodhartGuard::declare(guard_decl(), &cov).unwrap();
        assert_eq!(guard.covers(), &["sleep".to_string()].into_iter().collect());
        let verdict = guard.check(&json!({"savings": 100, "sleep": 3}), &Map::new());
        assert!(
            matches!(&verdict, Verdict::Refused { rule, .. } if rule == "sleep_floor"),
            "{verdict:?}"
        );
        // The same guard admits the healthy state.
        assert_eq!(
            guard.check(&json!({"savings": 100, "sleep": 8}), &Map::new()),
            Verdict::Admit
        );
    }

    #[test]
    fn a_guard_that_watches_nothing_unwatched_is_refused() {
        let cov = coverage(&omega());
        let redundant = ConstraintDecl {
            id: "savings_floor".into(),
            expression: "savings >= 0".into(),
            strategy: Strategy::Refuse,
            conserved_dimensions: vec![],
        };
        assert!(matches!(
            GoodhartGuard::declare(redundant, &cov),
            Err(GuardError::WatchesNothingUnwatched { .. })
        ));
    }

    /// ★ §XIV's reason for refusing the obvious fix, demonstrated in both
    /// directions rather than quoted.
    #[test]
    fn adding_an_operative_shrinks_u_without_emptying_it_and_a_new_dimension_refills_it() {
        let before = coverage(&omega());
        assert_eq!(before.unwatched.len(), 2);

        let rested = Operative::new(
            "curator",
            Utility::new()
                .with(Objective::new("rest", "sleep", Sense::Maximise))
                .unwrap(),
            &[Cynefin::Complex],
        )
        .unwrap();
        let after = coverage(&omega().with(rested).unwrap());
        // U → O for that dimension: it really did shrink...
        assert!(after.unwatched.len() < before.unwatched.len());
        // ...and it is not empty. A benefit, not a solution.
        assert!(after.has_blind_spot());
        assert_eq!(after.unwatched, ["goodwill".to_string()].into_iter().collect());

        // One step later: S gains a dimension, and a fresh U appears.
        let grown = Omega::over(world().with_dimension("reputation"))
            .with(saver())
            .unwrap();
        assert!(coverage(&grown).unwatched.contains("reputation"));
    }

    #[test]
    fn referenced_dimensions_walks_the_whole_predicate() {
        let node = typecheck_constraint(&ConstraintDecl {
            id: "compound".into(),
            expression: "sleep >= 6 AND goodwill >= 1".into(),
            strategy: Strategy::Refuse,
            conserved_dimensions: vec![],
        })
        .unwrap();
        assert_eq!(
            referenced_dimensions(&node),
            ["goodwill".to_string(), "sleep".to_string()].into_iter().collect()
        );
    }

    #[test]
    fn a_malformed_guard_is_refused_at_authoring_time() {
        let cov = coverage(&omega());
        let bad = ConstraintDecl {
            id: "nonsense".into(),
            expression: ">>> not a predicate".into(),
            strategy: Strategy::Refuse,
            conserved_dimensions: vec![],
        };
        assert!(matches!(
            GoodhartGuard::declare(bad, &cov),
            Err(GuardError::Malformed { .. })
        ));
    }

    #[test]
    fn the_observed_fraction_is_a_reading_not_a_target() {
        let c = coverage(&omega());
        assert!((c.observed_fraction() - 1.0 / 3.0).abs() < 1e-12);
    }
}
