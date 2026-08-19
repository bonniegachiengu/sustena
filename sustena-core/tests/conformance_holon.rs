//! Cross-holon transfer replayed against its conformance vectors
//! (R2 · Composition §4E.6 · the reference's `holon.transfer`).
//!
//! **PARITY vectors (R1).** `_execute_holon_transfer` is real in the reference,
//! so `holon.json` is *recorded from it* — driven against a live in-memory
//! `SustainEngine` with two genuinely linked habitats.
//!
//! ★★★ Each case asserts **three** things, and the second is the one that
//! matters: the verdict, **both sides' state afterwards**, and conservation. A
//! transfer that got the verdict right while leaving one side moved would pass
//! a verdict-only check and be precisely the failure this slice exists to rule
//! out — money that left one household and arrived nowhere.
//!
//! ★ **One recorded divergence, deliberate and stricter here.** The reference
//! conserves to six decimal places (`round(total, 6)`); this core conserves in
//! **integer minor units** via `Tolerance::Exact`, so a fractional amount is
//! refused rather than moved. `fractional_amount` records the reference's
//! `ok`, and the test below asserts the Rust refusal **explicitly** rather than
//! matching the vector — a divergence that passed silently would not be
//! recorded, it would be hidden. See `conformance/README.md`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use sustena_core::{
    holon_transfer, Enforcement, Link, Linked, Moving, Party, Transfer, CONFORMANCE_VERSION,
};

const LIQUID: &str = "finances.liquid.balance";
const PARENT: &str = "parent";
const CHILD: &str = "child";

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("holon.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "holon.json was written for a different contract version"
    );
    doc
}

fn state(n: f64) -> Value {
    json!({ "finances": { "liquid": { "balance": n } } })
}

fn party(id: &str, n: f64) -> Party {
    Party::new(id, state(n), Enforcement::default())
}

fn balance(v: &Value) -> f64 {
    v["finances"]["liquid"]["balance"].as_f64().expect("numeric")
}

/// The two endpoints for a case, in the direction the vector names.
fn endpoints(direction: &str) -> (&'static str, &'static str) {
    match direction {
        "down" => (PARENT, CHILD),
        "self" => (CHILD, CHILD),
        _ => (CHILD, PARENT),
    }
}

#[test]
fn every_recorded_case_matches_the_reference() {
    let doc = load();
    let cases = doc["cases"].as_array().expect("cases is a list");
    assert!(!cases.is_empty(), "the vector file recorded nothing");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        // The one deliberate divergence has its own test below.
        if name == "fractional_amount" {
            continue;
        }

        let from_balance = case["from_balance"].as_f64().unwrap();
        let to_balance = case["to_balance"].as_f64().unwrap();
        let amount = case["amount"].as_f64().unwrap();
        let (from_id, to_id) = endpoints(case["direction"].as_str().unwrap());
        let links: Vec<Link> = if case["linked"].as_bool().unwrap() {
            vec![Link::new(CHILD, PARENT)]
        } else {
            vec![]
        };

        let want = &case["expect"];
        let from = party(from_id, from_balance);
        let to = party(to_id, to_balance);

        // ★★ The link token IS the link check. When the reference refuses with
        //    `holon_link_exists` or `distinct_holons`, this core does not reach
        //    `transfer` at all — the token does not exist. That is the same
        //    refusal moved from a runtime branch to the type, and the assertion
        //    below is that the reference agrees there was nothing to do.
        let Some(link) = Linked::between(from_id, to_id, &links) else {
            assert_eq!(want["status"], "failed", "{name}: the reference committed it");
            let rule = want["constraint_violated"].as_str().unwrap();
            assert!(
                rule == "holon_link_exists" || rule == "distinct_holons",
                "{name}: no token, but the reference refused for '{rule}'"
            );
            assert_eq!(
                want["from_after"], want["from_before"],
                "{name}: the reference moved something anyway"
            );
            continue;
        };

        let t = holon_transfer(&link, &from, &to, &Moving::money(LIQUID), amount);

        match (&t, want["status"].as_str().unwrap()) {
            (Transfer::Committed(s), "ok") => {
                let (f, c) = s.legs();
                assert_eq!(
                    f.balance_after(),
                    want["from_after"].as_f64().unwrap(),
                    "{name}: sender's balance after"
                );
                assert_eq!(
                    c.balance_after(),
                    want["to_after"].as_f64().unwrap(),
                    "{name}: receiver's balance after"
                );
                // ★★★ Conservation, as an equation over the two REAL states —
                //     not the settlement restating itself.
                let after = balance(f.state_after()) + balance(c.state_after());
                let before = from_balance + to_balance;
                assert_eq!(after, before, "{name}: Σ money must be identical");
                assert_eq!(
                    before,
                    want["total_before"].as_f64().unwrap(),
                    "{name}: the reference started somewhere else"
                );
                assert!(s.conserved());
            }
            (Transfer::Refused { rule, .. }, "failed") => {
                assert_eq!(
                    rule,
                    want["constraint_violated"].as_str().unwrap(),
                    "{name}: refused for a different reason"
                );
                // ★★★ NOTHING MOVED. A refusal produces no leg at all, so the
                //     only states in existence are the originals — asserted
                //     against the reference's own recorded after-values.
                assert!(t.settlement().is_none());
                assert_eq!(
                    balance(&from.state),
                    want["from_after"].as_f64().unwrap(),
                    "{name}: the sender must be byte-identical to before"
                );
                assert_eq!(
                    balance(&to.state),
                    want["to_after"].as_f64().unwrap(),
                    "{name}: the receiver must be byte-identical to before"
                );
            }
            (got, expected) => panic!("{name}: reference said {expected}, this core said {got:?}"),
        }
    }
}

/// ★ The recorded divergence, asserted rather than skipped.
#[test]
fn a_fractional_amount_commits_in_python_and_is_refused_here() {
    let doc = load();
    let case = doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "fractional_amount")
        .expect("recorded");

    assert_eq!(
        case["expect"]["status"], "ok",
        "the reference is expected to allow a fractional move — if it stopped, \
         this divergence is stale and conformance/README.md needs revisiting"
    );

    let link = Linked::between(CHILD, PARENT, &[Link::new(CHILD, PARENT)]).unwrap();
    let t = holon_transfer(
        &link,
        &party(CHILD, 100.0),
        &party(PARENT, 0.0),
        &Moving::money(LIQUID),
        0.5,
    );
    match &t {
        Transfer::Refused { rule, .. } => assert_eq!(rule, "conservation"),
        Transfer::Committed(_) => {
            panic!("exact conservation must refuse a fractional move")
        }
    }

    // ★★ And the same move IS legal when the caller declares a tolerance out
    //    loud — the strictness is about undeclared precision, not about
    //    forbidding continuous quantities.
    let named = holon_transfer(
        &link,
        &party(CHILD, 100.0),
        &party(PARENT, 0.0),
        &Moving::real(LIQUID, 1e-9),
        0.5,
    );
    assert!(named.committed());
}

/// ★★★ The failure path, explicitly: the SECOND leg refuses, and the first is
/// untouched. The reference has no shipped template whose invariant a credit
/// can break, so there is no vector to record — it is proven here directly,
/// which is the case the whole slice exists for.
#[test]
fn a_refused_second_leg_leaves_the_first_untouched() {
    let link = Linked::between(CHILD, PARENT, &[Link::new(CHILD, PARENT)]).unwrap();
    let sender = party(CHILD, 500.0);
    let receiver = Party::new(
        PARENT,
        state(900.0),
        Enforcement {
            enabled: true,
            invariants: vec![("ceiling".into(), "finances.liquid.balance <= 1000".into())],
            ..Default::default()
        },
    );

    let t = holon_transfer(&link, &sender, &receiver, &Moving::money(LIQUID), 300.0);

    match &t {
        Transfer::Refused { rule, reason } => {
            assert_eq!(rule, "enforcement_gate");
            assert!(reason.contains(PARENT), "the refusal names the side: {reason}");
        }
        Transfer::Committed(_) => panic!("the receiver's own ceiling must refuse this"),
    }
    assert!(t.settlement().is_none(), "no leg exists to be applied");
    assert_eq!(balance(&sender.state), 500.0, "the debit must not have happened");
    assert_eq!(balance(&receiver.state), 900.0);
    assert_eq!(
        balance(&sender.state) + balance(&receiver.state),
        1400.0,
        "Σ money is exactly what it was"
    );
}
