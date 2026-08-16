//! The wider CRDT family, replayed against its conformance vectors
//! (R2 · Multiparty §VI · MUL-10).
//!
//! **SPEC vectors, Rust-only** — with one term at parity in *shape*. §VI says
//! the G-counter *"is exactly a shared-pocket roll-up"*, and the reference
//! roll-up genuinely is one: `included: [{member, value}]` per aggregate, each
//! member's own entry, the total a fold over them, no member's number ever
//! written by the group. What is absent is the **merge** — and the reason is
//! architectural rather than an oversight: `get_shared_engine()` is a
//! single-process singleton over one SQLite file, so there have never been two
//! divergent replicas to reconcile. The reference has the G-counter's shape
//! and does not yet need its algebra.
//!
//! The proof of value: every permutation of a delivery set, folded, and folded
//! again with every update duplicated — one state out of all of it — plus a
//! merge that provably cannot lower anybody's entry.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    converges, laws_hold, merge_stamped_crdt, CausalVerdict, CrdtError, ElementId, GCounter,
    JoinSemilattice, OrSet, PnCounter, Rga, Stamped, Tag, VectorClock, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("crdt.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "crdt.json was written for a different contract version"
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

fn counter(obj: &Value) -> GCounter {
    let pairs: Vec<(String, u64)> = obj
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap()))
        .collect();
    let refs: Vec<(&str, u64)> = pairs.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    GCounter::from_entries(&refs)
}

fn group(doc: &Value) -> GCounter {
    counter(&doc["fixture"]["group"])
}

// ── the laws ────────────────────────────────────────────────────────────────

/// ★ The laws are the property, so every member of the family is checked
/// against a real triple — not asserted in a doc comment beside each one.
#[test]
fn the_three_laws_hold_for_every_member_of_the_family() {
    let doc = load();
    let c = case(&doc, "law_cases", "the_three_laws_hold_for_every_member_of_the_family");
    let e = &c["expect"];

    let gc = laws_hold(
        &GCounter::from_entries(&[("a", 3)]),
        &GCounter::from_entries(&[("b", 5)]),
        &GCounter::from_entries(&[("a", 9), ("c", 1)]),
    );
    let pn = laws_hold(
        &PnCounter::new().increment("a", 5).unwrap(),
        &PnCounter::new().decrement("b", 2).unwrap(),
        &PnCounter::new().increment("c", 7).unwrap().decrement("c", 3).unwrap(),
    );
    let os_a = OrSet::new().add("rice", Tag::new("a", 1)).unwrap();
    let os = laws_hold(
        &os_a,
        &OrSet::new().add("beans", Tag::new("b", 1)).unwrap(),
        &os_a.remove("rice"),
    );
    let head = Rga::new().insert_after(ElementId::new("a", 1), "milk", None).unwrap();
    let rga = laws_hold(
        &head,
        &head
            .insert_after(ElementId::new("b", 2), "eggs", Some(&ElementId::new("a", 1)))
            .unwrap(),
        &head.remove(&ElementId::new("a", 1)),
    );

    // Every declared type is actually exercised here, not just listed.
    let declared: Vec<&str> = c["types"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(declared, vec!["GCounter", "PnCounter", "OrSet", "Rga"]);

    for (name, r) in [("GCounter", gc), ("PnCounter", pn), ("OrSet", os), ("Rga", rga)] {
        assert_eq!(r.commutative, e["commutative"].as_bool().unwrap(), "{name}: {}", r.describe());
        assert_eq!(r.associative, e["associative"].as_bool().unwrap(), "{name}: {}", r.describe());
        assert_eq!(r.idempotent, e["idempotent"].as_bool().unwrap(), "{name}: {}", r.describe());
        assert!(r.all_hold(), "{name}: {}", r.describe());
    }
}

#[test]
fn the_lattice_order_is_derived_from_the_join() {
    let doc = load();
    let c = case(&doc, "law_cases", "the_lattice_order_is_derived_from_the_join");
    let small = GCounter::from_entries(&[("a", 1)]);
    let big = GCounter::from_entries(&[("a", 2), ("b", 1)]);
    let e = &c["expect"];
    assert_eq!(small.leq(&big), e["small_leq_big"].as_bool().unwrap());
    assert_eq!(big.leq(&small), e["big_leq_small"].as_bool().unwrap());
    assert_eq!(small.leq(&small), e["reflexive"].as_bool().unwrap());
}

// ── ★★ order, duplication, retry ────────────────────────────────────────────

/// ★★ THE PROOF OF VALUE — the article's sentence executed.
#[test]
fn a_g_counter_converges_over_every_ordering_and_every_duplication() {
    let doc = load();
    let c = case(
        &doc,
        "convergence_cases",
        "★_a_g_counter_converges_over_every_ordering_and_every_duplication",
    );
    let updates: Vec<GCounter> = c["updates"].as_array().unwrap().iter().map(counter).collect();
    let r = converges(&updates).expect("within the limit");
    let e = &c["expect"];
    assert_eq!(r.orderings, e["orderings"].as_u64().unwrap() as usize);
    assert_eq!(r.order_independent, e["order_independent"].as_bool().unwrap());
    assert_eq!(r.duplication_free, e["duplication_free"].as_bool().unwrap());
    assert!(r.strongly_eventually_consistent());
}

#[test]
fn an_or_set_converges_over_every_ordering_and_every_duplication() {
    let doc = load();
    let c = case(
        &doc,
        "convergence_cases",
        "an_or_set_converges_over_every_ordering_and_every_duplication",
    );
    let added = OrSet::new().add("rice", Tag::new("a", 1)).unwrap();
    let removed = added.remove("rice");
    let readded = OrSet::new().add("rice", Tag::new("c", 1)).unwrap();
    let r = converges(&[added, removed, readded]).expect("within the limit");
    let e = &c["expect"];
    assert_eq!(r.orderings, e["orderings"].as_u64().unwrap() as usize);
    assert!(r.strongly_eventually_consistent());
}

#[test]
fn a_sequence_converges_over_every_ordering_and_every_duplication() {
    let doc = load();
    let _ = case(
        &doc,
        "convergence_cases",
        "a_sequence_converges_over_every_ordering_and_every_duplication",
    );
    let anchor = ElementId::new("a", 1);
    let head = Rga::new().insert_after(anchor.clone(), "milk", None).unwrap();
    let x = head.insert_after(ElementId::new("b", 2), "eggs", Some(&anchor)).unwrap();
    let y = head.insert_after(ElementId::new("c", 2), "bread", Some(&anchor)).unwrap();
    let r = converges(&[head, x, y]).expect("within the limit");
    assert!(r.strongly_eventually_consistent());
}

#[test]
fn convergence_refuses_to_sample_rather_than_prove() {
    let doc = load();
    let _ = case(&doc, "convergence_cases", "convergence_refuses_to_sample_rather_than_prove");
    let many: Vec<GCounter> = (0..8).map(|i| GCounter::from_entries(&[("n", i)])).collect();
    assert!(matches!(converges(&many), Err(CrdtError::TooManyToPermute { .. })));
}

// ── ★★ never erase the node ─────────────────────────────────────────────────

/// ★★ Not a policy on top of the merge — a theorem about `max`.
#[test]
fn a_merge_can_never_lower_anybody_s_entry() {
    let doc = load();
    let c = case(&doc, "never_erase_cases", "★_a_merge_can_never_lower_anybody_s_entry");
    let mine = group(&doc);
    let stale = counter(&doc["fixture"]["stale"]);

    for other in [
        stale.clone(),
        GCounter::new(),
        GCounter::from_entries(&[("ama", 0)]),
    ] {
        let merged = mine.join(&other);
        for node in mine.nodes() {
            assert!(
                merged.of(node) >= mine.of(node),
                "{node} was lowered by a merge"
            );
        }
    }
    assert_eq!(
        mine.join(&stale).of("ama"),
        c["expect"]["ama_stays"].as_u64().unwrap()
    );
}

#[test]
fn the_group_total_is_the_merge_and_the_entries_stay_the_members_own() {
    let doc = load();
    let c = case(
        &doc,
        "never_erase_cases",
        "the_group_total_is_the_merge_and_the_entries_stay_the_members_own",
    );
    let g = group(&doc);
    let e = &c["expect"];
    assert_eq!(g.value(), e["total_before"].as_u64().unwrap() as u128);
    // And the declared fixture total is the roll-up's own arithmetic.
    assert_eq!(g.value(), doc["fixture"]["total"].as_u64().unwrap() as u128);

    let merged = g.join(&counter(&doc["fixture"]["stale"]));
    assert_eq!(merged.value(), e["total_after"].as_u64().unwrap() as u128);
    assert_eq!(merged.of("ama"), e["ama_after"].as_u64().unwrap());
}

#[test]
fn a_pn_counter_carries_a_decrease_a_g_counter_would_discard() {
    let doc = load();
    let c = case(
        &doc,
        "never_erase_cases",
        "a_pn_counter_carries_a_decrease_a_g_counter_would_discard",
    );
    let up_only = GCounter::from_entries(&[("ama", 10)]).join(&GCounter::from_entries(&[("ama", 4)]));
    assert_eq!(up_only.of("ama"), c["expect"]["g_counter_keeps"].as_u64().unwrap());

    let pn = PnCounter::new().increment("ama", 10).unwrap().decrement("ama", 4).unwrap();
    assert_eq!(pn.value(), c["expect"]["pn_counter_value"].as_i64().unwrap() as i128);
    assert_eq!(pn.of("ama"), 6);
}

#[test]
fn counter_overflow_is_refused_not_wrapped() {
    let doc = load();
    let _ = case(&doc, "never_erase_cases", "counter_overflow_is_refused_not_wrapped");
    let c = GCounter::from_entries(&[("a", u64::MAX)]);
    assert!(matches!(c.increment("a", 1), Err(CrdtError::CounterOverflow(_))));
}

// ── the OR-set ──────────────────────────────────────────────────────────────

/// ★ Add wins — and not by a tie-break rule.
#[test]
fn a_concurrent_add_survives_a_remove_that_never_observed_it() {
    let doc = load();
    let c = case(
        &doc,
        "or_set_cases",
        "★_a_concurrent_add_survives_a_remove_that_never_observed_it",
    );
    let shared = OrSet::new().add("rice", Tag::new("ama", 1)).unwrap();
    let ama = shared.remove("rice");
    let ben = shared.add("rice", Tag::new("ben", 1)).unwrap();

    let e = &c["expect"];
    for merged in [ama.join(&ben), ben.join(&ama)] {
        assert_eq!(merged.contains("rice"), e["contains"].as_bool().unwrap());
        assert_eq!(merged.live_tags("rice").len(), e["live_tags"].as_u64().unwrap() as usize);
        assert_eq!(
            merged.live_tags("rice")[0].node,
            e["surviving_tag_node"].as_str().unwrap()
        );
    }
}

#[test]
fn a_remove_that_observed_every_tag_removes_the_element() {
    let doc = load();
    let c = case(&doc, "or_set_cases", "a_remove_that_observed_every_tag_removes_the_element");
    let s = OrSet::new()
        .add("rice", Tag::new("a", 1))
        .unwrap()
        .add("rice", Tag::new("b", 1))
        .unwrap();
    let gone = s.remove("rice");
    assert_eq!(gone.contains("rice"), c["expect"]["contains"].as_bool().unwrap());
    assert_eq!(
        gone.join(&gone) == gone,
        c["expect"]["duplicate_delivery_changes_nothing"].as_bool().unwrap()
    );
}

#[test]
fn a_reused_tag_is_refused() {
    let doc = load();
    let _ = case(&doc, "or_set_cases", "a_reused_tag_is_refused");
    let s = OrSet::new().add("rice", Tag::new("a", 1)).unwrap();
    assert!(matches!(
        s.add("beans", Tag::new("a", 1)),
        Err(CrdtError::TagReused { .. })
    ));
}

// ── the sequence ────────────────────────────────────────────────────────────

#[test]
fn concurrent_inserts_at_the_same_anchor_order_deterministically() {
    let doc = load();
    let c = case(
        &doc,
        "sequence_cases",
        "concurrent_inserts_at_the_same_anchor_order_deterministically",
    );
    let anchor = ElementId::new("a", 1);
    let head = Rga::new().insert_after(anchor.clone(), "milk", None).unwrap();
    let left = head.insert_after(ElementId::new("b", 2), "eggs", Some(&anchor)).unwrap();
    let right = head.insert_after(ElementId::new("c", 2), "bread", Some(&anchor)).unwrap();

    let want: Vec<&str> = c["expect"]["read"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(left.join(&right).read(), want);
    // Commutativity, as a user-visible fact rather than an algebraic one.
    assert_eq!(right.join(&left).read(), want);
}

#[test]
fn a_removed_element_stays_as_an_anchor() {
    let doc = load();
    let c = case(&doc, "sequence_cases", "a_removed_element_stays_as_an_anchor");
    let anchor = ElementId::new("a", 1);
    let head = Rga::new().insert_after(anchor.clone(), "milk", None).unwrap();
    let with_child = head
        .insert_after(ElementId::new("b", 2), "eggs", Some(&anchor))
        .unwrap();
    let removed = with_child.remove(&anchor);
    let want: Vec<&str> = c["expect"]["read"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(removed.read(), want, "the child is not orphaned");
    assert_eq!(removed.tombstones(), c["expect"]["tombstones"].as_u64().unwrap() as usize);
}

/// ★ Neither dropped nor guessed.
#[test]
fn an_element_whose_anchor_has_not_arrived_is_reported_not_guessed() {
    let doc = load();
    let c = case(
        &doc,
        "sequence_cases",
        "★_an_element_whose_anchor_has_not_arrived_is_reported_not_guessed",
    );
    let anchor = ElementId::new("a", 1);
    let child_id = ElementId::new("b", 2);
    let head = Rga::new().insert_after(anchor.clone(), "milk", None).unwrap();
    let full = head.insert_after(child_id.clone(), "eggs", Some(&anchor)).unwrap();

    // A partial delivery, which is exactly a delta that references an anchor
    // it does not carry: the child arrived, the head has not.
    let orphan_only = Rga::new().join(&full.delta(std::slice::from_ref(&child_id)));
    let e = &c["expect"];
    assert_eq!(orphan_only.read(), Vec::<&str>::new());
    assert_eq!(orphan_only.pending().len(), e["pending"].as_u64().unwrap() as usize);
    assert_eq!(orphan_only.pending()[0], &child_id);

    // And once the anchor arrives, the read is normal — nothing was lost.
    let healed = orphan_only.join(&full);
    let want: Vec<&str> = e["full_read"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(healed.read(), want);
    assert!(healed.pending().is_empty());
}

#[test]
fn inserting_after_something_never_seen_is_refused() {
    let doc = load();
    let _ = case(&doc, "sequence_cases", "inserting_after_something_never_seen_is_refused");
    let r = Rga::new();
    assert!(matches!(
        r.insert_after(ElementId::new("a", 1), "milk", Some(&ElementId::new("z", 9))),
        Err(CrdtError::UnknownAnchor { .. })
    ));
}

// ── the vclock composition point ────────────────────────────────────────────

/// ★★ The payoff the previous slice set up: a concurrent pair no longer needs
/// choosing between discarding one and handing back two.
#[test]
fn a_concurrent_pair_merges_rather_than_one_being_picked() {
    let doc = load();
    let c = case(
        &doc,
        "vclock_composition_cases",
        "★_a_concurrent_pair_merges_rather_than_one_being_picked",
    );
    let base = VectorClock::new();
    let a = Stamped::new(GCounter::from_entries(&[("ama", 5)]), base.tick("ama"), 100, "e-a");
    let b = Stamped::new(GCounter::from_entries(&[("ben", 7)]), base.tick("ben"), 200, "e-b");
    let (merged, verdict) = merge_stamped_crdt(&a, &b);

    let e = &c["expect"];
    assert_eq!(verdict, CausalVerdict::Concurrent);
    assert_eq!(e["verdict"].as_str().unwrap(), "Concurrent");
    assert_eq!(merged.value.of("ama"), e["ama"].as_u64().unwrap());
    assert_eq!(merged.value.of("ben"), e["ben"].as_u64().unwrap());
    assert_eq!(merged.value.value(), e["total"].as_u64().unwrap() as u128);
}

/// ★ Why the clock is no longer load-bearing for the answer here.
#[test]
fn a_causally_ordered_pair_merges_to_the_successor_which_is_what_lww_would_pick() {
    let doc = load();
    let c = case(
        &doc,
        "vclock_composition_cases",
        "★_a_causally_ordered_pair_merges_to_the_successor_which_is_what_lww_would_pick",
    );
    let c1 = VectorClock::new().tick("ama");
    let c2 = c1.tick("ama");
    let earlier = Stamped::new(GCounter::from_entries(&[("ama", 5)]), c1, 100, "e-1");
    let later = Stamped::new(GCounter::from_entries(&[("ama", 9)]), c2, 200, "e-2");

    let (merged, verdict) = merge_stamped_crdt(&earlier, &later);
    let e = &c["expect"];
    assert_eq!(verdict, CausalVerdict::HappensBefore);
    assert_eq!(e["verdict"].as_str().unwrap(), "HappensBefore");
    // a ⊔ successor(a) = successor(a) — exactly what LWW would pick.
    assert_eq!(merged.value == later.value, e["equals_successor"].as_bool().unwrap());
    assert_eq!(
        merge_stamped_crdt(&later, &earlier).0.value == later.value,
        e["order_irrelevant"].as_bool().unwrap()
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

    // ★ The gap is architectural, and said so.
    let why = d["★_and_the_reason_is_architectural_not_an_oversight"].as_str().unwrap();
    assert!(why.contains("single-process singleton"));
    assert!(why.contains("never been able to ask"));

    // ★ The G-counter term is at parity in SHAPE, and the distinction is kept.
    let gc = d["term_by_term"]["★ the G-counter — 'exactly a shared-pocket roll-up'"]
        .as_str()
        .unwrap();
    assert!(gc.contains("AT PARITY IN SHAPE"));
    assert!(gc.contains("does not yet need its algebra"));

    // ★ The subtle false positive is treated precisely, not dismissed.
    let fp = &d["the_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["rga (5 hits, 0 real)"].as_str().unwrap().contains("ba-RGA-ining"));
    let idem = fp["idempotent (20 hits) — ★ THE SUBTLE ONE, and it is worth being precise rather than dismissive"]
        .as_str()
        .unwrap();
    assert!(idem.contains("OPERATIONAL idempotence"));
    assert!(idem.contains("ALGEBRAIC idempotence"));
    assert!(idem.contains("reaching it the expensive way"));

    // The counterweight is generous and specific about what the reference has.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("hard-won"));
    assert!(cw.contains("not a criticism of a single-node engine"));

    // Three honest limits, all present.
    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in ["NO RNG IN THIS CRATE", "TOMBSTONES ARE NEVER COLLECTED", "REFUSES above its limit"] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = [
        "law_cases",
        "convergence_cases",
        "never_erase_cases",
        "or_set_cases",
        "sequence_cases",
        "vclock_composition_cases",
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
