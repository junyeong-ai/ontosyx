//! Soft-delete rewriter — Phase 7.
//!
//! Treats `_deleted_at: timestamp?` as the system-owned tombstone
//! property on every node and relationship. The rewriter has two
//! responsibilities:
//!
//! 1. **Read path.** Inject `<var>._deleted_at IS NULL` on every
//!    bound node variable in MATCH / OPTIONAL MATCH so soft-deleted
//!    rows are invisible to ordinary queries. Existing predicates
//!    AND-combine with the tombstone check.
//! 2. **Write path.** Rewrite `DELETE n` / `DETACH DELETE n` into
//!    `SET n._deleted_at = timestamp()`. The mutation surface is
//!    a single SET clause replacing the destructive one — the
//!    audit trail keeps the row, retention will hard-delete on a
//!    separate cron after the configured TTL.
//!
//! The pass is gated by [`RewriteContext::skip_soft_delete`] —
//! admin paths that intentionally need to see or hard-purge
//! tombstoned rows (the retention compaction job, support tooling,
//! an explicit "un-delete" action) flip the bypass on. Default for
//! every user-facing path is `false`.
//!
//! ## Phase-7 scoping notes
//!
//! - Edges generated `_deleted_at` injection on relationship variables
//!   is currently out of scope: `MATCH (a)-[r]->(b) WHERE
//!   r._deleted_at IS NULL` is the right shape, but pinning down the
//!   semantics of "edge soft delete" (does deleting an edge also
//!   tombstone the endpoints? what about COUNT(r) over tombstones?)
//!   needs more design. Phase 7 covers nodes only; deferred work
//!   is captured in `plan_roadmap_execution_2026_04_26.md`.
//! - `DETACH DELETE` retains its current rewrite — soft-delete the
//!   node, leave attached edges. The plan calls out edge hard-detach
//!   as a "judgment call"; promoting it requires AST clause-insertion
//!   (`OPTIONAL MATCH (n)-[r]-() DELETE r SET n._deleted_at = …`)
//!   which the partial AST does not yet expose ergonomically.
//!   Recorded in the roadmap's Deferred section.

use std::collections::{BTreeMap, HashSet};

use crate::cypher::ast::{
    ClauseKind, CypherAst, CypherClause, CypherPatternElement, CypherStatement, NodePattern,
};
use crate::cypher::rewrite::{
    CypherRewriter, RewriteContext, RewriteError, RewritePhase, RewrittenAst,
};
use crate::cypher::rewrite_helpers::{
    find_following_where_clause, leading_whitespace, split_leading_whitespace,
    strip_leading_keyword,
};
use crate::cypher::token::Span;

/// System-owned tombstone property. Kept as a string constant so
/// every rewriter / validator referring to it shares one source of
/// truth.
pub const TOMBSTONE_PROPERTY: &str = "_deleted_at";

/// Rewriter for soft-delete read filtering and DELETE→SET mutation.
/// Stateless — every per-request concern (skip toggle) flows
/// through [`RewriteContext::skip_soft_delete`].
#[derive(Debug, Clone, Default)]
pub struct SoftDeleteRewriter;

impl SoftDeleteRewriter {
    pub const fn new() -> Self {
        Self
    }
}

impl CypherRewriter for SoftDeleteRewriter {
    fn name(&self) -> &str {
        "soft-delete"
    }

    fn phase(&self) -> RewritePhase {
        RewritePhase::SoftDelete
    }

    fn rewrite(
        &self,
        mut ast: CypherAst,
        ctx: &RewriteContext,
    ) -> Result<RewrittenAst, RewriteError> {
        if ctx.skip_soft_delete {
            return Ok(RewrittenAst::passthrough(ast));
        }
        let mut modified = 0u32;
        for statement in &mut ast.statements {
            // Order matters: rewrite DELETE → SET before injecting
            // the read-side tombstone predicate, so the predicate
            // doesn't end up bound to a clause we're about to
            // replace.
            let write_changed = rewrite_delete_clauses(statement);
            let read_changed = inject_tombstone_predicate(statement);
            if write_changed || read_changed {
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
// Read path
// ---------------------------------------------------------------------------

/// Inject `<var>._deleted_at IS NULL` on every bound node variable
/// in every MATCH / OPTIONAL MATCH clause of `statement`. Mirrors
/// [`crate::cypher::rewrite::WorkspaceScopeRewriter::inject_read_scope`]
/// in shape (per-clause WHERE injection, idempotent across passes)
/// but uses an `IS NULL` predicate instead of a `=` parameter
/// binding.
fn inject_tombstone_predicate(statement: &mut CypherStatement) -> bool {
    let mut plan: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut queued: HashSet<String> = HashSet::new();

    for (idx, clause) in statement.clauses.iter().enumerate() {
        if !matches!(clause.kind, ClauseKind::Match | ClauseKind::OptionalMatch) {
            continue;
        }
        for var in bound_node_variables(clause) {
            if queued.contains(&var) {
                continue;
            }
            if statement_already_has_tombstone_check(statement, &var) {
                queued.insert(var);
                continue;
            }
            queued.insert(var.clone());
            plan.entry(idx).or_default().push(var);
        }
    }
    if plan.is_empty() {
        return false;
    }
    // Iterate in reverse so a freshly-inserted WHERE doesn't shift
    // earlier indices we still need to address.
    for (clause_idx, vars) in plan.iter().rev() {
        let combined = vars
            .iter()
            .map(|v| format!("{v}.{TOMBSTONE_PROPERTY} IS NULL"))
            .collect::<Vec<_>>()
            .join(" AND ");
        match find_following_where_clause(statement, *clause_idx) {
            Some(where_idx) => {
                prepend_to_where(&mut statement.clauses[where_idx], &combined);
            }
            None => {
                statement.clauses.insert(
                    clause_idx + 1,
                    CypherClause {
                        kind: ClauseKind::Where,
                        tokens: Vec::new(),
                        text: format!(" WHERE {combined}"),
                        span: Span::default(),
                        patterns: Vec::new(),
                    },
                );
            }
        }
    }
    true
}

fn bound_node_variables(clause: &CypherClause) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for pattern in &clause.patterns {
        for element in &pattern.elements {
            if let CypherPatternElement::Node(NodePattern {
                variable: Some(v), ..
            }) = element
                && !out.iter().any(|existing| existing == v)
            {
                out.push(v.clone());
            }
        }
    }
    out
}

/// Idempotency probe — does some clause already carry a tombstone
/// check on `var`?
///
/// Substring (not token) match: the rewriter mutates `clause.text`
/// but not `clause.tokens` after an injection, so a token-level
/// walk on the second pipeline pass would not see the predicate it
/// just injected on the first pass. The substring needle is the
/// specific `<var>._deleted_at` access pattern — we accept that an
/// authored string literal containing the same characters would
/// also satisfy the guard, since shadowing user input that looks
/// exactly like the injected shape is a degenerate input we'd
/// rather skip-inject than double-inject.
fn statement_already_has_tombstone_check(statement: &CypherStatement, var: &str) -> bool {
    let needle = format!("{var}.{TOMBSTONE_PROPERTY}");
    statement.clauses.iter().any(|c| c.text.contains(&needle))
}

fn prepend_to_where(clause: &mut CypherClause, condition: &str) {
    let original = std::mem::take(&mut clause.text);
    let (lead_ws, after_ws) = split_leading_whitespace(&original);
    let after_keyword = strip_leading_keyword(after_ws, "WHERE").unwrap_or(after_ws);
    let body = after_keyword.trim();
    // Empty body collapses to plain `WHERE <condition>` — we never
    // want a trailing dangling `AND` even though the parser's input
    // shouldn't produce it.
    clause.text = if body.is_empty() {
        format!("{lead_ws}WHERE {condition}")
    } else {
        format!("{lead_ws}WHERE {condition} AND {body}")
    };
}

// ---------------------------------------------------------------------------
// Write path
// ---------------------------------------------------------------------------

/// Rewrite every `DELETE` and `DETACH DELETE` clause into a
/// `SET <vars>._deleted_at = timestamp()` clause. Returns true iff
/// any clause was rewritten.
fn rewrite_delete_clauses(statement: &mut CypherStatement) -> bool {
    let mut any = false;
    for clause in &mut statement.clauses {
        let is_delete = matches!(
            clause.kind,
            ClauseKind::Delete | ClauseKind::DetachDelete
        );
        if !is_delete {
            continue;
        }
        let vars = parse_delete_targets(&clause.text);
        if vars.is_empty() {
            continue;
        }
        let assignments = vars
            .iter()
            .map(|v| format!("{v}.{TOMBSTONE_PROPERTY} = timestamp()"))
            .collect::<Vec<_>>()
            .join(", ");
        let original_lead = leading_whitespace(&clause.text);
        clause.kind = ClauseKind::Set;
        clause.text = format!("{original_lead}SET {assignments}");
        // The structural patterns slot is empty for these kinds; no
        // pattern to clear. Tokens are preserved for diagnostics —
        // anyone re-tokenising should consult `text`.
        any = true;
    }
    any
}

/// Extract the comma-separated variable list from a `DELETE` or
/// `DETACH DELETE` clause. The text starts with whitespace +
/// keyword; everything after the keyword (up to the end of the
/// clause text) is the target list. We accept simple identifiers
/// and quoted-identifier backtick forms; anything more complex
/// (function calls, list literals) keeps the clause as-is by
/// returning an empty vec.
fn parse_delete_targets(text: &str) -> Vec<String> {
    let trimmed = text.trim_start();
    let body = if let Some(rest) = strip_leading_keyword(trimmed, "DETACH") {
        match strip_leading_keyword(rest.trim_start(), "DELETE") {
            Some(b) => b,
            None => return Vec::new(),
        }
    } else if let Some(rest) = strip_leading_keyword(trimmed, "DELETE") {
        rest
    } else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for raw in body.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            return Vec::new();
        }
        if !is_identifier_like(token) {
            return Vec::new();
        }
        out.push(strip_backticks(token));
    }
    out
}

fn is_identifier_like(s: &str) -> bool {
    if s.starts_with('`') && s.ends_with('`') && s.len() >= 2 {
        return true;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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
    use crate::cypher::parse;
    use crate::cypher::rewrite::{CypherRewriter, RewriteContext};

    use super::*;

    fn rewrite(input: &str) -> String {
        let ctx = RewriteContext::new("ws");
        SoftDeleteRewriter::new()
            .rewrite(parse(input), &ctx)
            .expect("rewrite ok")
            .ast
            .render()
    }

    fn rewrite_with_skip(input: &str) -> String {
        let ctx = RewriteContext::new("ws").with_skip_soft_delete(true);
        SoftDeleteRewriter::new()
            .rewrite(parse(input), &ctx)
            .expect("rewrite ok")
            .ast
            .render()
    }

    // ---------------- Read path ----------------

    #[test]
    fn match_gains_is_null_when_no_existing_where() {
        let out = rewrite("MATCH (p:Person) RETURN p");
        assert!(
            out.contains("WHERE p._deleted_at IS NULL"),
            "got: {out}"
        );
    }

    #[test]
    fn existing_where_keeps_body_and_gets_is_null_prepended() {
        let out = rewrite("MATCH (p:Person) WHERE p.name = 'A' RETURN p");
        assert!(out.contains("WHERE p._deleted_at IS NULL AND"), "got: {out}");
        assert!(out.contains("p.name = 'A'"));
    }

    #[test]
    fn multi_variable_match_covers_every_bound_var() {
        let out = rewrite("MATCH (a:A)-[:R]->(b:B) RETURN a, b");
        assert!(out.contains("a._deleted_at IS NULL"));
        assert!(out.contains("b._deleted_at IS NULL"));
    }

    #[test]
    fn separate_matches_each_get_their_own_clause_inject() {
        // Two MATCHes — each gets its own WHERE for its own bound var.
        let out = rewrite("MATCH (a:A) WITH a MATCH (b:B) RETURN a, b");
        assert!(out.contains("a._deleted_at IS NULL"));
        assert!(out.contains("b._deleted_at IS NULL"));
    }

    #[test]
    fn skip_flag_disables_pass_entirely() {
        let out = rewrite_with_skip("MATCH (p:Person) RETURN p");
        assert_eq!(out, "MATCH (p:Person) RETURN p");
    }

    #[test]
    fn passthrough_when_match_has_no_bound_node_variable() {
        // `MATCH (:Person)` binds nothing — nothing to scope.
        let out = rewrite("MATCH (:Person) RETURN 1");
        assert_eq!(out, "MATCH (:Person) RETURN 1");
    }

    #[test]
    fn idempotent_when_user_authored_a_compatible_check() {
        // Author already wrote `p._deleted_at IS NULL` — no double
        // inject. (Rewriter pipeline running twice on the same AST
        // should also collapse onto the same output.)
        let input = "MATCH (p:Person) WHERE p._deleted_at IS NULL RETURN p";
        let out = rewrite(input);
        assert_eq!(out, input);
        // Run again — still no double inject.
        let again = rewrite(&out);
        assert_eq!(again, input);
    }

    // ---------------- Write path ----------------

    #[test]
    fn delete_rewrites_to_set_tombstone() {
        let out = rewrite("MATCH (p:Person {id: 1}) DELETE p");
        // Delete clause itself becomes SET …
        assert!(out.contains("SET p._deleted_at = timestamp()"), "got: {out}");
        assert!(!out.contains("DELETE p"), "destructive DELETE leaked: {out}");
    }

    #[test]
    fn detach_delete_rewrites_to_set_tombstone() {
        // Phase 7 ships node-soft-delete only; edge handling is a
        // documented follow-up. The rewrite shape stays consistent
        // with the plain DELETE form so the runtime doesn't fork on
        // the variant.
        let out = rewrite("MATCH (p:Person) DETACH DELETE p");
        assert!(out.contains("SET p._deleted_at = timestamp()"), "got: {out}");
    }

    #[test]
    fn delete_multiple_targets_each_get_set_assignment() {
        let out = rewrite("MATCH (a:A), (b:B) DELETE a, b");
        assert!(out.contains("a._deleted_at = timestamp()"));
        assert!(out.contains("b._deleted_at = timestamp()"));
    }

    #[test]
    fn write_path_skipped_when_skip_soft_delete_set() {
        let out = rewrite_with_skip("MATCH (p:Person) DELETE p");
        assert!(out.contains("DELETE p"));
    }

    #[test]
    fn complex_delete_target_falls_through_unrewritten() {
        // `DELETE n.email` (a property — semantically wrong, but
        // syntactically tolerated by Cypher) doesn't fit the
        // identifier-list shape; the rewriter declines to touch it
        // and lets a downstream validator reject it on its own.
        let input = "MATCH (n) DELETE n.email";
        let out = rewrite(input);
        // We did inject the tombstone IS NULL on the read side, so
        // the comparison is on the write clause alone.
        assert!(out.contains("DELETE n.email"));
    }

    #[test]
    fn modified_count_zero_when_passthrough() {
        let ctx = RewriteContext::new("ws").with_skip_soft_delete(true);
        let out = SoftDeleteRewriter::new()
            .rewrite(parse("MATCH (p:Person) RETURN p"), &ctx)
            .unwrap();
        assert_eq!(out.modified_statements, 0);
    }

    #[test]
    fn modified_count_one_for_simple_match() {
        let ctx = RewriteContext::new("ws");
        let out = SoftDeleteRewriter::new()
            .rewrite(parse("MATCH (p:Person) RETURN p"), &ctx)
            .unwrap();
        assert_eq!(out.modified_statements, 1);
    }
}
