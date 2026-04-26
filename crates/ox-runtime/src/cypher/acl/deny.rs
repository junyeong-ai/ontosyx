//! `Deny` action — inject `WHERE false` on every MATCH whose
//! pattern touches a denied resource. The constant predicate
//! collapses the clause to zero rows regardless of any other
//! conditions the author wrote.

use crate::cypher::ast::{
    ClauseKind, CypherClause, CypherPatternElement, CypherStatement,
};
use crate::cypher::rewrite_helpers::{
    find_following_where_clause, split_leading_whitespace, strip_leading_keyword,
};
use crate::cypher::token::Span;

use super::snapshot::{AclAction, AclSnapshot};

#[derive(Default)]
pub(super) struct DeniedResources {
    /// Specific node labels denied (case-sensitive — Cypher labels
    /// are case-sensitive in Neo4j).
    node_labels: Vec<String>,
    /// Specific edge types denied.
    edge_types: Vec<String>,
    /// `resource_value: None` on a `label` policy — every node label
    /// is denied for the principal.
    wildcard_node: bool,
    /// `resource_value: None` on an `edge_label` policy — every edge
    /// type is denied.
    wildcard_edge: bool,
}

impl DeniedResources {
    pub(super) fn is_empty(&self) -> bool {
        self.node_labels.is_empty()
            && self.edge_types.is_empty()
            && !self.wildcard_node
            && !self.wildcard_edge
    }
}

pub(super) fn collect_deny_resources(snapshot: &AclSnapshot) -> DeniedResources {
    let mut out = DeniedResources::default();
    for policy in &snapshot.policies {
        if policy.action != AclAction::Deny {
            continue;
        }
        match policy.resource_type.as_str() {
            "label" => match &policy.resource_value {
                Some(v) if !v.is_empty() && v != "*" => {
                    if !out.node_labels.contains(v) {
                        out.node_labels.push(v.clone());
                    }
                }
                _ => out.wildcard_node = true,
            },
            "edge_label" => match &policy.resource_value {
                Some(v) if !v.is_empty() && v != "*" => {
                    if !out.edge_types.contains(v) {
                        out.edge_types.push(v.clone());
                    }
                }
                _ => out.wildcard_edge = true,
            },
            _ => {}
        }
    }
    out
}

/// Walk every MATCH / OPTIONAL MATCH clause in `statement`. If the
/// clause's pattern contains a denied node label or edge type,
/// insert `WHERE false AND <prior body>` so the clause produces no
/// rows. Returns true when any clause was modified.
pub(super) fn inject_deny_in_statement(
    statement: &mut CypherStatement,
    denied: &DeniedResources,
) -> bool {
    let mut targets: Vec<usize> = Vec::new();
    for (idx, clause) in statement.clauses.iter().enumerate() {
        if !matches!(clause.kind, ClauseKind::Match | ClauseKind::OptionalMatch) {
            continue;
        }
        if clause_touches_denied_resource(clause, denied) {
            targets.push(idx);
        }
    }
    if targets.is_empty() {
        return false;
    }

    for &match_idx in targets.iter().rev() {
        match find_following_where_clause(statement, match_idx) {
            Some(where_idx) => {
                prepend_false_to_where(&mut statement.clauses[where_idx]);
            }
            None => {
                statement.clauses.insert(
                    match_idx + 1,
                    CypherClause {
                        kind: ClauseKind::Where,
                        tokens: Vec::new(),
                        text: " WHERE false".to_string(),
                        span: Span::default(),
                        patterns: Vec::new(),
                    },
                );
            }
        }
    }
    true
}

fn clause_touches_denied_resource(
    clause: &CypherClause,
    denied: &DeniedResources,
) -> bool {
    for pattern in &clause.patterns {
        for element in &pattern.elements {
            match element {
                CypherPatternElement::Node(node) => {
                    if denied.wildcard_node && !node.labels.is_empty() {
                        return true;
                    }
                    for label in &node.labels {
                        if denied.node_labels.contains(label) {
                            return true;
                        }
                    }
                }
                CypherPatternElement::Relationship(rel) => {
                    if denied.wildcard_edge && !rel.types.is_empty() {
                        return true;
                    }
                    for ty in &rel.types {
                        if denied.edge_types.contains(ty) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Rewrite an existing WHERE so it begins with `false AND <body>`.
/// `false AND <…>` is `false` regardless of the body, so the body
/// stays verbatim — preserves comments and exotic syntax the parser
/// hasn't yet learned. An empty body collapses to `WHERE false`
/// (no trailing AND) so a follow-on parse round-trip stays clean.
fn prepend_false_to_where(clause: &mut CypherClause) {
    let original = std::mem::take(&mut clause.text);
    let (lead_ws, after_ws) = split_leading_whitespace(&original);
    let after_keyword = strip_leading_keyword(after_ws, "WHERE").unwrap_or(after_ws);
    let body = after_keyword.trim();
    clause.text = if body.is_empty() {
        format!("{lead_ws}WHERE false")
    } else {
        format!("{lead_ws}WHERE false AND {body}")
    };
}
