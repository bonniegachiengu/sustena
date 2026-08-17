//! The sparse mixture-of-experts gate — Orchie's *route* duty
//! (Operative §XV + §XIII · OPV-28).
//!
//! ```text
//! g(x, s) = top-k( softmax( α·relevance(x,i) + η·Δû_i(x) + ζ·match(i,s) ) )
//! ```
//!
//! Jacobs, Jordan, Nowlan & Hinton (1991) gave the gating network — weights
//! over experts, trained so credit assignment rewards local competence and
//! experts specialise rather than averaging into mush. Shazeer et al. (2017)
//! made the gate **sparse**: top-`k` with load balancing.
//!
//! > Sparsity is the property Sustena needs, because **evaluating an operative
//! > runs a graph, may fork state, and is metered.**
//!
//! ## ★★ Sparsity is structural, not a post-filter
//!
//! If you score all `N` and then truncate, you have built a ranked list, not a
//! sparse gate — the expensive work already happened. So the expensive path is
//! not something a caller does *after* consulting the router; **the router
//! performs it.** [`Dispatch::run`] applies the caller's expensive function to
//! the selected tickets and to nothing else, and there is no method anywhere
//! that hands a [`Ticket`] to a non-selected operative or iterates the whole
//! population with `f`. `Ticket` has private fields and is minted only inside
//! [`Mixture::route`].
//!
//! That is checkable rather than asserted: the conformance case passes a
//! counting closure and asserts it ran **`k` times out of `N`** — and that the
//! un-selected operatives' function was never entered at all.
//!
//! The gating score itself must therefore be cheap, which is what the **hat**
//! in `Δû` is doing: routing cannot require running the expensive thing first.
//! This module has no I/O, no fork and no state engine — the estimate is
//! **supplied** by the caller, cheaply, exactly as MUL-14's pressure is.
//!
//! ## ★ `relevance` and `match` are different axes and cannot substitute
//!
//! > Note that the existing test and `match` are **different axes**: subject
//! > matter versus Cynefin regime. Conflating them, **including by reusing the
//! > key**, would make one unexpressible.
//!
//! [`Contender::subject_tags`] is the subject-matter axis and
//! [`crate::operative::Operative::suited`] is the Cynefin one. They are
//! different fields of different types — a `BTreeSet<String>` and a
//! `BTreeSet<Cynefin>` — so one cannot stand in for the other even by
//! accident. This is the same collision OPV-16 recorded and `ω` honoured; §XV
//! restates it, and the naming already holds.
//!
//! ## ★★ The routing scalar is walled off from presentation
//!
//! `Δû_i` is a **vector** (OPV-3) and the gate needs a scalar. Feeding a
//! vector into a scalar softmax is the §II collapse — the very thing OPV-29
//! just stopped a surface doing.
//!
//! The resolution is not to pretend otherwise. **The router's scalar is a
//! routing-only quantity: it chooses *whom to ask*, never *what to do.*** The
//! collapse is permitted here because attention is metered and someone must be
//! asked first; it is not permitted downstream, and the wall is structural:
//! [`Dispatch`] **carries no [`crate::operative::Alternative`] and no frontier
//! at all**, so there is no value in it that could reach
//! [`crate::presentation::present`]. The decision surface still returns the
//! whole frontier.
//!
//! Proven, not claimed: the conformance case routes, then presents, and
//! asserts the frontier is **byte-identical** to presenting without having
//! routed. The routing score has zero influence on what a person is shown.
//!
//! The collapse rule is [`EstimateRule`] — **declared and named**, so which
//! collapse was applied is auditable rather than buried.
//!
//! ## ★ Borrowed for selection, deliberately not for combination
//!
//! Jacobs et al.'s mixture *combines* expert outputs, `y = Σ_i g_i(x) f_i(x)`.
//! **Sustena must not.** Averaging expert outputs is the §II collapse again,
//! one layer further on — and §XV's third duty is to present the frontier, not
//! a blend. So the gate is taken for **selection** and the combination step is
//! deliberately not taken. `g_i` is reported (it is useful to see how sharply
//! the gate discriminated) and is never used to weight an answer.
//!
//! A related honesty note: **softmax is monotone**, so it does not change
//! *which* `k` are selected — top-`k` by softmax is top-`k` by raw score. It
//! normalises the weights for reporting. Saying so is better than implying the
//! softmax is doing selection work it is not.
//!
//! ## `match = 0` → abstention, and the coverage condition
//!
//! > When `match = 0` the cheap shipped behaviour is **abstention** —
//! > `CouncillorConfig.is_relevant()` already produces an ABSTAIN path, and
//! > domain mismatch is a **second** reason to take it.
//!
//! So a domain mismatch is withheld under its own reason
//! ([`WithheldReason::DomainMismatch`]), distinct from losing the top-`k`.
//! Conflating them would hide the coverage question behind a scoring result.
//!
//! > The population acquires requisite variety only if `⋃_i dom_i` covers all
//! > four domains. If some domain is in no operative's set, there is a regime
//! > in which **no operative is suited**, and the correct behaviour is to
//! > **surface the gap** rather than route to the least-bad scorer.
//!
//! [`coverage_gaps`] is cheap, so it is folded in here rather than deferred:
//! when `dom(s)` itself is uncovered, [`Mixture::route`] returns **zero
//! tickets** and names the gap. Routing to the least-bad scorer is not a
//! degraded answer, it is a confidently wrong one — an operative built for
//! complicated-domain optimisation *presupposes analysability*.
//!
//! **Left to OPV-16:** *disorder* as a detection target, and computing `dom(s)`
//! at all. Which brings us to:
//!
//! ## Slots — named, not stubbed
//!
//! - **`dom(s)` is SUPPLIED, not computed.** It is a Monitor output whose
//!   inputs include §VIII's criticality detector (OPV-14, ⬜). [`Mixture::route`]
//!   takes it as an argument and says so; nothing here infers a regime.
//! - **`Δû` is SUPPLIED.** Computing it would mean forking, which is the
//!   expensive thing routing exists to avoid.
//! - **Weights are DECLARED, never learned.** §XV: *"a declared gate is
//!   auditable where a learned one is not."* There is no training loop, no
//!   gradient and no fabricated gating network here.
//! - **Shazeer's load balancing is NOT built.** It is an auxiliary *training*
//!   loss that stops a gate collapsing onto a few experts, and without
//!   training infrastructure there is no loss to add — building something and
//!   calling it load balancing would be the fabrication this project refuses.
//!   What *is* cheap and honest is the adjacent read:
//!   [`Mixture::never_reachable`] names operatives that no `dom(s)` could ever
//!   select, which is the dead-expert problem load balancing is ultimately
//!   about, detected structurally rather than trained away.

use std::collections::BTreeSet;

use thiserror::Error;

use crate::operative::{Cynefin, Operative};

// ---------------------------------------------------------------------------
// The declared gate
// ---------------------------------------------------------------------------

/// `α`, `η`, `ζ` — **declared**, never learned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateWeights {
    /// `α` — weight on subject-matter relevance.
    pub alpha: f64,
    /// `η` — weight on the collapsed `Δû` estimate.
    pub eta: f64,
    /// `ζ` — weight on the Cynefin domain match.
    pub zeta: f64,
}

impl GateWeights {
    /// Refuses a negative or non-finite weight: a negative `α` would mean
    /// *prefer the operative that knows least about this*, which is not a
    /// tuning choice, it is a sign error nobody would declare on purpose.
    pub fn new(alpha: f64, eta: f64, zeta: f64) -> Result<Self, MixtureError> {
        for (name, v) in [("α", alpha), ("η", eta), ("ζ", zeta)] {
            if !v.is_finite() || v < 0.0 {
                return Err(MixtureError::BadWeight {
                    which: name.into(),
                    value: v.to_string(),
                });
            }
        }
        if alpha == 0.0 && eta == 0.0 && zeta == 0.0 {
            return Err(MixtureError::AllWeightsZero);
        }
        Ok(Self { alpha, eta, zeta })
    }
}

/// How the **vector** `Δû` becomes the gate's scalar.
///
/// ★ This is the §II collapse, **permitted here and only here**, because the
/// gate chooses whom to ask and someone must be asked. It is a named enum
/// rather than a hardcoded sum so that *which* collapse was applied is
/// auditable — the same reason §XV wants declared weights.
#[derive(Debug, Clone, PartialEq)]
pub enum EstimateRule {
    /// `Σ_k Δû_k` — signed, so a loss on one objective genuinely offsets a
    /// gain on another.
    NetGain,
    /// `Σ_k max(0, Δû_k)` — the optimistic read: how much this operative
    /// thinks it could improve, ignoring what that would cost elsewhere.
    /// Honest for routing (*is there anything here for you?*) and dishonest
    /// for deciding, which is why it may not leave this module.
    GainsOnly,
    /// `Σ_k λ_k Δû_k` with declared per-objective weights.
    Weighted(Vec<f64>),
}

impl EstimateRule {
    fn collapse(&self, estimate: &[f64]) -> Result<f64, MixtureError> {
        for v in estimate {
            if !v.is_finite() {
                return Err(MixtureError::BadEstimate(v.to_string()));
            }
        }
        Ok(match self {
            EstimateRule::NetGain => estimate.iter().sum(),
            EstimateRule::GainsOnly => estimate.iter().map(|v| v.max(0.0)).sum(),
            EstimateRule::Weighted(w) => {
                if w.len() != estimate.len() {
                    return Err(MixtureError::EstimateArity {
                        weights: w.len(),
                        estimate: estimate.len(),
                    });
                }
                w.iter().zip(estimate).map(|(a, b)| a * b).sum()
            }
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            EstimateRule::NetGain => "net gain",
            EstimateRule::GainsOnly => "gains only",
            EstimateRule::Weighted(_) => "declared weights",
        }
    }
}

// ---------------------------------------------------------------------------
// What routing produces
// ---------------------------------------------------------------------------

/// Permission to run one operative's **expensive** path.
///
/// Private fields, minted only by [`Mixture::route`], and issued only to
/// selected operatives. There is no constructor and no way to obtain one for
/// an operative the gate did not select.
#[derive(Debug, Clone, PartialEq)]
pub struct Ticket {
    operative: String,
    rank: usize,
    score: f64,
    gate_weight: f64,
}

impl Ticket {
    pub fn operative(&self) -> &str {
        &self.operative
    }

    /// 0 is the highest-scoring selection.
    pub fn rank(&self) -> usize {
        self.rank
    }

    /// The raw gate score. **Routing-only** — see the module docs on the wall.
    pub fn score(&self) -> f64 {
        self.score
    }

    /// `g_i` after softmax. Reported so it is visible how sharply the gate
    /// discriminated; never used to weight an answer, because averaging expert
    /// outputs is the collapse this whole layer exists to prevent.
    pub fn gate_weight(&self) -> f64 {
        self.gate_weight
    }
}

/// Why an operative was not asked.
///
/// ★ The two reasons are kept apart deliberately. *Not suited to this regime*
/// and *out-scored by others* are different facts, and collapsing them into
/// "not selected" would hide the coverage question behind a scoring result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithheldReason {
    /// `match(i,s) = 0` — §XIII's abstention. Not merely less useful in this
    /// regime; **confidently wrong**, because its method presupposes a
    /// different one.
    DomainMismatch,
    /// Suited and scored, but outside the top `k`. Attention is metered.
    BelowTopK,
}

/// An operative the gate did not ask, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Withheld {
    pub operative: String,
    pub reason: WithheldReason,
    /// `None` for a domain mismatch: it was never scored, because scoring it
    /// would imply the regime question had an answer worth weighing.
    pub score: Option<f64>,
}

/// The gate's output: **who to ask**, and who was not asked and why.
///
/// ★ Carries **no [`crate::operative::Alternative`] and no frontier**. That
/// absence is the wall: there is no value in here that could flow into
/// [`crate::presentation::present`], so the routing collapse cannot reach a
/// person's surface even by mistake.
#[derive(Debug, Clone, PartialEq)]
pub struct Dispatch {
    selected: Vec<Ticket>,
    withheld: Vec<Withheld>,
    considered: usize,
    /// ★ `dom(s)` is in **no** operative's `dom_i`. §XIII: surface the gap
    /// rather than route to the least-bad scorer.
    gap: Option<Cynefin>,
}

impl Dispatch {
    pub fn tickets(&self) -> &[Ticket] {
        &self.selected
    }

    pub fn withheld(&self) -> &[Withheld] {
        &self.withheld
    }

    pub fn considered(&self) -> usize {
        self.considered
    }

    /// ★ A regime no operative is suited to. When this is `Some`, **nobody was
    /// asked** — that is the correct behaviour, not a degraded one.
    pub fn coverage_gap(&self) -> Option<Cynefin> {
        self.gap
    }

    /// `k / N` — how much of the population's expensive path was skipped.
    pub fn sparsity(&self) -> f64 {
        if self.considered == 0 {
            return 0.0;
        }
        1.0 - (self.selected.len() as f64 / self.considered as f64)
    }

    /// ★★ **The expensive path runs here and only here.**
    ///
    /// `f` is applied to the selected tickets and to nothing else. There is no
    /// method on this type that hands a [`Ticket`] to an unselected operative
    /// and none that iterates the whole population with `f` — so *not
    /// evaluating* is a property of the shape rather than a discipline the
    /// caller has to keep.
    pub fn run<T>(&self, f: impl Fn(&Ticket) -> T) -> Vec<T> {
        self.selected.iter().map(f).collect()
    }

    pub fn describe(&self) -> String {
        if let Some(gap) = self.gap {
            return format!(
                "no operative is suited to a {} regime — gap surfaced, nobody asked",
                gap.name()
            );
        }
        format!(
            "asked {} of {} ({:.0}% of the expensive path skipped)",
            self.selected.len(),
            self.considered,
            self.sparsity() * 100.0
        )
    }
}

// ---------------------------------------------------------------------------
// The contender
// ---------------------------------------------------------------------------

/// One operative in the running, with the two axes and the cheap estimate.
///
/// `subject_tags` and the operative's own `dom_i` are **different axes** and
/// different types — §XV's warning made unrepresentable to violate.
#[derive(Debug, Clone, PartialEq)]
pub struct Contender<'a> {
    pub operative: &'a Operative,
    /// `relevance`'s axis — **subject matter**, the tag-list one.
    pub subject_tags: BTreeSet<String>,
    /// `Δû_i` — the cheap estimate, **supplied**. Computing it would mean
    /// forking, which is the expensive thing routing exists to avoid.
    pub estimate: Vec<f64>,
}

impl<'a> Contender<'a> {
    pub fn new(operative: &'a Operative, subject_tags: &[&str], estimate: &[f64]) -> Self {
        Self {
            operative,
            subject_tags: subject_tags.iter().map(|s| (*s).to_string()).collect(),
            estimate: estimate.to_vec(),
        }
    }
}

/// `relevance(x, i)` — a **soft** generalisation of the reference's 0/1 tag
/// intersection.
///
/// The fraction of the request's tags this operative covers. Two boundary
/// behaviours match `is_relevant()` exactly, which is what makes this a
/// generalisation rather than a replacement:
///
/// - an **untagged request** applies to everyone → `1.0`
///   (`if not proposal_domains: return True`);
/// - an operative with **no declared tags** → `0.0`
///   (`if not self.domain: return False`).
///
/// So `subject_relevance(..) > 0` exactly when `is_relevant(..)` is true, and
/// the values in between are the part the hard test cannot express.
pub fn subject_relevance(operative_tags: &BTreeSet<String>, request_tags: &BTreeSet<String>) -> f64 {
    if request_tags.is_empty() {
        return 1.0;
    }
    if operative_tags.is_empty() {
        return 0.0;
    }
    operative_tags.intersection(request_tags).count() as f64 / request_tags.len() as f64
}

/// `⋃_i dom_i` measured against all four domains — the Ashby coverage
/// condition (§XIII).
pub fn coverage_gaps(contenders: &[Contender<'_>]) -> BTreeSet<Cynefin> {
    let covered: BTreeSet<Cynefin> = contenders
        .iter()
        .flat_map(|c| c.operative.suited().iter().copied())
        .collect();
    Cynefin::ALL
        .into_iter()
        .filter(|d| !covered.contains(d))
        .collect()
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

/// The declared sparse gate.
#[derive(Debug, Clone, PartialEq)]
pub struct Mixture {
    weights: GateWeights,
    k: usize,
    rule: EstimateRule,
}

impl Mixture {
    /// Refuses `k = 0`: a gate that asks nobody is not sparse, it is off, and
    /// declaring it as a routing policy would hide that.
    pub fn new(weights: GateWeights, k: usize, rule: EstimateRule) -> Result<Self, MixtureError> {
        if k == 0 {
            return Err(MixtureError::ZeroK);
        }
        Ok(Self { weights, k, rule })
    }

    pub fn k(&self) -> usize {
        self.k
    }

    pub fn weights(&self) -> GateWeights {
        self.weights
    }

    pub fn rule(&self) -> &EstimateRule {
        &self.rule
    }

    /// The honest fragment of load balancing: operatives **no `dom(s)` could
    /// ever select**, because they declare no suited domain at all.
    ///
    /// Not Shazeer's auxiliary loss — that needs training infrastructure this
    /// build does not have, and inventing something to call load balancing
    /// would be fabrication. This is the dead-expert problem that loss exists
    /// to prevent, detected structurally instead.
    pub fn never_reachable<'a>(&self, contenders: &'a [Contender<'a>]) -> Vec<&'a str> {
        contenders
            .iter()
            .filter(|c| c.operative.suited().is_empty())
            .map(|c| c.operative.id())
            .collect()
    }

    /// `g(x,s) = top-k(softmax(α·relevance + η·Δû + ζ·match))`.
    ///
    /// `dom_s` is **supplied** — it is a Monitor output (OPV-14 feeds it) and
    /// nothing here infers a regime.
    ///
    /// Order of business, and the order matters:
    /// 1. **Coverage first.** If `dom(s)` is in no operative's `dom_i`, return
    ///    zero tickets and surface the gap. Routing to the least-bad scorer is
    ///    not a degraded answer; §XIII says it is a confidently wrong one.
    /// 2. **Abstain on mismatch.** `match = 0` withholds under its own reason
    ///    and is never scored.
    /// 3. **Score, softmax, take `k`.** Softmax is monotone so it does not
    ///    change the selection; it normalises the reported weights.
    pub fn route(
        &self,
        contenders: &[Contender<'_>],
        request_tags: &BTreeSet<String>,
        dom_s: Cynefin,
    ) -> Result<Dispatch, MixtureError> {
        if contenders.is_empty() {
            return Err(MixtureError::NoContenders);
        }
        let mut ids = BTreeSet::new();
        for c in contenders {
            if !ids.insert(c.operative.id()) {
                return Err(MixtureError::DuplicateContender(c.operative.id().to_string()));
            }
        }

        // 1 — the coverage condition. Surface the gap; ask nobody.
        if !contenders.iter().any(|c| c.operative.suited_to(dom_s)) {
            return Ok(Dispatch {
                selected: Vec::new(),
                withheld: contenders
                    .iter()
                    .map(|c| Withheld {
                        operative: c.operative.id().to_string(),
                        reason: WithheldReason::DomainMismatch,
                        score: None,
                    })
                    .collect(),
                considered: contenders.len(),
                gap: Some(dom_s),
            });
        }

        // 2 — abstain on mismatch, and 3 — score the rest.
        let mut scored: Vec<(usize, f64)> = Vec::new();
        let mut withheld = Vec::new();
        for (i, c) in contenders.iter().enumerate() {
            if !c.operative.suited_to(dom_s) {
                withheld.push(Withheld {
                    operative: c.operative.id().to_string(),
                    reason: WithheldReason::DomainMismatch,
                    score: None,
                });
                continue;
            }
            let relevance = subject_relevance(&c.subject_tags, request_tags);
            let estimate = self.rule.collapse(&c.estimate)?;
            let score = self.weights.alpha * relevance
                + self.weights.eta * estimate
                // match(i,s) = 1 here by construction — it is 0 for everything
                // in the branch above.
                + self.weights.zeta;
            scored.push((i, score));
        }

        // Descending score; ties broken by declaration order, so the answer is
        // deterministic.
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .expect("scores are finite")
                .then(a.0.cmp(&b.0))
        });

        let weights = softmax(&scored.iter().map(|(_, s)| *s).collect::<Vec<_>>());
        let take = self.k.min(scored.len());
        let mut selected = Vec::with_capacity(take);
        for (rank, ((i, score), g)) in scored.iter().zip(weights.iter()).enumerate() {
            if rank < take {
                selected.push(Ticket {
                    operative: contenders[*i].operative.id().to_string(),
                    rank,
                    score: *score,
                    gate_weight: *g,
                });
            } else {
                withheld.push(Withheld {
                    operative: contenders[*i].operative.id().to_string(),
                    reason: WithheldReason::BelowTopK,
                    score: Some(*score),
                });
            }
        }

        Ok(Dispatch {
            selected,
            withheld,
            considered: contenders.len(),
            gap: None,
        })
    }
}

/// Numerically stable softmax. **Monotone**, so it does not change which `k`
/// are selected — it normalises the reported weights.
fn softmax(scores: &[f64]) -> Vec<f64> {
    if scores.is_empty() {
        return Vec::new();
    }
    let max = scores.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = scores.iter().map(|s| (s - max).exp()).collect();
    let total: f64 = exps.iter().sum();
    if total == 0.0 {
        return vec![0.0; scores.len()];
    }
    exps.into_iter().map(|e| e / total).collect()
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Error)]
pub enum MixtureError {
    #[error("gate weight {which} is not usable: {value} — a negative weight would mean 'prefer the operative that knows least about this', which is a sign error rather than a tuning choice")]
    BadWeight { which: String, value: String },
    #[error("all three gate weights are zero — that is not a declared policy, it is an absent one")]
    AllWeightsZero,
    #[error("k = 0 — a gate that asks nobody is not sparse, it is off")]
    ZeroK,
    #[error("no contenders to route between")]
    NoContenders,
    #[error("operative '{0}' appears twice among the contenders")]
    DuplicateContender(String),
    #[error("Δû component is not a usable number: {0}")]
    BadEstimate(String),
    #[error("estimate rule declares {weights} weights against a Δû of {estimate} components")]
    EstimateArity { weights: usize, estimate: usize },
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operative::{Objective, Sense, Utility};
    use std::cell::RefCell;

    fn op(id: &str, dim: &str, suited: &[Cynefin]) -> Operative {
        Operative::new(
            id,
            Utility::new()
                .with(Objective::new(dim, dim, Sense::Maximise))
                .unwrap(),
            suited,
        )
        .unwrap()
    }

    fn tags(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    fn gate(k: usize) -> Mixture {
        Mixture::new(
            GateWeights::new(1.0, 1.0, 0.5).unwrap(),
            k,
            EstimateRule::NetGain,
        )
        .unwrap()
    }

    // -- ★★ structural sparsity ---------------------------------------------

    /// ★★ THE PROOF. The expensive path runs `k` times, not `N`, and the
    /// router — not a filter — is why.
    #[test]
    fn the_expensive_path_runs_k_times_not_n() {
        let a = op("a", "x", &[Cynefin::Clear]);
        let b = op("b", "x", &[Cynefin::Clear]);
        let c = op("c", "x", &[Cynefin::Clear]);
        let d = op("d", "x", &[Cynefin::Clear]);
        let contenders = vec![
            Contender::new(&a, &["food"], &[9.0]),
            Contender::new(&b, &["food"], &[5.0]),
            Contender::new(&c, &["food"], &[1.0]),
            Contender::new(&d, &["food"], &[0.5]),
        ];
        let dispatch = gate(2)
            .route(&contenders, &tags(&["food"]), Cynefin::Clear)
            .unwrap();

        let calls = RefCell::new(Vec::new());
        dispatch.run(|t| calls.borrow_mut().push(t.operative().to_string()));

        // 4 contenders, 2 asked, and the other two were never entered.
        assert_eq!(dispatch.considered(), 4);
        assert_eq!(calls.borrow().len(), 2);
        assert_eq!(*calls.borrow(), vec!["a".to_string(), "b".to_string()]);
        assert!((dispatch.sparsity() - 0.5).abs() < 1e-12);
        assert_eq!(dispatch.withheld().len(), 2);
        assert!(dispatch
            .withheld()
            .iter()
            .all(|w| w.reason == WithheldReason::BelowTopK));
    }

    #[test]
    fn softmax_is_monotone_so_it_does_not_change_the_selection() {
        let raw = [3.0, 1.0, 2.0];
        let g = softmax(&raw);
        assert!((g.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // Same argmax ordering.
        assert!(g[0] > g[2] && g[2] > g[1]);
    }

    // -- ★ the two axes ------------------------------------------------------

    #[test]
    fn relevance_generalises_the_hard_test_and_agrees_at_its_boundaries() {
        // Untagged request applies to everyone — `if not proposal_domains: True`.
        assert_eq!(subject_relevance(&tags(&["food"]), &BTreeSet::new()), 1.0);
        // No declared tags — `if not self.domain: False`.
        assert_eq!(subject_relevance(&BTreeSet::new(), &tags(&["food"])), 0.0);
        // Soft in between, where the hard test can only say "yes".
        let partial = subject_relevance(&tags(&["food"]), &tags(&["food", "rent"]));
        assert!((partial - 0.5).abs() < 1e-12);
        // > 0 exactly when the set intersection is non-empty.
        assert_eq!(subject_relevance(&tags(&["a"]), &tags(&["b"])), 0.0);
    }

    #[test]
    fn a_domain_mismatch_abstains_under_its_own_reason_and_is_never_scored() {
        let fit = op("fit", "x", &[Cynefin::Complex]);
        let unfit = op("unfit", "x", &[Cynefin::Complicated]);
        let contenders = vec![
            Contender::new(&fit, &["food"], &[1.0]),
            Contender::new(&unfit, &["food"], &[99.0]),
        ];
        let d = gate(2)
            .route(&contenders, &tags(&["food"]), Cynefin::Complex)
            .unwrap();
        // The high estimate does not buy its way past a regime mismatch.
        assert_eq!(d.tickets().len(), 1);
        assert_eq!(d.tickets()[0].operative(), "fit");
        let w = &d.withheld()[0];
        assert_eq!(w.operative, "unfit");
        assert_eq!(w.reason, WithheldReason::DomainMismatch);
        assert_eq!(w.score, None, "never scored — the regime question came first");
    }

    // -- ★ the coverage condition -------------------------------------------

    #[test]
    fn a_regime_no_operative_is_suited_to_surfaces_the_gap_and_asks_nobody() {
        let a = op("a", "x", &[Cynefin::Clear]);
        let b = op("b", "x", &[Cynefin::Complicated]);
        let contenders = vec![
            Contender::new(&a, &["food"], &[9.0]),
            Contender::new(&b, &["food"], &[9.0]),
        ];
        let d = gate(2)
            .route(&contenders, &tags(&["food"]), Cynefin::Chaotic)
            .unwrap();
        assert!(d.tickets().is_empty(), "nobody asked");
        assert_eq!(d.coverage_gap(), Some(Cynefin::Chaotic));
        assert!(d.describe().contains("gap surfaced"));

        // And the expensive path really does not run.
        let calls = RefCell::new(0);
        d.run(|_| *calls.borrow_mut() += 1);
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn coverage_gaps_measures_the_union_against_all_four() {
        let a = op("a", "x", &[Cynefin::Clear, Cynefin::Complicated]);
        let b = op("b", "x", &[Cynefin::Complex]);
        let contenders = vec![
            Contender::new(&a, &[], &[0.0]),
            Contender::new(&b, &[], &[0.0]),
        ];
        assert_eq!(
            coverage_gaps(&contenders),
            [Cynefin::Chaotic].into_iter().collect()
        );
    }

    #[test]
    fn an_operative_suited_to_nothing_is_named_as_never_reachable() {
        let dead = op("dead", "x", &[]);
        let live = op("live", "x", &[Cynefin::Clear]);
        let contenders = vec![
            Contender::new(&dead, &[], &[0.0]),
            Contender::new(&live, &[], &[0.0]),
        ];
        assert_eq!(gate(1).never_reachable(&contenders), vec!["dead"]);
    }

    // -- the declared gate ---------------------------------------------------

    #[test]
    fn the_estimate_collapse_is_a_named_declared_rule() {
        let v = [5.0, -3.0];
        assert_eq!(EstimateRule::NetGain.collapse(&v).unwrap(), 2.0);
        assert_eq!(EstimateRule::GainsOnly.collapse(&v).unwrap(), 5.0);
        assert_eq!(
            EstimateRule::Weighted(vec![1.0, 2.0]).collapse(&v).unwrap(),
            -1.0
        );
        assert!(matches!(
            EstimateRule::Weighted(vec![1.0]).collapse(&v),
            Err(MixtureError::EstimateArity { .. })
        ));
    }

    #[test]
    fn a_negative_weight_or_a_zero_k_is_refused() {
        assert!(matches!(
            GateWeights::new(-1.0, 1.0, 1.0),
            Err(MixtureError::BadWeight { .. })
        ));
        assert!(matches!(
            GateWeights::new(0.0, 0.0, 0.0),
            Err(MixtureError::AllWeightsZero)
        ));
        assert!(matches!(
            Mixture::new(GateWeights::new(1.0, 1.0, 1.0).unwrap(), 0, EstimateRule::NetGain),
            Err(MixtureError::ZeroK)
        ));
    }

    #[test]
    fn routing_is_deterministic_and_ties_break_by_declaration_order() {
        let a = op("a", "x", &[Cynefin::Clear]);
        let b = op("b", "x", &[Cynefin::Clear]);
        let contenders = vec![
            Contender::new(&a, &["food"], &[1.0]),
            Contender::new(&b, &["food"], &[1.0]),
        ];
        let first = gate(1)
            .route(&contenders, &tags(&["food"]), Cynefin::Clear)
            .unwrap();
        let second = gate(1)
            .route(&contenders, &tags(&["food"]), Cynefin::Clear)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.tickets()[0].operative(), "a");
    }

    #[test]
    fn a_duplicate_contender_is_refused() {
        let a = op("a", "x", &[Cynefin::Clear]);
        let contenders = vec![
            Contender::new(&a, &["food"], &[1.0]),
            Contender::new(&a, &["food"], &[2.0]),
        ];
        assert!(matches!(
            gate(1).route(&contenders, &tags(&["food"]), Cynefin::Clear),
            Err(MixtureError::DuplicateContender(_))
        ));
    }

    #[test]
    fn k_larger_than_the_field_asks_everyone_suited() {
        let a = op("a", "x", &[Cynefin::Clear]);
        let b = op("b", "x", &[Cynefin::Clear]);
        let contenders = vec![
            Contender::new(&a, &["food"], &[1.0]),
            Contender::new(&b, &["food"], &[2.0]),
        ];
        let d = gate(9)
            .route(&contenders, &tags(&["food"]), Cynefin::Clear)
            .unwrap();
        assert_eq!(d.tickets().len(), 2);
        assert!((d.sparsity() - 0.0).abs() < 1e-12);
    }
}
