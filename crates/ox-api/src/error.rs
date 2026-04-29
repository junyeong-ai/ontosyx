use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use ox_core::error::OxError;

// ---------------------------------------------------------------------------
// AppError — centralized API error handling
//
// All route handlers return `Result<T, AppError>`. OxError converts
// automatically via `From<OxError>`, mapping each variant to the
// appropriate HTTP status code.
//
// Response format (industry-standard structured error):
//   { "error": { "type": "not_found", "message": "Conversation not found" } }
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    error_type: &'static str,
    message: String,
    details: Option<Box<serde_json::Value>>,
    headers: Option<Box<HeaderMap>>,
}

impl AppError {
    pub fn not_found(entity: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error_type: "not_found",
            message: format!("{entity} not found"),
            details: None,
            headers: None,
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            error_type: "service_unavailable",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    /// 410 Gone — resource existed but is no longer available (e.g., a
    /// share token whose `expires_at` is in the past). Distinct from
    /// `not_found` so clients can render a "this link expired" message
    /// instead of a generic 404.
    pub fn gone(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GONE,
            error_type: "gone",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            error_type: "unprocessable_entity",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn unprocessable_with_details(
        error_type: &'static str,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            error_type,
            message: message.into(),
            details: Some(Box::new(details)),
            headers: None,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error_type: "bad_request",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn quality_gate(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            error_type: "quality_gate",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            error_type: "timeout",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            error_type: "unauthorized",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            error_type: "forbidden",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn rate_limited(retry_after_secs: u64) -> Self {
        let mut headers = HeaderMap::new();
        if let Ok(v) = retry_after_secs.to_string().parse() {
            headers.insert("retry-after", v);
        }
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            error_type: "rate_limited",
            message: format!("Rate limit exceeded. Retry after {retry_after_secs} seconds."),
            details: None,
            headers: Some(Box::new(headers)),
        }
    }

    /// 429 TOO_MANY_REQUESTS with a caller-supplied message — used by the
    /// per-user chat-stream concurrency limiter where the relevant
    /// signal isn't "slow down" but "close an existing stream first".
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            error_type: "concurrency_cap",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            error_type: "conflict",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error_type: "internal_error",
            message: message.into(),
            details: None,
            headers: None,
        }
    }

    /// Serialize a value to JSON, converting serialization failures to AppError.
    pub fn to_json(value: &impl serde::Serialize) -> Result<serde_json::Value, Self> {
        serde_json::to_value(value)
            .map_err(|e| Self::internal(format!("Serialization failed: {e}")))
    }

    // -----------------------------------------------------------------------
    // Domain-specific error constructors (error message catalog)
    //
    // Centralizes hardcoded error strings so route handlers use semantic
    // factory methods instead of duplicating string literals.
    // -----------------------------------------------------------------------

    pub fn project_not_found() -> Self {
        Self::not_found("Design project")
    }

    pub fn ontology_not_found() -> Self {
        Self::not_found("Saved ontology")
    }

    pub fn execution_not_found() -> Self {
        Self::not_found("Query execution")
    }

    pub fn pin_not_found() -> Self {
        Self::not_found("Pin")
    }

    pub fn perspective_not_found() -> Self {
        Self::not_found("Perspective")
    }

    pub fn revision_not_found() -> Self {
        Self::not_found("Ontology revision")
    }

    pub fn no_ontology() -> Self {
        Self::bad_request("Project has no ontology")
    }

    pub fn no_runtime() -> Self {
        Self::service_unavailable("Graph database not connected")
    }

    pub fn empty_source_data() -> Self {
        Self::bad_request("Source data must not be empty")
    }

    pub fn validation(field: &str, message: &str) -> Self {
        Self::bad_request(format!("{field}: {message}"))
    }
}

/// Map an OxError variant (non-Contextual) to HTTP status + error type.
fn ox_error_status(err: &OxError) -> (StatusCode, &'static str) {
    match err {
        OxError::Validation { .. } => (StatusCode::BAD_REQUEST, "validation_error"),
        OxError::Parse { .. } => (StatusCode::BAD_REQUEST, "parse_error"),
        OxError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
        OxError::Conflict { .. } => (StatusCode::CONFLICT, "conflict"),
        OxError::Ontology { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "ontology_error"),
        OxError::Compilation { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "compilation_error"),
        OxError::UnsupportedOperation { .. } => (StatusCode::NOT_IMPLEMENTED, "unsupported"),
        OxError::Serialization(_) => (StatusCode::BAD_REQUEST, "serialization_error"),
        OxError::MissingContext { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, "missing_context")
        }
        OxError::Runtime { .. } | OxError::Contextual { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
        }
    }
}

/// Stable redacted message returned to clients on 5xx errors. ADR-0045:
/// internal driver / wrapper text (sqlx SQLSTATE prose, neo4rs frame
/// dumps, file path prefixes from `OxError::Contextual`) must never
/// reach response bodies. The full text is kept on the server side
/// via `tracing::error!`; clients correlate via the `x-request-id`
/// response header set by the request-id middleware.
fn redacted_5xx_message(error_type: &'static str) -> &'static str {
    match error_type {
        "missing_context" => {
            "Server configuration error. Contact support with the \
             request id from the x-request-id response header."
        }
        "unsupported" => {
            "This operation is not supported by the configured backend."
        }
        _ => {
            "Internal server error. Contact support with the request \
             id from the x-request-id response header."
        }
    }
}

impl From<OxError> for AppError {
    fn from(err: OxError) -> Self {
        // Contextual wraps another OxError; delegate to inner source for status mapping
        // but use the full Display (which includes target/location prefix) for message.
        let (status, error_type) = match &err {
            OxError::Contextual { source, .. } => ox_error_status(source),
            other => ox_error_status(other),
        };

        let message = if status.is_server_error() {
            // ADR-0045: Log the verbose form server-side at `error`
            // level — operators get the full driver text + Contextual
            // chain for diagnosis. The wire response carries only a
            // stable string; correlation is via x-request-id.
            tracing::error!(
                error_type,
                status = status.as_u16(),
                error = %err,
                "5xx response"
            );
            redacted_5xx_message(error_type).to_string()
        } else {
            err.to_string()
        };

        Self {
            status,
            error_type,
            message,
            details: None,
            headers: None,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        crate::metrics::record_error(self.error_type);
        let mut error = serde_json::json!({
            "type": self.error_type,
            "message": self.message,
        });
        if let Some(details) = self.details {
            error["details"] = *details;
        }
        let body = serde_json::json!({ "error": error });
        let mut response = (self.status, Json(body)).into_response();
        if let Some(headers) = self.headers {
            response.headers_mut().extend(*headers);
        }
        response
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    /// 5xx bodies must never carry driver text or Contextual prefixes.
    /// Tripping this assertion means a future change re-leaked internal
    /// detail through the response body — see ADR-0045.
    #[test]
    fn runtime_5xx_redacts_driver_text_from_message() {
        let leaky = OxError::Runtime {
            message: "PostgreSQL error [42P01]: relation \"foo\" does not exist \
                      at /Users/dev/.cargo/registry/src/sqlx-core-0.8.0/src/error.rs:42"
                .to_string(),
        };
        let app_err: AppError = leaky.into();
        assert_eq!(app_err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            !app_err.message.contains("PostgreSQL"),
            "redacted message must drop driver name"
        );
        assert!(
            !app_err.message.contains("/Users/"),
            "redacted message must drop filesystem paths"
        );
        assert!(
            !app_err.message.contains("42P01"),
            "redacted message must drop SQLSTATE codes"
        );
        assert!(
            app_err.message.contains("x-request-id"),
            "redacted message must point clients at the correlation header"
        );
    }

    #[test]
    fn missing_context_5xx_uses_distinct_redacted_message() {
        let err = OxError::MissingContext {
            kind: "workspace".to_string(),
            message: "internal: forgot to wrap with WORKSPACE_ID.scope".to_string(),
        };
        let app_err: AppError = err.into();
        assert_eq!(app_err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(app_err.error_type, "missing_context");
        assert!(
            !app_err.message.contains("WORKSPACE_ID"),
            "redacted message must drop internal symbol names"
        );
        assert!(app_err.message.contains("Server configuration error"));
    }

    #[test]
    fn validation_4xx_keeps_full_message() {
        let err = OxError::Validation {
            field: "email".to_string(),
            message: "must contain '@'".to_string(),
        };
        let app_err: AppError = err.into();
        assert_eq!(app_err.status, StatusCode::BAD_REQUEST);
        // 4xx is the user's fault — keep the precise message so the
        // client can fix the request without spelunking server logs.
        assert!(app_err.message.contains("email"));
        assert!(app_err.message.contains("@"));
    }

    #[test]
    fn contextual_wrapping_a_runtime_still_redacts_at_5xx() {
        let inner = OxError::Runtime {
            message: "neo4rs: bolt frame oversized at handshake".to_string(),
        };
        let wrapped = inner.with_context("graph:neo4j", "graph_runtime::execute");
        let app_err: AppError = wrapped.into();
        assert_eq!(app_err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            !app_err.message.contains("neo4rs"),
            "Contextual must not bypass the redaction wrapper"
        );
        assert!(
            !app_err.message.contains("graph_runtime"),
            "Contextual prefix must not reach the body"
        );
    }
}
