// ---------------------------------------------------------------------------
// GraphIsolationStrategy — workspace isolation for graph databases
// ---------------------------------------------------------------------------
// PostgreSQL uses RLS (row-level security) for isolation.
// Graph databases need a different approach: property-based filtering
// on nodes/relationships, or (Enterprise only) separate databases.
// ---------------------------------------------------------------------------

/// Strategy for isolating graph data between workspaces.
///
/// Implementations transform Cypher queries to enforce workspace boundaries
/// at the graph layer. A single `scope` method handles both reads and writes
/// so that mixed queries (MATCH + CREATE) are correctly scoped.
pub trait GraphIsolationStrategy: Send + Sync {
    /// Inject workspace isolation into a Cypher query.
    ///
    /// Handles all query types:
    /// - MATCH patterns get WHERE filters (read isolation)
    /// - CREATE/MERGE patterns get SET clauses (write ownership)
    /// - Mixed queries get both transformations
    fn scope(&self, query: &str, workspace_id: &str) -> ScopedQuery;

    /// Strategy name (for logging and config).
    fn name(&self) -> &str;
}

/// A query with injected workspace isolation parameters.
pub struct ScopedQuery {
    pub query: String,
    pub params: Vec<(&'static str, String)>,
}

// ---------------------------------------------------------------------------
// PropertyStrategy — workspace isolation via node properties
// ---------------------------------------------------------------------------
// Community-compatible. Adds `_workspace_id` property to all nodes.
// MATCH patterns get WHERE filter; CREATE/MERGE get SET clause.
//
// Node labels remain semantically clean — no prefix pollution.
// ---------------------------------------------------------------------------

pub struct PropertyStrategy;

impl PropertyStrategy {
    /// The property name used for workspace isolation on graph nodes.
    pub const PROPERTY: &'static str = "_workspace_id";
    /// The Cypher parameter name bound at runtime (without $).
    pub const PARAM_NAME: &'static str = "_ws_id";
}

impl GraphIsolationStrategy for PropertyStrategy {
    fn scope(&self, query: &str, workspace_id: &str) -> ScopedQuery {
        let scoped = scope_cypher(query);
        ScopedQuery {
            query: scoped,
            params: vec![(Self::PARAM_NAME, workspace_id.to_string())],
        }
    }

    fn name(&self) -> &str {
        "property"
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
    fn scope(&self, query: &str, _workspace_id: &str) -> ScopedQuery {
        ScopedQuery {
            query: query.to_string(),
            params: vec![],
        }
    }

    fn name(&self) -> &str {
        "database"
    }
}

// ---------------------------------------------------------------------------
// Cypher rewriting — unified read + write scoping
// ---------------------------------------------------------------------------

/// Apply both read (WHERE filter) and write (SET clause) scoping to a query.
///
/// This handles mixed queries correctly by tracking which transformations
/// have been applied independently.
fn scope_cypher(query: &str) -> String {
    let mut result = query.to_string();

    // Phase 1: Inject WHERE filters for MATCH patterns (read isolation)
    if !has_workspace_filter(&result)
        && let Some(node_var) = extract_first_node_var(&result)
    {
        let ws_condition = format!(
            "{node_var}.{} = ${}",
            PropertyStrategy::PROPERTY,
            PropertyStrategy::PARAM_NAME
        );
        let upper = result.to_uppercase();

        if let Some(where_pos) = find_where_after_match(&upper) {
            let insert_pos = where_pos + 6; // "WHERE " is 6 chars
            result = format!(
                "{}{ws_condition} AND {}",
                &result[..insert_pos],
                &result[insert_pos..]
            );
        } else if let Some(match_end) = find_match_pattern_end(&result) {
            result = format!(
                "{} WHERE {ws_condition} {}",
                &result[..match_end],
                &result[match_end..]
            );
        }
    }

    // Phase 2: Inject SET clause for CREATE/MERGE patterns (write ownership)
    if !has_workspace_set(&result)
        && let Some(node_var) = extract_first_create_var(&result)
    {
        let set_fragment = format!(
            "{node_var}.{} = ${}",
            PropertyStrategy::PROPERTY,
            PropertyStrategy::PARAM_NAME
        );
        let upper = result.to_uppercase();

        if let Some(set_pos) = upper.find("SET ") {
            let insert_pos = set_pos + 4;
            result = format!(
                "{}{set_fragment}, {}",
                &result[..insert_pos],
                &result[insert_pos..]
            );
        } else if let Some(create_end) = find_create_pattern_end(&result) {
            result = format!(
                "{} SET {set_fragment}{}",
                &result[..create_end],
                &result[create_end..]
            );
        }
    }

    result
}

/// Check if a WHERE clause already contains a _workspace_id filter.
fn has_workspace_filter(query: &str) -> bool {
    let upper = query.to_uppercase();
    // Look for _workspace_id in a WHERE context (before CREATE/MERGE/SET)
    if let Some(where_pos) = upper.find("WHERE") {
        let after_where = &query[where_pos..];
        // Check if _workspace_id appears before the next major clause
        let end = ["CREATE", "MERGE", "SET ", "RETURN", "WITH "]
            .iter()
            .filter_map(|kw| after_where.to_uppercase().find(kw))
            .min()
            .unwrap_or(after_where.len());
        after_where[..end].contains(PropertyStrategy::PROPERTY)
    } else {
        false
    }
}

/// Check if a SET clause already contains a _workspace_id assignment.
fn has_workspace_set(query: &str) -> bool {
    let upper = query.to_uppercase();
    if let Some(set_pos) = upper.find("SET ") {
        let after_set = &query[set_pos..];
        after_set.contains(PropertyStrategy::PROPERTY)
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Simple Cypher parsing helpers
// ---------------------------------------------------------------------------

/// Extract the first node variable from a MATCH pattern.
/// e.g., `MATCH (n:Person)` → "n"
fn extract_first_node_var(query: &str) -> Option<String> {
    let upper = query.to_uppercase();
    let match_pos = upper.find("MATCH")?;
    let after_match = &query[match_pos + 5..];
    let paren_pos = after_match.find('(')?;
    let after_paren = &after_match[paren_pos + 1..];

    let var: String = after_paren
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if var.is_empty() { None } else { Some(var) }
}

/// Extract the first node variable from a CREATE/MERGE pattern.
fn extract_first_create_var(query: &str) -> Option<String> {
    let upper = query.to_uppercase();
    let pos = upper.find("CREATE").or_else(|| upper.find("MERGE"))?;
    let keyword_len = if upper[pos..].starts_with("CREATE") {
        6
    } else {
        5
    };
    let after = &query[pos + keyword_len..];
    let paren_pos = after.find('(')?;
    let after_paren = &after[paren_pos + 1..];

    let var: String = after_paren
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if var.is_empty() { None } else { Some(var) }
}

/// Find the position of WHERE keyword after the MATCH pattern ends.
/// Must search after the LAST `)` of the pattern, not the first.
fn find_where_after_match(upper_query: &str) -> Option<usize> {
    // Use find_match_pattern_end to locate where the pattern ends
    let pattern_end = find_match_pattern_end(upper_query)?;
    let after_pattern = &upper_query[pattern_end..];

    let where_offset = after_pattern.find("WHERE")?;
    let absolute_pos = pattern_end + where_offset;

    // WHERE must come before the next major keyword
    let next_keyword_pos = ["RETURN", "WITH ", "MATCH", "CREATE", "MERGE"]
        .iter()
        .filter_map(|kw| after_pattern.find(kw))
        .min()
        .unwrap_or(usize::MAX);

    if where_offset < next_keyword_pos {
        Some(absolute_pos)
    } else {
        None
    }
}

/// Find the end position of the MATCH pattern — the last `)` before the next
/// major Cypher keyword (WHERE, RETURN, WITH, CREATE, MERGE, SET, DELETE, ORDER).
///
/// Multi-node patterns like `MATCH (a)-[:REL]->(b)` have multiple `()` groups.
/// We must find the **last** closing paren of the entire path pattern, not the first.
fn find_match_pattern_end(query: &str) -> Option<usize> {
    let upper = query.to_uppercase();
    let match_pos = upper.find("MATCH")?;
    let after_match = &query[match_pos + 5..]; // skip "MATCH"
    let offset = match_pos + 5;

    // Find where the pattern ends: next major keyword preceded by whitespace.
    // Must check word boundary to avoid matching inside labels (e.g., "Order" vs "ORDER BY").
    let upper_after = after_match.to_uppercase();
    let boundary_keywords = [
        " WHERE ",
        " RETURN ",
        " WITH ",
        " CREATE ",
        " MERGE ",
        " SET ",
        " DELETE ",
        " ORDER ",
        " UNION ",
        " CALL ",
        "\nWHERE ",
        "\nRETURN ",
        "\nWITH ",
    ];
    let pattern_end = boundary_keywords
        .iter()
        .filter_map(|kw| upper_after.find(kw).map(|p| p + 1)) // +1 to skip leading space
        .min()
        .unwrap_or(after_match.len());

    // Find the last ')' within the pattern region
    let pattern_region = &after_match[..pattern_end];
    let last_close = pattern_region.rfind(')')?;
    Some(offset + last_close + 1)
}

/// Find the end position of the first CREATE/MERGE pattern (outermost closing paren).
fn find_create_pattern_end(query: &str) -> Option<usize> {
    let upper = query.to_uppercase();
    let pos = upper.find("CREATE").or_else(|| upper.find("MERGE"))?;
    let after = &query[pos..];

    let mut depth = 0;
    for (i, ch) in after.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos + i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- PropertyStrategy scope tests ---

    #[test]
    fn scope_simple_read() {
        let strategy = PropertyStrategy;
        let result = strategy.scope("MATCH (n:Person) RETURN n", "ws-123");
        assert!(result.query.contains("WHERE n._workspace_id = $_ws_id"));
        assert_eq!(result.params.len(), 1);
        assert_eq!(result.params[0], ("_ws_id", "ws-123".to_string()));
    }

    #[test]
    fn scope_read_with_existing_where() {
        let strategy = PropertyStrategy;
        let result = strategy.scope("MATCH (n:Person) WHERE n.age > 21 RETURN n", "ws-123");
        assert!(
            result
                .query
                .contains("n._workspace_id = $_ws_id AND n.age > 21")
        );
    }

    #[test]
    fn scope_no_double_inject_read() {
        let strategy = PropertyStrategy;
        let query = "MATCH (n:Person) WHERE n._workspace_id = 'existing' RETURN n";
        let result = strategy.scope(query, "ws-123");
        // Should not add another filter
        assert_eq!(
            result.query.matches("_workspace_id").count(),
            1,
            "Should not double-inject WHERE filter"
        );
    }

    #[test]
    fn scope_simple_create() {
        let strategy = PropertyStrategy;
        let result = strategy.scope("CREATE (n:Person {name: 'Alice'})", "ws-123");
        assert!(result.query.contains("SET n._workspace_id = $_ws_id"));
    }

    #[test]
    fn scope_create_with_existing_set() {
        let strategy = PropertyStrategy;
        let result = strategy.scope("CREATE (n:Person {name: 'Alice'}) SET n.age = 30", "ws-123");
        assert!(result.query.contains("n._workspace_id = $_ws_id"));
        assert!(result.query.contains("n.age = 30"));
    }

    #[test]
    fn scope_no_double_inject_write() {
        let strategy = PropertyStrategy;
        let query = "CREATE (n:Person) SET n._workspace_id = 'existing'";
        let result = strategy.scope(query, "ws-123");
        assert_eq!(
            result.query.matches("_workspace_id").count(),
            1,
            "Should not double-inject SET clause"
        );
    }

    #[test]
    fn scope_mixed_match_create() {
        let strategy = PropertyStrategy;
        let query = "MATCH (n:Person) CREATE (m:Company {name: 'Acme'}) CREATE (n)-[:WORKS_AT]->(m) RETURN m";
        let result = strategy.scope(query, "ws-123");
        // Should have BOTH WHERE filter for MATCH and SET for CREATE
        assert!(
            result.query.contains("WHERE n._workspace_id = $_ws_id"),
            "Mixed query should have WHERE filter: {}",
            result.query
        );
        assert!(
            result.query.contains("SET m._workspace_id = $_ws_id"),
            "Mixed query should have SET clause: {}",
            result.query
        );
    }

    #[test]
    fn scope_merge_pattern() {
        let strategy = PropertyStrategy;
        let result = strategy.scope("MERGE (n:Person {name: 'Alice'})", "ws-123");
        assert!(result.query.contains("SET n._workspace_id = $_ws_id"));
    }

    #[test]
    fn scope_no_pattern_passthrough() {
        let strategy = PropertyStrategy;
        let result = strategy.scope("RETURN 1", "ws-123");
        assert_eq!(result.query, "RETURN 1");
    }

    // --- Multi-node pattern tests (CRITICAL) ---

    #[test]
    fn scope_multi_node_pattern() {
        let strategy = PropertyStrategy;
        let result = strategy.scope(
            "MATCH (p:Product)-[:MADE_BY]->(b:Brand) RETURN b.name AS brand, count(p) AS products",
            "ws-123",
        );
        // WHERE must be AFTER the entire pattern, not in the middle
        assert!(
            result
                .query
                .contains("(b:Brand) WHERE p._workspace_id = $_ws_id"),
            "WHERE should be after the full pattern: {}",
            result.query,
        );
        assert!(
            !result.query.contains("(p:Product) WHERE"),
            "WHERE must NOT be after first node only: {}",
            result.query,
        );
    }

    #[test]
    fn scope_multi_node_with_existing_where() {
        let strategy = PropertyStrategy;
        let result = strategy.scope(
            "MATCH (c:Customer)-[:PLACED]->(o:Order) WHERE o.status = 'delivered' RETURN c, o",
            "ws-123",
        );
        assert!(
            result
                .query
                .contains("c._workspace_id = $_ws_id AND o.status"),
            "Should prepend to existing WHERE: {}",
            result.query,
        );
    }

    #[test]
    fn scope_three_node_chain() {
        let strategy = PropertyStrategy;
        let result = strategy.scope(
            "MATCH (c:Customer)-[:PLACED]->(o:Order)-[:CONTAINS]->(p:Product) RETURN c.name, p.name",
            "ws-123",
        );
        assert!(
            result.query.contains("(p:Product) WHERE c._workspace_id"),
            "WHERE after last node in chain: {}",
            result.query,
        );
    }

    // --- DatabaseStrategy tests ---

    #[test]
    fn database_strategy_passthrough() {
        let strategy = DatabaseStrategy;
        let result = strategy.scope("MATCH (n) RETURN n", "ws-123");
        assert_eq!(result.query, "MATCH (n) RETURN n");
        assert!(result.params.is_empty());
    }

    // --- Parsing helper tests ---

    #[test]
    fn extract_node_var_standard() {
        assert_eq!(
            extract_first_node_var("MATCH (n:Person)"),
            Some("n".to_string())
        );
        assert_eq!(
            extract_first_node_var("MATCH (node)"),
            Some("node".to_string())
        );
        assert_eq!(
            extract_first_node_var("MATCH (my_var:Label)"),
            Some("my_var".to_string())
        );
    }

    #[test]
    fn extract_node_var_none() {
        assert_eq!(extract_first_node_var("RETURN 1"), None);
        assert_eq!(extract_first_node_var("CREATE (n)"), None); // MATCH not found
    }

    #[test]
    fn extract_create_var_standard() {
        assert_eq!(
            extract_first_create_var("CREATE (n:Person)"),
            Some("n".to_string())
        );
        assert_eq!(
            extract_first_create_var("MERGE (m:Company)"),
            Some("m".to_string())
        );
    }

    #[test]
    fn find_where_position() {
        let q = "MATCH (N:PERSON) WHERE N.AGE > 21 RETURN N";
        assert!(find_where_after_match(q).is_some());
    }

    #[test]
    fn find_match_end() {
        let q = "MATCH (n:Person) RETURN n";
        let end = find_match_pattern_end(q).unwrap();
        assert_eq!(&q[..end], "MATCH (n:Person)");
    }

    #[test]
    fn find_create_end() {
        let q = "CREATE (n:Person {name: 'Alice'}) RETURN n";
        let end = find_create_pattern_end(q).unwrap();
        assert_eq!(&q[..end], "CREATE (n:Person {name: 'Alice'})");
    }
}
