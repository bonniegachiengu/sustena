//! The approval token replayed against its conformance vectors
//! (R2 · Operative §XVI · WBD N1).
//!
//! **These are SPEC vectors, and Rust replays them alone.** The reference
//! Python engine has no approval token — `approval_token`, `valid_token` and
//! `effect_class` are grep-0 in `apps/api/sustena/`, which the Operative
//! article's own Sustena Note records independently. `approval.json` carries
//! that as an explicit `divergence` block, and the first assertion below fails
//! if anyone quietly deletes it.
//!
//! A separate test binary from `conformance.rs` because this is a separate
//! slice: the R1 parity vectors and the R2 spec vectors should be able to fail
//! independently, and a reader should be able to tell which is which from the
//! file name.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    execute_admitted, ApprovalToken, Authorization, Binding, EffectClass, Enforcement, NonceLedger,
    ProposalStatus, Registry, Simulated, TokenError, TraceError, CONFORMANCE_VERSION,
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

fn params_of(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

fn status_of(s: &str) -> ProposalStatus {
    match s {
        "PASSED" => ProposalStatus::Passed,
        "IN_VOTING" => ProposalStatus::InVoting,
        "FAILED" => ProposalStatus::Failed,
        "DEFERRED" => ProposalStatus::Deferred,
        "OVERRIDDEN_BY_USER" => ProposalStatus::OverriddenByUser,
        other => panic!("unknown proposal status in vector: {other}"),
    }
}

/// Mint a token the only way the crate allows: sim -> vote -> approve.
fn mint(spec: &Value) -> ApprovalToken {
    let binding = Binding::new(
        spec["operator"].as_str().unwrap(),
        &params_of(&spec["params"]),
    );
    Simulated::from_sandbox("vector", binding, Ok(()))
        .expect("the sandbox run committed")
        .voted(ProposalStatus::Passed)
        .expect("the council passed it")
        .approve(
            spec["principal"].as_str().unwrap(),
            spec["nonce"].as_u64().unwrap(),
            spec["expiry"].as_u64().unwrap(),
        )
}

fn token_err_name(e: &TokenError) -> &'static str {
    match e {
        TokenError::WrongAct { .. } => "WrongAct",
        TokenError::WrongParams => "WrongParams",
        TokenError::WrongPrincipal { .. } => "WrongPrincipal",
        TokenError::Expired { .. } => "Expired",
        TokenError::AlreadySpent { .. } => "AlreadySpent",
    }
}

fn armed_homestead() -> Enforcement {
    Enforcement {
        enabled: true,
        invariants: vec![
            ("liquid_non_negative".into(), "finances.liquid.balance >= 0".into()),
            (
                "pocket_allocated_non_negative".into(),
                "ALL finances.pockets[*].allocated >= 0".into(),
            ),
        ],
        schema: None,
    }
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("approval.json");
    assert_eq!(
        doc["divergence"]["kind"], "rust-ahead-of-python",
        "a divergence that stops being documented is a divergence that became silent"
    );
    assert_eq!(doc["replayed_by"], json!(["rust"]));
}

#[test]
fn approval_trace_vectors() {
    let doc = load("approval.json");

    for case in doc["trace_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let chain = &case["chain"];
        let binding = Binding::new("budget.spend", &params_of(&json!({"amount": 1.0})));

        let sandbox = if chain["sandbox_committed"].as_bool().unwrap() {
            Ok(())
        } else {
            Err("would violate invariant".to_string())
        };

        let want_token = case["expect"]["token_minted"].as_bool().unwrap();

        // Walk the chain, keeping the FIRST stage that refused. Each `?` here
        // is a stage of e_sim ≺ e_vote ≺ e_approve, and the chain cannot be
        // entered part-way: `voted` consumes the `Simulated` by value.
        let outcome = Simulated::from_sandbox(chain["proposal_id"].as_str().unwrap(), binding, sandbox)
            .and_then(|sim| sim.voted(status_of(chain["vote"].as_str().unwrap())))
            .map(|voted| voted.approve("bonnie", 1, 100));

        match outcome {
            Ok(token) => {
                assert!(want_token, "{name}: a token was minted but the vector forbids it");
                assert_eq!(token.proposal_id(), chain["proposal_id"].as_str().unwrap());
            }
            Err(err) => {
                assert!(!want_token, "{name}: expected a token, but the chain refused: {err}");
                let stage = match err {
                    TraceError::SandboxRefused { .. } => "SandboxRefused",
                    TraceError::NotPassed { .. } => "NotPassed",
                };
                assert_eq!(
                    stage,
                    case["expect"]["error"].as_str().unwrap(),
                    "{name}: a different stage refused than the vector records"
                );
            }
        }
    }
}

#[test]
fn approval_validation_vectors() {
    let doc = load("approval.json");
    for case in doc["validation_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let token = mint(&case["token"]);
        let attempt = &case["attempt"];
        let attempted = Binding::new(
            attempt["operator"].as_str().unwrap(),
            &params_of(&attempt["params"]),
        );

        let got = token.validate(
            &attempted,
            attempt["acting_principal"].as_str(),
            attempt["now"].as_u64().unwrap(),
        );

        if case["expect"]["valid"].as_bool().unwrap() {
            assert_eq!(got, Ok(()), "{name}: expected the approval to admit this act");
        } else {
            let err = got.expect_err(&format!("{name}: expected a refusal"));
            assert_eq!(
                token_err_name(&err),
                case["expect"]["error"].as_str().unwrap(),
                "{name}: refused for a different reason than the vector records"
            );
        }
    }
}

#[test]
fn approval_single_use_vectors() {
    let doc = load("approval.json");
    for case in doc["single_use_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let nonce = case["nonce"].as_u64().unwrap();
        let spent: Vec<u64> = case["already_spent"]
            .as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();

        let token = mint(&json!({
            "operator": "budget.spend", "params": {"amount": 1.0},
            "principal": "bonnie", "nonce": nonce, "expiry": 100
        }));
        let mut ledger = NonceLedger::with_spent(spent);

        let want = case["expect"]["outcomes"].as_array().unwrap();
        assert_eq!(
            case["redemptions"].as_u64().unwrap() as usize, want.len(),
            "{name}: the vector is self-inconsistent"
        );

        for (i, expected) in want.iter().enumerate() {
            let got = ledger.redeem(&token);
            match expected.as_str().unwrap() {
                "ok" => assert_eq!(got, Ok(()), "{name}: redemption {i} should have succeeded"),
                other => {
                    let err = got.expect_err(&format!("{name}: redemption {i} should have failed"));
                    assert_eq!(token_err_name(&err), other, "{name}: redemption {i}");
                }
            }
        }
    }
}

#[test]
fn approval_gate_vectors() {
    let doc = load("approval.json");
    let registry = Registry::default();
    let allowed = registry.names();

    for case in doc["gate_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let state = case["state"].clone();
        let operator = case["operator"].as_str().unwrap();
        let params = params_of(&case["params"]);
        let mut ledger = NonceLedger::new();

        // Bound outside the match so the borrow outlives the EffectClass.
        let token = case.get("token").map(mint);
        let effect = match case["effect_class"].as_str().unwrap() {
            "sandbox" => EffectClass::Sandbox,
            "live" => EffectClass::Live {
                token: token.as_ref().expect("a live case carries a token"),
                now: case["now"].as_u64().unwrap(),
            },
            other => panic!("{name}: unknown effect_class {other}"),
        };

        let ex = execute_admitted(
            &registry, &allowed, &armed_homestead(), &state, operator, &params,
            &Authorization::Unchecked, &effect, &mut ledger,
        );

        let expect = &case["expect"];
        assert_eq!(
            ex.committed(), expect["committed"].as_bool().unwrap(),
            "{name}: wrong verdict ({:?})", ex.result.reason
        );

        if let Some(rule) = expect["rule"].as_str() {
            assert_eq!(ex.result.constraint_violated.as_deref(), Some(rule), "{name}: wrong rule");
        }
        if expect["state_unchanged"].as_bool() == Some(true) {
            assert_eq!(ex.state, state, "{name}: a refusal must change nothing");
        }
        if let Some(after) = expect["balance_after"].as_f64() {
            assert_eq!(ex.state["finances"]["liquid"]["balance"], json!(after), "{name}");
        }
        if expect["nonce_spent_after"].as_bool() == Some(false) {
            let n = case["token"]["nonce"].as_u64().unwrap();
            assert!(!ledger.is_spent(n), "{name}: a refusal must not burn the approval");
        }

        if case["run_twice"].as_bool() == Some(true) {
            let again = execute_admitted(
                &registry, &allowed, &armed_homestead(), &ex.state, operator, &params,
                &Authorization::Unchecked, &effect, &mut ledger,
            );
            assert_eq!(
                again.committed(), expect["second_committed"].as_bool().unwrap(),
                "{name}: the replay must not commit"
            );
            assert_eq!(
                again.result.constraint_violated.as_deref(),
                expect["second_rule"].as_str(),
                "{name}: the replay refused for the wrong reason"
            );
            assert_eq!(again.state, ex.state, "{name}: the replay must change nothing");
        }
    }
}
