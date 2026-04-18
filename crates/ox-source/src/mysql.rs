//! MySQL data source adapter.
//!
//! Implements the atomic primitives in [`crate::DataSourceAdapter`]:
//! `list_tables`, `describe_table`, `count_rows`, `sample_column`, and
//! `list_foreign_keys`. Cross-cutting concerns (retry / concurrency /
//! warning aggregation / caching) live in
//! [`crate::IntrospectionKernel`].
//!
//! MySQL quirks addressed here:
//!
//! - **Type fidelity**: `COLUMN_TYPE` preserves `tinyint(1)` vs plain
//!   `tinyint`, which matters downstream for inferring booleans.
//!   `DATA_TYPE` loses that distinction.
//! - **Approximate counts**: `information_schema.TABLES.TABLE_ROWS` is
//!   the InnoDB estimate (no full scan). We fall back to exact
//!   `COUNT(*)` only when the estimate is 0 — an empty or freshly
//!   created table.
//! - **Safe identifier quoting**: backtick-escape with doubling.

use async_trait::async_trait;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use std::time::Duration;
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef};

use crate::DataSourceAdapter;

/// Maximum distinct values to collect per column as samples.
const MAX_DISTINCT_VALUES: i64 = 30;
/// Introspection pool size; doubles as an implicit concurrency ceiling.
const POOL_MAX_CONNECTIONS: u32 = 10;
const POOL_ACQUIRE_TIMEOUT_SECS: u64 = 10;

pub struct MysqlAdapter {
    pool: MySqlPool,
    /// MySQL "schema" is the database name.
    schema_name: String,
}

impl MysqlAdapter {
    pub async fn connect(url: &str, schema_name: &str) -> OxResult<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(POOL_MAX_CONNECTIONS)
            .acquire_timeout(Duration::from_secs(POOL_ACQUIRE_TIMEOUT_SECS))
            .connect(url)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to connect to MySQL source: {e}"),
            })?;
        info!(schema = schema_name, "Connected to MySQL source");
        Ok(Self {
            pool,
            schema_name: schema_name.to_string(),
        })
    }
}

/// Quote a MySQL identifier (table/column name) safely by wrapping in
/// backticks and doubling any embedded backticks.
fn quote_ident(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}

#[async_trait]
impl DataSourceAdapter for MysqlAdapter {
    fn source_type(&self) -> &str {
        "mysql"
    }

    async fn list_tables(&self) -> OxResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT TABLE_NAME FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
             ORDER BY TABLE_NAME",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to list tables: {e}"),
        })
    }

    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
        // COLUMN_TYPE preserves width/precision ("tinyint(1)") that DATA_TYPE loses.
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE \
             FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
             ORDER BY ORDINAL_POSITION",
        )
        .bind(&self.schema_name)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to get columns for {table}: {e}"),
        })?;

        let columns: Vec<SourceColumnDef> = rows
            .into_iter()
            .map(|(name, data_type, is_nullable)| SourceColumnDef {
                name,
                data_type,
                nullable: is_nullable == "YES",
            })
            .collect();

        let primary_key: Vec<String> = sqlx::query_scalar(
            "SELECT kcu.COLUMN_NAME \
             FROM information_schema.TABLE_CONSTRAINTS tc \
             JOIN information_schema.KEY_COLUMN_USAGE kcu \
               ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
               AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA \
               AND tc.TABLE_NAME = kcu.TABLE_NAME \
             WHERE tc.TABLE_SCHEMA = ? AND tc.TABLE_NAME = ? \
               AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' \
             ORDER BY kcu.ORDINAL_POSITION",
        )
        .bind(&self.schema_name)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to get primary key for {table}: {e}"),
        })?;

        Ok(SourceTableDef {
            name: table.to_string(),
            columns,
            primary_key,
        })
    }

    async fn count_rows(&self, table: &str) -> OxResult<u64> {
        // Fast path: InnoDB estimate.
        let approx: Option<u64> = sqlx::query_scalar(
            "SELECT TABLE_ROWS FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
        )
        .bind(&self.schema_name)
        .bind(table)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to get approximate count for {table}: {e}"),
        })?
        .flatten();

        if let Some(n) = approx
            && n > 0
        {
            return Ok(n);
        }

        // Fallback: exact count for tables without InnoDB stats yet.
        let count_query = format!("SELECT COUNT(*) FROM {}", quote_ident(table));
        let exact: i64 = sqlx::query_scalar(&count_query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to count rows in {table}: {e}"),
            })?;
        Ok(exact.max(0) as u64)
    }

    async fn sample_column(&self, table: &str, column: &SourceColumnDef) -> OxResult<ColumnStats> {
        let qt = quote_ident(table);
        let qc = quote_ident(&column.name);

        // Combined aggregation — null + distinct + min + max in one round trip.
        let stats_query = format!(
            "SELECT \
                SUM(CASE WHEN {qc} IS NULL THEN 1 ELSE 0 END) AS null_count, \
                COUNT(DISTINCT {qc}) AS distinct_count, \
                MIN(CAST({qc} AS CHAR)) AS min_val, \
                MAX(CAST({qc} AS CHAR)) AS max_val \
             FROM {qt}",
        );
        let (null_count, distinct_count, min_value, max_value): (
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(&stats_query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to profile {table}.{}: {e}", column.name),
            })?;

        // Only collect per-value samples when cardinality is manageable.
        let sample_values = if distinct_count > 0 && distinct_count <= MAX_DISTINCT_VALUES {
            let sample_query = format!(
                "SELECT DISTINCT CAST({qc} AS CHAR) AS val \
                 FROM {qt} \
                 WHERE {qc} IS NOT NULL \
                 ORDER BY val \
                 LIMIT {MAX_DISTINCT_VALUES}",
            );
            match sqlx::query_scalar::<_, String>(&sample_query)
                .fetch_all(&self.pool)
                .await
            {
                Ok(values) => values,
                Err(err) => {
                    warn!(
                        table = %table,
                        column = %column.name,
                        error = %err,
                        "Omitting sample values for profiled column"
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok(ColumnStats {
            column_name: column.name.clone(),
            null_count: null_count.max(0) as u64,
            distinct_count: distinct_count.max(0) as u64,
            sample_values,
            min_value,
            max_value,
        })
    }

    async fn list_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
        // MySQL uses REFERENCED_TABLE_NAME / REFERENCED_COLUMN_NAME in
        // information_schema.KEY_COLUMN_USAGE (no constraint_column_usage table).
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT \
                kcu.CONSTRAINT_NAME, \
                kcu.TABLE_NAME AS from_table, \
                kcu.COLUMN_NAME AS from_column, \
                kcu.REFERENCED_TABLE_NAME AS to_table, \
                kcu.REFERENCED_COLUMN_NAME AS to_column \
             FROM information_schema.KEY_COLUMN_USAGE kcu \
             JOIN information_schema.TABLE_CONSTRAINTS tc \
               ON kcu.CONSTRAINT_NAME = tc.CONSTRAINT_NAME \
               AND kcu.TABLE_SCHEMA = tc.TABLE_SCHEMA \
               AND kcu.TABLE_NAME = tc.TABLE_NAME \
             WHERE kcu.TABLE_SCHEMA = ? \
               AND tc.CONSTRAINT_TYPE = 'FOREIGN KEY' \
               AND kcu.REFERENCED_TABLE_NAME IS NOT NULL \
             ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to discover foreign keys: {e}"),
        })?;

        Ok(rows
            .into_iter()
            .map(
                |(_, from_table, from_column, to_table, to_column)| ForeignKeyDef {
                    from_table,
                    from_column,
                    to_table,
                    to_column,
                    inferred: false,
                },
            )
            .collect())
    }
}
