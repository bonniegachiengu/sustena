//! **The domain × engine matrix** (Capstone · §VII).
//!
//! ```text
//!              clear        complicated    complex        chaotic
//!   Observe    ✓            ✓              ✓              ✓
//!   Model      ✓            ✓              bounded        REFUSE
//!   Learn      ✓            ✓              ✓              REFUSE
//!   Respond    ✓            ✓              probe          act first
//! ```
//!
//! ★★★ **The cells that say *do not* are the reason this is a matrix and not a
//! sentence.** Three of the four engines run in all four regimes with only a
//! change of manner; `Model` and `Learn` have a regime where the honest output
//! is a refusal. Modelling a chaotic system produces a confident wrong answer,
//! which is strictly worse than no answer — a person acts on it. Learning from
//! one fits noise and then carries the noise forward into every later decision.
//!
//! ★★★ **Refusing is not degrading.** A "reduced-confidence model" in a chaotic
//! regime is the same wrong number with a smaller font. [`Behaviour::Refuse`]
//! has no value in it to read.
//!
//! ## Disorder arrives as confidence over the estimate
//!
//! ★★★ **A 51%-confident "complicated" routed as complicated is an expert
//! method applied to a coin flip.** §VII asks for disorder as a first-class
//! reading surfaced *as confidence over the domain estimate*, and this is that:
//! an [`Estimate`] carries its confidence, a reading below the floor is disorder
//! however definite its label looks, and the matrix cannot be indexed by it.
//!
//! ★★ **Confidence is required, never defaulted.** A classifier that returned a
//! domain without one would be handing over a guess dressed as a reading, and
//! the whole point of the fifth state is that not knowing is an answer.

use crate::cynefin::{Disorder, DomainReading, ResponseMode};
use crate::operative::Cynefin;

/// The four engine functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Engine {
    /// Read what is happening.
    Observe,
    /// Build a transition model and simulate forward.
    Model,
    /// Fit parameters from history.
    Learn,
    /// Act.
    Respond,
}

impl Engine {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Model => "model",
            Self::Learn => "learn",
            Self::Respond => "respond",
        }
    }
}

/// What one engine does in one regime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Behaviour {
    /// Run it, in this manner.
    Run { how: &'static str },
    /// Run it, but the answer is only good this far.
    ///
    /// ★★ A separate variant from `Run` because "it works, with a limit" and "it
    /// works" are different promises, and collapsing them loses the limit at the
    /// first call site that forgets to read the prose.
    Bounded { how: &'static str, limit: &'static str },
    /// ★★★ **Do not.** No value to read, so a caller cannot use it anyway.
    Refuse { because: &'static str },
}

impl Behaviour {
    pub fn runs(&self) -> bool {
        !matches!(self, Self::Refuse { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Run { how } => how.to_string(),
            Self::Bounded { how, limit } => format!("{how} — but only {limit}"),
            Self::Refuse { because } => format!("do not: {because}"),
        }
    }
}

/// **One cell of the matrix.**
///
/// ★★ Declared as code rather than data because every cell is a claim somebody
/// has to be able to argue with in review, and a table in a config file gets
/// edited by whoever is annoyed by a refusal.
pub fn cell(engine: Engine, domain: Cynefin) -> Behaviour {
    use Cynefin::*;
    use Engine::*;
    match (engine, domain) {
        // Observing always works. It is the one engine that makes no
        // assumption about the regime — it only reports what the log holds.
        (Observe, Clear) => Behaviour::Run { how: "read the state against known categories" },
        (Observe, Complicated) => Behaviour::Run { how: "read the state and the causes behind it" },
        (Observe, Complex) => {
            Behaviour::Run { how: "read the state and watch for regime change" }
        }
        (Observe, Chaotic) => Behaviour::Run {
            how: "read the state — observing is the only thing that still works here",
        },

        (Model, Clear) => Behaviour::Run { how: "a lookup: the answer is already known" },
        (Model, Complicated) => Behaviour::Run { how: "analyse and optimise over the model" },
        (Model, Complex) => Behaviour::Bounded {
            how: "simulate forward",
            limit: "as far as the horizon H* says the output means anything",
        },
        (Model, Chaotic) => Behaviour::Refuse {
            because: "a model of a chaotic regime produces a confident wrong answer, and a \
                      person acts on it — worse than having none",
        },

        (Learn, Clear) => Behaviour::Run { how: "record the categories that keep applying" },
        (Learn, Complicated) => Behaviour::Run { how: "fit the parameters the experts left open" },
        (Learn, Complex) => Behaviour::Run {
            how: "learn from what the probes actually did, not from what was predicted",
        },
        (Learn, Chaotic) => Behaviour::Refuse {
            because: "there is nothing stable to learn from, and what gets fitted here is noise \
                      that then travels into every later decision",
        },

        (Respond, Clear) => Behaviour::Run { how: "apply the practice that fits" },
        (Respond, Complicated) => Behaviour::Run { how: "act on the analysis" },
        (Respond, Complex) => {
            Behaviour::Run { how: "small safe-to-fail probes, and amplify what worked" }
        }
        (Respond, Chaotic) => {
            Behaviour::Run { how: "establish order first and understand it afterwards" }
        }
    }
}

/// The floor below which a domain label is not a reading.
///
/// ★★ Two-thirds, and it is a declared number rather than a majority: a bare
/// majority over four options is barely above chance, and routing an expert
/// method on it is exactly the failure the fifth state exists to name.
pub const CONFIDENCE_FLOOR: f64 = 0.67;

/// A domain reading with the confidence it was produced at.
///
/// ★★★ There is no constructor that omits the confidence. A classifier handing
/// over a domain without one would be passing a guess dressed as a reading.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    reading: DomainReading,
    confidence: f64,
}

impl Estimate {
    pub fn new(reading: DomainReading, confidence: f64) -> Self {
        Self { reading, confidence }
    }

    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    pub fn reading(&self) -> &DomainReading {
        &self.reading
    }

    /// **Is this a reading, or is it disorder?**
    ///
    /// ★★★ A confident-looking label below the floor is disorder. The label is
    /// not evidence of itself.
    pub fn settled(&self) -> Result<Cynefin, Disorder> {
        match &self.reading {
            DomainReading::Indeterminate { why } => {
                Err(Disorder::IndeterminateReading { why: why.clone() })
            }
            DomainReading::Known(d) if self.confidence >= CONFIDENCE_FLOOR => Ok(*d),
            DomainReading::Known(d) => Err(Disorder::IndeterminateReading {
                why: format!(
                    "the reading says {} but only at {:.0}% confidence, under the {:.0}% floor",
                    d.name(),
                    self.confidence * 100.0,
                    CONFIDENCE_FLOOR * 100.0
                ),
            }),
        }
    }
}

/// What an engine should do, given an estimate rather than a certainty.
#[derive(Debug, Clone, PartialEq)]
pub enum Prescription {
    /// The regime is settled and the matrix has a cell for it.
    Do { domain: Cynefin, behaviour: Behaviour, mode: ResponseMode },
    /// ★★★ **Surface it.** Not a default cell, not the least-bad regime — the
    /// matrix cannot be indexed by a reading nobody trusts, and picking a row
    /// anyway is how an unrecognised situation gets handled as a familiar one.
    Surface { disorder: Disorder },
}

impl Prescription {
    pub fn describe(&self, engine: Engine) -> String {
        match self {
            Self::Do { domain, behaviour, mode } => format!(
                "{} in a {} regime ({}): {}",
                engine.name(),
                domain.name(),
                mode.name(),
                behaviour.describe()
            ),
            Self::Surface { disorder } => disorder.describe(),
        }
    }
}

/// **Index the matrix with an estimate.**
pub fn prescribe(estimate: &Estimate, engine: Engine) -> Prescription {
    match estimate.settled() {
        Ok(domain) => Prescription::Do {
            domain,
            behaviour: cell(engine, domain),
            mode: domain.response_mode(),
        },
        Err(disorder) => Prescription::Surface { disorder },
    }
}

/// Every engine that must stop in this regime.
///
/// ★★ Askable as a set, because "what can we still do here" is the question
/// somebody has in a bad week, and answering it one engine at a time invites
/// the one they forgot to ask about.
pub fn refused_in(domain: Cynefin) -> Vec<Engine> {
    [Engine::Observe, Engine::Model, Engine::Learn, Engine::Respond]
        .into_iter()
        .filter(|e| !cell(*e, domain).runs())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn certain(d: Cynefin) -> Estimate {
        Estimate::new(DomainReading::known(d), 0.95)
    }

    #[test]
    fn modelling_a_chaotic_regime_is_refused_rather_than_degraded() {
        // ★★★ A "reduced-confidence model" here is the same wrong number in a
        //     smaller font, and a person acts on it either way.
        let b = cell(Engine::Model, Cynefin::Chaotic);
        assert!(!b.runs());
        assert!(b.describe().contains("confident wrong answer"));
    }

    #[test]
    fn learning_in_chaos_is_refused_because_what_gets_fitted_travels() {
        // ★★★ Noise fitted here does not stay here — it goes into every later
        //     decision that reads the learned parameter.
        let b = cell(Engine::Learn, Cynefin::Chaotic);
        assert!(!b.runs());
        assert!(b.describe().contains("travels into every later decision"));
    }

    #[test]
    fn observing_is_the_one_engine_that_survives_every_regime() {
        // ★★ It makes no assumption about the regime; it reports what the log
        //    holds. That is why it is the thing left when the rest stops.
        for d in [Cynefin::Clear, Cynefin::Complicated, Cynefin::Complex, Cynefin::Chaotic] {
            assert!(cell(Engine::Observe, d).runs(), "{d:?}");
        }
    }

    #[test]
    fn what_still_works_in_a_bad_week_is_askable_as_a_set() {
        // ★★ Answering one engine at a time invites the one nobody asked about.
        assert_eq!(refused_in(Cynefin::Chaotic), vec![Engine::Model, Engine::Learn]);
        assert!(refused_in(Cynefin::Clear).is_empty());
        assert!(refused_in(Cynefin::Complex).is_empty());
    }

    #[test]
    fn modelling_a_complex_regime_runs_and_carries_its_limit() {
        // ★★ "It works, with a limit" and "it works" are different promises,
        //    and one variant for both loses the limit at the first call site
        //    that does not read the prose.
        match cell(Engine::Model, Cynefin::Complex) {
            Behaviour::Bounded { limit, .. } => assert!(limit.contains("H*")),
            other => panic!("the horizon must ride along: {other:?}"),
        }
    }

    #[test]
    fn a_barely_majority_reading_is_disorder_however_definite_its_label_looks() {
        // ★★★ A 51%-confident "complicated" routed as complicated is an expert
        //     method applied to a coin flip. The label is not evidence of itself.
        let shaky = Estimate::new(DomainReading::known(Cynefin::Complicated), 0.51);
        let d = shaky.settled().expect_err("must not settle");
        assert!(d.describe().contains("51% confidence"));
        assert!(matches!(prescribe(&shaky, Engine::Respond), Prescription::Surface { .. }));
    }

    #[test]
    fn a_confident_reading_settles_and_carries_its_response_mode() {
        match prescribe(&certain(Cynefin::Complex), Engine::Respond) {
            Prescription::Do { mode, behaviour, .. } => {
                assert_eq!(mode, ResponseMode::ProbeSenseRespond);
                assert!(behaviour.describe().contains("safe-to-fail"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_indeterminate_reading_is_surfaced_and_never_given_a_default_row() {
        // ★★★ Picking a row anyway is how an unrecognised situation gets
        //     handled as a familiar one — which is the failure, not the fix.
        let nothing = Estimate::new(DomainReading::indeterminate("the signals disagree"), 0.99);
        match prescribe(&nothing, Engine::Model) {
            Prescription::Surface { disorder } => {
                assert!(disorder.describe().contains("signals disagree"));
            }
            other => panic!("must not be given a regime: {other:?}"),
        }
    }

    #[test]
    fn high_confidence_in_not_knowing_is_still_not_knowing() {
        // ★★ The confidence is about the estimate, not about the certainty of
        //    being lost. An indeterminate reading at 99% is 99% sure it cannot
        //    tell — which is not a regime.
        let sure_of_nothing =
            Estimate::new(DomainReading::indeterminate("no stable signal"), 1.0);
        assert!(sure_of_nothing.settled().is_err());
    }

    #[test]
    fn the_refusal_is_carried_all_the_way_to_the_prescription() {
        // ★★ A cell that refuses must not be reachable as a `Do` that a caller
        //    reads the value out of — there is no value in it.
        match prescribe(&certain(Cynefin::Chaotic), Engine::Model) {
            Prescription::Do { behaviour, .. } => {
                assert!(!behaviour.runs());
                assert!(behaviour.describe().starts_with("do not"));
            }
            other => panic!("a settled regime still gets its cell: {other:?}"),
        }
    }

    #[test]
    fn every_pair_has_a_declared_cell() {
        // ★★ Sixteen claims, and a missing one would be an unhandled regime
        //    somebody discovers in the worst week.
        for e in [Engine::Observe, Engine::Model, Engine::Learn, Engine::Respond] {
            for d in [Cynefin::Clear, Cynefin::Complicated, Cynefin::Complex, Cynefin::Chaotic] {
                assert!(!cell(e, d).describe().is_empty(), "{e:?} × {d:?}");
            }
        }
    }
}
