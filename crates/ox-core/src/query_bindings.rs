//! Resolve QueryIR variable bindings against an OntologyIR.
//!
//! Walks the QueryIR AST and produces [`ResolvedQueryBindings`] — a structured
//! provenance record of which ontology entities (nodes, edges, properties) each
//! variable/pattern references. This powers "Show on graph" highlighting.
//!
//! **Scope-aware**: UNION branches and EXISTS sub-queries each get isolated
//! variable scopes. Variables defined inside a scope don't leak into siblings
//! or the outer scope. Property bindings track scope paths and allow duplicates
//! for the same property used in different contexts (WHERE + ORDER BY etc).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::ontology_ir::OntologyIR;
use crate::query_ir::{
    Expr, FieldRef, GraphPattern, MutateOp, NodeRef, PathElement, Projection, PropertyFilter,
    QueryIR, QueryOp,
};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResolvedQueryBindings {
    pub node_bindings: Vec<NodeBinding>,
    pub edge_bindings: Vec<EdgeBinding>,
    pub property_bindings: Vec<PropertyBinding>,
}

/// Which kind of query operation produced this binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    Match,
    PathFind,
    Chain,
    Exists,
    Mutation,
}

/// Scope path segment for nested query constructs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ScopeSegment {
    Root,
    UnionBranch { index: usize },
    ExistsSubquery { depth: usize },
    ChainStep { index: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeBinding {
    pub variable: String,
    pub node_id: String,
    pub label: String,
    pub binding_kind: BindingKind,
    pub pattern_index: usize,
    pub scope_path: Vec<ScopeSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeBinding {
    pub variable: Option<String>,
    pub edge_id: String,
    pub label: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub binding_kind: BindingKind,
    pub pattern_index: usize,
    pub scope_path: Vec<ScopeSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyBinding {
    pub owner_variable: Option<String>,
    pub property_name: String,
    pub property_id: String,
    pub owner_id: String,
    pub binding_kind: BindingKind,
    pub scope_path: Vec<ScopeSegment>,
    /// AST location hint for UI disambiguation (e.g. "filter", "projection", "order_by").
    pub usage_hint: PropertyUsageHint,
}

/// Where in the AST a property reference was encountered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyUsageHint {
    PatternFilter,
    WhereFilter,
    Projection,
    OrderBy,
    GroupBy,
    Aggregation,
    Mutation,
    General,
}

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

/// Resolve all variable bindings in a QueryIR against an OntologyIR.
///
/// Walks patterns, filters, projections, and sub-queries to extract which
/// ontology nodes/edges/properties are referenced. Scopes are isolated for
/// UNION branches and EXISTS sub-queries to prevent variable leakage.
pub fn resolve_query_bindings(query: &QueryIR, ontology: &OntologyIR) -> ResolvedQueryBindings {
    let mut ctx = ResolverCtx::new(ontology);
    ctx.resolve_op(&query.operation);

    // Also resolve ORDER BY projections
    let prev_hint = ctx.usage_hint;
    ctx.usage_hint = PropertyUsageHint::OrderBy;
    for clause in &query.order_by {
        ctx.resolve_projection(&clause.projection);
    }
    ctx.usage_hint = prev_hint;

    ctx.into_bindings()
}

// ---------------------------------------------------------------------------
// Internal resolver context
// ---------------------------------------------------------------------------

/// Saved variable scope for isolation at UNION/EXISTS boundaries.
#[derive(Clone)]
struct VarSnapshot {
    nodes: HashMap<String, (String, String)>,
    edges: HashMap<String, (String, String, String, String)>,
}

struct ResolverCtx<'a> {
    ontology: &'a OntologyIR,
    /// variable → (node_id, label) for property resolution lookups.
    /// Scope-isolated: saved/restored at UNION and EXISTS boundaries.
    var_nodes: HashMap<String, (String, String)>,
    /// variable → (edge_id, label, source_node_id, target_node_id).
    /// Scope-isolated: saved/restored at UNION and EXISTS boundaries.
    var_edges: HashMap<String, (String, String, String, String)>,
    /// All node bindings (no dedup — each occurrence is recorded)
    node_bindings: Vec<NodeBinding>,
    /// All edge bindings (no dedup)
    edge_bindings: Vec<EdgeBinding>,
    /// All property bindings (no dedup — allows same property in WHERE + ORDER BY)
    property_bindings: Vec<PropertyBinding>,
    /// Current binding kind
    binding_kind: BindingKind,
    /// Current pattern index within a Match operation
    pattern_index: usize,
    /// Current scope path (pushed/popped at scope boundaries)
    scope_path: Vec<ScopeSegment>,
    /// EXISTS nesting depth counter
    exists_depth: usize,
    /// Current AST location hint for property bindings
    usage_hint: PropertyUsageHint,
}

impl<'a> ResolverCtx<'a> {
    fn new(ontology: &'a OntologyIR) -> Self {
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

    fn into_bindings(self) -> ResolvedQueryBindings {
        ResolvedQueryBindings {
            node_bindings: self.node_bindings,
            edge_bindings: self.edge_bindings,
            property_bindings: self.property_bindings,
        }
    }

    /// Snapshot current variable scope for later restoration.
    fn snapshot_vars(&self) -> VarSnapshot {
        VarSnapshot {
            nodes: self.var_nodes.clone(),
            edges: self.var_edges.clone(),
        }
    }

    /// Restore variable scope from a snapshot.
    fn restore_vars(&mut self, snapshot: VarSnapshot) {
        self.var_nodes = snapshot.nodes;
        self.var_edges = snapshot.edges;
    }

    // -- Top-level operation dispatch --

    fn resolve_op(&mut self, op: &QueryOp) {
        match op {
            QueryOp::Match {
                patterns,
                filter,
                projections,
                group_by,
                ..
            } => {
                let prev_kind = self.binding_kind;
                self.binding_kind = BindingKind::Match;
                for (i, pat) in patterns.iter().enumerate() {
                    self.pattern_index = i;
                    self.resolve_pattern(pat);
                }
                if let Some(expr) = filter {
                    let prev_hint = self.usage_hint;
                    self.usage_hint = PropertyUsageHint::WhereFilter;
                    self.resolve_expr(expr);
                    self.usage_hint = prev_hint;
                }
                let prev_hint = self.usage_hint;
                self.usage_hint = PropertyUsageHint::Projection;
                for proj in projections {
                    self.resolve_projection(proj);
                }
                self.usage_hint = PropertyUsageHint::GroupBy;
                for proj in group_by {
                    self.resolve_projection(proj);
                }
                self.usage_hint = prev_hint;
                self.binding_kind = prev_kind;
            }

            QueryOp::PathFind {
                start,
                end,
                edge_types,
                ..
            } => {
                let prev_kind = self.binding_kind;
                self.binding_kind = BindingKind::PathFind;
                self.pattern_index = 0;
                self.resolve_node_ref(start);
                self.resolve_node_ref(end);
                // PathFind edge_types are labels — resolve all matching edges
                for label in edge_types {
                    for edge in &self.ontology.edge_types {
                        if edge.label == *label {
                            let key = format!("__pathfind_{}", edge.id);
                            self.var_edges.entry(key).or_insert_with(|| {
                                (
                                    edge.id.to_string(),
                                    edge.label.clone(),
                                    edge.source_node_id.to_string(),
                                    edge.target_node_id.to_string(),
                                )
                            });
                            self.edge_bindings.push(EdgeBinding {
                                variable: None,
                                edge_id: edge.id.to_string(),
                                label: edge.label.clone(),
                                source_node_id: edge.source_node_id.to_string(),
                                target_node_id: edge.target_node_id.to_string(),
                                binding_kind: BindingKind::PathFind,
                                pattern_index: 0,
                                scope_path: self.scope_path.clone(),
                            });
                        }
                    }
                }
                self.binding_kind = prev_kind;
            }

            QueryOp::Aggregate {
                source,
                group_by,
                aggregations,
            } => {
                self.resolve_op(&source.operation);
                let prev_hint = self.usage_hint;
                self.usage_hint = PropertyUsageHint::OrderBy;
                for clause in &source.order_by {
                    self.resolve_projection(&clause.projection);
                }
                self.usage_hint = PropertyUsageHint::GroupBy;
                for field in group_by {
                    self.resolve_field_ref(field);
                }
                self.usage_hint = PropertyUsageHint::Aggregation;
                for agg in aggregations {
                    self.resolve_field_ref(&agg.field);
                }
                self.usage_hint = prev_hint;
            }

            QueryOp::Union { queries, .. } => {
                // Each UNION branch gets its own isolated variable scope.
                // Variables from branch A do NOT leak into branch B.
                let snapshot = self.snapshot_vars();
                for (i, q) in queries.iter().enumerate() {
                    // Restore base scope for each branch (not cumulative)
                    self.restore_vars(snapshot.clone());
                    self.scope_path.push(ScopeSegment::UnionBranch { index: i });
                    self.resolve_op(&q.operation);
                    self.scope_path.pop();
                }
                // Restore original scope after UNION (UNION doesn't export vars)
                self.restore_vars(snapshot);
            }

            QueryOp::Chain { steps } => {
                let prev_kind = self.binding_kind;
                self.binding_kind = BindingKind::Chain;
                for (i, step) in steps.iter().enumerate() {
                    self.scope_path.push(ScopeSegment::ChainStep { index: i });
                    let prev_hint = self.usage_hint;
                    self.usage_hint = PropertyUsageHint::Projection;
                    for proj in &step.pass_through {
                        self.resolve_projection(proj);
                    }
                    self.usage_hint = prev_hint;
                    self.resolve_op(&step.operation);
                    self.scope_path.pop();
                }
                self.binding_kind = prev_kind;
            }

            QueryOp::Mutate {
                context,
                operations,
                returning,
            } => {
                if let Some(ctx_op) = context {
                    self.resolve_op(ctx_op);
                }
                let prev_kind = self.binding_kind;
                let prev_hint = self.usage_hint;
                self.binding_kind = BindingKind::Mutation;
                self.usage_hint = PropertyUsageHint::Mutation;
                self.pattern_index = 0;
                for mutation in operations {
                    self.resolve_mutation(mutation);
                }
                self.usage_hint = PropertyUsageHint::Projection;
                for proj in returning {
                    self.resolve_projection(proj);
                }
                self.binding_kind = prev_kind;
                self.usage_hint = prev_hint;
            }

            QueryOp::CallSubquery { inner, .. } => {
                // Subquery gets its own scope — resolve the inner query
                let snapshot = self.snapshot_vars();
                self.scope_path.push(ScopeSegment::ExistsSubquery {
                    depth: self.exists_depth + 1,
                });
                self.resolve_op(&inner.operation);
                self.scope_path.pop();
                self.restore_vars(snapshot);
            }

            QueryOp::Analytics {
                source,
                projections,
                params,
                ..
            } => {
                // Resolve the source subgraph if present
                if let crate::query_ir::AnalyticsSource::Subgraph { filter } = source {
                    self.resolve_op(filter);
                }
                // Resolve param expressions
                for expr in params.values() {
                    self.resolve_expr(expr);
                }
                // Resolve projections
                let prev_hint = self.usage_hint;
                self.usage_hint = PropertyUsageHint::Projection;
                for proj in projections {
                    self.resolve_projection(proj);
                }
                self.usage_hint = prev_hint;
            }
        }
    }

    // -- Pattern resolution --

    fn resolve_pattern(&mut self, pattern: &GraphPattern) {
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
                    // Resolve edge: find by label + source/target context
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
                                edge.label.clone(),
                                edge.source_node_id.to_string(),
                                edge.target_node_id.to_string(),
                            )
                        });
                        self.edge_bindings.push(EdgeBinding {
                            variable: variable.clone(),
                            edge_id: edge.id.to_string(),
                            label: edge.label.clone(),
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
                                    edge.label.clone(),
                                    edge.source_node_id.to_string(),
                                    edge.target_node_id.to_string(),
                                )
                            });
                            self.edge_bindings.push(EdgeBinding {
                                variable: variable.clone(),
                                edge_id: edge.id.to_string(),
                                label: edge.label.clone(),
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

    fn resolve_node_ref(&mut self, node_ref: &NodeRef) {
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

    fn bind_node_variable(&mut self, variable: &str, label: &str) {
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

    // -- Expression resolution --

    fn resolve_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Property { variable, field } => {
                if let Some(field) = field {
                    self.resolve_variable_property(variable, field);
                }
            }
            Expr::Comparison { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::Logical { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::Not { inner } => self.resolve_expr(inner),
            Expr::In { expr, .. } => self.resolve_expr(expr),
            Expr::IsNull { expr, .. } => self.resolve_expr(expr),
            Expr::StringOp { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.resolve_expr(arg);
                }
            }
            Expr::Exists { pattern } => {
                // EXISTS gets an isolated scope: variables defined inside
                // don't leak into the outer scope.
                let snapshot = self.snapshot_vars();
                let prev_kind = self.binding_kind;
                let prev_index = self.pattern_index;
                self.binding_kind = BindingKind::Exists;
                self.pattern_index = 0;
                self.exists_depth += 1;
                self.scope_path.push(ScopeSegment::ExistsSubquery {
                    depth: self.exists_depth,
                });
                self.resolve_pattern(pattern);
                self.scope_path.pop();
                self.exists_depth -= 1;
                self.binding_kind = prev_kind;
                self.pattern_index = prev_index;
                // Restore outer scope — EXISTS variables don't leak
                self.restore_vars(snapshot);
            }
            Expr::Case {
                operand,
                when_clauses,
                else_result,
            } => {
                if let Some(op) = operand {
                    self.resolve_expr(op);
                }
                for wc in when_clauses {
                    self.resolve_expr(&wc.condition);
                    self.resolve_expr(&wc.result);
                }
                if let Some(els) = else_result {
                    self.resolve_expr(els);
                }
            }
            Expr::Literal { .. } => {}
            Expr::Subquery { query, .. } => {
                // Subquery expression gets isolated scope
                let snapshot = self.snapshot_vars();
                self.scope_path.push(ScopeSegment::ExistsSubquery {
                    depth: self.exists_depth + 1,
                });
                self.resolve_op(&query.operation);
                self.scope_path.pop();
                self.restore_vars(snapshot);
            }
        }
    }

    fn resolve_property_filter(&mut self, variable: &str, pf: &PropertyFilter) {
        self.resolve_variable_property(variable, &pf.property);
        self.resolve_expr(&pf.value);
    }

    fn resolve_variable_property(&mut self, variable: &str, property_name: &str) {
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

    // -- Projection resolution --

    fn resolve_projection(&mut self, proj: &Projection) {
        match proj {
            Projection::Field {
                variable, field, ..
            } => {
                self.resolve_variable_property(variable, field);
            }
            Projection::Variable { .. } | Projection::AllProperties { .. } => {}
            Projection::Expression { expr, .. } => self.resolve_expr(expr),
            Projection::Aggregation { argument, .. } => {
                let prev_hint = self.usage_hint;
                self.usage_hint = PropertyUsageHint::Aggregation;
                self.resolve_projection(argument);
                self.usage_hint = prev_hint;
            }
        }
    }

    fn resolve_field_ref(&mut self, field_ref: &FieldRef) {
        if let Some(field) = &field_ref.field {
            self.resolve_variable_property(&field_ref.variable, field);
        }
    }

    // -- Mutation resolution --

    fn resolve_mutation(&mut self, mutation: &MutateOp) {
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
                            edge.label.clone(),
                            edge.source_node_id.to_string(),
                            edge.target_node_id.to_string(),
                        )
                    });
                    self.edge_bindings.push(EdgeBinding {
                        variable: None,
                        edge_id: edge.id.to_string(),
                        label: edge.label.clone(),
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology_ir::*;
    use crate::query_ir::*;
    use crate::types::{Direction, PropertyType, PropertyValue};

    fn test_ontology() -> OntologyIR {
        OntologyIR::new(
            "ont1".into(),
            "Test".into(),
            None,
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Person".into(),
                    description: None,
                    source_table: None,
                    properties: vec![PropertyDef {
                        id: "p1".into(),
                        name: "name".into(),
                        property_type: PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: None,
                    }],
                    constraints: vec![],
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Company".into(),
                    description: None,
                    source_table: None,
                    properties: vec![PropertyDef {
                        id: "p2".into(),
                        name: "title".into(),
                        property_type: PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: None,
                    }],
                    constraints: vec![],
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "WORKS_AT".into(),
                description: None,
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
            }],
            vec![],
        )
    }

    #[test]
    fn test_match_pattern_bindings() {
        let ontology = test_ontology();
        let query = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![
                    GraphPattern::Node {
                        variable: "p".into(),
                        label: Some("Person".into()),
                        property_filters: vec![],
                    },
                    GraphPattern::Relationship {
                        variable: Some("r".into()),
                        label: Some("WORKS_AT".into()),
                        source: "p".into(),
                        target: "c".into(),
                        direction: Direction::Outgoing,
                        property_filters: vec![],
                        var_length: None,
                    },
                    GraphPattern::Node {
                        variable: "c".into(),
                        label: Some("Company".into()),
                        property_filters: vec![],
                    },
                ],
                filter: None,
                projections: vec![Projection::Field {
                    variable: "p".into(),
                    field: "name".into(),
                    alias: None,
                }],
                optional: false,
                group_by: vec![],
            },
            limit: Some(10),
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        assert_eq!(bindings.node_bindings.len(), 2);
        let p_bind = bindings
            .node_bindings
            .iter()
            .find(|b| b.variable == "p")
            .unwrap();
        assert_eq!(p_bind.node_id, "n1");
        assert_eq!(p_bind.binding_kind, BindingKind::Match);
        assert_eq!(p_bind.pattern_index, 0);
        assert_eq!(p_bind.scope_path, vec![ScopeSegment::Root]);

        let c_bind = bindings
            .node_bindings
            .iter()
            .find(|b| b.variable == "c")
            .unwrap();
        assert_eq!(c_bind.node_id, "n2");
        assert_eq!(c_bind.binding_kind, BindingKind::Match);
        assert_eq!(c_bind.pattern_index, 2);

        assert_eq!(bindings.edge_bindings.len(), 1);
        let eb = &bindings.edge_bindings[0];
        assert_eq!(eb.variable.as_deref(), Some("r"));
        assert_eq!(eb.edge_id, "e1");
        assert_eq!(eb.binding_kind, BindingKind::Match);
        assert_eq!(eb.pattern_index, 1);

        assert_eq!(bindings.property_bindings.len(), 1);
        assert_eq!(bindings.property_bindings[0].property_name, "name");
        assert_eq!(bindings.property_bindings[0].property_id, "p1");
        assert_eq!(
            bindings.property_bindings[0].binding_kind,
            BindingKind::Match
        );
        assert_eq!(
            bindings.property_bindings[0].usage_hint,
            PropertyUsageHint::Projection
        );
    }

    #[test]
    fn test_filter_property_bindings() {
        let ontology = test_ontology();
        let query = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "p".into(),
                    label: Some("Person".into()),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: "p".into(),
                        field: Some("name".into()),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("Alice".into()),
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);
        assert_eq!(bindings.property_bindings.len(), 1);
        assert_eq!(bindings.property_bindings[0].property_name, "name");
        assert_eq!(
            bindings.property_bindings[0].usage_hint,
            PropertyUsageHint::WhereFilter
        );
    }

    #[test]
    fn test_unknown_label_ignored() {
        let ontology = test_ontology();
        let query = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "x".into(),
                    label: Some("UnknownType".into()),
                    property_filters: vec![],
                }],
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);
        assert!(bindings.node_bindings.is_empty());
    }

    #[test]
    fn test_exists_subquery_scope_isolation() {
        let ontology = test_ontology();
        let query = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "p".into(),
                    label: Some("Person".into()),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Exists {
                    pattern: Box::new(GraphPattern::Relationship {
                        variable: Some("r".into()),
                        label: Some("WORKS_AT".into()),
                        source: "p".into(),
                        target: "c".into(),
                        direction: Direction::Outgoing,
                        property_filters: vec![],
                        var_length: None,
                    }),
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        // Node from MATCH pattern
        let p_bind = bindings
            .node_bindings
            .iter()
            .find(|b| b.variable == "p")
            .unwrap();
        assert_eq!(p_bind.binding_kind, BindingKind::Match);
        assert_eq!(p_bind.pattern_index, 0);
        assert_eq!(p_bind.scope_path, vec![ScopeSegment::Root]);

        // Edge from EXISTS subquery — has scope_path with ExistsSubquery
        assert_eq!(bindings.edge_bindings.len(), 1);
        let eb = &bindings.edge_bindings[0];
        assert_eq!(eb.binding_kind, BindingKind::Exists);
        assert_eq!(eb.pattern_index, 0);
        assert_eq!(
            eb.scope_path,
            vec![
                ScopeSegment::Root,
                ScopeSegment::ExistsSubquery { depth: 1 },
            ]
        );
    }

    #[test]
    fn test_union_branch_scope_isolation() {
        let ontology = test_ontology();
        // UNION of (MATCH Person) and (MATCH Company) — variables should not leak between branches
        let query = QueryIR {
            operation: QueryOp::Union {
                queries: vec![
                    QueryIR {
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: "x".into(),
                                label: Some("Person".into()),
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![Projection::Field {
                                variable: "x".into(),
                                field: "name".into(),
                                alias: None,
                            }],
                            optional: false,
                            group_by: vec![],
                        },
                        limit: None,
                        skip: None,
                        order_by: vec![],
                    },
                    QueryIR {
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: "x".into(),
                                label: Some("Company".into()),
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![Projection::Field {
                                variable: "x".into(),
                                field: "title".into(),
                                alias: None,
                            }],
                            optional: false,
                            group_by: vec![],
                        },
                        limit: None,
                        skip: None,
                        order_by: vec![],
                    },
                ],
                all: false,
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        // Two node bindings: x→Person (branch 0) and x→Company (branch 1)
        assert_eq!(bindings.node_bindings.len(), 2);

        let branch0 = bindings
            .node_bindings
            .iter()
            .find(|b| b.node_id == "n1")
            .unwrap();
        assert_eq!(branch0.variable, "x");
        assert!(
            branch0
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 0 })
        );

        let branch1 = bindings
            .node_bindings
            .iter()
            .find(|b| b.node_id == "n2")
            .unwrap();
        assert_eq!(branch1.variable, "x");
        assert!(
            branch1
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 1 })
        );

        // Property bindings: name (Person, branch 0) and title (Company, branch 1)
        assert_eq!(bindings.property_bindings.len(), 2);

        let name_bind = bindings
            .property_bindings
            .iter()
            .find(|b| b.property_name == "name")
            .unwrap();
        assert_eq!(name_bind.property_id, "p1");
        assert!(
            name_bind
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 0 })
        );

        let title_bind = bindings
            .property_bindings
            .iter()
            .find(|b| b.property_name == "title")
            .unwrap();
        assert_eq!(title_bind.property_id, "p2");
        assert!(
            title_bind
                .scope_path
                .contains(&ScopeSegment::UnionBranch { index: 1 })
        );
    }

    #[test]
    fn test_property_multi_use_not_deduped() {
        // Same property used in WHERE + RETURN should produce 2 bindings
        let ontology = test_ontology();
        let query = QueryIR {
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: "p".into(),
                    label: Some("Person".into()),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Comparison {
                    left: Box::new(Expr::Property {
                        variable: "p".into(),
                        field: Some("name".into()),
                    }),
                    op: ComparisonOp::Eq,
                    right: Box::new(Expr::Literal {
                        value: PropertyValue::String("Alice".into()),
                    }),
                }),
                projections: vec![Projection::Field {
                    variable: "p".into(),
                    field: "name".into(),
                    alias: None,
                }],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        // name used in WHERE filter AND projection — should be 2 distinct bindings
        assert_eq!(bindings.property_bindings.len(), 2);
        let hints: Vec<_> = bindings
            .property_bindings
            .iter()
            .map(|b| b.usage_hint)
            .collect();
        assert!(hints.contains(&PropertyUsageHint::WhereFilter));
        assert!(hints.contains(&PropertyUsageHint::Projection));
    }

    #[test]
    fn test_pathfind_binding_kind() {
        let ontology = test_ontology();
        let query = QueryIR {
            operation: QueryOp::PathFind {
                start: NodeRef {
                    variable: "s".into(),
                    label: Some("Person".into()),
                    property_filters: vec![],
                },
                end: NodeRef {
                    variable: "e".into(),
                    label: Some("Company".into()),
                    property_filters: vec![],
                },
                edge_types: vec!["WORKS_AT".into()],
                direction: Direction::Outgoing,
                max_depth: Some(3),
                algorithm: PathAlgorithm::ShortestPath,
            },
            limit: None,
            skip: None,
            order_by: vec![],
        };

        let bindings = resolve_query_bindings(&query, &ontology);

        assert_eq!(bindings.node_bindings.len(), 2);
        for nb in &bindings.node_bindings {
            assert_eq!(nb.binding_kind, BindingKind::PathFind);
        }

        assert_eq!(bindings.edge_bindings.len(), 1);
        assert_eq!(
            bindings.edge_bindings[0].binding_kind,
            BindingKind::PathFind
        );
        assert_eq!(bindings.edge_bindings[0].edge_id, "e1");
    }
}
