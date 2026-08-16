//! Controller decision-math replayed against its conformance vectors
//! (R2 · Controller §II, §IV, §VII + Additions · CTL-2, CTL-4, CTL-7).
//!
//! **SPEC vectors, Rust-only.** The article's own Sustena Note says it: *"the
//! current code ships neither the table nor the four functions."* Re-confirmed by
//! grep. `controller.json` records that term by term, keeps the counterweight
//! (the reference's execute + rollback + council are the *mechanism* of
//! human-in-the-loop governance, and real), and names the one place this
//! implementation deliberately departs from §II.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    compute_urgency, is_stable_intervention, route, should_rollback, AutomationTable, ControlEvent,
    ControllerError, Interval, Preferences, Region, Routing, SheridanLevel, CONFORMANCE_VERSION,
};

fn load(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "{name} was written for a different contract version"
    );
    doc
}

/// The fixture region: a non-negative balance.
fn region_with(relations: &Value) -> Region {
    let mut r = Region::new()
        .bounding(Interval::at_least("finances.liquid.balance", 0.0))
        .weighing("finances.liquid.balance", 1.0);
    for rel in relations.as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        r = r.relating(rel.as_str().unwrap());
    }
    r
}

fn state(balance: f64) -> Value {
    json!({"finances": {"liquid": {"balance": balance}}})
}

fn level_of(s: &str) -> SheridanLevel {
    match s {
        "OffersNoAssistance" => SheridanLevel::OffersNoAssistance,
        "SuggestsOne" => SheridanLevel::SuggestsOne,
        "ExecuteIfApproved" => SheridanLevel::ExecuteIfApproved,
        "ExecuteAndNotify" => SheridanLevel::ExecuteAndNotify,
        "FullAutomation" => SheridanLevel::FullAutomation,
        other => panic!("unknown Sheridan level in vector: {other}"),
    }
}

fn table() -> AutomationTable {
    AutomationTable::new()
        .declaring("ROUTINE", SheridanLevel::ExecuteAndNotify)
        .declaring("FINANCIAL", SheridanLevel::ExecuteIfApproved)
        .declaring("CRITICAL", SheridanLevel::ExecuteIfApproved)
        .declaring("IOT_WRITE", SheridanLevel::ExecuteIfApproved)
        .declaring("ROLLBACK", SheridanLevel::ExecuteIfApproved)
}

fn routing_name(r: &Routing) -> &'static str {
    match r {
        Routing::Surface(_) => "surface",
        Routing::Automate { .. } => "automate",
        Routing::Withhold { .. } => "withhold",
    }
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("controller.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));

    let terms = &doc["divergence"]["term_by_term"];
    for t in [
        "is_stable_intervention (§II)",
        "AUTOMATION_LEVEL (§IV)",
        "should_surface / SNR (§VII)",
        "urgency as Lyapunov-derived (§VII)",
    ] {
        assert!(terms[t].is_string(), "the '{t}' term must stay accounted for");
    }
    // The counterweight: execute + rollback + council are real. Dropping this
    // would say the reference has no human-in-the-loop governance at all, which
    // is untrue — it has the mechanism and lacks the decision-math.
    assert!(
        doc["divergence"]["what_python_does_have_and_why_it_is_not_this"]
            .as_str().unwrap().contains("control.execute_approved"),
        "the mechanism-is-real note must stay"
    );
    // And the departure from §II, recorded rather than made silently.
    assert!(
        doc["divergence"]["one_deliberate_departure_from_the_article"]
            .as_str().unwrap().contains("d(s,V)"),
        "the region-not-a-point note must stay"
    );
}

#[test]
fn stability_vectors() {
    let doc = load("controller.json");
    for case in doc["stability_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_with(&case["relations"]);
        let s = is_stable_intervention(
            &r,
            &state(case["before"].as_f64().unwrap()),
            &state(case["after"].as_f64().unwrap()),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let expect = &case["expect"];

        if let Some(want) = expect["stable"].as_bool() {
            assert_eq!(s.stable, want, "{name}: W({}) vs W({})", s.after, s.before);
        }
        for (key, got) in [("w_before", s.before), ("w_after", s.after),
                           ("improvement", s.improvement())] {
            if let Some(w) = expect[key].as_f64() {
                assert!((got - w).abs() < 1e-9, "{name}: {key} = {got}, want {w}");
            }
        }
        if let Some(want) = expect["rollback"].as_bool() {
            assert_eq!(should_rollback(&s), want,
                       "{name}: the rollback panel's purpose is derived, not asserted");
        }
        if let Some(want) = expect["exact"].as_bool() {
            assert_eq!(s.exact, want, "{name}: a lower-bound comparison must be reported");
        }
    }
}

#[test]
fn sheridan_vectors() {
    let doc = load("controller.json");
    for case in doc["sheridan_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();

        if let Some(yes) = case["requires_approval"].as_array() {
            for l in yes {
                assert!(level_of(l.as_str().unwrap()).requires_approval(), "{name}: {l}");
            }
            for l in case["does_not"].as_array().unwrap() {
                assert!(!level_of(l.as_str().unwrap()).requires_approval(), "{name}: {l}");
            }
            continue;
        }

        if let Some(t) = case["table"].as_object() {
            let declared = table();
            for (action, level) in t {
                assert_eq!(declared.level_for(action), level_of(level.as_str().unwrap()),
                           "{name}: {action}");
            }
            continue;
        }

        // The conservative default.
        let unknown = case["unknown_type"].as_str().unwrap();
        let declared = table();
        let expect = &case["expect"];
        assert_eq!(declared.level_for(unknown), level_of(expect["level"].as_str().unwrap()),
                   "{name}: forgetting a declaration must not automate something");
        assert_eq!(declared.level_for(unknown).requires_approval(),
                   expect["requires_approval"].as_bool().unwrap(), "{name}");
        if expect["reported_undeclared"].as_bool() == Some(true) {
            assert_eq!(declared.undeclared(&["ROUTINE", unknown]), [unknown.to_string()], "{name}");
        }
    }
}

#[test]
fn urgency_vectors() {
    let doc = load("controller.json");
    for case in doc["urgency_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_with(&case["relations"]);
        let expect = &case["expect"];

        // Distance comparison.
        if let Some(compare) = case["compare"].as_array() {
            let p = Preferences::default();
            let vals: Vec<f64> = compare.iter().map(|c| {
                compute_urgency(&r, &state(c["balance"].as_f64().unwrap()),
                                &ControlEvent::new("FINANCIAL", "x"), &p, 0).unwrap().value
            }).collect();
            assert!(vals[1] > vals[0], "{name}: urgency must rise with distance from V");
            continue;
        }

        // Declared base weights.
        if let Some(weights) = case["weights"].as_object() {
            let mut p = Preferences::default();
            for (t, w) in weights {
                p = p.weighing(t, w.as_f64().unwrap());
            }
            for (t, want) in expect.as_object().unwrap() {
                let e = ControlEvent::new("A", "x").typed(t);
                let u = compute_urgency(&r, &state(case["balance"].as_f64().unwrap()), &e, &p, 0).unwrap();
                assert!((u.value - want.as_f64().unwrap()).abs() < 1e-9, "{name}: {t}");
            }
            continue;
        }

        // Deadlines.
        if let Some(deadlines) = case["compare_deadlines"].as_array() {
            let p = Preferences::default();
            let now = case["now"].as_u64().unwrap();
            let vals: Vec<_> = deadlines.iter().map(|d| {
                let e = ControlEvent::new("FINANCIAL", "x").due_at(d.as_u64().unwrap());
                compute_urgency(&r, &state(case["balance"].as_f64().unwrap()), &e, &p, now).unwrap()
            }).collect();
            assert!(vals[0].value > vals[1].value, "{name}: a nearer deadline presses harder");
            assert!((vals[0].time_pressure - expect["nearest_time_pressure"].as_f64().unwrap()).abs() < 1e-9,
                    "{name}");
            continue;
        }

        let mut e = ControlEvent::new("FINANCIAL", "x");
        if let Some(d) = case["deadline"].as_u64() {
            e = e.due_at(d);
        }
        let now = case["now"].as_u64().unwrap_or(0);
        let got = compute_urgency(&r, &state(case["balance"].as_f64().unwrap()),
                                  &e, &Preferences::default(), now);

        if let Some(want) = expect["error"].as_str() {
            let err = got.expect_err(&format!("{name}: expected a refusal"));
            assert!(matches!(err, ControllerError::DeadlinePassed { .. }),
                    "{name}: expected {want}, got {err}");
            continue;
        }

        let u = got.unwrap_or_else(|e| panic!("{name}: {e}"));
        for (key, val) in [("urgency", u.value), ("time_pressure", u.time_pressure)] {
            if let Some(w) = expect[key].as_f64() {
                assert!((val - w).abs() < 1e-9, "{name}: {key} = {val}, want {w}");
            }
        }
        if let Some(want) = expect["lyapunov_exact"].as_bool() {
            assert_eq!(u.lyapunov_exact, want,
                       "{name}: an under-estimate must not quietly rank low");
        }
    }
}

#[test]
fn routing_vectors() {
    let doc = load("controller.json");
    for case in doc["routing_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let r = region_with(&Value::Null);
        let e = ControlEvent::new(case["action_type"].as_str().unwrap(), "budget.spend");
        let before = state(case["balance"].as_f64().unwrap());
        let after = case["candidate_after"].as_f64().map(state);
        let expect = &case["expect"];

        // Two declared thresholds over the same event.
        if let Some(thresholds) = case["compare_thresholds"].as_array() {
            let surfaces: Vec<bool> = thresholds.iter().map(|t| {
                let p = Preferences::default().with_threshold(t.as_f64().unwrap());
                route(&r, &before, None, &e, &table(), &p, 0).unwrap().surfaces()
            }).collect();
            assert_eq!(surfaces[0], expect["first_surfaces"].as_bool().unwrap(), "{name}");
            assert_eq!(surfaces[1], expect["second_surfaces"].as_bool().unwrap(), "{name}");
            continue;
        }

        let p = Preferences::default().with_threshold(case["threshold"].as_f64().unwrap());
        let routed = route(&r, &before, after.as_ref(), &e, &table(), &p, 0).unwrap();

        assert_eq!(routing_name(&routed), expect["routing"].as_str().unwrap(),
                   "{name}: wrong route ({routed:?})");
        if let Some(want) = expect["interrupts_a_human"].as_bool() {
            assert_eq!(routed.interrupts_a_human(), want, "{name}");
        }

        match &routed {
            Routing::Automate { level } => {
                if let Some(want) = expect["level"].as_str() {
                    assert_eq!(*level, level_of(want), "{name}");
                }
            }
            Routing::Withhold { urgency, threshold } => {
                if expect["urgency_below_threshold"].as_bool() == Some(true) {
                    assert!(urgency.value < *threshold, "{name}");
                }
                // The distinction that matters most: withheld is not automated.
                assert!(!routed.surfaces(), "{name}: nobody is interrupted");
            }
            Routing::Surface(d) => {
                if let Some(want) = expect["vouched_stable"].as_bool() {
                    assert_eq!(d.vouched_stable(), want, "{name}");
                }
                if expect["has_stability"].as_bool() == Some(false) {
                    assert!(d.stability.is_none(),
                            "{name}: 'not assessed' is its own state, and not a yes");
                }
                if expect["rollback"].as_bool() == Some(true) {
                    assert!(should_rollback(d.stability.as_ref().unwrap()), "{name}");
                }
            }
        }
    }
}

#[test]
fn declaration_vectors() {
    let doc = load("controller.json");
    for case in doc["declaration_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();

        if let Some(rows) = case["cases"].as_array() {
            for row in rows {
                let mut p = Preferences::default().with_threshold(row["threshold"].as_f64().unwrap());
                if let Some(w) = row["bad_weight"].as_f64() {
                    p = p.weighing("x", w);
                }
                let got = p.typecheck();
                if row["well_formed"].as_bool().unwrap() {
                    assert_eq!(got, Ok(()), "{name}");
                } else {
                    let e = got.expect_err(&format!("{name}: must be a load-time finding"));
                    let kind = match e {
                        ControllerError::BadThreshold(_) => "BadThreshold",
                        ControllerError::BadWeight { .. } => "BadWeight",
                        other => panic!("{name}: unexpected {other}"),
                    };
                    assert_eq!(kind, row["error"].as_str().unwrap(), "{name}");
                }
            }
            continue;
        }

        // A missing weight must not make something invisible.
        let p = Preferences::default();
        let expect = &case["expect"];
        assert_eq!(p.weight_for("NEVER_DECLARED"), expect["weight"].as_f64().unwrap(), "{name}");
        let u = compute_urgency(&region_with(&Value::Null),
                                &state(case["balance"].as_f64().unwrap()),
                                &ControlEvent::new("X", "x"), &p, 0).unwrap();
        assert!((u.value - expect["urgency"].as_f64().unwrap()).abs() < 1e-9,
                "{name}: defaulting to 0.0 would silently suppress every unweighted event");
    }
}

/// **The slice's point**: Monitor → Controller → human → gate, using only what
/// earlier slices built.
#[test]
fn the_loop_closes_through_a_human() {
    use sustena_core::{
        execute_admitted, Authorization, Cusum, CusumSpec, EffectClass, Enforcement, Ewma,
        NonceLedger, ProposalStatus, Registry, Simulated, Watch,
    };

    let doc = load("controller.json");
    let case = &doc["loop_case"];
    let name = case["name"].as_str().unwrap();
    let expect = &case["expect"];

    // ── Monitor ─────────────────────────────────────────────────────────────
    let m = &case["monitor"];
    let mut watch = Watch::new(
        Ewma::new(m["alpha"].as_f64().unwrap()).unwrap(),
        Cusum::new(CusumSpec::new(
            m["cusum"]["mu_0"].as_f64().unwrap(),
            m["cusum"]["delta"].as_f64().unwrap(),
            m["cusum"]["h"].as_f64().unwrap(),
        ))
        .unwrap(),
    );
    let series = vec![m["series_value"].as_f64().unwrap(); m["series_len"].as_u64().unwrap() as usize];
    let readings = watch.observe_all(&series);
    assert_eq!(readings.iter().any(|r| r.alert.is_some()),
               expect["monitor_alerts"].as_bool().unwrap(),
               "{name}: the Monitor must see the drift");

    // ── Controller ──────────────────────────────────────────────────────────
    let c = &case["controller"];
    let params: Map<String, Value> = [
        ("pocket_name".to_string(), json!("food")),
        ("amount".to_string(), json!(250.0)),
        ("period".to_string(), json!("monthly")),
    ]
    .into_iter()
    .collect();
    let mut event = ControlEvent::new(
        c["action_type"].as_str().unwrap(),
        c["operator"].as_str().unwrap(),
    );
    event.params = params.clone();

    let prefs = Preferences::default().with_threshold(c["threshold"].as_f64().unwrap());
    let routed = route(
        &region_with(&Value::Null),
        &state(c["before"].as_f64().unwrap()),
        Some(&state(c["after"].as_f64().unwrap())),
        &event, &table(), &prefs, 0,
    )
    .unwrap();
    assert_eq!(routing_name(&routed), expect["routing"].as_str().unwrap(), "{name}");
    let Routing::Surface(decision) = routed else { panic!("{name}: must reach a person") };
    assert_eq!(decision.vouched_stable(), expect["vouched_stable"].as_bool().unwrap(), "{name}");
    assert_eq!(decision.binding().operator(), expect["binding_operator"].as_str().unwrap(), "{name}");

    // ── the human — the ONLY step that mints a token ────────────────────────
    let token = Simulated::from_sandbox("ctl-1", decision.binding(), Ok(()))
        .expect("the stability check IS the sandbox run")
        .voted(ProposalStatus::Passed)
        .expect("passed")
        .approve("bonnie", 1, 100);

    // ── the gate ────────────────────────────────────────────────────────────
    let registry = Registry::default();
    let allowed = registry.names();
    let live = json!({"finances": {"liquid": {"balance": 1000.0}, "pockets": {},
                                   "income": {"monthly_total": 0.0, "sources": []}}});
    let mut nonces = NonceLedger::new();
    let ex = execute_admitted(
        &registry, &allowed, &Enforcement::default(), &live, "budget.allocate", &params,
        &Authorization::Unchecked, &EffectClass::Live { token: &token, now: 50 }, &mut nonces,
    );
    assert_eq!(ex.committed(), expect["gate_commits"].as_bool().unwrap(),
               "{name}: {:?}", ex.result.reason);

    let replay = execute_admitted(
        &registry, &allowed, &Enforcement::default(), &ex.state, "budget.allocate", &params,
        &Authorization::Unchecked, &EffectClass::Live { token: &token, now: 50 }, &mut nonces,
    );
    assert_eq!(!replay.committed(), expect["replay_refused"].as_bool().unwrap(),
               "{name}: single-use must hold across the whole loop");
}
