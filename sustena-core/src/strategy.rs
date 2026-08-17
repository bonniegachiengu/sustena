//! `Π = ⟨N, E, entry, exit⟩` — the strategy DAG (OPV-4; the Operative paper,
//! §III).
//!
//! An operative's *how it reasons*: a graph whose **nodes are operator calls**
//! and whose **edges are (possibly conditional) transitions**. Running it is a
//! conditional topological walk — start at `entry`, execute each node, keep the
//! results, follow the first matching edge, stop at `exit`.
//!
//! ## ★★★ Proposition 3 — a strategy adds STRUCTURE, not CAPABILITY
//!
//! > The only material a thought can be made of here is legal moves.
//!
//! A [`StrategyNode`] holds a [`LegalMove`], and a `LegalMove` can only be
//! minted by [`Shared::legal_move`] — so **a node naming something outside `T`
//! is not rejected, it is unconstructible**. That is OPV-1's Proposition 1
//! mechanism reused one level up rather than a second check written beside it.
//!
//! The consequence is the proposition: `Π` re-sequences and conditions moves
//! that were already legal, so **its reachable set is bounded by `T`'s**.
//! Asserted directly against [`Shared::reachable`], which takes neither an
//! operative nor a strategy — a bound `Π` cannot widen because it is not a
//! parameter of the function that computes it.
//!
//! ## Each node goes through the real gate
//!
//! Running `Π` **is** running its operators, and each one is admitted exactly
//! as any other: guard, result-in-`V`, `D`, closure, `F`, and the
//! authorization/token conjuncts. There is no strategy bypass, because the
//! walker calls [`crate::operator::execute_admitted`] rather than an operator
//! function directly. ★ A node the gate refuses **stops that path honestly**
//! ([`WalkOutcome::Refused`]) with the gate's own reason, and the nodes that
//! already committed stay committed — the same stop-at-the-first-real-failure
//! discipline EVT-15's promotion follows.
//!
//! ## `$dot.path` is resolved, never evaluated
//!
//! A node's kwargs may reference an earlier node's result by `$node.field`,
//! resolved **at run time** against the accumulated results. Pattern-match and
//! dict-walk only — no `eval`, no `compile`, the same discipline as
//! `predicate`, `parse_rule` and every prior slice.
//!
//! `{{placeholder}}` calibration is the other half and happens **earlier**, at
//! build time, from a supplied calibration map. Two passes, two times, and they
//! do not see each other's syntax.
//!
//! ## The cycle guard reports rather than truncates
//!
//! The walk is bounded by `max(2·|N|, 10)` steps — the reference's own limit,
//! ported. ★ **But exhausting it is a reported outcome here, not a silent
//! stop.** The reference `break`s out of the loop and builds a proposal from
//! whatever the last result happened to be, so a cyclic graph and a graph that
//! finished look alike to the caller. [`WalkOutcome::StepsExhausted`] makes
//! them different, because *it ran out* and *it finished* are different facts.
//!
//! ## Π is not the router
//!
//! [`crate::mixture`] chooses **which** operative acts; `Π` is **how** the
//! chosen one reasons. Kept apart deliberately: a router that also reasoned
//! would make the sparsity guarantee meaningless, and a strategy that also
//! routed would be choosing its own author.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{Map, Value};

use crate::operative::{LegalMove, Shared};
use crate::operator::{execute_admitted, Authorization, Enforcement, Registry};
use crate::approval::{EffectClass, NonceLedger};

/// How a condition compares. Six operators, matching the reference exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
}

impl CompareOp {
    fn parse(raw: &str) -> Option<CompareOp> {
        Some(match raw {
            ">" => CompareOp::Gt,
            "<" => CompareOp::Lt,
            ">=" => CompareOp::Ge,
            "<=" => CompareOp::Le,
            "==" => CompareOp::Eq,
            "!=" => CompareOp::Ne,
            _ => return None,
        })
    }
}

/// An edge guard, read against the producing node's result data.
///
/// ★ Named `Condition` here and nowhere else in the crate; `admission` uses
/// `ConstraintDecl` and `predicate` uses `Predicate`, so there is no collision
/// to dodge.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// A dot-path into the result's data.
    pub field: String,
    pub op: CompareOp,
    pub value: Value,
}

impl Condition {
    pub fn new(field: &str, op: &str, value: Value) -> Option<Condition> {
        Some(Condition { field: field.to_string(), op: CompareOp::parse(op)?, value })
    }

    /// ★ **An absent field is `false`, not an error** — parity with the
    /// reference, and the right reading: a condition about something that did
    /// not happen has not been met. Likewise a type mismatch, which the
    /// reference catches as `TypeError` and treats the same way.
    pub fn holds(&self, data: &Value) -> bool {
        let Some(actual) = walk_path(data, &self.field) else { return false };
        let ord = match (actual, &self.value) {
            (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
                (Some(x), Some(y)) => x.partial_cmp(&y),
                _ => None,
            },
            (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
            _ => None,
        };
        match self.op {
            CompareOp::Eq => actual == &self.value,
            CompareOp::Ne => actual != &self.value,
            CompareOp::Gt => ord == Some(std::cmp::Ordering::Greater),
            CompareOp::Lt => ord == Some(std::cmp::Ordering::Less),
            CompareOp::Ge => matches!(
                ord,
                Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            ),
            CompareOp::Le => {
                matches!(ord, Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
            }
        }
    }
}

/// One node of `Π` — an operator call.
///
/// ★★★ It holds a [`LegalMove`], not a name. Proposition 3 is therefore a
/// property of the type: there is no constructor taking a bare string, so a
/// node outside `T` cannot be written down.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyNode {
    id: String,
    mv: LegalMove,
    kwargs: Map<String, Value>,
}

impl StrategyNode {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The move this node makes — always an element of `T`.
    pub fn move_name(&self) -> &str {
        self.mv.name()
    }

    pub fn kwargs(&self) -> &Map<String, Value> {
        &self.kwargs
    }
}

/// A directed transition, optionally guarded.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyEdge {
    pub from: String,
    pub to: String,
    /// `None` is an unconditional edge — parity with the reference's
    /// `condition: None` meaning *always matches*.
    pub condition: Option<Condition>,
}

/// Why a strategy could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyError {
    /// ★★★ The move is not in `T`. Reported at build rather than at run,
    /// because a strategy that cannot legally be run is a defect in the
    /// strategy, not an event during it.
    MoveNotInT { node: String, mv: String },
    DuplicateNode { node: String },
    /// An edge names a node the graph does not have.
    DanglingEdge { from: String, to: String },
    /// `entry` or `exit` names no node.
    MissingEndpoint { which: &'static str, node: String },
    /// A `{{token}}` the calibration map does not supply.
    Uncalibrated { node: String, param: String, token: String },
}

impl std::fmt::Display for StrategyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrategyError::MoveNotInT { node, mv } => write!(
                f,
                "node '{node}' names '{mv}', which is not a legal move: a strategy \
                 composes moves, it does not create them"
            ),
            StrategyError::DuplicateNode { node } => write!(f, "node '{node}' declared twice"),
            StrategyError::DanglingEdge { from, to } => {
                write!(f, "edge '{from}' → '{to}' names a node that does not exist")
            }
            StrategyError::MissingEndpoint { which, node } => {
                write!(f, "{which} node '{node}' is not in the graph")
            }
            StrategyError::Uncalibrated { node, param, token } => write!(
                f,
                "node '{node}' parameter '{param}' needs '{{{{{token}}}}}', which the \
                 calibration does not supply"
            ),
        }
    }
}

/// `Π = ⟨N, E, entry, exit⟩`.
///
/// ★ Named `StrategyGraph`, not `Strategy` — `admission::Strategy` is the
/// constraint verdict policy (refuse / clamp / defer), and confusing *what a
/// rule does when it is broken* with *how an operative reasons* would be a
/// genuinely misleading collision. Twenty-eighth; the newcomer takes the
/// longer name, matching `DefinitionGraph`'s precedent.
#[derive(Debug, Clone, PartialEq)]
pub struct StrategyGraph {
    nodes: BTreeMap<String, StrategyNode>,
    order: Vec<String>,
    edges: Vec<StrategyEdge>,
    entry: String,
    exit: String,
}

impl StrategyGraph {
    pub fn new(entry: &str, exit: &str) -> StrategyGraph {
        StrategyGraph {
            nodes: BTreeMap::new(),
            order: Vec::new(),
            edges: Vec::new(),
            entry: entry.to_string(),
            exit: exit.to_string(),
        }
    }

    /// Add a node. ★★★ The move is minted from `world`, so a name outside `T`
    /// never becomes a node.
    pub fn with_node(
        mut self,
        world: &Shared,
        id: &str,
        mv: &str,
        kwargs: Map<String, Value>,
    ) -> Result<StrategyGraph, StrategyError> {
        if self.nodes.contains_key(id) {
            return Err(StrategyError::DuplicateNode { node: id.to_string() });
        }
        let legal = world
            .legal_move(mv)
            .ok_or_else(|| StrategyError::MoveNotInT {
                node: id.to_string(),
                mv: mv.to_string(),
            })?;
        self.order.push(id.to_string());
        self.nodes
            .insert(id.to_string(), StrategyNode { id: id.to_string(), mv: legal, kwargs });
        Ok(self)
    }

    pub fn with_edge(mut self, from: &str, to: &str, condition: Option<Condition>) -> StrategyGraph {
        self.edges.push(StrategyEdge { from: from.to_string(), to: to.to_string(), condition });
        self
    }

    /// Check the shape: endpoints exist, edges point at real nodes.
    pub fn typecheck(&self) -> Result<(), StrategyError> {
        for (which, node) in [("entry", &self.entry), ("exit", &self.exit)] {
            if !self.nodes.contains_key(node) {
                return Err(StrategyError::MissingEndpoint { which, node: node.clone() });
            }
        }
        for e in &self.edges {
            if !self.nodes.contains_key(&e.from) || !self.nodes.contains_key(&e.to) {
                return Err(StrategyError::DanglingEdge {
                    from: e.from.clone(),
                    to: e.to.clone(),
                });
            }
        }
        Ok(())
    }

    /// Resolve every `{{token}}` in every node's kwargs from a calibration map.
    ///
    /// ★ Build-time, and a separate pass from `$dot.path`: the two syntaxes
    /// never meet, so a calibration value cannot be mistaken for a runtime
    /// reference or vice versa.
    pub fn calibrated(mut self, calibration: &Map<String, Value>) -> Result<StrategyGraph, StrategyError> {
        for id in &self.order {
            let node = self.nodes.get_mut(id).expect("order tracks nodes");
            let mut resolved = Map::new();
            for (k, v) in &node.kwargs {
                resolved.insert(k.clone(), resolve_placeholders(v, calibration, id, k)?);
            }
            node.kwargs = resolved;
        }
        Ok(self)
    }

    /// ★★★ **Proposition 3, as a set**: the moves this strategy can make.
    ///
    /// Every one is in `T` by construction, so `Π ⊆ T` needs no check — it is
    /// what the type already guaranteed. The method exists so the *bound* can
    /// be asserted rather than argued.
    pub fn moves(&self) -> BTreeSet<&str> {
        self.nodes.values().map(|n| n.mv.name()).collect()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &StrategyNode> {
        self.order.iter().filter_map(|id| self.nodes.get(id))
    }

    pub fn entry(&self) -> &str {
        &self.entry
    }

    pub fn exit(&self) -> &str {
        &self.exit
    }

    /// The reference's guard: `max(2·|N|, 10)`.
    pub fn step_limit(&self) -> usize {
        (self.nodes.len() * 2).max(10)
    }
}

/// What one node did.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRecord {
    pub node: String,
    pub mv: String,
    pub committed: bool,
    /// The gate's reason when it refused.
    pub reason: Option<String>,
}

/// How the walk ended. ★ Four outcomes, and none of them is silence.
#[derive(Debug, Clone, PartialEq)]
pub enum WalkOutcome {
    /// Reached `exit`.
    ReachedExit,
    /// No outgoing edge matched — a real terminal, not a failure: the strategy
    /// declined to continue given what it found.
    NoMatchingEdge { at: String },
    /// The gate refused a node. The walk stops; what committed stays committed.
    Refused { node: String, reason: String },
    /// ★ The step limit ran out. **Reported**, where the reference stops
    /// silently — *it ran out* and *it finished* are different facts.
    StepsExhausted { limit: usize },
}

impl WalkOutcome {
    pub fn finished(&self) -> bool {
        matches!(self, WalkOutcome::ReachedExit)
    }
}

/// What running `Π` produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Walk {
    pub steps: Vec<StepRecord>,
    pub outcome: WalkOutcome,
    /// State after the last committed node.
    pub state: Value,
    /// Each node's result data, keyed by node id, plus `trigger_event`.
    pub accumulated: Map<String, Value>,
}

impl Walk {
    /// The moves this run actually made — used to assert the reachability
    /// bound against `Shared::reachable`.
    pub fn moves_made(&self) -> BTreeSet<&str> {
        self.steps.iter().filter(|s| s.committed).map(|s| s.mv.as_str()).collect()
    }
}

/// `run(Π, ctx, trigger)` — the conditional topological walk.
///
/// ★ Every node goes through [`execute_admitted`], so `Π` inherits the whole
/// gate and has no bypass. State threads from node to node; a refusal stops the
/// walk with what already committed intact.
#[allow(clippy::too_many_arguments)]
pub fn run(
    strategy: &StrategyGraph,
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    initial: &Value,
    trigger: Value,
) -> Walk {
    let mut accumulated = Map::new();
    accumulated.insert("trigger_event".to_string(), trigger);

    let mut state = initial.clone();
    let mut steps = Vec::new();
    let mut current = strategy.entry.to_string();
    let limit = strategy.step_limit();
    let mut nonces = NonceLedger::new();

    for _ in 0..limit {
        let Some(node) = strategy.nodes.get(&current) else {
            // typecheck() rules this out; if a caller skipped it, the honest
            // answer is a dangling edge rather than a panic.
            return Walk {
                steps,
                outcome: WalkOutcome::NoMatchingEdge { at: current },
                state,
                accumulated,
            };
        };

        // ── $dot.path, resolved against what has accumulated ─────────────────
        let mut kwargs = Map::new();
        for (k, v) in &node.kwargs {
            kwargs.insert(k.clone(), resolve_dynamic(v, &accumulated));
        }

        let run = execute_admitted(
            registry,
            allowed,
            enforcement,
            &state,
            node.mv.name(),
            &kwargs,
            &Authorization::Unchecked,
            &EffectClass::Unchecked,
            &mut nonces,
        );

        if !run.committed() {
            let reason = run.result.reason.clone().unwrap_or_default();
            steps.push(StepRecord {
                node: node.id.clone(),
                mv: node.mv.name().to_string(),
                committed: false,
                reason: Some(reason.clone()),
            });
            return Walk {
                steps,
                outcome: WalkOutcome::Refused { node: node.id.clone(), reason },
                state,
                accumulated,
            };
        }

        state = run.state;
        accumulated.insert(node.id.clone(), run.result.data.clone());
        steps.push(StepRecord {
            node: node.id.clone(),
            mv: node.mv.name().to_string(),
            committed: true,
            reason: None,
        });

        if node.id == strategy.exit {
            return Walk { steps, outcome: WalkOutcome::ReachedExit, state, accumulated };
        }

        let produced = run.result.data.clone();
        let next = strategy
            .edges
            .iter()
            .find(|e| e.from == current && e.condition.as_ref().is_none_or(|c| c.holds(&produced)))
            .map(|e| e.to.clone());

        match next {
            Some(n) => current = n,
            None => {
                return Walk {
                    steps,
                    outcome: WalkOutcome::NoMatchingEdge { at: current },
                    state,
                    accumulated,
                }
            }
        }
    }

    Walk { steps, outcome: WalkOutcome::StepsExhausted { limit }, state, accumulated }
}

// ── resolution, both passes ──────────────────────────────────────────────────

fn walk_path<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    let mut node = data;
    for seg in path.split('.') {
        node = node.get(seg)?;
    }
    Some(node)
}

/// `$node.field` → the accumulated value. Pattern-match and dict-walk, no eval.
///
/// A `$` reference that does not resolve is left **as the literal string**,
/// which is the reference's behaviour: the operator then sees a string where it
/// expected a value and refuses through its own guard, rather than the resolver
/// inventing a `null`.
fn resolve_dynamic(value: &Value, accumulated: &Map<String, Value>) -> Value {
    match value {
        Value::String(s) => match s.strip_prefix('$') {
            Some(path) => walk_path(&Value::Object(accumulated.clone()), path)
                .cloned()
                .unwrap_or_else(|| value.clone()),
            None => value.clone(),
        },
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| resolve_dynamic(v, accumulated)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter().map(|(k, v)| (k.clone(), resolve_dynamic(v, accumulated))).collect(),
        ),
        other => other.clone(),
    }
}

/// `{{token}}` → the calibration value, at build time.
fn resolve_placeholders(
    value: &Value,
    calibration: &Map<String, Value>,
    node: &str,
    param: &str,
) -> Result<Value, StrategyError> {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            let Some(inner) = trimmed.strip_prefix("{{").and_then(|r| r.strip_suffix("}}")) else {
                return Ok(value.clone());
            };
            let token = inner.trim();
            walk_path(&Value::Object(calibration.clone()), token).cloned().ok_or_else(|| {
                StrategyError::Uncalibrated {
                    node: node.to_string(),
                    param: param.to_string(),
                    token: token.to_string(),
                }
            })
        }
        Value::Array(items) => Ok(Value::Array(
            items
                .iter()
                .map(|v| resolve_placeholders(v, calibration, node, param))
                .collect::<Result<_, _>>()?,
        )),
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve_placeholders(v, calibration, node, param)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// ★★★ **Proposition 3, executed.** The states a strategy can reach, bounded
/// by what `T` already allowed.
///
/// Runs `Shared`'s own reachability from `from`, restricted to the moves this
/// strategy actually names — and the result is necessarily a subset of
/// [`Shared::reachable`], because every move in `Π` came from `Shared`.
pub fn reachable_under(strategy: &StrategyGraph, world: &Shared, from: &str) -> BTreeSet<String> {
    let permitted = strategy.moves();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    seen.insert(from.to_string());
    queue.push_back(from.to_string());
    while let Some(s) = queue.pop_front() {
        for mv in permitted.iter() {
            if let Some(next) = world.next_state(&s, mv) {
                if seen.insert(next.clone()) {
                    queue.push_back(next);
                }
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Schema;
    use serde_json::json;

    fn world() -> Shared {
        Shared::new()
            .with_move("budget.record_income")
            .with_move("budget.allocate")
    }

    fn kwargs(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    fn allowed() -> Vec<String> {
        vec!["budget.record_income".to_string(), "budget.allocate".to_string()]
    }

    fn state() -> Value {
        json!({"finances": {
            "liquid": {"balance": 0.0},
            "income": {"monthly_total": 0.0, "sources": []},
            "pockets": {}
        }})
    }

    fn quiet() -> Enforcement {
        Enforcement { enabled: false, schema: Some(Schema::new()), ..Default::default() }
    }

    // ── Proposition 3 ────────────────────────────────────────────────────────

    #[test]
    fn a_node_outside_t_is_refused_at_build() {
        // ★★★ And it is unconstructible, not merely rejected: `StrategyNode`
        // holds a `LegalMove`, which only `Shared` can mint.
        let err = StrategyGraph::new("a", "a")
            .with_node(&world(), "a", "egress.prepare_household_summary", Map::new())
            .unwrap_err();
        assert_eq!(
            err,
            StrategyError::MoveNotInT {
                node: "a".into(),
                mv: "egress.prepare_household_summary".into()
            }
        );
    }

    #[test]
    fn every_move_a_strategy_names_is_in_t() {
        let w = world();
        let pi = StrategyGraph::new("a", "b")
            .with_node(&w, "a", "budget.record_income", Map::new())
            .unwrap()
            .with_node(&w, "b", "budget.allocate", Map::new())
            .unwrap();
        let t: BTreeSet<&str> = w.moves().into_iter().collect();
        assert!(pi.moves().is_subset(&t), "Π ⊆ T, by construction");
    }

    #[test]
    fn a_strategy_reaches_no_state_t_did_not_already_allow() {
        // ★★★ The proposition as a set comparison. Π re-sequences legal moves;
        // it cannot enlarge the reachable set.
        let w = Shared::new()
            .with_move("m1")
            .with_move("m2")
            .with_transition("s0", "m1", "s1")
            .unwrap()
            .with_transition("s1", "m2", "s2")
            .unwrap();
        let pi = StrategyGraph::new("a", "a").with_node(&w, "a", "m1", Map::new()).unwrap();

        let by_t = w.reachable("s0");
        let by_pi = reachable_under(&pi, &w, "s0");
        assert!(by_pi.is_subset(&by_t), "a strategy adds structure, not capability");
        assert!(by_t.contains("s2") && !by_pi.contains("s2"), "and it can only narrow");
    }

    // ── the walk ─────────────────────────────────────────────────────────────

    #[test]
    fn the_walk_runs_entry_to_exit_and_threads_state() {
        let w = world();
        let pi = StrategyGraph::new("earn", "spend")
            .with_node(
                &w,
                "earn",
                "budget.record_income",
                kwargs(&[
                    ("amount", json!(1000.0)),
                    ("source", json!("wages")),
                    ("entry_id", json!("e1")),
                ]),
            )
            .unwrap()
            .with_node(
                &w,
                "spend",
                "budget.allocate",
                kwargs(&[
                    ("pocket_name", json!("food")),
                    ("amount", json!(400.0)),
                    ("period", json!("monthly")),
                ]),
            )
            .unwrap()
            .with_edge("earn", "spend", None);
        pi.typecheck().unwrap();

        let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
        assert_eq!(walk.outcome, WalkOutcome::ReachedExit);
        assert_eq!(walk.steps.len(), 2);
        assert!(walk.steps.iter().all(|s| s.committed));
        assert_eq!(walk.state["finances"]["liquid"]["balance"], json!(600.0));
    }

    #[test]
    fn a_dollar_path_kwarg_is_resolved_from_an_earlier_result() {
        // ★ Resolved, never evaluated: the allocation's amount comes from the
        // income node's own reported figure.
        let w = world();
        let pi = StrategyGraph::new("earn", "spend")
            .with_node(
                &w,
                "earn",
                "budget.record_income",
                kwargs(&[
                    ("amount", json!(250.0)),
                    ("source", json!("wages")),
                    ("entry_id", json!("e1")),
                ]),
            )
            .unwrap()
            .with_node(
                &w,
                "spend",
                "budget.allocate",
                kwargs(&[
                    ("pocket_name", json!("food")),
                    ("amount", json!("$earn.amount")),
                    ("period", json!("monthly")),
                ]),
            )
            .unwrap()
            .with_edge("earn", "spend", None);

        let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
        assert_eq!(walk.outcome, WalkOutcome::ReachedExit);
        assert_eq!(walk.state["finances"]["liquid"]["balance"], json!(0.0), "250 in, 250 out");
    }

    #[test]
    fn a_conditional_edge_routes_on_the_producing_nodes_result() {
        let w = world();
        let pi = StrategyGraph::new("earn", "spend")
            .with_node(
                &w,
                "earn",
                "budget.record_income",
                kwargs(&[
                    ("amount", json!(10.0)),
                    ("source", json!("wages")),
                    ("entry_id", json!("e1")),
                ]),
            )
            .unwrap()
            .with_node(
                &w,
                "spend",
                "budget.allocate",
                kwargs(&[
                    ("pocket_name", json!("food")),
                    ("amount", json!(5.0)),
                    ("period", json!("monthly")),
                ]),
            )
            .unwrap()
            // Only allocate if the income was large.
            .with_edge("earn", "spend", Condition::new("amount", ">", json!(100.0)));

        let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
        assert_eq!(walk.outcome, WalkOutcome::NoMatchingEdge { at: "earn".into() });
        assert_eq!(walk.steps.len(), 1, "the second node never ran");
    }

    #[test]
    fn an_absent_condition_field_reads_false_rather_than_erroring() {
        let c = Condition::new("nowhere.at.all", ">", json!(1)).unwrap();
        assert!(!c.holds(&json!({"amount": 500})));
    }

    #[test]
    fn a_type_mismatch_reads_false_too() {
        let c = Condition::new("amount", ">", json!(1)).unwrap();
        assert!(!c.holds(&json!({"amount": "lots"})));
    }

    // ── the cycle guard ──────────────────────────────────────────────────────

    #[test]
    fn a_cycle_is_bounded_and_the_exhaustion_is_reported() {
        // ★ The reference stops silently here; this says so.
        let w = world();
        let pi = StrategyGraph::new("a", "never")
            .with_node(
                &w,
                "a",
                "budget.record_income",
                kwargs(&[
                    ("amount", json!(1.0)),
                    ("source", json!("wages")),
                    ("entry_id", json!("e1")),
                ]),
            )
            .unwrap()
            .with_edge("a", "a", None);

        let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
        assert_eq!(walk.outcome, WalkOutcome::StepsExhausted { limit: 10 });
        assert!(!walk.outcome.finished(), "it ran out; it did not finish");
        assert_eq!(walk.steps.len(), 10);
    }

    // ── the gate ─────────────────────────────────────────────────────────────

    #[test]
    fn a_node_the_gate_refuses_stops_the_walk_honestly() {
        // Allocating more than the balance is refused by the operator's own
        // guard — Π gets no bypass.
        let w = world();
        let pi = StrategyGraph::new("spend", "spend")
            .with_node(
                &w,
                "spend",
                "budget.allocate",
                kwargs(&[
                    ("pocket_name", json!("food")),
                    ("amount", json!(999.0)),
                    ("period", json!("monthly")),
                ]),
            )
            .unwrap();

        let walk = run(&pi, &Registry::default(), &allowed(), &quiet(), &state(), json!({}));
        match &walk.outcome {
            WalkOutcome::Refused { node, reason } => {
                assert_eq!(node, "spend");
                assert!(!reason.is_empty(), "the gate's own reason travels");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(walk.state, state(), "nothing committed");
    }

    #[test]
    fn an_operator_the_sustain_does_not_allow_is_refused_at_the_gate() {
        // In `T` for the world, not on this sustain's allow-list — two
        // different containments, and both are checked.
        let w = world();
        let pi = StrategyGraph::new("earn", "earn")
            .with_node(
                &w,
                "earn",
                "budget.record_income",
                kwargs(&[
                    ("amount", json!(1.0)),
                    ("source", json!("wages")),
                    ("entry_id", json!("e1")),
                ]),
            )
            .unwrap();
        let walk = run(&pi, &Registry::default(), &[], &quiet(), &state(), json!({}));
        assert!(matches!(walk.outcome, WalkOutcome::Refused { .. }));
    }

    // ── build-time shape and calibration ─────────────────────────────────────

    #[test]
    fn typecheck_catches_a_dangling_edge_and_a_missing_endpoint() {
        let w = world();
        let base = StrategyGraph::new("a", "a")
            .with_node(&w, "a", "budget.allocate", Map::new())
            .unwrap();
        assert!(matches!(
            base.clone().with_edge("a", "ghost", None).typecheck(),
            Err(StrategyError::DanglingEdge { .. })
        ));
        assert!(matches!(
            StrategyGraph::new("a", "missing")
                .with_node(&w, "a", "budget.allocate", Map::new())
                .unwrap()
                .typecheck(),
            Err(StrategyError::MissingEndpoint { which: "exit", .. })
        ));
    }

    #[test]
    fn calibration_resolves_a_placeholder_and_refuses_an_unsupplied_one() {
        let w = world();
        let pi = StrategyGraph::new("a", "a")
            .with_node(&w, "a", "budget.allocate", kwargs(&[("amount", json!("{{limit}}"))]))
            .unwrap();

        let cal: Map<String, Value> =
            [("limit".to_string(), json!(500.0))].into_iter().collect();
        let done = pi.clone().calibrated(&cal).unwrap();
        assert_eq!(done.nodes().next().unwrap().kwargs()["amount"], json!(500.0));

        assert!(matches!(
            pi.calibrated(&Map::new()),
            Err(StrategyError::Uncalibrated { token, .. }) if token == "limit"
        ));
    }

    #[test]
    fn the_two_resolution_passes_do_not_see_each_others_syntax() {
        // ★ `{{token}}` is build-time and `$path` is run-time; a calibration
        // pass must leave a `$` reference alone.
        let w = world();
        let pi = StrategyGraph::new("a", "a")
            .with_node(&w, "a", "budget.allocate", kwargs(&[("amount", json!("$earn.amount"))]))
            .unwrap()
            .calibrated(&Map::new())
            .unwrap();
        assert_eq!(pi.nodes().next().unwrap().kwargs()["amount"], json!("$earn.amount"));
    }
}
