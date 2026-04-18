mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use types::*;

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ---------------------------------------------------------------------------
// OntologyInvariantError — typed invariant violations
// ---------------------------------------------------------------------------

/// Invariants that the ontology IR maintains at construction and after any
/// structural mutation. A violation means the caller produced inconsistent
/// data (duplicate ids, references to non-existent entities, etc.).
///
/// Raised by [`OntologyIR::try_new`], [`OntologyIR::rebuild_indices`], and
/// every mutation method that can introduce duplicates.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum OntologyInvariantError {
    #[error("duplicate node type id: {id}")]
    DuplicateNodeTypeId { id: NodeTypeId },

    #[error("duplicate node type label: {label}")]
    DuplicateNodeTypeLabel { label: String },

    #[error("duplicate edge type id: {id}")]
    DuplicateEdgeTypeId { id: EdgeTypeId },

    #[error("duplicate property id: {id} (property ids must be unique across the ontology)")]
    DuplicatePropertyId { id: PropertyId },

    #[error("node type not found: {id}")]
    NodeTypeNotFound { id: NodeTypeId },

    #[error("edge type not found: {id}")]
    EdgeTypeNotFound { id: EdgeTypeId },

    #[error("index not found: {id}")]
    IndexNotFound { id: String },
}

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

/// Tiered schema view. Each level trades fidelity for tokens; pick the
/// smallest one that gives the consuming LLM enough signal.
///
/// See [`OntologyIR::schema_view`] for per-tier semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaView {
    /// Node + edge label names only. Cheapest.
    Labels,
    /// Labels + per-node property names + 1-hop edge connectivity.
    Structural,
    /// Full property types, descriptions, edge cardinality (current
    /// `compact_schema` output). Most expensive — use only for the
    /// subset already known to be needed.
    Detailed,
}

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
    /// Unique identifier for this ontology version.
    pub id: String,
    /// Human-readable name (e.g. "E-commerce Ontology"). Single canonical
    /// string; for localized display use the workspace's ontology catalog
    /// layer rather than embedding locale variants into the identifier.
    pub name: String,
    /// Localized human-readable description of the ontology.
    #[serde(default)]
    pub description: crate::i18n::LocalizedText,
    /// Version metadata (number + temporal window + provenance).
    pub version: OntologyVersion,
    /// All node types in this ontology. Accessed externally via
    /// [`OntologyIR::node_types`]; structural mutations go through
    /// [`OntologyIR::add_node_type`], [`OntologyIR::remove_node_type`], etc.
    pub(crate) node_types: Vec<NodeTypeDef>,
    /// All edge types (relationships) in this ontology. See
    /// [`OntologyIR::edge_types`] / [`OntologyIR::add_edge_type`].
    #[serde(default)]
    pub(crate) edge_types: Vec<EdgeTypeDef>,
    /// Global indexes that span multiple types. See
    /// [`OntologyIR::indexes`] / [`OntologyIR::add_index`].
    #[serde(default)]
    pub(crate) indexes: Vec<IndexDef>,

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
            #[serde(default)]
            description: crate::i18n::LocalizedText,
            version: OntologyVersion,
            node_types: Vec<NodeTypeDef>,
            #[serde(default)]
            edge_types: Vec<EdgeTypeDef>,
            #[serde(default)]
            indexes: Vec<IndexDef>,
        }

        let w = Wire::deserialize(deserializer)?;
        OntologyIR::try_new(
            w.id,
            w.name,
            w.description,
            w.version,
            w.node_types,
            w.edge_types,
            w.indexes,
        )
        .map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Construction + Index management + Resolver methods (O(1) via HashMap)
// ---------------------------------------------------------------------------

impl OntologyIR {
    /// Construct a new OntologyIR with prebuilt lookup indices.
    ///
    /// This is the ergonomic constructor used when the caller has just built
    /// the vectors with known-unique ids/labels (e.g. test fixtures, transform
    /// helpers). It delegates to [`OntologyIR::try_new`] and panics on invariant
    /// violations — such a panic indicates a programming bug in the caller.
    ///
    /// For deserialization or any input whose uniqueness cannot be guaranteed
    /// up-front, call [`OntologyIR::try_new`] directly and handle the
    /// [`OntologyInvariantError`] explicitly.
    #[allow(clippy::expect_used)]
    pub fn new(
        id: String,
        name: String,
        description: crate::i18n::LocalizedText,
        version: impl Into<OntologyVersion>,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Self {
        Self::try_new(id, name, description, version, node_types, edge_types, indexes)
            .expect(
                "OntologyIR::new called with duplicate ids/labels; \
                 caller must ensure uniqueness or use OntologyIR::try_new instead",
            )
    }

    /// Fallible constructor. Returns [`OntologyInvariantError`] if the input
    /// vectors contain duplicate node/edge/property ids or duplicate node
    /// labels.
    ///
    /// Use this whenever input cannot be statically guaranteed unique —
    /// deserialization, merging ontologies, LLM-generated data, etc.
    pub fn try_new(
        id: String,
        name: String,
        description: crate::i18n::LocalizedText,
        version: impl Into<OntologyVersion>,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Result<Self, OntologyInvariantError> {
        let mut ont = Self {
            id,
            name,
            description,
            version: version.into(),
            node_types,
            edge_types,
            indexes,
            lookup: OntologyLookup::default(),
        };
        ont.rebuild_indices()?;
        Ok(ont)
    }

    /// Construct a new OntologyIR, validate it, and return the validated instance.
    ///
    /// Structural invariant violations (duplicate ids/labels) and semantic
    /// validation errors are both returned as strings in the `Err` vec.
    pub fn new_validated(
        id: String,
        name: String,
        description: crate::i18n::LocalizedText,
        version: impl Into<OntologyVersion>,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Result<Self, Vec<String>> {
        let ont = Self::try_new(
            id,
            name,
            description,
            version,
            node_types,
            edge_types,
            indexes,
        )
        .map_err(|e| vec![e.to_string()])?;
        let errors = ont.validate();
        if errors.is_empty() {
            Ok(ont)
        } else {
            Err(errors)
        }
    }

    /// Rebuild all lookup indices from current data.
    ///
    /// Must be called after any structural mutation (add/remove/reorder
    /// nodes/edges/properties). Returns an error if duplicate ids or labels
    /// are detected — a programming mistake in the caller, not a user input
    /// error.
    ///
    /// Structural mutation methods on this type (`add_node_type`,
    /// `remove_edge_type`, `with_batch`, etc.) call this automatically, so
    /// most callers never need to invoke it directly.
    pub fn rebuild_indices(&mut self) -> Result<(), OntologyInvariantError> {
        let mut lookup = OntologyLookup::default();
        for (i, node) in self.node_types.iter().enumerate() {
            if lookup.node_id_idx.insert(node.id.clone(), i).is_some() {
                return Err(OntologyInvariantError::DuplicateNodeTypeId {
                    id: node.id.clone(),
                });
            }
            if lookup.node_label_idx.insert(node.label.clone(), i).is_some() {
                return Err(OntologyInvariantError::DuplicateNodeTypeLabel {
                    label: node.label.clone(),
                });
            }
            for (j, prop) in node.properties.iter().enumerate() {
                if lookup.prop_id_loc.insert(prop.id.clone(), (i, j)).is_some() {
                    return Err(OntologyInvariantError::DuplicatePropertyId {
                        id: prop.id.clone(),
                    });
                }
            }
        }
        for (i, edge) in self.edge_types.iter().enumerate() {
            if lookup.edge_id_idx.insert(edge.id.clone(), i).is_some() {
                return Err(OntologyInvariantError::DuplicateEdgeTypeId {
                    id: edge.id.clone(),
                });
            }
        }
        self.lookup = lookup;
        Ok(())
    }

    /// Consume self, rebuild indices, return self. Useful for chaining after
    /// ad-hoc mutation patterns; fails with [`OntologyInvariantError`] on
    /// duplicate detection.
    pub fn with_indices(mut self) -> Result<Self, OntologyInvariantError> {
        self.rebuild_indices()?;
        Ok(self)
    }

    // -----------------------------------------------------------------------
    // Structural mutation API — every method rebuilds the lookup internally
    // and surfaces invariant violations via [`OntologyInvariantError`].
    // -----------------------------------------------------------------------

    /// Add a node type. Fails if a node with the same id or label already
    /// exists. On success, returns an immutable reference to the inserted
    /// entry.
    pub fn add_node_type(
        &mut self,
        node: NodeTypeDef,
    ) -> Result<&NodeTypeDef, OntologyInvariantError> {
        if self.lookup.node_id_idx.contains_key(&node.id) {
            return Err(OntologyInvariantError::DuplicateNodeTypeId { id: node.id });
        }
        if self.lookup.node_label_idx.contains_key(&node.label) {
            return Err(OntologyInvariantError::DuplicateNodeTypeLabel {
                label: node.label,
            });
        }
        self.node_types.push(node);
        self.rebuild_indices()?;
        self.node_types
            .last()
            .ok_or_else(|| OntologyInvariantError::NodeTypeNotFound {
                id: NodeTypeId::new(""),
            })
    }

    /// Remove a node type by id and return the removed entry. Fails with
    /// [`OntologyInvariantError::NodeTypeNotFound`] if no such id exists.
    ///
    /// Edge types and indexes that reference the removed node are NOT
    /// cascaded — the caller is responsible for cleaning up references,
    /// typically via `ontology_command::OntologyCommand::DeleteNode`.
    pub fn remove_node_type(
        &mut self,
        id: &NodeTypeId,
    ) -> Result<NodeTypeDef, OntologyInvariantError> {
        let idx = *self.lookup.node_id_idx.get(id).ok_or_else(|| {
            OntologyInvariantError::NodeTypeNotFound { id: id.clone() }
        })?;
        let removed = self.node_types.remove(idx);
        self.rebuild_indices()?;
        Ok(removed)
    }

    /// Apply a closure to a node type in place.
    ///
    /// The closure receives a mutable reference to the entry; on return, the
    /// lookup is rebuilt and invariant violations surface as
    /// [`OntologyInvariantError`] (e.g. if the closure renamed the node to a
    /// duplicate label).
    pub fn update_node_type<F>(
        &mut self,
        id: &NodeTypeId,
        f: F,
    ) -> Result<(), OntologyInvariantError>
    where
        F: FnOnce(&mut NodeTypeDef),
    {
        let idx = *self.lookup.node_id_idx.get(id).ok_or_else(|| {
            OntologyInvariantError::NodeTypeNotFound { id: id.clone() }
        })?;
        f(&mut self.node_types[idx]);
        self.rebuild_indices()
    }

    /// Add an edge type. Fails on duplicate id.
    pub fn add_edge_type(
        &mut self,
        edge: EdgeTypeDef,
    ) -> Result<&EdgeTypeDef, OntologyInvariantError> {
        if self.lookup.edge_id_idx.contains_key(&edge.id) {
            return Err(OntologyInvariantError::DuplicateEdgeTypeId { id: edge.id });
        }
        self.edge_types.push(edge);
        self.rebuild_indices()?;
        self.edge_types
            .last()
            .ok_or_else(|| OntologyInvariantError::EdgeTypeNotFound {
                id: EdgeTypeId::new(""),
            })
    }

    /// Remove an edge type by id and return the removed entry.
    pub fn remove_edge_type(
        &mut self,
        id: &EdgeTypeId,
    ) -> Result<EdgeTypeDef, OntologyInvariantError> {
        let idx = *self.lookup.edge_id_idx.get(id).ok_or_else(|| {
            OntologyInvariantError::EdgeTypeNotFound { id: id.clone() }
        })?;
        let removed = self.edge_types.remove(idx);
        self.rebuild_indices()?;
        Ok(removed)
    }

    /// Apply a closure to an edge type in place.
    pub fn update_edge_type<F>(
        &mut self,
        id: &EdgeTypeId,
        f: F,
    ) -> Result<(), OntologyInvariantError>
    where
        F: FnOnce(&mut EdgeTypeDef),
    {
        let idx = *self.lookup.edge_id_idx.get(id).ok_or_else(|| {
            OntologyInvariantError::EdgeTypeNotFound { id: id.clone() }
        })?;
        f(&mut self.edge_types[idx]);
        self.rebuild_indices()
    }

    /// Add an index definition. No uniqueness check on ids (indexes carry
    /// caller-supplied ids that are not structural identifiers).
    pub fn add_index(&mut self, index: IndexDef) -> Result<(), OntologyInvariantError> {
        self.indexes.push(index);
        self.rebuild_indices()
    }

    /// Remove an index whose `id` matches `index_id`.
    pub fn remove_index(&mut self, index_id: &str) -> Result<IndexDef, OntologyInvariantError> {
        let pos = self.indexes.iter().position(|idx| match idx {
            IndexDef::Single { id, .. }
            | IndexDef::Composite { id, .. }
            | IndexDef::FullText { id, .. }
            | IndexDef::Vector { id, .. } => id == index_id,
        });
        let pos = pos.ok_or_else(|| OntologyInvariantError::IndexNotFound {
            id: index_id.to_string(),
        })?;
        let removed = self.indexes.remove(pos);
        self.rebuild_indices()?;
        Ok(removed)
    }

    /// Execute a batch of structural mutations with a single rebuild at the
    /// end.
    ///
    /// Use when a single logical change touches several entries (e.g.
    /// renaming a node and rewriting all edges that reference it). The
    /// closure receives direct mutable access to the internal vectors; on
    /// return, the lookup is rebuilt once and invariant violations surface
    /// as [`OntologyInvariantError`].
    pub fn with_batch<F, R>(&mut self, f: F) -> Result<R, OntologyInvariantError>
    where
        F: FnOnce(&mut Vec<NodeTypeDef>, &mut Vec<EdgeTypeDef>, &mut Vec<IndexDef>) -> R,
    {
        let r = f(&mut self.node_types, &mut self.edge_types, &mut self.indexes);
        self.rebuild_indices()?;
        Ok(r)
    }

    // -----------------------------------------------------------------------
    // Read accessors — prefer these over direct field access.
    // -----------------------------------------------------------------------

    /// All node types in declaration order.
    pub fn node_types(&self) -> &[NodeTypeDef] {
        &self.node_types
    }

    /// All edge types in declaration order.
    pub fn edge_types(&self) -> &[EdgeTypeDef] {
        &self.edge_types
    }

    /// All indexes in declaration order.
    pub fn indexes(&self) -> &[IndexDef] {
        &self.indexes
    }

    /// Mutable slice of node types. Intended for field-level mutation that
    /// does **not** change `id` or `label` (e.g. description/property edits).
    /// If you do change `id`/`label`, call [`OntologyIR::rebuild_indices`]
    /// afterwards to refresh the lookup tables; for structural add/remove,
    /// use [`OntologyIR::add_node_type`] / [`OntologyIR::remove_node_type`].
    pub fn node_types_mut(&mut self) -> &mut [NodeTypeDef] {
        &mut self.node_types
    }

    /// Mutable slice of edge types. Same contract as
    /// [`OntologyIR::node_types_mut`].
    pub fn edge_types_mut(&mut self) -> &mut [EdgeTypeDef] {
        &mut self.edge_types
    }

    /// Mutable slice of index definitions. Same contract as
    /// [`OntologyIR::node_types_mut`].
    pub fn indexes_mut(&mut self) -> &mut [IndexDef] {
        &mut self.indexes
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

            let desc = node.description.as_str();
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

    /// Render a tiered schema view. Pick the smallest tier that gives
    /// the LLM enough signal — every byte costs tokens.
    ///
    /// - `Labels` (~10 tokens/node): list of node and edge label names
    ///   only. Use during label discovery / RAG selection.
    /// - `Structural` (~40 tokens/node): per-node property names and
    ///   1-hop edge connectivity. Use when the model needs to plan a
    ///   query shape but does not yet need property types or
    ///   descriptions.
    /// - `Detailed` (~120 tokens/node): full property types, nullable,
    ///   descriptions, edge cardinality, edge properties. Use only for
    ///   the subset of labels actually needed in the final query.
    pub fn schema_view(
        &self,
        view: SchemaView,
        node_labels: &[&str],
    ) -> serde_json::Value {
        match view {
            SchemaView::Labels => self.labels_view(node_labels),
            SchemaView::Structural => self.structural_view(node_labels),
            SchemaView::Detailed => self.compact_schema(node_labels),
        }
    }

    /// Tier 1 — node and edge label lists only.
    fn labels_view(&self, node_labels: &[&str]) -> serde_json::Value {
        use std::collections::HashSet;
        let selected: HashSet<&str> = node_labels.iter().copied().collect();

        let nodes: Vec<&str> = self
            .node_types
            .iter()
            .filter(|n| selected.contains(n.label.as_str()))
            .map(|n| n.label.as_str())
            .collect();

        let edges: Vec<&str> = self
            .edge_types
            .iter()
            .filter(|e| {
                let src = self.node_label(e.source_node_id.as_ref()).unwrap_or("?");
                let tgt = self.node_label(e.target_node_id.as_ref()).unwrap_or("?");
                selected.contains(src) || selected.contains(tgt)
            })
            .map(|e| e.label.as_str())
            .collect();

        serde_json::json!({ "nodes": nodes, "edges": edges })
    }

    /// Tier 2 — per-node property names + 1-hop edge connectivity.
    /// Drops type metadata, descriptions, cardinality.
    fn structural_view(&self, node_labels: &[&str]) -> serde_json::Value {
        use std::collections::HashSet;
        let selected: HashSet<&str> = node_labels.iter().copied().collect();

        let mut nodes = serde_json::Map::new();
        for node in &self.node_types {
            if !selected.contains(node.label.as_str()) {
                continue;
            }
            let props: Vec<&str> =
                node.properties.iter().map(|p| p.name.as_str()).collect();
            nodes.insert(
                node.label.clone(),
                serde_json::json!({ "properties": props }),
            );
        }

        let mut edges = serde_json::Map::new();
        for edge in &self.edge_types {
            let src = self.node_label(edge.source_node_id.as_ref()).unwrap_or("?");
            let tgt = self.node_label(edge.target_node_id.as_ref()).unwrap_or("?");
            if !selected.contains(src) && !selected.contains(tgt) {
                continue;
            }
            edges.insert(
                edge.label.clone(),
                serde_json::json!({ "source": src, "target": tgt }),
            );
        }

        serde_json::json!({ "nodes": nodes, "edges": edges })
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
                let desc = p.description.as_str();
                let nullable = if p.nullable { ", nullable" } else { "" };
                let hints = property_hints(p);
                props.insert(
                    p.name.clone(),
                    serde_json::Value::String(
                        format!("{}{}{} {}", p.property_type, nullable, hints, desc)
                            .trim()
                            .to_string(),
                    ),
                );
            }
            let mut node_obj = serde_json::Map::new();
            let mut node_description = String::new();
            if !node.description.is_empty() {
                node_description.push_str(node.description.default_str());
            }
            if node.deprecated_at.is_some() {
                if !node_description.is_empty() {
                    node_description.push_str(" — ");
                }
                node_description.push_str("[DEPRECATED]");
            }
            if !node_description.is_empty() {
                node_obj.insert(
                    "description".into(),
                    serde_json::Value::String(node_description),
                );
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
                // Surface functional endpoint roles so the LLM sees that
                // MANAGES carries "manager"/"direct_report" semantics
                // rather than treating the edge label as the only hint.
                if let Some(role) = &edge.source_role {
                    edge_obj.insert(
                        "source_role".into(),
                        serde_json::Value::String(role.clone()),
                    );
                }
                if let Some(role) = &edge.target_role {
                    edge_obj.insert(
                        "target_role".into(),
                        serde_json::Value::String(role.clone()),
                    );
                }
                let mut edge_description = String::new();
                if !edge.description.is_empty() {
                    edge_description.push_str(edge.description.default_str());
                }
                if edge.deprecated_at.is_some() {
                    if !edge_description.is_empty() {
                        edge_description.push_str(" — ");
                    }
                    edge_description.push_str("[DEPRECATED]");
                }
                if !edge_description.is_empty() {
                    edge_obj.insert(
                        "description".into(),
                        serde_json::Value::String(edge_description),
                    );
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

/// Build a bracketed hint suffix carrying Phase A semantic flags for a
/// property: `", localized"`, `", deprecated"`, `", min N"`, `", max N"`.
/// Empty string when the property has no special flags. Kept on one line
/// so it slots into `compact_schema` without bloating the per-property
/// token budget — an LLM consuming this view gets the governance hints
/// inline rather than having to cross-reference a sidecar table.
fn property_hints(p: &PropertyDef) -> String {
    let mut hints: Vec<String> = Vec::new();
    if p.is_localized {
        hints.push("localized".into());
    }
    if let Some(min) = p.min_count {
        hints.push(format!("min {min}"));
    }
    if let Some(max) = p.max_count {
        hints.push(format!("max {max}"));
    }
    if p.deprecated_at.is_some() {
        hints.push("deprecated".into());
    }
    if hints.is_empty() {
        String::new()
    } else {
        format!(", {}", hints.join(", "))
    }
}
