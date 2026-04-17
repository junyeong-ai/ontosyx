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
    workspace_id: Uuid,
    ontology_name: String,
    ontology_version: i32,
    /// Per-session tool outcome tracking: session_id → list of outcomes.
    session_outcomes: DashMap<String, Vec<ToolOutcome>>,
    /// Dedup guard: (session_id, query_hash) tuples already turned into
    /// a knowledge entry. Prevents the same recovery from being recorded
    /// twice when the same successful query appears multiple times.
    processed_recoveries: DashMap<String, std::collections::HashSet<String>>,
}

/// Minimum Jaccard similarity between failed and successful query label
/// sets required to treat them as a recovery pair. Below this threshold
/// the queries probably target unrelated parts of the schema and the
/// failure→success sequence is coincidental.
const RECOVERY_JACCARD_THRESHOLD: f64 = 0.5;

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
        workspace_id: Uuid,
        ontology_name: String,
        ontology_version: i32,
    ) -> Self {
        Self {
            knowledge_store,
            memory,
            workspace_id,
            ontology_name,
            ontology_version,
            session_outcomes: DashMap::new(),
            processed_recoveries: DashMap::new(),
        }
    }

    /// Periodic cleanup: remove entries older than 10 minutes.
    fn cleanup_stale_sessions(&self) {
        let cutoff = Utc::now() - chrono::Duration::minutes(10);
        self.session_outcomes
            .retain(|_, outcomes| outcomes.last().is_some_and(|o| o.timestamp > cutoff));
        // Dedup tracker shadows session_outcomes — drop entries for
        // sessions that no longer have any outcomes recorded.
        self.processed_recoveries
            .retain(|sid, _| self.session_outcomes.contains_key(sid));
    }
}

/// Extract `:Label` tokens from a Cypher query (case-insensitive on the colon).
/// Used by [`is_structural_match`] to compare failed vs. successful queries.
fn extract_cypher_labels(query: &str) -> std::collections::HashSet<String> {
    let mut labels = std::collections::HashSet::new();
    let bytes = query.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() {
                let c = bytes[end];
                if c.is_ascii_alphanumeric() || c == b'_' || !c.is_ascii() {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                if let Ok(label) = std::str::from_utf8(&bytes[start..end])
                    && label
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || !c.is_ascii())
                {
                    labels.insert(label.to_string());
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    labels
}

/// Jaccard similarity between two label sets (`|A ∩ B| / |A ∪ B|`).
fn jaccard(
    a: &std::collections::HashSet<String>,
    b: &std::collections::HashSet<String>,
) -> Option<f64> {
    let union = a.union(b).count();
    if union == 0 {
        return None;
    }
    let intersection = a.intersection(b).count();
    Some(intersection as f64 / union as f64)
}

/// Returns true when `failed` and `succeeded` look structurally similar
/// enough to be considered a recovery pair (Jaccard label similarity ≥
/// [`RECOVERY_JACCARD_THRESHOLD`]). Errors with no compiled query fall
/// back to "match anything" because we have no labels to compare.
fn is_structural_match(failed_query: Option<&str>, succeeded_query: Option<&str>) -> bool {
    let succeeded = match succeeded_query {
        Some(q) => q,
        None => return false, // no signal — never pair
    };
    let succeeded_labels = extract_cypher_labels(succeeded);

    let Some(failed) = failed_query else {
        // Failure with no compiled query (parse error etc.) — labels
        // are unavailable on the failed side. The presence of a
        // successful query in the same session is enough.
        return true;
    };
    let failed_labels = extract_cypher_labels(failed);

    match jaccard(&failed_labels, &succeeded_labels) {
        Some(score) => score >= RECOVERY_JACCARD_THRESHOLD,
        None => true, // both empty (legacy queries without labels) — preserve old behavior
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

                    // Structural similarity gate: only treat as recovery if
                    // the failed and successful queries touch overlapping
                    // schema (Jaccard ≥ threshold). Otherwise the pairing
                    // is coincidental and would pollute the knowledge base.
                    if !is_structural_match(failure_compiled.as_deref(), success_query.as_deref())
                    {
                        // Not a real recovery pair — discard outcomes and bail
                        self.session_outcomes.remove(session_id);
                        return Ok(HookOutput::allow());
                    }

                    // Dedup by query hash within session: prevent duplicate
                    // knowledge entries when the same successful query
                    // recovers from the same failure repeatedly.
                    let dedup_key = success_query
                        .as_deref()
                        .map(ox_brain::knowledge_util::content_hash_query);
                    if let Some(ref key) = dedup_key {
                        let mut hashes = self
                            .processed_recoveries
                            .entry(session_id.clone())
                            .or_default();
                        if !hashes.insert(key.clone()) {
                            // Already processed this recovery in this session.
                            self.session_outcomes.remove(session_id);
                            return Ok(HookOutput::allow());
                        }
                    }

                    // Build correction content based on failure type.
                    // `prior_failure_data` is filtered to Error|Empty upstream,
                    // but the match must stay exhaustive over OutcomeKind.
                    // Returning Option lets us skip the Success arm safely —
                    // no panic, and the upstream filter bug (if any) shows
                    // up as a warning instead of a process crash.
                    let session_short = &session_id[..8.min(session_id.len())];
                    let Some((title, content, extraction_method)) = (match failure_kind {
                        OutcomeKind::Error => {
                            let error_excerpt = if failure_text.len() > 200 {
                                &failure_text[..failure_text.floor_char_boundary(200)]
                            } else {
                                &failure_text
                            };
                            Some((
                                format!(
                                    "Recovery: query_graph failed then succeeded in session {session_short}"
                                ),
                                format!(
                                    "Failed: {}\nCorrection: {}",
                                    error_excerpt,
                                    success_query.as_deref().unwrap_or("(successful query)"),
                                ),
                                "recovery_detection",
                            ))
                        }
                        OutcomeKind::Empty => Some((
                            format!(
                                "Refinement: query_graph empty then succeeded in session {session_short}"
                            ),
                            format!(
                                "Empty (0 rows): {}\nCorrection: {}",
                                failure_compiled.as_deref().unwrap_or("(unknown query)"),
                                success_query.as_deref().unwrap_or("(successful query)"),
                            ),
                            "zero_row_recovery",
                        )),
                        OutcomeKind::Success => None,
                    }) else {
                        tracing::warn!(
                            "RecoveryDetectionHook: Success outcome reached recovery match arm"
                        );
                        return Ok(HookOutput::allow());
                    };

                    let hash =
                        ox_brain::knowledge_util::content_hash(&self.ontology_name, &content);

                    let entry = KnowledgeEntry {
                        id: Uuid::new_v4(),
                        workspace_id: self.workspace_id,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_cypher_labels_basic() {
        let labels = extract_cypher_labels("MATCH (p:Person)-[:KNOWS]->(c:Company)");
        assert!(labels.contains("Person"));
        assert!(labels.contains("KNOWS"));
        assert!(labels.contains("Company"));
        assert_eq!(labels.len(), 3);
    }

    #[test]
    fn extract_cypher_labels_korean() {
        let labels = extract_cypher_labels("MATCH (n:사용자)-[:속함]->(t:팀)");
        assert!(labels.contains("사용자"));
        assert!(labels.contains("속함"));
        assert!(labels.contains("팀"));
    }

    #[test]
    fn extract_cypher_labels_skips_property_access() {
        let labels = extract_cypher_labels("MATCH (n:Person) WHERE n.name = 'x'");
        assert_eq!(labels, std::collections::HashSet::from(["Person".into()]));
    }

    #[test]
    fn extract_cypher_labels_ignores_numeric_starts() {
        let labels = extract_cypher_labels("RETURN $param, :123, :Foo");
        assert_eq!(labels, std::collections::HashSet::from(["Foo".into()]));
    }

    #[test]
    fn jaccard_basic() {
        let a: std::collections::HashSet<String> =
            ["A", "B", "C"].iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> =
            ["B", "C", "D"].iter().map(|s| s.to_string()).collect();
        // |A∩B|=2, |A∪B|=4 → 0.5
        assert_eq!(jaccard(&a, &b), Some(0.5));
    }

    #[test]
    fn jaccard_disjoint_is_zero() {
        let a: std::collections::HashSet<String> = ["A"].iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> = ["B"].iter().map(|s| s.to_string()).collect();
        assert_eq!(jaccard(&a, &b), Some(0.0));
    }

    #[test]
    fn jaccard_both_empty_returns_none() {
        let a = std::collections::HashSet::new();
        let b = std::collections::HashSet::new();
        assert_eq!(jaccard(&a, &b), None);
    }

    #[test]
    fn structural_match_at_threshold() {
        assert!(is_structural_match(
            Some("MATCH (n:Person) RETURN n"),
            Some("MATCH (p:Person {name: 'x'}) RETURN p"),
        ));
    }

    #[test]
    fn structural_match_no_overlap_rejected() {
        assert!(!is_structural_match(
            Some("MATCH (a:Order) RETURN a"),
            Some("MATCH (b:Customer) RETURN b"),
        ));
    }

    #[test]
    fn structural_match_partial_overlap() {
        // {Person, BOUGHT, Order} vs {Person, VIEWED, Product} → |∩|=1, |∪|=5 → 0.20 < 0.5
        assert!(!is_structural_match(
            Some("MATCH (p:Person)-[:BOUGHT]->(o:Order) RETURN p"),
            Some("MATCH (p:Person)-[:VIEWED]->(pd:Product) RETURN p"),
        ));
    }

    #[test]
    fn structural_match_failure_with_no_compiled_query_passes() {
        // No labels on the failed side — fall through to "anything goes".
        assert!(is_structural_match(
            None,
            Some("MATCH (n:Person) RETURN n"),
        ));
    }

    #[test]
    fn structural_match_no_success_query_rejected() {
        assert!(!is_structural_match(
            Some("MATCH (n:Person) RETURN n"),
            None,
        ));
    }
}
