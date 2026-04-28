//! BigQuery data source adapter.
//!
//! Implements the atomic primitives in [`crate::DataSourceAdapter`] on top
//! of the `gcp-bigquery-client` crate. Every primitive issues a BigQuery
//! Standard SQL query against INFORMATION_SCHEMA (or the `__TABLES__`
//! metadata table for row counts) and maps the `ResultSet` into
//! `ox_core::source_schema` types.
//!
//! ## Project model
//!
//! BigQuery distinguishes the project where the *data* lives from the
//! project that is *billed* for the query job. The two are usually the
//! same, but enterprise deployments routinely split them — e.g.
//! read-only access to a shared data project plus a separate "compute"
//! project that holds the user's `bigquery.jobs.create` permission.
//! VPC Service Controls perimeters can also force a particular billing
//! project.
//!
//! The adapter therefore takes:
//!
//! - `project_id` — the data project, used in fully-qualified
//!   identifiers (`{project_id}.{dataset}.INFORMATION_SCHEMA.X`).
//! - `billing_project_id` — optional, defaults to `project_id`. Used as
//!   the `projectId` argument when submitting jobs.
//!
//! ## Authentication
//!
//! ADC dispatch lives in [`ox_gcp::auth`]. The adapter consults
//! [`ox_gcp::auth::detect_adc`] (with the optional
//! `credentials_path` override) and routes the result to the matching
//! `gcp-bigquery-client` constructor:
//!
//! | Credential kind        | Constructor                                     |
//! | ---------------------- | ----------------------------------------------- |
//! | Service-account JSON   | `Client::from_service_account_key_file`         |
//! | Authorized-user secret | `Client::from_authorized_user_secret`           |
//! | Metadata server        | `Client::from_application_default_credentials`  |
//!
//! This keeps `gcloud auth application-default login` (the standard
//! developer-laptop flow) working without operators having to mint
//! service-account keys.

use std::path::Path;
use std::sync::Arc;

use arrow::array::{ArrayBuilder, RecordBatch};
use async_trait::async_trait;
use gcp_bigquery_client::Client;
use gcp_bigquery_client::model::query_request::QueryRequest;
use gcp_bigquery_client::model::query_response::ResultSet;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_gcp::auth::AdcCredential;
use ox_ontology::source_analysis::ENUM_CARDINALITY_THRESHOLD;

use crate::normalize::describe_to_arrow_schema;
use crate::text_scan::{append_text_cell, make_builder};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceTableDef, TableSummary,
};

use crate::DataSourceAdapter;

/// Baseline enum threshold — columns at or below this are definite enums.
const DEFINITE_ENUM_CARDINALITY: i64 = 30;

pub struct BigQueryAdapter {
    client: Arc<Client>,
    project_id: String,
    dataset: String,
    /// Project that pays for and runs the BQ jobs. Defaults to
    /// `project_id` when the caller does not split billing from data.
    billing_project_id: String,
}

impl std::fmt::Debug for BigQueryAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BigQueryAdapter")
            .field("project_id", &self.project_id)
            .field("dataset", &self.dataset)
            .field("billing_project_id", &self.billing_project_id)
            .finish_non_exhaustive()
    }
}

impl BigQueryAdapter {
    /// Parse a BigQuery connection URI and construct a ready-to-query
    /// adapter.
    ///
    /// Expected format:
    /// `bigquery://PROJECT_ID/DATASET[?billing_project_id=BILLING][&credentials_path=PATH]`
    pub async fn connect(connection_string: &str) -> OxResult<Self> {
        let parsed = parse_bigquery_uri(connection_string)?;
        validate_project_or_dataset("project_id", &parsed.project_id)?;
        validate_project_or_dataset("dataset", &parsed.dataset)?;
        if let Some(b) = &parsed.billing_project_id {
            validate_project_or_dataset("billing_project_id", b)?;
        }

        let override_path = parsed.credentials_path.as_deref().map(Path::new);
        let credential = ox_gcp::auth::detect_adc(override_path)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("BigQuery ADC dispatch failed: {e}"),
            })?;

        let client = build_client(&credential).await?;

        let billing_project_id = parsed
            .billing_project_id
            .clone()
            .unwrap_or_else(|| parsed.project_id.clone());

        info!(
            project_id = %parsed.project_id,
            dataset = %parsed.dataset,
            billing_project_id = %billing_project_id,
            credential = %credential_kind_label(&credential),
            "BigQuery adapter connected"
        );

        Ok(Self {
            client: Arc::new(client),
            project_id: parsed.project_id,
            dataset: parsed.dataset,
            billing_project_id,
        })
    }

    /// Execute a Standard SQL query and return the resulting rows.
    /// Jobs always run in the billing project — fully-qualified table
    /// names already carry the data-project prefix.
    async fn run_query(&self, sql: &str) -> OxResult<ResultSet> {
        self.client
            .job()
            .query(&self.billing_project_id, QueryRequest::new(sql))
            .await
            .map_err(|e| OxError::Runtime {
                message: format!(
                    "BigQuery query failed: {e} [query: {}]",
                    truncate_for_error(sql)
                ),
            })
    }
}

async fn build_client(credential: &AdcCredential) -> OxResult<Client> {
    let result = match credential {
        AdcCredential::ServiceAccountKey(path) => {
            Client::from_service_account_key_file(path_as_str(path)?).await
        }
        AdcCredential::AuthorizedUser(path) => {
            Client::from_authorized_user_secret(path_as_str(path)?).await
        }
        AdcCredential::Metadata => Client::from_application_default_credentials().await,
    };
    result.map_err(|e| OxError::Runtime {
        message: format!(
            "BigQuery client construction failed ({}): {e}",
            credential_kind_label(credential)
        ),
    })
}

fn path_as_str(path: &Path) -> OxResult<&str> {
    path.to_str().ok_or_else(|| OxError::Runtime {
        message: format!("BigQuery credential path is not valid UTF-8: {}", path.display()),
    })
}

fn credential_kind_label(credential: &AdcCredential) -> &'static str {
    match credential {
        AdcCredential::ServiceAccountKey(_) => "service_account",
        AdcCredential::AuthorizedUser(_) => "authorized_user",
        AdcCredential::Metadata => "metadata_server",
    }
}

fn truncate_for_error(q: &str) -> String {
    let max = 300usize;
    if q.len() <= max {
        q.to_string()
    } else {
        format!("{}…", &q[..max])
    }
}

/// Validate a BigQuery project ID or dataset ID. BigQuery rules are
/// slightly looser than Snowflake — hyphens are allowed in project IDs,
/// dataset IDs require `[A-Za-z_][A-Za-z0-9_]{0,1023}` — but a single
/// tight whitelist covers both without falsely rejecting legitimate
/// identifiers.
fn validate_project_or_dataset(field: &str, value: &str) -> OxResult<()> {
    if value.is_empty() {
        return Err(OxError::Validation {
            field: field.to_string(),
            message: format!("BigQuery {field} is required"),
        });
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return Err(OxError::Validation {
            field: field.to_string(),
            message: format!(
                "BigQuery {field} must be alphanumeric with `_` or `-` only: `{value}`"
            ),
        });
    }
    Ok(())
}

/// Quote a BigQuery identifier — wraps in backticks, escapes any
/// embedded backtick by doubling. Used only on names fetched from
/// INFORMATION_SCHEMA (already-trusted identifiers).
fn quote_ident(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}

/// Escape a SQL string literal — doubles any single-quote, wraps in
/// single quotes. Used for WHERE clauses that bind on string values.
fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[async_trait]
impl DataSourceAdapter for BigQueryAdapter {
    fn source_type(&self) -> &str {
        "bigquery"
    }

    fn supports_scan(&self) -> bool {
        true
    }

    /// BigQuery surfaces a small, recognisable set of failure modes
    /// inside the otherwise-noisy `BQError::ResponseError` shape.
    /// Promote them to specific [`WarningClass`] variants so the FE
    /// can render a targeted summary + actionable hint instead of a
    /// 500-character JSON dump.
    fn classify_warning(
        &self,
        level: ox_ontology::source_analysis::WarningLevel,
        phase: ox_ontology::source_analysis::AnalysisPhase,
        default_class: ox_ontology::source_analysis::WarningClass,
        scope: ox_ontology::source_analysis::WarningScope,
        error: &OxError,
    ) -> ox_ontology::source_analysis::AnalysisWarning {
        use ox_ontology::source_analysis::AnalysisWarning;
        let raw = error.to_string();
        let (class, params) = classify_bigquery_error(&raw, default_class);
        let mut warning = AnalysisWarning::new(level, phase, class, scope).with_detail(raw);
        for (key, value) in params {
            warning = warning.with_param(key, value);
        }
        warning
    }

    async fn list_tables(&self) -> OxResult<Vec<String>> {
        // INFORMATION_SCHEMA.TABLES surfaces every queryable object in
        // the dataset — base tables, views, materialized views,
        // external tables, snapshots, clones. We expose them all
        // because each is a legitimate analysis target: an ontology
        // designed against a warehouse should see the same surface
        // the warehouse exposes to its consumers, not just the
        // managed-table subset.
        //
        // The fully-qualified path lives inside one backtick pair —
        // BigQuery does not accept multi-segment dotted paths split
        // across separate quoting.
        let sql = format!(
            "SELECT table_name FROM `{project}.{dataset}.INFORMATION_SCHEMA.TABLES` \
             ORDER BY table_name",
            project = self.project_id,
            dataset = self.dataset,
        );
        let mut rs = self.run_query(&sql).await?;
        let mut tables = Vec::new();
        while rs.next_row() {
            if let Some(name) = rs.get_string_by_name("table_name").map_err(bq_row_err)? {
                tables.push(name);
            }
        }
        Ok(tables)
    }

    async fn list_tables_with_summary(&self) -> OxResult<Vec<TableSummary>> {
        // `__TABLES__` is the dataset-scoped metadata view that carries
        // both row_count and last_modified_time (millis since epoch),
        // and includes every table kind the dataset exposes — base
        // tables, views, clones, snapshots, external. The newer
        // `INFORMATION_SCHEMA.TABLE_STORAGE` view is region-scoped
        // (region-asia-northeast3.INFORMATION_SCHEMA…), introduces
        // cross-region permission coupling, and excludes views — so it
        // is the wrong primitive for a cheap dataset-level listing.
        //
        // Column count joins from INFORMATION_SCHEMA.COLUMNS via a
        // correlated subquery; both views are dataset-scoped so the
        // fully-qualified path stays inside a single backtick pair.
        let sql = format!(
            "SELECT t.table_id AS table_name, \
                    (SELECT COUNT(*) \
                       FROM `{project}.{dataset}.INFORMATION_SCHEMA.COLUMNS` c \
                      WHERE c.table_name = t.table_id) AS column_count, \
                    t.row_count, \
                    t.last_modified_time \
             FROM `{project}.{dataset}.__TABLES__` t \
             ORDER BY t.table_id",
            project = self.project_id,
            dataset = self.dataset,
        );
        let mut rs = self.run_query(&sql).await?;
        let mut out = Vec::new();
        while rs.next_row() {
            let name = match rs.get_string_by_name("table_name").map_err(bq_row_err)? {
                Some(n) => n,
                None => continue,
            };
            let column_count = rs
                .get_i64_by_name("column_count")
                .map_err(bq_row_err)?
                .and_then(|n| u32::try_from(n.max(0)).ok())
                .unwrap_or(0);
            let row_count = rs
                .get_i64_by_name("row_count")
                .map_err(bq_row_err)?
                .and_then(|n| u64::try_from(n.max(0)).ok());
            let last_modified = rs
                .get_i64_by_name("last_modified_time")
                .map_err(bq_row_err)?
                .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_millis);
            out.push(TableSummary {
                name,
                estimated_row_count: row_count,
                column_count,
                last_modified,
            });
        }
        Ok(out)
    }

    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
        let col_sql = format!(
            "SELECT column_name, data_type, is_nullable \
             FROM `{project}.{dataset}.INFORMATION_SCHEMA.COLUMNS` \
             WHERE table_name = {table} \
             ORDER BY ordinal_position",
            project = self.project_id,
            dataset = self.dataset,
            table = quote_literal(table),
        );
        let mut col_rs = self.run_query(&col_sql).await?;
        let mut columns = Vec::new();
        while col_rs.next_row() {
            let name = match col_rs
                .get_string_by_name("column_name")
                .map_err(bq_row_err)?
            {
                Some(n) => n,
                None => continue,
            };
            let data_type = col_rs
                .get_string_by_name("data_type")
                .map_err(bq_row_err)?
                .unwrap_or_default()
                .to_lowercase();
            let is_nullable = col_rs
                .get_string_by_name("is_nullable")
                .map_err(bq_row_err)?
                .unwrap_or_default();
            columns.push(SourceColumnDef {
                name,
                data_type,
                nullable: is_nullable.eq_ignore_ascii_case("YES"),
            });
        }

        // BigQuery added informational PRIMARY KEY constraints in 2023;
        // the catalog tables exist but may not be populated for every
        // table. Treat a failure-to-query here as "no declared PK" to
        // keep describe_table resilient.
        let primary_key = self.read_primary_key(table).await.unwrap_or_default();

        Ok(SourceTableDef {
            name: table.to_string(),
            columns,
            primary_key,
        })
    }

    async fn count_rows(&self, table: &str) -> OxResult<u64> {
        // Fast path: `__TABLES__` is a per-dataset metadata table
        // maintained by BigQuery with live row counts — no scan required.
        let meta_sql = format!(
            "SELECT row_count FROM `{project}.{dataset}.__TABLES__` \
             WHERE table_id = {table}",
            project = self.project_id,
            dataset = self.dataset,
            table = quote_literal(table),
        );
        let mut meta_rs = self.run_query(&meta_sql).await?;
        if meta_rs.next_row()
            && let Some(n) = meta_rs.get_i64_by_name("row_count").map_err(bq_row_err)?
            && n > 0
        {
            return Ok(n as u64);
        }

        // Fallback: full COUNT(*). BigQuery charges by bytes scanned;
        // `SELECT COUNT(*) FROM table` scans the table metadata only
        // (free tier) so this is cheap.
        let count_sql = format!(
            "SELECT COUNT(*) AS cnt FROM `{project}.{dataset}.{table}`",
            project = self.project_id,
            dataset = self.dataset,
            table = table,
        );
        let mut rs = self.run_query(&count_sql).await?;
        if rs.next_row() {
            let cnt = rs.get_i64_by_name("cnt").map_err(bq_row_err)?.unwrap_or(0);
            return Ok(cnt.max(0) as u64);
        }
        Ok(0)
    }

    async fn sample_column(&self, table: &str, column: &SourceColumnDef) -> OxResult<ColumnStats> {
        // `table` is always an INFORMATION_SCHEMA-returned identifier
        // (BigQuery enforces `[A-Za-z_][A-Za-z0-9_]*`) so it's safe to
        // interpolate directly into the dotted `project.dataset.table`
        // backtick path. Columns need their own backtick pair since they
        // appear standalone (`COUNTIF({qc} IS NULL)`).
        let qc = quote_ident(&column.name);

        // Combined aggregation (null_count + distinct_count + min + max)
        // in a single query pass.
        let stats_sql = format!(
            "SELECT \
                COUNTIF({qc} IS NULL) AS null_count, \
                COUNT(DISTINCT {qc}) AS distinct_count, \
                MIN(CAST({qc} AS STRING)) AS min_val, \
                MAX(CAST({qc} AS STRING)) AS max_val \
             FROM `{project}.{dataset}.{table}`",
            project = self.project_id,
            dataset = self.dataset,
            table = table,
        );
        let mut rs = self.run_query(&stats_sql).await?;
        if !rs.next_row() {
            return Err(OxError::Runtime {
                message: format!(
                    "BigQuery stats query for {table}.{} returned no rows",
                    column.name
                ),
            });
        }
        let null_count = rs
            .get_i64_by_name("null_count")
            .map_err(bq_row_err)?
            .unwrap_or(0);
        let distinct_count = rs
            .get_i64_by_name("distinct_count")
            .map_err(bq_row_err)?
            .unwrap_or(0);
        let min_value = rs.get_string_by_name("min_val").map_err(bq_row_err)?;
        let max_value = rs.get_string_by_name("max_val").map_err(bq_row_err)?;

        // Sample budget mirrors PostgresAdapter / SnowflakeAdapter.
        let extended_threshold = ENUM_CARDINALITY_THRESHOLD as i64;
        let sample_limit = if distinct_count <= 0 {
            0
        } else if distinct_count <= DEFINITE_ENUM_CARDINALITY {
            distinct_count
        } else if distinct_count <= extended_threshold {
            let avg_len_sql = format!(
                "SELECT CAST(COALESCE(AVG(LENGTH(val)), 0) AS INT64) AS avg_len \
                 FROM (SELECT CAST({qc} AS STRING) AS val \
                       FROM `{project}.{dataset}.{table}` \
                       WHERE {qc} IS NOT NULL LIMIT 1000)",
                project = self.project_id,
                dataset = self.dataset,
                table = table,
            );
            let mut avg_rs = self.run_query(&avg_len_sql).await?;
            let avg_len = if avg_rs.next_row() {
                avg_rs
                    .get_i64_by_name("avg_len")
                    .unwrap_or(None)
                    .unwrap_or(999)
            } else {
                999
            };
            if avg_len <= 50 { distinct_count } else { 0 }
        } else {
            0
        };

        let sample_values = if sample_limit <= 0 {
            Vec::new()
        } else {
            let sample_sql = format!(
                "SELECT DISTINCT SUBSTR(CAST({qc} AS STRING), 1, 200) AS val \
                 FROM `{project}.{dataset}.{table}` \
                 WHERE {qc} IS NOT NULL \
                 ORDER BY val \
                 LIMIT {sample_limit}",
                project = self.project_id,
                dataset = self.dataset,
                table = table,
            );
            let mut s_rs = self.run_query(&sample_sql).await?;
            let mut values = Vec::new();
            while s_rs.next_row() {
                if let Some(v) = s_rs.get_string_by_name("val").map_err(bq_row_err)? {
                    values.push(v);
                }
            }
            values
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
        // BigQuery added informational FOREIGN KEY constraints in 2023.
        // Availability varies: older datasets / regions may not populate
        // the catalog tables. A missing-table error is NOT a hard
        // failure — treat it as "this dataset has no declared FKs".
        let sql = format!(
            "SELECT \
                kcu.table_name AS from_table, \
                kcu.column_name AS from_column, \
                ccu.table_name AS to_table, \
                ccu.column_name AS to_column \
             FROM `{project}.{dataset}.INFORMATION_SCHEMA.REFERENTIAL_CONSTRAINTS` rc \
             JOIN `{project}.{dataset}.INFORMATION_SCHEMA.KEY_COLUMN_USAGE` kcu \
               ON rc.constraint_name = kcu.constraint_name \
             JOIN `{project}.{dataset}.INFORMATION_SCHEMA.KEY_COLUMN_USAGE` ccu \
               ON rc.unique_constraint_name = ccu.constraint_name \
               AND kcu.ordinal_position = ccu.ordinal_position \
             ORDER BY rc.constraint_name",
            project = self.project_id,
            dataset = self.dataset,
        );

        let mut rs = match self.run_query(&sql).await {
            Ok(rs) => rs,
            Err(_) => return Ok(Vec::new()),
        };

        let mut fks = Vec::new();
        while rs.next_row() {
            let from_table = rs
                .get_string_by_name("from_table")
                .map_err(bq_row_err)?
                .unwrap_or_default();
            let from_column = rs
                .get_string_by_name("from_column")
                .map_err(bq_row_err)?
                .unwrap_or_default();
            let to_table = rs
                .get_string_by_name("to_table")
                .map_err(bq_row_err)?
                .unwrap_or_default();
            let to_column = rs
                .get_string_by_name("to_column")
                .map_err(bq_row_err)?
                .unwrap_or_default();
            if !from_table.is_empty() && !to_table.is_empty() {
                fks.push(ForeignKeyDef {
                    from_table,
                    from_column,
                    to_table,
                    to_column,
                    inferred: false,
                });
            }
        }
        Ok(fks)
    }

    async fn scan(
        &self,
        table: &str,
        projection: Option<Vec<usize>>,
        limit: Option<usize>,
    ) -> OxResult<RecordBatch> {
        // Same shape as the Postgres / MySQL scan paths: stringify
        // every projected column on the server side so the Rust
        // layer only handles `Option<String>`, then feed through
        // per-Arrow-type builders. BigQuery's `CAST(x AS STRING)`
        // is the Standard SQL equivalent of Postgres's `::text`.
        let table_def = self.describe_table(table).await?;
        let arrow_schema = describe_to_arrow_schema("bigquery", &table_def);

        let selected_indices: Vec<usize> =
            projection.unwrap_or_else(|| (0..table_def.columns.len()).collect());

        let projected_schema = if selected_indices.len() == table_def.columns.len() {
            arrow_schema.clone()
        } else {
            arrow_schema
                .project(&selected_indices)
                .map_err(|e| OxError::Runtime {
                    message: format!("bigquery scan: projection error: {e}"),
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
                    "CAST({ident} AS STRING) AS {ident}",
                    ident = quote_ident(&c.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let limit_clause = limit
            .map(|n| format!(" LIMIT {n}"))
            .unwrap_or_default();
        let sql = format!(
            "SELECT {select_list} FROM `{project}.{dataset}.{table}`{limit}",
            project = self.project_id,
            dataset = self.dataset,
            table = table,
            limit = limit_clause,
        );

        let mut rs = self.run_query(&sql).await?;
        let mut builders: Vec<Box<dyn ArrayBuilder>> = projected_schema
            .fields()
            .iter()
            .map(|f| make_builder(f.data_type()))
            .collect();

        while rs.next_row() {
            for (idx, _col) in projected_columns.iter().enumerate() {
                let raw: Option<String> =
                    rs.get_string(idx).map_err(|e| OxError::Runtime {
                        message: format!(
                            "bigquery scan: failed to read column at offset {idx}: {e}"
                        ),
                    })?;
                append_text_cell(
                    "bigquery",
                    builders[idx].as_mut(),
                    projected_schema.field(idx).data_type(),
                    raw.as_deref(),
                )?;
            }
        }

        let arrays: Vec<arrow::array::ArrayRef> =
            builders.into_iter().map(|mut b| b.finish()).collect();
        RecordBatch::try_new(Arc::new(projected_schema), arrays).map_err(|e| OxError::Runtime {
            message: format!("bigquery scan: RecordBatch::try_new failed: {e}"),
        })
    }
}

impl BigQueryAdapter {
    /// Read declared primary-key columns from INFORMATION_SCHEMA for a
    /// single table. Returns an empty vec on constraint-table absence
    /// or any other query failure — a "no declared PK" outcome is
    /// legitimate for BigQuery tables.
    async fn read_primary_key(&self, table: &str) -> OxResult<Vec<String>> {
        let sql = format!(
            "SELECT kcu.column_name \
             FROM `{project}.{dataset}.INFORMATION_SCHEMA.TABLE_CONSTRAINTS` tc \
             JOIN `{project}.{dataset}.INFORMATION_SCHEMA.KEY_COLUMN_USAGE` kcu \
               ON tc.constraint_name = kcu.constraint_name \
             WHERE tc.table_name = {table} \
               AND tc.constraint_type = 'PRIMARY KEY' \
             ORDER BY kcu.ordinal_position",
            project = self.project_id,
            dataset = self.dataset,
            table = quote_literal(table),
        );
        let mut rs = self.run_query(&sql).await?;
        let mut pk = Vec::new();
        while rs.next_row() {
            if let Some(col) = rs.get_string_by_name("column_name").map_err(bq_row_err)? {
                pk.push(col);
            }
        }
        Ok(pk)
    }
}

fn bq_row_err(e: gcp_bigquery_client::error::BQError) -> OxError {
    OxError::Runtime {
        message: format!("BigQuery row extraction failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// URI parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct BigQueryUri {
    project_id: String,
    dataset: String,
    billing_project_id: Option<String>,
    credentials_path: Option<String>,
}

/// Parse `bigquery://PROJECT_ID/DATASET[?billing_project_id=BILLING][&credentials_path=PATH]`.
/// Uses the `url` crate so percent-encoding and query-string parsing
/// come for free.
fn parse_bigquery_uri(uri: &str) -> OxResult<BigQueryUri> {
    let trimmed = uri.trim();

    if !trimmed.starts_with("bigquery://") {
        return Err(OxError::Validation {
            field: "connection_string".to_string(),
            message: format!(
                "BigQuery connection string must start with 'bigquery://'. Got: {trimmed}"
            ),
        });
    }

    let url = url::Url::parse(trimmed).map_err(|e| OxError::Validation {
        field: "connection_string".to_string(),
        message: format!("Invalid BigQuery URI: {e}"),
    })?;

    let project_id = url.host_str().unwrap_or("").to_string();
    if project_id.is_empty() {
        return Err(OxError::Validation {
            field: "connection_string".to_string(),
            message: "BigQuery URI missing project_id (expected bigquery://PROJECT_ID/DATASET)"
                .to_string(),
        });
    }

    let dataset = url.path().trim_start_matches('/').to_string();
    if dataset.is_empty() {
        return Err(OxError::Validation {
            field: "connection_string".to_string(),
            message: "BigQuery URI missing dataset (expected bigquery://PROJECT_ID/DATASET)"
                .to_string(),
        });
    }

    let mut billing_project_id = None;
    let mut credentials_path = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "billing_project_id" => billing_project_id = Some(value.into_owned()),
            "credentials_path" => credentials_path = Some(value.into_owned()),
            other => {
                return Err(OxError::Validation {
                    field: "connection_string".to_string(),
                    message: format!("Unknown BigQuery URI parameter: `{other}`"),
                });
            }
        }
    }

    Ok(BigQueryUri {
        project_id,
        dataset,
        billing_project_id,
        credentials_path,
    })
}

// ---------------------------------------------------------------------------
// Error classification — promote known BigQuery failure modes
// ---------------------------------------------------------------------------

/// Inspect a BigQuery error string and promote recognised patterns
/// into specific [`WarningClass`] variants. Falls back to `default`
/// when nothing matches.
///
/// The match patterns track the stable substring BQ surfaces in each
/// `errors[].message` payload — the API has not rotated these
/// strings since v2 was introduced. Any bigger structural change
/// would surface as a `default` reclassification, not a regression.
fn classify_bigquery_error(
    raw: &str,
    default: ox_ontology::source_analysis::WarningClass,
) -> (
    ox_ontology::source_analysis::WarningClass,
    Vec<(&'static str, String)>,
) {
    use ox_ontology::source_analysis::WarningClass;

    if raw.contains("requires a partition filter")
        || (raw.contains("without a filter over column")
            && raw.contains("partition elimination"))
    {
        let column = extract_quoted_after(raw, "filter over column(s)")
            .or_else(|| extract_quoted_after(raw, "filter over column"));
        let mut params = Vec::new();
        if let Some(col) = column {
            params.push(("partition_column", col));
        }
        return (WarningClass::BigQueryPartitionFilterRequired, params);
    }
    if raw.contains("clustering filter") || raw.contains("filter over clustering column") {
        let column = extract_quoted_after(raw, "clustering column");
        let mut params = Vec::new();
        if let Some(col) = column {
            params.push(("clustering_column", col));
        }
        return (WarningClass::BigQueryClusteringFilterRequired, params);
    }
    if raw.contains("bigquery.jobs.create")
        || raw.contains("does not have bigquery.jobs.create permission")
    {
        return (WarningClass::BigQueryJobsCreateDenied, Vec::new());
    }
    (default, Vec::new())
}

/// Extract the first backtick / single-quote / double-quote enclosed
/// identifier following `marker`. Used to pull column names out of
/// BQ's English error prose ("over column(s) `partition_col`").
/// Returns `None` when the marker is missing or the trailing text
/// has no quoted identifier within ~80 characters.
fn extract_quoted_after(haystack: &str, marker: &str) -> Option<String> {
    let after = haystack.split_once(marker).map(|(_, tail)| tail)?;
    let scan = &after[..after.len().min(160)];
    for (open, close) in [('\'', '\''), ('`', '`'), ('"', '"')] {
        if let Some(start) = scan.find(open) {
            let rest = &scan[start + open.len_utf8()..];
            if let Some(end) = rest.find(close) {
                let quoted = &rest[..end];
                if !quoted.is_empty() {
                    return Some(quoted.to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests — URI parse + identifier validation only.
//
// Live integration tests are out of scope per plan. Any real BigQuery
// query requires a GCP project and credentials that CI doesn't carry.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_uri() {
        let parsed = parse_bigquery_uri("bigquery://my-gcp-project/analytics_prod").unwrap();
        assert_eq!(parsed.project_id, "my-gcp-project");
        assert_eq!(parsed.dataset, "analytics_prod");
        assert!(parsed.billing_project_id.is_none());
        assert!(parsed.credentials_path.is_none());
    }

    #[test]
    fn parse_uri_with_credentials() {
        let parsed =
            parse_bigquery_uri("bigquery://my-project/my_dataset?credentials_path=/etc/sa.json")
                .unwrap();
        assert_eq!(parsed.project_id, "my-project");
        assert_eq!(parsed.dataset, "my_dataset");
        assert_eq!(parsed.credentials_path.as_deref(), Some("/etc/sa.json"));
    }

    #[test]
    fn parse_uri_with_billing_project() {
        let parsed = parse_bigquery_uri(
            "bigquery://oydp-public-dw/dim?billing_project_id=oy-dwusers",
        )
        .unwrap();
        assert_eq!(parsed.project_id, "oydp-public-dw");
        assert_eq!(parsed.dataset, "dim");
        assert_eq!(parsed.billing_project_id.as_deref(), Some("oy-dwusers"));
    }

    #[test]
    fn parse_uri_with_billing_project_and_credentials() {
        let parsed = parse_bigquery_uri(
            "bigquery://data-proj/ds?billing_project_id=billing-proj&credentials_path=/x.json",
        )
        .unwrap();
        assert_eq!(parsed.billing_project_id.as_deref(), Some("billing-proj"));
        assert_eq!(parsed.credentials_path.as_deref(), Some("/x.json"));
    }

    #[test]
    fn parse_uri_rejects_unknown_param() {
        let err = parse_bigquery_uri("bigquery://p/d?bogus=1").unwrap_err();
        assert!(matches!(err, OxError::Validation { .. }));
    }

    #[test]
    fn parse_missing_scheme_is_error() {
        let result = parse_bigquery_uri("my-project/my_dataset");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_dataset_is_error() {
        let result = parse_bigquery_uri("bigquery://my-project/");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_project_is_error() {
        let result = parse_bigquery_uri("bigquery:///my_dataset");
        assert!(result.is_err());
    }

    #[test]
    fn validate_identifier_rejects_injection() {
        assert!(validate_project_or_dataset("project_id", "proj';DROP").is_err());
        assert!(validate_project_or_dataset("dataset", "data.set").is_err());
        assert!(validate_project_or_dataset("project_id", "").is_err());
    }

    #[test]
    fn validate_identifier_accepts_common_shapes() {
        assert!(validate_project_or_dataset("project_id", "my-gcp-project-123").is_ok());
        assert!(validate_project_or_dataset("dataset", "analytics_prod").is_ok());
    }

    #[test]
    fn quote_literal_doubles_single_quotes() {
        assert_eq!(quote_literal("O'Reilly"), "'O''Reilly'");
        assert_eq!(quote_literal("simple"), "'simple'");
    }

    #[test]
    fn quote_ident_doubles_backticks() {
        assert_eq!(quote_ident("plain"), "`plain`");
        assert_eq!(quote_ident("has`tick"), "`has``tick`");
    }

    #[test]
    fn classify_partition_filter_required() {
        use ox_ontology::source_analysis::WarningClass;
        let raw = "BigQuery query failed: Cannot query over table \
                   'oydp-public-dw.dim.dcs_mm_oy_mbr_h' without a filter over column(s) \
                   'stdrd_ym' that can be used for partition elimination";
        let (class, params) = classify_bigquery_error(raw, WarningClass::ColumnSampleSkipped);
        assert!(matches!(class, WarningClass::BigQueryPartitionFilterRequired));
        assert_eq!(
            params,
            vec![("partition_column", "stdrd_ym".to_string())]
        );
    }

    #[test]
    fn classify_jobs_create_denied() {
        use ox_ontology::source_analysis::WarningClass;
        let raw = "BigQuery query failed: Access Denied: User does not have \
                   bigquery.jobs.create permission in project oydp-public-dw";
        let (class, params) = classify_bigquery_error(raw, WarningClass::TableSkipped);
        assert!(matches!(class, WarningClass::BigQueryJobsCreateDenied));
        assert!(params.is_empty());
    }

    #[test]
    fn classify_unknown_falls_back_to_default() {
        use ox_ontology::source_analysis::WarningClass;
        let raw = "Some unrecognised BigQuery payload";
        let (class, params) = classify_bigquery_error(raw, WarningClass::TableSkipped);
        assert!(matches!(class, WarningClass::TableSkipped));
        assert!(params.is_empty());
    }
}
