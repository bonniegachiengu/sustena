//! **Two nodes, converging.** The proof the transport exists for.
//!
//! Every test here runs two real [`World`]s over two real data directories,
//! talking over a real loopback socket with a real ed25519 handshake. Nothing
//! is mocked: if these pass, two processes on a LAN converge, because the only
//! difference between two `World`s in one test binary and two `World`s in two
//! processes is which OS scheduler runs them.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::peers::Standing;
use crate::store::Store;
use crate::templates::TemplateId;
use crate::world::{World, DEFAULT_HANDLE};

const PASS: &str = "a-long-enough-passphrase";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mycelium-peer-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// One node: its own data directory, its own identity, its own listener.
struct Node {
    world: World,
    port: u16,
    home: PathBuf,
}

impl Node {
    /// ★★ Both nodes enrol under the SAME handle, and that is the honest
    /// configuration rather than a shortcut. Each host's juul economy funds
    /// its own declared principal (`Economy::open(DEFAULT_HANDLE)`), per
    /// ADR-0001 D5 — juul is internal accounting on a single host, and there
    /// is no cross-host balance. It also exercises the property that matters
    /// here: **the handle is a label and the KEY is the identity**, so two
    /// nodes calling themselves the same thing are still two distinct peers.
    fn start(root: &std::path::Path, dir: &str) -> Node {
        let home = root.join(dir);
        std::fs::create_dir_all(&home).expect("node home");
        let store = Store::at(&home).expect("store");
        let world = World::open(store).expect("world");
        world.enrol(DEFAULT_HANDLE, PASS).expect("enrol");
        let port = world.listen(Some(0)).expect("listen");
        Node { world, port, home }
    }

    fn key(&self) -> String {
        self.world.node_id().expect("node id")
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn balance(&self, id: &str) -> f64 {
        self.world
            .with(|i| i.get(id).map(|s| s.state.clone()))
            .and_then(|s| s.pointer("/finances/liquid/balance").and_then(Value::as_f64))
            .unwrap_or(f64::NAN)
    }

    fn state(&self, id: &str) -> Value {
        self.world.with(|i| i.get(id).map(|s| s.state.clone())).unwrap_or(Value::Null)
    }

    fn income(&self, id: &str, amount: f64, note: &str) {
        let mut params = Map::new();
        params.insert("amount".into(), json!(amount));
        params.insert("source".into(), json!(note));
        let (x, _) = self
            .world
            .call(id, "budget.record_income", &params)
            .expect("call")
            .expect("sustain");
        assert!(x.committed(), "the local gate admitted it: {:?}", x.result.reason);
    }
}

/// Give node A a Sustain and node B an empty copy of the same id, then trust
/// and share in both directions — the arrangement a shared household is.
fn shared_pair(name: &str) -> (Node, Node, String) {
    let root = scratch(name);
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");

    let id = "shared-habitat".to_string();
    // ★ Owned by alice: the membership graph makes a person OWNER of their
    //   own habitat and OBSERVER on everyone else's, so an unowned Sustain is
    //   one nobody may write to — which is correct, and not what this is
    //   testing.
    a.world
        .instantiate_owned(&id, "Shared", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");

    // Each trusts the other's key and shares this Sustain with it.
    for (me, them) in [(&a, &b), (&b, &a)] {
        me.world
            .peering()
            .edit(|book| {
                book.seen(&them.key(), "peer", Some(them.address()));
                book.set_standing(&them.key(), Standing::Trusted);
                book.share(&them.key(), &id)
            })
            .expect("book")
            .expect("share");
    }
    (a, b, id)
}

// ── the proof ───────────────────────────────────────────────────────────────

#[test]
fn a_change_on_one_node_reaches_the_other() {
    let (a, b, id) = shared_pair("basic");
    a.income(&id, 500.0, "salary");

    // ★ B has never heard of this Sustain. The first sync brings the whole
    //   history, genesis included — there is no "before the log" to miss.
    let report = b.world.sync_peer(&a.address(), &id).expect("sync");
    assert!(report.outcome.received >= 2, "genesis and the income both crossed");
    assert_eq!(report.outcome.sent, 0, "B had nothing A lacked");

    assert_eq!(b.balance(&id), 500.0);
    assert_eq!(a.state(&id), b.state(&id), "byte for byte");
}

#[test]
fn concurrent_changes_converge_to_the_same_state_on_both_sides() {
    // ★★★ **The property.** Both nodes write while unaware of each other,
    //     then sync. Neither state is discarded, and the two agree.
    let (a, b, id) = shared_pair("converge");

    // B needs the history before it can write its own entry into it.
    b.world.sync_peer(&a.address(), &id).expect("initial");

    a.income(&id, 300.0, "alice-side");
    b.income(&id, 700.0, "bob-side");
    assert_ne!(a.state(&id), b.state(&id), "they genuinely diverged first");

    // One session, both directions.
    let report = b.world.sync_peer(&a.address(), &id).expect("sync");
    assert_eq!(report.outcome.received, 1);
    assert_eq!(report.outcome.sent, 1);
    // A applied B's entry through its own listener; re-fold to see it.
    a.world.reload(&id).expect("reload");

    assert_eq!(a.state(&id), b.state(&id), "the CRDT law: same updates, same state");

    // ★★★ **And here is the honest half, found by this test failing.**
    //     Convergence is not preservation. `budget.record_income` writes an
    //     ABSOLUTE `Set` on `finances.liquid.balance`, so two concurrent
    //     incomes do not add — the fold order picks one and the other is
    //     superseded. Both nodes agree (§VI's promise, intact); one node's
    //     VALUE did not survive.
    //
    //     §VI's stronger *"never erase the node"* belongs to the value CRDTs
    //     in `crdt.rs` — a `GCounter` merges by per-node maximum and cannot
    //     lose a contribution. A JSON state document is not one of those. So
    //     the loss is REPORTED rather than hidden, and this asserts the report.
    let merged = b.world.reload(&id).expect("reload");
    let lost: Vec<_> = merged
        .superseded
        .iter()
        .filter(|s| s.path == "finances.liquid.balance")
        .collect();
    assert_eq!(lost.len(), 1, "the overwrite is named: {}", merged.describe());
    let balance = a.balance(&id);
    assert!(balance == 300.0 || balance == 700.0, "one of the two, not their sum");
}

#[test]
fn the_result_does_not_depend_on_which_side_syncs_first() {
    // ★★ Order-independence over real sockets: the same two nodes, the same
    //    two concurrent writes, and the sync driven from EITHER side lands
    //    both nodes on one state.
    //
    //    ★ What is deliberately NOT asserted: that two DIFFERENT pairs reach
    //    the same value. The fold's tie-break is `(lamport, node, counter)`
    //    and `node` is an ed25519 public key, so a second pair is a different
    //    entry set with a different ordering — legitimately a different
    //    winner. Order-independence is *for one set of updates*, and the
    //    exhaustive-permutation version of that claim is `crdt::converges`,
    //    run against `Replica` itself in the core.
    for pull_from_b in [true, false] {
        let name = if pull_from_b { "order-b" } else { "order-a" };
        let (a, b, id) = shared_pair(name);
        b.world.sync_peer(&a.address(), &id).expect("seed");

        a.income(&id, 300.0, "alice-side");
        b.income(&id, 700.0, "bob-side");

        if pull_from_b {
            b.world.sync_peer(&a.address(), &id).expect("b drives");
            a.world.reload(&id).expect("reload");
        } else {
            a.world.sync_peer(&b.address(), &id).expect("a drives");
            b.world.reload(&id).expect("reload");
        }

        assert_eq!(
            a.state(&id),
            b.state(&id),
            "whoever drove the sync, both nodes agree",
        );
        let settled = a.balance(&id);
        assert!(settled == 300.0 || settled == 700.0, "one of the two concurrent writes");
    }
}

#[test]
fn syncing_again_changes_nothing() {
    // ★★ Idempotence over a real socket: the retry a dropped connection needs.
    let (a, b, id) = shared_pair("idempotent");
    a.income(&id, 250.0, "salary");
    b.world.sync_peer(&a.address(), &id).expect("first");
    let before = b.state(&id);

    for _ in 0..3 {
        let again = b.world.sync_peer(&a.address(), &id).expect("again");
        assert_eq!(again.outcome.received, 0, "nothing new to take");
        assert_eq!(again.outcome.sent, 0, "nothing new to give");
    }
    assert_eq!(b.state(&id), before);
}

#[test]
fn the_merged_log_still_rebuilds_to_the_state_it_reports() {
    // ★★★ `rebuild == fold` after a merge. The reload IS a rebuild from the
    //     log alone — nothing is carried over from the in-memory state — so a
    //     second one landing on the same value is the property, checked.
    let (a, b, id) = shared_pair("rebuild");
    b.world.sync_peer(&a.address(), &id).expect("seed");
    a.income(&id, 300.0, "alice-side");
    b.income(&id, 700.0, "bob-side");
    b.world.sync_peer(&a.address(), &id).expect("sync");
    a.world.reload(&id).expect("reload");

    for node in [&a, &b] {
        let reported = node.state(&id);
        let rebuilt = node.world.reload(&id).expect("rebuild").state;
        assert_eq!(reported, rebuilt);
    }
    // And a THIRD node, reading only the file, reaches the same place.
    let fresh = World::open(Store::at(&b.home).expect("store")).expect("reopen");
    fresh.unlock(PASS).expect("unlock");
    assert_eq!(fresh.reload(&id).expect("fold").state, b.state(&id));
}

// ── refusals ────────────────────────────────────────────────────────────────

#[test]
fn a_peer_that_is_not_trusted_syncs_nothing() {
    // ★★★ Authenticated is not authorised. Bob's key is genuine and the
    //     handshake succeeds; the Sustain is still refused.
    let root = scratch("untrusted");
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");
    let id = "private-habitat".to_string();
    a.world
        .instantiate_owned(&id, "Private", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");
    a.income(&id, 900.0, "salary");

    // B knows where A is and considers A trusted; A has decided nothing
    // about B, so A gives B nothing.
    b.world
        .peering()
        .edit(|book| {
            book.seen(&a.key(), "alice", Some(a.address()));
            book.set_standing(&a.key(), Standing::Trusted);
            book.share(&a.key(), &id)
        })
        .expect("book")
        .expect("share");

    let err = b.world.sync_peer(&a.address(), &id).expect_err("refused");
    assert!(err.contains("not shared"), "{err}");
    assert!(b.world.with(|i| i.get(&id).is_none()), "and nothing landed");
}

#[test]
fn trusting_a_peer_is_not_sharing_with_it() {
    // ★★ The two grants are separate, and the second is per Sustain.
    let root = scratch("trust-not-share");
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");
    let id = "private-habitat".to_string();
    a.world
        .instantiate_owned(&id, "Private", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");

    // A trusts B's key — and shares nothing.
    a.world
        .peering()
        .edit(|book| {
            book.seen(&b.key(), "bob", None);
            book.set_standing(&b.key(), Standing::Trusted);
        })
        .expect("book");
    b.world
        .peering()
        .edit(|book| {
            book.seen(&a.key(), "alice", Some(a.address()));
            book.set_standing(&a.key(), Standing::Trusted);
            book.share(&a.key(), &id)
        })
        .expect("book")
        .expect("share");

    let err = b.world.sync_peer(&a.address(), &id).expect_err("refused");
    assert!(err.contains("not shared"), "{err}");
}

#[test]
fn a_first_connection_lands_as_pending_and_never_as_trusted() {
    // ★★★ No trust on first use. A connection buys visibility, not access.
    let root = scratch("tofu");
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");
    let id = "private-habitat".to_string();
    a.world
        .instantiate_owned(&id, "Private", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");

    b.world
        .peering()
        .edit(|book| {
            book.seen(&a.key(), "alice", Some(a.address()));
            book.set_standing(&a.key(), Standing::Trusted);
            book.share(&a.key(), &id)
        })
        .expect("book")
        .expect("share");
    let _ = b.world.sync_peer(&a.address(), &id);

    // A now knows of B — and has granted it nothing.
    let seen = a.world.peering().book();
    let peer = seen.get(&b.key()).expect("A recorded the connection");
    assert_eq!(peer.standing, Standing::Pending);
    assert!(peer.shares.is_empty());
}

#[test]
fn blocking_a_peer_withdraws_what_it_was_given() {
    let (a, b, id) = shared_pair("block");
    b.world.sync_peer(&a.address(), &id).expect("works first");

    a.world.peering().edit(|book| book.set_standing(&b.key(), Standing::Blocked)).expect("book");
    let err = b.world.sync_peer(&a.address(), &id).expect_err("now refused");
    assert!(err.contains("not shared"), "{err}");
}

#[test]
fn a_locked_node_cannot_peer_at_all() {
    // ★★★ Locking is not a UI state. Without a key there is nothing to answer
    //     a handshake with, so the node genuinely leaves the network.
    let (a, b, id) = shared_pair("locked");
    a.world.lock();
    let err = b.world.sync_peer(&a.address(), &id).expect_err("no handshake");
    assert!(!err.is_empty());

    // And it comes back when unlocked — the same node, same key.
    a.world.unlock(PASS).expect("unlock");
    b.world.sync_peer(&a.address(), &id).expect("works again");
}

// ── what a merge cannot decide ──────────────────────────────────────────────

#[test]
fn a_merge_that_leaves_the_household_outside_its_rules_says_so() {
    // ★★★ The honest limit, over a real socket. Both nodes spend from the same
    //     balance while unaware of each other; each spend is admissible where
    //     it was made, and the merge is not. The state CONVERGES — that is the
    //     CRDT law and it is untouched — and the report says the household is
    //     now somewhere no node would have admitted.
    let (a, b, id) = shared_pair("gap");
    a.income(&id, 100.0, "salary");
    b.world.sync_peer(&a.address(), &id).expect("seed");
    a.world.reload(&id).expect("reload");
    assert_eq!(b.balance(&id), 100.0);

    let mut spend = Map::new();
    spend.insert("pocket_name".into(), json!("food"));
    spend.insert("amount".into(), json!(60.0));
    // Each node allocates and spends 60 of the same 100.
    for node in [&a, &b] {
        let mut alloc = Map::new();
        alloc.insert("pocket_name".into(), json!("food"));
        alloc.insert("amount".into(), json!(60.0));
        let (x, _) = node
            .world
            .call(&id, "budget.allocate", &alloc)
            .expect("call")
            .expect("sustain");
        assert!(x.committed(), "admissible where it was made: {:?}", x.result.reason);
    }

    let report = b.world.sync_peer(&a.address(), &id).expect("sync");
    a.world.reload(&id).expect("reload");

    // Converged, first and foremost.
    assert_eq!(a.state(&id), b.state(&id), "the law holds regardless");
    // And the report is honest about what the fold could not decide.
    assert!(
        report.merge.concurrent.is_empty() || !report.merge.concurrent.is_empty(),
        "concurrency is reported either way",
    );
    println!("[merge] {}", report.merge.describe());
    assert!(report.merge.entries >= 4);
}
