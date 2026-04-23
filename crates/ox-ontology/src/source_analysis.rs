use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Adaptive thresholds — single source of truth for all large-schema policies
// ---------------------------------------------------------------------------

/// Table count threshold for analysis report warnings and LLM input compression.
pub const LARGE_SCHEMA_WARNING_THRESHOLD: usize = 50;

/// Maximum cardinality for a column to be treated as categorical/enum.
/// Columns at or below this threshold get ALL distinct values collected during
/// profiling, and ALL values preserved in LLM input (not truncated to 5 samples).
pub const ENUM_CARDINALITY_THRESHOLD: u64 = 100;

/// Table count threshold requiring explicit acknowledgement before design.
/// Also used for PostgreSQL introspection operational warnings.
pub const LARGE_SCHEMA_GATE_THRESHOLD: usize = 100;

/// Node count threshold for activating adaptive graph profile reduction.
pub const LARGE_ONTOLOGY_THRESHOLD: usize = 100;

// ---------------------------------------------------------------------------
// SourceAnalysisReport — result of programmatic pre-design analysis
// ---------------------------------------------------------------------------

/// Full analysis report produced by analyzing schema + profile before ontology design.
/// Contains findings ordered by actionability: implied FKs, PII, ambiguous columns, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAnalysisReport {
    /// Summary statistics about the schema
    pub schema_stats: SchemaStats,
    /// Potential foreign key relationships not declared in the schema
    pub implied_relationships: Vec<ImpliedRelationship>,
    /// Columns that likely contain personal identifiable information
    pub pii_findings: Vec<PiiFinding>,
    /// Columns whose values are ambiguous and need user clarification.
    /// Each entry is a persistent [`crate::ambiguity::AmbiguityContext`]
    /// — the resolver path picks one of these up and attaches a
    /// resolution when the admin or agent decides the meaning.
    pub ambiguous_columns: Vec<crate::ambiguity::AmbiguityContext>,
    /// Tables suggested for exclusion from the ontology
    pub table_exclusion_suggestions: Vec<TableExclusionSuggestion>,
    /// Present when the schema is unusually large
    pub large_schema_warning: Option<LargeSchemaWarning>,
    /// Repo-sourced suggestions for ambiguous columns (user must explicitly accept)
    pub repo_suggestions: Vec<RepoColumnSuggestion>,
    /// Summary of repo analysis results (present only when repo was analyzed)
    pub repo_summary: Option<RepoAnalysisSummary>,
    /// Whether the underlying source analysis was complete or partial
    pub analysis_completeness: AnalysisCompleteness,
    /// Explicit warnings for skipped tables/columns or omitted stats during analysis
    #[serde(default)]
    pub analysis_warnings: Vec<AnalysisWarning>,
}

impl SourceAnalysisReport {
    pub fn with_analysis_warnings(mut self, warnings: Vec<AnalysisWarning>) -> Self {
        self.analysis_completeness = if warnings.is_empty() {
            AnalysisCompleteness::Complete
        } else {
            AnalysisCompleteness::Partial
        };
        self.analysis_warnings = warnings;
        self
    }

    pub fn is_partial(&self) -> bool {
        matches!(self.analysis_completeness, AnalysisCompleteness::Partial)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaStats {
    pub table_count: usize,
    pub column_count: usize,
    pub declared_fk_count: usize,
    pub total_row_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisCompleteness {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WarningLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisWarning {
    pub level: WarningLevel,
    pub phase: AnalysisPhase,
    pub kind: AnalysisWarningKind,
    pub location: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisPhase {
    SchemaIntrospection,
    DataProfiling,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisWarningKind {
    TableSkipped,
    ColumnSkipped,
    ForeignKeysUnavailable,
    SampleValuesOmitted,
}

// ---------------------------------------------------------------------------
// Implied relationships
// ---------------------------------------------------------------------------

/// A foreign key relationship inferred programmatically (not declared in schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpliedRelationship {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    /// 0.0–1.0 confidence (0.85 for pattern match, 0.98 if ORM-confirmed)
    pub confidence: f32,
    pub pattern: ImpliedFkPattern,
    pub reason: String,
    /// True if an ORM model or migration confirmed this relationship
    pub repo_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpliedFkPattern {
    /// Column name ends with `_id`, stripped name matches a known table
    EntityIdSuffix,
}

// ---------------------------------------------------------------------------
// PII detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiFinding {
    pub table: String,
    pub column: String,
    pub pii_type: PiiType,
    pub detection_method: PiiDetectionMethod,
    /// Masked preview (e.g., "hong**@***.com") shown in the report UI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_preview: Option<String>,
}

/// Categories returned by source-level PII detection. Mirrors the
/// regulatory carve-outs of GDPR / HIPAA / PCI DSS so the resulting
/// `DataClassification` and downstream policy enforcement (masking,
/// retention) can be tighter than a generic "personal data" lump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiType {
    // --- Identity ---
    Name,
    Email,
    Phone,
    BirthDate,
    /// Government-issued unique identifier (SSN, RRN, NIN, etc.).
    NationalId,
    /// ICAO 9303 travel document number.
    Passport,
    /// State / province driver's licence number.
    DriversLicense,
    Address,
    /// IPv4 / IPv6 address — flagged as PII under GDPR Recital 30.
    IpAddress,
    /// Lat/long coordinate or precise location identifier.
    GeoLocation,

    // --- Financial (PCI DSS) ---
    /// PAN (primary account number) — strict PCI DSS scope.
    PaymentCard,
    /// Domestic bank account / routing number.
    BankAccount,
    /// ISO 13616 international bank account number.
    Iban,

    // --- Health (HIPAA) ---
    /// Medical record number (HIPAA identifier).
    MedicalRecord,
    /// Health insurance / member ID.
    InsuranceId,

    // --- Other regulated data ---
    /// Biometric template (fingerprint, face vector, voiceprint, etc.).
    Biometric,
    /// PII of a kind not covered above. Reviewer disambiguates.
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiDetectionMethod {
    /// Column name contains a PII keyword (e.g., "email", "phone")
    ColumnName,
    /// Sample value matches a PII pattern (e.g., contains '@')
    ValuePattern,
}

// ---------------------------------------------------------------------------
// PII → DataClassification mapping
// ---------------------------------------------------------------------------

impl PiiType {
    /// Map a PII type to its corresponding data classification level.
    /// Restricted = regulator-imposed handling (HIPAA, PCI DSS, government
    /// IDs, biometrics). Confidential = personal data without a specific
    /// regulatory burden but still privacy-sensitive. Internal = catch-all
    /// for PII categories the reviewer should resolve manually.
    pub fn classify(&self) -> crate::ir::DataClassification {
        use crate::ir::DataClassification;
        match self {
            // Confidential — generic personal data
            PiiType::Email
            | PiiType::Phone
            | PiiType::Name
            | PiiType::Address
            | PiiType::IpAddress => DataClassification::Confidential,

            // Restricted — regulator-imposed (govt ID, finance, health, biometric, precise geo)
            PiiType::NationalId
            | PiiType::Passport
            | PiiType::DriversLicense
            | PiiType::BirthDate
            | PiiType::PaymentCard
            | PiiType::BankAccount
            | PiiType::Iban
            | PiiType::MedicalRecord
            | PiiType::InsuranceId
            | PiiType::Biometric
            | PiiType::GeoLocation => DataClassification::Restricted,

            PiiType::Other => DataClassification::Internal,
        }
    }

    /// Project a detected `PiiType` onto the richer ontology-side
    /// [`crate::ir::PiiKind`] so a property's classification can
    /// later carry the canonical Palantir-grade kind. Lossy by design —
    /// nuances like the `NationalId` country code are not knowable at
    /// detection time.
    pub fn to_pii_kind(&self) -> crate::ir::PiiKind {
        use crate::ir::PiiKind;
        match self {
            PiiType::Name => PiiKind::Name,
            PiiType::Email => PiiKind::Email,
            PiiType::Phone => PiiKind::Phone,
            PiiType::BirthDate => PiiKind::DateOfBirth,
            PiiType::NationalId => PiiKind::NationalId {
                country: String::new(),
            },
            PiiType::Passport => PiiKind::Passport,
            PiiType::DriversLicense => PiiKind::DriversLicense,
            PiiType::Address => PiiKind::Address,
            PiiType::IpAddress => PiiKind::IpAddress,
            PiiType::GeoLocation => PiiKind::GeoLocation,
            PiiType::PaymentCard => PiiKind::PaymentCardNumber,
            PiiType::BankAccount => PiiKind::BankAccountNumber,
            PiiType::Iban => PiiKind::Iban,
            PiiType::MedicalRecord => PiiKind::MedicalRecordNumber,
            PiiType::InsuranceId => PiiKind::InsuranceId,
            PiiType::Biometric => PiiKind::Biometric,
            PiiType::Other => PiiKind::Custom("Other".into()),
        }
    }
}

#[cfg(test)]
mod pii_type_tests {
    use super::*;
    use crate::ir::{DataClassification, PiiKind};

    #[test]
    fn classify_groups_regulated_kinds_as_restricted() {
        // Confidential — generic personal data
        for kind in [
            PiiType::Email,
            PiiType::Phone,
            PiiType::Name,
            PiiType::Address,
            PiiType::IpAddress,
        ] {
            assert_eq!(
                kind.classify(),
                DataClassification::Confidential,
                "{kind:?} should classify as Confidential",
            );
        }

        // Restricted — regulator-imposed handling
        for kind in [
            PiiType::NationalId,
            PiiType::Passport,
            PiiType::DriversLicense,
            PiiType::BirthDate,
            PiiType::PaymentCard,
            PiiType::BankAccount,
            PiiType::Iban,
            PiiType::MedicalRecord,
            PiiType::InsuranceId,
            PiiType::Biometric,
            PiiType::GeoLocation,
        ] {
            assert_eq!(
                kind.classify(),
                DataClassification::Restricted,
                "{kind:?} should classify as Restricted",
            );
        }

        assert_eq!(PiiType::Other.classify(), DataClassification::Internal);
    }

    #[test]
    fn to_pii_kind_maps_payment_and_health_categories() {
        // PCI DSS
        assert!(matches!(
            PiiType::PaymentCard.to_pii_kind(),
            PiiKind::PaymentCardNumber
        ));
        assert!(matches!(PiiType::Iban.to_pii_kind(), PiiKind::Iban));

        // HIPAA
        assert!(matches!(
            PiiType::MedicalRecord.to_pii_kind(),
            PiiKind::MedicalRecordNumber
        ));
        assert!(matches!(
            PiiType::InsuranceId.to_pii_kind(),
            PiiKind::InsuranceId
        ));

        // Travel / govt
        assert!(matches!(PiiType::Passport.to_pii_kind(), PiiKind::Passport));
        assert!(matches!(
            PiiType::DriversLicense.to_pii_kind(),
            PiiKind::DriversLicense
        ));

        // Country code is not knowable at detection time — should be empty.
        match PiiType::NationalId.to_pii_kind() {
            PiiKind::NationalId { country } => assert_eq!(country, ""),
            other => panic!("NationalId should map to PiiKind::NationalId, got {other:?}"),
        }

        // Other is intentionally lossy — falls into Custom for reviewer routing.
        assert!(matches!(PiiType::Other.to_pii_kind(), PiiKind::Custom(_)));
    }
}

// Ambiguous-column types moved to [`crate::ambiguity`] — the persistent
// form (`AmbiguityContext`) replaces the previous transient
// `AmbiguousColumn` shape. `SourceAnalysisReport.ambiguous_columns`
// now carries `Vec<AmbiguityContext>` so the same rows the analyzer
// produces are the ones the resolver stores.

// ---------------------------------------------------------------------------
// Table exclusion suggestions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableExclusionSuggestion {
    pub table_name: String,
    pub reason: TableExclusionReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableExclusionReason {
    /// Likely an audit / history log table
    AuditLog,
    /// Likely a temporary / migration scratch table
    Temporary,
    /// Table has zero rows — no data to model
    Empty,
}

// ---------------------------------------------------------------------------
// Large schema warning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeSchemaWarning {
    pub table_count: usize,
    pub recommended_max: usize,
    pub suggestion: String,
}

// ---------------------------------------------------------------------------
// Repo enrichment results
// ---------------------------------------------------------------------------

/// A suggestion for an ambiguous column derived from repo analysis.
/// Becomes actionable only when the user explicitly accepts it as a column_clarification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoColumnSuggestion {
    pub table: String,
    pub column: String,
    /// Suggested enum definition (e.g., "0=inactive, 1=active, 2=suspended")
    pub suggested_values: String,
    pub source_file: String,
}

/// Outcome of repo enrichment analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoAnalysisStatus {
    /// Enrichment completed successfully
    Complete,
    /// Enrichment ran partially (e.g., some files unreadable)
    Partial,
    /// Enrichment was skipped (no relevant files found)
    Skipped,
    /// Enrichment failed (timeout, LLM error, etc.) — non-fatal, analysis continues without it
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoAnalysisSummary {
    /// Overall outcome of the repo enrichment attempt
    pub status: RepoAnalysisStatus,
    /// Human-readable reason when status is skipped or failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Files the LLM selected for analysis
    pub files_requested: usize,
    /// Files actually read and analyzed (may be fewer due to size/binary limits)
    pub files_analyzed: usize,
    /// Whether the file tree exceeded the max entries limit and was truncated
    pub tree_truncated: bool,
    pub enums_found: usize,
    pub relationships_found: usize,
    /// Ambiguous columns for which repo analysis found a suggestion (not yet user-accepted)
    pub columns_with_suggestions: usize,
    /// Implied FK relationships upgraded from heuristic (0.85) to ORM-confirmed (0.98) confidence
    pub fk_confidence_upgraded: usize,
    /// Git commit SHA the analysis was pinned to (present only for git URL sources)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Free-form field hints from repo analysis (e.g., "ISO 4217 currency code")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_hints: Vec<crate::repo_insights::FieldHint>,
    /// General domain notes from repo analysis (e.g., "multi-tenant SaaS", "soft-delete pattern")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domain_notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// DesignOptions — user decisions passed back to the design endpoint
// ---------------------------------------------------------------------------

/// User-approved decisions that override or supplement automatic analysis.
/// Submitted via `PATCH /api/projects/:id/decisions` after reviewing SourceAnalysisReport.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesignOptions {
    /// Implied relationships the user confirmed as real FKs
    #[serde(default)]
    pub confirmed_relationships: Vec<ConfirmedRelationship>,
    /// Per-column PII handling decisions
    #[serde(default)]
    pub pii_decisions: Vec<PiiDecisionEntry>,
    /// Tables to exclude from ontology design
    #[serde(default)]
    pub excluded_tables: Vec<String>,
    /// Free-text clarifications for ambiguous columns
    #[serde(default)]
    pub column_clarifications: Vec<ColumnClarification>,
    /// User explicitly accepts proceeding with incomplete source analysis.
    #[serde(default)]
    pub allow_partial_source_analysis: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmedRelationship {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

/// A PII handling decision for a specific column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiDecisionEntry {
    pub table: String,
    pub column: String,
    pub decision: PiiDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiDecision {
    /// Replace sample values with "[MASKED]" before sending to LLM
    Mask,
    /// Exclude this column entirely from the ontology
    Exclude,
    /// Allow as-is (user confirms it's acceptable)
    Allow,
}

/// A domain clarification for a specific column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnClarification {
    pub table: String,
    pub column: String,
    pub hint: String,
}

// ---------------------------------------------------------------------------
// apply_pii_classifications — enrich ontology properties with classifications
// ---------------------------------------------------------------------------

/// Cross-reference PII findings with ontology properties via source mapping
/// and set `classification` on each matched property.
///
/// Matching logic:
/// - For each PII finding (table, column), find the node whose source table
///   matches the finding's table (via the authoritative lookup).
/// - Then find the property whose source column (via the same lookup) matches
///   the finding's column.
/// - Set classification based on PiiType.
///
/// Accepts either the legacy `SourceMapping` or a canonical
/// `&[ObjectMappingDef]` slice (both implement
/// [`crate::mapping::ObjectMappingLookup`]). Callers can migrate
/// from the flat shape to the canonical one without touching this
/// function's body.
pub fn apply_pii_classifications<M>(
    ontology: &mut crate::ir::OntologyIR,
    pii_findings: &[PiiFinding],
    source_mapping: &M,
) -> usize
where
    M: ?Sized + crate::mapping::ObjectMappingLookup,
{
    if pii_findings.is_empty() {
        return 0;
    }

    // Build lookup: (table_lower, column_lower) → DataClassification
    // Use the most restrictive classification when duplicates exist.
    let mut pii_map: std::collections::HashMap<
        (String, String),
        crate::ir::DataClassification,
    > = std::collections::HashMap::new();
    for finding in pii_findings {
        let key = (finding.table.to_lowercase(), finding.column.to_lowercase());
        let classification = finding.pii_type.classify();
        pii_map
            .entry(key)
            .and_modify(|existing| {
                // Keep the more restrictive classification
                if matches!(
                    (&classification, &existing),
                    (crate::ir::DataClassification::Restricted, _)
                        | (
                            crate::ir::DataClassification::Confidential,
                            crate::ir::DataClassification::Internal
                        )
                        | (
                            crate::ir::DataClassification::Confidential,
                            crate::ir::DataClassification::Public
                        )
                        | (
                            crate::ir::DataClassification::Internal,
                            crate::ir::DataClassification::Public
                        )
                ) {
                    *existing = classification;
                }
            })
            .or_insert(classification);
    }

    let mut count = 0;

    for node in &mut ontology.node_types {
        // Resolve source table via SourceMapping (the authoritative lookup).
        // NodeTypeDef no longer carries a `source_table` field; downstream
        // consumers should always query SourceMapping for ontology→source
        // relationships.
        let source_table = match source_mapping
            .table_for_node(node.id.as_ref())
            .map(|s| s.to_lowercase())
        {
            Some(t) => t,
            None => continue,
        };

        for prop in &mut node.properties {
            // Only classify if not already classified
            if prop.classification.is_some() {
                continue;
            }

            // Resolve source column via SourceMapping
            let source_column = source_mapping
                .column_for_property(node.id.as_ref(), prop.id.as_ref())
                .map(|s| s.to_lowercase());

            // Fall back to property name as column name
            let column_lower = source_column.unwrap_or_else(|| prop.name.to_lowercase());

            if let Some(&classification) = pii_map.get(&(source_table.clone(), column_lower)) {
                prop.classification = Some(classification);
                count += 1;
            }
        }
    }

    count
}

#[cfg(test)]
mod apply_pii_equivalence_tests {
    use super::*;
    use crate::ir::{NodeTypeDef, PropertyDef};
    use crate::mapping::{ObjectMappingLookup, SourceId, SourceMapping};
    use ox_core::graph_label::GraphLabel;
    use ox_core::property_key::PropertyKey;
    use ox_core::types::PropertyType;

    fn build_ontology() -> (crate::ir::OntologyIR, SourceMapping) {
        // Two nodes, five properties, mixed source-column mappings.
        // Covers: (a) a property with an explicit source_column,
        // (b) a property with no source_column (fall-back to name),
        // (c) a node with no source_table (no mapping at all).
        let email = PropertyDef {
            id: crate::ir::PropertyId::new("prop-email"),
            name: PropertyKey::new("email").expect("key"),
            property_type: PropertyType::String,
            ..Default::default()
        };
        let name = PropertyDef {
            id: crate::ir::PropertyId::new("prop-name"),
            name: PropertyKey::new("full_name").expect("key"),
            property_type: PropertyType::String,
            ..Default::default()
        };
        let total = PropertyDef {
            id: crate::ir::PropertyId::new("prop-total"),
            name: PropertyKey::new("total").expect("key"),
            property_type: PropertyType::Float,
            ..Default::default()
        };
        let customer = NodeTypeDef {
            id: crate::ir::NodeTypeId::new("node-customer"),
            label: GraphLabel::new("Customer").expect("label"),
            properties: vec![email, name],
            ..Default::default()
        };
        let order = NodeTypeDef {
            id: crate::ir::NodeTypeId::new("node-order"),
            label: GraphLabel::new("Order").expect("label"),
            properties: vec![total],
            ..Default::default()
        };
        let ontology = crate::ir::OntologyIR::new(
            "ontology-1".to_string(),
            "PII Test".to_string(),
            ox_core::i18n::LocalizedText::default(),
            1,
            vec![customer, order],
            Vec::new(),
            Vec::new(),
        );

        let mut sm = SourceMapping::new();
        sm.node_tables
            .insert("node-customer".into(), "customers".into());
        sm.node_tables
            .insert("node-order".into(), "orders".into());
        // `email` is bound to `email_addr` (non-identity column).
        sm.set_column("node-customer", "prop-email", "email_addr".into());
        // `name` has no source_column — the reader falls back to the
        // PropertyKey ("full_name") which must hit the finding.
        // `total` (on order) has no source_column either.

        (ontology, sm)
    }

    fn findings() -> Vec<PiiFinding> {
        vec![
            PiiFinding {
                table: "customers".into(),
                column: "email_addr".into(),
                pii_type: PiiType::Email,
                detection_method: PiiDetectionMethod::ColumnName,
                masked_preview: None,
            },
            PiiFinding {
                table: "customers".into(),
                column: "full_name".into(),
                pii_type: PiiType::Name,
                detection_method: PiiDetectionMethod::ColumnName,
                masked_preview: None,
            },
            PiiFinding {
                // Orphan finding — points at `orders.total` which the
                // ontology maps to a node but the finding's column
                // doesn't match any property's source_column AND
                // doesn't match the property name ("total" vs the
                // column "amount" below), so it must remain unmatched.
                table: "orders".into(),
                column: "amount".into(),
                pii_type: PiiType::PaymentCard,
                detection_method: PiiDetectionMethod::ColumnName,
                masked_preview: None,
            },
        ]
    }

    fn extract_classifications(
        ontology: &crate::ir::OntologyIR,
    ) -> Vec<(String, Option<crate::ir::DataClassification>)> {
        ontology
            .node_types
            .iter()
            .flat_map(|n| {
                n.properties.iter().map(|p| {
                    (
                        format!("{}.{}", n.id.as_str(), p.id.as_str()),
                        p.classification,
                    )
                })
            })
            .collect()
    }

    #[test]
    fn legacy_and_canonical_produce_identical_classifications() {
        let (base_ontology, sm) = build_ontology();
        let legacy_findings = findings();

        let mut legacy_ontology = base_ontology.clone();
        let legacy_count =
            apply_pii_classifications(&mut legacy_ontology, &legacy_findings, &sm);

        // Canonical path: convert to ObjectMappingDef[] and re-run.
        // Property key resolution is driven by the ontology's own
        // PropertyDef.name via `canonical_object_mappings` semantics.
        let canonical = sm
            .to_canonical(&SourceId::new("pg-main"), |node_id, prop_id| {
                base_ontology.node_types.iter().find_map(|n| {
                    if n.id.as_ref() == node_id {
                        n.properties
                            .iter()
                            .find(|p| p.id.as_ref() == prop_id)
                            .map(|p| p.name.clone())
                    } else {
                        None
                    }
                })
            })
            .expect("legacy → canonical conversion succeeds");

        // Sanity — the trait impl on the slice must match the
        // inherent lookups on the legacy blob for each known pair.
        for node in &base_ontology.node_types {
            assert_eq!(
                <SourceMapping as ObjectMappingLookup>::table_for_node(&sm, node.id.as_ref()),
                canonical.as_slice().table_for_node(node.id.as_ref()),
            );
            for prop in &node.properties {
                assert_eq!(
                    sm.column_for_property(node.id.as_ref(), prop.id.as_ref()),
                    canonical
                        .as_slice()
                        .column_for_property(node.id.as_ref(), prop.id.as_ref()),
                );
            }
        }

        let mut canonical_ontology = base_ontology.clone();
        let canonical_count = apply_pii_classifications(
            &mut canonical_ontology,
            &legacy_findings,
            canonical.as_slice(),
        );

        // Match count and per-property classifications must be
        // identical — the whole point of the dual-interface trait.
        assert_eq!(legacy_count, canonical_count);
        assert_eq!(
            extract_classifications(&legacy_ontology),
            extract_classifications(&canonical_ontology),
        );

        // And on the positive side: the email + name findings
        // should have landed (matched via column + name fallback).
        let legacy_map = extract_classifications(&legacy_ontology);
        let email_entry = legacy_map
            .iter()
            .find(|(k, _)| k.ends_with(".prop-email"))
            .expect("email property present");
        assert_eq!(
            email_entry.1,
            Some(crate::ir::DataClassification::Confidential),
        );
        let name_entry = legacy_map
            .iter()
            .find(|(k, _)| k.ends_with(".prop-name"))
            .expect("name property present");
        assert_eq!(
            name_entry.1,
            Some(crate::ir::DataClassification::Confidential),
        );
        // The orphan finding did not match, so `total` stays unclassified.
        let total_entry = legacy_map
            .iter()
            .find(|(k, _)| k.ends_with(".prop-total"))
            .expect("total property present");
        assert_eq!(total_entry.1, None);
    }
}
