//! Fixed and learned rules, and the bound the learned tier holds, replayed
//! against their conformance vectors (R2 · Immune §V · IMM-10, consuming
//! IMM-7's capability).
//!
//! **SPEC vectors, Rust-only.** ★★ The ParseRule system is **Python-only** —
//! `ParseRule`, `parse_rule`, `transducer`, `OTP` and `R_fixed` are all grep-0
//! in this core, checked before anything was designed. So this is the generic
//! mechanism §V is about, not a port: the split, the ordering, and the bound.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    capability::Capability,
    learned::{
        BoundError, FixedRule, IngressBound, LearnedRule, RuleSet, RuleTrust, Screening,
        TrustPolicy,
    },
    operator::{execute_as, Authorization, Enforcement, Registry},
    principal::{
        MembershipEdge, Memberships, SkinRegistry, TIER_CONTRIBUTOR, TIER_MEMBER, TIER_OBSERVER,
        TIER_OWNER,
    },
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("learned.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "learned.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn policy() -> TrustPolicy {
    TrustPolicy::declare(TIER_MEMBER, TIER_CONTRIBUTOR, TIER_OBSERVER)
}

/// What ingest may reach AT ALL. Small by intent (HRU).
fn bound() -> IngressBound {
    IngressBound::declare(["budget.record_income"])
}

fn memberships(principal: &str, tier: u8) -> Memberships {
    let mut m = Memberships::new();
    m.grant(MembershipEdge {
        principal: principal.into(),
        sustain: "habitat".into(),
        tier,
        skin: None,
    });
    m
}

fn base_capability() -> Capability {
    let m = memberships("bonnie", TIER_OWNER);
    Capability::issue(&m, SkinRegistry::none(), "bonnie", "habitat").unwrap()
}

fn allowed() -> Vec<String> {
    vec!["budget.record_income".to_string(), "budget.allocate".to_string()]
}

fn armed() -> Enforcement {
    Enforcement { enabled: true, ..Default::default() }
}

fn state() -> Value {
    json!({"finances": {
        "liquid": {"balance": 1000.0},
        "income": {"monthly_total": 0.0, "sources": []},
        "pockets": {"food": {"allocated": 0.0, "spent": 0.0, "limit": 0.0}}
    }})
}

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

// ── the fixed tier ───────────────────────────────────────────────────────────

fn set() -> RuleSet {
    RuleSet::new(bound(), policy())
        .with_fixed(FixedRule::new("otp", "one-time password"))
        .with_learned(LearnedRule::new(
            "kcb_receive",
            "received",
            "budget.record_income",
            RuleTrust::Shipped,
        ))
}

#[test]
fn the_fixed_tier_runs_first_and_the_order_is_not_a_parameter() {
    // Matches BOTH tiers. There is no `screen_learned_first`, so this cannot
    // come out the other way round.
    let s = set().with_learned(LearnedRule::new(
        "greedy",
        "password",
        "budget.record_income",
        RuleTrust::Shipped,
    ));
    assert_eq!(
        s.screen("Your one-time password is 123456, received now"),
        Screening::Rejected { by: "otp".into() }
    );
}

#[test]
fn the_fixed_tier_is_a_fixed_point_of_every_edit_kind() {
    // ★★ `Edit::apply` is `(&Definition) -> Definition`: no parameter through
    // which a rule set could enter. Exercised, not argued from the signature.
    use sustena_core::editing::{Definition, Edit};
    use sustena_core::schema::{DimType, Schema};

    let s = set();
    let before = s.fixed().to_vec();
    let d = Definition::new(Schema::new().declare("x", DimType::Number { lo: None, hi: None }))
        .with_invariant("i", "x >= 0")
        .with_operator("budget.record_income");

    for e in [
        Edit::AddDim {
            name: "y".into(),
            ty: DimType::Number { lo: None, hi: None },
            default: json!(0),
        },
        Edit::RetypeDim { name: "x".into(), ty: DimType::Any },
        Edit::RetireDim { name: "x".into() },
        Edit::AddInv { id: "j".into(), expression: "x >= 1".into() },
        Edit::DropInv { id: "i".into() },
        Edit::ModifyInv { id: "i".into(), expression: "x >= 2".into() },
        Edit::AddOp { name: "budget.allocate".into() },
        Edit::RetireOp { name: "budget.record_income".into() },
    ] {
        let _ = e.apply(&d);
    }
    assert_eq!(s.fixed(), before.as_slice(), "∀e, ∀r ∈ R_fixed: e(r) = r");
}

#[test]
fn a_fixed_rule_has_no_capability_and_asking_is_an_error() {
    let s = set();
    assert_eq!(
        s.capability_for_fixed(&s.fixed()[0]),
        Err(BoundError::FixedRulesDoNotAct { rule_id: "otp".into() }),
        "an empty capability would read as bounded where the truth is not-applicable"
    );
}

#[test]
fn unmatched_is_a_different_answer_from_rejected() {
    assert_eq!(set().screen("nothing here"), Screening::Unmatched);
}

// ── trust as a real input ────────────────────────────────────────────────────

#[test]
fn trust_decides_precedence_when_two_learned_rules_match() {
    let s = RuleSet::new(bound(), policy())
        .with_learned(LearnedRule::new(
            "weak",
            "paid",
            "budget.record_income",
            RuleTrust::ProposedConfirmed,
        ))
        .with_learned(LearnedRule::new(
            "strong",
            "paid",
            "budget.record_income",
            RuleTrust::Shipped,
        ));

    match s.screen("paid to naivas") {
        Screening::Matched { rule_id, trust, .. } => {
            assert_eq!(rule_id, "strong", "declaration order says weak; trust says strong");
            assert_eq!(trust, RuleTrust::Shipped);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn trust_decides_the_privilege_ceiling() {
    let s = RuleSet::new(bound(), policy())
        .with_learned(LearnedRule::new(
            "a",
            "x",
            "budget.record_income",
            RuleTrust::Shipped,
        ))
        .with_learned(LearnedRule::new(
            "b",
            "x",
            "budget.record_income",
            RuleTrust::ProposedConfirmed,
        ));
    let base = base_capability();

    let strong = s.capability_for(&s.learned()[0], &base).unwrap();
    let weak = s.capability_for(&s.learned()[1], &base).unwrap();
    assert!(weak.tier() > strong.tier(), "confirming is cheaper than authoring");
}

#[test]
fn ties_keep_the_declared_order_stably() {
    let s = RuleSet::new(bound(), policy())
        .with_learned(LearnedRule::new("first", "x", "budget.record_income", RuleTrust::Shipped))
        .with_learned(LearnedRule::new("second", "x", "budget.record_income", RuleTrust::Shipped));
    assert_eq!(
        s.by_precedence().iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["first", "second"]
    );
}

#[test]
fn one_declared_policy_drives_both_decisions() {
    // Precedence and privilege both derive from `ceiling_for`, so they cannot
    // drift apart into two orderings that disagree.
    let s = RuleSet::new(bound(), policy())
        .with_learned(LearnedRule::new(
            "weak",
            "x",
            "budget.record_income",
            RuleTrust::ProposedConfirmed,
        ))
        .with_learned(LearnedRule::new(
            "strong",
            "x",
            "budget.record_income",
            RuleTrust::Shipped,
        ));
    let base = base_capability();

    let ordered = s.by_precedence();
    let first = s.capability_for(ordered[0], &base).unwrap();
    let second = s.capability_for(ordered[1], &base).unwrap();
    assert!(first.tier() <= second.tier(), "precedence order is ceiling order");
}

// ── least privilege, at the real gate ────────────────────────────────────────

#[test]
fn an_ingest_tier_rule_is_refused_at_the_gate_for_an_out_of_bound_operator() {
    // ★★★ THE KEYSTONE, end to end. The learned rule holds a capability
    // attenuated to the ingress bound; `budget.allocate` is outside it, so the
    // gate refuses even though the sustain itself allows the operator.
    let reg = Registry::default();
    let s = set();
    let cap = s.capability_for(&s.learned()[0], &base_capability()).unwrap();

    let before = state();
    let ex = execute_as(
        &reg,
        &allowed(),
        &armed(),
        &before,
        "budget.allocate",
        &params(&[
            ("pocket_name", json!("food")),
            ("amount", json!(10.0)),
            ("period", json!("monthly")),
        ]),
        &Authorization::Capability { capability: &cap, target: "habitat" },
    );

    assert!(!ex.committed());
    assert_eq!(ex.result.constraint_violated.as_deref(), Some("capability_carries"));
    assert!(ex.mutations.is_empty(), "a refusal changes nothing");
    assert!(ex.events.is_empty());
    assert_eq!(ex.state, before, "state byte-identical");
}

#[test]
fn the_in_bound_operator_still_runs() {
    // A bound that also blocks the legitimate call is an outage, not a bound.
    let reg = Registry::default();
    let s = set();
    let cap = s.capability_for(&s.learned()[0], &base_capability()).unwrap();

    let ex = execute_as(
        &reg,
        &allowed(),
        &armed(),
        &state(),
        "budget.record_income",
        &params(&[
            ("amount", json!(500.0)),
            ("source", json!("wages")),
            ("entry_id", json!("e1")),
        ]),
        &Authorization::Capability { capability: &cap, target: "habitat" },
    );
    assert!(ex.committed());
}

#[test]
fn a_rule_outside_the_ingress_bound_gets_no_capability_at_all() {
    let s = RuleSet::new(bound(), policy()).with_learned(LearnedRule::new(
        "sneaky",
        "x",
        "egress.prepare_household_summary",
        RuleTrust::Shipped,
    ));
    assert_eq!(
        s.capability_for(&s.learned()[0], &base_capability()),
        Err(BoundError::OutsideIngressBound {
            rule_id: "sneaky".into(),
            operator: "egress.prepare_household_summary".into(),
        })
    );
}

#[test]
fn no_trust_level_lifts_the_ingress_bound() {
    // ★★★ A ceiling with an exception is not a ceiling.
    let s = RuleSet::new(bound(), policy());
    let base = base_capability();
    for trust in [RuleTrust::Shipped, RuleTrust::UserCorrected, RuleTrust::ProposedConfirmed] {
        let rule = LearnedRule::new("r", "x", "budget.record_income", trust);
        let cap = s.capability_for(&rule, &base).unwrap();
        assert!(cap.rights().carries("budget.record_income"));
        assert!(!cap.rights().carries("budget.allocate"), "{trust:?} reached outside the bound");
        assert!(!cap.rights().carries("egress.prepare_household_summary"));
    }
}

#[test]
fn an_empty_bound_reaches_nothing_at_all() {
    let s = RuleSet::new(IngressBound::nothing(), policy());
    let rule = LearnedRule::new("r", "x", "budget.record_income", RuleTrust::Shipped);
    assert!(matches!(
        s.capability_for(&rule, &base_capability()),
        Err(BoundError::OutsideIngressBound { .. })
    ));
}

#[test]
fn the_bound_never_exceeds_the_principal_it_acts_for() {
    // A capability is a transfer: the policy sets a ceiling, never a floor.
    let m = memberships("guest", TIER_OBSERVER);
    let base = Capability::issue(&m, SkinRegistry::none(), "guest", "habitat").unwrap();
    let s = RuleSet::new(bound(), policy());

    // Shipped's ceiling is MEMBER — stronger than the base holder.
    let rule = LearnedRule::new("r", "x", "budget.record_income", RuleTrust::Shipped);
    let cap = s.capability_for(&rule, &base).unwrap();
    assert_eq!(cap.tier(), TIER_OBSERVER, "the weaker of the two wins");
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-ahead-of-python");

    let step0 = &doc["★★_the_STEP_0_reconcile_ParseRules_in_Rust_and_what_§V_already_has"];
    for key in [
        "1_ParseRules_do_NOT_exist_in_this_core",
        "2_so_the_scope_is_the_GENERIC_mechanism",
        "★_3_what_§V_already_has_and_is_NOT_rebuilt",
        "★★_4_a_related_but_DIFFERENT_thing_that_DOES_exist_in_Rust",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }

    // ★★ The fixed tier is the reference's best work and must stay credited in
    // full — both layers, and the non-authorability.
    let cw = d["★★_the_counterweight_the_FIXED_LEARNED_SPLIT_IS_AT_PARITY_AND_THE_FIXED_TIER_IS_THE_BEST_THING_THERE"]
        .as_str()
        .unwrap();
    for term in [
        "contains_sensitive_secret",
        "transducer.py:894",
        "ingest_engine.py:365",
        "non-authorable",
        "Adoption gating is real",
    ] {
        assert!(cw.contains(term), "missing counterweight detail: {term}");
    }

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(
        fp.keys().any(|k| k.contains("trust (47 hits, and ZERO of them are decisions)")),
        "the 47-mentions-zero-decisions finding must stay named"
    );
    assert!(
        fp.keys().any(|k| k.contains("R_fixed (grep-0, but the THING is real")),
        "the misleading zero must stay named"
    );
    assert!(
        fp.keys().any(|k| k.contains("Trust in RUST")),
        "source trust vs rule trust must stay distinguished"
    );

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "NO MATCHER LIBRARY, AND THAT IS THE POINT",
        "ADOPTION GATING IS NOT BUILT HERE",
        "ADOPTION AND AUTHORITY ARE DELIBERATELY SEPARATE",
        "THE TRUST→TIER MAPPING IS DECLARED, NOT DERIVED",
        "NOTHING HANDS THE CAPABILITY TO A RUNNING RULE ENGINE",
        "THE INGRESS BOUND IS PER-RULE-SET, NOT PER-SOURCE",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["fixed_tier_cases", "trust_as_input_cases", "least_privilege_cases"] {
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
