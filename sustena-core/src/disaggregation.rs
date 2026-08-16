//! Disaggregation — the reverse threshold, with hysteresis
//! (Multiparty §IX · MUL-14).
//!
//! > The reverse threshold. When pressure falls below a *release* value the
//! > body dissolves and nodes recover autonomy. Use **hysteresis** — aggregate
//! > at `θ↑`, disperse at `θ↓ < θ↑` — so the body doesn't flicker in and out
//! > near a single line; the gap between the two thresholds is the design
//! > parameter, and it is bought with responsiveness. On dispersal, shared CRDT
//! > state settles to a final merged value and each node carries its portion
//! > home. **The composition is destroyed; the members are not. Departure
//! > removes an edge, never a node.**
//!
//! This is MUL-2's closing bracket. That slice built `commit = 1[c > θ]`; this
//! one is `disperse = 1[c < θ↓]`, and the two thresholds are not the same
//! number on purpose.
//!
//! ## ★ Hysteresis is a property of where you ARE, not only what c is
//!
//! A single threshold makes the body's existence a function of one reading, so
//! a concentration hovering near it produces a body that forms and dissolves
//! and forms again. [`BandPosition::Within`] is the whole mechanism: between
//! `θ↓` and `θ↑` **nothing changes** — a formed body stays formed, a dispersed
//! one stays dispersed. The same reading means different things depending on
//! which state it arrives in, which is exactly what stops the flicker.
//!
//! [`Band::new`] **refuses `θ↓ ≥ θ↑`.** A zero-width band is a single
//! threshold wearing two names, and an inverted one is worse — it would form
//! and disperse in the same breath. Both are flicker, so neither is
//! constructible.
//!
//! **The gap is bought with responsiveness**, and [`Band::gap`] is the declared
//! price: wider is steadier and slower to react to a genuine collapse. That is
//! a statement about how much churn a household will tolerate versus how fast
//! it wants to be told, and it belongs where it can be argued with.
//!
//! ## ★ Departure removes an edge, never a node
//!
//! [`Dispersal::members_removed`] is **always 0**, and a test asserts it rather
//! than trusting the sentence. Every member's own portion appears in
//! [`Dispersal::carried_home`], untouched, whatever the shared value settled
//! to. The composition is destroyed; the members are not.
//!
//! This is the holon invariant applied to dissolution, and the same never-erase
//! property [`crate::vclock`] applies to a concurrent edit — a body coming
//! apart must not be a way to lose what its members brought.
//!
//! ## The settle (§IX), read honestly
//!
//! §IX says the shared state *"settles to a final merged value"*, singular.
//! That is exact for a [`Dimension::Snapshot`] dimension, where the merge
//! collapses and §VII proves collapsing is correct. For a
//! [`Dimension::Contested`] one it is not: discarding one of two genuinely
//! concurrent edits is the very thing §VII warns against, so the settle keeps
//! what the merge preserves. [`Dispersal::settled`] is therefore a list —
//! length 1 for a snapshot dimension, which is the reading §IX's phrasing has
//! in mind, and longer only when collapsing would lose something.
//!
//! ## Honest limits
//!
//! - **Pressure is supplied, not computed here.** `c` is the same quantity
//!   MUL-2 senses, and where it comes from is the caller's — a node's local
//!   `c_i`, or a measure the assembled body takes of itself. Reaching into a
//!   [`crate::population::Population`] for it would re-decide the no-global-view
//!   question this module has no business re-deciding.
//! - **Dispersal is terminal for this assembly.** Re-forming is a new
//!   assembly with the same members, not a resurrection — which is what makes
//!   "the composition is destroyed" true rather than decorative.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use thiserror::Error;

use crate::vclock::{merge, Dimension, Stamped};

/// `θ↑` and `θ↓` — the hysteresis band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    form_at: f64,
    disperse_at: f64,
}

impl Band {
    /// **Refuses `θ↓ ≥ θ↑`.**
    ///
    /// A zero-width band is a single threshold wearing two names; an inverted
    /// one would form and disperse in the same breath. Both are the flicker
    /// hysteresis exists to prevent, so neither is constructible.
    pub fn new(form_at: f64, disperse_at: f64) -> Result<Self, DisaggregationError> {
        for (name, v) in [("θ↑", form_at), ("θ↓", disperse_at)] {
            if !v.is_finite() {
                return Err(DisaggregationError::BadThreshold {
                    which: name.into(),
                    value: v.to_string(),
                });
            }
        }
        if disperse_at >= form_at {
            return Err(DisaggregationError::NoBand { form_at, disperse_at });
        }
        Ok(Self { form_at, disperse_at })
    }

    pub fn form_at(&self) -> f64 {
        self.form_at
    }

    pub fn disperse_at(&self) -> f64 {
        self.disperse_at
    }

    /// **The declared design parameter, bought with responsiveness.**
    ///
    /// Wider is steadier and slower to react to a genuine collapse.
    pub fn gap(&self) -> f64 {
        self.form_at - self.disperse_at
    }

    /// Which side of the band a pressure reading sits on.
    pub fn classify(&self, c: f64) -> BandPosition {
        if c >= self.form_at {
            BandPosition::AboveForm
        } else if c < self.disperse_at {
            BandPosition::BelowDisperse
        } else {
            BandPosition::Within
        }
    }
}

/// Where a reading falls relative to the band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandPosition {
    /// `c ≥ θ↑` — enough pressure to form.
    AboveForm,
    /// `θ↓ ≤ c < θ↑` — **the band. Whatever state the body is in, it stays.**
    ///
    /// This is the whole of hysteresis: the same reading means different things
    /// depending on which state it arrives in.
    Within,
    /// `c < θ↓` — the release value. The body dissolves.
    BelowDisperse,
}

/// Formed, or dispersed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyState {
    Dispersed,
    Formed,
}

/// What one pressure reading did.
#[derive(Debug, Clone, PartialEq)]
pub enum BandTransition {
    Formed { at: f64 },
    Dispersed(Box<Dispersal>),
    /// **Nothing changed** — either the reading was in the band, or it was on
    /// the side the body is already on.
    Held { position: BandPosition, state: AssemblyState },
}

impl BandTransition {
    pub fn changed(&self) -> bool {
        !matches!(self, BandTransition::Held { .. })
    }

    pub fn dispersal(&self) -> Option<&Dispersal> {
        match self {
            BandTransition::Dispersed(d) => Some(d),
            _ => None,
        }
    }
}

/// What survived the body coming apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Dispersal {
    /// The shared state, settled through the CRDT merge.
    ///
    /// Length 1 for a snapshot dimension — §IX's *"a final merged value"*.
    /// Longer only when collapsing would discard a genuinely concurrent edit.
    pub settled: Vec<Stamped<Value>>,
    /// **Each node carries its portion home.** Every member, untouched.
    pub carried_home: BTreeMap<String, Stamped<Value>>,
    /// The composition that was destroyed.
    pub edges_removed: usize,
    /// **Always 0.** Departure removes an edge, never a node.
    pub members_removed: usize,
    /// The reading that released it.
    pub released_at: f64,
}

impl Dispersal {
    /// Did every member get its portion back?
    pub fn everyone_carried_home(&self, members: &BTreeSet<String>) -> bool {
        members.iter().all(|m| self.carried_home.contains_key(m))
    }
}

/// A body that forms under pressure and comes apart when it falls away.
///
/// Named `Assembly` rather than `Body` — [`crate::consensus::Body`] is the
/// quorum's membership, a different thing that happens to share a noun.
#[derive(Debug, Clone)]
pub struct Assembly {
    band: Band,
    dimension: Dimension,
    state: AssemblyState,
    members: BTreeSet<String>,
    /// Each member's own contribution to the shared state.
    portions: BTreeMap<String, Stamped<Value>>,
    /// Declared edges of the composition. Removed on dispersal; the members
    /// they connect are not.
    edges: usize,
}

impl Assembly {
    pub fn new(band: Band, dimension: Dimension) -> Self {
        Self {
            band,
            dimension,
            state: AssemblyState::Dispersed,
            members: BTreeSet::new(),
            portions: BTreeMap::new(),
            edges: 0,
        }
    }

    pub fn with_member(mut self, id: &str) -> Self {
        if self.members.insert(id.to_string()) && self.members.len() > 1 {
            // A member joining a body of n adds one edge to it.
            self.edges += 1;
        }
        self
    }

    /// A member's portion of the shared state.
    pub fn contribute(
        &mut self,
        node: &str,
        portion: Stamped<Value>,
    ) -> Result<(), DisaggregationError> {
        if !self.members.contains(node) {
            return Err(DisaggregationError::NotAMember(node.to_string()));
        }
        self.portions.insert(node.to_string(), portion);
        Ok(())
    }

    pub fn state(&self) -> AssemblyState {
        self.state
    }

    pub fn band(&self) -> Band {
        self.band
    }

    pub fn members(&self) -> &BTreeSet<String> {
        &self.members
    }

    pub fn is_formed(&self) -> bool {
        self.state == AssemblyState::Formed
    }

    /// One pressure reading (§IX).
    ///
    /// `c` is the same quantity MUL-2 senses. Where it comes from is the
    /// caller's — see the module docs on why this does not reach for it.
    pub fn sense(&mut self, c: f64) -> Result<BandTransition, DisaggregationError> {
        if !c.is_finite() {
            return Err(DisaggregationError::BadPressure(c.to_string()));
        }
        let position = self.band.classify(c);

        match (position, self.state) {
            // Enough pressure, and not yet formed.
            (BandPosition::AboveForm, AssemblyState::Dispersed) => {
                self.state = AssemblyState::Formed;
                Ok(BandTransition::Formed { at: c })
            }
            // Below the release value, and currently formed.
            (BandPosition::BelowDisperse, AssemblyState::Formed) => {
                let d = self.dissolve(c);
                Ok(BandTransition::Dispersed(Box::new(d)))
            }
            // ★ In the band, or already on the side the reading points to.
            // Nothing changes — this is hysteresis.
            _ => Ok(BandTransition::Held { position, state: self.state }),
        }
    }

    /// Settle the shared state, hand every member its portion, drop the edges.
    fn dissolve(&mut self, released_at: f64) -> Dispersal {
        // §IX's settle, through the CRDT merge — collapsing only where §VII
        // proves collapsing is correct.
        let mut settled: Vec<Stamped<Value>> = Vec::new();
        for p in self.portions.values() {
            settled = match settled.len() {
                0 => vec![p.clone()],
                _ => {
                    let mut next: Vec<Stamped<Value>> = Vec::new();
                    for existing in &settled {
                        next.extend(merge(existing, p).resolve(self.dimension));
                    }
                    dedupe_by_id(next)
                }
            };
        }

        let edges_removed = self.edges;
        self.edges = 0;
        self.state = AssemblyState::Dispersed;

        Dispersal {
            settled,
            // ★ Every member's own portion, untouched.
            carried_home: self.portions.clone(),
            edges_removed,
            // ★ Departure removes an edge, never a node.
            members_removed: 0,
            released_at,
        }
    }
}

fn dedupe_by_id(mut xs: Vec<Stamped<Value>>) -> Vec<Stamped<Value>> {
    xs.sort_by(|a, b| a.order_key().cmp(&b.order_key()));
    xs.dedup_by(|a, b| a.id == b.id);
    xs
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum DisaggregationError {
    #[error("{which} must be finite; got {value}")]
    BadThreshold { which: String, value: String },
    #[error("θ↓ ({disperse_at}) must be strictly below θ↑ ({form_at}) — a zero-width band is one threshold wearing two names, and an inverted one would form and disperse in the same breath. Both are the flicker hysteresis exists to prevent")]
    NoBand { form_at: f64, disperse_at: f64 },
    #[error("pressure must be finite; got {0}")]
    BadPressure(String),
    #[error("'{0}' is not a member of this assembly")]
    NotAMember(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vclock::VectorClock;
    use serde_json::json;

    fn band() -> Band {
        // Form at 0.7, release at 0.3 — a wide band, so the stable middle is
        // unmistakable.
        Band::new(0.7, 0.3).expect("θ↓ < θ↑")
    }

    fn portion(node: &str, v: &str, clock: VectorClock, t: i64) -> Stamped<Value> {
        Stamped::new(json!(v), clock, t, node)
    }

    fn assembly(dim: Dimension) -> Assembly {
        Assembly::new(band(), dim).with_member("a").with_member("b").with_member("c")
    }

    // ── ★ the band ──────────────────────────────────────────────────────────

    #[test]
    fn a_zero_or_inverted_band_is_refused() {
        // ★ A zero-width band is a single threshold wearing two names.
        assert!(matches!(Band::new(0.5, 0.5), Err(DisaggregationError::NoBand { .. })));
        // And an inverted one would form and disperse in the same breath.
        let err = Band::new(0.3, 0.7).unwrap_err();
        assert!(matches!(err, DisaggregationError::NoBand { .. }));
        assert!(err.to_string().contains("flicker hysteresis exists to prevent"));
    }

    #[test]
    fn the_gap_is_the_declared_design_parameter() {
        let b = band();
        assert!((b.gap() - 0.4).abs() < 1e-12);
        assert!(b.gap() > 0.0, "bought with responsiveness — wider is steadier and slower");
    }

    #[test]
    fn the_band_classifies_all_three_regions() {
        let b = band();
        assert_eq!(b.classify(0.9), BandPosition::AboveForm);
        assert_eq!(b.classify(0.7), BandPosition::AboveForm, "θ↑ itself forms");
        assert_eq!(b.classify(0.5), BandPosition::Within);
        assert_eq!(b.classify(0.3), BandPosition::Within, "θ↓ itself holds — release is below it");
        assert_eq!(b.classify(0.1), BandPosition::BelowDisperse);
    }

    // ── ★★ hysteresis ───────────────────────────────────────────────────────

    #[test]
    fn the_body_forms_at_the_upper_threshold() {
        let mut a = assembly(Dimension::Snapshot);
        assert_eq!(a.state(), AssemblyState::Dispersed);
        assert!(matches!(a.sense(0.8).unwrap(), BandTransition::Formed { .. }));
        assert!(a.is_formed());
    }

    #[test]
    fn a_formed_body_does_not_dissolve_inside_the_band() {
        // ★★ THE PROOF OF VALUE, first half. Pressure falls well below θ↑ and
        // the body holds, because it is above θ↓. A single threshold would have
        // dissolved it here.
        let mut a = assembly(Dimension::Snapshot);
        a.sense(0.8).unwrap();

        for c in [0.69, 0.5, 0.31] {
            let t = a.sense(c).unwrap();
            assert!(!t.changed(), "★ held at c = {c}");
            assert!(matches!(
                t,
                BandTransition::Held { position: BandPosition::Within, state: AssemblyState::Formed }
            ));
            assert!(a.is_formed());
        }
    }

    #[test]
    fn a_dispersed_body_does_not_form_inside_the_band() {
        // ★★ The other half, and the reason it is hysteresis rather than a
        // grace period: the SAME readings that hold a formed body together also
        // fail to assemble a dispersed one. The band is symmetric; the state is
        // what differs.
        let mut a = assembly(Dimension::Snapshot);
        for c in [0.31, 0.5, 0.69] {
            let t = a.sense(c).unwrap();
            assert!(!t.changed(), "★ still dispersed at c = {c}");
            assert!(matches!(
                t,
                BandTransition::Held { position: BandPosition::Within, state: AssemblyState::Dispersed }
            ));
        }
        assert!(!a.is_formed());
    }

    #[test]
    fn the_full_loop_does_not_flicker() {
        // ★★ The whole point, walked: up through the band without forming, form
        // at θ↑, back down through the band without dissolving, dissolve below
        // θ↓, and back up through the band without re-forming.
        let mut a = assembly(Dimension::Snapshot);
        let mut changes = 0;
        for c in [0.4, 0.6, 0.8, 0.6, 0.4, 0.2, 0.4, 0.6] {
            if a.sense(c).unwrap().changed() {
                changes += 1;
            }
        }
        assert_eq!(changes, 2, "★★ eight readings, exactly two transitions");
        assert!(!a.is_formed(), "and it ended dispersed, still inside the band");
    }

    #[test]
    fn the_body_dissolves_only_below_the_release_value() {
        let mut a = assembly(Dimension::Snapshot);
        a.sense(0.9).unwrap();
        let t = a.sense(0.1).unwrap();
        assert!(t.changed());
        let d = t.dispersal().expect("dispersed");
        assert!((d.released_at - 0.1).abs() < 1e-12);
        assert!(!a.is_formed());
    }

    // ── ★★ departure removes an edge, never a node ──────────────────────────

    #[test]
    fn every_member_carries_its_portion_home() {
        // ★★ §IX: "the composition is destroyed; the members are not."
        let mut a = assembly(Dimension::Snapshot);
        let root = VectorClock::new().tick("shared");
        for (n, v) in [("a", "mine"), ("b", "yours"), ("c", "theirs")] {
            a.contribute(n, portion(n, v, root.tick(n), 100)).unwrap();
        }
        a.sense(0.9).unwrap();

        let t = a.sense(0.05).unwrap();
        let d = t.dispersal().unwrap();

        assert_eq!(d.members_removed, 0, "★★ departure removes an edge, NEVER a node");
        assert!(d.edges_removed > 0, "the composition genuinely came apart");
        assert!(d.everyone_carried_home(a.members()), "★★ every member got its portion back");
        assert_eq!(d.carried_home["a"].value, json!("mine"), "untouched");
        assert_eq!(d.carried_home["c"].value, json!("theirs"));
    }

    #[test]
    fn a_non_member_cannot_contribute() {
        let mut a = assembly(Dimension::Snapshot);
        let v = VectorClock::new().tick("x");
        assert!(matches!(
            a.contribute("outsider", portion("outsider", "v", v, 1)),
            Err(DisaggregationError::NotAMember(_))
        ));
    }

    // ── the settle ──────────────────────────────────────────────────────────

    #[test]
    fn a_snapshot_dimension_settles_to_one_final_value() {
        // §IX's phrasing, exact for a snapshot dimension: the newest reading of
        // an external authority supersedes by definition.
        let mut a = assembly(Dimension::Snapshot);
        let root = VectorClock::new().tick("shared");
        a.contribute("a", portion("a", "1200", root.tick("a"), 100)).unwrap();
        a.contribute("b", portion("b", "1350", root.tick("b"), 200)).unwrap();
        a.sense(0.9).unwrap();

        let d = a.sense(0.1).unwrap().dispersal().unwrap().clone();
        assert_eq!(d.settled.len(), 1, "a final merged value");
        assert_eq!(d.settled[0].value, json!("1350"));
        // And both members still went home with what they brought.
        assert_eq!(d.carried_home.len(), 2);
    }

    #[test]
    fn a_contested_dimension_keeps_what_collapsing_would_discard() {
        // ★ §IX says "a final merged value", singular — exact for a snapshot
        // dimension. For a contested one, collapsing is the very thing §VII
        // warns against, so the settle keeps what the merge preserves.
        let mut a = assembly(Dimension::Contested);
        let root = VectorClock::new().tick("shared");
        a.contribute("a", portion("a", "go monday", root.tick("a"), 100)).unwrap();
        a.contribute("b", portion("b", "go friday", root.tick("b"), 200)).unwrap();
        a.sense(0.9).unwrap();

        let d = a.sense(0.1).unwrap().dispersal().unwrap().clone();
        assert_eq!(d.settled.len(), 2, "★ genuinely concurrent — neither is discarded");
        let vs: Vec<&Value> = d.settled.iter().map(|s| &s.value).collect();
        assert!(vs.contains(&&json!("go monday")) && vs.contains(&&json!("go friday")));
    }

    #[test]
    fn a_causally_ordered_pair_settles_to_the_later_one_under_either_dimension() {
        // Not a conflict: the later portion genuinely supersedes.
        for dim in [Dimension::Snapshot, Dimension::Contested] {
            let mut a = Assembly::new(band(), dim).with_member("a").with_member("b");
            let first = VectorClock::new().tick("a");
            let second = first.tick("b");
            a.contribute("a", portion("a", "draft", first, 100)).unwrap();
            a.contribute("b", portion("b", "revised", second, 200)).unwrap();
            a.sense(0.9).unwrap();

            let d = a.sense(0.1).unwrap().dispersal().unwrap().clone();
            assert_eq!(d.settled.len(), 1, "{dim:?}: a supersession is not a conflict");
            assert_eq!(d.settled[0].value, json!("revised"));
        }
    }

    #[test]
    fn dispersing_an_empty_assembly_settles_to_nothing() {
        // Honest emptiness: no portions means no shared value, not a fabricated
        // one. And still no members removed.
        let mut a = assembly(Dimension::Snapshot);
        a.sense(0.9).unwrap();
        let d = a.sense(0.0).unwrap().dispersal().unwrap().clone();
        assert!(d.settled.is_empty());
        assert_eq!(d.members_removed, 0);
    }

    #[test]
    fn a_non_finite_pressure_is_refused() {
        let mut a = assembly(Dimension::Snapshot);
        assert!(matches!(a.sense(f64::NAN), Err(DisaggregationError::BadPressure(_))));
    }
}
