use async_trait::async_trait;
use ox_core::error::OxResult;

/// A single row of data fetched from a source table.
/// Keys are column names, values are JSON-compatible types.
pub type SourceRow = serde_json::Map<String, serde_json::Value>;

/// Fetch actual data from an external source for graph loading.
///
/// Separate from `DataSourceIntrospector` because introspection discovers
/// schema structure (lightweight, metadata-only) while fetching retrieves
/// actual data rows (can move large volumes, different security profile).
#[async_trait]
pub trait DataSourceFetcher: Send + Sync {
    /// Fetch a batch of rows from a table with column selection and pagination.
    ///
    /// # Arguments
    /// * `table` — Fully qualified table name (e.g., "public.products")
    /// * `columns` — Column names to SELECT. Empty = all columns.
    /// * `offset` — Row offset for pagination
    /// * `limit` — Maximum rows to return
    async fn fetch_batch(
        &self,
        table: &str,
        columns: &[String],
        offset: u64,
        limit: u64,
    ) -> OxResult<Vec<SourceRow>>;

    /// Count total rows in a table (for progress reporting).
    async fn count_rows(&self, table: &str) -> OxResult<u64>;

    /// Source type identifier (e.g., "postgresql", "mysql").
    fn source_type(&self) -> &str;
}
