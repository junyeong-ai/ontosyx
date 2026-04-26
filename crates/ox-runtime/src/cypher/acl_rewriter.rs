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
//! ## `Mask`
//!
//! `Mask` policies hide individual properties on the projection
//! surface (RETURN / WITH). Three projection shapes are covered:
//!
//! 1. **Direct access** — `<var>.<prop>` (with or without
//!    `AS alias`). The access is replaced with the policy's
//!    `mask_pattern` literal; the `AS alias` survives so downstream
//!    consumers see the original column name with the masked
//!    payload.
//! 2. **Chained access** — `<var>.<prop>.<sub>...`. The whole chain
//!    is replaced when the *first* hop matches a mask policy,
//!    otherwise the chain falls through unchanged. A masked
//!    property cannot leak through nested map indexing.
//! 3. **Bare variable** — `RETURN <var>`. Returning the whole node
//!    would otherwise expose every property including masked ones,
//!    so the rewriter rewrites the bare reference into a Cypher map
//!    projection that overrides every masked property:
//!    `RETURN <var> {.*, masked: '***', ...}`. References inside
//!    function calls (`count(p)`, `id(p)`) are left alone — those
//!    are scalar/aggregate operations the operator owns; aggregating
//!    over masked types should use explicit projection.
//!
//! `Mask` does **not** rewrite WHERE — predicates run against real
//! values so a `WHERE p.email = $email` lookup keeps working.
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
use crate::cypher::rewrite_helpers::{
    find_following_where_clause, split_leading_whitespace, strip_leading_keyword,
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
/// `false AND <…>` is `false` regardless of the body, so the
/// rewriter is free to leave the body verbatim — preserves comments
/// and exotic syntax we haven't taught the parser yet. An empty
/// body collapses to `WHERE false` (no trailing AND) so a
/// follow-on parse round-trip stays clean.
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
/// as a single-quoted literal. Cypher's escape rules (per openCypher
/// 9.0 §2.5.4) mark backslash as the escape character: `\\`, `\'`,
/// `\"`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uXXXX`. Anything else
/// after a `\` is a parse error. So a raw `mask_pattern` containing
/// a literal `\` MUST be doubled or it will produce an unrecognised
/// escape sequence at runtime — the bug the audit follow-up
/// surfaced.
///
/// Order matters: escape `\` first so the `\'` we introduce in the
/// next pass doesn't turn into `\\\'` and re-trigger the rule.
fn quote_pattern_literal(raw: &str) -> String {
    let escaped = raw.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
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

/// Walk a clause's tokens, build a list of edits, apply them in
/// reverse byte-offset order. Returns true iff any edit landed.
fn rewrite_property_accesses(
    clause: &mut CypherClause,
    var_labels: &HashMap<String, Vec<String>>,
    specs: &[MaskSpec],
) -> bool {
    let clause_start = clause.span.start;

    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for chain in find_property_access_chains(&clause.tokens) {
        let labels = match var_labels.get(&chain.var) {
            Some(l) => l,
            None => continue,
        };
        let pattern = match resolve_mask_pattern(labels, &chain.first_prop, specs) {
            Some(p) => p,
            None => continue,
        };
        let local_start = chain.span_start.saturating_sub(clause_start);
        let local_end = chain.span_end.saturating_sub(clause_start);
        if local_end <= clause.text.len() && local_start < local_end {
            replacements.push((local_start, local_end, pattern));
        }
    }

    for bare in find_bare_variable_references(&clause.tokens) {
        let labels = match var_labels.get(&bare.var) {
            Some(l) => l,
            None => continue,
        };
        let masked = collect_masked_properties_for(labels, specs);
        if masked.is_empty() {
            continue;
        }
        let projection = compose_map_projection_override(&bare.var, &masked);
        let local_start = bare.span_start.saturating_sub(clause_start);
        let local_end = bare.span_end.saturating_sub(clause_start);
        if local_end <= clause.text.len() && local_start < local_end {
            replacements.push((local_start, local_end, projection));
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

/// Collect every `(property, mask_pattern)` pair that applies to a
/// variable bound to `labels`. Used by the bare-variable rewrite to
/// build the map projection override.
fn collect_masked_properties_for(
    labels: &[String],
    specs: &[MaskSpec],
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for spec in specs {
        let label_match = match &spec.label {
            Some(l) => labels.iter().any(|v| v == l),
            None => !labels.is_empty(),
        };
        if !label_match {
            continue;
        }
        for prop in &spec.properties {
            if !out.iter().any(|(existing, _)| existing == prop) {
                out.push((prop.clone(), spec.pattern.clone()));
            }
        }
    }
    out
}

fn compose_map_projection_override(var: &str, masked: &[(String, String)]) -> String {
    let entries = masked
        .iter()
        .map(|(prop, pattern)| format!("{prop}: {pattern}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{var} {{.*, {entries}}}")
}

/// One discovered property-access chain: `<var>.<first>(.<rest>)*`.
/// `span_start..span_end` is the absolute byte range covering the
/// entire chain. A chain whose first hop matches a mask policy gets
/// replaced wholesale — masking the head implies masking everything
/// reachable through it.
struct PropertyAccessChain {
    var: String,
    first_prop: String,
    span_start: usize,
    span_end: usize,
}

/// Walk `tokens` and return every `<var>.<prop>(.<sub>)*` chain.
/// The chain starts on a non-property variable identifier (i.e., the
/// previous non-trivia token is NOT a dot), so an inner triplet
/// `b.c` of `a.b.c` is not double-counted.
fn find_property_access_chains(tokens: &[CypherToken]) -> Vec<PropertyAccessChain> {
    let non_trivia: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| (!t.is_trivia()).then_some(i))
        .collect();

    let mut chains = Vec::new();
    let mut i = 0;
    while i + 2 < non_trivia.len() {
        let a = &tokens[non_trivia[i]];
        let b = &tokens[non_trivia[i + 1]];
        let c = &tokens[non_trivia[i + 2]];

        let is_dot = b.kind == TokenKind::Operator && b.text == ".";
        let var_ok = matches!(a.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier);
        let prop_ok = matches!(c.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier);
        if !is_dot || !var_ok || !prop_ok {
            i += 1;
            continue;
        }

        // Reject when `a` is itself the tail of an earlier `.<a>` —
        // that would mean we're standing on the inner triplet of a
        // longer chain.
        if i > 0 {
            let prev = &tokens[non_trivia[i - 1]];
            if prev.kind == TokenKind::Operator && prev.text == "." {
                i += 1;
                continue;
            }
        }

        // Walk forward through `(.<ident>)*` to capture the chain end.
        let mut chain_end_idx = i + 2;
        let mut span_end = c.span.end;
        loop {
            if chain_end_idx + 2 >= non_trivia.len() {
                break;
            }
            let dot = &tokens[non_trivia[chain_end_idx + 1]];
            let ident = &tokens[non_trivia[chain_end_idx + 2]];
            let next_is_dot = dot.kind == TokenKind::Operator && dot.text == ".";
            let next_is_ident =
                matches!(ident.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier);
            if !(next_is_dot && next_is_ident) {
                break;
            }
            chain_end_idx += 2;
            span_end = ident.span.end;
        }

        chains.push(PropertyAccessChain {
            var: strip_backticks(&a.text),
            first_prop: strip_backticks(&c.text),
            span_start: a.span.start,
            span_end,
        });
        i = chain_end_idx + 1;
    }
    chains
}

/// One bare variable reference inside a RETURN / WITH clause —
/// `RETURN p`, `WITH p` (with or without `AS alias`). Function-call
/// arguments (`count(p)`, `id(p)`) are excluded.
struct BareVariableReference {
    var: String,
    span_start: usize,
    span_end: usize,
}

/// Walk `tokens` and return every bare variable reference. A
/// reference is "bare" when the token is an Identifier and:
///
/// * the previous non-trivia token is NOT `.` (would make this a
///   property access),
/// * the next non-trivia token is NOT `(` (would make this a
///   function call),
/// * the next non-trivia token is NOT `.` (would start a chain;
///   chains are handled by [`find_property_access_chains`]),
/// * the previous non-trivia token is NOT `(` (would make this a
///   function-call argument like `count(p)` — those are scalar /
///   aggregate operations the operator owns).
///
/// The keyword tokens (`RETURN`, `WITH`, `AS`, `DISTINCT`) are
/// excluded by the Identifier kind check.
fn find_bare_variable_references(tokens: &[CypherToken]) -> Vec<BareVariableReference> {
    let non_trivia: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| (!t.is_trivia()).then_some(i))
        .collect();

    let mut out = Vec::new();
    for (pos, &tok_idx) in non_trivia.iter().enumerate() {
        let token = &tokens[tok_idx];
        if !matches!(token.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier) {
            continue;
        }
        // Previous non-trivia token must not be `.` or `(`.
        if pos > 0 {
            let prev = &tokens[non_trivia[pos - 1]];
            if prev.kind == TokenKind::Operator && prev.text == "." {
                continue;
            }
            if prev.kind == TokenKind::Paren && prev.text == "(" {
                continue;
            }
        }
        // Next non-trivia token must not be `(` (function call) or
        // `.` (chain head).
        if pos + 1 < non_trivia.len() {
            let next = &tokens[non_trivia[pos + 1]];
            if next.kind == TokenKind::Paren && next.text == "(" {
                continue;
            }
            if next.kind == TokenKind::Operator && next.text == "." {
                continue;
            }
        }
        out.push(BareVariableReference {
            var: strip_backticks(&token.text),
            span_start: token.span.start,
            span_end: token.span.end,
        });
    }
    out
}

fn strip_backticks(s: &str) -> String {
    if s.starts_with('`') && s.ends_with('`') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
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
    fn mask_pattern_with_backslash_is_escaped() {
        // Audit-follow-up regression: prior `quote_pattern_literal`
        // only escaped single quotes. A `mask_pattern` containing a
        // literal `\` produced `'back\slash'` which Cypher rejects
        // as an unrecognised escape sequence. The escape order also
        // matters — `\` must be doubled before `'` is escaped, or
        // the new `\'` would itself be re-escaped.
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["nick"],
            Some("back\\slash"),
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.nick", &ctx);
        assert!(out.contains("'back\\\\slash'"), "got: {out}");
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
    fn mask_chained_access_replaces_whole_chain() {
        // `n.address.city` — when `address` is masked, the whole chain
        // becomes `'***'`. A nested map lookup must not leak through.
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["address"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN p.address.city", &ctx);
        assert!(out.contains("RETURN '***'"), "got: {out}");
        assert!(!out.contains("p.address"), "chain head leaked: {out}");
        assert!(!out.contains(".city"), "chain tail leaked: {out}");
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
        assert!(
            out.contains("p {.*, email: '***'}"),
            "expected map projection override, got: {out}"
        );
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
        // `count(p)`, `id(p)`, `properties(p)` are operator-owned —
        // applying map projection inside would break id/labels and
        // change aggregation semantics. Document by leaving them.
        let ctx = RewriteContext::new("ws").with_acl_snapshot(snapshot(vec![mask_label(
            "Person",
            &["email"],
            None,
        )]));
        let out = rewrite_str("MATCH (p:Person) RETURN count(p)", &ctx);
        assert!(out.contains("count(p)"), "got: {out}");
        assert!(!out.contains("p {"), "unexpected projection: {out}");
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
