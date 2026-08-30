//! The opening state as a join, not an overwrite — Multiparty §VI.
//!
//! ★★★ **What was wrong.** Every Sustain's log opens with one `ReplaceRoot`
//! carrying the template's default state, and the fold applied it by
//! *replacing* the root. That is fine while a Sustain has one history. It stops
//! being fine the moment two nodes each created the same Sustain before they
//! met — a household set up on a laptop and on a phone — because then two
//! genesis lines meet in one log and the later one erases everything folded
//! before it.
//!
//! §VI does not leave this open. Shared state is a join-semilattice and merges
//! take the least upper bound:
//!
//! $$s_i \sqcup s_j = s_j \sqcup s_i, \quad (s_i\sqcup s_j)\sqcup s_k =
//!   s_i\sqcup(s_j\sqcup s_k), \quad s\sqcup s = s$$
//!
//! — "commutative, associative, idempotent, which is exactly the statement that
//! arrival order, duplication, and retry cannot change the result." An
//! overwrite is none of the three. And §VI is explicit about what that buys:
//! "because the group value is a merge over member entries and never an
//! overwrite of them, the member's own number survives every aggregation by
//! construction. The holon invariant — never erase the node — is not a policy
//! sitting on top of the merge. It IS the merge."
//!
//! So a root that arrives on top of an existing root is **joined**.
//!
//! ★★ **The rules, and why each is the least upper bound and not a preference.**
//!
//! - **Objects** join key-wise over the union of their keys. A key only one
//!   side has is kept — dropping it would be below both operands and so not an
//!   upper bound at all.
//! - **Numbers** join by maximum. This is §VI's own grow-only counter,
//!   `out[id] = max(a[id], b[id])`, at the leaf.
//! - **Arrays** join by union, preserving first-seen order and dropping exact
//!   duplicates. Union is the join on sets; the ordering is a rendering
//!   detail, and de-duplication is what makes it idempotent.
//! - **Booleans** join by `or`, the join on the two-element lattice.
//! - **Strings and nulls** are not ordered, so two different ones have no least
//!   upper bound to take. The pair is kept as a `Disagreement` — see below.
//!
//! ★★★ **Disagreement is recorded, never resolved by fiat.** Picking one of two
//! unordered values would be an overwrite with better manners, and it would
//! break commutativity: whoever synced last would win. Instead the join reports
//! where it happened, so a household can be told two of its nodes disagree
//! about a label rather than silently shown one of them. Recording it keeps the
//! operation commutative — the same pair is reported whichever way round it is
//! joined.
//!
//! ★ In the case this exists for, every rule coincides: two genesis states of
//! the same template are equal, so the join is `s ⊔ s = s` and nothing moves.
//! That is the point. The machinery is here for correctness under merge, not to
//! change what a single-history household folds to.

use serde_json::{Map, Value};

use crate::crdt::JoinSemilattice;

/// Where two roots held values with no least upper bound between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disagreement {
    /// Dot-path to the leaf, `""` for the root itself.
    pub path: String,
    /// The two values, ordered canonically so the report does not depend on
    /// which side was joined first.
    pub left: String,
    pub right: String,
}

/// A state document under the §VI join.
///
/// ★ A newtype rather than an inherent impl on `Value`, so the lattice laws can
/// be checked with `crdt::laws_hold` against the very type the fold uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Root(pub Value);

impl JoinSemilattice for Root {
    fn join(&self, other: &Self) -> Self {
        Root(join_values(&self.0, &other.0, "", &mut Vec::new()))
    }
}

/// Join two roots, and say where they disagreed.
pub fn join_roots(a: &Value, b: &Value) -> (Value, Vec<Disagreement>) {
    let mut found = Vec::new();
    let joined = join_values(a, b, "", &mut found);
    (joined, found)
}

fn join_values(a: &Value, b: &Value, path: &str, found: &mut Vec<Disagreement>) -> Value {
    match (a, b) {
        // ★ Identical is the common case and the whole reason twin genesis is
        //   harmless: `s ⊔ s = s`, checked before anything else is attempted.
        _ if a == b => a.clone(),

        (Value::Object(x), Value::Object(y)) => {
            let mut out = Map::new();
            // Union of keys. ★ Iterating one side and then the other, with the
            // BTreeMap-backed Map keeping order stable, so the result does not
            // depend on argument order.
            for key in x.keys().chain(y.keys()) {
                if out.contains_key(key) {
                    continue;
                }
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let joined = match (x.get(key), y.get(key)) {
                    (Some(l), Some(r)) => join_values(l, r, &child, found),
                    (Some(only), None) | (None, Some(only)) => only.clone(),
                    (None, None) => unreachable!("the key came from one of them"),
                };
                out.insert(key.clone(), joined);
            }
            Value::Object(out)
        }

        // §VI's grow-only counter at the leaf: out = max(a, b).
        (Value::Number(x), Value::Number(y)) => {
            let (l, r) = (x.as_f64(), y.as_f64());
            match (l, r) {
                (Some(l), Some(r)) if r > l => b.clone(),
                (Some(_), Some(_)) => a.clone(),
                // A number that is not representable as f64 is not orderable
                // here, and guessing would be worse than saying so.
                _ => {
                    record(found, path, a, b);
                    a.clone()
                }
            }
        }

        (Value::Bool(x), Value::Bool(y)) => Value::Bool(*x || *y),

        (Value::Array(x), Value::Array(y)) => {
            let mut out: Vec<Value> = Vec::with_capacity(x.len() + y.len());
            for v in x.iter().chain(y.iter()) {
                if !out.contains(v) {
                    out.push(v.clone());
                }
            }
            Value::Array(out)
        }

        // ★★ Absent joins to present: null is the bottom of this lattice, so
        //    the other side is already the upper bound.
        (Value::Null, other) | (other, Value::Null) => other.clone(),

        // Two unordered values. Recorded, not resolved.
        _ => {
            record(found, path, a, b);
            // ★ Canonical choice so the RESULT is commutative even though the
            //   pair is unordered: the lexicographically smaller encoding.
            //   Reported as a disagreement either way, so nothing is hidden.
            let (l, r) = (a.to_string(), b.to_string());
            if l <= r {
                a.clone()
            } else {
                b.clone()
            }
        }
    }
}

fn record(found: &mut Vec<Disagreement>, path: &str, a: &Value, b: &Value) {
    let (l, r) = (a.to_string(), b.to_string());
    let (left, right) = if l <= r { (l, r) } else { (r, l) };
    let d = Disagreement { path: path.to_string(), left, right };
    if !found.contains(&d) {
        found.push(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::laws_hold;
    use serde_json::json;

    fn r(v: Value) -> Root {
        Root(v)
    }

    #[test]
    fn the_three_laws_hold_on_state_shaped_documents() {
        // ★★★ Checked with the article's own predicate, against the very type
        //     the fold uses. §VI is not paraphrased here, it is executed.
        let a = r(json!({"finances": {"liquid": {"balance": 300.0}, "pockets": {}}}));
        let b = r(json!({"finances": {"liquid": {"balance": 700.0}, "pockets": {"food": 1}}}));
        let c = r(json!({"finances": {"liquid": {"balance": 55.0}}, "inventory": []}));
        let report = laws_hold(&a, &b, &c);
        assert!(report.all_hold(), "{}", report.describe());
    }

    #[test]
    fn twin_genesis_converges_and_moves_nothing() {
        // ★★★ The case this module exists for. Two nodes created the same
        //     household from the same template before they met, so the two
        //     genesis states are equal and the join is `s ⊔ s = s`.
        let genesis = json!({
            "finances": {"liquid": {"balance": 0.0}, "pockets": {}, "accounts": {}},
            "inventory": {"assets": []},
            "vendors": {}
        });
        let (joined, disagreements) = join_roots(&genesis, &genesis);
        assert_eq!(joined, genesis, "identical roots join to themselves");
        assert!(disagreements.is_empty());
    }

    #[test]
    fn a_root_arriving_on_a_populated_one_erases_nothing() {
        // ★★★ The holon invariant, which §VI says IS the merge: no member
        //     entry is overwritten by an aggregation.
        let lived_in = json!({"finances": {"pockets": {"food": 6620, "rent": 15000}}});
        let fresh = json!({"finances": {"pockets": {}, "accounts": {}}});
        let (joined, _) = join_roots(&lived_in, &fresh);
        assert_eq!(joined["finances"]["pockets"]["food"], json!(6620));
        assert_eq!(joined["finances"]["pockets"]["rent"], json!(15000));
        assert_eq!(joined["finances"]["accounts"], json!({}), "and the new key arrived");

        // And the other way round, because commutativity is the point.
        let (other_way, _) = join_roots(&fresh, &lived_in);
        assert_eq!(joined, other_way);
    }

    #[test]
    fn numbers_join_by_maximum() {
        // §VI: `out[id] = max(a[id], b[id])` — the grow-only counter, at the leaf.
        let (joined, _) = join_roots(&json!({"n": 3}), &json!({"n": 9}));
        assert_eq!(joined["n"], json!(9));
    }

    #[test]
    fn arrays_join_by_union_and_stay_idempotent() {
        let a = json!({"sources": [{"label": "salary"}]});
        let b = json!({"sources": [{"label": "salary"}, {"label": "gift"}]});
        let (joined, _) = join_roots(&a, &b);
        assert_eq!(joined["sources"].as_array().unwrap().len(), 2, "no duplicate");
        let (again, _) = join_roots(&joined, &joined);
        assert_eq!(again, joined, "s ⊔ s = s");
    }

    #[test]
    fn two_unordered_values_are_reported_rather_than_silently_picked() {
        // ★★★ Choosing one would be an overwrite with better manners, and it
        //     would make the result depend on who synced last.
        let (_, one_way) = join_roots(&json!({"label": "Home"}), &json!({"label": "Nyumba"}));
        let (_, other_way) = join_roots(&json!({"label": "Nyumba"}), &json!({"label": "Home"}));
        assert_eq!(one_way.len(), 1);
        assert_eq!(one_way[0].path, "label");
        assert_eq!(one_way, other_way, "the same report either way round");
    }

    #[test]
    fn null_is_the_bottom_of_the_lattice() {
        let (joined, d) = join_roots(&json!({"x": null}), &json!({"x": 5}));
        assert_eq!(joined["x"], json!(5));
        assert!(d.is_empty(), "absent joining to present is not a disagreement");
    }
}
