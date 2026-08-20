//! **1000, not 700** — the scenario slice 6 could only name.
//!
//! Slice 6 proved two nodes converge and proved, honestly, that convergence is
//! not preservation: two concurrent incomes of 300 and 700 landed on 700,
//! because an absolute `Set` lets the later one in fold order win, and no
//! merge can recover an intent that was overwritten. This file is the fix
//! working — the same two writes, agreed one at a time, both landing.
//!
//! Every test runs two real `World`s over two data directories, talking over
//! real sockets with real ed25519 handshakes, exactly as `peers_test` does.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::peers::Standing;
use crate::store::Store;
use crate::templates::TemplateId;
use crate::world::{World, DEFAULT_HANDLE};

const PASS: &str = "a-long-enough-passphrase";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mycelium-quorum-e2e-{name}"));
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

    fn balance(&self, id: &str) -> f64 {
        self.world
            .with(|i| i.get(id).map(|s| s.state.clone()))
            .and_then(|s| s.pointer("/finances/liquid/balance").and_then(Value::as_f64))
            .unwrap_or(f64::NAN)
    }

    fn state(&self, id: &str) -> Value {
        self.world.with(|i| i.get(id).map(|s| s.state.clone())).unwrap_or(Value::Null)
    }

    /// Try to record income. Returns whether it committed, and the reason if not.
    fn try_income(&self, id: &str, amount: f64, note: &str) -> (bool, String) {
        let mut params = Map::new();
        params.insert("amount".into(), json!(amount));
        params.insert("source".into(), json!(note));
        match self.world.call(id, "budget.record_income", &params).expect("call") {
            None => (false, "no such sustain".into()),
            Some((x, _)) => {
                let reason = x.result.reason.clone().unwrap_or_default();
                (x.committed(), reason)
            }
        }
    }
}

/// Two nodes, one Sustain, co-owned by both, and each trusting the other's key.
fn co_owned(name: &str) -> (Node, Node, String) {
    let root = scratch(name);
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");
    let id = "shared-purse".to_string();

    a.world
        .instantiate_owned(&id, "Shared Purse", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");

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

    // B takes the Sustain over the wire before either declares co-ownership —
    // you cannot co-own a household you do not have.
    b.world.sync_peer(&a.address(), &id).expect("seed");

    // ★★ Both nodes declare the same body. A body one side does not know about
    //    is a body that would refuse to vote.
    for (me, them) in [(&a, &b), (&b, &a)] {
        me.world.share_ownership(&id, &[them.key()]).expect("share ownership");
    }
    (a, b, id)
}

// ── the property ────────────────────────────────────────────────────────────

#[test]
fn two_concurrent_incomes_both_land_and_the_total_is_preserved() {
    // ★★★ **1000, not 700.** The exact scenario slice 6 named and could not
    //     fix there. Both nodes try to write at once; the body orders them;
    //     neither intent is lost.
    let (a, b, id) = co_owned("preserved");
    assert_eq!(a.balance(&id), 0.0);

    // Both propose for the same slot. One wins the round outright.
    let (a_ok, a_why) = a.try_income(&id, 300.0, "alice-side");
    let (b_ok, b_why) = b.try_income(&id, 700.0, "bob-side");

    // ★★ Exactly one landed — that is what agreeing on a slot MEANS.
    assert!(
        a_ok ^ b_ok,
        "exactly one write takes the slot (alice: {a_ok} {a_why} · bob: {b_ok} {b_why})",
    );

    // The loser was told which kind of refusal it was, and can retry.
    let (loser, winner, amount) = if a_ok { (&b, &a, 700.0) } else { (&a, &b, 300.0) };
    let why = if a_ok { b_why } else { a_why };
    assert!(why.contains("slot"), "the refusal names the slot it lost: {why}");

    // Retry: sync first, so the loser recomputes against what actually landed.
    loser.world.sync_peer(&winner.address(), &id).expect("sync");
    let (retried, retry_why) = loser.try_income(&id, amount, "retry");
    assert!(retried, "the retry lands on the next slot: {retry_why}");

    // ★★★ Both sides, both writes, and the sum of the two intents.
    winner.world.sync_peer(&loser.address(), &id).expect("sync back");
    loser.world.reload(&id).expect("reload");
    winner.world.reload(&id).expect("reload");

    assert_eq!(a.balance(&id), 1000.0, "300 + 700 — neither intent was superseded");
    assert_eq!(b.balance(&id), 1000.0);
    assert_eq!(a.state(&id), b.state(&id), "and both sides agree, byte for byte");
}

#[test]
fn the_merged_log_still_rebuilds_to_what_it_reports() {
    let (a, b, id) = co_owned("rebuild");
    let (a_ok, _) = a.try_income(&id, 300.0, "alice-side");
    let (winner, loser) = if a_ok { (&a, &b) } else { (&b, &a) };

    loser.world.sync_peer(&winner.address(), &id).expect("sync");
    loser.try_income(&id, 700.0, "loser-side");
    winner.world.sync_peer(&loser.address(), &id).expect("sync back");

    for node in [&a, &b] {
        let reported = node.world.reload(&id).expect("reload").state;
        let again = node.world.reload(&id).expect("rebuild").state;
        assert_eq!(reported, again, "rebuild == fold");
    }
}

#[test]
fn agreement_leaves_nothing_to_supersede() {
    // ★★★ The mechanism, not just the total: with a slot agreed per write
    //     there are no CONCURRENT entries to one path, so `reconcile` has
    //     nothing to report as overwritten. Slice 6's `superseded` list is the
    //     thing this increment exists to empty.
    let (a, b, id) = co_owned("no-supersede");
    let (a_ok, _) = a.try_income(&id, 300.0, "alice-side");
    let (winner, loser) = if a_ok { (&a, &b) } else { (&b, &a) };

    loser.world.sync_peer(&winner.address(), &id).expect("sync");
    assert!(loser.try_income(&id, 700.0, "loser-side").0);
    winner.world.sync_peer(&loser.address(), &id).expect("sync back");

    for node in [&a, &b] {
        let merged = node.world.reload(&id).expect("reload");
        assert!(
            merged.superseded.is_empty(),
            "nothing was overwritten: {}",
            merged.describe(),
        );
    }
}

// ── refusals ────────────────────────────────────────────────────────────────

#[test]
fn a_write_that_cannot_reach_quorum_is_refused_and_changes_nothing() {
    // ★★★ A minority cannot write. Not degraded, not queued, not applied
    //     locally "for now" — refused, with the state byte-unchanged.
    let (a, b, id) = co_owned("no-quorum");
    let _ = a.try_income(&id, 100.0, "before");
    let before = a.state(&id);

    // Bob goes away. Alice is now one vote of a two-node body.
    b.world.lock();
    drop(b);

    let (ok, why) = a.try_income(&id, 500.0, "while alone");
    assert!(!ok, "a minority does not get to write");
    assert!(why.contains("NOT applied"), "and it says so plainly: {why}");
    assert!(why.contains("co-owners"), "{why}");
    assert_eq!(a.state(&id), before, "state byte-unchanged");
}

#[test]
fn losing_a_round_and_failing_to_reach_one_are_different_refusals() {
    // ★★ The two outcomes a person must be able to tell apart: *your family is
    //    offline* versus *your move was ordered behind theirs*.
    let (a, b, id) = co_owned("classes");
    let (a_ok, a_why) = a.try_income(&id, 300.0, "alice");
    let (b_ok, b_why) = b.try_income(&id, 700.0, "bob");
    let lost_why = if a_ok { b_why } else { a_why };
    assert!(a_ok ^ b_ok);
    assert!(lost_why.contains("retry"), "a lost round is retryable: {lost_why}");
    assert!(!lost_why.contains("NOT applied"), "and is not a reachability problem");
}

#[test]
fn a_peer_that_does_not_co_own_it_has_no_vote() {
    // ★★★ Being an authenticated, trusted, shared-with peer is still not being
    //     a co-owner. Three separate grants, and only the third is a vote.
    let root = scratch("not-a-co-owner");
    let a = Node::start(&root, "alice");
    let b = Node::start(&root, "bob");
    let id = "purse".to_string();
    a.world
        .instantiate_owned(&id, "Purse", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");
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
    b.world.sync_peer(&a.address(), &id).expect("seed");

    // Only ALICE declares the body. Bob never does, so Bob will not vote.
    a.world.share_ownership(&id, &[b.key()]).expect("share ownership");

    let (ok, why) = a.try_income(&id, 100.0, "alice alone");
    assert!(!ok, "bob refuses to vote on a body he never joined");
    assert!(why.contains("NOT applied"), "{why}");
}

// ── the local case is untouched ─────────────────────────────────────────────

#[test]
fn a_single_owner_sustain_writes_with_no_quorum_at_all() {
    // ★★★ The whole point of making this opt-in. A household on one device
    //     pays nothing: no round, no socket, no reachability requirement — and
    //     no acceptor is ever consulted, which is asserted rather than assumed.
    let root = scratch("local");
    let a = Node::start(&root, "alice");
    let id = "mine".to_string();
    a.world
        .instantiate_owned(&id, "Mine", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");
    assert!(a.world.owners_of(&id).is_empty(), "single-owner by default");

    let (ok, why) = a.try_income(&id, 250.0, "local");
    assert!(ok, "{why}");
    assert_eq!(a.balance(&id), 250.0);
    assert!(
        a.world.peering().acceptors().is_empty(),
        "no slot was ever proposed, so no acceptor state exists",
    );
}

#[test]
fn declaring_a_body_always_includes_this_node() {
    // ★ A body you are not in is one you could never write to.
    let root = scratch("self-included");
    let a = Node::start(&root, "alice");
    let id = "mine".to_string();
    a.world
        .instantiate_owned(&id, "Mine", TemplateId::Habitat, None, None, Some(DEFAULT_HANDLE))
        .expect("instantiate");
    let owners = a.world.share_ownership(&id, &["ff".repeat(32)]).expect("share");
    assert!(owners.contains(&a.key()), "the declaring node is in its own body");
    assert_eq!(owners.len(), 2);
}

#[test]
fn agreement_composes_with_the_gate_rather_than_replacing_it() {
    // ★★★ Winning the round is permission to TRY, never permission to land.
    //     The invariant gate still runs on the agreed write, and still refuses.
    let (a, _b, id) = co_owned("gate-still-runs");

    let mut params = Map::new();
    params.insert("pocket_name".into(), json!("food"));
    params.insert("amount".into(), json!(9_999_999.0));
    let (x, _) = a.world.call(&id, "budget.allocate", &params).expect("call").expect("sustain");
    assert!(!x.committed());
    let reason = x.result.reason.clone().unwrap_or_default();
    // The operator's own guard, not a consensus refusal.
    assert!(
        !reason.contains("co-owners") && !reason.contains("slot"),
        "refused by the gate, having already won the round: {reason}",
    );
}
