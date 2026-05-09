use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::graph_label::GraphLabel;
use ox_core::property_key::PropertyKey;
use ox_core::types::{Direction, PropertyValue};
use ox_core::variable_name::VariableName;

// ---------------------------------------------------------------------------
// GraphFunction — typed function registry
//
// Replaces `function: String` with a closed enum so the compiler can
// check backend support at IR level rather than failing at Cypher emit.
// ---------------------------------------------------------------------------

/// Named graph functions. Each variant maps to a specific Cypher
/// function via [`GraphFunction::cypher_name`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GraphFunction {
    // -- Aggregation --
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Collect,
    // -- String --
    ToUpper,
    ToLower,
    Trim,
    Replace,
    Substring,
    // -- Numeric --
    Abs,
    Ceil,
    Floor,
    Round,
    Sign,
    Rand,
    // -- List / Collection --
    Size,
    Head,
    Last,
    Range,
    Keys,
    // -- Graph introspection --
    Labels,
    Type,
    Id,
    Nodes,
    Relationships,
    Length,
    // -- Type conversion --
    ToString,
    ToInteger,
    ToFloat,
    ToBoolean,
    // -- Null handling --
    Coalesce,
    // -- Date/Time --
    Date,
    #[serde(alias = "datetime")]
    DateTime,
    #[serde(alias = "duration")]
    DurationFn,
}

impl GraphFunction {
    /// The Cypher function name emitted by the compiler.
    pub fn cypher_name(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Min => "min",
            Self::Max => "max",
            Self::Collect => "collect",
            Self::ToUpper => "toUpper",
            Self::ToLower => "toLower",
            Self::Trim => "trim",
            Self::Replace => "replace",
            Self::Substring => "substring",
            Self::Abs => "abs",
            Self::Ceil => "ceil",
            Self::Floor => "floor",
            Self::Round => "round",
            Self::Sign => "sign",
            Self::Rand => "rand",
            Self::Size => "size",
            Self::Head => "head",
            Self::Last => "last",
            Self::Range => "range",
            Self::Keys => "keys",
            Self::Labels => "labels",
            Self::Type => "type",
            Self::Id => "id",
            Self::Nodes => "nodes",
            Self::Relationships => "relationships",
            Self::Length => "length",
            Self::ToString => "toString",
            Self::ToInteger => "toInteger",
            Self::ToFloat => "toFloat",
            Self::ToBoolean => "toBoolean",
            Self::Coalesce => "coalesce",
            Self::Date => "date",
            Self::DateTime => "datetime",
            Self::DurationFn => "duration",
        }
    }
}

impl std::fmt::Display for GraphFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.cypher_name())
    }
}

// ---------------------------------------------------------------------------
// QueryIR — DB-agnostic graph query algebra
//
// Models the full range of graph query operations without binding to
// any specific query language (Cypher, Gremlin, GQL).
//
// Architecture follows the principle:
//   "Pattern matching turns a graph into a table;
//    the remaining operations manipulate that table."
//
// Compiles to:
//   Neo4j   → Cypher (MATCH ... WHERE ... RETURN)
//   Neptune → openCypher (subset, path ops may use Gremlin)
//   GQL     → ISO GQL (MATCH ... FILTER ... RETURN)
// ---------------------------------------------------------------------------

/// Current on-wire schema version for `QueryIR` JSONB. See
/// [`ox_ontology::ir::ONTOLOGY_IR_SCHEMA_VERSION`] for the versioning
/// rationale — the same contract applies: bump on incompatible shape
/// change, deserialisation rejects higher values.
// Bumped to 2 with the addition of `QueryOp::HybridSearch` —
// adding a tagged-enum variant is a breaking on-wire change for
// any reader on schema_version=1 (the strict serde_json
// deserialise rejects unknown `op` tags). Persisted
// `saved_query_patterns` remained Match-shape, so the upgrade
// is safe at write time.
pub const QUERY_IR_SCHEMA_VERSION: u32 = 2;

fn default_query_ir_schema_version() -> u32 {
    QUERY_IR_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct QueryIR {
    /// On-wire struct shape version. Future incompatible layout changes
    /// bump it; deserialisation rejects newer values so a stale server
    /// never silently drops fields a newer writer added.
    #[serde(default = "default_query_ir_schema_version")]
    pub schema_version: u32,
    /// The core query operation
    #[schema(no_recursion)]
    pub operation: QueryOp,
    /// LIMIT clause
    pub limit: Option<usize>,
    /// SKIP clause
    pub skip: Option<usize>,
    /// ORDER BY clauses
    pub order_by: Vec<OrderClause>,
    /// Temporal AS-OF pivot.
    ///
    /// When `Some`, the query evaluates against the ontology version
    /// that was live at `as_of`, not the latest committed schema. Lets
    /// Foundry-style "show me how this dashboard looked last Tuesday"
    /// workflows round-trip through a single saved PatternIR — the UI
    /// toggles the timestamp, the compiler consults the corresponding
    /// [`ox_ontology::ir::OntologyVersion`] snapshot, and the live
    /// graph data is filtered by the same instant.
    ///
    /// Compiler support is deferred to a dedicated Temporal pass; today
    /// the runtime rejects any `as_of`-tagged QueryIR with a clear
    /// `OxError::Compilation("temporal queries not yet supported")`.
    /// The field is additive — every existing call site keeps compiling
    /// because `serde(default)` and `Default` both collapse to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
}

impl QueryIR {
    /// Canonical hash fit for the quality-signal dashboard's
    /// reproducibility metric. Strips non-plan fields (`as_of`,
    /// `schema_version`) so two runs of the same logical question
    /// hash identically even when the call-time clock / IR-schema
    /// drifts. Uses stable-key JSON serialisation then sha256.
    ///
    /// `as_of` is excluded because the metric answers "did the same
    /// question hit the same plan?" — AS-OF is a per-call parameter,
    /// not part of the question.
    ///
    /// `schema_version` is excluded because an IR schema bump on
    /// the server doesn't make yesterday's query a different
    /// question. Fields the compiler actually reads (operation,
    /// limit/skip, order_by) are kept.
    pub fn canonical_hash(&self) -> String {
        use sha2::{Digest, Sha256};

        // Build a stable-key JSON value from only the identity-bearing
        // fields. `serde_json::to_string` on serde-derived types is
        // already stable-key for structs; `as_of` + `schema_version`
        // are dropped so volatile-but-irrelevant noise can't flip the
        // hash.
        let canonical = serde_json::json!({
            "operation": &self.operation,
            "limit": self.limit,
            "skip": self.skip,
            "order_by": &self.order_by,
        });
        let serialized = serde_json::to_string(&canonical).unwrap_or_default();

        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        let digest = hasher.finalize();
        // Hex — human-readable in logs, fits in a `TEXT` column
        // without base64 padding.
        let mut out = String::with_capacity(64);
        for byte in digest.iter() {
            // `write_fmt` to a `String` buffer is infallible.
            #[allow(clippy::let_underscore_must_use)]
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
        }
        out
    }

    /// Validate structural integrity of the QueryIR.
    ///
    /// Catches issues that would cause Cypher compilation failures:
    /// - Match operations with no patterns.
    /// - `CallSubquery.import_variables` / `Expr::Subquery.import_variables`
    ///   referencing variables that are not in scope at the call site —
    ///   Neo4j rejects these at parse time with a cryptic stack trace; we
    ///   catch them at IR validation with a full list of available
    ///   variables instead.
    pub fn validate(&self) -> Result<(), ox_core::error::OxError> {
        let mut scope: HashSet<String> = HashSet::new();
        Self::validate_op(&self.operation, &mut scope)
    }

    /// Walk the operation tree, threading a mutable `scope` of variables
    /// that have been declared up to this point. The scope is used to
    /// validate that every `CallSubquery` / `Expr::Subquery` import lists
    /// names that are actually available — inner subqueries receive a
    /// fresh scope seeded with their import list.
    fn validate_op(
        op: &QueryOp,
        scope: &mut HashSet<String>,
    ) -> Result<(), ox_core::error::OxError> {
        match op {
            QueryOp::Match {
                patterns, filter, ..
            } => {
                if patterns.is_empty() {
                    return Err(ox_core::error::OxError::Validation {
                        field: "patterns".into(),
                        message: "Match operation must have at least one pattern".into(),
                    });
                }
                for p in patterns {
                    collect_pattern_vars(p, scope);
                }
                // Filter expressions may contain Subquery — walk them.
                if let Some(expr) = filter {
                    validate_expr_scope(expr, scope)?;
                }
                Ok(())
            }
            QueryOp::PathFind { start, end, .. } => {
                scope.insert(start.variable.to_string());
                scope.insert(end.variable.to_string());
                Ok(())
            }
            QueryOp::Aggregate { source, .. } => Self::validate_op(&source.operation, scope),
            QueryOp::Union { queries, .. } => {
                // UNION branches are independent — each starts from the
                // outer scope, with their own additions discarded afterwards.
                for q in queries {
                    let mut branch_scope = scope.clone();
                    Self::validate_op(&q.operation, &mut branch_scope)?;
                }
                Ok(())
            }
            QueryOp::Chain { steps } => {
                for s in steps {
                    Self::validate_op(&s.operation, scope)?;
                }
                Ok(())
            }
            QueryOp::CallSubquery {
                inner,
                import_variables,
            } => {
                for imported in import_variables {
                    if !scope.contains(imported) {
                        return Err(ox_core::error::OxError::Validation {
                            field: "import_variables".into(),
                            message: format!(
                                "CallSubquery imports undeclared variable `{imported}`. \
                                 Available in scope: {}",
                                fmt_scope(scope)
                            ),
                        });
                    }
                }
                let mut sub_scope: HashSet<String> = import_variables.iter().cloned().collect();
                Self::validate_op(&inner.operation, &mut sub_scope)
            }
            QueryOp::HybridSearch { request } => {
                // Hybrid retrieval doesn't introduce pattern
                // variables — its result set is a ranked node
                // list, projected by the planner. Validate the
                // payload's dim invariant here; the runtime
                // never reaches the index reader with an
                // inconsistent vector.
                if !request.vector_query.dimension_consistent() {
                    return Err(ox_core::error::OxError::Validation {
                        field: "vector_query".into(),
                        message: format!(
                            "HybridSearch vector_query has inconsistent dim \
                             (vec.len()={} vs declared dim={})",
                            request.vector_query.vector.len(),
                            request.vector_query.dim,
                        ),
                    });
                }
                Ok(())
            }
            QueryOp::Mutate {
                context,
                operations,
                ..
            } => {
                if let Some(ctx) = context {
                    Self::validate_op(ctx, scope)?;
                }
                for mut_op in operations {
                    match mut_op {
                        MutateOp::CreateNode { variable, .. }
                        | MutateOp::MergeNode { variable, .. } => {
                            scope.insert(variable.to_string());
                        }
                        MutateOp::CreateEdge { variable, .. }
                        | MutateOp::MergeEdge { variable, .. } => {
                            if let Some(v) = variable {
                                scope.insert(v.to_string());
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            QueryOp::Analytics { .. } => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Scope helpers
// ---------------------------------------------------------------------------

fn collect_pattern_vars(pattern: &GraphPattern, scope: &mut HashSet<String>) {
    match pattern {
        GraphPattern::Node { variable, .. } => {
            scope.insert(variable.to_string());
        }
        GraphPattern::Relationship {
            variable,
            source,
            target,
            ..
        } => {
            if let Some(v) = variable {
                scope.insert(v.to_string());
            }
            scope.insert(source.to_string());
            scope.insert(target.to_string());
        }
        GraphPattern::Path { elements } => {
            for elem in elements {
                match elem {
                    PathElement::Node { variable, .. } => {
                        scope.insert(variable.to_string());
                    }
                    PathElement::Edge { variable, .. } => {
                        if let Some(v) = variable {
                            scope.insert(v.to_string());
                        }
                    }
                }
            }
        }
    }
}

fn validate_expr_scope(
    expr: &Expr,
    scope: &HashSet<String>,
) -> Result<(), ox_core::error::OxError> {
    match expr {
        Expr::Subquery {
            query,
            import_variables,
        } => {
            for imported in import_variables {
                if !scope.contains(imported) {
                    return Err(ox_core::error::OxError::Validation {
                        field: "import_variables".into(),
                        message: format!(
                            "Expr::Subquery imports undeclared variable `{imported}`. \
                             Available in scope: {}",
                            fmt_scope(scope)
                        ),
                    });
                }
            }
            let mut sub_scope: HashSet<String> = import_variables.iter().cloned().collect();
            QueryIR::validate_op(&query.operation, &mut sub_scope)
        }
        Expr::Comparison { left, right, .. } | Expr::StringOp { left, right, .. } => {
            validate_expr_scope(left, scope)?;
            validate_expr_scope(right, scope)
        }
        Expr::Logical { left, right, .. } => {
            validate_expr_scope(left, scope)?;
            validate_expr_scope(right, scope)
        }
        Expr::Not { inner } => validate_expr_scope(inner, scope),
        Expr::In { expr, .. } => validate_expr_scope(expr, scope),
        Expr::IsNull { expr, .. } => validate_expr_scope(expr, scope),
        Expr::FunctionCall { args, .. } => {
            for a in args {
                validate_expr_scope(a, scope)?;
            }
            Ok(())
        }
        Expr::Case {
            operand,
            when_clauses,
            else_result,
        } => {
            if let Some(op) = operand {
                validate_expr_scope(op, scope)?;
            }
            for w in when_clauses {
                validate_expr_scope(&w.condition, scope)?;
                validate_expr_scope(&w.result, scope)?;
            }
            if let Some(e) = else_result {
                validate_expr_scope(e, scope)?;
            }
            Ok(())
        }
        // Literal, Param, Property, Exists — no inner Subquery reachable from here.
        Expr::Literal { .. } | Expr::Param { .. } | Expr::Property { .. } | Expr::Exists { .. } => {
            Ok(())
        }
    }
}

/// Human-readable scope listing for validation-error messages.
/// Deterministic so tests can assert on the exact text.
fn fmt_scope(scope: &HashSet<String>) -> String {
    let mut names: Vec<&String> = scope.iter().collect();
    names.sort();
    if names.is_empty() {
        "<empty>".to_string()
    } else {
        names
            .iter()
            .map(|s| format!("`{s}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// ---------------------------------------------------------------------------
// Graph analytics — backend-agnostic algorithm descriptors
// ---------------------------------------------------------------------------

/// Graph algorithm type — backend-agnostic.
/// Each compiler maps these to native calls (e.g., Neo4j GDS, Neptune Analytics).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphAlgorithm {
    PageRank,
    CommunityDetection,
    BetweennessCentrality,
    ShortestPath,
    NodeSimilarity,
}

/// Source for analytics computation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalyticsSource {
    /// Run on all nodes/edges in the graph
    WholeGraph,
    /// Run on nodes with specific labels
    Labels { labels: Vec<GraphLabel> },
    /// Run on a filtered subgraph
    #[schema(no_recursion)]
    Subgraph { filter: Box<QueryOp> },
}

// ---------------------------------------------------------------------------
// QueryOp — top-level query operations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueryOp {
    /// Pattern matching with optional filtering and projection
    #[schema(no_recursion)]
    Match {
        patterns: Vec<GraphPattern>,
        filter: Option<Expr>,
        projections: Vec<Projection>,
        optional: bool,
        #[serde(default)]
        group_by: Vec<Projection>,
    },

    /// Path finding between two nodes
    /// Separate from Match because:
    /// - Neo4j: compiles to shortestPath() within MATCH
    /// - Neptune: may need Gremlin traversal (openCypher lacks shortestPath)
    PathFind {
        start: NodeRef,
        end: NodeRef,
        edge_types: Vec<GraphLabel>,
        direction: Direction,
        max_depth: Option<usize>,
        algorithm: PathAlgorithm,
    },

    /// Aggregation over a sub-query
    #[schema(no_recursion)]
    Aggregate {
        source: Box<QueryIR>,
        group_by: Vec<FieldRef>,
        aggregations: Vec<AggregationExpr>,
        /// HAVING-style filter applied after aggregation.
        ///
        /// Cypher has no HAVING keyword — this compiles to an extra
        /// `WITH <group-by aliases>, <aggregation aliases> WHERE <expr>`
        /// chained after the aggregation step. The expression can
        /// reference aggregation result aliases (e.g. `order_count > 10`)
        /// or group-by columns; it cannot re-reference raw pattern
        /// variables that were projected away.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        having: Option<Expr>,
    },

    /// UNION of multiple queries
    #[schema(no_recursion)]
    Union { queries: Vec<QueryIR>, all: bool },

    /// Sequential chaining (WITH ... MATCH ...)
    /// Allows intermediate result passing between query steps
    #[schema(no_recursion)]
    Chain { steps: Vec<ChainStep> },

    /// Subquery as a query step — runs a nested query inline.
    /// Compiles to Cypher CALL { WITH ... }
    #[schema(no_recursion)]
    CallSubquery {
        inner: Box<QueryIR>,
        /// Variables to pass from outer to inner scope
        import_variables: Vec<String>,
    },

    /// Mutation operations (CREATE, MERGE, DELETE, SET)
    #[schema(no_recursion)]
    Mutate {
        /// Optional preceding MATCH for context
        context: Option<Box<QueryOp>>,
        operations: Vec<MutateOp>,
        returning: Vec<Projection>,
    },

    /// Run a graph analytics algorithm.
    /// Each compiler maps this to backend-native calls (e.g., Neo4j GDS).
    #[schema(no_recursion)]
    Analytics {
        algorithm: GraphAlgorithm,
        source: AnalyticsSource,
        params: HashMap<String, Expr>,
        projections: Vec<Projection>,
    },

    /// Hybrid retrieval — vector kNN ⊕ full-text BM25 ⊕ optional
    /// graph-traversal predicate, fused via Reciprocal Rank
    /// Fusion (default) or weighted sum. The GraphRAG path the
    /// platform exposes for question-answer retrieval — unlike
    /// `Match`, the result set isn't pattern-shaped: it's a
    /// ranked list of nodes scored against the query embedding.
    ///
    /// Per-engine emission:
    /// - **Neo4j 5+**: lowers to
    ///   `db.index.vector.queryNodes` + `db.index.fulltext.queryNodes`
    ///   plus a Cypher fusion CTE.
    /// - **Memgraph**: lowers to `vector_search.search` +
    ///   `text_search.search` plus a fusion CTE.
    ///
    /// The `request` payload's `Embedding` carries the model id
    /// so the dispatcher can route the vector to the matching-
    /// dimension index — querying a 1536-dim vector against a
    /// 384-dim index is a typed compile-time error caught by
    /// the validator.
    #[schema(no_recursion)]
    HybridSearch {
        request: crate::hybrid_retrieval::HybridSearchRequest,
    },
}

// ---------------------------------------------------------------------------
// GraphPattern — describes a subgraph pattern to match
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphPattern {
    /// A single node: (variable:Label {props})
    #[schema(no_recursion)]
    Node {
        variable: VariableName,
        label: Option<GraphLabel>,
        property_filters: Vec<PropertyFilter>,
    },

    /// A relationship between two nodes:
    /// (source)-[variable:Label]->(target)
    #[schema(no_recursion)]
    Relationship {
        variable: Option<VariableName>,
        label: Option<GraphLabel>,
        source: VariableName,
        target: VariableName,
        direction: Direction,
        property_filters: Vec<PropertyFilter>,
        /// Variable-length path: *min..max
        var_length: Option<VarLength>,
    },

    /// A complete path pattern: (a)-[r1]->(b)-[r2]->(c)
    Path { elements: Vec<PathElement> },
}

/// Inline property filter within a pattern: {name: "Alice", age: 30}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct PropertyFilter {
    pub property: PropertyKey,
    #[schema(no_recursion)]
    pub value: Expr,
}

/// Variable-length path specification
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct VarLength {
    pub min: Option<usize>,
    pub max: Option<usize>,
}

/// A single element in a path pattern
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathElement {
    Node {
        variable: VariableName,
        label: Option<GraphLabel>,
    },
    Edge {
        variable: Option<VariableName>,
        label: Option<GraphLabel>,
        direction: Direction,
    },
}

// ---------------------------------------------------------------------------
// Expr — filter/where expressions (DB-agnostic expression tree)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "expr_type", rename_all = "snake_case")]
pub enum Expr {
    /// A literal value
    Literal { value: PropertyValue },

    /// A named parameter placeholder: `$name` in Cypher.
    ///
    /// Used for caller-supplied values that vary across executions of the
    /// same compiled query — the query plan stays warm in the driver's
    /// cache, and only the parameter values change per run. Compiles to
    /// a `$name` reference without entering the auto-generated parameter
    /// table, so the caller must bind the value at execution time (Neo4j
    /// fails loud with a missing-parameter error if not).
    ///
    /// The compiler rejects a name that doesn't match
    /// `is_valid_graph_identifier` to prevent Cypher injection via the
    /// parameter surface. Distinct from `Literal` (inline value baked
    /// into the plan) and from the compiler's internal `$p0` / `$p1`
    /// autogenerated params (opaque — caller can't change them).
    Param { name: String },

    /// A property reference: variable.field (field is None when referencing the whole variable)
    Property {
        variable: VariableName,
        #[serde(default)]
        field: Option<PropertyKey>,
    },

    /// Binary comparison: left op right
    #[schema(no_recursion)]
    Comparison {
        left: Box<Expr>,
        op: ComparisonOp,
        right: Box<Expr>,
    },

    /// Logical combination: left AND/OR right
    #[schema(no_recursion)]
    Logical {
        left: Box<Expr>,
        op: LogicalOp,
        right: Box<Expr>,
    },

    /// Negation: NOT expr
    #[schema(no_recursion)]
    Not { inner: Box<Expr> },

    /// IN check: expr IN [values]
    #[schema(no_recursion)]
    In {
        expr: Box<Expr>,
        values: Vec<PropertyValue>,
    },

    /// NULL check: expr IS NULL / IS NOT NULL
    #[schema(no_recursion)]
    IsNull { expr: Box<Expr>, negated: bool },

    /// String operations: STARTS WITH, ENDS WITH, CONTAINS
    #[schema(no_recursion)]
    StringOp {
        left: Box<Expr>,
        op: StringOp,
        right: Box<Expr>,
    },

    /// Function call: function(args)
    #[serde(alias = "function")]
    #[schema(no_recursion)]
    FunctionCall {
        #[serde(alias = "name")]
        function: GraphFunction,
        #[serde(default)]
        args: Vec<Expr>,
    },

    /// Pattern existence check: EXISTS { (a)-[:KNOWS]->(b) }
    #[schema(no_recursion)]
    Exists { pattern: Box<GraphPattern> },

    /// CASE expression: simple or searched
    #[schema(no_recursion)]
    Case {
        /// Optional operand for simple CASE (CASE expr WHEN ...)
        operand: Option<Box<Expr>>,
        /// WHEN condition THEN result pairs
        when_clauses: Vec<WhenClause>,
        /// ELSE result (defaults to null if omitted)
        else_result: Option<Box<Expr>>,
    },

    /// Subquery expression — evaluates a nested query and returns scalar/list result.
    /// Compiles to Cypher COUNT { ... } or CALL { ... } depending on context.
    #[schema(no_recursion)]
    Subquery {
        query: Box<QueryIR>,
        /// Variables from outer scope to import into subquery
        import_variables: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct WhenClause {
    #[schema(no_recursion)]
    pub condition: Expr,
    #[schema(no_recursion)]
    pub result: Expr,
}

/// Custom deserializer for Expr: handles common LLM output variations.
///
/// 1. Accepts "expr_type", "type", or "kind" as the discriminator key.
/// 2. When the discriminator value is a PropertyValue type name ("string", "int",
///    "float", "bool", "boolean", "null"), treats the whole object as
///    Expr::Literal { value: PropertyValue }.
///    LLMs sometimes emit `{"type": "string", "value": "Alice"}` (a PropertyValue)
///    where `{"expr_type": "literal", "value": {"type": "string", "value": "Alice"}}`
///    is expected.
impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = serde_json::Map::deserialize(deserializer)?;

        // Normalize discriminator: accept "type" or "kind" as aliases for "expr_type"
        if !map.contains_key("expr_type")
            && let Some(val) = map.remove("type").or_else(|| map.remove("kind"))
        {
            map.insert("expr_type".to_string(), val);
        }

        // Detect PropertyValue masquerading as Expr:
        // If expr_type is a PropertyValue type name, wrap the entire object as Expr::Literal.
        if let Some(
            "string" | "int" | "float" | "bool" | "boolean" | "null" | "date" | "date_time"
            | "duration" | "bytes" | "list" | "map",
        ) = map.get("expr_type").and_then(|v| v.as_str())
        {
            // This is a PropertyValue, not an Expr variant.
            // Reconstruct as {"type": <tag>, "value": <value>} for PropertyValue deser.
            // `map.get("expr_type")` just matched above, so `remove` yields Some;
            // we still guard defensively because that guarantee lives in a
            // pattern match, not in the types.
            if let Some(pv_type) = map.remove("expr_type") {
                map.insert("type".to_string(), pv_type);
            }
            let pv_value = serde_json::Value::Object(map);
            let pv: PropertyValue =
                serde_json::from_value(pv_value).map_err(serde::de::Error::custom)?;
            return Ok(Expr::Literal { value: pv });
        }

        let value = serde_json::Value::Object(map);

        #[derive(Deserialize)]
        #[serde(tag = "expr_type", rename_all = "snake_case")]
        enum ExprInner {
            Literal {
                value: PropertyValue,
            },
            Param {
                name: String,
            },
            Property {
                variable: VariableName,
                #[serde(default)]
                field: Option<PropertyKey>,
            },
            Comparison {
                left: Box<Expr>,
                op: ComparisonOp,
                right: Box<Expr>,
            },
            Logical {
                left: Box<Expr>,
                op: LogicalOp,
                right: Box<Expr>,
            },
            Not {
                inner: Box<Expr>,
            },
            In {
                expr: Box<Expr>,
                values: Vec<PropertyValue>,
            },
            IsNull {
                expr: Box<Expr>,
                negated: bool,
            },
            StringOp {
                left: Box<Expr>,
                op: StringOp,
                right: Box<Expr>,
            },
            #[serde(alias = "function")]
            FunctionCall {
                #[serde(alias = "name")]
                function: GraphFunction,
                #[serde(default)]
                args: Vec<Expr>,
            },
            Exists {
                pattern: Box<GraphPattern>,
            },
            Case {
                operand: Option<Box<Expr>>,
                when_clauses: Vec<WhenClause>,
                else_result: Option<Box<Expr>>,
            },
            Subquery {
                query: Box<QueryIR>,
                import_variables: Vec<String>,
            },
        }

        let inner: ExprInner = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(match inner {
            ExprInner::Literal { value } => Expr::Literal { value },
            ExprInner::Param { name } => Expr::Param { name },
            ExprInner::Property { variable, field } => Expr::Property { variable, field },
            ExprInner::Comparison { left, op, right } => Expr::Comparison { left, op, right },
            ExprInner::Logical { left, op, right } => Expr::Logical { left, op, right },
            ExprInner::Not { inner } => Expr::Not { inner },
            ExprInner::In { expr, values } => Expr::In { expr, values },
            ExprInner::IsNull { expr, negated } => Expr::IsNull { expr, negated },
            ExprInner::StringOp { left, op, right } => Expr::StringOp { left, op, right },
            ExprInner::FunctionCall { function, args } => Expr::FunctionCall { function, args },
            ExprInner::Exists { pattern } => Expr::Exists { pattern },
            ExprInner::Case {
                operand,
                when_clauses,
                else_result,
            } => Expr::Case {
                operand,
                when_clauses,
                else_result,
            },
            ExprInner::Subquery {
                query,
                import_variables,
            } => Expr::Subquery {
                query,
                import_variables,
            },
        })
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl std::fmt::Display for ComparisonOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eq => write!(f, "="),
            Self::Neq => write!(f, "<>"),
            Self::Lt => write!(f, "<"),
            Self::Lte => write!(f, "<="),
            Self::Gt => write!(f, ">"),
            Self::Gte => write!(f, ">="),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOp {
    And,
    Or,
    Xor,
}

impl std::fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::And => write!(f, "AND"),
            Self::Or => write!(f, "OR"),
            Self::Xor => write!(f, "XOR"),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StringOp {
    StartsWith,
    EndsWith,
    Contains,
    Regex,
}

impl std::fmt::Display for StringOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StartsWith => write!(f, "STARTS WITH"),
            Self::EndsWith => write!(f, "ENDS WITH"),
            Self::Contains => write!(f, "CONTAINS"),
            Self::Regex => write!(f, "=~"),
        }
    }
}

// ---------------------------------------------------------------------------
// Projection — RETURN clause
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Projection {
    /// A specific field: variable.field AS alias
    Field {
        variable: VariableName,
        field: PropertyKey,
        alias: Option<String>,
    },

    /// A variable (returns all properties): variable AS alias
    Variable {
        variable: VariableName,
        alias: Option<String>,
    },

    /// An expression: expr AS alias
    #[schema(no_recursion)]
    Expression { expr: Expr, alias: String },

    /// Aggregation: count(variable), sum(variable.field), etc.
    /// `argument` is `None` for wildcard-style calls like `count(*)`.
    #[schema(no_recursion)]
    Aggregation {
        function: AggFunction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        argument: Option<Box<Projection>>,
        alias: String,
        #[serde(default)]
        distinct: bool,
    },

    /// All properties of a variable: variable { .* }
    AllProperties { variable: VariableName },
}

/// Custom deserializer for Projection.
/// LLMs generate aggregation as `{"kind": "aggregate", "function": "count", "variable": "c", "alias": "..."}`.
/// We normalize this to the canonical form with a proper `argument` Projection.
impl<'de> Deserialize<'de> for Projection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("expected object for Projection"))?;

        let kind = obj
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("variable");

        let read_variable = |default: &str| -> Result<VariableName, D::Error> {
            let s = obj
                .get("variable")
                .and_then(|v| v.as_str())
                .unwrap_or(default);
            VariableName::new(s).map_err(serde::de::Error::custom)
        };

        match kind {
            "field" => {
                let variable = read_variable("n")?;
                let alias = obj.get("alias").and_then(|v| v.as_str()).map(String::from);
                // If field is empty/missing, treat as variable projection (not n.``)
                match obj
                    .get("field")
                    .and_then(|v| v.as_str())
                    .filter(|f| !f.is_empty())
                {
                    Some(field) => Ok(Projection::Field {
                        variable,
                        field: PropertyKey::new(field).map_err(serde::de::Error::custom)?,
                        alias,
                    }),
                    None => Ok(Projection::Variable { variable, alias }),
                }
            }
            "variable" => Ok(Projection::Variable {
                variable: read_variable("n")?,
                alias: obj.get("alias").and_then(|v| v.as_str()).map(String::from),
            }),
            "expression" => {
                let expr = obj
                    .get("expr")
                    .cloned()
                    .map(|v| serde_json::from_value(v).map_err(serde::de::Error::custom))
                    .transpose()?
                    .ok_or_else(|| serde::de::Error::missing_field("expr"))?;
                let alias = obj
                    .get("alias")
                    .and_then(|v| v.as_str())
                    .unwrap_or("expr")
                    .to_string();
                Ok(Projection::Expression { expr, alias })
            }
            "aggregation" | "aggregate" => {
                let function: AggFunction = obj
                    .get("function")
                    .cloned()
                    .map(|v| serde_json::from_value(v).map_err(serde::de::Error::custom))
                    .transpose()?
                    .ok_or_else(|| serde::de::Error::missing_field("function"))?;
                let alias = obj
                    .get("alias")
                    .and_then(|v| v.as_str())
                    .unwrap_or("result")
                    .to_string();
                let distinct = obj
                    .get("distinct")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                // Canonical form uses `argument`; several LLM variants
                // use `variable`/`field` shorthand, and `variable: "*"`
                // is the wildcard form for `count(*)` where `argument`
                // collapses to `None`.
                let argument = if let Some(arg_val) = obj.get("argument") {
                    Some(Box::new(
                        serde_json::from_value::<Projection>(arg_val.clone())
                            .map_err(serde::de::Error::custom)?,
                    ))
                } else if let Some(var_str) = obj.get("variable").and_then(|v| v.as_str()) {
                    if var_str == "*" {
                        None
                    } else {
                        let variable =
                            VariableName::new(var_str).map_err(serde::de::Error::custom)?;
                        if let Some(field) = obj.get("field").and_then(|v| v.as_str()) {
                            Some(Box::new(Projection::Field {
                                variable,
                                field: PropertyKey::new(field).map_err(serde::de::Error::custom)?,
                                alias: None,
                            }))
                        } else {
                            Some(Box::new(Projection::Variable {
                                variable,
                                alias: None,
                            }))
                        }
                    }
                } else {
                    None
                };

                Ok(Projection::Aggregation {
                    function,
                    argument,
                    alias,
                    distinct,
                })
            }
            "all_properties" => Ok(Projection::AllProperties {
                variable: read_variable("n")?,
            }),
            other => Err(serde::de::Error::custom(format!(
                "unknown Projection kind: {other}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct AggregationExpr {
    pub function: AggFunction,
    #[schema(no_recursion)]
    pub field: FieldRef,
    pub alias: String,
    pub distinct: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AggFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    Collect,
    StdDev,
    Percentile,
    CollectList,
}

// ---------------------------------------------------------------------------
// References & Ordering
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct FieldRef {
    pub variable: VariableName,
    pub field: Option<PropertyKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct NodeRef {
    pub variable: VariableName,
    pub label: Option<GraphLabel>,
    #[schema(no_recursion)]
    pub property_filters: Vec<PropertyFilter>,
}

#[derive(Debug, Clone, Serialize, JsonSchema, utoipa::ToSchema)]
pub struct OrderClause {
    /// What to order by — a full projection (variable, field, aggregation, etc.)
    #[schema(no_recursion)]
    pub projection: Projection,
    pub direction: SortDirection,
}

/// Custom deserializer: accepts both `projection` and `expression` fields.
/// LLMs often generate `{"expression": {"expr_type": "property", ...}, "direction": "desc"}`
/// instead of `{"projection": {...}, "direction": "desc"}`.
impl<'de> Deserialize<'de> for OrderClause {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("expected object for OrderClause"))?;

        let direction: SortDirection = obj
            .get("direction")
            .cloned()
            .map(|v| serde_json::from_value(v).unwrap_or(SortDirection::Asc))
            .unwrap_or(SortDirection::Asc);

        // Try "projection" first, then fall back to "expression"
        if let Some(proj_val) = obj.get("projection") {
            let projection: Projection =
                serde_json::from_value(proj_val.clone()).map_err(serde::de::Error::custom)?;
            return Ok(OrderClause {
                projection,
                direction,
            });
        }

        if let Some(expr_val) = obj.get("expression") {
            // Convert Expr to a Projection::Expression
            let expr: Expr =
                serde_json::from_value(expr_val.clone()).map_err(serde::de::Error::custom)?;
            // If it's a simple property reference, convert to Projection::Field
            let projection = match &expr {
                Expr::Property {
                    variable,
                    field: Some(f),
                } => Projection::Field {
                    variable: variable.clone(),
                    field: f.clone(),
                    alias: None,
                },
                Expr::Property {
                    variable,
                    field: None,
                } => Projection::Variable {
                    variable: variable.clone(),
                    alias: None,
                },
                _ => Projection::Expression {
                    expr,
                    alias: "sort_key".to_string(),
                },
            };
            return Ok(OrderClause {
                projection,
                direction,
            });
        }

        // If neither, try to parse the whole value as a simple field reference
        if let Some(field) = obj.get("field").and_then(|v| v.as_str()) {
            let variable_str = obj.get("variable").and_then(|v| v.as_str()).unwrap_or("n");
            let variable = VariableName::new(variable_str).map_err(serde::de::Error::custom)?;
            return Ok(OrderClause {
                projection: Projection::Field {
                    variable,
                    field: PropertyKey::new(field).map_err(serde::de::Error::custom)?,
                    alias: None,
                },
                direction,
            });
        }

        Err(serde::de::Error::custom(
            "OrderClause requires 'projection' or 'expression' field",
        ))
    }
}

impl OrderClause {
    /// Create an order clause from a projection alias (e.g. ordering by
    /// aggregation result). Returns an error when the alias fails the
    /// [`VariableName`] invariants.
    pub fn from_alias(
        alias: impl Into<String>,
        direction: SortDirection,
    ) -> ox_core::error::OxResult<Self> {
        Ok(Self {
            projection: Projection::Variable {
                variable: VariableName::new(alias)?,
                alias: None,
            },
            direction,
        })
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

// ---------------------------------------------------------------------------
// Path algorithms
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PathAlgorithm {
    ShortestPath,
    AllShortestPaths,
    AllPaths,
}

// ---------------------------------------------------------------------------
// Chain steps (WITH ... MATCH ... pattern)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ChainStep {
    #[schema(no_recursion)]
    pub pass_through: Vec<Projection>,
    #[schema(no_recursion)]
    pub operation: QueryOp,
}

// ---------------------------------------------------------------------------
// Mutations — CREATE, MERGE, DELETE, SET
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "mutation", rename_all = "snake_case")]
pub enum MutateOp {
    /// CREATE (node)
    CreateNode {
        variable: VariableName,
        label: GraphLabel,
        properties: Vec<PropertyAssignment>,
    },

    /// CREATE (source)-[edge]->(target)
    CreateEdge {
        variable: Option<VariableName>,
        label: GraphLabel,
        source: VariableName,
        target: VariableName,
        properties: Vec<PropertyAssignment>,
    },

    /// MERGE (node) ON CREATE SET ... ON MATCH SET ...
    MergeNode {
        variable: VariableName,
        label: GraphLabel,
        match_properties: Vec<PropertyAssignment>,
        on_create: Vec<PropertyAssignment>,
        on_match: Vec<PropertyAssignment>,
    },

    /// MERGE (source)-[edge]->(target) ON CREATE SET ... ON MATCH SET ...
    MergeEdge {
        variable: Option<VariableName>,
        label: GraphLabel,
        source: VariableName,
        target: VariableName,
        match_properties: Vec<PropertyAssignment>,
        on_create: Vec<PropertyAssignment>,
        on_match: Vec<PropertyAssignment>,
    },

    /// SET variable.property = value
    SetProperty {
        variable: VariableName,
        property: PropertyKey,
        value: Expr,
    },

    /// DELETE variable (optionally DETACH DELETE)
    Delete {
        variable: VariableName,
        detach: bool,
    },

    /// REMOVE variable.property
    RemoveProperty {
        variable: VariableName,
        property: PropertyKey,
    },

    /// REMOVE variable:Label
    RemoveLabel {
        variable: VariableName,
        label: GraphLabel,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct PropertyAssignment {
    pub property: PropertyKey,
    #[schema(no_recursion)]
    pub value: Expr,
}

// ---------------------------------------------------------------------------
// QueryResult — runtime result carrier
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct QueryResult {
    /// Column names
    pub columns: Vec<String>,
    /// Rows of values
    pub rows: Vec<Vec<PropertyValue>>,
    /// Execution metadata
    pub metadata: QueryMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct QueryMetadata {
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Number of rows returned
    pub rows_returned: usize,
    /// Number of nodes created/modified (for mutations)
    pub nodes_affected: Option<usize>,
    /// Number of relationships created/modified (for mutations)
    pub edges_affected: Option<usize>,

    /// Π-3: Provenance trail for the query response — which
    /// ontology version produced this result, which sources were
    /// read, which types participated, and a human-readable
    /// filter summary. Populated by the compiler + runtime +
    /// federation planner on their way through; consumed by the
    /// admin UI ("response basis" panel) and by the LLM when it
    /// needs to explain WHY a given result came back. Optional —
    /// fast paths (cache hits with lost context, simple mutations)
    /// may skip population.
    ///
    /// Distinct from `ox_ontology::ProvenanceDef`, which records
    /// PROV-O activity/entity/agent **at the ontology IR level**
    /// (long-lived historical facts). `QueryProvenance` is a
    /// **per-response** trail tied to one query execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<QueryProvenance>,

    /// Non-blocking diagnostics produced during pre-execute validation.
    ///
    /// Populated by the HTTP route / agent tool layer after the runtime
    /// returns. The graph runtime itself leaves this empty since the
    /// validation pass happens inside `run_pre_execute` and its output
    /// is currently logged, not returned; downstream callers re-run the
    /// strict validator against the compiled statement to capture the
    /// diagnostics via
    /// [`ox_graph_runtime::cypher::strict_advisory_diagnostics`].
    ///
    /// Structured rather than pre-formatted so UI consumers can filter
    /// by `level` or `validator` without string-parsing a single
    /// canonical format. Kept named `warnings` to match the field's
    /// typical severity, but `Error`-level diagnostics from the strict
    /// re-pass do appear here: they represent patterns the permissive
    /// runtime allowed through but that a stricter lint would have
    /// blocked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<QueryDiagnostic>,
}

/// A single advisory diagnostic produced by the pre-execute validator
/// pipeline and surfaced on the response envelope. Mirrors the shape
/// of `ox_graph_runtime::cypher::ValidationIssue` without creating an
/// `ox-query-ir → ox-graph-runtime` dependency.
///
/// Frontends pattern-match on `validator` to route diagnostics to the
/// right help link and on `level` to choose colour / iconography.
/// `message` is a structured [`ox_core::DiagnosticMessage`]
/// (RFC 7807 / gRPC `Status` shape): the FE renders `code` + `params`
/// through its i18n catalogue, the BE logs / fallbacks consume the
/// English `message` rendering. Adding a UI language never requires
/// touching emit sites.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
pub struct QueryDiagnostic {
    /// Which validator emitted the diagnostic. Matches
    /// `CypherValidator::name()` (e.g. `"complexity"`,
    /// `"semantic-guard"`, `"ontology"`, `"safety"`).
    pub validator: String,
    /// Severity tier.
    pub level: DiagnosticLevel,
    /// Structured diagnostic — `code` + English `message` + `params`.
    pub message: ox_core::DiagnosticMessage,
}

/// Wire-stable severity for [`QueryDiagnostic`]. Lowercase-serialised
/// so JSON clients can string-compare without knowing Rust enum
/// conventions.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq, Hash,
)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    /// Strict-pass violation the permissive runtime let through.
    /// Non-blocking on the response envelope — the query already
    /// executed — but the UI should surface these most prominently.
    Error,
    /// Something worth flagging but not strict-rejecting.
    Warning,
    /// Informational suggestion (e.g. "consider adding a LIMIT").
    Info,
}

/// Π-3: Per-response provenance carried on every
/// [`QueryResult`]. Populated opportunistically by each planner /
/// runtime layer on its way through; consumed by response
/// explainability UIs and by the LLM when asked to justify a
/// result.
///
/// Industry reference: Snowflake Cortex Analyst "show reasoning"
/// surface, Looker drill-down, Palantir Foundry "how computed".
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq,
)]
pub struct QueryProvenance {
    /// The ontology IR id this query ran against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_id: Option<String>,
    /// Semantic version of the ontology IR. Two queries at
    /// different versions may see different types / mappings /
    /// value sets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_version: Option<String>,
    /// Business-time anchor the temporal rewriter used. `None` =
    /// "current".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
    /// External `source_id`s the federation planner touched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
    /// Node type ids that participated in this query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_ids: Vec<String>,
    /// Human-readable summary of the filters that were applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_summary: Option<String>,
    /// Content-addressed hashes for every registry collection the
    /// query may have read from — code systems, glossary terms,
    /// value sets, notation patterns, concept maps. Two queries with
    /// identical text but different registry versions can produce
    /// different results (a glossary rename, a value-set update, a
    /// concept-map rewrite) and this map is the re-run anchor the
    /// reproducibility pass uses. Keyed by a stable short name
    /// (`"code_systems"`, `"glossary"`, etc.) so the wire shape is
    /// self-documenting without forcing every caller to import a
    /// per-kind enum.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub registry_versions: std::collections::BTreeMap<String, String>,
    /// Per-output-column attribution back to the source column that
    /// supplied the value. Populated by the federation planner once
    /// a scan plus its projection is known; raw-Cypher and from-IR
    /// paths leave this empty (their lineage is the query text
    /// itself).
    ///
    /// Industry reference: Snowflake column lineage in `ACCOUNT_USAGE
    /// .OBJECT_DEPENDENCIES`, BigQuery `INFORMATION_SCHEMA.COLUMN
    /// _LINEAGE`, Palantir Foundry "data lineage" view. Carrying
    /// the trail on the query response (not just on stored views)
    /// makes "why is this cell what it is?" answerable from a single
    /// payload.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub column_lineage: Vec<ColumnLineage>,
    /// Backend dispatch attribution from the `PlanRouter`. Stable,
    /// FE-renderable string ("graph runtime — single-source pattern
    /// execution", "federation runtime — cross-source traversal
    /// detected"). Surfaces on the result panel's "response basis"
    /// section so the operator sees *why* the platform picked the
    /// route. `None` for paths that bypass the router (the
    /// `from-raw-cypher` admin escape, system-bypass cron loads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<String>,
}

/// One output-column ↔ source-column attribution edge surfaced on
/// [`QueryProvenance`]. Authors of the federation planner pin one
/// of these per projected column whose origin can be statically
/// resolved (a direct `SELECT col FROM t` projection) so the admin
/// UI / downstream lineage tools never have to reverse-engineer the
/// scan-to-projection mapping from the rendered Cypher / SQL.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, PartialEq, Eq)]
pub struct ColumnLineage {
    /// The result-set column the consumer sees.
    pub output_column: String,
    /// `ObjectMappingDef.source_id` the value was sourced from.
    pub source_id: String,
    /// Physical column on the source side (table-qualified when
    /// the source is multi-table; bare when the source is a single
    /// inline payload like a CSV).
    pub source_column: String,
    /// Optional textual transform applied on the way out (e.g.
    /// `"UPPER(col)"`, `"concept_map(cs-2024 → cs-2026)"`). `None`
    /// when the value is the source column verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod validate_scope_tests {
    use super::*;
    use ox_core::error::OxError;

    fn vn(s: &'static str) -> VariableName {
        VariableName::new(s).expect("test variable literal")
    }

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test graph label literal")
    }

    fn simple_match(variable: &'static str, label: &'static str) -> QueryIR {
        QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn(variable),
                    label: Some(gl(label)),
                    property_filters: vec![],
                }],
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        }
    }

    #[test]
    fn call_subquery_importing_outer_variable_passes() {
        let inner = simple_match("m", "Friend");
        let ir = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Chain {
                steps: vec![
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: vn("p"),
                                label: Some(gl("Person")),
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![],
                            optional: false,
                            group_by: vec![],
                        },
                    },
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::CallSubquery {
                            inner: Box::new(inner),
                            import_variables: vec!["p".to_string()],
                        },
                    },
                ],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        ir.validate().expect("scope should resolve");
    }

    #[test]
    fn call_subquery_importing_undeclared_variable_fails() {
        let inner = simple_match("m", "Friend");
        let ir = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Chain {
                steps: vec![
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Node {
                                variable: vn("p"),
                                label: Some(gl("Person")),
                                property_filters: vec![],
                            }],
                            filter: None,
                            projections: vec![],
                            optional: false,
                            group_by: vec![],
                        },
                    },
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::CallSubquery {
                            inner: Box::new(inner),
                            import_variables: vec!["ghost".to_string()],
                        },
                    },
                ],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        match ir.validate() {
            Err(OxError::Validation { field, message }) => {
                assert_eq!(field, "import_variables");
                assert!(
                    message.contains("`ghost`") && message.contains("`p`"),
                    "message should list missing var + available scope, got: {message}"
                );
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn expr_subquery_import_scope_validated() {
        // Match(p:Person) WHERE EXISTS { Subquery with import["p"] }.
        let sub = simple_match("m", "Friend");
        let ir = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("p"),
                    label: Some(gl("Person")),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Subquery {
                    query: Box::new(sub),
                    import_variables: vec!["p".to_string()],
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        ir.validate()
            .expect("importing `p` defined by the Match should pass");
    }

    #[test]
    fn expr_subquery_import_undeclared_fails() {
        let sub = simple_match("m", "Friend");
        let ir = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![GraphPattern::Node {
                    variable: vn("p"),
                    label: Some(gl("Person")),
                    property_filters: vec![],
                }],
                filter: Some(Expr::Subquery {
                    query: Box::new(sub),
                    import_variables: vec!["ghost".to_string()],
                }),
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        match ir.validate() {
            Err(OxError::Validation { field, .. }) => {
                assert_eq!(field, "import_variables");
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn relationship_endpoints_added_to_scope() {
        // (a)-[:KNOWS]->(b) gives {a, b} in scope; subquery imports `b`.
        let sub = simple_match("m", "Other");
        let ir = QueryIR {
            schema_version: QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Chain {
                steps: vec![
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::Match {
                            patterns: vec![GraphPattern::Relationship {
                                variable: None,
                                label: Some(gl("KNOWS")),
                                source: vn("a"),
                                target: vn("b"),
                                direction: Direction::Outgoing,
                                property_filters: vec![],
                                var_length: None,
                            }],
                            filter: None,
                            projections: vec![],
                            optional: false,
                            group_by: vec![],
                        },
                    },
                    ChainStep {
                        pass_through: vec![],
                        operation: QueryOp::CallSubquery {
                            inner: Box::new(sub),
                            import_variables: vec!["b".to_string()],
                        },
                    },
                ],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        ir.validate()
            .expect("relationship endpoints must enter scope");
    }

    #[test]
    fn canonical_hash_is_deterministic() {
        let a = simple_match("x", "Foo");
        let b = simple_match("x", "Foo");
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn canonical_hash_ignores_as_of() {
        // Two queries identical but for `as_of` must produce the
        // same hash — the reproducibility metric asks "is the same
        // plan running again", not "did the clock move".
        let mut a = simple_match("x", "Foo");
        let mut b = simple_match("x", "Foo");
        a.as_of = Some(
            chrono::DateTime::parse_from_rfc3339("2026-03-15T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        b.as_of = Some(
            chrono::DateTime::parse_from_rfc3339("2026-04-22T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn canonical_hash_ignores_schema_version_drift() {
        let mut a = simple_match("x", "Foo");
        let mut b = simple_match("x", "Foo");
        a.schema_version = 1;
        b.schema_version = 2;
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }

    #[test]
    fn canonical_hash_changes_on_limit() {
        // Adding a LIMIT is a materially different plan.
        let mut a = simple_match("x", "Foo");
        a.limit = Some(10);
        let b = simple_match("x", "Foo");
        assert_ne!(a.canonical_hash(), b.canonical_hash());
    }
}
