//! Cypher validator pipeline.
//!
//! Companion to the [`crate::cypher::rewrite`] pipeline. A *rewriter*
//! mutates an AST and produces a new one; a *validator* inspects an AST
//! and produces diagnostics. Both share the same shape (trait +
//! ordered pipeline + context) on purpose — the two pipelines plug
//! into the same place in a query's life cycle.
//!
//! Two validators land here:
//!
//! - [`SafetyValidator`]: blocks destructive statements that lack a
//!   WHERE-bound scope (unrestricted DELETE / DETACH DELETE / REMOVE)
//!   and rejects DDL tokens (`DROP`) that must go through the schema
//!   migration path, not the runtime query path.
//! - [`OntologyValidator`]: verifies that every label, relationship
//!   type, and inline property key referenced in a pattern is defined
//!   on the active [`OntologyIR`]. Catches LLM hallucinations
//!   (`(u:Userr)` typos, invented properties) before they hit the DB.
//!
//! The old post-rewrite `WorkspaceScopeValidator` is gone. Its substring
//! probe ("does the rewritten text contain the scope property?") was a
//! weak approximation; the real check lives in
//! `bolt::pipeline::run_pre_execute`, which inspects
//! `ScopedAst.modified_statements` — a structural count produced by the
//! rewriter itself. Structural beats textual: a user-authored literal
//! `RETURN "_workspace_id"` no longer satisfies the gate by accident.
//!
//! The intended full flow for an LLM-generated query:
//!
//! ```text
//! parse → validate (safety + ontology)
//!   → if errors, reject
//!   → else rewrite (workspace-scope + future ACL / soft-delete)
//!   → check rewriter.modified_statements vs query_touches_graph
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

/// Phase ordering for validator passes.
///
/// Like [`crate::cypher::rewrite::RewritePhase`] but for validation. The
/// `bolt::pipeline` runs the pre-rewrite slots (`Safety` → `Ontology`)
/// before any rewriter; any future post-rewrite validator would run
/// in the `PostRewrite` slot after every rewriter has settled. Numeric
/// gaps leave room for new slots without renumbering the existing
/// ones.
///
/// The current set:
///
/// - `PreRewriteSafety` — hard blocks on destructive or DDL constructs.
/// - `PreRewriteOntology` — schema conformance of labels / properties /
///   relationship types against the active `OntologyIR`.
/// - `PostRewrite` — inspections that require rewriters to have run
///   first (none shipping today — the old substring scope gate was
///   replaced by the structural `modified_statements` check in the
///   runtime pipeline).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum ValidatePhase {
    PreRewriteSafety = 100,
    PreRewriteOntology = 200,
    PostRewrite = 900,
}

/// A single Cypher validation pass. Unlike [`crate::cypher::rewrite::CypherRewriter`],
/// validators never mutate the AST — they only inspect it.
pub trait CypherValidator: Send + Sync {
    /// Identifier used in diagnostics and logs.
    fn name(&self) -> &str;

    /// Slot this validator runs in. Same semantics as
    /// [`crate::cypher::rewrite::CypherRewriter::phase`] — pipeline
    /// stable-sorts by this value. Default is `PreRewriteOntology`
    /// because every in-tree validator lands in a pre-rewrite slot
    /// today; an out-of-tree post-rewrite validator must override.
    fn phase(&self) -> ValidatePhase {
        ValidatePhase::PreRewriteOntology
    }

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
    /// [`ValidationReport`]. Validators are sorted by
    /// [`CypherValidator::phase`] (stable) before execution, so the
    /// caller doesn't need to worry about the order they called
    /// `.with()` in; within each phase, issues appear in registration
    /// order.
    pub fn run_ast(&self, ast: &CypherAst, ctx: &ValidateContext) -> ValidationReport {
        let mut order: Vec<usize> = (0..self.validators.len()).collect();
        order.sort_by_key(|&i| self.validators[i].phase());

        let mut issues = Vec::new();
        for idx in order {
            issues.extend(self.validators[idx].validate(ast, ctx));
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

    fn phase(&self) -> ValidatePhase {
        ValidatePhase::PreRewriteSafety
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
                    issues.push(ValidationIssue::error("safety", msg).with_span(clause.span));
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

    fn phase(&self) -> ValidatePhase {
        ValidatePhase::PreRewriteOntology
    }

    fn validate(&self, ast: &CypherAst, _ctx: &ValidateContext) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        let known_edge_labels: HashSet<&str> = self
            .ontology
            .edge_types()
            .iter()
            .map(|e| e.label.as_str())
            .collect();

        // Each unknown identifier is reported once at its first occurrence.
        // Repeating the same diagnostic for every MATCH that re-uses a bad
        // label drowns the real issues; holding the dedup set per-pass
        // keeps the report tight while still pointing the IDE at the
        // first site it can highlight.
        let mut seen_unknown_labels: HashSet<String> = HashSet::new();
        let mut seen_unknown_rels: HashSet<String> = HashSet::new();

        for element in ast.pattern_elements() {
            match element {
                // --- Node pattern: label + inline property keys --------
                //
                // `(u:User {email: $e})` — verify `User` is declared and
                // that `email` is defined on it. A multi-label node is a
                // union (Cypher semantics), so the property check passes
                // when any label declares it.
                CypherPatternElement::Node(node) => {
                    for label in &node.labels {
                        if self.ontology.node_by_label(label).is_none()
                            && seen_unknown_labels.insert(label.clone())
                        {
                            issues.push(
                                ValidationIssue::error(
                                    "ontology",
                                    format!(
                                        "unknown node label `{label}` — not defined in the active ontology"
                                    ),
                                )
                                .with_span(node.span),
                            );
                        }
                    }
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
                            issues.push(
                                ValidationIssue::error(
                                    "ontology",
                                    format!(
                                        "property `{key}` not defined on label `{label_list}` in the active ontology"
                                    ),
                                )
                                .with_span(node.span),
                            );
                        }
                    }
                }
                // --- Relationship pattern: type -----------------------
                CypherPatternElement::Relationship(rel) => {
                    for rel_type in &rel.types {
                        if !known_edge_labels.contains(rel_type.as_str())
                            && seen_unknown_rels.insert(rel_type.clone())
                        {
                            issues.push(
                                ValidationIssue::error(
                                    "ontology",
                                    format!(
                                        "unknown relationship type `{rel_type}` — not defined in the active ontology"
                                    ),
                                )
                                .with_span(rel.span),
                            );
                        }
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::GraphLabel;
    use ox_core::i18n::LocalizedText;
    use ox_core::ontology_ir::{Cardinality, EdgeTypeDef, NodeTypeDef, PropertyDef};
    use ox_core::types::PropertyType;

    use crate::cypher::rewrite::{CypherRewriterPipeline, RewriteContext, WorkspaceScopeRewriter};

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label literal must be valid")
    }

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
                    label: gl("Person"),
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
                    label: gl("Company"),
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
                label: gl("WORKS_AT"),
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
        assert!(
            !r.has_errors(),
            "DELETE with WHERE must pass: {:?}",
            r.issues
        );
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
        assert!(
            r.errors()
                .filter(|e| e.message.contains("DETACH DELETE"))
                .count()
                >= 1
        );
        assert!(r.errors().filter(|e| e.message.contains("REMOVE")).count() >= 1);
    }

    // =====================================================================
    // OntologyValidator tests
    // =====================================================================

    #[test]
    fn ontology_accepts_known_labels_and_edges() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person)-[:WORKS_AT]->(c:Company) RETURN p, c");
        assert!(!r.has_errors(), "known entities must pass: {:?}", r.issues);
    }

    #[test]
    fn ontology_flags_unknown_node_label() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (u:Userr) RETURN u");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("Userr")));
    }

    #[test]
    fn ontology_flags_unknown_relationship_type() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person)-[:WORKS_FOR]->(c:Company) RETURN p");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("WORKS_FOR")));
    }

    #[test]
    fn ontology_flags_unknown_property_key_on_known_label() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person {emial: 'x'}) RETURN p");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.contains("emial")));
    }

    #[test]
    fn ontology_accepts_known_inline_property() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person {name: 'Alice'}) RETURN p");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_ignores_system_properties() {
        // `_workspace_id` is injected by the rewriter; validator must
        // not flag it as an unknown property even when it appears on a
        // labelled node.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person {_workspace_id: $ws}) RETURN p");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_ignores_unlabeled_node_properties() {
        // Without a label we cannot resolve the property — skip rather
        // than false-positive. A labelled sibling in the same pattern
        // takes care of the structural check.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (n {name: 'x'}) RETURN n");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_multi_label_accepts_property_on_either_label() {
        // Multi-label `(n:Person:Company)` — the property need only
        // be defined on at least one label (label union).
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (n:Person:Company {title: 'x'}) RETURN n");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn ontology_reports_each_issue_separately() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (u:Userr)-[:BADREL]->(v:Alsounknown) RETURN u");
        let err_count = r.errors().count();
        assert!(
            err_count >= 3,
            "expected 3 errors, got {err_count}: {:?}",
            r.issues
        );
    }

    #[test]
    fn ontology_passes_non_pattern_statement() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "RETURN 1");
        assert!(!r.has_errors());
    }

    #[test]
    fn ontology_errors_carry_spans_for_editor_highlighting() {
        // Every ontology error must carry a span. A missing span here
        // would send `None` to the editor and nothing would light up
        // in the gutter — the whole point of the error is to point the
        // user at the offending token.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (u:Userr)-[:BOGUS]->(c:Company) RETURN u");
        assert!(r.has_errors());
        for issue in r.errors() {
            assert!(
                issue.span.is_some(),
                "every ontology error must carry a span; offender: {issue:?}"
            );
        }
    }

    #[test]
    fn pipeline_sorts_validators_by_phase_regardless_of_registration_order() {
        // Ontology validator (PreRewriteOntology, 200) registered first;
        // safety validator (PreRewriteSafety, 100) registered second.
        // Even so, safety errors must appear before ontology errors in
        // the report — the runtime relies on this ordering when
        // aggregating issues for the LLM retry message.
        let p = CypherValidatorPipeline::new()
            .with(OntologyValidator::new(person_company_ontology()))
            .with(SafetyValidator::new());
        let r = run(&p, "MATCH (u:Userr) DELETE u");
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
        assert!(
            first_safety < first_onto,
            "PreRewriteSafety must sort ahead of PreRewriteOntology even \
             when safety is registered second: {:?}",
            r.issues
        );
    }

    #[test]
    fn ontology_dedupes_repeated_unknown_labels() {
        // Same unknown label in three different MATCH statements should
        // produce one error, not three — otherwise the report drowns
        // in duplicates for a query with a typo that appears repeatedly.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(
            &p,
            "MATCH (a:Userr) MATCH (b:Userr) MATCH (c:Userr) RETURN a, b, c",
        );
        let count = r.errors().filter(|e| e.message.contains("Userr")).count();
        assert_eq!(count, 1, "unknown label should report once: {:?}", r.issues);
    }

    const SCOPE_PROP: &str = "_workspace_id";

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
            .with(OntologyValidator::new(person_company_ontology()));
        let dbg = format!("{p:?}");
        assert!(dbg.contains("safety"));
        assert!(dbg.contains("ontology"));
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

        // Rewrite: inject workspace scope. The rewriter's
        // `modified_statements` count is what the runtime checks after
        // this pass — the old textual `WorkspaceScopeValidator` is gone.
        let rewriter =
            CypherRewriterPipeline::new().with(WorkspaceScopeRewriter::new(SCOPE_PROP, "_ws_id"));
        let rewritten = rewriter
            .run_ast(parse(query), &RewriteContext::new("ws-123"))
            .expect("workspace-scope rewriter has no failure cases");
        assert!(
            rewritten.modified_statements >= 1,
            "a graph-touching query must produce at least one modified statement"
        );
        let scoped = rewritten.ast.render();
        assert!(
            scoped.contains("_workspace_id = $_ws_id"),
            "rewritten query should carry the scope predicate: {scoped}"
        );
    }

    #[test]
    fn e2e_llm_flow_blocks_unsafe_query_before_rewrite() {
        let pre = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let report = pre.run("MATCH (n:Person) DELETE n", &ValidateContext::new("ws"));
        assert!(
            report.has_errors(),
            "pre-rewrite safety gate must catch unscoped DELETE"
        );
    }

    #[test]
    fn e2e_llm_flow_catches_ontology_miss_before_rewrite() {
        let pre =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let report = pre.run(
            "MATCH (p:NonExistent) RETURN p",
            &ValidateContext::new("ws"),
        );
        assert!(
            report.has_errors(),
            "pre-rewrite ontology gate must catch unknown label"
        );
    }
}
