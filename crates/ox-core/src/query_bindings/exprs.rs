//! Expression and projection resolution: walks WHERE clauses, projection
//! lists, and inline subqueries to record property/variable references.

use crate::query_ir::{Expr, Projection};

use super::ctx::ResolverCtx;
use super::{BindingKind, PropertyUsageHint, ScopeSegment};

impl ResolverCtx<'_> {
    pub(super) fn resolve_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Property { variable, field } => {
                if let Some(field) = field {
                    self.resolve_variable_property(variable.as_str(), field);
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
                // Mirror `Expr::Exists` and `QueryOp::CallSubquery`:
                // bump `exists_depth` so nested subqueries get
                // distinct scope-path segments.
                let snapshot = self.snapshot_vars();
                self.exists_depth += 1;
                self.scope_path.push(ScopeSegment::ExistsSubquery {
                    depth: self.exists_depth,
                });
                self.resolve_op(&query.operation);
                self.scope_path.pop();
                self.exists_depth -= 1;
                self.restore_vars(snapshot);
            }
        }
    }

    pub(super) fn resolve_projection(&mut self, proj: &Projection) {
        match proj {
            Projection::Field {
                variable, field, ..
            } => {
                self.resolve_variable_property(variable.as_str(), field);
            }
            Projection::Variable { .. } | Projection::AllProperties { .. } => {}
            Projection::Expression { expr, .. } => self.resolve_expr(expr),
            Projection::Aggregation { argument, .. } => {
                if let Some(arg) = argument {
                    let prev_hint = self.usage_hint;
                    self.usage_hint = PropertyUsageHint::Aggregation;
                    self.resolve_projection(arg);
                    self.usage_hint = prev_hint;
                }
            }
        }
    }
}
