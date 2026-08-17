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
            "increment" => state.increment(path, &op["delta"]),
            "decrement" => state.decrement(
                path,
                &op["delta"],
                op["allow_negative"].as_bool().unwrap_or(false),
            ),
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

#[test]
fn operator_vectors() {
    use sustena_core::operator::{execute, Enforcement, Registry};

    let doc = load("operators.json");
    let cases = doc["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "no operator vectors to check");

    let registry = Registry::default();

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let allowed: Vec<String> = case["allowed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        let enf = Enforcement {
            enabled: case["enforcement"]["enabled"].as_bool().unwrap_or(false),
            invariants: case["enforcement"]["invariants"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|i| {
                            (
                                i["id"].as_str().unwrap().to_string(),
                                i["expression"].as_str().unwrap().to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            // The R1 operator vectors predate typed state, so closure is not
            // enforced for them — they must keep passing unchanged. Transition
            // constraints are R2 for the same reason: the reference engine has
            // no D, so no R1 vector can carry one.
            schema: None,
            boundary: None,
            firewall: vec![],
            transitions: vec![],
        };

        let mut state = case["initial"].clone();
        let calls = case["calls"].as_array().unwrap();
        let expected = case["expect"]["outcomes"].as_array().unwrap();
        assert_eq!(calls.len(), expected.len(), "{name}: malformed vector");

        for (i, call) in calls.iter().enumerate() {
            let op = call["operator"].as_str().unwrap();
            let params: serde_json::Map<String, Value> =
                call["params"].as_object().cloned().unwrap_or_default();

            let ex = execute(&registry, &allowed, &enf, &state, op, &params);
            let want = &expected[i];
            let want_ok = want["status"] == "ok";

            assert_eq!(
                ex.committed(),
                want_ok,
                "{name}: call {i} ({op}) admitted={} but the reference said {}",
                ex.committed(),
                want["status"]
            );

            if !want_ok {
                assert_eq!(
                    ex.result.constraint_violated.as_deref(),
                    want["constraint_violated"].as_str(),
                    "{name}: call {i} refused by a different rule"
                );
                // The property that matters most: a refusal changes nothing.
                assert_eq!(ex.state, state, "{name}: call {i} refused but state moved");
                assert!(ex.mutations.is_empty(), "{name}: refusal left mutations");
                assert!(ex.events.is_empty(), "{name}: refusal emitted events");
            } else {
                let want_events: Vec<&str> = want["events"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|e| e.as_str()).collect())
                    .unwrap_or_default();
                let got_events: Vec<&str> =
                    ex.events.iter().map(|e| e.name.as_str()).collect();
                assert_eq!(got_events, want_events, "{name}: call {i} emitted different events");
            }

            state = ex.state;
        }

        // Identity and time are host-supplied (see budget.rs): the reference
        // mints a UUID and a timestamp inside the operator, which no
        // reproducible core can match. Blank both before comparing, so the
        // assertion is about behaviour rather than about entropy.
        assert_eq!(
            normalise_host_fields(&state),
            normalise_host_fields(&case["expect"]["final_state"]),
            "{name}: final state differs from the reference"
        );
    }
}


/// Blank the two fields a host supplies — a generated id and a wall-clock
/// timestamp. Everything else must match exactly.
fn normalise_host_fields(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, val)| {
                    let cleaned = if k == "id" || k == "received_at" {
                        Value::Null
                    } else {
                        normalise_host_fields(val)
                    };
                    (k.clone(), cleaned)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(normalise_host_fields).collect()),
        other => other.clone(),
    }
}

#[test]
fn council_vectors() {
    use sustena_core::council::{
        aggregate_delegated_votes, resolve, DelegatedVote, ResolutionInput, VoteChoice,
    };

    let doc = load("council.json");

    for case in doc["aggregation_cases"].as_array().expect("aggregation_cases") {
        let name = case["name"].as_str().unwrap();
        let delegated: Vec<DelegatedVote> = case["votes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| DelegatedVote {
                position: VoteChoice::parse(v["position"].as_str().unwrap()).unwrap(),
                confidence: v["confidence"].as_f64().unwrap(),
                reasoning: "r".into(),
            })
            .collect();

        let got = aggregate_delegated_votes(&delegated);
        let want_vote = case["expect"]["vote"].as_str().unwrap();
        assert_eq!(
            VoteChoice::parse(want_vote).unwrap(),
            got.vote,
            "{name}: aggregated vote differs"
        );
        let want_utility = case["expect"]["utility"].as_f64().unwrap();
        assert!(
            (got.utility - want_utility).abs() < 1e-9,
            "{name}: utility {} != {want_utility}",
            got.utility
        );
    }

    for case in doc["resolution_cases"].as_array().expect("resolution_cases") {
        let name = case["name"].as_str().unwrap();
        let votes: Vec<VoteChoice> = case["votes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| VoteChoice::parse(v.as_str().unwrap()).unwrap())
            .collect();

        let status = resolve(&ResolutionInput {
            votes: &votes,
            votes_collected: case["votes_collected"].as_bool().unwrap(),
            user_vote: case["user_vote"].as_str().and_then(VoteChoice::parse),
            expired: case["expired"].as_bool().unwrap(),
        });

        assert_eq!(
            status.as_str(),
            case["expect"]["status"].as_str().unwrap(),
            "{name}: resolution differs from the reference"
        );
    }
}


// ── R2: spec vectors ──────────────────────────────────────────────────────────
//
// These are AUTHORED FROM THE ARTICLES, not generated from the reference
// engine, because the reference does not implement this behaviour yet (it has
// only `refuse`). They record what the spec says, so the Rust core is measured
// against the articles rather than against a gap.

#[test]
fn admission_vectors() {
    use sustena_core::admission::{
        admit_one, typecheck_constraint, ConstraintDecl, DeclError, Verdict,
    };

    let doc = load("admission.json");
    assert_eq!(doc["phase"], "R2");

    for case in doc["cases"].as_array().expect("cases array") {
        let name = case["name"].as_str().unwrap();
        let decl: ConstraintDecl =
            serde_json::from_value(case["declaration"].clone()).expect("declaration parses");
        let expect = &case["expect"];

        let typechecked = typecheck_constraint(&decl);

        if let Some(want_err) = expect["declaration_error"].as_str() {
            let got = typechecked
                .err()
                .unwrap_or_else(|| panic!("{name}: expected the declaration to be rejected"));
            let kind = match got {
                DeclError::Unparseable { .. } => "unparseable",
                DeclError::ClampOnNonInterval { .. } => "clamp_on_non_interval",
                DeclError::ClampOnConserved { .. } => "clamp_on_conserved",
            };
            assert_eq!(kind, want_err, "{name}: rejected for a different reason");
            continue;
        }

        let node = typechecked
            .unwrap_or_else(|e| panic!("{name}: declaration should typecheck, got {e}"));
        let verdict = admit_one(&decl, &node, &case["candidate"], &serde_json::Map::new());

        match expect["verdict"].as_str().unwrap() {
            "admit" => assert_eq!(verdict, Verdict::Admit, "{name}"),

            "refused" => match verdict {
                Verdict::Refused { rule, .. } => {
                    assert_eq!(rule, expect["rule"].as_str().unwrap(), "{name}: wrong rule named")
                }
                other => panic!("{name}: expected a refusal, got {other:?}"),
            },

            "clamped" => match verdict {
                Verdict::Clamped { rule, path, requested, committed } => {
                    assert_eq!(rule, expect["rule"].as_str().unwrap(), "{name}");
                    assert_eq!(path, expect["path"].as_str().unwrap(), "{name}");
                    // Both values must survive: a clamp commits something
                    // nobody asked for, and must report both every time.
                    assert_eq!(requested, expect["requested"], "{name}: requested value lost");
                    assert_eq!(committed, expect["committed"], "{name}: committed value wrong");
                }
                other => panic!("{name}: expected a clamp, got {other:?}"),
            },

            "deferred" => match verdict {
                Verdict::Deferred(d) => {
                    assert_eq!(d.rule, expect["rule"].as_str().unwrap(), "{name}");
                    assert!(
                        d.requires_readmission,
                        "{name}: a deferral must be re-admitted at commit"
                    );
                }
                other => panic!("{name}: expected a deferral, got {other:?}"),
            },

            other => panic!("{name}: unknown expected verdict '{other}'"),
        }
    }
}

#[test]
fn event_vectors() {
    use sustena_core::event::{merge, CausalStamp, Event, Observation, Provenance};

    fn build(v: &Value) -> Event {
        Event {
            id: v["id"].as_str().unwrap().to_string(),
            name: "event.test.thing".into(),
            t_event: v["t_event"].as_i64().unwrap(),
            provenance: Provenance::Observed,
            stamp: CausalStamp {
                counter: v["counter"].as_u64().unwrap(),
                node: v["node"].as_str().unwrap().to_string(),
            },
            causes: v["causes"]
                .as_array()
                .map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            mutations: vec![],
        }
    }

    let doc = load("events.json");
    assert_eq!(doc["phase"], "R2");

    for case in doc["ordering_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let events: Vec<Event> = case["events"].as_array().unwrap().iter().map(build).collect();
        let want: Vec<&str> = case["expect"]["order"]
            .as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();

        let got = merge(events.clone());
        let got_ids: Vec<&str> = got.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(got_ids, want, "{name}: order differs");

        // Convergence: the result must not depend on the order they arrived in.
        if case["expect"]["order_is_independent_of_input_order"].as_bool() == Some(true) {
            let mut reversed = events;
            reversed.reverse();
            let other: Vec<String> = merge(reversed).iter().map(|e| e.id.clone()).collect();
            assert_eq!(other, want, "{name}: replicas disagreed on order");
        }
    }

    for case in doc["causality_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let earlier = build(&case["earlier"]);
        let later = build(&case["later"]);

        assert_eq!(
            earlier.happens_before(&later),
            case["expect"]["happens_before"].as_bool().unwrap(),
            "{name}: happens-before differs"
        );
        if case["expect"]["concurrent"].as_bool() == Some(true) {
            assert!(earlier.concurrent_with(&later), "{name}: expected concurrency");
        }
    }

    for case in doc["convergence_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let obs: Vec<Observation<i64>> = case["observations"]
            .as_array().unwrap().iter()
            .map(|o| Observation {
                value: o["value"].as_i64().unwrap(),
                t_event: o["t_event"].as_i64().unwrap(),
                id: o["id"].as_str().unwrap().to_string(),
            })
            .collect();

        let joined = obs.iter().skip(1).fold(obs[0].clone(), |acc, o| acc.join(o));
        assert_eq!(
            joined.value,
            case["expect"]["value"].as_i64().unwrap(),
            "{name}: joined value differs"
        );

        // Every permutation must land on the same value — that IS the CRDT
        // property, so it is checked rather than asserted in prose.
        if case["expect"]["converges_under_every_permutation"].as_bool() == Some(true) {
            let n = obs.len();
            for start in 0..n {
                let rotated: Vec<_> =
                    (0..n).map(|i| obs[(start + i) % n].clone()).collect();
                let out = rotated.iter().skip(1).fold(rotated[0].clone(), |a, o| a.join(o));
                assert_eq!(out, joined, "{name}: a replica diverged");
            }
        }
    }
}
