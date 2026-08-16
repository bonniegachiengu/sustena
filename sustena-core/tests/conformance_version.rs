//! Version history and rollback replayed against their conformance vectors
//! (R2 · Editing §V · WBD EDIT-7 · N3).
//!
//! **SPEC vectors, Rust-only.** The reference engine does the opposite of this
//! module: `update_definition` runs `UPDATE sustain_templates SET spec_json = ?,
//! version = ?` — an in-place overwrite that destroys the predecessor. There is
//! no history table, no parent pointer, no `D_{n-1}`, and therefore no `e⁻¹` to
//! apply. `version.json` records that term by term, including the breadcrumb the
//! reference *does* keep (an incrementing integer and `updated_at`) so the gap
//! is not overstated.
//!
//! See `conformance/README.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    Definition, DimType, Edit, PreImage, Rollback, Schema, VersionDag, VersionError,
    CONFORMANCE_VERSION,
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

/// The shared fixture schema: one record dimension, `moisture.level`.
fn schema() -> Schema {
    Schema::new().declare(
        "moisture",
        DimType::Record {
            fields: BTreeMap::from([("level".into(), DimType::Number { lo: None, hi: None })]),
        },
    )
}

fn definition_of(spec: &Value) -> Definition {
    let mut d = Definition::new(schema());
    for inv in spec["invariants"].as_array().unwrap() {
        let pair = inv.as_array().unwrap();
        d = d.with_invariant(pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
    }
    for op in spec["operators"].as_array().unwrap() {
        d = d.with_operator(op.as_str().unwrap());
    }
    d
}

fn edit_of(v: &Value) -> Edit {
    match v["kind"].as_str().unwrap() {
        "AddDim" => Edit::AddDim {
            name: v["name"].as_str().unwrap().to_string(),
            ty: DimType::Any,
            default: v.get("default").cloned().unwrap_or(Value::Null),
        },
        "RetypeDim" => Edit::RetypeDim {
            name: v["name"].as_str().unwrap().to_string(),
            ty: DimType::Any,
        },
        "RetireDim" => Edit::RetireDim { name: v["name"].as_str().unwrap().to_string() },
        "AddInv" => Edit::AddInv {
            id: v["id"].as_str().unwrap().to_string(),
            expression: v["expression"].as_str().unwrap().to_string(),
        },
        "DropInv" => Edit::DropInv { id: v["id"].as_str().unwrap().to_string() },
        "ModifyInv" => Edit::ModifyInv {
            id: v["id"].as_str().unwrap().to_string(),
            expression: v["expression"].as_str().unwrap().to_string(),
        },
        "AddOp" => Edit::AddOp { name: v["name"].as_str().unwrap().to_string() },
        "RetireOp" => Edit::RetireOp { name: v["name"].as_str().unwrap().to_string() },
        other => panic!("unknown edit kind in vector: {other}"),
    }
}

/// Build a DAG from a case's genesis + commit list.
fn dag_of(case: &Value) -> VersionDag {
    let mut d = VersionDag::genesis("v1", definition_of(&case["genesis"]), "bonnie", 100);
    for (i, c) in case["commits"].as_array().unwrap().iter().enumerate() {
        let author = c["author"].as_str().unwrap_or("bonnie");
        let t = c["t"].as_u64().unwrap_or(200 + i as u64 * 100);
        let mut pre = PreImage::new();
        if let Some(obj) = c["pre_image"].as_object() {
            for (dim, val) in obj {
                pre = pre.recording(dim, val.clone());
            }
        }
        d.commit_with_pre_image(
            c["id"].as_str().unwrap(),
            c["parent"].as_str().unwrap(),
            edit_of(&c["edit"]),
            pre,
            author,
            t,
        )
        .expect("the vector's commits must be well-formed");
    }
    d
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("version.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));

    let terms = &doc["divergence"]["term_by_term"];
    for t in ["append-only DAG", "e⁻¹ (the document)", "μ⁻¹ (the instances)", "three-valued rollback"] {
        assert!(terms[t].is_string(), "the {t} term must stay accounted for");
    }
    // The μ⁻¹ entry must keep saying that NEITHER engine can restore what was
    // genuinely forgotten — the honest half of a rust-ahead claim.
    assert!(
        terms["μ⁻¹ (the instances)"].as_str().unwrap().contains("Neither engine"),
        "the shared-limitation note must stay, or the claim overstates itself"
    );
    // And the breadcrumb Python does keep.
    assert!(doc["divergence"]["what_python_does_have_and_why_it_is_not_this"]
        .as_str().unwrap().contains("version"));
}

#[test]
fn append_only_vectors() {
    let doc = load("version.json");
    for case in doc["append_only_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let genesis_def = definition_of(&case["genesis"]);
        let d = dag_of(case);
        let expect = &case["expect"];

        if let Some(head) = expect["head"].as_str() {
            assert_eq!(d.head(), head, "{name}");
        }
        if let Some(n) = expect["nodes"].as_u64() {
            assert_eq!(d.len(), n as usize, "{name}");
        }
        if expect["predecessor_unchanged"].as_bool() == Some(true) {
            assert_eq!(
                d.get("v1").unwrap().definition, genesis_def,
                "{name}: the predecessor must survive byte for byte — that IS N3"
            );
        }
        if let Some(n) = expect["current_invariants"].as_u64() {
            assert_eq!(d.current().invariants.len(), n as usize, "{name}");
        }
        if expect["siblings_differ"].as_bool() == Some(true) {
            let a = &d.get("v2").unwrap().definition;
            let b = &d.get("v2b").unwrap().definition;
            assert_ne!(a, b, "{name}: two edits from one parent must be distinct nodes");
            assert_eq!(d.get("v2").unwrap().parent.as_deref(), Some("v1"), "{name}");
            assert_eq!(d.get("v2b").unwrap().parent.as_deref(), Some("v1"), "{name}");
        }
        // The whole tuple ⟨D, parent, e, μ, author, t⟩.
        if let Some(parent) = expect["parent"].as_str() {
            let n = d.get("v2").unwrap();
            assert_eq!(n.parent.as_deref(), Some(parent), "{name}");
            assert_eq!(n.edit.as_ref().unwrap().kind(), expect["edit_kind"].as_str().unwrap(), "{name}");
            assert_eq!(n.author, expect["author"].as_str().unwrap(), "{name}");
            assert_eq!(n.t, expect["t"].as_u64().unwrap(), "{name}");
        }
    }
}

#[test]
fn inverse_vectors() {
    let doc = load("version.json");
    for case in doc["inverse_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let base = definition_of(&case["base"]);

        if let Some(pairs) = case["pairs"].as_array() {
            for pair in pairs {
                let edit = edit_of(&pair["edit"]);
                let inv = edit
                    .inverse(&base)
                    .unwrap_or_else(|e| panic!("{name}: {} could not be inverted: {e}", edit.kind()));
                assert_eq!(
                    inv.kind(),
                    pair["inverse_kind"].as_str().unwrap(),
                    "{name}: {}⁻¹", edit.kind()
                );
            }
        }

        if let Some(edits) = case["round_trip"].as_array() {
            for spec in edits {
                let edit = edit_of(spec);
                let after = edit.apply(&base);
                let restored = edit.inverse(&base).unwrap().apply(&after);
                assert_eq!(restored, base, "{name}: {}⁻¹(e(D)) != D", edit.kind());
            }
        }

        if case["expect"]["invertible"].as_bool() == Some(false) {
            let edit = edit_of(&case["edit"]);
            assert!(edit.inverse(&base).is_err(),
                    "{name}: an inverse with nothing to restore FROM must fail loudly");
        }
    }
}

#[test]
fn rollback_vectors() {
    let doc = load("version.json");
    for case in doc["rollback_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let d = dag_of(case);
        let got = d.rollback(case["rollback"].as_str().unwrap());
        let expect = &case["expect"];

        if let Some(want) = expect["error"].as_str() {
            let err = got.err().unwrap_or_else(|| panic!("{name}: expected an error"));
            let kind = match err {
                VersionError::UnknownVersion(_) => "UnknownVersion",
                VersionError::AtGenesis(_) => "AtGenesis",
                VersionError::NoPath { .. } => "NoPath",
                VersionError::Irreversible(_) => "Irreversible",
            };
            assert_eq!(kind, want, "{name}");
            continue;
        }

        let r = got.unwrap_or_else(|e| panic!("{name}: {e}"));
        match expect["verdict"].as_str().unwrap() {
            "total" => {
                assert!(matches!(r, Rollback::Total { .. }), "{name}: expected Total, got {r:?}");
                assert!(r.delta().is_empty(), "{name}: Total can never carry a Δ");
            }
            "partial" => {
                assert!(matches!(r, Rollback::Partial { .. }), "{name}: expected Partial, got {r:?}");
                let want: Vec<String> = expect["delta"].as_array().unwrap()
                    .iter().map(|s| s.as_str().unwrap().to_string()).collect();
                assert_eq!(r.delta(), want.as_slice(), "{name}: Δ must NAME what is lost");
                assert!(!r.delta().is_empty(), "{name}: Partial with an empty Δ is a contradiction");
            }
            other => panic!("{name}: unknown verdict {other}"),
        }
        if let Some(k) = expect["inverse_kind"].as_str() {
            assert_eq!(r.inverse().unwrap().kind(), k, "{name}: the document half still comes back");
        }
    }
}

#[test]
fn groupoid_vectors() {
    let doc = load("version.json");
    for case in doc["groupoid_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let d = dag_of(case);
        let from = case["from"].as_str().unwrap();
        let to = case["to"].as_str().unwrap();
        let got = d.rollback_to(from, to);
        let expect = &case["expect"];

        if let Some(want) = expect["error"].as_str() {
            let err = got.err().unwrap_or_else(|| panic!("{name}: expected an error"));
            assert!(matches!(err, VersionError::NoPath { .. }), "{name}: expected {want}");
            assert!(d.path_between(from, to).is_none(), "{name}: and no path either");
            if expect["each_reaches_common_ancestor"].as_bool() == Some(true) {
                assert!(d.rollback_to(from, "v1").unwrap().is_total(), "{name}");
                assert!(d.rollback_to(to, "v1").unwrap().is_total(), "{name}");
            }
            continue;
        }

        let r = got.unwrap_or_else(|e| panic!("{name}: {e}"));
        if let Some(total) = expect["total"].as_bool() {
            assert_eq!(r.is_total(), total, "{name}: {:?}", r.unrestorable);
        }
        if let Some(steps) = expect["steps"].as_array() {
            let kinds: Vec<&str> = r.steps.iter().map(|e| e.kind()).collect();
            let want: Vec<&str> = steps.iter().map(|s| s.as_str().unwrap()).collect();
            assert_eq!(kinds, want, "{name}: inverses compose newest-first");
        }
        if let Some(n) = expect["step_count"].as_u64() {
            assert_eq!(r.steps.len(), n as usize, "{name}: every arrow is still reversed");
        }
        if let Some(delta) = expect["delta"].as_array() {
            let want: Vec<String> = delta.iter().map(|s| s.as_str().unwrap().to_string()).collect();
            assert_eq!(r.unrestorable, want, "{name}");
        }
    }
}
