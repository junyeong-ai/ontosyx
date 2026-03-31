use std::collections::{BTreeMap, BTreeSet, HashSet};

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef,
    TableProfile,
};
use serde_json::Value;

const INLINE_TABLE_NAME: &str = "records";
const MAX_DISTINCT_VALUES: usize = 30;

#[derive(Clone, Debug)]
struct Cell {
    raw: Option<String>,
    data_type: &'static str,
}

type Row = BTreeMap<String, Cell>;

pub fn analyze_csv(data: &str) -> OxResult<(SourceSchema, SourceProfile)> {
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
// DataSourceIntrospector wrappers for CSV and JSON
// ---------------------------------------------------------------------------

use async_trait::async_trait;

use crate::DataSourceIntrospector;

/// A `DataSourceIntrospector` backed by in-memory CSV data.
///
/// Schema and profile are computed eagerly at construction time (synchronous),
/// then returned from the async trait methods without further I/O.
pub struct CsvIntrospector {
    schema: SourceSchema,
    profile: SourceProfile,
}

impl CsvIntrospector {
    pub fn new(data: &str) -> OxResult<Self> {
        let (schema, profile) = analyze_csv(data)?;
        Ok(Self { schema, profile })
    }
}

#[async_trait]
impl DataSourceIntrospector for CsvIntrospector {
    fn source_type(&self) -> &str {
        "csv"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        Ok(self.schema.clone())
    }

    async fn collect_stats(&self, _schema: &SourceSchema) -> OxResult<SourceProfile> {
        Ok(self.profile.clone())
    }
}

/// A `DataSourceIntrospector` backed by in-memory JSON data.
///
/// Schema and profile are computed eagerly at construction time (synchronous),
/// then returned from the async trait methods without further I/O.
pub struct JsonIntrospector {
    schema: SourceSchema,
    profile: SourceProfile,
}

impl JsonIntrospector {
    pub fn new(data: &str) -> OxResult<Self> {
        let (schema, profile) = analyze_json(data)?;
        Ok(Self { schema, profile })
    }
}

#[async_trait]
impl DataSourceIntrospector for JsonIntrospector {
    fn source_type(&self) -> &str {
        "json"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        Ok(self.schema.clone())
    }

    async fn collect_stats(&self, _schema: &SourceSchema) -> OxResult<SourceProfile> {
        Ok(self.profile.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
