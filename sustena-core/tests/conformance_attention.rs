//! The two attentions, replayed against their conformance vectors
//! (R2 · Operative §V · OPV-7).
//!
//! **Reference-less**: the shape exists in the reference only by convention
//! (`should_evaluate` cheap vs `evaluate` costly), undeclared. ★★ So the
//! counterweight is **internal consistency** — `d(s,V)` and the four-surface
//! salience family, the `⊕` composition tree, and UI-1's `K ≈ 4` hold.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    governance::Parameters,
    approval::{Binding, EffectClass, NonceLedger, Simulated},
    attention::{
        broad_scan, narrow_scan, scan, Aperture, Attended, Attention, AttentionBudget,
        AttentionError, Reframing, Stance,
    },
    council::ProposalStatus,
    governance::{
        declared_parameters, definition, enforcement, opening_state, register as register_gov,
        ATTENTION_KAPPA,
    },
    operator::{execute_admitted, Authorization, Registry},
    curated::DEFAULT_BUDGET,
    detect::CusumSpec,
    monitor::{MonitorEngine, SustainWatch},
    region::{Interval, Region},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("attention.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "attention.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn region() -> Region {
    Region::new().bounding(Interval::new("balance", 0.0, 100.0)).weighing("balance", 1.0)
}

fn spec() -> CusumSpec {
    CusumSpec::new(0.0, 1.0, 5.0)
}

/// A real `⊕` tree: root → mid → leaf, with siblings at each level.
fn tree() -> MonitorEngine {
    MonitorEngine::flatten_holarchy(vec![
        SustainWatch::new("root", region(), 0.3, spec()),
        SustainWatch::new("mid_a", region(), 0.3, spec()).under("root"),
        SustainWatch::new("mid_b", region(), 0.3, spec()).under("root"),
        SustainWatch::new("mid_c", region(), 0.3, spec()).under("root"),
        SustainWatch::new("leaf_a", region(), 0.3, spec()).under("mid_a"),
        SustainWatch::new("leaf_b", region(), 0.3, spec()).under("mid_a"),
    ])
    .unwrap()
}

fn narrow() -> Aperture {
    Aperture::declared(1, 3, 4).unwrap()
}

fn broad() -> Aperture {
    Aperture::declared(12, 1, 1).unwrap()
}

// ── both attentions ──────────────────────────────────────────────────────────

#[test]
fn an_attention_carries_both_and_there_is_no_way_to_declare_one() {
    // ★★★ Two required fields, no Option, no single-attention constructor.
    let a = Attention::declared(narrow(), broad(), 100.0, &Parameters::genesis()).unwrap();
    assert_eq!(a.stances(), [Stance::Narrow, Stance::Broad].into_iter().collect());
}

#[test]
fn a_pair_that_is_one_region_twice_is_refused() {
    // ★★★ The partition arriving through the back door.
    let n1 = Aperture::declared(1, 3, 4).unwrap();
    let n2 = Aperture::declared(2, 4, 3).unwrap();
    assert_eq!((n1.stance(), n2.stance()), (Stance::Narrow, Stance::Narrow));
    assert_eq!(
        Attention::declared(n1, n2, 1000.0, &Parameters::genesis()),
        Err(AttentionError::NotTwoRegions { both: Stance::Narrow })
    );
}

#[test]
fn a_zero_axis_is_not_an_aperture() {
    assert_eq!(Aperture::declared(0, 1, 1), Err(AttentionError::ZeroAxis { axis: "breadth" }));
    assert!(Aperture::declared(1, 0, 1).is_err());
    assert!(Aperture::declared(1, 1, 0).is_err());
}

#[test]
fn the_stance_is_read_off_the_axes_not_declared() {
    assert_eq!(narrow().stance(), Stance::Narrow);
    assert_eq!(broad().stance(), Stance::Broad);
}

// ── the closed trade ─────────────────────────────────────────────────────────

#[test]
fn deep_and_narrow_costs_the_same_as_broad_and_shallow() {
    assert_eq!(Aperture::declared(1, 4, 3).unwrap().cost_under(&Parameters::genesis()), 12.0);
    assert_eq!(Aperture::declared(12, 1, 1).unwrap().cost_under(&Parameters::genesis()), 12.0);
}

#[test]
fn raising_an_axis_is_paid_for_by_lowering_another() {
    let budget = 12.0;
    assert!(Aperture::declared(2, 2, 3).unwrap().affordable_under(budget, &Parameters::genesis()));
    assert!(!Aperture::declared(2, 4, 3).unwrap().affordable_under(budget, &Parameters::genesis()), "no free axis");
    assert!(Aperture::declared(2, 4, 1).unwrap().affordable_under(budget, &Parameters::genesis()), "resolution paid for depth");
}

#[test]
fn an_attention_over_budget_is_refused() {
    assert!(matches!(
        Attention::declared(narrow(), broad(), 10.0, &Parameters::genesis()),
        Err(AttentionError::OverBudget { .. })
    ));
}

// ── a path, not a subtree ────────────────────────────────────────────────────

#[test]
fn the_scan_is_a_path_with_fringes_and_visits_at_most_b_times_d() {
    let engine = tree();
    let a = Aperture::declared(3, 2, 1).unwrap();
    let s = scan(&engine, "root", &a);
    assert!(s.visited.len() <= a.path_visits(), "linear in d, not geometric");
    assert!(s.visited.contains(&"mid_a".to_string()));
    assert!(s.visited.contains(&"mid_c".to_string()), "the fringe was sampled");
    assert!(s.visited.contains(&"leaf_a".to_string()), "and one child was descended into");
    assert_eq!(s.reached_depth, 2);
}

#[test]
fn the_geometric_traversal_is_not_expressible() {
    // ★★★ `subtree_visits` measures the gap; nothing consumes it, and there is
    // no `walk_subtree` to call.
    let a = Aperture::declared(8, 6, 1).unwrap();
    assert_eq!(a.path_visits(), 48);
    assert!(a.subtree_visits() > 30_000, "which is why nobody may ask for it");
}

#[test]
fn a_scan_stops_at_a_nano_sustain_rather_than_pretending_to_go_deeper() {
    let engine = tree();
    let s = scan(&engine, "mid_b", &Aperture::declared(3, 5, 1).unwrap());
    assert!(s.visited.is_empty());
    assert_eq!(s.reached_depth, 0, "asked for five, found none — and says so");
}

// ── two output types ─────────────────────────────────────────────────────────

#[test]
fn narrow_returns_a_score_and_it_comes_from_d_s_v() {
    // ★★★ `Score` has no constructor; the value is `Region::distance`'s.
    let engine = tree();
    let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
    let scores = narrow_scan(&s, &region(), |id| match id {
        "mid_a" => Some(json!({"balance": 160.0})),
        "mid_b" => Some(json!({"balance": 50.0})),
        _ => None,
    });
    assert_eq!(scores[0].sustain_id(), "mid_a", "worst first, by d(s,V)");
    assert!(scores[0].value() > 0.0);
    assert_eq!(scores[1].value(), 0.0, "inside V");
}

#[test]
fn an_operative_cannot_mint_a_score() {
    let engine = tree();
    let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
    assert!(
        narrow_scan(&s, &region(), |_| None).is_empty(),
        "no state, no score — not a fabricated zero"
    );
}

#[test]
fn broad_returns_a_reframing_not_a_number() {
    let engine = tree();
    let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
    let task: BTreeSet<&str> = ["mid_a"].into_iter().collect();
    match broad_scan(&s, &region(), &task, |id| match id {
        "mid_b" => Some(json!({"balance": 900.0})),
        _ => Some(json!({"balance": 50.0})),
    }) {
        Reframing::FrameChanged { at, .. } => assert_eq!(at, "mid_b"),
        other => panic!("expected a reframing, got {other:?}"),
    }
}

#[test]
fn broad_ignores_what_the_task_was_already_looking_at() {
    // ★ Broad is what narrow is NOT covering.
    let engine = tree();
    let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
    let task: BTreeSet<&str> = ["mid_a", "mid_b", "mid_c"].into_iter().collect();
    let r = broad_scan(&s, &region(), &task, |_| Some(json!({"balance": 900.0})));
    assert_eq!(r, Reframing::FrameHolds);
    assert!(!r.triggered());
}

// ── the footprint and the hold ───────────────────────────────────────────────

#[test]
fn the_scan_footprint_can_be_wide_while_the_hold_stays_four() {
    // ★ "Anything can scan widely and hold four; nothing can hold widely."
    let engine = tree();
    let s = scan(&engine, "root", &Aperture::declared(3, 2, 1).unwrap());
    assert!(s.footprint().len() > 2);
    assert_eq!(DEFAULT_BUDGET, 4, "the hold is UI-1's and the sweep did not change it");
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-spec");

    // ★★★ The STEP-0 constraint that replaced the missing parity half.
    let step0 = &doc["★★★_the_STEP_0_constraint_that_replaces_the_missing_counterweight"];
    for key in [
        "the_risk",
        "★★★_the_enforcement",
        "★_and_the_stance_is_read_not_declared",
        "the_three_internal_constraints_this_row_leaned_on",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    let enforcement = step0["★★★_the_enforcement"].as_str().unwrap();
    assert!(enforcement.contains("no public constructor"));
    assert!(enforcement.contains("fifth surface"), "the salience family tie must stay named");

    // ★★ The counterweight: the SHAPE is the reference's.
    let cw = d["★★_the_counterweight_the_SHAPE_is_the_reference's_and_it_is_honoured_there"]
        .as_str()
        .unwrap();
    assert!(cw.contains("should_evaluate"));
    assert!(cw.contains("did not discover the shape"), "credit must stay explicit");

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(
        fp.keys().any(|k| k.contains("attention (51 hits")),
        "the needs-attention false positive must stay named"
    );
    assert!(fp.keys().any(|k| k.contains("narrow / broad")));
    assert!(
        fp.keys().any(|k| k.contains("should_evaluate")),
        "the real one must stay named among the noisy zeros"
    );

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        // ★★ The two CLOSED limits must stay recorded AS closed — a stale
        // limit is worse than a missing one, and a silently dropped one is
        // worse than both.
        "`B_att` IS NOW A BALANCE",
        "ATTENTION IS NOW CALLED FROM AN OPERATIVE",
        // ★ And what stays true must stay named beside them.
        "still not denominated in juul",
        "declared and uncalibrated",
        "THE STANCE BOUNDARY IS A DECLARED HEURISTIC",
        "THE PATH DESCENDS INTO THE FIRST CHILD",
        "BROAD REPORTS THE FIRST FRAME CHANGE",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    // ★★★ The addendum must keep the reasons, not just the verdicts.
    let add = &doc["★★★_the_B_att_addendum_2026_08_18"];
    assert!(add["what_this_addendum_closes"].as_str().unwrap().contains("WAS A NUMBER"));
    assert!(add["★★★_the_identity_holds_BY_CONSTRUCTION_not_by_check"]
        .as_str()
        .unwrap()
        .contains("FOLD"));
    let clamp = add["★★★_REFUSED_NEVER_CLAMPED_and_the_reason_is_STRUCTURAL"].as_str().unwrap();
    assert!(clamp.contains("NOT on taste"), "the reason must stay, not just the policy");
    assert!(clamp.contains("UNSOUND HERE"));
    let kap = add["★★_kappa_att_is_now_GOVERNED_and_the_constant_was_DELETED"].as_str().unwrap();
    assert!(kap.contains("REMOVED OUTRIGHT"));
    assert!(kap.contains("NO PATH PRICES AN APERTURE FROM A CONSTANT"));
    assert!(add["★_what_is_governed_and_what_is_deliberately_NOT"]
        .as_str()
        .unwrap()
        .contains("BOUNDED, not POSITIVE"), "the kappa-of-zero limit must stay named");

    let mut seen = BTreeSet::new();
    for group in [
        "both_attentions_cases",
        "closed_trade_cases",
        "path_not_subtree_cases",
        "two_output_types_cases",
        "footprint_cases",
        "b_att_cases",
    ] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 23, "every declared case must be present");
}

// ── ★★★ B_att: a balance, not a number ───────────────────────────────────────

fn budget(n: f64) -> AttentionBudget {
    AttentionBudget::allocated(n).expect("a real allocation")
}

fn attention_pair() -> Attention {
    Attention::declared(narrow(), broad(), 100.0, &Parameters::genesis()).unwrap()
}

/// The identity, as an equation. ★ Checked at EVERY step, because an identity
/// that only holds at the end is a coincidence.
fn assert_identity(b: &AttentionBudget) {
    assert_eq!(
        b.spent() + b.remaining(),
        b.allocation(),
        "spent + remaining == allocated, by the shape of the type"
    );
}

#[test]
fn the_budget_identity_holds_as_an_equation_over_a_real_sequence() {
    let p = Parameters::genesis();
    let a = attention_pair();
    let per_turn = a.cost_under(&p);
    let mut b = budget(per_turn * 2.5);
    assert_identity(&b);

    // Two affordable turns.
    for turn in 1..=2 {
        assert!(b.spend(&a, &p).spent(), "turn {turn} is affordable");
        assert_identity(&b);
        assert_eq!(b.spends().len(), turn);
        assert_eq!(b.spent(), per_turn * turn as f64);
    }

    // A third does not fit in the half-turn that is left.
    let refused = b.spend(&a, &p);
    assert!(!refused.spent(), "{}", refused.describe());
    assert_identity(&b);
    assert_eq!(b.spends().len(), 2, "a refusal is not a spend");

    // ★ And renewal is the HOST saying a period turned — same allocation, no spends.
    let fresh = b.renew();
    assert_eq!(fresh.allocation(), b.allocation());
    assert_eq!(fresh.spent(), 0.0);
    assert_identity(&fresh);
}

#[test]
fn an_unaffordable_attention_is_refused_and_the_budget_is_byte_identical() {
    let p = Parameters::genesis();
    let a = attention_pair();
    let mut b = budget(1.0);
    let before = b.clone();

    match b.spend(&a, &p) {
        Attended::OverBudget { cost, remaining } => {
            assert_eq!(cost, a.cost_under(&p));
            assert_eq!(remaining, 1.0);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(b, before, "nothing was appended");
    assert_eq!(b.spend(&a, &p).shortfall(), Some(a.cost_under(&p) - 1.0), "how much short");
}

#[test]
fn clamping_is_unsound_because_it_can_flip_a_stance() {
    // ★★★ The reason the policy is REFUSE, proven rather than argued.
    let n = Aperture::declared(3, 2, 2).unwrap();
    assert_eq!(n.stance(), Stance::Narrow, "b=3 against d*rho=4");

    // A clamp would shrink an axis to fit. Lowering resolution by one...
    let clamped = Aperture::declared(3, 2, 1).unwrap();
    assert_eq!(clamped.stance(), Stance::Broad, "...and the stance FLIPS");

    // ...so the pair a clamp produced would be Broad twice — exactly what
    // `Attention::declared` refuses outright.
    assert_eq!(
        Attention::declared(clamped, broad(), 1_000.0, &Parameters::genesis()),
        Err(AttentionError::NotTwoRegions { both: Stance::Broad }),
        "a clamp could silently produce the attention this module forbids"
    );
}

#[test]
fn there_is_no_spend_amount_so_a_cost_cannot_be_invented() {
    // ★★★ Structural: `spend` takes an `Attention`, which cannot be built
    // except through `declared` — and each record NAMES what it paid for.
    let p = Parameters::genesis();
    let a = attention_pair();
    let mut b = budget(1_000.0);
    assert!(b.spend(&a, &p).spent());

    let record = &b.spends()[0];
    assert_eq!(record.attention(), &a, "what was this spent on stays answerable");
    assert_eq!(record.cost(), a.cost_under(&p));
    assert_eq!(b.spent(), record.cost(), "the fold is the record");
}

#[test]
fn a_zero_allocation_refuses_everything_visibly_and_a_negative_one_is_not_a_budget() {
    let p = Parameters::genesis();
    let mut zero = budget(0.0);
    assert!(!zero.spend(&attention_pair(), &p).spent(), "attend to nothing, honestly");
    assert_identity(&zero);
    assert!(AttentionBudget::allocated(-1.0).is_none(), "already past before spending");
    assert!(AttentionBudget::allocated(f64::NAN).is_none());
}

#[test]
fn the_governed_kappa_is_what_prices_an_aperture_and_no_constant_remains() {
    // ★ Parity first: the genesis coefficient prices exactly as the deleted
    // constant did.
    let genesis = Parameters::genesis();
    let a = Aperture::declared(1, 4, 3).unwrap();
    assert_eq!(a.cost_under(&genesis), 12.0);

    // ★★★ Then a REAL gated change, read back out of the resulting state.
    let d = definition(&declared_parameters());
    let mut params = serde_json::Map::new();
    params.insert("name".into(), json!(ATTENTION_KAPPA));
    params.insert("value".into(), json!(2.0));
    let token = Simulated::from_sandbox(
        "gov-1",
        Binding::new("governance.set_parameter", &params),
        Ok(()),
    )
    .expect("the sandbox committed")
    .voted(ProposalStatus::Passed)
    .expect("passed")
    .approve("bonnie", 1, 9_999);

    let mut reg = Registry::default();
    register_gov(&mut reg);
    let x = execute_admitted(
        &reg,
        &d.operators,
        &enforcement(&d),
        &opening_state(&declared_parameters()),
        "governance.set_parameter",
        &params,
        &Authorization::Unchecked,
        &EffectClass::Live { token: &token, now: 1_000 },
        &mut NonceLedger::new(),
    );
    assert!(x.committed(), "{:?}", x.result.reason);

    let live = Parameters::read(&x.state);
    assert_eq!(live.attention_kappa(), 2.0);
    assert_eq!(a.cost_under(&live), 24.0);
    assert_ne!(a.cost_under(&live), a.cost_under(&genesis), "not shadowed by a constant");

    // ★★ And the coefficient reaches the ENFORCEMENT, not just the arithmetic:
    // a budget that afforded this attention at genesis kappa refuses it now.
    let pair = Attention::declared(narrow(), broad(), 100.0, &genesis).unwrap();
    let mut b = budget(pair.cost_under(&genesis));
    assert!(b.clone().spend(&pair, &genesis).spent(), "affordable at genesis");
    assert!(!b.spend(&pair, &live).spent(), "and refused at the governed value");
}
