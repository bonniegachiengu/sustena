//! `w = ⟨inputs, render, emits⟩`, and the check **on the load path** (UI-2; the
//! Curated UI widget schema).
//!
//! ## ★★ What was here before this row: nothing
//!
//! Checked before designing: `curated` is **grep-0** in `sustena-core/src`, and
//! every `widget` / `Widget` / `salience` hit is **prose in a doc comment**.
//! There is no widget type, no binding table, no `compose(r)`, no knapsack and
//! no load path. So this row is not *port a checker* — a checker beside nothing
//! would check nothing. It is **build the typed load path with the check on
//! it**, which is the only shape in which UI-2's property means anything.
//!
//! ★ That also corrects a line this repository wrote yesterday: MON-7's vector
//! recorded *urgency → selection* as `AT PARITY — curated_ui.py's knapsack
//! there, curated_ui.rs's equivalent here`. **There is no `curated_ui.rs`.**
//! The selection half is Python-only and stays so until a UI row builds it;
//! the correction is applied in that vector rather than left standing.
//!
//! ## The judgment
//!
//! ```text
//!   Γ ⊢ w ok  ⟺  inputs(w) ⊆ dim(S)  ∧  emits(w) ⊆ T
//! ```
//!
//! `dim(S)` is the definition's declared [`Schema`], and `T` its operator
//! allow-list — both read from [`Definition`], never redefined. `inputs` are
//! parsed with [`crate::predicate::parse_state_path`], the same grammar the
//! predicate and transition layers use, because a second wildcard path grammar
//! inside one crate is exactly the divergence the Constraint article records
//! about the reference's two evaluators.
//!
//! ## ★★★ The check is ON the load path, structurally
//!
//! A [`WidgetDecl`] is what an author writes; a [`LoadedWidget`] is what a
//! surface may draw. `LoadedWidget` has **private fields and no public
//! constructor**, and the only way to obtain one is [`WidgetSet::load`], which
//! type-checks. So *a widget that does not typecheck cannot be rendered* is a
//! fact about the types rather than a rule someone has to remember to apply —
//! the same discipline as `Capability`, `ApprovalToken` and `VisualSpec`.
//!
//! Off-the-load-path — *validate if someone calls it* — is precisely the R2 #25
//! defect this row exists to fix, and it is the shape `min_privilege`, `Skin`
//! and `colour_rule` all had before their rows closed.
//!
//! ## ★★ Two containments, both enforced, at different times
//!
//! - **At load:** `emits(w) ⊆ T`. A widget may not even name an operator this
//!   definition does not permit.
//! - **At emit:** the operator must be in **this widget's own** declared
//!   `emits`. A loaded widget cannot invoke something it never declared, even
//!   though the definition permits it.
//!
//! Nested, so the widget's authority is bounded by its own declaration *and*
//! by the definition — the same shape IMM-10 gives a learned rule, where the
//! ingress bound and the rule's own operator are separate containments.
//!
//! ## ★★ All-or-nothing, and every error at once
//!
//! [`WidgetSet::load`] reports **every** failing widget rather than the first,
//! and refuses the whole set if any fails. Two deliberate positions:
//!
//! - **Every error**, because fixing a surface one message at a time is the
//!   experience `editing::typecheck`'s *well-typedness is a property of the
//!   whole definition* already avoids.
//! - **All-or-nothing**, because a **partial load is silent degradation**: the
//!   widget simply is not there, which is indistinguishable from one that
//!   decided it had nothing to show. A surface that fails loudly can be fixed;
//!   one that quietly renders four cards instead of five cannot.
//!
//! ## Honest scope
//!
//! This is the **typed load path and the check on it**. `render` is an opaque
//! tag the core does not interpret — the same encoder/renderer line MON-7
//! draws, and for the same reason: this core has no display. Binding tables,
//! `compose(r)`, the knapsack and the drawing itself are later UI rows.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::curated::BindingKey;
use crate::editing::Definition;
use crate::operator::Registry;
use crate::predicate::parse_state_path;

/// `w = ⟨inputs, render, emits⟩` — **as declared**. Untrusted until checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetDecl {
    pub id: String,
    /// State paths this widget reads. Checked against `dim(S)`.
    pub inputs: Vec<String>,
    /// An opaque tag naming what the surface should draw.
    ///
    /// ★ Deliberately not typed here. This core has no display, so inventing a
    /// vocabulary of card/ring/feed would be the core acquiring an opinion
    /// about rendering — the same line MON-7 draws between an encoder and a
    /// renderer.
    pub render: String,
    /// Operators this widget may invoke. Checked against `T`.
    pub emits: Vec<String>,
    /// β's key — `EventClass ∪ Unit` (UI-1).
    ///
    /// ★ A sum type, so a widget with **no** binding is unrepresentable. The
    /// reference has to reject that at runtime (*"must declare either
    /// event_class or unit=true"*); here there is nothing to reject.
    pub binding: BindingKey,
}

impl WidgetDecl {
    pub fn new(id: impl Into<String>, render: impl Into<String>) -> Self {
        WidgetDecl {
            id: id.into(),
            inputs: Vec::new(),
            render: render.into(),
            emits: Vec::new(),
            // Unit-bound unless a class is named: always eligible is the
            // conservative default, since an unbound widget would be
            // permanently invisible and nobody would know why.
            binding: BindingKey::Unit,
        }
    }

    /// Bind to an event class — eligible only when it appears in the log.
    pub fn bound_to(mut self, event_class: &str) -> Self {
        self.binding = BindingKey::event(event_class);
        self
    }

    /// Bind to `Unit` — always eligible. Explicit for readability; it is
    /// already the default.
    pub fn unit(mut self) -> Self {
        self.binding = BindingKey::Unit;
        self
    }

    pub fn reading(mut self, path: &str) -> Self {
        self.inputs.push(path.to_string());
        self
    }

    pub fn emitting(mut self, operator: &str) -> Self {
        self.emits.push(operator.to_string());
        self
    }
}

/// Why a widget did not load. Every variant names the widget and the offender:
/// a refusal that cannot say which is an alarm, not a diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// An input is not a path at all.
    UnparseableInput { widget: String, path: String, reason: String },
    /// `input ∉ dim(S)` — the definition declares no such dimension.
    UndeclaredInput { widget: String, path: String },
    /// `emit ∉ T` — this definition does not permit that operator.
    NotPermitted { widget: String, operator: String },
    /// The operator is not registered at all. ★ Kept distinct from
    /// `NotPermitted` for the same reason EVT-15 splits `Refused` from
    /// `NotPermitted`: *this sustain does not allow it* and *it does not exist*
    /// send an author to different repairs.
    NotRegistered { widget: String, operator: String },
    /// Two widgets share an id, so a later reference would be ambiguous.
    DuplicateId { widget: String },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::UnparseableInput { widget, path, reason } => {
                write!(f, "widget '{widget}': input '{path}' is not a state path: {reason}")
            }
            LoadError::UndeclaredInput { widget, path } => write!(
                f,
                "widget '{widget}': input '{path}' is not a declared dimension of this definition"
            ),
            LoadError::NotPermitted { widget, operator } => write!(
                f,
                "widget '{widget}': this definition does not permit operator '{operator}'"
            ),
            LoadError::NotRegistered { widget, operator } => {
                write!(f, "widget '{widget}': operator '{operator}' is not registered")
            }
            LoadError::DuplicateId { widget } => {
                write!(f, "two widgets share the id '{widget}'")
            }
        }
    }
}

/// Why an emission was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitError {
    /// ★ The widget loaded, and its `emits` were checked against `T` — but this
    /// operator is not among **its own** declared ones. The inner containment.
    NotDeclared { widget: String, operator: String },
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitError::NotDeclared { widget, operator } => write!(
                f,
                "widget '{widget}' did not declare '{operator}' among its emits"
            ),
        }
    }
}

/// A widget that **has** type-checked.
///
/// ★★ Private fields, no public constructor. The only way to hold one is
/// [`WidgetSet::load`], so the render path cannot be handed an unchecked
/// declaration — the defect is unspellable rather than merely refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedWidget {
    decl: WidgetDecl,
}

impl LoadedWidget {
    pub fn id(&self) -> &str {
        &self.decl.id
    }

    /// The opaque render tag, for whatever draws it.
    pub fn render(&self) -> &str {
        &self.decl.render
    }

    /// The state paths this widget reads — every one a declared dimension,
    /// because it loaded.
    pub fn inputs(&self) -> &[String] {
        &self.decl.inputs
    }

    /// The operators it may invoke — every one permitted by the definition,
    /// because it loaded.
    pub fn emits(&self) -> &[String] {
        &self.decl.emits
    }

    /// β's key for this widget (UI-1).
    pub fn binding(&self) -> &BindingKey {
        &self.decl.binding
    }

    /// Produce an emission. Refuses an operator this widget did not declare.
    pub fn emit(
        &self,
        operator: &str,
        params: Map<String, Value>,
    ) -> Result<WidgetEmission, EmitError> {
        if !self.decl.emits.iter().any(|e| e == operator) {
            return Err(EmitError::NotDeclared {
                widget: self.decl.id.clone(),
                operator: operator.to_string(),
            });
        }
        Ok(WidgetEmission {
            widget_id: self.decl.id.clone(),
            operator: operator.to_string(),
            params,
        })
    }
}

/// What a widget asks the host to run.
///
/// ★ No public constructor either: the only source is [`LoadedWidget::emit`],
/// so an emission naming an operator no widget declared cannot be forged and
/// handed to the gate.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetEmission {
    widget_id: String,
    operator: String,
    params: Map<String, Value>,
}

impl WidgetEmission {
    pub fn widget_id(&self) -> &str {
        &self.widget_id
    }

    pub fn operator(&self) -> &str {
        &self.operator
    }

    pub fn params(&self) -> &Map<String, Value> {
        &self.params
    }
}

/// Every widget a surface may draw, and the definition they were checked
/// against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetSet {
    widgets: BTreeMap<String, LoadedWidget>,
    /// ★★ **The order the household DECLARED them in** (UI-13).
    ///
    /// The map is keyed by id for lookup, and iterating a `BTreeMap` is
    /// alphabetical — which would make **the widget's own name** the residual
    /// ordering lever wherever nothing else separates two widgets. A widget
    /// naming itself `aaa_spending` to sit above `zzz_spending` is the same
    /// Goodhart move as staging its own brightness, so the residual order is
    /// kept as the **host's** declaration order instead: the household's lever,
    /// never the widget's.
    order: Vec<String>,
    /// A cheap structural key for the definition this set was checked against.
    checked_against: DefinitionKey,
}

/// The parts of a definition a widget check depends on, as a comparable key.
///
/// ★ Only `dim(S)` and `T`: an invariant changing does not invalidate a widget,
/// so a key over the whole definition would report staleness that is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionKey {
    dimensions: BTreeSet<String>,
    operators: BTreeSet<String>,
}

impl DefinitionKey {
    fn of(definition: &Definition) -> Self {
        DefinitionKey {
            dimensions: definition.schema.top_level().map(str::to_string).collect(),
            operators: definition.operators.iter().cloned().collect(),
        }
    }
}

impl WidgetSet {
    /// `Γ ⊢ w ok` for every widget, **on the load path**.
    ///
    /// All-or-nothing, reporting every error — see the module header on why a
    /// partial load would be silent degradation.
    pub fn load(
        decls: Vec<WidgetDecl>,
        definition: &Definition,
        registry: &Registry,
    ) -> Result<WidgetSet, Vec<LoadError>> {
        let mut errors = Vec::new();
        let mut widgets: BTreeMap<String, LoadedWidget> = BTreeMap::new();
        let mut order: Vec<String> = Vec::new();
        let permitted: BTreeSet<&str> = definition.operators.iter().map(String::as_str).collect();

        for decl in decls {
            // ── inputs ⊆ dim(S) ──────────────────────────────────────────────
            for path in &decl.inputs {
                match parse_state_path(path) {
                    Err(reason) => errors.push(LoadError::UnparseableInput {
                        widget: decl.id.clone(),
                        path: path.clone(),
                        reason,
                    }),
                    Ok(segments) => {
                        if !definition.schema.declares(&segments) {
                            errors.push(LoadError::UndeclaredInput {
                                widget: decl.id.clone(),
                                path: path.clone(),
                            });
                        }
                    }
                }
            }

            // ── emits ⊆ T ────────────────────────────────────────────────────
            for operator in &decl.emits {
                if !permitted.contains(operator.as_str()) {
                    errors.push(LoadError::NotPermitted {
                        widget: decl.id.clone(),
                        operator: operator.clone(),
                    });
                }
                if registry.get(operator).is_none() {
                    errors.push(LoadError::NotRegistered {
                        widget: decl.id.clone(),
                        operator: operator.clone(),
                    });
                }
            }

            if widgets.contains_key(&decl.id) {
                errors.push(LoadError::DuplicateId { widget: decl.id.clone() });
                continue;
            }
            order.push(decl.id.clone());
            widgets.insert(decl.id.clone(), LoadedWidget { decl });
        }

        if errors.is_empty() {
            Ok(WidgetSet { widgets, order, checked_against: DefinitionKey::of(definition) })
        } else {
            Err(errors)
        }
    }

    pub fn get(&self, id: &str) -> Option<&LoadedWidget> {
        self.widgets.get(id)
    }

    /// In **declaration order** — see [`WidgetSet::order`]'s reasoning.
    pub fn iter(&self) -> impl Iterator<Item = &LoadedWidget> {
        self.order.iter().filter_map(|id| self.widgets.get(id))
    }

    pub fn len(&self) -> usize {
        self.widgets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty()
    }

    /// ★ Is this set still checked against the definition in force?
    ///
    /// A definition edit (EDIT-13's `put`, or any typed `Edit`) can retire a
    /// dimension or an operator a loaded widget depends on. Nothing here
    /// re-checks automatically — the host reloads — but a set that has gone
    /// stale can at least **say so** rather than rendering against a definition
    /// that no longer holds.
    pub fn is_current_for(&self, definition: &Definition) -> bool {
        self.checked_against == DefinitionKey::of(definition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DimType, Schema};
    use serde_json::json;

    fn definition() -> Definition {
        Definition::new(
            Schema::new()
                .declare("finances", DimType::Any)
                .declare("roster", DimType::List { item: Box::new(DimType::Text) }),
        )
        .with_operator("budget.record_income")
        .with_operator("budget.allocate")
    }

    fn valid() -> WidgetDecl {
        WidgetDecl::new("pocket_ring", "ring")
            .reading("finances")
            .emitting("budget.allocate")
    }

    // ── the load path ────────────────────────────────────────────────────────

    #[test]
    fn a_valid_widget_loads() {
        let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
        assert_eq!(set.len(), 1);
        let w = set.get("pocket_ring").unwrap();
        assert_eq!(w.render(), "ring");
        assert_eq!(w.inputs(), ["finances".to_string()]);
    }

    #[test]
    fn a_made_up_input_is_rejected_at_load() {
        let bad = WidgetDecl::new("bad", "card").reading("finaces.liquid");
        let errors =
            WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
        assert_eq!(
            errors,
            vec![LoadError::UndeclaredInput {
                widget: "bad".into(),
                path: "finaces.liquid".into()
            }]
        );
    }

    #[test]
    fn an_unpermitted_emit_is_rejected_at_load() {
        // `budget.spend` IS registered — the definition just does not list it.
        // So exactly one error fires, and it is the right one.
        let bad = WidgetDecl::new("bad", "card").emitting("budget.spend");
        let errors =
            WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
        assert_eq!(
            errors,
            vec![LoadError::NotPermitted {
                widget: "bad".into(),
                operator: "budget.spend".into()
            }]
        );
    }

    #[test]
    fn an_unregistered_emit_is_a_different_error_from_an_unpermitted_one() {
        // ★ `budget.invented` is neither permitted nor registered, so BOTH
        // fire — and they are distinct findings, exactly as EVT-15 splits
        // `Refused` from `NotPermitted`.
        let d = definition().with_operator("budget.invented");
        let bad = WidgetDecl::new("bad", "card").emitting("budget.invented");
        let errors = WidgetSet::load(vec![bad], &d, &Registry::default()).unwrap_err();
        assert!(errors.contains(&LoadError::NotRegistered {
            widget: "bad".into(),
            operator: "budget.invented".into()
        }));
        assert!(!errors.iter().any(|e| matches!(e, LoadError::NotPermitted { .. })));
    }

    #[test]
    fn an_unparseable_input_is_reported_as_such() {
        let bad = WidgetDecl::new("bad", "card").reading("");
        let errors =
            WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
        assert!(matches!(errors[0], LoadError::UnparseableInput { .. }));
    }

    #[test]
    fn every_error_is_reported_not_only_the_first() {
        let bad = WidgetDecl::new("bad", "card")
            .reading("nope")
            .reading("alsonope")
            .emitting("budget.spend");
        let errors =
            WidgetSet::load(vec![bad], &definition(), &Registry::default()).unwrap_err();
        assert_eq!(errors.len(), 3, "fixing a surface one message at a time is the failure mode");
    }

    #[test]
    fn one_bad_widget_refuses_the_whole_set() {
        // ★★ A partial load would be silent degradation: the widget simply is
        // not there, which is indistinguishable from one that had nothing to
        // show.
        let decls = vec![valid(), WidgetDecl::new("bad", "card").reading("nope")];
        assert!(WidgetSet::load(decls, &definition(), &Registry::default()).is_err());
    }

    #[test]
    fn a_duplicate_id_is_refused() {
        let decls = vec![valid(), valid()];
        let errors =
            WidgetSet::load(decls, &definition(), &Registry::default()).unwrap_err();
        assert!(errors.contains(&LoadError::DuplicateId { widget: "pocket_ring".into() }));
    }

    #[test]
    fn a_wildcard_input_is_accepted_where_the_schema_declares_the_map() {
        let d = Definition::new(Schema::new().declare(
            "pockets",
            DimType::Map { value: Box::new(DimType::Number { lo: None, hi: None }) },
        ));
        let w = WidgetDecl::new("ring", "ring").reading("pockets[*]");
        assert!(WidgetSet::load(vec![w], &d, &Registry::default()).is_ok());
    }

    // ── the two containments ─────────────────────────────────────────────────

    #[test]
    fn a_loaded_widget_may_emit_what_it_declared() {
        let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
        let emission = set
            .get("pocket_ring")
            .unwrap()
            .emit("budget.allocate", Map::new())
            .unwrap();
        assert_eq!(emission.operator(), "budget.allocate");
        assert_eq!(emission.widget_id(), "pocket_ring");
    }

    #[test]
    fn a_loaded_widget_may_not_emit_what_it_did_not_declare() {
        // ★★ The inner containment. `budget.record_income` IS permitted by the
        // definition — this widget just never asked for it.
        let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
        assert_eq!(
            set.get("pocket_ring").unwrap().emit("budget.record_income", Map::new()),
            Err(EmitError::NotDeclared {
                widget: "pocket_ring".into(),
                operator: "budget.record_income".into()
            })
        );
    }

    #[test]
    fn params_travel_with_the_emission() {
        let set = WidgetSet::load(vec![valid()], &definition(), &Registry::default()).unwrap();
        let mut params = Map::new();
        params.insert("amount".into(), json!(10.0));
        let emission = set.get("pocket_ring").unwrap().emit("budget.allocate", params).unwrap();
        assert_eq!(emission.params()["amount"], json!(10.0));
    }

    // ── staleness ────────────────────────────────────────────────────────────

    #[test]
    fn retiring_a_dimension_makes_a_loaded_set_stale() {
        let d = definition();
        let set = WidgetSet::load(vec![valid()], &d, &Registry::default()).unwrap();
        assert!(set.is_current_for(&d));

        let edited = Definition {
            schema: d.schema.clone().without("finances"),
            ..d.clone()
        };
        assert!(!set.is_current_for(&edited));
    }

    #[test]
    fn changing_an_invariant_does_not_make_a_set_stale() {
        // ★ The key covers `dim(S)` and `T` only, because a widget check
        // depends on nothing else — a wider key would report staleness that is
        // not there and send a host reloading for no reason.
        let d = definition();
        let set = WidgetSet::load(vec![valid()], &d, &Registry::default()).unwrap();
        assert!(set.is_current_for(&d.clone().with_invariant("new", "finances >= 0")));
    }
}
