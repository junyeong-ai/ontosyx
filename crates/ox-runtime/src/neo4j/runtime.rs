use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use neo4rs::{ConfigBuilder, Graph, query};
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_ontology::graph_exploration::{GraphSchemaOverview, NodeExpansion, SearchResultNode};
use ox_query_ir::query::{QueryMetadata, QueryResult};
use ox_core::types::PropertyValue;

use crate::bolt::{
    LoadContext, RetryConfig, bind_params, json_to_property_value, run_batched_load,
    run_pre_execute, truncate_query, validate_identifier, with_retry,
};
use crate::isolation::GraphIsolationStrategy;
use crate::{
    GraphRuntime, LoadBatch, LoadResult, SandboxHandle, TransienceDetector,
    query_timeout_or_default,
};

const BACKEND_LABEL: &str = "Neo4j";

/// Neo4j-specific transient error detection.
///
/// Thin adapter over [`crate::transience::NEO4J_RULES`]. All matching
/// logic lives in the shared classifier — adding a new false-positive
/// case is a one-line rule + regression test there.
pub struct Neo4jTransienceDetector;

impl TransienceDetector for Neo4jTransienceDetector {
    fn is_transient(&self, err_msg: &str) -> bool {
        crate::transience::classify(&crate::transience::NEO4J_RULES, err_msg).is_transient()
    }
}

/// Neo4jRuntime — executes compiled Cypher against Neo4j via the Bolt protocol.
///
/// Backend-specific behavior lives here; everything generic (parameter
/// binding, retry, batched loads, isolation rewriting) is delegated to
/// [`crate::bolt`] so the same code is reused by [`crate::memgraph`].
pub struct Neo4jRuntime {
    graph: Graph,
    load_concurrency: usize,
    retry: RetryConfig,
    detector: Arc<dyn TransienceDetector>,
    /// Workspace isolation strategy. When set, all queries are automatically
    /// scoped by the current workspace (read from `GRAPH_WORKSPACE_ID`).
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
    pub fn with_isolation(mut self, strategy: Box<dyn GraphIsolationStrategy>) -> Self {
        info!(
            strategy = strategy.name(),
            "Graph workspace isolation enabled"
        );
        self.isolation = Some(strategy);
        self
    }
}

#[async_trait]
impl GraphRuntime for Neo4jRuntime {
    fn name(&self) -> &str {
        BACKEND_LABEL
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
    ) -> OxResult<(String, HashMap<String, PropertyValue>)> {
        run_pre_execute(self.isolation.as_deref(), cypher, params)
    }

    async fn execute_query_raw(
        &self,
        cypher: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> OxResult<QueryResult> {
        let start = std::time::Instant::now();
        let timeout = query_timeout_or_default();

        // Wrap the entire execute + row drain so a runaway query —
        // unbounded var-length, accidental Cartesian, missing index
        // — surfaces as a clean `OxError::Runtime` instead of holding
        // the connection until the OS gives up.
        let outcome = tokio::time::timeout(timeout, async {
            let mut result =
                with_retry(&self.retry, self.detector.as_ref(), BACKEND_LABEL, || {
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
            Ok::<(Vec<String>, Vec<Vec<PropertyValue>>), OxError>((columns, rows))
        })
        .await;

        let (columns, rows) = match outcome {
            Ok(inner) => inner?,
            Err(_) => {
                return Err(OxError::Runtime {
                    message: format!(
                        "Query timed out after {}s\nQuery: {}",
                        timeout.as_secs(),
                        truncate_query(cypher, 200)
                    ),
                });
            }
        };

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
                provenance: None,
                warnings: Vec::new(),
            },
        })
    }

    async fn execute_load_raw(
        &self,
        cypher: &str,
        scope_params: &HashMap<String, PropertyValue>,
        batch: LoadBatch,
    ) -> OxResult<LoadResult> {
        let scoped_params = crate::bolt::load::isolation_params_from(scope_params);
        let ctx = LoadContext {
            graph: self.graph.clone(),
            cypher: cypher.to_string(),
            scoped_params,
            max_concurrent: self.load_concurrency,
            retry: self.retry,
            detector: Arc::clone(&self.detector),
            backend_label: BACKEND_LABEL,
        };
        run_batched_load(ctx, batch).await
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
        self.do_search_nodes(search_query, limit, labels).await
    }

    async fn expand_node(&self, element_id: &str, limit: usize) -> OxResult<NodeExpansion> {
        self.do_expand_node(element_id, limit).await
    }

    async fn graph_overview(&self) -> OxResult<GraphSchemaOverview> {
        self.do_graph_overview().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neo4j_transience_detector_various_messages() {
        let detector = Neo4jTransienceDetector;

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

        assert!(detector.is_transient("CONNECTION RESET"));
        assert!(detector.is_transient("BROKEN PIPE"));

        assert!(!detector.is_transient("Syntax error in Cypher"));
        assert!(!detector.is_transient("Node not found"));
        assert!(!detector.is_transient("Permission denied"));
        assert!(!detector.is_transient("Invalid query"));
        assert!(!detector.is_transient(""));
    }
}
