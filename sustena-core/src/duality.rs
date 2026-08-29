//! **One duality, three scales — and depth adds conjuncts, never power**
//! (Operative · §VI).
//!
//! ```text
//!   ℓ0   inside one operative      propose ↔ dispose
//!   ℓ1   operative ↔ sub-operatives
//!   ℓ2   Orchie ↔ Council
//! ```
//!
//! ★★★ **Two quantifiers, and both hold at once.** (A) says every agent, at
//! whatever scale, holds *both* halves — it proposes and it judges. (B) says the
//! same duality recurs at each of the three scales. The row's warning is about
//! how they interact: **reading (B) as licence to violate (A) rebuilds the
//! half-mind council.**
//!
//! ★★★ **That failure is seductive because it sounds like architecture.** "The
//! duality lives at the Orchie↔Council scale, so an individual councillor need
//! only propose" is a clean-sounding sentence, and what it builds is a room full
//! of agents that can suggest and cannot judge, with one thing at the top doing
//! all the judging. That is not a council; it is a single mind with helpers, and
//! every claim about independent judgment made anywhere else in this system
//! quietly stops being true. [`Violation::HalfMindExcusedByScale`] exists so the
//! excuse has a name.
//!
//! ## A sub-operative is `⊕`, not a new primitive
//!
//! ★★★ `Σ_ω = (⊕_j Σ_ω'_j) ⊕ Σ_ω^own` — a sub-operative is a `Σ` and the
//! composition is the one that already exists ([`crate::sigma`]). Nothing here
//! introduces a `SubOperative` type, because introducing one would be the claim
//! that depth is a different kind of thing, and it is not.
//!
//! ★★★ **Corollary 2 — depth adds conjuncts and cost, never power.** Admittance
//! along the path is a conjunction, so a deeper nesting can only refuse *more*.
//! There is no depth at which something becomes permitted that a shallower path
//! would have refused, and [`depth_never_adds_power`] checks it on a real path
//! rather than asserting it in prose.

use std::collections::BTreeSet;

use crate::admissibility::{admissible_along, Level};
use crate::region::Region;
use crate::sigma::Sigma;

/// The two halves every agent must hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Half {
    /// Generate candidates.
    Propose,
    /// Judge them.
    Dispose,
}

impl Half {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Dispose => "dispose",
        }
    }
}

/// Where in the nesting a duality is being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scale {
    /// ℓ0 — inside one operative.
    WithinAnOperative,
    /// ℓ1 — an operative and the sub-operatives it composes.
    OperativeAndSubs,
    /// ℓ2 — Orchie and the Council.
    OrchieAndCouncil,
}

impl Scale {
    pub fn name(&self) -> &'static str {
        match self {
            Self::WithinAnOperative => "within an operative",
            Self::OperativeAndSubs => "an operative and its sub-operatives",
            Self::OrchieAndCouncil => "Orchie and the Council",
        }
    }

    pub fn all() -> [Scale; 3] {
        [Self::WithinAnOperative, Self::OperativeAndSubs, Self::OrchieAndCouncil]
    }
}

/// One agent, at one scale, holding some halves.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    pub id: String,
    pub scale: Scale,
    pub halves: BTreeSet<Half>,
    /// If the agent is missing a half, the reason its author gave.
    ///
    /// ★★ Recorded rather than discarded, because the *excuse* is the diagnostic
    /// — a half-mind justified by scale is a different mistake from one nobody
    /// noticed, and they get fixed differently.
    pub excuse: Option<String>,
}

impl Agent {
    pub fn whole(id: &str, scale: Scale) -> Self {
        Self {
            id: id.into(),
            scale,
            halves: [Half::Propose, Half::Dispose].into_iter().collect(),
            excuse: None,
        }
    }

    pub fn only(id: &str, scale: Scale, half: Half) -> Self {
        Self { id: id.into(), scale, halves: [half].into_iter().collect(), excuse: None }
    }

    pub fn excused_by(mut self, reason: &str) -> Self {
        self.excuse = Some(reason.into());
        self
    }

    /// Does it hold both halves?
    pub fn is_whole(&self) -> bool {
        self.halves.contains(&Half::Propose) && self.halves.contains(&Half::Dispose)
    }

    fn missing(&self) -> Vec<Half> {
        [Half::Propose, Half::Dispose].into_iter().filter(|h| !self.halves.contains(h)).collect()
    }
}

/// A way the duality has been broken.
#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    /// (A) broken: an agent holds one half and nobody gave a reason.
    HalfMind { agent: String, missing: Vec<Half> },
    /// ★★★ **(A) broken, and excused by (B).** The specific failure §VI names:
    /// the duality is said to live at another scale, so this agent is allowed to
    /// only propose — and the result is a room of suggesters with one judge.
    HalfMindExcusedByScale { agent: String, missing: Vec<Half>, excuse: String },
    /// (B) broken: a scale that exists has no whole agent in it.
    ScaleWithoutTheDuality { scale: Scale },
}

impl Violation {
    pub fn describe(&self) -> String {
        match self {
            Self::HalfMind { agent, missing } => format!(
                "{agent} cannot {} — it holds one half of the duality",
                missing.iter().map(Half::name).collect::<Vec<_>>().join(" or ")
            ),
            Self::HalfMindExcusedByScale { agent, missing, excuse } => format!(
                "{agent} cannot {} and this was excused as \"{excuse}\" — the duality holding at \
                 another scale is not licence to break it here; that is how a council becomes one \
                 mind with helpers",
                missing.iter().map(Half::name).collect::<Vec<_>>().join(" or ")
            ),
            Self::ScaleWithoutTheDuality { scale } => {
                format!("no whole agent at the scale of {}", scale.name())
            }
        }
    }
}

/// **Check both quantifiers over a population.**
///
/// ★★ Every violation is collected. Stopping at the first would report one
/// half-mind in a council of five and leave somebody to find the others by
/// running it again.
pub fn check(agents: &[Agent]) -> Vec<Violation> {
    let mut out = Vec::new();

    // (A) — over agents.
    for a in agents {
        if a.is_whole() {
            continue;
        }
        out.push(match &a.excuse {
            Some(excuse) => Violation::HalfMindExcusedByScale {
                agent: a.id.clone(),
                missing: a.missing(),
                excuse: excuse.clone(),
            },
            None => Violation::HalfMind { agent: a.id.clone(), missing: a.missing() },
        });
    }

    // (B) — over the scales this population actually occupies.
    //
    // ★★ Only scales somebody is at. Demanding a whole agent at a scale nobody
    //    has built yet would report a gap in a system that has not claimed to
    //    have one.
    let occupied: BTreeSet<Scale> = agents.iter().map(|a| a.scale).collect();
    for scale in occupied {
        if !agents.iter().any(|a| a.scale == scale && a.is_whole()) {
            out.push(Violation::ScaleWithoutTheDuality { scale });
        }
    }
    out
}

/// **`V_ω' ⊆ V_ω`** — a sub-operative's region sits inside its parent's.
///
/// ★★★ Checked over an enumerated space rather than symbolically, for the same
/// reason [`crate::kernel`] enumerates: containment of two interval-plus-relation
/// regions is not decidable in general, and a symbolic answer that was sometimes
/// wrong would be worse than an honest sample. Returns the states that escape.
pub fn escapes_the_parent(
    child: &Region,
    parent: &Region,
    space: &[(String, serde_json::Value)],
) -> Vec<String> {
    space
        .iter()
        .filter(|(_, s)| {
            let inside_child = child.membership(s).map(|m| m.is_viable()).unwrap_or(false);
            let inside_parent = parent.membership(s).map(|m| m.is_viable()).unwrap_or(false);
            inside_child && !inside_parent
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// **Corollary 2, checked: depth adds conjuncts and cost, never power.**
///
/// ★★★ Runs the same action against the full path and against every prefix of
/// it. If a deeper path ever admits something a shallower one refused, that is
/// returned — and it would mean the composition is not a conjunction, which is
/// the only thing holding the whole holon argument up.
pub fn depth_never_adds_power(path: &[Level]) -> Result<(), String> {
    let deep = admissible_along(path).admitted();
    for cut in 1..path.len() {
        let shallow = admissible_along(&path[..cut]).admitted();
        if deep && !shallow {
            return Err(format!(
                "the full path of {} levels admitted what its first {cut} refused — composition \
                 is not a conjunction",
                path.len()
            ));
        }
    }
    Ok(())
}

/// A sub-operative, and the fact that it is nothing new.
///
/// ★★★ Takes and returns [`Sigma`]. There is no `SubOperative` type in this
/// module, because having one would be the claim that depth is a different kind
/// of thing — and `Σ_ω = (⊕_j Σ_ω'_j) ⊕ Σ_ω^own` says it is not.
pub fn compose(parent: Sigma, sub: Sigma) -> Sigma {
    parent.with_child(sub)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admissibility::BindingRule;
    use crate::editing::Definition;
    use crate::region::Interval;
    use crate::schema::{DimType, Schema};
    use serde_json::json;

    fn council() -> Vec<Agent> {
        vec![
            Agent::whole("orchie", Scale::OrchieAndCouncil),
            Agent::whole("mentor", Scale::OrchieAndCouncil),
            Agent::whole("mentor's budget sub", Scale::OperativeAndSubs),
            Agent::whole("mentor's inner loop", Scale::WithinAnOperative),
        ]
    }

    #[test]
    fn a_whole_council_at_every_occupied_scale_passes_both_quantifiers() {
        assert!(check(&council()).is_empty());
    }

    #[test]
    fn an_agent_that_can_only_suggest_is_a_half_mind() {
        let mut c = council();
        c.push(Agent::only("attache", Scale::OrchieAndCouncil, Half::Propose));
        let v = check(&c);
        assert_eq!(v.len(), 1);
        assert!(v[0].describe().contains("cannot dispose"));
    }

    #[test]
    fn the_excuse_that_rebuilds_the_half_mind_council_has_its_own_name() {
        // ★★★ The failure §VI warns about, and it is seductive because it sounds
        //     like architecture: "the duality lives at the Orchie↔Council scale,
        //     so a councillor need only propose". What it builds is a room of
        //     suggesters with one judge.
        let mut c = council();
        c.push(
            Agent::only("attache", Scale::OrchieAndCouncil, Half::Propose)
                .excused_by("the duality lives at the Orchie↔Council scale"),
        );
        let v = check(&c);
        match &v[0] {
            Violation::HalfMindExcusedByScale { excuse, .. } => {
                assert!(excuse.contains("Orchie"));
            }
            other => panic!("the excuse must be named: {other:?}"),
        }
        assert!(v[0].describe().contains("one \nmind with helpers")
            || v[0].describe().contains("one mind with helpers"));
    }

    #[test]
    fn a_half_mind_nobody_noticed_and_one_that_was_argued_for_are_different_findings() {
        // ★★ They get fixed differently: one is an oversight, the other is a
        //    belief somebody holds and will re-introduce.
        let unnoticed = check(&[Agent::only("a", Scale::WithinAnOperative, Half::Propose)]);
        let argued = check(&[
            Agent::only("a", Scale::WithinAnOperative, Half::Propose).excused_by("by design")
        ]);
        assert!(matches!(unnoticed[0], Violation::HalfMind { .. }));
        assert!(matches!(argued[0], Violation::HalfMindExcusedByScale { .. }));
    }

    #[test]
    fn a_scale_with_nobody_whole_in_it_is_reported() {
        // ★★ (B): the duality has to recur, not merely exist somewhere.
        let v = check(&[
            Agent::whole("orchie", Scale::OrchieAndCouncil),
            Agent::only("a sub", Scale::OperativeAndSubs, Half::Propose),
        ]);
        assert!(v.iter().any(|x| matches!(
            x,
            Violation::ScaleWithoutTheDuality { scale: Scale::OperativeAndSubs }
        )));
    }

    #[test]
    fn a_scale_nobody_has_built_yet_is_not_a_gap() {
        // ★★ Demanding a whole agent at a scale nobody occupies would report a
        //    hole in a system that never claimed to have one.
        let just_one_scale = [Agent::whole("orchie", Scale::OrchieAndCouncil)];
        assert!(check(&just_one_scale).is_empty());
        assert_eq!(Scale::all().len(), 3, "and the three scales still exist as a vocabulary");
    }

    #[test]
    fn every_half_mind_is_reported_rather_than_the_first() {
        // ★★ Otherwise somebody fixes one and runs it again to find the next.
        let v = check(&[
            Agent::only("a", Scale::WithinAnOperative, Half::Propose),
            Agent::only("b", Scale::WithinAnOperative, Half::Dispose),
        ]);
        assert!(v.len() >= 2);
    }

    // ── OPV-10 ──────────────────────────────────────────────────────────────

    fn schema() -> Schema {
        Schema::new().declare("balance", DimType::Number { lo: None, hi: None })
    }

    /// ★★ `before` and `after` are both supplied. `admissible_along` refuses
    ///    only a NEWLY caused breach, so a fixture where they are equal can
    ///    never be refused — a first draft of the test below made exactly that
    ///    mistake and passed for the wrong reason until it did not.
    fn level(
        id: &str,
        rule: BindingRule,
        before: serde_json::Value,
        after: serde_json::Value,
    ) -> Level {
        Level { sustain_id: id.into(), rules: vec![rule], before, after }
    }

    #[test]
    fn a_sub_operative_is_a_sigma_and_there_is_no_new_type_for_it() {
        // ★★★ Having one would be the claim that depth is a different kind of
        //     thing, and `Σ_ω = (⊕_j Σ_ω'_j) ⊕ Σ_ω^own` says it is not.
        let parent = Sigma::new("mentor", Definition::new(schema()));
        let sub = Sigma::new("mentor's budget sub", Definition::new(schema()));
        let composed = compose(parent, sub);
        assert_eq!(composed.depth(), 2);
        assert!(composed.find("mentor's budget sub").is_some());
    }

    #[test]
    fn depth_can_only_refuse_more() {
        // ★★★ Corollary 2, checked on a real path rather than asserted. The
        //     conjunction is the only thing holding the holon argument up.
        let ok = json!({ "balance": 500.0 });
        let path = vec![
            level("the village", BindingRule::binding("v", "balance >= 0"), ok.clone(), ok.clone()),
            level(
                "the household",
                BindingRule::binding("h", "balance >= 100"),
                ok.clone(),
                ok.clone(),
            ),
            level("bonnie", BindingRule::binding("b", "balance >= 400"), ok.clone(), ok),
        ];
        assert!(admissible_along(&path).admitted());
        assert_eq!(depth_never_adds_power(&path), Ok(()));
    }

    #[test]
    fn a_deeper_level_refusing_is_the_ordinary_case_and_not_a_violation() {
        // ★★ Depth adding a conjunct is exactly what it is for. What would be
        //    wrong is depth REMOVING one.
        let was = json!({ "balance": 500.0 });
        let tight = json!({ "balance": 50.0 });
        let path = vec![
            level(
                "the village",
                BindingRule::binding("v", "balance >= 0"),
                was.clone(),
                tight.clone(),
            ),
            level("bonnie", BindingRule::binding("b", "balance >= 400"), was, tight),
        ];
        assert!(!admissible_along(&path).admitted());
        assert_eq!(depth_never_adds_power(&path), Ok(()));
    }

    #[test]
    fn a_sub_region_that_escapes_its_parent_is_named() {
        // ★★★ `V_ω' ⊆ V_ω`. A sub-operative viable where its parent is not
        //     would be a nested thing with more freedom than the thing
        //     containing it.
        let parent = Region::new().bounding(Interval::at_least("balance", 100.0));
        let child = Region::new().bounding(Interval::at_least("balance", 0.0));
        let space = vec![
            ("rich".to_string(), json!({ "balance": 500.0 })),
            ("thin".to_string(), json!({ "balance": 50.0 })),
        ];
        assert_eq!(escapes_the_parent(&child, &parent, &space), vec!["thin".to_string()]);
    }

    #[test]
    fn a_sub_region_inside_its_parent_escapes_nothing() {
        let parent = Region::new().bounding(Interval::at_least("balance", 0.0));
        let child = Region::new().bounding(Interval::at_least("balance", 100.0));
        let space = vec![
            ("rich".to_string(), json!({ "balance": 500.0 })),
            ("thin".to_string(), json!({ "balance": 50.0 })),
        ];
        assert!(escapes_the_parent(&child, &parent, &space).is_empty());
    }
}
