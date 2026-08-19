//! The wire contract — Rust types the TypeScript is **generated from**.
//!
//! ## ★★★ Why these live here and not in `sustena-core`
//!
//! The obvious move is to derive `specta::Type` directly on the engine's own
//! `Execution` / `OperatorResult` and export those. **ADR-0001 forbids it**:
//! the core's dependencies are fixed at `serde`, `serde_json` and `thiserror`
//! so it compiles unchanged to WASM, Android, iOS and desktop. Adding a
//! codegen crate to the engine to make a *desktop UI* convenient would be the
//! host's problem leaking into the portable core.
//!
//! ★★ And on inspection that constraint is pointing at the right design
//! anyway. The core has **no I/O and no opinion about a wire format** — which
//! is exactly what a DTO is. Putting the boundary here means:
//!
//! - the engine's internal types stay free to change without breaking a
//!   published wire shape, and a change that *should* break the UI does so at
//!   **this file**, in Rust, where it is one visible conversion rather than a
//!   silent drift;
//! - the conversion is ordinary Rust the compiler checks end to end, so the
//!   typed-boundary property the stack doc asks for is genuinely held — just
//!   held one layer out;
//! - nothing accidentally publishes an engine internal as public API by
//!   forgetting to think about it.
//!
//! ★ The cost, stated: these are hand-written and could drift from the core
//! types they mirror. That is real, and it is bounded by `From` impls — a core
//! field that vanishes fails to compile here.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;
use sustena_core::{
    editing::Definition,
    operator::{meta::OperatorStatus, EmittedEvent, Execution, OperatorResult},
};

use crate::templates::TemplateId;
use crate::world::Sustain;

/// What the gate decided about one call.
///
/// ★★ `Refused` is a **first-class variant, not an error**. A refusal is a
/// normal, correct outcome of asking the engine to do something the household's
/// rules forbid, and modelling it as an `Err` would push it into the UI's error
/// path — where it would render as *something went wrong* rather than *the
/// system did its job*.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Committed. The state moved.
    Admitted,
    /// The gate said no. **Nothing changed.**
    Refused,
    /// Awaiting a Council vote — not a failure, a decision not yet made.
    Deferred,
}

impl From<OperatorStatus> for Verdict {
    fn from(s: OperatorStatus) -> Self {
        match s {
            OperatorStatus::Ok => Verdict::Admitted,
            OperatorStatus::Failed => Verdict::Refused,
            OperatorStatus::Deferred => Verdict::Deferred,
        }
    }
}

/// One event the call published.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EventDto {
    pub name: String,
    pub payload: serde_json::Value,
}

impl From<&EmittedEvent> for EventDto {
    fn from(e: &EmittedEvent) -> Self {
        EventDto { name: e.name.clone(), payload: e.payload.clone() }
    }
}

/// The gate's own words about one call.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub verdict: Verdict,
    /// The operator that was asked for.
    pub operator: String,
    /// ★ The refusal reason, **as the engine wrote it**. Never re-phrased here:
    /// the engine names the rule and the numbers, and a host that softened that
    /// into "could not complete" would be hiding the only useful part.
    pub reason: Option<String>,
    /// Which rule refused it — a guard expression, or `enforcement_gate`.
    pub constraint_violated: Option<String>,
    /// Empty on a refusal, always — a refusal changes nothing.
    pub mutations: u32,
    pub events: Vec<EventDto>,
    /// The resulting state, or the **untouched original** if refused.
    pub state: serde_json::Value,
}

impl GateResult {
    pub fn of(operator: &str, x: &Execution) -> Self {
        let OperatorResult { status, reason, constraint_violated, .. } = &x.result;
        GateResult {
            verdict: Verdict::from(*status),
            operator: operator.to_string(),
            reason: reason.clone(),
            constraint_violated: constraint_violated.clone(),
            mutations: x.mutations.len() as u32,
            events: x.events.iter().map(EventDto::from).collect(),
            state: x.state.clone(),
        }
    }
}

/// A rule the household declared it must stay within.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvariantDto {
    pub id: String,
    pub expression: String,
}

/// `Σ = ⟨B, S, V, T, ⊕⟩`, as much of it as a cockpit needs to draw.
///
/// ★ Deliberately not the whole `Definition`: a walking skeleton should carry
/// what it actually renders, and inventing fields the UI does not use yet would
/// be publishing a contract nobody has tested.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SustainDto {
    pub id: String,
    pub label: String,
    /// `T` — the moves this Sustain may make.
    pub operators: Vec<String>,
    /// `V` — the viable region.
    pub invariants: Vec<InvariantDto>,
    /// Whether the gate is armed for it.
    pub gate_armed: bool,
    /// `S` — current state.
    pub state: serde_json::Value,
}

impl SustainDto {
    pub fn of(id: &str, label: &str, d: &Definition, gate_armed: bool, state: &serde_json::Value) -> Self {
        SustainDto {
            id: id.to_string(),
            label: label.to_string(),
            operators: d.operators.clone(),
            invariants: d
                .invariants
                .iter()
                .map(|(id, expression)| InvariantDto {
                    id: id.clone(),
                    expression: expression.clone(),
                })
                .collect(),
            gate_armed,
            state: state.clone(),
        }
    }
}

// ── the household ────────────────────────────────────────────────────────────

/// One Sustain, as a selector row needs it.
///
/// ★ Deliberately not the full `SustainDto`: a list of six should not carry six
/// full state documents, and a shape that made it easy to would invite it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SustainSummary {
    pub id: String,
    pub label: String,
    pub template: TemplateId,
    /// `⊕` — its parent, if it has one.
    pub parent: Option<String>,
    /// How many events its log holds. ★ The real number, folded from disk —
    /// this is what makes persistence visible rather than claimed.
    pub events: u32,
    /// Its liquid balance, or `null` when the Sustain declares no such
    /// dimension. **Never 0 for absent** — a missing figure renders as "—".
    pub liquid: Option<f64>,
}

impl SustainSummary {
    pub fn of(s: &Sustain) -> Self {
        SustainSummary {
            id: s.record.id.clone(),
            label: s.record.label.clone(),
            template: s.record.template,
            parent: s.record.parent.clone(),
            events: s.next_seq as u32,
            liquid: s
                .state
                .pointer("/finances/liquid/balance")
                .and_then(serde_json::Value::as_f64),
        }
    }
}

/// Whether the composition tree holds, **as the engine judges it**.
///
/// ★ A sum type rather than a bool: a tree that does not hold should say which
/// link broke it, and a `false` with the reason thrown away is an alarm rather
/// than a diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Holarchy {
    /// `MonitorEngine::flatten_holarchy` accepted it — no duplicate id, no
    /// unknown parent, no cycle.
    Holds { linked: u32 },
    /// It refused, and here is what it said.
    Broken { reason: String },
}

/// What the cockpit knows about the whole household.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldDto {
    pub sustains: Vec<SustainSummary>,
    pub selected: Option<String>,
    /// Where the log actually lives, shown so persistence is inspectable
    /// rather than a promise.
    pub store_path: String,
    /// ★★ `⊕` validated by the engine (`MonitorEngine::flatten_holarchy`).
    pub holarchy: Holarchy,
    /// ★★★ **NOT AVAILABLE, and said so.** Roll-up `ρ` — folding children's
    /// state into a parent aggregate — does **not exist in `sustena-core`**
    /// (the Python engine has it; the Rust port does not, per the UX spec's
    /// §9.2 gap list). The composition tree here is real and engine-checked;
    /// the *aggregate over it* is not computed, and the UI renders an honest
    /// unavailable state rather than summing the children in the host and
    /// passing host arithmetic off as an engine capability.
    pub rollup_available: bool,
}

// ── what the engine says about V, right now ──────────────────────────────────

/// One invariant, evaluated against current state **by the engine**.
///
/// ★★ `holds` and `reason` come from `sustena_core::predicate::check`, the same
/// evaluator the gate uses. The host does not judge a rule; it asks.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintReading {
    pub id: String,
    pub expression: String,
    pub holds: bool,
    /// ★ Empty when it holds; otherwise the engine's own words, naming the
    /// operand and the value that failed.
    pub reason: String,
}

/// One line of the persisted log, as a person reads it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LogEntryDto {
    pub seq: u32,
    /// The operator that caused it, or `genesis` for the opening line.
    pub operator: String,
    pub events: Vec<EventDto>,
    /// ★ How many state changes it carried. The mutations themselves stay on
    /// disk — a log view is for reading what happened, and a wall of paths is
    /// not that.
    pub mutations: u32,
}

// ── the push channel ─────────────────────────────────────────────────────────

/// ★★★ **A committed change, pushed.** One message per real change.
///
/// The event *is* the message: there is no tick, no sampler and no clock,
/// because the core has none and a sampler would manufacture events nothing
/// caused. A **refusal emits nothing** — the fold stays the truth, and a
/// message saying "nothing happened" would be a change that did not happen.
///
/// ★★ It carries the new state as well as the events, so a subscriber is
/// correct after a single message without replaying anything. The honest cost
/// is size: a large Sustain ships its whole state per change. That is fine for
/// a household and is named in the README rather than discovered later.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct Committed {
    pub sustain_id: String,
    pub operator: String,
    /// Its position in the append-only log — the same `seq` on disk.
    pub seq: u32,
    pub events: Vec<EventDto>,
    pub mutations: u32,
    pub state: serde_json::Value,
    /// `V` re-evaluated after the change, by the engine.
    pub constraints: Vec<ConstraintReading>,
    pub liquid: Option<f64>,
}
