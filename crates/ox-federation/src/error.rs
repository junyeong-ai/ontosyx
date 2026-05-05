//! Federation error surface.
//!
//! Two inward-facing concerns:
//!
//! 1. DataFusion returns `datafusion::error::DataFusionError`. Surfacing
//!    that type directly would leak the engine into every caller.
//! 2. Adapter calls return `ox_core::error::OxError`. Flattening both
//!    into one error type lets the API layer respond with a single
//!    error-class mapping.
//!
//! `FederationError` is therefore a small enum over the two origins
//! plus a typed variant for federation-specific policy rejections
//! (e.g. `UNSUPPORTED_PATH_ON_SOURCE`).

use ox_core::error::OxError;
use thiserror::Error;

pub type FederationResult<T> = Result<T, FederationError>;

#[derive(Debug, Error)]
pub enum FederationError {
    /// An adapter call failed. The underlying `OxError` carries the
    /// adapter-level context (source id, table, retry class).
    #[error(transparent)]
    Adapter(#[from] OxError),

    /// DataFusion rejected the plan or failed mid-execution.
    #[error("datafusion: {0}")]
    DataFusion(#[from] datafusion::error::DataFusionError),

    /// Arrow could not represent the shape the adapter produced —
    /// e.g. a row with a column count that disagrees with the schema.
    #[error("arrow: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    /// Planner-level rejection. The message should quote the concrete
    /// capability that failed (e.g. "recursive CTE not supported by
    /// source `mongo_users`") so the UI / agent can reason about
    /// fallback options.
    #[error("{0}")]
    Unsupported(String),

    /// Invariant violation inside the federation engine — a contract
    /// the planner upholds was breached at runtime. These are
    /// programmer errors, not user input errors; they propagate as
    /// `Result` instead of panicking so the API layer can return a
    /// 5xx with a request-id rather than tearing down the worker.
    #[error("internal: {0}")]
    Internal(String),
}

impl FederationError {
    /// Construct a planner-level rejection with a descriptive message.
    /// Prefer this over `anyhow!`-style wrapping so the
    /// `Unsupported` variant stays distinguishable downstream.
    pub fn unsupported(message: impl Into<String>) -> Self {
        FederationError::Unsupported(message.into())
    }

    /// Construct an internal invariant-violation error. Use only
    /// where a planner contract documents the precondition; the
    /// message should name the contract (e.g. "build_union_scan
    /// called with fewer than two entries").
    pub fn internal(message: impl Into<String>) -> Self {
        FederationError::Internal(message.into())
    }
}
