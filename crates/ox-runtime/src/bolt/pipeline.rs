//! Pre-execute pipeline shared by every Bolt-driver backend.
//!
//! Implements the `GraphRuntime::pre_execute` contract for Neo4j and
//! Memgraph: validate (safety + ontology) → rewrite (workspace-scope) →
//! validate (post-rewrite scope gate). Every cross-cutting Cypher concern
//! the runtime enforces lives here, reading its per-request inputs from
//! the task-locals established by the workspace middleware and the
//! agent/route boundary (`GRAPH_WORKSPACE_ID`, `GRAPH_SYSTEM_BYPASS`,
//! `GRAPH_ONTOLOGY`).
//!
//! Design choices:
//!
//! - **Validator → rewriter → validator ordering.** Safety and ontology
//!   errors must surface before workspace rewriting mutates the query,
//!   so the caller sees the author's original text in diagnostics. The
//!   scope gate runs *after* rewriting to catch any statement the
//!   rewriter failed to scope (hard bug surface, not user error).
//!
//! - **Aggregate-all validation.** The pipeline reports every issue
//!   collected in a single `OxError::Validation` so one LLM retry can
//!   fix all of them. Fail-fast would force one-error-per-round-trip.
//!
//! - **Task-local ontology.** Ontology snapshots are request-scoped; a
//!   runtime instance is shared across projects and cannot bind one
//!   ontology at construction. Absence of the task-local simply skips
//!   the ontology validator — that's how server-internal paths
//!   (`search_nodes`, profiler, introspection) bypass the gate without
//!   each caller having to remember to do so.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::ontology_ir::OntologyIR;
use ox_core::types::PropertyValue;

use crate::cypher::{
    CypherValidatorPipeline, OntologyValidator, SafetyValidator, ValidateContext,
    ValidationReport, WorkspaceScopeValidator,
};
use crate::isolation::GraphIsolationStrategy;
use crate::{GRAPH_ONTOLOGY, GRAPH_SYSTEM_BYPASS, GRAPH_WORKSPACE_ID};

/// Run the full pre-execute pipeline for a Cypher statement.
///
/// Steps:
/// 1. Validate (safety always; ontology iff `GRAPH_ONTOLOGY` is set).
/// 2. Apply the isolation strategy's `scope` — a no-op for
///    `DatabaseStrategy`; injects workspace predicates for
///    `PropertyStrategy`.
/// 3. Validate the rewritten query against the post-rewrite scope gate
///    (iff the strategy exposes a scope property and we're not in system
///    bypass).
///
/// The function is the single home for the per-request policy; both
/// Neo4j and Memgraph just delegate. A new Bolt backend gets safety,
/// ontology, and workspace-scope enforcement for free.
pub(crate) fn run_pre_execute(
    strategy: Option<&dyn GraphIsolationStrategy>,
    cypher: &str,
    params: &HashMap<String, PropertyValue>,
) -> OxResult<(String, HashMap<String, PropertyValue>)> {
    let workspace_id = GRAPH_WORKSPACE_ID
        .try_with(|id| id.to_string())
        .unwrap_or_default();
    let system_bypass = GRAPH_SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false);
    let ontology = GRAPH_ONTOLOGY.try_with(Arc::clone).ok();

    // --- Step 1: pre-rewrite validation ----------------------------------
    run_pre_rewrite_validation(cypher, &workspace_id, ontology.as_deref())?;

    // --- Step 2: rewrite (workspace-scope) -------------------------------
    let (rewritten, merged_params) = apply_isolation(strategy, cypher, params, system_bypass);

    // --- Step 3: post-rewrite validation ---------------------------------
    if !system_bypass
        && let Some(prop) = strategy.and_then(|s| s.scope_property())
    {
        run_post_rewrite_validation(&rewritten, &workspace_id, prop)?;
    }

    Ok((rewritten, merged_params))
}

fn run_pre_rewrite_validation(
    cypher: &str,
    workspace_id: &str,
    ontology: Option<&OntologyIR>,
) -> OxResult<()> {
    let mut pipeline = CypherValidatorPipeline::new().with(SafetyValidator::new());
    if let Some(onto) = ontology {
        pipeline = pipeline.with(OntologyValidator::new(onto.clone()));
    }
    let report = pipeline.run(cypher, &ValidateContext::new(workspace_id));
    log_non_errors(&report, "pre-rewrite", workspace_id);
    ensure_no_errors(&report)
}

fn run_post_rewrite_validation(
    rewritten: &str,
    workspace_id: &str,
    scope_property: &'static str,
) -> OxResult<()> {
    let pipeline =
        CypherValidatorPipeline::new().with(WorkspaceScopeValidator::new(scope_property));
    let report = pipeline.run(rewritten, &ValidateContext::new(workspace_id));
    log_non_errors(&report, "post-rewrite", workspace_id);
    ensure_no_errors(&report)
}

/// Emit `Warning` / `Info` issues to tracing so they show up in server
/// logs without blocking execution. The validators shipping today only
/// produce `Error`-level issues, so in practice this is a no-op; it
/// exists so future validators (complexity hints, ACL advisories) can
/// surface observations without each adding its own logging boilerplate.
fn log_non_errors(report: &ValidationReport, phase: &str, workspace_id: &str) {
    for issue in report.warnings() {
        warn!(
            phase,
            workspace_id,
            validator = %issue.validator_name,
            message = %issue.message,
            "Cypher validator warning",
        );
    }
    for issue in report.infos() {
        info!(
            phase,
            workspace_id,
            validator = %issue.validator_name,
            message = %issue.message,
            "Cypher validator info",
        );
    }
}

fn apply_isolation(
    strategy: Option<&dyn GraphIsolationStrategy>,
    cypher: &str,
    params: &HashMap<String, PropertyValue>,
    system_bypass: bool,
) -> (String, HashMap<String, PropertyValue>) {
    let Some(strategy) = strategy else {
        return (cypher.to_string(), params.clone());
    };
    if system_bypass {
        return (cypher.to_string(), params.clone());
    }
    let Ok(ws_id) = GRAPH_WORKSPACE_ID.try_with(|id| id.to_string()) else {
        return (cypher.to_string(), params.clone());
    };
    let scoped = strategy.scope(cypher, &ws_id);
    let mut merged = params.clone();
    for (key, value) in scoped.params {
        merged.insert(key.to_string(), PropertyValue::String(value));
    }
    (scoped.query, merged)
}

/// Collapse every `Error`-level issue into a single `OxError::Validation`
/// whose message lists the offending issues one per line. `Warning` /
/// `Info` issues are surfaced via `log_non_errors` before this runs,
/// so the caller sees them in tracing logs even when no error blocks
/// execution.
fn ensure_no_errors(report: &ValidationReport) -> OxResult<()> {
    if !report.has_errors() {
        return Ok(());
    }
    let mut lines = String::from("Query validation failed:");
    for issue in report.errors() {
        lines.push_str("\n  [");
        lines.push_str(&issue.validator_name);
        lines.push_str("] ");
        lines.push_str(&issue.message);
    }
    Err(OxError::Validation {
        field: "cypher_query".to_string(),
        message: lines,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use ox_core::i18n::LocalizedText;
    use ox_core::ontology_ir::{Cardinality, EdgeTypeDef, NodeTypeDef, PropertyDef};
    use ox_core::types::PropertyType;
    use uuid::Uuid;

    use crate::isolation::PropertyStrategy;

    fn ws_scope<F, R>(body: F) -> R
    where
        F: FnOnce() -> R,
    {
        GRAPH_WORKSPACE_ID.sync_scope(Uuid::nil(), body)
    }

    fn with_ontology<F, R>(ontology: OntologyIR, body: F) -> R
    where
        F: FnOnce() -> R,
    {
        GRAPH_ONTOLOGY.sync_scope(Arc::new(ontology), body)
    }

    fn person_company_ontology() -> OntologyIR {
        OntologyIR::new(
            "ont1".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Person".into(),
                    description: LocalizedText::default(),
                    properties: vec![PropertyDef {
                        id: "p1".into(),
                        name: "name".into(),
                        property_type: PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    }],
                    constraints: vec![],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Company".into(),
                    description: LocalizedText::default(),
                    properties: vec![],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "WORKS_AT".into(),
                description: LocalizedText::default(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
                ..Default::default()
            }],
            vec![],
        )
    }

    #[test]
    fn passes_when_query_is_safe_and_ontology_clean() {
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let result = ws_scope(|| {
            with_ontology(person_company_ontology(), || {
                run_pre_execute(
                    Some(&strategy as &dyn GraphIsolationStrategy),
                    "MATCH (p:Person) RETURN p",
                    &params,
                )
            })
        });
        let (rewritten, _) = result.expect("valid query must pass");
        assert!(rewritten.contains("_workspace_id = $_ws_id"));
    }

    #[test]
    fn rejects_unrestricted_delete_before_rewriting() {
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let err = ws_scope(|| {
            run_pre_execute(
                Some(&strategy as &dyn GraphIsolationStrategy),
                "MATCH (n:Person) DELETE n",
                &params,
            )
        })
        .expect_err("must fail safety gate");
        match err {
            OxError::Validation { message, .. } => {
                assert!(message.contains("DELETE"), "{message}");
                assert!(message.contains("safety"), "{message}");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_label_when_ontology_present() {
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let err = ws_scope(|| {
            with_ontology(person_company_ontology(), || {
                run_pre_execute(
                    Some(&strategy as &dyn GraphIsolationStrategy),
                    "MATCH (u:Userr) RETURN u",
                    &params,
                )
            })
        })
        .expect_err("must fail ontology gate");
        match err {
            OxError::Validation { message, .. } => {
                assert!(message.contains("Userr"), "{message}");
                assert!(message.contains("ontology"), "{message}");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn skips_ontology_validation_without_task_local() {
        // Unknown label passes when no ontology is in scope — internal
        // paths (search, profiler) rely on this.
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let result = ws_scope(|| {
            run_pre_execute(
                Some(&strategy as &dyn GraphIsolationStrategy),
                "MATCH (u:AnythingGoes) RETURN u",
                &params,
            )
        });
        assert!(result.is_ok(), "no ontology → skip ontology check");
    }

    #[test]
    fn aggregates_safety_and_ontology_errors_in_one_message() {
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let err = ws_scope(|| {
            with_ontology(person_company_ontology(), || {
                run_pre_execute(
                    Some(&strategy as &dyn GraphIsolationStrategy),
                    "MATCH (u:Userr) DELETE u",
                    &params,
                )
            })
        })
        .expect_err("both gates must fail");
        match err {
            OxError::Validation { message, .. } => {
                assert!(message.contains("DELETE"), "safety issue present: {message}");
                assert!(message.contains("Userr"), "ontology issue present: {message}");
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn system_bypass_skips_rewrite_and_scope_gate() {
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let result = ws_scope(|| {
            GRAPH_SYSTEM_BYPASS.sync_scope(true, || {
                run_pre_execute(
                    Some(&strategy as &dyn GraphIsolationStrategy),
                    "MATCH (p:Person) RETURN p",
                    &params,
                )
            })
        });
        let (rewritten, merged) = result.expect("system bypass must pass");
        assert_eq!(rewritten, "MATCH (p:Person) RETURN p");
        assert!(merged.is_empty(), "no scope param injected under bypass");
    }

    #[test]
    fn log_non_errors_does_not_block_execution() {
        // A ValidationReport carrying only Warnings + Infos should not
        // convert into an OxError. `ensure_no_errors` is what gates the
        // pipeline, and `log_non_errors` is a side-effect emitter —
        // double-check the split here so a future validator that emits
        // non-Error issues doesn't silently break execution.
        use crate::cypher::{ValidationIssue, ValidationReport};

        let report = ValidationReport {
            issues: vec![
                ValidationIssue::warning("hypothetical-advisory", "use LIMIT for big reads"),
                ValidationIssue::info("hypothetical-hint", "consider indexing :Person(name)"),
            ],
        };
        // log_non_errors is internal — call it directly to ensure it
        // tolerates a non-empty Warning / Info set without panicking.
        log_non_errors(&report, "pre-rewrite", "ws-123");
        // And the error gate sees no Errors so execution continues.
        assert!(ensure_no_errors(&report).is_ok());
    }

    #[test]
    fn post_rewrite_scope_gate_passes_after_rewriter_injection() {
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let (rewritten, _) = ws_scope(|| {
            run_pre_execute(
                Some(&strategy as &dyn GraphIsolationStrategy),
                "MATCH (p:Person) RETURN p",
                &params,
            )
        })
        .expect("valid query must pass");
        assert!(rewritten.contains("_workspace_id"));
    }
}
