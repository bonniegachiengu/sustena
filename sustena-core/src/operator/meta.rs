//! Operator metadata and the registry.
//!
//! An operator is metadata plus a body. The metadata is the declaration the
//! engine enforces: what must hold before, what may be emitted, what must hold
//! after. It is data, not documentation.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::operator::EmittedEvent;
use crate::compose::{EffectSummary, Step};
use crate::flow::Movement;
use crate::state::State;

/// The body of an operator. It may only reach state through `State`, which
/// records every change — there is no way to write without recording.
/// The effect.
///
/// ★ Returns the candidate state (through `&mut State`), the events, **and the
/// movements** — Constraint §II's `candidate, flows = o.effect(...)`. The
/// movements are the fourth parameter rather than a return value only because
/// Rust has no multiple return; the article's point is that the effect
/// *produces* them, and it does.
///
/// A movement is **declared, never inferred from the endpoints**. If flows
/// could be recovered by diffing before and after, `F` would be reducible to
/// `D` — see `crate::flow`'s irreducibility case. Most operators declare none,
/// and `F` is then vacuously satisfied.
pub type OperatorFn =
    fn(&mut State, &Map<String, Value>, &mut Vec<EmittedEvent>, &mut Vec<Movement>)
        -> OperatorResult;

#[derive(Debug, Clone, PartialEq)]
pub struct OperatorResult {
    pub status: OperatorStatus,
    pub data: Value,
    pub reason: Option<String>,
    /// Which rule refused it — a guard expression, or a well-known marker such
    /// as `enforcement_gate`.
    pub constraint_violated: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorStatus {
    Ok,
    Failed,
    /// Awaiting a Council vote. Not a failure — a decision that is not yet made.
    Deferred,
}

impl OperatorResult {
    pub fn ok(data: Value) -> Self {
        Self { status: OperatorStatus::Ok, data, reason: None, constraint_violated: None }
    }

    pub fn fail(reason: impl Into<String>, constraint: impl Into<String>) -> Self {
        Self {
            status: OperatorStatus::Failed,
            data: Value::Null,
            reason: Some(reason.into()),
            constraint_violated: Some(constraint.into()),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.status == OperatorStatus::Ok
    }
}

/// What kind of value a parameter takes.
///
/// Three, and deliberately coarse: this is enough for a proposer to know *how
/// to look for* a value in a description, and no more. A richer type lattice
/// belongs to the DSL, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Number,
    Text,
    Any,
}

/// One parameter an operator declares.
///
/// ★★ **Declared, not introspected — and that is a real difference from the
/// reference, named rather than smoothed over.** Python's `ℐ` reads
/// `inspect.signature(meta.fn)`, so its parameter list cannot drift from the
/// body. A Rust operator is an `OperatorFn` reading a `Map<String, Value>` by
/// name, so there is **no signature to introspect** and the honest analogue is
/// a declaration. That trades one risk for another and both are worth stating:
/// a declaration **can** disagree with the body (introspection cannot), but it
/// **is** the contract, where an introspected signature is only an accident of
/// how the body was written.
///
/// An operator that declares **no** parameters is not a lie — it is a
/// statement that a proposer cannot build `θ` for it, and
/// [`crate::enzyme::propose`] reports that as a **named gap** rather than
/// guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl {
    pub name: &'static str,
    pub kind: ParamKind,
    /// Whether `θ` is incomplete without it.
    pub required: bool,
    /// ★★★ A state path whose **keys** are the legal values — e.g.
    /// `finances.pockets` for a pocket name.
    ///
    /// This is what lets a proposer match a value against *the sustain's own
    /// live state* without the core knowing what a pocket is. The reference
    /// hardcodes pocket-matching; declaring the path instead keeps the
    /// mechanism generic over any sustain that names a collection.
    pub names_within: Option<&'static str>,
}

impl ParamDecl {
    pub fn number(name: &'static str) -> Self {
        ParamDecl { name, kind: ParamKind::Number, required: true, names_within: None }
    }

    pub fn text(name: &'static str) -> Self {
        ParamDecl { name, kind: ParamKind::Text, required: true, names_within: None }
    }

    /// A name drawn from a live-state collection.
    pub fn naming(name: &'static str, within: &'static str) -> Self {
        ParamDecl {
            name,
            kind: ParamKind::Text,
            required: true,
            names_within: Some(within),
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}

/// How an operator is triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Rpc,
    EventDriven,
    Polling,
    Streaming,
}

#[derive(Clone)]
pub struct OperatorMeta {
    pub name: &'static str,
    pub description: &'static str,
    /// ★★ `θ`'s shape, **declared**. See [`ParamDecl`] for why this is a
    /// declaration here where the reference introspects a signature.
    pub params: Vec<ParamDecl>,
    /// Pre-conditions. Checked before the body runs; a failure runs nothing.
    pub constraints: Vec<String>,
    /// Post-conditions, checked against the MUTATED state before commit.
    pub post_constraints: Vec<String>,
    /// Events this operator may publish.
    pub side_effects: Vec<&'static str>,
    pub pawa_cost: u32,
    pub protocol: Protocol,
    /// Minimum access tier. Declared and, in the reference engine, never read —
    /// an open gap (ENZYME / TOLERANCE) carried here as data so R2 can enforce
    /// it without changing the operator declarations again.
    pub min_privilege: u8,
    /// ★ What the body does, declared symbolically — Operator §III's `e_o` in
    /// the form `wp` can pull a postcondition back through.
    ///
    /// **Optional and absent by default.** A summary is a *claim* about the
    /// body, and checking the claim against the body is a different, larger
    /// row — so an operator without one yields `Unavailable` rather than a
    /// guess. See [`crate::obligation`].
    pub effect: Option<EffectSummary>,
    pub run: OperatorFn,
}
impl OperatorMeta {
    /// **This operator, as composition needs to see it.**
    ///
    /// ★★★ The bridge that was missing. `compose.rs` has held Hoare sequencing
    /// since it was written, and nothing ever handed it a real operator — so a
    /// chain that could never run was only ever discovered by running it. Every
    /// part is read off the declaration that already exists: the guard is the
    /// operator's own constraints, the postcondition its own post-constraints,
    /// the emissions its own declared side effects.
    ///
    /// ★★ Nothing is invented. An operator with no `EffectSummary` yields a
    /// step with no effect, and composition then declines to pull a successor's
    /// guard back through it rather than guessing what it did — the same
    /// honesty `obligation` already applies to the same absence.
    pub fn as_step(&self) -> Step {
        Step {
            name: self.name.to_string(),
            guard: self.constraints.iter().map(|c| c.to_string()).collect(),
            post: self.post_constraints.iter().map(|c| c.to_string()).collect(),
            emits: self.side_effects.iter().map(|e| e.to_string()).collect(),
            pawa_cost: self.pawa_cost,
            effect: self.effect.clone(),
        }
    }
}


impl std::fmt::Debug for OperatorMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OperatorMeta").field("name", &self.name).finish()
    }
}

/// The set of operators this core knows how to run.
#[derive(Debug, Clone)]
pub struct Registry {
    ops: BTreeMap<String, OperatorMeta>,
}

impl Registry {
    pub fn new() -> Self {
        Self { ops: BTreeMap::new() }
    }

    pub fn register(&mut self, meta: OperatorMeta) {
        self.ops.insert(meta.name.to_string(), meta);
    }

    pub fn get(&self, name: &str) -> Option<&OperatorMeta> {
        self.ops.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.ops.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl Registry {
    /// Every operator this core implements.
    pub fn with_builtins() -> Self {
        let mut r = Registry::new();
        crate::operator::budget::register(&mut r);
        crate::operator::device::register(&mut r);
        crate::operator::inventory::register(&mut r);
        crate::operator::vendor::register(&mut r);
        r
    }
}

/// `Default` builds the FULL set on purpose. An empty default would let a
/// caller run against nothing and conclude an operator does not exist, which
/// is indistinguishable from it being forbidden.
impl Default for Registry {
    fn default() -> Self {
        Self::with_builtins()
    }
}
