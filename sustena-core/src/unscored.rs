//! **Gating what nobody is optimising** (Sustain · CELL §XI).
//!
//! ```text
//!   U = ⋃ Symbiont utility dimensions
//!   d ∉ U  ⟹  d is unwatched, and unwatched is where Goodhart lands
//! ```
//!
//! ★★★ **Goodhart's law is not about the measured dimension going wrong.** It
//! is about the *unmeasured* ones. Optimise the pockets and the inventory
//! quietly empties; optimise the total and one member's tab quietly runs. The
//! dimension nobody scored is the one that moves, and it moves precisely
//! because nothing is looking.
//!
//! ★★★ **The half `goodhart.rs` deliberately did not build, and why this does
//! not build it either.** Its row says the auto-emit needs to know *what bound*,
//! and nobody declared one — a fabricated threshold over a dimension nobody was
//! watching would be exactly the confident-number-from-nowhere this codebase
//! refuses everywhere else. That reasoning is right and stands.
//!
//! ★★★ **But there is a bound that is not invented: the household's own
//! declared type.** A dimension declared `Number { lo: 0 }` has said, in the
//! definition, that it does not go below zero. Emitting an invariant from that
//! is *using their number*, not choosing one. It converts a bound the schema
//! already states into a bound the gate actually enforces — which is the
//! difference between a type that describes and a rule that holds.
//!
//! ★★ **Everything else is NAMED, not bounded.** A dimension nobody scores and
//! nobody bounded gets no invariant and appears in [`Unscored::ungated`]. That
//! is the honest output: *these are the things nothing is watching and nothing
//! is holding*, which a person can act on. Filling them with a guessed floor
//! would replace a visible gap with an invisible wrong answer.

use std::collections::BTreeSet;

use crate::schema::{DimType, Schema};

/// One dimension nobody is optimising, and what can honestly be said about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Unwatched {
    pub path: String,
    /// The invariant its own declared type supports, when it declared one.
    pub bound: Option<String>,
}

/// What the household is not watching.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Unscored {
    /// Dimensions with a declared bound, and the invariant that enforces it.
    pub gated: Vec<Unwatched>,
    /// ★★★ Unscored AND unbounded. Named rather than guessed at.
    pub ungated: Vec<String>,
}

impl Unscored {
    /// Every invariant this can honestly emit.
    pub fn invariants(&self) -> Vec<(String, String)> {
        self.gated
            .iter()
            .filter_map(|u| {
                u.bound.as_ref().map(|b| (format!("unscored:{}", u.path), b.clone()))
            })
            .collect()
    }

    /// ★★ True when something is unwatched and unheld. Not an error — a fact a
    /// person should see before deciding it is fine.
    pub fn has_gaps(&self) -> bool {
        !self.ungated.is_empty()
    }

    pub fn describe(&self) -> String {
        if self.ungated.is_empty() {
            return format!("{} unscored dimensions, all bounded by their own type", self.gated.len());
        }
        format!(
            "{} unscored dimensions bounded by their own type; {} watched by nothing and held by nothing: {}",
            self.gated.len(),
            self.ungated.len(),
            self.ungated.join(", ")
        )
    }
}

/// Every leaf dimension the schema declares, as a dot-path.
///
/// ★★ Leaves only. A record is not a quantity that can drift; its fields are.
fn leaves(schema: &Schema) -> Vec<(String, DimType)> {
    let mut out = Vec::new();
    for (name, ty) in &schema.dimensions {
        walk(name, ty, &mut out);
    }
    out
}

fn walk(path: &str, ty: &DimType, out: &mut Vec<(String, DimType)>) {
    match ty {
        DimType::Record { fields } => {
            for (name, inner) in fields {
                walk(&format!("{path}.{name}"), inner, out);
            }
        }
        // ★★★ An open map's KEYS belong to the household — a pocket is named by
        //     a person. There is no fixed set of paths to emit invariants over,
        //     so the map itself is the leaf and a quantifier is what would
        //     reach inside it. Enumerating today's keys would produce rules that
        //     silently fail to cover tomorrow's pocket.
        DimType::Map { .. } | DimType::List { .. } => out.push((path.to_string(), ty.clone())),
        other => out.push((path.to_string(), other.clone())),
    }
}

/// The invariant a dimension's own declared type supports.
///
/// ★★ `None` for anything that declared no bound. Not a failure — a dimension
/// with no declared range genuinely has nothing to enforce.
fn bound_from_type(path: &str, ty: &DimType) -> Option<String> {
    let DimType::Number { lo, hi } = ty else { return None };
    match (lo, hi) {
        (Some(l), Some(h)) => Some(format!("{path} >= {l} AND {path} <= {h}")),
        (Some(l), None) => Some(format!("{path} >= {l}")),
        (None, Some(h)) => Some(format!("{path} <= {h}")),
        (None, None) => None,
    }
}

/// **What nobody is optimising, and what can be done about it.**
///
/// ★★★ `scored` is `U` — the union of every Symbiont's utility dimensions.
/// A dimension in `U` is being watched by somebody, so it is not this
/// mechanism's business; the ones outside it are.
pub fn unscored(schema: &Schema, scored: &BTreeSet<String>) -> Unscored {
    let mut out = Unscored::default();
    for (path, ty) in leaves(schema) {
        // ★★ A dimension counts as watched if it, or a container it sits in, is
        //    scored. Somebody optimising `finances.liquid` is watching
        //    `finances.liquid.balance`, and reporting it as unwatched would be
        //    noise that teaches a person to skim the list.
        let watched = scored
            .iter()
            .any(|s| path == *s || path.starts_with(&format!("{s}.")));
        if watched {
            continue;
        }
        match bound_from_type(&path, &ty) {
            Some(bound) => out.gated.push(Unwatched { path, bound: Some(bound) }),
            None => out.ungated.push(path),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn scored(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    fn household() -> Schema {
        let mut liquid = BTreeMap::new();
        liquid.insert("balance".to_string(), DimType::Number { lo: Some(0.0), hi: None });

        let mut finances = BTreeMap::new();
        finances.insert("liquid".to_string(), DimType::Record { fields: liquid });
        finances.insert("mood".to_string(), DimType::Number { lo: None, hi: None });
        finances.insert("label".to_string(), DimType::Text);

        Schema::new()
            .declare("finances", DimType::Record { fields: finances })
            .declare("stock", DimType::Number { lo: Some(0.0), hi: Some(100.0) })
    }

    #[test]
    fn a_dimension_somebody_optimises_is_not_this_mechanisms_business() {
        let out = unscored(&household(), &scored(&["finances.liquid.balance", "stock"]));
        assert!(out.gated.iter().all(|u| u.path != "finances.liquid.balance"));
        assert!(!out.ungated.contains(&"stock".to_string()));
    }

    #[test]
    fn optimising_a_container_watches_what_is_inside_it() {
        // ★★ Somebody optimising `finances.liquid` is watching its balance.
        //    Reporting it as unwatched would be noise that teaches a person to
        //    skim the list.
        let out = unscored(&household(), &scored(&["finances.liquid"]));
        assert!(out.gated.iter().all(|u| u.path != "finances.liquid.balance"));
    }

    #[test]
    fn an_unscored_dimension_with_its_own_declared_bound_is_gated_by_that_bound() {
        // ★★★ Using THEIR number, not choosing one. It converts a bound the
        //     schema already states into a bound the gate enforces — the
        //     difference between a type that describes and a rule that holds.
        let out = unscored(&household(), &scored(&[]));
        let stock = out.gated.iter().find(|u| u.path == "stock").expect("bounded");
        assert_eq!(stock.bound.as_deref(), Some("stock >= 0 AND stock <= 100"));
    }

    #[test]
    fn a_one_sided_bound_emits_a_one_sided_rule() {
        let out = unscored(&household(), &scored(&[]));
        let balance =
            out.gated.iter().find(|u| u.path == "finances.liquid.balance").expect("bounded");
        assert_eq!(balance.bound.as_deref(), Some("finances.liquid.balance >= 0"));
    }

    #[test]
    fn an_unscored_unbounded_dimension_is_named_and_never_guessed_at() {
        // ★★★ The half `goodhart.rs` deliberately did not build, and this does
        //     not build either. A fabricated floor would replace a visible gap
        //     with an invisible wrong answer.
        let out = unscored(&household(), &scored(&[]));
        assert!(out.ungated.contains(&"finances.mood".to_string()));
        assert!(out.gated.iter().all(|u| u.path != "finances.mood"));
    }

    #[test]
    fn something_that_is_not_a_quantity_cannot_drift_and_gets_no_bound() {
        // ★★ A label does not go out of range. Emitting a rule over it would be
        //    a rule that can never fire, which is worse than none — it makes
        //    the list of guards look longer than the protection is.
        let out = unscored(&household(), &scored(&[]));
        assert!(out.gated.iter().all(|u| u.path != "finances.label"));
        assert!(out.ungated.contains(&"finances.label".to_string()));
    }

    #[test]
    fn the_emitted_invariants_are_real_predicates() {
        // ★★★ Emitting a string the evaluator cannot read would be a guard that
        //     silently never holds.
        let out = unscored(&household(), &scored(&[]));
        assert!(!out.invariants().is_empty());
        for (id, expression) in out.invariants() {
            assert!(
                crate::predicate::parse_predicate(&expression).is_ok(),
                "{id}: {expression}",
            );
        }
    }

    #[test]
    fn an_emitted_invariant_actually_refuses_what_it_says_it_refuses() {
        use crate::predicate::{eval::evaluate, parse_predicate};
        let out = unscored(&household(), &scored(&[]));
        let (_, expression) = out
            .invariants()
            .into_iter()
            .find(|(id, _)| id.contains("stock"))
            .expect("emitted");
        let node = parse_predicate(&expression).expect("parses");
        let empty = serde_json::Map::new();
        assert!(evaluate(&node, &serde_json::json!({"stock": 50}), &empty).0);
        assert!(!evaluate(&node, &serde_json::json!({"stock": 500}), &empty).0);
    }

    #[test]
    fn a_household_watching_nothing_is_told_what_is_unheld() {
        // ★★ Not an error — a fact a person should see before deciding it is
        //    fine.
        let out = unscored(&household(), &scored(&[]));
        assert!(out.has_gaps());
        let said = out.describe();
        assert!(said.contains("watched by nothing and held by nothing"), "{said}");
        assert!(said.contains("finances.mood"), "and names them: {said}");
    }

    #[test]
    fn an_open_map_is_left_to_a_quantifier_rather_than_enumerated() {
        // ★★★ A pocket is named by a person. Emitting rules over today's keys
        //     would produce guards that silently fail to cover tomorrow's
        //     pocket — the worst kind, because the list looks complete.
        let schema = Schema::new().declare(
            "pockets",
            DimType::Map { value: Box::new(DimType::Number { lo: Some(0.0), hi: None }) },
        );
        let out = unscored(&schema, &scored(&[]));
        assert!(out.gated.is_empty(), "no per-key rules invented");
        assert_eq!(out.ungated, vec!["pockets".to_string()]);
    }
}
