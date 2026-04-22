//! Typed ontology-edit log for the admin surface.
//!
//! Where [`crate::command::OntologyCommand`] targets the LLM agent
//! (and carries a Bedrock-compatible flat JsonSchema), this module's
//! [`OntologyEditOp`] is the fine-grained taxonomy the admin API
//! writes through: per-entity Create / Update / Delete across the
//! Phase-5B governance collections (CodeSystem, GlossaryTerm,
//! ObjectMapping / LinkMapping, NotationPattern, ConceptMap,
//! ValueSet).
//!
//! The split exists because the two audiences want different
//! strengths:
//!
//! * Agent tool: few variants, values typed permissively, one shot
//!   per turn. Optimised for LLM output ergonomics.
//! * Admin edit: every entity variant called out, typed strictly,
//!   batched atomically, gated by `expected_version` for optimistic
//!   locking. Optimised for audit + routing + validation.
//!
//! The two enums don't reuse a supertype — each path is independent
//! and free to grow without pulling its sibling along.
//!
//! ## Routing
//!
//! Every variant classifies into a
//! [`crate::change_routing::ChangeType`] via
//! [`OntologyEditOp::classify_change_type`]. The approval workflow
//! consumes the classification to decide auto-apply vs. queue.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::change_routing::ChangeType;
use crate::code_system::{CodeSystemDef, CodeSystemId, CodedValue, CodedValueId};
use crate::concept_map::{ConceptMapDef, ConceptMapId};
use crate::glossary::{GlossaryTermDef, GlossaryTermId};
use crate::ir::OntologyIR;
use crate::mapping::link::LinkMappingDef;
use crate::mapping::object::ObjectMappingDef;
use crate::mapping::refs::{LinkMappingId, ObjectMappingId};
use crate::notation_pattern::{NotationPatternDef, NotationPatternId};
use crate::value_set::{ValueSetDef, ValueSetId};

/// Typed ontology edit. Each variant names exactly one entity kind;
/// a batch of variants applies atomically (all-or-nothing) and is
/// validated together at the end.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum OntologyEditOp {
    // --- CodeSystem ---
    CreateCodeSystem {
        def: CodeSystemDef,
    },
    UpdateCodeSystem {
        id: CodeSystemId,
        def: CodeSystemDef,
    },
    DeleteCodeSystem {
        id: CodeSystemId,
    },

    // --- CodedValue (nested under CodeSystem) ---
    CreateCodedValue {
        code_system_id: CodeSystemId,
        value: CodedValue,
    },
    UpdateCodedValue {
        code_system_id: CodeSystemId,
        id: CodedValueId,
        value: CodedValue,
    },
    DeleteCodedValue {
        code_system_id: CodeSystemId,
        id: CodedValueId,
    },

    // --- GlossaryTerm ---
    CreateGlossaryTerm {
        def: GlossaryTermDef,
    },
    UpdateGlossaryTerm {
        id: GlossaryTermId,
        def: GlossaryTermDef,
    },
    DeleteGlossaryTerm {
        id: GlossaryTermId,
    },

    // --- ObjectMapping (NodeType → relation) ---
    CreateObjectMapping {
        mapping: ObjectMappingDef,
    },
    UpdateObjectMapping {
        id: ObjectMappingId,
        mapping: ObjectMappingDef,
    },
    DeleteObjectMapping {
        id: ObjectMappingId,
    },

    // --- LinkMapping (EdgeType → FK/Bridge/Computed/Federated) ---
    CreateLinkMapping {
        mapping: LinkMappingDef,
    },
    UpdateLinkMapping {
        id: LinkMappingId,
        mapping: LinkMappingDef,
    },
    DeleteLinkMapping {
        id: LinkMappingId,
    },

    // --- NotationPattern ---
    CreateNotationPattern {
        def: NotationPatternDef,
    },
    UpdateNotationPattern {
        id: NotationPatternId,
        def: NotationPatternDef,
    },
    DeleteNotationPattern {
        id: NotationPatternId,
    },

    // --- ConceptMap (bidirectional code system mapping) ---
    CreateConceptMap {
        def: ConceptMapDef,
    },
    UpdateConceptMap {
        id: ConceptMapId,
        def: ConceptMapDef,
    },
    DeleteConceptMap {
        id: ConceptMapId,
    },

    // --- ValueSet (CodeSystem composition) ---
    CreateValueSet {
        def: ValueSetDef,
    },
    UpdateValueSet {
        id: ValueSetId,
        def: ValueSetDef,
    },
    DeleteValueSet {
        id: ValueSetId,
    },
}

impl OntologyEditOp {
    /// Map this edit to its `ChangeType` for routing. The mapping is
    /// deliberately many-to-one: every CodedValue lifecycle op lands
    /// on `CodedValueCreate` or `CodedValueDeprecate` (the matrix
    /// row), and every terminology CRUD on Glossary / NotationPattern
    /// / ObjectMapping etc. lands on its matrix row. ConceptMap +
    /// ValueSet classify as `GlossaryTermCreate` for now — they share
    /// the "terminology edit" risk class with the matrix row until the
    /// patent adds dedicated rows for them.
    pub fn classify_change_type(&self) -> ChangeType {
        match self {
            Self::CreateCodeSystem { .. } | Self::UpdateCodeSystem { .. } => {
                ChangeType::CodedValueCreate
            }
            Self::DeleteCodeSystem { .. } => ChangeType::CodedValueDeprecate,

            Self::CreateCodedValue { .. } | Self::UpdateCodedValue { .. } => {
                ChangeType::CodedValueCreate
            }
            Self::DeleteCodedValue { .. } => ChangeType::CodedValueDeprecate,

            Self::CreateGlossaryTerm { .. } | Self::UpdateGlossaryTerm { .. } => {
                ChangeType::GlossaryTermCreate
            }
            Self::DeleteGlossaryTerm { .. } => ChangeType::GlossaryAliasAdd,

            Self::CreateObjectMapping { .. }
            | Self::UpdateObjectMapping { .. }
            | Self::DeleteObjectMapping { .. }
            | Self::CreateLinkMapping { .. }
            | Self::UpdateLinkMapping { .. }
            | Self::DeleteLinkMapping { .. } => ChangeType::TableMerge,

            Self::CreateNotationPattern { .. }
            | Self::UpdateNotationPattern { .. }
            | Self::DeleteNotationPattern { .. } => ChangeType::NotationPatternCreate,

            Self::CreateConceptMap { .. }
            | Self::UpdateConceptMap { .. }
            | Self::DeleteConceptMap { .. }
            | Self::CreateValueSet { .. }
            | Self::UpdateValueSet { .. }
            | Self::DeleteValueSet { .. } => ChangeType::GlossaryTermCreate,
        }
    }

    /// How many codes does this operation add or remove? Feeds the
    /// `ChangeScopeBelow` skip predicate on routing. Zero for
    /// operations that don't touch codes.
    pub fn code_count_delta(&self) -> u32 {
        match self {
            Self::CreateCodedValue { .. }
            | Self::UpdateCodedValue { .. }
            | Self::DeleteCodedValue { .. } => 1,
            Self::CreateCodeSystem { def, .. } | Self::UpdateCodeSystem { def, .. } => {
                def.codes.len() as u32
            }
            Self::DeleteCodeSystem { .. } => {
                // Cascades an unknown number of codes — from the
                // routing standpoint treat as "many" so the scope
                // predicate doesn't auto-approve.
                u32::MAX
            }
            _ => 0,
        }
    }
}

/// Apply the operation in place on a fresh IR clone. Errors return
/// the original `OntologyInvariantError` string so the caller can
/// surface it in the API response verbatim.
impl OntologyEditOp {
    pub fn apply_to(&self, ir: &mut OntologyIR) -> Result<(), String> {
        match self.clone() {
            Self::CreateCodeSystem { def } => {
                ir.add_code_system(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::UpdateCodeSystem { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_code_system: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_code_system(&id).map_err(|e| e.to_string())?;
                ir.add_code_system(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteCodeSystem { id } => ir
                .remove_code_system(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateCodedValue {
                code_system_id,
                value,
            } => {
                let cs = ir
                    .code_system_by_id(&code_system_id)
                    .ok_or_else(|| format!("code_system {} not found", code_system_id.as_str()))?
                    .clone();
                let mut codes = cs.codes.clone();
                if codes.iter().any(|c| c.id == value.id) {
                    return Err(format!("coded_value {} already exists", value.id.as_str()));
                }
                codes.push(value);
                let mut new_cs = cs;
                new_cs.codes = codes;
                ir.remove_code_system(&code_system_id).map_err(|e| e.to_string())?;
                ir.add_code_system(new_cs).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::UpdateCodedValue {
                code_system_id,
                id,
                value,
            } => {
                if value.id != id {
                    return Err(format!(
                        "update_coded_value: value.id ({}) does not match path id ({})",
                        value.id.as_str(),
                        id.as_str(),
                    ));
                }
                let cs = ir
                    .code_system_by_id(&code_system_id)
                    .ok_or_else(|| format!("code_system {} not found", code_system_id.as_str()))?
                    .clone();
                let mut codes = cs.codes.clone();
                let pos = codes
                    .iter()
                    .position(|c| c.id == id)
                    .ok_or_else(|| format!("coded_value {} not found", id.as_str()))?;
                codes[pos] = value;
                let mut new_cs = cs;
                new_cs.codes = codes;
                ir.remove_code_system(&code_system_id).map_err(|e| e.to_string())?;
                ir.add_code_system(new_cs).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteCodedValue {
                code_system_id,
                id,
            } => {
                let cs = ir
                    .code_system_by_id(&code_system_id)
                    .ok_or_else(|| format!("code_system {} not found", code_system_id.as_str()))?
                    .clone();
                let mut codes = cs.codes.clone();
                let len_before = codes.len();
                codes.retain(|c| c.id != id);
                if codes.len() == len_before {
                    return Err(format!("coded_value {} not found", id.as_str()));
                }
                let mut new_cs = cs;
                new_cs.codes = codes;
                ir.remove_code_system(&code_system_id).map_err(|e| e.to_string())?;
                ir.add_code_system(new_cs).map(|_| ()).map_err(|e| e.to_string())
            }

            Self::CreateGlossaryTerm { def } => {
                ir.add_glossary_term(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::UpdateGlossaryTerm { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_glossary_term: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_glossary_term(&id).map_err(|e| e.to_string())?;
                ir.add_glossary_term(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteGlossaryTerm { id } => ir
                .remove_glossary_term(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateObjectMapping { mapping } => ir
                .add_object_mapping(mapping)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::UpdateObjectMapping { id, mapping } => {
                if mapping.id != id {
                    return Err(format!(
                        "update_object_mapping: mapping.id ({}) does not match path id ({})",
                        mapping.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_object_mapping(&id).map_err(|e| e.to_string())?;
                ir.add_object_mapping(mapping).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteObjectMapping { id } => ir
                .remove_object_mapping(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateLinkMapping { mapping } => ir
                .add_link_mapping(mapping)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::UpdateLinkMapping { id, mapping } => {
                if mapping.id != id {
                    return Err(format!(
                        "update_link_mapping: mapping.id ({}) does not match path id ({})",
                        mapping.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_link_mapping(&id).map_err(|e| e.to_string())?;
                ir.add_link_mapping(mapping).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteLinkMapping { id } => ir
                .remove_link_mapping(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateNotationPattern { def } => ir
                .add_notation_pattern(def)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::UpdateNotationPattern { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_notation_pattern: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_notation_pattern(&id).map_err(|e| e.to_string())?;
                ir.add_notation_pattern(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteNotationPattern { id } => ir
                .remove_notation_pattern(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateConceptMap { def } => ir
                .add_concept_map(def)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::UpdateConceptMap { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_concept_map: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_concept_map(&id).map_err(|e| e.to_string())?;
                ir.add_concept_map(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteConceptMap { id } => ir
                .remove_concept_map(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateValueSet { def } => ir
                .add_value_set(def)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::UpdateValueSet { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_value_set: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_value_set(&id).map_err(|e| e.to_string())?;
                ir.add_value_set(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteValueSet { id } => ir
                .remove_value_set(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        }
    }
}

/// Admin edit request. `expected_version` is an optimistic-lock
/// guard — when the current ontology version doesn't match, the
/// handler returns 409 Conflict so the caller can refetch and retry.
/// Operations apply atomically: an error on any op rolls the whole
/// batch back.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyEditRequest {
    pub expected_version: u32,
    pub operations: Vec<OntologyEditOp>,
    /// Free-text commit message. Surfaces in audit log + lineage UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// When `true` the handler applies the operations to a **cloned**
    /// IR, runs `OntologyIR::validate()`, and returns the resulting
    /// [`OntologyEditPreCheck`] instead of committing a new version.
    /// No audit row, no approval routing, no version bump. Useful for
    /// "would this edit break anything?" previews in the admin UI.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dry_run: bool,
}

/// Response from a successful commit. Carries enough data for the UI
/// to update its version pointer without a round-trip refetch of
/// the whole ontology.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyEditReceipt {
    pub new_version: u32,
    pub new_version_id: Uuid,
    pub parent_version_id: Option<Uuid>,
    pub applied_operations: usize,
    pub committed_at: DateTime<Utc>,
}

/// Response for a `dry_run=true` edit. Surfaces validation outcome
/// without touching storage, so the admin UI can render a "would
/// produce N errors" banner before the operator commits.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyEditPreCheck {
    /// Number of operations that would apply cleanly to the in-memory
    /// IR. An operation that fails its `apply_to` step halts the
    /// preview just like a real commit would.
    pub applied_operations: usize,
    /// Index of the first failing operation, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_operation_index: Option<usize>,
    /// Human-readable error from the failed apply step, or `None`
    /// when every op applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
    /// Whole-IR validation errors (`OntologyIR::validate()`),
    /// including dangling registry references (Phase 1.7).
    pub validation_errors: Vec<String>,
    /// Classified `ChangeType` per operation — same classification
    /// the approval-routing layer uses. Lets the UI highlight
    /// "breaking change" previews differently from "safe" ones.
    pub classified_changes: Vec<String>,
    /// `true` when every op applied and validate() returned an empty
    /// error list — equivalent to "safe to commit".
    pub would_commit: bool,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::code_system::CodeSystemKind;
    use ox_core::i18n::LocalizedText;

    fn bare_code_system(id: &str) -> CodeSystemDef {
        CodeSystemDef {
            id: CodeSystemId::new(id),
            name: id.to_string(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            uri: None,
            version: "1".into(),
            kind: CodeSystemKind::Internal,
            hierarchical: false,
            codes: vec![],
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn bare_coded_value(id: &str, code: &str) -> CodedValue {
        CodedValue {
            id: CodedValueId::new(id),
            code: code.to_string(),
            display: LocalizedText::default(),
            definition: LocalizedText::default(),
            aliases: vec![],
            broader_id: None,
            examples: vec![],
            scope_note: LocalizedText::default(),
            valid_from: None,
            valid_to: None,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }

    fn bare_glossary_term(id: &str, term: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: GlossaryTermId::new(id),
            term: term.to_string(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            aliases: vec![],
            category: None,
            parent_term_id: None,
        }
    }

    #[test]
    fn all_create_variants_classify_without_panic() {
        let ops = [
            OntologyEditOp::CreateCodeSystem {
                def: bare_code_system("cs"),
            },
            OntologyEditOp::CreateGlossaryTerm {
                def: bare_glossary_term("g", "term"),
            },
            OntologyEditOp::DeleteCodeSystem {
                id: CodeSystemId::new("cs"),
            },
        ];
        for op in &ops {
            let _ = op.classify_change_type();
            let _ = op.code_count_delta();
        }
    }

    #[test]
    fn delete_code_system_signals_large_scope() {
        let op = OntologyEditOp::DeleteCodeSystem {
            id: CodeSystemId::new("cs-x"),
        };
        // The routing scope predicate must see a large delta so it
        // doesn't auto-approve a blanket cascade.
        assert_eq!(op.code_count_delta(), u32::MAX);
    }

    #[test]
    fn create_coded_value_counts_as_one() {
        let op = OntologyEditOp::CreateCodedValue {
            code_system_id: CodeSystemId::new("cs"),
            value: bare_coded_value("cv", "A"),
        };
        assert_eq!(op.code_count_delta(), 1);
    }

    #[test]
    fn request_roundtrips_through_json() {
        let req = OntologyEditRequest {
            expected_version: 3,
            operations: vec![OntologyEditOp::CreateGlossaryTerm {
                def: bare_glossary_term("g1", "매출"),
            }],
            message: Some("add GMV alias".into()),
            dry_run: false,
        };
        let j = serde_json::to_value(&req).unwrap();
        let back: OntologyEditRequest = serde_json::from_value(j).unwrap();
        assert_eq!(back.expected_version, 3);
        assert_eq!(back.operations.len(), 1);
    }

    #[test]
    fn op_has_internal_kind_tag_on_wire() {
        let op = OntologyEditOp::DeleteGlossaryTerm {
            id: GlossaryTermId::new("g1"),
        };
        let j = serde_json::to_string(&op).unwrap();
        assert!(j.contains("\"op\":\"delete_glossary_term\""));
    }

    #[test]
    fn every_variant_has_a_change_type_mapping() {
        // `match` in classify_change_type() is exhaustive — if a new
        // variant lands without a case, compilation breaks before
        // this test runs. Spot-checks below pin the mapping's shape
        // for the terminology-centric ops; mapping ops are covered
        // indirectly via the exhaustiveness guarantee.
        assert_eq!(
            OntologyEditOp::CreateCodedValue {
                code_system_id: CodeSystemId::new("cs"),
                value: bare_coded_value("cv", "A"),
            }
            .classify_change_type(),
            ChangeType::CodedValueCreate
        );
        assert_eq!(
            OntologyEditOp::DeleteCodedValue {
                code_system_id: CodeSystemId::new("cs"),
                id: CodedValueId::new("cv"),
            }
            .classify_change_type(),
            ChangeType::CodedValueDeprecate
        );
        assert_eq!(
            OntologyEditOp::CreateGlossaryTerm {
                def: bare_glossary_term("g", "t"),
            }
            .classify_change_type(),
            ChangeType::GlossaryTermCreate
        );
    }
}
