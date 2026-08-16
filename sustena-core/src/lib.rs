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

pub mod admission;
pub mod approval;
pub mod compose;
pub mod council;
pub mod editing;
pub mod error;
pub mod event;
pub mod fold;
pub mod inverse;
pub mod mutation;
pub mod operator;
pub mod path;
pub mod predicate;
pub mod principal;
pub mod schema;
pub mod state;
pub mod transition;

pub use admission::{admit_one, typecheck_constraint, ConstraintDecl, DeclError, Strategy, Verdict};
pub use approval::{
    ApprovalToken, Binding, EffectClass, NonceLedger, Simulated, TokenError, TraceError, Voted,
};
pub use compose::{
    compose, entails, wp, Change, Composed, EffectSummary, Entailment, Pathway, PathwayError,
    Rejected, Step, WpResult,
};
pub use council::{aggregate_delegated_votes, resolve, DelegatedVote, ProposalStatus, ResolutionInput, VoteChoice};
pub use editing::{
    admit_edit, safe, typecheck, AuthorityError, CouncilMint, Definition, Edit, EditAdmission,
    EditAuthority, EditEffect, EditError, EditToken, Governance, Instance, Migration, Stranded,
};
pub use event::{dedupe, merge, order, CausalStamp, Event, Observation, Provenance};
pub use error::{FoldError, FoldResult, StateError, StateResult};
pub use fold::{apply_mutation, diff_to_mutations, fold_events, FoldEvent};
pub use inverse::{invert, is_reversible, InverseError};
pub use mutation::Mutation;
pub use operator::{
    execute, execute_admitted, execute_as, Authorization, Enforcement, Execution, OperatorResult,
    Registry,
};
pub use principal::{effective_privilege, permitted, Denial, MembershipEdge, Memberships, Tier};
pub use predicate::{check, parse_predicate, Predicate};
pub use schema::{bind, preserves_shape, validate, DimType, Schema};
pub use state::State;
pub use transition::{
    check_all as check_transitions, Direction, Quantity, Tolerance, TransitionDeclError,
    TransitionRule, TransitionViolation,
};

/// Version of the conformance contract this build satisfies.
///
/// Bumped when the vector format changes, so a stale vector set fails loudly
/// instead of silently passing against the wrong expectations.
pub const CONFORMANCE_VERSION: u32 = 1;
