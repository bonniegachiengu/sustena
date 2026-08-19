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

/// A pocket, at summary scale.
///
/// ★ Small on purpose. The Constellation reads summaries for every Sustain and
/// hydrates full state only on drill-in, so this carries the three numbers a
/// vitals reading needs and not the document they came from.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PocketSummary {
    pub name: String,
    pub allocated: f64,
    pub spent: f64,
}

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
    /// Its pockets, at summary scale.
    pub pockets: Vec<PocketSummary>,
    /// ★★ `V` for THIS Sustain, evaluated by the engine — so a constellation
    /// can show a broken rule anywhere without hydrating anything.
    pub constraints: Vec<ConstraintReading>,
}

impl SustainSummary {
    pub fn of(s: &Sustain, constraints: Vec<ConstraintReading>) -> Self {
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
            pockets: s
                .state
                .pointer("/finances/pockets")
                .and_then(serde_json::Value::as_object)
                .map(|m| {
                    m.iter()
                        .map(|(name, p)| PocketSummary {
                            name: name.clone(),
                            allocated: p.get("allocated").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                            spent: p.get("spent").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            constraints,
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
    /// ★ The local principal, as declared by the host.
    ///
    /// ★★ **Not yet bound to anything the engine checks.** Every call still runs
    /// under `Authorization::Unchecked`; this is an identity the app displays,
    /// not one the gate enforces. Naming that here keeps a later capability
    /// slice honest about what it is actually adding.
    pub principal: String,
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

/// ★★★ **A refusal, pushed — and it carries NO STATE.**
///
/// A separate type from [`Committed`] on purpose. The two answer different
/// questions: *what changed* and *what was asked and declined*. Collapsing them
/// into one message with a verdict flag would make it possible for a consumer
/// to treat a refusal as a change by forgetting to branch — here there is no
/// `state` field to read, so that mistake is unspellable.
///
/// ★★ This does not weaken V1.2's rule. **A refusal still emits no state
/// delta**; the fold is still the only truth about state. What travels here is
/// the *activity*: a request happened and the gate declined it, which a person
/// watching a household genuinely wants to see.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct Refused {
    pub sustain_id: String,
    pub operator: String,
    /// The engine's own words.
    pub reason: Option<String>,
    /// Which rule declined it.
    pub constraint_violated: Option<String>,
}

impl Committed {
    /// Build the message a commit pushes.
    ///
    /// ★ Constructed HERE rather than at the emit site, so the headless proof
    /// and the running app produce the byte-identical message. A demo that
    /// built its own copy would be testing the demo.
    pub fn of(
        sustain_id: &str,
        operator: &str,
        seq: u64,
        x: &Execution,
        constraints: Vec<ConstraintReading>,
    ) -> Self {
        Committed {
            sustain_id: sustain_id.to_string(),
            operator: operator.to_string(),
            seq: seq as u32,
            events: x.events.iter().map(EventDto::from).collect(),
            mutations: x.mutations.len() as u32,
            liquid: x
                .state
                .pointer("/finances/liquid/balance")
                .and_then(serde_json::Value::as_f64),
            constraints,
            state: x.state.clone(),
        }
    }
}

impl Refused {
    /// Build the message a refusal pushes. ★ No state, by shape.
    pub fn of(sustain_id: &str, operator: &str, r: &GateResult) -> Self {
        Refused {
            sustain_id: sustain_id.to_string(),
            operator: operator.to_string(),
            reason: r.reason.clone(),
            constraint_violated: r.constraint_violated.clone(),
        }
    }
}

// ── the operator catalogue (Console) ─────────────────────────────────────────

/// One parameter an operator declares.
///
/// ★★ **Declared by the operator, not guessed by the UI.** `ParamDecl` carries
/// the name, the kind, whether it is required, and — for a naming parameter —
/// the state path whose KEYS are the legal values. So a picker can offer real
/// pocket names without this app knowing what a pocket is.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ParamDto {
    pub name: String,
    /// `number` | `text` | `any`.
    pub kind: String,
    pub required: bool,
    /// A state path whose keys are the legal values, when the operator declares one.
    pub names_within: Option<String>,
}

/// What the meter has actually measured for an operator.
///
/// ★★★ `None` means **not measured**, and the UI must say so rather than
/// showing a zero. A `0` from an unmeasured basis is silence, not cheapness.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredPawa {
    pub runs: u32,
    pub mean_pawa: f64,
    pub total_pawa: f64,
    pub total_compute: u32,
    pub total_storage: u32,
}

/// One operator the selected Sustain may actually run.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperatorDto {
    pub name: String,
    pub description: String,
    pub params: Vec<ParamDto>,
    /// The author's declared estimate. ★ Kept beside the measurement precisely
    /// so the two can be compared — the reference's estimate is usually 0.
    pub declared_pawa: u32,
    /// ★ `null` when the meter has never seen it run.
    pub measured: Option<MeasuredPawa>,
    pub side_effects: Vec<String>,
}

// ── simulation ───────────────────────────────────────────────────────────────

/// One step of a hypothetical branch.
///
/// ★★★ **Nothing here was written.** A fork is `state.clone()` plus the same
/// `execute_admitted` a real call uses, so a step refused here would be refused
/// for real — with the same reason. It touches no log, no ledger and no meter.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BranchStep {
    pub operator: String,
    pub verdict: Verdict,
    pub reason: Option<String>,
    pub constraint_violated: Option<String>,
    pub mutations: u32,
    pub events: Vec<EventDto>,
    /// The hypothetical state after this step.
    pub state: serde_json::Value,
}

/// A whole hypothetical branch.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub sustain_id: String,
    pub steps: Vec<BranchStep>,
    /// ★ Always true. Carried so a surface cannot render a branch without being
    /// handed the fact that it is one.
    pub hypothetical: bool,
    /// The state the branch started from, for a diff.
    pub from: serde_json::Value,
}

// ── the economy ──────────────────────────────────────────────────────────────

/// One line of the juul ledger.
///
/// ★★ The **kind** is the entry's own variant, not a sign on a number: a mint
/// names the declaration that authorised it, a debit names the run it paid for,
/// a transfer names both ends. Collapsing them would make *where did this juul
/// come from* unanswerable.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerEntryDto {
    /// `mint` | `debit` | `transfer`.
    pub kind: String,
    pub principal: String,
    pub counterparty: Option<String>,
    pub amount: f64,
    /// For a mint: the declaration that authorised it. For a debit: the operator.
    pub authority: String,
    /// Its effect on total circulation — `+` for a mint, `-` for a debit, `0` for a transfer.
    pub circulation_delta: f64,
}

/// One governed parameter and its declared bounds.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ParameterDto {
    pub name: String,
    pub value: f64,
    pub genesis: f64,
    pub min: f64,
    pub max: f64,
}

/// The whole economy, as one screen reads it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EconomyDto {
    pub principal: String,
    pub balance: f64,
    /// `Sigma(mints) - Sigma(debits)`, transfers at zero.
    pub circulation: f64,
    pub minted_total: f64,
    pub issued_total: f64,
    pub genesis_id: String,
    pub genesis_total: f64,
    /// ★ The audit is the engine's, not the host's: it names foreign mints,
    /// mismatched allocations and undeclared principals rather than returning a
    /// boolean.
    pub audit_clean: bool,
    pub audit_describes: String,
    pub entries: Vec<LedgerEntryDto>,
    pub parameters: Vec<ParameterDto>,
    /// Every reading the meter has taken, by operator.
    pub metered: Vec<(String, MeasuredPawa)>,
    /// ★★★ The permanent boundary, carried as data so a surface cannot forget
    /// to show it.
    pub boundary_notice: String,
}

// ── access (Profile) ─────────────────────────────────────────────────────────

/// Whether the principal may run one operator, as the engine judges it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperatorAccessDto {
    pub operator: String,
    /// The operator's declared `min_privilege`. ★ Lower is more privileged:
    /// 0 = owner, 3 = observer.
    pub required_tier: u8,
    pub permitted: bool,
    /// The engine's own denial text when it is not.
    pub denial: Option<String>,
}

/// The principal's authority over one Sustain.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccessDto {
    pub principal: String,
    pub sustain_id: String,
    /// Effective privilege after the weakest-link path walk. `null` when the
    /// principal has no membership at all.
    pub tier: Option<u8>,
    pub memberships: u32,
    pub operators: Vec<OperatorAccessDto>,
    /// ★★★ Whether the gate is currently checking any of this. It is not.
    pub enforced: bool,
    pub note: String,
}

// ── council ──────────────────────────────────────────────────────────────────

/// What `council::resolve` decided.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CouncilOutcomeDto {
    /// `Passed` | `Failed` | `InVoting` | `OverriddenByUser` | …
    pub status: String,
    /// The councillor's single vote after `aggregate_delegated_votes`.
    pub aggregated: String,
    /// The engine's own explanation of the aggregation.
    pub reasoning: String,
    pub utility: f64,
    pub counted: u32,
}
