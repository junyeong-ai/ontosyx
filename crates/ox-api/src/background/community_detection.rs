//! Community detection cron (Φ10.4).
//!
//! Per-workspace sweep that runs Leiden over the canonical
//! ontology graph and upserts the resulting hierarchical
//! partition into `community_summaries`. The GraphRAG
//! retrieval path consumes those rows alongside entity-level
//! matches.
//!
//! ## LLM summarization
//!
//! Each detected community feeds through
//! [`ox_brain::CommunitySummarizer::summarize_community`] to
//! produce the prose `summary` + headline `title` the
//! retrieval path's trigram match indexes against. Without
//! prose summaries the structural member listing is the only
//! anchor the trigram ranker has — and logical-id strings
//! rarely line up with operator vocabulary, so retrieval
//! quality collapses. The prose summary IS the GraphRAG
//! retrieval value.
//!
//! ### Cost guard via membership fingerprint
//!
//! Each community carries a sha256 over its sorted
//! `(kind, logical_id)` member set. Before invoking the
//! summarizer the cron loads the existing row (if any) by
//! `(version_id, community_id)` and compares fingerprints —
//! an unchanged fingerprint short-circuits the LLM call. Two
//! consecutive cron ticks against an unchanged ontology
//! version fire zero LLM calls; only structural drift
//! re-summarizes. Bounding the LLM cost to "actual structural
//! change" — not "every cron tick".
//!
//! ## Singleton + workspace fan
//!
//! Singleton-locked via
//! [`ADVISORY_LOCK_CRON_COMMUNITY_DETECTION`] so two replicas
//! don't race on the same UPSERT. Cross-workspace fan-out via
//! `list_workspace_ids` under `SYSTEM_BYPASS`; per-workspace
//! work runs inside `WORKSPACE_ID.scope(ws_id, ...)` so RLS
//! pins every read + write to the correct tenant.
//!
//! ## Cadence
//!
//! Default sweep interval is 6 hours. Schema-level community
//! structure changes only when the operator commits a new
//! ontology version; the cron's only job between commits is
//! to detect drift and refresh stale prose. The fingerprint
//! check makes the inter-commit cost essentially zero.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ox_core::i18n::LocalizedText;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use ox_brain::{Brain, CommunitySummaryMember, CommunitySummaryRequest};
use ox_core::error::OxResult;
use ox_ontology::CommunityDetectionPolicy;
use ox_ontology::community_detection::{
    CommunityGraph, DetectionResult, build_ontology_graph, detect_communities,
};
use ox_store::Store;
use ox_store::community::CommunitySummary;

use super::cron::{CronTask, spawn_cron};

const SWEEP_INTERVAL: Duration = Duration::from_secs(6 * 3600);

struct CommunityDetectionSweep {
    store: Arc<dyn Store>,
    brain: Arc<dyn Brain>,
    tokenizer_registry: Arc<ox_text::WorkspaceTokenizerRegistry>,
    embedder: Option<Arc<dyn ox_memory::EmbeddingProvider>>,
}

#[async_trait]
impl CronTask for CommunityDetectionSweep {
    fn name(&self) -> &'static str {
        "community-detection-sweep"
    }

    fn interval(&self) -> Duration {
        SWEEP_INTERVAL
    }

    fn singleton_key(&self) -> Option<i64> {
        Some(*ox_store::advisory_lock::ADVISORY_LOCK_CRON_COMMUNITY_DETECTION)
    }

    async fn run_once(&self) -> OxResult<()> {
        run_sweep(
            self.store.as_ref(),
            self.brain.as_ref(),
            &self.tokenizer_registry,
            self.embedder.as_ref(),
        )
        .await
    }
}

pub fn spawn_community_detection_sweep(
    store: Arc<dyn Store>,
    brain: Arc<dyn Brain>,
    tokenizer_registry: Arc<ox_text::WorkspaceTokenizerRegistry>,
    embedder: Option<Arc<dyn ox_memory::EmbeddingProvider>>,
    pool: ox_store::PgPool,
    cancel: CancellationToken,
) {
    spawn_cron(
        Arc::new(CommunityDetectionSweep {
            store,
            brain,
            tokenizer_registry,
            embedder,
        }),
        Some(pool),
        cancel,
    );
}

async fn run_sweep(
    store: &dyn Store,
    brain: &dyn Brain,
    tokenizer_registry: &ox_text::WorkspaceTokenizerRegistry,
    embedder: Option<&Arc<dyn ox_memory::EmbeddingProvider>>,
) -> OxResult<()> {
    let workspace_ids = store.list_workspace_ids().await?;
    if workspace_ids.is_empty() {
        return Ok(());
    }

    let mut workspaces_scanned = 0usize;
    let mut workspaces_skipped = 0usize;
    let mut total_communities = 0usize;
    let mut total_summarized = 0usize;
    let mut total_errors = 0usize;

    for ws_id in workspace_ids {
        let tokenizer = tokenizer_registry.for_workspace(ws_id);
        let outcome = ox_store::WORKSPACE_ID
            .scope(ws_id, async {
                sweep_workspace(store, brain, tokenizer.as_ref().as_ref(), embedder).await
            })
            .await;

        match outcome {
            Ok(WorkspaceSweepReport::Skipped) => workspaces_skipped += 1,
            Ok(WorkspaceSweepReport::Scanned {
                communities_emitted,
                communities_summarized,
            }) => {
                workspaces_scanned += 1;
                total_communities += communities_emitted;
                total_summarized += communities_summarized;
            }
            Err(e) => {
                warn!(
                    workspace_id = %ws_id,
                    error = %e,
                    "community detection sweep: workspace scan failed",
                );
                total_errors += 1;
            }
        }
    }

    if workspaces_scanned + workspaces_skipped + total_errors > 0 {
        info!(
            workspaces_scanned,
            workspaces_skipped,
            total_communities,
            total_summarized,
            total_errors,
            "community detection sweep tick complete",
        );
    }
    Ok(())
}

enum WorkspaceSweepReport {
    /// Skipped — workspace lacks a prerequisite (canonical
    /// ontology, non-empty graph). Not an error; not every
    /// workspace is at the stage where detection makes sense.
    Skipped,
    Scanned {
        communities_emitted: usize,
        /// Subset of `communities_emitted` whose membership
        /// changed since the last run and were re-summarized
        /// via the LLM. The complement was served from the
        /// stored row's prose (fingerprint match → no LLM call).
        communities_summarized: usize,
    },
}

async fn sweep_workspace(
    store: &dyn Store,
    brain: &dyn Brain,
    tokenizer: &dyn ox_text::Tokenizer,
    embedder: Option<&Arc<dyn ox_memory::EmbeddingProvider>>,
) -> OxResult<WorkspaceSweepReport> {
    // Resolve canonical ontology + version + IR.
    let Some(ontology) = store.get_workspace_ontology().await? else {
        return Ok(WorkspaceSweepReport::Skipped);
    };
    let Some(snapshot) = store.find_current_version(ontology.id).await? else {
        return Ok(WorkspaceSweepReport::Skipped);
    };
    let Some(ontology_ir) = store.get_ontology_ir(snapshot.id).await? else {
        return Ok(WorkspaceSweepReport::Skipped);
    };

    let policy = match store
        .find_community_detection_policy_by_name("default")
        .await?
    {
        Some(p) => p,
        None => {
            let ws_id = ox_store::WORKSPACE_ID.try_with(|id| *id).map_err(|_| {
                ox_core::error::OxError::Runtime {
                    message: "community detection sweep: workspace context missing".into(),
                }
            })?;
            CommunityDetectionPolicy::workspace_default(ws_id)
        }
    };

    let graph = build_ontology_graph(&ontology_ir);
    if graph.is_empty() {
        return Ok(WorkspaceSweepReport::Skipped);
    }

    let detection = match detect_communities(&graph, &policy) {
        Ok(r) => r,
        Err(err) => {
            warn!(
                workspace_id = %ontology.workspace_id,
                error = %err,
                "community detection: algorithm run failed",
            );
            return Ok(WorkspaceSweepReport::Skipped);
        }
    };

    let workspace_name = workspace_display_name(&ontology_ir.display_name, &ontology_ir.name);
    let tokenizer_dict_fingerprint = ox_text::glossary_tokenizer_fingerprint(&ontology_ir)
        .as_str()
        .to_string();

    let (communities_emitted, communities_summarized) = persist_partition(
        store,
        brain,
        tokenizer,
        &tokenizer_dict_fingerprint,
        embedder,
        &workspace_name,
        ontology.workspace_id,
        snapshot.id,
        &policy,
        &graph,
        &detection,
    )
    .await?;

    Ok(WorkspaceSweepReport::Scanned {
        communities_emitted,
        communities_summarized,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "persistence boundary: each parameter is an independent dependency (store / brain / \
              tokenizer / embedder) or identity axis (workspace_id / workspace_name / \
              ontology_version_id) or input shape (policy / graph / detection). Bundling into a \
              struct would only move the explosion to the construction site without reducing the \
              fan-in"
)]
async fn persist_partition(
    store: &dyn Store,
    brain: &dyn Brain,
    tokenizer: &dyn ox_text::Tokenizer,
    tokenizer_dict_fingerprint: &str,
    embedder: Option<&Arc<dyn ox_memory::EmbeddingProvider>>,
    workspace_name: &str,
    workspace_id: Uuid,
    ontology_version_id: Uuid,
    policy: &CommunityDetectionPolicy,
    graph: &CommunityGraph,
    detection: &DetectionResult,
) -> OxResult<(usize, usize)> {
    let min_size = policy.min_cluster_size as usize;
    let mut emitted = 0usize;
    let mut summarized = 0usize;

    for community in &detection.communities {
        if community.members.len() < min_size {
            continue;
        }

        let community_id = format!("leiden:{}", community.local_id);

        // Member-set sorted by (kind, logical_id) for the
        // fingerprint canon AND for the LLM input — same order
        // means stable prompt render hash + stable fingerprint.
        let mut sorted_members: Vec<_> = community
            .members
            .iter()
            .map(|&idx| {
                let node = &graph.nodes[idx];
                (
                    node.kind.as_str().to_string(),
                    node.logical_id.clone(),
                    node.display_name.clone(),
                )
            })
            .collect();
        sorted_members.sort();

        let member_entity_kinds: Vec<String> =
            sorted_members.iter().map(|(k, _, _)| k.clone()).collect();
        let member_logical_ids: Vec<String> =
            sorted_members.iter().map(|(_, l, _)| l.clone()).collect();
        let member_fingerprint =
            CommunitySummary::compute_member_fingerprint(&member_entity_kinds, &member_logical_ids);

        // Fingerprint check: same membership → reuse stored
        // prose, skip the LLM call. Only `generated_at`
        // refreshes (so the dashboard's "last seen" timestamp
        // stays honest).
        let existing = store
            .find_community_summary_by_natural_key(ontology_version_id, &community_id)
            .await?;

        let (title, summary, summarized_now) = match existing {
            Some(prior) if prior.member_fingerprint == member_fingerprint => {
                (prior.title, prior.summary, false)
            }
            _ => {
                let request = CommunitySummaryRequest {
                    workspace_name,
                    members: &sorted_members
                        .iter()
                        .map(|(k, l, d)| CommunitySummaryMember {
                            kind: k.as_str(),
                            logical_id: l.as_str(),
                            display_name: d.as_str(),
                        })
                        .collect::<Vec<_>>(),
                };
                match brain
                    .summarize_community(request, &entelix::ExecutionContext::default())
                    .await
                {
                    Ok((response, _provenance)) => (response.title, response.summary, true),
                    Err(err) => {
                        // LLM failure is non-fatal — fall back
                        // to a structural placeholder so the
                        // retrieval path still has *something*
                        // to anchor on. Log so the dashboard
                        // can surface chronic LLM failure.
                        warn!(
                            community_id = %community_id,
                            workspace_id = %workspace_id,
                            error = %err,
                            "community-summary LLM call failed; persisting structural fallback",
                        );
                        (
                            structural_title(community.members.len(), &sorted_members),
                            structural_summary(&sorted_members),
                            false,
                        )
                    }
                }
            }
        };

        // Tokenize the searchable surface (title + summary) via
        // the workspace's user-dict-aware tokenizer. Failure
        // falls back to empty `tokenized_text` — the trigram
        // ranker on raw text still serves retrieval; only the
        // tsvector-axis recall degrades.
        let tokenize_input = format!("{} {}", title, summary);
        let tokenized_text = tokenizer.tokenize(&tokenize_input).unwrap_or_else(|err| {
            warn!(
                community_id = %community_id,
                error = %err,
                "tokenize failed for community summary; persisting raw text",
            );
            tokenize_input.clone()
        });

        // Embed the title + summary when an embedder is wired in
        // — same `Arc` the Brain consumes for translate-time NN
        // retrieval. Cosine similarity over the LLM-shaped prose
        // is what the agent's GraphRAG fan reads. Embed failure is
        // non-fatal — the row still lands with `embedding = None`
        // and the trigram + FTS rankers continue to serve hybrid
        // retrieval.
        let embedding = if summarized_now {
            if let Some(provider) = embedder {
                let embed_input = format!("{} {}", title, summary);
                match provider
                    .embed(
                        &embed_input,
                        "Represent the community summary for retrieval",
                        ox_memory::EmbeddingRole::Document,
                    )
                    .await
                {
                    Ok(v) => Some(v),
                    Err(err) => {
                        warn!(
                            community_id = %community_id,
                            error = %err,
                            "community-summary embed failed; persisting without vector",
                        );
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        let row = CommunitySummary {
            id: Uuid::new_v4(),
            workspace_id,
            ontology_version_id,
            community_id,
            level: community.level,
            member_entity_kinds,
            member_logical_ids,
            member_fingerprint,
            title,
            summary,
            tokenized_text,
            tokenizer_dict_fingerprint: tokenizer_dict_fingerprint.to_string(),
            embedding,
            generated_at: chrono::Utc::now(),
        };
        store.upsert_community_summary(&row).await?;
        emitted += 1;
        if summarized_now {
            summarized += 1;
        }
    }

    Ok((emitted, summarized))
}

/// Resolve the workspace's preferred display name for the
/// summarizer prompt. Empty `LocalizedText` falls back to the
/// ontology's logical name; the LLM uses whichever surfaces
/// the operator's vocabulary best.
fn workspace_display_name(display: &LocalizedText, name: &str) -> String {
    let s = display.as_str();
    if s.trim().is_empty() {
        name.to_string()
    } else {
        s.to_string()
    }
}

/// Last-resort structural title. Used only when the LLM call
/// fails — the cron prefers prose. Keeps the retrieval path
/// usable until the next successful tick replaces this with
/// real summary.
fn structural_title(member_count: usize, sorted: &[(String, String, String)]) -> String {
    let display_names: Vec<&str> = sorted
        .iter()
        .filter_map(|(_, _, d)| if d.is_empty() { None } else { Some(d.as_str()) })
        .collect();
    if display_names.is_empty() {
        return format!("Cluster of {member_count} entities");
    }
    let lead: Vec<&str> = display_names.iter().take(3).copied().collect();
    if lead.len() < member_count {
        format!("{} +{} more", lead.join(" / "), member_count - lead.len())
    } else {
        lead.join(" / ")
    }
}

/// Last-resort structural summary. Same fallback rationale as
/// `structural_title`.
fn structural_summary(sorted: &[(String, String, String)]) -> String {
    use std::collections::BTreeMap;
    let mut by_kind: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (kind, logical, _) in sorted {
        by_kind.entry(kind).or_default().push(logical);
    }
    let mut buf = String::from("Structural summary (LLM unavailable). ");
    for (kind, ids) in by_kind {
        let head: Vec<&str> = ids.iter().take(8).copied().collect();
        buf.push_str(&format!("{kind} ({}): ", ids.len()));
        buf.push_str(&head.join(", "));
        if ids.len() > head.len() {
            buf.push_str(&format!(", … (+{} more). ", ids.len() - head.len()));
        } else {
            buf.push_str(". ");
        }
    }
    buf
}
