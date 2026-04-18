//! Top-level QueryOp dispatch — walks the AST and routes each variant
//! through the appropriate scope/binding logic.

use crate::query_ir::QueryOp;

use super::ctx::ResolverCtx;
use super::{BindingKind, PropertyUsageHint, ScopeSegment};

impl ResolverCtx<'_> {
    pub(super) fn resolve_op(&mut self, op: &QueryOp) {
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
                                    edge.label.to_string(),
                                    edge.source_node_id.to_string(),
                                    edge.target_node_id.to_string(),
                                )
                            });
                            self.edge_bindings.push(super::EdgeBinding {
                                variable: None,
                                edge_id: edge.id.to_string(),
                                label: edge.label.to_string(),
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
                    self.restore_vars(snapshot.clone());
                    self.scope_path.push(ScopeSegment::UnionBranch { index: i });
                    self.resolve_op(&q.operation);
                    self.scope_path.pop();
                }
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
                // Sub-queries get isolated scope. Bump `exists_depth`
                // before pushing the segment so nested CallSubquery /
                // EXISTS / Subquery layers stack as
                //   { depth: 1 } / { depth: 2 } / …
                // instead of all collapsing onto `depth: 1` (the prior
                // bug where the counter was only incremented inside
                // `Expr::Exists`).
                let snapshot = self.snapshot_vars();
                self.exists_depth += 1;
                self.scope_path.push(ScopeSegment::ExistsSubquery {
                    depth: self.exists_depth,
                });
                self.resolve_op(&inner.operation);
                self.scope_path.pop();
                self.exists_depth -= 1;
                self.restore_vars(snapshot);
            }

            QueryOp::Analytics {
                source,
                projections,
                params,
                ..
            } => {
                if let crate::query_ir::AnalyticsSource::Subgraph { filter } = source {
                    self.resolve_op(filter);
                }
                for expr in params.values() {
                    self.resolve_expr(expr);
                }
                let prev_hint = self.usage_hint;
                self.usage_hint = PropertyUsageHint::Projection;
                for proj in projections {
                    self.resolve_projection(proj);
                }
                self.usage_hint = prev_hint;
            }
        }
    }
}
