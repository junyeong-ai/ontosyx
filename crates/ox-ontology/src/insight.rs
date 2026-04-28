//! `InsightHint` — a proactive hint generated from ontology
//! structure, surfaced by the brain / agent stack as a conversational
//! starting point.
//!
//! Persisted insight artefacts (saved by the user, re-runnable
//! against future ontology versions) live in
//! `ox_query_ir::insight::InsightDef` — they reach into `QueryIR` +
//! `QueryProvenance`, which sit in `ox-query-ir`, and would otherwise
//! force a layering inversion if they lived here.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A proactive insight suggestion generated from ontology structure.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct InsightHint {
    /// Natural language question a data analyst would ask.
    pub question: String,
    /// Category: "trend", "distribution", "anomaly", "relationship", "summary".
    pub category: String,
    /// Suggested tool: "query_graph" or "execute_analysis".
    pub suggested_tool: String,
}
