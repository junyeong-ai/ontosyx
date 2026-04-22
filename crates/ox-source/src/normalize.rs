//! Raw-dialect → Arrow `DataType` normalisation.
//!
//! `SourceColumnDef.data_type` is stored as the adapter's raw dialect
//! string (PostgreSQL `int4`, MySQL `INT`, BigQuery `INT64`, Snowflake
//! `NUMBER(10,2)`, CSV's inferred `int` / `float` / `string`). The
//! federation layer cannot round-trip this soup directly — DataFusion
//! wants an Arrow `DataType`. This module owns the translation.
//!
//! Design notes:
//!
//! - **Source-dispatched.** The same literal `"int"` means different
//!   sizes across engines; we dispatch on `source_type` first and fall
//!   back to a best-effort generic classifier.
//! - **Lenient on unknowns.** An unrecognised raw type falls through
//!   to `DataType::Utf8`. That is the conservative choice: `Utf8` can
//!   carry any source representation, and DataFusion can still compare
//!   strings. Better to show the data than to fail because of an
//!   unexpected dialect variant.
//! - **Schema-only.** No value coercion happens here; row-level
//!   conversion lives in each adapter's `scan` implementation.
//!
//! Phase 2 covers the common SQL primitives plus JSON/BLOB. Phase 6
//! extends to temporal ranges with timezones, arrays, and nested
//! structs as adapters gain pushdown support for them.

use arrow_schema::{DataType, Field, Schema, TimeUnit};
use ox_core::source_schema::{SourceColumnDef, SourceTableDef};

/// Translate a `SourceTableDef` into an Arrow `Schema`. The returned
/// schema preserves column order and nullability.
pub fn describe_to_arrow_schema(source_type: &str, table: &SourceTableDef) -> Schema {
    let fields: Vec<Field> = table
        .columns
        .iter()
        .map(|col| column_to_field(source_type, col))
        .collect();
    Schema::new(fields)
}

/// Translate a single column into an Arrow `Field`.
pub fn column_to_field(source_type: &str, col: &SourceColumnDef) -> Field {
    Field::new(&col.name, raw_type_to_arrow(source_type, &col.data_type), col.nullable)
}

/// Raw-dialect string → Arrow `DataType`. See module docs for the
/// dispatch rule.
pub fn raw_type_to_arrow(source_type: &str, raw: &str) -> DataType {
    let lower = raw.trim().to_ascii_lowercase();
    match source_type {
        "postgresql" => postgresql_type(&lower),
        "mysql" => mysql_type(&lower),
        "snowflake" => snowflake_type(&lower),
        "bigquery" => bigquery_type(&lower),
        "mongodb" => mongodb_type(&lower),
        // DuckDB exposes an Arrow-native schema already; we still
        // normalise the stringified form for consistency with other
        // adapters.
        "duckdb" => generic_type(&lower),
        "csv" | "json" => generic_type(&lower),
        _ => generic_type(&lower),
    }
}

// ---------------------------------------------------------------------------
// Per-dialect classifiers.
// ---------------------------------------------------------------------------

fn postgresql_type(lower: &str) -> DataType {
    // Strip parenthesised modifiers: `varchar(255)` → `varchar`,
    // `numeric(10,2)` → `numeric`. Keeps the matcher simple without
    // losing information downstream (adapters already resolve width
    // in their own value conversion).
    let head = strip_paren_modifier(lower);
    match head {
        "bool" | "boolean" => DataType::Boolean,
        "int2" | "smallint" => DataType::Int16,
        "int4" | "integer" | "int" | "serial" => DataType::Int32,
        "int8" | "bigint" | "bigserial" => DataType::Int64,
        "float4" | "real" => DataType::Float32,
        "float8" | "double precision" | "double" => DataType::Float64,
        "numeric" | "decimal" => DataType::Float64,
        "date" => DataType::Date32,
        "time" | "timetz" => DataType::Time64(TimeUnit::Microsecond),
        "timestamp" | "timestamp without time zone" => {
            DataType::Timestamp(TimeUnit::Microsecond, None)
        }
        "timestamptz" | "timestamp with time zone" => {
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))
        }
        "uuid" | "json" | "jsonb" | "text" | "varchar" | "char" | "bpchar" | "name" => {
            DataType::Utf8
        }
        "bytea" => DataType::Binary,
        _ => DataType::Utf8,
    }
}

fn mysql_type(lower: &str) -> DataType {
    let head = strip_paren_modifier(lower);
    match head {
        "bool" | "boolean" | "bit" => DataType::Boolean,
        "tinyint" => DataType::Int8,
        "smallint" => DataType::Int16,
        "mediumint" | "int" | "integer" => DataType::Int32,
        "bigint" => DataType::Int64,
        "float" => DataType::Float32,
        "double" | "double precision" | "real" => DataType::Float64,
        "decimal" | "numeric" => DataType::Float64,
        "date" => DataType::Date32,
        "time" => DataType::Time64(TimeUnit::Microsecond),
        "datetime" | "timestamp" => DataType::Timestamp(TimeUnit::Microsecond, None),
        "json" | "varchar" | "char" | "text" | "mediumtext" | "longtext" | "enum" | "set" => {
            DataType::Utf8
        }
        "blob" | "mediumblob" | "longblob" | "binary" | "varbinary" => DataType::Binary,
        _ => DataType::Utf8,
    }
}

fn snowflake_type(lower: &str) -> DataType {
    let head = strip_paren_modifier(lower);
    match head {
        "boolean" => DataType::Boolean,
        "number" | "decimal" | "numeric" | "int" | "integer" | "bigint" | "smallint"
        | "tinyint" | "byteint" => DataType::Float64,
        "float" | "float4" | "float8" | "double" | "double precision" | "real" => {
            DataType::Float64
        }
        "date" => DataType::Date32,
        "time" => DataType::Time64(TimeUnit::Microsecond),
        "timestamp" | "timestamp_ntz" | "datetime" => {
            DataType::Timestamp(TimeUnit::Microsecond, None)
        }
        "timestamp_tz" | "timestamp_ltz" => {
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()))
        }
        "varchar" | "char" | "string" | "text" | "variant" | "object" | "array" => DataType::Utf8,
        "binary" | "varbinary" => DataType::Binary,
        _ => DataType::Utf8,
    }
}

fn bigquery_type(lower: &str) -> DataType {
    let head = strip_paren_modifier(lower);
    match head {
        "bool" | "boolean" => DataType::Boolean,
        "int64" | "integer" | "int" | "smallint" | "bigint" | "tinyint" | "byteint" => {
            DataType::Int64
        }
        "float64" | "double" => DataType::Float64,
        "numeric" | "bignumeric" | "decimal" => DataType::Float64,
        "date" => DataType::Date32,
        "time" => DataType::Time64(TimeUnit::Microsecond),
        "datetime" => DataType::Timestamp(TimeUnit::Microsecond, None),
        "timestamp" => DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        "string" | "json" | "struct" | "array" | "geography" | "interval" => DataType::Utf8,
        "bytes" => DataType::Binary,
        _ => DataType::Utf8,
    }
}

fn mongodb_type(lower: &str) -> DataType {
    match lower {
        "bool" | "boolean" => DataType::Boolean,
        "int" | "int32" => DataType::Int32,
        "long" | "int64" => DataType::Int64,
        "double" | "decimal" => DataType::Float64,
        "date" | "timestamp" => DataType::Timestamp(TimeUnit::Millisecond, None),
        "objectid" | "string" | "json" | "regex" => DataType::Utf8,
        "binary" => DataType::Binary,
        _ => DataType::Utf8,
    }
}

/// Best-effort fallback used by CSV / JSON / unknown dialects.
fn generic_type(lower: &str) -> DataType {
    let head = strip_paren_modifier(lower);
    match head {
        "bool" | "boolean" => DataType::Boolean,
        "int" | "integer" | "int2" | "int4" | "int8" | "int16" | "int32" | "int64"
        | "bigint" | "smallint" | "mediumint" | "tinyint" | "long" => DataType::Int64,
        "float" | "float4" | "float8" | "double" | "real" | "numeric" | "decimal" => {
            DataType::Float64
        }
        "date" => DataType::Date32,
        "timestamp" | "timestamptz" | "datetime" => {
            DataType::Timestamp(TimeUnit::Microsecond, None)
        }
        "uuid" | "string" | "text" | "varchar" | "char" | "json" | "jsonb" => DataType::Utf8,
        "bytes" | "blob" | "binary" => DataType::Binary,
        _ => DataType::Utf8,
    }
}

/// `"varchar(255)"` → `"varchar"`. Whitespace-preserving on the head
/// so `"double precision"` still matches.
fn strip_paren_modifier(raw: &str) -> &str {
    match raw.find('(') {
        Some(idx) => raw[..idx].trim(),
        None => raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgresql_integer_family() {
        assert_eq!(raw_type_to_arrow("postgresql", "int4"), DataType::Int32);
        assert_eq!(raw_type_to_arrow("postgresql", "int8"), DataType::Int64);
        assert_eq!(raw_type_to_arrow("postgresql", "smallint"), DataType::Int16);
        assert_eq!(raw_type_to_arrow("postgresql", "bigint"), DataType::Int64);
    }

    #[test]
    fn postgresql_timestamp_variants() {
        assert_eq!(
            raw_type_to_arrow("postgresql", "timestamp"),
            DataType::Timestamp(TimeUnit::Microsecond, None),
        );
        match raw_type_to_arrow("postgresql", "timestamptz") {
            DataType::Timestamp(TimeUnit::Microsecond, Some(tz)) => assert_eq!(tz.as_ref(), "UTC"),
            other => panic!("expected UTC timestamp, got {other:?}"),
        }
    }

    #[test]
    fn bigquery_collapses_integer_widths_to_int64() {
        assert_eq!(raw_type_to_arrow("bigquery", "int64"), DataType::Int64);
        assert_eq!(raw_type_to_arrow("bigquery", "integer"), DataType::Int64);
        assert_eq!(raw_type_to_arrow("bigquery", "smallint"), DataType::Int64);
    }

    #[test]
    fn unknown_dialect_falls_through_to_utf8() {
        assert_eq!(
            raw_type_to_arrow("cockroach", "whatever"),
            DataType::Utf8,
        );
    }

    #[test]
    fn paren_modifiers_are_stripped() {
        assert_eq!(
            raw_type_to_arrow("postgresql", "varchar(255)"),
            DataType::Utf8,
        );
        assert_eq!(
            raw_type_to_arrow("postgresql", "numeric(10,2)"),
            DataType::Float64,
        );
    }

    #[test]
    fn describe_to_arrow_preserves_column_order() {
        let table = SourceTableDef {
            name: "t".into(),
            columns: vec![
                SourceColumnDef {
                    name: "id".into(),
                    data_type: "int4".into(),
                    nullable: false,
                },
                SourceColumnDef {
                    name: "tag".into(),
                    data_type: "varchar(32)".into(),
                    nullable: true,
                },
            ],
            primary_key: vec!["id".into()],
        };
        let schema = describe_to_arrow_schema("postgresql", &table);
        assert_eq!(schema.fields()[0].name(), "id");
        assert_eq!(schema.fields()[0].data_type(), &DataType::Int32);
        assert!(!schema.fields()[0].is_nullable());
        assert_eq!(schema.fields()[1].name(), "tag");
        assert_eq!(schema.fields()[1].data_type(), &DataType::Utf8);
        assert!(schema.fields()[1].is_nullable());
    }
}
