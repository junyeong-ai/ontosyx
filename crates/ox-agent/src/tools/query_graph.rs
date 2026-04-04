use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use ox_core::resolve_query_bindings;
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
    /// Per-step timing breakdown.
    step_timings: Vec<StepTiming>,
    /// Guidance for the agent on how to proceed with results.
    #[serde(skip_serializing_if = "Option::is_none")]
    guidance: Option<String>,
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
}

#[async_trait]
impl SchemaTool for QueryGraphTool {
    type Input = QueryGraphInput;
    const NAME: &'static str = super::QUERY_GRAPH;
    const DESCRIPTION: &'static str = "Execute a natural language query against the knowledge graph database. \
         Translates the question to a graph query, runs it, and returns structured results. \
         IMPORTANT: Include ALL needed entities and relationships in a single question — \
         the engine handles multi-hop chains (e.g., A→B→C→D) in one query. \
         Do NOT split into separate calls per entity.";
    const READ_ONLY: bool = true;

    async fn handle(&self, input: Self::Input, ctx: &ExecutionContext) -> ToolResult {
        let ontology = match self.domain.ontology.as_ref() {
            Some(o) => o,
            None => {
                return ToolResult::error(
                    "No ontology loaded. Create a project from a data source first, \
                     or use introspect_source to connect to a database.",
                )
            }
        };

        let runtime = match self.domain.runtime.as_ref() {
            Some(r) => r,
            None => {
                return ToolResult::error(
                    "Graph database not connected. The project needs a deployed schema \
                     with loaded data before queries can execute.",
                )
            }
        };

        let start = std::time::Instant::now();
        let mut step_timings = Vec::with_capacity(3);

        let question = input.question.clone();

        // Step 1: Translate NL → QueryIR (timeout: 60s)
        // Brain emits sub-steps (schema_discovery, llm_primary, llm_fallback)
        // via ctx.emit_progress(), providing real-time visibility.
        let t1 = std::time::Instant::now();
        let query_ir = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.brain.translate_query(&question, ontology, ctx),
        )
        .await
        {
            Ok(Ok(ir)) => {
                let ms = t1.elapsed().as_millis() as u64;
                step_timings.push(StepTiming { step: "translating".into(), duration_ms: ms });
                ir
            }
            Ok(Err(e)) => {
                return ToolResult::error(format!("Query translation failed: {e}"));
            }
            Err(_) => {
                return ToolResult::error("Query translation timed out after 60 seconds");
            }
        };

        // Step 2: Compile QueryIR → target language
        ctx.progress("compiling").started();
        let t2 = std::time::Instant::now();
        let compiled = match self.domain.compiler.compile_query(&query_ir) {
            Ok(c) => {
                let ms = t2.elapsed().as_millis() as u64;
                step_timings.push(StepTiming { step: "compiling".into(), duration_ms: ms });
                ctx.progress("compiling").completed_with(ms,
                    serde_json::json!({ "cypher": truncate(&c.statement, 500) }));
                c
            }
            Err(e) => {
                ctx.progress("compiling").failed(t2.elapsed().as_millis() as u64);
                return ToolResult::error(format!("Query compilation failed: {e}"));
            }
        };

        // Step 3: Execute (timeout: 60s)
        ctx.progress("executing").started();
        let t3 = std::time::Instant::now();
        let results = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            runtime.execute_query(&compiled.statement, &compiled.params),
        )
        .await
        {
            Ok(Ok(r)) => {
                let ms = t3.elapsed().as_millis() as u64;
                step_timings.push(StepTiming { step: "executing".into(), duration_ms: ms });
                ctx.progress("executing").completed_with(ms,
                    serde_json::json!({ "row_count": r.metadata.rows_returned }));
                r
            }
            Ok(Err(e)) => {
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
        };

        let execution_time_ms = start.elapsed().as_millis() as i64;
        let execution_id = Uuid::new_v4();

        info!(
            execution_id = %execution_id,
            question = %input.question,
            target = self.domain.compiler.target_name(),
            rows = results.metadata.rows_returned,
            execution_time_ms,
            "Graph query executed"
        );

        // Persist query execution
        let bindings = resolve_query_bindings(&query_ir, ontology);
        let query_bindings_json = serde_json::to_value(&bindings).ok();
        let _params_json = serde_json::to_value(&compiled.params).ok();

        let execution = QueryExecution {
            id: execution_id,
            user_id: self.domain.user_id.clone(),
            question: input.question.clone(),
            ontology_id: ontology.id.clone(),
            ontology_version: ontology.version as i32,
            saved_ontology_id: self.domain.saved_ontology_id,
            ontology_snapshot: if self.domain.saved_ontology_id.is_some() {
                None
            } else {
                serde_json::to_value(ontology).ok()
            },
            query_ir: serde_json::to_value(&query_ir).unwrap_or_default(),
            compiled_target: self.domain.compiler.target_name().to_string(),
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
        let guidance = if results.metadata.rows_returned >= 2 {
            Some(format!(
                "Got {} rows with columns: [{}]. Analyze these results directly and present findings. \
                 Do NOT make additional queries unless the user asks a follow-up question. \
                 If using execute_analysis, pass this data in the 'data' field and access columns by these exact names.",
                results.metadata.rows_returned,
                results.columns.join(", "),
            ))
        } else if results.metadata.rows_returned == 0 {
            Some("No results found. Try broadening the search — use CONTAINS instead of exact match, \
                  or check property names in the ontology schema.".to_string())
        } else {
            None
        };

        let output = QueryGraphOutput {
            execution_id: execution_id.to_string(),
            compiled_query: compiled.statement,
            compiled_target: self.domain.compiler.target_name().to_string(),
            columns: results.columns.clone(),
            row_count: results.metadata.rows_returned as usize,
            rows: serde_json::to_value(&results.rows).unwrap_or_default(),
            widget_hint,
            step_timings,
            guidance,
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
