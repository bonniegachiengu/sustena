//! `M(t+1) = retain(select_{u,Viab}(vary(M(t))))` — learning as search over
//! meme space (OPV-6; the Operative paper, §IV).
//!
//! An operative learns by **varying its strategies**, **scoring the variants in
//! the sandbox**, and **retaining the survivors**. Holland's loop, with
//! Sustena's own scoring: `u_i` (OPV-3) and viability (`V`).
//!
//! ## ★★★ This is the one place structure moves — and it is not `M_world`
//!
//! OPV-11's [`WorldModel::structure_is_declared`](crate::models::WorldModel::structure_is_declared)
//! is **always `true`**, and that stays true. The two are consistent rather
//! than in tension, and the distinction is the whole scope of this row:
//!
//! - **`M_world`'s structure does not learn.** Which edges exist in `T̂` is
//!   declared; only the residual — a [`Scenario`](crate::ensemble::Scenario)'s
//!   probabilities — is bound from data. A world model that rewrote its own
//!   causal structure from observations would be a different and much larger
//!   claim.
//! - **The strategy library does.** A meme is a `Π` (OPV-4), and `Π` is *how an
//!   operative reasons* — reconfiguring, extending and tuning it is exactly
//!   what §IV asks for.
//!
//! So learning changes the operative, not the world it believes in.
//!
//! ## ★★ Proposition 3 survives variation
//!
//! A varied strategy is still a [`StrategyGraph`], and every node still holds a
//! [`LegalMove`](crate::operative::LegalMove) minted from [`Shared`]. There is
//! no path here that builds a node from a bare string, so **`vary` cannot
//! manufacture a move outside `T`** — [`VaryOp::Extend`] naming an illegal move
//! comes back in [`Varied::refused`] with `MoveNotInT`, reported rather than
//! dropped. The bound `Π ⊆ T` is inherited, not re-checked.
//!
//! ## ★★ `vary` is declared, not random
//!
//! ADR-0001 keeps RNG out of this core, so the variation operators are
//! **supplied by the caller** rather than sampled. Holland's framing is
//! stochastic; this is the same three moves — reconfigure, extend, optimise —
//! made deterministic. A host that wants stochastic search draws the ops
//! itself and hands them in, which also means a learning round is replayable.
//!
//! ## ★★ `select` returns a FRONTIER, not a winner
//!
//! `u_i` is a vector and [`Utility`] has no method that returns a scalar —
//! deliberately, since OPV-3. So selection scores each variant and returns the
//! **non-dominated set** ([`pareto_frontier`]). Collapsing the vector to pick a
//! single survivor would delete the objectives the operative was given, which
//! OPV-29 names as *a presentation bug with the consequences of an architecture
//! bug*; here it would be an architecture bug outright, because the survivor is
//! what the library keeps.
//!
//! A variant the gate refuses **scores accordingly** ([`Fitness::Refused`]) —
//! it is a bad score, not an error. And a variant whose resulting state is
//! missing a dimension `u_i` reads is [`Fitness::Unscorable`], the third honest
//! answer: **no score, not zero**, matching `Utility::at`'s own refusal.
//!
//! ## ★ Steps happen on RESULTS, not on a clock
//!
//! [`learn`] requires a [`Feedback`] — a correction, a divergence between what
//! `M_world` projected and what was observed, or another operative's objection.
//! **There is no `Feedback::Scheduled` variant**, so a timer-driven round is
//! not something this module can be asked for. ADR-0001 makes that doubly true:
//! the core has no clock to trigger one with.
//!
//! [`Feedback::from_projection`] returns `None` when a projection and the
//! observation agree — *a learning step with no triggering result is a no-op,
//! and it is the constructor that says so.*
//!
//! ## ★★ "Operatives are attractors" is a HYPOTHESIS
//!
//! §4G.1 reads operatives as attractors of this iteration under selection by
//! `u_i`. **Nothing here claims that happens.** There is no theorem that the
//! loop converges, that its fixed points are unique, or that a fixed point is
//! any good. What is built is the *instrument*: [`LearningTrace`] records the
//! rounds, [`LearningTrace::stabilisation`] reports whether the library has
//! stopped changing, and [`Stabilisation::proves_convergence`] is **`false` for
//! every value** — present so the claim can be asserted *absent* rather than
//! merely left unwritten, exactly as `structure_is_declared` is always `true`.
//!
//! The evolutionary literature supplies the mechanism. Sustena supplies the
//! scoring. Neither supplies a guarantee.
//!
//! ## ★★★ No Free Lunch — a library is worth this household's non-uniformity
//!
//! Averaged over all objective functions every search algorithm performs
//! identically (Wolpert & Macready 1997). A strategy library is therefore worth
//! exactly the non-uniformity of *this* Sustain's problem distribution, and a
//! stranger's library is worth their non-uniformity, not yours.
//!
//! So a [`MemeLibrary`] is **scoped** to one `(sustain, operative)` pair, and
//! [`MemeProvenance`] is **minted only by the library** — there is no public
//! constructor for a [`Meme`], and [`MemeLibrary::import`] always stamps
//! [`MemeProvenance::Imported`] with the origin scope it came from. **A foreign
//! meme cannot be written down as if it were native**, in the same way a
//! [`Capability`](crate::capability::Capability) cannot be minted or a
//! [`Score`](crate::attention::Score) declared. Importing is allowed; importing
//! *silently* is not.
//!
//! ## Reconcile with `M_self`
//!
//! *What changed about me* is [`Meme::self_model`], which delegates straight to
//! [`SelfModel::of`] — so a retained variant's self-model reflects the new
//! moves with no second construction path, and OPV-11's *composed, not forked*
//! property is untouched. [`SelfModel::moves_within`] still holds afterwards,
//! which is Proposition 3 arriving at the self-model: **learning cannot produce
//! an `M_self` that escapes `T`.**

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::approval::EffectClass;
use crate::attention::Attention;
use crate::models::{Projection, SelfModel};
use crate::operative::{pareto_frontier, Operative, Shared};
use crate::operator::{Enforcement, Registry};
use crate::region::{Membership, Region};
use crate::state::State;
use crate::strategy::{run_with_effect, StrategyError, StrategyGraph, WalkOutcome};

// ── the trigger ──────────────────────────────────────────────────────────────

/// Why a learning round is running.
///
/// ★ **There is no variant for a timer.** A round happens because something
/// *came back* — the three §IV names, and nothing else. A host with a
/// scheduler still has to produce one of these, which means it has to say what
/// result it is reacting to.
#[derive(Debug, Clone, PartialEq)]
pub enum Feedback {
    /// A human corrected the operative.
    Correction { by: String, note: String },
    /// ★ `M_world` projected one thing and the world did another.
    ///
    /// Carries the horizon, because OPV-11's [`Projection`] cannot be quoted
    /// without one and a divergence quoted without its horizon is not a fact.
    PredictionDiverged { model: String, predicted: f64, observed: f64, horizon: usize },
    /// Another operative objected.
    Objection { by: String, note: String },
}

impl Feedback {
    /// A divergence, **or `None` when there is nothing to react to**.
    ///
    /// ★ This is the *no triggering result ⇒ no step* rule, put where it cannot
    /// be forgotten: an agreeing projection does not produce a `Feedback`, so
    /// it cannot produce a round.
    pub fn from_projection(model: &str, projection: &Projection, observed: f64) -> Option<Feedback> {
        let (predicted, horizon) = projection.reported();
        if predicted == observed {
            return None;
        }
        Some(Feedback::PredictionDiverged {
            model: model.to_string(),
            predicted,
            observed,
            horizon,
        })
    }

    /// A short, human-legible reason, recorded on every meme the round retains.
    pub fn describe(&self) -> String {
        match self {
            Feedback::Correction { by, note } => format!("corrected by {by}: {note}"),
            Feedback::PredictionDiverged { model, predicted, observed, horizon } => format!(
                "'{model}' projected {predicted} over {horizon} steps; {observed} was observed"
            ),
            Feedback::Objection { by, note } => format!("{by} objected: {note}"),
        }
    }
}

// ── vary ─────────────────────────────────────────────────────────────────────

/// Holland's three, made declarative.
#[derive(Debug, Clone, PartialEq)]
pub enum VaryOp {
    /// Rewire: send an existing edge somewhere else.
    Reconfigure { edge: usize, to: String },
    /// ★★★ Extend: add a node after another. The move is minted from
    /// [`Shared`], so this is the only place variation could have escaped `T`
    /// and it does not.
    ///
    /// ★ Extending **at the exit moves the exit** — *and then also do this* is
    /// what extending a strategy at its end means, and a new node left
    /// unreachable behind the old exit would grow `moves()` without changing
    /// any behaviour, which is exactly the kind of inert addition this build
    /// refuses elsewhere.
    Extend { node: String, mv: String, after: String, kwargs: Map<String, Value> },
    /// Optimise: retune one node's parameter.
    Optimise { node: String, param: String, value: Value },
}

impl VaryOp {
    /// A human-legible description, recorded on the retained meme's
    /// [`MemeProvenance::Varied`].
    ///
    /// ★ It carries the **value**, because *retuned `amount`* and *retuned
    /// `amount` to 250* are different facts and only the second says what was
    /// learned. It is a description and **not an identity** — see [`vary`],
    /// which mints ids by position.
    fn label(&self) -> String {
        match self {
            VaryOp::Reconfigure { edge, to } => format!("reconfigure:{edge}->{to}"),
            VaryOp::Extend { node, mv, .. } => format!("extend:{node}={mv}"),
            VaryOp::Optimise { node, param, value } => {
                format!("optimise:{node}.{param}={value}")
            }
        }
    }
}

/// One product of variation — still a `Π`, still bounded by `T`.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    id: String,
    parent: String,
    op: VaryOp,
    strategy: StrategyGraph,
}

impl Variant {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The meme it was varied from.
    pub fn parent(&self) -> &str {
        &self.parent
    }

    pub fn op(&self) -> &VaryOp {
        &self.op
    }

    pub fn strategy(&self) -> &StrategyGraph {
        &self.strategy
    }

    /// ★★ Proposition 3, asserted on the variant rather than argued about it.
    pub fn moves(&self) -> BTreeSet<&str> {
        self.strategy.moves()
    }
}

/// What `vary` produced — and what it refused.
#[derive(Debug, Clone, PartialEq)]
pub struct Varied {
    pub variants: Vec<Variant>,
    /// ★ Reported, not dropped. A `VaryOp` naming a move outside `T` lands
    /// here with `MoveNotInT`, which is how *variation cannot widen `T`* is
    /// observable rather than merely true.
    pub refused: Vec<(VaryOp, StrategyError)>,
}

/// `vary(M(t))` — apply each declared operator to `meme`, independently.
///
/// Each variant is the parent with **one** change, so a variant's fitness is
/// attributable to the change that made it.
///
/// ★ A variant's id is `parent/index:label` and the **index is what makes it
/// unique**. Caught by a test rather than by inspection: with the label alone
/// as the id, two `Optimise` ops on the same node and parameter minted the
/// *same* id, and two genuinely different variants were silently conflated —
/// one scored, the other reported as *already in the library*. An identity and
/// a description are different jobs, and this is what happens when one value
/// does both.
pub fn vary(meme: &Meme, world: &Shared, ops: &[VaryOp]) -> Varied {
    let mut variants = Vec::new();
    let mut refused = Vec::new();

    for (i, op) in ops.iter().enumerate() {
        match apply_vary(meme.strategy(), world, op) {
            Ok(strategy) => variants.push(Variant {
                id: format!("{}/{i}:{}", meme.id(), op.label()),
                parent: meme.id().to_string(),
                op: op.clone(),
                strategy,
            }),
            Err(e) => refused.push((op.clone(), e)),
        }
    }

    Varied { variants, refused }
}

fn apply_vary(
    parent: &StrategyGraph,
    world: &Shared,
    op: &VaryOp,
) -> Result<StrategyGraph, StrategyError> {
    // ★ Extending at the exit moves the exit; see `VaryOp::Extend`.
    let new_exit = match op {
        VaryOp::Extend { node, after, .. } if after == parent.exit() => node.clone(),
        _ => parent.exit().to_string(),
    };

    // Rebuild through the public constructors only — there is no back door
    // that skips `with_node`'s minting.
    let mut rebuilt = StrategyGraph::new(parent.entry(), &new_exit);
    for node in parent.nodes() {
        let mut kwargs = node.kwargs().clone();
        if let VaryOp::Optimise { node: target, param, value } = op {
            if node.id() == target {
                kwargs.insert(param.clone(), value.clone());
            }
        }
        rebuilt = rebuilt.with_node(world, node.id(), node.move_name(), kwargs)?;
    }

    if let VaryOp::Extend { node, mv, kwargs, .. } = op {
        // ★★★ The one place a NEW move enters a strategy, and it goes through
        // `with_node`, which mints from `world`. An illegal move is a
        // `MoveNotInT` here, not a node.
        rebuilt = rebuilt.with_node(world, node, mv, kwargs.clone())?;
    }

    let edges: Vec<_> = parent.edges().to_vec();
    for (i, e) in edges.iter().enumerate() {
        let to = match op {
            VaryOp::Reconfigure { edge, to } if *edge == i => to.clone(),
            _ => e.to.clone(),
        };
        rebuilt = rebuilt.with_edge(&e.from, &to, e.condition.clone());
    }
    if let VaryOp::Extend { node, after, .. } = op {
        rebuilt = rebuilt.with_edge(after, node, None);
    }

    rebuilt.typecheck()?;
    Ok(rebuilt)
}

// ── select ───────────────────────────────────────────────────────────────────

/// How a variant scored. ★ Four outcomes, and none of them is a fabricated
/// number.
#[derive(Debug, Clone, PartialEq)]
pub enum Fitness {
    /// Ran to `exit` and landed in `V`. `utility` is `u_i(s')` — **a vector,
    /// and it stays one**.
    Viable { utility: Vec<f64>, membership: Membership },
    /// Ran to `exit`, but the resulting state is outside `V`. Carries the
    /// weighted `d(s,V)` so *how far outside* is readable.
    Outside { utility: Vec<f64>, distance: f64 },
    /// ★ The gate refused a node, or the walk did not reach `exit`. A variant
    /// that violates the gate **scores accordingly** — this is a bad score, not
    /// an error the caller has to handle.
    Refused { node: String, reason: String },
    /// ★ The third honest answer: the run finished, but a dimension `u_i` reads
    /// is absent from the resulting state. **No score. Not zero.**
    Unscorable { reason: String },
}

impl Fitness {
    /// Only a `Viable` variant is eligible for the frontier. Being outside `V`
    /// is not a tie-break against a viable one; it is disqualifying.
    pub fn utility_if_viable(&self) -> Option<&[f64]> {
        match self {
            Fitness::Viable { utility, .. } => Some(utility),
            _ => None,
        }
    }
}

/// `select_{u,Viab}(...)` — every variant's score, and the non-dominated set.
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    /// Every variant, scored. Nothing is discarded before it is recorded.
    pub scored: Vec<(String, Fitness)>,
    /// ★★ The survivors: `𝒫(X)` over the viable variants' `u_i` vectors.
    /// **A set, never a winner.**
    pub frontier: Vec<String>,
}

/// Score every variant by running it in the sandbox.
///
/// ★★ The venue is [`run_with_effect`] under [`EffectClass::Sandbox`] — the
/// full §4E gate, Operative §X's *fork and replay under the same rules*. There
/// is no scoring heuristic here; a variant's fitness is what actually happened
/// when it ran.
///
/// ★ Isolation is by construction rather than by mechanism: this core is pure,
/// so the walk is handed a **clone** of the state and its result is read and
/// dropped. Nothing was protected from a write, because there was no write to
/// protect from.
#[allow(clippy::too_many_arguments)]
pub fn select(
    variants: &[Variant],
    operative: &Operative,
    viable: &Region,
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    trigger: Value,
) -> Selection {
    let mut scored = Vec::new();
    let mut points: Vec<(String, Vec<f64>)> = Vec::new();

    for v in variants {
        let walk = run_with_effect(
            v.strategy(),
            registry,
            allowed,
            enforcement,
            &state.clone(),
            trigger.clone(),
            &EffectClass::Sandbox,
        );

        let fitness = match &walk.outcome {
            WalkOutcome::Refused { node, reason } => {
                Fitness::Refused { node: node.clone(), reason: reason.clone() }
            }
            WalkOutcome::NoMatchingEdge { at } => Fitness::Refused {
                node: at.clone(),
                reason: "the walk stopped before reaching exit".to_string(),
            },
            WalkOutcome::StepsExhausted { limit } => Fitness::Refused {
                node: v.strategy().entry().to_string(),
                reason: format!("the walk ran out after {limit} steps"),
            },
            WalkOutcome::ReachedExit => score_state(operative, viable, &walk.state),
        };

        if let Some(u) = fitness.utility_if_viable() {
            points.push((v.id().to_string(), u.to_vec()));
        }
        scored.push((v.id().to_string(), fitness));
    }

    // `pareto_frontier` refuses ragged vectors; every point here came from the
    // same `Utility`, so the lengths agree by construction.
    let frontier = pareto_frontier(&points).unwrap_or_default();
    Selection { scored, frontier }
}

fn score_state(operative: &Operative, viable: &Region, state: &Value) -> Fitness {
    let reader = State::new(state.clone());
    let mut point = BTreeMap::new();
    for dim in operative.utility().support() {
        match reader.get(dim).and_then(|v| v.as_f64()) {
            Some(v) => {
                point.insert(dim.to_string(), v);
            }
            // ★ No score, not zero.
            None => {
                return Fitness::Unscorable {
                    reason: format!("'{dim}' does not resolve to a number in the resulting state"),
                }
            }
        }
    }

    let utility = match operative.utility().at(&point) {
        Ok(u) => u,
        Err(e) => return Fitness::Unscorable { reason: e.to_string() },
    };

    match viable.membership(state) {
        Ok(m) if m.is_viable() => Fitness::Viable { utility, membership: m },
        Ok(_) => {
            let d = viable.distance(state).map(|d| d.weighted).unwrap_or(f64::INFINITY);
            Fitness::Outside { utility, distance: d }
        }
        Err(e) => Fitness::Unscorable { reason: e.to_string() },
    }
}

// ── memes, provenance and the scoped library ─────────────────────────────────

/// Which `(sustain, operative)` a library belongs to.
///
/// ★★★ No `Default`. A library with no scope would be the global shared pool
/// No Free Lunch says cannot exist.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryScope {
    sustain: String,
    operative: String,
}

impl LibraryScope {
    pub fn of(sustain: &str, operative: &str) -> LibraryScope {
        LibraryScope { sustain: sustain.to_string(), operative: operative.to_string() }
    }

    pub fn sustain(&self) -> &str {
        &self.sustain
    }

    pub fn operative(&self) -> &str {
        &self.operative
    }
}

/// Where a meme came from.
///
/// ★ Named `MemeProvenance`, not `Provenance` — [`crate::event::Provenance`]
/// answers whether an event's *time* was observed or inferred, a different
/// question. Thirtieth collision; the newcomer takes the longer name, matching
/// `CycleProvenance`'s precedent.
///
/// ★★★ **Minted only by [`MemeLibrary`].** There is no public constructor for
/// a [`Meme`], so a caller cannot write down a foreign strategy carrying
/// `Authored`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemeProvenance {
    /// Written for this Sustain, in this library.
    Authored,
    /// Produced by [`vary`] from a meme already here.
    Varied { from: String, op: String },
    /// ★★★ Brought in from a different scope. **The origin is not optional**,
    /// so an import can never read as native — and NFL says an imported meme
    /// is worth the *origin's* non-uniformity, not this household's.
    Imported { origin: LibraryScope },
}

/// One strategy in the library — a `Π` with a history.
#[derive(Debug, Clone, PartialEq)]
pub struct Meme {
    id: String,
    strategy: StrategyGraph,
    provenance: MemeProvenance,
    /// The fitness that caused it to be kept, when [`retain`] kept it.
    retained_by: Option<Fitness>,
    /// The [`Feedback`] of the round that kept it. ★ The record is not
    /// discarded: *why it survived* is part of what it is.
    because: Option<String>,
}

impl Meme {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn strategy(&self) -> &StrategyGraph {
        &self.strategy
    }

    pub fn provenance(&self) -> &MemeProvenance {
        &self.provenance
    }

    pub fn retained_by(&self) -> Option<&Fitness> {
        self.retained_by.as_ref()
    }

    pub fn because(&self) -> Option<&str> {
        self.because.as_deref()
    }

    /// ★ *What changed about me* — OPV-11's `M_self`, over this meme's `Π`.
    ///
    /// Delegates to [`SelfModel::of`] and adds nothing, so the *composed, not
    /// forked* property is inherited rather than re-implemented.
    pub fn self_model(
        &self,
        operative: &Operative,
        attention: &Attention,
        boundary: &[&str],
    ) -> SelfModel {
        SelfModel::of(operative, &self.strategy, attention, boundary)
    }
}

/// Why a library refused something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    DuplicateMeme { id: String },
    /// An import naming this library's own scope. Importing from yourself is
    /// not an import; the honest answer is that nothing crossed a boundary.
    SameScope { scope: LibraryScope },
    UnknownMeme { id: String },
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibraryError::DuplicateMeme { id } => write!(f, "meme '{id}' is already in this library"),
            LibraryError::SameScope { scope } => write!(
                f,
                "cannot import from '{}/{}' into itself: nothing crossed a boundary",
                scope.sustain, scope.operative
            ),
            LibraryError::UnknownMeme { id } => write!(f, "no meme '{id}' in this library"),
        }
    }
}

/// `M` — one operative's strategies, in one Sustain.
///
/// ★★★ Scoped, because No Free Lunch says a library is worth exactly the
/// non-uniformity of the problem distribution it was built against. There is no
/// global pool and no way to make one.
#[derive(Debug, Clone, PartialEq)]
pub struct MemeLibrary {
    scope: LibraryScope,
    memes: BTreeMap<String, Meme>,
    order: Vec<String>,
}

impl MemeLibrary {
    pub fn of(scope: LibraryScope) -> MemeLibrary {
        MemeLibrary { scope, memes: BTreeMap::new(), order: Vec::new() }
    }

    pub fn scope(&self) -> &LibraryScope {
        &self.scope
    }

    /// Add a strategy written here. The only route to
    /// [`MemeProvenance::Authored`].
    pub fn author(&mut self, id: &str, strategy: StrategyGraph) -> Result<(), LibraryError> {
        self.insert(Meme {
            id: id.to_string(),
            strategy,
            provenance: MemeProvenance::Authored,
            retained_by: None,
            because: None,
        })
    }

    /// ★★★ Bring in a meme from another scope.
    ///
    /// The provenance is **re-stamped**, not copied: whatever the meme claimed
    /// where it lived, here it is [`MemeProvenance::Imported`] naming the scope
    /// it came from. This is the whole NFL discipline as a mechanism — a
    /// stranger's meme cannot enter as if it were the household's own, because
    /// `import` is the only door and it always stamps.
    pub fn import(
        &mut self,
        meme: &Meme,
        from: &LibraryScope,
        as_id: &str,
    ) -> Result<(), LibraryError> {
        if *from == self.scope {
            return Err(LibraryError::SameScope { scope: from.clone() });
        }
        self.insert(Meme {
            id: as_id.to_string(),
            strategy: meme.strategy.clone(),
            provenance: MemeProvenance::Imported { origin: from.clone() },
            // ★ A score earned in another household is not a score here.
            retained_by: None,
            because: None,
        })
    }

    fn insert(&mut self, meme: Meme) -> Result<(), LibraryError> {
        if self.memes.contains_key(&meme.id) {
            return Err(LibraryError::DuplicateMeme { id: meme.id });
        }
        self.order.push(meme.id.clone());
        self.memes.insert(meme.id.clone(), meme);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Meme> {
        self.memes.get(id)
    }

    /// Declaration order, so a report reads the way the library was built.
    pub fn iter(&self) -> impl Iterator<Item = &Meme> {
        self.order.iter().filter_map(|id| self.memes.get(id))
    }

    pub fn len(&self) -> usize {
        self.memes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.memes.is_empty()
    }

    /// The ids, for comparing one round's library against the next.
    pub fn ids(&self) -> BTreeSet<&str> {
        self.memes.keys().map(String::as_str).collect()
    }

    /// ★★ Every meme that came from somewhere else, with where.
    ///
    /// Exists so a household can be *told* what fraction of its library was
    /// built against somebody else's problem distribution.
    pub fn imported(&self) -> Vec<(&str, &LibraryScope)> {
        self.iter()
            .filter_map(|m| match &m.provenance {
                MemeProvenance::Imported { origin } => Some((m.id.as_str(), origin)),
                _ => None,
            })
            .collect()
    }
}

// ── retain ───────────────────────────────────────────────────────────────────

/// What one round kept, and what it did not.
#[derive(Debug, Clone, PartialEq)]
pub struct Retention {
    /// Ids added to the library this round.
    pub kept: Vec<String>,
    /// Variants that were scored and not kept, with why in one word.
    pub dropped: Vec<(String, String)>,
}

/// `retain(...)` — add the frontier to the library, with provenance and reason.
///
/// ★ Append-only within a round: nothing already in the library is removed, so
/// a meme that stopped winning is still there to be read. Forgetting is a
/// separate decision from learning, and this row does not make it.
pub fn retain(
    library: &mut MemeLibrary,
    variants: &[Variant],
    selection: &Selection,
    feedback: &Feedback,
) -> Retention {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    let because = feedback.describe();

    for (id, fitness) in &selection.scored {
        let Some(variant) = variants.iter().find(|v| v.id() == id) else { continue };
        if !selection.frontier.iter().any(|f| f == id) {
            dropped.push((id.clone(), drop_reason(fitness)));
            continue;
        }
        let meme = Meme {
            id: id.clone(),
            strategy: variant.strategy().clone(),
            provenance: MemeProvenance::Varied {
                from: variant.parent().to_string(),
                op: variant.op().label(),
            },
            retained_by: Some(fitness.clone()),
            because: Some(because.clone()),
        };
        if library.insert(meme).is_ok() {
            kept.push(id.clone());
        } else {
            dropped.push((id.clone(), "already in the library".to_string()));
        }
    }

    Retention { kept, dropped }
}

fn drop_reason(f: &Fitness) -> String {
    match f {
        Fitness::Viable { .. } => "dominated on every objective".to_string(),
        Fitness::Outside { distance, .. } => format!("outside V by {distance}"),
        Fitness::Refused { node, reason } => format!("gate refused '{node}': {reason}"),
        Fitness::Unscorable { reason } => format!("unscorable: {reason}"),
    }
}

// ── the loop, and the instrument ─────────────────────────────────────────────

/// One turn of `M(t+1) = retain(select(vary(M(t))))`.
///
/// ★ Named `LearningRound`, not `Round` — [`crate::consensus::Round`] is a
/// Paxos ballot, a genuinely different thing to confuse a learning step with.
/// Thirty-first collision; the newcomer takes the longer name.
#[derive(Debug, Clone, PartialEq)]
pub struct LearningRound {
    /// ★ What triggered it. There is no round without one.
    pub feedback: Feedback,
    pub varied: Varied,
    pub selection: Selection,
    pub retention: Retention,
    /// Library size before and after — the quantity `stabilisation` reads.
    pub library_before: usize,
    pub library_after: usize,
}

impl LearningRound {
    pub fn changed_the_library(&self) -> bool {
        !self.retention.kept.is_empty()
    }
}

/// `M(t+1) = retain(select_{u,Viab}(vary(M(t))))`, for one meme.
///
/// ★ Takes a [`Feedback`] by value and there is no overload without one, so
/// *a learning step needs a result to react to* is a property of the signature.
#[allow(clippy::too_many_arguments)]
pub fn learn(
    library: &mut MemeLibrary,
    meme_id: &str,
    world: &Shared,
    operative: &Operative,
    viable: &Region,
    ops: &[VaryOp],
    registry: &Registry,
    allowed: &[String],
    enforcement: &Enforcement,
    state: &Value,
    feedback: Feedback,
) -> Result<LearningRound, LibraryError> {
    let meme = library
        .get(meme_id)
        .cloned()
        .ok_or_else(|| LibraryError::UnknownMeme { id: meme_id.to_string() })?;

    let before = library.len();
    let varied = vary(&meme, world, ops);
    let selection = select(
        &varied.variants,
        operative,
        viable,
        registry,
        allowed,
        enforcement,
        state,
        Value::Object(Map::new()),
    );
    let retention = retain(library, &varied.variants, &selection, &feedback);

    Ok(LearningRound {
        feedback,
        varied,
        selection,
        retention,
        library_before: before,
        library_after: library.len(),
    })
}

/// Whether the library has stopped changing.
///
/// ★★ **An observation, never a theorem.** See [`Stabilisation::proves_convergence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stabilisation {
    /// The last `rounds` rounds added nothing.
    UnchangedFor { rounds: usize },
    /// The most recent round changed the library.
    Churning,
    /// No round has run. ★ The third answer: *nothing has been observed* is not
    /// *nothing changed*.
    Unobserved,
}

impl Stabilisation {
    /// ★★ **`false` for every value, always.**
    ///
    /// §4G.1 reads operatives as attractors of this iteration. That is a
    /// *hypothesis*: there is no theorem here that the loop converges, that a
    /// fixed point is unique, or that reaching one is good. This method exists
    /// so the absence of the claim can be **asserted** rather than merely left
    /// unwritten — the same discipline as
    /// [`structure_is_declared`](crate::models::WorldModel::structure_is_declared)
    /// always being `true`.
    pub fn proves_convergence(&self) -> bool {
        false
    }

    pub fn describe(&self) -> String {
        match self {
            Stabilisation::UnchangedFor { rounds } => format!(
                "the library has not changed for {rounds} round(s) — an observation about \
                 rounds that have run, not a proof that it will not change on the next result"
            ),
            Stabilisation::Churning => "the last round changed the library".to_string(),
            Stabilisation::Unobserved => "no round has run; there is nothing to compare".to_string(),
        }
    }
}

/// The instrument: the rounds that have run, in order.
///
/// ★★ Built so the attractor reading is **measurable** — did the library
/// settle, and did the memes that were kept actually score better — without
/// anything here asserting that it did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LearningTrace {
    rounds: Vec<LearningRound>,
}

impl LearningTrace {
    pub fn new() -> LearningTrace {
        LearningTrace::default()
    }

    pub fn record(&mut self, round: LearningRound) {
        self.rounds.push(round);
    }

    pub fn rounds(&self) -> &[LearningRound] {
        &self.rounds
    }

    /// Trailing rounds that added nothing.
    pub fn stabilisation(&self) -> Stabilisation {
        if self.rounds.is_empty() {
            return Stabilisation::Unobserved;
        }
        let trailing = self.rounds.iter().rev().take_while(|r| !r.changed_the_library()).count();
        if trailing == 0 {
            Stabilisation::Churning
        } else {
            Stabilisation::UnchangedFor { rounds: trailing }
        }
    }

    /// ★ Every retained meme's fitness, so *did the survivors actually score
    /// better* is answerable from the record rather than assumed.
    pub fn retained_fitness(&self) -> Vec<(&str, &Fitness)> {
        self.rounds
            .iter()
            .flat_map(|r| {
                r.retention.kept.iter().filter_map(move |id| {
                    r.selection
                        .scored
                        .iter()
                        .find(|(sid, _)| sid == id)
                        .map(|(sid, f)| (sid.as_str(), f))
                })
            })
            .collect()
    }

    /// How many rounds each kind of feedback triggered. ★ Reads back the
    /// *results-not-a-clock* rule from the record itself.
    pub fn triggers(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for r in &self.rounds {
            let k = match r.feedback {
                Feedback::Correction { .. } => "correction",
                Feedback::PredictionDiverged { .. } => "prediction_diverged",
                Feedback::Objection { .. } => "objection",
            };
            *out.entry(k).or_insert(0) += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attention::Aperture;
    use crate::operative::{Cynefin, Objective, Sense, Utility};
    use crate::region::Interval;
    use serde_json::json;

    fn world() -> Shared {
        Shared::new().with_move("budget.allocate").with_move("budget.record_income")
    }

    fn registry() -> Registry {
        Registry::default()
    }

    fn allowed() -> Vec<String> {
        vec!["budget.allocate".to_string(), "budget.record_income".to_string()]
    }

    fn operative() -> Operative {
        let u = Utility::new()
            .with(Objective::new(
                "liquidity",
                "finances.liquid.balance",
                Sense::Maximise,
            ))
            .unwrap();
        Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
    }

    /// An operative reading a dimension no operator here ever writes.
    fn operative_reading_wellbeing() -> Operative {
        let u = Utility::new()
            .with(Objective::new("liquidity", "finances.liquid.balance", Sense::Maximise))
            .unwrap()
            .with(Objective::new("wellbeing", "household.wellbeing", Sense::Maximise))
            .unwrap();
        Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
    }

    fn viable() -> Region {
        Region::new().bounding(Interval::at_least("finances.liquid.balance", 0.0))
    }

    /// The gate, armed with the household's real invariant.
    fn armed() -> Enforcement {
        Enforcement {
            enabled: true,
            invariants: vec![(
                "liquid_non_negative".into(),
                "finances.liquid.balance >= 0".into(),
            )],
            ..Enforcement::default()
        }
    }

    fn base(world: &Shared) -> StrategyGraph {
        let mut kwargs = Map::new();
        kwargs.insert("amount".into(), json!(100.0));
        kwargs.insert("source".into(), json!("salary"));
        StrategyGraph::new("a", "a")
            .with_node(world, "a", "budget.record_income", kwargs)
            .unwrap()
    }

    fn library() -> MemeLibrary {
        let w = world();
        let mut lib = MemeLibrary::of(LibraryScope::of("household", "mentor"));
        lib.author("m0", base(&w)).unwrap();
        lib
    }

    fn attention() -> Attention {
        Attention::declared(
            Aperture::declared(1, 3, 4).unwrap(),
            Aperture::declared(12, 1, 1).unwrap(),
            100,
        )
        .unwrap()
    }

    fn allocate_kwargs() -> Map<String, Value> {
        let mut k = Map::new();
        k.insert("pocket_name".into(), json!("food"));
        k.insert("amount".into(), json!(50.0));
        k
    }

    fn state() -> Value {
        json!({"finances": {
            "liquid": {"balance": 500.0},
            "pockets": {},
            "income": {"monthly_total": 0.0, "sources": []}
        }})
    }

    // ── vary ─────────────────────────────────────────────────────────────────

    #[test]
    fn variation_cannot_manufacture_a_move_outside_t() {
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[VaryOp::Extend {
                node: "b".into(),
                mv: "budget.teleport".into(),
                after: "a".into(),
                kwargs: Map::new(),
            }],
        );
        assert!(out.variants.is_empty());
        assert!(matches!(
            out.refused.first().map(|(_, e)| e),
            Some(StrategyError::MoveNotInT { .. })
        ));
    }

    #[test]
    fn an_extension_with_a_legal_move_is_still_bounded_by_t() {
        let w = world();
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &w,
            &[VaryOp::Extend {
                node: "b".into(),
                mv: "budget.allocate".into(),
                after: "a".into(),
                kwargs: allocate_kwargs(),
            }],
        );
        assert_eq!(out.refused.len(), 0);
        let v = &out.variants[0];
        let t: BTreeSet<&str> = w.moves().into_iter().collect();
        assert!(v.moves().is_subset(&t), "Proposition 3 holds through variation");
    }

    #[test]
    fn optimise_retunes_a_parameter_and_leaves_the_moves_alone() {
        let lib = library();
        let before = lib.get("m0").unwrap().strategy().moves();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(250.0) }],
        );
        let v = &out.variants[0];
        assert_eq!(v.moves(), before);
        let node = v.strategy().nodes().next().unwrap();
        assert_eq!(node.kwargs().get("amount"), Some(&json!(250.0)));
    }

    #[test]
    fn a_refused_variation_is_reported_not_dropped() {
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[
                VaryOp::Extend { node: "b".into(), mv: "nope".into(), after: "a".into(), kwargs: Map::new() },
                VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(1.0) },
            ],
        );
        assert_eq!(out.variants.len(), 1, "the legal one still ran");
        assert_eq!(out.refused.len(), 1, "the illegal one is on the record");
    }

    // ── select ───────────────────────────────────────────────────────────────

    #[test]
    fn a_variant_is_scored_by_running_it_in_the_sandbox() {
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(250.0) }],
        );
        let sel = select(
            &out.variants,
            &operative(),
            &viable(),
            &registry(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Value::Object(Map::new()),
        );
        assert_eq!(sel.scored.len(), 1);
        assert!(matches!(sel.scored[0].1, Fitness::Viable { .. }));
        assert_eq!(sel.frontier.len(), 1);
    }

    #[test]
    fn scoring_leaves_the_state_it_was_handed_untouched() {
        // ★ Isolation by construction: the core is pure, so there is nothing to
        // write back unless a caller chooses to.
        let s = state();
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(999.0) }],
        );
        let _ = select(
            &out.variants,
            &operative(),
            &viable(),
            &registry(),
            &allowed(),
            &Enforcement::default(),
            &s,
            Value::Object(Map::new()),
        );
        assert_eq!(s, state());
    }

    #[test]
    fn a_variant_the_gate_refuses_scores_rather_than_erroring() {
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[VaryOp::Optimise {
                node: "a".into(),
                param: "amount".into(),
                value: json!(-1000.0),
            }],
        );
        let sel = select(
            &out.variants,
            &operative(),
            &viable(),
            &registry(),
            &allowed(),
            &armed(),
            &state(),
            Value::Object(Map::new()),
        );
        assert!(matches!(sel.scored[0].1, Fitness::Refused { .. }), "{:?}", sel.scored[0].1);
        assert!(sel.frontier.is_empty(), "a refused variant never reaches the frontier");
    }

    #[test]
    fn a_missing_dimension_is_unscorable_not_zero() {
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(10.0) }],
        );
        // The walk succeeds; `household.wellbeing` is simply not there.
        let sel = select(
            &out.variants,
            &operative_reading_wellbeing(),
            &viable(),
            &registry(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Value::Object(Map::new()),
        );
        assert!(
            matches!(sel.scored[0].1, Fitness::Unscorable { .. }),
            "no fabricated zero: {:?}",
            sel.scored[0].1
        );
        assert!(sel.frontier.is_empty(), "and an unscorable variant is not a survivor");
    }

    #[test]
    fn selection_returns_a_set_and_there_is_no_method_that_returns_a_winner() {
        let sel = Selection { scored: vec![], frontier: vec!["x".into(), "y".into()] };
        // The type carries a `Vec`; nothing here collapses it.
        assert_eq!(sel.frontier.len(), 2);
    }

    #[test]
    fn a_variants_id_is_unique_per_op_not_per_label() {
        // ★ The regression, and it was a test that found it: with the label
        // alone as the id, two `Optimise` ops on the same node and parameter
        // minted the SAME id and `retain` reported the second as *already in
        // the library* — two different variants, silently conflated.
        let lib = library();
        let out = vary(
            lib.get("m0").unwrap(),
            &world(),
            &[
                VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(100.0) },
                VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(300.0) },
            ],
        );
        assert_eq!(out.variants.len(), 2);
        assert_ne!(out.variants[0].id(), out.variants[1].id());
        assert!(out.variants[1].id().contains("300"), "the label still says what was learned");
    }

    // ── retain, provenance and scope ─────────────────────────────────────────

    #[test]
    fn a_retained_variant_carries_where_it_came_from_and_why() {
        let mut lib = library();
        let fb = Feedback::Correction { by: "bonnie".into(), note: "too small".into() };
        let round = learn(
            &mut lib,
            "m0",
            &world(),
            &operative(),
            &viable(),
            &[VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(250.0) }],
            &registry(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            fb.clone(),
        )
        .unwrap();

        assert_eq!(round.retention.kept.len(), 1);
        let kept = lib.get(&round.retention.kept[0]).unwrap();
        assert!(matches!(kept.provenance(), MemeProvenance::Varied { from, .. } if from == "m0"));
        assert!(kept.retained_by().is_some(), "the score that kept it is on the record");
        assert_eq!(kept.because(), Some(fb.describe().as_str()));
    }

    #[test]
    fn an_imported_meme_cannot_read_as_native() {
        let donor = library();
        let m = donor.get("m0").unwrap().clone();
        let mut mine = MemeLibrary::of(LibraryScope::of("neighbour", "mentor"));
        mine.import(&m, donor.scope(), "borrowed").unwrap();

        let got = mine.get("borrowed").unwrap();
        match got.provenance() {
            MemeProvenance::Imported { origin } => assert_eq!(origin, donor.scope()),
            other => panic!("an import read as {other:?}"),
        }
        assert_eq!(mine.imported().len(), 1, "and it is countable");
    }

    #[test]
    fn a_library_is_scoped_and_importing_from_itself_is_refused() {
        let lib = library();
        let m = lib.get("m0").unwrap().clone();
        let mut same = MemeLibrary::of(LibraryScope::of("household", "mentor"));
        assert!(matches!(
            same.import(&m, &LibraryScope::of("household", "mentor"), "x"),
            Err(LibraryError::SameScope { .. })
        ));
    }

    #[test]
    fn an_imported_meme_does_not_inherit_the_donors_score() {
        let mut donor = library();
        learn(
            &mut donor,
            "m0",
            &world(),
            &operative(),
            &viable(),
            &[VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(250.0) }],
            &registry(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Correction { by: "b".into(), note: "n".into() },
        )
        .unwrap();
        let scored_id = donor.iter().find(|m| m.retained_by().is_some()).unwrap().id().to_string();
        let m = donor.get(&scored_id).unwrap().clone();

        let mut mine = MemeLibrary::of(LibraryScope::of("neighbour", "mentor"));
        mine.import(&m, donor.scope(), "borrowed").unwrap();
        assert!(
            mine.get("borrowed").unwrap().retained_by().is_none(),
            "a score earned against another household's problems is not a score here"
        );
    }

    // ── the trigger ──────────────────────────────────────────────────────────

    #[test]
    fn there_is_no_feedback_variant_for_a_timer() {
        // Structural: the three constructors below are exhaustive, and the
        // module has no clock to build a fourth from (ADR-0001).
        for f in [
            Feedback::Correction { by: "b".into(), note: "n".into() },
            Feedback::PredictionDiverged {
                model: "m".into(),
                predicted: 1.0,
                observed: 2.0,
                horizon: 3,
            },
            Feedback::Objection { by: "curator".into(), note: "n".into() },
        ] {
            assert!(!f.describe().is_empty());
        }
    }

    #[test]
    fn an_agreeing_projection_produces_no_feedback_and_so_no_round() {
        let p = Projection::over(10.0, 4);
        assert!(Feedback::from_projection("household", &p, 10.0).is_none());
        let diverged = Feedback::from_projection("household", &p, 12.0).unwrap();
        match diverged {
            Feedback::PredictionDiverged { horizon, .. } => assert_eq!(horizon, 4),
            other => panic!("{other:?}"),
        }
    }

    // ── the instrument ───────────────────────────────────────────────────────

    #[test]
    fn stabilisation_never_proves_convergence() {
        for s in [
            Stabilisation::UnchangedFor { rounds: 99 },
            Stabilisation::Churning,
            Stabilisation::Unobserved,
        ] {
            assert!(!s.proves_convergence(), "{s:?} must not claim a theorem");
        }
    }

    #[test]
    fn an_empty_trace_is_unobserved_not_stable() {
        assert_eq!(LearningTrace::new().stabilisation(), Stabilisation::Unobserved);
    }

    #[test]
    fn a_round_that_kept_nothing_reads_as_unchanged() {
        let mut lib = library();
        let mut trace = LearningTrace::new();
        // An illegal extension: nothing to score, nothing to keep.
        let r = learn(
            &mut lib,
            "m0",
            &world(),
            &operative(),
            &viable(),
            &[VaryOp::Extend { node: "b".into(), mv: "nope".into(), after: "a".into(), kwargs: Map::new() }],
            &registry(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Objection { by: "curator".into(), note: "n".into() },
        )
        .unwrap();
        assert!(!r.changed_the_library());
        trace.record(r);
        assert_eq!(trace.stabilisation(), Stabilisation::UnchangedFor { rounds: 1 });
    }

    #[test]
    fn the_trace_reads_back_what_triggered_each_round() {
        let mut lib = library();
        let mut trace = LearningTrace::new();
        for (i, fb) in [
            Feedback::Correction { by: "b".into(), note: "n".into() },
            Feedback::Objection { by: "c".into(), note: "n".into() },
        ]
        .into_iter()
        .enumerate()
        {
            let r = learn(
                &mut lib,
                "m0",
                &world(),
                &operative(),
                &viable(),
                &[VaryOp::Optimise {
                    node: "a".into(),
                    param: "amount".into(),
                    value: json!(200.0 + i as f64),
                }],
                &registry(),
                &allowed(),
                &Enforcement::default(),
                &state(),
                fb,
            )
            .unwrap();
            trace.record(r);
        }
        let t = trace.triggers();
        assert_eq!(t.get("correction"), Some(&1));
        assert_eq!(t.get("objection"), Some(&1));
        assert!(!trace.retained_fitness().is_empty());
    }

    // ── the M_self reconcile ─────────────────────────────────────────────────

    #[test]
    fn what_changed_about_me_shows_up_in_the_self_model() {
        let w = world();
        let mut lib = library();
        let op = operative();
        let att = attention();

        let before = lib.get("m0").unwrap().self_model(&op, &att, &["household"]);
        assert!(!before.moves().contains("budget.allocate"));

        let round = learn(
            &mut lib,
            "m0",
            &w,
            &op,
            &viable(),
            &[VaryOp::Extend {
                node: "b".into(),
                mv: "budget.allocate".into(),
                after: "a".into(),
                kwargs: allocate_kwargs(),
            }],
            &registry(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Correction { by: "bonnie".into(), note: "allocate it".into() },
        )
        .unwrap();

        let kept = lib.get(round.retention.kept.first().expect("a variant survived")).unwrap();
        let after = kept.self_model(&op, &att, &["household"]);
        assert!(after.moves().contains("budget.allocate"), "the self-model reflects the change");
        // ★★★ And Proposition 3 arrives at M_self: learning cannot escape T.
        assert!(after.moves_within(&w));
    }
}
