//! The Consensus primitive — how several parties agree on one change (SLIME).
//!
//! Two decisions live here, and they are deliberately separate:
//!
//! **Aggregation.** A councillor may delegate to sub-operatives. Their
//! individual votes combine, weighted by confidence, into that councillor's
//! single vote.
//!
//! **Resolution.** Councillor votes plus the person's own decision determine
//! what happens to a proposal.
//!
//! ## The person decides
//!
//! Operatives never resolve a proposal on their own. Enough councillor support
//! moves it to IN_VOTING — *awaiting the human* — and no further. A person's
//! YES passes it; their NO overrides it outright, regardless of how the
//! council voted. Left alone past its deadline it is DEFERRED, never
//! auto-approved. Silence is not consent.
//!
//! That asymmetry is the whole design (Controller: "the human enters at
//! DECIDE"), so it is asserted directly by the conformance vectors rather than
//! left implicit in the branch order.
//!
//! ## No clock here
//!
//! Expiry compares two timestamps the caller supplies. The core does not read
//! a clock — that is a host concern, and a core that read one could not be
//! replayed deterministically.

use serde::{Deserialize, Serialize};

/// How a councillor (or sub-operative) voted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum VoteChoice {
    Yes,
    No,
    Abstain,
}

impl VoteChoice {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "YES" => Some(Self::Yes),
            "NO" => Some(Self::No),
            "ABSTAIN" => Some(Self::Abstain),
            _ => None,
        }
    }
}

/// One sub-operative's vote, with the confidence it carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegatedVote {
    pub position: VoteChoice,
    pub confidence: f64,
    #[serde(default)]
    pub reasoning: String,
}

/// A councillor's single vote, after aggregation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperativeVote {
    pub vote: VoteChoice,
    pub reasoning: String,
    pub utility: f64,
}

/// Combine delegated votes into the councillor's one vote.
///
/// - ABSTAIN carries no weight and is excluded from the sums.
/// - `weighted_yes` / `weighted_no` are the summed confidences.
/// - The heavier side wins; a tie, or all-abstain, is ABSTAIN.
/// - `utility` is the mean confidence of the YES votes, or 0.5 if there are
///   none — an explicit "no opinion" rather than a misleading zero.
pub fn aggregate_delegated_votes(delegated: &[DelegatedVote]) -> OperativeVote {
    if delegated.is_empty() {
        return OperativeVote {
            vote: VoteChoice::Abstain,
            reasoning: "No delegated votes.".into(),
            utility: 0.5,
        };
    }

    let reasoning = delegated
        .iter()
        .map(|d| d.reasoning.clone())
        .collect::<Vec<_>>()
        .join(" | ");

    let weighted_yes: f64 = delegated
        .iter()
        .filter(|d| d.position == VoteChoice::Yes)
        .map(|d| d.confidence)
        .sum();
    let weighted_no: f64 = delegated
        .iter()
        .filter(|d| d.position == VoteChoice::No)
        .map(|d| d.confidence)
        .sum();

    let yes_confidences: Vec<f64> = delegated
        .iter()
        .filter(|d| d.position == VoteChoice::Yes)
        .map(|d| d.confidence)
        .collect();
    let utility = if yes_confidences.is_empty() {
        0.5
    } else {
        yes_confidences.iter().sum::<f64>() / yes_confidences.len() as f64
    };

    let vote = if weighted_yes > weighted_no {
        VoteChoice::Yes
    } else if weighted_no > weighted_yes {
        VoteChoice::No
    } else {
        VoteChoice::Abstain
    };

    OperativeVote { vote, reasoning, utility }
}

/// Where a proposal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProposalStatus {
    /// Enough support to matter, waiting on the person. Not an approval.
    InVoting,
    Passed,
    /// The council produced no votes at all.
    Failed,
    /// The deadline passed with no human decision. Never an auto-approval.
    Deferred,
    OverriddenByUser,
}

impl ProposalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InVoting => "IN_VOTING",
            Self::Passed => "PASSED",
            Self::Failed => "FAILED",
            Self::Deferred => "DEFERRED",
            Self::OverriddenByUser => "OVERRIDDEN_BY_USER",
        }
    }
}

/// Everything resolution needs. Time is supplied, never read.
#[derive(Debug, Clone, Default)]
pub struct ResolutionInput<'a> {
    /// The councillor votes cast so far.
    pub votes: &'a [VoteChoice],
    /// True once vote collection has run — distinguishes "no votes yet" from
    /// "asked, and nobody answered".
    pub votes_collected: bool,
    /// The person's decision, if they have made one.
    pub user_vote: Option<VoteChoice>,
    /// Whether the proposal's deadline has passed. Computed by the host from
    /// its own clock; the core compares nothing it cannot replay.
    pub expired: bool,
}

/// Resolve a proposal.
///
/// Branch order matters and mirrors the reference exactly:
///
/// 1. Asked, and nobody answered → FAILED.
/// 2. The person decided → PASSED or OVERRIDDEN_BY_USER. **This outranks the
///    council**: a NO overrides any amount of councillor support.
/// 3. Expired with no human decision → DEFERRED.
/// 4. Otherwise → IN_VOTING, still waiting on the person.
pub fn resolve(input: &ResolutionInput) -> ProposalStatus {
    if input.votes_collected && input.votes.is_empty() {
        return ProposalStatus::Failed;
    }

    if let Some(user) = input.user_vote {
        return match user {
            VoteChoice::Yes => ProposalStatus::Passed,
            VoteChoice::No => ProposalStatus::OverriddenByUser,
            // An abstaining person has not decided, so it keeps waiting.
            VoteChoice::Abstain => ProposalStatus::InVoting,
        };
    }

    if input.expired {
        return ProposalStatus::Deferred;
    }

    // Councillor support alone never passes anything. Reaching the threshold
    // changes who is waiting, not whether it is approved.
    ProposalStatus::InVoting
}

/// How many councillors voted YES.
pub fn yes_count(votes: &[VoteChoice]) -> usize {
    votes.iter().filter(|v| **v == VoteChoice::Yes).count()
}

/// The support threshold at which a person is asked to decide.
pub const NOTIFY_THRESHOLD: usize = 2;

#[cfg(test)]
mod tests {
    use super::*;

    fn dv(position: VoteChoice, confidence: f64) -> DelegatedVote {
        DelegatedVote { position, confidence, reasoning: "r".into() }
    }

    #[test]
    fn heavier_confidence_wins() {
        let out = aggregate_delegated_votes(&[
            dv(VoteChoice::Yes, 0.9),
            dv(VoteChoice::No, 0.4),
        ]);
        assert_eq!(out.vote, VoteChoice::Yes);
    }

    #[test]
    fn a_tie_abstains_rather_than_guessing() {
        let out = aggregate_delegated_votes(&[
            dv(VoteChoice::Yes, 0.5),
            dv(VoteChoice::No, 0.5),
        ]);
        assert_eq!(out.vote, VoteChoice::Abstain);
    }

    #[test]
    fn abstentions_carry_no_weight() {
        let out = aggregate_delegated_votes(&[
            dv(VoteChoice::Abstain, 1.0),
            dv(VoteChoice::Yes, 0.1),
        ]);
        assert_eq!(out.vote, VoteChoice::Yes, "a confident abstention is still no opinion");
    }

    #[test]
    fn no_votes_at_all_abstains_with_a_neutral_utility() {
        let out = aggregate_delegated_votes(&[]);
        assert_eq!(out.vote, VoteChoice::Abstain);
        assert_eq!(out.utility, 0.5);
    }

    #[test]
    fn utility_is_neutral_when_nobody_said_yes() {
        let out = aggregate_delegated_votes(&[dv(VoteChoice::No, 0.9)]);
        assert_eq!(out.utility, 0.5, "0.0 would read as a confident no-value");
    }

    #[test]
    fn council_support_alone_never_passes_a_proposal() {
        let status = resolve(&ResolutionInput {
            votes: &[VoteChoice::Yes, VoteChoice::Yes, VoteChoice::Yes],
            votes_collected: true,
            user_vote: None,
            expired: false,
        });
        assert_eq!(status, ProposalStatus::InVoting, "the person still has to decide");
    }

    #[test]
    fn a_person_can_override_a_unanimous_council() {
        let status = resolve(&ResolutionInput {
            votes: &[VoteChoice::Yes, VoteChoice::Yes, VoteChoice::Yes],
            votes_collected: true,
            user_vote: Some(VoteChoice::No),
            expired: false,
        });
        assert_eq!(status, ProposalStatus::OverriddenByUser);
    }

    #[test]
    fn a_person_can_pass_a_proposal_the_council_rejected() {
        let status = resolve(&ResolutionInput {
            votes: &[VoteChoice::No, VoteChoice::No],
            votes_collected: true,
            user_vote: Some(VoteChoice::Yes),
            expired: false,
        });
        assert_eq!(status, ProposalStatus::Passed);
    }

    #[test]
    fn silence_past_the_deadline_defers_and_never_approves() {
        let status = resolve(&ResolutionInput {
            votes: &[VoteChoice::Yes, VoteChoice::Yes],
            votes_collected: true,
            user_vote: None,
            expired: true,
        });
        assert_eq!(status, ProposalStatus::Deferred, "silence is not consent");
    }

    #[test]
    fn asked_and_nobody_answered_is_a_failure_not_a_wait() {
        let status = resolve(&ResolutionInput {
            votes: &[],
            votes_collected: true,
            user_vote: None,
            expired: false,
        });
        assert_eq!(status, ProposalStatus::Failed);
    }

    #[test]
    fn not_yet_asked_keeps_waiting() {
        let status = resolve(&ResolutionInput {
            votes: &[],
            votes_collected: false,
            user_vote: None,
            expired: false,
        });
        assert_eq!(status, ProposalStatus::InVoting);
    }
}
