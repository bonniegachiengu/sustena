//! Signal functions and the temporal pincer replayed against their
//! conformance vectors (R2 · Tenet §IX, §X · TEN-8, TEN-9).
//!
//! **SPEC vectors, Rust-only.** The reference engine has neither half —
//! `trajectory`, `signal_function`, `linked_action`, `re_inversion`, `replan`,
//! `HMM`, `pincer` and `reclassif` are all grep-0. `pincer.json` records that
//! term by term and **names four greppable false positives**, the largest
//! being `history` at 102 hits — essentially all of it the merchant-to-pocket
//! classification table and chat history, never a sequence of state snapshots.
//!
//! The proof of value: a declared signal reads the observed HISTORY, fires,
//! reclassifies the belief, and the re-inverted policy takes a different
//! action.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    run_pincer, BellmanSpec, DeadDropBook, Ensemble, InversionPoint, ModelTemplate, PincerError,
    PincerRun, PincerSpec, Prob, RecomputeReason, Scenario, ScriptedDraws, Signal, SignalMonitor,
    SignalSpec, SignalVerdict, Space, Trajectory, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("pincer.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "pincer.json was written for a different contract version"
    );
    doc
}

// ── fixture, built from the vector file ─────────────────────────────────────

fn dim(doc: &Value) -> String {
    doc["fixture"]["dimension"].as_str().unwrap().to_string()
}

fn history_of(doc: &Value, vals: &[f64]) -> Vec<Value> {
    let d = dim(doc);
    vals.iter().map(|v| json!({ d.clone(): v })).collect()
}

fn trajectory_of(spec: &Value) -> Trajectory {
    let state = spec["state"].as_str().unwrap();
    match spec["kind"].as_str().unwrap() {
        "never_within" => Trajectory::never_within(spec["steps"].as_u64().unwrap() as usize, state),
        "ever_within" => Trajectory::ever_within(spec["steps"].as_u64().unwrap() as usize, state),
        "never_unbounded" => Trajectory::Never { within: None, state: state.to_string() },
        "count" => Trajectory::Count {
            within: Some(spec["steps"].as_u64().unwrap() as usize),
            state: state.to_string(),
            at_least: spec["at_least"].as_u64().unwrap() as usize,
        },
        other => panic!("unknown predicate kind '{other}'"),
    }
}

fn space_of(doc: &Value) -> Space {
    let d = dim(doc);
    doc["fixture"]["states"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, v)| (id.clone(), json!({ d.clone(): v.as_f64().unwrap() })))
        .collect()
}

fn prob_of(s: &str) -> Prob {
    let (kind, rest) = s.split_once(':').expect("tagged");
    match kind {
        "param" => Prob::param(rest),
        "one_minus" => Prob::one_minus(rest),
        "fixed" => Prob::Fixed(rest.parse().unwrap()),
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

fn ensemble_of(doc: &Value) -> Ensemble {
    let scs: Vec<Scenario> = doc["fixture"]["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let mut s = Scenario::new(v["name"].as_str().unwrap());
            for (k, p) in v["params"].as_object().unwrap() {
                s = s.assigning(k, p.as_f64().unwrap());
            }
            s
        })
        .collect();
    Ensemble::new(scs, doc["fixture"]["base_scenario"].as_str().unwrap()).expect("well formed")
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

fn bellman_of(doc: &Value) -> BellmanSpec {
    let f = &doc["fixture"];
    let mut b = BellmanSpec::new(
        f["horizon"].as_u64().unwrap() as usize,
        f["gamma"].as_f64().unwrap(),
        f["terminal_reward"].as_f64().unwrap(),
    );
    for (k, r) in f["rewards"].as_object().unwrap() {
        let (s, a) = k.split_once('|').expect("state|action");
        b = b.rewarding(s, a, r.as_f64().unwrap());
    }
    b
}

fn monitor_of(doc: &Value) -> SignalMonitor {
    let sg = &doc["fixture"]["signal"];
    let mut s = Signal::new(sg["id"].as_str().unwrap(), trajectory_of(&sg["predicate"]));
    if let Some(sc) = sg["evidence_for"].as_str() {
        s = s.evidence_for(sc);
    }
    if let Some(a) = sg["linked_action"].as_str() {
        s = s.re_planning(a);
    }
    SignalMonitor::new(vec![s], signal_spec_of(doc)).expect("one declared signal")
}

fn signal_spec_of(doc: &Value) -> SignalSpec {
    SignalSpec::new(
        doc["signal_spec"]["theta_fire"].as_f64().unwrap(),
        doc["signal_spec"]["refractory_ticks"].as_u64().unwrap() as usize,
    )
    .expect("a real medium")
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from pincer.json"))
}

fn verdict_matches(v: &SignalVerdict, want: &str, settles: Option<&Value>) -> bool {
    match (v, want) {
        (SignalVerdict::Holds, "Holds") | (SignalVerdict::Fails, "Fails") => true,
        (SignalVerdict::Undetermined { settles_at }, "Undetermined") => match settles {
            Some(Value::Null) | None => settles_at.is_none(),
            Some(n) => *settles_at == Some(n.as_u64().unwrap() as usize),
        },
        _ => false,
    }
}

fn run_case(doc: &Value, c: &Value) -> Result<PincerRun, PincerError> {
    let mut m = monitor_of(doc);
    let mut drops = DeadDropBook::new();
    let draws: Vec<f64> =
        c["draws"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap()).collect()).unwrap_or(vec![0.0]);
    let mut d = ScriptedDraws::new(draws);

    let mut spec = PincerSpec::new(c["max_steps"].as_u64().unwrap_or(6) as usize);
    if let Some(cad) = c["cadence"].as_u64() {
        spec = spec.every(cad as usize).expect("non-zero");
    }

    run_pincer(
        &ensemble_of(doc),
        &template_of(doc),
        &space_of(doc),
        &ip_of(doc),
        &bellman_of(doc),
        &mut m,
        &mut drops,
        &spec,
        "start",
        &mut d,
    )
}

// ── §IX — the trajectory predicate, and the article correction ──────────────

#[test]
fn the_three_valued_trajectory_verdicts_match_the_vector() {
    let doc = load();
    for name in [
        "a_bounded_never_is_undetermined_until_its_window_closes",
        "a_never_settles_false_the_moment_it_is_contradicted",
        "an_ever_settles_true_early_and_false_only_at_the_window",
        "a_count_settles_false_as_soon_as_it_is_unreachable",
    ] {
        let c = case(&doc, "trajectory_cases", name);
        let sigma = trajectory_of(&c["predicate"]);
        for e in c["expect"].as_array().unwrap() {
            let vals: Vec<f64> =
                e["history"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
            let got = sigma.evaluate(&history_of(&doc, &vals)).expect("evaluates");
            assert!(
                verdict_matches(&got, e["verdict"].as_str().unwrap(), e.get("settles_at")),
                "{name}: history {vals:?} gave {got:?}, vector says {}",
                e["verdict"]
            );
        }
    }
}

#[test]
fn an_empty_history_does_not_make_a_warning_true() {
    // ★★ THE ARTICLE CORRECTION, isolated. §IX's two-valued reading makes
    // "no customers by week 6" true at week 0 and fires every warning before
    // the run begins.
    let doc = load();
    let c = case(&doc, "trajectory_cases", "a_bounded_never_is_undetermined_until_its_window_closes");
    let sigma = trajectory_of(&c["predicate"]);

    let got = sigma.evaluate(&[]).expect("evaluates");
    assert!(
        matches!(got, SignalVerdict::Undetermined { .. }),
        "nothing has happened yet, which is not the same as the warning being true; got {got:?}"
    );
    assert!(!got.holds(), "and so it must not fire");

    // The correction is recorded in the vector, not just in the code.
    let corr = &doc["article_correction"];
    assert!(corr["why_it_matters"].as_str().unwrap().contains("TRUE at week 0"));
    assert!(corr["same_discipline_as"].as_str().unwrap().contains("corrected, not copied"));
}

#[test]
fn an_unbounded_never_can_only_ever_be_falsified() {
    let doc = load();
    let c = case(&doc, "trajectory_cases", "an_unbounded_never_can_only_ever_be_falsified");
    let sigma = trajectory_of(&c["predicate"]);

    let e = &c["expect"].as_array().unwrap()[0];
    let zeros = vec![0.0; e["history_zeros"].as_u64().unwrap() as usize];
    let got = sigma.evaluate(&history_of(&doc, &zeros)).unwrap();
    assert!(
        matches!(got, SignalVerdict::Undetermined { settles_at: None }),
        "there is always more history — an unbounded never cannot be confirmed; got {got:?}"
    );

    let e = &c["expect"].as_array().unwrap()[1];
    let vals: Vec<f64> = e["history"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert_eq!(sigma.evaluate(&history_of(&doc, &vals)).unwrap(), SignalVerdict::Fails);
}

// ── §IX — Monitor ───────────────────────────────────────────────────────────

#[test]
fn a_signal_that_holds_triggers_with_its_linked_action() {
    let doc = load();
    let c = case(&doc, "monitor_cases", "a_signal_that_holds_triggers_with_its_linked_action");
    let mut m = monitor_of(&doc);
    let vals: Vec<f64> = c["history"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();

    let r = m.monitor(&history_of(&doc, &vals)).expect("monitors");
    let fired: Vec<&str> = r.triggered.iter().map(|t| t.signal.as_str()).collect();
    let want: Vec<&str> =
        c["expect"]["triggered"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(fired, want);
    assert_eq!(r.triggered[0].linked_action.as_deref(), c["expect"]["linked_action"].as_str());
    assert_eq!(r.triggered[0].reclassifies_to.as_deref(), c["expect"]["reclassifies_to"].as_str());
}

#[test]
fn a_persistent_condition_is_debounced_by_the_refractory_window() {
    let doc = load();
    let c = case(&doc, "monitor_cases", "a_persistent_condition_is_debounced_by_the_refractory_window");
    let mut m = monitor_of(&doc);

    let hs = c["histories"].as_array().unwrap();
    let first: Vec<f64> = hs[0].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let second: Vec<f64> = hs[1].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();

    let r1 = m.monitor(&history_of(&doc, &first)).unwrap();
    assert_eq!(r1.fired(), c["expect"]["first_fires"].as_bool().unwrap());

    let r2 = m.monitor(&history_of(&doc, &second)).unwrap();
    assert_eq!(
        r2.fired(),
        c["expect"]["second_fires"].as_bool().unwrap(),
        "★ the same persistent condition must not re-trigger a full re-inversion"
    );
    let want: Vec<String> = c["expect"]["second_debounced"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(r2.debounced, want, "and the suppression is VISIBLE, not silent");
}

#[test]
fn nothing_fired_carries_the_list_it_was_watching() {
    let doc = load();
    let c = case(&doc, "monitor_cases", "nothing_fired_carries_the_list_it_was_watching");
    let mut m = monitor_of(&doc);
    let vals: Vec<f64> = c["history"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();

    let r = m.monitor(&history_of(&doc, &vals)).unwrap();
    assert_eq!(r.fired(), c["expect"]["fired"].as_bool().unwrap());

    let want: Vec<String> = c["expect"]["watched"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(r.watched(), want.as_slice());
    assert!(
        r.describe().contains(c["expect"]["describe_contains"].as_str().unwrap()),
        "★ 'nothing fired' must never read as 'nothing is wrong': {}",
        r.describe()
    );
}

#[test]
fn an_empty_signal_set_is_refused() {
    let doc = load();
    let _ = case(&doc, "monitor_cases", "an_empty_signal_set_is_refused");
    assert!(
        matches!(SignalMonitor::new(vec![], signal_spec_of(&doc)), Err(PincerError::NoSignals)),
        "'nothing fired' from an empty set is a category error, not reassurance"
    );
}

// ── ★ §X — the pincer, and the proof of value ───────────────────────────────

#[test]
fn a_fired_signal_reclassifies_and_the_policy_changes() {
    let doc = load();
    let c = case(&doc, "pincer_cases", "a_fired_signal_reclassifies_and_the_policy_changes");
    let run = run_case(&doc, c).expect("runs");
    let e = &c["expect"];

    let effective = run.effective_recomputes();
    assert_eq!(
        !effective.is_empty(),
        e["some_recompute_changed_the_policy"].as_bool().unwrap(),
        "★ a trigger that changed nothing would not be a re-inversion"
    );

    let r = effective[0];
    assert!(
        matches!(r.reason, RecomputeReason::SignalFired(_)),
        "the re-plan was caused by the SIGNAL, not the cadence; got {:?}",
        r.reason
    );
    assert_eq!(r.base_before, e["base_before"].as_str().unwrap());
    assert_eq!(
        r.base_after,
        e["base_after"].as_str().unwrap(),
        "★ the observable emission inferred the latent scenario (§IX's HMM reading)"
    );
    assert_eq!(
        r.action_before != r.action_after,
        e["action_changed"].as_bool().unwrap(),
        "★ and the plan genuinely changed: {:?} → {:?}",
        r.action_before,
        r.action_after
    );

    // The signal read the HISTORY, not the current state — the whole point of
    // §IX. The run visited more than one state before it fired.
    assert!(run.history.len() > 1);
}

#[test]
fn without_a_signal_firing_the_belief_is_untouched() {
    let doc = load();
    let c = case(&doc, "pincer_cases", "without_a_signal_firing_the_belief_is_untouched");
    let run = run_case(&doc, c).expect("runs");
    let e = &c["expect"];

    assert_eq!(run.reached_ip, e["reached_ip"].as_bool().unwrap());
    let stays = e["base_stays"].as_str().unwrap();
    assert!(
        run.recomputes.iter().all(|r| r.base_after == stays),
        "nothing fired, so nothing was reclassified — the proof case must not pass by \
         reclassifying on every run"
    );
}

#[test]
fn the_cadence_recomputes_even_when_no_signal_fires() {
    let doc = load();
    let c = case(&doc, "pincer_cases", "the_cadence_recomputes_even_when_no_signal_fires");

    // A signal that can never hold, so only the cadence can cause a recompute.
    let mut m = SignalMonitor::new(
        vec![Signal::new("never_fires", Trajectory::ever_within(1, "customers >= 999"))],
        signal_spec_of(&doc),
    )
    .unwrap();
    let mut drops = DeadDropBook::new();
    let draws: Vec<f64> = c["draws"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let mut d = ScriptedDraws::new(draws);
    let spec = PincerSpec::new(c["max_steps"].as_u64().unwrap() as usize)
        .every(c["cadence"].as_u64().unwrap() as usize)
        .unwrap();

    let run = run_pincer(
        &ensemble_of(&doc),
        &template_of(&doc),
        &space_of(&doc),
        &ip_of(&doc),
        &bellman_of(&doc),
        &mut m,
        &mut drops,
        &spec,
        "start",
        &mut d,
    )
    .expect("runs");

    assert_eq!(
        run.recomputes.iter().any(|r| r.reason == RecomputeReason::Cadence),
        c["expect"]["has_cadence_recompute"].as_bool().unwrap(),
        "the backward process must run on its own clock too, not only in reaction"
    );
}

#[test]
fn the_forward_process_samples_the_declared_transition_reproducibly() {
    let doc = load();
    let c = case(&doc, "pincer_cases", "the_forward_process_samples_the_declared_transition_reproducibly");

    let a = run_case(&doc, c).expect("runs");
    let b = run_case(&doc, c).expect("runs again");
    let path = |r: &PincerRun| r.steps.iter().map(|s| s.to.clone()).collect::<Vec<_>>();

    assert_eq!(
        (path(&a) == path(&b)),
        c["expect"]["two_runs_identical"].as_bool().unwrap(),
        "★ no RNG in core — the same supplied draws walk the same path"
    );
    assert_eq!(
        a.steps[0].to,
        c["expect"]["first_step_to"].as_str().unwrap(),
        "0.99 takes the unlucky branch of p_demand = 0.9"
    );
}

#[test]
fn the_recompute_carries_the_window_it_is_about() {
    let doc = load();
    let c = case(&doc, "pincer_cases", "the_recompute_carries_the_window_it_is_about");
    let run = run_case(&doc, c).expect("runs");
    let h = c["expect"]["every_recompute_horizon"].as_u64().unwrap() as usize;

    assert!(
        run.recomputes.iter().all(|r| r.horizon == h),
        "re-running the sweep more often does not make it see further"
    );
}

#[test]
fn the_run_refuses_what_it_cannot_honestly_do() {
    let doc = load();

    let _ = case(&doc, "pincer_cases", "an_unknown_start_state_is_refused");
    let mut m = monitor_of(&doc);
    let mut drops = DeadDropBook::new();
    let mut d = ScriptedDraws::always_first();
    assert!(matches!(
        run_pincer(
            &ensemble_of(&doc),
            &template_of(&doc),
            &space_of(&doc),
            &ip_of(&doc),
            &bellman_of(&doc),
            &mut m,
            &mut drops,
            &PincerSpec::new(4),
            "nowhere",
            &mut d,
        ),
        Err(PincerError::UnknownState(_))
    ));

    let _ = case(&doc, "pincer_cases", "a_zero_cadence_is_refused");
    assert!(
        matches!(PincerSpec::new(4).every(0), Err(PincerError::ZeroCadence)),
        "a cadence of 0 recomputes on every step and executes nothing — the pincer with one arm"
    );
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // The false positives, named — `history` at 102 hits being the one most
    // likely to be believed.
    let fp = &d["the_four_greppable_false_positives_named_so_nobody_re_derives_them"];
    let h = fp["history (102 hits)"].as_str().unwrap();
    assert!(h.contains("capture_classification_history"));
    assert!(h.contains("S^t"), "and why a lookup table is not a trajectory");
    assert!(fp["cadence (4 hits)"].as_str().unwrap().contains("staleness"));
    assert!(fp["recompute (1 hit)"].as_str().unwrap().contains("AGGREGATE"));

    // Counterweight: the advisory layer is real, and the two differences are
    // visible in its signatures rather than asserted.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("evaluate_operatives"), "the counterweight names it");
    assert!(cw.contains("rule_unallocated_income(state: dict)"), "with the signature as evidence");
    assert!(cw.contains("SUGGESTION for a human"), "and what a trigger there actually produces");

    // TEN-10 is already done and NOT claimed by this slice.
    assert!(
        d["term_by_term"]["the fork runs under the gate (Additions, TEN-10)"]
            .as_str()
            .unwrap()
            .contains("ALREADY DONE"),
        "this slice must not appear to claim a row R1 finished"
    );

    // "Parallel" is interleaved, and the file says so.
    assert!(
        d["the_pincer_is_interleaved_not_threaded_and_that_is_deliberate"]
            .as_str()
            .unwrap()
            .contains("would imply something this does not do")
    );

    // Every case explains what it pins.
    for group in ["trajectory_cases", "monitor_cases", "pincer_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    let mut seen = BTreeSet::new();
    for group in ["trajectory_cases", "monitor_cases", "pincer_cases"] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
