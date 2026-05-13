use std::sync::Arc;

use async_trait::async_trait;
use entelix::tools::ToolEffect;
use entelix::{AgentContext, SchemaTool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_store::KnowledgeStore;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ConsultKnowledgeInput {
    /// Search query: natural language description of what you need to know.
    pub query: String,
    /// Optional filter: "correction" or "hint".
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConsultKnowledgeOutput {
    entries: Vec<KnowledgeHitEntry>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct KnowledgeHitEntry {
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
    type Output = ConsultKnowledgeOutput;
    const NAME: &'static str = super::CONSULT_KNOWLEDGE;

    fn description(&self) -> &str {
        "Search the workspace knowledge base for learned corrections and admin hints. \
         Call before complex queries to surface known pitfalls for this ontology."
    }

    fn effect(&self) -> ToolEffect {
        ToolEffect::ReadOnly
    }

    async fn execute(
        &self,
        input: Self::Input,
        _ctx: &AgentContext<()>,
    ) -> entelix::Result<Self::Output> {
        let ontology_name = self
            .ontology_name
            .as_deref()
            .ok_or_else(|| entelix::Error::invalid_request("No ontology context available"))?;
        let version = self.ontology_version.unwrap_or(1);

        // Pull words from the query that *could* be node / edge labels.
        // PascalCase / non-ASCII (Korean, Japanese) starts qualify;
        // lowercase tokens are treated as keywords for the fallback
        // path rather than label candidates.
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

        // Label-based search first; falls back to active-knowledge
        // listing when no labels surface (or the label search misses).
        let mut entries = if !possible_labels.is_empty() {
            let mut results = self
                .knowledge_store
                .search_knowledge_by_labels(ontology_name, version, &possible_labels, 20)
                .await
                .unwrap_or_default();
            results.retain(|e| kinds.contains(&e.kind.as_str()));
            results.truncate(10);
            results
        } else {
            vec![]
        };

        if entries.is_empty() {
            entries = self
                .knowledge_store
                .list_active_knowledge(ontology_name, version, &kinds, 10)
                .await
                .unwrap_or_default();
        }

        // Surface usage so the FE quality dashboard sees coverage.
        // Telemetry — a DB blip should not fail the tool, but a silent
        // drop would hide outages, so the warn-log keeps the signal.
        let ids: Vec<uuid::Uuid> = entries.iter().map(|e| e.id).collect();
        if let Err(error) = self.knowledge_store.record_knowledge_usage(&ids).await {
            tracing::warn!(?error, hits = ids.len(), "knowledge usage record failed");
        }

        let hits: Vec<KnowledgeHitEntry> = entries
            .into_iter()
            .map(|e| KnowledgeHitEntry {
                kind: e.kind.to_string(),
                title: e.title,
                content: e.content,
                confidence: e.confidence,
                affected_labels: e.affected_labels,
            })
            .collect();

        let total = hits.len();
        Ok(ConsultKnowledgeOutput {
            entries: hits,
            total,
        })
    }
}
