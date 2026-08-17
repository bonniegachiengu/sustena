//! `β` and `compose(r)`, replayed against their conformance vectors
//! (R2 · Curated UI §§1–5 · UI-1, measured against the Slice-13 Python build).
//!
//! **Parity plus three sharpenings.** The algorithm is the reference's and is
//! ported as-is; what is sharper is the ∪ as a sum type, the **typed
//! population** UI-2 bought, and a **derived** rather than declared cost.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    curated::{
        attention_cost, compose_view, knapsack_select, BindingKey, BindingTable, Eligibility,
        PolicyError, Request, SaliencePolicy, UrgencyBasis, WidgetCandidate,
    },
    editing::Definition,
    event::{CausalStamp, Event, Provenance},
    operator::Registry,
    region::{Interval, Region},
    schema::{DimType, Schema},
    widget::{WidgetDecl, WidgetSet},
    CONFORMANCE_VERSION,
};

fn load_vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("curated.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "curated.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn definition() -> Definition {
    Definition::new(
        Schema::new()
            .declare("balance", DimType::Number { lo: None, hi: None })
            .declare("stock", DimType::Number { lo: None, hi: None })
            .declare("mood", DimType::Number { lo: None, hi: None }),
    )
    .with_operator("budget.allocate")
}

fn region() -> Region {
    Region::new()
        .bounding(Interval::new("balance", 0.0, 100.0))
        .bounding(Interval::new("stock", 0.0, 100.0))
        .bounding(Interval::new("mood", 0.0, 100.0))
        .weighing("balance", 1.0)
        .weighing("stock", 1.0)
        .weighing("mood", 1.0)
}

fn event(name: &str) -> Event {
    Event {
        id: format!("e-{name}"),
        name: name.into(),
        t_event: 1_000,
        t_ingest: None,
        provenance: Provenance::Observed,
        source: None,
        stamp: CausalStamp::new("test"),
        causes: vec![],
        mutations: vec![],
    }
}

fn load(decls: Vec<WidgetDecl>) -> WidgetSet {
    WidgetSet::load(decls, &definition(), &Registry::default()).unwrap()
}

fn candidate(id: &str, cost: usize, score: f64) -> WidgetCandidate {
    WidgetCandidate {
        id: id.into(),
        render: "card".into(),
        cost,
        urgency: score,
        basis: UrgencyBasis::Bounded,
        relevance: 0.0,
        score,
        why: Eligibility::AlwaysEligible,
    }
}

// ── β and event-first ────────────────────────────────────────────────────────

#[test]
fn beta_keys_on_an_event_class_or_unit_never_on_an_operator() {
    let set = load(vec![
        WidgetDecl::new("always", "card").unit().reading("balance"),
        WidgetDecl::new("on_spend", "card")
            .bound_to("event.finances.pocket_spent")
            .reading("balance"),
    ]);
    let beta = BindingTable::build(&set);
    assert_eq!(beta.at(&BindingKey::Unit), ["always".to_string()]);
    assert_eq!(
        beta.at(&BindingKey::event("event.finances.pocket_spent")),
        ["on_spend".to_string()]
    );
    assert!(
        beta.at(&BindingKey::event("budget.allocate")).is_empty(),
        "β has no key an operator name could occupy"
    );
}

#[test]
fn an_event_bound_widget_is_not_a_candidate_when_its_class_did_not_fire() {
    let set = load(vec![
        WidgetDecl::new("always", "card").unit().reading("balance"),
        WidgetDecl::new("on_spend", "card")
            .bound_to("event.finances.pocket_spent")
            .reading("balance"),
    ]);
    let view = compose_view(&set, &region(), &json!({"balance": 50.0}), &[], &Request::default(), &SaliencePolicy::default());
    assert_eq!(view.candidates_considered, 1, "never scored, not scored-and-ranked-low");
}

#[test]
fn it_becomes_a_candidate_the_moment_its_class_appears() {
    let set = load(vec![WidgetDecl::new("on_spend", "card")
        .bound_to("event.finances.pocket_spent")
        .reading("balance")]);
    let log = [event("event.finances.pocket_spent")];
    let view =
        compose_view(&set, &region(), &json!({"balance": 160.0}), &log, &Request::default(), &SaliencePolicy::default());
    assert_eq!(view.candidates_considered, 1);
    assert_eq!(
        view.selected[0].why,
        Eligibility::EventFired { class: "event.finances.pocket_spent".into() }
    );
}

// ── urgency is d(s,V) ────────────────────────────────────────────────────────

#[test]
fn a_widget_reading_the_breached_dimension_outranks_one_reading_a_healthy_one() {
    let set = load(vec![
        WidgetDecl::new("money", "card").unit().reading("balance"),
        WidgetDecl::new("feelings", "card").unit().reading("mood"),
    ]);
    let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
    let view = compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default());

    let money = view.selected.iter().find(|c| c.id == "money").unwrap();
    let feelings = view.selected.iter().find(|c| c.id == "feelings").unwrap();
    assert!(money.urgency > 0.0);
    assert_eq!(feelings.urgency, 0.0, "a healthy dimension carries no urgency");
    assert!(money.score > feelings.score);
}

#[test]
fn inside_the_viable_region_nothing_is_urgent() {
    let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
    let view = compose_view(
        &set,
        &region(),
        &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
        &[],
        &Request::default(),
        &SaliencePolicy::default(),
    );
    assert_eq!(view.excluded.iter().chain(view.selected.iter()).next().unwrap().urgency, 0.0);
}

#[test]
fn a_widget_cannot_set_its_own_urgency() {
    let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
    let pick = |v: &sustena_core::curated::View| {
        v.selected.iter().chain(v.excluded.iter()).next().unwrap().urgency
    };
    let calm = compose_view(
        &set,
        &region(),
        &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
        &[],
        &Request::default(),
        &SaliencePolicy::default(),
    );
    let breached = compose_view(
        &set,
        &region(),
        &json!({"balance": 160.0, "stock": 50.0, "mood": 50.0}),
        &[],
        &Request::default(),
        &SaliencePolicy::default(),
    );
    assert!(pick(&breached) > pick(&calm));
}

#[test]
fn cost_is_superlinear_in_the_declared_inputs() {
    assert_eq!([attention_cost(1), attention_cost(2), attention_cost(3), attention_cost(4)], [1, 2, 4, 7]);
    assert!(
        attention_cost(4) - attention_cost(3) > attention_cost(3) - attention_cost(2),
        "superlinear is checked, not asserted"
    );
}

#[test]
fn grabbing_every_dimension_to_look_urgent_is_self_defeating() {
    // ★★★ The greedy widget captures the whole of d(s,V) and scores maximally
    // — and costs 4. Against a budget of 3 it loses to two focused widgets.
    let set = load(vec![
        WidgetDecl::new("greedy", "card")
            .unit()
            .reading("balance")
            .reading("stock")
            .reading("mood"),
        WidgetDecl::new("focused_a", "card").unit().reading("balance"),
        WidgetDecl::new("focused_b", "card").unit().reading("stock"),
    ]);
    let state = json!({"balance": 160.0, "stock": 160.0, "mood": 50.0});
    let view = compose_view(&set, &region(), &state, &[], &Request::default().with_budget(3), &SaliencePolicy::default());

    let greedy = view.excluded.iter().find(|c| c.id == "greedy").expect("excluded");
    assert_eq!(greedy.cost, 4, "three inputs cost four chunks");
    assert_eq!(view.selected.len(), 2, "two focused widgets fit where one greedy one did not");
}

// ── UI-7: the blind spot, and honest zero vs manufactured zero ───────────────

/// The case UI-7's row names: a pocket with **zero allocation** spent from.
/// Under the proxy that is `spent/allocated` at a zero denominator, special-cased
/// to `0.0` — scored as perfectly calm, backwards.
fn unfunded_pocket_state() -> Value {
    json!({"balance": -40.0, "stock": 50.0, "mood": 50.0})
}

#[test]
fn the_proxy_blind_spot_is_gone_because_there_is_no_denominator() {
    // ★★★ CASE A — V declares the wall. The unfunded pocket drove balance
    // negative, d(s,V) registers it, and the widget reading it is urgent.
    let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
    let view = compose_view(
        &set,
        &region(),
        &unfunded_pocket_state(),
        &[],
        &Request::default(),
        &SaliencePolicy::default(),
    );
    assert!(view.selected[0].urgency > 0.0, "an unfunded pocket spent from is NOT calm");
    assert_eq!(view.selected[0].basis, UrgencyBasis::Bounded);
}

#[test]
fn an_undeclared_dimension_reads_zero_but_says_it_is_silence_not_safety() {
    // ★★★ CASE B — V declares no wall on what this widget reads. The urgency
    // is honestly 0, and the BASIS says why. No wall is invented to force it.
    let unbounded =
        Region::new().bounding(Interval::new("balance", 0.0, 100.0)).weighing("balance", 1.0);
    let set = load(vec![WidgetDecl::new("feelings", "card").unit().reading("mood")]);
    let view = compose_view(
        &set,
        &unbounded,
        &unfunded_pocket_state(),
        &[],
        &Request::default(),
        &SaliencePolicy::default(),
    );
    let c = view.selected.iter().chain(view.excluded.iter()).next().unwrap();
    assert_eq!(c.urgency, 0.0);
    assert_eq!(c.basis, UrgencyBasis::Undeclared { dimensions: vec!["mood".to_string()] });
    assert!(!c.basis.is_measured());
    assert!(c.basis.describe().contains("silence rather than safety"));
}

#[test]
fn an_honest_zero_inside_v_is_told_apart_from_an_unmeasured_one() {
    // ★★ Both read 0.0; only one of them means calm.
    let unbounded =
        Region::new().bounding(Interval::new("balance", 0.0, 100.0)).weighing("balance", 1.0);
    let set = load(vec![
        WidgetDecl::new("bounded", "card").unit().reading("balance"),
        WidgetDecl::new("unmeasured", "card").unit().reading("mood"),
    ]);
    let view = compose_view(
        &set,
        &unbounded,
        &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
        &[],
        &Request::default(),
        &SaliencePolicy::default(),
    );
    let all: Vec<_> = view.selected.iter().chain(view.excluded.iter()).collect();
    let b = all.iter().find(|c| c.id == "bounded").unwrap();
    let u = all.iter().find(|c| c.id == "unmeasured").unwrap();
    assert_eq!(b.urgency, u.urgency, "the same number");
    assert!(b.basis.is_measured() && !u.basis.is_measured(), "and a different meaning");
}

// ── UI-7: the weights are declared policy ────────────────────────────────────

#[test]
fn the_default_policy_is_the_articles_declared_preference() {
    let p = SaliencePolicy::default();
    assert_eq!((p.alpha(), p.lambda()), (0.75, 0.25));
}

#[test]
fn a_household_may_argue_the_ratio() {
    let strict = SaliencePolicy::declared(0.9, 0.1).unwrap();
    let set = load(vec![WidgetDecl::new("balance_card", "card").unit().reading("balance")]);
    let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
    let default_view = compose_view(
        &set,
        &region(),
        &state,
        &[],
        &Request::asking("weather"),
        &SaliencePolicy::default(),
    );
    let strict_view =
        compose_view(&set, &region(), &state, &[], &Request::asking("weather"), &strict);
    assert!(strict_view.selected[0].score > default_view.selected[0].score);
}

#[test]
fn urgency_cannot_be_argued_into_second_place() {
    // ★★★ The one thing that is not arguable.
    assert_eq!(SaliencePolicy::declared(0.4, 0.6), Err(PolicyError::UrgencyNotDominant));
    assert_eq!(SaliencePolicy::declared(0.5, 0.5), Err(PolicyError::UrgencyNotDominant));
}

#[test]
fn the_weights_must_sum_to_one_so_a_score_means_the_same_thing_everywhere() {
    assert_eq!(SaliencePolicy::declared(0.9, 0.9), Err(PolicyError::WeightsDoNotSumToOne));
    assert_eq!(SaliencePolicy::declared(f64::NAN, 0.0), Err(PolicyError::NotAWeight));
}

#[test]
fn a_widget_cannot_set_the_weights() {
    // The policy is the household's parameter, not a widget field — and the
    // urgency itself is untouched by it.
    let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
    let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
    let a = compose_view(&set, &region(), &state, &[], &Request::default(),
                         &SaliencePolicy::default());
    let b = compose_view(&set, &region(), &state, &[], &Request::default(),
                         &SaliencePolicy::declared(0.95, 0.05).unwrap());
    assert_ne!(a.selected[0].score, b.selected[0].score);
    assert_eq!(a.selected[0].urgency, b.selected[0].urgency);
}

// ── the knapsack ─────────────────────────────────────────────────────────────

#[test]
fn it_is_a_real_knapsack_and_not_a_sort_dressed_up() {
    let cands = vec![candidate("big", 4, 0.90), candidate("a", 2, 0.60), candidate("b", 2, 0.55)];
    let (selected, excluded) = knapsack_select(&cands, 4);
    let ids: BTreeSet<&str> = selected.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"].into_iter().collect::<BTreeSet<_>>());
    assert_eq!(excluded[0].id, "big");
}

#[test]
fn the_selection_is_optimal_against_brute_force() {
    let cands = vec![
        candidate("p", 1, 0.31),
        candidate("q", 2, 0.62),
        candidate("r", 3, 0.80),
        candidate("s", 2, 0.55),
        candidate("t", 1, 0.20),
    ];
    let budget = 4;
    let (selected, _) = knapsack_select(&cands, budget);
    let got: f64 = selected.iter().map(|c| c.score).sum();

    let mut best = 0.0_f64;
    for mask in 0..(1u32 << cands.len()) {
        let (mut w, mut v) = (0usize, 0.0_f64);
        for (i, c) in cands.iter().enumerate() {
            if mask & (1 << i) != 0 {
                w += c.cost;
                v += c.score;
            }
        }
        if w <= budget && v > best {
            best = v;
        }
    }
    assert!((got - best).abs() < 1e-9, "DP got {got}, brute force found {best}");
}

#[test]
fn every_exclusion_comes_back_with_its_score() {
    let cands = vec![candidate("a", 3, 0.9), candidate("b", 3, 0.4)];
    let (selected, excluded) = knapsack_select(&cands, 3);
    assert_eq!(selected.len(), 1);
    assert_eq!(excluded[0].score, 0.4, "the quiet one can say what it scored");
}

#[test]
fn a_zero_score_widget_is_not_shown_and_says_so_from_the_excluded_list() {
    // ★★ At parity with the reference: the DP takes an item only when it
    // strictly improves the total.
    let set = load(vec![WidgetDecl::new("nothing_to_say", "card").unit().reading("mood")]);
    let view = compose_view(
        &set,
        &region(),
        &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
        &[],
        &Request::asking("weather"),
        &SaliencePolicy::default(),
    );
    assert_eq!(view.candidates_considered, 1, "it WAS considered");
    assert!(view.selected.is_empty(), "and not shown");
    assert_eq!(view.excluded[0].score, 0.0, "and it can say why");
    assert_eq!(view.spent, 0);
}

#[test]
fn an_empty_field_selects_nothing_and_says_so() {
    let (selected, excluded) = knapsack_select(&[], 4);
    assert!(selected.is_empty() && excluded.is_empty());
}

// ── read-only ────────────────────────────────────────────────────────────────

#[test]
fn compose_changes_nothing() {
    let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
    let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
    let log = vec![event("event.finances.pocket_spent")];
    let (before_state, before_log) = (state.clone(), log.clone());

    let _ = compose_view(&set, &region(), &state, &log, &Request::default(), &SaliencePolicy::default());
    let _ = compose_view(&set, &region(), &state, &log, &Request::default(), &SaliencePolicy::default());
    assert_eq!(state, before_state);
    assert_eq!(log, before_log);
}

#[test]
fn composing_twice_gives_the_same_view() {
    let set = load(vec![
        WidgetDecl::new("a", "card").unit().reading("balance"),
        WidgetDecl::new("b", "card").unit().reading("stock"),
    ]);
    let state = json!({"balance": 160.0, "stock": 120.0, "mood": 50.0});
    let one = compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default());
    let two = compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default());
    assert_eq!(one, two, "resolved fresh, and deterministically");
}

#[test]
fn relevance_is_neutral_with_no_query_and_real_with_one() {
    let set = load(vec![WidgetDecl::new("balance_card", "card").unit().reading("balance")]);
    let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
    assert_eq!(
        compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default()).selected[0].relevance,
        0.5
    );
    assert_eq!(
        compose_view(&set, &region(), &state, &[], &Request::asking("balance"), &SaliencePolicy::default()).selected[0]
            .relevance,
        1.0
    );
    assert_eq!(
        compose_view(&set, &region(), &state, &[], &Request::asking("weather"), &SaliencePolicy::default()).selected[0]
            .relevance,
        0.0
    );
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load_vectors();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "parity-plus-sharpening");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★_1_there_is_NO_EventClass_in_this_core_and_none_in_the_reference_either",
        "★★_2_the_one_sharpening_that_IS_worth_it_the_UNION_AS_A_SUM_TYPE",
        "★★_3_UI_2's_typed_population_is_exactly_the_ordering_benefit_it_was_taken_first_for",
        "★_4_and_the_urgency_signal_is_ALREADY_HERE_and_better_than_the_one_being_ported",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★ UI-7's reconcile: the blind spot was discharged by UI-1, and the
    // genuine residual was the weights. Both must stay on the record.
    for key in [
        "★★_5_UI_7_reconcile_2026_08_17_the_blind_spot_was_ALREADY_DISCHARGED",
        "★★_6_the_GENUINE_residual_was_the_WEIGHTS_and_it_was_real",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    assert!(step0["★★_5_UI_7_reconcile_2026_08_17_the_blind_spot_was_ALREADY_DISCHARGED"]
        .as_str()
        .unwrap()
        .contains("grep-0"), "the 1.1 pin must stay recorded as checked");

    let blind = d["★★_UI_7_the_blind_spot_is_the_PROXY'S_and_the_proxy_is_gone"].as_str().unwrap();
    assert!(blind.contains("manufactured zero"));
    assert!(
        blind.contains("Rather than invent a wall to force urgency"),
        "the refusal to fabricate a wall must stay stated"
    );
    let weights = d["★★_the_weights_are_now_DECLARED_POLICY_not_a_module_constant"].as_str().unwrap();
    assert!(weights.contains("may not argue urgency into second place"));

    // The sentinel collision must stay described as theoretical, not inflated.
    assert!(step0["★★_2_the_one_sharpening_that_IS_worth_it_the_UNION_AS_A_SUM_TYPE"]
        .as_str()
        .unwrap()
        .contains("theoretical"));

    // ★★★ The cost finding: a formula written down and never computed.
    let cost = d["★★★_the_sharpening_that_matters_most_COST_IS_DERIVED_NOT_DECLARED"]
        .as_str()
        .unwrap();
    assert!(cost.contains("written down and never computed"));
    assert!(cost.contains("colour_rule"), "the pattern must stay named");

    // ★★ The counterweight: the algorithm is the reference's and it is good.
    let cw = d["★★_the_counterweight_THE_ALGORITHM_IS_THE_REFERENCE'S_AND_IT_IS_GOOD"]
        .as_str()
        .unwrap();
    for term in ["genuine 0/1 DP", "Slice 0", "verbatim", "declared as a proxy"] {
        assert!(cw.contains(term), "missing counterweight detail: {term}");
    }

    let fp = d["the_greppable_false_positives_named_so_nobody_re_derives_them"]
        .as_object()
        .unwrap();
    assert!(fp.keys().any(|k| k.contains("compose (real hits in Rust")));
    assert!(fp.keys().any(|k| k.contains("EventClass / event_class (grep-0 on BOTH sides)")));
    assert!(
        fp.keys().any(|k| k.contains("spent / allocated")),
        "the grep a reviewer runs to check UI-7 must stay named"
    );

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "`render` STAYS OPAQUE",
        "URGENCY IS A **SHARE** OF `d(s,V)`",
        "`relations_violated` CONTRIBUTES NO URGENCY",
        "A ZERO-SCORE WIDGET IS NEVER SELECTED",
        "`device` IS CARRIED AND NOT USED",
        "RELEVANCE IS TOKEN OVERLAP, NOT MEANING",
        "UNDECLARED IS REPORTED, NOT PENALISED",
        "THE BASIS IS PER-WIDGET, NOT PER-DIMENSION-OF-THE-SUM",
        "THE SUM-TO-ONE RULE IS A CONSTRAINT ON THE ARGUMENT, NOT A DERIVATION",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in ["beta_cases", "urgency_cases", "knapsack_cases", "read_only_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 24, "every declared case must be present");
}
