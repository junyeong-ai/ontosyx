//! LLM-facing ontology design output type.
//!
//! The canonical [`InputOntologyDef`] is the wire shape Ontosyx
//! accepts on every external entry point: file uploads, legacy
//! imports, command replays. It carries every nicety those entry
//! points need — explicit ids, full `LocalizedText`, indexes,
//! tagged-enum node constraints, format versions — and the resulting
//! JSON Schema (~73 properties, ~36 optional) is well past the
//! cohort size where Anthropic / OpenAI structured-output mode
//! produces consistently valid responses. Without intervention the
//! provider layer trips its complexity gate on every single design
//! batch and falls back to free-form JSON, defeating the safety the
//! schema is supposed to provide.
//!
//! [`LlmDesignOutput`] is the LLM-only contract: a strict subset
//! that drops everything the model is not the source of truth for
//! (server-generated ids, indexes, constraints — those layer in
//! during `normalize` or in a follow-up enrichment pass) and
//! flattens [`LocalizedText`] to a single string the server wraps.
//! The resulting schema fits comfortably under
//! [`LLM_OUTPUT_PROPERTY_BUDGET`], so structured output is the
//! steady-state path; JSON-mode fallback is a defensive last
//! resort that never fires under normal operation.
//!
//! [`into_input_ontology`] converts back to [`InputOntologyDef`]
//! without any LLM round-trip — the existing `normalize` pipeline
//! takes it from there.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;
use ox_core::types::PropertyType;
use ox_ontology::input::{InputEdgeTypeDef, InputNodeTypeDef, InputOntologyDef, InputPropertyDef};
use ox_ontology::ir::Cardinality;

/// Hard ceiling on the JSON Schema produced from any LLM output
/// type. Both the Anthropic and OpenAI structured-output paths
/// remain reliable well below 50 total properties; budgeting at
/// 30 leaves explicit headroom for fields added later without
/// dropping back to free-form JSON. Enforced by
/// [`crate::design::llm_output::tests::schema_fits_budget`].
pub const LLM_OUTPUT_PROPERTY_BUDGET: usize = 30;

/// Ontology shape produced by the design LLM. Strictly narrower
/// than [`InputOntologyDef`] — the server fills in everything the
/// model is not the source of truth for.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LlmDesignOutput {
    /// Short, human-readable ontology name (e.g. `"E-commerce"`).
    pub name: String,
    /// One-paragraph English description. Server wraps this in a
    /// [`LocalizedText`]; translations come later through the
    /// content-management surfaces.
    pub description: String,
    pub node_types: Vec<LlmNodeType>,
    pub edge_types: Vec<LlmEdgeType>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LlmNodeType {
    /// PascalCase class label (`Customer`, `LineItem`).
    pub label: String,
    /// One-line description used to ground LLM downstream calls.
    pub description: String,
    /// Source table name this node was derived from.
    pub source_table: Option<String>,
    pub properties: Vec<LlmProperty>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LlmEdgeType {
    /// Verb-phrase label (`PLACES`, `CONTAINS`).
    pub label: String,
    pub description: String,
    /// Source node label or id (resolved during normalize).
    pub source_type: String,
    /// Target node label or id (resolved during normalize).
    pub target_type: String,
    pub cardinality: Cardinality,
    pub properties: Vec<LlmProperty>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct LlmProperty {
    /// snake_case attribute name (`email`, `created_at`).
    pub name: String,
    pub property_type: PropertyType,
    pub nullable: bool,
    pub description: String,
    /// Source column name this property was derived from.
    pub source_column: Option<String>,
}

/// Convert the LLM-facing shape into the canonical input shape that
/// `ox_ontology::input::normalize` consumes. Server fills every
/// field the LLM was not asked to produce.
pub fn into_input_ontology(out: LlmDesignOutput) -> InputOntologyDef {
    InputOntologyDef {
        format_version: 1,
        id: None,
        name: out.name,
        description: LocalizedText::new(&out.description),
        version: 1,
        node_types: out.node_types.into_iter().map(node_into_input).collect(),
        edge_types: out.edge_types.into_iter().map(edge_into_input).collect(),
        indexes: Vec::new(),
    }
}

fn node_into_input(node: LlmNodeType) -> InputNodeTypeDef {
    InputNodeTypeDef {
        id: None,
        label: node.label,
        description: LocalizedText::new(&node.description),
        source_table: node.source_table,
        properties: node
            .properties
            .into_iter()
            .map(property_into_input)
            .collect(),
        constraints: Vec::new(),
    }
}

fn edge_into_input(edge: LlmEdgeType) -> InputEdgeTypeDef {
    InputEdgeTypeDef {
        id: None,
        label: edge.label,
        description: LocalizedText::new(&edge.description),
        source_type: edge.source_type,
        target_type: edge.target_type,
        properties: edge
            .properties
            .into_iter()
            .map(property_into_input)
            .collect(),
        cardinality: edge.cardinality,
        // The narrow LLM schema doesn't carry edge classification; the
        // operator promotes plain Associations to Composition /
        // Aggregation in the design-review UI when warranted.
        kind: ox_ontology::ir::EdgeKind::Association,
    }
}

fn property_into_input(prop: LlmProperty) -> InputPropertyDef {
    InputPropertyDef {
        id: None,
        name: prop.name,
        property_type: prop.property_type,
        nullable: prop.nullable,
        default_value: None,
        description: LocalizedText::new(&prop.description),
        source_column: prop.source_column,
    }
}

/// Recursively merge a fresh batch's LLM output into a baseline
/// ontology. Tables already present in the baseline carry through
/// untouched; new tables append. Mirrors the existing
/// `merge_input_irs` semantics that the divide-and-conquer batch
/// path relies on, but operates on the narrower [`LlmDesignOutput`]
/// type so the LLM never sees fields it cannot populate.
pub fn merge_llm_outputs(base: LlmDesignOutput, addition: LlmDesignOutput) -> LlmDesignOutput {
    let mut nodes_by_label: HashMap<String, LlmNodeType> = base
        .node_types
        .into_iter()
        .map(|n| (n.label.clone(), n))
        .collect();
    for node in addition.node_types {
        nodes_by_label.entry(node.label.clone()).or_insert(node);
    }

    let mut edges_by_label: HashMap<String, LlmEdgeType> = base
        .edge_types
        .into_iter()
        .map(|e| (edge_key(&e), e))
        .collect();
    for edge in addition.edge_types {
        edges_by_label.entry(edge_key(&edge)).or_insert(edge);
    }

    LlmDesignOutput {
        name: base.name,
        description: base.description,
        node_types: nodes_by_label.into_values().collect(),
        edge_types: edges_by_label.into_values().collect(),
    }
}

fn edge_key(edge: &LlmEdgeType) -> String {
    format!("{}:{}->{}", edge.label, edge.source_type, edge.target_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::count_total_properties;

    #[test]
    fn schema_fits_budget() {
        let schema = schemars::schema_for!(LlmDesignOutput);
        let value = schema.to_value();
        let total = count_total_properties(&value);
        assert!(
            total < LLM_OUTPUT_PROPERTY_BUDGET,
            "LlmDesignOutput JSON Schema has {total} properties; \
             budget is {LLM_OUTPUT_PROPERTY_BUDGET}. Trim fields or \
             defer them to a follow-up enrichment pass before \
             shipping a wider schema."
        );
    }

    #[test]
    fn into_input_wraps_localized_text() {
        let llm = LlmDesignOutput {
            name: "Test".into(),
            description: "Body".into(),
            node_types: vec![],
            edge_types: vec![],
        };
        let input = into_input_ontology(llm);
        assert_eq!(input.name, "Test");
        assert_eq!(input.description.default, "Body");
        assert_eq!(input.format_version, 1);
        assert_eq!(input.version, 1);
    }
}
