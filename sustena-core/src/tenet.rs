//! Tenet — the optimiser core: `T: S×A→Δ(S)` and backward induction
//! (Tenet §II, §V + Additions · TEN-1, TEN-4).
//!
//! > This is the inversion.
//!
//! ```text
//! J_t(s) = max_a [ R(s,a) + γ · Σ_s' P(s'|s,a) · J_{t+1}(s') ]
//! ```
//!
//! **The value function is written `J`, not `V`.** §V uses `V`, which collides
//! with `V` = the viable region everywhere else in the series; the Additions
//! rename it, and this module follows the Additions. One letter, one meaning.
//!
//! ## `T: S × A → Δ(S)` — a distribution, not a state (§II)
//!
//! > The codomain is **Δ(S)** — a probability distribution, not a single state.
//! > Actions shift probabilities.
//!
//! The R1 forward simulator is the **deterministic** case, and it stays exactly
//! that: a point mass. [`Distribution::certain`] builds one, so the existing
//! behaviour is a *special case* of this model rather than a parallel mechanism.
//!
//! **A distribution that does not sum to 1 is not a distribution.**
//! [`Distribution::new`] refuses one rather than normalising it — normalising
//! would invent probabilities nobody declared, in the one place the whole
//! optimisation reads as evidence.
//!
//! ## Backward induction (§V)
//!
//! > The key insight — computing from the terminal state backward — is that the
//! > value of future states must be known before the value of present states can
//! > be computed. So you sweep from the end.
//!
//! The terminal condition is the **inversion point** `IP: S → {0,1}` (§III), and
//! [`InversionPoint`] can be a declared predicate, **the viability kernel**, or
//! both. The kernel reading is the interesting one: *"worth reaching"* becomes
//! *"from here you can keep going"*, which is what [`crate::kernel`] computes.
//!
//! `π` is a **complete policy** — for every state, at every time, the optimal
//! action. *Built from the future. Pointing toward it.*
//!
//! ## The CTL-12 join: Lyapunov descent is the H = 1 case
//!
//! The Controller's `is_stable_intervention` picks the move that most reduces
//! `W = d(s,V)`. Set `R(s,a) = W(s) − E[W(s')]` — the expected Lyapunov
//! improvement, which is what [`rewards_from_region`] builds — and the `H = 1`
//! sweep's `argmax J` **is** that greedy choice.
//!
//! So the Controller and Tenet share **one** optimal-control core rather than
//! two mechanisms that happen to agree: Lyapunov descent is backward induction's
//! greedy one-step special case, and a vector checks that they pick the same
//! action rather than asserting it. What the longer horizon buys is the ability
//! to accept a *worse* first step for a better end — which is exactly the case
//! the greedy rule cannot see.
//!
//! ## Honest limits, encoded the same way the kernel's were
//!
//! - **Enumerated space.** The sweep runs over states the caller enumerated. It
//!   is exact for that space and claims nothing about states nobody listed.
//! - **Horizon-limited.** A policy computed to `H` is meaningful *over `H`*, and
//!   [`Plan::horizon`] carries that rather than leaving it implicit. Past the
//!   chaos horizon `H*` (OPV-13) a longer sweep is compute spent on noise, so a
//!   plan states the window it is about rather than implying forever.
//! - **`γ` , `R` and `H` are declared** spec fields, like the region weights and
//!   the CUSUM parameters. A discount rate is a statement about how much a
//!   household values later over sooner, and it belongs where it can be argued
//!   with.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use thiserror::Error;

use crate::kernel::{Kernel, Space};
use crate::region::Region;

/// One branch of a transition: where it lands, and with what probability.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub to: String,
    pub p: f64,
}

/// **Δ(S)** — a probability distribution over next states.
///
/// Constructible only through [`Distribution::new`] or
/// [`Distribution::certain`], both of which enforce that it *is* a distribution.
#[derive(Debug, Clone, PartialEq)]
pub struct Distribution {
    outcomes: Vec<Outcome>,
}

impl Distribution {
    /// Probabilities must be finite, non-negative, and sum to 1.
    ///
    /// **Refused rather than normalised.** Normalising would invent
    /// probabilities nobody declared, and this is the one input the whole
    /// optimisation reads as evidence about the world.
    pub fn new(outcomes: Vec<Outcome>) -> Result<Self, TenetError> {
        if outcomes.is_empty() {
            return Err(TenetError::EmptyDistribution);
        }
        let mut total = 0.0;
        for o in &outcomes {
            if !o.p.is_finite() || o.p < 0.0 {
                return Err(TenetError::BadProbability { to: o.to.clone(), p: o.p.to_string() });
            }
            total += o.p;
        }
        if (total - 1.0).abs() > 1e-9 {
            return Err(TenetError::NotADistribution { total });
        }
        Ok(Self { outcomes })
    }

    /// The **deterministic** case — a point mass.
    ///
    /// This is what the R1 forward simulator does, expressed in the general
    /// model rather than beside it.
    pub fn certain(to: &str) -> Self {
        Self { outcomes: vec![Outcome { to: to.to_string(), p: 1.0 }] }
    }

    pub fn outcomes(&self) -> &[Outcome] {
        &self.outcomes
    }

    pub fn is_deterministic(&self) -> bool {
        self.outcomes.len() == 1
    }

    /// `Σ_s' P(s'|s,a) · f(s')` — the expectation of a function over next states.
    pub fn expectation<F: Fn(&str) -> f64>(&self, f: F) -> f64 {
        self.outcomes.iter().map(|o| o.p * f(&o.to)).sum()
    }
}

/// `T : S × A → Δ(S)`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TransitionModel {
    table: BTreeMap<(String, String), Distribution>,
}

impl TransitionModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn declaring(mut self, state: &str, action: &str, dist: Distribution) -> Self {
        self.table.insert((state.to_string(), action.to_string()), dist);
        self
    }

    /// The deterministic convenience, so the common case stays readable.
    pub fn moving(self, state: &str, action: &str, to: &str) -> Self {
        self.declaring(state, action, Distribution::certain(to))
    }

    pub fn get(&self, state: &str, action: &str) -> Option<&Distribution> {
        self.table.get(&(state.to_string(), action.to_string()))
    }

    /// Every action declared for a state — its admissible set.
    ///
    /// A state with none is terminal by absence rather than by declaration, and
    /// the sweep treats it as such: it keeps whatever value it already has.
    pub fn actions_at(&self, state: &str) -> Vec<String> {
        self.table
            .keys()
            .filter(|(s, _)| s == state)
            .map(|(_, a)| a.clone())
            .collect()
    }

    pub fn actions(&self) -> BTreeSet<String> {
        self.table.keys().map(|(_, a)| a.clone()).collect()
    }

    /// Outcomes pointing at states nobody enumerated.
    ///
    /// Reported rather than tolerated: a sweep over a space with dangling edges
    /// silently treats those branches as worth zero, which is a claim about the
    /// world nobody made.
    pub fn dangling(&self, space: &BTreeSet<String>) -> Vec<String> {
        let mut out: Vec<String> = self
            .table
            .values()
            .flat_map(|d| d.outcomes.iter())
            .filter(|o| !space.contains(&o.to))
            .map(|o| o.to.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

/// `IP : S → {0,1}` — the terminal condition (§III).
#[derive(Debug, Clone)]
pub enum InversionPoint<'a> {
    /// Declared predicate expressions over state, conjoined.
    Predicate(Vec<String>),
    /// **The viability kernel.** *"Worth reaching"* read as *"from here you can
    /// keep going"* — the set [`crate::kernel::viability_kernel`] computes.
    Kernel(&'a Kernel),
    /// Both: in the kernel **and** satisfying the declared conditions.
    KernelAnd(&'a Kernel, Vec<String>),
}

impl InversionPoint<'_> {
    fn holds(&self, id: &str, state: &Value) -> Result<bool, TenetError> {
        let empty = Map::new();
        let preds = |exprs: &[String]| -> Result<bool, TenetError> {
            for e in exprs {
                let (ok, _) = crate::predicate::check(e, state, &empty)
                    .map_err(|err| TenetError::BadPredicate {
                        expr: e.clone(),
                        detail: err.to_string(),
                    })?;
                if !ok {
                    return Ok(false);
                }
            }
            Ok(true)
        };
        match self {
            InversionPoint::Predicate(exprs) => preds(exprs),
            InversionPoint::Kernel(k) => Ok(k.contains(id)),
            InversionPoint::KernelAnd(k, exprs) => Ok(k.contains(id) && preds(exprs)?),
        }
    }
}

/// The declared parameters of a sweep.
///
/// `γ`, `R` and `H` are spec fields for the same reason the region weights are:
/// a discount rate is a statement about how much a household values later over
/// sooner, and it belongs where it can be argued with.
#[derive(Debug, Clone, PartialEq)]
pub struct BellmanSpec {
    pub horizon: usize,
    /// `γ ∈ [0,1]`.
    pub gamma: f64,
    /// `R_terminal` — the value of standing at the inversion point.
    pub terminal_reward: f64,
    /// `R(s,a)`. Absent pairs contribute `0.0`.
    pub rewards: BTreeMap<(String, String), f64>,
}

impl BellmanSpec {
    pub fn new(horizon: usize, gamma: f64, terminal_reward: f64) -> Self {
        Self { horizon, gamma, terminal_reward, rewards: BTreeMap::new() }
    }

    pub fn rewarding(mut self, state: &str, action: &str, r: f64) -> Self {
        self.rewards.insert((state.to_string(), action.to_string()), r);
        self
    }

    pub fn reward(&self, state: &str, action: &str) -> f64 {
        self.rewards.get(&(state.to_string(), action.to_string())).copied().unwrap_or(0.0)
    }

    pub fn typecheck(&self) -> Result<(), TenetError> {
        if !self.gamma.is_finite() || !(0.0..=1.0).contains(&self.gamma) {
            return Err(TenetError::BadGamma(self.gamma.to_string()));
        }
        if !self.terminal_reward.is_finite() {
            return Err(TenetError::BadReward {
                what: "terminal_reward".into(),
                r: self.terminal_reward.to_string(),
            });
        }
        for ((s, a), r) in &self.rewards {
            if !r.is_finite() {
                return Err(TenetError::BadReward { what: format!("R({s},{a})"), r: r.to_string() });
            }
        }
        if self.horizon == 0 {
            return Err(TenetError::ZeroHorizon);
        }
        Ok(())
    }
}

/// `J` and `π` — the value function and the complete policy.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// `J[t][s]`, for `t ∈ 0..=H`.
    j: Vec<BTreeMap<String, f64>>,
    /// `π[t][s]`, for `t ∈ 0..H`. Absent when a state has no admissible action.
    policy: Vec<BTreeMap<String, String>>,
    horizon: usize,
}

impl Plan {
    /// `J_t(s)`.
    pub fn value(&self, t: usize, state: &str) -> Option<f64> {
        self.j.get(t)?.get(state).copied()
    }

    /// `π_t(s)` — the optimal action, *built from the future, pointing toward
    /// it*.
    pub fn action(&self, t: usize, state: &str) -> Option<&str> {
        self.policy.get(t)?.get(state).map(|s| s.as_str())
    }

    /// **The window this plan is about.**
    ///
    /// A policy computed to `H` is meaningful over `H` and no further. Carried
    /// rather than implied, because past the chaos horizon `H*` (OPV-13) a
    /// longer sweep is compute spent on noise — and a plan that does not state
    /// its window invites being read as forever.
    pub fn horizon(&self) -> usize {
        self.horizon
    }

    /// Follow `π` forward from a state, taking the most likely branch at each
    /// step. A reading aid for a person, not the policy itself.
    pub fn most_likely_path(&self, model: &TransitionModel, from: &str) -> Vec<String> {
        let mut path = vec![from.to_string()];
        let mut here = from.to_string();
        for t in 0..self.horizon {
            let Some(a) = self.action(t, &here) else { break };
            let Some(d) = model.get(&here, a) else { break };
            let Some(best) = d
                .outcomes
                .iter()
                .max_by(|x, y| x.p.partial_cmp(&y.p).unwrap_or(std::cmp::Ordering::Equal))
            else {
                break;
            };
            here = best.to.clone();
            path.push(here.clone());
        }
        path
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum TenetError {
    #[error("a distribution needs at least one outcome")]
    EmptyDistribution,
    #[error("P(→{to}) must be finite and non-negative; got {p}")]
    BadProbability { to: String, p: String },
    #[error("probabilities sum to {total}, not 1 — a distribution that does not sum to 1 is not a distribution, and normalising it would invent probabilities nobody declared")]
    NotADistribution { total: f64 },
    #[error("γ must be in [0,1]; got {0}")]
    BadGamma(String),
    #[error("{what} must be finite; got {r}")]
    BadReward { what: String, r: String },
    #[error("a horizon of 0 has nothing to sweep")]
    ZeroHorizon,
    #[error("inversion-point predicate '{expr}' could not be parsed: {detail}")]
    BadPredicate { expr: String, detail: String },
    #[error("the transition model points at {0:?}, which is not in the enumerated space")]
    DanglingOutcomes(Vec<String>),
}

/// **Backward induction** (§V) — sweep from the end.
///
/// ```text
/// J[H][s] = R_terminal if IP(s) else 0
/// J[t][s] = max_a [ R(s,a) + γ · Σ_s' P(s'|s,a) · J[t+1][s'] ]
/// π[t][s] = argmax of the same expression
/// ```
///
/// Refuses a model with outcomes outside the enumerated space: treating those
/// branches as worth zero would be a claim about the world nobody made.
pub fn backward_induct(
    space: &Space,
    model: &TransitionModel,
    ip: &InversionPoint,
    spec: &BellmanSpec,
) -> Result<Plan, TenetError> {
    spec.typecheck()?;
    let ids: BTreeSet<String> = space.keys().cloned().collect();
    let dangling = model.dangling(&ids);
    if !dangling.is_empty() {
        return Err(TenetError::DanglingOutcomes(dangling));
    }

    // ── terminal values: the inversion point IS the terminal condition ──────
    let mut j: Vec<BTreeMap<String, f64>> = vec![BTreeMap::new(); spec.horizon + 1];
    for (id, state) in space {
        let v = if ip.holds(id, state)? { spec.terminal_reward } else { 0.0 };
        j[spec.horizon].insert(id.clone(), v);
    }

    let mut policy: Vec<BTreeMap<String, String>> = vec![BTreeMap::new(); spec.horizon];

    // ── sweep backward: the future must be known before the present ─────────
    for t in (0..spec.horizon).rev() {
        for id in &ids {
            let mut best: Option<(f64, String)> = None;
            for a in model.actions_at(id) {
                let d = model.get(id, &a).expect("declared above");
                let expected = d.expectation(|s| j[t + 1].get(s).copied().unwrap_or(0.0));
                let q = spec.reward(id, &a) + spec.gamma * expected;
                if best.as_ref().is_none_or(|(bq, _)| q > *bq) {
                    best = Some((q, a));
                }
            }
            match best {
                Some((q, a)) => {
                    j[t].insert(id.clone(), q);
                    policy[t].insert(id.clone(), a);
                }
                // No admissible action: terminal by absence. It keeps the value
                // it already had rather than being scored as worthless, which
                // would be a different claim.
                None => {
                    let carried = j[t + 1].get(id).copied().unwrap_or(0.0);
                    j[t].insert(id.clone(), carried);
                }
            }
        }
    }

    Ok(Plan { j, policy, horizon: spec.horizon })
}

/// `R(s,a) = W(s) − E[W(s')]` — the **expected Lyapunov improvement**.
///
/// This is the CTL-12 join made concrete. With these rewards, the `H = 1` sweep's
/// `argmax J` is exactly the Controller's greedy `argmin W(o(s))`, so
/// `is_stable_intervention` is backward induction's one-step special case rather
/// than a parallel mechanism.
pub fn rewards_from_region(
    region: &Region,
    space: &Space,
    model: &TransitionModel,
    mut spec: BellmanSpec,
) -> Result<BellmanSpec, TenetError> {
    let w = |id: &str| -> f64 {
        space
            .get(id)
            .and_then(|s| region.distance(s).ok())
            .map(|d| d.weighted)
            .unwrap_or(0.0)
    };
    for id in space.keys() {
        for a in model.actions_at(id) {
            let d = model.get(id, &a).expect("declared");
            let improvement = w(id) - d.expectation(w);
            spec = spec.rewarding(id, &a, improvement);
        }
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Interval;
    use serde_json::json;

    /// The proof-of-value space: a quick path that dead-ends, and a slower one
    /// that reaches the goal.
    fn space() -> Space {
        [
            ("start", 100.0),
            ("quick", 120.0),
            ("trap", 121.0),
            ("slow", 60.0),
            ("goal", 500.0),
        ]
        .into_iter()
        .map(|(id, v)| (id.to_string(), json!({ "balance": v })))
        .collect()
    }

    fn model() -> TransitionModel {
        TransitionModel::new()
            .moving("start", "dash", "quick")
            .moving("start", "build", "slow")
            .moving("quick", "coast", "trap")
            .moving("trap", "coast", "trap")
            .moving("slow", "finish", "goal")
            .moving("goal", "hold", "goal")
    }

    fn at_goal() -> InversionPoint<'static> {
        InversionPoint::Predicate(vec!["balance >= 500".into()])
    }

    /// A minimum operating float of 100 — so `slow` (60) genuinely dips BELOW
    /// the viable region, which is what makes it a worse first step rather than
    /// merely a smaller number.
    fn region() -> Region {
        Region::new()
            .bounding(Interval::at_least("balance", 100.0))
            .weighing("balance", 1.0)
    }

    // ── TEN-1: Δ(S), not a state (§II) ──────────────────────────────────────

    #[test]
    fn a_distribution_must_sum_to_one() {
        let bad = Distribution::new(vec![
            Outcome { to: "a".into(), p: 0.5 },
            Outcome { to: "b".into(), p: 0.2 },
        ]);
        assert!(matches!(bad, Err(TenetError::NotADistribution { .. })),
                "normalising would invent probabilities nobody declared");
    }

    #[test]
    fn a_negative_probability_is_refused() {
        let bad = Distribution::new(vec![
            Outcome { to: "a".into(), p: 1.5 },
            Outcome { to: "b".into(), p: -0.5 },
        ]);
        assert!(matches!(bad, Err(TenetError::BadProbability { .. })));
    }

    #[test]
    fn the_deterministic_case_is_a_point_mass() {
        // The R1 forward simulator, expressed IN this model rather than beside
        // it — so the general case generalises rather than replaces.
        let d = Distribution::certain("b");
        assert!(d.is_deterministic());
        assert_eq!(d.expectation(|s| if s == "b" { 7.0 } else { 0.0 }), 7.0);
    }

    #[test]
    fn a_genuine_distribution_spreads_the_expectation() {
        let d = Distribution::new(vec![
            Outcome { to: "a".into(), p: 0.25 },
            Outcome { to: "b".into(), p: 0.75 },
        ])
        .unwrap();
        assert!(!d.is_deterministic());
        assert_eq!(d.expectation(|s| if s == "a" { 100.0 } else { 0.0 }), 25.0);
    }

    #[test]
    fn outcomes_outside_the_enumerated_space_are_refused() {
        // Treating a dangling branch as worth zero is a claim about the world
        // nobody made.
        let m = model().moving("goal", "vanish", "elsewhere");
        let out = backward_induct(&space(), &m, &at_goal(), &BellmanSpec::new(3, 0.9, 100.0));
        assert!(matches!(out, Err(TenetError::DanglingOutcomes(_))));
    }

    // ── TEN-4: the sweep (§V) ───────────────────────────────────────────────

    #[test]
    fn the_terminal_condition_is_the_inversion_point() {
        let spec = BellmanSpec::new(3, 0.9, 100.0);
        let plan = backward_induct(&space(), &model(), &at_goal(), &spec).unwrap();
        assert_eq!(plan.value(3, "goal"), Some(100.0), "IP(goal) = 1");
        assert_eq!(plan.value(3, "trap"), Some(0.0), "and nowhere else");
        assert_eq!(plan.value(3, "start"), Some(0.0));
    }

    #[test]
    fn value_propagates_backward_discounted() {
        // goal is 1 step from slow, 2 from start. γ = 0.9, R_terminal = 100.
        let spec = BellmanSpec::new(3, 0.9, 100.0);
        let plan = backward_induct(&space(), &model(), &at_goal(), &spec).unwrap();
        assert_eq!(plan.value(2, "goal"), Some(90.0), "one discount from terminal");
        assert!((plan.value(1, "slow").unwrap() - 81.0).abs() < 1e-9, "two");
        assert!((plan.value(0, "start").unwrap() - 72.9).abs() < 1e-9, "three");
    }

    #[test]
    fn the_policy_is_complete_for_every_state_at_every_time() {
        let spec = BellmanSpec::new(3, 0.9, 100.0);
        let plan = backward_induct(&space(), &model(), &at_goal(), &spec).unwrap();
        for t in 0..3 {
            for s in ["start", "quick", "trap", "slow", "goal"] {
                assert!(plan.action(t, s).is_some(), "π[{t}][{s}] must exist");
            }
        }
    }

    /// **THE PROOF OF VALUE** — the case a greedy one-step rule cannot see.
    #[test]
    fn backward_induction_takes_a_worse_first_step_for_a_better_end() {
        // `dash` improves the balance now (100 → 120) and dead-ends at `trap`.
        // `build` makes things WORSE now (100 → 60) and reaches the goal.
        let (sp, m) = (space(), model());

        // Greedy, one step, rewarded by Lyapunov improvement: prefers `dash`.
        let greedy = rewards_from_region(&region(), &sp, &m, BellmanSpec::new(1, 0.9, 100.0)).unwrap();
        let short = backward_induct(&sp, &m, &at_goal(), &greedy).unwrap();
        assert_eq!(short.action(0, "start"), Some("dash"),
                   "the one-step rule takes the immediate improvement");

        // The full sweep sees the dead end and pays the worse first step.
        let far = rewards_from_region(&region(), &sp, &m, BellmanSpec::new(3, 0.9, 500.0)).unwrap();
        let plan = backward_induct(&sp, &m, &at_goal(), &far).unwrap();
        assert_eq!(plan.action(0, "start"), Some("build"),
                   "backward induction finds the policy the greedy step misses");
        assert_eq!(plan.most_likely_path(&m, "start"), ["start", "slow", "goal", "goal"]);
    }

    // ── the CTL-12 join ─────────────────────────────────────────────────────

    #[test]
    fn at_h_equals_one_bellman_reproduces_the_lyapunov_choice() {
        // The join, checked rather than asserted: with R = expected Lyapunov
        // improvement, argmax J at H = 1 IS argmin W(o(s)).
        let (sp, m) = (space(), model());
        let r = region();
        let spec = rewards_from_region(&r, &sp, &m, BellmanSpec::new(1, 0.9, 0.0)).unwrap();
        let plan = backward_induct(&sp, &m, &at_goal(), &spec).unwrap();

        // What the Controller's greedy rule would pick from `start`.
        let w = |id: &str| r.distance(&sp[id]).unwrap().weighted;
        let greedy = m
            .actions_at("start")
            .into_iter()
            .min_by(|a, b| {
                let wa = m.get("start", a).unwrap().expectation(w);
                let wb = m.get("start", b).unwrap().expectation(w);
                wa.partial_cmp(&wb).unwrap()
            })
            .unwrap();

        assert_eq!(plan.action(0, "start"), Some(greedy.as_str()),
                   "one optimal-control core, not two mechanisms that agree");
    }

    #[test]
    fn a_longer_horizon_can_change_the_first_move() {
        // What the extra horizon actually buys — and it is the whole reason for
        // the module.
        let (sp, m) = (space(), model());
        let one = rewards_from_region(&region(), &sp, &m, BellmanSpec::new(1, 0.9, 500.0)).unwrap();
        let three = rewards_from_region(&region(), &sp, &m, BellmanSpec::new(3, 0.9, 500.0)).unwrap();
        let a = backward_induct(&sp, &m, &at_goal(), &one).unwrap();
        let b = backward_induct(&sp, &m, &at_goal(), &three).unwrap();
        assert_ne!(a.action(0, "start"), b.action(0, "start"));
    }

    // ── the kernel as the inversion point ───────────────────────────────────

    #[test]
    fn the_viability_kernel_can_be_the_terminal_condition() {
        use crate::compose::Change;
        use crate::kernel::{viability_kernel, Move};

        // "Worth reaching" read as "from here you can keep going".
        let sp: Space = [("a", 10.0), ("b", 20.0), ("dead", 30.0)]
            .into_iter()
            .map(|(id, v)| (id.to_string(), json!({ "balance": v })))
            .collect();
        let moves = vec![
            Move::new("up").guarded_by("balance <= 10").changing("balance", Change::ShiftBy(10.0)),
            Move::new("down").guarded_by("balance >= 20").guarded_by("balance <= 20")
                .changing("balance", Change::ShiftBy(-10.0)),
        ];
        // Its own region: these balances are small, and the point here is the
        // kernel as a terminal condition rather than the Lyapunov term.
        let small = Region::new()
            .bounding(Interval::at_least("balance", 0.0))
            .weighing("balance", 1.0);
        let k = viability_kernel(&small, &sp, &moves).unwrap();
        assert!(k.contains("a") && k.contains("b") && !k.contains("dead"));

        let m = TransitionModel::new()
            .moving("a", "up", "b").moving("b", "down", "a").moving("dead", "stay", "dead");
        let spec = BellmanSpec::new(2, 1.0, 50.0);
        let plan = backward_induct(&sp, &m, &InversionPoint::Kernel(&k), &spec).unwrap();

        assert_eq!(plan.value(2, "a"), Some(50.0), "in the kernel: worth reaching");
        assert_eq!(plan.value(2, "dead"), Some(0.0), "in V, but nothing to reach");
    }

    // ── declared parameters + honest limits ─────────────────────────────────

    #[test]
    fn the_parameters_are_checked_at_authoring_time() {
        assert!(BellmanSpec::new(3, 0.9, 100.0).typecheck().is_ok());
        assert!(matches!(BellmanSpec::new(3, 1.5, 100.0).typecheck(), Err(TenetError::BadGamma(_))));
        assert!(matches!(BellmanSpec::new(0, 0.9, 100.0).typecheck(), Err(TenetError::ZeroHorizon)));
        assert!(matches!(
            BellmanSpec::new(3, 0.9, f64::NAN).typecheck(),
            Err(TenetError::BadReward { .. })
        ));
    }

    #[test]
    fn a_plan_states_the_window_it_is_about() {
        // Past the chaos horizon H* a longer sweep is compute spent on noise, so
        // a plan that does not carry its horizon invites being read as forever.
        let plan = backward_induct(&space(), &model(), &at_goal(), &BellmanSpec::new(3, 0.9, 100.0)).unwrap();
        assert_eq!(plan.horizon(), 3);
        assert_eq!(plan.action(3, "start"), None, "there is no policy past the horizon");
    }

    #[test]
    fn gamma_zero_is_pure_myopia() {
        // γ = 0 discards the future entirely, which is the degenerate case worth
        // pinning: it must reduce to picking the best immediate reward.
        let (sp, m) = (space(), model());
        let spec = rewards_from_region(&region(), &sp, &m, BellmanSpec::new(3, 0.0, 500.0)).unwrap();
        let plan = backward_induct(&sp, &m, &at_goal(), &spec).unwrap();
        assert_eq!(plan.action(0, "start"), Some("dash"), "no discounting of the future — no future");
    }

    #[test]
    fn a_state_with_no_admissible_action_carries_its_value_rather_than_scoring_zero() {
        // Terminal by absence is a different claim from worthless.
        let sp: Space = [("only", 500.0)].into_iter()
            .map(|(id, v)| (id.to_string(), json!({ "balance": v }))).collect();
        let plan = backward_induct(&sp, &TransitionModel::new(), &at_goal(),
                                   &BellmanSpec::new(2, 0.9, 100.0)).unwrap();
        assert_eq!(plan.value(2, "only"), Some(100.0));
        assert_eq!(plan.value(0, "only"), Some(100.0), "carried, not zeroed");
        assert_eq!(plan.action(0, "only"), None, "and no action is invented for it");
    }
}
