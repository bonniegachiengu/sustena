//! V-as-a-region and `urgency = d(s,V)` replayed against their conformance
//! vectors (R2 · Sustain §V, §VII · Monitor MON-10/11 · R2_BACKLOG #23).
//!
//! **SPEC vectors, Rust-only.** The reference engine has neither a region nor a
//! distance: its `V` is a flat list of boolean invariant strings, and its urgency
//! is `pct = spent / allocated` on one hardcoded pair of fields. `region.json`
//! records that term by term — including the counterweight, which matters here:
//! the `pct` proxy is **not arbitrary**, and the reference is honest about what
//! it is. This slice replaces it with the real quantity; it does not add a
//! competing one.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{Interval, Membership, Region, RegionError, CONFORMANCE_VERSION};

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

/// The shared fixture: a non-negative balance and the article's own fridge band.
fn household() -> Region {
    Region::new()
        .bounding(Interval::at_least("finances.liquid.balance", 0.0))
        .bounding(Interval::new("fridge.temperature", 2.0, 8.0))
        .weighing("finances.liquid.balance", 1.0)
        .weighing("fridge.temperature", 1.0)
}

/// Build a region from a vector's `region_override`, or start from the fixture.
fn region_of(case: &Value) -> Region {
    let mut r = match case.get("region_override") {
        None => household(),
        Some(spec) => {
            let mut r = Region::new();
            for iv in spec["intervals"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                let a = iv.as_array().unwrap();
                let lo = a[1].as_f64().unwrap_or(f64::NEG_INFINITY);
                let hi = a[2].as_f64().unwrap_or(f64::INFINITY);
                r = r.bounding(Interval::new(a[0].as_str().unwrap(), lo, hi));
            }
            for (dim, w) in spec["weights"].as_object().map(|m| m.iter().collect::<Vec<_>>()).unwrap_or_default() {
                r = r.weighing(dim, w.as_f64().unwrap());
            }
            r
        }
    };
    if let Some(ws) = case["weights"].as_object() {
        for (dim, w) in ws {
            r = r.weighing(dim, w.as_f64().unwrap());
        }
    }
    for rel in case["relations"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        r = r.relating(rel.as_str().unwrap());
    }
    r
}

/// A region literal from a `parent` / `child` spec.
fn plain_region(spec: &Value) -> Region {
    let mut r = Region::new();
    for iv in spec["intervals"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let a = iv.as_array().unwrap();
        r = r.bounding(Interval::new(
            a[0].as_str().unwrap(),
            a[1].as_f64().unwrap_or(f64::NEG_INFINITY),
            a[2].as_f64().unwrap_or(f64::INFINITY),
        ));
    }
    for rel in spec["relations"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        r = r.relating(rel.as_str().unwrap());
    }
    r
}

fn err_name(e: &RegionError) -> &'static str {
    match e {
        RegionError::NotANumber { .. } => "NotANumber",
        RegionError::BadRelation { .. } => "BadRelation",
        RegionError::EmptyInterval { .. } => "EmptyInterval",
        RegionError::BadWeight { .. } => "BadWeight",
    }
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("region.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));

    let terms = &doc["divergence"]["term_by_term"];
    for t in [
        "V as a region",
        "urgency = d(s,V)",
        "the allocated<=0 blind spot",
        "declared weights w",
        "three-valued membership + margin",
    ] {
        assert!(terms[t].is_string(), "the '{t}' term must stay accounted for");
    }
    // Two counterweights, both of which keep the claim from overstating itself.
    assert!(
        doc["divergence"]["what_python_does_have_and_why_it_is_not_this"]
            .as_str().unwrap().contains("NOT arbitrary"),
        "the pct-proxy-is-principled note must stay"
    );
    assert!(
        doc["divergence"]["shared_limit_not_a_rust_shortfall"]
            .as_str().unwrap().contains("boolean has no natural distance"),
        "the relations-cannot-be-scored note must stay"
    );
}

#[test]
fn measure_vectors() {
    let doc = load("region.json");
    for case in doc["measure_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_of(case);
        let got = r.distance(&case["state"]);
        let expect = &case["expect"];

        if let Some(want) = expect["error"].as_str() {
            let e = got.expect_err(&format!("{name}: expected a refusal"));
            assert_eq!(err_name(&e), want, "{name}");
            continue;
        }

        let d = got.unwrap_or_else(|e| panic!("{name}: {e}"));
        if let Some(w) = expect["weighted"].as_f64() {
            assert!((d.weighted - w).abs() < 1e-9, "{name}: W = {} want {w}", d.weighted);
        }
        if let Some(z) = expect["is_zero"].as_bool() {
            assert_eq!(d.is_zero(), z, "{name}");
        }
        if expect.get("binding").is_some() {
            assert_eq!(d.binding.as_deref(), expect["binding"].as_str(), "{name}");
        }
    }
}

#[test]
fn projection_vectors() {
    let doc = load("region.json");
    for case in doc["projection_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_of(case);
        let expect = &case["expect"];

        if let Some(want) = expect["undeclared_weights"].as_array() {
            let got = r.undeclared_weights();
            let want: Vec<&str> = want.iter().map(|s| s.as_str().unwrap()).collect();
            assert_eq!(got, want, "{name}: a default-made judgement must be visible");
            assert_eq!(r.weight_for("b"), expect["weight_for_b"].as_f64().unwrap(), "{name}");
            continue;
        }

        let d = r.distance(&case["state"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(d.binding.as_deref(), expect["binding"].as_str(), "{name}");
        if let Some(n) = expect["per_dimension_count"].as_u64() {
            assert_eq!(d.per_dimension.len(), n as usize, "{name}: both are reported, worst first");
        }
    }
}

#[test]
fn relation_vectors() {
    let doc = load("region.json");
    for case in doc["relation_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_of(case);

        // The both-directions table.
        if let Some(table) = case["table"].as_array() {
            for row in table {
                let d = r.distance(&row["state"]).unwrap();
                let viable = row["viable"].as_bool().unwrap();
                assert_eq!(d.is_zero(), viable, "{name}: W = 0 for {}", row["state"]);
                assert_eq!(
                    r.membership(&row["state"]).unwrap().is_viable(), viable,
                    "{name}: membership for {}", row["state"]
                );
            }
            continue;
        }

        let got = r.distance(&case["state"]);
        let expect = &case["expect"];
        if let Some(want) = expect["error"].as_str() {
            let e = got.expect_err(&format!("{name}: an unreadable rule is not a rule that holds"));
            assert_eq!(err_name(&e), want, "{name}");
            continue;
        }

        let d = got.unwrap();
        assert!((d.weighted - expect["weighted"].as_f64().unwrap()).abs() < 1e-9, "{name}");
        assert_eq!(d.relations_violated.len(),
                   expect["relations_violated"].as_u64().unwrap() as usize, "{name}");
        assert_eq!(d.is_zero(), expect["is_zero"].as_bool().unwrap(),
                   "{name}: a zero box distance with a broken relation is NOT W = 0");
        assert_eq!(d.is_exact(), expect["is_exact"].as_bool().unwrap(),
                   "{name}: and `weighted` is only a lower bound here");
    }
}

#[test]
fn membership_vectors() {
    let doc = load("region.json");
    for case in doc["membership_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_of(case);

        if let Some(table) = case["table"].as_array() {
            for row in table {
                let want = match row["membership"].as_str().unwrap() {
                    "interior" => Membership::Interior,
                    "boundary" => Membership::Boundary,
                    "outside" => Membership::Outside,
                    other => panic!("{name}: unknown membership {other}"),
                };
                assert_eq!(r.membership(&row["state"]).unwrap(), want, "{name}: {}", row["state"]);
            }
            continue;
        }

        if case.get("comfortable").is_some() {
            let a = r.margin(&case["comfortable"]).unwrap().expect("in V");
            let b = r.margin(&case["precarious"]).unwrap().expect("in V");
            assert!(a > b, "{name}: both have W = 0; only the margin tells them apart");
            assert!((b - case["expect"]["precarious_margin"].as_f64().unwrap()).abs() < 1e-9, "{name}");
            continue;
        }

        assert_eq!(r.margin(&case["state"]).unwrap(), None,
                   "{name}: outside V the question is the distance back in");
    }
}

#[test]
fn containment_vectors() {
    let doc = load("region.json");
    for case in doc["containment_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let parent = plain_region(&case["parent"]);
        let child = plain_region(&case["child"]);
        let report = child.fits_within_report(&parent);
        let expect = &case["expect"];

        assert_eq!(report.fits, expect["fits"].as_bool().unwrap(), "{name}: {report:?}");
        for (key, got) in [
            ("wider", &report.wider),
            ("unbounded", &report.unbounded),
            ("missing_relations", &report.missing_relations),
        ] {
            if let Some(want) = expect[key].as_array() {
                let want: Vec<String> =
                    want.iter().map(|s| s.as_str().unwrap().to_string()).collect();
                assert_eq!(got, &want, "{name}: {key}");
            }
        }
    }
}

#[test]
fn declaration_vectors() {
    let doc = load("region.json");
    for case in doc["declaration_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_of(case);
        let got = r.typecheck();
        let expect = &case["expect"];

        if expect["well_formed"].as_bool().unwrap() {
            assert_eq!(got, Ok(()), "{name}");
        } else {
            let e = got.expect_err(&format!("{name}: must be a load-time finding"));
            assert_eq!(err_name(&e), expect["error"].as_str().unwrap(), "{name}");
        }
    }
}

#[test]
fn replacement_vectors() {
    let doc = load("region.json");
    for case in doc["replacement_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_of(case);
        let d = r.distance(&case["state"]).unwrap();
        let expect = &case["expect"];

        assert!((d.weighted - expect["weighted"].as_f64().unwrap()).abs() < 1e-9,
                "{name}: the overspend is the distance");
        assert!(!d.is_zero(), "{name}");
        assert_eq!(d.binding.as_deref(), expect["binding"].as_str(), "{name}");

        // The defect this replaces, computed the reference's way so the
        // difference is in the vector rather than only in prose.
        let allocated = 0.0_f64;
        let pct_proxy = if allocated <= 0.0 { 0.0 } else { 1.0 };
        assert_eq!(pct_proxy, expect["pct_proxy_would_say"].as_f64().unwrap(),
                   "{name}: the reference reports an overspent unfunded pocket as maximally calm");
        assert!(d.weighted > pct_proxy, "{name}: the region does not");
    }
}
