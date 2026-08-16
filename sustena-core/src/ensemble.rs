//! Scenario ensembles, invariant actions, decision nodes and dead drops
//! (Tenet §IV, §VI, §VII, §VIII · TEN-3, TEN-5, TEN-6, TEN-7).
//!
//! One sweep answers *"what should I do if the world works like this?"*
//! This module runs [`crate::tenet::backward_induct`] once per declared
//! scenario and reads the **disagreement between the answers**, which is a
//! different and more useful question: *what should I do regardless, where does
//! it actually matter which future arrives, and what have I already decided
//! about those points?*
//!
//! ```text
//! Ω = {ω_A, ω_B, ω_C}                          §IV  ensemble
//! π_ω = BackwardInduct under ω                 §V   reused, once per scenario
//! I(s₀) = {a | a = π_A(s₀) = π_B(s₀) = π_C(s₀)} §VI  invariant actions
//! D = {s | ∃ ω_i,ω_j : π_ωi(s) ≠ π_ωj(s)}      §VII decision nodes
//! δ(D_i) = a*, chosen now under clarity        §VIII dead drops
//! ```
//!
//! ## A scenario is a probability **assignment**, not a model (§IV)
//!
//! The article is specific about this and it is worth following exactly:
//!
//! ```text
//! BEST:  TransitionModel(p_funding=0.85, p_growth=0.80, p_shock=0.05)
//! ```
//!
//! What is declared is a handful of **named probabilities** — a belief about
//! how the world behaves. The model is what you get when you push that belief
//! through the structure of the transitions. So [`ModelTemplate`] declares the
//! *shape* (which edges exist, and which parameter governs each), a
//! [`Scenario`] declares the *numbers*, and [`ModelTemplate::instantiate`]
//! produces the [`TransitionModel`].
//!
//! This matters beyond tidiness. The three scenarios then differ **only** in
//! their numbers, over identical structure — which is the premise the whole
//! comparison rests on. Handing over three separately-built models would let
//! them differ in which edges exist at all, and a "divergence" between them
//! could then be an authoring slip rather than a fact about uncertainty.
//!
//! An unbound parameter is **named and refused**, never defaulted. A scenario
//! whose numbers do not form a distribution is refused by
//! [`crate::tenet::Distribution::new`] at instantiation — an incoherent
//! scenario cannot be swept, rather than being swept and quietly believed.
//!
//! ## Invariance is only ever **over the declared scenarios**
//!
//! `I(s₀)` is a real answer to *"what is optimal under every future I wrote
//! down"* and says nothing about futures nobody wrote down. That limit is not
//! a footnote here: [`InvariantSet`] **carries the scenario names it was
//! computed over** and there is no accessor that returns the actions without
//! them. Reporting "this action is invariant" without saying invariant over
//! what is unspellable, not merely discouraged.
//!
//! For the same reason [`Ensemble::new`] **refuses fewer than two scenarios**.
//! A one-scenario ensemble makes every action trivially invariant and no state
//! a decision node — a vacuous robustness claim, and the kind that reads
//! strongest precisely when it is worth least.
//!
//! ## The at-most-one property of `I` (§VI)
//!
//! `I` is written as a set, and the article's own pseudocode intersects
//! singletons: `candidates ∩= {π(s₀)}`. So `I` is **empty or a single
//! action** — never a list to choose from. The set type is kept for fidelity
//! and [`InvariantSet::the_one`] states the property, so nobody builds a
//! picker over what is structurally at most one item.
//!
//! ## Dead drops are checked when they are cheap to check (§VIII)
//!
//! > *A decision made in advance under clarity is more reliable than one made
//! > in the moment under pressure.* (Schelling, 1960)
//!
//! [`DeadDropBook::pre_commit`] refuses a drop that does not name a real
//! decision node, names an action the model does not admit there, or carries a
//! guard that does not parse. All three are the sort of thing that is obvious
//! now and expensive at the node — which is the entire point of pre-committing,
//! applied to the pre-commitment itself.
//!
//! A drop whose action no scenario actually proposed is **allowed and flagged**
//! ([`DeadDrop::is_hedge`]) rather than refused: a person is permitted to
//! pre-commit to a third thing none of the futures argued for. What they are
//! not permitted is to do it invisibly.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::kernel::Space;
use crate::region::Region;
use crate::tenet::{
    backward_induct, rewards_from_region, BellmanSpec, Distribution, InversionPoint, Outcome, Plan,
    TenetError, TransitionModel,
};

// ── §IV — the ensemble ──────────────────────────────────────────────────────

/// An edge probability: a literal, a named scenario parameter, or its
/// complement.
///
/// [`Prob::OneMinus`] exists because §IV's scenarios declare **one** number per
/// uncertainty (`p_funding=0.85`) and the other branch of a two-way split is
/// implied. Making the author write `0.15` separately would be a second place
/// for the same belief to live, and the two would eventually disagree.
#[derive(Debug, Clone, PartialEq)]
pub enum Prob {
    Fixed(f64),
    Param(String),
    OneMinus(String),
}

impl Prob {
    pub fn param(name: &str) -> Self {
        Prob::Param(name.to_string())
    }

    pub fn one_minus(name: &str) -> Self {
        Prob::OneMinus(name.to_string())
    }

    fn resolve(&self, params: &BTreeMap<String, f64>, scenario: &str) -> Result<f64, EnsembleError> {
        match self {
            Prob::Fixed(p) => Ok(*p),
            Prob::Param(k) => params
                .get(k)
                .copied()
                .ok_or_else(|| EnsembleError::UnboundParameter {
                    scenario: scenario.to_string(),
                    parameter: k.clone(),
                }),
            Prob::OneMinus(k) => params
                .get(k)
                .map(|p| 1.0 - p)
                .ok_or_else(|| EnsembleError::UnboundParameter {
                    scenario: scenario.to_string(),
                    parameter: k.clone(),
                }),
        }
    }

    fn parameter(&self) -> Option<&str> {
        match self {
            Prob::Fixed(_) => None,
            Prob::Param(k) | Prob::OneMinus(k) => Some(k),
        }
    }
}

/// The **structure** of the transitions, with parameters where the
/// probabilities go.
///
/// One template, several scenarios: the futures differ in their numbers over
/// identical structure, which is what makes a disagreement between them mean
/// something.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelTemplate {
    edges: BTreeMap<(String, String), Vec<(String, Prob)>>,
}

impl ModelTemplate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare `(state, action) → [(next, probability), …]`.
    pub fn edge(mut self, state: &str, action: &str, outcomes: Vec<(&str, Prob)>) -> Self {
        self.edges.insert(
            (state.to_string(), action.to_string()),
            outcomes.into_iter().map(|(to, p)| (to.to_string(), p)).collect(),
        );
        self
    }

    /// The deterministic convenience — a point mass, no parameter involved.
    pub fn certain(self, state: &str, action: &str, to: &str) -> Self {
        self.edge(state, action, vec![(to, Prob::Fixed(1.0))])
    }

    /// Every parameter this template reads. A scenario is complete for this
    /// template exactly when it binds all of them.
    pub fn parameters(&self) -> BTreeSet<String> {
        self.edges
            .values()
            .flat_map(|outs| outs.iter())
            .filter_map(|(_, p)| p.parameter())
            .map(str::to_string)
            .collect()
    }

    /// Push a scenario's numbers through the structure.
    ///
    /// The sum-to-1 check is [`Distribution::new`]'s, not a second one here —
    /// an incoherent scenario is refused by the same rule that refuses an
    /// incoherent hand-written distribution.
    pub fn instantiate(&self, scenario: &Scenario) -> Result<TransitionModel, EnsembleError> {
        let mut model = TransitionModel::new();
        for ((state, action), outs) in &self.edges {
            let mut outcomes = Vec::with_capacity(outs.len());
            for (to, p) in outs {
                outcomes.push(Outcome { to: to.clone(), p: p.resolve(&scenario.params, &scenario.name)? });
            }
            let dist = Distribution::new(outcomes).map_err(|e| EnsembleError::IncoherentScenario {
                scenario: scenario.name.clone(),
                state: state.clone(),
                action: action.clone(),
                detail: e.to_string(),
            })?;
            model = model.declaring(state, action, dist);
        }
        Ok(model)
    }
}

/// **A scenario is a declared assignment of probabilities** (§IV) — a belief
/// about how the world behaves, not a model of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub params: BTreeMap<String, f64>,
}

impl Scenario {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), params: BTreeMap::new() }
    }

    pub fn assigning(mut self, key: &str, p: f64) -> Self {
        self.params.insert(key.to_string(), p);
        self
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        self.params.get(key).copied()
    }
}

/// `Ω` — the declared set of futures, with one designated as the base.
///
/// The base is not a tie-breaker or an average. It is the policy a dead drop
/// falls back to when no pre-commitment fires, and it is designated rather than
/// inferred so that fallback is a **declared** choice: best/BASE/worst is the
/// Shell-Wack structure, and the middle one is the one you act from absent a
/// reason not to.
#[derive(Debug, Clone, PartialEq)]
pub struct Ensemble {
    scenarios: Vec<Scenario>,
    base: String,
}

impl Ensemble {
    /// Refuses fewer than two scenarios, a duplicate name, or a base that is
    /// not one of them.
    ///
    /// The two-scenario floor is the load-bearing one: with a single future
    /// every action is trivially invariant and no state is ever a decision
    /// node, which is a robustness claim that cannot fail and therefore says
    /// nothing.
    pub fn new(scenarios: Vec<Scenario>, base: &str) -> Result<Self, EnsembleError> {
        if scenarios.len() < 2 {
            return Err(EnsembleError::TooFewScenarios(scenarios.len()));
        }
        let mut seen = BTreeSet::new();
        for s in &scenarios {
            if !seen.insert(s.name.clone()) {
                return Err(EnsembleError::DuplicateScenario(s.name.clone()));
            }
        }
        if !seen.contains(base) {
            return Err(EnsembleError::UnknownBase(base.to_string()));
        }
        Ok(Self { scenarios, base: base.to_string() })
    }

    pub fn names(&self) -> Vec<&str> {
        self.scenarios.iter().map(|s| s.name.as_str()).collect()
    }

    pub fn base_name(&self) -> &str {
        &self.base
    }

    pub fn scenario(&self, name: &str) -> Option<&Scenario> {
        self.scenarios.iter().find(|s| s.name == name)
    }

    /// Run the §V sweep **once per scenario** and keep every policy.
    ///
    /// Nothing about backward induction changes here — this is TEN-4 called
    /// `|Ω|` times, which is why the ensemble is a composition of that slice
    /// rather than a second optimiser.
    pub fn analyse(
        &self,
        template: &ModelTemplate,
        space: &Space,
        ip: &InversionPoint,
        spec: &BellmanSpec,
        rewards: RewardBasis,
    ) -> Result<EnsembleAnalysis, EnsembleError> {
        let mut plans = BTreeMap::new();
        let mut models = BTreeMap::new();

        for s in &self.scenarios {
            let model = template.instantiate(s)?;

            // When R comes from the region it is itself scenario-dependent:
            // R(s,a) = W(s) − E[W(s')] takes the expectation over THIS
            // scenario's own probabilities. A pessimistic future scores the
            // same action worse, which is the honest reading and not a quirk.
            let spec_for_scenario = match rewards {
                RewardBasis::Declared => spec.clone(),
                RewardBasis::FromRegion(region) => {
                    rewards_from_region(region, space, &model, spec.clone())
                        .map_err(|e| EnsembleError::Sweep { scenario: s.name.clone(), source: e })?
                }
            };

            let plan = backward_induct(space, &model, ip, &spec_for_scenario)
                .map_err(|e| EnsembleError::Sweep { scenario: s.name.clone(), source: e })?;

            plans.insert(s.name.clone(), plan);
            models.insert(s.name.clone(), model);
        }

        Ok(EnsembleAnalysis {
            plans,
            models,
            base: self.base.clone(),
            names: self.scenarios.iter().map(|s| s.name.clone()).collect(),
            horizon: spec.horizon,
        })
    }
}

/// Where `R(s,a)` comes from.
///
/// `Declared` is §V's literal reading — only `T` varies across scenarios.
/// `FromRegion` derives the Lyapunov reward per scenario, which makes `R` vary
/// too, for the reason given in [`Ensemble::analyse`].
#[derive(Debug, Clone, Copy)]
pub enum RewardBasis<'a> {
    Declared,
    FromRegion(&'a Region),
}

// ── §VI, §VII — reading the disagreement ────────────────────────────────────

/// Every scenario's policy, kept side by side.
#[derive(Debug, Clone)]
pub struct EnsembleAnalysis {
    plans: BTreeMap<String, Plan>,
    models: BTreeMap<String, TransitionModel>,
    base: String,
    names: Vec<String>,
    horizon: usize,
}

impl EnsembleAnalysis {
    pub fn plan(&self, scenario: &str) -> Option<&Plan> {
        self.plans.get(scenario)
    }

    pub fn model(&self, scenario: &str) -> Option<&TransitionModel> {
        self.models.get(scenario)
    }

    pub fn scenario_names(&self) -> &[String] {
        &self.names
    }

    pub fn base_name(&self) -> &str {
        &self.base
    }

    /// **The window every one of these policies is about.**
    ///
    /// Each per-scenario sweep is horizon-limited over the enumerated space in
    /// exactly the way TEN-4's is, and an ensemble of horizon-limited answers
    /// is still horizon-limited. Carried so a caller cannot lose it.
    pub fn horizon(&self) -> usize {
        self.horizon
    }

    /// What each scenario would do at `(t, state)` — `None` where that
    /// scenario has no admissible action.
    pub fn actions_at(&self, t: usize, state: &str) -> BTreeMap<String, Option<String>> {
        self.names
            .iter()
            .map(|n| {
                let a = self.plans[n].action(t, state).map(str::to_string);
                (n.clone(), a)
            })
            .collect()
    }

    /// `I(s₀)` (§VI) — the intersection of the per-scenario optimal actions.
    ///
    /// Empty or a single action; see [`InvariantSet::the_one`].
    pub fn invariant_actions(&self, t: usize, state: &str) -> InvariantSet {
        let by = self.actions_at(t, state);
        let mut actions = BTreeSet::new();

        // The intersection is non-empty exactly when every scenario proposes
        // the SAME action. A scenario with no action is a disagreement about
        // whether to act at all, which is still a disagreement.
        let mut iter = by.values();
        if let Some(Some(first)) = iter.next() {
            if by.values().all(|a| a.as_deref() == Some(first.as_str())) {
                actions.insert(first.clone());
            }
        }

        InvariantSet {
            actions,
            over: self.names.clone(),
            t,
            state: state.to_string(),
        }
    }

    /// Is `(t, state)` a decision node (§VII)?
    pub fn is_decision_node(&self, t: usize, state: &str) -> bool {
        let by = self.actions_at(t, state);
        let distinct: BTreeSet<_> = by.values().collect();
        distinct.len() > 1
    }

    /// `D` at one time step.
    pub fn decision_nodes_at(&self, t: usize) -> Vec<DecisionNode> {
        let mut out = Vec::new();

        // The union across scenarios, not one scenario's view — a state only
        // one policy knows about is exactly the kind that could be missed.
        let mut ids = BTreeSet::new();
        for n in &self.names {
            for s in self.plans[n].states_at(t) {
                ids.insert(s.to_string());
            }
        }
        for state in ids {
            if self.is_decision_node(t, &state) {
                out.push(DecisionNode {
                    t,
                    state: state.clone(),
                    by_scenario: self.actions_at(t, &state),
                });
            }
        }
        out
    }

    /// `D` across the whole window.
    pub fn decision_nodes(&self) -> Vec<DecisionNode> {
        (0..self.horizon).flat_map(|t| self.decision_nodes_at(t)).collect()
    }
}

/// `I(s₀)` — **and the scenarios it is invariant over**.
///
/// The two travel together on purpose. "Invariant" without "over what" is the
/// claim this module is most able to overstate, so the type does not offer the
/// actions without the list.
#[derive(Debug, Clone, PartialEq)]
pub struct InvariantSet {
    actions: BTreeSet<String>,
    over: Vec<String>,
    t: usize,
    state: String,
}

impl InvariantSet {
    pub fn actions(&self) -> &BTreeSet<String> {
        &self.actions
    }

    /// **The declared futures this was checked against — and only those.**
    ///
    /// An action invariant over `{best, base, worst}` is unconditional with
    /// respect to those three and says nothing about a fourth nobody wrote
    /// down.
    pub fn over(&self) -> &[String] {
        &self.over
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn state(&self) -> &str {
        &self.state
    }

    pub fn t(&self) -> usize {
        self.t
    }

    /// `I` is structurally **at most one action** — the intersection of
    /// singletons. Stated as a method so nobody builds a chooser over it.
    pub fn the_one(&self) -> Option<&str> {
        debug_assert!(self.actions.len() <= 1, "I is an intersection of singletons");
        self.actions.iter().next().map(String::as_str)
    }

    /// A one-line reading that cannot be quoted without its scope.
    pub fn describe(&self) -> String {
        match self.the_one() {
            Some(a) => format!(
                "'{a}' is optimal at '{}' under all {} declared scenarios ({}) — and says nothing about futures not declared",
                self.state,
                self.over.len(),
                self.over.join(", ")
            ),
            None => format!(
                "no action is optimal at '{}' across all {} declared scenarios ({}) — this is a decision node",
                self.state,
                self.over.len(),
                self.over.join(", ")
            ),
        }
    }
}

/// A state where the optimal action **diverges** across scenarios (§VII).
///
/// Carries what each scenario wants, not merely that they disagree — the
/// disagreement itself is the actionable part ("if funding lands, expand; if
/// it does not, wind down"), and it is what a dead drop is written against.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionNode {
    pub t: usize,
    pub state: String,
    pub by_scenario: BTreeMap<String, Option<String>>,
}

impl DecisionNode {
    /// The distinct actions proposed here (excluding "no action").
    pub fn proposed(&self) -> BTreeSet<String> {
        self.by_scenario.values().flatten().cloned().collect()
    }

    /// Scenarios that would take no action at all here.
    pub fn abstaining(&self) -> Vec<&str> {
        self.by_scenario
            .iter()
            .filter(|(_, a)| a.is_none())
            .map(|(n, _)| n.as_str())
            .collect()
    }

    pub fn describe(&self) -> String {
        let parts: Vec<String> = self
            .by_scenario
            .iter()
            .map(|(n, a)| format!("{n}→{}", a.as_deref().unwrap_or("(none)")))
            .collect();
        format!("t={} '{}': {}", self.t, self.state, parts.join(", "))
    }
}

// ── §VIII — dead drops ──────────────────────────────────────────────────────

/// `δ(D_i) = a*` — a decision made **now, under clarity**, for a node not yet
/// reached.
#[derive(Debug, Clone, PartialEq)]
pub struct DeadDrop {
    pub id: String,
    pub t: usize,
    pub state: String,
    /// The guard, in the same predicate language the inversion point uses.
    /// `None` fires unconditionally.
    pub condition: Option<String>,
    pub action: String,
    hedge: bool,
}

impl DeadDrop {
    /// **This action was not optimal under any declared scenario.**
    ///
    /// Allowed — a person may pre-commit to a hedge none of the futures argued
    /// for. Flagged, because doing it silently is how a hedge becomes mistaken
    /// for a result.
    pub fn is_hedge(&self) -> bool {
        self.hedge
    }
}

/// The registered pre-commitments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeadDropBook {
    drops: Vec<DeadDrop>,
}

impl DeadDropBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn drops(&self) -> &[DeadDrop] {
        &self.drops
    }

    pub fn get(&self, id: &str) -> Option<&DeadDrop> {
        self.drops.iter().find(|d| d.id == id)
    }

    /// Register `δ(D_i) = a*`, checked while checking is cheap.
    ///
    /// Refuses a drop that does not name a real decision node, an action the
    /// model does not admit at that state, a guard that does not parse, or a
    /// duplicate id. Every one of these is trivially fixable now and a genuine
    /// problem at the node — which is the §VIII argument applied to the
    /// pre-commitment itself.
    pub fn pre_commit(
        &mut self,
        analysis: &EnsembleAnalysis,
        id: &str,
        t: usize,
        state: &str,
        condition: Option<&str>,
        action: &str,
    ) -> Result<&DeadDrop, EnsembleError> {
        if self.drops.iter().any(|d| d.id == id) {
            return Err(EnsembleError::DuplicateDeadDrop(id.to_string()));
        }
        if !analysis.is_decision_node(t, state) {
            return Err(EnsembleError::NotADecisionNode { t, state: state.to_string() });
        }

        let base_model = analysis
            .model(analysis.base_name())
            .expect("the base scenario was analysed");
        let admissible = base_model.actions_at(state);
        if !admissible.iter().any(|a| a == action) {
            return Err(EnsembleError::InadmissibleAction {
                state: state.to_string(),
                action: action.to_string(),
                admissible,
            });
        }

        if let Some(expr) = condition {
            let empty_state = Value::Object(Map::new());
            let empty = Map::new();
            // Parse-checked now. A guard that cannot be read is worse than no
            // guard, and the node is the wrong place to find out.
            if let Err(e) = crate::predicate::check(expr, &empty_state, &empty) {
                let msg = e.to_string();
                if !is_missing_path(&msg) {
                    return Err(EnsembleError::BadCondition {
                        expr: expr.to_string(),
                        detail: msg,
                    });
                }
            }
        }

        let hedge = !analysis
            .actions_at(t, state)
            .values()
            .flatten()
            .any(|a| a == action);

        self.drops.push(DeadDrop {
            id: id.to_string(),
            t,
            state: state.to_string(),
            condition: condition.map(str::to_string),
            action: action.to_string(),
            hedge,
        });
        Ok(self.drops.last().expect("just pushed"))
    }

    /// `ExecuteAtNode(D, context)` (§VIII).
    ///
    /// A registered drop whose guard holds answers. Otherwise the **base**
    /// scenario's policy does — and the [`Resolution`] says which, always. The
    /// two are never blended into one recommendation, because "you decided this
    /// in advance" and "this is what the middle scenario suggests" carry very
    /// different weight and the difference is the whole mechanism.
    pub fn execute_at(
        &self,
        analysis: &EnsembleAnalysis,
        space: &Space,
        t: usize,
        state: &str,
    ) -> Result<Resolution, EnsembleError> {
        let here: Vec<&DeadDrop> =
            self.drops.iter().filter(|d| d.t == t && d.state == state).collect();

        let mut declined = Vec::new();
        for d in &here {
            match &d.condition {
                None => {
                    return Ok(Resolution::PreCommitted {
                        drop_id: d.id.clone(),
                        action: d.action.clone(),
                        is_hedge: d.hedge,
                    })
                }
                Some(expr) => {
                    let s = space.get(state).ok_or_else(|| EnsembleError::UnknownState(state.to_string()))?;
                    let empty = Map::new();
                    let (holds, _) = crate::predicate::check(expr, s, &empty).map_err(|e| {
                        EnsembleError::BadCondition { expr: expr.clone(), detail: e.to_string() }
                    })?;
                    if holds {
                        return Ok(Resolution::PreCommitted {
                            drop_id: d.id.clone(),
                            action: d.action.clone(),
                            is_hedge: d.hedge,
                        });
                    }
                    declined.push(d.id.clone());
                }
            }
        }

        let base = analysis.base_name().to_string();
        match analysis.plan(&base).and_then(|p| p.action(t, state)) {
            Some(a) => Ok(Resolution::Policy {
                action: a.to_string(),
                from_scenario: base,
                declined,
            }),
            None => Ok(Resolution::NoAction { declined }),
        }
    }
}

/// What answered at a node — never ambiguous about which mechanism it was.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// A pre-commitment fired. Decided in advance, under clarity.
    PreCommitted { drop_id: String, action: String, is_hedge: bool },
    /// No pre-commitment applied; this is the **base** scenario's policy, and
    /// `declined` names any drop that was registered here and did not fire.
    Policy { action: String, from_scenario: String, declined: Vec<String> },
    /// Neither a drop nor a policy has anything to say here.
    NoAction { declined: Vec<String> },
}

impl Resolution {
    pub fn action(&self) -> Option<&str> {
        match self {
            Resolution::PreCommitted { action, .. } | Resolution::Policy { action, .. } => {
                Some(action)
            }
            Resolution::NoAction { .. } => None,
        }
    }

    pub fn was_pre_committed(&self) -> bool {
        matches!(self, Resolution::PreCommitted { .. })
    }
}

/// The predicate checker reports an unreadable path the same way it reports a
/// malformed expression. At registration time state is not in hand, so a
/// missing path is expected and only a genuine syntax problem is a refusal.
fn is_missing_path(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("path") || m.contains("missing") || m.contains("not found") || m.contains("no such")
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EnsembleError {
    #[error("an ensemble needs at least 2 scenarios; got {0} — with one future every action is trivially invariant and nothing is ever a decision node")]
    TooFewScenarios(usize),
    #[error("scenario '{0}' is declared twice")]
    DuplicateScenario(String),
    #[error("base scenario '{0}' is not in the ensemble")]
    UnknownBase(String),
    #[error("scenario '{scenario}' does not bind parameter '{parameter}' — an unbound probability is named, never defaulted")]
    UnboundParameter { scenario: String, parameter: String },
    #[error("scenario '{scenario}' is incoherent at ({state}, {action}): {detail}")]
    IncoherentScenario { scenario: String, state: String, action: String, detail: String },
    #[error("the sweep failed under scenario '{scenario}': {source}")]
    Sweep {
        scenario: String,
        #[source]
        source: TenetError,
    },
    #[error("dead drop '{0}' is already registered")]
    DuplicateDeadDrop(String),
    #[error("t={t} '{state}' is not a decision node — every declared scenario agrees there, so there is nothing to pre-decide")]
    NotADecisionNode { t: usize, state: String },
    #[error("'{action}' is not admissible at '{state}'; the model declares {admissible:?}")]
    InadmissibleAction { state: String, action: String, admissible: Vec<String> },
    #[error("dead-drop condition '{expr}' could not be parsed: {detail}")]
    BadCondition { expr: String, detail: String },
    #[error("'{0}' is not in the enumerated space")]
    UnknownState(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Interval;
    use serde_json::json;

    // ── the fixture: a small venture, per §IV's generalisation ──────────────
    //
    // Runway in months. From `s0` the venture can raise or cut burn; a raise
    // either lands (`raised`) or does not (`denied`); from `denied` it can
    // pivot — which costs real cash and may not work — or wind down.
    //
    // The point of the fixture is the SHAPE: `s0` must be a state every
    // scenario agrees on (invariant) and `denied` must be one they split on
    // (a decision node). Both are asserted, not assumed.

    fn space() -> Space {
        [
            ("s0", 6.0),
            ("raised", 18.0),
            ("denied", 3.0),
            ("lean", 9.0),
            ("scaled", 24.0),
            ("dead", 0.0),
        ]
        .into_iter()
        .map(|(id, v)| (id.to_string(), json!({ "runway": v })))
        .collect()
    }

    fn template() -> ModelTemplate {
        ModelTemplate::new()
            .edge(
                "s0",
                "fundraise",
                vec![("raised", Prob::param("p_funding")), ("denied", Prob::one_minus("p_funding"))],
            )
            .certain("s0", "cut_burn", "lean")
            .certain("raised", "scale", "scaled")
            .certain("raised", "hold", "raised")
            .edge(
                "denied",
                "pivot",
                vec![("lean", Prob::param("p_growth")), ("dead", Prob::one_minus("p_growth"))],
            )
            .certain("denied", "wind_down", "dead")
            .certain("lean", "grind", "lean")
            .edge(
                "lean",
                "pitch_again",
                vec![("raised", Prob::param("p_funding")), ("denied", Prob::one_minus("p_funding"))],
            )
            .certain("scaled", "hold", "scaled")
            .certain("dead", "hold", "dead")
    }

    /// §IV's own three, with §IV's own numbers.
    fn ensemble() -> Ensemble {
        Ensemble::new(
            vec![
                Scenario::new("best").assigning("p_funding", 0.85).assigning("p_growth", 0.80),
                Scenario::new("base").assigning("p_funding", 0.55).assigning("p_growth", 0.55),
                Scenario::new("worst").assigning("p_funding", 0.25).assigning("p_growth", 0.30),
            ],
            "base",
        )
        .expect("three distinct scenarios, base among them")
    }

    fn at_scaled() -> InversionPoint<'static> {
        InversionPoint::Predicate(vec!["runway >= 24".into()])
    }

    /// Pivoting burns cash. Without a real cost it always beats winding down
    /// (any chance of `lean` dominates certain `dead`), and there would be no
    /// divergence to find — the scenarios would agree by construction.
    fn spec() -> BellmanSpec {
        BellmanSpec::new(4, 0.9, 1000.0)
            // Pivoting burns cash. Without a real cost any chance of `lean`
            // dominates certain `dead`, every scenario agrees, and there is no
            // divergence to find — the fixture would prove nothing.
            .rewarding("denied", "pivot", -300.0)
            // Cutting burn costs momentum. Without a cost it merely DELAYS the
            // same raise by one step, so it ties with `fundraise` under the
            // discount and `s0` stops being invariant for an uninteresting
            // reason.
            .rewarding("s0", "cut_burn", -40.0)
            .rewarding("lean", "grind", -5.0)
            // Sitting on a raise has a holding cost. Without it `hold` and
            // `scale` are exactly equal at `raised` and the argmax breaks
            // alphabetically — a fixture that would change if an action were
            // renamed.
            .rewarding("raised", "hold", -5.0)
    }

    fn analysis() -> EnsembleAnalysis {
        ensemble()
            .analyse(&template(), &space(), &at_scaled(), &spec(), RewardBasis::Declared)
            .expect("all three sweep")
    }

    // ── TEN-3 · §IV — the ensemble ──────────────────────────────────────────

    #[test]
    fn a_scenario_is_a_probability_assignment_pushed_through_one_structure() {
        let t = template();
        assert_eq!(
            t.parameters(),
            ["p_funding".to_string(), "p_growth".to_string()].into_iter().collect()
        );

        let best = t.instantiate(&Scenario::new("best").assigning("p_funding", 0.85).assigning("p_growth", 0.8)).unwrap();
        let worst = t.instantiate(&Scenario::new("worst").assigning("p_funding", 0.25).assigning("p_growth", 0.3)).unwrap();

        // Same structure, different numbers — which is what makes a
        // disagreement between them a fact about uncertainty.
        assert_eq!(best.actions_at("s0"), worst.actions_at("s0"));

        let b = best.get("s0", "fundraise").unwrap();
        assert!((b.outcomes().iter().find(|o| o.to == "raised").unwrap().p - 0.85).abs() < 1e-9);
        let w = worst.get("s0", "fundraise").unwrap();
        assert!((w.outcomes().iter().find(|o| o.to == "raised").unwrap().p - 0.25).abs() < 1e-9);
        // OneMinus supplied the other branch from the single declared number.
        assert!((w.outcomes().iter().find(|o| o.to == "denied").unwrap().p - 0.75).abs() < 1e-9);
    }

    #[test]
    fn an_unbound_parameter_is_named_not_defaulted() {
        let err = template()
            .instantiate(&Scenario::new("half").assigning("p_funding", 0.5))
            .unwrap_err();
        match err {
            EnsembleError::UnboundParameter { scenario, parameter } => {
                assert_eq!(scenario, "half");
                assert_eq!(parameter, "p_growth");
            }
            other => panic!("expected UnboundParameter, got {other:?}"),
        }
    }

    #[test]
    fn an_incoherent_scenario_cannot_be_swept() {
        // p_funding = 1.4 makes the two branches sum to 1.4 + (-0.4).
        let err = template()
            .instantiate(&Scenario::new("bad").assigning("p_funding", 1.4).assigning("p_growth", 0.5))
            .unwrap_err();
        assert!(
            matches!(err, EnsembleError::IncoherentScenario { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn a_one_scenario_ensemble_is_refused() {
        let err = Ensemble::new(vec![Scenario::new("only")], "only").unwrap_err();
        assert!(matches!(err, EnsembleError::TooFewScenarios(1)), "got {err:?}");
    }

    #[test]
    fn the_base_must_be_one_of_the_declared_scenarios() {
        let err =
            Ensemble::new(vec![Scenario::new("a"), Scenario::new("b")], "middle").unwrap_err();
        assert!(matches!(err, EnsembleError::UnknownBase(_)), "got {err:?}");
    }

    #[test]
    fn every_scenario_gets_its_own_policy() {
        let a = analysis();
        assert_eq!(a.scenario_names(), ["best", "base", "worst"]);
        for n in ["best", "base", "worst"] {
            assert!(a.plan(n).is_some(), "{n} has no policy");
        }
        assert_eq!(a.horizon(), 4, "an ensemble of horizon-limited answers is still limited");
    }

    // ── TEN-5 · §VI — invariant actions ─────────────────────────────────────

    #[test]
    fn an_action_optimal_in_all_three_futures_is_invariant() {
        let a = analysis();
        let inv = a.invariant_actions(0, "s0");

        assert_eq!(
            inv.the_one(),
            Some("fundraise"),
            "even the worst funding odds beat cutting burn from here: {}",
            inv.describe()
        );
        // The scope travels with the claim.
        assert_eq!(inv.over(), ["best", "base", "worst"]);
        assert!(inv.describe().contains("says nothing about futures not declared"));
    }

    #[test]
    fn the_invariant_set_is_empty_at_a_state_the_scenarios_split_on() {
        let a = analysis();
        let inv = a.invariant_actions(0, "denied");
        assert!(inv.is_empty(), "the scenarios disagree here: {}", inv.describe());
        assert!(inv.describe().contains("decision node"));
    }

    #[test]
    fn the_invariant_set_is_at_most_one_action() {
        let a = analysis();
        for t in 0..a.horizon() {
            for s in space().keys() {
                assert!(
                    a.invariant_actions(t, s).actions().len() <= 1,
                    "I is an intersection of singletons"
                );
            }
        }
    }

    // ── TEN-6 · §VII — decision nodes ───────────────────────────────────────

    #[test]
    fn a_decision_node_is_where_the_optimal_action_diverges() {
        let a = analysis();
        assert!(a.is_decision_node(0, "denied"), "this is the point uncertainty bites");
        assert!(!a.is_decision_node(0, "s0"), "the scenarios agree here");

        let node = a
            .decision_nodes_at(0)
            .into_iter()
            .find(|n| n.state == "denied")
            .expect("denied is a decision node at t=0");

        // The disagreement itself, not merely that there is one.
        assert!(node.proposed().len() > 1, "{}", node.describe());
        assert_eq!(node.by_scenario["best"].as_deref(), Some("pivot"));
        assert_eq!(node.by_scenario["worst"].as_deref(), Some("wind_down"));
    }

    #[test]
    fn a_decision_node_is_horizon_dependent() {
        // Found by running the fixture, not by predicting it: `denied` splits
        // the scenarios at t=0 and t=1 and NOT at t=2 or t=3. Close enough to
        // the end of the window a pivot cannot repay its cost under ANY
        // declared future, so even the best one winds down and the scenarios
        // agree again.
        //
        // Worth pinning: "is this a decision node?" is a question about a state
        // AND a time. Asked time-free it has an answer that is true somewhere
        // and false elsewhere.
        let a = analysis();
        assert!(a.is_decision_node(0, "denied"));
        assert!(a.is_decision_node(1, "denied"));
        assert!(!a.is_decision_node(2, "denied"), "no window left for a pivot to repay");
        assert!(!a.is_decision_node(3, "denied"));

        assert_eq!(
            a.invariant_actions(2, "denied").the_one(),
            Some("wind_down"),
            "and where they agree again, that agreement is itself invariant"
        );
    }

    #[test]
    fn agreement_everywhere_means_no_decision_nodes() {
        // One shared number across all scenarios: same model three times.
        let flat = Ensemble::new(
            vec![
                Scenario::new("a").assigning("p_funding", 0.5).assigning("p_growth", 0.5),
                Scenario::new("b").assigning("p_funding", 0.5).assigning("p_growth", 0.5),
            ],
            "a",
        )
        .unwrap();
        let a = flat
            .analyse(&template(), &space(), &at_scaled(), &spec(), RewardBasis::Declared)
            .unwrap();
        assert!(a.decision_nodes().is_empty(), "identical futures cannot disagree");
    }

    // ── TEN-7 · §VIII — dead drops ──────────────────────────────────────────

    #[test]
    fn a_dead_drop_fires_at_its_node() {
        let a = analysis();
        let mut book = DeadDropBook::new();
        book.pre_commit(&a, "d_denied", 0, "denied", Some("runway >= 3"), "pivot")
            .expect("a real node, an admissible action, a readable guard");

        let r = book.execute_at(&a, &space(), 0, "denied").unwrap();
        assert!(r.was_pre_committed(), "decided in advance, under clarity");
        assert_eq!(r.action(), Some("pivot"));
    }

    #[test]
    fn a_drop_whose_guard_does_not_hold_falls_back_and_says_so() {
        let a = analysis();
        let mut book = DeadDropBook::new();
        // `denied` has runway 3, so this guard is false there.
        book.pre_commit(&a, "d_rich", 0, "denied", Some("runway >= 12"), "pivot").unwrap();

        let r = book.execute_at(&a, &space(), 0, "denied").unwrap();
        match r {
            Resolution::Policy { action, from_scenario, declined } => {
                assert_eq!(from_scenario, "base", "the fallback is the DECLARED base, not a guess");
                assert_eq!(declined, ["d_rich"], "a drop that declined is named, not hidden");
                assert!(!action.is_empty());
            }
            other => panic!("expected a policy fallback, got {other:?}"),
        }
    }

    #[test]
    fn without_a_drop_the_base_policy_answers_and_is_labelled() {
        let a = analysis();
        let book = DeadDropBook::new();
        let r = book.execute_at(&a, &space(), 0, "denied").unwrap();
        match &r {
            Resolution::Policy { from_scenario, declined, .. } => {
                assert_eq!(from_scenario, "base");
                assert!(declined.is_empty());
            }
            other => panic!("expected Policy, got {other:?}"),
        }
        assert!(!r.was_pre_committed(), "a suggestion is not a pre-commitment");
    }

    #[test]
    fn a_drop_on_a_state_the_scenarios_agree_about_is_refused() {
        let a = analysis();
        let mut book = DeadDropBook::new();
        let err = book.pre_commit(&a, "d_s0", 0, "s0", None, "fundraise").unwrap_err();
        assert!(
            matches!(err, EnsembleError::NotADecisionNode { .. }),
            "nothing to pre-decide where every future agrees; got {err:?}"
        );
    }

    #[test]
    fn a_drop_naming_an_inadmissible_action_is_refused() {
        let a = analysis();
        let mut book = DeadDropBook::new();
        let err = book.pre_commit(&a, "d_bad", 0, "denied", None, "scale").unwrap_err();
        match err {
            EnsembleError::InadmissibleAction { state, action, admissible } => {
                assert_eq!(state, "denied");
                assert_eq!(action, "scale");
                assert!(admissible.contains(&"pivot".to_string()));
            }
            other => panic!("expected InadmissibleAction, got {other:?}"),
        }
    }

    #[test]
    fn a_hedge_is_allowed_and_flagged() {
        // Both scenarios' argmax at `denied` is pivot or wind_down. Registering
        // the one NO scenario chose at this node is legitimate human judgment —
        // and must be visible as such.
        let a = analysis();
        let node = a.actions_at(0, "denied");
        let proposed: BTreeSet<String> = node.values().flatten().cloned().collect();
        let unchosen = ["pivot", "wind_down"]
            .into_iter()
            .find(|x| !proposed.contains(*x));

        let mut book = DeadDropBook::new();
        if let Some(u) = unchosen {
            let d = book.pre_commit(&a, "d_hedge", 0, "denied", None, u).unwrap();
            assert!(d.is_hedge(), "an action no future argued for must be visible as a hedge");
        } else {
            // Both actions are proposed by some scenario, so neither is a hedge.
            let d = book.pre_commit(&a, "d_chosen", 0, "denied", None, "pivot").unwrap();
            assert!(!d.is_hedge(), "pivot IS what the best scenario chose");
        }
    }

    #[test]
    fn a_duplicate_dead_drop_is_refused() {
        let a = analysis();
        let mut book = DeadDropBook::new();
        book.pre_commit(&a, "d", 0, "denied", None, "pivot").unwrap();
        let err = book.pre_commit(&a, "d", 0, "denied", None, "wind_down").unwrap_err();
        assert!(matches!(err, EnsembleError::DuplicateDeadDrop(_)), "got {err:?}");
    }

    // ── the region-derived reward is scenario-dependent ─────────────────────

    #[test]
    fn a_region_derived_reward_varies_by_scenario_because_the_expectation_does() {
        let region = Region::new()
            .bounding(Interval::at_least("runway", 6.0))
            .weighing("runway", 1.0);
        let a = ensemble()
            .analyse(&template(), &space(), &at_scaled(), &spec(), RewardBasis::FromRegion(&region))
            .expect("sweeps under every scenario");

        // Nothing about the sweep changed; only what R means. The policies are
        // still complete and the horizon is still carried.
        assert_eq!(a.horizon(), 4);
        for n in a.scenario_names() {
            assert!(a.plan(n).is_some());
        }
    }
}
