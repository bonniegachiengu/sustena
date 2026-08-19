//! Roll-up **ρ** — a household's declared aggregates, folded from itself and
//! its children (Composition §VIII · the reference's `compute_rollup`).
//!
//! `⊕` composes Sustains into a holarchy; ρ is the information that flows back
//! **up** it. A parent declares what it wants totalled — an id, a `child_path`
//! into each member's state, and how to combine them — and ρ answers it from
//! whatever is readable right now.
//!
//! ## ★★★ Three properties, and each one is a refusal to fabricate
//!
//! **1 · Fresh every call, never persisted.** There is no store here and no
//! cached total anywhere. A household figure that could be *stale* would be a
//! number with no way to tell you when it stopped being true — the same reason
//! [`crate::fold`] recomputes state from the log rather than trusting a
//! snapshot. Call it twice against unchanged states and it reproduces itself.
//!
//! **2 · An unreadable member is EXCLUDED AND NAMED, never counted as zero.**
//! A missing child treated as `0.0` is the most dangerous possible answer: it
//! is arithmetically indistinguishable from a member who genuinely holds
//! nothing, and it makes a household look richer or poorer than it is with no
//! trace. So every [`AggregateReading`] carries [`included`](AggregateReading::included)
//! **and** [`excluded`](AggregateReading::excluded), and a surface that shows
//! the value without the exclusions is misreporting on its own account.
//!
//! **3 · The household's own contribution is IN.** The reference shipped this
//! children-only at first and corrected it: a "household total" that skipped
//! the household read as *what your children collectively hold*, which is a
//! different question from the one the label asks. [`compute`] takes the
//! parent's own state as the first contributor, tagged
//! [`Contributor::Household`] so a consumer can tell it from a member without
//! guessing.
//!
//! ## ★★ Nothing here is a second implementation of anything
//!
//! The five combining ops are [`AggFunc`] — **the predicate DSL's own**, which
//! already had exactly these five and, as it happens, exactly the reference
//! roll-up's empty-set answers. The reference has the arithmetic twice
//! (`predicates.py` and `sustain_engine.py`'s `_ROLLUP_OPS`); two copies that
//! agree today are still two copies, so [`AggFunc::reduce`] is the one
//! definition and both callers reach it.
//!
//! The path grammar is [`parse_state_path`] and the resolver is
//! [`resolve_path`] — the same two `V` uses. A wildcard therefore means the
//! same thing to a rule and to a household total, which is the divergence this
//! core deleted rather than ported.
//!
//! ## ★ What is NOT here
//!
//! **Authority.** ρ is upward *information*. A parent invariant that reads an
//! aggregate and refuses a child's transition is a different mechanism
//! (downward authority) and it is not this module — the reference keeps them
//! apart too, and defaults its aggregate rules to advisory precisely so the
//! parent observes its children rather than vetoing them.
//!
//! **The "what if" arm.** The reference's `_hypothetical_rollup` carries an
//! override parameter so the binding gate and the simulator can substitute one
//! child's not-yet-committed state for a disk read. This crate has no disk, so
//! that arm collapses: a caller wanting the hypothetical simply passes the
//! candidate state as that child's state. One function, no mode flag.

use serde_json::Value;

use crate::predicate::eval::resolve_path;
use crate::predicate::{parse_state_path, AggFunc, PathSegment};

/// One thing a household declared it wants totalled.
///
/// ★ Constructed only through [`declare`](AggregateDecl::declare), which parses
/// the path and rejects an unknown op — a declaration that cannot be resolved
/// is a finding at declaration time, not a silent `null` at read time.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateDecl {
    id: String,
    child_path: String,
    segments: Vec<PathSegment>,
    op: AggFunc,
}

impl AggregateDecl {
    /// Declare an aggregate. `op` is one of `sum` / `count` / `avg` / `min` /
    /// `max`, in any case — [`AggFunc::parse`] is the same reader the DSL uses.
    pub fn declare(id: &str, child_path: &str, op: &str) -> Result<Self, RollupError> {
        let func = AggFunc::parse(op).ok_or_else(|| RollupError::UnknownOp {
            aggregate: id.to_string(),
            op: op.to_string(),
        })?;
        let segments = parse_state_path(child_path).map_err(|reason| RollupError::BadPath {
            aggregate: id.to_string(),
            path: child_path.to_string(),
            reason,
        })?;
        Ok(Self {
            id: id.to_string(),
            child_path: child_path.to_string(),
            segments,
            op: func,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn child_path(&self) -> &str {
        &self.child_path
    }

    pub fn op(&self) -> AggFunc {
        self.op
    }
}

/// Who a number came from.
///
/// ★ An enum rather than a nullable member name: "the household's own
/// contribution" and "a member called nothing in particular" are different
/// facts, and a consumer that had to tell them apart by checking whether a
/// string was empty would get it wrong eventually.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contributor {
    /// The parent's own state.
    Household { sustain_id: String },
    /// A linked child.
    Member {
        sustain_id: String,
        slot: Option<String>,
        member: Option<String>,
    },
}

impl Contributor {
    pub fn sustain_id(&self) -> &str {
        match self {
            Self::Household { sustain_id } => sustain_id,
            Self::Member { sustain_id, .. } => sustain_id,
        }
    }

    pub fn is_household(&self) -> bool {
        matches!(self, Self::Household { .. })
    }

    /// A human label: the member's name, else the slot, else the id.
    pub fn label(&self) -> &str {
        match self {
            Self::Household { sustain_id } => sustain_id,
            Self::Member {
                sustain_id,
                slot,
                member,
            } => member
                .as_deref()
                .or(slot.as_deref())
                .unwrap_or(sustain_id.as_str()),
        }
    }
}

/// A contributor that resolved to a number.
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    contributor: Contributor,
    value: f64,
}

impl Contribution {
    pub fn contributor(&self) -> &Contributor {
        &self.contributor
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

/// A contributor that did not, **and why**.
///
/// ★★ The reason is the whole point. "6 of 7 members counted" is an alarm;
/// "Frankie — no state found, the link may be stale" is a diagnosis.
#[derive(Debug, Clone, PartialEq)]
pub struct Exclusion {
    contributor: Contributor,
    reason: String,
}

impl Exclusion {
    pub fn contributor(&self) -> &Contributor {
        &self.contributor
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// One declared aggregate, answered.
///
/// ★★★ **No public constructor.** A household total is exactly the kind of
/// figure a surface would be tempted to assemble itself, and one assembled
/// outside [`compute`] would carry no `included`/`excluded` and no way to know
/// what it missed. The only way to hold one of these is to have computed it —
/// the same structural guarantee [`crate::pawa::PawaReading`] gives a debit.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateReading {
    id: String,
    child_path: String,
    op: AggFunc,
    value: Value,
    included: Vec<Contribution>,
    excluded: Vec<Exclusion>,
    includes_household_own: bool,
}

impl AggregateReading {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn child_path(&self) -> &str {
        &self.child_path
    }

    pub fn op(&self) -> AggFunc {
        self.op
    }

    /// The combined figure. `Null` only for `min`/`max` over nothing — there is
    /// no smallest element of an empty set, and `0` would be a claim.
    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn included(&self) -> &[Contribution] {
        &self.included
    }

    pub fn excluded(&self) -> &[Exclusion] {
        &self.excluded
    }

    /// Whether the parent's own state was offered as a contributor.
    pub fn includes_household_own(&self) -> bool {
        self.includes_household_own
    }

    /// ★ Whether anything at all was readable.
    ///
    /// `sum` over nothing is `0` — arithmetically correct, and the reference
    /// says the same — but a surface showing a bare `0` when *every* member was
    /// excluded is reporting an emptiness it did not measure. This is the check
    /// that lets it say so instead.
    pub fn is_grounded(&self) -> bool {
        !self.included.is_empty()
    }
}

/// Whether a linked child could be read at all.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildStatus {
    contributor: Contributor,
    readable: bool,
}

impl ChildStatus {
    pub fn contributor(&self) -> &Contributor {
        &self.contributor
    }

    pub fn readable(&self) -> bool {
        self.readable
    }
}

/// A linked child, as offered to ρ.
///
/// `state` is `None` when the caller could not read it — a stale link, a
/// removed child. That is a fact ρ reports; it is not an error, because one
/// unreadable member must not cost a household every other member's figure.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildState {
    pub sustain_id: String,
    pub slot: Option<String>,
    pub member: Option<String>,
    pub state: Option<Value>,
}

impl ChildState {
    pub fn readable(sustain_id: &str, state: Value) -> Self {
        Self {
            sustain_id: sustain_id.to_string(),
            slot: None,
            member: None,
            state: Some(state),
        }
    }

    /// A child the caller could not read. Excluded and named, never zeroed.
    pub fn unreadable(sustain_id: &str) -> Self {
        Self {
            sustain_id: sustain_id.to_string(),
            slot: None,
            member: None,
            state: None,
        }
    }

    pub fn in_slot(mut self, slot: &str) -> Self {
        self.slot = Some(slot.to_string());
        self
    }

    pub fn named(mut self, member: &str) -> Self {
        self.member = Some(member.to_string());
        self
    }

    fn contributor(&self) -> Contributor {
        Contributor::Member {
            sustain_id: self.sustain_id.clone(),
            slot: self.slot.clone(),
            member: self.member.clone(),
        }
    }
}

/// Every declared aggregate, answered, plus who was readable.
#[derive(Debug, Clone, PartialEq)]
pub struct Rollup {
    children: Vec<ChildStatus>,
    aggregates: Vec<AggregateReading>,
}

impl Rollup {
    pub fn children(&self) -> &[ChildStatus] {
        &self.children
    }

    pub fn aggregates(&self) -> &[AggregateReading] {
        &self.aggregates
    }

    pub fn aggregate(&self, id: &str) -> Option<&AggregateReading> {
        self.aggregates.iter().find(|a| a.id == id)
    }

    /// Children the caller could not read at all.
    pub fn unreadable_children(&self) -> Vec<&ChildStatus> {
        self.children.iter().filter(|c| !c.readable).collect()
    }
}

/// **ρ.** Fold the household's own state and every child's into the declared
/// aggregates.
///
/// `household` is the parent's own state, or `None` for the children-only
/// reading. Nothing is written, nothing is cached, and the same inputs give the
/// same answer every time.
pub fn compute(
    declared: &[AggregateDecl],
    household_id: &str,
    household: Option<&Value>,
    children: &[ChildState],
) -> Rollup {
    let child_statuses: Vec<ChildStatus> = children
        .iter()
        .map(|c| ChildStatus {
            contributor: c.contributor(),
            readable: c.state.is_some(),
        })
        .collect();

    let aggregates = declared
        .iter()
        .map(|decl| {
            let mut included: Vec<Contribution> = Vec::new();
            let mut excluded: Vec<Exclusion> = Vec::new();
            let mut nums: Vec<f64> = Vec::new();

            // The household's own contribution first — it is a member of its
            // own total, not an observer of it.
            if let Some(own) = household {
                let contributor = Contributor::Household {
                    sustain_id: household_id.to_string(),
                };
                match resolve_contribution(own, &decl.segments) {
                    Some(v) => {
                        nums.push(v);
                        included.push(Contribution {
                            contributor,
                            value: v,
                        });
                    }
                    None => excluded.push(Exclusion {
                        contributor,
                        reason: format!(
                            "path '{}' not present or not numeric on the household's own state",
                            decl.child_path
                        ),
                    }),
                }
            }

            for child in children {
                let contributor = child.contributor();
                let Some(state) = &child.state else {
                    excluded.push(Exclusion {
                        contributor,
                        reason: "child state unavailable".to_string(),
                    });
                    continue;
                };
                match resolve_contribution(state, &decl.segments) {
                    Some(v) => {
                        nums.push(v);
                        included.push(Contribution {
                            contributor,
                            value: v,
                        });
                    }
                    None => excluded.push(Exclusion {
                        contributor,
                        reason: format!(
                            "path '{}' not present or not numeric on this child",
                            decl.child_path
                        ),
                    }),
                }
            }

            // ★ The population IS the numeric set: anything that did not
            //   resolve is already excluded and named above, so `count`
            //   counts contributors rather than attempted reads.
            let value = decl.op.reduce(&nums, nums.len());

            AggregateReading {
                id: decl.id.clone(),
                child_path: decl.child_path.clone(),
                op: decl.op,
                value,
                included,
                excluded,
                includes_household_own: household.is_some(),
            }
        })
        .collect();

    Rollup {
        children: child_statuses,
        aggregates,
    }
}

/// One contributor's number, or `None` if the path does not land on one.
///
/// Two forms, matching the reference:
///
/// - a plain path (`finances.liquid.balance`) → that number.
/// - a wildcard path (`finances.pockets[*].allocated`) → the **sum** across the
///   contributor's own container. ★ The wildcard's own reduction is always sum,
///   whatever the aggregate's `op` is: `op` combines contributors, and a
///   household asking for the *minimum* member is not asking for the minimum
///   pocket inside each member.
fn resolve_contribution(state: &Value, segments: &[PathSegment]) -> Option<f64> {
    let wildcard_at = segments.iter().position(|s| *s == PathSegment::Wildcard);

    let Some(i) = wildcard_at else {
        let v = resolve_path(state, segments)?;
        return numeric(&v);
    };

    // ★ The container has to genuinely be a collection. Resolving a wildcard
    //   against a scalar yields an empty list, and summing that would report a
    //   confident `0` for a contributor whose shape we could not read at all —
    //   the fabricated zero this module exists to refuse.
    let container = resolve_path(state, &segments[..i])?;
    if !container.is_object() && !container.is_array() {
        return None;
    }

    let mapped = resolve_path(state, segments)?;
    let items = mapped.as_array()?;
    // ★ `Sum for f64` folds from the additive identity `-0.0`, so an empty
    //   container yields NEGATIVE zero — which formats as `-0.00` and reads to a
    //   person as a debt. It is the same number, and it is not the same claim.
    //   The reference seeds its own loop with `0.0` and never sees this.
    Some(unsign_zero(items.iter().filter_map(numeric).sum()))
}

/// `-0.0` is `0.0`, and only one of them is honest on a screen.
fn unsign_zero(v: f64) -> f64 {
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

/// Numbers only. ★ A bool is excluded even though Python counts `True` as `1`
/// — the reference's roll-up guards against it explicitly, and a flag silently
/// worth one unit of money is a bug wearing a value.
fn numeric(v: &Value) -> Option<f64> {
    if v.is_boolean() {
        return None;
    }
    v.as_f64()
}

/// A declaration that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollupError {
    UnknownOp { aggregate: String, op: String },
    BadPath {
        aggregate: String,
        path: String,
        reason: String,
    },
}

impl std::fmt::Display for RollupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownOp { aggregate, op } => {
                write!(f, "aggregate '{aggregate}' declares unknown op '{op}'")
            }
            Self::BadPath {
                aggregate,
                path,
                reason,
            } => write!(
                f,
                "aggregate '{aggregate}' declares unreadable path '{path}': {reason}"
            ),
        }
    }
}

impl std::error::Error for RollupError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn liquid(n: f64) -> Value {
        json!({ "finances": { "liquid": { "balance": n } } })
    }

    fn decl(op: &str) -> AggregateDecl {
        AggregateDecl::declare("household_liquid_total", "finances.liquid.balance", op).unwrap()
    }

    #[test]
    fn sums_the_household_and_every_child() {
        let r = compute(
            &[decl("sum")],
            "homestead",
            Some(&liquid(14000.0)),
            &[
                ChildState::readable("bonnie", liquid(1800.0)).named("Bonnie"),
                ChildState::readable("cira", liquid(5000.0)).named("Cira"),
            ],
        );
        let a = r.aggregate("household_liquid_total").unwrap();
        assert_eq!(a.value(), &json!(20800));
        assert_eq!(a.included().len(), 3);
        assert!(a.excluded().is_empty());
        assert!(a.includes_household_own());
        assert!(a.included()[0].contributor().is_household());
    }

    #[test]
    fn the_household_can_be_left_out() {
        let r = compute(
            &[decl("sum")],
            "homestead",
            None,
            &[ChildState::readable("bonnie", liquid(1800.0))],
        );
        let a = r.aggregate("household_liquid_total").unwrap();
        assert_eq!(a.value(), &json!(1800));
        assert!(!a.includes_household_own());
    }

    #[test]
    fn an_unreadable_child_is_excluded_and_named_not_zeroed() {
        let r = compute(
            &[decl("sum")],
            "homestead",
            Some(&liquid(1000.0)),
            &[
                ChildState::readable("bonnie", liquid(600.0)).named("Bonnie"),
                ChildState::unreadable("frankie").named("Frankie"),
            ],
        );
        let a = r.aggregate("household_liquid_total").unwrap();
        // 1600, not 1600-with-a-silent-zero and not a refusal.
        assert_eq!(a.value(), &json!(1600));
        assert_eq!(a.excluded().len(), 1);
        assert_eq!(a.excluded()[0].contributor().label(), "Frankie");
        assert_eq!(a.excluded()[0].reason(), "child state unavailable");
        assert_eq!(r.unreadable_children().len(), 1);
    }

    #[test]
    fn a_child_missing_the_path_is_excluded_and_named() {
        let r = compute(
            &[decl("sum")],
            "homestead",
            Some(&liquid(1000.0)),
            &[ChildState::readable("garden", json!({ "soil": 40 })).named("Garden")],
        );
        let a = r.aggregate("household_liquid_total").unwrap();
        assert_eq!(a.value(), &json!(1000));
        assert_eq!(a.excluded().len(), 1);
        assert!(a.excluded()[0].reason().contains("not present or not numeric"));
    }

    #[test]
    fn min_and_max_over_nothing_are_null_not_zero() {
        for op in ["min", "max"] {
            let r = compute(&[decl(op)], "homestead", None, &[]);
            let a = r.aggregate("household_liquid_total").unwrap();
            assert_eq!(a.value(), &Value::Null, "{op} over nothing");
            assert!(!a.is_grounded());
        }
    }

    #[test]
    fn sum_over_nothing_is_zero_but_not_grounded() {
        let r = compute(&[decl("sum")], "homestead", None, &[]);
        let a = r.aggregate("household_liquid_total").unwrap();
        assert_eq!(a.value(), &json!(0));
        assert!(!a.is_grounded(), "nothing was measured, so nothing grounds the 0");
    }

    #[test]
    fn count_counts_contributors_not_attempts() {
        let r = compute(
            &[decl("count")],
            "homestead",
            Some(&liquid(1.0)),
            &[
                ChildState::readable("a", liquid(2.0)),
                ChildState::unreadable("b"),
            ],
        );
        assert_eq!(r.aggregate("household_liquid_total").unwrap().value(), &json!(2));
    }

    #[test]
    fn avg_min_max_combine_contributors() {
        let kids = [
            ChildState::readable("a", liquid(10.0)),
            ChildState::readable("b", liquid(30.0)),
        ];
        for (op, want) in [("avg", json!(20)), ("min", json!(10)), ("max", json!(30))] {
            let r = compute(&[decl(op)], "homestead", None, &kids);
            assert_eq!(r.aggregate("household_liquid_total").unwrap().value(), &want, "{op}");
        }
    }

    #[test]
    fn a_wildcard_path_sums_within_each_contributor() {
        let d = AggregateDecl::declare("pockets", "finances.pockets[*].allocated", "sum").unwrap();
        let with_pockets = json!({
            "finances": { "pockets": { "food": { "allocated": 800 }, "rent": { "allocated": 200 } } }
        });
        let r = compute(
            &[d],
            "homestead",
            Some(&with_pockets),
            &[ChildState::readable("bonnie", with_pockets.clone())],
        );
        let a = r.aggregate("pockets").unwrap();
        assert_eq!(a.value(), &json!(2000));
        assert_eq!(a.included()[0].value(), 1000.0);
    }

    #[test]
    fn an_empty_pocket_dict_is_a_real_zero_a_scalar_is_not() {
        let d = AggregateDecl::declare("pockets", "finances.pockets[*].allocated", "sum").unwrap();
        let empty = json!({ "finances": { "pockets": {} } });
        let scalar = json!({ "finances": { "pockets": 7 } });
        let r = compute(
            &[d],
            "homestead",
            None,
            &[
                ChildState::readable("empty", empty).named("Empty"),
                ChildState::readable("scalar", scalar).named("Scalar"),
            ],
        );
        let a = r.aggregate("pockets").unwrap();
        // The empty dict genuinely holds nothing; the scalar could not be read
        // at all, and reporting it as 0 would be the fabrication.
        assert_eq!(a.included().len(), 1);
        assert_eq!(a.included()[0].contributor().label(), "Empty");
        assert_eq!(a.excluded().len(), 1);
        assert_eq!(a.excluded()[0].contributor().label(), "Scalar");
    }

    #[test]
    fn an_empty_container_contributes_a_positive_zero() {
        // ★ Rust folds an empty f64 sum from -0.0. Same number, and `-0.00` on
        //   a household screen reads as a debt.
        let d = AggregateDecl::declare("pockets", "finances.pockets[*].allocated", "sum").unwrap();
        let r = compute(
            &[d],
            "homestead",
            None,
            &[ChildState::readable("empty", json!({ "finances": { "pockets": {} } }))],
        );
        let a = r.aggregate("pockets").unwrap();
        assert!(!a.included()[0].value().is_sign_negative(), "no negative zero");
        assert_eq!(a.value(), &json!(0));
    }

    #[test]
    fn a_bool_is_not_a_number() {
        let r = compute(
            &[decl("sum")],
            "homestead",
            None,
            &[ChildState::readable(
                "b",
                json!({ "finances": { "liquid": { "balance": true } } }),
            )],
        );
        let a = r.aggregate("household_liquid_total").unwrap();
        assert!(!a.is_grounded());
        assert_eq!(a.excluded().len(), 1);
    }

    #[test]
    fn recomputing_reproduces_itself() {
        let kids = [ChildState::readable("a", liquid(3.0))];
        let first = compute(&[decl("sum")], "homestead", Some(&liquid(4.0)), &kids);
        let second = compute(&[decl("sum")], "homestead", Some(&liquid(4.0)), &kids);
        assert_eq!(first, second);
    }

    #[test]
    fn an_unknown_op_is_refused_at_declaration_time() {
        let e = AggregateDecl::declare("x", "a.b", "median").unwrap_err();
        assert!(matches!(e, RollupError::UnknownOp { .. }));
        assert!(e.to_string().contains("median"));
    }

    #[test]
    fn an_unreadable_path_is_refused_at_declaration_time() {
        let e = AggregateDecl::declare("x", "", "sum").unwrap_err();
        assert!(matches!(e, RollupError::BadPath { .. }));
    }

    #[test]
    fn several_aggregates_are_answered_independently() {
        let liquid_decl = decl("sum");
        let pockets_decl =
            AggregateDecl::declare("pockets", "finances.pockets[*].allocated", "sum").unwrap();
        let state = json!({
            "finances": { "liquid": { "balance": 5 }, "pockets": { "food": { "allocated": 9 } } }
        });
        let r = compute(&[liquid_decl, pockets_decl], "h", Some(&state), &[]);
        assert_eq!(r.aggregates().len(), 2);
        assert_eq!(r.aggregate("household_liquid_total").unwrap().value(), &json!(5));
        assert_eq!(r.aggregate("pockets").unwrap().value(), &json!(9));
    }
}
