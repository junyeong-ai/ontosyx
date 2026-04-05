use std::sync::Arc;

use async_trait::async_trait;
use branchforge::hooks::{Hook, HookContext, HookEvent, HookEventData, HookInput, HookOutput};
use chrono::Utc;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use tracing::{info, warn};
use uuid::Uuid;

use ox_memory::{MemoryEntry, MemoryMetadata, MemorySource, MemoryStore};
use ox_store::{KnowledgeEntry, KnowledgeStore};

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
                            let _ = store
                                .create_pending_embedding(&content_clone, &metadata_json)
                                .await;
                        }
                    }
                }
            };

            // Wrap with context scope if available (propagates workspace task-locals)
            if let Some(scope) = context_scope {
                let _ = scope
                    .wrap_tool_future(Box::pin(async move {
                        embed_fut.await;
                        branchforge::ToolResult::success("")
                    }))
                    .await;
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
                    let end = output.floor_char_boundary(500);
                    format!("{}...", &output[..end])
                } else {
                    output.to_string()
                };
                Some((content, MemorySource::Analysis))
            }
            "explain_ontology" => {
                // Brain explain output is plain text (not JSON)
                let truncated = if output.len() > 500 {
                    let end = output.floor_char_boundary(500);
                    format!("{}...", &output[..end])
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

// ---------------------------------------------------------------------------
// RecoveryDetectionHook — detect failure→success patterns for knowledge
// ---------------------------------------------------------------------------

/// Tracks query_graph tool calls per session. When a success follows a failure
/// in the same session, creates a verified `correction` knowledge entry.
///
/// - Non-blocking (fail-open): extraction failures never delay agent execution.
/// - In-memory tracking per session (DashMap, cleaned up after 10 minutes).
/// - Zero LLM cost: corrections are extracted mechanically from tool outputs.
pub struct RecoveryDetectionHook {
    knowledge_store: Arc<dyn KnowledgeStore>,
    memory: Option<Arc<ox_memory::MemoryStore>>,
    ontology_name: String,
    ontology_version: i32,
    /// Per-session tool outcome tracking: session_id → list of outcomes.
    session_outcomes: DashMap<String, Vec<ToolOutcome>>,
}

/// Distinguishes three outcome states for recovery detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeKind {
    /// Tool returned an error.
    Error,
    /// Tool succeeded but query returned 0 rows.
    Empty,
    /// Tool succeeded with row_count > 0.
    Success,
}

struct ToolOutcome {
    kind: OutcomeKind,
    text: String,
    compiled_query: Option<String>,
    #[allow(dead_code)]
    row_count: usize,
    timestamp: chrono::DateTime<Utc>,
}

impl RecoveryDetectionHook {
    pub fn new(
        knowledge_store: Arc<dyn KnowledgeStore>,
        memory: Option<Arc<ox_memory::MemoryStore>>,
        ontology_name: String,
        ontology_version: i32,
    ) -> Self {
        Self {
            knowledge_store,
            memory,
            ontology_name,
            ontology_version,
            session_outcomes: DashMap::new(),
        }
    }

    /// Periodic cleanup: remove entries older than 10 minutes.
    fn cleanup_stale_sessions(&self) {
        let cutoff = Utc::now() - chrono::Duration::minutes(10);
        self.session_outcomes
            .retain(|_, outcomes| outcomes.last().is_some_and(|o| o.timestamp > cutoff));
    }
}

#[async_trait]
impl Hook for RecoveryDetectionHook {
    fn name(&self) -> &str {
        "ontosyx_recovery_detection"
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
            // Only track query_graph calls
            if tool_name != "query_graph" {
                return Ok(HookOutput::allow());
            }

            let session_id = &ctx.session_id;
            let is_error = tool_result.is_error();
            let text = tool_result.text();

            // Classify outcome: Error / Empty (0 rows) / Success (N rows)
            let (kind, compiled_query, row_count) = if is_error {
                (OutcomeKind::Error, None, 0)
            } else {
                let (cq, rc) = parse_query_metrics(&text);
                let kind = if rc == 0 {
                    OutcomeKind::Empty
                } else {
                    OutcomeKind::Success
                };
                (kind, cq, rc)
            };

            // Record this outcome
            let outcome = ToolOutcome {
                kind,
                text: text.clone(),
                compiled_query,
                row_count,
                timestamp: Utc::now(),
            };
            self.session_outcomes
                .entry(session_id.clone())
                .or_default()
                .push(outcome);

            // Check for recovery pattern: prior failure (error or empty) + current success
            if kind == OutcomeKind::Success {
                // Extract failure data while holding the DashMap guard
                let prior_failure_data =
                    self.session_outcomes.get(session_id).and_then(|outcomes| {
                        outcomes
                            .iter()
                            .rev()
                            .skip(1)
                            .find(|o| matches!(o.kind, OutcomeKind::Error | OutcomeKind::Empty))
                            .map(|o| (o.kind, o.text.clone(), o.compiled_query.clone()))
                    });

                if let Some((failure_kind, failure_text, failure_compiled)) = prior_failure_data {
                    // Extract labels and query from success output
                    let (success_query, labels, execution_id) = parse_success_output(&text);

                    // Build correction content based on failure type
                    let session_short = &session_id[..8.min(session_id.len())];
                    let (title, content, extraction_method) = match failure_kind {
                        OutcomeKind::Error => {
                            let error_excerpt = if failure_text.len() > 200 {
                                &failure_text[..failure_text.floor_char_boundary(200)]
                            } else {
                                &failure_text
                            };
                            (
                                format!(
                                    "Recovery: query_graph failed then succeeded in session {session_short}"
                                ),
                                format!(
                                    "Failed: {}\nCorrection: {}",
                                    error_excerpt,
                                    success_query.as_deref().unwrap_or("(successful query)"),
                                ),
                                "recovery_detection",
                            )
                        }
                        OutcomeKind::Empty => (
                            format!(
                                "Refinement: query_graph empty then succeeded in session {session_short}"
                            ),
                            format!(
                                "Empty (0 rows): {}\nCorrection: {}",
                                failure_compiled.as_deref().unwrap_or("(unknown query)"),
                                success_query.as_deref().unwrap_or("(successful query)"),
                            ),
                            "zero_row_recovery",
                        ),
                        OutcomeKind::Success => unreachable!(),
                    };

                    let hash =
                        ox_brain::knowledge_util::content_hash(&self.ontology_name, &content);

                    let entry = KnowledgeEntry {
                        id: Uuid::new_v4(),
                        workspace_id: Uuid::nil(), // RLS default
                        ontology_name: self.ontology_name.clone(),
                        ontology_version_min: self.ontology_version,
                        ontology_version_max: None,
                        kind: "correction".to_string(),
                        status: "approved".to_string(),
                        confidence: 0.8,
                        title,
                        content,
                        structured_data: serde_json::json!({
                            "extraction_method": extraction_method,
                            "failure_kind": format!("{:?}", failure_kind),
                            "success_query": success_query,
                            "success_execution_id": execution_id,
                        }),
                        embedding: None,
                        version_checked: self.ontology_version,
                        content_hash: hash,
                        source_execution_ids: execution_id
                            .and_then(|id| Uuid::parse_str(&id).ok())
                            .into_iter()
                            .collect(),
                        source_session_id: Uuid::parse_str(session_id).ok(),
                        affected_labels: labels,
                        affected_properties: vec![],
                        created_by: "system:recovery".to_string(),
                        reviewed_by: None,
                        reviewed_at: None,
                        review_notes: None,
                        use_count: 0,
                        last_used_at: None,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    };

                    // Non-blocking: best-effort persistence
                    let store = Arc::clone(&self.knowledge_store);
                    // Persist with workspace context (required for RLS).
                    // Without context_scope, the INSERT would fail because
                    // app.workspace_id session var is not set on the connection.
                    if let Some(scope) = ctx.context_scope.clone() {
                        tokio::spawn(async move {
                            let _ = scope
                                .wrap_tool_future(Box::pin(async move {
                                    match store.create_knowledge_entry(&entry).await {
                                        Ok(()) => info!(
                                            ontology = %entry.ontology_name,
                                            "Knowledge correction from recovery detection"
                                        ),
                                        Err(e) => {
                                            warn!(error = %e, "Failed to save recovery correction")
                                        }
                                    }
                                    branchforge::ToolResult::success("")
                                }))
                                .await;
                        });
                    } else {
                        warn!("Cannot persist recovery correction: no workspace context scope");
                    }

                    // Clean stale session memories (poisoned by failed queries)
                    if let Some(ref memory) = self.memory {
                        let sid = session_id.to_string();
                        let mem = Arc::clone(memory);
                        tokio::spawn(async move {
                            match mem.cleanup_by_session(&sid).await {
                                Ok(n) if n > 0 => info!(
                                    session_id = %sid,
                                    count = n,
                                    "Cleaned stale session memories after recovery"
                                ),
                                Err(e) => {
                                    warn!(error = %e, "Failed to clean stale session memories")
                                }
                                _ => {}
                            }
                        });
                    }

                    // Clear session outcomes after extraction
                    self.session_outcomes.remove(session_id);
                } else {
                    // Success with no prior failure — clean up to prevent unbounded growth.
                    self.session_outcomes.remove(session_id);
                }
            }

            // Periodic cleanup of stale sessions
            if self.session_outcomes.len() > 50 {
                self.cleanup_stale_sessions();
            }
        }

        Ok(HookOutput::allow())
    }
}

/// Parse compiled_query and row_count from query_graph output (success or empty).
/// Used for outcome classification (Error/Empty/Success).
fn parse_query_metrics(output: &str) -> (Option<String>, usize) {
    let parsed: serde_json::Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => return (None, 0),
    };
    let compiled_query = parsed
        .get("compiled_query")
        .and_then(|v| v.as_str())
        .map(String::from);
    let row_count = parsed
        .get("row_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    (compiled_query, row_count)
}

/// Parse successful query_graph output to extract compiled query, labels, and execution ID.
fn parse_success_output(output: &str) -> (Option<String>, Vec<String>, Option<String>) {
    let parsed: serde_json::Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(_) => return (None, vec![], None),
    };

    let compiled_query = parsed
        .get("compiled_query")
        .and_then(|v| v.as_str())
        .map(String::from);
    let execution_id = parsed
        .get("execution_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract labels from columns (heuristic: PascalCase or non-ASCII starts like Korean)
    let labels: Vec<String> = parsed
        .get("columns")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| {
                    s.chars()
                        .next()
                        .is_some_and(|c| c.is_uppercase() || !c.is_ascii())
                })
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    (compiled_query, labels, execution_id)
}
