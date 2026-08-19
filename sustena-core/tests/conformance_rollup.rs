//! Roll-up ρ replayed against its conformance vectors
//! (R2 · Composition §VIII · the reference's `compute_rollup`).
//!
//! **PARITY vectors (R1).** Unlike most of this suite, ρ genuinely exists in
//! the reference engine — `sustain_engine.py::_aggregate_from_child_states` —
//! so `rollup.json` is *recorded from it* rather than written from the spec.
//! Every case below asserts the Rust core answers what Python answered: the
//! same combined value, the same contributors included with the same numbers,
//! and the same contributors excluded.
//!
//! ★★ The exclusions are asserted as hard as the values. A roll-up that got the
//! arithmetic right while silently dropping a member would pass a value-only
//! check and be exactly the failure ρ exists to prevent, so every case compares
//! the included and excluded sets by identity.
//!
//! ★ **Two recorded divergences, both narrow, both leaning Rust** — see
//! `conformance/README.md`:
//!
//! 1. **Whole floats render as integers.** The wildcard form seeds its inner
//!    sum with Python's `0.0`, so the reference reports `28200.0` where this
//!    core reports `28200`. The crate normalises every number it produces
//!    through one rule; the *values* are equal, so this file compares numbers
//!    numerically rather than by JSON token.
//! 2. **An array container is readable under a wildcard.** Python's roll-up
//!    accepts only a dict there and excludes anything else; this core accepts a
//!    dict or an array, because [`resolve_path`] is the same resolver `V` uses
//!    and a wildcard that meant two different things in a rule and in a total
//!    would be the two-evaluator divergence this core deleted. No vector
//!    exercises it — Python has no recorded answer to compare against — so it
//!    is stated here rather than smuggled past a green suite.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    compute_rollup, AggregateDecl, AggregateReading, ChildState, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("rollup.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "rollup.json was written for a different contract version"
    );
    doc
}

/// The reference's parent id in the generator. Only ever compared against
/// itself, so it is a fixture rather than a claim.
const HOUSEHOLD: &str = "homestead";

fn opt_str(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// Numeric equality across the int/float rendering divergence (see the header).
fn same_number(rust: &Value, python: &Value) -> bool {
    match (rust.as_f64(), python.as_f64()) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-9,
        _ => rust == python,
    }
}

fn ids_of(reading: &AggregateReading) -> (BTreeSet<String>, BTreeSet<String>) {
    (
        reading
            .included()
            .iter()
            .map(|c| c.contributor().sustain_id().to_string())
            .collect(),
        reading
            .excluded()
            .iter()
            .map(|e| e.contributor().sustain_id().to_string())
            .collect(),
    )
}

#[test]
fn every_recorded_case_matches_the_reference() {
    let doc = load();
    let cases = doc["cases"].as_array().expect("cases is a list");
    assert!(!cases.is_empty(), "the vector file recorded nothing");

    for case in cases {
        let name = case["name"].as_str().unwrap();

        let declared: Vec<AggregateDecl> = case["aggregates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                AggregateDecl::declare(
                    a["id"].as_str().unwrap(),
                    a["child_path"].as_str().unwrap(),
                    a["op"].as_str().unwrap(),
                )
                .unwrap_or_else(|e| panic!("{name}: the reference declared {a}, which this core refused: {e}"))
            })
            .collect();

        let states = &case["child_states"];
        let children: Vec<ChildState> = case["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let id = l["child_sustain_id"].as_str().unwrap();
                let mut c = match states.get(id) {
                    // ★ The reference's `child_states` map simply has no entry
                    //   for a child whose state could not be read; that absence
                    //   IS the unreadable case.
                    Some(s) if !s.is_null() => ChildState::readable(id, s.clone()),
                    _ => ChildState::unreadable(id),
                };
                if let Some(slot) = opt_str(&l["slot"]) {
                    c = c.in_slot(&slot);
                }
                if let Some(member) = opt_str(&l["member"]) {
                    c = c.named(&member);
                }
                c
            })
            .collect();

        let own = case["own_state"].as_object().map(|_| &case["own_state"]);
        let rollup = compute_rollup(&declared, HOUSEHOLD, own, &children);

        let expect = case["expect"].as_object().unwrap();
        assert_eq!(
            rollup.aggregates().len(),
            expect.len(),
            "{name}: answered a different number of aggregates"
        );

        for (agg_id, want) in expect {
            let got = rollup
                .aggregate(agg_id)
                .unwrap_or_else(|| panic!("{name}: no reading for '{agg_id}'"));

            assert!(
                same_number(got.value(), &want["value"]),
                "{name}/{agg_id}: value {} != reference {}",
                got.value(),
                want["value"]
            );
            assert_eq!(
                got.child_path(),
                want["child_path"].as_str().unwrap(),
                "{name}/{agg_id}: child_path"
            );
            assert_eq!(
                got.includes_household_own(),
                want["includes_parent_own_contribution"].as_bool().unwrap(),
                "{name}/{agg_id}: whether the household's own state was offered"
            );

            // ★★ Who was counted, and who was not — by identity, not by count.
            let (got_in, got_out) = ids_of(got);
            let want_in: BTreeSet<String> = want["included"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["sustain_id"].as_str().unwrap().to_string())
                .collect();
            let want_out: BTreeSet<String> = want["excluded"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["sustain_id"].as_str().unwrap().to_string())
                .collect();
            assert_eq!(got_in, want_in, "{name}/{agg_id}: included set");
            assert_eq!(got_out, want_out, "{name}/{agg_id}: excluded set");

            // Each contributor's own number, not just the total.
            for want_c in want["included"].as_array().unwrap() {
                let id = want_c["sustain_id"].as_str().unwrap();
                let got_c = got
                    .included()
                    .iter()
                    .find(|c| c.contributor().sustain_id() == id)
                    .unwrap();
                assert!(
                    (got_c.value() - want_c["value"].as_f64().unwrap()).abs() < 1e-9,
                    "{name}/{agg_id}: {id} contributed {} not {}",
                    got_c.value(),
                    want_c["value"]
                );
            }

            // The household's own contribution is identified as such.
            for c in got.included() {
                let is_household = c.contributor().is_household();
                let want_self = want["included"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|w| w["sustain_id"].as_str().unwrap() == c.contributor().sustain_id())
                    .and_then(|w| w["is_self"].as_bool())
                    .unwrap_or(false);
                assert_eq!(
                    is_household, want_self,
                    "{name}/{agg_id}: {} tagged as the household's own?",
                    c.contributor().sustain_id()
                );
            }
        }
    }
}

#[test]
fn the_real_household_totals_match_a_hand_sum() {
    let doc = load();
    let case = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "the_real_household_shape")
        .expect("the household case is recorded");

    // Hand-summed from the vector's own inputs, independently of both engines.
    let mut hand = case["own_state"]["finances"]["liquid"]["balance"]
        .as_f64()
        .unwrap();
    for (_, st) in case["child_states"].as_object().unwrap() {
        hand += st["finances"]["liquid"]["balance"].as_f64().unwrap();
    }

    let reference = case["expect"]["household_liquid_total"]["value"]
        .as_f64()
        .unwrap();
    assert_eq!(
        hand, reference,
        "the reference's own recorded total does not match a hand sum"
    );
}

#[test]
fn a_recomputation_reproduces_itself() {
    let doc = load();
    let case = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "household_plus_children")
        .unwrap();

    let declared: Vec<AggregateDecl> = case["aggregates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            AggregateDecl::declare(
                a["id"].as_str().unwrap(),
                a["child_path"].as_str().unwrap(),
                a["op"].as_str().unwrap(),
            )
            .unwrap()
        })
        .collect();
    let children: Vec<ChildState> = case["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let id = l["child_sustain_id"].as_str().unwrap();
            ChildState::readable(id, case["child_states"][id].clone())
        })
        .collect();

    let a = compute_rollup(&declared, HOUSEHOLD, Some(&case["own_state"]), &children);
    let b = compute_rollup(&declared, HOUSEHOLD, Some(&case["own_state"]), &children);
    assert_eq!(a, b, "ρ is fresh every call and must reproduce itself");
}
