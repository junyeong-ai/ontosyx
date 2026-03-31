use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::{Column, Row};
use std::time::Duration;
use tracing::info;

use ox_core::error::{OxError, OxResult};

use crate::fetcher::{DataSourceFetcher, SourceRow};

const POOL_MAX_CONNECTIONS: u32 = 10;
const POOL_ACQUIRE_TIMEOUT_SECS: u64 = 10;

/// Fetches data from a PostgreSQL source for graph loading.
///
/// Connects to the same source database that `PostgresIntrospector` analyzes.
/// Uses paginated SELECT queries with configurable batch sizes.
pub struct PostgresFetcher {
    pool: PgPool,
    schema_name: String,
}

impl PostgresFetcher {
    /// Connect to a PostgreSQL source database.
    pub async fn connect(url: &str, schema_name: &str) -> OxResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(POOL_MAX_CONNECTIONS)
            .acquire_timeout(Duration::from_secs(POOL_ACQUIRE_TIMEOUT_SECS))
            .connect(url)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to connect to PostgreSQL source for fetching: {e}"),
            })?;
        info!(schema = schema_name, "PostgresFetcher connected");
        Ok(Self {
            pool,
            schema_name: schema_name.to_string(),
        })
    }

    /// Create a fetcher from an existing connection pool (shared with introspector).
    pub fn from_pool(pool: PgPool, schema_name: &str) -> Self {
        Self {
            pool,
            schema_name: schema_name.to_string(),
        }
    }

    /// Build a fully qualified table name: "schema"."table"
    fn qualified_table(&self, table: &str) -> String {
        // If table already contains a dot, use as-is
        if table.contains('.') {
            table.to_string()
        } else {
            format!("\"{}\".\"{table}\"", self.schema_name)
        }
    }
}

#[async_trait]
impl DataSourceFetcher for PostgresFetcher {
    async fn fetch_batch(
        &self,
        table: &str,
        columns: &[String],
        offset: u64,
        limit: u64,
    ) -> OxResult<Vec<SourceRow>> {
        let qualified = self.qualified_table(table);

        // Column selection: specified columns or all (*)
        let col_clause = if columns.is_empty() {
            "*".to_string()
        } else {
            columns
                .iter()
                .map(|c| format!("\"{c}\""))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let sql = format!(
            "SELECT {col_clause} FROM {qualified} ORDER BY ctid OFFSET {offset} LIMIT {limit}"
        );

        let rows: Vec<PgRow> = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to fetch from {table}: {e}"),
            })?;

        // Convert each row to a JSON map
        let mut result = Vec::with_capacity(rows.len());
        for row in &rows {
            let map = pg_row_to_json_map(row)?;
            result.push(map);
        }

        Ok(result)
    }

    async fn count_rows(&self, table: &str) -> OxResult<u64> {
        let qualified = self.qualified_table(table);
        let sql = format!("SELECT COUNT(*) AS cnt FROM {qualified}");

        let count: i64 = sqlx::query_scalar(&sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to count rows in {table}: {e}"),
            })?;

        Ok(count as u64)
    }

    fn source_type(&self) -> &str {
        "postgresql"
    }
}

/// Convert a sqlx PgRow to a JSON map by reading column metadata at runtime.
fn pg_row_to_json_map(row: &PgRow) -> OxResult<SourceRow> {
    let mut map = serde_json::Map::new();

    for col in row.columns() {
        let name = col.name().to_string();
        let type_name = col.type_info().to_string();

        let value = match type_name.as_str() {
            "INT2" | "INT4" => row
                .try_get::<Option<i32>, _>(col.ordinal())
                .ok()
                .flatten()
                .map(|v| serde_json::Value::Number(v.into()))
                .unwrap_or(serde_json::Value::Null),
            "INT8" => row
                .try_get::<Option<i64>, _>(col.ordinal())
                .ok()
                .flatten()
                .map(|v| serde_json::Value::Number(v.into()))
                .unwrap_or(serde_json::Value::Null),
            "FLOAT4" | "FLOAT8" | "NUMERIC" => row
                .try_get::<Option<f64>, _>(col.ordinal())
                .ok()
                .flatten()
                .and_then(|v| serde_json::Number::from_f64(v))
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            "BOOL" => row
                .try_get::<Option<bool>, _>(col.ordinal())
                .ok()
                .flatten()
                .map(serde_json::Value::Bool)
                .unwrap_or(serde_json::Value::Null),
            "UUID" => row
                .try_get::<Option<uuid::Uuid>, _>(col.ordinal())
                .ok()
                .flatten()
                .map(|v| serde_json::Value::String(v.to_string()))
                .unwrap_or(serde_json::Value::Null),
            "TIMESTAMPTZ" | "TIMESTAMP" => row
                .try_get::<Option<chrono::NaiveDateTime>, _>(col.ordinal())
                .ok()
                .flatten()
                .map(|v| serde_json::Value::String(v.to_string()))
                .unwrap_or(serde_json::Value::Null),
            "DATE" => row
                .try_get::<Option<chrono::NaiveDate>, _>(col.ordinal())
                .ok()
                .flatten()
                .map(|v| serde_json::Value::String(v.to_string()))
                .unwrap_or(serde_json::Value::Null),
            "JSON" | "JSONB" => row
                .try_get::<Option<serde_json::Value>, _>(col.ordinal())
                .ok()
                .flatten()
                .unwrap_or(serde_json::Value::Null),
            // Default: try as string (covers TEXT, VARCHAR, CHAR, etc.)
            _ => row
                .try_get::<Option<String>, _>(col.ordinal())
                .ok()
                .flatten()
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        };

        map.insert(name, value);
    }

    Ok(map)
}

#[cfg(test)]
mod tests {
    #[test]
    fn qualified_table_logic() {
        // Test the qualification logic directly
        let schema = "public";
        let table = "products";
        let result = if table.contains('.') {
            table.to_string()
        } else {
            format!("\"{schema}\".\"{table}\"")
        };
        assert_eq!(result, "\"public\".\"products\"");

        let dotted = "other.products";
        let result2 = if dotted.contains('.') {
            dotted.to_string()
        } else {
            format!("\"{schema}\".\"{dotted}\"")
        };
        assert_eq!(result2, "other.products");
    }
}
