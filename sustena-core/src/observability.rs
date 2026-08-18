//! Observability — what can be known (MONITOR §I · MON-1).
//!
//! > *"Every state variable worth governing must be reachable from a measurable
//! > output — unmeasured state is ungoverned state."*
//!
//! ## ★★ The observability matrix is a declined import, like the Kalman filter
//!
//! §I's literal test is the observability matrix
//! `O = [C; CA; CA²; …; CAⁿ⁻¹]`, with `rank(O) = n` for complete
//! observability. **That is not built, and its absence is a position rather
//! than a gap** — the same position MON-2 takes on the Kalman filter, for the
//! same reason MON-11's ratified fit caveat gives:
//!
//! > *"The observability and Kalman apparatus of §I–II assumes a continuous
//! > linear system `ẋ = Ax + Bu`. Sustena's state is discrete, typed, and
//! > event-sourced [...] the observability matrix and the Kalman filter are a
//! > loose import, not a literal fit."*
//!
//! ★ And the concrete reason, rather than only the categorical one:
//! **`CAᵏ` requires a single `A` to take powers of, and there isn't one.** The
//! "dynamics" here is a *choice* — which operator someone runs — not a fixed
//! linear map. Picking one operator's effect to stand in for `A` would be a
//! fabrication, and MON-1's own row says it: *a rough answer to a rank question
//! is worse than none.*
//!
//! ## ★★ What is built: the discrete analogue, and it is a translation
//!
//! A dimension is observable iff its value can be **inferred from a measurable
//! output** — so the question is *reachability to a measurement over the
//! operator graph*, and the correspondence to the matrix is exact rather than
//! loose:
//!
//! | continuous-linear | discrete, event-sourced |
//! |---|---|
//! | `C` — what the sensors read | dimensions a declared source measures |
//! | `A` — one step of the dynamics | one operator: reads `X`, writes `Y` ⇒ `X → Y` |
//! | `CAᵏ` — the k-th block row | a measurement reachable in `k` operator hops |
//! | `rank(O) = n` | every dimension reaches some measurement |
//!
//! The edge direction is the part worth stating carefully: an operator that
//! **reads `X` and writes `Y`** makes `Y` carry information about `X`, so
//! observing `Y` tells you about `X`. Information flows `X → Y`, and `X` is
//! observable when some path from it lands on something measured.
//!
//! ## ★ Conservative where the effects are not summarised, and it says so
//!
//! Writes come from an operator's [`EffectSummary`], which is **optional and
//! absent by default** (OP-3). An operator without one contributes **no edges**
//! — so the analysis can report a dimension unobservable that a summary would
//! have shown observable.
//!
//! That is the safe direction, and the same discipline the Goodhart guard takes
//! with support matching: **surfacing a design finding that might be spurious
//! beats hiding one that is real.** But it is not left implicit —
//! [`Observability::Unobservable`] carries the operators whose summaries were
//! missing, and [`Observability::is_conservative`] says whether the verdict
//! rests on them. *Unobservable* and *we could not see* stay distinguishable.
//!
//! ★★ **Over today's registry that boundary is the whole picture**, and a test
//! asserts it so it cannot quietly stop being true: no shipped operator carries
//! an `EffectSummary` (OP-3's own finding), so the graph has no propagation
//! edges at all and every non-measured dimension reads conservative.
//!
//! ## ★ Where this meets MON-8, and how it differs from `ungoverned()`
//!
//! §I's *partial observability → a distribution, not a point* is already built:
//! that distribution is MON-8's belief. MON-1 says **which** dimensions need
//! one — [`ObservabilityReport::requiring_belief`] is precisely the set where a
//! point estimate would be a claim nobody can check.
//!
//! And the distinction from `BeliefTracker::ungoverned()` is worth keeping
//! sharp rather than calling this an extension of it:
//!
//! - `ungoverned()` is a **runtime** question — *nothing has spoken about this
//!   yet*. It can be answered by waiting.
//! - This is a **structural** one — *nothing ever could*. Waiting will not
//!   help, because there is no path.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::compose::EffectSummary;
use crate::goodhart::referenced_dimensions;
use crate::operator::{OperatorMeta, Registry};
use crate::predicate::parse_predicate;

/// What a declared source measures directly — §I's `C`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasuredBy {
    pub source_id: String,
    /// Root dimension names this source reads directly.
    pub dimensions: BTreeSet<String>,
}

impl MeasuredBy {
    pub fn new(source_id: impl Into<String>, dimensions: &[&str]) -> Self {
        Self {
            source_id: source_id.into(),
            dimensions: dimensions.iter().map(|d| (*d).to_string()).collect(),
        }
    }
}

/// What can go wrong building the graph.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ObservabilityError {
    #[error("no source measures anything — with an empty `C` nothing is observable, which is a fact about the declaration rather than about the state")]
    NothingMeasured,
}

/// How a dimension's value can be known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Observability {
    /// A declared source reads it directly — §I's `C` row.
    Measured { by: Vec<String> },
    /// Not measured, but a chain of summarised operators carries its value to
    /// something that is. `hops` is which block row `CAᵏ` it would appear in.
    Inferable { via: Vec<String>, hops: usize, reaches: String },
    /// ★ No path to any measurement was found — **and the operators whose
    /// effect summaries were missing are named**, because a verdict that rests
    /// on them is conservative rather than definitive.
    Unobservable { unsummarised: Vec<String> },
}

impl Observability {
    pub fn is_observable(&self) -> bool {
        !matches!(self, Observability::Unobservable { .. })
    }

    /// ★ Is this verdict *conservative* — reached partly because some operator
    /// did not say what it writes?
    ///
    /// A conservative `Unobservable` may become observable once the effects are
    /// summarised. One that is not conservative will not.
    pub fn is_conservative(&self) -> bool {
        matches!(self, Observability::Unobservable { unsummarised } if !unsummarised.is_empty())
    }

    /// ★ §I's consequence: an unobservable dimension is exactly one where a
    /// point estimate would be a claim nobody can check, so the honest answer
    /// is MON-8's belief distribution.
    pub fn requires_belief(&self) -> bool {
        !self.is_observable()
    }

    pub fn describe(&self, dimension: &str) -> String {
        match self {
            Observability::Measured { by } => {
                format!("'{dimension}' is measured directly by {}", by.join(", "))
            }
            Observability::Inferable { via, hops, reaches } => format!(
                "'{dimension}' is inferable in {hops} operator hop(s) — its value reaches the \
                 measured dimension '{reaches}' through {}",
                via.join(" → ")
            ),
            Observability::Unobservable { unsummarised } if unsummarised.is_empty() => format!(
                "'{dimension}' is UNOBSERVABLE: no declared source reads it and no operator \
                 carries its value to one. Unmeasured state is ungoverned state"
            ),
            Observability::Unobservable { unsummarised } => format!(
                "'{dimension}' reads UNOBSERVABLE, but conservatively — {} operator(s) declare no \
                 effect summary ({}), so a path may exist that this analysis cannot see. \
                 Surfaced rather than assumed away",
                unsummarised.len(),
                unsummarised.join(", ")
            ),
        }
    }
}

/// The operator/source graph §I's matrix is replaced by.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityGraph {
    /// `X → {Y}`: an operator reads `X` and writes `Y`, so `Y` carries
    /// information about `X`.
    edges: BTreeMap<String, BTreeSet<String>>,
    /// Which operator supplied each edge, for the trace.
    via: BTreeMap<(String, String), String>,
    /// dimension → the sources that read it directly.
    measured: BTreeMap<String, BTreeSet<String>>,
    /// ★ Operators that declared no effect summary. Their absence is why a
    /// verdict can be conservative.
    unsummarised: BTreeSet<String>,
}

impl ObservabilityGraph {
    /// Build the graph from the real registry and the declared sources.
    pub fn build(registry: &Registry, sources: &[MeasuredBy]) -> Result<Self, ObservabilityError> {
        let mut measured: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for s in sources {
            for d in &s.dimensions {
                measured.entry(d.clone()).or_default().insert(s.source_id.clone());
            }
        }
        if measured.is_empty() {
            return Err(ObservabilityError::NothingMeasured);
        }

        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut via: BTreeMap<(String, String), String> = BTreeMap::new();
        let mut unsummarised = BTreeSet::new();

        for name in registry.names() {
            let Some(meta) = registry.get(&name) else { continue };
            match &meta.effect {
                // ★ No summary — no edges. The safe direction: it can make a
                // dimension read unobservable that is in fact observable, which
                // surfaces a finding rather than hiding one.
                None => {
                    unsummarised.insert(meta.name.to_string());
                }
                Some(effect) => {
                    let writes = written_dimensions(effect);
                    for r in read_dimensions(meta) {
                        for w in &writes {
                            if &r != w {
                                edges.entry(r.clone()).or_default().insert(w.clone());
                                via.entry((r.clone(), w.clone()))
                                    .or_insert_with(|| meta.name.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(Self { edges, via, measured, unsummarised })
    }

    /// Operators that declared no effect summary.
    pub fn unsummarised(&self) -> Vec<String> {
        self.unsummarised.iter().cloned().collect()
    }

    /// Dimensions a declared source reads directly.
    pub fn measured_dimensions(&self) -> Vec<String> {
        self.measured.keys().cloned().collect()
    }

    /// ★ How this dimension can be known — the discrete analogue of asking
    /// which block row of `O` it appears in.
    pub fn observability_of(&self, dimension: &str) -> Observability {
        if let Some(by) = self.measured.get(dimension) {
            return Observability::Measured { by: by.iter().cloned().collect() };
        }

        // Breadth-first along `reads → writes`, so the first measurement found
        // is the fewest operator hops away.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut queue: VecDeque<(&str, usize, Vec<String>)> = VecDeque::new();
        seen.insert(dimension);
        queue.push_back((dimension, 0, Vec::new()));

        while let Some((cur, hops, trail)) = queue.pop_front() {
            for next in self.edges.get(cur).into_iter().flatten() {
                if !seen.insert(next.as_str()) {
                    continue;
                }
                let mut trail = trail.clone();
                if let Some(op) = self.via.get(&(cur.to_string(), next.clone())) {
                    trail.push(op.clone());
                }
                if self.measured.contains_key(next) {
                    return Observability::Inferable {
                        via: trail,
                        hops: hops + 1,
                        reaches: next.clone(),
                    };
                }
                queue.push_back((next.as_str(), hops + 1, trail));
            }
        }

        Observability::Unobservable { unsummarised: self.unsummarised() }
    }

    /// Read every dimension someone declared worth governing.
    pub fn report(&self, governed: &[&str]) -> ObservabilityReport {
        ObservabilityReport {
            findings: governed
                .iter()
                .map(|d| ((*d).to_string(), self.observability_of(d)))
                .collect(),
        }
    }
}

/// §I's question answered over a declared set of governed dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservabilityReport {
    pub findings: Vec<(String, Observability)>,
}

impl ObservabilityReport {
    /// ★★ **The design finding**: declared worth governing, and not reachable
    /// from any measurement. *Unmeasured state is ungoverned state.*
    pub fn declared_but_unobservable(&self) -> Vec<&str> {
        self.findings
            .iter()
            .filter(|(_, o)| !o.is_observable())
            .map(|(d, _)| d.as_str())
            .collect()
    }

    /// ★ Which dimensions **must** fall back to a MON-8 belief distribution,
    /// because a point estimate there would be a claim nobody can check.
    pub fn requiring_belief(&self) -> Vec<&str> {
        self.declared_but_unobservable()
    }

    /// Of the unobservable findings, those that rest on a missing effect
    /// summary — and so may resolve once the effects are declared.
    pub fn conservative(&self) -> Vec<&str> {
        self.findings
            .iter()
            .filter(|(_, o)| o.is_conservative())
            .map(|(d, _)| d.as_str())
            .collect()
    }

    /// The discrete analogue of `rank(O) = n`: is every governed dimension
    /// reachable from a measurement?
    pub fn completely_observable(&self) -> bool {
        self.findings.iter().all(|(_, o)| o.is_observable())
    }
}

// ── reading the operator declarations ──────────────────────────────────────

/// Root dimension names an operator's declared predicates read.
///
/// A constraint that does not parse contributes nothing — conservative in the
/// same direction as a missing effect summary.
fn read_dimensions(meta: &OperatorMeta) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for expr in meta.constraints.iter().chain(meta.post_constraints.iter()) {
        if let Ok(p) = parse_predicate(expr) {
            out.extend(referenced_dimensions(&p));
        }
    }
    out
}

/// Root dimension names an effect summary writes.
///
/// ★ A [`Change::Opaque`](crate::compose::Change) still counts: *opaque* is
/// about the value, not about whether the path was written, so the edge is
/// real even where `wp` cannot fold it.
fn written_dimensions(effect: &EffectSummary) -> BTreeSet<String> {
    effect
        .changes
        .iter()
        .map(|(path, _)| path.split('.').next().unwrap_or(path).to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::Change;
    use crate::operator::meta::Protocol;
    use crate::operator::OperatorResult;
    use serde_json::json;

    fn noop(
        _s: &mut crate::State,
        _p: &serde_json::Map<String, serde_json::Value>,
        _e: &mut Vec<crate::operator::EmittedEvent>,
        _m: &mut Vec<crate::flow::Movement>,
    ) -> OperatorResult {
        OperatorResult::ok(json!({}))
    }

    fn op(
        name: &'static str,
        reads: &[&str],
        writes: Option<&[(&str, Change)]>,
    ) -> OperatorMeta {
        OperatorMeta {
            name,
            description: "",
            params: vec![],
            constraints: reads.iter().map(|r| format!("{r} >= 0")).collect(),
            post_constraints: vec![],
            side_effects: vec![],
            pawa_cost: 0,
            protocol: Protocol::Rpc,
            min_privilege: 0,
            effect: writes.map(|ws| {
                ws.iter().fold(EffectSummary::new(), |e, (p, c)| e.with(p, c.clone()))
            }),
            run: noop,
        }
    }

    /// `sensor.raw → derived.level → finances.liquid`, with only the last
    /// measured — so the first two are inferable at 2 and 1 hops.
    fn chain_registry() -> Registry {
        let mut r = Registry::new();
        r.register(op(
            "a.lift",
            &["sensor"],
            Some(&[("derived.level", Change::SetTo(json!(1)))]),
        ));
        r.register(op(
            "b.carry",
            &["derived"],
            Some(&[("finances.liquid", Change::Opaque)]),
        ));
        r
    }

    fn sources() -> Vec<MeasuredBy> {
        vec![MeasuredBy::new("mpesa", &["finances"])]
    }

    // ── the discrete analogue ──────────────────────────────────────────────

    #[test]
    fn a_directly_measured_dimension_is_the_c_row() {
        let g = ObservabilityGraph::build(&chain_registry(), &sources()).unwrap();
        match g.observability_of("finances") {
            Observability::Measured { by } => assert_eq!(by, ["mpesa"]),
            other => panic!("{other:?}"),
        }
        assert_eq!(g.measured_dimensions(), ["finances"]);
    }

    #[test]
    fn a_dimension_one_operator_from_a_measurement_is_inferable_at_one_hop() {
        // ★ `CA¹`: `derived` is read by `b.carry`, which writes `finances`.
        let g = ObservabilityGraph::build(&chain_registry(), &sources()).unwrap();
        match g.observability_of("derived") {
            Observability::Inferable { hops, reaches, via } => {
                assert_eq!(hops, 1);
                assert_eq!(reaches, "finances");
                assert_eq!(via, ["b.carry"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_chain_of_two_operators_is_inferable_at_two_hops() {
        // ★ `CA²` — the block row §I would have had to compute.
        let g = ObservabilityGraph::build(&chain_registry(), &sources()).unwrap();
        match g.observability_of("sensor") {
            Observability::Inferable { hops, via, reaches } => {
                assert_eq!(hops, 2);
                assert_eq!(via, ["a.lift", "b.carry"], "the chain that carries it");
                assert_eq!(reaches, "finances");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn breadth_first_reports_the_fewest_hops_not_the_first_path_found() {
        // A shortcut exists; the reading must take it, since "which block row"
        // is a question about the smallest k.
        let mut r = chain_registry();
        r.register(op("c.shortcut", &["sensor"], Some(&[("finances.other", Change::Opaque)])));
        let g = ObservabilityGraph::build(&r, &sources()).unwrap();
        match g.observability_of("sensor") {
            Observability::Inferable { hops, .. } => assert_eq!(hops, 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_dimension_nothing_carries_is_unobservable() {
        let mut r = chain_registry();
        r.register(op("d.island", &["orphan"], Some(&[("orphan.other", Change::Opaque)])));
        let g = ObservabilityGraph::build(&r, &sources()).unwrap();

        let o = g.observability_of("orphan");
        assert!(!o.is_observable());
        assert!(!o.is_conservative(), "every operator here declared its effects");
        assert!(o.describe("orphan").contains("ungoverned state"));
    }

    #[test]
    fn a_cycle_does_not_hang_the_search() {
        let mut r = Registry::new();
        r.register(op("x.to_y", &["x"], Some(&[("y.v", Change::Opaque)])));
        r.register(op("y.to_x", &["y"], Some(&[("x.v", Change::Opaque)])));
        let g = ObservabilityGraph::build(&r, &sources()).unwrap();
        assert!(!g.observability_of("x").is_observable());
    }

    // ── ★ conservative where the effects are not summarised ────────────────

    #[test]
    fn an_operator_with_no_effect_summary_contributes_no_edges_and_is_named() {
        // ★ The safe direction: a dimension can read unobservable that a
        // summary would have shown observable — and the verdict says so rather
        // than presenting itself as definitive.
        let mut r = Registry::new();
        r.register(op("a.opaque", &["sensor"], None));
        let g = ObservabilityGraph::build(&r, &sources()).unwrap();

        let o = g.observability_of("sensor");
        assert!(!o.is_observable());
        assert!(o.is_conservative(), "it rests on a missing summary");
        match &o {
            Observability::Unobservable { unsummarised } => {
                assert_eq!(unsummarised, &["a.opaque".to_string()])
            }
            other => panic!("{other:?}"),
        }
        assert!(o.describe("sensor").contains("conservatively"));
        assert!(o.describe("sensor").contains("may exist that this analysis cannot see"));
    }

    #[test]
    fn declaring_the_effect_turns_the_same_dimension_observable() {
        // The pair: the difference between the two readings is a declaration.
        let mut without = Registry::new();
        without.register(op("a.lift", &["sensor"], None));
        assert!(!ObservabilityGraph::build(&without, &sources())
            .unwrap()
            .observability_of("sensor")
            .is_observable());

        let mut with = Registry::new();
        with.register(op("a.lift", &["sensor"], Some(&[("finances.liquid", Change::Opaque)])));
        assert!(ObservabilityGraph::build(&with, &sources())
            .unwrap()
            .observability_of("sensor")
            .is_observable());
    }

    #[test]
    fn an_opaque_change_still_makes_an_edge_because_opaque_is_about_the_value() {
        let mut r = Registry::new();
        r.register(op("a.lift", &["sensor"], Some(&[("finances.liquid", Change::Opaque)])));
        let g = ObservabilityGraph::build(&r, &sources()).unwrap();
        assert!(g.observability_of("sensor").is_observable());
    }

    // ── ★★ the finding over the REAL registry ──────────────────────────────

    #[test]
    fn over_the_real_registry_every_verdict_is_conservative() {
        // ★★ OP-3's finding, seen from here: no shipped operator declares an
        // `EffectSummary`, so the graph has no edges at all. Asserted so it
        // cannot quietly stop being true — a future summarised operator breaks
        // this and forces a look.
        let g = ObservabilityGraph::build(&Registry::with_builtins(), &sources()).unwrap();
        assert!(!g.unsummarised().is_empty(), "the whole registry is unsummarised");

        let o = g.observability_of("tasks");
        assert!(!o.is_observable());
        assert!(o.is_conservative(), "and it says the verdict rests on that");
    }

    // ── the report, and §I's implication ───────────────────────────────────

    #[test]
    fn a_governed_but_unobservable_dimension_is_a_design_finding() {
        let mut r = chain_registry();
        r.register(op("d.island", &["orphan"], Some(&[("orphan.other", Change::Opaque)])));
        let g = ObservabilityGraph::build(&r, &sources()).unwrap();
        let report = g.report(&["finances", "sensor", "orphan"]);

        assert_eq!(report.declared_but_unobservable(), ["orphan"]);
        assert!(!report.completely_observable(), "the discrete rank(O) < n");
    }

    #[test]
    fn everything_reachable_reads_as_completely_observable() {
        let g = ObservabilityGraph::build(&chain_registry(), &sources()).unwrap();
        assert!(g.report(&["finances", "derived", "sensor"]).completely_observable());
    }

    #[test]
    fn an_unobservable_dimension_is_exactly_one_that_needs_a_belief() {
        // ★ §I's *partial observability → a distribution, not a point*, wired to
        // where that distribution lives (MON-8).
        let mut r = chain_registry();
        r.register(op("d.island", &["orphan"], Some(&[("orphan.other", Change::Opaque)])));
        let g = ObservabilityGraph::build(&r, &sources()).unwrap();
        let report = g.report(&["finances", "orphan"]);

        assert_eq!(report.requiring_belief(), ["orphan"]);
        assert!(g.observability_of("orphan").requires_belief());
        assert!(!g.observability_of("finances").requires_belief());
    }

    #[test]
    fn with_nothing_measured_the_graph_refuses_rather_than_reporting_all_unobservable() {
        // An empty `C` makes every answer trivially "unobservable", which is a
        // fact about the declaration rather than about the state — so it is an
        // error, not a report full of findings.
        assert_eq!(
            ObservabilityGraph::build(&chain_registry(), &[]),
            Err(ObservabilityError::NothingMeasured)
        );
    }
}
