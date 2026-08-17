//! Vector clocks and the merge they sharpen, replayed against their
//! conformance vectors (R2 · Events & Time §VI–§VII · MUL-10, EVT-8).
//!
//! **SPEC vectors, Rust-only.** `vector_clock`, `happens_before`, `concurrent`,
//! `causal`, `lamport`, `lww` and `last_writer` are **all** grep-0 in the
//! reference — it has no causal machinery of any kind.
//!
//! `vclock.json` separates two things that would be easy to conflate: the
//! scalar→vector improvement is a **Rust-to-Rust sharpening** of an earlier R2
//! slice, not a gap in Python; what *is* a Python divergence is that there is
//! no causal clock there at all. Its counterweight explains why `seq` is a
//! correct design for one writer and stops being sufficient at two.
//!
//! The proof of value has two halves: the scalar clock reports a **false
//! concurrent** across a transitive chain, and two genuinely concurrent edits
//! are detected and **both survive**.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    assign_clocks, merge_stamped, CausalStamp, CausalVerdict, ClockError, Dimension, Event,
    MergeResolution, Provenance, Stamped, VectorClock, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("vclock.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "vclock.json was written for a different contract version"
    );
    doc
}

// ── fixtures, built from the vector file ────────────────────────────────────

fn events_of(doc: &Value, fixture: &str) -> Vec<Event> {
    doc[fixture]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| Event {
            id: e["id"].as_str().unwrap().to_string(),
            name: "e".into(),
            t_event: e["t_event"].as_i64().unwrap(),
            provenance: Provenance::Observed,
            t_ingest: None,
            source: None,
            stamp: CausalStamp {
                counter: e["counter"].as_u64().unwrap(),
                node: e["node"].as_str().unwrap().to_string(),
            },
            causes: e["causes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_string())
                .collect(),
            mutations: vec![],
        })
        .collect()
}

fn find<'a>(events: &'a [Event], id: &str) -> &'a Event {
    events.iter().find(|e| e.id == id).expect("declared event")
}

fn verdict_name(v: CausalVerdict) -> &'static str {
    match v {
        CausalVerdict::HappensBefore => "HappensBefore",
        CausalVerdict::HappenedAfter => "HappenedAfter",
        CausalVerdict::Concurrent => "Concurrent",
        CausalVerdict::Same => "Same",
    }
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from vclock.json"))
}

fn stamped(v: &str, clock: VectorClock, t: i64, id: &str) -> Stamped<String> {
    Stamped::new(v.to_string(), clock, t, id)
}

/// A shared root, then two independent local steps — a genuine fork.
fn forked() -> (VectorClock, VectorClock) {
    let root = VectorClock::new().tick("shared");
    (root.tick("phone"), root.tick("laptop"))
}

// ── §VI — the clock ─────────────────────────────────────────────────────────

#[test]
fn a_local_event_advances_only_its_own_component() {
    let doc = load();
    let c = case(&doc, "clock_cases", "a_local_event_advances_only_its_own_component");
    let v = VectorClock::new().tick("phone");

    assert_eq!(v.get("phone"), c["expect"]["phone"].as_u64().unwrap());
    assert_eq!(
        v.get("laptop"),
        c["expect"]["laptop"].as_u64().unwrap(),
        "★ absent means 0 — a node that has never acted has done nothing"
    );
}

#[test]
fn receiving_takes_the_componentwise_max_then_steps() {
    let doc = load();
    let c = case(&doc, "clock_cases", "receiving_takes_the_componentwise_max_then_steps");
    let phone = VectorClock::new().tick("phone").tick("phone");
    let after = VectorClock::new().tick("laptop").observe("laptop", &phone);

    assert_eq!(after.get("phone"), c["expect"]["phone"].as_u64().unwrap(), "learned the message");
    assert_eq!(after.get("laptop"), c["expect"]["laptop"].as_u64().unwrap(), "and took a step");
}

#[test]
fn clocks_over_different_node_sets_compare_correctly() {
    let doc = load();
    let c = case(&doc, "clock_cases", "clocks_over_different_node_sets_compare_correctly");
    let a = VectorClock::new().tick("phone");
    let b = a.tick("phone").tick("laptop");

    assert_eq!(a.happens_before(&b), c["expect"]["a_before_b"].as_bool().unwrap());
    assert_eq!(
        b.compare(&a) == CausalVerdict::HappenedAfter,
        c["expect"]["b_after_a"].as_bool().unwrap()
    );
}

#[test]
fn the_biconditional_holds_in_all_four_cases() {
    let doc = load();
    let c = case(&doc, "clock_cases", "the_biconditional_holds_in_all_four_cases");
    let base = VectorClock::new().tick("p");
    let later = base.tick("p");
    let other = base.tick("q");
    let e = &c["expect"];

    assert_eq!(verdict_name(base.compare(&base)), e["same"].as_str().unwrap());
    assert_eq!(verdict_name(base.compare(&later)), e["before"].as_str().unwrap());
    assert_eq!(verdict_name(later.compare(&base)), e["after"].as_str().unwrap());
    assert_eq!(
        verdict_name(later.compare(&other)),
        e["neither"].as_str().unwrap(),
        "★ neither dominates — a real answer the scalar clock cannot produce"
    );
}

#[test]
fn the_metadata_is_one_counter_per_node_that_has_acted() {
    let doc = load();
    let c = case(&doc, "clock_cases", "the_metadata_is_one_counter_per_node_that_has_acted");
    let v = VectorClock::new().tick("phone").tick("laptop").tick("phone");
    assert_eq!(v.size(), c["expect"]["size"].as_u64().unwrap() as usize);
}

// ── ★★ the false concurrent ─────────────────────────────────────────────────

#[test]
fn the_scalar_clock_reports_a_false_concurrent_across_a_transitive_chain() {
    let doc = load();
    let c = case(&doc, "false_concurrent_cases", "the_scalar_clock_reports_a_false_concurrent_across_a_transitive_chain");
    let events = events_of(&doc, "chain_fixture");
    let (a, cc) = (find(&events, "a"), find(&events, "c"));
    let e = &c["expect"];

    // Run against the REAL Event::happens_before, not a reconstruction of it.
    assert_eq!(
        a.happens_before(cc),
        e["scalar_says_happens_before"].as_bool().unwrap(),
        "the scalar clock cannot see through `b`"
    );
    assert_eq!(
        a.concurrent_with(cc),
        e["scalar_says_concurrent"].as_bool().unwrap(),
        "★★ so it calls two causally-ordered events concurrent"
    );
}

#[test]
fn the_vector_clock_sees_the_transitive_chain_correctly() {
    let doc = load();
    let c = case(&doc, "false_concurrent_cases", "the_vector_clock_sees_the_transitive_chain_correctly");
    let clocks = assign_clocks(&events_of(&doc, "chain_fixture")).unwrap();
    let e = &c["expect"];

    assert_eq!(
        verdict_name(clocks["a"].compare(&clocks["c"])),
        e["verdict"].as_str().unwrap(),
        "★★ a → c, which is the truth"
    );
    assert_eq!(
        clocks["a"].concurrent_with(&clocks["c"]),
        e["concurrent"].as_bool().unwrap()
    );
}

#[test]
fn genuinely_concurrent_events_are_still_reported_concurrent() {
    let doc = load();
    let c = case(&doc, "false_concurrent_cases", "genuinely_concurrent_events_are_still_reported_concurrent");
    let clocks = assign_clocks(&events_of(&doc, "fork_fixture")).unwrap();

    assert_eq!(
        verdict_name(clocks["x"].compare(&clocks["y"])),
        c["expect"]["verdict"].as_str().unwrap(),
        "★ the fix must not simply order everything"
    );
}

#[test]
fn assign_clocks_refuses_a_cycle_but_not_a_partial_log() {
    let doc = load();

    let c = case(&doc, "false_concurrent_cases", "a_causal_cycle_is_refused");
    let cyclic = vec![
        Event {
            id: "p".into(),
            name: "e".into(),
            t_event: 1,
            provenance: Provenance::Observed,
            t_ingest: None,
            source: None,
            stamp: CausalStamp { counter: 1, node: "n".into() },
            causes: vec!["q".into()],
            mutations: vec![],
        },
        Event {
            id: "q".into(),
            name: "e".into(),
            t_event: 2,
            provenance: Provenance::Observed,
            t_ingest: None,
            source: None,
            stamp: CausalStamp { counter: 2, node: "n".into() },
            causes: vec!["p".into()],
            mutations: vec![],
        },
    ];
    assert!(matches!(assign_clocks(&cyclic), Err(ClockError::CausalCycle(_))), "{}", c["name"]);

    let c = case(&doc, "false_concurrent_cases", "a_cause_outside_the_supplied_set_is_history_not_a_deadlock");
    let partial = vec![Event {
        id: "only".into(),
        name: "e".into(),
        t_event: 500,
        provenance: Provenance::Observed,
        t_ingest: None,
        source: None,
        stamp: CausalStamp { counter: 5, node: "n".into() },
        causes: vec!["something-older".into()],
        mutations: vec![],
    }];
    let clocks = assign_clocks(&partial).expect("★ partial replay must be possible");
    assert_eq!(clocks["only"].get("n"), c["expect"]["clock_for_only"].as_u64().unwrap());
}

// ── ★★ the merge, sharpened ─────────────────────────────────────────────────

#[test]
fn a_causally_ordered_pair_is_a_supersession_not_a_conflict() {
    let doc = load();
    let c = case(&doc, "merge_cases", "a_causally_ordered_pair_is_a_supersession_not_a_conflict");
    let base = VectorClock::new().tick("p");
    let later = base.tick("p");

    let r = merge_stamped(&stamped("old", base, 100, "e1"), &stamped("new", later, 200, "e2"));
    let e = &c["expect"];

    assert_eq!(r.is_concurrent(), e["concurrent"].as_bool().unwrap());
    assert_eq!(r.as_snapshot().value, e["snapshot_value"].as_str().unwrap());
    assert_eq!(
        r.as_contested().len(),
        e["contested_count"].as_u64().unwrap() as usize,
        "nothing was lost, so nothing survives twice"
    );
}

#[test]
fn two_concurrent_edits_are_detected_and_both_survive() {
    let doc = load();
    let c = case(&doc, "merge_cases", "two_concurrent_edits_are_detected_and_both_survive");
    let (mine, yours) = forked();
    let (lv, rv) = (c["left"].as_str().unwrap(), c["right"].as_str().unwrap());

    let r = merge_stamped(&stamped(lv, mine, 100, "e1"), &stamped(rv, yours, 200, "e2"));
    let e = &c["expect"];

    assert_eq!(
        r.is_concurrent(),
        e["concurrent"].as_bool().unwrap(),
        "★★ detectable only with a vector clock"
    );

    let resolved = r.resolve(Dimension::Contested);
    // ★ EVT-10 made this a typed outcome: a contested resolution can no longer
    // be mistaken for a snapshot one, which is the distinction §VII warns
    // about turning a correct theorem into a lost transaction.
    assert!(matches!(resolved, sustena_core::Resolved::NeedsReconciliation(_)));
    let kept = resolved.values();
    assert_eq!(
        kept.len(),
        e["contested_count"].as_u64().unwrap() as usize,
        "★★ §VII: LWW would silently discard one. Here neither is."
    );
    let values: Vec<&str> = kept.iter().map(|s| s.value.as_str()).collect();
    assert_eq!(
        values.contains(&lv) && values.contains(&rv),
        e["both_present"].as_bool().unwrap(),
        "never erase the node, applied to a concurrent edit"
    );
}

#[test]
fn the_same_pair_under_snapshot_semantics_collapses_and_that_is_correct() {
    let doc = load();
    let c = case(&doc, "merge_cases", "the_same_pair_under_snapshot_semantics_collapses_and_that_is_correct");
    let (earlier, later) = forked();

    let r = merge_stamped(
        &stamped(c["left"].as_str().unwrap(), earlier, 100, "e1"),
        &stamped(c["right"].as_str().unwrap(), later, 200, "e2"),
    );
    let e = &c["expect"];

    assert_eq!(r.is_concurrent(), e["concurrent"].as_bool().unwrap(), "concurrent either way");

    let resolved = r.resolve(Dimension::Snapshot);
    // ★ EVT-10: a snapshot resolution is now its own variant, so "the newer
    // reading supersedes" cannot be confused with "both were kept".
    assert!(matches!(resolved, sustena_core::Resolved::Superseded(_)));
    let kept = resolved.values();
    assert_eq!(kept.len(), e["snapshot_count"].as_u64().unwrap() as usize);
    assert_eq!(
        kept[0].value,
        e["snapshot_value"].as_str().unwrap(),
        "★ the newer READING supersedes by definition — same causality, correct opposite outcome"
    );
}

#[test]
fn the_contested_pair_surfaces_in_a_deterministic_order() {
    let doc = load();
    let c = case(&doc, "merge_cases", "the_contested_pair_surfaces_in_a_deterministic_order");
    let (a, b) = forked();
    let (x, y) = (stamped("A", a, 100, "e1"), stamped("B", b, 200, "e2"));

    assert_eq!(
        merge_stamped(&x, &y).as_contested() == merge_stamped(&y, &x).as_contested(),
        c["expect"]["order_independent"].as_bool().unwrap(),
        "★ 'both survive' must still converge — order of arrival cannot change the result"
    );
}

#[test]
fn re_delivering_the_same_stamp_is_idempotent() {
    let doc = load();
    let c = case(&doc, "merge_cases", "re_delivering_the_same_stamp_is_idempotent");
    let v = VectorClock::new().tick("p");

    let r = merge_stamped(&stamped("x", v.clone(), 100, "e1"), &stamped("x", v, 100, "e1"));
    assert!(matches!(r, MergeResolution::Same(_)), "{}", c["expect"]["resolution"]);
    assert_eq!(r.as_contested().len(), c["expect"]["count"].as_u64().unwrap() as usize);
}

#[test]
fn the_physical_stamp_is_never_used_to_decide_causality() {
    let doc = load();
    let c = case(&doc, "merge_cases", "the_physical_stamp_is_never_used_to_decide_causality");
    let (a, b) = forked();
    let gap = c["t_gap"].as_i64().unwrap();

    let r = merge_stamped(&stamped("A", a, 0, "e1"), &stamped("B", b, gap, "e2"));
    assert_eq!(
        r.is_concurrent(),
        c["expect"]["still_concurrent"].as_bool().unwrap(),
        "★ §VI: t_event is a physical stamp, not a causal clock — a huge gap is not evidence"
    );
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // ★ The two things it would be easy to conflate, kept apart.
    let internal = d["★_an_internal_sharpening_that_is_NOT_a_python_divergence"].as_str().unwrap();
    assert!(internal.contains("RUST-TO-RUST sharpening"));
    assert!(
        internal.contains("wrong to present it as a gap in the reference engine"),
        "the scalar clock was an earlier R2 slice, not a port"
    );

    // The defect is named precisely, with the shape that exposes it.
    let defect = d["★_the_defect_this_removes_named_precisely"].as_str().unwrap();
    assert!(defect.contains("FALSE CONCURRENT"));
    assert!(defect.contains("treat a supersession as a conflict"), "and why it matters");

    // The counterweight: `seq` is correct for one writer.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("MAX(seq) + 1"), "it names the real mechanism");
    assert!(cw.contains("BECAUSE there is a single writer"));
    assert!(
        cw.contains("not a shortcut"),
        "a correct design for its scale, not a deficiency"
    );

    // One term is AT PARITY, and one is at parity for its scale.
    assert!(d["term_by_term"]["t_event as a physical stamp"].as_str().unwrap().contains("AT PARITY"));
    assert!(
        d["term_by_term"]["the ordering that actually ships in Python"]
            .as_str()
            .unwrap()
            .contains("AT PARITY FOR ITS SCALE")
    );

    // Both honest limits recorded, including the one about causes.
    let lim = d["the_honest_limits"].as_str().unwrap();
    assert!(lim.contains("O(n) METADATA"));
    assert!(
        lim.contains("only as complete as the causes a producer actually recorded"),
        "the derivation's own limit"
    );

    for group in ["clock_cases", "false_concurrent_cases", "merge_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in ["clock_cases", "false_concurrent_cases", "merge_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
