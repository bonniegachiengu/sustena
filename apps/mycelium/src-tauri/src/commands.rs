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
    AttentionDto, CaptureContextDto, CaptureResult, CardDto, ChoiceDto, FeedDto, IdentityDto,
    InferenceDto, IngestDto, MessageDto, QuietDto, RuleDto, SourceDto,
    BodyDto, CoOwnerDto, InstallDto, LibraryDto, NetworkDto, OfferDto, PackageDto,
    OrderDto, PeerDto, PeerShelfDto,
    RoyaltyDto, SupersededDto,
    SyncDto,
    TransferLegDto, TransferResult, Verdict, WorldDto,
};
use crate::arena::Publication;
use crate::peers::Standing;
use sustena_core::package::{Authenticity, InstallVerdict, Integrity, Origin};
use sustena_core::royalty::Settlement;
use crate::definitions::{AuthoredDefinition, DefinitionVerdict};
use crate::templates::TemplateId;
use sustena_core::holon::Transfer as HolonTransfer;
use crate::ingest::Capture;
use tauri_plugin_sms_capture::SmsBatch;
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
/// Which capture the classify card should offer next.
///
/// ★★ Oldest first, so the same message is offered until it is dealt with
/// rather than a different one on every refresh. One id, never a list: the
/// card works the queue one message at a time, which is the disclosure machine
/// of Curated UI VII and the reason the screen cannot grow with the queue.
fn oldest_waiting(queued: &[crate::ingest::IngestedMessage]) -> Option<CaptureContextDto> {
    let m = queued.iter().min_by_key(|m| m.seq)?;
    let f = |k: &str| m.parsed_fields.get(k);
    Some(CaptureContextDto {
        id: m.id.clone(),
        raw: m.raw_payload.clone(),
        source: m.source_id.clone(),
        // A number the transducer wrote as text is still a number.
        amount: f("amount")
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))),
        counterparty: f("counterparty").and_then(|v| v.as_str()).map(str::to_string),
        direction: f("direction").and_then(|v| v.as_str()).map(str::to_string),
        reason: m.reason.clone(),
    })
}

/// ★★★ `(async)`, because this reads the whole ingest log.
///
/// A sync command runs inline on the IPC thread, which on a phone is the thread
/// that draws. `get_feed` parses every stored message to find the ones still
/// waiting, and on an inbox that had been read that was thousands of them. The
/// screen froze for about a minute after unlocking, with no reading happening
/// at all: this is what it was doing.
#[tauri::command(async)]
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

    // ★ Installed cards, read back from the arena. An undecodable one is
    //   counted rather than silently dropped — see `Arena::widgets_for`.
    let (installed, undecodable) = world.arena().widgets_for(&sustain_id);
    if undecodable > 0 {
        trace!("{undecodable} installed widget(s) no longer decode and were left out");
    }
    let extra: Vec<sustena_core::widget::WidgetDecl> =
        installed.iter().map(|w| w.to_decl()).collect();

    let (reading, view) =
        crate::orchie::feed(&world.operators, &state, queued.len(), &recent, query.as_deref(), extra)
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
    // ★★★ **No capture rows here, and that is the point.**
    //
    // Classifying already IS a widget: `classify_capture`, declared in
    // `orchie.rs`, Unit-bound, reading the `unclassified` dimension and
    // emitting the budget Enzymes. `compose(r)` scores it against everything
    // else and the knapsack decides whether it fits.
    //
    // This function used to push a row per queued message ALONGSIDE that, a
    // second surface the budget never saw. Reading a real inbox made it
    // thousands of rows and the screen tried to work every one at once. A view
    // is the knapsack's pick (Curated UI III to VI); a list that grows with the
    // number of events is what the budget exists to forbid.
    //
    // So the card is the only capture surface, and all it needs from here is
    // which message to offer next.
    // The oldest still needing a person. ONE id, never a list: the card works
    // them one at a time, which is the disclosure machine of Curated UI VII.
    let queue_head = oldest_waiting(&queued);

    Ok(FeedDto {
        sustain_id: sustain_id.clone(),
        label,
        cards,
        queue_head,
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
        protocol: crate::wire::PROTOCOL,
        // ★★ The claim is exactly what the code does and no more — no
        //    post-quantum, no formal proof, no rekeying. Overclaiming here
        //    would be the one place it really matters.
        session: "X25519 ephemeral · signed by both identity keys ·                   XChaCha20-Poly1305 per direction"
            .to_string(),
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

// ---------------------------------------------------------------------------
// The arena
// ---------------------------------------------------------------------------

fn verdict_words(v: &InstallVerdict) -> (String, String, Vec<String>) {
    match v {
        InstallVerdict::Admitted(_) => ("admitted".into(), String::new(), vec![]),
        InstallVerdict::Refused { rule, errors } => {
            ("refused".into(), (*rule).to_string(), errors.clone())
        }
        InstallVerdict::NotHere { why } => ("not_here".into(), String::new(), vec![why.clone()]),
    }
}

/// The registry: every package, judged **now**.
#[tauri::command]
#[specta::specta]
pub fn get_library(world: State<'_, World>, into: Option<String>) -> LibraryDto {
    let installs = world.arena().installs();
    // ★★ The holdings this node has actually SEEN. Empty unless a peer shelf
    //    was listed this session — which the reading reports as *nobody was
    //    asked*, honestly distinct from *nobody has it*.
    let holdings: Vec<(String, Vec<String>)> = Vec::new();
    let packages = world
        .arena()
        .all()
        .into_iter()
        .map(|p| {
            let prov = p.provenance();
            // ★★ Judged on read, not remembered from publish. A definition that
            //    typechecked yesterday can strand an instance that moved since.
            let verdict = world.judge(&p, into.as_deref());
            let (outcome, _, _) = verdict_words(&verdict);
            let install = installs.iter().find(|i| i.package_id == p.id);
            let trust = world.trust_in(&p, &holdings);
            PackageDto {
                trust: trust.standing.label().to_string(),
                trust_is_a_reading: trust.standing.is_a_reading(),
                trust_signals: trust.signals.iter().map(|s| s.describe()).collect(),
                id: p.id.clone(),
                name: p.name.clone(),
                kind: p.kind.label().to_string(),
                version: p.version.clone(),
                description: p.description.clone(),
                tags: p.tags.clone(),
                author: p.author.clone(),
                author_handle: p.author_handle.clone(),
                content_hash: p.content_hash.clone(),
                integrity: match prov.integrity {
                    Integrity::Intact => "intact".into(),
                    Integrity::Altered { .. } => "altered".into(),
                },
                authenticity: match prov.authenticity {
                    Authenticity::Signed => "signed".into(),
                    Authenticity::Unsigned => "unsigned".into(),
                    Authenticity::Forged => "forged".into(),
                },
                origin: match &prov.origin {
                    Origin::Authored => "authored here".into(),
                    Origin::Bundled => "bundled".into(),
                    Origin::FromPeer { peer } => format!("from {peer}"),
                },
                provenance: prov.describe(),
                per_mille: p.per_mille,
                installed: install.is_some(),
                installed_into: install.and_then(|i| i.into.clone()),
                verdict: verdict.describe(),
                installable: outcome == "admitted" && prov.safe_to_install(),
            }
        })
        .collect();

    let published: Vec<String> = world.arena().all().into_iter().map(|p| p.name).collect();
    let publishable = world
        .definitions()
        .into_iter()
        .filter(|d| !published.contains(&d.label))
        .map(|d| (d.id, d.label))
        .collect();
    let targets = world.with(|i| {
        i.order()
            .iter()
            .filter_map(|id| i.get(id).map(|s| (id.clone(), s.record.label.clone())))
            .collect()
    });

    LibraryDto { packages, publishable, targets, unlocked: world.is_unlocked() }
}

/// Publish an artifact. ★★★ The gate runs before the write.
#[tauri::command]
#[specta::specta]
pub fn publish_package(
    world: State<'_, World>,
    request: Publication,
) -> Result<InstallDto, String> {
    let (package, verdict) = world.publish(&request)?;
    let (outcome, rule, errors) = verdict_words(&verdict);
    trace!("publish {} → {outcome}", package.name);
    Ok(InstallDto {
        outcome,
        rule,
        errors,
        provenance: package.provenance().describe(),
        applied: None,
        summary: verdict.describe(),
    })
}

/// Install a package, through the same gate a local artifact faces.
#[tauri::command]
#[specta::specta]
pub fn install_package(
    world: State<'_, World>,
    package_id: String,
    into: Option<String>,
) -> Result<InstallDto, String> {
    let out = world.install(&package_id, into.as_deref())?;
    let (outcome, rule, errors) = verdict_words(&out.verdict);
    trace!("install {package_id} → {outcome}");
    Ok(InstallDto {
        outcome,
        rule,
        errors,
        provenance: out.provenance.describe(),
        applied: out.applied,
        summary: out.verdict.describe(),
    })
}

/// Pay a package's royalty, in juul. ★★★ Internal credit. Never money.
#[tauri::command]
#[specta::specta]
pub fn pay_royalty(
    world: State<'_, World>,
    package_id: String,
    amount: u32,
) -> Result<RoyaltyDto, String> {
    let before = world.circulation();
    let settlement = world.pay_royalty(&package_id, u64::from(amount))?;
    let after = world.circulation();
    let (outcome, shares) = match &settlement {
        Settlement::Settled { shares, .. } => (
            "settled",
            shares
                .iter()
                .map(|s| (s.role.name().to_string(), s.recipient.clone(), s.amount as u32))
                .collect(),
        ),
        Settlement::NoRoyalty => ("no_royalty", vec![]),
        Settlement::Insufficient { .. } => ("insufficient", vec![]),
    };
    Ok(RoyaltyDto {
        outcome: outcome.to_string(),
        transferred: settlement.transferred() as u32,
        shares,
        circulation_before: format!("{before:.0}"),
        circulation_after: format!("{after:.0}"),
    })
}

/// Every shared Sustain's body, and whether it could be written to now.
#[tauri::command]
#[specta::specta]
pub fn get_bodies(world: State<'_, World>) -> Vec<BodyDto> {
    let me = world.node_id().unwrap_or_default();
    let book = world.peering().book();
    let ids: Vec<(String, String, Vec<String>)> = world.with(|i| {
        i.order()
            .iter()
            .filter_map(|id| {
                let s = i.get(id)?;
                if s.record.owners.is_empty() {
                    return None;
                }
                Some((id.clone(), s.record.label.clone(), s.record.owners.clone()))
            })
            .collect()
    });

    ids.into_iter()
        .map(|(sustain_id, label, owners)| {
            let quorum = owners.len() / 2 + 1;
            let dtos: Vec<CoOwnerDto> = owners
                .iter()
                .map(|key| {
                    let is_self = *key == me;
                    let peer = book.get(key);
                    CoOwnerDto {
                        key: key.clone(),
                        handle: peer
                            .map(|p| p.handle.clone())
                            .unwrap_or_else(|| if is_self { "this node".into() } else { "unknown".into() }),
                        // ★ This node is always reachable to itself; a peer needs
                        //   an address AND trust before a round could even start.
                        reachable: is_self
                            || peer.is_some_and(|p| {
                                p.address.is_some() && p.standing == Standing::Trusted
                            }),
                        is_self,
                    }
                })
                .collect();
            let reachable = dtos.iter().filter(|o| o.reachable).count();
            let last_agreed = world.last_agreed(&sustain_id).map(|n| n as u32);
            BodyDto {
                sustain_id,
                label,
                owners: dtos,
                quorum: quorum as u32,
                reachable: reachable as u32,
                can_write: reachable >= quorum,
                tolerates_traitors: ((owners.len().saturating_sub(1)) / 3) as u32,
                last_agreed,
            }
        })
        .collect()
}

/// Declare a Sustain co-owned by a set of node keys.
#[tauri::command]
#[specta::specta]
pub fn share_ownership(
    world: State<'_, World>,
    sustain_id: String,
    // ★ Named `owners`, not `with`: `with` is a reserved word in strict-mode
    //   TypeScript, so the generated binding would not parse. The typed
    //   boundary caught it at `tsc`, which is what it is for.
    owners: Vec<String>,
) -> Result<Vec<String>, String> {
    let owners = world.share_ownership(&sustain_id, &owners)?;
    trace!("{sustain_id} is now co-owned by {} node(s)", owners.len());
    Ok(owners)
}

/// What every trusted, addressable peer is offering.
///
/// ★★★ **Only peers you are connected to.** There is no index, no mesh
/// search and no global catalogue — this is exactly the list of nodes you
/// chose to peer with, and the screen says so rather than implying a market.
#[tauri::command]
#[specta::specta]
pub fn get_peer_shelves(world: State<'_, World>) -> Vec<PeerShelfDto> {
    let here: Vec<String> =
        world.arena().all().into_iter().map(|p| p.content_hash).collect();
    world
        .peering()
        .book()
        .all()
        .into_iter()
        .filter(|p| p.standing == Standing::Trusted)
        .filter_map(|p| p.address.clone().map(|a| (p.clone(), a)))
        .map(|(peer, address)| match world.peer_offers(&address) {
            Ok((_, offers)) => PeerShelfDto {
                peer: peer.public_key.clone(),
                handle: peer.handle.clone(),
                address,
                packages: offers
                    .into_iter()
                    .map(|o| OfferDto {
                        already_here: here.contains(&o.content_hash),
                        id: o.id,
                        name: o.name,
                        kind: o.kind,
                        version: o.version,
                        description: o.description,
                        author: o.author,
                        author_handle: o.author_handle,
                        content_hash: o.content_hash,
                        signed: o.signed,
                    })
                    .collect(),
                unreachable: None,
            },
            Err(why) => PeerShelfDto {
                peer: peer.public_key.clone(),
                handle: peer.handle.clone(),
                address,
                packages: vec![],
                unreachable: Some(why),
            },
        })
        .collect()
}

/// Fetch one package from a peer. ★★ Records it; does **not** install it.
#[tauri::command]
#[specta::specta]
pub fn fetch_package(
    world: State<'_, World>,
    address: String,
    content_hash: String,
) -> Result<String, String> {
    let package = world.fetch_package(&address, &content_hash)?;
    trace!("fetched {} from {address}", package.name);
    Ok(package.id)
}

fn order_dto(o: &crate::arena::Order) -> OrderDto {
    OrderDto {
        reference: o.reference.clone(),
        package_id: o.package_id.clone(),
        package_name: o.package_name.clone(),
        by: o.by.clone(),
        paid: o.paid as u32,
        per_mille: o.per_mille,
        shares: o
            .shares
            .iter()
            .map(|(role, to, amount)| (role.clone(), to.clone(), *amount as u32))
            .collect(),
        placed_at: o.placed_at.to_string(),
    }
}

/// What this node has acquired.
#[tauri::command]
#[specta::specta]
pub fn get_orders(world: State<'_, World>) -> Vec<OrderDto> {
    world.arena().orders().iter().map(order_dto).collect()
}

/// Acquire a package: settle its royalty in **juul** and record the order.
///
/// ★★★ Internal credit only. Circulation is unchanged by construction —
/// `settle` transfers and never mints — and no part of this touches money.
#[tauri::command]
#[specta::specta]
pub fn place_order(
    world: State<'_, World>,
    package_id: String,
    on: u32,
) -> Result<OrderDto, String> {
    let order = world.place_order(&package_id, u64::from(on))?;
    trace!("order {} · {} juul", order.reference, order.paid);
    Ok(order_dto(&order))
}

// ═══════════════════════════════════════════════════════════════════════════
// SMS auto-capture
//
// The plugin reads texts and filters them. The engine is written here, through
// `world.capture`, which is the same call a pasted message makes: same
// transducer, same rules, same admission. There is no second way in.
//
// Why draining happens here and not when the text arrives: a write needs the
// unlocked key, and a text usually arrives while the phone is locked. So the
// receiver only ever puts the text in a local queue, and this runs when the
// app is open. Capturing costs nothing and needs nobody; applying needs a key.
// ═══════════════════════════════════════════════════════════════════════════

/// What one sweep did. Every number is counted from a real outcome.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SmsSweep {
    /// Read off the phone and offered to the engine.
    pub read: u32,
    /// Understood and routed on their own. Income only.
    pub applied: u32,
    /// Understood, and waiting for a person to say which pocket.
    pub needs_you: u32,
    /// Seen before. Captured once, counted here, changed nothing.
    pub duplicates: u32,
    /// No rule recognised the shape.
    pub unparsed: u32,
    /// Carried a one-time code. Nothing about them was stored.
    pub refused: u32,
    /// Read on the phone and dropped there: not from M-Pesa or KCB.
    pub skipped_other_senders: u32,
    /// Dropped on the phone as a one-time code, before reaching this side.
    pub skipped_secrets: u32,
    /// A text the engine refused outright, with the first reason.
    pub failed: u32,
    pub first_failure: Option<String>,
    /// Another page or batch is waiting.
    pub has_more: bool,
    /// Where the next page starts. Reading only.
    pub next_offset: u32,
    /// Still queued after this batch. Draining only.
    pub remaining: u32,
}

/// The inbox is read as one stream rather than per sender, so the mark is kept
/// under one key. ★ The store keys marks per source anyway, so splitting the
/// read later needs no migration.
const ANY_SOURCE: &str = "inbox";

/// M-Pesa or KCB, decided by WHO SENT IT. Never by the wording: several real
/// KCB messages say M-PESA in their own text and are still KCB.
fn source_of(sender: &str) -> Option<&'static str> {
    let s = sender.to_uppercase();
    // KCB first. A sender carrying both substrings is the bank.
    if s.contains("KCB") {
        Some("kcb")
    } else if s.contains("MPESA") {
        Some("mpesa")
    } else {
        None
    }
}

fn sweep(world: &World, sustain_id: &str, batch: SmsBatch) -> SmsSweep {
    let mut out = SmsSweep {
        skipped_other_senders: batch.filtered_out,
        skipped_secrets: batch.secrets_refused,
        has_more: batch.has_more,
        next_offset: batch.next_offset,
        remaining: batch.remaining,
        ..Default::default()
    };
    for m in batch.messages {
        let Some(source) = source_of(&m.sender) else {
            out.skipped_other_senders += 1;
            continue;
        };
        out.read += 1;
        match world.capture(sustain_id, source, &m.body) {
            Ok(Capture::Rejected { .. }) => out.refused += 1,
            Ok(Capture::Duplicate(_)) => out.duplicates += 1,
            Ok(Capture::Stored(stored)) => match stored.status.as_str() {
                "mapped" => out.applied += 1,
                "unparsed" => out.unparsed += 1,
                _ => out.needs_you += 1,
            },
            Err(e) => {
                out.failed += 1;
                if out.first_failure.is_none() {
                    out.first_failure = Some(e.to_string());
                }
            }
        }
    }
    out
}

/// Has the phone been given permission to read texts yet?
#[tauri::command(async)]
#[specta::specta]
pub fn sms_permission_state(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_sms_capture::SmsCaptureExt;
    app.sms_capture()
        .permission_state()
        .map(|p| p.sms)
        .map_err(|e| e.to_string())
}

/// Ask for it. The reason is shown in the app first, before this is called.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_request_permission(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_sms_capture::SmsCaptureExt;
    app.sms_capture()
        .request_permission()
        .map(|p| p.sms)
        .map_err(|e| e.to_string())
}

/// ★★★ **`(async)` on a sync body, and it is the whole freeze fix.**
///
/// Tauri's macro defaults a plain `fn` command to `ExecutionContext::Blocking`,
/// which the generated handler runs INLINE on the IPC thread. On a phone that
/// is the UI thread, so a command that takes a while takes the interface with
/// it. Marking it `async` on a synchronous body selects the `sync_threadpool`
/// path instead: the same code, run off the thread that draws.
///
/// Found the hard way. 6,078 texts on the reporting device, 2,779 of them
/// matching, every one captured before the one call returned. The button sat
/// reading "read my texts" the entire time, and unlocking did the same thing
/// because the queue drains there.
///
/// ONE PAGE of the backfill. The caller loops, and shows progress between
/// pages. `since_days` of 0 means the whole inbox.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_import_page(
    world: State<'_, World>,
    app: tauri::AppHandle,
    sustain_id: String,
    since_days: i32,
    offset: u32,
    limit: u32,
) -> Result<SmsSweep, String> {
    use tauri_plugin_sms_capture::{ReadInboxArgs, SmsCaptureExt};

    // ★★★ Only what arrived since the last read.
    //
    // A repeat read used to walk the whole inbox and offer every message again.
    // Nothing was double-counted, because the dedup index caught them, but two
    // thousand messages were re-read to learn that two thousand times. The mark
    // is the newest message a completed read saw; a repeat starts there.
    let since_ms = world.ingest().read_mark(&sustain_id, ANY_SOURCE).unwrap_or(0);

    let batch = app
        .sms_capture()
        .read_inbox(ReadInboxArgs { since_days, offset, limit, since_ms })
        .map_err(|e| e.to_string())?;
    trace!("sms page @{offset} since {since_ms}: {} offered", batch.messages.len());

    // The newest this page saw, so the mark can move once the read finishes.
    // ★ Kept in Rust only: the mark is the store's business and a timestamp
    //   cannot cross into TypeScript anyway (specta forbids i64).
    let newest = batch.messages.iter().map(|m| m.timestamp_ms).max();
    let out = sweep(&world, &sustain_id, batch);

    // ★★ The mark moves only when the LAST page lands. A read abandoned halfway
    //    must not make the next one skip what it never looked at.
    if !out.has_more {
        if let Some(ms) = newest.or(Some(since_ms)) {
            let _ = world.ingest().set_read_mark(&sustain_id, ANY_SOURCE, ms);
        }
    }
    Ok(out)
}

/// ONE BATCH of whatever arrived while the app was closed. Taking clears what
/// was taken, so a text is offered once; the engine's own dedup covers the
/// rest. The caller loops while `has_more`.
///
/// Bounded and off the UI thread for the same reason as the page above: this
/// runs on unlock, and a queue that had built up froze the unlock itself.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_drain_queue(
    world: State<'_, World>,
    app: tauri::AppHandle,
    sustain_id: String,
    limit: u32,
) -> Result<SmsSweep, String> {
    use tauri_plugin_sms_capture::{DrainArgs, SmsCaptureExt};
    let batch = app
        .sms_capture()
        .drain_queue(DrainArgs { limit })
        .map_err(|e| e.to_string())?;
    if !batch.messages.is_empty() {
        trace!("sms drain: {} taken, {} left", batch.messages.len(), batch.remaining);
    }
    Ok(sweep(&world, &sustain_id, batch))
}

/// How many texts are waiting, without taking any. Cheap enough to ask before
/// deciding whether to show progress at all.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_queue_depth(app: tauri::AppHandle) -> Result<u32, String> {
    use tauri_plugin_sms_capture::SmsCaptureExt;
    app.sms_capture().queue_depth().map_err(|e| e.to_string())
}

#[cfg(test)]
mod feed_surface_tests {
    use super::*;
    use crate::ingest::IngestedMessage;

    fn msg(seq: u64) -> IngestedMessage {
        IngestedMessage {
            id: format!("m{seq}"),
            sustain_id: "h".into(),
            source_id: "mpesa".into(),
            raw_payload: "Ksh100 paid to SOMEONE".into(),
            dedup_key: format!("k{seq}"),
            status: "parsed_unmapped".into(),
            parser_name: "mpesa_buygoods".into(),
            reason: "which pocket is yours to decide".into(),
            parsed_fields: Default::default(),
            external_ref: None,
            operator: None,
            params: Default::default(),
            applied: false,
            gate_reason: None,
            resolved: false,
            seq,
        }
    }

    /// ★★★ The property the freeze was a violation of.
    ///
    /// Two thousand waiting captures reach the screen as ONE id, exactly as two
    /// do. What the person sees is `compose(r)`'s pick under the budget, and
    /// nothing here grows with the queue.
    #[test]
    fn a_large_queue_reaches_the_screen_as_one_id() {
        let many: Vec<_> = (0..2_000).map(msg).collect();
        let head = oldest_waiting(&many);
        assert_eq!(head.map(|c| c.id).as_deref(), Some("m0"));
    }

    /// Oldest first, whatever order they arrive in.
    #[test]
    fn the_oldest_is_offered_first() {
        let some = vec![msg(9), msg(3), msg(7)];
        assert_eq!(oldest_waiting(&some).map(|c| c.id).as_deref(), Some("m3"));
    }

    /// An empty queue offers nothing rather than a fabricated id.
    #[test]
    fn nothing_waiting_offers_nothing() {
        assert!(oldest_waiting(&[]).is_none());
    }
}
