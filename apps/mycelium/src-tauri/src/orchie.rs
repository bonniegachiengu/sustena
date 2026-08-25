//! **Orchie** — the phone-first curated face, assembled from real engine parts.
//!
//! Mycelium shows you a household. Orchie shows you **the few things that need
//! you**, one at a time, and nothing else. Same app, same engine, same gate —
//! a different question.
//!
//! ## ★★★ The ranking is the engine's, not this file's
//!
//! [`sustena_core::compose_view`] does the work: `β` binds widgets to what
//! happened, `score(w) = α·urgency + λ·relevance` under the household's own
//! declared policy, and a **0/1 knapsack** picks what fits the attention
//! budget. This module only supplies the four things `compose` needs — the
//! widgets, the region, the state, the recent events — and reports what came
//! back. Nothing here sorts anything.
//!
//! ## ★★★ The region is the household's OWN numbers
//!
//! Urgency is `d(s,V)`, and `V` needs intervals. V1.2 recorded that these
//! templates declare **invariants, not intervals**, and refused to invent a
//! region — *inventing one to borrow its authority would be declaring
//! thresholds nobody chose*. That refusal still stands, and this does not
//! break it: every interval below is **read off the household's own state**.
//!
//! ```text
//!   for each pocket p:   spent(p) ∈ [0, allocated(p)]
//!   liquid balance:      ∈ [0, ∞)
//! ```
//!
//! `allocated` is a limit the household **set for itself**. Reading it is not
//! declaring a threshold; it is using theirs. A pocket with nothing allocated
//! contributes no interval at all, because `[0, 0]` would score every shilling
//! spent from an unfunded pocket as maximal strain — the exact
//! zero-denominator trap the engine's own urgency notes warn about.
//!
//! ★★ And this **retires** the V1.2 caveat honestly: Mycelium's 80% line was
//! *this app's declared policy over real numbers*, needed because there was no
//! region. There is one now, so Orchie ranks on `d(s,V)` and says so.
//!
//! ## ★★★ The projection, and the finding that forced it
//!
//! `compose` attributes urgency at **root-dimension** granularity: `roots_read`
//! splits a widget's input path at the first `.`, and `urgency_of` keeps the
//! per-dimension excesses whose name that root matches. That works when a
//! dimension IS a top-level numeric field — `moisture`, `soil` — and it does
//! **not** work for a household, whose money lives at
//! `finances.pockets.food.spent`. Every widget's root is `finances`; every
//! interval's name is a path; they never match, so every card scores `0`
//! however strained the household is.
//!
//! Found by writing the test and watching it fail, not by reading the code.
//!
//! ★★ **So Orchie composes over a PROJECTION**, and says so. [`reading_of`]
//! derives a handful of top-level numbers **from the household's own state** —
//! nothing invented, every figure theirs — and `compose` reasons over that.
//! The alternative was changing `roots_read` in the core, which would move a
//! deliberate, conformance-recorded decision to suit one product's state shape.
//!
//! ```text
//!   liquid       the balance                       ∈ [0, ∞)
//!   worst_spent  the most-strained pocket's spend  ∈ [0, its own allocation]
//!   unclassified captures still needing a person   ∈ [0, 0]
//! ```
//!
//! ★ `unclassified ∈ [0,0]` is a real declaration rather than a threshold: it
//! says *anything unclassified needs you*, which is what an unclassified
//! capture means. The count is the queue's own.

use serde_json::Value;
use sustena_core::{
    compose_view, BindingKey, Interval, Region, Request, SaliencePolicy, View, WidgetDecl,
    WidgetSet,
};
use sustena_core::editing::Definition;
use sustena_core::schema::{DimType, Schema};
use sustena_core::event::Event;
use sustena_core::operator::Registry;

/// The household's declared attention budget. ★ Cowan's four chunks — the
/// engine's own default, not a number this file picked.
pub const BUDGET: usize = 4;

/// The widgets Orchie offers a household.
///
/// ★★ Declared here, in the host, for the same reason templates are: which
/// cards a product offers is a product decision. Every one is checked against
/// the Sustain's own `dim(S)` and `T` by [`WidgetSet::load`], so a card reading
/// a dimension the household does not have is a **load-time** failure rather
/// than an empty box at render time.
pub fn widget_declarations() -> Vec<WidgetDecl> {
    vec![
        // ★ Reads the strained-pocket reading. Withdraws when the household has
        //   no pockets — `grounded()` sees that without this file checking.
        WidgetDecl {
            id: "pocket_strain".into(),
            inputs: vec!["worst_spent".into()],
            render: "pocket_strain".into(),
            emits: vec!["budget.allocate".into()],
            binding: BindingKey::Unit,
        },
        // ★★ The captures still waiting on a person. Its own dimension, so it
        //   competes on its own urgency rather than borrowing the pockets'.
        WidgetDecl {
            id: "classify_capture".into(),
            inputs: vec!["unclassified".into()],
            render: "classify_capture".into(),
            // ★★★ `unspend` is here because a refund arrives in the same queue
            //   as a purchase. Without it the only way to answer a refund was
            //   to file it as a spend, which counts the original charge twice.
            emits: vec![
                "budget.spend".into(),
                "budget.allocate".into(),
                "budget.unspend".into(),
            ],
            binding: BindingKey::Unit,
        },
        // The calm read — the one card worth showing when nothing is wrong.
        WidgetDecl {
            id: "household_summary".into(),
            inputs: vec!["liquid".into()],
            render: "household_summary".into(),
            emits: vec![],
            binding: BindingKey::Unit,
        },
        // ★ Event-bound: only a candidate when money has actually moved. β does
        //   this, not an `if` in the host.
        WidgetDecl {
            id: "recent_spend".into(),
            inputs: vec!["worst_spent".into()],
            render: "recent_spend".into(),
            emits: vec!["budget.spend".into()],
            binding: BindingKey::event("event.finances.pocket_spent"),
        },
    ]
}

/// The dimensions the projection carries. ★ The widget check runs against
/// **this**, because a curated view reads a projection, not raw state — and a
/// check against a schema the view does not have would guarantee nothing.
pub fn view_definition() -> Definition {
    Definition::new(
        Schema::new()
            .declare("liquid", DimType::Number { lo: None, hi: None })
            .declare("worst_spent", DimType::Number { lo: None, hi: None })
            .declare("worst_pocket", DimType::Text)
            .declare("worst_allocated", DimType::Number { lo: None, hi: None })
            .declare("unclassified", DimType::Number { lo: None, hi: None }),
    )
    .with_operator("budget.allocate")
    .with_operator("budget.spend")
    // ★★ A card may only emit what the view permits, and a refund arrives in
    //    the same queue as a purchase. Without this the only answer available
    //    for one was to file it as a spend.
    .with_operator("budget.unspend")
}

/// **The projection.** Top-level readings, every one derived from the
/// household's own state and its own queue — nothing invented.
///
/// ★ `worst_*` is absent entirely when there is no funded pocket, so the strain
/// card WITHDRAWS rather than reading a fabricated zero.
pub fn reading_of(state: &Value, unclassified: usize) -> Value {
    let mut out = serde_json::Map::new();
    if let Some(b) = state.pointer("/finances/liquid/balance").and_then(Value::as_f64) {
        out.insert("liquid".into(), serde_json::json!(b));
    }
    // ★★ Omitted when there is nothing to classify -- the same discipline the
    //    `worst_*` reading follows below. A zero here would be a READING that
    //    the card would then rank at zero urgency and still render, as an empty
    //    box. An absent dimension is the honest shape of "there is nothing to
    //    say", and `grounded()` turns it into a withdrawal WITH A REASON.
    if unclassified > 0 {
        out.insert("unclassified".into(), serde_json::json!(unclassified));
    }

    // The most-strained funded pocket: the largest (spent − allocated), or the
    // largest fraction when nothing is over. Its OWN allocation is the ceiling.
    let pockets = state.pointer("/finances/pockets").and_then(Value::as_object);
    let mut worst: Option<(&String, f64, f64)> = None;
    for (name, p) in pockets.into_iter().flatten() {
        let allocated = p.get("allocated").and_then(Value::as_f64).unwrap_or(0.0);
        let spent = p.get("spent").and_then(Value::as_f64).unwrap_or(0.0);
        if allocated <= 0.0 {
            continue; // unfunded: no ceiling of its own to be measured against
        }
        let strain = spent / allocated;
        if worst.map(|(_, s, a)| strain > s / a).unwrap_or(true) {
            worst = Some((name, spent, allocated));
        }
    }
    if let Some((name, spent, allocated)) = worst {
        out.insert("worst_pocket".into(), serde_json::json!(name));
        out.insert("worst_spent".into(), serde_json::json!(spent));
        out.insert("worst_allocated".into(), serde_json::json!(allocated));
    }
    Value::Object(out)
}

/// `V` over the projection, **read off the household's own numbers**. See the
/// module docs for why this does not break V1.2's refusal to invent a region.
pub fn region_from(reading: &Value) -> Region {
    let mut region = Region::new();

    // A floor everyone has: money does not go negative.
    if reading.get("liquid").and_then(Value::as_f64).is_some() {
        region = region.bounding(Interval::at_least("liquid", 0.0)).weighing("liquid", 1.0);
    }

    // ★★ The ceiling IS the household's own allocation for that pocket. Reading
    //    it is not declaring a threshold; it is using theirs.
    if let Some(allocated) = reading.get("worst_allocated").and_then(Value::as_f64) {
        if allocated > 0.0 {
            region = region
                .bounding(Interval::new("worst_spent", 0.0, allocated))
                .weighing("worst_spent", 1.0);
        }
    }

    // ★ Anything unclassified needs a person. Not a threshold — the meaning of
    //   the tier.
    if reading.get("unclassified").is_some() {
        region = region
            .bounding(Interval::new("unclassified", 0.0, 0.0))
            .weighing("unclassified", 1.0);
    }
    region
}

/// **The feed.** Everything `compose(r)` decided, for one household.
///
/// ★ Read-only by construction: no `Registry` reaches `compose_view`, so there
/// is nothing in here that could run an operator.
pub fn feed(
    registry: &Registry,
    state: &Value,
    unclassified: usize,
    recent: &[Event],
    query: Option<&str>,
    installed: Vec<WidgetDecl>,
) -> Result<(Value, View), Vec<String>> {
    let definition = view_definition();
    // ★★★ Installed cards face **exactly** the load gate the built-in set
    //     faces — the same `WidgetSet::load`, in the same call, against the
    //     same definition. An installed widget therefore cannot read a
    //     dimension a built-in could not, and a broken one takes the whole set
    //     down rather than degrading silently: `WidgetSet::load`'s own
    //     all-or-nothing rule, not a new policy invented for packages.
    let mut decls = widget_declarations();
    decls.extend(installed);
    let widgets = WidgetSet::load(decls, &definition, registry)
        .map_err(|errors| errors.iter().map(|e| format!("{e}")).collect::<Vec<_>>())?;

    let reading = reading_of(state, unclassified);
    let region = region_from(&reading);
    // ★★ The household's policy. `declared` refuses weights that do not make
    //    sense, so there is no silently-renormalised pair.
    let policy = SaliencePolicy::declared(0.75, 0.25).map_err(|e| vec![format!("{e}")])?;

    let mut request = Request::default().with_budget(BUDGET);
    request.query = query.map(str::to_string);

    let view = compose_view(&widgets, &region, &reading, recent, &request, &policy);
    Ok((reading, view))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state(liquid: f64, pockets: Value) -> Value {
        json!({ "finances": { "liquid": { "balance": liquid }, "pockets": pockets } })
    }

    fn compose(s: &Value, unclassified: usize, recent: &[Event], q: Option<&str>) -> View {
        feed(&Registry::default(), s, unclassified, recent, q, Vec::new()).expect("composed").1
    }

    fn card<'a>(v: &'a View, id: &str) -> Option<&'a sustena_core::WidgetCandidate> {
        v.selected.iter().chain(v.excluded.iter()).find(|c| c.id == id)
    }

    #[test]
    fn every_declared_widget_loads_against_the_view_schema() {
        // ★ A card reading a dimension the projection does not carry is a
        //   load-time failure, not an empty box at render time.
        let r = WidgetSet::load(widget_declarations(), &view_definition(), &Registry::default());
        assert!(r.is_ok(), "a declared card was refused at load: {:?}", r.err());
    }

    #[test]
    fn a_widget_reading_a_dimension_the_view_lacks_is_refused_at_load() {
        let bad = vec![WidgetDecl {
            id: "nonsense".into(),
            inputs: vec!["rainfall".into()],
            render: "x".into(),
            emits: vec![],
            binding: BindingKey::Unit,
        }];
        assert!(WidgetSet::load(bad, &view_definition(), &Registry::default()).is_err());
    }

    #[test]
    fn the_projection_is_read_off_the_household_not_declared() {
        let s = state(500.0, json!({
            "food": { "allocated": 8000.0, "spent": 7600.0 },
            "rent": { "allocated": 20000.0, "spent": 20000.0 },
        }));
        let r = reading_of(&s, 0);
        assert_eq!(r["liquid"], json!(500.0));
        // rent is at 100%, food at 95% — the worst is rent, and the ceiling is
        // rent's OWN allocation.
        assert_eq!(r["worst_pocket"], json!("rent"));
        assert_eq!(r["worst_allocated"], json!(20000.0));
        assert_eq!(region_from(&r).intervals.iter().find(|i| i.dim == "worst_spent").unwrap().hi,
                   20000.0);
    }

    #[test]
    fn an_unfunded_pocket_is_never_the_worst() {
        // ★ `[0,0]` on an unfunded pocket would score every shilling as maximal
        //   strain — the zero-denominator trap in a different costume.
        let s = state(500.0, json!({ "empty": { "allocated": 0.0, "spent": 40.0 } }));
        let r = reading_of(&s, 0);
        assert!(r.get("worst_pocket").is_none());
    }

    #[test]
    fn a_household_inside_its_own_limits_is_not_urgent() {
        let s = state(500.0, json!({ "food": { "allocated": 8000.0, "spent": 100.0 } }));
        let v = compose(&s, 0, &[], None);
        assert!(v.selected.iter().all(|c| c.urgency == 0.0), "inside V, nothing is urgent");
    }

    /// ★★★ The property the projection exists for.
    #[test]
    fn overspending_a_pocket_the_household_declared_is_real_measured_urgency() {
        let s = state(500.0, json!({ "food": { "allocated": 8000.0, "spent": 9000.0 } }));
        let v = compose(&s, 0, &[], None);
        let strained = card(&v, "pocket_strain").expect("the card is a candidate");
        assert!(strained.urgency > 0.0, "the household's own ceiling was crossed");
        assert!(strained.basis.is_measured(), "and on a declared basis, not a guess");
        // The calm card reads the same household and is NOT urgent — the
        // attribution is genuinely per-dimension now.
        assert_eq!(card(&v, "household_summary").map(|c| c.urgency), Some(0.0));
    }

    #[test]
    fn an_unclassified_capture_is_urgent_on_its_own_dimension() {
        let s = state(500.0, json!({ "food": { "allocated": 8000.0, "spent": 10.0 } }));

        // ★★ Nothing to classify: the card WITHDRAWS with a reason rather than
        //    ranking at zero and rendering an empty box. It is not a candidate
        //    at all, so it cannot displace one that has something to say.
        let quiet = compose(&s, 0, &[], None);
        assert_eq!(card(&quiet, "classify_capture").map(|c| c.urgency), None);
        let out = quiet
            .withdrawn
            .iter()
            .find(|w| w.id == "classify_capture")
            .expect("a withdrawal, not a silent absence");
        assert!(!out.reason.is_empty(), "and it says why");

        // The pocket card is unaffected: one card having nothing to say does
        // not cost another its urgency.
        let loud = compose(&s, 3, &[], None);
        assert!(card(&loud, "classify_capture").unwrap().urgency > 0.0);
    }

    #[test]
    fn a_card_with_nothing_to_say_withdraws_before_ranking() {
        // ★★ No funded pocket: the strain card has nothing to report and
        //    withdraws WITH A REASON rather than rendering an unexplained zero.
        let s = state(500.0, json!({}));
        let v = compose(&s, 0, &[], None);
        assert!(
            v.withdrawn.iter().any(|w| w.id == "pocket_strain"),
            "expected a withdrawal, got {:?}",
            v.withdrawn
        );
        assert!(v.withdrawn.iter().all(|w| !w.reason.is_empty()), "and it says why");
        // Withdrawn is NOT excluded: it was never a candidate, so it has no
        // score to report.
        assert!(v.excluded.iter().all(|c| c.id != "pocket_strain"));
    }

    #[test]
    fn an_event_bound_card_is_not_a_candidate_until_its_event_fires() {
        let s = state(500.0, json!({ "food": { "allocated": 100.0, "spent": 10.0 } }));
        assert!(card(&compose(&s, 0, &[], None), "recent_spend").is_none());
        let spent = Event::backfilled(
            "e1",
            "event.finances.pocket_spent",
            0,
            sustena_core::event::CausalStamp::new("host"),
        );
        assert!(card(&compose(&s, 0, &[spent], None), "recent_spend").is_some());
    }

    #[test]
    fn the_feed_never_exceeds_the_attention_budget() {
        let s = state(500.0, json!({ "food": { "allocated": 100.0, "spent": 400.0 } }));
        let v = compose(&s, 5, &[], None);
        assert!(v.spent <= v.budget, "{} > {}", v.spent, v.budget);
        assert_eq!(v.budget, BUDGET);
    }

    #[test]
    fn what_did_not_make_the_cut_carries_its_score() {
        // ★ An exclusion nobody can inspect is indistinguishable from a widget
        //   that was never considered.
        let s = state(500.0, json!({ "food": { "allocated": 100.0, "spent": 400.0 } }));
        let v = compose(&s, 2, &[], None);
        assert!(v.excluded.iter().all(|c| c.score >= 0.0));
        assert_eq!(v.candidates_considered, v.selected.len() + v.excluded.len());
    }

    #[test]
    fn a_query_moves_relevance_and_the_engine_reports_both_halves() {
        let s = state(500.0, json!({ "food": { "allocated": 100.0, "spent": 10.0 } }));
        let plain = compose(&s, 0, &[], None);
        let asked = compose(&s, 0, &[], Some("pocket strain"));
        assert_eq!(card(&plain, "pocket_strain").unwrap().relevance, 0.5, "neutral with no query");
        assert!(card(&asked, "pocket_strain").unwrap().relevance > 0.5, "a query moves it");
    }

    #[test]
    fn the_feed_is_reproducible() {
        let s = state(500.0, json!({ "food": { "allocated": 100.0, "spent": 400.0 } }));
        assert_eq!(compose(&s, 1, &[], None), compose(&s, 1, &[], None));
    }
}
