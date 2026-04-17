use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use ox_brain::Brain;
use ox_compiler::GraphCompiler;
use ox_core::ontology_ir::OntologyIR;
use ox_core::types::PropertyValue;
use ox_runtime::GraphRuntime;
use ox_store::Store;
use ox_store::store::CursorParams;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};

// ---------------------------------------------------------------------------
// Rate limiter — tumbling 60s window per MCP session
//
// `StreamableHttpService` constructs one `OntosyxMcpServer` per session via
// its factory closure, so the limiter is naturally per-session: every clone
// of this server shares the same `Arc<Mutex<...>>` state.
// ---------------------------------------------------------------------------

const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const RATE_LIMIT_MAX_CALLS: u32 = 100;

#[derive(Default)]
struct SessionLimiter {
    state: Mutex<Option<(Instant, u32)>>,
}

impl SessionLimiter {
    fn check(&self) -> Result<(), McpError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|e| McpError::internal_error(format!("Limiter poisoned: {e}"), None))?;

        match guard.as_mut() {
            None => {
                *guard = Some((Instant::now(), 1));
                Ok(())
            }
            Some((start, count)) if start.elapsed() > RATE_LIMIT_WINDOW => {
                *start = Instant::now();
                *count = 1;
                Ok(())
            }
            Some((_, count)) if *count < RATE_LIMIT_MAX_CALLS => {
                *count += 1;
                Ok(())
            }
            _ => {
                warn!(
                    max = RATE_LIMIT_MAX_CALLS,
                    window_secs = RATE_LIMIT_WINDOW.as_secs(),
                    "MCP rate limit exceeded"
                );
                Err(McpError::invalid_request(
                    format!(
                        "Rate limit exceeded: max {RATE_LIMIT_MAX_CALLS} tool calls per {} seconds per session",
                        RATE_LIMIT_WINDOW.as_secs()
                    ),
                    None,
                ))
            }
        }
    }
}

/// Wrap a tool's future with the configured per-call timeout.
async fn with_call_timeout<F, T>(timeout: Duration, fut: F) -> Result<T, McpError>
where
    F: std::future::Future<Output = Result<T, McpError>>,
{
    match tokio::time::timeout(timeout, fut).await {
        Ok(res) => res,
        Err(_) => Err(McpError::internal_error(
            format!(
                "Tool execution timed out after {} seconds",
                timeout.as_secs()
            ),
            None,
        )),
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
    store: Arc<dyn Store>,
    limiter: Arc<SessionLimiter>,
    call_timeout: Duration,
    tool_router: ToolRouter<Self>,
}

impl OntosyxMcpServer {
    pub fn new(
        brain: Arc<dyn Brain>,
        compiler: Arc<dyn GraphCompiler>,
        runtime: Option<Arc<dyn GraphRuntime>>,
        store: Arc<dyn Store>,
        call_timeout: Duration,
    ) -> Self {
        Self {
            brain,
            compiler,
            runtime,
            store,
            limiter: Arc::new(SessionLimiter::default()),
            call_timeout,
            tool_router: Self::tool_router(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool parameter types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct QueryParams {
    /// Natural language question to ask the knowledge graph
    question: String,
    /// Name of the saved ontology to query against (use ontosyx_list_ontologies to find available names)
    ontology_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListOntologiesParams {
    /// Maximum number of ontologies to return (default 50, max 100)
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DescribeOntologyParams {
    /// Name of the ontology to describe
    ontology_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ExportParams {
    /// Name of the ontology to export
    ontology_name: String,
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
}

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
struct OntologySummary {
    id: String,
    name: String,
    version: i32,
    description: Option<String>,
    node_count: usize,
    edge_count: usize,
}

#[derive(Serialize)]
struct ListOntologiesResponse {
    ontologies: Vec<OntologySummary>,
}

#[derive(Serialize)]
struct NodeSummary {
    label: String,
    description: Option<String>,
    properties: Vec<PropertySummary>,
    constraints: Vec<String>,
}

#[derive(Serialize)]
struct PropertySummary {
    name: String,
    property_type: String,
    nullable: bool,
    description: Option<String>,
}

#[derive(Serialize)]
struct EdgeSummary {
    label: String,
    description: Option<String>,
    source: String,
    target: String,
    cardinality: String,
}

#[derive(Serialize)]
struct DescribeOntologyResponse {
    name: String,
    description: Option<String>,
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

async fn load_ontology(store: &dyn Store, name: &str) -> Result<OntologyIR, McpError> {
    let saved = store
        .get_latest_ontology(name)
        .await
        .map_err(|e| McpError::internal_error(format!("Store error: {e}"), None))?
        .ok_or_else(|| McpError::invalid_params(format!("Ontology '{name}' not found"), None))?;

    serde_json::from_value::<OntologyIR>(saved.ontology_ir)
        .map_err(|e| McpError::internal_error(format!("Failed to deserialize ontology: {e}"), None))
}

/// Serialize a response struct to pretty JSON text, mapping errors to McpError.
fn to_json_text(value: &impl Serialize) -> Result<String, McpError> {
    serde_json::to_string_pretty(value)
        .map_err(|e| McpError::internal_error(format!("Serialization failed: {e}"), None))
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

#[tool_router]
impl OntosyxMcpServer {
    #[tool(
        name = "ontosyx_query",
        description = "Query a knowledge graph using natural language. Translates the question into a graph query, executes it, and returns results with a natural language explanation. Requires a saved ontology name — use ontosyx_list_ontologies to discover available ontologies first."
    )]
    async fn query(
        &self,
        Parameters(params): Parameters<QueryParams>,
    ) -> Result<CallToolResult, McpError> {
        self.limiter.check()?;
        with_call_timeout(self.call_timeout, self.do_query(params)).await
    }

    #[tool(
        name = "ontosyx_list_ontologies",
        description = "List all saved ontologies available for querying. Returns names, versions, descriptions, and node/edge counts. Use this to discover which ontologies are available before using ontosyx_query or ontosyx_describe_ontology."
    )]
    async fn list_ontologies(
        &self,
        Parameters(params): Parameters<ListOntologiesParams>,
    ) -> Result<CallToolResult, McpError> {
        self.limiter.check()?;
        with_call_timeout(self.call_timeout, self.do_list_ontologies(params)).await
    }

    #[tool(
        name = "ontosyx_describe_ontology",
        description = "Get the detailed structure of a specific ontology, including all node types with their properties and constraints, and all edge types with their source/target connections. Useful for understanding the graph schema before writing queries."
    )]
    async fn describe_ontology(
        &self,
        Parameters(params): Parameters<DescribeOntologyParams>,
    ) -> Result<CallToolResult, McpError> {
        self.limiter.check()?;
        with_call_timeout(self.call_timeout, self.do_describe_ontology(params)).await
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
        with_call_timeout(self.call_timeout, self.do_export(params)).await
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
        with_call_timeout(self.call_timeout, self.do_execute_cypher(params)).await
    }
}

impl OntosyxMcpServer {
    async fn do_query(&self, params: QueryParams) -> Result<CallToolResult, McpError> {
        let start = Instant::now();
        info!(
            ontology = %params.ontology_name,
            question = %params.question,
            "MCP: ontosyx_query invoked"
        );

        // Load ontology
        let ontology = load_ontology(self.store.as_ref(), &params.ontology_name).await?;

        // Translate NL -> QueryIR
        let query_ir = self
            .brain
            .translate_query(
                &params.question,
                &ontology,
                &branchforge::ExecutionContext::empty(),
            )
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Query translation failed: {e}"), None)
            })?;

        // Compile QueryIR -> target language
        let compiled = self.compiler.compile_query(&query_ir).map_err(|e| {
            McpError::internal_error(format!("Query compilation failed: {e}"), None)
        })?;

        // Execute query
        let runtime = self.runtime.as_ref().ok_or_else(|| {
            McpError::internal_error("Graph database not connected".to_string(), None)
        })?;

        let results = runtime
            .execute_query(&compiled.statement, &compiled.params)
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

        let explanation = self.brain.explain(&explain_prompt).await.map_err(|e| {
            McpError::internal_error(format!("Explanation generation failed: {e}"), None)
        })?;

        let elapsed = start.elapsed();
        info!(
            ontology = %params.ontology_name,
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

    async fn do_list_ontologies(
        &self,
        params: ListOntologiesParams,
    ) -> Result<CallToolResult, McpError> {
        info!("MCP: ontosyx_list_ontologies invoked");

        let pagination = CursorParams {
            limit: params.limit.unwrap_or(50),
            cursor: None,
        };

        let page = self
            .store
            .list_saved_ontologies(&pagination)
            .await
            .map_err(|e| McpError::internal_error(format!("Store error: {e}"), None))?;

        let ontologies: Vec<OntologySummary> = page
            .items
            .into_iter()
            .map(|saved| {
                let (node_count, edge_count) = saved
                    .ontology_ir
                    .as_object()
                    .map(|obj| {
                        let nc = obj
                            .get("node_types")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        let ec = obj
                            .get("edge_types")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        (nc, ec)
                    })
                    .unwrap_or((0, 0));

                OntologySummary {
                    id: saved.id.to_string(),
                    name: saved.name,
                    version: saved.version,
                    description: saved.description,
                    node_count,
                    edge_count,
                }
            })
            .collect();

        info!(
            count = ontologies.len(),
            "MCP: ontosyx_list_ontologies completed"
        );

        let response = ListOntologiesResponse { ontologies };
        Ok(CallToolResult::success(vec![Content::text(to_json_text(
            &response,
        )?)]))
    }

    async fn do_describe_ontology(
        &self,
        params: DescribeOntologyParams,
    ) -> Result<CallToolResult, McpError> {
        info!(
            ontology = %params.ontology_name,
            "MCP: ontosyx_describe_ontology invoked"
        );

        let ontology = load_ontology(self.store.as_ref(), &params.ontology_name).await?;

        let nodes: Vec<NodeSummary> = ontology
            .node_types
            .iter()
            .map(|n| {
                let properties = n
                    .properties
                    .iter()
                    .map(|p| PropertySummary {
                        name: p.name.clone(),
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
                    label: n.label.clone(),
                    description: n.description.clone(),
                    properties,
                    constraints,
                }
            })
            .collect();

        let edges: Vec<EdgeSummary> = ontology
            .edge_types
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
                    label: e.label.clone(),
                    description: e.description.clone(),
                    source,
                    target,
                    cardinality: format!("{:?}", e.cardinality),
                }
            })
            .collect();

        info!(
            ontology = %params.ontology_name,
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
        let format_name = match &params.format {
            ExportFormat::Cypher => "cypher",
            ExportFormat::Graphql => "graphql",
            ExportFormat::Owl => "owl",
            ExportFormat::Mermaid => "mermaid",
        };

        info!(
            ontology = %params.ontology_name,
            format = format_name,
            "MCP: ontosyx_export invoked"
        );

        let ontology = load_ontology(self.store.as_ref(), &params.ontology_name).await?;

        let content = match params.format {
            ExportFormat::Cypher => ox_compiler::export::generate_cypher_ddl(&ontology),
            ExportFormat::Graphql => ox_compiler::export::generate_graphql(&ontology),
            ExportFormat::Owl => ox_compiler::export::generate_owl_turtle(&ontology),
            ExportFormat::Mermaid => ox_compiler::export::generate_mermaid(&ontology),
        };

        info!(
            ontology = %params.ontology_name,
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

        let runtime = self.runtime.as_ref().ok_or_else(|| {
            McpError::internal_error("Graph database not connected".to_string(), None)
        })?;

        let empty_params: HashMap<String, PropertyValue> = HashMap::new();
        let results = runtime
            .execute_query(&params.query, &empty_params)
            .await
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
