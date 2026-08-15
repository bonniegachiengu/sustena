//! Edit authority replayed against its conformance vectors
//! (R2 · Editing §VI · WBD EDIT-8).
//!
//! **SPEC vectors, Rust-only — but the divergence is PARTIAL and term-by-term.**
//! The reference engine genuinely implements `Safe(e, μ)` for `μ = id`
//! (`check_definition_edit_safety`, witness set and all), so that term is at
//! parity in shape. What it lacks is the authority term — it checks
//! `owner_user_id` inline, which is ownership, not authority — and any typed
//! edit at all, so `applicable(e, D)` has nothing to attach to.
//! `editing.json` records that split rather than one blanket claim.
//!
//! See `conformance/README.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    admit_edit, typecheck, AuthorityError, CouncilMint, Definition, DimType, Edit, EditAuthority,
    EditEffect, EditError, EditToken, Governance, Instance, Migration, ProposalStatus, Schema,
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

fn edit_of(spec: &Value) -> Edit {
    match spec["kind"].as_str().unwrap() {
        "AddInv" => Edit::AddInv {
            id: spec["id"].as_str().unwrap().to_string(),
            expression: spec["expression"].as_str().unwrap().to_string(),
        },
        "DropInv" => Edit::DropInv { id: spec["id"].as_str().unwrap().to_string() },
        "ModifyInv" => Edit::ModifyInv {
            id: spec["id"].as_str().unwrap().to_string(),
            expression: spec["expression"].as_str().unwrap().to_string(),
        },
        "AddDim" => Edit::AddDim {
            name: spec["name"].as_str().unwrap().to_string(),
            ty: DimType::Any,
            default: json!({}),
        },
        "RetypeDim" => Edit::RetypeDim {
            name: spec["name"].as_str().unwrap().to_string(),
            ty: DimType::Any,
        },
        "RetireDim" => Edit::RetireDim { name: spec["name"].as_str().unwrap().to_string() },
        "AddOp" => Edit::AddOp { name: spec["name"].as_str().unwrap().to_string() },
        "RetireOp" => Edit::RetireOp { name: spec["name"].as_str().unwrap().to_string() },
        other => panic!("unknown edit kind in vector: {other}"),
    }
}

fn instances_of(spec: &Value) -> Vec<Instance> {
    spec.as_array().unwrap().iter()
        .map(|i| Instance {
            id: i["id"].as_str().unwrap().to_string(),
            state: json!({"moisture": {"level": i["level"].as_f64().unwrap()}}),
        })
        .collect()
}

fn status_of(s: &str) -> ProposalStatus {
    match s {
        "PASSED" => ProposalStatus::Passed,
        "IN_VOTING" => ProposalStatus::InVoting,
        "FAILED" => ProposalStatus::Failed,
        "DEFERRED" => ProposalStatus::Deferred,
        "OVERRIDDEN_BY_USER" => ProposalStatus::OverriddenByUser,
        other => panic!("unknown proposal status: {other}"),
    }
}

fn authority_err_name(e: &AuthorityError) -> &'static str {
    match e {
        AuthorityError::WrongEdit { .. } => "WrongEdit",
        AuthorityError::WrongPrincipal { .. } => "WrongPrincipal",
        AuthorityError::Expired { .. } => "Expired",
        AuthorityError::NotTheOwner { .. } => "NotTheOwner",
        AuthorityError::SharedNeedsCouncil => "SharedNeedsCouncil",
        AuthorityError::CouncilDidNotPass { .. } => "CouncilDidNotPass",
        AuthorityError::QuorumNotMet => "QuorumNotMet",
    }
}

fn edit_err_name(e: &EditError) -> &'static str {
    match e {
        EditError::NotApplicable { .. } => "NotApplicable",
        EditError::NotWellTyped(_) => "NotWellTyped",
        EditError::WouldStrand(_) => "WouldStrand",
        EditError::Unauthorized(_) => "Unauthorized",
    }
}

/// A sole-owner token for exactly this edit.
fn owner_token(edit: Edit) -> EditToken {
    EditToken::mint(
        &Governance::SoleOwner("bonnie".into()),
        &EditAuthority::Owner("bonnie"),
        edit,
        100,
    )
    .expect("the owner may mint")
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("editing.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));
    // The term-by-term breakdown is the honest part; a blanket claim would
    // overstate the gap, since Safe() genuinely exists in the reference.
    let terms = &doc["divergence"]["term_by_term"];
    for t in ["tok(α,e)", "⊢e(D) ok", "Safe(e,μ)", "applicable(e,D)"] {
        assert!(terms[t].is_string(), "the {t} term must stay accounted for");
    }
    assert!(
        terms["Safe(e,μ)"].as_str().unwrap().contains("AT PARITY"),
        "Safe() is NOT rust-ahead — the reference implements it, and saying otherwise would overstate the gap"
    );
}

#[test]
fn editing_authority_vectors() {
    let doc = load("editing.json");
    for case in doc["authority_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let governance = match case["governance"]["kind"].as_str().unwrap() {
            "sole_owner" => Governance::SoleOwner(
                case["governance"]["owner"].as_str().unwrap().to_string(),
            ),
            "shared" => Governance::Shared,
            other => panic!("{name}: unknown governance {other}"),
        };
        let edit = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };

        // Bound outside the match so the borrow outlives EditAuthority.
        let mut mint_err: Option<AuthorityError> = None;
        let mint = if case["authority"]["kind"] == "council" {
            match CouncilMint::from_decision(
                case["authority"]["proposal_id"].as_str().unwrap(),
                status_of(case["authority"]["status"].as_str().unwrap()),
                case["authority"]["quorum_met"].as_bool().unwrap(),
            ) {
                Ok(m) => Some(m),
                Err(e) => {
                    mint_err = Some(e);
                    None
                }
            }
        } else {
            None
        };

        let got = match (&mint_err, &mint) {
            // The council refused before a token was ever attempted.
            (Some(e), _) => Err(e.clone()),
            (None, Some(m)) => {
                EditToken::mint(&governance, &EditAuthority::Council(m), edit, 100)
            }
            (None, None) => EditToken::mint(
                &governance,
                &EditAuthority::Owner(case["authority"]["principal"].as_str().unwrap()),
                edit,
                100,
            ),
        };

        if case["expect"]["minted"].as_bool().unwrap() {
            let token = got.unwrap_or_else(|e| panic!("{name}: expected a token, got {e}"));
            if let Some(pid) = case["expect"]["proposal_id"].as_str() {
                assert_eq!(token.proposal_id(), Some(pid), "{name}");
            }
        } else {
            let err = got.err().unwrap_or_else(|| panic!("{name}: expected a refusal"));
            assert_eq!(
                authority_err_name(&err),
                case["expect"]["error"].as_str().unwrap(),
                "{name}: refused for a different reason than the vector records"
            );
        }
    }
}

#[test]
fn editing_token_binding_vectors() {
    let doc = load("editing.json");
    for case in doc["token_binding_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let token = owner_token(edit_of(&case["approved"]));
        let attempted = edit_of(&case["attempted"]);

        let got = token.validate(
            &attempted,
            case["acting"].as_str(),
            case["now"].as_u64().unwrap(),
        );

        if case["expect"]["valid"].as_bool().unwrap() {
            assert_eq!(got, Ok(()), "{name}: expected the token to admit this edit");
        } else {
            let err = got.expect_err(&format!("{name}: expected a refusal"));
            assert_eq!(
                authority_err_name(&err),
                case["expect"]["error"].as_str().unwrap(),
                "{name}"
            );
        }
    }
}

#[test]
fn editing_gate_vectors() {
    let doc = load("editing.json");
    for case in doc["gate_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let definition = definition_of(&case["definition"]);
        let edit = edit_of(&case["edit"]);
        let instances = instances_of(&case["instances"]);

        // A token for a DIFFERENT edit, when the vector asks for one.
        let token = match case["effect"].as_str().unwrap() {
            "live" => Some(owner_token(
                case.get("token_for").map(edit_of).unwrap_or_else(|| edit.clone()),
            )),
            _ => None,
        };
        let effect = match case["effect"].as_str().unwrap() {
            "sandbox" => EditEffect::Sandbox,
            "live" => EditEffect::Live {
                token: token.as_ref().unwrap(),
                now: case["now"].as_u64().unwrap(),
            },
            other => panic!("{name}: unknown effect {other}"),
        };

        let out = admit_edit(&definition, &edit, &instances, Migration::Identity, &effect);
        let expect = &case["expect"];

        assert_eq!(
            out.admitted(),
            expect["admitted"].as_bool().unwrap(),
            "{name}: wrong verdict ({:?})",
            out.verdict
        );

        if let Some(want) = expect["error"].as_str() {
            let err = out.verdict.as_ref().err().unwrap();
            assert_eq!(edit_err_name(err), want, "{name}: refused for a different reason");
        }
        if let Some(n) = expect["invariants_after"].as_u64() {
            assert_eq!(out.candidate.invariants.len(), n as usize, "{name}");
        }
        if let Some(witnesses) = expect["stranded"].as_array() {
            match out.verdict.as_ref().err().unwrap() {
                EditError::WouldStrand(v) => {
                    assert_eq!(v.len(), witnesses.len(), "{name}: wrong number of witnesses");
                    for (got, want) in v.iter().zip(witnesses) {
                        assert_eq!(got.instance_id, want["instance_id"].as_str().unwrap(), "{name}");
                        assert_eq!(got.invariant_id, want["invariant_id"].as_str().unwrap(), "{name}");
                        assert!(!got.reason.is_empty(), "{name}: a refusal must say why");
                    }
                }
                other => panic!("{name}: expected WouldStrand, got {other:?}"),
            }
        }

        // Whatever happened, the definition the caller holds is untouched.
        assert_eq!(definition, definition_of(&case["definition"]), "{name}: D was mutated");
    }
}

#[test]
fn editing_induction_vector() {
    let doc = load("editing.json");
    let case = &doc["induction_case"];
    let name = case["name"].as_str().unwrap();

    let mut d = definition_of(&case["definition"]);
    let instances = instances_of(&case["instances"]);

    // Σ↑ starts inside V↑.
    assert!(typecheck(&d).is_ok(), "{name}: the starting definition must be well-typed");
    assert!(
        sustena_core::safe(&d, &instances, Migration::Identity).is_ok(),
        "{name}: every instance must start viable"
    );

    for spec in case["edits"].as_array().unwrap() {
        let edit = edit_of(spec);
        let token = owner_token(edit.clone());
        let out = admit_edit(
            &d, &edit, &instances, Migration::Identity,
            &EditEffect::Live { token: &token, now: 10 },
        );
        assert!(out.admitted(), "{name}: {:?} was refused: {:?}", spec, out.verdict);
        d = out.candidate;

        // ...and stays inside V↑ after every admitted edit. That IS the
        // induction: the property is re-established at each step, not assumed.
        assert!(typecheck(&d).is_ok(), "{name}: ⊢D ok must hold at every step");
        assert!(
            sustena_core::safe(&d, &instances, Migration::Identity).is_ok(),
            "{name}: instances must stay viable at every step"
        );
    }

    assert_eq!(d.invariants.len(), case["expect"]["invariants_after"].as_u64().unwrap() as usize);
    assert_eq!(d.operators.len(), case["expect"]["operators_after"].as_u64().unwrap() as usize);
}
