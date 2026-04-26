//! ACL row-level rewriter — Phase 5 (deny only).
//!
//! Drops policies live on a pre-filtered, principal-scoped
//! [`AclSnapshot`] threaded through [`crate::cypher::rewrite::RewriteContext`].
//! Per-MATCH inspection: when a node pattern's label matches a
//! `deny` policy's resource, the rewriter injects a constant `false`
//! predicate into that MATCH's WHERE so the clause yields no rows.
//! Existing predicates AND-combine with the constant; the existing
//! text doesn't have to be parsed structurally — `false AND <…>`
//! evaluates to false regardless.
//!
//! Phase 6 lifts the `mask` action; Phase 7 adds soft-delete. The
//! snapshot type is named so future actions can grow into the same
//! [`AclPolicySpec`] shape without churn.

use std::sync::Arc;

use crate::cypher::ast::{
    ClauseKind, CypherAst, CypherClause, CypherPatternElement, CypherStatement,
};
use crate::cypher::rewrite::{
    CypherRewriter, RewriteContext, RewriteError, RewritePhase, RewrittenAst,
};
use crate::cypher::token::Span;

// ---------------------------------------------------------------------------
// AclAction
// ---------------------------------------------------------------------------

/// Concrete actions a row-level policy can take. Phase 5 only
/// implements [`AclAction::Deny`]; [`AclAction::Mask`] lives here
/// already so the policy DTO doesn't grow a `String` action that
/// future passes have to re-validate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AclAction {
    /// Reject every row that matches this policy's resource.
    Deny,
    /// Replace specific properties with [`AclPolicySpec::mask_pattern`]
    /// (or a default placeholder) at projection time. **Wired in Phase 6.**
    Mask,
}

impl AclAction {
    /// Map an `acl_policies.action` string to its enum form. Returns
    /// `None` for unknown values — the loader on the ox-api side is
    /// responsible for surfacing the unknown action; the rewriter
    /// silently skips it so a typo can't accidentally re-classify a
    /// policy as `Deny`.
    pub fn from_db_string(s: &str) -> Option<Self> {
        match s {
            "deny" => Some(Self::Deny),
            "mask" => Some(Self::Mask),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// AclPolicySpec / AclSnapshot
// ---------------------------------------------------------------------------

/// Minimal policy slice the rewriter consumes. Independent of the
/// `ox-store` `AclPolicy` row so the rewriter layer doesn't pull a
/// persistence dependency. The ox-api loader converts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclPolicySpec {
    /// Concrete action to take when the resource matches.
    pub action: AclAction,
    /// Resource family. Phase 5 reads only `"label"` (node label
    /// match) and `"edge_label"` (edge type match). Other values are
    /// passed through unchanged.
    pub resource_type: String,
    /// Specific resource value (label or edge type). `None` is a
    /// wildcard match across the entire `resource_type`. Phase 5
    /// honours both forms.
    pub resource_value: Option<String>,
    /// Property whitelist for `Mask`. Unused by Phase 5.
    pub properties: Option<Vec<String>>,
    /// Placeholder for `Mask`. Unused by Phase 5.
    pub mask_pattern: Option<String>,
    /// Tie-breaker when more than one policy matches the same
    /// resource. Higher value wins. Sorting is the loader's
    /// responsibility — the rewriter trusts the order.
    pub priority: i32,
}

/// Pre-filtered, priority-sorted policies the rewriter applies.
/// Construction lives outside the rewriter so the loader (ox-api or
/// a test harness) can choose its own data source.
#[derive(Debug, Clone, Default)]
pub struct AclSnapshot {
    pub policies: Vec<AclPolicySpec>,
}

impl AclSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_policies(policies: Vec<AclPolicySpec>) -> Self {
        Self { policies }
    }

    /// `true` when the snapshot has no policies — the rewriter uses
    /// this to short-circuit the entire pass without walking the AST.
    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }
}

// ---------------------------------------------------------------------------
// AclRewriter
// ---------------------------------------------------------------------------

/// Rewriter for ACL deny policies. Stateless — every per-request
/// concern flows through [`RewriteContext::acl_snapshot`].
#[derive(Debug, Clone, Default)]
pub struct AclRewriter;

impl AclRewriter {
    pub const fn new() -> Self {
        Self
    }
}

impl CypherRewriter for AclRewriter {
    fn name(&self) -> &str {
        "acl"
    }

    fn phase(&self) -> RewritePhase {
        RewritePhase::Acl
    }

    fn rewrite(
        &self,
        mut ast: CypherAst,
        ctx: &RewriteContext,
    ) -> Result<RewrittenAst, RewriteError> {
        let snapshot = match &ctx.acl_snapshot {
            Some(s) if !s.is_empty() => Arc::clone(s),
            _ => return Ok(RewrittenAst::passthrough(ast)),
        };

        let denied = collect_deny_resources(&snapshot);
        if denied.node_labels.is_empty()
            && denied.edge_types.is_empty()
            && !denied.wildcard_node
            && !denied.wildcard_edge
        {
            return Ok(RewrittenAst::passthrough(ast));
        }

        let mut modified = 0u32;
        for statement in &mut ast.statements {
            if inject_deny_in_statement(statement, &denied) {
                modified = modified.saturating_add(1);
            }
        }

        Ok(RewrittenAst {
            ast,
            modified_statements: modified,
        })
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DeniedResources {
    /// Specific node labels denied (case-sensitive — Cypher labels
    /// are case-sensitive in Neo4j).
    node_labels: Vec<String>,
    /// Specific edge types denied.
    edge_types: Vec<String>,
    /// `resource_value: None` on a `label` policy — every node label
    /// is denied for the principal.
    wildcard_node: bool,
    /// `resource_value: None` on an `edge_label` policy — every
    /// edge type is denied.
    wildcard_edge: bool,
}

fn collect_deny_resources(snapshot: &AclSnapshot) -> DeniedResources {
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
            _ => {
                // Unknown resource_type — pass through.
            }
        }
    }
    out
}

/// Walk every MATCH / OPTIONAL MATCH clause in `statement`. If the
/// clause's pattern contains a denied node label or edge type,
/// insert `WHERE false AND <prior body>` so the clause produces no
/// rows. Returns true when any clause was modified.
fn inject_deny_in_statement(
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

    // Iterate in reverse so inserting a new WHERE at idx+1 doesn't
    // shift the earlier indices we've already validated.
    for &match_idx in targets.iter().rev() {
        match find_following_where(statement, match_idx) {
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

fn find_following_where(statement: &CypherStatement, match_idx: usize) -> Option<usize> {
    let next = match_idx + 1;
    statement
        .clauses
        .get(next)
        .filter(|c| c.kind == ClauseKind::Where)
        .map(|_| next)
}

/// Rewrite an existing WHERE so it begins with `false AND <body>`.
/// `false AND <…>` is `false` regardless of the body, so the
/// rewriter is free to leave the body verbatim — preserves comments
/// and exotic syntax we haven't taught the parser yet.
fn prepend_false_to_where(clause: &mut CypherClause) {
    let original = std::mem::take(&mut clause.text);
    let trimmed_len = original.len() - original.trim_start().len();
    let (lead_ws, after_ws) = original.split_at(trimmed_len);
    let after_keyword = strip_keyword(after_ws, "WHERE").unwrap_or(after_ws);
    let body = after_keyword.trim_start();
    clause.text = format!("{lead_ws}WHERE false AND {body}");
}

fn strip_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let kw = keyword.as_bytes();
    let bytes = text.as_bytes();
    if bytes.len() < kw.len() {
        return None;
    }
    for (i, k) in kw.iter().enumerate() {
        if !bytes[i].eq_ignore_ascii_case(k) {
            return None;
        }
    }
    let rest = &text[kw.len()..];
    if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace()) {
        Some(rest)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use crate::cypher::parse;
    use crate::cypher::rewrite::{CypherRewriter, RewriteContext};

    use super::*;

    fn snapshot(policies: Vec<AclPolicySpec>) -> Arc<AclSnapshot> {
        Arc::new(AclSnapshot::from_policies(policies))
    }

    fn deny_label(label: &str) -> AclPolicySpec {
        AclPolicySpec {
            action: AclAction::Deny,
            resource_type: "label".to_string(),
            resource_value: Some(label.to_string()),
            properties: None,
            mask_pattern: None,
            priority: 100,
        }
    }

    fn deny_edge(edge: &str) -> AclPolicySpec {
        AclPolicySpec {
            action: AclAction::Deny,
            resource_type: "edge_label".to_string(),
            resource_value: Some(edge.to_string()),
            properties: None,
            mask_pattern: None,
            priority: 100,
        }
    }

    fn rewrite_str(input: &str, ctx: &RewriteContext) -> String {
        let ast = parse(input);
        let out = AclRewriter::new().rewrite(ast, ctx).expect("rewrite ok");
        out.ast.render()
    }

    #[test]
    fn passthrough_when_snapshot_is_empty() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(Vec::new()));
        assert_eq!(
            rewrite_str("MATCH (p:Person) RETURN p", &ctx),
            "MATCH (p:Person) RETURN p"
        );
    }

    #[test]
    fn passthrough_when_snapshot_unset() {
        let ctx = RewriteContext::new("ws");
        assert_eq!(
            rewrite_str("MATCH (p:Person) RETURN p", &ctx),
            "MATCH (p:Person) RETURN p"
        );
    }

    #[test]
    fn deny_label_injects_false_when_no_existing_where() {
        let ctx = RewriteContext::new("ws")
            .with_acl_snapshot(snapshot(vec![deny_label("Person")]));
        let out = rewrite_str("MATCH (p:Person) RETURN p", &ctx);
        assert!(out.contains("WHERE false"), "no WHERE injected: {out}");
    }

    #[test]
    fn deny_label_prepends_false_to_existing_where() {
        let ctx = RewriteContext::new("ws")
            .with_acl_snapshot(snapshot(vec![deny_label("Person")]));
        let out = rewrite_str("MATCH (p:Person) WHERE p.name = 'A' RETURN p", &ctx);
        assert!(
            out.contains("WHERE false AND"),
            "existing WHERE not preserved: {out}"
        );
        assert!(out.contains("p.name = 'A'"));
    }

    #[test]
    fn deny_label_skips_unrelated_match() {
        let ctx = RewriteContext::new("ws")
            .with_acl_snapshot(snapshot(vec![deny_label("Receipt")]));
        let out = rewrite_str("MATCH (p:Person) RETURN p", &ctx);
        assert_eq!(out, "MATCH (p:Person) RETURN p");
    }

    #[test]
    fn deny_only_target_match_is_modified_in_chained_query() {
        // Two separate MATCHes — only the one whose pattern carries
        // a denied label should pick up `WHERE false`. The other
        // stays clean.
        let ctx = RewriteContext::new("ws")
            .with_acl_snapshot(snapshot(vec![deny_label("Receipt")]));
        let out = rewrite_str(
            "MATCH (p:Person) WITH p MATCH (o:Receipt) RETURN p, o",
            &ctx,
        );
        // The Person MATCH stays, the Receipt MATCH gains WHERE false.
        assert!(out.contains("MATCH (p:Person) WITH p"));
        assert!(out.contains("MATCH (o:Receipt) WHERE false"));
    }

    #[test]
    fn deny_edge_label_injects_false() {
        let ctx = RewriteContext::new("ws")
            .with_acl_snapshot(snapshot(vec![deny_edge("PURCHASED")]));
        let out = rewrite_str(
            "MATCH (p:Person)-[:PURCHASED]->(o:Receipt) RETURN p",
            &ctx,
        );
        assert!(out.contains("WHERE false"));
    }

    #[test]
    fn wildcard_label_denies_every_labeled_match() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![
            AclPolicySpec {
                action: AclAction::Deny,
                resource_type: "label".to_string(),
                resource_value: None,
                properties: None,
                mask_pattern: None,
                priority: 100,
            },
        ]));
        let out = rewrite_str("MATCH (p:Person) RETURN p", &ctx);
        assert!(out.contains("WHERE false"));
    }

    #[test]
    fn wildcard_label_does_not_deny_unlabeled_match() {
        // `MATCH (n)` with no label — the wildcard policy is intended
        // to cover labelled access. An unlabelled match is treated as
        // generic graph traversal; if a deployment wants to deny that
        // entirely, a separate policy at the workspace_role level
        // covers it.
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![
            AclPolicySpec {
                action: AclAction::Deny,
                resource_type: "label".to_string(),
                resource_value: None,
                properties: None,
                mask_pattern: None,
                priority: 100,
            },
        ]));
        let out = rewrite_str("MATCH (n) RETURN n", &ctx);
        assert_eq!(out, "MATCH (n) RETURN n");
    }

    #[test]
    fn mask_action_is_not_yet_applied() {
        // Phase 6 wires Mask. Phase 5 must not silently widen
        // mask-shaped policies into Deny.
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![
            AclPolicySpec {
                action: AclAction::Mask,
                resource_type: "label".to_string(),
                resource_value: Some("Person".to_string()),
                properties: Some(vec!["email".to_string()]),
                mask_pattern: Some("***".to_string()),
                priority: 100,
            },
        ]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email", &ctx);
        assert_eq!(out, "MATCH (p:Person) RETURN p.email");
    }

    #[test]
    fn modified_statements_count_matches_touched_statements() {
        let ctx = RewriteContext::new("ws")
            .with_acl_snapshot(snapshot(vec![deny_label("Receipt")]));
        let ast = parse("MATCH (p:Person) RETURN p");
        let out = AclRewriter::new().rewrite(ast, &ctx).unwrap();
        assert_eq!(out.modified_statements, 0);

        let ast = parse("MATCH (o:Receipt) RETURN o");
        let out = AclRewriter::new().rewrite(ast, &ctx).unwrap();
        assert_eq!(out.modified_statements, 1);
    }
}
