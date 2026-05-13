use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use ox_ontology::mapping::SourceId;
use ox_ontology::ontology_draft::{
    OntologyDraftStatus, SourceConfig, SourceHistoryEntry, SourceTypeKind,
};
use ox_ontology::quality::OntologyQualityReport;
use ox_ontology::source_analysis::DesignOptions;
use ox_store::store::CursorParams;
use ox_store::{OntologyDraft, OntologyDraftSummary};

use ox_source::fetcher::DataSourceFetcher;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

use super::helpers::{
    analyze_code_repository, analyze_source, load_ontology_draft_in_status, reload_ontology_draft,
    run_repo_enrichment, skipped_repo_summary,
};
use super::types::{
    CompleteOntologyDraftRequest, CreateOntologyDraftRequest, DataSourceSpec, OntologyDraftOrigin,
    OntologyDraftView,
};

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts — create + analyze (or from existing ontology)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology-drafts",
    request_body = CreateOntologyDraftRequest,
    responses(
        (status = 201, description = "Ontology draft created", body = OntologyDraftView),
        (status = 400, description = "Invalid input", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Base ontology not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn create_ontology_draft(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<CreateOntologyDraftRequest>,
) -> Result<(StatusCode, Json<ApiResponse<OntologyDraftView>>), AppError> {
    principal.require_designer()?;
    if let OntologyDraftOrigin::Source { selection, .. } = &req.origin {
        selection.validate().map_err(AppError::from)?;
    }
    let audit_user_id = principal.user_uuid().ok();
    let now = Utc::now();

    // Capture the canonical's current head as the project's parent
    // branching point. `None` for greenfield workspaces (no canonical
    // yet); `Some(version_id)` when the workspace already has a
    // committed ontology, so `complete_ontology_draft` can detect
    // intervening canonical writes via the lost-update guard.
    let workspace_parent_version = match state.store.get_workspace_ontology().await {
        Ok(Some(canonical)) => state
            .store
            .find_current_version(canonical.id)
            .await
            .ok()
            .flatten()
            .map(|v| v.id),
        _ => None,
    };

    let CreateOntologyDraftRequest { title, origin } = req;

    let project = match origin {
        OntologyDraftOrigin::BaseOntology => {
            // --- From workspace's canonical ontology ---
            // Resolve identity → current version → hydrate IR. The new
            // project carries the IR JSON so downstream design edits
            // operate on a local copy.
            let identity = state
                .store
                .get_workspace_ontology()
                .await
                .map_err(AppError::from)?
                .ok_or_else(AppError::ontology_not_found)?;
            let version = state
                .store
                .find_current_version(identity.id)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| AppError::ontology_not_committed(identity.lineage_id.clone()))?;
            let ir = state
                .store
                .get_ontology_ir(version.id)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| AppError::not_found("Ontology version"))?;
            let ontology_json = AppError::to_json(&ir)?;

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

            OntologyDraft {
                id: Uuid::new_v4(),
                user_id: principal.id,
                status: OntologyDraftStatus::Designed.to_string(),
                revision: 1,
                title: title.clone(),
                source_id: SourceId::from_source_config(&source_config).to_string(),
                source_config: AppError::to_json(&source_config)?,
                source_data: None,
                source_schema: None,
                source_profile: None,
                analysis_report: None,
                design_options: AppError::to_json(&DesignOptions::default())?,
                analysis_scope: AppError::to_json(&ox_source::AnalysisScope::default())?,
                ontology: Some(ontology_json),
                quality_report: None,
                // Ontology draft is branched off the canonical version we
                // just hydrated. complete_ontology_draft compares this
                // against the head at commit time to detect
                // intervening canonical writes.
                parent_version_id: Some(version.id),
                committed_version_id: None,
                source_history: AppError::to_json(&vec![history_entry])?,
                created_at: now,
                updated_at: now,
                analyzed_at: None,
            }
        }
        OntologyDraftOrigin::Source {
            source,
            repo_source,
            selection,
        } => {
            // --- From data source ---

            // CodeRepository requires LLM-based analysis — handle separately
            if let DataSourceSpec::CodeRepository { ref url } = source {
                let (source_config, source_schema, source_profile, report) =
                    analyze_code_repository(&state, url).await?;

                let history_entry = SourceHistoryEntry {
                    source_type: SourceTypeKind::CodeRepository,
                    added_at: now,
                    schema_name: None,
                    url: Some(url.clone()),
                    fingerprint: source_config.source_fingerprint.clone(),
                };

                let project = OntologyDraft {
                    id: Uuid::new_v4(),
                    user_id: principal.id,
                    status: OntologyDraftStatus::Analyzed.to_string(),
                    revision: 1,
                    title: title.clone(),
                    source_id: SourceId::from_source_config(&source_config).to_string(),
                    source_config: AppError::to_json(&source_config)?,
                    source_data: None,
                    source_schema: Some(AppError::to_json(&source_schema)?),
                    source_profile: Some(AppError::to_json(&source_profile)?),
                    analysis_report: Some(AppError::to_json(&report)?),
                    design_options: AppError::to_json(&DesignOptions::default())?,
                    // Code-repository projects don't carry a tabular
                    // schema — the scope's `included` / `deferred` /
                    // `fingerprints` machinery doesn't apply. Empty
                    // scope keeps the wire shape uniform across project
                    // origins.
                    analysis_scope: AppError::to_json(&ox_source::AnalysisScope::default())?,
                    ontology: None,
                    quality_report: None,
                    parent_version_id: workspace_parent_version,
                    committed_version_id: None,
                    source_history: AppError::to_json(&vec![history_entry])?,
                    created_at: now,
                    updated_at: now,
                    analyzed_at: Some(now),
                };

                state
                    .store
                    .create_ontology_draft(&project)
                    .await
                    .map_err(AppError::from)?;

                info!(
                    ontology_draft_id = %project.id,
                    source_type = "code_repository",
                    "Design project created from code repository"
                );

                // Fire-and-forget audit
                {
                    let audit_store = Arc::clone(&state.store);
                    let audit_project_id = project.id.to_string();
                    ox_context::spawn_scoped(async move {
                        if let Err(error) = audit_store
                            .record_audit(
                                audit_user_id,
                                "ontology_draft.create",
                                "ontology_draft",
                                Some(&audit_project_id),
                                serde_json::json!({"source_type": "code_repository"}),
                            )
                            .await
                        {
                            tracing::warn!(?error, "telemetry record failed");
                        }
                    });
                }

                return Ok((
                    StatusCode::CREATED,
                    ApiResponse::of(OntologyDraftView::from_ontology_draft(project)),
                ));
            }

            let analyzed =
                analyze_source(source, &state.adapter_registry, selection.clone(), None).await?;
            super::helpers::capture_source_contracts(&state, &analyzed).await?;
            let analyzed_at = analyzed.schema.as_ref().map(|_| now);
            let source_config = analyzed.config;
            let source_data = analyzed.raw_data;
            let source_schema = analyzed.schema;
            let source_profile = analyzed.profile;
            let mut report = analyzed.report;

            // Optional repo enrichment (non-fatal — failures recorded in repo_summary)
            if let (Some(source), Some(rpt)) = (&repo_source, &mut report) {
                match source.validate(
                    &state.repo_policy.allowed_roots,
                    &state.repo_policy.allowed_git_hosts,
                ) {
                    Ok(validated) => run_repo_enrichment(&state, &validated, rpt).await,
                    Err(reason) => {
                        warn!(reason = %reason, "Repo enrichment skipped");
                        rpt.repo_summary = Some(skipped_repo_summary(
                            ox_ontology::source_analysis::RepoFailureKind::PolicyRejected,
                        ));
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

            OntologyDraft {
                id: Uuid::new_v4(),
                user_id: principal.id,
                status: OntologyDraftStatus::Analyzed.to_string(),
                revision: 1,
                title: title.clone(),
                source_id: SourceId::from_source_config(&source_config).to_string(),
                source_config: AppError::to_json(&source_config)?,
                source_data,
                source_schema: source_schema.as_ref().map(AppError::to_json).transpose()?,
                source_profile: source_profile.as_ref().map(AppError::to_json).transpose()?,
                analysis_report: report.as_ref().map(AppError::to_json).transpose()?,
                design_options: AppError::to_json(&DesignOptions::default())?,
                analysis_scope: AppError::to_json(&{
                    let mut scope = ox_source::AnalysisScope::default();
                    let all_tables: std::collections::BTreeSet<String> = source_schema
                        .as_ref()
                        .map(|s| s.tables.iter().map(|t| t.name.clone()).collect())
                        .unwrap_or_default();
                    scope.record_selection(&selection, &all_tables, now);
                    if let Some(schema) = source_schema.as_ref() {
                        scope.record_fingerprints(schema.tables.iter().map(|t| {
                            (t.name.clone(), ox_core::source_schema::table_fingerprint(t))
                        }));
                    }
                    scope
                })?,
                ontology: None,
                quality_report: None,
                parent_version_id: workspace_parent_version,
                committed_version_id: None,
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
        .create_ontology_draft(&project)
        .await
        .map_err(AppError::from)?;

    info!(
        ontology_draft_id = %project.id,
        source_type = %source_type,
        "Design project created"
    );

    // Fire-and-forget audit
    {
        let audit_store = Arc::clone(&state.store);
        let audit_project_id = project.id.to_string();
        let audit_source_type = source_type.clone();
        ox_context::spawn_scoped(async move {
            if let Err(error) = audit_store
                .record_audit(
                    audit_user_id,
                    "ontology_draft.create",
                    "ontology_draft",
                    Some(&audit_project_id),
                    serde_json::json!({"source_type": audit_source_type}),
                )
                .await
            {
                tracing::warn!(?error, "telemetry record failed");
            }
        });
    }

    Ok((
        StatusCode::CREATED,
        ApiResponse::of(OntologyDraftView::from_ontology_draft(project)),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontology-drafts",
    params(
        ("limit" = Option<u32>, Query, description = "Max items to return (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from a previous response"),
    ),
    responses(
        (status = 200, description = "Paginated project list", body = crate::openapi::OntologyDraftSummaryPage),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn list_ontology_drafts(
    State(state): State<AppState>,
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<ApiResponse<Vec<OntologyDraftSummary>>>, AppError> {
    let page = state
        .store
        .list_ontology_drafts(&pagination)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// GET /api/ontology-drafts/:id
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontology-drafts/{id}",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    responses(
        (status = 200, description = "Ontology draft details", body = OntologyDraftView),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn get_ontology_draft(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<OntologyDraftView>>, AppError> {
    let project = reload_ontology_draft(&state, id).await?;
    Ok(ApiResponse::of(OntologyDraftView::from_ontology_draft(
        project,
    )))
}

// ---------------------------------------------------------------------------
// DELETE /api/ontology-drafts/:id
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/ontology-drafts/{id}",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    responses(
        (status = 204, description = "Ontology draft deleted"),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn delete_ontology_draft(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;

    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::ontology_draft_not_found)?;

    principal.require_project_owner(&project.user_id)?;

    let deleted = state
        .store
        .delete_ontology_draft(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        // Fire-and-forget audit
        {
            let audit_store = Arc::clone(&state.store);
            let audit_user_id = principal.user_uuid().ok();
            let audit_project_id = id.to_string();
            ox_context::spawn_scoped(async move {
                if let Err(error) = audit_store
                    .record_audit(
                        audit_user_id,
                        "ontology_draft.delete",
                        "ontology_draft",
                        Some(&audit_project_id),
                        serde_json::json!({}),
                    )
                    .await
                {
                    tracing::warn!(?error, "telemetry record failed");
                }
            });
        }

        // Fire-and-forget: clean up orphaned memory entries scoped to
        // the workspace's canonical ontology. The project does not
        // own the ontology — it iterates against the workspace's
        // singleton — so we look the id up rather than carrying a
        // stale FK on the project row.
        if let Some(ref memory) = state.memory
            && let Ok(Some(ontology)) = state.store.get_workspace_ontology().await
        {
            let mem = Arc::clone(memory);
            let oid = ontology.id.to_string();
            ox_context::spawn_scoped(async move {
                match mem.cleanup_by_ontology(&oid).await {
                    Ok(n) if n > 0 => {
                        info!(count = n, ontology_id = %oid, "Cleaned orphaned memory entries")
                    }
                    Err(e) => warn!(error = %e, "Memory cleanup failed"),
                    _ => {}
                }
            });
        }
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::ontology_draft_not_found())
    }
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/complete
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/complete",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = CompleteOntologyDraftRequest,
    responses(
        (status = 200, description = "Ontology draft completed, ontology saved", body = OntologyDraftView),
        (status = 400, description = "Ontology draft has no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Quality gate failed", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn complete_ontology_draft(
    State(state): State<AppState>,
    principal: Principal,
    _ws: crate::workspace::WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteOntologyDraftRequest>,
) -> Result<Json<ApiResponse<OntologyDraftView>>, AppError> {
    principal.require_designer()?;
    let project = load_ontology_draft_in_status(&state, id, OntologyDraftStatus::Designed).await?;

    // Quality gate: reject completion unless confidence is high or user explicitly acknowledges
    if !req.acknowledge_quality_risks
        && let Some(qr) = &project.quality_report
        && let Ok(report) = serde_json::from_value::<OntologyQualityReport>(qr.clone())
        && !matches!(
            report.confidence,
            ox_ontology::quality::QualityConfidence::High
        )
    {
        return Err(AppError::quality_gate(format!(
            "Quality confidence is '{}'. Resolve gaps via refine, \
             or set acknowledge_quality_risks=true to proceed.",
            match report.confidence {
                ox_ontology::quality::QualityConfidence::Low => "low",
                ox_ontology::quality::QualityConfidence::Medium => "medium",
                ox_ontology::quality::QualityConfidence::High => "high",
            }
        )));
    }

    let ontology_json = project
        .ontology
        .as_ref()
        .ok_or_else(AppError::no_ontology)?
        .clone();
    let ontology: ox_ontology::OntologyIR = serde_json::from_value(ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse project ontology: {e}")))?;

    // Φ12 — pre-commit gate: every mapping must reference a relation
    // and column that exists on the captured source-contract bank
    // for its source. Soft-skips per source until that source has at
    // least one captured contract — bootstrap-path exemption (see
    // ADR-0036). Once introspection has run for the source, the gate
    // becomes enforcing.
    let source_contracts = state
        .store
        .list_source_contracts()
        .await
        .map_err(AppError::from)?;
    let contract_violations = ontology.validate_against_source_contracts(&source_contracts);
    if !contract_violations.is_empty() {
        return Err(AppError::source_contract_mismatch(contract_violations));
    }

    // Identity resolution: workspace × ontology is 1:1, so the
    // commit either lands on the workspace's canonical (existing
    // identity → version N+1) or creates the canonical (no
    // identity → first version of the workspace's lineage).
    let existing_identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?;

    let (identity, parent_version, next_version_tag, previous_ir) =
        if let Some(identity) = existing_identity {
            let current = state
                .store
                .find_current_version(identity.id)
                .await
                .map_err(AppError::from)?;

            // Lost-update guard. The project's `parent_version_id`
            // pins the canonical version its draft was branched
            // from. If canonical advanced past that point while the
            // project was in flight, a blind commit would silently
            // overwrite the intervening changes — refuse instead and
            // surface the rebase context to the FE. Greenfield
            // workspaces (no canonical yet) and projects whose
            // parent matches the current head fall through.
            match (project.parent_version_id, current.as_ref().map(|v| v.id)) {
                (Some(parent), Some(head)) if parent != head => {
                    let parent_tag = state
                        .store
                        .get_version_snapshot(parent)
                        .await
                        .map_err(AppError::from)?
                        .map(|s| s.version)
                        .unwrap_or_else(|| "?".to_string());
                    let head_tag = current
                        .as_ref()
                        .map(|v| v.version.clone())
                        .unwrap_or_else(|| "?".to_string());
                    return Err(AppError::ontology_draft_stale_parent(
                        &parent_tag,
                        &head_tag,
                    ));
                }
                (Some(_), None) => {
                    return Err(AppError::ontology_draft_stale_parent_canonical_wiped());
                }
                _ => {}
            }

            let next_tag = current
                .as_ref()
                .map(|v| super::helpers::next_ontology_version_tag(&v.version))
                .unwrap_or_else(|| "1".to_string());
            let prev_ir = if let Some(v) = &current {
                state
                    .store
                    .get_ontology_ir(v.id)
                    .await
                    .map_err(AppError::from)?
            } else {
                None
            };
            (identity, current.map(|v| v.id), next_tag, prev_ir)
        } else {
            let display_name_json = AppError::to_json(&ontology.display_name)?;
            let description_json = AppError::to_json(&req.description)?;
            // Seed the new identity's lineage id from the ontology's own id so
            // external references such as quality rules and saved queries keep
            // a stable, user-visible anchor.
            let lineage_seed = if ontology.id.is_empty() {
                None
            } else {
                Some(ontology.id.as_str())
            };
            let identity = state
                .store
                .create_ontology(
                    &req.name,
                    &display_name_json,
                    &description_json,
                    lineage_seed,
                )
                .await
                .map_err(AppError::from)?;
            (identity, None, "1".to_string(), None)
        };

    let commit_message = format!(
        "Completed design project {id} — v{next_version_tag}{maybe_note}",
        maybe_note = if parent_version.is_some() {
            ""
        } else {
            " (initial commit)"
        }
    );
    let snapshot = state
        .store
        .commit_version(
            identity.id,
            &ontology,
            &next_version_tag,
            parent_version,
            &principal.id,
            &commit_message,
            ox_ontology::ProvenanceCapture::ontology_edit(
                ox_ontology::AgentRef::User {
                    user_id: principal.id.clone(),
                },
                &commit_message,
            )
            .with_derived_from(parent_version.into_iter().map(|pv| {
                ox_ontology::EntityRef::Arbitrary {
                    label: format!("ontology_version_snapshot:{pv}"),
                }
            }))
            .with_used(std::iter::once(ox_ontology::EntityRef::Arbitrary {
                label: format!("ontology_draft:{id}"),
            })),
            ox_text::glossary_tokenizer_fingerprint(&ontology).as_str(),
        )
        .await
        .map_err(AppError::from)?;

    state
        .store
        .complete_ontology_draft(id, snapshot.id, req.revision)
        .await
        .map_err(AppError::from)?;

    // Republish the workspace tokenizer if the glossary
    // shifted; backfill stale tokenized_text rows in the
    // background.
    crate::tokenizer_publish::publish_workspace_tokenizer_after_commit(
        &state,
        identity.workspace_id,
        parent_version,
        &ontology,
    )
    .await
    .map_err(AppError::from)?;

    let updated = reload_ontology_draft(&state, id).await?;

    // Invalidate the compile-plan cache: every entry keyed against the
    // previous schema version is now stale (GraphLabel / PropertyKey
    // identifiers may have been renamed, indexes added/dropped). A
    // rebuild-on-demand is cheaper than serving a stale plan, and
    // dashboard users will re-populate the cache within seconds of their
    // next refresh.
    if let Some(cache) = &state.plan_cache {
        cache.invalidate_all();
    }

    info!(
        ontology_draft_id = %id,
        ontology_id = %identity.id,
        version_id = %snapshot.id,
        version = %next_version_tag,
        "Design project completed"
    );

    // Schema RAG indexing: embed ontology nodes for vector search in query translation.
    // Non-blocking — indexing failure doesn't affect project completion.
    if let Some(memory) = &state.memory {
        let memory = Arc::clone(memory);
        let ontology_key = identity.id.to_string();
        let ont_clone = ontology.clone();
        ox_context::spawn_scoped(async move {
            ox_brain::schema_rag::index_ontology_schema(&memory, &ont_clone, &ontology_key).await;
        });
    }

    // Knowledge lifecycle: mark stale entries when breaking schema changes detected.
    // Non-blocking — lifecycle failure doesn't affect project completion.
    if let Some(prev_ont) = previous_ir {
        let store = Arc::clone(&state.store);
        let ontology_name = req.name.clone();
        let new_ont = ontology.clone();
        ox_context::spawn_scoped(async move {
            let diff = ox_ontology::compute_diff(&prev_ont, &new_ont);
            if diff.is_empty() {
                return;
            }
            let breaking = ox_ontology::breaking_labels(&diff);
            if breaking.is_empty() {
                return;
            }
            // Postgres binding expects `&[String]`; the breaking_labels
            // call returns GraphLabel so we unwrap through `.to_string()`
            // at the store boundary rather than threading a newtype
            // through the sqlx encoder.
            let breaking_str: Vec<String> = breaking.iter().map(|l| l.to_string()).collect();
            match store
                .expire_knowledge_by_labels(&ontology_name, &breaking_str)
                .await
            {
                Ok(count) if count > 0 => {
                    tracing::info!(
                        ontology = %ontology_name,
                        stale_count = count,
                        "Marked knowledge entries as stale due to breaking schema changes"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Knowledge lifecycle failed");
                }
                _ => {}
            }
        });
    }

    Ok(ApiResponse::of(OntologyDraftView::from_ontology_draft(
        updated,
    )))
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/deploy-schema — deploy ontology schema to graph DB
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DeployOntologyDraftSchemaRequest {
    /// If true, return DDL statements without executing them
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DeployOntologyDraftSchemaResponse {
    /// Generated DDL statements
    pub statements: Vec<String>,
    /// Whether the statements were actually executed
    pub executed: bool,
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/deploy-schema",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = DeployOntologyDraftSchemaRequest,
    responses(
        (status = 200, description = "Schema deployed or DDL preview returned", body = DeployOntologyDraftSchemaResponse),
        (status = 400, description = "Ontology draft has no ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn deploy_schema(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<DeployOntologyDraftSchemaRequest>,
) -> Result<Json<ApiResponse<DeployOntologyDraftSchemaResponse>>, AppError> {
    principal.require_designer()?;

    // Check if workspace has pending approval blocking this deployment
    let pending = state
        .store
        .list_pending_approvals(ws.workspace_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to check approvals: {e}")))?;

    let blocked_by_approval = pending.iter().any(|a| {
        a.resource_type == "ontology_draft"
            && a.resource_id == id.to_string()
            && a.action_type == "deploy_schema"
            && a.status == "pending"
    });

    if blocked_by_approval {
        return Err(AppError::deploy_pending_approval());
    }

    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::ontology_draft_not_found)?;

    let ontology_json = project.ontology.ok_or_else(AppError::no_ontology)?;
    let ontology: ox_ontology::ir::OntologyIR = serde_json::from_value(ontology_json)
        .map_err(|e| AppError::internal(format!("Failed to parse ontology: {e}")))?;

    let statements = state
        .compiler
        .compile_schema(&ontology)
        .map_err(AppError::from)?;

    if req.dry_run {
        return Ok(ApiResponse::of(DeployOntologyDraftSchemaResponse {
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
        ontology_draft_id = %id,
        statements = statements.len(),
        "Schema deployed to graph database"
    );

    let now = Utc::now();
    let lineage_entry = ox_store::LineageEntry {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        ontology_draft_id: Some(id),
        graph_label: "schema_deploy".to_string(),
        graph_element_type: "schema".to_string(),
        source_type: "ontology_ir".to_string(),
        source_name: project.title.clone().unwrap_or_default(),
        source_table: None,
        source_columns: None,
        load_plan_hash: None,
        property_mappings: None,
        record_count: statements.len() as i64,
        loaded_by: principal.user_uuid().ok(),
        started_at: now,
        completed_at: Some(now),
        status: "succeeded".to_string(),
        error_message: None,
    };
    if let Err(error) = state.store.create_lineage_entry(&lineage_entry).await {
        tracing::warn!(?error, "lineage entry create failed for schema deploy");
    }

    {
        let audit_store = Arc::clone(&state.store);
        let audit_user_id = principal.user_uuid().ok();
        let audit_project_id = id.to_string();
        let stmt_count = statements.len();
        ox_context::spawn_scoped(async move {
            if let Err(error) = audit_store
                .record_audit(
                    audit_user_id,
                    "schema.deploy",
                    "ontology_draft",
                    Some(&audit_project_id),
                    serde_json::json!({"statements_count": stmt_count}),
                )
                .await
            {
                tracing::warn!(?error, "telemetry record failed");
            }
        });
    }

    Ok(ApiResponse::of(DeployOntologyDraftSchemaResponse {
        statements,
        executed: true,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/load-plan — generate a LoadPlan for the project
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct GenerateOntologyDraftLoadPlanResponse {
    /// The generated load plan
    pub plan: ox_ontology::load_plan::LoadPlan,
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/load-plan",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    responses(
        (status = 200, description = "Load plan generated", body = GenerateOntologyDraftLoadPlanResponse),
        (status = 400, description = "Ontology draft has no ontology or source mapping", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn generate_load_plan(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<GenerateOntologyDraftLoadPlanResponse>>, AppError> {
    principal.require_designer()?;

    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::ontology_draft_not_found)?;

    let ontology_json = project
        .ontology
        .as_ref()
        .ok_or_else(AppError::no_ontology)?;
    let ontology: ox_ontology::ir::OntologyIR = serde_json::from_value(ontology_json.clone())
        .map_err(|e| AppError::internal(format!("Failed to parse ontology: {e}")))?;

    let source_schema_json = project.source_schema.as_ref().ok_or_else(|| {
        AppError::ontology_draft_missing_source_schema(
            "Run analyze + introspect first to populate the source schema.",
        )
    })?;
    let source_schema: ox_core::SourceSchema =
        serde_json::from_value(source_schema_json.clone())
            .map_err(|e| AppError::internal(format!("Failed to parse source schema: {e}")))?;

    let plan = state
        .brain
        .generate_load_plan(
            &ontology,
            &source_schema,
            &entelix::ExecutionContext::default(),
        )
        .await
        .map_err(AppError::from)?;

    info!(
        ontology_draft_id = %id,
        steps = plan.steps.len(),
        "Load plan generated"
    );

    Ok(ApiResponse::of(GenerateOntologyDraftLoadPlanResponse {
        plan,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/load/compile — compile a LoadPlan into target DDL
//
// Returns the compiled Cypher statements for preview. The statements contain
// $batch parameter placeholders — actual execution requires the source data
// pipeline (AdapterRegistry → fetch → batch → execute_load) which is
// not yet connected.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CompileOntologyDraftLoadPlanRequest {
    /// The load plan to compile
    pub plan: ox_ontology::load_plan::LoadPlan,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CompileOntologyDraftLoadPlanResponse {
    /// Compiled load statements (parameterized — $batch must be bound at execution time)
    pub statements: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/load/compile",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = CompileOntologyDraftLoadPlanRequest,
    responses(
        (status = 200, description = "Compiled load statements", body = CompileOntologyDraftLoadPlanResponse),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn compile_load(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<CompileOntologyDraftLoadPlanRequest>,
) -> Result<Json<ApiResponse<CompileOntologyDraftLoadPlanResponse>>, AppError> {
    principal.require_designer()?;

    // Verify project exists. The `?` chain propagates the error; we
    // discard the resolved value because compile_load only needs the
    // request payload.
    state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::ontology_draft_not_found)?;

    let statements = state
        .compiler
        .compile_load(&req.plan)
        .map_err(AppError::from)?;

    info!(
        ontology_draft_id = %id,
        statements = statements.len(),
        "Load plan compiled"
    );

    Ok(ApiResponse::of(CompileOntologyDraftLoadPlanResponse {
        statements,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/ontology-drafts/:id/load/execute — fetch from source + load into graph
//
// Completes the E2E pipeline: source → fetch → compile → execute_load.
// Requires the source connection string (not stored for security).
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ExecuteOntologyDraftLoadRequest {
    /// Pre-computed load plan (from generate_load_plan or manual)
    pub plan: ox_ontology::load_plan::LoadPlan,
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
pub struct ExecuteOntologyDraftLoadResponse {
    /// Total rows fetched from source
    pub rows_fetched: u64,
    /// Load execution result
    pub result: LoadResultResponse,
    /// Number of load steps executed
    pub steps_executed: usize,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoadResultResponse {
    pub nodes_created: usize,
    pub nodes_updated: usize,
    pub edges_created: usize,
    pub edges_updated: usize,
    pub batches_processed: usize,
    pub batches_failed: usize,
    pub errors: Vec<LoadErrorResponse>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoadErrorResponse {
    pub batch_index: usize,
    pub message: String,
}

impl From<ox_graph_runtime::LoadResult> for LoadResultResponse {
    fn from(result: ox_graph_runtime::LoadResult) -> Self {
        Self {
            nodes_created: result.nodes_created,
            nodes_updated: result.nodes_updated,
            edges_created: result.edges_created,
            edges_updated: result.edges_updated,
            batches_processed: result.batches_processed,
            batches_failed: result.batches_failed,
            errors: result.errors.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ox_graph_runtime::LoadError> for LoadErrorResponse {
    fn from(error: ox_graph_runtime::LoadError) -> Self {
        Self {
            batch_index: error.batch_index,
            message: error.message,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/ontology-drafts/{id}/load/execute",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = ExecuteOntologyDraftLoadRequest,
    responses(
        (status = 200, description = "Data loaded from source into graph", body = ExecuteOntologyDraftLoadResponse),
        (status = 400, description = "Missing ontology or source mapping", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph runtime not connected", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn execute_load_from_source(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<ExecuteOntologyDraftLoadRequest>,
) -> Result<Json<ApiResponse<ExecuteOntologyDraftLoadResponse>>, AppError> {
    principal.require_designer()?;

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::ontology_draft_not_found)?;

    // Parse the project's ontology — object_mappings on the IR are
    // the single source of truth for "which source table supplies this
    // node", replacing the legacy flat SourceMapping side-car.
    let ontology_json = project
        .ontology
        .as_ref()
        .ok_or_else(AppError::no_ontology)?;
    let ontology: ox_ontology::ir::OntologyIR = serde_json::from_value(ontology_json.clone())
        .map_err(|e| AppError::internal(format!("Failed to parse ontology: {e}")))?;
    let object_mappings = ontology.object_mappings();

    // Determine schema name from project source config
    let source_config: ox_ontology::ontology_draft::SourceConfig =
        serde_json::from_value(project.source_config.clone())
            .map_err(|e| AppError::internal(format!("Failed to parse source config: {e}")))?;
    let schema_name = source_config.schema_name.as_deref().unwrap_or("public");

    // Connect to source database
    let fetcher =
        ox_source::postgres_fetcher::PostgresFetcher::connect(&req.connection_string, schema_name)
            .await
            .map_err(|e| AppError::source_connection_failed("postgresql", e.to_string()))?;

    // Compile load plan to Cypher statements
    let compiled_statements = state
        .compiler
        .compile_load(&req.plan)
        .map_err(AppError::from)?;

    info!(
        ontology_draft_id = %id,
        steps = req.plan.steps.len(),
        compiled = compiled_statements.len(),
        "Starting load execution from source"
    );

    // Create lineage entry before load execution
    let lineage_id = Uuid::new_v4();
    let source_type_str = source_config.source_type.to_string();
    let property_mappings = extract_all_mappings(&req.plan);
    let lineage_entry = ox_store::LineageEntry {
        id: lineage_id,
        workspace_id: ws.workspace_id,
        ontology_draft_id: Some(id),
        graph_label: "batch_load".to_string(),
        graph_element_type: "node".to_string(),
        source_type: source_type_str,
        source_name: schema_name.to_string(),
        source_table: None,
        source_columns: None,
        load_plan_hash: None,
        property_mappings: serde_json::to_value(&property_mappings).ok(),
        record_count: 0,
        loaded_by: principal.user_uuid().ok(),
        started_at: Utc::now(),
        completed_at: None,
        status: "running".to_string(),
        error_message: None,
    };
    if let Err(error) = state.store.create_lineage_entry(&lineage_entry).await {
        tracing::warn!(?error, "lineage entry create failed");
    }

    let mut total_rows_fetched: u64 = 0;
    let mut combined_result = ox_graph_runtime::LoadResult {
        nodes_created: 0,
        nodes_updated: 0,
        edges_created: 0,
        edges_updated: 0,
        batches_processed: 0,
        batches_failed: 0,
        errors: Vec::new(),
    };

    // Determine load mode: full (default) or incremental (watermark-based)
    let incremental_config = match &req.plan.mode {
        ox_ontology::load_plan::LoadMode::Incremental { watermark_column } => {
            Some(watermark_column.clone())
        }
        ox_ontology::load_plan::LoadMode::Full => None,
    };

    // Execute each load step: fetch from source table → execute against graph
    for (step_idx, (step, cypher)) in req.plan.steps.iter().zip(&compiled_statements).enumerate() {
        // Determine source table from the load operation
        let source_table = resolve_source_table(&step.operation, object_mappings, &ontology);
        let source_table = match source_table {
            Some(t) => t,
            None => {
                warn!(
                    step = step_idx,
                    "Could not resolve source table for step — skipping"
                );
                continue;
            }
        };

        // Determine which columns to fetch based on the operation's property mappings
        let columns = extract_source_columns(&step.operation);
        let graph_label = graph_label_for_op(&step.operation);

        // Incremental mode: look up previous checkpoint to get the watermark value
        let watermark_state = if let Some(wm_col) = &incremental_config {
            let checkpoint = state
                .store
                .get_load_checkpoint(id, &source_table, &graph_label)
                .await
                .map_err(|e| AppError::internal(format!("Failed to read checkpoint: {e}")))?;
            Some((wm_col.clone(), checkpoint))
        } else {
            None
        };

        // Branch: incremental fetch vs. full fetch
        if let Some((wm_col, checkpoint)) = &watermark_state {
            let wm_value = checkpoint
                .as_ref()
                .map(|c| c.watermark_value.clone())
                .unwrap_or_default();

            info!(
                step = step_idx,
                table = %source_table,
                watermark_column = %wm_col,
                watermark_from = %wm_value,
                "Fetching incremental data for load step"
            );

            // Ensure the watermark column is included in the fetched columns
            let mut inc_columns = columns.clone();
            if !inc_columns.contains(wm_col) {
                inc_columns.push(wm_col.clone());
            }

            let mut max_watermark = wm_value.clone();
            let mut step_rows: i64 = 0;

            loop {
                let rows = fetcher
                    .fetch_incremental(
                        &source_table,
                        &inc_columns,
                        wm_col,
                        &max_watermark,
                        req.batch_size,
                    )
                    .await
                    .map_err(|e| {
                        AppError::internal(format!(
                            "Failed to fetch incremental from {source_table}: {e}"
                        ))
                    })?;

                if rows.is_empty() {
                    break;
                }

                let batch_len = rows.len();
                total_rows_fetched += batch_len as u64;
                step_rows += batch_len as i64;

                // Track the max watermark value in this batch
                for row in &rows {
                    if let Some(val) = row.get(wm_col) {
                        let val_str = match val {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        if val_str > max_watermark {
                            max_watermark = val_str;
                        }
                    }
                }

                let values: Vec<serde_json::Value> =
                    rows.into_iter().map(serde_json::Value::Object).collect();
                let batch =
                    ox_graph_runtime::LoadBatch::from_values(values).map_err(AppError::from)?;

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

                // If we got fewer rows than the batch size, we're done
                if (batch_len as u64) < req.batch_size {
                    break;
                }
            }

            // Upsert checkpoint with the new max watermark.
            // `id` and `workspace_id` are persistence-side fields the
            // store fills in from the column DEFAULT and the bound
            // task-local respectively.
            if max_watermark != wm_value || step_rows > 0 {
                let cp = ox_store::LoadCheckpoint::draft(
                    id,
                    source_table.clone(),
                    graph_label.clone(),
                    wm_col.clone(),
                    max_watermark,
                    step_rows,
                );
                if let Err(error) = state.store.upsert_load_checkpoint(&cp).await {
                    tracing::warn!(?error, %graph_label, "load checkpoint upsert failed");
                }
            }
        } else {
            // Full mode: original pagination-based fetch
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
                        AppError::internal(format!(
                            "Failed to fetch batch from {source_table}: {e}"
                        ))
                    })?;

                if rows.is_empty() {
                    break;
                }

                let batch_len = rows.len();
                total_rows_fetched += batch_len as u64;

                let values: Vec<serde_json::Value> =
                    rows.into_iter().map(serde_json::Value::Object).collect();
                let batch =
                    ox_graph_runtime::LoadBatch::from_values(values).map_err(AppError::from)?;

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
    }

    // Complete lineage entry after load
    let lineage_status = if combined_result.batches_failed > 0 {
        "partial"
    } else {
        "completed"
    };
    let lineage_error = if combined_result.errors.is_empty() {
        None
    } else {
        Some(
            combined_result
                .errors
                .iter()
                .take(3)
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        )
    };
    if let Err(error) = state
        .store
        .complete_lineage_entry(
            lineage_id,
            total_rows_fetched as i64,
            lineage_status,
            lineage_error.as_deref(),
        )
        .await
    {
        tracing::warn!(?error, %lineage_id, "lineage completion record failed");
    }

    info!(
        ontology_draft_id = %id,
        rows_fetched = total_rows_fetched,
        nodes_created = combined_result.nodes_created,
        edges_created = combined_result.edges_created,
        "Load execution completed"
    );

    // Auto-enrich ontology descriptions with sample values from loaded data.
    // Fire-and-forget: enrichment failure doesn't affect load success. Under
    // the Λ storage model, each enrichment produces a new version snapshot
    // (immutable history) rather than overwriting an IR in place.
    if let Some(runtime) = &state.runtime {
        let runtime = Arc::clone(runtime);
        let store = Arc::clone(&state.store);
        let committer = principal.id.clone();
        ox_context::spawn_scoped(async move {
            let Ok(Some(canonical)) = store.get_workspace_ontology().await else {
                return;
            };
            let ont_id = canonical.id;
            let Ok(Some(current_version)) = store.find_current_version(ont_id).await else {
                return;
            };
            let Ok(Some(ontology)) = store.get_ontology_ir(current_version.id).await else {
                return;
            };
            let config = ox_graph_runtime::profiler::ProfileConfig::for_ontology_size(
                ontology.node_types().len(),
            );
            let Ok(profile) =
                ox_graph_runtime::profiler::profile_graph(runtime.as_ref(), &ontology, &config)
                    .await
            else {
                return;
            };
            let result = ox_graph_runtime::enrichment::enrich_descriptions(&ontology, &profile);
            if result.changes.is_empty() {
                return;
            }
            let next_tag = crate::routes::ontology_drafts::helpers::next_ontology_version_tag(
                &current_version.version,
            );
            let message = format!(
                "Auto-enrichment after data load: {} property description(s) updated",
                result.changes.len()
            );
            match store
                .commit_version(
                    ont_id,
                    &result.ontology,
                    &next_tag,
                    Some(current_version.id),
                    &committer,
                    &message,
                    ox_ontology::ProvenanceCapture::ontology_edit(
                        ox_ontology::AgentRef::Service {
                            service_id: "auto_enrichment".into(),
                        },
                        &message,
                    )
                    .with_derived_from(std::iter::once(
                        ox_ontology::EntityRef::Arbitrary {
                            label: format!("ontology_version_snapshot:{}", current_version.id),
                        },
                    )),
                    ox_text::glossary_tokenizer_fingerprint(&result.ontology).as_str(),
                )
                .await
            {
                Ok(snapshot) => tracing::info!(
                    ontology_id = %ont_id,
                    version_id = %snapshot.id,
                    version = %next_tag,
                    changes = result.changes.len(),
                    "Auto-enriched ontology after data load"
                ),
                Err(e) => tracing::warn!(error = %e, "Auto-enrichment commit failed"),
            }
        });
    }

    // Record metering (fire-and-forget)
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        let steps = req.plan.steps.len();
        let nodes = combined_result.nodes_created;
        let edges = combined_result.edges_created;
        let rows = total_rows_fetched;
        ox_context::spawn_scoped(async move {
            if let Err(error) = meter_store
                .record_usage(
                    meter_user,
                    "data_load",
                    None,
                    None,
                    Some("load_from_source"),
                    0,
                    0,
                    0, // duration not tracked for load
                    0.0,
                    serde_json::json!({
                        "rows_fetched": rows,
                        "nodes_created": nodes,
                        "edges_created": edges,
                        "steps": steps,
                    }),
                )
                .await
            {
                tracing::warn!(?error, "telemetry record failed");
            }
        });
    }

    Ok(ApiResponse::of(ExecuteOntologyDraftLoadResponse {
        rows_fetched: total_rows_fetched,
        result: combined_result.into(),
        steps_executed: req.plan.steps.len(),
    }))
}

/// Resolve which source table to fetch from, given a load operation and
/// the canonical object-mapping slice from the ontology. Resolves by
/// matching the load op's target/source node label against the label
/// of the `NodeTypeDef` pointed at by each `ObjectMappingDef` — the IR
/// is the single source of truth for node→table binding.
fn resolve_source_table(
    op: &ox_ontology::load_plan::LoadOp,
    object_mappings: &[ox_ontology::ObjectMappingDef],
    ontology: &ox_ontology::ir::OntologyIR,
) -> Option<String> {
    use ox_ontology::load_plan::LoadOp;

    // Look up the ObjectMappingDef whose node's label matches `label`
    // (case-insensitive). Returns the `relation` (source table).
    let table_for_label = |label: &str| -> Option<String> {
        let lower = label.to_lowercase();
        object_mappings.iter().find_map(|om| {
            let node_label = ontology
                .node_types()
                .iter()
                .find(|n| n.id == om.node_type_id)?
                .label
                .as_str()
                .to_lowercase();
            if node_label == lower {
                Some(om.relation.clone())
            } else {
                None
            }
        })
    };

    match op {
        LoadOp::UpsertNode { target_label, .. } => {
            table_for_label(target_label).or_else(|| {
                // Fallback: case-insensitive pluralized relation-name heuristic,
                // mirroring the previous behaviour for ontologies whose labels
                // don't match their table names directly.
                let lower = target_label.to_lowercase();
                object_mappings
                    .iter()
                    .map(|om| om.relation.clone())
                    .find(|t| {
                        let tl = t.to_lowercase();
                        tl == lower
                            || tl == format!("{lower}s")
                            || tl.ends_with(&format!("_{lower}"))
                    })
            })
        }
        LoadOp::UpsertEdge { source_match, .. } => {
            // Edges typically come from one of the node tables or a junction table.
            // Match on the source node's label to find the originating table.
            table_for_label(&source_match.label).or_else(|| {
                let lower = source_match.label.to_lowercase();
                object_mappings
                    .iter()
                    .map(|om| om.relation.clone())
                    .find(|t| t.to_lowercase().contains(&lower))
            })
        }
    }
}

/// Per-label aggregated property mappings extracted from a LoadPlan.
#[derive(Serialize)]
struct LabelMappings {
    label: String,
    element_type: String,
    mappings: Vec<FlatPropertyMapping>,
}

/// Flat serializable property mapping for JSON storage.
#[derive(Serialize)]
struct FlatPropertyMapping {
    source_column: String,
    graph_property: String,
    transform: Option<String>,
    mapping_kind: String, // "match" or "set"
}

/// Extract all property mappings from a LoadPlan, grouped by target label.
fn extract_all_mappings(plan: &ox_ontology::load_plan::LoadPlan) -> Vec<LabelMappings> {
    use ox_ontology::load_plan::LoadOp;

    plan.steps
        .iter()
        .map(|step| match &step.operation {
            LoadOp::UpsertNode {
                target_label,
                match_fields,
                set_fields,
                ..
            } => {
                let mut mappings: Vec<FlatPropertyMapping> = match_fields
                    .iter()
                    .map(|m| FlatPropertyMapping {
                        source_column: m.source_column.clone(),
                        graph_property: m.graph_property.clone(),
                        transform: m.transform.as_ref().map(|t| format!("{t:?}")),
                        mapping_kind: "match".to_string(),
                    })
                    .collect();
                mappings.extend(set_fields.iter().map(|m| FlatPropertyMapping {
                    source_column: m.source_column.clone(),
                    graph_property: m.graph_property.clone(),
                    transform: m.transform.as_ref().map(|t| format!("{t:?}")),
                    mapping_kind: "set".to_string(),
                }));
                LabelMappings {
                    label: target_label.clone(),
                    element_type: "node".to_string(),
                    mappings,
                }
            }
            LoadOp::UpsertEdge {
                target_label,
                source_match,
                target_match,
                set_fields,
                ..
            } => {
                let mut mappings = vec![
                    FlatPropertyMapping {
                        source_column: source_match.source_field.clone(),
                        graph_property: format!(
                            "{}:{}",
                            source_match.label, source_match.match_property
                        ),
                        transform: None,
                        mapping_kind: "match".to_string(),
                    },
                    FlatPropertyMapping {
                        source_column: target_match.source_field.clone(),
                        graph_property: format!(
                            "{}:{}",
                            target_match.label, target_match.match_property
                        ),
                        transform: None,
                        mapping_kind: "match".to_string(),
                    },
                ];
                mappings.extend(set_fields.iter().map(|m| FlatPropertyMapping {
                    source_column: m.source_column.clone(),
                    graph_property: m.graph_property.clone(),
                    transform: m.transform.as_ref().map(|t| format!("{t:?}")),
                    mapping_kind: "set".to_string(),
                }));
                LabelMappings {
                    label: target_label.clone(),
                    element_type: "edge".to_string(),
                    mappings,
                }
            }
        })
        .collect()
}

/// Extract source column names from a load operation's property mappings.
fn extract_source_columns(op: &ox_ontology::load_plan::LoadOp) -> Vec<String> {
    use ox_ontology::load_plan::LoadOp;

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

/// Extract the graph label from a load operation (for checkpoint keying).
fn graph_label_for_op(op: &ox_ontology::load_plan::LoadOp) -> String {
    use ox_ontology::load_plan::LoadOp;
    match op {
        LoadOp::UpsertNode { target_label, .. } => target_label.clone(),
        LoadOp::UpsertEdge { target_label, .. } => target_label.clone(),
    }
}
