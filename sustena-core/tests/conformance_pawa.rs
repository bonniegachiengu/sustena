//! The pawa meter, replayed against its conformance vectors
//! (R1-parity + Rust wiring · Pawa §1 · PAWA-1).
//!
//! ★★★ **The hard boundary (ADR-0001 D5):** everything in the economy layer is
//! **internal accounting on a single host**. Juul is a utility unit — never real
//! money, never a real transfer, never a payment rail. This module is the
//! easiest place to keep that promise, because it has **no balance at all**.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    operator::{execute, Enforcement, Execution, Registry},
    governance::Parameters,
    pawa::{compute_units, constraint_eval_count, meter, Meter, PawaReading},
    schema::{DimType, Schema},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("pawa.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "pawa.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

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
            (
                "pocket_allocated_non_negative".into(),
                "ALL finances.pockets[*].allocated >= 0".into(),
            ),
        ],
        schema: Some(Schema::new().declare("finances", DimType::Any)),
        ..Enforcement::default()
    }
}

fn income() -> Vec<(&'static str, Value)> {
    vec![("amount", json!(100.0)), ("source", json!("salary"))]
}

fn run(op: &str, p: &[(&str, Value)], e: &Enforcement) -> Execution {
    execute(&registry(), &allowed(), e, &state(), op, &params(p))
}

fn meter_run(op: &str, p: &[(&str, Value)], e: &Enforcement) -> Option<PawaReading> {
    let x = run(op, p, e);
    meter(&x, registry().get(op).unwrap(), e, &Parameters::genesis(), "household", "bonnie", 1_000)
}

// ── the formula ──────────────────────────────────────────────────────────────

#[test]
fn the_formula_is_kappa_c_compute_plus_kappa_s_storage() {
    assert_eq!(Parameters::genesis().price(7.0, 542.0), 12.42);
    assert_eq!(compute_units(3, 1, 2), 6);
}

#[test]
fn a_sustains_invariants_count_only_when_the_gate_actually_ran() {
    let reg = registry();
    let meta = reg.get("budget.allocate").unwrap();
    let declared = meta.constraints.len() + meta.post_constraints.len();
    assert_eq!(constraint_eval_count(meta, &off()), declared);
    assert_eq!(constraint_eval_count(meta, &armed()), declared + 2);
}

#[test]
fn storage_is_the_real_serialized_size_not_an_estimate() {
    let x = run("budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(50.0))], &off());
    let expected: usize = x
        .events
        .iter()
        .map(|e| serde_json::to_string(&e.payload).unwrap().len())
        .sum::<usize>()
        + serde_json::to_string(&x.mutations).unwrap().len();
    let r = meter(&x, registry().get("budget.allocate").unwrap(), &off(), &Parameters::genesis(), "h", "b", 1).unwrap();
    assert_eq!(r.storage(), expected);
}

#[test]
fn arming_the_gate_raises_the_compute_reading_by_the_invariants_checked() {
    let p = [("pocket_name", json!("food")), ("amount", json!(10.0))];
    let a = meter_run("budget.allocate", &p, &off()).unwrap();
    let b = meter_run("budget.allocate", &p, &armed()).unwrap();
    assert_eq!(b.compute(), a.compute() + 2, "two more real checks happened");
    assert!(b.pawa() > a.pawa());
}

// ── measurement ──────────────────────────────────────────────────────────────

#[test]
fn a_real_run_is_measured_from_what_it_actually_did() {
    let r = meter_run("budget.record_income", &income(), &off()).expect("it committed");
    assert_eq!(r.operator(), "budget.record_income");
    assert_eq!(r.sustain(), "household");
    assert_eq!(r.principal(), "bonnie");
    assert_eq!(r.at(), 1_000, "host-supplied; the core has no clock");
    assert!(r.compute() > 0);
    assert!(r.storage() > 0);
    assert_eq!(r.pawa(), Parameters::genesis().price(r.compute() as f64, r.storage() as f64));
}

#[test]
fn an_identical_run_meters_an_identical_pawa() {
    // ★★★ Reproducibility — the whole reason wall-clock is excluded.
    let a = meter_run("budget.record_income", &income(), &off()).unwrap();
    let b = meter_run("budget.record_income", &income(), &off()).unwrap();
    assert_eq!(a.compute(), b.compute());
    assert_eq!(a.storage(), b.storage());
    assert_eq!(a.pawa(), b.pawa());
}

#[test]
fn elapsed_time_is_recorded_but_never_priced() {
    let base = meter_run("budget.record_income", &income(), &off()).unwrap();
    assert_eq!(base.elapsed_ms(), None, "and none is invented");
    let slow = base.clone().with_elapsed(9_999);
    let fast = base.clone().with_elapsed(1);
    assert_eq!(slow.pawa(), fast.pawa());
    assert_eq!(slow.pawa(), base.pawa());
    assert_eq!(slow.elapsed_ms(), Some(9_999));
}

#[test]
fn the_declared_cost_is_carried_beside_the_measurement_not_replaced_by_it() {
    // ★ `pawa_cost` is the author's static estimate; the meter is the measured
    // reality. Both ride on the reading, so the gap is a readable finding.
    let r = meter_run("budget.record_income", &income(), &off()).unwrap();
    assert_eq!(r.declared(), registry().get("budget.record_income").unwrap().pawa_cost);
    assert!(r.pawa() > f64::from(r.declared()), "the declared 0 was an underestimate");
}

// ── refusal meters nothing ───────────────────────────────────────────────────

#[test]
fn a_gate_refused_run_meters_nothing_at_all() {
    // ★★★ Not a zero reading — NO reading, and no other constructor exists.
    let x = run("budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(9_999.0))], &armed());
    assert!(!x.committed(), "the gate refused it");
    assert!(meter(&x, registry().get("budget.allocate").unwrap(), &armed(), &Parameters::genesis(), "h", "b", 1).is_none());
}

#[test]
fn a_refusal_leaves_the_meter_byte_unchanged() {
    let mut m = Meter::new();
    m.record(meter_run("budget.record_income", &income(), &off()).unwrap());
    let before = m.clone();

    let refused = run(
        "budget.allocate",
        &[("pocket_name", json!("food")), ("amount", json!(9_999.0))],
        &armed(),
    );
    if let Some(r) = meter(&refused, registry().get("budget.allocate").unwrap(), &armed(), &Parameters::genesis(), "h", "b", 2) {
        m.record(r);
    }
    assert_eq!(m, before, "a refused run did zero real work and cost nothing");
}

#[test]
fn a_never_run_operator_has_no_reading_rather_than_a_zero() {
    let mut m = Meter::new();
    m.record(meter_run("budget.record_income", &income(), &off()).unwrap());
    assert!(m.stats_for("budget.record_income").is_some());
    assert!(m.stats_for("budget.spend").is_none(), "never measured is not zero");
    assert!(Meter::new().stats_for("budget.record_income").is_none());
}

// ── accumulation and the hard boundary ───────────────────────────────────────

#[test]
fn stats_accumulate_over_repeated_runs() {
    let mut m = Meter::new();
    for _ in 0..3 {
        m.record(meter_run("budget.record_income", &income(), &off()).unwrap());
    }
    let s = m.stats_for("budget.record_income").unwrap();
    assert_eq!(s.runs, 3);
    assert_eq!(s.mean_pawa(), s.total_pawa / 3.0);
    assert_eq!(m.total_for_sustain("household"), s.total_pawa);
    assert_eq!(m.total_for_principal("bonnie"), s.total_pawa);
    // ★ A sum over an empty set, not a claim about an unmeasured operator.
    assert_eq!(m.total_for_sustain("somebody_else"), 0.0);
    assert_eq!(m.by_operator().len(), 1);
}

#[test]
fn a_reading_is_inert_and_moves_no_value() {
    // ★★★ No debit, no credit, no balance anywhere in this module.
    let before = state();
    let mut m = Meter::new();
    m.record(meter_run("budget.record_income", &income(), &off()).unwrap());
    assert_eq!(before, state(), "metering touched nothing");
    assert_eq!(m.len(), 1);
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "parity-plus-rust-wiring");

    // ★★★ The hard boundary must stay stated, at the top level.
    let hb = doc["★★★_the_hard_boundary_and_it_is_ADR_0001_D5"].as_str().unwrap();
    assert!(hb.contains("INTERNAL ACCOUNTING"));
    assert!(hb.contains("DISTINCT, LATER, HUMAN-AUTHORISED ACT"));
    assert!(hb.contains("never a payment rail"));
    assert!(hb.contains("NO BALANCE AT ALL"));

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_parity_with_the_Python_meter_term_by_term",
        "★★★_2_the_DECLARED_cost_and_the_MEASURED_one_are_different_things_and_stay_apart",
        "★★_3_what_is_RUST_WIRING_rather_than_parity",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    let split = step0
        ["★★★_2_the_DECLARED_cost_and_the_MEASURED_one_are_different_things_and_stay_apart"]
        .as_str()
        .unwrap();
    assert!(split.contains("pawa_cost"), "the static side must stay named");
    assert!(split.contains("BESIDE"), "and the two must stay carried together");

    let strengthening = d["★★★_the_structural_strengthening_over_the_reference"].as_str().unwrap();
    assert!(strengthening.contains("NO PUBLIC CONSTRUCTOR"));
    assert!(strengthening.contains("unconstructible"));

    let kappa = d["★★_the_kappa_honesty_carried_verbatim"].as_str().unwrap();
    assert!(kappa.contains("HAVE NOT BEEN FITTED"));
    assert!(kappa.contains("No calibration was invented"));

    let terms = d["term_by_term"].as_object().unwrap();
    for k in ["pawa", "compute", "storage", "constraint_eval_count", "the reading"] {
        assert!(terms.contains_key(k), "term_by_term is missing {k}");
    }

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "κ IS DECLARED AND UNCALIBRATED",
        "NOTHING HERE TOUCHES A BALANCE",
        "THE TIMESTAMP IS HOST-SUPPLIED",
        "`Meter` IS IN-MEMORY",
        "THE COMPUTE PROXY IS A COUNT, NOT A COST MODEL",
        "NOTHING CALLS `meter()` FROM `execute_admitted` YET",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["formula_cases", "measurement_cases", "refusal_cases", "boundary_cases"] {
        for c in doc[group].as_array().unwrap_or_else(|| panic!("{group} is an array")) {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 13, "every declared case must be present");
}
