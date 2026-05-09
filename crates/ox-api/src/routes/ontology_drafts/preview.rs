//! `POST /api/ontology-drafts/source-preview`
//!
//! Cheap table listing for an arbitrary [`DataSourceSpec`] — no
//! introspection, no profiling, no persistence. Designers call this
//! before [`super::lifecycle::create_ontology_draft`] so they can pick a
//! subset of tables to feed into the design flow.
//!
//! The route mirrors the admin-side
//! `GET /api/admin/federation/adapters/{source_id}/tables` shape but
//! accepts a fresh connection in the body (no source-id required) and
//! sits behind the designer role.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

use super::helpers::build_adapter;
use super::types::DataSourceSpec;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PreviewSourceRequest {
    /// Connection details for the source — same shape used by
    /// `CreateOntologyDraftRequest::origin::Source { source }`.
    #[serde(flatten)]
    pub source: DataSourceSpec,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PreviewSourceResponse {
    /// Source kind — useful for the UI to render type-specific
    /// affordances (e.g., schema picker for SQL backends).
    pub source_type: String,
    pub tables: Vec<PreviewTableSummary>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PreviewTableSummary {
    pub name: String,
    /// Approximate row count from cheap backend statistics.
    /// `None` when the backend has no cheap path to this answer.
    pub estimated_row_count: Option<u64>,
    /// Number of columns reported by the catalog.
    pub column_count: u32,
    /// Last-modified timestamp when the backend exposes one.
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/source-preview",
    request_body = PreviewSourceRequest,
    responses(
        (status = 200, description = "Cheap table listing for the source",
            body = PreviewSourceResponse),
        (status = 400, description = "Source kind not previewable or empty data",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 403, description = "Designer role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn preview_source(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<PreviewSourceRequest>,
) -> Result<Json<ApiResponse<PreviewSourceResponse>>, AppError> {
    principal.require_designer()?;

    let prepared = build_adapter(req.source, &state.adapter_registry).await?;
    let source_type = prepared.config.source_type.to_string();

    let summaries = prepared
        .adapter
        .list_tables_with_summary()
        .await
        .map_err(AppError::from)?;

    let mut tables: Vec<PreviewTableSummary> = summaries
        .into_iter()
        .map(|s| PreviewTableSummary {
            name: s.name,
            estimated_row_count: s.estimated_row_count,
            column_count: s.column_count,
            last_modified: s.last_modified,
        })
        .collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ApiResponse::of(PreviewSourceResponse {
        source_type,
        tables,
    }))
}
