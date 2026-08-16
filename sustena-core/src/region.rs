//! **V as a region**, and `urgency(s) = d(s, V)` (Sustain §V, §VII · Monitor MON-10/11).
//!
//! ```text
//! V = (∏_d [lo_d, hi_d])  ∩  ⋂_i { s : C_i(s) }  ⊆  S
//!      the box                  cross-dimension relations
//! ```
//!
//! Two parts: a **box** of per-dimension intervals, and a set of **relations**
//! that tie dimensions together. A Sustain is alive while `s ∈ V` — Ashby's
//! essential variables, held inside their limits.
//!
//! ## Why a region and not a list of booleans
//!
//! > The upgrade from a flat list of boolean expressions to a declared *region*
//! > is not cosmetic, and this is the crux of the module.
//!
//! A list can only be **true or false**. A region can be:
//!
//! | operation | method | why it needs a region |
//! |---|---|---|
//! | **measured** | [`Region::distance`] | *how far outside are we?* |
//! | **projected** | [`Distance::binding`] | *which dimension is the binding one?* |
//! | **intersected** | [`Region::fits_within`] | *does the child's region fit inside the parent's?* |
//! | **searched** | (kernel, §VI — not built) | *is there a path back in?* |
//!
//! ## `W(s) = d(s, V) = inf_{v∈V} ‖s − v‖_w`
//!
//! The weighted distance from the current state to the nearest point of `V`,
//! with `W(s) = 0 ⟺ s ∈ V̄`. **This is THE salience quantity** — the Curated UI
//! consumes it as salience and the Controller consumes it as its escalation
//! trigger, and because it is a fact about the household's own declared region
//! it cannot be gamed by a widget or an operative competing for attention.
//! There is no second notion of "important".
//!
//! ## Two things this exposes rather than hides
//!
//! **1. The weights `w` are a modelling choice, not a fact.** Dimensions have
//! incommensurable units — shillings, kilograms, hours. Any single scalar
//! distance encodes a judgement about their relative importance, and that
//! judgement is **declared in the spec where it can be argued with, never buried
//! in a norm**. So [`Region::weights`] is a spec field, and
//! [`Region::undeclared_weights`] reports every dimension relying on the
//! fallback — an undeclared weight is a finding, not a silent `1.0`.
//!
//! **2. Inside `V`, `W ≡ 0` and cannot rank anything.** So membership is
//! three-valued ([`Membership`]) rather than boolean, and [`Region::margin`]
//! gives the secondary objective inside the region: *"comfortably fine"* and
//! *"one bad week from not fine"* are different states, and only a margin
//! distinguishes them.
//!
//! ## The honest limit of a scalar W
//!
//! An interval violation has a natural distance. **A relation is a boolean and
//! has none.** So a state can satisfy every interval — box distance `0` — and
//! still be outside `V` because a cross-dimension relation fails. Reporting that
//! as `W = 0` would say *"in the viable region"* about a state that is not.
//!
//! [`Distance::weighted`] is therefore a **lower bound**, and
//! [`Distance::is_zero`] requires *both* a zero box distance *and* no violated
//! relations. Violated relations are **named, not scored** — inventing a penalty
//! number for them would put a fabricated quantity into the one signal the whole
//! system is supposed to be able to trust.
//!
//! ## Native form, not an imported one (MON-11)
//!
//! This computes distance-to-V **on state folded from the log**, which is what
//! the Monitor article's own caveat asks for: the observability/Kalman apparatus
//! assumes a continuous linear system, and Sustena's state is discrete, typed and
//! event-sourced. Nothing here assumes an ODE. CUSUM/EWMA over the resulting `W`
//! series is the anomaly layer above this one and is deliberately a separate
//! module.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::predicate::{self, SyntaxError};
use crate::state::State;

/// One dimension's declared band. `lo`/`hi` may be infinite.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    pub dim: String,
    pub lo: f64,
    pub hi: f64,
}

impl Interval {
    pub fn new(dim: &str, lo: f64, hi: f64) -> Self {
        Self { dim: dim.to_string(), lo, hi }
    }

    /// `[lo, ∞)` — the common shape: a floor with no ceiling.
    pub fn at_least(dim: &str, lo: f64) -> Self {
        Self::new(dim, lo, f64::INFINITY)
    }

    pub fn at_most(dim: &str, hi: f64) -> Self {
        Self::new(dim, f64::NEG_INFINITY, hi)
    }

    /// How far outside this band a value sits. `0.0` when inside or on an edge.
    fn excess(&self, v: f64) -> f64 {
        if v < self.lo {
            self.lo - v
        } else if v > self.hi {
            v - self.hi
        } else {
            0.0
        }
    }

    /// Distance to the nearest edge, from inside. Infinite for an unbounded side.
    fn margin(&self, v: f64) -> f64 {
        (v - self.lo).min(self.hi - v)
    }

    /// Is every point of `self` a point of `other`?
    fn within(&self, other: &Interval) -> bool {
        self.lo >= other.lo && self.hi <= other.hi
    }
}

/// **V** — the viable region.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Region {
    pub intervals: Vec<Interval>,
    /// Cross-dimension constraints `C_i : S → bool`, as predicate expressions.
    pub relations: Vec<String>,
    /// `w` — the declared relative importance of each dimension.
    ///
    /// **A modelling choice, not a fact.** Declared here so it can be argued
    /// with; see [`Region::undeclared_weights`].
    pub weights: BTreeMap<String, f64>,
}

/// Where a state sits relative to `V`. Three-valued, because `W ≡ 0` inside the
/// region cannot rank anything and the margin needs somewhere to live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Membership {
    /// Strictly inside every interval, every relation holding.
    Interior,
    /// In `V`, but sitting on an interval edge — `W = 0` with no room left.
    Boundary,
    Outside,
}

impl Membership {
    /// `s ∈ V̄` — interior or boundary.
    pub fn is_viable(&self) -> bool {
        !matches!(self, Membership::Outside)
    }
}

/// `d(s, V)`, decomposed so a person can be told *why*.
#[derive(Debug, Clone, PartialEq)]
pub struct Distance {
    /// The weighted box distance. **A lower bound on `d(s,V)`** whenever
    /// `relations_violated` is non-empty — see the module docs.
    pub weighted: f64,
    /// Per-dimension weighted excess, worst first. Empty inside the box.
    pub per_dimension: Vec<(String, f64)>,
    /// The dimension contributing most — *which one is binding*.
    pub binding: Option<String>,
    /// Relations that fail. **Named, not scored:** a boolean has no natural
    /// distance, and inventing one would fabricate the very quantity the rest
    /// of the system is supposed to be able to trust.
    pub relations_violated: Vec<String>,
}

impl Distance {
    /// `W(s) = 0 ⟺ s ∈ V̄`.
    ///
    /// Requires **both** halves: a zero box distance with a violated relation is
    /// not zero distance, it is an unmeasurable one.
    pub fn is_zero(&self) -> bool {
        self.weighted == 0.0 && self.relations_violated.is_empty()
    }

    /// Whether `weighted` is the true `d(s,V)` rather than a lower bound.
    pub fn is_exact(&self) -> bool {
        self.relations_violated.is_empty()
    }
}

// Not `Eq`: an empty-interval error carries the f64 bounds that were wrong.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RegionError {
    #[error("dimension '{dim}' does not resolve to a number in this state")]
    NotANumber { dim: String },

    #[error("relation '{expr}' could not be parsed: {detail}")]
    BadRelation { expr: String, detail: String },

    #[error("interval on '{dim}' is empty: lo {lo} > hi {hi}")]
    EmptyInterval { dim: String, lo: f64, hi: f64 },

    #[error("weight for '{dim}' must be finite and positive, got {w}")]
    BadWeight { dim: String, w: String },
}

impl Region {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bounding(mut self, interval: Interval) -> Self {
        self.intervals.push(interval);
        self
    }

    pub fn relating(mut self, expr: &str) -> Self {
        self.relations.push(expr.to_string());
        self
    }

    /// Declare a dimension's weight — its relative importance in `‖·‖_w`.
    pub fn weighing(mut self, dim: &str, w: f64) -> Self {
        self.weights.insert(dim.to_string(), w);
        self
    }

    /// The declared weight, falling back to `1.0`.
    ///
    /// The fallback exists so a region is usable before every weight is argued
    /// out, **not** so the judgement can be skipped — [`Region::undeclared_weights`]
    /// is how it stays visible.
    pub fn weight_for(&self, dim: &str) -> f64 {
        self.weights.get(dim).copied().unwrap_or(1.0)
    }

    /// Dimensions with a band but no declared weight.
    ///
    /// Every entry here is a relative-importance judgement currently being made
    /// silently by a default. The article is explicit that this judgement
    /// belongs in the spec *where it can be argued with*, so it is surfaced
    /// rather than left to be discovered.
    pub fn undeclared_weights(&self) -> Vec<&str> {
        self.intervals
            .iter()
            .filter(|iv| !self.weights.contains_key(&iv.dim))
            .map(|iv| iv.dim.as_str())
            .collect()
    }

    /// Authoring-time well-formedness, so a region that could never hold
    /// anything is a load-time finding.
    pub fn typecheck(&self) -> Result<(), RegionError> {
        for iv in &self.intervals {
            if iv.lo > iv.hi {
                return Err(RegionError::EmptyInterval {
                    dim: iv.dim.clone(),
                    lo: iv.lo,
                    hi: iv.hi,
                });
            }
        }
        for (dim, w) in &self.weights {
            if !w.is_finite() || *w <= 0.0 {
                return Err(RegionError::BadWeight { dim: dim.clone(), w: w.to_string() });
            }
        }
        for expr in &self.relations {
            predicate::parse_predicate(expr).map_err(|e: SyntaxError| RegionError::BadRelation {
                expr: expr.clone(),
                detail: e.to_string(),
            })?;
        }
        Ok(())
    }

    /// `in_region(s, V)`, three-valued.
    pub fn membership(&self, state: &Value) -> Result<Membership, RegionError> {
        let d = self.distance(state)?;
        if !d.is_zero() {
            return Ok(Membership::Outside);
        }
        // Inside the box and every relation holds. Interior or on an edge?
        let reader = State::new(state.clone());
        for iv in &self.intervals {
            let v = number_at(&reader, &iv.dim)?;
            if v == iv.lo || v == iv.hi {
                return Ok(Membership::Boundary);
            }
        }
        Ok(Membership::Interior)
    }

    /// **`W(s) = d(s, V)`** — the urgency quantity.
    ///
    /// Computed on whatever state is handed in, which in practice is the fold of
    /// the log. Nothing here assumes a continuous system.
    pub fn distance(&self, state: &Value) -> Result<Distance, RegionError> {
        let reader = State::new(state.clone());
        let mut per_dimension: Vec<(String, f64)> = Vec::new();

        for iv in &self.intervals {
            let v = number_at(&reader, &iv.dim)?;
            let excess = iv.excess(v);
            if excess > 0.0 {
                per_dimension.push((iv.dim.clone(), excess * self.weight_for(&iv.dim)));
            }
        }
        // Worst first, so "which dimension is binding" is the head of the list.
        per_dimension.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let empty = Map::new();
        let mut relations_violated = Vec::new();
        for expr in &self.relations {
            match predicate::check(expr, state, &empty) {
                // A relation that will not parse is a violation, not a skip —
                // the same fail-safe rule the gate follows.
                Err(e) => {
                    return Err(RegionError::BadRelation {
                        expr: expr.clone(),
                        detail: e.to_string(),
                    })
                }
                Ok((false, _)) => relations_violated.push(expr.clone()),
                Ok((true, _)) => {}
            }
        }

        // ‖·‖_w as a weighted L2 norm over the per-dimension excesses.
        let weighted = per_dimension.iter().map(|(_, e)| e * e).sum::<f64>().sqrt();
        let binding = per_dimension.first().map(|(d, _)| d.clone());

        Ok(Distance { weighted, per_dimension, binding, relations_violated })
    }

    /// The secondary objective **inside** `V`: how far from the edge.
    ///
    /// `None` when the state is outside (use [`Region::distance`]) or when every
    /// bounding side is infinite (there is no edge to be near).
    ///
    /// **This is the BOX margin, not the kernel margin.** The article's
    /// `kernel_margin` is distance from the edge of `Viab_T(V)`, which needs the
    /// viability kernel (§VI) and therefore `T` — not built. The box margin is an
    /// honest **over**-estimate of it (the kernel is a subset of `V`, so its edge
    /// is never further away), and it is named as such rather than passed off as
    /// the real thing.
    pub fn margin(&self, state: &Value) -> Result<Option<f64>, RegionError> {
        if !self.membership(state)?.is_viable() {
            return Ok(None);
        }
        let reader = State::new(state.clone());
        let mut nearest: Option<f64> = None;
        for iv in &self.intervals {
            let v = number_at(&reader, &iv.dim)?;
            let m = iv.margin(v) * self.weight_for(&iv.dim);
            if m.is_finite() {
                nearest = Some(nearest.map_or(m, |n: f64| n.min(m)));
            }
        }
        Ok(nearest)
    }

    /// Does every point of this region lie inside `parent`?
    ///
    /// The composition check of §VIII, read at the region level: a child's viable
    /// region must fit inside its parent's, or the child can be alive while the
    /// parent is not.
    ///
    /// **Honest about relations.** Interval containment is decidable here.
    /// Relations are opaque predicates, so containment is only *provable* when
    /// the child carries every relation the parent does; anything else returns
    /// `false` rather than guessing, and [`Region::fits_within_report`] says why.
    pub fn fits_within(&self, parent: &Region) -> bool {
        self.fits_within_report(parent).fits
    }

    /// [`Region::fits_within`], with the reason.
    pub fn fits_within_report(&self, parent: &Region) -> FitReport {
        let mut unbounded = Vec::new();
        let mut wider = Vec::new();

        for p in &parent.intervals {
            match self.intervals.iter().find(|c| c.dim == p.dim) {
                None => unbounded.push(p.dim.clone()),
                Some(c) if !c.within(p) => wider.push(p.dim.clone()),
                Some(_) => {}
            }
        }
        let missing_relations: Vec<String> = parent
            .relations
            .iter()
            .filter(|r| !self.relations.contains(r))
            .cloned()
            .collect();

        FitReport {
            fits: unbounded.is_empty() && wider.is_empty() && missing_relations.is_empty(),
            unbounded,
            wider,
            missing_relations,
        }
    }
}

/// Why a child region does or does not fit inside a parent's.
#[derive(Debug, Clone, PartialEq)]
pub struct FitReport {
    pub fits: bool,
    /// Dimensions the parent bounds and the child does not bound at all.
    pub unbounded: Vec<String>,
    /// Dimensions where the child's band is wider than the parent's.
    pub wider: Vec<String>,
    /// Parent relations the child does not also carry.
    pub missing_relations: Vec<String>,
}

fn number_at(reader: &State, dim: &str) -> Result<f64, RegionError> {
    reader
        .get(dim)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| RegionError::NotANumber { dim: dim.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A household's region: a non-negative balance, a fridge band, and a
    /// cross-dimension relation.
    fn household() -> Region {
        Region::new()
            .bounding(Interval::at_least("finances.liquid.balance", 0.0))
            .bounding(Interval::new("fridge.temperature", 2.0, 8.0))
            .weighing("finances.liquid.balance", 1.0)
            .weighing("fridge.temperature", 1.0)
    }

    fn state(balance: f64, temp: f64) -> Value {
        json!({"finances": {"liquid": {"balance": balance}}, "fridge": {"temperature": temp}})
    }

    // ── a region can be MEASURED (§V) ───────────────────────────────────────

    #[test]
    fn inside_the_region_the_distance_is_zero() {
        let d = household().distance(&state(1000.0, 5.0)).unwrap();
        assert!(d.is_zero());
        assert_eq!(d.weighted, 0.0);
        assert!(d.binding.is_none());
    }

    #[test]
    fn outside_the_region_the_distance_says_how_far() {
        // A list of booleans can only say "false". A region says 250.
        let d = household().distance(&state(-250.0, 5.0)).unwrap();
        assert!(!d.is_zero());
        assert_eq!(d.weighted, 250.0);
    }

    #[test]
    fn a_band_is_violated_from_either_side() {
        let too_cold = household().distance(&state(0.0, 1.0)).unwrap();
        let too_warm = household().distance(&state(0.0, 12.0)).unwrap();
        assert_eq!(too_cold.weighted, 1.0);
        assert_eq!(too_warm.weighted, 4.0);
    }

    // ── PROJECTED — which dimension is binding (§V) ─────────────────────────

    #[test]
    fn the_binding_dimension_is_named() {
        let d = household().distance(&state(-10.0, 20.0)).unwrap();
        assert_eq!(d.binding.as_deref(), Some("fridge.temperature"),
                   "12 over the ceiling beats 10 under the floor");
        assert_eq!(d.per_dimension.len(), 2, "and both are reported, worst first");
    }

    #[test]
    fn the_declared_weights_change_which_dimension_binds() {
        // THE point of §VII's first warning: shillings and degrees are
        // incommensurable, so the ranking is a judgement — and it is declared.
        let money_matters = household().weighing("finances.liquid.balance", 10.0);
        let d = money_matters.distance(&state(-10.0, 20.0)).unwrap();
        assert_eq!(d.binding.as_deref(), Some("finances.liquid.balance"),
                   "with money weighted 10×, the same state has a different binding dimension");
    }

    #[test]
    fn an_undeclared_weight_is_reported_not_silently_defaulted() {
        let partial = Region::new()
            .bounding(Interval::at_least("a", 0.0))
            .bounding(Interval::at_least("b", 0.0))
            .weighing("a", 2.0);
        assert_eq!(partial.undeclared_weights(), ["b"],
                   "a relative-importance judgement being made by a default must be visible");
        assert_eq!(partial.weight_for("b"), 1.0, "usable meanwhile, but not silent");
    }

    // ── the honest limit of a scalar W ──────────────────────────────────────

    #[test]
    fn a_violated_relation_is_named_not_scored() {
        let r = household().relating("finances.liquid.balance >= fridge.temperature");
        let d = r.distance(&state(1.0, 5.0)).unwrap();
        assert_eq!(d.weighted, 0.0, "every interval is satisfied");
        assert_eq!(d.relations_violated.len(), 1);
        assert!(!d.is_zero(), "a zero box distance with a broken relation is NOT W = 0");
        assert!(!d.is_exact(), "and `weighted` is only a lower bound here");
    }

    #[test]
    fn w_is_zero_exactly_when_the_state_is_viable() {
        // W(s) = 0 ⟺ s ∈ V̄, checked in both directions including the case a
        // box-only distance would get wrong.
        let r = household().relating("finances.liquid.balance >= fridge.temperature");
        for (s, viable) in [
            (state(1000.0, 5.0), true),
            (state(-1.0, 5.0), false),   // interval
            (state(1.0, 5.0), false),    // relation only
        ] {
            let d = r.distance(&s).unwrap();
            assert_eq!(d.is_zero(), viable, "state {s}");
            assert_eq!(r.membership(&s).unwrap().is_viable(), viable, "state {s}");
        }
    }

    #[test]
    fn an_unparseable_relation_refuses_rather_than_passing() {
        let r = household().relating("this is (not ) valid");
        assert!(matches!(r.distance(&state(1.0, 5.0)), Err(RegionError::BadRelation { .. })));
    }

    // ── three-valued membership + the margin (§VII) ─────────────────────────

    #[test]
    fn membership_distinguishes_the_boundary_from_the_interior() {
        // Inside V, W ≡ 0 and cannot rank anything — so the boundary has to be
        // its own answer, or the margin has nowhere to live.
        assert_eq!(household().membership(&state(1000.0, 5.0)).unwrap(), Membership::Interior);
        assert_eq!(household().membership(&state(0.0, 5.0)).unwrap(), Membership::Boundary);
        assert_eq!(household().membership(&state(-1.0, 5.0)).unwrap(), Membership::Outside);
    }

    #[test]
    fn the_margin_distinguishes_comfortably_fine_from_one_bad_week_away() {
        let comfortable = household().margin(&state(1000.0, 5.0)).unwrap().unwrap();
        let precarious = household().margin(&state(1000.0, 7.9)).unwrap().unwrap();
        assert!(comfortable > precarious,
                "both are in V with W = 0; only the margin tells them apart");
        assert!((precarious - 0.1).abs() < 1e-9);
    }

    #[test]
    fn there_is_no_margin_outside_the_region() {
        assert_eq!(household().margin(&state(-1.0, 5.0)).unwrap(), None);
    }

    #[test]
    fn an_entirely_unbounded_region_has_no_edge_to_be_near() {
        let open = Region::new().bounding(Interval::new("x", f64::NEG_INFINITY, f64::INFINITY));
        assert_eq!(open.margin(&json!({"x": 5.0})).unwrap(), None);
    }

    // ── INTERSECTED — does the child fit inside the parent (§V, §VIII) ──────

    #[test]
    fn a_tighter_child_fits_inside_its_parent() {
        let parent = Region::new().bounding(Interval::new("x", 0.0, 100.0));
        let child = Region::new().bounding(Interval::new("x", 10.0, 90.0));
        assert!(child.fits_within(&parent));
    }

    #[test]
    fn a_wider_child_does_not_and_the_dimension_is_named() {
        let parent = Region::new().bounding(Interval::new("x", 0.0, 100.0));
        let child = Region::new().bounding(Interval::new("x", -10.0, 90.0));
        let r = child.fits_within_report(&parent);
        assert!(!r.fits);
        assert_eq!(r.wider, ["x"]);
    }

    #[test]
    fn a_child_that_does_not_bound_a_parent_dimension_at_all_does_not_fit() {
        let parent = Region::new().bounding(Interval::new("x", 0.0, 100.0));
        let child = Region::new().bounding(Interval::new("y", 0.0, 1.0));
        let r = child.fits_within_report(&parent);
        assert!(!r.fits);
        assert_eq!(r.unbounded, ["x"], "unbounded where the parent bounds is not containment");
    }

    #[test]
    fn containment_over_relations_is_provable_only_when_the_child_carries_them() {
        let parent = Region::new().relating("a >= b");
        let bare = Region::new();
        assert!(!bare.fits_within(&parent), "an opaque predicate is not guessed at");
        assert_eq!(bare.fits_within_report(&parent).missing_relations, ["a >= b"]);

        let carrying = Region::new().relating("a >= b");
        assert!(carrying.fits_within(&parent));
    }

    // ── authoring-time typing ───────────────────────────────────────────────

    #[test]
    fn an_empty_interval_is_rejected_at_authoring_time() {
        let bad = Region::new().bounding(Interval::new("x", 10.0, 5.0));
        assert!(matches!(bad.typecheck(), Err(RegionError::EmptyInterval { .. })),
                "a region nothing can be inside is a load-time finding");
    }

    #[test]
    fn a_non_positive_weight_is_rejected() {
        for w in [0.0, -1.0, f64::NAN] {
            let bad = Region::new().bounding(Interval::at_least("x", 0.0)).weighing("x", w);
            assert!(matches!(bad.typecheck(), Err(RegionError::BadWeight { .. })), "w = {w}");
        }
    }

    #[test]
    fn a_well_formed_region_typechecks() {
        assert_eq!(household().relating("finances.liquid.balance >= 0").typecheck(), Ok(()));
    }

    #[test]
    fn a_dimension_that_is_not_a_number_is_refused() {
        let r = Region::new().bounding(Interval::at_least("name", 0.0));
        assert!(matches!(r.distance(&json!({"name": "bonnie"})), Err(RegionError::NotANumber { .. })));
    }

    // ── what this replaces ──────────────────────────────────────────────────

    #[test]
    fn an_unfunded_pocket_can_be_urgent_which_the_pct_proxy_could_never_say() {
        // The reference engine's urgency is `pct = spent / allocated`, and it
        // returns 0.0 whenever `allocated <= 0` — so a pocket with nothing in it
        // and money going out of it is reported as maximally calm.
        //
        // As a region it is simply outside V, by exactly the overspend.
        let pocket = Region::new()
            .bounding(Interval::at_least("pockets.food.remaining", 0.0))
            .weighing("pockets.food.remaining", 1.0);

        let overspent = json!({"pockets": {"food": {"remaining": -300.0}}});
        let d = pocket.distance(&overspent).unwrap();
        assert_eq!(d.weighted, 300.0, "300 outside, not 0.0");
        assert!(!d.is_zero());
        assert_eq!(d.binding.as_deref(), Some("pockets.food.remaining"));
    }
}
