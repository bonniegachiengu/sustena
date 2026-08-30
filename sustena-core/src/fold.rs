//! The fold — `state = fold(events)`.
//!
//! A sustain's state is the replay of everything that ever happened to it, not
//! a blob edited in place (CELL / the Sustain paper). This module is that
//! reducer: pure, storage-free, and identical whether it is rebuilding from
//! scratch or keeping a cache in step.
//!
//! Each event carries the mutation records produced while the operator that
//! published it ran. Replaying them cannot drift from what the operator did,
//! because it *is* what the operator did — there is no second "what should
//! this event mean" implementation to keep in sync with every operator.
//!
//! The honest limit, same as the reference engine: this reproduces state, not
//! domain intent. A mutation is an opaque patch, so you cannot yet ask what
//! state would look like if an event had meant something else. Richer
//! domain-typed replay can layer on later without changing the stored shape.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{FoldError, FoldResult};
use crate::mutation::Mutation;
use crate::state::State;

/// One event as the fold sees it. Ordering is the caller's job — the engine
/// queries `ORDER BY seq ASC`; this replays exactly what it is handed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldEvent {
    #[serde(default)]
    pub mutations: Vec<Mutation>,
}

/// Apply one mutation, returning the state to continue folding with.
pub fn apply_mutation(state: &mut State, mutation: &Mutation) -> FoldResult<()> {
    match mutation {
        Mutation::ReplaceRoot { value } => {
            // ★★★ **Joined, not replaced — Multiparty §VI.** A root that lands
            //     on an existing one used to erase it, which is exactly the
            //     overwrite a join-semilattice forbids. It only ever mattered
            //     once two nodes each created the same Sustain before they met:
            //     two genesis lines in one log, and the later one wiping the
            //     history folded before it.
            //
            // ★★ On a fresh fold the state is empty, so the join IS the
            //    replacement and nothing about a single-history household
            //    changes. On a merged log it is `s ⊔ s = s` for two genesis
            //    states of the same template — which is the case that was
            //    broken — and a real least upper bound otherwise.
            let (joined, disagreements) =
                crate::root_join::join_roots(&state.snapshot(), value);
            state.replace_root(joined);
            if !disagreements.is_empty() {
                // ★ Reported, never resolved by fiat: picking one of two
                //   unordered values would be an overwrite with better manners,
                //   and whoever synced last would win.
                for d in &disagreements {
                    let at = if d.path.is_empty() { "the root" } else { &d.path };
                    eprintln!(
                        "[fold] two histories disagree at {at}: {} vs {}",
                        d.left, d.right
                    );
                }
            }
            Ok(())
        }
        Mutation::Set { path, new, .. } => {
            state.set(path, new.clone()).map_err(|e| {
                FoldError::Unreplayable(format!("failed to replay mutation {mutation:?}: {e}"))
            })
        }
        Mutation::Append { path, item, item_id } => state
            .append(path, item.clone(), item_id)
            .map(|_| ())
            .map_err(|e| {
                FoldError::Unreplayable(format!("failed to replay mutation {mutation:?}: {e}"))
            }),
        Mutation::Remove { path, item_id } => state.remove(path, item_id).map_err(|e| {
            FoldError::Unreplayable(format!("failed to replay mutation {mutation:?}: {e}"))
        }),
    }
}

/// Fold an ordered list of events into a state value.
pub fn fold_events(events: &[FoldEvent], initial: Option<Value>) -> FoldResult<Value> {
    let mut state = State::new(initial.unwrap_or_else(|| Value::Object(Map::new())));
    for event in events {
        for mutation in &event.mutations {
            apply_mutation(&mut state, mutation)?;
        }
    }
    Ok(state.snapshot())
}

/// The fold's inverse: the mutations that transform `before` into `after`.
///
/// Kept even though this engine cannot produce an unrecorded change (see
/// `state.rs`), because it is genuinely needed elsewhere — verifying that a
/// log reproduces a state, and diffing two states for the simulator.
///
/// Emits the narrowest correct patch, widening to a whole-value `set` (or
/// `replace_root`) whenever a narrower one could not faithfully reproduce the
/// result: a removed key, which `set` cannot express; a key outside the
/// `\w+` path grammar, which cannot be addressed; and any list change, since
/// the vocabulary has no index semantics. Correctness first, minimality second.
pub fn diff_to_mutations(before: &Value, after: &Value, path: &str) -> Vec<Mutation> {
    if before == after {
        return Vec::new();
    }

    if let (Value::Object(b), Value::Object(a)) = (before, after) {
        let removed = b.keys().any(|k| !a.contains_key(k));
        let unsafe_key = a.keys().any(|k| !is_addressable(k));

        if removed || unsafe_key {
            return vec![widen(before, after, path)];
        }

        let mut out = Vec::new();
        for (key, new_val) in a {
            let child = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            match b.get(key) {
                None => out.push(Mutation::Set {
                    path: child,
                    old: Value::Null,
                    new: new_val.clone(),
                }),
                Some(old_val) => out.extend(diff_to_mutations(old_val, new_val, &child)),
            }
        }
        return out;
    }

    vec![widen(before, after, path)]
}

fn widen(before: &Value, after: &Value, path: &str) -> Mutation {
    if path.is_empty() {
        Mutation::ReplaceRoot { value: after.clone() }
    } else {
        Mutation::Set {
            path: path.to_string(),
            old: before.clone(),
            new: after.clone(),
        }
    }
}

fn is_addressable(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roundtrip(before: Value, after: Value) -> Value {
        let muts = diff_to_mutations(&before, &after, "");
        fold_events(&[FoldEvent { mutations: muts }], Some(before)).unwrap()
    }

    #[test]
    fn identical_states_need_nothing() {
        assert!(diff_to_mutations(&json!({"a":1}), &json!({"a":1}), "").is_empty());
    }

    #[test]
    fn nested_change_is_addressed_precisely() {
        let muts = diff_to_mutations(
            &json!({"finances":{"pockets":{"food":{"spent":0}}}}),
            &json!({"finances":{"pockets":{"food":{"spent":250}}}}),
            "",
        );
        assert_eq!(muts.len(), 1);
        assert_eq!(muts[0].path(), Some("finances.pockets.food.spent"));
    }

    #[test]
    fn widens_where_set_cannot_express_the_change() {
        // removed key
        assert_eq!(roundtrip(json!({"x":{"k1":1,"k2":2}}), json!({"x":{"k1":1}})), json!({"x":{"k1":1}}));
        // key outside the path grammar
        assert_eq!(
            roundtrip(json!({"p":{"holiday fund":0}}), json!({"p":{"holiday fund":50}})),
            json!({"p":{"holiday fund":50}})
        );
        // list change
        assert_eq!(roundtrip(json!({"i":[1,2]}), json!({"i":[1,2,3]})), json!({"i":[1,2,3]}));
    }

    #[test]
    fn a_root_is_joined_onto_what_came_before_rather_than_discarding_it() {
        // ★★★ **This test used to assert the opposite**, and asserting it was
        //     the bug: a `ReplaceRoot` that discards is an overwrite, and
        //     Multiparty §VI is explicit that a merge takes the least upper
        //     bound and never overwrites -- "the holon invariant, never erase
        //     the node, is not a policy sitting on top of the merge, it IS the
        //     merge." Discarding only looked harmless while a Sustain had one
        //     history. Two nodes that each created the same household before
        //     they met put two genesis lines in one log, and the later one
        //     erased everything folded before it.
        let out = fold_events(
            &[FoldEvent { mutations: vec![Mutation::ReplaceRoot { value: json!({"a":1}) }] }],
            Some(json!({"stale":true})),
        )
        .unwrap();
        assert_eq!(out, json!({"a": 1, "stale": true}), "both survive the join");
    }

    #[test]
    fn a_fresh_fold_is_unchanged_by_the_join() {
        // ★★ The join must not alter what a single-history household folds to.
        //    On a fresh fold the state is empty, so the join IS the
        //    replacement.
        let genesis = json!({"finances": {"liquid": {"balance": 0.0}}});
        let out = fold_events(
            &[FoldEvent { mutations: vec![Mutation::ReplaceRoot { value: genesis.clone() }] }],
            None,
        )
        .unwrap();
        assert_eq!(out, genesis);
    }

    #[test]
    fn twin_genesis_in_one_log_converges_and_keeps_the_history_between_them() {
        // ★★★ The whole point, end to end through the fold: two genesis lines
        //     for one Sustain, with real history folded in between, and the
        //     second one erases none of it.
        let genesis = json!({"finances": {"liquid": {"balance": 0.0}}, "pockets": {}});
        let out = fold_events(
            &[
                FoldEvent {
                    mutations: vec![Mutation::ReplaceRoot { value: genesis.clone() }],
                },
                FoldEvent {
                    mutations: vec![Mutation::Set {
                        path: "finances.liquid.balance".into(),
                        old: json!(0.0),
                        new: json!(516_699.48),
                    }],
                },
                // The other node's genesis, arriving by sync.
                FoldEvent { mutations: vec![Mutation::ReplaceRoot { value: genesis }] },
            ],
            None,
        )
        .unwrap();
        assert_eq!(
            out["finances"]["liquid"]["balance"],
            json!(516_699.48),
            "the household survived the twin"
        );
    }

    #[test]
    fn an_unreplayable_mutation_fails_loudly() {
        let err = fold_events(
            &[FoldEvent {
                mutations: vec![Mutation::Remove { path: "missing".into(), item_id: "x".into() }],
            }],
            None,
        );
        assert!(err.is_err(), "silently returning wrong state would defeat the fold");
    }
}
