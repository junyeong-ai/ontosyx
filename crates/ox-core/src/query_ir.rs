use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::{Direction, PropertyValue};

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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryIR {
    /// The core query operation
    pub operation: QueryOp,
    /// LIMIT clause
    pub limit: Option<usize>,
    /// SKIP clause
    pub skip: Option<usize>,
    /// ORDER BY clauses
    pub order_by: Vec<OrderClause>,
}

impl QueryIR {
    /// Validate structural integrity of the QueryIR.
    ///
    /// Catches issues that would cause Cypher compilation failures:
    /// - CASE expressions with no WHEN clauses
    /// - IN expressions with empty value lists
    /// - Match operations with no patterns
    pub fn validate(&self) -> Result<(), crate::error::OxError> {
        Self::validate_op(&self.operation)
    }

    fn validate_op(op: &QueryOp) -> Result<(), crate::error::OxError> {
        match op {
            QueryOp::Match { patterns, .. } => {
                if patterns.is_empty() {
                    return Err(crate::error::OxError::Validation {
                        field: "patterns".into(),
                        message: "Match operation must have at least one pattern".into(),
                    });
                }
                Ok(())
            }
            QueryOp::Aggregate { source, .. } => Self::validate_op(&source.operation),
            QueryOp::Union { queries, .. } => {
                for q in queries {
                    Self::validate_op(&q.operation)?;
                }
                Ok(())
            }
            QueryOp::Chain { steps } => {
                for s in steps {
                    Self::validate_op(&s.operation)?;
                }
                Ok(())
            }
            QueryOp::CallSubquery { inner, .. } => Self::validate_op(&inner.operation),
            QueryOp::Mutate { context, .. } => {
                if let Some(ctx) = context {
                    Self::validate_op(ctx)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Graph analytics — backend-agnostic algorithm descriptors
// ---------------------------------------------------------------------------

/// Graph algorithm type — backend-agnostic.
/// Each compiler maps these to native calls (e.g., Neo4j GDS, Neptune Analytics).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphAlgorithm {
    PageRank,
    CommunityDetection,
    BetweennessCentrality,
    ShortestPath,
    NodeSimilarity,
}

/// Source for analytics computation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalyticsSource {
    /// Run on all nodes/edges in the graph
    WholeGraph,
    /// Run on nodes with specific labels
    Labels { labels: Vec<String> },
    /// Run on a filtered subgraph
    Subgraph { filter: Box<QueryOp> },
}

// ---------------------------------------------------------------------------
// QueryOp — top-level query operations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueryOp {
    /// Pattern matching with optional filtering and projection
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
        edge_types: Vec<String>,
        direction: Direction,
        max_depth: Option<usize>,
        algorithm: PathAlgorithm,
    },

    /// Aggregation over a sub-query
    Aggregate {
        source: Box<QueryIR>,
        group_by: Vec<FieldRef>,
        aggregations: Vec<AggregationExpr>,
    },

    /// UNION of multiple queries
    Union { queries: Vec<QueryIR>, all: bool },

    /// Sequential chaining (WITH ... MATCH ...)
    /// Allows intermediate result passing between query steps
    Chain { steps: Vec<ChainStep> },

    /// Subquery as a query step — runs a nested query inline.
    /// Compiles to Cypher CALL { WITH ... }
    CallSubquery {
        inner: Box<QueryIR>,
        /// Variables to pass from outer to inner scope
        import_variables: Vec<String>,
    },

    /// Mutation operations (CREATE, MERGE, DELETE, SET)
    Mutate {
        /// Optional preceding MATCH for context
        context: Option<Box<QueryOp>>,
        operations: Vec<MutateOp>,
        returning: Vec<Projection>,
    },

    /// Run a graph analytics algorithm.
    /// Each compiler maps this to backend-native calls (e.g., Neo4j GDS).
    Analytics {
        algorithm: GraphAlgorithm,
        source: AnalyticsSource,
        params: HashMap<String, Expr>,
        projections: Vec<Projection>,
    },
}

// ---------------------------------------------------------------------------
// GraphPattern — describes a subgraph pattern to match
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphPattern {
    /// A single node: (variable:Label {props})
    Node {
        variable: String,
        label: Option<String>,
        property_filters: Vec<PropertyFilter>,
    },

    /// A relationship between two nodes:
    /// (source)-[variable:Label]->(target)
    Relationship {
        variable: Option<String>,
        label: Option<String>,
        source: String,
        target: String,
        direction: Direction,
        property_filters: Vec<PropertyFilter>,
        /// Variable-length path: *min..max
        var_length: Option<VarLength>,
    },

    /// A complete path pattern: (a)-[r1]->(b)-[r2]->(c)
    Path { elements: Vec<PathElement> },
}

/// Inline property filter within a pattern: {name: "Alice", age: 30}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PropertyFilter {
    pub property: String,
    pub value: Expr,
}

/// Variable-length path specification
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VarLength {
    pub min: Option<usize>,
    pub max: Option<usize>,
}

/// A single element in a path pattern
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathElement {
    Node {
        variable: String,
        label: Option<String>,
    },
    Edge {
        variable: Option<String>,
        label: Option<String>,
        direction: Direction,
    },
}

// ---------------------------------------------------------------------------
// Expr — filter/where expressions (DB-agnostic expression tree)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "expr_type", rename_all = "snake_case")]
pub enum Expr {
    /// A literal value
    Literal { value: PropertyValue },

    /// A property reference: variable.field (field is None when referencing the whole variable)
    Property {
        variable: String,
        #[serde(default)]
        field: Option<String>,
    },

    /// Binary comparison: left op right
    Comparison {
        left: Box<Expr>,
        op: ComparisonOp,
        right: Box<Expr>,
    },

    /// Logical combination: left AND/OR right
    Logical {
        left: Box<Expr>,
        op: LogicalOp,
        right: Box<Expr>,
    },

    /// Negation: NOT expr
    Not { inner: Box<Expr> },

    /// IN check: expr IN [values]
    In {
        expr: Box<Expr>,
        values: Vec<PropertyValue>,
    },

    /// NULL check: expr IS NULL / IS NOT NULL
    IsNull { expr: Box<Expr>, negated: bool },

    /// String operations: STARTS WITH, ENDS WITH, CONTAINS
    StringOp {
        left: Box<Expr>,
        op: StringOp,
        right: Box<Expr>,
    },

    /// Function call: function(args)
    #[serde(alias = "function")]
    FunctionCall {
        #[serde(alias = "name")]
        function: String,
        #[serde(default)]
        args: Vec<Expr>,
    },

    /// Pattern existence check: EXISTS { (a)-[:KNOWS]->(b) }
    Exists { pattern: Box<GraphPattern> },

    /// CASE expression: simple or searched
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
    Subquery {
        query: Box<QueryIR>,
        /// Variables from outer scope to import into subquery
        import_variables: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WhenClause {
    pub condition: Expr,
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
            let pv_type = map.remove("expr_type").unwrap();
            map.insert("type".to_string(), pv_type);
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
            Property {
                variable: String,
                #[serde(default)]
                field: Option<String>,
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
                function: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Projection {
    /// A specific field: variable.field AS alias
    Field {
        variable: String,
        field: String,
        alias: Option<String>,
    },

    /// A variable (returns all properties): variable AS alias
    Variable {
        variable: String,
        alias: Option<String>,
    },

    /// An expression: expr AS alias
    Expression { expr: Expr, alias: String },

    /// Aggregation: count(variable), sum(variable.field), etc.
    Aggregation {
        function: AggFunction,
        argument: Box<Projection>,
        alias: String,
        #[serde(default)]
        distinct: bool,
    },

    /// All properties of a variable: variable { .* }
    AllProperties { variable: String },
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

        match kind {
            "field" => {
                let variable = obj
                    .get("variable")
                    .and_then(|v| v.as_str())
                    .unwrap_or("n")
                    .to_string();
                let alias = obj.get("alias").and_then(|v| v.as_str()).map(String::from);
                // If field is empty/missing, treat as variable projection (not n.``)
                match obj
                    .get("field")
                    .and_then(|v| v.as_str())
                    .filter(|f| !f.is_empty())
                {
                    Some(field) => Ok(Projection::Field {
                        variable,
                        field: field.to_string(),
                        alias,
                    }),
                    None => Ok(Projection::Variable { variable, alias }),
                }
            }
            "variable" => Ok(Projection::Variable {
                variable: obj
                    .get("variable")
                    .and_then(|v| v.as_str())
                    .unwrap_or("n")
                    .to_string(),
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

                // Try canonical "argument" first, then fall back to "variable"/"field"
                let argument = if let Some(arg_val) = obj.get("argument") {
                    Box::new(
                        serde_json::from_value::<Projection>(arg_val.clone())
                            .map_err(serde::de::Error::custom)?,
                    )
                } else if let Some(var) = obj.get("variable").and_then(|v| v.as_str()) {
                    // LLM shorthand: {"kind": "aggregate", "function": "count", "variable": "c"}
                    if let Some(field) = obj.get("field").and_then(|v| v.as_str()) {
                        Box::new(Projection::Field {
                            variable: var.to_string(),
                            field: field.to_string(),
                            alias: None,
                        })
                    } else {
                        Box::new(Projection::Variable {
                            variable: var.to_string(),
                            alias: None,
                        })
                    }
                } else {
                    // count(*) — use wildcard variable
                    Box::new(Projection::Variable {
                        variable: "*".to_string(),
                        alias: None,
                    })
                };

                Ok(Projection::Aggregation {
                    function,
                    argument,
                    alias,
                    distinct,
                })
            }
            "all_properties" => {
                let variable = obj
                    .get("variable")
                    .and_then(|v| v.as_str())
                    .unwrap_or("n")
                    .to_string();
                Ok(Projection::AllProperties { variable })
            }
            other => Err(serde::de::Error::custom(format!(
                "unknown Projection kind: {other}"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AggregationExpr {
    pub function: AggFunction,
    pub field: FieldRef,
    pub alias: String,
    pub distinct: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FieldRef {
    pub variable: String,
    pub field: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeRef {
    pub variable: String,
    pub label: Option<String>,
    pub property_filters: Vec<PropertyFilter>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OrderClause {
    /// What to order by — a full projection (variable, field, aggregation, etc.)
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
            let variable = obj.get("variable").and_then(|v| v.as_str()).unwrap_or("n");
            return Ok(OrderClause {
                projection: Projection::Field {
                    variable: variable.to_string(),
                    field: field.to_string(),
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
    /// Create an order clause from a projection alias (e.g. ordering by aggregation result).
    pub fn from_alias(alias: impl Into<String>, direction: SortDirection) -> Self {
        Self {
            projection: Projection::Variable {
                variable: alias.into(),
                alias: None,
            },
            direction,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

// ---------------------------------------------------------------------------
// Path algorithms
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathAlgorithm {
    ShortestPath,
    AllShortestPaths,
    AllPaths,
}

// ---------------------------------------------------------------------------
// Chain steps (WITH ... MATCH ... pattern)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChainStep {
    pub pass_through: Vec<Projection>,
    pub operation: QueryOp,
}

// ---------------------------------------------------------------------------
// Mutations — CREATE, MERGE, DELETE, SET
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mutation", rename_all = "snake_case")]
pub enum MutateOp {
    /// CREATE (node)
    CreateNode {
        variable: String,
        label: String,
        properties: Vec<PropertyAssignment>,
    },

    /// CREATE (source)-[edge]->(target)
    CreateEdge {
        variable: Option<String>,
        label: String,
        source: String,
        target: String,
        properties: Vec<PropertyAssignment>,
    },

    /// MERGE (node) ON CREATE SET ... ON MATCH SET ...
    MergeNode {
        variable: String,
        label: String,
        match_properties: Vec<PropertyAssignment>,
        on_create: Vec<PropertyAssignment>,
        on_match: Vec<PropertyAssignment>,
    },

    /// MERGE (source)-[edge]->(target) ON CREATE SET ... ON MATCH SET ...
    MergeEdge {
        variable: Option<String>,
        label: String,
        source: String,
        target: String,
        match_properties: Vec<PropertyAssignment>,
        on_create: Vec<PropertyAssignment>,
        on_match: Vec<PropertyAssignment>,
    },

    /// SET variable.property = value
    SetProperty {
        variable: String,
        property: String,
        value: Expr,
    },

    /// DELETE variable (optionally DETACH DELETE)
    Delete { variable: String, detach: bool },

    /// REMOVE variable.property
    RemoveProperty { variable: String, property: String },

    /// REMOVE variable:Label
    RemoveLabel { variable: String, label: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PropertyAssignment {
    pub property: String,
    pub value: Expr,
}

// ---------------------------------------------------------------------------
// QueryResult — runtime result carrier
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryResult {
    /// Column names
    pub columns: Vec<String>,
    /// Rows of values
    pub rows: Vec<Vec<PropertyValue>>,
    /// Execution metadata
    pub metadata: QueryMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QueryMetadata {
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Number of rows returned
    pub rows_returned: usize,
    /// Number of nodes created/modified (for mutations)
    pub nodes_affected: Option<usize>,
    /// Number of relationships created/modified (for mutations)
    pub edges_affected: Option<usize>,
}
