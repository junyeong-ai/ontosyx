mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use types::*;

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// OntologyIR — DB-agnostic ontology definition
//
// Describes the graph schema (node types, edge types, constraints, indexes)
// without any reference to Cypher, Gremlin, or GQL syntax.
//
// All entities carry stable UUIDs (`id` fields). Cross-references between
// entities use these IDs rather than labels/names so that renames do not
// break referential integrity.
//
// Compiles to:
//   Neo4j   → CREATE CONSTRAINT / CREATE INDEX statements
//   Neptune → Property graph schema (or schema-less with validation)
//   GQL     → CREATE NODE TYPE / CREATE EDGE TYPE (ISO 39075)
// ---------------------------------------------------------------------------

/// Precomputed lookup indices for O(1) resolver access.
/// Rebuilt automatically on deserialization and after mutations.
#[derive(Debug, Clone, Default)]
struct OntologyLookup {
    /// node id → index in node_types
    node_id_idx: HashMap<NodeTypeId, usize>,
    /// node label → index in node_types
    node_label_idx: HashMap<String, usize>,
    /// edge id → index in edge_types
    edge_id_idx: HashMap<EdgeTypeId, usize>,
    /// property id → (node_types index, property index within that node)
    prop_id_loc: HashMap<PropertyId, (usize, usize)>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OntologyIR {
    /// Unique identifier for this ontology version
    pub id: String,
    /// Human-readable name (e.g. "E-commerce Ontology")
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Monotonically increasing version number
    pub version: u32,
    /// All node types in this ontology
    pub node_types: Vec<NodeTypeDef>,
    /// All edge types (relationships) in this ontology
    #[serde(default)]
    pub edge_types: Vec<EdgeTypeDef>,
    /// Global indexes that span multiple types
    #[serde(default)]
    pub indexes: Vec<IndexDef>,

    /// Precomputed lookup indices — not serialized, rebuilt on deserialize.
    #[serde(skip)]
    #[schemars(skip)]
    lookup: OntologyLookup,
}

/// Custom Deserialize that auto-builds lookup indices after loading.
impl<'de> Deserialize<'de> for OntologyIR {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: String,
            name: String,
            description: Option<String>,
            version: u32,
            node_types: Vec<NodeTypeDef>,
            #[serde(default)]
            edge_types: Vec<EdgeTypeDef>,
            #[serde(default)]
            indexes: Vec<IndexDef>,
        }
        let w = Wire::deserialize(deserializer)?;
        let mut ont = OntologyIR {
            id: w.id,
            name: w.name,
            description: w.description,
            version: w.version,
            node_types: w.node_types,
            edge_types: w.edge_types,
            indexes: w.indexes,
            lookup: OntologyLookup::default(),
        };
        ont.rebuild_indices();
        Ok(ont)
    }
}

// ---------------------------------------------------------------------------
// Construction + Index management + Resolver methods (O(1) via HashMap)
// ---------------------------------------------------------------------------

impl OntologyIR {
    /// Construct a new OntologyIR with prebuilt lookup indices.
    pub fn new(
        id: String,
        name: String,
        description: Option<String>,
        version: u32,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Self {
        let mut ont = Self {
            id,
            name,
            description,
            version,
            node_types,
            edge_types,
            indexes,
            lookup: OntologyLookup::default(),
        };
        ont.rebuild_indices();
        ont
    }

    /// Construct a new OntologyIR, validate it, and return the validated instance.
    /// Returns an error if validation fails, ensuring all OntologyIR instances
    /// created through this constructor are valid by construction.
    pub fn new_validated(
        id: String,
        name: String,
        description: Option<String>,
        version: u32,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Result<Self, Vec<String>> {
        let ont = Self::new(
            id,
            name,
            description,
            version,
            node_types,
            edge_types,
            indexes,
        );
        let errors = ont.validate();
        if errors.is_empty() {
            Ok(ont)
        } else {
            Err(errors)
        }
    }

    /// Rebuild all lookup indices from current data.
    /// Must be called after any structural mutation (add/remove/reorder nodes/edges/properties).
    ///
    /// # Panics (debug only)
    /// Debug-asserts if duplicate IDs or labels are found, indicating a corrupt ontology.
    pub fn rebuild_indices(&mut self) {
        let mut lookup = OntologyLookup::default();
        for (i, node) in self.node_types.iter().enumerate() {
            let prev_id = lookup.node_id_idx.insert(node.id.clone(), i);
            debug_assert!(prev_id.is_none(), "duplicate node id: {}", node.id);
            let prev_label = lookup.node_label_idx.insert(node.label.clone(), i);
            debug_assert!(prev_label.is_none(), "duplicate node label: {}", node.label);
            for (j, prop) in node.properties.iter().enumerate() {
                let prev_prop = lookup.prop_id_loc.insert(prop.id.clone(), (i, j));
                debug_assert!(prev_prop.is_none(), "duplicate property id: {}", prop.id);
            }
        }
        for (i, edge) in self.edge_types.iter().enumerate() {
            let prev = lookup.edge_id_idx.insert(edge.id.clone(), i);
            debug_assert!(prev.is_none(), "duplicate edge id: {}", edge.id);
        }
        self.lookup = lookup;
    }

    /// Consume self, rebuild indices, return self. Useful for chaining after construction.
    pub fn with_indices(mut self) -> Self {
        self.rebuild_indices();
        self
    }

    /// Resolve a node's label from its ID. O(1).
    pub fn node_label(&self, node_id: &str) -> Option<&str> {
        self.lookup
            .node_id_idx
            .get(node_id)
            .map(|&i| self.node_types[i].label.as_str())
    }

    /// Look up a node type by its stable ID. O(1).
    pub fn node_by_id(&self, node_id: &str) -> Option<&NodeTypeDef> {
        self.lookup
            .node_id_idx
            .get(node_id)
            .map(|&i| &self.node_types[i])
    }

    /// Look up a node type by its label. O(1).
    pub fn node_by_label(&self, label: &str) -> Option<&NodeTypeDef> {
        self.lookup
            .node_label_idx
            .get(label)
            .map(|&i| &self.node_types[i])
    }

    /// Look up a property by its stable ID across all node types. O(1).
    /// Returns the owning node and the property.
    pub fn property_by_id(&self, prop_id: &str) -> Option<(&NodeTypeDef, &PropertyDef)> {
        self.lookup
            .prop_id_loc
            .get(prop_id)
            .map(|&(ni, pi)| (&self.node_types[ni], &self.node_types[ni].properties[pi]))
    }

    /// Look up an edge type by its stable ID. O(1).
    pub fn edge_by_id(&self, edge_id: &str) -> Option<&EdgeTypeDef> {
        self.lookup
            .edge_id_idx
            .get(edge_id)
            .map(|&i| &self.edge_types[i])
    }

    /// Find a property by ID within a specific property list.
    pub fn property_in<'a>(
        &self,
        properties: &'a [PropertyDef],
        prop_id: &str,
    ) -> Option<&'a PropertyDef> {
        properties.iter().find(|p| p.id == prop_id)
    }

    // -----------------------------------------------------------------------
    // Schema RAG — natural language descriptions for embedding + compact schema
    // -----------------------------------------------------------------------

    /// Convert each node+edge into a natural language description for semantic embedding.
    /// Each entry is `(stable_id, natural_language_text)`.
    pub fn to_schema_entries(&self) -> Vec<(String, String)> {
        let mut entries = Vec::new();

        for node in &self.node_types {
            // Collect connected edges
            let outgoing: Vec<&str> = self
                .edge_types
                .iter()
                .filter(|e| e.source_node_id == node.id)
                .map(|e| self.node_label(e.target_node_id.as_ref()).unwrap_or("?"))
                .collect();
            let incoming: Vec<(&str, &str)> = self
                .edge_types
                .iter()
                .filter(|e| e.target_node_id == node.id)
                .map(|e| {
                    let src = self.node_label(e.source_node_id.as_ref()).unwrap_or("?");
                    (src, e.label.as_str())
                })
                .collect();

            let props: Vec<&str> = node.properties.iter().map(|p| p.name.as_str()).collect();

            let desc = node.description.as_deref().unwrap_or("");
            let mut text = format!("{}: {} Properties: {}.", node.label, desc, props.join(", "));

            if !outgoing.is_empty() {
                text.push_str(&format!(" Connected to: {}.", outgoing.join(", ")));
            }
            if !incoming.is_empty() {
                let rels: Vec<String> = incoming
                    .iter()
                    .map(|(src, edge)| format!("{src} via {edge}"))
                    .collect();
                text.push_str(&format!(" Referenced by: {}.", rels.join(", ")));
            }

            entries.push((node.id.as_ref().to_string(), text));
        }

        entries
    }

    /// Build a compact JSON schema for a subset of nodes (identified by labels).
    /// Includes full property descriptions and edge connections — minimal but complete
    /// for LLM query translation.
    pub fn compact_schema(&self, node_labels: &[&str]) -> serde_json::Value {
        use std::collections::HashSet;
        let selected: HashSet<&str> = node_labels.iter().copied().collect();

        let mut nodes = serde_json::Map::new();
        let mut edges = serde_json::Map::new();

        for node in &self.node_types {
            if !selected.contains(node.label.as_str()) {
                continue;
            }
            let mut props = serde_json::Map::new();
            for p in &node.properties {
                let desc = p.description.as_deref().unwrap_or("");
                let nullable = if p.nullable { ", nullable" } else { "" };
                props.insert(
                    p.name.clone(),
                    serde_json::Value::String(
                        format!("{}{} {}", p.property_type, nullable, desc)
                            .trim()
                            .to_string(),
                    ),
                );
            }
            let mut node_obj = serde_json::Map::new();
            if let Some(d) = &node.description {
                node_obj.insert("description".into(), serde_json::Value::String(d.clone()));
            }
            node_obj.insert("properties".into(), serde_json::Value::Object(props));
            nodes.insert(node.label.clone(), serde_json::Value::Object(node_obj));
        }

        // Include edges where both source and target are in the selected set
        for edge in &self.edge_types {
            let src_label = self.node_label(edge.source_node_id.as_ref()).unwrap_or("?");
            let tgt_label = self.node_label(edge.target_node_id.as_ref()).unwrap_or("?");
            if selected.contains(src_label) || selected.contains(tgt_label) {
                let mut edge_obj = serde_json::Map::new();
                edge_obj.insert(
                    "source".into(),
                    serde_json::Value::String(src_label.to_string()),
                );
                edge_obj.insert(
                    "target".into(),
                    serde_json::Value::String(tgt_label.to_string()),
                );
                edge_obj.insert(
                    "cardinality".into(),
                    serde_json::Value::String(format!("{:?}", edge.cardinality)),
                );
                if let Some(d) = &edge.description {
                    edge_obj.insert("description".into(), serde_json::Value::String(d.clone()));
                }
                if !edge.properties.is_empty() {
                    let props: Vec<String> =
                        edge.properties.iter().map(|p| p.name.clone()).collect();
                    edge_obj.insert("properties".into(), serde_json::json!(props));
                }
                edges.insert(edge.label.clone(), serde_json::Value::Object(edge_obj));
            }
        }

        serde_json::json!({
            "nodes": nodes,
            "edges": edges,
        })
    }

    /// Get 1-hop neighbor labels for a given node label.
    pub fn neighbor_labels(&self, label: &str) -> Vec<&str> {
        let node = match self.node_by_label(label) {
            Some(n) => n,
            None => return vec![],
        };
        let mut neighbors = Vec::new();
        for edge in &self.edge_types {
            if edge.source_node_id == node.id
                && let Some(tgt) = self.node_label(edge.target_node_id.as_ref())
            {
                neighbors.push(tgt);
            }
            if edge.target_node_id == node.id
                && let Some(src) = self.node_label(edge.source_node_id.as_ref())
            {
                neighbors.push(src);
            }
        }
        neighbors.sort_unstable();
        neighbors.dedup();
        neighbors
    }
}
