//! **The operatives themselves** — Mentor and Attaché, as graphs.
//!
//! ```text
//! Mentor (finance)
//!   identify ─► suggest ─┬─(a pocket)────► spend ─► remember
//!                        └─(nothing known)─► (stops: a person decides)
//!
//! Attaché (people and vendors)
//!   identify ─► suggest ─┬─(nothing known)─► remember ─► link
//!                        └─(already known)─► (stops: nothing to learn)
//! ```
//!
//! ★★★ **These are data, and that is the entire point.** Both shapes already
//! existed — as the order of calls inside a screen. An order written in a host
//! function cannot be inspected, cannot be checked before it runs, and cannot
//! be changed without editing the app. Written as [`Dag`]s they are checked
//! when they are *written* (every path through [`Dag::typecheck`]), and running
//! one is a single call that goes through the same gate as a tap.
//!
//! ★★ **Two operatives, because they answer different questions.** Mentor asks
//! "where does this money belong"; Attaché asks "who is this, and should the
//! household know them". They share `identify` and `suggest` and diverge on the
//! *same* condition read opposite ways — which is how you can tell they are
//! genuinely two jobs rather than one job split for tidiness.
//!
//! ## What is honestly missing
//!
//! ★★★ **Graduation is not here, because the operator for it does not exist.**
//! §4 of the money model has a virtual person pocket graduating into a real
//! child Sustain; that needs `holon.create_child`, which this engine does not
//! yet provide (the Python reference does). Attaché therefore takes a
//! counterparty as far as *linked and known* and stops. A node calling an
//! operator that is not there would be refused by `typecheck` anyway — which is
//! the check working, not a workaround to route around.
//!
//! ★★ **No parse node.** The transducer runs at the ingest boundary and is not
//! an operator, so Mentor starts from a counterparty rather than from raw text.
//! Naming a `parse` node that nothing implements would make the picture prettier
//! and the graph unrunnable.

use crate::dag::{Condition, Dag, Edge, Node};

/// The node a caller reads Mentor's decision out of, when it filed something.
pub const MENTOR_FILED: &str = "spend";
/// The node whose emptiness means Mentor could not decide alone.
pub const MENTOR_SUGGEST: &str = "suggest";

/// **Mentor — where does this money belong?**
///
/// ★★ It files only what it already knows. A vendor with no remembered pocket
/// leaves the graph at `suggest`, and the run reports `file` as unreached — the
/// honest shape of "a person has to answer this", rather than a guess that
/// would be wrong at exactly the rate the guessing was bad.
pub fn mentor() -> Dag {
    Dag::new("mentor", "identify")
        .with_node(Node::new("identify", "vendor.identify").from_input("counterparty"))
        .with_node(
            Node::new("suggest", "vendor.suggest")
                .from_node("vendor", "identify", "vendor")
                .from_input("number")
                .from_input("direction"),
        )
        // ★★★ The pocket comes from what `suggest` FOUND, not from the input.
        //     Taking it from the input would make the graph a way of doing what
        //     the caller already decided, which is not an operative at all.
        .with_node(
            Node::new("spend", "budget.spend")
                .from_node("pocket_name", "suggest", "pocket")
                .from_input("amount")
                .from_input("account")
                .from_input("description"),
        )
        // ★★ Re-filing the same vendor raises its count, which is how confident
        //    the next suggestion may sound. Running it again after a successful
        //    file is not redundant — it is the learning.
        .with_node(
            Node::new("remember", "vendor.remember")
                .from_node("vendor", "identify", "vendor")
                .from_node("pocket_name", "suggest", "pocket"),
        )
        .with_edge(Edge::new("identify", "suggest"))
        .with_edge(Edge::new("suggest", "spend").when(Condition::Present {
            node: "suggest".into(),
            field: "pocket".into(),
        }))
        .with_edge(Edge::new("spend", "remember"))
}

/// **Attaché — who is this, and should the household know them?**
///
/// ★★★ The mirror condition. Mentor acts when the household already knows a
/// counterparty; Attaché acts when it does not. One `suggest`, read both ways,
/// and neither operative needs to know the other exists.
///
/// ★★ `link` runs only when the caller supplied a whole number. It is
/// `Condition::Present` on the identify step rather than a check inside the
/// operator, so a run without a number simply ends at `remember` — a vendor the
/// household now recognises by name, which is a real and complete outcome.
pub fn attache() -> Dag {
    Dag::new("attache", "identify")
        .with_node(Node::new("identify", "vendor.identify").from_input("counterparty"))
        .with_node(
            Node::new("suggest", "vendor.suggest").from_node("vendor", "identify", "vendor"),
        )
        .with_node(
            Node::new("remember", "vendor.remember")
                .from_node("vendor", "identify", "vendor")
                .from_input("pocket_name"),
        )
        // ★★★ A number ties the pocket to a PERSON, which is the step that
        //     makes a tab two-way. It is the furthest this engine can take a
        //     counterparty toward being a real participant today.
        .with_node(
            Node::new("link", "vendor.link_number")
                .from_input("number")
                .from_input("pocket_name"),
        )
        .with_edge(Edge::new("identify", "suggest"))
        .with_edge(Edge::new("suggest", "remember").when(Condition::Absent {
            node: "suggest".into(),
            field: "pocket".into(),
        }))
        .with_edge(Edge::new("remember", "link"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{run, DagRun};
    use crate::operator::{execute, Enforcement, Registry};
    use serde_json::{json, Map, Value};

    fn household() -> Value {
        json!({"finances": {
            "liquid": {"balance": 5000.0},
            "pockets": {"food": {"allocated": 2000.0, "spent": 0.0, "limit": 0.0},
                        "Aida": {"allocated": 3000.0, "spent": 0.0, "limit": 0.0}},
            "accounts": {}, "income": {"monthly_total": 0.0, "sources": []}},
            "vendors": {}})
    }

    fn input(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    fn go(dag: &Dag, state: &Value, inp: &[(&str, Value)]) -> DagRun {
        let reg = Registry::default();
        run(dag, &reg, &reg.names(), &Enforcement::default(), state, &input(inp))
            .expect("the graph typechecks")
    }

    fn known(state: &Value, vendor: &str, pocket: &str) -> Value {
        let reg = Registry::default();
        let ex = execute(
            &reg,
            &reg.names(),
            &Enforcement::default(),
            state,
            "vendor.remember",
            &input(&[("counterparty", json!(vendor)), ("pocket_name", json!(pocket))]),
        );
        assert!(ex.committed(), "{:?}", ex.result.reason);
        ex.state
    }

    #[test]
    fn both_operatives_are_checked_before_they_ever_run() {
        // ★★★ The whole claim of the layer: a graph that could not work is
        //     refused when it is written. If either shipped shape were
        //     unrunnable, this is where it would be said.
        let reg = Registry::default();
        mentor().typecheck(&reg).expect("mentor is a runnable graph");
        attache().typecheck(&reg).expect("attaché is a runnable graph");
    }

    #[test]
    fn mentor_files_a_vendor_the_household_already_knows() {
        // ★★★ Four operator calls, in an order that is data, through the real
        //     gate — and no host function in the middle deciding anything.
        let state = known(&household(), "NAIVAS SUPERMARKET", "food");
        let after = go(
            &mentor(),
            &state,
            &[("counterparty", json!("NAIVAS SUPERMARKET LTD")), ("amount", json!(250.0))],
        );

        assert!(after.succeeded(), "{:?}", after.steps.last().map(|s| &s.result.reason));
        assert_eq!(
            after.state.pointer("/finances/pockets/food/spent").and_then(Value::as_f64),
            Some(250.0),
        );
        // And it learned from it: filed twice now, so the next offer is surer.
        assert_eq!(after.state.pointer("/vendors/naivas_supermarket/times"), Some(&json!(2)));
    }

    #[test]
    fn mentor_stops_at_a_vendor_it_has_never_seen_rather_than_guessing() {
        // ★★★ A guess here is wrong at exactly the rate the guessing is bad,
        //     and it is wrong about money. Stopping is the answer; the run says
        //     which step was never reached so a screen can ask.
        let before = household();
        let after = go(
            &mentor(),
            &before,
            &[("counterparty", json!("A SHOP NOBODY KNOWS")), ("amount", json!(250.0))],
        );

        assert!(after.succeeded(), "not deciding is not a failure");
        assert!(after.unreached.contains(&MENTOR_FILED.to_string()));
        assert_eq!(after.state, before, "nothing moved");
        assert_eq!(
            after.result_of(MENTOR_SUGGEST).unwrap().data["pocket"],
            Value::Null,
            "and it says plainly that it had nothing to go on",
        );
    }

    #[test]
    fn mentor_refuses_out_loud_when_the_pocket_cannot_take_it() {
        // ★★ The operative has no gate of its own, so an overspend is refused
        //    by the same rule that would refuse it from a screen.
        let state = known(&household(), "NAIVAS", "food");
        let after = go(
            &mentor(),
            &state,
            &[("counterparty", json!("NAIVAS")), ("amount", json!(999_999.0))],
        );
        assert!(!after.succeeded());
        assert_eq!(after.refused_at.as_deref(), Some("spend"));
        assert!(after.unreached.contains(&"remember".to_string()));
    }

    #[test]
    fn attache_learns_a_counterparty_the_household_did_not_know() {
        let after = go(
            &attache(),
            &household(),
            &[
                ("counterparty", json!("MARY NGIGI")),
                ("pocket_name", json!("Aida")),
                ("number", json!("0726123961")),
            ],
        );
        assert!(after.succeeded(), "{:?}", after.steps.last().map(|s| &s.result.reason));
        assert_eq!(after.state.pointer("/vendors/mary_ngigi/pocket"), Some(&json!("Aida")));
        // ★★★ And the number now ties that pocket to a person, which is what
        //     makes the tab run both ways.
        assert_eq!(after.state.pointer("/finances/links/726123961"), Some(&json!("Aida")));
    }

    #[test]
    fn attache_leaves_alone_a_counterparty_the_household_already_knows() {
        // ★★★ The mirror of Mentor's condition, on the same `suggest` step.
        //     Two operatives, one shared reading, opposite answers — which is
        //     how you can tell they are two jobs and not one split in half.
        let state = known(&household(), "MARY NGIGI", "Aida");
        let after = go(
            &attache(),
            &state,
            &[("counterparty", json!("MARY NGIGI")), ("pocket_name", json!("food"))],
        );
        assert!(after.succeeded());
        assert!(after.unreached.contains(&"remember".to_string()));
        assert_eq!(
            after.state.pointer("/vendors/mary_ngigi/pocket"),
            Some(&json!("Aida")),
            "it did not overwrite what was already known",
        );
    }

    #[test]
    fn the_two_operatives_never_both_act_on_the_same_counterparty() {
        // ★★★ Worth asserting rather than assuming: their conditions are exact
        //     opposites, so for any vendor exactly one of them does something.
        //     If that ever stopped being true, one of them would be quietly
        //     duplicating the other's work on real money.
        let unknown = household();
        let m1 = go(&mentor(), &unknown, &[("counterparty", json!("NEW")), ("amount", json!(10.0))]);
        let a1 = go(
            &attache(),
            &unknown,
            &[("counterparty", json!("NEW")), ("pocket_name", json!("food"))],
        );
        assert!(m1.unreached.contains(&"spend".to_string()), "mentor stood down");
        assert!(a1.result_of("remember").is_some(), "attaché acted");

        let seen = known(&unknown, "NEW", "food");
        let m2 = go(&mentor(), &seen, &[("counterparty", json!("NEW")), ("amount", json!(10.0))]);
        let a2 = go(
            &attache(),
            &seen,
            &[("counterparty", json!("NEW")), ("pocket_name", json!("food"))],
        );
        assert!(m2.result_of("spend").is_some(), "mentor acted");
        assert!(a2.unreached.contains(&"remember".to_string()), "attaché stood down");
    }
}
