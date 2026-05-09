//! `SourceContractDef` — frozen snapshot of a source relation's
//! column / type / primary-key shape at the moment introspection
//! ran.
//!
//! Without this contract layer, `OntologyIR::validate_with_sources`
//! can only check that referenced `source_id`s are *registered*
//! — it has no signal for "this `ObjectMappingDef.relation` does
//! not actually exist on that source" or "this
//! `PropertyMappingDef.column` is mapped to a column the source
//! never returned". Both of those land at runtime as adapter-side
//! failures (broken queries, silent NULLs) instead of at commit
//! time as a typed validation reject.
//!
//! `SourceContractDef` closes the gap. The introspection pipeline
//! upserts a contract per `(source_id, relation)` whenever it
//! reads a relation; the commit-path validator
//! (`OntologyIR::validate_against_source_contracts`) walks every
//! mapping against the contract bank and surfaces violations as
//! diagnostic messages with the same code+params shape as the
//! existing IR validators.
//!
//! ## Lifecycle
//!
//! - `introspect` runs against a source → for each relation it
//!   examined, an upsert lands a `SourceContractDef` row keyed on
//!   `(workspace_id, source_id, relation)`.
//! - `complete_ontology_draft` (and the canonical-edit commit
//!   path) loads every contract for the workspace and calls the
//!   validator before the version snapshot lands.
//! - The contract carries a `fingerprint` (sha256 over the
//!   serialised columns + primary key) so two consecutive
//!   introspections of an unchanged relation are byte-identical
//!   no-ops at the row-data level. A fingerprint mismatch on the
//!   incoming row vs the stored row is the schema-drift signal
//!   the FE surfaces as "this source moved on".
//!
//! ## Why a separate type from `TableInventoryEntry`
//!
//! `TableInventoryEntry` carries the *operator-intent axis*: which
//! tables the project chose to import, which it declined, which
//! it retracted. It tracks contribution (`contributed_node_ids`,
//! `contributed_edge_ids`) and a structural digest, but no
//! per-column data — by design, because the inventory is
//! workspace-curation metadata and survives schema drift.
//!
//! `SourceContractDef` carries the *physical-fidelity axis*:
//! exactly which columns + types + keys the source returned the
//! last time the kernel asked. It mutates with the source. The
//! two axes serve orthogonal needs and intentionally do not share
//! a type — collapsing them would force one of the two to bend
//! out of shape.

use chrono::{DateTime, Utc};
use ox_core::types::PropertyType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::mapping::SourceId;

/// Coarse semantic category every source data-type spelling
/// reduces to. The validator checks compatibility at this level
/// rather than at exact-string equality so a Postgres `bigint`
/// and a BigQuery `INT64` both pass when the property is `Int`.
///
/// `Unknown` is the fail-open bucket: a data-type spelling the
/// classifier doesn't recognise (vendor-specific extensions,
/// driver quirks) silently passes the validator instead of
/// surfacing as a false positive. The categoriser is heuristic
/// by design — operators that hit a real incompatibility see it
/// at runtime; the gate is for catching common typos at commit
/// time, not for proving a complete type-system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTypeCategory {
    Boolean,
    Integer,
    Numeric,
    Text,
    Date,
    Timestamp,
    Duration,
    Bytes,
    /// JSON / struct / array / nested object.
    Json,
    /// UUID specifically — coerces to Text downstream but the
    /// classifier carries the explicit category for surfaces that
    /// want to surface "this is an opaque identifier" hints.
    Uuid,
    Unknown,
}

impl SourceTypeCategory {
    /// `true` when the category fits the property type. The
    /// matrix is intentionally generous — coercions every
    /// production system already does (Int → Float, anything →
    /// String when stored as text) are accepted so the gate
    /// catches typos, not theory-of-types violations.
    ///
    /// `Unknown` is universally compatible — an unrecognised
    /// data type is fail-open. Likewise `Text` for the property
    /// `String` accepts every category because `String` is the
    /// catch-all the operator picks when they don't want a
    /// constrained type.
    pub fn is_compatible_with(self, property_type: &PropertyType) -> bool {
        if matches!(self, SourceTypeCategory::Unknown) {
            return true;
        }
        match property_type {
            PropertyType::Bool => matches!(
                self,
                SourceTypeCategory::Boolean | SourceTypeCategory::Integer
            ),
            PropertyType::Int => matches!(self, SourceTypeCategory::Integer),
            PropertyType::Float => matches!(
                self,
                SourceTypeCategory::Integer | SourceTypeCategory::Numeric
            ),
            // String is the catch-all: every source value can be
            // cast to text on the way through the planner.
            PropertyType::String => true,
            PropertyType::Date => matches!(
                self,
                SourceTypeCategory::Date | SourceTypeCategory::Timestamp
            ),
            PropertyType::DateTime => matches!(self, SourceTypeCategory::Timestamp),
            PropertyType::Duration => matches!(self, SourceTypeCategory::Duration),
            PropertyType::Bytes => matches!(self, SourceTypeCategory::Bytes),
            // Containers map to JSON-shaped sources. List<scalar>
            // can also ride a Text column with a delimiter, but
            // that route goes through `PropertyTransform::Concat`
            // / `SqlExpr` so we don't accept Text here.
            PropertyType::List { .. } | PropertyType::Map => {
                matches!(self, SourceTypeCategory::Json)
            }
        }
    }
}

/// Best-effort categorisation of a source-side data type spelling.
///
/// The classifier is heuristic — substring matching against
/// well-known dialect spellings (Postgres / MySQL / BigQuery /
/// Snowflake / DuckDB / SQLite). Vendor-specific or unknown
/// spellings collapse to [`SourceTypeCategory::Unknown`], which
/// the compatibility check fail-opens on so the validator does
/// not produce false positives.
///
/// Comparisons run on the *trimmed, lower-cased* spelling, so
/// `BIGINT`, ` bigint`, and `bigint` all collapse to one class.
/// Length / precision suffixes (`varchar(255)`, `numeric(10,2)`)
/// are folded by stripping at the first `(`.
pub fn categorize_data_type(data_type: &str) -> SourceTypeCategory {
    let trimmed = data_type.trim().to_ascii_lowercase();
    // Strip parameters: `varchar(255)` → `varchar`,
    // `numeric(10,2)` → `numeric`, `array<int64>` → `array<int64>`
    // (kept; the Json arm matches it whole).
    let head = trimmed.split('(').next().unwrap_or(&trimmed).trim();

    // JSON / structured first because they often contain bracketed
    // type spellings that would confuse a substring match below.
    if head == "json"
        || head == "jsonb"
        || head.starts_with("array")
        || head.starts_with("struct")
        || head.starts_with("map")
        || head == "record"
        || head == "variant"
        || head == "object"
    {
        return SourceTypeCategory::Json;
    }

    // UUID — explicit category; downstream String compat absorbs.
    if head == "uuid" || head == "uniqueidentifier" {
        return SourceTypeCategory::Uuid;
    }

    // Booleans.
    if head == "bool" || head == "boolean" || head == "tinyint(1)" || head == "bit" {
        return SourceTypeCategory::Boolean;
    }

    // Integers (incl. unsigned variants).
    if matches!(
        head,
        "int" | "int2"
            | "int4"
            | "int8"
            | "integer"
            | "bigint"
            | "smallint"
            | "tinyint"
            | "mediumint"
            | "int64"
            | "long"
            | "serial"
            | "bigserial"
            | "smallserial"
    ) {
        return SourceTypeCategory::Integer;
    }

    // Floating / decimal.
    if matches!(
        head,
        "float"
            | "float4"
            | "float8"
            | "real"
            | "double"
            | "double precision"
            | "numeric"
            | "decimal"
            | "number"
            | "money"
            | "float64"
            | "float32"
    ) {
        return SourceTypeCategory::Numeric;
    }

    // Date.
    if head == "date" {
        return SourceTypeCategory::Date;
    }

    // Timestamp / datetime variants. Match a few dialects whose
    // spelling carries a timezone tag. `time without time zone`
    // is intentionally bucketed Timestamp here — closer to
    // DateTime than to Duration on a typical mapping.
    if matches!(
        head,
        "timestamp"
            | "timestamptz"
            | "timestamp with time zone"
            | "timestamp without time zone"
            | "datetime"
            | "datetime2"
            | "smalldatetime"
            | "time"
            | "timetz"
    ) || head.starts_with("timestamp")
    {
        return SourceTypeCategory::Timestamp;
    }

    // Duration.
    if head == "interval" || head == "duration" {
        return SourceTypeCategory::Duration;
    }

    // Bytes.
    if matches!(
        head,
        "bytea" | "blob" | "binary" | "varbinary" | "bytes" | "longblob" | "mediumblob" | "tinyblob"
    ) || head.starts_with("varbinary")
        || head.starts_with("binary")
    {
        return SourceTypeCategory::Bytes;
    }

    // Text — catch-all for character spellings.
    if matches!(
        head,
        "text"
            | "varchar"
            | "char"
            | "nchar"
            | "nvarchar"
            | "string"
            | "clob"
            | "longtext"
            | "mediumtext"
            | "tinytext"
            | "character varying"
            | "character"
            | "citext"
    ) {
        return SourceTypeCategory::Text;
    }

    SourceTypeCategory::Unknown
}

/// One column the source returned at introspection time.
///
/// `data_type` is the source's native type spelling
/// (`bigint` for Postgres, `STRING` for BigQuery, `Int64` for
/// DuckDB, etc.). The contract validator only checks *presence* of
/// the column at this phase; richer type-compatibility checking
/// (ontology `String` ↔ source `varchar`) is a future axis that
/// hangs off this same data without a substrate change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ColumnSpec {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

impl ColumnSpec {
    pub fn new(name: impl Into<String>, data_type: impl Into<String>, nullable: bool) -> Self {
        Self {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
        }
    }

    /// Heuristic categorisation of `data_type`. Convenience
    /// wrapper over [`categorize_data_type`] that callers walking
    /// a contract can use without re-importing the free function.
    pub fn category(&self) -> SourceTypeCategory {
        categorize_data_type(&self.data_type)
    }
}

/// One row of the source-contract bank: the structural shape of a
/// single relation on a single source.
///
/// `(workspace_id, source_id, relation)` is the upsert natural key
/// — re-introspecting an unchanged relation idempotently overwrites
/// in place and refreshes `introspected_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct SourceContractDef {
    pub source_id: SourceId,
    pub relation: String,
    pub columns: Vec<ColumnSpec>,
    /// Empty when the source advertises no primary key (CSV,
    /// JSON, some legacy views). The mapping validator emits a
    /// distinct diagnostic for "mapping declares a primary_key
    /// column the contract has no PK to compare against" so the
    /// operator sees it as an explicit warning, not a silent pass.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_key: Vec<String>,
    /// sha256 over the canonicalised columns + primary_key.
    /// Stable across cosmetic re-orderings because both vectors
    /// are sorted before hashing in [`Self::compute_fingerprint`].
    pub fingerprint: String,
    pub introspected_at: DateTime<Utc>,
}

impl SourceContractDef {
    /// Construct a contract stamped with `Utc::now()` and a
    /// canonical fingerprint. Callers from the introspection
    /// pipeline use this constructor; deserialisation skips the
    /// builder and trusts the persisted fingerprint.
    pub fn new(
        source_id: SourceId,
        relation: impl Into<String>,
        columns: Vec<ColumnSpec>,
        primary_key: Vec<String>,
    ) -> Self {
        let fingerprint = Self::compute_fingerprint(&columns, &primary_key);
        Self {
            source_id,
            relation: relation.into(),
            columns,
            primary_key,
            fingerprint,
            introspected_at: Utc::now(),
        }
    }

    /// `O(n)` lookup. Linear scan is fine — contracts rarely
    /// exceed a few hundred columns and the validator only walks
    /// each contract a bounded number of times per commit.
    pub fn column(&self, name: &str) -> Option<&ColumnSpec> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn has_column(&self, name: &str) -> bool {
        self.column(name).is_some()
    }

    /// sha256 over a canonical encoding of the columns + primary
    /// key. Sorts both vectors before hashing so two contracts
    /// that differ only in column order produce the same digest —
    /// the introspector can return columns in whatever order the
    /// driver hands back without surfacing as drift.
    pub fn compute_fingerprint(columns: &[ColumnSpec], primary_key: &[String]) -> String {
        let mut sorted_cols: Vec<&ColumnSpec> = columns.iter().collect();
        sorted_cols.sort_by(|a, b| a.name.cmp(&b.name));
        let mut sorted_pk: Vec<&String> = primary_key.iter().collect();
        sorted_pk.sort();

        let mut hasher = sha2::Sha256::new();
        for col in sorted_cols {
            hasher.update(col.name.as_bytes());
            hasher.update(b"\x00");
            hasher.update(col.data_type.as_bytes());
            hasher.update(b"\x00");
            hasher.update(if col.nullable { b"1" } else { b"0" });
            hasher.update(b"\x01");
        }
        hasher.update(b"\x02");
        for pk in sorted_pk {
            hasher.update(pk.as_bytes());
            hasher.update(b"\x00");
        }
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, dt: &str, nullable: bool) -> ColumnSpec {
        ColumnSpec::new(name, dt, nullable)
    }

    #[test]
    fn fingerprint_is_stable_under_column_reordering() {
        let cols_a = vec![
            col("id", "bigint", false),
            col("name", "text", true),
            col("created_at", "timestamptz", true),
        ];
        let cols_b = vec![
            col("created_at", "timestamptz", true),
            col("id", "bigint", false),
            col("name", "text", true),
        ];
        let pk = vec!["id".to_string()];
        assert_eq!(
            SourceContractDef::compute_fingerprint(&cols_a, &pk),
            SourceContractDef::compute_fingerprint(&cols_b, &pk),
        );
    }

    #[test]
    fn fingerprint_changes_when_column_added() {
        let pk = vec!["id".to_string()];
        let a = SourceContractDef::compute_fingerprint(
            &[col("id", "bigint", false), col("name", "text", true)],
            &pk,
        );
        let b = SourceContractDef::compute_fingerprint(
            &[
                col("id", "bigint", false),
                col("name", "text", true),
                col("email", "text", true),
            ],
            &pk,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_when_nullability_flips() {
        let pk = vec!["id".to_string()];
        let a = SourceContractDef::compute_fingerprint(
            &[col("id", "bigint", false), col("name", "text", true)],
            &pk,
        );
        let b = SourceContractDef::compute_fingerprint(
            &[col("id", "bigint", false), col("name", "text", false)],
            &pk,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_changes_when_primary_key_changes() {
        let cols = [col("id", "bigint", false), col("alt_id", "bigint", false)];
        let a = SourceContractDef::compute_fingerprint(&cols, &["id".to_string()]);
        let b = SourceContractDef::compute_fingerprint(&cols, &["alt_id".to_string()]);
        assert_ne!(a, b);
    }

    #[test]
    fn round_trips_through_serde() {
        let contract = SourceContractDef::new(
            SourceId::new("pg-main"),
            "customers",
            vec![
                col("id", "bigint", false),
                col("email", "text", true),
                col("created_at", "timestamptz", false),
            ],
            vec!["id".to_string()],
        );
        let json = serde_json::to_string(&contract).unwrap();
        let back: SourceContractDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, contract);
    }

    #[test]
    fn column_lookup_is_case_sensitive() {
        let contract = SourceContractDef::new(
            SourceId::new("pg-main"),
            "customers",
            vec![col("Id", "bigint", false), col("name", "text", true)],
            vec!["Id".to_string()],
        );
        assert!(contract.has_column("Id"));
        assert!(!contract.has_column("id"));
        assert!(contract.has_column("name"));
    }

    #[test]
    fn categorize_postgres_dialect_spellings() {
        assert_eq!(categorize_data_type("bigint"), SourceTypeCategory::Integer);
        assert_eq!(categorize_data_type("INT8"), SourceTypeCategory::Integer);
        assert_eq!(
            categorize_data_type("varchar(255)"),
            SourceTypeCategory::Text,
        );
        assert_eq!(
            categorize_data_type("timestamptz"),
            SourceTypeCategory::Timestamp,
        );
        assert_eq!(
            categorize_data_type("timestamp with time zone"),
            SourceTypeCategory::Timestamp,
        );
        assert_eq!(
            categorize_data_type("numeric(10,2)"),
            SourceTypeCategory::Numeric,
        );
        assert_eq!(categorize_data_type("jsonb"), SourceTypeCategory::Json);
        assert_eq!(categorize_data_type("uuid"), SourceTypeCategory::Uuid);
        assert_eq!(categorize_data_type("bytea"), SourceTypeCategory::Bytes);
        assert_eq!(
            categorize_data_type("interval"),
            SourceTypeCategory::Duration,
        );
    }

    #[test]
    fn categorize_bigquery_and_snowflake_dialect_spellings() {
        assert_eq!(categorize_data_type("INT64"), SourceTypeCategory::Integer);
        assert_eq!(categorize_data_type("FLOAT64"), SourceTypeCategory::Numeric);
        assert_eq!(categorize_data_type("STRING"), SourceTypeCategory::Text);
        assert_eq!(categorize_data_type("BYTES"), SourceTypeCategory::Bytes);
        assert_eq!(categorize_data_type("ARRAY<INT64>"), SourceTypeCategory::Json);
        assert_eq!(categorize_data_type("STRUCT<…>"), SourceTypeCategory::Json);
        assert_eq!(categorize_data_type("VARIANT"), SourceTypeCategory::Json);
        assert_eq!(categorize_data_type("NUMBER(38,9)"), SourceTypeCategory::Numeric);
    }

    #[test]
    fn unknown_data_type_falls_into_unknown() {
        assert_eq!(
            categorize_data_type("vendor_specific_blob_type"),
            SourceTypeCategory::Unknown,
        );
    }

    #[test]
    fn category_compatibility_int_property() {
        assert!(SourceTypeCategory::Integer.is_compatible_with(&PropertyType::Int));
        assert!(!SourceTypeCategory::Numeric.is_compatible_with(&PropertyType::Int));
        assert!(!SourceTypeCategory::Text.is_compatible_with(&PropertyType::Int));
    }

    #[test]
    fn category_compatibility_float_property_accepts_int_too() {
        assert!(SourceTypeCategory::Integer.is_compatible_with(&PropertyType::Float));
        assert!(SourceTypeCategory::Numeric.is_compatible_with(&PropertyType::Float));
        assert!(!SourceTypeCategory::Text.is_compatible_with(&PropertyType::Float));
    }

    #[test]
    fn category_compatibility_string_is_universal() {
        for c in [
            SourceTypeCategory::Boolean,
            SourceTypeCategory::Integer,
            SourceTypeCategory::Numeric,
            SourceTypeCategory::Text,
            SourceTypeCategory::Date,
            SourceTypeCategory::Timestamp,
            SourceTypeCategory::Bytes,
            SourceTypeCategory::Json,
            SourceTypeCategory::Uuid,
        ] {
            assert!(
                c.is_compatible_with(&PropertyType::String),
                "String property must accept every source category, including {c:?}",
            );
        }
    }

    #[test]
    fn category_compatibility_unknown_is_universal() {
        for pt in [
            PropertyType::Bool,
            PropertyType::Int,
            PropertyType::Float,
            PropertyType::String,
            PropertyType::Date,
            PropertyType::DateTime,
            PropertyType::Duration,
            PropertyType::Bytes,
            PropertyType::Map,
        ] {
            assert!(
                SourceTypeCategory::Unknown.is_compatible_with(&pt),
                "Unknown source category must fail-open against {pt:?}",
            );
        }
    }

    #[test]
    fn category_compatibility_datetime_property() {
        assert!(SourceTypeCategory::Timestamp.is_compatible_with(&PropertyType::DateTime));
        assert!(!SourceTypeCategory::Date.is_compatible_with(&PropertyType::DateTime));
    }

    #[test]
    fn category_compatibility_date_property_accepts_timestamp() {
        assert!(SourceTypeCategory::Date.is_compatible_with(&PropertyType::Date));
        // Timestamp can be cast / truncated to Date — accept.
        assert!(SourceTypeCategory::Timestamp.is_compatible_with(&PropertyType::Date));
    }

    #[test]
    fn category_compatibility_list_property_requires_json() {
        let list_int = PropertyType::List {
            element: Box::new(PropertyType::Int),
        };
        assert!(SourceTypeCategory::Json.is_compatible_with(&list_int));
        assert!(!SourceTypeCategory::Text.is_compatible_with(&list_int));
        assert!(!SourceTypeCategory::Integer.is_compatible_with(&list_int));
    }

    #[test]
    fn new_stamps_timestamp_within_window() {
        let before = Utc::now();
        let contract = SourceContractDef::new(
            SourceId::new("pg-main"),
            "users",
            vec![col("id", "bigint", false)],
            vec!["id".to_string()],
        );
        let after = Utc::now();
        assert!(contract.introspected_at >= before);
        assert!(contract.introspected_at <= after);
    }
}
