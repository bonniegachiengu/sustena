//! EWMA + CUSUM replayed against their conformance vectors
//! (R2 · Monitor §V, §VI · MON-5, MON-6, MON-11 · R2_BACKLOG #23).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no time series at all —
//! its urgency is a scalar computed fresh per render — so there is nothing for a
//! detector to run over. `detect.json` records that term by term, keeps the
//! counterweight (the reference *does* have a real attention budget; budgeting
//! *what* to show is a different question from deciding *whether* anything
//! changed), and pins the one place this implementation deliberately departs
//! from the article's pseudocode.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    classify, Cusum, CusumSpec, DetectorError, Ewma, Severity, Shift, Watch, CONFORMANCE_VERSION,
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

fn series_of(v: &Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}

fn spec_of(v: &Value) -> CusumSpec {
    CusumSpec::new(
        v["mu_0"].as_f64().unwrap(),
        v["delta"].as_f64().unwrap(),
        v["h"].as_f64().unwrap(),
    )
}

fn severity_of(s: &str) -> Severity {
    match s {
        "INFO" => Severity::Info,
        "WARNING" => Severity::Warning,
        "CRITICAL" => Severity::Critical,
        other => panic!("unknown severity in vector: {other}"),
    }
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("detect.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));

    let terms = &doc["divergence"]["term_by_term"];
    for t in [
        "a W series to detect over",
        "EWMA (§V)",
        "CUSUM (§VI)",
        "the Monitor→Controller boundary",
    ] {
        assert!(terms[t].is_string(), "the '{t}' term must stay accounted for");
    }
    // The counterweight: the reference's attention budget is real, and is a
    // different question. Dropping this would overstate the gap.
    assert!(
        doc["divergence"]["what_python_does_have_and_why_it_is_not_this"]
            .as_str().unwrap().contains("knapsack"),
        "the attention-budget-is-real note must stay"
    );
    // And the departure from the article, recorded rather than silently made.
    assert!(
        doc["divergence"]["an_article_defect_levelled_up_not_copied"]
            .as_str().unwrap().contains("classify()"),
        "the pseudocode-ordering note must stay"
    );
}

#[test]
fn ewma_vectors() {
    let doc = load("detect.json");
    for case in doc["ewma_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();

        // Bounds on α.
        if let Some(rejected) = case["rejected"].as_array() {
            for a in rejected {
                assert!(Ewma::new(a.as_f64().unwrap()).is_err(),
                        "{name}: α = {a} must be refused");
            }
            for a in case["accepted"].as_array().unwrap() {
                assert!(Ewma::new(a.as_f64().unwrap()).is_ok(), "{name}: α = {a}");
            }
            continue;
        }

        // Half-life, as the article's own worked values.
        if let Some(hl) = case["half_lives"].as_array() {
            for row in hl {
                let e = Ewma::new(row["alpha"].as_f64().unwrap()).unwrap();
                let want = row["approx"].as_f64().unwrap();
                assert!((e.half_life() - want).abs() < 0.05,
                        "{name}: α = {} → t½ {} want ≈ {want}", row["alpha"], e.half_life());
            }
            continue;
        }

        // Two declared α over the same series.
        if let Some(compare) = case["compare"].as_array() {
            let series = series_of(&case["series"]);
            let levels: Vec<f64> = compare.iter().map(|c| {
                let mut e = Ewma::new(c["alpha"].as_f64().unwrap()).unwrap();
                series.iter().map(|y| e.update(*y)).last().unwrap()
            }).collect();
            assert!(levels[0] > levels[1],
                    "{name}: a larger α must respond faster ({levels:?})");
            continue;
        }

        let mut e = Ewma::new(case["alpha"].as_f64().unwrap()).unwrap();
        let got: Vec<f64> = series_of(&case["series"]).iter().map(|y| e.update(*y)).collect();
        let want = series_of(&case["expect"]["levels"]);
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            assert!((g - w).abs() < 1e-9, "{name}: level {i} = {g}, want {w}");
        }
    }
}

#[test]
fn cusum_vectors() {
    let doc = load("detect.json");
    for case in doc["cusum_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let series = series_of(&case["series"]);

        // Two declared thresholds over the same series.
        if let Some(compare) = case["compare"].as_array() {
            for c in compare {
                let mut d = Cusum::new(spec_of(&c["spec"])).unwrap();
                let fired = series.iter().any(|y| d.update(*y).is_some());
                assert_eq!(fired, c["fires"].as_bool().unwrap(),
                           "{name}: h = {} should {}fire", c["spec"]["h"],
                           if c["fires"].as_bool().unwrap() { "" } else { "not " });
            }
            continue;
        }

        let mut d = Cusum::new(spec_of(&case["spec"])).unwrap();
        let mut fired_at: Option<usize> = None;
        let mut shift: Option<Shift> = None;
        for (i, y) in series.iter().enumerate() {
            if let Some(a) = d.update(*y) {
                fired_at = Some(i + 1);
                shift = Some(a.shift);
                break;
            }
        }

        let expect = &case["expect"];
        match expect["alerts_at_step"].as_u64() {
            Some(step) => {
                assert_eq!(fired_at, Some(step as usize), "{name}: fired at the wrong step");
                assert_eq!(
                    shift.unwrap().as_str(),
                    expect["shift"].as_str().unwrap(),
                    "{name}: wrong direction"
                );
            }
            None => {
                assert_eq!(fired_at, None,
                           "{name}: this series must NOT alert — that is the whole point");
                if let Some(s) = expect["s_pos_after"].as_f64() {
                    assert!((d.s_pos() - s).abs() < 1e-9,
                            "{name}: S⁺ = {}, want {s}", d.s_pos());
                }
            }
        }
    }
}

/// The proof of value, stated as its own test because it is the reason this
/// detector and not a threshold: a sustained SMALL drift alerts, while a single
/// MUCH LARGER spike that corrects does not.
#[test]
fn persistent_drift_alerts_where_a_bigger_spike_does_not() {
    let doc = load("detect.json");
    let cases = doc["cusum_cases"].as_array().unwrap();
    let drift = cases.iter().find(|c| c["name"] == "a_small_persistent_drift_alerts").unwrap();
    let spike = cases.iter()
        .find(|c| c["name"] == "a_single_much_larger_spike_that_corrects_does_not")
        .unwrap();

    let drift_series = series_of(&drift["series"]);
    let spike_series = series_of(&spike["series"]);
    assert!(
        spike_series[0] > drift_series[0] * 4.0,
        "the spike must be MUCH larger for this comparison to mean anything"
    );

    let mut a = Cusum::new(spec_of(&drift["spec"])).unwrap();
    let mut b = Cusum::new(spec_of(&spike["spec"])).unwrap();
    assert!(drift_series.iter().any(|y| a.update(*y).is_some()),
            "5% over for ten days is seen");
    assert!(spike_series.iter().all(|y| b.update(*y).is_none()),
            "a 50% spike that immediately corrects is not");
}

#[test]
fn severity_vectors() {
    let doc = load("detect.json");
    for case in doc["severity_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();

        if let Some(table) = case["table"].as_array() {
            for row in table {
                let got = classify(row["excess"].as_f64().unwrap());
                assert_eq!(got, severity_of(row["severity"].as_str().unwrap()),
                           "{name}: excess {}", row["excess"]);
            }
            continue;
        }

        // The boundary itself.
        if let Some(esc) = case["escalates"].as_array() {
            for s in esc {
                assert!(severity_of(s.as_str().unwrap()).escalates(),
                        "{name}: {s} must reach the Controller");
            }
            for s in case["stays"].as_array().unwrap() {
                assert!(!severity_of(s.as_str().unwrap()).escalates(),
                        "{name}: {s} stays in the Monitor");
            }
            continue;
        }

        // The article's pseudocode-ordering defect, pinned.
        let mut d = Cusum::new(spec_of(&case["spec"])).unwrap();
        let series = series_of(&case["series"]);
        let alert = series.iter().find_map(|y| d.update(*y))
            .unwrap_or_else(|| panic!("{name}: expected an alert"));
        let expect = &case["expect"];

        assert!((alert.excess - expect["excess"].as_f64().unwrap()).abs() < 1e-9,
                "{name}: the excess must be captured BEFORE the reset");
        assert_eq!(alert.severity, severity_of(expect["severity"].as_str().unwrap()), "{name}");
        assert_eq!(alert.severity.escalates(), expect["escalates"].as_bool().unwrap(),
                   "{name}: classifying after the reset would have made this silent");
        assert!((d.s_pos() - expect["s_pos_after"].as_f64().unwrap()).abs() < 1e-9,
                "{name}: the reset itself is kept — only its ORDER changed");
    }
}

#[test]
fn declaration_vectors() {
    let doc = load("detect.json");
    for case in doc["declaration_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = spec_of(&case["spec"]).typecheck();
        let expect = &case["expect"];

        if expect["well_formed"].as_bool().unwrap() {
            assert_eq!(got, Ok(()), "{name}");
        } else {
            let e = got.expect_err(&format!("{name}: must be a load-time finding"));
            let kind = match e {
                DetectorError::BadAlpha(_) => "BadAlpha",
                DetectorError::BadThreshold(_) => "BadThreshold",
                DetectorError::BadDelta(_) => "BadDelta",
                DetectorError::BadMean(_) => "BadMean",
            };
            assert_eq!(kind, expect["error"].as_str().unwrap(), "{name}");
        }
    }
}

#[test]
fn pipeline_vectors() {
    let doc = load("detect.json");
    for case in doc["pipeline_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut w = Watch::new(
            Ewma::new(case["alpha"].as_f64().unwrap()).unwrap(),
            Cusum::new(spec_of(&case["spec"])).unwrap(),
        );
        let readings = w.observe_all(&series_of(&case["series"]));
        let expect = &case["expect"];

        // The raw W is carried alongside the smoothed level: both are wanted,
        // and they are genuinely different quantities.
        if let Some(raw) = expect["raw"].as_array() {
            let want_raw = series_of(&json!(raw));
            let want_smooth = series_of(&expect["smoothed"]);
            for (i, r) in readings.iter().enumerate() {
                assert!((r.w - want_raw[i]).abs() < 1e-9, "{name}: raw {i}");
                assert!((r.smoothed - want_smooth[i]).abs() < 1e-9, "{name}: smoothed {i}");
            }
            continue;
        }

        let any = readings.iter().any(|r| r.alert.is_some());
        assert_eq!(any, expect["any_alert"].as_bool().unwrap(), "{name}");

        if let Some(no_esc) = expect["any_escalation"].as_bool() {
            assert_eq!(readings.iter().any(|r| r.escalates()), no_esc, "{name}");
        }
        if let Some(after) = expect["first_alert_after_step"].as_u64() {
            let first = readings.iter().position(|r| r.alert.is_some())
                .unwrap_or_else(|| panic!("{name}: expected an alert"));
            assert!(first >= after as usize,
                    "{name}: alerting on the first sample would be a threshold, not a CUSUM");
        }
    }
}
