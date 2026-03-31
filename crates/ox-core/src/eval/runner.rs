//! Evaluation runner — executes eval cases and produces quality reports.
//!
//! The runner is decoupled from specific Brain/Compiler implementations via
//! async closures, so it can be used from integration tests or CLI tools
//! without ox-core depending on ox-brain or ox-compiler.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use crate::error::OxResult;
use crate::ontology_ir::OntologyIR;
use crate::query_ir::{GraphPattern, PathElement, QueryIR, QueryOp};

use super::cases::{EvalCase, EvalCategory, ExpectedOp};

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Result of evaluating a single case.
#[derive(Debug, Clone)]
pub struct EvalResult {
    /// Which case this result belongs to.
    pub case_id: String,
    /// Category of the case.
    pub category: EvalCategory,
    /// Whether the case passed all checks.
    pub passed: bool,
    /// Whether the operation type matched the expected type.
    pub operation_match: bool,
    /// Whether all expected node labels were found in the QueryIR.
    pub node_labels_match: bool,
    /// Whether all expected edge labels were found in the QueryIR.
    pub edge_labels_match: bool,
    /// The generated QueryIR (if translation succeeded).
    pub generated_query_ir: Option<QueryIR>,
    /// Whether the generated QueryIR compiled successfully.
    pub compilation_success: bool,
    /// Error message (if any step failed).
    pub error: Option<String>,
    /// Translation latency in milliseconds.
    pub latency_ms: u64,
}

/// Aggregated result for a single category.
#[derive(Debug, Clone)]
pub struct CategoryResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
}

/// Summary of all evaluation results.
#[derive(Debug, Clone)]
pub struct EvalSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
    pub by_category: HashMap<EvalCategory, CategoryResult>,
    pub avg_latency_ms: u64,
    pub results: Vec<EvalResult>,
}

impl std::fmt::Display for EvalSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Evaluation Summary ===")?;
        writeln!(
            f,
            "Total: {} | Passed: {} | Failed: {} | Pass Rate: {:.1}%",
            self.total,
            self.passed,
            self.failed,
            self.pass_rate * 100.0
        )?;
        writeln!(f, "Average Latency: {}ms", self.avg_latency_ms)?;
        writeln!(f)?;

        // Sort categories for deterministic output
        let mut cats: Vec<_> = self.by_category.iter().collect();
        cats.sort_by_key(|(cat, _)| format!("{cat}"));

        writeln!(f, "--- By Category ---")?;
        for (cat, result) in &cats {
            writeln!(
                f,
                "  {:<25} {}/{} ({:.0}%)",
                format!("{cat}"),
                result.passed,
                result.total,
                result.pass_rate * 100.0
            )?;
        }

        // Show failed cases
        let failures: Vec<_> = self.results.iter().filter(|r| !r.passed).collect();
        if !failures.is_empty() {
            writeln!(f)?;
            writeln!(f, "--- Failed Cases ---")?;
            for r in &failures {
                write!(f, "  [{}] ", r.case_id)?;
                if !r.operation_match {
                    write!(f, "OP_MISMATCH ")?;
                }
                if !r.node_labels_match {
                    write!(f, "NODE_LABELS ")?;
                }
                if !r.edge_labels_match {
                    write!(f, "EDGE_LABELS ")?;
                }
                if !r.compilation_success {
                    write!(f, "COMPILE_FAIL ")?;
                }
                if let Some(err) = &r.error {
                    write!(f, "| {err}")?;
                }
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

/// Type alias for the translate function.
pub type TranslateFn = Box<
    dyn Fn(String, OntologyIR) -> Pin<Box<dyn Future<Output = OxResult<QueryIR>> + Send>>
        + Send
        + Sync,
>;

/// Type alias for the compile function.
pub type CompileFn = Box<dyn Fn(&QueryIR) -> OxResult<String> + Send + Sync>;

/// Evaluation runner that tests NL-to-QueryIR translation quality.
///
/// Decoupled from Brain/Compiler via closures, making it usable from any crate.
pub struct EvalRunner {
    translate: TranslateFn,
    compile: CompileFn,
}

impl EvalRunner {
    /// Create a new eval runner with translation and compilation functions.
    ///
    /// # Arguments
    /// * `translate` - Async function: (question, ontology) -> QueryIR
    /// * `compile` - Function: QueryIR -> compiled query string (for validation)
    pub fn new(translate: TranslateFn, compile: CompileFn) -> Self {
        Self { translate, compile }
    }

    /// Run all evaluation cases and produce a summary.
    pub async fn run_all(&self, cases: &[EvalCase]) -> EvalSummary {
        let mut results = Vec::with_capacity(cases.len());

        for case in cases {
            let result = self.run_one(case).await;
            results.push(result);
        }

        Self::summarize(results)
    }

    /// Run a single evaluation case.
    async fn run_one(&self, case: &EvalCase) -> EvalResult {
        let start = Instant::now();

        // Step 1: Translate
        let translate_result = (self.translate)(case.question.clone(), case.ontology.clone()).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        let query_ir = match translate_result {
            Ok(qir) => qir,
            Err(e) => {
                return EvalResult {
                    case_id: case.id.clone(),
                    category: case.category,
                    passed: false,
                    operation_match: false,
                    node_labels_match: false,
                    edge_labels_match: false,
                    generated_query_ir: None,
                    compilation_success: false,
                    error: Some(format!("Translation failed: {e}")),
                    latency_ms,
                };
            }
        };

        // Step 2: Check operation type
        let operation_match = check_operation_type(&query_ir.operation, case.expected_op);

        // Step 3: Check node labels
        let actual_node_labels = extract_node_labels(&query_ir);
        let node_labels_match = if case.expected_node_labels.is_empty() {
            // For edge cases where no specific labels are expected, pass if translation succeeded
            true
        } else {
            case.expected_node_labels
                .iter()
                .all(|expected| actual_node_labels.contains(expected))
        };

        // Step 4: Check edge labels
        let actual_edge_labels = extract_edge_labels(&query_ir);
        let edge_labels_match = if case.expected_edge_labels.is_empty() {
            true
        } else {
            case.expected_edge_labels
                .iter()
                .all(|expected| actual_edge_labels.contains(expected))
        };

        // Step 5: Try to compile
        let compilation_success = (self.compile)(&query_ir).is_ok();

        let passed =
            operation_match && node_labels_match && edge_labels_match && compilation_success;

        EvalResult {
            case_id: case.id.clone(),
            category: case.category,
            passed,
            operation_match,
            node_labels_match,
            edge_labels_match,
            generated_query_ir: Some(query_ir),
            compilation_success,
            error: None,
            latency_ms,
        }
    }

    /// Aggregate individual results into a summary.
    fn summarize(results: Vec<EvalResult>) -> EvalSummary {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let pass_rate = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };

        let total_latency: u64 = results.iter().map(|r| r.latency_ms).sum();
        let avg_latency_ms = if total > 0 {
            total_latency / total as u64
        } else {
            0
        };

        // Group by category
        let mut by_category: HashMap<EvalCategory, (usize, usize)> = HashMap::new();
        for r in &results {
            let entry = by_category.entry(r.category).or_insert((0, 0));
            entry.0 += 1; // total
            if r.passed {
                entry.1 += 1; // passed
            }
        }

        let by_category = by_category
            .into_iter()
            .map(|(cat, (t, p))| {
                (
                    cat,
                    CategoryResult {
                        total: t,
                        passed: p,
                        failed: t - p,
                        pass_rate: if t > 0 { p as f64 / t as f64 } else { 0.0 },
                    },
                )
            })
            .collect();

        EvalSummary {
            total,
            passed,
            failed,
            pass_rate,
            by_category,
            avg_latency_ms,
            results,
        }
    }
}

// ---------------------------------------------------------------------------
// QueryIR inspection helpers
// ---------------------------------------------------------------------------

/// Check if the operation type matches the expected type.
fn check_operation_type(op: &QueryOp, expected: ExpectedOp) -> bool {
    match (op, expected) {
        (QueryOp::Match { .. }, ExpectedOp::Match) => true,
        (QueryOp::PathFind { .. }, ExpectedOp::PathFind) => true,
        (QueryOp::Aggregate { .. }, ExpectedOp::Aggregate) => true,
        (QueryOp::Union { .. }, ExpectedOp::Union) => true,
        (QueryOp::Chain { .. }, ExpectedOp::Chain) => true,
        (QueryOp::Mutate { .. }, ExpectedOp::Mutate) => true,
        // Also allow Match when Aggregate is expected (the prompt may produce
        // aggregation as Match with aggregation projections + group_by)
        (QueryOp::Match { .. }, ExpectedOp::Aggregate) => true,
        // Allow Match when Chain is expected (simpler models might produce
        // a single Match with all patterns)
        (QueryOp::Match { .. }, ExpectedOp::Chain) => true,
        _ => false,
    }
}

/// Extract all node labels referenced in the QueryIR.
pub fn extract_node_labels(query: &QueryIR) -> Vec<String> {
    let mut labels = Vec::new();
    extract_node_labels_from_op(&query.operation, &mut labels);
    labels.sort();
    labels.dedup();
    labels
}

fn extract_node_labels_from_op(op: &QueryOp, labels: &mut Vec<String>) {
    match op {
        QueryOp::Match { patterns, .. } => {
            for p in patterns {
                extract_node_labels_from_pattern(p, labels);
            }
        }
        QueryOp::PathFind { start, end, .. } => {
            if let Some(label) = &start.label {
                labels.push(label.clone());
            }
            if let Some(label) = &end.label {
                labels.push(label.clone());
            }
        }
        QueryOp::Aggregate { source, .. } => {
            extract_node_labels_from_op(&source.operation, labels);
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                extract_node_labels_from_op(&q.operation, labels);
            }
        }
        QueryOp::Chain { steps } => {
            for step in steps {
                extract_node_labels_from_op(&step.operation, labels);
            }
        }
        QueryOp::Mutate {
            context,
            operations,
            ..
        } => {
            if let Some(ctx) = context {
                extract_node_labels_from_op(ctx, labels);
            }
            for op in operations {
                match op {
                    crate::query_ir::MutateOp::CreateNode { label, .. }
                    | crate::query_ir::MutateOp::MergeNode { label, .. } => {
                        labels.push(label.clone());
                    }
                    _ => {}
                }
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            extract_node_labels_from_op(&inner.operation, labels);
        }
        QueryOp::Analytics { source, .. } => match source {
            crate::query_ir::AnalyticsSource::Labels { labels: src_labels } => {
                labels.extend(src_labels.iter().cloned());
            }
            crate::query_ir::AnalyticsSource::Subgraph { filter } => {
                extract_node_labels_from_op(filter, labels);
            }
            crate::query_ir::AnalyticsSource::WholeGraph => {}
        },
    }
}

fn extract_node_labels_from_pattern(pattern: &GraphPattern, labels: &mut Vec<String>) {
    match pattern {
        GraphPattern::Node { label, .. } => {
            if let Some(l) = label {
                labels.push(l.clone());
            }
        }
        GraphPattern::Relationship { .. } => {
            // Relationship patterns reference node variables, not labels directly.
            // Node labels come from the Node patterns in the same MATCH clause.
        }
        GraphPattern::Path { elements } => {
            for elem in elements {
                if let PathElement::Node { label, .. } = elem
                    && let Some(l) = label
                {
                    labels.push(l.clone());
                }
            }
        }
    }
}

/// Extract all edge labels referenced in the QueryIR.
pub fn extract_edge_labels(query: &QueryIR) -> Vec<String> {
    let mut labels = Vec::new();
    extract_edge_labels_from_op(&query.operation, &mut labels);
    labels.sort();
    labels.dedup();
    labels
}

fn extract_edge_labels_from_op(op: &QueryOp, labels: &mut Vec<String>) {
    match op {
        QueryOp::Match { patterns, .. } => {
            for p in patterns {
                extract_edge_labels_from_pattern(p, labels);
            }
        }
        QueryOp::PathFind { edge_types, .. } => {
            labels.extend(edge_types.iter().cloned());
        }
        QueryOp::Aggregate { source, .. } => {
            extract_edge_labels_from_op(&source.operation, labels);
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                extract_edge_labels_from_op(&q.operation, labels);
            }
        }
        QueryOp::Chain { steps } => {
            for step in steps {
                extract_edge_labels_from_op(&step.operation, labels);
            }
        }
        QueryOp::Mutate {
            context,
            operations,
            ..
        } => {
            if let Some(ctx) = context {
                extract_edge_labels_from_op(ctx, labels);
            }
            for op in operations {
                match op {
                    crate::query_ir::MutateOp::CreateEdge { label, .. }
                    | crate::query_ir::MutateOp::MergeEdge { label, .. } => {
                        labels.push(label.clone());
                    }
                    _ => {}
                }
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            extract_edge_labels_from_op(&inner.operation, labels);
        }
        QueryOp::Analytics { source, .. } => {
            if let crate::query_ir::AnalyticsSource::Subgraph { filter } = source {
                extract_edge_labels_from_op(filter, labels);
            }
        }
    }
}

fn extract_edge_labels_from_pattern(pattern: &GraphPattern, labels: &mut Vec<String>) {
    match pattern {
        GraphPattern::Relationship { label, .. } => {
            if let Some(l) = label {
                labels.push(l.clone());
            }
        }
        GraphPattern::Path { elements } => {
            for elem in elements {
                if let PathElement::Edge { label, .. } = elem
                    && let Some(l) = label
                {
                    labels.push(l.clone());
                }
            }
        }
        GraphPattern::Node { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::*;
    use crate::types::Direction;

    #[test]
    fn test_extract_node_labels_simple_match() {
        let qir = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "c".into(),
                    label: Some("Customer".into()),
                    property_filters: vec![],
                }],
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };
        assert_eq!(extract_node_labels(&qir), vec!["Customer".to_string()]);
    }

    #[test]
    fn test_extract_edge_labels_relationship() {
        let qir = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: "c".into(),
                        label: Some("Customer".into()),
                        property_filters: vec![],
                    },
                    GraphPattern::Relationship {
                        variable: Some("r".into()),
                        label: Some("PLACED".into()),
                        source: "c".into(),
                        target: "o".into(),
                        direction: Direction::Outgoing,
                        property_filters: vec![],
                        var_length: None,
                    },
                    GraphPattern::Node {
                        variable: "o".into(),
                        label: Some("Order".into()),
                        property_filters: vec![],
                    },
                ],
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };
        assert_eq!(extract_edge_labels(&qir), vec!["PLACED".to_string()]);
        assert_eq!(
            extract_node_labels(&qir),
            vec!["Customer".to_string(), "Order".to_string()]
        );
    }

    #[test]
    fn test_check_operation_type() {
        let match_op = QueryOp::Match {
            patterns: vec![],
            filter: None,
            projections: vec![],
            optional: false,
            group_by: vec![],
        };
        assert!(check_operation_type(&match_op, ExpectedOp::Match));
        assert!(check_operation_type(&match_op, ExpectedOp::Aggregate)); // allowed
        assert!(!check_operation_type(&match_op, ExpectedOp::PathFind));
    }

    #[test]
    fn test_summarize_empty() {
        let summary = EvalRunner::summarize(vec![]);
        assert_eq!(summary.total, 0);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.pass_rate, 0.0);
    }

    #[test]
    fn test_summary_display() {
        let summary = EvalSummary {
            total: 2,
            passed: 1,
            failed: 1,
            pass_rate: 0.5,
            by_category: HashMap::from([(
                EvalCategory::SimpleMatch,
                CategoryResult {
                    total: 2,
                    passed: 1,
                    failed: 1,
                    pass_rate: 0.5,
                },
            )]),
            avg_latency_ms: 100,
            results: vec![
                EvalResult {
                    case_id: "SM-01".into(),
                    category: EvalCategory::SimpleMatch,
                    passed: true,
                    operation_match: true,
                    node_labels_match: true,
                    edge_labels_match: true,
                    generated_query_ir: None,
                    compilation_success: true,
                    error: None,
                    latency_ms: 80,
                },
                EvalResult {
                    case_id: "SM-02".into(),
                    category: EvalCategory::SimpleMatch,
                    passed: false,
                    operation_match: true,
                    node_labels_match: false,
                    edge_labels_match: true,
                    generated_query_ir: None,
                    compilation_success: true,
                    error: None,
                    latency_ms: 120,
                },
            ],
        };
        let display = format!("{summary}");
        assert!(display.contains("Pass Rate: 50.0%"));
        assert!(display.contains("SM-02"));
        assert!(display.contains("NODE_LABELS"));
    }
}
