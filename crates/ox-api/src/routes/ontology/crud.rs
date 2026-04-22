use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use ox_ontology::command::OntologyCommand;
use ox_ontology::ir::OntologyIR;
use ox_store::DesignProject;
use ox_store::store::{CursorPage, CursorParams};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::routes::projects::helpers::{
    assess_quality_from_project, get_design_options, load_mutable_project, reload_project,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/ontologies — paginated ontology identities
//
// Returns Level-1 identity rows plus a thin current-version summary. The
// IR itself is intentionally NOT embedded here — a 50-row list would
// otherwise pull 50 full hydrated ontologies into one response. Callers
// that need the IR load it on demand via the detail route.
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct CurrentVersionSummary {
    pub version_id: Uuid,
    pub version: String,
    pub committed_by: String,
    pub commit_message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct OntologyListItem {
    pub id: Uuid,
    pub lineage_id: String,
    pub name: String,
    /// LocalizedText JSONB — `{default, translations}` shape.
    #[schema(value_type = Object)]
    pub description: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// `None` iff the identity has no committed version yet — an ontology
    /// can exist with no commits during multi-step create flows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<CurrentVersionSummary>,
}

#[utoipa::path(
    get,
    path = "/api/ontologies",
    params(
        ("limit" = Option<u32>, Query, description = "Max items to return (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from a previous response"),
    ),
    responses(
        (status = 200, description = "Paginated list of ontology identities", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn list_ontologies(
    State(state): State<AppState>,
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<ApiResponse<Vec<OntologyListItem>>>, AppError> {
    let page = state
        .store
        .list_ontologies(&pagination)
        .await
        .map_err(AppError::from)?;

    // Fan out current-version lookups per row. Sequential await is fine for
    // page sizes bounded at 100 — the store's `get_current_version` hits a
    // partial index (`ontology_version_snapshots_current_idx`) so each call
    // is a single btree seek. Parallelism would force us to clone the store
    // Arc per row for negligible wall-time gain.
    let mut items = Vec::with_capacity(page.items.len());
    for row in page.items {
        let version = state
            .store
            .get_current_version(row.id)
            .await
            .map_err(AppError::from)?;
        items.push(OntologyListItem {
            id: row.id,
            lineage_id: row.lineage_id,
            name: row.name,
            description: row.description,
            created_at: row.created_at,
            updated_at: row.updated_at,
            current_version: version.map(|v| CurrentVersionSummary {
                version_id: v.id,
                version: v.version,
                committed_by: v.committed_by,
                commit_message: v.commit_message,
                created_at: v.created_at,
            }),
        });
    }

    Ok(ApiResponse::page(CursorPage {
        items,
        next_cursor: page.next_cursor,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/ontologies/:id — identity + hydrated IR + current version
//
// The list endpoint intentionally omits the IR (a 50-row page would
// otherwise pull 50 full hydrated ontologies). This detail route is the
// companion fetch when a caller needs to *work with* an ontology —
// e.g. loading it into the canvas or running adhoc queries.
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct OntologyDetail {
    pub id: Uuid,
    pub lineage_id: String,
    pub name: String,
    /// LocalizedText JSONB — `{default, translations}` shape.
    #[schema(value_type = Object)]
    pub description: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<CurrentVersionSummary>,
    /// Fully hydrated `OntologyIR` at the current version. `None` when the
    /// identity exists but has no committed version yet — the caller
    /// should treat it as an empty ontology (fresh project seed).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    pub ontology_ir: Option<ox_ontology::ir::OntologyIR>,
}

#[utoipa::path(
    get,
    path = "/api/ontologies/{id}",
    params(("id" = Uuid, Path, description = "Ontology identity ID")),
    responses(
        (status = 200, description = "Ontology detail with hydrated IR", body = OntologyDetail),
        (status = 404, description = "Ontology not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn get_ontology_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<OntologyDetail>>, AppError> {
    let identity = state
        .store
        .get_ontology(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let version = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?;
    let ir = if let Some(v) = &version {
        Some(
            state
                .store
                .load_version(v.id)
                .await
                .map_err(AppError::from)?,
        )
    } else {
        None
    };

    Ok(ApiResponse::of(OntologyDetail {
        id: identity.id,
        lineage_id: identity.lineage_id,
        name: identity.name,
        description: identity.description,
        created_at: identity.created_at,
        updated_at: identity.updated_at,
        current_version: version.map(|v| CurrentVersionSummary {
            version_id: v.id,
            version: v.version,
            committed_by: v.committed_by,
            commit_message: v.commit_message,
            created_at: v.created_at,
        }),
        ontology_ir: ir,
    }))
}

// ---------------------------------------------------------------------------
// PATCH /api/projects/{id}/ontology — apply batch of OntologyCommand
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct OntologyCommandsRequest {
    pub revision: i32,
    /// List of ontology mutation commands.
    #[schema(value_type = Vec<Object>)]
    pub commands: Vec<OntologyCommand>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct OntologyCommandsResponse {
    #[schema(value_type = Object)]
    pub project: DesignProject,
}

#[utoipa::path(
    patch,
    path = "/api/projects/{id}/ontology",
    params(
        ("id" = Uuid, Path, description = "Design project ID"),
    ),
    request_body = OntologyCommandsRequest,
    responses(
        (status = 200, description = "Commands applied", body = OntologyCommandsResponse),
        (status = 400, description = "Empty commands or invalid ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Command execution or validation failed", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn apply_ontology_commands(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<OntologyCommandsRequest>,
) -> Result<(StatusCode, Json<ApiResponse<OntologyCommandsResponse>>), AppError> {
    principal.require_designer()?;
    if req.commands.is_empty() {
        return Err(AppError::bad_request("commands must not be empty"));
    }

    let project = load_mutable_project(&state, id).await?;

    // Snapshot current state before mutation (best-effort)
    if let Some(ont) = &project.ontology
        && let Err(e) = state
            .store
            .create_ontology_snapshot(
                id,
                project.revision,
                ont,
                project.source_mapping.as_ref(),
                project.quality_report.as_ref(),
            )
            .await
    {
        warn!(project_id = %id, error = %e, "Failed to save ontology snapshot");
    }

    let mut ontology: OntologyIR = match project.ontology.as_ref() {
        None => return Err(AppError::no_ontology()),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| AppError::internal(format!("Corrupt ontology in project: {e}")))?,
    };

    // Apply each command sequentially, tracking changed element IDs
    let mut changed_element_ids: Vec<String> = Vec::new();
    for cmd in &req.commands {
        changed_element_ids.extend(cmd.affected_element_ids());
        let result = cmd.execute(&ontology).map_err(AppError::unprocessable)?;
        ontology = result.new_ontology;
    }

    if !changed_element_ids.is_empty() {
        let id_refs: Vec<&str> = changed_element_ids.iter().map(|s| s.as_str()).collect();
        if let Err(e) = state
            .store
            .invalidate_for_elements(&ontology.id, &id_refs, "ontology_command")
            .await
        {
            warn!(error = %e, "Failed to invalidate verifications for changed elements");
        }
    }

    let errors = ontology.validate();
    if !errors.is_empty() {
        return Err(AppError::unprocessable(errors.join("; ")));
    }

    let opts = get_design_options(&project);
    let quality_report = assess_quality_from_project(
        &project,
        &ontology,
        &opts.excluded_tables,
        &opts.column_clarifications,
    )?;

    let ontology_json = AppError::to_json(&ontology)?;
    let qr_json = AppError::to_json(&quality_report)?;

    state
        .store
        .update_design_result(
            id,
            &ontology_json,
            project.source_mapping.as_ref(),
            Some(&qr_json),
            req.revision,
        )
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    Ok((
        StatusCode::OK,
        ApiResponse::of(OntologyCommandsResponse { project: updated }),
    ))
}
