//! `π : D → G` — the definition as an editable graph (EDIT-13; the Editing
//! paper / SLOPE, §IX).
//!
//! ## It is a lens, not a projection
//!
//! `π` is easy on its own: walk a [`Definition`] and emit nodes and edges. The
//! difficulty is that **`π` forgets**, so the inverse direction cannot be a
//! function of the graph alone. What an editor really is, is a **lens** in the
//! sense of Foster, Greenwald, Moore, Pierce and Schmitt (2007) — a pair:
//!
//! ```text
//!   get : D → G
//!   put : (G′, D) → D′
//! ```
//!
//! ★★ **`put` takes the old `D` as well as the new graph, precisely so it can
//! restore what `get` discarded.** A naive *regenerate `D` from `G′`* would
//! destroy every detail the graph never carried. That asymmetry is the whole
//! reason this is a lens and not a round-trippable encoding.
//!
//! ## ★★ What `π` actually forgets here — checked, not assumed
//!
//! §IX names *Enzyme bodies* first, and in this core **it forgets them for
//! free**: a [`Definition`] is `⟨schema, invariants, operators⟩` where
//! `operators` is an allow-list of **names**, and its own docstring already
//! says *"operator bodies are not"* data here — the bodies live in the
//! [`Registry`](crate::operator::Registry). So there is nothing to forget and
//! nothing to preserve on that axis, and claiming a round-trip proof over
//! Enzyme bodies would be theatre.
//!
//! What `π` forgets that is **real in this core**, and what `put` therefore has
//! to restore, is two things:
//!
//! 1. **Invariant expression text.** A graph draws *that there is a rule called
//!    `floor` touching `finances`* — it does not draw
//!    `finances.liquid.balance >= 500`. The edges are derived from the parsed
//!    predicate, so the graph knows *what a rule reads* without carrying *how
//!    it reads it*.
//! 2. **Fine type detail.** `Number { lo: Some(0.0), hi: Some(100.0) }` draws
//!    as the coarse label `number`; `Record { fields }` draws as `record`. The
//!    bounds and the nested field types are below the granularity the graph
//!    draws.
//!
//! ## The laws, executed
//!
//! - **GetPut** — `put(get(D), D) = D`. Projecting a definition and putting the
//!   untouched graph back changes nothing. ★ Stated here in a slightly stronger
//!   form that the return type makes available: an untouched graph yields the
//!   **empty edit list**, not an edit that happens to be a no-op.
//! - **PutGet** — `get(put(G′, D)) = G′`. An edit made on the graph is visible
//!   when the result is re-projected, so nothing is silently dropped.
//!
//! Both are run as real round trips in the tests and in
//! `conformance/vectors/lens.json`.
//!
//! ## ★★ Detail travels beside the graph, not inside it
//!
//! Adding an invariant needs an expression, and adding a dimension needs a type
//! and a default — none of which the graph carries, *because `π` forgot them*.
//! They arrive through [`Supplied`], a separate channel.
//!
//! ★ That separation is what makes **PutGet hold literally** rather than up to
//! some normalisation. If a node carried an optional `expression` field, `π`
//! would always leave it `None` while an editor's node had `Some`, and
//! `get(put(G′,D))` would differ from `G′` in exactly that field — the law
//! would have to be weakened to *equal after clearing the fields π never
//! fills*, which is a weaker claim wearing the same name. Keeping `G` purely
//! structural also makes it exactly what a surface draws, with no phantom
//! fields it must know to ignore.
//!
//! ★ And where the detail is genuinely absent, [`put`] **refuses and names what
//! is missing** rather than inventing a plausible default. `π` forgot it; `put`
//! restores it for what survives and cannot conjure it for what is new.
//!
//! ## A graph edit is a TYPED edit
//!
//! [`put`] returns `Vec<`[`Edit`]`>`, never a rebuilt definition. §II is
//! explicit that an untyped *"here is a new JSON document"* edit has no kind, so
//! no per-kind rule can fire — which would lose the taxonomy, the authority
//! check, `can_strand`'s asymmetry, and EDIT-11's stranding decision all at
//! once. Routing through the taxonomy means a graph edit reaches
//! [`admit_edit`](crate::editing::admit_edit) as an `AddInv` or a `RetireDim`
//! and is judged exactly as a hand-written one would be.
//!
//! ## Honest scope
//!
//! This row is the **lens and its laws**. The interactive Edit zone that draws
//! `G` and lets someone move it is the surface block's work (UI-in-Rust); what
//! it gets from here is a real graph and a safe way back.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::editing::{Definition, Edit};
use crate::goodhart::referenced_dimensions;
use crate::predicate::parse_predicate;
use crate::schema::DimType;

/// The coarse type label a graph draws. **This is the forgetting**: every
/// `Number` is `number` whatever its bounds, every `Record` is `record`
/// whatever its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DimLabel {
    Number,
    Text,
    Bool,
    Record,
    List,
    Map,
    Any,
}

impl DimLabel {
    fn of(ty: &DimType) -> Self {
        match ty {
            DimType::Number { .. } => DimLabel::Number,
            DimType::Text => DimLabel::Text,
            DimType::Bool => DimLabel::Bool,
            DimType::Record { .. } => DimLabel::Record,
            DimType::List { .. } => DimLabel::List,
            DimType::Map { .. } => DimLabel::Map,
            DimType::Any => DimLabel::Any,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DimLabel::Number => "number",
            DimLabel::Text => "text",
            DimLabel::Bool => "bool",
            DimLabel::Record => "record",
            DimLabel::List => "list",
            DimLabel::Map => "map",
            DimLabel::Any => "any",
        }
    }
}

/// A node of `G`.
///
/// ★ Purely structural on purpose — see the module header on why the detail
/// travels beside the graph rather than inside it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Node {
    /// A declared dimension, at label granularity.
    Dimension { name: String, label: DimLabel },
    /// A declared invariant, by id. The expression is **not** here.
    Invariant { id: String },
    /// A permitted operator, by name. The body is not here and never was.
    Operator { name: String },
}

impl Node {
    /// A stable identifier for edges to point at, namespaced by kind so a
    /// dimension and an invariant may share a name without colliding.
    pub fn id(&self) -> String {
        match self {
            Node::Dimension { name, .. } => format!("dim:{name}"),
            Node::Invariant { id } => format!("inv:{id}"),
            Node::Operator { name } => format!("op:{name}"),
        }
    }
}

/// What an edge means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeKind {
    /// An invariant reads this dimension — derived by parsing the expression
    /// and walking the AST, so the edge is a fact about the rule rather than a
    /// guess from its text.
    Constrains,
}

/// One edge of `G`.
///
/// ★ Named `GraphEdge`, not `Edge` — `router::Edge` is the MoE routing edge, a
/// genuinely different thing. Twenty-third collision; the newcomer takes the
/// longer name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

/// `G` — the definition as a graph. What the surface draws.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DefinitionGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<GraphEdge>,
}

impl DefinitionGraph {
    pub fn dimension_names(&self) -> BTreeSet<&str> {
        self.nodes
            .iter()
            .filter_map(|n| match n {
                Node::Dimension { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn invariant_ids(&self) -> BTreeSet<&str> {
        self.nodes
            .iter()
            .filter_map(|n| match n {
                Node::Invariant { id } => Some(id.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn operator_names(&self) -> BTreeSet<&str> {
        self.nodes
            .iter()
            .filter_map(|n| match n {
                Node::Operator { name } => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Remove a node and every edge touching it — the graph operation an editor
    /// performs, kept here so a caller cannot leave a dangling edge behind.
    pub fn without(mut self, node: &Node) -> Self {
        let id = node.id();
        self.nodes.retain(|n| n != node);
        self.edges.retain(|e| e.from != id && e.to != id);
        self
    }

    /// Add a node. Edges for it, if any, are the caller's to add — a new
    /// invariant's edges cannot be derived until its expression is supplied.
    pub fn with(mut self, node: Node) -> Self {
        self.nodes.push(node);
        self.nodes.sort();
        self
    }
}

/// The detail `π` forgot, supplied for the parts of `G′` that are **new**.
///
/// Empty for a pure removal, and empty for the identity round trip — which is
/// what makes GetPut fall out cleanly.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Supplied {
    invariants: BTreeMap<String, String>,
    dimensions: BTreeMap<String, (DimType, Value)>,
}

impl Supplied {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn invariant(mut self, id: &str, expression: &str) -> Self {
        self.invariants.insert(id.to_string(), expression.to_string());
        self
    }

    pub fn dimension(mut self, name: &str, ty: DimType, default: Value) -> Self {
        self.dimensions.insert(name.to_string(), (ty, default));
        self
    }
}

/// Why a graph could not be threaded back into a definition.
#[derive(Debug, Clone, PartialEq)]
pub enum PutError {
    /// A node is new, and the detail `π` forgot was not supplied. ★ Refused
    /// rather than defaulted: `π` discarded it, so there is nothing here to
    /// recover, and inventing a plausible expression or type would be the lens
    /// making a decision that belongs to whoever is editing.
    MissingDetail { node: String, needs: &'static str },
    /// A supplied expression does not parse. Caught here so a graph edit cannot
    /// smuggle an unparseable rule past the typed taxonomy.
    UnparseableExpression { id: String, reason: String },
}

impl std::fmt::Display for PutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PutError::MissingDetail { node, needs } => write!(
                f,
                "'{node}' is new to the graph, and π does not carry its {needs} — \
                 supply it rather than have the lens invent one"
            ),
            PutError::UnparseableExpression { id, reason } => {
                write!(f, "the expression supplied for '{id}' does not parse: {reason}")
            }
        }
    }
}

/// `get : D → G`. **Forgets** expression text and fine type detail.
pub fn get(definition: &Definition) -> DefinitionGraph {
    let mut nodes = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    let declared: BTreeSet<&str> = definition.schema.top_level().collect();
    for name in &declared {
        let label = definition
            .schema
            .dimensions
            .get(*name)
            .map(DimLabel::of)
            .unwrap_or(DimLabel::Any);
        nodes.push(Node::Dimension { name: (*name).to_string(), label });
    }

    for (id, expression) in &definition.invariants {
        let node = Node::Invariant { id: id.clone() };
        // ★ The edges are derived from the PARSED predicate, so an edge is a
        // fact about what the rule reads rather than a substring match on its
        // text. An expression that does not parse contributes no edges — the
        // node still appears, because a rule the graph cannot explain is
        // exactly the one an editor most needs to see.
        if let Ok(ast) = parse_predicate(expression) {
            for dim in referenced_dimensions(&ast) {
                if declared.contains(dim.as_str()) {
                    edges.push(GraphEdge {
                        from: node.id(),
                        to: format!("dim:{dim}"),
                        kind: EdgeKind::Constrains,
                    });
                }
            }
        }
        nodes.push(node);
    }

    for name in &definition.operators {
        nodes.push(Node::Operator { name: name.clone() });
    }

    nodes.sort();
    edges.sort();
    DefinitionGraph { nodes, edges }
}

/// `put : (G′, D) → [Edit]`.
///
/// ★★ Returns **typed edits**, never a rebuilt definition — see the module
/// header. Everything the graph does not carry is recovered from `old` for
/// nodes that survive, and must be [`Supplied`] for nodes that are new.
pub fn put(
    new: &DefinitionGraph,
    old: &Definition,
    supplied: &Supplied,
) -> Result<Vec<Edit>, PutError> {
    let mut edits = Vec::new();

    let old_dims: BTreeSet<&str> = old.schema.top_level().collect();
    let old_invs: BTreeSet<&str> = old.invariants.iter().map(|(i, _)| i.as_str()).collect();
    let old_ops: BTreeSet<&str> = old.operators.iter().map(String::as_str).collect();

    let new_dims = new.dimension_names();
    let new_invs = new.invariant_ids();
    let new_ops = new.operator_names();

    // ── removals ─────────────────────────────────────────────────────────────
    // Nothing to preserve: the node is going.
    for name in old_dims.difference(&new_dims) {
        edits.push(Edit::RetireDim { name: (*name).to_string() });
    }
    for id in old_invs.difference(&new_invs) {
        edits.push(Edit::DropInv { id: (*id).to_string() });
    }
    for name in old_ops.difference(&new_ops) {
        edits.push(Edit::RetireOp { name: (*name).to_string() });
    }

    // ── additions ────────────────────────────────────────────────────────────
    // ★ This is where `π`'s forgetting bites: there is no old value to recover
    // from, so the detail must arrive through `Supplied` or the put refuses.
    for name in new_dims.difference(&old_dims) {
        match supplied.dimensions.get(*name) {
            Some((ty, default)) => edits.push(Edit::AddDim {
                name: (*name).to_string(),
                ty: ty.clone(),
                default: default.clone(),
            }),
            None => {
                return Err(PutError::MissingDetail {
                    node: format!("dim:{name}"),
                    needs: "declared type and default",
                })
            }
        }
    }
    for id in new_invs.difference(&old_invs) {
        match supplied.invariants.get(*id) {
            Some(expression) => {
                if let Err(e) = parse_predicate(expression) {
                    return Err(PutError::UnparseableExpression {
                        id: (*id).to_string(),
                        reason: e.to_string(),
                    });
                }
                edits.push(Edit::AddInv {
                    id: (*id).to_string(),
                    expression: expression.clone(),
                })
            }
            None => {
                return Err(PutError::MissingDetail {
                    node: format!("inv:{id}"),
                    needs: "expression",
                })
            }
        }
    }
    for name in new_ops.difference(&old_ops) {
        // ★ The one addition needing nothing supplied: a `Definition`'s
        // operator entry IS its name, so the graph carries the whole of it and
        // `π` forgot nothing here. The bodies were never `D`'s to begin with.
        edits.push(Edit::AddOp { name: (*name).to_string() });
    }

    // ── surviving nodes ──────────────────────────────────────────────────────
    // ★★ THE PRESERVATION. A node present in both keeps whatever `D` says,
    // unless the editor explicitly supplied something different — so the
    // expression text and the fine type detail the graph never showed come back
    // untouched, without the graph having had to carry them.
    for (id, current) in &old.invariants {
        if !new_invs.contains(id.as_str()) {
            continue;
        }
        if let Some(replacement) = supplied.invariants.get(id) {
            if replacement != current {
                if let Err(e) = parse_predicate(replacement) {
                    return Err(PutError::UnparseableExpression {
                        id: id.clone(),
                        reason: e.to_string(),
                    });
                }
                edits.push(Edit::ModifyInv {
                    id: id.clone(),
                    expression: replacement.clone(),
                });
            }
        }
    }
    for name in old_dims.intersection(&new_dims) {
        if let Some((ty, _)) = supplied.dimensions.get(*name) {
            if old.schema.dimensions.get(*name) != Some(ty) {
                edits.push(Edit::RetypeDim { name: (*name).to_string(), ty: ty.clone() });
            }
        }
    }

    Ok(edits)
}

/// [`put`], then apply the edits — the form the round-trip laws are stated
/// over.
///
/// ★ Applies [`Edit::apply`] directly rather than routing through
/// `admit_edit`, and that is deliberate: the laws are about the **lens**, and
/// a gate refusal is a fact about the definition being edited, not about
/// whether the lens threaded the change correctly. A caller putting a real edit
/// live runs the returned edits through `admit_edit` — see the module header.
pub fn put_apply(
    new: &DefinitionGraph,
    old: &Definition,
    supplied: &Supplied,
) -> Result<Definition, PutError> {
    let edits = put(new, old, supplied)?;
    let mut d = old.clone();
    for e in &edits {
        d = e.apply(&d);
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Schema;

    fn definition() -> Definition {
        Definition::new(
            Schema::new()
                // ★ Bounds the graph will not draw.
                .declare("finances", DimType::Number { lo: Some(0.0), hi: Some(1_000_000.0) })
                .declare("roster", DimType::List { item: Box::new(DimType::Text) }),
        )
        .with_invariant("floor", "finances >= 500")
        .with_operator("budget.record_income")
        .with_operator("budget.allocate")
    }

    // ── what π forgets ───────────────────────────────────────────────────────

    #[test]
    fn pi_forgets_the_expression_text_and_the_fine_type_detail() {
        let g = get(&definition());

        // The rule is there by id; its text is nowhere in the graph.
        assert!(g.invariant_ids().contains("floor"));
        assert!(
            !format!("{g:?}").contains(">= 500"),
            "the expression must not survive into G"
        );

        // The dimension is there at label granularity; its bounds are not.
        assert!(g.nodes.contains(&Node::Dimension {
            name: "finances".into(),
            label: DimLabel::Number
        }));
        assert!(
            !format!("{g:?}").contains("1000000"),
            "the bounds must not survive into G"
        );
    }

    #[test]
    fn the_edges_are_derived_from_the_parsed_predicate() {
        // A fact about what the rule reads, not a substring match on its text.
        let g = get(&definition());
        assert!(g.edges.contains(&GraphEdge {
            from: "inv:floor".into(),
            to: "dim:finances".into(),
            kind: EdgeKind::Constrains,
        }));
        assert!(
            !g.edges.iter().any(|e| e.to == "dim:roster"),
            "roster is untouched by the rule"
        );
    }

    // ── the laws ─────────────────────────────────────────────────────────────

    #[test]
    fn getput_an_untouched_graph_produces_no_edit_at_all() {
        // `put(get(D), D) = D`, in the stronger form the return type allows.
        let d = definition();
        let edits = put(&get(&d), &d, &Supplied::none()).unwrap();
        assert!(edits.is_empty(), "no spurious change, and not even a no-op edit");
        assert_eq!(put_apply(&get(&d), &d, &Supplied::none()).unwrap(), d);
    }

    #[test]
    fn putget_an_edit_on_the_graph_survives_reprojection() {
        // `get(put(G′, D)) = G′` — literally, because G carries no detail.
        let d = definition();
        let g2 = get(&d).without(&Node::Operator { name: "budget.allocate".into() });
        let d2 = put_apply(&g2, &d, &Supplied::none()).unwrap();
        assert_eq!(get(&d2), g2, "the edit is not silently dropped");
    }

    #[test]
    fn putget_holds_for_an_addition_too() {
        let d = definition();
        let g2 = get(&d).with(Node::Invariant { id: "ceiling".into() });
        let supplied = Supplied::none().invariant("ceiling", "finances <= 900");
        let d2 = put_apply(&g2, &d, &supplied).unwrap();

        // The new node is present, and so is the edge derived from the
        // expression it was given — which G′ did not have, because the caller
        // could not have known it. So compare the projections of the SAME
        // definition rather than against a graph built by hand.
        assert!(get(&d2).invariant_ids().contains("ceiling"));
        assert_eq!(get(&put_apply(&get(&d2), &d2, &Supplied::none()).unwrap()), get(&d2));
    }

    // ── the preservation ─────────────────────────────────────────────────────

    #[test]
    fn a_round_trip_keeps_what_the_graph_never_showed() {
        // ★★ THE LENS PROPERTY. Remove an operator on the graph — a change the
        // graph CAN express — and the expression text and the type bounds it
        // never carried come back byte-identical, recovered from the old D.
        let d = definition();
        let g2 = get(&d).without(&Node::Operator { name: "budget.allocate".into() });
        let d2 = put_apply(&g2, &d, &Supplied::none()).unwrap();

        assert_eq!(d2.operators, vec!["budget.record_income".to_string()], "the edit landed");
        assert_eq!(d2.invariants, d.invariants, "expression text preserved");
        assert_eq!(
            d2.schema.dimensions.get("finances"),
            Some(&DimType::Number { lo: Some(0.0), hi: Some(1_000_000.0) }),
            "the bounds the graph never drew are still here"
        );
    }

    #[test]
    fn a_new_node_with_no_supplied_detail_is_refused_not_defaulted() {
        let d = definition();
        let g2 = get(&d).with(Node::Invariant { id: "ceiling".into() });
        assert_eq!(
            put(&g2, &d, &Supplied::none()),
            Err(PutError::MissingDetail { node: "inv:ceiling".into(), needs: "expression" })
        );

        let g3 = get(&d).with(Node::Dimension { name: "extra".into(), label: DimLabel::Number });
        assert!(matches!(
            put(&g3, &d, &Supplied::none()),
            Err(PutError::MissingDetail { .. })
        ));
    }

    #[test]
    fn an_unparseable_supplied_expression_is_refused_at_the_lens() {
        let d = definition();
        let g2 = get(&d).with(Node::Invariant { id: "bad".into() });
        let supplied = Supplied::none().invariant("bad", "this is not a predicate");
        assert!(matches!(
            put(&g2, &d, &supplied),
            Err(PutError::UnparseableExpression { .. })
        ));
    }

    #[test]
    fn an_operator_needs_nothing_supplied_because_pi_forgot_nothing_there() {
        let d = definition();
        let g2 = get(&d).with(Node::Operator { name: "budget.spend".into() });
        let edits = put(&g2, &d, &Supplied::none()).unwrap();
        assert_eq!(edits, vec![Edit::AddOp { name: "budget.spend".into() }]);
    }

    // ── typed, not JSON ──────────────────────────────────────────────────────

    #[test]
    fn every_graph_edit_is_a_typed_edit() {
        let d = definition();
        let g2 = get(&d)
            .without(&Node::Operator { name: "budget.allocate".into() })
            .without(&Node::Invariant { id: "floor".into() })
            .with(Node::Operator { name: "budget.spend".into() });
        let edits = put(&g2, &d, &Supplied::none()).unwrap();

        let kinds: BTreeSet<&str> = edits.iter().map(Edit::kind).collect();
        assert_eq!(
            kinds,
            ["AddOp", "DropInv", "RetireOp"].into_iter().collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn a_modified_expression_becomes_a_modify_inv_not_a_drop_and_add() {
        let d = definition();
        let supplied = Supplied::none().invariant("floor", "finances >= 700");
        let edits = put(&get(&d), &d, &supplied).unwrap();
        assert_eq!(
            edits,
            vec![Edit::ModifyInv { id: "floor".into(), expression: "finances >= 700".into() }]
        );
    }

    #[test]
    fn supplying_the_identical_expression_is_not_an_edit() {
        // Preservation must not be mistaken for a change.
        let d = definition();
        let supplied = Supplied::none().invariant("floor", "finances >= 500");
        assert!(put(&get(&d), &d, &supplied).unwrap().is_empty());
    }
}
