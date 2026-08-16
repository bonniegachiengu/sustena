//! μ and Expand–Migrate–Contract replayed against their conformance vectors
//! (R2 · Editing §VIII · WBD EDIT-12 — the last piece of M-EDIT).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no migration function
//! at all: its classification is binary — compatible, or refused — because
//! "migratable" needs a μ to represent it with. Its discipline is
//! *refuse-if-unsafe*, never *migrate*. `migrate.json` records that term by
//! term, and keeps the counterweight: refuse-if-unsafe **is** a real safety
//! property and is at parity in shape with `Safe(e, id)`. What it lacks is the
//! escape — it can say no, and cannot offer a way through.
//!
//! See `conformance/README.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    safe, Definition, DimType, Edit, Emc, EmcVersionIds, Instance, MigrateError, Migration, Mu,
    Rollback, Schema, VersionDag, CONFORMANCE_VERSION,
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

/// The fixture schema: the OLD shape, `legacy.level`.
fn schema() -> Schema {
    Schema::new().declare(
        "legacy",
        DimType::Record {
            fields: BTreeMap::from([("level".into(), DimType::Number { lo: None, hi: None })]),
        },
    )
}

fn definition_with(invariants: &Value) -> Definition {
    let mut d = Definition::new(schema());
    for inv in invariants.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let pair = inv.as_array().unwrap();
        d = d.with_invariant(pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
    }
    d
}

fn mu_of(v: &Value) -> Mu {
    let mut mu = Mu::new();
    for m in v.as_array().unwrap() {
        if let Some(pair) = m.get("copy") {
            let a = pair.as_array().unwrap();
            mu = mu.copying(a[0].as_str().unwrap(), a[1].as_str().unwrap());
        } else if let Some(pair) = m.get("fill") {
            let a = pair.as_array().unwrap();
            mu = mu.filling(a[0].as_str().unwrap(), a[1].clone());
        } else if let Some(p) = m.get("drop") {
            mu = mu.dropping(p.as_str().unwrap());
        } else {
            panic!("unknown move in vector: {m}");
        }
    }
    mu
}

fn instances_of(v: &Value) -> Vec<Instance> {
    v.as_array().unwrap().iter()
        .map(|i| Instance {
            id: i["id"].as_str().unwrap().to_string(),
            state: i["state"].clone(),
        })
        .collect()
}

fn reshape_of(v: &Value) -> Emc {
    Emc::reshape(
        v["old"].as_str().unwrap(),
        v["new"].as_str().unwrap(),
        DimType::Any,
        json!({"level": 0}),
    )
    .expect("a reshape is a valid plan")
}

fn ids() -> EmcVersionIds<'static> {
    EmcVersionIds { expand: "v2", contract: "v3" }
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("migrate.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));

    let terms = &doc["divergence"]["term_by_term"];
    for t in [
        "μ has a representation",
        "Safe(e, μ) judges μ(sᵢ)",
        "Expand–Migrate–Contract",
        "backward/forward compatibility",
    ] {
        assert!(terms[t].is_string(), "the '{t}' term must stay accounted for");
    }
    // The counterweight: refuse-if-unsafe is real, and is at parity in shape.
    // Dropping it would overstate the gap.
    assert!(
        doc["divergence"]["what_python_does_have_and_why_it_is_not_this"]
            .as_str().unwrap().contains("real safety property"),
        "the refuse-if-unsafe-is-real note must stay"
    );
}

#[test]
fn mu_vectors() {
    let doc = load("migrate.json");
    for case in doc["mu_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mu = mu_of(&case["moves"]);
        let got = mu.apply(&case["state"]);
        let expect = &case["expect"];

        if expect["ok"].as_bool().unwrap() {
            let after = got.unwrap_or_else(|e| panic!("{name}: {e}"));
            for (k, v) in expect["after"].as_object().unwrap() {
                assert_eq!(&after[k], v, "{name}: '{k}' after μ");
            }
        } else {
            let err = got.expect_err(&format!("{name}: μ must refuse"));
            let kind = match err {
                MigrateError::SourceMissing { .. } => "SourceMissing",
                MigrateError::Unapplicable { .. } => "Unapplicable",
                MigrateError::ExpandIsNotAWeakening { .. } => "ExpandIsNotAWeakening",
                MigrateError::ContractStillStrands(_) => "ContractStillStrands",
                MigrateError::Version(_) => "Version",
            };
            assert_eq!(kind, expect["error"].as_str().unwrap(), "{name}");
        }
    }
}

#[test]
fn safe_vectors() {
    let doc = load("migrate.json");
    for case in doc["safe_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let candidate = definition_with(&case["candidate_invariants"]);
        let instances = instances_of(&case["instances"]);
        let mu = mu_of(&case["mu"]);
        let expect = &case["expect"];

        let under_id = safe(&candidate, &instances, &Migration::Identity);
        assert_eq!(
            under_id.is_ok(),
            expect["under_identity"] == "admitted",
            "{name}: under μ = id"
        );

        let under_mu = safe(&candidate, &instances, &Migration::Apply(mu));
        assert_eq!(
            under_mu.is_ok(),
            expect["under_mu"] == "admitted",
            "{name}: under a declared μ — {under_mu:?}"
        );
    }
}

#[test]
fn asymmetry_vectors() {
    let doc = load("migrate.json");
    let cases = doc["asymmetry_cases"].as_array().unwrap();

    // Which kinds can strand, exactly.
    let c = &cases[0];
    let name = c["name"].as_str().unwrap();
    let sample = |kind: &str| -> Edit {
        match kind {
            "AddInv" => Edit::AddInv { id: "x".into(), expression: "a >= 0".into() },
            "ModifyInv" => Edit::ModifyInv { id: "x".into(), expression: "a >= 0".into() },
            "RetypeDim" => Edit::RetypeDim { name: "x".into(), ty: DimType::Any },
            "AddDim" => Edit::AddDim { name: "x".into(), ty: DimType::Any, default: json!(0) },
            "RetireDim" => Edit::RetireDim { name: "x".into() },
            "DropInv" => Edit::DropInv { id: "x".into() },
            "AddOp" => Edit::AddOp { name: "x".into() },
            "RetireOp" => Edit::RetireOp { name: "x".into() },
            other => panic!("{name}: unknown kind {other}"),
        }
    };
    for k in c["can_strand"].as_array().unwrap() {
        let kind = k.as_str().unwrap();
        assert!(sample(kind).can_strand(), "{name}: {kind} must be scanned");
    }
    for k in c["cannot_strand"].as_array().unwrap() {
        let kind = k.as_str().unwrap();
        assert!(!sample(kind).can_strand(),
                "{name}: {kind} cannot strand, so e₊ skips the scan entirely");
    }

    // An expand that is not a weakening is refused at plan time.
    let c = &cases[1];
    let name = c["name"].as_str().unwrap();
    let bad = Emc::plan(
        Edit::AddInv {
            id: c["expand"]["id"].as_str().unwrap().to_string(),
            expression: c["expand"]["expression"].as_str().unwrap().to_string(),
        },
        Mu::new(),
        Edit::RetireDim { name: "legacy".into() },
    );
    assert!(matches!(bad, Err(MigrateError::ExpandIsNotAWeakening { .. })), "{name}");
}

#[test]
fn sequence_vectors() {
    let doc = load("migrate.json");
    for case in doc["sequence_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let base = definition_with(&case["base_invariants"]);
        let instances = instances_of(&case["instances"]);
        let expect = &case["expect"];

        // The step-by-step "never without a shape that holds it" case.
        if case.get("expand").is_some() {
            let emc = Emc::plan(
                Edit::AddDim {
                    name: case["expand"]["name"].as_str().unwrap().to_string(),
                    ty: DimType::Any,
                    default: json!({"level": 0}),
                },
                mu_of(&case["mu"]),
                Edit::RetireOp { name: "nothing".into() },
            )
            .expect("a weakening expand plans");

            assert_eq!(safe(&base, &instances, &Migration::Identity).is_ok(),
                       expect["held_before"].as_bool().unwrap(), "{name}: before");
            let transitional = emc.transitional(&base);
            assert_eq!(safe(&transitional, &instances, &Migration::Identity).is_ok(),
                       expect["held_during"].as_bool().unwrap(), "{name}: during");
            let migrated: Vec<Instance> = instances.iter()
                .map(|i| Instance { id: i.id.clone(), state: emc.mu().apply(&i.state).unwrap() })
                .collect();
            assert_eq!(safe(&transitional, &migrated, &Migration::Identity).is_ok(),
                       expect["held_after_mu"].as_bool().unwrap(), "{name}: after μ");
            continue;
        }

        let mut dag = VersionDag::genesis("v1", base, "bonnie", 100);
        let emc = reshape_of(&case["reshape"]);
        let got = emc.run(&mut dag, &instances, ids(), "bonnie", 200);

        if !expect["ran"].as_bool().unwrap() {
            let err = got.expect_err(&format!("{name}: this sequence must be refused"));
            assert!(matches!(err, MigrateError::ContractStillStrands(_)), "{name}: {err}");
            continue;
        }

        let applied = got.unwrap_or_else(|e| panic!("{name}: {e}"));
        if let Some(v) = expect["transitional_version"].as_str() {
            assert_eq!(applied.transitional_version, v, "{name}");
        }
        if let Some(v) = expect["final_version"].as_str() {
            assert_eq!(applied.final_version, v, "{name}");
        }
        if let Some(levels) = expect["migrated_levels"].as_array() {
            for (i, want) in levels.iter().enumerate() {
                assert_eq!(&applied.instances[i].state["moisture"]["level"], want, "{name}: instance {i}");
            }
        }
        // The transition period, recorded rather than collapsed.
        if let Some(has) = expect["transitional_has"].as_array() {
            let d = &dag.get("v2").unwrap().definition;
            for n in has {
                assert!(d.schema.top_level().any(|x| x == n.as_str().unwrap()),
                        "{name}: D† must still declare {n}");
            }
        }
        if let Some(lacks) = expect["final_lacks"].as_array() {
            let d = &dag.get("v3").unwrap().definition;
            for n in lacks {
                assert!(!d.schema.top_level().any(|x| x == n.as_str().unwrap()),
                        "{name}: and only THEN is {n} removed");
            }
        }
        if let Some(has) = expect["final_has"].as_array() {
            let d = &dag.get("v3").unwrap().definition;
            for n in has {
                assert!(d.schema.top_level().any(|x| x == n.as_str().unwrap()), "{name}");
            }
        }
    }
}

#[test]
fn compatibility_vectors() {
    let doc = load("migrate.json");
    for case in doc["compatibility_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let emc = Emc::plan(
            Edit::AddDim { name: "moisture".into(), ty: DimType::Any, default: json!(0) },
            mu_of(&case["mu"]),
            Edit::RetireOp { name: "x".into() },
        )
        .expect("plans");
        let c = emc.compatibility();
        let expect = &case["expect"];

        assert_eq!(c.backward, expect["backward"].as_bool().unwrap(), "{name}: backward");
        assert_eq!(c.forward, expect["forward"].as_bool().unwrap(), "{name}: forward");
        assert_eq!(c.is_one_way(), expect["one_way"].as_bool().unwrap(), "{name}: one-way");
    }
}

#[test]
fn knock_on_to_edit7_vectors() {
    let doc = load("migrate.json");
    for case in doc["knock_on_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let expect = &case["expect"];

        if let Some(mu_spec) = case.get("mu").filter(|_| case.get("state").is_some()) {
            let pre = mu_of(mu_spec).pre_image_of(&case["state"]);
            for (dim, want) in expect["journalled"].as_object().unwrap() {
                assert_eq!(pre.dimension_defaults.get(dim), Some(want),
                           "{name}: a lookup, not an inference");
            }
            continue;
        }

        let instances = instances_of(&case["instances"]);

        // Baseline: a bare RetireDim of a genesis dimension.
        let mut plain = VersionDag::genesis("v1", Definition::new(schema()), "bonnie", 100);
        plain.commit("v2", "v1", Edit::RetireDim { name: "legacy".into() }, "bonnie", 200).unwrap();
        let bare = plain.rollback("v2").unwrap();
        assert_eq!(
            matches!(bare, Rollback::Partial { .. }),
            expect["bare_retire_rollback"] == "partial",
            "{name}: baseline — nothing recorded what was lost"
        );

        // Through EMC: μ journals what it erased.
        let mut dag = VersionDag::genesis("v1", Definition::new(schema()), "bonnie", 100);
        reshape_of(&case["reshape"]).run(&mut dag, &instances, ids(), "bonnie", 200).unwrap();
        let after = dag.rollback("v3").unwrap();
        assert_eq!(
            after.is_total(),
            expect["emc_rollback"] == "total",
            "{name}: a μ that journals its pre-image is losslessly reversible — {after:?}"
        );
    }
}
