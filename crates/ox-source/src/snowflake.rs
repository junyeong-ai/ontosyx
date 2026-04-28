//! Snowflake data source adapter.
//!
//! Implements the atomic primitives in [`crate::DataSourceAdapter`] on top
//! of the `snowflake-api` crate. Every primitive issues an INFORMATION_SCHEMA
//! query (or the equivalent SHOW command) and maps the Arrow / JSON result
//! into `ox_core::source_schema` types.
//!
//! Snowflake's connection and query execution are routed through HTTPS (REST
//! SQL API), so there is no long-lived session state on the client side.
//! `SnowflakeApi` exposes `exec(&self, sql)`, making concurrent primitive
//! calls safe without mutex-serialisation — the `IntrospectionKernel`'s
//! fan-out delivers real parallelism on Snowflake.
//!
//! Auth is password-based for now. Key-pair / OAuth / browser-auth can plug
//! in through additional factory functions on `SnowflakeAdapter` without
//! changing the trait implementation.

use std::sync::Arc;

use arrow::array::{Array, Int32Array, Int64Array, StringArray};
use arrow::datatypes::DataType;
use async_trait::async_trait;
use snowflake_api::{QueryResult, SnowflakeApi};

use ox_core::error::{OxError, OxResult};
use ox_ontology::source_analysis::ENUM_CARDINALITY_THRESHOLD;
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef, TableSummary,
};

use crate::DataSourceAdapter;

/// Baseline enum threshold — columns at or below this are definite enums
/// (collect every distinct value as a sample).
const DEFINITE_ENUM_CARDINALITY: i64 = 30;

pub struct SnowflakeAdapter {
    /// Shared Snowflake REST SQL API client. `Arc` because `exec()` is
    /// `&self`, so concurrent kernel primitives share one client.
    client: Arc<SnowflakeApi>,
    /// Connection snapshots. Retained for diagnostic display and for
    /// schema-qualifying INFORMATION_SCHEMA queries (database/schema).
    /// `#[allow(dead_code)]` on account/warehouse/user: exposed for test
    /// assertions and future observability; not read by production code.
    #[allow(dead_code)]
    account: String,
    database: String,
    schema: String,
    #[allow(dead_code)]
    warehouse: String,
    #[allow(dead_code)]
    user: String,
}

// `SnowflakeApi` has no `Debug`, so we implement one here that elides the
// client (it carries credentials) and surfaces the connection identity.
impl std::fmt::Debug for SnowflakeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnowflakeAdapter")
            .field("account", &self.account)
            .field("database", &self.database)
            .field("schema", &self.schema)
            .field("warehouse", &self.warehouse)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

impl SnowflakeAdapter {
    /// Construct from individual credentials. `schema` is the
    /// default schema both for `USE SCHEMA` on the session and for
    /// qualifying INFORMATION_SCHEMA queries in every primitive.
    pub fn from_params(
        account: &str,
        user: &str,
        password: &str,
        warehouse: &str,
        database: &str,
        schema: &str,
    ) -> OxResult<Self> {
        validate_param("account", account)?;
        validate_param("user", user)?;
        validate_param("database", database)?;

        let client = SnowflakeApi::with_password_auth(
            account,
            Some(warehouse),
            Some(database),
            Some(schema),
            user,
            None,
            password,
        )
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to build Snowflake client: {e}"),
        })?;

        Ok(Self {
            client: Arc::new(client),
            account: account.to_string(),
            database: database.to_string(),
            schema: schema.to_uppercase(),
            warehouse: warehouse.to_string(),
            user: user.to_string(),
        })
    }

    /// Parse a Snowflake connection URI in the format:
    /// `snowflake://{account}/{database}/{schema}?user={user}&password={password}&warehouse={warehouse}`
    ///
    /// Deliberate use of manual parsing (no `url` crate dependency on this
    /// path) — the URI shape is bespoke anyway (account identifier is the
    /// host, path carries database + schema).
    pub fn from_connection_string(connection_string: &str) -> OxResult<Self> {
        let cs = connection_string.trim();
        let expected_format = "snowflake://{account}/{database}/{schema}\
                               ?user={user}&password={password}&warehouse={warehouse}";

        let rest = cs
            .strip_prefix("snowflake://")
            .ok_or_else(|| OxError::Validation {
                field: "connection_string".to_string(),
                message: format!(
                    "Expected 'snowflake://' scheme. Expected format: {expected_format}"
                ),
            })?;

        let (path_part, query_part) = match rest.split_once('?') {
            Some((p, q)) => (p, q),
            None => (rest, ""),
        };

        let segments: Vec<&str> = path_part.split('/').collect();
        let account = (*segments.first().unwrap_or(&"")).to_string();
        let database = (*segments.get(1).unwrap_or(&"")).to_string();
        let schema = if segments.len() > 2 && !segments[2].is_empty() {
            segments[2].to_string()
        } else {
            "PUBLIC".to_string()
        };

        let params: std::collections::HashMap<&str, &str> = query_part
            .split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| pair.split_once('='))
            .collect();

        let user = (*params.get("user").unwrap_or(&"")).to_string();
        let password = (*params.get("password").unwrap_or(&"")).to_string();
        let warehouse = (*params.get("warehouse").unwrap_or(&"")).to_string();

        Self::from_params(&account, &user, &password, &warehouse, &database, &schema)
    }

    /// Execute a SQL statement and return its rows as a list of
    /// string-coerced cells. Non-string Arrow column types are rendered
    /// via their standard `Display` impl; SQL `NULL` becomes `None`.
    ///
    /// Kept at `Vec<Vec<Option<String>>>` because every query issued here
    /// reads from INFORMATION_SCHEMA — a handful of columns per row,
    /// overwhelmingly text / integer. Callers cast the cells they need.
    async fn exec_rows(&self, sql: &str) -> OxResult<Vec<Vec<Option<String>>>> {
        let result = self.client.exec(sql).await.map_err(|e| OxError::Runtime {
            message: format!("Snowflake query failed: {e} [query: {sql}]"),
        })?;

        match result {
            QueryResult::Arrow(batches) => Ok(rows_from_arrow_batches(&batches)),
            QueryResult::Json(j) => Ok(rows_from_json_payload(&j.value)),
            QueryResult::Empty => Ok(Vec::new()),
        }
    }
}

/// Validate a Snowflake identifier-ish parameter: present and alphanumeric
/// plus `_` / `-` / `.`. Applied to account/user/database at construction
/// so downstream SQL can use them without re-validating.
fn validate_param(field: &str, value: &str) -> OxResult<()> {
    if value.is_empty() {
        return Err(OxError::Validation {
            field: field.to_string(),
            message: format!("Snowflake {field} is required"),
        });
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(OxError::Validation {
            field: field.to_string(),
            message: format!(
                "Snowflake {field} must be alphanumeric with `_`, `-`, or `.` only: `{value}`"
            ),
        });
    }
    Ok(())
}

/// Quote a Snowflake identifier — wraps in double quotes, doubles any
/// embedded quotes. Used only on table/column names that we've fetched
/// from INFORMATION_SCHEMA (already-trusted identifiers), never on
/// free-form user input.
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Escape a SQL string literal — doubles any single-quote, wraps in
/// single quotes. Used for WHERE clauses that bind on string values
/// returned from prior queries (table names, column names).
fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[async_trait]
impl DataSourceAdapter for SnowflakeAdapter {
    fn source_type(&self) -> &str {
        "snowflake"
    }

    /// Promote Snowflake-specific failure modes (warehouse suspended,
    /// …) into stable [`WarningClass`] variants for the FE.
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
        let class = if raw.contains("Warehouse") && raw.contains("suspended") {
            WarningClass::SnowflakeWarehouseSuspended
        } else {
            default_class
        };
        AnalysisWarning::new(level, phase, class, scope).with_detail(raw)
    }

    async fn list_tables(&self) -> OxResult<Vec<String>> {
        // Qualify INFORMATION_SCHEMA with the database so we can query
        // across databases if needed (Snowflake defaults to the session
        // database, but an explicit qualifier removes ambiguity).
        // INFORMATION_SCHEMA.TABLES surfaces every queryable Snowflake
        // object — base tables, transient tables, views, materialised
        // views, external tables, dynamic tables. Temporary tables are
        // session-local and do not appear here.
        let sql = format!(
            "SELECT TABLE_NAME FROM {db}.INFORMATION_SCHEMA.TABLES \
             WHERE TABLE_SCHEMA = {schema} \
             ORDER BY TABLE_NAME",
            db = quote_ident(&self.database),
            schema = quote_literal(&self.schema),
        );

        let rows = self.exec_rows(&sql).await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.into_iter().next().and_then(|c| c))
            .collect())
    }

    async fn list_tables_with_summary(&self) -> OxResult<Vec<TableSummary>> {
        // Single round-trip: ROW_COUNT is the maintained estimate
        // (NULL for views / external / dynamic tables that do not
        // materialise rows directly), LAST_ALTERED is the DDL/DML
        // high-water mark, COLUMN_COUNT joins via a correlated
        // subquery against INFORMATION_SCHEMA.COLUMNS. Every Snowflake
        // table kind in the schema is included — managed and view-like
        // alike — so callers see the full data surface.
        let sql = format!(
            "SELECT t.TABLE_NAME, \
                    (SELECT COUNT(*) \
                       FROM {db}.INFORMATION_SCHEMA.COLUMNS c \
                      WHERE c.TABLE_SCHEMA = t.TABLE_SCHEMA \
                        AND c.TABLE_NAME = t.TABLE_NAME) AS COLUMN_COUNT, \
                    t.ROW_COUNT, \
                    TO_VARCHAR(t.LAST_ALTERED, 'YYYY-MM-DD\"T\"HH24:MI:SS.FF3\"Z\"') AS LAST_ALTERED \
             FROM {db}.INFORMATION_SCHEMA.TABLES t \
             WHERE t.TABLE_SCHEMA = {schema} \
             ORDER BY t.TABLE_NAME",
            db = quote_ident(&self.database),
            schema = quote_literal(&self.schema),
        );
        let rows = self.exec_rows(&sql).await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let name = match row.first().and_then(|c| c.clone()) {
                Some(n) => n,
                None => continue,
            };
            let column_count = row
                .get(1)
                .and_then(|c| c.as_ref())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let estimated_row_count = row
                .get(2)
                .and_then(|c| c.as_ref())
                .and_then(|s| s.parse::<u64>().ok());
            let last_modified = row
                .get(3)
                .and_then(|c| c.as_ref())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));
            out.push(TableSummary {
                name,
                estimated_row_count,
                column_count,
                last_modified,
            });
        }
        Ok(out)
    }

    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
        let col_sql = format!(
            "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE \
             FROM {db}.INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA = {schema} AND TABLE_NAME = {table} \
             ORDER BY ORDINAL_POSITION",
            db = quote_ident(&self.database),
            schema = quote_literal(&self.schema),
            table = quote_literal(table),
        );
        let col_rows = self.exec_rows(&col_sql).await?;
        let columns: Vec<SourceColumnDef> = col_rows
            .into_iter()
            .filter_map(|row| {
                let name = row.first().cloned().flatten()?;
                let data_type = row.get(1).cloned().flatten().unwrap_or_default();
                let is_nullable = row.get(2).cloned().flatten().unwrap_or_default();
                Some(SourceColumnDef {
                    name,
                    data_type: data_type.to_lowercase(),
                    nullable: is_nullable.eq_ignore_ascii_case("YES"),
                })
            })
            .collect();

        let pk_sql = format!(
            "SELECT kcu.COLUMN_NAME \
             FROM {db}.INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
             JOIN {db}.INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu \
               ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
               AND tc.TABLE_SCHEMA = kcu.TABLE_SCHEMA \
               AND tc.TABLE_NAME = kcu.TABLE_NAME \
             WHERE tc.TABLE_SCHEMA = {schema} AND tc.TABLE_NAME = {table} \
               AND tc.CONSTRAINT_TYPE = 'PRIMARY KEY' \
             ORDER BY kcu.ORDINAL_POSITION",
            db = quote_ident(&self.database),
            schema = quote_literal(&self.schema),
            table = quote_literal(table),
        );
        let pk_rows = self.exec_rows(&pk_sql).await?;
        let primary_key: Vec<String> = pk_rows
            .into_iter()
            .filter_map(|r| r.into_iter().next().and_then(|c| c))
            .collect();

        Ok(SourceTableDef {
            name: table.to_string(),
            columns,
            primary_key,
        })
    }

    async fn count_rows(&self, table: &str) -> OxResult<u64> {
        // Snowflake maintains per-table row count in INFORMATION_SCHEMA.TABLES
        // (ROW_COUNT column). Clone-table / recently-vacuumed tables may
        // report 0; in that case we fall back to an exact COUNT(*).
        let approx_sql = format!(
            "SELECT ROW_COUNT FROM {db}.INFORMATION_SCHEMA.TABLES \
             WHERE TABLE_SCHEMA = {schema} AND TABLE_NAME = {table}",
            db = quote_ident(&self.database),
            schema = quote_literal(&self.schema),
            table = quote_literal(table),
        );
        let approx_rows = self.exec_rows(&approx_sql).await?;
        let approx = approx_rows
            .into_iter()
            .next()
            .and_then(|r| r.into_iter().next().and_then(|c| c))
            .and_then(|s| s.parse::<i64>().ok());

        if let Some(n) = approx
            && n > 0
        {
            return Ok(n as u64);
        }

        // Exact fallback. Uses the fully qualified name to guard against
        // session default-schema drift.
        let count_sql = format!(
            "SELECT COUNT(*) FROM {db}.{schema}.{table}",
            db = quote_ident(&self.database),
            schema = quote_ident(&self.schema),
            table = quote_ident(table),
        );
        let rows = self.exec_rows(&count_sql).await?;
        let cell = rows
            .into_iter()
            .next()
            .and_then(|r| r.into_iter().next().and_then(|c| c))
            .unwrap_or_else(|| "0".to_string());
        Ok(cell.parse::<i64>().unwrap_or_default().max(0) as u64)
    }

    async fn sample_column(&self, table: &str, column: &SourceColumnDef) -> OxResult<ColumnStats> {
        let qt = quote_ident(table);
        let qc = quote_ident(&column.name);

        // Combined aggregation: null count + distinct count + min + max in a
        // single round-trip. Snowflake can evaluate all four off one table
        // scan.
        let stats_sql = format!(
            "SELECT \
                COUNT_IF({qc} IS NULL), \
                COUNT(DISTINCT {qc}), \
                MIN(CAST({qc} AS STRING)), \
                MAX(CAST({qc} AS STRING)) \
             FROM {db}.{schema}.{qt}",
            db = quote_ident(&self.database),
            schema = quote_ident(&self.schema),
        );
        let rows = self.exec_rows(&stats_sql).await?;
        let row = rows.into_iter().next().ok_or_else(|| OxError::Runtime {
            message: format!(
                "Snowflake stats query for {table}.{} returned no rows",
                column.name
            ),
        })?;

        let null_count = parse_i64_cell(row.first());
        let distinct_count = parse_i64_cell(row.get(1));
        let min_value = row.get(2).cloned().flatten();
        let max_value = row.get(3).cloned().flatten();

        // Sample-value budget mirrors PostgresAdapter: definite enums
        // (<=30) collect every value, medium cardinality only if short,
        // high cardinality skips.
        let extended_threshold = ENUM_CARDINALITY_THRESHOLD as i64;
        let sample_limit = if distinct_count <= 0 {
            0
        } else if distinct_count <= DEFINITE_ENUM_CARDINALITY {
            distinct_count
        } else if distinct_count <= extended_threshold {
            let avg_len_sql = format!(
                "SELECT COALESCE(AVG(LENGTH(val)), 0)::INT FROM (\
                 SELECT CAST({qc} AS STRING) AS val FROM {db}.{schema}.{qt} \
                 WHERE {qc} IS NOT NULL LIMIT 1000\
                 ) sub",
                db = quote_ident(&self.database),
                schema = quote_ident(&self.schema),
            );
            let avg_rows = self.exec_rows(&avg_len_sql).await.unwrap_or_default();
            let avg_len = avg_rows
                .into_iter()
                .next()
                .and_then(|r| r.into_iter().next().and_then(|c| c))
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(999);
            if avg_len <= 50 { distinct_count } else { 0 }
        } else {
            0
        };

        let sample_values = if sample_limit <= 0 {
            Vec::new()
        } else {
            let sample_sql = format!(
                "SELECT DISTINCT LEFT(CAST({qc} AS STRING), 200) AS val \
                 FROM {db}.{schema}.{qt} \
                 WHERE {qc} IS NOT NULL \
                 ORDER BY val \
                 LIMIT {sample_limit}",
                db = quote_ident(&self.database),
                schema = quote_ident(&self.schema),
            );
            let sample_rows = self.exec_rows(&sample_sql).await.unwrap_or_default();
            sample_rows
                .into_iter()
                .filter_map(|r| r.into_iter().next().and_then(|c| c))
                .collect()
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
        // Snowflake's declared foreign keys surface in
        // INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS, joined against
        // KEY_COLUMN_USAGE for the from/to column pairs.
        let sql = format!(
            "SELECT \
                rc.CONSTRAINT_NAME, \
                kcu.TABLE_NAME AS from_table, \
                kcu.COLUMN_NAME AS from_column, \
                rc.UNIQUE_CONSTRAINT_SCHEMA || '.' || rc.UNIQUE_CONSTRAINT_NAME AS to_key, \
                uc_kcu.TABLE_NAME AS to_table, \
                uc_kcu.COLUMN_NAME AS to_column \
             FROM {db}.INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS rc \
             JOIN {db}.INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu \
               ON rc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
               AND rc.CONSTRAINT_SCHEMA = kcu.CONSTRAINT_SCHEMA \
             JOIN {db}.INFORMATION_SCHEMA.KEY_COLUMN_USAGE uc_kcu \
               ON rc.UNIQUE_CONSTRAINT_NAME = uc_kcu.CONSTRAINT_NAME \
               AND rc.UNIQUE_CONSTRAINT_SCHEMA = uc_kcu.CONSTRAINT_SCHEMA \
               AND kcu.ORDINAL_POSITION = uc_kcu.ORDINAL_POSITION \
             WHERE rc.CONSTRAINT_SCHEMA = {schema} \
             ORDER BY rc.CONSTRAINT_NAME",
            db = quote_ident(&self.database),
            schema = quote_literal(&self.schema),
        );
        let rows = self.exec_rows(&sql).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let from_table = row.get(1).cloned().flatten()?;
                let from_column = row.get(2).cloned().flatten()?;
                let to_table = row.get(4).cloned().flatten()?;
                let to_column = row.get(5).cloned().flatten()?;
                Some(ForeignKeyDef {
                    from_table,
                    from_column,
                    to_table,
                    to_column,
                    inferred: false,
                })
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Result-shape helpers
// ---------------------------------------------------------------------------

/// Convert an Arrow `RecordBatch` vector into `Vec<Vec<Option<String>>>`.
/// Handles the subset of Arrow types that Snowflake actually returns for
/// INFORMATION_SCHEMA queries (Utf8 / LargeUtf8 / Int / BigInt) plus a
/// fallback that formats any other array's scalar value via its `Display`.
fn rows_from_arrow_batches(batches: &[arrow::array::RecordBatch]) -> Vec<Vec<Option<String>>> {
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    for batch in batches {
        let num_rows = batch.num_rows();
        let num_cols = batch.num_columns();
        for r in 0..num_rows {
            let mut row = Vec::with_capacity(num_cols);
            for c in 0..num_cols {
                let column = batch.column(c);
                if column.is_null(r) {
                    row.push(None);
                    continue;
                }
                let value = arrow_cell_to_string(column.as_ref(), r);
                row.push(Some(value));
            }
            rows.push(row);
        }
    }
    rows
}

/// Pull a single cell out of an Arrow array at row `r`. Covers the three
/// concrete types INFORMATION_SCHEMA queries actually return and falls
/// back to the array's debug representation for anything else.
fn arrow_cell_to_string(array: &dyn Array, r: usize) -> String {
    match array.data_type() {
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|a| a.value(r).to_string())
            .unwrap_or_default(),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| a.value(r).to_string())
            .unwrap_or_default(),
        DataType::Int32 => array
            .as_any()
            .downcast_ref::<Int32Array>()
            .map(|a| a.value(r).to_string())
            .unwrap_or_default(),
        _ => {
            // Coarse fallback: ask the array to format itself. This keeps
            // the primitive working on type variants Snowflake might send
            // for BIGINT arithmetic (Int32 vs Int64) or DATE results.
            format!("{:?}", array.slice(r, 1))
        }
    }
}

/// Parse a JSON payload shaped as a 2-d array of scalars —
/// `[["a", "1"], ["b", "2"]]` — into `Vec<Vec<Option<String>>>`.
///
/// Snowflake's JSON variant on non-SELECT or metadata endpoints takes
/// this shape; adapt the structure defensively and fall back to an
/// empty vec on unexpected layouts so the caller's kernel-level flow
/// surfaces an empty result rather than a hard error.
fn rows_from_json_payload(value: &serde_json::Value) -> Vec<Vec<Option<String>>> {
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|row| row.as_array())
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    serde_json::Value::Null => None,
                    serde_json::Value::String(s) => Some(s.clone()),
                    other => Some(other.to_string()),
                })
                .collect()
        })
        .collect()
}

fn parse_i64_cell(cell: Option<&Option<String>>) -> i64 {
    cell.and_then(|c| c.as_ref())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests — connection-string parsing + parameter validation only.
// Live integration is out of scope per plan (no stubs, no `#[ignore]` gated
// network tests). Connection establishment requires a real Snowflake
// account which CI cannot provide.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_connection_string() {
        let cs = "snowflake://xy12345.us-east-1/MY_DB/MY_SCHEMA?user=alice&password=secret&warehouse=COMPUTE_WH";
        let adapter = SnowflakeAdapter::from_connection_string(cs).unwrap();
        assert_eq!(adapter.account, "xy12345.us-east-1");
        assert_eq!(adapter.database, "MY_DB");
        assert_eq!(adapter.schema, "MY_SCHEMA");
        assert_eq!(adapter.warehouse, "COMPUTE_WH");
        assert_eq!(adapter.user, "alice");
    }

    #[test]
    fn parse_connection_string_defaults_schema_to_public() {
        let cs = "snowflake://xy12345/MY_DB?user=alice&password=secret&warehouse=WH";
        let adapter = SnowflakeAdapter::from_connection_string(cs).unwrap();
        assert_eq!(adapter.schema, "PUBLIC");
    }

    #[test]
    fn parse_connection_string_normalizes_schema_case() {
        // Schema stored upper-cased so INFORMATION_SCHEMA WHERE clauses
        // line up with Snowflake's default normalisation.
        let cs = "snowflake://xy/MY_DB/my_schema?user=a&password=b&warehouse=w";
        let adapter = SnowflakeAdapter::from_connection_string(cs).unwrap();
        assert_eq!(adapter.schema, "MY_SCHEMA");
    }

    #[test]
    fn parse_connection_string_wrong_scheme() {
        let result = SnowflakeAdapter::from_connection_string("postgres://host/db");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("snowflake://"),
            "Error should mention expected scheme: {err}"
        );
    }

    #[test]
    fn from_params_validates_required_fields() {
        assert!(SnowflakeAdapter::from_params("", "user", "pass", "wh", "db", "schema").is_err());
        assert!(SnowflakeAdapter::from_params("acct", "", "pass", "wh", "db", "schema").is_err());
        assert!(SnowflakeAdapter::from_params("acct", "user", "pass", "wh", "", "schema").is_err());
    }

    #[test]
    fn from_params_rejects_identifier_injection() {
        // Single quotes and semicolons must be rejected at the parameter
        // layer — otherwise they'd end up un-escaped in INFORMATION_SCHEMA
        // predicates we interpolate.
        assert!(
            SnowflakeAdapter::from_params(
                "acct';DROP DATABASE neo4j;--",
                "user",
                "pass",
                "wh",
                "db",
                "schema"
            )
            .is_err()
        );
        assert!(
            SnowflakeAdapter::from_params("acct", "user\"name", "pass", "wh", "db", "schema")
                .is_err()
        );
    }

    #[test]
    fn source_type_returns_snowflake() {
        let adapter =
            SnowflakeAdapter::from_params("acct", "user", "pass", "wh", "db", "schema").unwrap();
        assert_eq!(adapter.source_type(), "snowflake");
    }

    #[test]
    fn quote_literal_doubles_single_quotes() {
        assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
        assert_eq!(quote_literal("simple"), "'simple'");
        assert_eq!(quote_literal(""), "''");
    }

    #[test]
    fn quote_ident_doubles_double_quotes() {
        assert_eq!(quote_ident("plain"), "\"plain\"");
        assert_eq!(quote_ident("has\"quote"), "\"has\"\"quote\"");
    }
}
