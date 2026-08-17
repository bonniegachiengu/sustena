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
//! ## ★★ Honest empty-slate suppression: a card WITHDRAWS (UI-5)
//!
//! A widget with nothing to say is **not shown at all** — it is not scored
//! low, it is **withdrawn before any ranking happens**. The distinction is the
//! row's whole point: *considered and outranked* and *never had anything to
//! say* are different facts, and rendering an unexplained `0` conflates them.
//!
//! ★★ **The grounding condition is derived, not declared.** A widget is
//! grounded when at least one of its declared `inputs` resolves to something
//! real in current state — present, non-null, and not an empty container. A
//! card reading `finances.pockets` in a household with no pockets has nothing
//! to say, and it says so by leaving. Derived from the inputs UI-2 already
//! checked against `dim(S)`, so a widget cannot declare itself grounded any
//! more than it can declare itself cheap or persistent.
//!
//! ★ **The withdrawal is reported, not silent** ([`View::withdrawn`]): a
//! surface can say *N had nothing to show* separately from *N stayed quiet*,
//! because those are different answers. And a widget declaring **no** inputs is
//! grounded by default — it makes no claim about state, so there is nothing
//! for state to fail to support.
//!
//! ## ★★★ The third salience surface: ORDERING (UI-13)
//!
//! *Salience is never rendered as salience* has three surfaces a widget could
//! try to stage its own prominence on, and the first two were already closed:
//!
//! 1. **Brightness** — MON-7. Field-relative, and `VisualSpec` has no public
//!    constructor, so a widget cannot encode itself bright.
//! 2. **Score** — UI-1 and UI-7. Urgency is the household's `d(s,V)`, cost is
//!    **derived** from the declared inputs, and the weights are the
//!    household's [`SaliencePolicy`], which refuses to let urgency be argued
//!    into second place.
//! 3. ★★★ **Position** — this row, and it was **genuinely open**. `WidgetSet`
//!    is keyed by id and Rust's `sort_by` is *stable*, so wherever two widgets
//!    tied on score the alphabetically-earlier **id** took the higher slot —
//!    and the id is widget-authored. `aaa_spending` outranked `zzz_spending`
//!    at identical score. Small, real, and exactly the Goodhart move.
//!
//! It is closed the way the other two were — by removing the lever rather than
//! policing it. [`rank_selection`] orders on three keys a widget cannot set:
//! **score** desc, then **urgency** desc, then **cost** asc (derived, and the
//! right preference — cheaper first at equal value). Past those, two widgets
//! are indistinguishable on every un-gameable axis, and rather than fabricate a
//! winner the tie is **declared**: [`WidgetCandidate::rank`] is equal for tied
//! peers, so **position carries no information the score did not already
//! give**. The same discipline as `Skew::Unknown` and
//! [`UrgencyBasis::Undeclared`] — do not invent a distinction you do not have.
//!
//! ★★ And the residual array order within a tied group is the **household's
//! declaration order**, not the widget's name: `WidgetSet` keeps the order it
//! was loaded in for exactly this reason. **Renaming a widget changes neither
//! its rank nor its position**, which is the property asserted.
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
use crate::panel::{PanelInView, PanelSet};
use crate::region::Region;
use crate::state::State;
use crate::widget::{LoadedWidget, WidgetSet};

/// `α` — the **article's declared preference** for urgency's weight, not a
/// derived value. Kept as the default a household inherits, never as the only
/// one available: see [`SaliencePolicy`].
pub const ALPHA_URGENCY: f64 = 0.75;
/// `λ` — relevance's weight, same status.
pub const LAMBDA_RELEVANCE: f64 = 0.25;
/// `K ≈ 4` chunks (Cowan). The attention budget.
pub const DEFAULT_BUDGET: usize = 4;
/// Score → integer value for the DP table.
const SCORE_SCALE: f64 = 1000.0;

/// The salience weights — **a declared preference, argued with rather than
/// hidden** (UI-7).
///
/// §IV is blunt about the defect this closes: *"Nothing derives `α = 0.75`. It
/// is a declared preference ... belongs in a declared constraint where it can
/// be **argued with**, not a module constant."* ★ The precedent is already in
/// this codebase — [`Region::weights`](crate::region::Region::weights) carries
/// exactly this treatment in its own docstring (*"A modelling choice, not a
/// fact. Declared here so it can be argued with"*). The salience weights simply
/// had not been given it.
///
/// ★★ It **does** have a `Default`, and that is the honest call rather than a
/// lapse. [`TrustPolicy`](crate::learned::TrustPolicy) has none because what a
/// trust level may reach is a genuine per-deployment decision with no safe
/// answer. Here the article states a preference and it is a good one; the
/// defect was never *there is a default*, it was *there is no way to hold a
/// different one*. So the default is kept **and named as a declaration rather
/// than a derivation**.
///
/// ★★★ **Two things you may not argue into.** [`declared`] refuses a policy
/// where relevance meets or beats urgency, and refuses weights that do not sum
/// to one:
///
/// - `α > λ` is §IV's whole point. A policy that let a *search query* outrank
///   *the household being in danger* would reintroduce, at the weights, exactly
///   the gaming UI-1 closed at the inputs.
/// - `α + λ = 1` keeps `score ∈ [0,1]`, so a score means the same thing across
///   sustains and the knapsack's integer scaling stays interpretable.
///
/// You may argue about the ratio. You may not argue urgency into second place.
///
/// [`declared`]: SaliencePolicy::declared
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SaliencePolicy {
    alpha: f64,
    lambda: f64,
}

impl Default for SaliencePolicy {
    fn default() -> Self {
        SaliencePolicy { alpha: ALPHA_URGENCY, lambda: LAMBDA_RELEVANCE }
    }
}

/// Why a [`SaliencePolicy`] could not be declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyError {
    /// Relevance meets or beats urgency.
    UrgencyNotDominant,
    /// The weights do not sum to one.
    WeightsDoNotSumToOne,
    /// A weight is negative or not finite.
    NotAWeight,
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyError::UrgencyNotDominant => write!(
                f,
                "α must exceed λ: a policy where relevance outranks urgency would let                  a search query outrank the household being in danger"
            ),
            PolicyError::WeightsDoNotSumToOne => {
                write!(f, "α + λ must be 1, so a score means the same thing across sustains")
            }
            PolicyError::NotAWeight => write!(f, "a weight must be finite and non-negative"),
        }
    }
}

impl SaliencePolicy {
    /// Declare the weights. Refuses the two things that are not arguable.
    pub fn declared(alpha: f64, lambda: f64) -> Result<SaliencePolicy, PolicyError> {
        if !alpha.is_finite() || !lambda.is_finite() || alpha < 0.0 || lambda < 0.0 {
            return Err(PolicyError::NotAWeight);
        }
        if (alpha + lambda - 1.0).abs() > 1e-9 {
            return Err(PolicyError::WeightsDoNotSumToOne);
        }
        if alpha <= lambda {
            return Err(PolicyError::UrgencyNotDominant);
        }
        Ok(SaliencePolicy { alpha, lambda })
    }

    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    pub fn lambda(&self) -> f64 {
        self.lambda
    }

    /// `score(w) = α·urgency + λ·relevance`.
    pub fn score(&self, urgency: f64, relevance: f64) -> f64 {
        self.alpha * urgency + self.lambda * relevance
    }
}

/// ★★★ Why an urgency of `0` is `0` — **UI-7's blind spot, reported rather than
/// merely fixed**.
///
/// The row's named defect belongs to the *proxy*: `spent/allocated` is
/// undefined at zero allocation, and returning `0.0` there scored an unfunded
/// pocket that had been spent from as perfectly calm — **backwards**. UI-1
/// replaced that proxy with `d(s,V)`, so there is no denominator left to be
/// undefined, and the artifact is structurally gone.
///
/// ★★ But a `0` can still arise two ways, and they mean opposite things:
///
/// - **`Bounded`** — every dimension the widget reads has an interval in `V`,
///   so a `0` means *inside the viable region*. Genuinely calm.
/// - **`Undeclared`** — some dimension it reads is **not bounded by `V` at
///   all**, so a `0` means *the household declared no wall here*, not *nothing
///   is wrong*. The metric is silent, and silence is not safety.
///
/// ★★ The distinction is the finding: the proxy **manufactured** a wrong zero;
/// `d(s,V)` gives an honest zero, and this type makes which kind it is legible
/// so a surface can say *nothing is declared here* instead of showing calm.
/// **No wall is invented to force urgency** — that would be this core deciding
/// what the household should care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrgencyBasis {
    /// Every dimension read is bounded by `V`.
    Bounded,
    /// These dimensions have no interval in `V`. A zero here is silence.
    Undeclared { dimensions: Vec<String> },
}

impl UrgencyBasis {
    /// Is a reading of `0` from this basis *calm*, or merely *unmeasured*?
    pub fn is_measured(&self) -> bool {
        matches!(self, UrgencyBasis::Bounded)
    }

    pub fn describe(&self) -> String {
        match self {
            UrgencyBasis::Bounded => "measured against the declared viable region".to_string(),
            UrgencyBasis::Undeclared { dimensions } => format!(
                "not measured: {} {} no bound in V, so a zero here is silence rather than safety",
                dimensions.join(", "),
                if dimensions.len() == 1 { "has" } else { "have" },
            ),
        }
    }
}

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

/// A widget that never entered the ranking, and why (UI-5).
///
/// ★ Distinct from an exclusion: an excluded widget was **considered and
/// outranked**, and carries a score. A withdrawn one had **nothing to say**,
/// and carries a reason instead — there is no score to report because none was
/// computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdrawn {
    pub id: String,
    pub reason: String,
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
    /// ★★ Whether that number was **measured** — see [`UrgencyBasis`]. A `0`
    /// with an `Undeclared` basis is silence, not safety.
    pub basis: UrgencyBasis,
    /// ★★★ Salience rank within the returned list, `0` first — and **equal for
    /// widgets that tie on every un-gameable key** (UI-13).
    ///
    /// A surface should render *this*, not the array index: where two widgets
    /// share a rank they are genuinely peers, and showing one above the other
    /// as though it mattered is precisely the fabricated prominence this row is
    /// about. Set by [`rank_selection`]; `0` on a candidate that has not been
    /// through it.
    pub rank: usize,
    pub relevance: f64,
    pub score: f64,
    pub why: Eligibility,
}

/// What `compose` resolved. Never stored; recomputed on every call.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// ★★★ The always-shown tier (CTL-8), **outside the attention budget**.
    ///
    /// A persistent panel is shown whatever else is happening, so it does not
    /// consume one of Cowan's four chunks — a panel that could be displaced by
    /// a busy day is not persistent, it is merely high-scoring. It carries no
    /// score and no rank, because ranking it would put it back into the
    /// competition it is exempt from.
    pub persistent: Vec<PanelInView>,
    pub selected: Vec<WidgetCandidate>,
    /// ★ Everything that did **not** make the cut, **with its score** — so the
    /// *"N other things stayed quiet"* line has something true to say. An
    /// exclusion nobody can inspect is indistinguishable from a widget that was
    /// never considered.
    pub excluded: Vec<WidgetCandidate>,
    /// ★★ Widgets that **withdrew** before ranking because nothing grounded
    /// them (UI-5). Not counted in `candidates_considered` — they were never
    /// candidates.
    pub withdrawn: Vec<Withdrawn>,
    pub candidates_considered: usize,
    pub budget: usize,
    /// Attention actually spent **by the dynamic tier**. `budget - spent` is
    /// what was left on the table; the persistent tier is not counted, because
    /// it never competed.
    pub spent: usize,
}

impl View {
    /// Everything a surface should draw, in the order it should be considered:
    /// the unconditional tier, then the ranked one.
    pub fn shown(&self) -> (&[PanelInView], &[WidgetCandidate]) {
        (&self.persistent, &self.selected)
    }
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
/// Root-name granularity, matching how `Distance::per_dimension` reports and
/// how a widget's inputs are checked at load.
fn roots_read(widget: &LoadedWidget) -> BTreeSet<&str> {
    widget.inputs().iter().map(|p| p.split(['.', '[']).next().unwrap_or(p)).collect()
}

/// ★★ Which of the dimensions a widget reads `V` says nothing about.
///
/// A widget reading only unbounded dimensions can never show urgency — and
/// that is a fact about the **household's declaration**, not about the widget
/// or the state. Reported so a zero can be told apart from calm.
fn basis_for(widget: &LoadedWidget, region: &Region) -> UrgencyBasis {
    let bounded: BTreeSet<&str> = region.intervals.iter().map(|iv| iv.dim.as_str()).collect();
    let mut undeclared: Vec<String> = roots_read(widget)
        .into_iter()
        .filter(|d| !bounded.contains(d))
        .map(str::to_string)
        .collect();
    if undeclared.is_empty() {
        UrgencyBasis::Bounded
    } else {
        undeclared.sort();
        UrgencyBasis::Undeclared { dimensions: undeclared }
    }
}

/// ★★ Does this widget have anything to say about current state? (UI-5)
///
/// Grounded when **at least one** declared input resolves to something real:
/// present, non-null, and not an empty container. An empty map or list is the
/// *nothing to say* case exactly — no pockets, no captures — and it is what
/// distinguishes a card that should withdraw from one that should read zero.
///
/// ★ A widget declaring **no** inputs is grounded: it makes no claim about
/// state, so there is nothing for state to fail to support. Refusing it would
/// be inventing a requirement the row does not make.
fn grounded(widget: &LoadedWidget, state: &State) -> bool {
    if widget.inputs().is_empty() {
        return true;
    }
    widget.inputs().iter().any(|path| match state.get(path) {
        None | Some(Value::Null) => false,
        Some(Value::Object(m)) => !m.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(_) => true,
    })
}

/// A widget's share of `d(s,V)`.
///
/// ★★ **No ratio, so no undefined denominator.** The proxy UI-7's row is about
/// divided by `allocated`, which is why zero allocation had to be special-cased
/// to `0.0` — scoring an unfunded pocket that had been spent from as calm. This
/// reads `region.distance()` and nothing else; there is no denominator that can
/// be zero, and therefore no case to special-case wrongly.
fn urgency_of(widget: &LoadedWidget, region: &Region, state: &Value) -> f64 {
    let Ok(distance) = region.distance(state) else {
        // A region that cannot be measured against this state yields no
        // urgency, rather than a fabricated one. `Skew::Unknown`'s discipline.
        return 0.0;
    };
    if distance.weighted <= 0.0 {
        return 0.0; // Inside V. Nothing is urgent.
    }
    let reads = roots_read(widget);
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

/// `score(w) = α·urgency + λ·relevance`, under the **default** declared policy.
///
/// A convenience for a caller that has not declared its own; the weights it
/// uses are still [`SaliencePolicy`]'s, so there is one place they live.
pub fn salience(urgency: f64, relevance: f64) -> f64 {
    SaliencePolicy::default().score(urgency, relevance)
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
    rank_selection(&mut selected);
    rank_selection(&mut excluded);
    (selected, excluded)
}

/// ★★★ Order a list on keys **a widget cannot set**, and declare the ties.
///
/// Three keys, in order, all un-gameable:
///
/// 1. **score** descending — the household's, via `d(s,V)` and its declared
///    [`SaliencePolicy`].
/// 2. **urgency** descending — the same signal without the relevance term, so a
///    query cannot break a tie between two genuinely urgent cards.
/// 3. **cost** ascending — **derived** from the declared inputs (UI-1), and the
///    right preference: at equal value, spend less attention.
///
/// ★★ Past those three the widgets are indistinguishable on everything the
/// household can measure, so **the tie is declared rather than broken**: they
/// share a [`rank`](WidgetCandidate::rank). The array order among them is the
/// order the set was **declared in** — the household's, never the widget's own
/// name — so renaming a widget changes neither its rank nor its position.
///
/// `partial_cmp(...).unwrap_or(Equal)` is the total-order fallback and is
/// **unreachable**: urgency is a clamp of a finite quotient, relevance a ratio
/// of counts, and `SaliencePolicy` refuses a non-finite weight, so no score can
/// be `NaN`. Named rather than left as a silent absorber.
pub fn rank_selection(items: &mut [WidgetCandidate]) {
    use std::cmp::Ordering;
    // A STABLE sort, so anything the three keys leave tied keeps the order it
    // arrived in — which is the declaration order, not an alphabetical one.
    items.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.urgency.partial_cmp(&a.urgency).unwrap_or(Ordering::Equal))
            .then_with(|| a.cost.cmp(&b.cost))
    });

    let tied = |a: &WidgetCandidate, b: &WidgetCandidate| {
        a.score == b.score && a.urgency == b.urgency && a.cost == b.cost
    };
    let mut rank = 0usize;
    for i in 0..items.len() {
        if i > 0 && !tied(&items[i - 1], &items[i]) {
            rank = i;
        }
        items[i].rank = rank;
    }
}

/// `compose(r)` — resolve a curated view.
///
/// Read-only by signature: shared references in, a `View` out, and no
/// `Registry`, so there is nothing here that could run an operator.
/// ★ Named `compose_view`, not `compose` — [`crate::compose`] is OP-5's
/// checked-composition module (`wp`, pathway chaining), a genuinely different
/// thing. Twenty-sixth collision; the newcomer takes the longer name.
/// ★ `policy` is the **household's**, passed by whoever assembles the view —
/// never read off a widget, which has no field for it. The weights are
/// argued-with, and they are argued with by the household.
pub fn compose_view(
    widgets: &WidgetSet,
    region: &Region,
    state: &Value,
    recent: &[Event],
    request: &Request,
    policy: &SaliencePolicy,
) -> View {
    compose_with_panels(widgets, &PanelSet::none(), region, state, recent, request, policy)
}

/// ★★★ `compose(r)` with the household's declared panels — the **two-tier**
/// view of CTL-8.
///
/// ```text
///   view = persistent(policy = ALWAYS)          — unconditional
///        ∪ knapsack(dynamic widgets, K ≈ 4)     — competing
/// ```
///
/// ★★ The persistent tier is **not subject to selection**: it is not scored,
/// not ranked, and does not consume budget. ★ A `WHEN_ACTIVE` panel is simply a
/// widget and joins the competing tier, which is why that policy needs no
/// machinery of its own.
///
/// ★★★ **No state is stored for any of it.** A persistent panel is not a
/// remembered identity surviving a recomposition — it is an unconditional
/// inclusion by declared policy, re-derived on every call, which is why this
/// tier fits a `compose` that deliberately keeps nothing.
#[allow(clippy::too_many_arguments)]
pub fn compose_with_panels(
    widgets: &WidgetSet,
    panels: &PanelSet,
    region: &Region,
    state: &Value,
    recent: &[Event],
    request: &Request,
    policy: &SaliencePolicy,
) -> View {
    // ── the unconditional tier ───────────────────────────────────────────────
    let persistent: Vec<PanelInView> = panels
        .persistent()
        .iter()
        .map(|w| PanelInView::of(w, panels.justification_for(w.id()).unwrap_or_default()))
        .collect();

    let mut view = compose_dynamic(widgets, region, state, recent, request, policy);

    // ★ A `WHEN_ACTIVE` panel competes exactly like a widget, so it is composed
    // through the same path and merged into the same candidate pool.
    if !panels.dynamic().is_empty() {
        let extra = compose_dynamic(panels.dynamic(), region, state, recent, request, policy);
        let mut pool: Vec<WidgetCandidate> = view
            .selected
            .into_iter()
            .chain(view.excluded)
            .chain(extra.selected)
            .chain(extra.excluded)
            .collect();
        pool.sort_by(|a, b| a.id.cmp(&b.id));
        pool.dedup_by(|a, b| a.id == b.id);
        let considered = pool.len();
        let (selected, excluded) = knapsack_select(&pool, request.budget);
        let spent = selected.iter().map(|c| c.cost).sum();
        let mut withdrawn = view.withdrawn;
        withdrawn.extend(extra.withdrawn);
        view = View {
            persistent: Vec::new(),
            selected,
            excluded,
            withdrawn,
            candidates_considered: considered,
            budget: request.budget,
            spent,
        };
    }

    view.persistent = persistent;
    view
}

fn compose_dynamic(
    widgets: &WidgetSet,
    region: &Region,
    state: &Value,
    recent: &[Event],
    request: &Request,
    policy: &SaliencePolicy,
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

    // ★★ UI-5's grounding condition, enforced BEFORE any ranking: a widget
    // with nothing to say withdraws rather than being scored low.
    let reader = State::new(state.clone());
    let mut withdrawn: Vec<Withdrawn> = Vec::new();
    let eligible: Vec<(&LoadedWidget, Eligibility)> = eligible
        .into_iter()
        .filter(|(w, _)| {
            if grounded(w, &reader) {
                true
            } else {
                withdrawn.push(Withdrawn {
                    id: w.id().to_string(),
                    reason: format!(
                        "nothing to show: none of {} resolves in current state",
                        w.inputs().join(", ")
                    ),
                });
                false
            }
        })
        .collect();

    let candidates: Vec<WidgetCandidate> = eligible
        .into_iter()
        .map(|(w, why)| {
            let urgency = urgency_of(w, region, state);
            let relevance = relevance_of(w, request.query.as_deref());
            WidgetCandidate {
                id: w.id().to_string(),
                render: w.render().to_string(),
                cost: attention_cost(w.inputs().len()),
                urgency,
                basis: basis_for(w, region),
                relevance,
                score: policy.score(urgency, relevance),
                // Assigned by `rank_selection` once the field is known; a
                // candidate cannot carry a rank it chose for itself.
                rank: 0,
                why,
            }
        })
        .collect();
    // ★★★ Deliberately NOT sorted by id. `WidgetSet` iterates in declaration
    // order, and that is the order kept here — sorting by id would hand the
    // residual tiebreak to the widget's own name, which is the lever UI-13
    // closes. The DP's tie-breaking stays reproducible because the declaration
    // order is.

    let considered = candidates.len();
    let (selected, excluded) = knapsack_select(&candidates, request.budget);
    let spent = selected.iter().map(|c| c.cost).sum();

    View {
        persistent: Vec::new(),
        selected,
        excluded,
        withdrawn,
        candidates_considered: considered,
        budget: request.budget,
        spent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::Definition;
    use crate::event::Event;
    use crate::operator::Registry;
    use crate::region::Interval;
    use crate::schema::{DimType, Schema};
    use crate::panel::PanelSet;
    use crate::widget::{WidgetDecl, WidgetSet};
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
        let view = compose_view(&set, &region(), &json!({"balance": 50.0}), &[], &Request::default(), &SaliencePolicy::default());
        assert_eq!(view.candidates_considered, 1);
        assert!(view.selected.iter().all(|c| c.id == "always"));
    }

    #[test]
    fn it_becomes_a_candidate_the_moment_its_class_appears() {
        let set = load(vec![WidgetDecl::new("on_spend", "card")
            .bound_to("event.finances.pocket_spent")
            .reading("balance")]);
        let log = [event("event.finances.pocket_spent")];
        let view = compose_view(&set, &region(), &json!({"balance": 50.0}), &log, &Request::default(), &SaliencePolicy::default());
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
        let view = compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default());

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
            &SaliencePolicy::default(),
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
            &SaliencePolicy::default(),
        );
        let breached = compose_view(
            &set,
            &region(),
            &json!({"balance": 160.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
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
        let view = compose_view(&set, &region(), &state, &[], &Request::default().with_budget(3), &SaliencePolicy::default());

        let greedy = view.excluded.iter().find(|c| c.id == "greedy");
        assert!(greedy.is_some(), "the greedy widget is excluded");
        assert_eq!(greedy.unwrap().cost, 4, "three inputs cost four chunks");
        assert_eq!(view.selected.len(), 2, "two focused widgets fit where one greedy one did not");
    }

    // ── UI-7: the blind spot, and honest zero vs manufactured zero ───────────

    /// The exact case UI-7's row worries about: a pocket with **zero
    /// allocation** that has been spent from. Under the proxy this is
    /// `spent/allocated` at zero denominator, special-cased to `0.0` — "calm".
    fn unfunded_pocket_state() -> Value {
        // Spending from an unfunded pocket drove `liquid` negative.
        json!({"balance": -40.0, "stock": 50.0, "mood": 50.0})
    }

    #[test]
    fn the_proxy_blind_spot_is_gone_because_there_is_no_denominator() {
        // ★★★ CASE A — `V` declares the wall (`balance ≥ 0`). The unfunded
        // pocket drove balance negative, so `d(s,V)` registers it and the
        // widget reading it is urgent. The metric the proxy stood in for
        // catches exactly what the proxy scored as calm.
        let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let view = compose_view(
            &set,
            &region(),
            &unfunded_pocket_state(),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        let money = &view.selected[0];
        assert!(money.urgency > 0.0, "an unfunded pocket spent from is NOT calm");
        assert_eq!(money.basis, UrgencyBasis::Bounded, "and the reading is measured");
    }

    #[test]
    fn an_undeclared_dimension_reads_zero_but_says_it_is_silence_not_safety() {
        // ★★★ CASE B — `V` declares no wall on what this widget reads. The
        // urgency is honestly 0, and the BASIS says why: nothing is declared
        // here. No wall is invented to force urgency — that would be this core
        // deciding what the household should care about.
        let unbounded = Region::new()
            .bounding(Interval::new("balance", 0.0, 100.0))
            .weighing("balance", 1.0);
        let set = load(vec![WidgetDecl::new("feelings", "card").unit().reading("mood")]);
        let view = compose_view(
            &set,
            &unbounded,
            &unfunded_pocket_state(),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        let c = view.selected.iter().chain(view.excluded.iter()).next().unwrap();
        assert_eq!(c.urgency, 0.0);
        assert_eq!(
            c.basis,
            UrgencyBasis::Undeclared { dimensions: vec!["mood".to_string()] },
            "a zero here is the household's silence, not its safety"
        );
        assert!(!c.basis.is_measured());
        assert!(c.basis.describe().contains("silence rather than safety"));
    }

    #[test]
    fn an_honest_zero_inside_v_is_told_apart_from_an_unmeasured_one() {
        // ★★ The distinction, side by side: both read 0.0, and only one of
        // them means calm.
        let set = load(vec![
            WidgetDecl::new("bounded", "card").unit().reading("balance"),
            WidgetDecl::new("unmeasured", "card").unit().reading("mood"),
        ]);
        let unbounded = Region::new()
            .bounding(Interval::new("balance", 0.0, 100.0))
            .weighing("balance", 1.0);
        let view = compose_view(
            &set,
            &unbounded,
            &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        let all: Vec<_> = view.selected.iter().chain(view.excluded.iter()).collect();
        let bounded = all.iter().find(|c| c.id == "bounded").unwrap();
        let unmeasured = all.iter().find(|c| c.id == "unmeasured").unwrap();
        assert_eq!(bounded.urgency, unmeasured.urgency, "the same number");
        assert!(bounded.basis.is_measured());
        assert!(!unmeasured.basis.is_measured(), "and a different meaning");
    }

    // ── UI-7: the weights are declared policy, not a module constant ─────────

    #[test]
    fn the_default_policy_is_the_articles_declared_preference() {
        let p = SaliencePolicy::default();
        assert_eq!((p.alpha(), p.lambda()), (0.75, 0.25));
    }

    #[test]
    fn a_household_may_argue_the_ratio() {
        let strict = SaliencePolicy::declared(0.9, 0.1).unwrap();
        let set = load(vec![WidgetDecl::new("balance_card", "card").unit().reading("balance")]);
        let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});

        let under_default = compose_view(
            &set,
            &region(),
            &state,
            &[],
            &Request::asking("weather"),
            &SaliencePolicy::default(),
        );
        let under_strict =
            compose_view(&set, &region(), &state, &[], &Request::asking("weather"), &strict);
        assert!(
            under_strict.selected[0].score > under_default.selected[0].score,
            "weighting urgency harder raises a purely-urgent widget's score"
        );
    }

    #[test]
    fn urgency_cannot_be_argued_into_second_place() {
        // ★★★ The one thing that is not arguable. A policy where a search
        // query could outrank the household being in danger is refused.
        assert_eq!(SaliencePolicy::declared(0.4, 0.6), Err(PolicyError::UrgencyNotDominant));
        assert_eq!(SaliencePolicy::declared(0.5, 0.5), Err(PolicyError::UrgencyNotDominant));
    }

    #[test]
    fn the_weights_must_sum_to_one_so_a_score_means_the_same_thing_everywhere() {
        assert_eq!(SaliencePolicy::declared(0.9, 0.9), Err(PolicyError::WeightsDoNotSumToOne));
        assert_eq!(SaliencePolicy::declared(0.3, 0.2), Err(PolicyError::WeightsDoNotSumToOne));
        assert_eq!(SaliencePolicy::declared(f64::NAN, 0.0), Err(PolicyError::NotAWeight));
    }

    #[test]
    fn a_widget_cannot_set_the_weights() {
        // The policy is a parameter of the household's view, not a field of a
        // widget — `WidgetDecl` has nowhere to put one, and `compose_view`
        // never reads one off the set. Asserted as the observable consequence:
        // the same widget scores differently only when the HOUSEHOLD changes
        // its policy.
        let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
        let a = compose_view(&set, &region(), &state, &[], &Request::default(),
                             &SaliencePolicy::default());
        let b = compose_view(&set, &region(), &state, &[], &Request::default(),
                             &SaliencePolicy::declared(0.95, 0.05).unwrap());
        assert_ne!(a.selected[0].score, b.selected[0].score);
        assert_eq!(a.selected[0].urgency, b.selected[0].urgency, "the urgency itself is untouched");
    }

    // ── the knapsack is real ─────────────────────────────────────────────────

    fn candidate(id: &str, cost: usize, score: f64) -> WidgetCandidate {
        WidgetCandidate {
            id: id.into(),
            render: "card".into(),
            cost,
            urgency: score,
            basis: UrgencyBasis::Bounded,
            rank: 0,
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

    // ── UI-13: the third salience surface — ORDERING ─────────────────────────

    /// Two widgets that tie on score, urgency and cost. Under the old
    /// behaviour the alphabetically-earlier id took the higher slot.
    fn tied_pair(first: &str, second: &str) -> WidgetSet {
        load(vec![
            WidgetDecl::new(first, "card").unit().reading("balance"),
            WidgetDecl::new(second, "card").unit().reading("stock"),
        ])
    }

    fn tied_state() -> Value {
        // balance and stock breached by the same amount, so the two widgets
        // read identical urgency.
        json!({"balance": 160.0, "stock": 160.0, "mood": 50.0})
    }

    #[test]
    fn ordering_follows_the_score_a_widget_cannot_set() {
        let set = load(vec![
            WidgetDecl::new("zzz_urgent", "card").unit().reading("balance"),
            WidgetDecl::new("aaa_calm", "card").unit().reading("mood"),
        ]);
        let view = compose_view(
            &set,
            &region(),
            &json!({"balance": 160.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        // Alphabetically `aaa_calm` comes first; by score it does not.
        assert_eq!(view.selected[0].id, "zzz_urgent");
        assert!(view.selected[0].rank < view.selected[1].rank);
    }

    #[test]
    fn renaming_a_widget_changes_neither_its_rank_nor_its_position() {
        // ★★★ THE PROPERTY. Two widgets tied on every un-gameable key. Rename
        // the second to sort first alphabetically — and nothing moves.
        let before = compose_view(
            &tied_pair("m_one", "m_two"),
            &region(),
            &tied_state(),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        let after = compose_view(
            &tied_pair("m_one", "aaa_two"),
            &region(),
            &tied_state(),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        assert_eq!(before.selected[0].id, "m_one");
        assert_eq!(after.selected[0].id, "m_one", "a rename must not stage a position");
        assert_eq!(
            before.selected.iter().map(|c| c.rank).collect::<Vec<_>>(),
            after.selected.iter().map(|c| c.rank).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_tie_is_declared_rather_than_broken() {
        // ★★ Equal rank for genuine peers: position carries no information the
        // score did not already give.
        let view = compose_view(
            &tied_pair("m_one", "m_two"),
            &region(),
            &tied_state(),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        assert_eq!(view.selected.len(), 2);
        assert_eq!(view.selected[0].score, view.selected[1].score);
        assert_eq!(view.selected[0].rank, view.selected[1].rank, "peers, and it says so");
        assert_eq!(view.selected[0].rank, 0);
    }

    #[test]
    fn the_residual_order_is_the_households_declaration_order() {
        // ★ The household CAN reorder tied peers — it declared them. The
        // widget cannot. Same two widgets, swapped in the declaration.
        let a = compose_view(
            &tied_pair("m_one", "m_two"),
            &region(),
            &tied_state(),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        let b = compose_view(
            &tied_pair("m_two", "m_one"),
            &region(),
            &tied_state(),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        assert_eq!(a.selected[0].id, "m_one");
        assert_eq!(b.selected[0].id, "m_two");
        assert_eq!(a.selected[0].rank, b.selected[0].rank, "and the rank is unmoved either way");
    }

    #[test]
    fn cost_breaks_a_score_tie_and_cost_is_derived() {
        // At equal score and urgency the cheaper widget ranks first — and cost
        // comes from the declared inputs, so a widget cannot buy the slot.
        let mut cheap = candidate("cheap", 1, 0.5);
        cheap.urgency = 0.5;
        let mut dear = candidate("dear", 4, 0.5);
        dear.urgency = 0.5;
        let mut items = vec![dear, cheap];
        rank_selection(&mut items);
        assert_eq!(items[0].id, "cheap");
        assert!(items[0].rank < items[1].rank, "not a tie: cost separated them");
    }

    #[test]
    fn urgency_breaks_a_score_tie_before_a_query_can() {
        // Two cards reaching the same score by different routes: one urgent
        // and irrelevant, one relevant and calm. Urgency wins the tiebreak, so
        // a search term cannot outrank the household's own distance-to-V.
        let mut urgent = candidate("urgent", 2, 0.5);
        urgent.urgency = 0.6;
        urgent.relevance = 0.2;
        let mut relevant = candidate("relevant", 2, 0.5);
        relevant.urgency = 0.2;
        relevant.relevance = 0.9;
        let mut items = vec![relevant, urgent];
        rank_selection(&mut items);
        assert_eq!(items[0].id, "urgent");
    }

    #[test]
    fn ranks_are_positions_not_a_dense_sequence() {
        // Two peers then a third: ranks 0, 0, 2 — so a reader can see that the
        // third is genuinely behind two, not behind one group.
        let mut a = candidate("a", 1, 0.9);
        a.urgency = 0.9;
        let mut b = candidate("b", 1, 0.9);
        b.urgency = 0.9;
        let mut c = candidate("c", 1, 0.1);
        c.urgency = 0.1;
        let mut items = vec![a, b, c];
        rank_selection(&mut items);
        assert_eq!(items.iter().map(|i| i.rank).collect::<Vec<_>>(), vec![0, 0, 2]);
    }

    // ── relevance ────────────────────────────────────────────────────────────

    #[test]
    fn relevance_is_neutral_with_no_query_and_real_with_one() {
        let set = load(vec![WidgetDecl::new("balance_card", "card").unit().reading("balance")]);
        let state = json!({"balance": 50.0});
        let quiet = compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default());
        assert_eq!(quiet.selected[0].relevance, 0.5);

        let asked = compose_view(&set, &region(), &state, &[], &Request::asking("balance"), &SaliencePolicy::default());
        assert_eq!(asked.selected[0].relevance, 1.0);

        // ★★ Zero relevance on a fully-viable sustain means a score of exactly
        // 0.0 — and a zero-value widget is NOT selected. See below.
        let unrelated = compose_view(&set, &region(), &state, &[], &Request::asking("weather"), &SaliencePolicy::default());
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
            &SaliencePolicy::default(),
        );
        assert_eq!(view.candidates_considered, 1, "it WAS considered");
        assert!(view.selected.is_empty(), "and not shown");
        assert_eq!(view.excluded[0].score, 0.0, "and it can say why");
        assert_eq!(view.spent, 0, "no attention spent on nothing");
    }

    // ── UI-5: honest empty-slate suppression — the card withdraws ────────────

    #[test]
    fn a_widget_with_nothing_to_say_withdraws_rather_than_scoring_low() {
        // ★★★ THE PROPERTY. `pockets` is an empty map — the card has nothing to
        // show. It is NOT a candidate, so it is not scored, not excluded with a
        // score, and not counted in `candidates_considered`.
        let d = Definition::new(
            Schema::new()
                .declare("balance", DimType::Number { lo: None, hi: None })
                .declare("pockets", DimType::Map { value: Box::new(DimType::Any) }),
        );
        let set = WidgetSet::load(
            vec![
                WidgetDecl::new("pockets_card", "card").unit().reading("pockets"),
                WidgetDecl::new("balance_card", "card").unit().reading("balance"),
            ],
            &d,
            &Registry::default(),
        )
        .unwrap();
        let r = Region::new().bounding(Interval::new("balance", 0.0, 100.0)).weighing("balance", 1.0);

        let view = compose_view(
            &set,
            &r,
            &json!({"balance": 50.0, "pockets": {}}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        assert_eq!(view.candidates_considered, 1, "the empty card was never a candidate");
        assert!(view.selected.iter().all(|c| c.id == "balance_card"));
        assert!(view.excluded.iter().all(|c| c.id != "pockets_card"), "not excluded — withdrawn");
        assert_eq!(view.withdrawn.len(), 1);
        assert_eq!(view.withdrawn[0].id, "pockets_card");
        assert!(view.withdrawn[0].reason.contains("nothing to show"));
    }

    #[test]
    fn the_same_card_appears_the_moment_it_has_something_to_say() {
        let d = Definition::new(
            Schema::new().declare("pockets", DimType::Map { value: Box::new(DimType::Any) }),
        );
        let set = WidgetSet::load(
            vec![WidgetDecl::new("pockets_card", "card").unit().reading("pockets")],
            &d,
            &Registry::default(),
        )
        .unwrap();
        let r = Region::new();

        let empty = compose_view(&set, &r, &json!({"pockets": {}}), &[], &Request::default(),
                                 &SaliencePolicy::default());
        assert!(empty.selected.is_empty() && empty.withdrawn.len() == 1);

        let filled = compose_view(&set, &r, &json!({"pockets": {"food": {}}}), &[],
                                  &Request::default(), &SaliencePolicy::default());
        assert!(filled.withdrawn.is_empty());
        assert_eq!(filled.selected.len(), 1);
    }

    #[test]
    fn withdrawal_is_a_different_answer_from_exclusion() {
        // ★★ "Never had anything to say" and "considered and outranked" are
        // different facts, and the view keeps them in different lists — an
        // excluded card carries a score, a withdrawn one carries a reason.
        let d = Definition::new(
            Schema::new()
                .declare("balance", DimType::Number { lo: None, hi: None })
                .declare("pockets", DimType::Map { value: Box::new(DimType::Any) }),
        );
        let set = WidgetSet::load(
            vec![
                WidgetDecl::new("empty", "card").unit().reading("pockets"),
                WidgetDecl::new("loud", "card").unit().reading("balance"),
                WidgetDecl::new("quiet", "card").unit().reading("balance"),
            ],
            &d,
            &Registry::default(),
        )
        .unwrap();
        let r = Region::new().bounding(Interval::new("balance", 0.0, 100.0)).weighing("balance", 1.0);
        let view = compose_view(
            &set,
            &r,
            &json!({"balance": 900.0, "pockets": {}}),
            &[],
            &Request::default().with_budget(1),
            &SaliencePolicy::default(),
        );
        assert_eq!(view.withdrawn.len(), 1, "one had nothing to say");
        assert_eq!(view.excluded.len(), 1, "one was outranked");
        assert!(view.excluded[0].score > 0.0, "and it can say what it scored");
    }

    #[test]
    fn a_widget_declaring_no_inputs_is_grounded_by_default() {
        // ★ It makes no claim about state, so there is nothing for state to
        // fail to support. Refusing it would invent a requirement.
        let d = Definition::new(Schema::new());
        let set =
            WidgetSet::load(vec![WidgetDecl::new("static", "card").unit()], &d, &Registry::default())
                .unwrap();
        let view = compose_view(&set, &Region::new(), &json!({}), &[], &Request::default(),
                                &SaliencePolicy::default());
        assert!(view.withdrawn.is_empty());
        assert_eq!(view.candidates_considered, 1);
    }

    #[test]
    fn a_null_or_absent_input_does_not_ground_a_widget() {
        let d = Definition::new(Schema::new().declare("maybe", DimType::Any));
        let set = WidgetSet::load(
            vec![WidgetDecl::new("card", "card").unit().reading("maybe")],
            &d,
            &Registry::default(),
        )
        .unwrap();
        for state in [json!({}), json!({"maybe": null}), json!({"maybe": []})] {
            let view = compose_view(&set, &Region::new(), &state, &[], &Request::default(),
                                    &SaliencePolicy::default());
            assert_eq!(view.withdrawn.len(), 1, "state {state} should withdraw the card");
        }
    }

    #[test]
    fn grounding_does_not_apply_to_the_persistent_tier() {
        // ★ Deliberate: ALWAYS means always. A console that reads a dimension
        // current state happens not to carry is still shown — the household
        // declared it unconditional, and *nothing is here yet* is an answer.
        let d = Definition::new(Schema::new().declare("mood", DimType::Any));
        let p = PanelSet::load(
            vec![crate::panel::PanelDecl::always(
                WidgetDecl::new("console", "console").unit().reading("mood"),
                "always informative",
            )],
            &d,
            &Registry::default(),
        )
        .unwrap();
        let view = compose_with_panels(
            &WidgetSet::empty(),
            &p,
            &Region::new(),
            &json!({}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        assert_eq!(view.persistent.len(), 1);
        assert!(view.withdrawn.is_empty(), "the persistent tier is not filtered");
    }

    // ── CTL-8: the persistent tier, outside the attention budget ─────────────

    fn console_panel() -> crate::panel::PanelDecl {
        crate::panel::PanelDecl::always(
            WidgetDecl::new("console", "console").unit().reading("mood"),
            "the console answers 'is anything running' in every state, not only a bad one",
        )
    }

    fn panels(decls: Vec<crate::panel::PanelDecl>) -> PanelSet {
        PanelSet::load(decls, &definition(), &Registry::default()).unwrap()
    }

    #[test]
    fn a_persistent_panel_is_shown_against_a_full_urgent_budget() {
        // ★★★ THE PROPERTY. Budget 1, and a maximally-urgent widget takes it.
        // The panel is there anyway, because it never competed.
        let dynamic = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let view = compose_with_panels(
            &dynamic,
            &panels(vec![console_panel()]),
            &region(),
            &json!({"balance": 900.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default().with_budget(1),
            &SaliencePolicy::default(),
        );
        assert_eq!(view.selected.len(), 1, "the budget is full");
        assert_eq!(view.selected[0].id, "money");
        assert_eq!(view.persistent.len(), 1, "and the panel is shown anyway");
        assert_eq!(view.persistent[0].id, "console");
    }

    #[test]
    fn the_persistent_tier_does_not_consume_budget() {
        // `spent` counts the dynamic tier alone — a panel that could eat a
        // chunk could displace a widget, which is exactly what it must not do.
        let dynamic = load(vec![
            WidgetDecl::new("a", "card").unit().reading("balance"),
            WidgetDecl::new("b", "card").unit().reading("stock"),
        ]);
        let with_panel = compose_with_panels(
            &dynamic,
            &panels(vec![console_panel()]),
            &region(),
            &json!({"balance": 900.0, "stock": 900.0, "mood": 50.0}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        let without = compose_with_panels(
            &dynamic,
            &PanelSet::none(),
            &region(),
            &json!({"balance": 900.0, "stock": 900.0, "mood": 50.0}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        assert_eq!(with_panel.spent, without.spent, "the panel cost nothing");
        assert_eq!(
            with_panel.selected.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            without.selected.iter().map(|c| c.id.clone()).collect::<Vec<_>>(),
            "and displaced nobody"
        );
    }

    #[test]
    fn a_persistent_panel_is_shown_when_nothing_is_wrong() {
        // The point of I(p) being high for ALL states: "nothing is wrong" is
        // itself the answer the panel gives.
        let view = compose_with_panels(
            &load(vec![]),
            &panels(vec![console_panel()]),
            &region(),
            &json!({"balance": 50.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        assert!(view.selected.is_empty(), "nothing dynamic earns a slot");
        assert_eq!(view.persistent.len(), 1, "and the panel is still there");
    }

    #[test]
    fn a_persistent_panel_carries_no_score_and_no_rank() {
        // ★★ Structural: `PanelInView` has neither field. Ranking it would put
        // it back into the competition it is exempt from.
        let view = compose_with_panels(
            &load(vec![]),
            &panels(vec![console_panel()]),
            &region(),
            &json!({"balance": 50.0}),
            &[],
            &Request::default(),
            &SaliencePolicy::default(),
        );
        let rendered = format!("{:?}", view.persistent[0]);
        assert!(!rendered.contains("score"));
        assert!(!rendered.contains("rank"));
        assert!(view.persistent[0].justification.contains("every state"));
    }

    #[test]
    fn a_when_active_panel_competes_like_any_widget() {
        // ★ The other half of the policy, and it needs no machinery: it is a
        // widget, so it is eligible through β and selected through the
        // knapsack. Here it loses to a more urgent one under a budget of 1.
        let quiet = crate::panel::PanelDecl::when_active(
            WidgetDecl::new("quiet", "card").unit().reading("mood"),
        );
        let dynamic = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let view = compose_with_panels(
            &dynamic,
            &panels(vec![quiet]),
            &region(),
            &json!({"balance": 900.0, "stock": 50.0, "mood": 50.0}),
            &[],
            &Request::default().with_budget(1),
            &SaliencePolicy::default(),
        );
        assert!(view.persistent.is_empty(), "WHEN_ACTIVE is not the persistent tier");
        assert_eq!(view.candidates_considered, 2, "but it did compete");
        assert_eq!(view.selected[0].id, "money");
        assert!(view.excluded.iter().any(|c| c.id == "quiet"));
    }

    #[test]
    fn a_widget_cannot_declare_itself_persistent() {
        // ★★★ Unspellable: `WidgetDecl` has no policy field, so persistence is
        // a wrapper only the household constructs. The observable consequence
        // is that an identical widget declaration lands in whichever tier the
        // HOUSEHOLD put it in, and in neither by its own doing.
        let w = WidgetDecl::new("same", "card").unit().reading("mood");
        let as_panel = panels(vec![crate::panel::PanelDecl::always(w.clone(), "declared always")]);
        let as_widget = panels(vec![crate::panel::PanelDecl::when_active(w)]);
        assert!(as_panel.is_persistent("same"));
        assert!(!as_widget.is_persistent("same"));
    }

    #[test]
    fn composing_twice_with_panels_gives_the_same_view() {
        // ★★ No stored state: the persistent tier is re-derived, not
        // remembered, which is why it fits a compose that keeps nothing.
        let p = panels(vec![console_panel()]);
        let dynamic = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let state = json!({"balance": 900.0, "stock": 50.0, "mood": 50.0});
        let one = compose_with_panels(&dynamic, &p, &region(), &state, &[], &Request::default(),
                                      &SaliencePolicy::default());
        let two = compose_with_panels(&dynamic, &p, &region(), &state, &[], &Request::default(),
                                      &SaliencePolicy::default());
        assert_eq!(one, two);
    }

    // ── read-only ────────────────────────────────────────────────────────────

    #[test]
    fn compose_changes_nothing() {
        let set = load(vec![WidgetDecl::new("money", "card").unit().reading("balance")]);
        let state = json!({"balance": 160.0, "stock": 50.0, "mood": 50.0});
        let log = vec![event("event.finances.pocket_spent")];

        let before_state = state.clone();
        let before_log = log.clone();
        let _ = compose_view(&set, &region(), &state, &log, &Request::default(), &SaliencePolicy::default());
        let _ = compose_view(&set, &region(), &state, &log, &Request::default(), &SaliencePolicy::default());
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
        let one = compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default());
        let two = compose_view(&set, &region(), &state, &[], &Request::default(), &SaliencePolicy::default());
        assert_eq!(one, two, "resolved fresh, and deterministically");
    }
}
