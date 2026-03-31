use std::sync::Arc;

use async_trait::async_trait;
use branchforge::hooks::{Hook, HookContext, HookEvent, HookEventData, HookInput, HookOutput};
use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use tracing::{info, warn};

use ox_memory::{MemoryEntry, MemoryMetadata, MemorySource, MemoryStore};

/// Maximum concurrent background embedding tasks.
const MAX_CONCURRENT_EMBEDDINGS: usize = 8;

static EMBEDDING_SEMAPHORE: std::sync::LazyLock<Arc<Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_EMBEDDINGS)));

// ---------------------------------------------------------------------------
// EmbeddingHook — auto-embed tool results into long-term memory
// ---------------------------------------------------------------------------

/// branchforge PostToolUse hook that automatically embeds tool results
/// into the semantic memory store.
///
/// - Non-blocking (fail-open): embedding failures never delay agent execution.
/// - Content-hash deduplication: identical content is not re-embedded.
/// - Session summaries embedded separately from chat handler on AgentEvent::Complete.
/// - Failed embeddings are enqueued for retry when a retry store is available.
pub struct EmbeddingHook {
    memory: Arc<MemoryStore>,
    ontology_id: Option<String>,
    retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>>,
}

impl EmbeddingHook {
    pub fn new(memory: Arc<MemoryStore>) -> Self {
        Self {
            memory,
            ontology_id: None,
            retry_store: None,
        }
    }

    pub fn with_ontology_id(
        memory: Arc<MemoryStore>,
        ontology_id: Option<String>,
        retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>>,
    ) -> Self {
        Self {
            memory,
            ontology_id,
            retry_store,
        }
    }

    /// Embed content asynchronously in background — never blocks caller.
    /// Uses content hash as entry ID for automatic deduplication.
    /// Failed embeddings are enqueued for retry when a retry store is provided.
    pub fn embed_async(
        memory: &Arc<MemoryStore>,
        content: String,
        source: MemorySource,
        ontology_id: Option<String>,
        session_id: Option<String>,
        retry_store: Option<&Arc<dyn ox_store::EmbeddingRetryStore>>,
        context_scope: Option<branchforge::SharedContextScope>,
    ) {
        if content.trim().is_empty() {
            return;
        }

        let memory = Arc::clone(memory);
        let retry_store = retry_store.cloned();

        // Content-hash ID for deduplication (includes ontology_id to avoid cross-ontology collisions)
        let mut hasher = Sha256::new();
        if let Some(ref oid) = ontology_id {
            hasher.update(oid.as_bytes());
        }
        hasher.update(content.as_bytes());
        let entry_id = format!("mem_{:x}", hasher.finalize());

        let metadata = MemoryMetadata {
            source,
            ontology_id,
            session_id,
            created_at: Utc::now(),
        };

        tokio::spawn(async move {
            let embed_fut = async {
                let _permit = match EMBEDDING_SEMAPHORE.try_acquire() {
                    Ok(p) => p,
                    Err(_) => {
                        warn!("Embedding semaphore full — skipping");
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
                    Ok(()) => info!(id = %entry_id, "Embedded in memory"),
                    Err(e) => {
                        warn!(id = %entry_id, error = %e, "Memory embedding failed");
                        if let Some(store) = retry_store {
                            let _ = store.enqueue_pending_embedding(&content_clone, &metadata_json).await;
                        }
                    }
                }
            };

            // Wrap with context scope if available (propagates workspace task-locals)
            if let Some(scope) = context_scope {
                let _ = scope.wrap_tool_future(Box::pin(async move {
                    embed_fut.await;
                    branchforge::ToolResult::success("")
                })).await;
            } else {
                embed_fut.await;
            }
        });
    }

    fn extract_tool_content(tool_name: &str, output: &str) -> Option<(String, MemorySource)> {
        match tool_name {
            "query_graph" => {
                let parsed: serde_json::Value = serde_json::from_str(output).ok()?;
                let query = parsed.get("compiled_query")?.as_str()?;
                let row_count = parsed.get("row_count")?.as_u64()?;
                let columns = parsed
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
                let parsed: serde_json::Value = serde_json::from_str(output).ok()?;
                let explanation = parsed.get("explanation")?.as_str()?;
                let cmd_count = parsed.get("command_count")?.as_u64()?;
                Some((
                    format!("Ontology edit ({cmd_count} commands): {explanation}"),
                    MemorySource::Edit,
                ))
            }
            "execute_analysis" => {
                let content = if output.len() > 500 {
                    format!("{}...", &output[..500])
                } else {
                    output.to_string()
                };
                Some((content, MemorySource::Analysis))
            }
            "explain_ontology" => {
                // Brain explain output is plain text (not JSON)
                let truncated = if output.len() > 500 {
                    format!("{}...", &output[..500])
                } else {
                    output.to_string()
                };
                Some((truncated, MemorySource::Session))
            }
            "visualize" => {
                let parsed: serde_json::Value = serde_json::from_str(output).ok()?;
                let chart_type = parsed.get("chart_type")?.as_str()?;
                let title = parsed
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Untitled");
                let cols = parsed
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
}

#[async_trait]
impl Hook for EmbeddingHook {
    fn name(&self) -> &str {
        "ontosyx_embedding"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    async fn execute(
        &self,
        input: HookInput,
        ctx: &HookContext,
    ) -> Result<HookOutput, branchforge::Error> {
        if let HookEventData::PostToolUse {
            tool_name,
            tool_result,
        } = &input.data
        {
            let output_text = tool_result.text();
            if let Some((content, source)) = Self::extract_tool_content(tool_name, &output_text) {
                let ontology_id = self.ontology_id.clone();
                let session_id = if ctx.session_id.is_empty() {
                    None
                } else {
                    Some(ctx.session_id.clone())
                };
                Self::embed_async(
                    &self.memory,
                    content,
                    source,
                    ontology_id,
                    session_id,
                    self.retry_store.as_ref(),
                    ctx.context_scope.clone(),
                );
            }
        }

        Ok(HookOutput::allow())
    }
}
