// ---------------------------------------------------------------------------
// MemGraphRuntime — executes Cypher against Memgraph via Bolt protocol
//
// Memgraph is Bolt-compatible with Neo4j, so the neo4rs driver works directly.
// Key differences from Neo4j:
//   - Constraints use Neo4j 4.x syntax (ASSERT ... IS UNIQUE)
//   - No full-text indexes (db.index.fulltext.*)
//   - No vector indexes
//   - No GDS library (graph algorithms are built-in: mg.pagerank, etc.)
//   - No APOC procedures
//   - No separate databases — sandboxing uses label-prefix isolation
//   - Memory-first engine: smaller batch sizes recommended (100-500)
//   - No CALL { ... } subquery syntax (pre-5.0 style)
//   - No SHOW NODE/RELATIONSHIP TYPE PROPERTIES (uses mg.* procedures)
// ---------------------------------------------------------------------------

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use neo4rs::{ConfigBuilder, Graph, query};
use tokio::time::sleep;
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::query_ir::{QueryMetadata, QueryResult};
use ox_core::types::PropertyValue;

use crate::isolation::GraphIsolationStrategy;
use crate::{
    GRAPH_SYSTEM_BYPASS, GRAPH_WORKSPACE_ID, GraphRuntime, LoadBatch, LoadResult, SandboxHandle,
    TransienceDetector,
};

// ---------------------------------------------------------------------------
// Helpers (shared patterns with Neo4j — kept local to avoid coupling)
// ---------------------------------------------------------------------------

fn truncate_query(q: &str, max: usize) -> String {
    if q.len() <= max {
        q.to_string()
    } else {
        format!("{}...", &q[..max])
    }
}

fn bind_params(q: neo4rs::Query, params: &HashMap<String, PropertyValue>) -> neo4rs::Query {
    let mut q = q;
    for (name, value) in params {
        q = match value {
            PropertyValue::Bool(b) => q.param(name, *b),
            PropertyValue::Int(i) => q.param(name, *i),
            PropertyValue::Float(f) => q.param(name, *f),
            PropertyValue::String(s) => q.param(name, s.as_str()),
            PropertyValue::List(items) => {
                let json = serde_json::to_string(items).unwrap_or_default();
                q.param(name, json)
            }
            PropertyValue::Map(map) => {
                let json = serde_json::to_string(map).unwrap_or_default();
                q.param(name, json)
            }
            _ => q,
        };
    }
    q
}

fn bind_json_field(q: neo4rs::Query, name: &str, value: &serde_json::Value) -> neo4rs::Query {
    match value {
        serde_json::Value::String(s) => q.param(name, s.as_str()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.param(name, i)
            } else if let Some(f) = n.as_f64() {
                q.param(name, f)
            } else {
                q.param(name, n.to_string())
            }
        }
        serde_json::Value::Bool(b) => q.param(name, *b),
        serde_json::Value::Null => q,
        _ => q.param(name, value.to_string()),
    }
}

fn json_to_property_value(value: Option<&serde_json::Value>) -> PropertyValue {
    match value {
        Some(serde_json::Value::String(s)) => PropertyValue::String(s.clone()),
        Some(serde_json::Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                PropertyValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::Null
            }
        }
        Some(serde_json::Value::Bool(b)) => PropertyValue::Bool(*b),
        Some(serde_json::Value::Array(arr)) => PropertyValue::List(
            arr.iter()
                .map(|v| json_to_property_value(Some(v)))
                .collect(),
        ),
        Some(serde_json::Value::Object(obj)) => PropertyValue::Map(
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_property_value(Some(v))))
                .collect(),
        ),
        Some(serde_json::Value::Null) | None => PropertyValue::Null,
    }
}

// ---------------------------------------------------------------------------
// Schema statement filtering for Memgraph compatibility
// ---------------------------------------------------------------------------

/// Filter schema DDL statements for Memgraph compatibility.
///
/// Memgraph does not support:
/// - Full-text indexes (`CREATE FULLTEXT INDEX ...`)
/// - Vector indexes (`CREATE VECTOR INDEX ...`)
/// - Neo4j 5.x constraint syntax (`FOR (n:Label) REQUIRE ...`)
///
/// Memgraph uses the older Neo4j 4.x constraint syntax:
///   `CREATE CONSTRAINT ON (n:Label) ASSERT n.prop IS UNIQUE`
///
/// This function:
/// 1. Skips unsupported statements entirely (full-text, vector)
/// 2. Rewrites Neo4j 5.x constraints to 4.x syntax
/// 3. Passes through range indexes unchanged
fn filter_schema_for_memgraph(statements: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(statements.len());

    for stmt in statements {
        let upper = stmt.to_uppercase();

        // Skip full-text indexes — Memgraph has no equivalent
        if upper.contains("FULLTEXT INDEX") {
            info!(
                statement = %truncate_query(stmt, 100),
                "Skipping full-text index (unsupported by Memgraph)"
            );
            continue;
        }

        // Skip vector indexes — Memgraph has no equivalent
        if upper.contains("VECTOR INDEX") {
            info!(
                statement = %truncate_query(stmt, 100),
                "Skipping vector index (unsupported by Memgraph)"
            );
            continue;
        }

        // Rewrite Neo4j 5.x UNIQUE constraints to Memgraph syntax
        // From: CREATE CONSTRAINT IF NOT EXISTS FOR (n:Label) REQUIRE (n.prop) IS UNIQUE
        // To:   CREATE CONSTRAINT ON (n:Label) ASSERT n.prop IS UNIQUE
        if upper.contains("IS UNIQUE")
            && upper.contains("REQUIRE")
            && let Some(rewritten) = rewrite_unique_constraint(stmt)
        {
            result.push(rewritten);
            continue;
        }

        // Rewrite Neo4j 5.x EXISTS constraints to Memgraph syntax
        // From: CREATE CONSTRAINT IF NOT EXISTS FOR (n:Label) REQUIRE n.prop IS NOT NULL
        // To:   CREATE CONSTRAINT ON (n:Label) ASSERT EXISTS (n.prop)
        if upper.contains("IS NOT NULL")
            && upper.contains("REQUIRE")
            && let Some(rewritten) = rewrite_exists_constraint(stmt)
        {
            result.push(rewritten);
            continue;
        }

        // Rewrite Neo4j 5.x NODE KEY constraints to Memgraph syntax
        // From: CREATE CONSTRAINT IF NOT EXISTS FOR (n:Label) REQUIRE (n.a, n.b) IS NODE KEY
        // Memgraph does not support NODE KEY — skip with warning
        if upper.contains("IS NODE KEY") {
            info!(
                statement = %truncate_query(stmt, 100),
                "Skipping NODE KEY constraint (unsupported by Memgraph)"
            );
            continue;
        }

        // Rewrite indexes: strip IF NOT EXISTS (Memgraph may not support it)
        // From: CREATE INDEX IF NOT EXISTS FOR (n:Label) ON (n.prop)
        // To:   CREATE INDEX ON :Label(prop)
        if upper.starts_with("CREATE INDEX")
            && upper.contains("FOR (")
            && let Some(rewritten) = rewrite_range_index(stmt)
        {
            result.push(rewritten);
            continue;
        }

        // Pass through unchanged
        result.push(stmt.clone());
    }

    result
}

/// Rewrite a Neo4j 5.x UNIQUE constraint to Memgraph syntax.
///
/// Input:  `CREATE CONSTRAINT IF NOT EXISTS FOR (n:Label) REQUIRE (n.prop) IS UNIQUE`
/// Output: `CREATE CONSTRAINT ON (n:Label) ASSERT n.prop IS UNIQUE`
fn rewrite_unique_constraint(stmt: &str) -> Option<String> {
    let upper = stmt.to_uppercase();

    // Extract the (n:Label) pattern
    let for_pos = upper.find("FOR ")?;
    let after_for = &stmt[for_pos + 4..];
    let paren_open = after_for.find('(')?;
    let paren_close = after_for.find(')')?;
    let node_pattern = after_for[paren_open..=paren_close].trim();

    // Extract the property expression after REQUIRE
    let require_pos = upper.find("REQUIRE ")?;
    let after_require = &stmt[require_pos + 8..];
    let is_unique_pos = after_require.to_uppercase().find("IS UNIQUE")?;
    let prop_expr = after_require[..is_unique_pos]
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')');

    Some(format!(
        "CREATE CONSTRAINT ON {node_pattern} ASSERT {prop_expr} IS UNIQUE"
    ))
}

/// Rewrite a Neo4j 5.x EXISTS constraint to Memgraph syntax.
///
/// Input:  `CREATE CONSTRAINT IF NOT EXISTS FOR (n:Label) REQUIRE n.prop IS NOT NULL`
/// Output: `CREATE CONSTRAINT ON (n:Label) ASSERT EXISTS (n.prop)`
fn rewrite_exists_constraint(stmt: &str) -> Option<String> {
    let upper = stmt.to_uppercase();

    let for_pos = upper.find("FOR ")?;
    let after_for = &stmt[for_pos + 4..];
    let paren_open = after_for.find('(')?;
    let paren_close = after_for.find(')')?;
    let node_pattern = after_for[paren_open..=paren_close].trim();

    let require_pos = upper.find("REQUIRE ")?;
    let after_require = &stmt[require_pos + 8..];
    let is_not_null_pos = after_require.to_uppercase().find("IS NOT NULL")?;
    let prop_expr = after_require[..is_not_null_pos].trim();

    Some(format!(
        "CREATE CONSTRAINT ON {node_pattern} ASSERT EXISTS ({prop_expr})"
    ))
}

/// Rewrite a Neo4j 5.x range index to Memgraph syntax.
///
/// Input:  `CREATE INDEX IF NOT EXISTS FOR (n:Label) ON (n.prop)`
/// Output: `CREATE INDEX ON :Label(prop)`
fn rewrite_range_index(stmt: &str) -> Option<String> {
    let upper = stmt.to_uppercase();

    // Extract label from FOR (n:Label)
    let for_pos = upper.find("FOR (")?;
    let after_for = &stmt[for_pos + 5..];
    let paren_close = after_for.find(')')?;
    let node_def = &after_for[..paren_close]; // e.g., "n:Label" or "n:`My Label`"
    let colon_pos = node_def.find(':')?;
    let label = node_def[colon_pos + 1..].trim();

    // Extract property from ON (n.prop) or ON (n.prop, n.prop2)
    let on_pos = upper.find(" ON (")?;
    let after_on = &stmt[on_pos + 5..];
    let on_close = after_on.find(')')?;
    let props_str = &after_on[..on_close];

    // For single property: extract just the property name
    let props: Vec<&str> = props_str
        .split(',')
        .map(|p| {
            let p = p.trim();
            // n.prop -> prop (strip variable prefix)
            if let Some(dot_pos) = p.find('.') {
                p[dot_pos + 1..].trim()
            } else {
                p
            }
        })
        .collect();

    if props.len() == 1 {
        Some(format!("CREATE INDEX ON :{label}({});", props[0]))
    } else {
        // Memgraph supports composite label-property indexes via multiple CREATE INDEX
        // statements. Emit one per property.
        // However, for simplicity, just create a single statement for the first property
        // and log a warning.
        warn!(
            "Memgraph does not support composite indexes natively; \
             creating single-property index for first property only"
        );
        Some(format!("CREATE INDEX ON :{label}({});", props[0]))
    }
}

// ---------------------------------------------------------------------------
// Retry with exponential backoff
// ---------------------------------------------------------------------------

struct RetryConfig {
    max_retries: u32,
    initial_delay: Duration,
    max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
        }
    }
}

// ---------------------------------------------------------------------------
// Memgraph transient error detector
// ---------------------------------------------------------------------------

/// Memgraph-specific transient error detection.
/// Similar to Neo4j's but includes Memgraph-specific error patterns.
pub struct MemGraphTransienceDetector;

impl TransienceDetector for MemGraphTransienceDetector {
    fn is_transient(&self, err_msg: &str) -> bool {
        let lower = err_msg.to_lowercase();
        lower.contains("connection reset")
            || lower.contains("broken pipe")
            || lower.contains("connection refused")
            || lower.contains("timed out")
            || lower.contains("timeout")
            || lower.contains("too many requests")
            || lower.contains("service unavailable")
            || lower.contains("couldn't connect")
            || lower.contains("server is not available")
            || lower.contains("cluster is not available")
    }
}

async fn with_retry<F, Fut, T>(
    config: &RetryConfig,
    detector: &dyn TransienceDetector,
    operation: F,
) -> Result<T, neo4rs::Error>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, neo4rs::Error>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                let msg = e.to_string();
                if attempt >= config.max_retries || !detector.is_transient(&msg) {
                    return Err(e);
                }
                let delay =
                    std::cmp::min(config.initial_delay * 2u32.pow(attempt), config.max_delay);
                warn!(
                    attempt = attempt + 1,
                    max = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %msg,
                    "Transient Memgraph error, retrying"
                );
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MemGraphRuntime
// ---------------------------------------------------------------------------

/// Default load concurrency for Memgraph (lower than Neo4j due to memory-first arch).
const DEFAULT_LOAD_CONCURRENCY: usize = 4;

/// Maximum concurrent connections for Memgraph (memory-conscious default).
const DEFAULT_MAX_CONNECTIONS: usize = 10;

pub struct MemGraphRuntime {
    graph: Graph,
    load_concurrency: usize,
    retry: RetryConfig,
    detector: Arc<dyn TransienceDetector>,
    isolation: Option<Box<dyn GraphIsolationStrategy>>,
}

impl MemGraphRuntime {
    pub async fn connect(
        uri: &str,
        user: &str,
        password: &str,
        _database: Option<&str>, // Memgraph has no multi-database — ignored
        max_connections: Option<u32>,
        load_concurrency: Option<usize>,
        retry_max: Option<u32>,
        retry_initial_delay_ms: Option<u64>,
        retry_max_delay_ms: Option<u64>,
    ) -> OxResult<Self> {
        let max_conn = max_connections
            .map(|c| c as usize)
            .unwrap_or(DEFAULT_MAX_CONNECTIONS);

        let config = ConfigBuilder::default()
            .uri(uri)
            .user(user)
            .password(password)
            .max_connections(max_conn)
            .build()
            .map_err(|e| OxError::Runtime {
                message: format!("Memgraph config error: {e}"),
            })?;

        let graph = Graph::connect(config).await.map_err(|e| OxError::Runtime {
            message: format!("Memgraph connection error: {e}"),
        })?;

        info!("Connected to Memgraph at {uri}");
        Ok(Self {
            graph,
            load_concurrency: load_concurrency.unwrap_or(DEFAULT_LOAD_CONCURRENCY),
            retry: RetryConfig {
                max_retries: retry_max.unwrap_or(3),
                initial_delay: Duration::from_millis(retry_initial_delay_ms.unwrap_or(100)),
                max_delay: Duration::from_millis(retry_max_delay_ms.unwrap_or(5000)),
            },
            detector: Arc::new(MemGraphTransienceDetector),
            isolation: None,
        })
    }

    /// Set the workspace isolation strategy.
    pub fn with_isolation(mut self, strategy: Box<dyn GraphIsolationStrategy>) -> Self {
        info!(
            strategy = strategy.name(),
            "Graph workspace isolation enabled (Memgraph)"
        );
        self.isolation = Some(strategy);
        self
    }

    /// Apply workspace scoping to a query if isolation is configured.
    fn scope_query(
        &self,
        cypher: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> (String, HashMap<String, PropertyValue>) {
        let strategy = match &self.isolation {
            Some(s) => s,
            None => return (cypher.to_string(), params.clone()),
        };

        if GRAPH_SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
            return (cypher.to_string(), params.clone());
        }

        match GRAPH_WORKSPACE_ID.try_with(|id| id.to_string()) {
            Ok(ws_id) => {
                let scoped = strategy.scope(cypher, &ws_id);
                let mut merged = params.clone();
                for (key, value) in scoped.params {
                    merged.insert(key.to_string(), PropertyValue::String(value));
                }
                (scoped.query, merged)
            }
            Err(_) => (cypher.to_string(), params.clone()),
        }
    }
}

fn validate_identifier(name: &str) -> OxResult<()> {
    if name.is_empty() || name.len() > 63 {
        return Err(OxError::Validation {
            field: "name".to_string(),
            message: "Identifier must be 1-63 characters".to_string(),
        });
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(OxError::Validation {
            field: "name".to_string(),
            message: "Identifier must be alphanumeric or underscore only".to_string(),
        });
    }
    Ok(())
}

#[async_trait]
impl GraphRuntime for MemGraphRuntime {
    fn runtime_name(&self) -> &str {
        "Memgraph"
    }

    async fn execute_schema(&self, statements: &[String]) -> OxResult<()> {
        // Filter and rewrite statements for Memgraph compatibility
        let filtered = filter_schema_for_memgraph(statements);

        if filtered.len() < statements.len() {
            info!(
                original = statements.len(),
                filtered = filtered.len(),
                skipped = statements.len() - filtered.len(),
                "Filtered unsupported schema statements for Memgraph"
            );
        }

        // Memgraph does not support multi-statement transactions for DDL.
        // Execute each statement individually.
        for stmt in &filtered {
            info!(statement = %stmt, "Executing schema statement (Memgraph)");
            self.graph
                .run(query(stmt))
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!(
                        "Schema execution failed: {e}\nStatement: {}",
                        truncate_query(stmt, 200)
                    ),
                })?;
        }

        Ok(())
    }

    async fn execute_query(
        &self,
        cypher: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> OxResult<QueryResult> {
        let (scoped_cypher, scoped_params) = self.scope_query(cypher, params);
        let start = std::time::Instant::now();

        let mut result = with_retry(&self.retry, self.detector.as_ref(), || {
            let q = bind_params(query(&scoped_cypher), &scoped_params);
            self.graph.execute(q)
        })
        .await
        .map_err(|e| OxError::Runtime {
            message: format!(
                "Query execution failed: {e}\nQuery: {}",
                truncate_query(cypher, 200)
            ),
        })?;

        let mut columns: Vec<String> = Vec::new();
        let mut rows: Vec<Vec<PropertyValue>> = Vec::new();

        while let Some(row) = result.next().await.map_err(|e| OxError::Runtime {
            message: format!("Failed to fetch row: {e}"),
        })? {
            let map: HashMap<String, serde_json::Value> =
                row.to().map_err(|e| OxError::Runtime {
                    message: format!("Failed to deserialize row: {e}"),
                })?;

            if columns.is_empty() {
                columns = map.keys().cloned().collect();
                columns.sort();
            }

            let values: Vec<PropertyValue> = columns
                .iter()
                .map(|col| json_to_property_value(map.get(col)))
                .collect();
            rows.push(values);
        }

        let elapsed = start.elapsed();
        let row_count = rows.len();

        Ok(QueryResult {
            columns,
            rows,
            metadata: QueryMetadata {
                execution_time_ms: elapsed.as_millis() as u64,
                rows_returned: row_count,
                nodes_affected: None,
                edges_affected: None,
            },
        })
    }

    async fn execute_load(&self, cypher: &str, batch: LoadBatch) -> OxResult<LoadResult> {
        use futures::stream::{FuturesUnordered, StreamExt};

        let (cypher, _scoped_params) = self.scope_query(cypher, &HashMap::new());
        let cypher = &cypher;

        let max_concurrent = self.load_concurrency;

        let mut result = LoadResult {
            nodes_created: 0,
            nodes_updated: 0,
            edges_created: 0,
            edges_updated: 0,
            batches_processed: 0,
            batches_failed: 0,
            errors: Vec::new(),
        };

        type BatchFuture = std::pin::Pin<
            Box<dyn std::future::Future<Output = (usize, Result<(), neo4rs::Error>)> + Send>,
        >;
        let mut futures: FuturesUnordered<BatchFuture> = FuturesUnordered::new();
        let mut iter = batch.into_records().into_iter().enumerate();

        let retry_max_retries = self.retry.max_retries;
        let retry_initial_delay = self.retry.initial_delay;
        let retry_max_delay = self.retry.max_delay;
        let detector = Arc::clone(&self.detector);

        let isolation_params: Vec<(String, String)> = _scoped_params
            .iter()
            .filter_map(|(k, v)| {
                if let PropertyValue::String(s) = v {
                    Some((k.clone(), s.clone()))
                } else {
                    None
                }
            })
            .collect();

        let spawn_batch = |futures: &mut FuturesUnordered<BatchFuture>,
                           i: usize,
                           record: serde_json::Map<String, serde_json::Value>,
                           graph: Graph,
                           cypher: String,
                           detector: Arc<dyn TransienceDetector>,
                           iso_params: Vec<(String, String)>| {
            let field_pairs: Vec<(String, serde_json::Value)> = record.into_iter().collect();

            futures.push(Box::pin(async move {
                let retry = RetryConfig {
                    max_retries: retry_max_retries,
                    initial_delay: retry_initial_delay,
                    max_delay: retry_max_delay,
                };
                let res = with_retry(&retry, detector.as_ref(), || {
                    let mut q = query(&cypher);
                    for (key, val) in &field_pairs {
                        q = bind_json_field(q, &format!("row_{key}"), val);
                    }
                    for (key, val) in &iso_params {
                        q = q.param(key.as_str(), val.as_str());
                    }
                    graph.run(q)
                })
                .await;
                (i, res)
            }));
        };

        for _ in 0..max_concurrent {
            if let Some((i, record)) = iter.next() {
                spawn_batch(
                    &mut futures,
                    i,
                    record,
                    self.graph.clone(),
                    cypher.to_owned(),
                    Arc::clone(&detector),
                    isolation_params.clone(),
                );
            } else {
                break;
            }
        }

        while let Some((idx, res)) = futures.next().await {
            match res {
                Ok(()) => {
                    result.batches_processed += 1;
                }
                Err(e) => {
                    let msg = format!("Batch {idx} failed: {e}");
                    warn!(%msg);
                    result.batches_failed += 1;
                    result.errors.push(crate::LoadError {
                        batch_index: idx,
                        message: msg,
                    });
                }
            }

            if let Some((i, record)) = iter.next() {
                spawn_batch(
                    &mut futures,
                    i,
                    record,
                    self.graph.clone(),
                    cypher.to_owned(),
                    Arc::clone(&detector),
                    isolation_params.clone(),
                );
            }
        }

        Ok(result)
    }

    async fn create_sandbox(&self, name: &str) -> OxResult<SandboxHandle> {
        validate_identifier(name)?;
        // Memgraph has no multi-database support.
        // Use label-prefix isolation: all sandbox nodes get a `_sandbox_<name>` label.
        let sandbox_label = format!("_sandbox_{name}");
        info!(label = %sandbox_label, "Created Memgraph sandbox (label-based isolation)");
        Ok(SandboxHandle {
            name: name.to_string(),
            database: sandbox_label,
        })
    }

    async fn drop_sandbox(&self, handle: &SandboxHandle) -> OxResult<()> {
        validate_identifier(&handle.name)?;
        // Delete all nodes with the sandbox label
        let cypher = format!("MATCH (n:`{}`) DETACH DELETE n", handle.database);
        self.graph
            .run(query(&cypher))
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to drop sandbox: {e}"),
            })?;
        info!(label = %handle.database, "Dropped Memgraph sandbox");
        Ok(())
    }

    async fn health_check(&self) -> bool {
        tokio::time::timeout(Duration::from_secs(2), self.graph.run(query("RETURN 1")))
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false)
    }

    // ---- Graph exploration (Memgraph Cypher implementation) ----

    async fn search_nodes(
        &self,
        search_query: &str,
        limit: usize,
        labels: Option<&[String]>,
    ) -> OxResult<Vec<ox_core::graph_exploration::SearchResultNode>> {
        use ox_core::graph_exploration::SearchResultNode;

        let label_match = match labels {
            Some(lbls) if !lbls.is_empty() => {
                let safe: Vec<&str> = lbls
                    .iter()
                    .filter(|l| l.chars().all(|c| c.is_alphanumeric() || c == '_'))
                    .map(|l| l.as_str())
                    .collect();
                if safe.is_empty() {
                    "MATCH (n)".to_string()
                } else {
                    let clauses: Vec<String> = safe.iter().map(|l| format!("n:`{l}`")).collect();
                    format!("MATCH (n) WHERE ({})", clauses.join(" OR "))
                }
            }
            _ => "MATCH (n)".to_string(),
        };

        let has_where = label_match.contains("WHERE");
        let property_clause = if search_query == "*" {
            String::new()
        } else if has_where {
            " AND any(key IN keys(n) WHERE toString(n[key]) CONTAINS $search)".to_string()
        } else {
            " WHERE any(key IN keys(n) WHERE toString(n[key]) CONTAINS $search)".to_string()
        };

        // Memgraph uses id(n) instead of elementId(n)
        let cypher = format!(
            "{label_match}{property_clause} \
             RETURN labels(n) AS labels, properties(n) AS props, id(n) AS element_id \
             LIMIT $limit"
        );

        let mut params = HashMap::new();
        if search_query != "*" {
            params.insert(
                "search".to_string(),
                PropertyValue::String(search_query.to_string()),
            );
        }
        params.insert("limit".to_string(), PropertyValue::Int(limit as i64));

        let result = self.execute_query(&cypher, &params).await?;

        let col_idx: HashMap<&str, usize> = result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let nodes = result
            .rows
            .into_iter()
            .filter_map(|row| {
                let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));

                let element_id = match get("element_id")? {
                    PropertyValue::String(s) => s.clone(),
                    PropertyValue::Int(i) => i.to_string(),
                    other => other.to_string(),
                };
                let labels = match get("labels") {
                    Some(PropertyValue::List(arr)) => arr
                        .iter()
                        .filter_map(|v| match v {
                            PropertyValue::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect(),
                    _ => vec![],
                };
                let props: HashMap<String, serde_json::Value> = match get("props") {
                    Some(PropertyValue::Map(m)) => m
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                        .collect(),
                    _ => HashMap::new(),
                };

                Some(SearchResultNode {
                    element_id,
                    labels,
                    props,
                })
            })
            .collect();

        Ok(nodes)
    }

    async fn expand_node(
        &self,
        element_id: &str,
        limit: usize,
    ) -> OxResult<ox_core::graph_exploration::NodeExpansion> {
        use ox_core::graph_exploration::{ExpandNeighbor, NodeExpansion};

        // Memgraph uses id(n) (integer) instead of elementId(n) (string).
        // Try parsing as integer first; fall back to string comparison.
        let (id_clause, id_param) = if let Ok(int_id) = element_id.parse::<i64>() {
            ("id(n) = $id".to_string(), PropertyValue::Int(int_id))
        } else {
            (
                "toString(id(n)) = $id".to_string(),
                PropertyValue::String(element_id.to_string()),
            )
        };

        let cypher = format!(
            "MATCH (n)-[r]-(m) WHERE {id_clause} \
             RETURN type(r) AS rel_type, \
                    labels(m) AS labels, \
                    properties(m) AS props, \
                    id(m) AS element_id, \
                    CASE WHEN startNode(r) = n THEN 'outgoing' ELSE 'incoming' END AS direction \
             LIMIT $limit"
        );

        let mut params = HashMap::new();
        params.insert("id".to_string(), id_param);
        params.insert("limit".to_string(), PropertyValue::Int(limit as i64));

        let result = self.execute_query(&cypher, &params).await?;

        let col_idx: HashMap<&str, usize> = result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let neighbors = result
            .rows
            .into_iter()
            .filter_map(|row| {
                let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));

                let eid = match get("element_id")? {
                    PropertyValue::String(s) => s.clone(),
                    PropertyValue::Int(i) => i.to_string(),
                    other => other.to_string(),
                };
                let labels = match get("labels") {
                    Some(PropertyValue::List(arr)) => arr
                        .iter()
                        .filter_map(|v| match v {
                            PropertyValue::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect(),
                    _ => vec![],
                };
                let props: HashMap<String, serde_json::Value> = match get("props") {
                    Some(PropertyValue::Map(m)) => m
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                        .collect(),
                    _ => HashMap::new(),
                };
                let relationship_type = match get("rel_type")? {
                    PropertyValue::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let direction = match get("direction") {
                    Some(PropertyValue::String(s)) => s.clone(),
                    _ => "outgoing".to_string(),
                };

                Some(ExpandNeighbor {
                    element_id: eid,
                    labels,
                    props,
                    relationship_type,
                    direction,
                })
            })
            .collect();

        Ok(NodeExpansion {
            source_id: element_id.to_string(),
            neighbors,
        })
    }

    async fn graph_overview(&self) -> OxResult<ox_core::graph_exploration::GraphSchemaOverview> {
        use ox_core::graph_exploration::{GraphSchemaOverview, LabelStat, RelationshipPattern};

        let empty_params = HashMap::new();

        // Memgraph label statistics — uses CALL db.labels() like Neo4j
        // but subqueries (CALL { ... }) are not supported, so we use
        // a simpler approach: get labels first, then count.
        let label_cypher = "\
            MATCH (n) \
            UNWIND labels(n) AS label \
            RETURN label, count(n) AS cnt \
            ORDER BY cnt DESC";

        let label_result = self.execute_query(label_cypher, &empty_params).await?;
        let label_col_idx: HashMap<&str, usize> = label_result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let mut labels = Vec::new();
        let mut total_nodes: i64 = 0;
        for row in &label_result.rows {
            let get = |name: &str| label_col_idx.get(name).and_then(|&i| row.get(i));
            let label = match get("label") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let count = match get("cnt") {
                Some(PropertyValue::Int(n)) => *n,
                _ => 0,
            };
            total_nodes += count;
            labels.push(LabelStat { label, count });
        }

        // Relationship patterns
        let rel_cypher = "\
            MATCH (a)-[r]->(b) \
            WITH labels(a)[0] AS from_label, type(r) AS rel_type, labels(b)[0] AS to_label, count(*) AS cnt \
            RETURN from_label, rel_type, to_label, cnt \
            ORDER BY cnt DESC LIMIT 50";

        let rel_result = self.execute_query(rel_cypher, &empty_params).await?;
        let rel_col_idx: HashMap<&str, usize> = rel_result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let mut relationships = Vec::new();
        let mut total_relationships: i64 = 0;
        for row in &rel_result.rows {
            let get = |name: &str| rel_col_idx.get(name).and_then(|&i| row.get(i));
            let from_label = match get("from_label") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let rel_type = match get("rel_type") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let to_label = match get("to_label") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let count = match get("cnt") {
                Some(PropertyValue::Int(n)) => *n,
                _ => 0,
            };
            total_relationships += count;
            relationships.push(RelationshipPattern {
                from_label,
                rel_type,
                to_label,
                count,
            });
        }

        // Memgraph does not support SHOW NODE TYPE PROPERTIES or
        // db.schema.nodeTypeProperties(). Property introspection is skipped.
        let node_properties = Vec::new();
        let rel_properties = Vec::new();

        Ok(GraphSchemaOverview {
            labels,
            relationships,
            total_nodes,
            total_relationships,
            node_properties,
            rel_properties,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Schema filtering tests ---

    #[test]
    fn filter_skips_fulltext_indexes() {
        let stmts = vec![
            "CREATE FULLTEXT INDEX ft_name IF NOT EXISTS FOR (n:Person) ON EACH [n.name]"
                .to_string(),
            "CREATE INDEX IF NOT EXISTS FOR (n:Person) ON (n.age)".to_string(),
        ];
        let filtered = filter_schema_for_memgraph(&stmts);
        assert_eq!(filtered.len(), 1);
        assert!(!filtered[0].contains("FULLTEXT"));
    }

    #[test]
    fn filter_skips_vector_indexes() {
        let stmts = vec![
            "CREATE VECTOR INDEX IF NOT EXISTS FOR (n:Doc) ON (n.embedding) OPTIONS {indexConfig: {`vector.dimensions`: 1536, `vector.similarity_function`: 'cosine'}}".to_string(),
        ];
        let filtered = filter_schema_for_memgraph(&stmts);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_rewrites_unique_constraint() {
        let stmts = vec![
            "CREATE CONSTRAINT IF NOT EXISTS FOR (n:Person) REQUIRE (n.email) IS UNIQUE"
                .to_string(),
        ];
        let filtered = filter_schema_for_memgraph(&stmts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0],
            "CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE"
        );
    }

    #[test]
    fn filter_rewrites_exists_constraint() {
        let stmts = vec![
            "CREATE CONSTRAINT IF NOT EXISTS FOR (n:Person) REQUIRE n.name IS NOT NULL".to_string(),
        ];
        let filtered = filter_schema_for_memgraph(&stmts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0],
            "CREATE CONSTRAINT ON (n:Person) ASSERT EXISTS (n.name)"
        );
    }

    #[test]
    fn filter_skips_node_key_constraint() {
        let stmts = vec![
            "CREATE CONSTRAINT IF NOT EXISTS FOR (n:Person) REQUIRE (n.first, n.last) IS NODE KEY"
                .to_string(),
        ];
        let filtered = filter_schema_for_memgraph(&stmts);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_rewrites_range_index() {
        let stmts = vec!["CREATE INDEX IF NOT EXISTS FOR (n:Person) ON (n.age)".to_string()];
        let filtered = filter_schema_for_memgraph(&stmts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], "CREATE INDEX ON :Person(age);");
    }

    #[test]
    fn filter_passthrough_plain_statements() {
        let stmts = vec!["MATCH (n) DETACH DELETE n".to_string()];
        let filtered = filter_schema_for_memgraph(&stmts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], "MATCH (n) DETACH DELETE n");
    }

    // --- Transience detector tests ---

    #[test]
    fn memgraph_transience_detector() {
        let detector = MemGraphTransienceDetector;

        // Transient
        assert!(detector.is_transient("Connection reset by peer"));
        assert!(detector.is_transient("broken pipe"));
        assert!(detector.is_transient("Connection refused"));
        assert!(detector.is_transient("request timed out"));
        assert!(detector.is_transient("couldn't connect to server"));
        assert!(detector.is_transient("server is not available"));
        assert!(detector.is_transient("Cluster is not available"));

        // Non-transient
        assert!(!detector.is_transient("Syntax error in Cypher"));
        assert!(!detector.is_transient("Node not found"));
        assert!(!detector.is_transient("Permission denied"));
        assert!(!detector.is_transient(""));
    }

    // --- Constraint rewriting tests ---

    #[test]
    fn rewrite_unique_simple() {
        let stmt = "CREATE CONSTRAINT IF NOT EXISTS FOR (n:Person) REQUIRE (n.email) IS UNIQUE";
        let result = rewrite_unique_constraint(stmt).unwrap();
        assert_eq!(
            result,
            "CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE"
        );
    }

    #[test]
    fn rewrite_exists_simple() {
        let stmt = "CREATE CONSTRAINT IF NOT EXISTS FOR (n:Person) REQUIRE n.name IS NOT NULL";
        let result = rewrite_exists_constraint(stmt).unwrap();
        assert_eq!(
            result,
            "CREATE CONSTRAINT ON (n:Person) ASSERT EXISTS (n.name)"
        );
    }

    #[test]
    fn rewrite_range_index_simple() {
        let stmt = "CREATE INDEX IF NOT EXISTS FOR (n:Person) ON (n.age)";
        let result = rewrite_range_index(stmt).unwrap();
        assert_eq!(result, "CREATE INDEX ON :Person(age);");
    }
}
