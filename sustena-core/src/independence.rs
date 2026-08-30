//! **What a council's agreement is actually worth** (Operative · §VII, §XIII).
//!
//! ★★★ **Condorcet's jury theorem needs independent votes, and a council of
//! forks does not have them.** Five councillors reasoning under one world model
//! are not five draws from the world; they are one draw, reported five times.
//! The comfort of a large `N` with the accuracy of a small one is the exact
//! shape of the failure, and it is invisible from inside — a unanimous council
//! looks like strong evidence whether the agreement came from the world or from
//! the model they share.
//!
//! ★★★ **Forks buy independence of SAMPLING, not of ASSUMPTIONS.** Giving each
//! councillor its own sandbox stops them contaminating each other's arithmetic.
//! It does nothing about the thing they were all handed before they started. So
//! per-councillor forking is necessary and is not sufficient, and this module
//! exists to keep the difference sayable.
//!
//! ## Effective N is an UPPER bound, and the sign is the honest part
//!
//! ★★★ Counting distinct world models over-states independence: two models
//! written by the same person, from the same data, on the same afternoon, are
//! different files and not different assumptions. The true effective `N` is
//! **at most** this and nobody inside the system can say by how much — which is
//! §XIII's own qualification, and the reason [`Independence::effective_n`] is
//! documented as a ceiling rather than a measurement. A number presented as a
//! measurement here would be the failure it is trying to report.
//!
//! ## Unanimity is read as a question, not as a result
//!
//! ★★★ When every councillor shares a model, unanimity carries **no information
//! about the world** — it is the model talking. [`Reading::agreement_is`] says
//! which of the two a given agreement can be, and it refuses to call a
//! single-model unanimity evidence.

use std::collections::BTreeSet;

use crate::council::VoteChoice;

/// What a world model claims to be faithful about.
///
/// ★★★ Declared, because an undeclared fidelity claim is the one that never gets
/// checked. "This model is right about income timing and says nothing about
/// prices" is a sentence somebody can disagree with; a model with no claim
/// attached is one nobody can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fidelity {
    /// The model claims to be faithful about these things and silent elsewhere.
    Claims { about: BTreeSet<String> },
    /// ★★★ Nothing was declared. **Not read as "faithful about everything"** —
    /// read as unknown, which is what it is.
    Undeclared,
}

impl Fidelity {
    pub fn about(items: &[&str]) -> Self {
        Self::Claims { about: items.iter().map(|s| s.to_string()).collect() }
    }

    /// Does this model claim to be faithful about the thing being decided?
    ///
    /// ★★ An undeclared model answers **false**. A councillor reasoning under a
    /// model that never claimed to know about this is not adding a judgment; it
    /// is adding a number.
    pub fn covers(&self, topic: &str) -> bool {
        match self {
            Self::Claims { about } => about.contains(topic),
            Self::Undeclared => false,
        }
    }
}

/// The model a councillor reasons under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldModel {
    pub id: String,
    pub fidelity: Fidelity,
}

impl WorldModel {
    pub fn new(id: &str, fidelity: Fidelity) -> Self {
        Self { id: id.into(), fidelity }
    }
}

/// One councillor's seat: who they are, what they ran in, what they assumed.
#[derive(Debug, Clone, PartialEq)]
pub struct Seat {
    pub councillor: String,
    /// ★★★ The sandbox this councillor ran in. Distinct per councillor, or they
    /// are contaminating each other's arithmetic as well as sharing assumptions.
    pub fork_id: String,
    pub model_id: String,
    pub vote: VoteChoice,
}

impl Seat {
    pub fn new(councillor: &str, fork_id: &str, model_id: &str, vote: VoteChoice) -> Self {
        Self {
            councillor: councillor.into(),
            fork_id: fork_id.into(),
            model_id: model_id.into(),
            vote,
        }
    }
}

/// What a council's agreement can and cannot be evidence of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// Councillors reasoning under genuinely different models agreed.
    ///
    /// ★★ Still bounded: distinct models are not proven-independent
    /// assumptions, so this is the strongest available reading and not a proof.
    EvidenceFromDisagreeingStarts { models: usize },
    /// ★★★ Everyone shared a model, so agreement says nothing about the world.
    ///
    /// **The dangerous case, because it is the one that looks best.**
    TheModelTalking { model_id: String },
    /// They did not agree. Not a failure — a council that disagrees has told
    /// you something a unanimous one cannot.
    Divided,
}

/// The independence reading over one vote.
#[derive(Debug, Clone, PartialEq)]
pub struct Independence {
    pub headcount: usize,
    /// ★★★ **A CEILING on the number of independent judgments, never a
    /// measurement.** Distinct models may still share assumptions, and nobody
    /// inside the system can say by how much.
    pub effective_n: usize,
    /// Forks that were reused across councillors — a separate failure from
    /// sharing a model, and a cruder one.
    pub shared_forks: Vec<String>,
    /// Councillors whose model never claimed to know about this topic.
    pub speaking_outside_their_claim: Vec<String>,
}

impl Independence {
    /// How much of the apparent council is real, at most.
    ///
    /// ★★ Named `at_most` so a caller reading it aloud says the qualification.
    pub fn at_most(&self) -> usize {
        self.effective_n
    }

    /// Is the council smaller than it looks?
    pub fn is_inflated(&self) -> bool {
        self.effective_n < self.headcount
    }

    pub fn describe(&self) -> String {
        let mut said = if self.is_inflated() {
            format!(
                "{} councillors, but at most {} independent judgments",
                self.headcount, self.effective_n
            )
        } else {
            format!("{} councillors under {} different models", self.headcount, self.effective_n)
        };
        if !self.shared_forks.is_empty() {
            said.push_str(&format!(
                "; {} sandbox(es) were shared, so some did not even reason separately",
                self.shared_forks.len()
            ));
        }
        if !self.speaking_outside_their_claim.is_empty() {
            said.push_str(&format!(
                "; {} spoke outside what their model claims to know",
                self.speaking_outside_their_claim.join(", ")
            ));
        }
        said
    }
}

/// **Read a council's independence.**
pub fn independence(seats: &[Seat], models: &[WorldModel], topic: &str) -> Independence {
    let distinct_models: BTreeSet<&str> = seats.iter().map(|s| s.model_id.as_str()).collect();

    let mut shared_forks = Vec::new();
    let mut seen_forks: BTreeSet<&str> = BTreeSet::new();
    for s in seats {
        if !seen_forks.insert(s.fork_id.as_str()) && !shared_forks.contains(&s.fork_id) {
            shared_forks.push(s.fork_id.clone());
        }
    }

    let speaking_outside_their_claim = seats
        .iter()
        .filter(|s| {
            models
                .iter()
                .find(|m| m.id == s.model_id)
                // ★★ A model nobody declared is treated as not covering the
                //    topic, which is the same answer `Fidelity::Undeclared`
                //    gives — a missing model is not a permissive one.
                .map_or(true, |m| !m.fidelity.covers(topic))
        })
        .map(|s| s.councillor.clone())
        .collect();

    Independence {
        headcount: seats.len(),
        effective_n: distinct_models.len(),
        shared_forks,
        speaking_outside_their_claim,
    }
}

impl Reading {
    /// **What this agreement can be evidence of.**
    ///
    /// ★★★ Refuses to call a single-model unanimity evidence. That is the whole
    /// row: the agreement that looks strongest is the one that carries least.
    pub fn agreement_is(seats: &[Seat]) -> Reading {
        if seats.is_empty() {
            return Reading::Divided;
        }
        let first = &seats[0].vote;
        if !seats.iter().all(|s| &s.vote == first) {
            return Reading::Divided;
        }
        let models: BTreeSet<&str> = seats.iter().map(|s| s.model_id.as_str()).collect();
        if models.len() == 1 {
            return Reading::TheModelTalking {
                model_id: seats[0].model_id.clone(),
            };
        }
        Reading::EvidenceFromDisagreeingStarts { models: models.len() }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::EvidenceFromDisagreeingStarts { models } => format!(
                "they agreed from {models} different models, which is the strongest reading \
                 available and still not a proof"
            ),
            Self::TheModelTalking { model_id } => format!(
                "they all agreed and they all reasoned under '{model_id}' — this says the model \
                 is consistent, not that the world agrees"
            ),
            Self::Divided => {
                "they did not agree, which tells you something a unanimous council cannot".into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_model() -> Vec<WorldModel> {
        vec![WorldModel::new("house", Fidelity::about(&["income timing"]))]
    }

    fn three_models() -> Vec<WorldModel> {
        vec![
            WorldModel::new("house", Fidelity::about(&["income timing"])),
            WorldModel::new("cautious", Fidelity::about(&["income timing"])),
            WorldModel::new("optimistic", Fidelity::about(&["income timing"])),
        ]
    }

    fn all_agree_one_model() -> Vec<Seat> {
        vec![
            Seat::new("mentor", "fork-1", "house", VoteChoice::Yes),
            Seat::new("attache", "fork-2", "house", VoteChoice::Yes),
            Seat::new("curator", "fork-3", "house", VoteChoice::Yes),
        ]
    }

    fn all_agree_three_models() -> Vec<Seat> {
        vec![
            Seat::new("mentor", "fork-1", "house", VoteChoice::Yes),
            Seat::new("attache", "fork-2", "cautious", VoteChoice::Yes),
            Seat::new("curator", "fork-3", "optimistic", VoteChoice::Yes),
        ]
    }

    #[test]
    fn three_councillors_under_one_model_are_one_judgment_reported_three_times() {
        // ★★★ The comfort of a large N with the accuracy of a small one.
        let i = independence(&all_agree_one_model(), &one_model(), "income timing");
        assert_eq!(i.headcount, 3);
        assert_eq!(i.at_most(), 1);
        assert!(i.is_inflated());
        assert!(i.describe().contains("at most 1 independent"));
    }

    #[test]
    fn separate_forks_do_not_fix_shared_assumptions() {
        // ★★★ Forks buy independence of sampling, not of assumptions. Three
        //     genuinely separate sandboxes, and still one judgment.
        let seats = all_agree_one_model();
        let forks: BTreeSet<&str> = seats.iter().map(|s| s.fork_id.as_str()).collect();
        assert_eq!(forks.len(), 3, "they really did run separately");
        assert_eq!(independence(&seats, &one_model(), "income timing").at_most(), 1);
    }

    #[test]
    fn a_unanimous_single_model_council_is_not_evidence() {
        // ★★★ The dangerous case, because it is the one that looks best.
        match Reading::agreement_is(&all_agree_one_model()) {
            Reading::TheModelTalking { model_id } => assert_eq!(model_id, "house"),
            other => panic!("must not read as evidence: {other:?}"),
        }
        assert!(Reading::agreement_is(&all_agree_one_model())
            .describe()
            .contains("not that the world agrees"));
    }

    #[test]
    fn agreement_from_different_models_is_the_strongest_reading_and_still_not_a_proof() {
        // ★★ Distinct models are not proven-independent assumptions.
        let r = Reading::agreement_is(&all_agree_three_models());
        assert_eq!(r, Reading::EvidenceFromDisagreeingStarts { models: 3 });
        assert!(r.describe().contains("still not a proof"));
    }

    #[test]
    fn effective_n_is_a_ceiling_and_the_wording_carries_it() {
        // ★★★ §XIII's own qualification: the true figure is at most this and
        //     nobody inside can say by how much. A number presented as a
        //     measurement here would be the failure it is reporting.
        let i = independence(&all_agree_three_models(), &three_models(), "income timing");
        assert_eq!(i.at_most(), 3);
        assert!(!i.is_inflated(), "nothing detectable was lost — which is not the same as none");
    }

    #[test]
    fn a_divided_council_told_you_something_a_unanimous_one_cannot() {
        let mut seats = all_agree_three_models();
        seats[1].vote = VoteChoice::No;
        assert_eq!(Reading::agreement_is(&seats), Reading::Divided);
    }

    #[test]
    fn a_reused_sandbox_is_a_separate_and_cruder_failure() {
        // ★★ Sharing a model is subtle; sharing a fork means they did not even
        //    reason separately, and the two are worth distinguishing.
        let seats = vec![
            Seat::new("mentor", "fork-1", "house", VoteChoice::Yes),
            Seat::new("attache", "fork-1", "cautious", VoteChoice::Yes),
        ];
        let i = independence(&seats, &three_models(), "income timing");
        assert_eq!(i.shared_forks, vec!["fork-1"]);
        assert!(i.describe().contains("did not even reason separately"));
    }

    #[test]
    fn a_councillor_whose_model_never_claimed_to_know_this_is_named() {
        // ★★ It is not adding a judgment; it is adding a number.
        let models = vec![WorldModel::new("house", Fidelity::about(&["prices"]))];
        let i = independence(&all_agree_one_model(), &models, "income timing");
        assert_eq!(i.speaking_outside_their_claim.len(), 3);
        assert!(i.describe().contains("outside what their model claims"));
    }

    #[test]
    fn an_undeclared_fidelity_is_unknown_and_not_universal() {
        // ★★★ The undeclared claim is the one that never gets checked, so it
        //     covers nothing rather than everything.
        assert!(!Fidelity::Undeclared.covers("anything at all"));
        assert!(Fidelity::about(&["income timing"]).covers("income timing"));
        assert!(!Fidelity::about(&["income timing"]).covers("prices"));
    }

    #[test]
    fn a_model_nobody_declared_is_not_a_permissive_one() {
        // ★★ A seat naming a model that is not in the list gets the same answer
        //    an undeclared one does.
        let i = independence(&all_agree_one_model(), &[], "income timing");
        assert_eq!(i.speaking_outside_their_claim.len(), 3);
    }

    #[test]
    fn an_empty_council_has_not_agreed_about_anything() {
        assert_eq!(Reading::agreement_is(&[]), Reading::Divided);
    }
}
