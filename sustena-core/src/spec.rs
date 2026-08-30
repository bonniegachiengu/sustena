//! **The whole-spec validator** (DSL · GENOME §VI).
//!
//! ```text
//!   C : Text ⇀ Σ        Ok(Σ)  or  Err({ε})  —  never a partial Sustain
//! ```
//!
//! ★★★ **One door, and everything a definition promises goes through it.**
//! `editing::typecheck` checks the invariants, and invariants are only one of
//! the things a definition *names*. It also names Enzymes it may run, aggregates
//! it will total, and widgets a surface will draw. Each of those is a reference
//! to something that has to exist, and until now only one of them was checked.
//!
//! ★★★ **The failure mode is the same one DSL-5 fixed, in a different place.**
//! A definition naming an Enzyme nothing provides loads perfectly. The first
//! person to try it is told the operator *is not available on this sustain* —
//! which sounds like a permission, reads like a rule, and is neither. It is a
//! typo in a document, and the only honest moment to say so is when the
//! document is written.
//!
//! ## What "whole" means here, and what it does not
//!
//! ★★ Every finding is collected before returning. A validator that stopped at
//! the first error would make fixing a spec an N-round conversation, and the
//! second error is often the one that explains the first.
//!
//! ★★ **Duplicates are findings, not tie-breaks.** Two invariants with one id,
//! two aggregates with one id — somebody meant two different things and one of
//! them is silently unreachable. Picking a winner here would be this module
//! deciding which of somebody's rules to discard.
//!
//! ★★ Widgets and children are **optional inputs**, because a definition is
//! validated in more than one place: at author time with everything to hand, and
//! at load time before a surface exists. Absent is not the same as empty — the
//! checks that need them are skipped rather than failed.

use std::collections::BTreeSet;

use crate::editing::Definition;
use crate::operator::Registry;
use crate::schema::{Schema, TypeError};

/// Everything a whole-spec check needs beyond the definition itself.
///
/// ★★ Each field is optional in effect: an empty `children` skips the holon
/// check exactly as GENOME §IX intends (a holon is declared before it is
/// populated), and no widgets means no widget check rather than no widgets.
#[derive(Default)]
pub struct SpecContext<'a> {
    pub children: Vec<(&'a str, &'a Schema)>,
}

/// **Validate everything a definition names.**
///
/// ★★★ Order is deliberate: names first, then types. An Enzyme that does not
/// exist and a rule that does not type are different mistakes, and reporting the
/// second while the first is outstanding sends a person to fix the wrong thing.
pub fn validate_spec(
    def: &Definition,
    registry: &Registry,
    context: &SpecContext,
) -> Result<(), Vec<TypeError>> {
    let mut errors = Vec::new();

    // ── Enzyme references ───────────────────────────────────────────────────
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for name in &def.operators {
        if !seen.insert(name.as_str()) {
            errors.push(TypeError {
                path: name.clone(),
                detail: "is listed twice".into(),
            });
        }
        if registry.get(name).is_none() {
            // ★★★ The whole point of this row. Without it the first person to
            //     try it is told the operator "is not available on this
            //     sustain" — which sounds like a permission and is a typo.
            errors.push(TypeError {
                path: name.clone(),
                detail: "is not an Enzyme this engine provides — check the spelling".into(),
            });
        }
    }

    // ── ids that have to be unique to mean anything ─────────────────────────
    let mut invariant_ids: BTreeSet<&str> = BTreeSet::new();
    for (id, _) in &def.invariants {
        if !invariant_ids.insert(id.as_str()) {
            errors.push(TypeError {
                path: id.clone(),
                detail: "names two different rules, so one of them can never be reported".into(),
            });
        }
    }
    let mut aggregate_ids: BTreeSet<&str> = BTreeSet::new();
    for agg in &def.aggregates {
        if !aggregate_ids.insert(agg.id()) {
            errors.push(TypeError {
                path: agg.id().to_string(),
                detail: "names two different aggregates, so one of them is unreachable".into(),
            });
        }
        // ★★ An aggregate whose id collides with a real dimension would shadow
        //    it in the joint program, and a parent rule reading that name would
        //    silently get the total instead of the dimension.
        if def.schema.dimensions.contains_key(agg.id()) {
            errors.push(TypeError {
                path: agg.id().to_string(),
                detail: "is also a declared dimension — a rule reading that name would get \
                         the total instead"
                    .into(),
            });
        }
    }

    // ── the rules, and the holon path they may read across ──────────────────
    if let Err(mut e) = crate::holon_typing::typecheck_holon(def, &context.children) {
        errors.append(&mut e);
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::DimType;

    fn number() -> DimType {
        DimType::Number { lo: None, hi: None }
    }

    fn base() -> Definition {
        Definition::new(
            Schema::new().declare(
                "finances",
                DimType::Record {
                    fields: [(
                        "liquid".to_string(),
                        DimType::Record {
                            fields: [("balance".to_string(), number())].into_iter().collect(),
                        },
                    )]
                    .into_iter()
                    .collect(),
                },
            ),
        )
    }

    fn reg() -> Registry {
        Registry::default()
    }

    #[test]
    fn a_definition_naming_real_enzymes_and_real_rules_validates() {
        let def = base()
            .with_operator("budget.spend")
            .with_operator("budget.allocate")
            .with_invariant("floor", "finances.liquid.balance >= 0");
        assert!(validate_spec(&def, &reg(), &SpecContext::default()).is_ok());
    }

    #[test]
    fn an_enzyme_nothing_provides_is_caught_when_the_document_is_written() {
        // ★★★ Without this the first person to try it is told the operator "is
        //     not available on this sustain" — which sounds like a permission,
        //     reads like a rule, and is a typo in a document.
        let def = base().with_operator("budget.spned");
        let errors = validate_spec(&def, &reg(), &SpecContext::default()).expect_err("refuses");
        assert_eq!(errors[0].path, "budget.spned");
        assert!(errors[0].detail.contains("check the spelling"), "{:?}", errors[0]);
    }

    #[test]
    fn every_finding_is_collected_rather_than_the_first() {
        // ★★ Stopping at the first would make fixing a spec an N-round
        //    conversation, and the second error is often what explains the
        //    first.
        let def = base()
            .with_operator("nope.one")
            .with_operator("nope.two")
            .with_invariant("bad", "finances.liquid.balance >= 'plenty'");
        let errors = validate_spec(&def, &reg(), &SpecContext::default()).expect_err("refuses");
        assert!(errors.len() >= 3, "{errors:?}");
    }

    #[test]
    fn two_rules_with_one_id_is_a_finding_not_a_tie_break() {
        // ★★★ Somebody meant two different things and one is silently
        //     unreachable. Picking a winner here would be this module deciding
        //     which of their rules to discard.
        let def = base()
            .with_invariant("floor", "finances.liquid.balance >= 0")
            .with_invariant("floor", "finances.liquid.balance >= 100");
        let errors = validate_spec(&def, &reg(), &SpecContext::default()).expect_err("refuses");
        assert!(errors[0].detail.contains("never be reported"), "{:?}", errors[0]);
    }

    #[test]
    fn an_enzyme_listed_twice_is_named() {
        let def = base().with_operator("budget.spend").with_operator("budget.spend");
        let errors = validate_spec(&def, &reg(), &SpecContext::default()).expect_err("refuses");
        assert!(errors[0].detail.contains("twice"));
    }

    #[test]
    fn an_aggregate_that_shadows_a_dimension_is_refused() {
        // ★★★ It would shadow the dimension in the joint program, so a parent
        //     rule reading that name would silently get the total instead of
        //     the thing the household actually declared.
        let def = base()
            .with_aggregate("finances", "finances.liquid.balance", "sum")
            .expect("declares");
        let errors = validate_spec(&def, &reg(), &SpecContext::default()).expect_err("refuses");
        assert!(errors[0].detail.contains("total instead"), "{:?}", errors[0]);
    }

    #[test]
    fn two_aggregates_with_one_id_is_a_finding() {
        let def = base()
            .with_aggregate("total", "finances.liquid.balance", "sum")
            .expect("declares")
            .with_aggregate("total", "finances.liquid.balance", "max")
            .expect("declares");
        let errors = validate_spec(&def, &reg(), &SpecContext::default()).expect_err("refuses");
        assert!(errors.iter().any(|e| e.detail.contains("unreachable")), "{errors:?}");
    }

    #[test]
    fn the_holon_path_is_part_of_the_whole_spec_check() {
        // ★★ One door. A parent whose aggregate no child can satisfy is not a
        //    separate kind of broken document.
        let child = Schema::new().declare("takings", number());
        let def = base()
            .with_aggregate("household_total", "finances.liquid.balance", "sum")
            .expect("declares");
        let ctx = SpecContext { children: vec![("the shop", &child)] };
        assert!(validate_spec(&def, &reg(), &ctx).is_err());
        // And with no children declared yet, it is simply not checked.
        assert!(validate_spec(&def, &reg(), &SpecContext::default()).is_ok());
    }

    #[test]
    fn every_shipped_operator_name_in_the_registry_validates() {
        // ★★★ The check's first real job: a definition naming EVERY Enzyme the
        //     engine provides must pass. A finding here would mean the registry
        //     and the validator disagree about what exists.
        let mut def = base();
        for name in reg().names() {
            def = def.with_operator(&name);
        }
        assert!(validate_spec(&def, &reg(), &SpecContext::default()).is_ok());
    }
}
