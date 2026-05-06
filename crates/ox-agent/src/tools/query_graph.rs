use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use ox_brain::plan_router::{CostBudget, HeuristicPlanRouter, PlanRouter, RouteDecision};
use ox_core::error::OxError;
use ox_graph_runtime::cypher::strict_advisory_diagnostics;
use ox_query_ir::resolve_query_bindings;
use ox_store::{
    EntryPointSearchOptions, LlmRenderOptions, NeighborExpandOptions, QueryExecution,
};

use crate::DomainContext;

// ---------------------------------------------------------------------------
// QueryGraphTool — NL → Cypher → Execute → Results → Persist
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryGraphInput {
    /// Natural language question about the graph data.
    pub question: String,
}

/// Tool result returned to the LLM. Carries only what the model needs
/// to reason about the next step — execution metadata (provenance,
/// compiled target, per-step timing, attribution) lives on the
/// persisted `QueryExecution` row and the streaming progress events;
/// the FE renders timing from the SSE stream and provenance via
/// `/api/executions/{id}`.
#[derive(Debug, Serialize)]
struct QueryGraphOutput {
    execution_id: String,
    compiled_query: String,
    columns: Vec<String>,
    row_count: usize,
    rows: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    widget_hint: Option<WidgetHintOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<ox_compiler::cost::QueryCost>,
    #[serde(skip_serializing_if = "Option::is_none")]
    guidance: Option<String>,
    /// Validator diagnostics; the LLM reads a flattened form via
    /// `guidance`, the structured list stays here for the chat UI.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<ox_query_ir::query::QueryDiagnostic>,
    /// Unresolved AmbiguityContext entries the source-analyzer flagged
    /// on columns this query touched. Distinct wire field from
    /// `warnings` because validator diagnostics and ambiguity hints
    /// have different origins (Cypher AST vs source analysis) and
    /// different consumers (LLM error-recovery vs FE chip rendering).
    /// Rendered as deep-link chips in the chat tool-call card —
    /// each chip jumps to the Glossary workbench so the modeller can
    /// bind a term.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    unresolved_ambiguities: Vec<UnresolvedAmbiguityHint>,
}

/// Wire-stable hint pointing at one unresolved AmbiguityContext.
/// FE chips read these directly; the LLM doesn't reason about them
/// (the `guidance` text already nudges the model toward the
/// `resolve_ambiguity` tool, so duplicating that reasoning surface
/// here would be noise).
#[derive(Debug, Serialize)]
struct UnresolvedAmbiguityHint {
    /// `AmbiguityContext.id` — the FE chip uses this as the
    /// `?ambiguity=…` deep-link target on `/glossary`.
    context_id: String,
    /// Source relation (table) the ambiguous column lives in.
    relation: String,
    /// The ambiguous column name itself.
    column: String,
}

#[derive(Debug, Serialize)]
struct WidgetHintOutput {
    widget_type: String,
    title: String,
}

/// Translates natural language to a graph query, executes it, persists the result,
/// and returns structured data.
pub struct QueryGraphTool {
    pub domain: Arc<DomainContext>,
    pub brain: Arc<dyn ox_brain::Brain>,
    /// Refuse to execute `RiskLevel::High` queries (Cartesian products,
    /// unbounded variable-length paths, unindexed high-fanout traversals).
    /// Flip to `false` only when the workspace has consciously opted into
    /// that risk — see `AgentConfig::reject_high_cost`.
    pub reject_high_cost: bool,
}

#[async_trait]
impl SchemaTool for QueryGraphTool {
    type Input = QueryGraphInput;
    const NAME: &'static str = super::QUERY_GRAPH;
    const DESCRIPTION: &'static str = "Execute a natural-language query against the knowledge graph. \
         Include all entities and relationships in one question — the engine handles multi-hop \
         chains (A→B→C→D) in a single query. Do NOT split per entity.";
    const READ_ONLY: bool = true;

    async fn handle(&self, input: Self::Input, ctx: &ExecutionContext) -> ToolResult {
        // Load the current ontology snapshot — a tool that edits the
        // ontology mid-session publishes a replacement into the shared
        // `ArcSwap`, and we pick it up here on the next invocation.
        let ontology = match self.domain.current_ontology() {
            Some(o) => o,
            None => {
                return ToolResult::error(
                    "No ontology loaded. Create an ontology draft from a data source first, \
                     or use introspect_source to connect to a database.",
                );
            }
        };

        let runtime = match self.domain.runtime.as_ref() {
            Some(r) => r,
            None => {
                return ToolResult::error(
                    "Graph database not connected. The workspace needs a deployed schema \
                     with loaded data before queries can execute.",
                );
            }
        };

        let start = std::time::Instant::now();
        let cancel = ctx.cancel_token().cloned();

        let question = input.question.clone();

        // GraphRAG step — walk the OntologyNavigationStore (Postgres-
        // backed Level-3 indexes) to surface a question-anchored
        // subgraph slice for the LLM prompt. Skipped silently when
        // the session has no committed ontology version (ad-hoc
        // draft, system-bypass test) or when navigation calls fail
        // (unavailable index, transient DB blip — the schema RAG
        // path on the Brain side still carries the prompt context).
        let retrieved_subgraph_md =
            try_retrieve_subgraph_md(&self.domain, &question).await;
        if let Some(md) = retrieved_subgraph_md.as_deref() {
            let approx_chars = md.len();
            ctx.progress("graphrag_retrieval").completed_with(
                0,
                serde_json::json!({ "chars": approx_chars }),
            );
        }

        // Step 1: Translate NL → QueryIR (timeout: 60s)
        // Brain emits sub-steps (schema_discovery, llm_primary, llm_fallback)
        // via ctx.emit_progress(), providing real-time visibility.
        // Cancel is handled by branchforge ToolRegistry at the outer level.
        let query_ir = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.brain.translate_query(
                &question,
                &ontology,
                retrieved_subgraph_md.as_deref(),
                ctx,
            ),
        )
        .await
        {
            // The agent's persistence path (`QueryExecution`) doesn't
            // carry CallProvenance today; eval case-execute drives
            // that capture instead. Drop the provenance handle.
            Ok(Ok((ir, _provenance))) => ir,
            Ok(Err(e)) => {
                warn!(question = %question, error = %e, "Query translation failed");
                return ToolResult::error(format!("Query translation failed: {e}"));
            }
            Err(_) => {
                warn!(question = %question, "Query translation timed out after 60s");
                return ToolResult::error("Query translation timed out after 60 seconds");
            }
        };

        // PlanRouter dispatch — single source of truth for backend
        // selection + cost-budget enforcement. The router consults
        // `cost::estimate_cost` once, gates `RiskLevel::High` against
        // `CostBudget.allow_high_cost`, detects cross-source
        // traversal for `Federation` routing, and emits a stable
        // attribution string the FE result panel renders. The
        // `reject_high_cost` agent flag maps to the inverse of
        // `allow_high_cost` so workspace policy rides on the same
        // gate as the LLM-toggled `?allow_high_cost=true` query
        // parameter.
        let router = HeuristicPlanRouter::new();
        let budget = CostBudget {
            allow_high_cost: !self.reject_high_cost,
            ..Default::default()
        };
        let decision = match router.route(&query_ir, &ontology, Some(&budget)).await {
            Ok(d) => d,
            Err(OxError::Validation { message, .. }) if message.starts_with("[budget]") => {
                warn!(question = %question, message = %message, "PlanRouter refused query");
                return ToolResult::error(format!(
                    "Query rejected: {message}. Reformulate with a bounded path length / \
                     indexed filter / connected pattern, or pass `allow_high_cost=true` \
                     to override."
                ));
            }
            Err(e) => {
                warn!(question = %question, error = %e, "PlanRouter error");
                return ToolResult::error(format!("PlanRouter error: {e}"));
            }
        };
        let routing_reason = decision.reason();
        let cost_estimate = decision
            .cost()
            .cloned()
            .expect("HeuristicPlanRouter always populates cost");
        match &decision {
            RouteDecision::Federation { .. } => {
                // Cross-source traversal: today the agent still
                // executes via the graph runtime path because
                // federation execute_plan integration ships in T3.
                // Surface the routing decision in the attribution
                // so the operator + EvaluationCapture see the
                // detection actually fired.
                info!(question = %question, routing = routing_reason, "Federation routing detected");
            }
            RouteDecision::Hybrid { .. } => {
                info!(question = %question, routing = routing_reason, "Hybrid routing");
            }
            RouteDecision::Graph { .. } => {}
        }

        // Step 2: Compile QueryIR → target language. The compiler
        // applies ConceptMap rewrite internally against the active
        // ontology — every backend gets the same safety net without
        // each tool re-implementing the funnel.
        ctx.progress("compiling").started();
        let t2 = std::time::Instant::now();
        let compiled = match self
            .domain
            .compiler
            .compile_query(&query_ir, Some(ontology.as_ref()))
        {
            Ok(c) => {
                ctx.progress("compiling").completed_with(
                    t2.elapsed().as_millis() as u64,
                    serde_json::json!({ "cypher": truncate(&c.statement, 500) }),
                );
                c
            }
            Err(e) => {
                warn!(question = %question, error = %e, "Query compilation failed");
                ctx.progress("compiling")
                    .failed(t2.elapsed().as_millis() as u64);
                return ToolResult::error(format!("Query compilation failed: {e}"));
            }
        };

        // Step 3: Execute (timeout: 60s, cancel-aware)
        //
        // `GRAPH_ONTOLOGY.scope` hands the runtime a reference to the active
        // ontology snapshot so its Cypher validator pipeline can reject
        // unknown labels / relationships / properties before hitting the
        // driver. The Arc clone is cheap (atomic inc); the runtime sees it
        // through the task-local, not a parameter, so internal paths
        // (search, profiler, introspection) that never set the local stay
        // exempt.
        ctx.progress("executing").started();
        let t3 = std::time::Instant::now();
        let execute_fut = ox_graph_runtime::GRAPH_ONTOLOGY.scope(
            Arc::clone(&ontology),
            runtime.execute_query(&compiled.statement, &compiled.params),
        );
        let results = tokio::select! {
            timeout_result = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                execute_fut,
            ) => {
                match timeout_result {
                    Ok(Ok(r)) => {
                        ctx.progress("executing").completed_with(
                            t3.elapsed().as_millis() as u64,
                            serde_json::json!({ "row_count": r.metadata.rows_returned }),
                        );
                        r
                    }
                    Ok(Err(e)) => {
                        warn!(question = %question, error = %e, query = %truncate(&compiled.statement, 200), "Query execution failed");
                        ctx.progress("executing").failed(t3.elapsed().as_millis() as u64);
                        return ToolResult::error(format!(
                            "Query execution failed: {e}\nCompiled query: {}",
                            truncate(&compiled.statement, 500),
                        ));
                    }
                    Err(_) => {
                        ctx.progress("executing").failed(60_000);
                        return ToolResult::error("Query execution timed out after 60 seconds");
                    }
                }
            }
            _ = async {
                if let Some(ref token) = cancel {
                    token.cancelled().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                ctx.progress("executing").failed(t3.elapsed().as_millis() as u64);
                return ToolResult::error("Query execution cancelled");
            }
        };

        let execution_time_ms = start.elapsed().as_millis() as i64;
        let execution_id = Uuid::new_v4();

        // Persist routing attribution onto the per-response
        // provenance trail so the FE result panel + the
        // `useExecution` hook surface "why this backend".
        let mut results = results;
        let provenance = results
            .metadata
            .provenance
            .get_or_insert_with(Default::default);
        provenance.routing = Some(routing_reason.to_string());

        info!(
            execution_id = %execution_id,
            question = %input.question,
            target = self.domain.compiler.name(),
            rows = results.metadata.rows_returned,
            routing = routing_reason,
            execution_time_ms,
            "Graph query executed"
        );

        // Persist query execution
        let bindings = resolve_query_bindings(&query_ir, &ontology);
        let query_bindings_json = serde_json::to_value(&bindings).ok();
        let _params_json = serde_json::to_value(&compiled.params).ok();

        let execution = QueryExecution {
            id: execution_id,
            user_id: self.domain.user_id.clone(),
            question: input.question.clone(),
            ontology_lineage_id: ontology.id.clone(),
            ontology_version: ontology.version.number as i32,
            ontology_id: self.domain.ontology_id,
            ontology_snapshot: if self.domain.ontology_id.is_some() {
                None
            } else {
                serde_json::to_value(&*ontology).ok()
            },
            query_ir: serde_json::to_value(&query_ir).unwrap_or_default(),
            compiled_target: self.domain.compiler.name().to_string(),
            compiled_query: compiled.statement.clone(),
            results: serde_json::to_value(&results).unwrap_or_default(),
            widget: None,
            explanation: String::new(),
            model: self.brain.default_model_info().model.clone(),
            execution_time_ms,
            query_bindings: query_bindings_json,
            feedback: None,
            created_at: Utc::now(),
        };

        if let Err(e) = self.domain.store.create_query_execution(&execution).await {
            warn!("Failed to persist query execution: {e}");
        }

        // Step 4: Auto-detect best widget type (fast model, non-blocking)
        let widget_hint = if results.metadata.rows_returned > 0 {
            let sample = serde_json::to_string(&results.rows.iter().take(5).collect::<Vec<_>>())
                .unwrap_or_default();
            match self.brain.select_widget(&query_ir, &sample).await {
                Ok(hint) => {
                    let wt = serde_json::to_value(hint.widget_type)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_else(|| "table".to_string());
                    Some(WidgetHintOutput {
                        widget_type: wt,
                        title: hint.title.unwrap_or_default(),
                    })
                }
                Err(e) => {
                    warn!("Widget hint failed (non-critical): {e}");
                    None
                }
            }
        } else {
            None
        };

        // Guidance: tell agent when data is sufficient to avoid unnecessary follow-up queries
        let mut guidance = if results.metadata.rows_returned >= 2 {
            Some(format!(
                "Got {} rows with columns: [{}]. \
                 Present a complete analysis NOW — summaries, totals, key findings, and actionable insights. \
                 Do NOT make additional queries unless the user asks a follow-up.",
                results.metadata.rows_returned,
                results.columns.join(", "),
            ))
        } else if results.metadata.rows_returned == 0 {
            Some(
                "No results. Broaden the search — use CONTAINS instead of exact match, \
                  or check property names in the ontology schema."
                    .to_string(),
            )
        } else {
            None
        };

        // Append cost warnings to guidance if risk is elevated
        if cost_estimate.risk_level != ox_compiler::cost::RiskLevel::Low
            && !cost_estimate.warnings.is_empty()
        {
            let cost_note = format!(
                " [Query cost: {:?} — {}]",
                cost_estimate.risk_level,
                cost_estimate.warnings.join("; "),
            );
            match &mut guidance {
                Some(g) => g.push_str(&cost_note),
                None => guidance = Some(cost_note),
            }
        }

        // Advisory-validator diagnostics run in strict mode via the
        // shared `ox_graph_runtime::cypher::diagnostics` helper so the agent
        // surface and the HTTP surface (see
        // `ox-api/src/routes/query.rs`) stay aligned on which
        // validators run and how their output is shaped. The runtime
        // pipeline runs a *permissive* variant so power users aren't
        // blocked; this strict re-pass is pure advice — the query
        // already executed.
        let validator_notes = strict_advisory_diagnostics(
            &compiled.statement,
            &self.domain.workspace_id.to_string(),
        );
        if !validator_notes.is_empty() {
            // The LLM sees a flattened, LLM-friendly form ("<validator>
            // <level>: <message>") in the guidance tail; the structured
            // list stays on the tool output envelope for the UI.
            let joined = validator_notes
                .iter()
                .map(format_diagnostic_for_guidance)
                .collect::<Vec<_>>()
                .join("; ");
            let joined = format!(" [Validation: {joined}]");
            match &mut guidance {
                Some(g) => g.push_str(&joined),
                None => guidance = Some(joined),
            }
        }

        // Surface unresolved AmbiguityContext entries on two
        // channels: a single text line on `guidance` for the LLM
        // (nudge toward the `resolve_ambiguity` tool), and a typed
        // `unresolved_ambiguities` Vec on the result envelope for
        // the FE (one chip per context, deep-linking to the
        // Glossary workbench). The two channels stay narrow — the
        // LLM never reasons over the chip list, the FE never parses
        // the text. A failed list load is non-fatal: a missed nudge
        // is better than a failed query.
        let ambiguity_contexts = self
            .domain
            .store
            .list_ambiguity_contexts_in_workspace()
            .await
            .unwrap_or_default();
        let mut unresolved_ambiguities: Vec<UnresolvedAmbiguityHint> = Vec::new();
        for ctx in &ambiguity_contexts {
            let active = self
                .domain
                .store
                .find_active_ambiguity_resolution(&ctx.source_id, &ctx.column)
                .await
                .ok()
                .flatten();
            if active.is_none() {
                unresolved_ambiguities.push(UnresolvedAmbiguityHint {
                    context_id: ctx.id.as_str().to_string(),
                    relation: ctx.column.relation.clone(),
                    column: ctx.column.column.clone(),
                });
            }
        }
        if !unresolved_ambiguities.is_empty() {
            let preview = unresolved_ambiguities
                .iter()
                .take(3)
                .map(|h| format!("{}.{}", h.relation, h.column))
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if unresolved_ambiguities.len() > 3 {
                format!(" (+{} more)", unresolved_ambiguities.len() - 3)
            } else {
                String::new()
            };
            let note = format!(
                " [Ambiguity: {} unresolved column{} ({}{}); consider calling \
                 resolve_ambiguity to bind one before the next query]",
                unresolved_ambiguities.len(),
                if unresolved_ambiguities.len() == 1 { "" } else { "s" },
                preview,
                suffix,
            );
            match &mut guidance {
                Some(g) => g.push_str(&note),
                None => guidance = Some(note),
            }
        }

        let cost = if cost_estimate.risk_level != ox_compiler::cost::RiskLevel::Low {
            Some(cost_estimate)
        } else {
            None
        };

        // Π-3 — build response provenance inline. The agent tool path
        // never sets `QueryResult.metadata.provenance` (the runtime
        // leaves it `None`), so the signal only reaches the client if
        // the tool explicitly assembles it here. Current-version
        // lookup is a cheap btree seek (partial index); a failure
        // downgrades to `ontology_version: None` rather than failing
        // the whole tool call.
        //
        // Same snapshot feeds the anchor-search below — fetch once
        // and reuse.
        let current_version_snapshot = match self.domain.ontology_id {
            Some(ontology_id) => self
                .domain
                .store
                .find_current_version(ontology_id)
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let provenance = if let (Some(ontology_id), Some(snapshot)) =
            (self.domain.ontology_id, current_version_snapshot.as_ref())
        {
            Some(ox_compiler::build_provenance(
                &query_ir,
                &ox_compiler::ProvenanceContext {
                    ontology_id: Some(ontology_id.to_string()),
                    ontology_version: Some(snapshot.version.clone()),
                    as_of: None,
                    source_ids: Vec::new(),
                    ontology: Some(ontology.as_ref()),
                },
            ))
        } else {
            None
        };

        // Anchor search. Runs `search_entry_points` against the
        // current version's searchable document index and records
        // the top blended score + hit kinds in the signal. Off-path
        // for the user-facing query result, but the
        // `anchor_match_rate` tile on /settings/quality/signals
        // needs the reading.
        let anchor_hit: Option<(f32, Vec<String>)> = if let Some(snapshot) =
            current_version_snapshot.as_ref()
        {
            let opts = ox_store::navigation::EntryPointSearchOptions::new(
                snapshot.id,
                &question,
                5,
            );
            match self.domain.store.search_entry_points(opts).await {
                Ok(hits) if !hits.is_empty() => {
                    let top = hits[0].score;
                    let kinds: Vec<String> =
                        hits.iter().map(|h| h.entity_kind.clone()).collect();
                    Some((top, kinds))
                }
                Ok(_) => Some((0.0, Vec::new())),
                Err(e) => {
                    warn!(error = %e, "anchor search failed (signal tile will show stale window)");
                    None
                }
            }
        } else {
            None
        };

        // Quality-signal capture. Fire-and-forget: a write failure
        // is logged but does NOT fail the user-facing query. Runs
        // after the execution row lands so the FK
        // `query_execution_signals.execution_id → query_executions(id)`
        // always resolves.
        {
            // Did a `resolve_ambiguity` call in this same branchforge
            // session land a resolution in the recent past? The
            // tracker lives on `AppState` so resolve + query turns
            // can land on different chat-stream requests and still
            // correlate.
            let ambiguity_was_clarified = self
                .domain
                .clarification_tracker
                .was_clarified_within(
                    ctx.session_id(),
                    chrono::Duration::minutes(
                        crate::clarification_tracker::DEFAULT_WINDOW_MINUTES,
                    ),
                );
            let signal = build_query_execution_signal(
                execution_id,
                self.domain.workspace_id,
                &query_ir,
                provenance.as_ref(),
                &validator_notes,
                Some(ontology.as_ref()),
                anchor_hit.as_ref(),
                ambiguity_was_clarified,
            );
            let type_kinds = signal_type_kinds(provenance.as_ref());
            let store = Arc::clone(&self.domain.store);
            let workspace_id = self.domain.workspace_id;
            // `WORKSPACE_ID` is threaded through the spawned future so
            // the store's RLS `before_acquire` hook sees the same
            // tenant the tool ran under — without this scope the
            // spawned task hits the deny-all policy branch.
            #[allow(clippy::disallowed_methods)]
            tokio::spawn(async move {
                ox_store::WORKSPACE_ID
                    .scope(workspace_id, async move {
                        if let Err(e) = store
                            .create_query_execution_signal(&signal)
                            .await
                        {
                            warn!(error = %e, "quality signal persist failed");
                        }
                        if !type_kinds.is_empty() {
                            let refs: Vec<(Uuid, &str)> = type_kinds
                                .iter()
                                .map(|(id, k)| (*id, k.as_str()))
                                .collect();
                            if let Err(e) = store.upsert_type_last_used(&refs).await {
                                warn!(error = %e, "type_last_used upsert failed");
                            }
                        }
                    })
                    .await
            });
        }

        let output = QueryGraphOutput {
            execution_id: execution_id.to_string(),
            compiled_query: compiled.statement,
            columns: results.columns.clone(),
            row_count: results.metadata.rows_returned as usize,
            rows: serde_json::to_value(&results.rows).unwrap_or_default(),
            widget_hint,
            cost,
            guidance,
            warnings: validator_notes,
            unresolved_ambiguities,
        };

        ToolResult::success(serde_json::to_string_pretty(&output).unwrap_or_default())
    }
}

/// Walk `OntologyNavigationStore` (Postgres-backed Level-3
/// indexes) and assemble a question-anchored subgraph slice for
/// the LLM prompt. Returns `None` when navigation is unavailable
/// (no committed ontology, no version snapshot, fetch failure) —
/// the translator's schema RAG path stays the source of truth on
/// the prompt side, this just adds a denser, anchor-expanded
/// slice when one is reachable.
///
/// Three-step Progressive Disclosure flow per
/// `crates/ox-store/src/store/ontology_navigation.rs`:
///   1. `search_entry_points(top_k=8)` → blended trigram + FTS +
///      embedding anchors against the question.
///   2. `expand_neighbors{depth:2, max_nodes:40}` → BFS the
///      anchors into a single subgraph.
///   3. `render_subgraph_for_llm` → markdown the prompt template
///      surfaces under `## Retrieved subgraph`.
async fn try_retrieve_subgraph_md(
    domain: &DomainContext,
    question: &str,
) -> Option<String> {
    let ontology_id = domain.ontology_id?;
    let snapshot = domain
        .store
        .find_current_version(ontology_id)
        .await
        .ok()??;
    let version_id = snapshot.id;

    let anchors = match domain
        .store
        .search_entry_points(EntryPointSearchOptions::new(version_id, question, 8))
        .await
    {
        Ok(hits) if !hits.is_empty() => hits,
        _ => return None,
    };

    let anchor_refs: Vec<ox_store::EntityRef> =
        anchors.iter().map(|h| h.as_entity_ref()).collect();

    let mut expand_options =
        NeighborExpandOptions::new(version_id, anchor_refs);
    expand_options.depth = 2;
    expand_options.max_nodes = 40;

    let subgraph = match domain
        .store
        .expand_neighbors(expand_options)
        .await
    {
        Ok(g) => g,
        Err(_) => return None,
    };

    // Cap the GraphRAG injection at a conservative token slice so
    // a large ontology can't squeeze the operator's question, the
    // schema RAG, the conversation history, and the answer
    // reservation out of the context window. The 2000-token
    // budget is the empirical sweet spot — wide enough to carry
    // a 2-hop subgraph for a complex domain, narrow enough that
    // the prompt overhead stays under 30% of even an 8K window.
    let render_options = LlmRenderOptions {
        max_nodes: 40,
        max_tokens: Some(2_000),
        include_doc_snippets: true,
    };
    let markdown = domain
        .store
        .render_subgraph_for_llm(&subgraph, &render_options);
    if markdown.trim().is_empty() {
        None
    } else {
        Some(format!("## Retrieved subgraph\n\n{markdown}"))
    }
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..s.floor_char_boundary(max_len)]
    }
}

/// Flatten a structured [`QueryDiagnostic`] into the `"<validator>
/// <level>: <message>"` shape the LLM's `guidance` tail carries.
/// Kept alongside the caller so the format stays local to the one
/// consumer that needs a string form — all other surfaces render
/// structured fields directly.
fn format_diagnostic_for_guidance(d: &ox_query_ir::query::QueryDiagnostic) -> String {
    // `level` serialises as `"warning"` / `"error"` / `"info"` on the
    // wire; mirror that lowercase rendering in the LLM tail so
    // operators and the model see one consistent token across logs,
    // responses, and prompts.
    let level = match d.level {
        ox_query_ir::query::DiagnosticLevel::Error => "error",
        ox_query_ir::query::DiagnosticLevel::Warning => "warning",
        ox_query_ir::query::DiagnosticLevel::Info => "info",
    };
    format!("{} {}: {}", d.validator, level, d.message)
}

/// Translate a strict-advisory diagnostic tagged `validator:"shacl"`
/// into the typed [`ShaclFailureKind`] for the signal store. Falls
/// back to `Other` when the message fingerprint doesn't match a
/// known bucket — `Other` itself is a first-class variant so every
/// failure still shows up in the failure-kind histogram.
fn first_shacl_failure_kind(
    diagnostics: &[ox_query_ir::query::QueryDiagnostic],
) -> Option<ox_store::ShaclFailureKind> {
    use ox_query_ir::query::DiagnosticLevel;
    use ox_store::ShaclFailureKind;

    diagnostics
        .iter()
        .find(|d| d.validator == "shacl" && d.level == DiagnosticLevel::Error)
        .map(|d| match d.message.code.as_str() {
            // SHACL diagnostic codes are the stable contract — see
            // `crates/ox-graph-runtime/src/cypher/shacl_validator.rs` emit
            // sites. Adding a new SHACL code requires a matching arm
            // here so the failure-kind histogram stays partitioned.
            "runtime.cypher.shacl.min_count_missing" => {
                ShaclFailureKind::MandatoryPropertyMissing
            }
            "runtime.cypher.shacl.value_not_in_set"
            | "runtime.cypher.shacl.notation_pattern_mismatch" => {
                ShaclFailureKind::UnknownCodedValue
            }
            "runtime.cypher.shacl.measure_group_by_violation" => {
                ShaclFailureKind::MeasureGroupBy
            }
            "runtime.cypher.shacl.cardinality_violation" => {
                ShaclFailureKind::CardinalityViolation
            }
            "runtime.cypher.shacl.temporal_grain_mismatch" => {
                ShaclFailureKind::TemporalGrainMismatch
            }
            _ => ShaclFailureKind::Other,
        })
}

/// Assemble a `QueryExecutionSignal` from the agent-path context.
///
/// The signal carries the top anchor-search score + the list of
/// hit entity kinds for the question that triggered the query.
/// When `anchor_hit` is `Some((score, kinds))` those populate
/// `anchor_top_score` and `anchor_hit_kinds`; aggregation
/// thresholds at `score >= 0.5` so a zero is still valid
/// ("ontology under-indexed for this phrasing").
///
/// `glossary_term_hits` is populated from the ontology: every
/// property on a referenced type that carries a
/// `PropertyDef::glossary_term_id` is treated as a potential hit.
/// Overcounts if the query only touched a sibling property, but
/// gives the `glossary_hit_rate` tile a non-zero signal until the
/// compile-time walk that attributes hits per-property lands.
fn build_query_execution_signal(
    execution_id: Uuid,
    workspace_id: Uuid,
    query_ir: &ox_query_ir::query::QueryIR,
    provenance: Option<&ox_query_ir::query::QueryProvenance>,
    validator_notes: &[ox_query_ir::query::QueryDiagnostic],
    ontology: Option<&ox_ontology::OntologyIR>,
    anchor_hit: Option<&(f32, Vec<String>)>,
    ambiguity_was_clarified: bool,
) -> ox_store::QueryExecutionSignal {
    // SHACL failure: any `validator: "shacl"` entry with `Error` level
    // in the strict re-pass means the runtime's permissive pass let
    // the query through but the stricter set would have rejected it.
    let shacl_failure_kind = first_shacl_failure_kind(validator_notes);
    let shacl_passed = shacl_failure_kind.is_none();

    // `type_ids` is `Vec<String>` on `QueryProvenance`. Signal store
    // wants `Vec<Uuid>` so non-parseable ids (external identifiers,
    // legacy strings) are skipped — the metric's accuracy depends on
    // UUIDs anyway (FK into ontology rows).
    let referenced_type_ids: Vec<Uuid> = provenance
        .map(|p| {
            p.type_ids
                .iter()
                .filter_map(|s| Uuid::parse_str(s).ok())
                .collect()
        })
        .unwrap_or_default();

    let glossary_term_hits = collect_glossary_hits(ontology, provenance);
    let (anchor_top_score, anchor_hit_kinds) = match anchor_hit {
        Some((score, kinds)) => (Some(*score), kinds.clone()),
        None => (None, Vec::new()),
    };

    ox_store::QueryExecutionSignal {
        execution_id,
        workspace_id,
        captured_at: Utc::now(),
        anchor_top_score,
        anchor_hit_kinds,
        glossary_term_hits,
        ambiguity_resolution_ids: Vec::new(),
        ambiguity_was_clarified,
        shacl_passed,
        shacl_failure_kind,
        query_ir_normalized_hash: query_ir.canonical_hash(),
        referenced_type_ids,
    }
}

/// Cross-reference the referenced types with the ontology and return
/// every glossary-term pointer that fires. UUID-only — id newtypes
/// that happen to be non-UUID strings (external identifiers, legacy
/// slugs) are silently dropped since the signal column is
/// `uuid[]` in postgres.
fn collect_glossary_hits(
    ontology: Option<&ox_ontology::OntologyIR>,
    provenance: Option<&ox_query_ir::query::QueryProvenance>,
) -> Vec<Uuid> {
    let (Some(ir), Some(prov)) = (ontology, provenance) else {
        return Vec::new();
    };
    use std::collections::{BTreeSet, HashSet};
    let type_id_set: HashSet<&str> = prov.type_ids.iter().map(|s| s.as_str()).collect();
    let mut hits: BTreeSet<Uuid> = BTreeSet::new();
    let walk_properties =
        |hits: &mut BTreeSet<Uuid>, properties: &[ox_ontology::ir::PropertyDef]| {
            for prop in properties {
                if let Some(gid) = prop.glossary_term_id()
                    && let Ok(uuid) = Uuid::parse_str(gid.as_str())
                {
                    hits.insert(uuid);
                }
            }
        };
    for node in ir.node_types() {
        if type_id_set.contains(node.id.as_str()) {
            walk_properties(&mut hits, &node.properties);
        }
    }
    for edge in ir.edge_types() {
        if type_id_set.contains(edge.id.as_str()) {
            walk_properties(&mut hits, &edge.properties);
        }
    }
    hits.into_iter().collect()
}

/// `(type_id, kind)` pairs feeding the `ontology_type_last_used`
/// upsert. Kind defaults to `"NodeType"` since the provenance
/// currently tags every id by node-type heuristics — a future
/// signal version can carry edge-type / property-level kinds when
/// the provenance does.
fn signal_type_kinds(
    provenance: Option<&ox_query_ir::query::QueryProvenance>,
) -> Vec<(Uuid, String)> {
    provenance
        .map(|p| {
            p.type_ids
                .iter()
                .filter_map(|s| Uuid::parse_str(s).ok().map(|u| (u, "NodeType".to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::collect_glossary_hits;
    use ox_core::{GraphLabel, PropertyKey};
    use ox_ontology::glossary::GlossaryTermId;
    use ox_ontology::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef, PropertyId};
    use ox_query_ir::query::QueryProvenance;

    fn property_with_term(id: &str, gid_uuid: &str) -> PropertyDef {
        PropertyDef {
            id: PropertyId::new(id),
            name: PropertyKey::new(id).unwrap(),
            property_type: ox_core::types::PropertyType::String,
            bindings: vec![ox_ontology::PropertyBinding::glossary(GlossaryTermId::new(gid_uuid),)],
            ..Default::default()
        }
    }

    fn sample_ir_with_bound_property(node_id: &str, term_uuid: &str) -> OntologyIR {
        let node = NodeTypeDef {
            id: NodeTypeId::new(node_id),
            label: GraphLabel::new(node_id).unwrap(),
            properties: vec![property_with_term("tier", term_uuid)],
            ..Default::default()
        };
        OntologyIR::new(
            "ont-test".into(),
            "Test".into(),
            Default::default(),
            1,
            vec![node],
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn collect_glossary_hits_empty_when_no_ontology() {
        let hits = collect_glossary_hits(None, None);
        assert!(hits.is_empty());
    }

    #[test]
    fn collect_glossary_hits_returns_uuid_from_bound_property() {
        let term_uuid = "00000000-0000-0000-0000-00000000abcd";
        let ir = sample_ir_with_bound_property("Customer", term_uuid);
        let prov = QueryProvenance {
            type_ids: vec!["Customer".into()],
            ..Default::default()
        };
        let hits = collect_glossary_hits(Some(&ir), Some(&prov));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].to_string(), term_uuid);
    }

    #[test]
    fn collect_glossary_hits_skips_unreferenced_types() {
        let term_uuid = "00000000-0000-0000-0000-00000000abcd";
        let ir = sample_ir_with_bound_property("Customer", term_uuid);
        let prov = QueryProvenance {
            type_ids: vec!["Order".into()], // not in ontology
            ..Default::default()
        };
        let hits = collect_glossary_hits(Some(&ir), Some(&prov));
        assert!(hits.is_empty());
    }

    #[test]
    fn collect_glossary_hits_drops_non_uuid_term_ids() {
        // `glossary_term_id` that doesn't parse as UUID (legacy slug)
        // is silently dropped — the signal column is `uuid[]`.
        let ir = sample_ir_with_bound_property("Customer", "g-vip-legacy");
        let prov = QueryProvenance {
            type_ids: vec!["Customer".into()],
            ..Default::default()
        };
        let hits = collect_glossary_hits(Some(&ir), Some(&prov));
        assert!(hits.is_empty());
    }
}
