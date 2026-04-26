//! ACL row-level rewriter — Phase 5 (deny) + Phase 6 (mask).
//!
//! Policies live on a pre-filtered, principal-scoped [`AclSnapshot`]
//! threaded through [`crate::cypher::rewrite::RewriteContext`].
//!
//! ## Phase 5 — `Deny`
//!
//! Per MATCH / OPTIONAL MATCH inspection: when a node pattern's
//! label matches a `Deny` policy's resource, the rewriter injects a
//! constant `false` predicate so the clause yields no rows. Existing
//! predicates AND-combine with the constant; the existing text
//! doesn't have to be parsed structurally — `false AND <…>` is
//! `false` regardless.
//!
//! ## Phase 6 — `Mask`
//!
//! `Mask` policies hide individual properties on the projection
//! surface (RETURN / WITH). The rewriter scans property-access
//! triplets `<var>.<prop>` in those clauses and, when the
//! variable's bound label matches a mask policy whose `properties`
//! list contains `prop`, replaces the access with the policy's
//! `mask_pattern` literal (default `'***'`). Any trailing `AS alias`
//! is preserved unchanged so downstream consumers see the original
//! column name with the masked payload.
//!
//! `Mask` does **not** rewrite WHERE — predicates stay against the
//! real values so a deny-shaped condition can still execute. The
//! plan calls this out explicitly: WHERE is internal; only what
//! leaves the row through RETURN / WITH gets the mask treatment.
//!
//! Multiple mask policies on the same property: the snapshot is
//! priority-sorted by the loader, so the rewriter takes the first
//! matching policy's `mask_pattern` (priority-desc first match
//! wins, deterministic).

use std::collections::HashMap;
use std::sync::Arc;

use crate::cypher::ast::{
    ClauseKind, CypherAst, CypherClause, CypherPatternElement, CypherStatement,
};
use crate::cypher::rewrite::{
    CypherRewriter, RewriteContext, RewriteError, RewritePhase, RewrittenAst,
};
use crate::cypher::token::{CypherToken, Span, TokenKind};

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
        let mask_specs = collect_mask_specs(&snapshot);
        let no_deny = denied.node_labels.is_empty()
            && denied.edge_types.is_empty()
            && !denied.wildcard_node
            && !denied.wildcard_edge;
        if no_deny && mask_specs.is_empty() {
            return Ok(RewrittenAst::passthrough(ast));
        }

        let mut modified = 0u32;
        for statement in &mut ast.statements {
            let deny_changed = !no_deny && inject_deny_in_statement(statement, &denied);
            let mask_changed = !mask_specs.is_empty()
                && apply_mask_in_statement(statement, &mask_specs);
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
// Mask — Phase 6
// ---------------------------------------------------------------------------

/// Per-policy mask spec collected from the snapshot. `label = None`
/// is the wildcard form (`resource_value: None | "*"`) and applies
/// to every variable's labels in the query.
#[derive(Debug, Clone)]
struct MaskSpec {
    label: Option<String>,
    properties: Vec<String>,
    pattern: String,
}

const DEFAULT_MASK_PATTERN: &str = "'***'";

fn collect_mask_specs(snapshot: &AclSnapshot) -> Vec<MaskSpec> {
    let mut out = Vec::new();
    for policy in &snapshot.policies {
        if policy.action != AclAction::Mask {
            continue;
        }
        if policy.resource_type != "label" {
            continue;
        }
        let properties = match policy.properties.as_ref() {
            Some(p) if !p.is_empty() => p.clone(),
            _ => continue,
        };
        let pattern = policy
            .mask_pattern
            .as_deref()
            .map(quote_pattern_literal)
            .unwrap_or_else(|| DEFAULT_MASK_PATTERN.to_string());
        let label = match policy.resource_value.as_deref() {
            Some(v) if !v.is_empty() && v != "*" => Some(v.to_string()),
            _ => None,
        };
        out.push(MaskSpec {
            label,
            properties,
            pattern,
        });
    }
    out
}

/// The DB stores `mask_pattern` as a free-form string (`***`,
/// `XXXX-XXXX`, etc.) but the rewriter has to splice it into Cypher
/// as a literal. Wrap in single quotes and escape any embedded
/// single quote — the only character a Cypher single-quoted literal
/// needs escaped.
fn quote_pattern_literal(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "\\'"))
}

/// Apply mask rewrites to every RETURN / WITH clause in `statement`.
/// Returns true iff any clause text was modified.
fn apply_mask_in_statement(statement: &mut CypherStatement, specs: &[MaskSpec]) -> bool {
    if specs.is_empty() {
        return false;
    }
    let var_labels = build_var_label_map(statement);
    if var_labels.is_empty() {
        return false;
    }

    let mut any = false;
    for clause in &mut statement.clauses {
        if !matches!(clause.kind, ClauseKind::Return | ClauseKind::With) {
            continue;
        }
        if rewrite_property_accesses(clause, &var_labels, specs) {
            any = true;
        }
    }
    any
}

/// Walk MATCH / OPTIONAL MATCH / CREATE / MERGE clauses and build
/// `variable -> labels` for every bound node. A variable bound in
/// multiple patterns accumulates the union of its labels.
fn build_var_label_map(statement: &CypherStatement) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for clause in &statement.clauses {
        if !matches!(
            clause.kind,
            ClauseKind::Match
                | ClauseKind::OptionalMatch
                | ClauseKind::Create
                | ClauseKind::Merge
        ) {
            continue;
        }
        for pattern in &clause.patterns {
            for element in &pattern.elements {
                if let CypherPatternElement::Node(node) = element
                    && let Some(var) = &node.variable
                {
                    let entry = map.entry(var.clone()).or_default();
                    for label in &node.labels {
                        if !entry.contains(label) {
                            entry.push(label.clone());
                        }
                    }
                }
            }
        }
    }
    map
}

/// Find Identifier-`.`-Identifier triplets in `clause.tokens`. For
/// every triplet whose `<var>.<prop>` matches a mask policy, replace
/// the corresponding byte range in `clause.text` with the policy's
/// pattern literal. Returns true iff anything was rewritten.
///
/// Replacements are applied in reverse byte-offset order so each
/// edit's offset stays valid after the previous edit landed.
fn rewrite_property_accesses(
    clause: &mut CypherClause,
    var_labels: &HashMap<String, Vec<String>>,
    specs: &[MaskSpec],
) -> bool {
    // Token spans are absolute against the original source. The
    // clause's text is a slice of the same source starting at
    // `clause.span.start`, so local offset = absolute - clause start.
    let clause_start = clause.span.start;

    let triplets = find_property_access_triplets(&clause.tokens);
    if triplets.is_empty() {
        return false;
    }

    // Collect replacements (byte_start, byte_end_exclusive,
    // replacement_text) sorted descending by start offset.
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for (var, prop, span_start_abs, span_end_abs) in triplets {
        let labels = match var_labels.get(&var) {
            Some(l) => l,
            None => continue,
        };
        let pattern = match resolve_mask_pattern(labels, &prop, specs) {
            Some(p) => p,
            None => continue,
        };
        let local_start = span_start_abs.saturating_sub(clause_start);
        let local_end = span_end_abs.saturating_sub(clause_start);
        if local_end <= clause.text.len() && local_start < local_end {
            replacements.push((local_start, local_end, pattern));
        }
    }
    if replacements.is_empty() {
        return false;
    }
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut text = std::mem::take(&mut clause.text);
    for (start, end, value) in replacements {
        text.replace_range(start..end, &value);
    }
    clause.text = text;
    true
}

/// Returns `Some(pattern)` when at least one mask spec matches
/// `prop` for any of the variable's `labels`. Specs are scanned in
/// snapshot order — which the loader sorted priority-desc — so
/// "first match wins" maps naturally to highest-priority wins.
fn resolve_mask_pattern(labels: &[String], prop: &str, specs: &[MaskSpec]) -> Option<String> {
    for spec in specs {
        let label_match = match &spec.label {
            Some(l) => labels.iter().any(|v| v == l),
            None => !labels.is_empty(),
        };
        if !label_match {
            continue;
        }
        if spec.properties.iter().any(|p| p == prop) {
            return Some(spec.pattern.clone());
        }
    }
    None
}

/// Walk `tokens` for non-trivia sequences `Identifier`-`Operator(.)`-`Identifier`.
/// Returns `(var, prop, byte_start, byte_end)` where the byte range
/// covers the entire `var.prop` span (inclusive of var, the dot, and
/// the prop). Spans are absolute (the same coordinate space the
/// clause's text lives in via `clause.span.start`).
fn find_property_access_triplets(
    tokens: &[CypherToken],
) -> Vec<(String, String, usize, usize)> {
    let mut idx_of_non_trivia: Vec<usize> = Vec::with_capacity(tokens.len());
    for (i, t) in tokens.iter().enumerate() {
        if !t.is_trivia() {
            idx_of_non_trivia.push(i);
        }
    }
    let mut out = Vec::new();
    for window in idx_of_non_trivia.windows(3) {
        let a = &tokens[window[0]];
        let b = &tokens[window[1]];
        let c = &tokens[window[2]];
        let is_dot = b.kind == TokenKind::Operator && b.text == ".";
        let var_ok =
            a.kind == TokenKind::Identifier || a.kind == TokenKind::QuotedIdentifier;
        let prop_ok =
            c.kind == TokenKind::Identifier || c.kind == TokenKind::QuotedIdentifier;
        if !is_dot || !var_ok || !prop_ok {
            continue;
        }
        // Skip cases where the property access is part of a deeper
        // chain (`a.b.c`) — we only mask the *first* level. A second
        // access `b.c` would otherwise be misclassified as a top-
        // level `b.c` access. Detect by checking the token after `c`
        // — if it's another `.`, this triplet is a prefix of a
        // longer access, so the rewriter should leave it alone.
        let next_idx = idx_of_non_trivia
            .iter()
            .position(|&i| i == window[2])
            .map(|p| p + 1)
            .and_then(|p| idx_of_non_trivia.get(p))
            .copied();
        if let Some(next) = next_idx {
            let nt = &tokens[next];
            if nt.kind == TokenKind::Operator && nt.text == "." {
                continue;
            }
        }
        let var = strip_backticks(&a.text);
        let prop = strip_backticks(&c.text);
        out.push((var, prop, a.span.start, c.span.end));
    }
    out
}

fn strip_backticks(s: &str) -> String {
    let mut out = s.to_string();
    if out.starts_with('`') && out.ends_with('`') && out.len() >= 2 {
        out = out[1..out.len() - 1].to_string();
    }
    out
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
    fn mask_action_does_not_widen_into_deny() {
        // Cross-action regression: a `Mask` policy must NOT inject
        // a `WHERE false` predicate. Mask only rewrites projection
        // surfaces — the underlying MATCH still has to run for the
        // surrounding query (joins, aggregates) to see real rows.
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
        assert!(!out.contains("WHERE false"), "mask widened into deny: {out}");
        assert!(out.contains("'***'"), "mask did not apply: {out}");
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

    #[test]
    fn mask_replaces_property_in_return_with_default_pattern() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.email", &ctx);
        assert!(
            out.contains("RETURN '***'") || out.contains("RETURN '***' "),
            "expected mask pattern in RETURN, got: {out}"
        );
        assert!(!out.contains("p.email"), "raw access leaked: {out}");
    }

    #[test]
    fn mask_uses_policy_pattern_when_provided() {
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["ssn"],
            Some("XXX-XX-XXXX"),
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.ssn", &ctx);
        assert!(out.contains("'XXX-XX-XXXX'"), "got: {out}");
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
        assert!(out.contains(" AS e"), "alias dropped: {out}");
    }

    #[test]
    fn mask_skips_non_target_properties() {
        // Mask covers only `email`. `name` access stays untouched.
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
        // Plan-stated boundary: WHERE is internal, only RETURN /
        // WITH are projection surfaces and get masked.
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str(
            "MATCH (p:Person) WHERE p.email = $e RETURN p.name",
            &ctx,
        );
        assert!(out.contains("WHERE p.email = $e"), "WHERE got rewritten: {out}");
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
        // Loader sorts priority-desc, so the first matching policy
        // in the snapshot is the highest-priority. Pin the
        // determinism here.
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
        assert!(out.contains("'HIGH'"), "got: {out}");
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
        assert!(out.contains("'o\\'malley'"), "got: {out}");
    }

    #[test]
    fn mask_does_not_overwrite_string_literal_matching_var_dot_prop() {
        // The token-level walk classifies StringLiteral as one
        // token, so `'p.email'` won't be mistaken for an access.
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN 'p.email'", &ctx);
        assert!(out.contains("'p.email'"), "string literal got rewritten: {out}");
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
