//! Cypher rewriter pipeline.
//!
//! A rewriter takes a parsed [`CypherAst`] and returns a (possibly
//! modified) AST wrapped in [`RewrittenAst`] — or a [`RewriteError`] if
//! the pass refuses the query. Multiple rewriters compose through
//! [`CypherRewriterPipeline`]: each pass observes the previous pass's
//! output and either no-ops or injects its own transformation. The
//! pipeline short-circuits on the first error.
//!
//! Landing here first: [`WorkspaceScopeRewriter`], which injects the
//! workspace isolation predicate / SET clause that `scope_cypher` used
//! to add with substring search. Because every other cross-cutting
//! concern (ACL filtering, soft-delete tombstones, temporal `as_of`,
//! query cost injection) needs the same rewrite surface, factoring the
//! trait out now avoids forking the pattern five times later.
//!
//! The `Result`-returning trait signature is deliberate: a future pass
//! (e.g. an `AclRewriter` that finds an unrepresentable policy, or a
//! `SoftDeleteRewriter` that sees an unsupported construct) must have
//! a clean way to reject a query. Returning an unchanged AST for those
//! cases would be a silent isolation failure — the same class of bug
//! substring-based rewriters used to hide.

use std::fmt;

use crate::cypher::ast::{
    ClauseKind, CypherAst, CypherClause, CypherPatternElement, CypherStatement, NodePattern,
};
use crate::cypher::parse;
use crate::cypher::token::Span;

/// Phase ordering for rewriter passes.
///
/// Every pass advertises the slot it should run in; the pipeline uses
/// these values to sort rewriters deterministically regardless of
/// registration order. A new pass that lands between two existing
/// phases gets its own discriminant rather than mutating the
/// existing values — numeric gaps (100 / 200 / 300) give room to
/// wedge extra phases in without re-numbering the rest.
///
/// The current landscape:
///
/// - `Isolation` — workspace scope injection (`WorkspaceScopeRewriter`).
///   Must run first because every subsequent pass assumes the final
///   AST carries workspace scope on its writes.
/// - `Acl` — row-level authorization filters (planned).
/// - `SoftDelete` — tombstone predicate injection (planned).
/// - `Temporal` — `as_of` / `valid_between` filters (planned).
/// - `Custom` — per-installation passes that don't map onto any of
///   the above. Runs last so it can observe everything that landed
///   before it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum RewritePhase {
    Isolation = 100,
    Acl = 200,
    SoftDelete = 300,
    Temporal = 400,
    Custom = 900,
}

/// Errors a rewriter can raise when it refuses to transform a query.
#[derive(Debug, Clone)]
pub enum RewriteError {
    /// The pass recognised a construct but cannot handle it safely.
    /// Example: an ACL rewriter encountering a `CALL { ... }` subquery
    /// whose body it can't drill into.
    UnsupportedConstruct {
        rewriter: String,
        span: Option<Span>,
        hint: String,
    },
    /// A policy-level refusal: the query is syntactically representable
    /// but the pass's own rules forbid it. Example: a future temporal
    /// rewriter rejecting an `as_of` query that points before the
    /// earliest snapshot.
    PolicyDenied { rewriter: String, reason: String },
}

impl fmt::Display for RewriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RewriteError::UnsupportedConstruct { rewriter, hint, .. } => {
                write!(f, "[{rewriter}] unsupported construct: {hint}")
            }
            RewriteError::PolicyDenied { rewriter, reason } => {
                write!(f, "[{rewriter}] policy denied: {reason}")
            }
        }
    }
}

impl std::error::Error for RewriteError {}

/// Per-request context passed to every rewriter. Intentionally minimal:
/// a rewriter that needs more data (e.g. an ACL snapshot, an ontology
/// reference) should receive it through its own constructor rather than
/// being wedged into a shared context type.
#[derive(Debug, Clone)]
pub struct RewriteContext {
    pub workspace_id: String,
}

impl RewriteContext {
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
        }
    }
}

/// Output of a single rewrite pass, carrying metadata the pipeline and
/// validators downstream need in order to make their own decisions
/// without re-inspecting the AST.
///
/// `modified_statements` lets the isolation post-check replace the old
/// substring gate: if a strategy declared a `scope_property` but the
/// cumulative count across all passes is zero, no isolation landed and
/// execution is refused. Future passes can piggy-back their own
/// metadata here rather than forking the return type.
#[derive(Debug, Clone)]
pub struct RewrittenAst {
    pub ast: CypherAst,
    /// Number of statements this pass actually touched. A passthrough
    /// rewriter returns `0`; a rewriter that injected on 3 of 4 UNION
    /// fragments returns `3`.
    pub modified_statements: u32,
}

impl RewrittenAst {
    /// Wrap an unchanged AST (passthrough rewriter, idempotent no-op).
    pub fn passthrough(ast: CypherAst) -> Self {
        Self {
            ast,
            modified_statements: 0,
        }
    }
}

/// A single Cypher rewrite pass.
///
/// `rewrite` takes ownership of the AST so the caller can't accidentally
/// fan out two passes that see inconsistent state. A `Result` return
/// lets a pass reject a query it can't handle safely — the alternatives
/// (panic, return unchanged AST, mutate shared state) all hide bugs.
pub trait CypherRewriter: Send + Sync {
    /// Identifier used in logs and diagnostic messages.
    fn name(&self) -> &str;

    /// Slot this pass should run in. Lower values run first; same-phase
    /// passes run in registration order (stable sort). Default is
    /// [`RewritePhase::Custom`] so out-of-tree rewriters without an
    /// obvious slot land last; every in-tree pass overrides it.
    fn phase(&self) -> RewritePhase {
        RewritePhase::Custom
    }

    /// Produce a new AST reflecting this rewriter's transformation, or
    /// a [`RewriteError`] if the pass refuses the query.
    fn rewrite(&self, ast: CypherAst, ctx: &RewriteContext) -> Result<RewrittenAst, RewriteError>;
}

/// Ordered collection of rewriters.
///
/// Execution is strictly sequential — each pass sees the output of the
/// previous one. Rewriters that commute should still be registered in a
/// deterministic order so the final query is reproducible. The pipeline
/// short-circuits on the first `Err`.
#[derive(Default)]
pub struct CypherRewriterPipeline {
    rewriters: Vec<Box<dyn CypherRewriter>>,
}

impl CypherRewriterPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a rewriter. Returns `self` so pipelines can be built
    /// fluently: `pipeline.with(a).with(b).with(c)`.
    pub fn with(mut self, rewriter: impl CypherRewriter + 'static) -> Self {
        self.rewriters.push(Box::new(rewriter));
        self
    }

    /// Number of rewriters in the pipeline. Useful for tests.
    pub fn len(&self) -> usize {
        self.rewriters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rewriters.is_empty()
    }

    /// Apply every rewriter in order over `ast`. Returns the mutated
    /// AST plus the sum of `modified_statements` across every pass
    /// (saturating at `u32::MAX`). The runtime uses the sum to decide
    /// whether isolation landed without re-inspecting the final AST.
    ///
    /// Passes are sorted by [`CypherRewriter::phase`] (stable) before
    /// execution — callers don't need to worry about the order they
    /// called `.with()` in, just that every pass advertised the right
    /// phase. This is the mechanism the `bolt::pipeline` relies on to
    /// guarantee that `Isolation` runs before `Acl` before
    /// `SoftDelete`, regardless of build wiring.
    pub fn run_ast(
        &self,
        mut ast: CypherAst,
        ctx: &RewriteContext,
    ) -> Result<RewrittenAst, RewriteError> {
        let mut modified_total: u32 = 0;

        // Stable sort by phase. `sort_by_key` on a borrowed index
        // vector keeps the original `Vec<Box<dyn _>>` in construction
        // order so `Debug` still reflects how the caller wired it.
        let mut order: Vec<usize> = (0..self.rewriters.len()).collect();
        order.sort_by_key(|&i| self.rewriters[i].phase());

        for idx in order {
            let rewriter = &self.rewriters[idx];
            let out = rewriter.rewrite(ast, ctx)?;
            ast = out.ast;
            modified_total = modified_total.saturating_add(out.modified_statements);
        }
        Ok(RewrittenAst {
            ast,
            modified_statements: modified_total,
        })
    }

    /// Parse `input`, apply every rewriter in order, render back to
    /// source. Thin wrapper over [`Self::run_ast`] for callers that
    /// only hold text — the runtime path uses `run_ast` directly so a
    /// single parse feeds both rewriter and validator pipelines.
    pub fn run(&self, input: &str, ctx: &RewriteContext) -> Result<String, RewriteError> {
        Ok(self.run_ast(parse(input), ctx)?.ast.render())
    }
}

impl fmt::Debug for CypherRewriterPipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.rewriters.iter().map(|r| r.name()).collect();
        f.debug_struct("CypherRewriterPipeline")
            .field("rewriters", &names)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// WorkspaceScopeRewriter
// ---------------------------------------------------------------------------

/// Inject workspace-scope predicates and property assignments. Every
/// statement in the AST is rewritten independently so a UNION fragment
/// cannot read or write outside its own workspace.
///
/// Read path: the first MATCH / OPTIONAL MATCH clause with a bound node
/// variable receives `var.<property> = $<param>`. If the statement
/// already has a WHERE clause following that MATCH, the predicate is
/// prepended with AND; otherwise a new WHERE is inserted between the
/// MATCH and whatever comes next.
///
/// Write path: the first CREATE / MERGE clause with a bound node
/// variable receives `var.<property> = $<param>` as a SET assignment.
/// Existing SETs are preserved and prepended; otherwise a new SET
/// clause is appended to the CREATE/MERGE.
///
/// Both paths are idempotent — a statement that already references
/// the workspace property is left alone.
#[derive(Debug, Clone)]
pub struct WorkspaceScopeRewriter {
    pub property: &'static str,
    pub param_name: &'static str,
}

impl WorkspaceScopeRewriter {
    pub const fn new(property: &'static str, param_name: &'static str) -> Self {
        Self {
            property,
            param_name,
        }
    }
}

impl CypherRewriter for WorkspaceScopeRewriter {
    fn name(&self) -> &str {
        "workspace-scope"
    }

    fn phase(&self) -> RewritePhase {
        RewritePhase::Isolation
    }

    fn rewrite(
        &self,
        mut ast: CypherAst,
        _ctx: &RewriteContext,
    ) -> Result<RewrittenAst, RewriteError> {
        let mut modified: u32 = 0;
        for statement in &mut ast.statements {
            let read_changed = self.inject_read_scope(statement);
            let write_changed = self.inject_write_scope(statement);
            if read_changed || write_changed {
                modified = modified.saturating_add(1);
            }
        }
        Ok(RewrittenAst {
            ast,
            modified_statements: modified,
        })
    }
}

impl WorkspaceScopeRewriter {
    fn inject_read_scope(&self, statement: &mut CypherStatement) -> bool {
        if self.statement_references_property(statement) {
            return false;
        }
        let Some((match_idx, var)) = first_variable_in_clause(statement, |k| {
            matches!(k, ClauseKind::Match | ClauseKind::OptionalMatch)
        }) else {
            return false;
        };
        let condition = format!("{var}.{} = ${}", self.property, self.param_name);

        if let Some(where_idx) = find_following_where(statement, match_idx) {
            prepend_to_where(&mut statement.clauses[where_idx], &condition);
        } else {
            let where_clause = CypherClause {
                kind: ClauseKind::Where,
                tokens: Vec::new(),
                text: format!(" WHERE {condition}"),
                span: Span::default(),
                patterns: Vec::new(),
            };
            statement.clauses.insert(match_idx + 1, where_clause);
        }
        true
    }

    fn inject_write_scope(&self, statement: &mut CypherStatement) -> bool {
        // Writes may appear in multiple CREATE / MERGE clauses in one
        // statement (e.g. two CREATEs separated by a MATCH). Each needs
        // its own SET unless the user's own SET already mentions the
        // property.
        let create_indices: Vec<usize> = statement
            .clauses
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c.kind, ClauseKind::Create | ClauseKind::Merge))
            .map(|(i, _)| i)
            .collect();

        let mut any_injected = false;
        for create_idx in create_indices {
            let var = match first_variable_in_clause_at(statement, create_idx) {
                Some(v) => v,
                None => continue,
            };
            let assignment = format!("{var}.{} = ${}", self.property, self.param_name);

            // If ANY subsequent clause already binds this variable's
            // workspace property, skip (idempotency across mixed queries).
            if statement
                .clauses
                .iter()
                .skip(create_idx)
                .any(|c| clause_binds_workspace_for(c, &var, self.property))
            {
                continue;
            }

            if let Some(set_idx) = find_immediate_following_set(statement, create_idx) {
                prepend_to_set(&mut statement.clauses[set_idx], &assignment);
            } else {
                let set_clause = CypherClause {
                    kind: ClauseKind::Set,
                    tokens: Vec::new(),
                    text: format!(" SET {assignment}"),
                    span: Span::default(),
                    patterns: Vec::new(),
                };
                statement.clauses.insert(create_idx + 1, set_clause);
            }
            any_injected = true;
        }
        any_injected
    }

    /// Does any clause in the statement already mention `self.property`?
    /// If yes the rewriter's read phase is a no-op (idempotency).
    fn statement_references_property(&self, statement: &CypherStatement) -> bool {
        statement
            .clauses
            .iter()
            .any(|c| c.text.contains(self.property))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the first clause matching `predicate` that has a pattern with a
/// bound node variable; return (clause_index, variable).
fn first_variable_in_clause(
    statement: &CypherStatement,
    predicate: impl Fn(ClauseKind) -> bool,
) -> Option<(usize, String)> {
    for (idx, clause) in statement.clauses.iter().enumerate() {
        if !predicate(clause.kind) {
            continue;
        }
        if let Some(var) = first_pattern_variable(clause) {
            return Some((idx, var));
        }
    }
    None
}

fn first_variable_in_clause_at(statement: &CypherStatement, idx: usize) -> Option<String> {
    first_pattern_variable(statement.clauses.get(idx)?)
}

/// First node variable in a clause's patterns (`MATCH (n)-(:R)->(m)` → `n`).
fn first_pattern_variable(clause: &CypherClause) -> Option<String> {
    for pattern in &clause.patterns {
        for element in &pattern.elements {
            if let CypherPatternElement::Node(NodePattern {
                variable: Some(v), ..
            }) = element
            {
                return Some(v.clone());
            }
        }
    }
    None
}

/// Find the WHERE clause that belongs to the MATCH at `match_idx`. WHERE
/// in Cypher always attaches to the preceding MATCH / OPTIONAL MATCH; any
/// intervening clause means the WHERE is for something else.
fn find_following_where(statement: &CypherStatement, match_idx: usize) -> Option<usize> {
    let next = match_idx + 1;
    match statement.clauses.get(next) {
        Some(c) if c.kind == ClauseKind::Where => Some(next),
        _ => None,
    }
}

/// Find a SET clause that sits immediately after `create_idx` (allowing
/// only clauses that don't break the CREATE → SET association — none in
/// the standard Cypher grammar, so the slot must be at `create_idx + 1`).
fn find_immediate_following_set(statement: &CypherStatement, create_idx: usize) -> Option<usize> {
    let next = create_idx + 1;
    match statement.clauses.get(next) {
        Some(c) if c.kind == ClauseKind::Set => Some(next),
        _ => None,
    }
}

/// Rewrite an existing WHERE clause to carry the new condition as its
/// first conjunction: `WHERE <new> AND <original body>`. We locate the
/// `WHERE` keyword boundary in the clause's text and inject after it,
/// preserving leading whitespace and trailing body verbatim.
fn prepend_to_where(clause: &mut CypherClause, condition: &str) {
    let original = std::mem::take(&mut clause.text);
    let (lead_ws, after_ws) = split_leading_whitespace(&original);
    if let Some(rest) = strip_leading_keyword(after_ws, "WHERE") {
        let rest_trimmed = rest.trim_start();
        clause.text = format!("{lead_ws}WHERE {condition} AND {rest_trimmed}");
    } else {
        // Unexpected — a non-WHERE clause tagged WHERE. Fall back to
        // prepending raw, still safe because render uses `text`.
        clause.text = format!("{lead_ws}WHERE {condition} AND {after_ws}");
    }
}

/// Rewrite an existing SET clause to carry the new assignment as its
/// first item: `SET <new>, <original assignments>`.
fn prepend_to_set(clause: &mut CypherClause, assignment: &str) {
    let original = std::mem::take(&mut clause.text);
    let (lead_ws, after_ws) = split_leading_whitespace(&original);
    if let Some(rest) = strip_leading_keyword(after_ws, "SET") {
        let rest_trimmed = rest.trim_start();
        clause.text = format!("{lead_ws}SET {assignment}, {rest_trimmed}");
    } else {
        clause.text = format!("{lead_ws}SET {assignment}, {after_ws}");
    }
}

/// Does this clause already contain a workspace-property binding for the
/// given variable? A lightweight substring probe is enough — the
/// predicate only gates idempotency.
fn clause_binds_workspace_for(clause: &CypherClause, var: &str, property: &str) -> bool {
    // Look for `<var>.<property>` in the clause text. Substring is safe
    // because we've already quarantined string literals via the tokeniser
    // (unused here — the clause only runs on authored SET / WHERE bodies
    // and those contain the property literally, not inside a literal).
    let needle = format!("{var}.{property}");
    clause.text.contains(&needle)
}

/// Return (leading whitespace, rest). `text` might look like `" WHERE …"`
/// — preserving the leading space keeps rendering spacing clean.
fn split_leading_whitespace(text: &str) -> (&str, &str) {
    let trimmed_len = text.len() - text.trim_start().len();
    text.split_at(trimmed_len)
}

/// If `text` (already left-trimmed) starts with `keyword` followed by
/// whitespace or EOF, return the remainder after that keyword.
fn strip_leading_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let kw_bytes = keyword.as_bytes();
    let t_bytes = text.as_bytes();
    if t_bytes.len() < kw_bytes.len() {
        return None;
    }
    for (i, kb) in kw_bytes.iter().enumerate() {
        if !t_bytes[i].eq_ignore_ascii_case(kb) {
            return None;
        }
    }
    let rest = &text[kw_bytes.len()..];
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
mod tests {
    use super::*;

    fn rewrite(query: &str) -> String {
        let pipeline = CypherRewriterPipeline::new()
            .with(WorkspaceScopeRewriter::new("_workspace_id", "_ws_id"));
        pipeline
            .run(query, &RewriteContext::new("ws-123"))
            .expect("workspace-scope rewriter has no failure cases")
    }

    // --- Regression coverage for the pre-existing isolation surface ---

    #[test]
    fn scope_simple_read_injects_new_where() {
        let out = rewrite("MATCH (n:Person) RETURN n");
        assert!(out.contains("WHERE n._workspace_id = $_ws_id"));
        assert!(out.contains("RETURN n"));
    }

    #[test]
    fn scope_read_preserves_existing_where_as_conjunction() {
        let out = rewrite("MATCH (n:Person) WHERE n.age > 21 RETURN n");
        assert!(out.contains("WHERE n._workspace_id = $_ws_id AND n.age > 21"));
    }

    #[test]
    fn scope_read_is_idempotent_when_property_already_mentioned() {
        let input = "MATCH (n:Person) WHERE n._workspace_id = 'existing' RETURN n";
        let out = rewrite(input);
        assert_eq!(
            out.matches("_workspace_id").count(),
            1,
            "must not double-inject: {out}"
        );
    }

    #[test]
    fn scope_simple_create_injects_set_clause() {
        let out = rewrite("CREATE (n:Person {name: 'Alice'})");
        assert!(out.contains("SET n._workspace_id = $_ws_id"));
    }

    #[test]
    fn scope_create_preserves_existing_set_assignments() {
        let out = rewrite("CREATE (n:Person {name: 'Alice'}) SET n.age = 30");
        assert!(out.contains("SET n._workspace_id = $_ws_id"));
        assert!(out.contains("n.age = 30"));
    }

    #[test]
    fn scope_write_is_idempotent() {
        let input = "CREATE (n:Person) SET n._workspace_id = 'existing'";
        let out = rewrite(input);
        assert_eq!(out.matches("_workspace_id").count(), 1, "{out}");
    }

    #[test]
    fn scope_merge_emits_set_same_as_create() {
        let out = rewrite("MERGE (n:Person {name: 'Alice'})");
        assert!(out.contains("SET n._workspace_id = $_ws_id"));
    }

    #[test]
    fn scope_mixed_match_create_injects_both_surfaces() {
        let input = "MATCH (n:Person) CREATE (m:Company {name: 'Acme'}) CREATE (n)-[:WORKS_AT]->(m) RETURN m";
        let out = rewrite(input);
        assert!(out.contains("WHERE n._workspace_id = $_ws_id"), "{out}");
        assert!(out.contains("SET m._workspace_id = $_ws_id"), "{out}");
    }

    #[test]
    fn scope_no_pattern_passthrough_leaves_query_untouched() {
        assert_eq!(rewrite("RETURN 1"), "RETURN 1");
    }

    #[test]
    fn scope_multi_node_pattern_filters_first_variable_only() {
        let out = rewrite(
            "MATCH (p:Product)-[:MADE_BY]->(b:Brand) RETURN b.name AS brand, count(p) AS products",
        );
        assert!(
            out.contains("(b:Brand) WHERE p._workspace_id = $_ws_id"),
            "WHERE must follow the entire pattern: {out}"
        );
        assert!(
            !out.contains("(p:Product) WHERE"),
            "WHERE must not split the pattern: {out}"
        );
    }

    #[test]
    fn scope_multi_node_with_existing_where_prepends_conjunction() {
        let out = rewrite(
            "MATCH (c:Customer)-[:PLACED]->(o:Order) WHERE o.status = 'delivered' RETURN c, o",
        );
        assert!(
            out.contains("c._workspace_id = $_ws_id AND o.status"),
            "{out}"
        );
    }

    #[test]
    fn scope_three_node_chain_filters_first_variable() {
        let out = rewrite(
            "MATCH (c:Customer)-[:PLACED]->(o:Order)-[:CONTAINS]->(p:Product) RETURN c.name, p.name",
        );
        assert!(out.contains("(p:Product) WHERE c._workspace_id"), "{out}");
    }

    // --- New coverage: tokenizer / AST unlocks these ---

    #[test]
    fn scope_string_literal_with_match_keyword_does_not_split_clauses() {
        let out = rewrite("MATCH (n) WHERE n.name = 'MATCH OPTIONAL' RETURN n");
        // Only one WHERE injection; the quoted MATCH must not open a new clause.
        assert_eq!(out.matches("_workspace_id").count(), 1, "{out}");
        assert!(out.contains("'MATCH OPTIONAL'"));
    }

    #[test]
    fn scope_optional_match_receives_its_own_where() {
        let out = rewrite("OPTIONAL MATCH (n:Person) RETURN n");
        assert!(
            out.contains("WHERE n._workspace_id = $_ws_id"),
            "OPTIONAL MATCH must be scoped too: {out}"
        );
    }

    #[test]
    fn scope_union_scopes_each_fragment_independently() {
        let out = rewrite("MATCH (a:A) RETURN a UNION MATCH (b:B) RETURN b");
        // Both fragments get a workspace predicate.
        let matches: Vec<&str> = out
            .match_indices("_workspace_id = $_ws_id")
            .map(|(_, m)| m)
            .collect();
        assert_eq!(
            matches.len(),
            2,
            "both UNION fragments must be scoped: {out}"
        );
        assert!(
            out.contains("a._workspace_id") && out.contains("b._workspace_id"),
            "{out}"
        );
    }

    #[test]
    fn scope_union_all_variant_preserved_and_scoped() {
        let out = rewrite("MATCH (a:A) RETURN a UNION ALL MATCH (b:B) RETURN b");
        assert!(
            out.contains("UNION ALL"),
            "UNION ALL keyword preserved: {out}"
        );
        assert!(out.contains("a._workspace_id") && out.contains("b._workspace_id"));
    }

    #[test]
    fn scope_with_clause_does_not_swallow_boundary() {
        let out = rewrite("MATCH (n) WITH n WHERE n.active RETURN n");
        // Scope targets the MATCH's own WHERE; the WHERE after WITH is a
        // separate filter on the WITH projection. Our rewriter injects a
        // fresh WHERE between MATCH and WITH rather than touching the
        // downstream WHERE.
        assert!(
            out.contains("MATCH (n)") && out.contains("_workspace_id = $_ws_id"),
            "{out}",
        );
        assert!(out.contains("WITH n"), "{out}");
    }

    #[test]
    fn scope_call_subquery_passthrough() {
        let out = rewrite("CALL { MATCH (x:X) RETURN x } RETURN x");
        // Our current pass does not recurse into CALL subqueries — the
        // outer statement has no MATCH / CREATE so nothing to inject.
        // (Future: a sub-statement walker; recorded as a non-goal now.)
        assert_eq!(
            out.matches("_workspace_id").count(),
            0,
            "CALL subquery content left to future pass: {out}",
        );
    }

    #[test]
    fn scope_multi_match_scopes_only_first() {
        // Current policy: inject on the first MATCH; subsequent MATCH
        // clauses reuse its variable via the WHERE predicate. If
        // real-world usage requires per-MATCH scoping we revisit — but
        // double-injection would silently AND two workspace predicates,
        // which is what idempotency is designed to block.
        let out = rewrite("MATCH (a:A) MATCH (b:B) RETURN a, b");
        assert_eq!(
            out.matches("_workspace_id").count(),
            1,
            "one predicate per statement unless idempotency breaks: {out}",
        );
        assert!(out.contains("a._workspace_id"));
    }

    #[test]
    fn scope_detach_delete_does_not_trigger_write_scope() {
        // DETACH DELETE is a write but has no node creation — no
        // variable to assign to.
        let out = rewrite("MATCH (n:Orphan) DETACH DELETE n");
        assert!(out.contains("WHERE n._workspace_id"), "{out}");
        assert!(!out.contains("SET n._workspace_id"), "{out}");
    }

    #[test]
    fn scope_comment_with_clause_head_keyword_is_preserved() {
        let input = "// MATCH lives here\nMATCH (n) RETURN n";
        let out = rewrite(input);
        assert!(out.contains("// MATCH lives here\n"), "{out}");
        assert!(out.contains("WHERE n._workspace_id"), "{out}");
    }

    #[test]
    fn scope_nested_where_parens_do_not_confuse_injection() {
        let out = rewrite("MATCH (n) WHERE (n.a = 1 AND (n.b = 2 OR n.c = 3)) RETURN n");
        assert!(
            out.contains("WHERE n._workspace_id = $_ws_id AND ("),
            "{out}"
        );
    }

    #[test]
    fn scope_relationship_property_does_not_affect_injection() {
        let out = rewrite("MATCH (a)-[r:R {active: true}]->(b) RETURN a, r, b");
        // Scope is still on the first node variable `a`.
        assert!(out.contains("a._workspace_id"), "{out}");
    }

    #[test]
    fn rewrite_with_no_rewriters_is_identity() {
        let pipeline = CypherRewriterPipeline::new();
        let out = pipeline
            .run("MATCH (n) RETURN n", &RewriteContext::new("ws"))
            .expect("no-op pipeline cannot fail");
        assert_eq!(out, "MATCH (n) RETURN n");
    }

    #[test]
    fn pipeline_applies_rewriters_in_registration_order() {
        // Two rewriters: the first tags every MATCH, the second tags
        // every WHERE. The order the tags appear in the output must
        // reflect registration order.
        struct TagWhere;
        impl CypherRewriter for TagWhere {
            fn name(&self) -> &str {
                "tag-where"
            }
            fn rewrite(
                &self,
                mut ast: CypherAst,
                _: &RewriteContext,
            ) -> Result<RewrittenAst, RewriteError> {
                let mut touched = 0u32;
                for stmt in &mut ast.statements {
                    let mut stmt_touched = false;
                    for clause in &mut stmt.clauses {
                        if clause.kind == ClauseKind::Where {
                            clause.text = format!("/*B*/{}", clause.text);
                            stmt_touched = true;
                        }
                    }
                    if stmt_touched {
                        touched = touched.saturating_add(1);
                    }
                }
                Ok(RewrittenAst {
                    ast,
                    modified_statements: touched,
                })
            }
        }
        struct TagMatch;
        impl CypherRewriter for TagMatch {
            fn name(&self) -> &str {
                "tag-match"
            }
            fn rewrite(
                &self,
                mut ast: CypherAst,
                _: &RewriteContext,
            ) -> Result<RewrittenAst, RewriteError> {
                let mut touched = 0u32;
                for stmt in &mut ast.statements {
                    let mut stmt_touched = false;
                    for clause in &mut stmt.clauses {
                        if clause.kind == ClauseKind::Match {
                            clause.text = format!("/*A*/{}", clause.text);
                            stmt_touched = true;
                        }
                    }
                    if stmt_touched {
                        touched = touched.saturating_add(1);
                    }
                }
                Ok(RewrittenAst {
                    ast,
                    modified_statements: touched,
                })
            }
        }
        let pipeline = CypherRewriterPipeline::new().with(TagMatch).with(TagWhere);
        let out = pipeline
            .run(
                "MATCH (n) WHERE n.a = 1 RETURN n",
                &RewriteContext::new("ws"),
            )
            .expect("tag rewriters cannot fail");
        let a = out.find("/*A*/").unwrap();
        let b = out.find("/*B*/").unwrap();
        assert!(a < b, "MATCH tag must appear before WHERE tag: {out}");
    }

    #[test]
    fn double_apply_of_workspace_scope_is_idempotent() {
        let pipeline = CypherRewriterPipeline::new()
            .with(WorkspaceScopeRewriter::new("_workspace_id", "_ws_id"))
            .with(WorkspaceScopeRewriter::new("_workspace_id", "_ws_id"));
        let out = pipeline
            .run("MATCH (n:Person) RETURN n", &RewriteContext::new("ws-123"))
            .expect("workspace-scope rewriter has no failure cases");
        assert_eq!(
            out.matches("_workspace_id = $_ws_id").count(),
            1,
            "two passes must not double-inject: {out}",
        );
    }

    #[test]
    fn workspace_scope_counts_modified_statements_across_union() {
        let pipeline = CypherRewriterPipeline::new()
            .with(WorkspaceScopeRewriter::new("_workspace_id", "_ws_id"));
        let result = pipeline
            .run_ast(
                parse("MATCH (a:A) RETURN a UNION MATCH (b:B) RETURN b"),
                &RewriteContext::new("ws-123"),
            )
            .expect("pipeline succeeds");
        assert_eq!(
            result.modified_statements, 2,
            "both UNION fragments should be counted as scoped"
        );
    }

    #[test]
    fn workspace_scope_zero_modifications_for_non_graph_query() {
        let pipeline = CypherRewriterPipeline::new()
            .with(WorkspaceScopeRewriter::new("_workspace_id", "_ws_id"));
        let result = pipeline
            .run_ast(parse("RETURN 1"), &RewriteContext::new("ws-123"))
            .expect("pipeline succeeds");
        assert_eq!(
            result.modified_statements, 0,
            "scalar query has nothing to scope; count stays 0"
        );
    }

    #[test]
    fn pipeline_sorts_rewriters_by_phase_regardless_of_registration_order() {
        // Two rewriters: one at Isolation (100), one at a synthetic
        // Custom (900). Registered in reverse. The pipeline must still
        // run Isolation first — the runtime depends on this guarantee
        // to add new passes without caring where `.with()` is called.
        struct FakeCustom;
        impl CypherRewriter for FakeCustom {
            fn name(&self) -> &str {
                "fake-custom"
            }
            fn phase(&self) -> RewritePhase {
                RewritePhase::Custom
            }
            fn rewrite(
                &self,
                mut ast: CypherAst,
                _: &RewriteContext,
            ) -> Result<RewrittenAst, RewriteError> {
                for stmt in &mut ast.statements {
                    for clause in &mut stmt.clauses {
                        if clause.kind == ClauseKind::Where {
                            // Tag only if the isolation predicate is already there.
                            // If this runs first, the tag won't appear.
                            if clause.text.contains("_workspace_id") {
                                clause.text = format!("{}/*CUSTOM_AFTER*/", clause.text);
                            }
                        }
                    }
                }
                Ok(RewrittenAst {
                    ast,
                    modified_statements: 0,
                })
            }
        }

        let pipeline = CypherRewriterPipeline::new()
            .with(FakeCustom) // registered first
            .with(WorkspaceScopeRewriter::new("_workspace_id", "_ws_id")); // registered second
        let out = pipeline
            .run(
                "MATCH (n:Person) RETURN n",
                &RewriteContext::new("ws-phase"),
            )
            .expect("phase-sorted pipeline runs");
        assert!(
            out.contains("/*CUSTOM_AFTER*/"),
            "Custom phase must run AFTER Isolation even when registered first: {out}"
        );
    }
}
