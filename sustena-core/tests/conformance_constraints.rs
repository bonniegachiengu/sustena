//! Transition constraints replayed against their conformance vectors
//! (R2 · Constraint §I, §VI · WBD CON-1, CON-7).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no transition-constraint
//! family at all: both of its evaluators are single-state *by signature*, so
//! conservation is not merely unimplemented there but inexpressible.
//! `constraints.json` records that — including the part that would be easy to
//! overstate, namely that Python *does* have transactional **atomicity** for
//! `holon.transfer`, which is a different guarantee and does not check that the
//! debit and the credit are equal.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    check_transitions, execute, Direction, Enforcement, Quantity, Registry, Tolerance,
    TransitionDeclError, TransitionRule, CONFORMANCE_VERSION,
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

fn paths_of(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect()
}

fn tolerance_of(v: &Value) -> Tolerance {
    match v {
        Value::String(s) if s == "exact" => Tolerance::Exact,
        Value::Number(n) => Tolerance::Within(n.as_f64().unwrap()),
        other => panic!("unknown tolerance in vector: {other}"),
    }
}

fn conservation(id: &str, paths: &[String], tolerance: Tolerance) -> TransitionRule {
    TransitionRule::Conservation {
        id: id.to_string(),
        quantity: Quantity {
            id: "quantity".to_string(),
            paths: paths.to_vec(),
        },
        tolerance,
    }
}

/// Assert a vector's `holds` / `detail_contains` expectation against a result.
fn assert_expectation(name: &str, expect: &Value, got: Result<(), sustena_core::TransitionViolation>) {
    if expect["holds"].as_bool().unwrap() {
        assert_eq!(got, Ok(()), "{name}: the rule should have held");
    } else {
        let v = got.expect_err(&format!("{name}: the rule should have refused"));
        if let Some(needle) = expect["detail_contains"].as_str() {
            assert!(
                v.detail.contains(needle),
                "{name}: expected the refusal to mention '{needle}', got: {}",
                v.detail
            );
        }
    }
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("constraints.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));
    // The atomicity distinction is the part that keeps the claim honest in both
    // directions — Python is not defenceless, and atomicity is not conservation.
    let note = doc["divergence"]["what_python_does_have_and_why_it_is_not_D"]
        .as_str()
        .expect("the atomicity-is-not-conservation note must stay");
    assert!(note.contains("atomicity") || note.contains("ATOMICITY"));
}

#[test]
fn expressiveness_vectors() {
    let doc = load("constraints.json");
    let cases = doc["expressiveness_cases"].as_array().unwrap();

    // Case 1: every shape holds vacuously against a single state.
    let c = &cases[0];
    let s = &c["state"];
    let money = conservation(
        "money",
        &["finances.liquid.balance".into(), "finances.pockets[*].allocated".into()],
        Tolerance::Exact,
    );
    for rule in [
        money.clone(),
        TransitionRule::RateLimit { id: "r".into(), path: "finances.liquid.balance".into(), delta: 0.0 },
        TransitionRule::Monotone {
            id: "m".into(),
            path: "finances.liquid.balance".into(),
            direction: Direction::NonIncreasing,
        },
    ] {
        assert_eq!(
            rule.check(s, s), Ok(()),
            "{}: {} must hold vacuously against a single state",
            c["name"].as_str().unwrap(), rule.id()
        );
    }

    // Case 2: the after-state alone is admissible; only the STEP is not.
    let c = &cases[1];
    let name = c["name"].as_str().unwrap();
    let before = &c["before"];
    let after = &c["after"];
    // Every balance in the after-state is non-negative — a state constraint sees
    // nothing wrong.
    let params = Map::new();
    let (ok, _) = sustena_core::check("finances.liquid.balance >= 0", after, &params).unwrap();
    assert!(ok, "{name}: the after-state must be individually valid for this case to mean anything");
    let (ok, _) = sustena_core::check("ALL finances.pockets[*].allocated >= 0", after, &params).unwrap();
    assert!(ok, "{name}: pockets are individually valid too");
    assert!(money.check(before, after).is_err(), "{name}: but the STEP created money");
}

#[test]
fn conservation_vectors() {
    let doc = load("constraints.json");
    for case in doc["conservation_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let rule = conservation("law", &paths_of(&case["quantity"]), tolerance_of(&case["tolerance"]));
        assert_expectation(name, &case["expect"], rule.check(&case["before"], &case["after"]));
    }
}

#[test]
fn rate_limit_vectors() {
    let doc = load("constraints.json");
    for case in doc["rate_limit_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let rule = TransitionRule::RateLimit {
            id: "limit".into(),
            path: case["path"].as_str().unwrap().to_string(),
            delta: case["delta"].as_f64().unwrap(),
        };
        assert_expectation(name, &case["expect"], rule.check(&case["before"], &case["after"]));
    }
}

#[test]
fn monotonicity_vectors() {
    let doc = load("constraints.json");
    for case in doc["monotonicity_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let rule = TransitionRule::Monotone {
            id: "mono".into(),
            path: case["path"].as_str().unwrap().to_string(),
            direction: match case["direction"].as_str().unwrap() {
                "non_increasing" => Direction::NonIncreasing,
                "non_decreasing" => Direction::NonDecreasing,
                other => panic!("{name}: unknown direction {other}"),
            },
        };
        assert_expectation(name, &case["expect"], rule.check(&case["before"], &case["after"]));
    }
}

#[test]
fn fail_safe_vectors() {
    let doc = load("constraints.json");
    for case in doc["fail_safe_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let rule = TransitionRule::RateLimit {
            id: "limit".into(),
            path: case["path"].as_str().unwrap().to_string(),
            delta: case["delta"].as_f64().unwrap(),
        };
        assert_expectation(name, &case["expect"], rule.check(&case["before"], &case["after"]));
    }
}

#[test]
fn declaration_vectors() {
    let doc = load("constraints.json");
    for case in doc["declaration_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let spec = &case["rule"];
        let rule = match spec["kind"].as_str().unwrap() {
            "conservation" => conservation(
                "law",
                &paths_of(&spec["paths"]),
                tolerance_of(&spec["tolerance"]),
            ),
            "rate_limit" => TransitionRule::RateLimit {
                id: "limit".into(),
                path: spec["path"].as_str().unwrap().to_string(),
                delta: spec["delta"].as_f64().unwrap(),
            },
            other => panic!("{name}: unknown rule kind {other}"),
        };

        let got = rule.typecheck();
        if case["expect"]["well_formed"].as_bool().unwrap() {
            assert_eq!(got, Ok(()), "{name}: should be well-formed");
        } else {
            let err = got.expect_err(&format!("{name}: should be rejected at authoring time"));
            let kind = match err {
                TransitionDeclError::BadPath { .. } => "BadPath",
                TransitionDeclError::BadDelta { .. } => "BadDelta",
                TransitionDeclError::BadTolerance { .. } => "BadTolerance",
                TransitionDeclError::EmptyQuantity { .. } => "EmptyQuantity",
            };
            assert_eq!(kind, case["expect"]["error"].as_str().unwrap(), "{name}");
        }
    }
}

#[test]
fn gate_vectors() {
    use sustena_core::operator::meta::{OperatorMeta, Protocol};
    use sustena_core::operator::EmittedEvent;
    use sustena_core::OperatorResult;

    let doc = load("constraints.json");

    // The operator the vectors need: credits a pocket with no matching debit.
    let mut registry = Registry::default();
    registry.register(OperatorMeta {
        name: "test.mint",
        description: "Credits a pocket with no matching debit. Exists to prove D refuses it.",
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.test.minted"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 0,
        run: |state, _p, events| {
            let _ = state.set("finances.pockets.food.allocated", json!(25000));
            events.push(EmittedEvent { name: "event.test.minted".into(), payload: json!({}) });
            OperatorResult::ok(json!({}))
        },
    });
    let mut allowed = registry.names();
    allowed.push("test.mint".to_string());

    for case in doc["gate_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let state = case["state"].clone();
        let params: Map<String, Value> = case["params"].as_object().cloned().unwrap_or_default();

        let transitions = match case["conservation"].as_array() {
            Some(paths) => vec![conservation(
                "money_conserved",
                &paths.iter().map(|p| p.as_str().unwrap().to_string()).collect::<Vec<_>>(),
                Tolerance::Exact,
            )],
            None => vec![],
        };

        let enforcement = Enforcement {
            enabled: case["enabled"].as_bool().unwrap(),
            invariants: vec![],
            schema: None,
            transitions,
        };

        let ex = execute(
            &registry, &allowed, &enforcement, &state,
            case["operator"].as_str().unwrap(), &params,
        );

        let expect = &case["expect"];
        assert_eq!(
            ex.committed(), expect["committed"].as_bool().unwrap(),
            "{name}: wrong verdict ({:?})", ex.result.reason
        );
        if let Some(rule) = expect["rule"].as_str() {
            assert_eq!(ex.result.constraint_violated.as_deref(), Some(rule), "{name}");
        }
        if expect["state_unchanged"].as_bool() == Some(true) {
            assert_eq!(ex.state, state, "{name}: a refusal must change nothing");
            assert!(ex.mutations.is_empty(), "{name}");
            assert!(ex.events.is_empty(), "{name}");
        }
        if let Some(needle) = expect["reason_contains"].as_str() {
            let why = ex.result.reason.as_deref().unwrap_or_default();
            assert!(why.contains(needle), "{name}: expected '{needle}' in: {why}");
        }
    }
}

#[test]
fn conjunction_is_the_only_combinator() {
    // §I: adding a constraint can only ever SHRINK the admissible region, and
    // the refusal names which law refused. Both are checked here because both
    // are properties of the combinator, not of any one rule.
    let before = json!({"finances": {"liquid": {"balance": 100000}, "pockets": {"food": {"allocated": 20000}}}});
    let after = json!({"finances": {"liquid": {"balance": 90000}, "pockets": {"food": {"allocated": 30000}}}});

    let money = conservation(
        "money_conserved",
        &["finances.liquid.balance".into(), "finances.pockets[*].allocated".into()],
        Tolerance::Exact,
    );
    assert_eq!(check_transitions(std::slice::from_ref(&money), &before, &after), Ok(()));

    let tight = TransitionRule::RateLimit {
        id: "no_big_jumps".into(),
        path: "finances.liquid.balance".into(),
        delta: 1000.0,
    };
    let v = check_transitions(&[money, tight], &before, &after)
        .expect_err("adding a rule can only shrink what is admissible");
    assert_eq!(v.rule_id, "no_big_jumps", "the refusal names which law refused");
}
