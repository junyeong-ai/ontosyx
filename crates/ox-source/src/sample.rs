use std::collections::{BTreeMap, BTreeSet, HashSet};

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef,
    TableProfile,
};
use serde_json::Value;

const INLINE_TABLE_NAME: &str = "records";
const MAX_DISTINCT_VALUES: usize = 30;

/// Upper bound on the size of an in-memory CSV / JSON payload the
/// analyzer will accept. Both `analyze_csv` and `analyze_json` materialise
/// the full input into memory (the CSV reader collects every row into
/// `Vec<Row>`; `serde_json::from_str` parses the whole tree). Without a
/// cap a 1 GiB CSV would OOM the process before a single schema row
/// was produced. 100 MiB covers every realistic inline / file-upload
/// workload and still rejects denial-of-service payloads eagerly.
///
/// Large data sets belong in a DataSourceAdapter backed by a real DB
/// (DuckDB, PostgreSQL) which scans lazily. When this limit is hit the
/// error points the caller at that path explicitly.
pub const MAX_INLINE_SOURCE_BYTES: usize = 100 * 1024 * 1024;

fn guard_payload_size(source_type: &str, data: &str) -> OxResult<()> {
    if data.len() > MAX_INLINE_SOURCE_BYTES {
        return Err(OxError::Validation {
            field: "source.data".to_string(),
            message: format!(
                "{source_type} payload is {} bytes; the inline analyzer caps at {} bytes. \
                 Load large files through a DuckDB / PostgreSQL adapter instead of inline.",
                data.len(),
                MAX_INLINE_SOURCE_BYTES,
            ),
        });
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Cell {
    raw: Option<String>,
    data_type: &'static str,
}

type Row = BTreeMap<String, Cell>;

pub fn analyze_csv(data: &str) -> OxResult<(SourceSchema, SourceProfile)> {
    guard_payload_size("csv", data)?;

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(data.as_bytes());

    let headers = reader
        .headers()
        .map_err(|e| OxError::Validation {
            field: "source.data".to_string(),
            message: format!("Invalid CSV headers: {e}"),
        })?
        .iter()
        .map(|header| header.trim().to_string())
        .collect::<Vec<_>>();

    if headers.is_empty() {
        return Err(OxError::Validation {
            field: "source.data".to_string(),
            message: "CSV source must contain a header row".to_string(),
        });
    }

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| OxError::Validation {
            field: "source.data".to_string(),
            message: format!("Invalid CSV record: {e}"),
        })?;

        let row = headers
            .iter()
            .enumerate()
            .map(|(index, header)| {
                let raw = record.get(index).map(str::trim).unwrap_or_default();
                (header.clone(), cell_from_str(raw))
            })
            .collect();
        rows.push(row);
    }

    build_schema_profile("csv", INLINE_TABLE_NAME, &headers, &rows)
}

pub fn analyze_json(data: &str) -> OxResult<(SourceSchema, SourceProfile)> {
    guard_payload_size("json", data)?;

    let value: Value = serde_json::from_str(data).map_err(|e| OxError::Validation {
        field: "source.data".to_string(),
        message: format!("Invalid JSON source: {e}"),
    })?;

    let items = match value {
        Value::Array(items) => items,
        Value::Object(_) => vec![value],
        scalar => vec![Value::Object(serde_json::Map::from_iter([(
            "value".to_string(),
            scalar,
        )]))],
    };

    let mut tables = Vec::new();
    let mut profiles = Vec::new();
    let mut foreign_keys = Vec::new();

    extract_json_tables(
        INLINE_TABLE_NAME,
        &items,
        &mut tables,
        &mut profiles,
        &mut foreign_keys,
    );

    if tables.is_empty() {
        return Err(OxError::Validation {
            field: "source.data".to_string(),
            message: "JSON source produced no analyzable structure".to_string(),
        });
    }

    Ok((
        SourceSchema {
            source_type: "json".to_string(),
            tables,
            foreign_keys,
        },
        SourceProfile {
            table_profiles: profiles,
        },
    ))
}

/// Recursively extract tables from JSON objects.
/// Nested objects become `{parent}_{field}` tables with an FK back to parent.
/// Arrays of objects become `{field}` tables with an FK back to parent.
/// Scalar arrays and mixed arrays remain as opaque JSON columns.
fn extract_json_tables(
    table_name: &str,
    items: &[Value],
    tables: &mut Vec<SourceTableDef>,
    profiles: &mut Vec<TableProfile>,
    foreign_keys: &mut Vec<ForeignKeyDef>,
) {
    let mut columns = Vec::new();
    let mut seen_columns = HashSet::new();
    let mut rows = Vec::with_capacity(items.len());

    // Collect nested fields that should be extracted as child tables
    // Key: field name, Value: collected child items across all rows
    let mut nested_objects: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut nested_arrays: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for item in items {
        let map = match item {
            Value::Object(map) => map,
            _ => {
                // Non-object items in the array — flatten to scalar row
                let mut row = BTreeMap::new();
                row.insert("value".to_string(), cell_from_json_value(item));
                if seen_columns.insert("value".to_string()) {
                    columns.push("value".to_string());
                }
                rows.push(row);
                continue;
            }
        };

        let mut row = BTreeMap::new();
        for (key, value) in map {
            match value {
                // Nested object → extract as child table
                Value::Object(_) => {
                    nested_objects
                        .entry(key.clone())
                        .or_default()
                        .push(value.clone());
                }
                // Array of objects → extract as child table
                Value::Array(arr) if arr.iter().any(|v| v.is_object()) => {
                    let child_items: Vec<Value> = arr.clone();
                    nested_arrays
                        .entry(key.clone())
                        .or_default()
                        .extend(child_items);
                }
                // Scalar array or empty array → keep as JSON column
                _ => {
                    row.insert(key.clone(), cell_from_json_value(value));
                    if seen_columns.insert(key.clone()) {
                        columns.push(key.clone());
                    }
                }
            }
        }
        rows.push(row);
    }

    if columns.is_empty() && nested_objects.is_empty() && nested_arrays.is_empty() {
        columns.push("value".to_string());
    }

    // Build this table's schema and profile
    if !columns.is_empty() || rows.is_empty() {
        let mut column_defs = Vec::with_capacity(columns.len());
        let mut column_stats = Vec::with_capacity(columns.len());

        for column in &columns {
            let values: Vec<Cell> = rows
                .iter()
                .map(|row| {
                    row.get(column).cloned().unwrap_or(Cell {
                        raw: None,
                        data_type: "null",
                    })
                })
                .collect();

            let nullable = values.iter().any(|cell| cell.raw.is_none());
            let data_type = infer_column_type(&values).to_string();
            let stat = build_column_stats(column, &values);

            column_defs.push(SourceColumnDef {
                name: column.clone(),
                data_type,
                nullable,
            });
            column_stats.push(stat);
        }

        let primary_key = columns
            .iter()
            .filter(|col| col.eq_ignore_ascii_case("id"))
            .find(|col| is_unique_non_null(col, &rows))
            .cloned()
            .into_iter()
            .collect::<Vec<_>>();

        tables.push(SourceTableDef {
            name: table_name.to_string(),
            columns: column_defs,
            primary_key,
        });
        profiles.push(TableProfile {
            table_name: table_name.to_string(),
            row_count: rows.len() as u64,
            column_stats,
        });
    }

    // Only emit FK relationships if the parent table has a PK column to reference.
    let parent_pk_column = tables
        .iter()
        .find(|t| t.name == table_name)
        .and_then(|t| t.primary_key.first().cloned());

    // Recursively extract nested object tables.
    // FK relationships are inferred from the JSON nesting structure.
    // Unlike DB sources, child tables do NOT get a synthetic FK column — the relationship
    // is expressed only via ForeignKeyDef so the schema stays faithful to the source data.
    for (field, child_items) in &nested_objects {
        let child_table = format!("{table_name}_{field}");
        extract_json_tables(&child_table, child_items, tables, profiles, foreign_keys);

        if let Some(pk_col) = &parent_pk_column {
            foreign_keys.push(ForeignKeyDef {
                from_table: child_table,
                from_column: format!("(nested in {field})"),
                to_table: table_name.to_string(),
                to_column: pk_col.clone(),
                inferred: true,
            });
        }
    }

    // Recursively extract nested array-of-objects tables (namespaced to avoid collisions)
    for (field, child_items) in &nested_arrays {
        let child_table = format!("{table_name}_{field}");
        extract_json_tables(&child_table, child_items, tables, profiles, foreign_keys);

        if let Some(pk_col) = &parent_pk_column {
            foreign_keys.push(ForeignKeyDef {
                from_table: child_table,
                from_column: format!("(nested in {field})"),
                to_table: table_name.to_string(),
                to_column: pk_col.clone(),
                inferred: true,
            });
        }
    }
}

fn build_schema_profile(
    source_type: &str,
    table_name: &str,
    columns: &[String],
    rows: &[Row],
) -> OxResult<(SourceSchema, SourceProfile)> {
    if columns.is_empty() {
        return Err(OxError::Validation {
            field: "source.data".to_string(),
            message: "Structured source must contain at least one column".to_string(),
        });
    }

    let mut column_defs = Vec::with_capacity(columns.len());
    let mut column_stats = Vec::with_capacity(columns.len());

    for column in columns {
        let values = rows
            .iter()
            .map(|row| {
                row.get(column).cloned().unwrap_or(Cell {
                    raw: None,
                    data_type: "null",
                })
            })
            .collect::<Vec<_>>();

        let nullable = values.iter().any(|cell| cell.raw.is_none());
        let data_type = infer_column_type(&values).to_string();
        let stat = build_column_stats(column, &values);

        column_defs.push(SourceColumnDef {
            name: column.clone(),
            data_type,
            nullable,
        });
        column_stats.push(stat);
    }

    let primary_key = columns
        .iter()
        .filter(|column| column.eq_ignore_ascii_case("id"))
        .find(|column| is_unique_non_null(column, rows))
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();

    let schema = SourceSchema {
        source_type: source_type.to_string(),
        tables: vec![SourceTableDef {
            name: table_name.to_string(),
            columns: column_defs,
            primary_key,
        }],
        foreign_keys: Vec::new(),
    };

    let profile = SourceProfile {
        table_profiles: vec![TableProfile {
            table_name: table_name.to_string(),
            row_count: rows.len() as u64,
            column_stats,
        }],
    };

    Ok((schema, profile))
}

fn is_unique_non_null(column: &str, rows: &[Row]) -> bool {
    if rows.is_empty() {
        return false;
    }

    let mut seen = HashSet::new();
    for row in rows {
        let Some(value) = row.get(column).and_then(|cell| cell.raw.as_ref()) else {
            return false;
        };

        if !seen.insert(value.clone()) {
            return false;
        }
    }

    true
}

fn build_column_stats(column: &str, values: &[Cell]) -> ColumnStats {
    let mut distinct = HashSet::new();
    let mut sample_values = Vec::new();
    let mut sample_seen = HashSet::new();
    let mut ordered_values = BTreeSet::new();
    let mut null_count = 0u64;

    for cell in values {
        match &cell.raw {
            Some(value) => {
                distinct.insert(value.clone());
                ordered_values.insert(value.clone());

                if sample_seen.insert(value.clone()) && sample_values.len() < MAX_DISTINCT_VALUES {
                    sample_values.push(value.clone());
                }
            }
            None => null_count += 1,
        }
    }

    let min_value = ordered_values.first().cloned();
    let max_value = ordered_values.last().cloned();

    ColumnStats {
        column_name: column.to_string(),
        null_count,
        distinct_count: distinct.len() as u64,
        sample_values,
        min_value,
        max_value,
    }
}

fn infer_column_type(values: &[Cell]) -> &'static str {
    let mut detected = "null";
    for cell in values {
        if cell.raw.is_none() {
            continue;
        }
        detected = merge_types(detected, cell.data_type);
    }

    if detected == "null" {
        "string"
    } else {
        detected
    }
}

fn merge_types(left: &'static str, right: &'static str) -> &'static str {
    match (left, right) {
        ("null", other) | (other, "null") => other,
        ("int", "int") => "int",
        ("int", "float") | ("float", "int") | ("float", "float") => "float",
        ("bool", "bool") => "bool",
        ("json", _) | (_, "json") => "json",
        _ => "string",
    }
}

fn cell_from_str(raw: &str) -> Cell {
    if raw.is_empty() {
        return Cell {
            raw: None,
            data_type: "null",
        };
    }

    Cell {
        raw: Some(raw.to_string()),
        data_type: infer_scalar_type(raw),
    }
}

fn cell_from_json_value(value: &Value) -> Cell {
    match value {
        Value::Null => Cell {
            raw: None,
            data_type: "null",
        },
        Value::Bool(v) => Cell {
            raw: Some(v.to_string()),
            data_type: "bool",
        },
        Value::Number(v) => Cell {
            raw: Some(v.to_string()),
            data_type: if v.is_i64() || v.is_u64() {
                "int"
            } else {
                "float"
            },
        },
        Value::String(v) => Cell {
            raw: Some(v.clone()),
            data_type: infer_scalar_type(v),
        },
        Value::Array(_) | Value::Object(_) => Cell {
            raw: Some(value.to_string()),
            data_type: "json",
        },
    }
}

fn infer_scalar_type(raw: &str) -> &'static str {
    let lower = raw.to_ascii_lowercase();
    if matches!(lower.as_str(), "true" | "false") {
        "bool"
    } else if raw.parse::<i64>().is_ok() {
        "int"
    } else if raw.parse::<f64>().is_ok() {
        "float"
    } else {
        "string"
    }
}

// ---------------------------------------------------------------------------
// DataSourceAdapter wrappers for CSV and JSON
// ---------------------------------------------------------------------------
//
// CSV and JSON sources are fully in-memory — `analyze_csv` / `analyze_json`
// compute the entire schema + profile eagerly at construction. The adapter
// primitives are then plain lookups against those pre-computed structures,
// so every method is O(1) w.r.t. I/O and returns cloned snapshots.

use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::RecordBatch;
use arrow_array::{ArrayRef, builder::ArrayBuilder};
use arrow_schema::Schema;
use async_trait::async_trait;

use crate::DataSourceAdapter;
use crate::json_scan::append_json_cell;
use crate::normalize::describe_to_arrow_schema;
use crate::text_scan::{append_text_cell, make_builder};

/// A [`DataSourceAdapter`] backed by in-memory CSV data.
///
/// The raw CSV string is held in an `Arc<str>` so cheap `clone`s keep
/// `scan()` re-parsable without mutating the adapter. This costs one
/// 1× payload copy at construction (bounded by `MAX_INLINE_SOURCE_BYTES`)
/// in exchange for a scan path that stays faithful to the analyzer
/// pipeline — value parsing goes through the same `infer_scalar_type`
/// choices the schema was built from, so column types in a scanned
/// `RecordBatch` line up with the Arrow schema reported by
/// [`crate::normalize::describe_to_arrow_schema`].
pub struct CsvAdapter {
    raw_data: Arc<str>,
    schema: SourceSchema,
    stats_by_column: HashMap<(String, String), ColumnStats>,
    counts_by_table: HashMap<String, u64>,
}

impl CsvAdapter {
    /// Build an adapter from an inline CSV payload.
    ///
    /// Accepts any `Into<Arc<str>>` so callers can pass a borrowed
    /// `&str`, an owned `String`, or an already-shared `Arc<str>`
    /// without the ctor choosing for them. The latter two
    /// conversions are zero-copy (`Arc::from(String)` reuses the
    /// String's allocation; `Arc<str> → Arc<str>` is a refcount
    /// bump). The `&str` branch allocates once — unavoidable to
    /// take ownership.
    ///
    /// This is load-bearing for the federation inline-CSV flow: the
    /// `Credential::Inline { value: Arc<str> }` produced by the
    /// admin handler is handed directly to this ctor, so a 100 MiB
    /// payload crosses the boundary as a refcount bump, not a
    /// memcpy.
    pub fn new(data: impl Into<Arc<str>>) -> OxResult<Self> {
        let raw_data: Arc<str> = data.into();
        let (schema, profile) = analyze_csv(&raw_data)?;
        let (stats_by_column, counts_by_table) = index_profile(&profile);
        Ok(Self {
            raw_data,
            schema,
            stats_by_column,
            counts_by_table,
        })
    }
}

/// A [`DataSourceAdapter`] backed by in-memory JSON data.
pub struct JsonAdapter {
    /// Raw JSON payload — kept around so `scan()` can rematerialise
    /// the top-level table into Arrow. Held behind an `Arc<str>` so
    /// per-scan cloning is cheap and the value is immutably shared
    /// across async tasks (DataSourceAdapter is `Send + Sync`).
    raw_data: Arc<str>,
    schema: SourceSchema,
    stats_by_column: HashMap<(String, String), ColumnStats>,
    counts_by_table: HashMap<String, u64>,
}

impl JsonAdapter {
    /// Build an adapter from an inline JSON payload. See
    /// [`CsvAdapter::new`] for the `impl Into<Arc<str>>` rationale;
    /// semantics are identical.
    pub fn new(data: impl Into<Arc<str>>) -> OxResult<Self> {
        let raw_data: Arc<str> = data.into();
        let (schema, profile) = analyze_json(&raw_data)?;
        let (stats_by_column, counts_by_table) = index_profile(&profile);
        Ok(Self {
            raw_data,
            schema,
            stats_by_column,
            counts_by_table,
        })
    }
}

#[async_trait]
impl DataSourceAdapter for CsvAdapter {
    fn source_type(&self) -> &str {
        "csv"
    }
    async fn list_tables(&self) -> OxResult<Vec<String>> {
        Ok(list_tables_from_schema(&self.schema))
    }
    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
        describe_from_schema(&self.schema, table)
    }
    async fn count_rows(&self, table: &str) -> OxResult<u64> {
        Ok(self.counts_by_table.get(table).copied().unwrap_or_default())
    }
    async fn sample_column(&self, table: &str, column: &SourceColumnDef) -> OxResult<ColumnStats> {
        Ok(self
            .stats_by_column
            .get(&(table.to_string(), column.name.clone()))
            .cloned()
            .unwrap_or_else(|| empty_stats(&column.name)))
    }
    async fn list_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
        Ok(self.schema.foreign_keys.clone())
    }

    async fn scan(
        &self,
        table: &str,
        projection: Option<Vec<usize>>,
        limit: Option<usize>,
    ) -> OxResult<RecordBatch> {
        let table_def = describe_from_schema(&self.schema, table)?;
        let raw = Arc::clone(&self.raw_data);
        let arrow_schema = describe_to_arrow_schema("csv", &table_def);
        scan_csv_into_batch(&raw, &table_def, &arrow_schema, projection, limit)
    }
}

#[async_trait]
impl DataSourceAdapter for JsonAdapter {
    fn source_type(&self) -> &str {
        "json"
    }
    async fn list_tables(&self) -> OxResult<Vec<String>> {
        Ok(list_tables_from_schema(&self.schema))
    }
    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef> {
        describe_from_schema(&self.schema, table)
    }
    async fn count_rows(&self, table: &str) -> OxResult<u64> {
        Ok(self.counts_by_table.get(table).copied().unwrap_or_default())
    }
    async fn sample_column(&self, table: &str, column: &SourceColumnDef) -> OxResult<ColumnStats> {
        Ok(self
            .stats_by_column
            .get(&(table.to_string(), column.name.clone()))
            .cloned()
            .unwrap_or_else(|| empty_stats(&column.name)))
    }
    async fn list_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
        Ok(self.schema.foreign_keys.clone())
    }

    async fn scan(
        &self,
        table: &str,
        projection: Option<Vec<usize>>,
        limit: Option<usize>,
    ) -> OxResult<RecordBatch> {
        let table_def = describe_from_schema(&self.schema, table)?;
        let arrow_schema = describe_to_arrow_schema("json", &table_def);
        let items = self.resolve_items_for_table(table)?;
        scan_json_items_into_batch(&items, &table_def, &arrow_schema, projection, limit)
    }
}

impl JsonAdapter {
    /// Return the slice of JSON items that make up `table`'s rows.
    ///
    /// Recognised relations map to items as follows:
    ///
    /// - [`INLINE_TABLE_NAME`] (`"records"`) — the payload itself is
    ///   an array of objects, an object (wrapped to a single row),
    ///   or a scalar (wrapped into a single `{value: scalar}`
    ///   object).
    /// - `records_<a>` — a top-level array-of-objects field.
    /// - `records_<a>_<b>...` — a nested array-of-objects reached by
    ///   walking the parent chain (tracked via
    ///   `schema.foreign_keys`) top-down and flattening each hop.
    ///
    /// The parent-chain walk is how we disambiguate field names that
    /// happen to contain `_`: `records_user_id` is whatever
    /// `analyze_json` emitted when it saw a top-level `user_id`
    /// array, while `records_user_id_when_id_is_nested` would have
    /// distinct FK entries linking it to a chain. `_`-in-name
    /// collisions never reach scan because the profiler is the
    /// single source of truth for both naming and the FK chain.
    fn resolve_items_for_table(&self, table: &str) -> OxResult<Vec<Value>> {
        let value: Value =
            serde_json::from_str(&self.raw_data).map_err(|e| OxError::Validation {
                field: "source.data".to_string(),
                message: format!("Invalid JSON source: {e}"),
            })?;

        if table == INLINE_TABLE_NAME {
            return Ok(match value {
                Value::Array(items) => items,
                Value::Object(_) => vec![value],
                scalar => vec![Value::Object(serde_json::Map::from_iter([(
                    "value".to_string(),
                    scalar,
                )]))],
            });
        }

        // Reject unknown tables up-front. Downstream walk assumes
        // every name in the input appears in the schema.
        if !self.schema.tables.iter().any(|t| t.name == table) {
            return Err(OxError::NotFound {
                entity: format!("table `{table}`"),
            });
        }

        let path = self.parent_chain_fields(table);
        if path.is_empty() {
            return Err(OxError::UnsupportedOperation {
                target: "json".into(),
                operation: format!(
                    "scan(table={table}) — the profiler did not link this \
                     relation to any parent, so there is no way to walk the \
                     JSON payload for its rows"
                ),
            });
        }

        Ok(walk_json_path(&value, &path))
    }

    /// Build the parent-first field path from `table` up to
    /// `records`. At each step we find the **longest** known
    /// sibling table name that is a proper prefix of the current
    /// table — that table is the parent, and the remainder after
    /// its `_` delimiter is the field JSON uses to nest it.
    ///
    /// Walking against `schema.tables` (not `foreign_keys`)
    /// side-steps two issues that come up with the profiler's
    /// naming scheme:
    ///
    /// 1. `analyze_json` only emits a `ForeignKeyDef` when the
    ///    parent table has a primary-key column. A nested array
    ///    under a PK-less parent leaves the chain unlinked on the
    ///    FK side; walking `tables` doesn't care.
    /// 2. Field names can contain `_` (`user_addresses`). A naïve
    ///    `_`-split heuristic would pick `user` as a parent even
    ///    when no such table exists. The longest-known-prefix rule
    ///    picks the right parent deterministically because the
    ///    profiler already decided what is and isn't a separate
    ///    table.
    fn parent_chain_fields(&self, table: &str) -> Vec<String> {
        let mut path: Vec<String> = Vec::new();
        let mut current = table.to_string();
        loop {
            if current == INLINE_TABLE_NAME {
                break;
            }
            let Some(parent) = self.resolve_parent(&current) else {
                break;
            };
            let prefix = format!("{parent}_");
            let field = current
                .strip_prefix(&prefix)
                .unwrap_or(current.as_str())
                .to_string();
            path.push(field);
            current = parent;
        }
        path.reverse();
        path
    }

    /// Resolve the immediate parent of `current` in the profiler's
    /// naming scheme. Returns `None` when no parent can be found —
    /// that's the terminal condition for the walker.
    ///
    /// Two candidate sources, in priority order:
    ///
    /// 1. The schema table list itself. The longest known sibling
    ///    that is a proper `{name}_` prefix of `current` wins.
    ///    `current` is excluded from its own candidate set.
    /// 2. [`INLINE_TABLE_NAME`] as an implicit root. When the
    ///    top-level payload has only nested arrays / objects, the
    ///    profiler does not emit a `records` entry in
    ///    `schema.tables` — the child tables still carry the
    ///    `records_` prefix, so we fall back to the constant.
    ///
    /// A schema match, when present, is always at least as long as
    /// the implicit-root candidate (any schema match that competes
    /// must already start with `records_`), so `schema_match` wins
    /// any tie — the single `.or` below is exhaustive.
    ///
    /// Prefix checks use byte-level comparison rather than
    /// `format!("{name}_")` to avoid a per-iteration String
    /// allocation — for a wide schema this is O(N) allocations
    /// per walk step, which sample analysis hits on every JSON
    /// scan.
    fn resolve_parent(&self, current: &str) -> Option<String> {
        let schema_match = self
            .schema
            .tables
            .iter()
            .map(|t| t.name.as_str())
            .filter(|name| *name != current)
            .filter(|name| is_underscore_prefix(current, name))
            .max_by_key(|n| n.len())
            .map(|s| s.to_string());
        let implicit_root = is_underscore_prefix(current, INLINE_TABLE_NAME)
            .then(|| INLINE_TABLE_NAME.to_string());
        schema_match.or(implicit_root)
    }
}

/// Returns true iff `current` is `prefix` followed by an underscore
/// and then at least one more character — i.e. `prefix` is the
/// parent table name and `current` is one of its descendants under
/// the `{parent}_{field}` naming scheme.
///
/// Free-function rather than a `str` method so it's reusable and so
/// both walker candidates run the same check.
fn is_underscore_prefix(current: &str, prefix: &str) -> bool {
    current.len() > prefix.len()
        && current.as_bytes().get(prefix.len()) == Some(&b'_')
        && current.starts_with(prefix)
}

/// Walk a parsed JSON payload using the parent-first field path.
/// Each field takes the current pool of values (objects or a
/// singleton at the root), looks up the field on each, and flattens
/// any array-of-objects into the next hop's pool. Non-array /
/// non-object values contribute nothing.
fn walk_json_path(top: &Value, path: &[String]) -> Vec<Value> {
    // Seed: if the top-level is an object, start with a single-item
    // pool. If it's an array, start with the elements. Scalars have
    // no structure to descend into.
    let mut pool: Vec<Value> = match top {
        Value::Object(_) => vec![top.clone()],
        Value::Array(items) => items.clone(),
        _ => return Vec::new(),
    };
    for field in path {
        let mut next: Vec<Value> = Vec::new();
        for item in &pool {
            if let Value::Object(obj) = item {
                match obj.get(field) {
                    Some(Value::Array(items)) => {
                        next.extend(items.iter().filter(|v| v.is_object()).cloned());
                    }
                    Some(Value::Object(_)) => {
                        // Nested single object — treat as a one-row
                        // child relation, same way `analyze_json`
                        // does when it emits the child table.
                        if let Some(nested) = obj.get(field) {
                            next.push(nested.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        pool = next;
    }
    pool
}

/// Materialise a pre-selected slice of JSON items into an Arrow
/// `RecordBatch` that matches `table_def`'s column layout.
///
/// The caller owns item selection (see [`resolve_items_for_table`])
/// so this function is agnostic to whether the rows came from the
/// top-level array or from a nested field. Missing columns on a
/// row surface as `NULL` in the Arrow output.
fn scan_json_items_into_batch(
    items: &[Value],
    table_def: &SourceTableDef,
    arrow_schema: &Schema,
    projection: Option<Vec<usize>>,
    limit: Option<usize>,
) -> OxResult<RecordBatch> {
    let selected_indices: Vec<usize> =
        projection.unwrap_or_else(|| (0..table_def.columns.len()).collect());

    let projected_schema = if selected_indices.len() == table_def.columns.len() {
        arrow_schema.clone()
    } else {
        arrow_schema
            .project(&selected_indices)
            .map_err(|e| OxError::Runtime {
                message: format!("json scan: projection error: {e}"),
            })?
    };

    let mut builders: Vec<Box<dyn ArrayBuilder>> = projected_schema
        .fields()
        .iter()
        .map(|f| make_builder(f.data_type()))
        .collect();

    let mut rows_emitted = 0usize;
    for item in items {
        for (builder_idx, col_idx) in selected_indices.iter().enumerate() {
            let col_def = table_def
                .columns
                .get(*col_idx)
                .ok_or_else(|| OxError::Runtime {
                    message: format!("json scan: column index {col_idx} out of range"),
                })?;
            // Dispatch JSON value → Arrow builder directly in
            // `append_json_cell`. No owned-String detour: integers
            // preserve i64 precision beyond f64's exact range,
            // strings borrow into the builder without a clone,
            // structural values (array / object) serialise once at
            // the Utf8 fallback branch.
            append_json_cell(
                "json",
                builders[builder_idx].as_mut(),
                projected_schema.field(builder_idx).data_type(),
                item.get(&col_def.name),
            )?;
        }
        rows_emitted += 1;
        if let Some(cap) = limit
            && rows_emitted >= cap
        {
            break;
        }
    }

    let arrays: Vec<ArrayRef> = builders.into_iter().map(|mut b| b.finish()).collect();
    RecordBatch::try_new(Arc::new(projected_schema), arrays).map_err(|e| OxError::Runtime {
        message: format!("json scan: RecordBatch::try_new failed: {e}"),
    })
}

/// Index a `SourceProfile` into `(table, column) → ColumnStats` and
/// `table → row_count` lookups. Both adapters pre-compute this once;
/// primitives then read in O(1).
fn index_profile(
    profile: &SourceProfile,
) -> (HashMap<(String, String), ColumnStats>, HashMap<String, u64>) {
    let mut stats = HashMap::new();
    let mut counts = HashMap::new();
    for tp in &profile.table_profiles {
        counts.insert(tp.table_name.clone(), tp.row_count);
        for col in &tp.column_stats {
            stats.insert(
                (tp.table_name.clone(), col.column_name.clone()),
                col.clone(),
            );
        }
    }
    (stats, counts)
}

fn list_tables_from_schema(schema: &SourceSchema) -> Vec<String> {
    schema.tables.iter().map(|t| t.name.clone()).collect()
}

fn describe_from_schema(schema: &SourceSchema, table: &str) -> OxResult<SourceTableDef> {
    schema
        .tables
        .iter()
        .find(|t| t.name == table)
        .cloned()
        .ok_or_else(|| OxError::NotFound {
            entity: format!("table `{table}`"),
        })
}

fn empty_stats(column_name: &str) -> ColumnStats {
    ColumnStats {
        column_name: column_name.to_string(),
        null_count: 0,
        distinct_count: 0,
        sample_values: Vec::new(),
        min_value: None,
        max_value: None,
    }
}

// ---------------------------------------------------------------------------
// CSV scan — row → Arrow RecordBatch
// ---------------------------------------------------------------------------
//
// Produces a RecordBatch whose schema matches
// `crate::normalize::describe_to_arrow_schema("csv", table_def)`.
// Projection narrows the emitted columns; limit bounds the row count.
// Each column is built through the Arrow builder matching its Arrow
// `DataType`; unknown / unsupported types fall back to `StringBuilder`
// so the scan never fails because of a dialect gap.

fn scan_csv_into_batch(
    raw: &str,
    table_def: &SourceTableDef,
    arrow_schema: &Schema,
    projection: Option<Vec<usize>>,
    limit: Option<usize>,
) -> OxResult<RecordBatch> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(raw.as_bytes());

    // Header positions in the raw file (may be a superset of columns
    // we expose). We anchor per-column reads to the table-def's column
    // name so extraneous header columns are ignored silently.
    let header_positions: HashMap<String, usize> = reader
        .headers()
        .map_err(|e| OxError::Validation {
            field: "source.data".to_string(),
            message: format!("Invalid CSV headers: {e}"),
        })?
        .iter()
        .enumerate()
        .map(|(idx, name)| (name.trim().to_string(), idx))
        .collect();

    let selected_indices: Vec<usize> = projection
        .unwrap_or_else(|| (0..table_def.columns.len()).collect());

    let projected_schema = if selected_indices.len() == table_def.columns.len() {
        arrow_schema.clone()
    } else {
        arrow_schema
            .project(&selected_indices)
            .map_err(|e| OxError::Runtime {
                message: format!("csv scan: projection error: {e}"),
            })?
    };

    let mut builders: Vec<Box<dyn ArrayBuilder>> = projected_schema
        .fields()
        .iter()
        .map(|f| make_builder(f.data_type()))
        .collect();

    let mut rows_emitted = 0usize;
    for record in reader.records() {
        let record = record.map_err(|e| OxError::Validation {
            field: "source.data".to_string(),
            message: format!("Invalid CSV record: {e}"),
        })?;

        for (builder_idx, col_idx) in selected_indices.iter().enumerate() {
            let col_def = table_def
                .columns
                .get(*col_idx)
                .ok_or_else(|| OxError::Runtime {
                    message: format!("csv scan: column index {col_idx} out of range"),
                })?;
            let raw_value = header_positions
                .get(&col_def.name)
                .and_then(|h| record.get(*h))
                .map(str::trim);
            let normalised = match raw_value {
                Some("") | None => None,
                Some(v) => Some(v),
            };
            append_text_cell(
                "csv",
                builders[builder_idx].as_mut(),
                projected_schema.field(builder_idx).data_type(),
                normalised,
            )?;
        }

        rows_emitted += 1;
        if let Some(cap) = limit
            && rows_emitted >= cap
        {
            break;
        }
    }

    let arrays: Vec<ArrayRef> = builders.into_iter().map(|mut b| b.finish()).collect();
    RecordBatch::try_new(Arc::new(projected_schema), arrays).map_err(|e| OxError::Runtime {
        message: format!("csv scan: RecordBatch::try_new failed: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::Array;

    #[test]
    fn analyze_csv_rejects_oversized_payload() {
        // Build a payload just over the cap. Use an inert filler so the
        // CSV reader is not the bottleneck under test — the guard must
        // fire before parsing starts.
        let header = "id,name\n";
        let body_size = MAX_INLINE_SOURCE_BYTES + 1 - header.len();
        let mut payload = String::with_capacity(MAX_INLINE_SOURCE_BYTES + 1);
        payload.push_str(header);
        payload.extend(std::iter::repeat_n('x', body_size));
        let err = analyze_csv(&payload).expect_err("oversized csv must be rejected");
        assert!(matches!(err, OxError::Validation { ref field, .. } if field == "source.data"));
    }

    #[test]
    fn analyze_json_rejects_oversized_payload() {
        // Size cap fires before `serde_json::from_str` allocates the
        // parse tree — important for DoS resistance, since the parser
        // would otherwise still walk the whole document first.
        let payload = "[".to_string() + &"0,".repeat(MAX_INLINE_SOURCE_BYTES / 2) + "0]";
        assert!(payload.len() > MAX_INLINE_SOURCE_BYTES);
        let err = analyze_json(&payload).expect_err("oversized json must be rejected");
        assert!(matches!(err, OxError::Validation { ref field, .. } if field == "source.data"));
    }

    #[test]
    fn analyze_csv_builds_schema_and_profile() {
        let csv = "id,status,name\n1,1,Alice\n2,2,Bob\n3,3,Charlie\n";
        let (schema, profile) = analyze_csv(csv).expect("csv analysis");

        assert_eq!(schema.source_type, "csv");
        assert_eq!(schema.tables[0].primary_key, vec!["id".to_string()]);
        assert_eq!(profile.table_profiles[0].row_count, 3);
        assert_eq!(profile.table_profiles[0].column_stats[1].distinct_count, 3);
    }

    #[test]
    fn analyze_json_extracts_nested_objects_as_child_tables() {
        let json = r#"[{"id":1,"status":"N","meta":{"tier":"gold"}},{"id":2,"status":"Regular","meta":{"tier":"silver"}}]"#;
        let (schema, profile) = analyze_json(json).expect("json analysis");

        assert_eq!(schema.source_type, "json");

        // Parent table: records (id, status) — no meta column
        let parent = schema
            .tables
            .iter()
            .find(|t| t.name == "records")
            .expect("records table");
        assert!(parent.columns.iter().any(|c| c.name == "id"));
        assert!(parent.columns.iter().any(|c| c.name == "status"));
        assert!(
            !parent.columns.iter().any(|c| c.name == "meta"),
            "meta should be extracted, not inlined"
        );

        // Child table: records_meta (tier only — no synthetic FK column)
        let child = schema
            .tables
            .iter()
            .find(|t| t.name == "records_meta")
            .expect("records_meta table");
        assert!(
            !child.columns.iter().any(|c| c.name == "records_id"),
            "no synthetic FK column on child"
        );
        assert!(child.columns.iter().any(|c| c.name == "tier"));

        // FK relationship via ForeignKeyDef (from_column is descriptive, not a real column)
        assert!(schema.foreign_keys.iter().any(|fk| {
            fk.from_table == "records_meta" && fk.to_table == "records" && fk.to_column == "id"
        }));

        assert_eq!(profile.table_profiles.len(), 2);
    }

    #[test]
    fn analyze_json_extracts_nested_arrays_as_child_tables() {
        let json =
            r#"[{"id":1,"name":"Order A","items":[{"sku":"X","qty":2},{"sku":"Y","qty":1}]}]"#;
        let (schema, _profile) = analyze_json(json).expect("json analysis");

        // Parent: records (id, name)
        let parent = schema
            .tables
            .iter()
            .find(|t| t.name == "records")
            .expect("records table");
        assert!(parent.columns.iter().any(|c| c.name == "id"));
        assert!(parent.columns.iter().any(|c| c.name == "name"));

        // Child: records_items (sku, qty — no synthetic FK column)
        let child = schema
            .tables
            .iter()
            .find(|t| t.name == "records_items")
            .expect("records_items table");
        assert!(
            !child.columns.iter().any(|c| c.name == "records_id"),
            "no synthetic FK column on child"
        );
        assert!(child.columns.iter().any(|c| c.name == "sku"));
        assert!(child.columns.iter().any(|c| c.name == "qty"));

        // FK relationship via ForeignKeyDef
        assert!(schema.foreign_keys.iter().any(|fk| {
            fk.from_table == "records_items" && fk.to_table == "records" && fk.to_column == "id"
        }));
    }

    #[test]
    fn analyze_json_no_fk_when_parent_has_no_pk() {
        // Parent has no "id" field → no PK → no FK should be created
        let json = r#"[{"name":"Alice","address":{"city":"Seoul","zip":"06000"}}]"#;
        let (schema, _profile) = analyze_json(json).expect("json analysis");

        // Both tables exist
        assert!(schema.tables.iter().any(|t| t.name == "records"));
        assert!(schema.tables.iter().any(|t| t.name == "records_address"));

        // Parent has no PK
        let parent = schema.tables.iter().find(|t| t.name == "records").unwrap();
        assert!(parent.primary_key.is_empty(), "parent should have no PK");

        // No FK columns or relationships
        assert!(
            schema.foreign_keys.is_empty(),
            "no FK when parent has no PK"
        );
        let child = schema
            .tables
            .iter()
            .find(|t| t.name == "records_address")
            .unwrap();
        assert!(
            !child.columns.iter().any(|c| c.name == "records_id"),
            "no FK column when parent has no PK"
        );
    }

    /// Regression: a column carrying both integer and float rows
    /// must be inferred as Float64 (via `merge_types`) so the scan
    /// path's `append_to_builder` parses every row as `f64` and
    /// never drops an integer cell to NULL. Exercises CSV.
    #[tokio::test]
    async fn csv_scan_widens_mixed_int_float_column_to_float64() {
        let adapter = CsvAdapter::new("id,amount\n1,100\n2,2.5\n3,42\n").unwrap();
        // `float` at the analyzer layer becomes `Float64` in Arrow.
        let table_def = adapter.describe_table(INLINE_TABLE_NAME).await.unwrap();
        let amount = table_def
            .columns
            .iter()
            .find(|c| c.name == "amount")
            .expect("amount column exists");
        assert_eq!(
            amount.data_type, "float",
            "mixed int+float column must widen to float at analyzer level"
        );
        let batch = adapter
            .scan(INLINE_TABLE_NAME, None, None)
            .await
            .unwrap();
        assert_eq!(batch.num_rows(), 3);
        let arr = batch
            .column(1)
            .as_any()
            .downcast_ref::<arrow_array::Float64Array>()
            .unwrap();
        // Every row is populated — no NULL from a failed int parse.
        assert_eq!(arr.null_count(), 0);
        assert!((arr.value(0) - 100.0).abs() < 1e-9);
        assert!((arr.value(1) - 2.5).abs() < 1e-9);
        assert!((arr.value(2) - 42.0).abs() < 1e-9);
    }

    /// Same property for the JSON profiler: a numeric field with
    /// mixed integer and float values must land as Float64. Pins
    /// the `merge_types("int", "float") == "float"` branch against
    /// future refactors.
    #[tokio::test]
    async fn json_scan_widens_mixed_int_float_column_to_float64() {
        let payload =
            r#"[{"id": 1, "amount": 100}, {"id": 2, "amount": 2.5}, {"id": 3, "amount": 42}]"#;
        let adapter = JsonAdapter::new(payload).unwrap();
        let table_def = adapter.describe_table(INLINE_TABLE_NAME).await.unwrap();
        let amount = table_def
            .columns
            .iter()
            .find(|c| c.name == "amount")
            .expect("amount column exists");
        assert_eq!(
            amount.data_type, "float",
            "mixed int+float JSON column must widen to float"
        );
        let batch = adapter
            .scan(INLINE_TABLE_NAME, None, None)
            .await
            .unwrap();
        assert_eq!(batch.num_rows(), 3);
        // Locate the `amount` column by schema ordering.
        let amount_idx = batch
            .schema()
            .fields()
            .iter()
            .position(|f| f.name() == "amount")
            .expect("amount column is in the batch schema");
        let arr = batch
            .column(amount_idx)
            .as_any()
            .downcast_ref::<arrow_array::Float64Array>()
            .unwrap();
        assert_eq!(arr.null_count(), 0);
        assert!((arr.value(0) - 100.0).abs() < 1e-9);
        assert!((arr.value(1) - 2.5).abs() < 1e-9);
        assert!((arr.value(2) - 42.0).abs() < 1e-9);
    }
}
