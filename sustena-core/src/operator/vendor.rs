//! Who you paid, and what you usually call it — `vendor.*`.
//!
//! ★★★ **Three operators, not one, because they are three different kinds of
//! act.** Identifying a vendor reads a string. Remembering what a vendor's
//! spending is for changes what the household knows. Suggesting reads that
//! knowledge back. Folding them into one call would make a lookup and a write
//! indistinguishable to anything composing them, and composition is the point:
//!
//! ```text
//! vendor.identify ──► vendor.suggest ──► budget.spend ──► vendor.remember
//!                                   └──► (no memory: ask a person)
//! ```
//!
//! That is a finance operative's shape — a DAG whose nodes are operator calls
//! — rather than a function someone wired into a screen.
//!
//! ★★ `identify` and `suggest` are **read-only**: no mutations, no events,
//! zero pawa. They are operators anyway, because a node in a DAG has to be one.
//! A capability that cannot appear in a graph cannot be part of an operative,
//! and "everything is an operator" is what makes the graph expressible at all.
//!
//! ★★★ **The memory is state, so it folds.** The same knowledge lived in a
//! side JSON file next to the log, which meant a rebuild could not reproduce
//! it and no other Sustain could ever read it. Here it is an ordinary
//! dimension: written through the gate, replayed by the fold, visible to
//! composition and roll-up like any other thing the household knows.

use serde_json::{json, Map, Value};

use super::meta::{OperatorMeta, OperatorResult, ParamDecl, Protocol, Registry};
use super::EmittedEvent;
use crate::flow::Movement;
use crate::state::State;

fn text(params: &Map<String, Value>, key: &str) -> String {
    params.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// The stable name for a merchant, from however a bank wrote it.
///
/// ★★ Deterministic and boring on purpose. Banks print the same shop as
/// `NAIVAS SUPERMARKET`, `Naivas Supermarket LTD` and `NAIVAS  SUPERMARKET`,
/// and a memory keyed on the raw string learns each spelling separately —
/// which is the same as learning nothing.
///
/// ★★★ It does NOT try to be clever. Stripping company suffixes and folding
/// case is safe because it cannot merge two different shops; fuzzy matching
/// could, and a merged vendor sends the next payment to the wrong pocket.
pub fn vendor_key(raw: &str) -> String {
    const NOISE: &[&str] = &["ltd", "limited", "co", "company", "plc", "inc", "kenya", "ke"];
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect();
    let words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| !NOISE.contains(w))
        // ★ A bare number is a till or an account, not a name. Keeping it
        //   would key the same shop differently per branch.
        .filter(|w| !w.chars().all(|c| c.is_ascii_digit()))
        .collect();
    words.join("_")
}

// ── vendor.identify ──────────────────────────────────────────────────────────

/// Read a vendor out of a transaction. Changes nothing.
fn identify(
    _state: &mut State,
    params: &Map<String, Value>,
    _events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let raw = {
        let c = text(params, "counterparty");
        if c.is_empty() { text(params, "text") } else { c }
    };
    let key = vendor_key(&raw);
    if key.is_empty() {
        // ★★ No vendor is a real answer, not a failure. A withdrawal or a fee
        //    has nobody on the other side, and refusing would break any graph
        //    that runs this before knowing which it had.
        return OperatorResult::ok(json!({"vendor": Value::Null, "from": raw}));
    }
    OperatorResult::ok(json!({"vendor": key, "from": raw}))
}

// ── vendor.suggest ───────────────────────────────────────────────────────────

/// What this vendor's spending has been for before. Changes nothing.
fn suggest(
    state: &mut State,
    params: &Map<String, Value>,
    _events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let vendor = {
        let v = text(params, "vendor");
        if v.is_empty() { vendor_key(&text(params, "counterparty")) } else { v }
    };
    if vendor.is_empty() {
        return OperatorResult::ok(json!({"pocket": Value::Null, "why": "no vendor to go on"}));
    }

    let known = state.get(&format!("vendors.{vendor}")).cloned();
    let Some(record) = known else {
        return OperatorResult::ok(json!({
            "vendor": vendor,
            "pocket": Value::Null,
            "why": "nothing filed against this one yet",
        }));
    };

    let pocket = record.get("pocket").and_then(Value::as_str).unwrap_or("").to_string();
    let times = record.get("times").and_then(Value::as_u64).unwrap_or(0);

    // ★★★ A suggestion is only as good as the pocket still existing. One that
    //     was renamed or removed would be offered forever and refused every
    //     time, which teaches a person the suggestions are wrong.
    if pocket.is_empty() || !state.exists(&format!("finances.pockets.{pocket}")) {
        return OperatorResult::ok(json!({
            "vendor": vendor,
            "pocket": Value::Null,
            "why": "the pocket it used to go to is gone",
        }));
    }

    OperatorResult::ok(json!({
        "vendor": vendor,
        "pocket": pocket,
        "times": times,
        "why": format!("filed to {pocket} {times} time{} before", if times == 1 { "" } else { "s" }),
    }))
}

// ── vendor.remember ──────────────────────────────────────────────────────────

/// Record what this vendor's spending is for.
///
/// ★★ Last answer wins, and the count keeps rising. A person who re-files a
/// vendor has changed their mind, and the newest decision is the one to offer
/// next; the count is how confident the offer may sound, not a vote.
fn remember(
    state: &mut State,
    params: &Map<String, Value>,
    events: &mut Vec<EmittedEvent>,
    _movements: &mut Vec<Movement>,
) -> OperatorResult {
    let vendor = {
        let v = text(params, "vendor");
        if v.is_empty() { vendor_key(&text(params, "counterparty")) } else { v }
    };
    if vendor.is_empty() {
        return OperatorResult::fail("There is no vendor here to remember.", "vendor_named");
    }
    let pocket = text(params, "pocket_name");
    if pocket.is_empty() {
        return OperatorResult::fail("Remember it as what?", "pocket_named");
    }
    if !state.exists(&format!("finances.pockets.{pocket}")) {
        return OperatorResult::fail(
            format!("Pocket '{pocket}' does not exist."),
            "pocket_exists",
        );
    }

    let path = format!("vendors.{vendor}");
    let times = state
        .get(&path)
        .and_then(|r| r.get("times"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    if state.set(&path, json!({"pocket": pocket, "times": times})).is_err() {
        return OperatorResult::fail(
            format!("'{vendor}' is not a usable name to file under."),
            "vendor_name_valid",
        );
    }

    events.push(EmittedEvent {
        name: "event.vendor.remembered".into(),
        payload: json!({"vendor": vendor, "pocket": pocket, "times": times}),
    });

    OperatorResult::ok(json!({"vendor": vendor, "pocket": pocket, "times": times}))
}

pub fn register(registry: &mut Registry) {
    registry.register(OperatorMeta {
        name: "vendor.identify",
        description: "Read a stable vendor name out of a transaction.",
        params: vec![
            ParamDecl::text("counterparty").optional(),
            ParamDecl::text("text").optional(),
        ],
        constraints: vec![],
        post_constraints: vec![],
        // ★ Nothing. A read-only operator that declared an effect would be a
        //   lie composition believes.
        side_effects: vec![],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 0,
        effect: None,
        run: identify,
    });

    registry.register(OperatorMeta {
        name: "vendor.suggest",
        description: "What this vendor's spending has been filed as before.",
        params: vec![
            ParamDecl::text("vendor").optional(),
            ParamDecl::text("counterparty").optional(),
        ],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec![],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 0,
        effect: None,
        run: suggest,
    });

    registry.register(OperatorMeta {
        name: "vendor.remember",
        description: "Record what a vendor's spending is for, so it can be offered next time.",
        params: vec![
            ParamDecl::naming("pocket_name", "finances.pockets"),
            ParamDecl::text("vendor").optional(),
            ParamDecl::text("counterparty").optional(),
        ],
        constraints: vec![],
        post_constraints: vec![],
        side_effects: vec!["event.vendor.remembered"],
        pawa_cost: 0,
        protocol: Protocol::Rpc,
        min_privilege: 1,
        effect: None,
        run: remember,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::{execute, Enforcement, Execution, Registry as Reg};

    fn household() -> Value {
        json!({"finances": {
            "liquid": {"balance": 1000.0},
            "pockets": {"food": {"allocated": 500.0, "spent": 0.0, "limit": 0.0},
                        "transport": {"allocated": 200.0, "spent": 0.0, "limit": 0.0}},
            "accounts": {}, "income": {"monthly_total": 0.0, "sources": []}}})
    }

    fn run(state: &Value, op: &str, ps: &[(&str, Value)]) -> Execution {
        let reg = Reg::default();
        let params: Map<String, Value> =
            ps.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect();
        execute(&reg, &reg.names(), &Enforcement::default(), state, op, &params)
    }

    #[test]
    fn one_shop_written_three_ways_is_one_vendor() {
        // ★★★ A memory keyed on the raw string learns each spelling
        //     separately, which is the same as learning nothing.
        let a = vendor_key("NAIVAS SUPERMARKET");
        assert_eq!(a, vendor_key("Naivas Supermarket LTD"));
        assert_eq!(a, vendor_key("naivas  supermarket"));
        assert_eq!(a, "naivas_supermarket");
    }

    #[test]
    fn two_different_shops_stay_different() {
        // ★★★ The failure that would cost real money: a merged vendor sends
        //     the next payment to the wrong pocket. Normalising may never
        //     merge, which is why there is no fuzzy matching here.
        assert_ne!(vendor_key("NAIVAS SUPERMARKET"), vendor_key("QUICKMART SUPERMARKET"));
        assert_ne!(vendor_key("JAVA HOUSE"), vendor_key("JAVA COFFEE"));
    }

    #[test]
    fn a_till_number_does_not_split_a_shop_by_branch() {
        assert_eq!(vendor_key("NAIVAS 0912"), vendor_key("NAIVAS 4471"));
    }

    #[test]
    fn identifying_changes_nothing_at_all() {
        // ★★ Read-only in the strong sense: no mutations, no events, and the
        //    state it hands back is the state it was given.
        let before = household();
        let ex = run(&before, "vendor.identify", &[("counterparty", json!("NAIVAS SUPERMARKET"))]);
        assert!(ex.committed());
        assert_eq!(ex.result.data["vendor"], json!("naivas_supermarket"));
        assert_eq!(ex.state, before);
        assert!(ex.mutations.is_empty());
        assert!(ex.events.is_empty());
    }

    #[test]
    fn a_transaction_with_nobody_on_the_other_side_is_answered_not_refused() {
        // ★★ A withdrawal or a fee has no vendor. Refusing would break any
        //    graph that runs identify before knowing which it had.
        let ex = run(&household(), "vendor.identify", &[("counterparty", json!("  "))]);
        assert!(ex.committed());
        assert_eq!(ex.result.data["vendor"], Value::Null);
    }

    #[test]
    fn an_unknown_vendor_suggests_nothing_and_says_why() {
        let ex = run(&household(), "vendor.suggest", &[("counterparty", json!("NEW SHOP"))]);
        assert!(ex.committed());
        assert_eq!(ex.result.data["pocket"], Value::Null);
        assert!(ex.result.data["why"].as_str().unwrap().contains("nothing filed"));
    }

    #[test]
    fn what_was_remembered_is_what_gets_suggested() {
        // ★★★ The loop, end to end: file it once, and the next one is offered.
        let after = run(
            &household(),
            "vendor.remember",
            &[("counterparty", json!("NAIVAS SUPERMARKET")), ("pocket_name", json!("food"))],
        );
        assert!(after.committed(), "{:?}", after.result.reason);

        // The same words, however cased and whatever company suffix: found.
        let same = run(&after.state, "vendor.suggest",
                       &[("counterparty", json!("Naivas Supermarket LTD"))]);
        assert_eq!(same.result.data["pocket"], json!("food"));
        assert_eq!(same.result.data["times"], json!(1));

        // ★★★ And the tradeoff, stated rather than hidden: DROPPING a word
        //     makes a different vendor. "Naivas Ltd" is not
        //     "Naivas Supermarket", so it is learned separately.
        //
        //     That is the conservative side of the only choice that matters
        //     here. Matching loosely enough to join them would also be loose
        //     enough to join two real shops, and a merged vendor sends the
        //     next payment to the wrong pocket. Being asked twice about one
        //     shop costs a tap; being asked nothing and filed wrongly costs
        //     the truth of the ledger.
        let shorter = run(&after.state, "vendor.suggest", &[("counterparty", json!("Naivas Ltd"))]);
        assert_eq!(shorter.result.data["pocket"], Value::Null, "a different word set is a different vendor");
    }

    #[test]
    fn remembering_again_raises_the_count_and_the_newest_answer_wins() {
        // ★★ Re-filing a vendor is someone changing their mind, and the newest
        //    decision is the one to offer next. The count is how confident the
        //    offer may sound, not a vote.
        let a = run(&household(), "vendor.remember",
                    &[("vendor", json!("java")), ("pocket_name", json!("food"))]);
        let b = run(&a.state, "vendor.remember",
                    &[("vendor", json!("java")), ("pocket_name", json!("transport"))]);
        let s = run(&b.state, "vendor.suggest", &[("vendor", json!("java"))]);
        assert_eq!(s.result.data["pocket"], json!("transport"), "the newest answer");
        assert_eq!(s.result.data["times"], json!(2), "and it has been filed twice");
    }

    #[test]
    fn a_suggestion_for_a_pocket_that_is_gone_is_withheld() {
        // ★★★ Offered forever and refused every time is how a person learns
        //     the suggestions are wrong.
        let remembered = run(&household(), "vendor.remember",
                             &[("vendor", json!("java")), ("pocket_name", json!("food"))]);
        let mut without = remembered.state.clone();
        without
            .pointer_mut("/finances/pockets")
            .and_then(Value::as_object_mut)
            .unwrap()
            .remove("food");

        let s = run(&without, "vendor.suggest", &[("vendor", json!("java"))]);
        assert_eq!(s.result.data["pocket"], Value::Null);
        assert!(s.result.data["why"].as_str().unwrap().contains("gone"));
    }

    #[test]
    fn remembering_a_pocket_that_does_not_exist_is_refused() {
        let ex = run(&household(), "vendor.remember",
                     &[("vendor", json!("java")), ("pocket_name", json!("ghost"))]);
        assert!(!ex.committed());
        assert_eq!(ex.result.constraint_violated.as_deref(), Some("pocket_exists"));
    }

    #[test]
    fn remembering_nothing_in_particular_is_refused() {
        let no_vendor = run(&household(), "vendor.remember", &[("pocket_name", json!("food"))]);
        assert!(!no_vendor.committed());
        assert_eq!(no_vendor.result.constraint_violated.as_deref(), Some("vendor_named"));

        let no_pocket = run(&household(), "vendor.remember", &[("vendor", json!("java"))]);
        assert!(!no_pocket.committed());
        assert_eq!(no_pocket.result.constraint_violated.as_deref(), Some("pocket_named"));
    }

    #[test]
    fn the_memory_is_state_so_it_can_be_folded_and_read_by_anything() {
        // ★★★ The deviation this closes. The same knowledge lived in a side
        //     JSON file beside the log, so a rebuild could not reproduce it and
        //     no other Sustain could ever read it. Here it is an ordinary
        //     dimension.
        let ex = run(&household(), "vendor.remember",
                     &[("vendor", json!("java")), ("pocket_name", json!("food"))]);
        assert_eq!(ex.state.pointer("/vendors/java/pocket"), Some(&json!("food")));
        assert!(!ex.mutations.is_empty(), "a real mutation, so the fold replays it");
        assert_eq!(ex.events.len(), 1, "and a real event");
    }
}
