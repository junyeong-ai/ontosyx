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

/// Query parameters accepted by `GET /api/ontologies`.
///
/// Two shapes, picked by which fields are populated:
/// - Paginated list (`limit`, `cursor`) — the default browse path.
/// - Single-name lookup (`name_eq`) — returns the one ontology
///   (if any) whose workspace-scoped name matches. The lookup
///   predates the Bootstrap wizard's re-entry flow, where Step 6
///   must check whether a pilot name is already taken before
///   calling `seed-glossary`. Exposing the existing
///   `find_ontology_by_name` store method here keeps the FE on a
///   single ontology endpoint rather than growing an endpoint per
///   use case.
///
/// The two modes return the same wire envelope (`items` plus an
/// optional `next_cursor`) so the FE type is unchanged — callers
/// that set `name_eq` see either `items: []` or a single-element
/// `items: [ontology]`, with `next_cursor` always absent.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListOntologiesQuery {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
    /// Exact workspace-scoped name match. Trimmed; whitespace-only
    /// values behave as if unset.
    #[serde(default)]
    pub name_eq: Option<String>,
}

impl ListOntologiesQuery {
    fn pagination(&self) -> CursorParams {
        CursorParams {
            limit: self.limit.unwrap_or(50),
            cursor: self.cursor.clone(),
        }
    }

    /// Normalised single-name lookup — returns `Some` only when
    /// the caller supplied a non-empty, non-whitespace value.
    fn name_eq_trimmed(&self) -> Option<&str> {
        self.name_eq
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

#[utoipa::path(
    get,
    path = "/api/ontologies",
    params(
        ("limit" = Option<u32>, Query, description = "Max items to return (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from a previous response"),
        ("name_eq" = Option<String>, Query, description = "Return only the ontology whose workspace-scoped name matches exactly (0 or 1 items). When set, pagination is ignored."),
    ),
    responses(
        (status = 200, description = "Paginated list of ontology identities", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn list_ontologies(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListOntologiesQuery>,
) -> Result<Json<ApiResponse<Vec<OntologyListItem>>>, AppError> {
    // Single-name lookup mode — short-circuit before any paginated
    // scan. Returns a 0- or 1-element `items` vec with no cursor.
    if let Some(name) = query.name_eq_trimmed() {
        let row = state
            .store
            .find_ontology_by_name(name)
            .await
            .map_err(AppError::from)?;
        let mut items = Vec::new();
        if let Some(row) = row {
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
        return Ok(ApiResponse::page(CursorPage {
            items,
            next_cursor: None,
        }));
    }

    let pagination = query.pagination();
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
        state
            .store
            .get_ontology_ir(v.id)
            .await
            .map_err(AppError::from)?
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

#[cfg(test)]
mod tests {
    use super::*;

    fn q_with_name(name_eq: Option<&str>) -> ListOntologiesQuery {
        ListOntologiesQuery {
            limit: None,
            cursor: None,
            name_eq: name_eq.map(str::to_string),
        }
    }

    #[test]
    fn name_eq_trimmed_returns_none_when_absent() {
        assert_eq!(q_with_name(None).name_eq_trimmed(), None);
    }

    #[test]
    fn name_eq_trimmed_returns_none_when_blank() {
        assert_eq!(q_with_name(Some("")).name_eq_trimmed(), None);
        assert_eq!(q_with_name(Some("   ")).name_eq_trimmed(), None);
    }

    #[test]
    fn name_eq_trimmed_trims_surrounding_but_preserves_inner_whitespace() {
        assert_eq!(
            q_with_name(Some("  Pilot Alpha  ")).name_eq_trimmed(),
            Some("Pilot Alpha"),
        );
    }

    #[test]
    fn pagination_defaults_to_fifty_when_limit_absent() {
        let q = ListOntologiesQuery::default();
        let p = q.pagination();
        assert_eq!(p.limit, 50);
        assert!(p.cursor.is_none());
    }

    #[test]
    fn pagination_respects_explicit_limit_and_cursor() {
        let q = ListOntologiesQuery {
            limit: Some(20),
            cursor: Some("abc".to_string()),
            name_eq: None,
        };
        let p = q.pagination();
        assert_eq!(p.limit, 20);
        assert_eq!(p.cursor.as_deref(), Some("abc"));
    }
}
