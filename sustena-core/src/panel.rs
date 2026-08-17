//! The persistent-panel invariant (CTL-8; the Controller paper, §VIII).
//!
//! ## `I(p)`, and why it is declared rather than computed
//!
//! §VIII defines a panel's information value as
//!
//! ```text
//!   I(p) = H(state | panel absent) − H(state | panel present)
//! ```
//!
//! and the invariant follows from it: **a panel is persistent iff `I(p)` is
//! high for ALL states.** A dynamic widget earns its slot in some states — the
//! pocket it watches is strained *now* — while a persistent panel reduces
//! uncertainty whatever the state happens to be. Console, system health, IoT
//! status, active sessions: you want them whether or not anything is wrong,
//! because *nothing is wrong* is itself the answer they give.
//!
//! ★★ **The entropy is not computed here, and that is deliberate.** `H(state |
//! ·)` over a real sustain's state space is intractable in exactly the way
//! MON-2's Kalman filter and MON-1's observability matrix are — and this
//! codebase has twice declined to approximate a quantity rather than fabricate
//! it. So persistence is a **declared policy** with `I(p)` as its
//! **justification**, and [`PanelDecl::justification`] is where that argument
//! is written down so a person can disagree with it. There is no
//! `information_value()` returning a number, because there is no honest number
//! to return.
//!
//! Same treatment `Region::weights` already carries (*"A modelling choice, not
//! a fact. Declared here so it can be argued with"*) and [`SaliencePolicy`]
//! gained in UI-7.
//!
//! [`SaliencePolicy`]: crate::curated::SaliencePolicy
//!
//! ## ★★★ Two tiers, and the persistent one is OUTSIDE the attention budget
//!
//! ```text
//!   view = persistent(policy = ALWAYS)          — unconditional
//!        ∪ knapsack(dynamic widgets, K ≈ 4)     — competing
//! ```
//!
//! A persistent panel is shown **whatever else is happening**, so it must not
//! consume one of Cowan's four chunks — a panel that could be displaced by a
//! busy day is not persistent, it is merely high-scoring. `View::spent` counts
//! the dynamic tier alone, and a persistent panel appears even when the budget
//! is full of maximally-urgent widgets.
//!
//! ★ **`WHEN_ACTIVE` is the other half and needs no machinery**: such a panel
//! *is* a widget — eligible through β, selected through the knapsack, exactly
//! like any other. The policy is genuinely two-valued with a real behavioural
//! difference, rather than a label with one live branch.
//!
//! ## ★★★ No stored state, which is what resolves the tension
//!
//! `compose_view` resolves fresh on every call and stores nothing, and a
//! persistent panel needs **nothing stored**: it is not a remembered identity
//! surviving a recomposition, it is an **unconditional inclusion by declared
//! policy**, re-derived each time. That is why CTL-8 is a core property rather
//! than host state — adding this tier required no storage at all.
//!
//! ## ★★★ A widget cannot declare itself persistent
//!
//! Persistence would be the perfect Goodhart move: skip the knapsack entirely
//! by claiming to be always-informative. So it is **unspellable from the widget
//! side** — [`crate::widget::WidgetDecl`] has no policy field, and a
//! [`PanelDecl`] is a *wrapper* only the household constructs. A widget's
//! declaration is the same either way; what differs is whether the household
//! chose to wrap it.
//!
//! That is the fourth salience surface, sitting alongside UI-13's three:
//! brightness (MON-7), score (UI-1/UI-7), position (UI-13), and now
//! **exemption**.
//!
//! ## Honest scope
//!
//! **Flicker-stability is not this row.** Keeping a *dynamic* widget in its
//! slot across recompositions to avoid churn is a genuinely different and
//! harder concern — it needs memory of the last view, which is exactly what
//! `compose_view` deliberately does not keep. CTL-8 is the always-shown tier;
//! hysteresis on the competing tier is separate work and is named rather than
//! smuggled in here.

use crate::editing::Definition;
use crate::operator::Registry;
use crate::widget::{LoadedWidget, LoadError, WidgetDecl, WidgetSet};

/// §VIII's `PANEL_POLICY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PanelPolicy {
    /// `I(p)` argued high for **all** states — shown unconditionally, outside
    /// the attention budget.
    Always,
    /// High `I(p)` only in particular states, so it competes like any widget:
    /// eligible through β, selected through the knapsack.
    WhenActive,
}

/// A panel declaration — **a household's wrapper around a widget**, never
/// something a widget carries.
///
/// ★★★ [`WidgetDecl`] has no policy field, so a widget cannot exempt itself
/// from the attention budget by claiming persistence. The wrapping is the
/// household's act; the widget's own declaration is identical either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelDecl {
    widget: WidgetDecl,
    policy: PanelPolicy,
    justification: String,
}

/// Why a panel set could not be declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelError {
    /// An `ALWAYS` panel with no argument for why `I(p)` is high everywhere.
    ///
    /// ★ Refused rather than defaulted: the entropy is not computed, so the
    /// justification is the *only* thing standing behind the claim. A panel
    /// that skips the budget without saying why is an exemption nobody can
    /// argue with.
    UnjustifiedPersistence { panel: String },
    /// The panel's widget half did not type-check (UI-2).
    Widget(Vec<LoadError>),
}

impl std::fmt::Display for PanelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PanelError::UnjustifiedPersistence { panel } => write!(
                f,
                "panel '{panel}' claims ALWAYS but argues no I(p): persistence skips the \
                 attention budget, so the reason must be written down to be argued with"
            ),
            PanelError::Widget(errors) => {
                write!(f, "panel widget did not type-check: {} error(s)", errors.len())
            }
        }
    }
}

impl PanelDecl {
    /// Declare a panel **persistent**. The justification is required.
    pub fn always(widget: WidgetDecl, justification: &str) -> PanelDecl {
        PanelDecl {
            widget,
            policy: PanelPolicy::Always,
            justification: justification.to_string(),
        }
    }

    /// Declare a panel that competes like any widget.
    pub fn when_active(widget: WidgetDecl) -> PanelDecl {
        PanelDecl {
            widget,
            policy: PanelPolicy::WhenActive,
            justification: String::new(),
        }
    }

    pub fn policy(&self) -> PanelPolicy {
        self.policy
    }

    pub fn id(&self) -> &str {
        &self.widget.id
    }

    /// The household's argument for `I(p)` — see the module header on why this
    /// is prose and not a number.
    pub fn justification(&self) -> &str {
        &self.justification
    }
}

/// The household's declared panels, type-checked.
///
/// Persistent panels are held apart from the dynamic ones because they are
/// composed differently — the split is in the type, not in a flag someone has
/// to remember to read.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelSet {
    persistent: WidgetSet,
    persistent_reasons: Vec<(String, String)>,
    dynamic: WidgetSet,
}

impl PanelSet {
    /// Load a declared panel set against `dim(S)` and `T`.
    ///
    /// The widget halves go through UI-2's check unchanged — a persistent panel
    /// is not exempt from type-checking, only from the attention budget.
    pub fn load(
        decls: Vec<PanelDecl>,
        definition: &Definition,
        registry: &Registry,
    ) -> Result<PanelSet, PanelError> {
        let mut always = Vec::new();
        let mut reasons = Vec::new();
        let mut when_active = Vec::new();

        for decl in decls {
            match decl.policy {
                PanelPolicy::Always => {
                    if decl.justification.trim().is_empty() {
                        return Err(PanelError::UnjustifiedPersistence {
                            panel: decl.widget.id.clone(),
                        });
                    }
                    reasons.push((decl.widget.id.clone(), decl.justification));
                    always.push(decl.widget);
                }
                PanelPolicy::WhenActive => when_active.push(decl.widget),
            }
        }

        let persistent =
            WidgetSet::load(always, definition, registry).map_err(PanelError::Widget)?;
        let dynamic =
            WidgetSet::load(when_active, definition, registry).map_err(PanelError::Widget)?;

        Ok(PanelSet { persistent, persistent_reasons: reasons, dynamic })
    }

    /// An empty set — a household that has declared no panels.
    pub fn none() -> PanelSet {
        PanelSet {
            persistent: WidgetSet::empty(),
            persistent_reasons: Vec::new(),
            dynamic: WidgetSet::empty(),
        }
    }

    /// The always-shown tier.
    pub fn persistent(&self) -> &WidgetSet {
        &self.persistent
    }

    /// The competing tier — these are widgets, and are composed as such.
    pub fn dynamic(&self) -> &WidgetSet {
        &self.dynamic
    }

    pub fn justification_for(&self, id: &str) -> Option<&str> {
        self.persistent_reasons
            .iter()
            .find(|(pid, _)| pid == id)
            .map(|(_, reason)| reason.as_str())
    }

    pub fn is_persistent(&self, id: &str) -> bool {
        self.persistent.get(id).is_some()
    }
}

/// A persistent panel as it appears in a view.
///
/// ★★★ It carries **no score and no rank**. Ranking it would put it back into
/// the competition it is exempt from, and a score would invite a surface to
/// order it against widgets it was never competing with.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelInView {
    pub id: String,
    pub render: String,
    /// The household's argument for showing this whatever the state.
    pub justification: String,
}

impl PanelInView {
    pub(crate) fn of(widget: &LoadedWidget, justification: &str) -> PanelInView {
        PanelInView {
            id: widget.id().to_string(),
            render: widget.render().to_string(),
            justification: justification.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DimType, Schema};

    fn definition() -> Definition {
        Definition::new(Schema::new().declare("balance", DimType::Number { lo: None, hi: None }))
    }

    fn console() -> WidgetDecl {
        WidgetDecl::new("console", "console").unit().reading("balance")
    }

    #[test]
    fn a_persistent_panel_must_argue_its_information_value() {
        // ★ There is no computed I(p) standing behind the claim, so the
        // argument is the only thing that does.
        let decls = vec![PanelDecl::always(console(), "   ")];
        assert_eq!(
            PanelSet::load(decls, &definition(), &Registry::default()),
            Err(PanelError::UnjustifiedPersistence { panel: "console".into() })
        );
    }

    #[test]
    fn the_justification_travels_with_the_panel() {
        let set = PanelSet::load(
            vec![PanelDecl::always(console(), "the console answers 'is anything running' in every state")],
            &definition(),
            &Registry::default(),
        )
        .unwrap();
        assert!(set.is_persistent("console"));
        assert!(set.justification_for("console").unwrap().contains("every state"));
    }

    #[test]
    fn a_when_active_panel_is_just_a_widget() {
        let set = PanelSet::load(
            vec![PanelDecl::when_active(console())],
            &definition(),
            &Registry::default(),
        )
        .unwrap();
        assert!(!set.is_persistent("console"));
        assert_eq!(set.dynamic().len(), 1);
        assert_eq!(set.persistent().len(), 0);
    }

    #[test]
    fn a_persistent_panel_still_type_checks() {
        // Exempt from the budget, not from `inputs ⊆ dim(S)`.
        let bad = WidgetDecl::new("ghost", "card").unit().reading("not_a_dimension");
        assert!(matches!(
            PanelSet::load(vec![PanelDecl::always(bad, "because")], &definition(), &Registry::default()),
            Err(PanelError::Widget(_))
        ));
    }

    #[test]
    fn there_is_no_information_value_function() {
        // ★★ Structural, and the point: `H(state | ·)` is intractable, so no
        // method returns a number for it. The justification is prose because
        // there is no honest number to return — the same position MON-1 takes
        // on the observability matrix and MON-2 on Kalman.
        let set = PanelSet::load(
            vec![PanelDecl::always(console(), "always informative")],
            &definition(),
            &Registry::default(),
        )
        .unwrap();
        // The only thing retrievable is the argument.
        assert_eq!(set.justification_for("console"), Some("always informative"));
    }
}
