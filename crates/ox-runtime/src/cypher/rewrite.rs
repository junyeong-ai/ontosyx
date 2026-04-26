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

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::cypher::ast::{
    ClauseKind, CypherAst, CypherClause, CypherPatternElement, CypherStatement, NodePattern,
};
use crate::cypher::parse;
use crate::cypher::rewrite_helpers::{
    find_following_where_clause, split_leading_whitespace, strip_leading_keyword,
};
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

/// Per-request context passed to every rewriter.
///
/// Originally minimal (workspace id only) — extended at Phase 5 with
/// ACL fields so [`crate::cypher::acl_rewriter::AclRewriter`] can run
/// pre-filtered, principal-scoped policies without each rewriter
/// receiving its own loader. The snapshot is loaded once per request
/// by the runtime entry point and threaded through unchanged for
/// every pass.
///
/// Intentionally **not** `Default` — `Default::default()` would
/// produce an empty `workspace_id` string, and any rewriter that
/// reads it would silently scope to "no workspace". Construction
/// goes through [`RewriteContext::new(workspace_id)`] so the
/// caller has to think about the scope.
#[derive(Debug, Clone)]
pub struct RewriteContext {
    pub workspace_id: String,
    /// UUID of the authenticated principal, if the request carried
    /// one. `None` means a system-bypass / scheduled-task call;
    /// rewriters that need a principal should treat `None` as
    /// "skip this pass" rather than "deny everything".
    pub principal_id: Option<uuid::Uuid>,
    /// Workspace role string ("owner" / "admin" / "member" / "viewer").
    /// Carried as a free-form string to keep the rewriter layer
    /// independent of the ox-store enum and to leave room for
    /// platform-role overrides without a context shape change.
    pub principal_role: Option<String>,
    /// Pre-loaded ACL policy snapshot for the current principal in
    /// the current workspace, sorted priority-desc. Loaded by the
    /// runtime entry point ahead of pipeline execution; `None`
    /// disables ACL rewriting for this request.
    pub acl_snapshot:
        Option<std::sync::Arc<crate::cypher::acl_rewriter::AclSnapshot>>,
    /// Bypass the [`crate::cypher::soft_delete_rewriter::SoftDeleteRewriter`]
    /// pass for this request. `true` only on admin paths that
    /// intentionally need to read or hard-delete already-tombstoned
    /// rows (the retention compaction job, support tooling, an
    /// "un-delete" admin action). The agent / chat / federation
    /// paths leave this `false` so they never see soft-deleted data.
    pub skip_soft_delete: bool,
}

impl RewriteContext {
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            principal_id: None,
            principal_role: None,
            acl_snapshot: None,
            skip_soft_delete: false,
        }
    }

    pub fn with_principal(
        mut self,
        principal_id: uuid::Uuid,
        role: impl Into<String>,
    ) -> Self {
        self.principal_id = Some(principal_id);
        self.principal_role = Some(role.into());
        self
    }

    pub fn with_acl_snapshot(
        mut self,
        snapshot: std::sync::Arc<crate::cypher::acl_rewriter::AclSnapshot>,
    ) -> Self {
        self.acl_snapshot = Some(snapshot);
        self
    }

    pub fn with_skip_soft_delete(mut self, skip: bool) -> Self {
        self.skip_soft_delete = skip;
        self
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
/// Read path: for **every** MATCH / OPTIONAL MATCH clause in the
/// statement, **every** bound node variable in its pattern receives
/// `var.<property> = $<param>`. A single WHERE clause per MATCH
/// carries all injected conditions; if the statement already has a
/// WHERE immediately following that MATCH, the conditions are
/// prepended as a conjunction; otherwise a new WHERE is inserted
/// between the MATCH and whatever comes next.
///
/// A chained pattern (`MATCH (a:A)-[r]->(b:B)`) binds two node
/// variables — both are scoped. Sequential clauses
/// (`MATCH (a:A) WITH a MATCH (b:B) RETURN a, b`) bind separate node
/// variables — both are scoped. The pre-rewrite design that only
/// scoped the first variable was an isolation gap: the query
/// `MATCH (a:A) MATCH (b:B) RETURN a, b` is a Cartesian product and
/// `b` was unconstrained.
///
/// Write path: the first CREATE / MERGE clause with a bound node
/// variable receives `var.<property> = $<param>` as a SET assignment.
/// Existing SETs are preserved and prepended; otherwise a new SET
/// clause is appended to the CREATE/MERGE.
///
/// Idempotency: a per-variable guard checks whether some clause in the
/// statement already binds `<var>.<property> = $<param>` (the
/// system-injected shape). If so, that variable is skipped — a
/// pipeline that runs the rewriter twice does not double-inject. A
/// user-supplied literal such as `WHERE n._workspace_id = 'other'`
/// does **not** match the guard; it is AND-neutralised by the
/// injected system predicate, so a query attempting to read another
/// workspace evaluates to the empty set rather than silently
/// bypassing isolation.
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
        // Collect (clause_idx, variable) pairs for every bound node
        // variable in every MATCH / OPTIONAL MATCH clause. A single
        // MATCH can bind multiple variables (chained pattern); a
        // statement can contain multiple sequential MATCHes. Both must
        // be scoped.
        //
        // `queued` dedupes across the whole statement — once a variable
        // is slated for injection (or is already system-bound), it is
        // not queued again in a later clause. Order preserved via
        // `BTreeMap<clause_idx, Vec<var>>` so the rendering is stable.
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
                if statement_binds_system_param_for(
                    statement,
                    &var,
                    self.property,
                    self.param_name,
                ) {
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

        // Iterate in reverse order so inserting a new WHERE at
        // `clause_idx + 1` does not shift earlier indices.
        for (clause_idx, vars) in plan.iter().rev() {
            let combined = vars
                .iter()
                .map(|v| format!("{v}.{} = ${}", self.property, self.param_name))
                .collect::<Vec<_>>()
                .join(" AND ");

            if let Some(where_idx) = find_following_where_clause(statement, *clause_idx) {
                prepend_to_where(&mut statement.clauses[where_idx], &combined);
            } else {
                let where_clause = CypherClause {
                    kind: ClauseKind::Where,
                    tokens: Vec::new(),
                    text: format!(" WHERE {combined}"),
                    span: Span::default(),
                    patterns: Vec::new(),
                };
                statement.clauses.insert(*clause_idx + 1, where_clause);
            }
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
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Node variables bound by a clause's patterns, in order, deduplicated.
/// `MATCH (a)-[:R]->(b)-[:S]->(a)` returns `["a", "b"]` — each variable
/// at most once, preserving first-occurrence order for stable rendering.
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

/// First node variable bound by the clause at `idx`, if any. Used by the
/// write path, which still targets a single variable per CREATE/MERGE
/// (the write surface has not been lifted to per-variable coverage yet;
/// see Phase 1 follow-up).
fn first_variable_in_clause_at(statement: &CypherStatement, idx: usize) -> Option<String> {
    bound_node_variables(statement.clauses.get(idx)?)
        .into_iter()
        .next()
}

/// Does some clause in `statement` already contain the literal
/// Idempotency check for the workspace-scope rewriter — does some
/// clause already carry the literal system-injected shape
/// `<var>.<property> = $<param>`?
///
/// Substring (not token) match on purpose: the rewriter mutates
/// `clause.text` but not `clause.tokens` after an injection, so a
/// token-level walk on the second pass would not see the predicate
/// it just injected on the first pass. Text-substring trivially
/// observes the just-written text — exactly the property
/// idempotency wants.
///
/// Only the system-param form satisfies the guard. A user-supplied
/// predicate such as `n._workspace_id = 'other_ws'` does NOT match,
/// so the rewriter still injects its own `$_ws_id` clause, and the
/// two AND-combine to evaluate to the empty set for any non-matching
/// literal — the rewriter's gate is authoritative, not the author's
/// prior text.
fn statement_binds_system_param_for(
    statement: &CypherStatement,
    var: &str,
    property: &str,
    param_name: &str,
) -> bool {
    let needle = format!("{var}.{property} = ${param_name}");
    statement.clauses.iter().any(|c| c.text.contains(&needle))
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
    fn scope_user_supplied_literal_is_and_neutralised() {
        // A user-supplied literal workspace predicate does not satisfy
        // the system-param idempotency guard (that guard looks for
        // `$_ws_id` specifically). The rewriter still injects its own
        // system predicate, AND-combined with the author's text, so a
        // query attempting to read another workspace evaluates to the
        // empty set rather than silently bypassing isolation.
        let input = "MATCH (n:Person) WHERE n._workspace_id = 'other_ws' RETURN n";
        let out = rewrite(input);
        assert!(
            out.contains("n._workspace_id = $_ws_id AND n._workspace_id = 'other_ws'"),
            "author literal must be AND-neutralised by the system predicate: {out}"
        );
    }

    #[test]
    fn scope_read_is_idempotent_when_system_param_already_bound() {
        // The system-injected shape (`= $_ws_id`) is exactly what
        // dedup detects. A prior pass (or a human who wrote the
        // canonical predicate) leaves the statement alone.
        let input = "MATCH (n:Person) WHERE n._workspace_id = $_ws_id RETURN n";
        let out = rewrite(input);
        assert_eq!(
            out.matches("_workspace_id = $_ws_id").count(),
            1,
            "must not double-inject the system predicate: {out}"
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
    fn scope_multi_node_pattern_scopes_every_bound_variable() {
        // Every node variable bound by the pattern gets its own
        // predicate, joined with AND. The WHERE still follows the
        // entire pattern — pattern boundaries are never split.
        let out = rewrite(
            "MATCH (p:Product)-[:MADE_BY]->(b:Brand) RETURN b.name AS brand, count(p) AS products",
        );
        assert!(
            out.contains("p._workspace_id = $_ws_id AND b._workspace_id = $_ws_id"),
            "both p and b must be scoped: {out}"
        );
        assert!(
            !out.contains("(p:Product) WHERE"),
            "WHERE must not split the pattern: {out}"
        );
    }

    #[test]
    fn scope_multi_node_with_existing_where_prepends_all_conjunctions() {
        let out = rewrite(
            "MATCH (c:Customer)-[:PLACED]->(o:Order) WHERE o.status = 'delivered' RETURN c, o",
        );
        assert!(
            out.contains(
                "c._workspace_id = $_ws_id AND o._workspace_id = $_ws_id AND o.status"
            ),
            "injected predicates must precede the author WHERE body: {out}"
        );
    }

    #[test]
    fn scope_three_node_chain_scopes_every_bound_variable() {
        let out = rewrite(
            "MATCH (c:Customer)-[:PLACED]->(o:Order)-[:CONTAINS]->(p:Product) RETURN c.name, p.name",
        );
        for var in ["c", "o", "p"] {
            assert!(
                out.contains(&format!("{var}._workspace_id = $_ws_id")),
                "chain variable {var} must be scoped: {out}"
            );
        }
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
    fn scope_multi_match_scopes_every_match_clause() {
        // `MATCH (a:A) MATCH (b:B) RETURN a, b` is a Cartesian product.
        // Scoping only `a` was an isolation gap — `b` was unconstrained
        // and any matching node from any workspace would join in.
        // Every MATCH clause now receives its own WHERE, each bound
        // variable scoped to the active workspace.
        let out = rewrite("MATCH (a:A) MATCH (b:B) RETURN a, b");
        assert_eq!(
            out.matches("_workspace_id = $_ws_id").count(),
            2,
            "each MATCH must produce its own predicate: {out}",
        );
        assert!(out.contains("a._workspace_id = $_ws_id"), "{out}");
        assert!(out.contains("b._workspace_id = $_ws_id"), "{out}");
    }

    #[test]
    fn scope_match_then_with_then_match_covers_both_matches() {
        // A WITH projection separates two MATCH clauses. Both bind
        // their own variable; both need scoping.
        let out = rewrite("MATCH (a:A) WITH a MATCH (b:B) RETURN a, b");
        assert!(out.contains("a._workspace_id = $_ws_id"), "{out}");
        assert!(out.contains("b._workspace_id = $_ws_id"), "{out}");
    }

    #[test]
    fn scope_optional_match_reusing_variable_does_not_double_inject() {
        // `OPTIONAL MATCH (a)-->(b)` introduces `b`; `a` is already
        // bound by the outer MATCH and already scoped, so only `b`
        // gets a fresh predicate on the OPTIONAL MATCH.
        let out = rewrite("MATCH (a:A) OPTIONAL MATCH (a)-->(b:B) RETURN a, b");
        assert_eq!(
            out.matches("a._workspace_id = $_ws_id").count(),
            1,
            "a scoped once, not once per MATCH: {out}",
        );
        assert!(
            out.contains("b._workspace_id = $_ws_id"),
            "b must be scoped on the OPTIONAL MATCH: {out}",
        );
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
    fn scope_relationship_property_does_not_affect_node_injection() {
        // A relationship variable with its own property map does not
        // change node-variable injection: both `a` and `b` are scoped.
        // (The relationship variable `r` is not scoped — phase-1 write
        // coverage of edges is tracked as a follow-up.)
        let out = rewrite("MATCH (a)-[r:R {active: true}]->(b) RETURN a, r, b");
        assert!(out.contains("a._workspace_id = $_ws_id"), "{out}");
        assert!(out.contains("b._workspace_id = $_ws_id"), "{out}");
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
