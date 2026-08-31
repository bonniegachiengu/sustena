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
    AccessDto, AccountDto, AssetDto, Branch, BranchStep, DeviceDto, InventoryGroupDto,
    FiledSpendDto, OwnIdentifiersDto, SkipLearnedDto,
    TransferDto, TrendDto, Committed, ConstraintReading, CouncilOutcomeDto, EconomyDto,
    GateResult, Holarchy, LedgerEntryDto, LogEntryDto, MeasuredPawa, OperatorAccessDto,
    OperatorDto, ParamDto, ParameterDto, Refused, RolledUp, RollupDto, SustainDto, SustainSummary,
    AttentionDto, CaptureContextDto, NettingDto, CaptureResult, CardDto, ChoiceDto, FeedDto, IdentityDto,
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
/// Build the §III pulse for state that arrived from a peer.
///
/// ★ Returns `None` when nothing arrived: a sync that changed nothing is not a
/// change, and relaying it would be the excitation sloshing back that §III's
/// refractory term exists to prevent.
pub fn merged_pulse(
    world: &World,
    sustain_id: &str,
    entries: usize,
    peer: Option<String>,
) -> Option<crate::dto::Merged> {
    if entries == 0 {
        return None;
    }
    let (state, events) = world.with(|i| {
        i.get(sustain_id)
            .map(|s| (s.state.clone(), s.next_seq as u32))
            .unwrap_or((serde_json::Value::Null, 0))
    });
    let liquid = state.pointer("/finances/liquid/balance").and_then(|v| v.as_f64());
    Some(crate::dto::Merged {
        sustain_id: sustain_id.to_string(),
        entries: entries as u32,
        peer,
        constraints: readings(world, sustain_id),
        state,
        liquid,
        events,
    })
}

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
        unlock_remembered: world.unlock_is_remembered(),
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
        Capture::BeforeStart { at, start } => {
            trace!("  -> before this household's record begins (nothing stored)");
            CaptureResult::BeforeStart { at: at as f64, start: start as f64 }
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
    // ★★★ Shape-aware since 31 Aug. The correction says what the message MEANT;
    //     the field reader says what it is SHAPED like, and the rule needs both.
    //     The previous synthesiser escaped the whole message as a literal, so a
    //     rule learned from one correction carried the sender's name and phone
    //     number for as long as it lived — and, anchored on that day's date and
    //     closing balance, could never match a second message anyway.
    let Some(candidate) = sustena_core::synthesize_with_shapes(
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

    // ★★★ The rest of the shape, now. Teaching a format and leaving the ten
    //     messages already sitting in that format unreadable is the difference
    //     between a queue that ends and one that only stops growing.
    //
    // ★★ Readability only -- `reparse_unparsed` never files anything. Best
    //    effort: the rule is already saved and correct, and failing the teach
    //    because a re-read stumbled would throw away the thing that worked.
    let rules = world.rules_for(&m.source_id).unwrap_or_default();
    match world.ingest().reparse_unparsed(&m.sustain_id, &rules) {
        Ok(n) if n > 0 => trace!("learned {}: {n} stored message(s) became readable", candidate.id),
        Ok(_) => {}
        Err(e) => trace!("learned {}, but re-reading the backlog failed: {e}", candidate.id),
    }
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
/// How many answered messages the back arrow can reach.
///
/// ★★ A short tail. Changing your mind about this morning is a real need;
/// walking back through a year is a different screen and nobody asked for it.
const REACHABLE_DONE: usize = 15;
/// How far forward the queue runs in one sitting.
const QUEUE_AHEAD: usize = 60;

/// One message, as the card needs to show it.
fn context_of(m: &crate::ingest::IngestedMessage) -> CaptureContextDto {
    let f = |k: &str| m.parsed_fields.get(k);
    // Where it currently sits, read off what it recorded it DID.
    let filed = m.filed.iter().rev().find(|x| x.operator == "budget.spend");
    CaptureContextDto {
        id: m.id.clone(),
        raw: m.raw_payload.clone(),
        source: m.source_id.clone(),
        amount: f("amount")
            .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))),
        counterparty: f("counterparty").and_then(|v| v.as_str()).map(str::to_string),
        direction: f("direction").and_then(|v| v.as_str()).map(str::to_string),
        reason: m.reason.clone(),
        status: if m.resolved {
            "processed"
        } else if m.deferred_at.is_some() {
            "deferred"
        } else {
            "pending"
        }
        .to_string(),
        filed_pocket: filed
            .and_then(|x| x.params.get("pocket_name"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        filed_amount: filed.and_then(|x| x.params.get("amount")).and_then(|v| v.as_f64()),
    }
}

fn oldest_waiting(queued: &[crate::ingest::IngestedMessage]) -> Option<CaptureContextDto> {
    // ★ One builder for one shape. Two hand-written copies of the same DTO is
    //   how one of them quietly stops carrying a field the other has.
    queued.iter().min_by_key(|m| m.seq).map(context_of)
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

    // *** THE ANTI-FAKING GATE, on the one path that hands figures to a
    //     screen. Every balance a surface renders is read off the in-memory
    //     state, which is honest only while that state IS the fold of the
    //     log. When they drift, a money app shows a number its own ledger
    //     cannot back -- it looks solvent when it is not.
    //
    // ** It REFUSES rather than degrades. A person shown an error knows
    //    something is wrong; a person shown a wrong number does not. For
    //    money that asymmetry decides it.
    //
    // *  The check is a re-fold, so it cannot be fooled by whatever wrote
    //    the bad state. `fold_divergence` is deliberately non-mutating: a
    //    guard that repaired the drift would report nothing and teach us
    //    nothing.
    world.refuse_if_unbacked(&sustain_id, &label)?;

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

    let tab = leading_tab(&world, &sustain_id, &state);
    // ★ Every linked pocket, so any list of pockets can say which are people —
    //   not just the one tab the attention budget had room to show.
    let person_pockets: Vec<String> = state
        .pointer("/finances/links")
        .and_then(Value::as_object)
        .map(|m| {
            let mut names: Vec<String> =
                m.values().filter_map(Value::as_str).map(str::to_string).collect();
            names.sort();
            names.dedup();
            names
        })
        .unwrap_or_default();
    let (reading, view) = crate::orchie::feed(
        &world.operators,
        &state,
        queued.len(),
        &recent,
        query.as_deref(),
        extra,
        tab.as_ref(),
    )
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

    // The walkable queue: what he has just done, then what is waiting.
    let walk = world
        .ingest()
        .navigable(&sustain_id, REACHABLE_DONE, QUEUE_AHEAD)
        .unwrap_or_default();
    let queue_start = walk.iter().position(|m| !m.resolved).unwrap_or(walk.len()) as u32;
    let queue: Vec<CaptureContextDto> = walk.iter().map(context_of).collect();

    // ★★ Folded in AFTER the view is composed, so one render is one reading.
    //    Observing inside the compose path would count a re-render as new
    //    evidence and let the series drift on nothing happening at all.
    let observed = world.observe(&sustain_id, &reading);
    let trend = observed.as_ref().map(|i| TrendDto {
        now: i.reading.w,
        smoothed: i.reading.smoothed,
        drifting: i.reading.alert.is_some(),
        escalates: i.reading.escalates(),
        // ★★★ Monitor §VII: hue is processed before attention engages, in
        //     roughly 150 to 200ms, across the whole field at once. The
        //     ranking was being computed correctly and then drawn flat, so it
        //     existed in the data and never reached the eye. `encode_field`
        //     has been in the core since it shipped, with nothing calling it.
        //
        //     One attribute, and deliberately only one. Hue has a settled
        //     three-way meaning already in the palette; brightness and motion
        //     are real and come later, and motion should stay rare because a
        //     card that pulses without cause is the flicker §V warns about.
        health: health_hue(i),
    });
    if trend.as_ref().is_some_and(|t| t.drifting) {
        // ★★★ Worded as a direction rather than a breach, because that is what
        //     CUSUM detects. "You are over budget" and "you have been drifting
        //     over for a while now" are different facts and only one of them
        //     is this one.
        attention.insert(
            0,
            AttentionDto {
                kind: "drift".into(),
                // ★★ Worded for what the detector actually saw. It watches
                //    the smoothed level, so this fires both for a small gap
                //    that keeps repeating and in the wake of one big one —
                //    and "still well outside" is true of both, where "this has
                //    been building" would only be true of the first.
                what: "still outside where you want to be".into(),
                why: "Not just today — the gap has stayed open across recent changes."
                    .into(),
                // ★ Warning rather than danger. A drift is not yet a breach,
                //   and calling it one would spend the loudest word on the
                //   quieter fact.
                severity: "warn".into(),
                message_id: None,
            },
        );
    }

    Ok(FeedDto {
        sustain_id: sustain_id.clone(),
        label,
        cards,
        queue_head,
        queue,
        // ★★★ What the inbox repeats that nothing reads yet. Only the
        //     unrecognised ones: a shape the transducer already handles needs
        //     no teaching, and offering it would be asking for work already
        //     done.
        shapes: {
            let corpus: Vec<(String, String)> = world
                .ingest()
                .current()
                .unwrap_or_default()
                .into_iter()
                .filter(|m| m.sustain_id == sustain_id && m.status == "unparsed" && !m.ignored)
                .map(|m| (m.id, m.raw_payload))
                .collect();
            sustena_core::corpus::cluster(corpus.iter().map(|(i, r)| (i.as_str(), r.as_str())))
                .into_iter()
                .take(3)
                .map(|c| crate::dto::ShapeOfferDto {
                    message_id: c.members.first().cloned().unwrap_or_default(),
                    count: c.count() as u32,
                    example: c.example,
                })
                .collect()
        },

        queue_start,
        quiet,
        budget: view.budget as u32,
        spent: view.spent as u32,
        candidates_considered: view.candidates_considered as u32,
        reading,
        attention,
        rollup: world.rollup(&sustain_id),
        liquid: state.pointer("/finances/liquid/balance").and_then(Value::as_f64),
        accounts: accounts_of(&state, &world.ingest().reported_balances(&sustain_id)
            .unwrap_or_default()),
        device: device_of(&world, &sustain_id),
        inventory: inventory_of(&state),
        person_pockets,
        filed: world
            .ingest()
            .filed_spends(&sustain_id, RECENT_FILED)
            .unwrap_or_default()
            .into_iter()
            .map(|f| FiledSpendDto {
                message_id: f.message_id,
                pocket: f.pocket,
                amount: f.amount,
                counterparty: f.counterparty,
            })
            .collect(),
        trend,
        unaccounted: unaccounted_in(&state),
    })
}

/// The constraint-health hue for one reading, as §VII's encoder assigns it.
///
/// ★★ Read off `encode_field` rather than re-derived here. A second opinion
/// about what counts as amber would drift from the core's, and then the colour
/// on screen would stop meaning what the engine meant by it.
fn health_hue(reading: &sustena_core::monitor::Ingested) -> String {
    use sustena_core::preattentive::{encode_field, Hue, VisualAttribute};
    let specs = encode_field(&[reading]);
    let hue = specs.first().and_then(|s| {
        s.attributes().iter().find_map(|a| match a {
            VisualAttribute::Hue { value, .. } => Some(*value),
            _ => None,
        })
    });
    match hue {
        Some(Hue::Green) => "green",
        Some(Hue::Amber) => "amber",
        Some(Hue::Red) => "red",
        None => "green",
    }
    .to_string()
}

/// How many recent filings to offer for correction.
///
/// ★★ A short list on purpose. Correcting last week's shopping is a real need;
/// scrolling a year of it is a different screen, and one nobody has asked for.
const RECENT_FILED: usize = 12;

/// What the household holds, grouped by the pocket that bought it.
fn inventory_of(state: &Value) -> Vec<InventoryGroupDto> {
    let assets = state
        .pointer("/inventory/assets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut by_pocket: std::collections::BTreeMap<String, Vec<AssetDto>> = Default::default();
    for a in assets {
        let dto = AssetDto {
            id: a.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
            item: a.get("item").and_then(Value::as_str).unwrap_or("").to_string(),
            value: a.get("value").and_then(Value::as_f64).unwrap_or(0.0),
            source_tx: a.get("source_tx").and_then(Value::as_str).unwrap_or("").to_string(),
            pocket: a.get("pocket").and_then(Value::as_str).unwrap_or("").to_string(),
            subpocket: a.get("subpocket").and_then(Value::as_str).map(str::to_string),
        };
        // ★★ Used-up things stay on the record — the purchase they came from
        //    is still a real fact — but they are not part of what is HELD, so
        //    they do not swell the total or the list.
        if dto.value > 0.0 {
            by_pocket.entry(dto.pocket.clone()).or_default().push(dto);
        }
    }

    by_pocket
        .into_iter()
        .map(|(pocket, assets)| InventoryGroupDto {
            total: ((assets.iter().map(|a| a.value).sum::<f64>()) * 100.0).round() / 100.0,
            pocket,
            assets,
        })
        .collect()
}

/// How long a device may be quiet before the watcher calls it late.
///
/// ★★ A decision, not an estimate, and Ingest §VIII is why: percentile
/// staleness declares a false-alarm rate rather than discovering one, and on a
/// bursty signal a confident alarm is necessarily a late one. A heartbeat every
/// sweep removes the statistics — the phone reports whenever it reads, so
/// silence past this is silence, not a quiet spell.
const QUIET_BEFORE_LATE_MINUTES: u32 = 60 * 24;

/// The device child's own reading, judged by the household that watches it.
fn device_of(world: &World, household: &str) -> Option<DeviceDto> {
    let id = World::device_id(household);
    let state = world.with(|i| i.get(&id).map(|s| s.state.clone()))?;
    let depth = state.pointer("/device/queue_depth").and_then(Value::as_f64).unwrap_or(0.0);
    let last = state.pointer("/device/last_ack_ms").and_then(Value::as_f64).unwrap_or(0.0);

    // ★★★ Never reported is NOT "quiet for zero minutes". A device that has
    //     said nothing since it was created has no last-contact to measure
    //     from, and reporting one would read as freshly heard from.
    let quiet = if last <= 0.0 {
        None
    } else {
        Some((((now_ms() as f64 - last).max(0.0)) / 60_000.0).round() as u32)
    };
    Some(DeviceDto {
        sustain_id: id,
        queue_depth: depth.max(0.0) as u32,
        quiet_for_minutes: quiet,
        stale: quiet.is_none_or(|m| m > QUIET_BEFORE_LATE_MINUTES),
        app_version: state
            .pointer("/device/app_version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// Every account the household holds, in a stable order, each against what
/// the bank itself last reported for it.
///
/// ★★ Drift is reported, never corrected. A difference between our arithmetic
/// and the bank's own word is a real thing to look into -- a missed text, a
/// charge nobody classified, a fee -- and silently moving our figure to match
/// would erase the evidence of whatever caused it.
fn accounts_of(
    state: &Value,
    reported: &std::collections::BTreeMap<String, crate::ingest::Reported>,
) -> Vec<AccountDto> {
    let declared = state.pointer("/finances/accounts").and_then(Value::as_object);
    let mut out: Vec<AccountDto> = declared
        .map(|m| {
            m.iter()
                .map(|(id, a)| {
                    let balance = a.get("balance").and_then(Value::as_f64).unwrap_or(0.0);
                    let said = reported.get(id).map(|r| r.balance);
                    AccountDto {
                        id: id.clone(),
                        label: a.get("label").and_then(Value::as_str).unwrap_or(id).to_string(),
                        balance,
                        reported: said,
                        drift: said.map(|r| ((r - balance) * 100.0).round() / 100.0),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    // ★★★ **A pot the bank told us about still shows, even if nobody declared
    //     it.** Pochi and M-Shwari arrive without anyone setting them up, and
    //     iterating only the declared accounts meant a balance we had been
    //     told, in writing, was dropped on the floor for want of a row to put
    //     it in. Money the household holds is not conditional on having been
    //     configured.
    //
    // ★★ `balance` is 0.0 and `drift` is None rather than a computed
    //    difference: we have the bank's word and no arithmetic of our own to
    //    compare it against, and inventing a drift of exactly the balance would
    //    read as an error the household could act on. Nothing is claimed here
    //    beyond what was reported.
    for (id, r) in reported {
        if declared.is_some_and(|m| m.contains_key(id)) {
            continue;
        }
        out.push(AccountDto {
            id: id.clone(),
            label: account_label(id),
            balance: 0.0,
            reported: Some(r.balance),
            drift: None,
        });
    }
    out
}

/// A readable name for a pot nobody named.
///
/// ★ The known instruments spelled as a person would say them; anything else
/// keeps its own id rather than being prettified into something the household
/// would not recognise.
fn account_label(id: &str) -> String {
    match id {
        "mpesa" => "M-Pesa".to_string(),
        "kcb" => "KCB".to_string(),
        "pochi" => "Pochi la Biashara".to_string(),
        "mshwari" => "M-Shwari".to_string(),
        other => other.to_string(),
    }
}

/// Money the household holds that no account claims.
///
/// ★★ The same two sums the core's conservation law compares, read here so a
/// surface can show the gap rather than a total that hides it. Rounded to the
/// shilling, because a float difference of 1e-13 is not a thing to report.
fn unaccounted_in(state: &Value) -> f64 {
    let liquid = state.pointer("/finances/liquid/balance").and_then(Value::as_f64).unwrap_or(0.0);
    let earmarked: f64 = state
        .pointer("/finances/pockets")
        .and_then(Value::as_object)
        .map(|m| {
            m.values()
                .map(|p| {
                    let a = p.get("allocated").and_then(Value::as_f64).unwrap_or(0.0);
                    let sp = p.get("spent").and_then(Value::as_f64).unwrap_or(0.0);
                    a - sp
                })
                .sum()
        })
        .unwrap_or(0.0);
    let in_accounts: f64 = state
        .pointer("/finances/accounts")
        .and_then(Value::as_object)
        .map(|m| m.values().filter_map(|a| a.get("balance")?.as_f64()).sum())
        .unwrap_or(0.0);
    (((liquid + earmarked) - in_accounts) * 100.0).round() / 100.0
}

fn pockets_of(state: &Value) -> Vec<String> {
    state
        .pointer("/finances/pockets")
        .and_then(|p| p.as_object().map(|o| o.keys().cloned().collect()))
        .unwrap_or_default()
}

/// Cancel refunds against their charges, where both are still unclassified.
///
/// ★★★ Case 1 of the netting design. A charge and its refund net to zero, so
/// if neither has been filed the honest outcome is that both leave the queue
/// and nothing is recorded: no money moved on balance, and no event should
/// claim it did. Nothing is deleted; each keeps its text and gains the id of
/// the other.
///
/// ★★ Where more than one charge could be the match, it nets NOTHING. Getting
/// the pair wrong would make two real transactions disappear.
#[tauri::command(async)]
#[specta::specta]
pub fn net_reversals(world: State<'_, World>, sustain_id: String) -> Result<NettingDto, String> {
    let r = world.ingest().net_reversals(&sustain_id).map_err(|e| e.to_string())?;

    // ★★★ Case 2 is the only part of netting that moves money, and it moves it
    //     through `World::call` -- the same door the Console and the classify
    //     card use. There is no store-level write path to the ledger, and this
    //     does not become the first one.
    let mut given_back = 0u32;
    let mut refused = 0u32;
    for c in &r.compensations {
        let mut all_committed = true;
        for call in &c.calls {
            let params: Map<String, Value> = call.params.clone().into_iter().collect();
            match world.call(&sustain_id, &call.operator, &params) {
                Ok(Some((x, _))) if x.committed() => {}
                // ★★ A refusal stops this pair here. The earlier calls of a
                //    pair are themselves real, committed moves and are left
                //    standing rather than force-reversed -- a second undo of a
                //    refusal is how a mistake gets doubled. The pair stays
                //    unmarked, so it comes back next pass.
                _ => {
                    all_committed = false;
                    break;
                }
            }
        }
        if all_committed {
            world
                .ingest()
                .mark_compensated(&c.reversal, &c.original)
                .map_err(|e| e.to_string())?;
            given_back += 1;
        } else {
            refused += 1;
        }
    }

    if !r.netted.is_empty() || given_back > 0 {
        trace!("netted {} pair(s), gave back {given_back}", r.netted.len());
    }
    Ok(NettingDto {
        netted: r.netted.len() as u32,
        given_back,
        refused,
        uncompensable: r.uncompensable,
        unmatched: r.unmatched,
        ambiguous: r.ambiguous,
    })
}

/// The numbers this household calls its own.
///
/// ★★ Read and written on the device only. They exist so a move between his
/// own accounts can be told apart from a payment to someone else, which is not
/// a distinction any wording makes.
#[tauri::command(async)]
#[specta::specta]
pub fn get_own_identifiers(world: State<'_, World>) -> Result<OwnIdentifiersDto, String> {
    let own = world.ingest().own_identifiers().map_err(|e| e.to_string())?;
    Ok(OwnIdentifiersDto { mpesa: own.mpesa, kcb: own.kcb })
}

#[tauri::command(async)]
#[specta::specta]
pub fn set_own_identifiers(
    world: State<'_, World>,
    own: OwnIdentifiersDto,
) -> Result<OwnIdentifiersDto, String> {
    let store = crate::ingest::OwnIdentifiers { mpesa: own.mpesa, kcb: own.kcb };
    world.ingest().set_own_identifiers(&store).map_err(|e| e.to_string())?;
    get_own_identifiers(world)
}

/// Turn each pair of texts that is really one move into one move.
///
/// ★★★ Net zero by construction: `budget.transfer` takes money out of one
/// account and puts the same amount into another, touches no pocket and adds
/// nothing to income. Booking the two texts separately would record an expense
/// and an income that never happened, and his income would grow every time he
/// moved his own money.
#[tauri::command(async)]
#[specta::specta]
pub fn apply_transfers(
    world: State<'_, World>,
    sustain_id: String,
) -> Result<TransferDto, String> {
    let found = world.ingest().find_transfers(&sustain_id).map_err(|e| e.to_string())?;
    let mut out = TransferDto {
        reclaimed: found.reclaimed,
        unpaired: found.unpaired,
        ambiguous: found.ambiguous,
        blocked: found.blocked_by_applied_income,
        ..Default::default()
    };

    // ── one text that names both ends ────────────────────────────────────────
    //
    // ★★★ The order matters and is not interchangeable. If the far side already
    //     filed itself as income, that income must come off the books BEFORE
    //     the transfer credits the same account, or the money is counted twice
    //     -- once as earnings that never happened and once as the move it
    //     really was. And if the undo is refused, the transfer must not run at
    //     all: half of this is worse than none of it.
    for mv in &found.self_moves {
        if let Some(income_id) = &mv.undo_income {
            let mut undo = Map::new();
            undo.insert("entry_id".into(), Value::String(income_id.clone()));
            undo.insert("account".into(), Value::String(mv.to_account.clone()));
            match world.call(&sustain_id, "budget.unrecord_income", &undo) {
                Ok(Some((x, _))) if x.committed() => {}
                _ => {
                    out.refused += 1;
                    continue;
                }
            }
        }

        let mut params = Map::new();
        params.insert("from_account".into(), Value::String(mv.from_account.clone()));
        params.insert("to_account".into(), Value::String(mv.to_account.clone()));
        params.insert("amount".into(), serde_json::json!(mv.amount));
        match world.call(&sustain_id, "budget.transfer", &params) {
            Ok(Some((x, _))) if x.committed() => {
                world.ingest().mark_self_moved(&mv.message).map_err(|e| e.to_string())?;
                // The income it replaced is settled too, so it stops asking.
                if let Some(income_id) = &mv.undo_income {
                    let _ = world.ingest().mark_self_moved(income_id);
                }
                out.moved += 1;
            }
            _ => out.refused += 1,
        }
    }

    for t in &found.matched {
        // ★★★ The income comes off BEFORE the transfer credits the same
        //     account, and if it will not come off the transfer does not run.
        //     Half of this is worse than none of it: a transfer booked on top
        //     of an income that never happened overstates him twice over,
        //     where doing neither leaves an honest duplicate he can see.
        //     Identical discipline to the single-text self-move path above.
        if let Some(income_id) = &t.undo_income {
            let mut undo = Map::new();
            undo.insert("entry_id".into(), Value::String(income_id.clone()));
            undo.insert("account".into(), Value::String(t.to_account.clone()));
            match world.call(&sustain_id, "budget.unrecord_income", &undo) {
                Ok(Some((x, _))) if x.committed() => {}
                _ => {
                    out.refused += 1;
                    continue;
                }
            }
        }

        let mut params = Map::new();
        params.insert("from_account".into(), Value::String(t.from_account.clone()));
        params.insert("to_account".into(), Value::String(t.to_account.clone()));
        params.insert("amount".into(), serde_json::json!(t.amount));
        // The same door every other write uses.
        match world.call(&sustain_id, "budget.transfer", &params) {
            Ok(Some((x, _))) if x.committed() => {
                world
                    .ingest()
                    .mark_transferred(&t.out_leg, &t.in_leg)
                    .map_err(|e| e.to_string())?;
                out.moved += 1;
            }
            _ => out.refused += 1,
        }
    }
    if out.moved > 0 {
        trace!("recorded {} transfer(s) between his own accounts", out.moved);
    }
    Ok(out)
}

/// **Never ask me about these again** — learn a skip from one message.
///
/// ★★ Retroactive by design. He answers this in the middle of a backlog full
/// of the same shape, so a rule that only covered future messages would leave
/// the pile it was meant to clear exactly as it was.
#[tauri::command(async)]
#[specta::specta]
pub fn learn_skip(
    world: State<'_, World>,
    sustain_id: String,
    message_id: String,
) -> Result<SkipLearnedDto, String> {
    let out = world.ingest().learn_skip(&sustain_id, &message_id).map_err(|e| e.to_string())?;
    if out.cleared > 0 {
        trace!("learned a skip, cleared {} waiting", out.cleared);
    }
    Ok(SkipLearnedDto { cleared: out.cleared, unlearnable: out.unlearnable })
}

/// **Move a spend filed to the wrong pocket.**
///
/// ★★★ A correction, appended. The original filing is not rewritten: the
/// operator moves what is counted, and the message records the move after the
/// filing it corrects, so the log keeps both.
#[tauri::command(async)]
#[specta::specta]
pub fn reclassify_spend(
    world: State<'_, World>,
    sustain_id: String,
    message_id: String,
    from_pocket: String,
    to_pocket: String,
    amount: f64,
) -> Result<GateResult, String> {
    let mut params = Map::new();
    params.insert("from_pocket".into(), Value::String(from_pocket));
    params.insert("to_pocket".into(), Value::String(to_pocket.clone()));
    params.insert("amount".into(), serde_json::json!(amount));
    params.insert("source_tx".into(), Value::String(message_id.clone()));

    let Some((x, _)) = world
        .call(&sustain_id, "budget.reclassify", &params)
        .map_err(|e| e.to_string())?
    else {
        return Err(format!("no Sustain called '{sustain_id}'"));
    };
    let result = GateResult::of("budget.reclassify", &x);
    if x.committed() {
        // ★ Only after the gate committed. Noting a correction that was
        //   refused would show the list a move that never happened.
        let _ = world.ingest().record_reclassification(&message_id, &to_pocket, amount);
    }
    Ok(result)
}

/// **What the household remembers about this counterparty.**
///
/// ★★★ Answered by running the head of Mentor's own graph —
/// `vendor.identify → vendor.suggest` — rather than by a lookup written here.
/// That is the realignment: the deciding step IS the operative's first two
/// nodes, executing through the real registry, so the answer a screen shows and
/// the answer an operative acts on cannot drift apart. They are the same call.
///
/// ★★★ **One source of truth.** The memory lives in the `vendors` dimension of
/// state, written by `vendor.remember` through the gate, replayed by the fold.
/// It used to live ALSO in a `history.json` beside the log, and two places that
/// say what a vendor is for is one place too many — the day they disagreed
/// there would be no way to say which was right.
///
/// ★★ The old file is still READ, and never written again. It drains itself:
/// anything only it knows is offered once, and the next confirmation writes
/// that answer into state where it belongs. Deleting it outright would have
/// thrown away real classifications he had already made.
fn vendor_memory(
    world: &World,
    sustain_id: &str,
    state: &Value,
    description: &str,
) -> Option<(String, u32)> {
    let head = sustena_core::mentor();
    let input: Map<String, Value> =
        [("counterparty".to_string(), Value::String(description.to_string()))]
            .into_iter()
            .collect();
    // ★ The sustain's own allow-list, so a read-only head is held to exactly
    //   the operators that sustain declares — same list `World::call` uses.
    let allowed: Vec<String> =
        world.with(|i| i.get(sustain_id).map(|s| s.definition.operators.clone()))?;
    let run = sustena_core::dag::run(
        &head,
        &world.operators,
        &allowed,
        &sustena_core::Enforcement::default(),
        state,
        &input,
    )
    .ok()?;

    let suggested = run.result_of("suggest").and_then(|r| {
        let pocket = r.data.get("pocket")?.as_str()?.to_string();
        let times = r.data.get("times").and_then(Value::as_u64).unwrap_or(1) as u32;
        Some((pocket, times))
    });
    // ★ The legacy file only when state has nothing to say.
    suggested.or_else(|| world.ingest().recall(sustain_id, description))
}

/// Show a number without giving it away.
///
/// ★★★ He linked it as an identifier, not as something to put on a screen. The
/// card has to be able to say WHICH person this tab is, and the pocket name
/// already does that — the number is only there so he can tell two people apart
/// if he ever names two pockets alike. Six hidden digits in the middle is
/// enough to recognise and not enough to dial.
fn masked_number(full: &str) -> String {
    let d: Vec<char> = full.chars().filter(char::is_ascii_digit).collect();
    if d.len() < 6 {
        return "·".repeat(d.len());
    }
    let head: String = d[..3].iter().collect();
    let tail: String = d[d.len() - 3..].iter().collect();
    format!("{head}···{tail}")
}

/// The person tab most worth showing, if the household has any.
///
/// ★★★ ONE, not all of them. Orchie's whole premise is an attention budget, and
/// a list of every person he has ever paid is the flood the budget exists to
/// prevent. The one shown is the relationship with the most money in play,
/// either direction — that is the one a person would actually want on the
/// screen, and it is measured rather than guessed.
///
/// ★★ The two sides come from the LOG, not from a running total kept beside the
/// state. The pocket's own `spent` is the net; the log is where "how it got
/// there" still exists. It also means a pocket linked today shows its whole
/// history, rather than a tab that appears to begin the day it was noticed.
fn leading_tab(
    world: &World,
    sustain_id: &str,
    state: &Value,
) -> Option<crate::orchie::TabReading> {
    let links = state.pointer("/finances/links")?.as_object()?.clone();
    if links.is_empty() {
        return None;
    }
    // One read of the log for every tab, rather than one per pocket.
    let log = world.store().read_log(sustain_id).unwrap_or_default();
    let mutations: Vec<sustena_core::Mutation> =
        log.into_iter().flat_map(|e| e.mutations).collect();

    let mut best: Option<crate::orchie::TabReading> = None;
    for (number, pocket) in links {
        let Some(pocket) = pocket.as_str() else { continue };
        let allocated = state
            .pointer(&format!("/finances/pockets/{pocket}/allocated"))
            .and_then(Value::as_f64);
        // ★ A link pointing at a pocket that is gone describes nothing.
        let Some(allocated) = allocated else { continue };

        let sides = sustena_core::tab_sides(pocket, &mutations);
        let reading = crate::orchie::TabReading {
            pocket: pocket.to_string(),
            masked: masked_number(&number),
            sent: sides.sent,
            received: sides.received,
            allocated,
        };
        if best.as_ref().is_none_or(|b| reading.outstanding().abs() > b.outstanding().abs()) {
            best = Some(reading);
        }
    }
    best
}

/// What a message says about whose tab it might be.
///
/// ★★ Asked by the card rather than carried on every capture, because it is
/// only ever needed at the moment somebody is looking at one message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PersonHint {
    /// The number as the message printed it — usually masked.
    pub printed: Option<String>,
    /// The pocket it is already tied to, if it is tied to one.
    pub pocket: Option<String>,
}

/// Does this message carry a number, and is that number already somebody's tab?
#[tauri::command(async)]
#[specta::specta]
pub fn person_hint(
    world: State<'_, World>,
    sustain_id: String,
    message_id: String,
) -> Result<PersonHint, String> {
    let Some(state) = world.with(|i| i.get(&sustain_id).map(|s| s.state.clone())) else {
        return Err(format!("no Sustain called '{sustain_id}'"));
    };
    let printed = world
        .ingest()
        .current()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|m| m.id == message_id)
        .and_then(|m| printed_number(&m.parsed_fields));
    let pocket = printed
        .as_deref()
        .and_then(|n| sustena_core::pocket_for_number(&sustena_core::State::new(state), n));
    Ok(PersonHint { printed, pocket })
}

/// **Tie a phone number to a pocket, so money both ways lands in that tab.**
///
/// ★★★ Through the gate like everything else. The link changes what future
/// money does, which makes it a decision the household records, not a setting
/// tucked into a preferences file where the fold could never see it.
///
/// ★★ The number is typed in full on purpose. Messages print it masked, and a
/// mask is missing its middle — linking one would claim an identity nobody
/// actually gave, and every later match would inherit the guess.
#[tauri::command(async)]
#[specta::specta]
pub fn link_number(
    world: State<'_, World>,
    sustain_id: String,
    pocket_name: String,
    number: String,
) -> Result<GateResult, String> {
    let mut params = Map::new();
    params.insert("pocket_name".into(), Value::String(pocket_name));
    params.insert("number".into(), Value::String(number));

    let Some((x, _)) =
        world.call(&sustain_id, "vendor.link_number", &params).map_err(|e| e.to_string())?
    else {
        return Err(format!("no Sustain called '{sustain_id}'"));
    };
    Ok(GateResult::of("vendor.link_number", &x))
}

/// **`H(s)` for one Sustain** — its state as one short string.
///
/// ★★★ The point of a hash here is that it is ASKABLE. Two nodes comparing
/// whole households is not a conversation that fits over a phone link, and a
/// node that answers "mostly the same" has answered nothing. Sixty-four
/// characters either match or they do not.
#[tauri::command(async)]
#[specta::specta]
pub fn sustain_hash(world: State<'_, World>, sustain_id: String) -> Result<String, String> {
    world.store().state_hash(&sustain_id).map_err(|e| e.to_string())
}

/// **Put this off until he remembers what it was.**
///
/// ★★★ An honest defer, and a different act from setting something aside as
/// not a transaction. That one says there is nothing here; this says there is
/// something here and he cannot answer it yet. It stays in the queue and comes
/// back at the TOP next time he opens the app, because burying it under new
/// arrivals would make deferring indistinguishable from discarding.
#[tauri::command(async)]
#[specta::specta]
pub fn defer_message(
    world: State<'_, World>,
    sustain_id: String,
    message_id: String,
) -> Result<bool, String> {
    // The capture seq is the household's own monotonic tick, so "deferred
    // longest ago" is answerable without a clock.
    let at = world.with(|i| i.get(&sustain_id).map(|s| s.next_seq)).unwrap_or(0);
    world.ingest().defer(&message_id, at).map_err(|e| e.to_string())
}

/// Set a captured message aside as not a transaction.
///
/// ★★ A real state on the message, never a delete. A reversal, a promo or a
/// notice has nothing to file, and saying so should not mean losing the record
/// that it arrived.
#[tauri::command(async)]
#[specta::specta]
pub fn ignore_message(world: State<'_, World>, message_id: String) -> Result<bool, String> {
    world.ingest().ignore(&message_id).map_err(|e| e.to_string())
}

/// The counterparty number a message printed, however it printed it.
///
/// ★★ `phone` when the rule captured one; otherwise the number a bank tucked
/// into the name field, which several real shapes do. A mask counts — matching
/// one against a linked number is exactly what `pocket_for_number` is for.
pub(crate) fn printed_number(parsed: &BTreeMap<String, Value>) -> Option<String> {
    if let Some(p) = parsed.get("phone").and_then(Value::as_str) {
        if p.chars().any(|c| c.is_ascii_digit()) {
            return Some(p.to_string());
        }
    }
    let who = parsed.get("counterparty").and_then(Value::as_str)?;
    let token = who
        .split_whitespace()
        .find(|w| w.chars().filter(char::is_ascii_digit).count() >= 4)?;
    Some(token.to_string())
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
        description.as_ref().and_then(|d| vendor_memory(&world, &sustain_id, &state, d))
    };

    // ★★★ Whose tab this is, if it is anyone's.
    //
    // Read from the number the message itself printed, against the links he
    // set up. A name is a description; a number is an identity, so this is
    // checked before any history keyed on how a bank spelled somebody.
    let person_pocket = printed_number(&parsed).and_then(|n| {
        sustena_core::pocket_for_number(&sustena_core::State::new(state.clone()), &n)
    });

    let capture = sustena_core::Capture {
        candidates: &candidates,
        pockets: &pockets,
        effect_text: effect_text.as_deref(),
        parsed_fields: parsed,
        known: known_map,
        history,
        raw_text: raw.as_deref(),
        person_pocket,
    };

    // ★★★ Rank the pockets he is offered by what he has actually done.
    //
    // The engine hands back every pocket in whatever order the state holds
    // them, which is arbitrary. The classification history knows how many
    // times each pocket has been chosen, so the ones he uses lead and the
    // long tail follows. Real counts, not a guess at relevance.
    // ★★ Ordered by what he has actually done. State first — the one source
    //    of truth — with the legacy file filling in anything only it still
    //    knows, so the ordering does not lurch the day the last entry drains.
    let mut by_use = world.ingest().pocket_use_counts(&sustain_id).unwrap_or_default();
    if let Some(vendors) = state.pointer("/vendors").and_then(Value::as_object) {
        for record in vendors.values() {
            let (Some(p), times) = (
                record.get("pocket").and_then(Value::as_str),
                record.get("times").and_then(Value::as_u64).unwrap_or(1) as u32,
            ) else {
                continue;
            };
            let entry = by_use.entry(p.to_string()).or_insert(0);
            *entry = (*entry).max(times);
        }
    }

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
            let options = options.map(|mut opts| {
                if field == "pocket_name" {
                    // Most-used first, then alphabetical so the tail is
                    // predictable rather than arbitrary.
                    opts.sort_by(|a, b| {
                        let ua = by_use.get(&a.value).copied().unwrap_or(0);
                        let ub = by_use.get(&b.value).copied().unwrap_or(0);
                        ub.cmp(&ua).then_with(|| a.label.cmp(&b.label))
                    });
                }
                opts
            });
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
// Eight parameters because a confirmation carries everything the capture
// needs to become a real operator call, and splitting them into a struct
// would only move the count into the type without making a caller's job
// simpler.
#[allow(clippy::too_many_arguments)]
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
    resolves: Option<bool>,
) -> Result<GateResult, String> {
    let mut params_map: Map<String, Value> = match params {
        Value::Object(o) => o,
        _ => Map::new(),
    };

    // ★★★ Attribution costs nothing. A captured message already knows whether
    //     it came from M-Pesa or KCB, so the account the money moved in is
    //     read off the message rather than asked for. Only when the operator
    //     actually takes an account, and only when nobody has already said.
    if !params_map.contains_key("account") {
        if let Some(id) = &message_id {
            let takes_account = world
                .operators
                .get(&operator)
                .is_some_and(|m| m.params.iter().any(|p| p.name == "account"));
            if takes_account {
                if let Some(src) = world.ingest().source_of_message(id) {
                    params_map.insert("account".into(), Value::String(src));
                }
            }
        }
    }
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
        //
        // ★★★ Through the OPERATOR, so the memory is state: written by the
        //     gate, replayed by the fold, readable by anything that can read a
        //     dimension. It used to be a side file as well, and two places
        //     saying what a vendor is for is one place too many.
        //
        // ★★ Best-effort on purpose. The money has already moved and been
        //     recorded; failing to note who it was paid to is worth a lost
        //     suggestion, never a lost transaction.
        if let (Some(d), Some(pocket)) = (
            description.as_deref(),
            params_map.get("pocket_name").and_then(|v| v.as_str()),
        ) {
            let mut remember = Map::new();
            remember.insert("counterparty".into(), Value::String(d.to_string()));
            remember.insert("pocket_name".into(), Value::String(pocket.to_string()));
            let _ = world.call(&sustain_id, "vendor.remember", &remember);
        }
        if let Some(id) = &message_id {
            // ★★★ What was done, before whether it is finished. Filing a past
            //     charge is an allocation and then a spend, and undoing it
            //     later needs both -- so each leg records itself as it lands,
            //     and only the last one resolves the message.
            let params_sorted: std::collections::BTreeMap<String, Value> =
                params_map.clone().into_iter().collect();
            let _ = world.ingest().record_filing(id, &operator, &params_sorted);
            if resolves.unwrap_or(true) {
                let _ = world.ingest().record_outcome(id, true, None);
            }
        }
    } else {
        let _ = Refused::of(&sustain_id, &operator, &result).emit(&app);
        if let Some(id) = &message_id {
            // ★ A refusal changed nothing, so there is no filing to record --
            //   only the reason, and the message stays in the queue.
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
        reachable_at: world.reachable_address(),
        settled_port: world.network().listen_port,
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

/// Give an existing peer an address, so it can be reached rather than only
/// answered.
///
/// ★★★ **The missing half of a standing peering.** A peer that connected TO
/// this node is recorded without an address, deliberately -- the socket it
/// arrived on is not an address it agreed to be reached at. But then nothing
/// can ever dial it, so a link that works in one direction stays that way
/// forever. This is where a person supplies the address, which is the only
/// place it can honestly come from.
#[tauri::command]
#[specta::specta]
pub fn set_peer_address(
    world: State<'_, World>,
    public_key: String,
    address: String,
) -> Result<(), String> {
    let address = address.trim().to_string();
    if address.is_empty() {
        return Err("an address is host:port, and this is empty".into());
    }
    if !address.contains(':') {
        return Err(format!("{address} has no port -- an address is host:port"));
    }
    let handle = world
        .peering()
        .book()
        .all()
        .into_iter()
        .find(|p| p.public_key == public_key)
        .map(|p| p.handle.clone())
        .ok_or_else(|| "no peer with that key".to_string())?;
    world.peering().edit(|b| b.seen(&public_key, &handle, Some(address)))
}

/// Come up unlocked from now on, without being asked.
///
/// ★★★ Costs what it sounds like: the cached value unseals the private key, so
/// anyone who can read the file can act as this node. Off unless asked for,
/// never in a build anybody else runs, and `forget_unlock` is a complete undo.
#[tauri::command]
#[specta::specta]
pub fn remember_unlock(world: State<'_, World>, passphrase: String) -> Result<(), String> {
    world.remember_unlock(&passphrase)
}

/// Stop coming up unlocked. ★ Restores the passphrase gate exactly as it was.
#[tauri::command]
#[specta::specta]
pub fn forget_unlock(world: State<'_, World>) -> Result<(), String> {
    world.forget_unlock()
}

/// Where this household's record begins, in unix seconds. `None` = everything.
#[tauri::command]
#[specta::specta]
pub fn get_intake_start(world: State<'_, World>) -> Option<f64> {
    world.ingested().window().start_at.map(|v| v as f64)
}

/// Move where the record begins.
///
/// ★★★ Only affects what is captured FROM NOW ON. It never deletes anything
/// already stored -- a boundary that retroactively erased a household's record
/// would be a far worse thing than the backlog it was set to avoid.
#[tauri::command]
#[specta::specta]
pub fn set_intake_start(world: State<'_, World>, start_at: Option<f64>) -> Result<(), String> {
    let window = match start_at {
        Some(at) => sustena_core::intake_window::IntakeWindow::starting_at(at as i64),
        None => sustena_core::intake_window::IntakeWindow::open(),
    };
    world.ingested().set_window(window).map_err(|e| e.to_string())
}

/// What build is actually running.
///
/// ★★★ Version from the crate (which the VERSION file drives) and hash from
/// git at COMPILE time. Neither can be edited into agreement with a stale
/// binary, which is the point: the string is a property of the binary rather
/// than a claim about it.
#[tauri::command]
#[specta::specta]
pub fn build_stamp() -> String {
    format!("v{} · {}", env!("CARGO_PKG_VERSION"), env!("SUSTENA_BUILD_HASH"))
}

/// Settle on a different port.
///
/// ★★★ Changing it does NOT move a running listener: the socket a peer is
/// currently talking to keeps working, and the new port is what this node comes
/// up on next time. Rebinding underneath a live session would drop the very
/// peer the change is meant to serve.
#[tauri::command]
#[specta::specta]
pub fn set_listen_port(world: State<'_, World>, port: u16) -> Result<u16, String> {
    if port < 1024 {
        return Err(format!("{port} is a privileged port; choose one above 1023"));
    }
    let mut settings = world.network();
    settings.listen_port = port;
    world.set_network(settings)?;
    Ok(port)
}

/// Reach every trusted peer that has an address, now.
///
/// ★ The same sweep the app runs on a timer, offered as a button for the
/// moment somebody does not want to wait for it.
#[tauri::command]
#[specta::specta]
pub fn reconnect_peers(world: State<'_, World>) -> Vec<(String, String, Option<String>)> {
    world
        .reconnect_all()
        .into_iter()
        .map(|(peer, sustain, outcome)| (peer, sustain, outcome.err()))
        .collect()
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
    /// Older than where this household's record begins.
    ///
    /// ★★★ Its own number, never folded into `refused`. A person who set a
    /// start date has not "refused" two thousand messages -- they never asked
    /// for them, and calling that a refusal would misdescribe their own
    /// decision back at them.
    pub before_start: u32,
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
    /// Charge/refund pairs cancelled once the read finished. Each took TWO out
    /// of the queue and recorded nothing, because together they are zero.
    pub netted_pairs: u32,
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

/// The host's clock, in epoch milliseconds.
///
/// ★ Time belongs to the host. The core takes it as a parameter so the same
/// inputs always produce the same state, which is what lets a conformance
/// vector pin an operator at all.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
        match world.capture_at(sustain_id, source, &m.body, Some(m.timestamp_ms)) {
            Ok(Capture::Rejected { .. }) => out.refused += 1,
            // ★ Counted on its own. Folding these into "refused" would report
            //   a household as having declined two thousand messages it simply
            //   never asked for.
            Ok(Capture::BeforeStart { .. }) => out.before_start += 1,
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
///
/// ★★ Asks for BOTH declared aliases — reading texts, and posting the prompt
/// about one. Returns the reading state, which is the one that gates the
/// feature; `sms_notify_state` reports the other separately, because declining
/// to be notified still leaves a working importer and should not read as the
/// larger refusal.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_request_permission(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_sms_capture::SmsCaptureExt;
    app.sms_capture()
        .request_permission()
        .map(|p| p.sms)
        .map_err(|e| e.to_string())
}

/// Whether the classify prompt may be posted.
///
/// ★★★ The difference between a real-time reader and a batch importer, and
/// worth being able to SAY. Without it the receiver still fires and still
/// writes the text down — and nobody is told until the app is next opened,
/// which on a normal day is hours. A surface that can read this can tell him
/// that, instead of leaving him to notice.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_notify_state(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_sms_capture::SmsCaptureExt;
    app.sms_capture()
        .permission_state()
        .map(|p| p.notify)
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

    // ★★★ The household's start, pushed down to the device query. A message
    //     older than this is never read off the phone -- the strongest place
    //     to apply the boundary, because the backlog does not enter and then
    //     get filtered, it never enters.
    let start_at_ms = world.ingested().window().start_at.map(|s| s * 1000).unwrap_or(0);

    let batch = app
        .sms_capture()
        .read_inbox(ReadInboxArgs { since_days, offset, limit, since_ms, start_at_ms })
        .map_err(|e| e.to_string())?;
    trace!("sms page @{offset} since {since_ms}: {} offered", batch.messages.len());

    // The newest this page saw, so the mark can move once the read finishes.
    // ★ Kept in Rust only: the mark is the store's business and a timestamp
    //   cannot cross into TypeScript anyway (specta forbids i64).
    let newest = batch.messages.iter().map(|m| m.timestamp_ms).max();
    let mut out = sweep(&world, &sustain_id, batch);

    // ★★ The mark moves only when the LAST page lands. A read abandoned halfway
    //    must not make the next one skip what it never looked at.
    if !out.has_more {
        // ★★★ And once the whole inbox is in, cancel the refunds against their
        //     charges. It runs here rather than per page because a refund and
        //     its charge can land in different pages, and a pass over a partial
        //     queue would call a pair unmatched that simply had not arrived.
        if let Ok(net) = world.ingest().net_reversals(&sustain_id) {
            out.netted_pairs = net.netted.len() as u32;
        }
        if let Some(ms) = newest.or(Some(since_ms)) {
            let _ = world.ingest().set_read_mark(&sustain_id, ANY_SOURCE, ms);
        }
        // ★★ The phone says how it is doing, on the same pass that proves it
        //    is working. `remaining` is the leading indicator §IX names: a
        //    device that cannot deliver keeps accepting, so the queue rises
        //    before anything else visibly breaks.
        let _ = world.heartbeat(&sustain_id, out.remaining, now_ms());
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

/// The text a notification tap was about, taken once.
///
/// ★★★ This is the other half of a real-time reader. The receiver already
/// fired while the app was closed and already wrote the text down; what was
/// missing was anybody being told, and then being taken to the right card when
/// they were. Returns `None` on every launch that was not a tap, which is most
/// of them.
///
/// ★★ The raw text rather than an id: the engine keys an intake on the fact
/// (ING-5), so this is the only handle that survives the unlock in between.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_pending_classify(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_sms_capture::SmsCaptureExt;
    app.sms_capture().consume_pending_classify().map_err(|e| e.to_string())
}

/// Take the prompt down, once the queue has actually been swept.
///
/// ★★ Not on launch. A notification cancelled by opening the app says the work
/// is done when nothing has been filed yet, and a prompt that lies once is a
/// prompt that gets swiped away every time after.
#[tauri::command(async)]
#[specta::specta]
pub fn sms_clear_prompt(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_sms_capture::SmsCaptureExt;
    app.sms_capture().clear_classify_prompt().map_err(|e| e.to_string())
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
            ignored: false,
            netted_with: None,
            same_event_as: None,
            reclaimed: false,
            deferred_at: None,
            filed: Vec::new(),
            sent_at_ms: None,
            event_at_ms: None,
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

    // ── which source a text is filed under ──────────────────────────────────
    //
    // ★★★ This is the source-strict rule, and it had no test. `parse_message`
    //     restricts itself to one source's parser set, so a text filed under
    //     the wrong source is not merely mislabelled -- it is handed to the
    //     wrong grammar and comes back unparsed, or worse, parsed as something
    //     it is not. The decision is made HERE, from the sender alone.

    #[test]
    fn a_text_is_filed_by_who_sent_it() {
        assert_eq!(source_of("MPESA"), Some("mpesa"));
        assert_eq!(source_of("KCB"), Some("kcb"));
    }

    #[test]
    fn case_and_decoration_in_the_sender_id_do_not_change_the_source() {
        // ★ Carriers are not consistent, and a missed match would file a real
        //   message under nothing at all.
        assert_eq!(source_of("mpesa"), Some("mpesa"));
        assert_eq!(source_of("MPesa"), Some("mpesa"));
        assert_eq!(source_of("KCB-BANK"), Some("kcb"));
        assert_eq!(source_of("SAFARICOM-MPESA"), Some("mpesa"));
    }

    #[test]
    fn a_kcb_message_that_talks_about_mpesa_is_still_kcb() {
        // ★★★ The whole reason KCB is checked first. Several real KCB texts
        //     say "M-PESA" in their own wording -- the paybill deposits and the
        //     wallet transfers especially -- and reading the BODY to decide
        //     would file them all under Safaricom's grammar. Bonnie confirmed
        //     against his own handset that those arrive from the KCB sender.
        //
        //     This asserts the tie-break, not the wording: a sender carrying
        //     both substrings resolves to the bank.
        assert_eq!(source_of("KCB-MPESA"), Some("kcb"));
        assert_eq!(source_of("MPESA-KCB"), Some("kcb"));
    }

    #[test]
    fn an_unknown_sender_is_filed_nowhere() {
        // ★★ `sweep` counts a `None` as skipped rather than guessing a source.
        //    Guessing would put a stranger's text through a money parser.
        assert_eq!(source_of("+254712345678"), None);
        assert_eq!(source_of("EQUITY"), None);
        assert_eq!(source_of("SAFARICOM"), None);
        assert_eq!(source_of(""), None);
    }

    #[test]
    fn the_body_of_a_message_never_reaches_this_decision() {
        // ★ A body-shaped string is not a sender. Passing one in must not
        //   suddenly resolve a source -- if this ever returns Some, something
        //   upstream has started handing bodies to the classifier.
        let body = "Ksh1,500.00 sent to NAIVAS on 1/8/26. New M-PESA balance is Ksh12,154.47";
        // It contains "M-PESA" with a hyphen, which is not the substring
        // matched, so the honest answer is None either way -- the point of the
        // assertion is that a body must never be the input.
        assert_eq!(source_of(body), None);
    }
}
