//! Capabilities and the confused deputy, replayed against their conformance
//! vectors (R2 · Immune §IV · IMM-7's third clause, plus IMM-6's `skins`).
//!
//! **SPEC vectors, Rust-only.** ★★ Two of IMM-7's three clauses were already
//! built when this slice opened — the approval token (OPV-30/31) and
//! `permitted(α,o,Σ)` with per-edge privileges (`principal.rs`), both
//! 2026-08-12. The reconcile is recorded in the vector rather than the code
//! rebuilt. What is new here is the clause neither side had: **authority that
//! travels WITH the request**.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    capability::{Amplification, Attenuation, Capability, Rights},
    operator::{execute_as, Authorization, Enforcement, Registry},
    principal::{
        effective_privilege, effective_privilege_with, Denial, MembershipEdge, Memberships, Skin,
        SkinRegistry, Tier, TIER_CONTRIBUTOR, TIER_MEMBER, TIER_OBSERVER, TIER_OWNER,
    },
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("capability.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "capability.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn edge(principal: &str, sustain: &str, tier: Tier) -> MembershipEdge {
    MembershipEdge { principal: principal.into(), sustain: sustain.into(), tier, skin: None }
}

fn skinned(principal: &str, sustain: &str, tier: Tier, skin: &str) -> MembershipEdge {
    MembershipEdge {
        principal: principal.into(),
        sustain: sustain.into(),
        tier,
        skin: Some(skin.into()),
    }
}

/// Bonnie owns both his habitat and the household that contains it — the shape
/// that makes a deputy dangerous.
fn deputy_setup() -> Memberships {
    let mut m = Memberships::new();
    m.grant(edge("bonnie", "house", TIER_OWNER));
    m.grant(edge("bonnie", "habitat", TIER_OWNER));
    m
}

fn allowed() -> Vec<String> {
    vec!["budget.allocate".to_string(), "budget.summary".to_string()]
}

fn armed() -> Enforcement {
    Enforcement { enabled: true, ..Default::default() }
}

fn homestead_state() -> Value {
    json!({
        "finances": {
            "liquid": {"balance": 1000.0},
            "pockets": {"food": {"allocated": 0.0, "spent": 0.0, "limit": 0.0}}
        }
    })
}

fn spend_params() -> Map<String, Value> {
    let mut p = Map::new();
    p.insert("pocket_name".into(), json!("food"));
    p.insert("amount".into(), json!(10.0));
    p.insert("period".into(), json!("monthly"));
    p
}

// ── confused deputy ──────────────────────────────────────────────────────────

#[test]
fn ambient_authority_lets_the_deputy_reach_an_unauthorised_object() {
    // Asserted so the fix has something to be a fix OF. The untrusted input
    // names the sustain; ambient authority cannot tell which object the task
    // was about, so the call goes through against the household.
    let m = deputy_setup();
    let reg = Registry::default();
    let untrusted = vec!["house".to_string()];

    let ex = execute_as(
        &reg,
        &allowed(),
        &armed(),
        &homestead_state(),
        "budget.allocate",
        &spend_params(),
        &Authorization::Principal {
            id: "bonnie",
            memberships: &m,
            path: &untrusted,
            skins: SkinRegistry::none(),
        },
    );
    assert!(ex.committed(), "Hardy's compiler, in one call");
}

#[test]
fn a_capability_refuses_the_same_deputy_the_same_input() {
    let m = deputy_setup();
    let reg = Registry::default();

    let cap = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat")
        .unwrap()
        .attenuate(&Attenuation::to_rights(Rights::only(["budget.allocate"])))
        .unwrap();

    let before = homestead_state();
    let ex = execute_as(
        &reg,
        &allowed(),
        &armed(),
        &before,
        "budget.allocate",
        &spend_params(),
        &Authorization::Capability { capability: &cap, target: "house" },
    );

    assert!(!ex.committed());
    assert_eq!(ex.result.constraint_violated.as_deref(), Some("capability_designates"));
    assert!(ex.mutations.is_empty(), "a refusal changes nothing");
    assert!(ex.events.is_empty());
    assert_eq!(ex.state, before, "state byte-identical");
}

#[test]
fn the_authorised_task_is_unaffected() {
    // A fix that breaks the legitimate call is an outage, not a fix.
    let m = deputy_setup();
    let reg = Registry::default();
    let cap = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat")
        .unwrap()
        .attenuate(&Attenuation::to_rights(Rights::only(["budget.allocate"])))
        .unwrap();

    let ex = execute_as(
        &reg,
        &allowed(),
        &armed(),
        &homestead_state(),
        "budget.allocate",
        &spend_params(),
        &Authorization::Capability { capability: &cap, target: "habitat" },
    );
    assert!(ex.committed());
}

#[test]
fn a_capability_narrowed_to_one_operator_refuses_the_others() {
    let m = deputy_setup();
    let reg = Registry::default();
    let cap = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat")
        .unwrap()
        .attenuate(&Attenuation::to_rights(Rights::only(["budget.summary"])))
        .unwrap();

    let ex = execute_as(
        &reg,
        &allowed(),
        &armed(),
        &homestead_state(),
        "budget.allocate",
        &spend_params(),
        &Authorization::Capability { capability: &cap, target: "habitat" },
    );
    assert!(!ex.committed());
    assert_eq!(
        ex.result.constraint_violated.as_deref(),
        Some("capability_carries"),
        "wrong-object and wrong-operator are different repairs"
    );
}

// ── attenuation ──────────────────────────────────────────────────────────────

#[test]
fn attenuation_weakens_and_refuses_to_strengthen() {
    let m = deputy_setup();
    let owner = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat").unwrap();
    let weaker = owner.attenuate(&Attenuation::to_tier(TIER_CONTRIBUTOR)).unwrap();

    assert_eq!(
        weaker.attenuate(&Attenuation::to_tier(TIER_OWNER)),
        Err(Amplification::Tier { held: TIER_CONTRIBUTOR, requested: TIER_OWNER }),
        "refused and named, never silently bounded"
    );
}

#[test]
fn rights_narrow_and_cannot_widen() {
    let m = deputy_setup();
    let all = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat").unwrap();
    let narrowed = all
        .attenuate(&Attenuation::to_rights(Rights::only(["budget.spend"])))
        .unwrap();

    assert_eq!(
        narrowed.attenuate(&Attenuation::to_rights(Rights::All)),
        Err(Amplification::Rights)
    );
    // The sideways case a naive length check would let through.
    assert_eq!(
        narrowed.attenuate(&Attenuation::to_rights(Rights::only(["budget.allocate"]))),
        Err(Amplification::Rights)
    );
}

#[test]
fn redesignation_is_unrepresentable() {
    // `Attenuation` carries no sustain, so there is no call to refuse. All that
    // remains to assert is that the designation survives every weakening.
    let m = deputy_setup();
    let cap = Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat").unwrap();
    let weakened = cap
        .attenuate(
            &Attenuation::to_tier(TIER_OBSERVER).and_rights(Rights::only(["budget.summary"])),
        )
        .unwrap();

    assert_eq!(weakened.sustain(), "habitat");
    assert!(weakened.permits("house", "budget.summary", TIER_OBSERVER).is_err());
}

#[test]
fn issuing_is_a_transfer_never_a_mint() {
    let m = deputy_setup();
    assert!(matches!(
        Capability::issue(&m, SkinRegistry::none(), "stranger", "habitat"),
        Err(Denial::NoEdge { .. })
    ));
}

#[test]
fn issuing_across_a_path_takes_the_weakest_link() {
    let mut m = Memberships::new();
    m.grant(edge("epha", "house", TIER_OBSERVER));
    m.grant(edge("epha", "habitat", TIER_OWNER));

    let nested = Capability::issue_across(
        &m,
        SkinRegistry::none(),
        "epha",
        &["house".to_string(), "habitat".to_string()],
    )
    .unwrap();

    assert_eq!(nested.sustain(), "habitat");
    assert_eq!(nested.tier(), TIER_OBSERVER, "issuing must not route around ⊕");
}

// ── skins ────────────────────────────────────────────────────────────────────

#[test]
fn a_skin_restricts_an_edge_that_would_otherwise_be_stronger() {
    let mut m = Memberships::new();
    m.grant(skinned("frankie", "house", TIER_OWNER, "guest"));
    let mut s = SkinRegistry::new();
    s.define(Skin { name: "guest".into(), tier: TIER_OBSERVER });

    assert_eq!(
        effective_privilege_with(&m, &s, "frankie", &["house".into()]).unwrap(),
        TIER_OBSERVER
    );
}

#[test]
fn a_skin_cannot_widen_an_edge_it_is_attached_to() {
    let mut m = Memberships::new();
    m.grant(skinned("kui", "house", TIER_OBSERVER, "admin"));
    let mut s = SkinRegistry::new();
    s.define(Skin { name: "admin".into(), tier: TIER_OWNER });

    assert_eq!(
        effective_privilege_with(&m, &s, "kui", &["house".into()]).unwrap(),
        TIER_OBSERVER,
        "a bundle is not a promotion route"
    );
}

#[test]
fn an_unresolvable_skin_refuses_rather_than_falling_back() {
    let mut m = Memberships::new();
    m.grant(skinned("mum", "house", TIER_OWNER, "gest"));

    assert!(
        matches!(
            effective_privilege_with(&m, SkinRegistry::none(), "mum", &["house".into()]),
            Err(Denial::UnresolvedSkin { .. })
        ),
        "falling back to the raw tier is how a typo becomes owner-by-accident"
    );
}

#[test]
fn an_edge_without_a_skin_is_unaffected_by_the_registry() {
    let mut m = Memberships::new();
    m.grant(edge("cira", "house", TIER_MEMBER));
    let mut s = SkinRegistry::new();
    s.define(Skin { name: "guest".into(), tier: TIER_OBSERVER });

    assert_eq!(
        effective_privilege(&m, "cira", &["house".into()]).unwrap(),
        effective_privilege_with(&m, &s, "cira", &["house".into()]).unwrap(),
    );
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The STEP-0 reconcile must stay on the record: two of three clauses were
    // already built, and rebuilding them would have been the real failure.
    let step0 = &doc["★★_the_STEP_0_reconcile_what_was_ALREADY_THERE"];
    for key in [
        "1_the_approval_token_is_BUILT_do_not_rebuild",
        "2_permitted_is_BUILT_TOO_and_this_is_the_correction",
        "★_3_which_tracker_row_was_stale",
        "4_what_was_genuinely_missing",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }

    // The counterweight, and it is two things.
    let cw = d["★_the_counterweight_TWO_things_and_the_first_is_at_genuine_parity"]
        .as_str()
        .unwrap();
    assert!(cw.contains("PBKDF2"), "authn parity must stay named");
    assert!(cw.contains("token_version"), "real revocation must stay named");
    assert!(cw.contains("SUS-7"), "the boundary substrate must stay named");

    // The reference's one real ownership check is credited, precisely.
    let real = d["★_the_reference_DOES_have_one_real_ownership_check_and_it_is_worth_naming_precisely"]
        .as_str()
        .unwrap();
    for term in ["_assert_owns_sustain", "single-owner", "binary", "route layer"] {
        assert!(real.contains(term), "missing credit detail: {term}");
    }

    // ★★ The false positive that would most mislead a re-grepper.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(
        fp.keys().any(|k| k.contains("skin (16 hits, ZERO real")),
        "the skin/asking false positive must stay named"
    );
    assert!(
        fp.keys().any(|k| k.contains("permitted (11 hits")),
        "the allow-list-is-not-a-principal-check conflation must stay named"
    );

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "UNFORGEABILITY HERE IS STRUCTURAL, NOT CRYPTOGRAPHIC",
        "THIS IS NOT REVOCATION",
        "NOTHING ISSUES CAPABILITIES TO SYMBIONTS YET",
        "HRU: THE SAFETY QUESTION IS UNDECIDABLE",
        "SKINS ARE TIER-ONLY",
        "THE APPROVAL TOKEN AND THE CAPABILITY ARE NOT UNIFIED",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = ["confused_deputy_cases", "attenuation_cases", "skin_cases"];
    let mut seen = BTreeSet::new();
    for group in groups {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 14, "every declared case must be present");
}
