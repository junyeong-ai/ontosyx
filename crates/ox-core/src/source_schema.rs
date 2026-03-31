use serde::{Deserialize, Serialize};

/// Schema information extracted from a data source (RDBMS, document DB, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSchema {
    /// Data source type (e.g., "postgresql", "mysql", "mongodb")
    pub source_type: String,
    /// Tables/collections discovered
    pub tables: Vec<SourceTableDef>,
    /// Foreign key relationships (critical for graph edge inference)
    pub foreign_keys: Vec<ForeignKeyDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTableDef {
    pub name: String,
    pub columns: Vec<SourceColumnDef>,
    pub primary_key: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceColumnDef {
    pub name: String,
    /// Original DB type (e.g., "varchar", "int4", "jsonb", "timestamp")
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyDef {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    /// True if this relationship was inferred from document structure (e.g., JSON nesting)
    /// rather than declared in the source schema (e.g., DB foreign key constraint).
    #[serde(default, skip_serializing_if = "is_false")]
    pub inferred: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

/// Statistics collected from actual data in the source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceProfile {
    pub table_profiles: Vec<TableProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableProfile {
    pub table_name: String,
    pub row_count: u64,
    pub column_stats: Vec<ColumnStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub column_name: String,
    pub null_count: u64,
    pub distinct_count: u64,
    /// Up to 30 distinct values. Empty if too many distinct values.
    pub sample_values: Vec<String>,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
}
