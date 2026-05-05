use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use tracing::warn;
use uuid::Uuid;

use ox_ontology::rebase::{analyze_rebase, RebaseAnalysis};
use ox_ontology::{OntologyDiff, OntologyIR, compute_diff};
use ox_store::{OntologySnapshot, OntologySnapshotSummary};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::routes::ontology_drafts::helpers::{load_mutable_project, reload_project};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts/:id/revisions — list ontology revision history
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontology-drafts/{id}/revisions",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "List of ontology revision snapshots", body = Object),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn list_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<OntologySnapshotSummary>>>, AppError> {
    // Verify project exists
    let _ = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let snapshots = state
        .store
        .list_ontology_snapshots(id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(snapshots))
}

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts/:id/revisions/:rev — get a specific revision snapshot
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontology-drafts/{id}/revisions/{rev}",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev" = i32, Path, description = "Revision number"),
    ),
    responses(
        (status = 200, description = "Ontology revision snapshot", body = Object),
        (status = 404, description = "Revision not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn get_revision(
    State(state): State<AppState>,
    Path((id, rev)): Path<(Uuid, i32)>,
) -> Result<Json<ApiResponse<OntologySnapshot>>, AppError> {
    let snapshot = state
        .store
        .get_ontology_snapshot(id, rev)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    Ok(ApiResponse::of(snapshot))
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/revisions/:rev/restore — restore a previous revision
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct RestoreOntologyDraftRevisionResponse {
    #[schema(value_type = Object)]
    pub project: super::types::ProjectView,
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/revisions/{rev}/restore",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev" = i32, Path, description = "Revision number to restore"),
    ),
    responses(
        (status = 200, description = "Revision restored", body = RestoreOntologyDraftRevisionResponse),
        (status = 404, description = "Project or revision not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn restore_revision(
    State(state): State<AppState>,
    principal: Principal,
    Path((id, rev)): Path<(Uuid, i32)>,
) -> Result<Json<ApiResponse<RestoreOntologyDraftRevisionResponse>>, AppError> {
    principal.require_designer()?;
    let project = load_mutable_project(&state, id).await?;

    // Snapshot current state before restore (best-effort)
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
        warn!(ontology_draft_id = %id, error = %e, "Failed to save ontology snapshot before restore");
    }

    let snapshot = state
        .store
        .get_ontology_snapshot(id, rev)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    state
        .store
        .update_design_result(
            id,
            &snapshot.ontology,
            snapshot.quality_report.as_ref(),
            project.revision,
        )
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    Ok(ApiResponse::of(RestoreOntologyDraftRevisionResponse {
        project: super::types::ProjectView::from_project(updated),
    }))
}

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts/:id/revisions/:rev1/diff/:rev2 — diff between two revisions
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontology-drafts/{id}/revisions/{rev1}/diff/{rev2}",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev1" = i32, Path, description = "Base revision number"),
        ("rev2" = i32, Path, description = "Target revision number"),
    ),
    responses(
        (status = 200, description = "Ontology diff between two revisions", body = Object),
        (status = 404, description = "Revision not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn diff_revisions(
    State(state): State<AppState>,
    Path((id, rev1, rev2)): Path<(Uuid, i32, i32)>,
) -> Result<Json<ApiResponse<OntologyDiff>>, AppError> {
    let snap1 = state
        .store
        .get_ontology_snapshot(id, rev1)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let snap2 = state
        .store
        .get_ontology_snapshot(id, rev2)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let old: OntologyIR = serde_json::from_value(snap1.ontology).map_err(|e| {
        AppError::internal(format!("Failed to parse revision {rev1} ontology: {e}"))
    })?;
    let new: OntologyIR = serde_json::from_value(snap2.ontology).map_err(|e| {
        AppError::internal(format!("Failed to parse revision {rev2} ontology: {e}"))
    })?;

    Ok(ApiResponse::of(compute_diff(&old, &new)))
}

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts/:id/diff/current — diff current ontology vs latest snapshot
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontology-drafts/{id}/diff/current",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Diff between current ontology and latest snapshot", body = Object),
        (status = 400, description = "Project has no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found or no snapshots", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn diff_current(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<OntologyDiff>>, AppError> {
    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let current_ontology_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let current: OntologyIR = serde_json::from_value(current_ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse current ontology: {e}")))?;

    let snapshots = state
        .store
        .list_ontology_snapshots(id)
        .await
        .map_err(AppError::from)?;

    let latest = snapshots
        .first()
        .ok_or_else(|| AppError::not_found("No revision snapshots exist for this project"))?;

    let snapshot = state
        .store
        .get_ontology_snapshot(id, latest.revision)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let old: OntologyIR = serde_json::from_value(snapshot.ontology)
        .map_err(|e| AppError::internal(format!("Failed to parse snapshot ontology: {e}")))?;

    Ok(ApiResponse::of(compute_diff(&old, &current)))
}

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts/:id/diff/canonical — diff vs workspace canonical
//
// Branching diff: how does the draft's in-flight ontology differ
// from the workspace's current canonical head? This is the merge-
// time view operators reach for from the Branches surface ("what
// will land if I commit?"). Distinct from `/diff/current` which
// shows working changes against the draft's own latest snapshot.
//
// Greenfield (no canonical yet) returns the full draft ontology
// as additions — the comparison baseline is "empty" rather than
// 404, so the FE renders the same diff shell either way.
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontology-drafts/{id}/diff/canonical",
    params(("id" = Uuid, Path, description = "Draft ID")),
    responses(
        (status = 200, description = "Diff between draft ontology and workspace canonical head", body = Object),
        (status = 400, description = "Draft has no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Draft not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn diff_canonical(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<OntologyDiff>>, AppError> {
    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let current_ontology_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let current: OntologyIR = serde_json::from_value(current_ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse draft ontology: {e}")))?;

    // Resolve the workspace canonical's current IR. Greenfield
    // workspaces (no canonical) compare against an empty IR so
    // the diff shell renders consistently — every node / edge in
    // `current` lands as an addition, the FE renders the same
    // panel either way.
    let canonical_ir: Option<OntologyIR> = match state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
    {
        Some(identity) => match state
            .store
            .get_current_version(identity.id)
            .await
            .map_err(AppError::from)?
        {
            Some(v) => state
                .store
                .get_ontology_ir(v.id)
                .await
                .map_err(AppError::from)?,
            None => None,
        },
        None => None,
    };
    let baseline = canonical_ir.unwrap_or_else(|| {
        OntologyIR::new(
            String::new(),
            String::new(),
            ox_core::i18n::LocalizedText::default(),
            1u32,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    });

    Ok(ApiResponse::of(compute_diff(&baseline, &current)))
}

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts/:id/rebase/preview — conflict-aware analysis
//
// Computes the rebase preview without mutating anything: how
// has the canonical advanced since the draft's parent? what
// has the draft done? where do they overlap?
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct RebasePreviewResponse {
    /// `true` when the canonical has not advanced (draft already
    /// pinned to head). The FE skips the rebase call in that case.
    pub already_at_head: bool,
    /// Full rebase analysis — `base_to_head`, `base_to_draft`,
    /// `conflicts`. Wire shape mirrors `ox_ontology::rebase::RebaseAnalysis`.
    #[schema(value_type = Object)]
    pub analysis: RebaseAnalysis,
    /// Canonical head id at the time of analysis. The rebase
    /// confirm call must echo this back so a sibling commit
    /// landing between preview and rebase doesn't silently land
    /// on a fresher head than the operator inspected.
    pub head_version_id: Option<Uuid>,
}

#[utoipa::path(
    get,
    path = "/api/ontology-drafts/{id}/rebase/preview",
    params(("id" = Uuid, Path, description = "Draft ID")),
    responses(
        (status = 200, description = "Rebase analysis — conflicts, base→head, base→draft", body = RebasePreviewResponse),
        (status = 400, description = "Draft has no ontology yet", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Draft not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn rebase_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<RebasePreviewResponse>>, AppError> {
    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;
    let draft_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let draft_ir: OntologyIR = serde_json::from_value(draft_json)
        .map_err(|e| AppError::internal(format!("Failed to parse draft ontology: {e}")))?;

    // Resolve canonical head + parent IRs.
    let identity = state.store.get_workspace_ontology().await.map_err(AppError::from)?;
    let head = match identity {
        Some(ref id) => state
            .store
            .get_current_version(id.id)
            .await
            .map_err(AppError::from)?,
        None => None,
    };
    let head_id = head.as_ref().map(|v| v.id);

    let head_ir = match head.as_ref() {
        Some(v) => state
            .store
            .get_ontology_ir(v.id)
            .await
            .map_err(AppError::from)?,
        None => None,
    };
    let parent_ir = match project.parent_version_id {
        Some(parent_id) => state
            .store
            .get_ontology_ir(parent_id)
            .await
            .map_err(AppError::from)?,
        None => None,
    };

    let already_at_head = matches!(
        (project.parent_version_id, head_id),
        (Some(p), Some(h)) if p == h
    );

    // Greenfield baselines fall back to an empty IR so the
    // analysis surface stays consistent.
    let empty = || {
        OntologyIR::new(
            String::new(),
            String::new(),
            ox_core::i18n::LocalizedText::default(),
            1u32,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    };
    let base = parent_ir.unwrap_or_else(empty);
    let head = head_ir.unwrap_or_else(empty);
    let analysis = analyze_rebase(&base, &head, &draft_ir);

    Ok(ApiResponse::of(RebasePreviewResponse {
        already_at_head,
        analysis,
        head_version_id: head_id,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/rebase — fast-forward parent_version_id
//
// Branching rebase. The workspace's canonical head may have
// advanced since the draft was forked (a sibling draft was
// committed first); rebasing the draft means pinning its
// `parent_version_id` to the new head so the eventual commit
// passes the lost-update guard.
//
// The MVP rebase is structural — it moves the parent pointer
// without re-applying canonical changes onto the draft. A
// future commit handles the "your draft conflicts with version
// V's edits" case; today the operator rebases when the diff
// against canonical shows no conflict, and the SHACL gate +
// completion path catch the rest.
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize, utoipa::ToSchema, Default)]
pub struct RebaseProjectRequest {
    /// When `true`, the operator has reviewed the rebase preview
    /// and accepts the listed conflicts. The pin still goes
    /// through; the draft's content stays under operator
    /// control to reconcile by hand. When `false` (default), a
    /// non-empty conflict set causes the endpoint to return
    /// `409` so the FE can route the operator to the preview
    /// surface first.
    #[serde(default)]
    pub acknowledge_conflicts: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RebaseProjectResponse {
    pub project: crate::routes::ontology_drafts::types::ProjectView,
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/rebase",
    params(("id" = Uuid, Path, description = "Draft ID")),
    request_body = RebaseProjectRequest,
    responses(
        (status = 200, description = "Rebased — parent_version_id pinned to canonical head", body = RebaseProjectResponse),
        (status = 400, description = "Workspace has no canonical to rebase onto", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Conflicts present — preview first then resubmit with `acknowledge_conflicts: true`", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn rebase_draft(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Option<Json<RebaseProjectRequest>>,
) -> Result<Json<ApiResponse<RebaseProjectResponse>>, AppError> {
    let body = body.map(|b| b.0).unwrap_or_default();

    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::query_ir_invalid(
                "workspace has no canonical to rebase onto — first-version commits use the project complete path".to_string(),
            )
        })?;

    let head = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::query_ir_invalid(
                "workspace canonical has no committed head — nothing to rebase onto".to_string(),
            )
        })?;

    // Conflict-aware gate. Run the same analysis the preview
    // endpoint emits and refuse the pin when conflicts exist
    // unless the operator explicitly acknowledged.
    if !body.acknowledge_conflicts {
        if let Some(draft_json) = project.ontology.clone() {
            let draft_ir: OntologyIR = serde_json::from_value(draft_json).map_err(|e| {
                AppError::internal(format!("Failed to parse draft ontology: {e}"))
            })?;
            let head_ir = state
                .store
                .get_ontology_ir(head.id)
                .await
                .map_err(AppError::from)?;
            let parent_ir = match project.parent_version_id {
                Some(parent_id) => state
                    .store
                    .get_ontology_ir(parent_id)
                    .await
                    .map_err(AppError::from)?,
                None => None,
            };
            let empty = || {
                OntologyIR::new(
                    String::new(),
                    String::new(),
                    ox_core::i18n::LocalizedText::default(),
                    1u32,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )
            };
            let analysis =
                analyze_rebase(&parent_ir.unwrap_or_else(empty), &head_ir.unwrap_or_else(empty), &draft_ir);
            if !analysis.is_clean() {
                return Err(AppError::conflict(format!(
                    "rebase has {} conflict(s) — preview first then resubmit with `acknowledge_conflicts: true`",
                    analysis.conflicts.len()
                )));
            }
        }
    }

    state
        .store
        .update_draft_parent_version(project.id, head.id)
        .await
        .map_err(AppError::from)?;

    let refreshed = crate::routes::ontology_drafts::helpers::reload_project(&state, project.id)
        .await?;
    let project_view =
        crate::routes::ontology_drafts::types::ProjectView::from_project(refreshed);
    Ok(ApiResponse::of(RebaseProjectResponse {
        project: project_view,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/revisions/:rev/migrate — migrate schema between revisions
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct MigrateProjectSchemaRequest {
    /// If true, return migration plan without executing it
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct MigrateProjectSchemaResponse {
    /// Forward DDL statements
    pub up: Vec<String>,
    /// Rollback DDL statements
    pub down: Vec<String>,
    /// Non-breaking warnings
    pub warnings: Vec<String>,
    /// Breaking changes requiring confirmation
    pub breaking_changes: Vec<String>,
    /// Whether the migration was executed
    pub executed: bool,
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/revisions/{rev}/migrate",
    params(
        ("id" = Uuid, Path, description = "Project ID"),
        ("rev" = i32, Path, description = "Base revision (deployed state) — migration goes FROM this revision TO current ontology"),
    ),
    request_body = MigrateProjectSchemaRequest,
    responses(
        (status = 200, description = "Migration plan or execution result", body = MigrateProjectSchemaResponse),
        (status = 400, description = "No ontology in project or revision", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project or revision not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn migrate_schema(
    State(state): State<AppState>,
    principal: Principal,
    Path((id, rev)): Path<(Uuid, i32)>,
    Json(req): Json<MigrateProjectSchemaRequest>,
) -> Result<Json<ApiResponse<MigrateProjectSchemaResponse>>, AppError> {
    principal.require_designer()?;

    // Load current project ontology
    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let current_ontology_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let current: OntologyIR = serde_json::from_value(current_ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse current ontology: {e}")))?;

    // Load target revision ontology
    let snapshot = state
        .store
        .get_ontology_snapshot(id, rev)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::revision_not_found)?;

    let old: OntologyIR = serde_json::from_value(snapshot.ontology)
        .map_err(|e| AppError::internal(format!("Failed to parse revision {rev} ontology: {e}")))?;

    // Compute diff (old revision → current)
    let diff = compute_diff(&old, &current);

    if diff.is_empty() {
        return Ok(ApiResponse::of(MigrateProjectSchemaResponse {
            up: vec![],
            down: vec![],
            warnings: vec![],
            breaking_changes: vec![],
            executed: false,
        }));
    }

    // Compile migration plan. The migration endpoint was built for Neo4j;
    // a Memgraph-aware variant needs its own review once the revisions
    // flow supports per-project backend routing.
    let plan = ox_compiler::cypher::migration::compile_migration(
        &diff,
        &old,
        &current,
        ox_compiler::cypher::CypherDialect::Neo4j,
    );

    if req.dry_run || !plan.breaking_changes.is_empty() {
        return Ok(ApiResponse::of(MigrateProjectSchemaResponse {
            up: plan.up,
            down: plan.down,
            warnings: plan.warnings,
            breaking_changes: plan.breaking_changes,
            executed: false,
        }));
    }

    // Nothing to execute if up is empty (diff only produced warnings)
    if plan.up.is_empty() {
        return Ok(ApiResponse::of(MigrateProjectSchemaResponse {
            up: plan.up,
            down: plan.down,
            warnings: plan.warnings,
            breaking_changes: plan.breaking_changes,
            executed: false,
        }));
    }

    // Execute forward migration
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    runtime
        .execute_schema(&plan.up)
        .await
        .map_err(AppError::from)?;

    tracing::info!(
        ontology_draft_id = %id,
        from_revision = rev,
        statements = plan.up.len(),
        "Schema migration executed"
    );

    Ok(ApiResponse::of(MigrateProjectSchemaResponse {
        up: plan.up,
        down: plan.down,
        warnings: plan.warnings,
        breaking_changes: plan.breaking_changes,
        executed: true,
    }))
}
