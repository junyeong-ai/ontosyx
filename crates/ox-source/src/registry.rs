use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ox_core::error::OxResult;

use crate::DataSourceIntrospector;

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
type IntrospectorFuture =
    Pin<Box<dyn Future<Output = OxResult<Box<dyn DataSourceIntrospector>>> + Send>>;

/// Async factory function that creates a `DataSourceIntrospector` from source input.
type IntrospectorFactory = Arc<dyn Fn(SourceInput) -> IntrospectorFuture + Send + Sync>;

/// Registry mapping source type identifiers to introspector factories.
///
/// Provides a pluggable way to add new data source types without modifying
/// the dispatch logic. Built-in types (postgresql, csv, json) are registered
/// via `with_defaults()`.
pub struct IntrospectorRegistry {
    factories: HashMap<String, IntrospectorFactory>,
}

impl IntrospectorRegistry {
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
        Fut: Future<Output = OxResult<Box<dyn DataSourceIntrospector>>> + Send + 'static,
    {
        let factory: IntrospectorFactory =
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
    ) -> Option<OxResult<Box<dyn DataSourceIntrospector>>> {
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
            let introspector = crate::postgres::PostgresIntrospector::connect(conn, schema).await?;
            Ok(Box::new(introspector) as Box<dyn DataSourceIntrospector>)
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
            let introspector = crate::mysql::MysqlIntrospector::connect(conn, schema).await?;
            Ok(Box::new(introspector) as Box<dyn DataSourceIntrospector>)
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
            let introspector =
                crate::mongodb::MongoIntrospector::connect(conn, database).await?;
            Ok(Box::new(introspector) as Box<dyn DataSourceIntrospector>)
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
            let introspector = crate::sample::CsvIntrospector::new(data)?;
            Ok(Box::new(introspector) as Box<dyn DataSourceIntrospector>)
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
            let introspector = crate::sample::JsonIntrospector::new(data)?;
            Ok(Box::new(introspector) as Box<dyn DataSourceIntrospector>)
        });

        registry
    }
}

impl Default for IntrospectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_with_defaults_registers_builtin_types() {
        let registry = IntrospectorRegistry::with_defaults();
        assert!(registry.supports("postgresql"));
        assert!(registry.supports("mysql"));
        assert!(registry.supports("mongodb"));
        assert!(registry.supports("csv"));
        assert!(registry.supports("json"));
        assert!(!registry.supports("text"));
    }

    #[test]
    fn registry_registered_types() {
        let registry = IntrospectorRegistry::with_defaults();
        let mut types = registry.registered_types();
        types.sort();
        assert_eq!(types, vec!["csv", "json", "mongodb", "mysql", "postgresql"]);
    }

    #[tokio::test]
    async fn create_returns_none_for_unknown_type() {
        let registry = IntrospectorRegistry::with_defaults();
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
        let registry = IntrospectorRegistry::with_defaults();
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
        let introspector = result.unwrap().unwrap();
        assert_eq!(introspector.source_type(), "csv");

        let schema = introspector.introspect_schema().await.unwrap();
        assert_eq!(schema.tables.len(), 1);
        assert_eq!(schema.tables[0].columns.len(), 2);
    }

    #[tokio::test]
    async fn create_json_introspector() {
        let registry = IntrospectorRegistry::with_defaults();
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
        let introspector = result.unwrap().unwrap();
        assert_eq!(introspector.source_type(), "json");

        let schema = introspector.introspect_schema().await.unwrap();
        assert_eq!(schema.tables.len(), 1);
    }

    #[tokio::test]
    async fn create_csv_missing_data_returns_error() {
        let registry = IntrospectorRegistry::with_defaults();
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
        let mut registry = IntrospectorRegistry::new();
        registry.register("csv", |input| async move {
            let data = input.data.as_deref().unwrap_or("");
            let introspector = crate::sample::CsvIntrospector::new(data)?;
            Ok(Box::new(introspector) as Box<dyn DataSourceIntrospector>)
        });
        assert!(registry.supports("csv"));
        assert!(!registry.supports("json"));
    }
}
