//! # sustena-core
//!
//! The portable Sustena engine: one core that runs on Android, iOS, desktop
//! and the web (ADR-0001, Decision 1).
//!
//! Sustena models any describable system — a household, a farm, a savings
//! group — as one recursive primitive, the **Sustain**:
//!
//! ```text
//! Σ = ⟨B, S, V, T, ⊕⟩     Boundary · State · Viable region · Transitions · composition
//! ```
//!
//! ## What this crate is for
//!
//! Deterministic engine logic only. No database, no HTTP, no clock, no random
//! source. Everything here is a pure function of its inputs, which is what
//! makes it portable to every surface and testable by conformance vectors
//! shared with the reference engine.
//!
//! Anything that needs the outside world — storage, network, time, id
//! generation — belongs to the host that embeds this crate. Where the Python
//! engine mints a UUID inside `append()`, this core takes the id as an
//! argument: a core whose promise is reproducibility cannot contain a
//! non-reproducible call.
//!
//! ## Build phase
//!
//! **R1 — parity.** Match the reference Python engine's observable behaviour
//! exactly, proven by the vectors in `conformance/vectors/`. Internals are
//! built clean; behaviour is copied.
//!
//! **R2 — specs.** Implement what the articles specify but the reference
//! engine does not yet do. In progress; see `docs/R2_BACKLOG.md`. R2 behaviour
//! is authored FROM the articles, so its vectors record the specified
//! behaviour rather than the reference engine's — the reference does not have
//! it yet. The R1 vectors must keep passing throughout.
//!
//! ## Slices
//!
//! | Slice | Module | Status |
//! |---|---|---|
//! | State | [`state`] | R1 |
//! | Event fold | [`fold`] | R1 |
//! | Rules | [`predicate`] | R1 |
//! | Operators | [`operator`] | R1 |
//! | Council | [`council`] | R1 |
//! | Approval token | [`approval`] | R2 |
//! | Edit authority | [`editing`] | R2 |
//! | Transition constraints | [`transition`] | R2 |
//! | Checked composition | [`compose`] | R2 |
//! | Version history | [`version`] | R2 |
//! | Migration (μ, EMC) | [`migrate`] | R2 |
//! | V as a region · urgency | [`region`] | R2 |
//! | EWMA + CUSUM detectors | [`detect`] | R2 |
//! | Controller decision-math | [`controller`] | R2 |
//! | Viability kernel · runway | [`kernel`] | R2 |
//! | Tenet — T:S×A→Δ(S), Bellman | [`tenet`] | R2 |
//! | Scenario ensembles · dead drops | [`ensemble`] | R2 |
//! | Signal — relayed refractory pulse | [`signal`] | R2 |
//! | Signal fns · temporal pincer | [`pincer`] | R2 |
//! | OODA loop state machine | [`ooda`] | R2 |
//! | MonitorEngine — owns the chain | [`monitor`] | R2 |
//! | Holarchic escalation | [`holarchy`] | R2 |
//! | Population · quorum sensing | [`population`] | R2 |
//! | Consensus — quorum · Paxos | [`consensus`] | R2 |
//! | Physarum router | [`router`] | R2 |
//! | Vector clocks · concurrency | [`vclock`] | R2 |
//! | Disaggregation · hysteresis | [`disaggregation`] | R2 |
//! | Division of labour · `rb > c` | [`division`] | R2 |
//! | CRDT family · join-semilattice | [`crdt`] | R2 |
//! | Operative ω · utility as a vector | [`operative`] | R2 |
//! | Goodhart guard — invariant over U | [`goodhart`] | R2 |
//! | Present the frontier, not the winner | [`presentation`] | R2 |
//! | Sparse mixture-of-experts gate | [`mixture`] | R2 |
//! | Criticality — branching ratio σ̂ | [`criticality`] | R2 |

pub mod admission;
pub mod approval;
pub mod compose;
pub mod consensus;
pub mod crdt;
pub mod controller;
pub mod council;
pub mod criticality;
pub mod detect;
pub mod disaggregation;
pub mod division;
pub mod editing;
pub mod ensemble;
pub mod error;
pub mod event;
pub mod fold;
pub mod goodhart;
pub mod holarchy;
pub mod inverse;
pub mod kernel;
pub mod migrate;
pub mod mixture;
pub mod monitor;
pub mod mutation;
pub mod operative;
pub mod operator;
pub mod path;
pub mod ooda;
pub mod pincer;
pub mod population;
pub mod predicate;
pub mod presentation;
pub mod region;
pub mod principal;
pub mod router;
pub mod schema;
pub mod signal;
pub mod state;
pub mod tenet;
pub mod transition;
pub mod vclock;
pub mod version;

pub use admission::{admit_one, typecheck_constraint, ConstraintDecl, DeclError, Strategy, Verdict};
pub use approval::{
    ApprovalToken, Binding, EffectClass, NonceLedger, Simulated, TokenError, TraceError, Voted,
};
pub use compose::{
    compose, entails, wp, Change, Composed, EffectSummary, Entailment, Pathway, PathwayError,
    Rejected, Step, WpResult,
};
pub use consensus::{
    propose, Accepted, Body, ByzantineBound, ConsensusError, Decision, Ledger, Promise,
    ProposalNumber, Round, RoundOutcome, RoundPhase,
};
pub use controller::{
    compute_urgency, is_stable_intervention, route, should_rollback, AutomationTable,
    ControlEvent, ControllerError, Preferences, Routing, SheridanLevel, Stability, SurfacedDecision,
    Urgency,
};
pub use council::{aggregate_delegated_votes, resolve, DelegatedVote, ProposalStatus, ResolutionInput, VoteChoice};
pub use criticality::{
    branching_ratio, critical_slowing_down, BranchingReading, CascadeSummary, CriticalityError,
    CriticalitySpec, EarlyWarning, Regime,
};
pub use detect::{
    classify, Alert, Cusum, CusumSpec, DetectorError, Ewma, Reading, Severity, Shift, Watch,
};
pub use disaggregation::{
    Assembly, AssemblyState, Band, BandPosition, BandTransition, DisaggregationError, Dispersal,
};
pub use crdt::{
    converges, laws_hold, merge_stamped as merge_stamped_crdt, ConvergenceReport, CrdtError,
    ElementId, GCounter, JoinSemilattice, LawReport, OrSet, PnCounter, Rga, Tag, PERMUTATION_LIMIT,
};
pub use division::{
    Assignment, AssignmentOutcome, Diagnosis, Division, DivisionError, Role, SharedInterest,
    StabilityCheck, Unfillable, SEARCH_LIMIT,
};
pub use editing::{
    admit_edit, safe, typecheck, AuthorityError, CouncilMint, Definition, Edit, EditAdmission,
    EditAuthority, EditEffect, EditError, EditToken, Governance, Instance, Irreversible, Migration,
    Stranded,
};
pub use event::{dedupe, merge, order, CausalStamp, Event, Observation, Provenance};
pub use error::{FoldError, FoldResult, StateError, StateResult};
pub use fold::{apply_mutation, diff_to_mutations, fold_events, FoldEvent};
pub use inverse::{invert, is_reversible, InverseError};
pub use kernel::{
    is_stable_toward_kernel, kernel_margin, runway, viability_kernel, viability_kernel_horizon,
    Kernel, KernelError, KernelStability, Move, Obligation, Runway, Space,
};
pub use migrate::{Applied, Compatibility, Emc, EmcVersionIds, MigrateError, Mu, StateMove};
pub use mutation::Mutation;
pub use operative::{
    dominance, geometric_mean, nash_product, pareto_frontier, scalarise, Alternative, Cynefin,
    Dominance, LegalMove, NashOutcome, Objective, Omega, Operative, OperativeError, Proposal,
    Ranking, Sense, Shared, StatePoint, Utility,
};
pub use operator::{
    execute, execute_admitted, execute_as, Authorization, Enforcement, Execution, OperatorResult,
    Registry,
};
pub use principal::{effective_privilege, permitted, Denial, MembershipEdge, Memberships, Tier};
pub use population::{
    sweep, Cascade, LocalView, PhaseTransition, Population, PopulationError, PopulationSpec,
    StepOutcome,
};
pub use predicate::{check, parse_predicate, Predicate};
pub use region::{
    Distance, FitReport, Interval, Membership, Region, RegionError,
};
pub use router::{
    Convergence, Edge, Reinforcement, Router, RouterError, RouterSpec,
};
pub use schema::{bind, preserves_shape, validate, DimType, Schema};
pub use state::State;
pub use vclock::{
    assign_clocks, merge as merge_stamped, CausalVerdict, ClockError, Dimension, MergeResolution,
    Stamped, VectorClock,
};
pub use version::{
    PathRollback, PreImage, Rollback, VersionDag, VersionError, VersionNode,
};
pub use ensemble::{
    DeadDrop, DeadDropBook, DecisionNode, Ensemble, EnsembleAnalysis, EnsembleError, InvariantSet,
    ModelTemplate, Prob, Resolution, RewardBasis, Scenario,
};
pub use goodhart::{coverage, referenced_dimensions, Coverage, GoodhartGuard, GuardError};
pub use presentation::{
    discharged_by_this_build, present, present_joint, CollapseRule, Collapsed, Discharge, Duty,
    PresentationError, Presented,
};
pub use holarchy::{
    escalate, validate_holarchy, Breach, Capacity, Escalation, EscalationOutcome, HolarchyError,
    HolarchyReport, Hop, LevelCheck, Levels,
};
pub use mixture::{
    coverage_gaps, subject_relevance, Contender, Dispatch, EstimateRule, GateWeights, Mixture,
    MixtureError, Ticket, Withheld, WithheldReason,
};
pub use monitor::{
    Driven, Ingested, MonitorEngine, MonitorError, SustainWatch,
};
pub use ooda::{
    Candidate, Ooda, OodaError, OodaObservation, OodaPhase, OodaStep, Orientation, StayReason,
    Transition,
};
pub use pincer::{
    run_pincer, Draws, ForwardStep, MonitorReport, PincerError, PincerRun, PincerSpec, Recompute,
    RecomputeReason, ScriptedDraws, Signal, SignalMonitor, SignalVerdict, Trajectory, Trigger,
};
pub use signal::{
    Field, Fired, Phase, Pulse, Refusal, RefusalReason, SignalError, SignalSpec, StepReport,
};
pub use tenet::{
    backward_induct, rewards_from_region, BellmanSpec, Distribution, InversionPoint, Outcome,
    Plan, TenetError, TransitionModel,
};
pub use transition::{
    check_all as check_transitions, Direction, Quantity, Tolerance, TransitionDeclError,
    TransitionRule, TransitionViolation,
};

/// Version of the conformance contract this build satisfies.
///
/// Bumped when the vector format changes, so a stale vector set fails loudly
/// instead of silently passing against the wrong expectations.
pub const CONFORMANCE_VERSION: u32 = 1;
