//! The Signal primitive replayed against its conformance vectors
//! (R2 · Multiparty §III + Additions · MUL-4, MUL-15).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no Signal primitive —
//! `refractory`, `excitable`, `FitzHugh`, `Nagumo`, `theta_fire`, `RESTING`,
//! `EXCITED` and `neighbour` are all grep-0. `signal.json` records that term by
//! term and **names four greppable false positives**, of which the sharpest is
//! `pulse`: five hits, every one the substring in *"imPULSE spending"*.
//!
//! The proof of value is one case: a pulse recruits a whole field from a single
//! origin that does not know the field, and **cannot re-excite its sender**.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{
    Field, Phase, RefusalReason, SignalError, SignalSpec, StepReport, CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("signal.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "signal.json was written for a different contract version"
    );
    doc
}

fn spec_of(doc: &Value) -> SignalSpec {
    let s = &doc["spec"];
    SignalSpec::new(
        s["theta_fire"].as_f64().unwrap(),
        s["refractory_ticks"].as_u64().unwrap() as usize,
    )
    .expect("the declared spec is a real medium")
}

/// Build a declared topology. Edges are wired with the symmetric `connect`.
fn field_of(doc: &Value, name: &str, spec: SignalSpec) -> Field {
    let topo = &doc["topologies"][name];
    let mut f = Field::new(spec);
    for n in topo["nodes"].as_array().unwrap() {
        f = f.with_node(n.as_str().unwrap());
    }
    for e in topo["edges"].as_array().unwrap() {
        let a = e.as_array().unwrap();
        f.connect(a[0].as_str().unwrap(), a[1].as_str().unwrap()).expect("declared nodes");
    }
    f
}

fn case<'a>(doc: &'a Value, group: &str, name: &str) -> &'a Value {
    doc[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("{group}/{name} missing from signal.json"))
}

fn fire_order(trace: &[StepReport]) -> Vec<&str> {
    trace.iter().flat_map(|r| r.fired.iter()).map(|f| f.node.as_str()).collect()
}

// ── the declared spec ───────────────────────────────────────────────────────

#[test]
fn a_medium_with_no_refractory_period_is_refused() {
    let doc = load();
    let c = case(&doc, "spec_cases", "a_medium_with_no_refractory_period_is_refused");

    let err = SignalSpec::new(
        doc["spec"]["theta_fire"].as_f64().unwrap(),
        c["refractory_ticks"].as_u64().unwrap() as usize,
    )
    .unwrap_err();

    assert!(matches!(err, SignalError::NoRefractoryPeriod), "got {err:?}");
    assert!(
        err.to_string().contains(c["expect"]["message_contains"].as_str().unwrap()),
        "the refusal must explain WHY the mechanism cannot be zeroed: {err}"
    );
}

#[test]
fn a_threshold_that_everything_clears_is_refused() {
    let doc = load();
    let c = case(&doc, "spec_cases", "a_threshold_that_everything_clears_is_refused");
    for th in c["thresholds"].as_array().unwrap() {
        let th = th.as_f64().unwrap();
        assert!(
            matches!(SignalSpec::new(th, 2), Err(SignalError::BadThreshold(_))),
            "θ_fire = {th} should be refused"
        );
    }
    // NaN is not expressible in JSON, so it is checked here rather than declared.
    assert!(matches!(SignalSpec::new(f64::NAN, 2), Err(SignalError::BadThreshold(_))));
}

#[test]
fn a_self_edge_is_refused() {
    let doc = load();
    let _ = case(&doc, "spec_cases", "a_self_edge_is_refused");
    let mut f = Field::new(spec_of(&doc)).with_node("a");
    assert!(
        matches!(f.connect("a", "a"), Err(SignalError::SelfEdge(_))),
        "a node relaying to itself is the echo the refractory period prevents, by another route"
    );
}

#[test]
fn edges_are_symmetric() {
    let doc = load();
    let c = case(&doc, "spec_cases", "edges_are_symmetric");
    let f = field_of(&doc, "chain", spec_of(&doc));

    assert_eq!(f.neighbours("a").unwrap().contains("b"), c["expect"]["a_knows_b"].as_bool().unwrap());
    assert_eq!(
        f.neighbours("b").unwrap().contains("a"),
        c["expect"]["b_knows_a"].as_bool().unwrap(),
        "★ nothing in the topology may prefer a direction, or the wave's directionality \
         would be evidence about the graph rather than about refractoriness"
    );

    // Stronger: EVERY declared edge is symmetric, so no direction hides anywhere.
    for e in doc["topologies"]["ring"]["edges"].as_array().unwrap() {
        let a = e.as_array().unwrap();
        let (x, y) = (a[0].as_str().unwrap(), a[1].as_str().unwrap());
        let r = field_of(&doc, "ring", spec_of(&doc));
        assert!(r.neighbours(x).unwrap().contains(y) && r.neighbours(y).unwrap().contains(x));
    }
}

// ── §III — the relay ────────────────────────────────────────────────────────

#[test]
fn the_pulse_travels_one_node_per_tick() {
    let doc = load();
    let c = case(&doc, "relay_cases", "the_pulse_travels_one_node_per_tick");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    f.stimulate(c["stimulate"].as_str().unwrap(), 1.0).unwrap();

    for expected in c["expect"]["fired_by_tick"].as_array().unwrap() {
        let want: Vec<&str> =
            expected.as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        let r = f.step();
        let got: Vec<&str> = r.fired.iter().map(|x| x.node.as_str()).collect();
        assert_eq!(got, want, "tick {}", r.tick);
    }
}

#[test]
fn a_stimulus_below_threshold_does_nothing() {
    let doc = load();
    let c = case(&doc, "relay_cases", "a_stimulus_below_threshold_does_nothing");
    let spec = SignalSpec::new(c["theta_fire"].as_f64().unwrap(), 2).unwrap();
    let mut f = field_of(&doc, "chain", spec);

    f.stimulate("a", c["stimulate_strength"].as_f64().unwrap()).unwrap();
    let r = f.step();

    assert!(r.fired.is_empty());
    assert!(matches!(r.refused_because("a"), Some(RefusalReason::BelowThreshold { .. })));
    assert_eq!(f.phase("b"), Some(Phase::Resting), "nothing travelled");
}

#[test]
fn the_three_phases_are_all_observable() {
    let doc = load();
    let c = case(&doc, "relay_cases", "the_three_phases_are_all_observable");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    f.stimulate(c["stimulate"].as_str().unwrap(), 1.0).unwrap();

    let phases = c["expect"]["phase_by_tick"].as_array().unwrap();
    for (i, want) in phases.iter().enumerate() {
        let got = f.phase("a").unwrap();
        let want = match want.as_str().unwrap() {
            "Resting" => Phase::Resting,
            "Excited" => Phase::Excited,
            "Refractory" => Phase::Refractory,
            other => panic!("unknown phase '{other}'"),
        };
        assert_eq!(got, want, "at tick {i}");
        if i + 1 < phases.len() {
            f.step();
        }
    }
}

#[test]
fn the_origin_travels_not_the_last_hop() {
    let doc = load();
    let c = case(&doc, "relay_cases", "the_origin_travels_not_the_last_hop");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    f.stimulate(c["stimulate"].as_str().unwrap(), 1.0).unwrap();

    f.step();
    let r2 = f.step();
    let r3 = f.step();

    for (node, report) in [("b", &r2), ("c", &r3)] {
        let want = &c["expect"][node];
        let fired = report.fired.iter().find(|x| x.node == node).expect("fired");
        assert_eq!(fired.pulse.origin, want["origin"].as_str().unwrap());
        assert_eq!(
            fired.pulse.hops,
            want["hops"].as_u64().unwrap() as usize,
            "hops is distance from the ORIGIN, not from the neighbour that handed it over"
        );
    }
}

#[test]
fn an_external_stimulus_goes_through_the_same_threshold_as_a_relayed_one() {
    let doc = load();
    let c = case(&doc, "relay_cases", "an_external_stimulus_goes_through_the_same_threshold_as_a_relayed_one");
    let spec = SignalSpec::new(c["theta_fire"].as_f64().unwrap(), 2).unwrap();
    let mut f = Field::new(spec).with_node("a");

    f.stimulate("a", c["stimulate_strength"].as_f64().unwrap()).unwrap();
    let r = f.step();

    assert!(r.fired.is_empty(), "no privileged way in — the threshold belongs to the node");
    assert!(matches!(r.refused_because("a"), Some(RefusalReason::BelowThreshold { .. })));
}

// ── ★ the load-bearing half ─────────────────────────────────────────────────

#[test]
fn a_relayed_pulse_cannot_re_excite_its_sender() {
    let doc = load();
    let c = case(&doc, "refractory_cases", "a_relayed_pulse_cannot_re_excite_its_sender");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    let origin = c["stimulate"].as_str().unwrap();
    f.stimulate(origin, 1.0).unwrap();

    let mut report = f.step();
    while report.tick < c["expect"]["at_tick"].as_u64().unwrap() as usize {
        report = f.step();
    }

    let node = c["expect"]["node"].as_str().unwrap();
    assert_eq!(
        report.refused_because(node),
        Some(&RefusalReason::Refractory),
        "★ the relayed message reached its sender over a symmetric edge and could \
         not re-excite it — observed, not inferred from a missing second excitation"
    );
    assert_eq!(report.fired_at(node), c["expect"]["fired"].as_bool().unwrap());
}

#[test]
fn the_wave_is_directional_despite_symmetric_edges() {
    let doc = load();
    let c = case(&doc, "refractory_cases", "the_wave_is_directional_despite_symmetric_edges");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    f.stimulate(c["stimulate"].as_str().unwrap(), 1.0).unwrap();

    let trace = f.run(50).expect("settles");
    let want: Vec<&str> =
        c["expect"]["fire_order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();

    assert_eq!(
        fire_order(&trace),
        want,
        "★ outward only, over a topology in which nothing prefers that direction"
    );
}

#[test]
fn the_field_falls_quiet_rather_than_ringing_forever() {
    let doc = load();
    let c = case(&doc, "refractory_cases", "the_field_falls_quiet_rather_than_ringing_forever");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    f.stimulate(c["stimulate"].as_str().unwrap(), 1.0).unwrap();

    let trace = f.run(50).expect("an excitable medium terminates");
    assert!(trace.last().unwrap().is_quiet(), "termination is why a spine can rest on this");
}

#[test]
fn two_wavefronts_annihilate_where_they_meet() {
    let doc = load();
    let c = case(&doc, "refractory_cases", "two_wavefronts_annihilate_where_they_meet");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    f.stimulate(c["stimulate"].as_str().unwrap(), 1.0).unwrap();

    let trace = f.run(50).expect("settles");

    for (node, tick) in c["expect"]["fire_tick"].as_object().unwrap() {
        let got = trace.iter().find(|r| r.fired_at(node)).map(|r| r.tick);
        assert_eq!(got, Some(tick.as_u64().unwrap() as usize), "{node} fired at the wrong tick");
    }

    // The precision: late refusals are inside the excitation cycle (EXCITED at
    // a head-on meeting, REFRACTORY behind a wavefront) — never BelowThreshold,
    // which would mean the wave simply ran out of graph.
    let late: Vec<&RefusalReason> =
        trace.iter().filter(|r| r.tick > 3).flat_map(|r| r.refused.iter()).map(|r| &r.reason).collect();
    assert!(!late.is_empty(), "the wavefronts did meet");
    assert!(
        late.iter().all(|r| matches!(r, RefusalReason::Excited | RefusalReason::Refractory)),
        "stopped inside the excitation cycle, not for want of drive"
    );
    assert!(
        c["expect"]["no_late_below_threshold"].as_bool().unwrap()
            && !late.iter().any(|r| matches!(r, RefusalReason::BelowThreshold { .. }))
    );
}

// ── ★ the proof of value ────────────────────────────────────────────────────

#[test]
fn one_origin_recruits_the_whole_field_with_no_broadcaster() {
    let doc = load();
    let c = case(&doc, "proof_of_value_cases", "one_origin_recruits_the_whole_field_with_no_broadcaster");
    let mut f = field_of(&doc, c["topology"].as_str().unwrap(), spec_of(&doc));
    let origin = c["stimulate"].as_str().unwrap();
    let e = &c["expect"];

    assert_eq!(
        f.neighbours(origin).unwrap().len(),
        e["origin_neighbours"].as_u64().unwrap() as usize
    );
    assert_eq!(f.node_count(), e["node_count"].as_u64().unwrap() as usize);
    assert!(
        f.neighbours(origin).unwrap().len() < f.node_count(),
        "★ the origin does not know the field — there is no broadcaster anywhere"
    );

    f.stimulate(origin, 1.0).unwrap();
    let trace = f.run(50).expect("settles");

    let fired: BTreeSet<&str> = fire_order(&trace).into_iter().collect();
    let all: BTreeSet<&str> = f.nodes().collect();
    assert_eq!(fired, all, "★ every node fired, recruited from one origin");

    for x in trace.iter().flat_map(|r| r.fired.iter()) {
        assert_eq!(x.pulse.origin, e["every_pulse_carries_origin"].as_str().unwrap());
    }

    assert_eq!(
        fire_order(&trace).len(),
        e["total_excitations"].as_u64().unwrap() as usize,
        "★ one excitation each — recruited AND terminating, which is the whole primitive"
    );
}

// ── spatial summation ───────────────────────────────────────────────────────

#[test]
fn a_threshold_reads_as_a_count_of_agreeing_neighbours() {
    let doc = load();

    let c = case(&doc, "summation_cases", "a_higher_threshold_needs_more_than_one_neighbour_to_agree");
    let theta = c["theta_fire"].as_f64().unwrap();
    let mut both = Field::new(SignalSpec::new(theta, 2).unwrap())
        .with_node("l")
        .with_node("r")
        .with_node("mid")
        .linking("l", "mid")
        .linking("r", "mid");
    both.stimulate("l", theta).unwrap();
    both.stimulate("r", theta).unwrap();
    let r1 = both.step();
    assert!(r1.fired_at("l") && r1.fired_at("r"));
    assert_eq!(both.step().fired_at("mid"), c["expect"]["mid_fires"].as_bool().unwrap());

    let c = case(&doc, "summation_cases", "one_neighbour_alone_does_not_clear_a_two_neighbour_threshold");
    let theta = c["theta_fire"].as_f64().unwrap();
    let mut one = Field::new(SignalSpec::new(theta, 2).unwrap())
        .with_node("l")
        .with_node("mid")
        .linking("l", "mid");
    one.stimulate("l", theta).unwrap();
    one.step();
    let r2 = one.step();
    assert_eq!(r2.fired_at("mid"), c["expect"]["mid_fires"].as_bool().unwrap());
    assert!(matches!(r2.refused_because("mid"), Some(RefusalReason::BelowThreshold { .. })));
}

#[test]
fn theta_fire_is_not_inert_across_different_thresholds() {
    let doc = load();
    let c = case(&doc, "summation_cases", "theta_fire_is_not_inert_across_different_thresholds");

    // The regression test for a real flaw this slice shipped and then caught:
    // emitting θ_fire itself makes ONE neighbour sufficient at EVERY threshold,
    // leaving θ_fire declared, validated, and completely inert. Every
    // single-threshold test still passed. Only two thresholds on the same
    // topology reveal it.
    let reach = |theta: f64| {
        let mut f = Field::new(SignalSpec::new(theta, 2).unwrap())
            .with_node("l")
            .with_node("mid")
            .linking("l", "mid");
        f.stimulate("l", theta).unwrap();
        f.step();
        f.step().fired_at("mid")
    };

    assert!(c["expect"]["one_neighbour_clears_theta_1"].as_bool().unwrap());
    assert!(reach(1.0), "one neighbour clears θ=1");

    assert!(c["expect"]["one_neighbour_does_not_clear_theta_2"].as_bool().unwrap());
    assert!(!reach(2.0), "one neighbour must NOT clear θ=2 — otherwise θ_fire is inert");
}

// ── honesty ─────────────────────────────────────────────────────────────────

#[test]
fn a_field_that_does_not_settle_says_so() {
    let doc = load();
    let c = case(&doc, "honesty_cases", "a_field_that_does_not_settle_says_so");
    let mut f = field_of(&doc, "chain", spec_of(&doc));
    f.stimulate("a", 1.0).unwrap();

    let max = c["max_ticks"].as_u64().unwrap() as usize;
    let err = f.run(max).unwrap_err();
    assert!(
        matches!(err, SignalError::DidNotSettle { max_ticks } if max_ticks == max),
        "a partial trace must not read like a finished one; got {err:?}"
    );
}

#[test]
fn silence_is_not_a_refusal() {
    let doc = load();
    let c = case(&doc, "honesty_cases", "silence_is_not_a_refusal");
    let mut f = field_of(&doc, "chain", spec_of(&doc));
    assert_eq!(f.step().is_quiet(), c["expect"]["quiet"].as_bool().unwrap());
}

// ── the divergence stays documented ─────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];

    assert_eq!(d["kind"], "rust-ahead-of-python");

    // All four greppable false positives named — the sharpest being `pulse`,
    // whose five hits are every one the substring in "imPULSE spending".
    let fp = &d["the_four_greppable_false_positives_named_so_nobody_re_derives_them"];
    assert!(fp["pulse (5 hits)"].as_str().unwrap().contains("imPULSE"));
    assert!(fp["relay (4 hits)"].as_str().unwrap().contains("Phase 2"));
    assert!(fp["propagat (2 hits)"].as_str().unwrap().contains("EXCEPTION"));
    assert!(fp["quorum (1 hit)"].as_str().unwrap().contains("Mock"));

    // The counterweight: EventBus is real, and the distinction is precise.
    let cw = d["what_python_does_have_and_why_it_is_not_this"].as_str().unwrap();
    assert!(cw.contains("EventBus is real"), "the counterweight names it");
    assert!(
        cw.contains("WHO SUBSCRIBED") && cw.contains("WHO IS REACHABLE"),
        "and states the distinction rather than waving at it"
    );

    // The gap is not a bug in Python — it does not need a refractory period
    // because it does not relay.
    let n = d["the_reference_does_not_need_a_refractory_period_and_that_is_the_point"]
        .as_str()
        .unwrap();
    assert!(
        n.contains("not yet having the problem"),
        "one engine not yet having the problem, not two solving it differently"
    );

    // Every case explains what it pins.
    for group in [
        "spec_cases",
        "relay_cases",
        "refractory_cases",
        "proof_of_value_cases",
        "summation_cases",
        "honesty_cases",
    ] {
        for c in doc[group].as_array().unwrap() {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
        }
    }

    // Unique names, so `case()` can never silently match the wrong one.
    let mut seen = BTreeSet::new();
    for group in [
        "spec_cases",
        "relay_cases",
        "refractory_cases",
        "proof_of_value_cases",
        "summation_cases",
        "honesty_cases",
    ] {
        for c in doc[group].as_array().unwrap() {
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
}
