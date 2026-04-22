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

use crate::cypher::{
    ComplexityValidator, CypherValidatorPipeline, IssueLevel, SemanticGuardValidator,
    ValidateContext, parse,
};
use ox_query_ir::query::{DiagnosticLevel, QueryDiagnostic};

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
        let diag = QueryDiagnostic {
            validator: "complexity".to_string(),
            level: DiagnosticLevel::Warning,
            message: "test".to_string(),
        };
        let json = serde_json::to_string(&diag).unwrap();
        assert!(json.contains("\"level\":\"warning\""), "{json}");
    }
}
