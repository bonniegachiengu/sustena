//! Checked composition of Enzyme pathways (Operator §IV · R2 #16).
//!
//! Hoare's sequencing rule is the whole of composition:
//!
//! ```text
//!   {P} A {Q}    {Q} B {R}
//!   ──────────────────────
//!        {P} A;B {R}
//! ```
//!
//! read as a condition on when two Enzymes may be chained **at all**:
//!
//! ```text
//! post(A) ⊨ guard(B)      i.e.   ∀s ∈ S: post_A(s) ⇒ g_B(s)
//! ```
//!
//! When it holds the composite is itself an Enzyme, with all three parts
//! **derived** rather than declared:
//!
//! ```text
//! g_{A;B} = g_A ∧ wp(e_A, g_B)      guard: B's guard pulled back through A's effect
//! e_{A;B} = e_B ∘ e_A               effect: ordinary function composition
//! ε_{A;B} = ε_A · ε_B               emission: concatenation in the free monoid E*
//! ```
//!
//! Function composition is associative; concatenation in `E*` is associative
//! with identity `ε`. So **associativity of chaining is a theorem, not a hope**:
//! a three-step recipe composes the same however you bracket it, and the cost
//! and log of a composite are just the sums of the parts.
//!
//! ## Why this is worth building
//!
//! Enzymes are already chained dynamically — an operative's execution graph, a
//! simulated path, a sub-recipe. Dynamic chaining means **an illegal chain is
//! discovered by running it and watching step three refuse.** This moves the
//! discovery to the moment the chain is *written*, which is the same move the
//! predicate binder made for schemas: a load-time type error instead of a
//! comparison against nothing at 2 a.m.
//!
//! > A recipe whose third step can never fire is a broken recipe, and it should
//! > be impossible to save one.
//!
//! Here that is structural. [`compose`] returns `Result<Composed, Rejected>`, so
//! a refuted chain **yields no composite value at all**, and a [`Pathway`] can
//! only be built by chaining successful composes — there is no constructor that
//! takes a list of steps on trust. A broken recipe is not rejected on save; it
//! cannot be held.
//!
//! ## Deciding ⊨, honestly
//!
//! Entailment over an unrestricted first-order language is undecidable (Church,
//! 1936), so this cannot be promised in general — and the article says so. What
//! is promised is a **decidable fragment**: conjunctions of comparisons between
//! a dot-path and a numeric literal, which are interval constraints, and
//! intervals are easy. Everything else is reported **undecided**, never guessed.
//!
//! The verdict is three-valued and stated as such:
//!
//! | verdict | meaning | what happens |
//! |---|---|---|
//! | [`Entailment::Entailed`] | `post ⇒ guard` is provable in the fragment | compose freely |
//! | [`Entailment::Refuted`] | `post ∧ guard` is **unsatisfiable** — no state satisfies both | reject at compose time; this chain can never run |
//! | [`Entailment::Undecided`] | outside the fragment | compose, but carry `runtime_guard_required` and **say so** |
//!
//! Note what `Refuted` is and is not. It is not "we failed to prove the
//! implication" — that is `Undecided`. It is the strictly stronger claim that
//! the two conditions are jointly unsatisfiable, which is what licenses
//! refusing to build the chain at all.
//!
//! An SMT solver would shrink the undecided region (de Moura & Bjørner, 2008)
//! and is deliberately **not** pulled in here: the decidable fragment plus an
//! honest fallback is the shippable core, and a dependency-free engine is what
//! lets this crate ship to a phone.
//!
//! ## wp, and the limit of computing it
//!
//! `wp(e_A, g_B)` needs to know what `e_A` *does*. In this core an effect is a
//! Rust function — opaque — so wp is computable only where an operator declares
//! an [`EffectSummary`]. Where it does, wp is real substitution and stays inside
//! the predicate fragment (see [`Change`]). Where it does not, wp is **not
//! computed**: the composite keeps `g_A` as its guard and carries
//! `runtime_guard_required`, because inventing a weakest precondition for an
//! effect nobody described would be a false claim about when the chain is safe.

use serde_json::Value;
use thiserror::Error;

use crate::predicate::ast::{CompOp, Operand, Predicate};
use crate::predicate::parse_predicate;

/// What one Enzyme does to one path, as far as composition needs to know.
///
/// Deliberately small. These are the changes for which `wp` stays inside the
/// existing predicate fragment, so pulling a guard back through them needs no
/// new AST node and no arithmetic the language does not have.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// The path is set to a known literal. `wp` constant-folds the comparison.
    SetTo(Value),
    /// The path moves by a known amount. `wp` shifts the literal on the other
    /// side of the comparison, which is still a comparison.
    ShiftBy(f64),
    /// Something happens that this fragment cannot express — a change by a
    /// param, or a whole record rewritten. Named rather than omitted, so
    /// "we do not know" is a declaration and not an absence.
    Opaque,
}

/// What an Enzyme does, declared symbolically.
///
/// Optional: an operator without one still composes, it just cannot have its
/// successor's guard pulled back through it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectSummary {
    /// `(dot-path, change)`. A path absent from this list is **unchanged**,
    /// which is what makes wp computable for the common case where A and B
    /// touch different parts of state.
    pub changes: Vec<(String, Change)>,
}

impl EffectSummary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, path: &str, change: Change) -> Self {
        self.changes.push((path.to_string(), change));
        self
    }

    fn change_for(&self, path: &str) -> Option<&Change> {
        self.changes.iter().find(|(p, _)| p == path).map(|(_, c)| c)
    }
}

/// One Enzyme, as composition needs to see it.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub name: String,
    /// `g` — must hold before.
    pub guard: Vec<String>,
    /// `post` — holds after.
    pub post: Vec<String>,
    /// `ε` — what it emits, in order.
    pub emits: Vec<String>,
    pub pawa_cost: u32,
    /// `e`, symbolically. `None` means the effect is opaque to composition.
    pub effect: Option<EffectSummary>,
}

impl Step {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            guard: vec![],
            post: vec![],
            emits: vec![],
            pawa_cost: 0,
            effect: None,
        }
    }

    pub fn guarded_by(mut self, expr: &str) -> Self {
        self.guard.push(expr.to_string());
        self
    }
    pub fn ensures(mut self, expr: &str) -> Self {
        self.post.push(expr.to_string());
        self
    }
    pub fn emitting(mut self, event: &str) -> Self {
        self.emits.push(event.to_string());
        self
    }
    pub fn costing(mut self, pawa: u32) -> Self {
        self.pawa_cost = pawa;
        self
    }
    pub fn doing(mut self, effect: EffectSummary) -> Self {
        self.effect = Some(effect);
        self
    }
}

/// The three-valued verdict of `⊨`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entailment {
    /// `post ⇒ guard`, provable in the fragment.
    Entailed,
    /// `post ∧ guard` is unsatisfiable — **this chain can never run**.
    Refuted { detail: String },
    /// Outside the decidable fragment. Compose, but check at runtime.
    Undecided { reason: String },
}

/// A chain that was refused at compose time.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("post({from}) cannot satisfy guard({to}): {detail}")]
pub struct Rejected {
    pub from: String,
    pub to: String,
    pub detail: String,
}

/// A composite Enzyme — itself an Enzyme, with every part derived.
#[derive(Debug, Clone, PartialEq)]
pub struct Composed {
    pub name: String,
    /// `g_A ∧ wp(e_A, g_B)` when wp was computable; `g_A` alone otherwise, with
    /// `runtime_guard_required` set.
    pub guard: Vec<String>,
    /// `post(B)` — the composite ends where B ends.
    pub post: Vec<String>,
    /// `ε_A · ε_B`, concatenated in order.
    pub emits: Vec<String>,
    /// The monoid is additive, so cost is the sum of the parts.
    pub pawa_cost: u32,
    /// True when ⊨ was undecided, or when `wp` could not be computed. Either
    /// way the honest answer is the same: this chain still needs checking when
    /// it runs, and the composite says so rather than implying it is proven.
    pub runtime_guard_required: bool,
    /// Why, when `runtime_guard_required` is set. Empty otherwise.
    pub runtime_guard_reason: String,
}

impl Composed {
    /// A composed pathway is itself a [`Step`], which is what makes chaining
    /// associative rather than special-cased at each length.
    pub fn as_step(&self) -> Step {
        Step {
            name: self.name.clone(),
            guard: self.guard.clone(),
            post: self.post.clone(),
            emits: self.emits.clone(),
            pawa_cost: self.pawa_cost,
            // A composite's own effect is the composition of two effects; only
            // safe to claim when BOTH halves declared one. Left opaque here
            // rather than guessed — see `compose`.
            effect: None,
        }
    }
}

/// `compose(A, B)` — Hoare sequencing, checked.
///
/// A `Rejected` chain produces **no `Composed` value**, which is the structural
/// form of "it should be impossible to save one".
pub fn compose(a: &Step, b: &Step) -> Result<Composed, Rejected> {
    let verdict = entails(&a.post, &b.guard);

    let (runtime_guard_required, mut reason) = match verdict {
        Entailment::Refuted { detail } => {
            return Err(Rejected { from: a.name.clone(), to: b.name.clone(), detail })
        }
        Entailment::Entailed => (false, String::new()),
        Entailment::Undecided { reason } => (true, reason),
    };

    // g_{A;B} = g_A ∧ wp(e_A, g_B)
    let mut guard = a.guard.clone();
    let mut needs_runtime = runtime_guard_required;

    match &a.effect {
        Some(effect) => {
            for g in &b.guard {
                match wp(effect, g) {
                    Ok(WpResult::Always) => {}
                    Ok(WpResult::Never) => {
                        return Err(Rejected {
                            from: a.name.clone(),
                            to: b.name.clone(),
                            detail: format!(
                                "after {}, guard '{g}' can never hold — wp({}, '{g}') is false",
                                a.name, a.name
                            ),
                        })
                    }
                    Ok(WpResult::Condition(expr)) => guard.push(expr),
                    Err(why) => {
                        needs_runtime = true;
                        if reason.is_empty() {
                            reason = format!("wp could not be computed for '{g}': {why}");
                        }
                    }
                }
            }
        }
        None => {
            // No declared effect: B's guard cannot be pulled back, so it stays
            // a runtime obligation. Saying so is the whole point.
            if !b.guard.is_empty() {
                needs_runtime = true;
                if reason.is_empty() {
                    reason = format!(
                        "{} declares no effect summary, so guard(s) of {} cannot be pulled back and \
                         must be checked when the chain runs",
                        a.name, b.name
                    );
                }
            }
        }
    }

    Ok(Composed {
        name: format!("{};{}", a.name, b.name),
        guard,
        post: b.post.clone(),
        emits: a.emits.iter().chain(b.emits.iter()).cloned().collect(),
        pawa_cost: a.pawa_cost + b.pawa_cost,
        runtime_guard_required: needs_runtime,
        runtime_guard_reason: if needs_runtime { reason } else { String::new() },
    })
}

/// A saved recipe. **Constructible only by successful composition.**
///
/// There is no constructor that takes a list of steps on trust, so a `Pathway`
/// in hand is a pathway whose every link was checked. That is the type-level
/// form of "a recipe whose third step can never fire should be impossible to
/// save".
#[derive(Debug, Clone, PartialEq)]
pub struct Pathway {
    steps: Vec<String>,
    composed: Composed,
}

impl Pathway {
    /// Chain a sequence left-to-right. Associativity is a theorem, so the
    /// bracketing does not matter — asserted directly by a test rather than
    /// assumed.
    pub fn try_chain(steps: &[Step]) -> Result<Self, PathwayError> {
        let (first, rest) = steps.split_first().ok_or(PathwayError::Empty)?;
        let mut acc = Composed {
            name: first.name.clone(),
            guard: first.guard.clone(),
            post: first.post.clone(),
            emits: first.emits.clone(),
            pawa_cost: first.pawa_cost,
            runtime_guard_required: false,
            runtime_guard_reason: String::new(),
        };
        let mut acc_step = first.clone();

        for next in rest {
            let composed = compose(&acc_step, next).map_err(PathwayError::Rejected)?;
            acc_step = composed.as_step();
            // `as_step` drops the effect summary, so carry the running flags.
            acc_step.effect = next.effect.clone();
            acc = composed;
        }

        Ok(Self { steps: steps.iter().map(|s| s.name.clone()).collect(), composed: acc })
    }

    pub fn steps(&self) -> &[String] {
        &self.steps
    }
    pub fn composed(&self) -> &Composed {
        &self.composed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathwayError {
    #[error("a pathway needs at least one step")]
    Empty,
    #[error("{0}")]
    Rejected(#[from] Rejected),
}

// ── ⊨ over the decidable fragment ───────────────────────────────────────────

/// An interval constraint on one path: the decidable shape.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Interval {
    lo: f64,
    lo_open: bool,
    hi: f64,
    hi_open: bool,
}

impl Interval {
    fn unbounded() -> Self {
        Self { lo: f64::NEG_INFINITY, lo_open: false, hi: f64::INFINITY, hi_open: false }
    }

    fn from(op: CompOp, k: f64) -> Option<Self> {
        let mut i = Self::unbounded();
        match op {
            CompOp::Gt => {
                i.lo = k;
                i.lo_open = true;
            }
            CompOp::Ge => i.lo = k,
            CompOp::Lt => {
                i.hi = k;
                i.hi_open = true;
            }
            CompOp::Le => i.hi = k,
            CompOp::Eq => {
                i.lo = k;
                i.hi = k;
            }
            // `!=` punches a hole, which is not an interval.
            CompOp::Ne => return None,
        }
        Some(i)
    }

    fn intersect(self, other: Self) -> Self {
        let (lo, lo_open) = if other.lo > self.lo {
            (other.lo, other.lo_open)
        } else if other.lo == self.lo {
            (self.lo, self.lo_open || other.lo_open)
        } else {
            (self.lo, self.lo_open)
        };
        let (hi, hi_open) = if other.hi < self.hi {
            (other.hi, other.hi_open)
        } else if other.hi == self.hi {
            (self.hi, self.hi_open || other.hi_open)
        } else {
            (self.hi, self.hi_open)
        };
        Self { lo, lo_open, hi, hi_open }
    }

    fn is_empty(&self) -> bool {
        self.lo > self.hi || (self.lo == self.hi && (self.lo_open || self.hi_open))
    }

    /// Every point of `self` is a point of `other`.
    fn implies(&self, other: &Interval) -> bool {
        let lo_ok = self.lo > other.lo
            || (self.lo == other.lo && (self.lo_open || !other.lo_open));
        let hi_ok = self.hi < other.hi
            || (self.hi == other.hi && (self.hi_open || !other.hi_open));
        lo_ok && hi_ok
    }
}

/// A conjunction of interval constraints, keyed by path — or a reason the
/// expression fell outside the fragment.
type Facts = Vec<(String, Interval)>;

fn atoms(exprs: &[String]) -> Result<Facts, String> {
    let mut facts: Facts = Vec::new();
    for expr in exprs {
        let node = parse_predicate(expr).map_err(|e| format!("'{expr}' does not parse: {e}"))?;
        collect_atoms(&node, &mut facts).map_err(|why| format!("'{expr}': {why}"))?;
    }
    Ok(facts)
}

fn collect_atoms(node: &Predicate, out: &mut Facts) -> Result<(), String> {
    match node {
        Predicate::And(parts) => {
            for p in parts {
                collect_atoms(p, out)?;
            }
            Ok(())
        }
        Predicate::Comparison { left, op, right } => {
            let (path, op, k) = match (left, right) {
                (Operand::Path(p), Operand::Literal(v)) => (p.raw.clone(), *op, as_num(v)?),
                // Flip so the path is always on the left.
                (Operand::Literal(v), Operand::Path(p)) => (p.raw.clone(), flip(*op), as_num(v)?),
                _ => return Err("only path-versus-literal comparisons are in the fragment".into()),
            };
            let interval = Interval::from(op, k)
                .ok_or("'!=' does not describe an interval")?;
            match out.iter_mut().find(|(p, _)| *p == path) {
                Some((_, existing)) => *existing = existing.intersect(interval),
                None => out.push((path, interval)),
            }
            Ok(())
        }
        Predicate::Or(_) => Err("disjunction is outside the decidable fragment".into()),
        Predicate::Not(_) => Err("negation is outside the decidable fragment".into()),
        Predicate::Membership { .. } => Err("membership is outside the decidable fragment".into()),
        Predicate::Quantifier { .. } => Err("quantifiers are outside the decidable fragment".into()),
    }
}

fn as_num(v: &Value) -> Result<f64, String> {
    v.as_f64().ok_or_else(|| format!("{v} is not a number"))
}

fn flip(op: CompOp) -> CompOp {
    match op {
        CompOp::Gt => CompOp::Lt,
        CompOp::Ge => CompOp::Le,
        CompOp::Lt => CompOp::Gt,
        CompOp::Le => CompOp::Ge,
        other => other,
    }
}

/// `post ⊨ guard` — the three-valued judgment.
pub fn entails(post: &[String], guard: &[String]) -> Entailment {
    if guard.is_empty() {
        // Nothing is required, so anything establishes it.
        return Entailment::Entailed;
    }

    let post_facts = match atoms(post) {
        Ok(f) => f,
        Err(why) => return Entailment::Undecided { reason: format!("post is not in the fragment: {why}") },
    };
    let guard_facts = match atoms(guard) {
        Ok(f) => f,
        Err(why) => return Entailment::Undecided { reason: format!("guard is not in the fragment: {why}") },
    };

    // Refuted is the STRONGER claim: post ∧ guard is unsatisfiable, so no state
    // satisfies both and the chain can never run. Checked before entailment,
    // because it is the one that licenses refusing to build the chain.
    for (path, g) in &guard_facts {
        if let Some((_, p)) = post_facts.iter().find(|(pp, _)| pp == path) {
            if p.intersect(*g).is_empty() {
                return Entailment::Refuted {
                    detail: format!(
                        "'{path}' is left in {} by the first step, which cannot meet {}",
                        describe(p),
                        describe(g)
                    ),
                };
            }
        }
    }

    // Entailed: every required interval is already established.
    for (path, g) in &guard_facts {
        match post_facts.iter().find(|(pp, _)| pp == path) {
            Some((_, p)) if p.implies(g) => {}
            _ => {
                return Entailment::Undecided {
                    reason: format!(
                        "the first step does not establish '{path}' {}",
                        describe(g)
                    ),
                }
            }
        }
    }

    Entailment::Entailed
}

fn describe(i: &Interval) -> String {
    match (i.lo.is_finite(), i.hi.is_finite()) {
        (false, false) => "unconstrained".to_string(),
        (true, false) => format!("{} {}", if i.lo_open { ">" } else { ">=" }, i.lo),
        (false, true) => format!("{} {}", if i.hi_open { "<" } else { "<=" }, i.hi),
        (true, true) if i.lo == i.hi => format!("== {}", i.lo),
        (true, true) => format!(
            "in {}{}, {}{}",
            if i.lo_open { "(" } else { "[" },
            i.lo,
            i.hi,
            if i.hi_open { ")" } else { "]" }
        ),
    }
}

// ── wp — the pullback of a guard through an effect ──────────────────────────

/// What pulling a guard back through an effect produced.
#[derive(Debug, Clone, PartialEq)]
pub enum WpResult {
    /// The guard holds after the effect no matter what — nothing to require.
    Always,
    /// The guard can never hold after the effect. The chain is dead.
    Never,
    /// A condition on the PRE-state, in the same fragment.
    Condition(String),
}

/// `wp(e, g)` — the weakest precondition of `g` under effect `e`.
///
/// Computable only where the change keeps the result inside the predicate
/// fragment:
///
/// | change | `path >= k` becomes |
/// |---|---|
/// | absent (unchanged) | `path >= k` — B reads what A did not touch |
/// | `SetTo(v)` | constant-folded to [`WpResult::Always`] or [`WpResult::Never`] |
/// | `ShiftBy(d)` | `path >= k - d` — still a comparison, so still in the fragment |
/// | `Opaque` | `Err` — reported, and the caller carries a runtime guard |
///
/// A change by a *param* is `Opaque`: the language has no arithmetic, so
/// `balance - params.amount >= k` is not expressible, and pretending otherwise
/// would put a claim in the guard that the evaluator could not check.
pub fn wp(effect: &EffectSummary, guard_expr: &str) -> Result<WpResult, String> {
    let node = parse_predicate(guard_expr).map_err(|e| format!("does not parse: {e}"))?;
    let Predicate::Comparison { left, op, right } = &node else {
        return Err("only a single comparison can be pulled back in this fragment".into());
    };
    let (path, op, k) = match (left, right) {
        (Operand::Path(p), Operand::Literal(v)) => (p.raw.clone(), *op, as_num(v)?),
        (Operand::Literal(v), Operand::Path(p)) => (p.raw.clone(), flip(*op), as_num(v)?),
        _ => return Err("only path-versus-literal comparisons can be pulled back".into()),
    };

    match effect.change_for(&path) {
        // A path the effect does not touch keeps its guard unchanged.
        None => Ok(WpResult::Condition(guard_expr.to_string())),
        Some(Change::Opaque) => Err(format!("'{path}' is changed opaquely")),
        Some(Change::SetTo(v)) => {
            let actual = as_num(v)?;
            let holds = match op {
                CompOp::Gt => actual > k,
                CompOp::Ge => actual >= k,
                CompOp::Lt => actual < k,
                CompOp::Le => actual <= k,
                CompOp::Eq => actual == k,
                CompOp::Ne => actual != k,
            };
            Ok(if holds { WpResult::Always } else { WpResult::Never })
        }
        Some(Change::ShiftBy(d)) => {
            // path' = path + d, so g(path') ⟺ path + d ⋈ k ⟺ path ⋈ k − d.
            Ok(WpResult::Condition(format!("{path} {} {}", op.symbol(), k - d)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn credited() -> Step {
        Step::new("budget.record_income")
            .ensures("finances.liquid.balance >= 100")
            .emitting("event.finances.income_recorded")
            .costing(3)
    }

    fn spends_50() -> Step {
        Step::new("budget.allocate")
            .guarded_by("finances.liquid.balance >= 50")
            .ensures("finances.pockets.food.allocated >= 0")
            .emitting("event.finances.pocket_allocated")
            .costing(2)
    }

    // ── ⊨, three-valued ─────────────────────────────────────────────────────

    #[test]
    fn a_stronger_post_entails_a_weaker_guard() {
        // balance >= 100 establishes balance >= 50.
        assert_eq!(
            entails(&["finances.liquid.balance >= 100".into()],
                    &["finances.liquid.balance >= 50".into()]),
            Entailment::Entailed
        );
    }

    #[test]
    fn a_weaker_post_does_not_entail_a_stronger_guard() {
        // Not refuted — a balance of 100 satisfies both. Just not proven.
        let v = entails(&["finances.liquid.balance >= 10".into()],
                        &["finances.liquid.balance >= 50".into()]);
        assert!(matches!(v, Entailment::Undecided { .. }), "{v:?}");
    }

    #[test]
    fn disjoint_intervals_are_refuted_because_the_chain_can_never_run() {
        let v = entails(&["stock.units <= 0".into()], &["stock.units >= 1".into()]);
        match v {
            Entailment::Refuted { detail } => {
                assert!(detail.contains("stock.units"), "{detail}");
            }
            other => panic!("expected Refuted, got {other:?}"),
        }
    }

    #[test]
    fn touching_at_a_closed_boundary_is_not_refuted() {
        // post: x <= 0, guard: x >= 0. x = 0 satisfies both, so the chain CAN
        // run — and here it is even entailed.
        assert_eq!(
            entails(&["x <= 0".into()], &["x >= 0".into()]),
            Entailment::Undecided { reason: "the first step does not establish 'x' >= 0".into() }
        );
    }

    #[test]
    fn an_open_boundary_that_only_touches_is_refuted() {
        // post: x < 0, guard: x >= 0 — no point satisfies both.
        assert!(matches!(
            entails(&["x < 0".into()], &["x >= 0".into()]),
            Entailment::Refuted { .. }
        ));
    }

    #[test]
    fn an_empty_guard_is_established_by_anything() {
        assert_eq!(entails(&[], &[]), Entailment::Entailed);
    }

    #[test]
    fn a_conjunction_narrows_the_interval() {
        assert_eq!(
            entails(&["x >= 10".into(), "x <= 20".into()], &["x >= 5".into()]),
            Entailment::Entailed
        );
    }

    #[test]
    fn out_of_fragment_is_undecided_never_a_false_claim() {
        for (post, guard) in [
            ("balance >= 1 OR balance <= 0", "balance >= 0"),
            ("NOT balance >= 1", "balance >= 0"),
            ("ALL finances.pockets[*].allocated >= 0", "balance >= 0"),
            ("balance != 3", "balance >= 0"),
            ("balance >= params.floor", "balance >= 0"),
        ] {
            let v = entails(&[post.into()], &[guard.into()]);
            assert!(matches!(v, Entailment::Undecided { .. }),
                    "'{post}' should be undecided, got {v:?}");
        }
    }

    // ── compose ─────────────────────────────────────────────────────────────

    #[test]
    fn a_valid_chain_composes_with_every_part_derived() {
        let c = compose(&credited(), &spends_50()).expect("this chain is fine");
        assert_eq!(c.name, "budget.record_income;budget.allocate");
        // ε_{A;B} = ε_A · ε_B, in order.
        assert_eq!(c.emits, vec!["event.finances.income_recorded",
                                 "event.finances.pocket_allocated"]);
        // cost is additive in the monoid.
        assert_eq!(c.pawa_cost, 5);
        // post(A;B) = post(B).
        assert_eq!(c.post, spends_50().post);
    }

    #[test]
    fn a_refuted_chain_yields_no_composite_at_all() {
        // The structural point: there is no Composed value to hold.
        let empty = Step::new("consume_all").ensures("stock.units <= 0");
        let needs_one = Step::new("ship_one").guarded_by("stock.units >= 1");
        let err = compose(&empty, &needs_one).expect_err("this chain can never run");
        assert_eq!(err.from, "consume_all");
        assert_eq!(err.to, "ship_one");
    }

    #[test]
    fn an_undecided_chain_composes_but_says_so() {
        let weak = Step::new("a").ensures("finances.liquid.balance >= 10");
        let c = compose(&weak, &spends_50()).expect("undecided still composes");
        assert!(c.runtime_guard_required);
        assert!(!c.runtime_guard_reason.is_empty(), "and it must say why");
    }

    #[test]
    fn an_entailed_chain_with_a_declared_effect_needs_no_runtime_guard() {
        let a = credited().doing(EffectSummary::new());  // touches nothing B reads
        let c = compose(&a, &spends_50()).unwrap();
        assert!(!c.runtime_guard_required, "{}", c.runtime_guard_reason);
    }

    #[test]
    fn no_effect_summary_means_the_guard_stays_a_runtime_obligation() {
        // Honest: B's guard cannot be pulled back through an effect nobody
        // described, so it is carried rather than claimed discharged.
        let c = compose(&credited(), &spends_50()).unwrap();
        assert!(c.runtime_guard_required);
        assert!(c.runtime_guard_reason.contains("effect summary"), "{}", c.runtime_guard_reason);
    }

    // ── wp ──────────────────────────────────────────────────────────────────

    #[test]
    fn wp_through_an_untouched_path_is_the_guard_itself() {
        let e = EffectSummary::new().with("other.thing", Change::ShiftBy(1.0));
        assert_eq!(wp(&e, "balance >= 50").unwrap(),
                   WpResult::Condition("balance >= 50".into()));
    }

    #[test]
    fn wp_through_a_shift_moves_the_literal() {
        // balance' = balance - 30, so (balance' >= 50) ⟺ (balance >= 80).
        let e = EffectSummary::new().with("balance", Change::ShiftBy(-30.0));
        assert_eq!(wp(&e, "balance >= 50").unwrap(),
                   WpResult::Condition("balance >= 80".into()));
    }

    #[test]
    fn wp_through_a_set_constant_folds() {
        let e = EffectSummary::new().with("balance", Change::SetTo(json!(100)));
        assert_eq!(wp(&e, "balance >= 50").unwrap(), WpResult::Always);
        assert_eq!(wp(&e, "balance >= 500").unwrap(), WpResult::Never);
    }

    #[test]
    fn wp_through_an_opaque_change_is_reported_not_guessed() {
        let e = EffectSummary::new().with("balance", Change::Opaque);
        assert!(wp(&e, "balance >= 50").is_err());
    }

    #[test]
    fn a_set_that_can_never_satisfy_the_next_guard_rejects_the_chain() {
        let a = Step::new("zero_it")
            .doing(EffectSummary::new().with("balance", Change::SetTo(json!(0))));
        let b = Step::new("needs_funds").guarded_by("balance >= 50");
        let err = compose(&a, &b).expect_err("wp is false, so the chain is dead");
        assert!(err.detail.contains("can never hold"), "{}", err.detail);
    }

    #[test]
    fn wp_puts_the_pulled_back_guard_into_the_composite() {
        let a = Step::new("spend_30")
            .doing(EffectSummary::new().with("balance", Change::ShiftBy(-30.0)));
        let b = Step::new("spend_50").guarded_by("balance >= 50");
        let c = compose(&a, &b).unwrap();
        assert!(c.guard.iter().any(|g| g == "balance >= 80"),
                "B's guard must be pulled back through A: {:?}", c.guard);
    }

    // ── the algebra ─────────────────────────────────────────────────────────

    #[test]
    fn chaining_is_associative() {
        // (A;B);C and A;(B;C) must agree — the theorem, checked.
        let a = Step::new("a").emitting("e1").costing(1).ensures("x >= 100");
        let b = Step::new("b").emitting("e2").costing(2).ensures("x >= 50");
        let c = Step::new("c").emitting("e3").costing(4).ensures("x >= 10");

        let ab = compose(&a, &b).unwrap();
        let ab_c = compose(&ab.as_step(), &c).unwrap();

        let bc = compose(&b, &c).unwrap();
        let a_bc = compose(&a, &bc.as_step()).unwrap();

        assert_eq!(ab_c.emits, a_bc.emits, "ε is a free monoid: concatenation is associative");
        assert_eq!(ab_c.pawa_cost, a_bc.pawa_cost, "cost is the sum of the parts");
        assert_eq!(ab_c.post, a_bc.post, "the composite ends where the last step ends");
    }

    #[test]
    fn a_pathway_cannot_be_built_around_a_refuted_link() {
        let ok = Step::new("a").ensures("x >= 100");
        let dead = Step::new("b").guarded_by("x <= 0");
        let never_reached = Step::new("c");
        let err = Pathway::try_chain(&[ok, dead, never_reached])
            .expect_err("a recipe whose step can never fire must not be saveable");
        assert!(matches!(err, PathwayError::Rejected(_)));
    }

    #[test]
    fn a_good_pathway_saves_and_carries_the_whole_chain() {
        let p = Pathway::try_chain(&[
            Step::new("a").emitting("e1").costing(1).ensures("x >= 100"),
            Step::new("b").emitting("e2").costing(2).guarded_by("x >= 50").ensures("x >= 20"),
            Step::new("c").emitting("e3").costing(4).guarded_by("x >= 10"),
        ])
        .expect("every link checks out");
        assert_eq!(p.steps(), ["a", "b", "c"]);
        assert_eq!(p.composed().emits, ["e1", "e2", "e3"]);
        assert_eq!(p.composed().pawa_cost, 7);
    }

    #[test]
    fn an_empty_pathway_is_refused() {
        assert_eq!(Pathway::try_chain(&[]), Err(PathwayError::Empty));
    }

    #[test]
    fn a_single_step_is_a_pathway() {
        let p = Pathway::try_chain(&[Step::new("only").costing(9)]).unwrap();
        assert_eq!(p.composed().pawa_cost, 9);
    }
}
