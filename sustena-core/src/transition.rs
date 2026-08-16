//! Transition constraints — `D : S × S → bool` (Constraint §I, §VI).
//!
//! The gate's third conjunct, and the one the reference engine cannot express:
//!
//! ```text
//! admit(o,s) ⟺ g_o(s) ∧ o(s) ∈ A ∧ D(s, o(s))
//!               guard     result     THE STEP
//! ```
//!
//! `D` is **strictly more expressive than a state constraint and cannot be
//! simulated by one.** Its three canonical shapes are all *true of the step* and
//! say nothing about the endpoints in isolation — **any single state satisfies
//! all of them vacuously**:
//!
//! ```text
//! |π_p(s') − π_p(s)| ≤ δ        rate limit
//! π_p(s') ≤ π_p(s)              monotonicity
//! Σ_{p∈P} π_p(s') = Σ_{p∈P} π_p(s)    conservation
//! ```
//!
//! That is why "money is conserved across a transfer" has been inexpressible:
//! both existing evaluators take one state. The plumbing was already there — the
//! run path holds the pre-load state and the mutated candidate — only the
//! language was missing.
//!
//! ## Conservation, and the discipline that goes with it
//!
//! > A conserved quantity is the shadow of a sameness. If you assert a
//! > conservation law, you should be able to name the sameness it is the shadow
//! > of — and if you cannot, you have probably asserted an accounting
//! > convention rather than a law.
//!
//! Money conserved across `transfer` is the shadow of a real sameness: the total
//! does not depend on which pocket you call which. Relabel the pockets and the
//! household's finances are the same finances. Contrast *"spending decreases 5%
//! each month"* — a target, with no symmetry behind it, which belongs in a
//! controller objective and **never** in `D`.
//!
//! Noether is the named analogy and the article is careful about it: Sustena has
//! no action functional and no continuous symmetry group, so the theorem does
//! not literally apply to a household ledger. What transfers is the design
//! instruction above, not the mathematics.
//!
//! ## Two things are unrepresentable here
//!
//! 1. **A clamped conservation law.** [`TransitionRule`] has **no strategy
//!    field at all**. The article's rule — *never clamp a conserved quantity* —
//!    is not enforced by a check that could be forgotten; there is nowhere to
//!    write the clamp down. Capping a transfer's outflow at the available
//!    balance while leaving the inflow untouched **creates money**: it satisfies
//!    `C` by violating `D`. (The state-constraint side of the same rule is
//!    already enforced at authoring time by
//!    [`crate::admission::typecheck_constraint`].)
//!
//! 2. **Money summed in floating point.** [`Tolerance::Exact`] sums as `i128`
//!    over integer minor units, and a non-integer at a conserved path is a
//!    **refusal**, not a rounding. A float cannot enter the computation, so a
//!    law that is "true except in the seventh decimal place" cannot be written.
//!    The article is explicit: *where money is the quantity, integer minor units
//!    and `tolerance = 0` are the correct choice, and the constraint is then
//!    exact.*
//!
//! `tolerance` is not sloppiness — it is where representation and declared
//! rounding rules get named out loud. [`Tolerance::Within`] exists for genuinely
//! continuous quantities and makes the choice visible at the declaration.

use serde_json::Value;
use thiserror::Error;

use crate::predicate::{parse_state_path, PathSegment};

/// Which way a monotone quantity is allowed to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `π_p(s') ≤ π_p(s)` — "stock may only decrease".
    NonIncreasing,
    /// `π_p(s') ≥ π_p(s)` — a meter that never runs backwards.
    NonDecreasing,
}

impl Direction {
    fn as_str(&self) -> &'static str {
        match self {
            Direction::NonIncreasing => "non-increasing",
            Direction::NonDecreasing => "non-decreasing",
        }
    }
}

/// How exactly a conserved total must match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tolerance {
    /// **Integer minor units, tolerance 0.** The correct choice for money. Sums
    /// as `i128`; a non-integer value at a conserved path is refused rather than
    /// rounded, because a conservation law that holds only to the sixth decimal
    /// place is not a law.
    Exact,
    /// A declared tolerance for a genuinely continuous quantity. Naming the
    /// number is the point: it is where floating-point representation gets
    /// stated out loud instead of silently producing a false law.
    Within(f64),
}

/// A quantity that is summed across a set of paths.
///
/// Wildcards are allowed, so `finances.pockets[*].allocated` is one path
/// contributing every pocket's allocation. The sum runs over every path in the
/// set — for money, that is typically the liquid balance plus every pocket, so
/// that moving between them nets to zero.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    pub id: String,
    pub paths: Vec<String>,
}

impl Quantity {
    pub fn new(id: &str, paths: &[&str]) -> Self {
        Self { id: id.to_string(), paths: paths.iter().map(|p| p.to_string()).collect() }
    }
}

/// `D : S × S → bool` — a predicate on the ordered pair.
///
/// **Carries no strategy.** A transition constraint refuses or it passes. See
/// the module docs for why that is a type-level decision and not an omission.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionRule {
    /// `|SUM(after[q]) − SUM(before[q])| ≤ tolerance`
    Conservation { id: String, quantity: Quantity, tolerance: Tolerance },
    /// `|π_p(s') − π_p(s)| ≤ δ` — "balance may not jump by more than δ".
    RateLimit { id: String, path: String, delta: f64 },
    /// `π_p(s') ≤ π_p(s)` or `≥` — "stock may only decrease".
    Monotone { id: String, path: String, direction: Direction },
}

/// A declared transition rule that is not well-formed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransitionDeclError {
    #[error("transition rule '{id}': path '{path}' is not a valid dot-path: {detail}")]
    BadPath { id: String, path: String, detail: String },

    #[error("transition rule '{id}': a rate limit's δ must be finite and non-negative, got {delta}")]
    BadDelta { id: String, delta: String },

    #[error("transition rule '{id}': a conservation tolerance must be finite and non-negative, got {tolerance}")]
    BadTolerance { id: String, tolerance: String },

    #[error("transition rule '{id}': a conservation law must name at least one path — an empty sum is conserved vacuously and says nothing")]
    EmptyQuantity { id: String },
}

/// A transition the gate refused, and why — with both sides of the step, since
/// the whole point of `D` is that neither endpoint alone is the story.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionViolation {
    pub rule_id: String,
    pub kind: &'static str,
    pub detail: String,
}

impl std::fmt::Display for TransitionViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "would violate {} '{}': {}", self.kind, self.rule_id, self.detail)
    }
}

/// The exact or approximate total of a quantity over one state.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Total {
    Minor(i128),
    Real(f64),
}

impl std::fmt::Display for Total {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Total::Minor(n) => write!(f, "{n}"),
            Total::Real(x) => write!(f, "{x}"),
        }
    }
}

impl TransitionRule {
    pub fn id(&self) -> &str {
        match self {
            TransitionRule::Conservation { id, .. }
            | TransitionRule::RateLimit { id, .. }
            | TransitionRule::Monotone { id, .. } => id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            TransitionRule::Conservation { .. } => "conservation law",
            TransitionRule::RateLimit { .. } => "rate limit",
            TransitionRule::Monotone { .. } => "monotonicity rule",
        }
    }

    /// Authoring-time well-formedness. Run when a spec is loaded, so a rule that
    /// could never be evaluated is a load-time finding rather than a 2 a.m. one.
    pub fn typecheck(&self) -> Result<(), TransitionDeclError> {
        let check_path = |p: &str| {
            parse_state_path(p).map(|_| ()).map_err(|e| TransitionDeclError::BadPath {
                id: self.id().to_string(),
                path: p.to_string(),
                detail: e.to_string(),
            })
        };
        match self {
            TransitionRule::Conservation { id, quantity, tolerance } => {
                if quantity.paths.is_empty() {
                    return Err(TransitionDeclError::EmptyQuantity { id: id.clone() });
                }
                for p in &quantity.paths {
                    check_path(p)?;
                }
                if let Tolerance::Within(t) = tolerance {
                    if !t.is_finite() || *t < 0.0 {
                        return Err(TransitionDeclError::BadTolerance {
                            id: id.clone(),
                            tolerance: t.to_string(),
                        });
                    }
                }
            }
            TransitionRule::RateLimit { id, path: p, delta } => {
                check_path(p)?;
                if !delta.is_finite() || *delta < 0.0 {
                    return Err(TransitionDeclError::BadDelta {
                        id: id.clone(),
                        delta: delta.to_string(),
                    });
                }
            }
            TransitionRule::Monotone { path: p, .. } => check_path(p)?,
        }
        Ok(())
    }

    /// Evaluate `D(before, after)`.
    pub fn check(&self, before: &Value, after: &Value) -> Result<(), TransitionViolation> {
        let violation = |detail: String| TransitionViolation {
            rule_id: self.id().to_string(),
            kind: self.kind(),
            detail,
        };

        match self {
            TransitionRule::Conservation { quantity, tolerance, .. } => {
                let b = total(before, quantity, *tolerance).map_err(&violation)?;
                let a = total(after, quantity, *tolerance).map_err(&violation)?;
                match (b, a, tolerance) {
                    (Total::Minor(b), Total::Minor(a), Tolerance::Exact) => {
                        if a != b {
                            let delta = a - b;
                            let verb = if delta > 0 { "created" } else { "destroyed" };
                            return Err(violation(format!(
                                "'{}' is conserved, but this step {verb} {} minor units \
                                 (before {b}, after {a})",
                                quantity.id,
                                delta.abs()
                            )));
                        }
                    }
                    (Total::Real(b), Total::Real(a), Tolerance::Within(t)) => {
                        if (a - b).abs() > *t {
                            return Err(violation(format!(
                                "'{}' is conserved to within {t}, but this step moved the total by {} \
                                 (before {b}, after {a})",
                                quantity.id,
                                (a - b).abs()
                            )));
                        }
                    }
                    // total() always returns the variant matching the tolerance.
                    _ => unreachable!("total() returns the variant its tolerance selects"),
                }
            }

            TransitionRule::RateLimit { path: p, delta, .. } => {
                let b = number_at(before, p).map_err(&violation)?;
                let a = number_at(after, p).map_err(&violation)?;
                if (a - b).abs() > *delta {
                    return Err(violation(format!(
                        "'{p}' may move by at most {delta} in one step, but moved by {} \
                         (before {b}, after {a})",
                        (a - b).abs()
                    )));
                }
            }

            TransitionRule::Monotone { path: p, direction, .. } => {
                let b = number_at(before, p).map_err(&violation)?;
                let a = number_at(after, p).map_err(&violation)?;
                let ok = match direction {
                    Direction::NonIncreasing => a <= b,
                    Direction::NonDecreasing => a >= b,
                };
                if !ok {
                    return Err(violation(format!(
                        "'{p}' must be {}, but moved from {b} to {a}",
                        direction.as_str()
                    )));
                }
            }
        }
        Ok(())
    }
}

/// `D(s, s') = ⋀ᵢ Dᵢ(s, s')` — conjunction is the only combinator, so adding a
/// rule can only ever shrink what is admissible. The first refusal is returned,
/// named.
pub fn check_all(
    rules: &[TransitionRule],
    before: &Value,
    after: &Value,
) -> Result<(), TransitionViolation> {
    for rule in rules {
        rule.check(before, after)?;
    }
    Ok(())
}

/// Sum a quantity over one state, in the arithmetic its tolerance selects.
fn total(state: &Value, quantity: &Quantity, tolerance: Tolerance) -> Result<Total, String> {
    let mut minor: i128 = 0;
    let mut real: f64 = 0.0;

    for p in &quantity.paths {
        let segments = parse_state_path(p).map_err(|e| format!("path '{p}' is invalid: {e}"))?;
        let mut found = Vec::new();
        collect(state, &segments, &mut found);

        for value in found {
            match tolerance {
                Tolerance::Exact => {
                    // Money is a whole number of minor units. JSON does not
                    // distinguish 95000 from 95000.0, and arithmetic on the
                    // reference engine promotes to float, so an INTEGRAL float
                    // is accepted — it is the same quantity, differently
                    // written. A genuinely FRACTIONAL value is refused rather
                    // than truncated: half a cent that no rounding rule
                    // declared is precisely the "law false in the seventh
                    // decimal place" this mode exists to prevent.
                    let n = as_whole_minor_units(&value).ok_or_else(|| {
                        format!(
                            "'{}' is declared exact (integer minor units), but '{p}' holds {} —                              a conservation law cannot be exact over a fractional value",
                            quantity.id, value
                        )
                    })?;
                    minor += n;
                }
                Tolerance::Within(_) => {
                    let n = value.as_f64().ok_or_else(|| {
                        format!("'{}' sums numbers, but '{p}' holds {}", quantity.id, value)
                    })?;
                    real += n;
                }
            }
        }
    }

    Ok(match tolerance {
        Tolerance::Exact => Total::Minor(minor),
        Tolerance::Within(_) => Total::Real(real),
    })
}

/// A JSON number as a whole count of minor units, or `None` if it is fractional
/// (or too large to count exactly).
///
/// Accepting `95000.0` alongside `95000` is not a loosening of the law: they are
/// the same quantity, differently written, and the reference engine's arithmetic
/// promotes to float. Rejecting `99999.5` IS the law.
fn as_whole_minor_units(value: &Value) -> Option<i128> {
    if let Some(n) = value.as_i64() {
        return Some(n as i128);
    }
    let f = value.as_f64()?;
    if !f.is_finite() || f.fract() != 0.0 {
        return None;
    }
    // Beyond 2^53 an f64 no longer represents every integer, so a "whole" value
    // there is not reliably the value that was written.
    if f.abs() > 9_007_199_254_740_992.0 {
        return None;
    }
    Some(f as i128)
}

/// Gather every value a (possibly wildcard) path reaches.
///
/// A path that resolves to nothing contributes nothing — which is correct for a
/// sum: a pocket that does not exist yet holds zero. That is also why a
/// conservation law over an empty path set is rejected at authoring time, since
/// it would be satisfied vacuously and say nothing.
fn collect(node: &Value, segments: &[PathSegment], out: &mut Vec<Value>) {
    let Some((seg, rest)) = segments.split_first() else {
        out.push(node.clone());
        return;
    };
    match seg {
        PathSegment::Name(name) => {
            if let Some(child) = node.as_object().and_then(|m| m.get(name.as_str())) {
                collect(child, rest, out);
            }
        }
        PathSegment::Index(i) => {
            if let Some(child) = node.as_array().and_then(|a| a.get(*i)) {
                collect(child, rest, out);
            }
        }
        PathSegment::Wildcard => match node {
            Value::Object(m) => m.values().for_each(|v| collect(v, rest, out)),
            Value::Array(a) => a.iter().for_each(|v| collect(v, rest, out)),
            _ => {}
        },
    }
}

/// A single number at a path, for the rate-limit and monotonicity shapes.
///
/// A path that does not resolve is refused rather than treated as zero: for a
/// rate limit, "absent" and "zero" are different claims, and guessing which one
/// was meant is how a rule stops meaning what it says.
fn number_at(state: &Value, p: &str) -> Result<f64, String> {
    let segments = parse_state_path(p).map_err(|e| format!("path '{p}' is invalid: {e}"))?;
    let mut found = Vec::new();
    collect(state, &segments, &mut found);
    match found.len() {
        0 => Err(format!("'{p}' does not resolve in this state")),
        1 => found[0]
            .as_f64()
            .ok_or_else(|| format!("'{p}' holds {}, which is not a number", found[0])),
        n => Err(format!(
            "'{p}' resolves to {n} values; a rate limit or monotonicity rule needs one \
             (use a conservation law to constrain a set)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Money as integer minor units — cents, not shillings.
    fn ledger(liquid: i64, food: i64, rent: i64) -> Value {
        json!({"finances": {
            "liquid": {"balance": liquid},
            "pockets": {
                "food": {"allocated": food},
                "rent": {"allocated": rent}
            }
        }})
    }

    fn money() -> TransitionRule {
        TransitionRule::Conservation {
            id: "money_conserved".into(),
            quantity: Quantity::new(
                "household money",
                &["finances.liquid.balance", "finances.pockets[*].allocated"],
            ),
            tolerance: Tolerance::Exact,
        }
    }

    // ── conservation (§VI) ───────────────────────────────────────────────────

    #[test]
    fn a_transfer_between_pockets_conserves_money() {
        // The shadow of a real sameness: the total does not depend on which
        // pocket you call which.
        let before = ledger(100_000, 20_000, 30_000);
        let after = ledger(90_000, 30_000, 30_000);
        assert_eq!(money().check(&before, &after), Ok(()));
    }

    #[test]
    fn relabelling_the_pockets_leaves_the_household_the_same() {
        let before = ledger(0, 20_000, 30_000);
        let swapped = ledger(0, 30_000, 20_000);
        assert_eq!(money().check(&before, &swapped), Ok(()));
    }

    #[test]
    fn money_created_out_of_nothing_is_refused() {
        let before = ledger(100_000, 20_000, 30_000);
        let after = ledger(100_000, 20_001, 30_000);
        let err = money().check(&before, &after).unwrap_err();
        assert_eq!(err.rule_id, "money_conserved");
        assert!(err.detail.contains("created 1 minor units"), "{}", err.detail);
    }

    #[test]
    fn money_destroyed_is_refused_too() {
        let before = ledger(100_000, 20_000, 30_000);
        let after = ledger(99_999, 20_000, 30_000);
        let err = money().check(&before, &after).unwrap_err();
        assert!(err.detail.contains("destroyed 1 minor units"), "{}", err.detail);
    }

    #[test]
    fn one_minor_unit_is_a_violation_because_the_law_is_exact() {
        // "A law that is false in the seventh decimal place" is the failure
        // mode tolerance exists to prevent. At integer minor units there is no
        // seventh decimal place to be false in.
        let before = ledger(1, 0, 0);
        let after = ledger(0, 0, 0);
        assert!(money().check(&before, &after).is_err());
    }

    #[test]
    fn a_new_pocket_appearing_with_money_in_it_is_refused() {
        // The clearest form of "created": a pocket that did not exist now holds
        // funds nobody moved into it.
        let before = ledger(100_000, 20_000, 30_000);
        let mut after = before.clone();
        after["finances"]["pockets"]["holiday"] = json!({"allocated": 5_000});
        assert!(money().check(&before, &after).is_err());
    }

    #[test]
    fn an_empty_pocket_appearing_is_fine() {
        let before = ledger(100_000, 20_000, 30_000);
        let mut after = before.clone();
        after["finances"]["pockets"]["holiday"] = json!({"allocated": 0});
        assert_eq!(money().check(&before, &after), Ok(()));
    }

    #[test]
    fn a_non_integer_under_an_exact_law_is_refused_not_rounded() {
        let before = ledger(100_000, 20_000, 30_000);
        let mut after = before.clone();
        after["finances"]["liquid"]["balance"] = json!(99_999.5);
        let err = money().check(&before, &after).unwrap_err();
        assert!(err.detail.contains("fractional"), "{}", err.detail);
    }

    #[test]
    fn a_continuous_quantity_may_declare_a_tolerance() {
        let rule = TransitionRule::Conservation {
            id: "mass".into(),
            quantity: Quantity::new("mass", &["tanks[*].kg"]),
            tolerance: Tolerance::Within(0.01),
        };
        let before = json!({"tanks": {"a": {"kg": 1.0}, "b": {"kg": 2.0}}});
        let within = json!({"tanks": {"a": {"kg": 1.005}, "b": {"kg": 1.995}}});
        let beyond = json!({"tanks": {"a": {"kg": 1.5}, "b": {"kg": 2.0}}});
        assert_eq!(rule.check(&before, &within), Ok(()));
        assert!(rule.check(&before, &beyond).is_err());
    }

    // ── the family is more expressive than a state constraint (§I) ───────────

    #[test]
    fn every_single_state_satisfies_a_transition_rule_vacuously() {
        // The article's own point: these say nothing about the endpoints in
        // isolation. Checking a state against itself is always fine, for every
        // shape — which is exactly why a state constraint cannot simulate one.
        let s = ledger(100_000, 20_000, 30_000);
        for rule in [
            money(),
            TransitionRule::RateLimit { id: "r".into(), path: "finances.liquid.balance".into(), delta: 0.0 },
            TransitionRule::Monotone {
                id: "m".into(),
                path: "finances.liquid.balance".into(),
                direction: Direction::NonIncreasing,
            },
        ] {
            assert_eq!(rule.check(&s, &s), Ok(()), "{} should hold vacuously", rule.id());
        }
    }

    // ── rate limit ───────────────────────────────────────────────────────────

    #[test]
    fn a_rate_limit_bounds_one_step_not_the_endpoint() {
        let rule = TransitionRule::RateLimit {
            id: "no_big_jumps".into(),
            path: "finances.liquid.balance".into(),
            delta: 5_000.0,
        };
        assert_eq!(rule.check(&ledger(100_000, 0, 0), &ledger(95_000, 0, 0)), Ok(()));
        let err = rule.check(&ledger(100_000, 0, 0), &ledger(90_000, 0, 0)).unwrap_err();
        assert!(err.detail.contains("moved by 10000"), "{}", err.detail);
    }

    #[test]
    fn a_rate_limit_is_symmetric() {
        let rule = TransitionRule::RateLimit {
            id: "r".into(), path: "finances.liquid.balance".into(), delta: 100.0,
        };
        assert!(rule.check(&ledger(0, 0, 0), &ledger(500, 0, 0)).is_err(), "up counts too");
    }

    // ── monotonicity ─────────────────────────────────────────────────────────

    #[test]
    fn stock_may_only_decrease() {
        let rule = TransitionRule::Monotone {
            id: "stock_only_falls".into(),
            path: "stock.units".into(),
            direction: Direction::NonIncreasing,
        };
        let before = json!({"stock": {"units": 10}});
        assert_eq!(rule.check(&before, &json!({"stock": {"units": 4}})), Ok(()));
        assert_eq!(rule.check(&before, &before), Ok(()), "unchanged is non-increasing");
        assert!(rule.check(&before, &json!({"stock": {"units": 11}})).is_err());
    }

    #[test]
    fn a_meter_never_runs_backwards() {
        let rule = TransitionRule::Monotone {
            id: "meter".into(), path: "meter.kwh".into(), direction: Direction::NonDecreasing,
        };
        let before = json!({"meter": {"kwh": 100}});
        assert_eq!(rule.check(&before, &json!({"meter": {"kwh": 101}})), Ok(()));
        assert!(rule.check(&before, &json!({"meter": {"kwh": 99}})).is_err());
    }

    // ── conjunction is the only combinator (§I) ──────────────────────────────

    #[test]
    fn adding_a_rule_can_only_shrink_what_is_admissible() {
        let before = ledger(100_000, 20_000, 30_000);
        let after = ledger(90_000, 30_000, 30_000);
        assert_eq!(check_all(&[money()], &before, &after), Ok(()));

        let tight = TransitionRule::RateLimit {
            id: "no_big_jumps".into(), path: "finances.liquid.balance".into(), delta: 1_000.0,
        };
        let err = check_all(&[money(), tight], &before, &after).unwrap_err();
        assert_eq!(err.rule_id, "no_big_jumps", "the refusal names which law refused");
    }

    // ── fail-safe: an unevaluable rule refuses, never passes ─────────────────

    #[test]
    fn a_path_that_does_not_resolve_refuses_rather_than_defaulting_to_zero() {
        let rule = TransitionRule::RateLimit {
            id: "r".into(), path: "nowhere.at.all".into(), delta: 1.0,
        };
        let s = ledger(0, 0, 0);
        let err = rule.check(&s, &s).unwrap_err();
        assert!(err.detail.contains("does not resolve"), "{}", err.detail);
    }

    #[test]
    fn a_rate_limit_over_a_wildcard_is_refused_as_ambiguous() {
        let rule = TransitionRule::RateLimit {
            id: "r".into(), path: "finances.pockets[*].allocated".into(), delta: 1.0,
        };
        let s = ledger(0, 1, 2);
        let err = rule.check(&s, &s).unwrap_err();
        assert!(err.detail.contains("resolves to 2 values"), "{}", err.detail);
    }

    // ── authoring-time typing ────────────────────────────────────────────────

    #[test]
    fn a_conservation_law_over_nothing_is_rejected_at_authoring_time() {
        let rule = TransitionRule::Conservation {
            id: "empty".into(),
            quantity: Quantity { id: "nothing".into(), paths: vec![] },
            tolerance: Tolerance::Exact,
        };
        assert!(matches!(rule.typecheck(), Err(TransitionDeclError::EmptyQuantity { .. })),
                "an empty sum is conserved vacuously and asserts nothing");
    }

    #[test]
    fn a_negative_delta_is_rejected_at_authoring_time() {
        let rule = TransitionRule::RateLimit {
            id: "r".into(), path: "a.b".into(), delta: -1.0,
        };
        assert!(matches!(rule.typecheck(), Err(TransitionDeclError::BadDelta { .. })));
    }

    #[test]
    fn a_nan_tolerance_is_rejected_at_authoring_time() {
        let rule = TransitionRule::Conservation {
            id: "c".into(),
            quantity: Quantity::new("q", &["a.b"]),
            tolerance: Tolerance::Within(f64::NAN),
        };
        assert!(matches!(rule.typecheck(), Err(TransitionDeclError::BadTolerance { .. })));
    }

    #[test]
    fn a_well_formed_rule_typechecks() {
        assert_eq!(money().typecheck(), Ok(()));
    }

    /// The structural claim, recorded as a test even though the compiler is the
    /// real proof.
    #[test]
    fn a_transition_rule_cannot_be_given_a_clamp_strategy() {
        // `TransitionRule` has no strategy field, so `strategy: Strategy::Clamp`
        // does not compile on any variant. Capping a transfer's outflow while
        // leaving the inflow untouched creates money — it satisfies C by
        // violating D — and the way to prevent that is to make it unspellable,
        // not to check for it.
        let rule = money();
        assert_eq!(rule.kind(), "conservation law");
    }
}
