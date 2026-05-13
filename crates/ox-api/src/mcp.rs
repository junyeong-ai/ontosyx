use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use ox_brain::Brain;
use ox_compiler::GraphCompiler;
use ox_core::LocalizedText;
use ox_core::types::PropertyValue;
use ox_graph_runtime::GraphRuntime;
use ox_ontology::ir::OntologyIR;
use ox_store::Store;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};

use crate::config::McpRateLimitConfig;

// ---------------------------------------------------------------------------
// Rate limiter — sliding window per MCP session
//
// `StreamableHttpService` constructs one `OntosyxMcpServer` per session via
// its factory closure, so the limiter is naturally per-session: every clone
// of this server shares the same `Arc<Mutex<...>>` state.
//
// Sliding window (timestamps of past calls) instead of a tumbling counter
// closes the burst hole — a tumbling window would allow 2× the budget
// (one full bucket at the tail of bucket N, another at the head of N+1).
// ---------------------------------------------------------------------------

struct SessionLimiter {
    window: Duration,
    max_calls: u32,
    state: Mutex<VecDeque<Instant>>,
}

impl SessionLimiter {
    fn new(window: Duration, max_calls: u32) -> Self {
        Self {
            window,
            max_calls,
            state: Mutex::new(VecDeque::new()),
        }
    }

    fn check(&self) -> Result<(), McpError> {
        let mut q = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("Limiter poisoned: {e}"), None))?;

        let now = Instant::now();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        while q.front().is_some_and(|t| *t < cutoff) {
            q.pop_front();
        }

        if q.len() >= self.max_calls as usize {
            metrics::counter!("mcp_rate_limit_rejections_total").increment(1);
            warn!(
                max = self.max_calls,
                window_secs = self.window.as_secs(),
                "MCP rate limit exceeded"
            );
            return Err(McpError::invalid_request(
                format!(
                    "Rate limit exceeded: max {} tool calls per {} seconds per session",
                    self.max_calls,
                    self.window.as_secs()
                ),
                None,
            ));
        }

        q.push_back(now);
        // Defensive memory cap: refuse to let the deque grow beyond
        // 2× the call budget, even if somehow the cutoff logic drifts.
        let cap = (self.max_calls as usize).saturating_mul(2);
        while q.len() > cap {
            q.pop_front();
        }
        Ok(())
    }
}

/// Wrap a tool's future with the configured per-call timeout.
///
/// `tool` is the tool name (e.g. `"ontosyx_query"`) used as the label
/// for the `mcp_tool_timeouts_total` counter when the timeout fires.
async fn with_call_timeout<F, T>(
    tool: &'static str,
    timeout: Duration,
    fut: F,
) -> Result<T, McpError>
where
    F: std::future::Future<Output = Result<T, McpError>>,
{
    match tokio::time::timeout(timeout, fut).await {
        Ok(res) => res,
        Err(_) => {
            metrics::counter!("mcp_tool_timeouts_total", "tool" => tool).increment(1);
            Err(McpError::internal_error(
                format!(
                    "Tool execution timed out after {} seconds",
                    timeout.as_secs()
                ),
                None,
            ))
        }
    }
}

/// RAII guard that records `mcp_tool_duration_seconds{tool}` when dropped.
/// Ensures every exit path — success, error, or early return — emits the
/// histogram sample exactly once.
struct DurationGuard {
    tool: &'static str,
    started: Instant,
}

impl DurationGuard {
    fn new(tool: &'static str, started: Instant) -> Self {
        Self { tool, started }
    }
}

impl Drop for DurationGuard {
    fn drop(&mut self) {
        metrics::histogram!("mcp_tool_duration_seconds", "tool" => self.tool)
            .record(self.started.elapsed().as_secs_f64());
    }
}

// ---------------------------------------------------------------------------
// MCP Server struct — holds references to AppState components
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct OntosyxMcpServer {
    brain: Arc<dyn Brain>,
    compiler: Arc<dyn GraphCompiler>,
    runtime: Option<Arc<dyn GraphRuntime>>,
    /// Optional read-only runtime used by `execute_cypher` when the
    /// operator has configured a dedicated R/O DB role. Falls back to
    /// `runtime` (the R/W handle) when `None` (with a logged warning).
    readonly_runtime: Option<Arc<dyn GraphRuntime>>,
    store: Arc<dyn Store>,
    limiter: Arc<SessionLimiter>,
    call_timeout: Duration,
    /// Mirror of `AgentConfig::reject_high_cost` — MCP tools enforce the
    /// same pre-execute gate as the chat tool surface.
    reject_high_cost: bool,
    /// Touched only via the `#[tool_handler]` macro expansion — looks
    /// dead to the compiler but is required for the rmcp dispatch table.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl OntosyxMcpServer {
    pub fn new(
        brain: Arc<dyn Brain>,
        compiler: Arc<dyn GraphCompiler>,
        runtime: Option<Arc<dyn GraphRuntime>>,
        readonly_runtime: Option<Arc<dyn GraphRuntime>>,
        store: Arc<dyn Store>,
        call_timeout: Duration,
        rate_limit: &McpRateLimitConfig,
        reject_high_cost: bool,
    ) -> Self {
        if readonly_runtime.is_none() {
            warn!(
                "MCP execute_cypher will run with R/W credentials; configure \
                 graph.readonly_user / graph.readonly_password for defense-in-depth"
            );
        }
        let limiter = Arc::new(SessionLimiter::new(
            Duration::from_secs(rate_limit.window_seconds),
            rate_limit.max_calls,
        ));
        Self {
            brain,
            compiler,
            runtime,
            readonly_runtime,
            store,
            limiter,
            call_timeout,
            reject_high_cost,
            tool_router: Self::tool_router(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool parameter types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct QueryParams {
    /// Natural language question to ask the workspace's knowledge
    /// graph. Workspace × ontology is 1:1, so no name selector.
    question: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportParams {
    /// Export format: "cypher" (Neo4j DDL), "graphql" (GraphQL schema), "owl" (OWL/Turtle), or "mermaid" (ER diagram)
    format: ExportFormat,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ExportFormat {
    /// Neo4j Cypher DDL statements
    Cypher,
    /// GraphQL schema definition
    Graphql,
    /// OWL/Turtle ontology format
    Owl,
    /// Mermaid ER diagram for visualization
    Mermaid,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExecuteCypherParams {
    /// Cypher query statement to execute
    query: String,
    /// When `true`, the runtime's OntologyValidator rejects unknown
    /// labels / relationship types / properties before the query
    /// hits the driver. Same contract as `ontosyx_query`. Workspace
    /// × ontology is 1:1, so no name selector.
    #[serde(default)]
    validate_against_ontology: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DescribeOntologyParams {}

// ---------------------------------------------------------------------------
// Tool response types (for structured output)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct QueryResponse {
    answer: String,
    query: String,
    results: serde_json::Value,
    row_count: usize,
}

#[derive(Serialize)]
struct NodeSummary {
    label: String,
    description: LocalizedText,
    properties: Vec<PropertySummary>,
    constraints: Vec<String>,
}

#[derive(Serialize)]
struct PropertySummary {
    name: String,
    property_type: String,
    nullable: bool,
    description: LocalizedText,
}

#[derive(Serialize)]
struct EdgeSummary {
    label: String,
    description: LocalizedText,
    source: String,
    target: String,
    cardinality: String,
}

#[derive(Serialize)]
struct DescribeOntologyResponse {
    name: String,
    description: LocalizedText,
    version: u32,
    nodes: Vec<NodeSummary>,
    edges: Vec<EdgeSummary>,
}

#[derive(Serialize)]
struct ExportResponse {
    format: String,
    content: String,
}

#[derive(Serialize)]
struct ExecuteCypherResponse {
    columns: Vec<String>,
    rows: serde_json::Value,
    row_count: usize,
    execution_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Helper: load ontology by name from store
// ---------------------------------------------------------------------------

async fn load_ontology(store: &dyn Store) -> Result<OntologyIR, McpError> {
    let identity = store
        .get_workspace_ontology()
        .await
        .map_err(|e| McpError::internal_error(format!("Store error: {e}"), None))?
        .ok_or_else(|| McpError::invalid_params("Workspace has no canonical ontology yet", None))?;
    let version = store
        .find_current_version(identity.id)
        .await
        .map_err(|e| McpError::internal_error(format!("Store error: {e}"), None))?
        .ok_or_else(|| {
            McpError::invalid_params(
                format!("Ontology '{}' has no committed version", identity.name),
                None,
            )
        })?;
    store
        .get_ontology_ir(version.id)
        .await
        .map_err(|e| McpError::internal_error(format!("Hydrate error: {e}"), None))?
        .ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "Ontology '{}' snapshot is no longer available",
                    identity.name
                ),
                None,
            )
        })
}

/// Serialize a response struct to pretty JSON text, mapping errors to McpError.
fn to_json_text(value: &impl Serialize) -> Result<String, McpError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(format!("Serialization failed: {e}"), None))
}

/// Return the first Cypher write keyword found in `q`, or `None` if the
/// query is read-only.
///
/// The match is whitespace-bounded and case-insensitive so common
/// formatting quirks (`detach delete`, `\nMERGE\n`) all trip the gate
/// while harmless substrings (`creator`, `setting`, comments) do not.
/// This is a heuristic — a determined attacker could likely sneak past
/// it — but it runs as inner defense-in-depth behind `require_auth`
/// (outer HTTP gate) and the read-only runtime role (innermost DB
/// gate). Once the Cypher `SafetyValidator` moves to a guaranteed
/// pre-execute pass on the MCP path, this heuristic becomes redundant
/// and can be removed.
fn forbidden_cypher_keyword(q: &str) -> Option<&'static str> {
    const WRITE_KEYWORDS: &[&str] = &[
        "CREATE", "MERGE", "DELETE", "SET", "REMOVE", "DROP", "FOREACH", "LOAD",
    ];
    let upper = q.to_uppercase();
    let bytes = upper.as_bytes();
    for &kw in WRITE_KEYWORDS {
        let kw_bytes = kw.as_bytes();
        let mut start = 0;
        while let Some(pos) = upper[start..].find(kw) {
            let abs = start + pos;
            let before_ok = abs == 0 || !is_word_byte(bytes[abs - 1]);
            let after_idx = abs + kw_bytes.len();
            let after_ok = after_idx >= bytes.len() || !is_word_byte(bytes[after_idx]);
            if before_ok && after_ok {
                return Some(kw);
            }
            start = abs + 1;
        }
    }
    None
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_cypher_keyword_blocks_writes() {
        assert_eq!(
            forbidden_cypher_keyword("MATCH (n) DETACH DELETE n"),
            Some("DELETE")
        );
        assert_eq!(
            forbidden_cypher_keyword("CREATE (a:Person)"),
            Some("CREATE")
        );
        assert_eq!(
            forbidden_cypher_keyword("MERGE (a:Person {name: 'x'})"),
            Some("MERGE")
        );
        assert_eq!(
            forbidden_cypher_keyword("MATCH (n) SET n.x = 1"),
            Some("SET")
        );
        assert_eq!(
            forbidden_cypher_keyword("MATCH (n) REMOVE n:Stale"),
            Some("REMOVE")
        );
        assert_eq!(forbidden_cypher_keyword("DROP INDEX foo"), Some("DROP"));
        assert_eq!(
            forbidden_cypher_keyword("LOAD CSV FROM 'x' AS row"),
            Some("LOAD")
        );
        // case-insensitive
        assert_eq!(
            forbidden_cypher_keyword("match (n) detach delete n"),
            Some("DELETE")
        );
    }

    #[test]
    fn forbidden_cypher_keyword_allows_reads() {
        assert_eq!(forbidden_cypher_keyword("MATCH (n) RETURN n"), None);
        assert_eq!(
            forbidden_cypher_keyword("MATCH (n) WHERE n.score > 0 RETURN n"),
            None
        );
        assert_eq!(
            forbidden_cypher_keyword("MATCH (a)-[r]->(b) RETURN a, r, b LIMIT 10"),
            None
        );
    }

    #[test]
    fn forbidden_cypher_keyword_ignores_substrings() {
        // "creator", "settings", etc. should NOT trip the gate.
        assert_eq!(
            forbidden_cypher_keyword("MATCH (u:User) WHERE u.creator = 'x' RETURN u"),
            None
        );
        assert_eq!(
            forbidden_cypher_keyword("MATCH (n) WHERE n.settings IS NOT NULL RETURN n"),
            None
        );
        assert_eq!(
            forbidden_cypher_keyword("MATCH (n {removed: false}) RETURN n"),
            None
        );
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router]
impl OntosyxMcpServer {
    #[tool(
        name = "ontosyx_query",
        description = "Query the workspace's knowledge graph using natural language. Translates the question into a graph query, executes it, and returns results with a natural language explanation. The workspace owns exactly one canonical ontology — no name selector needed."
    )]
    async fn query(
        &self,
        Parameters(params): Parameters<QueryParams>,
    ) -> Result<CallToolResult, McpError> {
        self.limiter.check()?;
        with_call_timeout("ontosyx_query", self.call_timeout, self.do_query(params)).await
    }

    #[tool(
        name = "ontosyx_describe_ontology",
        description = "Get the structure of the workspace's canonical ontology — name, version, description, all node types with their properties and constraints, and all edge types with their source/target connections. Useful for understanding the graph schema before writing queries."
    )]
    async fn describe_ontology(
        &self,
        Parameters(params): Parameters<DescribeOntologyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.limiter.check()?;
        with_call_timeout(
            "ontosyx_describe_ontology",
            self.call_timeout,
            self.do_describe_ontology(params),
        )
        .await
    }

    #[tool(
        name = "ontosyx_export",
        description = "Export an ontology in a specific format. Available formats: 'cypher' (Neo4j DDL statements), 'graphql' (GraphQL schema), 'owl' (OWL/Turtle ontology), 'mermaid' (Mermaid ER diagram for visualization)."
    )]
    async fn export(
        &self,
        Parameters(params): Parameters<ExportParams>,
    ) -> Result<CallToolResult, McpError> {
        self.limiter.check()?;
        with_call_timeout("ontosyx_export", self.call_timeout, self.do_export(params)).await
    }

    #[tool(
        name = "ontosyx_execute_cypher",
        description = "Execute a raw Cypher query directly against the Neo4j graph database. For power users who want to run specific Cypher statements. Use ontosyx_query for natural language queries instead."
    )]
    async fn execute_cypher(
        &self,
        Parameters(params): Parameters<ExecuteCypherParams>,
    ) -> Result<CallToolResult, McpError> {
        self.limiter.check()?;
        with_call_timeout(
            "ontosyx_execute_cypher",
            self.call_timeout,
            self.do_execute_cypher(params),
        )
        .await
    }
}

impl OntosyxMcpServer {
    async fn do_query(&self, params: QueryParams) -> Result<CallToolResult, McpError> {
        metrics::counter!("mcp_tool_invocations_total", "tool" => "ontosyx_query").increment(1);
        let start = Instant::now();
        let _duration_guard = DurationGuard::new("ontosyx_query", start);
        info!(
            question = %params.question,
            "MCP: ontosyx_query invoked"
        );

        // Load ontology
        let ontology = load_ontology(self.store.as_ref()).await?;

        // Translate NL -> QueryIR inside an `InferenceSession`
        // scope. Φ9 PipelineStage state machine attributes the
        // attempt chain to the session; `run_in_inference_session`
        // opens / scopes / finalises the session around the call.
        let (query_ir, _provenance) = ox_store::run_in_inference_session(
            self.store.as_ref(),
            &params.question,
            ox_ontology::AgentRef::Service {
                service_id: "mcp_server".into(),
            },
            || async {
                self.brain
                    .translate_query(
                        &params.question,
                        &ontology,
                        None,
                        &entelix::ExecutionContext::default(),
                    )
                    .await
            },
        )
        .await
        .map_err(|e| McpError::internal_error(format!("Query translation failed: {e}"), None))?;

        // Pre-execute cost gating: reject High-risk shapes before they hit
        // the driver. Mirrors the check in `QueryGraphTool` so the MCP
        // path gets the same protection as the chat tool surface.
        let cost_estimate = ox_compiler::cost::estimate_cost(&query_ir, &ontology);
        if cost_estimate.risk_level == ox_compiler::cost::RiskLevel::High && self.reject_high_cost {
            let detail = cost_estimate.warnings.join("; ");
            return Err(McpError::invalid_request(
                format!(
                    "Query rejected: cost estimator flagged this as high-risk ({detail}). \
                     Reformulate with a bounded path length / indexed filter / connected pattern."
                ),
                None,
            ));
        }

        // Compile QueryIR -> target language. The compiler applies
        // ConceptMap rewrite internally against the active ontology.
        let compiled = self
            .compiler
            .compile_query(&query_ir, Some(&ontology))
            .map_err(|e| {
                McpError::internal_error(format!("Query compilation failed: {e}"), None)
            })?;

        // Execute query
        let runtime = self.runtime.as_ref().ok_or_else(|| {
            McpError::internal_error("Graph database not connected".to_string(), None)
        })?;

        let results = ox_graph_runtime::GRAPH_ONTOLOGY
            .scope(
                Arc::new(ontology.clone()),
                runtime.execute_query(&compiled.statement, &compiled.params),
            )
            .await
            .map_err(|e| McpError::internal_error(format!("Query execution failed: {e}"), None))?;

        // Generate explanation
        let row_count = results.metadata.rows_returned;
        let results_json = serde_json::to_value(&results).unwrap_or_default();

        let preview_limit = 10;
        let preview_rows: Vec<_> = results.rows.iter().take(preview_limit).collect();
        let preview_json =
            serde_json::to_string_pretty(&preview_rows).unwrap_or_else(|_| "[]".to_string());
        let truncated = results.rows.len() > preview_rows.len();

        let explain_prompt = format!(
            "User asked: \"{}\"\n\n\
             Actual query results:\n\
             - rows_returned: {}\n\
             - columns: {:?}\n\
             - execution_time_ms: {}\n\
             - preview_rows: {}\n\
             - preview_truncated: {}\n\n\
             Summarize only what the returned data shows in 1-2 sentences.",
            params.question,
            row_count,
            results.columns,
            results.metadata.execution_time_ms,
            preview_json,
            truncated,
        );

        let explanation = self
            .brain
            .explain(&explain_prompt, &entelix::ExecutionContext::default())
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Explanation generation failed: {e}"), None)
            })?;

        let elapsed = start.elapsed();
        info!(
            row_count,
            elapsed_ms = elapsed.as_millis() as u64,
            "MCP: ontosyx_query completed"
        );

        let response = QueryResponse {
            answer: explanation.content,
            query: compiled.statement,
            results: results_json,
            row_count,
        };

        Ok(CallToolResult::success(vec![Content::text(to_json_text(
            &response,
        )?)]))
    }

    async fn do_describe_ontology(
        &self,
        _params: DescribeOntologyParams,
    ) -> Result<CallToolResult, McpError> {
        metrics::counter!(
            "mcp_tool_invocations_total",
            "tool" => "ontosyx_describe_ontology",
        )
        .increment(1);
        let _duration_guard = DurationGuard::new("ontosyx_describe_ontology", Instant::now());
        info!("MCP: ontosyx_describe_ontology invoked");

        let ontology = load_ontology(self.store.as_ref()).await?;

        let nodes: Vec<NodeSummary> = ontology
            .node_types()
            .iter()
            .map(|n| {
                let properties = n
                    .properties
                    .iter()
                    .map(|p| PropertySummary {
                        name: p.name.to_string(),
                        property_type: format!("{:?}", p.property_type),
                        nullable: p.nullable,
                        description: p.description.clone(),
                    })
                    .collect();

                let constraints = n
                    .constraints
                    .iter()
                    .map(|c| format!("{:?}", c.constraint))
                    .collect();

                NodeSummary {
                    label: n.label.to_string(),
                    description: n.description.clone(),
                    properties,
                    constraints,
                }
            })
            .collect();

        let edges: Vec<EdgeSummary> = ontology
            .edge_types()
            .iter()
            .map(|e| {
                let source = ontology
                    .node_label(&e.source_node_id)
                    .unwrap_or("unknown")
                    .to_string();
                let target = ontology
                    .node_label(&e.target_node_id)
                    .unwrap_or("unknown")
                    .to_string();

                EdgeSummary {
                    label: e.label.to_string(),
                    description: e.description.clone(),
                    source,
                    target,
                    cardinality: format!("{:?}", e.cardinality),
                }
            })
            .collect();

        info!(
            node_count = nodes.len(),
            edge_count = edges.len(),
            "MCP: ontosyx_describe_ontology completed"
        );

        let response = DescribeOntologyResponse {
            name: ontology.name,
            description: ontology.description,
            version: ontology.version.number,
            nodes,
            edges,
        };

        Ok(CallToolResult::success(vec![Content::text(to_json_text(
            &response,
        )?)]))
    }

    async fn do_export(&self, params: ExportParams) -> Result<CallToolResult, McpError> {
        metrics::counter!("mcp_tool_invocations_total", "tool" => "ontosyx_export").increment(1);
        let _duration_guard = DurationGuard::new("ontosyx_export", Instant::now());
        let format_name = match &params.format {
            ExportFormat::Cypher => "cypher",
            ExportFormat::Graphql => "graphql",
            ExportFormat::Owl => "owl",
            ExportFormat::Mermaid => "mermaid",
        };

        info!(format = format_name, "MCP: ontosyx_export invoked");

        let ontology = load_ontology(self.store.as_ref()).await?;

        let content = match params.format {
            ExportFormat::Cypher => ox_compiler::export::generate_cypher_ddl(&ontology),
            ExportFormat::Graphql => ox_compiler::export::generate_graphql(&ontology),
            ExportFormat::Owl => ox_compiler::export::generate_owl_turtle(&ontology),
            ExportFormat::Mermaid => ox_compiler::export::generate_mermaid(&ontology),
        };

        info!(
            format = format_name,
            content_len = content.len(),
            "MCP: ontosyx_export completed"
        );

        let response = ExportResponse {
            format: format_name.to_string(),
            content,
        };

        Ok(CallToolResult::success(vec![Content::text(to_json_text(
            &response,
        )?)]))
    }

    async fn do_execute_cypher(
        &self,
        params: ExecuteCypherParams,
    ) -> Result<CallToolResult, McpError> {
        metrics::counter!(
            "mcp_tool_invocations_total",
            "tool" => "ontosyx_execute_cypher",
        )
        .increment(1);
        let _duration_guard = DurationGuard::new("ontosyx_execute_cypher", Instant::now());
        info!(
            query_len = params.query.len(),
            "MCP: ontosyx_execute_cypher invoked"
        );

        if params.query.trim().is_empty() {
            return Err(McpError::invalid_params(
                "query must not be empty".to_string(),
                None,
            ));
        }

        // Read-only enforcement: MCP sessions don't carry user role,
        // so a destructive Cypher (`DELETE`, `DETACH DELETE`, `CREATE`,
        // `MERGE`, `SET`, `REMOVE`, `DROP`, `CALL` of write procs)
        // would mutate data with no accountability. The keyword
        // heuristic runs first as cheap defense-in-depth; the real
        // guard is the read-only DB role (`readonly_runtime`) so even
        // a bypass of the heuristic (comment tricks, exotic whitespace)
        // fails at the DB level.
        if let Some(forbidden) = forbidden_cypher_keyword(&params.query) {
            return Err(McpError::invalid_request(
                format!(
                    "Cypher keyword `{forbidden}` is not permitted via MCP. \
                     Use the authenticated HTTP API for graph mutations."
                ),
                None,
            ));
        }

        // Prefer the read-only runtime when configured; fall back to the
        // primary R/W handle (and rely solely on the heuristic gate).
        let runtime = self
            .readonly_runtime
            .as_ref()
            .or(self.runtime.as_ref())
            .ok_or_else(|| {
                McpError::internal_error("Graph database not connected".to_string(), None)
            })?;

        // Optional ontology gate — mirrors `ontosyx_query`: when the
        // caller asks for it, load the workspace's canonical ontology
        // and thread the task-local so the OntologyValidator runs.
        // Off → raw path (safety + workspace-scope only).
        let ontology = if params.validate_against_ontology {
            Some(Arc::new(load_ontology(self.store.as_ref()).await?))
        } else {
            None
        };

        let empty_params: HashMap<String, PropertyValue> = HashMap::new();
        let exec_fut = runtime.execute_query(&params.query, &empty_params);
        let results = match ontology {
            Some(o) => ox_graph_runtime::GRAPH_ONTOLOGY.scope(o, exec_fut).await,
            None => exec_fut.await,
        }
        .map_err(|e| McpError::internal_error(format!("Query execution failed: {e}"), None))?;

        let row_count = results.metadata.rows_returned;
        let execution_time_ms = results.metadata.execution_time_ms;
        let rows_json = serde_json::to_value(&results.rows).unwrap_or_default();

        info!(
            row_count,
            execution_time_ms, "MCP: ontosyx_execute_cypher completed"
        );

        let response = ExecuteCypherResponse {
            columns: results.columns,
            rows: rows_json,
            row_count,
            execution_time_ms,
        };

        Ok(CallToolResult::success(vec![Content::text(to_json_text(
            &response,
        )?)]))
    }
}

// ---------------------------------------------------------------------------
// ServerHandler implementation
// ---------------------------------------------------------------------------

#[tool_handler]
impl ServerHandler for OntosyxMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info({
                let mut info = Implementation::from_build_env();
                info.name = "ontosyx".to_string();
                info.version = env!("CARGO_PKG_VERSION").to_string();
                info.title = Some("Ontosyx - The Semantic Orchestrator".to_string());
                info
            })
            .with_instructions(
                "Ontosyx is a Knowledge Graph Lifecycle Platform. \
             Use ontosyx_list_ontologies to discover available ontologies, \
             ontosyx_describe_ontology to understand graph schemas, \
             ontosyx_query to ask natural language questions over knowledge graphs, \
             ontosyx_export to get ontology definitions in various formats, \
             and ontosyx_execute_cypher for direct Cypher queries."
                    .to_string(),
            )
    }
}
