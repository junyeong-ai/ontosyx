//! ACL row-level rewriter — Deny + Mask.
//!
//! Policies live on a pre-filtered, principal-scoped [`AclSnapshot`]
//! threaded through [`crate::cypher::rewrite::RewriteContext`].
//!
//! ## `Deny`
//!
//! Per MATCH / OPTIONAL MATCH inspection: when a node pattern's
//! label or relationship type matches a `Deny` policy's resource,
//! the rewriter injects a constant `false` predicate so the clause
//! yields no rows. Existing predicates AND-combine with the
//! constant; the existing text doesn't have to be parsed
//! structurally — `false AND <…>` is `false` regardless.
//!
//! ## `Mask`
//!
//! `Mask` policies hide individual properties on the projection
//! surface (RETURN / WITH). Three projection shapes are covered:
//!
//! 1. **Direct access** — `<var>.<prop>` (with or without
//!    `AS alias`). The access is replaced with the policy's
//!    `mask_pattern` literal; the `AS alias` survives.
//! 2. **Chained access** — `<var>.<prop>.<sub>...`. The whole chain
//!    is replaced when the *first* hop matches a mask policy.
//! 3. **Bare variable** — `RETURN <var>`. Returning the whole node
//!    would otherwise expose every property including masked ones,
//!    so the rewriter rewrites the bare reference into a Cypher map
//!    projection that overrides every masked property:
//!    `RETURN <var> {.*, masked: '***', ...}`. References inside
//!    function calls (`count(p)`, `id(p)`) are left alone.
//!
//! `Mask` does **not** rewrite WHERE — predicates run against real
//! values so a `WHERE p.email = $email` lookup keeps working.
//!
//! Multiple mask policies on the same property: the snapshot is
//! priority-sorted by the loader, so the first matching policy
//! wins (priority-desc first match wins, deterministic).

mod deny;
mod mask;
mod snapshot;

use std::sync::Arc;

use crate::cypher::ast::CypherAst;
use crate::cypher::rewrite::{
    CypherRewriter, RewriteContext, RewriteError, RewritePhase, RewrittenAst,
};

pub use snapshot::{AclAction, AclPolicySpec, AclSnapshot};

/// Rewriter for ACL Deny + Mask. Stateless — every per-request
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

        let denied = deny::collect_deny_resources(&snapshot);
        let mask_specs = mask::collect_mask_specs(&snapshot);
        let no_deny = denied.is_empty();
        if no_deny && mask_specs.is_empty() {
            return Ok(RewrittenAst::passthrough(ast));
        }

        let mut modified = 0u32;
        for statement in &mut ast.statements {
            let deny_changed =
                !no_deny && deny::inject_deny_in_statement(statement, &denied);
            let mask_changed = !mask_specs.is_empty()
                && mask::apply_mask_in_statement(statement, &mask_specs);
            if deny_changed || mask_changed {
                modified = modified.saturating_add(1);
            }
        }

        Ok(RewrittenAst {
            ast,
            modified_statements: modified,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use crate::cypher::parse;
    use crate::cypher::rewrite::{CypherRewriter, RewriteContext};

    use super::*;

    fn snapshot(policies: Vec<AclPolicySpec>) -> Arc<AclSnapshot> {
        Arc::new(AclSnapshot { policies })
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

    fn mask_label(label: &str, properties: &[&str], pattern: Option<&str>) -> AclPolicySpec {
        AclPolicySpec {
            action: AclAction::Mask,
            resource_type: "label".to_string(),
            resource_value: Some(label.to_string()),
            properties: Some(properties.iter().map(|s| s.to_string()).collect()),
            mask_pattern: pattern.map(|s| s.to_string()),
            priority: 100,
        }
    }

    fn rewrite_str(input: &str, ctx: &RewriteContext) -> String {
        let ast = parse(input);
        let out = AclRewriter::new().rewrite(ast, ctx).expect("rewrite ok");
        out.ast.render()
    }

    // -------- Passthrough --------

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

    // -------- Deny --------

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
        assert!(out.contains("WHERE false AND"), "{out}");
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
        let ctx = RewriteContext::new("ws")
            .with_acl_snapshot(snapshot(vec![deny_label("Receipt")]));
        let out = rewrite_str(
            "MATCH (p:Person) WITH p MATCH (o:Receipt) RETURN p, o",
            &ctx,
        );
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

    // -------- Mask --------

    #[test]
    fn mask_action_does_not_widen_into_deny() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            Some("***"),
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email", &ctx);
        assert!(!out.contains("WHERE false"));
        assert!(out.contains("'***'"));
    }

    #[test]
    fn mask_replaces_property_in_return_with_default_pattern() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email", &ctx);
        assert!(out.contains("RETURN '***'"), "{out}");
        assert!(!out.contains("p.email"));
    }

    #[test]
    fn mask_uses_policy_pattern_when_provided() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["ssn"],
            Some("XXX-XX-XXXX"),
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.ssn", &ctx);
        assert!(out.contains("'XXX-XX-XXXX'"));
    }

    #[test]
    fn mask_preserves_alias_clause() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email AS e", &ctx);
        assert!(out.contains("'***'"));
        assert!(out.contains(" AS e"));
    }

    #[test]
    fn mask_skips_non_target_properties() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.name, p.email", &ctx);
        assert!(out.contains("p.name"));
        assert!(!out.contains("p.email"));
        assert!(out.contains("'***'"));
    }

    #[test]
    fn mask_does_not_rewrite_where_clauses() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str(
            "MATCH (p:Person) WHERE p.email = $e RETURN p.name",
            &ctx,
        );
        assert!(out.contains("WHERE p.email = $e"));
    }

    #[test]
    fn mask_rewrites_with_clause_too() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str(
            "MATCH (p:Person) WITH p.email AS contact RETURN contact",
            &ctx,
        );
        assert!(out.contains("'***' AS contact"));
        assert!(!out.contains("p.email"));
    }

    #[test]
    fn mask_skips_unrelated_label() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Customer",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email", &ctx);
        assert_eq!(out, "MATCH (p:Person) RETURN p.email");
    }

    #[test]
    fn mask_first_match_wins_on_priority_desc() {
        let high = AclPolicySpec {
            action: AclAction::Mask,
            resource_type: "label".to_string(),
            resource_value: Some("Person".to_string()),
            properties: Some(vec!["email".to_string()]),
            mask_pattern: Some("HIGH".to_string()),
            priority: 200,
        };
        let low = AclPolicySpec {
            action: AclAction::Mask,
            resource_type: "label".to_string(),
            resource_value: Some("Person".to_string()),
            properties: Some(vec!["email".to_string()]),
            mask_pattern: Some("LOW".to_string()),
            priority: 100,
        };
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![high, low]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email", &ctx);
        assert!(out.contains("'HIGH'"));
        assert!(!out.contains("'LOW'"));
    }

    #[test]
    fn mask_wildcard_label_applies_to_any_labelled_var() {
        let wildcard = AclPolicySpec {
            action: AclAction::Mask,
            resource_type: "label".to_string(),
            resource_value: None,
            properties: Some(vec!["email".to_string()]),
            mask_pattern: None,
            priority: 100,
        };
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![wildcard]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email", &ctx);
        assert!(out.contains("'***'"));
    }

    #[test]
    fn mask_pattern_with_single_quote_is_escaped() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["nick"],
            Some("o'malley"),
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.nick", &ctx);
        assert!(out.contains("'o\\'malley'"));
    }

    #[test]
    fn mask_pattern_with_backslash_is_escaped() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["nick"],
            Some("back\\slash"),
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.nick", &ctx);
        assert!(out.contains("'back\\\\slash'"));
    }

    #[test]
    fn mask_does_not_overwrite_string_literal_matching_var_dot_prop() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN 'p.email'", &ctx);
        assert!(out.contains("'p.email'"));
    }

    #[test]
    fn mask_chained_access_replaces_whole_chain() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["address"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.address.city", &ctx);
        assert!(out.contains("RETURN '***'"));
        assert!(!out.contains("p.address"));
        assert!(!out.contains(".city"));
    }

    #[test]
    fn mask_chained_access_left_alone_when_first_hop_unmasked() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.metadata.last_login", &ctx);
        assert!(out.contains("p.metadata.last_login"));
    }

    #[test]
    fn mask_bare_variable_rewrites_to_map_projection() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p", &ctx);
        assert!(out.contains("p {.*, email: '***'}"));
    }

    #[test]
    fn mask_bare_variable_preserves_alias() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p AS person", &ctx);
        assert!(out.contains("p {.*, email: '***'} AS person"));
    }

    #[test]
    fn mask_bare_variable_includes_every_masked_property() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email", "ssn"],
            Some("XXX"),
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p", &ctx);
        assert!(out.contains("email: 'XXX'"));
        assert!(out.contains("ssn: 'XXX'"));
    }

    #[test]
    fn mask_bare_variable_skipped_when_no_property_is_masked() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Customer",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p", &ctx);
        assert_eq!(out, "MATCH (p:Person) RETURN p");
    }

    #[test]
    fn mask_function_call_argument_is_not_rewritten() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN count(p)", &ctx);
        assert!(out.contains("count(p)"));
        assert!(!out.contains("p {"));
    }

    #[test]
    fn mask_with_clause_rewrites_bare_var() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) WITH p RETURN p.name", &ctx);
        assert!(out.contains("WITH p {.*, email: '***'}"));
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
