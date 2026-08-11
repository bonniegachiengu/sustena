//! Typed errors for the core.
//!
//! The Python engine signals these with exceptions (`StatePathError`,
//! `StateValueError`, `FoldError`). Named identically here so a conformance
//! vector can assert the SAME failure, not merely "something failed" — a port
//! that fails for a different reason has not matched behaviour.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum StateError {
    /// A dot-path did not resolve to a valid location.
    /// Mirrors Python `StatePathError`.
    #[error("{0}")]
    Path(String),

    /// A value operation was invalid — incrementing a non-number, or a
    /// decrement that would go negative. Mirrors Python `StateValueError`.
    #[error("{0}")]
    Value(String),
}

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum FoldError {
    /// A mutation could not be replayed. Mirrors Python `FoldError`.
    ///
    /// Deliberately loud. Silently returning wrong state from a log the engine
    /// itself produced would defeat the point of the fold being a proof.
    #[error("{0}")]
    Unreplayable(String),
}

impl From<StateError> for FoldError {
    fn from(e: StateError) -> Self {
        FoldError::Unreplayable(e.to_string())
    }
}

pub type StateResult<T> = Result<T, StateError>;
pub type FoldResult<T> = Result<T, FoldError>;
