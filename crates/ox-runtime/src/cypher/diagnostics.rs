//! Post-execution advisory diagnostics for Cypher.
//!
//! The runtime's [`bolt::pipeline::run_pre_execute`](crate::bolt) wires
//! `ComplexityValidator::permissive()` and `SemanticGuardValidator`
//! into the gate that guards every query. "Permissive" keeps
//! exploratory queries flowing — power users asked for `-[*]->` on an
//! ad-hoc traversal shouldn't be blocked.
//!
//! What the permissive pass *does not* do is surface the complexity
//! warnings back to the caller. This module re-runs the same two
//! validators in **strict** mode against the compiled Cypher and
//! returns the issues as structured [`QueryDiagnostic`] values
//! suitable for both:
//!
//! - `QueryMetadata.warnings` on the HTTP response envelope (rendered
//!   by the frontend `ResponseBasis` panel), and
//! - the agent `query_graph` tool's structured `warnings` field plus
//!   the `guidance` tail the LLM reads on the next turn.
//!
//! Extracting the strict-pass into one helper keeps both egress paths
//! aligned: a future validator or formatting change lands in one
//! place rather than drifting across the HTTP and agent call sites.
//! Returning structured values (not pre-formatted strings) lets UI
//! consumers filter by `level` or `validator` without inventing a
//! parse-the-string contract.

use ox_core::error::{OxError, OxResult};
use ox_query_ir::query::{DiagnosticLevel, QueryDiagnostic};

use crate::cypher::{
    ComplexityValidator, CypherValidatorPipeline, IssueLevel, SemanticGuardValidator,
    ValidateContext, parse,
};

/// Pre-execute blocking gate built on the same advisory validators
/// as [`strict_advisory_diagnostics`]. Returns
/// `Err(OxError::Validation)` when any issue fires at `Error`
/// level — the routes layer wires this into the query handlers so a
/// Cartesian product or destructive-write smell never reaches the
/// driver. Other levels (`Warning`, `Info`) flow through to the
/// post-execute advisory channel without blocking.
///
/// Pure pre-execute pass: blank `cypher` is treated as "nothing to
/// gate" and yields `Ok(())`.
pub fn strict_blocking_gate(cypher: &str, workspace_id: &str) -> OxResult<()> {
    if cypher.trim().is_empty() {
        return Ok(());
    }
    let ast = parse(cypher);
    let report = CypherValidatorPipeline::new()
        .with(ComplexityValidator::new())
        .with(SemanticGuardValidator::new())
        .run_ast(&ast, &ValidateContext::new(workspace_id));
    let blocking: Vec<_> = report
        .issues
        .into_iter()
        .filter(|i| i.level == IssueLevel::Error)
        .collect();
    if blocking.is_empty() {
        return Ok(());
    }
    let aggregated = blocking
        .iter()
        .map(|i| format!("[{}] {}", i.validator_name, i.message))
        .collect::<Vec<_>>()
        .join("\n");
    Err(OxError::Validation {
        field: "cypher_query".to_string(),
        message: format!("Query rejected by complexity/safety gate:\n{aggregated}"),
    })
}

/// Run the advisory validators (complexity + semantic-guard) against
/// `cypher` in strict mode and return one [`QueryDiagnostic`] per
/// non-`Info` issue. Empty when `cypher` is blank or no issue fires.
/// Never rejects — the runtime already executed the query.
///
/// `Info`-level issues are filtered out: they're author-facing hints
/// ("consider a LIMIT") that tend to be noisy on every legitimate
/// query and haven't proven their cost-benefit yet. Callers that want
/// them can re-run the pipeline locally; keeping the filter here
/// gives every caller the same signal-to-noise ratio.
pub fn strict_advisory_diagnostics(cypher: &str, workspace_id: &str) -> Vec<QueryDiagnostic> {
    if cypher.trim().is_empty() {
        return Vec::new();
    }
    let ast = parse(cypher);
    let report = CypherValidatorPipeline::new()
        .with(ComplexityValidator::new())
        .with(SemanticGuardValidator::new())
        .run_ast(&ast, &ValidateContext::new(workspace_id));
    report
        .issues
        .into_iter()
        .filter(|i| i.level != IssueLevel::Info)
        .map(|i| QueryDiagnostic {
            validator: i.validator_name,
            level: match i.level {
                IssueLevel::Error => DiagnosticLevel::Error,
                IssueLevel::Warning => DiagnosticLevel::Warning,
                IssueLevel::Info => DiagnosticLevel::Info,
            },
            message: i.message,
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_cypher_returns_empty_vec() {
        assert!(strict_advisory_diagnostics("", "ws-1").is_empty());
        assert!(strict_advisory_diagnostics("   \n\t  ", "ws-1").is_empty());
    }

    #[test]
    fn benign_query_returns_empty_vec() {
        let diags = strict_advisory_diagnostics(
            "MATCH (n:Person) WHERE n.id = 1 RETURN n LIMIT 10",
            "ws-1",
        );
        assert!(diags.is_empty(), "clean query should produce no diagnostics: {diags:?}");
    }

    #[test]
    fn unbounded_var_length_produces_complexity_diagnostic() {
        let diags = strict_advisory_diagnostics(
            "MATCH (a)-[*]->(b) RETURN a, b LIMIT 10",
            "ws-1",
        );
        assert!(!diags.is_empty(), "unbounded var-length must emit a diagnostic");
        assert!(
            diags.iter().any(|d| d.validator == "complexity"),
            "diagnostic should be tagged with the complexity validator: {diags:?}"
        );
    }

    #[test]
    fn tautological_delete_produces_semantic_guard_diagnostic() {
        let diags = strict_advisory_diagnostics(
            "MATCH (n) WHERE n.id = n.id DELETE n",
            "ws-1",
        );
        assert!(
            diags.iter().any(|d| d.validator == "semantic-guard"),
            "self-ref tautology + delete must produce a semantic-guard diagnostic: {diags:?}"
        );
    }

    #[test]
    fn info_level_is_filtered_but_warning_and_error_survive() {
        // Today's validators emit only Warning/Error; the assertion
        // pins the *filter semantics* rather than an exhaustive
        // sample. A future Info-level advisory must not reach the
        // caller without an explicit opt-in.
        let diags = strict_advisory_diagnostics(
            "MATCH (a)-[*]->(b) RETURN a, b",
            "ws-1",
        );
        for d in &diags {
            assert_ne!(d.level, DiagnosticLevel::Info, "info should be filtered");
        }
    }

    #[test]
    fn level_serializes_as_lowercase_json() {
        // Wire-stable invariant — clients string-compare on
        // "warning" / "info" / "error", not Rust enum text.
        let qd = QueryDiagnostic {
            validator: "complexity".to_string(),
            level: DiagnosticLevel::Warning,
            message: ox_core::diagnostic::diag("test.fixture").message("test"),
        };
        let json = serde_json::to_string(&qd).unwrap();
        assert!(json.contains("\"level\":\"warning\""), "{json}");
    }

    #[test]
    fn blocking_gate_passes_benign_query() {
        let result = strict_blocking_gate(
            "MATCH (n:Person) WHERE n.id = 1 RETURN n LIMIT 10",
            "ws-1",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn blocking_gate_passes_empty_query() {
        assert!(strict_blocking_gate("", "ws-1").is_ok());
    }

    #[test]
    fn blocking_gate_rejects_when_validator_emits_error_level() {
        // The semantic-guard validator emits Error level on a
        // tautological-WHERE + DELETE combo (would delete every node).
        let result = strict_blocking_gate(
            "MATCH (n) WHERE n.id = n.id DELETE n",
            "ws-1",
        );
        assert!(
            result.is_err(),
            "tautological delete should be blocked: {result:?}"
        );
        match result.unwrap_err() {
            ox_core::error::OxError::Validation { field, message } => {
                assert_eq!(field, "cypher_query");
                assert!(
                    message.contains("complexity") || message.contains("semantic-guard"),
                    "rejection should name the validator: {message}"
                );
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }
}
