//! Eager rule-drift detection for value-set bindings.
//!
//! When a property is bound to a `ValueSetDef`, the platform derives
//! a SHACL `InValueSet` rule that rejects writes carrying a code
//! outside the set. The set is captured once — at design time — and
//! never re-checked, so when the source data later grows a new code
//! the rule silently rejects valid writes.
//!
//! [`detect_value_set_drift`] is the eager half of the defence: every
//! time a fresh `SourceProfile` lands (analyze / extend / reanalyze
//! pass), walk the ontology's bindings and look for sample values
//! that fall outside the bound set's expansion. Each drift event
//! becomes a `WarningClass::ValueSetDriftDetected` warning the FE
//! renders next to the other source diagnostics.
//!
//! Pure function: no I/O, no LLM, no state. The set-difference is
//! deterministic — same `(ontology, profile)` pair always produces
//! the same warnings — so callers can safely re-run on cache replay
//! without spurious noise.

use std::collections::{BTreeMap, BTreeSet};

use ox_core::source_schema::SourceProfile;

use crate::binding::PropertyBinding;
use crate::ir::OntologyIR;
use crate::mapping::PropertyLocation;
use crate::source_analysis::{
    AnalysisPhase, AnalysisWarning, WarningClass, WarningLevel, WarningScope,
};
use crate::value_set::expand_value_set;

/// Walk every property binding and emit a warning for every
/// (table, column) whose live samples carry codes the bound
/// `ValueSetDef` does not include.
///
/// Skips columns whose mapping is a `JsonPath` — sample values for
/// nested document fields don't round-trip through `ColumnStats` yet,
/// so drift detection on them is not yet observable. The visible
/// surface (column-mapped properties) covers the vast majority of
/// value-set bindings in practice.
pub fn detect_value_set_drift(
    ontology: &OntologyIR,
    profile: &SourceProfile,
) -> Vec<AnalysisWarning> {
    let mut warnings = Vec::new();

    for object in ontology.object_mappings() {
        let Some(node) = ontology.node_by_id(object.node_type_id.as_str()) else {
            continue;
        };

        for prop_mapping in &object.property_mappings {
            let PropertyLocation::Column(column_ref) = &prop_mapping.location else {
                continue;
            };

            let Some(prop) = node
                .properties
                .iter()
                .find(|p| p.id == prop_mapping.property_id)
            else {
                continue;
            };

            let Some(value_set_id) = prop.bindings.iter().find_map(|b| match b {
                PropertyBinding::ValueSet { id, .. } => Some(id),
                _ => None,
            }) else {
                continue;
            };

            let Some(value_set) = ontology.value_set_by_id(value_set_id) else {
                continue;
            };

            let Some(table_profile) = profile
                .table_profiles
                .iter()
                .find(|tp| tp.table_name == column_ref.relation)
            else {
                continue;
            };
            let Some(stats) = table_profile
                .column_stats
                .iter()
                .find(|cs| cs.column_name == column_ref.column)
            else {
                continue;
            };
            if stats.sample_values.is_empty() {
                continue;
            }

            let expansion = expand_value_set(value_set, ontology.code_systems());
            let allowed: BTreeSet<&str> =
                expansion.codes.iter().map(|cv| cv.code.as_str()).collect();

            // Preserve order + dedup the unmapped slice so the
            // warning's `params` list stays deterministic across
            // sample iteration order.
            let mut seen = BTreeSet::new();
            let unexpected: Vec<String> = stats
                .sample_values
                .iter()
                .filter(|v| !allowed.contains(v.as_str()))
                .filter(|v| seen.insert((*v).clone()))
                .cloned()
                .collect();
            if unexpected.is_empty() {
                continue;
            }

            let mut params: BTreeMap<String, String> = BTreeMap::new();
            params.insert("value_set".into(), value_set.name.clone());
            params.insert("unmapped_count".into(), unexpected.len().to_string());
            params.insert("unmapped_codes".into(), unexpected.join(","));

            warnings.push(AnalysisWarning {
                level: WarningLevel::Warning,
                phase: AnalysisPhase::DataProfiling,
                class: WarningClass::ValueSetDriftDetected,
                scope: WarningScope::Column {
                    table: column_ref.relation.clone(),
                    column: column_ref.column.clone(),
                },
                params,
                detail: None,
                group_key: format!(
                    "value_set_drift_detected:{}.{}",
                    column_ref.relation, column_ref.column
                ),
            });
        }
    }

    warnings
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::property_key::PropertyKey;
    use ox_core::source_schema::{ColumnStats, SourceProfile, TableProfile};
    use ox_core::types::PropertyType;

    use crate::binding::BindingStrength;
    use crate::code_system::{
        CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId,
    };
    use crate::ir::{NodeTypeDef, NodeTypeId, OntologyIR, PropertyDef};
    use crate::mapping::{
        ColumnRef, ObjectMappingDef, ObjectMappingId, PropertyMappingDef, PropertyTransform,
        SourceId,
    };
    use crate::value_set::{
        IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule, ValueSetSelector,
    };

    fn coded(id: &str, code: &str) -> CodedValue {
        CodedValue {
            id: CodedValueId::new(id),
            code: code.into(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: Vec::new(),
            broader_id: None,
            examples: Vec::new(),
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn order_status_system() -> CodeSystemDef {
        CodeSystemDef {
            id: CodeSystemId::new("cs-order-status"),
            name: "OrderStatus".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            uri: None,
            hierarchical: false,
            codes: vec![
                coded("cv-active", "ACTIVE"),
                coded("cv-pending", "PENDING"),
                coded("cv-closed", "CLOSED"),
            ],
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn order_status_value_set() -> ValueSetDef {
        ValueSetDef {
            id: ValueSetId::new("vs-order-status"),
            name: "OpenOrderStates".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            version: "1".into(),
            composition: vec![ValueSetIncludeRule {
                system_id: CodeSystemId::new("cs-order-status"),
                selector: ValueSetSelector::All,
                mode: IncludeMode::Include,
            }],
        }
    }

    fn ontology_with_status_binding() -> OntologyIR {
        let order = NodeTypeDef {
            id: "nt-order".into(),
            label: GraphLabel::new("Order").unwrap(),
            description: LocalizedText::default(),
            properties: vec![PropertyDef {
                id: "prop-status".into(),
                name: PropertyKey::new("status").unwrap(),
                property_type: PropertyType::String,
                nullable: false,
                bindings: vec![PropertyBinding::ValueSet {
                    id: ValueSetId::new("vs-order-status"),
                    strength: BindingStrength::Required,
                    concept_map_id: None,
                    valid_from: None,
                    valid_to: None,
                }],
                ..Default::default()
            }],
            constraints: Vec::new(),
            ..Default::default()
        };

        let mut ir = OntologyIR::try_new(
            "ont".into(),
            "DriftTest".into(),
            LocalizedText::default(),
            1u32,
            vec![order],
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        ir.add_code_system(order_status_system()).unwrap();
        ir.add_value_set(order_status_value_set()).unwrap();

        ir.add_object_mapping(ObjectMappingDef {
            id: ObjectMappingId::new("om-order"),
            node_type_id: NodeTypeId::new("nt-order"),
            source_id: SourceId::new("src-pg"),
            relation: "orders".into(),
            relation_kind: Default::default(),
            primary_key_columns: vec![],
            row_filter: None,
            property_mappings: vec![PropertyMappingDef {
                property_id: "prop-status".into(),
                property_key: PropertyKey::new("status").unwrap(),
                location: PropertyLocation::Column(ColumnRef::new("orders", "status")),
                transform: PropertyTransform::Identity,
                concept_map_id: None,
            }],
            partition_columns: Vec::new(),
            workspace_scope: None,
            precedence: 0,
            valid_from: None,
            valid_to: None,
            cache_hint: Default::default(),
        })
        .unwrap();

        ir
    }

    fn profile(table: &str, column: &str, samples: &[&str]) -> SourceProfile {
        SourceProfile {
            table_profiles: vec![TableProfile {
                table_name: table.into(),
                row_count: 100,
                column_stats: vec![ColumnStats {
                    column_name: column.into(),
                    null_count: 0,
                    distinct_count: samples.len() as u64,
                    sample_values: samples.iter().map(|s| (*s).to_string()).collect(),
                    min_value: None,
                    max_value: None,
                    pii_redacted: None,
                }],
            }],
        }
    }

    #[test]
    fn no_warning_when_samples_match_value_set() {
        let ontology = ontology_with_status_binding();
        let profile = profile("orders", "status", &["ACTIVE", "PENDING", "CLOSED"]);
        let warnings = detect_value_set_drift(&ontology, &profile);
        assert!(warnings.is_empty());
    }

    #[test]
    fn warns_on_unmapped_sample_value() {
        let ontology = ontology_with_status_binding();
        let profile = profile(
            "orders",
            "status",
            &["ACTIVE", "PENDING", "CLOSED", "ARCHIVED"],
        );
        let warnings = detect_value_set_drift(&ontology, &profile);
        assert_eq!(warnings.len(), 1);

        let w = &warnings[0];
        assert_eq!(w.class, WarningClass::ValueSetDriftDetected);
        assert!(matches!(
            &w.scope,
            WarningScope::Column { table, column } if table == "orders" && column == "status"
        ));
        assert_eq!(
            w.params.get("unmapped_count").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            w.params.get("unmapped_codes").map(String::as_str),
            Some("ARCHIVED")
        );
        assert_eq!(
            w.params.get("value_set").map(String::as_str),
            Some("OpenOrderStates")
        );
    }

    #[test]
    fn warns_with_deterministic_order_for_multiple_unmapped() {
        let ontology = ontology_with_status_binding();
        // Codes are listed in their first-encounter order in the
        // sample, with duplicates collapsed. Sample order is itself
        // deterministic per `SourceProfile` snapshot, so the same
        // input always produces the same `unmapped_codes` string.
        let profile = profile(
            "orders",
            "status",
            &["SUSPENDED", "ARCHIVED", "ACTIVE", "ARCHIVED"],
        );
        let warnings = detect_value_set_drift(&ontology, &profile);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].params.get("unmapped_codes").map(String::as_str),
            Some("SUSPENDED,ARCHIVED")
        );
        assert_eq!(
            warnings[0].params.get("unmapped_count").map(String::as_str),
            Some("2")
        );
    }

    #[test]
    fn no_warning_when_column_has_no_samples() {
        let ontology = ontology_with_status_binding();
        let profile = profile("orders", "status", &[]);
        let warnings = detect_value_set_drift(&ontology, &profile);
        assert!(warnings.is_empty());
    }

    #[test]
    fn no_warning_when_table_missing_from_profile() {
        let ontology = ontology_with_status_binding();
        let profile = profile("other_table", "status", &["ACTIVE", "ARCHIVED"]);
        let warnings = detect_value_set_drift(&ontology, &profile);
        assert!(warnings.is_empty());
    }

    #[test]
    fn group_key_is_stable_per_column() {
        let ontology = ontology_with_status_binding();
        let profile = profile("orders", "status", &["ARCHIVED"]);
        let warnings = detect_value_set_drift(&ontology, &profile);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].group_key,
            "value_set_drift_detected:orders.status"
        );
    }
}
