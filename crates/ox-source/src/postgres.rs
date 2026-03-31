use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::HashSet;
use std::time::Duration;
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_analysis::{
    AnalysisPhase, AnalysisWarning, AnalysisWarningKind, ENUM_CARDINALITY_THRESHOLD,
    LARGE_SCHEMA_GATE_THRESHOLD, WarningLevel,
};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef,
    TableProfile,
};

use crate::{
    AnalysisResult, DEFAULT_INTROSPECTION_CONCURRENCY, DataSourceIntrospector,
    introspect_tables_concurrent,
};

/// Returns true for PostgreSQL types that typically contain large structured/binary data.
/// These columns produce meaningless multi-KB sample values that waste LLM tokens.
/// NOTE: `text` and `varchar` are NOT included — they are commonly used for short values
/// (names, addresses, statuses). The `left(..., 200)` in sample collection handles
/// unexpectedly long text values.
fn is_blob_type(data_type: &str) -> bool {
    let dt = data_type.to_lowercase();
    matches!(dt.as_str(), "json" | "jsonb" | "xml" | "bytea" | "oid")
}

type ProfileResult = (
    usize,
    String,
    Result<(TableProfile, Vec<AnalysisWarning>), OxError>,
);

/// Baseline enum threshold: columns at or below this are definite enums.
const DEFINITE_ENUM_CARDINALITY: i64 = 30;
/// Introspection pool: connection count doubles as concurrent task limit
const POOL_MAX_CONNECTIONS: u32 = 10;
const POOL_ACQUIRE_TIMEOUT_SECS: u64 = 10;

pub struct PostgresIntrospector {
    pool: PgPool,
    schema_name: String,
}

impl PostgresIntrospector {
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

    pub async fn introspect_schema_resilient(
        &self,
    ) -> OxResult<(SourceSchema, Vec<AnalysisWarning>)> {
        // 1. Discover tables
        let table_names: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(&self.schema_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to list tables: {e}"),
        })?;

        // Schemas above this threshold take significant introspection time
        if table_names.len() >= LARGE_SCHEMA_GATE_THRESHOLD {
            warn!(
                table_count = table_names.len(),
                threshold = LARGE_SCHEMA_GATE_THRESHOLD,
                "Large schema detected. Stats collection may take significant time on the source DB.",
            );
        }

        let mut warnings = Vec::new();

        // Introspect tables concurrently with bounded parallelism.
        // Pool size (POOL_MAX_CONNECTIONS) provides an additional DB-level bound.
        let pool = self.pool.clone();
        let schema_name = self.schema_name.clone();
        let introspection_results = introspect_tables_concurrent(
            &table_names,
            DEFAULT_INTROSPECTION_CONCURRENCY,
            |table_name| {
                let pool = pool.clone();
                let schema_name = schema_name.clone();
                async move { introspect_table_pg(&pool, &schema_name, &table_name).await }
            },
        )
        .await;

        let mut tables = Vec::with_capacity(table_names.len());
        for (table_name, result) in introspection_results {
            match result {
                Ok(table) => tables.push(table),
                Err(err) => {
                    warn!(table = %table_name, error = %err, "Skipping inaccessible table during schema introspection");
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::SchemaIntrospection,
                        kind: AnalysisWarningKind::TableSkipped,
                        location: table_name,
                        message: err.to_string(),
                    });
                }
            }
        }

        if tables.is_empty() {
            return Err(OxError::Runtime {
                message: format!(
                    "No accessible tables were introspected in schema {}",
                    self.schema_name
                ),
            });
        }

        // 2. Discover foreign keys
        let mut foreign_keys = match self.discover_foreign_keys().await {
            Ok(foreign_keys) => foreign_keys,
            Err(err) => {
                warn!(schema = %self.schema_name, error = %err, "Foreign key discovery failed; continuing without declared foreign keys");
                warnings.push(AnalysisWarning {
                    level: WarningLevel::Warning,
                    phase: AnalysisPhase::SchemaIntrospection,
                    kind: AnalysisWarningKind::ForeignKeysUnavailable,
                    location: self.schema_name.clone(),
                    message: err.to_string(),
                });
                Vec::new()
            }
        };

        let accessible_tables: HashSet<&str> =
            tables.iter().map(|table| table.name.as_str()).collect();
        foreign_keys.retain(|fk| {
            accessible_tables.contains(fk.from_table.as_str())
                && accessible_tables.contains(fk.to_table.as_str())
        });

        Ok((
            SourceSchema {
                source_type: "postgresql".to_string(),
                tables,
                foreign_keys,
            },
            warnings,
        ))
    }

    pub async fn collect_stats_resilient(
        &self,
        schema: &SourceSchema,
    ) -> OxResult<(SourceProfile, Vec<AnalysisWarning>)> {
        // Profile tables concurrently, preserving original order via enumerate index.
        // Pool size (POOL_MAX_CONNECTIONS) bounds actual DB concurrency.
        let mut futures = FuturesUnordered::new();
        for (idx, table) in schema.tables.iter().enumerate() {
            futures.push(async move {
                let result = self
                    .profile_table_resilient(&table.name, &table.columns)
                    .await;
                (idx, table.name.clone(), result)
            });
        }

        let mut indexed_results: Vec<ProfileResult> = Vec::with_capacity(schema.tables.len());
        while let Some(item) = futures.next().await {
            indexed_results.push(item);
        }

        // Sort by original index to restore deterministic order
        indexed_results.sort_by_key(|(idx, _, _)| *idx);

        let mut table_profiles = Vec::new();
        let mut warnings = Vec::new();

        for (_, table_name, result) in indexed_results {
            match result {
                Ok((table_profile, mut table_warnings)) => {
                    table_profiles.push(table_profile);
                    warnings.append(&mut table_warnings);
                }
                Err(err) => {
                    warn!(table = %table_name, error = %err, "Skipping table during data profiling");
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::DataProfiling,
                        kind: AnalysisWarningKind::TableSkipped,
                        location: table_name,
                        message: err.to_string(),
                    });
                }
            }
        }

        if table_profiles.is_empty() && !schema.tables.is_empty() {
            return Err(OxError::Runtime {
                message: format!(
                    "Failed to collect stats for every accessible table in schema {}",
                    self.schema_name
                ),
            });
        }

        Ok((SourceProfile { table_profiles }, warnings))
    }
}

/// Quote a PostgreSQL identifier (table/column name) safely.
/// Wraps in double quotes and escapes any embedded double quotes by doubling them.
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

#[async_trait]
impl DataSourceIntrospector for PostgresIntrospector {
    fn source_type(&self) -> &str {
        "postgresql"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        let (schema, warnings) = self.introspect_schema_resilient().await?;
        for warning in warnings {
            warn!(
                phase = ?warning.phase,
                kind = ?warning.kind,
                location = %warning.location,
                message = %warning.message,
                "PostgreSQL schema introspection completed with warnings"
            );
        }
        Ok(schema)
    }

    async fn collect_stats(&self, schema: &SourceSchema) -> OxResult<SourceProfile> {
        let (profile, warnings) = self.collect_stats_resilient(schema).await?;
        for warning in warnings {
            warn!(
                phase = ?warning.phase,
                kind = ?warning.kind,
                location = %warning.location,
                message = %warning.message,
                "PostgreSQL data profiling completed with warnings"
            );
        }
        Ok(profile)
    }

    /// Full analysis with resilient per-table/column warnings.
    ///
    /// Unlike the default `introspect_schema` + `collect_stats` path (which
    /// logs warnings and discards them), this captures all warnings in the
    /// returned `AnalysisResult` so callers can surface them to users.
    async fn analyze(&self) -> OxResult<AnalysisResult> {
        let (schema, mut warnings) = self.introspect_schema_resilient().await?;
        let (profile, profile_warnings) = self.collect_stats_resilient(&schema).await?;
        warnings.extend(profile_warnings);
        Ok(AnalysisResult {
            schema,
            profile,
            warnings,
        })
    }
}

/// Introspect a single PostgreSQL table: columns + primary key.
/// Free function to enable capture in concurrent closures without borrowing `self`.
async fn introspect_table_pg(
    pool: &PgPool,
    schema_name: &str,
    table_name: &str,
) -> OxResult<SourceTableDef> {
    // Columns
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT column_name, data_type, is_nullable \
         FROM information_schema.columns \
         WHERE table_schema = $1 AND table_name = $2 \
         ORDER BY ordinal_position",
    )
    .bind(schema_name)
    .bind(table_name)
    .fetch_all(pool)
    .await
    .map_err(|e| OxError::Runtime {
        message: format!("Failed to get columns for {table_name}: {e}"),
    })?;

    let columns: Vec<SourceColumnDef> = rows
        .into_iter()
        .map(|(name, data_type, is_nullable)| SourceColumnDef {
            name,
            data_type,
            nullable: is_nullable == "YES",
        })
        .collect();

    // Primary key
    let pk_columns: Vec<String> = sqlx::query_scalar(
        "SELECT kcu.column_name \
         FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON tc.constraint_name = kcu.constraint_name \
           AND tc.table_schema = kcu.table_schema \
         WHERE tc.table_schema = $1 AND tc.table_name = $2 \
           AND tc.constraint_type = 'PRIMARY KEY' \
         ORDER BY kcu.ordinal_position",
    )
    .bind(schema_name)
    .bind(table_name)
    .fetch_all(pool)
    .await
    .map_err(|e| OxError::Runtime {
        message: format!("Failed to get primary key for {table_name}: {e}"),
    })?;

    Ok(SourceTableDef {
        name: table_name.to_string(),
        columns,
        primary_key: pk_columns,
    })
}

impl PostgresIntrospector {
    async fn discover_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
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
                |(_constraint_name, from_table, from_column, to_table, to_column)| ForeignKeyDef {
                    from_table,
                    from_column,
                    to_table,
                    to_column,
                    inferred: false,
                },
            )
            .collect())
    }

    async fn profile_table_resilient(
        &self,
        table_name: &str,
        columns: &[SourceColumnDef],
    ) -> OxResult<(TableProfile, Vec<AnalysisWarning>)> {
        // Approximate row count from pg_stat_user_tables (MVCC-safe, no full scan).
        // Falls back to count(*) only when stats are unavailable (e.g., freshly created table).
        let row_count = self.approximate_row_count(table_name).await?;

        let mut column_stats = Vec::new();
        let mut warnings = Vec::new();
        for col in columns {
            match self
                .profile_column(table_name, &col.name, &col.data_type)
                .await
            {
                Ok((stats, sample_warning)) => {
                    column_stats.push(stats);
                    if let Some(sample_warning) = sample_warning {
                        warnings.push(sample_warning);
                    }
                }
                Err(err) => {
                    warn!(
                        table = %table_name,
                        column = %col.name,
                        error = %err,
                        "Skipping column during data profiling"
                    );
                    warnings.push(AnalysisWarning {
                        level: WarningLevel::Warning,
                        phase: AnalysisPhase::DataProfiling,
                        kind: AnalysisWarningKind::ColumnSkipped,
                        location: format!("{table_name}.{}", col.name),
                        message: err.to_string(),
                    });
                }
            }
        }

        Ok((
            TableProfile {
                table_name: table_name.to_string(),
                row_count,
                column_stats,
            },
            warnings,
        ))
    }

    /// Get approximate row count using PostgreSQL statistics catalog.
    /// `pg_stat_user_tables.n_live_tup` is maintained by autovacuum and avoids
    /// the full table scan that `count(*)` requires under MVCC.
    /// Falls back to `count(*)` when stats report 0 (table never vacuumed/analyzed).
    async fn approximate_row_count(&self, table_name: &str) -> OxResult<u64> {
        let approx: Option<i64> = sqlx::query_scalar(
            "SELECT n_live_tup::bigint FROM pg_stat_user_tables \
             WHERE schemaname = $1 AND relname = $2",
        )
        .bind(&self.schema_name)
        .bind(table_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to get approximate count for {table_name}: {e}"),
        })?
        .flatten();

        match approx {
            Some(n) if n > 0 => Ok(n as u64),
            _ => {
                // Fallback: exact count for tables without stats (freshly created, never analyzed)
                let count_query = format!("SELECT count(*) FROM {}", quote_ident(table_name));
                let exact: i64 = sqlx::query_scalar(&count_query)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| OxError::Runtime {
                        message: format!("Failed to count rows in {table_name}: {e}"),
                    })?;
                Ok(exact.max(0) as u64)
            }
        }
    }

    async fn profile_column(
        &self,
        table_name: &str,
        column_name: &str,
        data_type: &str,
    ) -> OxResult<(ColumnStats, Option<AnalysisWarning>)> {
        let qt = quote_ident(table_name);
        let qc = quote_ident(column_name);

        // Skip DISTINCT/min/max for large-object types — they cause full table scans
        // and produce meaningless multi-KB sample values.
        let is_blob = is_blob_type(data_type);

        let (null_count, distinct_count, min_value, max_value) = if is_blob {
            // For blob types: only count nulls (cheap), skip distinct/min/max
            let q = format!("SELECT count(*) FILTER (WHERE {qc} IS NULL) AS null_count FROM {qt}");
            let null_count: (i64,) =
                sqlx::query_as(&q)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| OxError::Runtime {
                        message: format!("Failed to profile {table_name}.{column_name}: {e}"),
                    })?;
            (null_count.0, 0i64, None, None)
        } else {
            let stats_query = format!(
                "SELECT \
                    count(*) FILTER (WHERE {qc} IS NULL) AS null_count, \
                    count(DISTINCT {qc}) AS distinct_count, \
                    min({qc}::text) AS min_val, \
                    max({qc}::text) AS max_val \
                 FROM {qt}",
            );
            let row: (i64, i64, Option<String>, Option<String>) = sqlx::query_as(&stats_query)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("Failed to profile {table_name}.{column_name}: {e}"),
                })?;
            row
        };

        // Collect sample values based on cardinality and data type:
        // - Blobs (json/xml/bytea): skip entirely
        // - Low cardinality (≤ 30): collect ALL values (enum/categorical)
        // - Medium cardinality (31-100) + short values: collect ALL (likely code/category)
        // - High cardinality (> 100): skip (free-text, IDs)
        let extended_threshold = ENUM_CARDINALITY_THRESHOLD as i64;
        let sample_limit = if is_blob || distinct_count <= 0 {
            0
        } else if distinct_count <= DEFINITE_ENUM_CARDINALITY {
            // Definite enum: collect all
            distinct_count
        } else if distinct_count <= extended_threshold {
            // Possible enum: check average value length to decide
            // Short strings (codes, statuses) → collect all; long strings → skip
            let avg_len_query = format!(
                "SELECT coalesce(avg(length(val)), 0)::int FROM (\
                 SELECT {qc}::text AS val FROM {qt} WHERE {qc} IS NOT NULL LIMIT 1000\
                 ) sub"
            );
            let avg_len: (i32,) = sqlx::query_as(&avg_len_query)
                .fetch_one(&self.pool)
                .await
                .unwrap_or((999,));
            if avg_len.0 <= 50 {
                distinct_count // Short values → likely enum codes, collect all
            } else {
                0 // Long values → not enum
            }
        } else {
            0 // High cardinality → free-text
        };

        let (sample_values, sample_warning) = if sample_limit <= 0 {
            (Vec::new(), None)
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
                Ok(values) => (values, None),
                Err(err) => {
                    let message = format!(
                        "Failed to collect sample values for {table_name}.{column_name}: {err}"
                    );
                    warn!(
                        table = %table_name,
                        column = %column_name,
                        error = %err,
                        "Omitting sample values for profiled column"
                    );
                    (
                        Vec::new(),
                        Some(AnalysisWarning {
                            level: WarningLevel::Info,
                            phase: AnalysisPhase::DataProfiling,
                            kind: AnalysisWarningKind::SampleValuesOmitted,
                            location: format!("{table_name}.{column_name}"),
                            message,
                        }),
                    )
                }
            }
        };

        Ok((
            ColumnStats {
                column_name: column_name.to_string(),
                null_count: null_count.max(0) as u64,
                distinct_count: distinct_count.max(0) as u64,
                sample_values,
                min_value,
                max_value,
            },
            sample_warning,
        ))
    }
}
