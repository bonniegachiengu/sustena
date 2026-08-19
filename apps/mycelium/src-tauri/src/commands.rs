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
    AttentionDto, CaptureResult, CardDto, ChoiceDto, FeedDto, IdentityDto,
    InferenceDto, IngestDto, MessageDto, QuietDto, RuleDto, SourceDto,
    NetworkDto, PeerDto, SupersededDto, SyncDto,
    TransferLegDto, TransferResult, Verdict, WorldDto,
};
use crate::peers::Standing;
use crate::definitions::{AuthoredDefinition, DefinitionVerdict};
use crate::templates::TemplateId;
use sustena_core::holon::Transfer as HolonTransfer;
use crate::ingest::Capture;
use std::collections::BTreeMap;
use crate::world::{World, DEFAULT_HANDLE};

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
        // ★ Empty while locked. A cockpit that showed a name before the key was
        //   recovered would be showing a claim, not an identity.
        principal: world.principal().unwrap_or_default(),
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
    let who = world.principal().unwrap_or_else(|| DEFAULT_HANDLE.to_string());
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
            principal: who.clone(),
            balance: e.ledger.balance_of(&who),
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
/// ★★★ **And it IS what the gate checks.** Every call runs
/// `Authorization::Principal` with the unlocked identity, so this is authority
/// in force. It asks the same `permitted` the gate asks, from the same
/// memberships and the same path — a screen computing permission its own way
/// would eventually disagree with the thing that decides.
#[tauri::command]
#[specta::specta]
pub fn get_access(world: State<'_, World>, sustain_id: String) -> AccessDto {
    use sustena_core::principal::{effective_privilege, permitted};

    // ★★★ The SAME principal, memberships and path the gate uses. A screen
    //   computing permission its own way would eventually disagree with the
    //   thing that actually decides.
    let who = world.principal().unwrap_or_default();
    let m = world.memberships_for(&who);
    let path = world.authority_path(&sustain_id);
    let tier = effective_privilege(&m, &who, &path).ok();

    let allowed: Vec<String> =
        world.with(|i| i.get(&sustain_id).map(|s| s.definition.operators.clone()).unwrap_or_default());

    let operators = allowed
        .iter()
        .filter_map(|name| {
            let meta = world.operators.get(name)?;
            let verdict = permitted(&m, &who, &path, meta.min_privilege);
            Some(OperatorAccessDto {
                operator: name.clone(),
                required_tier: meta.min_privilege,
                permitted: verdict.is_ok(),
                denial: verdict.err().map(|d| format!("{d}")),
            })
        })
        .collect();

    AccessDto {
        principal: who,
        sustain_id,
        tier,
        memberships: m.len() as u32,
        operators,
        // ★ True now, and it is the host reporting a fact about itself:
        //   `World::call` passes `Authorization::Principal`, never `Unchecked`.
        enforced: true,
        note: "Enforced. Every call runs Authorization::Principal with the unlocked \nidentity, and permitted(principal, operator, sustain) is a conjunct of admit() -- \ndecided BEFORE the guard and refused with its own name. An authorization refusal \nreads insufficient_privilege or not_a_member, never enforcement_gate."
            .to_string(),
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

// ── identity ──────────────────────────────────────────────────

/// Who this machine is, and whether the key is unlocked.
#[tauri::command]
#[specta::specta]
pub fn get_identity(world: State<'_, World>) -> IdentityDto {
    let store = world.identity_store();
    let file = store.read().ok();
    IdentityDto {
        enrolled: store.exists(),
        unlocked: world.is_unlocked(),
        handle: world.principal().or_else(|| file.as_ref().map(|f| f.handle.clone())),
        public_key: world.public_key(),
        kdf: file.as_ref().map(|f| f.kdf.clone()),
        iterations: file.as_ref().map(|f| f.iterations),
    }
}

/// **Unlock** the local identity with a passphrase.
///
/// ★★★ The passphrase is not compared against anything. It derives a key, and
/// either that key decrypts the private half or it does not — there is no
/// branch here an attacker could invert, and a wrong passphrase leaves the
/// world locked. The error text is the same for a wrong passphrase and a
/// tampered file, on purpose.
#[tauri::command]
#[specta::specta]
pub fn unlock_identity(world: State<'_, World>, passphrase: String) -> Result<IdentityDto, String> {
    trace!("unlock_identity");
    world.unlock(&passphrase).map_err(|e| e.to_string())?;
    trace!("  -> unlocked as {:?}", world.principal());
    Ok(get_identity(world))
}

/// **Enrol** a new identity on a machine that has none.
#[tauri::command]
#[specta::specta]
pub fn enrol_identity(
    world: State<'_, World>,
    handle: String,
    passphrase: String,
) -> Result<IdentityDto, String> {
    let handle = if handle.trim().is_empty() { DEFAULT_HANDLE.to_string() } else { handle };
    trace!("enrol_identity  {handle}");
    world.enrol(&handle, &passphrase).map_err(|e| e.to_string())?;
    Ok(get_identity(world))
}

/// Drop the private key from memory. ★ Not a UI state — the key genuinely
/// leaves, so a locked cockpit cannot act even if a surface forgot to stop it.
#[tauri::command]
#[specta::specta]
pub fn lock_identity(world: State<'_, World>) -> IdentityDto {
    trace!("lock_identity");
    world.lock();
    get_identity(world)
}

// ── ingest ──────────────────────────────────────────────────

/// The capture queue, the declared sources, and the rules in force.
#[tauri::command]
#[specta::specta]
pub fn get_ingest(world: State<'_, World>, sustain_id: String) -> Result<IngestDto, String> {
    let ing = world.ingest();
    let messages: Vec<MessageDto> = ing
        .current()
        .map_err(|e| e.to_string())?
        .iter()
        .filter(|m| m.sustain_id == sustain_id)
        .map(MessageDto::of)
        .collect();

    let sources: Vec<SourceDto> = ing
        .sources()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| SourceDto {
            id: s.id,
            label: s.label,
            captures: s.captures as u32,
            expected_interval_minutes: s.expected_interval_minutes,
            ever_seen: s.last_seen_seq.is_some(),
        })
        .collect();

    // ★ The shipped rules AND this household's corrections, in one library
    //   view — a person should be able to see what the engine knows, not only
    //   what they taught it.
    let mut rules: Vec<RuleDto> = Vec::new();
    for r in sustena_core::all_seed_rules().iter().chain(
        ing.effective_rules().map_err(|e| e.to_string())?.iter(),
    ) {
        rules.push(RuleDto {
            id: r.id.clone(),
            source: r.source.clone(),
            version: r.version,
            status: match r.status {
                sustena_core::RuleStatus::Mapped => "mapped",
                sustena_core::RuleStatus::ParsedUnmapped => "parsed_unmapped",
                sustena_core::RuleStatus::Informational => "informational",
            }
            .to_string(),
            operator: r.operator.clone(),
            trust: match r.trust {
                sustena_core::ParseRuleTrust::Shipped => "shipped",
                sustena_core::ParseRuleTrust::UserCorrected => "user_corrected",
                sustena_core::ParseRuleTrust::ProposedConfirmed => "proposed_confirmed",
            }
            .to_string(),
            examples: r.examples.len() as u32,
        });
    }

    let needs_attention = messages.iter().filter(|m| m.needs_attention).count() as u32;
    Ok(IngestDto {
        messages,
        sources,
        rules,
        rejected: ing.rejected_count() as u32,
        needs_attention,
    })
}

/// **Capture one message.**
///
/// ★★★ A message carrying a secret is refused **before anything is written**
/// — the result has no message field at all, because there is nothing to show.
/// A mapped message applies through the same gated path the Console uses, as
/// the unlocked principal.
#[tauri::command]
#[specta::specta]
pub fn capture_message(
    world: State<'_, World>,
    sustain_id: String,
    source_id: String,
    raw: String,
) -> Result<CaptureResult, String> {
    trace!("capture  {sustain_id} / {source_id}  ({} bytes)", raw.len());
    // ★ The text is NEVER traced. A log line is a store too.
    let captured = world.capture(&sustain_id, &source_id, &raw).map_err(|e| e.to_string())?;
    Ok(match captured {
        Capture::Rejected { reason } => {
            trace!("  -> REJECTED (nothing stored)");
            CaptureResult::Rejected { reason }
        }
        Capture::Duplicate(m) => {
            trace!("  -> duplicate of {}", m.id);
            CaptureResult::Duplicate { message: MessageDto::of(&m) }
        }
        Capture::Stored(m) => {
            trace!("  -> {} via {} (applied={})", m.status, m.parser_name, m.applied);
            CaptureResult::Stored { message: MessageDto::of(&m) }
        }
    })
}

/// Declare a capture source, optionally with the cadence it should keep.
#[tauri::command]
#[specta::specta]
pub fn declare_source(
    world: State<'_, World>,
    id: String,
    label: String,
    expected_interval_minutes: Option<u32>,
) -> Result<(), String> {
    world
        .ingest()
        .declare_source(&id, &label, expected_interval_minutes)
        .map_err(|e| e.to_string())
}

/// Mark a queued message as handled by a person.
#[tauri::command]
#[specta::specta]
pub fn resolve_message(world: State<'_, World>, id: String) -> Result<bool, String> {
    world.ingest().resolve(&id).map_err(|e| e.to_string())
}

/// **Remember this format** — synthesise a rule from a confirmed correction.
///
/// ★★ Verified before it is ever added: it must be well-typed, must match the
/// message that taught it, and must not capture a message an existing rule
/// already handles. ★★★ A learned SPEND rule carries no operator, so a
/// correction can never teach the system to spend on someone's behalf.
#[tauri::command]
#[specta::specta]
pub fn learn_rule(
    world: State<'_, World>,
    message_id: String,
    operator: String,
    params: Value,
) -> Result<String, String> {
    let Some(m) = world
        .ingest()
        .current()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|m| m.id == message_id)
    else {
        return Err("no such captured message".into());
    };

    let params: BTreeMap<String, Value> = match params {
        Value::Object(o) => o.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    let id = format!("learned_{}_{}", m.source_id, &m.dedup_key[..12]);
    let Some(candidate) = sustena_core::synthesize_from_correction(
        &m.source_id,
        &m.raw_payload,
        &operator,
        &params,
        &id,
    ) else {
        return Err(
            "nothing to learn from: the confirmed amount does not appear in the message text"
                .into(),
        );
    };

    let existing = world.rules_for(&m.source_id).map_err(|e| e.to_string())?;
    sustena_core::verify_candidate(&candidate, &m.raw_payload, &existing, &world.operators)
        .map_err(|e| e.to_string())?;
    world.ingest().add_rule(&candidate).map_err(|e| e.to_string())?;
    Ok(candidate.id)
}

// ── Orchie ──────────────────────────────────────────────────

/// **The curated feed** — `compose(r)` over one household.
#[tauri::command]
#[specta::specta]
pub fn get_feed(
    world: State<'_, World>,
    sustain_id: String,
    query: Option<String>,
) -> Result<FeedDto, String> {
    let Some((label, state)) = world
        .with(|i| i.get(&sustain_id).map(|s| (s.record.label.clone(), s.state.clone())))
    else {
        return Err(format!("no Sustain called '{sustain_id}'"));
    };

    let queued: Vec<_> = world
        .ingest()
        .current()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|m| m.sustain_id == sustain_id && m.needs_attention())
        .collect();

    // ★ The recent log, as β needs it — what actually happened, not a guess.
    let recent: Vec<sustena_core::event::Event> = world
        .log(&sustain_id)
        .map_err(|e| e.to_string())?
        .iter()
        .rev()
        .take(25)
        .flat_map(|entry| {
            entry.events.iter().map(|e| {
                sustena_core::event::Event::backfilled(
                    format!("{}-{}", entry.seq, e.name),
                    e.name.clone(),
                    entry.seq as i64,
                    sustena_core::event::CausalStamp::new("host"),
                )
            })
        })
        .collect();

    let (reading, view) =
        crate::orchie::feed(&world.operators, &state, queued.len(), &recent, query.as_deref())
            .map_err(|errors| errors.join("; "))?;

    let emits_of = |id: &str| -> Vec<String> {
        crate::orchie::widget_declarations()
            .into_iter()
            .find(|w| w.id == id)
            .map(|w| w.emits)
            .unwrap_or_default()
    };

    let cards: Vec<CardDto> = view
        .selected
        .iter()
        .map(|c| CardDto {
            id: c.id.clone(),
            render: c.render.clone(),
            rank: c.rank as u32,
            urgency: c.urgency,
            measured: c.basis.is_measured(),
            basis: c.basis.describe(),
            relevance: c.relevance,
            score: c.score,
            cost: c.cost as u32,
            eligibility: c.why.describe(),
            emits: emits_of(&c.id),
        })
        .collect();

    // ★★ Withdrawn and excluded in ONE list, each saying which it is — the
    //    "N stayed quiet" line has to be able to tell them apart.
    let mut quiet: Vec<QuietDto> = view
        .withdrawn
        .iter()
        .map(|w| QuietDto {
            id: w.id.clone(),
            reason: Some(w.reason.clone()),
            score: None,
            withdrew: true,
        })
        .collect();
    quiet.extend(view.excluded.iter().map(|c| QuietDto {
        id: c.id.clone(),
        reason: None,
        score: Some(c.score),
        withdrew: false,
    }));

    // The standing things, each with a real "why".
    let mut attention: Vec<AttentionDto> = Vec::new();
    if let Some(pocket) = reading.get("worst_pocket").and_then(|v| v.as_str()) {
        let spent = reading.get("worst_spent").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let allocated = reading.get("worst_allocated").and_then(|v| v.as_f64()).unwrap_or(1.0);
        if spent >= allocated {
            attention.push(AttentionDto {
                kind: "pocket".into(),
                what: pocket.to_string(),
                why: format!(
                    "{spent:.0} of {allocated:.0} — past the limit you set for it"
                ),
                severity: "danger".into(),
                message_id: None,
            });
        }
    }
    for m in &queued {
        attention.push(AttentionDto {
            kind: "capture".into(),
            what: m
                .parsed_fields
                .get("counterparty")
                .and_then(|v| v.as_str())
                .unwrap_or("a captured message")
                .to_string(),
            why: m.reason.clone(),
            severity: "warn".into(),
            message_id: Some(m.id.clone()),
        });
    }

    Ok(FeedDto {
        sustain_id: sustain_id.clone(),
        label,
        cards,
        quiet,
        budget: view.budget as u32,
        spent: view.spent as u32,
        candidates_considered: view.candidates_considered as u32,
        reading,
        attention,
        rollup: world.rollup(&sustain_id),
        liquid: state.pointer("/finances/liquid/balance").and_then(Value::as_f64),
    })
}

fn pockets_of(state: &Value) -> Vec<String> {
    state
        .pointer("/finances/pockets")
        .and_then(|p| p.as_object().map(|o| o.keys().cloned().collect()))
        .unwrap_or_default()
}

/// **`ε → (o, θ)`** — one inference pass over a narrated effect or a captured
/// message. Read-only: it resolves, it never writes.
#[tauri::command]
#[specta::specta]
pub fn orchie_infer(
    world: State<'_, World>,
    sustain_id: String,
    message_id: Option<String>,
    effect_text: Option<String>,
    known: Value,
    ignore_history: bool,
) -> Result<InferenceDto, String> {
    let Some(state) = world.with(|i| i.get(&sustain_id).map(|s| s.state.clone())) else {
        return Err(format!("no Sustain called '{sustain_id}'"));
    };
    let pockets = pockets_of(&state);

    let message = match &message_id {
        Some(id) => world
            .ingest()
            .current()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|m| &m.id == id),
        None => None,
    };

    let known_map: BTreeMap<String, Value> = match known {
        Value::Object(o) => o.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    let parsed = message.as_ref().map(|m| m.parsed_fields.clone()).unwrap_or_default();
    let raw = message.as_ref().map(|m| m.raw_payload.clone());

    // ★ The candidates are the CARD's own declared emits — a widget may only
    //   propose what it declared it can propose.
    let candidates: Vec<String> = crate::orchie::widget_declarations()
        .into_iter()
        .find(|w| w.id == "classify_capture")
        .map(|w| w.emits)
        .unwrap_or_default();

    // ★★ History is looked up by the SAME description the inference will
    //    resolve, so the two can never key on different things.
    let description = sustena_core::resolve_description(effect_text.as_deref(), &parsed, &known_map);
    let history = if ignore_history {
        None
    } else {
        description.as_ref().and_then(|d| world.ingest().recall(&sustain_id, d))
    };

    let capture = sustena_core::Capture {
        candidates: &candidates,
        pockets: &pockets,
        effect_text: effect_text.as_deref(),
        parsed_fields: parsed,
        known: known_map,
        history,
        raw_text: raw.as_deref(),
    };

    Ok(match sustena_core::infer(&world.operators, &capture) {
        sustena_core::Inference::Ready {
            operator,
            params,
            why,
            description,
            from_history,
            history_use_count,
        } => InferenceDto::Ready {
            operator,
            params: Value::Object(params.into_iter().collect()),
            why,
            description,
            from_history,
            history_use_count,
        },
        sustena_core::Inference::NeedsDisambiguation { field, question, options, why } => {
            InferenceDto::NeedsDisambiguation {
                field,
                question,
                options: options.map(|o| {
                    o.into_iter()
                        .map(|c| ChoiceDto { value: c.value, label: c.label })
                        .collect()
                }),
                why,
            }
        }
        sustena_core::Inference::CannotInfer { why } => InferenceDto::CannotInfer { why },
    })
}

/// **Confirm** an inferred capture: run the operator through the real gate.
///
/// ★★★ The same `World::call` the Console uses, as the unlocked principal.
/// A refusal comes back as a normal verdict — an inference made at time T can
/// honestly fail at T+n if the household moved, and that is the correct
/// outcome, not an error.
#[tauri::command]
#[specta::specta]
pub fn orchie_confirm(
    app: AppHandle,
    world: State<'_, World>,
    sustain_id: String,
    operator: String,
    params: Value,
    message_id: Option<String>,
    description: Option<String>,
) -> Result<GateResult, String> {
    let params_map: Map<String, Value> = match params {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    trace!("orchie_confirm  {sustain_id}  {operator}");

    let Some((x, seq)) = world
        .call(&sustain_id, &operator, &params_map)
        .map_err(|e| e.to_string())?
    else {
        return Err(format!("no Sustain called '{sustain_id}'"));
    };
    let result = GateResult::of(&operator, &x);

    if x.committed() {
        let msg = Committed::of(&sustain_id, &operator, seq, &x, readings(&world, &sustain_id));
        let _ = msg.emit(&app);
        if let Some(subject) = world.rollup_subject(&sustain_id) {
            if let Some(r) = world.rollup(&subject) {
                let _ = (RolledUp { rollup: r }).emit(&app);
            }
        }
        // ★ Remember the classification only on a real success, and only when
        //   there is a pocket to remember — income has none.
        if let (Some(d), Some(pocket)) = (
            description.as_deref(),
            params_map.get("pocket_name").and_then(|v| v.as_str()),
        ) {
            let _ = world.ingest().remember(&sustain_id, d, pocket);
        }
        if let Some(id) = &message_id {
            let _ = world.ingest().record_outcome(id, true, None);
        }
    } else {
        let _ = Refused::of(&sustain_id, &operator, &result).emit(&app);
        if let Some(id) = &message_id {
            let _ = world.ingest().record_outcome(id, false, result.reason.clone());
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Network
// ---------------------------------------------------------------------------

fn peer_dto(p: &crate::peers::Peer) -> PeerDto {
    PeerDto {
        public_key: p.public_key.clone(),
        handle: p.handle.clone(),
        address: p.address.clone(),
        standing: match p.standing {
            Standing::Pending => "pending",
            Standing::Trusted => "trusted",
            Standing::Blocked => "blocked",
        }
        .to_string(),
        shares: p.shares.iter().cloned().collect(),
        last_synced: p.last_synced.map(|t| t.to_string()),
        last_error: p.last_error.clone(),
    }
}

/// This node, its peers, and whether it is reachable at all.
#[tauri::command]
#[specta::specta]
pub fn get_network(world: State<'_, World>) -> NetworkDto {
    let book = world.peering().book();
    let shareable = world.with(|i| {
        i.order()
            .iter()
            .filter_map(|id| i.get(id).map(|s| (id.clone(), s.record.label.clone())))
            .collect()
    });
    NetworkDto {
        node_id: world.node_id(),
        handle: world.principal(),
        listening: world.peering().port(),
        unlocked: world.is_unlocked(),
        peers: book.all().into_iter().map(peer_dto).collect(),
        shareable,
    }
}

/// Start accepting peers. ★ A locked node refuses, because it has nothing to
/// answer a handshake with.
#[tauri::command]
#[specta::specta]
pub fn start_listening(world: State<'_, World>, port: Option<u16>) -> Result<u16, String> {
    let bound = world.listen(port)?;
    trace!("listening for peers on {bound}");
    Ok(bound)
}

/// Record a peer by key and address, without granting it anything.
#[tauri::command]
#[specta::specta]
pub fn add_peer(
    world: State<'_, World>,
    public_key: String,
    handle: String,
    address: String,
) -> Result<(), String> {
    world.peering().edit(|b| b.seen(&public_key, &handle, Some(address)))
}

/// Trust, un-trust or block a peer. ★★ Trusting is not sharing: it makes
/// sharing POSSIBLE, and each Sustain is still granted one at a time.
#[tauri::command]
#[specta::specta]
pub fn set_peer_standing(
    world: State<'_, World>,
    public_key: String,
    standing: String,
) -> Result<bool, String> {
    let standing = match standing.as_str() {
        "pending" => Standing::Pending,
        "trusted" => Standing::Trusted,
        "blocked" => Standing::Blocked,
        other => return Err(format!("no such standing: {other}")),
    };
    world.peering().edit(|b| b.set_standing(&public_key, standing))
}

/// Share one Sustain with one peer.
#[tauri::command]
#[specta::specta]
pub fn share_sustain(
    world: State<'_, World>,
    public_key: String,
    sustain_id: String,
) -> Result<(), String> {
    world.peering().edit(|b| b.share(&public_key, &sustain_id))?
}

/// Withdraw one Sustain from one peer.
#[tauri::command]
#[specta::specta]
pub fn unshare_sustain(
    world: State<'_, World>,
    public_key: String,
    sustain_id: String,
) -> Result<bool, String> {
    world.peering().edit(|b| b.unshare(&public_key, &sustain_id))
}

/// Converge one Sustain with one peer, both directions, in one session.
#[tauri::command]
#[specta::specta]
pub fn sync_with_peer(
    world: State<'_, World>,
    address: String,
    sustain_id: String,
) -> Result<SyncDto, String> {
    let report = world.sync_peer(&address, &sustain_id)?;
    trace!(
        "sync {sustain_id} with {address}: +{} / -{}",
        report.outcome.received,
        report.outcome.sent
    );
    Ok(SyncDto {
        peer: report.outcome.peer,
        handle: report.outcome.handle,
        sustain_id: report.outcome.sustain_id,
        received: report.outcome.received as u32,
        sent: report.outcome.sent as u32,
        entries: report.merge.entries as u32,
        concurrent: report.merge.concurrent.len() as u32,
        superseded: report
            .merge
            .superseded
            .iter()
            .map(|x| SupersededDto {
                path: x.path.clone(),
                winner: format!("{}:{}", short(&x.winner.node), x.winner.counter),
                loser: format!("{}:{}", short(&x.loser.node), x.loser.counter),
            })
            .collect(),
        forks: report.merge.forks.len() as u32,
        admissible: report.merge.admissible,
        violated: report.merge.violated.clone(),
        summary: report.merge.describe(),
    })
}

/// A key is 64 hex characters; a screen needs the first few.
fn short(key: &str) -> String {
    key.chars().take(8).collect()
}
