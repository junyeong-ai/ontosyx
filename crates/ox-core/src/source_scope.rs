//! Source-analysis selection and draft-lifecycle scope state.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::{OxError, OxResult};

/// Which subset of an external source to introspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableSelection {
    /// Every table the source advertises.
    All,
    /// Only the named tables.
    Subset(BTreeSet<String>),
}

impl TableSelection {
    pub fn subset<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Subset(names.into_iter().map(Into::into).collect())
    }

    pub fn includes(&self, table: &str) -> bool {
        match self {
            Self::All => true,
            Self::Subset(set) => set.contains(table),
        }
    }
}

/// User-facing intent for a source analysis run.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalyzeSelection {
    All,
    Subset { tables: BTreeSet<String> },
    Extend { tables: BTreeSet<String> },
    Reduce { tables: BTreeSet<String> },
    Staged { tables: BTreeSet<String> },
}

impl AnalyzeSelection {
    pub fn additive_tables(&self) -> &BTreeSet<String> {
        static EMPTY: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
        match self {
            Self::Subset { tables } | Self::Extend { tables } | Self::Staged { tables } => tables,
            Self::All | Self::Reduce { .. } => EMPTY.get_or_init(BTreeSet::new),
        }
    }

    pub fn removal_tables(&self) -> &BTreeSet<String> {
        static EMPTY: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
        match self {
            Self::Reduce { tables } => tables,
            Self::All | Self::Subset { .. } | Self::Extend { .. } | Self::Staged { .. } => {
                EMPTY.get_or_init(BTreeSet::new)
            }
        }
    }

    pub fn as_table_selection(&self) -> TableSelection {
        match self {
            Self::All => TableSelection::All,
            Self::Subset { tables } | Self::Extend { tables } | Self::Staged { tables } => {
                TableSelection::Subset(tables.clone())
            }
            Self::Reduce { .. } => TableSelection::Subset(BTreeSet::new()),
        }
    }

    pub fn validate(&self) -> OxResult<()> {
        match self {
            Self::All => Ok(()),
            Self::Subset { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "subset selection requires at least one table name".to_string(),
            }),
            Self::Extend { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "extend selection requires at least one table name".to_string(),
            }),
            Self::Reduce { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "reduce selection requires at least one table name".to_string(),
            }),
            Self::Staged { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "staged selection requires at least one table name".to_string(),
            }),
            Self::Subset { .. }
            | Self::Extend { .. }
            | Self::Reduce { .. }
            | Self::Staged { .. } => Ok(()),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    utoipa::ToSchema,
)]
pub struct AnalysisScope {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub included: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deferred: Vec<DeferredTable>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub excluded_by_policy: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub fingerprints: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_introspected_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema,
)]
pub struct DeferredTable {
    pub table: String,
    pub reason: String,
    pub deferred_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revisit_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSchemaDriftKind {
    Changed,
    Removed,
}

impl TableSchemaDriftKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Removed => "removed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchemaDrift {
    pub table: String,
    pub kind: TableSchemaDriftKind,
}

impl AnalysisScope {
    pub fn record_selection(
        &mut self,
        selection: &AnalyzeSelection,
        all_tables_for_all_selection: &BTreeSet<String>,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        match selection {
            AnalyzeSelection::All => {
                for t in all_tables_for_all_selection {
                    self.include_one(t);
                }
            }
            AnalyzeSelection::Subset { tables } | AnalyzeSelection::Extend { tables } => {
                for t in tables {
                    self.include_one(t);
                }
            }
            AnalyzeSelection::Staged { tables } => {
                for t in tables {
                    self.include_one(t);
                }
                self.defer_remaining(all_tables_for_all_selection, "deferred at bootstrap", now);
            }
            AnalyzeSelection::Reduce { tables } => {
                for t in tables {
                    self.included.remove(t);
                    if !self.deferred.iter().any(|d| &d.table == t) {
                        self.deferred.push(DeferredTable {
                            table: t.clone(),
                            reason: "removed via reduce".into(),
                            deferred_at: now,
                            revisit_at: None,
                        });
                    }
                }
            }
        }
        self.last_introspected_at = Some(now);
    }

    fn include_one(&mut self, table: &str) {
        self.deferred.retain(|d| d.table != table);
        if !self.included.contains(table) {
            self.included.insert(table.to_string());
        }
    }

    pub fn record_fingerprints(
        &mut self,
        fingerprints: impl IntoIterator<Item = (String, String)>,
    ) {
        self.fingerprints = fingerprints.into_iter().collect();
    }

    pub fn defer_remaining(
        &mut self,
        all_tables: &BTreeSet<String>,
        reason: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        for t in all_tables {
            if self.included.contains(t)
                || self.deferred.iter().any(|d| &d.table == t)
                || self.excluded_by_policy.contains(t)
            {
                continue;
            }
            self.deferred.push(DeferredTable {
                table: t.clone(),
                reason: reason.to_string(),
                deferred_at: now,
                revisit_at: None,
            });
        }
    }

    pub fn detect_table_schema_drift(
        &self,
        fresh: &std::collections::BTreeMap<String, String>,
    ) -> Vec<TableSchemaDrift> {
        let mut out = Vec::new();
        for (table, prior_fp) in &self.fingerprints {
            let kind = match fresh.get(table) {
                Some(fresh_fp) if fresh_fp == prior_fp => continue,
                Some(_) => TableSchemaDriftKind::Changed,
                None => TableSchemaDriftKind::Removed,
            };
            out.push(TableSchemaDrift {
                table: table.clone(),
                kind,
            });
        }
        out
    }
}
