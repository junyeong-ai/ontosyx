use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use neo4rs::{ConfigBuilder, Graph, query};
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_core::graph_exploration::{GraphSchemaOverview, NodeExpansion, SearchResultNode};
use ox_core::query_ir::{QueryMetadata, QueryResult};
use ox_core::types::PropertyValue;

use crate::isolation::GraphIsolationStrategy;
use crate::{
    GRAPH_SYSTEM_BYPASS, GRAPH_WORKSPACE_ID, GraphRuntime, LoadBatch, LoadResult, SandboxHandle,
    TransienceDetector,
};

use super::helpers::{bind_params, json_to_property_value, truncate_query, validate_identifier};
use super::retry::{RetryConfig, with_retry};
use super::transience::Neo4jTransienceDetector;

/// Neo4jRuntime — executes compiled Cypher against Neo4j via Bolt protocol.
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
        info!(
            strategy = strategy.name(),
            "Graph workspace isolation enabled"
        );
        self.isolation = Some(strategy);
        self
    }

    /// Bridge for `load.rs`, which still needs to scope the load query
    /// outside the trait pipeline (the trait hooks fire on `execute_query`,
    /// not bulk loads).
    pub(super) fn scope_query(
        &self,
        cypher: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> (String, HashMap<String, PropertyValue>) {
        self.pre_execute(cypher, params)
    }

    // -- Internal accessors used by the load/ and search/ submodules --

    pub(super) fn graph_clone(&self) -> Graph {
        self.graph.clone()
    }

    pub(super) fn load_concurrency(&self) -> usize {
        self.load_concurrency
    }

    pub(super) fn retry_max_retries(&self) -> u32 {
        self.retry.max_retries
    }

    pub(super) fn retry_initial_delay(&self) -> Duration {
        self.retry.initial_delay
    }

    pub(super) fn retry_max_delay(&self) -> Duration {
        self.retry.max_delay
    }

    pub(super) fn detector_arc(&self) -> Arc<dyn TransienceDetector> {
        Arc::clone(&self.detector)
    }
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

    fn pre_execute(
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

    async fn execute_query_raw(
        &self,
        cypher: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> OxResult<QueryResult> {
        let start = std::time::Instant::now();

        let mut result = with_retry(&self.retry, self.detector.as_ref(), || {
            let q = bind_params(query(cypher), params);
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
        self.execute_load_impl(cypher, batch).await
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

    async fn search_nodes(
        &self,
        search_query: &str,
        limit: usize,
        labels: Option<&[String]>,
    ) -> OxResult<Vec<SearchResultNode>> {
        self.search_nodes_impl(search_query, limit, labels).await
    }

    async fn expand_node(&self, element_id: &str, limit: usize) -> OxResult<NodeExpansion> {
        self.expand_node_impl(element_id, limit).await
    }

    async fn graph_overview(&self) -> OxResult<GraphSchemaOverview> {
        self.graph_overview_impl().await
    }
}
