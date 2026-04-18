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

use crate::cypher::{CypherAst, CypherRewriterPipeline, RewriteContext, WorkspaceScopeRewriter};

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
    fn scope_ast(&self, ast: CypherAst, workspace_id: &str) -> ScopedAst;

    /// Strategy name (for logging and config).
    fn name(&self) -> &str;

    /// Property name that every graph-touching statement must textually
    /// reference after `scope_ast` runs, so a post-rewrite validator can
    /// gate isolation. `Some(prop)` for property-based strategies;
    /// `None` for connection-level strategies whose isolation is enforced
    /// outside the Cypher text (e.g. database-per-workspace).
    fn scope_property(&self) -> Option<&'static str>;
}

/// An AST with any bind-parameters the strategy introduced during rewriting.
///
/// `params` uses owned `String` keys so future strategies aren't
/// constrained to compile-time constants for parameter names (the
/// original `&'static str` worked for today's two strategies but
/// would break an ACL strategy that derives parameter names from
/// per-request policy).
pub struct ScopedAst {
    pub ast: CypherAst,
    pub params: Vec<(String, String)>,
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
    fn scope_ast(&self, ast: CypherAst, workspace_id: &str) -> ScopedAst {
        let pipeline = CypherRewriterPipeline::new()
            .with(WorkspaceScopeRewriter::new(Self::PROPERTY, Self::PARAM_NAME));
        let scoped = pipeline.run_ast(ast, &RewriteContext::new(workspace_id));
        ScopedAst {
            ast: scoped,
            params: vec![(Self::PARAM_NAME.to_string(), workspace_id.to_string())],
        }
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
    fn scope_ast(&self, ast: CypherAst, _workspace_id: &str) -> ScopedAst {
        ScopedAst {
            ast,
            params: vec![],
        }
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
        strategy.scope_ast(parse(query), ws)
    }

    // --- PropertyStrategy scope tests ---

    #[test]
    fn scope_simple_read() {
        let strategy = PropertyStrategy;
        let result = scope_text(&strategy, "MATCH (n:Person) RETURN n", "ws-123");
        let rendered = result.ast.render();
        assert!(rendered.contains("WHERE n._workspace_id = $_ws_id"));
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0], ("_ws_id".to_string(), "ws-123".to_string()));
    }

    #[test]
    fn scope_read_with_existing_where() {
        let strategy = PropertyStrategy;
        let result = scope_text(
            &strategy,
            "MATCH (n:Person) WHERE n.age > 21 RETURN n",
            "ws-123",
        );
        assert!(result.ast.render().contains("n._workspace_id = $_ws_id AND n.age > 21"));
    }

    #[test]
    fn scope_no_double_inject_read() {
        let strategy = PropertyStrategy;
        let query = "MATCH (n:Person) WHERE n._workspace_id = 'existing' RETURN n";
        let result = scope_text(&strategy, query, "ws-123");
        assert_eq!(
            result.ast.render().matches("_workspace_id").count(),
            1,
            "Should not double-inject WHERE filter"
        );
    }

    #[test]
    fn scope_simple_create() {
        let strategy = PropertyStrategy;
        let result = scope_text(&strategy, "CREATE (n:Person {name: 'Alice'})", "ws-123");
        assert!(result.ast.render().contains("SET n._workspace_id = $_ws_id"));
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
        assert!(result.ast.render().contains("SET n._workspace_id = $_ws_id"));
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
        let strategy = PropertyStrategy;
        let result = scope_text(
            &strategy,
            "MATCH (p:Product)-[:MADE_BY]->(b:Brand) RETURN b.name AS brand, count(p) AS products",
            "ws-123",
        );
        let rendered = result.ast.render();
        assert!(
            rendered.contains("(b:Brand) WHERE p._workspace_id = $_ws_id"),
            "WHERE should be after the full pattern: {rendered}"
        );
        assert!(
            !rendered.contains("(p:Product) WHERE"),
            "WHERE must NOT be after first node only: {rendered}"
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
            result.ast.render().contains("c._workspace_id = $_ws_id AND o.status"),
            "Should prepend to existing WHERE"
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
        assert!(
            result.ast.render().contains("(p:Product) WHERE c._workspace_id"),
            "WHERE after last node in chain"
        );
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
