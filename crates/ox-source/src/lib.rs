pub mod analyzer;
pub mod fetcher;
pub mod mongodb;
pub mod mysql;
pub mod postgres;
pub mod postgres_fetcher;
pub mod registry;
pub mod repo;
pub mod sample;

use async_trait::async_trait;
use ox_core::error::OxResult;
use ox_core::source_analysis::AnalysisWarning;
use ox_core::source_schema::{SourceProfile, SourceSchema, SourceTableDef};

/// Default concurrency limit for table introspection.
pub const DEFAULT_INTROSPECTION_CONCURRENCY: usize = 8;

/// Execute table introspection concurrently with bounded parallelism.
/// Returns results in the original table order.
pub async fn introspect_tables_concurrent<F, Fut>(
    table_names: &[String],
    concurrency: usize,
    introspect_fn: F,
) -> Vec<(String, OxResult<SourceTableDef>)>
where
    F: Fn(String) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = OxResult<SourceTableDef>> + Send,
{
    use futures::stream::{self, StreamExt};

    let introspect_fn = &introspect_fn;
    let mut results: Vec<_> = stream::iter(table_names.iter().cloned().enumerate())
        .map(|(idx, name)| {
            let name_clone = name.clone();
            async move {
                let result = introspect_fn(name_clone).await;
                (idx, name, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    results.sort_by_key(|(idx, _, _)| *idx);
    results
        .into_iter()
        .map(|(_, name, result)| (name, result))
        .collect()
}

/// Result of a full source analysis: schema, profile, and any warnings
/// encountered during introspection or profiling.
///
/// The default `analyze()` implementation returns an empty `warnings` vec.
/// Backends that perform resilient analysis (e.g., PostgreSQL skipping
/// inaccessible tables) override `analyze()` to populate warnings.
pub struct AnalysisResult {
    pub schema: SourceSchema,
    pub profile: SourceProfile,
    pub warnings: Vec<AnalysisWarning>,
}

/// Introspect an external data source to discover its schema and collect statistics.
/// Used to provide structured input to the ontology design LLM.
#[async_trait]
pub trait DataSourceIntrospector: Send + Sync {
    /// Discover tables, columns, types, constraints, foreign keys
    async fn introspect_schema(&self) -> OxResult<SourceSchema>;

    /// Collect data statistics (row counts, distinct values, ranges)
    async fn collect_stats(&self, schema: &SourceSchema) -> OxResult<SourceProfile>;

    /// Source type identifier (e.g., "postgresql", "mysql")
    fn source_type(&self) -> &str;

    /// Run full analysis including per-table/column warnings.
    ///
    /// Default implementation delegates to `introspect_schema` + `collect_stats`
    /// with an empty warnings list. Backends with resilient analysis (e.g.,
    /// PostgreSQL) override this to capture partial-analysis warnings.
    async fn analyze(&self) -> OxResult<AnalysisResult> {
        let schema = self.introspect_schema().await?;
        let profile = self.collect_stats(&schema).await?;
        Ok(AnalysisResult {
            schema,
            profile,
            warnings: Vec::new(),
        })
    }
}
