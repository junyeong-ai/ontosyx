//! ResolverCtx — mutable scope state shared across pattern/expr/projection
//! resolution.
//!
//! Owns the per-query `var_nodes`/`var_edges` maps, the scope stack, and the
//! growing collection of bindings. Each resolution submodule (`ops`,
//! `patterns`, `exprs`) adds its own `impl ResolverCtx` block.

use std::collections::HashMap;

use crate::ontology_ir::OntologyIR;
use crate::query_ir::FieldRef;

use super::{
    BindingKind, EdgeBinding, NodeBinding, PropertyBinding, PropertyUsageHint,
    ResolvedQueryBindings, ScopeSegment,
};

/// Saved variable scope for isolation at UNION/EXISTS boundaries.
#[derive(Clone)]
pub(super) struct VarSnapshot {
    pub nodes: HashMap<String, (String, String)>,
    pub edges: HashMap<String, (String, String, String, String)>,
}

pub(super) struct ResolverCtx<'a> {
    pub(super) ontology: &'a OntologyIR,
    /// variable → (node_id, label) for property resolution lookups.
    /// Scope-isolated: saved/restored at UNION and EXISTS boundaries.
    pub(super) var_nodes: HashMap<String, (String, String)>,
    /// variable → (edge_id, label, source_node_id, target_node_id).
    /// Scope-isolated: saved/restored at UNION and EXISTS boundaries.
    pub(super) var_edges: HashMap<String, (String, String, String, String)>,
    /// All node bindings (no dedup — each occurrence is recorded)
    pub(super) node_bindings: Vec<NodeBinding>,
    /// All edge bindings (no dedup)
    pub(super) edge_bindings: Vec<EdgeBinding>,
    /// All property bindings (no dedup — allows same property in WHERE + ORDER BY)
    pub(super) property_bindings: Vec<PropertyBinding>,
    /// Current binding kind
    pub(super) binding_kind: BindingKind,
    /// Current pattern index within a Match operation
    pub(super) pattern_index: usize,
    /// Current scope path (pushed/popped at scope boundaries)
    pub(super) scope_path: Vec<ScopeSegment>,
    /// EXISTS nesting depth counter
    pub(super) exists_depth: usize,
    /// Current AST location hint for property bindings
    pub(super) usage_hint: PropertyUsageHint,
}

impl<'a> ResolverCtx<'a> {
    pub(super) fn new(ontology: &'a OntologyIR) -> Self {
        Self {
            ontology,
            var_nodes: HashMap::new(),
            var_edges: HashMap::new(),
            node_bindings: Vec::new(),
            edge_bindings: Vec::new(),
            property_bindings: Vec::new(),
            binding_kind: BindingKind::Match,
            pattern_index: 0,
            scope_path: vec![ScopeSegment::Root],
            exists_depth: 0,
            usage_hint: PropertyUsageHint::General,
        }
    }

    pub(super) fn into_bindings(self) -> ResolvedQueryBindings {
        ResolvedQueryBindings {
            node_bindings: self.node_bindings,
            edge_bindings: self.edge_bindings,
            property_bindings: self.property_bindings,
        }
    }

    /// Snapshot current variable scope for later restoration.
    pub(super) fn snapshot_vars(&self) -> VarSnapshot {
        VarSnapshot {
            nodes: self.var_nodes.clone(),
            edges: self.var_edges.clone(),
        }
    }

    /// Restore variable scope from a snapshot.
    pub(super) fn restore_vars(&mut self, snapshot: VarSnapshot) {
        self.var_nodes = snapshot.nodes;
        self.var_edges = snapshot.edges;
    }

    /// Bind a graph variable to its ontology node type.
    pub(super) fn bind_node_variable(&mut self, variable: &str, label: &str) {
        if let Some(node) = self.ontology.node_types.iter().find(|n| n.label == *label) {
            self.var_nodes
                .entry(variable.to_string())
                .or_insert_with(|| (node.id.to_string(), node.label.clone()));
            self.node_bindings.push(NodeBinding {
                variable: variable.to_string(),
                node_id: node.id.to_string(),
                label: node.label.clone(),
                binding_kind: self.binding_kind,
                pattern_index: self.pattern_index,
                scope_path: self.scope_path.clone(),
            });
        }
    }

    /// Resolve `variable.property` against the active scope, recording a
    /// `PropertyBinding` if both the variable and property name match.
    pub(super) fn resolve_variable_property(&mut self, variable: &str, property_name: &str) {
        let binding_kind = self.binding_kind;
        let scope_path = self.scope_path.clone();
        let usage_hint = self.usage_hint;

        // Try node first
        if let Some((node_id, _)) = self.var_nodes.get(variable)
            && let Some(node) = self.ontology.node_types.iter().find(|n| n.id == *node_id)
            && let Some(prop) = node.properties.iter().find(|p| p.name == property_name)
        {
            self.property_bindings.push(PropertyBinding {
                owner_variable: Some(variable.to_string()),
                property_name: property_name.to_string(),
                property_id: prop.id.to_string(),
                owner_id: node_id.clone(),
                binding_kind,
                scope_path: scope_path.clone(),
                usage_hint,
            });
            return;
        }

        // Try edge
        if let Some((edge_id, _, _, _)) = self.var_edges.get(variable)
            && let Some(edge) = self.ontology.edge_types.iter().find(|e| e.id == *edge_id)
            && let Some(prop) = edge.properties.iter().find(|p| p.name == property_name)
        {
            self.property_bindings.push(PropertyBinding {
                owner_variable: Some(variable.to_string()),
                property_name: property_name.to_string(),
                property_id: prop.id.to_string(),
                owner_id: edge_id.clone(),
                binding_kind,
                scope_path,
                usage_hint,
            });
        }
    }

    pub(super) fn resolve_field_ref(&mut self, field_ref: &FieldRef) {
        if let Some(field) = &field_ref.field {
            self.resolve_variable_property(&field_ref.variable, field);
        }
    }
}
