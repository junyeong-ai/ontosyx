//! Evaluation case definitions for NL-to-QueryIR translation quality.

use crate::ontology_ir::OntologyIR;
use serde::{Deserialize, Serialize};

/// A single evaluation case for the NL-to-QueryIR pipeline.
#[derive(Debug, Clone)]
pub struct EvalCase {
    /// Unique identifier for this test case.
    pub id: String,
    /// Category of the evaluation (determines which aspects to validate).
    pub category: EvalCategory,
    /// The natural language question to translate.
    pub question: String,
    /// The ontology to translate against.
    pub ontology: OntologyIR,
    /// Expected top-level operation type.
    pub expected_op: ExpectedOp,
    /// Node labels that MUST appear in the generated QueryIR patterns.
    pub expected_node_labels: Vec<String>,
    /// Edge labels that MUST appear in the generated QueryIR patterns.
    pub expected_edge_labels: Vec<String>,
    /// Human-readable description of what this case tests.
    pub description: String,
}

/// Category of evaluation case — groups cases for per-category pass rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalCategory {
    /// Simple single-node MATCH queries.
    SimpleMatch,
    /// Queries traversing one or more relationships.
    RelationshipTraversal,
    /// Queries using aggregation functions (count, sum, avg, etc.).
    Aggregation,
    /// Path-finding queries (shortest path, all paths).
    PathFinding,
    /// Queries with multiple filter conditions.
    MultiFilter,
    /// Top-N queries with ordering.
    TopN,
    /// Multi-step queries requiring Chain or complex patterns.
    MultiStep,
    /// Edge cases: queries that can't be answered, ambiguous queries, etc.
    EdgeCase,
}

impl std::fmt::Display for EvalCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimpleMatch => write!(f, "Simple Match"),
            Self::RelationshipTraversal => write!(f, "Relationship Traversal"),
            Self::Aggregation => write!(f, "Aggregation"),
            Self::PathFinding => write!(f, "Path Finding"),
            Self::MultiFilter => write!(f, "Multi-Filter"),
            Self::TopN => write!(f, "Top-N"),
            Self::MultiStep => write!(f, "Multi-Step"),
            Self::EdgeCase => write!(f, "Edge Case"),
        }
    }
}

/// Expected top-level operation type for an eval case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedOp {
    Match,
    PathFind,
    Aggregate,
    Union,
    Chain,
    Mutate,
}

impl std::fmt::Display for ExpectedOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Match => write!(f, "match"),
            Self::PathFind => write!(f, "path_find"),
            Self::Aggregate => write!(f, "aggregate"),
            Self::Union => write!(f, "union"),
            Self::Chain => write!(f, "chain"),
            Self::Mutate => write!(f, "mutate"),
        }
    }
}
