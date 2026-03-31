use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ox_core::error::OxResult;

use crate::GraphRuntime;

// ---------------------------------------------------------------------------
// GraphBackendConfig — common connection parameters for graph databases
// ---------------------------------------------------------------------------

/// Connection parameters for a graph database backend.
/// Factories receive this struct and extract the fields they need.
#[derive(Debug, Clone)]
pub struct GraphBackendConfig {
    pub uri: String,
    pub username: String,
    pub password: String,
    pub database: String,
    pub max_connections: u32,
    pub load_concurrency: Option<usize>,
    pub retry_max: Option<u32>,
    pub retry_initial_delay_ms: Option<u64>,
    pub retry_max_delay_ms: Option<u64>,
    /// Workspace isolation strategy: "property", "database", or "none"
    pub isolation_strategy: String,
}

// ---------------------------------------------------------------------------
// GraphBackend — paired compiler + runtime
// ---------------------------------------------------------------------------

/// A graph backend consisting of a compiler (IR → query language)
/// and an optional runtime (query execution).
///
/// The runtime is optional because the server can run in "compile-only" mode
/// if the graph database is unavailable.
pub struct GraphBackend {
    pub compiler: Arc<dyn ox_compiler::GraphCompiler>,
    pub runtime: Option<Arc<dyn GraphRuntime>>,
}

// ---------------------------------------------------------------------------
// GraphBackendRegistry — pluggable factory for graph backends
// ---------------------------------------------------------------------------

/// Future returned by a backend factory.
type BackendFuture = Pin<Box<dyn Future<Output = OxResult<GraphBackend>> + Send>>;

/// Async factory function that creates a `GraphBackend` from config.
type BackendFactory = Arc<dyn Fn(GraphBackendConfig) -> BackendFuture + Send + Sync>;

/// Registry mapping backend identifiers to async factories.
///
/// Follows the same pattern as `IntrospectorRegistry` in ox-source.
/// Adding a new graph DB = implement GraphCompiler + GraphRuntime + register factory.
pub struct GraphBackendRegistry {
    factories: HashMap<String, BackendFactory>,
}

impl GraphBackendRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register an async factory for a backend type.
    pub fn register<F, Fut>(&mut self, backend: &str, factory: F)
    where
        F: Fn(GraphBackendConfig) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = OxResult<GraphBackend>> + Send + 'static,
    {
        let factory: BackendFactory =
            Arc::new(move |config: GraphBackendConfig| Box::pin(factory(config)));
        self.factories.insert(backend.to_string(), factory);
    }

    /// Create a graph backend for the given backend type.
    pub async fn create(
        &self,
        backend: &str,
        config: GraphBackendConfig,
    ) -> OxResult<GraphBackend> {
        let factory = self.factories.get(backend).ok_or_else(|| {
            let supported: Vec<&str> = self.factories.keys().map(|s| s.as_str()).collect();
            ox_core::error::OxError::Validation {
                field: "backend".to_string(),
                message: format!(
                    "Unsupported graph backend: '{backend}'. Supported: {}",
                    supported.join(", ")
                ),
            }
        })?;
        factory(config).await
    }

    /// Returns true if a factory is registered for the given backend.
    pub fn supports(&self, backend: &str) -> bool {
        self.factories.contains_key(backend)
    }

    /// List all registered backend identifiers.
    pub fn registered_backends(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }

    /// Build a registry with all built-in backends pre-registered.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        registry.register("neo4j", |config| async move {
            let compiler = Arc::new(ox_compiler::cypher::CypherCompiler)
                as Arc<dyn ox_compiler::GraphCompiler>;

            let isolation_strategy = config.isolation_strategy.clone();
            let runtime = match crate::neo4j::Neo4jRuntime::connect(
                &config.uri,
                &config.username,
                &config.password,
                Some(&config.database),
                Some(config.max_connections),
                config.load_concurrency,
                config.retry_max,
                config.retry_initial_delay_ms,
                config.retry_max_delay_ms,
            )
            .await
            {
                Ok(rt) => {
                    // Apply workspace isolation strategy
                    let rt = match isolation_strategy.as_str() {
                        "property" => {
                            rt.with_isolation(Box::new(crate::isolation::PropertyStrategy))
                        }
                        "database" => {
                            rt.with_isolation(Box::new(crate::isolation::DatabaseStrategy))
                        }
                        "none" => {
                            tracing::warn!(
                                "Graph isolation DISABLED — all workspaces share graph data"
                            );
                            rt
                        }
                        other => {
                            tracing::warn!(
                                strategy = other,
                                "Unknown graph isolation strategy, defaulting to 'property'"
                            );
                            rt.with_isolation(Box::new(crate::isolation::PropertyStrategy))
                        }
                    };
                    let rt = Arc::new(rt) as Arc<dyn GraphRuntime>;
                    if rt.health_check().await {
                        tracing::info!(uri = %config.uri, "Connected to Neo4j");
                        Some(rt)
                    } else {
                        tracing::warn!(
                            uri = %config.uri,
                            "Neo4j not reachable — running without graph database"
                        );
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!("Neo4j not available: {e} — running without graph database");
                    None
                }
            };

            Ok(GraphBackend { compiler, runtime })
        });

        registry
    }
}

impl Default for GraphBackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_with_defaults_registers_neo4j() {
        let registry = GraphBackendRegistry::with_defaults();
        assert!(registry.supports("neo4j"));
        assert!(!registry.supports("neptune"));
    }

    #[test]
    fn registry_registered_backends() {
        let registry = GraphBackendRegistry::with_defaults();
        let backends = registry.registered_backends();
        assert_eq!(backends, vec!["neo4j"]);
    }
}
