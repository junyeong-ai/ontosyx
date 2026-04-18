//! Adapter connection-pool and timeout configuration.
//!
//! The SQL adapters (`postgres`, `mysql`, `postgres_fetcher`) and the
//! document adapter (`mongodb`) used to hardcode the same set of magic
//! numbers — `10` connections, `10s` acquire timeout, `10s` connect
//! timeout — in four different files. That worked right up to the
//! first production operator who needed a higher pool ceiling on a
//! busy backend: two places to change, four missed.
//!
//! `AdapterConfig` centralises the knobs. Each adapter's `connect`
//! constructor takes an `AdapterConfig` (with a `Default` that matches
//! the historical constants) and applies the values to whichever driver
//! primitive it binds to. Adding a new tunable — e.g. socket keepalive
//! — means adding one field here and threading it once into each
//! adapter, instead of grepping for every magic number.

use std::time::Duration;

/// Per-adapter connection-pool and timeout configuration.
///
/// Defaults reproduce the legacy hardcoded values so callers that
/// don't opt in see no behavioural change. Override on construction
/// for workloads that need a different envelope:
///
/// ```ignore
/// let cfg = AdapterConfig {
///     pool_max_connections: 50,
///     acquire_timeout: Duration::from_secs(30),
///     ..AdapterConfig::default()
/// };
/// let adapter = PostgresAdapter::connect_with_config(url, cfg).await?;
/// ```
#[derive(Debug, Clone, Copy)]
pub struct AdapterConfig {
    /// Maximum concurrent connections the pool will open. SQL drivers
    /// call this `max_connections`; MongoDB maps onto its
    /// `MongoClientOptions::max_pool_size`.
    pub pool_max_connections: u32,
    /// Time `acquire()` waits for a free connection before giving up.
    /// SQL drivers only — MongoDB's server-selection timeout below
    /// is the nearest equivalent for that driver.
    pub acquire_timeout: Duration,
    /// Time the MongoDB driver waits when opening a new TCP connection
    /// to a replica-set member. Unused by SQL adapters.
    pub mongo_connect_timeout: Duration,
    /// Time the MongoDB driver waits for a suitable server to be
    /// selected (primary / secondary depending on read preference).
    /// Unused by SQL adapters.
    pub mongo_server_selection_timeout: Duration,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            pool_max_connections: 10,
            acquire_timeout: Duration::from_secs(10),
            mongo_connect_timeout: Duration::from_secs(10),
            mongo_server_selection_timeout: Duration::from_secs(10),
        }
    }
}
