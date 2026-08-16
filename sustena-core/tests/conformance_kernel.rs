//! The viability kernel and runway replayed against their conformance vectors
//! (R2 · Sustain §VI · SUS-10, OPV-19).
//!
//! **SPEC vectors, Rust-only.** The Operative article's own Note says it:
//! *"`Viab` / `viability_kernel` — grep-0; no kernel anywhere. Hence no runway:
//! `burn_rate_analysis.json` computes a RATE, not an exit time."*
//! `kernel.json` records that term by term, keeps the counterweight (a burn rate
//! is genuinely useful — it just cannot name a date), and states the scoping of
//! both honest limitations §VI names.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    is_stable_toward_kernel, kernel_margin, runway, viability_kernel, viability_kernel_horizon,
    Change, Interval, Kernel, KernelError, Move, Obligation, Region, Space, CONFORMANCE_VERSION,
};

fn load(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "{name} was written for a different contract version"
    );
    doc
}

fn region_of(spec: &Value) -> Region {
    let mut r = Region::new();
    for iv in spec["intervals"].as_array().unwrap() {
        let a = iv.as_array().unwrap();
        r = r.bounding(Interval::new(
            a[0].as_str().unwrap(),
            a[1].as_f64().unwrap_or(f64::NEG_INFINITY),
            a[2].as_f64().unwrap_or(f64::INFINITY),
        ));
    }
    for (dim, w) in spec["weights"].as_object().unwrap() {
        r = r.weighing(dim, w.as_f64().unwrap());
    }
    r
}

fn space_of(spec: &Value) -> Space {
    spec.as_object().unwrap().iter()
        .map(|(id, v)| (id.clone(), json!({"balance": v.as_f64().unwrap()})))
        .collect()
}

/// Build the named moves from the shared space declaration, plus a synthetic
/// `opaque` for the refusal case.
fn moves_of(space_spec: &Value, names: &[&str]) -> Vec<Move> {
    names.iter().map(|name| {
        if *name == "opaque" {
            return Move::new("opaque").changing("balance", Change::Opaque);
        }
        let m = space_spec["moves"].as_array().unwrap().iter()
            .find(|m| m["name"] == *name)
            .unwrap_or_else(|| panic!("no move '{name}' in the vector"));
        let mut out = Move::new(name);
        for g in m["guard"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            out = out.guarded_by(g.as_str().unwrap());
        }
        out.changing("balance", Change::ShiftBy(m["shift"].as_f64().unwrap()))
    }).collect()
}

fn named(v: &Value) -> Vec<&str> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect()
}

fn ids(v: &Value) -> BTreeSet<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect()
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("kernel.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));

    let terms = &doc["divergence"]["term_by_term"];
    for t in ["Viab_T(V)", "runway R(s)", "the kernel margin", "descent toward the kernel"] {
        assert!(terms[t].is_string(), "the '{t}' term must stay accounted for");
    }
    // The counterweight: a burn rate is genuinely useful, it just cannot name a
    // date. Dropping this would say the reference offers nothing here.
    assert!(
        doc["divergence"]["what_python_does_have_and_why_it_is_not_this"]
            .as_str().unwrap().contains("burn_rate_analysis"),
        "the burn-rate-is-real note must stay"
    );
    // And BOTH of §VI's honest limitations must stay stated.
    let scoping = doc["divergence"]["scoping_stated_plainly"].as_str().unwrap();
    assert!(scoping.contains("exponential"), "limitation (1) must stay");
    assert!(scoping.contains("SUPERSET"), "limitation (2) must stay");
}

#[test]
fn kernel_vectors() {
    let doc = load("kernel.json");
    let sp_spec = &doc["space"];
    let region = region_of(&sp_spec["region"]);
    let space = space_of(&sp_spec["states"]);

    for case in doc["kernel_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let moves = moves_of(sp_spec, &named(&case["moves"]));
        let got = viability_kernel(&region, &space, &moves);
        let expect = &case["expect"];

        if expect["error"].as_str() == Some("OpaqueEffect") {
            assert!(matches!(got, Err(KernelError::OpaqueEffect { .. })),
                    "{name}: a move nobody described cannot support a survival claim");
            continue;
        }

        let k = got.unwrap_or_else(|e| panic!("{name}: {e}"));
        if let Some(want) = expect["is_guarantee"].as_bool() {
            assert_eq!(k.is_guarantee(), want, "{name}");
        }
        if let Some(want) = expect.get("in_kernel").filter(|v| v.is_array()) {
            for id in ids(want) {
                assert!(k.contains(&id), "{name}: {id} should be in the kernel");
            }
        }
        if expect["empty"].as_bool() == Some(true) {
            assert!(k.is_empty(), "{name}: the cascade must empty it — {:?}", k.states());
        }
        // The headline: in V, and not in the kernel.
        if let Some(doomed) = expect.get("in_v_but_not_kernel").filter(|v| v.is_array()) {
            for id in ids(doomed) {
                assert!(region.membership(&space[&id]).unwrap().is_viable(),
                        "{name}: {id} must genuinely be IN V for this to mean anything");
                assert!(!k.contains(&id), "{name}: ...and outside the kernel");
            }
        }
        if let Some(outside) = expect.get("not_in_v").filter(|v| v.is_array()) {
            for id in ids(outside) {
                assert!(!k.contains(&id), "{name}: {id} was never in V");
            }
        }
    }
}

#[test]
fn horizon_vectors() {
    let doc = load("kernel.json");
    let sp_spec = &doc["space"];
    let region = region_of(&sp_spec["region"]);
    let space = space_of(&sp_spec["states"]);

    for case in doc["horizon_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let moves = moves_of(sp_spec, &named(&case["moves"]));
        let expect = &case["expect"];

        if let Some(h) = case["horizon"].as_u64() {
            let k = viability_kernel_horizon(&region, &space, &moves, h as usize).unwrap();
            assert_eq!(k.is_guarantee(), expect["is_guarantee"].as_bool().unwrap(),
                       "{name}: surviving H steps is easier than surviving forever");
            assert!(matches!(k, Kernel::Horizon { .. }), "{name}");
            continue;
        }

        let horizons: Vec<usize> =
            case["horizons"].as_array().unwrap().iter().map(|h| h.as_u64().unwrap() as usize).collect();

        if expect["superset_of_exact"].as_bool() == Some(true) {
            let exact = viability_kernel(&region, &space, &moves).unwrap();
            for h in &horizons {
                let hk = viability_kernel_horizon(&region, &space, &moves, *h).unwrap();
                assert!(exact.states().is_subset(hk.states()),
                        "{name}: H = {h} must be OPTIMISTIC, never pessimistic");
            }
        }
        if expect["longer_is_subset_of_shorter"].as_bool() == Some(true) {
            let short = viability_kernel_horizon(&region, &space, &moves, horizons[0]).unwrap();
            let long = viability_kernel_horizon(&region, &space, &moves, horizons[1]).unwrap();
            assert!(long.states().is_subset(short.states()),
                    "{name}: more looking only removes");
        }
    }
}

#[test]
fn runway_vectors() {
    let doc = load("kernel.json");
    let region = region_of(&doc["space"]["region"]);

    for case in doc["runway_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let start = json!({"balance": case["balance"].as_f64().unwrap()});
        let expect = &case["expect"];

        let mut schedule: Vec<Obligation> = case["schedule"].as_array()
            .map(|a| a.iter().map(|o| {
                Obligation::new(o["at"].as_u64().unwrap() as usize, o["name"].as_str().unwrap())
                    .changing("balance", Change::ShiftBy(o["shift"].as_f64().unwrap()))
            }).collect())
            .unwrap_or_default();
        if let Some(rep) = case.get("repeating").filter(|v| v.is_object()) {
            let n = rep["count"].as_u64().unwrap() as usize;
            let every = rep["every"].as_u64().unwrap() as usize;
            schedule = (0..n).map(|k| {
                Obligation::new(k * every, rep["name"].as_str().unwrap())
                    .changing("balance", Change::ShiftBy(rep["shift"].as_f64().unwrap()))
            }).collect();
        }

        // The "surviving the scan is not a promise" case runs two horizons.
        if let Some(hs) = case["horizons"].as_array() {
            let short = runway(&region, &start, &schedule, hs[0].as_u64().unwrap() as usize).unwrap();
            let long = runway(&region, &start, &schedule, hs[1].as_u64().unwrap() as usize).unwrap();
            assert_eq!(short.survived_the_scan(), expect["short_survives"].as_bool().unwrap(),
                       "{name}: not seen inside the short horizon");
            assert_eq!(long.steps, Some(expect["long_steps"].as_u64().unwrap() as usize),
                       "{name}: but it was there all along");
            continue;
        }

        let horizon = case["horizon"].as_u64().unwrap() as usize;

        // The headline case also pins what the household looks like TODAY, so
        // "fine right now" is measured rather than asserted.
        if let Some(w) = expect["w_today"].as_f64() {
            assert_eq!(region.distance(&start).unwrap().weighted, w, "{name}: W today");
            assert!(region.membership(&start).unwrap().is_viable(), "{name}: in V today");
            assert_eq!(region.margin(&start).unwrap(),
                       Some(expect["box_margin_today"].as_f64().unwrap()),
                       "{name}: and a healthy box margin");
        }

        let out = runway(&region, &start, &schedule, horizon).unwrap();
        match expect["steps"].as_u64() {
            Some(k) => assert_eq!(out.steps, Some(k as usize), "{name}: R(s)"),
            None if expect["steps"].is_null() => {
                assert_eq!(out.steps, None, "{name}");
                if let Some(want) = expect["survived_the_scan"].as_bool() {
                    assert_eq!(out.survived_the_scan(), want, "{name}");
                }
            }
            None => {}
        }
        if let Some(by) = expect["breached_by"].as_str() {
            assert_eq!(out.breached_by.as_deref(), Some(by), "{name}");
        }
        if expect["is_under_estimate"].as_bool() == Some(true) {
            assert!(out.is_under_estimate(),
                    "{name}: a real policy can only last longer, never shorter");
        }
    }
}

#[test]
fn margin_vectors() {
    let doc = load("kernel.json");
    let sp_spec = &doc["space"];
    let region = region_of(&sp_spec["region"]);
    let space = space_of(&sp_spec["states"]);

    for case in doc["margin_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let moves = moves_of(sp_spec, &named(&case["moves"]));
        let k = viability_kernel(&region, &space, &moves).unwrap();
        let id = case["state"].as_str().unwrap();
        let got = kernel_margin(&region, &space, &k, id).unwrap();
        let expect = &case["expect"];

        if expect["kernel_margin"].is_null() {
            assert_eq!(got, None, "{name}: outside the kernel the question is runway, not room");
            continue;
        }

        let kern = got.expect("in the kernel");
        assert!((kern - expect["kernel_margin"].as_f64().unwrap()).abs() < 1e-9, "{name}");

        if let Some(bm) = expect["box_margin"].as_f64() {
            let boxm = region.margin(&space[id]).unwrap().expect("in V");
            assert!((boxm - bm).abs() < 1e-9, "{name}: box margin");
            if expect["kernel_is_tighter"].as_bool() == Some(true) {
                assert!(kern < boxm,
                        "{name}: the box margin was an over-estimate, exactly as declared");
            }
        }
    }
}

#[test]
fn stability_vectors() {
    let doc = load("kernel.json");
    let sp_spec = &doc["space"];
    let region = region_of(&sp_spec["region"]);
    let space = space_of(&sp_spec["states"]);

    for case in doc["stability_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let moves = moves_of(sp_spec, &named(&case["moves"]));
        let k = viability_kernel(&region, &space, &moves).unwrap();
        let before = case["before"].as_str().unwrap();
        let after = case["after"].as_str().unwrap();
        let ks = is_stable_toward_kernel(&k, before, after);
        let expect = &case["expect"];

        assert_eq!(ks.stable(), expect["stable"].as_bool().unwrap(), "{name}");
        assert_eq!(ks.abandons_the_kernel(),
                   expect["abandons_the_kernel"].as_bool().unwrap(), "{name}");

        // The sharpening of CTL-2, measured: the box margin IMPROVES while the
        // move walks out of the kernel.
        if expect["box_margin_improved"].as_bool() == Some(true) {
            let b = region.margin(&space[before]).unwrap().expect("in V");
            let a = region.margin(&space[after]).unwrap().expect("in V");
            assert!((b - expect["box_margin_before"].as_f64().unwrap()).abs() < 1e-9, "{name}");
            assert!((a - expect["box_margin_after"].as_f64().unwrap()).abs() < 1e-9, "{name}");
            assert!(a > b,
                    "{name}: further from V's edge, and still the wrong move — which is exactly \
                     why descent toward the KERNEL is the stronger condition");
        }
    }
}
