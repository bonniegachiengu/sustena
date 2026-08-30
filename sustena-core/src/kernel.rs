//! The viability kernel `Viab_T(V)` and runway `R(s)` (Sustain §VI · SUS-10, OPV-19).
//!
//! > Here is the formal version of the point of no return.
//!
//! ```text
//! Viab_T(V) = { s₀ ∈ V : ∃ (o₁,o₂,…) ∈ Tʷ, ∀k ≥ 0, sₖ ∈ V }
//! ```
//!
//! The states from which **some** sequence of legal moves keeps the Sustain
//! inside `V` **forever** — Aubin's viability kernel, and *the correct target for
//! "keep this thing alive"*. Being in `V` is not it: `V` is where you are, the
//! kernel is where you can stay.
//!
//! ## The distinction a household actually needs
//!
//! > `balance ≥ 0` today puts you in `V`. If no sequence of available Enzymes
//! > keeps `balance ≥ 0` through Thursday's rent, you are in `V` and **outside
//! > `Viab_T(V)` — fine right now, already lost.** Sustena's job is to say that
//! > on Monday.
//!
//! That case is the proof of value, and it is a conformance vector rather than a
//! sentence: a state that is comfortably inside `V`, with `W = 0` and a healthy
//! box margin, and no path through the week.
//!
//! ## The characterisation is the useful part
//!
//! `Viab_T(V)` is the **largest** `D ⊆ V` with `∀s∈D ∃o∈T: o(s) ∈ D` — the
//! greatest fixed point of `Φ(D) = {s ∈ D : ∃o ∈ T, o(s) ∈ D}`, computed by
//! **descending iteration**:
//!
//! ```text
//! D₀ = V,   D_{k+1} = Φ(D_k),   Viab_T(V) = ⋂ D_k
//! ```
//!
//! Each pass deletes the states whose every legal move leads somewhere already
//! deleted. On a finite or discretised space this terminates.
//!
//! ## Two honest limitations, and how each is encoded
//!
//! **(1) Exact kernels are exponential in the number of dimensions.** So this
//! computes the kernel over an **explicitly enumerated** state set — a finite or
//! discretised space the caller supplies. That is the honest exact form: it is
//! genuinely `Viab_T(V)` *for the space it was given*, and it makes no claim
//! about the states nobody enumerated.
//!
//! **(2) The finite-horizon `Viab^H` is a SUPERSET of the true kernel**, because
//! surviving `H` steps is easier than surviving forever. A horizon-limited
//! answer is therefore **optimistic** and must be reported as such rather than
//! as a guarantee — so [`Kernel`] is an enum whose [`Kernel::Horizon`] variant
//! **has no way to claim it is exact**. [`Kernel::is_guarantee`] is true only for
//! [`Kernel::Exact`], and there is no constructor that produces an `Exact` from
//! a bounded run. Reporting an optimistic set as a guarantee is unspellable, not
//! merely discouraged.
//!
//! ## Runway — the cheap half (OPV-19)
//!
//! ```text
//! R(s) = min{ k : sₖ ∉ V }    under the null policy
//! ```
//!
//! **One policy, one forward scan** — which is why the Operative article calls it
//! the cheapest §X term to make real. The null policy takes no action; the world
//! still moves, because obligations fall due whether or not anyone acts. That is
//! what makes runway a *clock* rather than a restatement of `W`.
//!
//! It is an **under-estimate in a declared direction**, and the direction is the
//! point: a better policy can only last longer, never shorter. [`Runway`] carries
//! that sign rather than leaving the caller to remember it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use thiserror::Error;

use crate::compose::Change;
use crate::region::{Region, RegionError};
use crate::state::State;

/// A declared change to one path, reusing [`crate::compose::Change`] so the
/// engine has one vocabulary for "what an Enzyme does" rather than two.
pub type Effect = Vec<(String, Change)>;

fn apply_effect(state: &Value, effect: &Effect) -> Result<Value, KernelError> {
    let mut working = State::new(state.clone());
    for (path, change) in effect {
        let next = match change {
            Change::SetTo(v) => v.clone(),
            Change::ShiftBy(d) => {
                let current = working
                    .get(path)
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| KernelError::NotANumber { path: path.clone() })?;
                Value::from(current + d)
            }
            // A move nobody described cannot be simulated, so it cannot be part
            // of a survival claim. Refused rather than skipped: skipping would
            // quietly shrink T and make the kernel look smaller than it is.
            Change::Opaque => return Err(KernelError::OpaqueEffect { path: path.clone() }),
        };
        working
            .set(path, next)
            .map_err(|e| KernelError::Unapplicable { path: path.clone(), detail: e.to_string() })?;
    }
    Ok(working.snapshot())
}

/// One element of `T` — a legal move, as data.
#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub name: String,
    /// `g_o` — predicate expressions that must hold before it may be applied.
    pub guard: Vec<String>,
    pub effect: Effect,
}

impl Move {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), guard: vec![], effect: vec![] }
    }

    pub fn guarded_by(mut self, expr: &str) -> Self {
        self.guard.push(expr.to_string());
        self
    }

    pub fn changing(mut self, path: &str, change: Change) -> Self {
        self.effect.push((path.to_string(), change));
        self
    }

    /// **Where this move lands from `state`, if it is legal and stays in the
    /// enumerated space.**
    ///
    /// ★★★ Public so [`crate::reach`] can ask the same question the kernel asks,
    /// through the same code. Two definitions of "a legal move" would let the
    /// kernel and the reach disagree about what the system can do, and the
    /// disagreement would be silent.
    pub fn lands_on(
        &self,
        space: &Space,
        state: &Value,
    ) -> Result<Option<String>, KernelError> {
        if !self.admissible(state)? {
            return Ok(None);
        }
        let after = apply_effect(state, &self.effect)?;
        Ok(successor(space, &after))
    }

    fn admissible(&self, state: &Value) -> Result<bool, KernelError> {
        let empty = Map::new();
        for g in &self.guard {
            let (ok, _) = crate::predicate::check(g, state, &empty)
                .map_err(|e| KernelError::BadGuard { expr: g.clone(), detail: e.to_string() })?;
            if !ok {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// An exogenous obligation — something that happens **whether or not anyone
/// acts**. Thursday's rent.
///
/// Runway needs these: a null policy that changed nothing in a world that
/// changed nothing would never leave `V`, and the clock would never tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Obligation {
    /// The step it falls due on, counting from 0.
    pub at: usize,
    pub name: String,
    pub effect: Effect,
}

impl Obligation {
    pub fn new(at: usize, name: &str) -> Self {
        Self { at, name: name.to_string(), effect: vec![] }
    }

    pub fn changing(mut self, path: &str, change: Change) -> Self {
        self.effect.push((path.to_string(), change));
        self
    }
}

/// The result of a kernel computation.
///
/// **`Horizon` cannot claim to be a guarantee.** There is no constructor that
/// turns a bounded run into an `Exact`, so the optimism of a finite-horizon
/// answer cannot be dropped on the way to a caller.
#[derive(Debug, Clone, PartialEq)]
pub enum Kernel {
    /// The descending iteration reached its greatest fixed point. This **is**
    /// `Viab_T(V)` for the enumerated space.
    Exact {
        states: BTreeSet<String>,
        /// How many passes it took to converge.
        passes: usize,
    },
    /// `Viab^H` — "can stay in `V` for `H` steps".
    ///
    /// **A SUPERSET of the true kernel, and therefore OPTIMISTIC**: surviving
    /// `H` steps is easier than surviving forever, so a state in here may still
    /// be doomed just beyond the horizon.
    Horizon { states: BTreeSet<String>, horizon: usize },
}

impl Kernel {
    /// True only for [`Kernel::Exact`].
    ///
    /// A horizon answer is evidence, not a promise — and the difference is a
    /// property of the type rather than something a caller has to remember.
    pub fn is_guarantee(&self) -> bool {
        matches!(self, Kernel::Exact { .. })
    }

    pub fn states(&self) -> &BTreeSet<String> {
        match self {
            Kernel::Exact { states, .. } | Kernel::Horizon { states, .. } => states,
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.states().contains(id)
    }

    pub fn len(&self) -> usize {
        self.states().len()
    }

    pub fn is_empty(&self) -> bool {
        self.states().is_empty()
    }
}

/// `R(s)` — the point-of-no-return clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Runway {
    /// The first step at which the null policy leaves `V`. `None` when the scan
    /// reached its horizon still inside.
    pub steps: Option<usize>,
    /// How far the scan looked.
    pub horizon: usize,
    /// The obligation that pushed it out, when one did.
    pub breached_by: Option<String>,
}

impl Runway {
    /// Under the null policy nobody acted, so **a real policy can only last
    /// longer, never shorter**. The sign of the bound, carried rather than
    /// remembered.
    pub fn is_under_estimate(&self) -> bool {
        true
    }

    /// `true` when the scan ended still inside `V` — which is **not** a promise
    /// of survival, only the absence of evidence within `horizon`.
    pub fn survived_the_scan(&self) -> bool {
        self.steps.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum KernelError {
    #[error("'{path}' does not resolve to a number")]
    NotANumber { path: String },
    #[error("guard '{expr}' could not be parsed: {detail}")]
    BadGuard { expr: String, detail: String },
    #[error("'{path}' is changed opaquely, so this move cannot be simulated — a survival claim cannot rest on a move nobody described")]
    OpaqueEffect { path: String },
    #[error("could not apply a change to '{path}': {detail}")]
    Unapplicable { path: String, detail: String },
    #[error("{0}")]
    Region(#[from] RegionError),
}

/// A finite or discretised state space: `(id, state)` pairs.
pub type Space = BTreeMap<String, Value>;

/// The states of `space` that lie in `V`. `D₀`.
fn in_region(region: &Region, space: &Space) -> Result<BTreeSet<String>, KernelError> {
    let mut out = BTreeSet::new();
    for (id, s) in space {
        if region.membership(s)?.is_viable() {
            out.insert(id.clone());
        }
    }
    Ok(out)
}

/// Find which enumerated state a move lands on, if any.
///
/// A move that leaves the enumerated space is treated as **not** keeping the
/// system inside `D`, which is the conservative reading: the kernel may only
/// shrink from not knowing where a move went, never grow.
fn successor(space: &Space, after: &Value) -> Option<String> {
    space.iter().find(|(_, v)| *v == after).map(|(id, _)| id.clone())
}

/// `Φ(D) = { s ∈ D : ∃o ∈ T, o(s) ∈ D }` — one pass of the descending iteration.
fn phi(
    space: &Space,
    moves: &[Move],
    d: &BTreeSet<String>,
) -> Result<BTreeSet<String>, KernelError> {
    let mut survivors = BTreeSet::new();
    for id in d {
        let s = &space[id];
        for m in moves {
            if !m.admissible(s)? {
                continue;
            }
            let after = apply_effect(s, &m.effect)?;
            if let Some(next) = successor(space, &after) {
                if d.contains(&next) {
                    survivors.insert(id.clone());
                    break;
                }
            }
        }
    }
    Ok(survivors)
}

/// **`Viab_T(V)`**, exactly, over an enumerated space (Sustain §VI).
///
/// Descending iteration from `V` until the greatest fixed point. Terminates
/// because each pass either removes at least one state or stops.
///
/// This is exact **for the space it was given** — see the module docs on why
/// enumeration is the honest form of an exponential problem.
pub fn viability_kernel(
    region: &Region,
    space: &Space,
    moves: &[Move],
) -> Result<Kernel, KernelError> {
    let mut d = in_region(region, space)?;
    let mut passes = 0;
    loop {
        let survivors = phi(space, moves, &d)?;
        passes += 1;
        if survivors == d {
            return Ok(Kernel::Exact { states: d, passes });
        }
        d = survivors;
    }
}

/// **`Viab^H`** — "can stay in `V` for `H` steps".
///
/// The receding-horizon approximation the article names for when an exact
/// kernel is out of reach. Returns [`Kernel::Horizon`], which **cannot** be
/// mistaken for a guarantee: it is a superset of the true kernel, so a state in
/// it may still be doomed at `H+1`.
pub fn viability_kernel_horizon(
    region: &Region,
    space: &Space,
    moves: &[Move],
    horizon: usize,
) -> Result<Kernel, KernelError> {
    let mut d = in_region(region, space)?;
    for _ in 0..horizon {
        let survivors = phi(space, moves, &d)?;
        if survivors == d {
            break;
        }
        d = survivors;
    }
    Ok(Kernel::Horizon { states: d, horizon })
}

/// **`R(s)`** — runway under the null policy (OPV-19).
///
/// One policy, one forward scan. Obligations fall due whether or not anyone
/// acts; the scan applies them and asks when `V` is first left.
pub fn runway(
    region: &Region,
    start: &Value,
    schedule: &[Obligation],
    horizon: usize,
) -> Result<Runway, KernelError> {
    let mut s = start.clone();

    // Leaving on step 0 is a real answer: already outside before anything falls
    // due. Reported as 0 rather than as "no runway", which would be the same
    // number dressed as an absence.
    if !region.membership(&s)?.is_viable() {
        return Ok(Runway { steps: Some(0), horizon, breached_by: None });
    }

    for k in 0..horizon {
        let mut last = None;
        for ob in schedule.iter().filter(|o| o.at == k) {
            s = apply_effect(&s, &ob.effect)?;
            last = Some(ob.name.clone());
        }
        if !region.membership(&s)?.is_viable() {
            return Ok(Runway { steps: Some(k + 1), horizon, breached_by: last });
        }
    }
    Ok(Runway { steps: None, horizon, breached_by: None })
}

/// **The kernel margin** — the secondary objective inside `V`, done properly.
///
/// `region::Region::margin` measures distance to the edge of `V`, which is an
/// honest **over**-estimate: the kernel is a subset of `V`, so its edge is never
/// further away. This measures distance to the edge of the **kernel** — the
/// nearest state that is in `V` but from which survival is not possible.
///
/// `None` when every viable state is in the kernel (there is no edge to be near)
/// or when the state is not in the kernel at all (use [`runway`] instead).
pub fn kernel_margin(
    region: &Region,
    space: &Space,
    kernel: &Kernel,
    id: &str,
) -> Result<Option<f64>, KernelError> {
    if !kernel.contains(id) {
        return Ok(None);
    }
    let here = &space[id];
    let viable = in_region(region, space)?;

    let mut nearest: Option<f64> = None;
    for doomed in viable.difference(kernel.states()) {
        let d = weighted_gap(region, here, &space[doomed])?;
        nearest = Some(nearest.map_or(d, |n: f64| n.min(d)));
    }
    Ok(nearest)
}

/// Weighted distance between two states over the region's declared dimensions,
/// using the region's own weights so the kernel margin and `W` are commensurate.
fn weighted_gap(region: &Region, a: &Value, b: &Value) -> Result<f64, KernelError> {
    let (ra, rb) = (State::new(a.clone()), State::new(b.clone()));
    let mut sum = 0.0;
    for iv in &region.intervals {
        let x = ra.get(&iv.dim).and_then(|v| v.as_f64())
            .ok_or_else(|| KernelError::NotANumber { path: iv.dim.clone() })?;
        let y = rb.get(&iv.dim).and_then(|v| v.as_f64())
            .ok_or_else(|| KernelError::NotANumber { path: iv.dim.clone() })?;
        let d = (x - y) * region.weight_for(&iv.dim);
        sum += d * d;
    }
    Ok(sum.sqrt())
}

/// Whether an intervention moves **into or within the kernel** — a strictly
/// stronger condition than the Lyapunov descent toward `V` of
/// [`crate::controller::is_stable_intervention`].
///
/// Descent toward `V` says the household is getting closer to being alive today.
/// Descent toward the kernel says it is getting closer to being able to *stay*
/// alive, which is the question that actually matters — and the two genuinely
/// differ, because a move can reduce `W` while stepping out of the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelStability {
    pub before_in_kernel: bool,
    pub after_in_kernel: bool,
}

impl KernelStability {
    /// Ends inside the kernel. The strong condition.
    pub fn stable(&self) -> bool {
        self.after_in_kernel
    }

    /// The move left the kernel — a step toward being *"fine right now, already
    /// lost"*, even if `W` went down.
    pub fn abandons_the_kernel(&self) -> bool {
        self.before_in_kernel && !self.after_in_kernel
    }
}

/// [`KernelStability`] for a move between two enumerated states.
pub fn is_stable_toward_kernel(kernel: &Kernel, before: &str, after: &str) -> KernelStability {
    KernelStability {
        before_in_kernel: kernel.contains(before),
        after_in_kernel: kernel.contains(after),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::Interval;
    use serde_json::json;

    fn region() -> Region {
        Region::new()
            .bounding(Interval::at_least("balance", 0.0))
            .weighing("balance", 1.0)
    }

    /// A discretised ledger: balances 0, 10, 20, 30, and one already-negative.
    fn space() -> Space {
        [
            ("neg", -10.0),
            ("b0", 0.0),
            ("b10", 10.0),
            ("b20", 20.0),
            ("b30", 30.0),
        ]
        .into_iter()
        .map(|(id, v)| (id.to_string(), json!({ "balance": v })))
        .collect()
    }

    /// You may top up while under the ceiling. Above it, funds are locked away
    /// and neither move is available — which is what makes the kernel partial
    /// rather than trivially equal to V.
    fn earn() -> Move {
        Move::new("earn")
            .guarded_by("balance <= 20")
            .changing("balance", Change::ShiftBy(10.0))
    }

    fn spend() -> Move {
        Move::new("spend")
            .guarded_by("balance >= 10")
            .guarded_by("balance <= 20")
            .changing("balance", Change::ShiftBy(-10.0))
    }

    // ── the descending iteration (§VI) ──────────────────────────────────────

    #[test]
    fn the_kernel_is_a_proper_subset_of_v() {
        // THE distinction the module exists for: b30 satisfies `balance >= 0`,
        // so it is in V — and every move is blocked there, so no sequence keeps
        // it alive. In V, outside the kernel: fine right now, already lost.
        let (r, sp) = (region(), space());
        let k = viability_kernel(&r, &sp, &[earn(), spend()]).unwrap();
        assert!(k.is_guarantee());
        for id in ["b0", "b10", "b20"] {
            assert!(k.contains(id), "{id} has a move that keeps it going");
        }
        assert!(r.membership(&sp["b30"]).unwrap().is_viable(), "b30 IS in V");
        assert!(!k.contains("b30"), "...and is not in the kernel");
        assert!(!k.contains("neg"), "already outside V, so never in the kernel");
    }

    #[test]
    fn a_state_with_no_admissible_move_is_deleted_on_the_first_pass() {
        // Not knowing where a move went, or having none at all, can only shrink
        // the kernel — never grow it.
        let k = viability_kernel(&region(), &space(), &[earn(), spend()]).unwrap();
        assert!(!k.contains("b30"));
    }

    #[test]
    fn a_state_whose_every_move_leads_somewhere_deleted_is_deleted_too() {
        // Only `spend` is legal. b10 → b0; b0 cannot spend (guard) and has no
        // other move, so b0 goes first, then b10 follows it out, and so on.
        let k = viability_kernel(&region(), &space(), &[spend()]).unwrap();
        assert!(k.is_empty(), "the cascade empties the kernel: {:?}", k.states());
    }

    #[test]
    fn the_iteration_reaches_a_fixed_point_and_says_how_many_passes() {
        let k = viability_kernel(&region(), &space(), &[earn(), spend()]).unwrap();
        match k {
            Kernel::Exact { passes, .. } => assert!(passes >= 1),
            other => panic!("expected an exact kernel, got {other:?}"),
        }
    }

    #[test]
    fn a_cycle_survives_forever() {
        // earn then spend, back and forth: b10 ⇄ b20 is a genuine ω-trajectory.
        let k = viability_kernel(&region(), &space(), &[earn(), spend()]).unwrap();
        assert!(k.contains("b10") && k.contains("b20"));
    }

    #[test]
    fn an_opaque_move_is_refused_rather_than_skipped() {
        // Skipping would quietly shrink T and make the kernel look smaller than
        // it is — a pessimistic lie is still a lie.
        let m = Move::new("mystery").changing("balance", Change::Opaque);
        assert!(matches!(
            viability_kernel(&region(), &space(), &[m]),
            Err(KernelError::OpaqueEffect { .. })
        ));
    }

    // ── Viab^H is a SUPERSET, and cannot claim otherwise ────────────────────

    #[test]
    fn a_horizon_answer_is_never_a_guarantee() {
        let k = viability_kernel_horizon(&region(), &space(), &[spend()], 1).unwrap();
        assert!(!k.is_guarantee(), "surviving H steps is easier than surviving forever");
        // `Kernel::Exact { .. }` cannot be built from a horizon run: there is no
        // constructor for it, so the optimism cannot be dropped en route.
    }

    #[test]
    fn the_horizon_set_contains_the_true_kernel() {
        // Viab^H ⊇ Viab_T(V), for every H. Checked rather than asserted.
        let (r, sp, mv) = (region(), space(), vec![spend()]);
        let exact = viability_kernel(&r, &sp, &mv).unwrap();
        for h in [1, 2, 3] {
            let horizon = viability_kernel_horizon(&r, &sp, &mv, h).unwrap();
            assert!(
                exact.states().is_subset(horizon.states()),
                "H = {h} must be optimistic, never pessimistic"
            );
        }
    }

    #[test]
    fn a_longer_horizon_is_never_looser() {
        let (r, sp, mv) = (region(), space(), vec![spend()]);
        let short = viability_kernel_horizon(&r, &sp, &mv, 1).unwrap();
        let long = viability_kernel_horizon(&r, &sp, &mv, 4).unwrap();
        assert!(long.states().is_subset(short.states()), "more looking only removes");
    }

    // ── runway (OPV-19) ─────────────────────────────────────────────────────

    /// **The proof of value**, and the article's own example.
    #[test]
    fn fine_today_and_already_lost_by_thursday() {
        let s = json!({"balance": 500.0});
        let r = region();

        // Today: comfortably inside V, W = 0, and a healthy box margin.
        assert!(r.membership(&s).unwrap().is_viable());
        assert_eq!(r.distance(&s).unwrap().weighted, 0.0);
        assert_eq!(r.margin(&s).unwrap(), Some(500.0));

        // ...and rent falls due on Thursday whether or not anyone acts.
        let rent = Obligation::new(3, "rent").changing("balance", Change::ShiftBy(-800.0));
        let out = runway(&r, &s, &[rent], 14).unwrap();
        assert_eq!(out.steps, Some(4), "R(s) = 4: fine on Monday, gone on Thursday");
        assert_eq!(out.breached_by.as_deref(), Some("rent"));
        assert!(out.is_under_estimate(), "a real policy can only last longer");
    }

    #[test]
    fn a_solvent_household_survives_the_scan() {
        let s = json!({"balance": 5000.0});
        let rent = Obligation::new(3, "rent").changing("balance", Change::ShiftBy(-800.0));
        let out = runway(&region(), &s, &[rent], 14).unwrap();
        assert!(out.survived_the_scan());
        assert_eq!(out.steps, None);
    }

    #[test]
    fn surviving_the_scan_is_not_a_promise() {
        // The horizon ran out; that is the absence of evidence, not evidence of
        // absence — which is exactly why `steps` is None rather than ∞.
        let s = json!({"balance": 100.0});
        let rent = Obligation::new(5, "rent").changing("balance", Change::ShiftBy(-800.0));
        let short = runway(&region(), &s, std::slice::from_ref(&rent), 3).unwrap();
        let long = runway(&region(), &s, std::slice::from_ref(&rent), 14).unwrap();
        assert!(short.survived_the_scan(), "not seen inside 3 steps");
        assert_eq!(long.steps, Some(6), "but it was there all along");
    }

    #[test]
    fn already_outside_is_a_runway_of_zero_not_an_absence() {
        let out = runway(&region(), &json!({"balance": -1.0}), &[], 10).unwrap();
        assert_eq!(out.steps, Some(0));
    }

    #[test]
    fn repeated_obligations_accumulate() {
        let s = json!({"balance": 250.0});
        let weekly: Vec<Obligation> = (0..10)
            .map(|k| Obligation::new(k, "weekly").changing("balance", Change::ShiftBy(-100.0)))
            .collect();
        let out = runway(&region(), &s, &weekly, 10).unwrap();
        assert_eq!(out.steps, Some(3), "250 survives two hundreds, not three");
    }

    // ── the kernel margin upgrades the box margin ───────────────────────────

    #[test]
    fn the_kernel_margin_is_tighter_than_the_box_margin() {
        // The box margin measures distance to the edge of V; the kernel margin
        // measures distance to the edge of the SET YOU CAN STAY IN, which is
        // never further away and is usually nearer.
        let (r, sp) = (region(), space());
        let k = viability_kernel(&r, &sp, &[earn(), spend()]).unwrap();
        assert!(k.contains("b20") && !k.contains("b30"));

        let box_margin = r.margin(&sp["b20"]).unwrap().expect("in V");
        let kern = kernel_margin(&r, &sp, &k, "b20").unwrap().expect("in the kernel");
        assert_eq!(box_margin, 20.0, "20 above the floor of V");
        assert_eq!(kern, 10.0, "but only 10 from b30, which is in V and doomed");
        assert!(kern < box_margin, "the box margin was an over-estimate, as declared");
    }

    #[test]
    fn a_state_outside_the_kernel_has_no_margin() {
        let (r, sp) = (region(), space());
        let k = viability_kernel(&r, &sp, &[earn(), spend()]).unwrap();
        assert_eq!(kernel_margin(&r, &sp, &k, "b30").unwrap(), None,
                   "outside the kernel the question is runway, not room");
    }

    #[test]
    fn there_is_no_kernel_edge_when_every_viable_state_survives() {
        let (r, sp) = (region(), space());
        // Both moves: every non-negative state can cycle, except b30 which
        // cannot earn within the space — so trim the space to make the point.
        let small: Space = sp.iter().filter(|(id, _)| *id != "b30" && *id != "neg")
            .map(|(k, v)| (k.clone(), v.clone())).collect();
        let k = viability_kernel(&r, &small, &[earn(), spend()]).unwrap();
        assert_eq!(k.len(), small.len(), "everything viable survives");
        assert_eq!(kernel_margin(&r, &small, &k, "b10").unwrap(), None, "no edge to be near");
    }

    // ── sharpening CTL-2 ────────────────────────────────────────────────────

    #[test]
    fn a_move_can_reduce_w_and_still_abandon_the_kernel() {
        // THE reason descent toward the kernel is the stronger condition. The
        // Lyapunov test sees an improvement; the kernel test sees a household
        // walking out of the set it could have stayed in.
        let (r, sp) = (region(), space());
        let k = viability_kernel(&r, &sp, &[earn(), spend()]).unwrap();
        assert!(k.contains("b20") && !k.contains("b30"));

        // b20 → b30 moves AWAY from V's floor, so W is unchanged at 0 and the
        // box margin actually improves — yet it leaves the kernel.
        let ks = is_stable_toward_kernel(&k, "b20", "b30");
        assert!(ks.abandons_the_kernel());
        assert!(!ks.stable());
        assert!(r.margin(&sp["b30"]).unwrap().unwrap() > r.margin(&sp["b20"]).unwrap().unwrap(),
                "further from V's edge, and still the wrong move");
    }

    #[test]
    fn a_move_within_the_kernel_is_stable_by_the_strong_condition() {
        let k = viability_kernel(&region(), &space(), &[earn(), spend()]).unwrap();
        let ks = is_stable_toward_kernel(&k, "b10", "b20");
        assert!(ks.stable() && !ks.abandons_the_kernel());
    }

    #[test]
    fn re_entering_the_kernel_is_stable_too() {
        let k = viability_kernel(&region(), &space(), &[earn(), spend()]).unwrap();
        let ks = is_stable_toward_kernel(&k, "b30", "b20");
        assert!(ks.stable(), "ending inside is what matters");
        assert!(!ks.abandons_the_kernel());
    }
}
