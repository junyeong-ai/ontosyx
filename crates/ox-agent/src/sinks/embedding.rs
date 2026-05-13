//! `EmbeddingSink` — pushes tool-output snippets into the long-term
//! semantic memory store on every successful tool dispatch.
//!
//! Subscribes to [`AgentEvent::ToolComplete`] and forwards the parsed
//! JSON output to the embedder. RLS task-locals are captured at emit
//! time via [`ox_context::ContextScope`] and re-applied inside the
//! spawned embed task so workspace-scoped writes land under the right
//! tenant.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use entelix::{AgentEvent, AgentEventSink, ReActState};
use ox_context::ContextScope;
use ox_memory::{MemoryEntry, MemoryMetadata, MemorySource, MemoryStore};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use tracing::{info, warn};

/// Maximum concurrent background embedding tasks per process. Caps
/// the embedder's hot-path queue depth so a burst of tool completions
/// can't pin every CPU on hash + vectorise work.
const MAX_CONCURRENT_EMBEDDINGS: usize = 8;

static EMBEDDING_SEMAPHORE: std::sync::LazyLock<Arc<Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_EMBEDDINGS)));

/// Auto-embeds tool results into long-term memory. Fail-open — every
/// internal failure logs and drops the embedding rather than halting
/// the agent.
pub struct EmbeddingSink {
    memory: Arc<MemoryStore>,
    ontology_lineage_id: Option<String>,
    retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>>,
}

impl EmbeddingSink {
    /// Default sink with no lineage / retry plumbing. Production
    /// builds typically use [`Self::with_ontology_lineage_id`].
    #[must_use]
    pub fn new(memory: Arc<MemoryStore>) -> Self {
        Self {
            memory,
            ontology_lineage_id: None,
            retry_store: None,
        }
    }

    /// Bind the sink to one ontology lineage (so RAG filters hit the
    /// right slice) plus a retry store for failed-embedding queueing.
    #[must_use]
    pub fn with_ontology_lineage_id(
        memory: Arc<MemoryStore>,
        ontology_lineage_id: Option<String>,
        retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>>,
    ) -> Self {
        Self {
            memory,
            ontology_lineage_id,
            retry_store,
        }
    }

    /// Map a tool's name + JSON output to the LLM-relevant snippet to
    /// embed. Returns `None` for tools whose output never carries
    /// reusable signal — the per-tool extraction list is curated
    /// (query_graph / edit_ontology / execute_analysis /
    /// explain_ontology / visualize) so the embedded surface stays
    /// coherent rather than a noisy union of every tool's wire.
    fn extract_tool_content(tool_name: &str, output: &Value) -> Option<(String, MemorySource)> {
        match tool_name {
            "query_graph" => {
                let query = output.get("compiled_query")?.as_str()?;
                let row_count = output.get("row_count")?.as_u64()?;
                let columns = output
                    .get("columns")
                    .and_then(|c| c.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                Some((
                    format!("Query: {query}\nColumns: {columns}\nRows: {row_count}"),
                    MemorySource::Query,
                ))
            }
            "edit_ontology" => {
                let explanation = output.get("explanation")?.as_str()?;
                let cmd_count = output.get("command_count")?.as_u64()?;
                Some((
                    format!("Ontology edit ({cmd_count} commands): {explanation}"),
                    MemorySource::Edit,
                ))
            }
            "execute_analysis" => {
                let raw = serde_json::to_string(output).ok()?;
                Some((truncate(&raw, 500), MemorySource::Analysis))
            }
            "explain_ontology" => {
                // Brain explain output may be plain text wrapped or a
                // JSON Value with a single string. Accept both.
                let text = output
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| serde_json::to_string(output).ok())?;
                Some((truncate(&text, 500), MemorySource::Session))
            }
            "visualize" => {
                let chart_type = output.get("chart_type")?.as_str()?;
                let title = output
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Untitled");
                let cols = output
                    .get("columns")
                    .and_then(|c| c.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                Some((
                    format!("Chart ({chart_type}): {title}\nColumns: {cols}"),
                    MemorySource::Query,
                ))
            }
            _ => None,
        }
    }

    /// Spawn the embed task. Captured `ContextScope` re-applies the
    /// caller's RLS task-locals inside the spawn so workspace-scoped
    /// memory writes land under the right tenant.
    fn embed_async(
        memory: Arc<MemoryStore>,
        content: String,
        source: MemorySource,
        ontology_lineage_id: Option<String>,
        session_id: Option<String>,
        retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>>,
        scope: ContextScope,
    ) {
        if content.trim().is_empty() {
            return;
        }

        // Content-hash ID for deduplication. Includes
        // `ontology_lineage_id` to avoid cross-ontology collisions.
        let mut hasher = Sha256::new();
        if let Some(ref oid) = ontology_lineage_id {
            hasher.update(oid.as_bytes());
        }
        hasher.update(content.as_bytes());
        let entry_id = format!("mem_{}", hex::encode(hasher.finalize()));

        let metadata = MemoryMetadata {
            source,
            ontology_lineage_id,
            session_id,
            created_at: Utc::now(),
        };

        // The agent runtime holds task-locals while polling the sink;
        // `ContextScope::spawn` re-enters the captured scope inside
        // the spawned task so the workspace-scoped memory store write
        // lands under the right tenant.
        scope.spawn(async move {
            let _permit = match EMBEDDING_SEMAPHORE.try_acquire() {
                Ok(p) => p,
                Err(_) => {
                    warn!("embedding semaphore full — skipping (queue overflow)");
                    return;
                }
            };
            let content_clone = content.clone();
            let metadata_json = serde_json::to_value(&metadata).unwrap_or_default();
            let entry = MemoryEntry {
                id: entry_id.clone(),
                content,
                metadata,
            };
            match memory.store(entry).await {
                Ok(()) => info!(id = %entry_id, "embedded in memory"),
                Err(e) => {
                    warn!(id = %entry_id, error = %e, "memory embedding failed");
                    if let Some(retry) = retry_store
                        && let Err(retry_err) = retry
                            .create_pending_embedding(&content_clone, &metadata_json)
                            .await
                    {
                        warn!(
                            id = %entry_id,
                            error = %retry_err,
                            "failed to enqueue embedding for retry — entry lost",
                        );
                    }
                }
            }
        });
    }
}

#[async_trait]
impl AgentEventSink<ReActState> for EmbeddingSink {
    async fn send(&self, event: AgentEvent<ReActState>) -> entelix::Result<()> {
        if let AgentEvent::ToolComplete {
            tool,
            output,
            run_id,
            ..
        } = event
            && let Some((content, source)) = Self::extract_tool_content(&tool, &output)
        {
            // Capture RLS task-locals before spawn — the agent runtime
            // holds them while polling sinks, so this read sees the
            // active workspace; re-applying inside the spawn keeps
            // memory writes under the right tenant.
            let scope = ContextScope::capture_current();
            Self::embed_async(
                Arc::clone(&self.memory),
                content,
                source,
                self.ontology_lineage_id.clone(),
                Some(run_id),
                self.retry_store.clone(),
                scope,
            );
        }
        Ok(())
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn extract_query_graph_emits_columns_and_row_count() {
        let output = json!({
            "compiled_query": "MATCH (n) RETURN n",
            "row_count": 42,
            "columns": ["a", "b"],
        });
        let (content, source) =
            EmbeddingSink::extract_tool_content("query_graph", &output).expect("extracted");
        assert!(content.contains("MATCH (n) RETURN n"));
        assert!(content.contains("Columns: a, b"));
        assert!(content.contains("Rows: 42"));
        assert!(matches!(source, MemorySource::Query));
    }

    #[test]
    fn extract_query_graph_returns_none_without_compiled_query() {
        let output = json!({"row_count": 0, "columns": []});
        assert!(EmbeddingSink::extract_tool_content("query_graph", &output).is_none());
    }

    #[test]
    fn extract_edit_ontology_carries_command_count_and_explanation() {
        let output = json!({"command_count": 3, "explanation": "promoted alias"});
        let (content, source) =
            EmbeddingSink::extract_tool_content("edit_ontology", &output).expect("extracted");
        assert!(content.contains("3 commands"));
        assert!(content.contains("promoted alias"));
        assert!(matches!(source, MemorySource::Edit));
    }

    #[test]
    fn extract_visualize_falls_back_to_untitled() {
        let output = json!({"chart_type": "bar", "columns": ["x"]});
        let (content, _) =
            EmbeddingSink::extract_tool_content("visualize", &output).expect("extracted");
        assert!(content.contains("Chart (bar)"));
        assert!(content.contains("Untitled"));
        assert!(content.contains("Columns: x"));
    }

    #[test]
    fn unsupported_tool_name_returns_none() {
        // Curated extraction list — adding a tool needs an explicit
        // arm. The wire shape is intentionally narrow so embedded
        // memory entries stay coherent, not a union of every tool's
        // raw JSON.
        let output = json!({"foo": "bar"});
        assert!(EmbeddingSink::extract_tool_content("brand_new_tool", &output).is_none());
    }

    #[test]
    fn truncate_respects_char_boundary_for_korean() {
        // Korean chars are multi-byte; naive truncation at a byte
        // index can land mid-codepoint. The helper walks back to the
        // nearest char boundary so the resulting string is always
        // valid UTF-8.
        let input = "한글입니다";
        let truncated = truncate(input, 5);
        assert!(truncated.ends_with("..."));
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_returns_input_when_shorter_than_max() {
        assert_eq!(truncate("short", 100), "short");
    }
}
