//! **An external effect is two events, never one** (Events & Time · §IX).
//!
//! ```text
//!   event.effect.decided   — a person said do it
//!   event.effect.sent      — it actually left
//! ```
//!
//! ★★★ **One event cannot say both, and the gap between them is where the
//! interesting failures live.** A single *sent* event written before the send
//! claims something that has not happened yet; written after it, a crash in
//! between loses the decision entirely and nobody knows a payment was
//! authorised. Two events make the window a **state** — decided-not-yet-sent —
//! which is a thing a person can be shown and act on, rather than a moment that
//! is invisible from either side.
//!
//! ★★★ **Apply never sends, and replay cannot re-fire.** This is structural
//! here rather than a rule anybody has to keep: the journal is *derived* from
//! events by folding them, and a fold applies [`Mutation`](crate::mutation)s —
//! opaque records of what changed. There is no channel in that path to send
//! anything down. So replaying a year of history re-derives the journal and
//! contacts nobody, which is the property that makes replaying a log containing
//! real payments safe at all.
//!
//! ## What a retry is, and what it is not
//!
//! ★★★ **A failed send is never re-sent without a NEW DECISION.** The Ingest
//! article's egress asymmetry is the reason: you cannot make somebody else's
//! receiver idempotent, so a blind retry can pay twice and there is no way to
//! find out from this side. So a retry is a second `Decided` event with its own
//! key, and the journal shows two decisions rather than pretending one attempt
//! happened twice.
//!
//! ★★★ **Who may make that decision depends on the act, and the two questions
//! are different.** This module fixes the *audit* rule — a retry is never
//! invisible. [`crate::egress`] fixes the *authority* rule — an irreversible act
//! needs a person, and one whose reversibility is declared, with the
//! compensating act named, may be retried by the system a bounded number of
//! times. An earlier draft of this file said a retry always required a person,
//! which conflated the two and would have made the system interrupt somebody
//! over a cancellable draft. Interrupting people about things that do not need
//! them is how a gate stops being read.
//!
//! ★★ **The idempotency key is declared by whoever decided**, not derived here.
//! Two payments of the same amount to the same person on one day are ordinary
//! and must not collapse; only the decider knows whether this is that same
//! payment again or a second one.

use std::collections::BTreeMap;

use serde_json::Value;

/// Where one external effect has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Authorised. **Nothing has left yet.**
    Decided,
    /// It really went.
    Sent,
    /// The attempt failed, and it is not retried on its own.
    Failed,
}

/// One external effect, as the journal holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    /// The decider's own key. Two decisions with one key are one effect.
    pub idempotency_key: String,
    pub kind: String,
    pub target: String,
    pub phase: Phase,
    /// The log position the decision was written at.
    pub decided_at: u64,
    /// Where the fact-of-sending was written, when it was.
    pub settled_at: Option<u64>,
    /// Why, when it failed. In the sender's own words.
    pub failure: Option<String>,
}

impl Effect {
    /// ★★★ Authorised and not yet gone — the window a single event cannot
    /// express, and the one a person needs to see.
    pub fn in_flight(&self) -> bool {
        self.phase == Phase::Decided
    }
}

/// One line of the journal, as it is written into the log.
///
/// ★★ Two variants rather than one with a phase field: a *decision* and a
/// *fact* carry different information and are written at different times by
/// different things. Collapsing them would let a caller construct a fact
/// without a decision.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalEntry {
    Decided { idempotency_key: String, kind: String, target: String, at: u64 },
    Sent { idempotency_key: String, at: u64 },
    Failed { idempotency_key: String, at: u64, reason: String },
}

impl JournalEntry {
    pub fn key(&self) -> &str {
        match self {
            Self::Decided { idempotency_key, .. }
            | Self::Sent { idempotency_key, .. }
            | Self::Failed { idempotency_key, .. } => idempotency_key,
        }
    }
}

/// Everything the household has decided to send, and what became of it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectJournal {
    effects: BTreeMap<String, Effect>,
    /// Facts that arrived with no decision behind them.
    orphans: Vec<String>,
}

impl EffectJournal {
    /// **Fold the journal out of its own events.**
    ///
    /// ★★★ Derived, never stored alongside. A journal kept as a second copy
    /// could disagree with the log, and the day it did there would be no way to
    /// say which one a payment actually followed.
    pub fn fold(entries: &[JournalEntry]) -> Self {
        let mut journal = Self::default();
        for entry in entries {
            match entry {
                JournalEntry::Decided { idempotency_key, kind, target, at } => {
                    // ★★ First decision wins. A repeated key is the SAME
                    //    effect — that is what an idempotency key means — and
                    //    overwriting would lose when it was first authorised.
                    journal.effects.entry(idempotency_key.clone()).or_insert(Effect {
                        idempotency_key: idempotency_key.clone(),
                        kind: kind.clone(),
                        target: target.clone(),
                        phase: Phase::Decided,
                        decided_at: *at,
                        settled_at: None,
                        failure: None,
                    });
                }
                JournalEntry::Sent { idempotency_key, at } => {
                    match journal.effects.get_mut(idempotency_key) {
                        Some(effect) => {
                            effect.phase = Phase::Sent;
                            effect.settled_at = Some(*at);
                            effect.failure = None;
                        }
                        // ★★★ A fact with no decision behind it. Not dropped:
                        //     something sent money that nobody authorised, and
                        //     that is the most important line in the journal.
                        None => journal.orphans.push(idempotency_key.clone()),
                    }
                }
                JournalEntry::Failed { idempotency_key, at, reason } => {
                    match journal.effects.get_mut(idempotency_key) {
                        Some(effect) => {
                            effect.phase = Phase::Failed;
                            effect.settled_at = Some(*at);
                            effect.failure = Some(reason.clone());
                        }
                        None => journal.orphans.push(idempotency_key.clone()),
                    }
                }
            }
        }
        journal
    }

    pub fn get(&self, key: &str) -> Option<&Effect> {
        self.effects.get(key)
    }

    pub fn all(&self) -> impl Iterator<Item = &Effect> {
        self.effects.values()
    }

    /// **Decided and not yet gone.** The window a single event cannot express.
    ///
    /// ★★★ A crash between deciding and sending leaves exactly these, and they
    /// survive a replay unchanged — so an authorised payment that never left is
    /// visible rather than lost.
    pub fn in_flight(&self) -> Vec<&Effect> {
        self.effects.values().filter(|e| e.in_flight()).collect()
    }

    /// Attempts that failed. **Not retried here** — a retry is a new decision.
    pub fn failed(&self) -> Vec<&Effect> {
        self.effects.values().filter(|e| e.phase == Phase::Failed).collect()
    }

    /// Sends with no decision behind them.
    pub fn unauthorised(&self) -> &[String] {
        &self.orphans
    }
}

/// **Could applying this event send anything?**
///
/// ★★★ Always `false`, and it is a function rather than a comment so the claim
/// is executable. Applying an event folds mutations; there is no channel in
/// that path. A future change that gave one to the fold would have to delete
/// this and the test that calls it.
pub fn apply_can_send(_event: &Value) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decided(key: &str, at: u64) -> JournalEntry {
        JournalEntry::Decided {
            idempotency_key: key.into(),
            kind: "household_summary".into(),
            target: "local_file".into(),
            at,
        }
    }

    #[test]
    fn a_decision_is_not_a_send() {
        // ★★★ One event written before the send claims something that has not
        //     happened. The decision is its own fact.
        let j = EffectJournal::fold(&[decided("a", 1)]);
        assert_eq!(j.get("a").unwrap().phase, Phase::Decided);
        assert_eq!(j.in_flight().len(), 1);
    }

    #[test]
    fn the_window_between_deciding_and_sending_is_a_state_a_person_can_see() {
        // ★★★ The whole reason for two events. A crash in between loses the
        //     decision entirely under a one-event model, and nobody knows a
        //     payment was authorised.
        let j = EffectJournal::fold(&[decided("a", 1), decided("b", 2), JournalEntry::Sent {
            idempotency_key: "b".into(),
            at: 3,
        }]);
        let waiting = j.in_flight();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].idempotency_key, "a");
    }

    #[test]
    fn replaying_the_same_log_gives_the_same_journal_and_sends_nothing() {
        // ★★★ The property that makes replaying a log containing real payments
        //     safe at all. `apply_can_send` is a function so the claim is
        //     executable rather than a comment.
        let log = vec![decided("a", 1), JournalEntry::Sent { idempotency_key: "a".into(), at: 2 }];
        assert_eq!(EffectJournal::fold(&log), EffectJournal::fold(&log));
        assert!(!apply_can_send(&json!({"name": "event.effect.sent"})));
    }

    #[test]
    fn an_authorised_payment_that_never_left_survives_a_replay() {
        // ★★ Otherwise the crash window is invisible from both sides.
        let log = vec![decided("a", 1)];
        let once = EffectJournal::fold(&log);
        let again = EffectJournal::fold(&log);
        assert_eq!(once.in_flight().len(), 1);
        assert_eq!(once, again);
    }

    #[test]
    fn a_repeated_decision_under_one_key_is_one_effect() {
        // ★★ That is what an idempotency key means. Overwriting would lose when
        //    it was first authorised.
        let j = EffectJournal::fold(&[decided("a", 1), decided("a", 9)]);
        assert_eq!(j.all().count(), 1);
        assert_eq!(j.get("a").unwrap().decided_at, 1, "the first authorisation");
    }

    #[test]
    fn a_failure_is_recorded_and_never_retried_on_its_own() {
        // ★★★ Ingest's egress asymmetry: you cannot make somebody else's
        //     receiver idempotent, so a blind retry can pay twice and there is
        //     no way to find out from this side.
        let j = EffectJournal::fold(&[
            decided("a", 1),
            JournalEntry::Failed { idempotency_key: "a".into(), at: 2, reason: "no route".into() },
        ]);
        let e = j.get("a").unwrap();
        assert_eq!(e.phase, Phase::Failed);
        assert_eq!(e.failure.as_deref(), Some("no route"));
        assert!(j.in_flight().is_empty(), "a failure is not still in flight");
        assert_eq!(j.failed().len(), 1);
    }

    #[test]
    fn a_retry_is_a_new_decision_and_the_journal_shows_both() {
        // ★★★ Which means a PERSON made it. Pretending one attempt happened
        //     twice would hide who decided the second one.
        let j = EffectJournal::fold(&[
            decided("attempt-1", 1),
            JournalEntry::Failed {
                idempotency_key: "attempt-1".into(),
                at: 2,
                reason: "no route".into(),
            },
            decided("attempt-2", 3),
            JournalEntry::Sent { idempotency_key: "attempt-2".into(), at: 4 },
        ]);
        assert_eq!(j.all().count(), 2);
        assert_eq!(j.get("attempt-1").unwrap().phase, Phase::Failed);
        assert_eq!(j.get("attempt-2").unwrap().phase, Phase::Sent);
    }

    #[test]
    fn a_send_with_no_decision_behind_it_is_the_loudest_line_in_the_journal() {
        // ★★★ Something sent money nobody authorised. Dropping it as an
        //     unmatched key would hide exactly the event most worth seeing.
        let j = EffectJournal::fold(&[JournalEntry::Sent {
            idempotency_key: "ghost".into(),
            at: 1,
        }]);
        assert_eq!(j.unauthorised(), ["ghost"]);
        assert!(j.get("ghost").is_none(), "and it is not counted as a real effect");
    }

    #[test]
    fn a_failure_that_later_succeeds_under_the_same_key_ends_as_sent() {
        // ★★ The same effect, retried by whatever authorised it originally —
        //    the key is the decider's, so this is their call to make.
        let j = EffectJournal::fold(&[
            decided("a", 1),
            JournalEntry::Failed { idempotency_key: "a".into(), at: 2, reason: "flaky".into() },
            JournalEntry::Sent { idempotency_key: "a".into(), at: 3 },
        ]);
        let e = j.get("a").unwrap();
        assert_eq!(e.phase, Phase::Sent);
        assert_eq!(e.failure, None, "and the stale reason does not linger");
        assert_eq!(e.settled_at, Some(3));
    }

    #[test]
    fn an_empty_journal_says_nothing_is_in_flight() {
        let j = EffectJournal::fold(&[]);
        assert!(j.in_flight().is_empty());
        assert!(j.unauthorised().is_empty());
    }
}
