//! The household, as the host holds it — **many Sustains, one engine**.
//!
//! ★★★ Every operator call still goes down the same path
//! `sustena-core/examples/household_run.rs` walks: `Registry::default()`, a
//! `Definition`, `enforcement_of`, `execute_admitted`. What changed from the
//! skeleton is only *which* Sustain is on the other end and that the result is
//! written to a durable log before it is reported.
//!
//! ★★ **State lives only in the log.** A `Sustain` here caches the folded
//! state so the UI does not re-read the disk on every keystroke, but that cache
//! is only ever written from a committed `Execution` — the same value the fold
//! would produce — and a relaunch rebuilds it from the log alone.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};
use sustena_core::{
    approval::{EffectClass, NonceLedger},
    detect::CusumSpec,
    editing::Definition,
    juul::Affordability,
    // ★ `Ingested` is also the name of this app's capture store, so the trend
    //   reading keeps a longer name rather than shadowing it.
    monitor::{Ingested as TrendReading, MonitorEngine, SustainWatch},
    operator::{execute_admitted, execute_afforded, Authorization, Enforcement, Execution, Registry},
    pawa::{meter, Meter},
    predicate::check,
    region::Region,
    semantic::enforcement_of,
    compute_rollup, ChildState,
    holon::transfer as holon_transfer,
    holon::{Leg, Link, Linked, Moving, Party, Transfer},
    operator::OperatorResult,
    ParseRule,
};

use sustena_core::principal::{
    permitted, Denial, MembershipEdge, Memberships, SkinRegistry, Tier, TIER_MEMBER,
    TIER_OBSERVER, TIER_OWNER,
};

use crate::definitions::{check as check_definition, AuthoredDefinition, DefinitionVerdict};
use crate::dto::{EventDto, RollupDto};
use crate::economy::Economy;
use crate::identity::{IdentityError, IdentityStore, Unlocked};
use crate::ingest::{Capture, Ingested};
use sustena_core::sync::Reconciliation;
use crate::arena::{content_hash, signed_message, Arena, Install, Package, Publication};
use crate::widgets::AuthoredWidget;
use sustena_core::editing::Instance;
use sustena_core::package::{
    definition_installs, operator_installs, strategy_installs, widget_installs, InstallVerdict,
    Kind, Origin, PackageProvenance,
};
use sustena_core::royalty::{settle, Licence, Recipients, RevenueType, Settlement};
use crate::arena::content_hash as content_hash_of;
use crate::arena::Order;
use crate::peers::Standing;
use crate::wire::PackageOffer;
use sustena_core::trust::{PackageTrust, TrustSignal};
use sustena_core::package::{Authenticity, Integrity};
use crate::quorum::{Agreement, Slot, Write};
use sustena_core::consensus::{
    accepted_count, adopt, granted, Accepted, Body, Promise, ProposalNumber,
};
use crate::peers::{Peering, SyncOutcome};
use crate::wire::SharedSpec;
use crate::store::{LoggedEvent, Registry as Records, Store, StoreError, StoreResult, SustainRecord};
use crate::templates::{self, TemplateId};

/// ★ The local principal this app runs as.
///
/// ★★ Declared, not authenticated. There is no sign-in and the gate still runs
/// every call under `Authorization::Unchecked` — this is the name the cockpit
/// displays, and binding it to a real capability is a later slice. Said plainly
/// so nobody mistakes a label for a check.
/// The handle this machine enrols under. ★★ It is no longer *the principal* —
/// it is the name a keypair is enrolled with. Nothing acts under it until
/// [`crate::identity::IdentityStore::unlock`] has recovered the private key,
/// and every acting id comes from that unlocked identity rather than from here.
pub const DEFAULT_HANDLE: &str = "bg.myc";

/// ★ Tier a cross-Sustain transfer demands. Declared in the host beside the
/// household model, because `holon::transfer` is not a registry operator and so
/// carries no `min_privilege` of its own. Set at MEMBER: moving money is a
/// member act, not a bystander act.
pub const TRANSFER_MIN_PRIVILEGE: Tier = TIER_MEMBER;

/// One live Sustain.
pub struct Sustain {
    pub record: SustainRecord,
    pub definition: Definition,
    pub enforcement: Enforcement,
    pub state: Value,
    pub next_seq: u64,
}

/// Everything the cockpit can look at.
/// A refusal that never reached the operator, in the shape every other
/// refusal has.
///
/// ★ `state: Value::Null` and no mutations: this is not a failed run, it is a
/// run that did not happen. A caller reading `committed()` sees false either
/// way, and a caller reading the reason learns which.
fn refusal(operator: &str, reason: &str, rule: &'static str) -> Execution {
    let _ = operator;
    Execution {
        result: OperatorResult::fail(reason.to_string(), rule),
        mutations: vec![],
        events: vec![],
        state: Value::Null,
    }
}

/// What an install attempt did.
#[derive(Debug, Clone)]
pub struct Installed {
    pub verdict: InstallVerdict,
    pub provenance: PackageProvenance,
    /// The artifact's id once applied. `None` for anything refused, and also
    /// for a strategy — which was not refused and did not apply either.
    pub applied: Option<String>,
}

/// A key is 64 hex characters; a reading needs the first few.
fn short_key(key: &str) -> String {
    key.chars().take(8).collect()
}

/// The commons this host's treasury share lands in.
pub const TREASURY: &str = "treasury";

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What one peer sync did, and what the merge could not decide.
#[derive(Debug, Clone)]
pub struct SyncReport {
    pub outcome: SyncOutcome,
    pub merge: Reconciliation,
}

/// One household's `W` series, and the last thing it said.
///
/// ★ `last` exists so a re-render can repeat the most recent verdict without
/// manufacturing a new observation to produce one.
struct Trend {
    engine: Option<MonitorEngine>,
    /// The sustain sequence this series last saw, so a render is not an event.
    seen_seq: Option<u64>,
    last: Option<TrendReading>,
}

pub struct World {
    pub operators: Registry,
    inner: Mutex<Inner>,
    store: Store,
    /// ★★ The economy, live. Every committed call is metered against it and
    /// charged to it, and the serving seam issues for the work done.
    economy: Mutex<Economy>,
    /// ★★ Every `PawaReading` this host has taken. This is what makes the
    /// Console's *measured pawa* a measurement rather than an author's guess.
    meter: Mutex<Meter>,
    /// Definitions a person authored, checked by the engine before landing.
    definitions: Mutex<Vec<AuthoredDefinition>>,
    /// ★★★ The `W` series, per household. Without somewhere to live across
    /// calls there is no series at all — the distance to V was computed on
    /// every render and thrown away, so nothing could be smoothed and no drift
    /// could be detected. See [`World::observe`].
    monitors: Mutex<BTreeMap<String, Trend>>,
    /// ★★★ The unlocked identity, or `None`. **This is the authentication.**
    /// Every write path reads it; a locked world can decide nothing, because
    /// there is no principal to decide on behalf of.
    identity: Mutex<Option<Unlocked>>,
    /// Where the keypair lives on disk.
    identities: IdentityStore,
    /// ★ A monotonic proposal round, seeded from the clock at first use. See
    /// `propose_write` on why a retry must outbid its own previous attempt.
    round: AtomicU64,
    /// The package registry. ★ Its own store beside the logs, like the ingest
    /// queue: a package is not state, it is a thing that MIGHT become state
    /// once a gate says it may.
    arena: Arena,
    /// ★★★ **The network half.** Its own store handle and its own copy of the
    /// unlocked identity, because a listener runs on its own thread and must
    /// not hold the live world. Locking clears its identity too, so the node
    /// genuinely leaves the network rather than merely hiding the button.
    peering: Arc<Peering>,
    /// The capture queue. ★ Its own store, beside the logs — a message is not
    /// state, it is a thing that MIGHT become state once a person or a rule
    /// says what it means.
    ingest: Ingested,
}

pub struct Inner {
    sustains: BTreeMap<String, Sustain>,
    order: Vec<String>,
    selected: Option<String>,
}

impl World {
    /// Open the household from disk, folding every log.
    pub fn open(store: Store) -> StoreResult<World> {
        let store_root = store.root().to_path_buf();
        // ★★★ BEFORE anything is folded. A transfer a crash interrupted is
        //   finished here, so no state is ever computed from a log that is
        //   missing a leg its counterpart already has. Recovery runs first
        //   precisely because the fold would otherwise be correct about a
        //   household that was briefly wrong.
        // ★ Reported on stderr, never silently: a half-transfer that healed
        //   itself without telling anyone is still a household that was wrong.
        for id in store.recover_transfers()? {
            eprintln!("[mycelium] recovered an interrupted transfer: {id}");
        }

        let records = store.load_registry()?;
        let mut sustains = BTreeMap::new();
        let mut order = Vec::new();

        let authored = store.read_definitions()?;
        for record in &records.sustains {
            let definition = match &record.custom {
                Some(cid) => authored
                    .iter()
                    .find(|d| &d.id == cid)
                    .map(|d| d.to_definition())
                    .unwrap_or_else(|| templates::definition(record.template)),
                None => templates::definition(record.template),
            };
            let enforcement = enforcement_of(&definition);
            let (state, next_seq) = store.load_state(&record.id)?;
            order.push(record.id.clone());
            sustains.insert(
                record.id.clone(),
                Sustain { record: record.clone(), definition, enforcement, state, next_seq },
            );
        }

        let selected = records.selected.filter(|id| sustains.contains_key(id)).or(order.first().cloned());
        let definitions = authored;

        let peering = Arc::new(Peering::at(&store_root, store.clone()));
        let arena = Arena::at(&store_root);
        let round = AtomicU64::new(0);

        Ok(World {
            operators: Registry::default(),
            peering,
            arena,
            round,
            inner: Mutex::new(Inner { sustains, order, selected }),
            store,
            economy: Mutex::new(Economy::open(DEFAULT_HANDLE)),
            meter: Mutex::new(Meter::new()),
            definitions: Mutex::new(definitions),
            monitors: Mutex::new(BTreeMap::new()),
            identity: Mutex::new(None),
            identities: IdentityStore::at(&store_root),
            ingest: Ingested::at(&store_root)?,
        })
    }

    pub fn with_economy<T>(&self, f: impl FnOnce(&Economy) -> T) -> T {
        f(&self.economy.lock().expect("economy lock"))
    }

    pub fn with_meter<T>(&self, f: impl FnOnce(&Meter) -> T) -> T {
        f(&self.meter.lock().expect("meter lock"))
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    fn persist_registry(&self, inner: &Inner) -> StoreResult<()> {
        self.store.save_registry(&Records {
            sustains: inner.order.iter().filter_map(|id| inner.sustains.get(id)).map(|s| s.record.clone()).collect(),
            selected: inner.selected.clone(),
        })
    }

    // ── reading ──────────────────────────────────────────────────────────────

    /// Per-household trend engines. See [`observe`].
    #[cfg(test)]
    pub fn monitors_len(&self) -> usize {
        self.monitors.lock().expect("monitor lock").len()
    }

    pub fn with<T>(&self, f: impl FnOnce(&Inner) -> T) -> T {
        f(&self.inner.lock().expect("world lock"))
    }

    pub fn is_empty(&self) -> bool {
        self.with(|i| i.sustains.is_empty())
    }

    pub fn selected_id(&self) -> Option<String> {
        self.with(|i| i.selected.clone())
    }

    pub fn select(&self, id: &str) -> StoreResult<bool> {
        let mut inner = self.inner.lock().expect("world lock");
        if !inner.sustains.contains_key(id) {
            return Ok(false);
        }
        inner.selected = Some(id.to_string());
        self.persist_registry(&inner)?;
        drop(inner);
        // ★ A Sustain created after the listener started is describable
        //   without restarting anything.
        self.install_specs();
        self.install_bodies();
        Ok(true)
    }

    // ── creating ─────────────────────────────────────────────────────────────

    /// Instantiate a Sustain and write its **genesis** line.
    ///
    /// ★ The opening state is not stored as a state; it is the first entry of
    /// the log, so a Sustain's whole history — including how it began — folds
    /// from one place.
    pub fn instantiate(
        &self,
        id: &str,
        label: &str,
        template: TemplateId,
        parent: Option<&str>,
    ) -> StoreResult<bool> {
        self.instantiate_from(id, label, template, None, parent)
    }

    /// Instantiate from a built-in template or an AUTHORED definition.
    ///
    /// ★★ One path. An authored definition is opened, gated and logged exactly
    /// as a built-in is — nothing about being user-written makes it a
    /// second-class Sustain.
    pub fn instantiate_from(
        &self,
        id: &str,
        label: &str,
        template: TemplateId,
        custom: Option<&str>,
        parent: Option<&str>,
    ) -> StoreResult<bool> {
        self.instantiate_owned(id, label, template, custom, parent, None)
    }

    /// [`instantiate_from`], declaring who the new Sustain belongs to.
    pub fn instantiate_owned(
        &self,
        id: &str,
        label: &str,
        template: TemplateId,
        custom: Option<&str>,
        parent: Option<&str>,
        owner: Option<&str>,
    ) -> StoreResult<bool> {
        let mut inner = self.inner.lock().expect("world lock");
        if inner.sustains.contains_key(id) {
            return Ok(false);
        }
        if let Some(p) = parent {
            if !inner.sustains.contains_key(p) {
                return Err(StoreError::Io(format!("no such parent Sustain: {p}")));
            }
        }

        let (definition, state) = self
            .resolve_template(template, custom)
            .ok_or_else(|| StoreError::Io(format!("no such definition: {custom:?}")))?;
        let genesis = self.stamped(id, 0, LoggedEvent::genesis(&state));
        self.store.append(id, &genesis)?;

        let record = SustainRecord {
            id: id.to_string(),
            label: label.to_string(),
            template,
            custom: custom.map(str::to_string),
            parent: parent.map(str::to_string),
            owner: owner.map(str::to_string),
            owners: Vec::new(),
        };
        let enforcement = enforcement_of(&definition);

        inner.order.push(id.to_string());
        inner.sustains.insert(
            id.to_string(),
            Sustain { record, definition, enforcement, state, next_seq: 1 },
        );
        if inner.selected.is_none() {
            inner.selected = Some(id.to_string());
        }
        self.persist_registry(&inner)?;
        drop(inner);
        // ★ A Sustain created after the listener started is describable
        //   without restarting anything.
        self.install_specs();
        self.install_bodies();
        Ok(true)
    }

    /// ★★ Bonnie's household, once.
    ///
    /// Idempotent by the only check that means anything here: it does nothing
    /// at all if the world already holds a Sustain. A seed that ran on a
    /// populated store would either duplicate the household or silently edit
    /// it, and neither is something a person asked for.
    pub fn seed_if_empty(&self) -> StoreResult<bool> {
        if !self.is_empty() {
            return Ok(false);
        }
        // ★★★ Ownership is DECLARED at seed time, not inferred later. The
        //   person running this machine owns the household and their own
        //   habitat; the other five belong to members who have no identity here
        //   yet, and saying so is what makes the observer tier honest rather
        //   than an accident.
        let me = self.identities.handle().unwrap_or_else(|| DEFAULT_HANDLE.to_string());
        self.instantiate_owned("homestead", "Homestead", TemplateId::Homestead, None, None, Some(&me))?;
        for member in ["Bonnie", "Cira", "Epha", "Mum", "Kui", "Frankie"] {
            let id = format!("habitat-{}", member.to_lowercase());
            // Bonnie is the person at this keyboard; the rest are family whose
            // handles do not exist yet, so their habitats stay unclaimed.
            let owner = if member == "Bonnie" { Some(me.as_str()) } else { None };
            self.instantiate_owned(&id, member, TemplateId::Habitat, None, Some("homestead"), owner)?;
        }
        let mut inner = self.inner.lock().expect("world lock");
        inner.selected = Some("homestead".to_string());
        self.persist_registry(&inner)?;
        Ok(true)
    }

    // ── acting ───────────────────────────────────────────────────────────────

    /// ★★★ One real call, through the real gate, **durably**.
    ///
    /// The order is the whole point: the engine decides, and only a committed
    /// decision is appended to the log and reflected in the cache. A refusal
    /// writes nothing — which is the same thing the engine already guarantees,
    /// said twice, because the disk is the one place a mistake would outlive
    /// the process.
    pub fn call(
        &self,
        sustain_id: &str,
        operator: &str,
        params: &Map<String, Value>,
    ) -> StoreResult<Option<(Execution, u64)>> {
        // ★★★ AUTHENTICATION FIRST. A locked world has no principal, so there is
        //   nobody to decide on behalf of — and `Unchecked` is no longer an
        //   option this host can reach.
        let Some(principal) = self.principal() else {
            return Ok(Some((
                Execution {
                    result: OperatorResult::fail(
                        "the local identity is locked — unlock it before acting".to_string(),
                        "not_authenticated",
                    ),
                    mutations: vec![],
                    events: vec![],
                    state: Value::Null,
                },
                0,
            )));
        };
        // ★★★ **AGREEMENT BEFORE THE APPEND, for a shared Sustain only.**
        //
        // A Sustain with declared co-owners cannot tolerate a supersession —
        // that is what declaring them means — so the body agrees which write
        // takes the next slot BEFORE anything is applied here. A single-owner
        // Sustain skips this entirely: `owners_of` is empty, there is no round,
        // no socket and no reachability requirement.
        //
        // ★★ Deliberately placed BEFORE the gate rather than after. A write
        //    that lost its round was never this node's to make, so running the
        //    gate on it would meter pawa and evaluate invariants for a move
        //    that cannot land. The gate still runs on everything that wins.
        if !self.owners_of(sustain_id).is_empty() {
            match self.propose_write(sustain_id, operator, params) {
                Err(e) => {
                    return Ok(Some((
                        refusal(operator, &format!("consensus could not run: {e}"), "no_quorum"),
                        0,
                    )))
                }
                Ok(agreement) if !agreement.won() => {
                    let rule = agreement.rule().unwrap_or("no_quorum");
                    return Ok(Some((refusal(operator, &agreement.describe(), rule), 0)));
                }
                Ok(_) => {}
            }
        }

        let path = self.authority_path(sustain_id);
        let memberships = self.memberships_for(&principal);
        let skins = SkinRegistry::empty();

        let mut inner = self.inner.lock().expect("world lock");
        let Some(sustain) = inner.sustains.get(sustain_id) else {
            return Ok(None);
        };

        // ★★★ THE REAL ECONOMY, in the real gate.
        //
        // `Affordability::Metered` puts `balance >= pawa` into `admit` itself,
        // priced against the CANDIDATE's real cost — so an unaffordable run is
        // refused before it costs anything. `serving` opts into PAWA-8's
        // issuance seam, so this host earns `rate x pawa_served` for the work
        // it did, at the GOVERNED rate.
        let mut economy = self.economy.lock().expect("economy lock");
        let parameters = economy.parameters();
        let issuance = economy.issuance.clone();
        let x = {
            let mut afford = Affordability::Metered {
                ledger: &mut economy.ledger,
                parameters: &parameters,
                principal: &principal,
                sustain: sustain_id,
                at: 0,
                serving: Some((&issuance, &principal)),
            };
            execute_afforded(
                &self.operators,
                &sustain.definition.operators,
                &sustain.enforcement,
                &sustain.state,
                operator,
                params,
                // ★★★ The real conjunct. `permitted(α, o, Σ)` runs BEFORE the
                //   guard, and its refusal is its own class — "you may not" is
                //   not "that would break a rule".
                &Authorization::Principal {
                    id: &principal,
                    memberships: &memberships,
                    path: &path,
                    skins: &skins,
                },
                &EffectClass::Unchecked,
                &mut NonceLedger::new(),
                &mut afford,
            )
        };
        drop(economy);

        // ★★ The meter reading, recorded. `meter` returns `None` for anything
        //    uncommitted, so a refused run can never contribute a measurement —
        //    which is why the Console's figures are honest.
        if let Some(m) = self.operators.get(operator) {
            if let Some(reading) =
                meter(&x, m, &sustain.enforcement, &parameters, sustain_id, &principal, 0)
            {
                self.meter.lock().expect("meter lock").record(reading);
            }
        }

        // ★ The seq a REFUSAL would have taken is never consumed: nothing is
        //   appended, so the next committed call takes it. A log with holes in
        //   it would imply events that were never written.
        let seq = sustain.next_seq;
        if x.committed() {
            let line = self.stamped(sustain_id, seq, LoggedEvent::of(seq, operator, &x));
            self.store.append(sustain_id, &line)?;
            let sustain = inner.sustains.get_mut(sustain_id).expect("checked above");
            sustain.state = x.state.clone();
            sustain.next_seq = seq + 1;
        }
        Ok(Some((x, seq)))
    }

    /// ★★★ Ask the engine whether `V` holds, right now.
    ///
    /// Each declared invariant goes through `predicate::check` — the **same
    /// evaluator the gate uses**. The host does not judge a rule and does not
    /// re-implement one; it asks and reports the answer, reason text included.
    ///
    /// ★ A rule whose expression will not parse is reported as **not holding**,
    /// with the syntax error as its reason. Treating an unparseable rule as
    /// satisfied would be the one failure mode a viable region must not have.
    pub fn constraints(&self, sustain_id: &str) -> Vec<(String, String, bool, String)> {
        let empty = Map::new();
        self.with(|i| {
            let Some(s) = i.get(sustain_id) else { return Vec::new() };
            s.definition
                .invariants
                .iter()
                .map(|(id, expr)| match check(expr, &s.state, &empty) {
                    Ok((holds, reason)) => (id.clone(), expr.clone(), holds, reason),
                    Err(e) => (id.clone(), expr.clone(), false, format!("unparseable: {e:?}")),
                })
                .collect()
        })
    }

    /// The persisted log for one Sustain, newest last.
    pub fn log(&self, sustain_id: &str) -> StoreResult<Vec<LoggedEvent>> {
        self.store.read_log(sustain_id)
    }

    // ── simulation: a fork through the REAL gate ─────────────────────────────

    /// ★★★ Run a sequence of operators against a **copy** of live state.
    ///
    /// STEP-0 found no scenario-fork API in `sustena-core` — `ensemble::Scenario`
    /// is about model ensembles (`M_world`), not Sustain simulation. So a fork
    /// here is exactly two things and nothing more: **`state.clone()`** and the
    /// **same `execute_admitted`** every real call goes through.
    ///
    /// ★★ That makes it the engine's own gate on a copy rather than a simulator
    /// this app invented — a step that would be refused for real is refused
    /// here, with the same reason text.
    ///
    /// ★★ Nothing is written. It does not touch the log, the ledger, the meter
    /// or the cached state, and it takes `&self` with no mutation path to any of
    /// them. A branch is a value, not an effect.
    ///
    /// ★ It is deliberately **unmetered**: charging juul for a hypothetical
    /// would make thinking cost money, and the reading would be a measurement
    /// of work that never happened.
    pub fn fork(
        &self,
        sustain_id: &str,
        steps: &[(String, Map<String, Value>)],
    ) -> Option<Vec<(String, Execution)>> {
        let (definition, enforcement, mut state) = self.with(|i| {
            let s = i.get(sustain_id)?;
            Some((s.definition.clone(), s.enforcement.clone(), s.state.clone()))
        })?;

        let mut out = Vec::new();
        for (operator, params) in steps {
            let x = execute_admitted(
                &self.operators,
                &definition.operators,
                &enforcement,
                &state,
                operator,
                params,
                &Authorization::Unchecked,
                &EffectClass::Unchecked,
                &mut NonceLedger::new(),
            );
            // ★ A refused step does not advance the fork, exactly as it would
            //   not advance reality. The branch stops being useful past it, and
            //   the caller can see why.
            if x.committed() {
                state = x.state.clone();
            }
            let stop = !x.committed();
            out.push((operator.clone(), x));
            if stop {
                break;
            }
        }
        Some(out)
    }

    // ── governance: the only path to a parameter ─────────────────────────────

    /// ★★★ Change a governed parameter — **through the gate**, like anything else.
    ///
    /// `governance.set_parameter` is an ordinary Enzyme on an ordinary
    /// `Definition`, so the bounds are ordinary invariants and an out-of-range
    /// change is refused with `enforcement_gate`. There is no setter anywhere
    /// that bypasses this.
    ///
    /// ★ It runs `Unchecked`/`Unchecked` like every other call in this host, so
    /// the approval-token clause is vacuous here. Said plainly: the *bounds* are
    /// enforced, the *authority* is not yet.
    pub fn set_parameter(&self, name: &str, value: f64) -> Execution {
        use sustena_core::governance;

        let d = governance::definition(&governance::declared_parameters());
        let mut reg = Registry::default();
        governance::register(&mut reg);

        let mut params = Map::new();
        params.insert("name".into(), Value::String(name.to_string()));
        params.insert(
            "value".into(),
            serde_json::Number::from_f64(value).map(Value::Number).unwrap_or(Value::Null),
        );

        let mut economy = self.economy.lock().expect("economy lock");
        let x = execute_admitted(
            &reg,
            &d.operators,
            &governance::enforcement(&d),
            &economy.governance_state,
            "governance.set_parameter",
            &params,
            &Authorization::Unchecked,
            &EffectClass::Unchecked,
            &mut NonceLedger::new(),
        );
        if x.committed() {
            economy.governance_state = x.state.clone();
        }
        x
    }

    // ── the composition, checked by the core ─────────────────────────────────

    /// ★★★ Validate `⊕` with the engine, not with host bookkeeping.
    ///
    /// `MonitorEngine::flatten_holarchy` refuses a duplicate id, a parent that
    /// was never declared, and a **cycle** — so the parent/child links the
    /// registry holds are checked by the same code the attention scan walks,
    /// rather than trusted because the host wrote them.
    ///
    /// ★ Returns the error text rather than a bool: a tree that does not hold
    /// should say which link broke it.
    pub fn check_holarchy(&self) -> Result<usize, String> {
        let watches: Vec<SustainWatch> = self.with(|i| {
            i.order
                .iter()
                .filter_map(|id| i.sustains.get(id))
                .map(|s| {
                    // ★ An empty `Region` and a nominal detector: this call is
                    //   asked ONLY about the shape of the tree. Real monitoring
                    //   thresholds belong to the Monitor screen (V1.3), and
                    //   inventing them here to make a structural check compile
                    //   would be declaring rules nobody chose.
                    let w = SustainWatch::new(
                        &s.record.id,
                        Region::new(),
                        0.3,
                        CusumSpec::new(0.0, 1.0, 5.0),
                    );
                    match &s.record.parent {
                        Some(p) => w.under(p),
                        None => w,
                    }
                })
                .collect()
        });

        if watches.is_empty() {
            return Ok(0);
        }
        MonitorEngine::flatten_holarchy(watches)
            .map(|e| e.sustains().len())
            .map_err(|e| format!("{e:?}"))
    }

    // ── ingest ──────────────────────────────────────────────

    pub fn ingest(&self) -> &Ingested {
        &self.ingest
    }

    /// **Capture one message**, and apply it if `τ` mapped it unambiguously.
    ///
    /// ★★★ A mapped operator goes through `World::call` — the SAME path the
    /// Console uses. It is authorised as the unlocked principal, priced, gated,
    /// logged and pushed. There is no ingest-specific write path, so a captured
    /// message cannot reach state by a route a person could not have taken.
    ///
    /// ★★ A rejection never reaches the store. See [`Ingested::capture`].
    pub fn capture(
        &self,
        sustain_id: &str,
        source_id: &str,
        raw: &str,
    ) -> StoreResult<Capture> {
        self.capture_at(sustain_id, source_id, raw, None)
    }

    /// Capture, carrying when the phone says the message arrived.
    pub fn capture_at(
        &self,
        sustain_id: &str,
        source_id: &str,
        raw: &str,
        sent_at_ms: Option<i64>,
    ) -> StoreResult<Capture> {
        let rules = self.ingest.effective_rules()?;
        let captured = self.ingest.capture_at(sustain_id, source_id, raw, &rules, sent_at_ms)?;

        // Only a freshly-stored, unambiguously-mapped message applies. A
        // duplicate has already had its chance; anything else is a person's.
        let Capture::Stored(m) = &captured else { return Ok(captured) };
        let (Some(operator), true) = (m.operator.clone(), m.status == "mapped") else {
            return Ok(captured);
        };

        let mut params: Map<String, Value> = m.params.clone().into_iter().collect();

        // ★★★ The message that caused it, carried into the entry it creates.
        //
        // Money arriving from his own other account reads exactly like money
        // arriving from an employer, so it is filed as income here, before
        // anything can reveal which it was. When the other half turns up and
        // says it was his own money moving, that income has to come back off
        // the books -- and taking the right entry out of a list needs the entry
        // to be identifiable. Without this the only options are to remove the
        // wrong one or to leave a false income standing, and both are worse
        // than the cost of one extra field.
        if operator == "budget.record_income" {
            params.entry("entry_id".to_string()).or_insert_with(|| Value::String(m.id.clone()));
        }
        // ★★ Which account it landed in, from the source of the text itself.
        //    Free attribution, and the same fact the transfer pass needs.
        if !params.contains_key("account") {
            let takes_account = self
                .operators
                .get(&operator)
                .is_some_and(|meta| meta.params.iter().any(|p| p.name == "account"));
            if takes_account {
                params.insert("account".to_string(), Value::String(source_id.to_string()));
            }
        }

        match self.call(sustain_id, &operator, &params)? {
            Some((x, _)) => {
                let reason = x.result.reason.clone();
                self.ingest.record_outcome(&m.id, x.committed(), reason)?;
            }
            None => {
                self.ingest.record_outcome(
                    &m.id,
                    false,
                    Some(format!("no Sustain called '{sustain_id}'")),
                )?;
            }
        }
        // Re-read, so the caller sees what the gate decided.
        let updated = self
            .ingest
            .current()?
            .into_iter()
            .find(|x| x.id == m.id)
            .map(Box::new)
            .unwrap_or_else(|| m.clone());
        Ok(Capture::Stored(updated))
    }

    /// Fold one reading into this household's trend, and say what it means.
    ///
    /// ★★★ The piece that was missing. `compose(r)` computed the distance to V
    /// on every call and then forgot it, so there was no series — nothing to
    /// smooth, and no drift to detect. Monitor §V and §VI both need a history
    /// and neither had one.
    ///
    /// ★★ The engine lives here rather than in the feed because the feed is
    /// rebuilt constantly and a series that resets on every render is not a
    /// series. It is per-household, keyed by sustain, and holds only what the
    /// detectors need.
    ///
    /// ★ In memory only, deliberately for now. A trend rebuilt from an empty
    /// history after a restart understates drift rather than inventing it,
    /// which is the safe direction to be wrong in; persisting the series is a
    /// real follow-up and is called out rather than quietly assumed done.
    pub fn observe(&self, sustain_id: &str, reading: &Value) -> Option<TrendReading> {
        // ★★★ One reading per EVENT, not per render.
        //
        // Found by watching the series: the feed is rebuilt on every poll, so
        // observing per call fed the detectors a fresh reading when nothing had
        // happened at all. A household sitting still then "drifts" purely
        // because it was looked at often, and one bad afternoon reads as a
        // trend for as long as someone keeps the app open. The sustain's own
        // sequence number is the honest tick: it moves when something really
        // changed and not otherwise.
        let seq = self.with(|i| i.get(sustain_id).map(|s| s.next_seq))?;
        let mut monitors = self.monitors.lock().expect("monitor lock");
        let entry = monitors.entry(sustain_id.to_string()).or_insert_with(|| {
            let watch = crate::orchie::watch_for(sustain_id, reading);
            Trend {
                engine: MonitorEngine::watching(watch).ok(),
                seen_seq: None,
                last: None,
            }
        });
        if entry.seen_seq == Some(seq) {
            // Nothing new happened; report what the last real reading said.
            return entry.last.clone();
        }
        entry.seen_seq = Some(seq);
        let out = entry.engine.as_mut()?.ingest(sustain_id, reading).ok();
        entry.last = out.clone();
        out
    }

    /// The id this household's capture device goes by.
    ///
    /// ★ Derived from the household rather than random, so the same phone
    /// reporting twice is the same Sustain both times rather than a new one
    /// every sweep.
    pub fn device_id(household: &str) -> String {
        format!("device-{household}")
    }

    /// Record that the phone is alive and how far behind it is, creating the
    /// device Sustain the first time.
    ///
    /// ★★★ §IX in one call. The phone is not a special case wired into the
    /// ingest path; it is a child Sustain whose state changes through the same
    /// gate as any other, so everything already built for children — roll-up,
    /// composition, the feed — reads it for free.
    ///
    /// ★★ Best-effort by design. A heartbeat that failed must never take a
    /// real capture down with it: the texts are the point, and knowing how the
    /// phone felt about delivering them is not worth losing one.
    pub fn heartbeat(&self, household: &str, queue_depth: u32, at_ms: i64) -> StoreResult<()> {
        let id = Self::device_id(household);
        if self.with(|i| i.get(&id).is_none()) {
            // ★★★ Owned by whoever owns the household it reports to. A device
            //     created ownerless leaves its own household's principal with
            //     viewer rights on it, and every heartbeat is then refused for
            //     insufficient privilege -- silently, since a heartbeat must
            //     never take a real capture down with it. Found by testing.
            // Whoever owns the household, or failing that whoever is holding
            // the phone -- which is the honest answer for a device anyway.
            let owner = self
                .with(|i| i.get(household).and_then(|s| s.record.owner.clone()))
                .or_else(|| self.principal());
            self.instantiate_owned(
                &id,
                "this phone",
                TemplateId::Device,
                None,
                Some(household),
                owner.as_deref(),
            )?;
        }
        let mut params = Map::new();
        params.insert("queue_depth".into(), serde_json::json!(queue_depth));
        params.insert("at_ms".into(), serde_json::json!(at_ms));
        params.insert("app_version".into(), Value::String(env!("CARGO_PKG_VERSION").to_string()));
        let _ = self.call(&id, "device.heartbeat", &params)?;
        Ok(())
    }

    /// The rules in force for a source: the shipped set plus this household's
    /// own corrections. ★ A correction is tried FIRST — a person's answer wins
    /// over the shape it corrects.
    pub fn rules_for(&self, source: &str) -> StoreResult<Vec<ParseRule>> {
        let mut out: Vec<ParseRule> = self
            .ingest
            .effective_rules()?
            .into_iter()
            .filter(|r| r.source.eq_ignore_ascii_case(source))
            .collect();
        out.extend(sustena_core::seed_rules(source).iter().cloned());
        Ok(out)
    }

    // ── identity ─────────────────────────────────────────

    pub fn identity_store(&self) -> &IdentityStore {
        &self.identities
    }

    /// The authenticated principal, or `None` while locked.
    ///
    /// ★★★ There is no fallback to a declared string. A caller that wants an
    /// id when the world is locked does not get one — which is what makes
    /// "nothing acts until the key is unlocked" a property rather than a habit.
    pub fn principal(&self) -> Option<String> {
        self.identity.lock().expect("identity lock").as_ref().map(|u| u.handle().to_string())
    }

    pub fn is_unlocked(&self) -> bool {
        self.identity.lock().expect("identity lock").is_some()
    }

    /// The public half, for a screen that wants to show WHICH key is acting.
    pub fn public_key(&self) -> Option<String> {
        self.identity.lock().expect("identity lock").as_ref().map(|u| u.public_key())
    }

    /// Unlock the local identity. The passphrase is checked by the only thing
    /// that can check it — whether it decrypts the private key.
    pub fn unlock(&self, passphrase: &str) -> Result<String, IdentityError> {
        let u = self.identities.unlock(passphrase)?;
        let handle = u.handle().to_string();
        // ★ The peering gets its own copy: a session must be able to prove this
        //   node's key without reaching into the world.
        self.peering.set_identity(Some(u.clone()));
        *self.identity.lock().expect("identity lock") = Some(u);
        // ★ A pre-ownership store claims itself once, out loud.
        if let Ok(claimed) = self.claim_unowned_for(&handle) {
            for id in claimed {
                eprintln!("[mycelium] {handle} claimed ownership of {id} (seeded before ownership was declared)");
            }
        }
        Ok(handle)
    }

    /// Mint an identity on a machine that has none.
    pub fn enrol(&self, handle: &str, passphrase: &str) -> Result<String, IdentityError> {
        let u = self.identities.enrol(handle, passphrase)?;
        let handle = u.handle().to_string();
        self.peering.set_identity(Some(u.clone()));
        *self.identity.lock().expect("identity lock") = Some(u);
        Ok(handle)
    }

    /// Drop the private key. ★ Not a UI state — the key genuinely leaves memory,
    /// so a locked cockpit cannot act even if a surface forgot to stop it.
    pub fn lock(&self) {
        // ★★★ Both, and in this order. A listener still holding a key after
        //   the screen said locked would be a lie with a socket attached.
        self.peering.set_identity(None);
        *self.identity.lock().expect("identity lock") = None;
    }

    // ── peering ─────────────────────────────────────────────────────────

    pub fn peering(&self) -> &Arc<Peering> {
        &self.peering
    }

    // ── quorum ───────────────────────────────────────────────────────

    /// Declare a Sustain co-owned by a set of node keys.
    ///
    /// ★★ The declaring node is always included — a body you are not in is
    /// not a body you can write to, and silently excluding yourself would
    /// produce a Sustain this node could never change again.
    pub fn share_ownership(&self, sustain_id: &str, with: &[String]) -> Result<Vec<String>, String> {
        let me = self.node_id().ok_or_else(|| "no identity on this machine".to_string())?;
        let mut owners: BTreeSet<String> = with.iter().cloned().collect();
        owners.insert(me);
        let owners: Vec<String> = owners.into_iter().collect();

        let mut inner = self.inner.lock().expect("world lock");
        let Some(su) = inner.sustains.get_mut(sustain_id) else {
            return Err(format!("no such Sustain: {sustain_id}"));
        };
        su.record.owners = owners.clone();
        self.persist_registry(&inner).map_err(|e| e.to_string())?;
        drop(inner);
        self.install_bodies();
        Ok(owners)
    }

    /// Push the co-ownership map to the peering, so the listener can vote.
    fn install_bodies(&self) {
        let bodies = self.with(|i| {
            i.order()
                .iter()
                .filter_map(|id| {
                    let s = i.get(id)?;
                    if s.record.owners.is_empty() {
                        return None;
                    }
                    Some((id.clone(), s.record.owners.clone()))
                })
                .collect()
        });
        self.peering.set_bodies(bodies);
    }

    /// The highest slot this node has accepted anything for.
    ///
    /// ★ `None` means **nothing has been agreed**, which is not slot zero —
    /// slot zero is a real position a real write can hold.
    pub fn last_agreed(&self, sustain_id: &str) -> Option<u64> {
        let mut highest = None;
        for slot in 0..64u64 {
            if self.peering.acceptors().accepted(&Slot::new(sustain_id, slot)).is_some() {
                highest = Some(slot);
            }
        }
        highest
    }

    /// The co-owners of a Sustain, or empty for a single-owner one.
    pub fn owners_of(&self, sustain_id: &str) -> Vec<String> {
        self.with(|i| i.get(sustain_id).map(|s| s.record.owners.clone()).unwrap_or_default())
    }

    /// ★★★ **Agree on who gets this slot, before anything is written.**
    ///
    /// §V's two phases, driven over slice 6's real transport: this node's own
    /// acceptor answers in-process, every other co-owner answers over a
    /// socket. The adoption rule (*highest-numbered accepted value seen, else
    /// own*) is `consensus::adopt` — the **same function** the in-process
    /// `propose` uses, so the distributed proposer cannot drift from the
    /// simulation on the one line safety rests on.
    ///
    /// ★★ An unreachable co-owner is simply **not a grant**. It is not an
    /// error to be propagated and not a retry to be hidden: it is one fewer
    /// vote, which is what makes a minority unable to write.
    pub fn propose_write(
        &self,
        sustain_id: &str,
        operator: &str,
        params: &Map<String, Value>,
    ) -> Result<Agreement, String> {
        let me = self
            .identity
            .lock()
            .expect("identity lock")
            .clone()
            .ok_or_else(|| "this node is locked".to_string())?;
        let my_key = me.public_key();
        let owners = self.owners_of(sustain_id);

        let body = Body::majority(owners.clone()).map_err(|e| e.to_string())?;
        let quorum = body.quorum_size();

        // ★★★ **The slot is the LOG's next position, not this node's next
        //     sequence.** Found by the two-node test: `next_seq` is a
        //     PER-NODE counter (slice 6 made it so deliberately, precisely
        //     because two nodes must not share a counter), so Alice proposed
        //     slot 1 while Bob proposed slot 0 — two different Paxos instances,
        //     and both won. A body has to be agreeing about the same position
        //     or it is not agreeing about anything.
        //
        // ★★ The log's length is a genuinely shared quantity: both nodes
        //    holding the same entries see the same number. A node that is
        //    BEHIND proposes a lower slot, which has already been decided —
        //    and PREPARE hands it the accepted value, so it adopts, loses, and
        //    is told to retry. That is recovery, not a special case.
        let slot = self.store.read_replica(sustain_id, &my_key).map_err(|e| e.to_string())?.len()
            as u64;
        let mine = Write {
            operator: operator.to_string(),
            params: params.clone(),
            by: my_key.clone(),
        };
        // ★★ Monotonic per node, seeded from the clock. A retry after being
        //    outbid must mint a HIGHER number than its own last attempt — two
        //    attempts inside one second would otherwise tie, and the retry
        //    would lose to the same competitor forever.
        let round = self
            .round
            .fetch_update(SeqCst, SeqCst, |prev| Some(now_secs().max(prev + 1)))
            .map(|prev| now_secs().max(prev + 1))
            .unwrap_or_else(|_| now_secs());
        let number = ProposalNumber::new(round, &my_key);
        let slot_key = Slot::new(sustain_id, slot);

        // ── PREPARE ─────────────────────────────────────────────────────────
        let book = self.peering.book();
        let mut promises: Vec<Promise> = Vec::new();
        let mut unreachable: Vec<String> = Vec::new();

        // This node's own acceptor, in-process. ★ It votes like any other: a
        // proposer that exempted itself would be a body of one wearing a
        // quorum's name.
        promises.push(
            self.peering.acceptors().on_prepare(&slot_key, &number)?,
        );

        let mut reachable: Vec<(String, String)> = Vec::new();
        for owner in owners.iter().filter(|o| **o != my_key) {
            match book.get(owner).and_then(|p| p.address.clone()) {
                Some(address) => reachable.push((owner.clone(), address)),
                None => unreachable.push(owner.clone()),
            }
        }

        let mut accepts: Vec<(String, String, Accepted)> = Vec::new();
        let mut round_promises: Vec<(String, String, Promise)> = Vec::new();
        for (owner, address) in reachable {
            // ★ Both phases in one connection: see `Peering::round_with` on
            //   why a held session would hide a co-owner going away.
            match self.peering.round_with(
                &address,
                &me,
                sustain_id,
                slot,
                &number,
                &mine.value(),
            ) {
                Ok((promise, accepted)) => {
                    round_promises.push((owner.clone(), address.clone(), promise));
                    if let Some(a) = accepted {
                        accepts.push((owner, address, a));
                    }
                }
                Err(_) => unreachable.push(owner),
            }
        }
        promises.extend(round_promises.iter().map(|(_, _, p)| p.clone()));

        if granted(&promises) < quorum {
            // ★★★ **Refused is not absent.** If anyone answered with a higher
            //     promise, this round was OUTBID — a live competitor, not an
            //     offline family. Collapsing the two produced the
            //     self-contradicting *"0 of 2 answered · unreachable: none"*.
            let outbid = promises.iter().find_map(|p| match p {
                Promise::Refused { promised } => Some(promised.clone()),
                Promise::Granted { .. } => None,
            });
            return Ok(match outbid {
                Some(by) => Agreement::Outbid { slot, by },
                None => Agreement::NoQuorum {
                    slot,
                    reached: granted(&promises),
                    needed: quorum,
                    unreachable,
                },
            });
        }

        // ── the adoption rule, shared with the in-process proposer ──────────
        let value = adopt(&promises, &mine.value());

        // ── ACCEPT ──────────────────────────────────────────────────────────
        // ★ This node's own acceptor takes the ADOPTED value, which may not be
        //   the one it proposed — that is the rule working, not a bug.
        let mut votes: Vec<Accepted> =
            vec![self.peering.acceptors().on_accept(&slot_key, &number, &value)?];
        votes.extend(accepts.iter().map(|(_, _, a)| a.clone()));

        if accepted_count(&votes) < quorum {
            let outbid = votes.iter().find_map(|a| match a {
                Accepted::Refused { promised } => Some(promised.clone()),
                Accepted::Ok => None,
            });
            return Ok(match outbid {
                Some(by) => Agreement::Outbid { slot, by },
                None => Agreement::NoQuorum {
                    slot,
                    reached: accepted_count(&votes),
                    needed: quorum,
                    unreachable,
                },
            });
        }

        // ── chosen ──────────────────────────────────────────────────────────
        let chosen = Write::from_value(&value)
            .ok_or_else(|| "the body agreed on something that is not a write".to_string())?;
        if chosen.by == my_key && chosen.operator == mine.operator {
            Ok(Agreement::Won { number, slot })
        } else {
            // ★★ Someone else's write holds this slot. Nothing was applied
            //    here; the caller retries against the state that write leaves.
            Ok(Agreement::Lost { slot, to: chosen.by.clone(), chose: Box::new(chosen) })
        }
    }

    // ── the arena ──────────────────────────────────────────────────────

    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    /// Push this node's catalogue to the peering, so a peer can browse it.
    ///
    /// ★★ Only packages whose bytes are **intact** are offered. A record that
    /// no longer matches its own hash is this node's problem to notice, not
    /// something to hand to somebody else and let them discover.
    /// Re-read the catalogue this node offers.
    ///
    /// ★ Public because the arena can change under the world — a package
    /// edited on disk, or recorded through `arena()` directly — and a stale
    /// offer list would advertise something this node no longer has. `publish`
    /// and `fetch_package` call it for you; this is for everything else.
    pub fn refresh_offers(&self) {
        self.install_offers();
    }

    fn install_offers(&self) {
        let offers = self
            .arena
            .all()
            .into_iter()
            .filter(|p| p.provenance().integrity == Integrity::Intact)
            .filter_map(|p| {
                let value = serde_json::to_value(&p).ok()?;
                Some((
                    PackageOffer {
                        id: p.id.clone(),
                        name: p.name.clone(),
                        kind: p.kind.label().to_string(),
                        version: p.version.clone(),
                        description: p.description.clone(),
                        author: p.author.clone(),
                        author_handle: p.author_handle.clone(),
                        content_hash: p.content_hash.clone(),
                        signed: p.signature.is_some(),
                    },
                    value,
                ))
            })
            .collect();
        self.peering.set_offers(offers);
    }

    /// ★★★ **The trust reading for one package, out of signals this node
    ///     actually holds.** Nothing here is invented: every signal is
    ///     something checkable on this machine right now.
    ///
    /// ★★ `peer_holdings` is *what your peers offered when you last looked* —
    /// pass an empty slice if you have not looked, and the reading will say
    /// *held by 0 of the 0 peers you asked*, which is honestly different from
    /// *nobody has it*.
    pub fn trust_in(&self, package: &Package, peer_holdings: &[(String, Vec<String>)]) -> PackageTrust {
        let mut signals = Vec::new();
        let me = self.node_id().unwrap_or_default();

        match package.provenance().authenticity {
            Authenticity::Signed => signals.push(TrustSignal::Signed),
            Authenticity::Unsigned => signals.push(TrustSignal::Unsigned),
            Authenticity::Forged => signals.push(TrustSignal::Forged),
        }

        if package.author == me {
            signals.push(TrustSignal::AuthoredHere);
        } else {
            let book = self.peering.book();
            match book.get(&package.author) {
                // ★ A peer you TRUSTED. Merely having met them is not a
                //   recommendation, so `Pending` and `Blocked` do not count.
                Some(p) if p.standing == Standing::Trusted => {
                    signals.push(TrustSignal::AuthorIsATrustedPeer { handle: p.handle.clone() })
                }
                _ => signals.push(TrustSignal::AuthorUnknown),
            }
        }

        if let Origin::FromPeer { peer } = &package.origin {
            signals.push(TrustSignal::ImportedFrom { peer: short_key(peer) });
        }

        if !peer_holdings.is_empty() {
            let count = peer_holdings
                .iter()
                .filter(|(_, hashes)| hashes.contains(&package.content_hash))
                .count();
            signals.push(TrustSignal::HeldByPeers { count, of: peer_holdings.len() });
        }

        if self.arena.installs().iter().any(|i| i.package_id == package.id) {
            signals.push(TrustSignal::InstalledHere);
        }

        PackageTrust::from_signals(signals)
    }

    /// Acquire a package: settle its royalty in **juul** and record the order.
    ///
    /// ★★★ **An order moves internal credit and nothing else.** `settle` only
    /// ever calls `JuulLedger::transfer`, so circulation is unchanged by
    /// construction — nothing is minted, nothing leaves this host, and no part
    /// of this touches money. ADR-0001 D5, and the panel says it too.
    ///
    /// ★★ Ordering is **not** installing. A package you have paid for still
    /// faces the whole gate, and one you have not paid for is not blocked from
    /// it — the royalty is a contribution, not a licence check.
    pub fn place_order(&self, package_id: &str, on: u64) -> Result<Order, String> {
        let Some(package) = self.arena.get(package_id) else {
            return Err(format!("no such package: {package_id}"));
        };
        let by = self.principal().ok_or_else(|| "this node is locked".to_string())?;
        let settlement = self.pay_royalty(package_id, on)?;

        let (paid, shares) = match &settlement {
            Settlement::Settled { shares, .. } => (
                settlement.transferred(),
                shares
                    .iter()
                    .map(|x| (x.role.name().to_string(), x.recipient.clone(), x.amount))
                    .collect(),
            ),
            // ★ A free package orders at zero, and that is a real order rather
            //   than a refusal: it records that you took it.
            Settlement::NoRoyalty => (0, Vec::new()),
            Settlement::Insufficient { required, balance } => {
                return Err(format!(
                    "the royalty is {required} juul and this node holds {balance:.0} —                      nothing moved, and nothing was ordered"
                ))
            }
        };

        let order = Order {
            reference: format!("SXI-{}", package.content_hash[..6].to_uppercase()),
            package_id: package.id.clone(),
            package_name: package.name.clone(),
            by,
            paid,
            shares,
            per_mille: package.per_mille,
            placed_at: now_secs(),
        };
        self.arena.record_order(order.clone())?;
        Ok(order)
    }

    /// What a peer is offering. ★ A listing; nothing is fetched or installed.
    pub fn peer_offers(&self, address: &str) -> Result<(String, Vec<PackageOffer>), String> {
        let me = self
            .identity
            .lock()
            .expect("identity lock")
            .clone()
            .ok_or_else(|| "this node is locked".to_string())?;
        self.peering.offers_from(address, &me)
    }

    /// Fetch one package from a peer and record it locally.
    ///
    /// ★★★ **This does NOT install it, and that separation is the point.**
    /// Arriving is not installing; a fetched package lands in the local arena
    /// exactly like a locally-published one and then faces
    /// [`World::install`] — **the same function, unmodified**. There is no
    /// wire-specific install path, so `Admitted`'s no-public-constructor
    /// property holds for a peer's package identically.
    ///
    /// ★★★ **Two different tampering questions, both asked.** The session's
    /// AEAD already proves nothing changed **in flight** — that is increment 2
    /// and it is not re-litigated here. This checks the other one: that the
    /// bytes the sender actually held hash to what was asked for. A package
    /// mangled at rest on the sender's disk passes the AEAD perfectly and is
    /// caught here.
    pub fn fetch_package(&self, address: &str, content_hash: &str) -> Result<Package, String> {
        let me = self
            .identity
            .lock()
            .expect("identity lock")
            .clone()
            .ok_or_else(|| "this node is locked".to_string())?;
        let (peer, value) = self.peering.fetch_from(address, &me, content_hash)?;

        let mut package: Package = serde_json::from_value(value)
            .map_err(|e| format!("that is not a package: {e}"))?;

        // ★★★ Hashed here, from the bytes that arrived — never trusted from
        //     the record, which is the field an attacker would edit.
        let actual = content_hash_of(&package.spec);
        if actual != content_hash {
            return Err(format!(
                "refused on arrival: asked for {content_hash}, the bytes hash to {actual} —                  this is not the package that was requested"
            ));
        }
        if package.content_hash != actual {
            return Err(format!(
                "refused on arrival: the package claims hash {} but its bytes hash to {actual}",
                package.content_hash
            ));
        }

        // ★★ The origin is rewritten to what it actually is. A package cannot
        //    arrive claiming to have been written here — `MemeProvenance`'s rule
        //    (*an import can never read as native*), enforced at the boundary.
        package.origin = Origin::FromPeer { peer };

        // ★ Authenticity is checked by `provenance()` on every read, so a
        //   forged signature does not need catching twice — but a package that
        //   arrives already forged is worth refusing at the door rather than
        //   storing and refusing later.
        if package.provenance().authenticity == Authenticity::Forged {
            return Err(format!(
                "refused on arrival: {} does not verify against the author key it names",
                package.name
            ));
        }

        self.arena.record(package.clone())?;
        self.install_offers();
        Ok(package)
    }

    /// Every juul this host has, across every balance.
    ///
    /// ★★★ The number a royalty must not change. `settle` only transfers,
    /// so this is how *nothing was minted* is checked rather than asserted.
    pub fn circulation(&self) -> f64 {
        self.economy.lock().expect("economy lock").ledger.total_in_circulation()
    }

    /// Live instances of one authored definition — what `safe` needs.
    fn instances_of(&self, definition_id: &str) -> Vec<Instance> {
        self.with(|i| {
            i.order()
                .iter()
                .filter_map(|id| i.get(id))
                .filter(|s| s.record.custom.as_deref() == Some(definition_id))
                .map(|s| Instance { id: s.record.id.clone(), state: s.state.clone() })
                .collect()
        })
    }

    /// ★★★ **The gate, for whatever kind this is.** One function, so there is
    /// exactly one answer to *may this be installed* and no per-call-site
    /// variant of it. `into` is the target Sustain a widget would join —
    /// meaningless for a definition, and required for a widget, because
    /// `inputs ⊆ dim(S)` is a question about a household.
    pub fn judge(&self, package: &Package, into: Option<&str>) -> InstallVerdict {
        match package.kind {
            Kind::Definition => {
                let authored: AuthoredDefinition = match serde_json::from_value(package.spec.clone())
                {
                    Ok(a) => a,
                    Err(e) => {
                        return InstallVerdict::Refused {
                            rule: "decodable",
                            errors: vec![format!("this is not a definition: {e}")],
                        }
                    }
                };
                let instances = self.instances_of(&authored.id);
                definition_installs(&authored.id, &authored.to_definition(), &instances)
            }
            Kind::Widget => {
                let authored: AuthoredWidget = match serde_json::from_value(package.spec.clone()) {
                    Ok(a) => a,
                    Err(e) => {
                        return InstallVerdict::Refused {
                            rule: "decodable",
                            errors: vec![format!("this is not a widget: {e}")],
                        }
                    }
                };
                let Some(target) = into else {
                    return InstallVerdict::Refused {
                        rule: "needs_a_target",
                        errors: vec![
                            "a widget is checked against the household it would join, so one \
                             has to be named"
                                .to_string(),
                        ],
                    };
                };
                if self.with(|i| i.get(target).is_none()) {
                    return InstallVerdict::Refused {
                        rule: "needs_a_target",
                        errors: vec![format!("no such Sustain: {target}")],
                    };
                }
                // ★★★ **Checked against the VIEW definition, not the
                //     household's**, and that is not a shortcut — it is the
                //     schema the widget will actually be loaded against.
                //     `compose(r)` runs over a PROJECTION (slice 5's finding:
                //     root-granularity attribution makes per-card urgency
                //     impossible on a nested money shape), so an Orchie card
                //     reads `liquid` and `worst_spent`, never
                //     `finances.liquid.balance`. Checking it against the
                //     household's own schema here would admit widgets the feed
                //     then refuses — two gates disagreeing, which is worse than
                //     either being wrong.
                //
                // ★ The target still matters: it names WHOSE feed this joins.
                widget_installs(authored.to_decl(), &crate::orchie::view_definition(), &self.operators)
            }
            Kind::Operator => {
                let name = package.spec.get("name").and_then(Value::as_str).unwrap_or(&package.name);
                operator_installs(name, &self.operators)
            }
            Kind::Strategy => strategy_installs(&package.name),
        }
    }

    /// Publish an artifact: stamp it, **gate it**, then store it.
    ///
    /// ★★★ The gate runs BEFORE the write. A registry that stored a broken
    /// artifact and refused it later would be a place where bad things wait —
    /// which is precisely what the reference does.
    pub fn publish(&self, request: &Publication) -> Result<(Package, InstallVerdict), String> {
        let me = self
            .identity
            .lock()
            .expect("identity lock")
            .clone()
            .ok_or_else(|| "this node is locked, so nothing can be published under its key".to_string())?;

        let Some(kind) = Kind::parse(&request.kind) else {
            return Err(format!(
                "this build has no package kind called '{}' — it knows definition, widget,                  operator and strategy",
                request.kind
            ));
        };
        let content_hash = content_hash(&request.spec);
        let package = Package {
            id: format!("pkg-{}", &content_hash[..12]),
            name: request.name.clone(),
            kind,
            version: request.version.clone(),
            description: request.description.clone(),
            tags: request.tags.clone(),
            spec: request.spec.clone(),
            author: me.public_key(),
            author_handle: me.handle().to_string(),
            // ★ Signed over the HASH, not over the spec: the signature and the
            //   integrity check then agree by construction, and a signature can
            //   be verified without re-serialising anything.
            signature: Some(me.sign(signed_message(&content_hash).as_bytes())),
            content_hash,
            origin: Origin::Authored,
            per_mille: request.per_mille,
            published_at: now_secs(),
        };

        let verdict = self.judge(&package, request.into.as_deref());
        match &verdict {
            InstallVerdict::Refused { .. } => Ok((package, verdict)),
            // ★★ `NotHere` publishes. Nothing is wrong with a strategy; this
            //    node simply has nothing that would run one, and refusing to
            //    publish it would make this registry unable to carry anything
            //    it cannot itself consume.
            _ => {
                self.arena.record(package.clone())?;
                self.install_offers();
                Ok((package, verdict))
            }
        }
    }

    /// Install a published package, through the **same** gate.
    ///
    /// ★★★ Three checks in order, and each answers its own question:
    /// integrity (are these the published bytes), authenticity (did that key
    /// publish them), then the **real gate**. A package can be impeccably
    /// signed and still refused; it can be unsigned and still install.
    pub fn install(&self, package_id: &str, into: Option<&str>) -> Result<Installed, String> {
        let Some(package) = self.arena.get(package_id) else {
            return Err(format!("no such package: {package_id}"));
        };
        let provenance = package.provenance();
        if !provenance.safe_to_install() {
            return Ok(Installed {
                verdict: InstallVerdict::Refused {
                    rule: "provenance",
                    errors: vec![provenance.describe()],
                },
                provenance,
                applied: None,
            });
        }

        let verdict = self.judge(&package, into);
        let Some(admitted) = verdict.admitted() else {
            return Ok(Installed { verdict, provenance, applied: None });
        };

        // ★★★ From here the artifact takes the SAME path a local one does.
        let applied = match admitted.kind() {
            Kind::Definition => {
                let authored: AuthoredDefinition = serde_json::from_value(package.spec.clone())
                    .map_err(|e| e.to_string())?;
                // The same call the Define screen makes. It re-runs the check,
                // which is not waste: it is the one place a definition lands.
                match self.author_definition(&authored).map_err(|e| e.to_string())? {
                    DefinitionVerdict::Accepted => Some(authored.id.clone()),
                    other => {
                        return Ok(Installed {
                            verdict: InstallVerdict::Refused {
                                rule: "well_typed",
                                errors: vec![format!("{other:?}")],
                            },
                            provenance,
                            applied: None,
                        })
                    }
                }
            }
            // A widget install is a record: `orchie::feed` reads installed
            // widgets back and offers them alongside the built-in set.
            Kind::Widget => Some(admitted.id().to_string()),
            // An operator was already present — that is what admitting it
            // meant. Recording the install is what makes it visible as chosen.
            Kind::Operator => Some(admitted.id().to_string()),
            Kind::Strategy => None,
        };

        if let Some(artifact_id) = &applied {
            self.arena.record_install(Install {
                package_id: package.id.clone(),
                kind: package.kind,
                artifact_id: artifact_id.clone(),
                into: into.map(str::to_string),
                content_hash: package.content_hash.clone(),
                installed_at: now_secs(),
            })?;
        }

        Ok(Installed { verdict, provenance, applied })
    }

    /// Pay a package's royalty, in **juul**.
    ///
    /// ★★★ **Internal points, and the boundary is the point.** `royalty::settle`
    /// only ever calls `JuulLedger::transfer`, so circulation is unchanged and
    /// nothing is minted. Per ADR-0001 D5 juul is internal accounting on a
    /// single host: it is **never real money, never transferable off this host,
    /// and never a payment rail**. A package trades in definitions, trust and
    /// internal credit — never cash.
    pub fn pay_royalty(&self, package_id: &str, amount: u64) -> Result<Settlement, String> {
        let Some(package) = self.arena.get(package_id) else {
            return Err(format!("no such package: {package_id}"));
        };
        let payer = self.principal().ok_or_else(|| "this node is locked".to_string())?;
        let licence = if package.per_mille == 0 {
            Licence::Free
        } else {
            Licence::Royalty { per_mille: package.per_mille, payee: package.author_handle.clone() }
        };
        let mut economy = self.economy.lock().expect("economy lock");
        let recipients = Recipients {
            contributor: package.author_handle.clone(),
            treasury: TREASURY.to_string(),
            // ★ A single host genuinely has no validator — there is no second
            //   node to validate anything. An absent role's share folds into
            //   the treasury by the declared rule, never dropped.
            validator: None,
            proposer: None,
            referrer: None,
        };
        Ok(settle(
            &mut economy.ledger,
            &payer,
            &licence,
            amount,
            // ★★ `Access`, not `Usage`: installing is paying to HAVE it. Paying
            //    to run it is the pawa meter's business, and that is a cost
            //    rather than a transfer.
            RevenueType::Access,
            &recipients,
        ))
    }

    /// Stamp a line with this node and the next clock for that Sustain.
    ///
    /// ★★★ **Every local write is stamped, from genesis onward.** A line
    /// without an origin is only readable because the migration resolves it
    /// to this node; nothing NEW should need that fallback, or the fallback
    /// becomes load-bearing and the next node to join inherits ambiguity.
    ///
    /// ★ The lamport comes from the replica rather than from a counter in
    /// memory, so a line written after a peer's entries arrived is ordered
    /// **after** them — which is §IV's `max(C_i, C_msg) + 1` and the whole
    /// reason a merged history stays causal.
    fn stamped(&self, sustain_id: &str, seq: u64, line: LoggedEvent) -> LoggedEvent {
        let Some(node) = self.node_id() else {
            return line;
        };
        let Ok(replica) = self.store.read_replica(sustain_id, &node) else {
            return line;
        };
        let lamport = replica.next_write(&node).1;
        line.written_by(&node, seq, lamport, replica.frontier())
    }

    /// This node's own network identity — its public key — readable while
    /// **locked**, because it is public and a screen should be able to show
    /// who this machine is without holding the private half.
    pub fn node_id(&self) -> Option<String> {
        self.identities.read().ok().map(|f| f.public_key)
    }

    /// Start accepting peers. `None` port asks the OS for a free one.
    pub fn listen(&self, port: Option<u16>) -> Result<u16, String> {
        if !self.is_unlocked() {
            return Err("this node is locked, so it cannot prove its own key".into());
        }
        self.install_specs();
        match port {
            Some(p) => self.peering.listen_on(p),
            None => self.peering.listen(),
        }
    }

    /// Converge one Sustain with one peer, then re-read it from disk.
    ///
    /// ★★★ **The reload is not a refresh, it is the correction.** The world
    /// holds a state folded from the log as it was; a sync appends entries
    /// that may belong EARLIER in causal order than lines already there, so
    /// the in-memory state is not merely stale, it is folded from the wrong
    /// sequence. Re-folding in causal order is what makes the two nodes equal.
    /// Teach the peering to answer *what is this Sustain*.
    ///
    /// ★ A closure over the registry rather than a copy of it: a Sustain
    /// created after the listener started is describable without restarting
    /// anything.
    fn install_specs(&self) {
        let specs = self.with(|i| {
            i.order()
                .iter()
                .filter_map(|id| {
                    let s = i.get(id)?;
                    // ★★ An authored definition is refused rather than
                    //    half-shared — see `SharedSpec`. Omitting it means the
                    //    receiver is told what it is missing rather than handed
                    //    a household whose rules it cannot reconstruct.
                    if s.record.custom.is_some() {
                        return None;
                    }
                    Some((
                        id.clone(),
                        SharedSpec {
                            label: s.record.label.clone(),
                            template: s.record.template.label().to_string(),
                            owner: s.record.owner.clone(),
                        },
                    ))
                })
                .collect()
        });
        self.peering.set_specs(specs);
    }

    /// Register a Sustain this node is meeting for the first time.
    ///
    /// ★★★ **No genesis is written.** The opening line arrives as an entry
    /// like every other; writing one here would fork the history at line zero
    /// — two genesis lines, two origins, and a household that never agrees
    /// about where it started.
    pub fn adopt(&self, id: &str, spec: &SharedSpec) -> Result<bool, String> {
        let Some(template) = TemplateId::all().into_iter().find(|t| t.label() == spec.template)
        else {
            return Err(format!("this node has no template called '{}'", spec.template));
        };
        let mut inner = self.inner.lock().expect("world lock");
        if inner.sustains.contains_key(id) {
            return Ok(false);
        }
        let (definition, _) = self
            .resolve_template(template, None)
            .ok_or_else(|| format!("no such definition: {}", spec.template))?;
        let enforcement = enforcement_of(&definition);
        let record = SustainRecord {
            id: id.to_string(),
            label: spec.label.clone(),
            template,
            custom: None,
            parent: None,
            owner: spec.owner.clone(),
            owners: Vec::new(),
        };
        inner.order.push(id.to_string());
        inner.sustains.insert(
            id.to_string(),
            // ★ An empty state and seq 0 until the fold runs: the log is the
            //   authority, and it has not been read yet.
            Sustain { record, definition, enforcement, state: Value::Null, next_seq: 0 },
        );
        self.persist_registry(&inner).map_err(|e| e.to_string())?;
        drop(inner);
        self.install_specs();
        Ok(true)
    }

    pub fn sync_peer(&self, address: &str, sustain_id: &str) -> Result<SyncReport, String> {
        let me = self
            .identity
            .lock()
            .expect("identity lock")
            .clone()
            .ok_or_else(|| "this node is locked".to_string())?;
        self.install_specs();
        let outcome = self.peering.sync(address, sustain_id, &me)?;
        // ★★ Adopt BEFORE folding: without a definition there is nothing to
        //    check the merged state against, and `reload` would report
        //    *unmeasured* for a household whose rules did arrive.
        if let Some(spec) = &outcome.spec {
            self.adopt(sustain_id, spec)?;
        }
        let merge = self.reload(sustain_id)?;
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.peering.edit(|b| b.note_sync(&outcome.peer, at))?;
        Ok(SyncReport { outcome, merge })
    }

    /// Re-fold one Sustain from disk in causal order, and adopt the result.
    ///
    /// ★★ Returns the [`Reconciliation`] rather than swallowing it, because
    /// *the merged state is outside the viable region* is exactly the thing a
    /// person needs told — every entry was admitted where it was made, and
    /// the merge was not.
    pub fn reload(&self, sustain_id: &str) -> Result<Reconciliation, String> {
        let node = self.node_id().ok_or_else(|| "no identity on this machine".to_string())?;
        // ★★ The Sustain's OWN invariants, and only when enforcement is armed
        //    for it. A Sustain that never opted in is genuinely unmeasured
        //    rather than passing: reporting `admissible: true` for a household
        //    that declared no rules would be a claim it never made.
        let rules: Option<Vec<(String, String)>> = self.with(|i| {
            i.get(sustain_id).and_then(|s| {
                s.enforcement.enabled.then(|| s.enforcement.invariants.clone())
            })
        });
        let (out, next_seq) = self
            .store
            .load_replicated(sustain_id, &node, rules.as_deref())
            .map_err(|e| e.to_string())?;
        let mut inner = self.inner.lock().expect("world lock");
        if let Some(su) = inner.sustains.get_mut(sustain_id) {
            su.state = out.state.clone();
            su.next_seq = next_seq;
        }
        Ok(out)
    }

    // ── the membership model ─────────────────────────────────

    /// `path` for `permitted`: outermost containing Sustain first, target last.
    ///
    /// ★★ The weakest link along this path is the authority in force, so the
    /// path is not decoration — it is what makes "a child cannot be acted on
    /// more freely than its household" true by construction.
    pub fn authority_path(&self, sustain_id: &str) -> Vec<String> {
        let mut chain = vec![sustain_id.to_string()];
        let mut cursor = sustain_id.to_string();
        // Bounded by the number of Sustains: a cycle cannot outlast it, and
        // `flatten_holarchy` refuses one anyway.
        for _ in 0..64 {
            let parent = self.with(|i| i.get(&cursor).and_then(|s| s.record.parent.clone()));
            match parent {
                Some(p) => {
                    chain.push(p.clone());
                    cursor = p;
                }
                None => break,
            }
        }
        chain.reverse();
        chain
    }

    /// The declared membership graph for the authenticated principal.
    ///
    /// ★★★ **The household model, and it is not uniform on purpose.** The
    /// person is OWNER of the household and of their OWN habitat, and an
    /// OBSERVER on every other member's habitat. That is what a household
    /// actually means: you can see what your family holds — the roll-up reads
    /// state, which authority does not gate — and you may not spend it.
    ///
    /// ★ A uniform owner-everywhere graph would make the enforcement true and
    /// pointless: nothing would ever be refused, and a check that can never
    /// fire proves nothing about the gate.
    pub fn memberships_for(&self, principal: &str) -> Memberships {
        let mut m = Memberships::new();
        let ids: Vec<String> = self.with(|i| i.order().to_vec());
        for id in ids {
            let owner = self.with(|i| i.get(&id).and_then(|s| s.record.owner.clone()));
            // ★ OWNER where the Sustain says so, OBSERVER everywhere else. An
            //   unclaimed Sustain is not yours; it is nobody's, and reading it
            //   is all anyone may do.
            let tier = if owner.as_deref() == Some(principal) { TIER_OWNER } else { TIER_OBSERVER };
            m.grant(MembershipEdge {
                principal: principal.to_string(),
                sustain: id,
                tier,
                skin: None,
            });
        }
        m
    }

    /// ★★ A store seeded before ownership was declared has none, which would
    /// lock its own household out of itself. Claim the root and the enrolling
    /// handle's own habitat once, and say so — a silent backfill of an
    /// authority model would be the wrong thing to do quietly.
    pub fn claim_unowned_for(&self, principal: &str) -> StoreResult<Vec<String>> {
        let ids: Vec<String> = self.with(|i| i.order().to_vec());
        let any_owner = ids.iter().any(|id| {
            self.with(|i| i.get(id).map(|s| s.record.owner.is_some())).unwrap_or(false)
        });
        if any_owner {
            return Ok(Vec::new());
        }
        let mut claimed = Vec::new();
        {
            let mut inner = self.inner.lock().expect("world lock");
            for id in &ids {
                let Some(su) = inner.sustains.get_mut(id) else { continue };
                let is_root = su.record.parent.is_none();
                if is_root || id == "habitat-bonnie" {
                    su.record.owner = Some(principal.to_string());
                    claimed.push(id.clone());
                }
            }
        }
        if !claimed.is_empty() {
            let inner = self.inner.lock().expect("world lock");
            self.persist_registry(&inner)?;
        }
        Ok(claimed)
    }

    /// Every declared parent/child edge, as `holon::Linked` needs them.
    fn links(&self) -> Vec<Link> {
        self.with(|i| {
            i.order()
                .iter()
                .filter_map(|id| i.get(id))
                .filter_map(|s| s.record.parent.as_ref().map(|p| Link::new(&s.record.id, p)))
                .collect()
        })
    }

    /// ★★★ **The atomic, conserved cross-Sustain transfer.**
    ///
    /// The engine decides; this writes. Both legs land in both logs or neither
    /// does — see [`Store::commit_transfer`] for how, and why a write-ahead
    /// journal is the only honest mechanism when two files must move together.
    ///
    /// ★★ The `transfer_id` is minted HERE, not in the core: the core has no
    /// clock and no random source on purpose, so that its answers are
    /// reproducible. Identity is a host concern.
    ///
    /// Returns the refusal as a value, never an error — a refused transfer is
    /// an expected outcome the gate produced, not a failure of the machinery.
    pub fn transfer(
        &self,
        from_id: &str,
        to_id: &str,
        path: &str,
        amount: f64,
    ) -> StoreResult<Transfer> {
        // ★★★ A transfer is not a registry operator, so it does not inherit
        //   `execute_afforded`'s authorization conjunct — it needs its own, and
        //   it needs it on BOTH sides. Moving money out of a Sustain you may
        //   not act on is exactly the hole an operator-only check would leave.
        let Some(principal) = self.principal() else {
            return Ok(Transfer::Refused {
                rule: "not_authenticated".into(),
                reason: "the local identity is locked — unlock it before acting".into(),
            });
        };
        let memberships = self.memberships_for(&principal);
        for side in [from_id, to_id] {
            let path = self.authority_path(side);
            if let Err(denial) =
                permitted(&memberships, &principal, &path, TRANSFER_MIN_PRIVILEGE)
            {
                return Ok(Transfer::Refused {
                    rule: denial_rule(&denial).to_string(),
                    reason: format!("'{side}': {denial}"),
                });
            }
        }

        let links = self.links();
        let Some(link) = Linked::between(from_id, to_id, &links) else {
            return Ok(Transfer::Refused {
                rule: "holon_link_exists".into(),
                reason: format!(
                    "'{to_id}' is not a directly linked parent or child of '{from_id}'."
                ),
            });
        };

        let parties = self.with(|i| {
            let a = i.get(from_id)?;
            let b = i.get(to_id)?;
            Some((
                Party::new(from_id, a.state.clone(), a.enforcement.clone()),
                Party::new(to_id, b.state.clone(), b.enforcement.clone()),
                a.next_seq,
                b.next_seq,
            ))
        });
        let Some((from, to, from_seq, to_seq)) = parties else {
            return Ok(Transfer::Refused {
                rule: "holon_known".into(),
                reason: "one of the two Sustains is not open in this world.".into(),
            });
        };

        let settled = holon_transfer(&link, &from, &to, &Moving::money(path), amount);
        let Transfer::Committed(s) = &settled else {
            // ★ Nothing was produced, so there is nothing to unwind.
            return Ok(settled);
        };

        let (debit, credit) = s.legs();
        let transfer_id = format!("t-{from_id}-{to_id}-{from_seq}-{to_seq}");
        self.store.commit_transfer(
            &transfer_id,
            [
                (
                    debit.sustain_id().to_string(),
                    self.stamped(debit.sustain_id(), from_seq, leg_line(from_seq, debit)),
                ),
                (
                    credit.sustain_id().to_string(),
                    self.stamped(credit.sustain_id(), to_seq, leg_line(to_seq, credit)),
                ),
            ],
        )?;

        // Only after both lines are durable.
        {
            let mut inner = self.inner.lock().expect("world lock");
            for (leg, seq) in [(debit, from_seq), (credit, to_seq)] {
                if let Some(su) = inner.sustains.get_mut(leg.sustain_id()) {
                    su.state = leg.state_after().clone();
                    su.next_seq = seq + 1;
                }
            }
        }
        Ok(settled)
    }

    /// **ρ** — the declared totals for one Sustain, folded from its own state
    /// and every linked child's.
    ///
    /// ★★★ The host reads states and the ENGINE does the arithmetic. Nothing
    /// here sums anything: it hands `compute_rollup` the parent's state and the
    /// children's, and reports what came back — the same division of labour as
    /// `V`, where the host asks `predicate::check` rather than judging a rule.
    ///
    /// ★★ Computed fresh, never cached. A household figure that could be stale
    /// would be a number with no way to say when it stopped being true.
    ///
    /// `None` only when there is no such Sustain. A Sustain that declares no
    /// aggregates returns a real, empty reading — *asked for no totals* is a
    /// different fact from *a total that could not be computed*.
    pub fn rollup(&self, sustain_id: &str) -> Option<RollupDto> {
        self.with(|i| {
            let parent = i.get(sustain_id)?;
            let children: Vec<ChildState> = i
                .children_of(sustain_id)
                .into_iter()
                .map(|c| {
                    // ★ Every child in this host is in memory and readable, so
                    //   the unreadable arm is not exercised here — but the
                    //   contract travels anyway, because the state a caller
                    //   cannot read is exactly what must not silently become 0.
                    ChildState::readable(&c.record.id, c.state.clone())
                        .named(c.record.label.as_str())
                })
                .collect();
            let r = compute_rollup(
                &parent.definition.aggregates,
                sustain_id,
                Some(&parent.state),
                &children,
            );
            Some(RollupDto::of(sustain_id, &r))
        })
    }

    /// The Sustain whose roll-up a commit on `sustain_id` could have changed.
    ///
    /// ★ A member's commit moves its household's total, not its own — so this
    /// answers with the PARENT when there is one, and with the Sustain itself
    /// when it is a parent that declares totals. `None` when neither, which is
    /// most Sustains and is why the channel stays quiet for them.
    pub fn rollup_subject(&self, sustain_id: &str) -> Option<String> {
        self.with(|i| {
            let s = i.get(sustain_id)?;
            if let Some(p) = &s.record.parent {
                if i.get(p).is_some_and(|p| !p.definition.aggregates.is_empty()) {
                    return Some(p.clone());
                }
            }
            if !s.definition.aggregates.is_empty() {
                return Some(sustain_id.to_string());
            }
            None
        })
    }
}

impl Inner {
    pub fn order(&self) -> &[String] {
        &self.order
    }
    pub fn get(&self, id: &str) -> Option<&Sustain> {
        self.sustains.get(id)
    }
    pub fn selected(&self) -> Option<&Sustain> {
        self.selected.as_ref().and_then(|id| self.sustains.get(id))
    }
    pub fn children_of<'a>(&'a self, id: &str) -> Vec<&'a Sustain> {
        self.order
            .iter()
            .filter_map(|k| self.sustains.get(k))
            .filter(|s| s.record.parent.as_deref() == Some(id))
            .collect()
    }
}

// ── authored definitions ─────────────────────────────────────────────────────

impl World {
    pub fn definitions(&self) -> Vec<AuthoredDefinition> {
        self.definitions.lock().expect("definitions lock").clone()
    }

    /// ★★★ Check an authored definition with the ENGINE, and persist it only if
    /// the engine accepted it.
    ///
    /// `typecheck` parses every invariant and binds it against the schema;
    /// `safe` evaluates the candidate against every live instance already on
    /// this definition. A definition that fails either is **never written**, so
    /// the store cannot hold one that was broken when it was authored.
    pub fn author_definition(&self, authored: &AuthoredDefinition) -> StoreResult<DefinitionVerdict> {
        // Live instances already on this definition — empty for a new one,
        // which is exactly why creating is safe and editing is where stranding
        // can bite.
        let instances: Vec<sustena_core::editing::Instance> = self.with(|i| {
            i.order()
                .iter()
                .filter_map(|id| i.get(id))
                .filter(|s| s.record.custom.as_deref() == Some(authored.id.as_str()))
                .map(|s| sustena_core::editing::Instance {
                    id: s.record.id.clone(),
                    state: s.state.clone(),
                })
                .collect()
        });

        let verdict = check_definition(authored, &instances);
        if matches!(verdict, DefinitionVerdict::Accepted) {
            self.store.append_definition(authored)?;
            let mut defs = self.definitions.lock().expect("definitions lock");
            defs.retain(|d| d.id != authored.id);
            defs.push(authored.clone());
        }
        Ok(verdict)
    }

    /// The `Definition` and opening state for a template ref — built-in or authored.
    ///
    /// ★ One resolver, so an authored definition is instantiated through
    /// **exactly** the path a built-in uses. Nothing about being user-written
    /// makes it a second-class Sustain.
    pub fn resolve_template(
        &self,
        template: TemplateId,
        custom: Option<&str>,
    ) -> Option<(Definition, Value)> {
        match custom {
            None => Some((templates::definition(template), templates::opening_state(template))),
            Some(id) => {
                let defs = self.definitions.lock().expect("definitions lock");
                let d = defs.iter().find(|d| d.id == id)?;
                Some((d.to_definition(), d.opening_state.clone()))
            }
        }
    }
}

// ── capability: the real authorization model ─────────────────────────────────

/// ★★★ The household capability this app runs under, and what it permits.
///
/// The core has a genuine capability model — `Capability::issue`, `attenuate`
/// (which **refuses amplification**), and `permits(sustain, operator, tier)`.
/// This builds the one the local principal holds and asks it about every
/// operator, so the Profile screen shows a real permission matrix.
///
/// ★★ **And it is not yet what the gate checks.** Every call in this host still
/// runs `Authorization::Unchecked`, so this is the authority a person HAS,
/// displayed — not an authority that is currently being enforced. Saying that
/// plainly is the difference between a security model and a security theatre.
/// The refusal name for one denial — the SAME six the core's own gate uses, so
/// an authorization refusal reads identically whichever path produced it.
pub fn denial_rule(d: &Denial) -> &'static str {
    match d {
        Denial::NoEdge { .. } => "not_a_member",
        Denial::InsufficientTier { .. } => "insufficient_privilege",
        Denial::UnresolvedSkin { .. } => "skin_resolves",
        Denial::NotDesignated { .. } => "capability_designates",
        Denial::RightNotHeld { .. } => "capability_carries",
    }
}

/// One leg as a log line. ★ Both carry the same `operator`, so a reader
/// scanning either log sees the same event by the same name.
fn leg_line(seq: u64, leg: &Leg) -> LoggedEvent {
    LoggedEvent {
        seq,
        operator: "holon.transfer".to_string(),
        events: vec![EventDto::from(leg.event())],
        mutations: leg.mutations().to_vec(),
        origin: None,
        lamport: None,
        clock: None,
    }
}

#[cfg(test)]
mod trend_tests {
    use super::*;
    use crate::store::Store;
    use serde_json::json;

    fn world(name: &str) -> World {
        let home = std::env::temp_dir().join(format!("mycelium-trend-{name}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("home");
        let w = World::open(Store::at(&home).expect("store")).expect("world");
        w.enrol(DEFAULT_HANDLE, "a-long-enough-passphrase").expect("enrol");
        w.instantiate_owned("home", "Home", TemplateId::Homestead, None, None, Some(DEFAULT_HANDLE))
            .expect("household");
        w
    }

    /// Make something really happen, so the sustain's sequence moves.
    ///
    /// ★★ The series ticks on events. A test that called `observe` repeatedly
    /// without changing anything would be testing the very thing the seq gate
    /// exists to prevent.
    fn something_happens(w: &World, n: usize) {
        let mut params = Map::new();
        params.insert("pocket_name".into(), Value::String(format!("p{n}")));
        let _ = w.call("home", "budget.add_pocket", &params);
    }

    /// Inside its own limits.
    fn calm() -> Value {
        json!({"liquid": 5000.0, "worst_spent": 100.0, "worst_allocated": 1000.0,
               "unclassified": 0.0})
    }

    /// Over its own limit by `by`.
    fn over(by: f64) -> Value {
        json!({"liquid": 5000.0, "worst_spent": 1000.0 + by, "worst_allocated": 1000.0,
               "unclassified": 0.0})
    }

    #[test]
    fn a_reading_now_has_a_history_to_be_read_against() {
        // ★★★ The whole of P3. Before this the distance was computed on every
        //     render and thrown away, so there was nothing to smooth.
        let w = world("series");
        something_happens(&w, 0);
        let first = w.observe("home", &calm()).expect("a reading");
        something_happens(&w, 1);
        let second = w.observe("home", &over(200.0)).expect("a reading");
        assert!(second.reading.w > first.reading.w, "it really did get worse");
        assert!(
            second.reading.smoothed < second.reading.w,
            "the level lags the jump rather than chasing it, which is what smoothing is for"
        );
    }

    #[test]
    fn looking_again_is_not_something_happening() {
        // ★★★ The bug this gate exists for, found by watching the series. The
        //     feed rebuilds on every poll, so observing per call fed the
        //     detectors fresh readings when nothing had changed — and a
        //     household sitting still would "drift" purely because someone had
        //     the app open.
        let w = world("renders");
        something_happens(&w, 0);
        let first = w.observe("home", &over(300.0)).expect("a reading");
        let again = w.observe("home", &over(300.0)).expect("the same reading");
        let third = w.observe("home", &over(300.0)).expect("still the same");
        assert_eq!(first.reading.smoothed, again.reading.smoothed);
        assert_eq!(first.reading.smoothed, third.reading.smoothed);

        // ★ And when something really does happen, the series moves again.
        //   Checked with a DIFFERENT reading: the level seeds from the first
        //   observation, so repeating an identical one correctly leaves it
        //   exactly where it was, which would prove nothing either way.
        something_happens(&w, 1);
        let moved = w.observe("home", &over(900.0)).expect("a new reading");
        assert!(
            moved.reading.smoothed > first.reading.smoothed,
            "a real change moves the level"
        );
    }

    #[test]
    fn a_household_inside_its_limits_is_never_told_it_is_drifting() {
        let w = world("calm");
        let mut alerted = false;
        for n in 0..20 {
            something_happens(&w, n);
            if let Some(r) = w.observe("home", &calm()) {
                alerted |= r.reading.alert.is_some();
            }
        }
        assert!(!alerted, "nothing was ever wrong, so nothing should have been said");
    }

    #[test]
    fn a_small_persistent_gap_is_caught_though_no_single_day_looks_bad() {
        // ★★★ The case a threshold cannot see, and the reason CUSUM is here:
        //     each of these readings on its own is unremarkable, and the
        //     household's own allocation is what makes "small" mean anything.
        let w = world("drift");
        let mut alerted = false;
        for n in 0..20 {
            something_happens(&w, n);
            if let Some(r) = w.observe("home", &over(60.0)) {
                alerted |= r.reading.alert.is_some();
            }
        }
        assert!(alerted, "a gap that keeps repeating is a real signal, whatever one day says");
    }

    #[test]
    fn the_threshold_scales_to_what_the_household_itself_set_aside() {
        // ★★★ A fixed threshold would shout forever at a household budgeting
        //     hundreds of thousands and stay silent for one budgeting hundreds.
        let small = crate::orchie::drift_spec(&json!({"worst_allocated": 1000.0}));
        let large = crate::orchie::drift_spec(&json!({"worst_allocated": 100000.0}));
        assert!(large.delta > small.delta * 50.0, "the bigger household needs a bigger shift");
    }

    #[test]
    fn health_is_read_off_the_core_encoder_not_re_derived() {
        // ★★★ Monitor §VII, end to end. The ranking was computed correctly and
        //     drawn flat, so it lived in the data and never reached the eye.
        //     `encode_field` had been in the core since it shipped with nothing
        //     calling it; this is the caller.
        use sustena_core::preattentive::{encode_field, Hue, VisualAttribute};

        let w = world("hue");
        something_happens(&w, 0);
        let calm_reading = w.observe("home", &calm()).expect("a reading");
        let hue_of = |r: &TrendReading| {
            encode_field(&[r])
                .first()
                .and_then(|s| {
                    s.attributes().iter().find_map(|a| match a {
                        VisualAttribute::Hue { value, .. } => Some(*value),
                        _ => None,
                    })
                })
                .expect("the encoder always assigns a hue")
        };
        assert_eq!(hue_of(&calm_reading), Hue::Green, "inside its limits reads calm");

        let w2 = world("hue-bad");
        for n in 0..8 {
            something_happens(&w2, n);
            w2.observe("home", &over(800.0));
        }
        let bad = w2.observe("home", &over(800.0)).expect("a reading");
        assert_ne!(hue_of(&bad), Hue::Green, "well outside does not read as calm");
    }

    #[test]
    fn each_household_has_its_own_history() {
        let w = world("scoped");
        for n in 0..20 {
            something_happens(&w, n);
            w.observe("home", &over(60.0));
        }
        // A second household, calm, sharing nothing.
        w.instantiate_owned("other", "Other", TemplateId::Homestead, None, None, Some(DEFAULT_HANDLE))
            .expect("other");
        let b = w.observe("other", &calm()).expect("a reading");
        assert!(b.reading.alert.is_none(), "it has been calm and knows nothing of the first");
        assert_eq!(w.monitors_len(), 2);
    }
}

#[cfg(test)]
mod device_tests {
    use super::*;
    use crate::store::Store;

    /// A household owned by the enrolling handle, as enrolment leaves it in
    /// the real app.
    fn own_household(w: &World, id: &str, label: &str) {
        w.instantiate_owned(id, label, TemplateId::Homestead, None, None, Some(DEFAULT_HANDLE))
            .expect("household");
    }

    /// A real world on a scratch directory, enrolled so `call` has a principal.
    fn test_world(name: &str) -> World {
        let home = std::env::temp_dir().join(format!("mycelium-device-{name}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("home");
        let w = World::open(Store::at(&home).expect("store")).expect("world");
        w.enrol(DEFAULT_HANDLE, "a-long-enough-passphrase").expect("enrol");
        w
    }

    /// ★★★ §IX in one assertion: the phone is a CHILD SUSTAIN, not a special
    /// case bolted onto the ingest path. Everything already built for children
    /// reads it for free, which is the whole reason for modelling it this way.
    #[test]
    fn the_phone_becomes_a_child_of_the_household_it_reports_to() {
        let w = test_world("child");
        own_household(&w, "home", "Home");
        w.heartbeat("home", 3, 1_700_000_000_000).expect("heartbeat");

        let id = World::device_id("home");
        let rec = w.with(|i| i.get(&id).map(|s| s.record.clone())).expect("the device exists");
        assert_eq!(rec.parent.as_deref(), Some("home"), "watched by the household");
        assert_eq!(rec.template, TemplateId::Device);
    }

    #[test]
    fn a_heartbeat_records_the_queue_and_the_moment() {
        let w = test_world("records");
        own_household(&w, "home", "Home");
        w.heartbeat("home", 7, 1_700_000_000_000).expect("heartbeat");

        let state = w
            .with(|i| i.get(&World::device_id("home")).map(|s| s.state.clone()))
            .expect("state");
        assert_eq!(state.pointer("/device/queue_depth").and_then(Value::as_f64), Some(7.0));
        assert_eq!(
            state.pointer("/device/last_ack_ms").and_then(Value::as_f64),
            Some(1_700_000_000_000.0)
        );
    }

    #[test]
    fn reporting_twice_is_the_same_phone_not_two() {
        // ★★ The id is derived from the household, so a phone that reports on
        //    every sweep does not leave a trail of dead Sustains behind it.
        let w = test_world("twice");
        own_household(&w, "home", "Home");
        w.heartbeat("home", 1, 1_000).expect("first");
        w.heartbeat("home", 0, 2_000).expect("second");

        let devices = w.with(|i| {
            i.order.iter().filter(|id| id.starts_with("device-")).count()
        });
        assert_eq!(devices, 1, "one phone, reporting twice");
    }

    #[test]
    fn each_household_watches_its_own_phone() {
        let w = test_world("scoped");
        own_household(&w, "a", "A");
        own_household(&w, "b", "B");
        w.heartbeat("a", 5, 1_000).expect("a beat");

        assert!(w.with(|i| i.get(&World::device_id("a")).is_some()));
        assert!(
            w.with(|i| i.get(&World::device_id("b")).is_none()),
            "a household that has not reported has no device"
        );
    }
}

