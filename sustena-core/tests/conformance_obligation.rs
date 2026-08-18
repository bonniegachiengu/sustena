//! The author-time obligation `g ⟹ wp(e, Q)`, replayed against its
//! conformance vectors (R2 · Operator §III · OP-3).
//!
//! **SPEC vectors, Rust-only — with the DYNAMIC half at parity.** The
//! reference evaluates `post_constraints` per call against the candidate,
//! exactly as this core does, and §III itself says *"the honest position for
//! Sustena today is the dynamic one, with the static obligation named as the
//! thing to want."* The gap is not that postconditions go unchecked; it is
//! that nothing asks, once, whether the guard could ever have guaranteed them.
//!
//! ★ And it **flags rather than blocks** — §III asks for a diagnostic, and the
//! per-call check is untouched.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    audit, check_obligation, execute, flagged, Change, EffectSummary, Enforcement, OperatorMeta,
    OperatorResult, Registry, Soundness, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("obligation.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "obligation.json was written for a different contract version"
    );
    doc
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap_or_else(|| panic!("group {group} missing"))
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("case {group}/{name} missing"))
}

// ── fixture, from the vector ────────────────────────────────────────────────

/// The declared constant effect — the one shape the fragment can summarise.
fn effect(doc: &Value) -> EffectSummary {
    let e = doc["fixture"]["effect"].as_array().unwrap();
    assert_eq!(e[1].as_str().unwrap(), "SetTo");
    EffectSummary::new().with(
        e[0].as_str().unwrap(),
        Change::SetTo(json!(e[2].as_f64().unwrap())),
    )
}

fn meta(post: &[&str], guard: &[&str], eff: Option<EffectSummary>) -> OperatorMeta {
    OperatorMeta {
        name: "test.force_negative",
        description: "Force a pocket negative. Exists to prove the gate refuses it.",
        params: vec![],
        constraints: guard.iter().map(|s| s.to_string()).collect(),
        post_constraints: post.iter().map(|s| s.to_string()).collect(),
        side_effects: vec![],
        pawa_cost: 0,
        protocol: sustena_core::operator::meta::Protocol::Rpc,
        min_privilege: 0,
        effect: eff,
        run: |_s, _p, _e, _m| OperatorResult::ok(json!({})),
    }
}

// ── ★★ the worked proof ─────────────────────────────────────────────────────

/// ★★ §III's own sentence, executed.
#[test]
fn an_operator_whose_effect_defeats_its_own_postcondition_is_flagged() {
    let doc = load();
    let c = case(
        &doc,
        "obligation_cases",
        "★★_an_operator_whose_effect_defeats_its_own_postcondition_is_FLAGGED",
    );
    let e = &c["expect"];
    let impossible = doc["fixture"]["impossible_post"].as_str().unwrap();

    let report = check_obligation(&meta(&[impossible], &[], Some(effect(&doc))));
    assert_eq!(report.is_flagged(), e["flagged"].as_bool().unwrap());
    assert_eq!(report.flagged().len(), e["count"].as_u64().unwrap() as usize);

    let d = report.flagged()[0].describe();
    for term in e["describe_contains"].as_array().unwrap() {
        assert!(d.contains(term.as_str().unwrap()), "{d}");
    }
    // The flagged finding names the postcondition it is about.
    assert_eq!(report.flagged()[0].postcondition(), impossible);
}

/// ★ The verdict isolates to the postcondition.
#[test]
fn the_same_operator_with_an_achievable_postcondition_is_sound() {
    let doc = load();
    let c = case(
        &doc,
        "obligation_cases",
        "★_the_SAME_operator_with_an_achievable_postcondition_is_sound",
    );
    let ok_post = doc["fixture"]["guaranteed_post"].as_str().unwrap();
    let report = check_obligation(&meta(&[ok_post], &[], Some(effect(&doc))));
    assert_eq!(report.is_flagged(), c["expect"]["flagged"].as_bool().unwrap());
    assert!(matches!(report.findings[0], Soundness::Sound { .. }));
    assert_eq!(c["expect"]["soundness"].as_str().unwrap(), "Sound");
}

/// ★★ The third outcome, and the reason there are three.
#[test]
fn an_unsummarised_effect_is_unavailable_neither_trusted_nor_flagged() {
    let doc = load();
    let c = case(
        &doc,
        "obligation_cases",
        "★★_an_unsummarised_effect_is_UNAVAILABLE_neither_trusted_nor_flagged",
    );
    let e = &c["expect"];
    let impossible = doc["fixture"]["impossible_post"].as_str().unwrap();

    // Same postcondition that was FLAGGED above — but with no summary, the
    // static claim is unavailable rather than either verdict.
    let report = check_obligation(&meta(&[impossible], &[], None));
    assert_eq!(report.is_flagged(), e["flagged"].as_bool().unwrap());
    assert_eq!(report.unavailable(), e["unavailable"].as_u64().unwrap() as usize);
    assert!(report.findings[0]
        .describe()
        .contains(e["describe_contains"].as_str().unwrap()));
}

#[test]
fn a_param_driven_change_is_opaque_and_therefore_unavailable() {
    let doc = load();
    let c = case(
        &doc,
        "obligation_cases",
        "a_param_driven_change_is_opaque_and_therefore_unavailable",
    );
    let e = &c["expect"];
    let by_param = EffectSummary::new().with("finances.liquid.balance", Change::Opaque);
    let report = check_obligation(&meta(&["finances.liquid.balance >= 0"], &[], Some(by_param)));
    assert_eq!(report.is_flagged(), e["flagged"].as_bool().unwrap());
    assert_eq!(report.unavailable(), e["unavailable"].as_u64().unwrap() as usize);
}

#[test]
fn an_operator_with_no_postcondition_has_nothing_to_be_unsound_about() {
    let doc = load();
    let c = case(
        &doc,
        "obligation_cases",
        "an_operator_with_no_postcondition_has_nothing_to_be_unsound_about",
    );
    let e = &c["expect"];
    let report = check_obligation(&meta(&[], &["params.amount > 0"], None));
    assert_eq!(report.findings.len(), e["findings"].as_u64().unwrap() as usize);
    assert_eq!(report.is_flagged(), e["flagged"].as_bool().unwrap());
    assert!(report.describe().contains("declares no postcondition"));
}

// ── ★★ the finding ──────────────────────────────────────────────────────────

/// ★★ Zero operators proven either way — the check demonstrating §III's own
/// claim rather than contradicting it.
#[test]
fn every_operator_this_core_ships_is_currently_unavailable() {
    let doc = load();
    let c = case(
        &doc,
        "registry_cases",
        "★★_every_operator_this_core_ships_is_currently_UNAVAILABLE",
    );
    let e = &c["expect"];
    let reg = Registry::default();
    let reports = audit(&reg);

    assert_eq!(!reports.is_empty(), e["reports_nonempty"].as_bool().unwrap());
    assert_eq!(flagged(&reg).len(), e["flagged"].as_u64().unwrap() as usize);
    for r in &reports {
        assert_eq!(
            r.findings.len(),
            r.unavailable(),
            "{} has a finding that is not Unavailable — the registry has changed, and this \
             assertion exists so someone looks",
            r.operator
        );
    }
    assert!(e["all_findings_unavailable"].as_bool().unwrap());
}

// ── ★★ the posture ──────────────────────────────────────────────────────────

/// ★★ It flags, and it does not block — both halves of §III in one case.
#[test]
fn an_unsound_operator_is_flagged_at_author_time_and_still_runs() {
    let doc = load();
    let c = case(
        &doc,
        "posture_cases",
        "★★_an_unsound_operator_is_FLAGGED_at_author_time_and_STILL_RUNS",
    );
    let e = &c["expect"];
    let impossible = doc["fixture"]["impossible_post"].as_str().unwrap();

    // A real operator that genuinely does what its effect summary claims.
    let mut unsound = meta(&[impossible], &[], Some(effect(&doc)));
    unsound.name = "test.unsound";
    unsound.run = |state, _p, _e, _m| {
        let _ = state.set("finances.pockets.food.allocated", json!(-999.0));
        OperatorResult::ok(json!({}))
    };

    // Static: flagged.
    assert_eq!(
        check_obligation(&unsound).is_flagged(),
        e["statically_flagged"].as_bool().unwrap()
    );

    // Dynamic: it RUNS, and the post-condition catches it after the fact.
    let mut reg = Registry::default();
    reg.register(unsound);
    let before = json!({"finances": {"pockets": {"food": {"allocated": 10.0}}}});
    let ex = execute(
        &reg, &reg.names(), &Enforcement::default(), &before,
        "test.unsound", &serde_json::Map::new(),
    );
    assert!(!ex.committed(), "the dynamic check should catch it");
    assert!(ex
        .result
        .reason
        .as_deref()
        .unwrap_or("")
        .contains("post-condition"));
    assert_eq!(ex.state, before, "a refusal changes nothing");
    assert!(e["still_runs_and_is_caught_dynamically"].as_bool().unwrap());

    // ★ And the flag is not on the execution path: a SOUND operator of the
    //   identical shape commits normally.
    let mut sound = meta(
        &[doc["fixture"]["guaranteed_post"].as_str().unwrap()],
        &[],
        Some(effect(&doc)),
    );
    sound.name = "test.sound";
    sound.run = |state, _p, _e, _m| {
        let _ = state.set("finances.pockets.food.allocated", json!(-999.0));
        OperatorResult::ok(json!({}))
    };
    assert!(!check_obligation(&sound).is_flagged());
    let mut reg2 = Registry::default();
    reg2.register(sound);
    let ok = execute(
        &reg2, &reg2.names(), &Enforcement::default(), &before,
        "test.sound", &serde_json::Map::new(),
    );
    assert_eq!(ok.committed(), e["sound_variant_commits"].as_bool().unwrap(), "{:?}", ok.result);
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ The dynamic half is at parity, and the article agrees it should be.
    let cw = d["★_the_counterweight_the_DYNAMIC_half_is_at_parity_and_the_article_says_it_is_the_honest_position"]
        .as_str()
        .unwrap();
    assert!(cw.contains("sustain_engine.py:497"));
    assert!(cw.contains("checks them exactly when the article says to"));

    // ★ The near-miss: an author-time discipline exists, unapplied here.
    let near = d["★_a_near_miss_worth_naming_the_reference_DOES_have_an_author_time_discipline_elsewhere"]
        .as_str()
        .unwrap();
    assert!(near.contains("Γ ⊢ r"));
    assert!(near.contains("unapplied one"));

    // ★★ The registry finding.
    let found = d["★★_what_running_it_over_the_real_registry_FOUND"].as_str().unwrap();
    assert!(found.contains("zero operators proven either way"));
    assert!(found.contains("running it is what DEMONSTRATES it"));

    // The Hoare shape is credited as parity so the gap is not overstated.
    let shape = d["term_by_term"]["the Hoare triple's shape"].as_str().unwrap();
    assert!(shape.contains("AT PARITY"));

    // Four limits, with the posture and the summary-can-be-wrong among them.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "IT FLAGS, IT DOES NOT BLOCK",
        "THE FRAGMENT IS THE FRAGMENT",
        "PARTIAL CORRECTNESS ONLY",
        "A SUMMARY IS A DECLARATION AND CAN BE WRONG",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    // The adjacent fail-open difference is attributed elsewhere, not claimed.
    let what = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(what.contains("not something this slice closed"));

    let groups = ["obligation_cases", "registry_cases", "posture_cases"];
    let mut seen = BTreeSet::new();
    for group in groups {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(
                seen.insert(c["name"].as_str().unwrap().to_string()),
                "duplicate case name"
            );
        }
    }
}
