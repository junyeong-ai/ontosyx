//! Typed ontology-edit log for the admin surface.
//!
//! Where [`crate::command::OntologyCommand`] targets the LLM agent
//! (and carries a Bedrock-compatible flat JsonSchema), this module's
//! [`OntologyEditOp`] is the fine-grained taxonomy the admin API
//! writes through: per-entity Create / Update / Delete across the
//! Phase-5B governance collections (CodeSystem, Concept,
//! GlossaryTerm, ObjectMapping / LinkMapping, NotationPattern,
//! ConceptMap, ValueSet).
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

use crate::action::RuleId;
use crate::change_routing::ChangeType;
use crate::code_system::{CodeSystemDef, CodeSystemId, CodedValue, CodedValueId};
use crate::concept::{ConceptDef, ConceptId};
use crate::concept_map::{ConceptMapDef, ConceptMapId};
use crate::glossary::{GlossaryTermDef, GlossaryTermId};
use crate::ir::{EdgeTypeId, NodeTypeId, OntologyIR, PropertyId};
use crate::mapping::link::LinkMappingDef;
use crate::mapping::object::ObjectMappingDef;
use crate::mapping::refs::{LinkMappingId, ObjectMappingId};
use crate::notation_pattern::{NotationPatternDef, NotationPatternId};
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

    // --- Concept ---
    CreateConcept {
        def: ConceptDef,
    },
    UpdateConcept {
        id: ConceptId,
        def: ConceptDef,
    },
    DeleteConcept {
        id: ConceptId,
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
    // These variants write semantic bindings on PropertyDef.
    /// Append a [`PropertyBinding`] to the named property. If a
    /// binding with the same target already exists it is replaced
    /// (so the call is idempotent on `target` identity, not on the
    /// strength / concept-map / window fields). The target's id
    /// must resolve in its registry — the OntologyIR-level
    /// `validate()` surfaces a dangling-ref if it doesn't.
    BindProperty {
        owner: PropertyOwnerPath,
        property_id: PropertyId,
        binding: crate::binding::PropertyBinding,
    },
    /// Remove every binding on the named property whose
    /// `(kind, id)` matches `target`. No-op if no binding matches.
    UnbindProperty {
        owner: PropertyOwnerPath,
        property_id: PropertyId,
        target: crate::binding::PropertyBindingHandle,
    },
}

impl OntologyEditOp {
    /// Map this edit to its `ChangeType` for routing. The mapping is
    /// deliberately many-to-one: every CodedValue lifecycle op lands
    /// on `CodedValueCreate` or `CodedValueDeprecate`, terminology
    /// registry CRUD lands on `TerminologyRegistryUpdate`, and
    /// property-level semantic links land on `SemanticBindingUpdate`.
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

            Self::CreateConcept { .. }
            | Self::UpdateConcept { .. }
            | Self::DeleteConcept { .. }
            | Self::CreateGlossaryTerm { .. }
            | Self::UpdateGlossaryTerm { .. }
            | Self::DeleteGlossaryTerm { .. } => ChangeType::TerminologyRegistryUpdate,

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
            | Self::DeleteValueSet { .. } => ChangeType::TerminologyRegistryUpdate,

            Self::BindProperty { .. } | Self::UnbindProperty { .. } => {
                ChangeType::SemanticBindingUpdate
            }

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
            Self::CreateCodeSystem { def } => ir
                .add_code_system(def)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::UpdateCodeSystem { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_code_system: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.replace_code_system(&id, def)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
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
                ir.replace_code_system(&code_system_id, new_cs)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
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
                ir.replace_code_system(&code_system_id, new_cs)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Self::DeleteCodedValue { code_system_id, id } => {
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
                ir.replace_code_system(&code_system_id, new_cs)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }

            Self::CreateConcept { def } => {
                ir.add_concept(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::UpdateConcept { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_concept: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.replace_concept(&id, def)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Self::DeleteConcept { id } => ir
                .remove_concept(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateGlossaryTerm { def } => ir
                .add_glossary_term(def)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Self::UpdateGlossaryTerm { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_glossary_term: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.replace_glossary_term(&id, def)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
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
                ir.replace_object_mapping(&id, mapping)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
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
                ir.replace_link_mapping(&id, mapping)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
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
                ir.replace_notation_pattern(&id, def)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
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
                ir.replace_concept_map(&id, def)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Self::DeleteConceptMap { id } => ir
                .remove_concept_map(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateValueSet { def } => {
                ir.add_value_set(def).map(|_| ()).map_err(|e| e.to_string())
            }
            Self::UpdateValueSet { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_value_set: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.replace_value_set(&id, def)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Self::DeleteValueSet { id } => ir
                .remove_value_set(&id)
                .map(|_| ())
                .map_err(|e| e.to_string()),

            Self::CreateRule { def } => ir.add_rule(def).map(|_| ()).map_err(|e| e.to_string()),
            Self::UpdateRule { id, def } => {
                if def.id != id {
                    return Err(format!(
                        "update_rule: def.id ({}) does not match path id ({})",
                        def.id.as_str(),
                        id.as_str(),
                    ));
                }
                ir.replace_rule(&id, def)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
            Self::DeleteRule { id } => ir.remove_rule(&id).map(|_| ()).map_err(|e| e.to_string()),

            Self::BindProperty {
                owner,
                property_id,
                binding,
            } => mutate_property(ir, &owner, &property_id, |p| {
                // Idempotent on `(kind, id)`: replace if present, push
                // otherwise. Strength / window / concept-map fields
                // ride along.
                let handle = binding.handle();
                if let Some(slot) = p.bindings.iter_mut().find(|b| b.handle() == handle) {
                    *slot = binding;
                } else {
                    p.bindings.push(binding);
                }
            }),
            Self::UnbindProperty {
                owner,
                property_id,
                target,
            } => mutate_property(ir, &owner, &property_id, |p| {
                p.bindings.retain(|b| b.handle() != target);
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
pub struct EditOntologyRequest {
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
    /// including dangling registry references. Each entry is a
    /// structured [`ox_core::DiagnosticMessage`] — the FE renders
    /// `code` + `params` through its i18n catalogue.
    pub validation_errors: Vec<ox_core::DiagnosticMessage>,
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
            term: LocalizedText::new(term),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            examples: Vec::new(),
            aliases: Vec::new(),
            related_terms: Vec::new(),
            category: None,
            governance: crate::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            concept_id: None,
            term_pos: Default::default(),
        }
    }

    fn bare_concept(id: &str, term_id: &str) -> ConceptDef {
        ConceptDef {
            id: ConceptId::new(id),
            canonical_term_id: GlossaryTermId::new(term_id),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: crate::concept::ConceptGovernance::default(),
        }
    }

    fn bare_object_mapping(id: &str, relation: &str) -> ObjectMappingDef {
        ObjectMappingDef::new(id, "Customer", "pg-main", relation)
    }

    fn bare_link_mapping(id: &str, predicate: &str) -> LinkMappingDef {
        use crate::mapping::{
            EndpointRef, JoinCostHint, LinkCardinality, LinkMappingKind, SourceId,
        };

        let endpoint = EndpointRef {
            source_id: SourceId::new("pg-main"),
            relation: "customers".to_string(),
            key_columns: vec!["id".to_string()],
        };
        LinkMappingDef {
            id: LinkMappingId::new(id),
            edge_type_id: EdgeTypeId::new("KNOWS"),
            kind: LinkMappingKind::Computed {
                predicate: predicate.to_string(),
            },
            source_endpoint: endpoint.clone(),
            target_endpoint: endpoint,
            join_cost_hint: JoinCostHint::Unknown,
            precedence: 0,
            cardinality: LinkCardinality::ManyToMany,
        }
    }

    #[test]
    fn all_create_variants_classify_without_panic() {
        let ops = [
            OntologyEditOp::CreateCodeSystem {
                def: bare_code_system("cs"),
            },
            OntologyEditOp::CreateConcept {
                def: bare_concept("c-term", "g"),
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
        let req = EditOntologyRequest {
            expected_version: 3,
            operations: vec![OntologyEditOp::CreateGlossaryTerm {
                def: bare_glossary_term("g1", "매출"),
            }],
            message: Some("add GMV alias".into()),
            dry_run: false,
        };
        let j = serde_json::to_value(&req).unwrap();
        let back: EditOntologyRequest = serde_json::from_value(j).unwrap();
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
            OntologyEditOp::CreateConcept {
                def: bare_concept("c-g", "g"),
            }
            .classify_change_type(),
            ChangeType::TerminologyRegistryUpdate
        );
        assert_eq!(
            OntologyEditOp::CreateGlossaryTerm {
                def: bare_glossary_term("g", "t"),
            }
            .classify_change_type(),
            ChangeType::TerminologyRegistryUpdate
        );
        assert_eq!(
            OntologyEditOp::BindProperty {
                owner: PropertyOwnerPath::Node {
                    type_id: NodeTypeId::new("Customer"),
                },
                property_id: PropertyId::new("tier"),
                binding: crate::binding::PropertyBinding::concept(crate::concept::ConceptId::new(
                    "vip"
                ),),
            }
            .classify_change_type(),
            ChangeType::SemanticBindingUpdate
        );
    }

    // ------------------------------------------------------------------
    // Property-binding ops.
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
        use crate::concept::{ConceptDef, ConceptGovernance, ConceptId};
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
        let mut term = bare_glossary_term("vip", "VIP");
        term.concept_id = Some(ConceptId::new("c-vip"));
        ir.add_glossary_term(term).unwrap();
        ir.add_concept(ConceptDef {
            id: ConceptId::new("c-vip"),
            canonical_term_id: GlossaryTermId::new("vip"),
            alias_term_ids: Vec::new(),
            broader: None,
            description: LocalizedText::default(),
            examples: Vec::new(),
            category: None,
            realisation: None,
            lifecycle: crate::glossary::TermLifecycle::default(),
            replaced_by: None,
            valid_from: None,
            valid_to: None,
            governance: ConceptGovernance::default(),
        })
        .unwrap();
        ir
    }

    fn concept_binding(id: &str) -> crate::binding::PropertyBinding {
        crate::binding::PropertyBinding::concept(crate::concept::ConceptId::new(id))
    }

    fn sample_ir_with_concepts() -> OntologyIR {
        let mut ir = OntologyIR::new(
            "ont-test".to_string(),
            "Test".to_string(),
            LocalizedText::default(),
            1,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        for (term_id, label, concept_id) in [
            ("gt-party", "Party", "c-party"),
            ("gt-customer", "Customer", "c-customer"),
            ("gt-client", "Client", "c-client"),
        ] {
            let mut term = bare_glossary_term(term_id, label);
            term.concept_id = Some(ConceptId::new(concept_id));
            ir.add_glossary_term(term).unwrap();
        }
        ir.add_concept(bare_concept("c-party", "gt-party")).unwrap();
        let mut customer = bare_concept("c-customer", "gt-customer");
        customer.broader = Some(ConceptId::new("c-party"));
        ir.add_concept(customer).unwrap();
        ir.add_concept(bare_concept("c-client", "gt-client"))
            .unwrap();
        ir
    }

    #[test]
    fn create_concept_op_adds_registry_concept() {
        let mut ir = OntologyIR::new(
            "ont-test".to_string(),
            "Test".to_string(),
            LocalizedText::default(),
            1,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let mut term = bare_glossary_term("gt-customer", "Customer");
        term.concept_id = Some(ConceptId::new("c-customer"));
        ir.add_glossary_term(term).unwrap();

        OntologyEditOp::CreateConcept {
            def: bare_concept("c-customer", "gt-customer"),
        }
        .apply_to(&mut ir)
        .unwrap();

        assert_eq!(
            ir.concept_by_id(&ConceptId::new("c-customer"))
                .unwrap()
                .canonical_term_id
                .as_str(),
            "gt-customer"
        );
    }

    #[test]
    fn update_concept_op_uses_atomic_replace() {
        let mut ir = sample_ir_with_concepts();
        let mut replacement = bare_concept("c-party", "gt-party");
        replacement.broader = Some(ConceptId::new("c-customer"));

        let err = OntologyEditOp::UpdateConcept {
            id: ConceptId::new("c-party"),
            def: replacement,
        }
        .apply_to(&mut ir)
        .unwrap_err();

        assert!(err.contains("concept.broader.cycle"));
        assert!(
            ir.concept_by_id(&ConceptId::new("c-party"))
                .unwrap()
                .broader
                .is_none()
        );
    }

    #[test]
    fn delete_concept_op_rejects_referenced_concept() {
        let mut ir = sample_ir_with_concepts();

        let err = OntologyEditOp::DeleteConcept {
            id: ConceptId::new("c-party"),
        }
        .apply_to(&mut ir)
        .unwrap_err();

        assert!(err.contains("concept.in_use_by_glossary_term"));
        assert!(ir.concept_by_id(&ConceptId::new("c-party")).is_some());
    }

    #[test]
    fn update_object_mapping_uses_atomic_replace() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::CreateObjectMapping {
            mapping: bare_object_mapping("om-customer", "customers"),
        }
        .apply_to(&mut ir)
        .unwrap();

        OntologyEditOp::UpdateObjectMapping {
            id: ObjectMappingId::new("om-customer"),
            mapping: bare_object_mapping("om-customer", "crm_customers"),
        }
        .apply_to(&mut ir)
        .unwrap();

        assert_eq!(ir.object_mappings().len(), 1);
        assert_eq!(ir.object_mappings()[0].relation, "crm_customers");
    }

    #[test]
    fn update_link_mapping_uses_atomic_replace() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::CreateLinkMapping {
            mapping: bare_link_mapping("lm-knows", "a.id = b.id"),
        }
        .apply_to(&mut ir)
        .unwrap();

        OntologyEditOp::UpdateLinkMapping {
            id: LinkMappingId::new("lm-knows"),
            mapping: bare_link_mapping("lm-knows", "a.manager_id = b.id"),
        }
        .apply_to(&mut ir)
        .unwrap();

        assert_eq!(ir.link_mappings().len(), 1);
        assert!(matches!(
            &ir.link_mappings()[0].kind,
            crate::mapping::LinkMappingKind::Computed { predicate }
                if predicate == "a.manager_id = b.id"
        ));
    }

    #[test]
    fn bind_property_appends_to_bindings_list() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::BindProperty {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-tier"),
            binding: concept_binding("c-vip"),
        }
        .apply_to(&mut ir)
        .unwrap();
        let prop = &ir.node_types()[0].properties[0];
        assert_eq!(prop.concept_id().unwrap().as_str(), "c-vip");
    }

    #[test]
    fn unbind_property_removes_matching_target() {
        let mut ir = sample_ir_with_property();
        OntologyEditOp::BindProperty {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-tier"),
            binding: concept_binding("c-vip"),
        }
        .apply_to(&mut ir)
        .unwrap();
        OntologyEditOp::UnbindProperty {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-tier"),
            target: crate::binding::PropertyBindingHandle::Concept {
                id: crate::concept::ConceptId::new("c-vip"),
            },
        }
        .apply_to(&mut ir)
        .unwrap();
        assert!(ir.node_types()[0].properties[0].concept_id().is_none());
    }

    #[test]
    fn bind_property_missing_property_surfaces_error() {
        let mut ir = sample_ir_with_property();
        let err = OntologyEditOp::BindProperty {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("p-missing"),
            binding: concept_binding("c-vip"),
        }
        .apply_to(&mut ir)
        .unwrap_err();
        assert!(err.contains("property `p-missing` not found"));
    }

    // ------------------------------------------------------------------
    // Type-deprecation ops.
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
        assert_eq!(node.replaced_by_id.as_ref().unwrap().as_str(), "CustomerV2");
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
        use crate::rule::{RuleActivationKind, RuleKind, RuleOrigin};
        RuleDef {
            id: RuleId::new(id),
            name: LocalizedText::new(name),
            description: LocalizedText::default(),
            rationale: LocalizedText::default(),
            kind: RuleKind::CrossEntityShape {
                predicate: "1 = 1".to_string(),
            },
            severity: Default::default(),
            enforcement: Default::default(),
            activation: RuleActivationKind::Always,
            origin: RuleOrigin::Authored,
            constraints: Vec::new(),
            valid_from: None,
            valid_to: None,
            sh_message: None,
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
        assert_eq!(ir.rules()[0].name.default, "second");
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
    fn bind_property_wire_format_is_snake_case() {
        let op = OntologyEditOp::BindProperty {
            owner: PropertyOwnerPath::Node {
                type_id: NodeTypeId::new("Customer"),
            },
            property_id: PropertyId::new("tier"),
            binding: concept_binding("c-vip"),
        };
        let j = serde_json::to_string(&op).unwrap();
        assert!(j.contains("\"op\":\"bind_property\""));
        // owner discriminator
        assert!(j.contains("\"kind\":\"node\""));
        assert!(j.contains("\"type_id\":\"Customer\""));
        // binding variant discriminator (PropertyBinding is a tagged
        // enum on `kind`, with the binding kind under "binding"):
        assert!(j.contains("\"binding\""));
        assert!(j.contains("\"kind\":\"concept\""));
        assert!(j.contains("\"id\":\"c-vip\""));
    }
}
