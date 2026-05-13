use serde::Serialize;
use thiserror::Error;

/// Additional context for errors that occur in specific locations.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorContext {
    /// What component/resource was being operated on (e.g., "neo4j", "prompt:design_ontology")
    pub target: String,
    /// Where in the pipeline the error occurred (e.g., "compile_query", "execute_load.batch[3]")
    pub location: String,
}

/// Stable wire classification for LLM-side failures.
///
/// Carried inside [`OxError::Llm`] so the API boundary picks a
/// single typed `ApiErrorCode` per variant. The FE i18n catalogue
/// at `errors.llm_<code>` produces the user-facing prose. The enum
/// is closed — every entelix error variant the brain surfaces
/// folds into one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LlmErrorCode {
    /// Caller-shaped failure — empty messages, schema mismatch, or
    /// a 400 from the provider the brain cannot retry past.
    InvalidRequest,
    /// Provider rate limit (HTTP 429). The companion
    /// `retry_after_secs` on [`OxError::Llm`] carries the vendor's
    /// `Retry-After` hint when present.
    RateLimited,
    /// Credential failed at the auth boundary or the provider
    /// rejected the bearer (HTTP 401 / 403).
    AuthFailed,
    /// Network / TLS / DNS class failure — the SDK never received
    /// a complete HTTP framing.
    Transient,
    /// Provider responded with a 5xx — the vendor is reachable but
    /// failed the call.
    ProviderUnavailable,
    /// A configured `RunBudget` axis fired (token / cost / request
    /// / tool-call cap).
    BudgetExceeded,
    /// The execution context's cancellation token fired before the
    /// LLM call completed.
    Cancelled,
    /// The execution context's deadline fired before the LLM call
    /// completed.
    DeadlineExceeded,
    /// JSON serialisation failed at an entelix-managed boundary
    /// (codec, tool I/O).
    SerializationError,
    /// Dispatch raised an interrupt for human review.
    Interrupted,
    /// Validation retry budget exhausted — the model never produced
    /// a response that satisfied the typed-output validator.
    ModelRetry,
}

impl LlmErrorCode {
    /// Stable wire string. Mirrors the snake_case identifier the FE
    /// i18n catalogue keys on at `errors.llm_<code>`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::RateLimited => "rate_limited",
            Self::AuthFailed => "auth_failed",
            Self::Transient => "transient",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::BudgetExceeded => "budget_exceeded",
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::SerializationError => "serialization_error",
            Self::Interrupted => "interrupted",
            Self::ModelRetry => "model_retry",
        }
    }
}

impl std::fmt::Display for LlmErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
pub enum OxError {
    #[error("Compilation error: {message}")]
    Compilation { message: String },

    #[error("Runtime error: {message}")]
    Runtime { message: String },

    #[error("Validation error: {field} — {message}")]
    Validation { field: String, message: String },

    #[error("Parse error in {field}: {source}")]
    Parse {
        field: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },

    #[error("Ontology error: {message}")]
    Ontology { message: String },

    #[error("IR serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Unsupported operation for target: {target} — {operation}")]
    UnsupportedOperation { target: String, operation: String },

    #[error("Not found: {entity}")]
    NotFound { entity: String },

    #[error("Conflict: {message}")]
    Conflict { message: String },

    /// A required scope or context was not set on the calling
    /// task. The canonical case is a store mutation invoked
    /// outside any workspace scope: with neither `WORKSPACE_ID`
    /// nor `SYSTEM_BYPASS` task-locals bound, RLS would silently
    /// deny the row, leaving the caller to wonder why their write
    /// "succeeded" with zero rows affected. The variant is
    /// generic on `kind` so the same shape covers project,
    /// user, or any future scope axis without a per-axis variant.
    #[error("Missing {kind} context: {message}")]
    MissingContext { kind: String, message: String },

    /// LLM-side failure carrying a stable typed classification.
    /// The API boundary maps `code` onto an `ApiErrorCode::Llm*`
    /// variant; the FE i18n catalogue at `errors.llm_<code>`
    /// produces the user-facing prose. `retry_after_secs` carries
    /// the vendor's `Retry-After` hint when one was present so
    /// the API layer can emit a matching response header.
    #[error("LLM {code}: {detail}")]
    Llm {
        code: LlmErrorCode,
        detail: String,
        retry_after_secs: Option<u64>,
    },

    /// An error with additional diagnostic context.
    #[error("[{target}/{location}] {source}")]
    Contextual {
        source: Box<OxError>,
        target: String,
        location: String,
    },
}

impl OxError {
    /// Attach context to an error for better diagnostics.
    /// If the error is already Contextual, replaces the outer context
    /// (flattens to prevent nested wrapping).
    pub fn with_context(self, target: impl Into<String>, location: impl Into<String>) -> Self {
        let source = match self {
            OxError::Contextual { source, .. } => source,
            other => Box::new(other),
        };
        OxError::Contextual {
            source,
            target: target.into(),
            location: location.into(),
        }
    }

    /// Returns the diagnostic context if this is a `Contextual` error.
    pub fn context(&self) -> Option<ErrorContext> {
        match self {
            OxError::Contextual {
                target, location, ..
            } => Some(ErrorContext {
                target: target.clone(),
                location: location.clone(),
            }),
            _ => None,
        }
    }
}

pub type OxResult<T> = Result<T, OxError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_with_context() {
        let base = OxError::Runtime {
            message: "connection lost".to_string(),
        };
        let contextual = base.with_context("neo4j", "execute_query");

        let display = format!("{contextual}");
        assert!(
            display.contains("neo4j"),
            "display should include target: {display}"
        );
        assert!(
            display.contains("execute_query"),
            "display should include location: {display}"
        );
        assert!(
            display.contains("connection lost"),
            "display should include source message: {display}"
        );

        // Verify it wraps as Contextual variant
        match &contextual {
            OxError::Contextual {
                source,
                target,
                location,
            } => {
                assert_eq!(target, "neo4j");
                assert_eq!(location, "execute_query");
                assert!(matches!(source.as_ref(), OxError::Runtime { .. }));
            }
            _ => panic!("expected Contextual variant"),
        }
    }

    #[test]
    fn test_contextual_error_context_method() {
        let base = OxError::Compilation {
            message: "syntax error".to_string(),
        };
        // Non-contextual error should return None
        assert!(base.context().is_none());

        let contextual = base.with_context("cypher", "compile_query");
        let ctx = contextual.context().expect("should have context");
        assert_eq!(ctx.target, "cypher");
        assert_eq!(ctx.location, "compile_query");
    }
}
