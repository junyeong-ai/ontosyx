use std::sync::Arc;

use async_trait::async_trait;
use entelix::tools::ToolEffect;
use entelix::{AgentContext, SchemaTool};
use ox_memory::{MemoryFilter, MemorySource, MemoryStore};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Search mode for memory recall.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Semantic similarity search (cosine distance on embeddings).
    #[default]
    Semantic,
    /// Exact pattern matching (ILIKE with trigram index).
    Pattern,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecallMemoryInput {
    /// Search query: natural language for semantic, keywords/pattern for pattern mode.
    pub query: String,
    /// Search mode: "semantic" (default) or "pattern" for exact keyword matching.
    #[serde(default)]
    pub mode: SearchMode,
    /// Filter by source type: "query", "analysis", "edit", "session", "recipe".
    #[serde(default)]
    pub source: Option<String>,
    /// Filter results to current ontology scope.
    #[serde(default)]
    pub ontology_lineage_id: Option<String>,
    /// Maximum number of results (default 5, max 20).
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    5
}

#[derive(Debug, Serialize)]
pub struct RecallMemoryOutput {
    hits: Vec<MemoryHitEntry>,
    total: usize,
}

#[derive(Debug, Serialize)]
struct MemoryHitEntry {
    content: String,
    source: String,
    score: f32,
}

/// Searches long-term memory for relevant past interactions.
/// Supports semantic (vector similarity) and pattern (keyword) search modes.
pub struct RecallMemoryTool {
    pub memory: Arc<MemoryStore>,
    /// Current ontology scope — automatically applied as a filter so that
    /// memory recall doesn't leak results from unrelated ontologies.
    pub ontology_lineage_id: Option<String>,
}

#[async_trait]
impl SchemaTool for RecallMemoryTool {
    type Input = RecallMemoryInput;
    type Output = RecallMemoryOutput;
    const NAME: &'static str = super::RECALL_MEMORY;

    fn description(&self) -> &str {
        "Search long-term memory across past queries, analyses, edits, sessions. \
         'semantic' mode for meaning-based match, 'pattern' for exact keyword. \
         Call when prior session context would help."
    }

    fn effect(&self) -> ToolEffect {
        ToolEffect::ReadOnly
    }

    async fn execute(
        &self,
        input: Self::Input,
        _ctx: &AgentContext<()>,
    ) -> entelix::Result<Self::Output> {
        let source_hint = input.source.as_deref().and_then(parse_memory_source);
        let top_k = input.top_k.min(20);

        let hits = match input.mode {
            SearchMode::Semantic => {
                // Merge tool-level ontology scope with any per-request overrides.
                let effective_ontology_lineage_id = input
                    .ontology_lineage_id
                    .as_deref()
                    .or(self.ontology_lineage_id.as_deref());

                let filter = MemoryFilter {
                    ontology_lineage_id: effective_ontology_lineage_id.map(String::from),
                    source: input.source.clone(),
                    session_id: None,
                };

                self.memory
                    .search_filtered(&input.query, source_hint.as_ref(), top_k, &filter)
                    .await
            }
            SearchMode::Pattern => self.memory.pattern_search(&input.query, top_k).await,
        }
        .map_err(|e| entelix::Error::invalid_request(format!("Memory search failed: {e}")))?;

        let entries: Vec<MemoryHitEntry> = hits
            .iter()
            // Score threshold — filter low-relevance results so the LLM
            // doesn't get noisy context that would dilute its reasoning.
            .filter(|hit| hit.score >= 0.3)
            .map(|hit| MemoryHitEntry {
                content: hit.content.chars().take(500).collect(),
                source: format!("{:?}", hit.metadata.source),
                score: hit.score,
            })
            .collect();

        Ok(RecallMemoryOutput {
            total: entries.len(),
            hits: entries,
        })
    }
}

fn parse_memory_source(s: &str) -> Option<MemorySource> {
    match s {
        "query" => Some(MemorySource::Query),
        "analysis" => Some(MemorySource::Analysis),
        "edit" => Some(MemorySource::Edit),
        "session" => Some(MemorySource::Session),
        "recipe" => Some(MemorySource::Recipe),
        _ => None,
    }
}
