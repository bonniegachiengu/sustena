//! The engine, held by the host.
//!
//! ★★★ **This is the same call path `sustena-core/examples/household_run.rs`
//! walks** — `Registry::default()`, a `Definition`, `enforcement_of`, and
//! `execute_admitted`. Nothing about being behind a UI changes how the engine
//! is asked; the cockpit is a caller, not a special case.
//!
//! ★★ The state lives in a `Mutex` in the host, not in the core. The core has
//! no I/O and no globals by ADR-0001 — *who holds the state* is exactly the
//! host's job, and this is the smallest honest version of it. Durability is
//! **not built** (see the skeleton's stated gaps): quitting the app forgets
//! the household.

use std::sync::Mutex;

use serde_json::{json, Map, Value};
use sustena_core::{
    approval::{EffectClass, NonceLedger},
    editing::Definition,
    operator::{execute_admitted, Authorization, Enforcement, Execution, Registry},
    schema::{DimType, Schema},
    semantic::enforcement_of,
};

pub const SUSTAIN_ID: &str = "household.demo";
pub const SUSTAIN_LABEL: &str = "Household";

/// `Σ` — the household's declared rules.
///
/// ★ Identical in shape to the example's, so what the cockpit shows and what
/// the terminal walk shows are the same Sustain rather than two stories.
pub fn definition() -> Definition {
    Definition::new(Schema::new().declare("finances", DimType::Any))
        .with_operator("budget.record_income")
        .with_operator("budget.add_pocket")
        .with_operator("budget.allocate")
        .with_operator("budget.spend")
        .with_invariant("liquid_non_negative", "finances.liquid.balance >= 0")
        .with_invariant(
            "food_not_overspent",
            "finances.pockets.food.spent <= finances.pockets.food.allocated",
        )
}

/// ★ Seeded, not conjured: an invariant over a path the state does not hold
/// compares `None` to a number, which is a type error the gate then refuses
/// *everything* on. The treasury row learned this the hard way.
pub fn opening_state() -> Value {
    json!({
        "finances": {
            "liquid": {"balance": 0.0},
            "pockets": {"food": {"allocated": 0.0, "spent": 0.0, "limit": 0.0}},
            "income": {"monthly_total": 0.0, "sources": []}
        }
    })
}

/// The host's live Sustain.
pub struct Engine {
    pub registry: Registry,
    pub definition: Definition,
    pub enforcement: Enforcement,
    pub state: Mutex<Value>,
}

impl Default for Engine {
    fn default() -> Self {
        let definition = definition();
        let enforcement = enforcement_of(&definition);
        Engine {
            registry: Registry::default(),
            definition,
            enforcement,
            state: Mutex::new(opening_state()),
        }
    }
}

impl Engine {
    pub fn snapshot(&self) -> Value {
        self.state.lock().expect("state lock").clone()
    }

    /// ★★★ One real call, through the real gate.
    ///
    /// The `Execution` comes back whatever it decided, and the **state is only
    /// replaced when it committed** — which is belt-and-braces, since the
    /// engine already returns the untouched original on a refusal. Two things
    /// agreeing is cheaper than one of them being the only thing standing
    /// between a refusal and the ledger.
    pub fn call(&self, operator: &str, params: &Map<String, Value>) -> Execution {
        let before = self.snapshot();
        let x = execute_admitted(
            &self.registry,
            &self.definition.operators,
            &self.enforcement,
            &before,
            operator,
            params,
            &Authorization::Unchecked,
            &EffectClass::Unchecked,
            &mut NonceLedger::new(),
        );
        if x.committed() {
            *self.state.lock().expect("state lock") = x.state.clone();
        }
        x
    }

    /// Reset to the opening state — the cockpit's own *start over*.
    pub fn reset(&self) {
        *self.state.lock().expect("state lock") = opening_state();
    }
}
