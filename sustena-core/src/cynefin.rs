//! Suited domains — routing, coverage, and disorder
//! (Operative §IX · OPV-16).
//!
//! > Each operative declares (or has detected for it)
//! > `dom_i ⊆ {clear, complicated, complex, chaotic}` against §4D.6's field,
//! > with Snowden and Boone's response modes.
//!
//! | domain | response mode | an operative built for it |
//! |---|---|---|
//! | **clear** | sense → categorise → respond | a lookup / classification DAG; best practice applies |
//! | **complicated** | sense → **analyse** → respond | an optimisation DAG; expertise finds the answer |
//! | **complex** | **probe** → sense → respond | a portfolio of small safe-to-fail forks; amplify what worked |
//! | **chaotic** | **act** → sense → respond | establish order first; analyse afterwards |
//!
//! ## ★★ This layer routes ON a domain reading. It does not produce one.
//!
//! §IX is explicit that `dom(s)` is **a Monitor output**, with §VIII's
//! criticality detector as *one* of its inputs. The article gives **no
//! algorithm** for classifying a Sustain into the four domains, and inventing a
//! confident one would be precisely the confident-classification-from-nowhere
//! this codebase refuses everywhere else — the same rule that makes `0/0` not
//! `r = 0`, a missing cost not free, and a missing reading not zero.
//!
//! So [`DomainReading`] is **supplied**, exactly as MUL-14's pressure and
//! OPV-28's `dom(s)` are. The classifier is Monitor territory (CAP-13 /
//! §4D.6) — a correctly separate layer, **not an agent-layer gap**.
//!
//! And the supplied reading has an honest shape: the Monitor may not be able to
//! say. [`DomainReading::Indeterminate`] is a first-class answer rather than a
//! missing value — which is where the fifth state comes in.
//!
//! ## ★ Disorder is un-declarable, structurally
//!
//! > Cynefin's fifth state, *disorder*, is a **detection target, not a value to
//! > declare.**
//!
//! [`crate::operative::Cynefin`] has exactly four variants, so an operative
//! **cannot** write `dom_i = {disorder}` — there is no such value to write. It
//! is not forbidden by a check; it is unsayable. What disorder *is* here is a
//! detected condition, arriving two ways ([`Disorder`]): a regime **no
//! operative is suited to**, or a reading the Monitor could not determine.
//! Either way the answer is to **surface it**, never to route anyway.
//!
//! ## ★ Why the abstention exists — the presupposition, not the preference
//!
//! > An operative built for complicated-domain optimisation is **not merely
//! > less useful** in a chaotic regime; **it is confidently wrong, because its
//! > method presupposes analysability.**
//!
//! So a [`Mismatch`] carries the reason, and the reason is *derived from the
//! declared data* rather than being a canned string: each response mode has a
//! [`ResponseMode::presupposes`], and a mismatch names the presupposition that
//! fails against the regime that denies it. A routing layer that reported only
//! `match = 0` would be throwing away the one thing that makes the abstention
//! defensible.
//!
//! ## ★ The coverage condition — Ashby, and one source of truth
//!
//! > The population acquires requisite variety only if `⋃_i dom_i` covers all
//! > four domains. If some domain is in no operative's set, there is a regime
//! > in which no operative is suited, and the correct behaviour is to **surface
//! > the gap** rather than route to the least-bad scorer and present its output
//! > in the usual format.
//!
//! [`coverage`] is the canonical computation, over `&[&Operative]` so it is not
//! tied to any one caller's type. [`crate::mixture::coverage_gaps`] **delegates
//! to it** — the gate folded a version in when it needed one, and this row is
//! its home, so the duplicate was removed rather than left to drift.
//!
//! ## ★ The enumerable-mismatch property, and the half that needs `Π`
//!
//! > An operative whose `Π_i` is an optimisation DAG has no business declaring
//! > `chaotic`, and **writing the declaration down makes the mismatch an
//! > enumerable property rather than a paragraph in a document.**
//!
//! [`DomainCoverage::claimants`] is that enumeration: who claims which domain,
//! listed and checkable. What is **not** buildable yet is the cross-check
//! itself — comparing `dom_i` against the *shape* of `Π_i` needs `Π`, which is
//! OPV-4 and does not exist. So the declarations are enumerable and auditable
//! now; auditing them *against the graph* is named as a slot rather than
//! approximated from an operative's id or its objectives, which would be
//! guessing at a shape nobody declared.
//!
//! ## Honest limits
//!
//! - **`dom(s)` is supplied.** The classifier is CAP-13 / §4D.6, and calling
//!   its absence an agent-layer gap would misplace it.
//! - **The `Π`-shape mismatch check is OPV-4's.** Enumerable now, auditable
//!   against the graph later.
//! - **Criticality informs exactly one transition.** See
//!   [`transition_risk`] — §XIII licenses *complex → chaotic* and nothing else,
//!   so nothing else is inferred.

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::criticality::Regime;
use crate::operative::{Cynefin, Operative};

// ---------------------------------------------------------------------------
// Snowden & Boone's response modes
// ---------------------------------------------------------------------------

/// What acting in a domain actually consists of.
///
/// These are what make a mismatch mean something: a domain is not a label, it
/// is a claim about which method works, and the method carries a
/// presupposition that a different regime may deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseMode {
    /// clear — best practice applies.
    SenseCategoriseRespond,
    /// complicated — expertise finds the answer.
    SenseAnalyseRespond,
    /// complex — small safe-to-fail forks, amplify what worked.
    ProbeSenseRespond,
    /// chaotic — establish order first, analyse afterwards.
    ActSenseRespond,
}

impl ResponseMode {
    pub fn name(&self) -> &'static str {
        match self {
            ResponseMode::SenseCategoriseRespond => "sense → categorise → respond",
            ResponseMode::SenseAnalyseRespond => "sense → analyse → respond",
            ResponseMode::ProbeSenseRespond => "probe → sense → respond",
            ResponseMode::ActSenseRespond => "act → sense → respond",
        }
    }

    /// ★ What the method takes for granted — and therefore what a regime can
    /// deny.
    pub fn presupposes(&self) -> &'static str {
        match self {
            ResponseMode::SenseCategoriseRespond => {
                "that the situation is recognisable — that a known category already fits it"
            }
            ResponseMode::SenseAnalyseRespond => {
                "analysability — that expertise applied to the facts will find the answer"
            }
            ResponseMode::ProbeSenseRespond => {
                "that probing is affordable — that small failures are survivable and informative"
            }
            ResponseMode::ActSenseRespond => {
                "that acting beats analysing — that order must be established before it can be understood"
            }
        }
    }

    /// The shape of operative §IX says is built for this mode.
    pub fn built_as(&self) -> &'static str {
        match self {
            ResponseMode::SenseCategoriseRespond => "a lookup / classification DAG",
            ResponseMode::SenseAnalyseRespond => "an optimisation DAG",
            ResponseMode::ProbeSenseRespond => "a portfolio of small safe-to-fail forks",
            ResponseMode::ActSenseRespond => "one that establishes order first",
        }
    }
}

/// The response-mode semantics of the axis.
///
/// A second inherent impl, deliberately: `Cynefin` is *declared* with `ω`
/// (§I's `dom_i`), and what a domain *means for routing* belongs to the layer
/// that routes on it. Same type, and the two files own different halves of it.
impl Cynefin {
    pub fn response_mode(&self) -> ResponseMode {
        match self {
            Cynefin::Clear => ResponseMode::SenseCategoriseRespond,
            Cynefin::Complicated => ResponseMode::SenseAnalyseRespond,
            Cynefin::Complex => ResponseMode::ProbeSenseRespond,
            Cynefin::Chaotic => ResponseMode::ActSenseRespond,
        }
    }
}

// ---------------------------------------------------------------------------
// dom(s) — supplied, and honestly shaped
// ---------------------------------------------------------------------------

/// `dom(s)` — the Sustain's current domain.
///
/// ★ **Supplied, never computed here.** §IX says this is a Monitor output;
/// the classifier is CAP-13 / §4D.6 and inventing one would be a confident
/// classification from nowhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainReading {
    Known(Cynefin),
    /// ★ The Monitor could not determine a regime — **this is disorder**,
    /// Cynefin's fifth state, arriving as a detected condition rather than a
    /// declared value. A first-class answer, not a missing one.
    Indeterminate { why: String },
}

impl DomainReading {
    pub fn known(domain: Cynefin) -> Self {
        DomainReading::Known(domain)
    }

    pub fn indeterminate(why: &str) -> Self {
        DomainReading::Indeterminate {
            why: why.to_string(),
        }
    }

    pub fn domain(&self) -> Option<Cynefin> {
        match self {
            DomainReading::Known(d) => Some(*d),
            DomainReading::Indeterminate { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// match(i, s), and why the abstention exists
// ---------------------------------------------------------------------------

/// Why an operative is unsuited to the regime — **the presupposition, not the
/// preference**.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub operative: String,
    /// `dom(s)`.
    pub regime: Cynefin,
    /// `dom_i`.
    pub suited: BTreeSet<Cynefin>,
    /// Derived from the declared domains and their response modes — not a
    /// canned string.
    pub why: String,
}

/// `match(i,s) = 1[dom(s) ∈ dom_i]`, with the reason when it is 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suitability {
    Suited,
    /// Boxed: a mismatch carries a derived explanation and is much larger than
    /// the suited case.
    Mismatched(Box<Mismatch>),
}

impl Suitability {
    pub fn is_suited(&self) -> bool {
        matches!(self, Suitability::Suited)
    }

    pub fn mismatch(&self) -> Option<&Mismatch> {
        match self {
            Suitability::Suited => None,
            Suitability::Mismatched(m) => Some(m),
        }
    }
}

/// `match(i,s)` — and when it is 0, why.
///
/// The reason is built from the operative's own declared domains and their
/// response modes, so it says which presupposition fails and what the regime
/// requires instead. §IX's point is that this is not a preference ranking: an
/// optimisation operative in a chaotic regime is **confidently wrong**, and a
/// routing layer reporting only `match = 0` throws away the thing that makes
/// the abstention defensible.
pub fn assess(operative: &Operative, regime: Cynefin) -> Suitability {
    if operative.suited_to(regime) {
        return Suitability::Suited;
    }
    let suited: BTreeSet<Cynefin> = operative.suited().clone();
    let declared = if suited.is_empty() {
        "declares no suited domain at all, so no method of its own".to_string()
    } else {
        let modes: Vec<String> = suited
            .iter()
            .map(|d| format!("{} ({})", d.name(), d.response_mode().name()))
            .collect();
        format!(
            "declares {} — {}, which presupposes {}",
            modes.join(" and "),
            suited
                .iter()
                .map(|d| d.response_mode().built_as())
                .collect::<Vec<_>>()
                .join(" / "),
            suited
                .iter()
                .map(|d| d.response_mode().presupposes())
                .collect::<Vec<_>>()
                .join("; and "),
        )
    };
    let mode = regime.response_mode();
    Suitability::Mismatched(Box::new(Mismatch {
        operative: operative.id().to_string(),
        regime,
        suited,
        why: format!(
            "{} {}. The regime is {}, which requires {} and presupposes {}. \
             Not merely less useful here — confidently wrong, because its method \
             presupposes what this regime denies.",
            operative.id(),
            declared,
            regime.name(),
            mode.name(),
            mode.presupposes(),
        ),
    }))
}

// ---------------------------------------------------------------------------
// The coverage condition — canonical
// ---------------------------------------------------------------------------

/// `⋃_i dom_i` against all four — Ashby's requisite variety.
///
/// ★ The canonical computation, and the **enumeration** §IX's closing point
/// asks for: [`DomainCoverage::claimants`] lists who claims which domain, so a
/// mismatch is a checkable property rather than a paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainCoverage {
    covered: BTreeSet<Cynefin>,
    uncovered: BTreeSet<Cynefin>,
    claimants: BTreeMap<Cynefin, Vec<String>>,
}

impl DomainCoverage {
    /// Requisite variety: every domain in some operative's set.
    pub fn has_requisite_variety(&self) -> bool {
        self.uncovered.is_empty()
    }

    pub fn covered(&self) -> &BTreeSet<Cynefin> {
        &self.covered
    }

    /// Domains **no operative is suited to** — the regimes where the correct
    /// behaviour is to surface a gap.
    pub fn uncovered(&self) -> &BTreeSet<Cynefin> {
        &self.uncovered
    }

    /// ★ Who claims what. The enumerable half of §IX's closing point; auditing
    /// a claim against the *shape* of `Π_i` needs OPV-4.
    pub fn claimants(&self, domain: Cynefin) -> &[String] {
        self.claimants.get(&domain).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Operatives no regime could ever select, because they declare nothing.
    pub fn never_suited(&self, operatives: &[&Operative]) -> Vec<String> {
        operatives
            .iter()
            .filter(|o| o.suited().is_empty())
            .map(|o| o.id().to_string())
            .collect()
    }

    pub fn describe(&self) -> String {
        if self.has_requisite_variety() {
            return "requisite variety: every domain has a suited operative".to_string();
        }
        format!(
            "NO operative is suited to {} — surface the gap rather than route to the least-bad scorer",
            self.uncovered
                .iter()
                .map(|d| d.name())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// The canonical coverage computation.
///
/// Takes `&[&Operative]` rather than any caller's own type, which is what lets
/// [`crate::mixture::coverage_gaps`] delegate to it instead of keeping a
/// second copy.
///
/// **`domain_coverage`, not `coverage`** — [`crate::goodhart::coverage`]
/// already means something genuinely different (`O` and `U` over utility
/// supports, §XIV). Two real notions of coverage in one crate, so the newcomer
/// takes the longer name.
pub fn domain_coverage(operatives: &[&Operative]) -> DomainCoverage {
    let mut claimants: BTreeMap<Cynefin, Vec<String>> = BTreeMap::new();
    for o in operatives {
        for d in o.suited() {
            claimants.entry(*d).or_default().push(o.id().to_string());
        }
    }
    let covered: BTreeSet<Cynefin> = claimants.keys().copied().collect();
    let uncovered = Cynefin::ALL
        .into_iter()
        .filter(|d| !covered.contains(d))
        .collect();
    DomainCoverage {
        covered,
        uncovered,
        claimants,
    }
}

// ---------------------------------------------------------------------------
// Disorder — detected, never declared
// ---------------------------------------------------------------------------

/// Cynefin's fifth state, as **a detection target**.
///
/// ★ Un-declarable by construction: [`Cynefin`] has exactly four variants, so
/// `dom_i = {disorder}` is not something an operative is forbidden from
/// writing — it is something there is no value to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disorder {
    /// `dom(s)` is known and **no operative is suited to it**.
    UncoveredRegime {
        regime: Cynefin,
        /// What the regime would have required of whoever took it.
        requires: ResponseMode,
    },
    /// The Monitor could not determine a regime at all.
    IndeterminateReading { why: String },
}

impl Disorder {
    pub fn describe(&self) -> String {
        match self {
            Disorder::UncoveredRegime { regime, requires } => format!(
                "disorder: the regime is {} and no operative is suited to it — it requires {}, \
                 and nobody claims that method. Surface the gap; do not route to the least-bad scorer.",
                regime.name(),
                requires.name()
            ),
            Disorder::IndeterminateReading { why } => format!(
                "disorder: no domain reading is available — {why}. Not a regime to route in, \
                 and not a value anyone may declare."
            ),
        }
    }
}

/// Route on a supplied reading: who is suited, who is not and why, and whether
/// this is disorder.
#[derive(Debug, Clone, PartialEq)]
pub struct DomainRouting {
    suited: Vec<String>,
    mismatched: Vec<Mismatch>,
    disorder: Option<Disorder>,
}

impl DomainRouting {
    /// Operatives whose `dom_i` contains `dom(s)`. **Empty under disorder** —
    /// that is the correct behaviour, not a degraded one.
    pub fn suited(&self) -> &[String] {
        &self.suited
    }

    /// Every abstention, each carrying the presupposition that failed.
    pub fn mismatched(&self) -> &[Mismatch] {
        &self.mismatched
    }

    pub fn disorder(&self) -> Option<&Disorder> {
        self.disorder.as_ref()
    }

    /// ★ When this is true, the answer is to show a person the gap.
    pub fn surfaces_a_gap(&self) -> bool {
        self.disorder.is_some()
    }

    pub fn describe(&self) -> String {
        match &self.disorder {
            Some(d) => d.describe(),
            None => format!(
                "{} suited, {} abstaining on domain mismatch",
                self.suited.len(),
                self.mismatched.len()
            ),
        }
    }
}

/// `match(i,s)` across a population, over a **supplied** reading.
///
/// Under either kind of [`Disorder`] nobody is routed and the gap is carried,
/// because §IX's instruction is to surface it rather than route to the
/// least-bad scorer and present the output in the usual format.
pub fn route_on_domain(operatives: &[&Operative], reading: &DomainReading) -> DomainRouting {
    let regime = match reading {
        DomainReading::Indeterminate { why } => {
            return DomainRouting {
                suited: Vec::new(),
                mismatched: Vec::new(),
                disorder: Some(Disorder::IndeterminateReading { why: why.clone() }),
            }
        }
        DomainReading::Known(d) => *d,
    };

    let mut suited = Vec::new();
    let mut mismatched = Vec::new();
    for o in operatives {
        match assess(o, regime) {
            Suitability::Suited => suited.push(o.id().to_string()),
            Suitability::Mismatched(m) => mismatched.push(*m),
        }
    }

    let disorder = if suited.is_empty() {
        Some(Disorder::UncoveredRegime {
            regime,
            requires: regime.response_mode(),
        })
    } else {
        None
    };
    if disorder.is_some() {
        // Nobody is routed under disorder. The mismatches are kept, because
        // *why nobody was suited* is the content of the gap.
        return DomainRouting {
            suited: Vec::new(),
            mismatched,
            disorder,
        };
    }

    DomainRouting {
        suited,
        mismatched,
        disorder: None,
    }
}

// ---------------------------------------------------------------------------
// The one inference criticality licenses
// ---------------------------------------------------------------------------

/// The one transition §XIII connects to `σ̂`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionRisk {
    pub from: Cynefin,
    pub to: Cynefin,
    pub criticality: Regime,
}

impl TransitionRisk {
    pub fn describe(&self) -> String {
        format!(
            "{} → {} transition risk: the branching ratio reads {}, and §XIII names \
             near-criticality as precisely this transition. A flag, not a reclassification.",
            self.from.name(),
            self.to.name(),
            self.criticality.name()
        )
    }
}

/// ★ Wire OPV-14's reading in for the **one** inference §XIII licenses.
///
/// > `dom(s)` [is] a Monitor output; §VIII's criticality detector is one of its
/// > inputs, since **near-criticality is precisely the complex → chaotic
/// > transition risk**.
///
/// So a critical or supercritical `σ̂` **on a complex reading** flags that one
/// transition — and nothing else. It does **not** reclassify: the reading is
/// returned untouched by the caller, and there is no path here that produces a
/// `DomainReading`. σ̂ says nothing about clear → complicated, so nothing is
/// inferred there.
pub fn transition_risk(reading: &DomainReading, criticality: Regime) -> Option<TransitionRisk> {
    match reading {
        DomainReading::Known(Cynefin::Complex) if !criticality.mean_is_usable() => {
            Some(TransitionRisk {
                from: Cynefin::Complex,
                to: Cynefin::Chaotic,
                criticality,
            })
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Error)]
pub enum CynefinError {
    #[error("no operatives to route between")]
    NoOperatives,
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operative::{Objective, Sense, Utility};

    fn op(id: &str, suited: &[Cynefin]) -> Operative {
        Operative::new(
            id,
            Utility::new()
                .with(Objective::new("x", "x", Sense::Maximise))
                .unwrap(),
            suited,
        )
        .unwrap()
    }

    // -- the response modes --------------------------------------------------

    #[test]
    fn every_domain_has_a_distinct_response_mode() {
        let modes: BTreeSet<&str> = Cynefin::ALL
            .into_iter()
            .map(|d| d.response_mode().name())
            .collect();
        assert_eq!(modes.len(), 4);
        assert_eq!(
            Cynefin::Complicated.response_mode(),
            ResponseMode::SenseAnalyseRespond
        );
        assert_eq!(Cynefin::Chaotic.response_mode(), ResponseMode::ActSenseRespond);
        assert!(Cynefin::Complicated
            .response_mode()
            .presupposes()
            .contains("analysability"));
    }

    // -- ★ the abstention's reason ------------------------------------------

    #[test]
    fn a_mismatch_names_the_presupposition_that_fails() {
        let optimiser = op("mentor", &[Cynefin::Complicated]);
        let s = assess(&optimiser, Cynefin::Chaotic);
        let m = s.mismatch().expect("mismatched");
        assert_eq!(m.regime, Cynefin::Chaotic);
        // ★ §IX's own words, derived from the declaration.
        assert!(m.why.contains("analysability"));
        assert!(m.why.contains("act → sense → respond"));
        assert!(m.why.contains("confidently wrong"));
        assert!(!s.is_suited());
    }

    #[test]
    fn a_suited_operative_is_simply_suited() {
        let prober = op("curator", &[Cynefin::Complex, Cynefin::Chaotic]);
        assert!(assess(&prober, Cynefin::Complex).is_suited());
        assert!(assess(&prober, Cynefin::Chaotic).is_suited());
        assert!(!assess(&prober, Cynefin::Clear).is_suited());
    }

    // -- ★ coverage, canonical ----------------------------------------------

    #[test]
    fn coverage_measures_requisite_variety_and_enumerates_claimants() {
        let a = op("a", &[Cynefin::Clear, Cynefin::Complicated]);
        let b = op("b", &[Cynefin::Complex]);
        let c = domain_coverage(&[&a, &b]);
        assert!(!c.has_requisite_variety());
        assert_eq!(c.uncovered(), &[Cynefin::Chaotic].into_iter().collect());
        // ★ Enumerable: who claims what.
        assert_eq!(c.claimants(Cynefin::Clear), ["a".to_string()]);
        assert_eq!(c.claimants(Cynefin::Complex), ["b".to_string()]);
        assert!(c.claimants(Cynefin::Chaotic).is_empty());
        assert!(c.describe().contains("surface the gap"));

        let d = op("d", &[Cynefin::Chaotic]);
        let full = domain_coverage(&[&a, &b, &d]);
        assert!(full.has_requisite_variety());
        assert!(full.describe().contains("requisite variety"));
    }

    #[test]
    fn an_operative_declaring_nothing_is_never_suited() {
        let dead = op("dead", &[]);
        let live = op("live", &[Cynefin::Clear]);
        let c = domain_coverage(&[&dead, &live]);
        assert_eq!(c.never_suited(&[&dead, &live]), vec!["dead".to_string()]);
    }

    // -- ★ disorder ----------------------------------------------------------

    #[test]
    fn disorder_is_not_a_declarable_value() {
        // ★ Structural: there is no fifth variant to declare.
        assert_eq!(Cynefin::ALL.len(), 4);
        let names: BTreeSet<&str> = Cynefin::ALL.into_iter().map(|d| d.name()).collect();
        assert!(!names.contains("disorder"));
    }

    #[test]
    fn an_uncovered_regime_surfaces_a_gap_and_routes_nobody() {
        let a = op("a", &[Cynefin::Clear]);
        let b = op("b", &[Cynefin::Complicated]);
        let r = route_on_domain(&[&a, &b], &DomainReading::known(Cynefin::Chaotic));
        assert!(r.surfaces_a_gap());
        assert!(r.suited().is_empty());
        // The mismatches are kept — why nobody was suited IS the gap.
        assert_eq!(r.mismatched().len(), 2);
        match r.disorder().expect("disorder") {
            Disorder::UncoveredRegime { regime, requires } => {
                assert_eq!(*regime, Cynefin::Chaotic);
                assert_eq!(*requires, ResponseMode::ActSenseRespond);
            }
            other => panic!("expected UncoveredRegime, got {other:?}"),
        }
        assert!(r.describe().contains("least-bad scorer"));
    }

    #[test]
    fn an_indeterminate_reading_is_disorder_too() {
        let a = op("a", &[Cynefin::Clear]);
        let r = route_on_domain(
            &[&a],
            &DomainReading::indeterminate("the Monitor has too little history to say"),
        );
        assert!(r.surfaces_a_gap());
        assert!(r.suited().is_empty());
        assert!(matches!(
            r.disorder(),
            Some(Disorder::IndeterminateReading { .. })
        ));
        assert!(r.describe().contains("not a value anyone may declare"));
    }

    #[test]
    fn a_covered_regime_routes_the_suited_and_abstains_the_rest() {
        let a = op("a", &[Cynefin::Clear]);
        let b = op("b", &[Cynefin::Complicated]);
        let r = route_on_domain(&[&a, &b], &DomainReading::known(Cynefin::Clear));
        assert!(!r.surfaces_a_gap());
        assert_eq!(r.suited(), ["a".to_string()]);
        assert_eq!(r.mismatched().len(), 1);
        assert_eq!(r.mismatched()[0].operative, "b");
    }

    // -- ★ the one licensed inference ---------------------------------------

    #[test]
    fn criticality_flags_complex_to_chaotic_and_nothing_else() {
        let complex = DomainReading::known(Cynefin::Complex);
        let risk = transition_risk(&complex, Regime::Supercritical).expect("flagged");
        assert_eq!((risk.from, risk.to), (Cynefin::Complex, Cynefin::Chaotic));
        assert!(risk.describe().contains("not a reclassification"));

        // Calm complex: no flag.
        assert!(transition_risk(&complex, Regime::Subcritical).is_none());
        // ★ σ̂ says nothing about the other domains, so nothing is inferred.
        for d in [Cynefin::Clear, Cynefin::Complicated, Cynefin::Chaotic] {
            assert!(transition_risk(&DomainReading::known(d), Regime::Supercritical).is_none());
        }
        assert!(transition_risk(&DomainReading::indeterminate("x"), Regime::Supercritical).is_none());
    }

    #[test]
    fn a_flag_does_not_reclassify_the_reading() {
        let reading = DomainReading::known(Cynefin::Complex);
        let _ = transition_risk(&reading, Regime::Critical);
        // ★ Untouched — there is no path here that produces a DomainReading.
        assert_eq!(reading.domain(), Some(Cynefin::Complex));
    }
}
