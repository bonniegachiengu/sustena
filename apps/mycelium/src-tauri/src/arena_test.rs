//! **Publish, install, instantiate** — against a real `World` on a real store.
//!
//! The property under test throughout is the one that matters for a registry:
//! **an installed artifact faces exactly the gate a locally-authored one
//! faces.** Every refusal here is the engine's own, in the engine's own words.

use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::package::{Authenticity, Integrity, InstallVerdict};

use crate::definitions::AuthoredDefinition;
use crate::arena::Publication;
use crate::store::Store;
use crate::templates::TemplateId;
use crate::widgets::AuthoredWidget;
use crate::world::{World, DEFAULT_HANDLE};

const PASS: &str = "a-long-enough-passphrase";

fn node(name: &str) -> (World, PathBuf) {
    let home = std::env::temp_dir().join(format!("mycelium-arena-{name}"));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("home");
    let world = World::open(Store::at(&home).expect("store")).expect("world");
    world.enrol(DEFAULT_HANDLE, PASS).expect("enrol");
    (world, home)
}

/// The garden definition from the Define screen's own shape.
fn garden() -> AuthoredDefinition {
    serde_json::from_value(json!({
        "id": "garden",
        "label": "Garden",
        "dimensions": [
            { "path": "moisture", "kind": "number", "lo": 0.0, "hi": 100.0 },
            { "path": "beds", "kind": "number", "lo": 0.0, "hi": null }
        ],
        "operators": ["budget.record_income"],
        "invariants": [
            { "id": "not_a_swamp", "expression": "moisture <= 100" }
        ],
        "openingState": { "moisture": 40.0, "beds": 3.0 }
    }))
    .expect("a well-formed authored definition")
}

/// The same shape, with an invariant referencing a dimension it never declared.
fn broken_garden() -> AuthoredDefinition {
    let mut g = garden();
    g.id = "broken-garden".into();
    g.invariants[0].expression = "rainfall >= 0".into();
    g
}

// ── publish ─────────────────────────────────────────────────────────────────

#[test]
fn a_published_package_is_stamped_hashed_and_signed() {
    let (world, _home) = node("publish");
    let spec = serde_json::to_value(garden()).expect("spec");

    let (pkg, verdict) = world
        .publish(&Publication {
            name: "Garden".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "a small garden".into(),
            tags: vec!["demo".into()],
            spec: spec.clone(),
            per_mille: 0,
            into: None,
        })
        .expect("publish");

    assert!(verdict.admitted().is_some(), "{}", verdict.describe());
    assert_eq!(pkg.author, world.node_id().expect("node id"), "the author IS the key");
    assert_eq!(pkg.content_hash, crate::arena::content_hash(&spec));
    assert!(pkg.signature.is_some());

    // ★★★ The three questions, each answered on its own.
    let prov = pkg.provenance();
    assert_eq!(prov.integrity, Integrity::Intact);
    assert_eq!(prov.authenticity, Authenticity::Signed);
    assert!(prov.safe_to_install());

    // And it is on disk, readable back.
    assert_eq!(world.arena().all().len(), 1);
    assert_eq!(world.arena().get(&pkg.id).expect("stored").content_hash, pkg.content_hash);
}

#[test]
fn a_package_that_does_not_typecheck_is_refused_at_publish_not_stored() {
    // ★★★ The reference stores `spec_json` unvalidated and never reads it
    //     back. A registry that keeps known-broken artifacts is a place where
    //     bad things wait.
    let (world, _home) = node("publish-broken");
    let spec = serde_json::to_value(broken_garden()).expect("spec");

    let (_, verdict) = world
        .publish(&Publication {
            name: "Broken".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: None,
        })
        .expect("publish returns a verdict rather than an error");

    let InstallVerdict::Refused { rule, errors } = &verdict else {
        panic!("expected a refusal, got {verdict:?}")
    };
    assert_eq!(*rule, "well_typed");
    assert!(errors[0].contains("rainfall"), "the gate's own words: {errors:?}");
    assert!(world.arena().all().is_empty(), "and nothing was stored");
}

#[test]
fn a_locked_node_cannot_publish_under_its_key() {
    let (world, _home) = node("publish-locked");
    world.lock();
    let spec = serde_json::to_value(garden()).expect("spec");
    let err = world
        .publish(&Publication {
            name: "Garden".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: None,
        })
        .expect_err("no key, no author");
    assert!(err.contains("locked"), "{err}");
}

// ── install ─────────────────────────────────────────────────────────────────

#[test]
fn a_published_definition_installs_and_a_sustain_can_be_made_from_it() {
    // ★★★ The whole loop: publish → install → instantiate, and the last step
    //     uses the SAME `instantiate_from` a built-in template uses.
    let (world, _home) = node("install");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world
        .publish(&Publication {
            name: "Garden".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: None,
        })
        .expect("publish");

    let out = world.install(&pkg.id, None).expect("install");
    assert!(out.verdict.admitted().is_some(), "{}", out.verdict.describe());
    assert_eq!(out.applied.as_deref(), Some("garden"));

    // It is now an authored definition on this node, like any other.
    assert!(world.definitions().iter().any(|d| d.id == "garden"));

    // ★★ And instantiating from it is the unmodified built-in path.
    world
        .instantiate_from("my-garden", "My Garden", TemplateId::Habitat, Some("garden"), None)
        .expect("instantiate");
    let state = world.with(|i| i.get("my-garden").map(|s| s.state.clone())).expect("state");
    assert_eq!(state["moisture"], json!(40.0));

    // The install is recorded, with the hash that was actually accepted.
    let installs = world.arena().installs();
    assert_eq!(installs.len(), 1);
    assert_eq!(installs[0].content_hash, pkg.content_hash);
}

#[test]
fn an_installed_definition_is_gated_exactly_like_a_local_one() {
    // ★★★ The no-bypass property, demonstrated rather than asserted: the
    //     invariant that came in through the package refuses a real operator
    //     call on a real instance.
    let (world, _home) = node("install-gated");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world
        .publish(&Publication {
            name: "Garden".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: None,
        })
        .expect("publish");
    world.install(&pkg.id, None).expect("install");
    world
        .instantiate_from("my-garden", "My Garden", TemplateId::Habitat, Some("garden"), None)
        .expect("instantiate");

    // An operator the definition does not permit is refused by the same
    // allow-list check every Sustain uses.
    let mut params = Map::new();
    params.insert("pocket_name".into(), json!("beds"));
    params.insert("amount".into(), json!(1.0));
    let (x, _) = world
        .call("my-garden", "budget.allocate", &params)
        .expect("call")
        .expect("sustain");
    assert!(!x.committed(), "an operator outside the package's own list");
}

#[test]
fn a_tampered_package_is_refused_before_the_gate_is_even_asked() {
    // ★★★ Integrity is checked first and separately. The artifact below would
    //     typecheck perfectly — it is refused because the bytes are not the
    //     ones that were published.
    let (world, home) = node("tampered");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world
        .publish(&Publication {
            name: "Garden".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: None,
        })
        .expect("publish");

    // Edit the stored line by hand, exactly as someone with a text editor
    // would, leaving the claimed hash behind.
    let path = home.join("packages.jsonl");
    let text = std::fs::read_to_string(&path).expect("read");
    let mut record: Value = serde_json::from_str(text.trim()).expect("parse");
    record["spec"]["openingState"]["moisture"] = json!(99.0);
    std::fs::write(&path, format!("{record}\n")).expect("write");

    let reopened = World::open(Store::at(&home).expect("store")).expect("reopen");
    reopened.unlock(PASS).expect("unlock");
    let out = reopened.install(&pkg.id, None).expect("install");

    assert!(matches!(out.provenance.integrity, Integrity::Altered { .. }));
    let InstallVerdict::Refused { rule, .. } = &out.verdict else {
        panic!("expected a refusal, got {:?}", out.verdict)
    };
    assert_eq!(*rule, "provenance");
    assert!(out.applied.is_none());
    assert!(!reopened.definitions().iter().any(|d| d.id == "garden"));
}

// ── widgets ─────────────────────────────────────────────────────────────────

fn widget(id: &str, input: &str) -> AuthoredWidget {
    AuthoredWidget {
        id: id.into(),
        render: "readout".into(),
        inputs: vec![input.into()],
        emits: vec![],
        event_class: None,
    }
}

#[test]
fn an_installed_widget_appears_in_the_real_feed() {
    // ★★★ Install is observable: the card is composed by the same
    //     `compose(r)` the built-in set goes through, and shows up ranked.
    let (world, _home) = node("widget");
    world
        .instantiate_owned("home", "Home", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");

    let spec = serde_json::to_value(widget("beds_card", "liquid")).expect("spec");
    let (pkg, verdict) = world
        .publish(&Publication {
            name: "Beds".to_string(),
            kind: "widget".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: Some("home".to_string()),
        })
        .expect("publish");
    assert!(verdict.admitted().is_some(), "{}", verdict.describe());

    let out = world.install(&pkg.id, Some("home")).expect("install");
    assert!(out.verdict.admitted().is_some(), "{}", out.verdict.describe());

    let (installed, undecodable) = world.arena().widgets_for("home");
    assert_eq!(installed.len(), 1);
    assert_eq!(undecodable, 0);

    let state = world.with(|i| i.get("home").map(|s| s.state.clone())).expect("state");
    let extra = installed.iter().map(|w| w.to_decl()).collect();
    let (_, view) = crate::orchie::feed(&world.operators, &state, 0, &[], None, extra)
        .expect("the feed loads the installed card alongside the built-ins");
    assert!(
        view.selected.iter().chain(view.excluded.iter()).any(|c| c.id == "beds_card"),
        "the installed card is a real candidate",
    );
}

#[test]
fn a_widget_reading_a_dimension_the_view_does_not_have_is_refused() {
    // ★★ And the refusal is the CURATED-UI load gate's, not a registry rule.
    let (world, _home) = node("widget-bad");
    world
        .instantiate_owned("home", "Home", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");

    // `finances.liquid.balance` is the HOUSEHOLD's shape; an Orchie card reads
    // the projection, which has `liquid`. This is exactly the confusion the
    // view-definition check exists to catch.
    let spec = serde_json::to_value(widget("wrong", "finances.liquid.balance")).expect("spec");
    let (_, verdict) = world
        .publish(&Publication {
            name: "Wrong".to_string(),
            kind: "widget".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: Some("home".to_string()),
        })
        .expect("publish");

    let InstallVerdict::Refused { rule, errors } = &verdict else {
        panic!("expected a refusal, got {verdict:?}")
    };
    assert_eq!(*rule, "widget_well_typed");
    assert!(!errors.is_empty());
    assert!(world.arena().all().is_empty(), "not stored either");
}

#[test]
fn a_widget_needs_a_household_to_be_checked_against() {
    let (world, _home) = node("widget-no-target");
    let spec = serde_json::to_value(widget("card", "liquid")).expect("spec");
    let (_, verdict) = world
        .publish(&Publication {
            name: "Card".to_string(),
            kind: "widget".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: None,
        })
        .expect("publish");
    let InstallVerdict::Refused { rule, .. } = &verdict else { panic!("expected a refusal") };
    assert_eq!(*rule, "needs_a_target");
}

// ── the honest third answer ─────────────────────────────────────────────────

#[test]
fn a_strategy_publishes_and_says_it_cannot_run_here() {
    // ★★★ Not refused (nothing is wrong with it) and not installed (nothing
    //     would execute it). The third answer, all the way through the host.
    let (world, _home) = node("strategy");
    let (pkg, verdict) = world
        .publish(&Publication {
            name: "Frugal".to_string(),
            kind: "strategy".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec: json!({"nodes": []}),
            per_mille: 0,
            into: None,
        })
        .expect("publish");
    assert!(matches!(verdict, InstallVerdict::NotHere { .. }), "{verdict:?}");
    // ★★ It IS stored: a registry that could only carry what this node happens
    //    to run would be a much smaller thing.
    assert_eq!(world.arena().all().len(), 1);

    let out = world.install(&pkg.id, None).expect("install");
    assert!(matches!(out.verdict, InstallVerdict::NotHere { .. }));
    assert!(out.applied.is_none(), "nothing was applied, and nothing pretended to be");
    assert!(world.arena().installs().is_empty());
}

#[test]
fn an_operator_package_is_a_name_and_must_already_resolve() {
    let (world, _home) = node("operator");
    let real = world.operators.names().first().cloned().expect("this build ships operators");

    let (pkg, verdict) = world
        .publish(&Publication {
            name: real.clone(),
            kind: "operator".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec: json!({ "name": real }),
            per_mille: 0,
            into: None,
        })
        .expect("publish");
    assert!(verdict.admitted().is_some(), "{}", verdict.describe());
    assert!(world.install(&pkg.id, None).expect("install").applied.is_some());

    // ★★★ And one this node does not have is refused — a package cannot carry
    //     an implementation, so there would be nothing to install.
    let (_, bad) = world
        .publish(&Publication {
            name: "ghost.op".to_string(),
            kind: "operator".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec: json!({"name": "ghost.op"}),
            per_mille: 0,
            into: None,
        })
        .expect("publish");
    let InstallVerdict::Refused { rule, errors } = &bad else { panic!("expected a refusal") };
    assert_eq!(*rule, "operator_present");
    assert!(errors[0].contains("cannot carry one"), "{errors:?}");
}

// ── royalty ─────────────────────────────────────────────────────────────────

#[test]
fn a_royalty_is_internal_juul_and_conserves_circulation() {
    // ★★★ The hard boundary, asserted: `settle` only transfers, so the total
    //     across every balance is unchanged. Nothing is minted, nothing leaves,
    //     and none of it is money.
    let (world, _home) = node("royalty");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world
        .publish(&Publication {
            name: "Garden".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 50,
            into: None,
        })
        .expect("publish");

    let before = world.circulation();
    let settlement = world.pay_royalty(&pkg.id, 1_000).expect("settle");
    assert!(settlement.settled(), "{settlement:?}");
    // 50 per mille of 1000 = 50 juul moved.
    assert_eq!(settlement.transferred(), 50);
    assert_eq!(world.circulation(), before, "a transfer, never a mint");
}

#[test]
fn a_free_package_transfers_nothing() {
    let (world, _home) = node("free");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world
        .publish(&Publication {
            name: "Garden".to_string(),
            kind: "definition".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tags: vec![],
            spec,
            per_mille: 0,
            into: None,
        })
        .expect("publish");
    let before = world.circulation();
    let settlement = world.pay_royalty(&pkg.id, 1_000).expect("settle");
    assert_eq!(settlement.transferred(), 0);
    assert_eq!(world.circulation(), before);
}
