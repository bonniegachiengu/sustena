//! **An honest trust reading, and an order in juul.**
//!
//! The reference writes `trust_score = 0.0` at publish, never updates it, and
//! sorts the catalogue by it. These tests are the answer to *what could that
//! number honestly be made of* — and the most important one asserts that when
//! the answer is **nothing**, the reading says so instead of inventing a zero.



use serde_json::{json, Value};
use sustena_core::package::{Kind, Origin};
use sustena_core::trust::{TrustSignal, TrustStanding};

use crate::arena::{content_hash, Package, Publication};
use crate::definitions::AuthoredDefinition;
use crate::peers::Standing;
use crate::store::Store;
use crate::world::{World, DEFAULT_HANDLE};

const PASS: &str = "a-long-enough-passphrase";

fn node(name: &str) -> World {
    let home = std::env::temp_dir().join(format!("mycelium-trust-{name}"));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("home");
    let world = World::open(Store::at(&home).expect("store")).expect("world");
    world.enrol(DEFAULT_HANDLE, PASS).expect("enrol");
    world
}

fn garden() -> AuthoredDefinition {
    serde_json::from_value(json!({
        "id": "garden",
        "label": "Garden",
        "dimensions": [{ "path": "moisture", "kind": "number", "lo": 0.0, "hi": 100.0 }],
        "operators": ["budget.record_income"],
        "invariants": [{ "id": "not_a_swamp", "expression": "moisture <= 100" }],
        "openingState": { "moisture": 40.0 }
    }))
    .expect("authored")
}

fn publication(spec: Value, per_mille: u32) -> Publication {
    Publication {
        name: "Garden".into(),
        kind: "definition".into(),
        version: "1.0.0".into(),
        description: String::new(),
        tags: vec![],
        spec,
        per_mille,
        into: None,
    }
}

/// A package by somebody else, placed directly into the arena.
fn by_a_stranger(author: &str, signature: Option<&str>) -> Package {
    let spec = serde_json::to_value(garden()).expect("spec");
    let hash = content_hash(&spec);
    Package {
        id: format!("pkg-{}", &hash[..12]),
        name: "Garden".into(),
        kind: Kind::Definition,
        version: "1.0.0".into(),
        description: String::new(),
        tags: vec![],
        spec,
        author: author.to_string(),
        author_handle: "someone".into(),
        content_hash: hash,
        signature: signature.map(str::to_string),
        origin: Origin::FromPeer { peer: author.to_string() },
        per_mille: 0,
        published_at: 0,
    }
}

// ── the readings ────────────────────────────────────────────────────────────

#[test]
fn an_unsigned_package_from_a_stranger_nobody_holds_is_unrated_not_zero() {
    // ★★★ **The whole point.** There is nothing to go on, and the reading says
    //     so rather than producing a number that reads as somebody's low
    //     opinion. Nobody has an opinion.
    let world = node("unrated");
    let pkg = by_a_stranger(&"cc".repeat(32), None);
    world.arena().record(pkg.clone()).expect("record");

    let trust = world.trust_in(&pkg, &[]);
    assert_eq!(trust.standing, TrustStanding::Unrated);
    assert!(!trust.standing.is_a_reading(), "it must not render on the rated scale");
    assert!(trust.signals.contains(&TrustSignal::Unsigned));
    assert!(trust.signals.contains(&TrustSignal::AuthorUnknown));
    assert!(trust.describe().contains("unrated"), "{}", trust.describe());
}

#[test]
fn your_own_published_package_reads_first_hand() {
    let world = node("first-hand");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world.publish(&publication(spec, 0)).expect("publish");

    let trust = world.trust_in(&pkg, &[]);
    assert_eq!(trust.standing, TrustStanding::FirstHand);
    assert!(trust.signals.contains(&TrustSignal::Signed));
    assert!(trust.signals.contains(&TrustSignal::AuthoredHere));
    assert!(trust.describe().contains("you wrote it"));
}

#[test]
fn signed_by_a_trusted_peer_and_held_by_peers_reads_corroborated() {
    // ★★ Both real signals: an author you decided to trust, and peers who
    //    actually offered these exact bytes when you asked.
    let world = node("corroborated");
    let author = "aa".repeat(32);
    let mut pkg = by_a_stranger(&author, None);
    // Signed properly by that key would need its private half; what matters
    // for the band here is the author relationship plus the holdings.
    pkg.signature = None;
    world.arena().record(pkg.clone()).expect("record");
    world
        .peering()
        .edit(|book| {
            book.seen(&author, "bob", Some("127.0.0.1:1".into()));
            book.set_standing(&author, Standing::Trusted);
        })
        .expect("book");

    let holdings = vec![
        ("peer-1".to_string(), vec![pkg.content_hash.clone()]),
        ("peer-2".to_string(), vec![]),
    ];
    let trust = world.trust_in(&pkg, &holdings);
    assert_eq!(trust.standing, TrustStanding::Corroborated);
    assert!(trust
        .signals
        .contains(&TrustSignal::AuthorIsATrustedPeer { handle: "bob".into() }));
    assert!(trust.signals.contains(&TrustSignal::HeldByPeers { count: 1, of: 2 }));
    assert!(trust.describe().contains("1 of the 2"), "{}", trust.describe());
}

#[test]
fn having_merely_met_the_author_is_not_a_recommendation() {
    // ★★ `Pending` is not trust. A key that connected once and was never
    //    approved says nothing about the artifact it wrote.
    let world = node("pending-author");
    let author = "aa".repeat(32);
    let pkg = by_a_stranger(&author, None);
    world.arena().record(pkg.clone()).expect("record");
    world
        .peering()
        .edit(|book| book.seen(&author, "someone", None))
        .expect("book");

    let trust = world.trust_in(&pkg, &[]);
    assert!(trust.signals.contains(&TrustSignal::AuthorUnknown));
    assert_eq!(trust.standing, TrustStanding::Unrated);
}

#[test]
fn asking_peers_and_finding_none_is_reported_as_asking() {
    // ★★★ *Held by 0 of the 3 peers you asked* is a different fact from *you
    //     have not looked*, and the reading distinguishes them.
    let world = node("asked");
    let pkg = by_a_stranger(&"cc".repeat(32), None);
    world.arena().record(pkg.clone()).expect("record");

    let never_looked = world.trust_in(&pkg, &[]);
    assert!(
        !never_looked.signals.iter().any(|s| matches!(s, TrustSignal::HeldByPeers { .. })),
        "no holdings signal at all when nobody was asked",
    );

    let asked = world.trust_in(
        &pkg,
        &[
            ("a".into(), vec![]),
            ("b".into(), vec![]),
            ("c".into(), vec![]),
        ],
    );
    assert!(asked.signals.contains(&TrustSignal::HeldByPeers { count: 0, of: 3 }));
    assert!(asked.describe().contains("0 of the 3"));
}

#[test]
fn installing_it_makes_the_reading_first_hand() {
    // ★★ It passed your own gate, which beats anyone's opinion.
    let world = node("installed");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world.publish(&publication(spec, 0)).expect("publish");
    world.install(&pkg.id, None).expect("install");

    let trust = world.trust_in(&pkg, &[]);
    assert_eq!(trust.standing, TrustStanding::FirstHand);
    assert!(trust.signals.contains(&TrustSignal::InstalledHere));
}

// ── trust never gates ───────────────────────────────────────────────────────

#[test]
fn an_unrated_package_still_installs_through_the_real_gate() {
    // ★★★ **Trust informs; the gate decides.** The lowest possible reading and
    //     it installs anyway, because the question the gate asks is whether it
    //     typechecks — not whether anyone likes it.
    let world = node("unrated-installs");
    let pkg = by_a_stranger(&"cc".repeat(32), None);
    world.arena().record(pkg.clone()).expect("record");
    assert_eq!(world.trust_in(&pkg, &[]).standing, TrustStanding::Unrated);

    let out = world.install(&pkg.id, None).expect("install");
    assert!(out.verdict.admitted().is_some(), "{}", out.verdict.describe());
    assert_eq!(out.applied.as_deref(), Some("garden"));
}

#[test]
fn a_well_regarded_package_that_does_not_typecheck_is_still_refused() {
    // ★★★ The other direction, and the one that matters more: being liked is
    //     not being correct.
    let world = node("liked-but-broken");
    let author = "aa".repeat(32);
    let mut broken = garden();
    broken.id = "broken-garden".into();
    broken.invariants[0].expression = "rainfall >= 0".into();
    let spec = serde_json::to_value(&broken).expect("spec");
    let hash = content_hash(&spec);
    let pkg = Package {
        id: "pkg-broken".into(),
        name: "Broken".into(),
        kind: Kind::Definition,
        version: "1.0.0".into(),
        description: String::new(),
        tags: vec![],
        spec,
        author: author.clone(),
        author_handle: "bob".into(),
        content_hash: hash,
        signature: None,
        origin: Origin::Authored,
        per_mille: 0,
        published_at: 0,
    };
    world.arena().record(pkg.clone()).expect("record");
    world
        .peering()
        .edit(|book| {
            book.seen(&author, "bob", Some("127.0.0.1:1".into()));
            book.set_standing(&author, Standing::Trusted);
        })
        .expect("book");

    let holdings = vec![("p1".to_string(), vec![pkg.content_hash.clone()])];
    assert_eq!(world.trust_in(&pkg, &holdings).standing, TrustStanding::Corroborated);

    let out = world.install(&pkg.id, None).expect("install");
    assert!(out.verdict.admitted().is_none(), "corroboration is not a typecheck");
    assert!(out.applied.is_none());
}

// ── orders ──────────────────────────────────────────────────────────────────

#[test]
fn an_order_settles_a_real_royalty_in_juul_with_circulation_conserved() {
    // ★★★ The hard boundary, asserted rather than promised: an order moves
    //     internal credit between balances on this host. Nothing is minted,
    //     nothing leaves, and none of it is money.
    let world = node("order");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world.publish(&publication(spec, 50)).expect("publish");

    let before = world.circulation();
    let order = world.place_order(&pkg.id, 1_000).expect("order");
    let after = world.circulation();

    assert_eq!(after, before, "a transfer, never a mint");
    assert_eq!(order.paid, 50, "50 per mille of 1,000 juul");
    assert_eq!(order.per_mille, 50);
    assert!(order.reference.starts_with("SXI-"));
    assert_eq!(order.package_name, "Garden");
    assert!(!order.shares.is_empty(), "every share is recorded, including the ones that stayed");

    // And it is on disk, readable back.
    let orders = world.arena().orders();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0], order);
}

#[test]
fn a_free_package_orders_at_zero_and_is_still_an_order() {
    // ★★ Free of royalty, never free of the work it causes — and taking it is
    //    still a thing that happened, so it is still recorded.
    let world = node("free-order");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world.publish(&publication(spec, 0)).expect("publish");

    let before = world.circulation();
    let order = world.place_order(&pkg.id, 1_000).expect("order");
    assert_eq!(order.paid, 0);
    assert_eq!(order.per_mille, 0);
    assert_eq!(world.circulation(), before);
    assert_eq!(world.arena().orders().len(), 1, "a free acquisition is still an acquisition");
}

#[test]
fn ordering_is_not_installing() {
    // ★★★ Paying a contribution is not passing a gate, in either direction.
    let world = node("order-not-install");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world.publish(&publication(spec, 10)).expect("publish");

    world.place_order(&pkg.id, 500).expect("order");
    assert!(world.arena().installs().is_empty(), "an order installs nothing");

    // And installing without ordering is not blocked — the royalty is a
    // contribution, not a licence check.
    let world2 = node("install-without-order");
    let spec2 = serde_json::to_value(garden()).expect("spec");
    let (pkg2, _) = world2.publish(&publication(spec2, 10)).expect("publish");
    let out = world2.install(&pkg2.id, None).expect("install");
    assert!(out.verdict.admitted().is_some());
    assert!(world2.arena().orders().is_empty(), "and nothing was charged for it");
}

#[test]
fn an_order_nobody_can_afford_moves_nothing_and_is_not_recorded() {
    // ★ The honest refusal: the ledger is unchanged and there is no order
    //   claiming something happened.
    let world = node("unaffordable");
    let spec = serde_json::to_value(garden()).expect("spec");
    let (pkg, _) = world.publish(&publication(spec, 1_000)).expect("publish");

    let before = world.circulation();
    // A royalty larger than this host's entire genesis allocation.
    let err = world.place_order(&pkg.id, 99_000_000_000).expect_err("cannot cover it");
    assert!(err.contains("nothing moved"), "{err}");
    assert_eq!(world.circulation(), before);
    assert!(world.arena().orders().is_empty());
}
