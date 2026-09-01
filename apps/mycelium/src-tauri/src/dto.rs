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
    Rollup,
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
///
/// ★ `PartialEq` because a log line is compared for equality by the
/// replicated log: two nodes claiming one stamp must be told apart by their
/// CONTENT, and that comparison reaches all the way down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
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
    /// ★★★ What the operator returned alongside its verdict.
    ///
    /// A refusal from `budget.spend` carries `remaining`, `requested` and
    /// `shortfall`, put there so a caller can build an allocate-then-retry
    /// without reading an English sentence. It was being dropped here, which
    /// left the surface with a dead end and a paragraph.
    pub data: serde_json::Value,
}

impl GateResult {
    pub fn of(operator: &str, x: &Execution) -> Self {
        let OperatorResult { status, reason, constraint_violated, data, .. } = &x.result;
        GateResult {
            verdict: Verdict::from(*status),
            operator: operator.to_string(),
            reason: reason.clone(),
            constraint_violated: constraint_violated.clone(),
            mutations: x.mutations.len() as u32,
            events: x.events.iter().map(EventDto::from).collect(),
            state: x.state.clone(),
            data: data.clone(),
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

/// ★★★ **State that ARRIVED, pushed — Multiparty §III.**
///
/// A local commit relays a pulse through [`Committed`]. State that arrives from
/// a peer had no local cause at all, so without this it reached the store and
/// no surface ever heard: the household changed and the screen kept showing
/// what it had. §III is the mechanism the article already gives for this — a
/// relayed signal, refractory behind it — and the refractory half is what stops
/// the arriving pulse from being relayed back at the node that sent it.
///
/// ★★ It carries the folded state, exactly as `Committed` does, so a view
/// updates from the message rather than by asking the engine. A merge that made
/// every surface re-query would be a poll with extra steps.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct Merged {
    pub sustain_id: String,
    /// How many entries the peer gave us. ★ Zero is not emitted: a sync that
    /// changed nothing is not a change.
    pub entries: u32,
    /// Who it came from, for a surface that wants to say so.
    pub peer: Option<String>,
    pub state: serde_json::Value,
    pub constraints: Vec<ConstraintReading>,
    pub liquid: Option<f64>,
    /// Total entries in the log after the merge.
    pub events: u32,
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

// ── roll-up ρ ────────────────────────────────────────────────────────────────

/// One contributor to a household total.
///
/// ★★ `isHousehold` is a real distinction, not decoration: a surface has to be
/// able to say *the household's own 6,600 plus six members* rather than listing
/// seven anonymous numbers, and inferring it by comparing ids would be the kind
/// of guess this layer exists to remove.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ContributionDto {
    pub sustain_id: String,
    /// The member's name, else the slot, else the id.
    pub label: String,
    pub is_household: bool,
    pub value: f64,
}

/// A contributor that could NOT be read, and the engine's own reason.
///
/// ★★★ This is the honest-exclusion contract on the wire. A total that dropped
/// a member silently would be indistinguishable from one where that member
/// genuinely holds nothing — so the exclusions travel with the value, and a
/// screen showing one without the other is misreporting on its own account.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExclusionDto {
    pub sustain_id: String,
    pub label: String,
    pub is_household: bool,
    pub reason: String,
}

/// One declared aggregate, answered by the engine.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AggregateDto {
    pub id: String,
    pub child_path: String,
    /// `SUM` | `COUNT` | `AVG` | `MIN` | `MAX`.
    pub op: String,
    /// ★ `null` for `MIN`/`MAX` over nothing — there is no smallest element of
    /// an empty set, and `0` would be a claim.
    pub value: Option<f64>,
    /// ★ Whether ANYTHING was readable. `sum` over nothing is `0`, which is
    /// arithmetically right and still not a measurement — a surface uses this
    /// to say so rather than print a confident zero.
    pub grounded: bool,
    pub included: Vec<ContributionDto>,
    pub excluded: Vec<ExclusionDto>,
    pub includes_household_own: bool,
}

/// **ρ** for one parent, computed fresh.
///
/// ★ `declared` is empty for a Sustain that asked for no totals — which is a
/// different fact from *a total that could not be computed*, and the two must
/// not render the same.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RollupDto {
    pub sustain_id: String,
    pub aggregates: Vec<AggregateDto>,
    /// Linked children, and whether each was readable at all.
    pub children: Vec<ChildStatusDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChildStatusDto {
    pub sustain_id: String,
    pub label: String,
    pub readable: bool,
}

impl RollupDto {
    /// Build the wire form from the engine's own reading.
    ///
    /// ★ Built here, like `Committed::of`, so the command and the push event
    /// ship the byte-identical shape.
    pub fn of(sustain_id: &str, r: &Rollup) -> Self {
        RollupDto {
            sustain_id: sustain_id.to_string(),
            children: r
                .children()
                .iter()
                .map(|c| ChildStatusDto {
                    sustain_id: c.contributor().sustain_id().to_string(),
                    label: c.contributor().label().to_string(),
                    readable: c.readable(),
                })
                .collect(),
            aggregates: r
                .aggregates()
                .iter()
                .map(|a| AggregateDto {
                    id: a.id().to_string(),
                    child_path: a.child_path().to_string(),
                    op: a.op().name().to_string(),
                    value: a.value().as_f64(),
                    grounded: a.is_grounded(),
                    includes_household_own: a.includes_household_own(),
                    included: a
                        .included()
                        .iter()
                        .map(|c| ContributionDto {
                            sustain_id: c.contributor().sustain_id().to_string(),
                            label: c.contributor().label().to_string(),
                            is_household: c.contributor().is_household(),
                            value: c.value(),
                        })
                        .collect(),
                    excluded: a
                        .excluded()
                        .iter()
                        .map(|e| ExclusionDto {
                            sustain_id: e.contributor().sustain_id().to_string(),
                            label: e.contributor().label().to_string(),
                            is_household: e.contributor().is_household(),
                            reason: e.reason().to_string(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

/// ★★★ **A household total, recomputed and pushed.**
///
/// A THIRD typed event, for the same reason there is a second: it answers a
/// third question. `Committed` says *what changed, in the Sustain that
/// changed*; ρ is a **different Sustain's** derived reading, and folding it
/// into `Committed` would mean a message about Bonnie's habitat carrying the
/// homestead's state under a field name that did not say so.
///
/// ★★ It is emitted only after a real commit, only for a Sustain that actually
/// declares aggregates. A refusal emits nothing here either — ρ is a function
/// of state, and a refused call changed no state, so the total it would carry
/// is the one the subscriber already has.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct RolledUp {
    pub rollup: RollupDto,
}

impl RolledUp {
    pub fn of(sustain_id: &str, r: &Rollup) -> Self {
        RolledUp {
            rollup: RollupDto::of(sustain_id, r),
        }
    }
}

// ── the cross-holon transfer ─────────────────────────────────────────────────

/// One side of a settled transfer, as the cockpit shows it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferLegDto {
    pub sustain_id: String,
    pub balance_before: f64,
    pub balance_after: f64,
}

/// What the gate decided about a transfer.
///
/// ★★★ The two variants carry different shapes on purpose, exactly as
/// `Committed`/`Refused` do: a refusal has **no legs field at all**, so a
/// surface cannot render half a transfer by forgetting to branch.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TransferResult {
    /// ★★ `totalBefore == totalAfter` is carried, not computed by the UI —
    /// conservation is the engine's claim and the screen quotes it.
    ///
    /// ★ The per-variant `rename_all` is not decoration: an enum-level one
    /// renames the VARIANTS and leaves struct-variant fields alone, which the
    /// generated boundary caught the first time the UI read `totalBefore` and
    /// got `total_before`. Exactly what the typed seam is for.
    #[serde(rename_all = "camelCase")]
    Committed {
        path: String,
        amount: f64,
        from: TransferLegDto,
        to: TransferLegDto,
        total_before: f64,
        total_after: f64,
    },
    #[serde(rename_all = "camelCase")]
    Refused {
        rule: String,
        reason: String,
    },
}

// ── identity ──────────────────────────────────────────────────

/// Who this machine is, and whether the key is in memory.
///
/// ★★ `handle` is readable while LOCKED — a person should be able to see which
/// identity they are being asked to unlock. `publicKey` is not: it is derived
/// from the private half, and having it means the unlock genuinely happened.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IdentityDto {
    /// Whether an identity exists on this machine at all.
    pub enrolled: bool,
    /// Whether the private key is in memory right now.
    pub unlocked: bool,
    pub handle: Option<String>,
    /// The verifying key, hex — present only once unlocked.
    pub public_key: Option<String>,
    /// The KDF actually in force, named rather than assumed.
    pub kdf: Option<String>,
    pub iterations: Option<u32>,
    /// Whether this node comes up unlocked without being asked.
    ///
    /// ★★★ Surfaced because its ABSENCE caused a real failure: an app
    /// restarted for an update came back locked, a locked node cannot peer,
    /// and the household stopped syncing with nobody able to tell why. A
    /// setting that only exists in the engine is a setting nobody can use.
    pub unlock_remembered: bool,
}

// ── ingest ──────────────────────────────────────────────────

/// One captured message.
///
/// ★★★ A REJECTED message never appears here, because it never reached the
/// store. The queue counts them and holds none of them.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub id: String,
    pub source_id: String,
    /// `mapped` | `parsed_unmapped` | `informational` | `unparsed`.
    pub status: String,
    /// Which declared rule handled it — empty when none did.
    pub parser_name: String,
    /// The engine's own words about why this is what it is.
    pub reason: String,
    /// The raw text. Retained deliberately: a person correcting a
    /// classification needs to see what actually arrived.
    pub raw_payload: String,
    pub amount: Option<f64>,
    pub counterparty: Option<String>,
    pub direction: Option<String>,
    pub external_ref: Option<String>,
    pub operator: Option<String>,
    pub applied: bool,
    /// What the gate said, when a mapped message was refused.
    pub gate_reason: Option<String>,
    pub resolved: bool,
    pub needs_attention: bool,
}

/// A declared capture source.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceDto {
    pub id: String,
    pub label: String,
    pub captures: u32,
    /// ★ `null` when this source declared no cadence — and then `stale` is
    /// `false`, never a guess.
    pub expected_interval_minutes: Option<u32>,
    pub ever_seen: bool,
}

/// One declared parse rule, for the library view.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    pub id: String,
    pub source: String,
    pub version: u32,
    /// `mapped` | `parsed_unmapped` | `informational`.
    pub status: String,
    pub operator: Option<String>,
    /// `shipped` | `user_corrected` | `proposed_confirmed`.
    pub trust: String,
    pub examples: u32,
}

/// The whole ingest picture for one Sustain.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IngestDto {
    pub messages: Vec<MessageDto>,
    pub sources: Vec<SourceDto>,
    pub rules: Vec<RuleDto>,
    /// ★★★ How many messages were refused for carrying a secret. A COUNT,
    /// and nothing else — the messages themselves were never written.
    pub rejected: u32,
    pub needs_attention: u32,
}

/// What one capture did, on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CaptureResult {
    /// ★★★ No message field at all: there is nothing to render, because
    /// there was nothing to store.
    #[serde(rename_all = "camelCase")]
    Rejected { reason: String },
    /// Older than where this household's record begins.
    ///
    /// ★★★ Like `Rejected`, no message field: nothing was stored. Unlike
    /// `Rejected`, nothing was refused either -- it never crossed the
    /// boundary. The two numbers let a surface say by how much rather than
    /// just "no".
    #[serde(rename_all = "camelCase")]
    /// ★ `f64` rather than `i64` because specta forbids BigInt in the
    /// generated bindings, and unix seconds are exact in a double well past
    /// any date this will run in.
    BeforeStart { at: f64, start: f64 },
    #[serde(rename_all = "camelCase")]
    Duplicate { message: MessageDto },
    #[serde(rename_all = "camelCase")]
    Stored { message: MessageDto },
}

impl MessageDto {
    pub fn of(m: &crate::ingest::IngestedMessage) -> Self {
        let field = |k: &str| m.parsed_fields.get(k).cloned();
        MessageDto {
            id: m.id.clone(),
            source_id: m.source_id.clone(),
            status: m.status.clone(),
            parser_name: m.parser_name.clone(),
            reason: m.reason.clone(),
            raw_payload: m.raw_payload.clone(),
            amount: field("amount").and_then(|v| v.as_f64()),
            counterparty: field("counterparty")
                .and_then(|v| v.as_str().map(str::to_string)),
            direction: field("direction").and_then(|v| v.as_str().map(str::to_string)),
            external_ref: m.external_ref.clone(),
            operator: m.operator.clone(),
            applied: m.applied,
            gate_reason: m.gate_reason.clone(),
            resolved: m.resolved,
            needs_attention: m.needs_attention(),
        }
    }
}

// ── Orchie ──────────────────────────────────────────────────

/// One card the knapsack selected, with everything a person needs to ask
/// **"why am I seeing this?"** and get a true answer.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CardDto {
    pub id: String,
    /// What the surface should draw. An opaque tag from the declaration.
    pub render: String,
    /// ★★ Salience RANK, not array index. Widgets that tie on every
    /// un-gameable key share a rank, and showing one above the other as though
    /// it mattered is exactly the fabricated prominence to avoid.
    pub rank: u32,
    pub urgency: f64,
    /// ★★★ Whether that urgency was MEASURED. A `0` on an undeclared basis is
    /// silence, not safety, and the card says which it is.
    pub measured: bool,
    pub basis: String,
    pub relevance: f64,
    pub score: f64,
    pub cost: u32,
    /// Why it was eligible at all — always-on, or an event that fired.
    pub eligibility: String,
    /// The operators this card may emit, from its own declaration.
    pub emits: Vec<String>,
}

/// A card that withdrew before ranking.
///
/// ★★ Distinct from an exclusion: an excluded card was **considered and
/// outranked** and carries a score; a withdrawn one had **nothing to say** and
/// carries a reason, because no score was ever computed.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct QuietDto {
    pub id: String,
    /// A reason for a withdrawal; a score for an exclusion. Never both.
    pub reason: Option<String>,
    pub score: Option<f64>,
    pub withdrew: bool,
}

/// One standing thing that needs a person, and why.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AttentionDto {
    pub kind: String,
    pub what: String,
    pub why: String,
    pub severity: String,
    /// A captured message this points at, when there is one.
    pub message_id: Option<String>,
}

/// A recurring shape nothing recognises, and what teaching it would buy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShapeOfferDto {
    /// A real message from the cluster, so a person can read what they teach.
    pub example: String,
    /// The id of that message, so teaching it is the ordinary classify flow.
    pub message_id: String,
    /// How many messages share this shape, including the example.
    pub count: u32,
}

/// One figure a person pointed at while teaching a shape.
///
/// ★★★ He points at a number in his own bank's text and says what it is and
/// where it belongs. Nothing here is an operator name or a field path — those
/// are the machine's vocabulary, and he is describing his own money.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrainedFigureDto {
    /// The figure exactly as it appears in the message, e.g. `"5.52"`.
    pub text: String,
    /// `in` | `out` | `fee`.
    pub role: String,
    /// The pocket he said it belongs to.
    pub pocket: String,
}

/// One filing nobody was asked about, and what disagrees with it.
///
/// ★★★ The surface exists so nothing lands in his books silently. Most of
/// these are right, which is exactly why a wrong one is invisible without it:
/// it did not need him, so it never reached him.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AutoFiledDto {
    pub message_id: String,
    /// The text as it arrived.
    pub raw: String,
    /// What was booked, in plain words.
    pub what: String,
    pub operator: String,
    pub amount: f64,
    /// The pocket it went to, when it went to one.
    pub pocket: Option<String>,
    /// When it happened, in the message's own words (`"30/7/26 9:10 PM"`).
    ///
    /// ★★ The text's own date and clock rather than an epoch, for two reasons:
    /// the ordering is already settled by the time this is built, and quoting
    /// what the message says needs no formatting decisions that could differ
    /// from what he would read on his phone.
    pub when: String,
    /// Empty when nothing disagrees — which is most of them, and is the point.
    pub doubts: Vec<String>,
}

/// A thread this household reads money texts from.
///
/// ★★ The shipped two travel in the same shape as the ones a person adds,
/// so nothing downstream can tell them apart -- which is the point: they are
/// the same kind of thing, already there.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDto {
    /// The `source_id` its captures are tagged with, e.g. `equity`.
    pub id: String,
    pub label: String,
    /// Sender strings that belong to it, matched as case-insensitive
    /// substrings because sender ids are not standardised.
    pub senders: Vec<String>,
    /// Whether this one shipped, so the list can say so without deciding
    /// anything by it.
    pub built_in: bool,
}

impl ThreadDto {
    pub fn of(t: &sustena_core::Thread) -> Self {
        let built_in = sustena_core::built_in_threads().iter().any(|b| b.id == t.id);
        Self { id: t.id.clone(), label: t.label.clone(), senders: t.senders.clone(), built_in }
    }
}

/// The whole curated view.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FeedDto {
    pub sustain_id: String,
    pub label: String,
    pub cards: Vec<CardDto>,
    /// ★★★ The queue he can walk, in the order he should meet it.
    ///
    /// Recently answered first — so the back arrow reaches them — then what is
    /// waiting, with anything he deferred at the head of that. ONE card renders
    /// at a time; this is the list it steps through, not a list to display.
    pub queue: Vec<CaptureContextDto>,
    /// ★★★ Shapes this inbox repeats that nothing recognises yet. Teaching
    /// ONE of them teaches every message that shares it -- which is the whole
    /// leverage, and why the count is carried: it is what the answer is worth.
    ///
    /// ★★ Never a guess at meaning. A cluster says "these look alike", never
    /// "these are spends".
    pub shapes: Vec<ShapeOfferDto>,

    /// Where in `queue` the first unanswered message sits, so the card opens
    /// on work rather than on history.
    pub queue_start: u32,
    /// The oldest capture still needing a person, if there is one.
    ///
    /// ★★ ONE message rather than a list. The classify card works the queue one
    /// at a time; a list here would be the unbounded second surface the
    /// attention budget exists to prevent.
    pub queue_head: Option<CaptureContextDto>,
    /// ★ What stayed quiet — withdrawn and excluded alike, each saying which.
    pub quiet: Vec<QuietDto>,
    pub budget: u32,
    pub spent: u32,
    pub candidates_considered: u32,
    /// The projection `compose` reasoned over — exposed so a card can render
    /// the very numbers it was ranked on.
    pub reading: serde_json::Value,
    pub attention: Vec<AttentionDto>,
    /// The calm read: the household's own roll-up ρ, when it declares one.
    pub rollup: Option<RollupDto>,
    pub liquid: Option<f64>,
    /// Where the money is, as against what it is for.
    pub accounts: Vec<AccountDto>,
    /// The phone, as a Sustain the household watches. `None` before it has
    /// ever reported.
    pub device: Option<DeviceDto>,
    /// Where the household is heading, not just where it is. `None` until the
    /// series has a reading in it.
    pub trend: Option<TrendDto>,
    /// What the household holds, grouped by the pocket that bought it.
    pub inventory: Vec<InventoryGroupDto>,
    /// Recent spends, so one filed to the wrong pocket can be reached at all.
    pub filed: Vec<FiledSpendDto>,
    /// ★★★ Which pockets are PEOPLE rather than envelopes.
    ///
    /// A pocket tied to somebody's number behaves differently — it runs both
    /// ways and can sit in his favour — and a screen that draws it identically
    /// to `food` is telling him it is the same kind of thing. Sent as names
    /// rather than as a flag per pocket so any surface that lists pockets can
    /// mark them without a second call.
    pub person_pockets: Vec<String>,
    /// ★★★ Money the household holds that no account claims.
    ///
    /// Zero once every shilling has a place. Non-zero means the pooled balance
    /// has not been migrated yet, and it is shown rather than quietly folded
    /// into a total, because "how much is in M-Pesa" cannot be answered
    /// honestly while some of it is nowhere.
    pub unaccounted: f64,
}

/// The numbers a household calls its own, on the wire.
///
/// ★★★ These cross between this process and its own webview and nowhere else.
/// A phone number and a bank account are the address of a person; there is no
/// code path that sends them off the device, and there should not be one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OwnIdentifiersDto {
    pub mpesa: Vec<String>,
    pub kcb: Vec<String>,
}

/// One move between his own accounts, as it happened.
///
/// ★★★ A count alone said "3 moves were recognised" and nothing about WHICH
/// money, so a person could not check it against anything. These are what make
/// the report auditable rather than merely reassuring.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveLineDto {
    pub from: String,
    pub to: String,
    pub amount: f64,
    /// What the bank took. Zero when the text did not say.
    pub fee: f64,
    /// ★★ True when an income had already been filed for the arriving leg and
    /// was taken back off. Said out loud, because a balance that drops without
    /// explanation is the thing this whole mechanism exists to avoid.
    pub undid_income: bool,
}

/// What a transfer pass did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TransferDto {
    /// Pairs recognised as one move and recorded as one.
    pub moved: u32,
    /// The gate refused it. The pair stays in the queue rather than being
    /// marked done on a move that never landed.
    pub refused: u32,
    /// A leg to one of his own numbers whose other half is not here.
    pub unpaired: u32,
    /// ★★ Messages he had set aside that a shared reference brought back.
    /// Counted so a queue that GREW can say why, the same way one that shrank
    /// does.
    pub reclaimed: u32,
    /// A partner leg that had already been filed as income and could not be
    /// taken back. Reported rather than left as a silent zero.
    pub blocked: u32,
    pub ambiguous: u32,
    /// What actually moved, one line each.
    pub lines: Vec<MoveLineDto>,
}

/// The household's own distance from where it wants to be, over time.
///
/// ★★★ One reading is a number; a series is a story. Monitor §V smooths it so
/// a figure that jitters between reads does not train the eye to ignore it,
/// and §VI watches for the case a threshold cannot see — a household spending
/// slightly over every day for two weeks reads, on any single day, exactly
/// like one that had a bad afternoon and recovered.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrendDto {
    /// `W` — the raw distance to the viable region on this reading.
    pub now: f64,
    /// The smoothed level, which is what salience is read off.
    pub smoothed: f64,
    /// ★★ True when the drift has accumulated past what a single bad day
    /// explains. Not the same as being far away today.
    pub drifting: bool,
    /// Whether that crossing is severe enough to hand to the Controller.
    pub escalates: bool,
    /// ★★★ Constraint health as a colour: `green`, `amber` or `red`.
    ///
    /// Monitor §VII rests on Treisman: some visual attributes are processed in
    /// parallel across the whole field before attention engages, in roughly
    /// 150 to 200ms. Hue is one, and it is the one with a settled three-way
    /// meaning already in the palette. Assigned by the core's own encoder, so
    /// the colour on screen means what the engine meant by it.
    pub health: String,
}

/// How the capture device is doing, as its WATCHER sees it.
///
/// ★★★ Judged here rather than on the device, and that is Ingest §IX rather
/// than a convenience. A phone that has stopped cannot report that it has
/// stopped, so a liveness clause evaluated by the phone is worthless exactly
/// when it matters. The device records two plain readings; whether they add up
/// to "fine" is the watcher's call.
///
/// ★★ Not a declared invariant either, and this is a real limit rather than a
/// choice: the predicate DSL has no arithmetic and no notion of now, so
/// `now − t_last_ack <= theta` cannot be written as a rule. It is computed
/// here, against the same two dimensions a declared rule would have read.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    pub sustain_id: String,
    /// Texts caught but not yet handed over. §IX's leading indicator: it rises
    /// before anything else visibly breaks, because a device that cannot
    /// deliver keeps accepting.
    pub queue_depth: u32,
    /// Minutes since it last said anything. `None` means it never has.
    pub quiet_for_minutes: Option<u32>,
    /// Whether the watcher considers it late.
    pub stale: bool,
    pub app_version: String,
}

/// What "skip all like this" did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkipLearnedDto {
    /// How many already-waiting messages the new rule cleared.
    pub cleared: u32,
    /// ★★ True when nothing could be learned: no rule recognised this message,
    /// so the only shape it could describe is "everything I cannot read" — and
    /// that pile is exactly the one that needs a person's eyes.
    pub unlearnable: bool,
}

/// One thing the household holds, bought with a spend.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AssetDto {
    pub id: String,
    pub item: String,
    pub value: f64,
    /// The purchase it came out of, so a pocket can show what its spending
    /// actually bought.
    pub source_tx: String,
    pub pocket: String,
    pub subpocket: Option<String>,
}

/// What the household holds, and what it is grouped under.
///
/// ★★ Grouped by pocket rather than listed flat, because the question people
/// actually ask is "what did the food money buy", not "what do I own".
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryGroupDto {
    pub pocket: String,
    pub total: f64,
    pub assets: Vec<AssetDto>,
}

/// A spend that landed, and where it currently sits.
///
/// ★★ Read off what the message recorded it DID, so the list is the log's own
/// account of the filing rather than a guess reconstructed from a balance.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FiledSpendDto {
    pub message_id: String,
    pub pocket: String,
    pub amount: f64,
    pub counterparty: String,
}

/// One account, on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountDto {
    pub id: String,
    pub label: String,
    pub balance: f64,
    /// ★★★ What the bank itself last said was in there.
    ///
    /// The only figure in the whole system that is not our own arithmetic,
    /// which is exactly what makes it able to check it. `None` where no
    /// captured message for this account carried a running balance.
    pub reported: Option<f64>,
    /// Reported minus ours. Positive means the bank says there is more there
    /// than we have accounted for.
    pub drift: Option<f64>,
}

/// What the classify card needs in order to show a person WHAT they are filing.
///
/// ★★★ The card used to ask "which pocket does this belong to?" without showing
/// the message. A person was being asked to file something they could not see.
/// Everything here was already on the ingested message; none of it was reaching
/// the screen.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureContextDto {
    pub id: String,
    /// The text exactly as it arrived.
    pub raw: String,
    /// Which sender it came from: "mpesa" or "kcb".
    pub source: String,
    /// What the transducer made of it, when it could.
    pub amount: Option<f64>,
    pub counterparty: Option<String>,
    pub direction: Option<String>,
    /// The transducer's own words about why this is waiting.
    pub reason: String,
    /// `pending`, `deferred`, or `processed`.
    ///
    /// ★★ A processed message stays in the list on purpose: the back arrow
    /// has to reach a filing he wants to change, and a correction path with
    /// nothing to correct from is not a path.
    pub status: String,
    /// Where a processed one currently sits, so the card can say so.
    pub filed_pocket: Option<String>,
    pub filed_amount: Option<f64>,
}

/// What a netting pass did.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NettingDto {
    /// Charge/refund pairs cancelled. Each removes TWO from the queue.
    pub netted: u32,
    /// Refunds of charges already filed, where the money really went back
    /// through the gate.
    pub given_back: u32,
    /// Compensating calls the gate refused. The pair stays unsettled and comes
    /// back next pass, rather than being marked done on a move that never
    /// landed.
    pub refused: u32,
    /// Matched a filed charge, but nothing recorded HOW it was filed, so there
    /// is no honest way to undo it.
    pub uncompensable: u32,
    /// Refunds with no charge to cancel, left for a person.
    pub unmatched: u32,
    /// Refunds with more than one candidate. Deliberately untouched.
    pub ambiguous: u32,
}

/// A figure a taught shape says belongs somewhere, still awaiting a confirm.
///
/// ★★★ The Fuliza payoff. A borrow carries a sum AND an access fee that
/// belong in different pockets; the sum is pre-filled into the main question,
/// and each remaining figure comes back here so it can be confirmed in turn.
/// Pre-filled is not filed — every one of these is still a tap he makes.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoutedFigureDto {
    /// `in` | `out` | `fee`.
    pub role: String,
    /// Where he said figures like this belong.
    pub pocket: String,
    /// What this message's own figure actually reads.
    pub amount: f64,
}

/// One inference pass, on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum InferenceDto {
    #[serde(rename_all = "camelCase")]
    Ready {
        operator: String,
        params: serde_json::Value,
        why: String,
        description: Option<String>,
        /// ★★ A history pre-fill is never silent: the surface labels it and
        /// offers a change, and it still stops here for a confirmation.
        from_history: bool,
        history_use_count: Option<u32>,
        /// The pocket came from a shape he taught, not from a habit inferred.
        ///
        /// ★★ A separate flag from `from_history` because they are different
        /// claims and the screen says different things. "How you classified
        /// this before" is a guess from a pattern; "the shape you taught" is
        /// him being quoted back to himself.
        #[serde(default)]
        taught: bool,
        /// The other figures this taught shape places, each still to confirm.
        #[serde(default)]
        routed: Vec<RoutedFigureDto>,
    },
    /// ★ `options: null` means the answer is not a tap — render an input.
    #[serde(rename_all = "camelCase")]
    NeedsDisambiguation {
        field: String,
        question: String,
        options: Option<Vec<ChoiceDto>>,
        why: String,
    },
    #[serde(rename_all = "camelCase")]
    CannotInfer { why: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceDto {
    pub value: String,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

/// One peer, as a screen sees it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PeerDto {
    /// ★ The identity. The handle beside it is a label the peer chose.
    pub public_key: String,
    pub handle: String,
    /// `host:port`, or `None` for a peer that reached in and was never given
    /// an address to dial back. **Not** an empty string — unreachable and
    /// "reachable at nowhere" are different facts.
    pub address: Option<String>,
    /// `pending` | `trusted` | `blocked`.
    pub standing: String,
    /// Sustains this node has shared WITH this peer. Never what the peer holds
    /// — this node cannot know that.
    pub shares: Vec<String>,
    /// Seconds since the epoch at the last completed sync, as a string
    /// because a 64-bit integer would cross the boundary as a `BigInt`.
    /// `None` means **never synced**, which is not zero.
    pub last_synced: Option<String>,
    pub last_error: Option<String>,
}

/// This node on the network.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDto {
    /// This node's own public key, readable while locked — it is public.
    pub node_id: Option<String>,
    pub handle: Option<String>,
    /// The port this node accepts peers on. `None` means **not listening**,
    /// which a `0` would have read as a port.
    pub listening: Option<u16>,
    /// The address a peer on the same network can reach this node at.
    ///
    /// ★★★ **Not `127.0.0.1`.** A loopback address is only reachable over a
    /// cable tunnel, so handing it to somebody as "add me here" is telling them
    /// something that stops being true the moment they unplug. `None` means
    /// this machine has no route out, which is a real answer rather than an
    /// error.
    pub reachable_at: Option<String>,
    /// The port this node has settled on, whether or not it is listening now.
    /// ★ Stable across restarts by construction: it is stored, not negotiated.
    pub settled_port: u16,
    /// ★★ A locked node cannot prove its own key, so it cannot peer at all.
    /// The screen says which, rather than showing an idle network.
    pub unlocked: bool,
    pub peers: Vec<PeerDto>,
    /// The wire protocol this node speaks. ★★★ Reported rather than a padlock:
    /// a version is checkable, and a padlock that is always green regardless
    /// is worse than none.
    pub protocol: u32,
    /// What a session actually provides, in the words of what was built.
    pub session: String,
    /// Sustains this node could offer — `(id, label)`. An authored definition
    /// is absent, because v1 shares built-in templates only.
    pub shareable: Vec<(String, String)>,
}

/// One value a concurrent write overwrote.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SupersededDto {
    pub path: String,
    pub winner: String,
    pub loser: String,
}

/// What one sync did, and what the merge could not decide.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncDto {
    pub peer: String,
    pub handle: String,
    pub sustain_id: String,
    pub received: u32,
    pub sent: u32,
    pub entries: u32,
    /// Pairs neither of which happened before the other.
    pub concurrent: u32,
    /// ★★★ Values a concurrent write overwrote. **Every entry survives; these
    /// values did not.** Named rather than dropped.
    pub superseded: Vec<SupersededDto>,
    /// Stamps that arrived carrying two payloads — a node forking its history.
    pub forks: u32,
    /// ★★★ Whether the merged state still satisfies the Sustain's own rules.
    /// `None` = unmeasured (no armed enforcement), which is not *fine*.
    pub admissible: Option<bool>,
    /// `(id, why)` for each rule the merged state breaks.
    pub violated: Vec<(String, String)>,
    /// One line, in the engine's own words.
    pub summary: String,
}

// ---------------------------------------------------------------------------
// The arena
// ---------------------------------------------------------------------------

/// One published package, as a screen sees it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackageDto {
    pub id: String,
    pub name: String,
    /// `definition` | `widget` | `operator` | `strategy`.
    pub kind: String,
    pub version: String,
    pub description: String,
    pub tags: Vec<String>,
    /// The author's key, and the label they chose beside it. ★ The key is the
    /// identity; the handle is a label and the surface says so.
    pub author: String,
    pub author_handle: String,
    pub content_hash: String,
    /// ★★★ The three questions, kept apart all the way to the screen.
    pub integrity: String,
    pub authenticity: String,
    pub origin: String,
    /// One line, in the engine's own words.
    pub provenance: String,
    /// Royalty in **juul** per mille — the internal unit, never money.
    pub per_mille: u32,
    /// Whether this node has installed it, and into what.
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_into: Option<String>,
    /// ★★★ The trust band: `unrated` | `repudiated` | `thin` |
    /// `corroborated` | `first-hand`. **`unrated` is not a low score** — it
    /// means nothing is known, and a surface must not render it on the same
    /// scale as the others.
    pub trust: String,
    /// Whether `trust` may be shown as a reading at all. `false` for
    /// `unrated`.
    pub trust_is_a_reading: bool,
    /// The facts the band was composed from, each in its own words. ★ A band
    /// without its reasons is the reference's float with a nicer name.
    pub trust_signals: Vec<String>,
    /// ★★ What the gate says about installing it **right now** — recomputed,
    /// not remembered. A package that was fine yesterday can be refused today
    /// because a live instance moved.
    pub verdict: String,
    pub installable: bool,
}

/// The registry, and what this node could publish into it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDto {
    pub packages: Vec<PackageDto>,
    /// Definitions authored here that are not published yet.
    pub publishable: Vec<(String, String)>,
    /// Sustains a widget could be installed into.
    pub targets: Vec<(String, String)>,
    /// ★ A locked node cannot sign, so it cannot publish. Said rather than
    /// shown as an idle button.
    pub unlocked: bool,
}

/// What one install attempt did.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InstallDto {
    /// `admitted` | `refused` | `not_here`.
    pub outcome: String,
    /// The gate that refused, by its own name. Empty when admitted.
    pub rule: String,
    /// The gate's own words.
    pub errors: Vec<String>,
    pub provenance: String,
    /// The artifact's id once applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<String>,
    pub summary: String,
}

/// What a royalty moved. ★★★ In **juul**, always.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoyaltyDto {
    /// `settled` | `no_royalty` | `insufficient`.
    pub outcome: String,
    pub transferred: u32,
    /// `(role, recipient, amount)` — every share, including the ones that
    /// stayed put.
    pub shares: Vec<(String, String, u32)>,
    /// ★★★ Total juul across every balance, before and after. Equal, always:
    /// a royalty transfers and never mints.
    pub circulation_before: String,
    pub circulation_after: String,
}

/// One co-owner of a shared Sustain, and whether this node can reach it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CoOwnerDto {
    pub key: String,
    pub handle: String,
    /// ★★ Reachable means *there is an address and the peer is trusted* — not
    /// that a round would succeed. A surface must not promise liveness it has
    /// not tested.
    pub reachable: bool,
    pub is_self: bool,
}

/// A shared Sustain's body, as the Network screen sees it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BodyDto {
    pub sustain_id: String,
    pub label: String,
    pub owners: Vec<CoOwnerDto>,
    /// How many must agree. Majority of the body.
    pub quorum: u32,
    /// How many are reachable right now, this node included.
    pub reachable: u32,
    /// ★★★ Whether a write could even be attempted. `false` means writes are
    /// **refused**, not queued and not applied locally — the honest state
    /// rather than a silent degradation.
    pub can_write: bool,
    /// ★ The Byzantine bound for this body size, reported because a two-node
    /// body tolerates **zero** traitors and a person should know that.
    pub tolerates_traitors: u32,
    /// The highest log position this node has agreed anything for. `None` when
    /// nothing has been agreed — which is not slot zero.
    pub last_agreed: Option<u32>,
}

/// One package a peer says it holds. ★ A listing, never the artifact.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OfferDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub author_handle: String,
    pub content_hash: String,
    /// ★★ Shown **before** fetching, so authorship is a thing you decide on
    /// rather than discover afterwards.
    pub signed: bool,
    /// Whether this node already holds these exact bytes.
    pub already_here: bool,
}

/// What one peer is offering, or why nothing could be listed.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PeerShelfDto {
    pub peer: String,
    pub handle: String,
    pub address: String,
    pub packages: Vec<OfferDto>,
    /// ★★★ The honest failure. A peer that could not be reached says so;
    /// it does not appear as a peer with nothing to offer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreachable: Option<String>,
}

/// One acquisition. ★★★ `paid` is **juul**, this host's internal unit —
/// never money, never off this host, never a rail.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OrderDto {
    pub reference: String,
    pub package_id: String,
    pub package_name: String,
    pub by: String,
    pub paid: u32,
    pub per_mille: u32,
    /// `(role, recipient, amount)` — every share, including ones that stayed
    /// where they already were.
    pub shares: Vec<(String, String, u32)>,
    pub placed_at: String,
}
