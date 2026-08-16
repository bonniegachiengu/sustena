//! Conductance-adaptive routing — the Physarum model
//! (Multiparty §VIII · MUL-13, and the second half of MUL-15).
//!
//! > The body must route its shared resources, and it must do so with **no
//! > planner and no global view**. The mechanism is flow-reinforced
//! > conductance.
//!
//! ```text
//! Q_ij = (D_ij / L_ij)(p_i − p_j)          flux through an edge
//! dD_ij/dt = f(|Q_ij|) − D_ij              reinforce ∝ flow, else decay
//! ```
//!
//! Flow thickens a channel; a thick channel carries more flow; unused channels
//! decay. **The feedback is entirely local** — each edge sees only its own
//! flux — and the fixed points trade efficiency against fault tolerance about
//! as well as a designer would. The same dynamics provably solve shortest path
//! (Bonifaci, Mehlhorn & Varma 2012) and reproduce the Tokyo rail network from
//! food sources laid out like its suburbs (Tero et al. 2010).
//!
//! > **This is Mycelium**: shared money, tasks, compute, and storage routed by
//! > reinforcing what carries load and pruning what doesn't.
//!
//! ## What is global here, and what is not
//!
//! Worth being exact, because "no global view" is easy to overclaim. The
//! **reinforcement** is local: [`Reinforcement::apply`] sees one edge's own
//! flux and nothing else, and that is where all the adaptation happens. The
//! **flow solve** is a Kirchhoff conservation law over the whole graph — it is
//! physics, not planning. Nothing chooses a route, ranks alternatives, or
//! knows a destination; pressure equalises and each edge responds to what
//! passes through it. In a physical Physarum the solve *is* the fluid.
//!
//! ## Composed, not rebuilt
//!
//! [`Router::over`] takes a [`Population`] and uses its already-declared
//! neighbourhood as the graph — and its declared `d(i,j)` **is** `L_ij`. The
//! coordination layer declared those distances for concentration decay; the
//! same numbers are edge lengths here, which is the point of a shared topology
//! rather than a coincidence.
//!
//! ## Honest limits
//!
//! - **It is a fixed-point iteration, not a closed form.** [`Router::converge`]
//!   reports whether it actually converged; running out of steps comes back as
//!   `converged: false` rather than a partial result that reads finished.
//! - **Decay is asymptotic.** `D += dt·(f(|Q|) − D)` with `dt < 1` never
//!   reaches exactly zero from a positive start, so **"pruned" is a reading at
//!   a declared cutoff** ([`RouterSpec::prune_below`]), not a claim that an
//!   edge was removed. Nothing is deleted; the graph keeps every edge it was
//!   given.
//! - **A disconnected source and sink is refused, not zeroed.** With no path,
//!   the Laplacian is singular and there are no pressures to report. Returning
//!   zeros would say "no flow needed" where the truth is "no route exists".
//! - **`dt`, `f` and `L` are declared** — the same discipline as the region
//!   weights, the CUSUM parameters and `θ`/`λ`. A reinforcement exponent is a
//!   statement about how sharply this body commits to a channel.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::population::Population;

/// `f(|Q|)` — the monotone reinforcement rule.
///
/// §VIII names the shape (*"a monotone reinforcement rule"*) rather than one
/// function, so both standard forms are available and **declared**: `μ` sets
/// how sharply the body commits to a channel that is carrying load.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reinforcement {
    mu: f64,
    saturating: bool,
}

impl Reinforcement {
    /// `f(q) = q^μ`. Unbounded — conductance grows with flux without limit.
    pub fn power(mu: f64) -> Result<Self, RouterError> {
        Self::check(mu)?;
        Ok(Self { mu, saturating: false })
    }

    /// `f(q) = q^μ / (1 + q^μ)` — Tero et al.'s saturating form. A channel
    /// cannot thicken without bound however much passes through it.
    pub fn saturating(mu: f64) -> Result<Self, RouterError> {
        Self::check(mu)?;
        Ok(Self { mu, saturating: true })
    }

    fn check(mu: f64) -> Result<(), RouterError> {
        if !mu.is_finite() || mu <= 0.0 {
            return Err(RouterError::BadExponent(mu.to_string()));
        }
        Ok(())
    }

    /// **Sees one edge's own flux and nothing else.** This is where "no global
    /// view" actually lives.
    pub fn apply(&self, flux: f64) -> f64 {
        let q = flux.abs().powf(self.mu);
        if self.saturating {
            q / (1.0 + q)
        } else {
            q
        }
    }

    /// Monotone in `|Q|`, which is the property §VIII actually relies on.
    pub fn is_monotone(&self) -> bool {
        true
    }
}

/// The declared parameters of a router.
#[derive(Debug, Clone, PartialEq)]
pub struct RouterSpec {
    /// Integration step. `dt ∈ (0, 1]`.
    pub dt: f64,
    pub reinforcement: Reinforcement,
    /// The cutoff at which an edge is **reported** as pruned. Decay is
    /// asymptotic, so this is a reading, not a deletion.
    pub prune_below: f64,
}

impl RouterSpec {
    pub fn new(dt: f64, reinforcement: Reinforcement, prune_below: f64) -> Result<Self, RouterError> {
        if !dt.is_finite() || dt <= 0.0 || dt > 1.0 {
            return Err(RouterError::BadStep(dt.to_string()));
        }
        if !prune_below.is_finite() || prune_below < 0.0 {
            return Err(RouterError::BadPruneThreshold(prune_below.to_string()));
        }
        Ok(Self { dt, reinforcement, prune_below })
    }
}

/// One channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub a: String,
    pub b: String,
    /// `D_ij` — thickens with flow, decays without it.
    pub conductivity: f64,
    /// `L_ij` — declared, and fixed. Geometry does not adapt; conductance does.
    pub length: f64,
}

impl Edge {
    /// `D/L` — the edge's contribution to the Kirchhoff system.
    pub fn weight(&self) -> f64 {
        self.conductivity / self.length
    }
}

/// What one `physarum_step` did.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// `Q_ij` per edge, in the router's edge order.
    pub flux: Vec<f64>,
    /// The largest `|ΔD|` this step — what convergence is measured on.
    pub largest_change: f64,
}

/// The result of iterating to a fixed point.
#[derive(Debug, Clone, PartialEq)]
pub struct Convergence {
    pub steps: usize,
    /// **False when it ran out of steps.** A partial run must not read like a
    /// settled one.
    pub converged: bool,
    pub largest_change: f64,
}

/// The graph, its conductances, and the flow through them.
#[derive(Debug, Clone)]
pub struct Router {
    spec: RouterSpec,
    nodes: Vec<String>,
    index: BTreeMap<String, usize>,
    edges: Vec<Edge>,
}

impl Router {
    pub fn new(spec: RouterSpec) -> Self {
        Self { spec, nodes: Vec::new(), index: BTreeMap::new(), edges: Vec::new() }
    }

    pub fn with_node(mut self, id: &str) -> Self {
        if !self.index.contains_key(id) {
            self.index.insert(id.to_string(), self.nodes.len());
            self.nodes.push(id.to_string());
        }
        self
    }

    /// Declare an edge with length `L` and starting conductivity `D`.
    pub fn linking(mut self, a: &str, b: &str, length: f64, conductivity: f64) -> Self {
        self.link(a, b, length, conductivity).expect("nodes declared before linking");
        self
    }

    pub fn link(
        &mut self,
        a: &str,
        b: &str,
        length: f64,
        conductivity: f64,
    ) -> Result<(), RouterError> {
        if !length.is_finite() || length <= 0.0 {
            return Err(RouterError::BadLength(length.to_string()));
        }
        if !conductivity.is_finite() || conductivity <= 0.0 {
            return Err(RouterError::BadConductivity(conductivity.to_string()));
        }
        for id in [a, b] {
            if !self.index.contains_key(id) {
                return Err(RouterError::UnknownNode(id.to_string()));
            }
        }
        if a == b {
            return Err(RouterError::SelfEdge(a.to_string()));
        }
        self.edges.push(Edge {
            a: a.to_string(),
            b: b.to_string(),
            conductivity,
            length,
        });
        Ok(())
    }

    /// **Route over the coordination layer's already-declared topology.**
    ///
    /// A [`Population`]'s `N(i)` is the graph and its declared `d(i,j)` **is**
    /// `L_ij` — the same numbers the concentration decay reads, used here as
    /// edge lengths. That is what a shared topology is for.
    pub fn over(
        population: &Population,
        spec: RouterSpec,
        initial_conductivity: f64,
    ) -> Result<Self, RouterError> {
        let mut r = Router::new(spec);
        for id in population.nodes() {
            r = r.with_node(id);
        }
        for a in population.nodes() {
            let ns = population.neighbours(a).expect("declared");
            for (b, d) in ns {
                // Symmetric neighbourhood — take each edge once.
                if a < b {
                    r.link(a, b, *d, initial_conductivity)?;
                }
            }
        }
        if r.edges.is_empty() {
            return Err(RouterError::NoEdges);
        }
        Ok(r)
    }

    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn conductivity(&self, a: &str, b: &str) -> Option<f64> {
        self.edges
            .iter()
            .find(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
            .map(|e| e.conductivity)
    }

    /// Edges still above the declared cutoff — what the body is actually using.
    pub fn carrying(&self) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.conductivity >= self.spec.prune_below).collect()
    }

    /// Edges that have decayed below it. **Reported, not removed.**
    pub fn pruned(&self) -> Vec<&Edge> {
        self.edges.iter().filter(|e| e.conductivity < self.spec.prune_below).collect()
    }

    /// `solve_flow(edges, sources)` — Kirchhoff conservation at every node.
    ///
    /// Builds the weighted Laplacian `L` with `w_ij = D_ij / L_ij`, grounds the
    /// sink at `p = 0` (the system is singular otherwise — pressures are only
    /// defined up to a constant), and solves `L·p = s`.
    pub fn solve_flow(
        &self,
        source: &str,
        sink: &str,
        rate: f64,
    ) -> Result<Vec<f64>, RouterError> {
        if !rate.is_finite() || rate <= 0.0 {
            return Err(RouterError::BadRate(rate.to_string()));
        }
        let si = *self.index.get(source).ok_or_else(|| RouterError::UnknownNode(source.into()))?;
        let ti = *self.index.get(sink).ok_or_else(|| RouterError::UnknownNode(sink.into()))?;
        if si == ti {
            return Err(RouterError::SourceIsSink(source.to_string()));
        }

        let n = self.nodes.len();
        let mut lap = vec![vec![0.0f64; n]; n];
        for e in &self.edges {
            let (i, j) = (self.index[&e.a], self.index[&e.b]);
            let w = e.weight();
            lap[i][i] += w;
            lap[j][j] += w;
            lap[i][j] -= w;
            lap[j][i] -= w;
        }

        let mut s = vec![0.0f64; n];
        s[si] = rate;
        s[ti] = -rate;

        // Ground the sink: drop its row and column.
        let keep: Vec<usize> = (0..n).filter(|k| *k != ti).collect();
        let m = keep.len();
        let mut a = vec![vec![0.0f64; m + 1]; m];
        for (r, &i) in keep.iter().enumerate() {
            for (c, &j) in keep.iter().enumerate() {
                a[r][c] = lap[i][j];
            }
            a[r][m] = s[i];
        }

        let reduced = gaussian_solve(&mut a).ok_or(RouterError::NoRoute {
            from: source.to_string(),
            to: sink.to_string(),
        })?;

        let mut p = vec![0.0f64; n];
        for (r, &i) in keep.iter().enumerate() {
            p[i] = reduced[r];
        }
        p[ti] = 0.0;
        Ok(p)
    }

    /// `Q_ij = (D_ij / L_ij)(p_i − p_j)`.
    pub fn flux(&self, pressures: &[f64]) -> Vec<f64> {
        self.edges
            .iter()
            .map(|e| {
                let (i, j) = (self.index[&e.a], self.index[&e.b]);
                e.weight() * (pressures[i] - pressures[j])
            })
            .collect()
    }

    /// `physarum_step` (§VIII) — solve, then let every edge respond to its own
    /// flux.
    pub fn physarum_step(
        &mut self,
        source: &str,
        sink: &str,
        rate: f64,
    ) -> Result<Step, RouterError> {
        let p = self.solve_flow(source, sink, rate)?;
        let flux = self.flux(&p);

        let mut largest = 0.0f64;
        for (e, q) in self.edges.iter_mut().zip(flux.iter()) {
            // dD/dt = f(|Q|) − D. Local, and only local.
            let delta = self.spec.dt * (self.spec.reinforcement.apply(*q) - e.conductivity);
            e.conductivity += delta;
            largest = largest.max(delta.abs());
        }

        Ok(Step { flux, largest_change: largest })
    }

    /// Iterate to a fixed point.
    ///
    /// **Reports whether it converged.** Running out of steps is not a settled
    /// network, and saying so beats returning a partial result that reads
    /// finished.
    pub fn converge(
        &mut self,
        source: &str,
        sink: &str,
        rate: f64,
        max_steps: usize,
        tolerance: f64,
    ) -> Result<Convergence, RouterError> {
        let mut last = f64::INFINITY;
        for step in 1..=max_steps {
            let s = self.physarum_step(source, sink, rate)?;
            last = s.largest_change;
            if last < tolerance {
                return Ok(Convergence { steps: step, converged: true, largest_change: last });
            }
        }
        Ok(Convergence { steps: max_steps, converged: false, largest_change: last })
    }
}

/// Gauss–Jordan with partial pivoting on an augmented `m × (m+1)` matrix.
///
/// `None` when the system is singular — which here means the source and sink
/// are not connected through any edge still carrying conductance.
fn gaussian_solve(a: &mut [Vec<f64>]) -> Option<Vec<f64>> {
    let m = a.len();
    if m == 0 {
        return None;
    }
    for col in 0..m {
        let (pivot, best) = (col..m)
            .map(|r| (r, a[r][col].abs()))
            .max_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal))?;
        if best < 1e-12 {
            return None;
        }
        a.swap(col, pivot);

        let d = a[col][col];
        for v in a[col].iter_mut().skip(col) {
            *v /= d;
        }

        // The pivot row is read while every other row is written, so take a
        // copy rather than fighting the borrow checker over a 4×5 matrix.
        let pivot_row = a[col].clone();
        for (r, row) in a.iter_mut().enumerate() {
            if r == col {
                continue;
            }
            let factor = row[col];
            if factor == 0.0 {
                continue;
            }
            for (v, p) in row.iter_mut().zip(pivot_row.iter()).skip(col) {
                *v -= factor * p;
            }
        }
    }
    Some((0..m).map(|r| a[r][m]).collect())
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum RouterError {
    #[error("the reinforcement exponent μ must be finite and > 0; got {0}")]
    BadExponent(String),
    #[error("dt must be in (0, 1]; got {0} — a step above 1 overshoots the fixed point and a step of 0 never moves")]
    BadStep(String),
    #[error("the prune threshold must be finite and >= 0; got {0}")]
    BadPruneThreshold(String),
    #[error("an edge length must be finite and > 0; got {0}")]
    BadLength(String),
    #[error("a starting conductivity must be finite and > 0; got {0} — an edge that starts at zero can never carry the flux that would reinforce it")]
    BadConductivity(String),
    #[error("the flow rate must be finite and > 0; got {0}")]
    BadRate(String),
    #[error("'{0}' is not a node in this network")]
    UnknownNode(String),
    #[error("'{0}' cannot be an edge to itself")]
    SelfEdge(String),
    #[error("source and sink are both '{0}' — there is nothing to route")]
    SourceIsSink(String),
    #[error("a router over a population with no declared edges has nothing to route on")]
    NoEdges,
    // NOTE: not `source`/`sink` as field names — thiserror treats a field
    // literally called `source` as the error's cause, not as a format argument.
    #[error("no route from '{from}' to '{to}' — refused rather than reported as zero flow, because 'no flow needed' and 'no route exists' are different answers")]
    NoRoute { from: String, to: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::population::{Population, PopulationSpec};

    fn spec() -> RouterSpec {
        RouterSpec::new(0.5, Reinforcement::power(1.0).unwrap(), 0.05).unwrap()
    }

    /// Source and sink joined by two disjoint routes: a short one through `a`
    /// (total length 2) and a long one through `b` (total length 6). Equal
    /// starting conductivity, so nothing but the flow distinguishes them.
    fn two_routes() -> Router {
        Router::new(spec())
            .with_node("s")
            .with_node("a")
            .with_node("b")
            .with_node("t")
            .linking("s", "a", 1.0, 1.0)
            .linking("a", "t", 1.0, 1.0)
            .linking("s", "b", 3.0, 1.0)
            .linking("b", "t", 3.0, 1.0)
    }

    // ── the flow solve ──────────────────────────────────────────────────────

    #[test]
    fn flow_is_conserved_at_every_interior_node() {
        // Kirchhoff's law, which is what makes the solve physics rather than
        // planning: whatever enters `a` leaves it.
        let r = two_routes();
        let p = r.solve_flow("s", "t", 1.0).unwrap();
        let q = r.flux(&p);

        // Edges are (s,a), (a,t), (s,b), (b,t) in declaration order.
        assert!((q[0] - q[1]).abs() < 1e-9, "what enters `a` leaves it");
        assert!((q[2] - q[3]).abs() < 1e-9, "and the same through `b`");
        assert!((q[0] + q[2] - 1.0).abs() < 1e-9, "and the source injects exactly the rate");
    }

    #[test]
    fn the_shorter_route_carries_more_flow_before_any_adaptation() {
        // Equal conductivity, different length ⇒ different resistance. This is
        // the asymmetry the reinforcement then amplifies.
        let r = two_routes();
        let q = r.flux(&r.solve_flow("s", "t", 1.0).unwrap());
        assert!(q[0].abs() > q[2].abs(), "short route: {} vs long: {}", q[0], q[2]);
        // Resistance 2 vs 6 ⇒ a 3:1 split.
        assert!((q[0].abs() / q[2].abs() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_disconnected_sink_is_refused_not_zeroed() {
        // "No flow needed" and "no route exists" are different answers.
        let r = Router::new(spec())
            .with_node("s")
            .with_node("t")
            .with_node("island")
            .linking("s", "t", 1.0, 1.0);
        assert!(matches!(
            r.solve_flow("s", "island", 1.0),
            Err(RouterError::NoRoute { .. })
        ));
    }

    #[test]
    fn a_source_that_is_also_the_sink_is_refused() {
        let r = two_routes();
        assert!(matches!(r.solve_flow("s", "s", 1.0), Err(RouterError::SourceIsSink(_))));
    }

    // ── ★ the adaptation ────────────────────────────────────────────────────

    #[test]
    fn flow_reinforces_the_load_bearing_channel_and_prunes_the_unused_one() {
        // ★★ THE PROOF OF VALUE, and §VIII's own claim: the same dynamics
        // provably solve shortest path. Both routes start identical; only the
        // flow through them differs, and that is enough.
        let mut r = two_routes();
        let c = r.converge("s", "t", 1.0, 500, 1e-9).unwrap();
        assert!(c.converged, "settled in {} steps", c.steps);

        let short = r.conductivity("s", "a").unwrap();
        let long = r.conductivity("s", "b").unwrap();
        assert!(short > long, "short {short} vs long {long}");

        let carrying: Vec<&str> = r.carrying().iter().map(|e| e.b.as_str()).collect();
        let pruned: Vec<&str> = r.pruned().iter().map(|e| e.b.as_str()).collect();
        assert!(carrying.contains(&"a"), "★ the short route carries the load");
        assert!(pruned.contains(&"b"), "★ and the long one is pruned");
    }

    #[test]
    fn the_reinforcement_sees_only_its_own_edges_flux() {
        // ★ Where "no global view" actually lives. f is a function of one
        // number, and that number is this edge's flux.
        let f = Reinforcement::power(1.0).unwrap();
        assert_eq!(f.apply(2.0), 2.0);
        assert_eq!(f.apply(-2.0), 2.0, "direction does not matter, only magnitude");
        assert!(f.is_monotone());

        let sat = Reinforcement::saturating(1.0).unwrap();
        assert!(sat.apply(1000.0) < 1.0, "a channel cannot thicken without bound");
        assert!(sat.apply(2.0) > sat.apply(1.0), "and it is still monotone");
    }

    #[test]
    fn an_unused_channel_decays_toward_zero_without_ever_reaching_it() {
        // dD/dt = f(0) − D = −D with dt < 1 is asymptotic. Which is exactly why
        // "pruned" is a reading at a declared cutoff, not a deletion.
        let mut r = two_routes();
        r.converge("s", "t", 1.0, 500, 1e-12).unwrap();
        let long = r.conductivity("s", "b").unwrap();
        assert!(long > 0.0, "never exactly zero: {long}");
        assert!(long < r.spec.prune_below, "but below the cutoff");
        assert_eq!(r.edges().len(), 4, "★ and nothing was deleted");
    }

    #[test]
    fn a_run_that_does_not_settle_says_so() {
        let mut r = two_routes();
        let c = r.converge("s", "t", 1.0, 1, 1e-12).unwrap();
        assert!(!c.converged, "a partial run must not read like a settled network");
        assert_eq!(c.steps, 1);
    }

    #[test]
    fn equal_routes_stay_equal() {
        // The symmetric case: nothing distinguishes the two, so neither wins.
        // A router that broke the tie would be choosing, which is the one thing
        // it must not do.
        let mut r = Router::new(spec())
            .with_node("s")
            .with_node("a")
            .with_node("b")
            .with_node("t")
            .linking("s", "a", 1.0, 1.0)
            .linking("a", "t", 1.0, 1.0)
            .linking("s", "b", 1.0, 1.0)
            .linking("b", "t", 1.0, 1.0);
        r.converge("s", "t", 1.0, 500, 1e-9).unwrap();

        let x = r.conductivity("s", "a").unwrap();
        let y = r.conductivity("s", "b").unwrap();
        assert!((x - y).abs() < 1e-9, "★ nothing here chooses: {x} vs {y}");
    }

    // ── composed, not rebuilt ───────────────────────────────────────────────

    #[test]
    fn a_router_runs_over_the_populations_declared_topology() {
        // ★ The population declared d(i,j) for concentration decay; the same
        // numbers are edge lengths here. One topology, two readings.
        let p = Population::new(PopulationSpec::new(1.0, 0.0).unwrap())
            .with_node("s")
            .with_node("a")
            .with_node("t")
            .linking("s", "a", 2.0)
            .linking("a", "t", 4.0);

        let r = Router::over(&p, spec(), 1.0).unwrap();
        assert_eq!(r.nodes().len(), 3);
        assert_eq!(r.edges().len(), 2, "each symmetric neighbourhood edge taken once");

        // `over` stores each symmetric pair once with a < b, so match either
        // orientation rather than assuming one.
        let sa = r
            .edges()
            .iter()
            .find(|e| (e.a == "s" && e.b == "a") || (e.a == "a" && e.b == "s"))
            .expect("the s–a edge");
        assert_eq!(sa.length, 2.0, "★ the population's d(i,j) IS L_ij");
        let at = r
            .edges()
            .iter()
            .find(|e| (e.a == "a" && e.b == "t") || (e.a == "t" && e.b == "a"))
            .expect("the a–t edge");
        assert_eq!(at.length, 4.0, "and the longer one carries its own declared length");
    }

    #[test]
    fn a_population_with_no_edges_has_nothing_to_route_on() {
        let p = Population::new(PopulationSpec::new(1.0, 0.0).unwrap()).with_node("alone");
        assert!(matches!(Router::over(&p, spec(), 1.0), Err(RouterError::NoEdges)));
    }

    // ── declared parameters ─────────────────────────────────────────────────

    #[test]
    fn the_declared_parameters_are_checked() {
        assert!(matches!(Reinforcement::power(0.0), Err(RouterError::BadExponent(_))));
        assert!(matches!(
            RouterSpec::new(0.0, Reinforcement::power(1.0).unwrap(), 0.1),
            Err(RouterError::BadStep(_))
        ));
        assert!(
            matches!(
                RouterSpec::new(1.5, Reinforcement::power(1.0).unwrap(), 0.1),
                Err(RouterError::BadStep(_))
            ),
            "a step above 1 overshoots the fixed point"
        );
    }

    #[test]
    fn an_edge_that_starts_at_zero_could_never_be_reinforced() {
        let mut r = Router::new(spec()).with_node("a").with_node("b");
        assert!(matches!(r.link("a", "b", 1.0, 0.0), Err(RouterError::BadConductivity(_))));
    }
}
