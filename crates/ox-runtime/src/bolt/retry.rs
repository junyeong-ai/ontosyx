//! Exponential-backoff retry shared by every Bolt-driver backend.

use std::time::Duration;

use tokio::time::sleep;
use tracing::warn;

use crate::TransienceDetector;

#[derive(Clone, Copy)]
pub(crate) struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
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

/// Execute an async operation with exponential backoff on transient failures.
///
/// `backend_label` only appears in the warning log, so the same helper can
/// serve Neo4j ("Neo4j") and Memgraph ("Memgraph") without duplicating
/// retry logic.
pub(crate) async fn with_retry<F, Fut, T>(
    config: &RetryConfig,
    detector: &dyn TransienceDetector,
    backend_label: &str,
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
                    backend = backend_label,
                    attempt = attempt + 1,
                    max = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %msg,
                    "Transient Bolt error, retrying"
                );
                sleep(delay).await;
                attempt += 1;
            }
        }
    }
}
