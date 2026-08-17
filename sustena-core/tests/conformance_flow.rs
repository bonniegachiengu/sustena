//! The firewall `F`, replayed against its conformance vectors
//! (R2 · Constraint §I + §II · CON-1, CON-2, IMM-1).
//!
//! **SPEC vectors, Rust-only — with the conservation LAW at parity and the
//! FLOW notion not.** `transition.rs` already ships conservation, and the
//! reference gives `holon.transfer` real atomicity. But conservation is over
//! the **pair of endpoints** — *the total is unchanged* — where `F` is over
//! the **crossing** — *this specific movement is allowed*. The irreducibility
//! case below shows they come apart: two transitions that **both conserve**,
//! with byte-identical endpoints, one admitted and one refused.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    check_flows, classify_movement, execute, flows_of, Boundary, BoundaryDecl, Crossing,
    Enforcement, FlowDirection, FlowRule, Movement, Quantity, Registry, Scope, TransitionRule,
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("flow.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "flow.json was written for a different contract version"
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

// ── fixture ─────────────────────────────────────────────────────────────────

fn decl(doc: &Value) -> BoundaryDecl {
    let f = &doc["fixture"];
    let mut scope = Scope::new();
    for r in f["scope_roots"].as_array().unwrap() {
        scope = scope.spanning(r.as_str().unwrap());
    }
    scope = scope.with_entity_register(f["entity_register"].as_str().unwrap());
    let mut d = BoundaryDecl::new(scope);
    for p in f["owned_prefixes"].as_array().unwrap() {
        d = d.owning(p.as_str().unwrap());
    }
    d
}

fn household(doc: &Value, food: f64, savings: f64) -> Value {
    json!({
        "roster": doc["fixture"]["members"],
        "contacts": doc["fixture"]["contacts"],
        "finances": {
            "liquid": {"balance": 1000.0},
            "pockets": {
                "food": {"allocated": food, "spent": 0.0, "limit": 0.0},
                "savings": {"allocated": savings, "spent": 0.0, "limit": 0.0}
            },
            "income": {"monthly_total": 0.0, "sources": []}
        }
    })
}

fn boundary(doc: &Value) -> Boundary {
    decl(doc).read(&household(doc, 1000.0, 0.0))
}

fn movement(v: &Value) -> Movement {
    let a = v.as_array().unwrap();
    Movement::new(
        a[0].as_str().unwrap(),
        a[1].as_f64().unwrap(),
        a[2].as_str().unwrap(),
        a[3].as_str().unwrap(),
    )
}

fn movements(v: &Value) -> Vec<Movement> {
    v.as_array().unwrap().iter().map(movement).collect()
}

// ── ★ crossing is μ's question ──────────────────────────────────────────────

#[test]
fn pocket_to_pocket_is_internal_and_produces_no_flow() {
    let doc = load();
    let c = case(
        &doc,
        "crossing_cases",
        "★_pocket_to_pocket_is_INTERNAL_and_produces_no_flow",
    );
    let m = movement(&c["movement"]);
    let b = boundary(&doc);
    assert_eq!(classify_movement(&m, &b), Crossing::Internal);
    assert_eq!(
        flows_of(&[m], &b).len(),
        c["expect"]["flows"].as_u64().unwrap() as usize
    );
}

#[test]
fn an_outside_counterparty_makes_it_a_crossing_and_mu_picks_the_direction() {
    let doc = load();
    let c = case(
        &doc,
        "crossing_cases",
        "★_an_outside_counterparty_makes_it_a_crossing_and_μ_picks_the_direction",
    );
    let e = &c["expect"];
    let b = boundary(&doc);

    let inbound = Movement::new("money", 500.0, "employer", "finances.liquid");
    match classify_movement(&inbound, &b) {
        Crossing::Crossed(f) => {
            assert_eq!(f.dir.name(), e["inbound_dir"].as_str().unwrap());
            assert_eq!(f.counterparty, e["inbound_counterparty"].as_str().unwrap());
        }
        other => panic!("expected a crossing, got {other:?}"),
    }

    let outbound = Movement::new("money", 500.0, "finances.pockets.food", "naivas");
    match classify_movement(&outbound, &b) {
        Crossing::Crossed(f) => {
            assert_eq!(f.dir.name(), e["outbound_dir"].as_str().unwrap());
            assert_eq!(f.counterparty, e["outbound_counterparty"].as_str().unwrap());
        }
        other => panic!("expected a crossing, got {other:?}"),
    }
}

#[test]
fn a_movement_between_two_outside_parties_is_reported_not_ignored() {
    let doc = load();
    let c = case(
        &doc,
        "crossing_cases",
        "a_movement_between_two_outside_parties_is_reported_not_ignored",
    );
    let m = Movement::new("money", 5.0, "alice", "bob");
    let b = boundary(&doc);
    assert!(matches!(classify_movement(&m, &b), Crossing::Foreign { .. }));
    assert_eq!(
        flows_of(&[m], &b).len(),
        c["expect"]["flows"].as_u64().unwrap() as usize
    );
}

#[test]
fn an_owned_entity_counts_as_inside_as_well_as_an_owned_path() {
    let doc = load();
    let _ = case(
        &doc,
        "crossing_cases",
        "an_owned_ENTITY_counts_as_inside_as_well_as_an_owned_path",
    );
    let m = Movement::new("money", 10.0, "ama", "finances.pockets.food");
    assert_eq!(classify_movement(&m, &boundary(&doc)), Crossing::Internal);
}

// ── ★★ irreducibility ───────────────────────────────────────────────────────

/// ★★ THE SIGNATURE PROOF. Same endpoints, both conserving, only `F` separates.
#[test]
fn f_distinguishes_two_transitions_that_c_and_d_cannot() {
    let doc = load();
    let c = case(
        &doc,
        "irreducibility_cases",
        "★★_F_distinguishes_two_transitions_that_C_and_D_CANNOT",
    );
    let e = &c["expect"];
    let b = boundary(&doc);

    // The endpoints, shared by BOTH transitions.
    let before = household(&doc, 1000.0, 0.0);
    let after = household(&doc, 500.0, 500.0);
    assert!(e["endpoints_identical"].as_bool().unwrap());

    // ★ Even conservation — D's strongest shape — passes both, because both
    //   conserve. There is nothing in the endpoints to tell apart.
    let conservation = vec![TransitionRule::Conservation {
        id: "money_conserved".into(),
        quantity: Quantity::new(
            "household money",
            &["finances.pockets[*].allocated"],
        ),
        // Exact: integer minor units, tolerance zero — D's STRONGEST form.
        tolerance: sustena_core::Tolerance::Exact,
    }];
    assert!(
        sustena_core::check_transitions(&conservation, &before, &after).is_ok(),
        "conservation must pass the shared endpoint pair"
    );
    assert!(e["conservation_passes_both"].as_bool().unwrap());

    // The two transitions differ ONLY in what crossed.
    let internal = movements(&c["internal"]);
    let round_trip = movements(&c["round_trip"]);
    let fa = flows_of(&internal, &b);
    let fb = flows_of(&round_trip, &b);
    assert_eq!(fa.len(), e["internal_flows"].as_u64().unwrap() as usize);
    assert_eq!(fb.len(), e["round_trip_flows"].as_u64().unwrap() as usize);

    let rules = vec![FlowRule::Forbid {
        id: "no_gambling".into(),
        dir: None,
        kind: None,
    }];
    assert_eq!(
        check_flows(&fa, &rules, &before).is_ok(),
        e["internal_admitted"].as_bool().unwrap()
    );
    assert_eq!(
        check_flows(&fb, &rules, &before).is_err(),
        e["round_trip_refused"].as_bool().unwrap()
    );
}

// ── the rules ───────────────────────────────────────────────────────────────

#[test]
fn only_with_is_an_allowlist_and_max_qty_caps_one_crossing() {
    let doc = load();
    let c = case(
        &doc,
        "rule_cases",
        "only_with_is_an_allowlist_and_max_qty_caps_one_crossing",
    );
    let e = &c["expect"];
    let b = boundary(&doc);
    let state = household(&doc, 1000.0, 0.0);
    let big = flows_of(
        &[Movement::new("money", 900.0, "finances.pockets.food", "naivas")],
        &b,
    );

    let allow = vec![FlowRule::OnlyWith {
        id: "known_shops".into(),
        dir: Some(FlowDirection::Out),
        counterparties: ["landlord".to_string()].into_iter().collect(),
    }];
    let err = check_flows(&big, &allow, &state).unwrap_err();
    assert!(err.detail.contains(e["allowlist_refuses"].as_str().unwrap()));

    let cap = vec![FlowRule::MaxQty {
        id: "small_only".into(),
        dir: None,
        kind: Some("money".into()),
        limit: 100.0,
    }];
    assert_eq!(check_flows(&big, &cap, &state).is_err(), e["cap_refuses_900"].as_bool().unwrap());
    let small = flows_of(
        &[Movement::new("money", 50.0, "finances.pockets.food", "naivas")],
        &b,
    );
    assert_eq!(check_flows(&small, &cap, &state).is_ok(), e["cap_admits_50"].as_bool().unwrap());
}

/// ★ The shape that uses `F(φ, s)`'s state argument.
#[test]
fn only_known_reads_the_register_from_state() {
    let doc = load();
    let c = case(&doc, "rule_cases", "★_only_known_reads_the_register_from_STATE");
    let e = &c["expect"];
    let b = boundary(&doc);
    let state = household(&doc, 1000.0, 0.0);
    let rules = vec![FlowRule::OnlyKnown {
        id: "contacts_only".into(),
        dir: Some(FlowDirection::Out),
        register: "contacts".into(),
    }];

    let known = flows_of(
        &[Movement::new("money", 10.0, "finances.pockets.food", e["known_admitted"].as_str().unwrap())],
        &b,
    );
    assert!(check_flows(&known, &rules, &state).is_ok());

    let stranger = flows_of(
        &[Movement::new("money", 10.0, "finances.pockets.food", e["stranger_refused"].as_str().unwrap())],
        &b,
    );
    assert!(check_flows(&stranger, &rules, &state).is_err());
}

#[test]
fn a_rule_that_does_not_apply_is_silent() {
    let doc = load();
    let c = case(&doc, "rule_cases", "a_rule_that_does_not_apply_is_silent");
    let b = boundary(&doc);
    let inbound = flows_of(
        &[Movement::new("money", 5000.0, "employer", "finances.liquid")],
        &b,
    );
    let rules = vec![FlowRule::MaxQty {
        id: "out_cap".into(),
        dir: Some(FlowDirection::Out),
        kind: None,
        limit: 10.0,
    }];
    assert_eq!(
        check_flows(&inbound, &rules, &household(&doc, 1.0, 1.0)).is_ok(),
        c["expect"]["inbound_passes_out_cap"].as_bool().unwrap()
    );
}

/// ★ Conjunction only — there is no override to reach for.
#[test]
fn adding_a_rule_can_only_shrink_the_admissible_region() {
    let doc = load();
    let c = case(
        &doc,
        "rule_cases",
        "★_adding_a_rule_can_only_SHRINK_the_admissible_region",
    );
    let e = &c["expect"];
    let b = boundary(&doc);
    let state = household(&doc, 1000.0, 0.0);
    let out = flows_of(
        &[Movement::new("money", 50.0, "finances.pockets.food", "naivas")],
        &b,
    );

    assert_eq!(
        check_flows(&out, &[], &state).is_ok(),
        e["admitted_with_none"].as_bool().unwrap()
    );
    let forbid = FlowRule::Forbid {
        id: "no_outflow".into(),
        dir: Some(FlowDirection::Out),
        kind: None,
    };
    assert_eq!(
        check_flows(&out, std::slice::from_ref(&forbid), &state).is_err(),
        e["refused_with_forbid"].as_bool().unwrap()
    );

    // A more permissive second rule cannot undo the first.
    let both = vec![
        forbid,
        FlowRule::OnlyWith {
            id: "allow_naivas".into(),
            dir: Some(FlowDirection::Out),
            counterparties: ["naivas".to_string()].into_iter().collect(),
        },
    ];
    assert_eq!(
        check_flows(&out, &both, &state).is_err(),
        e["permissive_rule_cannot_override"].as_bool().unwrap()
    );
}

#[test]
fn no_flows_means_f_is_vacuously_satisfied() {
    let doc = load();
    let c = case(&doc, "rule_cases", "no_flows_means_F_is_vacuously_satisfied");
    let rules = vec![FlowRule::Forbid {
        id: "everything".into(),
        dir: None,
        kind: None,
    }];
    assert_eq!(
        check_flows(&[], &rules, &household(&doc, 1.0, 1.0)).is_ok(),
        c["expect"]["admitted"].as_bool().unwrap()
    );
}

// ── ★★ at the gate ──────────────────────────────────────────────────────────

fn with_firewall(doc: &Value, rules: Vec<FlowRule>) -> Enforcement {
    Enforcement {
        boundary: Some(decl(doc)),
        firewall: rules,
        ..Enforcement::default()
    }
}

/// ★★ The household's real case, through `execute`.
#[test]
fn the_firewall_refuses_an_inflow_from_an_unknown_counterparty_at_the_gate() {
    let doc = load();
    let c = case(
        &doc,
        "gate_cases",
        "★★_the_firewall_refuses_an_inflow_from_an_unknown_counterparty_AT_THE_GATE",
    );
    let e = &c["expect"];
    let reg = Registry::default();
    let rules = vec![FlowRule::OnlyKnown {
        id: "contacts_only".into(),
        dir: Some(FlowDirection::In),
        register: "contacts".into(),
    }];
    let mut p = serde_json::Map::new();
    p.insert("amount".into(), json!(500.0));
    p.insert("entry_id".into(), json!("e1"));

    // A known counterparty is admitted.
    let mut ok_params = p.clone();
    ok_params.insert("source".into(), json!(e["known_source_ok"].as_str().unwrap()));
    let ok = execute(
        &reg, &reg.names(), &with_firewall(&doc, rules.clone()),
        &household(&doc, 0.0, 0.0), "budget.record_income", &ok_params,
    );
    assert!(ok.committed(), "{:?}", ok.result);

    // ★ The SAME operator, SAME amount, unknown counterparty: refused.
    let before = household(&doc, 0.0, 0.0);
    let mut no_params = p;
    no_params.insert("source".into(), json!(e["unknown_source_refused"].as_str().unwrap()));
    let no = execute(
        &reg, &reg.names(), &with_firewall(&doc, rules), &before,
        "budget.record_income", &no_params,
    );
    assert!(!no.committed());
    assert_eq!(
        no.result.constraint_violated.as_deref(),
        Some(e["constraint_violated"].as_str().unwrap())
    );
    // The gate's standing promise survives a fourth conjunct.
    assert_eq!(no.state == before, e["state_untouched"].as_bool().unwrap());
    assert!(no.mutations.is_empty() && no.events.is_empty());
}

/// ★ Nothing crossed, so a rule forbidding everything still admits it.
#[test]
fn an_internal_move_passes_a_firewall_that_forbids_everything() {
    let doc = load();
    let c = case(
        &doc,
        "gate_cases",
        "★_an_INTERNAL_move_passes_a_firewall_that_forbids_EVERYTHING",
    );
    let reg = Registry::default();
    let forbid_all = vec![FlowRule::Forbid {
        id: "sealed".into(),
        dir: None,
        kind: None,
    }];
    let mut params = serde_json::Map::new();
    params.insert("pocket_name".into(), json!("food"));
    params.insert("amount".into(), json!(100.0));
    params.insert("period".into(), json!("monthly"));

    let ex = execute(
        &reg, &reg.names(), &with_firewall(&doc, forbid_all),
        &household(&doc, 0.0, 0.0), "budget.allocate", &params,
    );
    assert_eq!(ex.committed(), c["expect"]["admitted"].as_bool().unwrap(), "{:?}", ex.result);
}

#[test]
fn the_firewall_is_inert_without_a_boundary_to_cross() {
    let doc = load();
    let c = case(&doc, "gate_cases", "the_firewall_is_inert_without_a_boundary_to_cross");
    let reg = Registry::default();
    let rules_only = Enforcement {
        firewall: vec![FlowRule::Forbid { id: "sealed".into(), dir: None, kind: None }],
        ..Enforcement::default()
    };
    let mut params = serde_json::Map::new();
    params.insert("amount".into(), json!(500.0));
    params.insert("source".into(), json!("anyone"));
    params.insert("entry_id".into(), json!("e9"));

    let ex = execute(
        &reg, &reg.names(), &rules_only, &household(&doc, 0.0, 0.0),
        "budget.record_income", &params,
    );
    assert_eq!(ex.committed(), c["expect"]["admitted"].as_bool().unwrap(), "{:?}", ex.result);
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★ The counterweight distinguishes the LAW from the NOTION.
    let cw = d["★_the_counterweight_the_conservation_LAW_is_at_parity_the_FLOW_notion_is_not"]
        .as_str()
        .unwrap();
    assert!(cw.contains("conservation LAW is at parity"));
    assert!(cw.contains("both conserve"));
    assert!(cw.contains("nothing in the endpoints to tell apart"));

    // Conservation is credited so the gap is not overstated.
    let cons = d["term_by_term"]["conservation as a D shape"].as_str().unwrap();
    assert!(cons.contains("AT PARITY"));
    assert!(cons.contains("does not care whether money is conserved"));

    // ★ The clamp trap is honoured by construction, not by a note.
    let clamp = d["term_by_term"]["the clamp/conservation trap"].as_str().unwrap();
    assert!(clamp.contains("no clamp strategy at all"));
    assert!(clamp.contains("unrepresentable"));

    // The counterparty false positive is a real object, distinguished precisely.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    let cp = fp["counterparty (real, but a different object)"].as_str().unwrap();
    assert!(cp.contains("transducer"));
    assert!(cp.contains("not the same as having a rule"));

    // Four limits, and the slot named among them.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "DECLARED SHAPES",
        "NO CLAMP, DELIBERATELY",
        "the named slot",
        "BEFORE** STATE",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    // Additive, including the one invasive change named honestly.
    let add = d["not_a_divergence_in"].as_str().unwrap();
    assert!(add.contains("genuinely invasive change is the operator signature"));

    let groups = ["crossing_cases", "irreducibility_cases", "rule_cases", "gate_cases"];
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
