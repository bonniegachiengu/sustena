//! **A publishes, B pulls and installs** — over the encrypted session.
//!
//! Slice 7 built a registry that could only ever see itself. This is the same
//! registry with a peer dimension, and the property under test is the one that
//! matters: **a package that arrived over a wire faces exactly the gate a
//! local one faces.** Coming from a peer buys visibility, never permission.

use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::package::{Authenticity, InstallVerdict, Origin};

use crate::arena::Publication;
use crate::definitions::AuthoredDefinition;
use crate::peers::Standing;
use crate::store::Store;
use crate::templates::TemplateId;
use crate::world::{World, DEFAULT_HANDLE};

const PASS: &str = "a-long-enough-passphrase";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mycelium-arena-wire-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

struct Node {
    world: World,
    port: u16,
}

impl Node {
    fn start(root: &std::path::Path, dir: &str) -> Node {
        let home = root.join(dir);
        std::fs::create_dir_all(&home).expect("node home");
        let world = World::open(Store::at(&home).expect("store")).expect("world");
        world.enrol(DEFAULT_HANDLE, PASS).expect("enrol");
        let port = world.listen(Some(0)).expect("listen");
        Node { world, port }
    }

    fn key(&self) -> String {
        self.world.node_id().expect("node id")
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

fn garden() -> AuthoredDefinition {
    serde_json::from_value(json!({
        "id": "garden",
        "label": "Garden",
        "dimensions": [
            { "path": "moisture", "kind": "number", "lo": 0.0, "hi": 100.0 }
        ],
        "operators": ["budget.record_income"],
        "invariants": [{ "id": "not_a_swamp", "expression": "moisture <= 100" }],
        "openingState": { "moisture": 40.0 }
    }))
    .expect("a well-formed authored definition")
}

fn publication(spec: Value, name: &str) -> Publication {
    Publication {
        name: name.to_string(),
        kind: "definition".into(),
        version: "1.0.0".into(),
        description: "grown next door".into(),
        tags: vec![],
        spec,
        per_mille: 0,
        into: None,
    }
}

/// Two nodes that know and trust each other.
fn peered(name: &str) -> (Node, Node) {
    let root = scratch(name);
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");
    for (me, them) in [(&a, &b), (&b, &a)] {
        me.world
            .peering()
            .edit(|book| {
                book.seen(&them.key(), "peer", Some(them.address()));
                book.set_standing(&them.key(), Standing::Trusted);
            })
            .expect("book");
    }
    (a, b)
}

// ── the loop ────────────────────────────────────────────────────────────────

#[test]
fn a_publishes_b_pulls_installs_through_the_real_gate_and_instantiates() {
    // ★★★ The whole increment, end to end and over the encrypted session.
    let (a, b) = peered("pull");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, verdict) = a
        .world
        .publish(&publication(spec, "Garden"))
        .expect("publish");
    assert!(verdict.admitted().is_some(), "{}", verdict.describe());

    // ── list ────────────────────────────────────────────────────────────────
    let (peer, offers) = b.world.peer_offers(&a.address()).expect("offers");
    assert_eq!(peer, a.key(), "the listing came from the key we authenticated");
    assert_eq!(offers.len(), 1);
    let offer = &offers[0];
    assert_eq!(offer.name, "Garden");
    assert_eq!(offer.content_hash, pkg.content_hash);
    assert!(offer.signed, "A published under its own key");
    // ★★ A listing is metadata. Nothing landed by looking.
    assert!(b.world.arena().all().is_empty(), "browsing is not fetching");

    // ── fetch ───────────────────────────────────────────────────────────────
    let got = b.world.fetch_package(&a.address(), &offer.content_hash).expect("fetch");
    assert_eq!(got.content_hash, pkg.content_hash);
    // ★★★ The origin is what it actually is — an import can never read as
    //     native, and the boundary rewrites it rather than trusting the record.
    assert_eq!(got.origin, Origin::FromPeer { peer: a.key() });
    assert_eq!(got.provenance().authenticity, Authenticity::Signed);
    assert_eq!(got.author, a.key(), "the author is still A's key, not B's");
    // ★★ Fetched is not installed.
    assert!(b.world.arena().installs().is_empty(), "arriving is not installing");

    // ── install, through the unmodified local path ──────────────────────────
    let out = b.world.install(&got.id, None).expect("install");
    assert!(out.verdict.admitted().is_some(), "{}", out.verdict.describe());
    assert_eq!(out.applied.as_deref(), Some("garden"));
    assert!(b.world.definitions().iter().any(|d| d.id == "garden"));

    // ── and a Sustain is made from it, the same way a built-in is ───────────
    b.world
        .instantiate_from("my-garden", "My Garden", TemplateId::Habitat, Some("garden"), None)
        .expect("instantiate");
    let state = b
        .world
        .with(|i| i.get("my-garden").map(|s| s.state.clone()))
        .expect("state");
    assert_eq!(state["moisture"], json!(40.0));
}

// ── refusals ────────────────────────────────────────────────────────────────

#[test]
fn a_broken_package_from_a_peer_is_refused_at_install_with_the_real_reason() {
    // ★★★ **No bypass because it came from a peer.** The same `editing::typecheck`
    //     that refuses a locally-authored artifact refuses this one, in the
    //     same words.
    let (a, b) = peered("broken");
    let mut broken = garden();
    broken.id = "broken-garden".into();
    broken.invariants[0].expression = "rainfall >= 0".into();

    // ★★ A cannot publish it either — the gate is the same there — so the
    //    package is placed into A's arena directly, which is the only way a
    //    broken artifact could ever reach a wire at all.
    let spec = serde_json::to_value(&broken).expect("spec");
    let hash = crate::arena::content_hash(&spec);
    a.world
        .arena()
        .record(crate::arena::Package {
            id: "pkg-broken".into(),
            name: "Broken".into(),
            kind: sustena_core::package::Kind::Definition,
            version: "1.0.0".into(),
            description: String::new(),
            tags: vec![],
            spec,
            author: a.key(),
            author_handle: DEFAULT_HANDLE.into(),
            content_hash: hash.clone(),
            signature: None,
            origin: Origin::Authored,
            per_mille: 0,
            published_at: 0,
        })
        .expect("record");
    a.world.refresh_offers();

    let got = b.world.fetch_package(&a.address(), &hash).expect("it transfers fine");
    // ★ It arrived intact — integrity says nothing about quality.
    assert_eq!(got.provenance().integrity, sustena_core::package::Integrity::Intact);

    let out = b.world.install(&got.id, None).expect("install");
    let InstallVerdict::Refused { rule, errors } = &out.verdict else {
        panic!("expected a refusal, got {:?}", out.verdict)
    };
    assert_eq!(*rule, "well_typed");
    assert!(errors[0].contains("rainfall"), "the gate's own words: {errors:?}");
    assert!(out.applied.is_none());
    assert!(!b.world.definitions().iter().any(|d| d.id == "broken-garden"));
}

#[test]
fn a_package_whose_bytes_do_not_match_its_hash_is_refused_on_arrival() {
    // ★★★ The check the AEAD cannot make. Nothing changed in flight — the
    //     session guarantees that — but the sender's own record was mangled at
    //     rest, and the bytes do not hash to what the record claims.
    let (a, b) = peered("mangled");
    let spec = serde_json::to_value(garden()).expect("spec");
    let honest = crate::arena::content_hash(&spec);

    // Stored claiming a hash its bytes do not have.
    a.world
        .arena()
        .record(crate::arena::Package {
            id: "pkg-mangled".into(),
            name: "Mangled".into(),
            kind: sustena_core::package::Kind::Definition,
            version: "1.0.0".into(),
            description: String::new(),
            tags: vec![],
            spec,
            author: a.key(),
            author_handle: DEFAULT_HANDLE.into(),
            content_hash: "de".repeat(32),
            signature: None,
            origin: Origin::Authored,
            per_mille: 0,
            published_at: 0,
        })
        .expect("record");
    a.world.refresh_offers();

    // ★★ It is not even OFFERED — a record that fails its own hash is this
    //    node's problem to notice, not something to hand to somebody else.
    let (_, offers) = b.world.peer_offers(&a.address()).expect("offers");
    assert!(
        !offers.iter().any(|o| o.name == "Mangled"),
        "a package that fails its own integrity check is not offered",
    );

    // And asking for it by the claimed hash gets nothing.
    let err = b
        .world
        .fetch_package(&a.address(), &"de".repeat(32))
        .expect_err("not offered");
    assert!(err.contains("no package"), "{err}");
    // The honest hash is not on offer either, since the record is broken.
    assert!(b.world.fetch_package(&a.address(), &honest).is_err());
    assert!(b.world.arena().all().is_empty(), "nothing landed");
}

#[test]
fn asking_for_one_package_and_being_sent_another_is_refused() {
    // ★★★ Why the fetch is BY HASH: what arrives is hashed again and compared
    //     to what was asked for, so a peer cannot answer a request for one
    //     artifact with a different one.
    let (a, b) = peered("substitution");
    let spec = serde_json::to_value(garden()).expect("spec");
    a.world.publish(&publication(spec, "Garden")).expect("publish");

    let err = b
        .world
        .fetch_package(&a.address(), &"ab".repeat(32))
        .expect_err("a hash A does not hold");
    assert!(err.contains("no package"), "{err}");
    assert!(b.world.arena().all().is_empty());
}

#[test]
fn an_unsigned_package_from_a_peer_behaves_exactly_like_an_unsigned_local_one() {
    // ★★ Unsigned is not forged, on the wire as at home. It installs, and the
    //    provenance says plainly what is and is not known about it.
    let (a, b) = peered("unsigned");
    let spec = serde_json::to_value(garden()).expect("spec");
    let hash = crate::arena::content_hash(&spec);
    a.world
        .arena()
        .record(crate::arena::Package {
            id: "pkg-unsigned".into(),
            name: "Unsigned".into(),
            kind: sustena_core::package::Kind::Definition,
            version: "1.0.0".into(),
            description: String::new(),
            tags: vec![],
            spec,
            author: a.key(),
            author_handle: DEFAULT_HANDLE.into(),
            content_hash: hash.clone(),
            signature: None,
            origin: Origin::Authored,
            per_mille: 0,
            published_at: 0,
        })
        .expect("record");
    a.world.refresh_offers();

    let (_, offers) = b.world.peer_offers(&a.address()).expect("offers");
    let offer = offers.iter().find(|o| o.name == "Unsigned").expect("offered");
    assert!(!offer.signed, "and the listing says so BEFORE it is fetched");

    let got = b.world.fetch_package(&a.address(), &hash).expect("fetch");
    let prov = got.provenance();
    assert_eq!(prov.authenticity, Authenticity::Unsigned);
    assert!(prov.safe_to_install(), "unsigned is not forged");
    assert!(prov.describe().contains("from"), "and it says where it came from: {}", prov.describe());

    let out = b.world.install(&got.id, None).expect("install");
    assert!(out.verdict.admitted().is_some(), "{}", out.verdict.describe());
}

#[test]
fn a_forged_signature_from_a_peer_is_refused_at_the_door() {
    // ★★★ Signed by nobody who holds the key it names. Refused on arrival
    //     rather than stored and refused later.
    let (a, b) = peered("forged");
    let spec = serde_json::to_value(garden()).expect("spec");
    let hash = crate::arena::content_hash(&spec);
    a.world
        .arena()
        .record(crate::arena::Package {
            id: "pkg-forged".into(),
            name: "Forged".into(),
            kind: sustena_core::package::Kind::Definition,
            version: "1.0.0".into(),
            description: String::new(),
            tags: vec![],
            spec,
            author: a.key(),
            author_handle: DEFAULT_HANDLE.into(),
            content_hash: hash.clone(),
            // Syntactically fine, verifies against nothing.
            signature: Some("11".repeat(64)),
            origin: Origin::Authored,
            per_mille: 0,
            published_at: 0,
        })
        .expect("record");
    a.world.refresh_offers();

    let err = b.world.fetch_package(&a.address(), &hash).expect_err("forged");
    assert!(err.contains("does not verify"), "{err}");
    assert!(b.world.arena().all().is_empty(), "nothing was stored");
}

#[test]
fn browsing_a_peer_that_is_not_listening_fails_honestly() {
    let (a, b) = peered("unreachable");
    drop(a);
    let err = b.world.peer_offers("127.0.0.1:1").expect_err("nothing there");
    assert!(err.contains("cannot reach"), "{err}");
}
