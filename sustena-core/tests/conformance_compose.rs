//! Checked composition replayed against its conformance vectors
//! (R2 · Operator §IV · R2_BACKLOG #16).
//!
//! **SPEC vectors, Rust-only.** The reference engine has no compose-time check:
//! it chains Enzymes dynamically, so an illegal chain is discovered by running
//! it. `compose.json` records that, together with the counterweight worth not
//! erasing — `OperativeGraph` *does* validate at load time that every node names
//! a registered operator, which checks that a step **exists**, not that it can
//! **follow** the one before it.
//!
//! See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    compose, entails, wp, Change, EffectSummary, Entailment, Pathway, PathwayError, Step, WpResult,
    CONFORMANCE_VERSION,
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

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn verdict_name(v: &Entailment) -> &'static str {
    match v {
        Entailment::Entailed => "entailed",
        Entailment::Refuted { .. } => "refuted",
        Entailment::Undecided { .. } => "undecided",
    }
}

fn effect_of(v: &Value) -> EffectSummary {
    let mut e = EffectSummary::new();
    for entry in v.as_array().unwrap() {
        let pair = entry.as_array().unwrap();
        let path = pair[0].as_str().unwrap();
        let change = match &pair[1] {
            Value::String(s) if s == "opaque" => Change::Opaque,
            obj if obj.get("shift_by").is_some() => {
                Change::ShiftBy(obj["shift_by"].as_f64().unwrap())
            }
            obj if obj.get("set_to").is_some() => Change::SetTo(obj["set_to"].clone()),
            other => panic!("unknown change in vector: {other}"),
        };
        e = e.with(path, change);
    }
    e
}

/// Build a `Step` from a vector's step spec.
fn step_of(v: &Value) -> Step {
    let mut s = Step::new(v["name"].as_str().unwrap());
    for g in strings(&v["guard"]) {
        s = s.guarded_by(&g);
    }
    for p in strings(&v["post"]) {
        s = s.ensures(&p);
    }
    for e in strings(&v["emits"]) {
        s = s.emitting(&e);
    }
    if let Some(c) = v["cost"].as_u64() {
        s = s.costing(c as u32);
    }
    if let Some(effect) = v.get("effect") {
        s = s.doing(effect_of(effect));
    }
    s
}

#[test]
fn the_divergence_stays_recorded() {
    let doc = load("compose.json");
    assert_eq!(doc["divergence"]["kind"], "rust-ahead-of-python");
    assert_eq!(doc["replayed_by"], json!(["rust"]));

    // Both counterweights must stay: what Python DOES check, and the fact that
    // the undecided region is shared rather than a Rust shortfall. Dropping
    // either would turn an honest claim into an overstated one.
    let d = &doc["divergence"];
    assert!(
        d["what_python_does_have_and_why_it_is_not_this"]
            .as_str()
            .expect("the OperativeGraph-validates-existence note must stay")
            .contains("OPERATOR_REGISTRY")
    );
    assert!(
        d["the_undecided_region_is_shared_not_a_gap"]
            .as_str()
            .expect("the undecidability note must stay")
            .contains("Church")
    );
}

#[test]
fn entailment_vectors() {
    let doc = load("compose.json");
    for group in ["entailment_cases", "fragment_boundary_cases"] {
        for case in doc[group].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let got = entails(&strings(&case["post"]), &strings(&case["guard"]));
            assert_eq!(
                verdict_name(&got),
                case["expect"]["verdict"].as_str().unwrap(),
                "{name}: wrong verdict ({got:?})"
            );
            if let (Some(needle), Entailment::Refuted { detail }) =
                (case["expect"]["detail_contains"].as_str(), &got)
            {
                assert!(detail.contains(needle), "{name}: expected '{needle}' in {detail}");
            }
        }
    }
}

#[test]
fn wp_vectors() {
    let doc = load("compose.json");
    for case in doc["wp_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let effect = effect_of(&case["effect"]);
        let got = wp(&effect, case["guard"].as_str().unwrap());

        match case["expect"]["result"].as_str().unwrap() {
            "always" => assert_eq!(got, Ok(WpResult::Always), "{name}"),
            "never" => assert_eq!(got, Ok(WpResult::Never), "{name}"),
            "condition" => assert_eq!(
                got,
                Ok(WpResult::Condition(case["expect"]["expr"].as_str().unwrap().to_string())),
                "{name}"
            ),
            "error" => assert!(got.is_err(), "{name}: wp must report, not guess ({got:?})"),
            other => panic!("{name}: unknown expected result {other}"),
        }
    }
}

#[test]
fn compose_vectors() {
    let doc = load("compose.json");
    for case in doc["compose_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let a = step_of(&case["a"]);
        let b = step_of(&case["b"]);
        let got = compose(&a, &b);
        let expect = &case["expect"];

        if !expect["composed"].as_bool().unwrap() {
            let err = got.expect_err(&format!("{name}: this chain must be rejected"));
            if let Some(from) = expect["from"].as_str() {
                assert_eq!(err.from, from, "{name}");
            }
            if let Some(to) = expect["to"].as_str() {
                assert_eq!(err.to, to, "{name}");
            }
            if let Some(needle) = expect["detail_contains"].as_str() {
                assert!(err.detail.contains(needle), "{name}: {}", err.detail);
            }
            continue;
        }

        let c = got.unwrap_or_else(|e| panic!("{name}: should compose, got {e}"));
        if let Some(n) = expect["name"].as_str() {
            assert_eq!(c.name, n, "{name}");
        }
        if let Some(e) = expect.get("emits").filter(|v| v.is_array()) {
            assert_eq!(c.emits, strings(e), "{name}: ε_A · ε_B, in order");
        }
        if let Some(cost) = expect["cost"].as_u64() {
            assert_eq!(c.pawa_cost, cost as u32, "{name}: cost is the sum of the parts");
        }
        if let Some(p) = expect.get("post").filter(|v| v.is_array()) {
            assert_eq!(c.post, strings(p), "{name}: post(A;B) = post(B)");
        }
        if let Some(want) = expect["runtime_guard_required"].as_bool() {
            assert_eq!(
                c.runtime_guard_required, want,
                "{name}: runtime_guard_required ({})", c.runtime_guard_reason
            );
        }
        if expect["reason_nonempty"].as_bool() == Some(true) {
            assert!(!c.runtime_guard_reason.is_empty(), "{name}: it must say WHY");
        }
        if let Some(needle) = expect["reason_contains"].as_str() {
            assert!(
                c.runtime_guard_reason.contains(needle),
                "{name}: {}", c.runtime_guard_reason
            );
        }
        if let Some(needle) = expect["guard_contains"].as_str() {
            assert!(
                c.guard.iter().any(|g| g == needle),
                "{name}: expected '{needle}' in {:?}", c.guard
            );
        }
    }
}

#[test]
fn algebra_vectors() {
    let doc = load("compose.json");
    for case in doc["algebra_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let steps: Vec<Step> = case["steps"].as_array().unwrap().iter().map(step_of).collect();
        assert_eq!(steps.len(), 3, "{name}: associativity needs three");

        // (A;B);C
        let ab = compose(&steps[0], &steps[1]).unwrap();
        let ab_c = compose(&ab.as_step(), &steps[2]).unwrap();
        // A;(B;C)
        let bc = compose(&steps[1], &steps[2]).unwrap();
        let a_bc = compose(&steps[0], &bc.as_step()).unwrap();

        assert_eq!(ab_c.emits, a_bc.emits, "{name}: E* concatenation is associative");
        assert_eq!(ab_c.pawa_cost, a_bc.pawa_cost, "{name}: cost agrees under either bracketing");
        assert_eq!(ab_c.post, a_bc.post, "{name}: the composite ends where the last step ends");

        let expect = &case["expect"];
        assert_eq!(ab_c.emits, strings(&expect["emits"]), "{name}");
        assert_eq!(ab_c.pawa_cost, expect["cost"].as_u64().unwrap() as u32, "{name}");
    }
}

#[test]
fn pathway_vectors() {
    let doc = load("compose.json");
    for case in doc["pathway_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let steps: Vec<Step> = case["steps"].as_array().unwrap().iter().map(step_of).collect();
        let got = Pathway::try_chain(&steps);
        let expect = &case["expect"];

        if !expect["saved"].as_bool().unwrap() {
            let err = got.err().unwrap_or_else(|| panic!("{name}: must not be saveable"));
            if expect["error"].as_str() == Some("Empty") {
                assert_eq!(err, PathwayError::Empty, "{name}");
            }
            continue;
        }

        let p = got.unwrap_or_else(|e| panic!("{name}: should save, got {e}"));
        if let Some(want) = expect.get("steps").filter(|v| v.is_array()) {
            assert_eq!(p.steps(), strings(want).as_slice(), "{name}");
        }
        if let Some(want) = expect.get("emits").filter(|v| v.is_array()) {
            assert_eq!(p.composed().emits, strings(want), "{name}");
        }
        if let Some(cost) = expect["cost"].as_u64() {
            assert_eq!(p.composed().pawa_cost, cost as u32, "{name}");
        }
    }
}
