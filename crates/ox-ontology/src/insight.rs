//! `InsightSuggestion` — a proactive hint generated from ontology
//! structure, surfaced by the brain / agent stack as a conversational
//! starting point.
//!
//! Phase 3-B: moved out of `ox-core`'s `lib.rs` alongside the rest of
//! the domain model.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A proactive insight suggestion generated from ontology structure.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct InsightSuggestion {
    /// Natural language question a data analyst would ask.
    pub question: String,
    /// Category: "trend", "distribution", "anomaly", "relationship", "summary".
    pub category: String,
    /// Suggested tool: "query_graph" or "execute_analysis".
    pub suggested_tool: String,
}
