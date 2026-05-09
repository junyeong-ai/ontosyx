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

use std::sync::Arc;

use arrow::array::{ArrayBuilder, ArrayRef, RecordBatch};
use arrow::datatypes::Schema;
use async_trait::async_trait;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef, TableSummary,
};
use ox_ontology::source_analysis::ENUM_CARDINALITY_THRESHOLD;

use crate::DataSourceAdapter;
use crate::normalize::describe_to_arrow_schema;
use crate::text_scan::{append_text_cell, make_builder};

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

pub struct PostgresAdapter {
    pool: PgPool,
    schema_name: String,
}

impl PostgresAdapter {
    /// Connect with the default [`crate::AdapterConfig`]. Shorthand for
    /// [`Self::connect_with_config`] when the caller is happy with the
    /// historical `10 connections / 10s acquire timeout` envelope.
    pub async fn connect(url: &str, schema_name: &str) -> OxResult<Self> {
        Self::connect_with_config(url, schema_name, crate::AdapterConfig::default()).await
    }

    /// Connect with operator-supplied pool bounds and timeouts. Used by
    /// workloads that need a higher pool ceiling or a tighter timeout
    /// envelope than the defaults.
    pub async fn connect_with_config(
        url: &str,
        schema_name: &str,
        config: crate::AdapterConfig,
    ) -> OxResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.pool_max_connections)
            .acquire_timeout(config.acquire_timeout)
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

    fn supports_scan(&self) -> bool {
        true
    }

    fn capabilities(&self) -> crate::AdapterCapabilities {
        crate::AdapterCapabilities {
            supports_scan: true,
            predicate_pushdown: crate::PredicatePushdown::Full,
            limit_pushdown: true,
            aggregate_pushdown: true,
            partition_aware: true,
            computed_link_dialect: Some(crate::SqlDialect::PostgreSql),
        }
    }

    /// Lift recognised PostgreSQL error classes (permission denied, …)
    /// out of the raw libpq message and into a stable
    /// [`WarningClass`]. The default fallback keeps the raw message
    /// as `detail` for operator drilldown.
    fn classify_warning(
        &self,
        level: ox_ontology::source_analysis::WarningLevel,
        phase: ox_ontology::source_analysis::AnalysisPhase,
        default_class: ox_ontology::source_analysis::WarningClass,
        scope: ox_ontology::source_analysis::WarningScope,
        error: &OxError,
    ) -> ox_ontology::source_analysis::AnalysisWarning {
        use ox_ontology::source_analysis::{AnalysisWarning, WarningClass};
        let raw = error.to_string();
        let class = if raw.contains("permission denied") || raw.contains("must be owner") {
            WarningClass::PostgresPermissionDenied
        } else {
            default_class
        };
        AnalysisWarning::new(level, phase, class, scope).with_detail(raw)
    }

    async fn list_tables(&self) -> OxResult<Vec<String>> {
        // information_schema.tables surfaces every queryable relation
        // in the schema — base tables, views, and foreign tables.
        // Materialised views live in `pg_matviews` and are unioned in
        // explicitly so callers see the full data surface (rather than
        // only the managed-table subset historically returned here).
        sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
              WHERE table_schema = $1 \
              UNION ALL \
             SELECT matviewname AS table_name FROM pg_matviews \
              WHERE schemaname = $1 \
              ORDER BY table_name",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to list tables: {e}"),
        })
    }

    async fn list_tables_with_summary(&self) -> OxResult<Vec<TableSummary>> {
        // Cheap fast path: information_schema for column count joined
        // against pg_stat_user_tables (live row estimate + last analyze
        // timestamp — only populated for base tables; views surface
        // with NULL counters which the caller renders as "unknown").
        // Materialised views are unioned in from `pg_matviews` so the
        // listing covers every queryable relation in the schema.
        #[derive(sqlx::FromRow)]
        struct Row {
            table_name: String,
            column_count: Option<i64>,
            n_live_tup: Option<i64>,
            last_modified: Option<chrono::DateTime<chrono::Utc>>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT t.table_name AS table_name, \
                    (SELECT COUNT(*)::bigint \
                       FROM information_schema.columns c \
                      WHERE c.table_schema = t.table_schema \
                        AND c.table_name = t.table_name) AS column_count, \
                    s.n_live_tup AS n_live_tup, \
                    GREATEST(s.last_autoanalyze, s.last_analyze) AS last_modified \
               FROM information_schema.tables t \
               LEFT JOIN pg_stat_user_tables s \
                 ON s.schemaname = t.table_schema \
                AND s.relname = t.table_name \
              WHERE t.table_schema = $1 \
              UNION ALL \
             SELECT m.matviewname AS table_name, \
                    (SELECT COUNT(*)::bigint \
                       FROM information_schema.columns c \
                      WHERE c.table_schema = m.schemaname \
                        AND c.table_name = m.matviewname) AS column_count, \
                    s.n_live_tup AS n_live_tup, \
                    GREATEST(s.last_autoanalyze, s.last_analyze) AS last_modified \
               FROM pg_matviews m \
               LEFT JOIN pg_stat_user_tables s \
                 ON s.schemaname = m.schemaname \
                AND s.relname = m.matviewname \
              WHERE m.schemaname = $1 \
              ORDER BY table_name",
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
                estimated_row_count: r.n_live_tup.filter(|&n| n >= 0).map(|n| n as u64),
                column_count: r.column_count.unwrap_or(0).max(0) as u32,
                last_modified: r.last_modified,
            })
            .collect())
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
            pii_redacted: ox_core::source_schema::classify_pii_suspect_by_name(&column.name),
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

    async fn scan(
        &self,
        table: &str,
        projection: Option<Vec<usize>>,
        limit: Option<usize>,
    ) -> OxResult<RecordBatch> {
        // Reuse `describe_table` for the column schema — projection
        // is a list of column indices into *that* schema, mirroring
        // the CSV adapter's contract.
        let table_def = self.describe_table(table).await?;
        let arrow_schema = describe_to_arrow_schema("postgresql", &table_def);

        let selected_indices: Vec<usize> =
            projection.unwrap_or_else(|| (0..table_def.columns.len()).collect());

        let projected_schema = if selected_indices.len() == table_def.columns.len() {
            arrow_schema.clone()
        } else {
            arrow_schema
                .project(&selected_indices)
                .map_err(|e| OxError::Runtime {
                    message: format!("postgres scan: projection error: {e}"),
                })?
        };

        // Emit each selected column casted to TEXT. Postgres's text
        // output is deterministic per type, so the downstream parse
        // path reuses the same logic the CSV adapter already has.
        // This keeps the sqlx type-handling surface minimal — every
        // cell is `Option<String>` regardless of the source column's
        // actual Postgres type.
        let projected_columns: Vec<&SourceColumnDef> = selected_indices
            .iter()
            .map(|i| &table_def.columns[*i])
            .collect();
        let select_list = projected_columns
            .iter()
            .map(|c| format!("{}::text AS {}", quote_ident(&c.name), quote_ident(&c.name)))
            .collect::<Vec<_>>()
            .join(", ");
        let limit_clause = limit.map(|n| format!(" LIMIT {n}")).unwrap_or_default();
        let sql = format!(
            "SELECT {select_list} FROM {schema}.{table}{limit}",
            schema = quote_ident(&self.schema_name),
            table = quote_ident(table),
            limit = limit_clause,
        );

        let rows: Vec<PgRow> =
            sqlx::query(&sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("Failed to scan table `{table}`: {e}"),
                })?;

        build_record_batch_from_pg_rows(&rows, &projected_columns, &projected_schema)
    }
}

/// Convert TEXT-cast Postgres rows into an Arrow `RecordBatch` that
/// matches `arrow_schema`. Cell parsing lives in
/// [`crate::text_scan::append_text_cell`]; this function just drives
/// the per-row sqlx extraction into the shared helper.
fn build_record_batch_from_pg_rows(
    rows: &[PgRow],
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
            // `::text` casts render NULL as a real null; sqlx's
            // `try_get::<Option<String>, _>` surfaces that. The
            // column index matches `idx` because the SELECT list
            // was emitted in the same order as `columns`.
            let raw: Option<String> = row.try_get(idx).map_err(|e| OxError::Runtime {
                message: format!(
                    "postgres scan: failed to read column `{name}` at row \
                         offset {idx}: {e}",
                    name = col.name
                ),
            })?;
            append_text_cell(
                "postgres",
                builders[idx].as_mut(),
                arrow_schema.field(idx).data_type(),
                raw.as_deref(),
            )?;
        }
    }

    let arrays: Vec<ArrayRef> = builders.into_iter().map(|mut b| b.finish()).collect();
    RecordBatch::try_new(Arc::new(arrow_schema.clone()), arrays).map_err(|e| OxError::Runtime {
        message: format!("postgres scan: RecordBatch::try_new failed: {e}"),
    })
}
