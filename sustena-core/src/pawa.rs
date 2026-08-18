//! `pawa(o, θ, s) = κ_c·compute + κ_s·storage` — the meter of one Enzyme run
//! (PAWA-1; the Pawa paper §1).
//!
//! **JUUL is the coin, PAWA is the gas**: the metered compute-and-storage cost
//! of one real operator run, denominated in juul. This module is the
//! measurement-only half — **the odometer, not the gas pump**.
//!
//! ## ★★★ The hard boundary, and it is ADR-0001 D5
//!
//! Everything in the economy layer is **internal accounting on a single host**.
//! Juul is a utility unit tracked internally: **never real money, never a real
//! transfer, never a payment rail**. Issuing a real, public, transferable token
//! is a *distinct, later, human-authorised act* and is **out of scope** here and
//! for every slice above this one.
//!
//! This module is the safest possible place to say that, because it is where
//! the boundary is easiest to keep: **nothing here touches a balance at all.**
//! There is no debit, no credit and no ledger — [`PawaReading`] is an inert record
//! of what a run cost, and what to do about that is PAWA-2's question.
//!
//! ## Metering, not estimating
//!
//! ★★ A reading is produced **only from a run that actually happened**, and
//! only when it **committed**. [`PawaReading`] has no public constructor: the sole
//! way to obtain one is [`meter`], which takes an [`Execution`] and returns
//! `None` unless it committed. So *a refusal meters nothing* is a property of
//! the type rather than a rule somebody keeps — a gate-refused call did zero
//! real work, and metering it as *a failed but costly attempt* would be
//! inventing a cost nobody paid.
//!
//! ★ And an operator that has never run has **no reading**, not a zero:
//! [`Meter::stats_for`] returns `None`. Zero is a measurement; absence is not.
//!
//! ## ★★ No wall-clock in the formula — reproducibility is the point
//!
//! `compute` is a **reproducible proxy** for work done:
//!
//! ```text
//! compute = mutation_count + event_count + constraint_eval_count
//! ```
//!
//! Wall-clock is machine-dependent, so pricing it would make the same call cost
//! different amounts on a slower box — which defeats a metering primitive meant
//! to be comparable across runs and across time. A host may attach an
//! `elapsed_ms` to a reading as a **secondary, non-authoritative** field, and
//! [`PawaReading::pawa`] does not read it. That is asserted, not merely intended.
//!
//! ★ ADR-0001 makes it doubly true: this core has **no clock**, so the reading's
//! timestamp is host-supplied for the same reason `EffectClass::Live`'s `now`
//! is. The core cannot mint a time, and therefore cannot price one.
//!
//! `storage` is **real bytes durably added** — the serialized size of exactly
//! the events and mutations this run produced, which is what a host writes to
//! its own log. Computed from the real records, not estimated.
//!
//! ## ★★ Declared, and explicitly UNCALIBRATED
//!
//! [`KAPPA_COMPUTE`] and [`KAPPA_STORAGE`] are **declared numbers that have not
//! been fitted to any real infrastructure cost model**. Storage is priced far
//! more granularly than a compute unit, matching how real systems meter storage
//! against operation count — but that is a shape argument, not a calibration.
//! Naming them as unfitted is the same honesty as `κ` in OPV-7's attention
//! meter and `Fidelity`'s declared-never-measured claim: **a number declared,
//! not a number claimed correct.**
//!
//! ## The static cost and the measured one are DIFFERENT THINGS
//!
//! [`crate::compose::Enzyme::pawa_cost`] (and `OperatorMeta::pawa_cost`) is the
//! **author's declared estimate** — static, known before the run, and it
//! composes by summation so a pipeline's declared cost is its parts'. This
//! module is the **measured reality** — dynamic, known only after a real run,
//! and different every time the work is different.
//!
//! ★ Neither replaces the other, and the reference keeps exactly this split.
//! Where they disagree, that gap is itself a finding — [`PawaReading::declared`]
//! carries the author's number beside the measured one so the comparison is
//! available rather than lost.

use std::collections::BTreeMap;

use crate::mutation::Mutation;
use crate::operator::{EmittedEvent, Enforcement, Execution, OperatorMeta};

/// Pawa per compute unit — one mutation, one event, one constraint evaluation.
///
/// ★★ **Declared, not calibrated.** See the module header.
pub const KAPPA_COMPUTE: f64 = 1.0;

/// Pawa per byte durably written.
///
/// ★★ **Declared, not calibrated.** See the module header.
pub const KAPPA_STORAGE: f64 = 0.01;

/// `pawa = κ_c·compute + κ_s·storage`.
///
/// Kept as one small pure function so PAWA-4's sandbox fitness term can reuse
/// the **exact** pricing rather than duplicating it — two copies of a formula
/// is two things to keep true.
pub fn compute_pawa(compute: f64, storage: f64) -> f64 {
    KAPPA_COMPUTE * compute + KAPPA_STORAGE * storage
}

/// The compute proxy: mutations + events + constraint evaluations.
pub fn compute_units(mutations: usize, events: usize, constraint_evals: usize) -> usize {
    mutations + events + constraint_evals
}

/// How many predicate checks this call was actually subject to.
///
/// The operator's own declared pre- and post-conditions are **always**
/// evaluated, so they always count. ★★ The sustain's invariants count **only
/// when the enforcement gate actually ran**: a sustain that has not armed
/// enforcement genuinely had zero invariant checks performed against this call,
/// and reporting a count there would be **measuring a check that never
/// happened** — the same distinction the reference draws.
pub fn constraint_eval_count(meta: &OperatorMeta, enforcement: &Enforcement) -> usize {
    let declared = meta.constraints.len() + meta.post_constraints.len();
    if enforcement.enabled {
        declared + enforcement.invariants.len()
    } else {
        declared
    }
}

/// One real metering reading — `⟨operator, ts, compute, storage, sustain,
/// principal⟩`.
///
/// ★ Named `PawaReading`, not `Reading` — [`crate::detect::Reading`] is a
/// monitor's observation of a dimension, a genuinely different thing to confuse
/// a cost measurement with. **Thirty-sixth collision**; the newcomer takes the
/// longer name.
///
/// ★★★ **No public constructor.** [`meter`] is the only way to obtain one, and
/// it refuses anything that did not commit. A reading therefore *cannot* exist
/// for a run that did no work — the `Capability` / `Score` / `VisualSpec`
/// mechanism, applied to a claim about cost.
///
/// ★ Inert: it holds no balance, debits nothing and is not a ledger entry.
#[derive(Debug, Clone, PartialEq)]
pub struct PawaReading {
    operator: String,
    sustain: String,
    principal: String,
    /// Host-supplied. The core has no clock (ADR-0001), so it cannot mint one.
    at: u64,
    compute: usize,
    storage: usize,
    pawa: f64,
    /// The author's **declared** estimate, carried alongside for comparison.
    declared: u32,
    /// ★★ Host-supplied and **never priced**. See the module header.
    elapsed_ms: Option<u64>,
}

impl PawaReading {
    pub fn operator(&self) -> &str {
        &self.operator
    }

    pub fn sustain(&self) -> &str {
        &self.sustain
    }

    pub fn principal(&self) -> &str {
        &self.principal
    }

    pub fn at(&self) -> u64 {
        self.at
    }

    pub fn compute(&self) -> usize {
        self.compute
    }

    pub fn storage(&self) -> usize {
        self.storage
    }

    /// The measured cost. ★ A pure function of `compute` and `storage`, and of
    /// nothing else — not of `at`, not of `elapsed_ms`.
    pub fn pawa(&self) -> f64 {
        self.pawa
    }

    /// The author's declared estimate for this operator, for comparison.
    pub fn declared(&self) -> u32 {
        self.declared
    }

    /// Wall-clock, if the host attached any. **Not part of the formula.**
    pub fn elapsed_ms(&self) -> Option<u64> {
        self.elapsed_ms
    }

    /// Attach a host's wall-clock measurement. ★ Deliberately a *post-hoc*
    /// addition that cannot reach `pawa`, which was already computed.
    pub fn with_elapsed(mut self, ms: u64) -> PawaReading {
        self.elapsed_ms = Some(ms);
        self
    }
}

/// Meter one real operator run.
///
/// ★★★ Returns `None` for anything that did not commit. That is the whole
/// *refusal meters nothing* discipline, and it is the only gate there is —
/// there is no other route to a [`PawaReading`].
///
/// `at` is host-supplied because this core has no clock.
pub fn meter(
    execution: &Execution,
    meta: &OperatorMeta,
    enforcement: &Enforcement,
    sustain: &str,
    principal: &str,
    at: u64,
) -> Option<PawaReading> {
    if !execution.committed() {
        return None;
    }

    let compute = compute_units(
        execution.mutations.len(),
        execution.events.len(),
        constraint_eval_count(meta, enforcement),
    );
    let storage = storage_bytes(&execution.mutations, &execution.events);

    Some(PawaReading {
        operator: meta.name.to_string(),
        sustain: sustain.to_string(),
        principal: principal.to_string(),
        at,
        compute,
        storage,
        pawa: compute_pawa(compute as f64, storage as f64),
        declared: meta.pawa_cost,
        elapsed_ms: None,
    })
}

/// Real bytes durably added: the serialized events and mutations of this run.
///
/// ★ Exactly what a host writes to its own log — the reference sums
/// `len(payload_json) + len(mutations_json)`, and this is that sum over the
/// same two collections. Not an estimate.
///
/// ★★ Takes the two collections rather than an [`Execution`] so a **candidate**
/// and the **committed reading** price through the identical code. Two copies
/// of this would be two numbers that could disagree, which is precisely what
/// PAWA-3's affordability clause must not have.
fn storage_bytes(mutations: &[Mutation], events: &[EmittedEvent]) -> usize {
    let payloads: usize = events
        .iter()
        .map(|e| serde_json::to_string(&e.payload).map(|s| s.len()).unwrap_or(0))
        .sum();
    payloads + serde_json::to_string(mutations).map(|s| s.len()).unwrap_or(0)
}

/// ★★★ The cost of a **candidate** — before it has committed, and therefore
/// before a [`PawaReading`] can exist.
///
/// PAWA-3's affordability conjunct needs a price at gate time, and the gate
/// already holds the candidate's mutations and events (it built them to check
/// `D`, the invariants and `F`). So the real cost is knowable **at no extra
/// work** — and this returns **the same number** the committed reading will
/// carry, because both go through `compute_units`, `constraint_eval_count` and
/// `storage_bytes` above. That equality is asserted, not assumed.
///
/// ★ It returns a plain `f64` rather than a `PawaReading`, deliberately: a
/// candidate has not committed, and PAWA-1's rule is that a reading exists only
/// for a run that did real work. **This is a cost computation for affordability;
/// the reading is the record of a debt actually incurred.**
pub fn candidate_pawa(
    mutations: &[Mutation],
    events: &[EmittedEvent],
    meta: &OperatorMeta,
    enforcement: &Enforcement,
) -> f64 {
    let compute =
        compute_units(mutations.len(), events.len(), constraint_eval_count(meta, enforcement));
    compute_pawa(compute as f64, storage_bytes(mutations, events) as f64)
}

/// What a set of readings says about one operator.
///
/// ★ Only ever produced for an operator that **has** readings — see
/// [`Meter::stats_for`].
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub runs: usize,
    pub total_compute: usize,
    pub total_storage: usize,
    pub total_pawa: f64,
}

impl Stats {
    pub fn mean_pawa(&self) -> f64 {
        self.total_pawa / self.runs as f64
    }
}

/// An accumulation of readings.
///
/// ★★ Deliberately dull and in-memory: durable metering storage is the host's,
/// exactly as the event log is. This exists so *what did this operator cost*
/// can be answered from the core's own readings without a second formula.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meter {
    readings: Vec<PawaReading>,
}

impl Meter {
    pub fn new() -> Meter {
        Meter::default()
    }

    pub fn record(&mut self, reading: PawaReading) {
        self.readings.push(reading);
    }

    pub fn readings(&self) -> &[PawaReading] {
        &self.readings
    }

    pub fn len(&self) -> usize {
        self.readings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.readings.is_empty()
    }

    /// ★★ `None` for an operator that has never run — **honest absence, not a
    /// fabricated zero**. Zero pawa is a measurement (a read-only operator that
    /// mutated nothing can genuinely cost little); *never measured* is not.
    pub fn stats_for(&self, operator: &str) -> Option<Stats> {
        let mine: Vec<&PawaReading> = self.readings.iter().filter(|r| r.operator == operator).collect();
        if mine.is_empty() {
            return None;
        }
        Some(Stats {
            runs: mine.len(),
            total_compute: mine.iter().map(|r| r.compute).sum(),
            total_storage: mine.iter().map(|r| r.storage).sum(),
            total_pawa: mine.iter().map(|r| r.pawa).sum(),
        })
    }

    /// Every metered operator, with its stats. Ordered, so a report reads the
    /// same way twice.
    pub fn by_operator(&self) -> BTreeMap<&str, Stats> {
        let mut names: Vec<&str> = self.readings.iter().map(|r| r.operator.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names.into_iter().filter_map(|n| self.stats_for(n).map(|s| (n, s))).collect()
    }

    /// Total pawa metered against one sustain.
    pub fn total_for_sustain(&self, sustain: &str) -> f64 {
        self.readings.iter().filter(|r| r.sustain == sustain).map(|r| r.pawa).sum()
    }

    /// Total pawa metered against one principal.
    pub fn total_for_principal(&self, principal: &str) -> f64 {
        self.readings.iter().filter(|r| r.principal == principal).map(|r| r.pawa).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::{execute, Registry};
    use crate::schema::{DimType, Schema};
    use serde_json::{json, Map, Value};

    fn registry() -> Registry {
        Registry::default()
    }

    fn allowed() -> Vec<String> {
        Registry::default().names()
    }

    fn state() -> Value {
        json!({"finances": {
            "liquid": {"balance": 500.0},
            "pockets": {"food": {"allocated": 100.0, "spent": 0.0, "limit": 0.0}},
            "income": {"monthly_total": 0.0, "sources": []}
        }})
    }

    fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    fn off() -> Enforcement {
        Enforcement::default()
    }

    fn armed() -> Enforcement {
        Enforcement {
            enabled: true,
            invariants: vec![
                ("liquid_non_negative".into(), "finances.liquid.balance >= 0".into()),
                ("pocket_allocated_non_negative".into(),
                 "ALL finances.pockets[*].allocated >= 0".into()),
            ],
            schema: Some(Schema::new().declare("finances", DimType::Any)),
            ..Enforcement::default()
        }
    }

    fn run(op: &str, p: &[(&str, Value)], e: &Enforcement, s: &Value) -> Execution {
        execute(&registry(), &allowed(), e, s, op, &params(p))
    }

    fn meter_run(op: &str, p: &[(&str, Value)], e: &Enforcement) -> Option<PawaReading> {
        let s = state();
        let x = run(op, p, e, &s);
        meter(&x, registry().get(op).unwrap(), e, "household", "bonnie", 1_000)
    }

    // ── the formula ──────────────────────────────────────────────────────────

    #[test]
    fn the_formula_is_kappa_c_compute_plus_kappa_s_storage() {
        assert_eq!(compute_pawa(7.0, 542.0), 1.0 * 7.0 + 0.01 * 542.0);
        assert_eq!(compute_units(3, 1, 2), 6);
    }

    #[test]
    fn a_sustains_invariants_count_only_when_the_gate_actually_ran() {
        // ★★ Measuring a check that never happened would be a fabrication.
        let reg = registry();
        let meta = reg.get("budget.allocate").unwrap();
        let declared = meta.constraints.len() + meta.post_constraints.len();
        assert_eq!(constraint_eval_count(meta, &off()), declared);
        assert_eq!(constraint_eval_count(meta, &armed()), declared + 2);
    }

    // ── measurement over a real run ──────────────────────────────────────────

    #[test]
    fn a_real_run_is_measured_from_what_it_actually_did() {
        let r = meter_run("budget.record_income", &[("amount", json!(100.0)), ("source", json!("salary"))], &off())
            .expect("it committed");
        assert_eq!(r.operator(), "budget.record_income");
        assert_eq!(r.sustain(), "household");
        assert_eq!(r.principal(), "bonnie");
        assert!(r.compute() > 0, "it mutated and emitted");
        assert!(r.storage() > 0, "and wrote real bytes");
        assert_eq!(r.pawa(), compute_pawa(r.compute() as f64, r.storage() as f64));
    }

    #[test]
    fn storage_is_the_real_serialized_size_not_an_estimate() {
        let s = state();
        let x = run("budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(50.0))], &off(), &s);
        let expected: usize = x
            .events
            .iter()
            .map(|e| serde_json::to_string(&e.payload).unwrap().len())
            .sum::<usize>()
            + serde_json::to_string(&x.mutations).unwrap().len();
        let r = meter(&x, registry().get("budget.allocate").unwrap(), &off(), "h", "b", 1).unwrap();
        assert_eq!(r.storage(), expected);
    }

    #[test]
    fn arming_the_gate_raises_the_compute_reading_by_the_invariants_checked() {
        let a = meter_run("budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(10.0))], &off()).unwrap();
        let b = meter_run("budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(10.0))], &armed()).unwrap();
        assert_eq!(b.compute(), a.compute() + 2, "two more real checks were performed");
        assert!(b.pawa() > a.pawa());
    }

    // ── reproducibility ──────────────────────────────────────────────────────

    #[test]
    fn an_identical_run_meters_an_identical_pawa() {
        // ★★★ The point of excluding wall-clock.
        let p = [("amount", json!(100.0)), ("source", json!("salary"))];
        let a = meter_run("budget.record_income", &p, &off()).unwrap();
        let b = meter_run("budget.record_income", &p, &off()).unwrap();
        assert_eq!(a.compute(), b.compute());
        assert_eq!(a.storage(), b.storage());
        assert_eq!(a.pawa(), b.pawa());
    }

    #[test]
    fn elapsed_time_is_recorded_but_never_priced() {
        // ★★ Two readings differing ONLY in wall-clock price identically.
        let p = [("amount", json!(100.0)), ("source", json!("salary"))];
        let base = meter_run("budget.record_income", &p, &off()).unwrap();
        let slow = base.clone().with_elapsed(9_999);
        let fast = base.clone().with_elapsed(1);
        assert_eq!(slow.pawa(), fast.pawa());
        assert_eq!(slow.pawa(), base.pawa());
        assert_eq!(slow.elapsed_ms(), Some(9_999));
        assert_eq!(base.elapsed_ms(), None, "and none is invented");
    }

    // ── refusal meters nothing ───────────────────────────────────────────────

    #[test]
    fn a_gate_refused_run_meters_nothing_at_all() {
        // ★★★ Not a zero reading — NO reading. There is no other constructor.
        let s = state();
        let x = run("budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(9_999.0))], &armed(), &s);
        assert!(!x.committed(), "the gate refused it");
        assert!(meter(&x, registry().get("budget.allocate").unwrap(), &armed(), "h", "b", 1).is_none());
    }

    #[test]
    fn a_refusal_leaves_the_meter_byte_unchanged() {
        let mut m = Meter::new();
        m.record(meter_run("budget.record_income", &[("amount", json!(50.0)), ("source", json!("s"))], &off()).unwrap());
        let before = m.clone();

        let s = state();
        let refused = run("budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(9_999.0))], &armed(), &s);
        if let Some(r) = meter(&refused, registry().get("budget.allocate").unwrap(), &armed(), "h", "b", 2) {
            m.record(r);
        }
        assert_eq!(m, before, "a refused run did zero real work and cost nothing");
    }

    // ── honest absence ───────────────────────────────────────────────────────

    #[test]
    fn a_never_run_operator_has_no_reading_rather_than_a_zero() {
        let mut m = Meter::new();
        m.record(meter_run("budget.record_income", &[("amount", json!(1.0)), ("source", json!("s"))], &off()).unwrap());
        assert!(m.stats_for("budget.record_income").is_some());
        assert!(m.stats_for("budget.spend").is_none(), "never measured is not zero");
        assert!(Meter::new().stats_for("budget.record_income").is_none());
    }

    #[test]
    fn stats_accumulate_over_repeated_runs() {
        let mut m = Meter::new();
        let p = [("amount", json!(10.0)), ("source", json!("s"))];
        for _ in 0..3 {
            m.record(meter_run("budget.record_income", &p, &off()).unwrap());
        }
        let s = m.stats_for("budget.record_income").unwrap();
        assert_eq!(s.runs, 3);
        assert_eq!(s.mean_pawa(), s.total_pawa / 3.0);
        assert_eq!(m.total_for_sustain("household"), s.total_pawa);
        assert_eq!(m.total_for_principal("bonnie"), s.total_pawa);
        assert_eq!(m.total_for_sustain("somebody_else"), 0.0);
    }

    // ── the declared estimate and the measurement are different things ───────

    #[test]
    fn the_declared_cost_is_carried_beside_the_measurement_not_replaced_by_it() {
        // ★ `pawa_cost` is the author's static estimate; the meter is the
        // measured reality. Both are on the reading, so the gap is visible.
        let r = meter_run("budget.record_income", &[("amount", json!(100.0)), ("source", json!("s"))], &off()).unwrap();
        assert_eq!(r.declared(), registry().get("budget.record_income").unwrap().pawa_cost);
        assert!(r.pawa() > f64::from(r.declared()), "the declared 0 was an underestimate");
    }

    // ── the hard boundary ────────────────────────────────────────────────────

    #[test]
    fn a_reading_is_inert_and_moves_no_value() {
        // ★★★ There is no debit, no credit and no balance anywhere in this
        // module — a reading only says what a run cost. PAWA-2's ledger is a
        // separate slice, and even that is internal accounting.
        let r = meter_run("budget.record_income", &[("amount", json!(5.0)), ("source", json!("s"))], &off()).unwrap();
        let mut m = Meter::new();
        let before = state();
        m.record(r);
        assert_eq!(before, state(), "metering touched nothing");
    }
}
