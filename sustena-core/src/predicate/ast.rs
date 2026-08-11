//! The typed predicate AST — the Viable region V, as a tree rather than a string.
//!
//! GENOME/DSL specifies a declarative language that compiles to a typed AST the
//! gate runs. Nothing here evaluates strings at runtime; there is no `eval`,
//! no dynamic dispatch on user text. A predicate is parsed once into these
//! nodes, and the nodes are all that is ever executed.

use serde_json::Value;

/// A path into state: `finances.pockets[*].allocated`.
#[derive(Debug, Clone, PartialEq)]
pub struct StatePath {
    /// The original text, used verbatim in failure messages.
    pub raw: String,
    pub segments: Vec<PathSegment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    Name(String),
    Index(usize),
    /// `[*]` — map the remaining segments over every element.
    Wildcard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggFunc {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

impl AggFunc {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "SUM" => Some(Self::Sum),
            "COUNT" => Some(Self::Count),
            "AVG" => Some(Self::Avg),
            "MIN" => Some(Self::Min),
            "MAX" => Some(Self::Max),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Sum => "SUM",
            Self::Count => "COUNT",
            Self::Avg => "AVG",
            Self::Min => "MIN",
            Self::Max => "MAX",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompOp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
}

impl CompOp {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            ">" => Some(Self::Gt),
            ">=" => Some(Self::Ge),
            "<" => Some(Self::Lt),
            "<=" => Some(Self::Le),
            "==" => Some(Self::Eq),
            "!=" => Some(Self::Ne),
            _ => None,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Eq => "==",
            Self::Ne => "!=",
        }
    }

    /// True for the four that need mutually comparable operands. Equality
    /// works across any pair; ordering does not.
    pub fn is_ordering(&self) -> bool {
        !matches!(self, Self::Eq | Self::Ne)
    }
}

/// Something that produces a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Literal(Value),
    /// `params.amount` — a value from the operator call, not from state.
    Param(String),
    Path(StatePath),
    Aggregate { func: AggFunc, path: StatePath },
    List(Vec<Value>),
}

/// Something that produces a verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    Comparison {
        left: Operand,
        op: CompOp,
        right: Operand,
    },
    Membership {
        left: Operand,
        negate: bool,
        right: Operand,
    },
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
    Not(Box<Predicate>),
    /// `ALL pockets[*].allocated >= 0` — the body runs once per item, with the
    /// item as the scope a bare name resolves against first.
    Quantifier {
        kind: QuantKind,
        list_path: StatePath,
        body: Box<Predicate>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantKind {
    All,
    Exists,
}

impl QuantKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Exists => "EXISTS",
        }
    }
}

impl Operand {
    /// Human-legible label used in the failure reasons a person reads.
    pub fn describe(&self) -> String {
        match self {
            Operand::Literal(v) => crate::predicate::eval::py_repr(v),
            Operand::Param(k) => format!("params.{k}"),
            Operand::Path(p) => p.raw.clone(),
            Operand::Aggregate { func, path } => format!("{}({})", func.name(), path.raw),
            Operand::List(items) => format!("{items:?}"),
        }
    }
}
