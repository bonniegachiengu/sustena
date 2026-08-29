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
//! | Capabilities (confused deputy) | [`capability`] | R2 |
//! | Fixed vs learned rules, least privilege | [`learned`] | R2 |
//! | The definition lens π : D → G | [`lens`] | R2 |
//! | The preattentive encoder φ | [`preattentive`] | R2 |
//! | Widget schema, checked on the load path | [`widget`] | R2 |
//! | β and compose(r) — the curated view | [`curated`] | R2 |
//! | Π, the strategy DAG | [`strategy`] | R1+R2 |
//! | The two attentions ⟨narrow, broad⟩ | [`attention`] | R2 |
//! | M_self and M_world | [`models`] | R2 |
//! | The persistent-panel invariant | [`panel`] | R2 |
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
//! | Suited domains · coverage · disorder | [`cynefin`] | R2 |
//! | Boundary B · autopoietic closure | [`boundary`] | R2 |
//! | Firewall F — what crosses B | [`flow`] | R2 |
//! | wp obligation — author-time | [`obligation`] | R2 |
//! | Two clocks · skew · negative skew surfaced | [`clocks`] | R2 |
//! | Watermarks · windows · declared lateness | [`watermark`] | R2 |
//! | Periods · RRULE · zone-id anchoring | [`period`] | R2 |
//! | Replay · checkpoints (s_k, k) | [`checkpoint`] | R2 |
//! | Declared dimension kind · LWW attaches | [`dimension`] | R2 |
//! | Belief state under silence (no Kalman) | [`belief`] | R2 |
//! | Harmonics · cycle vs shift (DFT) | [`harmonics`] | R2 |
//! | Window typology · tumbling/sliding/session | [`windowing`] | R2 |
//! | Observability · reachability (no matrix) | [`observability`] | R2 |
//! | Damping · settles vs rings | [`damping`] | R2 |
//! | ★ the four wired into [`monitor`] (Phase 2 close) | [`monitor`] | R2 |
//! | Semantic replay under D′ | [`semantic`] | R2 |
//! | Stranding: states vs histories, three remedies | [`stranding`] | R2 |

pub mod admission;
pub mod agent;
pub mod attention;
pub mod approval;
pub mod capability;
pub mod belief;
pub mod boundary;
pub mod checkpoint;
pub mod clocks;
pub mod compose;
pub mod consensus;
pub mod crdt;
pub mod controller;
pub mod council;
pub mod council_default;
pub mod criticality;
pub mod cynefin;
pub mod curated;
pub mod damping;
pub mod detect;
pub mod device;
pub mod dimension;
pub mod duality;
pub mod disaggregation;
pub mod division;
pub mod editing;
pub mod effect_capture;
pub mod ensemble;
pub mod enzyme;
pub mod error;
pub mod event;
pub mod flow;
pub mod ledger;
pub mod fold;
pub mod goodhart;
pub mod governance;
pub mod harmonics;
pub mod horizon;
pub mod holarchy;
pub mod holon;
pub mod immune;
pub mod imports;
pub mod independence;
pub mod intake_key;
pub mod inverse;
pub mod issuance;
pub mod juul;
pub mod kernel;
pub mod learned;
pub mod learning;
pub mod lens;
pub mod meme;
pub mod migrate;
pub mod mixture;
pub mod models;
pub mod monitor;
pub mod mutation;
pub mod obligation;
pub mod operative;
pub mod operator;
pub mod panel;
pub mod parse_rule;
pub mod parse_rule_learn;
pub mod path;
pub mod pawa;
pub mod period;
pub mod observability;
pub mod ooda;
pub mod pincer;
pub mod population;
pub mod predicate;
pub mod presentation;
pub mod pricing;
pub mod probe;
pub mod proposal;
pub mod provenance;
pub mod reach;
pub mod region;
pub mod reputation;
pub mod preattentive;
pub mod principal;
pub mod router;
pub mod rollup;
pub mod royalty;
pub mod schema;
pub mod score;
pub mod secret_shape;
pub mod semantic;
pub mod stranding;
pub mod signal;
pub mod package;
pub mod dag;
pub mod operatives;
pub mod parameter;
pub mod holon_typing;
pub mod embroidery;
pub mod spec;
pub mod admissibility;
pub mod control_system;
pub mod correlation;
pub mod domain_engine;
pub mod domain_map;
pub mod effect_journal;
pub mod event_time;
pub mod liveness;
pub mod egress;
pub mod outbox;
pub mod sigma;
pub mod strong_admit;
pub mod unscored;
pub mod canonical;
pub mod lift;
pub mod surface;
pub mod surfaces;
pub mod overlay;
pub mod tab;
pub mod state;
pub mod sync;
pub mod trust;
pub mod strategy;
pub mod telemetry;
pub mod tenet;
pub mod transducer;
pub mod transition;
pub mod treasury;
pub mod utility;
pub mod vclock;
pub mod version;
pub mod watermark;
pub mod widget;
pub mod windowing;

pub use admission::{admit_one, typecheck_constraint, ConstraintDecl, DeclError, Strategy, Verdict};
pub use approval::{
    ApprovalToken, Binding, EffectClass, NonceLedger, Simulated, TokenError, TraceError, Voted,
};
pub use boundary::{
    admit_boundary_change, preserves_closure, preserves_membership, Boundary, BoundaryChange,
    BoundaryDecl, BoundaryToken, ClosureViolation, Owned, Scope,
};
pub use compose::{
    compose, entails, wp, Change, Composed, EffectSummary, Entailment, Pathway, PathwayError,
    Rejected, Step, WpResult,
};
pub use consensus::{
    accepted_count, adopt, granted, propose, Accepted, Acceptor, Body, ByzantineBound,
    ConsensusError, Decision, Ledger, Promise, ProposalNumber, Round, RoundOutcome, RoundPhase,
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
pub use cynefin::{
    assess, domain_coverage, route_on_domain, transition_risk, CynefinError, Disorder, DomainCoverage,
    DomainReading, DomainRouting, Mismatch, ResponseMode, Suitability, TransitionRisk,
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
pub use belief::{
    Belief, BeliefError, BeliefReading, BeliefTracker, Collapse, Dynamics, SilenceAlert,
    SilenceSpec,
};
pub use semantic::{
    enforcement_of, is_deterministic, replay_under, CallOutcome, EnzymeCall, ReplayMode,
    SemanticOutcome,
};
pub use stranding::{assess_edit, EditImpact, HistoryStranded, InstanceHistory, Remedy};
pub use damping::{
    crossings, overshoot, read_trajectory, DampingError, DampingVerdict, Overshoot,
    PoleVerdict, ProportionalLaw, Side,
};
pub use checkpoint::{events_applied, replay, replay_to, Checkpoint, ReplayError};
pub use observability::{
    MeasuredBy, Observability, ObservabilityError, ObservabilityGraph, ObservabilityReport,
};
pub use harmonics::{
    read as read_harmonics, spectrum, Component, HarmonicReading, HarmonicsError,
    CycleProvenance, CycleVerdict, HarmonicsSpec, KnownCycle,
};
pub use clocks::{clock_findings, skew_of, ClockFinding, Skew, Suspect};
pub use dimension::{
    AccumulatingDim, ContestedDim, DimensionError, DimensionSchema, DimensionValue,
    SnapshotDim, Update,
};
pub use period::{
    partitions, Anchor, CivilDateTime, DstPolicy, Freq, LocalResolution, Period, PeriodError,
    Recurrence, TzError, TzProvider, Weekday,
};
pub use windowing::{
    sessionise, tumbling_from_recurrence, SessionSpec, SessionState, Sliding, Typology,
    WindowingError,
};
pub use watermark::{
    estimate_heuristic, Advance, Closing, LateOutcome, LateRecord, Lateness, SourceGuarantee,
    Watermark, WatermarkError, WatermarkKind, WatermarkTracker, Window, WindowError,
    WindowState,
};
pub use event::{
    dedupe, merge, order, CausalStamp, Event, Observation, Provenance, Source, SourceKind,
    Trust,
};
pub use error::{FoldError, FoldResult, StateError, StateResult};
pub use flow::{
    check_flows, classify_movement, flows_of, Crossing, Flow, FlowDirection, FlowRefusal,
    FlowRule, Movement,
};
pub use fold::{apply_mutation, diff_to_mutations, fold_events, FoldEvent};
pub use inverse::{invert, is_reversible, InverseError};
pub use kernel::{
    is_stable_toward_kernel, kernel_margin, runway, viability_kernel, viability_kernel_horizon,
    Kernel, KernelError, KernelStability, Move, Obligation, Runway, Space,
};
pub use migrate::{Applied, Compatibility, Emc, EmcVersionIds, MigrateError, Mu, StateMove};
pub use mutation::Mutation;
pub use obligation::{
    audit, check_obligation, flagged, ObligationError, ObligationReport, Soundness,
};
pub use operative::{
    dominance, geometric_mean, nash_product, pareto_frontier, scalarise, Alternative, Cynefin,
    Dominance, LegalMove, NashOutcome, Objective, Omega, Operative, OperativeError, Proposal,
    Ranking, Sense, Shared, StatePoint, Utility,
};
pub use operator::vendor::{is_person_pocket, person_key, pocket_for_number};
pub use dag::{Dag, DagError, DagRun};
pub use operatives::{attache, mentor};
pub use ledger::{position, Holding, Position};
pub use parameter::{DefParam, ParamError, Parameterised};
pub use overlay::Overlay;
pub use holon_typing::{joint_schema, typecheck_holon};
pub use spec::{validate_spec, SpecContext};
pub use embroidery::{Action, AuthoredEnzyme, Expr, MovementDecl};
pub use surface::{parse_enzyme, render_enzyme, SurfaceError};
pub use lift::{propose as propose_lifts, Lift};
pub use canonical::{agrees_with, canonical, state_hash, Agreement as StateAgreement};
pub use admissibility::{admissible_along, Authority, BindingRule, GlobalVerdict, Level};
pub use control_system::{Control, ControlSystem, Move as ControlMove, Reading as ControlReading, Runway as ControlRunway, Step as ControlStep};
pub use correlation::{fact_key, find_duplicates, FactKey, Suspected};
pub use domain_map::{DomainMap, DomainRegion};
pub use effect_journal::{EffectJournal, JournalEntry, Phase as EffectPhase};
pub use unscored::{unscored, Unscored, Unwatched};
pub use event_time::{event_time, skew_ms, Local as LocalStamp};
pub use liveness::{learned_theta, liveness as source_liveness, Heartbeat, Liveness};
pub use egress::{
    admit_egress, awaiting_a_human, dispose as dispose_egress, Delivery, EgressEffect,
    Reversibility,
};
pub use outbox::{apply as apply_ack, delay_ms, queue_depth, Ack, Backoff, Disposition, Pending};
pub use sigma::Sigma;
pub use strong_admit::{admit_strong, doomed_but_viable, StrongVerdict};
pub use tab::{tab_sides, TabSides};
pub use operator::{
    execute, execute_admitted, execute_afforded, execute_as, Authorization, Enforcement, Execution,
    OperatorMeta,
    OperatorResult, ParamDecl, ParamKind,
    Registry,
};
pub use capability::{Amplification, Attenuation, Capability, Rights};
pub use agent::{
    Agent, Attending, Considered, MemeTrustPolicy, Reasoning, Warrant, WarrantError,
};
pub use learned::{
    BoundError, FixedRule, IngressBound, LearnedRule, RuleSet, RuleTrust, Screening, TrustPolicy,
};
// ★ `vary`/`select`/`retain`/`learn` are deliberately NOT re-exported at the
// crate root: four of the most generic verbs in the language, and `retain` is
// already `Vec::retain` to every Rust reader. `learning::vary(...)` says which
// vary it is.
pub use learning::{
    Feedback, Fitness, LearningRound, LearningTrace, LibraryError, LibraryScope, Meme,
    MemeLibrary, MemeProvenance, Retention, Selection, Stabilisation, Variant, Varied, VaryOp,
};
// ★ `get`/`put`/`put_apply` are deliberately NOT re-exported at the crate root:
// three-letter verbs that generic would collide with something eventually, and
// `lens::get` reads better than `get` at a call site anyway.
pub use lens::{DefinitionGraph, DimLabel, EdgeKind, GraphEdge, Node, PutError, Supplied};
pub use preattentive::{
    encode_field, Arrow, Channel, DataDimension, Fraction, Hue, Motion, VisualAttribute,
    VisualSpec,
};
pub use widget::{
    EmitError, LoadError, LoadedWidget, WidgetDecl, WidgetEmission, WidgetSet,
};
pub use curated::{
    attention_cost, compose_view, compose_with_panels, knapsack_select, rank_selection,
    salience, BindingKey,
    BindingTable, PolicyError, SaliencePolicy, UrgencyBasis,
    Eligibility, Request, View, WidgetCandidate, Withdrawn,
};
pub use issuance::{Issuance, IssuanceId, Issued, Schedule};
pub use juul::{Affordability, Audit, Charge, Entry, Genesis, GenesisId, JuulLedger, MintAuthority};
pub use treasury::{pay_out, PayOut};
pub use royalty::{
    settle, split, Licence, Recipients, RevenueType, RoyaltyRole, Settlement, Share,
};
pub use governance::{ParameterSpec, Parameters};
pub use pawa::{candidate_pawa, compute_units, constraint_eval_count, Meter, PawaReading, Stats};
pub use panel::{PanelDecl, PanelError, PanelInView, PanelPolicy, PanelSet};
pub use strategy::{
    reachable_under, run as run_strategy, CompareOp, Condition, StepRecord, StrategyEdge,
    StrategyError, StrategyGraph, StrategyNode, Walk, WalkOutcome,
};
pub use attention::{
    broad_scan, narrow_scan, scan, Aperture, Attended, Attention, AttentionBudget, AttentionError,
    AttentionSpend, AttentionSpending, Reframing, Scan, Score, Stance,
};
pub use models::{
    Agreement, EffectiveN, Fidelity, ModelAgreement, Projection, SelfModel, WorldModel,
};
pub use principal::{
    effective_privilege, effective_privilege_with, permitted, permitted_with, Denial,
    MembershipEdge, Memberships, Skin, SkinRegistry, Tier,
};
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
pub use package::{
    definition_installs, operator_installs, strategy_installs, widget_installs, Admitted,
    Authenticity, Integrity, InstallVerdict, Kind, Origin, PackageProvenance,
};
pub use trust::{PackageTrust, TrustSignal, TrustStanding};
pub use sync::{
    reconcile, Concurrent, LogEntry, Reconciliation, Replayable, Replica,
};
pub use vclock::{
    assign_clocks, merge as merge_stamped, CausalVerdict, ClockError, Dimension, MergeResolution, Resolved,
    Stamped, VectorClock,
};
pub use version::{
    PathRollback, PreImage, Rollback, VersionDag, VersionError, VersionNode,
};
pub use enzyme::{propose_call, CallProposal, Effect, Proposed};
pub use ensemble::{
    DeadDrop, DeadDropBook, DecisionNode, Ensemble, EnsembleAnalysis, EnsembleError, InvariantSet,
    ModelTemplate, Prob, Resolution, RewardBasis, Scenario,
};
pub use goodhart::{coverage, referenced_dimensions, Coverage, GoodhartGuard, GuardError};
pub use presentation::{
    discharged_by_this_build, present, present_joint, CollapseRule, Collapsed, Discharge, Duty,
    PresentationError, Presented,
};
pub use holon::{
    transfer as holon_transfer, Leg, Link, Linked, Moving, Party, Transfer, TransferSettlement,
};
pub use effect_capture::{
    build_params, extract_amount, extract_currency_amount, infer, match_pocket_names,
    missing_required, narrow_by_verb, required_params_satisfiable, resolve_description, Capture,
    Choice, DeclaredParams, Inference, OperatorParams,
};
pub use parse_rule_learn::{synthesize_from_correction, verify_candidate, LearningRefusal};
pub use parse_rule::{
    apply_rule, run_rules, typecheck_rule, FieldKind, FieldSpec, NoOperators, OperatorUniverse,
    ParseRule, ParseRuleTrust, RuleStatus,
};
pub use transducer::{
    all_seed_rules, contains_sensitive_secret, parse_message, seed_rules, seed_sources,
    Transduction,
};
pub use rollup::{
    compute as compute_rollup, AggregateDecl, AggregateReading, ChildState, ChildStatus,
    Contribution, Contributor, Exclusion, Rollup, RollupError,
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
    Driven, Ingested, MonitorEngine, MonitorError, SustainWatch, TimedReading,
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
