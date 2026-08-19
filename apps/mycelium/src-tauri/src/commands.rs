//! The command surface — every one of them a real engine call, on a real
//! Sustain, against a real log.
//!
//! ★ A command returns its own honest shape rather than an error type wherever
//! the "failure" is a normal outcome: a gate refusal is an answer, and an
//! unknown Sustain id is a `None`. `Result` is reserved for what genuinely went
//! wrong — the disk.

use serde_json::{Map, Value};
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::dto::{
    Branch, BranchStep, Committed, ConstraintReading, EconomyDto, GateResult, Holarchy,
    LedgerEntryDto, LogEntryDto, MeasuredPawa, OperatorDto, ParamDto, ParameterDto, Refused,
    SustainDto, SustainSummary, Verdict, WorldDto,
};
use crate::templates::TemplateId;
use crate::world::{World, PRINCIPAL};

/// `V`, evaluated against current state by the engine.
fn readings(world: &World, sustain_id: &str) -> Vec<ConstraintReading> {
    world
        .constraints(sustain_id)
        .into_iter()
        .map(|(id, expression, holds, reason)| ConstraintReading { id, expression, holds, reason })
        .collect()
}

/// ★ Every command logs what it was asked and what the engine answered.
///
/// Not decoration: it is how a real IPC call from the window is *observable*
/// from outside the webview. A dev run that prints these is a dev run where the
/// UI genuinely reached the engine.
macro_rules! trace {
    ($($t:tt)*) => { println!("[ipc] {}", format!($($t)*)) };
}

/// The household: every Sustain, the selection, and where the log lives.
#[tauri::command]
#[specta::specta]
pub fn get_world(world: State<'_, World>) -> WorldDto {
    let holarchy = match world.check_holarchy() {
        Ok(linked) => Holarchy::Holds { linked: linked as u32 },
        Err(reason) => Holarchy::Broken { reason },
    };
    // ★ Every summary carries `V` as the engine reads it, so a constellation can
    //   show a broken rule anywhere without hydrating a single state document.
    let ids: Vec<String> = world.with(|i| i.order().to_vec());
    let sustains: Vec<SustainSummary> = ids
        .iter()
        .filter_map(|id| {
            let readings = readings(&world, id);
            world.with(|i| i.get(id).map(|s| SustainSummary::of(s, readings.clone())))
        })
        .collect();
    let selected = world.with(|i| i.selected().map(|s| s.record.id.clone()));
    trace!("get_world -> {} sustain(s), holarchy={:?}", sustains.len(), holarchy);
    WorldDto {
        sustains,
        selected,
        store_path: world.store().root().display().to_string(),
        principal: PRINCIPAL.to_string(),
        holarchy,
        // ★★★ Stated, not implied. See `WorldDto::rollup_available`.
        rollup_available: false,
    }
}

/// `Σ` and current state for one Sustain — the selected one when `id` is absent.
#[tauri::command]
#[specta::specta]
pub fn get_sustain(world: State<'_, World>, id: Option<String>) -> Option<SustainDto> {
    world.with(|i| {
        let s = match &id {
            Some(id) => i.get(id)?,
            None => i.selected()?,
        };
        Some(SustainDto::of(
            &s.record.id,
            &s.record.label,
            &s.definition,
            s.enforcement.enabled,
            &s.state,
        ))
    })
}

/// Point the cockpit at a different Sustain. `false` when there is no such id.
#[tauri::command]
#[specta::specta]
pub fn select_sustain(world: State<'_, World>, id: String) -> Result<bool, String> {
    trace!("select_sustain {id}");
    world.select(&id).map_err(|e| e.to_string())
}

/// Instantiate a new Sustain. `false` when the id is already taken.
#[tauri::command]
#[specta::specta]
pub fn create_sustain(
    world: State<'_, World>,
    id: String,
    label: String,
    template: TemplateId,
    parent: Option<String>,
) -> Result<bool, String> {
    trace!("create_sustain {id} ({}) parent={parent:?}", template.label());
    world.instantiate(&id, &label, template, parent.as_deref()).map_err(|e| e.to_string())
}

/// `V` for one Sustain, evaluated now.
#[tauri::command]
#[specta::specta]
pub fn get_constraints(world: State<'_, World>, sustain_id: String) -> Vec<ConstraintReading> {
    readings(&world, &sustain_id)
}

/// ★★ The persisted log — the event log a person can read.
///
/// Read from **disk**, not from memory: what this shows is what actually
/// survives, which is the only version worth showing.
#[tauri::command]
#[specta::specta]
pub fn get_log(world: State<'_, World>, sustain_id: String) -> Result<Vec<LogEntryDto>, String> {
    let log = world.log(&sustain_id).map_err(|e| e.to_string())?;
    Ok(log
        .into_iter()
        .map(|e| LogEntryDto {
            seq: e.seq as u32,
            operator: e.operator,
            mutations: e.mutations.len() as u32,
            events: e.events,
        })
        .collect())
}

/// ★★★ Ask the engine to do something on one Sustain, and report the verdict.
///
/// Returns `GateResult` — **not** an error type. A refusal is a correct answer,
/// and typing it as an error would make the UI render the system working as the
/// system failing.
///
/// `None` means there is no such Sustain, which is a different fact from a
/// refusal and is kept a different shape. The outer `Result` is for the disk.
#[tauri::command]
#[specta::specta]
pub fn run_operator(
    app: AppHandle,
    world: State<'_, World>,
    sustain_id: String,
    operator: String,
    params: Value,
) -> Result<Option<GateResult>, String> {
    let params: Map<String, Value> = match params {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    trace!(
        "run_operator  {sustain_id}  {operator}  {}",
        serde_json::to_string(&params).unwrap_or_default()
    );

    let called = world.call(&sustain_id, &operator, &params).map_err(|e| e.to_string())?;
    let out = called.as_ref().map(|(x, _)| GateResult::of(&operator, x));

    match (&called, &out) {
        (Some((x, seq)), Some(r)) => {
            trace!(
                "  -> {:?}  mutations={} events={} reason={}",
                r.verdict,
                r.mutations,
                r.events.len(),
                r.reason.clone().unwrap_or_else(|| "-".into())
            );
            // ★★★ THE PUSH. Only on a commit — a refusal changed nothing, and a
            //   message announcing no change would be a change that did not
            //   happen. One message per real change, no tick, no sampler.
            if x.committed() {
                let msg =
                    Committed::of(&sustain_id, &operator, *seq, x, readings(&world, &sustain_id));
                if let Err(e) = msg.emit(&app) {
                    // ★ Reported, never swallowed: a push that silently failed
                    //   would leave the UI confidently stale.
                    trace!("  !! push failed: {e}");
                } else {
                    trace!("  ~> pushed Committed seq={} to the UI", seq);
                }
            } else {
                // ★★ A refusal pushes an ACTIVITY message carrying NO STATE.
                //    The state rule is untouched — nothing changed, so nothing
                //    about state travels. What travels is that a request was
                //    made and declined, which is worth watching.
                let msg = Refused::of(&sustain_id, &operator, r);
                if let Err(e) = msg.emit(&app) {
                    trace!("  !! push failed: {e}");
                } else {
                    trace!("  ~> pushed Refused to the UI (no state)");
                }
            }
        }
        _ => trace!("  -> no such sustain"),
    }
    Ok(out)
}

// ── Console: the operator catalogue ──────────────────────────────────────────

/// Every operator the given Sustain may actually run, with its **measured** cost.
///
/// ★★ The list is the Sustain's own `T`, not the whole registry — an operator a
/// definition does not permit would be refused with `operator_allowed`, and
/// offering it would be inviting a refusal the person could not have predicted.
#[tauri::command]
#[specta::specta]
pub fn get_operators(world: State<'_, World>, sustain_id: String) -> Vec<OperatorDto> {
    let allowed: Vec<String> =
        world.with(|i| i.get(&sustain_id).map(|s| s.definition.operators.clone()).unwrap_or_default());

    allowed
        .iter()
        .filter_map(|name| {
            let meta = world.operators.get(name)?;
            // ★★★ The measurement, or nothing. `stats_for` returns `None` for an
            //     operator the meter has never seen — and `None` must render as
            //     "not measured", never as a zero.
            let measured = world.with_meter(|m| m.stats_for(name)).map(|st| MeasuredPawa {
                runs: st.runs as u32,
                mean_pawa: st.mean_pawa(),
                total_pawa: st.total_pawa,
                total_compute: st.total_compute as u32,
                total_storage: st.total_storage as u32,
            });
            Some(OperatorDto {
                name: meta.name.to_string(),
                description: meta.description.to_string(),
                params: meta
                    .params
                    .iter()
                    .map(|p| ParamDto {
                        name: p.name.to_string(),
                        kind: format!("{:?}", p.kind).to_lowercase(),
                        required: p.required,
                        names_within: p.names_within.map(str::to_string),
                    })
                    .collect(),
                declared_pawa: meta.pawa_cost,
                measured,
                side_effects: meta.side_effects.iter().map(|s| s.to_string()).collect(),
            })
        })
        .collect()
}

// ── Simulate: a fork through the real gate ───────────────────────────────────

/// ★★★ Run a hypothetical branch. **Nothing is written.**
///
/// The fork is `state.clone()` plus the same `execute_admitted` a real call
/// uses, so a step refused here is refused for the same reason it would be for
/// real. It touches no log, no ledger and no meter.
#[tauri::command]
#[specta::specta]
pub fn simulate(
    world: State<'_, World>,
    sustain_id: String,
    steps: Vec<(String, Value)>,
) -> Option<Branch> {
    let from = world.with(|i| i.get(&sustain_id).map(|s| s.state.clone()))?;
    let prepared: Vec<(String, Map<String, Value>)> = steps
        .into_iter()
        .map(|(op, p)| {
            let m = match p {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            (op, m)
        })
        .collect();

    trace!("simulate  {sustain_id}  {} step(s)  (nothing will be written)", prepared.len());
    let run = world.fork(&sustain_id, &prepared)?;

    Some(Branch {
        sustain_id,
        hypothetical: true,
        from,
        steps: run
            .into_iter()
            .map(|(operator, x)| BranchStep {
                verdict: Verdict::from(x.result.status),
                operator,
                reason: x.result.reason.clone(),
                constraint_violated: x.result.constraint_violated.clone(),
                mutations: x.mutations.len() as u32,
                events: x.events.iter().map(crate::dto::EventDto::from).collect(),
                state: x.state.clone(),
            })
            .collect(),
    })
}

// ── Economy ──────────────────────────────────────────────────────────────────

/// The whole economy, read from the engine's own ledger and parameters.
#[tauri::command]
#[specta::specta]
pub fn get_economy(world: State<'_, World>) -> EconomyDto {
    use sustena_core::governance::declared_parameters;
    use sustena_core::juul::{Entry, MintAuthority};

    world.with_economy(|e| {
        let audit = e.genesis.audit(&e.ledger);
        let p = e.parameters();

        let entries = e
            .ledger
            .entries()
            .iter()
            .rev()
            .take(80)
            .map(|entry| match entry {
                Entry::Mint { principal, amount, authority } => LedgerEntryDto {
                    kind: "mint".into(),
                    principal: principal.clone(),
                    counterparty: None,
                    amount: *amount,
                    authority: match authority {
                        MintAuthority::Genesis(g) => format!("genesis {}", g.as_str()),
                        MintAuthority::Issued(i) => format!("issued {}", i.as_str()),
                    },
                    circulation_delta: entry.circulation_delta(),
                },
                Entry::Debit { principal, amount, operator, .. } => LedgerEntryDto {
                    kind: "debit".into(),
                    principal: principal.clone(),
                    counterparty: None,
                    amount: *amount,
                    authority: operator.clone(),
                    circulation_delta: entry.circulation_delta(),
                },
                Entry::Transfer { from, to, amount, reason } => LedgerEntryDto {
                    kind: "transfer".into(),
                    principal: from.clone(),
                    counterparty: Some(to.clone()),
                    amount: *amount,
                    authority: reason.clone(),
                    circulation_delta: entry.circulation_delta(),
                },
            })
            .collect();

        let parameters = declared_parameters()
            .into_iter()
            .map(|spec| ParameterDto {
                value: match spec.name {
                    "kappa_compute" => p.kappa_compute(),
                    "kappa_storage" => p.kappa_storage(),
                    "issuance_rate" => p.issuance_rate(),
                    "attention_kappa" => p.attention_kappa(),
                    _ => spec.genesis,
                },
                name: spec.name.to_string(),
                genesis: spec.genesis,
                min: spec.min,
                max: spec.max,
            })
            .collect();

        let metered = world.with_meter(|m| {
            m.by_operator()
                .into_iter()
                .map(|(name, st)| {
                    (
                        name.to_string(),
                        MeasuredPawa {
                            runs: st.runs as u32,
                            mean_pawa: st.mean_pawa(),
                            total_pawa: st.total_pawa,
                            total_compute: st.total_compute as u32,
                            total_storage: st.total_storage as u32,
                        },
                    )
                })
                .collect()
        });

        EconomyDto {
            principal: PRINCIPAL.to_string(),
            balance: e.ledger.balance_of(PRINCIPAL),
            circulation: e.ledger.total_in_circulation(),
            minted_total: e.ledger.minted_total(),
            issued_total: e.ledger.issued_total(),
            genesis_id: e.genesis.id().as_str().to_string(),
            genesis_total: e.genesis.total(),
            audit_clean: audit.clean(),
            audit_describes: audit.describe(),
            entries,
            parameters,
            metered,
            boundary_notice:
                "Internal points only. Juul is an accounting unit on this machine \
                 -- never real money, never transferable, never a payment rail. \
                 Nothing in this app can move real value (ADR-0001 D5)."
                    .into(),
        }
    })
}

/// ★★★ Change a governed parameter — through the gate, like anything else.
///
/// Returns the gate's verdict: an out-of-range value is refused with
/// `enforcement_gate`, the same reason a household breach gives.
#[tauri::command]
#[specta::specta]
pub fn set_parameter(world: State<'_, World>, name: String, value: f64) -> GateResult {
    trace!("set_parameter {name} = {value}");
    let x = world.set_parameter(&name, value);
    let out = GateResult::of("governance.set_parameter", &x);
    trace!("  -> {:?} {}", out.verdict, out.reason.clone().unwrap_or_default());
    out
}
