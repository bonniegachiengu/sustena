//! Damping — stability's frequency-domain face (CONTROLLER, Additions ·
//! CTL-11).
//!
//! > *"§II's Lyapunov condition is a time-domain guarantee that `V` decreases.
//! > It does not, by itself, forbid a controller that reduces `V` while
//! > oscillating — over-correcting each period. [...] A well-tuned Controller
//! > keeps the loop's response damped (its poles inside the unit circle, in the
//! > discrete form), so corrections settle rather than ring. **Lyapunov says the
//! > gap closes; damping says it closes without oscillation.**"*
//!
//! ## ★★ The gap this closes, and it is not hypothetical
//!
//! CTL-2's [`is_stable_intervention`](crate::controller::is_stable_intervention)
//! is `W(after) ≤ W(before)`, checked per step. Under a proportional law
//! `e_{k+1} = (1−g)·e_k` with a gain in `(1, 2]`, **that check passes at every
//! single step** while the state flips from one side of `V` to the other each
//! time:
//!
//! - `1 < g < 2` — `|1−g| < 1`, so `W` strictly decreases every step. It
//!   converges **and** rings.
//! - `g = 2` — the pole is exactly `−1`, so `W_{k+1} = W_k`. Lyapunov's `≤`
//!   holds **forever**, with equality, and the loop never settles.
//!
//! A conformance case runs the **real** `is_stable_intervention` over such a
//! trajectory and asserts it returns stable at every step. Only the damping
//! analysis separates *settles* from *rings*, which is precisely the article's
//! sentence made checkable.
//!
//! ## ★★ Honest fit: the pole where a gain exists, the trajectory where not
//!
//! *"Poles inside the unit circle"* is a **linear** characterization — it needs
//! a transfer function with a fixed gain to take poles of. So, the same
//! discipline MON-1 applies to §I's observability matrix:
//!
//! - **Where a proportional gain is genuinely declared**, the pole is `(1 − g)`
//!   and the analysis is exact. [`ProportionalLaw::classify`] computes it, and
//!   the four verdicts fall straight out of `|1−g|` and its sign.
//! - **Where the loop is discrete operator choices with no fixed gain**, the
//!   Sustena-native form is used instead: **ringing detected in the `W`
//!   trajectory itself** — alternating-sign steps ([`read_trajectory`]) and
//!   repeated boundary crossings ([`crossings`]).
//!
//! ★ **Those two native readings are not redundant, and a test says why.** `W`
//! is a magnitude, so a loop that flips side every step while converging has a
//! **monotonically falling** `W` — the magnitude series reads it `Damped` and
//! is not wrong to. The *side* series is what carries the ringing there. Each
//! reading sees something the other cannot, and using only one would have made
//! the underdamped case invisible in exactly the way Lyapunov already makes it
//! invisible.
//!
//! ★★ And the discipline is structural rather than promised: **there is no
//! `fit_gain(trajectory) -> ProportionalLaw`.** A law is declared or it does
//! not exist. Inferring one from a trajectory to get poles out of it would be
//! exactly the fabricated-`A` trap MON-1 declined the observability matrix to
//! avoid.
//!
//! ## ★ The concrete discrete failure: overshoot
//!
//! An intervention that moves the state **past** `V` — from below the band to
//! above it — genuinely reduces `W` and so passes Lyapunov, but now sits on the
//! far side needing a correction back. That is the over-corrector §II permits,
//! and [`overshoot`] names it per dimension with both sides and both values, so
//! the finding is checkable rather than a flag.
//!
//! A damped correction brings the state **to** the region, or approaches it
//! from the same side. The remedy for the other is the row's own word: reduce
//! the correction magnitude — damp the gain — so it settles.
//!
//! ## Complement, not replacement
//!
//! Nothing here supersedes CTL-2. Lyapunov says the gap closes; damping says it
//! closes without ringing, and a controller wants both. `W = d(s,V)` is
//! [`crate::region`]'s, the trajectory is the same series `detect.rs`'s EWMA
//! runs over, and the per-step check is `controller.rs`'s — none of the three
//! is rebuilt here.
//!
//! **A slot, named rather than built:** the Additions also note an
//! intervention's worth is its Lyapunov gain *net of pawa*, so the efficiency
//! of not spending corrections on ringing is economy-adjacent — and the economy
//! layer is parked.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::region::{Region, RegionError};

/// Which side of a declared band a value sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Below,
    Inside,
    Above,
}

impl Side {
    /// ★ A crossing — one side to the *opposite* one. Passing through `Inside`
    /// is not a crossing; landing on the far side is.
    pub fn crossed_to(self, other: Side) -> bool {
        matches!((self, other), (Side::Below, Side::Above) | (Side::Above, Side::Below))
    }
}

/// What can go wrong reading a trajectory.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DampingError {
    #[error("a proportional gain must be finite; {0} is not a gain")]
    BadGain(f64),
    #[error(transparent)]
    Region(#[from] RegionError),
}

// ── the pole, where a gain genuinely exists ────────────────────────────────

/// A declared proportional control law `e_{k+1} = (1 − g)·e_k`.
///
/// ★★ **Declared, never inferred.** There is deliberately no
/// `fit_gain(trajectory)` — a law someone stated is a fact about the
/// controller; a law fitted to a trajectory is a story about it, and taking
/// poles of a fitted model is the fabricated-`A` trap MON-1 declined the
/// observability matrix to avoid.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProportionalLaw {
    gain: f64,
}

impl ProportionalLaw {
    pub fn declared(gain: f64) -> Result<Self, DampingError> {
        if !gain.is_finite() {
            return Err(DampingError::BadGain(gain));
        }
        Ok(Self { gain })
    }

    pub fn gain(&self) -> f64 {
        self.gain
    }

    /// The discrete pole, `1 − g`. Inside the unit circle ⟺ `|1 − g| < 1`.
    pub fn pole(&self) -> f64 {
        1.0 - self.gain
    }

    /// One step of the law on a signed error.
    pub fn step(&self, error: f64) -> f64 {
        self.pole() * error
    }

    /// ★ Where the pole lands, and therefore how the loop behaves.
    pub fn classify(&self) -> PoleVerdict {
        let p = self.pole();
        if p.abs() >= 1.0 {
            PoleVerdict::Undamped { pole: p, sustained: p == -1.0 }
        } else if p == 0.0 {
            PoleVerdict::Deadbeat
        } else if p > 0.0 {
            PoleVerdict::Damped { pole: p }
        } else {
            PoleVerdict::Underdamped { pole: p }
        }
    }
}

/// Where a declared law's pole lands.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PoleVerdict {
    /// `0 < 1−g < 1` (`0 < g < 1`): settles from the same side, no ringing.
    Damped { pole: f64 },
    /// `g = 1` exactly: one step to zero.
    Deadbeat,
    /// ★★ `−1 < 1−g < 0` (`1 < g < 2`): **inside the unit circle, so it
    /// converges — and the sign flips every step, so it rings on the way.**
    /// This is the case Lyapunov passes at every step and cannot see.
    Underdamped { pole: f64 },
    /// `|1−g| ≥ 1`: it does not settle. `sustained` marks `pole = −1` exactly,
    /// where the amplitude is *constant* — so `W(after) ≤ W(before)` holds
    /// forever, with equality, and the loop rings for ever.
    Undamped { pole: f64, sustained: bool },
}

impl PoleVerdict {
    /// Does the loop settle at all?
    pub fn settles(&self) -> bool {
        !matches!(self, PoleVerdict::Undamped { .. })
    }

    /// ★ Does it ring on the way — a sign flip each step?
    pub fn rings(&self) -> bool {
        matches!(self, PoleVerdict::Underdamped { .. } | PoleVerdict::Undamped { .. })
    }

    /// The remedy the row names: reduce the correction magnitude.
    pub fn advice(&self) -> Option<String> {
        match self {
            PoleVerdict::Damped { .. } | PoleVerdict::Deadbeat => None,
            PoleVerdict::Underdamped { pole } => Some(format!(
                "pole {pole:.3} is inside the unit circle but negative — it converges while \
                 flipping side every step. Reduce the gain below 1 to settle without ringing"
            )),
            PoleVerdict::Undamped { pole, sustained } => Some(if *sustained {
                format!(
                    "pole {pole:.3} sits ON the unit circle — the amplitude never changes, so \
                     Lyapunov's `≤` holds forever with equality while nothing settles. Reduce the \
                     gain below 2"
                )
            } else {
                format!(
                    "pole {pole:.3} is outside the unit circle — corrections grow. Reduce the gain"
                )
            }),
        }
    }
}

// ── overshoot: the concrete discrete failure ───────────────────────────────

/// One dimension crossed from one side of its band to the opposite side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overshoot {
    pub dimension: String,
    pub from: Side,
    pub to: Side,
    pub before: f64,
    pub after: f64,
}

impl Overshoot {
    pub fn describe(&self) -> String {
        format!(
            "'{}' overshot: {:?} at {} → {:?} at {}. W fell, so Lyapunov passes — but the state \
             is now on the far side and needs a correction back, which is the ringing this check \
             exists to catch",
            self.dimension, self.from, self.before, self.to, self.after
        )
    }
}

/// Which side of its declared band a value sits on.
fn side_of(lo: f64, hi: f64, v: f64) -> Side {
    if v < lo {
        Side::Below
    } else if v > hi {
        Side::Above
    } else {
        Side::Inside
    }
}

fn read(state: &Value, dim: &str) -> Option<f64> {
    let mut cur = state;
    for seg in dim.split('.') {
        cur = cur.get(seg)?;
    }
    cur.as_f64()
}

/// ★ Dimensions that crossed `V` to the far side.
///
/// Reads the region's **declared** intervals — no band is invented, and a
/// dimension absent from the state contributes nothing rather than a guess.
pub fn overshoot(region: &Region, before: &Value, after: &Value) -> Vec<Overshoot> {
    let mut out = Vec::new();
    for i in &region.intervals {
        let (Some(b), Some(a)) = (read(before, &i.dim), read(after, &i.dim)) else {
            continue;
        };
        let (sb, sa) = (side_of(i.lo, i.hi, b), side_of(i.lo, i.hi, a));
        if sb.crossed_to(sa) {
            out.push(Overshoot {
                dimension: i.dim.clone(),
                from: sb,
                to: sa,
                before: b,
                after: a,
            });
        }
    }
    out
}

// ── the trajectory reading, where no gain is declared ──────────────────────

/// What a `W` trajectory says about damping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DampingVerdict {
    /// `W` moved toward the region without reversing — it settles.
    Damped { steps: usize, settled: bool },
    /// ★ Alternating-sign steps: the loop over-corrects and comes back.
    Ringing { alternations: usize, steps: usize },
    /// ★★ Not enough trajectory to say. **Not damped and not ringing** — the
    /// same third answer `Skew::Unknown` and `CycleVerdict::Indeterminate`
    /// carry, and for the same reason: two points cannot show a reversal.
    Indeterminate { reason: String },
}

impl DampingVerdict {
    pub fn rings(&self) -> bool {
        matches!(self, DampingVerdict::Ringing { .. })
    }

    pub fn is_indeterminate(&self) -> bool {
        matches!(self, DampingVerdict::Indeterminate { .. })
    }
}

/// Read a `W` trajectory for ringing.
///
/// ★ A damped correction walks `W` down. A ringing one over-corrects and comes
/// back, so the **sign of `ΔW` alternates** — which is visible in the magnitude
/// series without needing a model of what produced it. Three points is the
/// minimum that can show a reversal at all, and fewer is
/// [`DampingVerdict::Indeterminate`] rather than a guess.
pub fn read_trajectory(w_series: &[f64]) -> DampingVerdict {
    if w_series.len() < 3 {
        return DampingVerdict::Indeterminate {
            reason: format!(
                "{} point(s) cannot show a reversal — a trajectory needs at least 3 to have a \
                 direction that could change",
                w_series.len()
            ),
        };
    }

    let deltas: Vec<f64> = w_series.windows(2).map(|p| p[1] - p[0]).collect();
    let mut alternations = 0usize;
    for pair in deltas.windows(2) {
        if pair[0] * pair[1] < 0.0 {
            alternations += 1;
        }
    }

    if alternations > 0 {
        DampingVerdict::Ringing { alternations, steps: deltas.len() }
    } else {
        DampingVerdict::Damped {
            steps: deltas.len(),
            settled: w_series.last().is_some_and(|w| *w == 0.0),
        }
    }
}

/// ★ Repeated boundary crossings on one dimension — the other face of ringing,
/// read from the sides rather than from the magnitudes.
pub fn crossings(sides: &[Side]) -> usize {
    sides.windows(2).filter(|p| p[0].crossed_to(p[1])).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::is_stable_intervention;
    use crate::region::{Interval, Region};
    use serde_json::json;

    /// A household band: the pocket should hold between 100 and 200.
    fn band() -> Region {
        Region::new().bounding(Interval::new("finances.liquid", 100.0, 200.0))
    }

    fn at(v: f64) -> Value {
        json!({ "finances": { "liquid": v } })
    }

    // ── ★★ the gap CTL-2 cannot see ────────────────────────────────────────

    #[test]
    fn a_ringing_loop_passes_lyapunov_at_every_single_step() {
        // ★★ THE HEADLINE. Gain 1.5 → pole −0.5: inside the unit circle, so it
        // converges — and the sign flips every step. The REAL
        // `is_stable_intervention` is run over the whole trajectory and returns
        // stable each time, while the loop rings all the way down.
        let law = ProportionalLaw::declared(1.5).unwrap();
        let region = band();
        let centre = 150.0;

        let mut error = -100.0; // 100 below the band's centre: at 50
        let mut w_series = vec![region.distance(&at(centre + error)).unwrap().weighted];
        for _ in 0..6 {
            let before = at(centre + error);
            error = law.step(error);
            let after = at(centre + error);

            let s = is_stable_intervention(&region, &before, &after).unwrap();
            assert!(s.stable, "Lyapunov passes at every step of a ringing loop");
            w_series.push(region.distance(&after).unwrap().weighted);
        }

        // And only the damping analysis separates settle from ring.
        assert!(law.classify().rings());
        assert!(law.classify().settles(), "it does converge — that is the point");
        assert!(law.classify().advice().unwrap().contains("flipping side every step"));
    }

    #[test]
    fn a_sustained_oscillation_passes_lyapunov_forever_with_equality() {
        // ★★ Gain 2 → pole exactly −1. `W` never changes, so `W_after ≤
        // W_before` holds at every step, for ever, while nothing settles.
        let law = ProportionalLaw::declared(2.0).unwrap();
        let region = band();
        let mut error = -80.0;

        for _ in 0..8 {
            let before = at(150.0 + error);
            error = law.step(error);
            let after = at(150.0 + error);
            let s = is_stable_intervention(&region, &before, &after).unwrap();
            assert!(s.stable, "equality satisfies `≤`");
            assert_eq!(s.before, s.after, "and the distance never moves");
        }

        match law.classify() {
            PoleVerdict::Undamped { pole, sustained } => {
                assert_eq!(pole, -1.0);
                assert!(sustained);
            }
            other => panic!("{other:?}"),
        }
        assert!(!law.classify().settles());
        assert!(law.classify().advice().unwrap().contains("holds forever with equality"));
    }

    #[test]
    fn a_damped_gain_settles_from_the_same_side() {
        let law = ProportionalLaw::declared(0.5).unwrap();
        assert_eq!(law.pole(), 0.5);
        match law.classify() {
            PoleVerdict::Damped { pole } => assert_eq!(pole, 0.5),
            other => panic!("{other:?}"),
        }
        assert!(law.classify().settles() && !law.classify().rings());
        assert!(law.classify().advice().is_none(), "nothing to advise about a damped loop");
    }

    #[test]
    fn a_unit_gain_is_deadbeat() {
        let law = ProportionalLaw::declared(1.0).unwrap();
        assert_eq!(law.pole(), 0.0);
        assert_eq!(law.classify(), PoleVerdict::Deadbeat);
        assert_eq!(law.step(-100.0), 0.0, "one step to zero");
    }

    #[test]
    fn a_gain_past_two_diverges() {
        let law = ProportionalLaw::declared(3.0).unwrap();
        assert_eq!(law.pole(), -2.0);
        match law.classify() {
            PoleVerdict::Undamped { sustained, .. } => assert!(!sustained),
            other => panic!("{other:?}"),
        }
        assert!(law.classify().advice().unwrap().contains("corrections grow"));
    }

    #[test]
    fn a_gain_must_be_declared_and_cannot_be_fitted() {
        // ★★ Structural: there is no `fit_gain(trajectory)` in this module —
        // a law is declared or it does not exist, which is the fabricated-`A`
        // trap MON-1 declined the observability matrix to avoid. This test
        // records the constructor's own validation; the absence is the point.
        assert!(matches!(
            ProportionalLaw::declared(f64::NAN),
            Err(DampingError::BadGain(_))
        ));
        assert!(ProportionalLaw::declared(0.7).is_ok());
    }

    // ── ★ overshoot: the concrete discrete failure ─────────────────────────

    #[test]
    fn an_overshoot_reduces_w_and_is_still_the_failure() {
        // ★ 50 → 230 on a [100,200] band: W falls 50 → 30, so Lyapunov PASSES,
        // and the state is now on the far side needing a correction back.
        let region = band();
        let (before, after) = (at(50.0), at(230.0));

        let s = is_stable_intervention(&region, &before, &after).unwrap();
        assert!(s.stable, "W fell — CTL-2 is satisfied");
        assert!(s.after < s.before);

        let over = overshoot(&region, &before, &after);
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].from, Side::Below);
        assert_eq!(over[0].to, Side::Above);
        assert!(over[0].describe().contains("needs a correction back"));
    }

    #[test]
    fn a_damped_correction_lands_inside_and_is_no_overshoot() {
        let region = band();
        let (before, after) = (at(50.0), at(120.0));
        assert!(is_stable_intervention(&region, &before, &after).unwrap().stable);
        assert!(overshoot(&region, &before, &after).is_empty(), "it arrived, it did not pass");
    }

    #[test]
    fn approaching_from_the_same_side_is_not_an_overshoot_either() {
        let region = band();
        assert!(overshoot(&region, &at(20.0), &at(80.0)).is_empty());
    }

    #[test]
    fn passing_through_the_region_to_the_far_side_is_a_crossing() {
        assert!(Side::Below.crossed_to(Side::Above));
        assert!(Side::Above.crossed_to(Side::Below));
        assert!(!Side::Below.crossed_to(Side::Inside), "arriving is not overshooting");
        assert!(!Side::Inside.crossed_to(Side::Above));
    }

    #[test]
    fn a_dimension_absent_from_the_state_contributes_nothing_rather_than_a_guess() {
        let region = band();
        assert!(overshoot(&region, &json!({}), &json!({})).is_empty());
    }

    // ── ★ the trajectory reading, where no gain is declared ────────────────

    #[test]
    fn a_monotonically_falling_trajectory_reads_damped() {
        match read_trajectory(&[100.0, 60.0, 30.0, 10.0, 0.0]) {
            DampingVerdict::Damped { steps, settled } => {
                assert_eq!(steps, 4);
                assert!(settled, "it reached the region");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_alternating_trajectory_reads_ringing() {
        // ★ Down, up, down, up — the over-corrector coming back each time.
        match read_trajectory(&[100.0, 20.0, 70.0, 15.0, 50.0]) {
            DampingVerdict::Ringing { alternations, steps } => {
                assert_eq!(steps, 4);
                assert_eq!(alternations, 3);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fewer_than_three_points_is_indeterminate_not_damped() {
        // ★★ Two points cannot show a reversal, so calling them damped would
        // be a claim the data cannot support.
        let v = read_trajectory(&[100.0, 40.0]);
        assert!(v.is_indeterminate());
        assert!(!v.rings());
        match v {
            DampingVerdict::Indeterminate { reason } => assert!(reason.contains("at least 3")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn repeated_boundary_crossings_are_the_other_face_of_ringing() {
        // ★ Read from the sides rather than the magnitudes — the same failure
        // seen the other way.
        let sides = [Side::Below, Side::Above, Side::Below, Side::Above];
        assert_eq!(crossings(&sides), 3);
        assert_eq!(crossings(&[Side::Below, Side::Inside, Side::Inside]), 0);
    }

    #[test]
    fn the_magnitude_series_cannot_see_a_sign_flip_and_the_sides_can() {
        // ★★ A finding worth stating rather than smoothing over: a declared gain
        // of 1.5 rings, and the W-MAGNITUDE trajectory it produces reads
        // `Damped` — because |W| genuinely falls every step. The two readings
        // are not redundant and are not in conflict; they see different things,
        // which is why the side series exists beside the magnitude one.
        let law = ProportionalLaw::declared(1.5).unwrap();
        let mut error: f64 = -100.0;
        let mut w = vec![error.abs()];
        for _ in 0..5 {
            error = law.step(error);
            w.push(error.abs());
        }

        // ★ The MAGNITUDES fall monotonically — so the W trajectory alone reads
        // `Damped`, and that is not a bug in either reading: a magnitude series
        // genuinely cannot see a sign flip. It is exactly why both readings
        // exist, and why the side series is the one that carries the ringing.
        assert!(!read_trajectory(&w).rings(), "|W| falls every step");
        assert!(law.classify().rings(), "and the declared pole says it rings anyway");
        let sides: Vec<Side> = {
            let mut e: f64 = -100.0;
            let mut out = vec![Side::Below];
            for _ in 0..5 {
                e = law.step(e);
                out.push(if e < 0.0 { Side::Below } else { Side::Above });
            }
            out
        };
        assert!(crossings(&sides) > 0, "the crossings show what the magnitudes hide");
    }
}
