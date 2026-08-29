//! **What gets copied between households, and whether it can be argued with**
//! (Operative · §III).
//!
//! ```text
//!   Model     S × T → Δ(S)      a claim about how the world moves
//!   Strategy  a DAG             a way of acting
//!   Resource  ∈ S               a thing you have
//!   Network   an edge in ⊕      who you are joined to
//! ```
//!
//! ★★★ **Dawkins–Campbell: selection needs all three of copyable, variable and
//! differentially retained, and two out of three is not nearly.** A thing that
//! copies perfectly and never varies cannot improve; one that varies and is
//! never retained differentially is noise; one nobody copies is not
//! transmitted at all. So [`Replication::selects`] wants all three — and when it
//! refuses it **names the missing leg**, because *"this is not a meme"* is
//! unactionable and *"nothing ever varies it"* is a fix.
//!
//! ## Popper–Deutsch: an anti-rational meme escapes criticism
//!
//! ★★★ **Being unfalsifiable is not the fault. Being unfalsifiable *and*
//! selected-for is.** A bag of rice makes no claim and cannot be wrong; calling
//! it anti-rational would be a category error, and a check that did so would cry
//! wolf at the entire pantry. [`anti_rational`] therefore applies only to the
//! kinds that make claims — a Model says how the world moves, a Strategy says
//! what to do — and only when the thing is actually being retained.
//!
//! ★★★ **The dangerous meme is the one that spreads because it cannot be
//! tested**, not the one that is merely wrong. A wrong Model loses to a better
//! one on the evidence. A Model that admits no evidence never loses, and that is
//! the whole of what makes it anti-rational.

use std::collections::BTreeSet;

/// The four kinds a transmissible thing can be.
///
/// ★★ Distinct from [`crate::package::Kind`], which asks *what can be
/// published*. This asks *what is being copied*, and the two genuinely differ:
/// a Resource is copied between households and is not a publishable artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemeKind {
    /// `S × T → Δ(S)` — a claim about how the world moves.
    Model,
    /// A DAG — a way of acting.
    Strategy,
    /// `∈ S` — a thing you have.
    Resource,
    /// An edge in `⊕` — who you are joined to.
    Network,
}

impl MemeKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Strategy => "strategy",
            Self::Resource => "resource",
            Self::Network => "network",
        }
    }

    /// ★★★ Does this kind of thing make a claim that could be wrong?
    ///
    /// A Model and a Strategy do. A Resource and a Network do not — they are
    /// had, not asserted, and holding them to a standard of falsifiability
    /// would be a category error.
    pub fn makes_a_claim(&self) -> bool {
        matches!(self, Self::Model | Self::Strategy)
    }
}

/// How, if at all, this can be shown to be wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Criticism {
    /// There is something that would count as evidence against it.
    ///
    /// ★★ The `by` is required. "It is falsifiable" without saying by what is
    /// itself an unfalsifiable claim.
    Falsifiable { by: String },
    /// Nothing would count against it.
    Unfalsifiable { why: String },
}

impl Criticism {
    pub fn can_be_argued_with(&self) -> bool {
        matches!(self, Self::Falsifiable { .. })
    }
}

/// Whether selection can operate on this at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Replication {
    /// Somebody else can take it.
    pub copyable: bool,
    /// Copies differ.
    pub variable: bool,
    /// Some copies survive more than others.
    pub differentially_retained: bool,
}

impl Replication {
    pub fn new(copyable: bool, variable: bool, differentially_retained: bool) -> Self {
        Self { copyable, variable, differentially_retained }
    }

    /// All three, or nothing.
    pub fn selects(&self) -> bool {
        self.copyable && self.variable && self.differentially_retained
    }

    /// ★★★ Which leg is missing — the actionable half.
    ///
    /// "This is not a meme" tells somebody nothing. "Copies of it never differ,
    /// so it cannot improve" tells them what to change.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.copyable {
            out.push("nobody can take a copy, so it is not transmitted at all");
        }
        if !self.variable {
            out.push("copies never differ, so it cannot improve");
        }
        if !self.differentially_retained {
            out.push("every copy survives equally, so nothing is being selected");
        }
        out
    }
}

/// One transmissible thing.
#[derive(Debug, Clone, PartialEq)]
pub struct Meme {
    pub id: String,
    pub kind: MemeKind,
    pub replication: Replication,
    /// ★★ `None` for a kind that makes no claim. Not "unfalsifiable" — a bag of
    /// rice is not making an unfalsifiable assertion, it is not asserting.
    pub criticism: Option<Criticism>,
}

impl Meme {
    pub fn new(id: &str, kind: MemeKind, replication: Replication) -> Self {
        Self { id: id.into(), kind, replication, criticism: None }
    }

    pub fn criticised_by(mut self, c: Criticism) -> Self {
        self.criticism = Some(c);
        self
    }
}

/// What is wrong with a meme, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// Selection cannot operate on it, and here is why.
    NotUnderSelection { missing: Vec<&'static str> },
    /// ★★★ It makes a claim, admits no evidence against it, and is being
    /// retained anyway. **The anti-rational case.**
    EscapesCriticism { why: String },
    /// It makes a claim and nobody has said how it could be wrong.
    ///
    /// ★★ Different from escaping criticism: this is an omission somebody can
    /// fill in, not a structure that resists filling.
    NoCriticismDeclared,
}

impl Finding {
    pub fn describe(&self, id: &str) -> String {
        match self {
            Self::NotUnderSelection { missing } => {
                format!("{id} is not under selection: {}", missing.join("; "))
            }
            Self::EscapesCriticism { why } => format!(
                "{id} spreads and cannot be argued with ({why}) — it does not win on the \
                 evidence, it wins by admitting none"
            ),
            Self::NoCriticismDeclared => {
                format!("{id} makes a claim and nobody has said what would count against it")
            }
        }
    }
}

/// **Is this an anti-rational meme?**
///
/// ★★★ Claim-making, uncriticisable, and actually retained. All three, because
/// each alone is ordinary: a resource makes no claim, an untested idea nobody
/// copies harms nobody, and a falsifiable idea that spreads is just an idea that
/// is working.
pub fn anti_rational(meme: &Meme) -> bool {
    meme.kind.makes_a_claim()
        && meme.replication.differentially_retained
        && matches!(meme.criticism, Some(Criticism::Unfalsifiable { .. }))
}

/// Everything worth saying about one meme.
pub fn inspect(meme: &Meme) -> Vec<Finding> {
    let mut out = Vec::new();
    if !meme.replication.selects() {
        out.push(Finding::NotUnderSelection { missing: meme.replication.missing() });
    }
    if meme.kind.makes_a_claim() {
        match &meme.criticism {
            None => out.push(Finding::NoCriticismDeclared),
            Some(Criticism::Unfalsifiable { why }) if meme.replication.differentially_retained => {
                out.push(Finding::EscapesCriticism { why: why.clone() })
            }
            _ => {}
        }
    }
    out
}

/// The library: what a household holds, and what is wrong with any of it.
///
/// ★★ Ordered by id so a report over the same library reads the same twice.
pub fn audit(library: &[Meme]) -> Vec<(String, Vec<Finding>)> {
    let mut out: Vec<(String, Vec<Finding>)> = library
        .iter()
        .map(|m| (m.id.clone(), inspect(m)))
        .filter(|(_, f)| !f.is_empty())
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Which kinds a library actually holds.
///
/// ★★ A library of nothing but Resources is a household that copies things and
/// no ways of thinking, which is a real observation about it rather than a
/// defect to fix.
pub fn kinds_held(library: &[Meme]) -> BTreeSet<MemeKind> {
    library.iter().map(|m| m.kind).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selecting() -> Replication {
        Replication::new(true, true, true)
    }

    fn a_model(id: &str) -> Meme {
        Meme::new(id, MemeKind::Model, selecting())
    }

    #[test]
    fn selection_needs_all_three_and_names_the_missing_leg() {
        // ★★★ "This is not a meme" is unactionable; "copies never differ, so it
        //     cannot improve" tells somebody what to change.
        let frozen = Replication::new(true, false, true);
        assert!(!frozen.selects());
        assert_eq!(frozen.missing(), vec!["copies never differ, so it cannot improve"]);
    }

    #[test]
    fn two_out_of_three_is_not_nearly() {
        for r in [
            Replication::new(false, true, true),
            Replication::new(true, false, true),
            Replication::new(true, true, false),
        ] {
            assert!(!r.selects(), "{r:?}");
            assert_eq!(r.missing().len(), 1);
        }
        assert!(selecting().selects());
    }

    #[test]
    fn a_bag_of_rice_is_not_anti_rational_for_failing_to_be_falsifiable() {
        // ★★★ A category error, and a check that made it would cry wolf at the
        //     entire pantry. A Resource is had, not asserted.
        let rice = Meme::new("a sack of rice", MemeKind::Resource, selecting());
        assert!(!anti_rational(&rice));
        assert!(inspect(&rice).is_empty());
    }

    #[test]
    fn a_network_edge_makes_no_claim_either() {
        let neighbour = Meme::new("the Otieno household", MemeKind::Network, selecting());
        assert!(!anti_rational(&neighbour));
        assert!(inspect(&neighbour).is_empty());
    }

    #[test]
    fn a_model_that_spreads_and_admits_no_evidence_is_the_anti_rational_case() {
        // ★★★ The dangerous one is not the wrong model — a wrong model loses to
        //     a better one on the evidence. It is the model that admits no
        //     evidence, and therefore never loses.
        let m = a_model("saving is pointless").criticised_by(Criticism::Unfalsifiable {
            why: "any outcome is read as confirming it".into(),
        });
        assert!(anti_rational(&m));
        let f = inspect(&m);
        assert!(f[0].describe(&m.id).contains("it wins by admitting none"));
    }

    #[test]
    fn an_unfalsifiable_idea_nobody_retains_harms_nobody() {
        // ★★★ All three conditions, because each alone is ordinary. An untested
        //     idea that is not being selected for is just an idea.
        let mut m = a_model("a hunch").criticised_by(Criticism::Unfalsifiable {
            why: "nothing would count against it".into(),
        });
        m.replication = Replication::new(true, true, false);
        assert!(!anti_rational(&m));
    }

    #[test]
    fn a_falsifiable_model_that_spreads_is_just_an_idea_that_is_working() {
        let m = a_model("rent first").criticised_by(Criticism::Falsifiable {
            by: "a month where paying rent first left the household short".into(),
        });
        assert!(!anti_rational(&m));
        assert!(inspect(&m).is_empty());
    }

    #[test]
    fn saying_it_is_falsifiable_without_saying_by_what_is_not_available() {
        // ★★ The `by` is required by the type: "it is falsifiable" with nothing
        //    named is itself an unfalsifiable claim.
        let c = Criticism::Falsifiable { by: "a month it did not hold".into() };
        assert!(c.can_be_argued_with());
        assert!(!Criticism::Unfalsifiable { why: "x".into() }.can_be_argued_with());
    }

    #[test]
    fn a_claim_with_no_criticism_declared_is_an_omission_not_a_structure() {
        // ★★ Different findings because they need different fixes: one is a
        //    blank somebody can fill in, the other is a shape that resists
        //    being filled.
        let bare = a_model("some rule of thumb");
        assert_eq!(inspect(&bare), vec![Finding::NoCriticismDeclared]);
        assert!(!anti_rational(&bare));
    }

    #[test]
    fn a_strategy_makes_a_claim_and_is_held_to_it() {
        // ★★ A way of acting asserts that acting this way is better, which is
        //    something a month can disagree with.
        assert!(MemeKind::Strategy.makes_a_claim());
        assert!(MemeKind::Model.makes_a_claim());
        assert!(!MemeKind::Resource.makes_a_claim());
        assert!(!MemeKind::Network.makes_a_claim());
    }

    #[test]
    fn a_library_audit_reports_only_what_is_wrong_and_reads_the_same_twice() {
        let library = vec![
            a_model("rent first")
                .criticised_by(Criticism::Falsifiable { by: "a short month".into() }),
            a_model("saving is pointless").criticised_by(Criticism::Unfalsifiable {
                why: "any outcome confirms it".into(),
            }),
            Meme::new("a sack of rice", MemeKind::Resource, selecting()),
        ];
        let a = audit(&library);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].0, "saving is pointless");
        assert_eq!(a, audit(&library));
    }

    #[test]
    fn what_a_household_copies_is_a_real_observation_about_it() {
        // ★★ A library of nothing but Resources is a household that copies
        //    things and no ways of thinking — worth noticing, not a defect.
        let only_things = vec![
            Meme::new("rice", MemeKind::Resource, selecting()),
            Meme::new("a jiko", MemeKind::Resource, selecting()),
        ];
        assert_eq!(kinds_held(&only_things), [MemeKind::Resource].into_iter().collect());
    }
}
