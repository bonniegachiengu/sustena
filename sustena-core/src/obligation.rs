//! The author-time obligation `g_o ⟹ wp(e_o, Q_o)`
//! (Operator §III · OP-3).
//!
//! ```text
//! {g_o} e_o {Q_o}          Hoare (1969)
//! g_o ⟹ wp(e_o, Q_o)      Dijkstra (1976)
//! ```
//!
//! > **Declaring a postcondition does not make it true.** If the guard is
//! > weaker than `wp`, the Enzyme can legally fire and land wrong; the
//! > postcondition then catches it **after the fact** rather than preventing
//! > it. Both are useful, and they are not the same thing.
//!
//! ## ★★ This FLAGS. It does not block.
//!
//! > The honest position for Sustena today is the dynamic one, with the static
//! > obligation named as the thing to want. And there is a straightforward
//! > test that costs nothing: an Enzyme that declares a postcondition its guard
//! > cannot guarantee should at least be **flagged**, not silently trusted.
//!
//! So this is an **author-time diagnostic**, not a gate term. Nothing here is
//! called from [`crate::operator::execute`], and the per-call dynamic check on
//! `post_constraints` is untouched and still does all the runtime work. A test
//! asserts both halves directly: a proven-unsound operator is **flagged** by
//! the audit **and still runs**, with the dynamic check catching it exactly as
//! before. Turning the flag into a refusal would be a stronger posture than
//! §III takes, and taking it silently would be the wrong way to take it.
//!
//! ## Three honest outcomes, and the third is the point
//!
//! - [`Soundness::Sound`] — `g ⟹ wp(e, Q)` is provable in the fragment.
//! - [`Soundness::Unsound`] — provable that the guard **cannot** guarantee `Q`.
//!   This is the flag.
//! - [`Soundness::Unavailable`] — the effect or the predicate is outside the
//!   decidable fragment. **Not a pass and not a flag.**
//!
//! The third exists because both other readings of *"we cannot tell"* are
//! wrong. Silently passing an unsummarisable effect is exactly the *"silently
//! trusted"* this row exists to prevent; flagging one would report a defect
//! that has not been shown. An unknown effect is not a proven-wrong one.
//!
//! ## ★ What running it over the real registry actually found
//!
//! **Every operator this core ships is `Unavailable`** — and that is a finding
//! rather than a shortfall. `wp` is computable only where a change keeps the
//! result inside the predicate fragment: a [`Change::SetTo`] constant-folds
//! and a [`Change::ShiftBy`] moves the literal. Sustena's real operators are
//! **parameter-driven** — the amount comes from the call, not from the
//! declaration — and the fragment names that case `Opaque` itself.
//!
//! So the static obligation's reach over today's registry is zero operators
//! proven either way, which is precisely why §III says *"the honest position
//! for Sustena today is the dynamic one."* The check does not contradict that
//! sentence; **running it is what demonstrates it.** A test asserts the
//! finding, so if a future operator becomes summarisable the assertion fails
//! and someone has to look.
//!
//! ## Reused, not reinvented
//!
//! [`wp`] and [`entails`] are [`crate::compose`]'s, unchanged. This module adds
//! the single-Enzyme obligation on top of them and builds no prover of its own:
//! the decidable fragment is the same one, with the same honest `Undecided`.
//!
//! ## Honest limits
//!
//! - **The fragment is the fragment.** A guard or postcondition outside
//!   path-versus-literal comparisons is `Unavailable`, not wrong. Widening it
//!   is an SMT-shaped decision `compose.rs` already declined for a stated
//!   reason, and this row does not reopen it.
//! - **Partial correctness only**, as Hoare's triple is: this says nothing
//!   about an Enzyme that raises or hangs. §III assigns that to the engine
//!   catching exceptions and persisting nothing, which it does.
//! - **A summary is a declaration, and can be wrong.** `EffectSummary` says
//!   what the author claims the body does. Checking the summary against the
//!   body is a different, larger row — so a wrong summary yields a wrong
//!   verdict, which is why the summary is optional and absent by default
//!   rather than guessed at.

use thiserror::Error;

use crate::compose::{entails, wp, Entailment, WpResult};
use crate::operator::{OperatorMeta, Registry};

/// What the obligation check could establish about one postcondition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Soundness {
    /// `g ⟹ wp(e, Q)` — the guard is strong enough.
    Sound { postcondition: String },
    /// ★ **The flag.** The guard provably cannot guarantee this
    /// postcondition, so the Enzyme can fire legally and land wrong.
    Unsound {
        postcondition: String,
        detail: String,
    },
    /// Outside the decidable fragment. **Neither a pass nor a flag** — the
    /// static claim is unavailable, and the dynamic check is doing the work.
    Unavailable {
        postcondition: String,
        reason: String,
    },
}

impl Soundness {
    pub fn postcondition(&self) -> &str {
        match self {
            Soundness::Sound { postcondition }
            | Soundness::Unsound { postcondition, .. }
            | Soundness::Unavailable { postcondition, .. } => postcondition,
        }
    }

    pub fn is_flagged(&self) -> bool {
        matches!(self, Soundness::Unsound { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Soundness::Sound { postcondition } => {
                format!("sound: the guard guarantees `{postcondition}`")
            }
            Soundness::Unsound {
                postcondition,
                detail,
            } => format!(
                "FLAGGED: `{postcondition}` is declared but the guard cannot guarantee it — \
                 {detail}. The Enzyme can fire legally and land wrong; the dynamic check will \
                 catch it after the fact rather than preventing it"
            ),
            Soundness::Unavailable {
                postcondition,
                reason,
            } => format!(
                "unavailable: `{postcondition}` is outside the decidable fragment ({reason}) — \
                 not a pass and not a flag; the dynamic check stands"
            ),
        }
    }
}

/// Every finding for one Enzyme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObligationReport {
    pub operator: String,
    pub findings: Vec<Soundness>,
}

impl ObligationReport {
    /// The postconditions a guard provably cannot guarantee.
    pub fn flagged(&self) -> Vec<&Soundness> {
        self.findings.iter().filter(|f| f.is_flagged()).collect()
    }

    pub fn is_flagged(&self) -> bool {
        self.findings.iter().any(|f| f.is_flagged())
    }

    /// How many postconditions the static check could say nothing about.
    pub fn unavailable(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| matches!(f, Soundness::Unavailable { .. }))
            .count()
    }

    pub fn describe(&self) -> String {
        if self.findings.is_empty() {
            return format!("{}: declares no postcondition", self.operator);
        }
        format!(
            "{}: {}",
            self.operator,
            self.findings
                .iter()
                .map(|f| f.describe())
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

/// Check `g_o ⟹ wp(e_o, Q_o)` for one Enzyme, at author time.
///
/// An operator declaring **no** postcondition has nothing to be unsound about
/// and reports no findings — vacuously fine, and said rather than implied.
pub fn check_obligation(meta: &OperatorMeta) -> ObligationReport {
    let mut findings = Vec::new();

    for q in &meta.post_constraints {
        let Some(effect) = meta.effect.as_ref() else {
            findings.push(Soundness::Unavailable {
                postcondition: q.clone(),
                reason: "the operator declares no EffectSummary, so wp has nothing to pull back \
                         through — absent, not assumed harmless"
                    .into(),
            });
            continue;
        };

        // wp(e, Q) — the weakest precondition of the postcondition.
        let pulled = match wp(effect, q) {
            Ok(r) => r,
            Err(why) => {
                findings.push(Soundness::Unavailable {
                    postcondition: q.clone(),
                    reason: format!("wp is not computable here: {why}"),
                });
                continue;
            }
        };

        match pulled {
            // Q holds after the effect whatever the pre-state was.
            WpResult::Always => findings.push(Soundness::Sound {
                postcondition: q.clone(),
            }),
            // ★ No pre-state establishes Q. Declaring it is declaring
            //   something the effect makes impossible.
            WpResult::Never => findings.push(Soundness::Unsound {
                postcondition: q.clone(),
                detail: "the effect makes it unsatisfiable for EVERY pre-state, so no guard \
                         could be strong enough"
                    .into(),
            }),
            WpResult::Condition(required) => {
                // g ⟹ required, in the same fragment.
                match entails(&meta.constraints, std::slice::from_ref(&required)) {
                    Entailment::Entailed => findings.push(Soundness::Sound {
                        postcondition: q.clone(),
                    }),
                    // g ∧ required is unsatisfiable: whenever the guard holds,
                    // wp fails. Guaranteed to land wrong.
                    Entailment::Refuted { detail } => findings.push(Soundness::Unsound {
                        postcondition: q.clone(),
                        detail: format!(
                            "the guard requires the opposite of wp(e,Q) = `{required}` ({detail})"
                        ),
                    }),
                    Entailment::Undecided { reason } => findings.push(Soundness::Unavailable {
                        postcondition: q.clone(),
                        reason: format!("`g ⟹ {required}` is outside the fragment: {reason}"),
                    }),
                }
            }
        }
    }

    ObligationReport {
        operator: meta.name.to_string(),
        findings,
    }
}

/// Audit a whole registry at author time.
///
/// Returns a report per operator, including the ones with nothing to say — an
/// audit that silently omitted the operators it could not check would be the
/// same silent trust this row exists to replace.
pub fn audit(registry: &Registry) -> Vec<ObligationReport> {
    registry
        .names()
        .iter()
        .filter_map(|n| registry.get(n))
        .map(check_obligation)
        .collect()
}

/// Everything the audit flagged, across a registry.
pub fn flagged(registry: &Registry) -> Vec<ObligationReport> {
    audit(registry).into_iter().filter(|r| r.is_flagged()).collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ObligationError {
    #[error("operator '{0}' is not in the registry")]
    UnknownOperator(String),
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::{Change, EffectSummary};
    use crate::operator::meta::Protocol;
    use crate::operator::OperatorResult;
    use serde_json::json;

    /// A real registered operator's shape, with its effect summarised.
    fn op(post: &[&str], guard: &[&str], effect: Option<EffectSummary>) -> OperatorMeta {
        OperatorMeta {
            name: "test.force_negative",
            description: "Force a pocket negative. Exists to prove the gate refuses it.",
            params: vec![],
            constraints: guard.iter().map(|s| s.to_string()).collect(),
            post_constraints: post.iter().map(|s| s.to_string()).collect(),
            side_effects: vec![],
            pawa_cost: 0,
            protocol: Protocol::Rpc,
            min_privilege: 0,
            effect,
            run: |_s, _p, _e, _m| OperatorResult::ok(json!({})),
        }
    }

    /// `test.force_negative` sets `…food.allocated` to -999 — a constant, so
    /// the fragment can summarise it.
    fn forces_negative() -> EffectSummary {
        EffectSummary::new().with(
            "finances.pockets.food.allocated",
            Change::SetTo(json!(-999.0)),
        )
    }

    // -- ★★ the worked proof -------------------------------------------------

    /// ★★ THE FLAG. A real operator declaring a postcondition its effect makes
    /// impossible.
    #[test]
    fn an_operator_whose_effect_defeats_its_own_postcondition_is_flagged() {
        let meta = op(
            &["finances.pockets.food.allocated >= 0"],
            &[],
            Some(forces_negative()),
        );
        let report = check_obligation(&meta);
        assert!(report.is_flagged());
        assert_eq!(report.flagged().len(), 1);
        let d = report.flagged()[0].describe();
        assert!(d.contains("FLAGGED"));
        assert!(d.contains("catch it after the fact"));
    }

    /// ★ The same operator, a postcondition its effect DOES guarantee.
    #[test]
    fn the_same_operator_with_an_achievable_postcondition_is_sound() {
        let meta = op(
            &["finances.pockets.food.allocated <= 0"],
            &[],
            Some(forces_negative()),
        );
        let report = check_obligation(&meta);
        assert!(!report.is_flagged());
        assert!(matches!(report.findings[0], Soundness::Sound { .. }));
    }

    /// ★ Unavailable is neither — and it is the honest majority case.
    #[test]
    fn an_unsummarised_effect_is_unavailable_not_trusted_and_not_flagged() {
        let meta = op(&["finances.pockets.food.allocated >= 0"], &[], None);
        let report = check_obligation(&meta);
        assert!(!report.is_flagged(), "an unknown effect is not a proven-wrong one");
        assert_eq!(report.unavailable(), 1);
        assert!(report.findings[0].describe().contains("not a pass and not a flag"));
    }

    #[test]
    fn a_param_driven_change_is_opaque_and_therefore_unavailable() {
        let by_param = EffectSummary::new()
            .with("finances.liquid.balance", Change::Opaque);
        let meta = op(&["finances.liquid.balance >= 0"], &[], Some(by_param));
        let report = check_obligation(&meta);
        assert!(!report.is_flagged());
        assert_eq!(report.unavailable(), 1);
    }

    #[test]
    fn an_operator_with_no_postcondition_has_nothing_to_be_unsound_about() {
        let meta = op(&[], &["params.amount > 0"], None);
        let report = check_obligation(&meta);
        assert!(report.findings.is_empty());
        assert!(!report.is_flagged());
        assert!(report.describe().contains("declares no postcondition"));
    }

    // -- ★ the real registry -------------------------------------------------

    /// ★ The finding: every shipped operator is `Unavailable`, because their
    /// effects are parameter-driven. If one becomes summarisable this fails
    /// and someone has to look — which is the point of asserting it.
    #[test]
    fn every_operator_this_core_ships_is_currently_unavailable() {
        let reg = Registry::default();
        let reports = audit(&reg);
        assert!(!reports.is_empty());
        assert!(
            flagged(&reg).is_empty(),
            "nothing in the shipped registry is proven unsound"
        );
        for r in &reports {
            assert_eq!(
                r.findings.len(),
                r.unavailable(),
                "{} has a finding that is not Unavailable — the registry has changed",
                r.operator
            );
        }
    }
}
