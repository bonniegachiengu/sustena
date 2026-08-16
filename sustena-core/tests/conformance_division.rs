//! Division of labour, replayed against its conformance vectors
//! (R2 · Multiparty §VII · MUL-12).
//!
//! **SPEC vectors, Rust-only.** Nothing in the reference assigns roles or
//! checks whether an arrangement would hold — and the word `role` appears
//! there sixteen times without once meaning this (an Anthropic Messages API
//! protocol field, three display labels, a loop variable, and a free-text
//! `role_in_family` a person types about themselves).
//!
//! The counterweight is real and specific: `r` — *the share of the benefit
//! that comes back to the contributor through the composed Sustain* — is
//! derived from the roll-up, and the roll-up is **at parity**.
//! `compute_rollup` already returns `included: [{member, value}]` per
//! aggregate, folding the parent's own contribution in. The reference computes
//! the numerator and the denominator, member by member, and never forms the
//! ratio — because it has nothing to spend `r` on.
//!
//! The proof of value: a split that is cheaper, covered and redundant, and
//! fails only `rb > c`, is refused — and the price of refusing it is reported
//! as a number.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    AssignmentOutcome, Division, DivisionError, Population, PopulationSpec, Role, SharedInterest,
    Unfillable, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("division.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "division.json was written for a different contract version"
    );
    doc
}

// ── fixture, built from the vector ──────────────────────────────────────────

fn stakes(doc: &Value) -> Vec<(String, f64)> {
    doc["fixture"]["stakes"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_f64().unwrap()))
        .collect()
}

fn interest(doc: &Value) -> SharedInterest {
    let s = stakes(doc);
    let refs: Vec<(&str, f64)> = s.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    SharedInterest::from_rollup(&refs).expect("real stakes")
}

fn members(doc: &Value) -> Vec<String> {
    stakes(doc).into_iter().map(|(k, _)| k).collect()
}

/// The baseline body: three roles, everyone able to afford everything.
fn body(doc: &Value) -> Division {
    let mut d = Division::new(interest(doc));
    for r in doc["fixture"]["roles"].as_array().unwrap() {
        d = d.with_role(
            Role::new(
                r["id"].as_str().unwrap(),
                r["b"].as_f64().unwrap(),
                r["rho"].as_u64().unwrap() as usize,
            )
            .expect("role"),
        );
    }
    let base = doc["fixture"]["baseline_costs"].as_object().unwrap();
    for m in members(doc) {
        for (role, c) in base {
            d.set_cost(&m, role, c.as_f64().unwrap()).expect("declared");
        }
    }
    d
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap_or_else(|| panic!("group {group} missing"))
        .iter()
        .find(|c| c["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("case {group}/{name} missing"))
}

// ── r ───────────────────────────────────────────────────────────────────────

#[test]
fn r_is_derived_from_the_rollup_and_sums_to_one() {
    let doc = load();
    let c = case(&doc, "shared_interest_cases", "r_is_derived_from_the_rollup_and_sums_to_one");
    let r = interest(&doc);
    assert!((r.total() - c["expect"]["total"].as_f64().unwrap()).abs() < 1e-12);
    assert!((r.of("ama").unwrap() - c["expect"]["ama"].as_f64().unwrap()).abs() < 1e-12);
    assert!((r.of("dee").unwrap() - c["expect"]["dee"].as_f64().unwrap()).abs() < 1e-12);
}

#[test]
fn a_single_member_body_has_r_of_one() {
    let doc = load();
    let c = case(&doc, "shared_interest_cases", "a_single_member_body_has_r_of_one");
    let r = SharedInterest::from_rollup(&[("solo", 500.0)]).expect("stake");
    assert_eq!(r.of("solo"), Some(c["expect"]["r"].as_f64().unwrap()));
}

#[test]
fn shares_cannot_exceed_the_whole() {
    let doc = load();
    let c = case(&doc, "shared_interest_cases", "shares_cannot_exceed_the_whole");
    let declared: Vec<(String, f64)> = c["declared"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_f64().unwrap()))
        .collect();
    let refs: Vec<(&str, f64)> = declared.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let err = SharedInterest::declared(&refs).expect_err("refused");
    assert!(matches!(err, DivisionError::SharesExceedTheWhole(_)));
}

#[test]
fn a_zero_rollup_is_refused_not_read_as_a_zero_share() {
    let doc = load();
    let _ = case(&doc, "shared_interest_cases", "a_zero_rollup_is_refused_not_read_as_a_zero_share");
    let err = SharedInterest::from_rollup(&[("a", 0.0), ("b", 0.0)]).expect_err("refused");
    assert!(matches!(err, DivisionError::EmptyRollup));
}

// ── rb > c ──────────────────────────────────────────────────────────────────

#[test]
fn the_check_reports_every_number_it_used() {
    let doc = load();
    let c = case(&doc, "stability_cases", "the_check_reports_every_number_it_used");
    let d = body(&doc);
    let chk = d
        .check(c["member"].as_str().unwrap(), c["role"].as_str().unwrap())
        .expect("declared");
    let e = &c["expect"];
    assert!((chk.r - e["r"].as_f64().unwrap()).abs() < 1e-12);
    assert!((chk.b - e["b"].as_f64().unwrap()).abs() < 1e-12);
    assert!((chk.c - e["c"].as_f64().unwrap()).abs() < 1e-12);
    assert!((chk.rb() - e["rb"].as_f64().unwrap()).abs() < 1e-12);
    assert!((chk.margin() - e["margin"].as_f64().unwrap()).abs() < 1e-12);
    assert_eq!(chk.holds, e["holds"].as_bool().unwrap());
}

#[test]
fn dilution_destabilises_the_same_role_as_the_share_shrinks() {
    let doc = load();
    let c = case(&doc, "stability_cases", "dilution_destabilises_the_same_role_as_the_share_shrinks");
    let role = c["role"].as_str().unwrap();
    let cost = c["cost"].as_f64().unwrap();
    let mut d = body(&doc);
    for m in members(&doc) {
        d.set_cost(&m, role, cost).expect("declared");
    }
    // Identical role, identical cost — only r differs.
    assert_eq!(d.check("ama", role).unwrap().holds, c["expect"]["ama"].as_bool().unwrap());
    assert_eq!(d.check("dee", role).unwrap().holds, c["expect"]["dee"].as_bool().unwrap());
}

#[test]
fn a_role_that_benefits_nobody_is_stable_for_no_member_at_any_share() {
    let doc = load();
    let _ = case(
        &doc,
        "stability_cases",
        "a_role_that_benefits_nobody_is_stable_for_no_member_at_any_share",
    );
    let mut d = Division::new(interest(&doc)).with_role(Role::new("busywork", 0.0, 1).unwrap());
    for m in members(&doc) {
        d.set_cost(&m, "busywork", 0.01).expect("declared");
    }
    // Even the largest share cannot rescue rb = 0 against any positive cost.
    for m in members(&doc) {
        assert!(!d.check(&m, "busywork").unwrap().holds, "{m}");
    }
}

#[test]
fn a_free_role_is_stable_for_everyone_and_signals_nothing() {
    let doc = load();
    let c = case(&doc, "stability_cases", "a_free_role_is_stable_for_everyone_and_signals_nothing");
    let mut d = Division::new(interest(&doc)).with_role(Role::new("token", 1.0, 1).unwrap());
    for m in members(&doc) {
        d.set_cost(&m, "token", 0.0).expect("declared");
        let chk = d.check(&m, "token").unwrap();
        assert_eq!(chk.holds, c["expect"]["holds"].as_bool().unwrap());
        assert_eq!(
            chk.signals_commitment(),
            c["expect"]["signals_commitment"].as_bool().unwrap(),
            "a role that costs nothing is evidence of nothing"
        );
    }
}

#[test]
fn a_missing_cost_is_refused_never_read_as_free() {
    let doc = load();
    let _ = case(&doc, "stability_cases", "a_missing_cost_is_refused_never_read_as_free");
    let mut d = Division::new(interest(&doc));
    for r in doc["fixture"]["roles"].as_array().unwrap() {
        d = d.with_role(
            Role::new(
                r["id"].as_str().unwrap(),
                r["b"].as_f64().unwrap(),
                r["rho"].as_u64().unwrap() as usize,
            )
            .unwrap(),
        );
    }
    let err = d.check("ama", "coordinator").expect_err("refused");
    assert!(matches!(err, DivisionError::MissingCost { .. }));
}

// ── the optimisation ────────────────────────────────────────────────────────

#[test]
fn a_stable_split_is_admitted_covered_and_redundant() {
    let doc = load();
    let c = case(&doc, "assignment_cases", "a_stable_split_is_admitted_covered_and_redundant");
    let d = body(&doc);
    let out = d.assign().expect("searched");
    let a = out.assigned().expect("assigned");
    let e = &c["expect"];
    assert_eq!(a.holders_of("coordinator").len(), e["coordinator_holders"].as_u64().unwrap() as usize);
    assert_eq!(a.holders_of("worker").len(), e["worker_holders"].as_u64().unwrap() as usize);
    assert_eq!(a.holders_of("replica").len(), e["replica_holders"].as_u64().unwrap() as usize);
    assert!(a.checks().iter().all(|c| c.holds));
}

/// ★★ THE PROOF OF VALUE.
///
/// A split that is **cheaper**, **covered** and **redundant**, and fails only
/// `rb > c`, is refused — and the price of refusing it is a number.
#[test]
fn stability_is_binding_the_cheapest_split_is_refused_and_the_price_is_reported() {
    let doc = load();
    let c = case(
        &doc,
        "assignment_cases",
        "★_stability_is_binding_the_cheapest_split_is_refused_and_the_price_is_reported",
    );
    let mut d = body(&doc);
    for (m, cost) in c["costs"].as_object().unwrap() {
        d.set_cost(m, "coordinator", cost.as_f64().unwrap()).expect("declared");
    }

    // dee has the smallest share, so rb = 0.1 × 100 = 10 against c = 12.
    let dee = d.check("dee", "coordinator").expect("declared");
    assert!(!dee.holds, "{}", dee.describe());
    assert_eq!(dee.holds, c["expect"]["dee_coordinator_stable"].as_bool().unwrap());

    // The cheap split really is cheap, and really does meet both ordinary
    // constraints — it fails ONLY the third one.
    let cheap = d
        .diagnose(&[
            ("dee", "coordinator"),
            ("ama", "worker"),
            ("ben", "worker"),
            ("cira", "replica"),
        ])
        .expect("diagnosed");
    assert!(cheap.covers() && cheap.is_redundant());
    assert!(!cheap.is_stable());

    // And it is refused. The optimiser pays more.
    let out = d.assign().expect("searched");
    let stable = out.assigned().expect("assigned");
    assert_ne!(stable.role_of("dee"), Some("coordinator"));
    assert_eq!(
        stable.role_of("dee") != Some("coordinator"),
        c["expect"]["stable_avoids_dee_as_coordinator"].as_bool().unwrap()
    );
    assert!(stable.checks().iter().all(|k| k.holds));
    assert!(
        stable.total_cost() > cheap.total_cost,
        "the refused split was cheaper: {} vs {}",
        cheap.total_cost,
        stable.total_cost()
    );

    // The unconstrained minimum is at most the cheap split, and the gap
    // against the stable one is what stability cost.
    let loose = d.unconstrained_minimum().expect("searched").expect("solved");
    assert!(loose <= cheap.total_cost);
    let price = d.price_of_stability().expect("computed").expect("both solved");
    assert!((price - (stable.total_cost() - loose)).abs() < 1e-12);
    assert_eq!(
        price > 0.0,
        c["expect"]["price_of_stability_positive"].as_bool().unwrap(),
        "stability is bought, and the price is {price}"
    );
}

#[test]
fn a_member_with_no_stable_role_is_named_before_any_search() {
    let doc = load();
    let c = case(&doc, "assignment_cases", "a_member_with_no_stable_role_is_named_before_any_search");
    let mut d = body(&doc);
    for r in ["coordinator", "worker", "replica"] {
        d.set_cost("dee", r, 999.0).expect("declared");
    }
    match d.assign().expect("searched") {
        AssignmentOutcome::NoStableAssignment(Unfillable::NoStableRole { members, checks }) => {
            let want: Vec<String> = c["expect"]["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            assert_eq!(members, want);
            // The reason travels with the verdict.
            assert!(!checks.is_empty() && checks.iter().all(|k| !k.holds));
        }
        other => panic!("expected NoStableRole, got {other:?}"),
    }
}

#[test]
fn a_role_too_few_are_stable_in_cannot_reach_its_redundancy() {
    let doc = load();
    let c = case(
        &doc,
        "assignment_cases",
        "a_role_too_few_are_stable_in_cannot_reach_its_redundancy",
    );
    let mut d = Division::new(interest(&doc))
        .with_role(Role::new("guard", 100.0, 3).unwrap())
        .with_role(Role::new("rest", 10.0, 1).unwrap());
    for m in members(&doc) {
        d.set_cost(&m, "rest", 0.5).expect("declared");
        d.set_cost(&m, "guard", 35.0).expect("declared");
    }
    match d.assign().expect("searched") {
        AssignmentOutcome::NoStableAssignment(Unfillable::CannotMeetRedundancy {
            role,
            stable_candidates,
            needed,
        }) => {
            let e = &c["expect"];
            assert_eq!(role, e["role"].as_str().unwrap());
            assert_eq!(stable_candidates, e["stable_candidates"].as_u64().unwrap() as usize);
            assert_eq!(needed, e["needed"].as_u64().unwrap() as usize);
        }
        other => panic!("expected CannotMeetRedundancy, got {other:?}"),
    }
}

#[test]
fn roles_needing_more_holders_than_the_body_has_are_refused_by_counting() {
    let doc = load();
    let c = case(
        &doc,
        "assignment_cases",
        "roles_needing_more_holders_than_the_body_has_are_refused_by_counting",
    );
    let mut d = Division::new(
        SharedInterest::from_rollup(&[("ama", 1.0), ("ben", 1.0)]).expect("stakes"),
    )
    .with_role(Role::new("a", 10.0, 2).unwrap())
    .with_role(Role::new("b", 10.0, 2).unwrap());
    for m in ["ama", "ben"] {
        for r in ["a", "b"] {
            d.set_cost(m, r, 0.1).expect("declared");
        }
    }
    match d.assign().expect("searched") {
        AssignmentOutcome::NoStableAssignment(Unfillable::Oversubscribed { required, members }) => {
            let e = &c["expect"];
            assert_eq!(required, e["required"].as_u64().unwrap() as usize);
            assert_eq!(members, e["members"].as_u64().unwrap() as usize);
        }
        other => panic!("expected Oversubscribed, got {other:?}"),
    }
}

#[test]
fn the_search_is_deterministic() {
    let doc = load();
    let _ = case(&doc, "assignment_cases", "the_search_is_deterministic");
    let d = body(&doc);
    assert_eq!(d.assign().expect("searched"), d.assign().expect("searched"));
}

// ── refusal ─────────────────────────────────────────────────────────────────

/// ★★ The structural half: an unstable split can be **diagnosed in full** and
/// still yields no [`sustena_core::Assignment`].
#[test]
fn an_unstable_assignment_cannot_be_held_only_diagnosed() {
    let doc = load();
    let c = case(&doc, "refusal_cases", "★_an_unstable_assignment_cannot_be_HELD_only_diagnosed");
    let mut d = body(&doc);
    d.set_cost("dee", "coordinator", 12.0).expect("declared");

    let proposed: Vec<(String, String)> = c["proposed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_str().unwrap().to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = proposed.iter().map(|(m, r)| (m.as_str(), r.as_str())).collect();
    let diag = d.diagnose(&refs).expect("diagnosed");

    let e = &c["expect"];
    assert_eq!(diag.covers(), e["covers"].as_bool().unwrap());
    assert_eq!(diag.is_redundant(), e["is_redundant"].as_bool().unwrap());
    assert_eq!(diag.is_stable(), e["is_stable"].as_bool().unwrap());
    assert_eq!(diag.admissible(), e["admissible"].as_bool().unwrap());
    let unstable: Vec<&str> = diag.unstable.iter().map(|k| k.member.as_str()).collect();
    let want: Vec<&str> = e["unstable_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(unstable, want);
    assert!(diag.describe().contains(e["describe_contains"].as_str().unwrap()));

    // ★ And the assignment the optimiser DOES return is a different one.
    // There is no route from a diagnosis to an `Assignment`.
    let out = d.assign().expect("searched");
    let a = out.assigned().expect("assigned");
    assert_ne!(a.role_of("dee"), Some("coordinator"));
}

#[test]
fn coverage_and_redundancy_are_reported_separately() {
    let doc = load();
    let c = case(&doc, "refusal_cases", "coverage_and_redundancy_are_reported_separately");
    let d = body(&doc);
    let proposed: Vec<(String, String)> = c["proposed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_str().unwrap().to_string(), p[1].as_str().unwrap().to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = proposed.iter().map(|(m, r)| (m.as_str(), r.as_str())).collect();
    let diag = d.diagnose(&refs).expect("diagnosed");

    let want_uncovered: Vec<String> = c["expect"]["uncovered"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(diag.uncovered, want_uncovered);
    for role in ["worker", "replica"] {
        let e = c["expect"][role].as_array().unwrap();
        let (have, need) = (e[0].as_u64().unwrap() as usize, e[1].as_u64().unwrap() as usize);
        assert!(
            diag.under_redundant.iter().any(|(r, h, n)| r == role && *h == have && *n == need),
            "{role} should report {have}/{need}"
        );
    }
}

#[test]
fn a_role_nobody_must_hold_is_not_a_role() {
    let doc = load();
    let _ = case(&doc, "refusal_cases", "a_role_nobody_must_hold_is_not_a_role");
    let err = Role::new("optional", 10.0, 0).expect_err("refused");
    assert!(matches!(err, DivisionError::ZeroRedundancy(_)));
}

// ── composition ─────────────────────────────────────────────────────────────

#[test]
fn the_node_set_comes_from_the_population_not_a_second_roster() {
    let doc = load();
    let _ = case(
        &doc,
        "composition_cases",
        "the_node_set_comes_from_the_population_not_a_second_roster",
    );
    let mut pop = Population::new(PopulationSpec::new(0.5, 0.1).expect("spec"));
    for m in members(&doc) {
        pop = pop.with_node(&m);
    }
    let d = Division::over(&pop, interest(&doc)).expect("same body");
    let mut got: Vec<&str> = d.members().iter().map(|s| s.as_str()).collect();
    got.sort();
    let mut want: Vec<String> = members(&doc);
    want.sort();
    assert_eq!(got, want.iter().map(|s| s.as_str()).collect::<Vec<_>>());
}

#[test]
fn a_node_with_no_declared_share_is_refused_in_both_directions() {
    let doc = load();
    let c = case(
        &doc,
        "composition_cases",
        "a_node_with_no_declared_share_is_refused_in_both_directions",
    );
    let pop = Population::new(PopulationSpec::new(0.5, 0.1).expect("spec"))
        .with_node("ama")
        .with_node("ben");

    let missing = Division::over(
        &pop,
        SharedInterest::from_rollup(&[("ama", 1.0)]).expect("stakes"),
    );
    assert!(matches!(missing, Err(DivisionError::NodesWithoutShare(_))));
    assert_eq!(c["expect"]["missing_share"].as_str().unwrap(), "NodesWithoutShare");

    let stranger = Division::over(
        &pop,
        SharedInterest::from_rollup(&[("ama", 1.0), ("ben", 1.0), ("zed", 1.0)]).expect("stakes"),
    );
    assert!(matches!(stranger, Err(DivisionError::SharesWithoutNode(_))));
    assert_eq!(c["expect"]["stranger"].as_str().unwrap(), "SharesWithoutNode");
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ The word `role` is present in the reference and never means this.
    let roles = d["★_the_word_role_appears_and_never_once_means_this"].as_str().unwrap();
    assert!(roles.contains("Anthropic Messages API"));
    assert!(roles.contains("role_in_family"));
    assert!(roles.contains("a label somebody types about themselves"));

    // ★ The counterweight is specific: r's SOURCE is at parity.
    let r = d["term_by_term"]["r — the share that comes back through the composed Sustain"]
        .as_str()
        .unwrap();
    assert!(r.contains("compute_rollup"));
    assert!(r.contains("AT PARITY"));
    assert!(r.contains("never forms the ratio"));

    // The alarming grep count is named as the false positive it is.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["ESS (1058 hits, 0 real)"].as_str().unwrap().contains("Not one is Maynard Smith"));
    assert!(fp["defect (3 hits, 0 real)"].as_str().unwrap().contains("bug in code"));

    // The counterweight calls the reference's design coherent, not deficient.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("completely coherent design"));
    assert!(cw.contains("not a criticism of the reference"));

    // The free-rider tension under ρ > 1 is disclosed, not papered over.
    let ess = d["the_honest_reading_of_ESS"].as_str().unwrap();
    assert!(ess.contains("PIVOTAL"));
    assert!(ess.contains("free-rider"));
    assert!(ess.contains("this build does not invent"));

    // Three honest limits, all present.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in ["COVERAGE IS THE ρ=1 CASE", "ONE ROLE PER MEMBER", "REFUSES RATHER THAN APPROXIMATING"] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = [
        "shared_interest_cases",
        "stability_cases",
        "assignment_cases",
        "refusal_cases",
        "composition_cases",
    ];
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
