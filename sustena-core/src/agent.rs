//! `ω` **assembled** — the operative with its faculties on, and using them.
//!
//! OPV-1 built `ω`'s boundary: `⟨u_i, dom_i⟩` and, load-bearingly, **no `S` and
//! no `T`**. `Π` (OPV-4), attention (OPV-7), `M_self`/`M_world` (OPV-11) and
//! the meme library (OPV-6) were each left off with the same stated reason —
//! *a declared-but-inert field reads as built*. All four now exist, so the
//! wiring is legitimate. But the discipline runs in reverse from here: a field
//! `ω` merely stores is **the same defect, committed instead of dodged**.
//!
//! So this module does not add optional fields to [`Operative`]. It composes an
//! [`Agent`] whose faculties are **required** — no `Option`, no
//! partially-assembled constructor — and every one of them is read on the one
//! reasoning path below.
//!
//! ## ★★★ Prop 1 and Prop 2 survive: an `Agent` still has no `S` and no `T`
//!
//! [`Agent`] holds an [`Operative`], an [`Attention`] and a [`MemeLibrary`] —
//! **and no [`Shared`]**. Every method that needs the world takes it as an
//! argument, so an agent still cannot carry a private one, and
//! `Shared::reachable` still is not given the population. The sharing
//! constraint stays unrepresentable to violate rather than checked.
//!
//! ## ★★ One path, not five bolt-ons
//!
//! [`Agent::act`] is the whole reasoning turn and it uses three faculties in
//! order:
//!
//! 1. **attention** — [`scan`] with the narrow aperture over the real `⊕` tree,
//!    [`narrow_scan`] to rank what it reached by `d(s,V)`, [`broad_scan`] for
//!    whether the frame moved somewhere the task was not looking;
//! 2. **the decision** — ★ if broad attention fires, the turn ends as
//!    [`Reasoning::Reframed`] and **`Π` does not run**. A chosen semantic, and
//!    named as one: the alternative was to compute a [`Reframing`] and drop it,
//!    which would make broad attention exactly the inert faculty this module
//!    exists to avoid. *The frame you were about to reason in has changed* is a
//!    reason to stop, not a footnote to carry;
//! 3. **`Π`** — the top-scoring focus becomes the strategy's `trigger_event`,
//!    and the meme runs through [`run`], which goes through the real gate node
//!    by node.
//!
//! Attention therefore **filters and parameterises** what `Π` sees, rather than
//! being computed beside it.
//!
//! ## Reconcile with the router
//!
//! [`crate::mixture`] decides **which** operative acts, and `Dispatch::run` is
//! where the expensive path happens — `f` is applied to the selected tickets
//! and to nothing else. [`Agent::act`] **is** what such an `f` calls, and
//! [`Agent::operative`] hands back the `&Operative` a `Contender` borrows. Two
//! halves of one path: the router selects, the agent reasons. Nothing here
//! routes, for the reason `strategy`'s own header gives — *a strategy that also
//! routed would be choosing its own author*.
//!
//! ## `M_self` is DERIVED, never stored
//!
//! [`Agent::self_model`] calls [`SelfModel::of`] on the agent's own parts. A
//! stored copy could drift from the library it describes; a derivation cannot.
//!
//! ## ★★★ `M_world` is the household's, not the agent's
//!
//! §VII is explicit: **one `M_world` per household**, forked per councillor. So
//! the model hangs on [`crate::operative::Omega`] — the population that already
//! *owns the one world* — and an [`Agent`] has **no `WorldModel` field at
//! all**. That is not a gap; it is the claim. And it is used rather than held:
//! `Omega::agreement_over` returns `EffectiveN::BoundedAbove` when a shared
//! model is declared and `EffectiveN::Independent` when one genuinely is not,
//! so OPV-11's keystone fires off real population state.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::attention::{broad_scan, narrow_scan, scan, Attention, Reframing, Scan, Score};
use crate::learning::{learn, Feedback, LearningRound, LibraryError, MemeLibrary, VaryOp};
use crate::models::SelfModel;
use crate::monitor::MonitorEngine;
use crate::operative::{Operative, Shared};
use crate::operator::{Enforcement, Registry};
use crate::region::Region;
use crate::strategy::{run, Walk};

/// What one attend-then-reason turn produced.
///
/// ★ Named `Reasoning`, not `Act` — `ooda::OodaPhase::Act` is the *do it* phase
/// after a human authorised, a genuinely different thing to confuse a reasoning
/// turn with. Thirty-second collision; the newcomer takes the different name.
#[derive(Debug, Clone, PartialEq)]
pub enum Reasoning {
    /// ★ Broad attention fired: the frame moved somewhere the task was not
    /// looking, so the strategy **did not run**. Reasoning inside a frame you
    /// have just been told is wrong is the failure mode OPV-7's two output
    /// types exist to prevent.
    Reframed(Reframing),
    /// The scan reached nothing scorable — no focus, so nothing to reason
    /// about. ★ The third answer: *nothing was in view* is not *the strategy
    /// found nothing*.
    NothingInView { scanned: usize },
    /// The library has no such meme. Reported rather than silently skipped.
    NoSuchMeme { id: String },
    /// `Π` ran. `focus` is the sustain narrow attention ranked worst-first, and
    /// its `urgency` is `d(s,V)` — the one urgency, not a rival.
    Reasoned { focus: String, urgency: f64, walk: Box<Walk> },
}

impl Reasoning {
    /// Whether a strategy actually ran.
    pub fn ran(&self) -> bool {
        matches!(self, Reasoning::Reasoned { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Reasoning::Reframed(r) => match r {
                Reframing::FrameChanged { at, why } => {
                    format!("did not reason: the frame changed at '{at}' — {why}")
                }
                Reframing::FrameHolds => "did not reason: the frame holds".to_string(),
            },
            Reasoning::NothingInView { scanned } => {
                format!("scanned {scanned} and found nothing scorable")
            }
            Reasoning::NoSuchMeme { id } => format!("no meme '{id}' in this agent's library"),
            Reasoning::Reasoned { focus, urgency, walk } => {
                format!("reasoned about '{focus}' (d(s,V) = {urgency}): {:?}", walk.outcome)
            }
        }
    }
}

/// What attention alone saw, before any strategy ran.
///
/// Returned by [`Agent::consider`] so the filtering step is inspectable on its
/// own — [`Agent::act`] uses exactly this and adds nothing to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Considered {
    /// The narrow aperture's footprint.
    pub scan: Scan,
    /// Ranked worst-first by `d(s,V)`. ★ [`Score`] has no public constructor,
    /// so an agent cannot mint one — the urgency is the household's.
    pub ranked: Vec<Score>,
    /// What broad attention found outside the task's own branch.
    pub reframing: Reframing,
}

impl Considered {
    /// The one thing narrow attention would have `Π` look at.
    pub fn focus(&self) -> Option<&Score> {
        self.ranked.first()
    }
}

/// `ω` with its faculties on.
///
/// ★★ **Every faculty is required.** There is no `Option`, no default and no
/// partially-assembled constructor: an agent that could exist without attention
/// would be OPV-7's half-mind, and one without a library could not learn — so
/// both would be fields a method might find missing, which is the inert shape
/// in a different costume.
///
/// ★★★ **No `S`, no `T`.** OPV-1's absence is preserved exactly: the world is
/// an argument to every method that needs it, never a field.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    operative: Operative,
    attention: Attention,
    library: MemeLibrary,
}

impl Agent {
    /// Assemble. All three parts, or no agent.
    pub fn assemble(operative: Operative, attention: Attention, library: MemeLibrary) -> Agent {
        Agent { operative, attention, library }
    }

    /// The `⟨u_i, dom_i⟩` half — what `mixture::Contender` borrows.
    pub fn operative(&self) -> &Operative {
        &self.operative
    }

    pub fn id(&self) -> &str {
        self.operative.id()
    }

    /// `⟨narrow, broad⟩`.
    pub fn attention(&self) -> &Attention {
        &self.attention
    }

    /// `M` — this agent's own strategies. Scoped, per OPV-6's NFL discipline.
    pub fn library(&self) -> &MemeLibrary {
        &self.library
    }

    // ── attention ────────────────────────────────────────────────────────────

    /// Look, with both apertures.
    ///
    /// ★ Narrow ranks by `d(s,V)` via [`narrow_scan`], which is the only way to
    /// obtain a [`Score`]. Broad ignores what the task was already looking at,
    /// so it reports what narrow is *not* covering.
    pub fn consider(
        &self,
        engine: &MonitorEngine,
        from: &str,
        region: &Region,
        task_branch: &BTreeSet<&str>,
        state_of: impl Fn(&str) -> Option<Value>,
    ) -> Considered {
        let narrow = scan(engine, from, self.attention.narrow());
        let ranked = narrow_scan(&narrow, region, &state_of);

        let broad = scan(engine, from, self.attention.broad());
        let reframing = broad_scan(&broad, region, task_branch, &state_of);

        Considered { scan: narrow, ranked, reframing }
    }

    // ── the reasoning turn ───────────────────────────────────────────────────

    /// ★★ **The one path**: attend, decide, then run `Π`.
    ///
    /// See the module header for why a fired [`Reframing`] stops the turn.
    /// Every node of the strategy still goes through the real gate — this adds
    /// no bypass, it only chooses *whether* and *about what* to run.
    #[allow(clippy::too_many_arguments)]
    pub fn act(
        &self,
        meme_id: &str,
        engine: &MonitorEngine,
        from: &str,
        region: &Region,
        task_branch: &BTreeSet<&str>,
        state_of: impl Fn(&str) -> Option<Value>,
        registry: &Registry,
        allowed: &[String],
        enforcement: &Enforcement,
        state: &Value,
    ) -> Reasoning {
        let considered = self.consider(engine, from, region, task_branch, &state_of);

        // 2 — broad attention has the first word.
        if considered.reframing.triggered() {
            return Reasoning::Reframed(considered.reframing);
        }

        let Some(focus) = considered.focus() else {
            return Reasoning::NothingInView { scanned: considered.scan.visited.len() };
        };
        let (focus_id, urgency) = (focus.sustain_id().to_string(), focus.value());

        let Some(meme) = self.library.get(meme_id) else {
            return Reasoning::NoSuchMeme { id: meme_id.to_string() };
        };

        // 3 — what attention chose becomes what Π is triggered with.
        let mut trigger = Map::new();
        trigger.insert("focus".to_string(), Value::String(focus_id.clone()));
        trigger.insert(
            "urgency".to_string(),
            serde_json::Number::from_f64(urgency).map_or(Value::Null, Value::Number),
        );

        let walk =
            run(meme.strategy(), registry, allowed, enforcement, state, Value::Object(trigger));

        Reasoning::Reasoned { focus: focus_id, urgency, walk: Box::new(walk) }
    }

    // ── learning ─────────────────────────────────────────────────────────────

    /// One round of `M(t+1) = retain(select_{u,Viab}(vary(M(t))))` over **this
    /// agent's own** library.
    ///
    /// ★ Takes the [`Feedback`] by value, so the *results-not-a-clock* rule
    /// arrives here intact: an agent cannot be asked to learn on a timer,
    /// because there is no timer-shaped thing to hand it.
    #[allow(clippy::too_many_arguments)]
    pub fn learn_from(
        &mut self,
        meme_id: &str,
        world: &Shared,
        viable: &Region,
        ops: &[VaryOp],
        registry: &Registry,
        allowed: &[String],
        enforcement: &Enforcement,
        state: &Value,
        feedback: Feedback,
    ) -> Result<LearningRound, LibraryError> {
        learn(
            &mut self.library,
            meme_id,
            world,
            &self.operative,
            viable,
            ops,
            registry,
            allowed,
            enforcement,
            state,
            feedback,
        )
    }

    // ── M_self ───────────────────────────────────────────────────────────────

    /// `M_self` over one of this agent's memes — **derived, never stored**.
    ///
    /// ★ `None` for a meme this agent does not have: a self-model of a strategy
    /// you do not hold is not a model of yourself.
    pub fn self_model(&self, meme_id: &str, boundary: &[&str]) -> Option<SelfModel> {
        let meme = self.library.get(meme_id)?;
        Some(SelfModel::of(&self.operative, meme.strategy(), &self.attention, boundary))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attention::Aperture;
    use crate::detect::CusumSpec;
    use crate::ensemble::ModelTemplate;
    use crate::monitor::SustainWatch;
    use crate::learning::{Fitness, LibraryScope};
    use crate::models::{EffectiveN, Fidelity, WorldModel};
    use crate::operative::{Cynefin, Objective, Omega, Sense, Utility};
    use crate::region::Interval;
    use crate::strategy::StrategyGraph;
    use serde_json::json;

    fn world() -> Shared {
        Shared::new()
            .with_dimension("finances.liquid.balance")
            .with_move("budget.allocate")
            .with_move("budget.record_income")
    }

    fn operative() -> Operative {
        let u = Utility::new()
            .with(Objective::new("liquidity", "finances.liquid.balance", Sense::Maximise))
            .unwrap();
        Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
    }

    fn attention() -> Attention {
        Attention::declared(
            Aperture::declared(2, 3, 2).unwrap(),
            Aperture::declared(12, 1, 1).unwrap(),
            100,
        )
        .unwrap()
    }

    fn base() -> StrategyGraph {
        let mut kw = Map::new();
        kw.insert("amount".into(), json!(100.0));
        kw.insert("source".into(), json!("salary"));
        StrategyGraph::new("a", "a").with_node(&world(), "a", "budget.record_income", kw).unwrap()
    }

    fn agent() -> Agent {
        let mut lib = MemeLibrary::of(LibraryScope::of("household", "mentor"));
        lib.author("m0", base()).unwrap();
        Agent::assemble(operative(), attention(), lib)
    }

    fn region() -> Region {
        Region::new().bounding(Interval::at_least("finances.liquid.balance", 0.0))
    }

    fn state() -> Value {
        json!({"finances": {
            "liquid": {"balance": 500.0},
            "pockets": {},
            "income": {"monthly_total": 0.0, "sources": []}
        }})
    }

    fn allowed() -> Vec<String> {
        vec!["budget.allocate".to_string(), "budget.record_income".to_string()]
    }

    fn allocate_kwargs() -> Map<String, Value> {
        let mut k = Map::new();
        k.insert("pocket_name".into(), json!("food"));
        k.insert("amount".into(), json!(50.0));
        k
    }

    /// A household: root with two real children, so the scan reaches something.
    fn engine() -> MonitorEngine {
        let spec = CusumSpec::new(0.0, 1.0, 5.0);
        MonitorEngine::flatten_holarchy(vec![
            SustainWatch::new("household", region(), 0.3, spec),
            SustainWatch::new("bonnie", region(), 0.3, spec).under("household"),
            SustainWatch::new("cira", region(), 0.3, spec).under("household"),
        ])
        .unwrap()
    }

    fn healthy(id: &str) -> Option<Value> {
        match id {
            "bonnie" => Some(json!({"finances": {"liquid": {"balance": 10.0}}})),
            "cira" => Some(json!({"finances": {"liquid": {"balance": 900.0}}})),
            _ => None,
        }
    }

    // ── the faculties are genuinely used ─────────────────────────────────────

    #[test]
    fn attention_ranks_by_the_households_own_urgency() {
        let a = agent();
        let c = a.consider(&engine(), "household", &region(), &BTreeSet::new(), healthy);
        assert!(!c.ranked.is_empty(), "the narrow aperture reached something");
        // Both children are inside V here, so every score is 0 — the point is
        // that the values came from `Region::distance` and nowhere else.
        assert!(c.ranked.iter().all(|s| s.value() == 0.0));
    }

    #[test]
    fn a_reasoning_turn_runs_the_agents_own_strategy_through_the_gate() {
        let a = agent();
        let out = a.act(
            "m0",
            &engine(),
            "household",
            &region(),
            &BTreeSet::new(),
            healthy,
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
        );
        match out {
            Reasoning::Reasoned { ref walk, .. } => {
                assert!(walk.outcome.finished(), "{:?}", walk.outcome);
                assert!(walk.state["finances"]["liquid"]["balance"].as_f64().unwrap() > 500.0);
            }
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn attention_parameterises_what_the_strategy_is_triggered_with() {
        let a = agent();
        let out = a.act(
            "m0",
            &engine(),
            "household",
            &region(),
            &BTreeSet::new(),
            healthy,
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
        );
        match out {
            Reasoning::Reasoned { focus, ref walk, .. } => {
                let trigger = &walk.accumulated["trigger_event"];
                assert_eq!(trigger["focus"], Value::String(focus));
            }
            other => panic!("{}", other.describe()),
        }
    }

    #[test]
    fn a_changed_frame_stops_the_turn_before_the_strategy_runs() {
        // ★ Broad attention has the first word. `cira` is outside V and outside
        // the task's branch, so the frame moved.
        let a = agent();
        let breached = |id: &str| match id {
            "bonnie" => Some(json!({"finances": {"liquid": {"balance": 10.0}}})),
            "cira" => Some(json!({"finances": {"liquid": {"balance": -50.0}}})),
            _ => None,
        };
        let task: BTreeSet<&str> = ["bonnie"].into_iter().collect();
        let out = a.act(
            "m0",
            &engine(),
            "household",
            &region(),
            &task,
            breached,
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
        );
        assert!(matches!(out, Reasoning::Reframed(_)), "{}", out.describe());
        assert!(!out.ran(), "and Π did not run");
    }

    #[test]
    fn an_unknown_meme_is_reported_not_silently_skipped() {
        let a = agent();
        let out = a.act(
            "nope",
            &engine(),
            "household",
            &region(),
            &BTreeSet::new(),
            healthy,
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
        );
        assert!(matches!(out, Reasoning::NoSuchMeme { .. }));
    }

    #[test]
    fn nothing_in_view_is_a_distinct_answer_from_a_strategy_finding_nothing() {
        let a = agent();
        // A leaf: the scan reaches no children at all.
        let out = a.act(
            "m0",
            &engine(),
            "bonnie",
            &region(),
            &BTreeSet::new(),
            healthy,
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
        );
        assert!(matches!(out, Reasoning::NothingInView { scanned: 0 }), "{}", out.describe());
    }

    #[test]
    fn an_agent_learns_over_its_own_library() {
        let mut a = agent();
        let before = a.library().len();
        let round = a
            .learn_from(
                "m0",
                &world(),
                &region(),
                &[VaryOp::Optimise {
                    node: "a".into(),
                    param: "amount".into(),
                    value: json!(250.0),
                }],
                &Registry::default(),
                &allowed(),
                &Enforcement::default(),
                &state(),
                Feedback::Correction { by: "bonnie".into(), note: "too small".into() },
            )
            .unwrap();
        assert!(round.changed_the_library());
        assert_eq!(a.library().len(), before + 1);
        let kept = a.library().get(&round.retention.kept[0]).unwrap();
        assert!(matches!(kept.retained_by(), Some(Fitness::Viable { .. })));
    }

    #[test]
    fn what_it_learned_is_immediately_reasonable_with() {
        // ★ The library it learned into is the library it reasons from — one
        // object, so there is no sync step that could be forgotten.
        let mut a = agent();
        let round = a
            .learn_from(
                "m0",
                &world(),
                &region(),
                &[VaryOp::Extend {
                    node: "b".into(),
                    mv: "budget.allocate".into(),
                    after: "a".into(),
                    kwargs: allocate_kwargs(),
                }],
                &Registry::default(),
                &allowed(),
                &Enforcement::default(),
                &state(),
                Feedback::Objection { by: "curator".into(), note: "allocate it".into() },
            )
            .unwrap();
        let learned = round.retention.kept[0].clone();
        let out = a.act(
            &learned,
            &engine(),
            "household",
            &region(),
            &BTreeSet::new(),
            healthy,
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
        );
        assert!(out.ran(), "{}", out.describe());
    }

    // ── M_self, derived ──────────────────────────────────────────────────────

    #[test]
    fn the_self_model_is_derived_from_the_agents_own_parts() {
        let a = agent();
        let m = a.self_model("m0", &["household"]).unwrap();
        assert_eq!(m.operative_id(), "mentor");
        assert_eq!(m.attention_budget(), a.attention().cost());
        assert_eq!(m.objectives(), a.operative().utility().m());
        assert!(a.self_model("nope", &["household"]).is_none());
    }

    #[test]
    fn the_self_model_still_moves_within_the_shared_world_after_learning() {
        // ★★★ CELL's recursion, through the assembled agent.
        let mut a = agent();
        let round = a
            .learn_from(
                "m0",
                &world(),
                &region(),
                &[VaryOp::Extend {
                    node: "b".into(),
                    mv: "budget.allocate".into(),
                    after: "a".into(),
                    kwargs: allocate_kwargs(),
                }],
                &Registry::default(),
                &allowed(),
                &Enforcement::default(),
                &state(),
                Feedback::Correction { by: "b".into(), note: "n".into() },
            )
            .unwrap();
        let m = a.self_model(&round.retention.kept[0], &["household"]).unwrap();
        assert!(m.moves().contains("budget.allocate"));
        assert!(m.moves_within(&world()));
    }

    // ── M_world, the household's ─────────────────────────────────────────────

    fn household_model() -> WorldModel {
        WorldModel::declared(
            "household",
            &["household"],
            &["finances.liquid.balance"],
            region(),
            ModelTemplate::new().certain("s0", "budget.allocate", "s1"),
            Fidelity::claimed(0.6, "declared from the household's own spec").unwrap(),
        )
    }

    #[test]
    fn the_world_model_hangs_on_the_population_not_on_an_agent() {
        // ★★★ One per household. `Agent` has no `WorldModel` field, and the
        // only way to reach one is through `Omega`.
        let omega = Omega::over(world()).with(operative()).unwrap().modelling(household_model());
        assert_eq!(omega.world_model().map(|m| m.id()), Some("household"));
    }

    #[test]
    fn several_agents_sharing_one_model_are_not_independent_confirmations() {
        // ★★★ OPV-11's keystone, fired off real population state.
        let shared = Omega::over(world()).with(operative()).unwrap().modelling(household_model());
        assert!(matches!(
            shared.agreement_over(5),
            EffectiveN::BoundedAbove { headcount: 5, .. }
        ));

        let unmodelled = Omega::over(world()).with(operative()).unwrap();
        assert_eq!(unmodelled.agreement_over(5), EffectiveN::Independent(5));
    }
}
