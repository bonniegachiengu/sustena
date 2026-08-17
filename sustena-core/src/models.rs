//! `M_self` and `M_world` — the two models an operative carries (OPV-11; the
//! Operative paper, §VII).
//!
//! > Every good regulator of a system must be a model of that system.
//! > — Conant & Ashby, 1970
//!
//! ## ★★ What this row FRAMES rather than builds
//!
//! `M_world`'s engine already existed, in two pieces, and neither is rebuilt:
//!
//! - [`crate::tenet::TransitionModel`] is `S × T → Δ(S)` — §VII's declared type
//!   for `T̂`, with [`crate::tenet::backward_induct`] over it.
//! - ★★★ [`crate::ensemble::ModelTemplate`] is **already the declared-structure
//!   / learned-residual split**: the template declares which `(state, action) →
//!   next` edges exist, a [`Scenario`](crate::ensemble::Scenario) binds the
//!   probabilities, and `instantiate` puts them together. Structure declared,
//!   numbers swappable — which is exactly §VII's *starts declared, only the
//!   residual is learned*, built before this row for a different reason.
//!
//! So [`WorldModel`] is a **framing**: it bundles the template, the region and
//! the scope into `Σ̂ = ⟨B̂, Ŝ, V̂, T̂⟩` and attaches the two things the pieces
//! did not carry — a **fidelity claim** and an **identity**, the second being
//! what makes the correlation below detectable.
//!
//! ## ★★★ How an operative models a world it cannot hold
//!
//! OPV-1 kept `⟨S,T⟩` **out** of `ω` deliberately: that absence *is* the
//! sharing constraint. So `M_world` cannot be the world — and §VII does not ask
//! it to be. It is `Σ̂`, an **approximate copy**: a separate, declared object
//! whose `Ŝ`, `V̂` and `T̂` are *authored* and merely **consistent with**
//! [`Shared`], never a handle on it.
//!
//! ★ [`WorldModel::agrees_with`] is how that consistency is checked, and it is
//! deliberately a **report rather than a constructor guard**: a model may name
//! moves the world does not have — that is precisely what *the model is wrong*
//! looks like — and refusing to build it would hide the disagreement this row
//! exists to surface.
//!
//! ## ★★ A declared model is brittle, and that is a property
//!
//! > Easier and more brittle: a declared model is wrong in ways it cannot
//! > notice, because its structure is not up for revision by data.
//!
//! [`WorldModel::structure_is_declared`] is a constant `true`, and it says so
//! in its own name. Only the **parameters** move — a new [`Scenario`] rebinds
//! probabilities and the edge set is untouched. So a missing edge stays missing
//! however much data arrives, and the model **cannot notice** it. Named as a
//! property, not hidden as a defect.
//!
//! [`Scenario`]: crate::ensemble::Scenario
//!
//! ## §VII's three limits
//!
//! 1. **Every model is lossy** — so `M_world` carries a [`Fidelity`] claim, and
//!    ★ **the gate does not consult it.** There is no path from a fidelity claim
//!    to an admission: `operator::execute` takes no model. A model that
//!    believes it is accurate changes nothing about what commits.
//! 2. **Dimensions outside `M_world` are unregulated** — [`WorldModel::blind_to`]
//!    names them against a declared schema, and the guard against optimising
//!    into them is [`crate::goodhart`]'s, already built. Reconciled, not
//!    duplicated.
//! 3. **Model error compounds with the horizon** — so [`Projection`] carries the
//!    **horizon with the score**, and there is no accessor returning the value
//!    alone. A number without its horizon invites a search to exploit the
//!    model's error at depth and call it a plan.
//!
//! ## ★★★ The shared-model correlation
//!
//! One `M_world` per household, **forked per councillor**. Forking gives
//! independence of **sampling**, not of **assumptions** — the trajectories
//! differ, the model does not. So the residual correlation is the model's own
//! error, invisible from inside, and it presents as **unanimity**.
//!
//! ★★★ [`EffectiveN`] refuses to let that masquerade as independent
//! confirmation, and refuses equally to fake a correction: agreement reached
//! through a shared model yields [`EffectiveN::BoundedAbove`] carrying the
//! headcount and **no estimate**, because the shortfall is *an amount nobody
//! inside can measure*. Same third answer as `Skew::Unknown` and
//! `DampingVerdict::Indeterminate` — do not fabricate a distinction, and do not
//! fabricate a number either.
//!
//! ★★ And the masquerade is **unspellable**: [`Agreement::via_shared_model`]
//! cannot produce `Independent`, and [`Agreement::independent`] takes no model,
//! so there is no way to declare shared-model agreement independent.

use std::collections::BTreeSet;

use crate::attention::Attention;
use crate::ensemble::ModelTemplate;
use crate::operative::{Operative, Shared};
use crate::region::Region;
use crate::schema::Schema;
use crate::strategy::StrategyGraph;

/// How much of the world a model claims to capture.
///
/// ★ **Declared, never measured** — there is no procedure here that computes a
/// model's accuracy, because computing it would require the world the model
/// exists because we do not have. It is a claim an author makes, and the gate
/// does not consult it.
#[derive(Debug, Clone, PartialEq)]
pub struct Fidelity {
    claimed: f64,
    basis: String,
}

impl Fidelity {
    /// State a claim, and say what it rests on. Both are required: a number
    /// with no basis is the kind of confidence §VII warns about.
    pub fn claimed(value: f64, basis: &str) -> Option<Fidelity> {
        if !(0.0..=1.0).contains(&value) || basis.trim().is_empty() {
            return None;
        }
        Some(Fidelity { claimed: value, basis: basis.to_string() })
    }

    pub fn value(&self) -> f64 {
        self.claimed
    }

    pub fn basis(&self) -> &str {
        &self.basis
    }
}

/// `M_world ≈ Σ̂ = ⟨B̂, Ŝ, V̂, T̂⟩` — an approximate **copy** of the world.
///
/// Not the world, and not a handle on one: `Ŝ`/`V̂`/`T̂` are authored, and
/// [`agrees_with`](WorldModel::agrees_with) reports how far they match a real
/// [`Shared`] rather than requiring that they do.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldModel {
    id: String,
    /// `B̂` — the scope the model claims to cover.
    scope: BTreeSet<String>,
    /// `Ŝ` — the dimensions it models.
    dimensions: BTreeSet<String>,
    /// `V̂` — its idea of the viable region.
    viable: Region,
    /// `T̂` — the structure. Declared; only its parameters move.
    transitions: ModelTemplate,
    fidelity: Fidelity,
}

impl WorldModel {
    pub fn declared(
        id: &str,
        scope: &[&str],
        dimensions: &[&str],
        viable: Region,
        transitions: ModelTemplate,
        fidelity: Fidelity,
    ) -> WorldModel {
        WorldModel {
            id: id.to_string(),
            scope: scope.iter().map(|s| (*s).to_string()).collect(),
            dimensions: dimensions.iter().map(|s| (*s).to_string()).collect(),
            viable,
            transitions,
            fidelity,
        }
    }

    /// ★★ **Always `true`, and it is the row's honesty rather than a stub.**
    ///
    /// The structure is authored and is **not up for revision by data** — a new
    /// scenario rebinds probabilities and leaves the edge set alone. So the
    /// model is wrong in ways it cannot notice, and the constant says so where
    /// a reader will meet it.
    pub fn structure_is_declared(&self) -> bool {
        true
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn fidelity(&self) -> &Fidelity {
        &self.fidelity
    }

    pub fn viable(&self) -> &Region {
        &self.viable
    }

    pub fn transitions(&self) -> &ModelTemplate {
        &self.transitions
    }

    /// ★ Limit 2: dimensions the schema declares and this model does not.
    ///
    /// **Unregulated**, not merely unmodelled — and the guard against
    /// optimising into them is [`crate::goodhart`]'s, which this reports to
    /// rather than duplicates.
    pub fn blind_to(&self, schema: &Schema) -> Vec<String> {
        schema
            .top_level()
            .filter(|d| !self.dimensions.contains(*d))
            .map(str::to_string)
            .collect()
    }

    /// How far `T̂` matches a real world's `T`.
    ///
    /// ★ A **report, not a guard**: a model naming a move the world lacks is
    /// exactly what *the model is wrong* looks like, and refusing to build it
    /// would hide the disagreement this row exists to surface.
    pub fn agrees_with(&self, world: &Shared) -> ModelAgreement {
        let real: BTreeSet<&str> = world.moves().into_iter().collect();
        let modelled = self.transitions.actions();
        ModelAgreement {
            imagined: modelled
                .iter()
                .filter(|m| !real.contains(m.as_str()))
                .cloned()
                .collect(),
            unmodelled: real
                .iter()
                .filter(|m| !modelled.contains(**m))
                .map(|m| (*m).to_string())
                .collect(),
        }
    }
}

/// Where a model and the world differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAgreement {
    /// Moves the model has and the world does not — **the model is wrong**.
    pub imagined: BTreeSet<String>,
    /// Moves the world has and the model does not — **the model is blind**.
    pub unmodelled: BTreeSet<String>,
}

impl ModelAgreement {
    pub fn exact(&self) -> bool {
        self.imagined.is_empty() && self.unmodelled.is_empty()
    }
}

/// A value with the horizon it was computed over.
///
/// ★ Limit 3: model error compounds with depth, and a search will exploit it.
/// There is deliberately **no accessor returning the value alone** — a number
/// without its horizon invites exactly that exploitation and calls it a plan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    value: f64,
    horizon: usize,
}

impl Projection {
    pub fn over(value: f64, horizon: usize) -> Projection {
        Projection { value, horizon }
    }

    /// Both, together, always.
    pub fn reported(&self) -> (f64, usize) {
        (self.value, self.horizon)
    }

    pub fn horizon(&self) -> usize {
        self.horizon
    }
}

/// `M_self = Σ_i` — the operative **as a Sustain**, one level down.
///
/// ★★ Built **from** `ω`'s own parts, so it cannot disagree with them: the
/// objectives are `u`'s count, the moves are `Π`'s, and the budget is the
/// declared attention's cost. There is no second notion of any of them.
#[derive(Debug, Clone, PartialEq)]
pub struct SelfModel {
    operative_id: String,
    /// `B̂_i` — what it can see. The attention footprint is what supplies this.
    boundary: BTreeSet<String>,
    /// `T̂_i` — its moves, which are `Π`'s, which are `T`'s.
    moves: BTreeSet<String>,
    /// `|u_i|`.
    objectives: usize,
    /// `V̂_i` — the operative's own viability: what it must not spend past.
    attention_budget: usize,
}

impl SelfModel {
    /// Compose the operative's own parts into the operative-as-a-Sustain.
    pub fn of(
        operative: &Operative,
        strategy: &StrategyGraph,
        attention: &Attention,
        boundary: &[&str],
    ) -> SelfModel {
        SelfModel {
            operative_id: operative.id().to_string(),
            boundary: boundary.iter().map(|s| (*s).to_string()).collect(),
            moves: strategy.moves().into_iter().map(str::to_string).collect(),
            objectives: operative.utility().m(),
            attention_budget: attention.cost(),
        }
    }

    pub fn operative_id(&self) -> &str {
        &self.operative_id
    }

    pub fn boundary(&self) -> &BTreeSet<String> {
        &self.boundary
    }

    pub fn moves(&self) -> &BTreeSet<String> {
        &self.moves
    }

    pub fn objectives(&self) -> usize {
        self.objectives
    }

    pub fn attention_budget(&self) -> usize {
        self.attention_budget
    }

    /// ★★★ CELL's recursion, honestly: the operative-as-a-Sustain is itself
    /// bounded by the shared world, because its moves came from `Π` and `Π`'s
    /// came from `Shared`.
    pub fn moves_within(&self, world: &Shared) -> bool {
        let t: BTreeSet<&str> = world.moves().into_iter().collect();
        self.moves.iter().all(|m| t.contains(m.as_str()))
    }
}

/// ★★★ How many independent looks an agreement actually represents.
///
/// Not a number when the model is shared — see [`Agreement`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectiveN {
    /// Genuinely independent observers: the headcount *is* the effective N.
    Independent(usize),
    /// ★★★ Councillors who forked one model. The effective N is **smaller than
    /// the headcount by an amount nobody inside can measure**, so no estimate
    /// is offered — only the bound and the model they shared.
    BoundedAbove { headcount: usize, shared_model: String },
}

impl EffectiveN {
    /// The honest reading a surface should print.
    pub fn describe(&self) -> String {
        match self {
            EffectiveN::Independent(n) => format!("{n} independent observations"),
            EffectiveN::BoundedAbove { headcount, shared_model } => format!(
                "{headcount} councillors agreed, but all forked model '{shared_model}': \
                 fewer than {headcount} independent observations, by an amount that \
                 cannot be measured from inside"
            ),
        }
    }

    /// Is this agreement safe to read as `n`-independent confirmation?
    pub fn is_independent(&self) -> bool {
        matches!(self, EffectiveN::Independent(_))
    }
}

/// Concurrence among councillors, and where it came from.
///
/// ★★ The masquerade is **unspellable**: [`via_shared_model`] cannot yield
/// `Independent`, and [`independent`] takes no model — so there is no way to
/// declare shared-model agreement independent.
///
/// [`via_shared_model`]: Agreement::via_shared_model
/// [`independent`]: Agreement::independent
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agreement {
    headcount: usize,
    shared_model: Option<String>,
}

impl Agreement {
    /// Observers with genuinely separate models.
    pub fn independent(headcount: usize) -> Agreement {
        Agreement { headcount, shared_model: None }
    }

    /// ★★★ Councillors who forked one `M_world`. Forking gives independence of
    /// **sampling**, not of **assumptions**.
    pub fn via_shared_model(headcount: usize, model: &WorldModel) -> Agreement {
        Agreement { headcount, shared_model: Some(model.id().to_string()) }
    }

    pub fn headcount(&self) -> usize {
        self.headcount
    }

    pub fn effective_n(&self) -> EffectiveN {
        match &self.shared_model {
            None => EffectiveN::Independent(self.headcount),
            Some(m) => EffectiveN::BoundedAbove {
                headcount: self.headcount,
                shared_model: m.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attention::Aperture;
    use crate::ensemble::Prob;
    use crate::operative::{Cynefin, Objective, Sense, Utility};
    use crate::region::Interval;
    use crate::schema::{DimType, Schema};
    use serde_json::Map;

    fn world() -> Shared {
        Shared::new().with_move("budget.allocate").with_move("budget.record_income")
    }

    fn template() -> ModelTemplate {
        ModelTemplate::new().certain("s0", "budget.allocate", "s1")
    }

    fn fidelity() -> Fidelity {
        Fidelity::claimed(0.7, "declared from the household's own spec, unmeasured").unwrap()
    }

    fn model() -> WorldModel {
        WorldModel::declared(
            "household",
            &["household"],
            &["balance"],
            Region::new().bounding(Interval::new("balance", 0.0, 100.0)),
            template(),
            fidelity(),
        )
    }

    fn operative() -> Operative {
        let u = Utility::new()
            .with(Objective::new("thrift", "balance", Sense::Maximise))
            .unwrap();
        Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
    }

    fn strategy() -> StrategyGraph {
        StrategyGraph::new("a", "a")
            .with_node(&world(), "a", "budget.allocate", Map::new())
            .unwrap()
    }

    fn attention() -> Attention {
        Attention::declared(
            Aperture::declared(1, 3, 4).unwrap(),
            Aperture::declared(12, 1, 1).unwrap(),
            100,
        )
        .unwrap()
    }

    // ── M_self is consistent with ω ──────────────────────────────────────────

    #[test]
    fn the_self_model_is_built_from_omegas_own_parts() {
        // ★★ No second notion of utility, strategy or attention: each field is
        // read off the thing that already holds it.
        let m = SelfModel::of(&operative(), &strategy(), &attention(), &["household"]);
        assert_eq!(m.operative_id(), "mentor");
        assert_eq!(m.objectives(), operative().utility().m());
        assert_eq!(
            m.moves(),
            &strategy().moves().into_iter().map(str::to_string).collect::<BTreeSet<_>>()
        );
        assert_eq!(m.attention_budget(), attention().cost());
    }

    #[test]
    fn the_self_models_moves_are_bounded_by_t() {
        // ★★★ CELL's recursion, honestly: Π's moves came from Shared, so the
        // operative-as-a-Sustain is itself bounded by the shared world.
        let m = SelfModel::of(&operative(), &strategy(), &attention(), &["household"]);
        assert!(m.moves_within(&world()));
    }

    // ── M_world frames the Tenet rather than replacing it ────────────────────

    #[test]
    fn the_structure_is_declared_and_only_parameters_move() {
        // ★★ A new scenario rebinds probabilities; the edge set is untouched.
        let m = model();
        assert!(m.structure_is_declared());
        let before = m.transitions().actions();
        let scenario = crate::ensemble::Scenario::new("optimistic");
        let _ = m.transitions().instantiate(&scenario);
        assert_eq!(m.transitions().actions(), before, "structure is not up for revision");
    }

    #[test]
    fn a_model_may_disagree_with_the_world_and_the_disagreement_is_reported() {
        // ★ A report, not a guard: `imagined` is the model being wrong,
        // `unmodelled` is the model being blind. Refusing to build it would
        // hide exactly what this row exists to surface.
        let m = WorldModel::declared(
            "fanciful",
            &["household"],
            &["balance"],
            Region::new(),
            ModelTemplate::new().certain("s0", "budget.teleport", "s1"),
            fidelity(),
        );
        let a = m.agrees_with(&world());
        assert!(a.imagined.contains("budget.teleport"), "the model is wrong");
        assert!(a.unmodelled.contains("budget.allocate"), "and it is blind");
        assert!(!a.exact());
    }

    #[test]
    fn dimensions_outside_the_model_are_named_as_unregulated() {
        // ★ Limit 2, reported here and guarded by `goodhart`, not duplicated.
        let schema = Schema::new()
            .declare("balance", DimType::Any)
            .declare("wellbeing", DimType::Any);
        assert_eq!(model().blind_to(&schema), vec!["wellbeing".to_string()]);
    }

    #[test]
    fn a_fidelity_claim_needs_a_basis_and_the_gate_never_reads_it() {
        // ★ Limit 1. A number with no basis is refused; and there is no path
        // from a fidelity claim to an admission — `execute` takes no model.
        assert!(Fidelity::claimed(0.9, "   ").is_none());
        assert!(Fidelity::claimed(1.5, "anything").is_none());
        assert_eq!(model().fidelity().value(), 0.7);
    }

    #[test]
    fn a_projection_cannot_be_read_without_its_horizon() {
        // ★ Limit 3: there is no accessor returning the value alone.
        let p = Projection::over(12.5, 8);
        assert_eq!(p.reported(), (12.5, 8));
        assert_eq!(p.horizon(), 8);
    }

    // ── the shared-model correlation ─────────────────────────────────────────

    #[test]
    fn independent_observers_have_an_effective_n_equal_to_their_headcount() {
        assert_eq!(Agreement::independent(5).effective_n(), EffectiveN::Independent(5));
        assert!(Agreement::independent(5).effective_n().is_independent());
    }

    #[test]
    fn councillors_who_forked_one_model_are_not_n_independent_confirmations() {
        // ★★★ THE KEYSTONE. Five agreeing councillors, one model.
        let a = Agreement::via_shared_model(5, &model());
        let n = a.effective_n();
        assert_eq!(
            n,
            EffectiveN::BoundedAbove { headcount: 5, shared_model: "household".into() }
        );
        assert!(!n.is_independent(), "unanimity is not independence");
        assert_eq!(a.headcount(), 5, "the headcount is still reported");
    }

    #[test]
    fn no_effective_n_number_is_fabricated_for_a_shared_model() {
        // ★★★ The shortfall is "an amount nobody inside can measure", so no
        // estimate is offered — only the bound. Same third answer as
        // `Skew::Unknown` and `DampingVerdict::Indeterminate`.
        let n = Agreement::via_shared_model(9, &model()).effective_n();
        match n {
            EffectiveN::BoundedAbove { headcount, .. } => assert_eq!(headcount, 9),
            other => panic!("a number was fabricated: {other:?}"),
        }
        assert!(n.describe().contains("cannot be measured from inside"));
    }

    #[test]
    fn shared_model_agreement_cannot_be_declared_independent() {
        // ★★ Unspellable: `via_shared_model` cannot yield `Independent`, and
        // `independent` takes no model. The observable consequence is that the
        // same headcount reads differently depending only on provenance.
        let shared = Agreement::via_shared_model(4, &model()).effective_n();
        let alone = Agreement::independent(4).effective_n();
        assert_ne!(shared, alone);
        assert!(alone.is_independent() && !shared.is_independent());
    }

    #[test]
    fn forking_changes_the_sampling_not_the_assumptions() {
        // ★ Two councillors forking the SAME model agree via that model however
        // many of them there are — the correlation does not dilute with
        // headcount, which is the whole point.
        for n in [2usize, 20, 200] {
            let e = Agreement::via_shared_model(n, &model()).effective_n();
            assert!(!e.is_independent(), "{n} councillors, still one model");
        }
    }

    #[test]
    fn a_prob_bound_scenario_is_the_residual_being_learned_not_the_structure() {
        // ★★ The declared/learned split, as the ensemble already built it: the
        // scenario supplies numbers, the template supplies shape.
        let t = ModelTemplate::new()
            .edge("s0", "budget.allocate", vec![("s1", Prob::param("p"))]);
        assert!(t.parameters().contains("p"), "the residual is a named parameter");
        assert!(t.actions().contains("budget.allocate"), "the structure is not");
    }
}
