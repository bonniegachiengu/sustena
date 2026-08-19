//! The Sustain templates the cockpit can instantiate.
//!
//! ★★ **Declared in the host, deliberately.** `sustena-core` has no template
//! registry — a `Definition` is a value you build, and *which* definitions a
//! product offers is a product decision, not an engine one. Authoring a new
//! template from inside the app is the Define screen (V1.5); until then the two
//! the household actually needs live here, in Rust, where the compiler checks
//! them.
//!
//! ★ Both are the real thing: an ordinary `Definition` with `T` and `V`, armed
//! by the ordinary `enforcement_of`, so nothing about being a "template"
//! changes how the gate reads them.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use sustena_core::{
    editing::Definition,
    schema::{DimType, Schema},
};

/// Which kind of Sustain to instantiate.
///
/// ★ A closed enum rather than a string: a template the app cannot build is
/// unrepresentable, so `instantiate` has no "unknown template" failure to
/// invent a message for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TemplateId {
    /// The household — a parent Sustain, composed of habitats.
    Homestead,
    /// A person — the smallest Sustain that still holds its own money.
    Habitat,
}

impl TemplateId {
    pub fn label(&self) -> &'static str {
        match self {
            TemplateId::Homestead => "homestead",
            TemplateId::Habitat => "habitat",
        }
    }

    pub fn all() -> Vec<TemplateId> {
        vec![TemplateId::Homestead, TemplateId::Habitat]
    }
}

/// `Σ` for a template.
pub fn definition(template: TemplateId) -> Definition {
    let base = Definition::new(Schema::new().declare("finances", DimType::Any))
        .with_operator("budget.record_income")
        .with_operator("budget.add_pocket")
        .with_operator("budget.allocate")
        .with_operator("budget.spend")
        // Every Sustain that holds money holds this one.
        .with_invariant("liquid_non_negative", "finances.liquid.balance >= 0");

    match template {
        // ★ The household carries a shared pocket and the rule that guards it,
        //   and it is the one that declares what gets TOTALLED across the
        //   holarchy — roll-up ρ's declarations, part of `Σ` rather than of
        //   this app.
        //
        // ★★ Both totals include the household's OWN state, not only its
        //   members'. A "household total" that skipped the household would
        //   answer *what your members collectively hold*, which is a different
        //   question from the one the label asks — the reference shipped it
        //   children-only and corrected it for exactly that reason.
        TemplateId::Homestead => base
            .with_invariant(
                "food_not_overspent",
                "finances.pockets.food.spent <= finances.pockets.food.allocated",
            )
            .with_aggregate("household_liquid_total", "finances.liquid.balance", "sum")
            .expect("a declared aggregate this crate wrote itself")
            .with_aggregate(
                "household_pockets_total",
                "finances.pockets[*].allocated",
                "sum",
            )
            .expect("a declared aggregate this crate wrote itself"),
        // ★ A habitat declares no aggregate either: it has no children, and a
        //   "household total" over one person is that person's balance under a
        //   grander name. Declaring one would put a card on screen that only
        //   ever restated a figure already above it.
        //
        // ★ A habitat declares NO pocket invariant, because it opens with no
        //   pockets — and an invariant over a path the state does not hold
        //   compares `None` to a number, which is a type error the gate then
        //   refuses *everything* on. Declaring rules for dimensions that do not
        //   exist yet is how a Sustain bricks itself.
        TemplateId::Habitat => base,
    }
}

/// The opening state a template instantiates with.
pub fn opening_state(template: TemplateId) -> Value {
    match template {
        TemplateId::Homestead => json!({
            "finances": {
                "liquid": {"balance": 0.0},
                "pockets": {"food": {"allocated": 0.0, "spent": 0.0, "limit": 0.0}},
                "income": {"monthly_total": 0.0, "sources": []}
            }
        }),
        TemplateId::Habitat => json!({
            "finances": {
                "liquid": {"balance": 0.0},
                "pockets": {},
                "income": {"monthly_total": 0.0, "sources": []}
            }
        }),
    }
}
