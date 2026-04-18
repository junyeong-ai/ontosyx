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
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::query_ir::{QueryMetadata, QueryResult};
use ox_core::types::PropertyValue;

use crate::bolt::{
    LoadContext, RetryConfig, bind_params, json_to_property_value, run_batched_load,
    run_pre_execute, truncate_query, validate_identifier, with_retry,
};
use crate::isolation::GraphIsolationStrategy;
use crate::{GraphRuntime, LoadBatch, LoadResult, SandboxHandle, TransienceDetector};

const BACKEND_LABEL: &str = "Memgraph";

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
// Memgraph transient error detector
// ---------------------------------------------------------------------------

/// Memgraph-specific transient error detection.
/// Similar to Neo4j's but includes Memgraph-specific error patterns.
pub struct MemGraphTransienceDetector;

impl TransienceDetector for MemGraphTransienceDetector {
    fn is_transient(&self, err_msg: &str) -> bool {
        crate::transience::classify(&crate::transience::MEMGRAPH_RULES, err_msg).is_transient()
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
}

#[async_trait]
impl GraphRuntime for MemGraphRuntime {
    fn name(&self) -> &str {
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

        let mut result = with_retry(&self.retry, self.detector.as_ref(), BACKEND_LABEL, || {
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
