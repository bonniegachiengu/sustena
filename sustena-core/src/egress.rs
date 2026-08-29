//! **Egress is not ingest run backwards** (Ingest · §X).
//!
//! ```text
//!   ingest:   unknown → RETRY      our own intake is idempotent, so a repeat costs nothing
//!   egress:   unknown → ASK        someone else's receiver is not, and a repeat may cost money
//! ```
//!
//! ★★★ **The same word means opposite things on the two sides, and that is the
//! whole row.** [`crate::outbox`] retries on `Unknown` and is right to: the
//! destination is this engine, capture is keyed by fact, and a duplicate is
//! swallowed. Here the destination is a bank, a person, a till — and **we cannot
//! make someone else's receiver idempotent**. There is no key to send, nobody to
//! ask to honour it, and a second send of the same payment is a second payment.
//!
//! ★★★ **So there is no blind auto-retry, at all.** An egress whose outcome is
//! unknown stops and asks. That is slower, and it is the only honest answer: the
//! alternative is a system that quietly pays twice whenever the network is bad,
//! and a network being bad is not rare.
//!
//! ## Irreversibility is declared, never inferred
//!
//! ★★★ **The default is `Unknown`, and `Unknown` is treated as irreversible.**
//! Guessing in the other direction is the failure this module exists to prevent,
//! and the two mistakes are not symmetric: treating a reversible act as
//! irreversible costs one interruption, and treating an irreversible act as
//! reversible costs somebody's money. So an egress nobody has classified gets
//! the cautious answer rather than the convenient one.
//!
//! ★★ **"Reversible" must say HOW.** A flag claiming an act can be undone, with
//! no compensating act named, is a claim nobody can act on — and it is exactly
//! the claim an auto-retry would be relying on. [`Reversibility::Reversible`]
//! carries the undo, so the assertion and the means arrive together.
//!
//! ## The approval token is a term inside `admit`, not a step before it
//!
//! ★★★ Reusing [`EffectClass`] rather than inventing a second approval shape:
//! "live and unapproved" is already unrepresentable there, and a parallel
//! mechanism here would be a second door into the same room.

use crate::approval::{ApprovalToken, Binding, EffectClass, TokenError};

/// Can this act be undone, and by what?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reversibility {
    /// Undoable, and here is the act that undoes it.
    ///
    /// ★★ The compensating act is required. Without it this is a claim, and an
    /// auto-retry would be relying on a claim.
    Reversible { by: String },
    /// Once it lands it has landed.
    Irreversible,
    /// ★★★ Nobody has classified this. **Treated as irreversible** — the two
    /// mistakes are not symmetric.
    Unknown,
}

impl Reversibility {
    /// ★★★ `Unknown` answers `false`. The cautious direction is the default,
    /// and it is one method so nothing can quietly take the other one.
    pub fn is_undoable(&self) -> bool {
        matches!(self, Self::Reversible { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Reversible { by } => format!("undoable by {by}"),
            Self::Irreversible => "cannot be undone once it lands".into(),
            Self::Unknown => "nobody has said whether this can be undone".into(),
        }
    }
}

/// One thing this Sustain wants to send out of itself.
#[derive(Debug, Clone, PartialEq)]
pub struct EgressEffect {
    pub id: String,
    /// What is being asked for, in the same shape an approval is bound to.
    pub binding: Binding,
    pub reversibility: Reversibility,
    pub attempts: u32,
}

impl EgressEffect {
    /// ★★ Reversibility is a required argument. A constructor that defaulted it
    /// would let a caller ship an unclassified payment by not thinking about it.
    pub fn new(id: &str, binding: Binding, reversibility: Reversibility) -> Self {
        Self { id: id.into(), binding, reversibility, attempts: 0 }
    }
}

/// What the far side said — or did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// It landed, and the receiver said so.
    Acknowledged { reference: String },
    /// The receiver looked at it and said no. A definite answer, and a cheap one.
    Refused { reason: String },
    /// ★★★ No answer. **This is the case the row is about.**
    Unknown { detail: String },
}

/// What to do next with an egress.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Landed. Nothing more to do.
    Done { reference: String },
    /// Definitely refused. Stop — sending it again gets the same no.
    Abandon { because: String },
    /// Safe to send again without asking, because the act is undoable.
    Retry { attempts: u32 },
    /// ★★★ Stop and put it in front of a person, with the reason.
    ///
    /// Not a failure state. It is the correct outcome for an unknown outcome on
    /// an act nobody can take back, and the *only* one that does not gamble with
    /// somebody else's money.
    AskAHuman { because: String, may_have_landed: bool },
}

/// The most times a reversible egress will retry itself.
///
/// ★★ Bounded even where retrying is safe: undoable is not free, and an
/// unbounded loop against a receiver that is down is a different kind of harm.
pub const MAX_BLIND_RETRIES: u32 = 3;

/// **What to do with an egress, given what came back.**
///
/// ★★★ Compare [`crate::outbox::apply`] line for line: the same inputs, the
/// opposite answer on `Unknown`, and both are right. The difference is entirely
/// who owns the receiver.
pub fn dispose(effect: &EgressEffect, delivery: &Delivery) -> Disposition {
    match delivery {
        Delivery::Acknowledged { reference } => Disposition::Done { reference: reference.clone() },
        Delivery::Refused { reason } => {
            Disposition::Abandon { because: format!("the receiver refused it: {reason}") }
        }
        Delivery::Unknown { detail } => {
            if !effect.reversibility.is_undoable() {
                // ★★★ The row, in one branch. We do not know whether it landed,
                //     and we could not take it back if it did.
                return Disposition::AskAHuman {
                    because: format!(
                        "no answer from the receiver ({detail}), and this {}",
                        effect.reversibility.describe()
                    ),
                    may_have_landed: true,
                };
            }
            if effect.attempts >= MAX_BLIND_RETRIES {
                return Disposition::AskAHuman {
                    because: format!(
                        "{} attempts with no answer ({detail}) — retrying further is not \
                         telling us anything new",
                        effect.attempts
                    ),
                    may_have_landed: true,
                };
            }
            Disposition::Retry { attempts: effect.attempts + 1 }
        }
    }
}

/// ★★★ **There is no `NeedsApproval` refusal, and its absence is the finding.**
///
/// The obvious shape for this module was an enum with two arms — "you did not
/// approve it" and "your approval is wrong". The first arm can never be
/// constructed: [`EffectClass`] has no *live and unapproved* variant, so a
/// caller cannot reach the gate in that state to be refused in it. Writing the
/// arm anyway would put a refusal in the vocabulary that no input can produce,
/// which reads as a check being performed when nothing is being checked.
///
/// So the only way an egress fails admittance is by offering an approval that
/// does not admit it, and that is exactly [`TokenError`].
/// **`admit_egress ⟺ effect_class = sandbox ∨ valid_token(approve(o, principal))`**
///
/// ★★★ The approval token as *a term inside admittance*, not a checkbox before
/// it. A sandbox egress needs none — nothing leaves. A live one is admitted only
/// by a token bound to this exact act, and [`EffectClass`] makes "live and
/// unapproved" unrepresentable rather than merely refused.
///
/// ★★★ **A reversible live egress is admitted without a token on purpose.** The
/// human gate exists because an act cannot be taken back; asking for approval on
/// one that can would train people to approve without reading, which is how a
/// gate stops being a gate. **A token that is offered and wrong is still wrong**
/// either way — not asking and asking badly are different things.
pub fn admit_egress(effect: &EgressEffect, class: &EffectClass<'_>) -> Result<(), TokenError> {
    match class {
        EffectClass::Sandbox | EffectClass::Unchecked => Ok(()),
        EffectClass::Live { token, now } => validate_for(effect, token, *now),
    }
}

fn validate_for(
    effect: &EgressEffect,
    token: &ApprovalToken,
    now: u64,
) -> Result<(), TokenError> {
    token.validate(&effect.binding, Some(token.principal()), now)
}

/// Does this egress require a person before it may go live at all?
///
/// ★★ Separated from [`admit_egress`] so a surface can ask the question before
/// it has a token to offer — "will this need you?" is a different question from
/// "does this approval admit it?", and answering the second when a caller asked
/// the first is how a gate becomes a surprise.
pub fn needs_a_human(effect: &EgressEffect) -> bool {
    !effect.reversibility.is_undoable()
}

/// Everything waiting on a person, so a surface can show it as one queue.
///
/// ★★ Deliberately not merged with the outbox's depth metric: a deep *ingest*
/// queue means the engine is unreachable, and a deep *egress* queue means people
/// are being asked questions faster than they answer them. Two different
/// problems, and one list each.
pub fn awaiting_a_human(dispositions: &[(String, Disposition)]) -> Vec<(&str, &str)> {
    dispositions
        .iter()
        .filter_map(|(id, d)| match d {
            Disposition::AskAHuman { because, .. } => Some((id.as_str(), because.as_str())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::Simulated;
    use crate::council::ProposalStatus;
    use serde_json::{json, Map, Value};

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn payment() -> Binding {
        Binding::new(
            "egress.pay",
            &params(&[("to", json!("the landlord")), ("amount", json!(15000.0))]),
        )
    }

    fn token(b: Binding, principal: &str, nonce: u64, expiry: u64) -> ApprovalToken {
        Simulated::from_sandbox("prop-1", b, Ok(()))
            .expect("sandbox committed")
            .voted(ProposalStatus::Passed)
            .expect("passed")
            .approve(principal, nonce, expiry)
    }

    fn irreversible() -> EgressEffect {
        EgressEffect::new("e1", payment(), Reversibility::Irreversible)
    }

    fn undoable() -> EgressEffect {
        EgressEffect::new(
            "e3",
            payment(),
            Reversibility::Reversible { by: "egress.cancel_draft".into() },
        )
    }

    fn unknown_outcome() -> Delivery {
        Delivery::Unknown { detail: "timed out".into() }
    }

    #[test]
    fn an_acknowledged_egress_is_finished() {
        let d = dispose(&irreversible(), &Delivery::Acknowledged { reference: "REF9".into() });
        assert_eq!(d, Disposition::Done { reference: "REF9".into() });
    }

    #[test]
    fn a_definite_refusal_stops_rather_than_retrying() {
        // ★★ A definite answer is a cheap answer. Sending it again gets the
        //    same no, and a queue that keeps refused work never drains.
        let d = dispose(&irreversible(), &Delivery::Refused { reason: "no such till".into() });
        assert!(matches!(d, Disposition::Abandon { .. }));
    }

    #[test]
    fn an_unknown_outcome_on_an_irreversible_act_asks_a_person() {
        // ★★★ The row. We cannot make someone else's receiver idempotent, so a
        //     blind retry is a second payment.
        match dispose(&irreversible(), &unknown_outcome()) {
            Disposition::AskAHuman { because, may_have_landed } => {
                assert!(may_have_landed, "it may already have landed — say so");
                assert!(because.contains("cannot be undone"));
            }
            other => panic!("must not retry blindly: {other:?}"),
        }
    }

    #[test]
    fn the_same_unknown_that_ingest_retries_on_is_the_one_egress_stops_on() {
        // ★★★ Identical input, opposite answer, and both are right — the
        //     difference is entirely who owns the receiver.
        let ingest = crate::outbox::apply(
            &crate::outbox::Pending::new("e1"),
            &crate::outbox::Ack::Unknown { detail: "timed out".into() },
            crate::outbox::Backoff::new(1_000, 60_000),
        );
        assert!(matches!(ingest, crate::outbox::Disposition::Retry { .. }));
        assert!(matches!(
            dispose(&irreversible(), &unknown_outcome()),
            Disposition::AskAHuman { .. }
        ));
    }

    #[test]
    fn an_unclassified_egress_is_treated_as_irreversible() {
        // ★★★ The two mistakes are not symmetric: one costs an interruption,
        //     the other costs somebody's money.
        let unclassified = EgressEffect::new("e2", payment(), Reversibility::Unknown);
        assert!(!unclassified.reversibility.is_undoable());
        assert!(needs_a_human(&unclassified));
        assert!(matches!(
            dispose(&unclassified, &unknown_outcome()),
            Disposition::AskAHuman { .. }
        ));
    }

    #[test]
    fn a_reversible_act_may_retry_itself_because_it_can_be_taken_back() {
        assert_eq!(dispose(&undoable(), &unknown_outcome()), Disposition::Retry { attempts: 1 });
        assert!(!needs_a_human(&undoable()));
    }

    #[test]
    fn even_a_safe_retry_is_bounded() {
        // ★★ Undoable is not free, and an unbounded loop against a receiver
        //    that is down is a different kind of harm.
        let mut tired = undoable();
        tired.attempts = MAX_BLIND_RETRIES;
        match dispose(&tired, &unknown_outcome()) {
            Disposition::AskAHuman { because, .. } => {
                assert!(because.contains("not telling us anything new"));
            }
            other => panic!("expected escalation: {other:?}"),
        }
    }

    #[test]
    fn reversible_has_to_say_how() {
        // ★★ A flag claiming an act can be undone, with no compensating act
        //    named, is the claim an auto-retry would be relying on.
        assert!(undoable().reversibility.describe().contains("egress.cancel_draft"));
    }

    #[test]
    fn a_sandbox_egress_needs_no_approval_because_nothing_leaves() {
        assert_eq!(admit_egress(&irreversible(), &EffectClass::Sandbox), Ok(()));
    }

    #[test]
    fn an_approval_bound_to_this_act_admits_it() {
        // ★★★ The approval token as a term inside admittance. `EffectClass`
        //     makes "live and unapproved" unrepresentable rather than merely
        //     refused — there is no variant to construct.
        let t = token(payment(), "bonnie", 1, 100);
        assert_eq!(admit_egress(&irreversible(), &EffectClass::Live { token: &t, now: 50 }), Ok(()));
    }

    #[test]
    fn a_token_for_a_different_act_does_not_admit_this_one() {
        let other = Binding::new("egress.pay", &params(&[("to", json!("somebody else"))]));
        let t = token(other, "bonnie", 1, 100);
        assert!(admit_egress(&irreversible(), &EffectClass::Live { token: &t, now: 50 }).is_err());
    }

    #[test]
    fn an_expired_approval_does_not_admit_it_either() {
        let t = token(payment(), "bonnie", 1, 10);
        assert!(admit_egress(&irreversible(), &EffectClass::Live { token: &t, now: 500 }).is_err());
    }

    #[test]
    fn a_wrong_token_is_still_wrong_on_a_reversible_act() {
        // ★★ Not asking for approval and offering a bad one are different
        //    things. The second is a mistake whichever way the act runs.
        let other = Binding::new("egress.pay", &params(&[("to", json!("somebody else"))]));
        let t = token(other, "bonnie", 1, 100);
        assert!(admit_egress(&undoable(), &EffectClass::Live { token: &t, now: 50 }).is_err());
    }

    #[test]
    fn what_is_waiting_on_a_person_is_one_answerable_list() {
        // ★★ A deep INGEST queue means the engine is unreachable; a deep EGRESS
        //    queue means people are being asked faster than they answer. Two
        //    different problems, and one list each.
        let ds = vec![
            ("e1".to_string(), dispose(&irreversible(), &unknown_outcome())),
            (
                "e2".to_string(),
                dispose(&irreversible(), &Delivery::Acknowledged { reference: "R".into() }),
            ),
        ];
        let waiting = awaiting_a_human(&ds);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].0, "e1");
    }
}
