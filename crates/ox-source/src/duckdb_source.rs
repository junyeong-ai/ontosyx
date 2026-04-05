//! DuckDB data source introspector for local file analysis.
//!
//! Supports Parquet, CSV, and JSON/JSONL files via DuckDB's in-process
//! analytical engine. DuckDB reads files directly using SQL functions
//! (`read_parquet`, `read_csv_auto`, `read_json_auto`), so no external
//! service is needed.
//!
//! Since DuckDB's `Connection` is `!Send`, we avoid storing it in the
//! introspector struct. Instead, we create a fresh in-memory connection
//! per method call (DuckDB in-memory open is very fast) and run the
//! synchronous operations inside `spawn_blocking`.

use std::path::Path;

use async_trait::async_trait;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{
    ColumnStats, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef, TableProfile,
};

use crate::DataSourceIntrospector;

/// Maximum distinct values to collect per column for sample enumeration.
const MAX_SAMPLE_VALUES: usize = 30;

/// Table name used for the single virtual view over the file.
const VIEW_NAME: &str = "data";

/// Supported file extensions and their corresponding DuckDB read functions.
fn read_function_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "parquet" => Some("read_parquet"),
        "csv" | "tsv" => Some("read_csv_auto"),
        "json" | "jsonl" | "ndjson" => Some("read_json_auto"),
        _ => None,
    }
}

/// DuckDB-based introspector for local Parquet, CSV, and JSON files.
///
/// Stores only the file path and detected file type. A fresh in-memory
/// DuckDB connection is opened on each method call to sidestep the
/// `!Send` constraint of `duckdb::Connection`.
#[derive(Debug)]
pub struct DuckDbIntrospector {
    file_path: String,
    read_fn: &'static str,
}

impl DuckDbIntrospector {
    /// Create an introspector for the given local file path.
    ///
    /// Validates that the file exists and has a supported extension.
    /// Does NOT open a DuckDB connection yet.
    pub fn from_file(path: &str) -> OxResult<Self> {
        // Validate file exists
        if !Path::new(path).exists() {
            return Err(OxError::Validation {
                field: "file_path".into(),
                message: format!("File does not exist: {path}"),
            });
        }

        // Detect file type from extension
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let read_fn = read_function_for_ext(&ext).ok_or_else(|| OxError::Validation {
            field: "file_path".into(),
            message: format!(
                "Unsupported file type: '.{ext}'. Supported: parquet, csv, tsv, json, jsonl, ndjson"
            ),
        })?;

        info!(path = %path, read_fn = %read_fn, "DuckDB introspector created for local file");

        Ok(Self {
            file_path: path.to_string(),
            read_fn,
        })
    }

    /// Open a fresh in-memory DuckDB connection and create a view over the file.
    fn open_connection(&self) -> OxResult<duckdb::Connection> {
        let conn = duckdb::Connection::open_in_memory().map_err(|e| OxError::Runtime {
            message: format!("DuckDB init failed: {e}"),
        })?;

        // Escape single quotes in file path for SQL safety
        let escaped_path = self.file_path.replace('\'', "''");
        let create_view = format!(
            "CREATE VIEW {VIEW_NAME} AS SELECT * FROM {read_fn}('{escaped_path}')",
            read_fn = self.read_fn,
        );

        conn.execute(&create_view, [])
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to read file '{}': {e}", self.file_path),
            })?;

        Ok(conn)
    }
}

/// Map a DuckDB type string to a simplified type name for downstream use.
///
/// DuckDB reports types like "BIGINT", "VARCHAR", "DOUBLE", "BOOLEAN", etc.
/// We normalize to lowercase for consistency with other source introspectors.
fn normalize_duckdb_type(raw: &str) -> String {
    raw.to_lowercase()
}

/// Quote a DuckDB identifier (column name) safely.
/// DuckDB uses double quotes for identifiers.
fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Introspect schema synchronously using the given connection.
fn introspect_schema_sync(conn: &duckdb::Connection) -> OxResult<SourceSchema> {
    // DESCRIBE returns: column_name, column_type, null, key, default, extra
    let mut stmt = conn
        .prepare(&format!("DESCRIBE SELECT * FROM {VIEW_NAME}"))
        .map_err(|e| OxError::Runtime {
            message: format!("DuckDB DESCRIBE failed: {e}"),
        })?;

    let columns: Vec<SourceColumnDef> = stmt
        .query_map([], |row| {
            let name: String = row.get(0)?;
            let data_type: String = row.get(1)?;
            let nullable: String = row.get(2)?;
            Ok(SourceColumnDef {
                name,
                data_type: normalize_duckdb_type(&data_type),
                nullable: nullable.eq_ignore_ascii_case("YES"),
            })
        })
        .map_err(|e| OxError::Runtime {
            message: format!("DuckDB schema query failed: {e}"),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| OxError::Runtime {
            message: format!("DuckDB row read failed: {e}"),
        })?;

    if columns.is_empty() {
        return Err(OxError::Runtime {
            message: "DuckDB file contains no columns".to_string(),
        });
    }

    Ok(SourceSchema {
        source_type: "duckdb".to_string(),
        tables: vec![SourceTableDef {
            name: VIEW_NAME.to_string(),
            columns,
            primary_key: Vec::new(), // Files have no declared primary key
        }],
        foreign_keys: Vec::new(), // Single-file source — no foreign keys
    })
}

/// Collect statistics synchronously using the given connection.
fn collect_stats_sync(conn: &duckdb::Connection, schema: &SourceSchema) -> OxResult<SourceProfile> {
    let table = schema.tables.first().ok_or_else(|| OxError::Runtime {
        message: "No tables in schema to profile".to_string(),
    })?;

    // Row count
    let row_count: u64 = conn
        .query_row(&format!("SELECT count(*) FROM {VIEW_NAME}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|n| n.max(0) as u64)
        .map_err(|e| OxError::Runtime {
            message: format!("DuckDB count query failed: {e}"),
        })?;

    let mut column_stats = Vec::with_capacity(table.columns.len());

    for col in &table.columns {
        let qc = quote_ident(&col.name);

        // Combined stats: null count, distinct count, min, max
        let stats_query = format!(
            "SELECT \
                count(*) FILTER (WHERE {qc} IS NULL), \
                count(DISTINCT {qc}), \
                min({qc}::VARCHAR), \
                max({qc}::VARCHAR) \
             FROM {VIEW_NAME}",
        );

        let (null_count, distinct_count, min_value, max_value): (
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(&stats_query, [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|e| OxError::Runtime {
                message: format!("DuckDB stats query failed for '{}': {e}", col.name),
            })?;

        // Collect sample values for low-cardinality columns
        let sample_values = if distinct_count > 0 && (distinct_count as usize) <= MAX_SAMPLE_VALUES
        {
            let sample_query = format!(
                "SELECT DISTINCT {qc}::VARCHAR AS val \
                 FROM {VIEW_NAME} \
                 WHERE {qc} IS NOT NULL \
                 ORDER BY val \
                 LIMIT {MAX_SAMPLE_VALUES}",
            );
            let mut stmt = conn.prepare(&sample_query).map_err(|e| OxError::Runtime {
                message: format!("DuckDB sample query prepare failed for '{}': {e}", col.name),
            })?;
            stmt.query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| OxError::Runtime {
                    message: format!("DuckDB sample query failed for '{}': {e}", col.name),
                })?
                .filter_map(|r| r.ok())
                .collect()
        } else {
            Vec::new()
        };

        column_stats.push(ColumnStats {
            column_name: col.name.clone(),
            null_count: null_count.max(0) as u64,
            distinct_count: distinct_count.max(0) as u64,
            sample_values,
            min_value,
            max_value,
        });
    }

    Ok(SourceProfile {
        table_profiles: vec![TableProfile {
            table_name: VIEW_NAME.to_string(),
            row_count,
            column_stats,
        }],
    })
}

#[async_trait]
impl DataSourceIntrospector for DuckDbIntrospector {
    fn source_type(&self) -> &str {
        "duckdb"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        let file_path = self.file_path.clone();
        let read_fn = self.read_fn;

        tokio::task::spawn_blocking(move || {
            let introspector = DuckDbIntrospector { file_path, read_fn };
            let conn = introspector.open_connection()?;
            introspect_schema_sync(&conn)
        })
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("DuckDB introspection task panicked: {e}"),
        })?
    }

    async fn collect_stats(&self, schema: &SourceSchema) -> OxResult<SourceProfile> {
        let file_path = self.file_path.clone();
        let read_fn = self.read_fn;
        let schema = schema.clone();

        tokio::task::spawn_blocking(move || {
            let introspector = DuckDbIntrospector { file_path, read_fn };
            let conn = introspector.open_connection()?;
            collect_stats_sync(&conn, &schema)
        })
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("DuckDB stats collection task panicked: {e}"),
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn from_file_rejects_unsupported_extension() {
        // Create a real file with an unsupported extension
        let dir = tempfile::tempdir().unwrap();
        let xlsx_path = dir.path().join("data.xlsx");
        std::fs::write(&xlsx_path, "dummy").unwrap();

        let result = DuckDbIntrospector::from_file(xlsx_path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unsupported file type"), "got: {err}");
    }

    #[test]
    fn from_file_rejects_nonexistent_file() {
        let result = DuckDbIntrospector::from_file("/tmp/nonexistent_file_abc123.parquet");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not exist"), "got: {err}");
    }

    #[test]
    fn read_function_mapping() {
        assert_eq!(read_function_for_ext("parquet"), Some("read_parquet"));
        assert_eq!(read_function_for_ext("csv"), Some("read_csv_auto"));
        assert_eq!(read_function_for_ext("tsv"), Some("read_csv_auto"));
        assert_eq!(read_function_for_ext("json"), Some("read_json_auto"));
        assert_eq!(read_function_for_ext("jsonl"), Some("read_json_auto"));
        assert_eq!(read_function_for_ext("ndjson"), Some("read_json_auto"));
        assert_eq!(read_function_for_ext("xlsx"), None);
    }

    #[tokio::test]
    async fn introspect_csv_file() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");
        {
            let mut f = std::fs::File::create(&csv_path).unwrap();
            writeln!(f, "id,name,score").unwrap();
            writeln!(f, "1,Alice,95.5").unwrap();
            writeln!(f, "2,Bob,87.0").unwrap();
            writeln!(f, "3,Charlie,92.3").unwrap();
        }

        let introspector = DuckDbIntrospector::from_file(csv_path.to_str().unwrap()).unwrap();
        assert_eq!(introspector.source_type(), "duckdb");

        let schema = introspector.introspect_schema().await.unwrap();
        assert_eq!(schema.source_type, "duckdb");
        assert_eq!(schema.tables.len(), 1);
        assert_eq!(schema.tables[0].name, VIEW_NAME);
        assert_eq!(schema.tables[0].columns.len(), 3);

        let col_names: Vec<&str> = schema.tables[0]
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(col_names, vec!["id", "name", "score"]);

        // Collect stats
        let profile = introspector.collect_stats(&schema).await.unwrap();
        assert_eq!(profile.table_profiles.len(), 1);
        assert_eq!(profile.table_profiles[0].row_count, 3);
        assert_eq!(profile.table_profiles[0].column_stats.len(), 3);

        // Name column should have 3 distinct values with samples
        let name_stats = &profile.table_profiles[0].column_stats[1];
        assert_eq!(name_stats.column_name, "name");
        assert_eq!(name_stats.distinct_count, 3);
        assert_eq!(name_stats.null_count, 0);
        assert!(!name_stats.sample_values.is_empty());
    }

    #[tokio::test]
    async fn introspect_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let json_path = dir.path().join("test.json");
        std::fs::write(
            &json_path,
            r#"[{"id":1,"city":"Seoul"},{"id":2,"city":"Busan"}]"#,
        )
        .unwrap();

        let introspector = DuckDbIntrospector::from_file(json_path.to_str().unwrap()).unwrap();

        let schema = introspector.introspect_schema().await.unwrap();
        assert_eq!(schema.tables[0].columns.len(), 2);

        let profile = introspector.collect_stats(&schema).await.unwrap();
        assert_eq!(profile.table_profiles[0].row_count, 2);
    }

    #[tokio::test]
    async fn introspect_jsonl_file() {
        let dir = tempfile::tempdir().unwrap();
        let jsonl_path = dir.path().join("test.jsonl");
        std::fs::write(
            &jsonl_path,
            "{\"x\":1,\"y\":\"a\"}\n{\"x\":2,\"y\":\"b\"}\n",
        )
        .unwrap();

        let introspector = DuckDbIntrospector::from_file(jsonl_path.to_str().unwrap()).unwrap();

        let schema = introspector.introspect_schema().await.unwrap();
        assert_eq!(schema.tables[0].columns.len(), 2);

        let profile = introspector.collect_stats(&schema).await.unwrap();
        assert_eq!(profile.table_profiles[0].row_count, 2);
    }

    #[tokio::test]
    async fn full_analysis_via_trait() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("analysis.csv");
        {
            let mut f = std::fs::File::create(&csv_path).unwrap();
            writeln!(f, "status,count").unwrap();
            writeln!(f, "active,10").unwrap();
            writeln!(f, "inactive,5").unwrap();
        }

        let introspector = DuckDbIntrospector::from_file(csv_path.to_str().unwrap()).unwrap();

        // Use the default analyze() from the trait
        let result = introspector.analyze().await.unwrap();
        assert_eq!(result.schema.tables.len(), 1);
        assert_eq!(result.profile.table_profiles[0].row_count, 2);
        assert!(result.warnings.is_empty());
    }
}
