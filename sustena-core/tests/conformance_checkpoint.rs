//! Replay and checkpointing, replayed against its conformance vectors
//! (R2 · Events and Time §IX · EVT-12).
//!
//! **SPEC vectors, Rust-only — with the homomorphism already in continuous use
//! on the other side.** The reference advances `sustain_states` one call at a
//! time rather than re-folding, which *is* `fold(s_k, L[k+1..n])` at `k = n−1`;
//! what it lacks is a checkpoint at any other `k`.
//!
//! ★★ Two structural properties here: a checkpoint is **derived, never
//! authored**, and replay **cannot reach an effect channel** — so §IX's
//! `no_external_effects_were_issued` is a fact about the signature rather than
//! an assertion at the end.
//!
//! ★★ And one finding: §IX's own pseudocode initialises `seen` per call, which
//! makes the checkpointed and from-scratch paths disagree on a late duplicate.
//! A case runs the pseudocode literally to show it.
//!
//! See `conformance/README.md`.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    events_applied, merge, replay, replay_to, CausalStamp, Checkpoint, Event, Mutation, Provenance,
    ReplayError, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("checkpoint.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "checkpoint.json was written for a different contract version"
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

// ── the fixture, from the vector ────────────────────────────────────────────

fn ev(doc: &Value, spec: &Value) -> Event {
    Event {
        id: spec["id"].as_str().unwrap().into(),
        name: "event.finances.pocket_spent".into(),
        t_event: spec["t_event"].as_i64().unwrap(),
        t_ingest: None,
        provenance: Provenance::Observed,
        source: None,
        stamp: CausalStamp::new("phone"),
        causes: vec![],
        mutations: vec![Mutation::Set {
            path: doc["fixture"]["path"].as_str().unwrap().into(),
            old: json!(spec["old"].as_i64().unwrap()),
            new: json!(spec["new"].as_i64().unwrap()),
        }],
        payload: None,
    }
}

fn log(doc: &Value) -> Vec<Event> {
    doc["fixture"]["log"].as_array().unwrap().iter().map(|s| ev(doc, s)).collect()
}

/// The log plus a repeat of an early id arriving last.
fn log_with_late_duplicate(doc: &Value) -> Vec<Event> {
    let mut l = log(doc);
    l.push(ev(doc, &doc["fixture"]["late_duplicate"]));
    l
}

fn balance(doc: &Value, v: &Value) -> i64 {
    let mut cur = v;
    for seg in doc["fixture"]["path"].as_str().unwrap().split('.') {
        cur = &cur[seg];
    }
    cur.as_i64().unwrap()
}

// ── the homomorphism ────────────────────────────────────────────────────────

#[test]
fn a_checkpoint_and_a_from_scratch_replay_agree_at_every_k() {
    // ★★ Checked at every k rather than one convenient one — a scheme that is
    // right at k=3 and wrong at k=1 is not a scheme.
    let doc = load();
    let c = case(
        &doc,
        "homomorphism_cases",
        "★★_a_checkpoint_and_a_from_scratch_replay_agree_at_EVERY_k",
    );
    let l = log(&doc);
    let scratch = replay(&l, None).unwrap();

    for k in 1..=l.len() {
        let cp = Checkpoint::at(&l, k).unwrap();
        assert_eq!(replay(&l, Some(&cp)).unwrap(), scratch, "disagreed at k={k}");
    }
    assert_eq!(balance(&doc, &scratch), c["expect"]["final_balance"].as_i64().unwrap());
    assert_eq!(balance(&doc, &scratch), doc["fixture"]["final_balance"].as_i64().unwrap());
}

#[test]
fn the_cost_is_n_minus_k_and_it_is_countable() {
    // ★ §IX's O(n−k) as a number in a test rather than a sentence in a comment.
    let doc = load();
    let c = case(&doc, "homomorphism_cases", "★_the_cost_is_O(n−k)_and_it_is_COUNTABLE");
    let l = log(&doc);
    let cp = Checkpoint::at(&l, 3).unwrap();

    assert_eq!(
        events_applied(l.len(), None) as u64,
        c["expect"]["from_scratch"].as_u64().unwrap()
    );
    assert_eq!(
        events_applied(l.len(), Some(&cp)) as u64,
        c["expect"]["from_checkpoint_at_3"].as_u64().unwrap()
    );
    assert_eq!(replay(&l, Some(&cp)).unwrap(), replay(&l, None).unwrap());
}

#[test]
fn a_checkpoint_stays_useful_as_the_log_grows() {
    // The case a head-only cache cannot serve.
    let doc = load();
    let c = case(&doc, "homomorphism_cases", "a_checkpoint_stays_useful_as_the_log_grows");
    let mut l = log(&doc);
    let cp = Checkpoint::at(&l, 2).unwrap();

    let grown = c["expect"]["balance_after_growth"].as_i64().unwrap();
    l.push(ev(&doc, &json!({"id": "e6", "t_event": 600, "old": 380, "new": grown})));

    assert_eq!(balance(&doc, &replay(&l, Some(&cp)).unwrap()), grown);
    assert_eq!(replay(&l, Some(&cp)).unwrap(), replay(&l, None).unwrap());
}

// ── ★★ always discardable ──────────────────────────────────────────────────

#[test]
fn discarding_the_checkpoint_is_changing_one_argument() {
    // ★★ The fast path and the from-scratch path are the SAME function, so
    // there is no separate slow implementation that could drift from it.
    let doc = load();
    let l = log(&doc);
    let cp = Checkpoint::at(&l, 4).unwrap();
    assert_eq!(replay(&l, Some(&cp)).unwrap(), replay(&l, None).unwrap());
}

#[test]
fn a_checkpoint_is_derived_and_cannot_be_authored() {
    // ★★ `Checkpoint::at` is the only constructor and it FOLDS THE LOG. There
    // is no constructor taking a state, so a checkpoint claiming a state the
    // log never produced is not writable.
    let doc = load();
    let c = case(
        &doc,
        "discardable_cases",
        "★★_a_checkpoint_is_DERIVED_and_cannot_be_AUTHORED",
    );
    let l = log(&doc);
    let cp = Checkpoint::at(&l, 3).unwrap();

    assert_eq!(balance(&doc, cp.state()), c["expect"]["state_at_k3"].as_i64().unwrap());
    assert_eq!(cp.k() as u64, c["expect"]["k"].as_u64().unwrap());
    assert_eq!(cp.through_event_id(), c["expect"]["through_event_id"].as_str().unwrap());
    assert!(cp.verify(&l).unwrap());
}

#[test]
fn a_checkpoint_from_a_different_log_is_refused_in_o1() {
    let doc = load();
    let c = case(
        &doc,
        "discardable_cases",
        "★_a_checkpoint_from_a_DIFFERENT_LOG_is_refused_in_O(1)",
    );
    let l = log(&doc);
    let cp = Checkpoint::at(&l, 3).unwrap();

    let mut other = log(&doc);
    other[2] = ev(&doc, &json!({"id": "different", "t_event": 300, "old": 250, "new": 220}));

    match replay(&other, Some(&cp)) {
        Err(ReplayError::CheckpointNotOfThisLog { expected, found, .. }) => {
            assert_eq!(expected, c["expect"]["expected"].as_str().unwrap());
            assert_eq!(found, c["expect"]["found"].as_str().unwrap());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn verify_catches_a_corrupted_state_that_the_cheap_check_cannot() {
    // ★ The honest split, stated rather than blurred: the O(1) check catches
    // the wrong log; only verify() catches a state tampered with afterwards,
    // and it costs exactly what the checkpoint saved.
    let doc = load();
    let c = case(
        &doc,
        "discardable_cases",
        "★_verify_catches_a_CORRUPTED_state_that_the_cheap_check_cannot",
    );
    let l = log(&doc);
    let cp = Checkpoint::at(&l, 3).unwrap();

    let mut tampered: Value = serde_json::to_value(&cp).unwrap();
    tampered["state"]["finances"]["liquid"]["balance"] = json!(9999);
    let cp: Checkpoint = serde_json::from_value(tampered).unwrap();

    assert_eq!(replay(&l, Some(&cp)).is_ok(), c["expect"]["cheap_check_passes"].as_bool().unwrap());
    assert_eq!(cp.verify(&l).unwrap(), !c["expect"]["verify_returns_false"].as_bool().unwrap());
}

#[test]
fn a_checkpoint_at_zero_is_no_checkpoint_and_says_so() {
    let doc = load();
    let l = log(&doc);
    assert_eq!(Checkpoint::at(&l, 0), Err(ReplayError::EmptyCheckpoint));
    assert!(matches!(
        Checkpoint::at(&l, l.len() + 4),
        Err(ReplayError::CheckpointBeyondLog { .. })
    ));
}

// ── ★★ the finding about §IX's own pseudocode ──────────────────────────────

/// §IX's pseudocode, written **literally** — `seen` initialised empty at the
/// start of the call, exactly as printed.
fn replay_as_the_pseudocode_is_written(log: &[Event], from: Option<&Checkpoint>) -> Value {
    use sustena_core::{apply_mutation, State};

    let (start_state, start) = match from {
        Some(cp) => (cp.state().clone(), cp.k()),
        None => (json!({}), 0),
    };
    let mut state = State::new(start_state);
    let mut seen: HashSet<&str> = HashSet::new();
    for e in &log[start..] {
        if !seen.insert(e.id.as_str()) {
            continue;
        }
        for m in &e.mutations {
            apply_mutation(&mut state, m).unwrap();
        }
    }
    state.snapshot()
}

#[test]
fn the_pseudocodes_per_call_seen_set_makes_the_two_paths_disagree() {
    // ★★ THE FINDING, demonstrated rather than argued. Not a defect in the
    // mathematics — a gap between the mathematics and one line of the sketch.
    let doc = load();
    let c = case(
        &doc,
        "dedupe_finding_cases",
        "★★_the_pseudocode's_PER_CALL_seen_set_makes_the_two_paths_DISAGREE",
    );
    let l = log_with_late_duplicate(&doc);
    let cp = Checkpoint::at(&l[..5], 3).unwrap();

    let scratch = replay_as_the_pseudocode_is_written(&l, None);
    let fast = replay_as_the_pseudocode_is_written(&l, Some(&cp));

    assert_eq!(
        balance(&doc, &scratch),
        c["expect"]["from_scratch"].as_i64().unwrap(),
        "from scratch the repeat is skipped — its id is already in `seen`"
    );
    assert_eq!(
        balance(&doc, &fast),
        c["expect"]["from_checkpoint"].as_i64().unwrap(),
        "from the checkpoint it is applied — the checkpoint's ids never entered `seen`"
    );
    assert_ne!(scratch, fast, "the two paths genuinely disagree");
}

#[test]
fn replay_refuses_a_log_that_was_not_deduped_at_the_substrate() {
    // ★ Refused by name rather than silently given one of the two wrong answers.
    let doc = load();
    let l = log_with_late_duplicate(&doc);
    let dup = doc["fixture"]["late_duplicate"]["id"].as_str().unwrap().to_string();

    assert_eq!(replay(&l, None), Err(ReplayError::DuplicateEventId(dup.clone())));
    assert_eq!(Checkpoint::at(&l, 3), Err(ReplayError::DuplicateEventId(dup)));
}

#[test]
fn once_merged_the_two_paths_agree_again() {
    // ★ The resolution is the substrate primitive the codebase already ships.
    let doc = load();
    let merged = merge(log_with_late_duplicate(&doc));
    let cp = Checkpoint::at(&merged, 3).unwrap();
    assert_eq!(replay(&merged, Some(&cp)).unwrap(), replay(&merged, None).unwrap());
}

// ── ★★ purity, and no effects in reach ─────────────────────────────────────

#[test]
fn replay_cannot_reach_an_effect_channel_at_all() {
    // ★★ A fact about the signature: `&[Event]` in, `Value` out. No sink, no
    // registry, no `execute`. An SMS cannot be re-sent because there is nothing
    // here to send it with.
    let doc = load();
    let l = log(&doc);
    let before = l.clone();
    let _ = replay(&l, None).unwrap();
    assert_eq!(l, before, "the log is borrowed, not owned — and comes back untouched");
}

#[test]
fn replay_is_deterministic_because_apply_reads_no_clock() {
    let doc = load();
    let l = log(&doc);
    assert_eq!(replay(&l, None).unwrap(), replay(&l, None).unwrap());
}

// ── §IX's two consumers ────────────────────────────────────────────────────

#[test]
fn replay_to_reads_the_state_at_a_chosen_point() {
    // ★ Tenet: "stand in a chosen future and read the path back."
    let doc = load();
    let c = case(&doc, "consumer_cases", "★_replay_to_reads_the_state_at_a_CHOSEN_POINT");
    let l = log(&doc);

    assert_eq!(
        balance(&doc, &replay_to(&l, None, 2).unwrap()),
        c["expect"]["at_2"].as_i64().unwrap()
    );
    assert_eq!(
        balance(&doc, &replay_to(&l, None, 4).unwrap()),
        c["expect"]["at_4"].as_i64().unwrap()
    );
    assert!(matches!(
        replay_to(&l, None, l.len() + 4),
        Err(ReplayError::UptoBeyondLog { .. })
    ));
}

#[test]
fn a_checkpoint_past_the_point_asked_for_falls_back_to_the_log() {
    // ★ The whole of §IX in one behaviour: the log is always sufficient, so a
    // checkpoint that is no use here is ignored rather than made an error.
    let doc = load();
    let c = case(
        &doc,
        "consumer_cases",
        "★_a_checkpoint_PAST_the_point_asked_for_falls_back_to_the_LOG",
    );
    let l = log(&doc);
    let cp = Checkpoint::at(&l, 4).unwrap();

    assert_eq!(replay_to(&l, Some(&cp), 2).unwrap(), replay_to(&l, None, 2).unwrap());
    assert_eq!(
        balance(&doc, &replay_to(&l, Some(&cp), 2).unwrap()),
        c["expect"]["at_2_with_late_checkpoint"].as_i64().unwrap()
    );
}

// ── the recorded divergence keeps its counterweight ────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"].as_str().unwrap(), "rust-ahead-of-python");

    // ★★ The counterweight: the reference already RUNS the homomorphism.
    let cw = d["★★_the_counterweight_the_reference_ALREADY_RUNS_the_homomorphism"]
        .as_str()
        .unwrap();
    assert!(cw.contains("sustain_states"));
    assert!(cw.contains("k = n−1"), "names exactly which k it is running at");

    // ★ And discardability is real AND tested there — credited, not grudged.
    let disc = d["★_and_the_discardability_property_is_REAL_there_and_TESTED"].as_str().unwrap();
    assert!(disc.contains("rebuild_state"));
    assert!(disc.contains("Credit where it is due"));

    // ★★ The sharp difference: derived versus authorable.
    let sharp = d["★★_where_the_two_genuinely_differ_derived_versus_AUTHORABLE"].as_str().unwrap();
    assert!(sharp.contains("_persist_state"));
    assert!(sharp.contains("not writable rather than documented as forbidden"));

    // ★★ The finding about the article's own pseudocode is recorded.
    let finding = doc["★★_a_finding_about_§IX's_own_pseudocode"].as_str().unwrap();
    assert!(finding.contains("seen"));
    assert!(finding.contains("demonstrated rather than argued"));

    // The false positives — `snapshot` is the one that would mislead.
    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.starts_with("snapshot")));
    assert!(fp.keys().any(|k| k.starts_with("incremental")));

    // EVT-14 is named as out of scope rather than quietly claimed.
    assert!(d["term_by_term"]["external effects journaled as their own events"]
        .as_str()
        .unwrap()
        .contains("EVT-14"));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "THE CHEAP CHECK CATCHES THE WRONG LOG",
        "NOTHING SCHEDULES CHECKPOINTS",
        "REPLAY REQUIRES A DEDUPED LOG",
        "REPLAY-UNDER-`D′` IS NOT BUILT",
        "NO EFFECT JOURNALING",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let groups = [
        "homomorphism_cases",
        "discardable_cases",
        "dedupe_finding_cases",
        "purity_cases",
        "consumer_cases",
    ];
    let mut seen = BTreeSet::new();
    for group in groups {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
