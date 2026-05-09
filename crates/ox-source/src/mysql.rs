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

use std::sync::Arc;

use arrow::array::{ArrayBuilder, ArrayRef, RecordBatch};
use arrow::datatypes::Schema;
use async_trait::async_trait;
use sqlx::Row;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions, MySqlRow};
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef, TableSummary,
};

use crate::DataSourceAdapter;
use crate::normalize::describe_to_arrow_schema;
use crate::text_scan::{append_text_cell, make_builder};

/// Maximum distinct values to collect per column as samples.
const MAX_DISTINCT_VALUES: i64 = 30;

pub struct MysqlAdapter {
    pool: MySqlPool,
    /// MySQL "schema" is the database name.
    schema_name: String,
}

impl MysqlAdapter {
    /// Connect with the default [`AdapterConfig`].
    pub async fn connect(url: &str, schema_name: &str) -> OxResult<Self> {
        Self::connect_with_config(url, schema_name, crate::AdapterConfig::default()).await
    }

    /// Connect with operator-supplied pool bounds and timeouts.
    pub async fn connect_with_config(
        url: &str,
        schema_name: &str,
        config: crate::AdapterConfig,
    ) -> OxResult<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(config.pool_max_connections)
            .acquire_timeout(config.acquire_timeout)
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

    fn supports_scan(&self) -> bool {
        true
    }

    fn capabilities(&self) -> crate::AdapterCapabilities {
        crate::AdapterCapabilities {
            supports_scan: true,
            predicate_pushdown: crate::PredicatePushdown::Full,
            limit_pushdown: true,
            aggregate_pushdown: true,
            partition_aware: false,
            computed_link_dialect: Some(crate::SqlDialect::MySql),
        }
    }

    async fn list_tables(&self) -> OxResult<Vec<String>> {
        // information_schema.TABLES surfaces base tables and views in
        // the schema. SYSTEM VIEW rows belong to the server's own
        // catalogue tables and are filtered out — they are not part
        // of the user's data surface.
        sqlx::query_scalar(
            "SELECT TABLE_NAME FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_TYPE <> 'SYSTEM VIEW' \
             ORDER BY TABLE_NAME",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to list tables: {e}"),
        })
    }

    async fn list_tables_with_summary(&self) -> OxResult<Vec<TableSummary>> {
        // Single round-trip: TABLE_ROWS is InnoDB's autovacuum-equivalent
        // estimate (NULL for views), UPDATE_TIME is set when the table
        // file's last write landed (NULL for tables never written
        // through the server, and for views). Column count comes from
        // a correlated subquery against information_schema.COLUMNS.
        // SYSTEM VIEW rows are excluded — they are catalogue plumbing,
        // not part of the user's data surface.
        #[derive(sqlx::FromRow)]
        struct Row {
            table_name: String,
            column_count: Option<i64>,
            table_rows: Option<u64>,
            update_time: Option<chrono::DateTime<chrono::Utc>>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT t.TABLE_NAME AS table_name, \
                    (SELECT COUNT(*) \
                       FROM information_schema.COLUMNS c \
                      WHERE c.TABLE_SCHEMA = t.TABLE_SCHEMA \
                        AND c.TABLE_NAME = t.TABLE_NAME) AS column_count, \
                    t.TABLE_ROWS AS table_rows, \
                    t.UPDATE_TIME AS update_time \
             FROM information_schema.TABLES t \
             WHERE t.TABLE_SCHEMA = ? \
               AND t.TABLE_TYPE <> 'SYSTEM VIEW' \
             ORDER BY t.TABLE_NAME",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to list table summaries: {e}"),
        })?;

        Ok(rows
            .into_iter()
            .map(|r| TableSummary {
                name: r.table_name,
                estimated_row_count: r.table_rows,
                column_count: r.column_count.unwrap_or(0).max(0) as u32,
                last_modified: r.update_time,
            })
            .collect())
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
            pii_redacted: ox_core::source_schema::classify_pii_suspect_by_name(&column.name),
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

    async fn scan(
        &self,
        table: &str,
        projection: Option<Vec<usize>>,
        limit: Option<usize>,
    ) -> OxResult<RecordBatch> {
        // Mirrors the Postgres adapter's shape: push every column
        // through a `CAST … AS CHAR` so the Rust layer only handles
        // `Option<String>`, then reuse a per-Arrow-type builder
        // factory that parses the text back into typed cells. MySQL
        // `CAST(... AS CHAR)` is deterministic per type, same as
        // Postgres's `::text`.
        let table_def = self.describe_table(table).await?;
        let arrow_schema = describe_to_arrow_schema("mysql", &table_def);

        let selected_indices: Vec<usize> =
            projection.unwrap_or_else(|| (0..table_def.columns.len()).collect());

        let projected_schema = if selected_indices.len() == table_def.columns.len() {
            arrow_schema.clone()
        } else {
            arrow_schema
                .project(&selected_indices)
                .map_err(|e| OxError::Runtime {
                    message: format!("mysql scan: projection error: {e}"),
                })?
        };

        let projected_columns: Vec<&SourceColumnDef> = selected_indices
            .iter()
            .map(|i| &table_def.columns[*i])
            .collect();
        let select_list = projected_columns
            .iter()
            .map(|c| {
                format!(
                    "CAST({ident} AS CHAR) AS {ident}",
                    ident = quote_ident(&c.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let limit_clause = limit.map(|n| format!(" LIMIT {n}")).unwrap_or_default();
        let sql = format!(
            "SELECT {select_list} FROM {schema}.{table}{limit}",
            schema = quote_ident(&self.schema_name),
            table = quote_ident(table),
            limit = limit_clause,
        );

        let rows: Vec<MySqlRow> =
            sqlx::query(&sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("Failed to scan table `{table}`: {e}"),
                })?;

        build_record_batch_from_mysql_rows(&rows, &projected_columns, &projected_schema)
    }
}

/// Assemble an Arrow `RecordBatch` from a sequence of
/// `CAST … AS CHAR`-returned MySQL rows. Cell parsing lives in
/// [`crate::text_scan::append_text_cell`]; this function drives the
/// per-row sqlx extraction into the shared helper. MySQL's CAST-to-
/// CHAR renders bools as `0`/`1` (tinyint(1) is the underlying
/// representation); both pairs are accepted by `append_text_cell`.
fn build_record_batch_from_mysql_rows(
    rows: &[MySqlRow],
    columns: &[&SourceColumnDef],
    arrow_schema: &Schema,
) -> OxResult<RecordBatch> {
    let mut builders: Vec<Box<dyn ArrayBuilder>> = arrow_schema
        .fields()
        .iter()
        .map(|f| make_builder(f.data_type()))
        .collect();

    for row in rows {
        for (idx, col) in columns.iter().enumerate() {
            let raw: Option<String> = row.try_get(idx).map_err(|e| OxError::Runtime {
                message: format!(
                    "mysql scan: failed to read column `{name}` at row \
                         offset {idx}: {e}",
                    name = col.name
                ),
            })?;
            append_text_cell(
                "mysql",
                builders[idx].as_mut(),
                arrow_schema.field(idx).data_type(),
                raw.as_deref(),
            )?;
        }
    }

    let arrays: Vec<ArrayRef> = builders.into_iter().map(|mut b| b.finish()).collect();
    RecordBatch::try_new(Arc::new(arrow_schema.clone()), arrays).map_err(|e| OxError::Runtime {
        message: format!("mysql scan: RecordBatch::try_new failed: {e}"),
    })
}
