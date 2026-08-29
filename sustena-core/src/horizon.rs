//! **How far ahead a simulation still means anything** (Operative · §VIII).
//!
//! ```text
//!   H* ≈ (1/λ) · ln(ε / |δ₀|)
//! ```
//!
//! ★★★ **Past `H*` a simulator is spending compute on noise, and reporting it
//! as a result.** Two households that differ by one shilling diverge
//! exponentially under a chaotic transition model; after enough steps the
//! trajectory says nothing about *this* household, only about the arithmetic.
//! A simulator that returns a number without saying how far ahead the number is
//! meaningful has told the reader something false by omission.
//!
//! ★★★ **So the horizon is not an optional annotation on the answer — it is
//! part of the answer.** [`Horizon`] is what the estimate returns, and there is
//! no variant that says "as far as you like": the closest is `Unbounded`, which
//! says *this mechanism does not limit you* and explicitly not *nothing does*.
//!
//! ## The sting is in the logarithm
//!
//! ★★★ **`H*` grows with the LOG of precision, which is the practical content
//! of the whole row.** Halving the initial uncertainty buys `ln2/λ` more steps —
//! about seven-tenths of one Lyapunov time, whatever you started from. The
//! instinct on being told a forecast is short is to measure more carefully, and
//! against a chaotic system that instinct is almost worthless. [`steps_bought_by`]
//! exists so the answer to "what if we were ten times more precise" is a number
//! somebody can look at before funding it.
//!
//! ## λ is measured or it is unknown
//!
//! ★★★ **Never assumed.** An assumed exponent produces a horizon that looks
//! like a measurement, and a horizon that is wrong in the generous direction is
//! worse than no horizon at all — it licenses exactly the compute it should
//! refuse. Below a floor of observations [`lyapunov`] returns `None`, and an
//! unknown λ gives `Horizon::Unknown`, which is a third answer and not a large
//! one.

/// The fewest separation observations an exponent may be estimated from.
///
/// ★★ Five ratios is six measurements. Below that the "average growth rate" is
/// an average of two or three numbers, and it will produce a confident horizon
/// out of a coincidence.
pub const MIN_OBSERVATIONS: usize = 5;

/// Separations below this are treated as zero.
///
/// ★★ Two trajectories that have not separated at all give `ln(0)`, and a
/// floor is the honest way to say *we cannot see a difference yet* rather than
/// letting an infinity propagate into a horizon.
pub const RESOLUTION: f64 = 1e-12;

/// How far ahead a simulation is worth reading.
#[derive(Debug, Clone, PartialEq)]
pub enum Horizon {
    /// Meaningful for this many steps, and noise after.
    Steps { steps: usize, lambda: f64 },
    /// ★★★ **This mechanism does not limit you** — λ ≤ 0, so nearby
    /// trajectories converge rather than separate.
    ///
    /// Deliberately not called `Infinite`. Chaos is one reason a forecast stops
    /// meaning anything and there are others — an unmodelled event, a rule
    /// changing, somebody deciding differently. This variant is silent about
    /// those, and a name promising forever would not be.
    Unbounded { lambda: f64 },
    /// ★★★ Not enough was observed to estimate λ. **Not the same as unbounded**,
    /// and a caller that treats it as one has invented a licence.
    Unknown { why: String },
}

impl Horizon {
    /// Is a simulation this many steps out still worth reading?
    ///
    /// ★★★ `Unknown` answers **false**. An unmeasured horizon is not a long
    /// one, and defaulting the other way is exactly how compute gets spent on
    /// noise while wearing a guarantee.
    pub fn covers(&self, step: usize) -> bool {
        match self {
            Self::Steps { steps, .. } => step <= *steps,
            Self::Unbounded { .. } => true,
            Self::Unknown { .. } => false,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Steps { steps, lambda } => format!(
                "meaningful for about {steps} steps (λ≈{lambda:.3}); past that it is noise"
            ),
            Self::Unbounded { lambda } => format!(
                "nearby paths converge (λ≈{lambda:.3}), so chaos does not bound this — other \
                 things still might"
            ),
            Self::Unknown { why } => format!("no horizon could be estimated: {why}"),
        }
    }
}

/// **Estimate λ from how two nearby trajectories separated.**
///
/// The mean log growth rate of the separation, which is the definition read
/// straight off a measured series rather than fitted to one.
///
/// ★★ Ratios, not endpoints: `ln(δₙ/δ₀)/n` would give the same number for a
/// series that grew smoothly and one that jumped once and then sat still, and
/// those are different systems.
pub fn lyapunov(separations: &[f64]) -> Option<f64> {
    if separations.len() < MIN_OBSERVATIONS + 1 {
        return None;
    }
    let mut rates = Vec::new();
    for pair in separations.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a <= RESOLUTION || b <= RESOLUTION {
            // ★★ Skipped rather than clamped. A pair we could not resolve is
            //    an observation we do not have, and substituting the floor
            //    would invent a growth rate from the floor's own value.
            continue;
        }
        rates.push((b / a).ln());
    }
    if rates.len() < MIN_OBSERVATIONS {
        return None;
    }
    Some(rates.iter().sum::<f64>() / rates.len() as f64)
}

/// **`H* ≈ (1/λ) · ln(ε/|δ₀|)`**
///
/// `delta0` is what we do not know about the starting point; `epsilon` is how
/// much error still counts as an answer.
pub fn horizon(lambda: Option<f64>, delta0: f64, epsilon: f64) -> Horizon {
    let Some(lambda) = lambda else {
        return Horizon::Unknown {
            why: format!("fewer than {MIN_OBSERVATIONS} usable separations were observed"),
        };
    };
    if !(delta0 > 0.0) || !(epsilon > 0.0) {
        return Horizon::Unknown {
            why: "the starting uncertainty and the tolerance must both be positive and known"
                .into(),
        };
    }
    if lambda <= 0.0 {
        return Horizon::Unbounded { lambda };
    }
    if epsilon <= delta0 {
        // ★★★ Already outside tolerance before a single step. Reported as zero
        //     rather than as a negative horizon, and it is a real answer: the
        //     honest output is "you cannot simulate this usefully at all",
        //     which a caller can act on.
        return Horizon::Steps { steps: 0, lambda };
    }
    let steps = ((epsilon / delta0).ln() / lambda).floor();
    Horizon::Steps { steps: steps.max(0.0) as usize, lambda }
}

/// **How many more steps a `factor`× improvement in precision would buy.**
///
/// ★★★ `ln(factor)/λ`, and it does not depend on where you started — which is
/// the row's practical sting made askable. Ten times more precise buys
/// `ln10/λ`, about two and a third Lyapunov times, whether the current horizon
/// is five steps or five hundred.
pub fn steps_bought_by(factor: f64, lambda: f64) -> Option<usize> {
    if factor <= 1.0 || lambda <= 0.0 {
        return None;
    }
    Some((factor.ln() / lambda).floor().max(0.0) as usize)
}

/// A simulated reading, and how far it is worth trusting.
///
/// ★★★ The pairing is the point of the row: a value and its horizon travel
/// together, so a caller cannot hold one without the other.
#[derive(Debug, Clone, PartialEq)]
pub struct Bounded<T> {
    pub value: T,
    pub step: usize,
    pub horizon: Horizon,
}

impl<T> Bounded<T> {
    pub fn new(value: T, step: usize, horizon: Horizon) -> Self {
        Self { value, step, horizon }
    }

    /// Is this reading inside the horizon it was produced under?
    pub fn meaningful(&self) -> bool {
        self.horizon.covers(self.step)
    }

    /// The value, only if it means something.
    ///
    /// ★★ Returning `Option` rather than the value plus a flag: a flag beside a
    /// number is one a caller can forget to read, and this one cannot be.
    pub fn read(&self) -> Option<&T> {
        self.meaningful().then_some(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A separation that doubles every step: λ = ln 2 ≈ 0.693.
    fn doubling() -> Vec<f64> {
        (0..10).map(|i| 0.01 * 2_f64.powi(i)).collect()
    }

    /// A separation that halves every step: λ = −ln 2.
    fn shrinking() -> Vec<f64> {
        (0..10).map(|i| 1.0 * 0.5_f64.powi(i)).collect()
    }

    #[test]
    fn the_exponent_is_measured_from_how_far_things_actually_drifted() {
        let l = lyapunov(&doubling()).expect("enough observations");
        assert!((l - 2_f64.ln()).abs() < 1e-9, "λ was {l}");
    }

    #[test]
    fn too_little_drift_to_look_at_gives_no_exponent_rather_than_a_guess() {
        // ★★★ An assumed exponent produces a horizon that looks like a
        //     measurement, and one wrong in the generous direction licenses
        //     exactly the compute it should refuse.
        assert_eq!(lyapunov(&[0.01, 0.02, 0.04]), None);
    }

    #[test]
    fn an_unmeasured_horizon_is_not_a_long_one() {
        // ★★★ `Unknown` refuses. Defaulting the other way is how compute gets
        //     spent on noise while wearing a guarantee.
        let h = horizon(lyapunov(&[0.01, 0.02]), 0.01, 1.0);
        assert!(matches!(h, Horizon::Unknown { .. }));
        assert!(!h.covers(1), "not even one step");
        assert!(h.describe().contains("no horizon"));
    }

    #[test]
    fn a_chaotic_model_gets_a_finite_horizon_and_says_so() {
        let h = horizon(lyapunov(&doubling()), 0.01, 1.0);
        match h {
            Horizon::Steps { steps, .. } => {
                // ln(100)/ln(2) ≈ 6.64 → 6 whole steps.
                assert_eq!(steps, 6);
            }
            other => panic!("expected a bounded horizon: {other:?}"),
        }
    }

    #[test]
    fn converging_paths_are_unbounded_by_chaos_and_not_by_everything() {
        // ★★★ Not called `Infinite`. An unmodelled event, a rule changing or
        //     somebody deciding differently all still end a forecast, and a
        //     name promising forever would be silent about them.
        let h = horizon(lyapunov(&shrinking()), 0.01, 1.0);
        assert!(matches!(h, Horizon::Unbounded { .. }));
        assert!(h.describe().contains("other \nthings still might") || h.describe().contains("other things still might"));
    }

    #[test]
    fn precision_buys_almost_nothing_and_the_number_says_how_little() {
        // ★★★ The practical content of the row. Ten times more precise buys
        //     ln10/λ steps — about three, here — whether you started at six
        //     steps or six hundred.
        let l = 2_f64.ln();
        assert_eq!(steps_bought_by(10.0, l), Some(3));
        assert_eq!(steps_bought_by(2.0, l), Some(1));
        // And it does not depend on where you started.
        let tight = horizon(Some(l), 0.001, 1.0);
        let loose = horizon(Some(l), 0.01, 1.0);
        match (tight, loose) {
            (Horizon::Steps { steps: a, .. }, Horizon::Steps { steps: b, .. }) => {
                assert_eq!(a - b, 3, "ten times tighter bought three steps");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn no_improvement_in_precision_helps_a_non_chaotic_system_more_than_it_needs() {
        assert_eq!(steps_bought_by(10.0, -0.5), None);
        assert_eq!(steps_bought_by(1.0, 0.7), None);
    }

    #[test]
    fn a_starting_uncertainty_already_past_tolerance_gives_zero_not_a_negative() {
        // ★★★ A real answer: you cannot simulate this usefully at all.
        let h = horizon(Some(0.7), 2.0, 1.0);
        assert_eq!(h, Horizon::Steps { steps: 0, lambda: 0.7 });
        assert!(h.covers(0));
        assert!(!h.covers(1));
    }

    #[test]
    fn an_unknown_starting_uncertainty_gives_no_horizon() {
        assert!(matches!(horizon(Some(0.7), 0.0, 1.0), Horizon::Unknown { .. }));
        assert!(matches!(horizon(Some(0.7), 0.01, 0.0), Horizon::Unknown { .. }));
    }

    #[test]
    fn a_reading_and_its_horizon_cannot_be_held_apart() {
        // ★★★ The pairing is the row. A flag beside a number is one a caller
        //     can forget to read; an `Option` is not.
        let h = horizon(lyapunov(&doubling()), 0.01, 1.0);
        let early = Bounded::new(15_000.0, 3, h.clone());
        let late = Bounded::new(15_000.0, 40, h);
        assert_eq!(early.read(), Some(&15_000.0));
        assert_eq!(late.read(), None, "past the horizon there is no value to read");
        assert!(!late.meaningful());
    }

    #[test]
    fn a_pair_we_could_not_resolve_is_skipped_not_clamped() {
        // ★★ Substituting the floor would invent a growth rate from the
        //    floor's own value rather than from the system.
        let mut with_a_hole = doubling();
        with_a_hole.insert(0, 0.0);
        let l = lyapunov(&with_a_hole).expect("still enough real pairs");
        assert!((l - 2_f64.ln()).abs() < 1e-9, "the hole did not move λ: {l}");
    }

    #[test]
    fn a_jump_and_a_steady_climb_are_not_the_same_system() {
        // ★★ Endpoint-only estimation would call these identical. They are not,
        //    and one of them is not chaotic at all.
        let steady = doubling();
        let mut jumped = vec![0.01; 6];
        jumped.push(*steady.last().unwrap());
        jumped.extend([*steady.last().unwrap(); 4]);
        let a = lyapunov(&steady).expect("a");
        let b = lyapunov(&jumped).expect("b");
        assert!(b < a, "a single jump is not sustained divergence: {b} vs {a}");
    }
}
