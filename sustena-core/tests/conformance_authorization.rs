//! `permitted(α, o, Σ)` as a conjunct of `admit()` — replayed against its
//! vectors (R2 · Immune §IV · Sustain §4.5).
//!
//! **SPEC vectors, Rust-only — and the gap here is unusually clean.** The
//! reference *declares* `min_privilege` on `OperatorMeta` and **reads it
//! nowhere**: one dataclass field, one keyword argument, zero readers, and no
//! `permitted` / `effective_privilege` / `privilege.check` anywhere in the
//! Python engine. The conjunct exists in the articles and in the declaration,
//! and never once in a decision. So there is nothing to record from the
//! reference, and every case below is authored from the spec.
//!
//! ★★★ The property the whole file exists for is
//! `authorization_is_a_separate_conjunct`: **both** authority and the viable
//! region must hold, they refuse with **different rule names**, and authority
//! is decided **first** — because *may this principal act here* does not depend
//! on state, so there is no reason to run an effect for it to judge.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::operator::{execute_admitted, Authorization, Enforcement, Registry};
use sustena_core::principal::{
    permitted_with, Denial, MembershipEdge, Memberships, Skin, SkinRegistry, Tier,
};
use sustena_core::{EffectClass, NonceLedger, CONFORMANCE_VERSION};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("authorization.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "authorization.json was written for a different contract version"
    );
    doc
}

fn case<'a>(doc: &'a Value, name: &str) -> &'a Value {
    doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no case named {name}"))
}

fn rule_of(d: &Denial) -> &'static str {
    match d {
        Denial::NoEdge { .. } => "not_a_member",
        Denial::InsufficientTier { .. } => "insufficient_privilege",
        Denial::UnresolvedSkin { .. } => "skin_resolves",
        Denial::NotDesignated { .. } => "capability_designates",
        Denial::RightNotHeld { .. } => "capability_carries",
    }
}

/// Build the membership graph and skin registry a case declares.
fn model(c: &Value) -> (Memberships, SkinRegistry) {
    let mut m = Memberships::new();
    for e in c["edges"].as_array().into_iter().flatten() {
        m.grant(MembershipEdge {
            principal: c["principal"].as_str().unwrap().to_string(),
            sustain: e["sustain"].as_str().unwrap().to_string(),
            tier: e["tier"].as_u64().unwrap() as Tier,
            skin: e["skin"].as_str().map(str::to_string),
        });
    }
    let mut skins = SkinRegistry::empty();
    for s in c["skins"].as_array().into_iter().flatten() {
        skins.define(Skin {
            name: s["name"].as_str().unwrap().to_string(),
            tier: s["tier"].as_u64().unwrap() as Tier,
        });
    }
    (m, skins)
}

fn path_of(c: &Value) -> Vec<String> {
    c["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn every_declared_permission_case_holds() {
    let doc = load();
    for c in doc["cases"].as_array().unwrap() {
        // The composite case has its own test below.
        if c["path"].is_null() {
            continue;
        }
        let name = c["name"].as_str().unwrap();
        let (m, skins) = model(c);
        let path = path_of(c);
        let required = c["min_privilege"].as_u64().unwrap() as Tier;
        let principal = c["principal"].as_str().unwrap();

        let verdict = permitted_with(&m, &skins, principal, &path, required);
        let want = &c["expect"];

        assert_eq!(
            verdict.is_ok(),
            want["permitted"].as_bool().unwrap(),
            "{name}: permitted?"
        );

        if let Err(d) = &verdict {
            assert_eq!(rule_of(d), want["rule"].as_str().unwrap(), "{name}: rule");

            // ★★ The refusal must NAME what was missing. A "you may not" with
            //    no tiers in it tells a person nothing they can act on.
            if let Denial::InsufficientTier { held, required: req, .. } = d {
                if let Some(h) = want["held"].as_u64() {
                    assert_eq!(*held as u64, h, "{name}: tier held");
                }
                if let Some(r) = want["required"].as_u64() {
                    assert_eq!(*req as u64, r, "{name}: tier required");
                }
            }
            let text = d.to_string();
            for token in want["reason_mentions"].as_array().into_iter().flatten() {
                let token = token.as_str().unwrap();
                assert!(
                    text.contains(token),
                    "{name}: the refusal must mention {token:?} — got {text:?}"
                );
            }
        }
    }
}

// ── the composite property ───────────────────────────────────────────────────

fn registry() -> Registry {
    Registry::default()
}

fn run(tier: Tier, amount: f64, invariant: &str) -> sustena_core::operator::Execution {
    let mut m = Memberships::new();
    m.grant(MembershipEdge {
        principal: "bg.myc".into(),
        sustain: "own".into(),
        tier,
        skin: None,
    });
    let skins = SkinRegistry::empty();
    let path = vec!["own".to_string()];

    let enforcement = Enforcement {
        enabled: true,
        invariants: vec![("floor".into(), invariant.into())],
        ..Default::default()
    };
    let state = json!({ "finances": { "liquid": { "balance": 500.0 }, "pockets": {},
                                      "income": { "monthly_total": 0.0, "sources": [] } } });
    let mut params = Map::new();
    params.insert("amount".into(), json!(amount));
    params.insert("source".into(), json!("vector"));

    execute_admitted(
        &registry(),
        &["budget.record_income".to_string()],
        &enforcement,
        &state,
        "budget.record_income",
        &params,
        &Authorization::Principal {
            id: "bg.myc",
            memberships: &m,
            path: &path,
            skins: &skins,
        },
        &EffectClass::Unchecked,
        &mut NonceLedger::new(),
    )
}

/// ★★★ Authority and the viable region are two conjuncts, not one.
#[test]
fn authorization_is_a_separate_conjunct_from_the_viable_region() {
    let doc = load();
    let c = case(&doc, "authorization_is_a_separate_conjunct_from_the_viable_region");
    let invariant = c["invariant"].as_str().unwrap();

    for sub in c["cases"].as_array().unwrap() {
        let tier = sub["tier"].as_u64().unwrap() as Tier;
        let amount = sub["amount"].as_f64().unwrap();
        let want = &sub["expect"];

        let x = run(tier, amount, invariant);
        assert_eq!(
            x.committed(),
            want["committed"].as_bool().unwrap(),
            "tier {tier}, amount {amount}: committed?"
        );
        match want["rule"].as_str() {
            None => assert!(x.result.constraint_violated.is_none()),
            Some(rule) => assert_eq!(
                x.result.constraint_violated.as_deref(),
                Some(rule),
                "tier {tier}, amount {amount}: which conjunct refused"
            ),
        }
    }
}

/// ★★ An unauthorized call changes nothing — the same guarantee an
/// enforcement-gate refusal gives, for an entirely different reason.
#[test]
fn an_unauthorized_call_leaves_state_byte_unchanged() {
    let before = json!({ "finances": { "liquid": { "balance": 500.0 }, "pockets": {},
                                       "income": { "monthly_total": 0.0, "sources": [] } } });
    let x = run(3, 100.0, "finances.liquid.balance >= 0");
    assert!(!x.committed());
    assert_eq!(x.result.constraint_violated.as_deref(), Some("insufficient_privilege"));
    assert!(x.mutations.is_empty(), "no mutation");
    assert!(x.events.is_empty(), "no event");
    assert_eq!(x.state, before, "state byte-unchanged");
}

/// ★ The conjunct is decided BEFORE the guard, so an unauthorized caller is
/// never told what the state would have done.
#[test]
fn authority_is_decided_before_the_guard() {
    // An amount the guard itself would reject, from a principal who may not act
    // at all. The authorization refusal is the one that comes back.
    let x = run(3, -100.0, "finances.liquid.balance >= 0");
    assert_eq!(x.result.constraint_violated.as_deref(), Some("insufficient_privilege"));
}
