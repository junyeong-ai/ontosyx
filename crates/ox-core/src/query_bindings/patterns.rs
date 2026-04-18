//! Graph pattern resolution: nodes, relationships, paths. Walks `GraphPattern`
//! and `NodeRef` AST and binds variables to ontology entities for provenance.

use crate::query_ir::{GraphPattern, NodeRef, PathElement, PropertyFilter};

use super::ctx::ResolverCtx;
use super::{EdgeBinding, PropertyUsageHint};

impl ResolverCtx<'_> {
    pub(super) fn resolve_pattern(&mut self, pattern: &GraphPattern) {
        match pattern {
            GraphPattern::Node {
                variable,
                label,
                property_filters,
            } => {
                if let Some(label) = label {
                    self.bind_node_variable(variable, label);
                }
                let prev_hint = self.usage_hint;
                self.usage_hint = PropertyUsageHint::PatternFilter;
                for pf in property_filters {
                    self.resolve_property_filter(variable, pf);
                }
                self.usage_hint = prev_hint;
            }

            GraphPattern::Relationship {
                variable,
                label,
                source,
                target,
                property_filters,
                ..
            } => {
                if let Some(label) = label {
                    let source_node_id = self.var_nodes.get(source).map(|(id, _)| id.clone());
                    let target_node_id = self.var_nodes.get(target).map(|(id, _)| id.clone());

                    let edge = self.ontology.edge_types.iter().find(|e| {
                        e.label == *label
                            && source_node_id
                                .as_ref()
                                .is_none_or(|id| &e.source_node_id == id)
                            && target_node_id
                                .as_ref()
                                .is_none_or(|id| &e.target_node_id == id)
                    });

                    if let Some(edge) = edge {
                        let var_key = variable.as_deref().unwrap_or("").to_string();
                        self.var_edges.entry(var_key).or_insert_with(|| {
                            (
                                edge.id.to_string(),
                                edge.label.to_string(),
                                edge.source_node_id.to_string(),
                                edge.target_node_id.to_string(),
                            )
                        });
                        self.edge_bindings.push(EdgeBinding {
                            variable: variable.clone(),
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

                if let Some(var) = variable {
                    let prev_hint = self.usage_hint;
                    self.usage_hint = PropertyUsageHint::PatternFilter;
                    for pf in property_filters {
                        self.resolve_property_filter(var, pf);
                    }
                    self.usage_hint = prev_hint;
                }
            }

            GraphPattern::Path { elements } => {
                // First pass: resolve all nodes
                for elem in elements {
                    if let PathElement::Node {
                        variable,
                        label: Some(label),
                    } = elem
                    {
                        self.bind_node_variable(variable, label);
                    }
                }
                // Second pass: resolve edges with node context
                for (i, elem) in elements.iter().enumerate() {
                    if let PathElement::Edge {
                        variable,
                        label: Some(label),
                        ..
                    } = elem
                    {
                        let prev_node_id = if i > 0 {
                            if let PathElement::Node { variable, .. } = &elements[i - 1] {
                                self.var_nodes
                                    .get(variable.as_str())
                                    .map(|(id, _)| id.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let next_node_id = if i + 1 < elements.len() {
                            if let PathElement::Node { variable, .. } = &elements[i + 1] {
                                self.var_nodes
                                    .get(variable.as_str())
                                    .map(|(id, _)| id.clone())
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let edge = self.ontology.edge_types.iter().find(|e| {
                            e.label == *label
                                && prev_node_id
                                    .as_ref()
                                    .is_none_or(|id| &e.source_node_id == id)
                                && next_node_id
                                    .as_ref()
                                    .is_none_or(|id| &e.target_node_id == id)
                        });

                        if let Some(edge) = edge {
                            let var_key = variable.as_deref().unwrap_or("").to_string();
                            self.var_edges.entry(var_key).or_insert_with(|| {
                                (
                                    edge.id.to_string(),
                                    edge.label.to_string(),
                                    edge.source_node_id.to_string(),
                                    edge.target_node_id.to_string(),
                                )
                            });
                            self.edge_bindings.push(EdgeBinding {
                                variable: variable.clone(),
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
                }
            }
        }
    }

    pub(super) fn resolve_node_ref(&mut self, node_ref: &NodeRef) {
        if let Some(label) = &node_ref.label {
            self.bind_node_variable(&node_ref.variable, label);
        }
        let prev_hint = self.usage_hint;
        self.usage_hint = PropertyUsageHint::PatternFilter;
        for pf in &node_ref.property_filters {
            self.resolve_property_filter(&node_ref.variable, pf);
        }
        self.usage_hint = prev_hint;
    }

    pub(super) fn resolve_property_filter(&mut self, variable: &str, pf: &PropertyFilter) {
        self.resolve_variable_property(variable, &pf.property);
        self.resolve_expr(&pf.value);
    }
}
