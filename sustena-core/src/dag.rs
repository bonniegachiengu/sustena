//! **The operative layer: a graph of operator calls** (Operator §IV, wired).
//!
//! ```text
//! Mentor (finance):   parse ─► identify ─► suggest ─┬─(a pocket)──► spend ─► remember
//!                                                   └─(nothing)───► ask a person
//! ```
//!
//! ★★★ **This is the piece the audit named as missing.** Every deciding step
//! was already an operator — `vendor.identify`, `vendor.suggest`, `budget.spend`
//! — and each was called from a screen, in an order written in a host function.
//! An operative that lives in a screen is not an operative: it cannot be
//! inspected, cannot be checked before it runs, and cannot be replaced without
//! editing the app. Here the order is *data*, the check happens when the graph
//! is written, and running it is one call.
//!
//! ## What "checked before it runs" buys
//!
//! [`compose`](crate::compose) has held Hoare sequencing since it was written
//! and nothing ever called it:
//!
//! ```text
//!   {P} A {Q}    {Q} B {R}
//!   ──────────────────────      chainable ⟺ post(A) ⊨ guard(B)
//!        {P} A;B {R}
//! ```
//!
//! [`Dag::typecheck`] runs every root-to-leaf path through
//! [`Pathway::try_chain`]. A recipe whose third step can never fire is refused
//! **when it is written**, not discovered at 2 a.m. by watching step three
//! refuse. That was always the point of `compose.rs`; this is what wires it.
//!
//! ## Why a graph and not a list
//!
//! ★★★ A straight line cannot express the one operative we actually have.
//! Mentor's real shape branches: a vendor with a remembered pocket is filed, a
//! vendor without one goes to a person. Shipping a linear runner would have
//! meant either encoding that branch back in the host — the exact deviation
//! this closes — or pretending Mentor is something it is not.
//!
//! ★★ Conditions read a **prior node's own result**, never the state. A
//! condition over state would be a second, unchecked guard living beside the
//! operator's real one; a condition over a result is reading what the previous
//! step actually said.
//!
//! ## What this does NOT do
//!
//! ★★ No parallelism, no loops, no retry. Nodes run in dependency order, once
//! each. Money moves through here, and a runner that could run a step twice
//! because two edges reached it is a runner that can pay somebody twice.
//! [`Dag::typecheck`] refuses a cycle rather than detecting one at run time.
//!
//! ★★ Nothing here has its own gate. Every node executes through
//! [`execute`](crate::operator::execute) — the same admission, the same
//! invariants, the same double-entry check as a tap on a screen. A refusal
//! stops that branch and is reported; it is never stepped over.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use thiserror::Error;

use crate::compose::{Pathway, PathwayError, Step};
use crate::operator::{execute, Enforcement, OperatorResult, Registry};

/// Where one parameter's value comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Binding {
    /// A constant written into the graph.
    Literal(Value),
    /// A field of the input the graph was run with.
    Input(String),
    /// A field of an earlier node's result.
    ///
    /// ★★ Of its RESULT, not of state. A node that needs a number another node
    /// worked out should say so, rather than both of them reading a place in
    /// state and hoping the first one wrote there.
    From { node: String, field: String },
}

/// One operator call in the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub operator: String,
    pub bind: Vec<(String, Binding)>,
}

impl Node {
    pub fn new(id: &str, operator: &str) -> Self {
        Self { id: id.into(), operator: operator.into(), bind: Vec::new() }
    }

    pub fn with(mut self, param: &str, binding: Binding) -> Self {
        self.bind.push((param.to_string(), binding));
        self
    }

    /// Shorthand for the common case: a constant.
    pub fn literal(self, param: &str, value: Value) -> Self {
        self.with(param, Binding::Literal(value))
    }

    /// Shorthand: take it from the input.
    pub fn from_input(self, param: &str) -> Self {
        let p = param.to_string();
        self.with(&p.clone(), Binding::Input(p))
    }

    /// Shorthand: take it from an earlier node's result.
    pub fn from_node(self, param: &str, node: &str, field: &str) -> Self {
        self.with(param, Binding::From { node: node.into(), field: field.into() })
    }
}

/// When an edge is followed.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// The field is there and is not null — "there was an answer".
    Present { node: String, field: String },
    /// The field is missing or null — "there was no answer".
    Absent { node: String, field: String },
    /// The field equals a value exactly.
    Equals { node: String, field: String, value: Value },
}

impl Condition {
    fn holds(&self, results: &BTreeMap<String, Value>) -> bool {
        let read = |node: &str, field: &str| {
            results.get(node).and_then(|r| r.get(field)).filter(|v| !v.is_null()).cloned()
        };
        match self {
            Self::Present { node, field } => read(node, field).is_some(),
            Self::Absent { node, field } => read(node, field).is_none(),
            Self::Equals { node, field, value } => read(node, field).as_ref() == Some(value),
        }
    }
}

/// An edge, optionally guarded.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub when: Option<Condition>,
}

impl Edge {
    pub fn new(from: &str, to: &str) -> Self {
        Self { from: from.into(), to: to.into(), when: None }
    }

    pub fn when(mut self, condition: Condition) -> Self {
        self.when = Some(condition);
        self
    }
}

/// An operative, as data.
#[derive(Debug, Clone, PartialEq)]
pub struct Dag {
    pub name: String,
    pub entry: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Why a graph was refused before it ever ran.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DagError {
    #[error("'{0}' names no node in this graph")]
    UnknownNode(String),
    #[error("node '{node}' calls '{operator}', which no operator provides")]
    UnknownOperator { node: String, operator: String },
    #[error("two nodes are both called '{0}'")]
    DuplicateNode(String),
    #[error("node '{node}' reads '{field}' from '{reads_from}', which does not run before it")]
    OutOfOrder { node: String, reads_from: String, field: String },
    #[error("the graph loops back on itself through '{0}'")]
    Cycle(String),
    #[error("the path {path} cannot run: {reason}")]
    Unchainable { path: String, reason: String },
}

/// What one node did.
#[derive(Debug, Clone)]
pub struct NodeOutcome {
    pub node: String,
    pub operator: String,
    pub result: OperatorResult,
}

/// What a whole run did.
#[derive(Debug, Clone)]
pub struct DagRun {
    pub steps: Vec<NodeOutcome>,
    /// The state after every node that committed.
    pub state: Value,
    /// The node that refused, if one did. Everything downstream of it is
    /// unreached — and named as unreached rather than silently absent.
    pub refused_at: Option<String>,
    /// Nodes never reached, because a branch was not taken or a step refused.
    pub unreached: Vec<String>,
}

impl DagRun {
    pub fn succeeded(&self) -> bool {
        self.refused_at.is_none()
    }

    /// The result of one node, for a caller that wants a specific answer out.
    pub fn result_of(&self, node: &str) -> Option<&OperatorResult> {
        self.steps.iter().find(|s| s.node == node).map(|s| &s.result)
    }
}

impl Dag {
    pub fn new(name: &str, entry: &str) -> Self {
        Self { name: name.into(), entry: entry.into(), nodes: Vec::new(), edges: Vec::new() }
    }

    pub fn with_node(mut self, node: Node) -> Self {
        self.nodes.push(node);
        self
    }

    pub fn with_edge(mut self, edge: Edge) -> Self {
        self.edges.push(edge);
        self
    }

    fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// A topological order, or the node a cycle runs through.
    fn order(&self) -> Result<Vec<String>, DagError> {
        let mut incoming: BTreeMap<&str, usize> =
            self.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
        for e in &self.edges {
            *incoming.get_mut(e.to.as_str()).expect("checked") += 1;
        }
        let mut ready: Vec<&str> =
            incoming.iter().filter(|(_, n)| **n == 0).map(|(id, _)| *id).collect();
        let mut out = Vec::new();
        while let Some(id) = ready.pop() {
            out.push(id.to_string());
            for e in self.edges.iter().filter(|e| e.from == id) {
                let n = incoming.get_mut(e.to.as_str()).expect("checked");
                *n -= 1;
                if *n == 0 {
                    ready.push(e.to.as_str());
                }
            }
        }
        if out.len() != self.nodes.len() {
            let stuck = self
                .nodes
                .iter()
                .find(|n| !out.contains(&n.id))
                .map(|n| n.id.clone())
                .unwrap_or_default();
            return Err(DagError::Cycle(stuck));
        }
        Ok(out)
    }

    /// Every node that is guaranteed to run before this one.
    ///
    /// ★★★ Ancestry, not topological position. Two nodes with no edge between
    /// them have no order — which one a topological sort happens to put first
    /// is a tie-break, and a binding that depends on a tie-break is a binding
    /// that works until the sort changes. Only an edge is a promise.
    fn ancestors(&self, id: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut frontier = vec![id.to_string()];
        while let Some(cur) = frontier.pop() {
            for e in self.edges.iter().filter(|e| e.to == cur) {
                if seen.insert(e.from.clone()) {
                    frontier.push(e.from.clone());
                }
            }
        }
        seen
    }

    /// Every path from the entry to a leaf, as node ids.
    fn paths(&self) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        let mut stack = vec![vec![self.entry.clone()]];
        while let Some(path) = stack.pop() {
            let last = path.last().expect("non-empty").clone();
            let nexts: Vec<&Edge> = self.edges.iter().filter(|e| e.from == last).collect();
            if nexts.is_empty() {
                out.push(path);
                continue;
            }
            for e in nexts {
                let mut extended = path.clone();
                extended.push(e.to.clone());
                stack.push(extended);
            }
        }
        out
    }

    /// **Refuse a graph that could never work, at the moment it is written.**
    ///
    /// ★★★ This is what wiring `compose.rs` means. Every root-to-leaf path is
    /// run through [`Pathway::try_chain`], so a chain whose successor's guard
    /// its predecessor's postcondition can never satisfy yields no graph at
    /// all — the same structural refusal `compose` was built to give.
    ///
    /// ★★ The cheaper checks come first and are just as load-bearing: an
    /// operator that does not exist, a node reading a field from a node that
    /// runs after it, a cycle. Each of those is a graph that would fail
    /// halfway, having already moved money.
    pub fn typecheck(&self, registry: &Registry) -> Result<(), DagError> {
        let mut seen = BTreeSet::new();
        for n in &self.nodes {
            if !seen.insert(n.id.as_str()) {
                return Err(DagError::DuplicateNode(n.id.clone()));
            }
            if registry.get(&n.operator).is_none() {
                return Err(DagError::UnknownOperator {
                    node: n.id.clone(),
                    operator: n.operator.clone(),
                });
            }
        }
        if self.node(&self.entry).is_none() {
            return Err(DagError::UnknownNode(self.entry.clone()));
        }
        for e in &self.edges {
            for id in [&e.from, &e.to] {
                if self.node(id).is_none() {
                    return Err(DagError::UnknownNode(id.clone()));
                }
            }
        }

        // Refuses a cycle, and is what makes ancestry finite below.
        self.order()?;

        // ★★ A binding may only read a node the edges GUARANTEE runs first.
        //    Reading a sibling would work whenever the sort happened to favour
        //    it and silently stop when it did not; reading a later node is a
        //    value that is never there. Both surface at run time as a missing
        //    parameter, which the operator then refuses for a reason that names
        //    the wrong thing entirely.
        for n in &self.nodes {
            let before = self.ancestors(&n.id);
            for (_, b) in &n.bind {
                if let Binding::From { node, field } = b {
                    if self.node(node).is_none() {
                        return Err(DagError::UnknownNode(node.clone()));
                    }
                    if !before.contains(node) {
                        return Err(DagError::OutOfOrder {
                            node: n.id.clone(),
                            reads_from: node.clone(),
                            field: field.clone(),
                        });
                    }
                }
            }
        }

        for path in self.paths() {
            let steps: Vec<Step> = path
                .iter()
                .filter_map(|id| self.node(id))
                .filter_map(|n| registry.get(&n.operator).map(|m| m.as_step()))
                .collect();
            if let Err(PathwayError::Rejected(r)) = Pathway::try_chain(&steps) {
                return Err(DagError::Unchainable {
                    path: path.join(" → "),
                    reason: r.to_string(),
                });
            }
        }
        Ok(())
    }

    /// Resolve one node's parameters.
    fn params_for(
        &self,
        node: &Node,
        input: &Map<String, Value>,
        results: &BTreeMap<String, Value>,
    ) -> Map<String, Value> {
        let mut params = Map::new();
        for (name, binding) in &node.bind {
            // ★ An unresolvable binding contributes NOTHING rather than null.
            //   A missing parameter is a shape operators already handle — they
            //   have their own defaults and their own refusals — whereas an
            //   explicit null is a value, and one they would have to learn to
            //   distrust.
            let value = match binding {
                Binding::Literal(v) => Some(v.clone()),
                Binding::Input(k) => input.get(k).cloned(),
                Binding::From { node, field } => {
                    results.get(node).and_then(|r| r.get(field)).cloned()
                }
            };
            if let Some(v) = value.filter(|v| !v.is_null()) {
                params.insert(name.clone(), v);
            }
        }
        params
    }
}

/// **Run an operative.**
///
/// ★★★ Every node goes through `execute` — the real gate, the real invariants,
/// the real double-entry check. There is no second path into state here, which
/// is the only reason an operative can be trusted with money at all.
///
/// ★★ A refusal STOPS that branch. The alternative — carry on and let later
/// steps refuse too — would turn one honest "no" into a pile of confusing ones
/// and, worse, could let a step that does not depend on the refused one commit
/// as though the whole thing had worked.
pub fn run(
    dag: &Dag,
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    input: &Map<String, Value>,
) -> Result<DagRun, DagError> {
    dag.typecheck(registry)?;
    let order = dag.order()?;

    let mut results: BTreeMap<String, Value> = BTreeMap::new();
    let mut live: BTreeSet<String> = BTreeSet::new();
    live.insert(dag.entry.clone());

    let mut run = DagRun {
        steps: Vec::new(),
        state: state.clone(),
        refused_at: None,
        unreached: Vec::new(),
    };

    for id in &order {
        if !live.contains(id) {
            run.unreached.push(id.clone());
            continue;
        }
        let node = dag.node(id).expect("typechecked");
        let params = dag.params_for(node, input, &results);
        let outcome =
            execute(registry, allowed, enforcement, &run.state, &node.operator, &params);

        // ★ `Deferred` is not a refusal — it is a decision not yet made —
        //   but it is also not a commit, so the branch stops either way and the
        //   step carries its own status for a caller that cares which.
        let committed = outcome.result.status == crate::operator::meta::OperatorStatus::Ok;
        run.state = outcome.state;
        results.insert(id.clone(), outcome.result.data.clone());
        run.steps.push(NodeOutcome {
            node: id.clone(),
            operator: node.operator.clone(),
            result: outcome.result,
        });

        if !committed {
            run.refused_at = Some(id.clone());
            // Everything still ahead is unreached, and says so.
            run.unreached
                .extend(order.iter().skip_while(|o| *o != id).skip(1).cloned());
            return Ok(run);
        }

        for e in dag.edges.iter().filter(|e| &e.from == id) {
            if e.when.as_ref().is_none_or(|c| c.holds(&results)) {
                live.insert(e.to.clone());
            }
        }
    }
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn household() -> Value {
        json!({"finances": {
            "liquid": {"balance": 5000.0},
            "pockets": {"food": {"allocated": 2000.0, "spent": 0.0, "limit": 0.0}},
            "accounts": {}, "income": {"monthly_total": 0.0, "sources": []}},
            "vendors": {}})
    }

    fn input(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    /// identify → suggest, the read-only head of Mentor.
    fn reading_dag() -> Dag {
        Dag::new("read", "identify")
            .with_node(Node::new("identify", "vendor.identify").from_input("counterparty"))
            .with_node(
                Node::new("suggest", "vendor.suggest").from_node("vendor", "identify", "vendor"),
            )
            .with_edge(Edge::new("identify", "suggest"))
    }

    fn go(dag: &Dag, state: &Value, inp: &[(&str, Value)]) -> DagRun {
        let reg = Registry::default();
        run(dag, &reg, &reg.names(), &Enforcement::default(), state, &input(inp))
            .expect("typechecks")
    }

    #[test]
    fn one_node_hands_its_answer_to_the_next() {
        // ★★★ The whole premise: the order is data, and a value crosses between
        //     steps without a host function in the middle.
        let after = go(&reading_dag(), &household(), &[("counterparty", json!("NAIVAS LTD"))]);
        assert!(after.succeeded());
        assert_eq!(after.result_of("identify").unwrap().data["vendor"], json!("naivas"));
        assert_eq!(after.result_of("suggest").unwrap().data["vendor"], json!("naivas"));
    }

    #[test]
    fn a_graph_naming_an_operator_that_does_not_exist_is_refused_before_it_runs() {
        let dag = Dag::new("bad", "a").with_node(Node::new("a", "vendor.invent"));
        let reg = Registry::default();
        assert!(matches!(
            dag.typecheck(&reg),
            Err(DagError::UnknownOperator { .. })
        ));
    }

    #[test]
    fn reading_a_step_that_really_does_run_first_is_fine() {
        // The ordinary case: `suggest` reads `identify`, and an edge says so.
        assert!(reading_dag().typecheck(&Registry::default()).is_ok());
    }

    #[test]
    fn reading_a_step_that_runs_afterwards_is_refused() {
        // ★★★ Otherwise it fails at run time as a missing parameter, and the
        //     operator refuses for a reason that names the wrong thing.
        let backwards = Dag::new("back", "identify")
            .with_node(
                Node::new("identify", "vendor.identify")
                    .from_node("counterparty", "suggest", "vendor"),
            )
            .with_node(Node::new("suggest", "vendor.suggest"))
            .with_edge(Edge::new("identify", "suggest"));
        assert!(matches!(
            backwards.typecheck(&Registry::default()),
            Err(DagError::OutOfOrder { .. })
        ));
    }

    #[test]
    fn reading_a_step_with_no_edge_ordering_the_two_is_also_refused() {
        // ★★★ The subtle one, and the reason this checks ANCESTRY rather than
        //     where a topological sort happened to put things. Two branches off
        //     one parent have no order between them: such a binding would work
        //     for exactly as long as the sort kept favouring it.
        let siblings = reading_dag()
            .with_node(
                Node::new("cousin", "vendor.identify")
                    .from_node("counterparty", "suggest", "pocket"),
            )
            .with_edge(Edge::new("identify", "cousin"));
        assert!(matches!(
            siblings.typecheck(&Registry::default()),
            Err(DagError::OutOfOrder { .. })
        ));
    }

    #[test]
    fn a_graph_that_loops_is_refused_rather_than_run() {
        // ★★★ Money moves through here. A runner that can revisit a node is a
        //     runner that can pay somebody twice.
        let looping = Dag::new("loop", "a")
            .with_node(Node::new("a", "vendor.identify"))
            .with_node(Node::new("b", "vendor.suggest"))
            .with_edge(Edge::new("a", "b"))
            .with_edge(Edge::new("b", "a"));
        assert!(matches!(looping.typecheck(&Registry::default()), Err(DagError::Cycle(_))));
    }

    #[test]
    fn a_branch_not_taken_is_reported_unreached_not_silently_missing() {
        // ★★ "It did not run" and "it ran and said nothing" are different
        //    facts, and a caller acting on the second when the first is true
        //    would be acting on an absence.
        let dag = reading_dag()
            .with_node(
                Node::new("file", "budget.spend")
                    .from_node("pocket_name", "suggest", "pocket")
                    .literal("amount", json!(100.0)),
            )
            .with_edge(Edge::new("suggest", "file").when(Condition::Present {
                node: "suggest".into(),
                field: "pocket".into(),
            }));

        // Nothing was ever filed against this vendor, so `suggest` has no
        // pocket and the filing branch is not taken.
        let after = go(&dag, &household(), &[("counterparty", json!("BRAND NEW SHOP"))]);
        assert!(after.succeeded());
        assert!(after.result_of("file").is_none());
        assert!(after.unreached.contains(&"file".to_string()));
    }

    #[test]
    fn the_branch_is_taken_when_the_previous_step_actually_answered() {
        // ★★★ The other half of the same test, and the one that proves the
        //     condition reads a real result rather than always failing shut.
        let reg = Registry::default();
        let remembered = execute(
            &reg,
            &reg.names(),
            &Enforcement::default(),
            &household(),
            "vendor.remember",
            &input(&[("counterparty", json!("NAIVAS")), ("pocket_name", json!("food"))]),
        );
        assert_eq!(remembered.result.status, crate::operator::meta::OperatorStatus::Ok);

        let dag = reading_dag()
            .with_node(
                Node::new("file", "budget.spend")
                    .from_node("pocket_name", "suggest", "pocket")
                    .literal("amount", json!(100.0)),
            )
            .with_edge(Edge::new("suggest", "file").when(Condition::Present {
                node: "suggest".into(),
                field: "pocket".into(),
            }));

        let after = go(&dag, &remembered.state, &[("counterparty", json!("NAIVAS"))]);
        assert!(after.succeeded(), "{:?}", after.steps.last().map(|s| &s.result.reason));
        assert_eq!(
            after.state.pointer("/finances/pockets/food/spent").and_then(Value::as_f64),
            Some(100.0),
            "the real operator ran through the real gate",
        );
    }

    #[test]
    fn a_refused_step_stops_the_branch_and_says_which_one() {
        // ★★★ Carrying on past a refusal could let a later step that does not
        //     depend on it commit as though the whole thing had worked.
        let dag = Dag::new("overspend", "spend")
            .with_node(
                Node::new("spend", "budget.spend")
                    .literal("pocket_name", json!("food"))
                    .literal("amount", json!(999_999.0)),
            )
            .with_node(Node::new("after", "vendor.identify").literal("counterparty", json!("X")))
            .with_edge(Edge::new("spend", "after"));

        let before = household();
        let after = go(&dag, &before, &[]);
        assert!(!after.succeeded());
        assert_eq!(after.refused_at.as_deref(), Some("spend"));
        assert!(after.unreached.contains(&"after".to_string()));
        assert_eq!(after.state, before, "a refusal changes nothing");
    }

    #[test]
    fn an_operative_has_no_gate_of_its_own() {
        // ★★★ The property that makes a graph safe to hand money to: it cannot
        //     reach state except through the same door a tap uses. An operator
        //     the sustain does not allow is refused inside the graph exactly as
        //     it would be outside one.
        let dag = Dag::new("blocked", "spend").with_node(
            Node::new("spend", "budget.spend")
                .literal("pocket_name", json!("food"))
                .literal("amount", json!(10.0)),
        );
        let reg = Registry::default();
        let allowed: Vec<String> = vec!["vendor.identify".into()];
        let after = run(
            &dag,
            &reg,
            &allowed,
            &Enforcement::default(),
            &household(),
            &Map::new(),
        )
        .expect("typechecks");
        assert!(!after.succeeded());
        assert_eq!(
            after.steps[0].result.constraint_violated.as_deref(),
            Some("operator_allowed"),
        );
    }
}
