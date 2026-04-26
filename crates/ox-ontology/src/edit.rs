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
use crate::ir::{EdgeTypeId, NodeTypeId, OntologyIR, PropertyId};
use crate::mapping::link::LinkMappingDef;
use crate::mapping::object::ObjectMappingDef;
use crate::mapping::refs::{LinkMappingId, ObjectMappingId};
use crate::notation_pattern::{NotationPatternDef, NotationPatternId};
use crate::action::RuleId;
use crate::rule::RuleDef;
use crate::value_set::{ValueSetDef, ValueSetId};

/// Which side of the ontology topology a property lives on.
///
/// Used by the property-level registry-binding edits so the
/// operation carries both the type kind and the type id without
/// threading two separate enum variants per op.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyOwnerPath {
    Node { type_id: NodeTypeId },
    Edge { type_id: EdgeTypeId },
}

/// Typed ontology edit. Each variant names exactly one entity kind;
/// a batch of variants applies atomically (all-or-nothing) and is
/// validated together at the end.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
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

    // --- Rule (SHACL-style validation shape) ---
    //
    // Rules are first-class on `OntologyIR.rules`: they drive
    // `ShaclValidator` and feed the Phase-3 SHACL pass-rate signal.
    // Exposing CRUD through `OntologyEditOp` keeps the edit log as
    // the single audit trail for every governance change — a rule
    // addition shows up in the version log next to the glossary term
    // it guards.
    /// Declare a new [`RuleDef`]. Fails if another rule already
    /// carries the same id (enforced by `add_rule`'s
    /// referential-integrity check).
    CreateRule {
        def: RuleDef,
    },
    /// Replace a [`RuleDef`] in place. Semantically identical to a
    /// Delete + Create pair but atomic at the apply step; validation
    /// runs on the post-replace IR.
    UpdateRule {
        id: RuleId,
        def: RuleDef,
    },
    /// Remove a [`RuleDef`] by id. Removes the corresponding
    /// constraint from SHACL validation — reviewers should
    /// understand the coverage loss before approving.
    DeleteRule {
        id: RuleId,
    },

    // --- Type deprecation ---
    //
    // Emitted when an admin approves a `StaleConceptProposal` (Phase
    // 4.7). The handler writes `deprecated_at = now()` plus an
    // optional `replaced_by_id` on the target type; downstream tools
    // treat the timestamp as a soft-delete marker, so queries
    // against historical data still resolve labels.
    /// Mark a NodeType as deprecated. Stamps `deprecated_at` with
    /// the current UTC time; `replaced_by_id`, when present,
    /// records the successor so temporal queries can chain through.
    DeprecateNodeType {
        id: NodeTypeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replaced_by_id: Option<NodeTypeId>,
    },
    /// Mark an EdgeType as deprecated. Same shape as the node-type
    /// variant.
    DeprecateEdgeType {
        id: EdgeTypeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replaced_by_id: Option<EdgeTypeId>,
    },

    // --- Property → registry bindings ---
    //
    // These variants write single-field pointers on PropertyDef
    // (`glossary_term_id`, `value_set_id`, `notation_pattern_id`).
    // Passing `Some(id)` sets the binding, `None` clears it. Paired
    // with the Phase 1 suggestion endpoints so the admin UI can
    // "suggest → confirm → persist" in one pipeline.
    /// Attach (or detach, via `None`) a [`GlossaryTermDef`] to the
    /// named property. The term must already exist — the
    /// OntologyIR-level `validate()` surfaces a dangling-ref if it
    /// doesn't.
    BindPropertyToTerm {
        owner: PropertyOwnerPath,
        property_id: PropertyId,
        glossary_term_id: Option<GlossaryTermId>,
    },
    /// Attach (or detach, via `None`) a [`ValueSetDef`] to the named
    /// property. Runtime `ShaclValidator` uses the pointer to enforce
    /// an `InValueSet` constraint at write time.
    BindPropertyToValueSet {
        owner: PropertyOwnerPath,
        property_id: PropertyId,
        value_set_id: Option<ValueSetId>,
    },
    /// Attach (or detach, via `None`) a
    /// [`NotationPatternDef`] to the named property. Runtime
    /// `ShaclValidator` uses the pointer to enforce
    /// `MatchesPattern`.
    BindPropertyToNotationPattern {
        owner: PropertyOwnerPath,
        property_id: PropertyId,
        notation_pattern_id: Option<NotationPatternId>,
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

            // Property-level registry bindings route as
            // `GlossaryAliasAdd` (70% auto per patent matrix). The
            // data semantics are identical to adding / removing an
            // alias on a glossary term — the pointer is a reach
            // extension, not a structural edit.
            Self::BindPropertyToTerm { .. }
            | Self::BindPropertyToValueSet { .. }
            | Self::BindPropertyToNotationPattern { .. } => ChangeType::GlossaryAliasAdd,

            // Type deprecation is the exact matrix row for
            // `StaleConceptDeprecate` — 95% auto, which fits because
            // the proposal layer already did HITL review; this is
            // the mechanical write-back.
            Self::DeprecateNodeType { .. } | Self::DeprecateEdgeType { .. } => {
                ChangeType::StaleConceptDeprecate
            }

            // Rule lifecycle maps 1:1 onto the three rule matrix
            // rows (65% / 55% / always-queue) — keeping Create /
            // Modify / Delete as distinct ChangeTypes is what lets
            // the matrix treat rule deletion stricter than
            // modification.
            Self::CreateRule { .. } => ChangeType::RuleCreate,
            Self::UpdateRule { .. } => ChangeType::RuleModify,
            Self::DeleteRule { .. } => ChangeType::RuleDelete,
        }
    }

    /// Scope metrics this operation declares. Feeds the
    /// [`crate::change_routing::ApprovalSkipPredicate::ChangeScopeBelow`]
    /// predicate on routing. Ops that don't declare a scope
    /// (most non-CodedValue variants) return an empty vector —
    /// "no scope" is routed distinctly from "zero-sized scope" so
    /// an untracked op can't accidentally slip past scope gates.
    pub fn scopes(&self) -> Vec<crate::change_routing::ScopeValue> {
        use crate::change_routing::{ScopeKind, ScopeValue};
        match self {
            Self::CreateCodedValue { .. }
            | Self::UpdateCodedValue { .. }
            | Self::DeleteCodedValue { .. } => {
                vec![ScopeValue {
                    kind: ScopeKind::CodeCount,
                    value: 1,
                }]
            }
            Self::CreateCodeSystem { def, .. } | Self::UpdateCodeSystem { def, .. } => {
                vec![ScopeValue {
                    kind: ScopeKind::CodeCount,
                    value: def.codes.len() as u32,
                }]
            }
            Self::DeleteCodeSystem { .. } => {
                // Cascades an unknown number of codes — from the
                // routing standpoint treat as "many" so the scope
                // predicate doesn't auto-approve.
                vec![ScopeValue {
                    kind: ScopeKind::CodeCount,
                    value: u32::MAX,
                }]
            }
            _ => Vec::new(),
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

            Self::CreateRule { def } => {
                ir.add_rule(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::UpdateRule { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_rule: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.remove_rule(&id).map_err(|e| e.to_string())?;
                ir.add_rule(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::DeleteRule { id } => ir
                .remove_rule(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::BindPropertyToTerm {
                owner,
                property_id,
                glossary_term_id,
            } => mutate_property(ir, &owner, &property_id, |p| {
                p.glossary_term_id = glossary_term_id;
            }),
            Self::BindPropertyToValueSet {
                owner,
                property_id,
                value_set_id,
            } => mutate_property(ir, &owner, &property_id, |p| {
                p.value_set_id = value_set_id;
            }),
            Self::BindPropertyToNotationPattern {
                owner,
                property_id,
                notation_pattern_id,
            } => mutate_property(ir, &owner, &property_id, |p| {
                p.notation_pattern_id = notation_pattern_id;
            }),

            Self::DeprecateNodeType { id, replaced_by_id } => {
                let now = chrono::Utc::now();
                let node = ir
                    .node_types_mut()
                    .iter_mut()
                    .find(|n| n.id == id)
                    .ok_or_else(|| format!("node_type `{}` not found", id.as_str()))?;
                node.deprecated_at = Some(now);
                node.replaced_by_id = replaced_by_id;
                Ok(())
            }
            Self::DeprecateEdgeType { id, replaced_by_id } => {
                let now = chrono::Utc::now();
                let edge = ir
                    .edge_types_mut()
                    .iter_mut()
                    .find(|e| e.id == id)
                    .ok_or_else(|| format!("edge_type `{}` not found", id.as_str()))?;
                edge.deprecated_at = Some(now);
                edge.replaced_by_id = replaced_by_id;
                Ok(())
            }
        }
    }
}

/// Locate a property on the given owner and apply `mutate`. Returns
/// a descriptive `Err(String)` matching the taste of the rest of the
/// file when the owner or property id doesn't resolve.
fn mutate_property(
    ir: &mut OntologyIR,
    owner: &PropertyOwnerPath,
    property_id: &PropertyId,
    mutate: impl FnOnce(&mut crate::ir::PropertyDef),
) -> Result<(), String> {
    let (location, prop) = match owner {
        PropertyOwnerPath::Node { type_id } => {
            let node = ir
                .node_types_mut()
                .iter_mut()
                .find(|n| n.id == *type_id)
                .ok_or_else(|| format!("node_type `{}` not found", type_id.as_str()))?;
            (
                format!("node:{}", type_id.as_str()),
                node.properties.iter_mut().find(|p| p.id == *property_id),
            )
        }
        PropertyOwnerPath::Edge { type_id } => {
            let edge = ir
                .edge_types_mut()
                .iter_mut()
                .find(|e| e.id == *type_id)
                .ok_or_else(|| format!("edge_type `{}` not found", type_id.as_str()))?;
            (
                format!("edge:{}", type_id.as_str()),
                edge.properties.iter_mut().find(|p| p.id == *property_id),
            )
        }
    };
    let Some(prop) = prop else {
        return Err(format!(
            "property `{}` not found on {}",
            property_id.as_str(),
            location
        ));
    };
    mutate(prop);
    Ok(())
}

/// Admin edit request. `expected_version` is an optimistic-lock
/// guard — when the current ontology version doesn't match, the
/// handler returns 409 Conflict so the caller can refetch and retry.
/// Operations apply atomically: an error on any op rolls the whole
/// batch back.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
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
            let _ = op.scopes();
        }
    }

    #[test]
    fn delete_code_system_signals_large_scope() {
        use crate::change_routing::ScopeKind;
        let op = OntologyEditOp::DeleteCodeSystem {
            id: CodeSystemId::new("cs-x"),
        };
        // The routing scope predicate must see a large delta so it
        // doesn't auto-approve a blanket cascade.
        let scopes = op.scopes();
        let code = scopes
            .iter()
            .find(|s| s.kind == ScopeKind::CodeCount)
            .expect("DeleteCodeSystem declares a CodeCount scope");
        assert_eq!(code.value, u32::MAX);
    }

    #[test]
    fn create_coded_value_counts_as_one() {
        use crate::change_routing::ScopeKind;
        let op = OntologyEditOp::CreateCodedValue {
            code_system_id: CodeSystemId::new("cs"),
            value: bare_coded_value("cv", "A"),
        };
        let scopes = op.scopes();
        let code = scopes
            .iter()
            .find(|s| s.kind == ScopeKind::CodeCount)
            .expect("CreateCodedValue declares a CodeCount scope");
        assert_eq!(code.value, 1);
    }

    #[test]
    fn glossary_term_has_no_scope() {
        // Non-CodedValue ops don't declare a scope — the routing
        // layer treats "no matching scope" distinctly from
        // "zero-sized scope", so `ChangeScopeBelow` doesn't fire
        // for these ops.
        let op = OntologyEditOp::CreateGlossaryTerm {
            def: bare_glossary_term("g", "term"),
        };
        assert!(op.scopes().is_empty());
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
        assert_eq!(
            OntologyEditOp::BindPropertyToTerm {
                owner: PropertyOwnerPath::Node {
                    type_id: NodeTypeId::new("Customer"),
                },
                property_id: PropertyId::new("tier"),
                glossary_term_id: Some(GlossaryTermId::new("vip")),
            }
            .classify_change_type(),
            ChangeType::GlossaryAliasAdd
        );
    }

    // ------------------------------------------------------------------
    // Property-binding ops (Phase 4.5 backend half).
    // ------------------------------------------------------------------

    fn bare_property(id: &str, name: &str) -> crate::ir::PropertyDef {
        crate::ir::PropertyDef {
            id: PropertyId::new(id),
            name: ox_core::PropertyKey::new(name).unwrap(),
            property_type: ox_core::types::PropertyType::String,
            ..Default::default()
        }
    }

    fn sample_ir_with_property() -> OntologyIR {
        use crate::ir::NodeTypeDef;
        let node = NodeTypeDef {
            id: NodeTypeId::new("Customer"),
            label: ox_core::GraphLabel::new("Customer").unwrap(),
            properties: vec![bare_property("p-tier", "tier")],
            ..Default::default()
        };
        let mut ir = OntologyIR::new(
            "ont-test".to_string(),
            "Test".to_string(),
            LocalizedText::default(),
            1,
            vec![node],
            Vec::new(),
            Vec::new(),
        );
        ir.add_glossary_term(bare_glossary_term("vip", "VIP"))
            .unwrap();
        ir
    }

    #[test]
    fn bind_property_to_term_sets_pointer() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::BindPropertyToTerm {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-tier"),
            glossary_term_id: Some(GlossaryTermId::new("vip")),
        }
        .apply_to(&mut ir)
        .unwrap();
        let prop = &ir.node_types()[0].properties[0];
        assert_eq!(prop.glossary_term_id.as_ref().unwrap().as_str(), "vip");
    }

    #[test]
    fn bind_property_to_term_with_none_unbinds() {
        let mut ir = sample_ir_with_property();
        // First bind.
        OntologyEditOp::BindPropertyToTerm {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-tier"),
            glossary_term_id: Some(GlossaryTermId::new("vip")),
        }
        .apply_to(&mut ir)
        .unwrap();
        // Then unbind via None.
        OntologyEditOp::BindPropertyToTerm {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-tier"),
            glossary_term_id: None,
        }
        .apply_to(&mut ir)
        .unwrap();
        assert!(ir.node_types()[0].properties[0].glossary_term_id.is_none());
    }

    #[test]
    fn bind_property_missing_property_surfaces_error() {
        let mut ir = sample_ir_with_property();
        let err = OntologyEditOp::BindPropertyToTerm {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-missing"),
            glossary_term_id: Some(GlossaryTermId::new("vip")),
        }
        .apply_to(&mut ir)
        .unwrap_err();
        assert!(err.contains("property `p-missing` not found"));
    }

    // ------------------------------------------------------------------
    // Type-deprecation ops (Phase 4.7 follow-up).
    // ------------------------------------------------------------------

    #[test]
    fn deprecate_node_type_stamps_timestamp_and_replacement() {
        let mut ir = sample_ir_with_property();
        let before = chrono::Utc::now();
        OntologyEditOp::DeprecateNodeType {
            id: NodeTypeId::new("Customer"),
            replaced_by_id: Some(NodeTypeId::new("CustomerV2")),
        }
        .apply_to(&mut ir)
        .unwrap();
        let node = &ir.node_types()[0];
        let ts = node.deprecated_at.expect("deprecated_at set");
        assert!(ts >= before);
        assert_eq!(
            node.replaced_by_id.as_ref().unwrap().as_str(),
            "CustomerV2"
        );
    }

    #[test]
    fn deprecate_node_type_without_replacement_still_sets_timestamp() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::DeprecateNodeType {
            id: NodeTypeId::new("Customer"),
            replaced_by_id: None,
        }
        .apply_to(&mut ir)
        .unwrap();
        let node = &ir.node_types()[0];
        assert!(node.deprecated_at.is_some());
        assert!(node.replaced_by_id.is_none());
    }

    #[test]
    fn deprecate_missing_node_type_surfaces_error() {
        let mut ir = sample_ir_with_property();
        let err = OntologyEditOp::DeprecateNodeType {
            id: NodeTypeId::new("Missing"),
            replaced_by_id: None,
        }
        .apply_to(&mut ir)
        .unwrap_err();
        assert!(err.contains("node_type `Missing` not found"));
    }

    #[test]
    fn deprecate_type_classifies_as_stale_concept_deprecate() {
        assert_eq!(
            OntologyEditOp::DeprecateNodeType {
                id: NodeTypeId::new("Customer"),
                replaced_by_id: None,
            }
            .classify_change_type(),
            ChangeType::StaleConceptDeprecate,
        );
        assert_eq!(
            OntologyEditOp::DeprecateEdgeType {
                id: EdgeTypeId::new("OWNS"),
                replaced_by_id: None,
            }
            .classify_change_type(),
            ChangeType::StaleConceptDeprecate,
        );
    }

    #[test]
    fn deprecate_node_type_wire_format_is_snake_case() {
        let op = OntologyEditOp::DeprecateNodeType {
            id: NodeTypeId::new("Customer"),
            replaced_by_id: Some(NodeTypeId::new("CustomerV2")),
        };
        let j = serde_json::to_string(&op).unwrap();
        assert!(j.contains("\"op\":\"deprecate_node_type\""));
        assert!(j.contains("\"replaced_by_id\":\"CustomerV2\""));
    }

    // ------------------------------------------------------------------
    // Rule lifecycle ops.
    // ------------------------------------------------------------------

    fn bare_rule(id: &str, name: &str) -> RuleDef {
        use crate::rule::{RuleActivationKind, RuleKind};
        RuleDef {
            id: RuleId::new(id),
            name: name.to_string(),
            description: LocalizedText::default(),
            kind: RuleKind::CrossEntityShape {
                predicate: "1 = 1".to_string(),
            },
            severity: Default::default(),
            enforcement: Default::default(),
            activation: RuleActivationKind::Always,
            constraints: Vec::new(),
        }
    }

    #[test]
    fn create_rule_appends_to_ir() {
        let mut ir = sample_ir_with_property();
        assert!(ir.rules().is_empty());
        OntologyEditOp::CreateRule {
            def: bare_rule("r-1", "first"),
        }
        .apply_to(&mut ir)
        .unwrap();
        assert_eq!(ir.rules().len(), 1);
        assert_eq!(ir.rules()[0].id.as_str(), "r-1");
    }

    #[test]
    fn create_duplicate_rule_fails_on_apply() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::CreateRule {
            def: bare_rule("r-1", "first"),
        }
        .apply_to(&mut ir)
        .unwrap();
        let err = OntologyEditOp::CreateRule {
            def: bare_rule("r-1", "dup"),
        }
        .apply_to(&mut ir)
        .unwrap_err();
        assert!(err.to_lowercase().contains("rule"));
    }

    #[test]
    fn update_rule_replaces_in_place() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::CreateRule {
            def: bare_rule("r-1", "first"),
        }
        .apply_to(&mut ir)
        .unwrap();
        OntologyEditOp::UpdateRule {
            id: RuleId::new("r-1"),
            def: bare_rule("r-1", "second"),
        }
        .apply_to(&mut ir)
        .unwrap();
        assert_eq!(ir.rules().len(), 1);
        assert_eq!(ir.rules()[0].name, "second");
    }

    #[test]
    fn update_rule_path_id_mismatch_rejected() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::CreateRule {
            def: bare_rule("r-1", "first"),
        }
        .apply_to(&mut ir)
        .unwrap();
        let err = OntologyEditOp::UpdateRule {
            id: RuleId::new("r-1"),
            def: bare_rule("r-2", "second"),
        }
        .apply_to(&mut ir)
        .unwrap_err();
        assert!(err.contains("does not match path id"));
    }

    #[test]
    fn delete_rule_removes_entry() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::CreateRule {
            def: bare_rule("r-1", "first"),
        }
        .apply_to(&mut ir)
        .unwrap();
        OntologyEditOp::DeleteRule {
            id: RuleId::new("r-1"),
        }
        .apply_to(&mut ir)
        .unwrap();
        assert!(ir.rules().is_empty());
    }

    #[test]
    fn delete_missing_rule_surfaces_error() {
        let mut ir = sample_ir_with_property();
        let err = OntologyEditOp::DeleteRule {
            id: RuleId::new("r-missing"),
        }
        .apply_to(&mut ir)
        .unwrap_err();
        assert!(err.contains("not found") || err.contains("r-missing"));
    }

    #[test]
    fn rule_ops_classify_onto_matrix_rows() {
        assert_eq!(
            OntologyEditOp::CreateRule {
                def: bare_rule("r", "n"),
            }
            .classify_change_type(),
            ChangeType::RuleCreate,
        );
        assert_eq!(
            OntologyEditOp::UpdateRule {
                id: RuleId::new("r"),
                def: bare_rule("r", "n"),
            }
            .classify_change_type(),
            ChangeType::RuleModify,
        );
        assert_eq!(
            OntologyEditOp::DeleteRule {
                id: RuleId::new("r"),
            }
            .classify_change_type(),
            ChangeType::RuleDelete,
        );
    }

    #[test]
    fn create_rule_wire_format_is_snake_case() {
        let op = OntologyEditOp::CreateRule {
            def: bare_rule("r-1", "required-customer-email"),
        };
        let j = serde_json::to_string(&op).unwrap();
        assert!(j.contains("\"op\":\"create_rule\""));
    }

    #[test]
    fn bind_property_to_term_wire_format_is_snake_case() {
        let op = OntologyEditOp::BindPropertyToTerm {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("tier"),
            glossary_term_id: Some(GlossaryTermId::new("vip")),
        };
        let j = serde_json::to_string(&op).unwrap();
        assert!(j.contains("\"op\":\"bind_property_to_term\""));
        assert!(j.contains("\"kind\":\"node\""));
        assert!(j.contains("\"type_id\":\"Customer\""));
    }
}
