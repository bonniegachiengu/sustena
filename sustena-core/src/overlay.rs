//! **`ρ` — the typed overlay** (DSL · GENOME §VIII).
//!
//! ```text
//!   ρ : Path ⇀ Value        well-typed ⟺ ∀(p ↦ v) ∈ ρ:  Γ ⊢ p : τ  ∧  Γ ⊢ v : τ
//!   ρ₁ ⊕ ρ₂                 right-biased union — a monoid, identity ∅
//! ```
//!
//! ★★★ **What an overlay is FOR: you hand a person the overlay, never the
//! definition.** A definition says what a Sustain *is* — its dimensions, its
//! laws, what may run. An overlay says only what some of its values *are*. Give
//! somebody a definition to edit and every edit is potentially a change of
//! kind: a renamed dimension, a dropped invariant, a Sustain that is no longer
//! the thing it was. Give them a well-typed overlay and the worst they can do
//! is set a declared dimension to a value of its own type.
//!
//! That is why the Curated UI can offer an edit at all. A surface that had to
//! hand out the definition would have to decide, per field, whether this
//! particular person may change this particular thing — a judgment made in the
//! UI, where it cannot be checked. Here it is a **type**, and the check runs
//! once before anybody sees a form.
//!
//! ## Three properties, and why each is load-bearing
//!
//! ★★★ **An overlay cannot introduce a dimension.** Every path must already be
//! declared. This is organisational closure (`Sustain · CELL`) arriving one
//! layer earlier: the gate would refuse a shape change anyway, but an overlay
//! that could *propose* one would mean a person filled in a form and was told
//! no at the end. Refusing it as ill-typed says so before the form exists.
//!
//! ★★★ **Overlays compose, and the composition is associative.** `ρ₁ ⊕ ρ₂ ⊕ ρ₃`
//! means the same thing however it is bracketed, so a surface may build an
//! overlay incrementally — a field at a time, a section at a time — and hand
//! the accumulated one to the engine. Without associativity the order of
//! assembly would be part of the answer, and a form filled top-to-bottom would
//! differ from the same form filled bottom-to-top.
//!
//! ★★★ **Right-biased, because the later answer is the person's latest one.**
//! Somebody who types a value, thinks again and types another has changed their
//! mind; the second is what they meant. Left-bias would make the first edit of
//! a session unchangeable, which is not a rule anybody would choose out loud.
//!
//! ## What an overlay is not
//!
//! ★★ It is **not** a migration. `μ` transforms a state whose *shape* changed;
//! `ρ` sets values inside a shape that did not. Conflating them is how a
//! "small edit" becomes a stranded history — see [`crate::stranding`].
//!
//! ★★ It is **not** a way past the gate. Applying an overlay produces a
//! candidate state; whether that state may be *committed* is still the gate's
//! question, and an overlay that sets a balance below zero is well-typed and
//! still refused. Type-correct and permitted are different words.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::schema::{conforms, DimType, Schema, TypeError};
use crate::state::State;

/// `ρ` — a partial map from declared paths to values.
///
/// ★★ Ordered by path rather than by insertion, so two overlays built the same
/// way from different directions are the same value. Equality that depended on
/// assembly order would make the monoid laws untestable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overlay {
    entries: BTreeMap<String, Value>,
}

impl Overlay {
    /// `∅` — the identity of `⊕`.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn set(mut self, path: &str, value: Value) -> Self {
        self.entries.insert(path.to_string(), value);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.entries.keys()
    }

    pub fn get(&self, path: &str) -> Option<&Value> {
        self.entries.get(path)
    }

    /// `ρ₁ ⊕ ρ₂` — right-biased union.
    ///
    /// ★★★ Associative and unital, which is what lets a surface accumulate an
    /// overlay one field at a time and get the same answer as one built whole.
    pub fn compose(mut self, later: Overlay) -> Overlay {
        for (path, value) in later.entries {
            self.entries.insert(path, value);
        }
        self
    }

    /// **Well-typed ⟺ every path is a declared dimension of that type.**
    ///
    /// ★★ Two distinct findings, kept distinct: a path the schema does not
    /// declare, and a value that does not fit the type it does declare. They
    /// are different mistakes and a person fixes them differently.
    pub fn typecheck(&self, schema: &Schema) -> Result<(), Vec<TypeError>> {
        let mut errors = Vec::new();
        for (path, value) in &self.entries {
            let Some(ty) = resolve(schema, path) else {
                errors.push(TypeError {
                    path: path.clone(),
                    detail: "is not a dimension this Sustain declares".into(),
                });
                continue;
            };
            if let Some(detail) = conforms(value, ty) {
                errors.push(TypeError { path: path.clone(), detail });
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    /// Apply to a state, producing a candidate.
    ///
    /// ★★★ Type-checks FIRST and refuses as a whole. A partially-applied
    /// overlay is the worst outcome available: the person is told something
    /// went wrong, and some of what they typed is now in their state anyway.
    ///
    /// ★★ The result is a CANDIDATE. Whether it may be committed is the gate's
    /// question — an overlay that puts a balance below zero is perfectly
    /// well-typed and still refused.
    pub fn apply(&self, state: &Value, schema: &Schema) -> Result<Value, Vec<TypeError>> {
        self.typecheck(schema)?;
        let mut working = State::new(state.clone());
        let mut errors = Vec::new();
        for (path, value) in &self.entries {
            if let Err(e) = working.set(path, value.clone()) {
                errors.push(TypeError { path: path.clone(), detail: e.to_string() });
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        // ★★★ **Per-path typing is not enough, and a test found it.**
        //
        //     Every path can be a declared dimension of the right type and the
        //     RESULT can still be malformed: setting
        //     `finances.pockets.holiday.label` alone invents a pocket that has
        //     a name and no allocation. Each path passed; the pocket is half a
        //     pocket. That is the shape changing after all, one key at a time,
        //     which is exactly what the Curated UI leans on this not to allow.
        //
        //     ★★ So the candidate is validated as a whole. The safety property
        //     the surface relies on is about the RESULT, so that is where it is
        //     checked — the per-path judgment stays because it gives the better
        //     message when it is the one that fails.
        let candidate = working.snapshot();
        let broken = crate::schema::validate(&candidate, schema);
        if !broken.is_empty() {
            return Err(broken
                .into_iter()
                .map(|e| TypeError {
                    path: e.path,
                    detail: format!("{} — the overlay would leave this incomplete", e.detail),
                })
                .collect());
        }
        Ok(candidate)
    }
}

/// The declared type at a dot-path.
///
/// ★★ Goes through the schema's own resolver via a parsed path, so an overlay
/// resolves a name exactly the way an invariant does. A second spelling of
/// "what does this path mean" is how two parts of one language drift.
fn resolve<'a>(schema: &'a Schema, path: &str) -> Option<&'a DimType> {
    let segments = crate::predicate::parse_path(path).ok()?;
    schema.resolve(&segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn number() -> DimType {
        DimType::Number { lo: None, hi: None }
    }

    fn household() -> Schema {
        Schema::new()
            .declare(
                "finances",
                DimType::Record {
                    fields: [
                        (
                            "liquid".to_string(),
                            DimType::Record {
                                fields: [("balance".to_string(), number())].into_iter().collect(),
                            },
                        ),
                        (
                            "pockets".to_string(),
                            DimType::Map {
                                value: Box::new(DimType::Record {
                                    fields: [
                                        ("allocated".to_string(), number()),
                                        ("label".to_string(), DimType::Text),
                                    ]
                                    .into_iter()
                                    .collect(),
                                }),
                            },
                        ),
                    ]
                    .into_iter()
                    .collect(),
                },
            )
            .declare("notes", DimType::Text)
    }

    fn state() -> Value {
        json!({
            "finances": {
                "liquid": {"balance": 1000.0},
                "pockets": {"food": {"allocated": 500.0, "label": "Food"}}
            },
            "notes": ""
        })
    }

    #[test]
    fn an_overlay_that_sets_a_declared_dimension_is_well_typed() {
        let rho = Overlay::empty().set("notes", json!("shopping day"));
        assert!(rho.typecheck(&household()).is_ok());
    }

    #[test]
    fn an_overlay_cannot_introduce_a_dimension() {
        // ★★★ Organisational closure, one layer earlier. The gate would refuse
        //     a shape change anyway — but an overlay that could PROPOSE one
        //     means a person fills in a form and is told no at the end.
        let rho = Overlay::empty().set("secrets", json!("x"));
        let errors = rho.typecheck(&household()).expect_err("must refuse");
        assert!(errors[0].detail.contains("does not declare") || errors[0]
            .detail
            .contains("not a dimension"), "{:?}", errors[0]);
    }

    #[test]
    fn an_overlay_cannot_change_what_kind_a_dimension_is() {
        let rho = Overlay::empty().set("finances.liquid.balance", json!("plenty"));
        let errors = rho.typecheck(&household()).expect_err("must refuse");
        assert_eq!(errors[0].path, "finances.liquid.balance");
    }

    #[test]
    fn a_path_and_a_value_are_two_different_mistakes() {
        // ★★ A person fixes them differently: one is a wrong name, the other a
        //    wrong value. Reporting both as "invalid" would help with neither.
        let rho = Overlay::empty()
            .set("nowhere", json!(1))
            .set("finances.liquid.balance", json!("plenty"));
        let errors = rho.typecheck(&household()).expect_err("must refuse");
        assert_eq!(errors.len(), 2, "{errors:?}");
    }

    #[test]
    fn applying_a_well_typed_overlay_sets_exactly_what_it_names() {
        let rho = Overlay::empty()
            .set("notes", json!("shopping day"))
            .set("finances.pockets.food.allocated", json!(900.0));
        let after = rho.apply(&state(), &household()).expect("applies");
        assert_eq!(after.pointer("/notes"), Some(&json!("shopping day")));
        assert_eq!(after.pointer("/finances/pockets/food/allocated"), Some(&json!(900.0)));
        // And nothing else moved.
        assert_eq!(after.pointer("/finances/liquid/balance"), Some(&json!(1000.0)));
        assert_eq!(after.pointer("/finances/pockets/food/label"), Some(&json!("Food")));
    }

    #[test]
    fn a_bad_overlay_applies_nothing_at_all() {
        // ★★★ The worst available outcome is a partial application: the person
        //     is told something went wrong, and half of what they typed is in
        //     their state anyway.
        let before = state();
        let rho = Overlay::empty()
            .set("notes", json!("this one is fine"))
            .set("finances.liquid.balance", json!("this one is not"));
        assert!(rho.apply(&before, &household()).is_err());
        assert_eq!(before.pointer("/notes"), Some(&json!("")), "untouched");
    }

    #[test]
    fn an_open_map_accepts_a_key_the_schema_never_named() {
        // ★★ A pocket's NAME is a value, not a dimension. An overlay that
        //    refused a new pocket would be refusing the household's own
        //    vocabulary.
        let rho = Overlay::empty().set("finances.pockets.rent.allocated", json!(3000.0));
        assert!(rho.typecheck(&household()).is_ok());
    }

    // ── the monoid ──────────────────────────────────────────────────────────

    #[test]
    fn composing_with_nothing_changes_nothing() {
        let rho = Overlay::empty().set("notes", json!("a"));
        assert_eq!(rho.clone().compose(Overlay::empty()), rho);
        assert_eq!(Overlay::empty().compose(rho.clone()), rho);
    }

    #[test]
    fn composition_is_associative() {
        // ★★★ Why it matters: a surface accumulates an overlay a field at a
        //     time. Without this, a form filled top-to-bottom would mean
        //     something different from the same form filled bottom-to-top.
        let a = Overlay::empty().set("notes", json!("a"));
        let b = Overlay::empty().set("finances.liquid.balance", json!(1.0));
        let c = Overlay::empty().set("notes", json!("c"));

        let left = a.clone().compose(b.clone()).compose(c.clone());
        let right = a.compose(b.compose(c));
        assert_eq!(left, right);
    }

    #[test]
    fn the_later_answer_wins_because_it_is_the_persons_latest_one() {
        // ★★ Somebody who types a value, thinks again and types another has
        //    changed their mind. Left-bias would make the first edit of a
        //    session unchangeable.
        let first = Overlay::empty().set("notes", json!("first thought"));
        let second = Overlay::empty().set("notes", json!("second thought"));
        assert_eq!(
            first.compose(second).get("notes"),
            Some(&json!("second thought")),
        );
    }

    #[test]
    fn a_composed_overlay_is_well_typed_iff_both_halves_were() {
        let good = Overlay::empty().set("notes", json!("fine"));
        let bad = Overlay::empty().set("nowhere", json!(1));
        assert!(good.clone().compose(bad).typecheck(&household()).is_err());
        assert!(good.clone().compose(good).typecheck(&household()).is_ok());
    }

    #[test]
    fn a_well_typed_overlay_leaves_a_conforming_state_conforming() {
        // ★★★ The theorem the Curated UI actually relies on: hand somebody an
        //     overlay and the worst they can do is a value of the right type in
        //     a dimension that already existed. The SHAPE is not theirs to
        //     change, and that is a property rather than a promise.
        let schema = household();
        assert!(crate::schema::validate(&state(), &schema).is_empty(), "the fixture conforms");

        for (path, value) in [
            ("notes", json!("anything at all")),
            ("finances.liquid.balance", json!(-999.0)),
            ("finances.pockets.food.allocated", json!(0.0)),
        ] {
            let rho = Overlay::empty().set(path, value);
            let after = rho.apply(&state(), &schema).expect("well-typed");
            assert!(
                crate::schema::validate(&after, &schema).is_empty(),
                "{path} broke the shape",
            );
        }
    }

    #[test]
    fn an_overlay_that_would_leave_half_a_pocket_is_refused() {
        // ★★★ The finding this slice turned up, and it is the interesting one:
        //     **per-path typing is not enough.** Every path here is a declared
        //     dimension of exactly the right type, and the result is still
        //     malformed — a pocket with a name and no allocation. The shape
        //     changes anyway, one key at a time, which is precisely what the
        //     Curated UI leans on an overlay not to allow.
        let rho = Overlay::empty().set("finances.pockets.holiday.label", json!("Holiday"));
        assert!(rho.typecheck(&household()).is_ok(), "every path types");
        let errors = rho.apply(&state(), &household()).expect_err("and it is still refused");
        assert!(errors[0].detail.contains("incomplete"), "{:?}", errors[0]);
    }

    #[test]
    fn a_whole_new_pocket_is_accepted() {
        // ★★ The other half: adding a complete one is a value, not a shape
        //    change, and refusing it would refuse the household's own
        //    vocabulary.
        let rho = Overlay::empty()
            .set("finances.pockets.holiday.label", json!("Holiday"))
            .set("finances.pockets.holiday.allocated", json!(0.0));
        let after = rho.apply(&state(), &household()).expect("applies");
        assert_eq!(after.pointer("/finances/pockets/holiday/allocated"), Some(&json!(0.0)));
    }

    #[test]
    fn type_correct_is_not_the_same_word_as_permitted() {
        // ★★★ An overlay that puts a balance below zero is perfectly
        //     well-typed. Whether it may be COMMITTED is the gate's question,
        //     and an overlay is not a way past it.
        let rho = Overlay::empty().set("finances.liquid.balance", json!(-500.0));
        assert!(rho.typecheck(&household()).is_ok(), "the type says yes");
        let candidate = rho.apply(&state(), &household()).expect("applies");
        let region = crate::region::Region::new()
            .bounding(crate::region::Interval::at_least("finances.liquid.balance", 0.0));
        assert!(
            region.distance(&candidate).expect("distance").weighted > 0.0,
            "and the law still says no",
        );
    }
}
