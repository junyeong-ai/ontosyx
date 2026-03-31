use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use std::collections::HashSet;
use std::time::Duration;
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::source_analysis::{
    AnalysisPhase, AnalysisWarning, AnalysisWarningKind, LARGE_SCHEMA_GATE_THRESHOLD, WarningLevel,
};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef,
    TableProfile,
};

use crate::{AnalysisResult, DataSourceIntrospector, DEFAULT_INTROSPECTION_CONCURRENCY, introspect_tables_concurrent};

type ProfileResult = (
    usize,
    String,
    Result<(TableProfile, Vec<AnalysisWarning>), OxError>,
);

/// Maximum distinct values to collect per column
const MAX_DISTINCT_VALUES: i64 = 30;
/// Introspection pool: connection count doubles as concurrent task limit
const POOL_MAX_CONNECTIONS: u32 = 10;
const POOL_ACQUIRE_TIMEOUT_SECS: u64 = 10;

pub struct MysqlIntrospector {
    pool: MySqlPool,
    /// MySQL "schema" is the database name
    schema_name: String,
}

impl MysqlIntrospector {
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

    pub async fn introspect_schema_resilient(
        &self,
    ) -> OxResult<(SourceSchema, Vec<AnalysisWarning>)> {
        // 1. Discover tables
        let table_names: Vec<String> = sqlx::query_scalar(
            "SELECT TABLE_NAME FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
             ORDER BY TABLE_NAME",
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
                async move {
                    introspect_table_mysql(&pool, &schema_name, &table_name).await
                }
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
                source_type: "mysql".to_string(),
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

/// Quote a MySQL identifier (table/column name) safely.
/// Wraps in backticks and escapes any embedded backticks by doubling them.
fn quote_ident(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}

#[async_trait]
impl DataSourceIntrospector for MysqlIntrospector {
    fn source_type(&self) -> &str {
        "mysql"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        let (schema, warnings) = self.introspect_schema_resilient().await?;
        for warning in warnings {
            warn!(
                phase = ?warning.phase,
                kind = ?warning.kind,
                location = %warning.location,
                message = %warning.message,
                "MySQL schema introspection completed with warnings"
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
                "MySQL data profiling completed with warnings"
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

/// Introspect a single MySQL table: columns + primary key.
/// Free function to enable capture in concurrent closures without borrowing `self`.
async fn introspect_table_mysql(
    pool: &MySqlPool,
    schema_name: &str,
    table_name: &str,
) -> OxResult<SourceTableDef> {
    // Columns — MySQL uses COLUMN_TYPE for the full type (e.g., "tinyint(1)")
    // and DATA_TYPE for the base type (e.g., "tinyint"). We store COLUMN_TYPE
    // for richer downstream type mapping (distinguishing tinyint(1) as bool).
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
         ORDER BY ORDINAL_POSITION",
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

impl MysqlIntrospector {
    async fn discover_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
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
        // Approximate row count from information_schema.TABLES (InnoDB estimate).
        // Falls back to count(*) when the estimate is unavailable or zero.
        let row_count = self.approximate_row_count(table_name).await?;

        let mut column_stats = Vec::new();
        let mut warnings = Vec::new();
        for col in columns {
            match self.profile_column(table_name, &col.name).await {
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

    /// Get approximate row count using InnoDB table statistics.
    /// `information_schema.TABLES.TABLE_ROWS` is an InnoDB estimate (no full scan).
    /// Falls back to `count(*)` when the estimate reports 0 (empty or freshly created table).
    async fn approximate_row_count(&self, table_name: &str) -> OxResult<u64> {
        let approx: Option<u64> = sqlx::query_scalar(
            "SELECT TABLE_ROWS FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
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
            Some(n) if n > 0 => Ok(n),
            _ => {
                // Fallback: exact count for tables without stats
                let count_query = format!("SELECT COUNT(*) FROM {}", quote_ident(table_name));
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
    ) -> OxResult<(ColumnStats, Option<AnalysisWarning>)> {
        let qt = quote_ident(table_name);
        let qc = quote_ident(column_name);

        // Combined stats query — MySQL uses CAST(col AS CHAR) instead of col::text
        let stats_query = format!(
            "SELECT \
                SUM(CASE WHEN {qc} IS NULL THEN 1 ELSE 0 END) AS null_count, \
                COUNT(DISTINCT {qc}) AS distinct_count, \
                MIN(CAST({qc} AS CHAR)) AS min_val, \
                MAX(CAST({qc} AS CHAR)) AS max_val \
             FROM {qt}",
        );

        let row: (i64, i64, Option<String>, Option<String>) = sqlx::query_as(&stats_query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to profile {table_name}.{column_name}: {e}"),
            })?;

        let (null_count, distinct_count, min_value, max_value) = row;

        // Collect sample values only if distinct count is manageable
        let (sample_values, sample_warning) =
            if distinct_count > 0 && distinct_count <= MAX_DISTINCT_VALUES {
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
            } else {
                (Vec::new(), None)
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
