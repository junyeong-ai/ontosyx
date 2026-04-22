use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use ox_query_ir::resolve_query_bindings;
use ox_runtime::cypher::strict_advisory_diagnostics;
use ox_store::QueryExecution;

use crate::DomainContext;

// ---------------------------------------------------------------------------
// QueryGraphTool — NL → Cypher → Execute → Results → Persist
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryGraphInput {
    /// Natural language question about the graph data.
    pub question: String,
}

#[derive(Debug, Serialize)]
struct QueryGraphOutput {
    execution_id: String,
    compiled_query: String,
    compiled_target: String,
    columns: Vec<String>,
    row_count: usize,
    rows: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    widget_hint: Option<WidgetHintOutput>,
    /// Query cost estimation (risk level + warnings).
    #[serde(skip_serializing_if = "Option::is_none")]
    cost: Option<ox_compiler::cost::QueryCost>,
    /// Per-step timing breakdown.
    step_timings: Vec<StepTiming>,
    /// Guidance for the agent on how to proceed with results.
    #[serde(skip_serializing_if = "Option::is_none")]
    guidance: Option<String>,
    /// Π-3 response-attribution trail. Mirrors the wire shape the HTTP
    /// query routes stamp onto `QueryResult.metadata.provenance`, so
    /// the front-end's `ResponseBasis` panel renders the same summary
    /// whether the query ran through the agent tool path or the raw
    /// HTTP path. `None` iff the session has no pinned ontology
    /// identity (ad-hoc draft execution).
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ox_query_ir::query::QueryProvenance>,
    /// Advisory validator diagnostics — same shape as
    /// `QueryMetadata.warnings`. Surfaced here as a structured field
    /// so the frontend's ResponseBasis panel renders identically
    /// whether the query took the agent tool path or the HTTP
    /// `/api/query/from-ir` path. The LLM reads a flattened form of
    /// the same content via the `guidance` tail.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<ox_query_ir::query::QueryDiagnostic>,
}

#[derive(Debug, Serialize)]
struct StepTiming {
    step: String,
    duration_ms: u64,
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
                    "No ontology loaded. Create a project from a data source first, \
                     or use introspect_source to connect to a database.",
                );
            }
        };

        let runtime = match self.domain.runtime.as_ref() {
            Some(r) => r,
            None => {
                return ToolResult::error(
                    "Graph database not connected. The project needs a deployed schema \
                     with loaded data before queries can execute.",
                );
            }
        };

        let start = std::time::Instant::now();
        let mut step_timings = Vec::with_capacity(3);
        let cancel = ctx.cancel_token().cloned();

        let question = input.question.clone();

        // Step 1: Translate NL → QueryIR (timeout: 60s)
        // Brain emits sub-steps (schema_discovery, llm_primary, llm_fallback)
        // via ctx.emit_progress(), providing real-time visibility.
        // Cancel is handled by branchforge ToolRegistry at the outer level.
        let t1 = std::time::Instant::now();
        let query_ir = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.brain.translate_query(&question, &ontology, ctx),
        )
        .await
        {
            Ok(Ok(ir)) => {
                let ms = t1.elapsed().as_millis() as u64;
                step_timings.push(StepTiming {
                    step: "translating".into(),
                    duration_ms: ms,
                });
                ir
            }
            Ok(Err(e)) => {
                warn!(question = %question, error = %e, "Query translation failed");
                return ToolResult::error(format!("Query translation failed: {e}"));
            }
            Err(_) => {
                warn!(question = %question, "Query translation timed out after 60s");
                return ToolResult::error("Query translation timed out after 60 seconds");
            }
        };

        // Cost estimation: analyse QueryIR before compilation. High-risk
        // shapes (unbounded `*`, Cartesian joins, unindexed high-fanout
        // labels) are refused before they reach the driver when policy
        // allows — otherwise we still warn so operators see the shape.
        let cost_estimate = ox_compiler::cost::estimate_cost(&query_ir, &ontology);
        if cost_estimate.risk_level == ox_compiler::cost::RiskLevel::High {
            warn!(
                risk = ?cost_estimate.risk_level,
                cartesian = cost_estimate.has_cartesian,
                var_depth = cost_estimate.max_var_length_depth,
                "High-risk query detected"
            );
            if self.reject_high_cost {
                let detail = cost_estimate.warnings.join("; ");
                return ToolResult::error(format!(
                    "Query rejected: the cost estimator flagged this as high-risk ({detail}). \
                     Reformulate with a bounded path length / indexed filter / connected pattern, \
                     or ask an admin to disable `agent.reject_high_cost` for this workspace."
                ));
            }
        }

        // Step 2: Compile QueryIR → target language
        ctx.progress("compiling").started();
        let t2 = std::time::Instant::now();
        let compiled = match self.domain.compiler.compile_query(&query_ir) {
            Ok(c) => {
                let ms = t2.elapsed().as_millis() as u64;
                step_timings.push(StepTiming {
                    step: "compiling".into(),
                    duration_ms: ms,
                });
                ctx.progress("compiling").completed_with(
                    ms,
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
        let execute_fut = ox_runtime::GRAPH_ONTOLOGY.scope(
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
                        let ms = t3.elapsed().as_millis() as u64;
                        step_timings.push(StepTiming { step: "executing".into(), duration_ms: ms });
                        ctx.progress("executing").completed_with(ms,
                            serde_json::json!({ "row_count": r.metadata.rows_returned }));
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

        info!(
            execution_id = %execution_id,
            question = %input.question,
            target = self.domain.compiler.name(),
            rows = results.metadata.rows_returned,
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
        // shared `ox_runtime::cypher::diagnostics` helper so the agent
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
        // Phase 4.6 — the same snapshot feeds the anchor-search below,
        // so fetch it once and reuse.
        let current_version_snapshot = match self.domain.ontology_id {
            Some(ontology_id) => self
                .domain
                .store
                .get_current_version(ontology_id)
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

        // Phase 4.6 — anchor search. Runs `search_entry_points` against
        // the current version's searchable document index and records
        // the top blended score + hit kinds in the signal. Off-path
        // for the user-facing query result (we don't surface anchors
        // in the tool output today), but the `anchor_match_rate` tile
        // on /settings/quality/signals needs the reading.
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

        // Phase 3 — quality-signal capture. Fire-and-forget: a write
        // failure is logged but does NOT fail the user-facing query.
        // Runs after the execution row lands so the FK
        // `query_execution_signals.execution_id → query_executions(id)`
        // always resolves.
        {
            let signal = build_query_execution_signal(
                execution_id,
                self.domain.workspace_id,
                &query_ir,
                provenance.as_ref(),
                &validator_notes,
                Some(ontology.as_ref()),
                anchor_hit.as_ref(),
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
            compiled_target: self.domain.compiler.name().to_string(),
            columns: results.columns.clone(),
            row_count: results.metadata.rows_returned as usize,
            rows: serde_json::to_value(&results.rows).unwrap_or_default(),
            widget_hint,
            cost,
            step_timings,
            guidance,
            provenance,
            warnings: validator_notes,
        };

        ToolResult::success(serde_json::to_string_pretty(&output).unwrap_or_default())
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
        .map(|d| {
            let msg = d.message.to_ascii_lowercase();
            // Fingerprint on the bits of message text the ShaclValidator
            // emits today (see `crates/ox-runtime/src/cypher/shacl_validator.rs`).
            // Matching is substring-based so wording tweaks don't
            // invalidate the histogram until a validator rewrite
            // renames the category outright.
            if msg.contains("required by rule") || msg.contains("mincount") {
                ShaclFailureKind::MandatoryPropertyMissing
            } else if msg.contains("not defined")
                || msg.contains("violates rule")
                || msg.contains("not an enum")
            {
                ShaclFailureKind::UnknownCodedValue
            } else if msg.contains("measure") && msg.contains("group by") {
                ShaclFailureKind::MeasureGroupBy
            } else if msg.contains("cardinality")
                || msg.contains("many_to_many")
                || msg.contains("distinct")
            {
                ShaclFailureKind::CardinalityViolation
            } else if msg.contains("temporal") || msg.contains("grain") {
                ShaclFailureKind::TemporalGrainMismatch
            } else {
                ShaclFailureKind::Other
            }
        })
}

/// Assemble a `QueryExecutionSignal` from the agent-path context.
///
/// Phase 4.6 — the signal now carries the top anchor-search
/// score + the list of hit entity kinds for the question that
/// triggered the query. When `anchor_hit` is `Some((score, kinds))`
/// those populate `anchor_top_score` and `anchor_hit_kinds`; the
/// quality-signal aggregation thresholds at `score >= 0.5` so a
/// zero is still a valid reading ("ontology under-indexed for
/// this phrasing").
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
        ambiguity_was_clarified: false,
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
                if let Some(gid) = &prop.glossary_term_id
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
            glossary_term_id: Some(GlossaryTermId::new(gid_uuid)),
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
