//! Mutation resolution: CREATE/MERGE node/edge, SET/REMOVE property,
//! DELETE, REMOVE LABEL. Complements the pattern-matching resolvers in
//! [`super::patterns`] — mutations bind variables and record edge/property
//! references the same way, but act on write-side IR nodes (`MutateOp`).

use crate::query_ir::MutateOp;

use super::EdgeBinding;
use super::ctx::ResolverCtx;

impl ResolverCtx<'_> {
    pub(super) fn resolve_mutation(&mut self, mutation: &MutateOp) {
        match mutation {
            MutateOp::CreateNode {
                variable, label, ..
            }
            | MutateOp::MergeNode {
                variable, label, ..
            } => {
                self.bind_node_variable(variable, label);
            }
            MutateOp::CreateEdge {
                label,
                source,
                target,
                ..
            }
            | MutateOp::MergeEdge {
                label,
                source,
                target,
                ..
            } => {
                let source_id = self
                    .var_nodes
                    .get(source.as_str())
                    .map(|(id, _)| id.clone());
                let target_id = self
                    .var_nodes
                    .get(target.as_str())
                    .map(|(id, _)| id.clone());
                if let Some(edge) = self.ontology.edge_types.iter().find(|e| {
                    e.label == *label
                        && source_id.as_ref().is_none_or(|id| &e.source_node_id == id)
                        && target_id.as_ref().is_none_or(|id| &e.target_node_id == id)
                }) {
                    let key = format!("__mutate_{}_{}", label, edge.id);
                    self.var_edges.entry(key).or_insert_with(|| {
                        (
                            edge.id.to_string(),
                            edge.label.to_string(),
                            edge.source_node_id.to_string(),
                            edge.target_node_id.to_string(),
                        )
                    });
                    self.edge_bindings.push(EdgeBinding {
                        variable: None,
                        edge_id: edge.id.to_string(),
                        label: edge.label.to_string(),
                        source_node_id: edge.source_node_id.to_string(),
                        target_node_id: edge.target_node_id.to_string(),
                        binding_kind: self.binding_kind,
                        pattern_index: self.pattern_index,
                        scope_path: self.scope_path.clone(),
                    });
                }
            }
            MutateOp::SetProperty {
                variable,
                property,
                value,
            } => {
                self.resolve_variable_property(variable, property);
                self.resolve_expr(value);
            }
            MutateOp::RemoveProperty { variable, property } => {
                self.resolve_variable_property(variable, property);
            }
            MutateOp::Delete { .. } | MutateOp::RemoveLabel { .. } => {}
        }
    }
}
