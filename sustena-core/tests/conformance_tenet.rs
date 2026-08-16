//! The optimiser core replayed against its conformance vectors
//! (R2 · Tenet §II, §III, §V + Additions · TEN-1, TEN-4).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no optimiser:
//! `backward_induct`, `bellman`, `argmax`, `discount`, `stochastic`,
//! `expectation` and `markov` are all grep-0 in `apps/api/sustena/`.
//! `tenet.json` records that term by term, names the two greppable false
//! positives so nobody mistakes them for hits, and keeps the counterweight —
//! `simulate.fork → run_path → score` is real decision *support*, it just
//! evaluates a plan a human already wrote instead of searching for one.
//!
//! The sharp case is `backward_induction_takes_a_worse_first_step_for_a_better
//! _end`: the proof that a horizon buys something a greedy rule structurally
//! cannot reach. Its twin, `at_h_equals_one_bellman_reproduces_the_lyapunov
//! _choice`, checks the CTL-12 join rather than asserting it.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    backward_induct, rewards_from_region, viability_kernel, BellmanSpec, Change, Distribution,
    Interval, InversionPoint, Move, Outcome, Region, Space, TransitionModel, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("tenet.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "tenet.json was written for a different contract version"
    );
    doc
}

// ── fixture, built from the vector file rather than restated in Rust ────────

fn region_of(doc: &Value) -> Region {
    let spec = &doc["space"]["region"];
    let mut r = Region::new();
    for iv in spec["intervals"].as_array().unwrap() {
        let a = iv.as_array().unwrap();
        r = r.bounding(Interval::new(
            a[0].as_str().unwrap(),
            a[1].as_f64().unwrap_or(f64::NEG_INFINITY),
            a[2].as_f64().unwrap_or(f64::INFINITY),
        ));
    }
    for (dim, w) in spec["weights"].as_object().unwrap() {
        r = r.weighing(dim, w.as_f64().unwrap());
    }
    r
}

fn space_of(doc: &Value) -> Space {
    doc["space"]["states"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, v)| (id.clone(), json!({ "balance": v.as_f64().unwrap() })))
        .collect()
}

fn model_of(doc: &Value) -> TransitionModel {
    let mut m = TransitionModel::new();
    for e in doc["space"]["model"].as_array().unwrap() {
        m = m.moving(
            e["from"].as_str().unwrap(),
            e["action"].as_str().unwrap(),
            e["to"].as_str().unwrap(),
        );
    }
    m
}

fn ip_of(doc: &Value) -> InversionPoint<'static> {
    InversionPoint::Predicate(
        doc["space"]["inversion_point"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect(),
    )
}

/// The declared spec, with `R(s,a)` derived from the region — which is what
/// makes the `H = 1` case genuinely the Controller's rule and not a lookalike.
fn spec_of(doc: &Value, horizon: usize, gamma: f64) -> BellmanSpec {
    let terminal = doc["space"]["terminal_reward"].as_f64().unwrap();
    rewards_from_region(
        &region_of(doc),
        &space_of(doc),
        &model_of(doc),
        BellmanSpec::new(horizon, gamma, terminal),
    )
    .expect("rewards derive")
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from tenet.json"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// ── the rewards the vector declares are the rewards the code derives ────────

#[test]
fn the_declared_rewards_are_the_ones_derived_from_the_region() {
    let doc = load();
    let spec = spec_of(&doc, 3, 0.9);
    let declared = &doc["space"]["rewards_from_region"];

    for (key, want) in declared.as_object().unwrap() {
        let Some((s, a)) = key.split_once('|') else { continue }; // skip "why"
        assert!(
            close(spec.reward(s, a), want.as_f64().unwrap()),
            "R({s},{a}) = {} but the vector declares {want}",
            spec.reward(s, a)
        );
    }

    // And the W column the vector shows its working with.
    let region = region_of(&doc);
    let space = space_of(&doc);
    for (id, want) in doc["space"]["w_derived"].as_object().unwrap() {
        let w = region.distance(&space[id]).unwrap().weighted;
        assert!(close(w, want.as_f64().unwrap()), "W({id}) = {w}, vector says {want}");
    }
}

// ── §II — Δ(S), not a state ─────────────────────────────────────────────────

#[test]
fn a_distribution_is_refused_rather_than_normalised() {
    let doc = load();

    let c = case(&doc, "distribution_cases", "a_distribution_that_does_not_sum_to_one_is_refused_not_normalised");
    let outs: Vec<Outcome> = c["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let a = o.as_array().unwrap();
            Outcome { to: a[0].as_str().unwrap().into(), p: a[1].as_f64().unwrap() }
        })
        .collect();
    let err = Distribution::new(outs).unwrap_err();
    assert!(
        format!("{err:?}").starts_with("NotADistribution"),
        "expected NotADistribution, got {err:?}"
    );
    // The refusal names the total rather than quietly rescaling to it.
    let total = c["expect"]["total"].as_f64().unwrap();
    assert!(err.to_string().contains(&format!("{total}")), "the refusal states the total: {err}");

    let c = case(&doc, "distribution_cases", "a_negative_probability_is_refused");
    let outs: Vec<Outcome> = c["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let a = o.as_array().unwrap();
            Outcome { to: a[0].as_str().unwrap().into(), p: a[1].as_f64().unwrap() }
        })
        .collect();
    let err = Distribution::new(outs).unwrap_err();
    assert!(
        format!("{err:?}").starts_with("BadProbability"),
        "expected BadProbability, got {err:?}"
    );
}

#[test]
fn the_deterministic_case_is_the_reference_engine_expressed_in_the_model() {
    let doc = load();
    let c = case(&doc, "distribution_cases", "the_deterministic_case_is_a_point_mass");
    let d = Distribution::certain(c["certain"].as_str().unwrap());

    assert!(d.is_deterministic(), "the R1 forward simulator IS a point mass");
    assert_eq!(d.outcomes().len(), 1);
    assert_eq!(d.outcomes()[0].to, c["expect"]["outcomes"][0][0].as_str().unwrap());
    assert!(close(d.outcomes()[0].p, 1.0));
}

#[test]
fn a_genuine_distribution_spreads_the_expectation() {
    let doc = load();
    let c = case(&doc, "distribution_cases", "a_genuine_distribution_spreads_the_expectation");
    let region = region_of(&doc);
    let space = space_of(&doc);

    let outs: Vec<Outcome> = c["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let a = o.as_array().unwrap();
            Outcome { to: a[0].as_str().unwrap().into(), p: a[1].as_f64().unwrap() }
        })
        .collect();
    let d = Distribution::new(outs).expect("a real distribution");

    let w = |id: &str| region.distance(&space[id]).unwrap().weighted;
    let got = d.expectation(w);
    assert!(
        close(got, c["expect"]["expectation"].as_f64().unwrap()),
        "E[W] = {got}, vector says {}",
        c["expect"]["expectation"]
    );
    assert!(!d.is_deterministic(), "the thing a point mass cannot do");
}

// ── §V — backward induction, and the proof of value ─────────────────────────

#[test]
fn backward_induction_takes_a_worse_first_step_for_a_better_end() {
    let doc = load();
    let c = case(&doc, "sweep_cases", "backward_induction_takes_a_worse_first_step_for_a_better_end");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let h = c["horizon"].as_u64().unwrap() as usize;
    let spec = spec_of(&doc, h, c["gamma"].as_f64().unwrap());

    let plan = backward_induct(&space, &model, &ip_of(&doc), &spec).expect("sweeps");
    let from = c["from"].as_str().unwrap();

    assert_eq!(
        plan.action(0, from),
        Some(c["expect"]["action_at_t0"].as_str().unwrap()),
        "the horizon buys the worse first step"
    );
    let j = plan.value(0, from).unwrap();
    assert!(close(j, c["expect"]["value_at_t0"].as_f64().unwrap()), "J[0][{from}] = {j}");

    // And the step it takes really is worse by the greedy measure — otherwise
    // this vector would prove nothing.
    let region = region_of(&doc);
    let w = |id: &str| region.distance(&space[id]).unwrap().weighted;
    assert!(
        w("slow") > w("quick"),
        "the chosen first step must genuinely INCREASE W, or there is no tension to resolve"
    );
}

#[test]
fn at_h_equals_one_bellman_reproduces_the_lyapunov_choice() {
    let doc = load();
    let c = case(&doc, "sweep_cases", "at_h_equals_one_bellman_reproduces_the_lyapunov_choice");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let region = region_of(&doc);
    let from = c["from"].as_str().unwrap();

    let spec = spec_of(&doc, 1, c["gamma"].as_f64().unwrap());
    let plan = backward_induct(&space, &model, &ip_of(&doc), &spec).expect("sweeps");
    let bellman = plan.action(0, from).expect("an action");

    // The greedy rule, computed independently here rather than imported — so
    // this is a genuine cross-check and not the same function twice.
    let w = |id: &str| region.distance(&space[id]).unwrap().weighted;
    let greedy = model
        .actions_at(from)
        .into_iter()
        .min_by(|a, b| {
            let wa = model.get(from, a).unwrap().expectation(w);
            let wb = model.get(from, b).unwrap().expectation(w);
            wa.partial_cmp(&wb).unwrap()
        })
        .expect("an action");

    assert_eq!(bellman, greedy, "★ CTL-12: the H=1 sweep IS the greedy Lyapunov step");
    assert_eq!(bellman, c["expect"]["action_at_t0"].as_str().unwrap());
    assert!(c["expect"]["agrees_with_greedy_lyapunov"].as_bool().unwrap());
}

#[test]
fn a_longer_horizon_can_change_the_first_move() {
    let doc = load();
    let c = case(&doc, "sweep_cases", "a_longer_horizon_can_change_the_first_move");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let ip = ip_of(&doc);
    let from = c["from"].as_str().unwrap();
    let gamma = c["gamma"].as_f64().unwrap();

    let hs = c["horizons"].as_array().unwrap();
    let short = hs[0].as_u64().unwrap() as usize;
    let long = hs[1].as_u64().unwrap() as usize;

    let p_short = backward_induct(&space, &model, &ip, &spec_of(&doc, short, gamma)).unwrap();
    let p_long = backward_induct(&space, &model, &ip, &spec_of(&doc, long, gamma)).unwrap();

    assert_eq!(p_short.action(0, from), Some(c["expect"]["action_at_h1"].as_str().unwrap()));
    assert_eq!(p_long.action(0, from), Some(c["expect"]["action_at_h3"].as_str().unwrap()));
    assert_ne!(
        p_short.action(0, from),
        p_long.action(0, from),
        "same state, same model, same rewards — a different answer, because the window changed"
    );

    // The window is carried, not implied.
    assert_eq!(p_short.horizon(), short);
    assert_eq!(p_long.horizon(), long);
}

#[test]
fn gamma_zero_is_pure_myopia() {
    let doc = load();
    let c = case(&doc, "sweep_cases", "gamma_zero_is_pure_myopia");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let h = c["horizon"].as_u64().unwrap() as usize;

    let plan =
        backward_induct(&space, &model, &ip_of(&doc), &spec_of(&doc, h, 0.0)).expect("sweeps");

    assert_eq!(
        plan.action(0, c["from"].as_str().unwrap()),
        Some(c["expect"]["action_at_t0"].as_str().unwrap()),
        "no discounting of the future — no future"
    );
}

#[test]
fn value_propagates_backward_discounted() {
    let doc = load();
    let c = case(&doc, "sweep_cases", "value_propagates_backward_discounted");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let h = c["horizon"].as_u64().unwrap() as usize;
    let spec = spec_of(&doc, h, c["gamma"].as_f64().unwrap());

    let plan = backward_induct(&space, &model, &ip_of(&doc), &spec).expect("sweeps");
    let state = c["state"].as_str().unwrap();

    for (t, want) in c["expect"]["j"].as_array().unwrap().iter().enumerate() {
        let got = plan.value(t, state).unwrap();
        assert!(close(got, want.as_f64().unwrap()), "J[{t}][{state}] = {got}, vector says {want}");
    }
}

#[test]
fn the_policy_is_complete_for_every_state_at_every_time() {
    let doc = load();
    let c = case(&doc, "sweep_cases", "the_policy_is_complete_for_every_state_at_every_time");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let h = c["horizon"].as_u64().unwrap() as usize;
    let spec = spec_of(&doc, h, c["gamma"].as_f64().unwrap());

    let plan = backward_induct(&space, &model, &ip_of(&doc), &spec).expect("sweeps");

    // Every state in this fixture declares at least one action, so π is total.
    // A policy is not a path: it answers for states the plan never expected to
    // visit, which is the whole point when the world does not comply.
    for t in 0..h {
        for id in space.keys() {
            assert!(
                plan.action(t, id).is_some(),
                "π is a COMPLETE policy — no action at t={t} for {id}"
            );
        }
    }
    assert!(c["expect"]["action_defined_for_all_states_at_all_t"].as_bool().unwrap());
}

#[test]
fn the_most_likely_path_is_a_reading_aid_not_the_policy() {
    let doc = load();
    let c = case(&doc, "sweep_cases", "the_most_likely_path_is_a_reading_aid_not_the_policy");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let h = c["horizon"].as_u64().unwrap() as usize;
    let spec = spec_of(&doc, h, c["gamma"].as_f64().unwrap());

    let plan = backward_induct(&space, &model, &ip_of(&doc), &spec).expect("sweeps");
    let got = plan.most_likely_path(&model, c["from"].as_str().unwrap());
    let want: Vec<String> = c["expect"]["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert_eq!(got, want);
}

// ── §III — the inversion point IS the terminal condition ────────────────────

#[test]
fn the_terminal_condition_is_the_inversion_point() {
    let doc = load();
    let c = case(&doc, "terminal_condition_cases", "the_terminal_condition_is_the_inversion_point");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let h = c["horizon"].as_u64().unwrap() as usize;
    let spec = spec_of(&doc, h, c["gamma"].as_f64().unwrap());

    let plan = backward_induct(&space, &model, &ip_of(&doc), &spec).expect("sweeps");

    for (id, want) in c["expect"]["j_at_h"].as_object().unwrap() {
        let got = plan.value(h, id).unwrap();
        assert!(close(got, want.as_f64().unwrap()), "J[H][{id}] = {got}, vector says {want}");
    }
}

#[test]
fn the_viability_kernel_can_be_the_terminal_condition() {
    let doc = load();
    let c = case(&doc, "terminal_condition_cases", "the_viability_kernel_can_be_the_terminal_condition");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let region = region_of(&doc);

    // A move set over the same space, so the kernel is computed rather than
    // assumed: `hold` keeps a state where it is, so only states that can hold
    // (or reach one that can) survive the descending iteration.
    let moves = vec![
        Move::new("stay").changing("balance", Change::ShiftBy(0.0)),
        Move::new("drain").changing("balance", Change::ShiftBy(-50.0)),
    ];
    let kernel = viability_kernel(&region, &space, &moves).expect("computes");

    let spec = spec_of(&doc, 2, 0.9);
    let plan =
        backward_induct(&space, &model, &InversionPoint::Kernel(&kernel), &spec).expect("sweeps");

    let terminal = spec.terminal_reward;
    let mut tracked = 0;
    for id in space.keys() {
        let want = if kernel.contains(id) { terminal } else { 0.0 };
        let got = plan.value(2, id).unwrap();
        assert!(
            close(got, want),
            "J[H][{id}] = {got}; the kernel says in={} so it should be {want}",
            kernel.contains(id)
        );
        tracked += 1;
    }
    assert!(tracked > 0);
    assert!(c["expect"]["terminal_value_tracks_kernel_membership"].as_bool().unwrap());

    // The reading that makes this worth having: "worth reaching" is now
    // "from here you can keep going", which is a property of the world rather
    // than a number somebody picked.
    assert!(!kernel.contains("__nothing__"));
}

#[test]
fn a_state_with_no_admissible_action_carries_its_value_rather_than_scoring_zero() {
    let doc = load();
    let c = case(
        &doc,
        "terminal_condition_cases",
        "a_state_with_no_admissible_action_carries_its_value_rather_than_scoring_zero",
    );
    let mut space = space_of(&doc);
    space.insert("stranded".into(), json!({ "balance": 900.0 }));

    // No action is declared FROM `stranded` — terminal by absence.
    let model = model_of(&doc);
    let spec = BellmanSpec::new(2, 0.9, 500.0);
    let ip = InversionPoint::Predicate(vec!["balance >= 500".into()]);

    let plan = backward_induct(&space, &model, &ip, &spec).expect("sweeps");

    // It satisfies IP, so J[H] is the terminal reward — and it must still be
    // that at t=0. Scoring it 0 would claim standing there is worthless, when
    // the model only says nobody declared a move.
    assert!(close(plan.value(2, "stranded").unwrap(), 500.0));
    assert!(
        close(plan.value(0, "stranded").unwrap(), 500.0),
        "carried, not zeroed — the absence is stated rather than filled"
    );
    assert_eq!(plan.action(0, "stranded"), None, "no action, rather than an invented one");
    assert!(c["expect"]["carries_forward"].as_bool().unwrap());
}

// ── refusals ────────────────────────────────────────────────────────────────

#[test]
fn outcomes_outside_the_enumerated_space_are_refused() {
    let doc = load();
    let c = case(&doc, "refusal_cases", "outcomes_outside_the_enumerated_space_are_refused");
    let space = space_of(&doc);
    let dangling = c["dangling_to"].as_str().unwrap();

    let model = model_of(&doc).moving("start", "leap", dangling);
    let err = backward_induct(&space, &model, &ip_of(&doc), &spec_of(&doc, 2, 0.9)).unwrap_err();

    assert!(
        format!("{err:?}").starts_with("DanglingOutcomes"),
        "a dangling edge would be silently scored 0 — a claim nobody made; got {err:?}"
    );
    assert!(err.to_string().contains(dangling), "the refusal names the state: {err}");
}

#[test]
fn the_parameters_are_checked_at_authoring_time() {
    let doc = load();
    let c = case(&doc, "refusal_cases", "the_parameters_are_checked_at_authoring_time");
    let space = space_of(&doc);
    let model = model_of(&doc);
    let ip = ip_of(&doc);

    for sub in c["cases"].as_array().unwrap() {
        let want = sub["expect"].as_str().unwrap();
        let spec = if let Some(g) = sub["gamma"].as_f64() {
            BellmanSpec::new(2, g, 500.0)
        } else {
            BellmanSpec::new(sub["horizon"].as_u64().unwrap() as usize, 0.9, 500.0)
        };
        let err = backward_induct(&space, &model, &ip, &spec).unwrap_err();
        assert!(
            format!("{err:?}").starts_with(want),
            "expected {want}, got {err:?} — caught before a sweep runs, rather than \
             producing a plan that looks like a plan"
        );
    }
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The counterweight: the reference's simulator is REAL, and saying so is
    // what keeps the gap from being overstated.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("simulate.fork"), "the counterweight names the real mechanism");
    assert!(
        cw.contains("evaluates a sequence a human already wrote"),
        "and states precisely what it cannot do — search over sequences"
    );

    // The greppable false positives, named so nobody re-derives them as hits.
    let stmt = d["statement"].as_str().unwrap();
    assert!(stmt.contains("Gamma |- r"), "the 'gamma' hits are the typing judgment");
    assert!(stmt.contains("child_policy"), "the 'policy' hits are allow-lists");

    // The CTL-12 join is a SHARED mechanism, not extra rust-ahead surface.
    assert!(
        d["the_ctl12_join_is_not_a_divergence"].as_str().unwrap().contains("ONE mechanism"),
        "understating the shared core would overstate the gap"
    );

    // The V→J rename is a departure from the article's own text, recorded.
    let dep = d["a_departure_from_the_article_recorded_rather_than_made_silently"]
        .as_str()
        .unwrap();
    assert!(dep.contains("Additions"), "the article was corrected, not copied");

    // Every distribution/sweep/terminal/refusal case explains what it pins.
    for group in ["distribution_cases", "sweep_cases", "terminal_condition_cases", "refusal_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why — reviewing a spec vector means checking it \
                 against the article, not against the code",
                c["name"]
            );
        }
    }

    // Vector names are unique, so `case()` can never silently match the wrong one.
    let mut seen = BTreeSet::new();
    for group in ["distribution_cases", "sweep_cases", "terminal_condition_cases", "refusal_cases"]
    {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
