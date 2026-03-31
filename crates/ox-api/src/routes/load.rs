use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use tracing::info;

use ox_core::load_plan::LoadPlan;
use ox_core::ontology_ir::OntologyIR;
use ox_runtime::{LoadBatch, LoadResult};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::validation::validate_ontology_input;

// ---------------------------------------------------------------------------
// POST /api/load — generate a load plan for data ingestion
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoadPlanRequest {
    /// OntologyIR for the target graph schema.
    #[schema(value_type = Object)]
    pub ontology: OntologyIR,
    /// Description of the data source for the LLM.
    pub source_description: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoadPlanResponse {
    /// Generated load plan.
    #[schema(value_type = Object)]
    pub plan: LoadPlan,
    /// Compiled statements in the target language.
    pub compiled_statements: Vec<String>,
    /// Compiler target language (e.g., "cypher").
    pub target: String,
}

#[utoipa::path(
    post,
    path = "/api/load",
    request_body = LoadPlanRequest,
    responses(
        (status = 200, description = "Load plan generated", body = LoadPlanResponse),
        (status = 400, description = "Invalid input", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "Timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Load",
)]
pub async fn plan_load(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<LoadPlanRequest>,
) -> Result<Json<LoadPlanResponse>, AppError> {
    principal.require_designer()?;
    validate_ontology_input(&req.ontology)?;

    if req.source_description.trim().is_empty() {
        return Err(AppError::bad_request(
            "source_description must not be empty",
        ));
    }

    info!("Planning data load");

    let timeout = state.timeouts.design_operation;
    let plan = tokio::time::timeout(
        timeout,
        state
            .brain
            .plan_load(&req.ontology, &req.source_description),
    )
    .await
    .map_err(|_| {
        AppError::timeout(format!(
            "Load plan generation timed out after {}s",
            timeout.as_secs()
        ))
    })?
    .map_err(AppError::from)?;

    let compiled_statements = state.compiler.compile_load(&plan).map_err(|e| {
        tracing::warn!("Load plan compilation failed: {e}");
        AppError::from(e)
    })?;

    Ok(Json(LoadPlanResponse {
        plan,
        compiled_statements,
        target: state.compiler.target_name().to_string(),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/load/execute — compile and execute a load plan against the graph
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoadExecuteRequest {
    /// OntologyIR for the target graph schema.
    #[schema(value_type = Object)]
    pub ontology: OntologyIR,
    /// Description of the data source.
    pub source_description: String,
    /// Optional pre-computed load plan. If omitted, the plan is generated via LLM.
    #[schema(value_type = Option<Object>)]
    pub plan: Option<LoadPlan>,
    /// Data batches to load. Each element is a JSON object representing one record.
    pub data: Vec<serde_json::Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoadExecuteResponse {
    /// Load plan used.
    #[schema(value_type = Object)]
    pub plan: LoadPlan,
    /// Compiled statements executed.
    pub compiled_statements: Vec<String>,
    /// Compiler target language.
    pub target: String,
    /// Execution results.
    #[schema(value_type = Object)]
    pub result: LoadResult,
}

#[utoipa::path(
    post,
    path = "/api/load/execute",
    request_body = LoadExecuteRequest,
    responses(
        (status = 200, description = "Load executed", body = LoadExecuteResponse),
        (status = 400, description = "Invalid input", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "Timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Load",
)]
pub async fn execute_load(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<LoadExecuteRequest>,
) -> Result<Json<LoadExecuteResponse>, AppError> {
    principal.require_designer()?;
    validate_ontology_input(&req.ontology)?;

    let runtime = state
        .runtime
        .as_ref()
        .ok_or_else(AppError::no_runtime)?;

    if req.data.is_empty() {
        return Err(AppError::bad_request("data must not be empty"));
    }

    // Validate all records are JSON objects before doing any LLM work
    let batch = LoadBatch::from_values(req.data).map_err(AppError::from)?;

    // Generate or use provided load plan
    let plan = match req.plan {
        Some(plan) => plan,
        None => {
            if req.source_description.trim().is_empty() {
                return Err(AppError::bad_request(
                    "source_description must not be empty when plan is omitted",
                ));
            }

            info!("Generating load plan for execution");

            let timeout = state.timeouts.design_operation;
            tokio::time::timeout(
                timeout,
                state
                    .brain
                    .plan_load(&req.ontology, &req.source_description),
            )
            .await
            .map_err(|_| {
                AppError::timeout(format!(
                    "Load plan generation timed out after {}s",
                    timeout.as_secs()
                ))
            })?
            .map_err(AppError::from)?
        }
    };

    // Compile
    let compiled_statements = state.compiler.compile_load(&plan).map_err(|e| {
        tracing::warn!("Load plan compilation failed: {e}");
        AppError::from(e)
    })?;

    // Execute each compiled statement with the data batches
    info!(
        statements = compiled_statements.len(),
        records = batch.len(),
        "Executing load against graph runtime"
    );

    let timeout = state.timeouts.design_operation;
    let mut combined_result = LoadResult {
        nodes_created: 0,
        nodes_updated: 0,
        edges_created: 0,
        edges_updated: 0,
        batches_processed: 0,
        batches_failed: 0,
        errors: Vec::new(),
    };

    for statement in &compiled_statements {
        let result =
            tokio::time::timeout(timeout, runtime.execute_load(statement, batch.clone()))
                .await
                .map_err(|_| {
                    AppError::timeout(format!(
                        "Load execution timed out after {}s",
                        timeout.as_secs()
                    ))
                })?
                .map_err(AppError::from)?;

        // Accumulate results
        combined_result.nodes_created += result.nodes_created;
        combined_result.nodes_updated += result.nodes_updated;
        combined_result.edges_created += result.edges_created;
        combined_result.edges_updated += result.edges_updated;
        combined_result.batches_processed += result.batches_processed;
        combined_result.batches_failed += result.batches_failed;
        combined_result.errors.extend(result.errors);
    }

    if !combined_result.errors.is_empty() {
        tracing::warn!(
            error_count = combined_result.errors.len(),
            "Load execution completed with errors"
        );
    }

    info!(
        nodes_created = combined_result.nodes_created,
        edges_created = combined_result.edges_created,
        "Load execution completed"
    );

    Ok(Json(LoadExecuteResponse {
        plan,
        compiled_statements,
        target: state.compiler.target_name().to_string(),
        result: combined_result,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/prompts — list loaded prompt templates and versions
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct PromptInfo {
    pub name: String,
    pub version: String,
}

#[utoipa::path(
    get,
    path = "/api/prompts",
    responses(
        (status = 200, description = "List of loaded prompt templates", body = Vec<PromptInfo>),
    ),
    security(("api_key" = [])),
    tag = "System",
)]
pub async fn list_prompts(State(state): State<AppState>) -> Json<Vec<PromptInfo>> {
    let prompts = state
        .brain
        .list_prompts()
        .into_iter()
        .map(|(name, version)| PromptInfo { name, version })
        .collect();

    Json(prompts)
}
