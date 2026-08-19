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

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde_json::{Map, Value};
use sustena_core::{
    approval::{EffectClass, NonceLedger},
    detect::CusumSpec,
    editing::Definition,
    juul::Affordability,
    monitor::{MonitorEngine, SustainWatch},
    operator::{execute_admitted, execute_afforded, Authorization, Enforcement, Execution, Registry},
    pawa::{meter, Meter},
    predicate::check,
    region::Region,
    semantic::enforcement_of,
    compute_rollup, ChildState,
};

use sustena_core::principal::{MembershipEdge, Memberships, TIER_OWNER};

use crate::definitions::{check as check_definition, AuthoredDefinition, DefinitionVerdict};
use crate::dto::RollupDto;
use crate::economy::Economy;
use crate::store::{LoggedEvent, Registry as Records, Store, StoreError, StoreResult, SustainRecord};
use crate::templates::{self, TemplateId};

/// ★ The local principal this app runs as.
///
/// ★★ Declared, not authenticated. There is no sign-in and the gate still runs
/// every call under `Authorization::Unchecked` — this is the name the cockpit
/// displays, and binding it to a real capability is a later slice. Said plainly
/// so nobody mistakes a label for a check.
pub const PRINCIPAL: &str = "bg.myc";

/// One live Sustain.
pub struct Sustain {
    pub record: SustainRecord,
    pub definition: Definition,
    pub enforcement: Enforcement,
    pub state: Value,
    pub next_seq: u64,
}

/// Everything the cockpit can look at.
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
}

pub struct Inner {
    sustains: BTreeMap<String, Sustain>,
    order: Vec<String>,
    selected: Option<String>,
}

impl World {
    /// Open the household from disk, folding every log.
    pub fn open(store: Store) -> StoreResult<World> {
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

        Ok(World {
            operators: Registry::default(),
            inner: Mutex::new(Inner { sustains, order, selected }),
            store,
            economy: Mutex::new(Economy::open(PRINCIPAL)),
            meter: Mutex::new(Meter::new()),
            definitions: Mutex::new(definitions),
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
        let genesis = LoggedEvent::genesis(&state);
        self.store.append(id, &genesis)?;

        let record = SustainRecord {
            id: id.to_string(),
            label: label.to_string(),
            template,
            custom: custom.map(str::to_string),
            parent: parent.map(str::to_string),
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
        self.instantiate("homestead", "Homestead", TemplateId::Homestead, None)?;
        for member in ["Bonnie", "Cira", "Epha", "Mum", "Kui", "Frankie"] {
            let id = format!("habitat-{}", member.to_lowercase());
            self.instantiate(&id, member, TemplateId::Habitat, Some("homestead"))?;
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
                principal: PRINCIPAL,
                sustain: sustain_id,
                at: 0,
                serving: Some((&issuance, PRINCIPAL)),
            };
            execute_afforded(
                &self.operators,
                &sustain.definition.operators,
                &sustain.enforcement,
                &sustain.state,
                operator,
                params,
                &Authorization::Unchecked,
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
                meter(&x, m, &sustain.enforcement, &parameters, sustain_id, PRINCIPAL, 0)
            {
                self.meter.lock().expect("meter lock").record(reading);
            }
        }

        // ★ The seq a REFUSAL would have taken is never consumed: nothing is
        //   appended, so the next committed call takes it. A log with holes in
        //   it would imply events that were never written.
        let seq = sustain.next_seq;
        if x.committed() {
            self.store.append(sustain_id, &LoggedEvent::of(seq, operator, &x))?;
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
pub fn memberships() -> Memberships {
    // The declared membership graph. One Owner edge per Sustain, because this
    // is a single-person household today -- a real household with several
    // people is where the weakest-link path rule starts to earn its keep.
    let mut m = Memberships::new();
    for sustain in ["homestead", "habitat-bonnie", "habitat-cira", "habitat-epha",
                    "habitat-mum", "habitat-kui", "habitat-frankie"] {
        m.grant(MembershipEdge {
            principal: PRINCIPAL.to_string(),
            sustain: sustain.to_string(),
            tier: TIER_OWNER,
            skin: None,
        });
    }
    m
}
