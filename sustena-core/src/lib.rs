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
//! engine does not yet do. Not started; nothing here anticipates it.
//!
//! ## Slices
//!
//! | Slice | Module | Status |
//! |---|---|---|
//! | State | [`state`] | R1 |
//! | Event fold | [`fold`] | R1 |
//! | Rules | [`predicate`] | R1 |
//! | Operators | [`operator`] | R1 |
//! | Council | — | not started |

pub mod error;
pub mod fold;
pub mod mutation;
pub mod operator;
pub mod path;
pub mod predicate;
pub mod state;

pub use error::{FoldError, FoldResult, StateError, StateResult};
pub use fold::{apply_mutation, diff_to_mutations, fold_events, FoldEvent};
pub use mutation::Mutation;
pub use operator::{execute, Enforcement, Execution, OperatorResult, Registry};
pub use predicate::{check, parse_predicate, Predicate};
pub use state::State;

/// Version of the conformance contract this build satisfies.
///
/// Bumped when the vector format changes, so a stale vector set fails loudly
/// instead of silently passing against the wrong expectations.
pub const CONFORMANCE_VERSION: u32 = 1;
