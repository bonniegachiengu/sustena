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
};

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

        for record in &records.sustains {
            let definition = templates::definition(record.template);
            let enforcement = enforcement_of(&definition);
            let (state, next_seq) = store.load_state(&record.id)?;
            order.push(record.id.clone());
            sustains.insert(
                record.id.clone(),
                Sustain { record: record.clone(), definition, enforcement, state, next_seq },
            );
        }

        let selected = records.selected.filter(|id| sustains.contains_key(id)).or(order.first().cloned());

        Ok(World {
            operators: Registry::default(),
            inner: Mutex::new(Inner { sustains, order, selected }),
            store,
            economy: Mutex::new(Economy::open(PRINCIPAL)),
            meter: Mutex::new(Meter::new()),
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
        let mut inner = self.inner.lock().expect("world lock");
        if inner.sustains.contains_key(id) {
            return Ok(false);
        }
        if let Some(p) = parent {
            if !inner.sustains.contains_key(p) {
                return Err(StoreError::Io(format!("no such parent Sustain: {p}")));
            }
        }

        let state = templates::opening_state(template);
        let genesis = LoggedEvent::genesis(&state);
        self.store.append(id, &genesis)?;

        let record = SustainRecord {
            id: id.to_string(),
            label: label.to_string(),
            template,
            parent: parent.map(str::to_string),
        };
        let definition = templates::definition(template);
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
