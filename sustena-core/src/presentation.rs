//! Present the frontier, not the winner (Operative §XV · OPV-29).
//!
//! > **Present the frontier, not the winner** — §XIII computes `𝒫` precisely so
//! > a person can choose within it, and a layer showing only the top-ranked
//! > option has **silently re-collapsed the vector and reinstated the failure
//! > §II prevents.** That is the broad pole's characteristic error: **a
//! > presentation bug with the consequences of an architecture bug.**
//!
//! ## ★ Collapsing is possible, and it cannot be done silently or by default
//!
//! The previous slice made `u_i` a vector and kept it one all the way to
//! `𝒫(X)`. All of that is undone by a surface that shows one option. So
//! [`present`] returns a [`Presented`] carrying **the whole frontier**, and
//! there is no method on it that hands back a single id on its own terms.
//!
//! Getting to one option requires [`Presented::collapse`], which:
//!
//! 1. takes a **declared [`CollapseRule`]** — a caller must say *why*, and
//!    "because it was first" is a rule it has to name out loud;
//! 2. returns a [`Collapsed`] that carries **`hidden`** — everything the
//!    frontier held and the collapse dropped, which is never empty when there
//!    was a genuine choice;
//! 3. **refuses [`CollapseRule::SolePoint`]** when the frontier holds more than
//!    one option, so the honest "there was only one anyway" cannot be borrowed
//!    to justify a real deletion.
//!
//! ★ [`Presented::would_reinstate_the_collapse`] is the diagnostic §XV is
//! actually about: it is true exactly when showing one option would delete a
//! non-dominated alternative — the §II failure, arriving through the
//! presentation layer. The conformance case runs it against the same
//! non-convex fixture 1001 weight vectors already deleted, and shows the
//! surface deleting the same option the scalarisation did.
//!
//! ## Orchie's three duties (§XV), recorded — and what this build discharges
//!
//! > **Route** — sparsely, because attention is metered. **Hold the whole** —
//! > the integrating view no councillor has, including the domain reading and
//! > §VIII's criticality signal. **Present the frontier, not the winner.**
//!
//! [`Duty::ALL`] records them, and [`discharged_by_this_build`] says plainly
//! which are real here. It reports what is **not** built rather than implying
//! coverage: `Route`'s gate `g(x,s)` is OPV-28 and is not here, and
//! *hold the whole* is partial — the domain reading exists (the Cynefin axis
//! shipped with `ω`), the criticality signal does not (OPV-14). Neither is
//! stubbed; a declared-but-inert field reads as built and is worse than an
//! absence.
//!
//! §XV also restates the trap OPV-16 recorded and the previous slice honoured:
//! *"the existing test and `match` are different axes: subject matter versus
//! Cynefin regime. Conflating them, including by reusing the key, would make
//! one unexpressible."* That is already structural — the axis is
//! [`crate::operative::Cynefin`], and a subject-matter tag list can still be
//! added under its own name.
//!
//! ## Honest limits
//!
//! - **Presentation, not ordering.** Nothing here ranks the frontier. An order
//!   over non-dominated options is a preference about preferences, and
//!   inventing one would be the collapse wearing a different hat.
//! - **One utility per presentation.** A frontier under `u_i` is that
//!   operative's; the union across a population is
//!   [`crate::operative::Ranking::on_some_frontier`], which is what a joint
//!   surface should show. This module presents whichever set it is given and
//!   does not decide that question.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::operative::{Alternative, OperativeError, Ranking, Utility};

// ---------------------------------------------------------------------------
// Orchie's three duties
// ---------------------------------------------------------------------------

/// §XV's three duties of the orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Duty {
    /// Sparsely, because attention is metered. The gate `g(x,s)` is OPV-28.
    Route,
    /// The integrating view no councillor has — the domain reading and the
    /// criticality signal.
    HoldTheWhole,
    /// §XIII computes `𝒫` so a person can choose within it.
    PresentTheFrontier,
}

impl Duty {
    pub const ALL: [Duty; 3] = [Duty::Route, Duty::HoldTheWhole, Duty::PresentTheFrontier];

    pub fn name(&self) -> &'static str {
        match self {
            Duty::Route => "route",
            Duty::HoldTheWhole => "hold the whole",
            Duty::PresentTheFrontier => "present the frontier, not the winner",
        }
    }
}

/// Whether a duty is discharged here, and by what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discharge {
    /// Built in this crate.
    Built,
    /// Genuinely partial, with the missing half named.
    Partial,
    /// Not built. The row that would build it is named.
    NotBuilt,
}

/// ★ A status read, deliberately reporting what is **not** built.
///
/// This is not a capability table anything dispatches on — it exists so the
/// three duties can be stated together without any of them being implied by a
/// stub that does nothing.
pub fn discharged_by_this_build() -> [(Duty, Discharge, &'static str); 3] {
    [
        (
            Duty::Route,
            Discharge::NotBuilt,
            "the sparse MoE gate g(x,s) = top-k(softmax(α·relevance + η·Δû + ζ·match)) is OPV-28",
        ),
        (
            Duty::HoldTheWhole,
            Discharge::Partial,
            "the domain reading exists (Cynefin, shipped with ω); §VIII's criticality signal is OPV-14",
        ),
        (
            Duty::PresentTheFrontier,
            Discharge::Built,
            "present() returns 𝒫(X); collapsing needs a declared rule and carries what it hid",
        ),
    ]
}

// ---------------------------------------------------------------------------
// The presentation
// ---------------------------------------------------------------------------

/// Why a frontier was collapsed to one option.
///
/// A caller must **name** the reason. There is no default and no anonymous
/// variant, because *"it was first"* is a rule and should have to be said out
/// loud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollapseRule {
    /// The frontier held exactly one option, so nothing is being deleted.
    ///
    /// ★ **Refused when the frontier holds more than one.** The one honest
    /// reason must not be borrowable to justify a real deletion.
    SolePoint,
    /// A person picked this one. The only reason that resolves a genuine
    /// trade-off without a machine deciding it — §XV's whole point is that this
    /// choice is theirs.
    HumanChose { chose: String },
    /// A declared, named policy. The string is required so the collapse is
    /// auditable rather than anonymous.
    DeclaredPolicy { named: String, chose: String },
}

impl CollapseRule {
    pub fn describes(&self) -> &str {
        match self {
            CollapseRule::SolePoint => "the frontier held one option",
            CollapseRule::HumanChose { .. } => "a person chose",
            CollapseRule::DeclaredPolicy { named, .. } => named,
        }
    }
}

/// One option collapsed out of a frontier, **with what was hidden**.
///
/// `hidden` is not optional and not a courtesy: it is the record of what a
/// surface stopped showing, and it is non-empty whenever there was a genuine
/// choice.
#[derive(Debug, Clone, PartialEq)]
pub struct Collapsed {
    pub chosen: String,
    /// ★ Every non-dominated option this collapse dropped.
    pub hidden: Vec<String>,
    pub rule: CollapseRule,
}

impl Collapsed {
    /// Did this collapse actually delete a genuine alternative?
    pub fn deleted_an_option(&self) -> bool {
        !self.hidden.is_empty()
    }

    pub fn describe(&self) -> String {
        if self.hidden.is_empty() {
            return format!("{} — nothing else was on the frontier", self.chosen);
        }
        format!(
            "{} — {} ({} other non-dominated option(s) not shown: {})",
            self.chosen,
            self.rule.describes(),
            self.hidden.len(),
            self.hidden.join(", ")
        )
    }
}

/// `𝒫(X)` on its way to a person.
///
/// Carries the whole frontier. There is no method that returns a single id on
/// its own terms — [`Presented::collapse`] is the only route to one, and it
/// demands a declared rule and reports what it hid.
#[derive(Debug, Clone, PartialEq)]
pub struct Presented {
    frontier: Vec<String>,
    dominated: Vec<String>,
}

impl Presented {
    /// The non-dominated set — **the thing to show**.
    pub fn frontier(&self) -> &[String] {
        &self.frontier
    }

    /// What was genuinely dominated, and is therefore *not* a choice being
    /// withheld. Kept distinct from [`Collapsed::hidden`]: dropping a dominated
    /// option deletes nothing, dropping a frontier option deletes a trade-off.
    pub fn dominated(&self) -> &[String] {
        &self.dominated
    }

    /// How many real choices a person has here.
    pub fn choices(&self) -> usize {
        self.frontier.len()
    }

    /// ★ **The §XV diagnostic.**
    ///
    /// True exactly when showing one option would delete a non-dominated
    /// alternative — the §II failure arriving through the presentation layer.
    /// *A presentation bug with the consequences of an architecture bug.*
    pub fn would_reinstate_the_collapse(&self) -> bool {
        self.frontier.len() > 1
    }

    /// Reduce to one option — **a declared exception, never the default**.
    ///
    /// Refuses [`CollapseRule::SolePoint`] on a frontier of more than one, and
    /// refuses a choice that is not on the frontier at all (picking a dominated
    /// option is not resolving a trade-off, it is losing one).
    pub fn collapse(&self, rule: CollapseRule) -> Result<Collapsed, PresentationError> {
        let chosen = match &rule {
            CollapseRule::SolePoint => {
                if self.frontier.len() != 1 {
                    return Err(PresentationError::NotASolePoint {
                        frontier: self.frontier.clone(),
                    });
                }
                self.frontier[0].clone()
            }
            CollapseRule::HumanChose { chose } | CollapseRule::DeclaredPolicy { chose, .. } => {
                chose.clone()
            }
        };
        if !self.frontier.contains(&chosen) {
            return Err(PresentationError::NotOnTheFrontier {
                chosen,
                frontier: self.frontier.clone(),
            });
        }
        let hidden = self
            .frontier
            .iter()
            .filter(|f| **f != chosen)
            .cloned()
            .collect();
        Ok(Collapsed {
            chosen,
            hidden,
            rule,
        })
    }
}

/// Present a set of alternatives under one operative's utility.
///
/// Refuses an empty option set: a presentation of nothing is not a frontier,
/// and returning an empty one would read as *"there is no good choice"* rather
/// than *"you gave me no choices"*.
pub fn present(
    utility: &Utility,
    alternatives: &[Alternative],
) -> Result<Presented, PresentationError> {
    if alternatives.is_empty() {
        return Err(PresentationError::NothingToPresent);
    }
    let frontier = utility.frontier(alternatives)?;
    let on: BTreeSet<&str> = frontier.iter().map(|s| s.as_str()).collect();
    let dominated = alternatives
        .iter()
        .map(|a| a.id.clone())
        .filter(|id| !on.contains(id.as_str()))
        .collect();
    Ok(Presented {
        frontier,
        dominated,
    })
}

/// Present the **joint** surface for a population — every option on *some*
/// operative's frontier.
///
/// §XV's *hold the whole*: the integrating view no single councillor has. An
/// option only one operative finds non-dominated is still a real choice, and
/// dropping it because the others rank it lower is the same deletion at the
/// population level.
pub fn present_joint(ranking: &Ranking) -> Result<Presented, PresentationError> {
    if ranking.on_some_frontier.is_empty() && ranking.dominated_for_all.is_empty() {
        return Err(PresentationError::NothingToPresent);
    }
    Ok(Presented {
        frontier: ranking.on_some_frontier.clone(),
        dominated: ranking.dominated_for_all.clone(),
    })
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum PresentationError {
    #[error("no alternatives to present — an empty frontier would read as 'there is no good choice' rather than 'you gave me no choices'")]
    NothingToPresent,
    #[error(
        "SolePoint declared over a frontier of {} options ({frontier:?}) — the one \
         honest reason to collapse must not be borrowed to justify a real deletion",
        frontier.len()
    )]
    NotASolePoint { frontier: Vec<String> },
    #[error("'{chosen}' is not on the frontier {frontier:?} — picking a dominated option is not resolving a trade-off, it is losing one")]
    NotOnTheFrontier {
        chosen: String,
        frontier: Vec<String>,
    },
    #[error(transparent)]
    Utility(#[from] OperativeError),
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operative::{Cynefin, Objective, Omega, Operative, Sense, Shared, Utility};

    /// The same non-convex fixture 1001 weight vectors deleted 'c' from.
    fn alts() -> Vec<Alternative> {
        vec![
            Alternative::new("a", &[("x", 10.0), ("y", 0.0)]),
            Alternative::new("b", &[("x", 0.0), ("y", 10.0)]),
            Alternative::new("c", &[("x", 4.0), ("y", 4.0)]),
            Alternative::new("d", &[("x", 1.0), ("y", 1.0)]),
        ]
    }

    fn both() -> Utility {
        Utility::new()
            .with(Objective::new("x", "x", Sense::Maximise))
            .unwrap()
            .with(Objective::new("y", "y", Sense::Maximise))
            .unwrap()
    }

    /// ★★ THE PROOF. The surface keeps the whole trade-off.
    #[test]
    fn presentation_returns_the_frontier_not_a_winner() {
        let p = present(&both(), &alts()).unwrap();
        assert_eq!(p.frontier(), ["a", "b", "c"].map(String::from));
        // 'd' is genuinely dominated — dropping it deletes nothing.
        assert_eq!(p.dominated(), ["d".to_string()]);
        assert_eq!(p.choices(), 3);
        // ★ And showing one option here WOULD be the §II failure.
        assert!(p.would_reinstate_the_collapse());
    }

    /// ★ The presentation layer deletes exactly what the scalarisation did.
    #[test]
    fn collapsing_deletes_the_same_option_a_weight_vector_would() {
        let p = present(&both(), &alts()).unwrap();
        let c = p
            .collapse(CollapseRule::DeclaredPolicy {
                named: "highest x".into(),
                chose: "a".into(),
            })
            .unwrap();
        assert!(c.deleted_an_option());
        // 'c' — the non-convex option — is among what a surface stopped showing.
        assert!(c.hidden.contains(&"c".to_string()));
        assert!(c.describe().contains("not shown"));
    }

    #[test]
    fn sole_point_cannot_be_borrowed_to_justify_a_real_deletion() {
        let p = present(&both(), &alts()).unwrap();
        assert!(matches!(
            p.collapse(CollapseRule::SolePoint),
            Err(PresentationError::NotASolePoint { .. })
        ));
    }

    #[test]
    fn sole_point_is_honest_when_there_genuinely_is_one() {
        let single = [Alternative::new("only", &[("x", 1.0), ("y", 1.0)])];
        let p = present(&both(), &single).unwrap();
        assert!(!p.would_reinstate_the_collapse());
        let c = p.collapse(CollapseRule::SolePoint).unwrap();
        assert_eq!(c.chosen, "only");
        assert!(!c.deleted_an_option());
        assert!(c.describe().contains("nothing else was on the frontier"));
    }

    #[test]
    fn a_person_choosing_within_the_frontier_is_the_one_reason_that_resolves_it() {
        let p = present(&both(), &alts()).unwrap();
        let c = p
            .collapse(CollapseRule::HumanChose { chose: "c".into() })
            .unwrap();
        assert_eq!(c.chosen, "c");
        assert_eq!(c.hidden, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn choosing_a_dominated_option_is_refused() {
        let p = present(&both(), &alts()).unwrap();
        assert!(matches!(
            p.collapse(CollapseRule::HumanChose { chose: "d".into() }),
            Err(PresentationError::NotOnTheFrontier { .. })
        ));
    }

    #[test]
    fn presenting_nothing_is_refused_rather_than_returning_an_empty_frontier() {
        assert!(matches!(
            present(&both(), &[]),
            Err(PresentationError::NothingToPresent)
        ));
    }

    /// ★ *Hold the whole* — an option only one operative likes is still a
    /// choice.
    #[test]
    fn the_joint_surface_keeps_an_option_only_one_operative_finds_non_dominated() {
        let world = Shared::new().with_dimension("x").with_dimension("y");
        let xs = Operative::new(
            "x_only",
            Utility::new()
                .with(Objective::new("x", "x", Sense::Maximise))
                .unwrap(),
            &[Cynefin::Clear],
        )
        .unwrap();
        let ys = Operative::new(
            "y_only",
            Utility::new()
                .with(Objective::new("y", "y", Sense::Maximise))
                .unwrap(),
            &[Cynefin::Clear],
        )
        .unwrap();
        let omega = Omega::over(world).with(xs).unwrap().with(ys).unwrap();
        let ranking = omega.rank(&alts()).unwrap();
        let p = present_joint(&ranking).unwrap();
        // 'a' tops x, 'b' tops y — neither is dropped for the other.
        assert!(p.frontier().contains(&"a".to_string()));
        assert!(p.frontier().contains(&"b".to_string()));
        assert!(p.would_reinstate_the_collapse());
    }

    #[test]
    fn the_three_duties_are_recorded_with_what_is_not_built() {
        let d = discharged_by_this_build();
        assert_eq!(d.len(), Duty::ALL.len());
        let route = d.iter().find(|(x, _, _)| *x == Duty::Route).unwrap();
        assert_eq!(route.1, Discharge::NotBuilt);
        assert!(route.2.contains("OPV-28"));
        let whole = d.iter().find(|(x, _, _)| *x == Duty::HoldTheWhole).unwrap();
        assert_eq!(whole.1, Discharge::Partial);
        assert!(whole.2.contains("OPV-14"));
        let front = d
            .iter()
            .find(|(x, _, _)| *x == Duty::PresentTheFrontier)
            .unwrap();
        assert_eq!(front.1, Discharge::Built);
    }
}
