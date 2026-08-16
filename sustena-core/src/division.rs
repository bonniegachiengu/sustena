//! Division of labour — why a node would spend itself
//! (Multiparty §VII · MUL-12).
//!
//! > Some roles cost the node that takes them more than they return: the
//! > coordinator that spends its cycles ordering everyone else's work, the
//! > replica that stores what it will never read, the member that funds a
//! > shared pot it draws on less than it fills. Why would an autonomous node
//! > accept one? **Kin selection** (Hamilton, 1964): the costly role is
//! > favoured when `rb > c`. **Evolutionarily stable strategies** (Maynard
//! > Smith, 1982): a role assignment is stable if no node can do better by
//! > unilaterally defecting — stability is a property of the assignment,
//! > **checkable**, not a hope about the members. **Costly signalling**:
//! > bearing a real cost is what makes a commitment credible.
//!
//! ```text
//! min_assignment Σ_i cost_i(role)   s.t.  coverage,  stability (rb > c),  redundancy ≥ ρ
//! ```
//!
//! ## ★ The third constraint is the one this module exists for
//!
//! > Two of those constraints are ordinary engineering — every role covered,
//! > enough redundancy to survive a loss. The third is the one that is usually
//! > left implicit and shouldn't be: an assignment that violates `rb > c` for
//! > some member **is not merely unfair, it is _unstable_, and it will be
//! > defected out of whether or not anyone announces it.**
//!
//! So stability is not a warning printed beside an assignment that was
//! produced anyway. [`Assignment`]'s fields are private and there is **no
//! constructor that takes a role→member map on trust** — the only way to hold
//! one is [`Division::assign`], which filters unstable pairings out of the
//! search space *before* searching. An unstable assignment is not a cheaper
//! option that got rejected; it is not an option.
//!
//! The diagnostic half is separate and deliberate: [`Division::diagnose`] will
//! tell you, with the numbers, exactly why a hand-proposed split fails — and
//! still yields no [`Assignment`]. **Refused structurally, flagged
//! diagnostically.**
//!
//! ## ★ Where `r` comes from — derived, not asserted
//!
//! §VII: *"in a household or a crew it is the share of the benefit that comes
//! back to the contributor through the composed Sustain."* That share is what
//! the composition roll-up already computes, so [`SharedInterest::from_rollup`]
//! takes a roll-up's per-member contributions and normalises them. `Σ rᵢ = 1`
//! by construction: **a member cannot get back more than the whole**, and
//! [`SharedInterest::declared`] refuses a set of shares that tries to.
//!
//! Two boundaries fall straight out of the arithmetic, and both are §VII's
//! actual content rather than decoration:
//!
//! - A **single-member body** has `r = 1`, so stability collapses to `b > c` —
//!   the role simply has to be worth doing.
//! - The **larger the body**, the smaller each share, so the same costly role
//!   needs a proportionally larger `b` to stay stable. Cooperation is harder
//!   to sustain the more diluted your stake is.
//!
//! ## `rb > c` and ESS — the same inequality, and which deviation it is about
//!
//! The article gives kin selection and ESS as two of three answers, then names
//! `rb > c` as *the* stability constraint. They coincide exactly, for the
//! deviation §VII names. A member holding role `R` has payoff `r·b − c`; a
//! member that unilaterally defects abandons the role, the benefit is not
//! produced, and its payoff is `0`. Staying beats defecting iff `r·b − c > 0`
//! iff `r·b > c`. So this is not an approximation of the ESS condition — it
//! **is** it, for that deviation.
//!
//! **The honest limit, because the interaction is real:** that derivation
//! assumes the defector is *pivotal* — that its leaving costs the group the
//! benefit. Under `ρ > 1` a single defector is by construction **not** pivotal:
//! the other holders still produce `b`, so a defector would keep receiving
//! `r·b` while paying nothing, and defecting would pay whenever `c > 0`. That
//! is the classic free-rider problem, and resolving it needs the group's
//! *response* to defection — replacement, exclusion, a repeated game — which
//! §VII does not specify and this build does not invent. `rb > c` is therefore
//! applied per-holder exactly as the article states it, with the reading
//! disclosed rather than the tension papered over.
//!
//! ## Costly signalling, made checkable rather than noted
//!
//! Bearing a real cost is what makes a commitment credible *because a node
//! that did not mean it would not pay*. The corollary is the useful half and
//! it is one boolean over numbers already present:
//! [`StabilityCheck::signals_commitment`] is false for a **free** role. A role
//! that costs nothing is stable for everyone, trivially — and accepting it is
//! therefore no evidence of anything.
//!
//! ## Honest limits
//!
//! - **Coverage is the `ρ = 1` case of redundancy.** They are the same
//!   inequality at different values, and they are reported separately because
//!   they mean different things — *nobody is doing it* is a different failure
//!   from *only one person is doing it*. Saying so is better than inventing a
//!   distinction the arithmetic does not have.
//! - **One role per member.** §VII's *"the coordinator/worker/replica split"*
//!   is a partition, and that is what is built. A member holding two roles at
//!   once is a different model, not a generalisation of this one.
//! - **The search is exhaustive and exact, and refuses rather than
//!   approximating.** A household-scale body is small; above
//!   [`SEARCH_LIMIT`] combinations [`Division::assign`] returns
//!   [`DivisionError::SearchTooLarge`] rather than returning a heuristic's
//!   answer labelled as the minimum.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::population::Population;

/// Above this many combinations [`Division::assign`] refuses rather than
/// approximating. A minimum you cannot verify is not a minimum.
pub const SEARCH_LIMIT: u64 = 1_000_000;

/// Tolerance for the "shares cannot exceed the whole" check, so ordinary
/// floating-point summation of a legitimate partition is not refused.
const SHARE_EPSILON: f64 = 1e-9;

// ---------------------------------------------------------------------------
// r — the coefficient of shared interest
// ---------------------------------------------------------------------------

/// `r` — the share of the benefit that comes back to the contributor through
/// the composed Sustain (§VII).
///
/// Derived from the composition roll-up rather than asserted. See
/// [`SharedInterest::from_rollup`].
#[derive(Debug, Clone, PartialEq)]
pub struct SharedInterest {
    r: BTreeMap<String, f64>,
}

impl SharedInterest {
    /// **Derive `r` from a roll-up's per-member contributions.**
    ///
    /// This is the composition point §VII names: the roll-up already folds
    /// each member's own value into the composed total, so each member's share
    /// of that total *is* the share that comes back. Normalising here means
    /// `Σ rᵢ = 1` by construction rather than by a caller's discipline.
    ///
    /// Refuses a negative stake (a share of a pot cannot be negative) and a
    /// zero total (there is no share of nothing — `0/0` is not `r = 0`, it is
    /// an unanswerable question, and answering it would silently make every
    /// costly role unstable).
    pub fn from_rollup(stakes: &[(&str, f64)]) -> Result<Self, DivisionError> {
        if stakes.is_empty() {
            return Err(DivisionError::NoMembers);
        }
        let mut total = 0.0;
        for (m, v) in stakes {
            if !v.is_finite() || *v < 0.0 {
                return Err(DivisionError::BadStake {
                    member: (*m).to_string(),
                    value: v.to_string(),
                });
            }
            total += *v;
        }
        if total <= 0.0 {
            return Err(DivisionError::EmptyRollup);
        }
        let mut r = BTreeMap::new();
        for (m, v) in stakes {
            r.insert((*m).to_string(), *v / total);
        }
        Ok(Self { r })
    }

    /// Declare `r` directly, for a body that knows its shares without a
    /// roll-up to derive them from.
    ///
    /// Refuses any `r` outside `[0, 1]` and refuses `Σ r > 1`: a share is a
    /// share *of* something, and you cannot distribute more than the whole.
    pub fn declared(shares: &[(&str, f64)]) -> Result<Self, DivisionError> {
        if shares.is_empty() {
            return Err(DivisionError::NoMembers);
        }
        let mut total = 0.0;
        let mut r = BTreeMap::new();
        for (m, v) in shares {
            if !v.is_finite() || *v < 0.0 || *v > 1.0 {
                return Err(DivisionError::BadShare {
                    member: (*m).to_string(),
                    value: v.to_string(),
                });
            }
            total += *v;
            r.insert((*m).to_string(), *v);
        }
        if total > 1.0 + SHARE_EPSILON {
            return Err(DivisionError::SharesExceedTheWhole(total));
        }
        Ok(Self { r })
    }

    pub fn of(&self, member: &str) -> Option<f64> {
        self.r.get(member).copied()
    }

    pub fn members(&self) -> Vec<&str> {
        self.r.keys().map(|s| s.as_str()).collect()
    }

    /// `Σ rᵢ`. Exactly 1 (to floating-point) for a set derived from a roll-up.
    pub fn total(&self) -> f64 {
        self.r.values().sum()
    }
}

// ---------------------------------------------------------------------------
// The role
// ---------------------------------------------------------------------------

/// A role the body needs filled, with the two numbers §VII attaches to it.
///
/// `c` is deliberately **not** here: §VII's objective is `Σᵢ costᵢ(role)`, so
/// cost is a property of the *pairing*, not of the role. The coordinator role
/// is not equally expensive for everyone, and that is the whole reason there
/// is something to minimise.
#[derive(Debug, Clone, PartialEq)]
pub struct Role {
    id: String,
    benefit_to_others: f64,
    redundancy: usize,
}

impl Role {
    /// Refuses `ρ = 0` — a role nobody is required to hold is not a role, and
    /// it would make coverage vacuous. Refuses a negative or non-finite `b`.
    ///
    /// `b = 0` **is** allowed, and is the interesting degenerate case: a
    /// costly role that benefits nobody satisfies `rb > c` for no member at
    /// any `r`, so no assignment containing it can be stable. See
    /// [`Division::assign`].
    pub fn new(id: &str, benefit_to_others: f64, redundancy: usize) -> Result<Self, DivisionError> {
        if id.is_empty() {
            return Err(DivisionError::EmptyRoleId);
        }
        if !benefit_to_others.is_finite() || benefit_to_others < 0.0 {
            return Err(DivisionError::BadBenefit {
                role: id.to_string(),
                value: benefit_to_others.to_string(),
            });
        }
        if redundancy == 0 {
            return Err(DivisionError::ZeroRedundancy(id.to_string()));
        }
        Ok(Self {
            id: id.to_string(),
            benefit_to_others,
            redundancy,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// `b` — the benefit to the **others**, not to the taker.
    pub fn benefit_to_others(&self) -> f64 {
        self.benefit_to_others
    }

    /// `ρ` — how many members must hold it to survive a loss.
    pub fn redundancy(&self) -> usize {
        self.redundancy
    }
}

// ---------------------------------------------------------------------------
// The stability check
// ---------------------------------------------------------------------------

/// One evaluation of `r·b > c` for one (member, role) pairing, with every
/// number it was computed from.
///
/// A verdict that cannot be re-derived from its own report is a verdict you
/// have to take on trust, which is the opposite of *checkable*.
#[derive(Debug, Clone, PartialEq)]
pub struct StabilityCheck {
    pub member: String,
    pub role: String,
    /// `r` — this member's share of the composed benefit.
    pub r: f64,
    /// `b` — this role's benefit to the others.
    pub b: f64,
    /// `c` — what this role costs *this* member.
    pub c: f64,
    pub holds: bool,
}

impl StabilityCheck {
    /// `rb`.
    pub fn rb(&self) -> f64 {
        self.r * self.b
    }

    /// `rb − c`. Positive iff stable; how positive is how much slack the
    /// pairing has before it becomes something to defect out of.
    pub fn margin(&self) -> f64 {
        self.rb() - self.c
    }

    /// **Costly signalling, as the one checkable thing it implies.**
    ///
    /// A commitment is credible because a node that did not mean it would not
    /// pay — so a role that costs nothing signals nothing. False for `c = 0`
    /// however comfortably the pairing is stable.
    pub fn signals_commitment(&self) -> bool {
        self.holds && self.c > 0.0
    }

    pub fn describe(&self) -> String {
        format!(
            "{} in '{}': r={:.4} · b={} · rb={:.4} {} c={} ({})",
            self.member,
            self.role,
            self.r,
            self.b,
            self.rb(),
            if self.holds { ">" } else { "≤" },
            self.c,
            if self.holds { "stable" } else { "UNSTABLE — will be defected out of" },
        )
    }
}

// ---------------------------------------------------------------------------
// The assignment
// ---------------------------------------------------------------------------

/// A **stable, covering, sufficiently redundant** cost-minimising split.
///
/// Fields are private and there is no constructor taking a map on trust: the
/// only way to obtain one is [`Division::assign`], which never returns an
/// assignment violating any of the three constraints. Holding one of these
/// *is* the proof it satisfies them.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    of_member: BTreeMap<String, String>,
    holders: BTreeMap<String, Vec<String>>,
    total_cost: f64,
    checks: Vec<StabilityCheck>,
}

impl Assignment {
    /// Which role a member holds.
    pub fn role_of(&self, member: &str) -> Option<&str> {
        self.of_member.get(member).map(|s| s.as_str())
    }

    /// Who holds a role.
    pub fn holders_of(&self, role: &str) -> &[String] {
        self.holders.get(role).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// `Σᵢ costᵢ(role)` — the objective.
    pub fn total_cost(&self) -> f64 {
        self.total_cost
    }

    /// Every pairing's `rb > c`, with the numbers. All of them hold.
    pub fn checks(&self) -> &[StabilityCheck] {
        &self.checks
    }

    /// The pairings that are genuinely costly *and* stable — the ones whose
    /// acceptance is credible evidence of commitment (§VII, costly signalling).
    pub fn credible_signals(&self) -> Vec<&StabilityCheck> {
        self.checks.iter().filter(|c| c.signals_commitment()).collect()
    }

    /// The slimmest margin in the whole assignment — the pairing that would
    /// go unstable first if costs drifted.
    pub fn tightest(&self) -> Option<&StabilityCheck> {
        self.checks
            .iter()
            .min_by(|a, b| a.margin().partial_cmp(&b.margin()).expect("finite"))
    }
}

/// Why no assignment satisfies all three constraints.
///
/// The first three are decided **exactly, before any search** — a member with
/// no stable role is named directly rather than discovered by exhausting a
/// space that never contained an answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Unfillable {
    /// ★ These members have no role for which `rb > c`. Whatever the body
    /// declares, they will defect out of every one of them.
    NoStableRole {
        members: Vec<String>,
        checks: Vec<StabilityCheck>,
    },
    /// A role cannot reach `ρ` because too few members are stable in it.
    CannotMeetRedundancy {
        role: String,
        stable_candidates: usize,
        needed: usize,
    },
    /// `Σ ρ` exceeds the membership: with one role per member the roles cannot
    /// all be staffed however they are arranged.
    Oversubscribed { required: usize, members: usize },
    /// Every exact pre-flight check passed and the search still found no
    /// combination meeting coverage and redundancy simultaneously.
    NoCombination,
}

impl Unfillable {
    pub fn describe(&self) -> String {
        match self {
            Unfillable::NoStableRole { members, .. } => format!(
                "no stable role exists for {} — rb > c fails for every role on offer",
                members.join(", ")
            ),
            Unfillable::CannotMeetRedundancy {
                role,
                stable_candidates,
                needed,
            } => format!(
                "'{role}' needs {needed} holders and only {stable_candidates} member(s) are stable in it"
            ),
            Unfillable::Oversubscribed { required, members } => format!(
                "the declared roles need {required} holders and the body has {members} member(s)"
            ),
            Unfillable::NoCombination => {
                "no combination of stable pairings covers every role at its declared redundancy"
                    .to_string()
            }
        }
    }
}

/// The result of the optimisation.
///
/// [`AssignmentOutcome::NoStableAssignment`] is a **normal outcome**, not an
/// error — it is the body reporting that the roles it declared cannot be
/// stably filled by the members it has, which is information, not a fault.
/// (The same shape [`crate::consensus::RoundOutcome::Failed`] takes for a
/// round that did not decide.)
#[derive(Debug, Clone, PartialEq)]
pub enum AssignmentOutcome {
    Assigned(Box<Assignment>),
    NoStableAssignment(Unfillable),
}

impl AssignmentOutcome {
    pub fn assigned(&self) -> Option<&Assignment> {
        match self {
            AssignmentOutcome::Assigned(a) => Some(a),
            AssignmentOutcome::NoStableAssignment(_) => None,
        }
    }

    pub fn is_assigned(&self) -> bool {
        matches!(self, AssignmentOutcome::Assigned(_))
    }
}

/// What is wrong with a **hand-proposed** split, in full, with the numbers —
/// and without yielding an [`Assignment`] for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnosis {
    pub total_cost: f64,
    pub checks: Vec<StabilityCheck>,
    /// The pairings that fail `rb > c`. **Non-empty means the split will be
    /// defected out of**, announced or not.
    pub unstable: Vec<StabilityCheck>,
    /// Roles nobody holds.
    pub uncovered: Vec<String>,
    /// `(role, holders, ρ)` for roles held by too few.
    pub under_redundant: Vec<(String, usize, usize)>,
}

impl Diagnosis {
    pub fn is_stable(&self) -> bool {
        self.unstable.is_empty()
    }

    pub fn covers(&self) -> bool {
        self.uncovered.is_empty()
    }

    pub fn is_redundant(&self) -> bool {
        self.under_redundant.is_empty()
    }

    /// All three constraints, which is exactly what [`Division::assign`]
    /// guarantees and this proposal may not.
    pub fn admissible(&self) -> bool {
        self.is_stable() && self.covers() && self.is_redundant()
    }

    pub fn describe(&self) -> String {
        if self.admissible() {
            return format!("admissible · Σcost = {}", self.total_cost);
        }
        let mut parts = Vec::new();
        if !self.unstable.is_empty() {
            parts.push(format!(
                "UNSTABLE for {} — {}",
                self.unstable
                    .iter()
                    .map(|c| c.member.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                "an assignment violating rb > c will be defected out of, announced or not"
            ));
        }
        if !self.uncovered.is_empty() {
            parts.push(format!("uncovered: {}", self.uncovered.join(", ")));
        }
        for (role, have, need) in &self.under_redundant {
            parts.push(format!("'{role}' has {have} holder(s), needs {need}"));
        }
        parts.join(" · ")
    }
}

// ---------------------------------------------------------------------------
// The body's division of labour
// ---------------------------------------------------------------------------

/// The declared roles, per-pairing costs, and `r` — everything §VII's
/// optimisation takes as input.
#[derive(Debug, Clone)]
pub struct Division {
    roles: BTreeMap<String, Role>,
    members: Vec<String>,
    /// `member → role → c`. **Never defaulted.** A missing cost is refused at
    /// [`Division::assign`], because silently reading it as free would make an
    /// unaffordable pairing look stable.
    costs: BTreeMap<String, BTreeMap<String, f64>>,
    interest: SharedInterest,
}

impl Division {
    /// Members are exactly those with a declared `r`: `rb > c` is not
    /// computable without one, and defaulting a missing share to `0` (never
    /// stable) or `1` (always stable) would each be a fabricated answer.
    pub fn new(interest: SharedInterest) -> Self {
        let members = interest.members().into_iter().map(|s| s.to_string()).collect();
        Self {
            roles: BTreeMap::new(),
            members,
            costs: BTreeMap::new(),
            interest,
        }
    }

    /// Take the node set from a [`Population`] — the same body the coordination
    /// layer already declared, read here as *who can hold a role*.
    ///
    /// Refuses a mismatch in either direction: a node with no declared share
    /// cannot be assigned, and a share for someone who is not a node is a
    /// share in a body they are not in.
    pub fn over(
        population: &Population,
        interest: SharedInterest,
    ) -> Result<Self, DivisionError> {
        let nodes: BTreeSet<&str> = population.nodes().iter().map(|s| s.as_str()).collect();
        let shares: BTreeSet<&str> = interest.members().into_iter().collect();
        let missing: Vec<String> = nodes.difference(&shares).map(|s| (*s).to_string()).collect();
        if !missing.is_empty() {
            return Err(DivisionError::NodesWithoutShare(missing));
        }
        let strangers: Vec<String> = shares.difference(&nodes).map(|s| (*s).to_string()).collect();
        if !strangers.is_empty() {
            return Err(DivisionError::SharesWithoutNode(strangers));
        }
        Ok(Division::new(interest))
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.roles.insert(role.id.clone(), role);
        self
    }

    /// Declare `costᵢ(role)` for one pairing.
    pub fn costing(mut self, member: &str, role: &str, c: f64) -> Self {
        self.set_cost(member, role, c).expect("declared member and role, finite cost");
        self
    }

    pub fn set_cost(&mut self, member: &str, role: &str, c: f64) -> Result<(), DivisionError> {
        if !self.members.iter().any(|m| m == member) {
            return Err(DivisionError::UnknownMember(member.to_string()));
        }
        if !self.roles.contains_key(role) {
            return Err(DivisionError::UnknownRole(role.to_string()));
        }
        if !c.is_finite() || c < 0.0 {
            return Err(DivisionError::BadCost {
                member: member.to_string(),
                role: role.to_string(),
                value: c.to_string(),
            });
        }
        self.costs
            .entry(member.to_string())
            .or_default()
            .insert(role.to_string(), c);
        Ok(())
    }

    pub fn members(&self) -> &[String] {
        &self.members
    }

    pub fn roles(&self) -> Vec<&Role> {
        self.roles.values().collect()
    }

    pub fn interest(&self) -> &SharedInterest {
        &self.interest
    }

    pub fn cost_of(&self, member: &str, role: &str) -> Option<f64> {
        self.costs.get(member)?.get(role).copied()
    }

    /// `rᵢ · b(R) > cᵢ(R)`, with every number it used.
    pub fn check(&self, member: &str, role: &str) -> Result<StabilityCheck, DivisionError> {
        let r = self
            .interest
            .of(member)
            .ok_or_else(|| DivisionError::UnknownMember(member.to_string()))?;
        let role_def = self
            .roles
            .get(role)
            .ok_or_else(|| DivisionError::UnknownRole(role.to_string()))?;
        let c = self.cost_of(member, role).ok_or_else(|| DivisionError::MissingCost {
            member: member.to_string(),
            role: role.to_string(),
        })?;
        let b = role_def.benefit_to_others;
        Ok(StabilityCheck {
            member: member.to_string(),
            role: role.to_string(),
            r,
            b,
            c,
            holds: r * b > c,
        })
    }

    /// The roles this member would not defect out of.
    pub fn eligible_roles(&self, member: &str) -> Result<Vec<String>, DivisionError> {
        let mut out = Vec::new();
        for role in self.roles.keys() {
            if self.check(member, role)?.holds {
                out.push(role.clone());
            }
        }
        Ok(out)
    }

    /// **The optimisation** —
    /// `min Σᵢ costᵢ(role) s.t. coverage, stability (rb > c), redundancy ≥ ρ`.
    ///
    /// Stability is applied as a filter on the candidate set *before* the
    /// search, so an unstable pairing is never a cheaper option that lost — it
    /// is not in the space at all. Ties are broken by declaration order, so
    /// the result is deterministic.
    pub fn assign(&self) -> Result<AssignmentOutcome, DivisionError> {
        self.search(true)
    }

    /// **The price of stability, made visible.**
    ///
    /// `Σ cost` for the minimiser under *coverage and redundancy only* — what
    /// the body would have paid if `rb > c` were not a constraint. The gap
    /// against [`Assignment::total_cost`] is what stability costs, and it
    /// belongs where it can be argued with rather than inferred.
    ///
    /// `None` when even the unconstrained problem has no solution.
    pub fn unconstrained_minimum(&self) -> Result<Option<f64>, DivisionError> {
        Ok(self.search(false)?.assigned().map(|a| a.total_cost))
    }

    /// What stability actually cost this body: `assigned − unconstrained`.
    ///
    /// Always `≥ 0` — constraining a minimisation cannot lower its minimum.
    pub fn price_of_stability(&self) -> Result<Option<f64>, DivisionError> {
        let stable = match self.assign()? {
            AssignmentOutcome::Assigned(a) => a.total_cost,
            AssignmentOutcome::NoStableAssignment(_) => return Ok(None),
        };
        Ok(self.unconstrained_minimum()?.map(|u| stable - u))
    }

    /// Diagnose a hand-proposed split — **without yielding an
    /// [`Assignment`] for it**, whatever it says.
    pub fn diagnose(&self, proposed: &[(&str, &str)]) -> Result<Diagnosis, DivisionError> {
        let mut checks = Vec::new();
        let mut total = 0.0;
        let mut holders: BTreeMap<String, usize> = BTreeMap::new();
        for (member, role) in proposed {
            let chk = self.check(member, role)?;
            total += chk.c;
            *holders.entry((*role).to_string()).or_insert(0) += 1;
            checks.push(chk);
        }
        let unstable: Vec<StabilityCheck> = checks.iter().filter(|c| !c.holds).cloned().collect();
        let mut uncovered = Vec::new();
        let mut under = Vec::new();
        for (id, role) in &self.roles {
            let have = holders.get(id).copied().unwrap_or(0);
            if have == 0 {
                uncovered.push(id.clone());
            }
            if have < role.redundancy {
                under.push((id.clone(), have, role.redundancy));
            }
        }
        Ok(Diagnosis {
            total_cost: total,
            checks,
            unstable,
            uncovered,
            under_redundant: under,
        })
    }

    // -- the search ---------------------------------------------------------

    fn search(&self, respect_stability: bool) -> Result<AssignmentOutcome, DivisionError> {
        if self.members.is_empty() {
            return Err(DivisionError::NoMembers);
        }
        if self.roles.is_empty() {
            return Err(DivisionError::NoRoles);
        }

        // Exact pre-flight #1: Σρ against the membership. With one role per
        // member this is decidable by counting, before anything is arranged.
        let required: usize = self.roles.values().map(|r| r.redundancy).sum();
        if required > self.members.len() {
            return Ok(AssignmentOutcome::NoStableAssignment(
                Unfillable::Oversubscribed {
                    required,
                    members: self.members.len(),
                },
            ));
        }

        // Candidate sets. Every cost must be declared — a missing one is
        // refused rather than read as free.
        let mut candidates: Vec<Vec<String>> = Vec::with_capacity(self.members.len());
        let mut no_role_members = Vec::new();
        let mut no_role_checks = Vec::new();
        for member in &self.members {
            let mut eligible = Vec::new();
            let mut member_checks = Vec::new();
            for role in self.roles.keys() {
                let chk = self.check(member, role)?;
                if !respect_stability || chk.holds {
                    eligible.push(role.clone());
                }
                member_checks.push(chk);
            }
            if eligible.is_empty() {
                no_role_members.push(member.clone());
                no_role_checks.extend(member_checks);
            }
            candidates.push(eligible);
        }

        // Exact pre-flight #2: a member with nowhere stable to stand is named
        // directly, not discovered by exhausting a space that never held an
        // answer.
        if !no_role_members.is_empty() {
            return Ok(AssignmentOutcome::NoStableAssignment(
                Unfillable::NoStableRole {
                    members: no_role_members,
                    checks: no_role_checks,
                },
            ));
        }

        // Exact pre-flight #3: a role that too few members are stable in
        // cannot reach ρ however the rest is arranged.
        for (id, role) in &self.roles {
            let possible = candidates.iter().filter(|c| c.iter().any(|r| r == id)).count();
            if possible < role.redundancy {
                return Ok(AssignmentOutcome::NoStableAssignment(
                    Unfillable::CannotMeetRedundancy {
                        role: id.clone(),
                        stable_candidates: possible,
                        needed: role.redundancy,
                    },
                ));
            }
        }

        // No silent truncation: an unverifiable minimum is refused.
        let mut space: u64 = 1;
        for c in &candidates {
            space = space.saturating_mul(c.len() as u64);
            if space > SEARCH_LIMIT {
                return Err(DivisionError::SearchTooLarge {
                    members: self.members.len(),
                    roles: self.roles.len(),
                    limit: SEARCH_LIMIT,
                });
            }
        }

        let mut best: Option<(f64, Vec<usize>)> = None;
        let mut odometer = vec![0usize; self.members.len()];
        loop {
            let mut total = 0.0;
            let mut holders: BTreeMap<&str, usize> = BTreeMap::new();
            for (i, member) in self.members.iter().enumerate() {
                let role = &candidates[i][odometer[i]];
                total += self.cost_of(member, role).expect("checked above");
                *holders.entry(role.as_str()).or_insert(0) += 1;
            }
            let ok = self.roles.values().all(|role| {
                holders.get(role.id.as_str()).copied().unwrap_or(0) >= role.redundancy
            });
            // Strictly-less keeps the first minimum found, and enumeration
            // order is fixed, so the answer is deterministic.
            if ok && best.as_ref().is_none_or(|(b, _)| total < *b) {
                best = Some((total, odometer.clone()));
            }

            let mut i = 0;
            loop {
                if i == odometer.len() {
                    // Overflowed the most significant digit: space exhausted.
                    return Ok(match best {
                        Some((total, picks)) => {
                            AssignmentOutcome::Assigned(Box::new(self.build(&candidates, &picks, total)?))
                        }
                        None => AssignmentOutcome::NoStableAssignment(Unfillable::NoCombination),
                    });
                }
                odometer[i] += 1;
                if odometer[i] < candidates[i].len() {
                    break;
                }
                odometer[i] = 0;
                i += 1;
            }
        }
    }

    fn build(
        &self,
        candidates: &[Vec<String>],
        picks: &[usize],
        total: f64,
    ) -> Result<Assignment, DivisionError> {
        let mut of_member = BTreeMap::new();
        let mut holders: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut checks = Vec::new();
        for (i, member) in self.members.iter().enumerate() {
            let role = candidates[i][picks[i]].clone();
            checks.push(self.check(member, &role)?);
            holders.entry(role.clone()).or_default().push(member.clone());
            of_member.insert(member.clone(), role);
        }
        Ok(Assignment {
            of_member,
            holders,
            total_cost: total,
            checks,
        })
    }
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Error)]
pub enum DivisionError {
    #[error("a body with no members has no division of labour")]
    NoMembers,
    #[error("no roles declared — there is nothing to divide")]
    NoRoles,
    #[error("a role id cannot be empty")]
    EmptyRoleId,
    #[error("stake for '{member}' is not a usable share of a pot: {value}")]
    BadStake { member: String, value: String },
    #[error("share for '{member}' is outside [0, 1]: {value}")]
    BadShare { member: String, value: String },
    #[error(
        "shares sum to {0}: a share is a share OF something, and you cannot \
         distribute more than the whole"
    )]
    SharesExceedTheWhole(f64),
    #[error(
        "the roll-up totals zero — there is no share of nothing, and reading \
         0/0 as r = 0 would silently make every costly role unstable"
    )]
    EmptyRollup,
    #[error("benefit-to-others for role '{role}' is not a usable number: {value}")]
    BadBenefit { role: String, value: String },
    #[error("role '{0}' declares ρ = 0 — a role nobody must hold is not a role")]
    ZeroRedundancy(String),
    #[error("cost of '{role}' for '{member}' is not a usable number: {value}")]
    BadCost {
        member: String,
        role: String,
        value: String,
    },
    #[error(
        "no cost declared for '{member}' in '{role}' — a missing cost is \
         refused, never read as free, because free is always stable"
    )]
    MissingCost { member: String, role: String },
    #[error("unknown member '{0}'")]
    UnknownMember(String),
    #[error("unknown role '{0}'")]
    UnknownRole(String),
    #[error("population nodes with no declared share of the composed benefit: {0:?}")]
    NodesWithoutShare(Vec<String>),
    #[error("shares declared for non-members of the population: {0:?}")]
    SharesWithoutNode(Vec<String>),
    #[error(
        "{members} members × {roles} roles exceeds the {limit}-combination \
         search limit — refused rather than approximated, because a minimum \
         you cannot verify is not a minimum"
    )]
    SearchTooLarge {
        members: usize,
        roles: usize,
        limit: u64,
    },
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::population::PopulationSpec;

    fn interest() -> SharedInterest {
        // A four-member body whose composed benefit comes back unequally.
        SharedInterest::from_rollup(&[("ama", 40.0), ("ben", 30.0), ("cira", 20.0), ("dee", 10.0)])
            .expect("real stakes")
    }

    fn body() -> Division {
        Division::new(interest())
            .with_role(Role::new("coordinator", 100.0, 1).expect("role"))
            .with_role(Role::new("worker", 60.0, 2).expect("role"))
            .with_role(Role::new("replica", 40.0, 1).expect("role"))
    }

    /// Everyone can afford everything — the baseline the constrained cases are
    /// measured against.
    fn cheap(d: Division) -> Division {
        let mut d = d;
        for m in ["ama", "ben", "cira", "dee"] {
            for (r, c) in [("coordinator", 3.0), ("worker", 2.0), ("replica", 1.0)] {
                d.set_cost(m, r, c).expect("declared");
            }
        }
        d
    }

    // -- r ------------------------------------------------------------------

    #[test]
    fn r_derived_from_a_rollup_sums_to_one() {
        let r = interest();
        assert!((r.total() - 1.0).abs() < 1e-12);
        assert!((r.of("ama").expect("member") - 0.4).abs() < 1e-12);
        assert!((r.of("dee").expect("member") - 0.1).abs() < 1e-12);
    }

    #[test]
    fn a_single_member_body_has_r_of_one() {
        let r = SharedInterest::from_rollup(&[("solo", 500.0)]).expect("stake");
        assert_eq!(r.of("solo"), Some(1.0));
    }

    #[test]
    fn shares_cannot_exceed_the_whole() {
        let err = SharedInterest::declared(&[("a", 0.7), ("b", 0.7)]).expect_err("refused");
        assert!(matches!(err, DivisionError::SharesExceedTheWhole(_)));
    }

    #[test]
    fn a_zero_rollup_is_refused_not_read_as_zero_share() {
        let err = SharedInterest::from_rollup(&[("a", 0.0), ("b", 0.0)]).expect_err("refused");
        assert!(matches!(err, DivisionError::EmptyRollup));
    }

    #[test]
    fn a_negative_stake_is_refused() {
        assert!(SharedInterest::from_rollup(&[("a", -1.0)]).is_err());
        assert!(SharedInterest::declared(&[("a", 1.5)]).is_err());
    }

    // -- the role -----------------------------------------------------------

    #[test]
    fn a_role_nobody_must_hold_is_not_a_role() {
        let err = Role::new("optional", 10.0, 0).expect_err("refused");
        assert!(matches!(err, DivisionError::ZeroRedundancy(_)));
    }

    // -- rb > c -------------------------------------------------------------

    #[test]
    fn the_check_reports_every_number_it_used() {
        let d = cheap(body());
        let chk = d.check("ama", "coordinator").expect("declared");
        assert_eq!((chk.r, chk.b, chk.c), (0.4, 100.0, 3.0));
        assert!((chk.rb() - 40.0).abs() < 1e-12);
        assert!(chk.holds);
        assert!((chk.margin() - 37.0).abs() < 1e-12);
    }

    #[test]
    fn dilution_is_the_whole_point_the_same_role_destabilises_as_the_share_shrinks() {
        // r·b > c with b = 100 and c = 15: stable at r = 0.4, not at r = 0.1.
        let mut d = cheap(body());
        d.set_cost("ama", "coordinator", 15.0).expect("declared");
        d.set_cost("dee", "coordinator", 15.0).expect("declared");
        assert!(d.check("ama", "coordinator").expect("ok").holds);
        assert!(!d.check("dee", "coordinator").expect("ok").holds);
    }

    #[test]
    fn a_role_that_benefits_nobody_is_stable_for_no_member_at_any_share() {
        let mut d = Division::new(interest())
            .with_role(Role::new("busywork", 0.0, 1).expect("role"));
        for m in ["ama", "ben", "cira", "dee"] {
            d.set_cost(m, "busywork", 0.01).expect("declared");
        }
        for m in ["ama", "ben", "cira", "dee"] {
            assert!(!d.check(m, "busywork").expect("ok").holds, "{m}");
        }
    }

    #[test]
    fn a_free_role_is_stable_for_everyone_and_signals_nothing() {
        let mut d = Division::new(interest()).with_role(Role::new("token", 1.0, 1).expect("role"));
        for m in ["ama", "ben", "cira", "dee"] {
            d.set_cost(m, "token", 0.0).expect("declared");
            let chk = d.check(m, "token").expect("ok");
            assert!(chk.holds);
            assert!(!chk.signals_commitment(), "a free role is not a credible signal");
        }
    }

    #[test]
    fn a_missing_cost_is_refused_never_read_as_free() {
        let d = body(); // no costs declared at all
        let err = d.check("ama", "coordinator").expect_err("refused");
        assert!(matches!(err, DivisionError::MissingCost { .. }));
    }

    // -- the optimisation ---------------------------------------------------

    #[test]
    fn a_stable_split_is_admitted_covered_and_redundant() {
        let d = cheap(body());
        let out = d.assign().expect("searched");
        let a = out.assigned().expect("assigned");
        assert_eq!(a.holders_of("coordinator").len(), 1);
        assert!(a.holders_of("worker").len() >= 2);
        assert_eq!(a.holders_of("replica").len(), 1);
        assert!(a.checks().iter().all(|c| c.holds));
    }

    /// ★★ THE PROOF OF VALUE. The cheapest split is the unstable one, so the
    /// optimiser must refuse the global minimum and pay more.
    #[test]
    fn stability_is_binding_the_cheapest_split_is_refused_and_the_price_is_reported() {
        let mut d = cheap(body());
        // dee has the smallest share (r = 0.1), so rb = 10 for the
        // coordinator role — and coordinating is cheap for dee and dear for
        // everyone else. Pure cost minimisation puts dee in the chair.
        d.set_cost("dee", "coordinator", 12.0).expect("declared");
        d.set_cost("ama", "coordinator", 30.0).expect("declared");
        d.set_cost("ben", "coordinator", 30.0).expect("declared");
        d.set_cost("cira", "coordinator", 30.0).expect("declared");

        // The unconstrained minimiser really does pick dee.
        let loose = d.search(false).expect("searched");
        let loose = loose.assigned().expect("assigned");
        assert_eq!(loose.role_of("dee"), Some("coordinator"));

        // dee would defect out of it: r·b = 0.1 × 100 = 10, c = 12.
        let chk = d.check("dee", "coordinator").expect("ok");
        assert!(!chk.holds);

        // So the stable minimiser does not, and it costs more.
        let out = d.assign().expect("searched");
        let tight = out.assigned().expect("assigned");
        assert_ne!(tight.role_of("dee"), Some("coordinator"));
        assert!(tight.checks().iter().all(|c| c.holds));

        let price = d.price_of_stability().expect("computed").expect("both solved");
        assert!(price > 0.0, "stability is bought, and the price is {price}");
        assert!(tight.total_cost() > loose.total_cost());
    }

    #[test]
    fn a_member_with_no_stable_role_is_named_before_any_search() {
        let mut d = cheap(body());
        for r in ["coordinator", "worker", "replica"] {
            d.set_cost("dee", r, 999.0).expect("declared");
        }
        let out = d.assign().expect("searched");
        match out {
            AssignmentOutcome::NoStableAssignment(Unfillable::NoStableRole { members, checks }) => {
                assert_eq!(members, vec!["dee".to_string()]);
                assert!(checks.iter().all(|c| !c.holds));
            }
            other => panic!("expected NoStableRole, got {other:?}"),
        }
    }

    #[test]
    fn a_role_too_few_are_stable_in_cannot_reach_its_redundancy() {
        let mut d = Division::new(interest())
            .with_role(Role::new("guard", 100.0, 3).expect("role"))
            .with_role(Role::new("rest", 10.0, 1).expect("role"));
        for m in ["ama", "ben", "cira", "dee"] {
            d.set_cost(m, "rest", 0.5).expect("declared");
            // Only ama can afford guarding: rb is 40 · 30 · 20 · 10 against
            // c = 35, so ama clears it and nobody else comes close.
            d.set_cost(m, "guard", 35.0).expect("declared");
        }
        let out = d.assign().expect("searched");
        match out {
            AssignmentOutcome::NoStableAssignment(Unfillable::CannotMeetRedundancy {
                role,
                stable_candidates,
                needed,
            }) => {
                assert_eq!(role, "guard");
                assert_eq!((stable_candidates, needed), (1, 3));
            }
            other => panic!("expected CannotMeetRedundancy, got {other:?}"),
        }
    }

    #[test]
    fn roles_needing_more_holders_than_the_body_has_are_refused_by_counting() {
        let mut d = Division::new(
            SharedInterest::from_rollup(&[("ama", 1.0), ("ben", 1.0)]).expect("stakes"),
        )
        .with_role(Role::new("a", 10.0, 2).expect("role"))
        .with_role(Role::new("b", 10.0, 2).expect("role"));
        for m in ["ama", "ben"] {
            for r in ["a", "b"] {
                d.set_cost(m, r, 0.1).expect("declared");
            }
        }
        match d.assign().expect("searched") {
            AssignmentOutcome::NoStableAssignment(Unfillable::Oversubscribed {
                required,
                members,
            }) => assert_eq!((required, members), (4, 2)),
            other => panic!("expected Oversubscribed, got {other:?}"),
        }
    }

    // -- diagnose -----------------------------------------------------------

    #[test]
    fn a_hand_proposed_unstable_split_is_flagged_with_the_numbers_and_yields_no_assignment() {
        let mut d = cheap(body());
        d.set_cost("dee", "coordinator", 12.0).expect("declared");
        let diag = d
            .diagnose(&[
                ("dee", "coordinator"),
                ("ama", "worker"),
                ("ben", "worker"),
                ("cira", "replica"),
            ])
            .expect("diagnosed");
        assert!(diag.covers());
        assert!(diag.is_redundant());
        assert!(!diag.is_stable());
        assert!(!diag.admissible());
        assert_eq!(diag.unstable.len(), 1);
        assert_eq!(diag.unstable[0].member, "dee");
        assert!(diag.describe().contains("defected out of"));
    }

    #[test]
    fn diagnose_names_uncovered_and_under_redundant_roles_separately() {
        let d = cheap(body());
        let diag = d
            .diagnose(&[("ama", "coordinator"), ("ben", "worker")])
            .expect("diagnosed");
        assert_eq!(diag.uncovered, vec!["replica".to_string()]);
        // worker has 1 holder and needs 2; replica has 0 and needs 1.
        assert!(diag
            .under_redundant
            .iter()
            .any(|(r, have, need)| r == "worker" && *have == 1 && *need == 2));
        assert!(diag
            .under_redundant
            .iter()
            .any(|(r, have, need)| r == "replica" && *have == 0 && *need == 1));
    }

    // -- composition --------------------------------------------------------

    #[test]
    fn the_node_set_comes_from_the_population_and_a_mismatch_is_refused() {
        let pop = Population::new(PopulationSpec::new(0.5, 0.1).expect("spec"))
            .with_node("ama")
            .with_node("ben");
        let ok = Division::over(
            &pop,
            SharedInterest::from_rollup(&[("ama", 1.0), ("ben", 1.0)]).expect("stakes"),
        );
        assert!(ok.is_ok());

        let missing = Division::over(
            &pop,
            SharedInterest::from_rollup(&[("ama", 1.0)]).expect("stakes"),
        );
        assert!(matches!(missing, Err(DivisionError::NodesWithoutShare(_))));

        let stranger = Division::over(
            &pop,
            SharedInterest::from_rollup(&[("ama", 1.0), ("ben", 1.0), ("zed", 1.0)])
                .expect("stakes"),
        );
        assert!(matches!(stranger, Err(DivisionError::SharesWithoutNode(_))));
    }

    #[test]
    fn credible_signals_are_the_costly_stable_pairings_only() {
        let d = cheap(body());
        let out = d.assign().expect("searched");
        let a = out.assigned().expect("assigned");
        // Every cost in `cheap` is > 0, so every stable pairing signals.
        assert_eq!(a.credible_signals().len(), a.checks().len());
        assert!(a.tightest().is_some());
    }

    #[test]
    fn the_search_is_deterministic() {
        let d = cheap(body());
        let a = d.assign().expect("searched");
        let b = d.assign().expect("searched");
        assert_eq!(a, b);
    }
}
