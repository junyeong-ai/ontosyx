use tracing::{info, warn};

use ox_core::design_project::{SourceConfig, SourceTypeKind};
use ox_core::repo_insights::{RepoSource, ValidatedRepoSource};
use ox_core::source_analysis::{RepoAnalysisStatus, RepoAnalysisSummary, SourceAnalysisReport};
use ox_core::source_schema::{SourceProfile, SourceSchema};
use ox_source::analyzer::{build_analysis_report, enrich_with_repo};
use ox_source::repo::{RepoIntrospector, clone_repo, repo_insights_to_schema};

use crate::error::AppError;
use crate::state::AppState;

/// Run repo enrichment on an analysis report.
///
/// This is an optional supplement: failures (timeout, LLM error, no files)
/// are recorded in `report.repo_summary` with an appropriate status rather
/// than failing the entire create/reanalyze operation.
pub(crate) async fn run_repo_enrichment(
    state: &AppState,
    source: &ValidatedRepoSource,
    report: &mut SourceAnalysisReport,
) {
    info!(?source, "Starting repo analysis");

    // For git URLs, clone into a temp directory. The `_guard` keeps the
    // TempDir alive (and the files on disk) until enrichment completes.
    // `cloned_sha` captures the commit SHA so every summary path can include it.
    let (introspector, _guard, cloned_sha) = match source {
        ValidatedRepoSource::Local(path) => match RepoIntrospector::new(path) {
            Ok(i) => (i, None, None),
            Err(e) => {
                warn!(path, error = %e, "Cannot open repo — skipping enrichment");
                report.repo_summary = Some(skipped_repo_summary(format!("Cannot open repo: {e}")));
                return;
            }
        },
        ValidatedRepoSource::GitUrl { url, branch } => {
            match clone_repo(url, branch.as_deref()).await {
                Ok(result) => {
                    info!(commit_sha = %result.commit_sha, "Git clone pinned at commit");
                    let sha = result.commit_sha.clone();
                    (result.introspector, Some(result.tmpdir), Some(sha))
                }
                Err(e) => {
                    warn!(url, error = %e, "Git clone failed — skipping enrichment");
                    report.repo_summary =
                        Some(skipped_repo_summary(format!("Git clone failed: {e}")));
                    return;
                }
            }
        }
    };

    let (file_tree, tree_truncated) = match introspector.generate_tree() {
        Ok(t) => t,
        Err(e) => {
            warn!(?source, error = %e, "Cannot generate file tree — skipping enrichment");
            report.repo_summary = Some(RepoAnalysisSummary {
                commit_sha: cloned_sha.clone(),
                ..skipped_repo_summary(format!("Cannot generate file tree: {e}"))
            });
            return;
        }
    };

    let timeout =
        std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());

    // Navigate repo (LLM selects files)
    let selected_files =
        match tokio::time::timeout(timeout, state.brain.navigate_repo(&file_tree)).await {
            Ok(Ok(files)) => files,
            Ok(Err(e)) => {
                warn!(?source, error = %e, "Repo navigation LLM call failed — skipping enrichment");
                report.repo_summary = Some(RepoAnalysisSummary {
                    commit_sha: cloned_sha.clone(),
                    ..failed_repo_summary(format!("LLM navigation failed: {e}"), tree_truncated)
                });
                return;
            }
            Err(_) => {
                warn!(?source, "Repo navigation timed out — skipping enrichment");
                report.repo_summary = Some(RepoAnalysisSummary {
                    commit_sha: cloned_sha.clone(),
                    ..failed_repo_summary(
                        format!("Navigation timed out after {}s", timeout.as_secs()),
                        tree_truncated,
                    )
                });
                return;
            }
        };

    if selected_files.is_empty() {
        warn!(
            ?source,
            "Repo analysis found no relevant files — skipping enrichment"
        );
        report.repo_summary = Some(RepoAnalysisSummary {
            status: RepoAnalysisStatus::Skipped,
            status_reason: Some("No relevant files found in repository".into()),
            tree_truncated,
            commit_sha: cloned_sha.clone(),
            ..empty_repo_summary()
        });
        return;
    }

    let file_contents = match introspector.read_files(&selected_files) {
        Ok(c) => c,
        Err(e) => {
            warn!(?source, error = %e, "Cannot read selected files — skipping enrichment");
            report.repo_summary = Some(RepoAnalysisSummary {
                commit_sha: cloned_sha.clone(),
                ..failed_repo_summary(format!("Cannot read files: {e}"), tree_truncated)
            });
            return;
        }
    };

    if file_contents.is_empty() {
        warn!(?source, "No readable source files — skipping enrichment");
        report.repo_summary = Some(RepoAnalysisSummary {
            status: RepoAnalysisStatus::Skipped,
            status_reason: Some("Selected files could not be read".into()),
            files_requested: selected_files.len(),
            tree_truncated,
            commit_sha: cloned_sha.clone(),
            ..empty_repo_summary()
        });
        return;
    }

    // Analyze files (LLM extracts enums/relationships)
    let insights =
        match tokio::time::timeout(timeout, state.brain.analyze_repo_files(&file_contents)).await {
            Ok(Ok(ins)) => ins,
            Ok(Err(e)) => {
                warn!(?source, error = %e, "Repo analysis LLM call failed — skipping enrichment");
                report.repo_summary = Some(RepoAnalysisSummary {
                    status: RepoAnalysisStatus::Failed,
                    status_reason: Some(format!("LLM analysis failed: {e}")),
                    files_requested: selected_files.len(),
                    files_analyzed: file_contents.len(),
                    tree_truncated,
                    commit_sha: cloned_sha.clone(),
                    ..empty_repo_summary()
                });
                return;
            }
            Err(_) => {
                warn!(?source, "Repo analysis timed out — skipping enrichment");
                report.repo_summary = Some(RepoAnalysisSummary {
                    status: RepoAnalysisStatus::Failed,
                    status_reason: Some(format!("Analysis timed out after {}s", timeout.as_secs())),
                    files_requested: selected_files.len(),
                    files_analyzed: file_contents.len(),
                    tree_truncated,
                    commit_sha: cloned_sha.clone(),
                    ..empty_repo_summary()
                });
                return;
            }
        };

    enrich_with_repo(report, &insights);

    // Patch repo summary with caller-only info
    if let Some(summary) = &mut report.repo_summary {
        summary.status = if file_contents.len() < selected_files.len() {
            RepoAnalysisStatus::Partial
        } else {
            RepoAnalysisStatus::Complete
        };
        summary.files_requested = selected_files.len();
        summary.tree_truncated = tree_truncated;
        summary.commit_sha = cloned_sha;
    }

    info!(
        suggestions = report.repo_suggestions.len(),
        "Repo enrichment applied"
    );
}

pub(crate) fn empty_repo_summary() -> RepoAnalysisSummary {
    RepoAnalysisSummary {
        status: RepoAnalysisStatus::Skipped,
        status_reason: None,
        framework: None,
        files_requested: 0,
        files_analyzed: 0,
        tree_truncated: false,
        enums_found: 0,
        relationships_found: 0,
        columns_with_suggestions: 0,
        fk_confidence_upgraded: 0,
        commit_sha: None,
        field_hints: Vec::new(),
        domain_notes: Vec::new(),
    }
}

pub(crate) fn skipped_repo_summary(reason: String) -> RepoAnalysisSummary {
    RepoAnalysisSummary {
        status: RepoAnalysisStatus::Skipped,
        status_reason: Some(reason),
        ..empty_repo_summary()
    }
}

fn failed_repo_summary(reason: String, tree_truncated: bool) -> RepoAnalysisSummary {
    RepoAnalysisSummary {
        status: RepoAnalysisStatus::Failed,
        status_reason: Some(reason),
        tree_truncated,
        ..empty_repo_summary()
    }
}

// ---------------------------------------------------------------------------
// CodeRepository — primary source type from code analysis
// ---------------------------------------------------------------------------

/// Analyze a code repository as a primary data source.
///
/// Clones the repo, runs LLM-based file navigation and analysis, converts the
/// extracted RepoInsights into a SourceSchema, and builds an analysis report.
/// Unlike `run_repo_enrichment`, failures here are fatal (the entire operation
/// fails) because the repo is the sole data source.
pub(crate) async fn analyze_code_repository(
    state: &AppState,
    url: &str,
) -> Result<
    (
        SourceConfig,
        SourceSchema,
        SourceProfile,
        SourceAnalysisReport,
    ),
    AppError,
> {
    // Validate as a git URL using the existing validation infrastructure
    let repo_source = RepoSource::GitUrl {
        url: url.to_string(),
        branch: None,
    };
    let validated = repo_source
        .validate(
            &state.repo_policy.allowed_roots,
            &state.repo_policy.allowed_git_hosts,
        )
        .map_err(AppError::from)?;

    info!(url, "Analyzing code repository as primary source");

    // Clone the repo
    let ValidatedRepoSource::GitUrl {
        url: validated_url,
        branch,
    } = &validated
    else {
        return Err(AppError::bad_request(
            "CodeRepository source requires a git URL",
        ));
    };

    let clone_result = clone_repo(validated_url, branch.as_deref())
        .await
        .map_err(|e| AppError::bad_request(format!("Git clone failed: {e}")))?;

    let introspector = clone_result.introspector;
    let _guard = clone_result.tmpdir; // Keep alive until analysis completes
    let commit_sha = clone_result.commit_sha.clone();

    info!(commit_sha = %commit_sha, "Code repository cloned");

    // Generate file tree
    let (file_tree, tree_truncated) = introspector
        .generate_tree()
        .map_err(|e| AppError::bad_request(format!("Cannot generate file tree: {e}")))?;

    let timeout =
        std::time::Duration::from_secs(state.system_config.read().await.design_timeout_secs());

    // Navigate repo (LLM selects files)
    let selected_files = tokio::time::timeout(timeout, state.brain.navigate_repo(&file_tree))
        .await
        .map_err(|_| {
            AppError::timeout(format!(
                "Repo navigation timed out after {}s",
                timeout.as_secs()
            ))
        })?
        .map_err(|e| AppError::internal(format!("Repo navigation failed: {e}")))?;

    if selected_files.is_empty() {
        return Err(AppError::bad_request(
            "No relevant files found in repository for analysis",
        ));
    }

    // Read selected files
    let file_contents = introspector
        .read_files(&selected_files)
        .map_err(|e| AppError::bad_request(format!("Cannot read repo files: {e}")))?;

    if file_contents.is_empty() {
        return Err(AppError::bad_request(
            "Selected files could not be read from repository",
        ));
    }

    // Analyze files (LLM extracts enums/relationships)
    let insights = tokio::time::timeout(timeout, state.brain.analyze_repo_files(&file_contents))
        .await
        .map_err(|_| {
            AppError::timeout(format!(
                "Repo analysis timed out after {}s",
                timeout.as_secs()
            ))
        })?
        .map_err(|e| AppError::internal(format!("Repo analysis failed: {e}")))?;

    info!(
        enums = insights.enum_definitions.len(),
        relationships = insights.orm_relationships.len(),
        field_hints = insights.field_hints.len(),
        "Code repository analyzed"
    );

    // Convert RepoInsights → SourceSchema
    let (schema, profile) = repo_insights_to_schema(&insights);

    if schema.tables.is_empty() {
        return Err(AppError::bad_request(
            "No ORM models or entities found in repository",
        ));
    }

    // Build analysis report from the derived schema
    let mut report = build_analysis_report(&schema, &profile);

    // Enrich report with repo-specific data (enum suggestions, etc.)
    enrich_with_repo(&mut report, &insights);

    // Set repo summary
    report.repo_summary = Some(RepoAnalysisSummary {
        status: if file_contents.len() < selected_files.len() {
            RepoAnalysisStatus::Partial
        } else {
            RepoAnalysisStatus::Complete
        },
        status_reason: None,
        framework: insights.framework.clone(),
        files_requested: selected_files.len(),
        files_analyzed: file_contents.len(),
        tree_truncated,
        enums_found: insights.enum_definitions.len(),
        relationships_found: insights.orm_relationships.len(),
        columns_with_suggestions: report.repo_suggestions.len(),
        fk_confidence_upgraded: 0,
        commit_sha: Some(commit_sha),
        field_hints: insights.field_hints.clone(),
        domain_notes: insights.domain_notes.clone(),
    });

    let source_config = SourceConfig {
        source_type: SourceTypeKind::CodeRepository,
        schema_name: None,
        source_fingerprint: Some(format!("{:016x}", {
            // Use URL as fingerprint basis
            let mut hash: u64 = 0xcbf29ce484222325;
            for &byte in url.as_bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            hash
        })),
    };

    info!(
        tables = schema.tables.len(),
        fks = schema.foreign_keys.len(),
        "Code repository source schema generated"
    );

    Ok((source_config, schema, profile, report))
}
