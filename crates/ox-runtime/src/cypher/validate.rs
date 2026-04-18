//! Cypher validator pipeline.
//!
//! Companion to the [`crate::cypher::rewrite`] pipeline. A *rewriter*
//! mutates an AST and produces a new one; a *validator* inspects an AST
//! and produces diagnostics. Both share the same shape (trait +
//! ordered pipeline + context) on purpose — the two pipelines plug
//! into the same place in a query's life cycle.
//!
//! Three validators land here:
//!
//! - [`SafetyValidator`]: blocks destructive statements that lack a
//!   WHERE-bound scope (unrestricted DELETE / DETACH DELETE / REMOVE)
//!   and rejects DDL tokens (`DROP`) that must go through the schema
//!   migration path, not the runtime query path.
//! - [`OntologyValidator`]: verifies that every label, relationship
//!   type, and inline property key referenced in a pattern is defined
//!   on the active [`OntologyIR`]. Catches LLM hallucinations
//!   (`(u:Userr)` typos, invented properties) before they hit the DB.
//! - [`WorkspaceScopeValidator`]: a post-rewrite gate. Every statement
//!   that touches graph data must textually reference the
//!   workspace-scope property — otherwise isolation silently failed.
//!
//! The intended full flow for an LLM-generated query:
//!
//! ```text
//! parse → validate (safety + ontology)
//!   → if errors, reject
//!   → else rewrite (workspace-scope + future ACL / soft-delete)
//!   → validate (workspace-scope gate)
//!   → execute
//! ```
//!
//! A failing pass never aborts the pipeline — every validator runs so
//! the caller can present all issues at once (safety error + ontology
//! mismatch) instead of the LLM having to retry one error at a time.

use std::collections::HashSet;
use std::fmt;

use ox_core::ontology_ir::OntologyIR;

use crate::cypher::ast::{ClauseKind, CypherAst, CypherPatternElement};
use crate::cypher::parse;
use crate::cypher::token::Span;

// ---------------------------------------------------------------------------
// ValidateContext
// ---------------------------------------------------------------------------

/// Per-request context passed to every validator. Intentionally minimal:
/// a validator that needs richer data (e.g. an ontology snapshot, a
/// permissions table) receives it through its own constructor, mirroring
/// [`crate::cypher::rewrite::RewriteContext`].
#[derive(Debug, Clone)]
pub struct ValidateContext {
    pub workspace_id: String,
}

impl ValidateContext {
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// ValidationIssue
// ---------------------------------------------------------------------------

/// Severity of a validation issue.
///
/// `Error` blocks execution. `Warning` / `Info` bubble up for logging
/// and UI surfacing — a typo that the ontology is confident about may
/// be worth warning on, while a lower-severity pattern (e.g. a
/// `LIMIT` clause we'd like to suggest) can be `Info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IssueLevel {
    Error,
    Warning,
    Info,
}

impl fmt::Display for IssueLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IssueLevel::Error => "error",
            IssueLevel::Warning => "warning",
            IssueLevel::Info => "info",
        };
        f.write_str(s)
    }
}

/// A single diagnostic produced by a validator.
///
/// `span` points at the offending source location when available so
/// diagnostics can surface an inline underline / caret in the UI.
/// `validator_name` mirrors [`CypherValidator::name`] so aggregated
/// reports can group issues by the pass that produced them.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub level: IssueLevel,
    pub message: String,
    pub span: Option<Span>,
    pub validator_name: String,
}

impl ValidationIssue {
    pub fn error(validator: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Error,
            message: message.into(),
            span: None,
            validator_name: validator.into(),
        }
    }

    pub fn warning(validator: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Warning,
            message: message.into(),
            span: None,
            validator_name: validator.into(),
        }
    }

    pub fn info(validator: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Info,
            message: message.into(),
            span: None,
            validator_name: validator.into(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

// ---------------------------------------------------------------------------
// ValidationReport
// ---------------------------------------------------------------------------

/// Aggregated output of a [`CypherValidatorPipeline`] run.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    /// Does the report contain any [`IssueLevel::Error`]? Callers that
    /// must not execute on errors use this as their gate.
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.level == IssueLevel::Error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.level == IssueLevel::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.level == IssueLevel::Warning)
    }

    pub fn infos(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.level == IssueLevel::Info)
    }
}

// ---------------------------------------------------------------------------
// CypherValidator trait + Pipeline
// ---------------------------------------------------------------------------

/// A single Cypher validation pass. Unlike [`crate::cypher::rewrite::CypherRewriter`],
/// validators never mutate the AST — they only inspect it.
pub trait CypherValidator: Send + Sync {
    /// Identifier used in diagnostics and logs.
    fn name(&self) -> &str;

    /// Inspect `ast` and return any issues discovered.
    fn validate(&self, ast: &CypherAst, ctx: &ValidateContext) -> Vec<ValidationIssue>;
}

/// Ordered collection of validators.
///
/// All validators run regardless of earlier-pass failures. A single
/// query can surface a safety violation and an ontology mismatch in
/// one report so the caller can present both at once instead of
/// forcing the LLM to retry one error per round-trip.
#[derive(Default)]
pub struct CypherValidatorPipeline {
    validators: Vec<Box<dyn CypherValidator>>,
}

impl CypherValidatorPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a validator. Fluent: `pipeline.with(a).with(b)`.
    pub fn with(mut self, validator: impl CypherValidator + 'static) -> Self {
        self.validators.push(Box::new(validator));
        self
    }

    pub fn len(&self) -> usize {
        self.validators.len()
    }

    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }

    /// Run every validator over the given AST and return a combined
    /// [`ValidationReport`]. Issues appear in validator-registration
    /// order, and within each validator in whatever order it chose.
    pub fn run_ast(&self, ast: &CypherAst, ctx: &ValidateContext) -> ValidationReport {
        let mut issues = Vec::new();
        for v in &self.validators {
            issues.extend(v.validate(ast, ctx));
        }
        ValidationReport { issues }
    }

    /// Parse `input` and run every validator. Convenience for callers
    /// that hold source text and don't need access to the AST
    /// (typical LLM-generated-Cypher gate).
    pub fn run(&self, input: &str, ctx: &ValidateContext) -> ValidationReport {
        let ast = parse(input);
        self.run_ast(&ast, ctx)
    }
}

impl fmt::Debug for CypherValidatorPipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.validators.iter().map(|v| v.name()).collect();
        f.debug_struct("CypherValidatorPipeline")
            .field("validators", &names)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// SafetyValidator
// ---------------------------------------------------------------------------

/// Block destructive statements that lack a bounding WHERE predicate,
/// and DDL tokens that must travel the schema-migration path instead
/// of the runtime query path.
///
/// Policy:
///
/// - `DELETE` / `DETACH DELETE` / `REMOVE` require a WHERE clause
///   somewhere in the same statement. A naked `MATCH (n) DELETE n`
///   wipes the entire graph (or at least every node of the matched
///   label); idiomatic use always pairs a predicate with the write.
/// - Any appearance of a `DROP` keyword — as in DDL like
///   `DROP CONSTRAINT` — is rejected. Runtime queries must not tear
///   down schema objects; schema mutations go through the compiler's
///   migration emitter.
///
/// The validator never tries to outsmart the predicate: a trivially
/// tautological `WHERE true` passes. Spotting that nuance is a future
/// `SemanticGuardValidator`'s job.
#[derive(Debug, Clone, Default)]
pub struct SafetyValidator;

impl SafetyValidator {
    pub const fn new() -> Self {
        Self
    }
}

impl CypherValidator for SafetyValidator {
    fn name(&self) -> &str {
        "safety"
    }

    fn validate(&self, ast: &CypherAst, _ctx: &ValidateContext) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        for statement in &ast.statements {
            let has_where = statement
                .clauses
                .iter()
                .any(|c| c.kind == ClauseKind::Where);

            for clause in &statement.clauses {
                // Destructive writes must be WHERE-bounded.
                let destructive_msg = match clause.kind {
                    ClauseKind::Delete => Some(
                        "DELETE without WHERE is not allowed — add a predicate bounding the deletion",
                    ),
                    ClauseKind::DetachDelete => Some(
                        "DETACH DELETE without WHERE is not allowed — add a predicate bounding the deletion",
                    ),
                    ClauseKind::Remove => Some(
                        "REMOVE without WHERE is not allowed — add a predicate bounding the affected nodes",
                    ),
                    _ => None,
                };
                if let Some(msg) = destructive_msg
                    && !has_where
                {
                    issues.push(
                        ValidationIssue::error("safety", msg).with_span(clause.span),
                    );
                }

                // DDL tokens (DROP) are never allowed on the runtime path.
                for tok in &clause.tokens {
                    if tok.is_keyword("DROP") {
                        issues.push(
                            ValidationIssue::error(
                                "safety",
                                "DROP is not allowed in runtime queries — schema changes go through the migration path",
                            )
                            .with_span(tok.span),
                        );
                    }
                }
            }
        }

        issues
    }
}

// ---------------------------------------------------------------------------
// OntologyValidator
// ---------------------------------------------------------------------------

/// Verify that every label, relationship type, and inline property key
/// named by the query is defined on the active ontology.
///
/// Primary use: pre-flight check for LLM-generated Cypher. The model
/// occasionally hallucinates a label (`:Userr` typo), an invented
/// relationship (`[:WORKS_FOR]` when the ontology says `[:WORKS_AT]`),
/// or a non-existent property (`n.emial`). A bad execution surface
/// against the graph driver produces an opaque error; catching it here
/// gives the caller a structured issue with the exact offending name.
///
/// The validator treats underscore-prefixed property names (`_workspace_id`,
/// internal system keys) as opaque and skips them — those are injected by
/// the rewriter pipeline, not authored by the user.
pub struct OntologyValidator {
    ontology: OntologyIR,
}

impl OntologyValidator {
    pub fn new(ontology: OntologyIR) -> Self {
        Self { ontology }
    }
}

impl fmt::Debug for OntologyValidator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OntologyValidator")
            .field("ontology_id", &self.ontology.id)
            .finish()
    }
}

impl CypherValidator for OntologyValidator {
    fn name(&self) -> &str {
        "ontology"
    }

    fn validate(&self, ast: &CypherAst, _ctx: &ValidateContext) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // --- Node labels -----------------------------------------------------
        for label in ast.node_labels() {
            if self.ontology.node_by_label(&label).is_none() {
                issues.push(ValidationIssue::error(
                    "ontology",
                    format!(
                        "unknown node label `{label}` — not defined in the active ontology"
                    ),
                ));
            }
        }

        // --- Relationship types ---------------------------------------------
        let known_edge_labels: HashSet<&str> = self
            .ontology
            .edge_types()
            .iter()
            .map(|e| e.label.as_str())
            .collect();
        for rel in ast.relationship_types() {
            if !known_edge_labels.contains(rel.as_str()) {
                issues.push(ValidationIssue::error(
                    "ontology",
                    format!(
                        "unknown relationship type `{rel}` — not defined in the active ontology"
                    ),
                ));
            }
        }

        // --- Inline property keys on labelled node patterns -----------------
        //
        // `(u:User {email: $e})` — verify `email` is declared on `User`. If a
        // node pattern carries multiple labels we pass when any label declares
        // the property (multi-label nodes are a union).
        for element in ast.pattern_elements() {
            if let CypherPatternElement::Node(node) = element {
                if node.labels.is_empty() || node.properties.is_empty() {
                    continue;
                }
                for (key, _) in &node.properties {
                    if is_system_property(key) {
                        continue;
                    }
                    let matched = node.labels.iter().any(|label| {
                        self.ontology
                            .node_by_label(label)
                            .is_some_and(|n| n.properties.iter().any(|p| p.name == *key))
                    });
                    if !matched {
                        let label_list = node.labels.join("/");
                        issues.push(ValidationIssue::error(
                            "ontology",
                            format!(
                                "property `{key}` not defined on label `{label_list}` in the active ontology"
                            ),
                        ));
                    }
                }
            }
        }

        issues
    }
}

/// Internal/system property keys reserved for the rewriter pipeline
/// (workspace isolation, soft-delete tombstones, future ACL markers).
/// The ontology never declares these, so the validator must treat them
/// as opaque rather than flag them as unknown.
fn is_system_property(key: &str) -> bool {
    key.starts_with('_')
}

// ---------------------------------------------------------------------------
// WorkspaceScopeValidator
// ---------------------------------------------------------------------------

/// Post-rewrite gate: every statement that touches graph data must
/// reference the workspace-scope property. If it doesn't, the rewriter
/// silently failed to inject isolation — a catastrophic bug worth
/// surfacing as a hard error before the query hits the DB.
///
/// Statements that genuinely have no graph surface (e.g. `RETURN 1`,
/// `UNWIND [1,2,3] AS x RETURN x`) are scope-neutral and pass — there's
/// no node or relationship for the rewriter to scope.
#[derive(Debug, Clone)]
pub struct WorkspaceScopeValidator {
    pub property: &'static str,
}

impl WorkspaceScopeValidator {
    pub const fn new(property: &'static str) -> Self {
        Self { property }
    }
}

impl CypherValidator for WorkspaceScopeValidator {
    fn name(&self) -> &str {
        "workspace-scope"
    }

    fn validate(&self, ast: &CypherAst, _ctx: &ValidateContext) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        for (i, statement) in ast.statements.iter().enumerate() {
            let touches_graph = statement
                .clauses
                .iter()
                .any(|c| c.kind.has_patterns() || c.kind.is_write());
            if !touches_graph {
                continue;
            }
            let mentions_scope = statement
                .clauses
                .iter()
                .any(|c| c.text.contains(self.property));
            if !mentions_scope {
                issues.push(ValidationIssue::error(
                    "workspace-scope",
                    format!(
                        "statement #{n} does not reference `{prop}` — workspace isolation was not applied",
                        n = i + 1,
                        prop = self.property,
                    ),
                ));
            }
        }
        issues
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_core::ontology_ir::{Cardinality, EdgeTypeDef, NodeTypeDef, PropertyDef};
    use ox_core::types::PropertyType;

    use crate::cypher::rewrite::{CypherRewriterPipeline, RewriteContext, WorkspaceScopeRewriter};

    // --- Fixtures --------------------------------------------------------

    fn person_company_ontology() -> OntologyIR {
        OntologyIR::new(
            "ont1".into(),
            "Test".into(),
            LocalizedText::default(),
            1,
            vec![
                NodeTypeDef {
                    id: "n1".into(),
                    label: "Person".into(),
                    description: LocalizedText::default(),
                    properties: vec![
                        PropertyDef {
                            id: "p1".into(),
                            name: "name".into(),
                            property_type: PropertyType::String,
                            nullable: false,
                            default_value: None,
                            description: LocalizedText::default(),
                            classification: None,
                            ..Default::default()
                        },
                        PropertyDef {
                            id: "p2".into(),
                            name: "age".into(),
                            property_type: PropertyType::Int,
                            nullable: true,
                            default_value: None,
                            description: LocalizedText::default(),
                            classification: None,
                            ..Default::default()
                        },
                    ],
                    constraints: vec![],
                    ..Default::default()
                },
                NodeTypeDef {
                    id: "n2".into(),
                    label: "Company".into(),
                    description: LocalizedText::default(),
                    properties: vec![PropertyDef {
                        id: "p3".into(),
                        name: "title".into(),
                        property_type: PropertyType::String,
                        nullable: false,
                        default_value: None,
                        description: LocalizedText::default(),
                        classification: None,
                        ..Default::default()
                    }],
                    constraints: vec![],
                    ..Default::default()
                },
            ],
            vec![EdgeTypeDef {
                id: "e1".into(),
                label: "WORKS_AT".into(),
                description: LocalizedText::default(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                properties: vec![],
                cardinality: Cardinality::ManyToOne,
                ..Default::default()
            }],
            vec![],
        )
    }

    fn run(pipeline: &CypherValidatorPipeline, q: &str) -> ValidationReport {
        pipeline.run(q, &ValidateContext::new("ws-123"))
    }

    // =====================================================================
    // SafetyValidator tests
    // =====================================================================

    #[test]
    fn safety_blocks_unrestricted_delete() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n:Person) DELETE n");
        assert!(r.has_errors(), "unrestricted DELETE must be blocked");
        assert!(r.errors().any(|e| e.message.contains("DELETE")));
    }

    #[test]
    fn safety_blocks_unrestricted_detach_delete() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n) DETACH DELETE n");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("DETACH DELETE")));
    }

    #[test]
    fn safety_blocks_unrestricted_remove() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n:Person) REMOVE n.age");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("REMOVE")));
    }

    #[test]
    fn safety_allows_delete_with_where() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n:Person) WHERE n.id = $id DELETE n");
        assert!(!r.has_errors(), "DELETE with WHERE must pass: {:?}", r.issues);
    }

    #[test]
    fn safety_allows_detach_delete_with_where() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n:Person) WHERE n.id = $id DETACH DELETE n");
        assert!(!r.has_errors());
    }

    #[test]
    fn safety_allows_remove_with_where() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n:Person) WHERE n.id = $id REMOVE n.age");
        assert!(!r.has_errors());
    }

    #[test]
    fn safety_blocks_drop_keyword() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "DROP CONSTRAINT person_name IF EXISTS");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("DROP")));
    }

    #[test]
    fn safety_blocks_drop_inside_statement() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n) WITH n CALL { DROP INDEX foo } RETURN n");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("DROP")));
    }

    #[test]
    fn safety_passes_pure_read() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n:Person) RETURN n");
        assert!(!r.has_errors(), "pure read must pass: {:?}", r.issues);
    }

    #[test]
    fn safety_passes_pure_create() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "CREATE (n:Person {name: 'Alice'})");
        assert!(!r.has_errors());
    }

    #[test]
    fn safety_issue_span_points_at_clause() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n) DELETE n");
        let err = r.errors().next().expect("expected an error");
        let span = err.span.expect("safety error must carry a span");
        assert!(span.end > span.start);
    }

    #[test]
    fn safety_detects_multiple_destructive_issues_independently() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        // Two destructive operations in one statement, no WHERE anywhere.
        let r = run(&p, "MATCH (n) REMOVE n.age DETACH DELETE n");
        assert!(r.errors().filter(|e| e.message.contains("DETACH DELETE")).count() >= 1);
        assert!(r.errors().filter(|e| e.message.contains("REMOVE")).count() >= 1);
    }

    // =====================================================================
    // OntologyValidator tests
    // =====================================================================

    #[test]
    fn ontology_accepts_known_labels_and_edges() {
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person)-[:WORKS_AT]->(c:Company) RETURN p, c");
        assert!(!r.has_errors(), "known entities must pass: {:?}", r.issues);
    }

    #[test]
    fn ontology_flags_unknown_node_label() {
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (u:Userr) RETURN u");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("Userr")));
    }

    #[test]
    fn ontology_flags_unknown_relationship_type() {
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person)-[:WORKS_FOR]->(c:Company) RETURN p");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("WORKS_FOR")));
    }

    #[test]
    fn ontology_flags_unknown_property_key_on_known_label() {
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person {emial: 'x'}) RETURN p");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("emial")));
    }

    #[test]
    fn ontology_accepts_known_inline_property() {
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person {name: 'Alice'}) RETURN p");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_ignores_system_properties() {
        // `_workspace_id` is injected by the rewriter; validator must
        // not flag it as an unknown property even when it appears on a
        // labelled node.
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person {_workspace_id: $ws}) RETURN p");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_ignores_unlabeled_node_properties() {
        // Without a label we cannot resolve the property — skip rather
        // than false-positive. A labelled sibling in the same pattern
        // takes care of the structural check.
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (n {name: 'x'}) RETURN n");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_multi_label_accepts_property_on_either_label() {
        // Multi-label `(n:Person:Company)` — the property need only
        // be defined on at least one label (label union).
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (n:Person:Company {title: 'x'}) RETURN n");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_reports_each_issue_separately() {
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(
            &p,
            "MATCH (u:Userr)-[:BADREL]->(v:Alsounknown) RETURN u",
        );
        let err_count = r.errors().count();
        assert!(err_count >= 3, "expected 3 errors, got {err_count}: {:?}", r.issues);
    }

    #[test]
    fn ontology_passes_non_pattern_statement() {
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "RETURN 1");
        assert!(!r.has_errors());
    }

    // =====================================================================
    // WorkspaceScopeValidator tests
    // =====================================================================

    const SCOPE_PROP: &str = "_workspace_id";

    #[test]
    fn scope_validator_flags_missing_scope_on_read() {
        let p = CypherValidatorPipeline::new().with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let r = run(&p, "MATCH (n:Person) RETURN n");
        assert!(r.has_errors(), "unscoped read must fail scope gate");
    }

    #[test]
    fn scope_validator_flags_missing_scope_on_create() {
        let p = CypherValidatorPipeline::new().with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let r = run(&p, "CREATE (n:Person {name: 'Alice'})");
        assert!(r.has_errors());
    }

    #[test]
    fn scope_validator_passes_after_workspace_rewrite() {
        // The whole point: rewrite then validate must pass.
        let rewriter = CypherRewriterPipeline::new()
            .with(WorkspaceScopeRewriter::new(SCOPE_PROP, "_ws_id"));
        let scoped = rewriter.run("MATCH (n:Person) RETURN n", &RewriteContext::new("ws"));
        let validator = CypherValidatorPipeline::new()
            .with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let r = validator.run(&scoped, &ValidateContext::new("ws"));
        assert!(!r.has_errors(), "post-rewrite must pass scope gate: {scoped} => {:?}", r.issues);
    }

    #[test]
    fn scope_validator_passes_non_graph_statement() {
        let p = CypherValidatorPipeline::new().with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let r = run(&p, "RETURN 1");
        assert!(!r.has_errors(), "no graph surface → no scope required");
    }

    #[test]
    fn scope_validator_checks_each_union_fragment_independently() {
        // One fragment scoped, the other not. Expect exactly one issue.
        let raw = "MATCH (a:A) WHERE a._workspace_id = $x RETURN a UNION MATCH (b:B) RETURN b";
        let p = CypherValidatorPipeline::new().with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let r = run(&p, raw);
        assert_eq!(r.errors().count(), 1, "{:?}", r.issues);
    }

    #[test]
    fn scope_validator_error_references_statement_index() {
        let raw = "MATCH (a:A) WHERE a._workspace_id = $x RETURN a UNION MATCH (b:B) RETURN b";
        let p = CypherValidatorPipeline::new().with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let r = run(&p, raw);
        let err = r.errors().next().unwrap();
        assert!(err.message.contains("#2"), "error should name statement #2: {}", err.message);
    }

    // =====================================================================
    // Pipeline composition + report surface
    // =====================================================================

    #[test]
    fn empty_pipeline_reports_no_issues() {
        let p = CypherValidatorPipeline::new();
        let r = run(&p, "MATCH (n) DELETE n");
        assert!(r.is_empty());
        assert!(!r.has_errors());
    }

    #[test]
    fn pipeline_aggregates_issues_across_validators() {
        // Safety + ontology in one pipeline — a query with both a
        // destructive write AND an unknown label produces both issues.
        let p = CypherValidatorPipeline::new()
            .with(SafetyValidator::new())
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (u:Userr) DELETE u");
        assert!(r.errors().any(|e| e.validator_name == "safety"));
        assert!(r.errors().any(|e| e.validator_name == "ontology"));
    }

    #[test]
    fn pipeline_preserves_validator_order_in_report() {
        let p = CypherValidatorPipeline::new()
            .with(SafetyValidator::new())
            .with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (u:Userr) DELETE u");
        // Safety errors should come before ontology errors in the report.
        let first_safety = r
            .issues
            .iter()
            .position(|i| i.validator_name == "safety")
            .unwrap();
        let first_onto = r
            .issues
            .iter()
            .position(|i| i.validator_name == "ontology")
            .unwrap();
        assert!(first_safety < first_onto, "{:?}", r.issues);
    }

    #[test]
    fn pipeline_len_and_is_empty() {
        let p0 = CypherValidatorPipeline::new();
        assert!(p0.is_empty());
        assert_eq!(p0.len(), 0);

        let p1 = CypherValidatorPipeline::new().with(SafetyValidator::new());
        assert!(!p1.is_empty());
        assert_eq!(p1.len(), 1);
    }

    #[test]
    fn run_ast_equivalent_to_run() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let query = "MATCH (n) DELETE n";
        let ast = parse(query);
        let from_ast = p.run_ast(&ast, &ValidateContext::new("ws"));
        let from_text = p.run(query, &ValidateContext::new("ws"));
        assert_eq!(from_ast.errors().count(), from_text.errors().count());
    }

    #[test]
    fn report_classifies_by_level() {
        let issues = vec![
            ValidationIssue::error("a", "e1"),
            ValidationIssue::warning("b", "w1"),
            ValidationIssue::info("c", "i1"),
            ValidationIssue::error("d", "e2"),
        ];
        let r = ValidationReport { issues };
        assert!(r.has_errors());
        assert_eq!(r.errors().count(), 2);
        assert_eq!(r.warnings().count(), 1);
        assert_eq!(r.infos().count(), 1);
    }

    #[test]
    fn issue_level_display_roundtrip() {
        assert_eq!(format!("{}", IssueLevel::Error), "error");
        assert_eq!(format!("{}", IssueLevel::Warning), "warning");
        assert_eq!(format!("{}", IssueLevel::Info), "info");
    }

    #[test]
    fn issue_with_span_carries_span() {
        let span = Span::new(3, 7);
        let issue = ValidationIssue::error("x", "msg").with_span(span);
        assert_eq!(issue.span, Some(span));
    }

    #[test]
    fn validator_name_appears_in_debug() {
        let p = CypherValidatorPipeline::new()
            .with(SafetyValidator::new())
            .with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let dbg = format!("{p:?}");
        assert!(dbg.contains("safety"));
        assert!(dbg.contains("workspace-scope"));
    }

    // =====================================================================
    // End-to-end: validate → rewrite → validate (the LLM flow)
    // =====================================================================

    #[test]
    fn e2e_llm_flow_valid_query_passes_all_gates() {
        // Pre-rewrite: safety + ontology.
        let pre = CypherValidatorPipeline::new()
            .with(SafetyValidator::new())
            .with(OntologyValidator::new(person_company_ontology()));
        let query = "MATCH (p:Person) WHERE p.name = $name RETURN p";
        let pre_report = pre.run(query, &ValidateContext::new("ws"));
        assert!(!pre_report.has_errors(), "{:?}", pre_report.issues);

        // Rewrite: inject workspace scope.
        let rewriter = CypherRewriterPipeline::new()
            .with(WorkspaceScopeRewriter::new(SCOPE_PROP, "_ws_id"));
        let scoped = rewriter.run(query, &RewriteContext::new("ws-123"));

        // Post-rewrite: scope gate.
        let post = CypherValidatorPipeline::new()
            .with(WorkspaceScopeValidator::new(SCOPE_PROP));
        let post_report = post.run(&scoped, &ValidateContext::new("ws"));
        assert!(!post_report.has_errors(), "{scoped} => {:?}", post_report.issues);
    }

    #[test]
    fn e2e_llm_flow_blocks_unsafe_query_before_rewrite() {
        let pre = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let report = pre.run("MATCH (n:Person) DELETE n", &ValidateContext::new("ws"));
        assert!(report.has_errors(), "pre-rewrite safety gate must catch unscoped DELETE");
    }

    #[test]
    fn e2e_llm_flow_catches_ontology_miss_before_rewrite() {
        let pre = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()));
        let report = pre.run(
            "MATCH (p:NonExistent) RETURN p",
            &ValidateContext::new("ws"),
        );
        assert!(report.has_errors(), "pre-rewrite ontology gate must catch unknown label");
    }
}
