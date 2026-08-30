//! **Two surfaces, one truth** (Capstone · §VIII.1).
//!
//! ```text
//!   Mycelium  =  compose(r) at a large budget
//!   Orchie    =  compose(r) at K ≈ 4
//! ```
//!
//! ★★★ **The claim is that they differ ONLY in budget, and that is checkable.**
//! Two surfaces built from one object at two budgets must nest: whatever the
//! phone shows, the laptop shows too. If the small view ever contains something
//! the large one does not, they are not projections of one truth — they are two
//! answers, and a person moving between them would be reading two systems that
//! happen to share a login.
//!
//! ★★★ **That failure would be invisible from either surface alone.** Each looks
//! internally consistent; only comparing them at two budgets can catch it, which
//! is why this is a check and not a comment. [`nests`] is the check, and
//! [`Divergence`] names what escaped.
//!
//! ## What is allowed to differ
//!
//! ★★ **The laptop showing more is the whole point** — that is what a larger
//! budget *is*. Nesting is one-directional: `selected(small) ⊆ selected(large)`,
//! and nothing here demands the reverse.
//!
//! ★★ **The persistent tier is exempt from the budget and therefore from this
//! check.** A persistent panel never competed, so it is not evidence about how
//! competition resolved. Including it would make the nesting hold for a reason
//! that has nothing to do with the claim.

use std::collections::BTreeSet;

use crate::curated::View;

/// A widget the small surface showed and the large one did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub widget: String,
    /// What the small surface scored it, so the disagreement is inspectable
    /// rather than merely reported.
    pub small_rank: usize,
}

/// Whether two views of one object nest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nesting {
    /// Everything the smaller budget showed, the larger one showed too.
    Holds { small: usize, large: usize },
    /// ★★★ It did not. **They are not two views of one truth.**
    Broken { escaped: Vec<Divergence> },
}

impl Nesting {
    pub fn holds(&self) -> bool {
        matches!(self, Self::Holds { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Holds { small, large } => format!(
                "the phone's {small} are among the laptop's {large} — one object, two budgets"
            ),
            Self::Broken { escaped } => format!(
                "{} widget(s) appear on the small surface and not the large one, so these are \
                 two answers rather than two views: {}",
                escaped.len(),
                escaped.iter().map(|d| d.widget.as_str()).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

fn selected_ids(view: &View) -> Vec<&str> {
    view.selected.iter().map(|c| c.id.as_str()).collect()
}

/// **Do the two surfaces nest?**
///
/// ★★ `small` and `large` must be composed from the *same* state, events and
/// policy, differing only in `Request::budget`. Comparing views of different
/// moments would report a divergence that is only the world having moved.
pub fn nests(small: &View, large: &View) -> Nesting {
    let big: BTreeSet<&str> = selected_ids(large).into_iter().collect();
    let escaped: Vec<Divergence> = selected_ids(small)
        .into_iter()
        .enumerate()
        .filter(|(_, id)| !big.contains(id))
        .map(|(rank, id)| Divergence { widget: id.to_string(), small_rank: rank })
        .collect();
    if escaped.is_empty() {
        Nesting::Holds { small: small.selected.len(), large: large.selected.len() }
    } else {
        Nesting::Broken { escaped }
    }
}

/// What the larger surface shows that the smaller one had no room for.
///
/// ★★★ Not a defect — it is what the extra budget bought, and naming it is how
/// a person on the phone knows what they are not seeing. "Three other things
/// stayed quiet" is only true if somebody can say which three.
pub fn only_on_the_larger<'a>(small: &View, large: &'a View) -> Vec<&'a str> {
    let seen: BTreeSet<&str> = selected_ids(small).into_iter().collect();
    selected_ids(large).into_iter().filter(|id| !seen.contains(id)).collect()
}

/// Did the extra budget actually buy anything?
///
/// ★★ A larger surface that shows the same four things is not a larger view of
/// the truth; it is the same view with more room, and a person deciding whether
/// to open the laptop deserves to know which it is.
pub fn extra_budget_bought_something(small: &View, large: &View) -> bool {
    !only_on_the_larger(small, large).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curated::{
        compose_view, Request, SaliencePolicy, ALPHA_URGENCY, DEFAULT_BUDGET, LAMBDA_RELEVANCE,
    };
    use crate::editing::Definition;
    use crate::event::{CausalStamp, Event, Provenance};
    use crate::operator::Registry;
    use crate::region::{Interval, Region};
    use crate::schema::{DimType, Schema};
    use crate::widget::{WidgetDecl, WidgetSet};
    use serde_json::json;

    fn policy() -> SaliencePolicy {
        SaliencePolicy::declared(ALPHA_URGENCY, LAMBDA_RELEVANCE).expect("declared")
    }

    fn definition() -> Definition {
        Definition::new(Schema::new().declare("balance", DimType::Number { lo: None, hi: None }))
    }

    fn region() -> Region {
        Region::new().bounding(Interval::new("balance", 0.0, 100.0)).weighing("balance", 1.0)
    }

    fn state() -> serde_json::Value {
        json!({ "balance": 120.0 })
    }

    fn ev(id: &str) -> Event {
        Event {
            id: id.into(),
            name: "event.finances.pocket_spent".into(),
            t_event: 1_000,
            t_ingest: None,
            provenance: Provenance::Observed,
            source: None,
            stamp: CausalStamp::new("test"),
            causes: vec![],
            mutations: vec![],
            payload: None,
        }
    }

    /// More widgets than any small budget can hold, all genuinely eligible.
    fn widgets() -> WidgetSet {
        let decls: Vec<WidgetDecl> = (0..8)
            .map(|i| WidgetDecl::new(format!("w{i}"), "card").unit().reading("balance"))
            .collect();
        WidgetSet::load(decls, &definition(), &Registry::default()).expect("loads")
    }

    fn view_at(budget: usize) -> View {
        let request = Request { query: None, device: "any".into(), budget };
        compose_view(&widgets(), &region(), &state(), &[ev("e1")], &request, &policy())
    }

    fn empty_view(budget: usize) -> View {
        let request = Request { query: None, device: "any".into(), budget };
        compose_view(&WidgetSet::empty(), &region(), &state(), &[ev("e1")], &request, &policy())
    }

    #[test]
    fn the_phone_shows_a_subset_of_what_the_laptop_shows() {
        // ★★★ The claim, checked. If the small view ever held something the
        //     large one did not, a person moving between surfaces would be
        //     reading two systems that share a login.
        let n = nests(&view_at(DEFAULT_BUDGET), &view_at(64));
        assert!(n.holds(), "{}", n.describe());
        assert!(n.describe().contains("one object, two budgets"));
    }

    #[test]
    fn a_divergence_would_be_reported_with_what_escaped() {
        // ★★ Constructed rather than hoped for: a check that has never been
        //    seen to fail is a check nobody knows the shape of.
        let n = nests(&view_at(DEFAULT_BUDGET), &empty_view(64));
        assert!(!n.holds());
        assert!(n.describe().contains("two answers rather than two views"));
        match n {
            Nesting::Broken { escaped } => assert!(!escaped.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_larger_budget_is_allowed_to_show_more_and_that_is_the_point() {
        // ★★ Nesting is one-directional. Demanding the reverse would demand the
        //    laptop show nothing the phone could not.
        assert!(extra_budget_bought_something(&view_at(1), &view_at(64)));
    }

    #[test]
    fn what_the_phone_is_not_showing_is_nameable() {
        // ★★★ "Three other things stayed quiet" is only true if somebody can
        //     say which three.
        let small = view_at(1);
        let large = view_at(64);
        let hidden = only_on_the_larger(&small, &large);
        assert!(!hidden.is_empty(), "{hidden:?}");
    }

    #[test]
    fn a_bigger_budget_that_buys_nothing_says_so() {
        // ★★ The same view with more room is not a larger view of the truth,
        //    and a person deciding whether to open the laptop deserves to know.
        assert!(!extra_budget_bought_something(&view_at(64), &view_at(128)));
    }

    #[test]
    fn nesting_holds_at_every_budget_between_the_two_surfaces() {
        // ★★★ Not only at the ends. If the ordering were unstable the nesting
        //     could hold at K=4 and K=64 and break in the middle, and an
        //     end-to-end check would have missed it.
        let large = view_at(64);
        for k in 1..8 {
            let n = nests(&view_at(k), &large);
            assert!(n.holds(), "budget {k}: {}", n.describe());
        }
    }
}
