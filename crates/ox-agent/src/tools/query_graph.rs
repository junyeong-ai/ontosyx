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
         Translates the question to a graph query, runs it, and returns structured results \
         with columns and rows. Use this for any data retrieval, aggregation, or exploration.";

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let ontology = match self.domain.ontology.as_ref() {
            Some(o) => o,
            None => return ToolResult::error("No ontology loaded"),
        };

        let runtime = match self.domain.runtime.as_ref() {
            Some(r) => r,
            None => return ToolResult::error("Graph database not connected"),
        };

        let start = std::time::Instant::now();

        // Step 1: Translate NL → QueryIR (timeout: 60s)
        let query_ir = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            self.brain.translate_query(&input.question, ontology),
        )
        .await
        {
            Ok(Ok(ir)) => ir,
            Ok(Err(e)) => return ToolResult::error(format!("Query translation failed: {e}")),
            Err(_) => return ToolResult::error("Query translation timed out after 60 seconds"),
        };

        // Step 2: Compile QueryIR → target language
        let compiled = match self.domain.compiler.compile_query(&query_ir) {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("Query compilation failed: {e}")),
        };

        // Step 3: Execute (timeout: 60s)
        let results = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            runtime.execute_query(&compiled.statement, &compiled.params),
        )
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                return ToolResult::error(format!(
                    "Query execution failed: {e}\nCompiled query: {}",
                    truncate(&compiled.statement, 500),
                ));
            }
            Err(_) => return ToolResult::error("Query execution timed out after 60 seconds"),
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

        let output = QueryGraphOutput {
            execution_id: execution_id.to_string(),
            compiled_query: compiled.statement,
            compiled_target: self.domain.compiler.target_name().to_string(),
            columns: results.columns.clone(),
            row_count: results.metadata.rows_returned as usize,
            rows: serde_json::to_value(&results.rows).unwrap_or_default(),
            widget_hint,
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
