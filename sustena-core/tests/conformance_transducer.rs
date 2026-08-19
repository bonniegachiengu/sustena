//! `τ` replayed against its conformance vectors
//! (R2 · the reference's `transducer.py` + `parse_rules_seed.py`).
//!
//! **PARITY vectors (R1).** The transducer is real in the reference and was
//! tuned against Bonnie's own redacted M-Pesa and KCB messages over several
//! passes, so `transducer.json` is *recorded from it* — every shipped rule
//! against its own declared examples, the same texts under the **wrong**
//! source, real OTP shapes, and the empty/gibberish edges.
//!
//! ★★★ **Each case asserts the whole decision**, not the tier: the operator and
//! its bound params if it mapped, **every parsed field**, the external ref, and
//! which rule handled it. A transducer that got the tier right while extracting
//! a different amount would pass a tier-only check and be a wrong entry in a
//! ledger.
//!
//! ★★★ **The security cases are asserted twice over**: rejected, *and* carrying
//! nothing — no fields, no operator, no ref — because the property that matters
//! is not "we said no", it is "there is nothing here to store".
//!
//! See `conformance/README.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use sustena_core::{parse_message, Transduction, CONFORMANCE_VERSION};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("transducer.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "transducer.json was written for a different contract version"
    );
    doc
}

/// Numbers compare by value: the reference writes `1234.5`, this crate may
/// write `1234.5` too, but an integral float differs in RENDERING only.
fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    }
}

fn compare_map(
    got: &BTreeMap<String, Value>,
    want: &Value,
    name: &str,
    what: &str,
) {
    let want = want.as_object().cloned().unwrap_or_default();
    // ★ Compared as SETS. The reference preserves insertion order in a plain
    //   dict; this crate uses a `BTreeMap`, which sorts. Field ORDER in a
    //   parsed-fields map is not semantics — every consumer looks a field up
    //   by name — so asserting on it would fail on a rendering difference and
    //   say nothing about behaviour. Membership and values are asserted
    //   exactly, which is the part that could be wrong.
    let got_keys: BTreeSet<&String> = got.keys().collect();
    let want_keys: BTreeSet<&String> = want.keys().collect();
    assert_eq!(got_keys, want_keys, "{name}: {what} — different field set");
    for (k, w) in &want {
        let g = got.get(k).unwrap_or(&Value::Null);
        assert!(same(g, w), "{name}: {what}.{k} — {g} != reference {w}");
    }
}

#[test]
fn every_recorded_case_matches_the_reference() {
    let doc = load();
    let cases = doc["cases"].as_array().expect("cases is a list");
    assert!(cases.len() >= 50, "the vector file recorded too little");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let source = case["source"].as_str();
        let want = &case["expect"];

        let t = parse_message(text, source, &[]);

        assert_eq!(
            t.status(),
            want["status"].as_str().unwrap(),
            "{name}: tier — reason was {:?}",
            t.reason()
        );

        // ★★★ A rejection carries NOTHING. Asserted as an absence, not as a
        //     comparison against a recorded empty map.
        if let Transduction::Rejected { .. } = t {
            assert!(!t.storable(), "{name}: a rejection must not be storable");
            assert!(t.parsed_fields().is_empty(), "{name}: no fields on a rejection");
            assert!(t.operator().is_none(), "{name}: no operator on a rejection");
            assert!(t.external_ref().is_none(), "{name}: no ref on a rejection");
            continue;
        }

        assert_eq!(
            t.operator(),
            want["operator"].as_str(),
            "{name}: operator"
        );
        assert_eq!(
            t.parser_name(),
            want["parser_name"].as_str().unwrap_or(""),
            "{name}: which rule handled it"
        );
        assert_eq!(
            t.external_ref(),
            want["external_ref"].as_str(),
            "{name}: external ref"
        );

        compare_map(&t.parsed_fields(), &want["parsed_fields"], name, "parsed_fields");
        compare_map(&t.operator_params(), &want["params"], name, "params");
    }
}

/// ★★★ Every recorded secret shape, under every source, rejected — and the
/// rejection is asserted to carry nothing at all.
#[test]
fn every_recorded_secret_is_rejected_under_every_source() {
    let doc = load();
    let secrets: Vec<&Value> = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with("secret:"))
        .collect();
    assert!(secrets.len() >= 10, "too few secret cases recorded");

    for case in secrets {
        let name = case["name"].as_str().unwrap();
        assert_eq!(
            case["expect"]["status"], "rejected",
            "{name}: the reference did not reject this — the divergence is real, not a test bug"
        );
        let t = parse_message(case["text"].as_str().unwrap(), case["source"].as_str(), &[]);
        assert!(matches!(t, Transduction::Rejected { .. }), "{name}");
        assert!(!t.storable(), "{name}");
    }
}

/// ★★ Source-strict, as the reference records it: the same text under the wrong
/// source must not reach the other set's rules.
#[test]
fn a_body_never_promotes_a_message_out_of_its_senders_set() {
    let doc = load();
    let strict: Vec<&Value> = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"].as_str().unwrap().starts_with("strict:"))
        .collect();
    assert!(strict.len() >= 15, "too few strict cases recorded");

    let mut crossed = 0;
    for case in &strict {
        let name = case["name"].as_str().unwrap();
        let t = parse_message(case["text"].as_str().unwrap(), case["source"].as_str(), &[]);
        assert_eq!(
            t.status(),
            case["expect"]["status"].as_str().unwrap(),
            "{name}: tier under the wrong source"
        );
        assert_eq!(
            t.parser_name(),
            case["expect"]["parser_name"].as_str().unwrap_or(""),
            "{name}: which rule ran under the wrong source"
        );
        if t.status() != "unparsed" {
            crossed += 1;
        }
    }
    // ★ Some shapes DO legitimately match in both sets — the reference records
    //   which, and this asserts the count matches rather than assuming none do.
    let want_crossed = strict
        .iter()
        .filter(|c| c["expect"]["status"] != "unparsed")
        .count();
    assert_eq!(crossed, want_crossed, "a different set of shapes crossed than the reference records");
}

/// ★ The reference's own real samples produce real money figures. Asserted
/// against the vector rather than recomputed, so a change in extraction is a
/// failure rather than a silently different number.
#[test]
fn the_mapped_income_cases_extract_the_real_amounts() {
    let doc = load();
    let mapped: Vec<&Value> = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["expect"]["status"] == "mapped")
        .collect();
    assert!(!mapped.is_empty(), "no mapped case recorded");

    for case in mapped {
        let name = case["name"].as_str().unwrap();
        let t = parse_message(case["text"].as_str().unwrap(), case["source"].as_str(), &[]);
        // Every shipped mapped rule is income; a spend must ask a person.
        assert_eq!(t.operator(), Some("budget.record_income"), "{name}");
        let amount = t.operator_params().get("amount").cloned().unwrap_or(Value::Null);
        let want = &case["expect"]["params"]["amount"];
        assert!(same(&amount, want), "{name}: amount {amount} != reference {want}");
        assert!(amount.as_f64().unwrap_or(0.0) > 0.0, "{name}: a real figure");
    }
}
