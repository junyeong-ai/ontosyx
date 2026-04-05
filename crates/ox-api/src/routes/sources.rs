use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use ox_source::registry::SourceInput;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/sources/test-connection
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TestConnectionRequest {
    pub source_type: String,
    pub connection_string: Option<String>,
    pub schema_name: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TestConnectionResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tables: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
}

/// Classify a connection error message into a user-friendly error type.
fn classify_error(msg: &str) -> &'static str {
    let lower = msg.to_lowercase();
    if lower.contains("authentication") || lower.contains("password") || lower.contains("denied") {
        "auth_failed"
    } else if lower.contains("timeout") || lower.contains("connect") || lower.contains("refused") {
        "network"
    } else if lower.contains("not found") || lower.contains("does not exist") {
        "not_found"
    } else if lower.contains("permission") {
        "permission"
    } else {
        "connection_failed"
    }
}

#[utoipa::path(
    post,
    path = "/api/sources/test-connection",
    request_body = TestConnectionRequest,
    responses(
        (status = 200, description = "Connection test result", body = TestConnectionResponse),
        (status = 400, description = "Invalid input", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Sources",
)]
pub(crate) async fn test_source_connection(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<TestConnectionRequest>,
) -> Result<Json<TestConnectionResponse>, AppError> {
    principal.require_designer()?;

    let registry = &state.introspector_registry;

    if !registry.supports(&req.source_type) {
        return Err(AppError::bad_request(format!(
            "Unsupported source type: {}",
            req.source_type
        )));
    }

    let input = SourceInput {
        data: None,
        connection_string: req.connection_string.clone(),
        schema_name: req.schema_name.clone(),
    };

    info!(
        source_type = %req.source_type,
        "Testing source connection"
    );

    // Create the introspector (tests basic connectivity)
    let introspector = match registry.create(&req.source_type, input).await {
        None => {
            return Err(AppError::bad_request(format!(
                "Unsupported source type: {}",
                req.source_type
            )));
        }
        Some(Err(e)) => {
            let msg = e.to_string();
            warn!(source_type = %req.source_type, error = %msg, "Connection test: factory failed");
            return Ok(Json(TestConnectionResponse {
                success: false,
                table_count: None,
                tables: None,
                error: Some(msg.clone()),
                error_type: Some(classify_error(&msg).to_string()),
            }));
        }
        Some(Ok(i)) => i,
    };

    // Introspect schema with a 10-second timeout
    let schema_result =
        tokio::time::timeout(Duration::from_secs(10), introspector.introspect_schema()).await;

    match schema_result {
        Err(_elapsed) => {
            warn!(source_type = %req.source_type, "Connection test: schema introspection timed out");
            Ok(Json(TestConnectionResponse {
                success: false,
                table_count: None,
                tables: None,
                error: Some("Connection timed out after 10 seconds".to_string()),
                error_type: Some("network".to_string()),
            }))
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            warn!(source_type = %req.source_type, error = %msg, "Connection test: introspection failed");
            Ok(Json(TestConnectionResponse {
                success: false,
                table_count: None,
                tables: None,
                error: Some(msg.clone()),
                error_type: Some(classify_error(&msg).to_string()),
            }))
        }
        Ok(Ok(schema)) => {
            let table_names: Vec<String> = schema.tables.iter().map(|t| t.name.clone()).collect();
            let count = table_names.len();
            info!(
                source_type = %req.source_type,
                table_count = count,
                "Connection test: success"
            );
            Ok(Json(TestConnectionResponse {
                success: true,
                table_count: Some(count),
                tables: Some(table_names),
                error: None,
                error_type: None,
            }))
        }
    }
}
