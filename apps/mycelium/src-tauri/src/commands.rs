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
    AccessDto, Branch, BranchStep, Committed, ConstraintReading, CouncilOutcomeDto, EconomyDto,
    GateResult, Holarchy, LedgerEntryDto, LogEntryDto, MeasuredPawa, OperatorAccessDto,
    OperatorDto, ParamDto, ParameterDto, Refused, RolledUp, RollupDto, SustainDto, SustainSummary,
    TransferLegDto, TransferResult, Verdict, WorldDto,
};
use crate::definitions::{AuthoredDefinition, DefinitionVerdict};
use crate::templates::TemplateId;
use sustena_core::holon::Transfer as HolonTransfer;
use crate::world::{memberships, World, PRINCIPAL};

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

                // ★★★ ρ, recomputed and pushed — for the SUSTAIN WHOSE TOTAL
                //   MOVED, which for a member's commit is its household. A
                //   third question needs a third message: `Committed` is about
                //   the Sustain that changed, and a household total is a
                //   different Sustain's derived reading.
                //
                // ★ Nothing is pushed when no aggregate could have moved, so
                //   the channel stays silent for the Sustains that declare
                //   none — which is most of them.
                if let Some(subject) = world.rollup_subject(&sustain_id) {
                    if let Some(r) = world.rollup(&subject) {
                        let msg = RolledUp { rollup: r };
                        if let Err(e) = msg.emit(&app) {
                            trace!("  !! rollup push failed: {e}");
                        } else {
                            trace!("  ~> pushed RolledUp for {subject}");
                        }
                    }
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

// ── Define ───────────────────────────────────────────────────────────────────

/// Every definition a person has authored on this host.
#[tauri::command]
#[specta::specta]
pub fn get_definitions(world: State<'_, World>) -> Vec<AuthoredDefinition> {
    world.definitions()
}

/// ★★★ Author a definition — **the engine decides whether it lands**.
///
/// `editing::typecheck` parses every invariant and binds it against the schema;
/// `editing::safe` checks it against every live instance. A definition that
/// fails either is never written, and the verdict carries the engine's own
/// words rather than a summary of them.
#[tauri::command]
#[specta::specta]
pub fn author_definition(
    world: State<'_, World>,
    definition: AuthoredDefinition,
) -> Result<DefinitionVerdict, String> {
    trace!("author_definition {} ({} invariants)", definition.id, definition.invariants.len());
    let v = world.author_definition(&definition).map_err(|e| e.to_string())?;
    trace!("  -> {v:?}");
    Ok(v)
}

/// Instantiate a Sustain from an authored definition — the same path a
/// built-in uses.
#[tauri::command]
#[specta::specta]
pub fn create_from_definition(
    world: State<'_, World>,
    id: String,
    label: String,
    definition_id: String,
    parent: Option<String>,
) -> Result<bool, String> {
    trace!("create_from_definition {id} <- {definition_id}");
    world
        .instantiate_from(&id, &label, TemplateId::Habitat, Some(&definition_id), parent.as_deref())
        .map_err(|e| e.to_string())
}

// ── Profile: the real capability model ───────────────────────────────────────

/// What the local principal may do on one Sustain, **as the engine judges it**.
///
/// ★★ `permitted` walks the membership path with the weakest-link rule and
/// compares against each operator's declared `min_privilege`. This is the real
/// model, not a display of intentions.
///
/// ★★★ **And it is not what the gate currently checks.** Every call in this
/// host runs `Authorization::Unchecked`, so this is the authority a person
/// HOLDS, shown — not one being enforced. The screen says so, because a
/// permission matrix that implied enforcement would be security theatre.
#[tauri::command]
#[specta::specta]
pub fn get_access(world: State<'_, World>, sustain_id: String) -> AccessDto {
    use sustena_core::principal::{effective_privilege, permitted};

    let m = memberships();
    let path = vec![sustain_id.clone()];
    let tier = effective_privilege(&m, PRINCIPAL, &path).ok();

    let allowed: Vec<String> =
        world.with(|i| i.get(&sustain_id).map(|s| s.definition.operators.clone()).unwrap_or_default());

    let operators = allowed
        .iter()
        .filter_map(|name| {
            let meta = world.operators.get(name)?;
            let verdict = permitted(&m, PRINCIPAL, &path, meta.min_privilege);
            Some(OperatorAccessDto {
                operator: name.clone(),
                required_tier: meta.min_privilege,
                permitted: verdict.is_ok(),
                denial: verdict.err().map(|d| format!("{d}")),
            })
        })
        .collect();

    AccessDto {
        principal: PRINCIPAL.to_string(),
        sustain_id,
        tier,
        memberships: m.len() as u32,
        operators,
        enforced: false,
        note: "This is the authority the principal HOLDS. It is not yet what the gate \
               checks: every call in this host runs `Authorization::Unchecked`, so the \
               capability model is displayed, not enforced."
            .into(),
    }
}

// ── Council: real resolution ─────────────────────────────────────────────────

/// ★★★ Resolve a proposal with the engine's own `council::resolve`.
///
/// Real: the rule that a person's vote overrides the council, that an abstaining
/// person leaves it **in voting** rather than deciding, and that collected votes
/// with nobody in favour fail. None of it is re-implemented here.
#[tauri::command]
#[specta::specta]
pub fn resolve_proposal(
    votes: Vec<(String, String, f64)>,
    user_vote: Option<String>,
    votes_collected: bool,
) -> CouncilOutcomeDto {
    use sustena_core::council::{aggregate_delegated_votes, resolve, DelegatedVote, ResolutionInput, VoteChoice};

    let parse = |v: &str| match v {
        "yes" => VoteChoice::Yes,
        "no" => VoteChoice::No,
        _ => VoteChoice::Abstain,
    };

    let delegated: Vec<DelegatedVote> = votes
        .iter()
        .map(|(_, choice, confidence)| DelegatedVote {
            position: parse(choice),
            confidence: *confidence,
            reasoning: String::new(),
        })
        .collect();

    let aggregated = aggregate_delegated_votes(&delegated);
    let cast: Vec<VoteChoice> = delegated.iter().map(|d| d.position).collect();
    let status = resolve(&ResolutionInput {
        votes: &cast,
        votes_collected,
        user_vote: user_vote.as_deref().map(parse),
        // The core compares nothing it cannot replay, so a deadline is the
        // HOST'S question. This app has no proposal deadlines, so it is never
        // expired -- said rather than defaulted into silently.
        expired: false,
    });

    CouncilOutcomeDto {
        status: format!("{status:?}"),
        aggregated: format!("{:?}", aggregated.vote),
        reasoning: aggregated.reasoning,
        utility: aggregated.utility,
        counted: votes.len() as u32,
    }
}

/// **ρ** for one Sustain — its declared totals, folded fresh from its own state
/// and every linked child's.
///
/// ★ `None` only when there is no such Sustain. A Sustain that declares no
/// aggregates answers with an empty reading, because *asked for no totals* and
/// *a total that could not be computed* are different facts.
#[tauri::command]
#[specta::specta]
pub fn get_rollup(world: State<'_, World>, sustain_id: String) -> Option<RollupDto> {
    world.rollup(&sustain_id)
}

/// ★★★ **The atomic, conserved cross-Sustain transfer.**
///
/// Both legs commit or neither does — the core makes half a transfer
/// unrepresentable, and the store's write-ahead journal makes it unwritable.
///
/// A refusal comes back as a **value**, not an error: the gate declining is an
/// expected outcome, and turning it into a thrown error would put it in the
/// same bucket as a disk failure.
#[tauri::command]
#[specta::specta]
pub fn transfer(
    app: AppHandle,
    world: State<'_, World>,
    from_sustain_id: String,
    to_sustain_id: String,
    path: String,
    amount: f64,
) -> Result<TransferResult, String> {
    trace!("transfer  {from_sustain_id} -> {to_sustain_id}  {amount} along {path}");

    // Read both balances BEFORE, so the result can show the move rather than
    // only its endpoint.
    let before = |id: &str| -> f64 {
        world
            .with(|i| i.get(id).map(|s| s.state.clone()))
            .and_then(|st| dot(&st, &path))
            .unwrap_or(0.0)
    };
    let from_before = before(&from_sustain_id);
    let to_before = before(&to_sustain_id);

    let settled = world
        .transfer(&from_sustain_id, &to_sustain_id, &path, amount)
        .map_err(|e| e.to_string())?;

    let out = match &settled {
        HolonTransfer::Refused { rule, reason } => {
            trace!("  -> REFUSED  {rule}: {reason}");
            TransferResult::Refused { rule: rule.clone(), reason: reason.clone() }
        }
        HolonTransfer::Committed(s) => {
            let (debit, credit) = s.legs();
            trace!(
                "  -> COMMITTED  {} -> {}   total {} == {}",
                debit.balance_after(),
                credit.balance_after(),
                s.total_before(),
                s.total_after()
            );

            // ★★★ BOTH Sustains changed, so BOTH get a push. A transfer that
            //   announced only one side would leave every screen watching the
            //   other confidently stale — the exact asymmetry a conserved move
            //   must not have.
            for leg in [debit, credit] {
                let id = leg.sustain_id();
                let msg = Committed {
                    sustain_id: id.to_string(),
                    operator: "holon.transfer".to_string(),
                    seq: world.with(|i| i.get(id).map(|s| s.next_seq).unwrap_or(1)) as u32 - 1,
                    events: vec![crate::dto::EventDto::from(leg.event())],
                    mutations: leg.mutations().len() as u32,
                    liquid: leg
                        .state_after()
                        .pointer("/finances/liquid/balance")
                        .and_then(serde_json::Value::as_f64),
                    constraints: readings(&world, id),
                    state: leg.state_after().clone(),
                };
                if let Err(e) = msg.emit(&app) {
                    trace!("  !! push failed: {e}");
                }
                // And ρ for whoever's total moved.
                if let Some(subject) = world.rollup_subject(id) {
                    if let Some(r) = world.rollup(&subject) {
                        let rolled = RolledUp { rollup: r };
                        if let Err(e) = rolled.emit(&app) {
                            trace!("  !! rollup push failed: {e}");
                        }
                    }
                }
            }

            TransferResult::Committed {
                path: s.path().to_string(),
                amount: s.amount(),
                from: TransferLegDto {
                    sustain_id: debit.sustain_id().to_string(),
                    balance_before: from_before,
                    balance_after: debit.balance_after(),
                },
                to: TransferLegDto {
                    sustain_id: credit.sustain_id().to_string(),
                    balance_before: to_before,
                    balance_after: credit.balance_after(),
                },
                total_before: s.total_before(),
                total_after: s.total_after(),
            }
        }
    };
    Ok(out)
}

/// Resolve a dot-path to a number, for the before-reading above.
fn dot(state: &Value, path: &str) -> Option<f64> {
    let mut node = state;
    for seg in path.split('.') {
        node = node.get(seg)?;
    }
    node.as_f64()
}
