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
            message: format!(
                "Rate limit exceeded. Retry after {retry_after_secs} seconds."
            ),
            details: None,
            headers: Some(Box::new(headers)),
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
        OxError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
        OxError::Conflict { .. } => (StatusCode::CONFLICT, "conflict"),
        OxError::Ontology { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "ontology_error"),
        OxError::Compilation { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "compilation_error"),
        OxError::UnsupportedOperation { .. } => (StatusCode::NOT_IMPLEMENTED, "unsupported"),
        OxError::Serialization(_) => (StatusCode::BAD_REQUEST, "serialization_error"),
        OxError::Runtime { .. } | OxError::Contextual { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
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
        Self {
            status,
            error_type,
            message: err.to_string(),
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
