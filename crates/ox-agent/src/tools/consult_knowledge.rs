use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_store::KnowledgeStore;

// ---------------------------------------------------------------------------
// ConsultKnowledgeTool — search learned corrections and hints
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConsultKnowledgeInput {
    /// Search query: natural language description of what you need to know.
    pub query: String,
    /// Optional filter: "correction" or "hint".
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConsultKnowledgeOutput {
    entries: Vec<KnowledgeHitEntry>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct KnowledgeHitEntry {
    id: String,
    kind: String,
    title: String,
    content: String,
    confidence: f64,
    affected_labels: Vec<String>,
}

pub struct ConsultKnowledgeTool {
    pub knowledge_store: Arc<dyn KnowledgeStore>,
    pub ontology_name: Option<String>,
    pub ontology_version: Option<i32>,
}

#[async_trait]
impl SchemaTool for ConsultKnowledgeTool {
    type Input = ConsultKnowledgeInput;
    const NAME: &'static str = super::CONSULT_KNOWLEDGE;
    const DESCRIPTION: &'static str = "Search the workspace knowledge base for learned corrections from past query failures \
         and admin-created hints. Use before complex queries to check if there are known \
         pitfalls or recommended approaches for this ontology.";
    const READ_ONLY: bool = true;

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let ontology_name = match &self.ontology_name {
            Some(name) => name.as_str(),
            None => return ToolResult::error("No ontology context available"),
        };
        let version = self.ontology_version.unwrap_or(1);

        // Extract labels from query for label-based search
        // Use words that could be labels (PascalCase or non-ASCII like Korean)
        let possible_labels: Vec<&str> = input
            .query
            .split_whitespace()
            .filter(|w| {
                w.chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase() || !c.is_ascii())
            })
            .collect();

        let kinds: Vec<&str> = match input.kind.as_deref() {
            Some(k) => vec![k],
            None => vec!["correction", "hint"],
        };

        // Try label-based search first, then filter by kind
        let mut entries = if !possible_labels.is_empty() {
            let mut results = self
                .knowledge_store
                .search_knowledge_by_labels(ontology_name, version, &possible_labels, 20)
                .await
                .unwrap_or_default();
            // Apply kind filter post-query
            results.retain(|e| kinds.contains(&e.kind.as_str()));
            results.truncate(10);
            results
        } else {
            vec![]
        };

        // Fallback to list_active_knowledge if label search returned nothing
        if entries.is_empty() {
            entries = self
                .knowledge_store
                .list_active_knowledge(ontology_name, version, &kinds, 10)
                .await
                .unwrap_or_default();
        }

        // Record usage
        let ids: Vec<uuid::Uuid> = entries.iter().map(|e| e.id).collect();
        let _ = self.knowledge_store.record_knowledge_usage(&ids).await;

        let hits: Vec<KnowledgeHitEntry> = entries
            .into_iter()
            .map(|e| KnowledgeHitEntry {
                id: e.id.to_string(),
                kind: e.kind,
                title: e.title,
                content: e.content,
                confidence: e.confidence,
                affected_labels: e.affected_labels,
            })
            .collect();

        let total = hits.len();
        let output = ConsultKnowledgeOutput {
            entries: hits,
            total,
        };
        ToolResult::success(serde_json::to_string_pretty(&output).unwrap_or_default())
    }
}
