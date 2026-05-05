use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use ox_ontology::command::OntologyCommand;
use ox_ontology::ir::OntologyIR;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::routes::ontology_drafts::helpers::{
    assess_quality_from_ontology_draft, get_design_options, load_mutable_ontology_draft, reload_ontology_draft,
};
use crate::routes::ontology_drafts::types::OntologyDraftView;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/ontology — workspace's canonical ontology + hydrated IR
//
// Workspace × ontology is 1:1, so this is the single read path —
// no list, no `{id}` segment, no name lookup. Returns the identity
// row, the current version summary, and the hydrated IR.
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
pub struct OntologyDetail {
    pub id: Uuid,
    pub lineage_id: String,
    pub name: String,
    /// Localized description. Stored as JSONB; the OpenAPI surface
    /// uses the typed `LocalizedText` from the ontology IR so FE
    /// codegen carries the same shape.
    #[schema(value_type = ox_core::i18n::LocalizedText)]
    pub description: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<CurrentVersionSummary>,
    /// Fully hydrated `OntologyIR` at the current version. `None`
    /// when the identity exists but has no committed version yet —
    /// the caller should treat it as an empty ontology (fresh
    /// project seed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ontology_ir: Option<ox_ontology::ir::OntologyIR>,
}

/// Workspace ontology singleton response. Workspace × ontology
/// is 1:1 — at most one canonical row exists, and the
/// pre-canonical (greenfield) phase is a normal lifecycle state,
/// not a missing resource. `ontology` is `None` during that
/// phase; `404` is reserved for genuine missing-resource lookups
/// (`/api/ontology-drafts/{id}` for a non-existent draft).
#[derive(Serialize, utoipa::ToSchema)]
pub struct WorkspaceOntologyResponse {
    pub ontology: Option<OntologyDetail>,
}

#[utoipa::path(
    get,
    path = "/api/ontology",
    responses(
        (status = 200, description = "Workspace ontology singleton — `ontology` is null in the greenfield phase", body = WorkspaceOntologyResponse),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn get_workspace_ontology_detail(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<WorkspaceOntologyResponse>>, AppError> {
    let Some(identity) = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
    else {
        return Ok(ApiResponse::of(WorkspaceOntologyResponse { ontology: None }));
    };
    let version = state
        .store
        .find_current_version(identity.id)
        .await
        .map_err(AppError::from)?;
    let ir = if let Some(v) = &version {
        state
            .store
            .get_ontology_ir(v.id)
            .await
            .map_err(AppError::from)?
    } else {
        None
    };

    Ok(ApiResponse::of(WorkspaceOntologyResponse {
        ontology: Some(OntologyDetail {
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
        }),
    }))
}

// ---------------------------------------------------------------------------
// GET /api/ontology/versions — workspace canonical version history
//
// Workspace × ontology = 1:1, so the version axis is the only
// remaining navigation surface for the canonical lineage. The
// branching dashboard reads this list as the trunk and hangs
// drafts off each version via `parent_version_id`.
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct OntologyVersionEntry {
    pub id: Uuid,
    pub version: String,
    pub committed_by: String,
    pub commit_message: String,
    pub created_at: DateTime<Utc>,
    /// Parent version this commit was branched from. `None` for
    /// the very first version of the canonical lineage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_version_id: Option<Uuid>,
    /// `true` when this is the current canonical head — the row
    /// whose `valid_to` is null.
    pub is_current: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct OntologyVersionsResponse {
    pub versions: Vec<OntologyVersionEntry>,
}

#[utoipa::path(
    get,
    path = "/api/ontology/versions",
    responses(
        (status = 200, description = "Canonical version history, newest first", body = OntologyVersionsResponse),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn list_canonical_versions(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<OntologyVersionsResponse>>, AppError> {
    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?;
    let Some(identity) = identity else {
        // Greenfield — no canonical yet, no versions.
        return Ok(ApiResponse::of(OntologyVersionsResponse {
            versions: Vec::new(),
        }));
    };

    // Cap at 100 versions for the dashboard's tree view; deeper
    // history goes through the existing per-version endpoints.
    let snapshots = state
        .store
        .list_versions(identity.id, 100)
        .await
        .map_err(AppError::from)?;

    let versions = snapshots
        .into_iter()
        .map(|s| OntologyVersionEntry {
            id: s.id,
            version: s.version,
            committed_by: s.committed_by,
            commit_message: s.commit_message,
            created_at: s.created_at,
            parent_version_id: s.parent_version_id,
            is_current: s.valid_to.is_none(),
        })
        .collect();

    Ok(ApiResponse::of(OntologyVersionsResponse { versions }))
}

// ---------------------------------------------------------------------------
// PATCH /api/ontology-drafts/{id}/ontology — apply batch of OntologyCommand
//
// Draft-mediated edit path: command batch lands on the project's
// in-flight `ontology` JSONB, not the canonical. Governance gate
// fires at the canonical-commit boundary (`complete_ontology_draft`),
// where the parent_version_id check refuses stale commits.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ApplyOntologyCommandsRequest {
    pub revision: i32,
    /// List of ontology mutation commands.
    #[schema(value_type = Vec<Object>)]
    pub commands: Vec<OntologyCommand>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ApplyOntologyCommandsResponse {
    pub project: OntologyDraftView,
}

#[utoipa::path(
    patch,
    path = "/api/ontology-drafts/{id}/ontology",
    params(
        ("id" = Uuid, Path, description = "Design project ID"),
    ),
    request_body = ApplyOntologyCommandsRequest,
    responses(
        (status = 200, description = "Commands applied", body = ApplyOntologyCommandsResponse),
        (status = 400, description = "Empty commands or invalid ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Command execution or validation failed", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn apply_ontology_commands(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ApplyOntologyCommandsRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ApplyOntologyCommandsResponse>>), AppError> {
    principal.require_designer()?;
    if req.commands.is_empty() {
        return Err(AppError::required_field_empty("commands"));
    }

    let project = load_mutable_ontology_draft(&state, id).await?;

    // Snapshot current state before mutation (best-effort)
    if let Some(ont) = &project.ontology
        && let Err(e) = state
            .store
            .create_ontology_snapshot(
                id,
                project.revision,
                ont,
                project.quality_report.as_ref(),
            )
            .await
    {
        warn!(ontology_draft_id = %id, error = %e, "Failed to save ontology snapshot");
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
        let result = cmd.execute(&ontology).map_err(AppError::edit_operation_rejected)?;
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
        return Err(AppError::ontology_invariant_violation(errors));
    }

    let opts = get_design_options(&project);
    let quality_report = assess_quality_from_ontology_draft(
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
            Some(&qr_json),
            req.revision,
        )
        .await
        .map_err(AppError::from)?;

    let updated = reload_ontology_draft(&state, id).await?;

    // Broadcast the commit to every collaborator subscribed to the
    // project room so their `commandStack` baselines advance and any
    // in-flight conflict resolution surfaces the symmetric remote-ops
    // inventory in `<CommandStackDiffDialog>` instead of the opaque
    // fallback. Fire-and-forget — empty rooms (author editing solo)
    // drop the frame silently. The HTTP response remains the
    // authoritative path for the author's own UI.
    state
        .collaboration
        .broadcast_entity_updated(
            id,
            &principal.id,
            &principal.email,
            req.revision,
            updated.revision,
            req.commands.clone(),
        )
        .await;

    Ok((
        StatusCode::OK,
        ApiResponse::of(ApplyOntologyCommandsResponse {
            project: OntologyDraftView::from_ontology_draft(updated),
        }),
    ))
}
