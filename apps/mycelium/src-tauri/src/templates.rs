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
    /// ★★★ A capture device — the phone itself, as a Sustain.
    ///
    /// Ingest §IX: a sensor IS a Sustain, so noticing it has gone quiet needs
    /// no alerting subsystem of its own. Staleness becomes an ordinary reading
    /// of an ordinary child, and the connection panel is a view over its state
    /// rather than a separate thing to build.
    Device,
}

impl TemplateId {
    pub fn label(&self) -> &'static str {
        match self {
            TemplateId::Homestead => "homestead",
            TemplateId::Habitat => "habitat",
            TemplateId::Device => "device",
        }
    }

    pub fn all() -> Vec<TemplateId> {
        vec![TemplateId::Homestead, TemplateId::Habitat, TemplateId::Device]
    }
}

/// `Σ` for a device: what it knows about itself, and nothing else.
///
/// ★★ No money dimensions, and no `liquid_non_negative`. A phone holds no
/// money, and giving it the household's shape so the code could be shared
/// would put a balance on a thing that has none.
///
/// ★★★ **No liveness invariant here, deliberately.** §IX's `now − t_last_ack
/// <= theta` cannot be a rule the device carries: a phone that has stopped
/// cannot evaluate anything, least of all whether it has stopped. It is
/// evaluated by whoever is watching, over this child's own dimensions — which
/// is why `queue_depth` and `last_ack_ms` are recorded here and judged nowhere
/// near here.
fn device_definition() -> Definition {
    Definition::new(Schema::new().declare("device", DimType::Any))
        .with_operator("device.heartbeat")
}

/// `Σ` for a template.
pub fn definition(template: TemplateId) -> Definition {
    let base = Definition::new(Schema::new().declare("finances", DimType::Any))
        .with_operator("budget.record_income")
        .with_operator("budget.add_pocket")
        .with_operator("budget.allocate")
        .with_operator("budget.spend")
        // ★★ The declared inverses of the two moves above. A Sustain that can
        //    spend must be able to record a refund of that spend, or a real
        //    reversal has nowhere to go but a wrong number.
        .with_operator("budget.unspend")
        .with_operator("budget.unallocate")
        // ★★ Where the money is, as opposed to what it is for. A household
        //    holding money in two places cannot answer "how much do I have in
        //    M-Pesa" from a single pooled figure.
        .with_operator("budget.open_account")
        .with_operator("budget.transfer")
        // ★★ The inverse of income. Money arriving from his own other account
        //    reads exactly like earnings, so it is filed as income before
        //    anything can tell; when the other half says otherwise it has to
        //    come back off, or his earnings grow every time he moves his money.
        .with_operator("budget.unrecord_income")
        // Every Sustain that holds money holds this one.
        .with_invariant("liquid_non_negative", "finances.liquid.balance >= 0");

    if let TemplateId::Device = template {
        return device_definition();
    }
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
        TemplateId::Device => unreachable!("device returns before this match"),
    }
}

/// The opening state a template instantiates with.
pub fn opening_state(template: TemplateId) -> Value {
    match template {
        TemplateId::Homestead => json!({
            "finances": {
                "liquid": {"balance": 0.0},
                "pockets": {"food": {"allocated": 0.0, "spent": 0.0, "limit": 0.0}},
                // ★ Empty rather than pre-named. Which accounts someone holds
                //   is theirs to say, and the first text that arrives opens the
                //   one it came from anyway.
                "accounts": {},
                "income": {"monthly_total": 0.0, "sources": []}
            }
        }),
        TemplateId::Habitat => json!({
            "finances": {
                "liquid": {"balance": 0.0},
                "pockets": {},
                "accounts": {},
                "income": {"monthly_total": 0.0, "sources": []}
            }
        }),
        // ★ Zero and zero, not absent. A device that has never reported has a
        //   real queue of nothing and a real last-contact of never, and both
        //   are readings a watcher can act on.
        TemplateId::Device => json!({
            "device": {"queue_depth": 0.0, "last_ack_ms": 0.0, "app_version": ""}
        }),
    }
}
