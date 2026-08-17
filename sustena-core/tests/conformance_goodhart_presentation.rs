//! The Goodhart guard and present-the-frontier, replayed against their
//! conformance vectors (R2 · Operative §XIV + §XV · OPV-27, OPV-29).
//!
//! **SPEC vectors, Rust-only** — with two counterweights that are real and
//! specific, one per half.
//!
//! For §XIV: the **mechanism** the fix uses is at parity.
//! `_check_enforcement_gate` exists in the reference and refuses a transition
//! that would violate a declared invariant. It could enforce a Goodhart guard
//! today; what it lacks is the notion of `U` that says *where* one is needed.
//!
//! For §XV: the **disclosure instinct** is at parity. `knapsack_select`
//! already returns `(selected, excluded)` *"both with their score"*, and the
//! Orchie surface renders what stayed quiet. What it lacks is a vector — its
//! salience is `0.75·urgency + 0.25·relevance`, a weighted sum, which is
//! precisely the scalarisation §II names. The disclosure is honest; the thing
//! being disclosed has already been collapsed.
//!
//! The proof of value: damage to an unwatched dimension that **every**
//! operative reads as no change at all, caught only by the gate — and a
//! surface that hands back the whole frontier and cannot be reduced to one
//! option without naming a rule and reporting what it hid.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    coverage, present, present_joint, referenced_dimensions, typecheck_constraint, Alternative,
    CollapseRule, ConstraintDecl, Coverage, Cynefin, Discharge, Duty, GoodhartGuard, GuardError,
    Objective, Omega, Operative, PresentationError, Sense, Shared, StatePoint, Strategy, Utility,
    Verdict, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("goodhart_presentation.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "goodhart_presentation.json was written for a different contract version"
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

fn strings(v: &Value) -> BTreeSet<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

// ── fixture ─────────────────────────────────────────────────────────────────

fn world(doc: &Value) -> Shared {
    let mut w = Shared::new();
    for d in doc["fixture"]["dimensions"].as_array().unwrap() {
        w = w.with_dimension(d.as_str().unwrap());
    }
    w
}

fn mentor(doc: &Value) -> Operative {
    let dim = doc["fixture"]["watched_by_mentor"].as_str().unwrap();
    Operative::new(
        "mentor",
        Utility::new()
            .with(Objective::new(dim, dim, Sense::Maximise))
            .unwrap(),
        &[Cynefin::Clear],
    )
    .unwrap()
}

fn omega(doc: &Value) -> Omega {
    Omega::over(world(doc)).with(mentor(doc)).unwrap()
}

fn guard_decl(doc: &Value) -> ConstraintDecl {
    let g = &doc["fixture"]["guard"];
    ConstraintDecl {
        id: g["id"].as_str().unwrap().to_string(),
        expression: g["expression"].as_str().unwrap().to_string(),
        strategy: Strategy::Refuse,
        conserved_dimensions: vec![],
    }
}

fn alternatives(doc: &Value) -> Vec<Alternative> {
    let alts = doc["fixture"]["alternatives"].as_object().unwrap();
    ["a", "b", "c", "d"]
        .iter()
        .map(|k| {
            let v = alts[*k].as_array().unwrap();
            Alternative::new(k, &[("x", v[0].as_f64().unwrap()), ("y", v[1].as_f64().unwrap())])
        })
        .collect()
}

fn both_objectives() -> Utility {
    Utility::new()
        .with(Objective::new("x", "x", Sense::Maximise))
        .unwrap()
        .with(Objective::new("y", "y", Sense::Maximise))
        .unwrap()
}

fn cov(doc: &Value) -> Coverage {
    coverage(&omega(doc))
}

// ── coverage ────────────────────────────────────────────────────────────────

#[test]
fn coverage_reports_o_and_u_and_nothing_else() {
    let doc = load();
    let c = case(&doc, "coverage_cases", "coverage_reports_O_and_U_and_nothing_else");
    let e = &c["expect"];
    let got = cov(&doc);
    assert_eq!(got.observed, strings(&e["observed"]));
    assert_eq!(got.unwatched, strings(&e["unwatched"]));
    assert_eq!(got.has_blind_spot(), e["has_blind_spot"].as_bool().unwrap());
    assert!((got.observed_fraction() - e["observed_fraction"].as_f64().unwrap()).abs() < 1e-12);
    assert!(got.describe().contains("no agent's score changes"));
}

/// ★ §XIV's reason for refusing the obvious fix, in both directions.
#[test]
fn adding_an_operative_shrinks_u_without_emptying_it_and_a_new_dimension_refills_it() {
    let doc = load();
    let c = case(
        &doc,
        "coverage_cases",
        "★_adding_an_operative_shrinks_U_without_emptying_it_and_a_new_dimension_refills_it",
    );
    let e = &c["expect"];

    let before = cov(&doc);
    assert_eq!(before.unwatched.len(), e["unwatched_before"].as_u64().unwrap() as usize);

    let rested = Operative::new(
        "curator",
        Utility::new()
            .with(Objective::new("rest", "sleep", Sense::Maximise))
            .unwrap(),
        &[Cynefin::Complex],
    )
    .unwrap();
    let after = coverage(&omega(&doc).with(rested).unwrap());
    // U → O: it really did shrink...
    assert!(after.unwatched.len() < before.unwatched.len());
    // ...and it is not empty. A benefit, not a solution.
    assert_eq!(after.has_blind_spot(), e["still_has_blind_spot"].as_bool().unwrap());
    assert_eq!(after.unwatched, strings(&e["unwatched_after"]));

    // One step later: S gains a dimension, and a fresh U appears.
    let fresh = e["new_dimension_lands_in_U"].as_str().unwrap();
    let grown = Omega::over(world(&doc).with_dimension(fresh))
        .with(mentor(&doc))
        .unwrap();
    assert!(coverage(&grown).unwatched.contains(fresh));
}

// ── ★★ the guard ────────────────────────────────────────────────────────────

/// ★★ THE PROOF. Two-part, because §XIV's claim is two-part: invisible to
/// every score, caught only by the gate.
#[test]
fn damage_to_an_unwatched_dimension_is_seen_by_no_operative_and_caught_only_by_the_gate() {
    let doc = load();
    let c = case(
        &doc,
        "guard_cases",
        "★★_damage_to_an_unwatched_dimension_is_seen_by_no_operative_and_caught_only_by_the_gate",
    );
    let e = &c["expect"];
    let o = omega(&doc);
    let coverage = cov(&doc);
    assert!(coverage.unwatched.contains("sleep"));

    let as_point = |v: &Value| -> StatePoint {
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, x)| (k.clone(), x.as_f64().unwrap()))
            .collect()
    };
    let before = as_point(&c["before"]);
    let after = as_point(&c["after"]);

    // ★ Part one: no agent's score changes. Whatever operatives exist, none of
    //   them can rank against this, because none of them can see it.
    for m in o.members() {
        assert_eq!(
            m.utility().at(&before).unwrap(),
            m.utility().at(&after).unwrap(),
            "{} should be blind to sleep",
            m.id()
        );
        assert!(
            m.utility().delta(&before, &after).unwrap().iter().all(|d| *d == 0.0),
            "Δu should be the zero vector"
        );
    }
    assert!(e["every_operative_sees_no_change"].as_bool().unwrap());
    assert!(e["delta_is_zero_vector"].as_bool().unwrap());

    // ★ Part two: the gate refuses it, via an ordinary declared invariant.
    let guard = GoodhartGuard::declare(guard_decl(&doc), &coverage).unwrap();
    assert_eq!(guard.covers(), &strings(&e["guard_covers"]));

    let damaged = json!({"savings": 100, "sleep": 3});
    let verdict = guard.check(&damaged, &Map::new());
    match &verdict {
        Verdict::Refused { rule, .. } => {
            assert_eq!(e["verdict_on_damage"].as_str().unwrap(), "Refused");
            assert_eq!(rule, e["refusing_rule"].as_str().unwrap());
        }
        other => panic!("expected Refused, got {other:?}"),
    }

    // A floor, not a blanket refusal.
    assert_eq!(
        guard.check(&json!({"savings": 100, "sleep": 8}), &Map::new()),
        Verdict::Admit
    );
    assert_eq!(e["verdict_on_healthy"].as_str().unwrap(), "Admit");
}

#[test]
fn a_guard_that_watches_nothing_unwatched_is_refused() {
    let doc = load();
    let _ = case(&doc, "guard_cases", "a_guard_that_watches_nothing_unwatched_is_refused");
    let redundant = ConstraintDecl {
        id: "savings_floor".into(),
        expression: "savings >= 0".into(),
        strategy: Strategy::Refuse,
        conserved_dimensions: vec![],
    };
    assert!(matches!(
        GoodhartGuard::declare(redundant, &cov(&doc)),
        Err(GuardError::WatchesNothingUnwatched { .. })
    ));
}

#[test]
fn a_malformed_guard_is_refused_at_authoring_time() {
    let doc = load();
    let _ = case(&doc, "guard_cases", "a_malformed_guard_is_refused_at_authoring_time");
    let bad = ConstraintDecl {
        id: "nonsense".into(),
        expression: ">>> not a predicate".into(),
        strategy: Strategy::Refuse,
        conserved_dimensions: vec![],
    };
    assert!(matches!(
        GoodhartGuard::declare(bad, &cov(&doc)),
        Err(GuardError::Malformed { .. })
    ));
}

#[test]
fn referenced_dimensions_walks_the_whole_predicate() {
    let doc = load();
    let c = case(&doc, "guard_cases", "referenced_dimensions_walks_the_whole_predicate");
    let node = typecheck_constraint(&ConstraintDecl {
        id: "compound".into(),
        expression: c["expression"].as_str().unwrap().to_string(),
        strategy: Strategy::Refuse,
        conserved_dimensions: vec![],
    })
    .unwrap();
    assert_eq!(referenced_dimensions(&node), strings(&c["expect"]["references"]));
}

// ── ★★ the presentation ─────────────────────────────────────────────────────

/// ★★ THE PROOF for §XV: the surface keeps the whole trade-off.
#[test]
fn presentation_returns_the_frontier_not_a_winner() {
    let doc = load();
    let c = case(
        &doc,
        "presentation_cases",
        "★★_presentation_returns_the_frontier_not_a_winner",
    );
    let e = &c["expect"];
    let p = present(&both_objectives(), &alternatives(&doc)).unwrap();

    let want: Vec<String> = e["frontier"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(p.frontier(), want.as_slice());
    assert_eq!(p.dominated(), ["d".to_string()]);
    assert_eq!(p.choices(), e["choices"].as_u64().unwrap() as usize);
    assert_eq!(
        p.would_reinstate_the_collapse(),
        e["would_reinstate_the_collapse"].as_bool().unwrap()
    );
}

/// ★ §XV tied back to §II by a real computation.
#[test]
fn collapsing_deletes_the_same_option_a_weight_vector_would() {
    let doc = load();
    let c = case(
        &doc,
        "presentation_cases",
        "★_collapsing_deletes_the_same_option_a_weight_vector_would",
    );
    let p = present(&both_objectives(), &alternatives(&doc)).unwrap();
    let collapsed = p
        .collapse(CollapseRule::DeclaredPolicy {
            named: "highest x".into(),
            chose: "a".into(),
        })
        .unwrap();
    let e = &c["expect"];
    assert_eq!(collapsed.deleted_an_option(), e["deleted_an_option"].as_bool().unwrap());
    // 'c' — the non-convex option 1001 weight vectors deleted — is hidden here too.
    assert!(collapsed
        .hidden
        .contains(&e["hidden_contains"].as_str().unwrap().to_string()));
    assert!(collapsed.describe().contains("not shown"));
}

#[test]
fn sole_point_cannot_be_borrowed_to_justify_a_real_deletion() {
    let doc = load();
    let _ = case(
        &doc,
        "presentation_cases",
        "★_sole_point_cannot_be_borrowed_to_justify_a_real_deletion",
    );
    let p = present(&both_objectives(), &alternatives(&doc)).unwrap();
    assert!(matches!(
        p.collapse(CollapseRule::SolePoint),
        Err(PresentationError::NotASolePoint { .. })
    ));
}

#[test]
fn sole_point_is_honest_when_there_genuinely_is_one() {
    let doc = load();
    let c = case(&doc, "presentation_cases", "sole_point_is_honest_when_there_genuinely_is_one");
    let single = [Alternative::new("only", &[("x", 1.0), ("y", 1.0)])];
    let p = present(&both_objectives(), &single).unwrap();
    assert!(!p.would_reinstate_the_collapse());
    let collapsed = p.collapse(CollapseRule::SolePoint).unwrap();
    assert_eq!(collapsed.chosen, c["expect"]["chosen"].as_str().unwrap());
    assert_eq!(
        collapsed.deleted_an_option(),
        c["expect"]["deleted_an_option"].as_bool().unwrap()
    );
}

#[test]
fn a_person_choosing_within_the_frontier_is_the_one_reason_that_resolves_it() {
    let doc = load();
    let c = case(
        &doc,
        "presentation_cases",
        "a_person_choosing_within_the_frontier_is_the_one_reason_that_resolves_it",
    );
    let p = present(&both_objectives(), &alternatives(&doc)).unwrap();
    let e = &c["expect"];
    let collapsed = p
        .collapse(CollapseRule::HumanChose {
            chose: e["chosen"].as_str().unwrap().to_string(),
        })
        .unwrap();
    assert_eq!(collapsed.chosen, e["chosen"].as_str().unwrap());
    let hidden: Vec<String> = e["hidden"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(collapsed.hidden, hidden);
}

#[test]
fn choosing_a_dominated_option_is_refused() {
    let doc = load();
    let _ = case(&doc, "presentation_cases", "choosing_a_dominated_option_is_refused");
    let p = present(&both_objectives(), &alternatives(&doc)).unwrap();
    assert!(matches!(
        p.collapse(CollapseRule::HumanChose { chose: "d".into() }),
        Err(PresentationError::NotOnTheFrontier { .. })
    ));
}

#[test]
fn presenting_nothing_is_refused_rather_than_returning_an_empty_frontier() {
    let doc = load();
    let _ = case(
        &doc,
        "presentation_cases",
        "presenting_nothing_is_refused_rather_than_returning_an_empty_frontier",
    );
    assert!(matches!(
        present(&both_objectives(), &[]),
        Err(PresentationError::NothingToPresent)
    ));
}

/// ★ *Hold the whole* at the population level.
#[test]
fn the_joint_surface_keeps_an_option_only_one_operative_finds_non_dominated() {
    let doc = load();
    let c = case(
        &doc,
        "presentation_cases",
        "★_the_joint_surface_keeps_an_option_only_one_operative_finds_non_dominated",
    );
    let w = Shared::new().with_dimension("x").with_dimension("y");
    let xs = Operative::new(
        "x_only",
        Utility::new().with(Objective::new("x", "x", Sense::Maximise)).unwrap(),
        &[Cynefin::Clear],
    )
    .unwrap();
    let ys = Operative::new(
        "y_only",
        Utility::new().with(Objective::new("y", "y", Sense::Maximise)).unwrap(),
        &[Cynefin::Clear],
    )
    .unwrap();
    let omega = Omega::over(w).with(xs).unwrap().with(ys).unwrap();
    let p = present_joint(&omega.rank(&alternatives(&doc)).unwrap()).unwrap();

    for id in c["expect"]["frontier_contains"].as_array().unwrap() {
        assert!(
            p.frontier().contains(&id.as_str().unwrap().to_string()),
            "{} should survive the joint surface",
            id
        );
    }
    assert_eq!(
        p.would_reinstate_the_collapse(),
        c["expect"]["would_reinstate_the_collapse"].as_bool().unwrap()
    );
}

/// ★ The three duties, reported with what is NOT built.
#[test]
fn the_three_duties_are_recorded_with_what_is_not_built() {
    let doc = load();
    let c = case(
        &doc,
        "presentation_cases",
        "the_three_duties_are_recorded_with_what_is_NOT_built",
    );
    let e = &c["expect"];
    let d = sustena_core::discharged_by_this_build();
    assert_eq!(d.len(), Duty::ALL.len());

    let find = |want: Duty| *d.iter().find(|(x, _, _)| *x == want).unwrap();

    // Route moved NotBuilt → Partial when the mixture gate shipped.
    let (_, route, route_why) = find(Duty::Route);
    assert_eq!(route, Discharge::Partial);
    assert_eq!(e["route"].as_str().unwrap(), "Partial");
    assert!(route_why.contains(e["route_names"].as_str().unwrap()));
    assert!(route_why.contains("SUPPLIED"));

    let (_, whole, whole_why) = find(Duty::HoldTheWhole);
    assert_eq!(whole, Discharge::Partial);
    assert_eq!(e["hold_the_whole"].as_str().unwrap(), "Partial");
    assert!(whole_why.contains(e["hold_names"].as_str().unwrap()));
    // ★ σ̂ shipped and did NOT complete dom(s) — one input, not the whole
    // classification. Both duties wait on OPV-16.
    assert!(whole_why.contains("criticality signal ships"));
    assert!(whole_why.contains("different thing"));

    let (_, front, _) = find(Duty::PresentTheFrontier);
    assert_eq!(front, Discharge::Built);
    assert_eq!(e["present"].as_str().unwrap(), "Built");
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");
    let t = &d["term_by_term"];

    // ★ §XIV's counterweight: the MECHANISM is at parity, the concept is not.
    let fix = t["★ the fix is a gate-enforced invariant, NOT another operative"]
        .as_str()
        .unwrap();
    assert!(fix.contains("_check_enforcement_gate"));
    assert!(fix.contains("AT PARITY"));
    assert!(fix.contains("a concept, not a mechanism"));

    // ★ §XV's counterweight: the DISCLOSURE instinct is at parity.
    let disc = t["★ 'what did not make the cut travels with the answer'"].as_str().unwrap();
    assert!(disc.contains("knapsack_select"));
    assert!(disc.contains("AT PARITY"));
    assert!(disc.contains("already been collapsed"));

    // The near-miss is named precisely rather than dismissed.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    let near = fp["collapse (1 hit) — ★ a near-miss worth naming precisely, not dismissing"]
        .as_str()
        .unwrap();
    assert!(near.contains("Same word, different object"));
    assert!(near.contains("write path"));
    assert!(fp["top_k (2 hits, 0 real)"].as_str().unwrap().contains("top_key"));

    // OPV-28 was not built in that slice, and the note says which slice did.
    let moe = t["the sparse MoE gate g(x,s)"].as_str().unwrap();
    assert!(moe.contains("OPV-28"));

    // Four honest limits, all present.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "U IS COMPUTED AGAINST DECLARED DIMENSIONS",
        "ROOT-NAME GRANULARITY",
        "PRESENTATION IS NOT ORDERING",
        "ONE UTILITY PER PRESENTATION",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }
    // The granularity limit says which direction it errs in.
    assert!(limits.contains("SAFE direction"));

    let groups = ["coverage_cases", "guard_cases", "presentation_cases"];
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
