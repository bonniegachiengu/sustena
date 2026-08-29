//! **What a market needs to work, and what does not transfer** (Arena · §VIII).
//!
//! ★★★ **Matching theory does not apply here, and it is worth saying so rather
//! than borrowing the vocabulary.** Gale–Shapley and its descendants are about
//! *rival* goods under capacity constraints, where the binding concept is
//! stability against blocking pairs. An artefact is **non-rival and
//! replicable** — so there is no capacity constraint, no rationing, and no
//! blocking pair, because adoption by one buyer does not deny it to another.
//! Reaching for "stable matching" here would import a vocabulary whose central
//! object does not exist.
//!
//! ★★★ **What does transfer is Roth's account of what a market needs at all**:
//! thickness, lack of congestion, and safety. Three requirements, each mapping
//! onto something that already exists.
//!
//! ## The third row is the one to notice
//!
//! ★★★ **In most markets safety is achieved by rules** — disclosure, escrow,
//! enforcement. Here it is achieved **structurally**, because nothing bought can
//! act except as an admitted transition inside the buyer's own viable region.
//! That is not a nicer version of the same guarantee; it is a different kind of
//! guarantee, and it **changes the exploration/exploitation trade-off in the
//! Arena's favour**: a buyer can afford to try a low-evidence artefact in a way
//! a buyer in a conventional software market cannot.
//!
//! ## Biological markets, de-metaphorized
//!
//! ★★★ The mycorrhizal symbiosis and this catalogue are **not analogy and
//! referent** — they are two instances of one model, and the model is the one
//! that has the theorems. [`BiologicalMarket`] is Noë & Hammerstein's four
//! conditions, checked, so "this is a market" is a finding rather than a figure
//! of speech.

/// Noë & Hammerstein's four conditions for market behaviour without cognition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiologicalMarket {
    /// Two classes of trader offering **different** commodities.
    pub two_classes_different_commodities: bool,
    /// Traders can select among partners.
    pub partner_choice: bool,
    /// The exchange rate is set by the alternatives available to each side.
    pub outside_options: bool,
    /// Traders outbid each other for access to partners.
    pub within_class_competition: bool,
}

impl BiologicalMarket {
    /// ★★★ All four, or it is not a market — it is an exchange, and the
    /// theorems do not follow.
    pub fn is_a_market(&self) -> bool {
        self.two_classes_different_commodities
            && self.partner_choice
            && self.outside_options
            && self.within_class_competition
    }

    /// Which conditions are missing, so the answer is actionable.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.two_classes_different_commodities {
            out.push("both sides are offering the same thing");
        }
        if !self.partner_choice {
            out.push("nobody can choose who they trade with");
        }
        if !self.outside_options {
            out.push("neither side has an alternative, so there is no rate to discover");
        }
        if !self.within_class_competition {
            out.push("nobody is competing for access");
        }
        out
    }

    /// The Arena, as it is built.
    pub fn arena() -> Self {
        Self {
            // Publishers and adopters; competence and currency.
            two_classes_different_commodities: true,
            partner_choice: true,
            outside_options: true,
            within_class_competition: true,
        }
    }
}

/// Roth's three requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Requirement {
    /// Enough participants that useful matches exist.
    Thickness,
    /// Participants can evaluate enough options in the time available.
    LackOfCongestion,
    /// Participants can act on true preferences without being exploited.
    Safety,
}

impl Requirement {
    pub fn all() -> [Requirement; 3] {
        [Self::Thickness, Self::LackOfCongestion, Self::Safety]
    }

    /// What discharges it here.
    pub fn instrument(&self) -> &'static str {
        match self {
            Self::Thickness => {
                "the free tier, and the default Symbionts shipping free — a catalogue with \
                 nothing in it has no selection to run"
            }
            Self::LackOfCongestion => "per-niche conditioning, and the attention budget",
            Self::Safety => {
                "the buyer's own gate — an adopted artefact runs under the buyer's V and their \
                 admit()"
            }
        }
    }

    /// ★★★ Is this achieved by **rules** or by **structure**?
    ///
    /// Only safety is structural here, and that is the row's whole point.
    /// Disclosure and escrow are rules somebody must enforce; a gate that
    /// cannot be bypassed is not enforced by anybody.
    pub fn achieved_structurally(&self) -> bool {
        matches!(self, Self::Safety)
    }
}

/// What matching theory would have given, and why it is not available.
///
/// ★★★ A named refusal rather than a silence. Borrowing "stable matching" for a
/// non-rival good imports a vocabulary whose central object — the blocking pair
/// — does not exist here, and the borrowing would not fail loudly. It would
/// just quietly mean nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchingTheory {
    /// Rival goods, capacity constraints, blocking pairs.
    AppliesToRivalGoods,
}

impl MatchingTheory {
    pub fn applies_to(non_rival: bool) -> bool {
        !non_rival
    }

    pub fn why_not() -> &'static str {
        "an artefact is non-rival and replicable, so there is no capacity constraint, no \
         rationing, and no blocking pair — adoption by one buyer does not deny it to another"
    }
}

/// How safely a buyer can experiment, given how safety is achieved.
///
/// ★★★ The consequence worth having: **structural safety changes the
/// exploration/exploitation trade-off**. Where safety comes from rules, trying
/// something unproven risks whatever the rules failed to cover; where it comes
/// from the gate, the worst case is that the artefact is refused. So a buyer
/// here can afford a low-evidence artefact that a buyer elsewhere cannot.
pub fn can_afford_to_experiment(safety_is_structural: bool, evidence: f64) -> bool {
    safety_is_structural || evidence >= 0.8
}

/// Whether a market has what it needs.
pub fn viable(thick: bool, uncongested: bool, safe: bool) -> Vec<Requirement> {
    let mut missing = Vec::new();
    if !thick {
        missing.push(Requirement::Thickness);
    }
    if !uncongested {
        missing.push(Requirement::LackOfCongestion);
    }
    if !safe {
        missing.push(Requirement::Safety);
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_theory_does_not_apply_and_the_refusal_is_named() {
        // ★★★ Borrowing "stable matching" for a non-rival good imports a
        //     vocabulary whose central object does not exist — and the
        //     borrowing would not fail loudly, it would just quietly mean
        //     nothing.
        assert!(!MatchingTheory::applies_to(true));
        assert!(MatchingTheory::applies_to(false));
        assert!(MatchingTheory::why_not().contains("no blocking pair"));
    }

    #[test]
    fn the_arena_satisfies_all_four_conditions_so_it_is_a_market() {
        // ★★★ Not analogy and referent — two instances of one model, and the
        //     model is the one that has the theorems. So "this is a market" is
        //     a finding rather than a figure of speech.
        assert!(BiologicalMarket::arena().is_a_market());
        assert!(BiologicalMarket::arena().missing().is_empty());
    }

    #[test]
    fn three_out_of_four_is_an_exchange_and_not_a_market() {
        // ★★ And the missing condition is named, because "not a market" is
        //    unactionable and "nobody has an alternative" is a thing to fix.
        let captive = BiologicalMarket { outside_options: false, ..BiologicalMarket::arena() };
        assert!(!captive.is_a_market());
        assert_eq!(captive.missing().len(), 1);
        assert!(captive.missing()[0].contains("no rate to discover"));
    }

    #[test]
    fn only_safety_is_achieved_structurally() {
        // ★★★ The row's whole point. Disclosure and escrow are rules somebody
        //     must enforce; a gate that cannot be bypassed is not enforced by
        //     anybody.
        assert!(Requirement::Safety.achieved_structurally());
        assert!(!Requirement::Thickness.achieved_structurally());
        assert!(!Requirement::LackOfCongestion.achieved_structurally());
    }

    #[test]
    fn structural_safety_changes_what_a_buyer_can_afford_to_try() {
        // ★★★ The consequence worth having: where safety comes from rules,
        //     trying something unproven risks whatever the rules failed to
        //     cover. Where it comes from the gate, the worst case is a refusal.
        let unproven = 0.2;
        assert!(can_afford_to_experiment(true, unproven), "here");
        assert!(!can_afford_to_experiment(false, unproven), "in a conventional market");
    }

    #[test]
    fn a_well_evidenced_artefact_is_safe_to_try_either_way() {
        // ★★ The structural guarantee is what makes the LOW-evidence case
        //    affordable; it is not the only thing that makes anything
        //    affordable.
        assert!(can_afford_to_experiment(false, 0.95));
    }

    #[test]
    fn each_requirement_names_the_instrument_that_discharges_it() {
        for r in Requirement::all() {
            assert!(r.instrument().len() > 20, "{r:?}");
        }
        assert!(Requirement::Thickness.instrument().contains("no selection to run"));
    }

    #[test]
    fn a_market_missing_something_says_which() {
        // ★★ "The market is not working" sends somebody looking; "there is
        //    nothing in the catalogue" tells them what to do.
        assert!(viable(true, true, true).is_empty());
        assert_eq!(viable(false, true, true), vec![Requirement::Thickness]);
        assert_eq!(viable(false, false, false).len(), 3);
    }

    #[test]
    fn thickness_is_why_the_free_tier_exists() {
        // ★★ A catalogue with nothing in it has no selection to run, so the
        //    free tier is a market-design instrument rather than generosity.
        assert!(Requirement::Thickness.instrument().contains("free"));
    }
}
