mod execute;
mod reconcile;
#[cfg(test)]
mod tests;

pub use reconcile::{
    DeletedEntity, GeneratedEntity, MatchDecision, PreservedEntity, ReconcileConfidence,
    ReconcileEntityKind, ReconcileReport, ReconcileResult, UncertainMatch,
    apply_match_decisions, reconcile_refined,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use crate::ir::*;
use ox_core::types::{PropertyType, PropertyValue, deserialize_patch_property_value};

// ---------------------------------------------------------------------------
// OntologyCommand — atomic, invertible operations on OntologyIR
//
// `OntologyCommand` deliberately covers only the topology + mapping
// operations the project-flow's commandStack and undo / redo path
// need to be wire-stable: nodes, edges, properties, constraints,
// indices, object mappings.
//
// Governance / vocabulary / lineage collections (rules, actions,
// functions, metrics, enrichments, glossary terms, code systems,
// value sets, notation patterns, concept maps, segments, interfaces,
// column profiles, provenances, link mappings) intentionally stay
// outside this enum. Their mutations land through dedicated admin
// PATCH endpoints whose wire-shape is owned by the routes that
// surface them — bundling that long tail under an
// `OntologyCommand::Generic { payload: serde_json::Value }` escape
// would erase types at exactly the seam where the audit log, undo
// stack, and FE renderer benefit most from typed variants. Adding a
// new collection to `OntologyIR` does not require a variant here;
// expanding the project-flow's command surface is the only trigger.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum OntologyCommand {
    AddNode {
        id: NodeTypeId,
        label: GraphLabel,
        #[serde(default)]
        description: LocalizedText,
    },
    DeleteNode {
        node_id: NodeTypeId,
    },
    RenameNode {
        node_id: NodeTypeId,
        new_label: GraphLabel,
    },
    UpdateNodeDescription {
        node_id: NodeTypeId,
        #[serde(default)]
        description: LocalizedText,
    },
    /// Replace the full glossary-anchor list on a node type. Atomic
    /// — picker UX is "set the list", not "add one + remove one", so
    /// the audit-log entry records the diff in one row instead of N.
    SetNodeGlossaryAnchors {
        node_id: NodeTypeId,
        #[serde(default)]
        anchors: Vec<crate::glossary::GlossaryTermId>,
    },

    AddEdge {
        id: EdgeTypeId,
        label: GraphLabel,
        source_node_id: NodeTypeId,
        target_node_id: NodeTypeId,
        cardinality: Cardinality,
    },
    DeleteEdge {
        edge_id: EdgeTypeId,
    },
    RenameEdge {
        edge_id: EdgeTypeId,
        new_label: GraphLabel,
    },
    UpdateEdgeCardinality {
        edge_id: EdgeTypeId,
        cardinality: Cardinality,
    },
    UpdateEdgeDescription {
        edge_id: EdgeTypeId,
        #[serde(default)]
        description: LocalizedText,
    },
    /// Symmetric to [`SetNodeGlossaryAnchors`] for edge types.
    SetEdgeGlossaryAnchors {
        edge_id: EdgeTypeId,
        #[serde(default)]
        anchors: Vec<crate::glossary::GlossaryTermId>,
    },

    AddProperty {
        owner: PropertyOwner,
        // Boxed because `PropertyDef` carries the full property schema
        // (~400 bytes after Phase A's Palantir-grade fields) and would
        // otherwise dominate the size of every `OntologyCommand` value.
        property: Box<PropertyDef>,
    },
    DeleteProperty {
        owner: PropertyOwner,
        property_id: PropertyId,
    },
    UpdateProperty {
        owner: PropertyOwner,
        property_id: PropertyId,
        patch: PropertyPatch,
    },

    AddConstraint {
        node_id: NodeTypeId,
        constraint: ConstraintDef,
    },
    RemoveConstraint {
        node_id: NodeTypeId,
        constraint_id: ConstraintId,
    },

    AddIndex {
        index: IndexDef,
    },
    RemoveIndex {
        index_id: String,
    },

    /// Create an ObjectMapping. Bound to the project save pipeline so
    /// operators editing the Domain Context page get the same
    /// commandStack + undo / redo behaviour they get for every other
    /// type-shape edit. The wire-format
    /// [`crate::OntologyEditOp::CreateObjectMapping`] continues to
    /// serve admin-table CRUD on `/edits` — the two paths are
    /// deliberately distinct: project flow snapshots the whole IR,
    /// admin flow lands one op atomically.
    CreateObjectMapping {
        // Boxed because `ObjectMappingDef` is the largest single
        // payload in the command set (relation + property mappings +
        // primary key columns + advanced fields).
        mapping: Box<crate::mapping::ObjectMappingDef>,
    },
    UpdateObjectMapping {
        id: crate::mapping::ObjectMappingId,
        mapping: Box<crate::mapping::ObjectMappingDef>,
    },
    DeleteObjectMapping {
        id: crate::mapping::ObjectMappingId,
    },

    Batch {
        description: String,
        commands: Vec<OntologyCommand>,
    },
}

/// Custom JsonSchema: Bedrock-compatible flat object schema for OntologyCommand.
///
/// Bedrock doesn't support tagged enums (oneOf with discriminator) in JSON Schema.
/// Instead, we flatten all variants into a single object with:
/// - `op` as a required string enum listing all variant names
/// - All variant fields merged as optional properties
///
/// This mirrors the pattern used by `PropertyType` in `ox_core::types`.
impl JsonSchema for OntologyCommand {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OntologyCommand".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let value = serde_json::json!({
            "type": "object",
            "description": "An atomic ontology operation. The 'op' field selects the operation type. Only the fields relevant to the chosen op should be provided; all others are optional.",
            "required": ["op"],
            "properties": {
                "op": {
                    "type": "string",
                    "enum": [
                        "add_node", "delete_node", "rename_node", "update_node_description", "set_node_glossary_anchors",
                        "add_edge", "delete_edge", "rename_edge", "update_edge_cardinality", "update_edge_description", "set_edge_glossary_anchors",
                        "add_property", "delete_property", "update_property",
                        "add_constraint", "remove_constraint",
                        "add_index", "remove_index",
                        "create_object_mapping", "update_object_mapping", "delete_object_mapping",
                        "batch"
                    ],
                    "description": "The operation type"
                },
                // AddNode fields
                "id": { "type": "string", "description": "ID for the new node (AddNode) or index (AddIndex)" },
                "label": { "type": "string", "description": "Label for the new node or edge" },
                "description": { "type": ["string", "null"], "description": "Description for node/edge" },
                "source_table": { "type": ["string", "null"], "description": "Source table name (AddNode)" },
                // DeleteNode / RenameNode / UpdateNodeDescription / AddConstraint / RemoveConstraint
                "node_id": { "type": "string", "description": "Target node type ID" },
                "new_label": { "type": "string", "description": "New label for rename operations" },
                // Edge fields
                "source_node_id": { "type": "string", "description": "Source node ID for new edge" },
                "target_node_id": { "type": "string", "description": "Target node ID for new edge" },
                "cardinality": {
                    "type": "string",
                    "enum": ["one_to_one", "one_to_many", "many_to_one", "many_to_many"],
                    "description": "Edge cardinality"
                },
                "edge_id": { "type": "string", "description": "Target edge type ID" },
                // Property fields
                "owner_id": { "type": "string", "description": "Owner node or edge ID (for property operations)" },
                "property": {
                    "type": "object",
                    "description": "PropertyDef: {id, name, property_type, nullable, default_value, description}",
                    "properties": {
                        "id": { "type": "string", "description": "UUID for the property" },
                        "name": { "type": "string", "description": "Property name" },
                        "property_type": { "type": "string", "description": "Property type: bool, int, float, string, date, datetime, duration, bytes, map" },
                        "nullable": { "type": "boolean", "description": "Whether the property can be null" },
                        "default_value": { "type": ["string", "null"], "description": "Default value as string, or null for no default" },
                        "description": { "type": ["string", "null"], "description": "Human-readable description" }
                    },
                    "required": ["id", "name", "property_type", "nullable"]
                },
                "property_id": { "type": "string", "description": "Target property ID" },
                "patch": {
                    "type": "object",
                    "description": "Partial property update (UpdateProperty)",
                    "properties": {
                        "name": { "type": ["string", "null"] },
                        "property_type": { "type": ["string", "null"], "description": "Property type: bool, int, float, string, date, datetime, duration, bytes, map" },
                        "nullable": { "type": ["boolean", "null"] },
                        "default_value": { "type": ["string", "null"], "description": "New default value as string, or null to clear" },
                        "description": { "type": ["string", "null"], "description": "New description or null to clear" }
                    }
                },
                // Constraint fields
                "constraint": {
                    "type": "object",
                    "description": "ConstraintDef with flattened NodeConstraint. 'type' selects unique|exists|node_key.",
                    "properties": {
                        "id": { "type": "string", "description": "UUID for this constraint" },
                        "type": { "type": "string", "enum": ["unique", "exists", "node_key"], "description": "Constraint kind" },
                        "property_id": { "type": "string", "description": "Single property ID (for 'exists')" },
                        "property_ids": { "type": "array", "items": { "type": "string" }, "description": "Property IDs (for 'unique' and 'node_key')" }
                    },
                    "required": ["id", "type"]
                },
                "constraint_id": { "type": "string", "description": "Constraint ID to remove" },
                // Index fields
                "index": {
                    "type": "object",
                    "description": "IndexDef. 'type' selects single|composite|full_text|vector.",
                    "properties": {
                        "id": { "type": "string", "description": "UUID for this index" },
                        "type": { "type": "string", "enum": ["single", "composite", "full_text", "vector"], "description": "Index kind" },
                        "node_id": { "type": "string", "description": "Node type ID this index belongs to" },
                        "property_id": { "type": "string", "description": "Single property ID (for 'single' and 'vector')" },
                        "property_ids": { "type": "array", "items": { "type": "string" }, "description": "Property IDs (for 'composite' and 'full_text')" },
                        "name": { "type": "string", "description": "Index name (for 'full_text')" },
                        "dimensions": { "type": "integer", "description": "Vector dimensions (for 'vector')" },
                        "similarity": { "type": "string", "enum": ["cosine", "euclidean"], "description": "Similarity metric (for 'vector')" }
                    },
                    "required": ["id", "type", "node_id"]
                },
                "index_id": { "type": "string", "description": "Index ID to remove" },
                // Glossary-anchor fields (SetNodeGlossaryAnchors / SetEdgeGlossaryAnchors)
                "anchors": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Replacement list of GlossaryTermId values for the targeted node or edge type"
                },
                // ObjectMapping fields (CreateObjectMapping / UpdateObjectMapping / DeleteObjectMapping)
                "mapping": {
                    "type": "object",
                    "description": "ObjectMappingDef payload — see ox_ontology::ObjectMappingDef for the full schema"
                },
                // Batch fields
                "commands": {
                    "type": "array",
                    "items": { "type": "object", "description": "Nested OntologyCommand" },
                    "description": "Sub-commands for batch operation"
                }
            },
            "additionalProperties": false
        });
        let serde_json::Value::Object(map) = value else {
            // `json!({...})` always produces an Object; this arm is unreachable
            // at runtime but keeps the type system honest without panicking.
            return schemars::Schema::default();
        };
        schemars::Schema::from(map)
    }
}

// ---------------------------------------------------------------------------
// PropertyPatch — partial update for a property
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyPatch {
    pub name: Option<String>,
    pub property_type: Option<PropertyType>,
    pub nullable: Option<bool>,
    /// `Some(None)` = clear default, `Some(Some(v))` = set default, `None` = no change
    #[serde(default, deserialize_with = "deserialize_patch_property_value")]
    pub default_value: Option<Option<PropertyValue>>,
    /// `Some(text)` = set description to `text` (use empty `LocalizedText` to clear);
    /// `None` = no change.
    pub description: Option<LocalizedText>,
}

// ---------------------------------------------------------------------------
// CommandResult
// ---------------------------------------------------------------------------

pub struct CommandResult {
    pub new_ontology: OntologyIR,
    pub inverse: OntologyCommand,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Extract the id from an IndexDef.
pub(crate) fn index_id(index: &IndexDef) -> &str {
    match index {
        IndexDef::Single { id, .. }
        | IndexDef::Composite { id, .. }
        | IndexDef::FullText { id, .. }
        | IndexDef::Vector { id, .. } => id,
    }
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

impl OntologyCommand {
    /// Return element IDs affected by this command (for verification invalidation).
    pub fn affected_element_ids(&self) -> Vec<String> {
        match self {
            Self::AddNode { id, .. }
            | Self::DeleteNode { node_id: id, .. }
            | Self::RenameNode { node_id: id, .. }
            | Self::UpdateNodeDescription { node_id: id, .. }
            | Self::SetNodeGlossaryAnchors { node_id: id, .. } => {
                vec![id.0.clone()]
            }
            Self::AddEdge { id, .. }
            | Self::DeleteEdge { edge_id: id, .. }
            | Self::RenameEdge { edge_id: id, .. }
            | Self::UpdateEdgeCardinality { edge_id: id, .. }
            | Self::UpdateEdgeDescription { edge_id: id, .. }
            | Self::SetEdgeGlossaryAnchors { edge_id: id, .. } => {
                vec![id.0.clone()]
            }
            Self::AddProperty { owner, .. }
            | Self::DeleteProperty { owner, .. }
            | Self::UpdateProperty { owner, .. } => {
                vec![owner.as_str().to_string()]
            }
            Self::AddConstraint { node_id, .. } | Self::RemoveConstraint { node_id, .. } => {
                vec![node_id.0.clone()]
            }
            Self::AddIndex { .. } | Self::RemoveIndex { .. } => vec![],
            Self::CreateObjectMapping { mapping } => {
                vec![mapping.id.0.clone()]
            }
            Self::UpdateObjectMapping { id, .. } | Self::DeleteObjectMapping { id } => {
                vec![id.0.clone()]
            }
            Self::Batch { commands, .. } => commands
                .iter()
                .flat_map(|c| c.affected_element_ids())
                .collect(),
        }
    }

    /// Execute a command against an ontology.
    /// Clones the ontology, applies the mutation, rebuilds indices, and validates.
    /// For Batch commands, sub-commands skip per-step validation — only the final
    /// result is validated once.
    pub fn execute(&self, ontology: &OntologyIR) -> Result<CommandResult, String> {
        let result = self.execute_inner(ontology.clone())?;
        let mut new_ontology = result.new_ontology;
        new_ontology
            .rebuild_indices()
            .map_err(|e| format!("command produced invalid ontology: {e}"))?;
        let errors = new_ontology.validate();
        if errors.is_empty() {
            Ok(CommandResult {
                new_ontology,
                inverse: result.inverse,
            })
        } else {
            Err(format!(
                "command produced invalid ontology: {}",
                ox_core::join_messages(&errors, "; ")
            ))
        }
    }
}
