//! `β` and `compose(r)` — the curated view (UI-1; the Curated UI, §§1–5).
//!
//! The Rust build of what Slice 13 shipped in Python, so this row has a
//! **reference implementation to measure against** rather than only a spec.
//! Where it differs, it differs on purpose and says so.
//!
//! ## ★★ `β : (EventClass ∪ Unit) → 𝒫(W)`, and the ∪ is a type
//!
//! β is **event-first**: a widget is looked up FROM what happened, never from
//! an operator name. It is the formal inverse of the operator→widget
//! `ui_schema`, and additive to it.
//!
//! ★ STEP-0 found there is **no `EventClass` in this core** — `Event` carries a
//! `name: String` and nothing else that could serve as a class. And the
//! reference's `event_class` is *also* a plain string; its `binding_key`
//! returns either that string or a `UNIT` sentinel, both as `dict` keys. So the
//! honest port keys on `Event.name` directly, with one sharpening:
//! [`BindingKey`] is a **sum type**, so `EventClass ∪ Unit` is genuinely the ∪
//! the notation claims rather than a magic string sharing a namespace with real
//! event names. The collision that removes is theoretical — nobody names an
//! event `__unit__` — and it is worth having anyway, because a widget with **no**
//! binding also becomes unrepresentable, which the reference has to check for at
//! runtime (*"must declare either event_class or unit=true"*).
//!
//! ## ★★ `compose(r)`, `r = ⟨state, query, device⟩`
//!
//! Resolved fresh on every call and **never stored**. Unit-bound widgets are
//! always eligible; an event-bound widget is eligible **only when its class
//! appears in the recent log**. That is what makes it event-first in behaviour
//! and not only in shape — it never degrades into *scan everything and rank*.
//!
//! ## ★★★ Urgency is `d(s,V)` — and here it is the real thing
//!
//! The salience score is `score(w) = α·urgency + λ·relevance` with `α = 0.75`
//! dominant, the reference's own constants. ★★ But the reference's urgency is
//! **a declared proxy** — `pct = spent/allocated` — and its own row (UI-7) says
//! why: *"deliberately reusing the Monitor's Slice-0 signal because CUSUM/EWMA
//! don't exist"*. In this core they do, and so does [`crate::region`], so
//! urgency here is **the actual weighted distance to the viable region**.
//!
//! A widget's urgency is its **share of `d(s,V)`**: the sum of the per-dimension
//! weighted excesses for the dimensions it declares as `inputs`, over the
//! sustain's total. *The fraction of the household's distance-from-viable that
//! this widget can show you.* Fully viable ⇒ every urgency is 0, which is
//! correct: nothing is urgent.
//!
//! ## ★★★ Two gaming surfaces, both closed structurally
//!
//! 1. **A widget cannot set its own urgency.** It declares which dimensions it
//!    reads — checked against `dim(S)` at load by UI-2 — and the *distance* is
//!    the household's. Same one signal MON-7 encodes and the Monitor escalates
//!    on; three consumers, one number.
//! 2. ★★★ **A widget cannot declare itself cheap.** The reference's `cost` is an
//!    **author-declared integer defaulting to 1**, while its own docstring
//!    describes a *"Hick-Hyman superlinear per-item cost"* — the formula is
//!    written down and not computed. That is a gaming surface of exactly the
//!    `colour_rule` kind: a widget reading eight dimensions can declare `cost:
//!    1` and get a bargain, and an honest widget declaring `3` is penalised for
//!    honesty. Here [`attention_cost`] is **derived** from `inputs.len()` and
//!    superlinear: `1 + n(n−1)/2`, the pairwise relating a reader must do.
//!
//! ★★ Together they close the one hole share-of-distance would otherwise open.
//! A widget could raise its urgency by declaring **more** dimensions — but the
//! same declaration raises its cost **superlinearly**, so the attention budget
//! prices the strategy out. Grabbing the whole state to look urgent buys a
//! score of 1.0 at a cost no budget can afford, and it loses to two focused
//! widgets. **The greedy move is self-defeating**, and that is asserted rather
//! than argued.
//!
//! ## The knapsack is real
//!
//! A genuine 0/1 DP over integer cost and scaled score, not a sort dressed up —
//! proven by a case where greedy-by-score picks strictly worse. Every excluded
//! candidate comes back **with its score**, so the *"N stayed quiet"* line has
//! something true to say; and optimality is asserted against brute force over
//! every feasible subset.
//!
//! ★★ One behaviour worth naming because testing surfaced it rather than
//! design: the DP takes an item only when it **strictly improves** the total,
//! so a widget scoring exactly `0` is never selected. That is right rather than
//! a gap — `K ≈ 4` is scarce, and spending a chunk on a card worth nothing is
//! the noise a curated surface exists to remove. It is also **at parity** with
//! the reference, whose `dp[c-w] + v > dp[c]` behaves identically. Nothing is
//! hidden: the widget comes back in `excluded` carrying its `0.0`.
//!
//! ## Read-only
//!
//! Every parameter is a shared reference and there is no `Registry`, so
//! `compose` **cannot** execute an operator — a fact about the signature, not a
//! discipline. State and events are asserted byte-identical across a call
//! anyway, because a signature-only guarantee is worth exactly one refactor.
//!
//! ## Honest scope
//!
//! `render` stays the opaque tag UI-2 made it, so this produces a **ranked
//! selection with its reasons**, not a view fragment. The reference's
//! `WIDGET_RENDERERS` are hand-written per render id and belong to a surface
//! with a display — the same line MON-7 drew.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::event::Event;
use crate::region::Region;
use crate::widget::{LoadedWidget, WidgetSet};

/// `α` — urgency's weight in the salience score. Dominant, deliberately.
pub const ALPHA_URGENCY: f64 = 0.75;
/// `λ` — relevance's weight.
pub const LAMBDA_RELEVANCE: f64 = 0.25;
/// `K ≈ 4` chunks (Cowan). The attention budget.
pub const DEFAULT_BUDGET: usize = 4;
/// Score → integer value for the DP table.
const SCORE_SCALE: f64 = 1000.0;

/// `EventClass ∪ Unit` — β's key, as a **sum type**.
///
/// ★ The reference uses a `UNIT` sentinel string in the same namespace as real
/// event classes. A disjoint union removes that collision and, more usefully,
/// makes a widget with **no** binding unrepresentable — which the reference has
/// to reject at runtime.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingKey {
    /// Always eligible; supplies its own urgency from state.
    Unit,
    /// Eligible only when this event class appears in the recent log.
    Event(String),
}

impl BindingKey {
    pub fn event(name: impl Into<String>) -> Self {
        BindingKey::Event(name.into())
    }
}

/// `β : (EventClass ∪ Unit) → 𝒫(W)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingTable {
    bound: BTreeMap<BindingKey, Vec<String>>,
}

impl BindingTable {
    /// Build β from a **loaded** set — so every widget in it already
    /// type-checked (UI-2). A widget whose inputs name nothing declared cannot
    /// be here, and therefore cannot enter the knapsack.
    pub fn build(widgets: &WidgetSet) -> BindingTable {
        let mut bound: BTreeMap<BindingKey, Vec<String>> = BTreeMap::new();
        for w in widgets.iter() {
            bound.entry(w.binding().clone()).or_default().push(w.id().to_string());
        }
        BindingTable { bound }
    }

    /// The widgets bound to a key. Empty rather than absent — a key nothing
    /// binds is a real answer.
    pub fn at(&self, key: &BindingKey) -> &[String] {
        self.bound.get(key).map_or(&[], Vec::as_slice)
    }

    pub fn keys(&self) -> impl Iterator<Item = &BindingKey> {
        self.bound.keys()
    }

    pub fn len(&self) -> usize {
        self.bound.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bound.is_empty()
    }
}

/// `r = ⟨state, query, device⟩` — the request half. State and the log arrive
/// separately because they are reads, not requests.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub query: Option<String>,
    /// Carried and reported, not yet used to rank. See the limits.
    pub device: String,
    pub budget: usize,
}

impl Default for Request {
    fn default() -> Self {
        Request { query: None, device: "phone".to_string(), budget: DEFAULT_BUDGET }
    }
}

impl Request {
    pub fn asking(query: &str) -> Self {
        Request { query: Some(query.to_string()), ..Default::default() }
    }

    pub fn with_budget(mut self, budget: usize) -> Self {
        self.budget = budget;
        self
    }
}

/// Why a widget was eligible at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eligibility {
    /// Unit-bound — always a candidate.
    AlwaysEligible,
    /// This event class appeared in the recent log.
    EventFired { class: String },
}

impl Eligibility {
    pub fn describe(&self) -> String {
        match self {
            Eligibility::AlwaysEligible => "always eligible (Unit-bound)".to_string(),
            Eligibility::EventFired { class } => format!("'{class}' appeared in the recent log"),
        }
    }
}

/// One scored candidate.
///
/// ★ Named `WidgetCandidate`, not `Candidate` — `ooda::Candidate` is a
/// decision option surfaced at DECIDE, a genuinely different thing.
/// Twenty-seventh collision; the newcomer takes the longer name.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetCandidate {
    pub id: String,
    pub render: String,
    /// Derived from `inputs.len()`, never declared. See [`attention_cost`].
    pub cost: usize,
    /// This widget's share of `d(s,V)`, on `[0,1]`.
    pub urgency: f64,
    pub relevance: f64,
    pub score: f64,
    pub why: Eligibility,
}

/// What `compose` resolved. Never stored; recomputed on every call.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub selected: Vec<WidgetCandidate>,
    /// ★ Everything that did **not** make the cut, **with its score** — so the
    /// *"N other things stayed quiet"* line has something true to say. An
    /// exclusion nobody can inspect is indistinguishable from a widget that was
    /// never considered.
    pub excluded: Vec<WidgetCandidate>,
    pub candidates_considered: usize,
    pub budget: usize,
    /// Attention actually spent. `budget - spent` is what was left on the table.
    pub spent: usize,
}

/// Hick-Hyman, **computed**: reading `n` items is not `n` work, it is the
/// pairwise relating between them.
///
/// `1 + n(n−1)/2` — 1, 2, 4, 7, 11 … ★ Derived from the declaration UI-2
/// already checked, never declared by the widget, so a widget cannot buy a
/// bargain by asserting it is cheap.
pub fn attention_cost(input_count: usize) -> usize {
    1 + input_count.saturating_sub(1) * input_count / 2
}

/// A widget's share of `d(s,V)`: the per-dimension weighted excess over the
/// dimensions it reads, as a fraction of the sustain's total.
fn urgency_of(widget: &LoadedWidget, region: &Region, state: &Value) -> f64 {
    let Ok(distance) = region.distance(state) else {
        // A region that cannot be measured against this state yields no
        // urgency, rather than a fabricated one. `Skew::Unknown`'s discipline.
        return 0.0;
    };
    if distance.weighted <= 0.0 {
        return 0.0; // Inside V. Nothing is urgent.
    }
    // Root-name granularity, matching how `per_dimension` reports and how a
    // widget's inputs are checked.
    let reads: BTreeSet<&str> = widget
        .inputs()
        .iter()
        .map(|p| p.split(['.', '[']).next().unwrap_or(p))
        .collect();
    let mine: f64 = distance
        .per_dimension
        .iter()
        .filter(|(dim, _)| reads.contains(dim.as_str()))
        .map(|(_, excess)| excess)
        .sum();
    (mine / distance.weighted).clamp(0.0, 1.0)
}

/// Token overlap between the query and what the widget is about.
///
/// Neutral (0.5) with no query — the common case, since the usual question is
/// *what needs me right now* rather than a search.
fn relevance_of(widget: &LoadedWidget, query: Option<&str>) -> f64 {
    let Some(q) = query else { return 0.5 };
    let tokens: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect();
    if tokens.is_empty() {
        return 0.5;
    }
    let subject = format!("{} {} {}", widget.id(), widget.render(), widget.inputs().join(" "))
        .to_lowercase();
    let hits = tokens.iter().filter(|t| subject.contains(t.as_str())).count();
    hits as f64 / tokens.len() as f64
}

/// `score(w) = α·urgency + λ·relevance`.
pub fn salience(urgency: f64, relevance: f64) -> f64 {
    ALPHA_URGENCY * urgency + LAMBDA_RELEVANCE * relevance
}

/// A real 0/1 knapsack. Returns `(selected, excluded)`, both score-ordered.
pub fn knapsack_select(
    candidates: &[WidgetCandidate],
    budget: usize,
) -> (Vec<WidgetCandidate>, Vec<WidgetCandidate>) {
    let n = candidates.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }
    let values: Vec<i64> =
        candidates.iter().map(|c| (c.score * SCORE_SCALE).round().max(0.0) as i64).collect();
    let weights: Vec<usize> = candidates.iter().map(|c| c.cost.max(1)).collect();

    let mut dp = vec![0i64; budget + 1];
    let mut keep = vec![vec![false; budget + 1]; n];
    for i in 0..n {
        let (w, v) = (weights[i], values[i]);
        for c in (0..=budget).rev() {
            if w <= c && dp[c - w] + v > dp[c] {
                dp[c] = dp[c - w] + v;
                keep[i][c] = true;
            }
        }
    }

    let mut chosen: BTreeSet<usize> = BTreeSet::new();
    let mut c = budget;
    for i in (0..n).rev() {
        if keep[i][c] {
            chosen.insert(i);
            c -= weights[i];
        }
    }

    let mut selected: Vec<WidgetCandidate> = Vec::new();
    let mut excluded: Vec<WidgetCandidate> = Vec::new();
    for (i, cand) in candidates.iter().enumerate() {
        if chosen.contains(&i) {
            selected.push(cand.clone());
        } else {
            excluded.push(cand.clone());
        }
    }
    let by_score = |a: &WidgetCandidate, b: &WidgetCandidate| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
    };
    selected.sort_by(by_score);
    excluded.sort_by(by_score);
    (selected, excluded)
}

/// `compose(r)` — resolve a curated view.
///
/// Read-only by signature: shared references in, a `View` out, and no
/// `Registry`, so there is nothing here that could run an operator.
/// ★ Named `compose_view`, not `compose` — [`crate::compose`] is OP-5's
/// checked-composition module (`wp`, pathway chaining), a genuinely different
/// thing. Twenty-sixth collision; the newcomer takes the longer name.
pub fn compose_view(
    widgets: &WidgetSet,
    region: &Region,
    state: &Value,
    recent: &[Event],
    request: &Request,
) -> View {
    let beta = BindingTable::build(widgets);

    // ★ Event-first: the classes that ACTUALLY appeared, not every class β
    // knows about. A widget whose class is absent is not a candidate at all —
    // it is never scored and never considered.
    let fired: BTreeSet<&str> = recent.iter().map(|e| e.name.as_str()).collect();

    let mut eligible: Vec<(&LoadedWidget, Eligibility)> = Vec::new();
    for id in beta.at(&BindingKey::Unit) {
        if let Some(w) = widgets.get(id) {
            eligible.push((w, Eligibility::AlwaysEligible));
        }
    }
    for key in beta.keys() {
        let BindingKey::Event(class) = key else { continue };
        if !fired.contains(class.as_str()) {
            continue;
        }
        for id in beta.at(key) {
            if let Some(w) = widgets.get(id) {
                eligible.push((w, Eligibility::EventFired { class: class.clone() }));
            }
        }
    }

    let mut candidates: Vec<WidgetCandidate> = eligible
        .into_iter()
        .map(|(w, why)| {
            let urgency = urgency_of(w, region, state);
            let relevance = relevance_of(w, request.query.as_deref());
            WidgetCandidate {
                id: w.id().to_string(),
                render: w.render().to_string(),
                cost: attention_cost(w.inputs().len()),
                urgency,
                relevance,
                score: salience(urgency, relevance),
                why,
            }
        })
        .collect();
    // Deterministic input order, so the DP's tie-breaking is reproducible.
    candidates.sort_by(|a, b| a.id.cmp(&b.id));

    let considered = candidates.len();
    let (selected, excluded) = knapsack_select(&candidates, request.budget);
    let spent = selected.iter().map(|c| c.cost).sum();

    View { selected, excluded, candidates_considered: considered, budget: request.budget, spent }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::Definition;
    use crate::event::Event;
    use crate::operator::Registry;
    use crate::region::Interval;
    use crate::schema::{DimType, Schema};
    use crate::widget::WidgetDecl;
    use serde_json::json;

    fn definition() -> Definition {
        Definition::new(
            Schema::new()
                .declare("balance", DimType::Number { lo: None, hi: None })
                .declare("stock", DimType::Number { lo: None, hi: None })
                .declare("mood", DimType::Number { lo: None, hi: None }),
        )
        .with_operator("budget.allocate")
    }

    fn region() -> Region {
        Region::new()
            .bounding(Interval::new("balance", 0.0, 100.0))
            .bounding(Interval::new("stock", 0.0, 100.0))
            .bounding(Interval::new("mood", 0.0, 100.0))
            .weighing("balance", 1.0)
            .weighing("stock", 1.0)
            .weighing("mood", 1.0)
    }

    fn event(name: &str) -> Event {
        use crate::event::{CausalStamp, Provenance};
        Event {
            id: format!("e-{name}"),
            name: name.into(),
            t_event: 1_000,
            t_ingest: None,
            provenance: Provenance::Observed,
            source: None,
            stamp: CausalStamp::new("test"),
            causes: vec![],
            mutations: vec![],
        }
    }

    fn load(decls: Vec<WidgetDecl>) -> WidgetSet {
        WidgetSet::load(decls, &definition(), &Registry::default()).unwrap()
    }

    // ── β, and event-first ───────────────────────────────────────────────────

    #[test]
    fn beta_keys_on_an_event_class_or_unit_never_on_an_operator() {
        let set = load(vec![
            WidgetDecl::new("always", "card").unit().reading("balance"),
            WidgetDecl::new("on_spend", "card")
                .bound_to("event.finances.pocket_spent")
                .reading("balance"),
        ]);
        let beta = BindingTable::build(&set);
        assert_eq!(beta.at(&BindingKey::Unit), ["always".to_string()]);
        assert_eq!(
            beta.at(&BindingKey::event("event.finances.pocket_spent")),
            ["on_spend".to_string()]
        );
        // Nothing is reachable by operator name — β has no such key to hold it.
        assert!(beta.at(&BindingKey::event("budget.allocate")).is_empty());
    }

    #[test]
    fn an_event_bound_widget_is_not_a_candidate_when_its_class_did_not_fire() {
        // ★★ Event-first in BEHAVIOUR: absent from the log means never scored,
        // not scored-and-ranked-low. This is what stops it degrading into
        // "scan everything".
        let set = load(vec![
            WidgetDecl::new("always", "card").unit().reading("balance"),
            WidgetDecl::new("on_spend", "card")
                .bound_to("event.finances.pocket_spent")
                .reading("balance"),
        ]);
        let view = compose_view(&set, &region(), &json!({"balance": 50.0}), &[], &Request::default());
        assert_eq!(view.candidates_considered, 1);
        assert!(view.selected.iter().all(|c| c.id == "always"));
    }

    #[test]
    fn it_becomes_a_candidate_the_moment_its_class_appears() {
        let set = load(vec![WidgetDecl::new("on_spend", "card")
            .bound_to("event.finances.pocket_spent")
            .reading("balance")]);
        let log = [event("event.finances.pocket_spent")];
        let view = compose_view(&set, &region(), &json!({"balance": 50.0}), &log, &Request::default());
        assert_eq!(view.candidates_considered, 1);
        assert_eq!(
            view.selected[0].why,
            Eligibility::EventFired { class: "event.finances.pocket_spent".into() }
        );
    }

    // ── urgency is d(s,V) ────────────────────────────────────────────────────

    #[test]
    fn a_widget_reading_the_breached_dimension_outranks_one_reading_a_healthy_one() {
        // `balance` is 60 outside its band; `mood` is inside.
        let set = load(vec![
            WidgetDecl::new("money", "card").unit().reading("balance"),
            WidgetDecl::new("feelings", "card").unit().reading("mood"),
        ]);
        let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
        let view = compose_view(&set, &region(), &state, &[], &Request::default());

        let money = view.selected.iter().find(|c| c.id == "money").unwrap();
        let feelings = view.selected.iter().find(|c| c.id == "feelings").unwrap();
        assert!(money.urgency > 0.0);
        assert_eq!(feelings.urgency, 0.0, "a healthy dimension carries no urgency");
        assert!(money.score > feelings.score);
    }

    #[test]
    fn inside_the_viable_region_nothing_is_urgent() {
        let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let view = compose_view(
            &set,
            &region(),
            &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default(),
        );
        assert_eq!(view.selected[0].urgency, 0.0);
    }

    #[test]
    fn a_widget_cannot_set_its_own_urgency() {
        // The only lever it has is which DIMENSIONS it declares — and those are
        // checked against dim(S) at load. The distance is the household's.
        let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let calm = compose_view(
            &set,
            &region(),
            &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default(),
        );
        let breached = compose_view(
            &set,
            &region(),
            &json!({"balance": 160.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default(),
        );
        assert!(breached.selected[0].urgency > calm.selected[0].urgency);
    }

    // ── the cost is derived, and it prices out the greedy move ───────────────

    #[test]
    fn cost_is_superlinear_in_the_declared_inputs() {
        assert_eq!(attention_cost(1), 1);
        assert_eq!(attention_cost(2), 2);
        assert_eq!(attention_cost(3), 4);
        assert_eq!(attention_cost(4), 7);
        assert!(attention_cost(4) - attention_cost(3) > attention_cost(3) - attention_cost(2));
    }

    #[test]
    fn grabbing_every_dimension_to_look_urgent_is_self_defeating() {
        // ★★★ The greedy strategy: declare all three dimensions, capture the
        // whole of d(s,V), score 1.0 · α. It costs 4 and loses to two focused
        // widgets that cost 1 each.
        let set = load(vec![
            WidgetDecl::new("greedy", "card")
                .unit()
                .reading("balance")
                .reading("stock")
                .reading("mood"),
            WidgetDecl::new("focused_a", "card").unit().reading("balance"),
            WidgetDecl::new("focused_b", "card").unit().reading("stock"),
        ]);
        let state = json!({"balance": 160.0, "stock": 160.0, "mood": 50.0});
        let view = compose_view(&set, &region(), &state, &[], &Request::default().with_budget(3));

        let greedy = view.excluded.iter().find(|c| c.id == "greedy");
        assert!(greedy.is_some(), "the greedy widget is excluded");
        assert_eq!(greedy.unwrap().cost, 4, "three inputs cost four chunks");
        assert_eq!(view.selected.len(), 2, "two focused widgets fit where one greedy one did not");
    }

    // ── the knapsack is real ─────────────────────────────────────────────────

    fn candidate(id: &str, cost: usize, score: f64) -> WidgetCandidate {
        WidgetCandidate {
            id: id.into(),
            render: "card".into(),
            cost,
            urgency: score,
            relevance: 0.0,
            score,
            why: Eligibility::AlwaysEligible,
        }
    }

    #[test]
    fn it_is_a_real_knapsack_and_not_a_sort_dressed_up() {
        // ★★ Greedy-by-score takes the 0.9 and stops (total 0.9). The DP takes
        // the pair (total 1.15).
        let cands =
            vec![candidate("big", 4, 0.90), candidate("a", 2, 0.60), candidate("b", 2, 0.55)];
        let (selected, excluded) = knapsack_select(&cands, 4);
        let ids: BTreeSet<&str> = selected.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"].into_iter().collect::<BTreeSet<_>>());
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].id, "big");
    }

    #[test]
    fn the_selection_is_optimal_against_brute_force() {
        let cands = vec![
            candidate("p", 1, 0.31),
            candidate("q", 2, 0.62),
            candidate("r", 3, 0.80),
            candidate("s", 2, 0.55),
            candidate("t", 1, 0.20),
        ];
        let budget = 4;
        let (selected, _) = knapsack_select(&cands, budget);
        let got: f64 = selected.iter().map(|c| c.score).sum();

        let mut best = 0.0_f64;
        for mask in 0..(1u32 << cands.len()) {
            let (mut w, mut v) = (0usize, 0.0_f64);
            for (i, c) in cands.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    w += c.cost;
                    v += c.score;
                }
            }
            if w <= budget && v > best {
                best = v;
            }
        }
        assert!((got - best).abs() < 1e-9, "DP got {got}, brute force found {best}");
    }

    #[test]
    fn every_exclusion_comes_back_with_its_score() {
        let cands = vec![candidate("a", 3, 0.9), candidate("b", 3, 0.4)];
        let (selected, excluded) = knapsack_select(&cands, 3);
        assert_eq!(selected.len(), 1);
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].score, 0.4, "the quiet one can say what it scored");
    }

    #[test]
    fn an_empty_field_selects_nothing_and_says_so() {
        let (selected, excluded) = knapsack_select(&[], 4);
        assert!(selected.is_empty() && excluded.is_empty());
    }

    // ── relevance ────────────────────────────────────────────────────────────

    #[test]
    fn relevance_is_neutral_with_no_query_and_real_with_one() {
        let set = load(vec![WidgetDecl::new("balance_card", "card").unit().reading("balance")]);
        let state = json!({"balance": 50.0});
        let quiet = compose_view(&set, &region(), &state, &[], &Request::default());
        assert_eq!(quiet.selected[0].relevance, 0.5);

        let asked = compose_view(&set, &region(), &state, &[], &Request::asking("balance"));
        assert_eq!(asked.selected[0].relevance, 1.0);

        // ★★ Zero relevance on a fully-viable sustain means a score of exactly
        // 0.0 — and a zero-value widget is NOT selected. See below.
        let unrelated = compose_view(&set, &region(), &state, &[], &Request::asking("weather"));
        assert!(unrelated.selected.is_empty());
        assert_eq!(unrelated.excluded[0].relevance, 0.0);
    }

    #[test]
    fn a_zero_score_widget_is_not_shown_and_says_so_from_the_excluded_list() {
        // ★★ Found by testing, not design, and kept: the DP only takes an item
        // that strictly improves the total, so a widget with score exactly 0
        // never enters the selection. That is right rather than a gap — K ≈ 4
        // is scarce, and spending a chunk on a card worth nothing is exactly
        // the noise a curated surface exists to remove. It is also **at parity**
        // with the reference, whose `dp[c-w] + v > dp[c]` behaves identically.
        // Nothing is hidden: it comes back in `excluded` with its 0.0.
        let set = load(vec![WidgetDecl::new("nothing_to_say", "card").unit().reading("mood")]);
        let view = compose_view(
            &set,
            &region(),
            &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::asking("weather"),
        );
        assert_eq!(view.candidates_considered, 1, "it WAS considered");
        assert!(view.selected.is_empty(), "and not shown");
        assert_eq!(view.excluded[0].score, 0.0, "and it can say why");
        assert_eq!(view.spent, 0, "no attention spent on nothing");
    }

    // ── read-only ────────────────────────────────────────────────────────────

    #[test]
    fn compose_changes_nothing() {
        let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
        let log = vec![event("event.finances.pocket_spent")];

        let before_state = state.clone();
        let before_log = log.clone();
        let _ = compose_view(&set, &region(), &state, &log, &Request::default());
        let _ = compose_view(&set, &region(), &state, &log, &Request::default());
        assert_eq!(state, before_state);
        assert_eq!(log, before_log);
    }

    #[test]
    fn composing_twice_gives_the_same_view() {
        let set = load(vec![
            WidgetDecl::new("a", "card").unit().reading("balance"),
            WidgetDecl::new("b", "card").unit().reading("stock"),
        ]);
        let state = json!({"balance": 160.0, "stock": 120.0, "mood": 50.0});
        let one = compose_view(&set, &region(), &state, &[], &Request::default());
        let two = compose_view(&set, &region(), &state, &[], &Request::default());
        assert_eq!(one, two, "resolved fresh, and deterministically");
    }
}
