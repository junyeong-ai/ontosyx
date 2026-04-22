use std::sync::Arc;

use async_trait::async_trait;
use branchforge::hooks::{Hook, HookContext, HookEvent, HookEventData, HookInput, HookOutput};
use chrono::Utc;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
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
    ontology_lineage_id: Option<String>,
    retry_store: Option<Arc<dyn ox_store::EmbeddingRetryStore>>,
}

impl EmbeddingHook {
    pub fn new(memory: Arc<MemoryStore>) -> Self {
        Self {
            memory,
            ontology_lineage_id: None,
            retry_store: None,
        }
    }

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

    /// Embed content asynchronously in background — never blocks caller.
    /// Uses content hash as entry ID for automatic deduplication.
    /// Failed embeddings are enqueued for retry when a retry store is provided.
    ///
    /// `context_scope` is **required**: memory writes hit workspace-scoped
    /// RLS-protected tables, so the spawned future needs a scope that carries
    /// `WORKSPACE_ID` / `SYSTEM_BYPASS` across `tokio::spawn`. Callers that
    /// don't have a scope must not call this function — the type signature
    /// enforces that invariant rather than letting the INSERT fail silently
    /// inside the pool's `before_acquire` hook.
    pub fn embed_async(
        memory: &Arc<MemoryStore>,
        content: String,
        source: MemorySource,
        ontology_lineage_id: Option<String>,
        session_id: Option<String>,
        retry_store: Option<&Arc<dyn ox_store::EmbeddingRetryStore>>,
        context_scope: branchforge::SharedContextScope,
    ) {
        if content.trim().is_empty() {
            return;
        }

        let memory = Arc::clone(memory);
        let retry_store = retry_store.cloned();

        // Content-hash ID for deduplication (includes ontology_lineage_id to avoid cross-ontology collisions)
        let mut hasher = Sha256::new();
        if let Some(ref oid) = ontology_lineage_id {
            hasher.update(oid.as_bytes());
        }
        hasher.update(content.as_bytes());
        let entry_id = format!("mem_{:x}", hasher.finalize());

        let metadata = MemoryMetadata {
            source,
            ontology_lineage_id,
            session_id,
            created_at: Utc::now(),
        };

        // Workspace context is explicitly threaded through `context_scope.wrap_tool_future`
        // inside the spawned task, which is the sanctioned agent-side replacement for the
        // `ox-api` spawn helpers.
        #[allow(clippy::disallowed_methods)]
        tokio::spawn(async move {
            let _ = context_scope
                .wrap_tool_future(Box::pin(async move {
                    let _permit = match EMBEDDING_SEMAPHORE.try_acquire() {
                        Ok(p) => p,
                        Err(_) => {
                            warn!("Embedding semaphore full — skipping");
                            return branchforge::ToolResult::success("");
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
                    branchforge::ToolResult::success("")
                }))
                .await;
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
                let Some(scope) = ctx.context_scope.clone() else {
                    // Missing workspace scope means the branchforge runtime
                    // invoked this hook outside a tool-execution context —
                    // i.e. a configuration bug in the caller, not a runtime
                    // condition we can silently recover from. Log at error
                    // level so monitoring picks it up, and skip this
                    // invocation instead of writing a row that would either
                    // get rejected by RLS or, worse, land in the wrong
                    // workspace if app.workspace_id happens to leak in.
                    error!(
                        tool = %tool_name,
                        "EmbeddingHook invoked without context_scope — skipping embed to avoid cross-workspace leak"
                    );
                    return Ok(HookOutput::allow());
                };
                let ontology_lineage_id = self.ontology_lineage_id.clone();
                let session_id = if ctx.session_id.is_empty() {
                    None
                } else {
                    Some(ctx.session_id.clone())
                };
                Self::embed_async(
                    &self.memory,
                    content,
                    source,
                    ontology_lineage_id,
                    session_id,
                    self.retry_store.as_ref(),
                    scope,
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
/// - In-memory tracking per session (DashMap, cleaned up after the configured
///   `session_window_minutes`).
/// - Zero LLM cost: corrections are extracted mechanically from tool outputs.
pub struct RecoveryDetectionHook {
    knowledge_store: Arc<dyn KnowledgeStore>,
    memory: Option<Arc<ox_memory::MemoryStore>>,
    workspace_id: Uuid,
    ontology_name: String,
    ontology_version: i32,
    /// Runtime-tunable thresholds — see [`RecoveryHookConfig`].
    config: RecoveryHookConfig,
    /// Per-session tool outcome tracking: session_id → list of outcomes.
    session_outcomes: DashMap<String, Vec<ToolOutcome>>,
    /// Dedup guard: (session_id, query_hash) tuples already turned into
    /// a knowledge entry. Prevents the same recovery from being recorded
    /// twice when the same successful query appears multiple times.
    processed_recoveries: DashMap<String, std::collections::HashSet<String>>,
}

/// Runtime knobs for `RecoveryDetectionHook`.
///
/// The defaults preserve the previous hard-coded behavior
/// (0.5 Jaccard, 10-minute session window).
#[derive(Debug, Clone, Copy)]
pub struct RecoveryHookConfig {
    /// Minimum Jaccard similarity between failed and successful query
    /// label sets required to treat them as a recovery pair. Below
    /// this threshold the queries probably target unrelated parts of
    /// the schema and the failure→success sequence is coincidental.
    pub jaccard_threshold: f64,
    /// Per-session tracker retention, in minutes. Entries older than
    /// this are purged during periodic cleanup.
    pub session_window_minutes: i64,
}

impl Default for RecoveryHookConfig {
    fn default() -> Self {
        Self {
            jaccard_threshold: 0.5,
            session_window_minutes: 10,
        }
    }
}

/// Distinguishes three outcome states for recovery detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutcomeKind {
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

/// Confidence score to attach to a recovery correction, based on the
/// failure kind that preceded the successful query.
///
/// - `Error → Success`: the model actually hit an exception and then
///   produced a working query — strong evidence for the correction
///   (0.85).
/// - `Empty → Success`: the initial query parsed and ran but returned
///   0 rows; the "correction" might simply be a broader filter. Weaker
///   signal (0.70).
/// - `Success → Success`: not a recovery pair, not intended to be
///   called for this path; neutral fallback (0.0).
pub(crate) fn recovery_confidence_for(failure_kind: OutcomeKind) -> f64 {
    match failure_kind {
        OutcomeKind::Error => 0.85,
        OutcomeKind::Empty => 0.70,
        OutcomeKind::Success => 0.0,
    }
}

impl RecoveryDetectionHook {
    pub fn new(
        knowledge_store: Arc<dyn KnowledgeStore>,
        memory: Option<Arc<ox_memory::MemoryStore>>,
        workspace_id: Uuid,
        ontology_name: String,
        ontology_version: i32,
        config: RecoveryHookConfig,
    ) -> Self {
        Self {
            knowledge_store,
            memory,
            workspace_id,
            ontology_name,
            ontology_version,
            config,
            session_outcomes: DashMap::new(),
            processed_recoveries: DashMap::new(),
        }
    }

    /// Periodic cleanup: remove entries older than the configured
    /// `session_window_minutes`.
    fn cleanup_stale_sessions(&self) {
        let cutoff = Utc::now() - chrono::Duration::minutes(self.config.session_window_minutes);
        self.session_outcomes
            .retain(|_, outcomes| outcomes.last().is_some_and(|o| o.timestamp > cutoff));
        // Dedup tracker shadows session_outcomes — drop entries for
        // sessions that no longer have any outcomes recorded.
        self.processed_recoveries
            .retain(|sid, _| self.session_outcomes.contains_key(sid));
    }

    /// Drop both per-session maps in lockstep. Use this everywhere a
    /// session is "done" (recovery extracted, success without prior
    /// failure, etc.) so `processed_recoveries` cannot accumulate
    /// orphan entries after `session_outcomes` is wiped.
    fn forget_session(&self, session_id: &str) {
        self.session_outcomes.remove(session_id);
        self.processed_recoveries.remove(session_id);
    }
}

/// Extract `:Label` tokens from a Cypher query.
/// Used by [`is_structural_match`] to compare failed vs. successful queries.
///
/// Rules:
/// - A `:` followed by an identifier (alphanumerics, `_`, or non-ASCII Unicode)
///   is treated as a label only when the scanner is *not* inside a map
///   literal (tracked via brace depth). This rules out false positives
///   like `{name: "x"}` where `name:` is a property key, not a label.
/// - Identifier first char may be a letter, `_`, or non-ASCII char.
///   (A leading `_` is valid in internal/system labels such as
///   `_internal` or `_migration_tag`.)
/// - Labels inside map literals are suppressed; property keys in map
///   literals must start with a letter or `_` in Cypher anyway, so
///   this never drops a real label.
fn extract_cypher_labels(query: &str) -> std::collections::HashSet<String> {
    let mut labels = std::collections::HashSet::new();
    let bytes = query.as_bytes();
    let mut i = 0;
    // Track depth of `{ ... }` so property keys like `{name: "x"}` don't
    // leak into the label set.
    let mut brace_depth: usize = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                brace_depth += 1;
                i += 1;
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                i += 1;
            }
            b':' if brace_depth == 0 => {
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
                if end > start
                    && let Ok(label) = std::str::from_utf8(&bytes[start..end])
                    && label
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_' || !c.is_ascii())
                {
                    labels.insert(label.to_string());
                }
                i = end;
            }
            _ => i += 1,
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
/// `threshold`). Missing queries or empty label sets fall through to
/// `false` because we have no signal on which to base a match.
fn is_structural_match(
    failed_query: Option<&str>,
    succeeded_query: Option<&str>,
    threshold: f64,
) -> bool {
    // No signal on either side — refuse to pair. Used to be `true` on
    // missing-failed-query, but that admitted every "parse error
    // followed by ANY later success" as a recovery, which is exactly
    // the noise the Jaccard gate was meant to suppress.
    let (Some(failed), Some(succeeded)) = (failed_query, succeeded_query) else {
        return false;
    };

    let failed_labels = extract_cypher_labels(failed);
    let succeeded_labels = extract_cypher_labels(succeeded);

    match jaccard(&failed_labels, &succeeded_labels) {
        Some(score) => score >= threshold,
        // Both sides extracted zero labels: the queries are too
        // unstructured to compare. Refuse to pair (safer than the
        // previous "preserve old behavior" leniency).
        None => false,
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
                    if !is_structural_match(
                        failure_compiled.as_deref(),
                        success_query.as_deref(),
                        self.config.jaccard_threshold,
                    ) {
                        // Not a real recovery pair — discard outcomes and bail
                        self.forget_session(session_id);
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
                            self.forget_session(session_id);
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
                        confidence: recovery_confidence_for(failure_kind),
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

                    // Non-blocking knowledge persistence. `create_knowledge_entry`
                    // writes to a workspace-scoped RLS table, so the spawned
                    // future needs a scope carrying WORKSPACE_ID. Missing scope
                    // is a configuration bug — log at error level and skip
                    // this persistence, but keep the surrounding cleanup work
                    // running (it's idempotent and doesn't depend on scope).
                    match ctx.context_scope.clone() {
                        Some(scope) => {
                            let store = Arc::clone(&self.knowledge_store);
                            // `scope.wrap_tool_future` reapplies the caller's
                            // workspace context inside the spawned future, so
                            // this is a legitimate cross-boundary spawn.
                            #[allow(clippy::disallowed_methods)]
                            tokio::spawn(async move {
                                let _ = scope
                                    .wrap_tool_future(Box::pin(async move {
                                        match store.create_knowledge_entry(&entry).await {
                                            Ok(()) => info!(
                                                ontology = %entry.ontology_name,
                                                "Knowledge correction from recovery detection"
                                            ),
                                            Err(e) => warn!(
                                                error = %e,
                                                "Failed to save recovery correction"
                                            ),
                                        }
                                        branchforge::ToolResult::success("")
                                    }))
                                    .await;
                            });
                        }
                        None => {
                            error!(
                                session_id = %session_id,
                                "RecoveryDetectionHook invoked without context_scope — skipping knowledge persist"
                            );
                        }
                    }

                    // Clean stale session memories (poisoned by failed queries).
                    //
                    // `tokio::spawn` detaches from the caller's task-local
                    // scope, so the `memory_entries` INSERT/DELETE would hit
                    // the pool's `before_acquire` hook without a workspace
                    // binding and fall through to RLS deny-all. This is a
                    // cross-session background sweep — scope SYSTEM_BYPASS
                    // explicitly so the cleanup runs regardless of whose
                    // session we're pruning.
                    if let Some(ref memory) = self.memory {
                        let sid = session_id.to_string();
                        let mem = Arc::clone(memory);
                        // SYSTEM_BYPASS is scoped inline so the spawned
                        // future has the task-local it needs.
                        #[allow(clippy::disallowed_methods)]
                        tokio::spawn(
                            ox_store::SYSTEM_BYPASS.scope(true, async move {
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
                            }),
                        );
                    }

                    // Clear session outcomes after extraction
                    self.forget_session(session_id);
                } else {
                    // Success with no prior failure — clean up to prevent unbounded growth.
                    self.forget_session(session_id);
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
    fn extract_cypher_labels_ignores_map_literal_keys() {
        // `{name: "x"}` is a map literal — `name` must not be treated as a label.
        let labels = extract_cypher_labels(r#"MATCH (n {name: "x"}) RETURN n"#);
        assert!(labels.is_empty(), "no labels expected, got {labels:?}");
    }

    #[test]
    fn extract_cypher_labels_ignores_map_keys_with_label() {
        // Real label + map literal — only the real label must be emitted.
        let labels = extract_cypher_labels(r#"MATCH (n:Person {name: "x", age: 30}) RETURN n"#);
        assert_eq!(labels, std::collections::HashSet::from(["Person".into()]));
    }

    #[test]
    fn extract_cypher_labels_allows_leading_underscore() {
        // `_internal`, `_migration_tag`, etc. are valid internal label
        // conventions that the previous extractor refused to accept.
        let labels = extract_cypher_labels("MATCH (n:_internal) RETURN n");
        assert_eq!(
            labels,
            std::collections::HashSet::from(["_internal".into()])
        );
    }

    #[test]
    fn extract_cypher_labels_nested_map_literal() {
        // Nested map — the inner `:` should stay suppressed too.
        let labels = extract_cypher_labels(r#"MATCH (n:Person {meta: {k: "v"}}) RETURN n"#);
        assert_eq!(labels, std::collections::HashSet::from(["Person".into()]));
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

    /// Default Jaccard threshold used by the hook (kept separate here
    /// so tests document the value they're anchoring).
    const DEFAULT_THRESHOLD: f64 = 0.5;

    #[test]
    fn structural_match_at_threshold() {
        assert!(is_structural_match(
            Some("MATCH (n:Person) RETURN n"),
            Some("MATCH (p:Person {name: 'x'}) RETURN p"),
            DEFAULT_THRESHOLD,
        ));
    }

    #[test]
    fn structural_match_no_overlap_rejected() {
        assert!(!is_structural_match(
            Some("MATCH (a:Order) RETURN a"),
            Some("MATCH (b:Customer) RETURN b"),
            DEFAULT_THRESHOLD,
        ));
    }

    #[test]
    fn structural_match_partial_overlap() {
        // {Person, BOUGHT, Order} vs {Person, VIEWED, Product} → |∩|=1, |∪|=5 → 0.20 < 0.5
        assert!(!is_structural_match(
            Some("MATCH (p:Person)-[:BOUGHT]->(o:Order) RETURN p"),
            Some("MATCH (p:Person)-[:VIEWED]->(pd:Product) RETURN p"),
            DEFAULT_THRESHOLD,
        ));
    }

    #[test]
    fn structural_match_failure_with_no_compiled_query_rejected() {
        // Tightened: missing failed query yields no signal — refuse
        // to pair (previous behavior of returning true admitted noise).
        assert!(!is_structural_match(
            None,
            Some("MATCH (n:Person) RETURN n"),
            DEFAULT_THRESHOLD,
        ));
    }

    #[test]
    fn structural_match_no_success_query_rejected() {
        assert!(!is_structural_match(
            Some("MATCH (n:Person) RETURN n"),
            None,
            DEFAULT_THRESHOLD,
        ));
    }

    #[test]
    fn structural_match_threshold_is_respected() {
        // Two queries with partial overlap (Jaccard = 0.20) — passes
        // with threshold 0.2, fails with threshold 0.5.
        let a = Some("MATCH (p:Person)-[:BOUGHT]->(o:Order) RETURN p");
        let b = Some("MATCH (p:Person)-[:VIEWED]->(pd:Product) RETURN p");
        assert!(is_structural_match(a, b, 0.2));
        assert!(!is_structural_match(a, b, 0.5));
    }

    #[test]
    fn recovery_confidence_scales_with_failure_kind() {
        // Hard error → confident correction.
        assert!((recovery_confidence_for(OutcomeKind::Error) - 0.85).abs() < f64::EPSILON);
        // 0-row refinement → weaker signal.
        assert!((recovery_confidence_for(OutcomeKind::Empty) - 0.70).abs() < f64::EPSILON);
        // Success arm is unreachable in practice; the helper still
        // returns a safe neutral fallback.
        assert_eq!(recovery_confidence_for(OutcomeKind::Success), 0.0);

        // Ordering contract: hard-error confidence must strictly exceed
        // empty-refinement confidence.
        assert!(
            recovery_confidence_for(OutcomeKind::Error)
                > recovery_confidence_for(OutcomeKind::Empty)
        );
    }

    #[test]
    fn recovery_hook_config_defaults_match_legacy_constants() {
        // Preserves the previous hard-coded thresholds, so routes
        // that omit `[recovery]` get the same behavior as before.
        let c = RecoveryHookConfig::default();
        assert!((c.jaccard_threshold - 0.5).abs() < f64::EPSILON);
        assert_eq!(c.session_window_minutes, 10);
    }
}
