use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// DesignProjectStatus — lifecycle state of a design project
// ---------------------------------------------------------------------------

/// Status of a design project in its lifecycle.
///
/// ```text
/// analyzed ──→ designed ──→ completed
///    ↑            │
///    └── reanalyze ┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignProjectStatus {
    /// Source analyzed, awaiting user review and design.
    Analyzed,
    /// Ontology designed, available for refinement.
    Designed,
    /// Finalized and promoted to SavedOntology.
    Completed,
}

impl fmt::Display for DesignProjectStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Analyzed => "analyzed",
            Self::Designed => "designed",
            Self::Completed => "completed",
        };
        f.write_str(s)
    }
}

impl FromStr for DesignProjectStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "analyzed" => Ok(Self::Analyzed),
            "designed" => Ok(Self::Designed),
            "completed" => Ok(Self::Completed),
            other => Err(format!("Unknown project status: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// SourceTypeKind — type-safe source discriminator
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceTypeKind {
    Text,
    Csv,
    Json,
    Postgresql,
    Mysql,
    Mongodb,
    /// Code repository analyzed via LLM to extract ORM models as source schema.
    CodeRepository,
    /// Project started from an existing saved ontology (no data source).
    Ontology,
}

impl fmt::Display for SourceTypeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Text => "text",
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Postgresql => "postgresql",
            Self::Mysql => "mysql",
            Self::Mongodb => "mongodb",
            Self::CodeRepository => "code_repository",
            Self::Ontology => "ontology",
        };
        f.write_str(s)
    }
}

impl Serialize for SourceTypeKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SourceTypeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "text" => Ok(Self::Text),
            "csv" => Ok(Self::Csv),
            "json" => Ok(Self::Json),
            "postgresql" => Ok(Self::Postgresql),
            "mysql" => Ok(Self::Mysql),
            "mongodb" => Ok(Self::Mongodb),
            "code_repository" => Ok(Self::CodeRepository),
            "ontology" => Ok(Self::Ontology),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &[
                    "text",
                    "csv",
                    "json",
                    "postgresql",
                    "mysql",
                    "mongodb",
                    "code_repository",
                    "ontology",
                ],
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// SourceConfig — metadata about the data source (no secrets)
// ---------------------------------------------------------------------------

/// Describes which source type was used, without storing credentials.
/// Stored in the design project for display and reanalysis source-type matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub source_type: SourceTypeKind,
    /// Schema name (postgresql) or database name (mysql, mongodb)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    /// Identity fingerprint of the data source (e.g., hash of PG host+db, JSON structure hash).
    /// Used to detect when reanalyze targets a different source instance with the same schema shape,
    /// which invalidates structural decisions (excluded_tables, confirmed_relationships).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_fingerprint: Option<String>,
}

// ---------------------------------------------------------------------------
// SourceHistoryEntry — tracks each data source added to a project
// ---------------------------------------------------------------------------

/// A record of a data source that was used to build or extend the project's ontology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceHistoryEntry {
    pub source_type: SourceTypeKind,
    pub added_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}
