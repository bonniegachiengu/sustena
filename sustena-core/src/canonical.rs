//! **`H(s) = hash(canonical(s))`** — verification as a comparison (CELL §IX).
//!
//! ★★★ **`fold(log) == cache` is the law the whole engine rests on, and until
//! now checking it meant comparing two whole state trees.** That is fine for a
//! test and useless everywhere else: across a peer link you cannot send a
//! household's entire finances to ask *are we the same?*, and a node that
//! answers *mostly* has answered nothing. A hash turns the question into one
//! short string, which is what makes it askable at all — on a phone, over a
//! socket, in a log line.
//!
//! ## Why the canonical form has to exist first
//!
//! ★★★ **This crate builds its `Map` with `preserve_order`.** Two states that
//! are equal in every value can therefore serialize to different bytes purely
//! because somebody's keys arrived in a different order — and a hash over
//! *that* would report a divergence between a household and itself. Sorting
//! keys is not tidiness; it is the difference between a hash that means
//! something and one that raises a false alarm every time a fold rebuilds.
//!
//! ★★★ **Numbers are normalised the way the rest of the engine already
//! normalises them.** `predicate::ast::num` turns a whole float into an
//! integer, because `1.0` and `1` are one number and the engine says so
//! everywhere else. If canonicalisation disagreed, a cache written before that
//! rule and a fold replayed after it would hash differently while being the
//! same household — the alarm would fire on the representation, not the
//! substance.
//!
//! ★★ **Array order is preserved, because in a list order IS the value.**
//! Sorting a list of events, or of journal lines, would make two genuinely
//! different histories hash alike. Objects have no order to lose; arrays do.
//!
//! ## The dependency, named rather than slipped in
//!
//! ★★ `sha2` is the first hashing dependency this crate takes, and the reason
//! is peers. A non-cryptographic hash is fine when both sides are trying to
//! agree; this one is compared across nodes that need not trust each other, and
//! a hash somebody can collide on purpose is a verification that verifies
//! nothing. The host already links it, so nothing new reaches a binary.

use std::fmt::Write as _;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// One value, written the same way every time.
///
/// ★★ Not pretty-printed and not meant to be read. It exists to be hashed, and
/// every byte of prettiness would be a byte that could differ for no reason.
pub fn canonical(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(_) => {
            // ★★★ Through the engine's OWN number rule. A second opinion here
            //     would make a cache and its own rebuild hash differently.
            let normalised = value
                .as_f64()
                .map(crate::predicate::ast::num)
                .unwrap_or_else(|| value.clone());
            out.push_str(&normalised.to_string());
        }
        Value::String(s) => {
            // serde's own escaping, so the canonical form of a string is the
            // one every other part of this crate would have written.
            out.push_str(&Value::String(s.clone()).to_string());
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            // ★★★ Sorted. `preserve_order` means insertion order reaches here,
            //     and hashing that would report a household as diverged from
            //     itself because two keys arrived in a different order.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String((*k).clone()).to_string());
                out.push(':');
                write_canonical(&map[*k], out);
            }
            out.push('}');
        }
    }
}

/// **`H(s)`** — a state's identity, short enough to send.
pub fn state_hash(value: &Value) -> String {
    let digest = Sha256::digest(canonical(value).as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// What a comparison found.
///
/// ★★ Named rather than a bare `bool`, because *the same* and *different* call
/// for different things from whoever asked, and a caller that has to remember
/// which way round `true` meant will one day get it backwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agreement {
    /// Byte-for-byte the same state.
    Same { hash: String },
    /// Genuinely different, and each side says what it holds.
    Diverged { ours: String, theirs: String },
}

impl Agreement {
    pub fn is_same(&self) -> bool {
        matches!(self, Self::Same { .. })
    }
}

/// **Does this state agree with a hash somebody else sent?**
///
/// ★★★ The whole point of the row: verification as a comparison of two short
/// strings rather than of two whole households.
pub fn agrees_with(state: &Value, theirs: &str) -> Agreement {
    let ours = state_hash(state);
    if ours == theirs {
        Agreement::Same { hash: ours }
    } else {
        Agreement::Diverged { ours, theirs: theirs.to_string() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_same_state_written_two_ways_has_one_hash() {
        // ★★★ `preserve_order` means insertion order reaches the serializer.
        //     Without sorting, a household would report as diverged from itself
        //     because two keys arrived in a different order.
        let a = json!({"finances": {"liquid": 100, "savings": 5}, "notes": "x"});
        let b = json!({"notes": "x", "finances": {"savings": 5, "liquid": 100}});
        assert_eq!(state_hash(&a), state_hash(&b));
    }

    #[test]
    fn a_whole_float_and_its_integer_are_one_number() {
        // ★★★ The engine already says so everywhere else. If this disagreed, a
        //     cache written before the rule and a fold replayed after it would
        //     hash differently while being the same household.
        assert_eq!(state_hash(&json!({"n": 1.0})), state_hash(&json!({"n": 1})));
        assert_eq!(canonical(&json!(1000.0)), "1000");
    }

    #[test]
    fn a_real_difference_is_a_different_hash() {
        assert_ne!(state_hash(&json!({"n": 1})), state_hash(&json!({"n": 2})));
        assert_ne!(state_hash(&json!({"a": 1})), state_hash(&json!({"b": 1})));
    }

    #[test]
    fn order_in_a_list_is_part_of_the_value() {
        // ★★ Sorting arrays would make two genuinely different histories hash
        //    alike. Objects have no order to lose; lists do.
        assert_ne!(state_hash(&json!([1, 2])), state_hash(&json!([2, 1])));
    }

    #[test]
    fn a_missing_field_and_a_null_one_are_not_the_same_state() {
        // ★★ "We never recorded this" and "we recorded that there is none" are
        //    different facts, and a hash that conflated them would hide it.
        assert_ne!(state_hash(&json!({"a": 1})), state_hash(&json!({"a": 1, "b": null})));
    }

    #[test]
    fn nesting_does_not_escape_the_rule() {
        let a = json!({"x": {"deep": {"b": 2, "a": 1}}});
        let b = json!({"x": {"deep": {"a": 1, "b": 2}}});
        assert_eq!(state_hash(&a), state_hash(&b));
    }

    #[test]
    fn a_hash_is_short_enough_to_send() {
        // ★★★ The row's actual purpose. Across a peer link you cannot send a
        //     household's entire finances to ask "are we the same?".
        let big = json!({"finances": {"pockets": (0..500)
            .map(|i| (format!("p{i}"), json!({"allocated": i, "spent": 0})))
            .collect::<serde_json::Map<_, _>>()}});
        assert_eq!(state_hash(&big).len(), 64);
    }

    #[test]
    fn agreement_says_which_way_round_it_went() {
        // ★★ A bare bool is a thing somebody eventually reads backwards.
        let s = json!({"n": 1});
        let same = agrees_with(&s, &state_hash(&s));
        assert!(same.is_same());

        let differs = agrees_with(&s, "0000");
        assert!(!differs.is_same());
        match differs {
            Agreement::Diverged { ours, theirs } => {
                assert_eq!(ours, state_hash(&s));
                assert_eq!(theirs, "0000");
            }
            Agreement::Same { .. } => panic!("these are not the same"),
        }
    }

    #[test]
    fn a_folded_state_and_the_cache_it_was_folded_into_agree() {
        // ★★★ `fold(log) == cache` — the law the whole engine rests on — as a
        //     comparison of two short strings rather than two whole trees.
        use crate::fold::{fold_events, FoldEvent};
        use crate::mutation::Mutation;

        let genesis = FoldEvent {
            mutations: vec![Mutation::ReplaceRoot {
                value: json!({"finances": {"liquid": {"balance": 0.0}}}),
            }],
        };
        let income = FoldEvent {
            mutations: vec![Mutation::Set {
                path: "finances.liquid.balance".into(),
                old: json!(0.0),
                new: json!(1500.0),
            }],
        };
        let rebuilt = fold_events(&[genesis, income], None).expect("folds");
        let cache = json!({"finances": {"liquid": {"balance": 1500}}});
        assert_eq!(state_hash(&rebuilt), state_hash(&cache));
    }

    #[test]
    fn canonical_form_carries_no_whitespace_to_differ_over() {
        assert_eq!(canonical(&json!({"a": [1, 2]})), r#"{"a":[1,2]}"#);
    }
}
