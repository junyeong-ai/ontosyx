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

use ox_core::diagnostic::{diag, DiagnosticMessage};
use ox_ontology::ir::OntologyIR;

use crate::cypher::ast::{ClauseKind, CypherAst, CypherClause, CypherPatternElement};
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
///
/// `message` is a structured [`DiagnosticMessage`] (RFC 7807 / gRPC
/// `Status` shape): a stable `code`, an English `message` rendering,
/// and a `params` map. The FE resolves `code` + `params` through its
/// i18n catalogue (`next-intl` ICU MessageFormat) so adding a UI
/// language never touches the validator emit sites.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub level: IssueLevel,
    pub message: DiagnosticMessage,
    pub span: Option<Span>,
    pub validator_name: String,
}

impl ValidationIssue {
    pub fn error(validator: impl Into<String>, message: DiagnosticMessage) -> Self {
        Self {
            level: IssueLevel::Error,
            message,
            span: None,
            validator_name: validator.into(),
        }
    }

    pub fn warning(validator: impl Into<String>, message: DiagnosticMessage) -> Self {
        Self {
            level: IssueLevel::Warning,
            message,
            span: None,
            validator_name: validator.into(),
        }
    }

    pub fn info(validator: impl Into<String>, message: DiagnosticMessage) -> Self {
        Self {
            level: IssueLevel::Info,
            message,
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
/// Like [`crate::cypher::rewrite::RewritePhase`] but for validation.
/// The `bolt::pipeline` runs both slots **before** any rewriter —
/// validators never inspect the rewritten AST because the rewriter
/// injects workspace-scope predicates (`WHERE _workspace_id = $_ws_id`)
/// that would otherwise trigger false ontology errors ("unknown
/// property `_workspace_id`"). Structural post-rewrite invariants
/// (scope-propagation count, property-strategy completeness) live
/// on `run_pre_execute` directly and don't go through this trait.
///
/// If a future use case genuinely needs a validator that sees the
/// rewritten AST, it will need a new post-rewrite dispatch point
/// on `bolt::pipeline` — a new enum variant here alone wouldn't
/// wire it, because the pipeline only feeds this trait the pre-
/// rewrite AST.
///
/// Numeric gaps leave room for new slots without renumbering.
///
/// The current set:
///
/// - `PreRewriteSafety` — hard blocks on destructive or DDL constructs.
/// - `PreRewriteOntology` — schema conformance of labels / properties /
///   relationship types against the active `OntologyIR`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum ValidatePhase {
    PreRewriteSafety = 100,
    PreRewriteOntology = 200,
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
        use crate::cypher::system_properties::SYSTEM_PROPERTIES;
        use crate::cypher::token::TokenKind;

        let mut issues = Vec::new();

        for statement in &ast.statements {
            let has_where = statement
                .clauses
                .iter()
                .any(|c| c.kind == ClauseKind::Where);

            for clause in &statement.clauses {
                let destructive = match clause.kind {
                    ClauseKind::Delete => Some((
                        "runtime.cypher.safety.delete_without_where",
                        "DELETE without WHERE is not allowed — add a predicate bounding the deletion",
                    )),
                    ClauseKind::DetachDelete => Some((
                        "runtime.cypher.safety.detach_delete_without_where",
                        "DETACH DELETE without WHERE is not allowed — add a predicate bounding the deletion",
                    )),
                    ClauseKind::Remove => Some((
                        "runtime.cypher.safety.remove_without_where",
                        "REMOVE without WHERE is not allowed — add a predicate bounding the affected nodes",
                    )),
                    _ => None,
                };
                if let Some((code, msg)) = destructive
                    && !has_where
                {
                    issues.push(
                        ValidationIssue::error("safety", diag(code).message(msg))
                            .with_span(clause.span),
                    );
                }

                for tok in &clause.tokens {
                    if tok.is_keyword("DROP") {
                        issues.push(
                            ValidationIssue::error(
                                "safety",
                                diag("runtime.cypher.safety.drop_disallowed").message(
                                    "DROP is not allowed in runtime queries — schema changes go through the migration path",
                                ),
                            )
                            .with_span(tok.span),
                        );
                    }
                }

                // Reserved system properties — `_workspace_id`,
                // `_deleted_at` — are owned by the rewriter pipeline.
                // A user query that writes them directly would either
                // spoof workspace isolation (`SET n._workspace_id =
                // 'other_ws'`) or shadow the tombstone marker
                // (`SET n._deleted_at = NULL`). Reject every write
                // shape: `SET <var>.<prop>`, `CREATE (n {<prop>: …})`,
                // and `MERGE (n {<prop>: …}) ON {CREATE|MATCH} SET …`.
                if matches!(
                    clause.kind,
                    ClauseKind::Set | ClauseKind::Create | ClauseKind::Merge
                ) {
                    let non_trivia: Vec<&_> = clause
                        .tokens
                        .iter()
                        .filter(|t| !t.is_trivia())
                        .collect();
                    for window in non_trivia.windows(3) {
                        let dot = window[1];
                        if !(dot.kind == TokenKind::Operator && dot.text == ".") {
                            continue;
                        }
                        let prop_token = window[2];
                        if !matches!(
                            prop_token.kind,
                            TokenKind::Identifier | TokenKind::QuotedIdentifier
                        ) {
                            continue;
                        }
                        let prop_text = prop_token.text.trim_matches('`');
                        if SYSTEM_PROPERTIES.contains(&prop_text) {
                            issues.push(
                                ValidationIssue::error(
                                    "safety",
                                    diag("runtime.cypher.safety.system_property_write")
                                        .with("property", prop_text)
                                        .message(format!(
                                            "`{prop_text}` is a system-reserved property and cannot be written by a user query"
                                        )),
                                )
                                .with_span(prop_token.span),
                            );
                        }
                    }
                    // Inline property syntax — `(n {_workspace_id: 'x'})`
                    // and `[r {…}]`. The parser populates
                    // `pattern.element.properties` with the (key,
                    // raw_value) pairs, so the check is structural.
                    for pattern in &clause.patterns {
                        for element in &pattern.elements {
                            let props = match element {
                                CypherPatternElement::Node(n) => &n.properties,
                                CypherPatternElement::Relationship(r) => &r.properties,
                            };
                            for (key, _value) in props {
                                let key_clean = key.trim_matches('`');
                                if SYSTEM_PROPERTIES.contains(&key_clean) {
                                    issues.push(ValidationIssue::error(
                                        "safety",
                                        diag("runtime.cypher.safety.system_property_write")
                                            .with("property", key_clean)
                                            .message(format!(
                                                "`{key_clean}` is a system-reserved property and cannot be written by a user query"
                                            )),
                                    ));
                                }
                            }
                        }
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
        // SET / REMOVE diagnostics dedup on (variable, property) — the
        // same offending key on the same variable can appear in multiple
        // clauses (`SET u.emial = 'x'` then `SET u.emial = 'y'`); one
        // issue per pair keeps the report scoped to the typo, not its
        // multiplicity.
        let mut seen_unknown_set_props: HashSet<(String, String)> = HashSet::new();
        let mut seen_unknown_remove_props: HashSet<(String, String)> = HashSet::new();

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
                                    diag("runtime.cypher.ontology.unknown_node_label")
                                        .with("label", label.clone())
                                        .message(format!(
                                            "unknown node label `{label}` — not defined in the active ontology"
                                        )),
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
                                    diag("runtime.cypher.ontology.unknown_property")
                                        .with("property", key.clone())
                                        .with("labels", label_list.clone())
                                        .message(format!(
                                            "property `{key}` not defined on label `{label_list}` in the active ontology"
                                        )),
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
                                    diag("runtime.cypher.ontology.unknown_relationship_type")
                                        .with("relationship_type", rel_type.clone())
                                        .message(format!(
                                            "unknown relationship type `{rel_type}` — not defined in the active ontology"
                                        )),
                                )
                                .with_span(rel.span),
                            );
                        }
                    }
                }
            }
        }

        // SET / REMOVE: walk every assignment / removal and verify the
        // property exists on at least one of the variable's bound
        // labels. An LLM emitting `SET u.emial = 'x'` (typo) currently
        // slips past the schema — Cypher is schema-less, so the
        // driver silently writes a new property and the graph quietly
        // drifts. The variable→labels resolution is shared with the
        // SHACL validator (`CypherStatement::variable_labels`); when
        // the variable is unbound (e.g. introduced by WITH or UNWIND)
        // we cannot decide ontologically, so we skip rather than
        // false-flag.
        for statement in &ast.statements {
            let variable_labels = statement.variable_labels();
            for clause in &statement.clauses {
                match clause.kind {
                    ClauseKind::Set => {
                        for item in &clause.set_items {
                            if is_system_property(&item.property) {
                                continue;
                            }
                            let Some(labels) = variable_labels.get(&item.variable) else {
                                continue;
                            };
                            let known = labels.iter().any(|label| {
                                self.ontology.node_by_label(label).is_some_and(|n| {
                                    n.properties.iter().any(|p| p.name == item.property)
                                })
                            });
                            if !known
                                && seen_unknown_set_props
                                    .insert((item.variable.clone(), item.property.clone()))
                            {
                                let label_list = labels.join("/");
                                issues.push(
                                    ValidationIssue::error(
                                        "ontology",
                                        diag("runtime.cypher.ontology.unknown_set_property")
                                            .with("property", item.property.clone())
                                            .with("variable", item.variable.clone())
                                            .with("labels", label_list.clone())
                                            .message(format!(
                                                "SET assigns to property `{}` not defined on label `{label_list}` in the active ontology",
                                                item.property,
                                            )),
                                    )
                                    .with_span(item.span),
                                );
                            }
                        }
                    }
                    ClauseKind::Remove => {
                        for item in &clause.remove_items {
                            if is_system_property(&item.property) {
                                continue;
                            }
                            let Some(labels) = variable_labels.get(&item.variable) else {
                                continue;
                            };
                            let known = labels.iter().any(|label| {
                                self.ontology.node_by_label(label).is_some_and(|n| {
                                    n.properties.iter().any(|p| p.name == item.property)
                                })
                            });
                            if !known
                                && seen_unknown_remove_props
                                    .insert((item.variable.clone(), item.property.clone()))
                            {
                                let label_list = labels.join("/");
                                issues.push(
                                    ValidationIssue::error(
                                        "ontology",
                                        diag("runtime.cypher.ontology.unknown_remove_property")
                                            .with("property", item.property.clone())
                                            .with("variable", item.variable.clone())
                                            .with("labels", label_list.clone())
                                            .message(format!(
                                                "REMOVE targets property `{}` not defined on label `{label_list}` in the active ontology",
                                                item.property,
                                            )),
                                    )
                                    .with_span(item.span),
                                );
                            }
                        }
                    }
                    _ => {}
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
// SemanticGuardValidator
// ---------------------------------------------------------------------------

/// Strengthens [`SafetyValidator`]'s destructive-write gate by
/// inspecting the *content* of the WHERE clause — not just its
/// presence.
///
/// SafetyValidator refuses `DELETE` / `DETACH DELETE` / `REMOVE`
/// without a WHERE somewhere in the statement. That check is
/// structural and cheap; it intentionally doesn't try to reason
/// about the predicate. An LLM that wrote a naked `DELETE` and got
/// back "add a WHERE" can slip past by appending `WHERE true` — the
/// structural gate opens, the nothing-constrains predicate leaves
/// every row exposed, and the request bulk-deletes the workspace.
///
/// This validator catches the trivial tautology cases. The
/// detection is token-level (not a full expression parser) and
/// targets the forms an LLM-generated "make the validator happy"
/// retry would emit:
///
/// - `WHERE true` (case-insensitive)
/// - `WHERE NOT false`
/// - `WHERE <same-literal> = <same-literal>` — `1 = 1`, `'x' = 'x'`,
///   `$p = $p`
///
/// A determined adversary can slip past (e.g. `WHERE 1 + 0 = 1`)
/// but the goal here isn't a full SMT solver — it's raising the bar
/// high enough that LLM-default misuse stops working. Legitimate
/// WHERE clauses with real predicates (`WHERE n.id = $id`) pass
/// through.
///
/// Phase `PreRewriteSafety` — same slot as `SafetyValidator`; the
/// two run together so a destructive op with a tautological WHERE
/// receives one consolidated "this delete has no effective bound"
/// error rather than two near-duplicates.
#[derive(Debug, Clone, Default)]
pub struct SemanticGuardValidator;

impl SemanticGuardValidator {
    pub const fn new() -> Self {
        Self
    }
}

impl CypherValidator for SemanticGuardValidator {
    fn name(&self) -> &str {
        "semantic-guard"
    }

    fn phase(&self) -> ValidatePhase {
        ValidatePhase::PreRewriteSafety
    }

    fn validate(&self, ast: &CypherAst, _ctx: &ValidateContext) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        for statement in &ast.statements {
            // Statement-level summary: which destructive clauses appear,
            // and do the WHERE clauses (if any) actually constrain?
            let has_destructive = statement.clauses.iter().any(|c| {
                matches!(
                    c.kind,
                    ClauseKind::Delete | ClauseKind::DetachDelete | ClauseKind::Remove
                )
            });
            if !has_destructive {
                continue;
            }

            // At least one WHERE must carry a real predicate. We scan
            // every WHERE and flag only when *every* WHERE is
            // tautological — that matches the user's intent: a
            // selective MATCH-WHERE followed by a DELETE is safe,
            // and we only fire when the gating predicate actually
            // gates nothing.
            let where_clauses: Vec<&CypherClause> = statement
                .clauses
                .iter()
                .filter(|c| c.kind == ClauseKind::Where)
                .collect();

            if where_clauses.is_empty() {
                // SafetyValidator already flags this case; don't emit
                // a duplicate "no predicate" error from a different
                // validator.
                continue;
            }

            if where_clauses
                .iter()
                .all(|w| is_tautological_where(&w.tokens))
            {
                // Use the destructive clause's span — that's what the
                // editor should underline since the WHERE's emptiness
                // is a property *of the delete*, not of the WHERE
                // itself.
                let destructive_span = statement
                    .clauses
                    .iter()
                    .find(|c| {
                        matches!(
                            c.kind,
                            ClauseKind::Delete | ClauseKind::DetachDelete | ClauseKind::Remove
                        )
                    })
                    .map(|c| c.span);
                let mut issue = ValidationIssue::error(
                    "semantic-guard",
                    diag("runtime.cypher.semantic_guard.tautological_where").message(
                        "destructive operation is gated only by a tautological WHERE predicate \
                         (e.g. `WHERE true`, `WHERE 1 = 1`) — add a real constraint (a property \
                         filter on the matched variables) before DELETE / DETACH DELETE / REMOVE",
                    ),
                );
                if let Some(span) = destructive_span {
                    issue = issue.with_span(span);
                }
                issues.push(issue);
            }
        }

        issues
    }
}

/// Is the sequence of WHERE-clause tokens a trivial tautology?
/// Token-level heuristic — deliberately simple, deliberately narrow.
///
/// Recognises:
/// - `WHERE true`
/// - `WHERE NOT false`
/// - `WHERE <lit> = <lit>` with the two literals identical
/// - `WHERE <var> = <var>` — bare identifier self-reference
/// - `WHERE <var>.<key> = <var>.<key>` — property self-reference
///   (LLMs occasionally emit `n.id = n.id` as a "safe" predicate)
///
/// The WHERE keyword itself is the clause's first significant token;
/// we strip it + whitespace/comments and then inspect the remainder
/// as a vector. Anything outside the recognised patterns returns
/// false — validator stays narrow on purpose.
fn is_tautological_where(tokens: &[crate::cypher::token::CypherToken]) -> bool {
    use crate::cypher::token::{CypherToken, TokenKind};

    // Keep only tokens the heuristic cares about. Whitespace,
    // comments, and a leading WHERE keyword are noise.
    let significant: Vec<&CypherToken> = tokens
        .iter()
        .filter(|t| {
            !matches!(
                t.kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
            )
        })
        .skip_while(|t| t.is_keyword("WHERE"))
        .collect();

    match significant.len() {
        // `WHERE` followed by nothing — empty predicate, vacuously
        // "no constraint".
        0 => return true,
        // `WHERE true`
        1 => return significant[0].is_keyword("TRUE"),
        // `WHERE NOT false`
        2 => {
            return significant[0].is_keyword("NOT")
                && significant[1].is_keyword("FALSE");
        }
        // `WHERE <lit> = <lit>` | `WHERE <var> = <var>` — single
        // equality, both sides single-token.
        3 => {
            return is_equality(significant[1])
                && is_self_equality_operand(significant[0])
                && token_equal(significant[0], significant[2]);
        }
        _ => {}
    }

    // `WHERE <var>.<key> = <var>.<key>` — property access on both
    // sides with matching variable + property key.
    if significant.len() == 7 {
        let lhs = &significant[0..3];
        let op = significant[3];
        let rhs = &significant[4..7];
        if is_equality(op) && is_property_access(lhs) && is_property_access(rhs) {
            return token_equal(lhs[0], rhs[0]) && token_equal(lhs[2], rhs[2]);
        }
    }

    false
}

fn is_equality(tok: &crate::cypher::token::CypherToken) -> bool {
    use crate::cypher::token::TokenKind;
    tok.kind == TokenKind::Operator && tok.text == "="
}

/// Operand shapes the "`WHERE <x> = <x>`" rule accepts on each side:
/// identifiers (variable names) or literals (number / string / parameter).
/// Rejecting operators or punctuation keeps `=`/`.` from slipping in as
/// a false operand match.
fn is_self_equality_operand(tok: &crate::cypher::token::CypherToken) -> bool {
    use crate::cypher::token::TokenKind;
    matches!(
        tok.kind,
        TokenKind::Identifier
            | TokenKind::QuotedIdentifier
            | TokenKind::Number
            | TokenKind::StringLiteral
            | TokenKind::Parameter
    )
}

/// `<Identifier><.><Identifier>` triple — the tokenizer treats `.` as
/// an Operator, so a property access is exactly three tokens.
fn is_property_access(run: &[&crate::cypher::token::CypherToken]) -> bool {
    use crate::cypher::token::TokenKind;
    run.len() == 3
        && matches!(run[0].kind, TokenKind::Identifier | TokenKind::QuotedIdentifier)
        && run[1].kind == TokenKind::Operator
        && run[1].text == "."
        && matches!(run[2].kind, TokenKind::Identifier | TokenKind::QuotedIdentifier)
}

fn token_equal(
    a: &crate::cypher::token::CypherToken,
    b: &crate::cypher::token::CypherToken,
) -> bool {
    a.kind == b.kind && a.text == b.text
}

// ---------------------------------------------------------------------------
// ComplexityValidator
// ---------------------------------------------------------------------------

/// Flag query shapes that commonly blow up in execution time without the
/// author noticing: an unbounded variable-length path (`MATCH
/// (a)-[*]->(b)`), a disconnected MATCH that fans out into a cartesian
/// product, or a comma-separated pattern list whose components share
/// no variables.
///
/// ## Policy
///
/// - **Unbounded var-length** (`*` with no upper bound, or
///   `*{min}..` / `*..{max}` where only one side is pinned but no
///   cap lands inside a small sanity window): every hop depth after
///   ~5 materialises a cartesian blow-up on typical graphs. Emitted
///   as `IssueLevel::Error` when `reject_unbounded` is `true` (the
///   default — this catches the LLM-generated tier of queries that
///   meant `*1..5`), or `Warning` when the validator is configured
///   in permissive mode for power users.
/// - **Cartesian components** within a single MATCH clause (e.g.
///   `MATCH (a:A), (b:B) RETURN a, b`). Every comma-separated
///   pattern is a join boundary; two patterns that share no variable
///   and no reachable path between them is the canonical cartesian
///   footgun. Always `IssueLevel::Error` — a query that wanted a
///   cross product should say so explicitly via `CROSS JOIN`-shaped
///   intermediates or compute both sides separately.
/// - **Cross-clause disconnection** (multiple MATCH clauses whose
///   aggregate variable sets don't overlap) — `IssueLevel::Warning`.
///   Less immediately wrong than within-clause disconnection; a
///   pipeline like `MATCH (a), WITH …, MATCH (b)` is legal when the
///   WITH carries shared state.
///
/// ## Why inside the pre-execute pipeline and not the compiler?
///
/// The Cypher compiler already emits conservative plans. What the
/// compiler cannot catch is LLM-authored Cypher that threaded
/// around the compiler's checks by passing free-form text (the
/// `raw_query` path). The validator lands at the gate every Bolt
/// execution crosses, so the LLM hallucination path gets caught
/// regardless of whether it came through a compiled QueryIR or a
/// direct Cypher string.
#[derive(Debug, Clone)]
pub struct ComplexityValidator {
    /// When `true`, an unbounded variable-length path emits an Error.
    /// When `false`, it emits a Warning so power-user workflows
    /// (ad-hoc graph exploration with explicit caps) aren't blocked.
    /// Default: `true` — the common case is an LLM-generated query
    /// that meant to pin an upper bound and forgot.
    pub reject_unbounded: bool,
}

impl Default for ComplexityValidator {
    fn default() -> Self {
        Self {
            reject_unbounded: true,
        }
    }
}

impl ComplexityValidator {
    pub const fn new() -> Self {
        Self {
            reject_unbounded: true,
        }
    }

    /// Permissive mode: unbounded var-length downgrades from Error to
    /// Warning. Cartesian detection stays Error either way.
    pub const fn permissive() -> Self {
        Self {
            reject_unbounded: false,
        }
    }
}

impl CypherValidator for ComplexityValidator {
    fn name(&self) -> &str {
        "complexity"
    }

    fn phase(&self) -> ValidatePhase {
        // Runs alongside Ontology — both are pre-rewrite structural
        // checks that don't mutate the AST.
        ValidatePhase::PreRewriteOntology
    }

    fn validate(&self, ast: &CypherAst, _ctx: &ValidateContext) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        for statement in &ast.statements {
            // --- Per-clause cartesian within a single MATCH --------
            //
            // A clause's `patterns: Vec<CypherPattern>` holds every
            // comma-separated sub-pattern. We compute the variable set
            // for each and flag any pair that shares no variable with
            // the others. Single-pattern clauses skip the check.
            for clause in &statement.clauses {
                if clause.patterns.len() < 2 {
                    continue;
                }
                if !clause.kind.has_patterns() {
                    // Only MATCH / OPTIONAL MATCH / CREATE / MERGE
                    // carry patterns; skip defensively.
                    continue;
                }
                let components: Vec<std::collections::HashSet<String>> = clause
                    .patterns
                    .iter()
                    .map(pattern_variables)
                    .collect();
                if !components_all_connected(&components) {
                    issues.push(
                        ValidationIssue::error(
                            "complexity",
                            diag("runtime.cypher.complexity.cartesian_within_clause").message(
                                "comma-separated MATCH patterns share no variables — this is a cartesian product; \
                                 break into separate MATCHes joined by WITH, or add a shared variable",
                            ),
                        )
                        .with_span(clause.span),
                    );
                }
            }

            // --- Cross-clause disconnection (warning) --------------
            //
            // When a statement has multiple MATCH / OPTIONAL MATCH
            // clauses and their aggregate variable sets are
            // completely disjoint, execution still crosses a
            // cartesian boundary. A WITH clause between them is
            // fine — it carries shared state and we trust the author;
            // the warning is only for the plain "two MATCHes, no
            // shared variable" shape.
            issues.extend(flag_cross_clause_disconnection(statement));

            // --- Unbounded variable-length path --------------------
            for element in statement.clauses.iter().flat_map(|c| {
                c.patterns
                    .iter()
                    .flat_map(|p| p.elements.iter())
            }) {
                if let CypherPatternElement::Relationship(rel) = element
                    && is_unbounded_var_length(&rel.var_length)
                {
                    let d = diag("runtime.cypher.complexity.unbounded_var_length").message(
                        "variable-length relationship has no upper bound — pin a max depth \
                         (e.g. `*1..5`) to avoid unbounded traversal",
                    );
                    let issue = if self.reject_unbounded {
                        ValidationIssue::error("complexity", d)
                    } else {
                        ValidationIssue::warning("complexity", d)
                    };
                    issues.push(issue.with_span(rel.span));
                }
            }
        }

        issues
    }
}

/// Collect every variable bound by a single pattern — nodes' `variable:
/// Option<String>` plus relationships' `variable: Option<String>`. An
/// anonymous pattern element contributes nothing, which is correct:
/// `(:A)-[:X]->(:B)` with no bindings can never share a variable with
/// any other pattern, so the caller will flag it as disconnected even
/// if both elements are, physically, on the same path.
fn pattern_variables(pattern: &crate::cypher::ast::CypherPattern) -> std::collections::HashSet<String> {
    let mut vars = std::collections::HashSet::new();
    for el in &pattern.elements {
        match el {
            CypherPatternElement::Node(n) => {
                if let Some(v) = &n.variable {
                    vars.insert(v.clone());
                }
            }
            CypherPatternElement::Relationship(r) => {
                if let Some(v) = &r.variable {
                    vars.insert(v.clone());
                }
            }
        }
    }
    vars
}

/// Are the per-pattern variable sets transitively connected? Uses a
/// simple union-find over variable names: two sets overlap iff they
/// share a variable. Returns true when every component is reachable
/// from every other. Empty-variable components (`MATCH (), ()` — no
/// bindings) count as disconnected and trip the check.
fn components_all_connected(components: &[std::collections::HashSet<String>]) -> bool {
    if components.len() < 2 {
        return true;
    }
    // Build a graph where each component is a node and shared
    // variables are edges. A single DFS from component 0 must reach
    // every other component.
    let n = components.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in (i + 1)..n {
            if components[i].intersection(&components[j]).next().is_some() {
                adj[i].push(j);
                adj[j].push(i);
            }
        }
    }
    let mut visited = vec![false; n];
    let mut stack = vec![0];
    visited[0] = true;
    while let Some(node) = stack.pop() {
        for &next in &adj[node] {
            if !visited[next] {
                visited[next] = true;
                stack.push(next);
            }
        }
    }
    visited.iter().all(|&v| v)
}

/// Cross-clause disconnection: within a single WITH-segment of a
/// statement, MATCH clauses whose aggregate variable sets share
/// nothing indicate an accidental cartesian join. A WITH clause is
/// the author's "I'm carrying shared state" signal — it splits the
/// statement into segments, and each segment is checked independently.
///
/// - `MATCH (a) MATCH (b) RETURN a,b` — one segment, two disjoint
///   groups → warning.
/// - `MATCH (a) WITH a MATCH (b) RETURN a,b` — two segments, one
///   group each → silent.
/// - `MATCH (a), (b) WITH a,b MATCH (c) RETURN ...` — segment 1
///   already flags the within-clause cartesian (via the other
///   check); segment 2 is a single group, silent.
fn flag_cross_clause_disconnection(
    statement: &crate::cypher::ast::CypherStatement,
) -> Vec<ValidationIssue> {
    use crate::cypher::ast::ClauseKind;

    let mut segments: Vec<Vec<std::collections::HashSet<String>>> = vec![Vec::new()];
    for clause in &statement.clauses {
        match clause.kind {
            ClauseKind::With => {
                // Open a fresh segment — the WITH pins the author's
                // intent "anything after this runs in a new scope
                // whose connectedness I vouch for".
                segments.push(Vec::new());
            }
            ClauseKind::Match | ClauseKind::OptionalMatch => {
                let mut vars = std::collections::HashSet::new();
                for p in &clause.patterns {
                    vars.extend(pattern_variables(p));
                }
                if !vars.is_empty()
                    && let Some(current) = segments.last_mut()
                {
                    current.push(vars);
                }
            }
            _ => {}
        }
    }

    for segment in &segments {
        if segment.len() < 2 {
            continue;
        }
        if !components_all_connected(segment) {
            return vec![ValidationIssue::warning(
                "complexity",
                diag("runtime.cypher.complexity.cross_clause_disconnection").message(
                    "multiple MATCH clauses do not share a variable — execution crosses a cartesian boundary. \
                     If intentional, add a `WITH` clause to carry shared state; if not, connect them by a \
                     common variable.",
                ),
            )];
        }
    }
    Vec::new()
}

fn is_unbounded_var_length(var_length: &Option<(Option<u32>, Option<u32>)>) -> bool {
    match var_length {
        // No var-length marker — single hop.
        None => false,
        // `*` with no bounds at all, or `*min..` / `*..max` where the
        // missing side leaves one endpoint open. A pinned `*1..5` is
        // Some((Some, Some)) and fine.
        Some((_lo, hi)) => hi.is_none(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::GraphLabel;
    use ox_core::PropertyKey;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::ir::{Cardinality, EdgeTypeDef, NodeTypeDef, PropertyDef};
    use ox_core::types::PropertyType;

    use crate::cypher::rewrite::{CypherRewriterPipeline, RewriteContext, WorkspaceScopeRewriter};

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label literal must be valid")
    }

    fn pk(s: &'static str) -> PropertyKey {
        PropertyKey::new(s).expect("test property name literal must be valid")
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
                            name: pk("name"),
                            property_type: PropertyType::String,
                            nullable: false,
                            default_value: None,
                            description: LocalizedText::default(),
                            classification: None,
                            ..Default::default()
                        },
                        PropertyDef {
                            id: "p2".into(),
                            name: pk("age"),
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
                        name: pk("title"),
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
        assert!(r.errors().any(|e| e.message.message.contains("DELETE")));
    }

    #[test]
    fn safety_blocks_unrestricted_detach_delete() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n) DETACH DELETE n");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("DETACH DELETE")));
    }

    #[test]
    fn safety_blocks_unrestricted_remove() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n:Person) REMOVE n.age");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("REMOVE")));
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
    fn safety_blocks_user_write_to_workspace_property() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(
            &p,
            "MATCH (n:Person) WHERE n.id = $id SET n._workspace_id = 'other'",
        );
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("_workspace_id")));
    }

    #[test]
    fn safety_blocks_user_write_to_tombstone_property() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(
            &p,
            "MATCH (n:Person) WHERE n.id = $id SET n._deleted_at = NULL",
        );
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("_deleted_at")));
    }

    #[test]
    fn safety_blocks_create_with_reserved_property() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "CREATE (n:Person {_workspace_id: 'spoof'})");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("_workspace_id")));
    }

    #[test]
    fn safety_allows_user_write_to_non_reserved_property() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(
            &p,
            "MATCH (n:Person) WHERE n.id = $id SET n.name = 'Alice'",
        );
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    #[test]
    fn safety_blocks_drop_keyword() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "DROP CONSTRAINT person_name IF EXISTS");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("DROP")));
    }

    #[test]
    fn safety_blocks_drop_inside_statement() {
        let p = CypherValidatorPipeline::new().with(SafetyValidator::new());
        let r = run(&p, "MATCH (n) WITH n CALL { DROP INDEX foo } RETURN n");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("DROP")));
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
                .filter(|e| e.message.message.contains("DETACH DELETE"))
                .count()
                >= 1
        );
        assert!(r.errors().filter(|e| e.message.message.contains("REMOVE")).count() >= 1);
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
        assert!(r.errors().any(|e| e.message.message.contains("Userr")));
    }

    #[test]
    fn ontology_flags_unknown_relationship_type() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person)-[:WORKS_FOR]->(c:Company) RETURN p");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("WORKS_FOR")));
    }

    #[test]
    fn ontology_flags_unknown_property_key_on_known_label() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person {emial: 'x'}) RETURN p");
        assert!(r.has_errors());
        assert!(r.errors().any(|e| e.message.message.contains("emial")));
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
    fn ontology_flags_unknown_set_property() {
        // LLM emits `SET p.emial = 'x'` (typo) — the schema-less
        // graph would silently write a brand-new property without the
        // validator catching it. The variable→labels map binds `p` to
        // Person via the upstream MATCH, so the validator can resolve.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(
            &p,
            "MATCH (p:Person) SET p.emial = 'x@example.com' RETURN p",
        );
        assert!(r.has_errors(), "expected typo to be flagged");
        assert!(
            r.errors().any(|e| e.message.message.contains("emial")
                && e.message.message.contains("SET")),
            "diagnostic must name the SET clause and the typo: {:?}",
            r.issues
        );
    }

    #[test]
    fn ontology_accepts_known_set_property() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person) SET p.name = 'Alice' RETURN p");
        assert!(!r.has_errors(), "known property must pass: {:?}", r.issues);
    }

    #[test]
    fn ontology_flags_unknown_remove_property() {
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "MATCH (p:Person) REMOVE p.emial RETURN p");
        assert!(r.has_errors());
        assert!(
            r.errors().any(|e| e.message.message.contains("emial")
                && e.message.message.contains("REMOVE")),
            "diagnostic must name the REMOVE clause: {:?}",
            r.issues
        );
    }

    #[test]
    fn ontology_skips_unbound_set_variable() {
        // `WITH 1 AS x SET x.foo = 'y'` — the variable `x` is not
        // bound to a labelled node, so the validator cannot resolve
        // ontologically. Skip rather than false-flag — pattern-side
        // unknown labels are caught by the existing pattern walk.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(&p, "WITH 1 AS x SET x.foo = 'y' RETURN x");
        assert!(
            !r.errors().any(|e| e.message.message.contains("SET")),
            "unbound SET variable must not raise an ontology error: {:?}",
            r.issues
        );
    }

    #[test]
    fn ontology_skips_system_properties_in_set_clause() {
        // `_workspace_id` is rewriter-managed; even SET targeting it
        // (e.g. the isolation rewriter's own injection during
        // round-trip tests) must not be flagged as unknown.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(
            &p,
            "MATCH (p:Person) SET p._workspace_id = $ws RETURN p",
        );
        assert!(!r.has_errors(), "system property must pass: {:?}", r.issues);
    }

    #[test]
    fn ontology_dedupes_repeated_set_property_typos() {
        // `SET p.emial = 'x', p.emial = 'y'` — same typo on the same
        // variable in two assignments. One diagnostic, not two.
        let p =
            CypherValidatorPipeline::new().with(OntologyValidator::new(person_company_ontology()));
        let r = run(
            &p,
            "MATCH (p:Person) SET p.emial = 'x', p.emial = 'y' RETURN p",
        );
        let count = r
            .errors()
            .filter(|e| e.message.message.contains("emial"))
            .count();
        assert_eq!(count, 1, "expected single dedup'd diagnostic: {:?}", r.issues);
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
        let count = r.errors().filter(|e| e.message.message.contains("Userr")).count();
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
            ValidationIssue::error("a", diag("test.a").message("e1")),
            ValidationIssue::warning("b", diag("test.b").message("w1")),
            ValidationIssue::info("c", diag("test.c").message("i1")),
            ValidationIssue::error("d", diag("test.d").message("e2")),
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
        let issue = ValidationIssue::error("x", diag("test.x").message("msg"))
            .with_span(span);
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

    // =====================================================================
    // SemanticGuardValidator
    // =====================================================================

    /// Bare DELETE with no WHERE at all — SafetyValidator's job, not
    /// ours. SemanticGuard stays silent so the error set doesn't
    /// duplicate.
    #[test]
    fn semantic_guard_silent_when_no_where() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) DELETE n");
        assert!(!r.has_errors(), "no WHERE is SafetyValidator's concern: {:?}", r.issues);
    }

    /// Destructive write with a legitimate predicate — silent.
    #[test]
    fn semantic_guard_accepts_real_predicate() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n:Person) WHERE n.id = $id DELETE n");
        assert!(!r.has_errors(), "real predicate must pass: {:?}", r.issues);
    }

    /// `WHERE true` is the canonical tautology LLMs append when the
    /// SafetyValidator rejects a naked DELETE. Must now fail.
    #[test]
    fn semantic_guard_rejects_where_true_delete() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE true DELETE n");
        assert!(r.has_errors(), "WHERE true + DELETE must error: {:?}", r.issues);
        assert!(
            r.errors()
                .any(|e| e.validator_name == "semantic-guard"
                    && e.message.message.contains("tautological"))
        );
    }

    /// Case-insensitive `where TRUE` — Cypher keywords are
    /// case-insensitive and the tautology check must be too.
    #[test]
    fn semantic_guard_case_insensitive() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) where TRUE detach delete n");
        assert!(r.has_errors(), "case insensitive: {:?}", r.issues);
    }

    /// `WHERE NOT false` — the other keyword-level tautology.
    #[test]
    fn semantic_guard_rejects_not_false() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE NOT false REMOVE n:Label");
        assert!(r.has_errors(), "WHERE NOT false must error: {:?}", r.issues);
    }

    /// `WHERE 1 = 1` — literal-equals-literal tautology.
    #[test]
    fn semantic_guard_rejects_int_literal_self_equality() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE 1 = 1 DELETE n");
        assert!(r.has_errors(), "1 = 1 must error: {:?}", r.issues);
    }

    /// `WHERE 'x' = 'x'` — string literal variant.
    #[test]
    fn semantic_guard_rejects_string_literal_self_equality() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE 'x' = 'x' DELETE n");
        assert!(r.has_errors(), "'x' = 'x' must error: {:?}", r.issues);
    }

    /// `WHERE 1 = 2` — literal inequality is NOT a tautology (it's a
    /// contradiction, but either way it constrains). Must pass.
    #[test]
    fn semantic_guard_accepts_literal_inequality() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE 1 = 2 DELETE n");
        assert!(!r.has_errors(), "inequality constrains (to nothing) but is not a tautology: {:?}", r.issues);
    }

    /// `WHERE n.id = $id` — normal property equality must pass. The
    /// left-hand side is a property access (multi-token `n.id`), so
    /// the helper's "bare literal" shape rejects the tautology match.
    #[test]
    fn semantic_guard_accepts_property_equality() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n:Person) WHERE n.id = 42 DELETE n");
        assert!(!r.has_errors(), "property equality must pass: {:?}", r.issues);
    }

    /// Destructive op with two WHEREs — one tautological, one real.
    /// "At least one WHERE is real" passes; the policy is "every
    /// WHERE is tautological" before we flag.
    #[test]
    fn semantic_guard_passes_when_any_where_is_real() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(
            &p,
            "MATCH (n:Person) WHERE n.id = $id MATCH (m) WHERE true DELETE n, m",
        );
        assert!(!r.has_errors(), "one real WHERE is enough: {:?}", r.issues);
    }

    /// Non-destructive WHERE true is fine — the concern is only
    /// when paired with DELETE/REMOVE/DETACH DELETE.
    #[test]
    fn semantic_guard_silent_on_read_only_tautology() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE true RETURN n");
        assert!(!r.has_errors(), "read-only tautology is allowed: {:?}", r.issues);
    }

    /// `WHERE n = n` — bare variable self-reference. Structurally
    /// equivalent to `WHERE true` against any matched node, so the
    /// destructive gate should trip.
    #[test]
    fn semantic_guard_rejects_bare_variable_self_reference() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE n = n DELETE n");
        assert!(r.has_errors(), "n = n must error: {:?}", r.issues);
    }

    /// `WHERE n.id = n.id` — property self-reference. Common LLM
    /// "make the validator happy" output; the new rule catches it.
    #[test]
    fn semantic_guard_rejects_property_self_reference() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n:Person) WHERE n.id = n.id DELETE n");
        assert!(r.has_errors(), "n.id = n.id must error: {:?}", r.issues);
    }

    /// Different variables on each side — not a self-reference.
    /// `a.id = b.id` is a legitimate join predicate and must pass.
    #[test]
    fn semantic_guard_accepts_cross_variable_equality() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(
            &p,
            "MATCH (a)-[:KNOWS]->(b) WHERE a.id = b.id DELETE a",
        );
        assert!(!r.has_errors(), "cross-variable equality must pass: {:?}", r.issues);
    }

    /// Same variable, different property keys — still constrains. Must pass.
    #[test]
    fn semantic_guard_accepts_same_var_different_keys() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE n.id = n.name DELETE n");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    /// Three-token operand shape but with an `=`-like operator that
    /// isn't `=` (e.g. `!=`) — the rule must key on the exact `=`
    /// token so a negated self-reference doesn't trigger.
    #[test]
    fn semantic_guard_accepts_self_inequality() {
        let p = CypherValidatorPipeline::new().with(SemanticGuardValidator::new());
        let r = run(&p, "MATCH (n) WHERE n != n DELETE n");
        assert!(!r.has_errors(), "n != n is a contradiction, not a tautology: {:?}", r.issues);
    }

    // =====================================================================
    // ComplexityValidator
    // =====================================================================

    /// Single-hop MATCH with a connected pattern — no complaints.
    #[test]
    fn complexity_accepts_simple_connected_match() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(&p, "MATCH (a:Person)-[:WORKS_AT]->(b:Company) RETURN a, b");
        assert!(!r.has_errors(), "{:?}", r.issues);
        assert!(r.warnings().count() == 0);
    }

    /// Unbounded `*` without an upper bound is the canonical footgun —
    /// emits Error in the default (strict) mode.
    #[test]
    fn complexity_rejects_unbounded_variable_length() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(&p, "MATCH (a)-[*]->(b) RETURN a, b");
        assert!(r.has_errors(), "unbounded * must error: {:?}", r.issues);
        assert!(r.errors().any(|e| e.message.message.contains("variable-length")));
    }

    /// `*1..` (no upper) also triggers — lack of upper bound is the
    /// dangerous half, regardless of whether min is pinned.
    #[test]
    fn complexity_rejects_missing_upper_bound() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(&p, "MATCH (a)-[*1..]->(b) RETURN a, b");
        assert!(r.has_errors(), "missing upper bound must error: {:?}", r.issues);
    }

    /// `*1..5` is a pinned range — allowed.
    #[test]
    fn complexity_accepts_bounded_variable_length() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(&p, "MATCH (a)-[*1..5]->(b) RETURN a, b");
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    /// Permissive mode downgrades unbounded var-length to a Warning so
    /// power users running ad-hoc explores aren't blocked.
    #[test]
    fn complexity_permissive_downgrades_unbounded_to_warning() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::permissive());
        let r = run(&p, "MATCH (a)-[*]->(b) RETURN a, b");
        assert!(!r.has_errors(), "permissive must not error: {:?}", r.issues);
        assert!(r.warnings().any(|w| w.message.message.contains("variable-length")));
    }

    /// Two patterns in the same MATCH with no shared variable — that's
    /// a cartesian product. Always an Error regardless of mode.
    #[test]
    fn complexity_rejects_within_clause_cartesian() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(&p, "MATCH (a:Person), (b:Company) RETURN a, b");
        assert!(r.has_errors(), "disconnected comma-patterns must error: {:?}", r.issues);
        assert!(r.errors().any(|e| e.message.message.contains("cartesian")));
    }

    /// Same shape but now the two patterns share `a` — no cartesian.
    #[test]
    fn complexity_accepts_within_clause_patterns_sharing_variable() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(
            &p,
            "MATCH (a:Person)-[:WORKS_AT]->(b:Company), (a)-[:KNOWS]->(c:Person) RETURN a",
        );
        assert!(!r.has_errors(), "{:?}", r.issues);
    }

    /// Cross-clause disconnection (no WITH boundary) — a Warning,
    /// not an Error. The author may have meant it; the warning gives
    /// them a chance to notice in tooling without blocking the query.
    #[test]
    fn complexity_warns_on_cross_clause_disconnection() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(
            &p,
            "MATCH (a:Person) MATCH (b:Company) RETURN a, b",
        );
        assert!(!r.has_errors(), "cross-clause disconnect is a warning: {:?}", r.issues);
        assert!(
            r.warnings().any(|w| w.message.message.contains("cartesian")),
            "expected cross-clause warning: {:?}",
            r.issues
        );
    }

    /// A WITH between the two MATCHes signals author intent — no warning.
    #[test]
    fn complexity_accepts_cross_clause_with_boundary() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(
            &p,
            "MATCH (a:Person) WITH a MATCH (b:Company) RETURN a, b",
        );
        assert!(!r.has_errors(), "{:?}", r.issues);
        assert!(
            r.warnings().count() == 0,
            "WITH between MATCHes must silence the warning: {:?}",
            r.issues
        );
    }

    /// The three failure modes compose: unbounded path inside a
    /// cartesian-product pair yields two issues, correctly attributed
    /// to the complexity validator.
    #[test]
    fn complexity_reports_multiple_issues_in_one_query() {
        let p = CypherValidatorPipeline::new().with(ComplexityValidator::new());
        let r = run(&p, "MATCH (a)-[*]->(x), (b:Company) RETURN a, b");
        let complexity: Vec<_> = r
            .issues
            .iter()
            .filter(|i| i.validator_name == "complexity")
            .collect();
        assert!(
            complexity.len() >= 2,
            "expected ≥2 complexity issues: {:?}",
            r.issues
        );
    }
}
