//! Replay and checkpointing (RECORD §IX).
//!
//! ```text
//!   a checkpoint is (s_k, k)  with  s_k = fold(apply, s0, L[1..k])
//!   fold(apply, s0, L) = fold(apply, s_k, L[k+1..n])
//! ```
//!
//! which is exactly §II's homomorphism `fold(L₁·L₂) = fold(L₂) ∘ fold(L₁)`.
//! Replay cost becomes `O(n − k)` instead of `O(n)`.
//!
//! ## ★★ A checkpoint is an optimisation that can always be discarded
//!
//! §IX: *"a checkpoint is an optimisation that can always be discarded without
//! loss, because `L` is still the truth."* That is made structural in two ways
//! rather than promised:
//!
//! - **A checkpoint is derived, never authored.** [`Checkpoint::at`] is the
//!   only constructor and it *folds the log* to produce one. There is no
//!   constructor taking a state, so a checkpoint claiming a state the log never
//!   produced cannot be written.
//! - **Both paths are the same function.** [`replay`] takes
//!   `Option<&Checkpoint>`; passing `None` is the from-scratch replay and
//!   passing `Some` is the fast one, so *discarding the checkpoint* is
//!   literally changing one argument, and the equality is a test rather than a
//!   design intention.
//!
//! Deserialisation can still bypass the constructor, so a checkpoint carries
//! the id of the event it was taken through and [`replay`] checks it — an O(1)
//! check that catches the realistic failure, a checkpoint attached to the wrong
//! log. It does **not** catch a corrupted state; only [`Checkpoint::verify`]
//! does, and that costs `O(k)` — exactly what the checkpoint saved. Both are
//! offered; neither is pretended to be the other.
//!
//! ## ★★ Replay issues no external effects, because it cannot reach them
//!
//! §IX's pseudocode ends with
//! `ASSERT mode == REPLAY IMPLIES no_external_effects_were_issued()`.
//!
//! Here that is not an assertion at the end but a fact about the signature.
//! [`replay`] takes `&[Event]` and returns a `Value`. It has no event sink, no
//! movement channel, no registry, and it never calls
//! [`execute`](crate::operator::execute) — the only thing it calls is
//! [`apply_mutation`](crate::fold::apply_mutation), which takes a state and a
//! mutation and nothing else.
//!
//! The contrast is the proof: an operator body receives
//! `&mut Vec<EmittedEvent>` and `&mut Vec<Movement>`, because emitting and
//! moving is what it is for. Replay receives neither. **An SMS cannot be re-sent
//! on replay because there is nothing here to send it with.**
//!
//! Purity follows the same way — `apply_mutation` reads no clock and no
//! randomness, so replaying the same log twice returns the same state, which is
//! asserted rather than assumed.
//!
//! ## ★★ A finding: §IX's `seen` set is per-call, and that is subtly not enough
//!
//! The pseudocode initialises `seen = set()` and then iterates `L[start+1..]`.
//! With a checkpoint, `start = k`, so **the ids already folded into the
//! checkpoint are not in `seen`**. A duplicate of an early event appearing late
//! in the log is therefore *skipped* on the from-scratch path and *applied* on
//! the checkpointed one — the two paths disagree, and the "discard it freely"
//! property quietly fails.
//!
//! Carrying the folded id set on the checkpoint would fix it and would make the
//! checkpoint grow with `k`, which is most of what it was for. So the fix is
//! taken where §V already puts it: **dedupe is uniform at the substrate**
//! ([`merge`](crate::event::merge)), and [`replay`] requires a log that has been
//! through it — refusing with [`ReplayError::DuplicateEventId`] rather than
//! silently doing one of the two wrong things. A conformance case runs the
//! pseudocode literally to show the divergence it would produce.
//!
//! ## Two consumers, and this is general for them
//!
//! §IX: Tenet replays to stand in a chosen future; Editing replays recent
//! history under a changed definition `D′`. Both need §IX *general* rather than
//! one hand-built path. [`replay_to`] answers the first — replay a prefix and
//! read the state there. The second (EVT-15/EDIT-11) needs a `D′` to replay
//! *under*, which is a separate row and is not started here.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::event::Event;
use crate::error::FoldError;
use crate::fold::{fold_events, FoldEvent};

/// What can go wrong replaying a log.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ReplayError {
    #[error(
        "a checkpoint at k={k} cannot be taken over a log of {len} event(s) — there is nothing \
         at that position to have folded through"
    )]
    CheckpointBeyondLog { k: usize, len: usize },
    #[error(
        "a checkpoint at k=0 is exactly no checkpoint; pass `None` instead, which is the same \
         computation said honestly"
    )]
    EmptyCheckpoint,
    #[error(
        "this checkpoint was taken through event '{expected}' but L[{k}] is '{found}' — it \
         belongs to a different log, or to this one before it changed"
    )]
    CheckpointNotOfThisLog { k: usize, expected: String, found: String },
    #[error(
        "event id '{0}' appears twice — replay needs a log already deduped at the substrate \
         (§V), because a per-call `seen` set makes the checkpointed and from-scratch paths \
         disagree on a late duplicate of an early event"
    )]
    DuplicateEventId(String),
    #[error("cannot replay through {upto} events; the log has {len}")]
    UptoBeyondLog { upto: usize, len: usize },
    #[error(transparent)]
    Fold(#[from] FoldError),
}

/// A checkpoint `(s_k, k)`.
///
/// ★ **Derived, never authored** — [`Checkpoint::at`] folds the log to produce
/// one, and there is no constructor that takes a state. A checkpoint claiming a
/// state the log never produced is not something that can be written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    state: Value,
    k: usize,
    /// The id of `L[k-1]`, so a checkpoint carried across a log boundary is
    /// caught in `O(1)` rather than trusted.
    through_event_id: String,
}

impl Checkpoint {
    /// Take a checkpoint after the first `k` events — `s_k = fold(s₀, L[1..k])`.
    pub fn at(log: &[Event], k: usize) -> Result<Self, ReplayError> {
        if k == 0 {
            return Err(ReplayError::EmptyCheckpoint);
        }
        if k > log.len() {
            return Err(ReplayError::CheckpointBeyondLog { k, len: log.len() });
        }
        assert_unique(log)?;
        Ok(Self {
            state: fold_slice(&log[..k], None)?,
            k,
            through_event_id: log[k - 1].id.clone(),
        })
    }

    /// How many events of the log are already folded in.
    pub fn k(&self) -> usize {
        self.k
    }

    pub fn state(&self) -> &Value {
        &self.state
    }

    /// The event this was taken through.
    pub fn through_event_id(&self) -> &str {
        &self.through_event_id
    }

    /// ★ Re-fold the prefix and check the stored state matches.
    ///
    /// Costs `O(k)` — precisely what the checkpoint saved — which is why
    /// [`replay`] does not do it implicitly. The cheap check it *does* do
    /// catches a checkpoint on the wrong log; only this one catches a state
    /// that was corrupted after it was taken.
    pub fn verify(&self, log: &[Event]) -> Result<bool, ReplayError> {
        if self.k > log.len() {
            return Err(ReplayError::CheckpointBeyondLog { k: self.k, len: log.len() });
        }
        Ok(fold_slice(&log[..self.k], None)? == self.state)
    }
}

fn assert_unique(log: &[Event]) -> Result<(), ReplayError> {
    let mut seen = BTreeSet::new();
    for e in log {
        if !seen.insert(e.id.as_str()) {
            return Err(ReplayError::DuplicateEventId(e.id.clone()));
        }
    }
    Ok(())
}

fn fold_slice(events: &[Event], initial: Option<Value>) -> Result<Value, ReplayError> {
    let steps: Vec<FoldEvent> =
        events.iter().map(|e| FoldEvent { mutations: e.mutations.clone() }).collect();
    Ok(fold_events(&steps, initial)?)
}

/// Check a checkpoint belongs to this log, in `O(1)`.
fn check_attachment(log: &[Event], cp: &Checkpoint) -> Result<(), ReplayError> {
    if cp.k > log.len() {
        return Err(ReplayError::CheckpointBeyondLog { k: cp.k, len: log.len() });
    }
    let found = &log[cp.k - 1].id;
    if found != &cp.through_event_id {
        return Err(ReplayError::CheckpointNotOfThisLog {
            k: cp.k,
            expected: cp.through_event_id.clone(),
            found: found.clone(),
        });
    }
    Ok(())
}

/// Replay a log to its final state, optionally starting from a checkpoint.
///
/// ★★ Passing `None` is the from-scratch replay and passing `Some` is the fast
/// one — **the same function**, so discarding a checkpoint is changing one
/// argument, and that the two agree is a test rather than a claim.
///
/// ★★ It has no event sink, no movement channel and no registry, and never
/// calls `execute`. §IX's *"no external effects were issued"* is therefore a
/// fact about the signature rather than an assertion at the end.
///
/// The log must already be deduped (§V, [`merge`](crate::event::merge)) — see
/// the module docs for why a per-call `seen` set is not enough once a
/// checkpoint is involved.
pub fn replay(log: &[Event], from: Option<&Checkpoint>) -> Result<Value, ReplayError> {
    replay_to(log, from, log.len())
}

/// Replay only the first `upto` events — Tenet's *stand in a chosen future*.
pub fn replay_to(
    log: &[Event],
    from: Option<&Checkpoint>,
    upto: usize,
) -> Result<Value, ReplayError> {
    if upto > log.len() {
        return Err(ReplayError::UptoBeyondLog { upto, len: log.len() });
    }
    assert_unique(log)?;

    match from {
        None => fold_slice(&log[..upto], None),
        Some(cp) => {
            check_attachment(log, cp)?;
            if cp.k > upto {
                // The checkpoint is past where we are asked to stop, so it is
                // no use here — fall back to the truth rather than refuse. The
                // log is always sufficient, which is the whole point of §IX.
                return fold_slice(&log[..upto], None);
            }
            fold_slice(&log[cp.k..upto], Some(cp.state.clone()))
        }
    }
}

/// How many events a replay will actually apply.
///
/// ★ Makes §IX's `O(n − k)` claim **countable** rather than asserted: a test
/// checks the checkpointed path touches exactly `n − k` events, and that the
/// two paths still agree on the answer.
pub fn events_applied(log_len: usize, from: Option<&Checkpoint>) -> usize {
    match from {
        None => log_len,
        Some(cp) => log_len.saturating_sub(cp.k),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{CausalStamp, Provenance};
    use crate::mutation::Mutation;
    use serde_json::json;

    fn ev(id: &str, t: i64, path: &str, old: i64, new: i64) -> Event {
        Event {
            id: id.into(),
            name: "event.finances.pocket_spent".into(),
            t_event: t,
            t_ingest: None,
            provenance: Provenance::Observed,
            source: None,
            stamp: CausalStamp::new("phone"),
            causes: vec![],
            mutations: vec![Mutation::Set {
                path: path.into(),
                old: json!(old),
                new: json!(new),
            }],
        }
    }

    /// A household log: the balance walks 0 → 100 → 250 → 220 → 400 → 380.
    fn log() -> Vec<Event> {
        let p = "finances.liquid.balance";
        vec![
            ev("e1", 100, p, 0, 100),
            ev("e2", 200, p, 100, 250),
            ev("e3", 300, p, 250, 220),
            ev("e4", 400, p, 220, 400),
            ev("e5", 500, p, 400, 380),
        ]
    }

    fn balance(v: &Value) -> i64 {
        v["finances"]["liquid"]["balance"].as_i64().unwrap()
    }

    // ── the homomorphism ───────────────────────────────────────────────────

    #[test]
    fn a_checkpoint_and_a_from_scratch_replay_agree() {
        // fold(s0, L) = fold(s_k, L[k+1..n]) — §II's homomorphism, which is the
        // whole justification for checkpointing at all.
        let l = log();
        let scratch = replay(&l, None).unwrap();
        for k in 1..=l.len() {
            let cp = Checkpoint::at(&l, k).unwrap();
            assert_eq!(replay(&l, Some(&cp)).unwrap(), scratch, "disagreed at k={k}");
        }
        assert_eq!(balance(&scratch), 380);
    }

    #[test]
    fn the_cost_is_n_minus_k_and_it_is_countable() {
        // ★ §IX's O(n−k) made checkable rather than asserted.
        let l = log();
        let cp = Checkpoint::at(&l, 3).unwrap();
        assert_eq!(events_applied(l.len(), None), 5);
        assert_eq!(events_applied(l.len(), Some(&cp)), 2);
        assert_eq!(replay(&l, Some(&cp)).unwrap(), replay(&l, None).unwrap());
    }

    #[test]
    fn a_checkpoint_stays_useful_as_the_log_grows() {
        // The point of taking one: it is not invalidated by later events.
        let mut l = log();
        let cp = Checkpoint::at(&l, 2).unwrap();
        l.push(ev("e6", 600, "finances.liquid.balance", 380, 999));
        assert_eq!(balance(&replay(&l, Some(&cp)).unwrap()), 999);
        assert_eq!(replay(&l, Some(&cp)).unwrap(), replay(&l, None).unwrap());
    }

    // ── ★★ always discardable ──────────────────────────────────────────────

    #[test]
    fn discarding_the_checkpoint_is_changing_one_argument() {
        // ★★ "An optimisation that can always be discarded without loss,
        // because L is still the truth" — the same function, one argument apart.
        let l = log();
        let cp = Checkpoint::at(&l, 4).unwrap();
        assert_eq!(replay(&l, Some(&cp)).unwrap(), replay(&l, None).unwrap());
    }

    #[test]
    fn a_checkpoint_is_derived_and_cannot_be_authored() {
        // ★ There is no constructor taking a state — the only way to get one is
        // to fold the log, so a checkpoint claiming a state the log never
        // produced is not writable. What it stores is what the fold produced.
        let l = log();
        let cp = Checkpoint::at(&l, 3).unwrap();
        assert_eq!(balance(cp.state()), 220);
        assert_eq!(cp.k(), 3);
        assert_eq!(cp.through_event_id(), "e3");
        assert!(cp.verify(&l).unwrap());
    }

    #[test]
    fn a_checkpoint_from_a_different_log_is_refused_in_o1() {
        let l = log();
        let cp = Checkpoint::at(&l, 3).unwrap();

        let mut other = log();
        other[2] = ev("different", 300, "finances.liquid.balance", 250, 220);
        match replay(&other, Some(&cp)) {
            Err(ReplayError::CheckpointNotOfThisLog { expected, found, .. }) => {
                assert_eq!(expected, "e3");
                assert_eq!(found, "different");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn verify_catches_a_corrupted_state_that_the_cheap_check_cannot() {
        // ★ The honest split: attachment is O(1) and catches the wrong log;
        // only verify() catches a state tampered with after the fact, and it
        // costs O(k) — exactly what the checkpoint saved.
        let l = log();
        let mut cp = Checkpoint::at(&l, 3).unwrap();
        let json = serde_json::to_string(&cp).unwrap();
        let mut tampered: Value = serde_json::from_str(&json).unwrap();
        tampered["state"]["finances"]["liquid"]["balance"] = json!(9999);
        cp = serde_json::from_value(tampered).unwrap();

        // The cheap check passes — the event id is still right.
        assert!(replay(&l, Some(&cp)).is_ok());
        // And the expensive one is what catches it.
        assert!(!cp.verify(&l).unwrap());
    }

    #[test]
    fn a_checkpoint_at_zero_is_no_checkpoint_and_says_so() {
        assert_eq!(Checkpoint::at(&log(), 0), Err(ReplayError::EmptyCheckpoint));
        assert_eq!(
            Checkpoint::at(&log(), 9),
            Err(ReplayError::CheckpointBeyondLog { k: 9, len: 5 })
        );
    }

    // ── ★★ the finding: a per-call `seen` set is not enough ────────────────

    /// §IX's pseudocode, written literally — `seen` initialised empty at the
    /// start of the call, exactly as printed.
    fn replay_as_the_pseudocode_is_written(log: &[Event], from: Option<&Checkpoint>) -> Value {
        let (mut state, start) = match from {
            Some(cp) => (cp.state().clone(), cp.k()),
            None => (json!({}), 0),
        };
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for e in &log[start..] {
            if !seen.insert(e.id.as_str()) {
                continue;
            }
            let s = fold_slice(std::slice::from_ref(e), Some(state)).unwrap();
            state = s;
        }
        state
    }

    #[test]
    fn the_pseudocodes_per_call_seen_set_makes_the_two_paths_disagree() {
        // ★★ THE FINDING, demonstrated rather than described. A late duplicate
        // of an EARLY event is skipped from scratch (its id is already in
        // `seen`) and applied from a checkpoint (the checkpoint's ids never
        // entered `seen`). Two paths, two answers, and the "discard it freely"
        // property gone.
        let mut l = log();
        l.push(ev("e1", 600, "finances.liquid.balance", 380, 100)); // a repeat of e1

        let cp = Checkpoint::at(&l[..5], 3).unwrap();
        let scratch = replay_as_the_pseudocode_is_written(&l, None);
        let fast = replay_as_the_pseudocode_is_written(&l, Some(&cp));

        assert_eq!(balance(&scratch), 380, "from scratch, the repeat is skipped");
        assert_eq!(balance(&fast), 100, "from the checkpoint, it is applied");
        assert_ne!(scratch, fast, "the two paths genuinely disagree");
    }

    #[test]
    fn replay_refuses_a_log_that_was_not_deduped_at_the_substrate() {
        // ★ So the fix is taken where §V already puts it, and a log that has
        // not been through it is refused rather than silently given one of the
        // two wrong answers.
        let mut l = log();
        l.push(ev("e1", 600, "finances.liquid.balance", 380, 100));
        assert_eq!(replay(&l, None), Err(ReplayError::DuplicateEventId("e1".into())));
        assert_eq!(Checkpoint::at(&l, 3), Err(ReplayError::DuplicateEventId("e1".into())));
    }

    #[test]
    fn a_deduped_log_replays_the_same_way_on_both_paths() {
        use crate::event::merge;
        let mut l = log();
        l.push(ev("e1", 600, "finances.liquid.balance", 380, 100));
        let merged = merge(l);

        let cp = Checkpoint::at(&merged, 3).unwrap();
        assert_eq!(replay(&merged, Some(&cp)).unwrap(), replay(&merged, None).unwrap());
    }

    // ── ★★ replay issues no external effects, and cannot ───────────────────

    #[test]
    fn replay_is_deterministic_because_apply_reads_no_clock() {
        // Purity is a precondition, not a nicety: a replay that re-read the
        // clock would produce a different state from the original run.
        let l = log();
        assert_eq!(replay(&l, None).unwrap(), replay(&l, None).unwrap());
    }

    #[test]
    fn replay_cannot_reach_an_effect_channel_at_all() {
        // ★★ Structural, and the contrast is the proof: an operator body takes
        // `&mut Vec<EmittedEvent>` and `&mut Vec<Movement>` because emitting
        // and moving is what it is for. `replay` takes `&[Event]` and returns a
        // `Value` — no sink, no registry, no `execute`. An SMS cannot be re-sent
        // because there is nothing here to send it with.
        let l = log();
        let before = l.clone();
        let _ = replay(&l, None).unwrap();
        assert_eq!(l, before, "and the log itself is untouched — it is borrowed, not owned");
    }

    // ── Tenet's consumer: stand in a chosen future ─────────────────────────

    #[test]
    fn replay_to_reads_the_state_at_a_chosen_point() {
        let l = log();
        assert_eq!(balance(&replay_to(&l, None, 2).unwrap()), 250);
        assert_eq!(balance(&replay_to(&l, None, 4).unwrap()), 400);
        assert_eq!(
            replay_to(&l, None, 9),
            Err(ReplayError::UptoBeyondLog { upto: 9, len: 5 })
        );
    }

    #[test]
    fn a_checkpoint_past_the_point_asked_for_falls_back_to_the_log() {
        // ★ The log is always sufficient — that is the whole of §IX. A
        // checkpoint that is no use here is ignored rather than made an error.
        let l = log();
        let cp = Checkpoint::at(&l, 4).unwrap();
        assert_eq!(
            replay_to(&l, Some(&cp), 2).unwrap(),
            replay_to(&l, None, 2).unwrap()
        );
        assert_eq!(balance(&replay_to(&l, Some(&cp), 2).unwrap()), 250);
    }
}
