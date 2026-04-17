//! `FuturesUnordered`-based batch load executor shared by every Bolt
//! backend. Each backend supplies the resolved (post-isolation) Cypher
//! string + scope params via [`LoadContext`]; the executor handles
//! concurrency, retry, and per-batch error capture.

use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use neo4rs::{Graph, query};
use tracing::warn;

use ox_core::error::OxResult;
use ox_core::types::PropertyValue;

use crate::{LoadBatch, LoadError, LoadResult, TransienceDetector};

use super::helpers::bind_json_field;
use super::retry::{RetryConfig, with_retry};

/// Inputs needed to dispatch a batch of UNWIND-style write queries.
pub(crate) struct LoadContext<'a> {
    pub graph: Graph,
    pub cypher: String,
    pub scoped_params: Vec<(String, String)>,
    pub max_concurrent: usize,
    pub retry: RetryConfig,
    pub detector: Arc<dyn TransienceDetector>,
    pub backend_label: &'a str,
}

pub(crate) async fn run_batched_load(ctx: LoadContext<'_>, batch: LoadBatch) -> OxResult<LoadResult> {
    let LoadContext {
        graph,
        cypher,
        scoped_params,
        max_concurrent,
        retry,
        detector,
        backend_label,
    } = ctx;

    let cypher = Arc::new(cypher);
    let scoped_params = Arc::new(scoped_params);

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

    let spawn_one =
        |futures: &mut FuturesUnordered<BatchFuture>,
         i: usize,
         record: serde_json::Map<String, serde_json::Value>| {
            let field_pairs: Vec<(String, serde_json::Value)> = record.into_iter().collect();
            let graph = graph.clone();
            let cypher = Arc::clone(&cypher);
            let detector = Arc::clone(&detector);
            let scoped_params = Arc::clone(&scoped_params);
            let backend_label = backend_label.to_string();

            futures.push(Box::pin(async move {
                let res = with_retry(&retry, detector.as_ref(), &backend_label, || {
                    let mut q = query(&cypher);
                    for (key, val) in &field_pairs {
                        q = bind_json_field(q, &format!("row_{key}"), val);
                    }
                    for (key, val) in scoped_params.iter() {
                        q = q.param(key.as_str(), val.as_str());
                    }
                    graph.run(q)
                })
                .await;
                (i, res)
            }));
        };

    for _ in 0..max_concurrent {
        match iter.next() {
            Some((i, record)) => spawn_one(&mut futures, i, record),
            None => break,
        }
    }

    while let Some((idx, res)) = futures.next().await {
        match res {
            Ok(()) => {
                result.batches_processed += 1;
            }
            Err(e) => {
                let msg = format!("Batch {idx} failed: {e}");
                warn!(backend = backend_label, %msg);
                result.batches_failed += 1;
                result.errors.push(LoadError {
                    batch_index: idx,
                    message: msg,
                });
            }
        }

        if let Some((i, record)) = iter.next() {
            spawn_one(&mut futures, i, record);
        }
    }

    Ok(result)
}

/// Convert `pre_execute`'s scoped param map into the `Vec<(String, String)>`
/// that the load executor expects for per-batch binding. Non-string values
/// are dropped because Bolt parameter binding for loads only ever needs
/// the string-typed isolation predicates (e.g. `$_ws_id`).
pub(crate) fn isolation_params_from(
    scoped: &std::collections::HashMap<String, PropertyValue>,
) -> Vec<(String, String)> {
    scoped
        .iter()
        .filter_map(|(k, v)| match v {
            PropertyValue::String(s) => Some((k.clone(), s.clone())),
            _ => None,
        })
        .collect()
}
