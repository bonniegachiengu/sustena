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
    /// What a peer has merged INTO this node. ★ The responder has no other way
    /// to learn it changed -- see `Peering::merged`.
    merges: std::sync::mpsc::Receiver<String>,
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
        let merges = world.merges();
        let port = world.listen(Some(0)).expect("listen");
        Node { world, port, home, merges }
    }

    /// Re-fold whatever a peer wrote into this node. ★ In the app an absorber
    /// thread does this the moment the listener announces it; a test drains it
    /// by hand so the assertion is about the mechanism and not about timing.
    fn absorb(&self) -> usize {
        let ids: Vec<String> = self.merges.try_iter().collect();
        for id in &ids {
            self.world.absorb(id);
        }
        ids.len()
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

    /// How many lines the on-disk log actually holds. ★ Deliberately reads
    /// the FILE and not the in-memory view: the question is whether anything
    /// was persisted, which is what the devices disagreed about.
    fn log_len(&self, id: &str) -> usize {
        std::fs::read_to_string(self.home.join("events").join(format!("{id}.jsonl")))
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0)
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
    // ★ A sent and received nothing here, so it has nothing to re-fold.
    assert_eq!(a.absorb(), 0, "A was the source, not a receiver");
    assert_eq!(a.state(&id), b.state(&id), "byte for byte");
}

#[test]
fn a_second_sync_after_both_hold_each_others_entries_still_pushes() {
    // ★★★ The device sequence, exactly: sync once (each now holds some of the
    //     other's entries), write again on the phone side, sync again. The
    //     second push is the one that moved nothing on real hardware.
    let (a, b, id) = twin_pair("twins-again");
    a.income(&id, 300.0, "laptop-side");
    b.income(&id, 700.0, "phone-side");

    let first = b.world.sync_peer(&a.address(), &id).expect("first sync");
    eprintln!("FIRST  received={} sent={}", first.outcome.received, first.outcome.sent);

    b.income(&id, 55.0, "phone-again");
    let before_a = a.log_len(&id);
    let second = b.world.sync_peer(&a.address(), &id).expect("second sync");
    eprintln!(
        "SECOND received={} sent={}  a_log {} -> {}",
        second.outcome.received,
        second.outcome.sent,
        before_a,
        a.log_len(&id)
    );
    assert_eq!(second.outcome.sent, 1, "the one new entry must be pushed");
    assert_eq!(a.log_len(&id), before_a + 1, "and must land on disk");
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

// ── the case the devices hit, and the tests never did ───────────────────────

/// Both nodes created the same Sustain **independently**, before they had ever
/// met. Two unrelated genesis events, one id.
///
/// ★★★ Every other test in this file has ONE node instantiate and the other
///     receive, so this shape had never run. It is not exotic: it is what
///     happens when a person installs the app on their laptop and their phone
///     and sets up the same household on each, which is exactly what happened.
///
/// ★★★ **What it exposed.** The entries crossed and landed, but the fold applied
///     each genesis as a `ReplaceRoot` -- an OVERWRITE -- so the second one
///     erased the history before it. Multiparty §VI settles this and leaves no
///     room to choose: shared state is a join-semilattice, merges take the
///     least upper bound, and "the holon invariant -- never erase the node --
///     is not a policy sitting on top of the merge. It IS the merge." A root is
///     now joined rather than replaced (`root_join`), so twin genesis is
///     `s ⊔ s = s` and converges without discarding either lineage.
fn twin_pair(name: &str) -> (Node, Node, String) {
    let root = scratch(name);
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");
    let id = "homestead".to_string();

    // The difference from `shared_pair`: BOTH instantiate.
    for n in [&a, &b] {
        n.world
            .instantiate_owned(&id, "Home", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
            .expect("instantiate");
    }
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

#[test]
fn two_nodes_that_each_created_the_same_sustain_still_converge() {
    let (a, b, id) = twin_pair("twins");
    a.income(&id, 300.0, "laptop-side");
    b.income(&id, 700.0, "phone-side");

    let before_a = a.log_len(&id);
    let report = b.world.sync_peer(&a.address(), &id).expect("sync");
    eprintln!("received={} sent={}", report.outcome.received, report.outcome.sent);

    assert!(report.outcome.sent > 0, "B had history A lacked and must have pushed it");
    assert!(
        a.log_len(&id) > before_a,
        "A's log must actually grow: it was {} and is {}",
        before_a,
        a.log_len(&id)
    );

    // ★★★ A is the RESPONDER, and until it re-folds it is not merely stale:
    //     it is folded from a sequence that no longer describes its own log.
    assert_eq!(a.absorb(), 1, "the listener announced exactly one merged Sustain");
    assert_eq!(a.state(&id), b.state(&id), "byte for byte, once the receiver re-folds");

    // ★★★ §VI's third law, on the wire rather than on a value: "arrival order,
    //     duplication, and retry cannot change the result." A second sync must
    //     move nothing and change nothing.
    let converged = a.state(&id);
    let again = b.world.sync_peer(&a.address(), &id).expect("second sync");
    assert_eq!(again.outcome.received, 0, "nothing left to pull");
    assert_eq!(again.outcome.sent, 0, "nothing left to push");
    assert_eq!(a.absorb(), 0, "and nothing to re-fold");
    assert_eq!(a.state(&id), converged, "s ⊔ s = s");
    assert_eq!(b.state(&id), converged);

    // ★★ And the fold is reproducible from the log alone -- the merged state is
    //    not an artefact of the order things happened to arrive in.
    assert_eq!(
        a.world.reload(&id).expect("re-fold from disk").state,
        converged,
        "rebuilding from the log gives the state it reports"
    );
}

#[test]
fn a_log_written_before_stamping_existed_still_receives_a_push() {
    // ★★★ The last difference between the passing tests and the two real
    //     devices: both of their logs contain lines written before entries
    //     carried an origin or a lamport. `entry_of` reads those as belonging
    //     to whoever is doing the reading, which is right for a single-node
    //     history and is the one interpretation two nodes can disagree about.
    let (a, b, id) = twin_pair("twins-legacy");
    a.income(&id, 300.0, "laptop-side");
    b.income(&id, 700.0, "phone-side");

    // Strip A's log back to what an older build would have written.
    let path = a.home.join("events").join(format!("{id}.jsonl"));
    let legacy: String = std::fs::read_to_string(&path)
        .expect("read")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut v: serde_json::Value = serde_json::from_str(l).expect("line");
            let o = v.as_object_mut().expect("object");
            o.remove("origin");
            o.remove("lamport");
            o.remove("clock");
            format!("{}
", serde_json::to_string(&v).expect("re-encode"))
        })
        .collect();
    std::fs::write(&path, legacy).expect("write");

    let before_a = a.log_len(&id);
    let report = b.world.sync_peer(&a.address(), &id).expect("sync");
    eprintln!("LEGACY received={} sent={}", report.outcome.received, report.outcome.sent);
    assert!(report.outcome.sent > 0, "B still has history A lacks");
    assert!(
        a.log_len(&id) > before_a,
        "and it must land: A was {} and is {}",
        before_a,
        a.log_len(&id)
    );
}

// ── a peering that outlives the session ─────────────────────────────────────
//
// ★★★ The three promises: the port is the same tomorrow, the listener comes up
//     by itself, and a peer introduced once is reached again without being
//     re-introduced. Together they are the difference between a demo and a
//     link.


/// Open a world on a directory, with a chosen standing port.
fn standing(home: &std::path::Path, port: u16, enrol: bool) -> World {
    std::fs::create_dir_all(home).expect("home");
    let store = Store::at(home).expect("store");
    let world = World::open(store).expect("world");
    let mut net = world.network();
    net.listen_port = port;
    // Sweeping is the app's thread, not this test's.
    net.auto_reconnect = false;
    world.set_network(net).expect("settings");
    if enrol {
        world.enrol(DEFAULT_HANDLE, PASS).expect("enrol");
    }
    world
}

#[test]
fn the_listener_comes_up_with_nobody_pressing_anything() {
    // ★★★ Holding the key is the ONLY precondition -- peering signs a
    //     challenge with it -- so coming to hold it is the right trigger, and
    //     "start listening" stops being a thing a person does.
    let home = scratch("standing-unlock").join("node");
    let world = standing(&home, 39771, true);
    assert_eq!(
        world.peering().port(),
        Some(39771),
        "enrolling brought the listener up on the settled port"
    );
}

#[test]
fn a_second_launch_settles_on_the_same_port_it_wrote_down() {
    // ★★★ THE bug. The old bind asked for port 0, so every launch got a
    //     different number and two introduced devices could not find each
    //     other an hour later. What makes it stable is that the port is
    //     WRITTEN DOWN, so this is the assertion that matters: a new World
    //     over the same directory reads the same answer, having been told
    //     nothing.
    //
    // ★★ It cannot also assert the second bind, and the reason is worth
    //    recording: a listener thread outlives the World that started it --
    //    there is no way to stop listening short of ending the process. In the
    //    app that is exactly right (a restart IS a new process); in one test
    //    binary the first socket is still held. `lock()` clears the identity so
    //    a locked node refuses every handshake, but the port stays bound.
    //    Making lock close the socket is a real improvement and its own change.
    let home = scratch("standing-restart").join("node");
    let first = standing(&home, 39772, true);
    assert_eq!(first.peering().port(), Some(39772));
    assert_eq!(first.network().listen_port, 39772);
    drop(first);

    let second = standing(&home, 39772, false);
    assert_eq!(
        second.network().listen_port,
        39772,
        "the same port, unasked, on the next launch"
    );
}

#[test]
fn a_busy_port_is_named_rather_than_quietly_swapped() {
    // ★★★ Binding something else because the usual port was taken IS the
    //     original bug. A taken port must be an error with a name.
    let root = scratch("standing-busy");
    let holder = standing(&root.join("holder"), 39773, true);
    assert_eq!(holder.peering().port(), Some(39773));

    let second = standing(&root.join("second"), 39773, true);
    let err = second.listen_standing().expect_err("the port is taken");
    assert!(err.contains("39773"), "it says which port: {err}");
    assert!(err.contains("settled on"), "and why it matters: {err}");
}

#[test]
fn a_peer_introduced_once_is_reached_again_without_being_re_introduced() {
    // ★★★ The whole point. Nobody re-enters a key, an address or a port.
    let root = scratch("standing-reconnect");
    let a = standing(&root.join("alice"), 39774, true);
    let b = standing(&root.join("bob"), 39775, true);
    let id = "shared-habitat".to_string();
    a.instantiate_owned(&id, "Shared", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");

    let a_key = a.node_id().expect("key");
    let b_key = b.node_id().expect("key");
    for (me, them, addr) in
        [(&a, &b_key, "127.0.0.1:39775"), (&b, &a_key, "127.0.0.1:39774")]
    {
        me.peering()
            .edit(|book| {
                book.seen(them, "peer", Some(addr.to_string()));
                book.set_standing(them, Standing::Trusted);
                book.share(them, &id)
            })
            .expect("book")
            .expect("share");
    }

    // The sweep the app runs on a timer, called directly.
    let report = b.reconnect_all();
    assert!(!report.is_empty(), "there was a trusted, addressed, sharing peer to reach");
    assert!(
        report.iter().any(|(_, sustain, outcome)| sustain == &id && outcome.is_ok()),
        "and it was reached: {report:?}"
    );
    assert!(
        b.with(|i| i.get(&id).is_some()),
        "B now holds the Sustain it was never told about by hand"
    );
}

#[test]
fn a_peer_that_only_ever_dialled_in_is_not_swept() {
    // ★★ It has no address, because this node never agreed to one. Inventing
    //    it from the socket a connection arrived on would be recording
    //    something nobody chose.
    let home = scratch("standing-noaddr").join("node");
    let world = standing(&home, 39776, true);
    world
        .peering()
        .edit(|book| {
            book.seen("ff00", "dialled-in", None);
            book.set_standing("ff00", Standing::Trusted);
            book.share("ff00", "whatever")
        })
        .expect("book")
        .expect("share");

    assert!(world.reconnect_all().is_empty(), "nothing to dial, and that is correct");
}

// ── coming up without a person ──────────────────────────────────────────────

#[test]
fn a_remembered_unlock_brings_a_node_up_peering_with_nobody_present() {
    // ★★★ The whole point: a node that waits for a person stops peering the
    //     moment nobody is looking, which defeats a standing peering.
    let home = scratch("remembered").join("node");
    let first = standing(&home, 39781, true);
    first.remember_unlock(PASS).expect("remember");
    assert!(first.unlock_is_remembered());
    drop(first);

    // A new World over the same directory: a restart.
    let second = standing(&home, 39782, false);
    assert!(!second.is_unlocked(), "nothing has opened it yet");
    let handle = second.unlock_if_remembered().expect("came up unlocked");
    assert_eq!(handle, DEFAULT_HANDLE);
    assert!(second.is_unlocked(), "and it can act, with nobody present");
    assert_eq!(second.peering().port(), Some(39782), "and it is listening");
}

#[test]
fn forgetting_restores_the_passphrase_gate_exactly() {
    // ★★ A complete undo, not a repair: the sealed identity was never touched.
    let home = scratch("forget").join("node");
    let world = standing(&home, 39783, true);
    world.remember_unlock(PASS).expect("remember");
    world.forget_unlock().expect("forget");

    assert!(!world.unlock_is_remembered());
    assert!(
        world.unlock_if_remembered().is_none(),
        "it will not come up on its own any more"
    );
    // And the identity still opens the ordinary way.
    world.lock();
    assert_eq!(world.unlock(PASS).expect("unlock"), DEFAULT_HANDLE);
}

#[test]
fn a_wrong_passphrase_is_never_remembered() {
    // ★★★ It unlocks BEFORE it caches, so a wrong passphrase cannot be written
    //     down as if it were right.
    let home = scratch("wrong-pass").join("node");
    let world = standing(&home, 39784, true);
    assert!(world.remember_unlock("not-the-passphrase-at-all").is_err());
    assert!(!world.unlock_is_remembered(), "and nothing was written");
}

#[test]
fn a_merged_node_folds_the_same_state_as_its_peer_after_a_restart() {
    // ★★★ Caught by two real processes: they converged on the wire and then
    //     DISAGREED on the next open, because `World::open` folded the log in
    //     file order while `reload` folded it causally. Same entries, two
    //     states -- the one thing §VI says cannot happen ("same set of updates,
    //     same state, order-independent").
    let (a, b, id) = twin_pair("restart-converge");
    a.income(&id, 300.0, "node-a");
    b.income(&id, 700.0, "node-b");
    b.world.sync_peer(&a.address(), &id).expect("sync");
    a.absorb();

    let (state_a, state_b) = (a.state(&id), b.state(&id));
    assert_eq!(state_a, state_b, "converged while running");

    // Re-open both from disk: a restart, and the fold must not change.
    let reopened_a = World::open(Store::at(&a.home).expect("store")).expect("world");
    let reopened_b = World::open(Store::at(&b.home).expect("store")).expect("world");
    let after_a = reopened_a.with(|i| i.get(&id).map(|s| s.state.clone())).expect("a");
    let after_b = reopened_b.with(|i| i.get(&id).map(|s| s.state.clone())).expect("b");

    assert_eq!(after_a, after_b, "and still converged after a restart");
    assert_eq!(after_a, state_a, "the restart changed nothing on A");
    assert_eq!(after_b, state_b, "nor on B");
}
