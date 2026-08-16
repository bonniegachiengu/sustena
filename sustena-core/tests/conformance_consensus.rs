//! Consensus without a coordinator, replayed against its conformance vectors
//! (R2 · Multiparty §V · MUL-7, MUL-8, MUL-9).
//!
//! **SPEC vectors, Rust-only.** `paxos`, `acceptor`, `overlapping`,
//! `byzantine` and `FLP` are grep-0 in the reference.
//!
//! **The counterweight is the deliberation layer, and canon names it.**
//! `CouncilSession` is real and running — and §V itself says *"consensus is the
//! machinery; the session is what runs on it."* Its `resolve()` tallies
//! operative votes and then defers to the human. It is not a weak protocol; it
//! is a correct implementation of a different layer, one canon places *above*
//! this one.
//!
//! The proof of value: two proposers race, overlapping quorums force a single
//! chosen value, and the later round re-chooses it.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    propose, Accepted, AuthorityError, Body, ByzantineBound, ConsensusError, CouncilMint, Ledger,
    Promise, ProposalNumber, ProposalStatus, RoundOutcome, RoundPhase, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("consensus.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "consensus.json was written for a different contract version"
    );
    doc
}

// ── fixture ─────────────────────────────────────────────────────────────────

fn node_list(doc: &Value, key: &str) -> Vec<String> {
    doc["fixture"][key]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn body(doc: &Value) -> Body {
    Body::new(
        node_list(doc, "nodes"),
        doc["fixture"]["quorum_size"].as_u64().unwrap() as usize,
    )
    .expect("the declared body overlaps")
}

fn quorum(owned: &[String]) -> Vec<&str> {
    owned.iter().map(String::as_str).collect()
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from consensus.json"))
}

// ── ★ MUL-8 — safety is a property of the BODY ──────────────────────────────

#[test]
fn a_quorum_that_need_not_overlap_is_refused() {
    let doc = load();
    let c = case(&doc, "safety_cases", "a_quorum_that_need_not_overlap_is_refused");

    let err = Body::new(node_list(&doc, "nodes"), c["quorum_size"].as_u64().unwrap() as usize)
        .unwrap_err();

    assert!(matches!(err, ConsensusError::QuorumsNeedNotOverlap { .. }), "got {err:?}");
    assert!(
        err.to_string().contains(c["expect"]["message_contains"].as_str().unwrap()),
        "★★ the configuration that breaks safety cannot be constructed: {err}"
    );
}

#[test]
fn a_majority_is_the_smallest_quorum_that_overlaps() {
    let doc = load();
    let c = case(&doc, "safety_cases", "a_majority_is_the_smallest_quorum_that_overlaps");
    let b = Body::majority(node_list(&doc, "nodes")).unwrap();

    assert_eq!(b.quorum_size(), c["expect"]["quorum_size"].as_u64().unwrap() as usize);
    assert_eq!(2 * b.quorum_size() > b.size(), c["expect"]["overlaps"].as_bool().unwrap());
}

#[test]
fn an_empty_body_cannot_agree_on_anything() {
    let doc = load();
    let _ = case(&doc, "safety_cases", "an_empty_body_cannot_agree_on_anything");
    assert!(matches!(Body::new(vec![], 1), Err(ConsensusError::EmptyBody)));
    assert!(matches!(Body::majority(vec![]), Err(ConsensusError::EmptyBody)));
}

// ── §V's protocol ───────────────────────────────────────────────────────────

#[test]
fn a_value_is_chosen_when_a_quorum_accepts_it() {
    let doc = load();
    let c = case(&doc, "protocol_cases", "a_value_is_chosen_when_a_quorum_accepts_it");
    let mut l = Ledger::new(body(&doc));

    let q: Vec<String> =
        c["quorum"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let value = json!(c["value"].as_str().unwrap());
    let r = propose(&mut l, ProposalNumber::new(1, "p1"), value.clone(), &quorum(&q)).unwrap();

    assert_eq!(r.outcome, RoundOutcome::Chosen(value.clone()));
    assert_eq!(r.adopted_another_value(), c["expect"]["adopted_another_value"].as_bool().unwrap());

    let d = l.decision().expect("a quorum accepted");
    assert_eq!(d.value(), &value);
    assert_eq!(d.accepted_by(), c["expect"]["accepted_by"].as_u64().unwrap() as usize);
    assert_eq!(d.quorum(), &q.iter().cloned().collect::<BTreeSet<String>>(), "the quorum, named");
}

#[test]
fn nothing_is_chosen_before_a_quorum_accepts() {
    let doc = load();
    let _ = case(&doc, "protocol_cases", "nothing_is_chosen_before_a_quorum_accepts");
    let mut l = Ledger::new(body(&doc));
    let n = ProposalNumber::new(1, "p1");

    l.prepare(&n, &["a", "b", "c"]).unwrap();
    assert!(l.decision().is_none(), "promises are not acceptances");

    l.accept(&n, &json!("solar"), &["a", "b"]).unwrap();
    assert!(l.decision().is_none(), "two of three is not a quorum");
}

// ── ★★ the proof of value ───────────────────────────────────────────────────

#[test]
fn a_later_round_re_chooses_the_same_value() {
    let doc = load();
    let c = case(&doc, "protocol_cases", "a_later_round_re_chooses_the_same_value");
    let mut l = Ledger::new(body(&doc));

    let get = |k: &str| -> (Value, Vec<String>, u64) {
        let s = &c[k];
        (
            json!(s["value"].as_str().unwrap()),
            s["quorum"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            s["round"].as_u64().unwrap(),
        )
    };
    let (v1, q1, r1) = get("first");
    let (v2, q2, r2) = get("second");

    propose(&mut l, ProposalNumber::new(r1, "p1"), v1, &quorum(&q1)).unwrap();
    let second = propose(&mut l, ProposalNumber::new(r2, "p2"), v2, &quorum(&q2)).unwrap();

    let e = &c["expect"];
    assert_eq!(second.own_value, json!(e["second_own_value"].as_str().unwrap()), "it wanted this");
    assert_eq!(
        second.outcome,
        RoundOutcome::Chosen(json!(e["second_chose"].as_str().unwrap())),
        "★★ and re-chose the value already chosen — two proposers, one value"
    );
    assert_eq!(
        second.adopted_another_value(),
        e["adopted_another_value"].as_bool().unwrap()
    );
    assert_eq!(
        l.chosen(),
        Some(json!(e["still_chosen"].as_str().unwrap())),
        "a chosen value cannot be un-chosen"
    );
}

#[test]
fn the_overlap_is_what_forces_it_and_the_quorums_share_exactly_one_node() {
    let doc = load();
    let c = case(&doc, "protocol_cases", "the_overlap_is_what_forces_it_and_the_quorums_share_exactly_one_node");
    let q1: BTreeSet<String> = node_list(&doc, "first_quorum").into_iter().collect();
    let q2: BTreeSet<String> = node_list(&doc, "second_quorum").into_iter().collect();
    let e = &c["expect"];

    let shared: Vec<&String> = q1.intersection(&q2).collect();
    assert_eq!(
        shared.len(),
        e["shared_nodes"].as_u64().unwrap() as usize,
        "★ exactly one shared acceptor, and that is enough — safety is not statistical"
    );

    let mut l = Ledger::new(body(&doc));
    let q1v: Vec<String> = q1.iter().cloned().collect();
    propose(&mut l, ProposalNumber::new(1, "p1"), json!("solar"), &quorum(&q1v)).unwrap();

    let q2v: Vec<String> = q2.iter().cloned().collect();
    let promises = l.prepare(&ProposalNumber::new(2, "p2"), &quorum(&q2v)).unwrap();
    let carrying: Vec<&(String, Promise)> = promises
        .iter()
        .filter(|(_, p)| matches!(p, Promise::Granted { accepted: Some(_) }))
        .collect();

    assert_eq!(carrying.len(), e["carrying_promises"].as_u64().unwrap() as usize);
    assert_eq!(carrying[0].0, e["carrier"].as_str().unwrap());
}

#[test]
fn re_choosing_is_produced_by_the_protocol_not_remembered_by_the_ledger() {
    let doc = load();
    let c = case(&doc, "protocol_cases", "re_choosing_is_produced_by_the_protocol_not_remembered_by_the_ledger");
    let mut l = Ledger::new(body(&doc));
    propose(&mut l, ProposalNumber::new(1, "p1"), json!("solar"), &["a", "b", "c"]).unwrap();

    assert_eq!(
        l.decision() == l.decision(),
        c["expect"]["recomputed_stable"].as_bool().unwrap(),
        "★ recomputed from acceptor state every time — no 'final' field to trust"
    );
}

// ── ★ MUL-7 — FLP ───────────────────────────────────────────────────────────

#[test]
fn a_round_can_fail_at_prepare_and_that_is_a_normal_outcome() {
    let doc = load();
    let c = case(&doc, "flp_cases", "a_round_can_fail_at_prepare_and_that_is_a_normal_outcome");
    let mut l = Ledger::new(body(&doc));

    let blocking = ProposalNumber::new(c["blocking_round"].as_u64().unwrap(), "p9");
    l.prepare(&blocking, &["a", "b", "c", "d", "e"]).unwrap();

    let q: Vec<String> =
        c["quorum"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    let r = propose(
        &mut l,
        ProposalNumber::new(c["attempt_round"].as_u64().unwrap(), "p2"),
        json!("diesel"),
        &quorum(&q),
    )
    .expect("★★ a normal outcome, not an Err a caller could `?` into a crash");

    let e = &c["expect"];
    assert_eq!(r.succeeded(), e["succeeded"].as_bool().unwrap());
    match &r.outcome {
        RoundOutcome::Failed { at, reason } => {
            assert_eq!(*at, RoundPhase::Prepare);
            assert!(reason.contains(e["reason_contains"].as_str().unwrap()));
        }
        other => panic!("expected a failed round, got {other:?}"),
    }
    assert!(r.proposed.is_none(), "it never reached ACCEPT");
    assert!(l.decision().is_none(), "and nothing was chosen — safety held while liveness did not");
}

#[test]
fn a_round_can_fail_at_accept_when_a_higher_number_arrives_mid_round() {
    let doc = load();
    let c = case(&doc, "flp_cases", "a_round_can_fail_at_accept_when_a_higher_number_arrives_mid_round");
    let mut l = Ledger::new(body(&doc));

    let n = ProposalNumber::new(1, "p1");
    l.prepare(&n, &["a", "b", "c"]).unwrap();
    l.prepare(&ProposalNumber::new(5, "p5"), &["a", "b", "c"]).unwrap();

    let accepts = l.accept(&n, &json!("solar"), &["a", "b", "c"]).unwrap();
    assert_eq!(
        accepts.iter().all(|(_, a)| matches!(a, Accepted::Refused { .. })),
        c["expect"]["all_refused"].as_bool().unwrap()
    );
    assert!(l.decision().is_none());
}

#[test]
fn nothing_here_retries() {
    let doc = load();
    let c = case(&doc, "flp_cases", "nothing_here_retries");
    let mut l = Ledger::new(body(&doc));
    l.prepare(&ProposalNumber::new(9, "p9"), &["a", "b", "c"]).unwrap();

    let r = propose(&mut l, ProposalNumber::new(1, "p1"), json!("x"), &["a", "b", "c"]).unwrap();
    assert!(!r.succeeded());
    assert_eq!(
        r.number.round == 1,
        c["expect"]["round_number_unchanged"].as_bool().unwrap(),
        "★ buying around FLP needs a clock, and core has none — the host decides to retry"
    );
}

#[test]
fn the_protocol_refuses_what_it_cannot_honestly_run() {
    let doc = load();

    let c = case(&doc, "flp_cases", "a_round_sent_to_fewer_than_a_quorum_is_refused");
    let mut l = Ledger::new(body(&doc));
    let sent = c["sent_to"].as_u64().unwrap() as usize;
    let few: Vec<&str> = ["a", "b", "c"].into_iter().take(sent).collect();
    assert!(matches!(
        propose(&mut l, ProposalNumber::new(1, "p"), json!("x"), &few),
        Err(ConsensusError::QuorumTooSmall { .. })
    ));

    let _ = case(&doc, "flp_cases", "a_non_member_cannot_be_asked");
    assert!(matches!(
        l.prepare(&ProposalNumber::new(1, "p"), &["a", "outsider", "c"]),
        Err(ConsensusError::NotAMember(_))
    ));
}

// ── MUL-9 — the Byzantine bound ─────────────────────────────────────────────

#[test]
fn the_byzantine_bound_is_computable_and_this_build_does_not_meet_it() {
    let doc = load();
    let c = case(&doc, "byzantine_cases", "the_byzantine_bound_is_computable_and_this_build_does_not_meet_it");
    let e = &c["expect"];

    assert_eq!(
        ByzantineBound::THIS_BUILD_TOLERATES_BYZANTINE,
        e["tolerates_byzantine"].as_bool().unwrap(),
        "★ Paxos survives a node that STOPS, not one that LIES"
    );

    let b = body(&doc).byzantine();
    assert_eq!(
        b.traitors_a_bft_protocol_could_tolerate(),
        e["bft_would_tolerate"].as_u64().unwrap() as usize
    );
    assert_eq!(b.would_admit(1), e["would_admit_1"].as_bool().unwrap());
    assert_eq!(b.would_admit(2), e["would_admit_2"].as_bool().unwrap());
    assert_eq!(
        ByzantineBound::nodes_required_for(2),
        e["nodes_required_for_2"].as_u64().unwrap() as usize
    );
}

#[test]
fn a_household_sized_body_is_the_working_default() {
    let doc = load();
    let c = case(&doc, "byzantine_cases", "a_household_sized_body_is_the_working_default");
    let nodes: Vec<String> =
        c["nodes"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();

    let household = Body::majority(nodes).unwrap();
    assert_eq!(household.quorum_size(), c["expect"]["quorum_size"].as_u64().unwrap() as usize);
    assert_eq!(
        household.byzantine().traitors_a_bft_protocol_could_tolerate(),
        c["expect"]["bft_would_tolerate"].as_u64().unwrap() as usize
    );
}

// ── ★★ the loop closed ──────────────────────────────────────────────────────

#[test]
fn a_council_mint_derives_its_quorum_from_a_real_decision() {
    let doc = load();
    let c = case(&doc, "loop_closed_cases", "a_council_mint_derives_its_quorum_from_a_real_decision");
    let mut l = Ledger::new(body(&doc));

    propose(&mut l, ProposalNumber::new(1, "p1"), json!("raise-the-floor"), &["a", "b", "c"])
        .unwrap();
    let decision = l.decision().expect("a real quorum accepted");

    let id = c["expect"]["proposal_id"].as_str().unwrap();
    let mint = CouncilMint::from_consensus(id, ProposalStatus::Passed, &decision)
        .expect("★★ derived, not asserted");

    assert_eq!(mint.proposal_id(), id);
    assert_eq!(decision.accepted_by(), 3, "and the quorum it derived from is nameable");

    // The loop is recorded, not just coded.
    assert!(
        doc["divergence"]["★_the_loop_this_closes"]
            .as_str()
            .unwrap()
            .contains("DERIVED, not asserted")
    );
}

#[test]
fn a_mint_still_refuses_a_proposal_that_did_not_pass() {
    let doc = load();
    let _ = case(&doc, "loop_closed_cases", "a_mint_still_refuses_a_proposal_that_did_not_pass");
    let mut l = Ledger::new(body(&doc));
    propose(&mut l, ProposalNumber::new(1, "p1"), json!("x"), &["a", "b", "c"]).unwrap();
    let decision = l.decision().unwrap();

    // ★ The sharpest form: a real quorum accepted, and the human overrode it.
    // The machinery agreed; the layer above it said no, and that wins.
    assert!(
        matches!(
            CouncilMint::from_consensus("edit-1", ProposalStatus::OverriddenByUser, &decision),
            Err(AuthorityError::CouncilDidNotPass { .. })
        ),
        "★ 'Symbionts advise, the human decides' survives contact with a protocol          that could have ratified without them"
    );
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // ★ The counterweight: CouncilSession is a correct DIFFERENT layer.
    let cw = d["★_the_counterweight_is_the_deliberation_layer_and_canon_names_it"].as_str().unwrap();
    assert!(cw.contains("machinery"), "canon's own framing");
    assert!(cw.contains("the human decides"));
    assert!(
        cw.contains("not a weak consensus protocol"),
        "it is a different layer, not a worse version of this one"
    );

    // The false positives, named.
    let fp = &d["the_three_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["proposer (12 hits)"].as_str().unwrap().contains("parse_rule_proposer"));
    assert!(fp["majority (3 hits)"].as_str().unwrap().contains("Plain English"));

    // FLP is an impossibility result, not a gap.
    assert!(d["term_by_term"]["FLP (MUL-7)"].as_str().unwrap().contains("ENCODED CONSTRAINT"));
    assert!(
        d["the_honest_limit_stated_plainly"]
            .as_str()
            .unwrap()
            .contains("not a gap to be closed later")
    );

    // Byzantine is RECORDED, and 'rust-ahead' is the wrong word for it.
    assert!(
        d["term_by_term"]["the Byzantine bound (MUL-9)"]
            .as_str()
            .unwrap()
            .contains("RECORDED, NOT BUILT")
    );
    // And one term is AT PARITY.
    assert!(
        d["term_by_term"]["CouncilSession / StakeholderSession"]
            .as_str()
            .unwrap()
            .contains("AT PARITY")
    );

    // The loop-closer names the comment it falsifies.
    let loop_c = d["★_the_loop_this_closes"].as_str().unwrap();
    assert!(loop_c.contains("That comment is now false"));

    for group in
        ["safety_cases", "protocol_cases", "flp_cases", "byzantine_cases", "loop_closed_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in
        ["safety_cases", "protocol_cases", "flp_cases", "byzantine_cases", "loop_closed_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
