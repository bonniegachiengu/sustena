//! The Signal primitive — a relayed, refractory pulse
//! (Multiparty §III + Additions · MUL-4, MUL-15).
//!
//! > A quorum needs the signal to *travel*, not just pool locally.
//!
//! ```text
//! STATES: RESTING -> EXCITED -> REFRACTORY -> RESTING
//! FUNCTION relay(node, inbound):
//!     IF node.state == RESTING AND inbound >= θ_fire:
//!         node.state = EXCITED; emit_to(node.neighbors); start_refractory(node)
//! ```
//!
//! **This is the shared primitive** (MUL-15). Monitor (§2), Controller (§3) and
//! Tenet (§4) all ride it and cross-reference §III rather than restating it, so
//! it is built once, here — MON-13's harmonics, CTL-3's OODA loop and TEN-8's
//! signal-triggered re-inversion consume *this* type rather than three local
//! reimplementations that would later need reconciling.
//!
//! ## The refractory term is the load-bearing half, and the code says so
//!
//! The article is explicit that refractoriness is what makes the wave
//! directional and what stops a relayed message re-exciting its sender. That
//! claim is falsifiable, so this module is built to let it fail if it were
//! false: **[`Field::connect`] is symmetric.** Every edge runs both ways, no
//! topology anywhere prefers a direction, and the wave is *still* directional.
//! Directed edges would have made the wave directional for a reason that had
//! nothing to do with refractoriness — and the article's central claim about
//! §III would have gone untested while appearing to hold.
//!
//! A node that received inbound and declined to fire **says why**
//! ([`Refusal`]), so `RefusalReason::Refractory` is the no-echo property
//! *observed*, not inferred from the absence of a second excitation.
//!
//! ## Both parameters are declared, and a zero window is refused
//!
//! `θ_fire` and the refractory window are spec fields, the same discipline as
//! the region weights and the CUSUM parameters. [`SignalSpec::new`] refuses:
//!
//! - **`refractory_ticks == 0`** — an excitable medium with no refractory
//!   period is not an excitable medium. The excitation sloshes back into the
//!   elements that just fired and the field rings forever. The article calls
//!   this term load-bearing; a spec that zeroes it has deleted the mechanism
//!   while keeping the vocabulary.
//! - **`θ_fire <= 0`** — a threshold everything clears is not a threshold.
//!   Every node fires on any inbound at all, which is a broadcast wearing a
//!   relay's clothes.
//!
//! ## Logical time, because core has no clock
//!
//! Propagation advances one node per [`Field::step`]. There is no wall clock
//! here and no I/O — the caller drives the ticks, exactly as `detect.rs` takes
//! its readings and `approval.rs` takes its `now`. A refractory window is
//! counted in ticks, which is also what makes a wave reproducible: the same
//! field stimulated the same way recruits the same nodes in the same order.
//!
//! ## Scope — the primitive, not the population
//!
//! What is built here is the pulse and its relay semantics. [`Field`] is the
//! minimal substrate a relay needs to exist at all (nodes, symmetric adjacency,
//! a tick) and is deliberately **not** the population object.
//!
//! **Named follow-on:** MUL-1/MUL-2 — the `Population` with aggregation
//! thresholds, quorum sensing and cascade (§I, §II). That layer asks *how many
//! have committed and does that cross a quorum*; this one asks *does the news
//! reach them at all*. The second question is prior to the first, which is why
//! it is built first.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

/// A node's position in the excitation cycle (§III).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Able to fire if driven past `θ_fire`.
    Resting,
    /// Firing. Emits to its neighbours on the tick it enters this phase.
    Excited,
    /// **Cannot fire, whatever arrives.** The half the wave's direction rests
    /// on.
    Refractory,
}

/// What travels. Carries where it started and how far it has come, so a
/// consumer can tell a local event from one that reached it across the field.
#[derive(Debug, Clone, PartialEq)]
pub struct Pulse {
    /// The node the wave started at — not the neighbour it arrived from.
    pub origin: String,
    /// Relays since the origin. `0` is an external stimulus.
    pub hops: usize,
    pub strength: f64,
}

/// The declared parameters of a medium.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalSpec {
    /// `θ_fire` — inbound must reach this for a resting node to fire.
    pub theta_fire: f64,
    /// Ticks a node spends unable to fire after excitation.
    pub refractory_ticks: usize,
    /// Ticks a node spends excited. The pseudocode fires and starts the
    /// refractory period in one line; one tick of `EXCITED` is what makes the
    /// three-state machine §III names observable rather than notional.
    pub excited_ticks: usize,
    /// What one firing neighbour delivers.
    ///
    /// Declared rather than hardcoded, for the same reason the region weights
    /// and the CUSUM parameters are. **At the default of 1.0, `θ_fire` reads
    /// directly as a count of concurrently-firing neighbours** — which is the
    /// seam to §II's threshold commitment, the same number read as *how many
    /// of my neighbours are in*.
    ///
    /// An earlier draft emitted `θ_fire` itself. That is worth recording as a
    /// trap rather than a typo: it makes one neighbour sufficient at **every**
    /// threshold, so `θ_fire` becomes inert while still appearing in the spec
    /// and being checked at construction. A test would have to compare two
    /// different thresholds to notice.
    pub emit_strength: f64,
}

impl SignalSpec {
    /// Refuses a spec that has deleted the mechanism it is named for.
    pub fn new(theta_fire: f64, refractory_ticks: usize) -> Result<Self, SignalError> {
        if !theta_fire.is_finite() || theta_fire <= 0.0 {
            return Err(SignalError::BadThreshold(theta_fire.to_string()));
        }
        if refractory_ticks == 0 {
            return Err(SignalError::NoRefractoryPeriod);
        }
        Ok(Self { theta_fire, refractory_ticks, excited_ticks: 1, emit_strength: 1.0 })
    }

    /// Change what one firing neighbour delivers.
    pub fn with_emit_strength(mut self, strength: f64) -> Result<Self, SignalError> {
        if !strength.is_finite() || strength <= 0.0 {
            return Err(SignalError::BadStrength(strength.to_string()));
        }
        self.emit_strength = strength;
        Ok(self)
    }

    pub fn with_excited(mut self, ticks: usize) -> Result<Self, SignalError> {
        if ticks == 0 {
            return Err(SignalError::NoExcitedPeriod);
        }
        self.excited_ticks = ticks;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct NodeState {
    phase: Phase,
    /// Ticks left in the current phase.
    remaining: usize,
    /// Inbound accumulated for the next decision. Several neighbours firing at
    /// once **sum** — spatial summation, which is what lets `θ_fire > 1` mean
    /// "only if more than one of my neighbours agrees".
    inbox: f64,
    /// The pulse that arrived, kept so a relay carries the origin forward
    /// rather than rewriting it to the last hop.
    arrived: Option<Pulse>,
}

impl NodeState {
    fn resting() -> Self {
        Self { phase: Phase::Resting, remaining: 0, inbox: 0.0, arrived: None }
    }
}

/// A node fired, and what it was carrying.
#[derive(Debug, Clone, PartialEq)]
pub struct Fired {
    pub node: String,
    pub pulse: Pulse,
}

/// Why a node that received inbound did **not** fire.
#[derive(Debug, Clone, PartialEq)]
pub enum RefusalReason {
    /// **The no-echo property, observed.** Something arrived and the node was
    /// still recovering from its own excitation.
    Refractory,
    /// Still firing from the previous tick.
    Excited,
    /// Resting and willing, but not driven hard enough.
    BelowThreshold { inbound: f64, theta: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub node: String,
    pub reason: RefusalReason,
}

/// What one tick did. The record a consumer (MON-13, CTL-3, TEN-8) reads.
#[derive(Debug, Clone, PartialEq)]
pub struct StepReport {
    pub tick: usize,
    pub fired: Vec<Fired>,
    /// Nodes that received something and declined — with the reason, so a
    /// wave that stopped is distinguishable from a wave that never arrived.
    pub refused: Vec<Refusal>,
}

impl StepReport {
    pub fn is_quiet(&self) -> bool {
        self.fired.is_empty() && self.refused.is_empty()
    }

    pub fn fired_at(&self, node: &str) -> bool {
        self.fired.iter().any(|f| f.node == node)
    }

    pub fn refused_because(&self, node: &str) -> Option<&RefusalReason> {
        self.refused.iter().find(|r| r.node == node).map(|r| &r.reason)
    }
}

/// The medium: nodes, **symmetric** adjacency, and a logical clock.
///
/// Deliberately minimal — the substrate a relay needs to exist, not the
/// population object. See the module docs for the named follow-on.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    spec: SignalSpec,
    nodes: BTreeMap<String, NodeState>,
    adjacency: BTreeMap<String, BTreeSet<String>>,
    tick: usize,
}

impl Field {
    pub fn new(spec: SignalSpec) -> Self {
        Self { spec, nodes: BTreeMap::new(), adjacency: BTreeMap::new(), tick: 0 }
    }

    pub fn with_node(mut self, id: &str) -> Self {
        self.nodes.entry(id.to_string()).or_insert_with(NodeState::resting);
        self.adjacency.entry(id.to_string()).or_default();
        self
    }

    /// Connect two nodes. **Symmetric — an edge always runs both ways.**
    ///
    /// This is the design decision the module's central claim rests on. With a
    /// directed edge the wave would travel outward because the topology said
    /// so, and §III's assertion that *refractoriness* makes it directional
    /// would never be tested. Here nothing in the graph prefers a direction and
    /// the wave is directional anyway.
    pub fn connect(&mut self, a: &str, b: &str) -> Result<(), SignalError> {
        for id in [a, b] {
            if !self.nodes.contains_key(id) {
                return Err(SignalError::UnknownNode(id.to_string()));
            }
        }
        if a == b {
            return Err(SignalError::SelfEdge(a.to_string()));
        }
        self.adjacency.get_mut(a).expect("checked").insert(b.to_string());
        self.adjacency.get_mut(b).expect("checked").insert(a.to_string());
        Ok(())
    }

    pub fn linking(mut self, a: &str, b: &str) -> Self {
        self.connect(a, b).expect("nodes declared before linking");
        self
    }

    pub fn neighbours(&self, id: &str) -> Option<&BTreeSet<String>> {
        self.adjacency.get(id)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn phase(&self, id: &str) -> Option<Phase> {
        self.nodes.get(id).map(|n| n.phase)
    }

    pub fn tick(&self) -> usize {
        self.tick
    }

    pub fn nodes(&self) -> impl Iterator<Item = &str> {
        self.nodes.keys().map(String::as_str)
    }

    pub fn excited(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.phase == Phase::Excited)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Drive a node from outside the field — the origin of a wave.
    ///
    /// Adds to the inbox rather than firing directly: an external stimulus goes
    /// through exactly the same threshold test a relayed one does, so a stimulus
    /// below `θ_fire` does nothing and there is no privileged way in.
    pub fn stimulate(&mut self, id: &str, strength: f64) -> Result<(), SignalError> {
        if !strength.is_finite() || strength <= 0.0 {
            return Err(SignalError::BadStrength(strength.to_string()));
        }
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| SignalError::UnknownNode(id.to_string()))?;
        node.inbox += strength;
        node.arrived = Some(Pulse { origin: id.to_string(), hops: 0, strength });
        Ok(())
    }

    /// One tick of the medium.
    ///
    /// Decide → fire and queue emissions → advance every phase that did not
    /// just fire → deliver. Emissions land in the *next* tick's inboxes, which
    /// is what makes propagation one node per step and the wave observable as
    /// a wave rather than an instantaneous flood.
    pub fn step(&mut self) -> StepReport {
        let mut fired: Vec<Fired> = Vec::new();
        let mut refused: Vec<Refusal> = Vec::new();

        // ── decide ──────────────────────────────────────────────────────────
        for (id, n) in &self.nodes {
            if n.inbox <= 0.0 {
                continue; // nothing arrived; silence is not a refusal
            }
            match n.phase {
                Phase::Resting if n.inbox >= self.spec.theta_fire => {
                    let arrived = n.arrived.clone().unwrap_or(Pulse {
                        origin: id.clone(),
                        hops: 0,
                        strength: n.inbox,
                    });
                    fired.push(Fired {
                        node: id.clone(),
                        pulse: Pulse { origin: arrived.origin, hops: arrived.hops, strength: n.inbox },
                    });
                }
                Phase::Resting => refused.push(Refusal {
                    node: id.clone(),
                    reason: RefusalReason::BelowThreshold {
                        inbound: n.inbox,
                        theta: self.spec.theta_fire,
                    },
                }),
                // ★ The property the article calls load-bearing, recorded
                // rather than left as the absence of a second excitation.
                Phase::Refractory => {
                    refused.push(Refusal { node: id.clone(), reason: RefusalReason::Refractory })
                }
                Phase::Excited => {
                    refused.push(Refusal { node: id.clone(), reason: RefusalReason::Excited })
                }
            }
        }

        // ── fire, and queue what they emit ──────────────────────────────────
        let mut emissions: Vec<(String, Pulse)> = Vec::new();
        for f in &fired {
            let n = self.nodes.get_mut(&f.node).expect("just decided");
            n.phase = Phase::Excited;
            n.remaining = self.spec.excited_ticks;

            for nb in &self.adjacency[&f.node] {
                emissions.push((
                    nb.clone(),
                    Pulse {
                        // The ORIGIN travels, not the last hop — a consumer
                        // asking "where did this start" gets the wave's source.
                        origin: f.pulse.origin.clone(),
                        hops: f.pulse.hops + 1,
                        strength: self.spec.emit_strength,
                    },
                ));
            }
        }

        // ── advance every phase that did not just fire ──────────────────────
        let just_fired: BTreeSet<&str> = fired.iter().map(|f| f.node.as_str()).collect();
        for (id, n) in &mut self.nodes {
            if just_fired.contains(id.as_str()) {
                continue;
            }
            match n.phase {
                Phase::Excited => {
                    n.remaining = n.remaining.saturating_sub(1);
                    if n.remaining == 0 {
                        n.phase = Phase::Refractory;
                        n.remaining = self.spec.refractory_ticks;
                    }
                }
                Phase::Refractory => {
                    n.remaining = n.remaining.saturating_sub(1);
                    if n.remaining == 0 {
                        n.phase = Phase::Resting;
                    }
                }
                Phase::Resting => {}
            }
        }

        // ── deliver ─────────────────────────────────────────────────────────
        for n in self.nodes.values_mut() {
            n.inbox = 0.0;
            n.arrived = None;
        }
        for (to, pulse) in emissions {
            let n = self.nodes.get_mut(&to).expect("adjacency only names declared nodes");
            n.inbox += pulse.strength;
            n.arrived = Some(pulse);
        }

        self.tick += 1;
        StepReport { tick: self.tick, fired, refused }
    }

    /// Step until nothing fires or is refused, or `max_ticks` is reached.
    ///
    /// Returns the whole trace rather than a verdict — a wave is a sequence,
    /// and the order nodes were recruited in is the interesting part.
    ///
    /// **`max_ticks` is a bound, not a promise of termination.** A field that
    /// is still active when it runs out says so via
    /// [`SignalError::DidNotSettle` ] instead of returning a partial trace that
    /// reads like a finished one.
    pub fn run(&mut self, max_ticks: usize) -> Result<Vec<StepReport>, SignalError> {
        let mut trace = Vec::new();
        for _ in 0..max_ticks {
            let r = self.step();
            let quiet = r.is_quiet();
            trace.push(r);
            if quiet {
                return Ok(trace);
            }
        }
        Err(SignalError::DidNotSettle { max_ticks })
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum SignalError {
    #[error("θ_fire must be finite and > 0; got {0} — a threshold everything clears is a broadcast wearing a relay's clothes")]
    BadThreshold(String),
    #[error("a refractory window of 0 is not an excitable medium: the excitation sloshes back into the elements that just fired and the field rings forever. §III calls this term load-bearing; zeroing it deletes the mechanism and keeps the vocabulary")]
    NoRefractoryPeriod,
    #[error("a node must be excited for at least one tick, or the three-state machine §III names is not observable")]
    NoExcitedPeriod,
    #[error("stimulus strength must be finite and > 0; got {0}")]
    BadStrength(String),
    #[error("'{0}' is not a node in this field")]
    UnknownNode(String),
    #[error("'{0}' cannot be its own neighbour — a self-edge would let a node relay to itself, which is the echo the refractory period exists to prevent")]
    SelfEdge(String),
    #[error("the field was still active after {max_ticks} ticks")]
    DidNotSettle { max_ticks: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SignalSpec {
        SignalSpec::new(1.0, 2).expect("a real medium")
    }

    /// A–B–C, symmetric edges throughout.
    fn chain() -> Field {
        Field::new(spec())
            .with_node("a")
            .with_node("b")
            .with_node("c")
            .linking("a", "b")
            .linking("b", "c")
    }

    /// A 5-ring. Symmetric, so a stimulus at one node sends waves BOTH ways.
    fn ring() -> Field {
        Field::new(spec())
            .with_node("a")
            .with_node("b")
            .with_node("c")
            .with_node("d")
            .with_node("e")
            .linking("a", "b")
            .linking("b", "c")
            .linking("c", "d")
            .linking("d", "e")
            .linking("e", "a")
    }

    // ── the declared parameters ─────────────────────────────────────────────

    #[test]
    fn a_medium_with_no_refractory_period_is_refused() {
        let err = SignalSpec::new(1.0, 0).unwrap_err();
        assert!(matches!(err, SignalError::NoRefractoryPeriod), "got {err:?}");
        assert!(
            err.to_string().contains("load-bearing"),
            "the refusal explains WHY, not just that: {err}"
        );
    }

    #[test]
    fn a_threshold_that_everything_clears_is_refused() {
        for bad in [0.0, -1.0, f64::NAN] {
            assert!(
                matches!(SignalSpec::new(bad, 2), Err(SignalError::BadThreshold(_))),
                "θ_fire = {bad} should be refused"
            );
        }
    }

    #[test]
    fn a_self_edge_is_refused() {
        let mut f = Field::new(spec()).with_node("a");
        let err = f.connect("a", "a").unwrap_err();
        assert!(matches!(err, SignalError::SelfEdge(_)), "got {err:?}");
    }

    #[test]
    fn edges_are_symmetric() {
        let f = chain();
        assert!(f.neighbours("a").unwrap().contains("b"));
        assert!(
            f.neighbours("b").unwrap().contains("a"),
            "nothing in the topology may prefer a direction, or the wave's \
             directionality would not be evidence about refractoriness"
        );
    }

    // ── §III — the relay ────────────────────────────────────────────────────

    #[test]
    fn a_stimulus_below_threshold_does_nothing() {
        let mut f = Field::new(SignalSpec::new(2.0, 2).unwrap())
            .with_node("a")
            .with_node("b")
            .linking("a", "b");
        f.stimulate("a", 1.0).unwrap();
        let r = f.step();

        assert!(r.fired.is_empty());
        assert!(matches!(
            r.refused_because("a"),
            Some(RefusalReason::BelowThreshold { .. })
        ));
        assert_eq!(f.phase("b"), Some(Phase::Resting), "nothing travelled");
    }

    #[test]
    fn the_pulse_travels_one_node_per_tick() {
        let mut f = chain();
        f.stimulate("a", 1.0).unwrap();

        let r1 = f.step();
        assert!(r1.fired_at("a") && !r1.fired_at("b"));
        let r2 = f.step();
        assert!(r2.fired_at("b") && !r2.fired_at("c"));
        let r3 = f.step();
        assert!(r3.fired_at("c"));
    }

    #[test]
    fn the_origin_travels_not_the_last_hop() {
        let mut f = chain();
        f.stimulate("a", 1.0).unwrap();
        f.step();
        let r2 = f.step();
        let b = r2.fired.iter().find(|x| x.node == "b").unwrap();
        assert_eq!(b.pulse.origin, "a", "a consumer asking where this started gets the source");
        assert_eq!(b.pulse.hops, 1);

        let r3 = f.step();
        let c = r3.fired.iter().find(|x| x.node == "c").unwrap();
        assert_eq!(c.pulse.origin, "a");
        assert_eq!(c.pulse.hops, 2, "distance from the origin, not from the neighbour");
    }

    #[test]
    fn the_three_phases_are_all_observable() {
        let mut f = chain();
        f.stimulate("a", 1.0).unwrap();
        assert_eq!(f.phase("a"), Some(Phase::Resting));
        f.step();
        assert_eq!(f.phase("a"), Some(Phase::Excited));
        f.step();
        assert_eq!(f.phase("a"), Some(Phase::Refractory));
        f.step();
        f.step();
        assert_eq!(f.phase("a"), Some(Phase::Resting), "and back round");
    }

    // ── ★ the load-bearing half ─────────────────────────────────────────────

    #[test]
    fn a_relayed_pulse_cannot_re_excite_its_sender() {
        let mut f = chain();
        f.stimulate("a", 1.0).unwrap();

        f.step(); // a fires, emits to b
        f.step(); // b fires, emits BACK to a and on to c

        // b's emission genuinely reached a — the edge is symmetric, so it was
        // sent. a declines because it is refractory, and says so.
        let r3 = f.step();
        assert_eq!(
            r3.refused_because("a"),
            Some(&RefusalReason::Refractory),
            "★ the relayed message reached its sender and could not re-excite it"
        );
        assert!(!r3.fired_at("a"));
    }

    #[test]
    fn the_wave_is_directional_despite_symmetric_edges() {
        let mut f = chain();
        f.stimulate("a", 1.0).unwrap();
        let trace = f.run(20).expect("settles");

        // Each node fires exactly once, in order, and never again — with a
        // topology in which every edge runs both ways.
        let order: Vec<&str> = trace
            .iter()
            .flat_map(|r| r.fired.iter())
            .map(|x| x.node.as_str())
            .collect();
        assert_eq!(
            order,
            ["a", "b", "c"],
            "outward only — and nothing in the graph prefers that direction"
        );
    }

    #[test]
    fn the_field_falls_quiet_rather_than_ringing_forever() {
        let mut f = chain();
        f.stimulate("a", 1.0).unwrap();
        let trace = f.run(50).expect("an excitable medium terminates");
        assert!(trace.last().unwrap().is_quiet());
    }

    // ── ★ the proof of value ────────────────────────────────────────────────

    #[test]
    fn one_origin_recruits_the_whole_field_with_no_broadcaster() {
        let mut f = ring();
        assert_eq!(f.neighbours("a").unwrap().len(), 2);
        assert!(
            f.neighbours("a").unwrap().len() < f.node_count(),
            "the origin does not know the field — there is no broadcaster"
        );

        f.stimulate("a", 1.0).unwrap();
        let trace = f.run(50).expect("settles");

        let fired: BTreeSet<&str> =
            trace.iter().flat_map(|r| r.fired.iter()).map(|x| x.node.as_str()).collect();
        let all: BTreeSet<&str> = f.nodes().collect();
        assert_eq!(fired, all, "every node fired, recruited from one origin");

        // Every one of them carries the same origin.
        for f2 in trace.iter().flat_map(|r| r.fired.iter()) {
            assert_eq!(f2.pulse.origin, "a");
        }

        // And it terminated: each node fired exactly once.
        let count = trace.iter().flat_map(|r| r.fired.iter()).count();
        assert_eq!(count, f.node_count(), "one excitation each — no echo");
    }

    #[test]
    fn two_wavefronts_annihilate_where_they_meet() {
        // The excitable-media signature. A symmetric ring stimulated at one
        // node sends waves BOTH ways; where they meet each finds the other
        // refractory and both die. This is why the field terminates without
        // anyone counting hops or holding a map of it.
        let mut f = ring();
        f.stimulate("a", 1.0).unwrap();
        let trace = f.run(50).expect("settles");

        let t_of = |node: &str| {
            trace.iter().find(|r| r.fired_at(node)).map(|r| r.tick).unwrap()
        };
        assert_eq!(t_of("a"), 1);
        assert_eq!(t_of("b"), 2, "one way round");
        assert_eq!(t_of("e"), 2, "and the other, simultaneously");
        assert_eq!(t_of("c"), 3);
        assert_eq!(t_of("d"), 3);

        // c and d are the meeting point: each emitted into the other, and
        // neither re-fired. NOTE the phase — at a head-on collision the
        // opposing pulse lands while the node is still EXCITED, one tick
        // before it becomes REFRACTORY. A pulse arriving back at its own
        // SENDER is later and finds REFRACTORY (see the test above). Two
        // phases, one mechanism: not resting. Worth distinguishing rather
        // than blurring into "refractoriness stopped it".
        let after: Vec<&Refusal> = trace
            .iter()
            .filter(|r| r.tick > 3)
            .flat_map(|r| r.refused.iter())
            .collect();
        assert!(!after.is_empty(), "the wavefronts did meet");
        assert!(
            after.iter().all(|r| matches!(
                r.reason,
                RefusalReason::Excited | RefusalReason::Refractory
            )),
            "the wavefronts stopped inside the excitation cycle, not for want of              drive — a BelowThreshold here would mean they simply ran out of graph"
        );
    }

    // ── spatial summation ───────────────────────────────────────────────────

    #[test]
    fn a_higher_threshold_needs_more_than_one_neighbour_to_agree() {
        // θ = 2 with unit emissions: one excited neighbour is not enough, two
        // simultaneously are. This is the seam to §II's threshold commitment —
        // the same number read as "how many of my neighbours are in".
        let mut f = Field::new(SignalSpec::new(2.0, 2).unwrap())
            .with_node("l")
            .with_node("r")
            .with_node("mid")
            .linking("l", "mid")
            .linking("r", "mid");

        // Unit emissions, θ = 2 — so mid fires only if BOTH neighbours are
        // firing at the same time. Drive both.
        f.stimulate("l", 2.0).unwrap();
        f.stimulate("r", 2.0).unwrap();
        let r1 = f.step();
        assert!(r1.fired_at("l") && r1.fired_at("r"));

        let r2 = f.step();
        assert!(r2.fired_at("mid"), "both neighbours fired at once — 2 × 2.0 clears θ");
    }

    #[test]
    fn one_neighbour_alone_does_not_clear_a_two_neighbour_threshold() {
        let mut f = Field::new(SignalSpec::new(2.0, 2).unwrap())
            .with_node("l")
            .with_node("mid")
            .linking("l", "mid");
        f.stimulate("l", 2.0).unwrap();
        f.step();
        let r2 = f.step();

        assert!(!r2.fired_at("mid"));
        assert!(matches!(
            r2.refused_because("mid"),
            Some(RefusalReason::BelowThreshold { .. }),
            ));
    }

    #[test]
    fn theta_fire_is_not_inert_across_different_thresholds() {
        // The test that catches the trap recorded on SignalSpec::emit_strength.
        // An earlier draft emitted θ_fire itself, which made ONE neighbour
        // sufficient at every threshold — θ_fire present, checked, and inert.
        // Only comparing two thresholds on the same topology reveals it.
        let build = |theta: f64| {
            let mut f = Field::new(SignalSpec::new(theta, 2).unwrap())
                .with_node("l")
                .with_node("mid")
                .linking("l", "mid");
            f.stimulate("l", theta).unwrap();
            f.step();
            f.step()
        };
        assert!(build(1.0).fired_at("mid"), "one neighbour clears θ=1");
        assert!(!build(2.0).fired_at("mid"), "one neighbour must NOT clear θ=2");
    }

    // ── refusals ────────────────────────────────────────────────────────────

    #[test]
    fn stimulating_an_unknown_node_is_refused() {
        let mut f = chain();
        assert!(matches!(f.stimulate("nowhere", 1.0), Err(SignalError::UnknownNode(_))));
    }

    #[test]
    fn an_external_stimulus_goes_through_the_same_threshold_as_a_relayed_one() {
        // No privileged way in: a weak stimulus is refused exactly as a weak
        // relay is, so nothing can drive the field by calling the right method.
        let mut f = Field::new(SignalSpec::new(5.0, 2).unwrap()).with_node("a");
        f.stimulate("a", 1.0).unwrap();
        let r = f.step();
        assert!(r.fired.is_empty());
        assert!(matches!(r.refused_because("a"), Some(RefusalReason::BelowThreshold { .. })));
    }

    #[test]
    fn a_field_that_does_not_settle_says_so() {
        let mut f = chain();
        f.stimulate("a", 1.0).unwrap();
        let err = f.run(1).unwrap_err();
        assert!(
            matches!(err, SignalError::DidNotSettle { max_ticks: 1 }),
            "a partial trace must not read like a finished one; got {err:?}"
        );
    }

    #[test]
    fn silence_is_not_a_refusal() {
        let mut f = chain();
        let r = f.step();
        assert!(r.is_quiet(), "a node nothing arrived at has not declined anything");
    }
}
