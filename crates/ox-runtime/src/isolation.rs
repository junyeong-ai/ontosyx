// ---------------------------------------------------------------------------
// GraphIsolationStrategy — workspace isolation for graph databases
// ---------------------------------------------------------------------------
// PostgreSQL uses RLS (row-level security) for isolation.
// Graph databases need a different approach: property-based filtering
// on nodes/relationships, or (Enterprise only) separate databases.
//
// The trait is AST-based rather than text-based: `run_pre_execute`
// parses the incoming query once and then threads the resulting AST
// through validator → rewriter → validator without redundant parses.
// Strategies that don't need to touch the AST (e.g. DatabaseStrategy
// where isolation lives at the connection layer) return it unchanged.
// ---------------------------------------------------------------------------

use crate::cypher::{
    CypherAst, CypherRewriterPipeline, RewriteContext, RewriteError, WorkspaceScopeRewriter,
};

/// Strategy for isolating graph data between workspaces.
///
/// Implementations transform an AST to enforce workspace boundaries
/// at the graph layer. A single `scope_ast` method handles both reads
/// and writes so that mixed queries (MATCH + CREATE) are correctly
/// scoped in one pass.
pub trait GraphIsolationStrategy: Send + Sync {
    /// Inject workspace isolation into the Cypher AST.
    ///
    /// Handles all query types:
    /// - MATCH patterns get WHERE filters (read isolation)
    /// - CREATE/MERGE patterns get SET clauses (write ownership)
    /// - Mixed queries get both transformations
    ///
    /// Returns `Err(RewriteError)` if the underlying rewriter pipeline
    /// refuses the query. Today's strategies never refuse (property-
    /// strategy is purely additive; database-strategy is passthrough),
    /// but an ACL- or soft-delete-extended strategy will need to —
    /// and that needs to surface cleanly, not silently.
    fn scope_ast(&self, ast: CypherAst, workspace_id: &str) -> Result<ScopedAst, RewriteError>;

    /// Strategy name (for logging and config).
    fn name(&self) -> &str;

    /// Property name that every graph-touching statement must textually
    /// reference after `scope_ast` runs, so the runtime can gate
    /// isolation via `ScopedAst.modified_statements` instead of the
    /// old substring probe. `Some(prop)` for property-based
    /// strategies; `None` for connection-level strategies whose
    /// isolation is enforced outside the Cypher text (e.g.
    /// database-per-workspace).
    fn scope_property(&self) -> Option<&'static str>;
}

/// An AST with any bind-parameters the strategy introduced during
/// rewriting, plus the cumulative `modified_statements` count from the
/// underlying rewriter pipeline.
///
/// `params` uses owned `String` keys so future strategies aren't
/// constrained to compile-time constants for parameter names (the
/// original `&'static str` worked for today's two strategies but
/// would break an ACL strategy that derives parameter names from
/// per-request policy).
///
/// `modified_statements` lets the runtime refuse a query whose
/// declared `scope_property` requires isolation but whose rewriter
/// left every statement untouched — the case the old substring-based
/// `WorkspaceScopeValidator` was built to catch.
pub struct ScopedAst {
    pub ast: CypherAst,
    pub params: Vec<(String, String)>,
    pub modified_statements: u32,
}

// ---------------------------------------------------------------------------
// PropertyStrategy — workspace isolation via node properties
// ---------------------------------------------------------------------------
// Community-compatible. Adds `_workspace_id` property to all nodes.
// MATCH patterns get WHERE filter; CREATE/MERGE get SET clause.
//
// Node labels remain semantically clean — no prefix pollution.
//
// Implementation: delegates to `WorkspaceScopeRewriter` in the `cypher`
// module, which runs over a parsed AST. The rewriter understands clause
// boundaries, respects string literals, and scopes every UNION fragment
// independently — all the things the previous substring-based approach
// missed.
// ---------------------------------------------------------------------------

pub struct PropertyStrategy;

impl PropertyStrategy {
    /// The property name used for workspace isolation on graph nodes.
    pub const PROPERTY: &'static str = "_workspace_id";
    /// The Cypher parameter name bound at runtime (without $).
    pub const PARAM_NAME: &'static str = "_ws_id";
}

impl GraphIsolationStrategy for PropertyStrategy {
    fn scope_ast(&self, ast: CypherAst, workspace_id: &str) -> Result<ScopedAst, RewriteError> {
        let pipeline = CypherRewriterPipeline::new().with(WorkspaceScopeRewriter::new(
            Self::PROPERTY,
            Self::PARAM_NAME,
        ));
        let rewritten = pipeline.run_ast(ast, &RewriteContext::new(workspace_id))?;
        Ok(ScopedAst {
            ast: rewritten.ast,
            params: vec![(Self::PARAM_NAME.to_string(), workspace_id.to_string())],
            modified_statements: rewritten.modified_statements,
        })
    }

    fn name(&self) -> &str {
        "property"
    }

    fn scope_property(&self) -> Option<&'static str> {
        Some(Self::PROPERTY)
    }
}

// ---------------------------------------------------------------------------
// DatabaseStrategy — workspace isolation via separate Neo4j databases
// ---------------------------------------------------------------------------
// Enterprise/DozerDB only. Each workspace gets its own database.
// Queries are unmodified; isolation is at the connection level.
// ---------------------------------------------------------------------------

pub struct DatabaseStrategy;

impl GraphIsolationStrategy for DatabaseStrategy {
    fn scope_ast(&self, ast: CypherAst, _workspace_id: &str) -> Result<ScopedAst, RewriteError> {
        Ok(ScopedAst {
            ast,
            params: vec![],
            modified_statements: 0,
        })
    }

    fn name(&self) -> &str {
        "database"
    }

    fn scope_property(&self) -> Option<&'static str> {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cypher::parse;

    fn scope_text(strategy: &dyn GraphIsolationStrategy, query: &str, ws: &str) -> ScopedAst {
        strategy
            .scope_ast(parse(query), ws)
            .expect("built-in strategies never fail")
    }

    // --- PropertyStrategy scope tests ---

    #[test]
    fn scope_simple_read() {
        let strategy = PropertyStrategy;
        let result = scope_text(&strategy, "MATCH (n:Person) RETURN n", "ws-123");
        let rendered = result.ast.render();
        assert!(rendered.contains("WHERE n._workspace_id = $_ws_id"));
        assert_eq!(result.params.len(), 1);
        assert_eq!(
            result.params[0],
            ("_ws_id".to_string(), "ws-123".to_string())
        );
    }

    #[test]
    fn scope_read_with_existing_where() {
        let strategy = PropertyStrategy;
        let result = scope_text(
            &strategy,
            "MATCH (n:Person) WHERE n.age > 21 RETURN n",
            "ws-123",
        );
        assert!(
            result
                .ast
                .render()
                .contains("n._workspace_id = $_ws_id AND n.age > 21")
        );
    }

    #[test]
    fn scope_system_param_literal_is_not_double_injected() {
        // Idempotency pivots on the system-param shape (`= $_ws_id`),
        // not on the author's text. A prior pass that already stamped
        // the canonical predicate is detected and skipped.
        let strategy = PropertyStrategy;
        let query = "MATCH (n:Person) WHERE n._workspace_id = $_ws_id RETURN n";
        let result = scope_text(&strategy, query, "ws-123");
        assert_eq!(
            result.ast.render().matches("_workspace_id = $_ws_id").count(),
            1,
            "Should not double-inject the system predicate"
        );
    }

    #[test]
    fn scope_user_literal_is_and_neutralised() {
        // A user-supplied literal that would reach into another
        // workspace does not satisfy the idempotency guard. The
        // rewriter still injects its own `$_ws_id` predicate; the two
        // AND-combine, so the query evaluates to the empty set for any
        // literal that is not the active workspace.
        let strategy = PropertyStrategy;
        let query = "MATCH (n:Person) WHERE n._workspace_id = 'other_ws' RETURN n";
        let result = scope_text(&strategy, query, "ws-123");
        assert!(
            result.ast.render().contains(
                "n._workspace_id = $_ws_id AND n._workspace_id = 'other_ws'"
            ),
            "author literal must be AND-neutralised: {}",
            result.ast.render(),
        );
    }

    #[test]
    fn scope_simple_create() {
        let strategy = PropertyStrategy;
        let result = scope_text(&strategy, "CREATE (n:Person {name: 'Alice'})", "ws-123");
        assert!(
            result
                .ast
                .render()
                .contains("SET n._workspace_id = $_ws_id")
        );
    }

    #[test]
    fn scope_create_with_existing_set() {
        let strategy = PropertyStrategy;
        let result = scope_text(
            &strategy,
            "CREATE (n:Person {name: 'Alice'}) SET n.age = 30",
            "ws-123",
        );
        let rendered = result.ast.render();
        assert!(rendered.contains("n._workspace_id = $_ws_id"));
        assert!(rendered.contains("n.age = 30"));
    }

    #[test]
    fn scope_no_double_inject_write() {
        let strategy = PropertyStrategy;
        let query = "CREATE (n:Person) SET n._workspace_id = 'existing'";
        let result = scope_text(&strategy, query, "ws-123");
        assert_eq!(
            result.ast.render().matches("_workspace_id").count(),
            1,
            "Should not double-inject SET clause"
        );
    }

    #[test]
    fn scope_mixed_match_create() {
        let strategy = PropertyStrategy;
        let query = "MATCH (n:Person) CREATE (m:Company {name: 'Acme'}) CREATE (n)-[:WORKS_AT]->(m) RETURN m";
        let result = scope_text(&strategy, query, "ws-123");
        let rendered = result.ast.render();
        assert!(
            rendered.contains("WHERE n._workspace_id = $_ws_id"),
            "Mixed query should have WHERE filter: {rendered}"
        );
        assert!(
            rendered.contains("SET m._workspace_id = $_ws_id"),
            "Mixed query should have SET clause: {rendered}"
        );
    }

    #[test]
    fn scope_merge_pattern() {
        let strategy = PropertyStrategy;
        let result = scope_text(&strategy, "MERGE (n:Person {name: 'Alice'})", "ws-123");
        assert!(
            result
                .ast
                .render()
                .contains("SET n._workspace_id = $_ws_id")
        );
    }

    #[test]
    fn scope_no_pattern_passthrough() {
        let strategy = PropertyStrategy;
        let result = scope_text(&strategy, "RETURN 1", "ws-123");
        assert_eq!(result.ast.render(), "RETURN 1");
    }

    // --- Multi-node pattern tests (CRITICAL) ---

    #[test]
    fn scope_multi_node_pattern() {
        // Every node variable bound by the pattern is scoped, with a
        // single WHERE following the complete pattern (pattern
        // boundaries are never split by injection).
        let strategy = PropertyStrategy;
        let result = scope_text(
            &strategy,
            "MATCH (p:Product)-[:MADE_BY]->(b:Brand) RETURN b.name AS brand, count(p) AS products",
            "ws-123",
        );
        let rendered = result.ast.render();
        assert!(
            rendered.contains("p._workspace_id = $_ws_id AND b._workspace_id = $_ws_id"),
            "both p and b must be scoped: {rendered}"
        );
        assert!(
            !rendered.contains("(p:Product) WHERE"),
            "WHERE must NOT split the pattern: {rendered}"
        );
    }

    #[test]
    fn scope_multi_node_with_existing_where() {
        let strategy = PropertyStrategy;
        let result = scope_text(
            &strategy,
            "MATCH (c:Customer)-[:PLACED]->(o:Order) WHERE o.status = 'delivered' RETURN c, o",
            "ws-123",
        );
        assert!(
            result.ast.render().contains(
                "c._workspace_id = $_ws_id AND o._workspace_id = $_ws_id AND o.status"
            ),
            "every bound variable scoped before the author WHERE body: {}",
            result.ast.render(),
        );
    }

    #[test]
    fn scope_three_node_chain() {
        let strategy = PropertyStrategy;
        let result = scope_text(
            &strategy,
            "MATCH (c:Customer)-[:PLACED]->(o:Order)-[:CONTAINS]->(p:Product) RETURN c.name, p.name",
            "ws-123",
        );
        let rendered = result.ast.render();
        for var in ["c", "o", "p"] {
            assert!(
                rendered.contains(&format!("{var}._workspace_id = $_ws_id")),
                "chain variable {var} must be scoped: {rendered}"
            );
        }
    }

    // --- DatabaseStrategy tests ---

    #[test]
    fn database_strategy_passthrough() {
        let strategy = DatabaseStrategy;
        let result = scope_text(&strategy, "MATCH (n) RETURN n", "ws-123");
        assert_eq!(result.ast.render(), "MATCH (n) RETURN n");
        assert!(result.params.is_empty());
    }
}
