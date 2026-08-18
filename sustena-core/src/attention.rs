//! `attention = ⟨narrow, broad⟩` — the two attentions (OPV-7; the Operative
//! paper, §V).
//!
//! ## Both, for every operative — structurally
//!
//! > A council of one-attention agents is a council of half-minds.
//!
//! [`Attention`] has **two required fields and no `Option`**, so an operative
//! with only one attention cannot be written down. This is not a partition of
//! the council into narrow specialists and broad specialists; both belong to
//! one mind. ★ And they must land in **different regions** of the aperture
//! space — a pair that is narrow twice is refused, because declaring the same
//! aperture under two names is the partition arriving through the back door.
//!
//! ## ★ Different OUTPUT TYPES, which is the load-bearing distinction
//!
//! Narrow returns a [`Score`] on a candidate. Broad returns a [`Reframing`] —
//! not *this option is worth 0.7* but **the frame you were scoring in has
//! changed**. They are different types, not two numbers, so they cannot be
//! compared, summed or traded against each other.
//!
//! Ashby's reason: narrow deliberately reduces variety, and something must hold
//! the remainder. An agent with only narrow is a maximiser inside a frame it
//! cannot notice is wrong — which is the Goodhart-drift mechanism §XIV names,
//! arriving here through the attention layer rather than the utility one.
//!
//! ## ★★★ Narrow does NOT get a second notion of *important*
//!
//! This is the constraint that replaces the missing parity half. The salience
//! family closed rival-importance on four surfaces — brightness (MON-7), score
//! (UI-1/UI-7), position (UI-13), exemption (CTL-8) — and a scoring attention
//! is exactly where it could reopen.
//!
//! So [`Score`] has **no public constructor**. The only way to obtain one is
//! [`narrow_scan`], which computes it from [`Region::distance`] — the
//! household's own `d(s,V)`. An operative cannot mint a score, cannot raise its
//! own, and has no field anywhere in which to declare a rival metric. Same
//! mechanism as `VisualSpec`, applied to the fifth surface.
//!
//! ## ★★ The three-axis meter over the ⊕ tree
//!
//! `⟨b, d, ρ⟩` — **breadth** (sibling sustains entering consideration at a
//! level), **depth** (how many `⊕` levels *down* the composition the reasoning
//! runs), **resolution** (how finely each visited sustain is sampled). Cost is
//! `pawa(a) = κ·b·d·ρ`, bounded by `B_att`.
//!
//! ★★★ **And `B_att` is a BALANCE, not a number.** OPV-7 shipped the cost
//! function and a declaration-time bound but nothing that was ever *spent*, so
//! a second scan cost what the first did — a declared cost with nothing behind
//! it. [`AttentionBudget`] closes that: `spent` is the **fold** of an
//! append-only spend list, `spent + remaining == allocated` holds **by the
//! shape of the type**, an unaffordable attention is **refused** (never
//! clamped — see the type's docs for why clamping is *unsound*, not merely
//! unspecified), and a refusal appends nothing.
//!
//! ★★ `κ_att` is **governed** (PAWA-11): the module constant was demoted to
//! [`Parameters::GENESIS_ATTENTION_KAPPA`] and `Aperture::cost()` deleted, so
//! **nothing prices an aperture from a constant**.
//!
//! ★ Depth is the genuinely new axis and it is a property of **position in the
//! composition tree**, so it walks the real one: [`MonitorEngine`]'s parent
//! chain, the same structure `holarchy::escalate` climbs. Escalation goes up;
//! attention goes down. No parallel tree was introduced.
//!
//! ## ★★★ A path with fringes, not a subtree — and the subtree is unspellable
//!
//! Exhaustive fan-out visits `Σ b^ℓ`, geometric in `d`, which no budget
//! survives. Attention is affordable because it is a **path**: one chosen child
//! per level, with the siblings at each level **sampled as fringes and not
//! descended into**. That visits `~b·d`.
//!
//! ★ The discipline is structural rather than advised: there is **no
//! `walk_subtree`**, and [`scan`] descends through exactly one child per level.
//! A caller cannot request the geometric traversal, so the product form in the
//! cost function is a description of what the walker does rather than a budget
//! it might exceed.
//!
//! ## The closed trade
//!
//! Under a fixed `B_att` the product is fixed, so **raising any axis is paid
//! for by lowering another** — deep-and-narrow and broad-and-shallow are the
//! same spend. Narrow ≈ ⟨small `b`, the task's `d`, high `ρ`⟩ and broad ≈
//! ⟨large `b`, shallow `d`, low `ρ`⟩ are therefore **regions** of the space,
//! not labels, which is what lets *both non-empty* compile to *the traversal
//! must visit both regions*.
//!
//! ## Attention is the footprint; `K` is the hold
//!
//! Pawa bounds the **scan** — what may be looked at this period. UI-1's `K ≈ 4`
//! bounds the **hold** — what is kept in play at once. What attention scans is
//! what may enter the knapsack, so the two compose rather than compete:
//! **anything can scan widely and hold four; nothing can hold widely.**
//!
//! ## Attention is not `Π`
//!
//! Attention decides **what enters consideration**; [`crate::strategy`] reasons
//! **over what entered**. A filter that also reasoned would be choosing its own
//! answer.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::governance::Parameters;
use crate::monitor::MonitorEngine;
use crate::region::Region;

// ★★★ **`κ` USED TO LIVE HERE AS A CONSTANT, AND B_ATT MOVED IT.**
//
// It is now [`Parameters::GENESIS_ATTENTION_KAPPA`] — *the value κ_att is
// declared with at genesis*, not *the value it has* — and `Aperture::cost()`
// was **removed outright** in favour of [`Aperture::cost_under`], which takes
// the governed [`Parameters`]. The same demotion-and-deletion PAWA-11 applied
// to `κ_c`/`κ_s`, for the same reason: a governed parameter cannot also be a
// hardcoded const that silently overrides it.
//
// ★★ So **there is no path in this module that prices an aperture from a
// constant**. The genesis value is `1.0`, exactly the constant it replaced, so
// every already-declared aperture costs what it always did.

/// Which region of the aperture space an aperture lands in.
///
/// ★ Named `Stance`, not `Region` — [`crate::region::Region`] is `V`, the
/// viable region, and confusing *how widely an operative is looking* with
/// *where the household is allowed to be* would be a genuinely misleading
/// collision. Twenty-ninth; the newcomer takes the different name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stance {
    /// Small `b`, high `ρ` — few things, looked at closely.
    Narrow,
    /// Large `b`, shallow `d`, low `ρ` — many things, glanced at.
    Broad,
}

/// Why an attention could not be declared.
///
/// ★ No `Eq` — [`AttentionError::OverBudget`] carries real-valued costs, since
/// `κ_att` is a governed `f64` rather than the integer constant it replaced.
#[derive(Debug, Clone, PartialEq)]
pub enum AttentionError {
    /// An axis at zero is not an aperture — it is *look at nothing*.
    ZeroAxis { axis: &'static str },
    /// `κ·b·d·ρ` exceeds `B_att`.
    OverBudget { cost: f64, budget: f64 },
    /// ★★★ Both apertures land in the same region. Declaring one aperture
    /// under two names is the narrow/broad partition arriving through the back
    /// door, and §V is explicit that both belong to **one** mind.
    NotTwoRegions { both: Stance },
}

impl std::fmt::Display for AttentionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttentionError::ZeroAxis { axis } => {
                write!(f, "aperture axis '{axis}' is zero: that is not an aperture, it is blindness")
            }
            AttentionError::OverBudget { cost, budget } => {
                write!(f, "aperture costs {cost} against a budget of {budget}")
            }
            AttentionError::NotTwoRegions { both } => write!(
                f,
                "narrow and broad both land in the {both:?} region: an operative needs \
                 two attentions, not one aperture named twice"
            ),
        }
    }
}

/// `a = ⟨b, d, ρ⟩`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aperture {
    breadth: usize,
    depth: usize,
    resolution: usize,
}

impl Aperture {
    /// Every axis must be at least 1. An aperture with a zero axis looks at
    /// nothing, which is not a way of attending.
    pub fn declared(breadth: usize, depth: usize, resolution: usize) -> Result<Aperture, AttentionError> {
        for (axis, v) in [("breadth", breadth), ("depth", depth), ("resolution", resolution)] {
            if v == 0 {
                return Err(AttentionError::ZeroAxis { axis });
            }
        }
        Ok(Aperture { breadth, depth, resolution })
    }

    pub fn breadth(&self) -> usize {
        self.breadth
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn resolution(&self) -> usize {
        self.resolution
    }

    /// `pawa(a) = κ·b·d·ρ`, under the **governed** `κ_att`.
    ///
    /// ★ The **product** form is what makes the trade closed: under a fixed
    /// budget, raising one axis must be paid for by lowering another.
    ///
    /// ★★ Takes the parameters rather than reading a constant, so the cost an
    /// aperture is refused on is the same one it is charged.
    pub fn cost_under(&self, parameters: &Parameters) -> f64 {
        parameters.attention_kappa()
            * (self.breadth * self.depth * self.resolution) as f64
    }

    pub fn affordable_under(&self, budget: f64, parameters: &Parameters) -> bool {
        self.cost_under(parameters) <= budget
    }

    /// Which region this aperture lands in.
    ///
    /// ★ A region, not a label: the classification is read off `⟨b,d,ρ⟩`
    /// itself, so an operative cannot call a broad aperture *narrow* by
    /// declaring it so. Breadth against depth-times-resolution is the axis that
    /// separates *few things closely* from *many things glanced at*.
    pub fn stance(&self) -> Stance {
        if self.breadth > self.depth * self.resolution {
            Stance::Broad
        } else {
            Stance::Narrow
        }
    }

    /// ★ The number of sustains a **path with fringes** visits: one chosen
    /// child per level, `b` siblings sampled at each. Linear in `d`.
    pub fn path_visits(&self) -> usize {
        self.breadth * self.depth
    }

    /// What an **exhaustive subtree** would have visited: `Σ_{ℓ<d} b^ℓ`.
    ///
    /// ★ Present **only so the gap can be measured**, never used by the walker
    /// — there is no `walk_subtree` to consume it. Saturating, because the
    /// point of the comparison is that it overflows a real budget quickly.
    pub fn subtree_visits(&self) -> usize {
        (0..self.depth).fold(0usize, |acc, l| acc.saturating_add(self.breadth.saturating_pow(l as u32)))
    }
}

/// `attention = ⟨narrow, broad⟩`.
///
/// ★★★ Two required fields, no `Option`, no single-attention constructor — an
/// operative with one attention is **unrepresentable**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attention {
    narrow: Aperture,
    broad: Aperture,
}

impl Attention {
    /// Declare both. Refuses a pair that is one region twice, and a pair whose
    /// **combined** spend exceeds the budget.
    /// ★ Two different bounds, and both are real. **Here** the question is
    /// *can this pair be afforded at all*, once, against a period's whole
    /// allocation — a property of the declaration. [`AttentionBudget::spend`]
    /// asks the other one: *can it be afforded now, given what has already
    /// been spent* — a property of the sequence.
    pub fn declared(
        narrow: Aperture,
        broad: Aperture,
        budget: f64,
        parameters: &Parameters,
    ) -> Result<Attention, AttentionError> {
        if narrow.stance() == broad.stance() {
            return Err(AttentionError::NotTwoRegions { both: narrow.stance() });
        }
        let cost = narrow.cost_under(parameters) + broad.cost_under(parameters);
        if cost > budget {
            return Err(AttentionError::OverBudget { cost, budget });
        }
        Ok(Attention { narrow, broad })
    }

    pub fn narrow(&self) -> &Aperture {
        &self.narrow
    }

    pub fn broad(&self) -> &Aperture {
        &self.broad
    }

    /// `pawa(narrow) + pawa(broad)`, under the **governed** `κ_att`.
    pub fn cost_under(&self, parameters: &Parameters) -> f64 {
        self.narrow.cost_under(parameters) + self.broad.cost_under(parameters)
    }

    /// ★ Both regions are visited, by construction. Asserted as a set so the
    /// claim is checkable rather than argued.
    pub fn stances(&self) -> BTreeSet<Stance> {
        [self.narrow.stance(), self.broad.stance()].into_iter().collect()
    }
}

// ── `B_att` — a real, spendable budget ───────────────────────────────────────

/// One attention actually paid for.
///
/// ★★★ **`spend` takes an [`Attention`], not an amount** — the same discipline
/// as [`charge(&PawaReading)`](crate::juul::JuulLedger::charge) and
/// [`issue(&PawaReading)`](crate::juul::JuulLedger::issue), applied one layer
/// up. There is **no `spend(amount)`** on the budget, and an `Attention` cannot
/// be constructed except through [`Attention::declared`], which already refuses
/// a one-region pair and an unaffordable one. So a spend is always backed by a
/// real, well-formed attention, and **a cost cannot be invented**.
#[derive(Debug, Clone, PartialEq)]
pub struct AttentionSpend {
    attention: Attention,
    cost: f64,
}

impl AttentionSpend {
    /// The attention this spend paid for — so *what was this spent on* stays
    /// answerable, exactly as a [`Debit`](crate::juul::Entry::Debit) carries
    /// the run that incurred it.
    pub fn attention(&self) -> &Attention {
        &self.attention
    }

    pub fn cost(&self) -> f64 {
        self.cost
    }
}

/// What a spend against `B_att` did.
///
/// ★★ `OverBudget` is a **normal outcome, not an error** — the same shape as
/// [`Charge::Insufficient`](crate::juul::Charge::Insufficient). It reports both
/// numbers so a caller can say *how much short*, and **nothing is appended**,
/// so the budget is byte-identical afterwards.
#[derive(Debug, Clone, PartialEq)]
pub enum Attended {
    /// Paid for, and here is what is left.
    Spent { cost: f64, remaining: f64 },
    /// ★★★ **Refused, not clamped** — and that is not a preference. See
    /// [`AttentionBudget::spend`].
    OverBudget { cost: f64, remaining: f64 },
}

impl Attended {
    pub fn spent(&self) -> bool {
        matches!(self, Attended::Spent { .. })
    }

    /// How far short a refusal was. `None` when it was paid.
    pub fn shortfall(&self) -> Option<f64> {
        match self {
            Attended::OverBudget { cost, remaining } => Some(cost - remaining),
            Attended::Spent { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Attended::Spent { cost, remaining } => {
                format!("attended at {cost}; {remaining} of B_att left")
            }
            Attended::OverBudget { cost, remaining } => {
                format!("over B_att: this attention costs {cost} and {remaining} is left")
            }
        }
    }
}

/// `B_att` — the attention budget, as a **real spendable quantity**.
///
/// ★★★ **The residual OPV-7 recorded, closed.** That row's own words were that
/// `B_att` was *a number, not a balance*: the cost function and the
/// declaration-time bound were real, but nothing was ever **spent**, so a
/// second scan cost exactly what the first did and an operative could attend
/// without limit. A cost with nothing behind it is the same shape of lie as
/// PAWA-2's `debit(amount)` — and it is closed the same way.
///
/// ## ★★ The identity holds by construction, not by check
///
/// `spent` is the **fold of an append-only spend list** and `remaining` is
/// `allocated − spent`, so
///
/// ```text
/// spent + remaining == allocated
/// ```
///
/// is true **by the shape of the type**. There is no stored `spent` field that
/// could drift from the spends that produced it — the same discipline as
/// `state = fold(events)` and [`JuulLedger`](crate::juul::JuulLedger)'s balance.
///
/// ## ★★★ Refused, never clamped — and the reason is structural
///
/// The obvious alternative is to *clamp*: shrink an axis until the aperture
/// fits. It is rejected, and not on taste. [`Aperture::stance`] classifies by
/// `b > d·ρ`, so **lowering `ρ` or `d` can flip a Narrow aperture to Broad** —
/// and [`Attention::declared`] exists to refuse a pair that is **one region
/// twice**, because *declaring the same aperture under two names is the
/// partition of the council arriving through the back door*. A clamp could
/// therefore silently produce exactly the attention the module already forbids
/// anyone from declaring. It is proven, not argued: see the conformance case
/// `clamping_is_unsound_because_it_can_flip_a_stance`.
///
/// ★ And the operative is told. An unaffordable attention stops the turn with
/// [`Reasoning::AttentionUnaffordable`](crate::agent::Reasoning) rather than
/// quietly scanning less than it declared — *silently over-served* and
/// *silently under-served* are the same defect wearing different signs.
///
/// ## ★ The allocation is the host's, per operative — deliberately not governed
///
/// `κ_att` **is** governed (PAWA-11), because it is a coefficient of the cost
/// *model* and one number is the right number. The **allocation** is not, and
/// that is a judgement rather than an omission: how much attention *this*
/// operative gets *this* period is a policy that legitimately differs between
/// operatives, so a single governed scalar would be the wrong shape — the same
/// reason PAWA-8's `Schedule` type is declared-at-spec rather than governed.
///
/// ★★ There is **no period here.** This core has no clock (ADR-0001), so it
/// cannot know when a period turns; [`AttentionBudget::renew`] is the *host*
/// saying one has.
#[derive(Debug, Clone, PartialEq)]
pub struct AttentionBudget {
    allocated: f64,
    spends: Vec<AttentionSpend>,
}

impl AttentionBudget {
    /// Allocate `B_att` for a period.
    ///
    /// ★ Refuses a negative or non-finite allocation: a budget you are already
    /// past before spending anything is not a budget. **Zero is allowed** and
    /// is the honest *attend to nothing* — every spend is refused, visibly.
    pub fn allocated(budget: f64) -> Option<AttentionBudget> {
        if budget < 0.0 || !budget.is_finite() {
            return None;
        }
        Some(AttentionBudget { allocated: budget, spends: Vec::new() })
    }

    /// `B_att`, as declared.
    pub fn allocation(&self) -> f64 {
        self.allocated
    }

    /// ★★ The **fold** — there is no stored total to drift.
    pub fn spent(&self) -> f64 {
        self.spends.iter().map(|s| s.cost).sum()
    }

    /// `allocated − spent`.
    pub fn remaining(&self) -> f64 {
        self.allocated - self.spent()
    }

    /// ★★★ Pay for one attention, or refuse.
    ///
    /// Takes the [`Attention`] itself, so the cost is always computed from a
    /// real declared pair under the **governed** `κ_att` — the caller cannot
    /// name a number. On refusal **nothing is appended**, so the budget is
    /// unchanged and the identity still holds.
    ///
    /// ★ It never clamps; see the type's own docs for why that is unsound here
    /// rather than merely unspecified.
    pub fn spend(&mut self, attention: &Attention, parameters: &Parameters) -> Attended {
        let cost = attention.cost_under(parameters);
        let remaining = self.remaining();
        if cost > remaining {
            return Attended::OverBudget { cost, remaining };
        }
        self.spends.push(AttentionSpend { attention: *attention, cost });
        Attended::Spent { cost, remaining: remaining - cost }
    }

    /// Every spend, in order — the record `spent()` folds.
    pub fn spends(&self) -> &[AttentionSpend] {
        &self.spends
    }

    /// ★ A fresh period at the same allocation.
    ///
    /// The core has **no clock**, so it cannot know when a period turns: this
    /// is the host saying one has, exactly as `at` and `now` are host-supplied
    /// everywhere else.
    pub fn renew(&self) -> AttentionBudget {
        AttentionBudget { allocated: self.allocated, spends: Vec::new() }
    }

    /// Load a host's durable copy. ★ The same door
    /// [`JuulLedger::with_entries`](crate::juul::JuulLedger::with_entries) and
    /// `NonceLedger::with_spent` open: the core holds the decision, the host
    /// holds the durability.
    pub fn with_spends<I: IntoIterator<Item = AttentionSpend>>(
        budget: f64,
        spends: I,
    ) -> Option<AttentionBudget> {
        let mut b = AttentionBudget::allocated(budget)?;
        b.spends = spends.into_iter().collect();
        Some(b)
    }
}

/// Whether an attention budget is in force for one turn, and which.
///
/// ★★ `Unbudgeted` is the default for every caller written before this row,
/// and it is named for the same reason
/// [`Affordability::Unmetered`](crate::juul::Affordability::Unmetered),
/// `Authorization::Unchecked` and `EffectClass::Unchecked` are: *a bypass that
/// reads as ordinary is the problem; one that has to be spelled out is not.*
/// An absent budget makes the clause **vacuous** — it does not fail closed to
/// *attend to nothing*, because a host that has not allocated attention has not
/// said the operative may look at nothing, it has said there is no budget in
/// force.
#[derive(Debug)]
pub enum AttentionSpending<'a> {
    /// No budget in force. Behaviour is byte-identical to before this row.
    Unbudgeted,
    /// Charge each turn's attention against this budget, at these parameters.
    Budgeted { budget: &'a mut AttentionBudget, parameters: &'a Parameters },
}

impl AttentionSpending<'_> {
    /// Whether a budget is in force at all.
    pub fn budgeted(&self) -> bool {
        matches!(self, AttentionSpending::Budgeted { .. })
    }
}

/// Narrow's output: a **score on a candidate**.
///
/// ★★★ No public constructor. The only way to obtain one is [`narrow_scan`],
/// which computes `value` from [`Region::distance`] — the household's own
/// `d(s,V)`. An operative cannot mint a score, cannot raise its own, and has
/// nowhere to declare a rival notion of *important*.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    sustain_id: String,
    /// `d(s,V)` for this sustain, normalised against the field it was scanned
    /// with — the same field-relative discipline MON-7's brightness follows.
    value: f64,
}

impl Score {
    pub fn sustain_id(&self) -> &str {
        &self.sustain_id
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

/// Broad's output: **the frame changed**, or it did not.
///
/// ★ Deliberately not a number. *This option is worth 0.7* and *the frame you
/// were scoring in has changed* are different kinds of answer, and collapsing
/// them into one scale is what makes an agent a maximiser inside a frame it
/// cannot notice is wrong.
#[derive(Debug, Clone, PartialEq)]
pub enum Reframing {
    /// Nothing outside the task's own branch is out of its region.
    FrameHolds,
    /// Something the narrow view was not looking at has left `V`.
    FrameChanged {
        /// Where — a sustain the broad sweep visited, not the task's own.
        at: String,
        why: String,
    },
}

impl Reframing {
    pub fn triggered(&self) -> bool {
        matches!(self, Reframing::FrameChanged { .. })
    }
}

/// What one scan looked at.
#[derive(Debug, Clone, PartialEq)]
pub struct Scan {
    /// The sustains visited, in visit order — attention's **footprint**.
    pub visited: Vec<String>,
    /// ★★ The aperture this scan was run with — so the cost is **derivable
    /// from the governed `κ_att`** rather than frozen at whatever the
    /// coefficient happened to be. A stored number would be the shadow
    /// constant this row exists to remove, one level down.
    pub aperture: Aperture,
    /// How deep the path actually went (bounded by `d` and by the tree).
    pub reached_depth: usize,
}

impl Scan {
    /// What this scan cost, under the governed `κ_att`.
    pub fn cost_under(&self, parameters: &Parameters) -> f64 {
        self.aperture.cost_under(parameters)
    }

    /// ★ The footprint, as a set — what may enter the `K ≈ 4` hold.
    pub fn footprint(&self) -> BTreeSet<&str> {
        self.visited.iter().map(String::as_str).collect()
    }
}

/// The children of a sustain, derived from the engine's parent chain.
///
/// ★ Derived rather than stored: the `⊕` tree already exists as
/// [`MonitorEngine`]'s parent relation, and a second copy would be a second
/// thing to keep true.
fn children_of<'a>(engine: &'a MonitorEngine, id: &str) -> Vec<&'a str> {
    engine
        .sustains()
        .iter()
        .filter(|s| engine.parent_of(s) == Some(id))
        .map(String::as_str)
        .collect()
}

/// ★★★ **A path with fringes** — one chosen child per level, `b` siblings
/// sampled at each and *not* descended into.
///
/// There is deliberately **no `walk_subtree`**: the geometric traversal is not
/// merely discouraged, it is not expressible through this module. Visits at
/// most `b·d`.
pub fn scan(engine: &MonitorEngine, from: &str, aperture: &Aperture) -> Scan {
    let mut visited: Vec<String> = Vec::new();
    let mut here = from.to_string();
    let mut reached = 0usize;

    for _ in 0..aperture.depth {
        let kids = children_of(engine, &here);
        if kids.is_empty() {
            break;
        }
        // The fringe: up to `b` siblings sampled at this level.
        for k in kids.iter().take(aperture.breadth) {
            visited.push((*k).to_string());
        }
        reached += 1;
        // ★ The path: exactly ONE child is descended into. This is the line
        // that makes the traversal linear rather than geometric.
        here = kids[0].to_string();
    }

    Scan { visited, aperture: *aperture, reached_depth: reached }
}

/// Narrow: score the candidates the scan reached, from `d(s,V)`.
///
/// ★★★ The values come from `region.distance(state)` and nowhere else, and
/// [`Score`] cannot be built any other way — so narrow attention consumes the
/// one un-gameable urgency rather than introducing a rival.
pub fn narrow_scan(
    scan: &Scan,
    region: &Region,
    state_of: impl Fn(&str) -> Option<Value>,
) -> Vec<Score> {
    let mut out = Vec::new();
    for id in &scan.visited {
        let Some(state) = state_of(id) else { continue };
        let Ok(distance) = region.distance(&state) else { continue };
        out.push(Score { sustain_id: id.clone(), value: distance.weighted });
    }
    // Worst first — and the ordering is `d(s,V)`'s, not an operative's.
    out.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Broad: has the frame changed somewhere the task was not looking?
///
/// ★ Returns a [`Reframing`], never a score. It reports **that** the frame
/// moved and **where**, and deliberately offers no magnitude — a number here
/// would invite a caller to weigh it against a narrow score, which is exactly
/// the collapse the two output types exist to prevent.
pub fn broad_scan(
    scan: &Scan,
    region: &Region,
    task_branch: &BTreeSet<&str>,
    state_of: impl Fn(&str) -> Option<Value>,
) -> Reframing {
    for id in &scan.visited {
        if task_branch.contains(id.as_str()) {
            continue;
        }
        let Some(state) = state_of(id) else { continue };
        let Ok(membership) = region.membership(&state) else { continue };
        if !membership.is_viable() {
            return Reframing::FrameChanged {
                at: id.clone(),
                why: "outside the viable region, and outside what the task was looking at"
                    .to_string(),
            };
        }
    }
    Reframing::FrameHolds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::CusumSpec;
    use crate::monitor::SustainWatch;
    use crate::region::Interval;
    use serde_json::json;

    fn region() -> Region {
        Region::new().bounding(Interval::new("balance", 0.0, 100.0)).weighing("balance", 1.0)
    }

    fn spec() -> CusumSpec {
        CusumSpec::new(0.0, 1.0, 5.0)
    }

    /// A three-level ⊕ tree: root → mid → leaf, with siblings at each level.
    fn tree() -> MonitorEngine {
        let decls = vec![
            SustainWatch::new("root", region(), 0.3, spec()),
            SustainWatch::new("mid_a", region(), 0.3, spec()).under("root"),
            SustainWatch::new("mid_b", region(), 0.3, spec()).under("root"),
            SustainWatch::new("mid_c", region(), 0.3, spec()).under("root"),
            SustainWatch::new("leaf_a", region(), 0.3, spec()).under("mid_a"),
            SustainWatch::new("leaf_b", region(), 0.3, spec()).under("mid_a"),
        ];
        MonitorEngine::flatten_holarchy(decls).unwrap()
    }

    fn narrow() -> Aperture {
        Aperture::declared(1, 3, 4).unwrap()
    }

    fn broad() -> Aperture {
        Aperture::declared(12, 1, 1).unwrap()
    }

    // ── both attentions, structurally ────────────────────────────────────────

    #[test]
    fn an_attention_carries_both_and_there_is_no_way_to_declare_one() {
        // ★★★ Two required fields, no Option, no single-attention constructor.
        let a = Attention::declared(narrow(), broad(), 100.0, &Parameters::genesis()).unwrap();
        assert_eq!(a.stances(), [Stance::Narrow, Stance::Broad].into_iter().collect());
    }

    #[test]
    fn a_pair_that_is_one_region_twice_is_refused() {
        // ★★★ The partition arriving through the back door.
        let n1 = Aperture::declared(1, 3, 4).unwrap();
        let n2 = Aperture::declared(2, 4, 3).unwrap();
        assert_eq!(n1.stance(), Stance::Narrow);
        assert_eq!(n2.stance(), Stance::Narrow);
        assert_eq!(
            Attention::declared(n1, n2, 1000.0, &Parameters::genesis()),
            Err(AttentionError::NotTwoRegions { both: Stance::Narrow })
        );
    }

    #[test]
    fn a_zero_axis_is_not_an_aperture() {
        assert_eq!(
            Aperture::declared(0, 1, 1),
            Err(AttentionError::ZeroAxis { axis: "breadth" })
        );
        assert!(Aperture::declared(1, 0, 1).is_err());
        assert!(Aperture::declared(1, 1, 0).is_err());
    }

    #[test]
    fn the_stance_is_read_off_the_axes_not_declared() {
        // ★ An operative cannot call a broad aperture narrow by saying so:
        // there is no field to say it in.
        assert_eq!(Aperture::declared(1, 3, 4).unwrap().stance(), Stance::Narrow);
        assert_eq!(Aperture::declared(12, 1, 1).unwrap().stance(), Stance::Broad);
    }

    // ── the closed trade ─────────────────────────────────────────────────────

    #[test]
    fn deep_and_narrow_costs_the_same_as_broad_and_shallow() {
        // ★★ Iso-cost: the product is what is bounded, so the two shapes are
        // the same spend and neither axis is free.
        let deep = Aperture::declared(1, 4, 3).unwrap();
        let wide = Aperture::declared(12, 1, 1).unwrap();
        assert_eq!(deep.cost_under(&Parameters::genesis()), 12.0);
        assert_eq!(wide.cost_under(&Parameters::genesis()), 12.0);
    }

    // ── B_att: a balance, not a number ───────────────────────────────────────

    fn pair() -> Attention {
        Attention::declared(narrow(), broad(), 100.0, &Parameters::genesis()).unwrap()
    }

    #[test]
    fn spent_plus_remaining_is_always_the_allocation() {
        let p = Parameters::genesis();
        let a = pair();
        let mut b = AttentionBudget::allocated(a.cost_under(&p) * 2.0).unwrap();
        for _ in 0..3 {
            b.spend(&a, &p);
            assert_eq!(b.spent() + b.remaining(), b.allocation());
        }
        assert_eq!(b.spends().len(), 2, "the third did not fit and was not appended");
    }

    #[test]
    fn a_refusal_leaves_the_budget_byte_identical() {
        let p = Parameters::genesis();
        let mut b = AttentionBudget::allocated(1.0).unwrap();
        let before = b.clone();
        assert!(!b.spend(&pair(), &p).spent());
        assert_eq!(b, before);
    }

    #[test]
    fn every_spend_names_the_attention_it_paid_for() {
        let p = Parameters::genesis();
        let a = pair();
        let mut b = AttentionBudget::allocated(1_000.0).unwrap();
        assert!(b.spend(&a, &p).spent());
        assert_eq!(b.spends()[0].attention(), &a);
        assert_eq!(b.spends()[0].cost(), a.cost_under(&p));
    }

    #[test]
    fn a_governed_kappa_change_moves_what_an_aperture_costs() {
        let genesis = Parameters::genesis();
        let doubled = Parameters::read(&json!({"parameters": {"attention_kappa": 2.0}}));
        let a = Aperture::declared(1, 4, 3).unwrap();
        assert_eq!(a.cost_under(&genesis), 12.0, "the deleted constant's own value");
        assert_eq!(a.cost_under(&doubled), 24.0);
    }

    #[test]
    fn a_host_can_reload_a_budget_and_renew_a_period() {
        let p = Parameters::genesis();
        let a = pair();
        let mut b = AttentionBudget::allocated(1_000.0).unwrap();
        b.spend(&a, &p);
        let reloaded = AttentionBudget::with_spends(1_000.0, b.spends().to_vec()).unwrap();
        assert_eq!(reloaded, b, "the core holds the decision, the host the durability");
        assert_eq!(b.renew().spent(), 0.0);
    }

    #[test]
    fn raising_an_axis_is_paid_for_by_lowering_another() {

        let budget = 12.0;
        let a = Aperture::declared(2, 2, 3).unwrap();
        assert!(a.affordable_under(budget, &Parameters::genesis()));
        // Doubling depth alone breaks the budget...
        assert!(!Aperture::declared(2, 4, 3).unwrap().affordable_under(budget, &Parameters::genesis()));
        // ...unless resolution pays for it.
        assert!(Aperture::declared(2, 4, 1).unwrap().affordable_under(budget, &Parameters::genesis()));
    }

    #[test]
    fn an_attention_over_budget_is_refused() {
        assert!(matches!(
            Attention::declared(narrow(), broad(), 10.0, &Parameters::genesis()),
            Err(AttentionError::OverBudget { .. })
        ));
    }

    // ── path with fringes, not a subtree ─────────────────────────────────────

    #[test]
    fn the_scan_is_a_path_with_fringes_and_visits_at_most_b_times_d() {
        let engine = tree();
        let a = Aperture::declared(3, 2, 1).unwrap();
        let s = scan(&engine, "root", &a);
        assert!(s.visited.len() <= a.path_visits(), "linear in d, not geometric");
        // Level 1 sampled three mids; level 2 descended into ONE of them.
        assert!(s.visited.contains(&"mid_a".to_string()));
        assert!(s.visited.contains(&"mid_c".to_string()));
        assert!(s.visited.contains(&"leaf_a".to_string()));
        assert_eq!(s.reached_depth, 2);
    }

    #[test]
    fn the_geometric_traversal_is_not_expressible() {
        // ★★★ `subtree_visits` exists only to MEASURE the gap; nothing
        // consumes it, and there is no `walk_subtree` to call. At b=8, d=6 the
        // subtree is tens of thousands of visits and the path is 48.
        let a = Aperture::declared(8, 6, 1).unwrap();
        assert_eq!(a.path_visits(), 48);
        assert!(a.subtree_visits() > 30_000, "which is why nobody may ask for it");
    }

    #[test]
    fn a_scan_stops_at_a_nano_sustain_rather_than_pretending_to_go_deeper() {
        let engine = tree();
        // mid_b has no children.
        let s = scan(&engine, "mid_b", &Aperture::declared(3, 5, 1).unwrap());
        assert!(s.visited.is_empty());
        assert_eq!(s.reached_depth, 0);
    }

    // ── the two output types ─────────────────────────────────────────────────

    #[test]
    fn narrow_returns_a_score_and_it_comes_from_d_s_v() {
        // ★★★ `Score` has no constructor; the value is region.distance's.
        let engine = tree();
        let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
        let scores = narrow_scan(&s, &region(), |id| match id {
            "mid_a" => Some(json!({"balance": 160.0})),
            "mid_b" => Some(json!({"balance": 50.0})),
            _ => None,
        });
        assert_eq!(scores[0].sustain_id(), "mid_a", "worst first, by d(s,V)");
        assert!(scores[0].value() > 0.0);
        assert_eq!(scores[1].value(), 0.0, "inside V");
    }

    #[test]
    fn an_operative_cannot_mint_a_score() {
        // Compile-time really: `Score`'s fields are private and there is no
        // constructor. What is assertable is that every score that exists came
        // from a scan the household's own region measured.
        let engine = tree();
        let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
        let none = narrow_scan(&s, &region(), |_| None);
        assert!(none.is_empty(), "no state, no score — not a fabricated zero");
    }

    #[test]
    fn broad_returns_a_reframing_not_a_number() {
        let engine = tree();
        let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
        let task: BTreeSet<&str> = ["mid_a"].into_iter().collect();
        let r = broad_scan(&s, &region(), &task, |id| match id {
            "mid_a" => Some(json!({"balance": 50.0})),
            "mid_b" => Some(json!({"balance": 900.0})),
            _ => Some(json!({"balance": 50.0})),
        });
        match r {
            Reframing::FrameChanged { at, .. } => assert_eq!(at, "mid_b"),
            other => panic!("expected a reframing, got {other:?}"),
        }
    }

    #[test]
    fn broad_ignores_what_the_task_was_already_looking_at() {
        // ★ The point of broad is what narrow is NOT covering. A breach inside
        // the task's own branch is narrow's job, not a reframing.
        let engine = tree();
        let s = scan(&engine, "root", &Aperture::declared(3, 1, 1).unwrap());
        let task: BTreeSet<&str> = ["mid_a", "mid_b", "mid_c"].into_iter().collect();
        let r = broad_scan(&s, &region(), &task, |_| Some(json!({"balance": 900.0})));
        assert_eq!(r, Reframing::FrameHolds);
        assert!(!r.triggered());
    }

    // ── the footprint and the hold ───────────────────────────────────────────

    #[test]
    fn the_scan_footprint_can_be_wide_while_the_hold_stays_four() {
        // ★ "Anything can scan widely and hold four; nothing can hold widely."
        let engine = tree();
        let s = scan(&engine, "root", &Aperture::declared(3, 2, 1).unwrap());
        assert!(s.footprint().len() > crate::curated::DEFAULT_BUDGET / 2);
        // The hold is UI-1's, and it is unchanged by how widely attention swept.
        assert_eq!(crate::curated::DEFAULT_BUDGET, 4);
    }
}
