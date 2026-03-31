use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use ox_core::design_project::{
    DesignProjectStatus, SourceConfig, SourceHistoryEntry, SourceTypeKind,
};
use ox_core::quality::OntologyQualityReport;
use ox_core::source_analysis::DesignOptions;
use ox_store::store::CursorParams;
use ox_store::{DesignProject, DesignProjectSummary, SavedOntology};

use ox_source::fetcher::DataSourceFetcher;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

use super::helpers::{
    analyze_code_repository, analyze_source, load_project_in_status, reload_project,
    run_repo_enrichment, skipped_repo_summary,
};
use super::types::{ProjectCompleteRequest, CreateProjectRequest, ProjectOrigin, ProjectSource};

// ---------------------------------------------------------------------------
// POST /api/projects — create + analyze (or from existing ontology)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "Project created", body = Object),
        (status = 400, description = "Invalid input", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Base ontology not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn create_project(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<DesignProject>), AppError> {
    principal.require_designer()?;
    let audit_user_id = principal.user_uuid().ok();
    let now = Utc::now();

    let project = match req.origin {
        ProjectOrigin::BaseOntology { base_ontology_id } => {
            // --- From existing ontology ---
            let saved = state
                .store
                .get_saved_ontology(base_ontology_id)
                .await
                .map_err(AppError::from)?
                .ok_or_else(AppError::ontology_not_found)?;

            let source_config = SourceConfig {
                source_type: SourceTypeKind::Ontology,
                schema_name: None,
                source_fingerprint: None,
            };

            let history_entry = SourceHistoryEntry {
                source_type: SourceTypeKind::Ontology,
                added_at: now,
                schema_name: None,
                url: None,
                fingerprint: None,
            };

            DesignProject {
                id: Uuid::new_v4(),
                user_id: principal.id,
                status: DesignProjectStatus::Designed.to_string(),
                revision: 1,
                title: req.title,
                source_config: AppError::to_json(&source_config)?,
                source_data: None,
                source_schema: None,
                source_profile: None,
                analysis_report: None,
                design_options: AppError::to_json(&DesignOptions::default())?,
                source_mapping: None,
                ontology: Some(saved.ontology_ir),
                quality_report: None,
                saved_ontology_id: None,
                source_history: AppError::to_json(&vec![history_entry])?,
                created_at: now,
                updated_at: now,
                analyzed_at: None,
            }
        }
        ProjectOrigin::Source {
            source,
            repo_source,
        } => {
            // --- From data source ---

            // CodeRepository requires LLM-based analysis — handle separately
            if let ProjectSource::CodeRepository { ref url } = source {
                let (source_config, source_schema, source_profile, report) =
                    analyze_code_repository(&state, url).await?;

                let history_entry = SourceHistoryEntry {
                    source_type: SourceTypeKind::CodeRepository,
                    added_at: now,
                    schema_name: None,
                    url: Some(url.clone()),
                    fingerprint: source_config.source_fingerprint.clone(),
                };

                let project = DesignProject {
                    id: Uuid::new_v4(),
                    user_id: principal.id,
                    status: DesignProjectStatus::Analyzed.to_string(),
                    revision: 1,
                    title: req.title,
                    source_config: AppError::to_json(&source_config)?,
                    source_data: None,
                    source_schema: Some(AppError::to_json(&source_schema)?),
                    source_profile: Some(AppError::to_json(&source_profile)?),
                    analysis_report: Some(AppError::to_json(&report)?),
                    design_options: AppError::to_json(&DesignOptions::default())?,
                    source_mapping: None,
                    ontology: None,
                    quality_report: None,
                    saved_ontology_id: None,
                    source_history: AppError::to_json(&vec![history_entry])?,
                    created_at: now,
                    updated_at: now,
                    analyzed_at: Some(now),
                };

                state
                    .store
                    .create_design_project(&project)
                    .await
                    .map_err(AppError::from)?;

                info!(
                    project_id = %project.id,
                    source_type = "code_repository",
                    "Design project created from code repository"
                );

                // Fire-and-forget audit
                {
                    let audit_store = Arc::clone(&state.store);
                    let audit_project_id = project.id.to_string();
                    crate::spawn_scoped::spawn_scoped(async move {
                        let _ = audit_store.record_audit(
                            audit_user_id,
                            "project.create",
                            "project",
                            Some(&audit_project_id),
                            serde_json::json!({"source_type": "code_repository"}),
                        ).await;
                    });
                }

                return Ok((StatusCode::CREATED, Json(project)));
            }

            let (source_config, source_data, source_schema, source_profile, analysis_report) =
                analyze_source(source, &state.introspector_registry).await?;

            let analyzed_at = if source_schema.is_some() {
                Some(now)
            } else {
                None
            };

            let mut report = analysis_report;

            // Optional repo enrichment (non-fatal — failures recorded in repo_summary)
            if let (Some(source), Some(rpt)) = (&repo_source, &mut report) {
                match source.validate(
                    &state.repo_policy.allowed_roots,
                    &state.repo_policy.allowed_git_hosts,
                ) {
                    Ok(validated) => run_repo_enrichment(&state, &validated, rpt).await,
                    Err(reason) => {
                        let reason = reason.to_string();
                        warn!(reason, "Repo enrichment skipped");
                        rpt.repo_summary = Some(skipped_repo_summary(reason));
                    }
                }
            }

            let history_entry = SourceHistoryEntry {
                source_type: source_config.source_type.clone(),
                added_at: now,
                schema_name: source_config.schema_name.clone(),
                url: None,
                fingerprint: source_config.source_fingerprint.clone(),
            };

            DesignProject {
                id: Uuid::new_v4(),
                user_id: principal.id,
                status: DesignProjectStatus::Analyzed.to_string(),
                revision: 1,
                title: req.title,
                source_config: AppError::to_json(&source_config)?,
                source_data,
                source_schema: source_schema.as_ref().map(AppError::to_json).transpose()?,
                source_profile: source_profile.as_ref().map(AppError::to_json).transpose()?,
                analysis_report: report.as_ref().map(AppError::to_json).transpose()?,
                design_options: AppError::to_json(&DesignOptions::default())?,
                source_mapping: None,
                ontology: None,
                quality_report: None,
                saved_ontology_id: None,
                source_history: AppError::to_json(&vec![history_entry])?,
                created_at: now,
                updated_at: now,
                analyzed_at,
            }
        }
    };

    let source_type = serde_json::from_value::<SourceConfig>(project.source_config.clone())
        .map(|c| c.source_type.to_string())
        .unwrap_or_default();

    state
        .store
        .create_design_project(&project)
        .await
        .map_err(AppError::from)?;

    info!(
        project_id = %project.id,
        source_type = %source_type,
        "Design project created"
    );

    // Fire-and-forget audit
    {
        let audit_store = Arc::clone(&state.store);
        let audit_project_id = project.id.to_string();
        let audit_source_type = source_type.clone();
        crate::spawn_scoped::spawn_scoped(async move {
            let _ = audit_store.record_audit(
                audit_user_id,
                "project.create",
                "project",
                Some(&audit_project_id),
                serde_json::json!({"source_type": audit_source_type}),
            ).await;
        });
    }

    Ok((StatusCode::CREATED, Json(project)))
}

// ---------------------------------------------------------------------------
// GET /api/projects
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/projects",
    params(
        ("limit" = Option<u32>, Query, description = "Max items to return (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from a previous response"),
    ),
    responses(
        (status = 200, description = "Paginated project list", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn list_projects(
    State(state): State<AppState>,
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<ox_store::store::CursorPage<DesignProjectSummary>>, AppError> {
    let page = state
        .store
        .list_design_projects(&pagination)
        .await
        .map_err(AppError::from)?;
    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// GET /api/projects/:id
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/projects/{id}",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Project details", body = Object),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DesignProject>, AppError> {
    let project = reload_project(&state, id).await?;
    Ok(Json(project))
}

// ---------------------------------------------------------------------------
// DELETE /api/projects/:id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/projects/{id}",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 204, description = "Project deleted"),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn delete_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;

    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    principal.require_project_owner(&project.user_id)?;

    let deleted = state
        .store
        .delete_design_project(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        // Fire-and-forget audit
        {
            let audit_store = Arc::clone(&state.store);
            let audit_user_id = principal.user_uuid().ok();
            let audit_project_id = id.to_string();
            crate::spawn_scoped::spawn_scoped(async move {
                let _ = audit_store.record_audit(
                    audit_user_id,
                    "project.delete",
                    "project",
                    Some(&audit_project_id),
                    serde_json::json!({}),
                ).await;
            });
        }

        // Fire-and-forget: clean up orphaned memory entries for the deleted project's ontology.
        if let Some(ref memory) = state.memory {
            if let Some(ontology_id) = project.saved_ontology_id {
                let mem = Arc::clone(memory);
                let oid = ontology_id.to_string();
                crate::spawn_scoped::spawn_scoped(async move {
                    match mem.cleanup_by_ontology(&oid).await {
                        Ok(n) if n > 0 => {
                            info!(count = n, ontology_id = %oid, "Cleaned orphaned memory entries")
                        }
                        Err(e) => warn!(error = %e, "Memory cleanup failed"),
                        _ => {}
                    }
                });
            }
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::project_not_found())
    }
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/complete
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/projects/{id}/complete",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectCompleteRequest,
    responses(
        (status = 200, description = "Project completed, ontology saved", body = Object),
        (status = 400, description = "Project has no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Quality gate failed", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn complete_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectCompleteRequest>,
) -> Result<Json<DesignProject>, AppError> {
    principal.require_designer()?;
    let project = load_project_in_status(&state, id, DesignProjectStatus::Designed).await?;

    // Quality gate: reject completion unless confidence is high or user explicitly acknowledges
    if !req.acknowledge_quality_risks
        && let Some(qr) = &project.quality_report
        && let Ok(report) = serde_json::from_value::<OntologyQualityReport>(qr.clone())
        && !matches!(report.confidence, ox_core::quality::QualityConfidence::High)
    {
        return Err(AppError::quality_gate(format!(
            "Quality confidence is '{}'. Resolve gaps via refine, \
             or set acknowledge_quality_risks=true to proceed.",
            match report.confidence {
                ox_core::quality::QualityConfidence::Low => "low",
                ox_core::quality::QualityConfidence::Medium => "medium",
                ox_core::quality::QualityConfidence::High => "high",
            }
        )));
    }

    let ontology_ir = project
        .ontology
        .as_ref()
        .ok_or_else(AppError::no_ontology)?
        .clone();

    // Determine next version
    let latest = state
        .store
        .get_latest_ontology(&req.name)
        .await
        .map_err(AppError::from)?;
    let next_version = latest.map_or(1, |o| o.version + 1);

    let saved = SavedOntology {
        id: Uuid::new_v4(),
        name: req.name,
        description: req.description,
        version: next_version,
        ontology_ir,
        created_by: principal.id.clone(),
        created_at: Utc::now(),
    };

    state
        .store
        .complete_design_project(id, &saved, req.revision)
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    info!(project_id = %id, ontology_id = %saved.id, "Design project completed");

    // Schema RAG indexing: embed ontology nodes for vector search in query translation.
    // Non-blocking — indexing failure doesn't affect project completion.
    if let Some(memory) = &state.memory {
        let memory = Arc::clone(memory);
        let ontology_id = saved.id.to_string();
        let ontology: ox_core::OntologyIR = serde_json::from_value(saved.ontology_ir.clone())
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Failed to deserialize ontology for schema indexing");
                ox_core::OntologyIR::new(String::new(), String::new(), None, 0, vec![], vec![], vec![])
            });
        crate::spawn_scoped::spawn_scoped(async move {
            ox_brain::schema_rag::index_ontology_schema(&memory, &ontology, &ontology_id).await;
        });
    }

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/deploy-schema — deploy ontology schema to graph DB
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectDeployRequest {
    /// If true, return DDL statements without executing them
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectDeployResponse {
    /// Generated DDL statements
    pub statements: Vec<String>,
    /// Whether the statements were actually executed
    pub executed: bool,
}

#[utoipa::path(
    post,
    path = "/api/projects/{id}/deploy-schema",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectDeployRequest,
    responses(
        (status = 200, description = "Schema deployed or DDL preview returned", body = ProjectDeployResponse),
        (status = 400, description = "Project has no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn deploy_schema(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectDeployRequest>,
) -> Result<Json<ProjectDeployResponse>, AppError> {
    principal.require_designer()?;

    // Check if workspace has pending approval blocking this deployment
    let pending = state
        .store
        .list_pending_approvals(ws.workspace_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to check approvals: {e}")))?;

    let blocked_by_approval = pending.iter().any(|a| {
        a.resource_type == "project"
            && a.resource_id == id.to_string()
            && a.action_type == "deploy_schema"
            && a.status == "pending"
    });

    if blocked_by_approval {
        return Err(AppError::conflict(
            "Schema deployment is pending approval. Wait for admin review.",
        ));
    }

    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let ontology_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let ontology: ox_core::ontology_ir::OntologyIR = serde_json::from_value(ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse ontology: {e}")))?;

    let statements = state
        .compiler
        .compile_schema(&ontology)
        .map_err(AppError::from)?;

    if req.dry_run {
        return Ok(Json(ProjectDeployResponse {
            statements,
            executed: false,
        }));
    }

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    runtime
        .execute_schema(&statements)
        .await
        .map_err(AppError::from)?;

    info!(
        project_id = %id,
        statements = statements.len(),
        "Schema deployed to graph database"
    );

    // Fire-and-forget audit
    {
        let audit_store = Arc::clone(&state.store);
        let audit_user_id = principal.user_uuid().ok();
        let audit_project_id = id.to_string();
        let stmt_count = statements.len();
        crate::spawn_scoped::spawn_scoped(async move {
            let _ = audit_store.record_audit(
                audit_user_id,
                "schema.deploy",
                "project",
                Some(&audit_project_id),
                serde_json::json!({"statements_count": stmt_count}),
            ).await;
        });
    }

    Ok(Json(ProjectDeployResponse {
        statements,
        executed: true,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/load-plan — generate a LoadPlan for the project
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectLoadPlanResponse {
    /// The generated load plan
    #[schema(value_type = Object)]
    pub plan: ox_core::load_plan::LoadPlan,
}

#[utoipa::path(
    post,
    path = "/api/projects/{id}/load-plan",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Load plan generated", body = ProjectLoadPlanResponse),
        (status = 400, description = "Project has no ontology or source mapping", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn generate_load_plan(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ProjectLoadPlanResponse>, AppError> {
    principal.require_designer()?;

    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let ontology_json = project.ontology.as_ref().ok_or_else(AppError::no_ontology)?;
    let ontology: ox_core::ontology_ir::OntologyIR = serde_json::from_value(ontology_json.clone())
        .map_err(|e| AppError::internal(format!("Failed to parse ontology: {e}")))?;

    let source_mapping_json = project.source_mapping.as_ref().ok_or_else(|| {
        AppError::bad_request("Project has no source mapping — design the ontology first")
    })?;
    let source_mapping: ox_core::SourceMapping =
        serde_json::from_value(source_mapping_json.clone())
            .map_err(|e| AppError::internal(format!("Failed to parse source mapping: {e}")))?;

    let source_schema_json = project.source_schema.as_ref().ok_or_else(|| {
        AppError::bad_request("Project has no source schema")
    })?;
    let source_schema: ox_core::SourceSchema =
        serde_json::from_value(source_schema_json.clone())
            .map_err(|e| AppError::internal(format!("Failed to parse source schema: {e}")))?;

    let plan = state
        .brain
        .generate_load_plan(&ontology, &source_mapping, &source_schema)
        .await
        .map_err(AppError::from)?;

    info!(
        project_id = %id,
        steps = plan.steps.len(),
        "Load plan generated"
    );

    Ok(Json(ProjectLoadPlanResponse { plan }))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/load/compile — compile a LoadPlan into target DDL
//
// Returns the compiled Cypher statements for preview. The statements contain
// $batch parameter placeholders — actual execution requires the source data
// pipeline (IntrospectorRegistry → fetch → batch → execute_load) which is
// not yet connected.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectLoadCompileRequest {
    /// The load plan to compile
    #[schema(value_type = Object)]
    pub plan: ox_core::load_plan::LoadPlan,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectLoadCompileResponse {
    /// Compiled load statements (parameterized — $batch must be bound at execution time)
    pub statements: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/projects/{id}/load/compile",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectLoadCompileRequest,
    responses(
        (status = 200, description = "Compiled load statements", body = ProjectLoadCompileResponse),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn compile_load(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectLoadCompileRequest>,
) -> Result<Json<ProjectLoadCompileResponse>, AppError> {
    principal.require_designer()?;

    // Verify project exists
    let _ = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    let statements = state
        .compiler
        .compile_load(&req.plan)
        .map_err(AppError::from)?;

    info!(
        project_id = %id,
        statements = statements.len(),
        "Load plan compiled"
    );

    Ok(Json(ProjectLoadCompileResponse { statements }))
}

// ---------------------------------------------------------------------------
// POST /api/projects/:id/load/execute — fetch from source + load into graph
//
// Completes the E2E pipeline: source → fetch → compile → execute_load.
// Requires the source connection string (not stored for security).
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectLoadExecuteRequest {
    /// Pre-computed load plan (from generate_load_plan or manual)
    #[schema(value_type = Object)]
    pub plan: ox_core::load_plan::LoadPlan,
    /// Source database connection string (required for fetching data)
    pub connection_string: String,
    /// Batch size for fetching rows (default: 1000)
    #[serde(default = "default_batch_size")]
    pub batch_size: u64,
}

fn default_batch_size() -> u64 {
    1000
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectLoadExecuteResponse {
    /// Total rows fetched from source
    pub rows_fetched: u64,
    /// Load execution result
    #[schema(value_type = Object)]
    pub result: ox_runtime::LoadResult,
    /// Number of load steps executed
    pub steps_executed: usize,
}

#[utoipa::path(
    post,
    path = "/api/projects/{id}/load/execute",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = ProjectLoadExecuteRequest,
    responses(
        (status = 200, description = "Data loaded from source into graph", body = ProjectLoadExecuteResponse),
        (status = 400, description = "Missing ontology or source mapping", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph runtime not connected", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn execute_load_from_source(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<ProjectLoadExecuteRequest>,
) -> Result<Json<ProjectLoadExecuteResponse>, AppError> {
    principal.require_designer()?;

    let runtime = state
        .runtime
        .as_ref()
        .ok_or_else(AppError::no_runtime)?;

    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    // Extract source mapping to know which tables map to which nodes
    let source_mapping_json = project.source_mapping.as_ref().ok_or_else(|| {
        AppError::bad_request("Project has no source mapping — design the ontology first")
    })?;
    let source_mapping: ox_core::SourceMapping =
        serde_json::from_value(source_mapping_json.clone())
            .map_err(|e| AppError::internal(format!("Failed to parse source mapping: {e}")))?;

    // Determine schema name from project source config
    let source_config: ox_core::design_project::SourceConfig =
        serde_json::from_value(project.source_config.clone())
            .map_err(|e| AppError::internal(format!("Failed to parse source config: {e}")))?;
    let schema_name = source_config.schema_name.as_deref().unwrap_or("public");

    // Connect to source database
    let fetcher =
        ox_source::postgres_fetcher::PostgresFetcher::connect(&req.connection_string, schema_name)
            .await
            .map_err(|e| AppError::bad_request(format!("Failed to connect to source: {e}")))?;

    // Compile load plan to Cypher statements
    let compiled_statements = state
        .compiler
        .compile_load(&req.plan)
        .map_err(AppError::from)?;

    info!(
        project_id = %id,
        steps = req.plan.steps.len(),
        compiled = compiled_statements.len(),
        "Starting load execution from source"
    );

    // Create lineage entry before load execution
    let lineage_id = Uuid::new_v4();
    let source_type_str = source_config.source_type.to_string();
    let lineage_entry = ox_store::LineageEntry {
        id: lineage_id,
        workspace_id: ws.workspace_id,
        project_id: Some(id),
        graph_label: "batch_load".to_string(),
        graph_element_type: "node".to_string(),
        source_type: source_type_str,
        source_name: schema_name.to_string(),
        source_table: None,
        source_columns: None,
        load_plan_hash: None,
        record_count: 0,
        loaded_by: principal.user_uuid().ok(),
        started_at: Utc::now(),
        completed_at: None,
        status: "running".to_string(),
        error_message: None,
    };
    let _ = state.store.create_lineage_entry(&lineage_entry).await;

    let mut total_rows_fetched: u64 = 0;
    let mut combined_result = ox_runtime::LoadResult {
        nodes_created: 0,
        nodes_updated: 0,
        edges_created: 0,
        edges_updated: 0,
        batches_processed: 0,
        batches_failed: 0,
        errors: Vec::new(),
    };

    // Execute each load step: fetch from source table → execute against graph
    for (step_idx, (step, cypher)) in req.plan.steps.iter().zip(&compiled_statements).enumerate() {
        // Determine source table from the load operation
        let source_table = resolve_source_table(&step.operation, &source_mapping);
        let source_table = match source_table {
            Some(t) => t,
            None => {
                warn!(step = step_idx, "Could not resolve source table for step — skipping");
                continue;
            }
        };

        // Determine which columns to fetch based on the operation's property mappings
        let columns = extract_source_columns(&step.operation);

        // Fetch and load in batches
        let row_count = fetcher.count_rows(&source_table).await.map_err(|e| {
            AppError::internal(format!("Failed to count rows in {source_table}: {e}"))
        })?;

        info!(
            step = step_idx,
            table = %source_table,
            rows = row_count,
            "Fetching data for load step"
        );

        let mut offset = 0u64;
        while offset < row_count {
            let rows = fetcher
                .fetch_batch(&source_table, &columns, offset, req.batch_size)
                .await
                .map_err(|e| {
                    AppError::internal(format!("Failed to fetch batch from {source_table}: {e}"))
                })?;

            if rows.is_empty() {
                break;
            }

            let batch_len = rows.len();
            total_rows_fetched += batch_len as u64;

            // Convert to LoadBatch
            let values: Vec<serde_json::Value> = rows
                .into_iter()
                .map(serde_json::Value::Object)
                .collect();
            let batch = ox_runtime::LoadBatch::from_values(values).map_err(AppError::from)?;

            // Execute against graph
            let result = runtime
                .execute_load(cypher, batch)
                .await
                .map_err(AppError::from)?;

            combined_result.nodes_created += result.nodes_created;
            combined_result.nodes_updated += result.nodes_updated;
            combined_result.edges_created += result.edges_created;
            combined_result.edges_updated += result.edges_updated;
            combined_result.batches_processed += result.batches_processed;
            combined_result.batches_failed += result.batches_failed;
            combined_result.errors.extend(result.errors);

            offset += batch_len as u64;
        }
    }

    // Complete lineage entry after load
    let lineage_status = if combined_result.batches_failed > 0 { "partial" } else { "completed" };
    let lineage_error = if combined_result.errors.is_empty() {
        None
    } else {
        Some(combined_result.errors.iter().take(3).map(|e| e.message.as_str()).collect::<Vec<_>>().join("; "))
    };
    let _ = state.store.complete_lineage_entry(
        lineage_id,
        total_rows_fetched as i64,
        lineage_status,
        lineage_error.as_deref(),
    ).await;

    info!(
        project_id = %id,
        rows_fetched = total_rows_fetched,
        nodes_created = combined_result.nodes_created,
        edges_created = combined_result.edges_created,
        "Load execution completed"
    );

    // Record metering (fire-and-forget)
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        let steps = req.plan.steps.len();
        let nodes = combined_result.nodes_created;
        let edges = combined_result.edges_created;
        let rows = total_rows_fetched;
        crate::spawn_scoped::spawn_scoped(async move {
            let _ = meter_store.record_usage(
                meter_user,
                "data_load",
                None,
                None,
                Some("load_from_source"),
                0,
                0,
                0,  // duration not tracked for load
                0.0,
                serde_json::json!({
                    "rows_fetched": rows,
                    "nodes_created": nodes,
                    "edges_created": edges,
                    "steps": steps,
                }),
            ).await;
        });
    }

    Ok(Json(ProjectLoadExecuteResponse {
        rows_fetched: total_rows_fetched,
        result: combined_result,
        steps_executed: req.plan.steps.len(),
    }))
}

/// Resolve which source table to fetch from, given a load operation and source mapping.
fn resolve_source_table(
    op: &ox_core::load_plan::LoadOp,
    source_mapping: &ox_core::SourceMapping,
) -> Option<String> {
    use ox_core::load_plan::LoadOp;

    match op {
        LoadOp::UpsertNode { target_label, .. } => {
            // Look up by label in node_tables (values are source table names)
            source_mapping
                .node_tables
                .values()
                .find(|_| true) // If there's only one match, use it
                .cloned()
                .or_else(|| {
                    // Try matching by label name (case-insensitive pluralized heuristic)
                    let lower = target_label.to_lowercase();
                    source_mapping
                        .node_tables
                        .values()
                        .find(|t| {
                            let tl = t.to_lowercase();
                            tl == lower || tl == format!("{lower}s") || tl.ends_with(&format!("_{lower}"))
                        })
                        .cloned()
                })
        }
        LoadOp::UpsertEdge {
            source_match,
            ..
        } => {
            // Edges typically come from one of the node tables or a junction table.
            // Match on the source node's label to find the originating table.
            source_mapping
                .node_tables
                .iter()
                .find(|(_, table)| {
                    let tl = table.to_lowercase();
                    tl.contains(&source_match.label.to_lowercase())
                })
                .map(|(_, t)| t.clone())
        }
    }
}

/// Extract source column names from a load operation's property mappings.
fn extract_source_columns(op: &ox_core::load_plan::LoadOp) -> Vec<String> {
    use ox_core::load_plan::LoadOp;

    match op {
        LoadOp::UpsertNode {
            match_fields,
            set_fields,
            ..
        } => {
            let mut cols: Vec<String> = match_fields
                .iter()
                .chain(set_fields.iter())
                .map(|m| m.source_column.clone())
                .collect();
            cols.sort();
            cols.dedup();
            cols
        }
        LoadOp::UpsertEdge {
            source_match,
            target_match,
            set_fields,
            ..
        } => {
            let mut cols: Vec<String> = Vec::new();
            cols.push(source_match.source_field.clone());
            cols.push(target_match.source_field.clone());
            cols.extend(set_fields.iter().map(|m| m.source_column.clone()));
            cols.sort();
            cols.dedup();
            cols
        }
    }
}
