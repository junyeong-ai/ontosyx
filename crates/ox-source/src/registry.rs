use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ox_core::error::OxResult;

use crate::DataSourceAdapter;

/// Input provided to an introspector factory.
///
/// Factories receive this enum and extract the fields they need.
/// PostgreSQL uses `connection_string` + `schema_name`;
/// CSV/JSON use `data`.
#[derive(Debug, Clone)]
pub struct SourceInput {
    /// Raw source data (CSV content, JSON content, etc.)
    pub data: Option<String>,
    /// Database connection string
    pub connection_string: Option<String>,
    /// Database schema name (e.g., "public")
    pub schema_name: Option<String>,
}

/// Future returned by an introspector factory.
type AdapterFuture =
    Pin<Box<dyn Future<Output = OxResult<Arc<dyn DataSourceAdapter>>> + Send>>;

/// Async factory function that creates a `DataSourceAdapter` from source input.
type AdapterFactory = Arc<dyn Fn(SourceInput) -> AdapterFuture + Send + Sync>;

/// Registry mapping source type identifiers to introspector factories.
///
/// Provides a pluggable way to add new data source types without modifying
/// the dispatch logic. Built-in types (postgresql, csv, json) are registered
/// via `with_defaults()`.
pub struct AdapterRegistry {
    factories: HashMap<String, AdapterFactory>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register an async factory for a source type.
    ///
    /// The factory receives a `SourceInput` and returns a boxed introspector.
    /// If the source type was already registered, the old factory is replaced.
    pub fn register<F, Fut>(&mut self, source_type: &str, factory: F)
    where
        F: Fn(SourceInput) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = OxResult<Arc<dyn DataSourceAdapter>>> + Send + 'static,
    {
        let factory: AdapterFactory =
            Arc::new(move |input: SourceInput| Box::pin(factory(input)));

        self.factories.insert(source_type.to_string(), factory);
    }

    /// Create an introspector for the given source type.
    ///
    /// Returns `None` if no factory is registered for the source type.
    pub async fn create(
        &self,
        source_type: &str,
        input: SourceInput,
    ) -> Option<OxResult<Arc<dyn DataSourceAdapter>>> {
        let factory = self.factories.get(source_type)?;
        Some(factory(input).await)
    }

    /// Returns true if a factory is registered for the given source type.
    pub fn supports(&self, source_type: &str) -> bool {
        self.factories.contains_key(source_type)
    }

    /// List all registered source type identifiers.
    pub fn registered_types(&self) -> Vec<&str> {
        self.factories.keys().map(|s| s.as_str()).collect()
    }

    /// Build a registry with all built-in source types pre-registered.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // PostgreSQL: async connection pool setup
        registry.register("postgresql", |input| async move {
            let conn = input.connection_string.as_deref().ok_or_else(|| {
                ox_core::error::OxError::Validation {
                    field: "connection_string".to_string(),
                    message: "PostgreSQL source requires a connection_string".to_string(),
                }
            })?;
            let schema = input.schema_name.as_deref().unwrap_or("public");
            let introspector = crate::postgres::PostgresAdapter::connect(conn, schema).await?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        // MySQL: async connection pool setup
        registry.register("mysql", |input| async move {
            let conn = input.connection_string.as_deref().ok_or_else(|| {
                ox_core::error::OxError::Validation {
                    field: "connection_string".to_string(),
                    message: "MySQL source requires a connection_string".to_string(),
                }
            })?;
            let schema = input.schema_name.as_deref().ok_or_else(|| {
                ox_core::error::OxError::Validation {
                    field: "schema_name".to_string(),
                    message: "MySQL source requires a schema (database) name".to_string(),
                }
            })?;
            let introspector = crate::mysql::MysqlAdapter::connect(conn, schema).await?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        // MongoDB: async client setup with document sampling
        registry.register("mongodb", |input| async move {
            let conn = input.connection_string.as_deref().ok_or_else(|| {
                ox_core::error::OxError::Validation {
                    field: "connection_string".to_string(),
                    message: "MongoDB source requires a connection_string".to_string(),
                }
            })?;
            let database = input.schema_name.as_deref().ok_or_else(|| {
                ox_core::error::OxError::Validation {
                    field: "schema_name".to_string(),
                    message: "MongoDB source requires a database name".to_string(),
                }
            })?;
            let introspector = crate::mongodb::MongoAdapter::connect(conn, database).await?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        // CSV: synchronous analysis wrapped in async
        registry.register("csv", |input| async move {
            let data =
                input
                    .data
                    .as_deref()
                    .ok_or_else(|| ox_core::error::OxError::Validation {
                        field: "data".to_string(),
                        message: "CSV source requires data".to_string(),
                    })?;
            let introspector = crate::sample::CsvAdapter::new(data)?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        // Snowflake: stub implementation (REST SQL API integration pending)
        registry.register("snowflake", |input| async move {
            let conn = input.connection_string.as_deref().ok_or_else(|| {
                ox_core::error::OxError::Validation {
                    field: "connection_string".to_string(),
                    message: "Snowflake source requires a connection_string \
                              (format: snowflake://{account}/{database}/{schema}\
                              ?user={user}&password={password}&warehouse={warehouse})"
                        .to_string(),
                }
            })?;
            let introspector =
                crate::snowflake::SnowflakeAdapter::from_connection_string(conn)?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        // BigQuery: stub implementation (gcp-bigquery-client integration pending)
        registry.register("bigquery", |input| async move {
            let conn = input.connection_string.as_deref().ok_or_else(|| {
                ox_core::error::OxError::Validation {
                    field: "connection_string".to_string(),
                    message: "BigQuery source requires a connection_string \
                              (format: bigquery://PROJECT_ID/DATASET)"
                        .to_string(),
                }
            })?;
            let introspector = crate::bigquery::BigQueryAdapter::connect(conn).await?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        // DuckDB: in-process file analysis (Parquet, CSV, JSON)
        #[cfg(feature = "duckdb")]
        registry.register("duckdb", |input| async move {
            let path = input
                .data
                .as_deref()
                .or(input.connection_string.as_deref())
                .ok_or_else(|| ox_core::error::OxError::Validation {
                    field: "file_path".to_string(),
                    message: "DuckDB source requires a file path \
                              (provide as 'data' or 'connection_string')"
                        .to_string(),
                })?;
            let introspector = crate::duckdb_source::DuckDbAdapter::from_file(path)?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        // JSON: synchronous analysis wrapped in async
        registry.register("json", |input| async move {
            let data =
                input
                    .data
                    .as_deref()
                    .ok_or_else(|| ox_core::error::OxError::Validation {
                        field: "data".to_string(),
                        message: "JSON source requires data".to_string(),
                    })?;
            let introspector = crate::sample::JsonAdapter::new(data)?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });

        registry
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_with_defaults_registers_builtin_types() {
        let registry = AdapterRegistry::with_defaults();
        assert!(registry.supports("postgresql"));
        assert!(registry.supports("mysql"));
        assert!(registry.supports("mongodb"));
        assert!(registry.supports("csv"));
        assert!(registry.supports("json"));
        assert!(registry.supports("snowflake"));
        assert!(registry.supports("bigquery"));
        #[cfg(feature = "duckdb")]
        assert!(registry.supports("duckdb"));
        assert!(!registry.supports("text"));
    }

    #[test]
    fn registry_registered_types() {
        let registry = AdapterRegistry::with_defaults();
        let mut types = registry.registered_types();
        types.sort();
        #[cfg(feature = "duckdb")]
        assert_eq!(
            types,
            vec![
                "bigquery",
                "csv",
                "duckdb",
                "json",
                "mongodb",
                "mysql",
                "postgresql",
                "snowflake"
            ]
        );
        #[cfg(not(feature = "duckdb"))]
        assert_eq!(
            types,
            vec![
                "bigquery",
                "csv",
                "json",
                "mongodb",
                "mysql",
                "postgresql",
                "snowflake"
            ]
        );
    }

    #[tokio::test]
    async fn create_returns_none_for_unknown_type() {
        let registry = AdapterRegistry::with_defaults();
        let result = registry
            .create(
                "unknown",
                SourceInput {
                    data: None,
                    connection_string: None,
                    schema_name: None,
                },
            )
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn create_csv_introspector() {
        let registry = AdapterRegistry::with_defaults();
        let result = registry
            .create(
                "csv",
                SourceInput {
                    data: Some("id,name\n1,Alice\n2,Bob\n".to_string()),
                    connection_string: None,
                    schema_name: None,
                },
            )
            .await;
        let adapter = result.unwrap().unwrap();
        assert_eq!(adapter.source_type(), "csv");

        let tables = adapter.list_tables().await.unwrap();
        assert_eq!(tables.len(), 1);
        let table = adapter.describe_table(&tables[0]).await.unwrap();
        assert_eq!(table.columns.len(), 2);
    }

    #[tokio::test]
    async fn create_json_introspector() {
        let registry = AdapterRegistry::with_defaults();
        let result = registry
            .create(
                "json",
                SourceInput {
                    data: Some(r#"[{"id":1,"name":"Alice"}]"#.to_string()),
                    connection_string: None,
                    schema_name: None,
                },
            )
            .await;
        let adapter = result.unwrap().unwrap();
        assert_eq!(adapter.source_type(), "json");

        let tables = adapter.list_tables().await.unwrap();
        assert_eq!(tables.len(), 1);
    }

    #[tokio::test]
    async fn create_csv_missing_data_returns_error() {
        let registry = AdapterRegistry::with_defaults();
        let result = registry
            .create(
                "csv",
                SourceInput {
                    data: None,
                    connection_string: None,
                    schema_name: None,
                },
            )
            .await;
        assert!(result.unwrap().is_err());
    }

    #[tokio::test]
    async fn custom_factory_registration() {
        let mut registry = AdapterRegistry::new();
        registry.register("csv", |input| async move {
            let data = input.data.as_deref().unwrap_or("");
            let introspector = crate::sample::CsvAdapter::new(data)?;
            Ok(Arc::new(introspector) as Arc<dyn DataSourceAdapter>)
        });
        assert!(registry.supports("csv"));
        assert!(!registry.supports("json"));
    }
}
