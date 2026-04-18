//! PostgreSQL data source adapter.
//!
//! Implements the atomic primitives in [`crate::DataSourceAdapter`]:
//! `list_tables`, `describe_table`, `count_rows`, `sample_column`, and
//! `list_foreign_keys`. The [`crate::IntrospectionKernel`] orchestrates
//! them into schema discovery + profiling + analysis, so this file owns
//! only per-query SQL and result mapping.
//!
//! Connection management is via `sqlx::PgPool` with a bounded pool; the
//! kernel's `concurrency` parameter governs how many primitive calls
//! fan out in parallel, while the pool size caps actual DB concurrency.
//!
//! Approximate row counts use `pg_stat_user_tables.n_live_tup` to avoid
//! the full-table scan that `count(*)` would force under MVCC. A table
//! with no autovacuum stats yet (freshly created, never analyzed) falls
//! back to an exact `count(*)`.

use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_analysis::ENUM_CARDINALITY_THRESHOLD;
use ox_core::source_schema::{ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef};

use crate::DataSourceAdapter;

/// Returns true for PostgreSQL types that typically contain large
/// structured/binary data. These columns produce meaningless multi-KB
/// sample values that waste LLM tokens and force full scans on
/// `DISTINCT` / `min` / `max` aggregations.
///
/// `text` and `varchar` are NOT blob types — they are commonly used for
/// short values (names, addresses, statuses). The `left(..., 200)` in
/// `sample_column` handles unexpectedly long text values.
fn is_blob_type(data_type: &str) -> bool {
    let dt = data_type.to_lowercase();
    matches!(dt.as_str(), "json" | "jsonb" | "xml" | "bytea" | "oid")
}

/// Baseline enum threshold: columns at or below this are definite enums
/// (collect every distinct value as a sample).
const DEFINITE_ENUM_CARDINALITY: i64 = 30;
/// Connection pool bounds.
const POOL_MAX_CONNECTIONS: u32 = 10;
const POOL_ACQUIRE_TIMEOUT_SECS: u64 = 10;

pub struct PostgresAdapter {
    pool: PgPool,
    schema_name: String,
}

impl PostgresAdapter {
    pub async fn connect(url: &str, schema_name: &str) -> OxResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(POOL_MAX_CONNECTIONS)
            .acquire_timeout(Duration::from_secs(POOL_ACQUIRE_TIMEOUT_SECS))
            .connect(url)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to connect to PostgreSQL source: {e}"),
            })?;
        info!(schema = schema_name, "Connected to PostgreSQL source");
        Ok(Self {
            pool,
            schema_name: schema_name.to_string(),
        })
    }

    /// Access the underlying pool — shared with `PostgresFetcher` so
    /// schema introspection and row-batch fetching reuse the same pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn schema_name(&self) -> &str {
        &self.schema_name
    }
}

/// Quote a PostgreSQL identifier (table/column name) safely.
/// Wraps in double quotes and escapes any embedded double quotes by doubling them.
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

#[async_trait]
impl DataSourceAdapter for PostgresAdapter {
    fn source_type(&self) -> &str {
        "postgresql"
    }

    async fn list_tables(&self) -> OxResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to list tables: {e}"),
        })
    }

    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
        // Columns in declaration order.
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT column_name, data_type, is_nullable \
             FROM information_schema.columns \
             WHERE table_schema = $1 AND table_name = $2 \
             ORDER BY ordinal_position",
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

        // Primary key columns in position order.
        let primary_key: Vec<String> = sqlx::query_scalar(
            "SELECT kcu.column_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
               ON tc.constraint_name = kcu.constraint_name \
               AND tc.table_schema = kcu.table_schema \
             WHERE tc.table_schema = $1 AND tc.table_name = $2 \
               AND tc.constraint_type = 'PRIMARY KEY' \
             ORDER BY kcu.ordinal_position",
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
        // Fast path: pg_stat_user_tables is MVCC-safe and autovacuum-maintained,
        // so we avoid the full-table scan that count(*) would require.
        let approx: Option<i64> = sqlx::query_scalar(
            "SELECT n_live_tup::bigint FROM pg_stat_user_tables \
             WHERE schemaname = $1 AND relname = $2",
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
            return Ok(n as u64);
        }

        // Fallback: exact count for tables without stats (freshly created,
        // never analyzed — or truly empty).
        let count_query = format!("SELECT count(*) FROM {}", quote_ident(table));
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

        // Skip DISTINCT / min / max for large-object types — they cause full
        // table scans and produce meaningless multi-KB sample values.
        let is_blob = is_blob_type(&column.data_type);

        let (null_count, distinct_count, min_value, max_value) = if is_blob {
            let q = format!("SELECT count(*) FILTER (WHERE {qc} IS NULL) AS null_count FROM {qt}");
            let row: (i64,) = sqlx::query_as(&q)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("Failed to profile {table}.{}: {e}", column.name),
                })?;
            (row.0, 0i64, None, None)
        } else {
            let stats_query = format!(
                "SELECT \
                    count(*) FILTER (WHERE {qc} IS NULL) AS null_count, \
                    count(DISTINCT {qc}) AS distinct_count, \
                    min({qc}::text) AS min_val, \
                    max({qc}::text) AS max_val \
                 FROM {qt}",
            );
            sqlx::query_as::<_, (i64, i64, Option<String>, Option<String>)>(&stats_query)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("Failed to profile {table}.{}: {e}", column.name),
                })?
        };

        // Decide the per-column sample budget by cardinality and value length:
        // - Blob types: never sample.
        // - ≤ 30 distinct: definite enum, collect every value.
        // - 31..=ENUM_CARDINALITY_THRESHOLD: possible enum, collect only if
        //   the average value length is short (codes / statuses).
        // - Above that: free-text / IDs, skip sampling entirely.
        let extended_threshold = ENUM_CARDINALITY_THRESHOLD as i64;
        let sample_limit = if is_blob || distinct_count <= 0 {
            0
        } else if distinct_count <= DEFINITE_ENUM_CARDINALITY {
            distinct_count
        } else if distinct_count <= extended_threshold {
            let avg_len_query = format!(
                "SELECT coalesce(avg(length(val)), 0)::int FROM (\
                 SELECT {qc}::text AS val FROM {qt} WHERE {qc} IS NOT NULL LIMIT 1000\
                 ) sub"
            );
            let avg_len: (i32,) = sqlx::query_as(&avg_len_query)
                .fetch_one(&self.pool)
                .await
                .unwrap_or((999,));
            if avg_len.0 <= 50 { distinct_count } else { 0 }
        } else {
            0
        };

        let sample_values = if sample_limit <= 0 {
            Vec::new()
        } else {
            let sample_query = format!(
                "SELECT DISTINCT left({qc}::text, 200) AS val \
                 FROM {qt} \
                 WHERE {qc} IS NOT NULL \
                 ORDER BY val \
                 LIMIT {sample_limit}",
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
        let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
            "SELECT \
                tc.constraint_name, \
                kcu.table_name AS from_table, \
                kcu.column_name AS from_column, \
                ccu.table_name AS to_table, \
                ccu.column_name AS to_column \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
               ON tc.constraint_name = kcu.constraint_name \
               AND tc.table_schema = kcu.table_schema \
             JOIN information_schema.constraint_column_usage ccu \
               ON tc.constraint_name = ccu.constraint_name \
               AND tc.table_schema = ccu.table_schema \
             WHERE tc.table_schema = $1 AND tc.constraint_type = 'FOREIGN KEY' \
             ORDER BY tc.constraint_name",
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
