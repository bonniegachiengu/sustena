//! `B` and the autopoietic closure law, replayed against their conformance
//! vectors (R2 · Sustain §IV · SUS-7, SUS-8, OP-10).
//!
//! **SPEC vectors, Rust-only — with the boundary-change ACT at parity.**
//! `holon.create_child` / `dissolve_child` exist in the reference and are
//! recognised there as a different class of operator. ★ But the carve-out is a
//! **capability** escape hatch (`ctx.engine` — a wider reach), where §IV asks
//! for a *"separate, **higher** gate"*. Wider is not higher, and nothing there
//! asks who may move a boundary as distinct from who may spend.
//!
//! The proof of value: an ordinary operator that appends to the roster is
//! **refused at the gate**, naming what it did and what it would have needed
//! instead. The law is structural — with two paths there is nothing to exempt,
//! where one path would need the three holon operators carved out of it.
//!
//! ★ Flows across `B` — the firewall `F` — are deliberately **not here**. §IV
//! says they *"belong to the constraint-engine layer, not here."* That is
//! CON-1's follow-on and the immediate next slice; what this row unblocks is
//! its subject.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    admit_boundary_change, preserves_closure, preserves_membership, AuthorityError, Boundary,
    BoundaryChange, BoundaryDecl, BoundaryToken, ClosureViolation, EditAuthority, Governance,
    Owned, Scope, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("boundary.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "boundary.json was written for a different contract version"
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

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

// ── fixture, built from the vector ──────────────────────────────────────────

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

fn state(members: &[&str], balance: i64) -> Value {
    json!({
        "roster": members,
        "finances": {"liquid": {"balance": balance}},
        "weather": {"outside": "rain"}
    })
}

fn fixture_state(doc: &Value, balance: i64) -> Value {
    let members: Vec<&str> = doc["fixture"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    state(&members, balance)
}

fn read(doc: &Value, balance: i64) -> Boundary {
    decl(doc).read(&fixture_state(doc, balance))
}

// ── ★ μ ─────────────────────────────────────────────────────────────────────

/// ★ §IV's own two conjuncts, kept as two.
#[test]
fn owns_requires_both_scope_and_membership() {
    let doc = load();
    let c = case(&doc, "mu_cases", "★_owns_requires_BOTH_scope_and_membership");
    let e = &c["expect"];
    let b = read(&doc, 100);
    let members = strs(&doc["fixture"]["members"]);

    assert_eq!(b.owns(&Owned::entity(&members[0])), e["member"].as_bool().unwrap());
    assert_eq!(b.owns(&Owned::entity("stranger")), e["stranger"].as_bool().unwrap());
    assert_eq!(
        b.owns(&Owned::path("finances.liquid.balance")),
        e["owned_path"].as_bool().unwrap()
    );
    assert_eq!(
        b.owns(&Owned::path(doc["fixture"]["out_of_scope_path"].as_str().unwrap())),
        e["out_of_scope_path"].as_bool().unwrap()
    );
    // Own(Σ) over entities is exactly the declared roster.
    let owned: BTreeSet<String> = b.entities().clone();
    assert_eq!(owned, members.into_iter().collect::<BTreeSet<String>>());
}

/// ★ The scope conjunct earning its place.
#[test]
fn a_path_mu_claims_but_scope_does_not_span_is_not_owned() {
    let doc = load();
    let c = case(
        &doc,
        "mu_cases",
        "a_path_mu_claims_but_scope_does_not_span_is_not_owned",
    );
    let d = BoundaryDecl::new(Scope::new().spanning("finances")).owning("weather");
    let b = d.read(&json!({}));
    assert_eq!(
        b.owns(&Owned::path("weather.outside")),
        c["expect"]["owns"].as_bool().unwrap()
    );
}

#[test]
fn a_sustain_with_no_entity_register_owns_no_entities() {
    let doc = load();
    let c = case(&doc, "mu_cases", "a_sustain_with_no_entity_register_owns_no_entities");
    let e = &c["expect"];
    let d = BoundaryDecl::new(Scope::new().spanning("finances")).owning("finances");
    let b = d.read(&fixture_state(&doc, 1));
    assert_eq!(b.owns(&Owned::entity("ama")), e["owns_entity"].as_bool().unwrap());
    assert_eq!(b.register_readable(), e["register_readable"].as_bool().unwrap());
}

/// ★ Absent is not empty.
#[test]
fn an_unreadable_register_is_distinguished_from_an_empty_one() {
    let doc = load();
    let c = case(
        &doc,
        "mu_cases",
        "★_an_unreadable_register_is_distinguished_from_an_EMPTY_one",
    );
    let e = &c["expect"];
    let d = decl(&doc);

    let empty = d.read(&json!({"roster": [], "finances": {}}));
    assert!(empty.entities().is_empty());
    assert_eq!(empty.register_readable(), e["empty_readable"].as_bool().unwrap());

    let missing = d.read(&json!({"finances": {}}));
    assert!(missing.entities().is_empty());
    assert_eq!(missing.register_readable(), e["missing_readable"].as_bool().unwrap());
}

// ── ★★ the closure law ──────────────────────────────────────────────────────

#[test]
fn an_ordinary_move_changes_values_and_preserves_the_boundary() {
    let doc = load();
    let c = case(
        &doc,
        "closure_cases",
        "an_ordinary_move_changes_values_and_preserves_the_boundary",
    );
    let before = fixture_state(&doc, 100);
    let after = fixture_state(&doc, 40);
    assert_eq!(
        preserves_membership(&before, &after, &decl(&doc)).is_ok(),
        c["expect"]["preserved"].as_bool().unwrap()
    );
}

/// ★★ THE PROOF. An ordinary move cannot change what the boundary is.
#[test]
fn admitting_a_member_through_an_ordinary_move_is_refused() {
    let doc = load();
    let c = case(
        &doc,
        "closure_cases",
        "★★_admitting_a_member_through_an_ordinary_move_is_REFUSED",
    );
    let e = &c["expect"];
    let before = state(&["ama"], 100);
    let after = state(&["ama", "ben"], 100);

    match preserves_membership(&before, &after, &decl(&doc)) {
        Err(err @ ClosureViolation::Membership { .. }) => {
            assert!(e["refused"].as_bool().unwrap());
            let ClosureViolation::Membership { detail } = &err else {
                unreachable!()
            };
            assert!(
                detail.contains(e["detail_contains"].as_str().unwrap()),
                "{detail}"
            );
            // ★ The refusal names what it would have needed instead.
            assert!(err.to_string().contains(e["message_contains"].as_str().unwrap()));
        }
        other => panic!("expected a membership violation, got {other:?}"),
    }
}

#[test]
fn releasing_a_member_breaks_it_too() {
    let doc = load();
    let c = case(&doc, "closure_cases", "releasing_a_member_breaks_it_too");
    let before = state(&["ama", "ben"], 100);
    let after = state(&["ama"], 100);
    match preserves_membership(&before, &after, &decl(&doc)) {
        Err(ClosureViolation::Membership { detail }) => assert!(
            detail.contains(c["expect"]["detail_contains"].as_str().unwrap()),
            "{detail}"
        ),
        other => panic!("expected a membership violation, got {other:?}"),
    }
}

/// ★ The attack the two-halves design has to survive.
#[test]
fn moving_the_register_out_from_under_mu_is_refused_not_read_as_empty() {
    let doc = load();
    let _ = case(
        &doc,
        "closure_cases",
        "★_moving_the_register_out_from_under_μ_is_refused_not_read_as_empty",
    );
    let before = fixture_state(&doc, 100);
    let after = json!({"finances": {"liquid": {"balance": 100}}});
    assert!(matches!(
        preserves_membership(&before, &after, &decl(&doc)),
        Err(ClosureViolation::RegisterLost { .. })
    ));
}

/// ★ One law, two halves, and the error says which.
#[test]
fn the_whole_law_is_one_conjunction_and_the_error_names_which_half() {
    let doc = load();
    let c = case(
        &doc,
        "closure_cases",
        "★_the_whole_law_is_one_conjunction_and_the_error_names_WHICH_half",
    );
    let e = &c["expect"];
    let before = state(&["ama"], 100);
    let moved = state(&["ama", "ben"], 100);

    let err = preserves_closure(&before, &moved, Some(&decl(&doc)), None).unwrap_err();
    assert!(matches!(err, ClosureViolation::Membership { .. }));
    // The gate tags the two halves apart — see operator::execute.
    assert_eq!(e["membership_half"].as_str().unwrap(), "boundary_closure");
    assert_eq!(e["schema_half"].as_str().unwrap(), "organisational_closure");

    // An unchanged boundary passes the μ half regardless of anything else.
    assert!(preserves_closure(&before, &state(&["ama"], 7), Some(&decl(&doc)), None).is_ok());
}

// ── ★ nesting ───────────────────────────────────────────────────────────────

/// ★ Composes without dissolving.
#[test]
fn owning_a_child_does_not_own_the_childs_interior() {
    let doc = load();
    let c = case(
        &doc,
        "nesting_cases",
        "★_owning_a_child_does_not_own_the_childs_interior",
    );
    let e = &c["expect"];
    let b = decl(&doc).read(&state(&["cira"], 0));
    assert_eq!(b.owns(&Owned::entity("cira")), e["owns_child"].as_bool().unwrap());
    assert_eq!(
        b.owns_interior_of("cira", "finances.liquid.balance"),
        e["owns_childs_interior"].as_bool().unwrap()
    );
}

// ── ★ the higher gate ───────────────────────────────────────────────────────

#[test]
fn a_sole_owner_may_move_their_own_boundary() {
    let doc = load();
    let c = case(&doc, "higher_gate_cases", "a_sole_owner_may_move_their_own_boundary");
    let e = &c["expect"];
    let gov = Governance::SoleOwner("bonnie".into());
    let admitted = e["admitted"].as_str().unwrap();

    let tok = BoundaryToken::mint(
        &gov,
        &EditAuthority::Owner("bonnie"),
        BoundaryChange::AdmitEntity(admitted.to_string()),
    )
    .expect("the owner may");

    let before = decl(&doc).read(&state(&["ama"], 0));
    let after = admit_boundary_change(&before, tok).expect("applied");
    assert!(after.owns(&Owned::entity(admitted)));
    assert!(
        after.owns(&Owned::entity(e["existing_kept"].as_str().unwrap())),
        "admitting is not replacing"
    );
}

#[test]
fn a_stranger_cannot_move_a_sole_owners_boundary() {
    let doc = load();
    let _ = case(&doc, "higher_gate_cases", "a_stranger_cannot_move_a_sole_owners_boundary");
    assert!(matches!(
        BoundaryToken::mint(
            &Governance::SoleOwner("bonnie".into()),
            &EditAuthority::Owner("someone_else"),
            BoundaryChange::AdmitEntity("ben".into())
        ),
        Err(AuthorityError::NotTheOwner { .. })
    ));
}

/// ★★ The asymmetry, reused rather than reinvented.
#[test]
fn a_shared_sustains_boundary_cannot_be_moved_by_a_single_actor() {
    let doc = load();
    let _ = case(
        &doc,
        "higher_gate_cases",
        "★★_a_SHARED_sustains_boundary_cannot_be_moved_by_a_single_actor",
    );
    assert!(matches!(
        BoundaryToken::mint(
            &Governance::Shared,
            &EditAuthority::Owner("bonnie"),
            BoundaryChange::AdoptChild("cira".into())
        ),
        Err(AuthorityError::SharedNeedsCouncil)
    ));
}

/// ★ `⊕` is a boundary change, not an ordinary move.
#[test]
fn adopting_and_releasing_a_child_are_boundary_changes() {
    let doc = load();
    let c = case(
        &doc,
        "higher_gate_cases",
        "★_adopting_and_releasing_a_child_ARE_boundary_changes",
    );
    let e = &c["expect"];
    let gov = Governance::SoleOwner("bonnie".into());
    let child = e["after_adopt_owns"].as_str().unwrap();

    let b0 = decl(&doc).read(&state(&[], 0));
    let adopt = BoundaryToken::mint(
        &gov,
        &EditAuthority::Owner("bonnie"),
        BoundaryChange::AdoptChild(child.to_string()),
    )
    .unwrap();
    assert!(adopt
        .change()
        .describe()
        .contains(e["describe_contains"].as_str().unwrap()));

    let b1 = admit_boundary_change(&b0, adopt).unwrap();
    assert!(b1.owns(&Owned::entity(child)));

    let release = BoundaryToken::mint(
        &gov,
        &EditAuthority::Owner("bonnie"),
        BoundaryChange::ReleaseChild(child.to_string()),
    )
    .unwrap();
    let b2 = admit_boundary_change(&b1, release).unwrap();
    assert_eq!(
        b2.owns(&Owned::entity(child)),
        e["after_release_owns"].as_bool().unwrap()
    );
}

// ── the recorded divergence ─────────────────────────────────────────────────

/// The divergence is part of the artefact. If the reference catches up, this
/// test is where the note gets rewritten — not quietly deleted.
#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // The flat model is quoted, since §IV names it by its symptoms.
    let stmt = d["statement"].as_str().unwrap();
    assert!(stmt.contains("owner_ids"));
    assert!(stmt.contains("a state dict with an owner field"));

    // ★ The counterweight: the ACT exists and is recognised as different.
    let cw = d["★_the_counterweight_a_boundary_change_PATH_exists_and_is_recognised_as_a_different_class"]
        .as_str()
        .unwrap();
    assert!(cw.contains("holon.create_child"));
    assert!(cw.contains("escape hatch"));

    // ★★ And the gap: wider, not higher.
    let gap = d["★★_but_the_carve_out_is_WIDER_not_HIGHER_and_that_is_the_whole_gap"]
        .as_str()
        .unwrap();
    assert!(gap.contains("Wider is not higher"));
    assert!(gap.contains("nothing to exempt"));

    // Half the law was already at parity in Rust, and that is credited.
    let law = d["term_by_term"]["★ the autopoietic closure law"].as_str().unwrap();
    assert!(law.contains("HALF AT PARITY IN RUST ALREADY"));
    assert!(law.contains("anticipated this row"));

    // ★ The μ collision is named BEFORE it bites, not after.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["boundary (8 hits, 0 real)"].as_str().unwrap().contains("HARD SAFETY BOUNDARY"));
    let mu = fp["μ — ★ a collision worth naming BEFORE it bites"].as_str().unwrap();
    assert!(mu.contains("migrate::Mu"));
    assert!(mu.contains("does NOT introduce a type called `Mu`"));

    // Four honest limits, including the disclosed cosmetic one.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "DECLARED RULE SET",
        "TWO HALVES WITH DIFFERENT NATURES",
        "WORDED FOR DEFINITION EDITS",
        "FLOWS ARE NOT HERE",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    // ★ The slot, and what this row unblocks.
    let slot = d["★_the_slot_and_what_this_row_unblocks"].as_str().unwrap();
    assert!(slot.contains("belong to the constraint-engine layer, not here"));
    assert!(slot.contains("CON-2"));
    assert!(slot.contains("IMM-1"));

    // Additive: an absent boundary changes nothing.
    assert!(d["not_a_divergence_in"]
        .as_str()
        .unwrap()
        .contains("defaults to `None`"));

    let groups = ["mu_cases", "closure_cases", "nesting_cases", "higher_gate_cases"];
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
