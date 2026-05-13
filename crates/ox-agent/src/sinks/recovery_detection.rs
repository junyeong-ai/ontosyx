//! `RecoveryDetectionSink` — detect query_graph failure → success
//! patterns within one agent run and persist a verified `correction`
//! row in the knowledge bank so future RAG injects the lesson.
//!
//! [`AgentEvent::ToolComplete`] / [`AgentEvent::ToolError`] deliver
//! structured outcomes directly — `ToolError` *is* the error signal,
//! so failure detection collapses into a typed match.
//!
//! ## Keying — `run_id`
//!
//! [`AgentEvent`] carries `run_id` (per-`Agent::execute` correlation),
//! and the recovery pattern (fail → succeed within one agent run)
//! lives entirely under one `run_id` — one chat turn maps to one
//! run, so the semantic match is exact. Cross-conversation recovery
//! tracking that spans multiple runs is a future extension keyed on
//! [`ExecutionContext::thread_id`].

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use entelix::{AgentEvent, AgentEventSink, ReActState};
use ox_context::ContextScope;
use ox_store::{KnowledgeEntry, KnowledgeKind, KnowledgeStatus, KnowledgeStore};
use serde_json::Value;
use tracing::{info, warn};
use uuid::Uuid;

/// Runtime knobs for [`RecoveryDetectionSink`].
#[derive(Debug, Clone, Copy)]
pub struct RecoveryDetectionConfig {
    /// Minimum Jaccard similarity between failed and successful query
    /// label sets required to treat them as a recovery pair.
    pub jaccard_threshold: f64,
    /// Per-run tracker retention, in minutes. Entries older than this
    /// are purged on every cleanup tick.
    pub run_window_minutes: i64,
}

impl Default for RecoveryDetectionConfig {
    fn default() -> Self {
        Self {
            jaccard_threshold: 0.5,
            run_window_minutes: 10,
        }
    }
}

/// Three-way outcome classification for a `query_graph` dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeKind {
    /// Tool returned an error.
    Error,
    /// Tool succeeded but query returned 0 rows.
    Empty,
    /// Tool succeeded with `row_count > 0`.
    Success,
}

struct ToolOutcome {
    kind: OutcomeKind,
    text: String,
    compiled_query: Option<String>,
    timestamp: chrono::DateTime<Utc>,
}

/// Confidence to attach to a recovery correction, by failure kind.
fn recovery_confidence_for(failure_kind: OutcomeKind) -> f64 {
    match failure_kind {
        OutcomeKind::Error => 0.85,
        OutcomeKind::Empty => 0.70,
        OutcomeKind::Success => 0.0,
    }
}

/// Watches `query_graph` outcomes per agent run and creates verified
/// `correction` knowledge rows on detected failure → success pairs.
pub struct RecoveryDetectionSink {
    knowledge_store: Arc<dyn KnowledgeStore>,
    memory: Option<Arc<ox_memory::MemoryStore>>,
    workspace_id: Uuid,
    ontology_name: String,
    ontology_version: i32,
    config: RecoveryDetectionConfig,
    run_outcomes: DashMap<String, Vec<ToolOutcome>>,
    /// Dedup guard — `(run_id, query_hash)` tuples already turned into
    /// a knowledge entry. Prevents duplicate rows when the same
    /// successful query recovers from the same failure repeatedly.
    processed_recoveries: DashMap<String, HashSet<String>>,
}

impl RecoveryDetectionSink {
    /// Construct a sink bound to one ontology + workspace.
    #[must_use]
    pub fn new(
        knowledge_store: Arc<dyn KnowledgeStore>,
        memory: Option<Arc<ox_memory::MemoryStore>>,
        workspace_id: Uuid,
        ontology_name: String,
        ontology_version: i32,
        config: RecoveryDetectionConfig,
    ) -> Self {
        Self {
            knowledge_store,
            memory,
            workspace_id,
            ontology_name,
            ontology_version,
            config,
            run_outcomes: DashMap::new(),
            processed_recoveries: DashMap::new(),
        }
    }

    fn cleanup_stale_runs(&self) {
        let cutoff = Utc::now() - chrono::Duration::minutes(self.config.run_window_minutes);
        self.run_outcomes
            .retain(|_, outcomes| outcomes.last().is_some_and(|o| o.timestamp > cutoff));
        self.processed_recoveries
            .retain(|run_id, _| self.run_outcomes.contains_key(run_id));
    }

    fn forget_run(&self, run_id: &str) {
        self.run_outcomes.remove(run_id);
        self.processed_recoveries.remove(run_id);
    }

    fn record(&self, run_id: &str, outcome: ToolOutcome) {
        self.run_outcomes
            .entry(run_id.to_owned())
            .or_default()
            .push(outcome);
    }

    /// Pull the most recent failure (Error or Empty) older than the
    /// last outcome, skipping the current success. Returns the
    /// failure's `(kind, text, compiled_query)` tuple when present.
    fn find_prior_failure(&self, run_id: &str) -> Option<(OutcomeKind, String, Option<String>)> {
        self.run_outcomes.get(run_id).and_then(|outcomes| {
            outcomes
                .iter()
                .rev()
                .skip(1)
                .find(|o| matches!(o.kind, OutcomeKind::Error | OutcomeKind::Empty))
                .map(|o| (o.kind, o.text.clone(), o.compiled_query.clone()))
        })
    }
}

#[async_trait]
impl AgentEventSink<ReActState> for RecoveryDetectionSink {
    async fn send(&self, event: AgentEvent<ReActState>) -> entelix::Result<()> {
        match event {
            AgentEvent::ToolError {
                tool,
                run_id,
                error_for_llm,
                ..
            } => {
                if tool != "query_graph" {
                    return Ok(());
                }
                self.record(
                    &run_id,
                    ToolOutcome {
                        kind: OutcomeKind::Error,
                        text: error_for_llm.as_inner().clone(),
                        compiled_query: None,
                        timestamp: Utc::now(),
                    },
                );
            }
            AgentEvent::ToolComplete {
                tool,
                output,
                run_id,
                ..
            } => {
                if tool != "query_graph" {
                    return Ok(());
                }
                let (compiled_query, row_count) = parse_query_metrics(&output);
                let kind = if row_count == 0 {
                    OutcomeKind::Empty
                } else {
                    OutcomeKind::Success
                };
                let text = output.to_string();
                self.record(
                    &run_id,
                    ToolOutcome {
                        kind,
                        text: text.clone(),
                        compiled_query: compiled_query.clone(),
                        timestamp: Utc::now(),
                    },
                );

                if kind == OutcomeKind::Success {
                    self.handle_success(run_id, output, compiled_query).await;
                }

                if self.run_outcomes.len() > 50 {
                    self.cleanup_stale_runs();
                }
            }
            // Other variants (Started, ToolStart, ToolCallApproved/Denied, Failed, Complete)
            // carry no recovery signal. The fallback arm satisfies
            // #[non_exhaustive].
            _ => {}
        }
        Ok(())
    }
}

impl RecoveryDetectionSink {
    async fn handle_success(
        &self,
        run_id: String,
        success_output: Value,
        success_compiled_query: Option<String>,
    ) {
        let Some((failure_kind, failure_text, failure_compiled)) = self.find_prior_failure(&run_id)
        else {
            // Success without prior failure — drop the run's outcomes
            // to bound memory growth.
            self.forget_run(&run_id);
            return;
        };

        let (success_query, labels, execution_id) = parse_success_output(&success_output);
        let success_compiled = success_compiled_query
            .as_deref()
            .or(success_query.as_deref());

        if !is_structural_match(
            failure_compiled.as_deref(),
            success_compiled,
            self.config.jaccard_threshold,
        ) {
            self.forget_run(&run_id);
            return;
        }

        let dedup_key = success_compiled.map(ox_brain::knowledge_util::content_hash_query);
        if let Some(ref key) = dedup_key {
            let mut hashes = self.processed_recoveries.entry(run_id.clone()).or_default();
            if !hashes.insert(key.clone()) {
                self.forget_run(&run_id);
                return;
            }
        }

        let run_short = &run_id[..8.min(run_id.len())];
        let Some((title, content, extraction_method)) = (match failure_kind {
            OutcomeKind::Error => {
                let error_excerpt = clamp(&failure_text, 200);
                Some((
                    format!("Recovery: query_graph failed then succeeded in run {run_short}"),
                    format!(
                        "Failed: {}\nCorrection: {}",
                        error_excerpt,
                        success_compiled.unwrap_or("(successful query)"),
                    ),
                    "recovery_detection",
                ))
            }
            OutcomeKind::Empty => Some((
                format!("Refinement: query_graph empty then succeeded in run {run_short}"),
                format!(
                    "Empty (0 rows): {}\nCorrection: {}",
                    failure_compiled.as_deref().unwrap_or("(unknown query)"),
                    success_compiled.unwrap_or("(successful query)"),
                ),
                "zero_row_recovery",
            )),
            OutcomeKind::Success => None,
        }) else {
            // Reached only if the prior-failure filter regresses; log and
            // bail rather than panic.
            warn!("RecoveryDetectionSink: success outcome reached recovery match arm");
            return;
        };

        let hash = ox_brain::knowledge_util::content_hash(&self.ontology_name, &content);
        let entry = KnowledgeEntry {
            id: Uuid::new_v4(),
            workspace_id: self.workspace_id,
            ontology_name: self.ontology_name.clone(),
            ontology_version_min: self.ontology_version,
            ontology_version_max: None,
            kind: KnowledgeKind::Correction,
            status: KnowledgeStatus::Approved,
            confidence: recovery_confidence_for(failure_kind),
            title,
            content,
            structured_data: serde_json::json!({
                "extraction_method": extraction_method,
                "failure_kind": format!("{:?}", failure_kind),
                "success_query": success_compiled,
                "success_execution_id": execution_id,
            }),
            embedding: None,
            version_checked: self.ontology_version,
            content_hash: hash,
            source_execution_ids: execution_id
                .as_deref()
                .and_then(|id| Uuid::parse_str(id).ok())
                .into_iter()
                .collect(),
            // run_id is per-execute (UUID v7); only stamp when it
            // parses cleanly so the audit join column stays well-formed.
            source_session_id: Uuid::parse_str(&run_id).ok(),
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
            // The recovery sink lives in ox-agent which doesn't pull
            // the lindera tokenizer. Leave the searchable surface empty;
            // the next workspace tokenizer publish backfill (or the
            // scheduled retokenize sweep) lands the tokens.
            tokenized_text: String::new(),
            tokenizer_dict_fingerprint: String::new(),
        };

        // Capture RLS task-locals at sink fire — the agent runtime
        // holds them while polling sinks, so this read sees the active
        // workspace; re-applying inside the spawn keeps the
        // knowledge-store write under the right tenant.
        let store = Arc::clone(&self.knowledge_store);

        ContextScope::capture_current().spawn(async move {
            match store.create_knowledge_entry(&entry).await {
                Ok(()) => info!(
                    ontology = %entry.ontology_name,
                    "knowledge correction from recovery detection"
                ),
                Err(e) => warn!(error = %e, "failed to save recovery correction"),
            }
        });

        // Clean stale memory rows (poisoned by the failed query). Cross-
        // tenant sweep — system-bypass is appropriate because the
        // cleanup runs regardless of whose run we're pruning.
        if let Some(ref memory) = self.memory {
            let sid = run_id.clone();
            let mem = Arc::clone(memory);
            ox_context::spawn_system(async move {
                match mem.cleanup_by_session(&sid).await {
                    Ok(n) if n > 0 => info!(
                        run_id = %sid,
                        count = n,
                        "cleaned stale session memories after recovery"
                    ),
                    Err(e) => warn!(error = %e, "failed to clean stale session memories"),
                    _ => {}
                }
            });
        }

        self.forget_run(&run_id);
    }
}

fn parse_query_metrics(output: &Value) -> (Option<String>, usize) {
    let compiled_query = output
        .get("compiled_query")
        .and_then(|v| v.as_str())
        .map(String::from);
    let row_count = output
        .get("row_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    (compiled_query, row_count)
}

fn parse_success_output(output: &Value) -> (Option<String>, Vec<String>, Option<String>) {
    let compiled_query = output
        .get("compiled_query")
        .and_then(|v| v.as_str())
        .map(String::from);
    let execution_id = output
        .get("execution_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    let labels: Vec<String> = output
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

fn extract_cypher_labels(query: &str) -> HashSet<String> {
    let mut labels = HashSet::new();
    let bytes = query.as_bytes();
    let mut i = 0;
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

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> Option<f64> {
    let union = a.union(b).count();
    if union == 0 {
        return None;
    }
    let intersection = a.intersection(b).count();
    Some(intersection as f64 / union as f64)
}

fn is_structural_match(
    failed_query: Option<&str>,
    succeeded_query: Option<&str>,
    threshold: f64,
) -> bool {
    let (Some(failed), Some(succeeded)) = (failed_query, succeeded_query) else {
        return false;
    };
    let failed_labels = extract_cypher_labels(failed);
    let succeeded_labels = extract_cypher_labels(succeeded);
    match jaccard(&failed_labels, &succeeded_labels) {
        Some(score) => score >= threshold,
        None => false,
    }
}

fn clamp(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
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
        assert_eq!(labels, HashSet::from(["Person".into()]));
    }

    #[test]
    fn extract_cypher_labels_ignores_map_literal_keys() {
        let labels = extract_cypher_labels(r#"MATCH (n {name: "x"}) RETURN n"#);
        assert!(labels.is_empty(), "no labels expected, got {labels:?}");
    }

    #[test]
    fn extract_cypher_labels_ignores_map_keys_with_label() {
        let labels = extract_cypher_labels(r#"MATCH (n:Person {name: "x"}) RETURN n"#);
        assert_eq!(labels, HashSet::from(["Person".into()]));
    }

    #[test]
    fn jaccard_basic() {
        let a: HashSet<String> = ["A", "B", "C"].iter().map(|s| s.to_string()).collect();
        let b: HashSet<String> = ["B", "C", "D"].iter().map(|s| s.to_string()).collect();
        assert_eq!(jaccard(&a, &b), Some(0.5));
    }

    #[test]
    fn jaccard_disjoint_is_zero() {
        let a: HashSet<String> = ["A"].iter().map(|s| s.to_string()).collect();
        let b: HashSet<String> = ["B"].iter().map(|s| s.to_string()).collect();
        assert_eq!(jaccard(&a, &b), Some(0.0));
    }

    #[test]
    fn jaccard_both_empty_returns_none() {
        let a = HashSet::new();
        let b = HashSet::new();
        assert_eq!(jaccard(&a, &b), None);
    }

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
    fn structural_match_failure_with_no_compiled_query_rejected() {
        assert!(!is_structural_match(
            None,
            Some("MATCH (n:Person) RETURN n"),
            DEFAULT_THRESHOLD,
        ));
    }

    #[test]
    fn recovery_confidence_scales_with_failure_kind() {
        assert!((recovery_confidence_for(OutcomeKind::Error) - 0.85).abs() < f64::EPSILON);
        assert!((recovery_confidence_for(OutcomeKind::Empty) - 0.70).abs() < f64::EPSILON);
        assert_eq!(recovery_confidence_for(OutcomeKind::Success), 0.0);
        assert!(
            recovery_confidence_for(OutcomeKind::Error)
                > recovery_confidence_for(OutcomeKind::Empty)
        );
    }
}
