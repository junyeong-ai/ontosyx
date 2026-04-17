use std::collections::HashMap;
use std::sync::Arc;

use neo4rs::{Graph, query};
use tracing::warn;

use ox_core::error::OxResult;
use ox_core::types::PropertyValue;

use crate::{LoadBatch, LoadResult, TransienceDetector};

use super::helpers::bind_json_field;
use super::retry::{RetryConfig, with_retry};
use super::runtime::Neo4jRuntime;

impl Neo4jRuntime {
    pub(super) async fn execute_load_impl(
        &self,
        cypher: &str,
        batch: LoadBatch,
    ) -> OxResult<LoadResult> {
        use futures::stream::{FuturesUnordered, StreamExt};

        // Scope the load query for workspace isolation (writes)
        let (cypher, scoped_params) = self.scope_query(cypher, &HashMap::new());
        let cypher = &cypher;

        let max_concurrent = self.load_concurrency();

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

        let retry_max_retries = self.retry_max_retries();
        let retry_initial_delay = self.retry_initial_delay();
        let retry_max_delay = self.retry_max_delay();
        let detector = self.detector_arc();

        // Capture isolation params for load batches (e.g., _ws_id)
        let isolation_params: Vec<(String, String)> = scoped_params
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
            // Bind fields as individual $row_<field> params.
            // The compiler generates `$row_<source_column>` placeholders (no UNWIND).
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

        for _ in 0..max_concurrent {
            if let Some((i, record)) = iter.next() {
                spawn_batch(
                    &mut futures,
                    i,
                    record,
                    self.graph_clone(),
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
                    self.graph_clone(),
                    cypher.to_owned(),
                    Arc::clone(&detector),
                    isolation_params.clone(),
                );
            }
        }

        Ok(result)
    }
}
