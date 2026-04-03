use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use futures::StreamExt;
use futures::future::Future;
use futures::stream::FuturesUnordered;
use serde::Serialize;
use tokio::time::{Instant, timeout};
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::ontology_ir::OntologyIR;
use ox_core::types::{PropertyValue, escape_cypher_identifier};

use crate::GraphRuntime;

// ---------------------------------------------------------------------------
// Data Profile — statistics gathered from the actual graph database
//
// Used by the ontology enrichment agent to generate accurate property
// descriptions (enum values, ranges, formats) and traversal hints.
// ---------------------------------------------------------------------------

/// Maximum number of distinct values to collect per property.
/// Beyond this threshold, the property is treated as free-text.
const MAX_DISTINCT_VALUES: usize = 30;

// ---------------------------------------------------------------------------
// ProfileConfig — tunables for profiling behaviour
// ---------------------------------------------------------------------------

/// Configuration for graph profiling operations.
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    /// Per-query timeout. Prevents individual profile queries from hanging.
    pub query_timeout: Duration,
    /// Maximum number of sample values to collect per property.
    pub max_sample_size: usize,
    /// Maximum number of concurrent profile queries.
    pub concurrency: usize,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            query_timeout: Duration::from_secs(10),
            max_sample_size: MAX_DISTINCT_VALUES,
            concurrency: 8,
        }
    }
}

impl ProfileConfig {
    /// Adaptive config based on ontology size.
    ///
    /// Concurrency and sample sizes scale with ontology complexity:
    /// - Small (≤100 entities): concurrency 8, full samples — fast profiling for common case
    /// - Large (>100 entities): concurrency 4, reduced samples — protect Neo4j under load
    pub fn for_ontology_size(node_count: usize) -> Self {
        use ox_core::source_analysis::LARGE_ONTOLOGY_THRESHOLD;
        if node_count >= LARGE_ONTOLOGY_THRESHOLD {
            Self {
                query_timeout: Duration::from_secs(10),
                max_sample_size: 10,
                concurrency: 4,
            }
        } else {
            Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DataProfile {
    pub node_profiles: Vec<NodeProfile>,
    pub edge_profiles: Vec<EdgeProfile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeProfile {
    pub label: String,
    pub count: u64,
    pub property_stats: Vec<PropertyStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeProfile {
    pub label: String,
    pub source_type: String,
    pub target_type: String,
    pub count: u64,
    pub avg_out_degree: Option<f64>,
    pub property_stats: Vec<PropertyStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropertyStats {
    pub name: String,
    pub total_count: u64,
    pub null_count: u64,
    pub distinct_count: u64,
    /// Top distinct values (up to MAX_DISTINCT_VALUES). Empty if too many.
    pub sample_values: Vec<String>,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
}

// ---------------------------------------------------------------------------
// profile_graph — collect data statistics from the graph database
// ---------------------------------------------------------------------------

/// Profile the graph database based on an ontology definition.
/// Executes lightweight Cypher queries to gather property statistics.
///
/// Each profile query is subject to the timeout specified in `config`.
///
/// Node/edge types are profiled concurrently (up to `config.concurrency`).
/// Each type runs a single aggregation query that profiles all properties
/// at once, minimising round-trips to Neo4j.
pub async fn profile_graph(
    runtime: &dyn GraphRuntime,
    ontology: &OntologyIR,
    config: &ProfileConfig,
) -> OxResult<DataProfile> {
    let started = Instant::now();
    let node_count = ontology.node_types.len();
    let edge_count = ontology.edge_types.len();

    info!(
        node_count,
        edge_count,
        concurrency = config.concurrency,
        query_timeout_secs = config.query_timeout.as_secs(),
        "Starting graph profiling"
    );

    // --- Profile node types concurrently ---
    type NodeFut<'a> = Pin<Box<dyn Future<Output = (usize, OxResult<NodeProfile>)> + Send + 'a>>;
    let mut node_futures: FuturesUnordered<NodeFut<'_>> = FuturesUnordered::new();

    for (idx, node_type) in ontology.node_types.iter().enumerate() {
        let properties = property_names(&node_type.properties);
        let label = node_type.label.clone();
        node_futures.push(Box::pin(async move {
            let result = profile_node(runtime, &label, &properties, config).await;
            (idx, result)
        }));
    }

    let mut node_profiles: Vec<Option<NodeProfile>> = vec![None; ontology.node_types.len()];
    let mut in_flight = 0usize;

    // Drain with bounded concurrency
    while let Some((idx, result)) =
        next_bounded(&mut node_futures, &mut in_flight, config.concurrency).await
    {
        match result {
            Ok(profile) => node_profiles[idx] = Some(profile),
            Err(e) => {
                warn!(label = %ontology.node_types[idx].label, "Failed to profile node type: {e}")
            }
        }
    }

    let node_profiles: Vec<NodeProfile> = node_profiles.into_iter().flatten().collect();
    let node_elapsed = started.elapsed();

    // --- Profile edge types concurrently ---
    type EdgeFut<'a> = Pin<Box<dyn Future<Output = (usize, OxResult<EdgeProfile>)> + Send + 'a>>;
    let mut edge_futures: FuturesUnordered<EdgeFut<'_>> = FuturesUnordered::new();

    for (idx, edge_type) in ontology.edge_types.iter().enumerate() {
        let source_label = ontology
            .node_label(&edge_type.source_node_id)
            .unwrap_or("UNKNOWN")
            .to_string();
        let target_label = ontology
            .node_label(&edge_type.target_node_id)
            .unwrap_or("UNKNOWN")
            .to_string();
        let properties = property_names(&edge_type.properties);
        let label = edge_type.label.clone();
        edge_futures.push(Box::pin(async move {
            let result = profile_edge(
                runtime,
                &label,
                &source_label,
                &target_label,
                &properties,
                config,
            )
            .await;
            (idx, result)
        }));
    }

    let mut edge_profiles: Vec<Option<EdgeProfile>> = vec![None; ontology.edge_types.len()];
    in_flight = 0;

    while let Some((idx, result)) =
        next_bounded(&mut edge_futures, &mut in_flight, config.concurrency).await
    {
        match result {
            Ok(profile) => edge_profiles[idx] = Some(profile),
            Err(e) => {
                warn!(label = %ontology.edge_types[idx].label, "Failed to profile edge type: {e}")
            }
        }
    }

    let edge_profiles: Vec<EdgeProfile> = edge_profiles.into_iter().flatten().collect();
    let total_elapsed = started.elapsed();

    info!(
        node_profiled = node_profiles.len(),
        edge_profiled = edge_profiles.len(),
        node_phase_ms = node_elapsed.as_millis() as u64,
        edge_phase_ms = (total_elapsed - node_elapsed).as_millis() as u64,
        total_ms = total_elapsed.as_millis() as u64,
        "Graph profiling completed"
    );

    Ok(DataProfile {
        node_profiles,
        edge_profiles,
    })
}

/// Drive a `FuturesUnordered` with bounded concurrency.
///
/// Polls futures only when `in_flight < max_concurrency`, otherwise waits
/// for an existing future to complete before starting the next one.
async fn next_bounded<T>(
    futures: &mut FuturesUnordered<Pin<Box<dyn Future<Output = T> + Send + '_>>>,
    in_flight: &mut usize,
    max_concurrency: usize,
) -> Option<T> {
    // If we have capacity and there are pending futures, they're already in
    // FuturesUnordered — just track count.
    if *in_flight < max_concurrency && !futures.is_empty() {
        *in_flight = futures.len().min(max_concurrency);
    }
    let result = futures.next().await?;
    *in_flight = in_flight.saturating_sub(1);
    Some(result)
}

fn property_names(props: &[ox_core::ontology_ir::PropertyDef]) -> Vec<String> {
    props.iter().map(|p| p.name.clone()).collect()
}

// ---------------------------------------------------------------------------
// Single-query-per-label profiling
// ---------------------------------------------------------------------------

async fn profile_node(
    runtime: &dyn GraphRuntime,
    label: &str,
    properties: &[String],
    config: &ProfileConfig,
) -> OxResult<NodeProfile> {
    let escaped_label = escape_cypher_identifier(label);
    let match_clause = format!("(n:{escaped_label})");
    let (count, property_stats) =
        profile_entity(runtime, &match_clause, "n", properties, config).await?;

    Ok(NodeProfile {
        label: label.to_string(),
        count,
        property_stats,
    })
}

async fn profile_edge(
    runtime: &dyn GraphRuntime,
    label: &str,
    source_type: &str,
    target_type: &str,
    properties: &[String],
    config: &ProfileConfig,
) -> OxResult<EdgeProfile> {
    let escaped_label = escape_cypher_identifier(label);
    let escaped_source = escape_cypher_identifier(source_type);
    let escaped_target = escape_cypher_identifier(target_type);
    let match_clause = format!("(:{escaped_source})-[r:{escaped_label}]->(:{escaped_target})");

    // Count + avg out-degree in a single query
    let meta_query = format!(
        "MATCH (s:{escaped_source})-[:{escaped_label}]->(:{escaped_target}) \
         WITH s, count(*) AS deg \
         RETURN count(deg) AS cnt, avg(deg) AS avg_deg"
    );
    let meta_result = timed_query(runtime, &meta_query, config.query_timeout).await?;
    let count = extract_u64(&meta_result, "cnt").unwrap_or(0);
    let avg_out_degree = extract_f64(&meta_result, "avg_deg");

    // Profile properties
    let (_, property_stats) =
        profile_entity(runtime, &match_clause, "r", properties, config).await?;

    Ok(EdgeProfile {
        label: label.to_string(),
        source_type: source_type.to_string(),
        target_type: target_type.to_string(),
        count,
        avg_out_degree,
        property_stats,
    })
}

/// Profile all properties of a node or edge type in a single aggregation query.
///
/// Returns `(total_count, Vec<PropertyStats>)`.
///
/// `match_clause` is the pattern without MATCH keyword, e.g. `(n:Product)`.
/// `var` is the bound variable, e.g. `"n"` or `"r"`.
async fn profile_entity(
    runtime: &dyn GraphRuntime,
    match_clause: &str,
    var: &str,
    properties: &[String],
    config: &ProfileConfig,
) -> OxResult<(u64, Vec<PropertyStats>)> {
    if properties.is_empty() {
        // Just get count
        let count_query = format!("MATCH {match_clause} RETURN count({var}) AS total");
        let result = timed_query(runtime, &count_query, config.query_timeout).await?;
        let total = extract_u64(&result, "total").unwrap_or(0);
        return Ok((total, Vec::new()));
    }

    // Build a single aggregation query that profiles all properties at once:
    //   MATCH (n:Label)
    //   RETURN count(n) AS total,
    //          count(n.`prop`) AS `p0_count`, count(DISTINCT n.`prop`) AS `p0_distinct`,
    //          min(toString(n.`prop`)) AS `p0_min`, max(toString(n.`prop`)) AS `p0_max`,
    //          ...
    // Index-based aliases avoid Cypher syntax errors from property names with spaces.
    let mut return_parts = vec![format!("count({var}) AS total")];
    for (i, prop) in properties.iter().enumerate() {
        let escaped = escape_cypher_identifier(prop);
        return_parts.push(format!("count({var}.{escaped}) AS `p{i}_count`"));
        return_parts.push(format!(
            "count(DISTINCT {var}.{escaped}) AS `p{i}_distinct`"
        ));
        return_parts.push(format!("min(toString({var}.{escaped})) AS `p{i}_min`"));
        return_parts.push(format!("max(toString({var}.{escaped})) AS `p{i}_max`"));
    }

    let agg_query = format!("MATCH {match_clause} RETURN {}", return_parts.join(", "));
    let agg_result = timed_query(runtime, &agg_query, config.query_timeout).await?;
    let total = extract_u64(&agg_result, "total").unwrap_or(0);

    // Build PropertyStats from the aggregation result, then fetch sample
    // values only for low-cardinality properties.
    let max_sample = config.max_sample_size;
    let mut stats_list = Vec::with_capacity(properties.len());

    // Collect which properties need sample values: (property_index, name, distinct_count)
    let mut need_samples: Vec<(usize, &str, u64)> = Vec::new();

    for (i, prop) in properties.iter().enumerate() {
        let non_null_count = extract_u64(&agg_result, &format!("p{i}_count")).unwrap_or(0);
        let distinct_count = extract_u64(&agg_result, &format!("p{i}_distinct")).unwrap_or(0);
        let null_count = total.saturating_sub(non_null_count);
        let min_value = extract_string(&agg_result, &format!("p{i}_min"));
        let max_value = extract_string(&agg_result, &format!("p{i}_max"));

        if distinct_count > 0 {
            need_samples.push((i, prop, distinct_count));
        }

        stats_list.push(PropertyStats {
            name: prop.to_string(),
            total_count: total,
            null_count,
            distinct_count,
            sample_values: Vec::new(), // filled below
            min_value,
            max_value,
        });
    }

    // Fetch sample values for low-cardinality properties in a single query
    if !need_samples.is_empty() {
        // Build one query that collects samples for all low-cardinality props:
        //   MATCH (n:Label)
        //   RETURN
        //     [x IN collect(DISTINCT toString(n.`prop`)) WHERE x IS NOT NULL][..30] AS `p0_vals`,
        //     [x IN collect(DISTINCT toString(n.`prop2`)) WHERE x IS NOT NULL][..30] AS `p3_vals`
        let mut sample_parts = Vec::new();
        for (prop_idx, prop, _) in &need_samples {
            let escaped = escape_cypher_identifier(prop);
            sample_parts.push(format!(
                "[x IN collect(DISTINCT toString({var}.{escaped})) WHERE x IS NOT NULL][..{max_sample}] AS `p{prop_idx}_vals`"
            ));
        }

        let sample_query = format!("MATCH {match_clause} RETURN {}", sample_parts.join(", "));

        match timed_query(runtime, &sample_query, config.query_timeout).await {
            Ok(sample_result) => {
                // Create a lookup for quick property -> stats index
                let prop_to_idx: HashMap<&str, usize> = properties
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (p.as_str(), i))
                    .collect();

                for (prop_idx, prop, _) in &need_samples {
                    let col_name = format!("p{prop_idx}_vals");
                    let values = extract_string_list(&sample_result, &col_name);
                    if let Some(&idx) = prop_to_idx.get(prop) {
                        stats_list[idx].sample_values = values;
                    }
                }
            }
            Err(e) => {
                warn!("Failed to collect sample values: {e}");
            }
        }
    }

    Ok((total, stats_list))
}

// ---------------------------------------------------------------------------
// Timeout wrapper
// ---------------------------------------------------------------------------

/// Execute a query with a timeout. Returns `OxError::Runtime` on timeout.
async fn timed_query(
    runtime: &dyn GraphRuntime,
    cypher: &str,
    deadline: Duration,
) -> OxResult<ox_core::query_ir::QueryResult> {
    let empty_params: HashMap<String, PropertyValue> = HashMap::new();
    timeout(deadline, runtime.execute_query(cypher, &empty_params))
        .await
        .map_err(|_| OxError::Runtime {
            message: format!("Profile query timed out after {}s", deadline.as_secs()),
        })?
}

// ---------------------------------------------------------------------------
// Helpers — extract typed values from QueryResult
// ---------------------------------------------------------------------------

fn col_index(result: &ox_core::query_ir::QueryResult, name: &str) -> Option<usize> {
    result.columns.iter().position(|c| c == name)
}

fn extract_u64(result: &ox_core::query_ir::QueryResult, col: &str) -> Option<u64> {
    let idx = col_index(result, col)?;
    let row = result.rows.first()?;
    let val = row.get(idx)?;
    match val {
        ox_core::types::PropertyValue::Int(n) => Some(*n as u64),
        ox_core::types::PropertyValue::Float(f) => Some(*f as u64),
        _ => None,
    }
}

fn extract_f64(result: &ox_core::query_ir::QueryResult, col: &str) -> Option<f64> {
    let idx = col_index(result, col)?;
    let row = result.rows.first()?;
    let val = row.get(idx)?;
    match val {
        ox_core::types::PropertyValue::Float(f) => Some(*f),
        ox_core::types::PropertyValue::Int(n) => Some(*n as f64),
        _ => None,
    }
}

fn extract_string(result: &ox_core::query_ir::QueryResult, col: &str) -> Option<String> {
    let idx = col_index(result, col)?;
    let row = result.rows.first()?;
    let val = row.get(idx)?;
    match val {
        ox_core::types::PropertyValue::String(s) => Some(s.clone()),
        ox_core::types::PropertyValue::Null => None,
        other => Some(format!("{other:?}")),
    }
}

fn extract_string_list(result: &ox_core::query_ir::QueryResult, col: &str) -> Vec<String> {
    let Some(idx) = col_index(result, col) else {
        return Vec::new();
    };
    let Some(row) = result.rows.first() else {
        return Vec::new();
    };
    let Some(val) = row.get(idx) else {
        return Vec::new();
    };
    match val {
        ox_core::types::PropertyValue::List(items) => items
            .iter()
            .filter_map(|v| match v {
                ox_core::types::PropertyValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        ox_core::types::PropertyValue::String(s) => vec![s.clone()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_config_default() {
        let config = ProfileConfig::default();
        assert_eq!(config.query_timeout, Duration::from_secs(10));
        assert_eq!(config.max_sample_size, MAX_DISTINCT_VALUES);
        assert_eq!(config.max_sample_size, 30);
        assert_eq!(config.concurrency, 8);
    }

    #[test]
    fn test_profile_config_for_large_ontology() {
        // Below threshold — should use defaults
        let small = ProfileConfig::for_ontology_size(50);
        assert_eq!(small.concurrency, 8);
        assert_eq!(small.max_sample_size, 30);

        // At threshold boundary — should switch to large config
        let at_boundary = ProfileConfig::for_ontology_size(100);
        assert_eq!(at_boundary.concurrency, 4);
        assert_eq!(at_boundary.max_sample_size, 10);

        // Well above threshold
        let large = ProfileConfig::for_ontology_size(500);
        assert_eq!(large.concurrency, 4);
        assert_eq!(large.max_sample_size, 10);
        assert_eq!(large.query_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_profile_config_threshold_boundary() {
        // 99 should use default (small ontology)
        let just_below = ProfileConfig::for_ontology_size(99);
        assert_eq!(just_below.concurrency, 8);

        // 100 should use large ontology config
        let at_threshold = ProfileConfig::for_ontology_size(100);
        assert_eq!(at_threshold.concurrency, 4);
    }
}
