//! The operative, and utility as a vector
//! (Operative §I + §II · OPV-1, OPV-3).
//!
//! ```text
//! ω_i = ⟨ u_i, Π_i, attention_i, M_self_i, M_world_i, dom_i ⟩
//!
//! sharing constraint:   ∀i,j:  S_i = S_j = S   ∧   T_i = T_j = T
//! ```
//!
//! > N agents, one world, one set of legal moves, differing **only in `u_i`**.
//!
//! ## ★ The sharing constraint is not checked — it is unrepresentable to violate
//!
//! [`Operative`] has **no `S` field and no `T` field**. There is nowhere to put
//! a private state space or a private move set, because both live on the one
//! [`Shared`] world that [`Omega`] owns. So the two propositions §I proves are
//! not runtime guarantees here; they are shapes:
//!
//! **Proposition 1 (no private move).** *The actions any operative can propose
//! are exactly `T`.* [`LegalMove`] has private fields and **no public
//! constructor** — the only way to obtain one is [`Shared::legal_move`], and a
//! [`Proposal`] carries one. An operative cannot name a move outside `T`
//! because it cannot construct the thing a proposal is made of.
//!
//! **Proposition 2 (monotone in nothing).** *Adding `ω_{N+1}` leaves `R(s)`
//! unchanged; it can only re-rank it.* [`Shared::reachable`] takes no operative
//! and no population — `Omega` is **not an argument to it**. Adding a member
//! cannot change a function it is not passed to. What it does change is
//! [`Omega::rank`], and a test asserts exactly that pair: reachable set
//! byte-identical, ranking genuinely different.
//!
//! > An operative is a *preference and a search procedure*, not a capability.
//!
//! ## ★ Utility is a VECTOR, and nothing here collapses it
//!
//! ```text
//! u_i : S → ℝ^m
//! x ≻ y  ⟺  ∀k: u_ik(x) ≥ u_ik(y)  ∧  ∃k: u_ik(x) > u_ik(y)
//! 𝒫(X) = { x ∈ X : ∄ y ∈ X, y ≻ x }
//! ```
//!
//! [`Utility::at`] returns `Vec<f64>` and [`Utility::delta`] returns `Vec<f64>`.
//! **There is no method on [`Utility`] that returns a scalar.** [`scalarise`]
//! exists as a free function and is documented as what it is — a demonstration
//! of the defect, not an API to reach for. Nothing in this module calls it.
//!
//! > Scalarisation `Σ_k λ_k u_ik` has a named defect: weighted sums recover
//! > only the **convex hull** of the frontier (Das & Dennis, 1997) [...] there
//! > exist non-dominated options **no weight vector will ever select**.
//! > Collapsing to a scalar does not hide a trade-off; **it deletes options.**
//!
//! That is demonstrated rather than repeated: three options, one of them
//! non-dominated and inside a non-convex notch, swept against a dense grid of
//! weight vectors, never selected by any of them — and kept by
//! [`pareto_frontier`]. A convex companion case is swept too, so the finding
//! reads as *scalarisation misses exactly the non-convex ones*, not as
//! *scalarisation never works*.
//!
//! §II's third point is honoured in the shape of the comparison: it is over a
//! **pair**, `Δu_i(o,s) = u_i(o(s)) − u_i(s)`, transition-shaped exactly as
//! §4E's `D(s,s')` is.
//!
//! ## Nash bargaining, and where the reference's arithmetic actually sits
//!
//! §II: Sustena's `nash_utility_score` *"computes the geometric mean of YES-vote
//! utilities against `d = 0.5` — the same object in the degenerate case where
//! every `u_i` is one-dimensional and normalised. A correct implementation of a
//! principled thing over an impoverished input."*
//!
//! [`nash_product`] is `∏_i (u_i − d_i)` with the disagreement point genuinely
//! subtracted, and [`geometric_mean`] is the reference's own arithmetic, so the
//! two can be compared rather than assumed equivalent. They order the same way
//! in the ordinary case and part company at exactly one place, which
//! [`NashOutcome::NoBargain`] names: **a party that gains nothing zeroes Nash's
//! product**, which is the single property the product is most known for, and
//! does not zero a geometric mean of raw scores.
//!
//! ## ★ A tension found while grounding, stated rather than resolved by fiat
//!
//! Nash bargaining takes **one scalar per party**. Applying it to vector
//! utilities therefore requires collapsing each agent's vector — which is
//! precisely the deletion §II spends its first half warning about. The two
//! results are not in conflict, but the order of operations matters and is not
//! arbitrary: compute the frontier **first**, losslessly and in vector form,
//! and bargain only among frontier points if a single pick is genuinely
//! required. That is also why *presenting the frontier rather than the winner*
//! is its own duty (OPV-29) rather than a UI preference.
//!
//! ## ★ A recorded trap, honoured at the point it would have been sprung
//!
//! OPV-16 flagged a name collision in advance: the reference's
//! `operatives.<n>.domain` is a **subject-matter tag list** (finance,
//! procurement), not the Cynefin axis, and reusing the key would make one of
//! the two inexpressible. So the axis here is [`Cynefin`], named for what it
//! is. A subject-matter tag list can still be added later under its own name,
//! and both stay sayable.
//!
//! ## Declared slots — clean seams, deliberately not stubbed
//!
//! `Π_i` (the operator-DAG, OPV-4) is app-built; `M_self`/`M_world` (OPV-11),
//! `attention` (OPV-7) and learning (OPV-6) are follow-ons. None of them has a
//! placeholder field here, because a declared-but-inert field reads as built
//! and is worse than an absence.
//!
//! ## Honest limits
//!
//! - **`u_i` reads declared dimensions; it does not transform them.** An
//!   objective says *I care about this dimension, in this direction*. A
//!   concave or thresholded transform on it is OPV-6's business, and inventing
//!   one here would be inventing preferences nobody declared.
//! - **The non-selectability demonstration is a dense sweep, not an analytic
//!   proof.** The analytic result is Das & Dennis's and is cited; the sweep is
//!   what this crate can check, and it reports how many weight vectors it
//!   tried rather than implying exhaustiveness.
//! - **`R(s)` is over a declared transition relation.** The world declares its
//!   moves and where they lead; nothing here infers a transition.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use crate::models::{Agreement, EffectiveN, WorldModel};

/// A point of `S` — declared dimension readings. `u_i` is evaluated here.
pub type StatePoint = BTreeMap<String, f64>;

// ---------------------------------------------------------------------------
// dom_i — the Cynefin axis, named for what it is (OPV-16's recorded trap)
// ---------------------------------------------------------------------------

/// `dom_i ⊆ {clear, complicated, complex, chaotic}` — §I's *suited domains*.
///
/// **Named `Cynefin`, not `Domain`**, because OPV-16 recorded the collision in
/// advance: the reference's `operatives.<n>.domain` is a subject-matter tag
/// list, and one key cannot carry both without making the other inexpressible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cynefin {
    Clear,
    Complicated,
    Complex,
    Chaotic,
}

impl Cynefin {
    pub const ALL: [Cynefin; 4] = [
        Cynefin::Clear,
        Cynefin::Complicated,
        Cynefin::Complex,
        Cynefin::Chaotic,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Cynefin::Clear => "clear",
            Cynefin::Complicated => "complicated",
            Cynefin::Complex => "complex",
            Cynefin::Chaotic => "chaotic",
        }
    }
}

// ---------------------------------------------------------------------------
// The one world — S and T live here, and nowhere else
// ---------------------------------------------------------------------------

/// One legal move, drawn from `T`.
///
/// **Private fields, no public constructor.** The only source is
/// [`Shared::legal_move`], which is what makes Proposition 1 structural rather
/// than promised: an operative cannot name a move outside `T` because it
/// cannot build one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LegalMove {
    name: String,
}

impl LegalMove {
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// `⟨S, T⟩` — the **one** world the whole population shares.
///
/// The sharing constraint `∀i,j: S_i = S_j ∧ T_i = T_j` is not enforced by a
/// check anywhere; it holds because there is exactly one of these and no
/// operative carries either half.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shared {
    /// `dim(S)` — the declared dimensions of the state space.
    dimensions: BTreeSet<String>,
    /// `T` — the legal moves.
    moves: BTreeSet<String>,
    /// The declared transition relation: `(state, move) → state`.
    transitions: BTreeMap<(String, String), String>,
}

impl Shared {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_dimension(mut self, name: &str) -> Self {
        self.dimensions.insert(name.to_string());
        self
    }

    /// Declare a member of `T`.
    pub fn with_move(mut self, name: &str) -> Self {
        self.moves.insert(name.to_string());
        self
    }

    /// Declare where a move leads. Refuses an undeclared move — a transition on
    /// something outside `T` would be a capability nobody declared.
    pub fn with_transition(mut self, from: &str, mv: &str, to: &str) -> Result<Self, OperativeError> {
        if !self.moves.contains(mv) {
            return Err(OperativeError::MoveNotInT(mv.to_string()));
        }
        self.transitions
            .insert((from.to_string(), mv.to_string()), to.to_string());
        Ok(self)
    }

    /// **The only source of a [`LegalMove`].**
    pub fn legal_move(&self, name: &str) -> Option<LegalMove> {
        self.moves.get(name).map(|n| LegalMove { name: n.clone() })
    }

    pub fn moves(&self) -> Vec<&str> {
        self.moves.iter().map(|s| s.as_str()).collect()
    }

    pub fn dimensions(&self) -> Vec<&str> {
        self.dimensions.iter().map(|s| s.as_str()).collect()
    }

    pub fn declares_dimension(&self, name: &str) -> bool {
        self.dimensions.contains(name)
    }

    /// Where one move leads from one state, if `T` declares it.
    ///
    /// ★ A read, added for OPV-4 so a strategy's reachable set can be computed
    /// **from `Shared`'s own table** rather than a copy of it — the bound has
    /// to come from the thing it is a bound on.
    pub fn next_state(&self, from: &str, mv: &str) -> Option<String> {
        self.transitions.get(&(from.to_string(), mv.to_string())).cloned()
    }

    /// `R(s)` — the states reachable by admitted move sequences.
    ///
    /// ★ **Proposition 2, at the type level.** No operative and no population
    /// appears in this signature, so adding a member cannot change a function
    /// it is never passed to. The reachable set is a property of `⟨S,T⟩` alone,
    /// which is exactly what §I proves.
    pub fn reachable(&self, from: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::new();
        seen.insert(from.to_string());
        queue.push_back(from.to_string());
        while let Some(s) = queue.pop_front() {
            for mv in &self.moves {
                if let Some(next) = self.transitions.get(&(s.clone(), mv.clone())) {
                    if seen.insert(next.clone()) {
                        queue.push_back(next.clone());
                    }
                }
            }
        }
        seen
    }
}

// ---------------------------------------------------------------------------
// u_i — the vector
// ---------------------------------------------------------------------------

/// Which way an objective reads its dimension.
///
/// Without this, dominance is meaningless: *more spent* and *more saved* cannot
/// both be improvements on the same axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sense {
    Maximise,
    Minimise,
}

/// One component `u_ik` — a named objective reading one declared dimension.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Objective {
    pub name: String,
    pub dimension: String,
    pub sense: Sense,
}

impl Objective {
    pub fn new(name: &str, dimension: &str, sense: Sense) -> Self {
        Self {
            name: name.to_string(),
            dimension: dimension.to_string(),
            sense,
        }
    }
}

/// `u_i : S → ℝ^m` — **a vector, and it stays one**.
///
/// There is deliberately no method here that returns a scalar. [`scalarise`] is
/// a free function that exists to demonstrate what collapsing deletes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Utility {
    objectives: Vec<Objective>,
}

impl Utility {
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuses a duplicate objective name — two components that cannot be told
    /// apart make a report of *which* objective drove a verdict unreadable.
    pub fn with(mut self, objective: Objective) -> Result<Self, OperativeError> {
        if self.objectives.iter().any(|o| o.name == objective.name) {
            return Err(OperativeError::DuplicateObjective(objective.name));
        }
        self.objectives.push(objective);
        Ok(self)
    }

    /// `m` — the number of components.
    pub fn m(&self) -> usize {
        self.objectives.len()
    }

    pub fn objectives(&self) -> &[Objective] {
        &self.objectives
    }

    /// `supp(u_i)` — the state dimensions this utility actually reads.
    ///
    /// Real rather than notional, which is what makes §XIV's
    /// `O = ⋃_i supp(u_i)` computable when the Goodhart guard (OPV-27) comes
    /// to need it.
    pub fn support(&self) -> BTreeSet<&str> {
        self.objectives.iter().map(|o| o.dimension.as_str()).collect()
    }

    /// `u_i(s) ∈ ℝ^m`.
    ///
    /// A dimension this utility reads but the point does not carry is
    /// **refused, not read as zero**: zero is a value, and substituting it
    /// would produce a confident comparison against a number nobody supplied.
    pub fn at(&self, point: &StatePoint) -> Result<Vec<f64>, OperativeError> {
        self.objectives
            .iter()
            .map(|o| {
                let v = point
                    .get(&o.dimension)
                    .copied()
                    .ok_or_else(|| OperativeError::DimensionMissing {
                        objective: o.name.clone(),
                        dimension: o.dimension.clone(),
                    })?;
                if !v.is_finite() {
                    return Err(OperativeError::NotFinite {
                        dimension: o.dimension.clone(),
                        value: v.to_string(),
                    });
                }
                Ok(match o.sense {
                    Sense::Maximise => v,
                    Sense::Minimise => -v,
                })
            })
            .collect()
    }

    /// `Δu_i(o, s) = u_i(o(s)) − u_i(s)` — **transition-shaped**, exactly as
    /// §4E's `D(s,s')` is, and still a vector.
    pub fn delta(&self, before: &StatePoint, after: &StatePoint) -> Result<Vec<f64>, OperativeError> {
        let b = self.at(before)?;
        let a = self.at(after)?;
        Ok(a.iter().zip(b.iter()).map(|(x, y)| x - y).collect())
    }

    /// `𝒫(X)` under this utility — the ids of the non-dominated alternatives.
    pub fn frontier(&self, alternatives: &[Alternative]) -> Result<Vec<String>, OperativeError> {
        let points: Vec<(String, Vec<f64>)> = alternatives
            .iter()
            .map(|a| Ok((a.id.clone(), self.at(&a.point)?)))
            .collect::<Result<_, OperativeError>>()?;
        pareto_frontier(&points)
    }
}

/// One option under consideration — an id and the state it would produce.
#[derive(Debug, Clone, PartialEq)]
pub struct Alternative {
    pub id: String,
    pub point: StatePoint,
}

impl Alternative {
    pub fn new(id: &str, readings: &[(&str, f64)]) -> Self {
        Self {
            id: id.to_string(),
            point: readings.iter().map(|(k, v)| ((*k).to_string(), *v)).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Pareto
// ---------------------------------------------------------------------------

/// The result of comparing two utility vectors.
///
/// Four-valued on purpose: `Incomparable` is the case a scalar cannot express
/// at all, and it is the case §II is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dominance {
    /// `x ≻ y`.
    Dominates,
    /// `y ≻ x`.
    DominatedBy,
    /// Equal on every component.
    Equal,
    /// ★ Better on some, worse on others — a genuine trade-off, and the thing
    /// collapsing to a scalar silently resolves by fiat.
    Incomparable,
}

/// `x ≻ y ⟺ ∀k: x_k ≥ y_k ∧ ∃k: x_k > y_k`.
///
/// Refuses vectors of different length: comparing an `ℝ^2` against an `ℝ^3` is
/// not a close call, it is a category error.
pub fn dominance(x: &[f64], y: &[f64]) -> Result<Dominance, OperativeError> {
    if x.len() != y.len() {
        return Err(OperativeError::DimensionMismatch {
            left: x.len(),
            right: y.len(),
        });
    }
    if x.is_empty() {
        return Err(OperativeError::EmptyUtility);
    }
    let all_ge = x.iter().zip(y).all(|(a, b)| a >= b);
    let all_le = x.iter().zip(y).all(|(a, b)| a <= b);
    let any_gt = x.iter().zip(y).any(|(a, b)| a > b);
    let any_lt = x.iter().zip(y).any(|(a, b)| a < b);
    Ok(match (all_ge && any_gt, all_le && any_lt) {
        (true, _) => Dominance::Dominates,
        (_, true) => Dominance::DominatedBy,
        _ if !any_gt && !any_lt => Dominance::Equal,
        _ => Dominance::Incomparable,
    })
}

/// `𝒫(X) = { x ∈ X : ∄ y ∈ X, y ≻ x }`.
///
/// Order-preserving in the input, so the answer is deterministic.
pub fn pareto_frontier(points: &[(String, Vec<f64>)]) -> Result<Vec<String>, OperativeError> {
    let mut out = Vec::new();
    for (id, x) in points {
        let mut dominated = false;
        for (other_id, y) in points {
            if other_id == id {
                continue;
            }
            if dominance(y, x)? == Dominance::Dominates {
                dominated = true;
                break;
            }
        }
        if !dominated {
            out.push(id.clone());
        }
    }
    Ok(out)
}

/// `u_i^λ = Σ_k λ_k u_ik`.
///
/// ★ **This exists to demonstrate the defect, not to be used.** Nothing in
/// this module calls it, and [`Utility`] has no scalar-returning method.
/// Weighted sums recover only the convex hull of the frontier (Das & Dennis,
/// 1997), so a non-dominated option in a non-convex region is invisible to
/// every weight vector — see the conformance case that sweeps a dense grid and
/// finds exactly that.
pub fn scalarise(point: &[f64], weights: &[f64]) -> Result<f64, OperativeError> {
    if point.len() != weights.len() {
        return Err(OperativeError::DimensionMismatch {
            left: point.len(),
            right: weights.len(),
        });
    }
    if weights.iter().any(|w| !w.is_finite() || *w < 0.0) {
        return Err(OperativeError::BadWeights);
    }
    Ok(point.iter().zip(weights).map(|(v, w)| v * w).sum())
}

// ---------------------------------------------------------------------------
// Nash bargaining
// ---------------------------------------------------------------------------

/// What [`nash_product`] found.
#[derive(Debug, Clone, PartialEq)]
pub enum NashOutcome {
    /// `∏_i (u_i − d_i)`.
    Score(f64),
    /// ★ Some party is at or below its disagreement point, so it would refuse.
    ///
    /// This is the **answer**, not a low score: outside the bargaining set no
    /// bargain is struck. It is also the one place the product parts company
    /// with a geometric mean of raw scores.
    NoBargain { refused_by: Vec<usize> },
}

/// Nash bargaining (1950): `argmax_x ∏_i (u_i(x) − d_i)`, characterised by
/// Pareto efficiency, symmetry, affine invariance and IIA.
///
/// The disagreement point is genuinely subtracted, which is what makes a party
/// that gains **nothing** zero the product.
pub fn nash_product(utilities: &[f64], disagreement: &[f64]) -> Result<NashOutcome, OperativeError> {
    if utilities.len() != disagreement.len() {
        return Err(OperativeError::DimensionMismatch {
            left: utilities.len(),
            right: disagreement.len(),
        });
    }
    if utilities.is_empty() {
        return Err(OperativeError::EmptyUtility);
    }
    let refused: Vec<usize> = utilities
        .iter()
        .zip(disagreement)
        .enumerate()
        .filter(|(_, (u, d))| *u <= *d)
        .map(|(i, _)| i)
        .collect();
    if !refused.is_empty() {
        return Ok(NashOutcome::NoBargain { refused_by: refused });
    }
    Ok(NashOutcome::Score(
        utilities.iter().zip(disagreement).map(|(u, d)| u - d).product(),
    ))
}

/// The reference engine's own arithmetic — `exp(mean(ln u))` over YES-vote
/// utilities — provided so the two can be **compared** rather than assumed
/// equivalent.
///
/// See the conformance cases: same ordering in the ordinary case, and one
/// precise place they differ.
pub fn geometric_mean(utilities: &[f64]) -> Result<f64, OperativeError> {
    if utilities.is_empty() {
        return Err(OperativeError::EmptyUtility);
    }
    let log_sum: f64 = utilities.iter().map(|u| u.max(1e-9).ln()).sum();
    Ok((log_sum / utilities.len() as f64).exp())
}

// ---------------------------------------------------------------------------
// ω and Ω
// ---------------------------------------------------------------------------

/// `ω_i = ⟨u_i, Π_i, attention_i, M_self_i, M_world_i, dom_i⟩` — the parts of
/// it this slice builds.
///
/// **No `S`, no `T`.** That absence *is* the sharing constraint, and it holds
/// for the assembled agent too.
///
/// ★★ **The omission comment this doc used to carry is now resolved, and the
/// resolution is not "the fields were added".** `Π` (OPV-4), `attention`
/// (OPV-7), `M_self`/`M_world` (OPV-11) and learning (OPV-6) were left off
/// because *a declared-but-inert field reads as built*. All four now exist —
/// so they are composed by [`crate::agent::Agent`], where every one of them is
/// **read on a real reasoning path**, rather than added here as `Option`s a
/// method might find missing. That would have been the same defect, committed
/// instead of dodged. This type stays the boundary: `⟨u_i, dom_i⟩`, and no
/// world.
///
/// ★ `M_world` is the exception that proves the rule: §VII says **one per
/// household**, so it hangs on [`Omega`] (see [`Omega::modelling`]) and no
/// operative owns a private one.
#[derive(Debug, Clone, PartialEq)]
pub struct Operative {
    id: String,
    utility: Utility,
    suited: BTreeSet<Cynefin>,
}

impl Operative {
    /// Refuses an empty utility: an operative with no objectives has no
    /// preference, and §I says a preference is what an operative *is*.
    pub fn new(id: &str, utility: Utility, suited: &[Cynefin]) -> Result<Self, OperativeError> {
        if id.is_empty() {
            return Err(OperativeError::EmptyId);
        }
        if utility.m() == 0 {
            return Err(OperativeError::EmptyUtility);
        }
        Ok(Self {
            id: id.to_string(),
            utility,
            suited: suited.iter().copied().collect(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn utility(&self) -> &Utility {
        &self.utility
    }

    /// `dom_i`.
    pub fn suited(&self) -> &BTreeSet<Cynefin> {
        &self.suited
    }

    /// `match(i, s)` — is this operative suited to the situation's domain?
    pub fn suited_to(&self, domain: Cynefin) -> bool {
        self.suited.contains(&domain)
    }

    /// ★ **Proposition 1, structurally.**
    ///
    /// The move must be drawn from the world, and [`LegalMove`] cannot be built
    /// any other way — so a proposal naming something outside `T` is not
    /// rejected here, it is unconstructible.
    pub fn propose(&self, world: &Shared, mv: &str) -> Result<Proposal, OperativeError> {
        let legal = world
            .legal_move(mv)
            .ok_or_else(|| OperativeError::MoveNotInT(mv.to_string()))?;
        Ok(Proposal {
            by: self.id.clone(),
            mv: legal,
        })
    }
}

/// What an operative can put forward.
///
/// Carries a [`LegalMove`], which only [`Shared`] can mint — so every proposal
/// that exists names an element of `T`.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    by: String,
    mv: LegalMove,
}

impl Proposal {
    pub fn by(&self) -> &str {
        &self.by
    }

    pub fn move_name(&self) -> &str {
        self.mv.name()
    }
}

/// How a population ranked a set of alternatives.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranking {
    /// `𝒫(X)` per operative — the ids each one finds non-dominated.
    pub by_operative: BTreeMap<String, Vec<String>>,
    /// ★ On **no** operative's frontier. Dominated for everyone, which is a
    /// far stronger statement than a low aggregate score.
    pub dominated_for_all: Vec<String>,
    /// On **at least one** operative's frontier — the set worth presenting
    /// (OPV-29's *present the frontier, not the winner*).
    pub on_some_frontier: Vec<String>,
}

/// `Ω` — the population. **Owns the one world**, which is why every member is
/// over the same `⟨S, T⟩` by construction rather than by agreement.
#[derive(Debug, Clone, PartialEq)]
pub struct Omega {
    world: Shared,
    members: Vec<Operative>,
    /// ★★★ `M_world` — **one per household**, per §VII. It lives here, on the
    /// population that already owns the one world, and **not** on any
    /// [`Operative`]: a private world-model per operative would contradict the
    /// article outright and would make the correlation below undetectable.
    ///
    /// `Option`, and honestly so: a population that has not authored a model
    /// does not have one, and [`crate::models::WorldModel::declared`] requires
    /// a fidelity claim with a basis — inventing one would be exactly the
    /// confidence §VII warns about. The absence is **read**, not merely
    /// stored: see [`Omega::agreement_over`].
    world_model: Option<WorldModel>,
}

impl Omega {
    pub fn over(world: Shared) -> Self {
        Self {
            world,
            members: Vec::new(),
            world_model: None,
        }
    }

    /// Declare the household's `M_world`.
    pub fn modelling(mut self, model: WorldModel) -> Self {
        self.world_model = Some(model);
        self
    }

    /// `M_world`, if this household has declared one.
    pub fn world_model(&self) -> Option<&WorldModel> {
        self.world_model.as_ref()
    }

    /// ★★★ **OPV-11's keystone, fired off real population state.**
    ///
    /// If these `headcount` operatives all agreed while reasoning through the
    /// household's **one** `M_world`, that is not `headcount` independent
    /// confirmations — forking gives independence of *sampling*, not of
    /// *assumptions*. [`Agreement::via_shared_model`] cannot return
    /// [`EffectiveN::Independent`], so the masquerade is unspellable rather
    /// than discouraged; and when no model is declared there genuinely is no
    /// shared assumption to correlate on, so the honest answer is
    /// `Independent`.
    ///
    /// ★ No estimate is offered for the shared case, because the shortfall
    /// depends on how wrong the model is — which nobody inside it can measure.
    pub fn agreement_over(&self, headcount: usize) -> EffectiveN {
        match &self.world_model {
            Some(m) => Agreement::via_shared_model(headcount, m).effective_n(),
            None => Agreement::independent(headcount).effective_n(),
        }
    }

    /// Refuses a duplicate id, and refuses an objective reading a dimension the
    /// world does not declare — a preference over something that is not part of
    /// `S` would score against a number that never exists.
    pub fn with(mut self, op: Operative) -> Result<Self, OperativeError> {
        if self.members.iter().any(|m| m.id == op.id) {
            return Err(OperativeError::DuplicateOperative(op.id));
        }
        for d in op.utility.support() {
            if !self.world.declares_dimension(d) {
                return Err(OperativeError::DimensionNotInS {
                    operative: op.id.clone(),
                    dimension: d.to_string(),
                });
            }
        }
        self.members.push(op);
        Ok(self)
    }

    pub fn world(&self) -> &Shared {
        &self.world
    }

    pub fn members(&self) -> &[Operative] {
        &self.members
    }

    pub fn size(&self) -> usize {
        self.members.len()
    }

    /// `R(s)` — delegated straight to the world, which is the whole point.
    ///
    /// ★ Proposition 2: this cannot depend on `self.members`, because
    /// [`Shared::reachable`] is not given them.
    pub fn reachable(&self, from: &str) -> BTreeSet<String> {
        self.world.reachable(from)
    }

    /// `O = ⋃_i supp(u_i)` — the **observed** set (§XIV).
    ///
    /// Here because it falls out of real supports for free, and because
    /// OPV-27's guard will need it. The guard itself is not built.
    pub fn observed_dimensions(&self) -> BTreeSet<&str> {
        self.members.iter().flat_map(|m| m.utility.support()).collect()
    }

    /// `U = dim(S) \ O` — the dimensions **no operative's score reads**.
    ///
    /// Reported, not acted on. §XIV is explicit that the fix is not another
    /// operative, which is why this is a read rather than a remedy.
    pub fn unwatched_dimensions(&self) -> BTreeSet<&str> {
        let observed = self.observed_dimensions();
        self.world
            .dimensions
            .iter()
            .map(|s| s.as_str())
            .filter(|d| !observed.contains(d))
            .collect()
    }

    /// ★ **What adding an operative CAN change** — the ranking, and only it.
    pub fn rank(&self, alternatives: &[Alternative]) -> Result<Ranking, OperativeError> {
        let mut by_operative = BTreeMap::new();
        let mut on_some: BTreeSet<String> = BTreeSet::new();
        for m in &self.members {
            let f = m.utility.frontier(alternatives)?;
            on_some.extend(f.iter().cloned());
            by_operative.insert(m.id.clone(), f);
        }
        let dominated_for_all = alternatives
            .iter()
            .map(|a| a.id.clone())
            .filter(|id| !on_some.contains(id))
            .collect();
        // Input order, so the answer is deterministic.
        let on_some_frontier = alternatives
            .iter()
            .map(|a| a.id.clone())
            .filter(|id| on_some.contains(id))
            .collect();
        Ok(Ranking {
            by_operative,
            dominated_for_all,
            on_some_frontier,
        })
    }
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Error)]
pub enum OperativeError {
    #[error("an operative needs an id")]
    EmptyId,
    #[error("a utility with no objectives is not a preference, and a preference is what an operative is")]
    EmptyUtility,
    #[error("duplicate objective '{0}' — two components that cannot be told apart make a verdict unreadable")]
    DuplicateObjective(String),
    #[error("duplicate operative '{0}'")]
    DuplicateOperative(String),
    #[error("'{0}' is not in T — an operative proposes only legal moves, and capability is not a function of N")]
    MoveNotInT(String),
    #[error("operative '{operative}' has an objective over '{dimension}', which the shared world does not declare as part of S")]
    DimensionNotInS { operative: String, dimension: String },
    #[error("objective '{objective}' reads dimension '{dimension}', which this point does not carry — refused rather than read as zero, because zero is a value")]
    DimensionMissing { objective: String, dimension: String },
    #[error("dimension '{dimension}' is not a usable reading: {value}")]
    NotFinite { dimension: String, value: String },
    #[error("cannot compare an ℝ^{left} against an ℝ^{right} — not a close call, a category error")]
    DimensionMismatch { left: usize, right: usize },
    #[error("scalarisation weights must be finite and non-negative")]
    BadWeights,
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> Shared {
        Shared::new()
            .with_dimension("savings")
            .with_dimension("free_time")
            .with_dimension("noise")
            .with_move("save")
            .with_move("rest")
            .with_transition("s0", "save", "s1")
            .expect("declared")
            .with_transition("s1", "rest", "s2")
            .expect("declared")
    }

    fn thrift() -> Operative {
        Operative::new(
            "mentor",
            Utility::new()
                .with(Objective::new("savings", "savings", Sense::Maximise))
                .unwrap(),
            &[Cynefin::Clear, Cynefin::Complicated],
        )
        .unwrap()
    }

    fn balance() -> Operative {
        Operative::new(
            "curator",
            Utility::new()
                .with(Objective::new("savings", "savings", Sense::Maximise))
                .unwrap()
                .with(Objective::new("rest", "free_time", Sense::Maximise))
                .unwrap(),
            &[Cynefin::Complex],
        )
        .unwrap()
    }

    // -- ★ Proposition 1 ----------------------------------------------------

    #[test]
    fn an_operative_can_propose_only_elements_of_t() {
        let w = world();
        let op = thrift();
        assert_eq!(op.propose(&w, "save").unwrap().move_name(), "save");
        assert!(matches!(
            op.propose(&w, "print_money"),
            Err(OperativeError::MoveNotInT(_))
        ));
    }

    #[test]
    fn a_transition_on_an_undeclared_move_is_refused() {
        assert!(matches!(
            Shared::new().with_transition("s0", "teleport", "s9"),
            Err(OperativeError::MoveNotInT(_))
        ));
    }

    // -- ★★ Proposition 2 ---------------------------------------------------

    #[test]
    fn adding_an_operative_leaves_the_reachable_set_unchanged_and_only_re_ranks() {
        let alts = [
            Alternative::new("a", &[("savings", 10.0), ("free_time", 0.0)]),
            Alternative::new("b", &[("savings", 0.0), ("free_time", 10.0)]),
        ];

        let one = Omega::over(world()).with(thrift()).unwrap();
        let r_before = one.reachable("s0");
        let rank_before = one.rank(&alts).unwrap();

        let two = one.clone().with(balance()).unwrap();
        let r_after = two.reachable("s0");
        let rank_after = two.rank(&alts).unwrap();

        // ★ Capability: byte-identical.
        assert_eq!(r_before, r_after);
        assert_eq!(r_after, ["s0", "s1", "s2"].map(String::from).into_iter().collect());
        // ★ Ranking: genuinely different — thrift alone discards "b", the pair
        //   keeps it, because it is on the curator's frontier.
        assert_ne!(rank_before, rank_after);
        assert_eq!(rank_before.dominated_for_all, vec!["b".to_string()]);
        assert!(rank_after.dominated_for_all.is_empty());
    }

    // -- ★★ the vector, and what scalarising deletes ------------------------

    /// ★★ Three options; `c` sits in a non-convex notch. It is non-dominated,
    /// and no weight vector selects it.
    #[test]
    fn a_non_convex_option_is_kept_by_the_frontier_and_selected_by_no_weight_vector() {
        let points = vec![
            ("a".to_string(), vec![10.0, 0.0]),
            ("b".to_string(), vec![0.0, 10.0]),
            ("c".to_string(), vec![4.0, 4.0]),
        ];
        // Non-dominated: a is better on k=0 and worse on k=1, b the reverse.
        assert_eq!(dominance(&points[0].1, &points[2].1).unwrap(), Dominance::Incomparable);
        assert_eq!(dominance(&points[1].1, &points[2].1).unwrap(), Dominance::Incomparable);
        assert_eq!(
            pareto_frontier(&points).unwrap(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );

        // ...and invisible to every weight vector on a dense sweep.
        let steps = 1000;
        let mut c_ever_won = false;
        for i in 0..=steps {
            let l0 = i as f64 / steps as f64;
            let w = [l0, 1.0 - l0];
            let scores: Vec<f64> = points.iter().map(|(_, p)| scalarise(p, &w).unwrap()).collect();
            let best = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            if scores[2] >= best - 1e-12 {
                c_ever_won = true;
            }
        }
        assert!(!c_ever_won, "collapsing to a scalar deletes 'c'");
    }

    /// The companion, so the finding is not misread as "scalarisation never
    /// works": on a convex frontier, a weight vector does select the middle.
    #[test]
    fn a_convex_option_is_selected_by_some_weight_vector() {
        let points = [[10.0, 0.0], [0.0, 10.0], [8.0, 8.0]];
        let w = [0.5, 0.5];
        let scores: Vec<f64> = points.iter().map(|p| scalarise(p, &w).unwrap()).collect();
        assert!(scores[2] > scores[0] && scores[2] > scores[1]);
    }

    #[test]
    fn dominance_is_four_valued_and_incomparable_is_a_real_answer() {
        assert_eq!(dominance(&[2.0, 2.0], &[1.0, 1.0]).unwrap(), Dominance::Dominates);
        assert_eq!(dominance(&[1.0, 1.0], &[2.0, 2.0]).unwrap(), Dominance::DominatedBy);
        assert_eq!(dominance(&[1.0, 1.0], &[1.0, 1.0]).unwrap(), Dominance::Equal);
        assert_eq!(dominance(&[2.0, 0.0], &[0.0, 2.0]).unwrap(), Dominance::Incomparable);
        // ∀k ≥ with no strict improvement is NOT dominance.
        assert_eq!(dominance(&[1.0, 2.0], &[1.0, 2.0]).unwrap(), Dominance::Equal);
    }

    #[test]
    fn comparing_vectors_of_different_length_is_refused() {
        assert!(matches!(
            dominance(&[1.0], &[1.0, 2.0]),
            Err(OperativeError::DimensionMismatch { .. })
        ));
    }

    // -- Δu, transition-shaped ----------------------------------------------

    #[test]
    fn delta_is_over_a_pair_and_is_still_a_vector() {
        let u = balance().utility().clone();
        let before: StatePoint = [("savings".into(), 100.0), ("free_time".into(), 5.0)].into();
        let after: StatePoint = [("savings".into(), 130.0), ("free_time".into(), 3.0)].into();
        assert_eq!(u.delta(&before, &after).unwrap(), vec![30.0, -2.0]);
    }

    #[test]
    fn a_minimised_objective_reads_the_other_way() {
        let u = Utility::new()
            .with(Objective::new("quiet", "noise", Sense::Minimise))
            .unwrap();
        let loud: StatePoint = [("noise".into(), 9.0)].into();
        let quiet: StatePoint = [("noise".into(), 1.0)].into();
        assert_eq!(
            dominance(&u.at(&quiet).unwrap(), &u.at(&loud).unwrap()).unwrap(),
            Dominance::Dominates
        );
    }

    #[test]
    fn a_dimension_the_point_does_not_carry_is_refused_not_read_as_zero() {
        let u = thrift().utility().clone();
        let empty: StatePoint = BTreeMap::new();
        assert!(matches!(
            u.at(&empty),
            Err(OperativeError::DimensionMissing { .. })
        ));
    }

    // -- the sharing constraint ---------------------------------------------

    #[test]
    fn an_objective_over_a_dimension_outside_s_is_refused() {
        let stray = Operative::new(
            "stray",
            Utility::new()
                .with(Objective::new("glory", "glory", Sense::Maximise))
                .unwrap(),
            &[Cynefin::Chaotic],
        )
        .unwrap();
        assert!(matches!(
            Omega::over(world()).with(stray),
            Err(OperativeError::DimensionNotInS { .. })
        ));
    }

    #[test]
    fn the_observed_and_unwatched_sets_are_real() {
        let o = Omega::over(world()).with(thrift()).unwrap();
        assert_eq!(o.observed_dimensions(), ["savings"].into_iter().collect());
        assert_eq!(
            o.unwatched_dimensions(),
            ["free_time", "noise"].into_iter().collect()
        );
        let both = o.with(balance()).unwrap();
        assert_eq!(both.unwatched_dimensions(), ["noise"].into_iter().collect());
    }

    #[test]
    fn suited_domains_are_the_cynefin_axis() {
        assert!(thrift().suited_to(Cynefin::Clear));
        assert!(!thrift().suited_to(Cynefin::Complex));
        assert!(balance().suited_to(Cynefin::Complex));
        assert_eq!(Cynefin::ALL.len(), 4);
    }

    // -- Nash ----------------------------------------------------------------

    #[test]
    fn nash_subtracts_the_disagreement_point() {
        match nash_product(&[0.9, 0.7], &[0.5, 0.5]).unwrap() {
            NashOutcome::Score(s) => assert!((s - 0.08).abs() < 1e-12, "got {s}"),
            other => panic!("expected a score, got {other:?}"),
        }
    }

    /// ★ The one place the reference's arithmetic and Nash's part company.
    #[test]
    fn a_party_that_gains_nothing_zeroes_nash_but_not_a_geometric_mean() {
        let u = [1.0, 0.5];
        let d = [0.5, 0.5];
        assert_eq!(
            nash_product(&u, &d).unwrap(),
            NashOutcome::NoBargain { refused_by: vec![1] }
        );
        // The reference computes exp(mean(ln u)) over the RAW utilities.
        let gm = geometric_mean(&u).unwrap();
        assert!(gm > 0.7, "the geometric mean reads this as a good option: {gm}");
    }

    #[test]
    fn the_degenerate_one_dimensional_case_is_what_the_reference_computes() {
        // Every u_i one-dimensional and normalised, d = 0.5 — §II's own words.
        let yes = [0.8, 0.6, 0.9];
        let gm = geometric_mean(&yes).unwrap();
        let expected = (0.8f64 * 0.6 * 0.9).powf(1.0 / 3.0);
        assert!((gm - expected).abs() < 1e-12);
    }
}
