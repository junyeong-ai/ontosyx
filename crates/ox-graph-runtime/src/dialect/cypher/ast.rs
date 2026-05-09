//! Cypher partial AST.
//!
//! This is deliberately **not** a full openCypher parser. Its job is to give
//! rewriters (workspace isolation, future ACL / temporal / soft-delete passes)
//! and validators (ontology conformance, safety gates) enough structure to
//! reason about a query without having to re-implement parsing in each pass.
//!
//! The design is a two-level structure:
//!
//! 1. [`CypherAst`] — one or more [`CypherStatement`]s (separated by `UNION`).
//! 2. [`CypherStatement`] — a linear sequence of [`CypherClause`]s (MATCH,
//!    OPTIONAL MATCH, WHERE, CREATE, MERGE, SET, DELETE, RETURN, WITH, …).
//!
//! Clauses we understand in depth (MATCH / OPTIONAL MATCH / CREATE / MERGE)
//! parse their pattern list into [`CypherPattern`] with structured
//! [`CypherPatternElement`]s (nodes and relationships). Everything else is
//! captured as a token slice + the original source text so rewriters can
//! recognise the clause kind without disturbing grammar they don't care
//! about.
//!
//! Unsupported constructs survive as [`ClauseKind::Unknown`] with the raw
//! tokens preserved — the goal is *never to drop characters on the floor*.
//! A rewriter-then-render round trip of a query we didn't rewrite must
//! produce exactly the original text.

use crate::cypher::token::{CypherToken, Span};

/// Parsed Cypher query. A query is a sequence of [`CypherStatement`]s joined
/// by `UNION` / `UNION ALL`; isolation rewriters scope each statement
/// independently so a UNION cannot smuggle cross-workspace rows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CypherAst {
    pub statements: Vec<CypherStatement>,
    /// `UNION` separators between statements, preserved so renderers can
    /// reproduce the original concatenation (including `UNION ALL` vs `UNION`).
    pub unions: Vec<UnionKind>,
}

/// Which flavour of `UNION` separated two statements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnionKind {
    /// `UNION` — distinct-dedup semantics.
    Distinct,
    /// `UNION ALL` — preserves duplicates.
    All,
}

/// A single Cypher statement — the unit a rewriter scopes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CypherStatement {
    pub clauses: Vec<CypherClause>,
}

/// One clause in a statement. Clauses are stored in source order; the
/// `kind` identifies their role and any structured payload (patterns for
/// MATCH / CREATE / MERGE) lives alongside the preserved token slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CypherClause {
    pub kind: ClauseKind,
    /// The clause's tokens in source order (including its leading keyword).
    pub tokens: Vec<CypherToken>,
    /// The exact source text of the clause. Rewriters that don't touch a
    /// clause can render it back by copying this string verbatim, which
    /// keeps formatting and incidental whitespace intact.
    pub text: String,
    /// Byte span in the original input — useful for diagnostics.
    pub span: Span,
    /// Structured patterns for MATCH / OPTIONAL MATCH / CREATE / MERGE.
    /// Empty for every other kind.
    pub patterns: Vec<CypherPattern>,
    /// Property assignments captured from `SET` clauses
    /// (`SET n.status = 'X', e.weight = 0.5`). Empty for every kind
    /// other than `Set`. Pre-rewrite validators (SHACL) consume
    /// these to enforce constraints on the new value the same way
    /// they enforce inline `CREATE { ... }` maps.
    pub set_items: Vec<SetItem>,
    /// Property removals captured from `REMOVE` clauses
    /// (`REMOVE n.legacy_field, n.tmp`). Empty for every kind other
    /// than `Remove`. SHACL `MinCount` enforcement reads this to
    /// reject removals of required properties.
    pub remove_items: Vec<RemoveItem>,
}

/// One assignment inside a `SET` clause: `n.status = 'X'`.
///
/// Variable / property names are bare strings — backtick quoting is
/// stripped at parse time so the same key matches the inline-map
/// representation `(NodePattern.properties)`. `value_text` is the raw
/// source slice the parser saw on the right-hand side
/// (`'X'`, `42`, `$param`, `null`, `n.other_prop`, …); validators
/// inspect string-literal values directly and skip non-literals
/// because their compliance can't be settled at AST time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetItem {
    pub variable: String,
    pub property: String,
    pub value_text: String,
    pub span: Span,
}

/// One target inside a `REMOVE` clause: `n.legacy_field`.
///
/// Same naming rules as [`SetItem`] (bare variable / property,
/// backticks stripped). REMOVE on a label (`REMOVE n:Customer`) is
/// represented by a separate target shape later; the MVP captures
/// the property-removal form because that's the surface SHACL
/// MinCount enforcement covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveItem {
    pub variable: String,
    pub property: String,
    pub span: Span,
}

/// Recognised clause keyword at the head of a clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClauseKind {
    Match,
    OptionalMatch,
    Create,
    Merge,
    Where,
    Set,
    Delete,
    DetachDelete,
    Remove,
    Return,
    With,
    Unwind,
    Call,
    OrderBy,
    Skip,
    Limit,
    /// A clause whose leading keyword we don't specifically recognise
    /// (e.g. `FOREACH`, a bare expression between clauses). Preserved
    /// verbatim so rewriters / renderers don't mangle it.
    Unknown,
}

impl ClauseKind {
    /// Whether this clause kind carries pattern-shaped payload that
    /// rewriters may want to introspect.
    pub fn has_patterns(self) -> bool {
        matches!(
            self,
            ClauseKind::Match | ClauseKind::OptionalMatch | ClauseKind::Create | ClauseKind::Merge,
        )
    }

    /// Whether this kind is one of the write-effecting clauses that a
    /// safety validator wants to scrutinise.
    pub fn is_write(self) -> bool {
        matches!(
            self,
            ClauseKind::Create
                | ClauseKind::Merge
                | ClauseKind::Set
                | ClauseKind::Delete
                | ClauseKind::DetachDelete
                | ClauseKind::Remove,
        )
    }
}

/// A comma-separated pattern list inside a MATCH / CREATE / MERGE clause.
/// `MATCH (a)-[:X]->(b), (c)-[:Y]->(d)` parses as two patterns.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CypherPattern {
    pub elements: Vec<CypherPatternElement>,
    /// Byte span in the original input.
    pub span: Span,
}

/// One element of a pattern — a node or a relationship. Stored in source
/// order so `(a)-[:X]->(b)` is `[Node(a), Relationship(X), Node(b)]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CypherPatternElement {
    Node(NodePattern),
    Relationship(RelationshipPattern),
}

/// A parenthesised node pattern: `(var:Label {prop: value})`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodePattern {
    /// Variable bound by the node, if any. `(p:Person)` → `Some("p")`;
    /// `(:Person)` → `None`.
    pub variable: Option<String>,
    /// Labels in declaration order. Cypher supports multiple labels
    /// via `(n:Person:Employee)`.
    pub labels: Vec<String>,
    /// Inline property filters captured as `(key, raw_value_text)` pairs.
    /// Values are intentionally left as raw text — they can reference
    /// parameters, functions, or literals, and nothing in the rewriter or
    /// validator needs to interpret them at that level.
    pub properties: Vec<(String, String)>,
    /// Byte span in the original input.
    pub span: Span,
}

/// A bracketed relationship pattern: `-[var:TYPE*1..3 {prop: value}]->`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RelationshipPattern {
    pub variable: Option<String>,
    /// Relationship types (`|`-separated in Cypher). Empty means "any type".
    pub types: Vec<String>,
    pub direction: RelDirection,
    /// Variable-length bounds `*min..max`. `None` = single hop.
    /// `Some((None, None))` = unbounded `*`.
    pub var_length: Option<(Option<u32>, Option<u32>)>,
    pub properties: Vec<(String, String)>,
    pub span: Span,
}

/// Direction of a relationship pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelDirection {
    /// `(a)-[…]->(b)` — left to right.
    #[default]
    Outgoing,
    /// `(a)<-[…]-(b)` — right to left.
    Incoming,
    /// `(a)-[…]-(b)` — direction-agnostic.
    Undirected,
}

impl CypherAst {
    /// Walk every pattern element across every clause of every statement.
    /// Handy for extractors (label / property / relationship-type inventories).
    pub fn pattern_elements(&self) -> impl Iterator<Item = &CypherPatternElement> {
        self.statements.iter().flat_map(|stmt| {
            stmt.clauses
                .iter()
                .flat_map(|clause| clause.patterns.iter().flat_map(|p| p.elements.iter()))
        })
    }

    /// Collect every node label mentioned in any pattern, deduplicated
    /// (insertion order preserved).
    pub fn node_labels(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for el in self.pattern_elements() {
            if let CypherPatternElement::Node(n) = el {
                for label in &n.labels {
                    if !seen.contains(label) {
                        seen.push(label.clone());
                    }
                }
            }
        }
        seen
    }

    /// Collect every relationship type mentioned in any pattern, deduplicated.
    pub fn relationship_types(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for el in self.pattern_elements() {
            if let CypherPatternElement::Relationship(r) = el {
                for ty in &r.types {
                    if !seen.contains(ty) {
                        seen.push(ty.clone());
                    }
                }
            }
        }
        seen
    }

    /// Does any statement / clause carry a write-effecting kind?
    pub fn has_writes(&self) -> bool {
        self.statements
            .iter()
            .flat_map(|s| s.clauses.iter())
            .any(|c| c.kind.is_write())
    }
}

impl CypherStatement {
    /// Walk every pattern-bearing clause and collect the labels each
    /// **node** variable picks up. A variable can carry multiple
    /// labels across MATCH / CREATE / MERGE — `MATCH (n:Person)
    /// MERGE (n:Customer)` lands `n` with `[Person, Customer]` so
    /// downstream SET / REMOVE checks fire against every applicable
    /// label. The first declaration wins for ordering; duplicates
    /// are coalesced.
    ///
    /// Used by the ontology validator (unknown SET / REMOVE properties
    /// on a node variable) and the SHACL validator (PropertyShape
    /// resolution); shared so a single walk feeds both.
    pub fn variable_labels(&self) -> std::collections::HashMap<String, Vec<String>> {
        let mut out: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for clause in &self.clauses {
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
                        let entry = out.entry(var.clone()).or_default();
                        for label in &node.labels {
                            if !entry.contains(label) {
                                entry.push(label.clone());
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Same as [`Self::variable_labels`] but for **relationship**
    /// variables: the types each `r` in `[r:T1|T2]` is bound to.
    /// Used by the ontology validator to flag SET / REMOVE typos on
    /// edge properties (`SET r.sicne = 2020` when WORKS_AT defines
    /// `since`). Anonymous relationship patterns (no variable) and
    /// patterns without explicit types contribute nothing — without
    /// a type we cannot resolve ontologically and silently skip is
    /// the safer call than a false flag.
    pub fn variable_relationship_types(&self) -> std::collections::HashMap<String, Vec<String>> {
        let mut out: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for clause in &self.clauses {
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
                    if let CypherPatternElement::Relationship(rel) = element
                        && let Some(var) = &rel.variable
                    {
                        let entry = out.entry(var.clone()).or_default();
                        for ty in &rel.types {
                            if !entry.contains(ty) {
                                entry.push(ty.clone());
                            }
                        }
                    }
                }
            }
        }
        out
    }
}
