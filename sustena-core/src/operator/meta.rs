//! Operator metadata and the registry.
//!
//! An operator is metadata plus a body. The metadata is the declaration the
//! engine enforces: what must hold before, what may be emitted, what must hold
//! after. It is data, not documentation.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::operator::EmittedEvent;
use crate::state::State;

/// The body of an operator. It may only reach state through `State`, which
/// records every change — there is no way to write without recording.
pub type OperatorFn = fn(&mut State, &Map<String, Value>, &mut Vec<EmittedEvent>) -> OperatorResult;

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
    pub run: OperatorFn,
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
