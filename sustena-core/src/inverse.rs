//! Reversibility — declared and derived inverses (ENZYME / the Operator paper, §V).
//!
//! An enzyme `o` is **reversible** if there exists `o⁻¹ ∈ T` that undoes it —
//! with a second clause that is easy to forget and necessary: *an inverse whose
//! guard refuses at exactly the states `o` produces is not an inverse.*
//!
//! `T` is **not a group**. The reversible enzymes form a subset closed under
//! inversion; everything else is one-way. Membership requires the effect to be
//! injective: `increment(path, x)` is injective and trivially invertible;
//! `set(path, v)` is not, because it destroys the old value and the pre-image
//! is ambiguous.
//!
//! ## The substrate already solves half of it
//!
//! Every mutation is recorded as `{op, path, old, new}` — **the old value is
//! carried in the record**. So a *generic, patch-level inverse* exists for any
//! enzyme whose mutations are recorded: replay the mutation list backwards,
//! restoring `old` at each path.
//!
//! That is weaker than a declared semantic inverse (it undoes what *happened*,
//! not what was *meant*) and strictly stronger than snapshot-and-replace: it
//! touches only the paths that changed, it composes, and it is **derivable
//! rather than stored**.
//!
//! Derivable rather than stored is the point. The reference engine keeps
//! rollback snapshots in a module-level dictionary that is lost on restart
//! (backlog #20) — an entire mechanism for information the event log already
//! contains. Nothing here is stored; an inverse is computed from the record.
//!
//! ## What is honestly not invertible
//!
//! Two of the four mutation shapes cannot be inverted from the record alone,
//! and this module says so rather than guessing:
//!
//! - **`remove`** records the `item_id` but not the item. The value needed to
//!   put it back is not in the record.
//! - **`replace_root`** records the new root but not the previous one.
//! - **a `set` that CREATED a key** records `old: null`, which cannot be told
//!   apart from a key that genuinely held null. Restoring null would leave the
//!   key present where it had been absent — equivalent to every predicate, but
//!   not byte-identical, and byte-identity is what the fold promises. This one
//!   was found by testing rather than by reading, which is the usual way.
//!
//! Both are recoverable by folding the log from the beginning — which is a
//! different, more expensive operation, and one this module deliberately does
//! not pretend to be. Making them invertible in place would mean widening the
//! stored record, which touches the event storage model and therefore waits on
//! the same decision as backlog #17/#18.
//!
//! ## Why irreversibility is where the gates belong
//!
//! An irreversible step destroys the information required to undo it, so the
//! only moment a decision about it can be made at all is *before* it runs.
//! Where a move can be walked back the guard can afford to be permissive,
//! because the cost of admitting a wrong move is bounded by the walk-back.
//! Where it cannot, the guard is the only control that will ever exist over
//! that move.
//!
//! A declared inverse is a move in `T`, subject to the same gate as any other
//! move — not a side-door around it. That is what makes rollback a principled
//! operation rather than a rescue.

use crate::mutation::Mutation;

/// Why a mutation list cannot be inverted from the record alone.
#[derive(Debug, Clone, PartialEq)]
pub enum InverseError {
    /// `remove` carries the item's id, not the item.
    RemovedItemNotRecorded { path: String, item_id: String },
    /// `replace_root` carries the new root, not the previous one.
    PriorRootNotRecorded,
    /// A `set` that created a key records `old: null`, which is
    /// indistinguishable from a key that genuinely held null. Restoring null
    /// would leave the key PRESENT where it had been ABSENT — a state that is
    /// equivalent to every predicate but not byte-identical, and byte-identity
    /// is what the fold promises.
    PriorAbsenceAmbiguous { path: String },
}

impl std::fmt::Display for InverseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InverseError::RemovedItemNotRecorded { path, item_id } => write!(
                f,
                "cannot invert the removal of '{item_id}' from '{path}': the record carries the \
                 id but not the item, so there is nothing to put back. Rebuild from the log \
                 instead."
            ),
            InverseError::PriorRootNotRecorded => write!(
                f,
                "cannot invert a replace_root: the record carries the new root but not the \
                 previous one. Rebuild from the log instead."
            ),
            InverseError::PriorAbsenceAmbiguous { path } => write!(
                f,
                "cannot invert the set at '{path}': it recorded old=null, which cannot be told                  apart from a key that genuinely held null. Restoring null would leave the key                  present where it had been absent. Rebuild from the log instead."
            ),
        }
    }
}

/// The patch-level inverse of a mutation list.
///
/// Replays backwards, restoring `old` at each path. Applying the result to the
/// state those mutations produced returns the state they started from.
///
/// Derived, not stored — and it touches only the paths that actually changed,
/// which a snapshot restore cannot claim.
pub fn invert(mutations: &[Mutation]) -> Result<Vec<Mutation>, InverseError> {
    let mut out = Vec::with_capacity(mutations.len());

    // Backwards: the last change must be undone first, or an earlier undo
    // would be overwritten by a later one it was supposed to precede.
    for m in mutations.iter().rev() {
        out.push(match m {
            Mutation::Set { path, old, new } => {
                // The paper is explicit that `set` is not injective: it
                // destroys the old value. Where the old value was recorded,
                // the patch-level inverse recovers it. Where the record says
                // `null`, it cannot distinguish "this key did not exist" from
                // "this key held null" — and those undo differently.
                if old.is_null() {
                    return Err(InverseError::PriorAbsenceAmbiguous { path: path.clone() });
                }
                Mutation::Set {
                    path: path.clone(),
                    old: new.clone(),
                    new: old.clone(),
                }
            }
            Mutation::Append { path, item_id, .. } => Mutation::Remove {
                path: path.clone(),
                item_id: item_id.clone(),
            },
            Mutation::Remove { path, item_id } => {
                return Err(InverseError::RemovedItemNotRecorded {
                    path: path.clone(),
                    item_id: item_id.clone(),
                })
            }
            Mutation::ReplaceRoot { .. } => return Err(InverseError::PriorRootNotRecorded),
        });
    }

    Ok(out)
}

/// Can this mutation list be inverted from the record alone?
///
/// Worth asking BEFORE committing an operator, not after: irreversibility is
/// exactly the case where the guard is the only control that will ever exist.
pub fn is_reversible(mutations: &[Mutation]) -> bool {
    invert(mutations).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::{fold_events, FoldEvent};
    use crate::state::State;
    use serde_json::json;

    fn roundtrip(initial: serde_json::Value, mutations: Vec<Mutation>) -> serde_json::Value {
        let forward =
            fold_events(&[FoldEvent { mutations: mutations.clone() }], Some(initial)).unwrap();
        let back = invert(&mutations).expect("should invert");
        fold_events(&[FoldEvent { mutations: back }], Some(forward)).unwrap()
    }

    #[test]
    fn a_set_is_undone_by_restoring_the_old_value() {
        let start = json!({"a": 1});
        let mut s = State::new(start.clone());
        s.set("a", json!(2)).unwrap();
        assert_eq!(roundtrip(start.clone(), s.mutations().to_vec()), start);
    }

    #[test]
    fn an_increment_is_undone_exactly() {
        let start = json!({"finances": {"liquid": {"balance": 100.0}}});
        let mut s = State::new(start.clone());
        s.increment("finances.liquid.balance", &json!(250.0)).unwrap();
        assert_eq!(roundtrip(start.clone(), s.mutations().to_vec()), start);
    }

    #[test]
    fn an_append_is_undone_by_removing_what_was_appended() {
        let start = json!({"items": []});
        let mut s = State::new(start.clone());
        s.append("items", json!({"k": 1}), "i1").unwrap();
        assert_eq!(roundtrip(start.clone(), s.mutations().to_vec()), start);
    }

    #[test]
    fn a_multi_step_operator_is_undone_in_reverse_order() {
        // Order matters: undoing forwards would let an earlier restore be
        // overwritten by a later one it was supposed to precede.
        let start = json!({"finances": {"liquid": {"balance": 1000.0},
                                        "pockets": {"food": {"allocated": 0.0, "limit": 0.0}}}});
        let mut s = State::new(start.clone());
        s.decrement("finances.liquid.balance", &json!(250.0), false).unwrap();
        s.increment("finances.pockets.food.allocated", &json!(250.0)).unwrap();
        s.set("finances.pockets.food.limit", json!(500.0)).unwrap();

        assert_eq!(roundtrip(start.clone(), s.mutations().to_vec()), start);
    }

    #[test]
    fn creating_a_key_is_honestly_not_invertible() {
        // `set` on an absent key records old=null, which cannot be told apart
        // from a key that held null. Restoring null would leave it PRESENT
        // where it had been ABSENT — equivalent to every predicate, but not
        // byte-identical, and byte-identity is what the fold promises.
        let mut s = State::new(json!({"finances": {}}));
        s.set("finances.new_pocket", json!({"allocated": 0.0})).unwrap();
        match invert(s.mutations()) {
            Err(InverseError::PriorAbsenceAmbiguous { path }) => {
                assert_eq!(path, "finances.new_pocket");
            }
            other => panic!("expected an honest refusal, got {other:?}"),
        }
    }

    #[test]
    fn it_touches_only_the_paths_that_changed() {
        // The claim that makes this stronger than snapshot-and-replace.
        let start = json!({"a": 1, "untouched": {"deep": [1, 2, 3]}});
        let mut s = State::new(start.clone());
        s.set("a", json!(2)).unwrap();
        let back = invert(s.mutations()).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].path(), Some("a"));
    }

    #[test]
    fn a_removal_is_honestly_not_invertible_from_the_record() {
        let start = json!({"items": [{"id": "i1", "k": 1}]});
        let mut s = State::new(start);
        s.remove("items", "i1").unwrap();
        match invert(s.mutations()) {
            Err(InverseError::RemovedItemNotRecorded { item_id, .. }) => {
                assert_eq!(item_id, "i1");
            }
            other => panic!("expected an honest refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_root_replacement_is_honestly_not_invertible_from_the_record() {
        let muts = vec![Mutation::ReplaceRoot { value: json!({"a": 1}) }];
        assert_eq!(invert(&muts), Err(InverseError::PriorRootNotRecorded));
    }

    #[test]
    fn reversibility_can_be_asked_before_committing() {
        let mut s = State::new(json!({"a": 1, "items": [{"id": "x"}]}));
        s.set("a", json!(2)).unwrap();
        assert!(is_reversible(s.mutations()));

        s.remove("items", "x").unwrap();
        assert!(!is_reversible(s.mutations()), "the guard is now the only control there is");
    }

    #[test]
    fn nothing_is_stored_to_make_this_work() {
        // The inverse is derived from the record. No snapshot registry, and so
        // nothing to lose on restart.
        let start = json!({"a": 1});
        let mut s = State::new(start.clone());
        s.set("a", json!(99)).unwrap();
        let recorded = s.mutations().to_vec();

        // Only the recorded mutations are needed — the State is dropped here.
        drop(s);
        let back = invert(&recorded).unwrap();
        let restored = fold_events(&[FoldEvent { mutations: back }], Some(json!({"a": 99}))).unwrap();
        assert_eq!(restored, start);
    }
}
