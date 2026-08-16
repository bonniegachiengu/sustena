//! The node, the population, and threshold commitment
//! (Multiparty §I + §II · MUL-1, MUL-2, MUL-3).
//!
//! ```text
//! n_i = ⟨s_i, N(i), act_i⟩,   N(i) ⊆ P \ {n_i}                      §I
//! c_i = Σ_{j∈N(i)} σ_j · e^{−λ·d(i,j)},   commit_i = 1[c_i > θ]     §II
//! ```
//!
//! ## The hard constraint, and where it lives in the types (§I)
//!
//! > The population must reach *coordinated global state* using only *local
//! > information exchange*. **No node ever sees P.** Every result below lives
//! > under that constraint.
//!
//! So a commit decision is computed from a [`LocalView`], which is built from
//! `N(i)` and nothing else — there is no path from a node's decision to the
//! population's roster. [`Population::committed_fraction`] exists and is
//! **explicitly an observer's instrument**: it is what a test or a person
//! watching from outside can see, and it is never an input to
//! [`Population::quorum_step`]. A cascade that consulted it would be a
//! coordinated global state reached by reading the global state.
//!
//! ## ★ Why the commitment lattice is NOT routed through the refractory field
//!
//! This layer sits on [`crate::signal`], but not naively, and the distinction
//! is worth stating because getting it wrong is subtle and total:
//!
//! - **§II's `σ` is STANDING.** `node.signal = 1` is a state field that stays
//!   set; `c_i` sums it over neighbours as a persistent concentration.
//!   Commitment **latches** — that is what makes the cascade a cascade.
//! - **§III's pulse is TRANSIENT.** A node fires once and goes refractory; the
//!   wave passes through and the field falls quiet.
//!
//! Routing the commitment itself through an excitable medium would make it
//! **die out like a wave instead of latching**, and the phase transition would
//! vanish. So the commitment lattice is its own monotone state, and the Signal
//! primitive supplies what it genuinely supplies: **the medium** — a shared
//! topology with no broadcaster in it — and **debounced announcements**, so a
//! node announces its commitment once rather than every tick, exactly the job
//! §III's refractory window was described for.
//!
//! Neither of those is load-bearing for the cascade arithmetic, and saying so
//! is more useful than implying the reuse is deeper than it is.
//!
//! ## ★ MUL-3 — two different quorums, and the types keep them apart
//!
//! > The quorum of this section is a *count against a threshold*. The quorum of
//! > §V is an *intersecting majority*. The first decides whether a body forms
//! > at all. The second decides what a formed body may ratify. **Neither
//! > substitutes for the other.**
//!
//! That is not only a note here. §II's `θ` is an `f64` **concentration**, and a
//! commit is a threshold crossing on a real number; §V's quorum is a property
//! of **sets** — that any two of them intersect. A scalar comparison cannot
//! stand in for a set-intersection property, so the confusion the article warns
//! about is not expressible through this type. §V's quorum is the consensus
//! follow-on, and `editing.rs`'s `CouncilMint` already names it as the point
//! where it will attach.
//!
//! ## Honest limits
//!
//! - **Mean-field is an approximation.** §II's `ẋ = −x + f(βx)` and its
//!   critical gain `β*` describe a well-mixed population. The exact cascade
//!   depends on the topology, so nothing here reports a predicted `β*`.
//!   [`Population::sweep`] instead **observes** the transition on the graph
//!   that was actually declared, and [`PhaseTransition`] says which graph it is
//!   about.
//! - **Neighbourhoods here are symmetric.** §I permits `N(i)` to be any subset,
//!   so a node could sense a neighbour that does not sense it. This build
//!   declares symmetric neighbourhoods, because they share a medium with the
//!   Signal field, whose edges are symmetric by design. A disclosed scope
//!   choice, not an oversight.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::signal::{Field, SignalError, SignalSpec};

/// The declared parameters of a population (§II).
#[derive(Debug, Clone, PartialEq)]
pub struct PopulationSpec {
    /// `θ` — the concentration a node must sense before it commits.
    ///
    /// A **real number**, not a member count. See the module docs on MUL-3.
    pub theta: f64,
    /// `λ` — how sharply influence decays with declared distance.
    pub lambda: f64,
    /// The medium commitments are announced on.
    pub announce: SignalSpec,
}

impl PopulationSpec {
    pub fn new(theta: f64, lambda: f64) -> Result<Self, PopulationError> {
        if !theta.is_finite() || theta <= 0.0 {
            return Err(PopulationError::BadTheta(theta.to_string()));
        }
        if !lambda.is_finite() || lambda < 0.0 {
            return Err(PopulationError::BadLambda(lambda.to_string()));
        }
        Ok(Self {
            theta,
            lambda,
            announce: SignalSpec::new(1.0, 2).expect("a real medium"),
        })
    }

    pub fn announcing_on(mut self, spec: SignalSpec) -> Self {
        self.announce = spec;
        self
    }
}

/// **Everything a node can see** — `N(i)`, and nothing beyond it (§I).
///
/// The only input to a commit decision. There is no field on this type that
/// could carry the population's roster, which is what makes *"no node ever sees
/// P"* a property of the code rather than a rule someone follows.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalView {
    pub node: String,
    /// `c_i` — the concentration sensed from this node's own neighbourhood.
    pub concentration: f64,
    /// How many of **this node's** neighbours have committed.
    pub committed_neighbours: usize,
    /// How many neighbours it has at all.
    pub neighbours: usize,
}

impl LocalView {
    /// `commit_i = 1[c_i > θ]`.
    pub fn would_commit(&self, theta: f64) -> bool {
        self.concentration > theta
    }
}

/// What one `quorum_step` did.
#[derive(Debug, Clone, PartialEq)]
pub struct StepOutcome {
    pub tick: usize,
    /// Nodes that crossed `θ` on this tick.
    pub newly_committed: Vec<String>,
    /// Nodes that **announced** on the Signal medium this tick. A node
    /// announces once; the refractory window keeps it from re-announcing.
    pub announced: Vec<String>,
}

impl StepOutcome {
    pub fn is_quiet(&self) -> bool {
        self.newly_committed.is_empty()
    }
}

/// A completed cascade over the **declared** graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Cascade {
    pub steps: Vec<StepOutcome>,
    /// Committed / total, at the end.
    pub final_fraction: f64,
    /// Did it stop because nothing more committed, rather than running out of
    /// ticks?
    pub converged: bool,
    pub committed: BTreeSet<String>,
}

impl Cascade {
    /// Did commitment reach the whole population?
    pub fn total(&self, population_size: usize) -> bool {
        self.committed.len() == population_size
    }

    /// Ticks in which something new committed — the shape of the spread.
    pub fn waves(&self) -> usize {
        self.steps.iter().filter(|s| !s.is_quiet()).count()
    }
}

/// `P = {n_1, …, n_k}` (§I).
#[derive(Debug, Clone)]
pub struct Population {
    spec: PopulationSpec,
    order: Vec<String>,
    /// `N(i)` with the declared `d(i,j)`.
    neighbourhood: BTreeMap<String, BTreeMap<String, f64>>,
    committed: BTreeSet<String>,
    /// The medium announcements travel on. Not the commitment lattice.
    field: Field,
    tick: usize,
}

impl Population {
    pub fn new(spec: PopulationSpec) -> Self {
        let field = Field::new(spec.announce.clone());
        Self {
            spec,
            order: Vec::new(),
            neighbourhood: BTreeMap::new(),
            committed: BTreeSet::new(),
            field,
            tick: 0,
        }
    }

    pub fn with_node(mut self, id: &str) -> Self {
        if !self.neighbourhood.contains_key(id) {
            self.order.push(id.to_string());
            self.neighbourhood.insert(id.to_string(), BTreeMap::new());
            self.field = std::mem::replace(&mut self.field, Field::new(self.spec.announce.clone()))
                .with_node(id);
        }
        self
    }

    /// Declare `j ∈ N(i)` with distance `d(i,j)`, symmetrically.
    pub fn linking(mut self, a: &str, b: &str, distance: f64) -> Self {
        self.link(a, b, distance).expect("nodes declared before linking");
        self
    }

    pub fn link(&mut self, a: &str, b: &str, distance: f64) -> Result<(), PopulationError> {
        if !distance.is_finite() || distance < 0.0 {
            return Err(PopulationError::BadDistance(distance.to_string()));
        }
        for id in [a, b] {
            if !self.neighbourhood.contains_key(id) {
                return Err(PopulationError::UnknownNode(id.to_string()));
            }
        }
        if a == b {
            return Err(PopulationError::SelfNeighbour(a.to_string()));
        }
        self.neighbourhood.get_mut(a).expect("checked").insert(b.to_string(), distance);
        self.neighbourhood.get_mut(b).expect("checked").insert(a.to_string(), distance);
        self.field.connect(a, b)?;
        Ok(())
    }

    pub fn size(&self) -> usize {
        self.order.len()
    }

    pub fn nodes(&self) -> &[String] {
        &self.order
    }

    pub fn neighbours(&self, id: &str) -> Option<&BTreeMap<String, f64>> {
        self.neighbourhood.get(id)
    }

    pub fn is_committed(&self, id: &str) -> bool {
        self.committed.contains(id)
    }

    /// **An observer's instrument, never a node's input.**
    ///
    /// A cascade that consulted this would be reaching coordinated global state
    /// by reading the global state, which is the one thing §I rules out. It is
    /// here so a test — or a person watching from outside — can see what the
    /// population did.
    pub fn committed_fraction(&self) -> f64 {
        if self.order.is_empty() {
            return 0.0;
        }
        self.committed.len() as f64 / self.order.len() as f64
    }

    /// Commit a node directly — the initial pressure a cascade starts from.
    pub fn seed(&mut self, id: &str) -> Result<(), PopulationError> {
        if !self.neighbourhood.contains_key(id) {
            return Err(PopulationError::UnknownNode(id.to_string()));
        }
        self.committed.insert(id.to_string());
        Ok(())
    }

    /// `c_i = Σ_{j∈N(i)} σ_j · e^{−λ·d(i,j)}`, and nothing outside `N(i)`.
    pub fn local_view(&self, id: &str) -> Option<LocalView> {
        let n = self.neighbourhood.get(id)?;
        let mut c = 0.0;
        let mut committed_neighbours = 0;
        for (j, d) in n {
            if self.committed.contains(j) {
                committed_neighbours += 1;
                c += (-self.spec.lambda * d).exp();
            }
        }
        Some(LocalView {
            node: id.to_string(),
            concentration: c,
            committed_neighbours,
            neighbours: n.len(),
        })
    }

    pub fn concentration(&self, id: &str) -> Option<f64> {
        self.local_view(id).map(|v| v.concentration)
    }

    /// `quorum_step` (§II).
    ///
    /// Every node senses its own neighbourhood, commits if `c > θ`, and
    /// announces. Decisions are computed from the **pre-step** commitment
    /// state, so a node cannot be pushed over by a neighbour that crossed in
    /// the same tick — that would let the cascade run ahead of the information
    /// actually exchanged.
    pub fn quorum_step(&mut self) -> StepOutcome {
        let mut newly = Vec::new();
        for id in &self.order {
            if self.committed.contains(id) {
                continue;
            }
            let view = self.local_view(id).expect("declared");
            if view.would_commit(self.spec.theta) {
                newly.push(id.clone());
            }
        }
        for id in &newly {
            self.committed.insert(id.clone());
        }

        // Announce on the medium. The refractory window is what keeps a
        // committed node from re-announcing every tick.
        let theta_fire = self.field.spec().theta_fire;
        for id in &newly {
            let _ = self.field.stimulate(id, theta_fire);
        }
        let report = self.field.step();
        let announced: Vec<String> = report.fired.iter().map(|f| f.node.clone()).collect();

        self.tick += 1;
        StepOutcome { tick: self.tick, newly_committed: newly, announced }
    }

    /// Step until nothing new commits (§II's cascade).
    ///
    /// **Converging is the normal end.** Running out of ticks is reported as
    /// `converged: false` rather than silently returning a partial spread that
    /// reads like a finished one.
    pub fn cascade(&mut self, max_ticks: usize) -> Cascade {
        let mut steps = Vec::new();
        let mut converged = false;
        for _ in 0..max_ticks {
            let s = self.quorum_step();
            let quiet = s.is_quiet();
            steps.push(s);
            if quiet {
                converged = true;
                break;
            }
        }
        Cascade {
            steps,
            final_fraction: self.committed_fraction(),
            converged,
            committed: self.committed.clone(),
        }
    }

    /// Reset the commitment lattice and the medium, keeping the topology.
    pub fn reset(&mut self) {
        self.committed.clear();
        self.tick = 0;
        self.field = Field::new(self.spec.announce.clone());
        for id in &self.order {
            self.field = std::mem::replace(&mut self.field, Field::new(self.spec.announce.clone()))
                .with_node(id);
        }
        for (a, ns) in &self.neighbourhood {
            for b in ns.keys() {
                if a < b {
                    let _ = self.field.connect(a, b);
                }
            }
        }
    }
}

/// The observed phase transition **for one declared graph**.
///
/// Not a mean-field prediction. §II's `ẋ = −x + f(βx)` describes a well-mixed
/// population and its critical gain depends on the topology, so this reports
/// what actually happened on the graph it was given — and names it.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseTransition {
    /// Every `θ` tried, with the fraction that ended up committed.
    pub curve: Vec<(f64, f64)>,
    /// The largest `θ` at which the cascade still went total, and the smallest
    /// at which it did not. `None` if the sweep never crossed.
    pub critical: Option<(f64, f64)>,
    /// How many nodes the sweep was run over — the graph this is about.
    pub population_size: usize,
}

impl PhaseTransition {
    /// Did the sweep actually observe a jump, rather than a gentle slope?
    ///
    /// The bifurcation §II describes is a **discontinuity**: below the critical
    /// point almost nothing commits, above it almost everything does.
    pub fn is_sharp(&self) -> bool {
        match self.critical {
            None => false,
            Some((below, above)) => {
                let f = |t: f64| {
                    self.curve.iter().find(|(x, _)| *x == t).map(|(_, y)| *y).unwrap_or(0.0)
                };
                f(below) - f(above) > 0.5
            }
        }
    }
}

/// Run the cascade from the same seed at a sequence of `θ`, and report where it
/// tipped **on this graph**.
pub fn sweep(
    build: impl Fn() -> Population,
    seeds: &[&str],
    thetas: &[f64],
    max_ticks: usize,
) -> Result<PhaseTransition, PopulationError> {
    let mut curve = Vec::new();
    let mut size = 0;
    let mut last_total: Option<f64> = None;
    let mut critical = None;

    for theta in thetas {
        let base = build();
        let mut p = Population::new(
            PopulationSpec::new(*theta, base.spec.lambda)?.announcing_on(base.spec.announce.clone()),
        );
        p.order = base.order.clone();
        p.neighbourhood = base.neighbourhood.clone();
        p.reset();
        size = p.size();

        for s in seeds {
            p.seed(s)?;
        }
        let c = p.cascade(max_ticks);
        let total = c.total(size);
        curve.push((*theta, c.final_fraction));

        if total {
            last_total = Some(*theta);
        } else if let (Some(below), None) = (last_total, critical) {
            critical = Some((below, *theta));
        }
    }

    Ok(PhaseTransition { curve, critical, population_size: size })
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum PopulationError {
    #[error("θ must be finite and > 0; got {0} — a threshold everything clears is not a threshold, and a body that always forms has not sensed anything")]
    BadTheta(String),
    #[error("λ must be finite and >= 0; got {0}")]
    BadLambda(String),
    #[error("a declared distance must be finite and >= 0; got {0}")]
    BadDistance(String),
    #[error("'{0}' is not a node in this population")]
    UnknownNode(String),
    #[error("'{0}' cannot be its own neighbour — N(i) ⊆ P \\ {{n_i}}")]
    SelfNeighbour(String),
    #[error(transparent)]
    Signal(#[from] SignalError),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 6-node ring with unit distances — everyone has exactly two neighbours,
    /// so the concentration a node can sense is bounded and the threshold has
    /// something to bite on.
    fn ring(theta: f64) -> Population {
        let mut p = Population::new(PopulationSpec::new(theta, 0.0).unwrap());
        for id in ["a", "b", "c", "d", "e", "f"] {
            p = p.with_node(id);
        }
        for (x, y) in [("a", "b"), ("b", "c"), ("c", "d"), ("d", "e"), ("e", "f"), ("f", "a")] {
            p = p.linking(x, y, 1.0);
        }
        p
    }

    // ── §I — the node and the population ────────────────────────────────────

    #[test]
    fn a_node_senses_only_its_own_neighbourhood() {
        // ★ §I's hard constraint. `a`'s neighbours are `b` and `f`; committing
        // `d` — across the ring — changes nothing `a` can see.
        let mut p = ring(0.5);
        p.seed("d").unwrap();

        let view = p.local_view("a").unwrap();
        assert_eq!(view.concentration, 0.0, "★ no node ever sees P");
        assert_eq!(view.committed_neighbours, 0);
        assert_eq!(view.neighbours, 2);

        // And an observer CAN see it — which is why that accessor is named for
        // the observer.
        assert!(p.committed_fraction() > 0.0);
    }

    #[test]
    fn a_node_cannot_be_its_own_neighbour() {
        let mut p = ring(0.5);
        assert!(matches!(p.link("a", "a", 1.0), Err(PopulationError::SelfNeighbour(_))));
    }

    #[test]
    fn the_declared_parameters_are_checked() {
        assert!(matches!(PopulationSpec::new(0.0, 1.0), Err(PopulationError::BadTheta(_))));
        assert!(matches!(PopulationSpec::new(-1.0, 1.0), Err(PopulationError::BadTheta(_))));
        assert!(matches!(PopulationSpec::new(1.0, -1.0), Err(PopulationError::BadLambda(_))));
    }

    // ── §II — concentration and the threshold ───────────────────────────────

    #[test]
    fn concentration_decays_with_declared_distance() {
        // λ = 1: a neighbour at distance 2 contributes less than one at 1.
        let mut p = Population::new(PopulationSpec::new(0.5, 1.0).unwrap())
            .with_node("me")
            .with_node("near")
            .with_node("far")
            .linking("me", "near", 1.0)
            .linking("me", "far", 3.0);

        p.seed("near").unwrap();
        let near_only = p.concentration("me").unwrap();
        p.seed("far").unwrap();
        let both = p.concentration("me").unwrap();

        assert!((near_only - (-1.0f64).exp()).abs() < 1e-12);
        assert!(both > near_only);
        assert!(both - near_only < near_only, "★ the far neighbour counts for less");
    }

    #[test]
    fn below_theta_nothing_commits() {
        // One seed gives each of its two neighbours c = 1 (λ = 0), which does
        // not exceed θ = 1.5. "Below θ, nothing."
        let mut p = ring(1.5);
        p.seed("a").unwrap();
        let c = p.cascade(20);

        assert!(c.converged);
        assert_eq!(c.committed.len(), 1, "★ only the seed — nothing spread");
        assert_eq!(c.waves(), 0);
    }

    #[test]
    fn above_theta_commitment_cascades_through_the_whole_population() {
        // ★★ THE PROOF OF VALUE. θ = 0.5, so one committed neighbour is enough:
        // each commit raises a neighbour's c past θ, and the ring fills.
        let mut p = ring(0.5);
        p.seed("a").unwrap();
        let c = p.cascade(20);

        assert!(c.converged);
        assert!(c.total(p.size()), "★ the whole population committed");
        assert_eq!(c.final_fraction, 1.0);
        assert!(c.waves() >= 2, "and it SPREAD — it did not all happen at once");
    }

    #[test]
    fn the_cascade_spreads_outward_one_neighbourhood_at_a_time() {
        let mut p = ring(0.5);
        p.seed("a").unwrap();

        let first = p.quorum_step();
        assert_eq!(first.newly_committed, ["b", "f"], "a's neighbours, and only those");
        let second = p.quorum_step();
        assert_eq!(second.newly_committed, ["c", "e"], "then theirs");
    }

    #[test]
    fn a_node_is_not_pushed_over_by_a_neighbour_that_crossed_in_the_same_tick() {
        // Decisions read the PRE-step lattice. Otherwise the cascade would run
        // ahead of the information actually exchanged, and a single step could
        // sweep the graph.
        let mut p = ring(0.5);
        p.seed("a").unwrap();
        let first = p.quorum_step();
        assert_eq!(first.newly_committed.len(), 2, "not the whole ring in one tick");
    }

    // ── the announcement medium ─────────────────────────────────────────────

    #[test]
    fn a_commit_is_announced_once_not_every_tick() {
        // The Signal primitive doing what it is for: the refractory window
        // keeps a standing commitment from re-announcing forever.
        let mut p = ring(0.5);
        p.seed("a").unwrap();

        let mut announcements: BTreeMap<String, usize> = BTreeMap::new();
        let c = p.cascade(20);
        for s in &c.steps {
            for id in &s.announced {
                *announcements.entry(id.clone()).or_default() += 1;
            }
        }
        assert!(!announcements.is_empty(), "commitments were announced");
        assert!(
            announcements.values().all(|n| *n == 1),
            "★ each node announced exactly once: {announcements:?}"
        );
    }

    // ── ★ the phase transition, on THIS graph ───────────────────────────────

    #[test]
    fn the_transition_is_sharp_on_the_declared_graph() {
        // ★ §II's bifurcation, OBSERVED rather than predicted. Below the
        // critical θ the ring fills; above it, nothing moves. No mean-field β*
        // is reported, because its value depends on the topology.
        let t = sweep(|| ring(1.0), &["a"], &[0.5, 0.9, 1.1, 1.5], 20).unwrap();

        assert_eq!(t.population_size, 6);
        let (below, above) = t.critical.expect("the sweep crossed");
        assert!(below < above);
        assert!(t.is_sharp(), "★ a discontinuity, not a slope: {:?}", t.curve);

        // Concretely: total below, seed-only above.
        let f = |x: f64| t.curve.iter().find(|(a, _)| *a == x).unwrap().1;
        assert_eq!(f(0.9), 1.0);
        assert!(f(1.1) < 0.2);
    }

    #[test]
    fn a_sweep_that_never_crosses_reports_no_critical_point() {
        // Honest absence: if every θ tried behaves the same way, there is no
        // observed transition to report.
        let t = sweep(|| ring(1.0), &["a"], &[2.0, 3.0, 4.0], 20).unwrap();
        assert!(t.critical.is_none());
        assert!(!t.is_sharp());
    }

    #[test]
    fn a_cascade_that_runs_out_of_ticks_says_so() {
        let mut p = ring(0.5);
        p.seed("a").unwrap();
        let c = p.cascade(1);
        assert!(!c.converged, "a partial spread must not read like a finished one");
        assert!(!c.total(p.size()));
    }

    // ── MUL-3 — the two quorums are different objects ───────────────────────

    #[test]
    fn this_quorum_is_a_scalar_threshold_not_a_set_property() {
        // §II's quorum is a concentration crossing θ; §V's is the property that
        // any two quorum sets intersect. A commit here is a comparison of two
        // real numbers, so it structurally cannot answer a question about set
        // intersection — which is the confusion the article warns about.
        let mut p = ring(0.5);
        p.seed("a").unwrap();
        let view = p.local_view("b").unwrap();

        assert!(view.would_commit(0.5));
        assert!(!view.would_commit(1.5), "the same view, a different threshold");
        // The decision is a function of (concentration, θ) alone — no set of
        // members appears in it anywhere.
        assert_eq!(view.would_commit(0.5), view.concentration > 0.5);
    }
}
