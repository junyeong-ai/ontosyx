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

#[derive(Debug, Error)]
pub enum OxError {
    #[error("Compilation error: {message}")]
    Compilation { message: String },

    #[error("Runtime error: {message}")]
    Runtime { message: String },

    #[error("Validation error: {field} — {message}")]
    Validation { field: String, message: String },

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
