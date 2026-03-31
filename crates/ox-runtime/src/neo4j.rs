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
use crate::{GRAPH_SYSTEM_BYPASS, GRAPH_WORKSPACE_ID, GraphRuntime, LoadBatch, LoadResult, SandboxHandle, TransienceDetector};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Truncate a query string for inclusion in error messages.
fn truncate_query(q: &str, max: usize) -> String {
    if q.len() <= max {
        q.to_string()
    } else {
        format!("{}...", &q[..max])
    }
}

/// Bind `PropertyValue` parameters onto a neo4rs `Query`.
fn bind_params(q: neo4rs::Query, params: &HashMap<String, PropertyValue>) -> neo4rs::Query {
    let mut q = q;
    for (name, value) in params {
        q = match value {
            PropertyValue::Bool(b) => q.param(name, *b),
            PropertyValue::Int(i) => q.param(name, *i),
            PropertyValue::Float(f) => q.param(name, *f),
            PropertyValue::String(s) => q.param(name, s.as_str()),
            PropertyValue::List(items) => {
                // neo4rs 0.8 doesn't support list params directly; serialize as JSON string
                let json = serde_json::to_string(items).unwrap_or_default();
                q.param(name, json)
            }
            PropertyValue::Map(map) => {
                let json = serde_json::to_string(map).unwrap_or_default();
                q.param(name, json)
            }
            _ => q, // Skip Null, Date, DateTime, Duration, Bytes (handled inline in Cypher)
        };
    }
    q
}

/// Bind a single `serde_json::Value` as a neo4rs parameter with the correct type.
/// Used by `execute_load` to pass per-record fields as `$row_<field>` parameters.
fn bind_json_field(q: neo4rs::Query, name: &str, value: &serde_json::Value) -> neo4rs::Query {
    match value {
        serde_json::Value::String(s) => q.param(name, s.as_str()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.param(name, i)
            } else if let Some(f) = n.as_f64() {
                q.param(name, f)
            } else {
                // Fallback: serialize as string
                q.param(name, n.to_string())
            }
        }
        serde_json::Value::Bool(b) => q.param(name, *b),
        serde_json::Value::Null => q, // skip nulls — Cypher handles missing params gracefully
        // Arrays and objects: serialize as JSON string (best-effort)
        _ => q.param(name, value.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Retry with exponential backoff
// ---------------------------------------------------------------------------

/// Retry configuration for transient failures.
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

/// Neo4j-specific transient error detection.
pub struct Neo4jTransienceDetector;

impl TransienceDetector for Neo4jTransienceDetector {
    fn is_transient(&self, err_msg: &str) -> bool {
        let lower = err_msg.to_lowercase();
        lower.contains("connection reset")
            || lower.contains("broken pipe")
            || lower.contains("connection refused")
            || lower.contains("timed out")
            || lower.contains("timeout")
            || lower.contains("too many requests")
            || lower.contains("service unavailable")
            || lower.contains("leader switch")
            || lower.contains("no longer available")
            || lower.contains("database unavailable")
    }
}

/// Execute an async operation with exponential backoff on transient failures.
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
                    "Transient Neo4j error, retrying"
                );
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Neo4jRuntime — executes compiled Cypher against Neo4j via Bolt protocol
// ---------------------------------------------------------------------------

pub struct Neo4jRuntime {
    graph: Graph,
    load_concurrency: usize,
    retry: RetryConfig,
    detector: Arc<dyn TransienceDetector>,
    /// Workspace isolation strategy. When set, all queries are automatically
    /// scoped by the current workspace (read from GRAPH_WORKSPACE_ID task-local).
    isolation: Option<Box<dyn GraphIsolationStrategy>>,
}

impl Neo4jRuntime {
    pub async fn connect(
        uri: &str,
        user: &str,
        password: &str,
        database: Option<&str>,
        max_connections: Option<u32>,
        load_concurrency: Option<usize>,
        retry_max: Option<u32>,
        retry_initial_delay_ms: Option<u64>,
        retry_max_delay_ms: Option<u64>,
    ) -> OxResult<Self> {
        let mut builder = ConfigBuilder::default()
            .uri(uri)
            .user(user)
            .password(password);

        if let Some(db) = database {
            builder = builder.db(db);
        }

        if let Some(max_conn) = max_connections {
            builder = builder.max_connections(max_conn as usize);
        }

        let config = builder.build().map_err(|e| OxError::Runtime {
            message: format!("Neo4j config error: {e}"),
        })?;

        let graph = Graph::connect(config).await.map_err(|e| OxError::Runtime {
            message: format!("Neo4j connection error: {e}"),
        })?;

        info!("Connected to Neo4j at {uri}");
        Ok(Self {
            graph,
            load_concurrency: load_concurrency.unwrap_or(8),
            retry: RetryConfig {
                max_retries: retry_max.unwrap_or(3),
                initial_delay: Duration::from_millis(retry_initial_delay_ms.unwrap_or(100)),
                max_delay: Duration::from_millis(retry_max_delay_ms.unwrap_or(5000)),
            },
            detector: Arc::new(Neo4jTransienceDetector),
            isolation: None,
        })
    }

    /// Set the workspace isolation strategy.
    /// When set, all queries are automatically scoped to the current workspace
    /// via the GRAPH_WORKSPACE_ID task-local.
    pub fn with_isolation(mut self, strategy: Box<dyn GraphIsolationStrategy>) -> Self {
        info!(strategy = strategy.name(), "Graph workspace isolation enabled");
        self.isolation = Some(strategy);
        self
    }

    /// Apply workspace scoping to a query if isolation is configured.
    /// Reads GRAPH_WORKSPACE_ID / GRAPH_SYSTEM_BYPASS from task-locals.
    fn scope_query(
        &self,
        cypher: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> (String, HashMap<String, PropertyValue>) {
        let strategy = match &self.isolation {
            Some(s) => s,
            None => return (cypher.to_string(), params.clone()),
        };

        // System bypass: all data visible
        if GRAPH_SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
            return (cypher.to_string(), params.clone());
        }

        // Normal request: scope to workspace
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

/// Validate sandbox name: only alphanumeric + underscore, 1-63 chars.
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
impl GraphRuntime for Neo4jRuntime {
    fn runtime_name(&self) -> &str {
        "Neo4j"
    }

    async fn execute_schema(&self, statements: &[String]) -> OxResult<()> {
        let mut txn = self.graph.start_txn().await.map_err(|e| OxError::Runtime {
            message: format!("Failed to start transaction: {e}"),
        })?;

        for stmt in statements {
            info!(statement = %stmt, "Executing schema statement");
            txn.run(query(stmt)).await.map_err(|e| OxError::Runtime {
                message: format!(
                    "Schema execution failed: {e}\nStatement: {}",
                    truncate_query(stmt, 200)
                ),
            })?;
        }

        txn.commit().await.map_err(|e| OxError::Runtime {
            message: format!("Failed to commit schema transaction: {e}"),
        })?;

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
            let map: std::collections::HashMap<String, serde_json::Value> =
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

    async fn execute_load(
        &self,
        cypher: &str,
        batch: LoadBatch,
    ) -> OxResult<LoadResult> {
        use futures::stream::{FuturesUnordered, StreamExt};

        // Scope the load query for workspace isolation (writes)
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

        // neo4rs Graph wraps an Arc<Pool> — cheap to clone.
        type BatchFuture = std::pin::Pin<
            Box<dyn std::future::Future<Output = (usize, Result<(), neo4rs::Error>)> + Send>,
        >;
        let mut futures: FuturesUnordered<BatchFuture> = FuturesUnordered::new();
        let mut iter = batch.into_records().into_iter().enumerate();

        let retry_max_retries = self.retry.max_retries;
        let retry_initial_delay = self.retry.initial_delay;
        let retry_max_delay = self.retry.max_delay;
        let detector = Arc::clone(&self.detector);

        // Capture isolation params for load batches (e.g., _ws_id)
        let isolation_params: Vec<(String, String)> = _scoped_params
            .iter()
            .filter_map(|(k, v)| {
                if let PropertyValue::String(s) = v { Some((k.clone(), s.clone())) } else { None }
            })
            .collect();

        let spawn_batch = |futures: &mut FuturesUnordered<BatchFuture>,
                           i: usize,
                           record: serde_json::Map<String, serde_json::Value>,
                           graph: Graph,
                           cypher: String,
                           detector: Arc<dyn TransienceDetector>,
                           iso_params: Vec<(String, String)>| {
            // Bind fields as individual $row_<field> params.
            // The compiler generates `$row_<source_column>` placeholders (no UNWIND).
            let field_pairs: Vec<(String, serde_json::Value)> =
                record.into_iter().collect();

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
                    // Bind isolation params (e.g., $_ws_id for workspace scoping)
                    for (key, val) in &iso_params {
                        q = q.param(key.as_str(), val.as_str());
                    }
                    graph.run(q)
                })
                .await;
                (i, res)
            }));
        };

        // Seed initial batch of futures up to max_concurrent
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

        // Process completions and enqueue more to maintain concurrency
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
        let db_name = format!("sandbox_{name}");
        let stmt = format!("CREATE DATABASE `{db_name}` IF NOT EXISTS");
        self.graph
            .run(query(&stmt))
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to create sandbox database: {e}"),
            })?;

        info!(database = %db_name, "Created sandbox");
        Ok(SandboxHandle {
            name: name.to_string(),
            database: db_name,
        })
    }

    async fn drop_sandbox(&self, handle: &SandboxHandle) -> OxResult<()> {
        validate_identifier(&handle.name)?;
        let stmt = format!("DROP DATABASE `{}` IF EXISTS", handle.database);
        self.graph
            .run(query(&stmt))
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to drop sandbox: {e}"),
            })?;

        info!(database = %handle.database, "Dropped sandbox");
        Ok(())
    }

    async fn health_check(&self) -> bool {
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.graph.run(query("RETURN 1")),
        )
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false)
    }

    // ---- Graph exploration (Neo4j / Cypher implementation) ----

    async fn search_nodes(
        &self,
        search_query: &str,
        limit: usize,
        labels: Option<&[String]>,
    ) -> OxResult<Vec<ox_core::graph_exploration::SearchResultNode>> {
        use ox_core::graph_exploration::SearchResultNode;

        // Build label filter using Cypher pattern matching
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

        // Wildcard = match all; otherwise text search across properties
        let has_where = label_match.contains("WHERE");
        let property_clause = if search_query == "*" {
            String::new()
        } else if has_where {
            " AND any(key IN keys(n) WHERE toString(n[key]) CONTAINS $search)".to_string()
        } else {
            " WHERE any(key IN keys(n) WHERE toString(n[key]) CONTAINS $search)".to_string()
        };

        let cypher = format!(
            "{label_match}{property_clause} \
             RETURN labels(n) AS labels, properties(n) AS props, elementId(n) AS element_id \
             LIMIT $limit"
        );

        let mut params = HashMap::new();
        if search_query != "*" {
            params.insert("search".to_string(), PropertyValue::String(search_query.to_string()));
        }
        params.insert("limit".to_string(), PropertyValue::Int(limit as i64));

        let result = self.execute_query(&cypher, &params).await?;

        // Parse QueryResult rows into SearchResultNode
        let col_idx: HashMap<&str, usize> = result
            .columns.iter().enumerate().map(|(i, c)| (c.as_str(), i)).collect();

        let nodes = result.rows.into_iter().filter_map(|row| {
            let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));

            let element_id = match get("element_id")? {
                PropertyValue::String(s) => s.clone(),
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

            Some(SearchResultNode { element_id, labels, props })
        }).collect();

        Ok(nodes)
    }

    async fn expand_node(
        &self,
        element_id: &str,
        limit: usize,
    ) -> OxResult<ox_core::graph_exploration::NodeExpansion> {
        use ox_core::graph_exploration::{ExpandNeighbor, NodeExpansion};

        let cypher = "\
            MATCH (n)-[r]-(m) WHERE elementId(n) = $id \
            RETURN type(r) AS rel_type, \
                   labels(m) AS labels, \
                   properties(m) AS props, \
                   elementId(m) AS element_id, \
                   CASE WHEN startNode(r) = n THEN 'outgoing' ELSE 'incoming' END AS direction \
            LIMIT $limit";

        let mut params = HashMap::new();
        params.insert("id".to_string(), PropertyValue::String(element_id.to_string()));
        params.insert("limit".to_string(), PropertyValue::Int(limit as i64));

        let result = self.execute_query(cypher, &params).await?;

        let col_idx: HashMap<&str, usize> = result
            .columns.iter().enumerate().map(|(i, c)| (c.as_str(), i)).collect();

        let neighbors = result.rows.into_iter().filter_map(|row| {
            let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));

            let eid = match get("element_id")? {
                PropertyValue::String(s) => s.clone(),
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

            Some(ExpandNeighbor { element_id: eid, labels, props, relationship_type, direction })
        }).collect();

        Ok(NodeExpansion { source_id: element_id.to_string(), neighbors })
    }

    async fn graph_overview(&self) -> OxResult<ox_core::graph_exploration::GraphSchemaOverview> {
        use ox_core::graph_exploration::{GraphSchemaOverview, LabelStat, RelationshipPattern, PropertySchema};

        let empty_params = HashMap::new();

        // Label statistics
        let label_cypher = "\
            CALL db.labels() YIELD label \
            CALL { WITH label MATCH (n) WHERE label IN labels(n) RETURN count(n) AS cnt } \
            RETURN label, cnt ORDER BY cnt DESC";

        let label_result = self.execute_query(label_cypher, &empty_params).await?;
        let label_col_idx: HashMap<&str, usize> = label_result
            .columns.iter().enumerate().map(|(i, c)| (c.as_str(), i)).collect();

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
            .columns.iter().enumerate().map(|(i, c)| (c.as_str(), i)).collect();

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
            relationships.push(RelationshipPattern { from_label, rel_type, to_label, count });
        }

        // --- Property introspection ---
        // Neo4j 2026 uses SHOW NODE/RELATIONSHIP TYPE PROPERTIES (GQL standard).
        // Falls back to deprecated db.schema.* procedures for Neo4j 5.x compat.
        let mut node_properties = Vec::new();
        let mut rel_properties = Vec::new();

        // Node properties — try Neo4j 2026 GQL syntax first
        let node_prop_result = self.execute_query(
            "SHOW NODE TYPE PROPERTIES YIELD nodeType, propertyName, propertyTypes, mandatory RETURN nodeType, propertyName, propertyTypes, mandatory",
            &empty_params,
        ).await;
        let node_prop_result = match node_prop_result {
            Ok(r) => Ok(r),
            Err(_) => self.execute_query(
                "CALL db.schema.nodeTypeProperties() YIELD nodeType, propertyName, propertyTypes, mandatory RETURN nodeType, propertyName, propertyTypes, mandatory",
                &empty_params,
            ).await,
        };
        match node_prop_result {
            Ok(prop_result) => {
                let col_idx: std::collections::HashMap<&str, usize> = prop_result.columns.iter().enumerate().map(|(i, c)| (c.as_str(), i)).collect();
                for row in &prop_result.rows {
                    let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));
                    let entity_type = match get("nodeType") {
                        Some(PropertyValue::String(s)) => s.trim_start_matches(':').trim_matches('`').to_string(),
                        _ => continue,
                    };
                    let property_name = match get("propertyName") {
                        Some(PropertyValue::String(s)) => s.clone(),
                        _ => continue,
                    };
                    let property_types = match get("propertyTypes") {
                        Some(PropertyValue::List(list)) => list.iter().filter_map(|v| {
                            if let PropertyValue::String(s) = v { Some(s.clone()) } else { None }
                        }).collect(),
                        _ => vec!["STRING".to_string()],
                    };
                    let mandatory = matches!(get("mandatory"), Some(PropertyValue::Bool(true)));
                    node_properties.push(PropertySchema { entity_type, property_name, property_types, mandatory });
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "Property introspection not available — skipping node properties");
            }
        }

        // Relationship properties — try Neo4j 2026 GQL syntax first
        let rel_prop_result = self.execute_query(
            "SHOW RELATIONSHIP TYPE PROPERTIES YIELD relType, propertyName, propertyTypes, mandatory RETURN relType, propertyName, propertyTypes, mandatory",
            &empty_params,
        ).await;
        let rel_prop_result = match rel_prop_result {
            Ok(r) => Ok(r),
            Err(_) => self.execute_query(
                "CALL db.schema.relTypeProperties() YIELD relType, propertyName, propertyTypes, mandatory RETURN relType, propertyName, propertyTypes, mandatory",
                &empty_params,
            ).await,
        };
        match rel_prop_result {
            Ok(prop_result) => {
                let col_idx: std::collections::HashMap<&str, usize> = prop_result.columns.iter().enumerate().map(|(i, c)| (c.as_str(), i)).collect();
                for row in &prop_result.rows {
                    let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));
                    let entity_type = match get("relType") {
                        Some(PropertyValue::String(s)) => s.trim_start_matches(':').trim_matches('`').to_string(),
                        _ => continue,
                    };
                    let property_name = match get("propertyName") {
                        Some(PropertyValue::String(s)) => s.clone(),
                        _ => continue,
                    };
                    let property_types = match get("propertyTypes") {
                        Some(PropertyValue::List(list)) => list.iter().filter_map(|v| {
                            if let PropertyValue::String(s) = v { Some(s.clone()) } else { None }
                        }).collect(),
                        _ => vec!["STRING".to_string()],
                    };
                    let mandatory = matches!(get("mandatory"), Some(PropertyValue::Bool(true)));
                    rel_properties.push(PropertySchema { entity_type, property_name, property_types, mandatory });
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "Property introspection not available — skipping relationship properties");
            }
        }

        Ok(GraphSchemaOverview { labels, relationships, total_nodes, total_relationships, node_properties, rel_properties })
    }
}

// ---------------------------------------------------------------------------
// Value extraction helpers
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;
    use neo4rs::query;
    use ox_core::error::OxError;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // bind_params tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bind_params_string() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), PropertyValue::String("Alice".to_string()));
        params.insert("age".to_string(), PropertyValue::Int(30));
        params.insert("score".to_string(), PropertyValue::Float(9.5));
        params.insert("active".to_string(), PropertyValue::Bool(true));

        // bind_params should not panic; it returns a Query with params bound.
        // neo4rs::Query is opaque (no Debug), so we just verify it completes.
        let _q = bind_params(query("MATCH (n) WHERE n.name = $name RETURN n"), &params);
    }

    #[test]
    fn test_bind_params_null_skipped() {
        let mut params = HashMap::new();
        params.insert("value".to_string(), PropertyValue::Null);

        // Null values should be skipped (fall through the `_ => q` arm).
        // The query object should remain valid.
        let _q = bind_params(query("RETURN $value"), &params);
    }

    #[test]
    fn test_bind_params_list_json_serialized() {
        let mut params = HashMap::new();
        params.insert(
            "tags".to_string(),
            PropertyValue::List(vec![
                PropertyValue::String("a".to_string()),
                PropertyValue::String("b".to_string()),
            ]),
        );

        // List values are JSON-serialized as strings before binding.
        let _q = bind_params(query("RETURN $tags"), &params);
    }

    // -----------------------------------------------------------------------
    // json_to_property_value tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_json_to_property_value_all_types() {
        // String
        assert_eq!(
            json_to_property_value(Some(&json!("hello"))),
            PropertyValue::String("hello".to_string())
        );

        // Integer
        assert_eq!(
            json_to_property_value(Some(&json!(42))),
            PropertyValue::Int(42)
        );

        // Float
        assert_eq!(
            json_to_property_value(Some(&json!(3.14))),
            PropertyValue::Float(3.14)
        );

        // Boolean
        assert_eq!(
            json_to_property_value(Some(&json!(true))),
            PropertyValue::Bool(true)
        );
        assert_eq!(
            json_to_property_value(Some(&json!(false))),
            PropertyValue::Bool(false)
        );

        // Null
        assert_eq!(
            json_to_property_value(Some(&json!(null))),
            PropertyValue::Null
        );

        // None
        assert_eq!(json_to_property_value(None), PropertyValue::Null);
    }

    #[test]
    fn test_json_to_property_value_nested() {
        // Array
        let arr = json!([1, "two", true]);
        match json_to_property_value(Some(&arr)) {
            PropertyValue::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], PropertyValue::Int(1));
                assert_eq!(items[1], PropertyValue::String("two".to_string()));
                assert_eq!(items[2], PropertyValue::Bool(true));
            }
            other => panic!("Expected List, got {other:?}"),
        }

        // Object
        let obj = json!({"key": "value", "num": 99});
        match json_to_property_value(Some(&obj)) {
            PropertyValue::Map(map) => {
                assert_eq!(map.len(), 2);
                assert_eq!(
                    map.get("key"),
                    Some(&PropertyValue::String("value".to_string()))
                );
                assert_eq!(map.get("num"), Some(&PropertyValue::Int(99)));
            }
            other => panic!("Expected Map, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // validate_identifier tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_identifier_valid() {
        assert!(validate_identifier("test").is_ok());
        assert!(validate_identifier("my_sandbox").is_ok());
        assert!(validate_identifier("sandbox123").is_ok());
        assert!(validate_identifier("a").is_ok());
        // 63 characters — maximum allowed
        let max_len = "a".repeat(63);
        assert!(validate_identifier(&max_len).is_ok());
    }

    #[test]
    fn test_validate_identifier_invalid() {
        // Empty
        let err = validate_identifier("").unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));

        // Too long (64 chars)
        let too_long = "a".repeat(64);
        let err = validate_identifier(&too_long).unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));

        // SQL injection attempt
        let err = validate_identifier("test; DROP DATABASE neo4j").unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));

        // Backtick injection
        let err = validate_identifier("test`; DROP").unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));

        // Spaces not allowed
        let err = validate_identifier("my sandbox").unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));

        // Special characters
        let err = validate_identifier("test-name").unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));

        let err = validate_identifier("test.name").unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));
    }

    // -----------------------------------------------------------------------
    // Neo4jTransienceDetector tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_neo4j_transience_detector_various_messages() {
        let detector = Neo4jTransienceDetector;

        // Transient errors — should return true
        assert!(detector.is_transient("Connection reset by peer"));
        assert!(detector.is_transient("broken pipe"));
        assert!(detector.is_transient("Connection refused"));
        assert!(detector.is_transient("request timed out"));
        assert!(detector.is_transient("operation timeout"));
        assert!(detector.is_transient("Too many requests"));
        assert!(detector.is_transient("Service unavailable"));
        assert!(detector.is_transient("Leader switch in progress"));
        assert!(detector.is_transient("Database no longer available"));
        assert!(detector.is_transient("database unavailable"));

        // Case insensitive
        assert!(detector.is_transient("CONNECTION RESET"));
        assert!(detector.is_transient("BROKEN PIPE"));

        // Non-transient errors — should return false
        assert!(!detector.is_transient("Syntax error in Cypher"));
        assert!(!detector.is_transient("Node not found"));
        assert!(!detector.is_transient("Permission denied"));
        assert!(!detector.is_transient("Invalid query"));
        assert!(!detector.is_transient(""));
    }

    // -----------------------------------------------------------------------
    // truncate_query tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_truncate_query_short() {
        let short = "MATCH (n) RETURN n";
        assert_eq!(truncate_query(short, 100), short);
    }

    #[test]
    fn test_truncate_query_long() {
        let long = "a".repeat(300);
        let result = truncate_query(&long, 50);
        assert_eq!(result.len(), 53); // 50 chars + "..."
        assert!(result.ends_with("..."));
    }

    // -----------------------------------------------------------------------
    // LoadBatch tests (defined in lib.rs, tested here for convenience)
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_batch_valid_objects() {
        let values = vec![
            json!({"name": "Alice", "age": 30}),
            json!({"name": "Bob", "age": 25}),
        ];
        let batch = LoadBatch::from_values(values).expect("valid objects should be accepted");
        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());

        let records = batch.records();
        assert_eq!(records[0].get("name").unwrap(), "Alice");
        assert_eq!(records[1].get("name").unwrap(), "Bob");
    }

    #[test]
    fn test_load_batch_rejects_non_objects() {
        // Array
        let values = vec![json!([1, 2, 3])];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, message } => {
                assert_eq!(field, "batch[0]");
                assert!(message.contains("array"), "message should mention 'array': {message}");
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }

        // String
        let values = vec![json!("just a string")];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, message } => {
                assert_eq!(field, "batch[0]");
                assert!(message.contains("string"), "message should mention 'string': {message}");
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }

        // Null
        let values = vec![json!(null)];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, message } => {
                assert_eq!(field, "batch[0]");
                assert!(message.contains("null"), "message should mention 'null': {message}");
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }

        // Number
        let values = vec![json!(42)];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, message } => {
                assert_eq!(field, "batch[0]");
                assert!(message.contains("number"), "message should mention 'number': {message}");
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }

        // Boolean
        let values = vec![json!(true)];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, message } => {
                assert_eq!(field, "batch[0]");
                assert!(message.contains("boolean"), "message should mention 'boolean': {message}");
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }

        // Mixed: valid object then invalid
        let values = vec![json!({"valid": true}), json!("invalid")];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, .. } => {
                assert_eq!(field, "batch[1]", "should report index of the failing element");
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn test_load_batch_empty_is_ok() {
        let batch = LoadBatch::from_values(vec![]).expect("empty vec should be valid");
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
        assert!(batch.records().is_empty());
    }

    #[test]
    fn test_load_batch_into_records() {
        let values = vec![json!({"x": 1}), json!({"y": 2})];
        let batch = LoadBatch::from_values(values).unwrap();
        let records = batch.into_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].get("x").unwrap(), 1);
        assert_eq!(records[1].get("y").unwrap(), 2);
    }
}
