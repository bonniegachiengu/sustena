//! The Rust core replayed against the shared conformance vectors.
//!
//! These files were recorded from the reference Python engine and are replayed
//! there too (`apps/api/tests/test_conformance_vectors.py`). Passing both is
//! what "parity" means — measured against recorded behaviour, not against
//! someone's reading of it.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    diff_to_mutations, fold_events, FoldEvent, Mutation, State, StateError, CONFORMANCE_VERSION,
};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
}

fn load(name: &str) -> Value {
    let path = vectors_dir().join(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing {}: {e}\nrun conformance/generate.py", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "{name} was written for a different contract version"
    );
    doc
}

/// Outcome of one operation, in the vectors' own vocabulary.
enum OpOutcome {
    Ok(Value),
    Err { kind: &'static str },
}

fn err_kind(e: &StateError) -> &'static str {
    match e {
        StateError::Path(_) => "path",
        StateError::Value(_) => "value",
    }
}

fn run_ops(initial: &Value, ops: &[Value]) -> (State, Vec<OpOutcome>) {
    let mut state = State::new(initial.clone());
    let mut out = Vec::new();

    for op in ops {
        let kind = op["op"].as_str().expect("op has a kind");
        let path = op["path"].as_str().unwrap_or_default();

        let outcome = match kind {
            "set" => state
                .set(path, op["value"].clone())
                .map(|_| Value::Null)
                .map_err(|e| e),
            "increment" => state
                .increment(path, op["delta"].as_f64().unwrap())
                .map(num_to_value),
            "decrement" => state
                .decrement(
                    path,
                    op["delta"].as_f64().unwrap(),
                    op["allow_negative"].as_bool().unwrap_or(false),
                )
                .map(num_to_value),
            "append" => {
                let id = op["id"].as_str().unwrap_or_default();
                state
                    .append(path, op["item"].clone(), id)
                    .map(Value::String)
            }
            "remove" => state
                .remove(path, op["item_id"].as_str().unwrap_or_default())
                .map(|_| Value::Null),
            "get" => Ok(state.get(path).cloned().unwrap_or(Value::Null)),
            "get_strict" => state.get_strict(path).cloned(),
            "exists" => Ok(Value::Bool(state.exists(path))),
            other => panic!("unknown op in vector: {other}"),
        };

        out.push(match outcome {
            Ok(v) => OpOutcome::Ok(v),
            Err(e) => OpOutcome::Err { kind: err_kind(&e) },
        });
    }
    (state, out)
}

/// Whole floats render as integers, matching how both engines emit them.
fn num_to_value(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        json!(v as i64)
    } else {
        json!(v)
    }
}

fn wire(mutations: &[Mutation]) -> Value {
    Value::Array(mutations.iter().map(|m| m.to_wire()).collect())
}

#[test]
fn state_vectors() {
    let doc = load("state.json");
    let cases = doc["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "no state vectors to check");

    let mut checked = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let ops = case["ops"].as_array().unwrap();
        let expect = &case["expect"];

        let (state, results) = run_ops(&case["initial"], ops);

        assert_eq!(
            state.snapshot(),
            expect["snapshot"],
            "{name}: resulting state differs"
        );
        assert_eq!(
            wire(state.mutations()),
            expect["mutations"],
            "{name}: mutation records differ"
        );
        assert_eq!(
            wire(&state.reconciled_mutations()),
            expect["reconciled_mutations"],
            "{name}: reconciled mutations differ"
        );

        let want = expect["results"].as_array().unwrap();
        assert_eq!(results.len(), want.len(), "{name}: result count differs");

        for (i, (got, wanted)) in results.iter().zip(want).enumerate() {
            let wanted_err = wanted.get("error");
            match (got, wanted_err) {
                (OpOutcome::Ok(v), None) => assert_eq!(
                    v, &wanted["ok"],
                    "{name}: op {i} return value differs"
                ),
                (OpOutcome::Err { kind }, Some(e)) => assert_eq!(
                    *kind,
                    e["kind"].as_str().unwrap(),
                    "{name}: op {i} failed for a different reason"
                ),
                (OpOutcome::Ok(v), Some(e)) => panic!(
                    "{name}: op {i} succeeded with {v} but the reference failed ({})",
                    e["kind"]
                ),
                (OpOutcome::Err { kind }, None) => {
                    panic!("{name}: op {i} failed ({kind}) but the reference succeeded")
                }
            }
        }
        checked += 1;
    }
    eprintln!("state vectors: {checked} cases matched the reference engine");
}

#[test]
fn fold_vectors() {
    let doc = load("fold.json");
    let cases = doc["fold_cases"].as_array().expect("fold_cases array");
    assert!(!cases.is_empty(), "no fold vectors to check");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let events: Vec<FoldEvent> =
            serde_json::from_value(case["events"].clone()).expect("events parse");
        let expect = &case["expect"];

        let result = fold_events(&events, Some(case["initial"].clone()));

        if expect.get("error").is_some() {
            assert!(
                result.is_err(),
                "{name}: the reference failed to replay this log; silently \
                 returning wrong state would defeat the fold"
            );
        } else {
            assert_eq!(
                result.expect("fold should succeed"),
                expect["state"],
                "{name}: folded state differs"
            );
        }
    }
}

#[test]
fn diff_vectors() {
    let doc = load("fold.json");
    let cases = doc["diff_cases"].as_array().expect("diff_cases array");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let before = &case["before"];
        let after = &case["after"];

        let muts = diff_to_mutations(before, after, "");
        assert_eq!(
            wire(&muts),
            case["expect"]["mutations"],
            "{name}: derived mutations differ"
        );

        // The property that actually matters: the patch reproduces `after`.
        let replayed = fold_events(
            &[FoldEvent { mutations: muts }],
            Some(before.clone()),
        )
        .expect("a derived patch must always replay");
        assert_eq!(&replayed, after, "{name}: patch does not reproduce the target");
    }
}

#[test]
fn folding_a_log_never_mutates_its_source() {
    // Regression lock for the aliasing defect the vectors uncovered in the
    // reference engine: replaying a log mutated the state it was rebuilt from.
    // Rust's borrow rules make it unrepresentable, but the property is asserted
    // rather than assumed.
    let mut state = State::empty();
    state
        .append("todo.items", json!({"kind":"task"}), "i1")
        .unwrap();

    let before = state.snapshot();
    let replayed = fold_events(
        &[FoldEvent { mutations: state.mutations().to_vec() }],
        Some(json!({})),
    )
    .unwrap();

    assert_eq!(state.snapshot(), before, "folding corrupted its source");
    assert_eq!(replayed, before, "the log does not reproduce its own state");
    assert_eq!(
        replayed["todo"]["items"].as_array().unwrap().len(),
        1,
        "the item was applied twice"
    );
}

#[test]
fn rule_vectors() {
    let doc = load("rules.json");
    let cases = doc["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "no rule vectors to check");

    let mut matched = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expr = case["expr"].as_str().unwrap();
        let state = &case["state"];
        let params: serde_json::Map<String, Value> = case["params"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let expect = &case["expect"];

        match sustena_core::check(expr, state, &params) {
            Err(_) => assert!(
                expect.get("parse_error").is_some(),
                "{name}: this core rejected an expression the reference accepted"
            ),
            Ok((verdict, reason)) => {
                assert!(
                    expect.get("parse_error").is_none(),
                    "{name}: this core accepted an expression the reference rejected"
                );
                assert_eq!(
                    verdict,
                    expect["verdict"].as_bool().unwrap(),
                    "{name}: verdict differs (reason given: {reason})"
                );
            }
        }
        matched += 1;
    }
    eprintln!("rule vectors: {matched} cases matched the reference engine");
}
