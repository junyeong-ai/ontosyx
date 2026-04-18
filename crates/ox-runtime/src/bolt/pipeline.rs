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
//! - **Parse once.** The incoming text is parsed into a `CypherAst` a
//!   single time; both validator pipelines and the rewriter operate on
//!   the AST directly. The final render happens after the post-rewrite
//!   gate passes. This matches the triple-surface design documented in
//!   `ox-runtime::cypher`: one parse feeds three consumers.
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
//!
//! - **No silent isolation bypass.** If a strategy declares a
//!   `scope_property` (i.e. workspace isolation must show up in the
//!   rewritten text) but no `GRAPH_WORKSPACE_ID` is bound and we're
//!   not in system-bypass mode, the pipeline refuses to execute. The
//!   previous version passed the query through unchanged in that
//!   edge case — a workspace-isolation failure masquerading as a
//!   no-op.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::ontology_ir::OntologyIR;
use ox_core::types::PropertyValue;

use crate::cypher::{
    CypherAst, CypherValidatorPipeline, OntologyValidator, SafetyValidator, ValidateContext,
    ValidationReport, parse,
};
use crate::isolation::{GraphIsolationStrategy, ScopedAst};
use crate::{GRAPH_ONTOLOGY, GRAPH_SYSTEM_BYPASS, GRAPH_WORKSPACE_ID};

/// Run the full pre-execute pipeline for a Cypher statement.
///
/// Steps (one parse, three passes):
/// 1. Validate (safety always; ontology iff `GRAPH_ONTOLOGY` is set).
/// 2. Apply the isolation strategy's `scope_ast` — a no-op for
///    `DatabaseStrategy`; injects workspace predicates for
///    `PropertyStrategy`.
/// 3. Validate the rewritten AST against the post-rewrite scope gate
///    (iff the strategy exposes a scope property).
///
/// The function is the single home for the per-request policy; both
/// Neo4j and Memgraph just delegate. A new Bolt backend gets safety,
/// ontology, and workspace-scope enforcement for free.
pub(crate) fn run_pre_execute(
    strategy: Option<&dyn GraphIsolationStrategy>,
    cypher: &str,
    params: &HashMap<String, PropertyValue>,
) -> OxResult<(String, HashMap<String, PropertyValue>)> {
    let workspace_id = GRAPH_WORKSPACE_ID.try_with(|id| id.to_string()).ok();
    let system_bypass = GRAPH_SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false);
    let ontology = GRAPH_ONTOLOGY.try_with(Arc::clone).ok();

    // Parse once; every pipeline pass below operates on the AST.
    let ast = parse(cypher);
    let ws_id_str = workspace_id.as_deref().unwrap_or("");

    // --- Step 1: pre-rewrite validation ----------------------------------
    run_pre_rewrite_validation(&ast, ws_id_str, ontology.as_deref())?;

    // --- Step 2: rewrite (workspace-scope) -------------------------------
    //
    // A strategy declaring `scope_property` requires workspace isolation.
    // Running without `GRAPH_WORKSPACE_ID` (outside system-bypass) would
    // silently leak across workspaces — refuse up front rather than
    // discovering it via the post-rewrite gate.
    let (scoped, merged_params) = if system_bypass {
        (
            ScopedAst {
                ast,
                params: Vec::new(),
                modified_statements: 0,
            },
            params.clone(),
        )
    } else if let Some(strategy) = strategy {
        match workspace_id.as_deref() {
            Some(ws) => {
                let scoped = strategy
                    .scope_ast(ast, ws)
                    .map_err(|e| OxError::Validation {
                        field: "cypher_query".to_string(),
                        message: format!("Query validation failed:\n  [rewrite] {e}"),
                    })?;
                let mut merged = params.clone();
                for (key, value) in &scoped.params {
                    // Strategy parameters are system-critical and must
                    // not be spoofable from the outside — the merge is
                    // "strategy wins". When a user-supplied param
                    // collides with a scope-injected one, that silent
                    // overwrite is exactly the guarantee we want, but
                    // it's also exactly the kind of invisible behavior
                    // that hides bugs. Emit a warning + metrics counter
                    // so operators can find the offending caller; the
                    // query still runs safely.
                    if params.contains_key(key) {
                        warn!(
                            strategy = strategy.name(),
                            param = %key,
                            workspace_id = ws,
                            "User-supplied parameter collided with a strategy-injected \
                             scope parameter; strategy value wins (strategy-wins merge)."
                        );
                    }
                    merged.insert(key.clone(), PropertyValue::String(value.clone()));
                }
                (scoped, merged)
            }
            None if strategy.scope_property().is_some() => {
                return Err(OxError::Runtime {
                    message: format!(
                        "Graph isolation strategy `{}` requires GRAPH_WORKSPACE_ID, \
                         but no workspace is bound. Set the workspace middleware or \
                         run under system-bypass.",
                        strategy.name()
                    ),
                });
            }
            None => (
                ScopedAst {
                    ast,
                    params: Vec::new(),
                    modified_statements: 0,
                },
                params.clone(),
            ),
        }
    } else {
        (
            ScopedAst {
                ast,
                params: Vec::new(),
                modified_statements: 0,
            },
            params.clone(),
        )
    };

    // --- Step 3: post-rewrite scope check --------------------------------
    //
    // Replaces the old substring-based `WorkspaceScopeValidator`. When a
    // strategy declares a `scope_property` and at least one statement
    // actually touches graph data, the rewriter's cumulative
    // `modified_statements` must be non-zero — otherwise the pass
    // silently skipped injection and the query would execute without
    // isolation. Scalar queries (`RETURN 1`) legitimately yield zero
    // modifications and no graph-touching statements, so they pass.
    if !system_bypass
        && let Some(prop) = strategy.and_then(|s| s.scope_property())
        && query_touches_graph(&scoped.ast)
        && scoped.modified_statements == 0
    {
        return Err(OxError::Validation {
            field: "cypher_query".to_string(),
            message: format!(
                "Query validation failed:\n  [workspace-scope] rewriter applied no isolation but the query touches graph data; expected injection of `{prop}`",
            ),
        });
    }

    Ok((scoped.ast.render(), merged_params))
}

/// Does the AST contain at least one statement that reads or writes
/// graph data? The post-rewrite isolation check uses this to decide
/// whether zero modifications is a bug (some statement needed scoping
/// and didn't get it) or benign (nothing to scope, e.g. `RETURN 1`).
fn query_touches_graph(ast: &CypherAst) -> bool {
    ast.statements.iter().any(|statement| {
        statement
            .clauses
            .iter()
            .any(|c| c.kind.has_patterns() || c.kind.is_write())
    })
}

fn run_pre_rewrite_validation(
    ast: &CypherAst,
    workspace_id: &str,
    ontology: Option<&OntologyIR>,
) -> OxResult<()> {
    let mut pipeline = CypherValidatorPipeline::new().with(SafetyValidator::new());
    if let Some(onto) = ontology {
        pipeline = pipeline.with(OntologyValidator::new(onto.clone()));
    }
    let report = pipeline.run_ast(ast, &ValidateContext::new(workspace_id));
    log_non_errors(&report, "pre-rewrite", workspace_id);
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

    use ox_core::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::ontology_ir::{Cardinality, EdgeTypeDef, NodeTypeDef, PropertyDef};
    use ox_core::types::PropertyType;
    use uuid::Uuid;

    use crate::isolation::{DatabaseStrategy, PropertyStrategy};

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label literal must be valid")
    }

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
                    label: gl("Person"),
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
                    label: gl("Company"),
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
                assert!(
                    message.contains("DELETE"),
                    "safety issue present: {message}"
                );
                assert!(
                    message.contains("Userr"),
                    "ontology issue present: {message}"
                );
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

    #[test]
    fn log_non_errors_does_not_block_execution() {
        use crate::cypher::{ValidationIssue, ValidationReport};

        let report = ValidationReport {
            issues: vec![
                ValidationIssue::warning("hypothetical-advisory", "use LIMIT for big reads"),
                ValidationIssue::info("hypothetical-hint", "consider indexing :Person(name)"),
            ],
        };
        log_non_errors(&report, "pre-rewrite", "ws-123");
        assert!(ensure_no_errors(&report).is_ok());
    }

    #[test]
    fn refuses_execution_when_strategy_requires_scope_but_no_workspace_id() {
        // PropertyStrategy declares scope_property = Some("_workspace_id").
        // Without GRAPH_WORKSPACE_ID bound and no system-bypass, passing
        // the query through unchanged would leak across workspaces.
        // The pipeline must refuse.
        let params = HashMap::new();
        let strategy = PropertyStrategy;
        let err = run_pre_execute(
            Some(&strategy as &dyn GraphIsolationStrategy),
            "MATCH (p:Person) RETURN p",
            &params,
        )
        .expect_err("missing GRAPH_WORKSPACE_ID must block property-strategy execution");
        match err {
            OxError::Runtime { message } => {
                assert!(
                    message.contains("GRAPH_WORKSPACE_ID"),
                    "error should mention the missing local: {message}"
                );
            }
            other => panic!("expected Runtime error, got {other:?}"),
        }
    }

    #[test]
    fn allows_execution_without_workspace_id_for_database_strategy() {
        // DatabaseStrategy doesn't inject into the text (isolation lives
        // at the connection layer), so missing GRAPH_WORKSPACE_ID is fine.
        let params = HashMap::new();
        let strategy = DatabaseStrategy;
        let result = run_pre_execute(
            Some(&strategy as &dyn GraphIsolationStrategy),
            "MATCH (p:Person) RETURN p",
            &params,
        );
        assert!(result.is_ok(), "database strategy passthrough: {result:?}");
    }

    #[test]
    fn user_supplied_scope_param_is_overwritten_by_strategy() {
        // PropertyStrategy injects `_ws_id` as its scope bind parameter.
        // A caller that supplies the same key — whether by mistake or
        // as an attempted spoof — must see the strategy value win
        // silently with respect to the query result, with the collision
        // visible in the logs (not tested here; verified manually by
        // operators from the `warn!` entry).
        let mut params = HashMap::new();
        params.insert(
            "_ws_id".to_string(),
            PropertyValue::String("user-supplied-bogus".to_string()),
        );
        let strategy = PropertyStrategy;
        let (_rewritten, merged) = ws_scope(|| {
            run_pre_execute(
                Some(&strategy as &dyn GraphIsolationStrategy),
                "MATCH (p:Person) RETURN p",
                &params,
            )
        })
        .expect("collision must not fail the request — strategy-wins merge");
        match merged.get("_ws_id") {
            Some(PropertyValue::String(s)) => {
                assert_ne!(
                    s, "user-supplied-bogus",
                    "user's value must NOT survive the merge"
                );
            }
            other => panic!("expected a String _ws_id, got {other:?}"),
        }
    }

    #[test]
    fn query_touches_graph_distinguishes_graph_vs_scalar() {
        assert!(query_touches_graph(&parse("MATCH (n) RETURN n")));
        assert!(query_touches_graph(&parse("CREATE (n:Person {name: 'x'})")));
        assert!(!query_touches_graph(&parse("RETURN 1")));
        assert!(!query_touches_graph(&parse("RETURN 'hello'")));
    }
}
