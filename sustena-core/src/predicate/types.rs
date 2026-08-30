//! **`Γ ⊢ e : τ`** — the type judgment (DSL · GENOME §V).
//!
//! ```text
//!   Γ ⊢ l : τ    Γ ⊢ r : τ    ordered(τ)
//!   ────────────────────────────────────    Γ ⊢ (l ⋈ r) : Bool
//! ```
//!
//! ★★★ **Why this is the DSL's key gap, and it is not about elegance.** The
//! gate has three outcomes — refuse, clamp, defer — and **no fourth for "this
//! rule is broken"**. So an untyped mistake arrives dressed as a *refusal*: a
//! rule comparing a pocket's name to a number does not report itself as
//! nonsense, it reports that the household broke a law. Somebody then goes
//! looking for the money that moved, and there is none, because nothing
//! happened except that a document was wrong.
//!
//! > *Well-typed programs can't go wrong.* — Milner, 1978
//!
//! Here that is load-bearing rather than decorative: a predicate that survives
//! this cannot produce a comparison whose operands were never comparable, so
//! every refusal the gate reports afterwards is a real one.
//!
//! ## What the article settles, and what it leaves
//!
//! ★★ **Ordering needs the same type; equality does not.** `a == b` across two
//! types is honestly `false` — they are different values — and refusing it
//! would outlaw a perfectly good "is this the empty string" test. `a > b`
//! across two types has no answer at all, so it is a type error, not a verdict.
//!
//! ★★★ **An untyped dimension is `Any`, and `Any` is compatible with
//! everything.** `Any` exists in this schema so a definition can be adopted
//! one dimension at a time, and a type-checker that refused every `Any`
//! comparison would make adopting the checker cost a full re-declaration up
//! front. Every `Any` is an admission that nothing was said, and the honest
//! response to nothing-was-said is not to invent a complaint.
//!
//! ★★ **Params are typed from the operator's own declaration.** `params.amount`
//! is not a dimension of `S` — it comes from the call — so `Γ` carries the
//! operator's `ParamDecl`s alongside the schema. An operator that declares
//! nothing yields `Unknown`, which behaves exactly like `Any`: unchecked
//! because unstated.

use std::collections::BTreeMap;

use serde_json::Value;

use super::ast::{AggFunc, CompOp, Operand, Predicate, StatePath};
use crate::schema::{DimType, Schema, TypeError};

/// A type, as the judgment needs to see it.
///
/// ★ Deliberately coarser than [`DimType`]: bounds on a number do not change
/// what may be compared with it, and carrying them here would invite a checker
/// that started refusing arithmetic it cannot actually evaluate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Number,
    Text,
    Bool,
    List(Box<Ty>),
    /// A record or an open map — has fields, is not itself comparable.
    Structure,
    /// Nothing was declared. Compatible with everything, on purpose.
    Unknown,
}

impl Ty {
    /// Can values of this type be put in order?
    ///
    /// ★★ Text is ordered here because the evaluator really does order it, and
    /// a checker that called a comparison the evaluator performs a type error
    /// would be describing a different language from the one that runs.
    pub fn ordered(&self) -> bool {
        matches!(self, Self::Number | Self::Text | Self::Unknown)
    }

    /// Is a comparison between these two types meaningful?
    ///
    /// ★★★ `Unknown` says yes to everything. It is the shape of "nothing was
    /// declared", and inventing a complaint about silence would make adopting
    /// the checker cost a full re-declaration before it could say anything.
    pub fn compatible_with(&self, other: &Ty) -> bool {
        match (self, other) {
            (Ty::Unknown, _) | (_, Ty::Unknown) => true,
            (Ty::List(a), Ty::List(b)) => a.compatible_with(b),
            (a, b) => a == b,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Ty::Number => "a number".into(),
            Ty::Text => "text".into(),
            Ty::Bool => "true or false".into(),
            Ty::List(item) => format!("a list of {}", item.name()),
            Ty::Structure => "a record".into(),
            Ty::Unknown => "something undeclared".into(),
        }
    }
}

impl From<&DimType> for Ty {
    fn from(d: &DimType) -> Self {
        match d {
            DimType::Number { .. } => Ty::Number,
            DimType::Text => Ty::Text,
            DimType::Bool => Ty::Bool,
            DimType::List { item } => Ty::List(Box::new(Ty::from(item.as_ref()))),
            DimType::Record { .. } | DimType::Map { .. } => Ty::Structure,
            DimType::Any => Ty::Unknown,
        }
    }
}

/// The typing context: what the schema declares, plus what the call declares.
#[derive(Debug, Clone)]
pub struct Gamma<'a> {
    pub schema: &'a Schema,
    /// `params.*`, from the operator's own `ParamDecl`s.
    pub params: BTreeMap<String, Ty>,
}

impl<'a> Gamma<'a> {
    pub fn new(schema: &'a Schema) -> Self {
        Self { schema, params: BTreeMap::new() }
    }

    pub fn with_param(mut self, name: &str, ty: Ty) -> Self {
        self.params.insert(name.to_string(), ty);
        self
    }
}

/// The declared type at a path.
///
/// ★★★ `Schema::resolve` already walks paths — through records, through open
/// maps, and across a wildcard into what a container holds. Writing a second
/// walker here would have been a second answer to the same question, free to
/// drift from the one the binder and the evaluator agree on. `Ty` is a
/// deliberate forgetting of `DimType`, so it happens at the END of the walk
/// rather than at each step.
fn type_at(schema: &Schema, path: &StatePath) -> Ty {
    schema.resolve(&path.segments).map(Ty::from).unwrap_or(Ty::Unknown)
}

/// The type of a literal, read off the value it is.
fn literal_ty(v: &Value) -> Ty {
    match v {
        Value::Number(_) => Ty::Number,
        Value::String(_) => Ty::Text,
        Value::Bool(_) => Ty::Bool,
        Value::Array(items) => {
            // ★ A mixed list types as a list of Unknown rather than as an
            //   error: `IN [1, "any"]` is a membership test somebody may
            //   honestly mean, and equality across types is already defined.
            let first = items.first().map(literal_ty).unwrap_or(Ty::Unknown);
            let uniform = items.iter().all(|i| literal_ty(i) == first);
            Ty::List(Box::new(if uniform { first } else { Ty::Unknown }))
        }
        Value::Null | Value::Object(_) => Ty::Unknown,
    }
}

/// `Γ ⊢ e : τ` for an operand.
fn operand_ty(operand: &Operand, gamma: &Gamma, scope: Option<&StatePath>) -> Ty {
    match operand {
        Operand::Literal(v) => literal_ty(v),
        Operand::List(items) => literal_ty(&Value::Array(items.clone())),
        Operand::Param(name) => gamma.params.get(name).cloned().unwrap_or(Ty::Unknown),
        Operand::Path(p) => {
            let direct = type_at(gamma.schema, p);
            // Inside a quantifier a bare name may be the item's own field. The
            // evaluator tries the item first, so the judgment does too.
            if direct != Ty::Unknown {
                return direct;
            }
            match scope {
                Some(s) => type_at(gamma.schema, &joined(s, p)),
                None => Ty::Unknown,
            }
        }
        // ★★★ COUNT is a number whatever it counts. The others are a number
        //     only if what they range over is one — which is exactly the
        //     "summing a string" mistake this judgment exists to catch.
        Operand::Aggregate { func, path } => match func {
            AggFunc::Count => Ty::Number,
            _ => {
                let inner = type_at(gamma.schema, path);
                let inner = if inner == Ty::Unknown {
                    match scope {
                        Some(s) => type_at(gamma.schema, &joined(s, path)),
                        None => Ty::Unknown,
                    }
                } else {
                    inner
                };
                match inner {
                    Ty::List(item) => *item,
                    other => other,
                }
            }
        },
    }
}

fn joined(scope: &StatePath, path: &StatePath) -> StatePath {
    use super::ast::PathSegment;
    let mut segments = scope.segments.clone();
    segments.push(PathSegment::Wildcard);
    segments.extend(path.segments.iter().cloned());
    StatePath { raw: format!("{}[*].{}", scope.raw, path.raw), segments }
}

/// **The judgment.** Empty ⟺ every comparison in the predicate is between
/// operands that could be compared.
///
/// ★★★ Run at AUTHOR time. A predicate that survives cannot reach the gate and
/// be reported as a violated law when it was only ever a broken document.
pub fn typecheck(predicate: &Predicate, gamma: &Gamma) -> Vec<TypeError> {
    let mut errors = Vec::new();
    check(predicate, gamma, None, &mut errors);
    errors
}

fn check(
    node: &Predicate,
    gamma: &Gamma,
    scope: Option<&StatePath>,
    errors: &mut Vec<TypeError>,
) {
    match node {
        Predicate::And(parts) | Predicate::Or(parts) => {
            for p in parts {
                check(p, gamma, scope, errors);
            }
        }
        Predicate::Not(inner) => check(inner, gamma, scope, errors),

        Predicate::Comparison { left, op, right } => {
            let (lt, rt) = (operand_ty(left, gamma, scope), operand_ty(right, gamma, scope));
            let shown = describe(left, *op, right);

            // ★★★ **Equality is total; ordering is not.** `a == b` across two
            //     types is honestly `false` — they are different values — and
            //     the judgment has nothing to add. `a > b` across two types has
            //     no answer to give, so it is a broken document rather than a
            //     verdict, and that is the whole distinction this catches.
            if !op.is_ordering() {
                return;
            }
            if !lt.compatible_with(&rt) {
                errors.push(TypeError {
                    path: shown,
                    detail: format!(
                        "puts {} in order with {} — these are different kinds of thing",
                        lt.name(),
                        rt.name()
                    ),
                });
                return;
            }
            // ★★ Two records are equal or they are not; neither is GREATER.
            if !(lt.ordered() && rt.ordered()) {
                let unordered = if lt.ordered() { &rt } else { &lt };
                errors.push(TypeError {
                    path: shown,
                    detail: format!(
                        "puts {} in order with `{}`, and that has no meaning",
                        unordered.name(),
                        op.symbol()
                    ),
                });
            }
        }

        Predicate::Membership { left, right, .. } => {
            let lt = operand_ty(left, gamma, scope);
            match operand_ty(right, gamma, scope) {
                Ty::Unknown => {}
                Ty::List(item) => {
                    if !lt.compatible_with(&item) {
                        errors.push(TypeError {
                            path: format!("{} IN …", operand_name(left)),
                            detail: format!(
                                "looks for {} in a list of {}",
                                lt.name(),
                                item.name()
                            ),
                        });
                    }
                }
                other => errors.push(TypeError {
                    path: format!("{} IN …", operand_name(left)),
                    detail: format!("looks inside {}, which is not a list", other.name()),
                }),
            }
        }

        Predicate::Quantifier { list_path, body, .. } => {
            // ★★ A quantifier ranges over a container. Ranging over a single
            //    number is not a narrower loop, it is a mistake.
            match type_at(gamma.schema, list_path) {
                Ty::List(_) | Ty::Structure | Ty::Unknown => {}
                other => errors.push(TypeError {
                    path: list_path.raw.clone(),
                    detail: format!("is {}, so there is nothing to range over", other.name()),
                }),
            }
            check(body, gamma, Some(list_path), errors);
        }
    }
}

fn operand_name(o: &Operand) -> String {
    match o {
        Operand::Path(p) => p.raw.clone(),
        Operand::Param(n) => format!("params.{n}"),
        Operand::Aggregate { func, path } => format!("{}({})", func.name(), path.raw),
        Operand::Literal(v) => v.to_string(),
        Operand::List(_) => "a list".into(),
    }
}

fn describe(left: &Operand, op: CompOp, right: &Operand) -> String {
    format!("{} {} {}", operand_name(left), op.symbol(), operand_name(right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate::parse_predicate;
    use crate::schema::Schema;

    fn household() -> Schema {
        use std::collections::BTreeMap;
        let mut pocket = BTreeMap::new();
        pocket.insert("allocated".to_string(), DimType::Number { lo: None, hi: None });
        pocket.insert("spent".to_string(), DimType::Number { lo: None, hi: None });
        pocket.insert("label".to_string(), DimType::Text);

        let mut finances = BTreeMap::new();
        finances.insert(
            "liquid".to_string(),
            DimType::Record {
                fields: [("balance".to_string(), DimType::Number { lo: None, hi: None })]
                    .into_iter()
                    .collect(),
            },
        );
        finances.insert(
            "pockets".to_string(),
            DimType::Map { value: Box::new(DimType::Record { fields: pocket }) },
        );

        Schema::new()
            .declare("finances", DimType::Record { fields: finances })
            .declare("notes", DimType::Text)
            .declare("vendors", DimType::Any)
    }

    fn errors_for(src: &str) -> Vec<TypeError> {
        let p = parse_predicate(src).expect("parses");
        typecheck(&p, &Gamma::new(&household()))
    }

    #[test]
    fn a_comparison_between_two_numbers_is_well_typed() {
        assert!(errors_for("finances.liquid.balance >= 0").is_empty());
    }

    #[test]
    fn comparing_text_with_a_number_is_caught_before_it_can_run() {
        // ★★★ The whole point. Without this the gate reports a REFUSAL, and
        //     somebody goes looking for money that never moved.
        let errs = errors_for("notes > 5");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].detail.contains("different kinds of thing"), "{:?}", errs[0]);
    }

    #[test]
    fn the_message_names_both_sides_in_words() {
        // ★★ A person reading this has to be able to fix it. "text" and
        //    "a number" are the two facts that make the mistake obvious.
        let errs = errors_for("notes > 5");
        assert!(errs[0].detail.contains("text"));
        assert!(errs[0].detail.contains("a number"));
        assert!(errs[0].path.contains("notes"), "and which rule it was: {:?}", errs[0].path);
    }

    #[test]
    fn equality_across_types_is_allowed_and_ordering_is_not() {
        // ★★★ `a == b` across two types is honestly false — they are different
        //     values. `a > b` has no answer at all. Refusing both would outlaw
        //     a perfectly good "is this empty" test.
        assert!(errors_for("notes == 5").is_empty(), "equality is total");
        assert!(!errors_for("notes >= 5").is_empty(), "ordering is not");
    }

    #[test]
    fn an_undeclared_dimension_makes_no_type_complaint_of_its_own() {
        // ★★ `bind` already reports that, and naming one mistake twice in two
        //    voices teaches a person to read neither.
        assert!(errors_for("finances.nonsense > 5").is_empty());
    }

    #[test]
    fn an_untyped_dimension_is_compatible_with_everything() {
        // ★★★ `Any` is an admission that nothing was said. A checker that
        //     complained about silence would make adopting it cost a full
        //     re-declaration before it could say anything useful.
        assert!(errors_for("vendors > 5").is_empty());
        assert!(errors_for("vendors == 'x'").is_empty());
    }

    #[test]
    fn a_wildcard_types_as_what_the_container_holds() {
        assert!(errors_for("ALL finances.pockets[*]: allocated >= 0").is_empty());
        let errs = errors_for("ALL finances.pockets[*]: label >= 0");
        assert_eq!(errs.len(), 1, "a pocket's NAME is text: {errs:?}");
    }

    #[test]
    fn summing_something_that_is_not_a_number_is_caught() {
        // ★★★ The article's own example of what author-time typing buys.
        let errs = errors_for("SUM(finances.pockets[*].label) > 0");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].detail.contains("different kinds of thing"));
    }

    #[test]
    fn counting_is_a_number_whatever_it_counts() {
        assert!(errors_for("COUNT(finances.pockets[*].label) > 0").is_empty());
    }

    #[test]
    fn a_param_is_typed_from_what_the_operator_declared() {
        let schema = household();
        let gamma = Gamma::new(&schema).with_param("amount", Ty::Number);
        let good = parse_predicate("finances.liquid.balance >= params.amount").unwrap();
        assert!(typecheck(&good, &gamma).is_empty());

        let gamma = Gamma::new(&schema).with_param("pocket_name", Ty::Text);
        let bad = parse_predicate("finances.liquid.balance >= params.pocket_name").unwrap();
        assert_eq!(typecheck(&bad, &gamma).len(), 1);
    }

    #[test]
    fn an_undeclared_param_is_unchecked_rather_than_assumed() {
        // ★★ Same rule as `Any`: unstated is not the same as wrong.
        let schema = household();
        let p = parse_predicate("finances.liquid.balance >= params.whatever").unwrap();
        assert!(typecheck(&p, &Gamma::new(&schema)).is_empty());
    }

    #[test]
    fn ranging_over_a_single_number_is_a_mistake_not_a_narrow_loop() {
        let errs = errors_for("ALL finances.liquid.balance[*]: allocated >= 0");
        assert!(!errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn every_shipped_invariant_is_well_typed() {
        // ★★★ The checker's first real job: the rules already in production.
        // A finding here would be a live bug, not a test failure.
        for src in [
            "finances.liquid.balance >= 0",
            "ALL finances.pockets[*]: allocated >= 0",
        ] {
            assert!(errors_for(src).is_empty(), "{src}: {:?}", errors_for(src));
        }
    }
}
