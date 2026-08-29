//! **`u` is a vector, and when it stops being one matters** (Operative · §XI, §XIII).
//!
//! ★★★ **Arrow's wall is real and the escape is an ASSUMPTION, not a trick.**
//! No ordinal rule over three or more options satisfies unrestricted domain,
//! Pareto, independence and non-dictatorship at once. The declared escape
//! (Sen, Harsanyi) is to leave ordinal ranking behind and use *cardinal,
//! interpersonally comparable* utilities — and that comparability is a claim
//! somebody has to make, not a property the numbers have. So
//! [`Comparability`] is declared, and **aggregating across agents without it is
//! refused**: without the claim, adding one person's 0.8 to another's 0.6 is
//! arithmetic on two different scales that happen to share a font.
//!
//! ★★★ **Collapsing the vector too early is unrecoverable.** A scalar cannot
//! say *this is bad for the money and good for the quiet*; it can only say
//! *worse*. Once summed, the disagreement that made the choice a choice is gone
//! and no later step can recover it — which is why [`Utility`] is a vector all
//! the way to presentation, and the sum is a *view* of it rather than its value.
//!
//! ★★★ **`U` ranks, it does not decide.** The aggregate produces an ordering
//! and stops. Choosing within the frontier is the human's residual, and
//! [`crate::presentation`] already refuses to hand back a single option without
//! a declared collapse rule. Nothing here undoes that.
//!
//! ## The ten axes are an interface, not natural kinds
//!
//! ★★★ Russell's caution, honoured structurally: the ten appraisal names
//! ([`APPRAISAL_AXES`]) are a **convenience list of component names** and
//! nothing in this module privileges them. A `Utility` may carry three
//! components nobody has named before, or all ten, or none of them. An emotion
//! here is exactly *an appraisal with a sign and a weight* — a number on a named
//! axis — and there is no type that could hold anything more.

use std::collections::{BTreeMap, BTreeSet};

/// The ten appraisal names, offered as an interface and not a taxonomy.
///
/// ★★★ A `const` list, deliberately not an enum. An enum would make these the
/// only expressible axes, which is precisely the claim Russell's caution says
/// nobody is entitled to make.
pub const APPRAISAL_AXES: [&str; 10] = [
    "joy",
    "sadness",
    "fear",
    "anger",
    "disgust",
    "anxiety",
    "envy",
    "ennui",
    "embarrassment",
    "nostalgia",
];

/// `u_i` — one agent's utility, as components.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Utility {
    components: BTreeMap<String, f64>,
}

impl Utility {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on(mut self, axis: &str, value: f64) -> Self {
        self.components.insert(axis.to_string(), value);
        self
    }

    pub fn get(&self, axis: &str) -> Option<f64> {
        self.components.get(axis).copied()
    }

    pub fn axes(&self) -> Vec<&str> {
        self.components.keys().map(String::as_str).collect()
    }

    /// **A weighted view of the vector — not its value.**
    ///
    /// ★★ Named `scalarise` rather than `total` so a reader can see the vector
    /// is being flattened here and not somewhere earlier by accident.
    ///
    /// ★★★ An axis with **no declared weight contributes nothing**. A default of
    /// one would let an axis nobody weighed decide an outcome, which is a
    /// preference expressed by forgetting.
    pub fn scalarise(&self, weights: &Weights) -> f64 {
        self.components.iter().map(|(k, v)| v * weights.for_axis(k)).sum()
    }

    /// Axes carrying a value that nobody weighed.
    ///
    /// ★★ Askable, because "it contributed nothing" is only the right answer if
    /// somebody meant it to.
    pub fn unweighted_axes(&self, weights: &Weights) -> Vec<&str> {
        self.components
            .keys()
            .filter(|k| weights.declared_for(k).is_none())
            .map(String::as_str)
            .collect()
    }

    /// Does this dominate `other` — at least as good on every shared axis, and
    /// better on one?
    ///
    /// ★★★ **The comparison that needs no weights at all**, and therefore no
    /// interpersonal claim. It is what a frontier is built from, and it is why
    /// a frontier can be honest where a total cannot.
    pub fn dominates(&self, other: &Utility) -> bool {
        let axes: BTreeSet<&str> =
            self.axes().into_iter().chain(other.axes()).collect();
        if axes.is_empty() {
            return false;
        }
        let mut better_somewhere = false;
        for a in axes {
            let (x, y) = (self.get(a).unwrap_or(0.0), other.get(a).unwrap_or(0.0));
            if x < y {
                return false;
            }
            if x > y {
                better_somewhere = true;
            }
        }
        better_somewhere
    }
}

/// Declared weights, per axis.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Weights {
    per_axis: BTreeMap<String, f64>,
}

impl Weights {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn weighing(mut self, axis: &str, w: f64) -> Self {
        self.per_axis.insert(axis.to_string(), w);
        self
    }

    pub fn declared_for(&self, axis: &str) -> Option<f64> {
        self.per_axis.get(axis).copied()
    }

    fn for_axis(&self, axis: &str) -> f64 {
        self.declared_for(axis).unwrap_or(0.0)
    }
}

/// **The claim that two people's numbers are on one scale.**
///
/// ★★★ Declared, because it is an assumption and not a measurement. Sen and
/// Harsanyi's escape from Arrow's impossibility *is* this claim — without it,
/// interpersonal aggregation is arithmetic on two scales that share a font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparability {
    /// Somebody declared that these agents' utilities are on one scale, and on
    /// what basis.
    Declared { basis: String },
    /// ★★★ Nobody did. **Aggregation across agents is refused**, and ranking
    /// within one agent still works — a person can compare their own options
    /// without anybody claiming anything about anybody else.
    Undeclared,
}

/// Whose utility, and how much their view weighs.
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    pub agent: String,
    /// `w_i` — declared, never hardcoded.
    ///
    /// ★★ `w_human = 0.51` is a *declaration* that the person is decisive, not
    /// a constant hidden in a function. Anybody reading the council's weights
    /// sees the claim.
    pub weight: f64,
    pub utility: Utility,
}

/// Why an aggregation could not be done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregationRefusal {
    /// ★★★ More than one agent, and nobody claimed their scales line up.
    NoComparabilityClaim,
    /// Nothing to aggregate.
    NoVoices,
}

impl AggregationRefusal {
    pub fn describe(&self) -> String {
        match self {
            Self::NoComparabilityClaim => "these utilities have not been declared comparable, so \
                                           adding them would be arithmetic on two different \
                                           scales that happen to share a font"
                .into(),
            Self::NoVoices => "there is nobody to aggregate".into(),
        }
    }
}

/// An ordering over options. **Not a decision.**
///
/// ★★★ There is no `winner()`. Reaching one option is
/// [`crate::presentation::Presented::collapse`]'s job, which demands a declared
/// rule and reports what it dropped. Handing back a top id here would route
/// around that, and the routing-around is the failure §XIII names.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    /// Options with their aggregate scalar, best first, ties broken by name.
    pub ordered: Vec<(String, f64)>,
    /// Options no other option dominates on the raw vectors.
    ///
    /// ★★ Computed **without weights**, so it survives disagreement about them.
    pub frontier: Vec<String>,
}

/// **Aggregate one option's utilities across voices.**
pub fn aggregate(
    voices: &[Voice],
    weights: &Weights,
    comparability: &Comparability,
) -> Result<f64, AggregationRefusal> {
    if voices.is_empty() {
        return Err(AggregationRefusal::NoVoices);
    }
    let agents: BTreeSet<&str> = voices.iter().map(|v| v.agent.as_str()).collect();
    if agents.len() > 1 && matches!(comparability, Comparability::Undeclared) {
        return Err(AggregationRefusal::NoComparabilityClaim);
    }
    Ok(voices.iter().map(|v| v.weight * v.utility.scalarise(weights)).sum())
}

/// **Rank options — and hand back the frontier alongside the order.**
pub fn rank(
    options: &[(String, Vec<Voice>)],
    weights: &Weights,
    comparability: &Comparability,
) -> Result<Ranked, AggregationRefusal> {
    let mut ordered = Vec::new();
    for (name, voices) in options {
        ordered.push((name.clone(), aggregate(voices, weights, comparability)?));
    }
    ordered.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
    });

    // The frontier is over the *summed vectors* per option, weightless.
    let vectors: Vec<(&str, Utility)> = options
        .iter()
        .map(|(n, voices)| {
            let mut u = Utility::new();
            for v in voices {
                for a in v.utility.axes() {
                    let add = v.utility.get(a).unwrap_or(0.0);
                    let now = u.get(a).unwrap_or(0.0);
                    u = u.on(a, now + add);
                }
            }
            (n.as_str(), u)
        })
        .collect();
    let mut frontier: Vec<String> = vectors
        .iter()
        .filter(|(n, u)| !vectors.iter().any(|(m, other)| m != n && other.dominates(u)))
        .map(|(n, _)| n.to_string())
        .collect();
    frontier.sort();

    Ok(Ranked { ordered, frontier })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money_and_quiet(m: f64, q: f64) -> Utility {
        Utility::new().on("money", m).on("quiet", q)
    }

    fn w() -> Weights {
        Weights::new().weighing("money", 1.0).weighing("quiet", 1.0)
    }

    fn declared() -> Comparability {
        Comparability::Declared { basis: "the household agreed one shilling is one point".into() }
    }

    fn voice(agent: &str, weight: f64, u: Utility) -> Voice {
        Voice { agent: agent.into(), weight, utility: u }
    }

    #[test]
    fn adding_two_people_up_without_a_comparability_claim_is_refused() {
        // ★★★ Arrow's escape is an assumption, not a trick. Without the claim
        //     this is arithmetic on two scales that share a font.
        let voices =
            [voice("bonnie", 0.51, money_and_quiet(1.0, 0.0)), voice("mentor", 0.49, money_and_quiet(0.0, 1.0))];
        let refused = aggregate(&voices, &w(), &Comparability::Undeclared).expect_err("refuses");
        assert_eq!(refused, AggregationRefusal::NoComparabilityClaim);
        assert!(refused.describe().contains("share a font"));
    }

    #[test]
    fn one_person_can_rank_their_own_options_without_claiming_anything_about_anybody() {
        // ★★★ The refusal is about INTERPERSONAL comparison. A person weighing
        //     their own money against their own quiet needs no claim about a
        //     neighbour, and refusing that would be a wall in the wrong place.
        let alone = [voice("bonnie", 1.0, money_and_quiet(2.0, 1.0))];
        assert_eq!(aggregate(&alone, &w(), &Comparability::Undeclared), Ok(3.0));
    }

    #[test]
    fn a_declared_claim_lets_the_council_add_up() {
        let voices =
            [voice("bonnie", 0.51, money_and_quiet(1.0, 0.0)), voice("mentor", 0.49, money_and_quiet(0.0, 1.0))];
        let total = aggregate(&voices, &w(), &declared()).expect("declared");
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_humans_decisive_weight_is_a_declaration_and_not_a_constant() {
        // ★★ `w_human = 0.51` is a claim anybody reading the weights can see,
        //    rather than a number hidden inside a function.
        let voices = [
            voice("bonnie", 0.51, money_and_quiet(1.0, 1.0)),
            voice("mentor", 0.49, money_and_quiet(-1.0, -1.0)),
        ];
        let total = aggregate(&voices, &w(), &declared()).expect("declared");
        assert!(total > 0.0, "the person's view carries the day, because it was declared to");
    }

    #[test]
    fn an_axis_nobody_weighed_contributes_nothing_and_says_so() {
        // ★★★ A default weight of one would let an axis nobody thought about
        //     decide an outcome — a preference expressed by forgetting.
        let u = money_and_quiet(1.0, 5.0).on("nostalgia", 100.0);
        let weights = Weights::new().weighing("money", 1.0).weighing("quiet", 1.0);
        assert_eq!(u.scalarise(&weights), 6.0);
        assert_eq!(u.unweighted_axes(&weights), vec!["nostalgia"]);
    }

    #[test]
    fn the_frontier_needs_no_weights_and_so_survives_disagreement_about_them() {
        // ★★★ Dominance is the comparison that needs no interpersonal claim at
        //     all, which is why a frontier can be honest where a total cannot.
        let good_money = money_and_quiet(5.0, 0.0);
        let good_quiet = money_and_quiet(0.0, 5.0);
        assert!(!good_money.dominates(&good_quiet));
        assert!(!good_quiet.dominates(&good_money));
        assert!(money_and_quiet(5.0, 5.0).dominates(&good_money));
    }

    #[test]
    fn ranking_hands_back_an_order_and_a_frontier_and_no_winner() {
        // ★★★ There is no `winner()`. Reaching one option is a collapse with a
        //     declared rule, and routing around that is the failure §XIII names.
        let options = vec![
            ("spend".to_string(), vec![voice("bonnie", 1.0, money_and_quiet(5.0, 0.0))]),
            ("save".to_string(), vec![voice("bonnie", 1.0, money_and_quiet(0.0, 5.0))]),
        ];
        let r = rank(&options, &w(), &declared()).expect("ranks");
        assert_eq!(r.ordered.len(), 2);
        assert_eq!(r.frontier, vec!["save", "spend"], "neither dominates the other");
    }

    #[test]
    fn a_dominated_option_leaves_the_frontier_but_stays_in_the_order() {
        // ★★ The order is still a full report. Dropping a dominated option from
        //    the ranking too would hide that it was considered.
        let options = vec![
            ("good".to_string(), vec![voice("b", 1.0, money_and_quiet(5.0, 5.0))]),
            ("worse".to_string(), vec![voice("b", 1.0, money_and_quiet(1.0, 1.0))]),
        ];
        let r = rank(&options, &w(), &declared()).expect("ranks");
        assert_eq!(r.frontier, vec!["good"]);
        assert_eq!(r.ordered.len(), 2);
    }

    #[test]
    fn a_scalar_cannot_say_bad_for_the_money_and_good_for_the_quiet() {
        // ★★★ Once summed, the disagreement that made it a choice is gone and
        //     nothing later can recover it. Two genuinely different options,
        //     one identical number.
        let a = money_and_quiet(5.0, 0.0);
        let b = money_and_quiet(0.0, 5.0);
        assert_eq!(a.scalarise(&w()), b.scalarise(&w()));
        assert_ne!(a, b, "the vectors still know they are different");
    }

    #[test]
    fn the_ten_names_are_a_list_and_not_a_taxonomy() {
        // ★★★ Russell's caution, structural: an enum would make these the only
        //     expressible axes, which is exactly the claim nobody is entitled
        //     to make. A utility may carry an axis nobody has ever named.
        assert_eq!(APPRAISAL_AXES.len(), 10);
        let unnamed = Utility::new().on("the way the kitchen feels on a Sunday", 0.8);
        assert_eq!(unnamed.axes(), vec!["the way the kitchen feels on a Sunday"]);
    }

    #[test]
    fn an_emotion_here_is_an_appraisal_with_a_sign_and_a_weight() {
        // ★★ There is no type that could hold anything more, which is the
        //    honest reading of §XI rather than a claim about experience.
        let anxious = Utility::new().on("anxiety", -0.7).on("joy", 0.2);
        let weights = Weights::new().weighing("anxiety", 2.0).weighing("joy", 1.0);
        assert!((anxious.scalarise(&weights) - (-1.2)).abs() < 1e-9);
    }

    #[test]
    fn nobody_to_aggregate_is_its_own_refusal() {
        assert_eq!(aggregate(&[], &w(), &declared()), Err(AggregationRefusal::NoVoices));
    }
}
