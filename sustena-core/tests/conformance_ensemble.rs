//! Scenario ensembles, invariant actions, decision nodes and dead drops
//! replayed against their conformance vectors
//! (R2 · Tenet §IV, §VI, §VII, §VIII · TEN-3, TEN-5, TEN-6, TEN-7).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no ensemble layer —
//! `ensemble`, `invariant_action`, `decision_node`, `dead_drop`, `pre_commit`,
//! `best_case`, `worst_case` and `Schelling` are all grep-0. `ensemble.json`
//! records that term by term and **names the greppable false positive**: the
//! two hits for `scenario` are Slice 9's Simulator "scenario tree", a tree of
//! branches a human authored by hand over a *deterministic* simulator. Same
//! word, different object — a branch there is a sequence of operators someone
//! chose to try; a scenario here is a declared assignment of probabilities.
//!
//! The proof of value is the pair: `s0` is invariant across all three futures
//! while `denied` splits them and carries a pre-committed dead drop.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    BellmanSpec, DeadDropBook, Ensemble, EnsembleAnalysis, EnsembleError, Interval, InversionPoint,
    ModelTemplate, Prob, Region, Resolution, RewardBasis, Scenario, Space, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("ensemble.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "ensemble.json was written for a different contract version"
    );
    doc
}

// ── the fixture, built from the vector file rather than restated in Rust ────

fn space_of(doc: &Value) -> Space {
    let dim = doc["fixture"]["dimension"].as_str().unwrap().to_string();
    doc["fixture"]["states"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, v)| (id.clone(), json!({ dim.clone(): v.as_f64().unwrap() })))
        .collect()
}

/// `"param:p_funding"` / `"one_minus:p_funding"` / `"fixed:1.0"`.
fn prob_of(spec: &str) -> Prob {
    let (kind, rest) = spec.split_once(':').expect("probabilities are tagged");
    match kind {
        "param" => Prob::param(rest),
        "one_minus" => Prob::one_minus(rest),
        "fixed" => Prob::Fixed(rest.parse().expect("a number")),
        other => panic!("unknown probability kind '{other}'"),
    }
}

fn template_of(doc: &Value) -> ModelTemplate {
    let mut t = ModelTemplate::new();
    for e in doc["fixture"]["template"].as_array().unwrap() {
        let outs: Vec<(&str, Prob)> = e["outcomes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                let a = o.as_array().unwrap();
                (a[0].as_str().unwrap(), prob_of(a[1].as_str().unwrap()))
            })
            .collect();
        t = t.edge(e["state"].as_str().unwrap(), e["action"].as_str().unwrap(), outs);
    }
    t
}

fn scenario_of(v: &Value) -> Scenario {
    let mut s = Scenario::new(v["name"].as_str().unwrap());
    for (k, p) in v["params"].as_object().unwrap() {
        s = s.assigning(k, p.as_f64().unwrap());
    }
    s
}

fn ensemble_of(doc: &Value) -> Ensemble {
    let scenarios: Vec<Scenario> =
        doc["fixture"]["scenarios"].as_array().unwrap().iter().map(scenario_of).collect();
    Ensemble::new(scenarios, doc["fixture"]["base_scenario"].as_str().unwrap())
        .expect("the declared ensemble is well formed")
}

fn ip_of(doc: &Value) -> InversionPoint<'static> {
    InversionPoint::Predicate(
        doc["fixture"]["inversion_point"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect(),
    )
}

fn spec_of(doc: &Value) -> BellmanSpec {
    let f = &doc["fixture"];
    let mut spec = BellmanSpec::new(
        f["horizon"].as_u64().unwrap() as usize,
        f["gamma"].as_f64().unwrap(),
        f["terminal_reward"].as_f64().unwrap(),
    );
    for (key, r) in f["rewards"].as_object().unwrap() {
        if let Some((s, a)) = key.split_once('|') {
            spec = spec.rewarding(s, a, r.as_f64().unwrap());
        }
    }
    spec
}

fn analysis_of(doc: &Value) -> EnsembleAnalysis {
    ensemble_of(doc)
        .analyse(&template_of(doc), &space_of(doc), &ip_of(doc), &spec_of(doc), RewardBasis::Declared)
        .expect("every declared scenario sweeps")
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from ensemble.json"))
}

// ── §IV — the ensemble ──────────────────────────────────────────────────────

#[test]
fn a_scenario_is_a_probability_assignment_pushed_through_one_structure() {
    let doc = load();
    let c = case(&doc, "ensemble_cases", "a_scenario_is_a_probability_assignment_pushed_through_one_structure");
    let t = template_of(&doc);

    let want: BTreeSet<String> = c["expect"]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(t.parameters(), want);

    let scs = doc["fixture"]["scenarios"].as_array().unwrap();
    let best = t.instantiate(&scenario_of(&scs[0])).expect("best instantiates");
    let worst = t.instantiate(&scenario_of(&scs[2])).expect("worst instantiates");

    // Same structure, different numbers — the premise the comparison rests on.
    assert_eq!(
        best.actions_at("s0"),
        worst.actions_at("s0"),
        "the futures must differ only in their probabilities"
    );

    let p = |m: &sustena_core::TransitionModel, to: &str| {
        m.get("s0", "fundraise").unwrap().outcomes().iter().find(|o| o.to == to).unwrap().p
    };
    let e = &c["expect"];
    assert!((p(&best, "raised") - e["best_p_raised"].as_f64().unwrap()).abs() < 1e-9);
    assert!((p(&worst, "raised") - e["worst_p_raised"].as_f64().unwrap()).abs() < 1e-9);
    // OneMinus supplied the other branch from the single declared number.
    assert!((p(&worst, "denied") - e["worst_p_denied"].as_f64().unwrap()).abs() < 1e-9);
}

#[test]
fn an_unbound_parameter_is_named_not_defaulted() {
    let doc = load();
    let c = case(&doc, "ensemble_cases", "an_unbound_parameter_is_named_not_defaulted");

    let mut partial = Scenario::new("partial");
    for k in c["binds"].as_array().unwrap() {
        partial = partial.assigning(k.as_str().unwrap(), 0.5);
    }
    let err = template_of(&doc).instantiate(&partial).unwrap_err();
    match err {
        EnsembleError::UnboundParameter { parameter, .. } => {
            assert_eq!(parameter, c["expect"]["parameter"].as_str().unwrap());
        }
        other => panic!("expected UnboundParameter, got {other:?}"),
    }
}

#[test]
fn an_incoherent_scenario_cannot_be_swept() {
    let doc = load();
    let c = case(&doc, "ensemble_cases", "an_incoherent_scenario_cannot_be_swept");
    let bad = Scenario::new("bad")
        .assigning("p_funding", c["p_funding"].as_f64().unwrap())
        .assigning("p_growth", 0.5);

    let err = template_of(&doc).instantiate(&bad).unwrap_err();
    assert!(
        matches!(err, EnsembleError::IncoherentScenario { .. }),
        "refused by Distribution::new's own rule, not a second check; got {err:?}"
    );
}

#[test]
fn a_vacuous_ensemble_is_refused_structurally() {
    let doc = load();
    let _ = case(&doc, "ensemble_cases", "a_one_scenario_ensemble_is_refused");

    let err = Ensemble::new(vec![Scenario::new("only")], "only").unwrap_err();
    assert!(
        matches!(err, EnsembleError::TooFewScenarios(1)),
        "with one future every action is trivially invariant; got {err:?}"
    );

    let _ = case(&doc, "ensemble_cases", "the_base_must_be_one_of_the_declared_scenarios");
    let err = Ensemble::new(vec![Scenario::new("a"), Scenario::new("b")], "middle").unwrap_err();
    assert!(matches!(err, EnsembleError::UnknownBase(_)), "got {err:?}");
}

#[test]
fn every_scenario_gets_its_own_policy_and_the_window_is_carried() {
    let doc = load();
    let c = case(&doc, "ensemble_cases", "every_scenario_gets_its_own_policy_and_the_window_is_carried");
    let a = analysis_of(&doc);

    let want: Vec<String> = c["expect"]["scenario_names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(a.scenario_names(), want.as_slice());
    for n in &want {
        assert!(a.plan(n).is_some(), "{n} has no policy");
        assert!(a.model(n).is_some(), "{n} has no model");
    }
    assert_eq!(
        a.horizon(),
        c["expect"]["horizon"].as_u64().unwrap() as usize,
        "an ensemble of horizon-limited answers is still horizon-limited"
    );
}

// ── §VI — invariant actions ─────────────────────────────────────────────────

#[test]
fn an_action_optimal_in_all_three_futures_is_invariant() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "an_action_optimal_in_all_three_futures_is_invariant");
    let a = analysis_of(&doc);

    let inv = a.invariant_actions(
        c["t"].as_u64().unwrap() as usize,
        c["state"].as_str().unwrap(),
    );
    assert_eq!(
        inv.the_one(),
        Some(c["expect"]["the_one"].as_str().unwrap()),
        "{}",
        inv.describe()
    );

    let over: Vec<String> = c["expect"]["over"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(inv.over(), over.as_slice(), "the scope travels with the claim");
}

#[test]
fn the_invariant_claim_cannot_be_quoted_without_its_scope() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "the_invariant_claim_cannot_be_quoted_without_its_scope");
    let a = analysis_of(&doc);

    let inv = a.invariant_actions(
        c["t"].as_u64().unwrap() as usize,
        c["state"].as_str().unwrap(),
    );
    assert!(
        inv.describe().contains(c["expect"]["describe_contains"].as_str().unwrap()),
        "the one-line reading must carry the limit: {}",
        inv.describe()
    );
    // Every scenario is named in the reading, so it cannot be trimmed to a
    // bare "invariant" without losing something visible.
    for n in a.scenario_names() {
        assert!(inv.describe().contains(n.as_str()), "'{n}' missing from the reading");
    }
}

#[test]
fn the_invariant_set_is_empty_where_the_scenarios_split() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "the_invariant_set_is_empty_where_the_scenarios_split");
    let a = analysis_of(&doc);

    let inv = a.invariant_actions(
        c["t"].as_u64().unwrap() as usize,
        c["state"].as_str().unwrap(),
    );
    assert!(inv.is_empty(), "{}", inv.describe());
    assert!(inv.describe().contains(c["expect"]["describe_contains"].as_str().unwrap()));
}

#[test]
fn the_invariant_set_is_at_most_one_action() {
    let doc = load();
    let c = case(&doc, "invariant_cases", "the_invariant_set_is_at_most_one_action");
    let a = analysis_of(&doc);
    let max = c["expect"]["max_size_everywhere"].as_u64().unwrap() as usize;

    for t in 0..a.horizon() {
        for s in space_of(&doc).keys() {
            assert!(
                a.invariant_actions(t, s).actions().len() <= max,
                "I is an intersection of singletons — t={t} {s}"
            );
        }
    }
}

// ── §VII — decision nodes ───────────────────────────────────────────────────

#[test]
fn a_decision_node_is_where_the_optimal_action_diverges() {
    let doc = load();
    let c = case(&doc, "decision_node_cases", "a_decision_node_is_where_the_optimal_action_diverges");
    let a = analysis_of(&doc);
    let t = c["t"].as_u64().unwrap() as usize;
    let state = c["state"].as_str().unwrap();
    let e = &c["expect"];

    assert_eq!(a.is_decision_node(t, state), e["is_decision_node"].as_bool().unwrap());

    let node = a
        .decision_nodes_at(t)
        .into_iter()
        .find(|n| n.state == state)
        .unwrap_or_else(|| panic!("{state} should be a decision node at t={t}"));

    // The node carries WHAT each scenario wants — the disagreement is the
    // actionable part, and it is what a dead drop is written against.
    for name in ["best", "base", "worst"] {
        assert_eq!(
            node.by_scenario[name].as_deref(),
            Some(e[name].as_str().unwrap()),
            "{}",
            node.describe()
        );
    }
    assert_eq!(node.proposed().len(), e["proposed_count"].as_u64().unwrap() as usize);

    let neg = case(&doc, "decision_node_cases", "the_state_the_scenarios_agree_on_is_not_a_node");
    assert_eq!(
        a.is_decision_node(neg["t"].as_u64().unwrap() as usize, neg["state"].as_str().unwrap()),
        neg["expect"]["is_decision_node"].as_bool().unwrap(),
        "the test must not pass by calling everything a decision node"
    );
}

#[test]
fn a_decision_node_is_horizon_dependent() {
    let doc = load();
    let c = case(&doc, "decision_node_cases", "a_decision_node_is_horizon_dependent");
    let a = analysis_of(&doc);
    let state = c["state"].as_str().unwrap();

    for t in c["expect"]["node_at"].as_array().unwrap() {
        let t = t.as_u64().unwrap() as usize;
        assert!(a.is_decision_node(t, state), "expected a node at t={t}");
    }
    for t in c["expect"]["not_node_at"].as_array().unwrap() {
        let t = t.as_u64().unwrap() as usize;
        assert!(
            !a.is_decision_node(t, state),
            "no window left for a pivot to repay at t={t} — every future winds down"
        );
    }
    assert_eq!(
        a.invariant_actions(2, state).the_one(),
        Some(c["expect"]["invariant_at_2"].as_str().unwrap()),
        "where they agree again, that agreement is itself invariant"
    );
}

#[test]
fn identical_futures_cannot_disagree() {
    let doc = load();
    let c = case(&doc, "decision_node_cases", "identical_futures_cannot_disagree");

    let flat = Ensemble::new(
        vec![
            Scenario::new("a").assigning("p_funding", 0.5).assigning("p_growth", 0.5),
            Scenario::new("b").assigning("p_funding", 0.5).assigning("p_growth", 0.5),
        ],
        "a",
    )
    .expect("two distinct names");

    let a = flat
        .analyse(&template_of(&doc), &space_of(&doc), &ip_of(&doc), &spec_of(&doc), RewardBasis::Declared)
        .expect("sweeps");

    assert_eq!(
        a.decision_nodes().len(),
        c["expect"]["decision_nodes"].as_u64().unwrap() as usize,
        "divergence must be read from the POLICIES, not from the fact that several ran"
    );
}

// ── §VIII — dead drops ──────────────────────────────────────────────────────

#[test]
fn a_dead_drop_fires_at_its_node() {
    let doc = load();
    let c = case(&doc, "dead_drop_cases", "a_dead_drop_fires_at_its_node");
    let a = analysis_of(&doc);
    let t = c["t"].as_u64().unwrap() as usize;
    let state = c["state"].as_str().unwrap();

    let mut book = DeadDropBook::new();
    book.pre_commit(&a, "d_denied", t, state, c["condition"].as_str(), c["action"].as_str().unwrap())
        .expect("a real node, an admissible action, a readable guard");

    let r = book.execute_at(&a, &space_of(&doc), t, state).expect("resolves");
    assert!(r.was_pre_committed(), "decided in advance, under clarity");
    assert_eq!(r.action(), Some(c["expect"]["action"].as_str().unwrap()));
}

#[test]
fn a_drop_whose_guard_does_not_hold_falls_back_and_names_itself() {
    let doc = load();
    let c = case(&doc, "dead_drop_cases", "a_drop_whose_guard_does_not_hold_falls_back_and_names_itself");
    let a = analysis_of(&doc);
    let t = c["t"].as_u64().unwrap() as usize;
    let state = c["state"].as_str().unwrap();

    let mut book = DeadDropBook::new();
    book.pre_commit(&a, "d_rich", t, state, c["condition"].as_str(), c["action"].as_str().unwrap())
        .expect("registers");

    match book.execute_at(&a, &space_of(&doc), t, state).expect("resolves") {
        Resolution::Policy { from_scenario, declined, .. } => {
            assert_eq!(
                from_scenario,
                c["expect"]["from_scenario"].as_str().unwrap(),
                "the fallback is the DECLARED base, not a guess or an average"
            );
            let want: Vec<String> = c["expect"]["declined"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            assert_eq!(
                declined, want,
                "a pre-commitment that quietly did not fire is worse than none"
            );
        }
        other => panic!("expected a policy fallback, got {other:?}"),
    }
}

#[test]
fn a_suggestion_is_never_dressed_as_a_pre_commitment() {
    let doc = load();
    let c = case(&doc, "dead_drop_cases", "a_suggestion_is_never_dressed_as_a_pre_commitment");
    let a = analysis_of(&doc);
    let t = c["t"].as_u64().unwrap() as usize;
    let state = c["state"].as_str().unwrap();

    let book = DeadDropBook::new();
    let r = book.execute_at(&a, &space_of(&doc), t, state).expect("resolves");

    match &r {
        Resolution::Policy { from_scenario, declined, .. } => {
            assert_eq!(from_scenario, c["expect"]["from_scenario"].as_str().unwrap());
            assert!(declined.is_empty());
        }
        other => panic!("expected Policy, got {other:?}"),
    }
    assert_eq!(r.was_pre_committed(), c["expect"]["was_pre_committed"].as_bool().unwrap());
}

#[test]
fn a_pre_commitment_is_checked_while_checking_is_cheap() {
    let doc = load();
    let a = analysis_of(&doc);

    // Not a decision node — nothing to pre-decide where every future agrees.
    let c = case(&doc, "dead_drop_cases", "a_drop_on_a_state_the_scenarios_agree_about_is_refused");
    let mut book = DeadDropBook::new();
    let err = book
        .pre_commit(
            &a,
            "d_agreed",
            c["t"].as_u64().unwrap() as usize,
            c["state"].as_str().unwrap(),
            None,
            c["action"].as_str().unwrap(),
        )
        .unwrap_err();
    assert!(matches!(err, EnsembleError::NotADecisionNode { .. }), "got {err:?}");

    // Inadmissible action — a decision that would fail exactly when relied on.
    let c = case(&doc, "dead_drop_cases", "a_drop_naming_an_action_the_model_does_not_admit_is_refused");
    let err = book
        .pre_commit(
            &a,
            "d_bad",
            c["t"].as_u64().unwrap() as usize,
            c["state"].as_str().unwrap(),
            None,
            c["action"].as_str().unwrap(),
        )
        .unwrap_err();
    match err {
        EnsembleError::InadmissibleAction { admissible, .. } => assert!(
            admissible.contains(&c["expect"]["admissible_contains"].as_str().unwrap().to_string())
        ),
        other => panic!("expected InadmissibleAction, got {other:?}"),
    }

    // Duplicate id — an ambiguous decision is what pre-commitment removes.
    let _ = case(&doc, "dead_drop_cases", "a_duplicate_dead_drop_is_refused");
    book.pre_commit(&a, "d", 0, "denied", None, "pivot").expect("first registers");
    let err = book.pre_commit(&a, "d", 0, "denied", None, "wind_down").unwrap_err();
    assert!(matches!(err, EnsembleError::DuplicateDeadDrop(_)), "got {err:?}");
}

#[test]
fn a_hedge_is_allowed_and_flagged() {
    let doc = load();
    let c = case(&doc, "dead_drop_cases", "a_hedge_is_allowed_and_flagged");
    let a = analysis_of(&doc);

    let proposed: BTreeSet<String> =
        a.actions_at(0, "denied").values().flatten().cloned().collect();
    let mut book = DeadDropBook::new();

    // Whatever the analysis proposed here, the flag must match reality — a
    // hedge is allowed, it just cannot be invisible.
    for action in ["pivot", "wind_down"] {
        let id = format!("d_{action}");
        let d = book.pre_commit(&a, &id, 0, "denied", None, action).expect("admissible here");
        assert_eq!(
            d.is_hedge(),
            !proposed.contains(action),
            "'{action}' hedge flag must match whether any scenario proposed it"
        );
    }
    assert!(c["expect"]["hedge_flag_matches_whether_any_scenario_proposed_it"].as_bool().unwrap());
}

// ── the reward basis ────────────────────────────────────────────────────────

#[test]
fn a_region_derived_reward_is_itself_scenario_dependent() {
    let doc = load();
    let c = case(&doc, "reward_basis_cases", "a_region_derived_reward_is_itself_scenario_dependent");
    let dim = doc["fixture"]["dimension"].as_str().unwrap();

    let region = Region::new().bounding(Interval::at_least(dim, 6.0)).weighing(dim, 1.0);
    let a = ensemble_of(&doc)
        .analyse(
            &template_of(&doc),
            &space_of(&doc),
            &ip_of(&doc),
            &spec_of(&doc),
            RewardBasis::FromRegion(&region),
        )
        .expect("sweeps under every scenario");

    assert_eq!(a.horizon(), c["expect"]["horizon"].as_u64().unwrap() as usize);
    for n in a.scenario_names() {
        assert!(a.plan(n).is_some(), "{n} has no policy under a region-derived reward");
    }
    assert!(c["expect"]["sweeps_under_every_scenario"].as_bool().unwrap());
}

// ── ★ the proof of value, in one place ──────────────────────────────────────

#[test]
fn one_action_holds_in_every_future_while_another_state_splits_and_carries_a_dead_drop() {
    let doc = load();
    let a = analysis_of(&doc);
    let space = space_of(&doc);

    // 1. Invariant: raise, regardless of which world this turns out to be.
    let inv = a.invariant_actions(0, "s0");
    assert_eq!(inv.the_one(), Some("fundraise"));
    assert_eq!(inv.over().len(), 3, "and invariant over exactly the three declared");

    // 2. Divergent: at `denied` the futures genuinely disagree.
    assert!(a.is_decision_node(0, "denied"));
    let node = a.decision_nodes_at(0).into_iter().find(|n| n.state == "denied").unwrap();
    assert_eq!(node.by_scenario["best"].as_deref(), Some("pivot"));
    assert_eq!(node.by_scenario["worst"].as_deref(), Some("wind_down"));

    // 3. Pre-committed: the split is decided NOW, under clarity, with a guard.
    let mut book = DeadDropBook::new();
    book.pre_commit(&a, "d_pivot_if_runway", 0, "denied", Some("runway >= 3"), "pivot")
        .expect("a real node");

    let r = book.execute_at(&a, &space, 0, "denied").expect("resolves");
    assert!(r.was_pre_committed());
    assert_eq!(r.action(), Some("pivot"));

    // And the two are never confused: at the invariant state there is no drop
    // and no node, so the answer there is the base policy — labelled as such.
    assert!(!book.execute_at(&a, &space, 0, "s0").unwrap().was_pre_committed());
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The greppable false positive, named so nobody re-derives it as a hit.
    let fp = d["the_greppable_false_positive_named_so_nobody_re_derives_it"].as_str().unwrap();
    assert!(fp.contains("scenario tree"), "the two `scenario` hits are named");
    assert!(
        fp.contains("deterministic"),
        "and distinguished — a hand-authored branch is not a probability assignment"
    );

    // The counterweight: the Simulator is real, and outcomes-vs-policies is
    // the precise reason it is not this.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("Simulator panel is real"), "the counterweight names it");
    assert!(
        cw.contains("compares OUTCOMES") && cw.contains("compares POLICIES"),
        "and states the distinction precisely rather than waving at it"
    );

    // One term is AT PARITY — claiming the whole module as rust-ahead would
    // overstate the gap.
    assert!(
        d["term_by_term"]["the fork runs under the gate (Additions)"]
            .as_str()
            .unwrap()
            .contains("AT PARITY"),
        "the sandbox's gate enforcement is genuinely satisfied in Python today"
    );

    // The limit the method itself cannot remove.
    let lim = d["an_honest_limit_this_layer_cannot_remove"].as_str().unwrap();
    assert!(lim.contains("DECLARED scenarios"), "invariance has a scope");
    assert!(lim.contains("not a Rust shortfall"), "and it is a property of the method");

    // Every case explains what it pins.
    for group in
        ["ensemble_cases", "invariant_cases", "decision_node_cases", "dead_drop_cases", "reward_basis_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    // Names are unique, so `case()` can never silently match the wrong one.
    let mut seen = BTreeSet::new();
    for group in
        ["ensemble_cases", "invariant_cases", "decision_node_cases", "dead_drop_cases", "reward_basis_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
