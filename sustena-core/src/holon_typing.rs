//! **The holon path type-checked as ONE program** (DSL · GENOME §IX).
//!
//! ```text
//!   Γ_child ⊢ child_path : τ    aggregable(op, τ)
//!   ─────────────────────────────────────────────    Γ_parent ⊕ {id : Number}
//!                                                     ⊢ parent invariants
//! ```
//!
//! ★★★ **A parent and its children are not two programs that happen to talk.**
//! A parent declares an aggregate — *sum every child's
//! `finances.liquid.balance`* — and then writes rules about the total. Three
//! things have to agree for that to mean anything: the child must HAVE that
//! dimension, it must be a kind you can SUM, and the parent's rule must treat
//! the result as what it is. Check the parent alone and all three can be wrong
//! at once with nothing to report it.
//!
//! ★★★ **The failure this prevents is quiet, which is what makes it worth
//! author-time work.** An aggregate over a dimension no child declares does not
//! crash; it produces a roll-up of **zero contributions**, which reads as a
//! household with no money. Summing a string does not crash either — it skips
//! every child as unreadable and reports the same honest, useless zero. Both
//! look exactly like a household that genuinely has nothing, and there is no
//! moment afterwards at which anyone can tell the difference.
//!
//! ## The joint program
//!
//! ★★ An aggregate's id becomes a **readable dimension at the parent**, of type
//! Number. That is not a convention invented here — it is what roll-up already
//! does when it folds the computed totals into the parent's state. Typing the
//! parent's invariants against `Γ_parent ⊕ {id : Number}` is therefore checking
//! the program that actually runs, rather than a stricter one nobody executes.
//!
//! ★★ **COUNT is the exception, and it is a real one.** Counting how many
//! children have a thing is meaningful whatever the thing is — labels, flags,
//! nested records. `SUM` of a label is not. So `aggregable` is per-op rather
//! than a single "is it a number" rule applied to all five.

use serde_json::Value;

use crate::editing::Definition;
use crate::predicate::ast::AggFunc;
use crate::predicate::types::{typecheck as typecheck_predicate, Gamma};
use crate::predicate::parse_predicate;
use crate::schema::{DimType, Schema, TypeError};

/// Can this operation be applied to values of this type?
///
/// ★★ `Any` says yes to everything, the same admission it makes everywhere
/// else: nothing was declared, so there is nothing to disagree with.
fn aggregable(op: AggFunc, ty: &DimType) -> bool {
    match op {
        // Counting how many children have a thing is meaningful whatever the
        // thing is.
        AggFunc::Count => true,
        _ => matches!(ty, DimType::Number { .. } | DimType::Any),
    }
}

fn describe(ty: &DimType) -> &'static str {
    match ty {
        DimType::Number { .. } => "a number",
        DimType::Text => "text",
        DimType::Bool => "true or false",
        DimType::List { .. } => "a list",
        DimType::Record { .. } => "a record",
        DimType::Map { .. } => "a map",
        DimType::Any => "undeclared",
    }
}

/// **Type-check a parent against the children it will aggregate over.**
///
/// ★★★ Every child, not a sample. A holon whose members hold different
/// definitions is ordinary — a household of habitats and a shop — and an
/// aggregate is only sound if it is sound for each of them. Checking one and
/// assuming the rest is how the roll-up silently drops exactly the members that
/// differ.
pub fn typecheck_holon(
    parent: &Definition,
    children: &[(&str, &Schema)],
) -> Result<(), Vec<TypeError>> {
    let mut errors = Vec::new();

    for agg in &parent.aggregates {
        for (who, child) in children {
            match child.resolve(agg.segments()) {
                None => errors.push(TypeError {
                    path: agg.id().to_string(),
                    detail: format!(
                        "reads '{}', which {who} does not declare — the roll-up would \
                         quietly total nothing",
                        agg.child_path()
                    ),
                }),
                Some(ty) if !aggregable(agg.op(), ty) => errors.push(TypeError {
                    path: agg.id().to_string(),
                    detail: format!(
                        "takes {} of '{}', which is {} in {who}",
                        agg.op().name(),
                        agg.child_path(),
                        describe(ty)
                    ),
                }),
                Some(_) => {}
            }
        }
    }

    // ★★ The parent's own rules, against the program that actually runs: its
    //    schema PLUS the totals roll-up folds in. Typing them against the
    //    schema alone would report every aggregate reference as undeclared.
    let joint = joint_schema(parent);
    for (id, expression) in &parent.invariants {
        let Ok(p) = parse_predicate(expression) else { continue };
        for mut e in crate::schema::bind(&p, &joint) {
            e.detail = format!("invariant '{id}': {}", e.detail);
            errors.push(e);
        }
        for mut e in typecheck_predicate(&p, &Gamma::new(&joint)) {
            e.detail = format!("invariant '{id}': {}", e.detail);
            errors.push(e);
        }
    }

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// `Γ_parent ⊕ {aggregate_id : Number}`.
///
/// ★★ Number for every op including COUNT — a count is a number even when what
/// it counted was not.
pub fn joint_schema(parent: &Definition) -> Schema {
    let mut schema = parent.schema.clone();
    for agg in &parent.aggregates {
        schema = schema.declare(agg.id(), DimType::Number { lo: None, hi: None });
    }
    schema
}

/// Is this value readable as the type the aggregate needs?
///
/// ★ Used by roll-up at read time. The author-time check above is the one that
/// makes this rarely fire; it remains because a child's state can be older than
/// its schema.
pub fn readable(op: AggFunc, value: &Value) -> bool {
    match op {
        AggFunc::Count => true,
        _ => value.is_number(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::Definition;

    fn number() -> DimType {
        DimType::Number { lo: None, hi: None }
    }

    fn habitat() -> Schema {
        Schema::new().declare(
            "finances",
            DimType::Record {
                fields: [
                    (
                        "liquid".to_string(),
                        DimType::Record {
                            fields: [("balance".to_string(), number())].into_iter().collect(),
                        },
                    ),
                    ("label".to_string(), DimType::Text),
                ]
                .into_iter()
                .collect(),
            },
        )
    }

    fn shop() -> Schema {
        Schema::new().declare(
            "takings",
            DimType::Record {
                fields: [("today".to_string(), number())].into_iter().collect(),
            },
        )
    }

    fn household() -> Definition {
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

    #[test]
    fn a_parent_summing_a_dimension_its_children_have_is_well_typed() {
        let parent = household()
            .with_aggregate("household_total", "finances.liquid.balance", "sum")
            .expect("declares");
        assert!(typecheck_holon(&parent, &[("a habitat", &habitat())]).is_ok());
    }

    #[test]
    fn an_aggregate_over_a_dimension_no_child_has_is_caught_at_author_time() {
        // ★★★ The quiet failure. It does not crash — it produces a roll-up of
        //     zero contributions, which reads exactly like a household that
        //     genuinely has no money, and nothing afterwards can tell the
        //     difference.
        let parent = household()
            .with_aggregate("household_total", "finances.savings.balance", "sum")
            .expect("declares");
        let errors =
            typecheck_holon(&parent, &[("a habitat", &habitat())]).expect_err("must refuse");
        assert!(errors[0].detail.contains("does not declare"), "{:?}", errors[0]);
        assert!(errors[0].detail.contains("total nothing"), "and says why it matters");
    }

    #[test]
    fn summing_a_string_is_caught_at_author_time() {
        // ★★★ Also silent: every child is skipped as unreadable and the total
        //     is the same honest, useless zero.
        let parent = household()
            .with_aggregate("labels", "finances.label", "sum")
            .expect("declares");
        let errors =
            typecheck_holon(&parent, &[("a habitat", &habitat())]).expect_err("must refuse");
        assert!(errors[0].detail.contains("text"), "{:?}", errors[0]);
    }

    #[test]
    fn counting_something_that_is_not_a_number_is_fine() {
        // ★★ How many children have a label is a real question. What their
        //    labels SUM to is not.
        let parent = household()
            .with_aggregate("how_many_labelled", "finances.label", "count")
            .expect("declares");
        assert!(typecheck_holon(&parent, &[("a habitat", &habitat())]).is_ok());
    }

    #[test]
    fn every_child_is_checked_not_just_the_first() {
        // ★★★ A holon whose members hold different definitions is ordinary — a
        //     household of habitats and a shop. Checking one and assuming the
        //     rest is how a roll-up silently drops exactly the members that
        //     differ.
        let parent = household()
            .with_aggregate("household_total", "finances.liquid.balance", "sum")
            .expect("declares");
        let errors = typecheck_holon(&parent, &[("a habitat", &habitat()), ("the shop", &shop())])
            .expect_err("must refuse");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].detail.contains("the shop"), "and names which one");
    }

    #[test]
    fn a_parent_rule_may_read_an_aggregate_it_declared() {
        // ★★ The joint program: an aggregate's id is a readable dimension at
        //    the parent, because that is what roll-up already folds in. Typing
        //    against the schema alone would call every aggregate undeclared.
        let parent = household()
            .with_aggregate("household_total", "finances.liquid.balance", "sum")
            .expect("declares")
            .with_invariant("floor", "household_total >= 0");
        assert!(typecheck_holon(&parent, &[("a habitat", &habitat())]).is_ok());
    }

    #[test]
    fn a_parent_rule_reading_an_aggregate_nobody_declared_is_refused() {
        let parent = household().with_invariant("floor", "household_total >= 0");
        let errors =
            typecheck_holon(&parent, &[("a habitat", &habitat())]).expect_err("must refuse");
        assert!(errors[0].detail.contains("undeclared"), "{:?}", errors[0]);
    }

    #[test]
    fn a_parent_rule_that_misuses_an_aggregate_is_refused() {
        // ★★★ A total is a number. Comparing it to a word is the same class of
        //     broken document DSL-5 catches inside one Sustain, and it has to
        //     be caught across the holon path too or the joint program is only
        //     jointly NAME-checked.
        let parent = household()
            .with_aggregate("household_total", "finances.liquid.balance", "sum")
            .expect("declares")
            .with_invariant("nonsense", "household_total >= 'plenty'");
        let errors =
            typecheck_holon(&parent, &[("a habitat", &habitat())]).expect_err("must refuse");
        assert!(
            errors[0].detail.contains("different kinds of thing"),
            "{:?}",
            errors[0],
        );
    }

    #[test]
    fn a_parent_with_no_children_yet_is_not_an_error() {
        // ★★ A holon is declared before it is populated. Refusing here would
        //    make declaring one impossible before the first member joins.
        let parent = household()
            .with_aggregate("household_total", "finances.liquid.balance", "sum")
            .expect("declares");
        assert!(typecheck_holon(&parent, &[]).is_ok());
    }
}
